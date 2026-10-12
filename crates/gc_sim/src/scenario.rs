//! Scripted command streams that stand in for players.
//!
//! Used by the determinism tests, benchmarks and the M0 demo. A scenario is
//! itself deterministic: the same seed produces the same commands.

use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2, Pcg32};

use crate::{Command, PlayerCommand, PlayerId, UnitStats, World};

/// Fixed regiment types for tests and benchmarks (one per player), so their
/// golden hashes don't change when `data/units.ron` is tuned.
pub const DRILL_UNITS: [UnitStats; 4] = [
    UnitStats {
        speed: Fx::from_int(3),
    },
    UnitStats {
        speed: Fx::from_int(4),
    },
    UnitStats {
        speed: Fx::from_ratio(13, 4),
    },
    UnitStats {
        speed: Fx::from_ratio(5, 2),
    },
];

/// Four players each spawn a block of regiments, then keep ordering random
/// regiments to random points on the map. Player `p` fields unit type
/// `p % units` from the world's unit table.
pub struct Drill {
    rng: Pcg32,
    pub players: u8,
    pub regiments_per_player: u32,
    /// Map edge length in tiles.
    pub map_tiles: i32,
    /// Ticks between each round of orders.
    pub order_interval: u32,
    /// A player the drill spawns but never orders (left to a human).
    pub skip_player: Option<PlayerId>,
}

impl Drill {
    pub fn new(seed: u64) -> Drill {
        Drill {
            rng: Pcg32::new(seed, 0xD811),
            players: 4,
            regiments_per_player: 60,
            map_tiles: 256,
            order_interval: 10,
            skip_player: None,
        }
    }

    fn random_point(&mut self) -> FxVec2 {
        let max = (self.map_tiles as u32) << 16;
        FxVec2::new(
            Fx::from_raw(self.rng.below(max) as i32),
            Fx::from_raw(self.rng.below(max) as i32),
        )
    }

    /// Fill `out` with the commands for the world's next tick (clears it first).
    pub fn commands_for(&mut self, world: &World, out: &mut Vec<PlayerCommand>) {
        out.clear();
        let tick = world.tick_count();
        if tick == 0 {
            // Each player's block of regiments starts in its own corner.
            let quarter = self.map_tiles / 4;
            let unit_types = world.units().len().max(1);
            for player in 0..self.players {
                let cx = if player & 1 == 0 {
                    quarter
                } else {
                    self.map_tiles - quarter
                };
                let cy = if player & 2 == 0 {
                    quarter
                } else {
                    self.map_tiles - quarter
                };
                for k in 0..self.regiments_per_player as i32 {
                    let position =
                        FxVec2::from_ints(cx + (k % 10) * 7 - 32, cy + (k / 10) * 5 - 14);
                    out.push(PlayerCommand {
                        player,
                        command: Command::SpawnRegiment {
                            unit: (player as usize % unit_types) as u16,
                            position,
                        },
                    });
                }
            }
            return;
        }
        if !tick.is_multiple_of(self.order_interval) || world.regiments.is_empty() {
            return;
        }
        for player in 0..self.players {
            if self.skip_player == Some(player) {
                continue;
            }
            // Order a few of this player's regiments, picked by stepping through the pool.
            for _ in 0..3 {
                let Some(regiment) = self.pick_regiment(world, player) else {
                    break;
                };
                let target = self.random_point();
                out.push(PlayerCommand {
                    player,
                    command: Command::MoveTo { regiment, target },
                });
            }
        }
    }

    fn pick_regiment(&mut self, world: &World, player: PlayerId) -> Option<crate::RegimentHandle> {
        let owned = world.regiments.iter().filter(|r| r.owner == player).count() as u32;
        if owned == 0 {
            return None;
        }
        let nth = self.rng.below(owned) as usize;
        world
            .regiments
            .iter()
            .filter(|r| r.owner == player)
            .nth(nth)
            .map(|r| r.handle)
    }
}
