//! Grubcraft's computer players (docs/18-ai.md).
//!
//! **Rules in the sim, minds outside it.** A mind is a player: it reads the
//! simulation's public views, the same things a person sees on screen, and
//! acts only by sending the ordinary commands a person's clicks would. Minds
//! run on the host; in multiplayer their commands travel in the turn bundles
//! like anyone's, so nothing here can make two machines' games drift apart,
//! and the AI is free to use floating point and change between versions.
//!
//! A mind thinks in layers (docs/18-ai.md, "How a mind is put together").
//! This crate has the battle layers so far:
//! - [`perception`]: what can be seen, copied from the world once per tick.
//! - [`estimator`]: a quick model of who wins a fight, from the sim's rules.
//! - [`ground`]: what the ground offers a line: cover for a flank, high
//!   ground, water between the armies.
//! - [`captain`]: battle tactics: the plan, the roles, every regiment's job.
//! - [`orders`]: turns decisions into commands with a human's reaction time
//!   and attention.
//! - [`lens`]: what a mind is thinking, for the game to draw (the AI lens).
//!
//! Each mind has a [`Difficulty`] (how well it thinks) and a
//! [`Personality`](personality::Personality) (what it likes to do), and
//! keeps a [`Journal`](journal::Journal) of what it decided and why.

pub mod army;
pub mod captain;
pub mod difficulty;
pub mod estimator;
pub mod ground;
pub mod journal;
pub mod lens;
pub mod orders;
pub mod perception;
pub mod personality;
pub mod scripted;
pub mod terrain;

use gc_fixed::Pcg32;
use gc_sim::{PlayerCommand, PlayerId, World};

pub use crate::captain::{Captain, Doctrine, Posture, Role};
pub use crate::difficulty::{Difficulty, Knobs};
pub use crate::journal::Journal;
pub use crate::orders::{Orders, Want};
pub use crate::perception::Perception;
pub use crate::personality::{Faction, Personality};
pub use crate::terrain::Terrain;

/// How to set up a mind.
#[derive(Clone, Copy, Debug)]
pub struct MindConfig {
    /// The party it plays.
    pub party: PlayerId,
    pub difficulty: Difficulty,
    /// Index into [`personality::all`].
    pub personality: usize,
    /// Seeds its small random choices, so a replayed match plays the same.
    pub seed: u64,
}

/// One computer player.
pub struct Mind {
    party: PlayerId,
    difficulty: Difficulty,
    knobs: Knobs,
    persona: &'static Personality,
    rng: Pcg32,
    captain: Captain,
    orders: Orders,
    journal: Journal,
    next_think: u32,
    next_slip: u32,
    wants: Vec<Want>,
}

impl Mind {
    pub fn new(config: MindConfig) -> Mind {
        Mind::with_knobs(config, config.difficulty.knobs())
    }

    /// A mind with hand-set knobs, for tuning and tests.
    pub fn with_knobs(config: MindConfig, knobs: Knobs) -> Mind {
        let all = personality::all();
        let persona = &all[config.personality % all.len()];
        let mut journal = Journal::new();
        journal.note(
            0,
            format!(
                "{} ({}) takes the field",
                persona.name,
                config.difficulty.name()
            ),
        );
        Mind {
            party: config.party,
            difficulty: config.difficulty,
            knobs,
            persona,
            rng: Pcg32::new(config.seed, 0xA1 + config.party as u64),
            captain: Captain::new(),
            orders: Orders::new(config.party, knobs.reaction_ticks, knobs.orders_per_minute),
            journal,
            // Minds of a match look on different ticks, to spread the work.
            next_think: config.party as u32 % knobs.think_ticks.max(1),
            next_slip: 0,
            wants: Vec::new(),
        }
    }

    pub fn party(&self) -> PlayerId {
        self.party
    }

    pub fn difficulty(&self) -> Difficulty {
        self.difficulty
    }

    /// How well it thinks: its level's settings.
    pub fn knobs(&self) -> &Knobs {
        &self.knobs
    }

    pub fn personality(&self) -> &'static Personality {
        self.persona
    }

    pub fn captain(&self) -> &Captain {
        &self.captain
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Orders sent so far.
    pub fn orders_sent(&self) -> u32 {
        self.orders.count
    }

    /// The order layer, for its counters.
    pub fn orders(&self) -> &Orders {
        &self.orders
    }

    /// Look (when it's time to) and append the orders due now to `out`.
    pub fn tick(&mut self, view: &Perception, terrain: &Terrain, out: &mut Vec<PlayerCommand>) {
        if view.tick >= self.next_think {
            self.next_think = view.tick + self.knobs.think_ticks.max(1);
            self.wants.clear();
            let mut ctx = captain::Ctx {
                view,
                terrain,
                party: self.party,
                knobs: &self.knobs,
                persona: self.persona,
                rng: &mut self.rng,
                journal: &mut self.journal,
                next_slip: &mut self.next_slip,
            };
            self.captain.think(&mut ctx, &mut self.wants);
            self.orders.decide(view, &mut self.wants);
        }
        self.orders.release(view.tick, out);
    }

    /// One line on what it is doing, for the sandbox panel.
    pub fn summary(&self) -> String {
        let c = &self.captain;
        let expects = match c.estimate {
            Some(e) => format!(", expects {}", captain::describe_margin(e.margin)),
            None => String::new(),
        };
        let doing = if c.finishing {
            "finishing them off"
        } else {
            c.posture.name()
        };
        format!(
            "{} ({}): {}, {doing}{expects}",
            self.persona.name,
            self.difficulty.name(),
            c.doctrine.name(),
        )
    }
}

/// The minds of a match, sharing one look at the world per tick.
pub struct Minds {
    view: Perception,
    terrain: Terrain,
    minds: Vec<Mind>,
}

impl Minds {
    pub fn new(terrain: Terrain) -> Minds {
        Minds {
            view: Perception::new(),
            terrain,
            minds: Vec::new(),
        }
    }

    /// Put `mind` in charge of its party (replacing any mind already there).
    pub fn add(&mut self, mind: Mind) {
        self.remove(mind.party);
        self.minds.push(mind);
        self.minds.sort_by_key(|m| m.party);
    }

    pub fn remove(&mut self, party: PlayerId) -> Option<Mind> {
        let i = self.minds.iter().position(|m| m.party == party)?;
        Some(self.minds.remove(i))
    }

    pub fn get(&self, party: PlayerId) -> Option<&Mind> {
        self.minds.iter().find(|m| m.party == party)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Mind> {
        self.minds.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.minds.is_empty()
    }

    /// The ground the minds plan on.
    pub fn terrain(&self) -> &Terrain {
        &self.terrain
    }

    /// The last look at the world.
    pub fn view(&self) -> &Perception {
        &self.view
    }

    /// Look at `world` and append every mind's orders for its next tick
    /// to `out`.
    pub fn tick(&mut self, world: &World, out: &mut Vec<PlayerCommand>) {
        if self.minds.is_empty() {
            return;
        }
        self.view.capture(world);
        for m in &mut self.minds {
            m.tick(&self.view, &self.terrain, out);
        }
    }
}

/// The party left holding the field: some party with regiments still
/// fighting, when every other party with regiments still fighting is its
/// ally. `None` while enemies still face each other, or nobody stands.
pub fn holding_the_field(view: &Perception) -> Option<PlayerId> {
    let mut standing = view
        .regiments
        .iter()
        .filter(|r| !r.broken() && r.models > 0);
    let first = standing.next()?.owner;
    standing
        .all(|r| !view.hostile(first, r.owner))
        .then_some(first)
}
