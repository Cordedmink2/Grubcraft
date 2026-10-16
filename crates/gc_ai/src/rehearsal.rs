//! Rehearsal: thinking ahead with the real simulation (docs/18-ai.md,
//! "Rehearsal").
//!
//! The estimator is quick but rough. For the decision that matters most,
//! which plan to fight with, the Warlord does what a real general wishes
//! they could: it plays the battle forward in its head under the real
//! rules. Every [`EVERY`] ticks it sets the field out in scratch worlds
//! exactly as it sees it (on the same map, shared rather than rebuilt, with
//! every regiment where it stands, as many soldiers as it shows and the
//! nerve its banner shows: engine hooks 5 and 6), and in each one tries one
//! plan: a doctrine its army can fight with, and whether to wait for the
//! enemy, march on them, or decide that as it goes.
//!
//! The enemy in a scratch world is the commander the Warlord reads it to be
//! ([`crate::reading`]), with its plan made and its line laid out; an enemy
//! not read yet, and any ally, is a sound commander of middling temper
//! starting afresh. Each scratch world runs the real simulation [`HORIZON`]
//! ahead (or until one side has held the field for a few seconds), [`SEEDS`]
//! times with different luck, and is scored by who is left standing and how
//! the fight from there looks to the estimator. The plan with the best
//! average is the one it follows, if it beats the plan it follows now by
//! [`STICK`].
//!
//! It is fair: a scratch world holds only what the mind can see, never the
//! real world's hidden state (orders, wounds, charge and reload timers), and
//! every commander in it sees only what its side would, so a hidden reserve
//! or an ambush still surprises it. It runs on worker threads while the game
//! goes on; the answer is taken [`DELAY`] ticks after it was asked for, so
//! the same battle always plays out the same way on any machine (a machine
//! too slow to finish in time waits for it, except in a game played live,
//! where the answer is taken when it is ready: see [`crate::Minds::live`]).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};

use gc_platform::tagged_alloc::Tag;
use gc_platform::thread::{spawn_scoped_tagged, spawn_tagged, workers_leaving};
use gc_sim::{MoraleState, PlayerCommand, PlayerId, Standing, World};

use crate::captain::{self, Captain, Doctrine, Plan, Stance};
use crate::difficulty::{Difficulty, Knobs};
use crate::estimator::{self, Approach, Force};
use crate::perception::{Class, Perception, Seen, TICKS, to_fx_vec, vec_angle};
use crate::personality::{DoctrineLikes, Faction, Personality};
use crate::reading::Guess;
use crate::terrain::Terrain;
use crate::{Mind, MindConfig, Minds};

/// How far ahead a rehearsal plays, ticks.
pub const HORIZON: u32 = 90 * TICKS as u32;
/// Ticks from asking for a round of rehearsals to taking its answer.
pub const DELAY: u32 = TICKS as u32;
/// Ticks between rounds.
pub const EVERY: u32 = 10 * TICKS as u32;
/// Rounds of different Warlords start on one of this many beats within
/// [`EVERY`].
pub const BEATS: u32 = 8;
const _: () = assert!(EVERY.is_multiple_of(BEATS));
/// How many times each plan is played, with different luck.
pub const SEEDS: u64 = 6;
/// A plan must beat the one it follows by this much to replace it.
pub const STICK: f32 = 0.1;
/// Cores a round leaves alone, for the render and sim threads.
const SPARE_CORES: usize = 2;
/// A rehearsal stops early once one side has held the field this long.
const DECIDED: u32 = 5 * TICKS as u32;
/// The estimator's view of the fight left at the end counts this much
/// against what has happened by then.
const LOOK_BEYOND: f32 = 0.5;
/// How much a regiment counts for, by its state of mind, per share of its
/// full strength still standing.
const WAVERING: f32 = 0.75;
const ROUTING: f32 = 0.2;

/// What a round of rehearsals decided.
#[derive(Clone, Debug)]
pub struct Verdict {
    /// The tick the field was set out from.
    pub tick: u32,
    pub plan: Plan,
    /// Its score, from about -1.5 (we are swept away) to +1.5 (they are),
    /// and the score of the plan it followed before (if it had one).
    pub score: f32,
    pub before: Option<f32>,
    /// Every plan's score, best first.
    pub plans: Vec<(Plan, f32)>,
}

/// A round of rehearsals under way. Dropping it unfinished stops its
/// rehearsals.
pub struct Round {
    /// The tick the field was set out from, and the tick to take the answer.
    pub asked: u32,
    pub due: u32,
    job: Option<JoinHandle<Vec<(Plan, f32)>>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Round {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// What every rehearsal of a round shares.
struct Brief {
    party: PlayerId,
    view: Perception,
    terrain: Arc<Terrain>,
    captain: Captain,
    knobs: Knobs,
    persona: &'static Personality,
    /// Who the enemy seems to be, by party.
    foes: Vec<Guess>,
    seed: u64,
}

impl Round {
    /// Set out the field as `view` shows it to `mind` and start rehearsing
    /// every plan its army can fight with, against `foes`. `world` lends
    /// its map. None if there is nothing to rehearse.
    pub fn start(
        world: &World,
        view: &Perception,
        terrain: &Arc<Terrain>,
        mind: &Mind,
        foes: Vec<Guess>,
    ) -> Option<Round> {
        let plans = plans_for(view, terrain, mind);
        if plans.is_empty() || view.regiments.is_empty() {
            return None;
        }
        let capacity = view
            .regiments
            .iter()
            .map(|r| r.handle.index + 1)
            .max()
            .unwrap_or(1);
        let seed = u64::from(view.tick) << 8 | u64::from(mind.party);
        let template = world.scratch(capacity, seed);
        let brief = Brief {
            party: mind.party,
            view: view.clone(),
            terrain: terrain.clone(),
            captain: mind.captain.clone(),
            knobs: Knobs {
                rehearse: false,
                slip_percent: 0,
                ..mind.knobs
            },
            persona: mind.persona,
            foes,
            seed,
        };
        let stop = Arc::new(AtomicBool::new(false));
        let job = {
            let stop = stop.clone();
            spawn_tagged(Tag::Ai, move || {
                rehearse_all(&template, &brief, &plans, &stop)
            })
        };
        Some(Round {
            asked: view.tick,
            due: view.tick + DELAY,
            job: Some(job),
            stop,
        })
    }

    /// Whether the round's rehearsals are done.
    pub fn ready(&self) -> bool {
        self.job.as_ref().is_some_and(JoinHandle::is_finished)
    }

    /// Wait for the round's rehearsals and pick the plan to follow;
    /// `current` is the plan it follows now, which a new one must beat by
    /// [`STICK`] to replace.
    pub fn finish(mut self, current: Option<Plan>) -> Verdict {
        let job = self.job.take().expect("a round is finished once");
        let mut plans = job.join().expect("a rehearsal panicked");
        plans.sort_by(|a, b| b.1.total_cmp(&a.1));
        let before = current.and_then(|c| plans.iter().find(|(p, _)| *p == c).map(|(_, s)| *s));
        let (mut plan, mut score) = plans[0];
        if let (Some(c), Some(b)) = (current, before)
            && score < b + STICK
        {
            (plan, score) = (c, b);
        }
        Verdict {
            tick: self.asked,
            plan,
            score,
            before,
            plans,
        }
    }
}

/// The plans worth rehearsing for `mind`'s army in `view`: every doctrine
/// its level knows and its army can fight with (an ambush only where there
/// is cover, before the fighting starts), each waiting, advancing and as it
/// sees fit.
fn plans_for(view: &Perception, terrain: &Terrain, mind: &Mind) -> Vec<Plan> {
    let party = mind.party;
    let (mut line, mut shock, mut ranged) = (0, 0, 0);
    for r in view
        .regiments
        .iter()
        .filter(|r| r.owner == party && !r.broken())
    {
        match view.unit(r).class {
            Class::Line => line += 1,
            Class::Shock => shock += 1,
            Class::Ranged => ranged += 1,
        }
    }
    let mut plans = Vec::new();
    for &doctrine in mind.knobs.doctrines {
        if !captain::fits(doctrine, line, shock, ranged)
            || doctrine == Doctrine::Ambush && !(terrain.has_cover() && mind.captain.may_ambush())
        {
            continue;
        }
        for stance in Stance::ALL {
            plans.push(Plan { doctrine, stance });
        }
    }
    plans
}

/// Play every plan [`SEEDS`] times, spread over the machine's cores less
/// [`SPARE_CORES`] (kept for the game), and average each plan's scores
/// (unless `stop` is set first). The results are the same whatever the cores.
fn rehearse_all(
    template: &World,
    brief: &Brief,
    plans: &[Plan],
    stop: &AtomicBool,
) -> Vec<(Plan, f32)> {
    let seeds = SEEDS as usize;
    let jobs = plans.len() * seeds;
    let workers = workers_leaving(SPARE_CORES);
    let capacity = template.regiments.capacity() as u32;
    let next = AtomicUsize::new(0);
    let mut scores = vec![0.0; jobs];
    thread::scope(|s| {
        let handles: Vec<_> = (0..workers.min(jobs))
            .map(|_| {
                // A world can't be shared between threads: each worker sets
                // out from its own empty copy of the map.
                let map = template.scratch(capacity, 0);
                let next = &next;
                // Rehearsals take very different times (some are decided in
                // seconds), so each worker takes the next one when done.
                spawn_scoped_tagged(s, Tag::Ai, move || {
                    let mut done = Vec::new();
                    loop {
                        let j = next.fetch_add(1, Ordering::Relaxed);
                        if j >= jobs || stop.load(Ordering::Relaxed) {
                            return done;
                        }
                        let k = (j % seeds) as u64;
                        done.push((j, play(&map, brief, plans[j / seeds], k)));
                    }
                })
            })
            .collect();
        for h in handles {
            for (j, score) in h.join().expect("a rehearsal panicked") {
                scores[j] = score;
            }
        }
    });
    // Summed in the same order every time, so the same on every machine.
    plans
        .iter()
        .zip(scores.chunks(seeds))
        .map(|(&plan, mine)| (plan, mine.iter().sum::<f32>() / SEEDS as f32))
        .collect()
}

/// Play `plan` once, with luck `k`, on `map` (an empty scratch world) set
/// out as the brief's view shows the field, and score how it went.
fn play(map: &World, brief: &Brief, plan: Plan, k: u64) -> f32 {
    let party = brief.party;
    let start = &brief.view;
    let seed = brief.seed ^ k << 32;
    let mut world = map.scratch(map.regiments.capacity() as u32, seed);
    for r in &start.regiments {
        world.spawn_standing(&standing(r), Some(r.handle));
    }
    let mut minds = Minds::sharing(brief.terrain.clone());
    let mut captain = brief.captain.clone();
    captain.follow(plan);
    minds.add(Mind::seat(
        MindConfig {
            party,
            difficulty: Difficulty::Warlord,
            personality: 0,
            seed,
        },
        brief.knobs,
        brief.persona,
        captain,
    ));
    for r in &start.regiments {
        if minds.get(r.owner).is_some() {
            continue;
        }
        // The commander the enemy seems to be, as it stands; anyone else a
        // sound one starting afresh.
        let guess = brief.foes.iter().find(|g| g.party == r.owner);
        minds.add(Mind::seat(
            MindConfig {
                party: r.owner,
                difficulty: Difficulty::General,
                personality: 0,
                seed: seed ^ u64::from(r.owner),
            },
            Knobs {
                slip_percent: 0,
                ..Difficulty::General.knobs()
            },
            guess.map_or(middling(), |g| g.persona),
            guess.map_or_else(Captain::new, |g| g.captain.clone()),
        ));
    }
    let mut out: Vec<PlayerCommand> = Vec::new();
    let end = world.tick_count() + HORIZON;
    let mut held: Option<(PlayerId, u32)> = None;
    while world.tick_count() < end {
        out.clear();
        minds.tick(&world, &mut out);
        world.tick(&out);
        if world.tick_count().is_multiple_of(TICKS as u32) {
            let mut view = Perception::new();
            view.capture(&world);
            held = match (crate::holding_the_field(&view), held) {
                (Some(p), Some((q, since))) if p == q => Some((q, since)),
                (Some(p), _) => Some((p, world.tick_count())),
                (None, _) => None,
            };
            if held.is_some_and(|(_, since)| world.tick_count() >= since + DECIDED) {
                break;
            }
        }
    }
    let mut end = Perception::new();
    end.capture(&world);
    score(start, &end, party)
}

/// How a regiment the mind sees stands, to set out in a scratch world.
fn standing(r: &Seen) -> Standing {
    Standing {
        owner: r.owner,
        unit: r.unit,
        position: to_fx_vec(r.pos),
        facing: vec_angle(r.facing),
        files: r.files,
        formation: r.formation,
        models: r.models,
        morale_points: r.morale_points,
        morale: r.morale,
        hidden: r.hidden,
    }
}

/// How the rehearsal went for `party`, from `start` to `end`: the share of
/// its strength it kept less the share the enemy kept, and the estimator's
/// view of the fight still to come.
fn score(start: &Perception, end: &Perception, party: PlayerId) -> f32 {
    let ours = |p: PlayerId| !start.hostile(party, p);
    let worth = |v: &Perception, mine: bool| -> f32 {
        v.regiments
            .iter()
            .filter(|r| ours(r.owner) == mine)
            .map(|r| {
                r.strength()
                    * match r.morale {
                        MoraleState::Steady => 1.0,
                        MoraleState::Wavering => WAVERING,
                        MoraleState::Routing => ROUTING,
                        MoraleState::Shattered => 0.0,
                    }
            })
            .sum()
    };
    let kept = |mine: bool| worth(end, mine) / worth(start, mine).max(1e-3);
    let material = kept(true) - kept(false);
    let standing = |mine: bool| {
        end.regiments
            .iter()
            .filter(move |r| ours(r.owner) == mine && !r.broken())
    };
    let (a, b) = (
        Force::of(end, standing(true)),
        Force::of(end, standing(false)),
    );
    let gap = standing(true)
        .flat_map(|r| standing(false).map(move |f| captain::gap_between(r, f)))
        .fold(f32::MAX, f32::min)
        .clamp(0.0, 200.0);
    let closing = (a.speed + b.speed).max(0.5) * 0.5;
    let beyond = estimator::estimate(&a, &b, Approach { gap, closing }).margin;
    material + LOOK_BEYOND * beyond
}

/// The temper a rehearsal gives a commander it knows nothing about: every
/// trait and every doctrine in the middle.
fn middling() -> &'static Personality {
    static MIDDLING: OnceLock<Personality> = OnceLock::new();
    MIDDLING.get_or_init(|| Personality {
        name: "a sound commander".into(),
        faction: Faction::EmberClans,
        summary: String::new(),
        aggression: 50,
        caution: 50,
        patience: 50,
        cunning: 50,
        opportunism: 50,
        greed: 50,
        honour: 50,
        vengefulness: 50,
        doctrines: DoctrineLikes {
            hold_the_line: 50,
            hammer_and_anvil: 50,
            refused_flank: 50,
            skirmish_screen: 50,
            ambush: 50,
        },
        army: crate::army::Preset::Balanced,
    })
}
