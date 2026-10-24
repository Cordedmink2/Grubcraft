//! What the ground does in a fight: high ground, fords and woods
//! (docs/05-combat.md, "Terrain").

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::terrain::{Footing, HEIGHT_UNITS};
use gc_sim::{
    Capacities, Command, Cover, Ground, Mass, MatchSetup, PlayerCommand, RegimentHandle, UnitStats,
    World,
};

const SOUTH: Angle = Angle(0x4000);
const SPEARS: u16 = 0;
const CAVALRY: u16 = 1;
const ARCHERS: u16 = 2;
const MAP: u32 = 64;
/// Dry land two tiles up; the water line one tile up.
const LAND: u16 = 2 * HEIGHT_UNITS as u16;
const WATER_LEVEL: u16 = HEIGHT_UNITS as u16;

/// The drill units with morale off, so only the fighting is measured.
fn fearless() -> [UnitStats; 4] {
    DRILL_UNITS.map(|u| UnitStats { morale: 0, ..u })
}

/// One height per tile from `height(x, y)`.
fn heights(height: impl Fn(u32, u32) -> u16) -> Vec<u16> {
    (0..MAP * MAP).map(|i| height(i % MAP, i / MAP)).collect()
}

/// Land with a river across rows `y0..y1` (ground at the river bed, under water).
fn river(y0: u32, y1: u32) -> Vec<u16> {
    heights(|_, y| if (y0..y1).contains(&y) { 0 } else { LAND })
}

/// Land with a hill: rows above `edge` stand `rise` higher.
fn hill(edge: u32, rise: u16) -> Vec<u16> {
    heights(|_, y| if y < edge { LAND + rise } else { LAND })
}

/// Open ground with a wood over rows `y0..y1`.
fn wood(y0: u32, y1: u32) -> Vec<u8> {
    (0..MAP * MAP)
        .map(|i| {
            if (y0..y1).contains(&(i / MAP)) {
                Cover::Wood as u8
            } else {
                0
            }
        })
        .collect()
}

fn world_on(seed: u64, units: &[UnitStats], heights: &[u16]) -> World {
    world_with(seed, units, heights, &[])
}

fn world_with(seed: u64, units: &[UnitStats], heights: &[u16], cover: &[u8]) -> World {
    world_full(seed, units, heights, cover, &[])
}

fn world_full(
    seed: u64,
    units: &[UnitStats],
    heights: &[u16],
    cover: &[u8],
    footing: &[u8],
) -> World {
    World::new(&MatchSetup {
        seed,
        capacities: Capacities::default(),
        units,
        map_tiles: MAP,
        nav: &[],
        teams: &[],
        ground: Ground {
            heights,
            water_level: WATER_LEVEL,
            cover,
            growth: &[],
            footing,
        },
        champions: false,
        sky: gc_sim::Sky::STILL,
        // Generals off: these rules count wounds, and a general's extra hit
        // points would muddle the count (docs/37-commander.md).
        generals: false,
    })
}

/// Firm ground with soft ground (mud) over rows `y0..y1`.
fn mud(y0: u32, y1: u32) -> Vec<u8> {
    (0..MAP * MAP)
        .map(|i| u8::from((y0..y1).contains(&(i / MAP))) * Footing::Soft as u8)
        .collect()
}

/// Ground that climbs `per_tile` each tile northwards (towards y = 0).
fn ramp(per_tile: u16) -> Vec<u16> {
    heights(|_, y| LAND + (MAP - 1 - y) as u16 * per_tile)
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

fn move_to(w: &mut World, player: u8, regiment: RegimentHandle, x: Fx, y: Fx) {
    let stats = w.tick(&[PlayerCommand {
        player,
        command: Command::MoveTo {
            regiment,
            target: FxVec2::new(x, y),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
}

#[test]
fn flat_ground_has_no_terrain() {
    let w = world_on(1, &fearless(), &[]);
    assert!(w.terrain().is_flat());
    let p = FxVec2::from_ints(10, 10);
    assert!(!w.terrain().wet(p));
    assert_eq!(w.terrain().height(p), 0);
    // Heights of the wrong size are ignored rather than misread.
    let w = world_on(1, &fearless(), &[LAND; 10]);
    assert!(w.terrain().is_flat());
}

#[test]
fn wading_is_slow() {
    // How far spearmen march north in two seconds, starting on dry land
    // and starting in a river.
    let walked = |start_y: i32| {
        let mut w = world_on(1, &fearless(), &river(20, 40));
        let r = spawn(&mut w, 0, SPEARS, tile(32), tile(start_y), NORTH);
        move_to(&mut w, 0, r, tile(32), tile(start_y - 20));
        run(&mut w, 40);
        tile(start_y) - w.regiments.get(r).unwrap().position.y
    };
    let dry = walked(55);
    let wet = walked(35);
    // Three tiles a second on land; 60% of that in water.
    assert!(dry >= Fx::from_ratio(55, 10), "dry {dry:?}");
    assert!(
        wet * 10 <= dry * 7 && wet * 10 >= dry * 5,
        "wet {wet:?} vs dry {dry:?}"
    );
}

#[test]
fn woods_slow_the_march_and_heavy_troops_most() {
    // How far a regiment marches north in two seconds, starting in the
    // open and starting in a wood.
    let walked = |units: &[UnitStats], unit: u16, start_y: i32| {
        let mut w = world_with(1, units, &[], &wood(20, 40));
        let r = spawn(&mut w, 0, unit, tile(32), tile(start_y), NORTH);
        move_to(&mut w, 0, r, tile(32), tile(start_y - 20));
        run(&mut w, 40);
        tile(start_y) - w.regiments.get(r).unwrap().position.y
    };
    let units = fearless();
    let (open, woods) = (walked(&units, SPEARS, 60), walked(&units, SPEARS, 38));
    // 85% of the pace on foot.
    assert!(
        woods * 100 <= open * 88 && woods * 100 >= open * 80,
        "on foot: woods {woods:?} vs open {open:?}"
    );
    let heavy = units.map(|u| UnitStats {
        mass: Mass::Heavy,
        ..u
    });
    let (open, woods) = (walked(&heavy, CAVALRY, 60), walked(&heavy, CAVALRY, 38));
    // 70% for cavalry and monsters.
    assert!(
        woods * 100 <= open * 74 && woods * 100 >= open * 65,
        "heavy: woods {woods:?} vs open {open:?}"
    );
}

#[test]
fn trees_catch_arrows_shot_into_and_out_of_woods() {
    // Hit points a spear block loses to archers 20 tiles away in thirty
    // seconds, summed over seeds so the dice even out. A friend of the
    // archers stands just outside the wood to spot for them, and they are
    // told to shoot (hidden archers otherwise hold their fire).
    let wounds = |cover: &[u8]| -> u32 {
        (1..=6)
            .map(|seed| {
                let mut w = world_with(seed, &fearless(), &[], cover);
                let archers = spawn(&mut w, 0, ARCHERS, tile(32), tile(20), SOUTH);
                let target = spawn(&mut w, 1, SPEARS, tile(32), tile(40), NORTH);
                let _spotter = spawn(&mut w, 0, SPEARS, tile(32), tile(45), NORTH);
                w.tick(&[PlayerCommand {
                    player: 0,
                    command: Command::Attack {
                        regiment: archers,
                        target,
                    },
                }]);
                let before = hp(&w, target);
                run(&mut w, 20 * 30);
                before - hp(&w, target)
            })
            .sum()
    };
    let open = wounds(&[]);
    assert!(open > 0, "the archers hit nothing in the open");
    // 70% get through at each wooded end: 49% through both.
    for (what, cover, percent) in [
        ("into", wood(36, 44), 70),
        ("out of", wood(16, 24), 70),
        ("both", wood(16, 44), 49),
    ] {
        let (got, want) = (wounds(&cover), open * percent / 100);
        assert!(
            got * 100 >= want * 85 && got * 100 <= want * 115,
            "{what} the woods: {got} wounds, want about {want} ({open} in the open)"
        );
    }
}

/// Hit points a spear block standing at row `defender_y` loses in ten
/// seconds to another standing face to face with it at row `attacker_y`,
/// summed over a few seeds so the dice even out.
fn defender_losses(heights: &[u16], attacker_y: i32, defender_y: i32) -> u32 {
    let (towards, back) = if attacker_y < defender_y {
        (SOUTH, NORTH)
    } else {
        (NORTH, SOUTH)
    };
    (1..=6)
        .map(|seed| {
            let mut w = world_on(seed, &fearless(), heights);
            let d = spawn(&mut w, 1, SPEARS, tile(32), tile(defender_y), back);
            let _a = spawn(&mut w, 0, SPEARS, tile(32), tile(attacker_y), towards);
            let before = hp(&w, d);
            run(&mut w, 20 * 10);
            before - hp(&w, d)
        })
        .sum()
}

#[test]
fn regiments_in_water_take_more_melee_damage() {
    // The defender stands in a river (rows 36 to 39), the attacker on the
    // bank just south of it.
    let dry = defender_losses(&heights(|_, _| LAND), 40, 37);
    let wet = defender_losses(&river(30, 40), 40, 37);
    // +20% a blow; over ten seconds it compounds a little, as the wading
    // block loses soldiers and hits back with fewer.
    assert!(
        wet * 100 >= dry * 112 && wet * 100 <= dry * 145,
        "wet {wet} vs dry {dry}"
    );
}

#[test]
fn fighting_downhill_hits_harder() {
    // The attacker stands on a hill a tile higher (rows below 40) and the
    // defender at its foot.
    let flat = defender_losses(&heights(|_, _| LAND), 38, 41);
    let downhill = defender_losses(&hill(40, HEIGHT_UNITS as u16), 38, 41);
    assert!(
        downhill * 100 >= flat * 104 && downhill * 100 <= flat * 116,
        "downhill {downhill} vs flat {flat}"
    );
    // Uphill gives the attacker nothing extra (the defender, now above it,
    // hits harder).
    let uphill = defender_losses(&hill(40, HEIGHT_UNITS as u16), 41, 38);
    assert!(uphill * 100 <= flat * 104, "uphill {uphill} vs flat {flat}");
    // A gentle rise, under half a tile, doesn't count as high ground.
    let gentle = defender_losses(&hill(40, HEIGHT_UNITS as u16 / 4), 38, 41);
    assert_eq!(gentle, flat);
}

#[test]
fn archers_on_high_ground_shoot_further() {
    // Archers reach 25 tiles. Their target stands 27 tiles away.
    let wounds = |heights: &[u16]| {
        let mut w = world_on(3, &fearless(), heights);
        let _archers = spawn(&mut w, 0, ARCHERS, tile(32), tile(10), SOUTH);
        let target = spawn(&mut w, 1, SPEARS, tile(32), tile(37), NORTH);
        let before = hp(&w, target);
        run(&mut w, 20 * 10);
        before - hp(&w, target)
    };
    assert_eq!(wounds(&heights(|_, _| LAND)), 0, "out of range on the flat");
    assert!(
        wounds(&hill(20, HEIGHT_UNITS as u16)) > 0,
        "in range from the hill"
    );
}

#[test]
fn nobody_charges_out_of_a_river() {
    // Cavalry with a huge charge bonus run at spearmen on the far bank.
    // Starting in the river, it reaches them still wading and the charge
    // is lost.
    let mut units = fearless();
    units[CAVALRY as usize].charge_bonus = 250;
    let mut no_charge = units;
    no_charge[CAVALRY as usize].charge_bonus = 0;
    let first_blows = |units: &[UnitStats], heights: &[u16]| -> u32 {
        (1..=6)
            .map(|seed| {
                let mut w = world_on(seed, units, heights);
                let d = spawn(&mut w, 1, SPEARS, tile(32), tile(28), SOUTH);
                let a = spawn(&mut w, 0, CAVALRY, tile(32), tile(40), NORTH);
                w.tick(&[PlayerCommand {
                    player: 0,
                    command: Command::Attack {
                        regiment: a,
                        target: d,
                    },
                }]);
                let before = hp(&w, d);
                let mut ticks = 0;
                while !w.regiments.get(a).unwrap().engaged {
                    w.tick(&[]);
                    ticks += 1;
                    assert!(ticks < 20 * 20, "never reached them");
                }
                // The charge bonus fades over three seconds: three blows.
                run(&mut w, 60);
                before - hp(&w, d)
            })
            .sum()
    };
    let land = heights(|_, _| LAND);
    let dry = first_blows(&units, &land);
    let plain = first_blows(&no_charge, &land);
    let wet = first_blows(&units, &river(31, 45));
    assert!(
        dry * 10 > plain * 13,
        "the charge on land: {dry} vs {plain}"
    );
    assert!(
        wet * 10 < plain * 12,
        "wading cavalry still charged: {wet} vs {plain} without a charge"
    );
}

/// How far a regiment marches in two seconds from `start_y`, north if
/// `north`, else south.
fn marched(w: &mut World, unit: u16, start_y: i32, north: bool) -> Fx {
    let (facing, to) = if north {
        (NORTH, start_y - 20)
    } else {
        (SOUTH, start_y + 20)
    };
    let r = spawn(w, 0, unit, tile(32), tile(start_y), facing);
    move_to(w, 0, r, tile(32), tile(to));
    run(w, 40);
    (tile(start_y) - w.regiments.get(r).unwrap().position.y).abs()
}

#[test]
fn climbing_is_slow_and_going_down_is_not() {
    let flat = marched(&mut world_on(1, &fearless(), &ramp(0)), SPEARS, 50, true);
    // Half a tile of height a tile: a quarter of the pace lost climbing.
    let half = HEIGHT_UNITS as u16 / 2;
    let up = marched(&mut world_on(1, &fearless(), &ramp(half)), SPEARS, 50, true);
    assert!(
        up * 100 <= flat * 79 && up * 100 >= flat * 71,
        "up {up:?} vs flat {flat:?}"
    );
    // A cliff-steep climb loses at most 40%.
    let steep = marched(
        &mut world_on(1, &fearless(), &ramp(4 * half)),
        SPEARS,
        50,
        true,
    );
    assert!(
        steep * 100 >= flat * 57 && steep * 100 <= flat * 64,
        "steep {steep:?} vs flat {flat:?}"
    );
    let down = marched(
        &mut world_on(1, &fearless(), &ramp(half)),
        SPEARS,
        20,
        false,
    );
    assert!(down * 100 >= flat * 98, "down {down:?} vs flat {flat:?}");
}

#[test]
fn mud_is_slow_and_heavy_troops_slowest() {
    let units = fearless();
    let on = |units: &[UnitStats], unit, footing: &[u8]| {
        marched(&mut world_full(1, units, &[], &[], footing), unit, 38, true)
    };
    let (firm, soft) = (on(&units, SPEARS, &[]), on(&units, SPEARS, &mud(10, 40)));
    assert!(
        soft * 100 <= firm * 78 && soft * 100 >= firm * 72,
        "on foot: mud {soft:?} vs firm {firm:?}"
    );
    let heavy = units.map(|u| UnitStats {
        mass: Mass::Heavy,
        ..u
    });
    let (firm, soft) = (on(&heavy, CAVALRY, &[]), on(&heavy, CAVALRY, &mud(10, 40)));
    assert!(
        soft * 100 <= firm * 63 && soft * 100 >= firm * 57,
        "heavy: mud {soft:?} vs firm {firm:?}"
    );
}

/// Hit points spearmen at row 28 lose in the first three seconds after
/// cavalry starting at `cavalry_y` hits home, over six seeds.
fn charged(units: &[UnitStats], heights: &[u16], footing: &[u8], cavalry_y: i32) -> u32 {
    (1..=6)
        .map(|seed| {
            let mut w = world_full(seed, units, heights, &[], footing);
            let d = spawn(&mut w, 1, SPEARS, tile(32), tile(28), SOUTH);
            let a = spawn(&mut w, 0, CAVALRY, tile(32), tile(cavalry_y), NORTH);
            w.tick(&[PlayerCommand {
                player: 0,
                command: Command::Attack {
                    regiment: a,
                    target: d,
                },
            }]);
            let before = hp(&w, d);
            let mut ticks = 0;
            while !w.regiments.get(a).unwrap().engaged {
                w.tick(&[]);
                ticks += 1;
                assert!(ticks < 20 * 20, "never reached them");
            }
            run(&mut w, 60);
            before - hp(&w, d)
        })
        .sum()
}

/// The drill units, fearless, with cavalry charging for `bonus`.
fn chargers(bonus: u8) -> [UnitStats; 4] {
    let mut units = fearless();
    units[CAVALRY as usize].charge_bonus = bonus;
    units
}

#[test]
fn nobody_charges_through_mud() {
    let land = heights(|_, _| LAND);
    let plain = charged(&chargers(0), &land, &[], 40);
    let firm = charged(&chargers(250), &land, &[], 40);
    assert!(firm * 10 > plain * 13, "the charge: {firm} vs {plain}");
    // The spearmen stand in mud: the charge bogs down.
    let into = charged(&chargers(250), &land, &mud(25, 31), 40);
    assert!(
        into * 10 < plain * 12,
        "charged into mud: {into} vs {plain}"
    );
    // The cavalry starts in mud and is still in it when it hits home.
    let out = charged(&chargers(250), &land, &mud(29, 45), 40);
    assert!(
        out * 10 < plain * 12,
        "charged out of mud: {out} vs {plain}"
    );
}

#[test]
fn charges_from_high_ground_hit_harder_and_up_onto_it_softer() {
    // Cavalry charges north from row 40 at spearmen on row 28; rows north of
    // 31 stand a tile higher (uphill for the cavalry) or lower (downhill).
    let up = hill(31, HEIGHT_UNITS as u16);
    let down = heights(|_, y| {
        if y < 31 {
            LAND
        } else {
            LAND + HEIGHT_UNITS as u16
        }
    });
    let land = heights(|_, _| LAND);
    let bonus = |heights: &[u16]| {
        charged(&chargers(100), heights, &[], 40) - charged(&chargers(0), heights, &[], 40)
    };
    let (flat, downhill, uphill) = (bonus(&land), bonus(&down), bonus(&up));
    assert!(
        downhill * 10 > flat * 12,
        "downhill {downhill} vs flat {flat}"
    );
    assert!(uphill * 10 < flat * 8, "uphill {uphill} vs flat {flat}");
}

#[test]
fn terrain_is_deterministic() {
    let fight = || {
        let mut w = world_on(9, &fearless(), &river(30, 40));
        let a = spawn(&mut w, 0, SPEARS, tile(32), tile(44), NORTH);
        let d = spawn(&mut w, 1, SPEARS, tile(32), tile(24), SOUTH);
        w.tick(&[PlayerCommand {
            player: 0,
            command: Command::Attack {
                regiment: a,
                target: d,
            },
        }]);
        run(&mut w, 20 * 30);
        w.state_hash()
    };
    assert_eq!(fight(), fight());
}
