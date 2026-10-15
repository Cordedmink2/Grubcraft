//! Hiding in woods and tall grass, spotting, and springing ambushes
//! (docs/20-ambushes.md).

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, Drill, NORTH};
use gc_sim::vision::{AMBUSH_RIPPLE, AMBUSH_SHOCK, LINGER_TICKS, SURPRISE_TICKS, VOLLEY_SHOCK};
use gc_sim::{
    Ambush, Capacities, Command, Cover, Ground, MatchSetup, MoraleState, PlayerCommand,
    RegimentHandle, UnitStats, World,
};

const SOUTH: Angle = Angle(0x4000);
const EAST: Angle = Angle(0);
const WEST: Angle = Angle(0x8000);
const SPEARS: u16 = 0;
const CAVALRY: u16 = 1;
const ARCHERS: u16 = 2;
const MAP: u32 = 64;
const SECOND: u32 = gc_sim::TICKS_PER_SECOND;

/// A cover map with `kind` on the tiles `x0..x1` by `y0..y1`.
fn patch(kind: Cover, x0: u32, x1: u32, y0: u32, y1: u32) -> Vec<u8> {
    (0..MAP * MAP)
        .map(|i| {
            let (x, y) = (i % MAP, i / MAP);
            if (x0..x1).contains(&x) && (y0..y1).contains(&y) {
                kind as u8
            } else {
                Cover::Open as u8
            }
        })
        .collect()
}

fn world_with(seed: u64, units: &[UnitStats], cover: &[u8], teams: &[u8]) -> World {
    world_on(MAP, seed, units, cover, teams)
}

fn world_on(map: u32, seed: u64, units: &[UnitStats], cover: &[u8], teams: &[u8]) -> World {
    World::new(&MatchSetup {
        seed,
        capacities: Capacities::default(),
        units,
        map_tiles: map,
        nav: &[],
        teams,
        ground: Ground {
            heights: &[],
            water_level: 0,
            cover,
        },
    })
}

fn world(cover: &[u8]) -> World {
    world_with(1, &DRILL_UNITS, cover, &[])
}

fn spawn(w: &mut World, player: u8, unit: u16, x: Fx, y: Fx, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    let stats = w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit,
            position: FxVec2::new(x, y),
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

fn tile(n: i32) -> Fx {
    Fx::from_int(n) + Fx::HALF
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

/// Ticks until the first ambush springs, for at most `ticks` ticks.
fn first_ambush(w: &mut World, ticks: u32) -> Option<Ambush> {
    for _ in 0..ticks {
        if let Some(&a) = w.ambushes().first() {
            return Some(a);
        }
        w.tick(&[]);
    }
    w.ambushes().first().copied()
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn move_to(w: &mut World, player: u8, regiment: RegimentHandle, x: Fx, y: Fx) {
    assert!(order(
        w,
        player,
        Command::MoveTo {
            regiment,
            target: FxVec2::new(x, y),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    ));
}

fn attack(w: &mut World, player: u8, regiment: RegimentHandle, target: RegimentHandle) -> bool {
    order(w, player, Command::Attack { regiment, target })
}

fn hp(w: &World, h: RegimentHandle) -> u32 {
    w.regiments
        .model_hp(h)
        .map_or(0, |hp| hp.iter().copied().map(u32::from).sum())
}

fn morale(w: &World, h: RegimentHandle) -> u8 {
    w.regiments.get(h).unwrap().morale_points
}

#[test]
fn without_cover_everyone_sees_everything() {
    let mut w = world(&[]);
    assert!(!w.terrain().has_cover());
    let a = spawn(&mut w, 0, SPEARS, tile(20), tile(20), SOUTH);
    let b = spawn(&mut w, 1, SPEARS, tile(20), tile(50), NORTH);
    run(&mut w, 10);
    assert!(w.sees(0, b) && w.sees(1, a));
    assert!(!w.hidden(a) && !w.hidden(b));
}

#[test]
fn open_ground_hides_nothing() {
    let mut w = world(&patch(Cover::Wood, 40, 60, 0, 64));
    assert!(w.terrain().has_cover());
    let a = spawn(&mut w, 0, SPEARS, tile(10), tile(10), SOUTH);
    let b = spawn(&mut w, 1, SPEARS, tile(10), tile(55), NORTH);
    run(&mut w, 10);
    assert!(w.sees(0, b) && w.sees(1, a));
    assert!(!w.hidden(a) && !w.hidden(b));
}

#[test]
fn a_regiment_in_a_wood_is_hidden_until_an_enemy_comes_close() {
    let mut w = world(&patch(Cover::Wood, 10, 30, 10, 30));
    let hid = spawn(&mut w, 1, SPEARS, tile(20), tile(20), SOUTH);
    let scout = spawn(&mut w, 0, SPEARS, tile(20), tile(50), NORTH);
    run(&mut w, 2);
    // Its own side sees it; the enemy doesn't. The scout in the open is seen.
    assert!(w.sees(1, hid));
    assert!(!w.sees(0, hid));
    assert!(w.hidden(hid));
    assert!(w.sees(1, scout) && !w.hidden(scout));

    // Front to front about 5.4 tiles apart (the blocks are 2.55 tiles
    // deep, so their centres are 8 apart): still hidden.
    move_to(&mut w, 0, scout, tile(20), tile(28));
    run(&mut w, 10 * SECOND);
    assert!(!w.sees(0, hid), "spotted from 5 tiles");
    // About 3.4 tiles apart: spotted.
    move_to(&mut w, 0, scout, tile(20), tile(26));
    run(&mut w, 2 * SECOND);
    assert!(w.sees(0, hid), "not spotted from 3 tiles");
    assert!(!w.hidden(hid));

    // Back out of reach: it stays seen for a little while, then fades.
    move_to(&mut w, 0, scout, tile(20), tile(50));
    let mut gone_after = None;
    for t in 0..10 * SECOND {
        w.tick(&[]);
        if !w.sees(0, hid) {
            gone_after = Some(t);
            break;
        }
    }
    let t = gone_after.expect("never hidden again");
    assert!(t >= u32::from(LINGER_TICKS), "faded after {t} ticks");
}

#[test]
fn a_regiment_marching_in_a_wood_is_seen_from_further() {
    let mut w = world(&patch(Cover::Wood, 4, 60, 10, 30));
    let hid = spawn(&mut w, 1, SPEARS, tile(10), tile(20), EAST);
    // Edge to edge about 7 tiles away (centres 13 apart, blocks 6 wide).
    let _watcher = spawn(&mut w, 0, SPEARS, tile(16), tile(33), NORTH);
    run(&mut w, 2 * SECOND);
    assert!(!w.sees(0, hid), "spotted standing in the wood");
    move_to(&mut w, 1, hid, tile(26), tile(20));
    run(&mut w, SECOND);
    assert!(w.sees(0, hid), "not spotted marching in the wood");
}

#[test]
fn tall_grass_hides_only_regiments_standing_still() {
    let mut w = world(&patch(Cover::Grass, 4, 60, 10, 30));
    let hid = spawn(&mut w, 1, SPEARS, tile(10), tile(20), EAST);
    let watcher = spawn(&mut w, 0, SPEARS, tile(10), tile(32), NORTH);
    run(&mut w, 2 * SECOND);
    // 6 tiles edge to edge: hidden.
    assert!(!w.sees(0, hid));
    // Marching through grass: seen.
    move_to(&mut w, 1, hid, tile(30), tile(20));
    run(&mut w, 2);
    assert!(w.sees(0, hid));
    // Standing again, it fades.
    run(&mut w, 15 * SECOND);
    assert!(!w.sees(0, hid));
    // An enemy within 2 tiles finds it.
    move_to(&mut w, 0, watcher, tile(30), tile(28));
    run(&mut w, 10 * SECOND);
    assert!(w.sees(0, hid));
}

#[test]
fn half_in_a_wood_is_seen() {
    // The wood ends at x = 20: a block centred on its edge sticks out.
    let mut w = world(&patch(Cover::Wood, 0, 20, 0, 64));
    let half = spawn(&mut w, 1, SPEARS, tile(19), tile(20), SOUTH);
    let deep = spawn(&mut w, 1, SPEARS, tile(10), tile(40), SOUTH);
    let _enemy = spawn(&mut w, 0, SPEARS, tile(45), tile(30), WEST);
    run(&mut w, 2);
    assert!(w.sees(0, half));
    assert!(!w.sees(0, deep));
}

#[test]
fn allies_share_what_they_see() {
    // Parties 0 and 2 are allies against party 1.
    let mut w = world_with(
        1,
        &DRILL_UNITS,
        &patch(Cover::Wood, 10, 30, 10, 30),
        &[0, 1, 0],
    );
    let hid = spawn(&mut w, 1, SPEARS, tile(20), tile(20), SOUTH);
    let _far = spawn(&mut w, 0, SPEARS, tile(50), tile(55), NORTH);
    let near = spawn(&mut w, 2, SPEARS, tile(20), tile(50), NORTH);
    run(&mut w, 2);
    assert!(!w.sees(0, hid) && !w.sees(2, hid));
    move_to(&mut w, 2, near, tile(20), tile(26));
    run(&mut w, 8 * SECOND);
    assert!(w.sees(2, hid));
    assert!(w.sees(0, hid), "an ally's sighting isn't shared");
}

#[test]
fn fighting_gives_a_hidden_regiment_away() {
    let mut w = world(&patch(Cover::Wood, 10, 30, 10, 30));
    let hid = spawn(&mut w, 1, SPEARS, tile(20), tile(20), SOUTH);
    let other = spawn(&mut w, 0, SPEARS, tile(20), tile(23), NORTH);
    run(&mut w, 2);
    // Touching: in melee, seen by everyone, including a party far away.
    assert!(w.regiments.get(hid).unwrap().engaged);
    assert!(w.sees(0, hid) && w.sees(3, hid));
    assert!(!w.hidden(hid) && !w.hidden(other));
}

#[test]
fn attack_orders_need_a_target_the_side_can_see() {
    let mut w = world(&patch(Cover::Wood, 10, 30, 10, 30));
    let hid = spawn(&mut w, 1, SPEARS, tile(20), tile(20), SOUTH);
    let open = spawn(&mut w, 1, SPEARS, tile(45), tile(20), SOUTH);
    let me = spawn(&mut w, 0, CAVALRY, tile(30), tile(50), NORTH);
    run(&mut w, 2);
    assert!(
        !attack(&mut w, 0, me, hid),
        "attacked a regiment it can't see"
    );
    assert!(attack(&mut w, 0, me, open));
}

#[test]
fn archers_only_shoot_what_their_side_can_see() {
    // Archers reach 25 tiles; their target stands 22 tiles away.
    let wounds = |cover: &[u8]| {
        let mut w = world_with(3, &DRILL_UNITS, cover, &[]);
        let target = spawn(&mut w, 1, SPEARS, tile(20), tile(20), SOUTH);
        let _archers = spawn(&mut w, 0, ARCHERS, tile(20), tile(42), NORTH);
        let before = hp(&w, target);
        run(&mut w, 15 * SECOND);
        before - hp(&w, target)
    };
    assert!(wounds(&patch(Cover::Wood, 40, 60, 0, 64)) > 0);
    assert_eq!(wounds(&patch(Cover::Wood, 10, 30, 10, 30)), 0);
}

#[test]
fn hidden_archers_hold_their_fire_until_ordered() {
    let mut w = world(&patch(Cover::Wood, 10, 30, 10, 30));
    let archers = spawn(&mut w, 1, ARCHERS, tile(20), tile(22), SOUTH);
    let spears = spawn(&mut w, 0, SPEARS, tile(20), tile(42), NORTH);
    let before = hp(&w, spears);
    run(&mut w, 10 * SECOND);
    assert_eq!(hp(&w, spears), before, "hidden archers shot on their own");
    assert!(w.hidden(archers));

    // Ordered to shoot: they loose, give themselves away, and the volley
    // out of nowhere shakes the target.
    let steady = morale(&w, spears);
    assert!(attack(&mut w, 1, archers, spears));
    let a = first_ambush(&mut w, 2 * SECOND).expect("no ambush volley");
    assert!(a.volley);
    assert_eq!((a.ambusher, a.victim), (archers, spears));
    assert!(!w.hidden(archers));
    assert!(w.sees(0, archers));
    assert!(morale(&w, spears) + VOLLEY_SHOCK as u8 <= steady + 1);
    // And they keep shooting, seen.
    run(&mut w, 10 * SECOND);
    assert!(hp(&w, spears) < before);
}

#[test]
fn each_hidden_regiment_springs_one_ambush() {
    // Archers loose volley after volley from the wood: only the first
    // shakes the target.
    let mut w = world(&patch(Cover::Wood, 10, 30, 10, 30));
    let archers = spawn(&mut w, 1, ARCHERS, tile(20), tile(22), SOUTH);
    let spears = spawn(&mut w, 0, SPEARS, tile(20), tile(42), NORTH);
    run(&mut w, 5 * SECOND);
    assert!(attack(&mut w, 1, archers, spears));
    let mut volleys = w.ambushes().len();
    for _ in 0..15 * SECOND {
        w.tick(&[]);
        volleys += w.ambushes().len();
    }
    assert_eq!(volleys, 1);

    // Cavalry springs out on the spears and is ordered straight on to
    // their friend: the friend's side has it in full view by then.
    let (mut w, cav, spears, friend) = charge(true);
    let mut sprung = Vec::new();
    let mut ordered = false;
    for _ in 0..10 * SECOND {
        w.tick(&[]);
        sprung.extend(w.ambushes().iter().map(|a| a.victim));
        if !sprung.is_empty() && !ordered {
            ordered = attack(&mut w, 1, cav, friend);
        }
    }
    assert!(ordered);
    assert_eq!(sprung, vec![spears]);
}

/// A cavalry regiment charges a spear block from 12 tiles away. With
/// `wood`, it waits hidden in a wood until the order. Returns the world,
/// the cavalry, the spears, and a friend of the spears standing near them.
fn charge(wood: bool) -> (World, RegimentHandle, RegimentHandle, RegimentHandle) {
    let cover = if wood {
        patch(Cover::Wood, 10, 30, 10, 30)
    } else {
        patch(Cover::Wood, 50, 60, 50, 60)
    };
    let mut w = world(&cover);
    let cav = spawn(&mut w, 1, CAVALRY, tile(20), tile(24), SOUTH);
    let spears = spawn(&mut w, 0, SPEARS, tile(20), tile(36), NORTH);
    let friend = spawn(&mut w, 0, SPEARS, tile(30), tile(40), NORTH);
    run(&mut w, 5 * SECOND);
    assert_eq!(w.sees(0, cav), !wood);
    assert!(attack(&mut w, 1, cav, spears));
    (w, cav, spears, friend)
}

#[test]
fn a_charge_from_hiding_springs_an_ambush() {
    let (mut w, cav, spears, friend) = charge(true);
    let (steady, friend_steady) = (morale(&w, spears), morale(&w, friend));
    let a = first_ambush(&mut w, u32::from(SURPRISE_TICKS)).expect("no ambush");
    assert!(!a.volley);
    assert_eq!((a.ambusher, a.victim), (cav, spears));
    assert!(w.caught_off_guard(spears));
    assert!(!w.caught_off_guard(cav));
    assert!(morale(&w, spears) + AMBUSH_SHOCK as u8 <= steady + 1);
    assert!(morale(&w, friend) + AMBUSH_RIPPLE as u8 <= friend_steady + 1);
    // Caught off guard wears off.
    run(&mut w, 7 * SECOND);
    assert!(!w.caught_off_guard(spears));
}

#[test]
fn a_charge_seen_coming_is_no_ambush() {
    let (mut w, _cav, spears, _friend) = charge(false);
    for _ in 0..8 * SECOND {
        w.tick(&[]);
        assert!(w.ambushes().is_empty());
        assert!(!w.caught_off_guard(spears));
    }
    assert!(w.regiments.get(spears).unwrap().engaged);
}

#[test]
fn an_ambush_breaks_a_line_that_would_have_held() {
    // Spears locked in melee front to front; cavalry hits their flank from
    // 12 tiles to the east, from a wood or from open ground. Returns the
    // tick the flanked spears break, if they do within 40 seconds, summed
    // over a few seeds.
    let broke = |wood: bool| -> (u32, u32) {
        let (mut broken, mut when) = (0, 0);
        for seed in 1..=6 {
            let cover = if wood {
                patch(Cover::Wood, 31, 60, 20, 42)
            } else {
                patch(Cover::Wood, 50, 60, 50, 60)
            };
            let mut w = world_with(seed, &DRILL_UNITS, &cover, &[]);
            let line = spawn(&mut w, 0, SPEARS, tile(20), tile(31), NORTH);
            let _foe = spawn(&mut w, 1, SPEARS, tile(20), tile(28), SOUTH);
            let cav = spawn(&mut w, 1, CAVALRY, tile(36), tile(31), WEST);
            run(&mut w, 3 * SECOND);
            assert!(attack(&mut w, 1, cav, line));
            for t in 0..40 * SECOND {
                w.tick(&[]);
                let r = w.regiments.get(line);
                if r.is_none_or(|r| r.morale.broken()) {
                    broken += 1;
                    when += t;
                    break;
                }
            }
        }
        (broken, when)
    };
    let (ambushed, ambushed_when) = broke(true);
    let (open, open_when) = broke(false);
    println!(
        "ambushed: {ambushed} broke, ticks {ambushed_when}; open: {open} broke, ticks {open_when}"
    );
    assert!(
        ambushed > open || (ambushed == open && ambushed_when * 10 < open_when * 9),
        "ambushed {ambushed} ({ambushed_when}) vs open {open} ({open_when})"
    );
}

#[test]
fn a_chase_ends_where_the_target_was_last_seen() {
    let mut w = world(&patch(Cover::Wood, 40, 64, 0, 64));
    let quarry = spawn(&mut w, 1, SPEARS, tile(30), tile(20), EAST);
    let archers = spawn(&mut w, 0, ARCHERS, tile(10), tile(60), NORTH);
    assert!(attack(&mut w, 0, archers, quarry));
    // The quarry slips into the wood and stands there.
    move_to(&mut w, 1, quarry, tile(52), tile(20));
    let mut hidden_at = None;
    for _ in 0..20 * SECOND {
        w.tick(&[]);
        if !w.sees(0, quarry) {
            hidden_at = Some(w.regiments.get(quarry).unwrap().position);
            break;
        }
    }
    let last_seen = hidden_at.expect("never lost sight of it");
    // The archers head for where it was last seen, not where it goes next.
    w.tick(&[]);
    let heading = *w.route(archers).last().expect("archers stopped");
    assert_eq!(heading, last_seen);
    move_to(&mut w, 1, quarry, tile(52), tile(6));
    run(&mut w, 3 * SECOND);
    if let Some(&to) = w.route(archers).last() {
        assert_eq!(to, last_seen);
    }
    assert!(!w.sees(0, quarry));
}

#[test]
fn regiments_placed_in_a_wood_start_hidden_and_in_the_open_start_seen() {
    let mut w = world(&patch(Cover::Wood, 10, 30, 10, 30));
    let _enemy = spawn(&mut w, 0, SPEARS, tile(50), tile(50), NORTH);
    let hid = spawn(&mut w, 1, SPEARS, tile(20), tile(20), SOUTH);
    let open = spawn(&mut w, 1, SPEARS, tile(45), tile(20), SOUTH);
    assert!(!w.sees(0, hid));
    assert!(w.sees(0, open));
}

#[test]
fn a_fearless_regiment_shrugs_off_an_ambush() {
    // A fearless victim (morale stat 0) loses no morale to an ambush.
    let mut units = DRILL_UNITS;
    units[SPEARS as usize].morale = 0;
    let mut w = world_with(1, &units, &patch(Cover::Wood, 10, 30, 10, 30), &[]);
    let cav = spawn(&mut w, 1, CAVALRY, tile(20), tile(24), SOUTH);
    let spears = spawn(&mut w, 0, SPEARS, tile(20), tile(36), NORTH);
    run(&mut w, SECOND);
    assert!(attack(&mut w, 1, cav, spears));
    run(&mut w, 4 * SECOND);
    let r = w.regiments.get(spears).unwrap();
    assert_eq!(r.morale, MoraleState::Steady);
    assert!(r.engaged);
}

/// Woods and grass in a checker of 16-tile squares, for whole-map runs.
fn checker(map: u32) -> Vec<u8> {
    (0..map * map)
        .map(|i| match ((i % map) / 16 + (i / map) / 16) % 3 {
            0 => Cover::Wood as u8,
            1 => Cover::Grass as u8,
            _ => Cover::Open as u8,
        })
        .collect()
}

fn drill_hashes(cover: &[u8]) -> Vec<u64> {
    let mut world = world_on(256, 11, &DRILL_UNITS, cover, &[]);
    let mut drill = Drill::new(11);
    let mut commands = Vec::new();
    let mut hashes = Vec::new();
    for t in 0..600 {
        drill.commands_for(&world, &mut commands);
        world.tick(&commands);
        if t % 100 == 99 {
            hashes.push(world.state_hash());
        }
    }
    hashes
}

#[test]
fn hiding_is_deterministic_and_changes_the_hash() {
    let cover = checker(256);
    let a = drill_hashes(&cover);
    assert_eq!(a, drill_hashes(&cover));
    assert_ne!(a, drill_hashes(&[]));
}
