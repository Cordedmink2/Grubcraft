//! The simulation's own ruling on who won (`gc_sim::verdict`).

use gc_fixed::{Angle, FxVec2};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{
    Capacities, Command, Cover, HOLD_TICKS, MatchSetup, PlayerCommand, PlayerId, TICKS_PER_SECOND,
    Verdict, Win, World,
};

fn world(cover: &[u8]) -> World {
    World::new(&MatchSetup {
        seed: 1,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 64,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground {
            heights: &[],
            water_level: 0,
            cover,
            growth: &[],
            footing: &[],
        },
        champions: false,
    })
}

fn spawn(player: PlayerId, x: i32) -> PlayerCommand {
    PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::from_ints(x, 32),
            facing: Angle(0),
        },
    }
}

fn remove_all(world: &World, player: PlayerId) -> Vec<PlayerCommand> {
    world
        .regiments
        .iter()
        .filter(|r| r.owner == player)
        .map(|r| PlayerCommand {
            player,
            command: Command::RemoveRegiment { regiment: r.handle },
        })
        .collect()
}

#[test]
fn a_winner_hidden_in_a_wood_holds_the_field_once_nobody_stands_against_it() {
    // A wood over the west half of a small map.
    let cover: Vec<u8> = (0..64 * 64)
        .map(|i| if i % 64 < 32 { Cover::Wood as u8 } else { 0 })
        .collect();
    let mut world = world(&cover);
    world.tick(&[spawn(0, 12), spawn(1, 52)]);
    // The enemy in the open sees nothing of the hidden army: it holds.
    assert_eq!(world.holding_the_field(), Some(1));
    world.tick(&remove_all(&world, 1));
    // Nobody left against it: hiding doesn't matter.
    assert_eq!(world.holding_the_field(), Some(0));
}

#[test]
fn the_field_held_long_enough_decides_the_battle_for_good() {
    let mut world = world(&[]);
    world.tick(&[spawn(0, 20), spawn(1, 44)]);
    // Two armies in the open, in each other's sight.
    assert_eq!(world.holding_the_field(), None);
    world.tick(&remove_all(&world, 1));
    // The field is looked at once a second.
    let since = world.tick_count().next_multiple_of(TICKS_PER_SECOND);
    while world.verdict().is_none() {
        assert!(
            world.tick_count() < since + HOLD_TICKS,
            "not decided in time"
        );
        world.tick(&[]);
    }
    let decided = Verdict {
        holder: 0,
        tick: since + HOLD_TICKS,
        by: Win::Field,
    };
    assert_eq!(world.verdict(), Some(decided));
    // A new army arriving later doesn't undo it.
    world.tick(&[spawn(1, 44)]);
    world.tick(&[]);
    assert_eq!(world.verdict(), Some(decided));
}
