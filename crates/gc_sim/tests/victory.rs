//! Conquest's victories (docs/06-game-modes.md, "Victory conditions"):
//! Annihilation, with its countdown for a player who has lost every main
//! building, and holding the Titan's Heart.

use gc_fixed::FxVec2;
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, Role};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, EXILE_TICKS, HEART_HOLD_TICKS,
    HEART_UNLOCK_TICKS, HOLD_TICKS, MatchSetup, PlayerCommand, RegimentHandle, StructureHandle,
    TICKS_PER_SECOND, Verdict, Victories, Win, World,
};

const HALL: u16 = 0;
const HOUSE: u16 = 1;
/// The Heart's middle.
const HEART: (i32, i32) = (32, 32);
/// The Heart unlocks after this percent of its Standard wait: a minute.
const QUICK: u32 = 5;

fn buildings() -> [BuildingStats; 2] {
    let hall = BuildingStats {
        role: Role::Main,
        tier: 1,
        upgrades_from: None,
        cost_grub: 400,
        cost_marrow: 0,
        // Long enough to stay a foundation through a test.
        build_ticks: 60_000,
        hp: 3000,
        armour: Armour::Fortified,
        width: 3,
        depth: 3,
        supply: 10,
        flags: 0,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
        twist: gc_sim::structures::WallTwist::None,
        trap: gc_sim::structures::Trap::None,
    };
    let house = BuildingStats {
        role: Role::Housing,
        ..hall
    };
    [hall, house]
}

/// An open 64-tile map in Conquest for `teams`, played for `victories`,
/// the Heart in the middle. Every regiment is a worker, and every player
/// can pay for a hall.
fn conquest(teams: &[u8], victories: Victories) -> World {
    let side = 64 * CELLS_PER_TILE as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        seed: 9,
        capacities: Capacities { regiments: 32 },
        units: &[DRILL_UNITS[0]],
        map_tiles: 64,
        nav: &nav,
        teams,
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    assert!(w.conquest(&ConquestSetup {
        buildings: &buildings(),
        units: &[ConquestUnit {
            worker: true,
            ..ConquestUnit::default()
        }],
        start: [(1000, 0); gc_sim::MAX_PARTIES],
        structures: 32,
        build_percent: QUICK,
        victories,
        heart: Some(FxVec2::from_ints(HEART.0, HEART.1)),
        ..ConquestSetup::default()
    }));
    w
}

fn order(w: &mut World, player: u8, command: Command) {
    let applied = w
        .tick(&[PlayerCommand { player, command }])
        .commands_applied;
    assert_eq!(applied, 1, "{command:?}");
}

fn put_up(w: &mut World, player: u8, building: u16, x: i16, y: i16) -> StructureHandle {
    order(
        w,
        player,
        Command::SpawnStructure {
            building,
            tile: (x, y),
            turn: 0,
        },
    );
    let s = w.structures().unwrap();
    s.iter()
        .find(|v| v.tile == (i32::from(x), i32::from(y)))
        .unwrap()
        .handle
}

fn spawn(w: &mut World, player: u8, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    order(
        w,
        player,
        Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::from_ints(x, y),
            facing: NORTH,
        },
    );
    (w.regiments.iter())
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

/// Ticks until `done`, up to `most`.
fn until(w: &mut World, most: u32, done: impl Fn(&World) -> bool) -> u32 {
    for t in 1..=most {
        w.tick(&[]);
        if done(w) {
            return t;
        }
    }
    panic!("not done in {most} ticks");
}

/// Two players, each with a hall and a regiment, far from each other and
/// from the Heart.
fn two_sides(victories: Victories) -> (World, StructureHandle) {
    let mut w = conquest(&[], victories);
    put_up(&mut w, 0, HALL, 4, 4);
    let theirs = put_up(&mut w, 1, HALL, 56, 56);
    spawn(&mut w, 0, 10, 4);
    spawn(&mut w, 1, 54, 60);
    (w, theirs)
}

#[test]
fn a_player_without_a_main_building_is_out_after_ninety_seconds() {
    let (mut w, theirs) = two_sides(Victories::BOTH);
    put_up(&mut w, 1, HOUSE, 50, 50);
    run(&mut w, 2 * TICKS_PER_SECOND);
    assert_eq!(w.exiled(1), None);
    order(&mut w, 1, Command::RemoveStructure { structure: theirs });
    until(&mut w, TICKS_PER_SECOND, |w| w.exiled(1).is_some());
    let deadline = w.exiled(1).unwrap();
    assert_eq!(deadline, w.tick_count() + EXILE_TICKS);
    assert!(!w.is_out(1));
    // Not out a second early.
    let wait = deadline - TICKS_PER_SECOND - w.tick_count();
    run(&mut w, wait);
    assert!(!w.is_out(1) && w.verdict().is_none());
    until(&mut w, 2 * TICKS_PER_SECOND, |w| w.is_out(1));
    assert_eq!(w.tick_count(), deadline);
    // Their regiments and buildings leave the field.
    assert!(w.regiments.iter().all(|r| r.owner == 0));
    assert!(w.structures().unwrap().iter().all(|s| s.owner == 0));
    assert_eq!(w.exiled(1), None);
    assert_eq!(
        w.verdict(),
        Some(Verdict {
            holder: 0,
            tick: deadline,
            by: Win::Annihilation
        })
    );
}

#[test]
fn laying_a_main_building_stops_the_countdown() {
    let (mut w, theirs) = two_sides(Victories::BOTH);
    run(&mut w, TICKS_PER_SECOND);
    order(&mut w, 1, Command::RemoveStructure { structure: theirs });
    run(&mut w, 60 * TICKS_PER_SECOND);
    assert!(w.exiled(1).is_some());
    // A new hall ends it.
    put_up(&mut w, 1, HALL, 40, 56);
    run(&mut w, TICKS_PER_SECOND);
    assert_eq!(w.exiled(1), None);
    run(&mut w, EXILE_TICKS);
    assert!(!w.is_out(1));
    assert_eq!(w.verdict(), None);
}

#[test]
fn a_main_building_going_up_holds_the_countdown() {
    let (mut w, theirs) = two_sides(Victories::BOTH);
    let worker = spawn(&mut w, 1, 40, 50);
    run(&mut w, TICKS_PER_SECOND);
    order(&mut w, 1, Command::RemoveStructure { structure: theirs });
    run(&mut w, 30 * TICKS_PER_SECOND);
    let left = w.exiled(1).unwrap() - w.tick_count();
    order(
        &mut w,
        1,
        Command::Build {
            regiment: worker,
            building: HALL,
            tile: (40, 53),
            turn: 0,
        },
    );
    let site = (w.structures().unwrap().iter())
        .find(|s| s.tile == (40, 53))
        .unwrap();
    assert!(!site.finished());
    run(&mut w, EXILE_TICKS);
    assert_eq!(w.exiled(1), None);
    assert!(!w.is_out(1));
    // Calling it off takes the countdown up where it was, not from the top.
    order(
        &mut w,
        1,
        Command::CallOff {
            structure: site.handle,
        },
    );
    until(&mut w, TICKS_PER_SECOND, |w| w.exiled(1).is_some());
    let resumed = w.exiled(1).unwrap() - w.tick_count();
    assert!(resumed <= left && resumed + 2 * TICKS_PER_SECOND >= left);
    until(&mut w, resumed + TICKS_PER_SECOND, |w| w.is_out(1));
}

#[test]
fn the_last_players_put_out_together_draw() {
    let mut w = conquest(&[], Victories::BOTH);
    let halls = [
        put_up(&mut w, 0, HALL, 4, 4),
        put_up(&mut w, 1, HALL, 56, 56),
    ];
    run(&mut w, TICKS_PER_SECOND);
    let gone = halls.map(|structure| PlayerCommand {
        player: structure_owner(&w, structure),
        command: Command::RemoveStructure { structure },
    });
    assert_eq!(w.tick(&gone).commands_applied, 2);
    until(&mut w, EXILE_TICKS + TICKS_PER_SECOND, |w| w.is_out(0));
    assert!(w.is_out(1));
    assert_eq!(w.verdict().map(|v| v.by), Some(Win::Draw));
}

fn structure_owner(w: &World, h: StructureHandle) -> u8 {
    w.structures().unwrap().get(h).unwrap().owner
}

#[test]
fn an_army_without_a_main_building_is_still_standing() {
    let mut w = conquest(&[], Victories::BOTH);
    put_up(&mut w, 0, HALL, 4, 4);
    let theirs = put_up(&mut w, 1, HALL, 56, 56);
    let army = spawn(&mut w, 2, 30, 60);
    run(&mut w, TICKS_PER_SECOND);
    order(&mut w, 1, Command::RemoveStructure { structure: theirs });
    until(&mut w, EXILE_TICKS + TICKS_PER_SECOND, |w| w.is_out(1));
    run(&mut w, 2 * TICKS_PER_SECOND);
    assert_eq!(w.verdict(), None, "party 2's army still stands");
    order(&mut w, 2, Command::RemoveRegiment { regiment: army });
    until(&mut w, TICKS_PER_SECOND, |w| w.verdict().is_some());
    assert_eq!(
        w.verdict().map(|v| (v.holder, v.by)),
        Some((0, Win::Annihilation))
    );
}

#[test]
fn an_ally_keeps_the_heart_when_its_holder_is_out() {
    let mut w = conquest(&[0, 0, 1], Victories::BOTH);
    let first = put_up(&mut w, 0, HALL, 4, 4);
    put_up(&mut w, 1, HALL, 56, 4);
    put_up(&mut w, 2, HALL, 56, 56);
    spawn(&mut w, 0, HEART.0, HEART.1 + 2);
    spawn(&mut w, 1, HEART.0, HEART.1 - 2);
    run(&mut w, TICKS_PER_SECOND);
    order(&mut w, 0, Command::RemoveStructure { structure: first });
    // The Heart opens at a minute; party 0 is out half a minute later.
    until(&mut w, 70 * TICKS_PER_SECOND, |w| {
        w.heart().unwrap().held.is_some()
    });
    let (holder, since) = w.heart().unwrap().held.unwrap();
    assert_eq!(holder, 0);
    until(&mut w, EXILE_TICKS, |w| w.is_out(0));
    assert_eq!(w.heart().unwrap().held, Some((1, since)));
    assert_eq!(w.verdict(), None);
}

#[test]
fn conquest_is_not_won_by_holding_the_field() {
    let (mut w, _) = two_sides(Victories {
        annihilation: true,
        heart: false,
    });
    let theirs = w.regiments.iter().find(|r| r.owner == 1).unwrap().handle;
    order(&mut w, 1, Command::RemoveRegiment { regiment: theirs });
    // Their hall still stands.
    run(&mut w, 2 * HOLD_TICKS);
    assert_eq!(w.verdict(), None);
    assert_eq!(w.heart(), None, "the Heart doesn't count");
}

#[test]
fn allies_win_together_and_a_free_for_all_goes_on() {
    for (teams, ends) in [(&[0u8, 0, 1][..], true), (&[0, 1, 2][..], false)] {
        let mut w = conquest(teams, Victories::BOTH);
        put_up(&mut w, 0, HALL, 4, 4);
        put_up(&mut w, 1, HALL, 56, 4);
        let last = put_up(&mut w, 2, HALL, 56, 56);
        run(&mut w, TICKS_PER_SECOND);
        order(&mut w, 2, Command::RemoveStructure { structure: last });
        until(&mut w, EXILE_TICKS + 2 * TICKS_PER_SECOND, |w| w.is_out(2));
        assert_eq!(w.verdict().is_some(), ends, "{teams:?}");
        if ends {
            assert_eq!(w.verdict().unwrap().holder, 0);
        }
    }
}

#[test]
fn holding_the_heart_uncontested_for_three_minutes_wins() {
    let (mut w, _) = two_sides(Victories::BOTH);
    let unlock = HEART_UNLOCK_TICKS * QUICK / 100;
    assert_eq!(w.heart().unwrap().unlock, unlock);
    spawn(&mut w, 0, HEART.0, HEART.1 + 2);
    // Not held before it unlocks.
    let wait = unlock - w.tick_count() - 1;
    run(&mut w, wait);
    assert_eq!(w.heart().unwrap().held, None);
    assert!(!w.heart().unwrap().open);
    until(&mut w, 2 * TICKS_PER_SECOND, |w| {
        w.heart().unwrap().held.is_some()
    });
    assert!(w.heart().unwrap().open);
    let (holder, since) = w.heart().unwrap().held.unwrap();
    assert_eq!(holder, 0);
    // An enemy at the Heart contests it, and the hold starts over.
    run(&mut w, 60 * TICKS_PER_SECOND);
    let theirs = spawn(&mut w, 1, HEART.0 + 4, HEART.1 - 3);
    run(&mut w, TICKS_PER_SECOND);
    assert_eq!(w.heart().unwrap().held, None);
    order(&mut w, 1, Command::RemoveRegiment { regiment: theirs });
    until(&mut w, 2 * TICKS_PER_SECOND, |w| {
        w.heart().unwrap().held.is_some()
    });
    let (_, again) = w.heart().unwrap().held.unwrap();
    assert!(again > since + 60 * TICKS_PER_SECOND);
    let wait = HEART_HOLD_TICKS - (w.tick_count() - again) - 1;
    run(&mut w, wait);
    assert_eq!(w.verdict(), None);
    run(&mut w, 1);
    assert_eq!(
        w.verdict(),
        Some(Verdict {
            holder: 0,
            tick: again + HEART_HOLD_TICKS,
            by: Win::Heart
        })
    );
}

#[test]
fn the_sandbox_still_ends_by_holding_the_field() {
    let (mut w, _) = two_sides(Victories::default());
    assert_eq!(w.heart(), None);
    let theirs = w.regiments.iter().find(|r| r.owner == 1).unwrap().handle;
    order(&mut w, 1, Command::RemoveRegiment { regiment: theirs });
    run(&mut w, HOLD_TICKS + 2 * TICKS_PER_SECOND);
    assert_eq!(w.verdict().map(|v| (v.holder, v.by)), Some((0, Win::Field)));
}
