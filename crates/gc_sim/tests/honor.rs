//! Honor: the Ember Clans' meter (docs/03-factions.md). Honor is spent on
//! Edicts (Rice Tax, Forced March, Banner of the Ancestors, Ancestral
//! Return), earned by Shrines and by routs, and lost when one of the Clans'
//! own regiments routs.

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

/// The powers table: four Edicts paid from Honor, and one paid from
/// Resonance.
const RICE_TAX: u8 = 0;
const FORCED_MARCH: u8 = 1;
const BANNER: u8 = 2;
const OVERCLOCK: u8 = 3;
/// Calls up a regiment of skitterlings (who rout) for 2400 ticks.
const RETURN: u8 = 4;

const POWERS: [PowerStats; 5] = [
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
    PowerStats {
        tier: 1,
        cost: 40,
        meter: Meter::Honor,
        cooldown_ticks: 1200,
        duration_ticks: 2400,
        effect: PowerEffect::Summon { unit: SKITTERS },
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
    conquest_with(meters, &POWERS)
}

/// [`conquest`] with another powers table.
fn conquest_with(meters: &[Meter], powers: &[PowerStats]) -> World {
    conquest_on(meters, powers, &walled(|_, _| true))
}

/// The 64-tile map's nav costs with open ground (cost 1) where `open` says
/// so for a tile, and blocked ground elsewhere.
fn walled(open: impl Fn(i32, i32) -> bool) -> Vec<u8> {
    let side = 64 * CELLS_PER_TILE;
    (0..side * side)
        .map(|i| {
            let (x, y) = (i % side / CELLS_PER_TILE, i / side / CELLS_PER_TILE);
            if open(x, y) { 1 } else { gc_sim::nav::BLOCKED }
        })
        .collect()
}

/// [`conquest_with`] on the nav costs `nav`.
fn conquest_on(meters: &[Meter], powers: &[PowerStats], nav: &[u8]) -> World {
    let mut w = World::new(&MatchSetup {
        seed: 13,
        capacities: Capacities { regiments: 64 },
        units: &units(),
        map_tiles: 64,
        nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        // Generals off: their drill units never rout, so any of them could
        // take a general and change what these rules measure.
        generals: false,
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
        powers,
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
            tile: (x, y),
            turn: 0,
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
                turn: 0,
            },
        },
        PlayerCommand {
            player: 0,
            command: Command::SpawnStructure {
                building: SHRINE,
                tile: (10, 58),
                turn: 0,
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
            turn: 0,
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

/// An Ancestral Return world: `caster` has the Honor meter and 40 Honor
/// with `powers` for its table, and a finished Shrine at tile (30, 30)
/// (centre (31, 31)); the other player has Resonance and a Hall. Returns
/// the world and that Shrine.
fn return_world(caster: u8, powers: &[PowerStats]) -> (World, StructureHandle) {
    return_world_on(caster, powers, &walled(|_, _| true))
}

/// [`return_world`] on the nav costs `nav`.
fn return_world_on(caster: u8, powers: &[PowerStats], nav: &[u8]) -> (World, StructureHandle) {
    let meters = if caster == 0 {
        [Meter::Honor, Meter::Resonance]
    } else {
        [Meter::Resonance, Meter::Honor]
    };
    let mut w = conquest_on(&meters, powers, nav);
    hall(&mut w, 0);
    hall(&mut w, 1);
    shrines(&mut w, caster, 10);
    let altar = put_up(&mut w, caster, SHRINE, 30, 30);
    until_honor(&mut w, caster, 40);
    (w, altar)
}

/// The regiments of `player`, as handles.
fn regiments_of(w: &World, player: u8) -> Vec<RegimentHandle> {
    (w.regiments.iter())
        .filter(|r| r.owner == player)
        .map(|r| r.handle)
        .collect()
}

/// The regiment `player` has now that they didn't have in `before`.
fn raised(w: &World, player: u8, before: &[RegimentHandle]) -> RegimentHandle {
    let now = regiments_of(w, player);
    assert_eq!(now.len(), before.len() + 1, "exactly one regiment raised");
    *now.iter().find(|h| !before.contains(h)).unwrap()
}

/// Ancestral Return, aimed at one of the caster's finished Shrines, puts a
/// full regiment of its unit down beside it, facing away, for its Honor
/// price, with a cooldown, and takes no supply.
#[test]
fn a_summons_raises_a_full_regiment_beside_the_shrine_for_its_honor() {
    let (mut w, altar) = return_world(0, &POWERS);
    let supply = w.bank(0).unwrap().supply_used;
    let before = regiments_of(&w, 0);
    assert_eq!((honor(&w, 0), w.can_use_power(0, RETURN)), (40, Ok(())));
    let mut plain = return_world(0, &POWERS).0;

    assert!(order(
        &mut w,
        0,
        edict(RETURN, PowerTarget::Structure(altar))
    ));
    plain.tick(&[]);
    let h = raised(&w, 0, &before);
    let r = w.regiments.get(h).unwrap();
    assert_eq!(r.unit, SKITTERS);
    assert_eq!(r.models, units()[usize::from(SKITTERS)].models, "full");
    assert_eq!(r.morale, gc_sim::MoraleState::Steady);
    // The first spot round the 2x2 Shrine at (30, 30): four tiles south of
    // its centre, facing south, away from it.
    assert_eq!(r.position, FxVec2::from_ints(31, 35));
    assert_eq!(r.facing, SOUTH);
    assert_eq!(w.regiment_summoned(h), 2400, "its time starts at the cast");
    assert_eq!(honor(&plain, 0) - honor(&w, 0), 40, "40 Honor spent");
    assert!(w.power_cooling(0, RETURN) > 0, "and it cools down");
    assert_eq!(w.bank(0).unwrap().supply_used, supply, "no supply taken");
    assert_eq!(regiments_of(&plain, 0).len(), before.len());
    assert_eq!(plain.regiment_summoned(h), 0);

    // A second one at once is refused for the cooldown.
    assert!(!order(
        &mut w,
        0,
        edict(RETURN, PowerTarget::Structure(altar))
    ));
}

/// A second summons while the first still stands goes beside it, on the
/// next open spot round the Shrine, not on top of it.
#[test]
fn a_second_summons_takes_the_next_open_spot() {
    let (mut w, altar) = return_world(0, &POWERS);
    let first = {
        let before = regiments_of(&w, 0);
        assert!(order(
            &mut w,
            0,
            edict(RETURN, PowerTarget::Structure(altar))
        ));
        raised(&w, 0, &before)
    };
    // Past the cooldown, with the Honor to pay again.
    run(&mut w, 1200);
    until_honor(&mut w, 0, 40);
    let before = regiments_of(&w, 0);
    assert!(order(
        &mut w,
        0,
        edict(RETURN, PowerTarget::Structure(altar))
    ));
    let second = raised(&w, 0, &before);
    let (a, b) = (position(&w, first), position(&w, second));
    assert_ne!(a, b);
    assert!((a - b).length() >= Fx::from_int(4), "not on top of it");
    assert_eq!(a, FxVec2::from_ints(31, 35));
}

/// Ancestral Return is for the caster's own finished Shrines: anything
/// else is refused and costs nothing.
#[test]
fn a_summons_is_refused_for_anything_but_an_own_finished_shrine() {
    let mut w = conquest_with(&[Meter::Honor, Meter::Honor], &POWERS);
    hall(&mut w, 0);
    hall(&mut w, 1);
    shrines(&mut w, 0, 10);
    let theirs = put_up(&mut w, 1, SHRINE, 40, 20);
    // A foundation of theirs... and of ours, laid by a worker.
    let worker = spawn(&mut w, 0, WORKER, 20, 30, EAST);
    assert!(order(
        &mut w,
        0,
        Command::Build {
            regiment: worker,
            building: SHRINE,
            tile: (24, 30),
            turn: 0,
        }
    ));
    let foundation = at(&w, 24, 30).unwrap();
    assert!(!w.structures().unwrap().get(foundation).unwrap().finished());
    let gone = put_up(&mut w, 0, SHRINE, 50, 30);
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: gone }
    ));
    let own = put_up(&mut w, 0, SHRINE, 30, 30);
    until_honor(&mut w, 0, 40);

    let hall0 = at(&w, 2, 2).unwrap();
    let regiments = regiments_of(&w, 0).len();
    let refused = [
        ("an enemy Shrine", PowerTarget::Structure(theirs)),
        ("an unfinished Shrine", PowerTarget::Structure(foundation)),
        ("a Hall", PowerTarget::Structure(hall0)),
        ("a Shrine that is gone", PowerTarget::Structure(gone)),
        ("a regiment", PowerTarget::Regiment(worker)),
        ("the caster", PowerTarget::Caster),
        ("a point", PowerTarget::Point(FxVec2::from_ints(31, 31))),
    ];
    for (what, target) in refused {
        assert!(!order(&mut w, 0, edict(RETURN, target)), "{what}");
        assert_eq!(honor(&w, 0), 40, "{what}: nothing spent");
        assert_eq!(w.power_cooling(0, RETURN), 0, "{what}: no cooldown");
        assert_eq!(regiments_of(&w, 0).len(), regiments, "{what}: none raised");
    }
    assert!(order(&mut w, 0, edict(RETURN, PowerTarget::Structure(own))));
    assert_eq!(regiments_of(&w, 0).len(), regiments + 1);
}

/// Without the Honor, the tier or a Shrine the player has, a summons is
/// refused like any Edict.
#[test]
fn a_summons_needs_its_honor() {
    let (mut w, altar) = return_world(0, &POWERS);
    // Spend the Honor on a Rice Tax's worth first: 40 - 20 is under 40.
    assert!(order(&mut w, 0, edict(RICE_TAX, PowerTarget::Caster)));
    assert!(honor(&w, 0) < 40);
    assert_eq!(w.can_use_power(0, RETURN), Err(Refusal::Price));
    assert!(!order(
        &mut w,
        0,
        edict(RETURN, PowerTarget::Structure(altar))
    ));
}

/// A fight where `caster` has summoned skitterlings that player
/// `1 - caster`'s fearless spearmen attack, three tiles ahead of them.
/// Returns the world, the summoned regiment and the spearmen.
fn summoned_fight(caster: u8) -> (World, RegimentHandle, RegimentHandle) {
    let (mut w, altar) = return_world(caster, &POWERS);
    let before = regiments_of(&w, caster);
    assert!(order(
        &mut w,
        caster,
        edict(RETURN, PowerTarget::Structure(altar))
    ));
    let ghosts = raised(&w, caster, &before);
    // They stand at (31, 35) facing south: the spears meet them there.
    let spears = spawn(&mut w, 1 - caster, SPEARS, 31, 38, NORTH);
    (w, ghosts, spears)
}

/// A summoned regiment never routs: the same skitterlings, raised the
/// ordinary way and beaten the same way, run; the summoned ones stand and
/// fight on (wavering) until they are destroyed or their time is up.
#[test]
fn a_summoned_regiment_never_routs() {
    let mut control = conquest(&[Meter::Resonance, Meter::Honor]);
    hall(&mut control, 0);
    hall(&mut control, 1);
    let ordinary = spawn(&mut control, 1, SKITTERS, 31, 35, SOUTH);
    spawn(&mut control, 0, SPEARS, 31, 38, NORTH);
    let met = control.tick_count();
    let breaks_after = until_routs(&mut control, ordinary) - met;
    assert!(breaks_after < 2400, "they would rout well within its time");

    let (mut w, ghosts, _) = summoned_fight(1);
    let cast = w.tick_count();
    let (mut fought, mut still_there) = (false, false);
    while w.tick_count() < cast + 2400 {
        w.tick(&[]);
        let Some(r) = w.regiments.get(ghosts) else {
            break;
        };
        assert!(!r.morale.broken(), "routed at tick {}", w.tick_count());
        fought |= r.engaged;
        still_there |= w.tick_count() > cast + breaks_after + 40;
    }
    assert!(fought, "it fought");
    assert!(still_there, "and was still fighting when the others broke");
}

/// A world where player 0 has summoned a regiment of spearmen for 200
/// ticks beside a Shrine, with player 1's spearmen locked in with it.
/// Returns it, the summoned regiment, the foes and player 0's regiments
/// from before it was called.
fn short_lived() -> (World, RegimentHandle, RegimentHandle, Vec<RegimentHandle>) {
    let mut powers = POWERS;
    powers[usize::from(RETURN)] = PowerStats {
        duration_ticks: 200,
        effect: PowerEffect::Summon { unit: SPEARS },
        ..POWERS[usize::from(RETURN)]
    };
    let (mut w, altar) = return_world(0, &powers);
    let before = regiments_of(&w, 0);
    assert!(order(
        &mut w,
        0,
        edict(RETURN, PowerTarget::Structure(altar))
    ));
    let ghosts = raised(&w, 0, &before);
    let spears = spawn(&mut w, 1, SPEARS, 31, 38, NORTH);
    (w, ghosts, spears, before)
}

/// Its time up, a summoned regiment is gone: the regiment fighting it is
/// let go (nothing left locked with it), and the handle is stale. One
/// tick earlier it is still there. Two worlds do the same, bit for bit.
#[test]
fn a_summoned_regiment_vanishes_when_its_time_is_up_and_its_foes_let_go() {
    let (mut w, ghosts, spears, before) = short_lived();
    // The spawn of the foe took a tick: 199 ticks of the 200 are left.
    let mut engaged = false;
    while w.regiment_summoned(ghosts) > 1 {
        w.tick(&[]);
        engaged |= w.regiments.get(spears).is_some_and(|r| r.engaged);
        assert!(w.regiments.get(ghosts).is_some(), "gone early");
    }
    assert!(engaged, "locked with the summoned regiment");
    assert!(w.regiments.get(ghosts).unwrap().engaged);
    w.tick(&[]);
    assert!(w.regiments.get(ghosts).is_none(), "gone when it is up");
    assert_eq!(w.regiment_summoned(ghosts), 0);
    assert!(!w.regiments.get(spears).unwrap().engaged, "the foe let go");
    assert_eq!(regiments_of(&w, 0), before);

    let (mut again, ..) = short_lived();
    while again.tick_count() < w.tick_count() {
        again.tick(&[]);
    }
    assert_eq!(w.state_hash(), again.state_hash());
}

/// The summoned regiment's time is part of the state hash.
#[test]
fn a_summoned_regiments_time_is_in_the_state_hash() {
    let (mut a, ..) = short_lived();
    let (mut b, ..) = short_lived();
    assert_eq!(a.state_hash(), b.state_hash());
    run(&mut a, 1);
    run(&mut b, 1);
    assert_eq!(a.state_hash(), b.state_hash());
    // One that is cast a tick later has a different time left, so a
    // different hash at the same tick.
    let (mut c, altar) = {
        let mut powers = POWERS;
        powers[usize::from(RETURN)].duration_ticks = 201;
        powers[usize::from(RETURN)].effect = PowerEffect::Summon { unit: SPEARS };
        return_world(0, &powers)
    };
    assert!(order(
        &mut c,
        0,
        edict(RETURN, PowerTarget::Structure(altar))
    ));
    spawn(&mut c, 1, SPEARS, 31, 38, NORTH);
    run(&mut c, 1);
    assert_ne!(a.state_hash(), c.state_hash());
}

/// Open ground that is blocked from tile row 37 south (but for the row
/// the Honor Shrines stand on): the first spot round the Shrine, four tiles
/// south of it, is open for a centre but too near the edge for the
/// regiment's block.
fn shore() -> Vec<u8> {
    walled(|_, y| !(37..56).contains(&y))
}

/// A summoned regiment is settled clear of blocked ground, as a trained or
/// rallied one is, not put down on a spot whose centre alone is open.
#[test]
fn a_summoned_regiment_stands_clear_of_blocked_ground() {
    let (mut w, altar) = return_world_on(0, &POWERS, &shore());
    let before = regiments_of(&w, 0);
    assert!(w.can_summon_at(0, RETURN, altar));
    assert!(order(
        &mut w,
        0,
        edict(RETURN, PowerTarget::Structure(altar))
    ));
    let h = raised(&w, 0, &before);
    let r = w.regiments.get(h).unwrap();
    let stats = &units()[usize::from(SKITTERS)];
    let (width, depth) = stats.block_size_in(r.formation, r.files, r.models);
    let half = width.max(depth) / 2;
    assert_ne!(
        r.position,
        FxVec2::from_ints(31, 35),
        "not on the open edge"
    );
    assert!(
        Fx::from_int(37) - r.position.y >= half,
        "its whole block is on open ground: at {:?}, half a side {half:?}",
        r.position
    );
}

/// With open ground only in a strip four tiles wide, where a centre may
/// stand but no block fits, a summons has no room: the room check says so, the order is refused, and nothing is
/// spent.
#[test]
fn a_summons_with_no_room_for_the_block_is_refused_and_costs_nothing() {
    let strips = walled(|x, y| {
        (28..34).contains(&x) && (28..34).contains(&y)
            || (29..33).contains(&x) && y < 56
            || !(8..56).contains(&y)
    });
    let (mut w, altar) = return_world_on(0, &POWERS, &strips);
    let before = regiments_of(&w, 0);
    let honor_before = honor(&w, 0);
    assert!(!w.can_summon_at(0, RETURN, altar));
    assert!(!order(
        &mut w,
        0,
        edict(RETURN, PowerTarget::Structure(altar))
    ));
    assert_eq!(regiments_of(&w, 0), before);
    assert_eq!(honor(&w, 0), honor_before);
    assert_eq!(w.can_use_power(0, RETURN), Ok(()), "and no cooldown starts");
}

/// A full regiment pool is no room either.
#[test]
fn a_summons_with_a_full_regiment_pool_is_refused() {
    let (mut w, altar) = return_world(0, &POWERS);
    assert!(w.can_summon_at(0, RETURN, altar));
    for i in 0..200 {
        let command = Command::SpawnRegiment {
            unit: SQUAD,
            position: FxVec2::from_ints(2 + i % 20 * 2, 40 + i / 20 * 2),
            facing: EAST,
        };
        if !order(&mut w, 1, command) {
            break;
        }
    }
    let honor_before = honor(&w, 0);
    assert!(!w.can_summon_at(0, RETURN, altar));
    assert!(!order(
        &mut w,
        0,
        edict(RETURN, PowerTarget::Structure(altar))
    ));
    assert_eq!(honor(&w, 0), honor_before);
}

/// The room check is for summons, and for the caster's own Shrines.
#[test]
fn the_room_check_is_only_for_a_summons_on_an_own_shrine() {
    let (w, altar) = return_world(0, &POWERS);
    assert!(w.can_summon_at(0, RETURN, altar));
    assert!(!w.can_summon_at(1, RETURN, altar), "not their Shrine");
    assert!(!w.can_summon_at(0, RICE_TAX, altar), "not a summons");
    assert!(!w.can_summon_at(0, 99, altar), "no such power");
}
