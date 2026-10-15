//! The deterministic Grubcraft simulation.
//!
//! Rules this crate lives by (see `docs/10-architecture.md`):
//! - No floating point. Use `gc_fixed`. Enforced by `tests/no_floats.rs`.
//! - No OS access: no clock, files, threads or randomness from outside. `no_std`.
//! - No heap allocation inside [`World::tick`]. All storage is reserved when the
//!   world is created. Enforced by `tests/no_alloc_in_tick.rs`.
//! - Iterate in index order; never iterate a hash map.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

pub mod combat;
pub mod formation;
mod hash;
pub mod morale;
pub mod nav;
mod regiments;
pub mod scenario;
pub mod stances;
pub mod terrain;

pub use hash::StateHasher;
pub use morale::MoraleState;
pub use regiments::{Order, RegimentHandle, RegimentView, Regiments};
pub use stances::Stances;
pub use terrain::{Ground, Terrain};

use combat::Combat;
use formation::MAX_MODELS;
use gc_fixed::{Angle, Fx, FxVec2, Pcg32};
use nav::Nav;

/// Simulation ticks per second. One lockstep turn equals one tick.
pub const TICKS_PER_SECOND: u32 = 20;

/// Player slot index (0..=3 for four players; more reserved for neutrals).
pub type PlayerId = u8;

/// Fixed capacities reserved when a match starts.
#[derive(Clone, Copy, Debug)]
pub struct Capacities {
    pub regiments: u32,
}

impl Default for Capacities {
    fn default() -> Self {
        // Warfield sizing from docs/10-architecture.md.
        Capacities { regiments: 1536 }
    }
}

/// Index of a regiment type in the match's unit table.
pub type UnitId = u16;

/// How heavy a regiment's soldiers are when two lines crush together
/// (docs/05-combat.md): swarms are light, cavalry and monsters heavy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Mass {
    Light = 0,
    #[default]
    Medium = 1,
    Heavy = 2,
}

impl Mass {
    pub const ALL: [Mass; 3] = [Mass::Light, Mass::Medium, Mass::Heavy];

    pub fn from_u8(v: u8) -> Option<Mass> {
        Mass::ALL.get(v as usize).copied()
    }

    pub fn name(self) -> &'static str {
        match self {
            Mass::Light => "light",
            Mass::Medium => "medium",
            Mass::Heavy => "heavy",
        }
    }
}

/// How closely a regiment's soldiers stand (docs/05-combat.md, "Weapon
/// room"). The order changes the block's size, and with it how many arrows
/// find a mark, and how well each kind of weapon fights.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Formation {
    /// The unit's own spacing. Nothing changes.
    #[default]
    Line = 0,
    /// Closer ranks: +[`TIGHT_DEFENCE`] melee defence, but a denser target
    /// for arrows, and swinging weapons are cramped.
    Tight = 1,
    /// Spread out: fewer arrows hit, but gaps cost [`LOOSE_DEFENCE`] melee
    /// defence (not for swinging weapons, which want the room), and spears
    /// and shields lose their mutual support.
    Loose = 2,
    /// Shields locked (shielded regiments only): as Tight from the front,
    /// where arrows mostly glance off too, but no help on the flanks; it
    /// marches at half pace and can't charge.
    ShieldWall = 3,
}

/// Melee defence a regiment gains in Tight order or a shield wall.
pub const TIGHT_DEFENCE: i32 = 10;
/// Melee defence a regiment loses in Loose order (unless its weapons
/// need room to swing).
pub const LOOSE_DEFENCE: i32 = 10;
/// Damage, percent, swinging weapons deal from Tight order or a shield wall.
pub const CRAMPED_SWING_PERCENT: u64 = 80;
/// Damage, percent, spears and shields deal in Loose order.
pub const LOOSE_SUPPORT_PERCENT: u64 = 85;
/// Damage, percent, arrows do to a shield wall from the front.
pub const SHIELD_WALL_ARROW_PERCENT: u64 = 30;
/// Marching pace of a shield wall, percent of the regiment's speed.
pub const SHIELD_WALL_PACE_PERCENT: i32 = 50;
/// How much the second rank of a spear or pike regiment adds, percent of
/// a front-rank soldier's blow.
pub const SECOND_RANK_PERCENT: u64 = 50;

impl Formation {
    pub const ALL: [Formation; 4] = [
        Formation::Line,
        Formation::Tight,
        Formation::Loose,
        Formation::ShieldWall,
    ];

    pub fn from_u8(v: u8) -> Option<Formation> {
        Formation::ALL.get(v as usize).copied()
    }

    pub fn name(self) -> &'static str {
        match self {
            Formation::Line => "Line",
            Formation::Tight => "Tight",
            Formation::Loose => "Loose",
            Formation::ShieldWall => "Shield wall",
        }
    }

    /// `spacing` (a unit's file or rank spacing) in this formation.
    pub fn space(self, spacing: Fx) -> Fx {
        let percent = match self {
            Formation::Line => 100,
            Formation::Tight => 80,
            Formation::Loose => 150,
            Formation::ShieldWall => 70,
        };
        Fx::from_raw((spacing.raw() as i64 * percent / 100).max(1) as i32)
    }

    /// Whether the soldiers stand closer than usual.
    pub fn close(self) -> bool {
        matches!(self, Formation::Tight | Formation::ShieldWall)
    }

    /// Marching pace, percent of the regiment's speed.
    pub fn pace_percent(self) -> i32 {
        match self {
            Formation::ShieldWall => SHIELD_WALL_PACE_PERCENT,
            _ => 100,
        }
    }

    /// Whether a regiment in this formation can charge.
    pub fn can_charge(self) -> bool {
        self != Formation::ShieldWall
    }

    /// Damage, percent, a volley does to a regiment in this formation,
    /// landing on its `front` or not.
    pub fn arrow_percent(self, front: bool) -> u64 {
        match self {
            Formation::ShieldWall if front => SHIELD_WALL_ARROW_PERCENT,
            _ => 100,
        }
    }
}

/// Weapon flags as bits ([`UnitStats::weapons`]), as the data, the network
/// and replays carry them.
pub const WEAPON_REACH: u8 = 1;
pub const WEAPON_SWING_ROOM: u8 = 2;
pub const WEAPON_SHIELD: u8 = 4;

/// What the simulation needs from a regiment type's definition. The game
/// builds these from the baked data tables (`gc_data`); tests build them by
/// hand so golden hashes don't move when balance numbers are tuned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitStats {
    /// Marching speed, tiles per second.
    pub speed: Fx,
    /// Turning speed, in [`Angle`] steps per second (65,536 is a full turn).
    pub turn_rate: u32,
    /// Soldiers at full strength, at most [`MAX_MODELS`].
    pub models: u16,
    pub hp_per_model: u16,
    /// Soldiers per rank when the regiment is raised.
    pub files: u16,
    /// Distance between neighbouring soldiers in a rank, in tiles.
    pub file_spacing: Fx,
    /// Distance between ranks, in tiles.
    pub rank_spacing: Fx,
    /// Melee skill and defence, 0 to 100. A blow lands with chance
    /// 50 + attack - defence percent, kept between 10 and 90.
    pub melee_attack: u8,
    pub melee_defence: u8,
    /// Hit points a landed melee blow takes.
    pub melee_damage: u16,
    /// Extra melee damage, percent, at the moment a charge hits home.
    pub charge_bonus: u8,
    /// Shooting range in tiles; zero for regiments that don't shoot.
    pub range: Fx,
    /// Hit points a landed arrow takes.
    pub missile_damage: u16,
    /// Ticks between volleys.
    pub reload_ticks: u16,
    /// Percent of arrows that hit a block of normal density.
    pub accuracy: u8,
    /// Morale when fresh, 1 to 100 (see [`morale`]). Zero means the regiment
    /// never routs (the Concord's undead).
    pub morale: u8,
    /// How heavy its soldiers are, for lines that push.
    pub mass: Mass,
    /// Spears and pikes: the second rank fights too, at
    /// [`SECOND_RANK_PERCENT`], and they strike at once when lines meet.
    pub reach: bool,
    /// Big swinging weapons: cramped in Tight order, at home in Loose.
    pub swing_room: bool,
    /// Shields: may form a shield wall.
    pub shield: bool,
}

impl UnitStats {
    /// The same stats forced into the ranges the simulation relies on. The
    /// data baker already rejects anything outside them; this keeps the sim
    /// safe whatever it is handed.
    pub fn sanitized(self) -> UnitStats {
        let models = self.models.clamp(1, MAX_MODELS);
        UnitStats {
            speed: self.speed.max(Fx::ZERO),
            turn_rate: self.turn_rate.max(TICKS_PER_SECOND),
            models,
            hp_per_model: self.hp_per_model.max(1),
            files: self.files.clamp(1, models),
            file_spacing: self.file_spacing.max(Fx::from_raw(1)),
            rank_spacing: self.rank_spacing.max(Fx::from_raw(1)),
            melee_attack: self.melee_attack.min(100),
            melee_defence: self.melee_defence.min(100),
            accuracy: self.accuracy.min(100),
            range: self.range.max(Fx::ZERO),
            morale: self.morale.min(100),
            ..self
        }
    }

    fn hash_into(&self, h: &mut StateHasher) {
        h.i32(self.speed.raw());
        h.u32(self.turn_rate);
        h.u16(self.models);
        h.u16(self.hp_per_model);
        h.u16(self.files);
        h.i32(self.file_spacing.raw());
        h.i32(self.rank_spacing.raw());
        h.u8(self.melee_attack);
        h.u8(self.melee_defence);
        h.u16(self.melee_damage);
        h.u8(self.charge_bonus);
        h.i32(self.range.raw());
        h.u16(self.missile_damage);
        h.u16(self.reload_ticks);
        h.u8(self.accuracy);
        h.u8(self.morale);
        // Only when set, so unit tables from before weapon room hash as
        // they did.
        if self.mass != Mass::Medium || self.weapons() != 0 {
            h.u8(self.mass as u8);
            h.u8(self.weapons());
        }
    }

    /// The weapon flags as `WEAPON_*` bits.
    pub fn weapons(&self) -> u8 {
        let bit = |on: bool, b: u8| if on { b } else { 0 };
        bit(self.reach, WEAPON_REACH)
            | bit(self.swing_room, WEAPON_SWING_ROOM)
            | bit(self.shield, WEAPON_SHIELD)
    }

    /// The same stats with the weapon flags `bits` (as [`Self::weapons`]
    /// gives them); `None` if a bit is unknown.
    pub fn with_weapons(self, bits: u8) -> Option<UnitStats> {
        (bits & !(WEAPON_REACH | WEAPON_SWING_ROOM | WEAPON_SHIELD) == 0).then_some(UnitStats {
            reach: bits & WEAPON_REACH != 0,
            swing_room: bits & WEAPON_SWING_ROOM != 0,
            shield: bits & WEAPON_SHIELD != 0,
            ..self
        })
    }

    /// File and rank spacing in `formation`.
    pub fn spacing(&self, formation: Formation) -> (Fx, Fx) {
        (
            formation.space(self.file_spacing),
            formation.space(self.rank_spacing),
        )
    }

    /// Size of a block of `models` soldiers in `files` files, standing in
    /// `formation`: (width, depth).
    pub fn block_size_in(&self, formation: Formation, files: u16, models: u16) -> (Fx, Fx) {
        let (fs, rs) = self.spacing(formation);
        formation::block_size(files, models, fs, rs)
    }

    /// Whether this regiment may stand in `formation`.
    pub fn can_form(&self, formation: Formation) -> bool {
        formation != Formation::ShieldWall || self.shield
    }

    /// Soldiers who strike at a foe ahead, in percent of a soldier: the
    /// front rank, and for spears and pikes the second rank at
    /// [`SECOND_RANK_PERCENT`].
    pub fn front_fighters_percent(&self, files: u16, models: u16) -> u64 {
        let front = files.min(models) as u64;
        let second = if self.reach {
            front.min(models as u64 - front)
        } else {
            0
        };
        front * 100 + second * SECOND_RANK_PERCENT
    }

    /// Percent of its usual melee damage it deals in `formation`.
    pub fn weapon_room_percent(&self, formation: Formation) -> u64 {
        if self.swing_room && formation.close() {
            CRAMPED_SWING_PERCENT
        } else if (self.reach || self.shield) && formation == Formation::Loose {
            LOOSE_SUPPORT_PERCENT
        } else {
            100
        }
    }

    /// Melee defence it gains (or, negative, loses) in `formation` against
    /// a blow on its `front` or not. A shield wall guards its front only.
    pub fn formation_defence(&self, formation: Formation, front: bool) -> i32 {
        match formation {
            Formation::ShieldWall if !front => 0,
            f if f.close() => TIGHT_DEFENCE,
            Formation::Loose if !self.swing_room => -LOOSE_DEFENCE,
            _ => 0,
        }
    }
}

/// Everything needed to start a match identically on every machine.
#[derive(Clone, Copy, Debug)]
pub struct MatchSetup<'a> {
    pub seed: u64,
    pub capacities: Capacities,
    pub units: &'a [UnitStats],
    /// Map edge length in tiles; fleeing regiments stay inside it. Zero means
    /// no edge (tests on an open plane).
    pub map_tiles: u32,
    /// Ground cost of each half-tile cell, row by row (see [`nav`]): 0 is
    /// blocked, 1 open, higher is slow going routes avoid. Empty means an
    /// open plane where regiments walk in straight lines.
    pub nav: &'a [u8],
    /// Each party's team (see [`Stances`]): allies share a number. Empty
    /// means every party fights every other.
    pub teams: &'a [u8],
    /// The map's heights and water line, for the terrain rules (see
    /// [`terrain`]). [`Ground::FLAT`] means flat, dry ground.
    pub ground: Ground<'a>,
}

/// A player's order. The only way anything outside the sim changes the sim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Debug/sandbox: create a full-strength regiment. Real production comes with M3.
    SpawnRegiment {
        unit: UnitId,
        position: FxVec2,
        facing: Angle,
    },
    /// March the block's centre to `target`. Group orders are expanded into
    /// one of these per regiment by [`formation::group_move`].
    MoveTo {
        regiment: RegimentHandle,
        target: FxVec2,
        /// Facing on arrival; `None` keeps the direction of travel.
        facing: Option<Angle>,
        /// New frontage in soldiers; 0 keeps the current one.
        files: u16,
        /// Tiles per second; zero means the regiment's own speed.
        pace: Fx,
    },
    /// Close with an enemy regiment and fight it (melee), or move into range
    /// and shoot it (ranged).
    Attack {
        regiment: RegimentHandle,
        target: RegimentHandle,
    },
    /// Sandbox: take a regiment off the field (any player's).
    RemoveRegiment { regiment: RegimentHandle },
    /// Sandbox: change a regiment type's stats mid-match. Regiments already
    /// on the field take the new speed, turn rate and fighting stats at once;
    /// their soldiers keep the hit points they have.
    TuneUnit { unit: UnitId, stats: UnitStats },
    /// Change how closely a regiment's soldiers stand (see [`Formation`]).
    /// Only shielded regiments can form a shield wall; regiments locked in
    /// melee or running can't re-form.
    SetFormation {
        regiment: RegimentHandle,
        formation: Formation,
    },
}

/// A command tagged with who issued it. Turn bundles carry these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerCommand {
    pub player: PlayerId,
    pub command: Command,
}

/// Per-tick timing-independent counters, useful for tests and the dev overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TickStats {
    pub commands_applied: u32,
    pub commands_rejected: u32,
}

pub struct World {
    tick: u32,
    rng: Pcg32,
    combat_rng: Pcg32,
    combat: Combat,
    units: Vec<UnitStats>,
    map_tiles: u32,
    nav: Nav,
    stances: Stances,
    terrain: Terrain,
    pub regiments: Regiments,
}

impl World {
    pub fn new(setup: &MatchSetup) -> World {
        World {
            tick: 0,
            rng: Pcg32::new(setup.seed, streams::WORLD),
            combat_rng: Pcg32::new(setup.seed, streams::COMBAT),
            combat: Combat::new(setup.capacities.regiments as usize),
            units: setup.units.iter().map(|u| u.sanitized()).collect(),
            map_tiles: setup.map_tiles,
            nav: Nav::new(
                setup.nav,
                setup.map_tiles,
                setup.capacities.regiments as usize,
            ),
            stances: Stances::from_teams(setup.teams),
            terrain: Terrain::new(&setup.ground, setup.map_tiles),
            regiments: Regiments::with_capacity(setup.capacities.regiments),
        }
    }

    /// The match's regiment types, indexed by [`UnitId`].
    pub fn units(&self) -> &[UnitStats] {
        &self.units
    }

    /// Number of ticks simulated so far.
    pub fn tick_count(&self) -> u32 {
        self.tick
    }

    /// Advance the world by one tick, applying this turn's commands.
    ///
    /// `commands` must already be in canonical order (the host's turn bundle
    /// order). The same world plus the same commands always yields the same
    /// world, bit for bit, on every platform.
    pub fn tick(&mut self, commands: &[PlayerCommand]) -> TickStats {
        let mut stats = TickStats::default();
        for pc in commands {
            if self.apply(pc) {
                stats.commands_applied += 1;
            } else {
                stats.commands_rejected += 1;
            }
        }
        self.combat.before_move(
            &mut self.regiments,
            &self.units,
            &self.stances,
            &self.terrain,
        );
        self.nav.plan(&mut self.regiments);
        self.combat.mark_holders(&self.regiments);
        self.regiments.step_movement(
            &mut self.nav,
            &self.terrain,
            &self.combat,
            &self.units,
            &self.stances,
        );
        self.combat.after_move(
            &mut self.regiments,
            &self.units,
            &self.stances,
            &self.terrain,
            self.tick,
            &mut self.combat_rng,
        );
        morale::update(
            &mut self.regiments,
            &self.combat,
            &self.units,
            &self.stances,
            self.tick,
            self.map_tiles,
        );
        self.tick += 1;
        stats
    }

    fn apply(&mut self, pc: &PlayerCommand) -> bool {
        match pc.command {
            Command::SpawnRegiment {
                unit,
                position,
                facing,
            } => {
                let Some(stats) = self.units.get(unit as usize) else {
                    return false;
                };
                self.regiments
                    .spawn(pc.player, unit, position, facing, stats)
                    .is_some()
            }
            Command::MoveTo {
                regiment,
                target,
                facing,
                files,
                pace,
            } => {
                // Players may only order their own regiments.
                match self.regiments.owner(regiment) {
                    Some(owner) if owner == pc.player => self.regiments.order(
                        regiment,
                        Order {
                            target,
                            facing,
                            files,
                            pace,
                        },
                    ),
                    _ => false,
                }
            }
            Command::Attack { regiment, target } => {
                match (self.regiments.owner(regiment), self.regiments.owner(target)) {
                    (Some(owner), Some(enemy))
                        if owner == pc.player && self.stances.hostile(owner, enemy) =>
                    {
                        self.regiments.attack(regiment, target)
                    }
                    _ => false,
                }
            }
            Command::RemoveRegiment { regiment } => self.regiments.despawn(regiment),
            Command::TuneUnit { unit, stats } => {
                let Some(slot) = self.units.get_mut(unit as usize) else {
                    return false;
                };
                *slot = stats.sanitized();
                self.regiments.retune(unit, slot);
                true
            }
            Command::SetFormation {
                regiment,
                formation,
            } => match self.regiments.owner(regiment) {
                Some(owner) if owner == pc.player => {
                    self.regiments
                        .set_formation(regiment, formation, &self.units)
                }
                _ => false,
            },
        }
    }

    /// Who fights whom.
    pub fn stances(&self) -> &Stances {
        &self.stances
    }

    /// Whether `a`'s regiments fight `b`'s (see [`Stances::hostile`]).
    pub fn hostile(&self, a: PlayerId, b: PlayerId) -> bool {
        self.stances.hostile(a, b)
    }

    /// The ground's heights and water, and what they do in a fight.
    pub fn terrain(&self) -> &Terrain {
        &self.terrain
    }

    /// The walkable ground and the route searches in progress.
    pub fn nav(&self) -> &Nav {
        &self.nav
    }

    /// Where a regiment is still headed: the waypoints ahead of it, ending
    /// at its destination. Empty when it is standing, or waiting for a route.
    pub fn route(&self, h: RegimentHandle) -> &[FxVec2] {
        match self.regiments.resolve(h) {
            Some(i) => self.nav.route_ahead(i, self.regiments.target[i]),
            None => &[],
        }
    }

    /// Volleys in flight, for drawing arrows.
    pub fn volleys(&self) -> impl Iterator<Item = combat::VolleyView> + '_ {
        self.combat.volleys()
    }

    /// 64-bit hash of the complete simulation state, for desync detection.
    pub fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.u32(self.tick);
        let (state, inc) = self.rng.raw_state();
        h.u64(state);
        h.u64(inc);
        let (state, inc) = self.combat_rng.raw_state();
        h.u64(state);
        h.u64(inc);
        h.u32(self.units.len() as u32);
        for u in &self.units {
            u.hash_into(&mut h);
        }
        self.regiments.hash_into(&mut h);
        self.combat.hash_into(&mut h);
        self.nav.hash_into(&mut h);
        h.finish()
    }

    /// The match's deterministic random stream (combat, events, ...).
    pub fn rng(&mut self) -> &mut Pcg32 {
        &mut self.rng
    }
}

/// Random substream ids, so adding draws in one system never shifts another.
pub mod streams {
    pub const WORLD: u64 = 1;
    pub const COMBAT: u64 = 2;
    pub const EVENTS: u64 = 3;
}
