//! Towers shoot (docs/26-fortifications.md, stage 1): a finished tower
//! looses its shots at the enemy it should, a Tollgate earns from what it
//! hits, and a Bell Tower tolls when enemies come near.

use gc_fixed::{Fx, FxVec2};
use gc_sim::events::Event;
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, Breaker, Role, SILENCES, TOLL, TowerAttack};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MatchSetup, PlayerCommand,
    RegimentHandle, Shooter, StructureHandle, TICKS_PER_SECOND, World,
};

const SPEARS: u16 = 0;
/// A regiment whose blows fall on buildings as a ram's.
const RAM: u16 = 1;

const TOWER: u16 = 0;
const FARMSTEAD: u16 = 1;
const TOLLGATE: u16 = 2;
const BELL: u16 = 3;

const ATTACK: TowerAttack = TowerAttack {
    range: Fx::from_int(30),
    damage: 5,
    reload_ticks: 2 * TICKS_PER_SECOND as u16,
    shots: 2,
};

fn building(role: Role, side: u8, flags: u16, attack: Option<TowerAttack>) -> BuildingStats {
    BuildingStats {
        role,
        tier: 1,
        upgrades_from: None,
        cost_grub: 100,
        cost_marrow: 0,
        build_ticks: 600,
        hp: 1000,
        armour: Armour::Fortified,
        width: side,
        depth: side,
        supply: 0,
        flags,
        attack,
        field: 0,
        tunes: None,
        links: 0,
    }
}

fn buildings() -> [BuildingStats; 4] {
    [
        building(Role::Defence, 2, 0, Some(ATTACK)),
        building(Role::Housing, 3, 0, None),
        building(Role::Defence, 2, TOLL, Some(ATTACK)),
        building(Role::Defence, 2, SILENCES, Some(ATTACK)),
    ]
}

/// An open map 96 tiles across, in Conquest; unit 1 is a siege engine.
fn conquest() -> World {
    let tiles = 96;
    let side = tiles as usize * CELLS_PER_TILE as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        seed: 3,
        capacities: Capacities { regiments: 64 },
        units: &DRILL_UNITS,
        map_tiles: tiles,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
    });
    let units = [
        ConquestUnit::default(),
        ConquestUnit {
            breaker: Breaker::Siege,
            ..ConquestUnit::default()
        },
    ];
    assert!(w.conquest(&ConquestSetup {
        buildings: &buildings(),
        units: &units,
        structures: 32,
        ..ConquestSetup::default()
    }));
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn put_up(w: &mut World, player: u8, building: u16, x: i16, y: i16) -> StructureHandle {
    assert!(order(
        w,
        player,
        Command::SpawnStructure {
            building,
            tile: (x, y),
            turn: 0,
        }
    ));
    let s = w.structures().unwrap();
    s.iter()
        .find(|v| v.tile == (i32::from(x), i32::from(y)))
        .unwrap()
        .handle
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing: NORTH,
        }
    ));
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn models(w: &World, h: RegimentHandle) -> u16 {
    w.regiments.get(h).map_or(0, |r| r.models)
}

/// The regiments the tower `tower` loosed at in the next `ticks`.
fn targets(w: &mut World, tower: StructureHandle, ticks: u32) -> Vec<RegimentHandle> {
    let mut out = Vec::new();
    for _ in 0..ticks {
        w.tick(&[]);
        for e in w.events() {
            if let Event::Loosed {
                shooter: Shooter::Tower(t),
                target,
                ..
            } = *e
                && t == tower
            {
                out.push(target);
            }
        }
    }
    out
}

#[test]
fn a_tower_shoots_the_enemy_in_range_and_not_the_one_beyond() {
    let mut w = conquest();
    let tower = put_up(&mut w, 1, TOWER, 40, 40);
    // Its centre is (41, 41): one regiment 20 tiles off, one 40.
    let near = spawn(&mut w, 0, SPEARS, 41, 61);
    let far = spawn(&mut w, 0, SPEARS, 81, 41);
    let (near_before, far_before) = (models(&w, near), models(&w, far));
    let shot = targets(&mut w, tower, 20 * TICKS_PER_SECOND);
    assert!(!shot.is_empty(), "never shot");
    assert!(shot.iter().all(|&t| t == near), "shot out of range");
    assert!(models(&w, near) < near_before, "no one fell");
    assert_eq!(models(&w, far), far_before);
    // A volley every two seconds.
    assert!((9..=11).contains(&shot.len()), "{} volleys", shot.len());
}

#[test]
fn a_tower_shoots_from_its_middle_and_every_shot_lands() {
    let mut w = conquest();
    put_up(&mut w, 1, TOWER, 40, 40);
    let r = spawn(&mut w, 0, SPEARS, 41, 61);
    let hp = |w: &World| {
        let v = w.regiments.get(r).unwrap();
        u32::from(v.models) * 10
    };
    let mut seen_from = None;
    let before = hp(&w);
    // Ten volleys of 2 × 5, slowed to 85%: about 85 hit points, so about 8
    // of its 10-point soldiers.
    for _ in 0..20 * TICKS_PER_SECOND {
        w.tick(&[]);
        if let Some(v) = w.volleys().next() {
            seen_from = Some(v.from);
        }
    }
    assert_eq!(seen_from, Some(FxVec2::from_ints(41, 41)));
    let lost = before - hp(&w);
    assert!((60..=110).contains(&lost), "{lost} hit points");
}

#[test]
fn siege_engines_come_first_then_those_attacking_its_side() {
    let mut w = conquest();
    let tower = put_up(&mut w, 1, TOWER, 40, 40);
    let farm = put_up(&mut w, 1, FARMSTEAD, 52, 40);
    // The nearest stands idle; one further off attacks the farm.
    let idle = spawn(&mut w, 0, SPEARS, 41, 52);
    let raider = spawn(&mut w, 0, SPEARS, 60, 50);
    assert!(order(
        &mut w,
        0,
        Command::AttackStructure {
            regiment: raider,
            structure: farm
        }
    ));
    let shot = targets(&mut w, tower, 6 * TICKS_PER_SECOND);
    assert!(!shot.is_empty());
    assert!(shot.iter().all(|&t| t == raider), "{shot:?}");
    assert!(!shot.contains(&idle));
    // A ram further still comes before both.
    let ram = spawn(&mut w, 0, RAM, 41, 68);
    let shot = targets(&mut w, tower, 6 * TICKS_PER_SECOND);
    assert!(!shot.is_empty());
    assert!(shot.iter().all(|&t| t == ram), "{shot:?}");
}

#[test]
fn its_owner_can_tell_it_whom_to_shoot() {
    let mut w = conquest();
    let tower = put_up(&mut w, 1, TOWER, 40, 40);
    let _near = spawn(&mut w, 0, SPEARS, 41, 50);
    let far = spawn(&mut w, 0, SPEARS, 41, 66);
    let mine = spawn(&mut w, 1, SPEARS, 60, 30);
    let focus = |target| Command::FocusTower {
        structure: tower,
        target,
    };
    // Not on its own side's regiment, and not someone else's tower.
    assert!(!order(&mut w, 1, focus(Some(mine))));
    assert!(!order(&mut w, 0, focus(Some(far))));
    assert!(order(&mut w, 1, focus(Some(far))));
    assert_eq!(w.structures().unwrap().focus_of(tower), Some(far));
    let shot = targets(&mut w, tower, 6 * TICKS_PER_SECOND);
    assert!(!shot.is_empty());
    assert!(shot.iter().all(|&t| t == far), "{shot:?}");
    // Let go, it shoots the nearest again.
    assert!(order(&mut w, 1, focus(None)));
    let shot = targets(&mut w, tower, 6 * TICKS_PER_SECOND);
    assert!(shot.iter().all(|&t| t != far), "{shot:?}");
}

#[test]
fn a_tollgate_earns_a_grub_for_every_four_hit_points_it_takes() {
    let mut w = conquest();
    put_up(&mut w, 1, TOLLGATE, 40, 40);
    let r = spawn(&mut w, 0, SPEARS, 41, 61);
    let grub = |w: &World| w.bank(1).unwrap().grub;
    let hp = |w: &World| u32::from(w.regiments.get(r).map_or(0, |v| v.models)) * 10;
    let (grub_before, hp_before) = (grub(&w), hp(&w));
    for _ in 0..20 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    let taken = hp_before - hp(&w);
    let earned = grub(&w) - grub_before;
    assert!(earned > 0);
    // Soldiers' hit points are whole: a soldier hurt but standing counts
    // as untouched here, so the toll can run ahead by one soldier's worth.
    assert!(
        earned * 4 >= taken && earned * 4 <= taken + 10 + 4,
        "earned {earned} for {taken}"
    );
}

#[test]
fn a_bell_tower_tolls_when_an_enemy_comes_near_and_silences_powers_there() {
    let mut w = conquest();
    let bell = put_up(&mut w, 1, BELL, 40, 40);
    // Nothing near: no toll, no silence.
    for _ in 0..5 * TICKS_PER_SECOND {
        w.tick(&[]);
        assert!(!w.events().iter().any(|e| matches!(e, Event::Tolled { .. })));
    }
    let spot = FxVec2::from_ints(41, 50);
    assert!(!w.silenced(0, spot));
    // It tolls in the tick the enemy arrives.
    spawn(&mut w, 0, SPEARS, 41, 50);
    let tolled = |w: &World| w.events().contains(&Event::Tolled { structure: bell });
    assert!(tolled(&w));
    assert!(w.silenced(0, spot), "silenced near it");
    assert!(!w.silenced(0, FxVec2::from_ints(41, 70)), "not far off");
    assert!(!w.silenced(1, spot), "not for its own side");
    let mut tolls = vec![0];
    for t in 1..=30 * TICKS_PER_SECOND {
        w.tick(&[]);
        if tolled(&w) {
            tolls.push(t);
        }
    }
    // Once on arrival, then once in each 20 s it stays.
    assert_eq!(tolls, [0, 20 * TICKS_PER_SECOND], "{tolls:?}");
    // The silence lasts 5 s.
    let left = w.structures().unwrap().silence_left(bell);
    assert_eq!(left, 0);
}
