//! Which of the four factions' roster troops the sandbox can place
//! (docs/08-ui-ux.md#the-place-panel, docs/17-unit-rosters.md).
//!
//! The roster lives in `GameData::conquest_units`, a unit table of its own.
//! The sandbox places any troop of it by running on that table (the Place
//! panel's "All factions" tab). Everything in the table that fights as a
//! regiment is placeable: summoned units (Spectral Samurai, Bound Dead...)
//! and the tier-3 monsters included. What is left out is what is not a
//! regiment to fight with: the workers, which gather and build, and the
//! Carrion Concord's Caravan, which hauls Grub.
//!
//! The table holds no buildings (they have their own, and the sandbox's
//! building tool places those) and no heroes yet.

use gc_data::conquest::{Class, UNIT_BUILDS_ONLY};
use gc_data::{ConquestUnitRecord, Faction};

/// Troops the sandbox leaves out by name: not workers by class, yet not
/// regiments to fight with either.
const NOT_REGIMENTS: [&str; 1] = ["Caravan"];

/// Why the sandbox does not place `unit`, or `None` when it can.
pub fn left_out(unit: &ConquestUnitRecord) -> Option<&'static str> {
    if unit.worker() || unit.class() == Class::Worker || unit.flags & UNIT_BUILDS_ONLY != 0 {
        Some("a worker: it gathers and builds, which needs Conquest's economy")
    } else if NOT_REGIMENTS.contains(&unit.name()) {
        Some("a caravan: it hauls Grub, it is not a fighting regiment")
    } else {
        None
    }
}

/// The table indices of `faction`'s placeable troops, in roster order.
pub fn placeable(units: &[ConquestUnitRecord], faction: Faction) -> Vec<usize> {
    (0..units.len())
        .filter(|&i| units[i].faction() == faction && left_out(&units[i]).is_none())
        .collect()
}

/// The names of `faction`'s troops left out, comma-separated, and how many.
pub fn left_out_names(units: &[ConquestUnitRecord], faction: Faction) -> (usize, String) {
    let names: Vec<&str> = units
        .iter()
        .filter(|u| u.faction() == faction && left_out(u).is_some())
        .map(ConquestUnitRecord::name)
        .collect();
    (names.len(), names.join(", "))
}

/// A faction's short name, for the Place panel's chips.
pub fn short_name(faction: Faction) -> &'static str {
    match faction {
        Faction::EmberClans => "Ember",
        Faction::Brood => "Brood",
        Faction::Choir => "Choir",
        Faction::CarrionConcord => "Carrion",
    }
}

/// What a troop costs, in a few words: its Grub and Marrow, or that it is
/// summoned (a power calls it, nobody trains it).
pub fn cost_text(unit: &ConquestUnitRecord) -> String {
    if unit.tier == 0 {
        "summoned".into()
    } else if unit.cost_marrow > 0 {
        format!("{} Grub {} Marrow", unit.cost_grub, unit.cost_marrow)
    } else {
        format!("{} Grub", unit.cost_grub)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_faction_has_troops_to_place_and_only_non_regiments_are_left_out() {
        let data = crate::data::load();
        let mut listed = 0;
        for f in Faction::ALL {
            let ids = placeable(data.conquest_units, f);
            assert!(ids.len() >= 6, "{f:?} lists {} troops", ids.len());
            listed += ids.len();
        }
        let left: Vec<&str> = data
            .conquest_units
            .iter()
            .filter(|u| left_out(u).is_some())
            .map(ConquestUnitRecord::name)
            .collect();
        assert_eq!(
            listed + left.len(),
            data.conquest_units.len(),
            "every troop is listed or left out"
        );
        assert_eq!(
            left,
            ["Hands", "Gnawer", "Tuner", "Ledger Clerk", "Caravan"],
            "left out: the workers and the caravan"
        );
    }

    /// A flat 128-tile world on the roster table, set up as the sandbox on
    /// that table is: the roster's unit stats and the building tool's works.
    fn sandbox_world(data: &gc_data::GameData<'_>) -> gc_sim::World {
        let units: Vec<gc_sim::UnitStats> = data
            .conquest_units
            .iter()
            .map(|u| gc_match::stats_of(&u.stats))
            .collect();
        let map = gc_map::Map::generate(7, 128);
        let nav = map.nav_costs();
        let setup = gc_sim::MatchSetup {
            seed: 7,
            capacities: gc_sim::Capacities::default(),
            units: &units,
            map_tiles: map.tiles,
            nav: &nav,
            teams: &[],
            ground: map.ground(),
            champions: false,
        };
        let mut world = gc_sim::World::new(&setup);
        crate::buildings::Table::new(data).conquer(&mut world);
        world
    }

    /// The nearest walkable whole tile to (x, y), as the Place panel only puts
    /// regiments on ground they can stand on.
    fn stand(world: &gc_sim::World, x: i32, y: i32) -> (i32, i32) {
        for r in 0..40 {
            for dy in -r..=r {
                for dx in -r..=r {
                    let at = gc_fixed::FxVec2::from_ints(x + dx, y + dy);
                    if world.nav().walkable(at) {
                        return (x + dx, y + dy);
                    }
                }
            }
        }
        panic!("no walkable ground near {x},{y}");
    }

    /// A tile with open ground for ten tiles east and two either side.
    fn open_ground(world: &gc_sim::World) -> (i32, i32) {
        for y in 8..120 {
            for x in 8..100 {
                let open = (-2..=2).all(|dy| {
                    (0..=10).all(|dx| {
                        world
                            .nav()
                            .walkable(gc_fixed::FxVec2::from_ints(x + dx, y + dy))
                    })
                });
                if open {
                    return (x, y);
                }
            }
        }
        panic!("no open ground on the test map");
    }

    fn spawn(
        world: &gc_sim::World,
        unit: usize,
        side: u8,
        x: i32,
        y: i32,
        facing: i32,
    ) -> gc_sim::PlayerCommand {
        let (x, y) = stand(world, x, y);
        gc_sim::PlayerCommand {
            player: side,
            command: gc_sim::Command::SpawnRegiment {
                unit: unit as gc_sim::UnitId,
                position: gc_fixed::FxVec2::from_ints(x, y),
                facing: gc_fixed::Angle::of_vec(gc_fixed::FxVec2::from_ints(facing, 0)),
            },
        }
    }

    fn soldiers(world: &gc_sim::World) -> u32 {
        world.regiments.iter().map(|r| u32::from(r.models)).sum()
    }

    /// Every listed troop of every faction can be placed for any side, and
    /// fights without a panic: each meets another faction's troop head on.
    #[test]
    fn every_listed_troop_places_and_fights_on_the_roster_table() {
        let data = crate::data::load();
        let all: Vec<(usize, Faction)> = Faction::ALL
            .iter()
            .flat_map(|&f| {
                placeable(data.conquest_units, f)
                    .into_iter()
                    .map(move |i| (i, f))
            })
            .collect();
        assert_eq!(all.len(), 38);
        for (n, &(unit, faction)) in all.iter().enumerate() {
            // The foe: the next listed troop of another faction.
            let foe = all
                .iter()
                .cycle()
                .skip(n + 1)
                .find(|(_, f)| *f != faction)
                .unwrap()
                .0;
            let mut world = sandbox_world(&data);
            // Place for different sides (including side 3) to prove no side matters.
            let side = (n % 4) as u8;
            let other = (side + 1) % 4;
            let (x, y) = open_ground(&world);
            let spawns = [
                spawn(&world, unit, side, x, y, 1),
                spawn(&world, foe, other, x + 5, y, -1),
            ];
            world.tick(&spawns);
            assert_eq!(
                world.regiments.iter().count(),
                2,
                "{}",
                data.conquest_units[unit].name()
            );
            let before = soldiers(&world);
            // Each attacks the other, as a click on an enemy would order.
            let handles: Vec<_> = world
                .regiments
                .iter()
                .map(|r| (r.handle, r.owner))
                .collect();
            let attacks: Vec<gc_sim::PlayerCommand> = handles
                .iter()
                .zip(handles.iter().rev())
                .map(
                    |(&(regiment, player), &(target, _))| gc_sim::PlayerCommand {
                        player,
                        command: gc_sim::Command::Attack { regiment, target },
                    },
                )
                .collect();
            world.tick(&attacks);
            for _ in 0..40 * gc_sim::TICKS_PER_SECOND {
                world.tick(&[]);
            }
            assert!(
                soldiers(&world) < before,
                "{} against {} never fought",
                data.conquest_units[unit].name(),
                data.conquest_units[foe].name()
            );
        }
    }

    /// One troop of each faction, then the whole roster at once, four sides
    /// to the corners, marching on the middle for a minute.
    #[test]
    fn the_whole_roster_fights_in_one_sandbox() {
        let data = crate::data::load();
        let mut world = sandbox_world(&data);
        let mut spawns = Vec::new();
        for (k, &f) in Faction::ALL.iter().enumerate() {
            let (x0, y0) = (12 + 90 * (k as i32 % 2), 12 + 90 * (k as i32 / 2));
            for (j, unit) in placeable(data.conquest_units, f).into_iter().enumerate() {
                spawns.push(spawn(
                    &world,
                    unit,
                    k as u8,
                    x0 + 8 * (j as i32 % 3),
                    y0 + 8 * (j as i32 / 3),
                    1,
                ));
            }
        }
        world.tick(&spawns);
        assert_eq!(world.regiments.iter().count(), spawns.len());
        let orders: Vec<gc_sim::PlayerCommand> = world
            .regiments
            .iter()
            .map(|r| gc_sim::PlayerCommand {
                player: r.owner,
                command: gc_sim::Command::MoveTo {
                    regiment: r.handle,
                    target: gc_fixed::FxVec2::from_ints(64, 64),
                    facing: None,
                    files: 0,
                    pace: gc_fixed::Fx::ZERO,
                },
            })
            .collect();
        let before = soldiers(&world);
        world.tick(&orders);
        for _ in 0..90 * gc_sim::TICKS_PER_SECOND {
            world.tick(&[]);
        }
        assert!(
            soldiers(&world) < before,
            "the four factions met and fought"
        );
    }
}
