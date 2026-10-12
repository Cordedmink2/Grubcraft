//! How regiments march, turn and re-form under orders.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, MatchSetup, PlayerCommand, RegimentHandle, RegimentView, TICKS_PER_SECOND,
    World,
};

const EAST: Angle = Angle(0);
const SOUTH: Angle = Angle(0x4000);

/// A world with one regiment of drill unit 0 (24 soldiers, 8 files, 3 tiles/s,
/// 60 degrees/s) at (50, 50) facing north.
fn one_regiment() -> (World, RegimentHandle) {
    let mut w = World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        teams: &[],
    });
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::from_ints(50, 50),
            facing: NORTH,
        },
    }]);
    let h = w.regiments.iter().next().unwrap().handle;
    (w, h)
}

fn order(
    w: &mut World,
    regiment: RegimentHandle,
    target: FxVec2,
    facing: Option<Angle>,
    files: u16,
) {
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::MoveTo {
            regiment,
            target,
            facing,
            files,
            pace: Fx::ZERO,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
}

fn view(w: &World, h: RegimentHandle) -> RegimentView {
    w.regiments.get(h).unwrap()
}

fn run_until_still(w: &mut World, h: RegimentHandle, max_ticks: u32) -> u32 {
    for t in 0..max_ticks {
        if !view(w, h).moving {
            return t;
        }
        w.tick(&[]);
    }
    panic!("still moving after {max_ticks} ticks: {:?}", view(w, h));
}

#[test]
fn spawns_at_full_strength_in_default_formation() {
    let (w, h) = one_regiment();
    let r = view(&w, h);
    assert_eq!((r.files, r.models, r.ranks()), (8, 24, 3));
    assert_eq!(r.facing, NORTH);
    let hp = w.regiments.model_hp(h).unwrap();
    assert_eq!(hp.len(), 24);
    assert!(hp.iter().all(|&x| x == 10));
}

#[test]
fn turns_on_the_spot_before_marching_backwards() {
    let (mut w, h) = one_regiment();
    // Due south, 30 tiles: a half turn first.
    order(&mut w, h, FxVec2::from_ints(50, 80), None, 0);
    for _ in 0..10 {
        w.tick(&[]);
    }
    let r = view(&w, h);
    assert_eq!(
        r.position,
        FxVec2::from_ints(50, 50),
        "marched before turning"
    );
    assert_ne!(r.facing, NORTH, "did not start turning");
    let ticks = run_until_still(&mut w, h, 2_000);
    let r = view(&w, h);
    assert_eq!(r.position, FxVec2::from_ints(50, 80));
    assert!(r.facing.delta_to(SOUTH).abs() <= 2, "{:?}", r.facing);
    // 180 degrees at 60/s is 3 s of turning (not counting the last 45 while
    // walking), then 10 s of marching.
    let secs = ticks / TICKS_PER_SECOND;
    assert!((11..=14).contains(&secs), "took {secs} s");
}

#[test]
fn faces_the_ordered_direction_on_arrival() {
    let (mut w, h) = one_regiment();
    order(&mut w, h, FxVec2::from_ints(50, 30), Some(EAST), 0);
    run_until_still(&mut w, h, 2_000);
    let r = view(&w, h);
    assert_eq!(r.position, FxVec2::from_ints(50, 30));
    assert_eq!(r.facing, EAST);
}

#[test]
fn short_adjustments_shuffle_without_wheeling_round() {
    let (mut w, h) = one_regiment();
    // Step back two tiles, keep facing north.
    order(&mut w, h, FxVec2::from_ints(50, 52), Some(NORTH), 0);
    for _ in 0..200 {
        w.tick(&[]);
        assert_eq!(view(&w, h).facing, NORTH);
    }
    assert_eq!(view(&w, h).position, FxVec2::from_ints(50, 52));
    assert!(!view(&w, h).moving);
}

#[test]
fn frontage_changes_and_is_clamped() {
    let (mut w, h) = one_regiment();
    order(&mut w, h, FxVec2::from_ints(50, 50), None, 12);
    assert_eq!((view(&w, h).files, view(&w, h).ranks()), (12, 2));
    order(&mut w, h, FxVec2::from_ints(50, 50), None, 500);
    assert_eq!((view(&w, h).files, view(&w, h).ranks()), (24, 1));
    order(&mut w, h, FxVec2::from_ints(50, 50), None, 0);
    assert_eq!(view(&w, h).files, 24, "0 must keep the frontage");
}

#[test]
fn pace_caps_speed_but_never_raises_it() {
    let (mut w, h) = one_regiment();
    let start = view(&w, h).position;
    let target = FxVec2::from_ints(50, 0);
    let send = |w: &mut World, pace| {
        w.tick(&[PlayerCommand {
            player: 0,
            command: Command::MoveTo {
                regiment: h,
                target,
                facing: None,
                files: 0,
                pace,
            },
        }])
    };
    // Already facing north, so it marches at once: 1 tile/s for one tick.
    send(&mut w, Fx::ONE);
    let moved = start.y - view(&w, h).position.y;
    assert_eq!(moved, Fx::from_raw(Fx::ONE.raw() / 20));
    // Asking for 100 tiles/s gives its own 3 tiles/s.
    let before = view(&w, h).position;
    send(&mut w, Fx::from_int(100));
    let moved = before.y - view(&w, h).position.y;
    assert_eq!(moved, Fx::from_raw(Fx::from_int(3).raw() / 20));
}
