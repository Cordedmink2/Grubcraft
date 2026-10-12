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

mod hash;
mod regiments;
pub mod scenario;

pub use hash::StateHasher;
pub use regiments::{RegimentHandle, RegimentView, Regiments};

use gc_fixed::{Fx, FxVec2, Pcg32};

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

/// Everything needed to start a match identically on every machine.
#[derive(Clone, Copy, Debug)]
pub struct MatchSetup {
    pub seed: u64,
    pub capacities: Capacities,
}

/// A player's order. The only way anything outside the sim changes the sim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Debug/sandbox: create a regiment. Real production comes with M3.
    SpawnRegiment { position: FxVec2 },
    MoveTo {
        regiment: RegimentHandle,
        target: FxVec2,
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
    pub regiments: Regiments,
}

impl World {
    pub fn new(setup: &MatchSetup) -> World {
        World {
            tick: 0,
            rng: Pcg32::new(setup.seed, streams::WORLD),
            regiments: Regiments::with_capacity(setup.capacities.regiments),
        }
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
        self.regiments.step_movement();
        self.tick += 1;
        stats
    }

    fn apply(&mut self, pc: &PlayerCommand) -> bool {
        match pc.command {
            Command::SpawnRegiment { position } => self
                .regiments
                .spawn(pc.player, position, Fx::from_ratio(3, 20))
                .is_some(),
            Command::MoveTo { regiment, target } => {
                // Players may only order their own regiments.
                match self.regiments.owner(regiment) {
                    Some(owner) if owner == pc.player => {
                        self.regiments.set_target(regiment, target)
                    }
                    _ => false,
                }
            }
        }
    }

    /// 64-bit hash of the complete simulation state, for desync detection.
    pub fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.u32(self.tick);
        let (state, inc) = self.rng.raw_state();
        h.u64(state);
        h.u64(inc);
        self.regiments.hash_into(&mut h);
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
