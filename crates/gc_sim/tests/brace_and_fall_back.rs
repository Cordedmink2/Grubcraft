//! Brace and Fall back (docs/05-combat.md): spears set against a charge
//! turn it back on the charger, and a regiment can pull out of a fight in
//! good order.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, MatchSetup, PlayerCommand, RegimentHandle, RegimentView, UnitStats, World,
};

const SOUTH: Angle = Angle::QUARTER;
const WEST: Angle = Angle::HALF;
/// Drill unit 0: 24 spearmen in 8 files, given long weapons here.
const SPEARS: u16 = 0;
/// Drill unit 1: 24 fast chargers (charge bonus 40).
const CHARGERS: u16 = 1;
/// Drill unit 0 as it is, without long weapons.
const PLAIN: u16 = 3;

/// The drill's units with morale off (nobody routs, so the numbers are the
/// blows alone), unit 0 with long weapons and unit 3 a plain copy of it.
fn units() -> [UnitStats; 4] {
    let mut units = DRILL_UNITS.map(|u| UnitStats { morale: 0, ..u });
    units[PLAIN as usize] = units[SPEARS as usize];
    units[SPEARS as usize].reach = true;
    units
}

fn world(units: &[UnitStats]) -> World {
    World::new(&MatchSetup {
        seed: 31,
        capacities: Capacities::default(),
        units,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        // Generals off: these rules count wounds, and a general's extra hit
        // points would muddle the count (docs/37-commander.md).
        generals: false,
    })
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing,
        },
    ));
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn brace(w: &mut World, player: u8, regiment: RegimentHandle, on: bool) -> bool {
    order(w, player, Command::Brace { regiment, on })
}

fn fall_back(w: &mut World, player: u8, regiment: RegimentHandle) -> bool {
    order(w, player, Command::FallBack { regiment })
}

/// March `regiment` to (`x`, `y`) at `pace` (zero for its own speed).
fn march(w: &mut World, player: u8, regiment: RegimentHandle, x: i32, y: i32, pace: Fx) -> bool {
    order(
        w,
        player,
        Command::MoveTo {
            regiment,
            target: FxVec2::from_ints(x, y),
            facing: None,
            files: 0,
            pace,
        },
    )
}

fn view(w: &World, h: RegimentHandle) -> RegimentView {
    w.regiments.get(h).unwrap()
}

/// Hit points a regiment has left.
fn hp(w: &World, h: RegimentHandle) -> u32 {
    w.regiments
        .model_hp(h)
        .map_or(0, |hp| hp.iter().copied().map(u32::from).sum())
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

/// Tick until `regiment` is in melee (at most 20 seconds).
fn until_engaged(w: &mut World, regiment: RegimentHandle) {
    for _ in 0..20 * 20 {
        w.tick(&[]);
        if view(w, regiment).engaged {
            return;
        }
    }
    panic!("the regiments never met");
}

fn distance(a: FxVec2, b: FxVec2) -> f32 {
    (a - b).length().raw() as f32 / Fx::ONE.raw() as f32
}

/// Spearmen stand facing north at (100, 100), braced or not; chargers run
/// at them from `from`, facing `facing`, from far enough off for a
/// run-up. Returns the world and both handles on the tick they meet.
fn charge_into(
    braced: bool,
    ((x, y), facing): ((i32, i32), Angle),
) -> (World, RegimentHandle, RegimentHandle) {
    let units = units();
    let mut w = world(&units);
    let spears = spawn(&mut w, 0, SPEARS, 100, 100, NORTH);
    let chargers = spawn(&mut w, 1, CHARGERS, x, y, facing);
    if braced {
        assert!(brace(&mut w, 0, spears, true));
    }
    assert!(order(
        &mut w,
        1,
        Command::Attack {
            regiment: chargers,
            target: spears,
        },
    ));
    until_engaged(&mut w, chargers);
    (w, spears, chargers)
}

/// In front of the spearmen, and off their right flank.
const AHEAD: ((i32, i32), Angle) = ((100, 60), SOUTH);
const RIGHT: ((i32, i32), Angle) = ((140, 100), WEST);

/// Hit points each side loses in the three seconds a charge lasts.
fn charge_losses(braced: bool, from: ((i32, i32), Angle)) -> (u32, u32) {
    let (mut w, spears, chargers) = charge_into(braced, from);
    let (s, c) = (hp(&w, spears), hp(&w, chargers));
    run(&mut w, 3 * 20);
    (s - hp(&w, spears), c - hp(&w, chargers))
}

#[test]
fn only_long_weapons_brace() {
    let units = units();
    let mut w = world(&units);
    let spears = spawn(&mut w, 0, SPEARS, 100, 100, NORTH);
    let plain = spawn(&mut w, 0, PLAIN, 130, 100, NORTH);
    assert!(!brace(&mut w, 0, plain, true));
    assert!(!view(&w, plain).braced);
    // Nor anyone else's.
    assert!(!brace(&mut w, 1, spears, true));
    assert!(brace(&mut w, 0, spears, true));
    assert!(view(&w, spears).braced);
    assert!(brace(&mut w, 0, spears, false));
    assert!(!view(&w, spears).braced);
}

#[test]
fn bracing_halts_and_any_order_lifts_it() {
    let units = units();
    let mut w = world(&units);
    let spears = spawn(&mut w, 0, SPEARS, 100, 100, NORTH);
    assert!(march(&mut w, 0, spears, 100, 60, Fx::ZERO));
    run(&mut w, 20);
    assert!(brace(&mut w, 0, spears, true));
    let at = view(&w, spears).position;
    run(&mut w, 40);
    assert_eq!(view(&w, spears).position, at, "a braced block stands");
    assert!(march(&mut w, 0, spears, 100, 60, Fx::ZERO));
    assert!(!view(&w, spears).braced);
    run(&mut w, 20);
    assert_ne!(view(&w, spears).position, at);
}

#[test]
fn a_charge_onto_braced_points_is_turned_back() {
    let (w, spears, chargers) = charge_into(true, AHEAD);
    let (s, c) = (view(&w, spears), view(&w, chargers));
    assert_eq!(c.charge, 0, "the charge is spent on the points");
    assert!(c.disorder > 0, "the chargers' ranks are thrown");
    assert!(s.charge > 0, "the spears take the charge's weight");
    assert!(s.braced, "and stay braced while they fight");

    let (open_spears, open_chargers) = charge_losses(false, AHEAD);
    let (braced_spears, braced_chargers) = charge_losses(true, AHEAD);
    assert!(
        braced_spears * 2 < open_spears,
        "spears lost {braced_spears} braced, {open_spears} not"
    );
    assert!(
        braced_chargers * 4 > open_chargers * 5,
        "chargers lost {braced_chargers} on braced spears, {open_chargers} on open ones"
    );
}

#[test]
fn a_braced_flank_is_no_better_than_any_other() {
    let (w, _, chargers) = charge_into(true, RIGHT);
    let c = view(&w, chargers);
    assert!(c.charge > 0, "a charge into the flank hits home");
    assert_eq!(c.disorder, 0);
}

#[test]
fn spears_set_only_out_of_the_fight() {
    let (mut w, spears, _) = charge_into(false, AHEAD);
    assert!(!brace(&mut w, 0, spears, true));
}

/// Plain spearmen facing north at (100, 100), locked in melee with the
/// chargers, who walked in from the north. Returns the world and handles.
fn locked() -> (World, RegimentHandle, RegimentHandle) {
    let units = units();
    let mut w = world(&units);
    let mine = spawn(&mut w, 0, PLAIN, 100, 100, NORTH);
    let foe = spawn(&mut w, 1, CHARGERS, 100, 90, SOUTH);
    assert!(march(&mut w, 1, foe, 100, 100, Fx::ONE));
    until_engaged(&mut w, mine);
    run(&mut w, 3 * 20);
    assert!(view(&w, mine).engaged && view(&w, foe).engaged);
    (w, mine, foe)
}

#[test]
fn falling_back_pulls_out_of_the_fight_in_good_order() {
    let (mut w, mine, foe) = locked();
    let (start, facing) = (view(&w, mine).position, view(&w, mine).facing);
    let foe_at = view(&w, foe).position;
    assert!(!fall_back(&mut w, 1, mine), "only its owner can order it");
    let my_hp = hp(&w, mine);
    // The parting blow: the foe strikes as it pulls out; it strikes back no more.
    assert!(fall_back(&mut w, 0, mine));
    assert!(hp(&w, mine) < my_hp, "no parting blow");
    let foe_hp = hp(&w, foe);
    let v = view(&w, mine);
    assert!(v.falling_back && !v.engaged);
    run(&mut w, 6 * 20);
    let v = view(&w, mine);
    assert_eq!(hp(&w, foe), foe_hp, "it struck back while falling back");
    assert_eq!(v.facing, facing, "it backs away facing the foe");
    assert!(!v.engaged);
    let away = distance(v.position, start);
    assert!(
        (7.0..=8.5).contains(&away),
        "fell back {away} tiles, not about 8"
    );
    assert!(
        distance(v.position, foe_at) > distance(start, foe_at) + 7.0,
        "it fell back toward the foe"
    );
    // The foe lets it go.
    let f = view(&w, foe);
    assert!(!f.engaged);
    assert!(distance(f.position, foe_at) < 1.0, "the foe went after it");
    run(&mut w, 3 * 20);
    assert!(!view(&w, mine).falling_back, "still falling back after 9 s");
}

#[test]
fn a_foe_sent_after_it_waits_until_it_stands() {
    let (mut w, mine, foe) = locked();
    assert!(fall_back(&mut w, 0, mine));
    run(&mut w, 20);
    assert!(order(
        &mut w,
        1,
        Command::Attack {
            regiment: foe,
            target: mine,
        },
    ));
    // It can't lock the front of a block backing away from it, and waits
    // there rather than walking into it ...
    run(&mut w, 3 * 20);
    assert!(view(&w, mine).falling_back);
    assert!(!view(&w, foe).engaged);
    let apart = distance(view(&w, mine).position, view(&w, foe).position);
    assert!(
        apart > 2.0,
        "the foe walked into the block ({apart} tiles apart)"
    );
    assert!(
        !fall_back(&mut w, 0, mine),
        "a second fall back does nothing"
    );
    // ... only once it has finished falling back.
    run(&mut w, 6 * 20);
    assert!(!view(&w, mine).falling_back);
    assert!(view(&w, mine).engaged && view(&w, foe).engaged);
}

#[test]
fn falling_back_is_caught_from_the_flank() {
    let (mut w, mine, _) = locked();
    let flanker = spawn(&mut w, 1, PLAIN, 125, 104, WEST);
    assert!(fall_back(&mut w, 0, mine));
    assert!(order(
        &mut w,
        1,
        Command::Attack {
            regiment: flanker,
            target: mine,
        },
    ));
    until_engaged(&mut w, mine);
    let v = view(&w, mine);
    assert!(!v.falling_back, "caught, it stops falling back");
}

#[test]
fn falling_back_lifts_a_brace() {
    let units = units();
    let mut w = world(&units);
    let spears = spawn(&mut w, 0, SPEARS, 100, 100, NORTH);
    assert!(brace(&mut w, 0, spears, true));
    assert!(fall_back(&mut w, 0, spears));
    let v = view(&w, spears);
    assert!(v.falling_back && !v.braced);
    assert!(
        !brace(&mut w, 0, spears, true),
        "spears can't be set while backing away"
    );
}
