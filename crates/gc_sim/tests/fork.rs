//! Copies of a battle (engine hooks 5 and 6, docs/18-ai.md): scratch worlds
//! on the same map, regiments put on the field as they stand, and copies of
//! an area that play on exactly like the original.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, Drill, NORTH, ribs_map};
use gc_sim::{
    Capacities, Command, Formation, MatchSetup, MoraleState, PlayerCommand, RegimentHandle,
    RegimentView, Standing, World,
};

const SOUTH: Angle = Angle(0x4000);

fn world_on(nav: &[u8]) -> World {
    World::new(&MatchSetup {
        seed: 31,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        generals: true,
    })
}

/// An empty world to set regiments on directly, as the computer players'
/// rehearsals do.
fn scratch_on(nav: &[u8]) -> World {
    world_on(nav).scratch(64, 1)
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing,
        },
    }]);
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

/// Everything public about every regiment, with its soldiers' hit points
/// and the route ahead of it.
fn field(w: &World) -> Vec<(RegimentView, Vec<u16>, Vec<FxVec2>)> {
    w.regiments
        .iter()
        .map(|r| {
            let hp = w.regiments.model_hp(r.handle).unwrap().to_vec();
            (r, hp, w.route(r.handle).to_vec())
        })
        .collect()
}

/// The four-army drill on the ridged map (no archers, whose arrows in the
/// air a copy leaves out), forked part way through: the copy plays on
/// exactly as the original does, given the same orders.
#[test]
fn a_copy_of_the_whole_field_plays_on_exactly_like_the_original() {
    let nav = ribs_map(256);
    let mut world = world_on(&nav);
    let mut drill = Drill::new(7);
    drill.regiments_per_player = 20;
    drill.unit_of = |p, _, _| [0, 1, 3, 0][p as usize % 4];
    let mut commands = Vec::new();
    for _ in 0..300 {
        drill.commands_for(&world, &mut commands);
        world.tick(&commands);
    }
    let everywhere = Fx::from_int(1000);
    let mut copy = world.fork_region(FxVec2::from_ints(128, 128), everywhere);
    assert_eq!(copy.tick_count(), world.tick_count());
    assert_eq!(field(&copy), field(&world));
    let mut fights = 0;
    for t in 0..600 {
        drill.commands_for(&world, &mut commands);
        world.tick(&commands);
        copy.tick(&commands);
        if t % 20 == 0 {
            assert_eq!(field(&copy), field(&world), "after {t} ticks");
        }
        fights += world.regiments.iter().filter(|r| r.engaged).count();
    }
    assert_eq!(field(&copy), field(&world));
    assert!(fights > 0, "nobody fought, so the test shows little");
}

#[test]
fn a_copy_of_an_area_holds_only_the_regiments_in_it() {
    let mut world = world_on(&[]);
    let near = spawn(&mut world, 0, 0, 100, 100, NORTH);
    let edge = spawn(&mut world, 1, 0, 100, 120, SOUTH);
    let far = spawn(&mut world, 1, 0, 200, 100, SOUTH);
    let copy = world.fork_region(FxVec2::from_ints(100, 100), Fx::from_int(20));
    let handles: Vec<_> = copy.regiments.iter().map(|r| r.handle).collect();
    assert_eq!(handles, vec![near, edge]);
    assert!(copy.regiments.get(far).is_none());
    assert_eq!(copy.regiments.get(edge), world.regiments.get(edge));
}

#[test]
fn a_scratch_world_shares_the_map() {
    let nav = ribs_map(256);
    let mut world = world_on(&nav);
    for _ in 0..7 {
        world.tick(&[]);
    }
    let scratch = world.scratch(16, 99);
    assert_eq!(scratch.tick_count(), world.tick_count());
    assert_eq!(scratch.regiments.iter().count(), 0);
    assert_eq!(scratch.regiments.capacity(), 16);
    assert_eq!(scratch.nav().portal_count(), world.nav().portal_count());
    for y in (0..256).step_by(5) {
        for x in (0..256).step_by(5) {
            let p = FxVec2::from_ints(x, y);
            assert_eq!(scratch.nav().walkable(0, p), world.nav().walkable(0, p));
        }
    }
    // The same order finds the same route in both.
    let mut a = world.scratch(4, 1);
    let mut b = world_on(&nav).scratch(4, 1);
    let standing = Standing {
        owner: 0,
        unit: 0,
        position: FxVec2::from_ints(20, 20),
        facing: NORTH,
        files: 8,
        formation: Formation::Line,
        models: 24,
        morale_points: 60,
        morale: MoraleState::Steady,
        hidden: false,
    };
    let ha = a.spawn_standing(&standing, None).unwrap();
    let hb = b.spawn_standing(&standing, None).unwrap();
    let go = |h| PlayerCommand {
        player: 0,
        command: Command::MoveTo {
            regiment: h,
            target: FxVec2::from_ints(230, 230),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    };
    a.tick(&[go(ha)]);
    b.tick(&[go(hb)]);
    for _ in 0..5 {
        a.tick(&[]);
        b.tick(&[]);
    }
    assert!(a.route(ha).len() > 1, "{:?}", a.route(ha));
    assert_eq!(a.route(ha), b.route(hb));
}

#[test]
fn a_regiment_goes_on_the_field_as_it_stands() {
    let mut world = scratch_on(&[]);
    let s = Standing {
        owner: 1,
        unit: 0,
        position: FxVec2::from_ints(50, 60),
        facing: SOUTH,
        files: 5,
        formation: Formation::Line,
        models: 10,
        morale_points: 20,
        morale: MoraleState::Wavering,
        hidden: false,
    };
    let h = world.spawn_standing(&s, None).unwrap();
    let r = world.regiments.get(h).unwrap();
    assert_eq!(Standing::of(&r, false), s);
    assert!(!r.moving && !r.engaged);
    assert_eq!(world.regiments.model_hp(h).unwrap(), &[10; 10]);

    // Into a chosen slot, keeping a handle from another world.
    let wanted = RegimentHandle {
        index: 9,
        generation: 4,
    };
    let mut other = scratch_on(&[]);
    assert_eq!(other.spawn_standing(&s, Some(wanted)), Some(wanted));
    assert_eq!(other.spawn_standing(&s, Some(wanted)), None, "taken");
    assert_eq!(
        Standing::of(&other.regiments.get(wanted).unwrap(), false),
        s
    );
    // The next ordinary spawn still takes the first free slot.
    let next = other.spawn_standing(&s, None).unwrap();
    assert_eq!(next.index, 0);

    // Nothing to put on the field, or no such regiment type.
    assert_eq!(
        world.spawn_standing(&Standing { models: 0, ..s }, None),
        None
    );
    assert_eq!(
        world.spawn_standing(&Standing { unit: 99, ..s }, None),
        None
    );
    // More soldiers than the unit has, or a wider frontage than soldiers.
    let big = world
        .spawn_standing(
            &Standing {
                models: 500,
                files: 900,
                formation: Formation::Line,
                ..s
            },
            None,
        )
        .unwrap();
    let r = world.regiments.get(big).unwrap();
    assert_eq!((r.models, r.files), (24, 24));
}

/// A wavering regiment put on the field mid-fight routs sooner than a
/// fresh one would, and a routing one runs.
#[test]
fn morale_carries_over() {
    let fight = |points: u8, state: MoraleState| {
        let mut world = scratch_on(&[]);
        let us = Standing {
            owner: 0,
            unit: 0,
            position: FxVec2::from_ints(100, 100),
            facing: NORTH,
            files: 8,
            formation: Formation::Line,
            models: 24,
            morale_points: points,
            morale: state,
            hidden: false,
        };
        let h = world.spawn_standing(&us, None).unwrap();
        // Two regiments of skitterlings at its front and flank.
        for (x, y, f) in [(100, 96, SOUTH), (106, 100, Angle(0x8000))] {
            let them = Standing {
                owner: 1,
                unit: 1,
                position: FxVec2::from_ints(x, y),
                facing: f,
                ..us
            };
            let t = world.spawn_standing(&them, None).unwrap();
            world.tick(&[PlayerCommand {
                player: 1,
                command: Command::Attack {
                    regiment: t,
                    target: h,
                },
            }]);
        }
        for t in 0..600 {
            if world.regiments.get(h).unwrap().morale.broken() {
                return t;
            }
            world.tick(&[]);
        }
        600
    };
    let fresh = fight(60, MoraleState::Steady);
    let shaken = fight(20, MoraleState::Wavering);
    assert!(
        shaken < fresh,
        "wavering broke after {shaken}, fresh after {fresh}"
    );
    let mut world = scratch_on(&[]);
    let routing = Standing {
        owner: 0,
        unit: 0,
        position: FxVec2::from_ints(100, 100),
        facing: NORTH,
        files: 8,
        formation: Formation::Line,
        models: 24,
        morale_points: 0,
        morale: MoraleState::Routing,
        hidden: false,
    };
    let h = world.spawn_standing(&routing, None).unwrap();
    world
        .spawn_standing(
            &Standing {
                owner: 1,
                unit: 1,
                position: FxVec2::from_ints(100, 92),
                facing: SOUTH,
                ..routing
            },
            None,
        )
        .unwrap();
    // It turns about, then runs.
    for _ in 0..200 {
        world.tick(&[]);
    }
    let r = world.regiments.get(h).unwrap();
    assert!(r.morale.broken());
    assert!(
        r.position.y > FxVec2::from_ints(100, 104).y,
        "a routing regiment runs from the enemy: {:?}",
        r.position
    );
}

/// A match's own world changes only by its players' commands (review fix
/// R21): it refuses regiments put straight on the field.
#[test]
fn a_match_world_takes_no_regiments_from_outside() {
    let mut world = world_on(&[]);
    let s = Standing {
        owner: 0,
        unit: 0,
        position: FxVec2::from_ints(50, 60),
        facing: NORTH,
        files: 5,
        formation: Formation::Line,
        models: 10,
        morale_points: 20,
        morale: MoraleState::Steady,
        hidden: false,
    };
    let before = world.state_hash();
    assert_eq!(world.spawn_standing(&s, None), None);
    assert_eq!(world.state_hash(), before);
}
