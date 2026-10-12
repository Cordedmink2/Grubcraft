//! The battle AI in the arena (docs/18-ai.md, "Testing, tuning and
//! explaining"): against a scripted "charge straight in" opponent, which
//! the General must beat nine battles in ten (AI-1), and against itself,
//! where each level must beat the one below it at least 70% of the time.
//!
//! Both sides field the same army, set out the same way, facing each other
//! across open ground. Which side plays which end of the field alternates,
//! and each battle starts from a different seed and spot.
//!
//! Tuning tools (ignored by default, run with `--release`):
//! - `AI_BATTLES=120 cargo test -p gc_ai --release --test versus sweep -- --ignored --nocapture`
//! - `AI_TRACE=<seed> cargo test -p gc_ai --release --test versus one -- --ignored --nocapture`
//!   prints the field every two seconds of one battle.
//! - `knob_study` and `yardsticks`: see below.

use gc_ai::army::{Placement, deploy, spawn_commands};
use gc_ai::scripted::ChargeBot;
use gc_ai::{Difficulty, Knobs, Mind, MindConfig, Minds, Perception, Terrain, holding_the_field};
use gc_fixed::Pcg32;
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{Capacities, MatchSetup, PlayerCommand, PlayerId, UnitId, World};
use glam::Vec2;

const MAP: u32 = 256;
/// Spears, pikes, fast troops and archers (DRILL_UNITS 0, 3, 1, 2).
const ARMY: [UnitId; 10] = [0, 0, 0, 0, 3, 3, 1, 1, 2, 2];
/// A battle is over once one side has held the field this long, ticks.
const HELD: u32 = 10 * 20;
/// Or after eight minutes, when whoever has more soldiers still fighting wins.
const LIMIT: u32 = 8 * 60 * 20;

/// Who plays a side.
#[derive(Clone, Copy, Debug)]
enum Side {
    Mind(Difficulty),
    /// A difficulty with some knobs changed, for tuning.
    Tuned(Difficulty, fn(&mut Knobs)),
    /// Charge straight in.
    Charge,
    /// Stands still; only the simulation's own reflexes fight back.
    Idle,
}

#[derive(Debug)]
struct Outcome {
    /// Whether the side under test won.
    won: bool,
    ticks: u32,
    /// Share of each side's soldiers still standing and unbroken.
    left: f32,
    their_left: f32,
    orders: u32,
    /// The side under test's orders by priority, and attention spent.
    order_mix: String,
    journal: Vec<String>,
    /// The opponent's journal, if it is a mind.
    their_journal: Vec<String>,
}

enum Player {
    Mind,
    Bot(ChargeBot),
    Idle,
}

/// One battle: `side` against `against`.
fn fight(seed: u64, side: Side, against: Side) -> Outcome {
    let mut rng = Pcg32::new(seed, 7);
    let us: PlayerId = (seed % 2) as PlayerId;
    let them: PlayerId = 1 - us;
    let mut world = World::new(&MatchSetup {
        seed,
        capacities: Capacities { regiments: 32 },
        units: &DRILL_UNITS,
        map_tiles: MAP,
        nav: &[],
        teams: &[],
    });
    let terrain = Terrain::open(MAP);
    // Two armies facing each other across 70 to 100 tiles of open ground.
    let apart = 70.0 + rng.below(31) as f32;
    let x = 100.0 + rng.below(57) as f32;
    let tilt = (rng.below(41) as f32 - 20.0) / 100.0;
    let axis = Vec2::new(tilt, 1.0).normalize();
    let mid = Vec2::new(x, 128.0);
    let mut out: Vec<PlayerCommand> = Vec::new();
    for party in 0..2 {
        let towards = if party == 0 { axis } else { -axis };
        let centre = mid - towards * apart * 0.5;
        let placed: Vec<Placement> = deploy(&DRILL_UNITS, &ARMY, centre, towards, &terrain);
        spawn_commands(party, &placed, &mut out);
    }
    world.tick(&out);

    let mut minds = Minds::new(terrain);
    let mut seat = |party: PlayerId, side: Side| -> Player {
        let config = MindConfig {
            party,
            difficulty: Difficulty::General,
            // Personalities take turns, so every one is tested.
            personality: (seed / 2) as usize + party as usize * 3,
            seed: seed * 2 + party as u64,
        };
        match side {
            Side::Mind(difficulty) => {
                minds.add(Mind::new(MindConfig {
                    difficulty,
                    ..config
                }));
                Player::Mind
            }
            Side::Tuned(difficulty, tune) => {
                let mut knobs = difficulty.knobs();
                tune(&mut knobs);
                minds.add(Mind::with_knobs(
                    MindConfig {
                        difficulty,
                        ..config
                    },
                    knobs,
                ));
                Player::Mind
            }
            Side::Charge => Player::Bot(ChargeBot::new(party)),
            Side::Idle => Player::Idle,
        }
    };
    let mut players = [seat(us, side), seat(them, against)];
    let mut view = Perception::new();
    let mut held_by: Option<(PlayerId, u32)> = None;
    let mut winner = None;
    while world.tick_count() < LIMIT {
        out.clear();
        view.capture(&world);
        for p in &mut players {
            if let Player::Bot(bot) = p {
                bot.tick(&view, &mut out);
            }
        }
        minds.tick(&world, &mut out);
        world.tick(&out);
        if world.tick_count().is_multiple_of(20) {
            view.capture(&world);
            if trace(seed) && world.tick_count().is_multiple_of(40) {
                print_field(&view, us, minds.get(us));
            }
            match (holding_the_field(&view), held_by) {
                (Some(p), Some((q, since))) if p == q => {
                    if world.tick_count() >= since + HELD {
                        winner = Some(p);
                        break;
                    }
                }
                (Some(p), _) => held_by = Some((p, world.tick_count())),
                (None, _) => held_by = None,
            }
        }
    }
    view.capture(&world);
    let standing = |party: PlayerId| {
        let (mut left, mut all) = (0.0, 0.0);
        for r in view.regiments.iter().filter(|r| r.owner == party) {
            all += r.full as f32;
            if !r.broken() {
                left += r.models as f32;
            }
        }
        left / all.max(1.0)
    };
    let (left, their_left) = (standing(us), standing(them));
    let mind = minds.get(us);
    Outcome {
        won: match winner {
            Some(p) => p == us,
            None => left > their_left,
        },
        ticks: world.tick_count(),
        left,
        their_left,
        orders: mind.map_or(0, Mind::orders_sent),
        order_mix: mind.map_or(String::new(), |m| {
            let o = m.orders();
            format!(
                "orders by priority {:?}, waited for attention {:?}, attention spent {}",
                &o.decided[..5],
                &o.waited[..5],
                o.spent
            )
        }),
        journal: journal_of(mind),
        their_journal: journal_of(minds.get(them)),
    }
}

fn journal_of(mind: Option<&Mind>) -> Vec<String> {
    mind.map_or(Vec::new(), |m| {
        m.journal()
            .entries()
            .map(|e| format!("{:>5}: {}", e.tick, e.text))
            .collect()
    })
}

/// Battles `seeds` of `side` against `against`; returns the wins.
fn contest(side: Side, against: Side, seeds: std::ops::RangeInclusive<u64>) -> u32 {
    let mut wins = 0;
    let verbose = std::env::var_os("AI_VERBOSE").is_some();
    for seed in seeds {
        let r = fight(seed, side, against);
        if verbose || !r.won {
            println!(
                "{side:?} v {against:?}, seed {seed}: {} in {}s, {:.0}% left to {:.0}%, {} orders",
                if r.won { "won" } else { "LOST" },
                r.ticks / 20,
                r.left * 100.0,
                r.their_left * 100.0,
                r.orders,
            );
        }
        if std::env::var_os("AI_ORDERS").is_some() {
            println!("    {}", r.order_mix);
        }
        if std::env::var_os("AI_JOURNAL").is_some() {
            for line in &r.journal {
                println!("    {line}");
            }
        }
        wins += u32::from(r.won);
    }
    wins
}

/// `AI_FROM=<seed>`: start tuning runs from another seed, to check a
/// change on battles it wasn't tuned on.
fn first_seed() -> u64 {
    std::env::var("AI_FROM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

fn battles() -> u64 {
    std::env::var("AI_BATTLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120)
}

#[test]
fn the_general_beats_charge_straight_in_nine_times_in_ten() {
    let wins = contest(Side::Mind(Difficulty::General), Side::Charge, 1..=40);
    assert!(wins >= 36, "won {wins} of 40");
}

#[test]
fn the_veteran_beats_charge_straight_in_more_often_than_not() {
    let wins = contest(Side::Mind(Difficulty::Veteran), Side::Charge, 1..=40);
    assert!(wins > 20, "won {wins} of 40");
}

#[test]
fn the_general_beats_the_veteran_seven_times_in_ten() {
    let general = Side::Mind(Difficulty::General);
    let wins = contest(general, Side::Mind(Difficulty::Veteran), 1..=30);
    assert!(wins >= 21, "won {wins} of 30");
}

#[test]
fn the_veteran_beats_the_recruit_seven_times_in_ten() {
    let veteran = Side::Mind(Difficulty::Veteran);
    let wins = contest(veteran, Side::Mind(Difficulty::Recruit), 1..=30);
    assert!(wins >= 21, "won {wins} of 30");
}

#[test]
fn a_battle_plays_out_the_same_every_time() {
    let general = Side::Mind(Difficulty::General);
    let a = fight(3, general, Side::Charge);
    let b = fight(3, general, Side::Charge);
    assert!(a.orders > 20, "{a:?}");
    assert_eq!(
        (a.ticks, a.orders, a.journal),
        (b.ticks, b.orders, b.journal)
    );
}

/// `AI_TRACE=<seed>` prints the field every two seconds of that battle.
fn trace(seed: u64) -> bool {
    std::env::var("AI_TRACE").is_ok_and(|v| v == seed.to_string())
}

fn print_field(view: &Perception, us: PlayerId, mind: Option<&Mind>) {
    println!(
        "t={}s  {}",
        view.tick / 20,
        mind.map_or(String::new(), Mind::summary)
    );
    for r in &view.regiments {
        let role = mind
            .and_then(|mind| mind.captain().members.iter().find(|m| m.handle == r.handle))
            .map_or(String::new(), |m| {
                let target = m
                    .target
                    .map_or(String::new(), |t| format!(" ->{}", t.index));
                format!("{}{target}", m.role.name())
            });
        println!(
            "  {}{:>2} u{} ({:5.1},{:5.1}) f({:4.1},{:4.1}) {:>2}/{:>2} {:?}{}{} m{} {}",
            if r.owner == us { "A" } else { "b" },
            r.handle.index,
            r.unit,
            r.pos.x,
            r.pos.y,
            r.facing.x,
            r.facing.y,
            r.models,
            r.full,
            r.morale,
            if r.engaged { " ENG" } else { "" },
            if r.moving { " mov" } else { "" },
            r.morale_points,
            role,
        );
    }
}

/// One General battle, traced: against charge-straight-in, or against
/// `AI_AGAINST=Recruit|Veteran|General`.
#[test]
#[ignore]
fn one() {
    let seed = std::env::var("AI_TRACE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let r = fight(seed, Side::Mind(Difficulty::General), against());
    println!(
        "{} in {}s: {}",
        if r.won { "won" } else { "LOST" },
        r.ticks / 20,
        r.order_mix
    );
    for line in &r.journal {
        println!("    {line}");
    }
    if !r.their_journal.is_empty() {
        println!("  they:");
        for line in &r.their_journal {
            println!("    {line}");
        }
    }
}

/// `AI_AGAINST`: the opponent for tuning runs (charge straight in by default).
fn against() -> Side {
    match std::env::var("AI_AGAINST").as_deref() {
        Ok("Recruit") => Side::Mind(Difficulty::Recruit),
        Ok("Veteran") => Side::Mind(Difficulty::Veteran),
        Ok("General") => Side::Mind(Difficulty::General),
        _ => Side::Charge,
    }
}

/// Win rates over many battles, every level against charge-straight-in and
/// against the level below.
#[test]
#[ignore]
fn sweep() {
    let n = battles();
    let seeds = first_seed()..=first_seed() + n - 1;
    let levels = [
        Difficulty::General,
        Difficulty::Veteran,
        Difficulty::Recruit,
    ];
    for d in levels {
        let wins = contest(Side::Mind(d), Side::Charge, seeds.clone());
        println!("{d:?} v charge straight in: won {wins} of {n}");
    }
    for pair in levels.windows(2) {
        let wins = contest(Side::Mind(pair[0]), Side::Mind(pair[1]), seeds.clone());
        println!("{:?} v {:?}: won {wins} of {n}", pair[0], pair[1]);
    }
}

/// What a fair fight and doing nothing score, for scale.
#[test]
#[ignore]
fn yardsticks() {
    let n = battles();
    for side in [Side::Charge, Side::Idle] {
        let wins = contest(side, Side::Charge, 1..=n);
        println!("{side:?} v charge straight in: won {wins} of {n}");
    }
}

type Tune = (&'static str, fn(&mut Knobs));

/// How much each knob is worth: `AI_LEVEL=Veteran` picks the level,
/// `AI_AGAINST=Veteran` the opponent (charge straight in by default), and
/// `AI_TUNE=<text>` runs only the variants whose name contains it.
#[test]
#[ignore]
fn knob_study() {
    let n = battles();
    let level = match std::env::var("AI_LEVEL").as_deref() {
        Ok("Recruit") => Difficulty::Recruit,
        Ok("Veteran") => Difficulty::Veteran,
        _ => Difficulty::General,
    };
    let against = against();
    let only = std::env::var("AI_TUNE").ok();
    let tunes: [Tune; 10] = [
        ("as is", |_| {}),
        ("reaction 0.8 s", |k| k.reaction_ticks = 16),
        ("45 orders a minute", |k| k.orders_per_minute = 45),
        ("thinks every 5 ticks", |k| k.think_ticks = 5),
        ("no morale targeting", |k| k.morale_targeting = false),
        ("no focus", |k| k.focus_fire = false),
        ("no saving wavering", |k| k.save_wavering = false),
        ("untimed hammer", |k| k.timed_hammer = false),
        ("engages at 10", |k| k.engage_tiles = 10.0),
        ("slips 10%", |k| k.slip_percent = 10),
    ];
    for (name, tune) in tunes {
        if only.as_ref().is_some_and(|o| !name.contains(o.as_str())) {
            continue;
        }
        let from = first_seed();
        let wins = contest(Side::Tuned(level, tune), against, from..=from + n - 1);
        println!("{level:?} with {name} v {against:?}: won {wins} of {n}");
    }
}
