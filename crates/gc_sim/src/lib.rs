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
mod regiments;
pub mod scenario;

pub use hash::StateHasher;
pub use morale::MoraleState;
pub use regiments::{Order, RegimentHandle, RegimentView, Regiments};

use combat::Combat;
use formation::MAX_MODELS;
use gc_fixed::{Angle, Fx, FxVec2, Pcg32};

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

    /// Size of a block of `models` soldiers in `files` files: (width, depth).
    pub fn block_size(&self, files: u16, models: u16) -> (Fx, Fx) {
        formation::block_size(files, models, self.file_spacing, self.rank_spacing)
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
        self.combat.before_move(&mut self.regiments, &self.units);
        self.regiments.step_movement();
        self.combat.after_move(
            &mut self.regiments,
            &self.units,
            self.tick,
            &mut self.combat_rng,
        );
        morale::update(
            &mut self.regiments,
            &self.combat,
            &self.units,
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
            Command::Attack { regiment, target } => match self.regiments.owner(regiment) {
                Some(owner) if owner == pc.player => self.regiments.attack(regiment, target),
                _ => false,
            },
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
        self.regiments.hash_into(&mut h);
        self.combat.hash_into(&mut h);
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
