//! The deterministic Grubcraft simulation.
//!
//! Rules this crate lives by (see `docs/10-architecture.md`):
//! - No floating point. Use `gc_fixed`. Enforced by `tests/no_floats.rs`.
//! - No OS access: no clock, files, threads or randomness from outside. `no_std`.
//! - No heap allocation inside [`World::tick`]. All storage is reserved when the
//!   world is created. Enforced by `tests/no_alloc_in_tick.rs`.
//! - Iterate in index order; never iterate a hash map.

#![no_std]
// Float maths through inferred types or literals, which the token check in
// `tests/no_floats.rs` can miss. Test modules may use floats as a reference.
#![cfg_attr(not(test), deny(clippy::float_arithmetic))]

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;

pub mod champions;
pub mod combat;
pub mod conquest;
pub mod deploy;
pub mod economy;
pub mod events;
pub mod fork;
pub mod formation;
mod hash;
pub mod larvae;
pub mod morale;
pub mod nav;
pub mod production;
mod regiments;
pub mod scenario;
pub mod stances;
mod state;
pub mod structures;
pub mod terrain;
pub mod verdict;
pub mod vision;

pub use champions::Challenge;
use conquest::Conquest;
pub use conquest::{ConquestSetup, ConquestUnit};
pub use economy::{Bank, Economy, NodeSetup, NodeView, Resource};
pub use fork::Standing;
pub use hash::StateHasher;
pub use morale::MoraleState;
pub use production::Production;
pub use regiments::{Order, RegimentHandle, RegimentView, Regiments};
pub use stances::Stances;
use structures::Work;
pub use structures::{
    BuildingId, BuildingStats, Refusal, StructureHandle, StructureView, Structures,
};
pub use terrain::{Cover, Ground, Terrain};
pub use verdict::{
    EXILE_TICKS, HEART_HOLD_TICKS, HEART_REACH, HEART_UNLOCK_TICKS, HOLD_TICKS, HeartView, Verdict,
    Victories, Win,
};
pub use vision::Ambush;

use champions::Champions;
use combat::Combat;
use deploy::{Deployment, Zone};
use events::Events;
use formation::MAX_MODELS;
use gc_fixed::{Angle, Fx, FxVec2, Pcg32};
use nav::Nav;
use vision::Vision;

/// The parts of the simulation's state, in the order [`World::state_hash`]
/// hashes them and [`World::system_hashes`] lists them.
pub const SYSTEMS: [&str; 13] = [
    "clock",
    "random streams",
    "unit stats",
    "regiments",
    "combat",
    "pathfinding",
    "vision",
    "deployment",
    "verdict",
    "champions",
    "structures",
    "economy",
    "production",
];

/// The simulation's rules version. Two builds with the same version play
/// the same commands out the same way, whatever else differs, so the
/// lobby lets them play together and a replay made on one plays as
/// recorded on the other. Bump it in any change that makes the golden
/// replays need blessing (`gc_replay/tests/goldens.rs` checks).
pub const SIM_VERSION: u32 = 14;

/// Simulation ticks per second. One lockstep turn equals one tick.
pub const TICKS_PER_SECOND: u32 = 20;

/// Player slot index (0..=5 for the six lobby slots; more reserved for neutrals).
pub type PlayerId = u8;

/// Most parties (players, and later other factions) a match may have,
/// numbered from 0. Combat and sight keep one slot each (8-bit masks);
/// widening them is engine hook 8 (docs/18-ai.md). Regiments for a party
/// past this are refused.
pub const MAX_PARTIES: usize = 8;

/// Fixed capacities reserved when a match starts.
#[derive(Clone, Copy, Debug)]
pub struct Capacities {
    pub regiments: u32,
}

impl Capacities {
    /// The most regiments a match may reserve room for: far past any
    /// battle, and what anything reading a match from outside (a replay
    /// file) refuses beyond.
    pub const MAX_REGIMENTS: u32 = 1 << 16;
}

/// The largest map a match may be on, in tiles a side: far past any battle
/// map, small enough that its grids' sizes can't overflow.
pub const MAX_MAP_TILES: u32 = 4096;

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
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

    /// How much a blow weighs when lines push (docs/05-combat.md, "Lines
    /// that push"): monsters and cavalry shove hardest, swarms least.
    pub fn heft(self) -> u64 {
        match self {
            Mass::Light => 1,
            Mass::Medium => 2,
            Mass::Heavy => 4,
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
        Fx::from_raw((i64::from(spacing.raw()) * percent / 100).max(1) as i32)
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
    /// The same stats forced into the ranges the simulation relies on, so
    /// it is safe whatever it is handed. The unit data's own checks
    /// (`gc_data`) are mostly stricter but not the same: a turn rate under
    /// one step a tick passes them and is raised to one step a tick here.
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
        let front = u64::from(files.min(models));
        let second = if self.reach {
            front.min(u64::from(models) - front)
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

    /// Melee defence in `formation` against a blow on its `front` or not:
    /// its own, plus what close order gives or gaps cost. A shield wall
    /// guards its front only.
    pub fn melee_defence_in(&self, formation: Formation, front: bool) -> i32 {
        i32::from(self.melee_defence)
            + match formation {
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
    /// The map's heights, water line and cover, for the terrain rules (see
    /// [`terrain`]) and hiding (see [`vision`]). [`Ground::FLAT`] means
    /// flat, dry, open ground.
    pub ground: Ground<'a>,
    /// Whether champions may duel between the lines before battle (see
    /// [`champions`]): the lobby's option, for battles with a computer
    /// player.
    pub champions: bool,
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
    /// one of these per regiment by [`formation::group_move`]. While the
    /// armies deploy, the regiment is put straight down there instead, if
    /// that is inside its owner's zone (see [`deploy`]).
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
    /// and shoot it (ranged). The target must be one the player's side can
    /// see (see [`vision`]).
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
    /// Pull a regiment out of its fight in good order: it backs away
    /// [`combat::FALL_BACK_TILES`] straight back from its foe (or from where
    /// it faces, if it isn't fighting) at half pace, still facing the
    /// enemy. Its foes get one parting blow and don't follow; an enemy that
    /// reaches its flank or rear catches it (docs/05-combat.md).
    FallBack { regiment: RegimentHandle },
    /// Set spears or pikes against a charge, or lift them (`on` false): a
    /// braced regiment stands, and a charge into its front runs onto the
    /// points (docs/05-combat.md). Only regiments with long weapons, out of
    /// melee, can brace; any move or attack order lifts it.
    Brace { regiment: RegimentHandle, on: bool },
    /// Field Battle: give the issuing player a deployment zone. The first
    /// one starts the deployment phase (see [`deploy`]); more are accepted
    /// until the battle begins, one per player.
    DeployZone { center: FxVec2, radius: Fx },
    /// During deployment: you are done placing. The battle starts when
    /// every player with a zone is, or when time runs out.
    Ready,
    /// Conquest setup and sandbox: put up a finished building of the
    /// issuing player's with its north-west tile at `tile` (see
    /// [`structures`]). Refused where it doesn't fit.
    SpawnStructure {
        building: BuildingId,
        tile: (i16, i16),
    },
    /// Close with an enemy building and knock it down. In fog of war, only
    /// one the issuer's side has seen.
    AttackStructure {
        regiment: RegimentHandle,
        structure: StructureHandle,
    },
    /// Conquest: a worker regiment gathers at node `node` (a Grub seep or
    /// Marrow vein, see [`economy`]) and carries each load home, until
    /// given another order.
    Gather { regiment: RegimentHandle, node: u16 },
    /// Conquest: a worker lays the foundation of a building of type
    /// `building` with its north-west tile at `tile`, pays for it, and
    /// builds it (see [`structures`]). Refused where it doesn't fit, before
    /// its owner's tier allows it, or when they can't pay.
    Build {
        regiment: RegimentHandle,
        building: BuildingId,
        tile: (i16, i16),
    },
    /// Conquest: a worker helps build one of its owner's buildings, or
    /// mends it once it is up.
    Construct {
        regiment: RegimentHandle,
        structure: StructureHandle,
    },
    /// Conquest: call off a building still going up, or its tier upgrade,
    /// for most of what it cost.
    CallOff { structure: StructureHandle },
    /// Conquest: a building queues a regiment of type `unit` (see
    /// [`production`]), paid for now.
    Train {
        structure: StructureHandle,
        unit: UnitId,
    },
    /// Conquest: take the last regiment off a building's queue, for all it
    /// cost.
    Untrain { structure: StructureHandle },
    /// Conquest: a main building starts its tier upgrade.
    Upgrade { structure: StructureHandle },
    /// Conquest: where what a building trains marches to.
    Rally {
        structure: StructureHandle,
        at: FxVec2,
    },
    /// Sandbox: take a building off the field (any player's).
    RemoveStructure { structure: StructureHandle },
    /// Hand the issuing player's army to an ally, who then gives its
    /// orders (moving, attacking, formations); `to` the issuer itself takes
    /// it back (see [`Stances::commander`]). Refused for an enemy.
    HandOver { to: PlayerId },
}

impl Command {
    /// Whether this is one of a player's own orders to their regiments (or
    /// Ready), rather than setup (zones, raising armies) or the sandbox's
    /// tools (removing, tuning). In a network match only the host may send
    /// the rest. No `_` arm: a new kind of command must be decided here.
    pub fn is_player_order(&self) -> bool {
        match self {
            Command::MoveTo { .. }
            | Command::Attack { .. }
            | Command::SetFormation { .. }
            | Command::Ready
            | Command::FallBack { .. }
            | Command::Brace { .. }
            | Command::AttackStructure { .. }
            | Command::Gather { .. }
            | Command::Build { .. }
            | Command::Construct { .. }
            | Command::CallOff { .. }
            | Command::Train { .. }
            | Command::Untrain { .. }
            | Command::Upgrade { .. }
            | Command::Rally { .. }
            | Command::HandOver { .. } => true,
            Command::SpawnRegiment { .. }
            | Command::RemoveRegiment { .. }
            | Command::TuneUnit { .. }
            | Command::DeployZone { .. }
            | Command::SpawnStructure { .. }
            | Command::RemoveStructure { .. } => false,
        }
    }
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
    /// Shared with this world's scratch copies (see [`fork`]).
    terrain: Arc<Terrain>,
    vision: Vision,
    deploy: Deployment,
    /// Only in matches that allow champions.
    champions: Option<Champions>,
    /// Who holds the field, and who won (see [`verdict`]).
    judge: verdict::Judge,
    /// A scratch copy (see [`World::scratch`]), the only kind that may be
    /// changed other than by commands: rehearsals set out their own field.
    scratch: bool,
    /// What happened in the last tick (see [`events`]); not part of the
    /// match's state.
    events: Events,
    /// Conquest only: the buildings and the economy (see
    /// [`World::conquest`]).
    conquest: Option<Conquest>,
    pub regiments: Regiments,
}

impl World {
    pub fn new(setup: &MatchSetup) -> World {
        assert!(setup.teams.len() <= MAX_PARTIES, "too many parties");
        let stances = Stances::from_teams(setup.teams);
        let terrain = Terrain::new(&setup.ground, setup.map_tiles);
        let vision = Vision::new(setup.capacities.regiments as usize, &terrain, &stances);
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
            stances,
            terrain: Arc::new(terrain),
            vision,
            deploy: Deployment::default(),
            champions: setup.champions.then(|| {
                Champions::new(
                    setup.capacities.regiments,
                    Pcg32::new(setup.seed, streams::EVENTS),
                )
            }),
            judge: verdict::Judge::default(),
            events: Events::new(setup.capacities.regiments as usize),
            scratch: false,
            conquest: None,
            regiments: Regiments::with_capacity(setup.capacities.regiments),
        }
    }

    /// Make this a Conquest match: buildings and the economy. Only
    /// on a match's own world with a map, before its first tick, once;
    /// returns whether it was done.
    pub fn conquest(&mut self, setup: &ConquestSetup) -> bool {
        if self.tick != 0 || self.scratch || self.conquest.is_some() || !self.nav.has_map() {
            return false;
        }
        self.conquest = Some(Conquest::new(setup, self.regiments.capacity()));
        self.events.reserve(setup.structures as usize);
        // A match played for a victory is won only that way; the sandbox's
        // buildings leave holding the field as it was, and everything in
        // sight.
        // The Heart counts only on a map that has it.
        let mut v = setup.victories;
        v.heart &= setup.heart.is_some();
        if v.any() {
            self.judge
                .conquest(verdict::Throne::new(v, setup.heart, setup.build_percent));
            // ...and in fog of war.
            if let Some(c) = &mut self.conquest {
                c.structures.fog();
            }
            (self.vision).fog(
                self.regiments.capacity(),
                setup.structures as usize,
                self.map_tiles,
            );
        }
        true
    }

    /// The buildings on the field (Conquest only).
    pub fn structures(&self) -> Option<&Structures> {
        Some(&self.conquest.as_ref()?.structures)
    }

    /// Grub, Marrow and the nodes (Conquest only).
    pub fn economy(&self) -> Option<&Economy> {
        Some(&self.conquest.as_ref()?.economy)
    }

    /// Player `p`'s store and supply (Conquest only).
    pub fn bank(&self, p: PlayerId) -> Option<Bank> {
        Some(self.conquest.as_ref()?.bank(&self.regiments, p))
    }

    /// Training queues and rally points (Conquest only).
    pub fn production(&self) -> Option<&Production> {
        Some(&self.conquest.as_ref()?.production)
    }

    /// How far the regiment at the front of building `h`'s queue has
    /// trained, in percent (Conquest only).
    pub fn training_percent(&self, h: StructureHandle) -> Option<u8> {
        self.queue_percent(h, 0)
    }

    /// How far regiment `k` of building `h`'s queue has trained, in
    /// percent (Conquest only). Behind the front only units that hatch
    /// train, all at once.
    pub fn queue_percent(&self, h: StructureHandle, k: usize) -> Option<u8> {
        let c = self.conquest.as_ref()?;
        c.production.percent(h, k, &c.units)
    }

    /// Larvae ready in building `h`, if it is one that holds them
    /// (Conquest only).
    pub fn larvae_in(&self, h: StructureHandle) -> Option<u8> {
        let structures = self.structures()?;
        structures.larvae_in(structures.resolve(h)?)
    }

    /// Whether `player` could have a worker lay a building of type
    /// `building` at `tile` now, and if not, why: the check
    /// [`Command::Build`] makes.
    pub fn can_lay(
        &self,
        player: PlayerId,
        building: BuildingId,
        tile: (i32, i32),
    ) -> Result<(), Refusal> {
        self.conquest.as_ref().ok_or(Refusal::NoBuildings)?.can_lay(
            player,
            building,
            tile,
            &self.nav,
            &self.regiments,
        )
    }

    /// Whether `player` could have a worker lay a building of type
    /// `building` somewhere now: [`World::can_lay`] without the ground.
    pub fn may_lay(&self, player: PlayerId, building: BuildingId) -> Result<(), Refusal> {
        self.conquest
            .as_ref()
            .ok_or(Refusal::NoBuildings)?
            .may_lay(player, building)
    }

    /// Whether `player`'s building `h` could queue a regiment of type `unit`
    /// now, and if not, why: the check [`Command::Train`] makes.
    pub fn can_train(
        &self,
        player: PlayerId,
        h: StructureHandle,
        unit: UnitId,
    ) -> Result<(), Refusal> {
        self.conquest
            .as_ref()
            .ok_or(Refusal::NoBuildings)?
            .can_train(player, h, unit)
    }

    /// What `player`'s building `h` would upgrade into now, or why it
    /// can't: the check [`Command::Upgrade`] makes.
    pub fn can_upgrade(&self, player: PlayerId, h: StructureHandle) -> Result<BuildingId, Refusal> {
        let c = self.conquest.as_ref().ok_or(Refusal::NoBuildings)?;
        c.can_upgrade(player, h).map(|(_, to)| to)
    }

    /// Whether a building of type `building` could go up now with its
    /// north-west tile at `tile`, and if not, why: the check
    /// [`Command::SpawnStructure`] makes.
    pub fn can_build(&self, building: BuildingId, tile: (i32, i32)) -> Result<(), Refusal> {
        self.conquest.as_ref().ok_or(Refusal::NoBuildings)?.check(
            building,
            tile,
            &self.nav,
            &self.regiments,
        )
    }

    /// The node regiment `h` is gathering at, if any (Conquest only).
    pub fn gathers_at(&self, h: RegimentHandle) -> Option<u16> {
        let i = self.regiments.resolve(h)?;
        self.economy()?.work_of(&self.regiments, i)
    }

    /// `player`'s workers with nothing to do (Conquest only): standing
    /// still, unbroken, out of melee, and gathering, building or attacking
    /// nothing.
    pub fn idle_workers(&self, player: PlayerId) -> impl Iterator<Item = RegimentHandle> + '_ {
        self.conquest.iter().flat_map(move |c| {
            self.regiments
                .iter()
                .filter(move |r| {
                    r.owner == player
                        && !r.moving
                        && !r.engaged
                        && !r.morale.broken()
                        && c.units.get(usize::from(r.unit)).is_some_and(|u| u.worker)
                        && self.regiments.resolve(r.handle).is_some_and(|i| {
                            c.economy.work_of(&self.regiments, i).is_none()
                                && c.structures.job_of(&self.regiments, i).is_none()
                        })
                })
                .map(|r| r.handle)
        })
    }

    /// What regiment `h` carries, if anything (Conquest only).
    pub fn carries(&self, h: RegimentHandle) -> Option<(Resource, u32)> {
        let i = self.regiments.resolve(h)?;
        self.economy()?.carried(&self.regiments, i)
    }

    /// The building regiment `h` is attacking, if any.
    pub fn siege_target(&self, h: RegimentHandle) -> Option<StructureHandle> {
        self.job_of(h, Work::Attack)
    }

    /// The building regiment `h` is building or mending, if any.
    pub fn building_target(&self, h: RegimentHandle) -> Option<StructureHandle> {
        self.job_of(h, Work::Build)
    }

    fn job_of(&self, h: RegimentHandle, work: Work) -> Option<StructureHandle> {
        let i = self.regiments.resolve(h)?;
        self.structures()?
            .job_of(&self.regiments, i)
            .filter(|&(_, w)| w == work)
            .map(|(s, _)| s)
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
        self.events.clear();
        // Time stands still while the armies deploy.
        if self.deploying() {
            self.judge.deploying();
            self.tick += 1;
            return stats;
        }
        // A rehearsal has nobody to tell.
        if !self.scratch {
            self.events.before(&self.regiments);
        }
        self.combat.before_move(
            &mut self.regiments,
            &self.units,
            &self.stances,
            &self.terrain,
            self.tick,
        );
        self.combat.push_lines(
            &mut self.regiments,
            &self.stances,
            &self.terrain,
            &self.units,
            &self.nav,
            self.map_tiles,
        );
        self.nav.plan(&mut self.regiments, &self.units);
        self.combat.mark_holders(&self.regiments, &self.nav);
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
            &self.vision,
            self.tick,
            &mut self.combat_rng,
        );
        if let Some(conquest) = &mut self.conquest {
            conquest.update(&mut self.regiments, &self.units, &mut self.nav);
            self.raise_trained();
        }
        morale::update(
            &mut self.regiments,
            &self.combat,
            &self.units,
            &self.stances,
            &self.vision,
            self.tick,
            self.map_tiles,
        );
        if let Some(champions) = &mut self.champions {
            champions.update(
                &mut self.regiments,
                &self.units,
                &self.stances,
                &self.vision,
                self.tick,
            );
        }
        self.vision.update(
            &mut self.regiments,
            &self.combat,
            &self.units,
            &self.stances,
            &self.terrain,
            self.conquest.as_mut().map(|c| &mut c.structures),
            self.tick,
        );
        if !self.scratch {
            let consumed = self
                .conquest
                .as_ref()
                .map_or(&[][..], |c| &c.structures.consumed[..]);
            self.events.after(
                &self.regiments,
                &self.combat,
                self.vision.ambushes(),
                consumed,
            );
            if let Some(conquest) = &self.conquest {
                self.events.fallen(&conquest.structures.fallen);
            }
        }
        self.tick += 1;
        self.judge
            .update(&self.regiments, &self.stances, &self.vision, self.tick);
        self.judge_conquest();
        stats
    }

    /// Conquest's once-a-second look (see [`verdict`]): who still has a
    /// main building, who is at the Heart, and who is out.
    fn judge_conquest(&mut self) {
        let Some(c) = &mut self.conquest else {
            return;
        };
        let battle = self.deploy.battle_ticks(self.tick);
        let out = self.judge.update_conquest(
            &self.regiments,
            || c.structures.mains(),
            &self.stances,
            self.tick,
            battle,
        );
        if out != 0 {
            c.eliminate(out, &mut self.regiments, &mut self.nav);
            self.stances.take_back_from(out);
        }
    }

    fn apply(&mut self, pc: &PlayerCommand) -> bool {
        // While the armies deploy, only setting out and placing armies,
        // formations, bracing, readiness and hand-overs are accepted.
        if self.deploying()
            && !matches!(
                pc.command,
                Command::DeployZone { .. }
                    | Command::SpawnRegiment { .. }
                    | Command::SpawnStructure { .. }
                    | Command::MoveTo { .. }
                    | Command::SetFormation { .. }
                    | Command::Brace { .. }
                    | Command::Ready
                    | Command::HandOver { .. }
            )
        {
            return false;
        }
        match pc.command {
            Command::SpawnRegiment {
                unit,
                position,
                facing,
            } => {
                let Some(stats) = self.units.get(unit as usize) else {
                    return false;
                };
                // Inside your zone while deploying, if you have one.
                if self.deploying()
                    && self
                        .deploy
                        .zone(pc.player)
                        .is_some_and(|z| !z.contains(position))
                {
                    return false;
                }
                let Some(h) = self
                    .regiments
                    .spawn(pc.player, unit, position, facing, stats)
                else {
                    return false;
                };
                // Hidden or seen from the moment it is placed.
                self.vision.start(
                    &self.regiments,
                    &self.units,
                    &self.terrain,
                    h.index as usize,
                );
                true
            }
            Command::MoveTo {
                regiment,
                target,
                facing,
                files,
                pace,
            } => {
                if self.deploying() {
                    return self.place(pc.player, regiment, target, facing, files);
                }
                // Players may only order the regiments they command.
                match self.regiments.owner(regiment) {
                    Some(owner) if self.stances.can_command(pc.player, owner) => {
                        let done = self.regiments.order(
                            regiment,
                            Order {
                                target,
                                facing,
                                files,
                                pace,
                            },
                        );
                        if done {
                            let i = regiment.index as usize;
                            stand_back(&mut self.regiments, &self.units, &self.nav, i);
                            self.leave_off(regiment);
                        }
                        done
                    }
                    _ => false,
                }
            }
            Command::Attack { regiment, target } => {
                match (self.regiments.owner(regiment), self.regiments.owner(target)) {
                    (Some(owner), Some(enemy))
                        if self.stances.can_command(pc.player, owner)
                            && self.stances.hostile(owner, enemy)
                            && self.sees(owner, target) =>
                    {
                        let done = self.regiments.attack(regiment, target);
                        if done {
                            self.leave_off(regiment);
                        }
                        done
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
                Some(owner) if self.stances.can_command(pc.player, owner) => self
                    .regiments
                    .set_formation(regiment, formation, &self.units),
                _ => false,
            },
            Command::FallBack { regiment } => match self.regiments.resolve(regiment) {
                Some(i) if self.stances.can_command(pc.player, self.regiments.owner[i]) => {
                    let to = self.fall_back_point(i);
                    self.regiments.fall_back(regiment, to)
                }
                _ => false,
            },
            Command::Brace { regiment, on } => match self.regiments.owner(regiment) {
                Some(owner) if self.stances.can_command(pc.player, owner) => {
                    self.regiments.brace(regiment, on, &self.units)
                }
                _ => false,
            },
            Command::DeployZone { center, radius } => {
                self.deploy
                    .set_zone(pc.player, Zone { center, radius }, self.tick)
            }
            Command::Ready => self.deploy.set_ready(pc.player, self.tick),
            // Nobody hands an army to, or from, a party that is out.
            Command::HandOver { to } => {
                !self.is_out(pc.player) && !self.is_out(to) && self.stances.hand_over(pc.player, to)
            }
            Command::SpawnStructure { building, tile } => {
                let Some(conquest) = &mut self.conquest else {
                    return false;
                };
                let tile = (i32::from(tile.0), i32::from(tile.1));
                usize::from(pc.player) < MAX_PARTIES
                    && conquest
                        .check(building, tile, &self.nav, &self.regiments)
                        .is_ok()
                    && conquest
                        .structures
                        .spawn(pc.player, building, tile, &mut self.nav, &self.regiments)
                        .is_some()
            }
            Command::AttackStructure {
                regiment,
                structure,
            } => {
                let Some(conquest) = &mut self.conquest else {
                    return false;
                };
                match (
                    self.regiments.resolve(regiment),
                    conquest.structures.owner(structure),
                ) {
                    (Some(r), Some(enemy))
                        if self.stances.can_command(pc.player, self.regiments.owner[r])
                            && self.stances.hostile(self.regiments.owner[r], enemy)
                            && !self.regiments.morale_state[r].broken()
                            // In fog of war, only a building its side has seen.
                            && (conquest.structures.get(structure))
                                .is_some_and(|s| s.seen_by(self.regiments.owner[r])) =>
                    {
                        conquest.stop_work(r);
                        conquest
                            .structures
                            .besiege(&mut self.regiments, r, structure);
                        true
                    }
                    _ => false,
                }
            }
            Command::Gather { regiment, node } => {
                let Some(conquest) = &mut self.conquest else {
                    return false;
                };
                match self.regiments.resolve(regiment) {
                    Some(r)
                        if self.regiments.owner[r] == pc.player
                            && !self.regiments.morale_state[r].broken()
                            && conquest.can_gather(&self.regiments, r, node) =>
                    {
                        // Sent again to the node it works: it keeps on.
                        if conquest.economy.work_of(&self.regiments, r) != Some(node) {
                            conquest.stop_work(r);
                            conquest.economy.gather(&mut self.regiments, r, node);
                        }
                        true
                    }
                    _ => false,
                }
            }
            Command::Build {
                regiment,
                building,
                tile,
            } => {
                let tile = (i32::from(tile.0), i32::from(tile.1));
                match (self.worker_of(pc.player, regiment), &mut self.conquest) {
                    (Some(r), Some(c)) => c.lay(
                        pc.player,
                        r,
                        building,
                        tile,
                        &mut self.nav,
                        &mut self.regiments,
                    ),
                    _ => false,
                }
            }
            Command::Construct {
                regiment,
                structure,
            } => match (self.worker_of(pc.player, regiment), &mut self.conquest) {
                (Some(r), Some(c)) => c.construct(pc.player, r, structure, &mut self.regiments),
                _ => false,
            },
            Command::CallOff { structure } => self
                .conquest
                .as_mut()
                .is_some_and(|c| c.call_off(pc.player, structure, &mut self.nav, &self.regiments)),
            Command::Train { structure, unit } => self
                .conquest
                .as_mut()
                .is_some_and(|c| c.train(pc.player, structure, unit)),
            Command::Untrain { structure } => self
                .conquest
                .as_mut()
                .is_some_and(|c| c.untrain(pc.player, structure)),
            Command::Upgrade { structure } => self
                .conquest
                .as_mut()
                .is_some_and(|c| c.upgrade(pc.player, structure)),
            Command::Rally { structure, at } => self
                .conquest
                .as_mut()
                .is_some_and(|c| c.set_rally(pc.player, structure, at)),
            Command::RemoveStructure { structure } => self.conquest.as_mut().is_some_and(|c| {
                c.structures
                    .despawn(structure, &mut self.nav, &self.regiments)
            }),
        }
    }

    /// The slot of `player`'s regiment `h`, if it is theirs and not
    /// broken: one they can set to work.
    fn worker_of(&self, player: PlayerId, h: RegimentHandle) -> Option<usize> {
        self.regiments.resolve(h).filter(|&r| {
            self.regiments.owner[r] == player && !self.regiments.morale_state[r].broken()
        })
    }

    /// Put the regiments that finished training this tick on the field,
    /// out of the building's south side and marching to its rally point.
    fn raise_trained(&mut self) {
        // Down the map (scenario::NORTH is up it).
        const SOUTH: Angle = Angle(0x4000);
        let Some(conquest) = &mut self.conquest else {
            return;
        };
        // Taken out and put back, so the list keeps its room.
        let trained = core::mem::take(&mut conquest.production.trained);
        for t in &trained {
            let spawned = self
                .units
                .get(usize::from(t.unit))
                .and_then(|stats| self.regiments.spawn(t.owner, t.unit, t.at, SOUTH, stats));
            let Some(h) = spawned else {
                // No room for it: what it cost comes back.
                let u = conquest.unit(t.unit);
                conquest
                    .economy
                    .refund(t.owner, u32::from(u.cost_grub), u32::from(u.cost_marrow));
                continue;
            };
            self.vision.start(
                &self.regiments,
                &self.units,
                &self.terrain,
                h.index as usize,
            );
            if let Some(target) = t.rally {
                self.regiments.order(
                    h,
                    Order {
                        target,
                        facing: None,
                        files: 0,
                        pace: Fx::ZERO,
                    },
                );
                let i = h.index as usize;
                stand_back(&mut self.regiments, &self.units, &self.nav, i);
            }
        }
        conquest.production.trained = trained;
    }

    /// Regiment `h` was given another order: it stops attacking any
    /// building and stops gathering.
    fn leave_off(&mut self, h: RegimentHandle) {
        if let (Some(conquest), Some(i)) = (&mut self.conquest, self.regiments.resolve(h)) {
            conquest.stop_work(i);
        }
    }

    /// Where regiment slot `i` falls back to: [`combat::FALL_BACK_TILES`]
    /// straight back from the foe it is fighting, or behind it if it isn't
    /// fighting, kept on the map.
    fn fall_back_point(&self, i: usize) -> FxVec2 {
        let regs = &self.regiments;
        let here = regs.position[i];
        let behind = regs.facing[i].to_vec() * -Fx::ONE;
        let away = regs
            .foe(i)
            .map(|j| here - regs.position[j])
            .filter(|d| *d != FxVec2::ZERO)
            .map_or(behind, FxVec2::normalized);
        let to = here + away * combat::FALL_BACK_TILES;
        if self.map_tiles == 0 {
            return to;
        }
        let edge = Fx::from_int(self.map_tiles as i32);
        FxVec2::new(to.x.clamp(Fx::ZERO, edge), to.y.clamp(Fx::ZERO, edge))
    }

    /// Deployment: put `player`'s `regiment` straight down at `position`,
    /// inside their zone, on the map and on ground it can stand on.
    fn place(
        &mut self,
        player: PlayerId,
        regiment: RegimentHandle,
        position: FxVec2,
        facing: Option<Angle>,
        files: u16,
    ) -> bool {
        let edge = Fx::from_int(self.map_tiles as i32);
        let on_map =
            (Fx::ZERO..=edge).contains(&position.x) && (Fx::ZERO..=edge).contains(&position.y);
        let Some(r) = self.regiments.get(regiment) else {
            return false;
        };
        // Whoever commands the regiment places it, inside its owner's zone.
        let allowed = self.stances.can_command(player, r.owner)
            && on_map
            && self
                .deploy
                .zone(r.owner)
                .is_some_and(|z| z.contains(position))
            && self.nav.walkable(position);
        let Some(i) = allowed
            .then(|| {
                self.regiments
                    .place(regiment, position, facing.unwrap_or(r.facing), files)
            })
            .flatten()
        else {
            return false;
        };
        self.vision
            .start(&self.regiments, &self.units, &self.terrain, i);
        true
    }

    /// Whether the armies are still deploying: time stands still, and only
    /// placing, formations and readiness are accepted (see [`deploy`]).
    pub fn deploying(&self) -> bool {
        self.deploy.active(self.tick)
    }

    /// `player`'s deployment zone, if the match has a deployment phase.
    pub fn deploy_zone(&self, player: PlayerId) -> Option<Zone> {
        self.deploying().then(|| self.deploy.zone(player)).flatten()
    }

    /// Whether `player` has said they are done deploying.
    pub fn deploy_ready(&self, player: PlayerId) -> bool {
        self.deploy.is_ready(player)
    }

    /// Ticks of battle so far: the ticks since deployment ended, or all of
    /// them in a match without one.
    pub fn battle_ticks(&self) -> u32 {
        self.deploy.battle_ticks(self.tick)
    }

    /// Ticks until the battle starts, while deploying.
    pub fn deploy_ticks_left(&self) -> u32 {
        self.deploy.ticks_left(self.tick)
    }

    /// Who fights whom.
    pub fn stances(&self) -> &Stances {
        &self.stances
    }

    /// Whether `a`'s regiments fight `b`'s (see [`Stances::hostile`]).
    pub fn hostile(&self, a: PlayerId, b: PlayerId) -> bool {
        self.stances.hostile(a, b)
    }

    /// The ground's heights, water and cover, and what they do in a fight.
    pub fn terrain(&self) -> &Terrain {
        &self.terrain
    }

    /// Whether the match is played in fog of war (see [`vision`]).
    pub fn fog(&self) -> bool {
        self.vision.fogged()
    }

    /// Where `party`'s side can see in fog of war, as circles (centre and
    /// radius, in tiles) into `out`, which is cleared first: a circle round
    /// each of the side's regiments and buildings, reaching as far as it
    /// sees straight out from its longer sides. For the game's drawing of
    /// the fog; the simulation's own sight is edge to edge, so it reaches a
    /// little further at the corners and less off the short sides.
    pub fn sight(&self, party: PlayerId, out: &mut Vec<(FxVec2, Fx)>) {
        out.clear();
        let friend = |owner| !self.stances.hostile(party, owner);
        // Round a block: as far as its sight reaches straight out from its
        // longer sides.
        let round =
            |b: combat::Block, sight: Fx| (b.centre, sight + b.half_width.max(b.half_depth));
        let regs = &self.regiments;
        for i in (0..regs.alive.len()).filter(|&i| regs.alive[i] && friend(regs.owner[i])) {
            out.push(round(
                combat::Block::of(regs, &self.units, i),
                vision::SIGHT,
            ));
        }
        let posts = self.vision.posts().filter(|&(owner, ..)| friend(owner));
        out.extend(posts.map(|(_, block, sight)| round(block, sight)));
    }

    /// Whether `party`'s side can see regiment `h` (see [`vision`]). Always
    /// true on a map with no cover and no fog of war, and for a party's own
    /// and its allies' regiments; false for a regiment that is gone.
    /// While deploying, no enemy regiment is seen.
    pub fn sees(&self, party: PlayerId, h: RegimentHandle) -> bool {
        self.regiments.resolve(h).is_some_and(|i| {
            let owner = self.regiments.owner[i];
            !(self.deploying() && self.stances.hostile(party, owner))
                && self.vision.sees(party, i, owner)
        })
    }

    /// How the battle was decided, once it has been: a side held the field
    /// (see [`verdict`]).
    pub fn verdict(&self) -> Option<Verdict> {
        self.judge.verdict()
    }

    /// The tick Conquest player `party` is out unless it lays a main
    /// building first, while it has none ([`EXILE_TICKS`] after it lost
    /// its last).
    pub fn exiled(&self, party: PlayerId) -> Option<u32> {
        self.judge.throne()?.exiled(party)
    }

    /// Whether Conquest player `party` is out of the match.
    pub fn is_out(&self, party: PlayerId) -> bool {
        self.judge.throne().is_some_and(|t| t.is_out(party))
    }

    /// The Titan's Heart, in a Conquest match played for it.
    pub fn heart(&self) -> Option<HeartView> {
        self.judge.throne()?.heart(self.battle_ticks())
    }

    /// The party holding the field right now, if one is (see [`verdict`]).
    pub fn holding_the_field(&self) -> Option<PlayerId> {
        // Conquest's rulings don't count it.
        if self.deploying() || self.judge.throne().is_some() {
            return None;
        }
        verdict::holding_the_field(&self.regiments, &self.stances, &self.vision)
    }

    /// Whether cover hides regiment `h` from every enemy of its owner (see
    /// [`vision`]; being out of sight in fog of war is not hiding).
    pub fn hidden(&self, h: RegimentHandle) -> bool {
        self.regiments
            .resolve(h)
            .is_some_and(|i| self.vision.hidden(i, self.regiments.owner[i]))
    }

    /// Whether regiment `h` is caught off guard by an ambush: its blows deal
    /// less damage for a few seconds.
    pub fn caught_off_guard(&self, h: RegimentHandle) -> bool {
        self.regiments
            .resolve(h)
            .is_some_and(|i| self.vision.off_guard(i))
    }

    /// What happened in the last tick, in a fixed order: volleys loosed,
    /// volleys landed, then each regiment's losses, change of heart and
    /// first contact, by slot, then ambushes. None in a rehearsal's scratch
    /// world.
    pub fn events(&self) -> &[events::Event] {
        self.events.list()
    }

    /// The ambushes sprung in the last tick.
    pub fn ambushes(&self) -> &[Ambush] {
        self.vision.ambushes()
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
        for system in 0..SYSTEMS.len() {
            self.hash_system(system, &mut h);
        }
        h.finish()
    }

    /// Each part of [`World::state_hash`] hashed on its own, in the order
    /// of [`SYSTEMS`]: when two machines' worlds part, comparing these
    /// names the system that went first (review fix R12).
    pub fn system_hashes(&self) -> [u64; SYSTEMS.len()] {
        core::array::from_fn(|system| {
            let mut h = StateHasher::new();
            self.hash_system(system, &mut h);
            h.finish()
        })
    }

    /// Feed one of [`SYSTEMS`] into `h`. Matching on the name keeps each
    /// arm tied to its entry: a name with no arm fails every test that
    /// hashes a world.
    fn hash_system(&self, system: usize, h: &mut StateHasher) {
        match SYSTEMS[system] {
            "clock" => h.u32(self.tick),
            "random streams" => {
                for rng in [&self.rng, &self.combat_rng] {
                    let (state, inc) = rng.raw_state();
                    h.u64(state);
                    h.u64(inc);
                }
            }
            "unit stats" => {
                h.u32(self.units.len() as u32);
                for u in &self.units {
                    u.hash_into(h);
                }
            }
            "regiments" => {
                self.regiments.hash_into(h);
                self.stances.hash_into(h);
            }
            "combat" => self.combat.hash_into(h),
            "pathfinding" => self.nav.hash_into(h),
            "vision" => self.vision.hash_into(&self.regiments, h),
            "deployment" => self.deploy.hash_into(h),
            "verdict" => self.judge.hash_into(h),
            // Only in matches that allow champions, so other worlds hash as
            // they did before champions existed.
            "champions" => {
                if let Some(champions) = &self.champions {
                    champions.hash_into(h);
                }
            }
            // Only in Conquest, so Field Battles hash as before buildings.
            "structures" => {
                if let Some(conquest) = &self.conquest {
                    conquest.hash_structures(&self.regiments, h);
                }
            }
            "economy" => {
                if let Some(economy) = self.economy() {
                    economy.hash_into(&self.regiments, h);
                }
            }
            "production" => {
                if let Some(c) = &self.conquest {
                    c.production.hash_into(&c.structures, h);
                }
            }
            other => unreachable!("no hash for system {other}"),
        }
    }

    /// The champions' challenge, once one has been called (it stays after
    /// the duel is over).
    pub fn challenge(&self) -> Option<&Challenge> {
        self.champions.as_ref()?.challenge()
    }
}

/// Random substream ids, so adding draws in one system never shifts another.
pub mod streams {
    pub const WORLD: u64 = 1;
    pub const COMBAT: u64 = 2;
    pub const EVENTS: u64 = 3;
}

/// Slot `i` was just sent somewhere: if its block would hang over the
/// water there, it stands back from the edge instead (see [`Nav::settle`]).
fn stand_back(regs: &mut Regiments, units: &[UnitStats], nav: &Nav, i: usize) {
    if let Some(target) = regs.target[i] {
        let berth = regs.berth(units, i);
        regs.target[i] = Some(nav.settle(target, berth));
    }
}
