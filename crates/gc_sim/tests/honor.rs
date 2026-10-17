//! Honor: the Ember Clans' meter (docs/03-factions.md). Honor is spent on
//! Edicts (Rice Tax, Forced March, Banner of the Ancestors), earned by
//! Shrines and by routs, and lost when one of the Clans' own regiments
//! routs.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::honor::{FLED_HONOR, OUTNUMBERED_HONOR, ROUT_HONOR};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, HONOR as SHRINE_FLAG, Role};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MAX_PARTIES, MatchSetup,
    Meter, PlayerCommand, PowerEffect, PowerStats, PowerTarget, Refusal, RegimentHandle,
    StructureHandle, TICKS_PER_SECOND, TickStats, UnitStats, World,
};

const HALL: u16 = 0;
/// A Shrine. Put up with [`put_up`] it stands finished at once; laid as a
/// foundation by a worker it takes so long to build that it stays unfinished.
const SHRINE: u16 = 1;

/// Drill unit 0 as a worker: one soldier, which gathers and builds.
const WORKER: u16 = 0;
/// Drill unit 0's spearmen, fearless: they never rout, so only their
/// victims break.
const SPEARS: u16 = 1;
/// Drill unit 1's skitterlings, who rout.
const SKITTERS: u16 = 2;
/// Eight fearless spearmen: a smaller force than the skitterlings.
const SQUAD: u16 = 3;

/// The powers table: three Edicts paid from Honor, and one paid from
/// Resonance.
const RICE_TAX: u8 = 0;
const FORCED_MARCH: u8 = 1;
const BANNER: u8 = 2;
const OVERCLOCK: u8 = 3;

const POWERS: [PowerStats; 4] = [
    PowerStats {
        tier: 1,
        cost: 20,
        meter: Meter::Honor,
        cooldown_ticks: 1200,
        duration_ticks: 0,
        effect: PowerEffect::Grant { grub: 30 },
    },
    PowerStats {
        tier: 1,
        cost: 10,
        meter: Meter::Honor,
        cooldown_ticks: 1200,
        duration_ticks: 200,
        effect: PowerEffect::March { percent: 50 },
    },
    PowerStats {
        tier: 1,
        cost: 30,
        meter: Meter::Honor,
        cooldown_ticks: 1200,
        duration_ticks: 2400,
        effect: PowerEffect::Steadfast { tiles: 8 },
    },
    PowerStats {
        tier: 1,
        cost: 20,
        meter: Meter::Resonance,
        cooldown_ticks: 400,
        duration_ticks: 400,
        effect: PowerEffect::Haste { percent: 50 },
    },
];

/// Facing south, as the morale tests have their skitterlings.
const SOUTH: Angle = Angle(0x4000);
/// Facing east, the way the marches run.
const EAST: Angle = Angle(0);

fn units() -> [UnitStats; 4] {
    let worker = UnitStats {
        models: 1,
        files: 1,
        turn_rate: 180 * 65_536 / 360,
        ..DRILL_UNITS[0]
    };
    [
        worker,
        UnitStats {
            morale: 0,
            ..DRILL_UNITS[0]
        },
        DRILL_UNITS[1],
        UnitStats {
            models: 8,
            files: 4,
            morale: 0,
            ..DRILL_UNITS[0]
        },
    ]
}

fn conquest_units() -> [ConquestUnit; 4] {
    let worker = ConquestUnit {
        worker: true,
        supply: 1,
        ..ConquestUnit::default()
    };
    [
        worker,
        ConquestUnit::default(),
        ConquestUnit::default(),
        ConquestUnit::default(),
    ]
}

fn buildings() -> [BuildingStats; 2] {
    [
        BuildingStats {
            role: Role::Main,
            tier: 1,
            upgrades_from: None,
            cost_grub: 400,
            cost_marrow: 0,
            build_ticks: 1000,
            hp: 3000,
            armour: Armour::Living,
            width: 5,
            depth: 5,
            supply: 40,
            flags: 0,
            attack: None,
            field: 0,
            tunes: None,
            links: 0,
        },
        BuildingStats {
            role: Role::Economy,
            tier: 1,
            upgrades_from: None,
            cost_grub: 60,
            cost_marrow: 0,
            build_ticks: u16::MAX,
            hp: 1500,
            armour: Armour::Structure,
            width: 2,
            depth: 2,
            supply: 0,
            flags: SHRINE_FLAG,
            attack: None,
            field: 0,
            tunes: None,
            links: 0,
        },
    ]
}

/// A 64-tile open map in Conquest with no nodes, each player holding 100
/// Grub, and the given meters for the first players (the rest have none).
fn conquest(meters: &[Meter]) -> World {
    let side = 64 * CELLS_PER_TILE as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        seed: 13,
        capacities: Capacities { regiments: 64 },
        units: &units(),
        map_tiles: 64,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
    });
    let mut start = [(0, 0); MAX_PARTIES];
    start[..2].fill((100, 0));
    let mut by_player = [Meter::NotBuilt; MAX_PARTIES];
    by_player[..meters.len()].copy_from_slice(meters);
    assert!(w.conquest(&ConquestSetup {
        buildings: &buildings(),
        units: &conquest_units(),
        structures: 32,
        nodes: &[],
        start,
        powers: &POWERS,
        meters: by_player,
        ..ConquestSetup::default()
    }));
    w
}

/// One tick with one command, from `player`.
fn tick_one(w: &mut World, player: u8, command: Command) -> TickStats {
    w.tick(&[PlayerCommand { player, command }])
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    tick_one(w, player, command).commands_applied == 1
}

fn at(w: &World, x: i32, y: i32) -> Option<StructureHandle> {
    w.structures()?
        .iter()
        .find(|v| v.tile == (x, y))
        .map(|v| v.handle)
}

/// Player `player` puts up a finished building with its north-west tile at
/// `(x, y)`.
fn put_up(w: &mut World, player: u8, building: u16, x: i16, y: i16) -> StructureHandle {
    assert!(order(
        w,
        player,
        Command::SpawnStructure {
            building,
            tile: (x, y)
        }
    ));
    at(w, i32::from(x), i32::from(y)).unwrap()
}

/// Player `player` spawns a regiment of type `unit` at `(x, y)`.
fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing,
        }
    ));
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn position(w: &World, h: RegimentHandle) -> FxVec2 {
    w.regiments.iter().find(|r| r.handle == h).unwrap().position
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

fn move_to(regiment: RegimentHandle, x: i32, y: i32) -> Command {
    Command::MoveTo {
        regiment,
        target: FxVec2::from_ints(x, y),
        facing: None,
        files: 0,
        pace: Fx::ZERO,
    }
}

fn edict(power: u8, target: PowerTarget) -> Command {
    Command::UsePower { power, target }
}

/// Player `player`'s Honor, as of the last tick.
fn honor(w: &World, player: u8) -> u32 {
    w.bank(player).unwrap().honor
}

/// Player `player`'s Grub, as of the last tick.
fn grub(w: &World, player: u8) -> u32 {
    w.bank(player).unwrap().grub
}

/// Player `player` gets a Hall in its own corner of the map.
fn hall(w: &mut World, player: u8) {
    put_up(w, player, HALL, 2 + 12 * i16::from(player), 2);
}

/// `n` finished Shrines of `player`, one command each, in a row along the
/// south of the map.
fn shrines(w: &mut World, player: u8, n: u16) {
    for i in 0..n {
        put_up(
            w,
            player,
            SHRINE,
            6 + 4 * i as i16,
            58 + 4 * i16::from(player),
        );
    }
}

/// Runs until `player` has at least `points` Honor. Honor comes in points
/// of one, so from below it stops on exactly `points`.
fn until_honor(w: &mut World, player: u8, points: u32) {
    for _ in 0..100_000 {
        if honor(w, player) >= points {
            return;
        }
        w.tick(&[]);
    }
    panic!("no {points} Honor in 100000 ticks");
}

/// Runs until regiment `h` is broken, and returns the tick count it is seen
/// broken at. The rout's Honor is earned on that same tick.
fn until_routs(w: &mut World, h: RegimentHandle) -> u32 {
    for _ in 0..6000 {
        match w.regiments.get(h) {
            Some(r) if r.morale.broken() => return w.tick_count(),
            Some(_) => {}
            None => panic!("it died before it routed"),
        }
        w.tick(&[]);
    }
    panic!("it never routed");
}

/// Runs `w` on, with nothing ordered, until its tick count is `tick`.
fn run_to(w: &mut World, tick: u32) {
    while w.tick_count() < tick {
        w.tick(&[]);
    }
}

/// Player 0's regiment of type `unit` stands three tiles from player 1's
/// skitterlings, facing them. Returns (player 0's, the skitterlings).
fn engage(w: &mut World, unit: u16) -> (RegimentHandle, RegimentHandle) {
    let mine = spawn(w, 0, unit, 30, 30, NORTH);
    let theirs = spawn(w, 1, SKITTERS, 30, 27, SOUTH);
    (mine, theirs)
}

/// The meter gate: a player on the Clans' Honor can't use a Resonance power,
/// a Resonance player can't use an Honor one, and a player with no meter
/// can use neither. Each is refused as unknown, whatever they hold.
#[test]
fn a_power_of_another_meter_is_unknown_to_the_player() {
    let mut w = conquest(&[Meter::Honor, Meter::Resonance, Meter::NotBuilt]);
    for p in 0..3 {
        hall(&mut w, p);
    }
    let own_hall = at(&w, 2, 2).unwrap();

    // The Honor player's Edict passes the gate and is refused for the price;
    // its Resonance power is unknown to it.
    assert_eq!(w.can_use_power(0, RICE_TAX), Err(Refusal::Price));
    assert_eq!(w.can_use_power(0, OVERCLOCK), Err(Refusal::Unknown));
    // The Resonance player: the reverse.
    assert_eq!(w.can_use_power(1, OVERCLOCK), Err(Refusal::Price));
    assert_eq!(w.can_use_power(1, RICE_TAX), Err(Refusal::Unknown));
    // No meter: neither, with a Hall and a tier to spare.
    assert_eq!(w.can_use_power(2, RICE_TAX), Err(Refusal::Unknown));
    assert_eq!(w.can_use_power(2, OVERCLOCK), Err(Refusal::Unknown));

    let stats = tick_one(
        &mut w,
        0,
        edict(OVERCLOCK, PowerTarget::Structure(own_hall)),
    );
    assert_eq!(stats.commands_rejected, 1, "Honor player, Resonance power");
    let stats = tick_one(&mut w, 1, edict(RICE_TAX, PowerTarget::Caster));
    assert_eq!(stats.commands_rejected, 1, "Resonance player, Honor power");
    let stats = tick_one(&mut w, 2, edict(RICE_TAX, PowerTarget::Caster));
    assert_eq!(stats.commands_rejected, 1, "player with no meter");
}

/// Two identical worlds, one where player 0 takes Rice Tax and one where
/// they don't, differ by exactly its 30 Grub and its 20 Honor, and still do
/// after more ticks.
#[test]
fn rice_tax_adds_exactly_its_grub_and_spends_its_honor_cost() {
    let make = || {
        let mut w = conquest(&[Meter::Honor]);
        hall(&mut w, 0);
        shrines(&mut w, 0, 10);
        until_honor(&mut w, 0, 20);
        w
    };
    let mut taxed = make();
    let mut plain = make();
    assert_eq!(honor(&taxed, 0), 20);
    assert_eq!(honor(&taxed, 0), honor(&plain, 0));
    assert_eq!(grub(&taxed, 0), grub(&plain, 0));

    let stats = taxed.tick(&[PlayerCommand {
        player: 0,
        command: edict(RICE_TAX, PowerTarget::Caster),
    }]);
    assert_eq!(stats.commands_applied, 1);
    plain.tick(&[]);
    assert_eq!(grub(&taxed, 0), grub(&plain, 0) + 30, "30 Grub more");
    assert_eq!(honor(&plain, 0) - honor(&taxed, 0), 20, "20 Honor spent");

    run(&mut taxed, 100);
    run(&mut plain, 100);
    assert_eq!(grub(&taxed, 0), grub(&plain, 0) + 30, "still 30 more");
    assert_eq!(honor(&plain, 0) - honor(&taxed, 0), 20, "still 20 spent");
}

/// Rice Tax is refused below its 20 Honor: at 0 and at 19 nothing is
/// granted, spent or put on cooldown. At 20 it goes through.
#[test]
fn rice_tax_is_refused_without_its_honor_and_spends_nothing() {
    let mut w = conquest(&[Meter::Honor]);
    hall(&mut w, 0);
    shrines(&mut w, 0, 10);
    assert_eq!(honor(&w, 0), 0);
    let grub_before = grub(&w, 0);

    assert_eq!(w.can_use_power(0, RICE_TAX), Err(Refusal::Price));
    let stats = tick_one(&mut w, 0, edict(RICE_TAX, PowerTarget::Caster));
    assert_eq!(stats.commands_rejected, 1, "refused at 0 Honor");
    assert_eq!(grub(&w, 0), grub_before, "no Grub granted");
    assert_eq!(w.power_cooling(0, RICE_TAX), 0, "no cooldown started");

    until_honor(&mut w, 0, 19);
    assert_eq!(honor(&w, 0), 19);
    assert_eq!(w.can_use_power(0, RICE_TAX), Err(Refusal::Price));
    let stats = tick_one(&mut w, 0, edict(RICE_TAX, PowerTarget::Caster));
    assert_eq!(stats.commands_rejected, 1, "refused at 19 Honor");
    assert_eq!(grub(&w, 0), grub_before, "still no Grub granted");
    assert_eq!(honor(&w, 0), 19, "and nothing spent");

    until_honor(&mut w, 0, 20);
    assert_eq!(honor(&w, 0), 20);
    assert_eq!(w.can_use_power(0, RICE_TAX), Ok(()));
    assert!(order(&mut w, 0, edict(RICE_TAX, PowerTarget::Caster)));
    assert_eq!(grub(&w, 0), grub_before + 30, "the grant lands");
    assert!(w.power_cooling(0, RICE_TAX) > 0, "and it cools down");
}

/// Player 0 with 10 Honor (a March's price) and two spearmen on the same
/// line, the first at y=20 and the second at y=44. Returns (the world, the
/// spearman at y=20, the one at y=44).
fn marchers() -> (World, RegimentHandle, RegimentHandle) {
    let mut w = conquest(&[Meter::Honor]);
    hall(&mut w, 0);
    shrines(&mut w, 0, 10);
    until_honor(&mut w, 0, 10);
    let north = spawn(&mut w, 0, SPEARS, 6, 20, EAST);
    let south = spawn(&mut w, 0, SPEARS, 6, 44, EAST);
    (w, north, south)
}

/// Forced March on the first spearman, and both are sent east to x = 60.
fn march_both(w: &mut World, marched: RegimentHandle, plain: RegimentHandle) -> TickStats {
    w.tick(&[
        PlayerCommand {
            player: 0,
            command: edict(FORCED_MARCH, PowerTarget::Regiment(marched)),
        },
        PlayerCommand {
            player: 0,
            command: move_to(marched, 60, 20),
        },
        PlayerCommand {
            player: 0,
            command: move_to(plain, 60, 44),
        },
    ])
}

/// A marched regiment covers half as much ground again as an identical one
/// over the same ticks (its march is 50 percent faster).
#[test]
fn a_marched_regiment_covers_half_as_much_ground_again() {
    let (mut w, marched, plain) = marchers();
    let (a0, b0) = (position(&w, marched), position(&w, plain));
    let stats = march_both(&mut w, marched, plain);
    assert_eq!(stats.commands_applied, 3);
    run(&mut w, 100);
    let ran_marched = i64::from((position(&w, marched) - a0).length().raw());
    let ran_plain = i64::from((position(&w, plain) - b0).length().raw());
    assert!(ran_plain > 0, "the plain one marched");
    // 3 * plain is 2 * marched: within 2 percent.
    let (three, two) = (3 * ran_plain, 2 * ran_marched);
    assert!(
        (two - three).abs() * 50 <= three,
        "marched {ran_marched}, plain {ran_plain}"
    );
}

/// Forced March counts its duration down tick by tick, ends at 0, and from
/// then the two regiments move at the same speed.
#[test]
fn forced_march_counts_down_and_then_both_go_at_the_same_speed() {
    let (mut w, marched, plain) = marchers();
    let start = w.tick_count();
    assert_eq!(w.regiment_march(marched), 0);
    assert_eq!(march_both(&mut w, marched, plain).commands_applied, 3);
    while w.tick_count() < start + 200 {
        assert_eq!(
            u32::from(w.regiment_march(marched)),
            start + 200 - w.tick_count(),
            "ticks left at tick {}",
            w.tick_count()
        );
        assert_eq!(w.regiment_march(plain), 0, "never marched");
        w.tick(&[]);
    }
    assert_eq!(w.regiment_march(marched), 0, "the march is over");

    for _ in 0..20 {
        let (a, b) = (position(&w, marched), position(&w, plain));
        w.tick(&[]);
        let step_a = (position(&w, marched) - a).length().raw();
        let step_b = (position(&w, plain) - b).length().raw();
        assert!(step_b > 0, "still marching");
        assert!(
            (step_a - step_b).abs() <= 8,
            "after the march: {step_a} and {step_b} raw per tick"
        );
    }
}

/// A second Forced March in the same window as the first rides on its
/// payment: one Honor price, not two. A third, after the window, is refused.
#[test]
fn a_second_march_in_the_same_window_rides_on_the_first_payment() {
    let (mut w, first, second) = marchers();
    let (mut idle, _, _) = marchers();
    assert_eq!(honor(&w, 0), honor(&idle, 0));

    let stats = march_both(&mut w, first, second);
    assert_eq!(stats.commands_applied, 3);
    // Next tick, the second regiment, within the window.
    let stats = w.tick(&[
        PlayerCommand {
            player: 0,
            command: edict(FORCED_MARCH, PowerTarget::Regiment(second)),
        },
        PlayerCommand {
            player: 0,
            command: move_to(second, 60, 44),
        },
    ]);
    assert_eq!(stats.commands_applied, 2, "it rides on the first");
    assert!(w.regiment_march(second) > 0, "and it marches");

    run(&mut idle, 2);
    assert_eq!(
        honor(&idle, 0) - honor(&w, 0),
        10,
        "one March's price for both"
    );

    // Past the window, the next March is refused: the power is cooling.
    run(&mut w, gc_sim::powers::RIDE_TICKS);
    let late = tick_one(
        &mut w,
        0,
        edict(FORCED_MARCH, PowerTarget::Regiment(second)),
    );
    assert_eq!(late.commands_rejected, 1, "too late to ride, and cooling");
}

/// A Banner fight: player 0's spearmen face player 1's skitterlings three
/// tiles off, and `caster` has 10 Shrines and 30 Honor for a Banner. Returns
/// (the world, the spearmen, the skitterlings).
fn banner_fight(caster: u8) -> (World, RegimentHandle, RegimentHandle) {
    let meters = if caster == 0 {
        [Meter::Honor, Meter::Resonance]
    } else {
        [Meter::Resonance, Meter::Honor]
    };
    let mut w = conquest(&meters);
    hall(&mut w, 0);
    hall(&mut w, 1);
    shrines(&mut w, caster, 10);
    until_honor(&mut w, caster, 30);
    let (spears, skitters) = engage(&mut w, SPEARS);
    (w, spears, skitters)
}

/// Under a Banner, a regiment of the caster's inside its radius doesn't rout
/// under a beating that routs the same regiment outside it: it stands
/// wavering and fights on until it is destroyed, never running.
#[test]
fn a_banner_keeps_its_own_regiment_inside_its_radius_from_routing() {
    let (mut control, _, skitters_c) = banner_fight(1);
    let routed_at = until_routs(&mut control, skitters_c);

    let (mut w, _, skitters) = banner_fight(1);
    let on_them = position(&w, skitters);
    assert!(order(&mut w, 1, edict(BANNER, PowerTarget::Point(on_them))));
    assert!(w.regiment_steadfast(skitters) > 0);
    let mut alive_at_rout = false;
    while w.tick_count() < routed_at + 20 * 60 {
        w.tick(&[]);
        match w.regiments.get(skitters) {
            Some(r) => {
                assert!(
                    !r.morale.broken(),
                    "routed under the Banner at tick {}",
                    w.tick_count()
                );
                alive_at_rout |= w.tick_count() == routed_at;
            }
            None => break,
        }
    }
    assert!(alive_at_rout, "still fighting when the outside one broke");
}

/// A Banner's radius is 8 tiles: one planted 13 tiles from the caster's
/// only regiment steadies none, so it is refused and costs nothing, and
/// the regiment routs as it would have anyway.
#[test]
fn a_banner_out_of_reach_of_the_casters_regiments_is_refused() {
    let (mut control, _, skitters_c) = banner_fight(1);
    let routed_at = until_routs(&mut control, skitters_c);

    let (mut w, _, skitters) = banner_fight(1);
    let far = FxVec2::from_ints(30, 40);
    let before = honor(&w, 1);
    assert!(!order(&mut w, 1, edict(BANNER, PowerTarget::Point(far))));
    assert_eq!(w.regiment_steadfast(skitters), 0, "out of its reach");
    assert!(honor(&w, 1) >= before, "nothing spent");
    assert_eq!(w.can_use_power(1, BANNER), Ok(()), "and not cooling");
    assert_eq!(until_routs(&mut w, skitters), routed_at);
}

/// A marched regiment that routs runs at a router's pace: its march ends.
#[test]
fn a_regiment_that_routs_loses_its_march() {
    let (mut w, _, skitters) = banner_fight(1);
    assert!(order(
        &mut w,
        1,
        edict(FORCED_MARCH, PowerTarget::Regiment(skitters))
    ));
    assert!(w.regiment_march(skitters) > 0);
    until_routs(&mut w, skitters);
    assert_eq!(w.regiment_march(skitters), 0);
}

/// Edicts are cast from a Shrine: with the Honor for one but no finished
/// Shrine left standing, every Edict is refused.
#[test]
fn edicts_need_a_finished_shrine() {
    let mut w = conquest(&[Meter::Honor]);
    hall(&mut w, 0);
    shrines(&mut w, 0, 10);
    until_honor(&mut w, 0, 30);
    assert_eq!(w.can_use_power(0, RICE_TAX), Ok(()));
    let standing: Vec<StructureHandle> = (w.structures().unwrap().iter())
        .filter(|s| s.building == SHRINE)
        .map(|s| s.handle)
        .collect();
    for structure in standing {
        assert!(order(&mut w, 0, Command::RemoveStructure { structure }));
    }
    for power in [RICE_TAX, FORCED_MARCH, BANNER] {
        assert_eq!(w.can_use_power(0, power), Err(Refusal::NoShrine));
    }
    assert!(!order(&mut w, 0, edict(RICE_TAX, PowerTarget::Caster)));
}

/// A Banner steadies only the caster's own regiments: an enemy regiment
/// inside its radius, beaten the same way, still routs when it would have.
#[test]
fn a_banner_does_not_steady_an_enemy_regiment_inside_its_radius() {
    let (mut control, _, skitters_c) = banner_fight(0);
    let routed_at = until_routs(&mut control, skitters_c);

    let (mut w, spears, skitters) = banner_fight(0);
    let on_us = position(&w, spears);
    assert!(order(&mut w, 0, edict(BANNER, PowerTarget::Point(on_us))));
    assert!(w.regiment_steadfast(spears) > 0, "our own are steadied");
    assert_eq!(w.regiment_steadfast(skitters), 0, "theirs are not");
    assert_eq!(until_routs(&mut w, skitters), routed_at);
}

/// A finished Shrine earns a point every 30 seconds (600 ticks). Its clock
/// starts on the tick it stands, which is lined up on a whole second here.
#[test]
fn a_finished_shrine_earns_a_point_every_thirty_seconds() {
    let mut w = conquest(&[Meter::Honor]);
    hall(&mut w, 0);
    // Line the Shrine up on a whole second, so its clock is easy to follow.
    while !w.tick_count().is_multiple_of(TICKS_PER_SECOND) {
        w.tick(&[]);
    }
    put_up(&mut w, 0, SHRINE, 6, 58);
    // The shrine has stood for 580 ticks: not yet 30 seconds.
    run(&mut w, 579);
    assert_eq!(honor(&w, 0), 0, "not a point before 30 seconds");
    run(&mut w, 20);
    assert_eq!(honor(&w, 0), 1, "a point at 30 seconds");
    for k in 2..=20 {
        run(&mut w, 600);
        assert_eq!(honor(&w, 0), k, "a point per 30 seconds, {k} by now");
    }
}

/// Two finished Shrines earn two points every 30 seconds: the rate adds up
/// with the Shrines standing.
#[test]
fn two_shrines_earn_twice_as_fast() {
    let mut w = conquest(&[Meter::Honor]);
    hall(&mut w, 0);
    while !w.tick_count().is_multiple_of(TICKS_PER_SECOND) {
        w.tick(&[]);
    }
    let both = w.tick(&[
        PlayerCommand {
            player: 0,
            command: Command::SpawnStructure {
                building: SHRINE,
                tile: (6, 58),
            },
        },
        PlayerCommand {
            player: 0,
            command: Command::SpawnStructure {
                building: SHRINE,
                tile: (10, 58),
            },
        },
    ]);
    assert_eq!(both.commands_applied, 2);
    // Two Shrines' clocks add twice as fast: one point is due by 580 ticks,
    // two by 600.
    run(&mut w, 579);
    assert_eq!(honor(&w, 0), 1, "a point after 29 seconds");
    run(&mut w, 20);
    assert_eq!(honor(&w, 0), 2, "two points at 30 seconds");
    run(&mut w, 600 * 4);
    assert_eq!(honor(&w, 0), 10, "two more every 30 seconds for 2 minutes");
}

/// An unfinished Shrine, one a worker has begun and not finished, earns no
/// Honor however long it stands.
#[test]
fn an_unfinished_shrine_earns_no_honor() {
    let mut w = conquest(&[Meter::Honor]);
    hall(&mut w, 0);
    let worker = spawn(&mut w, 0, WORKER, 20, 30, EAST);
    assert!(order(
        &mut w,
        0,
        Command::Build {
            regiment: worker,
            building: SHRINE,
            tile: (24, 30),
        }
    ));
    let foundation = at(&w, 24, 30).unwrap();
    assert!(
        w.structures()
            .unwrap()
            .get(foundation)
            .unwrap()
            .built_percent
            < 100
    );
    run(&mut w, 12_000);
    assert_eq!(honor(&w, 0), 0, "no Honor from a foundation");
    assert!(
        w.structures()
            .unwrap()
            .get(foundation)
            .unwrap()
            .built_percent
            < 100
    );
}

/// Shrines earn nothing for a Resonance player, while the same Shrines
/// earn an Honor player theirs.
#[test]
fn a_resonance_player_earns_nothing_from_shrines() {
    let mut w = conquest(&[Meter::Resonance, Meter::Honor]);
    hall(&mut w, 0);
    hall(&mut w, 1);
    shrines(&mut w, 0, 10);
    shrines(&mut w, 1, 10);
    run(&mut w, 6000);
    assert_eq!(
        honor(&w, 0),
        0,
        "the Resonance player's Shrines earn nothing"
    );
    assert!(honor(&w, 1) > 0, "the Honor player's Shrines do");
}

/// Spearmen of player 0 (Honor) rout skitterlings of player 1 (Resonance):
/// the spearmen earn the rout's Honor.
#[test]
fn a_rout_earns_the_honor_player_rout_honor() {
    let mut w = conquest(&[Meter::Honor, Meter::Resonance]);
    hall(&mut w, 0);
    hall(&mut w, 1);
    let (_, skitters) = engage(&mut w, SPEARS);
    until_routs(&mut w, skitters);
    assert_eq!(honor(&w, 0), ROUT_HONOR, "the rout's Honor");
    assert_eq!(honor(&w, 1), 0, "the router's side earns nothing");
}

/// A squad of eight spearmen closes on the skitterlings and routs them while
/// more than eight still stand: the Honor player earns the outnumbered rate.
#[test]
fn a_rout_by_fewer_soldiers_earns_the_outnumbered_honor() {
    let mut w = conquest(&[Meter::Honor, Meter::Resonance]);
    hall(&mut w, 0);
    hall(&mut w, 1);
    let (squad, skitters) = engage(&mut w, SQUAD);
    // One rank of eight does not reach them from where they stand: close in.
    assert!(order(
        &mut w,
        0,
        Command::Attack {
            regiment: squad,
            target: skitters,
        }
    ));
    until_routs(&mut w, skitters);
    assert!(
        w.regiments.get(skitters).unwrap().models > 8,
        "the skitterlings still outnumber the squad"
    );
    assert_eq!(honor(&w, 0), OUTNUMBERED_HONOR, "the outnumbered rate");
}

/// A Clans regiment that routs costs its owner `FLED_HONOR`: an Honor player
/// with 10 Honor whose skitterlings rout ends with that much less than the
/// same player in the same world with no fight.
#[test]
fn a_routing_regiment_costs_its_honor_player_fled_honor() {
    let make = |fight: bool| {
        let mut w = conquest(&[Meter::Resonance, Meter::Honor]);
        hall(&mut w, 0);
        hall(&mut w, 1);
        shrines(&mut w, 1, 10);
        until_honor(&mut w, 1, 10);
        let skitters = if fight {
            engage(&mut w, SPEARS).1
        } else {
            // Out of reach: the same skitterlings, nobody to fight them.
            spawn(&mut w, 0, SPEARS, 50, 30, NORTH);
            spawn(&mut w, 1, SKITTERS, 30, 27, SOUTH)
        };
        (w, skitters)
    };
    let (mut fight, skitters) = make(true);
    let routed_at = until_routs(&mut fight, skitters);
    let (mut idle, _) = make(false);
    run_to(&mut idle, routed_at);
    assert_eq!(
        honor(&idle, 1) - honor(&fight, 1),
        FLED_HONOR,
        "fled honor lost at the rout"
    );
}

/// A Resonance player's regiment that routs an Honor player's, with Shrines
/// standing for the Resonance player, earns the Resonance player no Honor.
#[test]
fn a_resonance_player_earns_no_honor_from_a_rout() {
    let mut w = conquest(&[Meter::Resonance, Meter::Honor]);
    hall(&mut w, 0);
    hall(&mut w, 1);
    shrines(&mut w, 0, 10);
    let (_, skitters) = engage(&mut w, SPEARS);
    until_routs(&mut w, skitters);
    assert_eq!(honor(&w, 0), 0, "no Honor for the winner");
}
