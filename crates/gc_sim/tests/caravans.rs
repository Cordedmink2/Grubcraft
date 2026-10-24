//! Concord caravans (docs/03-factions.md, "Economy: trade routes and
//! tithes"): a unit that trades, sent to a trade post, shuttles cargo
//! between its owner's main building and the post and banks each delivery
//! as Grub. A hamlet pays in full, the owner's own Waystation at half, an
//! ally's main building in full with a fifth more for the ally; any other
//! order stops the run; and a caravan destroyed with cargo drops half of
//! it as a Grub pile anyone can gather.

use gc_fixed::{Fx, FxVec2};
use gc_sim::economy::{ALLY_PERCENT, Resource, WAYSTATION_PERCENT};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, DROP_OFF_GRUB, DROP_OFF_MARROW, Role, TRADE_POST};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MAX_PARTIES, MatchSetup,
    NodeSetup, PlayerCommand, RegimentHandle, StructureHandle, TICKS_PER_SECOND, TradePost,
    UnitStats, Victories, World,
};

/// A caravan: one soldier that never routs, 3 tiles a second.
const CARAVAN: u16 = 0;
/// A worker, who gathers and banks Grub where there is a place for it.
const WORKER: u16 = 1;
/// Drill unit 0's spearmen, to kill a caravan.
const SPEARS: u16 = 2;
/// A worker type that takes 70 supply, to put its owner into upkeep.
const HEAVY: u16 = 3;
/// A worker of a faction with no Grub drop-off, like the Concord's Clerk.
const CLERK: u16 = 4;

/// A main building that takes Marrow only, like the Concord's Counting
/// House; a Waystation (a trade post and a Marrow drop-off); a granary
/// that takes Grub.
const HALL: u16 = 0;
const WAYSTATION: u16 = 1;
const GRANARY: u16 = 2;

const TILES: u32 = 64;
const SECOND: u32 = TICKS_PER_SECOND;

/// Players 0 and 1 are allies; player 2 is against them.
const TEAMS: [u8; 3] = [0, 0, 1];

fn units() -> [UnitStats; 5] {
    let worker = UnitStats {
        models: 1,
        files: 1,
        turn_rate: 180 * 65_536 / 360,
        ..DRILL_UNITS[0]
    };
    let caravan = UnitStats {
        speed: Fx::from_int(3),
        hp_per_model: 60,
        melee_attack: 0,
        melee_defence: 10,
        melee_damage: 1,
        morale: 0,
        ..worker
    };
    [caravan, worker, DRILL_UNITS[0], worker, worker]
}

fn conquest_units() -> [ConquestUnit; 5] {
    let worker = ConquestUnit {
        worker: true,
        supply: 1,
        ..ConquestUnit::default()
    };
    [
        ConquestUnit {
            supply: 2,
            trade_base: 40,
            trade_tenths: 12,
            ..ConquestUnit::default()
        },
        worker,
        ConquestUnit::default(),
        ConquestUnit {
            supply: 70,
            ..worker
        },
        ConquestUnit {
            cannot_bank: DROP_OFF_GRUB,
            ..worker
        },
    ]
}

fn buildings() -> [BuildingStats; 3] {
    let hall = BuildingStats {
        role: Role::Main,
        tier: 1,
        upgrades_from: None,
        cost_grub: 400,
        cost_marrow: 0,
        build_ticks: 500,
        hp: 3000,
        armour: Armour::Fortified,
        width: 5,
        depth: 5,
        supply: 15,
        flags: DROP_OFF_MARROW,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
    };
    [
        hall,
        BuildingStats {
            role: Role::Outpost,
            cost_grub: 200,
            width: 4,
            depth: 4,
            supply: 0,
            flags: DROP_OFF_MARROW | TRADE_POST,
            ..hall
        },
        BuildingStats {
            role: Role::Housing,
            cost_grub: 50,
            width: 2,
            depth: 2,
            supply: 5,
            flags: DROP_OFF_GRUB,
            ..hall
        },
    ]
}

/// A post `tiles` tiles due east of the middle of a hall put up at
/// `(4, 4)`: its middle is at (6.5, 6.5).
fn post_east(tiles: i32) -> FxVec2 {
    FxVec2::new(
        Fx::from_ratio(13, 2) + Fx::from_int(tiles),
        Fx::from_ratio(13, 2),
    )
}

/// An open 64-tile map in Conquest with no Grub in the stores and these
/// nodes and neutral trade posts: players 0 and 1 are allies, 2 their foe.
fn conquest(nodes: &[NodeSetup], posts: &[FxVec2]) -> World {
    conquest_for(nodes, posts, Victories::default())
}

/// [`conquest`], played for `victories`.
fn conquest_for(nodes: &[NodeSetup], posts: &[FxVec2], victories: Victories) -> World {
    let side = TILES as usize * CELLS_PER_TILE as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities { regiments: 64 },
        units: &units(),
        map_tiles: TILES,
        nav: &nav,
        teams: &TEAMS,
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        // Generals off: their drill units never rout, so any of them could
        // take a general and change what these rules measure.
        generals: false,
    });
    assert!(w.conquest(&ConquestSetup {
        buildings: &buildings(),
        units: &conquest_units(),
        structures: 32,
        nodes,
        posts,
        start: [(0, 0); MAX_PARTIES],
        victories,
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

fn trade(w: &mut World, player: u8, regiment: RegimentHandle, post: TradePost) -> bool {
    order(w, player, Command::Trade { regiment, post })
}

fn grub(w: &World, player: u8) -> u32 {
    w.bank(player).unwrap().grub
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

/// Runs until player `player` has more Grub than they do now, and says how
/// much more.
fn until_paid(w: &mut World, player: u8) -> u32 {
    let before = grub(w, player);
    for _ in 0..5 * 60 * SECOND {
        w.tick(&[]);
        if grub(w, player) > before {
            return grub(w, player) - before;
        }
    }
    panic!("player {player} was never paid");
}

/// What a delivery over `tiles` whole tiles pays at a full-rate post, in
/// Grub: 40 and 1.2 a tile.
fn full_pay(tiles: u32) -> u32 {
    (4000 + 120 * tiles) / 100
}

/// Player 0's Counting House at (4, 4) and a caravan beside it.
fn base(w: &mut World) -> RegimentHandle {
    put_up(w, 0, HALL, 4, 4);
    spawn(w, 0, CARAVAN, 11, 6)
}

#[test]
fn a_delivery_pays_forty_grub_and_one_point_two_a_tile() {
    // 20, 40 and 50 tiles from the Counting House's middle: 64, 88 and 100.
    for (tiles, pay) in [(20, 64), (40, 88), (50, 100)] {
        assert_eq!(full_pay(tiles as u32), pay);
        let mut w = conquest(&[], &[post_east(tiles)]);
        let caravan = base(&mut w);
        assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
        assert_eq!(until_paid(&mut w, 0), pay, "{tiles} tiles");
        // Nothing for anyone else.
        assert_eq!(grub(&w, 1), 0);
    }
}

#[test]
fn a_caravan_repeats_its_run_until_given_another_order() {
    let mut w = conquest(&[], &[post_east(20)]);
    let caravan = base(&mut w);
    assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
    assert_eq!(w.trades_at(caravan), Some(TradePost::Hamlet(0)));
    let first = until_paid(&mut w, 0);
    // It loads, comes home and is paid again, and again.
    let second = until_paid(&mut w, 0);
    let third = until_paid(&mut w, 0);
    assert_eq!((first, second, third), (64, 64, 64));
    assert_eq!(grub(&w, 0), 3 * 64);
    assert_eq!(w.trades_at(caravan), Some(TradePost::Hamlet(0)));
    // Sent again to the post it works, it keeps on.
    assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
}

#[test]
fn it_carries_the_cargo_while_it_is_on_the_way_home() {
    let mut w = conquest(&[], &[post_east(30)]);
    let caravan = base(&mut w);
    assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
    assert_eq!(w.cargo(caravan), None, "nothing before the post");
    for _ in 0..120 * SECOND {
        w.tick(&[]);
        if w.cargo(caravan).is_some() {
            break;
        }
    }
    // 40 and 36 for 30 tiles: 76.
    assert_eq!(w.cargo(caravan), Some(76));
    assert_eq!(grub(&w, 0), 0, "not banked until it is home");
    until_paid(&mut w, 0);
    assert_eq!(w.cargo(caravan), None);
}

#[test]
fn a_waystation_pays_half() {
    let mut w = conquest(&[], &[]);
    let caravan = base(&mut w);
    // Its middle, (46, 7), is 39.5 tiles from the Counting House's: 39.
    let post = put_up(&mut w, 0, WAYSTATION, 44, 5);
    assert!(trade(&mut w, 0, caravan, TradePost::Structure(post)));
    let pay = until_paid(&mut w, 0);
    assert_eq!(WAYSTATION_PERCENT, 50);
    assert_eq!(pay, full_pay(39) / 2);
    assert_eq!(pay, 43);
}

#[test]
fn an_ally_pays_in_full_and_gains_a_fifth_of_it() {
    let mut w = conquest(&[], &[]);
    let caravan = base(&mut w);
    // Their Counting House's middle, (52.5, 6.5), is 46 tiles from ours.
    let ally = put_up(&mut w, 1, HALL, 50, 4);
    assert!(trade(&mut w, 0, caravan, TradePost::Structure(ally)));
    let pay = until_paid(&mut w, 0);
    assert_eq!(pay, full_pay(46));
    assert_eq!(pay, 95);
    assert_eq!(ALLY_PERCENT, 20);
    // The ally was paid the same tick: a fifth of 95.2.
    assert_eq!(grub(&w, 1), 19);
    assert_eq!(grub(&w, 2), 0);
    // And again on the next delivery.
    until_paid(&mut w, 0);
    assert_eq!(grub(&w, 1), 19 + 19);
}

#[test]
fn only_a_post_that_pays_can_be_traded_at() {
    let mut w = conquest(&[], &[post_east(20)]);
    let caravan = base(&mut w);
    let worker = spawn(&mut w, 0, WORKER, 11, 8);
    let own_hall = w.structures().unwrap().iter().next().unwrap().handle;
    let enemy_hall = put_up(&mut w, 2, HALL, 30, 30);
    let ally_hall = put_up(&mut w, 1, HALL, 50, 4);
    let ally_station = put_up(&mut w, 1, WAYSTATION, 44, 40);
    let own_station = put_up(&mut w, 0, WAYSTATION, 20, 40);
    let granary = put_up(&mut w, 0, GRANARY, 20, 20);
    let refused = |w: &mut World, regiment, post| !trade(w, 0, regiment, post);
    // A unit that doesn't trade, and a hamlet that isn't there.
    assert!(refused(&mut w, worker, TradePost::Hamlet(0)));
    assert!(refused(&mut w, caravan, TradePost::Hamlet(1)));
    // Your own main building, an enemy's, an ally's Waystation, and a
    // building that isn't a trade post.
    assert!(refused(&mut w, caravan, TradePost::Structure(own_hall)));
    assert!(refused(&mut w, caravan, TradePost::Structure(enemy_hall)));
    assert!(refused(&mut w, caravan, TradePost::Structure(ally_station)));
    assert!(refused(&mut w, caravan, TradePost::Structure(granary)));
    // Nobody else's caravans.
    assert!(!trade(&mut w, 1, caravan, TradePost::Hamlet(0)));
    // And the ones that do pay.
    assert!(trade(&mut w, 0, caravan, TradePost::Structure(own_station)));
    assert!(trade(&mut w, 0, caravan, TradePost::Structure(ally_hall)));
    assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
    assert!(w.is_trade_post(0, TradePost::Hamlet(0)));
    assert!(!w.is_trade_post(0, TradePost::Structure(enemy_hall)));
}

#[test]
fn another_order_stops_the_run() {
    let mut w = conquest(&[], &[post_east(20)]);
    let caravan = base(&mut w);
    assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
    until_paid(&mut w, 0);
    let banked = grub(&w, 0);
    assert!(order(
        &mut w,
        0,
        Command::MoveTo {
            regiment: caravan,
            target: FxVec2::from_ints(30, 30),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
    assert_eq!(w.trades_at(caravan), None);
    run(&mut w, 5 * 60 * SECOND);
    assert_eq!(grub(&w, 0), banked, "no more deliveries");
    assert_eq!(w.idle_traders(0).count(), 1);
    // Sent to trade again, it goes back to work.
    assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
    assert_eq!(w.idle_traders(0).count(), 0);
    until_paid(&mut w, 0);
}

#[test]
fn a_caravan_that_has_cargo_when_stopped_keeps_it_and_takes_it_home() {
    let mut w = conquest(&[], &[post_east(30)]);
    let caravan = base(&mut w);
    assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
    while w.cargo(caravan).is_none() {
        w.tick(&[]);
    }
    assert!(order(
        &mut w,
        0,
        Command::MoveTo {
            regiment: caravan,
            target: FxVec2::from_ints(40, 40),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
    assert_eq!(w.cargo(caravan), Some(76), "it keeps what it carries");
    // Sent to trade, it brings that home first.
    assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
    assert_eq!(until_paid(&mut w, 0), 76);
}

#[test]
fn a_post_that_falls_ends_the_run() {
    let mut w = conquest(&[], &[]);
    let caravan = base(&mut w);
    let post = put_up(&mut w, 0, WAYSTATION, 44, 5);
    assert!(trade(&mut w, 0, caravan, TradePost::Structure(post)));
    run(&mut w, 2 * SECOND);
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: post }
    ));
    run(&mut w, SECOND);
    assert_eq!(w.trades_at(caravan), None);
}

#[test]
fn trade_pay_is_cut_by_upkeep_like_other_grub() {
    let mut w = conquest(&[], &[post_east(40)]);
    let caravan = base(&mut w);
    // 72 supply in use (the caravan's 2 and a 70-supply worker's): 75%.
    spawn(&mut w, 0, HEAVY, 8, 12);
    assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
    assert_eq!(until_paid(&mut w, 0), 88 * 75 / 100);
}

#[test]
fn a_caravan_with_no_main_building_waits_for_one() {
    let mut w = conquest(&[], &[post_east(20)]);
    let caravan = spawn(&mut w, 0, CARAVAN, 11, 6);
    assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
    run(&mut w, 60 * SECOND);
    assert_eq!(grub(&w, 0), 0);
    put_up(&mut w, 0, HALL, 4, 4);
    assert_eq!(until_paid(&mut w, 0), 64);
}

/// Player 2's granary and a spearmen regiment of theirs beside a caravan
/// of player 0's that has just taken on cargo; returns the caravan.
fn ambush(w: &mut World) -> RegimentHandle {
    let caravan = base(w);
    assert!(trade(w, 0, caravan, TradePost::Hamlet(0)));
    while w.cargo(caravan).is_none() {
        w.tick(&[]);
    }
    let at = w
        .regiments
        .iter()
        .find(|r| r.handle == caravan)
        .unwrap()
        .position;
    let spears = spawn(w, 2, SPEARS, at.x.floor_int() + 3, at.y.floor_int() + 3);
    assert!(order(
        w,
        2,
        Command::Attack {
            regiment: spears,
            target: caravan,
        }
    ));
    for _ in 0..60 * SECOND {
        if !w.regiments.iter().any(|r| r.handle == caravan) {
            return caravan;
        }
        w.tick(&[]);
    }
    panic!("the caravan survived");
}

fn piles(w: &World) -> Vec<gc_sim::NodeView> {
    w.economy()
        .unwrap()
        .nodes()
        .filter(|n| n.temporary)
        .collect()
}

#[test]
fn a_destroyed_caravan_drops_half_its_cargo_as_a_grub_pile() {
    let mut w = conquest(&[], &[post_east(30)]);
    ambush(&mut w);
    // Half of 76 Grub, lying where it fell.
    let piles = piles(&w);
    assert_eq!(piles.len(), 1);
    assert_eq!(
        (piles[0].resource, piles[0].left, piles[0].full),
        (Resource::Grub, 38, 38)
    );
    assert!(piles[0].index as usize >= w.economy().unwrap().node_positions().len());
    // The rest is lost, and nothing is banked.
    assert_eq!(grub(&w, 0), 0);
    run(&mut w, 60 * SECOND);
    assert_eq!(grub(&w, 0), 0);
}

#[test]
fn anyones_workers_gather_a_pile_and_it_is_gone_when_empty() {
    let mut w = conquest(&[], &[post_east(30)]);
    ambush(&mut w);
    let pile = piles(&w)[0];
    // The foe's granary stands by, and their worker is sent to the pile.
    put_up(&mut w, 2, GRANARY, 30, 30);
    let (px, py) = (pile.at.x.floor_int(), pile.at.y.floor_int());
    let worker = spawn(&mut w, 2, WORKER, px + 3, py + 3);
    assert!(order(
        &mut w,
        2,
        Command::Gather {
            regiment: worker,
            node: pile.index
        }
    ));
    for _ in 0..10 * 60 * SECOND {
        if piles(&w).is_empty() {
            break;
        }
        w.tick(&[]);
    }
    assert!(piles(&w).is_empty(), "the empty pile is gone");
    // Gathering at it is refused once it is.
    assert!(!order(
        &mut w,
        2,
        Command::Gather {
            regiment: worker,
            node: pile.index
        }
    ));
    // Every Grub of it came home: 38, over four loads.
    run(&mut w, 60 * SECOND);
    assert_eq!(grub(&w, 2), 38);
}

#[test]
fn a_pile_is_not_a_node_of_the_map() {
    let mut w = conquest(
        &[NodeSetup {
            resource: Resource::Grub,
            at: FxVec2::from_ints(50, 50),
            amount: 100,
        }],
        &[post_east(30)],
    );
    ambush(&mut w);
    let e = w.economy().unwrap();
    assert_eq!(
        e.node_positions().len(),
        1,
        "buildings are laid by the map's"
    );
    let nodes: Vec<_> = e.nodes().collect();
    assert_eq!(nodes.len(), 2);
    assert_eq!((nodes[0].temporary, nodes[1].temporary), (false, true));
    // The pile comes after the map's node in the numbering.
    assert_eq!(nodes[1].index, 1);
}

#[test]
fn a_caravan_cargo_is_part_of_the_state_hash() {
    let play = |load: bool| {
        let mut w = conquest(&[], &[post_east(30)]);
        let caravan = base(&mut w);
        assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
        for _ in 0..40 * SECOND {
            w.tick(&[]);
            if load && w.cargo(caravan).is_some() {
                break;
            }
        }
        (w.state_hash(), w.cargo(caravan))
    };
    // The same world and orders hash alike...
    assert_eq!(play(true), play(true));
    // ...and a caravan with cargo hashes unlike one without.
    let (loaded, cargo) = play(true);
    let (unloaded, _) = {
        let mut w = conquest(&[], &[post_east(30)]);
        let caravan = base(&mut w);
        assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
        run(&mut w, 40);
        (w.state_hash(), ())
    };
    assert!(cargo.is_some());
    assert_ne!(loaded, unloaded);
}

#[test]
fn a_worker_whose_owner_has_nowhere_for_grub_is_refused_a_seep() {
    // A seep and a vein by a Counting House that takes Marrow only.
    let nodes = [
        NodeSetup {
            resource: Resource::Grub,
            at: FxVec2::from_ints(16, 8),
            amount: 7500,
        },
        NodeSetup {
            resource: Resource::Marrow,
            at: FxVec2::from_ints(16, 14),
            amount: 3000,
        },
    ];
    let mut w = conquest(&nodes, &[post_east(30)]);
    put_up(&mut w, 0, HALL, 4, 4);
    let clerk = spawn(&mut w, 0, CLERK, 12, 8);
    let on_marrow = spawn(&mut w, 0, WORKER, 12, 12);
    // Grub has nowhere to go, so the order is refused (and so is the
    // check the game's controls make)...
    assert!(!w.can_gather(clerk, 0));
    assert!(!order(
        &mut w,
        0,
        Command::Gather {
            regiment: clerk,
            node: 0
        }
    ));
    assert_eq!(w.gathers_at(clerk), None);
    // ...but Marrow is fine, and comes home.
    assert!(w.can_gather(on_marrow, 1));
    assert!(order(
        &mut w,
        0,
        Command::Gather {
            regiment: on_marrow,
            node: 1
        }
    ));
    run(&mut w, 3 * 60 * SECOND);
    let bank = w.bank(0).unwrap();
    assert!(bank.marrow >= 20, "Marrow comes home: {}", bank.marrow);
    assert_eq!(bank.grub, 0);
    // A Grub pile is Grub too.
    let mut w = conquest(&nodes, &[post_east(30)]);
    ambush(&mut w);
    let clerk = spawn(&mut w, 0, CLERK, 12, 8);
    let pile = piles(&w)[0];
    assert!(!w.can_gather(clerk, pile.index));
    assert!(!order(
        &mut w,
        0,
        Command::Gather {
            regiment: clerk,
            node: pile.index
        }
    ));
    // A worker of a faction that has a granary can gather both.
    let worker = spawn(&mut w, 0, WORKER, 12, 8);
    assert!(w.can_gather(worker, 0));
    assert!(w.can_gather(worker, pile.index));
}

#[test]
fn an_ally_that_is_out_of_the_match_when_the_cargo_comes_home_gets_no_share() {
    let mut w = conquest_for(&[], &[post_east(30)], Victories::BOTH);
    // The foe keeps a main building, so the match goes on.
    put_up(&mut w, 2, HALL, 30, 30);
    let caravan = base(&mut w);
    let ally = put_up(&mut w, 1, HALL, 50, 4);
    assert!(trade(&mut w, 0, caravan, TradePost::Structure(ally)));
    while w.cargo(caravan).is_none() {
        w.tick(&[]);
    }
    // Stopped with its cargo, which still has the ally's share in it.
    assert!(order(
        &mut w,
        0,
        Command::MoveTo {
            regiment: caravan,
            target: FxVec2::from_ints(40, 40),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
    // The ally loses its only main building, and is out ninety seconds on.
    assert!(order(
        &mut w,
        1,
        Command::RemoveStructure { structure: ally }
    ));
    while !w.is_out(1) {
        w.tick(&[]);
    }
    assert!(trade(&mut w, 0, caravan, TradePost::Hamlet(0)));
    assert_eq!(until_paid(&mut w, 0), full_pay(46));
    run(&mut w, SECOND);
    assert_eq!(grub(&w, 1), 0, "an ally that is out is paid nothing");
}

#[test]
fn what_dead_workers_took_goes_back_to_their_pile_before_it_is_cleared() {
    let mut w = conquest(&[], &[post_east(30)]);
    ambush(&mut w);
    let pile = piles(&w)[0];
    put_up(&mut w, 2, GRANARY, 30, 30);
    let (px, py) = (pile.at.x.floor_int(), pile.at.y.floor_int());
    let workers: Vec<_> = (0..4)
        .map(|i| spawn(&mut w, 2, WORKER, px + 1 + i, py + 1))
        .collect();
    for &regiment in &workers {
        assert!(order(
            &mut w,
            2,
            Command::Gather {
                regiment,
                node: pile.index
            }
        ));
    }
    // Four loads of 10 take the whole pile of 38 between them.
    while piles(&w).iter().all(|n| n.left > 0) {
        w.tick(&[]);
    }
    assert_eq!(piles(&w)[0].left, 0);
    let commands: Vec<_> = workers
        .iter()
        .map(|&regiment| PlayerCommand {
            player: 2,
            command: Command::RemoveRegiment { regiment },
        })
        .collect();
    w.tick(&commands);
    w.tick(&[]);
    // All of it is back on the pile, which is still there.
    let left: Vec<_> = piles(&w).iter().map(|n| n.left).collect();
    assert_eq!(left, [38]);
}
