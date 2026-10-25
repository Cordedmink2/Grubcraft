//! The faction twists of the top wall level (docs/26-fortifications.md,
//! stage 4c): stones and loopholes on the Ember Castle Wall, the Concord
//! Mausoleum Wall mended by the dead, the Choir Singing Wall slowing those
//! who strike it, and the Brood Thorn Bulwark hurting them.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::events::Event;
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, LINE, Role, WallTwist};
use gc_sim::twists::{STONE_HP, STONE_TICKS, THORN_HP};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, Cover, Ground, Growth,
    MatchSetup, PlayerCommand, RegimentHandle, StructureHandle, TICKS_PER_SECOND, UnitStats, World,
};

const FOOT: u16 = 1;
const ARCHERS: u16 = 2;
const HALL: u16 = 1;
/// The level 2 walls, one per twist (and one with none), in
/// [`WallTwist`] order.
const WALLS: [u16; 5] = [2, 3, 4, 5, 6];
const MAP: u32 = 64;

/// Open ground; each side has a hall, and side 0 has a three-tile wall of
/// each twist (and one with none) running north to south at x = 10, 20,
/// 30, 40 and 50, from y = 20 to 22.
fn world() -> World {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let n = MAP as usize;
    let cover = vec![Cover::Open as u8; n * n];
    let growth = vec![Growth::Open as u8; n * n];
    let worker = UnitStats {
        models: 1,
        files: 1,
        ..DRILL_UNITS[0]
    };
    let mut w = World::new(&MatchSetup {
        seed: 7,
        capacities: Capacities { regiments: 16 },
        units: &[worker, DRILL_UNITS[0], DRILL_UNITS[2]],
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: Ground {
            heights: &[],
            water_level: 0,
            cover: &cover,
            growth: &growth,
            footing: &[],
            flow: &[],
        },
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: false,
    });
    let palisade = BuildingStats {
        role: Role::Wall,
        tier: 1,
        upgrades_from: None,
        cost_grub: 10,
        cost_marrow: 0,
        build_ticks: 40,
        hp: 250,
        armour: Armour::Fortified,
        width: 1,
        depth: 1,
        supply: 0,
        flags: LINE,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
        twist: WallTwist::None,
        trap: gc_sim::structures::Trap::None,
    };
    let level = |twist| BuildingStats {
        upgrades_from: Some(0),
        hp: 1000,
        twist,
        ..palisade
    };
    let soldier = ConquestUnit {
        mans_walls: true,
        ..ConquestUnit::default()
    };
    assert!(w.conquest(&ConquestSetup {
        buildings: &[
            palisade,
            BuildingStats {
                role: Role::Main,
                width: 3,
                depth: 3,
                flags: 0,
                ..palisade
            },
            level(WallTwist::None),
            level(WallTwist::Stones),
            level(WallTwist::Mends),
            level(WallTwist::Sings),
            level(WallTwist::Thorns),
        ],
        units: &[
            ConquestUnit {
                worker: true,
                ..ConquestUnit::default()
            },
            soldier,
            soldier,
        ],
        structures: 1024,
        ..ConquestSetup::default()
    }));
    for (player, tile) in [(0, (2, 2)), (1, (58, 58))] {
        spawn_structure(&mut w, player, HALL, tile);
    }
    for (k, &b) in WALLS.iter().enumerate() {
        for y in 20..23 {
            spawn_structure(&mut w, 0, b, (wall_x(k) as i16, y));
        }
    }
    w
}

/// The column of the wall with twist `k` (in [`WallTwist`] order).
fn wall_x(k: usize) -> i32 {
    10 + 10 * k as i32
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn spawn_structure(w: &mut World, player: u8, building: u16, tile: (i16, i16)) {
    assert!(order(
        w,
        player,
        Command::SpawnStructure {
            building,
            tile,
            turn: 0,
        }
    ));
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing: if player == 0 { NORTH } else { Angle(0x4000) },
        }
    ));
    (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .unwrap()
}

fn tile(w: &World, at: (i32, i32)) -> StructureHandle {
    let s = w.structures().unwrap();
    s.iter().find(|v| v.tile == at).unwrap().handle
}

fn wall_hp(w: &World, h: StructureHandle) -> u32 {
    let s = w.structures().unwrap();
    s.iter().find(|v| v.handle == h).map_or(0, |v| v.hp)
}

/// Hit points left in regiment `h`, every soldier together.
fn hp(w: &World, h: RegimentHandle) -> u32 {
    w.regiments
        .model_hp(h)
        .map_or(0, |hp| hp.iter().map(|&v| u32::from(v)).sum())
}

/// Side 1's `unit` regiment, east of the wall with twist `k`, sent to
/// attack its middle tile.
fn attacker(w: &mut World, k: usize, unit: u16) -> (RegimentHandle, StructureHandle) {
    let x = wall_x(k);
    let r = spawn(w, 1, unit, x + 5, 21);
    let structure = tile(w, (x, 21));
    assert!(order(
        w,
        1,
        Command::AttackStructure {
            regiment: r,
            structure,
        }
    ));
    (r, structure)
}

/// What `ticks` ticks tell: the blows regiment `r` struck on buildings,
/// and the twists that answered, by twist.
fn watch(w: &mut World, r: RegimentHandle, ticks: u32) -> (u32, Vec<WallTwist>) {
    let (mut blows, mut twists) = (0, Vec::new());
    for _ in 0..ticks {
        w.tick(&[]);
        for e in w.events() {
            match *e {
                Event::Struck { by, .. } if by == r => blows += 1,
                Event::Twist { twist, on, .. } if on.is_none_or(|on| on == r) => twists.push(twist),
                _ => {}
            }
        }
    }
    (blows, twists)
}

#[test]
fn a_plain_wall_does_nothing_to_those_who_strike_it() {
    let mut w = world();
    let (r, _) = attacker(&mut w, 0, FOOT);
    let full = hp(&w, r);
    let (blows, twists) = watch(&mut w, r, 20 * TICKS_PER_SECOND);
    assert!(blows > 5, "{blows}");
    assert!(twists.is_empty());
    assert_eq!(hp(&w, r), full);
}

#[test]
fn stones_fall_on_those_striking_a_castle_wall() {
    let mut w = world();
    let (r, _) = attacker(&mut w, 1, FOOT);
    let full = hp(&w, r);
    let (blows, twists) = watch(&mut w, r, 20 * TICKS_PER_SECOND);
    let stones = twists.len() as u32;
    assert!(twists.iter().all(|&t| t == WallTwist::Stones));
    // No more than one every few seconds, however many blows.
    assert!(
        blows > stones && stones >= 3,
        "{blows} blows, {stones} stones"
    );
    assert!(stones <= 20 * TICKS_PER_SECOND / u32::from(STONE_TICKS) + 1);
    assert_eq!(hp(&w, r), full - stones * STONE_HP);
}

#[test]
fn stones_miss_archers_shooting_from_afar() {
    let mut w = world();
    let (r, _) = attacker(&mut w, 1, ARCHERS);
    let full = hp(&w, r);
    let (blows, twists) = watch(&mut w, r, 20 * TICKS_PER_SECOND);
    assert!(blows > 0);
    assert!(twists.is_empty());
    assert_eq!(hp(&w, r), full);
}

#[test]
fn every_melee_blow_on_a_thorn_bulwark_costs_the_striker() {
    let mut w = world();
    let (r, _) = attacker(&mut w, 4, FOOT);
    let full = hp(&w, r);
    let (blows, twists) = watch(&mut w, r, 20 * TICKS_PER_SECOND);
    assert!(blows > 5, "{blows}");
    assert_eq!(twists.len() as u32, blows);
    assert!(twists.iter().all(|&t| t == WallTwist::Thorns));
    assert_eq!(hp(&w, r), full - blows * THORN_HP);
}

#[test]
fn a_singing_wall_slows_those_who_strike_it() {
    let blows = |k| {
        let mut w = world();
        let (r, _) = attacker(&mut w, k, FOOT);
        let (blows, _) = watch(&mut w, r, 40 * TICKS_PER_SECOND);
        let sung = w.regiments.iter().find(|v| v.handle == r).unwrap().sung;
        (blows, sung)
    };
    let (plain, plain_sung) = blows(0);
    let (sung, slowed) = blows(3);
    assert!(!plain_sung && slowed);
    // Half as many blows, give or take the walk there.
    assert!(sung * 3 < plain * 2, "{sung} against {plain}");
    assert!(sung * 3 > plain, "{sung} against {plain}");
}

#[test]
fn a_regiment_a_singing_wall_slowed_walks_slower() {
    let walked = |k| {
        let mut w = world();
        let (r, _) = attacker(&mut w, k, FOOT);
        watch(&mut w, r, 20 * TICKS_PER_SECOND);
        let from = w.regiments.iter().find(|v| v.handle == r).unwrap().position;
        let to = from + FxVec2::from_ints(10, 0);
        assert!(order(
            &mut w,
            1,
            Command::MoveTo {
                regiment: r,
                target: to,
                facing: None,
                files: 0,
                pace: Fx::ZERO,
            }
        ));
        for _ in 0..3 * TICKS_PER_SECOND {
            w.tick(&[]);
        }
        let at = w.regiments.iter().find(|v| v.handle == r).unwrap().position;
        (at - from).length()
    };
    let (plain, sung) = (walked(0), walked(3));
    assert!(
        sung < plain * Fx::from_ratio(4, 5),
        "{sung:?} against {plain:?}"
    );
    assert!(sung > Fx::ZERO);
}

#[test]
fn the_dead_at_its_foot_mend_a_mausoleum_wall() {
    let mut w = world();
    let (r, wall) = attacker(&mut w, 2, FOOT);
    watch(&mut w, r, 30 * TICKS_PER_SECOND);
    let hurt = wall_hp(&w, wall);
    assert!(hurt < 1000, "{hurt}");
    // Side 0 comes out to fight them at the foot of the wall.
    let x = wall_x(2);
    let defender = spawn(&mut w, 0, FOOT, x + 3, 18);
    assert!(order(
        &mut w,
        0,
        Command::Attack {
            regiment: defender,
            target: r,
        }
    ));
    let (_, twists) = watch(&mut w, r, 30 * TICKS_PER_SECOND);
    let mended = twists.iter().filter(|&&t| t == WallTwist::Mends).count();
    assert!(mended > 0);
    assert!(
        wall_hp(&w, wall) > hurt,
        "{} after {hurt}",
        wall_hp(&w, wall)
    );
}

#[test]
fn archers_on_a_castle_wall_shoot_through_loopholes() {
    // Ticks between volleys, on average.
    let reload = |k| {
        let mut w = world();
        let x = wall_x(k);
        let archers = spawn(&mut w, 0, ARCHERS, x - 3, 21);
        let structure = tile(&w, (x, 21));
        assert!(order(
            &mut w,
            0,
            Command::ManWall {
                regiment: archers,
                structure,
            }
        ));
        for _ in 0..30 * TICKS_PER_SECOND {
            if w.regiments.iter().any(|v| v.handle == archers && v.on_wall) {
                break;
            }
            w.tick(&[]);
        }
        assert!(w.regiments.iter().any(|v| v.handle == archers && v.on_wall));
        // A target in reach that can't come at them.
        spawn(&mut w, 1, FOOT, x + 6, 21);
        let mut loosed = Vec::new();
        for _ in 0..40 * TICKS_PER_SECOND {
            w.tick(&[]);
            for e in w.events() {
                if let Event::Loosed { shooter, .. } = *e
                    && shooter.regiment() == Some(archers)
                {
                    loosed.push(w.tick_count());
                }
            }
        }
        assert!(loosed.len() > 4, "{loosed:?}");
        (loosed[loosed.len() - 1] - loosed[0]) / (loosed.len() as u32 - 1)
    };
    let (plain, loopholes) = (reload(0), reload(1));
    // A quarter faster: five volleys in the time of four.
    assert!(
        loopholes * 5 <= plain * 4 + 5,
        "{loopholes} against {plain}"
    );
    assert!(
        loopholes * 5 + 5 >= plain * 4,
        "{loopholes} against {plain}"
    );
}
