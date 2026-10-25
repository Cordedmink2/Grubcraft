//! Night battles: the dark hides troops in the open, and torches give them
//! away (docs/32-changing-land.md, stage 3b).

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::sky::at_hour;
use gc_sim::{
    Capacities, Command, Cover, Ground, MatchSetup, PlayerCommand, RegimentHandle, Sky, World,
};

const SOUTH: Angle = Angle(0x4000);
const SPEARS: u16 = 0;
const CAVALRY: u16 = 1;
const ARCHERS: u16 = 2;
const MAP: u32 = 64;
const SECOND: u32 = gc_sim::TICKS_PER_SECOND;

/// Midnight with the clock stopped: dark all match.
const NIGHT: Sky<'static> = Sky {
    start: at_hour(23, 30),
    ..Sky::STILL
};

/// A cover map with woods on the tiles `x0..x1` by `y0..y1` (a map needs
/// some cover for anything to hide).
fn woods(x0: u32, x1: u32, y0: u32, y1: u32) -> Vec<u8> {
    (0..MAP * MAP)
        .map(|i| {
            let (x, y) = (i % MAP, i / MAP);
            if (x0..x1).contains(&x) && (y0..y1).contains(&y) {
                Cover::Wood as u8
            } else {
                Cover::Open as u8
            }
        })
        .collect()
}

/// Cover only in a far corner: the middle of the map is open.
fn open() -> Vec<u8> {
    woods(60, 64, 60, 64)
}

fn world(cover: &[u8], sky: Sky<'static>) -> World {
    World::new(&MatchSetup {
        seed: 1,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: MAP,
        nav: &[],
        teams: &[],
        ground: Ground {
            heights: &[],
            water_level: 0,
            cover,
            growth: &[],
            footing: &[],
            flow: &[],
        },
        champions: false,
        sky,
        generals: false,
    })
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    let stats = w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit,
            position: FxVec2::new(Fx::from_int(x) + Fx::HALF, Fx::from_int(y) + Fx::HALF),
            facing,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn light(w: &mut World, player: u8, regiment: RegimentHandle) -> bool {
    order(
        w,
        player,
        Command::Torches {
            regiment,
            lit: true,
        },
    )
}

fn hp(w: &World, h: RegimentHandle) -> u32 {
    w.regiments
        .model_hp(h)
        .map_or(0, |hp| hp.iter().copied().map(u32::from).sum())
}

#[test]
fn the_dark_hides_troops_in_the_open_until_they_are_close() {
    let cover = open();
    // Twenty tiles apart, standing: seen by day, hidden at night.
    for (sky, seen) in [(Sky::STILL, true), (NIGHT, false)] {
        let mut w = world(&cover, sky);
        assert_eq!(w.air().dark, !seen);
        let a = spawn(&mut w, 0, SPEARS, 20, 32, NORTH);
        let b = spawn(&mut w, 1, SPEARS, 20, 12, SOUTH);
        run(&mut w, 5 * SECOND);
        assert_eq!(w.sees(0, b), seen);
        assert_eq!(w.sees(1, a), seen);
        assert_eq!(w.in_dark(a), !seen);
        // Hidden by the dark is not hiding in cover: both still face each
        // other, and may be ordered against each other.
        assert!(!w.hidden(a));
        assert!(w.faces(0, b));
    }
    // A few tiles apart, they see each other even at night.
    let mut w = world(&cover, NIGHT);
    let a = spawn(&mut w, 0, SPEARS, 20, 32, NORTH);
    let b = spawn(&mut w, 1, SPEARS, 20, 25, SOUTH);
    run(&mut w, 5 * SECOND);
    assert!(w.sees(0, b) && w.sees(1, a));
}

#[test]
fn torches_show_their_bearers_and_the_ground_round_them() {
    let mut w = world(&open(), NIGHT);
    let a = spawn(&mut w, 0, SPEARS, 20, 40, NORTH);
    let b = spawn(&mut w, 1, SPEARS, 20, 10, SOUTH);
    let c = spawn(&mut w, 1, SPEARS, 20, 30, SOUTH);
    run(&mut w, 5 * SECOND);
    assert!(!w.sees(1, a) && !w.sees(0, c));
    assert!(light(&mut w, 0, a));
    run(&mut w, 2);
    assert!(w.regiments.get(a).unwrap().torches);
    // Its bearer is seen from thirty tiles off, and the enemy standing in
    // its light, ten tiles off, is seen by it; the one further off is not.
    assert!(w.sees(1, a));
    assert!(w.sees(0, c));
    assert!(!w.sees(0, b));
    // By day the order is kept, but torches don't burn.
    let mut day = world(&open(), Sky::STILL);
    let a = spawn(&mut day, 0, SPEARS, 20, 40, NORTH);
    assert!(light(&mut day, 0, a));
    assert!(!day.air().dark);
}

#[test]
fn archers_shoot_a_lit_target_from_their_day_range() {
    // At night the archers' 25 tiles shrink to under half: a target 20
    // tiles off is out of reach, unless it carries torches.
    let shot = |lit: bool| {
        let mut w = world(&open(), NIGHT);
        assert!(w.air().sight(DRILL_UNITS[ARCHERS as usize].range) < Fx::from_int(16));
        let archers = spawn(&mut w, 0, ARCHERS, 20, 40, NORTH);
        let target = spawn(&mut w, 1, SPEARS, 20, 20, SOUTH);
        if lit {
            assert!(light(&mut w, 1, target));
        }
        let _ = archers;
        let full = hp(&w, target);
        run(&mut w, 15 * SECOND);
        hp(&w, target) < full
    };
    assert!(!shot(false));
    assert!(shot(true));
}

/// Cavalry charges spears at night, from a wood or out of the dark of
/// the open (sixteen tiles off); `torches` are lit on the spears
/// themselves, on a friend beside them (whose light falls on them), or on
/// neither. Returns whether an ambush was sprung.
fn ambush_at_night(wood: bool, torches: Option<bool>) -> bool {
    let cover = if wood { woods(10, 30, 10, 30) } else { open() };
    let mut w = world(&cover, NIGHT);
    let cav = spawn(&mut w, 1, CAVALRY, 20, if wood { 24 } else { 20 }, SOUTH);
    let spears = spawn(&mut w, 0, SPEARS, 20, 36, NORTH);
    let friend = spawn(&mut w, 0, SPEARS, 30, 40, NORTH);
    match torches {
        Some(true) => assert!(light(&mut w, 0, spears)),
        Some(false) => assert!(light(&mut w, 0, friend)),
        None => {}
    }
    run(&mut w, 5 * SECOND);
    assert!(!w.sees(0, cav));
    assert!(order(
        &mut w,
        1,
        Command::Attack {
            regiment: cav,
            target: spears,
        }
    ));
    for _ in 0..10 * SECOND {
        if !w.ambushes().is_empty() {
            return true;
        }
        w.tick(&[]);
    }
    assert!(w.regiments.get(spears).unwrap().engaged);
    false
}

#[test]
fn torches_guard_their_bearers_against_ambush_out_of_the_dark() {
    // Out of the dark: spears lit by a friend's torches are seen long
    // before the charge, so it takes them by surprise; with torches of
    // their own they see what comes at them; dark, each side stumbles on
    // the other, and nobody is surprised.
    assert!(ambush_at_night(false, Some(false)));
    assert!(!ambush_at_night(false, Some(true)));
    assert!(!ambush_at_night(false, None));
    // A charge out of a wood takes them by surprise, torches or not.
    assert!(ambush_at_night(true, Some(true)));
    assert!(ambush_at_night(true, None));
}

#[test]
fn armies_lost_in_the_dark_still_hold_each_other_in_the_field() {
    let mut w = world(&open(), NIGHT);
    let a = spawn(&mut w, 0, SPEARS, 20, 50, NORTH);
    let b = spawn(&mut w, 1, SPEARS, 20, 10, SOUTH);
    run(&mut w, 90 * SECOND);
    assert!(!w.sees(0, b));
    assert_eq!(w.verdict(), None, "nobody holds the field in the dark");
    // An order against the enemy heard in the dark is taken, and the chase
    // goes on until they meet.
    assert!(order(
        &mut w,
        0,
        Command::Attack {
            regiment: a,
            target: b,
        }
    ));
    run(&mut w, 30 * SECOND);
    assert!(w.regiments.get(a).unwrap().engaged);
}

#[test]
fn only_its_own_side_lights_a_regiments_torches() {
    let mut w = world(&open(), NIGHT);
    let a = spawn(&mut w, 0, SPEARS, 20, 40, NORTH);
    assert!(!light(&mut w, 1, a));
    assert!(light(&mut w, 0, a));
    // Lighting them again changes nothing.
    assert!(!light(&mut w, 0, a));
    assert!(order(
        &mut w,
        0,
        Command::Torches {
            regiment: a,
            lit: false
        }
    ));
}
