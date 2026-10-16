//! Scripted command streams that stand in for players.
//!
//! Used by the determinism tests, benchmarks and the M0 demo. A scenario is
//! itself deterministic: the same seed produces the same commands.

use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2, Pcg32};

use crate::formation::{self, GroupMember, GroupOrder};
use crate::{Command, Mass, PlayerCommand, PlayerId, RegimentHandle, UnitId, UnitStats, World};

/// Turn rate of `deg` degrees per second, in angle steps per second.
const fn degrees_per_second(deg: u32) -> u32 {
    deg * 65_536 / 360
}

/// Fixed regiment types for tests and benchmarks (one per player), so their
/// golden hashes don't change when `data/units.ron` is tuned. Type 2 shoots.
pub const DRILL_UNITS: [UnitStats; 4] = [
    UnitStats {
        speed: Fx::from_int(3),
        turn_rate: degrees_per_second(60),
        models: 24,
        hp_per_model: 10,
        files: 8,
        file_spacing: Fx::from_ratio(3, 4),
        rank_spacing: Fx::from_ratio(17, 20),
        melee_attack: 30,
        melee_defence: 45,
        melee_damage: 3,
        charge_bonus: 20,
        range: Fx::ZERO,
        missile_damage: 0,
        reload_ticks: 0,
        accuracy: 0,
        morale: 60,
        mass: Mass::Medium,
        reach: false,
        swing_room: false,
        shield: false,
    },
    UnitStats {
        speed: Fx::from_int(4),
        turn_rate: degrees_per_second(120),
        models: 24,
        hp_per_model: 6,
        files: 6,
        file_spacing: Fx::from_ratio(4, 5),
        rank_spacing: Fx::from_ratio(4, 5),
        melee_attack: 40,
        melee_defence: 20,
        melee_damage: 2,
        charge_bonus: 40,
        range: Fx::ZERO,
        missile_damage: 0,
        reload_ticks: 0,
        accuracy: 0,
        morale: 45,
        mass: Mass::Medium,
        reach: false,
        swing_room: false,
        shield: false,
    },
    UnitStats {
        speed: Fx::from_ratio(13, 4),
        turn_rate: degrees_per_second(80),
        models: 12,
        hp_per_model: 9,
        files: 6,
        file_spacing: Fx::from_ratio(7, 10),
        rank_spacing: Fx::from_ratio(9, 10),
        melee_attack: 15,
        melee_defence: 20,
        melee_damage: 2,
        charge_bonus: 5,
        range: Fx::from_int(25),
        missile_damage: 4,
        reload_ticks: 80,
        accuracy: 30,
        morale: 50,
        mass: Mass::Medium,
        reach: false,
        swing_room: false,
        shield: false,
    },
    UnitStats {
        speed: Fx::from_ratio(5, 2),
        turn_rate: degrees_per_second(45),
        models: 20,
        hp_per_model: 8,
        files: 10,
        file_spacing: Fx::from_ratio(11, 20),
        rank_spacing: Fx::from_ratio(9, 10),
        melee_attack: 25,
        melee_defence: 50,
        melee_damage: 3,
        charge_bonus: 10,
        range: Fx::ZERO,
        missile_damage: 0,
        reload_ticks: 0,
        accuracy: 0,
        morale: 70,
        mass: Mass::Medium,
        reach: false,
        swing_room: false,
        shield: false,
    },
];

/// Regiments face north (up the map) when spawned by the drill.
pub const NORTH: Angle = Angle(0xC000);

/// Each player (four by default) spawns a block of regiments, then keep ordering them about
/// the map: a plain move, a move with a new facing and frontage, and a group
/// move of three regiments. Player `p` fields unit type `p % units` from the
/// world's unit table.
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
    /// Picks the unit type of player `p`'s `k`th regiment, given how many
    /// types there are. By default every player fields type `p % types`.
    pub unit_of: fn(player: PlayerId, k: u32, types: usize) -> UnitId,
    /// Scratch space for expanding group orders.
    group_commands: Vec<Command>,
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
            unit_of: |player, _, types| (player as usize % types) as UnitId,
            group_commands: Vec::with_capacity(8),
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
                // Players past four take the middles of the north and south.
                let cx = if player >= 4 {
                    self.map_tiles / 2
                } else if player & 1 == 0 {
                    quarter
                } else {
                    self.map_tiles - quarter
                };
                let cy = if (player >= 4 && player & 1 == 0) || (player < 4 && player & 2 == 0) {
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
                            unit: (self.unit_of)(player, k as u32, unit_types),
                            position,
                            facing: NORTH,
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
            let Some(regiment) = self.pick_regiment(world, player) else {
                continue;
            };
            // A plain move: face the way it walks, keep its frontage.
            let target = self.random_point();
            out.push(PlayerCommand {
                player,
                command: Command::MoveTo {
                    regiment,
                    target,
                    facing: None,
                    files: 0,
                    pace: Fx::ZERO,
                },
            });

            // A move with a chosen facing and frontage (0 keeps the frontage).
            if let Some(regiment) = self.pick_regiment(world, player) {
                let target = self.random_point();
                let facing = Some(Angle(self.rng.next_u32() as u16));
                let files = self.rng.below(13) as u16;
                out.push(PlayerCommand {
                    player,
                    command: Command::MoveTo {
                        regiment,
                        target,
                        facing,
                        files,
                        pace: Fx::ZERO,
                    },
                });
            }

            // A group move of up to three different regiments.
            let mut members = [GroupMember {
                handle: regiment,
                position: FxVec2::ZERO,
                facing: Angle::ZERO,
                files: 0,
                models: 0,
                file_spacing: Fx::ZERO,
                rank_spacing: Fx::ZERO,
                speed: Fx::ZERO,
            }; 3];
            let mut count = 0;
            for _ in 0..members.len() {
                let Some(handle) = self.pick_regiment(world, player) else {
                    break;
                };
                if members[..count].iter().any(|m| m.handle == handle) {
                    continue;
                }
                let r = world.regiments.get(handle).unwrap();
                members[count] = GroupMember::of(&r, &world.units()[r.unit as usize]);
                count += 1;
            }
            let target = self.random_point();
            self.group_commands.clear();
            let order = GroupOrder {
                target,
                facing: None,
                width: None,
            };
            formation::group_move(&mut members[..count], order, &mut self.group_commands);
            out.extend(
                self.group_commands
                    .iter()
                    .map(|&command| PlayerCommand { player, command }),
            );
        }
    }

    fn pick_regiment(&mut self, world: &World, player: PlayerId) -> Option<RegimentHandle> {
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

/// A test map `map_tiles` across, as nav costs (see [`crate::nav`]): long
/// diagonal ridges 3 tiles thick every 96 tiles, each broken by an 8-tile
/// pass every 40 tiles, and a lake ringed by bog in the middle. Integer
/// only, so every machine builds the same map.
pub fn ribs_map(map_tiles: u32) -> Vec<u8> {
    let side = map_tiles as i32 * crate::nav::CELLS_PER_TILE;
    let mut cells = Vec::with_capacity((side * side) as usize);
    let (cx, cy) = (side / 2, side / 2);
    for y in 0..side {
        for x in 0..side {
            // Across and along the ridges, in fifths of a cell (|(4, 3)| = 5).
            let across = (4 * x + 3 * y).rem_euclid(192 * 5);
            let along = (3 * x - 4 * y).rem_euclid(80 * 5);
            let ridge = across < 6 * 5 && along >= 16 * 5;
            let d2 = (x - cx) * (x - cx) + (y - cy) * (y - cy);
            cells.push(if ridge || d2 < 24 * 24 {
                crate::nav::BLOCKED
            } else if d2 < 36 * 36 {
                3
            } else {
                1
            });
        }
    }
    cells
}
