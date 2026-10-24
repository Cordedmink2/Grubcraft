//! Parties past [`MAX_PARTIES`] would share combat and sight slots with
//! parties 0 to 7 (walking through their enemies, seen by nobody's rules),
//! so the sim refuses regiments for them.

use gc_fixed::{Angle, FxVec2};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{Capacities, Command, MAX_PARTIES, MatchSetup, PlayerCommand, Standing, World};

fn world() -> World {
    World::new(&MatchSetup {
        seed: 1,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 64,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    })
}

#[test]
fn no_regiments_for_parties_past_the_limit() {
    let mut w = world();
    let spawn = |player| PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::from_ints(32, 32),
            facing: Angle(0),
        },
    };
    let last = (MAX_PARTIES - 1) as u8;
    let stats = w.tick(&[spawn(last), spawn(last + 1)]);
    assert_eq!((stats.commands_applied, stats.commands_rejected), (1, 1));
    let owners: Vec<u8> = w.regiments.iter().map(|r| r.owner).collect();
    assert_eq!(owners, [last]);
}

#[test]
fn no_standing_regiments_for_parties_past_the_limit() {
    let w = world();
    let mut scratch = w.scratch(4, 0);
    let standing = |owner| Standing {
        owner,
        unit: 0,
        position: FxVec2::from_ints(32, 32),
        facing: Angle(0),
        files: 8,
        formation: gc_sim::Formation::Line,
        models: 10,
        morale_points: 60,
        morale: gc_sim::MoraleState::Steady,
        hidden: false,
    };
    assert!(scratch.spawn_standing(&standing(0), None).is_some());
    assert!(
        scratch
            .spawn_standing(&standing(MAX_PARTIES as u8), None)
            .is_none()
    );
}
