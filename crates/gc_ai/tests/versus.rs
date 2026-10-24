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
use gc_ai::{
    Difficulty, Doctrine, Knobs, Mind, MindConfig, Minds, Perception, Plan, Stance, Terrain,
};
use gc_fixed::Pcg32;
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{Capacities, MatchSetup, PlayerCommand, PlayerId, UnitId, World};
use glam::Vec2;

const MAP: u32 = 256;
/// Spears, pikes, fast troops and archers (DRILL_UNITS 0, 3, 1, 2).
const ARMY: [UnitId; 10] = [0, 0, 0, 0, 3, 3, 1, 1, 2, 2];
/// A battle is over once the simulation's verdict names who held the
/// field, or after eight minutes, when whoever has more soldiers still
/// fighting wins.
const LIMIT: u32 = 8 * 60 * 20;

/// Who plays a side.
#[derive(Clone, Copy, Debug)]
enum Side {
    Mind(Difficulty),
    /// A difficulty with some knobs changed, for tuning.
    Tuned(Difficulty, fn(&mut Knobs)),
    /// A difficulty told to fight with one plan throughout (no rehearsals).
    Planned(Difficulty, Plan),
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

/// Battle `seed`'s field: two armies facing each other across 70 to 100
/// tiles of open ground, and the side under test (the other is `1 -` it).
fn field(seed: u64) -> (World, Terrain, PlayerId) {
    let mut rng = Pcg32::new(seed, 7);
    let mut world = World::new(&MatchSetup {
        seed,
        capacities: Capacities { regiments: 32 },
        units: &DRILL_UNITS,
        map_tiles: MAP,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    let terrain = Terrain::open(MAP);
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
    (world, terrain, (seed % 2) as PlayerId)
}

/// How `party` is set up in battle `seed`, at the General's level.
fn config(seed: u64, party: PlayerId) -> MindConfig {
    MindConfig {
        party,
        difficulty: Difficulty::General,
        // Personalities take turns, so every one is tested.
        personality: (seed / 2) as usize + usize::from(party) * 3,
        seed: seed * 2 + u64::from(party),
    }
}

/// One battle: `side` against `against`.
fn fight(seed: u64, side: Side, against: Side) -> Outcome {
    let (mut world, terrain, us) = field(seed);
    let them = 1 - us;
    let mut out: Vec<PlayerCommand> = Vec::new();
    let mut minds = Minds::new(terrain);
    let mut seat = |party: PlayerId, side: Side| -> Player {
        let config = config(seed, party);
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
            Side::Planned(difficulty, plan) => {
                let mut knobs = difficulty.knobs();
                knobs.rehearse = false;
                let mut mind = Mind::with_knobs(
                    MindConfig {
                        difficulty,
                        ..config
                    },
                    knobs,
                );
                mind.follow(plan);
                minds.add(mind);
                Player::Mind
            }
            Side::Charge => Player::Bot(ChargeBot::new(party)),
            Side::Idle => Player::Idle,
        }
    };
    let mut players = [seat(us, side), seat(them, against)];
    let mut view = Perception::new();
    let mut winner = None;
    let mut last_verdict = None;
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
        if let Some(v) = world.verdict() {
            winner = Some(v.holder);
            break;
        }
        if trace(seed)
            && let Some(v) = minds.get(us).and_then(Mind::verdict)
            && Some(v.tick) != last_verdict
        {
            last_verdict = Some(v.tick);
            print_verdict(v);
        }
        if world.tick_count().is_multiple_of(20) {
            view.capture(&world);
            if trace(seed) && world.tick_count().is_multiple_of(40) {
                print_field(&view, us, minds.get(us));
            }
        }
    }
    view.capture(&world);
    let standing = |party: PlayerId| {
        let (mut left, mut all) = (0.0, 0.0);
        for r in view.regiments.iter().filter(|r| r.owner == party) {
            all += f32::from(r.full);
            if !r.broken() {
                left += f32::from(r.models);
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

fn print_verdict(v: &gc_ai::Verdict) {
    println!(
        "t={}s rehearsed: {} ({:+.2}, before {:?})",
        v.tick / 20,
        v.plan.describe(),
        v.score,
        v.before
    );
    for (plan, score) in &v.plans {
        println!("    {:>40} {score:+.2}", plan.describe());
    }
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
/// `AI_AGAINST=Recruit|Veteran|General|Warlord`.
#[test]
#[ignore = "traces one battle, run by hand"]
fn one() {
    let seed = std::env::var("AI_TRACE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let r = fight(seed, Side::Mind(level()), against());
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
        Ok("Warlord") => Side::Mind(Difficulty::Warlord),
        _ => Side::Charge,
    }
}

/// `AI_LEVEL`: the level under test in tuning runs (the General by default).
fn level() -> Difficulty {
    match std::env::var("AI_LEVEL").as_deref() {
        Ok("Recruit") => Difficulty::Recruit,
        Ok("Veteran") => Difficulty::Veteran,
        Ok("Warlord") => Difficulty::Warlord,
        _ => Difficulty::General,
    }
}

/// `AI_LEVEL` against `AI_AGAINST` over `AI_BATTLES` from `AI_FROM`, in
/// parallel, with the time it took. `AI_LEVEL=Hands` is the Warlord without
/// its rehearsals.
#[test]
#[ignore = "slow: hundreds of arena battles, run by hand"]
fn duel() {
    let level = match std::env::var("AI_LEVEL").as_deref() {
        Ok("Hands") => Side::Tuned(Difficulty::Warlord, |k| k.rehearse = false),
        _ => Side::Mind(level()),
    };
    let against = against();
    let t0 = std::time::Instant::now();
    let n = battles();
    let wins = contest_parallel(level, against, first_seed()..=first_seed() + n - 1);
    println!(
        "{level:?} v {against:?}: won {wins} of {n} in {:?}",
        t0.elapsed()
    );
}

/// Win rates over many battles, every level against charge-straight-in and
/// against the level below.
#[test]
#[ignore = "slow: hundreds of arena battles, run by hand"]
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
#[ignore = "slow: arena battles for scale, run by hand"]
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
#[ignore = "slow: arena battles per tuning knob, run by hand"]
fn knob_study() {
    let n = battles();
    let level = match std::env::var("AI_LEVEL").as_deref() {
        Ok("Recruit") => Difficulty::Recruit,
        Ok("Veteran") => Difficulty::Veteran,
        _ => Difficulty::General,
    };
    let against = against();
    let only = std::env::var("AI_TUNE").ok();
    let tunes: [Tune; 11] = [
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
        ("hunt leash 55", |k| k.hunt_leash = 55.0),
    ];
    for (name, tune) in tunes {
        if only.as_ref().is_some_and(|o| !name.contains(o.as_str())) {
            continue;
        }
        let from = first_seed();
        let wins = contest_parallel(Side::Tuned(level, tune), against, from..=from + n - 1);
        println!("{level:?} with {name} v {against:?}: won {wins} of {n}");
    }
}

/// Every plan fought throughout by the Warlord's hands against `AI_AGAINST`
/// (the General by default): is one plan simply better?
#[test]
#[ignore = "slow: every plan over many arena battles, run by hand"]
fn plans() {
    let against = match std::env::var("AI_AGAINST") {
        Ok(_) => against(),
        Err(_) => Side::Mind(Difficulty::General),
    };
    let seeds = first_seed()..=first_seed() + battles() - 1;
    for doctrine in Doctrine::ALL {
        for stance in Stance::ALL {
            let plan = Plan { doctrine, stance };
            let wins = contest_parallel(
                Side::Planned(Difficulty::Warlord, plan),
                against,
                seeds.clone(),
            );
            println!("{:>40}: won {wins} of {}", plan.describe(), battles());
        }
    }
}

/// [`contest`] spread over the machine's cores.
fn contest_parallel(side: Side, against: Side, seeds: std::ops::RangeInclusive<u64>) -> u32 {
    let seeds: Vec<u64> = seeds.collect();
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    std::thread::scope(|s| {
        seeds
            .chunks(seeds.len().div_ceil(threads).max(1))
            .map(|c| s.spawn(move || contest(side, against, c[0]..=c[c.len() - 1])))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().unwrap())
            .sum()
    })
}

// The Warlord rehearses in the real simulation, too slow unoptimised: CI
// runs these with `cargo test --release -p gc_ai --test versus warlord`.

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "too slow unoptimised: CI runs it in release"
)]
fn the_warlord_beats_the_general_seven_times_in_ten() {
    let warlord = Side::Mind(Difficulty::Warlord);
    let wins = contest_parallel(warlord, Side::Mind(Difficulty::General), 1..=24);
    assert!(wins >= 17, "won {wins} of 24");
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "too slow unoptimised: CI runs it in release"
)]
fn a_warlord_battle_plays_out_the_same_every_time() {
    let warlord = Side::Mind(Difficulty::Warlord);
    let general = Side::Mind(Difficulty::General);
    let (a, b) = std::thread::scope(|s| {
        let a = s.spawn(|| fight(5, warlord, general));
        let b = s.spawn(|| fight(5, warlord, general));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert!(a.journal.iter().any(|l| l.contains("Rehearsed")), "{a:?}");
    assert_eq!(
        (a.ticks, a.orders, a.journal),
        (b.ticks, b.orders, b.journal)
    );
}

/// The Warlord's reader names the plan of the General it faces (its
/// doctrine, and whether it holds or advances) before the armies meet, in
/// at least 9 of 10 battles.
#[test]
fn the_reader_names_the_enemy_plan_before_contact() {
    use gc_ai::reading::Reading;
    use std::sync::Arc;
    let (mut doctrine, mut posture) = (0, 0);
    for seed in 1..=10 {
        let (mut world, terrain, us) = field(seed);
        let them = 1 - us;
        let terrain = Arc::new(terrain);
        let mut minds: Vec<Mind> = (0..2).map(|p| Mind::new(config(seed, p))).collect();
        let mut reading = Reading::new(them, Difficulty::General, seed);
        let mut view = Perception::new();
        let mut out = Vec::new();
        for _ in 0..15 * 20 {
            view.capture(&world);
            reading.tick(&view, &terrain, &world);
            out.clear();
            for m in &mut minds {
                m.tick(&view, &terrain, &world, &mut out);
            }
            world.tick(&out);
        }
        view.capture(&world);
        assert!(
            view.regiments.iter().all(|r| !r.engaged),
            "battle {seed} met within 15 s"
        );
        let guess = reading.guess().expect("a guess");
        let real = minds[them as usize].captain();
        doctrine += usize::from(guess.captain.doctrine == real.doctrine);
        posture += usize::from(guess.captain.posture == real.posture);
    }
    assert!(doctrine >= 9, "named the doctrine in {doctrine} of 10");
    assert!(posture >= 9, "named the posture in {posture} of 10");
}
