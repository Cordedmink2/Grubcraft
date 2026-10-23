//! Lines that push (docs/05-combat.md): in a locked melee the side losing
//! the exchange gives ground and the winners follow; a regiment with
//! nowhere to go is pinned, falls into disorder and loses heart.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::combat::DISORDER_TICKS;
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::terrain::HEIGHT_UNITS;
use gc_sim::{
    Capacities, Command, Ground, Mass, MatchSetup, PlayerCommand, RegimentHandle, UnitStats, World,
};

const SOUTH: Angle = Angle(0x4000);
const MAP: u32 = 64;
const STRONG: u16 = 0;
const WEAK: u16 = 1;
const EVEN: u16 = 2;
const HEAVY: u16 = 3;
const LIGHT: u16 = 4;
/// Thick-skinned: fights between them last long enough to watch, and
/// nobody falls in the first seconds.
const STURDY: u16 = 5;

/// Spearmen from the drill who never run and take a long time to kill,
/// fighting well, badly, evenly, heavy or light.
fn units() -> [UnitStats; 6] {
    let base = UnitStats {
        morale: 0,
        hp_per_model: 60,
        ..DRILL_UNITS[0]
    };
    [
        UnitStats {
            melee_attack: 60,
            melee_damage: 6,
            ..base
        },
        UnitStats {
            melee_attack: 20,
            melee_damage: 2,
            ..base
        },
        base,
        UnitStats {
            mass: Mass::Heavy,
            ..base
        },
        UnitStats {
            mass: Mass::Light,
            ..base
        },
        UnitStats {
            melee_attack: 20,
            melee_damage: 2,
            hp_per_model: 1000,
            morale: 60,
            ..base
        },
    ]
}

enum Behind {
    Open,
    /// Impassable from this row of tiles south.
    Cliff(u32),
    /// Under water from this row of tiles south.
    River(u32),
}

fn world(behind: Behind) -> World {
    let side = MAP * CELLS_PER_TILE as u32;
    let (nav, heights): (Vec<u8>, Vec<u16>) = match behind {
        Behind::Open => (Vec::new(), Vec::new()),
        Behind::Cliff(row) => {
            let cell = |c: u32| {
                if c / side / CELLS_PER_TILE as u32 >= row {
                    BLOCKED
                } else {
                    1
                }
            };
            ((0..side * side).map(cell).collect(), Vec::new())
        }
        Behind::River(row) => {
            let tile = |i: u32| {
                if i / MAP >= row {
                    0
                } else {
                    2 * HEIGHT_UNITS as u16
                }
            };
            (Vec::new(), (0..MAP * MAP).map(tile).collect())
        }
    };
    World::new(&MatchSetup {
        seed: 9,
        capacities: Capacities { regiments: 16 },
        units: &units(),
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: Ground {
            heights: &heights,
            // Ignored on flat maps.
            water_level: HEIGHT_UNITS as u16,
            cover: &[],
        },
        champions: false,
    })
}

fn spawn(w: &mut World, player: u8, unit: u16, x: Fx, y: Fx, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit,
            position: FxVec2::new(x, y),
            facing,
        },
    }]);
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

/// Two regiments face to face, in contact: `north` (player 1) faces south,
/// `south` (player 0, whose back is to the south) faces north.
fn face_off(w: &mut World, north: u16, south: u16) -> (RegimentHandle, RegimentHandle) {
    let a = spawn(w, 1, north, Fx::from_int(32), Fx::from_int(20), SOUTH);
    let b = spawn(w, 0, south, Fx::from_int(32), Fx::from_int(23), NORTH);
    w.tick(&[]);
    assert!(w.regiments.get(a).unwrap().engaged);
    (a, b)
}

fn at(w: &World, h: RegimentHandle) -> FxVec2 {
    w.regiments.get(h).unwrap().position
}

/// How far each of `regiments` has moved south (positive) or north since
/// `from`, in tiles.
fn moved_south(w: &World, regiments: &[(RegimentHandle, FxVec2)]) -> Vec<f64> {
    regiments
        .iter()
        .map(|&(h, from)| f64::from((at(w, h).y - from.y).raw()) / 65536.0)
        .collect()
}

fn seconds(w: &mut World, s: u32) {
    for _ in 0..s * gc_sim::TICKS_PER_SECOND {
        w.tick(&[]);
    }
}

#[test]
fn the_side_losing_the_exchange_gives_ground_and_the_winners_follow() {
    let mut w = world(Behind::Open);
    let (strong, weak) = face_off(&mut w, STRONG, WEAK);
    let start = [(strong, at(&w, strong)), (weak, at(&w, weak))];
    for _ in 0..15 * 20 {
        w.tick(&[]);
        // Locked together the whole way: nobody breaks contact and charges
        // back in.
        for h in [strong, weak] {
            let r = w.regiments.get(h).unwrap();
            assert!(r.engaged && r.charge == 0, "{r:?}");
        }
    }
    let moved = moved_south(&w, &start);
    // Driven back near the full 0.4 tiles a second, straight back.
    assert!(moved[1] > 4.0, "{moved:?}");
    assert!((moved[0] - moved[1]).abs() < 0.3, "{moved:?}");
    assert_eq!(at(&w, weak).x, start[1].1.x);
    assert!(w.regiments.get(weak).unwrap().giving_ground);
    assert!(!w.regiments.get(strong).unwrap().giving_ground);
}

#[test]
fn evenly_matched_lines_hold_their_ground() {
    let mut w = world(Behind::Open);
    let (a, b) = face_off(&mut w, EVEN, EVEN);
    let start = [(a, at(&w, a)), (b, at(&w, b))];
    seconds(&mut w, 15);
    let moved = moved_south(&w, &start);
    assert!(moved.iter().all(|m| m.abs() < 0.2), "{moved:?}");
    for h in [a, b] {
        assert!(!w.regiments.get(h).unwrap().giving_ground);
    }
}

#[test]
fn heavy_men_drive_light_ones_back_trading_even_blows() {
    // Whichever side the light men are on, they give ground.
    for (north, south, loser) in [(HEAVY, LIGHT, 1), (LIGHT, HEAVY, 0)] {
        let mut w = world(Behind::Open);
        let (a, b) = face_off(&mut w, north, south);
        let start = [(a, at(&w, a)), (b, at(&w, b))];
        seconds(&mut w, 10);
        let moved = moved_south(&w, &start);
        let back = if loser == 1 { moved[1] } else { -moved[0] };
        assert!(back > 2.5, "{moved:?}");
        assert!((moved[0] - moved[1]).abs() < 0.3, "{moved:?}");
    }
}

#[test]
fn a_fight_three_deep_moves_as_one() {
    // The strong north block drives the weak one south; a third block
    // hits the strong one from behind and keeps up with it.
    let mut w = world(Behind::Open);
    let (strong, weak) = face_off(&mut w, STRONG, WEAK);
    let rear = spawn(
        &mut w,
        0,
        EVEN,
        Fx::from_int(32),
        Fx::from_ratio(1696, 100),
        SOUTH,
    );
    w.tick(&[]);
    let start = [
        (strong, at(&w, strong)),
        (weak, at(&w, weak)),
        (rear, at(&w, rear)),
    ];
    for _ in 0..10 * 20 {
        w.tick(&[]);
        let r = w.regiments.get(rear).unwrap();
        assert!(r.engaged && r.charge == 0, "{r:?}");
    }
    let moved = moved_south(&w, &start);
    assert!(moved[1] > 2.0, "{moved:?}");
    assert!(
        moved.iter().all(|m| (m - moved[1]).abs() < 0.3),
        "{moved:?}"
    );
}

/// The weak block is driven south into whatever is behind it: it goes no
/// further than `room` tiles, and ends pinned in disorder (not always full:
/// when a rank falls the block steps up to keep its front, and has room
/// for a few ticks).
fn pinned_against(w: &mut World, room: f64) -> (RegimentHandle, RegimentHandle) {
    let (strong, weak) = face_off(w, STRONG, WEAK);
    let start = [(weak, at(w, weak))];
    seconds(w, 15);
    let moved = moved_south(w, &start)[0];
    assert!(
        moved > room - 0.6 && moved < room + 0.1,
        "{moved} of {room}"
    );
    let r = w.regiments.get(weak).unwrap();
    assert!(r.giving_ground);
    assert!(r.disorder > DISORDER_TICKS / 2, "{r:?}");
    (strong, weak)
}

#[test]
fn driven_into_a_friend_a_regiment_is_pinned_and_falls_into_disorder() {
    let mut w = world(Behind::Open);
    // Spear blocks are 2.55 tiles deep: 1.45 tiles of gap, then a tile of
    // overlap before it can go no further.
    let friend = spawn(&mut w, 0, EVEN, Fx::from_int(32), Fx::from_int(27), NORTH);
    pinned_against(&mut w, 2.45);
    assert_eq!(at(&w, friend), FxVec2::from_ints(32, 27));
}

#[test]
fn driven_to_a_cliff_edge_a_regiment_is_pinned() {
    // The block's rear is 1.275 tiles behind its centre at 23.
    let mut w = world(Behind::Cliff(27));
    pinned_against(&mut w, 27.0 - 24.275);
}

#[test]
fn driven_to_a_river_bank_a_regiment_is_pinned() {
    let mut w = world(Behind::River(27));
    pinned_against(&mut w, 27.0 - 24.275);
}

#[test]
fn a_pinned_regiment_loses_heart() {
    // No one falls in these fights: wounds cost a little morale, being
    // pinned much more.
    let morale = |behind: Behind| {
        let mut w = world(behind);
        let (_, sturdy) = face_off(&mut w, STRONG, STURDY);
        seconds(&mut w, 20);
        let r = w.regiments.get(sturdy).unwrap();
        assert_eq!(r.models, 24);
        r.morale_points
    };
    let free = morale(Behind::Open);
    let pinned = morale(Behind::Cliff(26));
    assert!((55..=60).contains(&free), "{free}");
    // Pinned after about four seconds, then 3 points a second at the
    // battle's pace (`gc_sim::combat::BATTLE_PACE_PERCENT`): at full pace,
    // 12 to 18 seconds pinned, 36 to 54 points.
    let lost =
        (i64::from(free) - i64::from(pinned)) * 100 / gc_sim::combat::BATTLE_PACE_PERCENT as i64;
    assert!((36..=54).contains(&lost), "{pinned}");
}

#[test]
fn a_regiment_with_its_back_already_over_the_edge_is_pinned_at_once() {
    // The rear edge (24.275) already overhangs the cliff or the water.
    for behind in [Behind::Cliff(24), Behind::River(24)] {
        let mut w = world(behind);
        pinned_against(&mut w, 0.0);
    }
}

#[test]
fn a_regiment_holds_when_the_foe_beating_it_is_losing_its_own_fight() {
    // A second strong block hits the strong one from behind: while the
    // strong block is losing overall it isn't pushing, and the weak one
    // holds instead of backing out of the fight. (Once the strong block
    // holds its own again, it drives the weak one back as usual.)
    let mut w = world(Behind::Open);
    let (strong, weak) = face_off(&mut w, STRONG, WEAK);
    spawn(
        &mut w,
        0,
        STRONG,
        Fx::from_int(32),
        Fx::from_ratio(1696, 100),
        SOUTH,
    );
    let mut losing_ticks = 0;
    for _ in 0..15 * 20 {
        let before = [(weak, at(&w, weak))];
        let losing = w.regiments.get(strong).unwrap().giving_ground;
        w.tick(&[]);
        if losing {
            losing_ticks += 1;
            assert!(moved_south(&w, &before)[0] <= 0.0);
        }
    }
    assert!(losing_ticks > 5 * 20);
}

#[test]
fn a_new_foe_starts_the_exchange_afresh() {
    let mut w = world(Behind::Open);
    let (strong, weak) = face_off(&mut w, STRONG, WEAK);
    seconds(&mut w, 6);
    assert!(w.regiments.get(weak).unwrap().giving_ground);
    // The strong block leaves; an equal one takes its place.
    let at_strong = at(&w, strong);
    w.tick(&[PlayerCommand {
        player: 1,
        command: Command::RemoveRegiment { regiment: strong },
    }]);
    spawn(&mut w, 1, WEAK, at_strong.x, at_strong.y, SOUTH);
    let start = [(weak, at(&w, weak))];
    seconds(&mut w, 4);
    assert!(!w.regiments.get(weak).unwrap().giving_ground);
    assert!(moved_south(&w, &start)[0].abs() < 0.3);
}
