//! Weapon room (docs/05-combat.md): spears that fight from two ranks and
//! strike first, swinging weapons that need room, shields, and the Tight,
//! Loose and Shield wall formations.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, Formation, MatchSetup, MoraleState, PlayerCommand, RegimentHandle,
    UnitStats, World,
};

const SOUTH: Angle = Angle(0x4000);
const WEST: Angle = Angle(0x8000);
/// Drill unit 0: 24 spearmen in 8 files, 10 hp each. Here it is the
/// regiment under test (player 0), its stats changed per test.
const TESTED: u16 = 0;
/// Unit 1 is a plain copy of unit 0: the dummy it fights (player 1).
const DUMMY: u16 = 1;
/// Drill unit 2: 12 archers with a 25-tile range.
const ARCHERS: u16 = 2;

/// The drill's units with morale off (nobody routs) and no charge bonus
/// (contact is made at a walk anyway), unit 0 changed by `tested`, and
/// unit 1 a plain copy of the original unit 0.
fn units(tested: impl FnOnce(&mut UnitStats)) -> [UnitStats; 4] {
    let mut units = DRILL_UNITS.map(|u| UnitStats {
        morale: 0,
        charge_bonus: 0,
        ..u
    });
    units[DUMMY as usize] = units[TESTED as usize];
    tested(&mut units[TESTED as usize]);
    units
}

fn world(units: &[UnitStats]) -> World {
    World::new(&MatchSetup {
        seed: 23,
        capacities: Capacities::default(),
        units,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
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

fn form(w: &mut World, player: u8, regiment: RegimentHandle, formation: Formation) -> bool {
    order(
        w,
        player,
        Command::SetFormation {
            regiment,
            formation,
        },
    )
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

/// Tick until `regiment` is in melee (at most 20 seconds).
fn until_engaged(w: &mut World, regiment: RegimentHandle) {
    for _ in 0..20 * 20 {
        w.tick(&[]);
        if w.regiments.get(regiment).unwrap().engaged {
            return;
        }
    }
    panic!("the regiments never met");
}

/// Where the dummy starts and which way it faces: straight ahead of the
/// tested regiment, or off its right flank.
const AHEAD: ((i32, i32), Angle) = ((100, 90), SOUTH);
const RIGHT: ((i32, i32), Angle) = ((110, 100), WEST);

/// The tested regiment stands facing north in `mine`; the dummy, in
/// `theirs`, walks into it from `from` without charging. Returns the hit
/// points the dummy lost on the tick they met, and in the `ticks` after it,
/// and the hit points the tested regiment lost in that time.
fn melee(
    tested: impl FnOnce(&mut UnitStats),
    mine: Formation,
    theirs: Formation,
    ((x, y), facing): ((i32, i32), Angle),
    ticks: u32,
) -> (u32, u32, u32) {
    let units = units(tested);
    let mut w = world(&units);
    let me = spawn(&mut w, 0, TESTED, 100, 100, NORTH);
    let foe = spawn(&mut w, 1, DUMMY, x, y, facing);
    assert!(form(&mut w, 0, me, mine));
    assert!(form(&mut w, 1, foe, theirs));
    assert!(march(&mut w, 1, foe, 100, 100, Fx::ONE));
    let full = hp(&w, foe);
    until_engaged(&mut w, me);
    let first = full - hp(&w, foe);
    let (mine_before, theirs_before) = (hp(&w, me), hp(&w, foe));
    run(&mut w, ticks);
    (first, theirs_before - hp(&w, foe), mine_before - hp(&w, me))
}

const FIGHT: u32 = 20 * 12;

#[test]
fn spears_fight_from_two_ranks() {
    let (_, plain, _) = melee(|_| {}, Formation::Line, Formation::Line, AHEAD, FIGHT);
    let (_, spears, _) = melee(
        |u| u.reach = true,
        Formation::Line,
        Formation::Line,
        AHEAD,
        FIGHT,
    );
    // Eight in the front rank and eight behind at half weight: about half
    // as much again.
    let ratio = spears as f32 / plain as f32;
    assert!(
        (1.3..1.7).contains(&ratio),
        "plain {plain}, spears {spears}"
    );
}

#[test]
fn spears_strike_first_when_lines_meet() {
    let (first, ..) = melee(
        |u| u.reach = true,
        Formation::Line,
        Formation::Line,
        AHEAD,
        0,
    );
    assert!(first > 0, "no blow landed the moment they met");
}

#[test]
fn spears_strike_first_only_at_their_points() {
    // Taken in the flank, a spear block waits for its turn like anyone.
    let (first, ..) = melee(
        |u| u.reach = true,
        Formation::Line,
        Formation::Line,
        RIGHT,
        0,
    );
    assert_eq!(first, 0);
}

#[test]
fn ranks_re_form_only_out_of_the_fight() {
    let units = units(|_| {});
    let mut w = world(&units);
    let me = spawn(&mut w, 0, TESTED, 100, 100, NORTH);
    let foe = spawn(&mut w, 1, DUMMY, 100, 90, SOUTH);
    assert!(march(&mut w, 1, foe, 100, 100, Fx::ONE));
    until_engaged(&mut w, me);
    assert!(!form(&mut w, 0, me, Formation::Tight));
    assert_eq!(w.regiments.get(me).unwrap().formation, Formation::Line);
}

#[test]
fn swinging_weapons_are_cramped_in_tight_order() {
    let swing = |u: &mut UnitStats| u.swing_room = true;
    let (_, line, _) = melee(swing, Formation::Line, Formation::Line, AHEAD, FIGHT);
    let (_, tight, _) = melee(swing, Formation::Tight, Formation::Line, AHEAD, FIGHT);
    let ratio = tight as f32 / line as f32;
    assert!((0.65..0.92).contains(&ratio), "line {line}, tight {tight}");
}

#[test]
fn spears_lose_their_support_in_loose_order() {
    let reach = |u: &mut UnitStats| u.reach = true;
    let (_, line, _) = melee(reach, Formation::Line, Formation::Line, AHEAD, FIGHT);
    let (_, loose, _) = melee(reach, Formation::Loose, Formation::Line, AHEAD, FIGHT);
    assert!(loose < line, "line {line}, loose {loose}");
}

#[test]
fn tight_order_holds_and_loose_order_has_gaps() {
    // The dummy attacks the tested regiment in each formation.
    let taken =
        |tested: fn(&mut UnitStats), mine| melee(tested, mine, Formation::Line, AHEAD, FIGHT).2;
    let line = taken(|_| {}, Formation::Line);
    let tight = taken(|_| {}, Formation::Tight);
    let loose = taken(|_| {}, Formation::Loose);
    assert!(
        tight < line && line < loose,
        "tight {tight}, line {line}, loose {loose}"
    );
    // Big swings want the room: Loose costs them no defence.
    let swing_loose = taken(|u| u.swing_room = true, Formation::Loose);
    let swing_line = taken(|u| u.swing_room = true, Formation::Line);
    let ratio = swing_loose as f32 / swing_line as f32;
    assert!(
        (0.85..1.15).contains(&ratio),
        "line {swing_line}, loose {swing_loose}"
    );
}

/// Hit points a spear block in `formation` loses to one block of archers
/// shooting at its front for `ticks`.
fn arrows_taken(shield: bool, formation: Formation, ticks: u32) -> u32 {
    let units = units(|u| u.shield = shield);
    let mut w = world(&units);
    let target = spawn(&mut w, 0, TESTED, 100, 100, NORTH);
    assert!(form(&mut w, 0, target, formation));
    spawn(&mut w, 1, ARCHERS, 100, 80, SOUTH);
    let full = hp(&w, target);
    run(&mut w, ticks);
    full - hp(&w, target)
}

#[test]
fn closer_blocks_take_more_arrows() {
    let t = 20 * 40;
    let tight = arrows_taken(false, Formation::Tight, t);
    let line = arrows_taken(false, Formation::Line, t);
    let loose = arrows_taken(false, Formation::Loose, t);
    assert!(
        tight > line && line > loose && loose > 0,
        "tight {tight}, line {line}, loose {loose}"
    );
}

#[test]
fn a_shield_wall_turns_arrows_from_the_front() {
    let t = 20 * 40;
    let tight = arrows_taken(true, Formation::Tight, t);
    let wall = arrows_taken(true, Formation::ShieldWall, t);
    let ratio = wall as f32 / tight as f32;
    assert!((0.15..0.45).contains(&ratio), "tight {tight}, wall {wall}");
}

#[test]
fn only_shields_form_a_wall() {
    let plain = units(|_| {});
    let mut w = world(&plain);
    let r = spawn(&mut w, 0, TESTED, 50, 50, NORTH);
    assert!(!form(&mut w, 0, r, Formation::ShieldWall));
    assert_eq!(w.regiments.get(r).unwrap().formation, Formation::Line);

    let shielded = units(|u| u.shield = true);
    let mut w = world(&shielded);
    let shields = spawn(&mut w, 0, TESTED, 50, 50, NORTH);
    // Only its owner can order it.
    assert!(!form(&mut w, 1, shields, Formation::ShieldWall));
    assert!(form(&mut w, 0, shields, Formation::ShieldWall));
    assert_eq!(
        w.regiments.get(shields).unwrap().formation,
        Formation::ShieldWall
    );
    // Taking the shields away (the sandbox's tuning) breaks the wall.
    let mut stats = w.units()[TESTED as usize];
    stats.shield = false;
    assert!(order(
        &mut w,
        0,
        Command::TuneUnit {
            unit: TESTED,
            stats
        }
    ));
    assert_eq!(w.regiments.get(shields).unwrap().formation, Formation::Line);
}

#[test]
fn a_shield_wall_guards_its_front_and_nothing_else() {
    // Hit points the shield regiment loses; it can't turn to face a flank
    // attack.
    let taken = |formation, from| {
        let shields = |u: &mut UnitStats| {
            u.shield = true;
            u.turn_rate = 0;
        };
        melee(shields, formation, Formation::Line, from, FIGHT).2
    };
    let front = |f| taken(f, AHEAD);
    let flank = |f| taken(f, RIGHT);
    let (line, wall) = (front(Formation::Line), front(Formation::ShieldWall));
    assert!(wall < line, "front: line {line}, wall {wall}");
    let (line, wall) = (flank(Formation::Line), flank(Formation::ShieldWall));
    let ratio = wall as f32 / line as f32;
    assert!(
        (0.85..1.15).contains(&ratio),
        "flank: line {line}, wall {wall}"
    );
}

#[test]
fn a_shield_wall_marches_at_half_pace() {
    let walked = |formation| {
        let units = units(|u| u.shield = true);
        let mut w = world(&units);
        let r = spawn(&mut w, 0, TESTED, 50, 100, NORTH);
        assert!(form(&mut w, 0, r, formation));
        assert!(march(&mut w, 0, r, 50, 20, Fx::ZERO));
        let y0 = w.regiments.get(r).unwrap().position.y;
        run(&mut w, 20 * 4);
        (y0 - w.regiments.get(r).unwrap().position.y).raw()
    };
    let line = walked(Formation::Line);
    let wall = walked(Formation::ShieldWall);
    assert_eq!(wall * 2, line, "line {line}, wall {wall}");
}

#[test]
fn a_shield_wall_cannot_charge() {
    // Charging damage with a big bonus: from Line it hits home hard, from a
    // shield wall it is an ordinary blow.
    let charge_taken = |formation| {
        let mut units = units(|u| u.shield = true);
        units[TESTED as usize].charge_bonus = 200;
        let mut w = world(&units);
        let me = spawn(&mut w, 0, TESTED, 100, 110, NORTH);
        let foe = spawn(&mut w, 1, DUMMY, 100, 100, SOUTH);
        assert!(form(&mut w, 0, me, formation));
        assert!(order(
            &mut w,
            0,
            Command::Attack {
                regiment: me,
                target: foe,
            },
        ));
        let full = hp(&w, foe);
        // Walk in (a shield wall takes longer), then two seconds of blows.
        until_engaged(&mut w, me);
        run(&mut w, 40);
        full - hp(&w, foe)
    };
    let line = charge_taken(Formation::Line);
    let wall = charge_taken(Formation::ShieldWall);
    assert!(wall * 2 < line, "line {line}, wall {wall}");
}

#[test]
fn routers_keep_no_ranks() {
    // A brittle regiment in Tight order, beaten until it runs.
    let mut units = units(|_| {});
    units[TESTED as usize].morale = 5;
    let mut w = world(&units);
    let me = spawn(&mut w, 0, TESTED, 100, 100, NORTH);
    assert!(form(&mut w, 0, me, Formation::Tight));
    spawn(&mut w, 1, DUMMY, 100, 98, SOUTH);
    for _ in 0..20 * 60 {
        w.tick(&[]);
        if w.regiments.get(me).is_none_or(|r| r.morale.broken()) {
            break;
        }
    }
    let r = w.regiments.get(me).expect("destroyed before it broke");
    assert!(matches!(
        r.morale,
        MoraleState::Routing | MoraleState::Shattered
    ));
    assert_eq!(r.formation, Formation::Line);
    // And a running regiment takes no formation orders.
    assert!(!form(&mut w, 0, me, Formation::Loose));
}

#[test]
fn formation_is_part_of_the_state() {
    let units = units(|_| {});
    let mut a = world(&units);
    let mut b = world(&units);
    let ra = spawn(&mut a, 0, TESTED, 50, 50, NORTH);
    let rb = spawn(&mut b, 0, TESTED, 50, 50, NORTH);
    assert_eq!(a.state_hash(), b.state_hash());
    assert!(form(&mut a, 0, ra, Formation::Loose));
    assert!(form(&mut b, 0, rb, Formation::Line));
    assert_ne!(a.state_hash(), b.state_hash());
}
