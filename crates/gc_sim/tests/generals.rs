//! The commander on the field (docs/37-commander.md): each side's general
//! rides with its best fighting regiment, steadies the regiments near him,
//! speaks before the lines meet, and when he falls his side panics.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::generals::{PANIC, PANIC_FAR, PICK_DELAY, SPEECH, SPEECH_SECONDS};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, General, MatchSetup, PlayerCommand, RegimentHandle, TICKS_PER_SECOND,
    World,
};

const SOUTH: Angle = Angle(0x4000);
const SPEARS: u16 = 0;
const SKITTERS: u16 = 1;
const ARCHERS: u16 = 2;
const GUARDS: u16 = 3;

fn world() -> World {
    world_with(&DRILL_UNITS)
}

fn world_with(units: &[gc_sim::UnitStats]) -> World {
    World::new(&MatchSetup {
        seed: 3,
        capacities: Capacities { regiments: 32 },
        units,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        generals: true,
    })
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

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

fn general(w: &World, party: u8) -> Option<General> {
    w.generals().find(|g| g.party == party).copied()
}

fn points(w: &World, h: RegimentHandle) -> u8 {
    w.regiments.get(h).unwrap().morale_points
}

#[test]
fn the_general_rides_with_the_best_fighting_regiment_behind_its_front_rank() {
    let mut w = world();
    spawn(&mut w, 0, SPEARS, 60, 60, NORTH);
    spawn(&mut w, 0, ARCHERS, 80, 60, NORTH);
    let guards = spawn(&mut w, 0, GUARDS, 100, 60, NORTH);
    spawn(&mut w, 0, SKITTERS, 120, 60, NORTH);
    assert!(general(&w, 0).is_none(), "not before a second has passed");
    run(&mut w, PICK_DELAY);
    let g = general(&w, 0).expect("a general");
    assert_eq!(g.regiment, guards);
    // Ten files of twenty: the centre of the second rank.
    assert_eq!(w.regiments.get(guards).unwrap().general, Some(15));
    // Better armoured than his men.
    let hp = w.regiments.model_hp(guards).unwrap();
    let man = DRILL_UNITS[GUARDS as usize].hp_per_model;
    assert_eq!(hp[15], man * 3);
    assert!(hp.iter().enumerate().all(|(k, &h)| k == 15 || h == man));
    assert!(g.spoke.is_none() && g.fell.is_none());
    // Nobody else's side has one, and he is never replaced by another.
    assert!(general(&w, 1).is_none());
    spawn(&mut w, 0, GUARDS, 140, 60, NORTH);
    run(&mut w, 2 * PICK_DELAY);
    assert_eq!(general(&w, 0).unwrap().regiment, guards);
}

#[test]
fn he_speaks_once_an_enemy_comes_in_sight_and_lifts_the_regiments_near_him() {
    let mut w = world();
    let guards = spawn(&mut w, 0, GUARDS, 100, 100, NORTH);
    let near = spawn(&mut w, 0, SPEARS, 112, 100, NORTH);
    let far = spawn(&mut w, 0, SPEARS, 160, 100, NORTH);
    run(&mut w, PICK_DELAY);
    let (fresh_near, fresh_far) = (points(&w, near), points(&w, far));
    run(&mut w, 3 * TICKS_PER_SECOND);
    assert!(
        general(&w, 0).unwrap().spoke.is_none(),
        "nobody to speak of yet"
    );
    // An enemy comes within sight.
    spawn(&mut w, 1, SPEARS, 100, 65, SOUTH);
    run(&mut w, 2 * TICKS_PER_SECOND);
    let g = general(&w, 0).unwrap();
    assert!(g.spoke.is_some());
    assert_eq!(points(&w, near), fresh_near + SPEECH as u8);
    assert!(points(&w, guards) > DRILL_UNITS[GUARDS as usize].morale);
    assert_eq!(points(&w, far), fresh_far, "too far off to hear him");
    // The lift lasts its minute, then the ceiling falls back.
    run(&mut w, SPEECH_SECONDS * TICKS_PER_SECOND / 2);
    assert_eq!(points(&w, near), fresh_near + SPEECH as u8);
    run(&mut w, SPEECH_SECONDS * TICKS_PER_SECOND);
    assert_eq!(points(&w, near), fresh_near);
}

#[test]
fn when_he_falls_his_side_panics_most_near_him() {
    let mut w = world();
    let guards = spawn(&mut w, 0, GUARDS, 100, 100, NORTH);
    let near = spawn(&mut w, 0, SPEARS, 112, 100, NORTH);
    let far = spawn(&mut w, 0, SPEARS, 200, 100, NORTH);
    let enemy = spawn(&mut w, 1, SPEARS, 30, 200, NORTH);
    run(&mut w, PICK_DELAY + TICKS_PER_SECOND);
    let (before_near, before_far, before_enemy) =
        (points(&w, near), points(&w, far), points(&w, enemy));
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::RemoveRegiment { regiment: guards },
    }]);
    w.tick(&[]);
    let g = general(&w, 0).unwrap();
    assert!(g.fell.is_some());
    assert_eq!(points(&w, near), before_near - PANIC as u8);
    assert_eq!(points(&w, far), before_far - PANIC_FAR as u8);
    assert_eq!(points(&w, enemy), before_enemy, "it wasn't fighting him");
    // Nobody leads them now, and nobody takes his place.
    run(&mut w, 3 * PICK_DELAY);
    assert_eq!(general(&w, 0).unwrap().regiment, guards);
}

#[test]
fn he_falls_only_once_his_regiment_is_cut_down_to_him() {
    // A guard that won't break, against fiercer blocks: it is cut down.
    let mut units = DRILL_UNITS;
    units[GUARDS as usize].morale = 100;
    units[GUARDS as usize].melee_defence = 0;
    units[SKITTERS as usize].morale = 100;
    let mut w = world_with(&units);
    let guards = spawn(&mut w, 0, GUARDS, 100, 100, NORTH);
    let foes = [
        spawn(&mut w, 1, SKITTERS, 100, 90, SOUTH),
        spawn(&mut w, 1, SKITTERS, 112, 90, SOUTH),
    ];
    w.tick(&foes.map(|regiment| PlayerCommand {
        player: 1,
        command: Command::Attack {
            regiment,
            target: guards,
        },
    }));
    let mut standing = DRILL_UNITS[GUARDS as usize].models;
    for _ in 0..90 * TICKS_PER_SECOND {
        w.tick(&[]);
        if general(&w, 0).is_some_and(|g| g.fell.is_some()) {
            break;
        }
        standing = w.regiments.get(guards).map_or(0, |r| r.models);
    }
    assert!(
        general(&w, 0).unwrap().fell.is_some(),
        "the general never fell: {:?}",
        w.regiments.get(guards)
    );
    assert!(w.regiments.get(guards).is_none_or(|r| r.general.is_none()));
    // He stood behind the front rank: only once he was the last man left
    // behind it could a gap pull him into the fight.
    let place = 10 + 10 / 2;
    assert!(standing <= place + 1, "{standing} men still stood");
}

#[test]
fn a_copy_of_the_field_keeps_the_generals_and_plays_on_alike() {
    let mut w = world();
    spawn(&mut w, 0, GUARDS, 100, 100, NORTH);
    spawn(&mut w, 0, SPEARS, 112, 100, NORTH);
    spawn(&mut w, 1, GUARDS, 100, 30, SOUTH);
    run(&mut w, PICK_DELAY / 2);
    // Copied before the generals are picked, and after.
    for _ in 0..2 {
        let mut copy = w.fork_region(FxVec2::from_ints(100, 70), Fx::from_int(200));
        for _ in 0..10 * TICKS_PER_SECOND {
            w.tick(&[]);
            copy.tick(&[]);
            assert_eq!(
                w.generals().copied().collect::<Vec<_>>(),
                copy.generals().copied().collect::<Vec<_>>()
            );
            assert_eq!(
                w.regiments
                    .iter()
                    .map(|r| r.morale_points)
                    .collect::<Vec<_>>(),
                copy.regiments
                    .iter()
                    .map(|r| r.morale_points)
                    .collect::<Vec<_>>()
            );
        }
    }
    // A copy without the general's regiment has no general for that side,
    // and none takes his place.
    let copy = {
        let mut c = w.fork_region(FxVec2::from_ints(100, 30), Fx::from_int(5));
        run(&mut c, 3 * PICK_DELAY);
        c
    };
    assert!(copy.generals().all(|g| g.party == 1));
}

/// The drill units with spears that won't break and, as unit 4, undead
/// spears that never rout (morale 0).
fn with_undead() -> Vec<gc_sim::UnitStats> {
    let mut units = DRILL_UNITS.to_vec();
    units[SPEARS as usize].morale = 100;
    let mut undead = DRILL_UNITS[SPEARS as usize];
    undead.morale = 0;
    units.push(undead);
    units
}
const UNDEAD: u16 = 4;

fn hp(w: &World, h: RegimentHandle) -> u32 {
    w.regiments
        .model_hp(h)
        .map_or(0, |hp| hp.iter().map(|&h| u32::from(h)).sum())
}

/// What a block of undead deals a foe in eight seconds, with their general
/// standing at `x`, `y`.
fn undead_fight(x: i32, y: i32) -> u32 {
    let mut w = world_with(&with_undead());
    let guards = spawn(&mut w, 0, GUARDS, x, y, NORTH);
    let undead = spawn(&mut w, 0, UNDEAD, 112, 100, NORTH);
    let foe = spawn(&mut w, 1, SPEARS, 112, 92, SOUTH);
    run(&mut w, PICK_DELAY);
    assert_eq!(general(&w, 0).unwrap().regiment, guards);
    let before = hp(&w, foe);
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::Attack {
            regiment: undead,
            target: foe,
        },
    }]);
    run(&mut w, 8 * TICKS_PER_SECOND);
    before - hp(&w, foe)
}

#[test]
fn undead_near_him_strike_harder() {
    // His guard just behind them, out of the fight, or far off.
    let (near, far) = (undead_fight(112, 108), undead_fight(30, 220));
    assert!(far > 0, "they never fought");
    // A tenth harder, give or take which blows landed.
    assert!(near * 100 >= far * 105, "near {near} vs far {far}");
}

#[test]
fn when_he_falls_the_undead_near_him_falter() {
    let mut w = world_with(&with_undead());
    let guards = spawn(&mut w, 0, GUARDS, 100, 100, NORTH);
    let near = spawn(&mut w, 0, UNDEAD, 112, 100, NORTH);
    let far = spawn(&mut w, 0, UNDEAD, 200, 100, NORTH);
    run(&mut w, PICK_DELAY + TICKS_PER_SECOND);
    assert_eq!(general(&w, 0).unwrap().regiment, guards);
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::RemoveRegiment { regiment: guards },
    }]);
    w.tick(&[]);
    assert!(general(&w, 0).unwrap().fell.is_some());
    assert!(w.regiments.get(near).unwrap().disorder > 0);
    assert_eq!(w.regiments.get(far).unwrap().disorder, 0);
}
