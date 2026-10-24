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
//! - [`rehearsal`]: the Warlord tries plans out in a scratch copy of the
//!   battle under the real rules before committing to one.
//! - [`lens`]: what a mind is thinking, for the game to draw (the AI lens).
//!
//! Each mind has a [`Difficulty`] (how well it thinks) and a
//! [`Personality`](personality::Personality) (what it likes to do), and
//! keeps a [`Journal`](journal::Journal) of what it decided and why.

pub mod army;
pub mod captain;
pub mod conquest;
pub mod difficulty;
pub mod estimator;
pub mod ground;
pub mod journal;
pub mod lens;
pub mod orders;
pub mod perception;
pub mod personality;
pub mod reading;
pub mod rehearsal;
pub mod scripted;
pub mod terrain;

use std::sync::Arc;

use gc_fixed::Pcg32;
use gc_sim::{Command, PlayerCommand, PlayerId, World};

pub use crate::captain::{Captain, Doctrine, Plan, Posture, Role, Stance};
pub use crate::difficulty::{Difficulty, Knobs};
pub use crate::journal::Journal;
pub use crate::orders::{Orders, Want};
use crate::perception::Class;
pub use crate::perception::Perception;
pub use crate::personality::{Faction, Personality};
pub use crate::rehearsal::Verdict;
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
    /// How it was set up.
    config: MindConfig,
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
    /// The last torches order given to each regiment, by pool index, so
    /// one still on its way isn't given again.
    torches_given: Vec<Option<(gc_sim::RegimentHandle, bool)>>,
    /// What its side sees, with the enemies it remembers (on a map with
    /// cover).
    sight: Perception,
    /// Rehearsals under way, when the next round is due, and what the last
    /// one decided.
    round: Option<rehearsal::Round>,
    next_round: u32,
    verdict: Option<Verdict>,
    /// What it makes of each enemy (Warlord only).
    readings: Vec<reading::Reading>,
    /// Playing in real time (see [`Minds::live`]).
    live: bool,
    /// Its economy and army, in Conquest.
    realm: Option<conquest::Realm>,
}

impl Mind {
    pub fn new(config: MindConfig) -> Mind {
        Mind::with_knobs(config, config.difficulty.knobs())
    }

    /// A mind with hand-set knobs, for tuning and tests.
    pub fn with_knobs(config: MindConfig, knobs: Knobs) -> Mind {
        let all = personality::all();
        Mind::seat(
            config,
            knobs,
            &all[config.personality % all.len()],
            Captain::new(),
        )
    }

    /// A mind with a given temper, taking over `captain` (a rehearsal's
    /// players). `config.personality` is ignored.
    pub(crate) fn seat(
        config: MindConfig,
        knobs: Knobs,
        persona: &'static Personality,
        captain: Captain,
    ) -> Mind {
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
            config,
            party: config.party,
            difficulty: config.difficulty,
            knobs,
            persona,
            rng: Pcg32::new(config.seed, 0xA1 + u64::from(config.party)),
            captain,
            orders: Orders::new(config.party, knobs.reaction_ticks, knobs.orders_per_minute),
            journal,
            // Minds of a match look on different ticks, to spread the work.
            next_think: u32::from(config.party) % knobs.think_ticks.max(1),
            next_slip: 0,
            wants: Vec::new(),
            torches_given: Vec::new(),
            sight: Perception::new(),
            round: None,
            // Rounds start on a beat of their own too, so several Warlords
            // (up to 6 players) don't all set out their rehearsals in one tick.
            next_round: u32::from(config.party) % rehearsal::BEATS
                * (rehearsal::EVERY / rehearsal::BEATS),
            verdict: None,
            readings: Vec::new(),
            live: false,
            realm: None,
        }
    }

    /// A Conquest player of `kit`'s faction against `rivals` (the other
    /// players' factions): it runs its economy and army as well as its
    /// battles. It picks its own opening unless given one (for tests and
    /// tuning).
    pub fn conquest(
        config: MindConfig,
        kit: Arc<conquest::kit::Kit>,
        rivals: &[personality::Faction],
        opening: Option<&'static conquest::opening::Opening>,
    ) -> Mind {
        let mut mind = Mind::new(config);
        let plan = conquest::strategy::Strategy::of(config.difficulty);
        let opening = opening.unwrap_or_else(|| {
            conquest::opening::pick(mind.persona, config.difficulty, rivals, config.seed)
        });
        mind.journal.note(
            0,
            format!("Takes up the Conquest: opens with {}", opening.name),
        );
        mind.realm = Some(conquest::Realm::new(
            kit,
            config.party,
            plan,
            opening,
            &mind.knobs,
            plan.expand_after(mind.persona.greed),
        ));
        mind
    }

    /// Its economy and army, in Conquest.
    pub fn realm(&self) -> Option<&conquest::Realm> {
        self.realm.as_ref()
    }

    pub fn party(&self) -> PlayerId {
        self.party
    }

    /// How it was set up (to seat the same commander with other knobs).
    pub fn config(&self) -> MindConfig {
        self.config
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
        self.orders.count()
            + self
                .realm
                .as_ref()
                .map_or(0, |r| r.marshal.orders().count() + r.clicks().count)
    }

    /// The order layer, for its counters.
    pub fn orders(&self) -> &Orders {
        &self.orders
    }

    /// Fight with `plan` from now on (until a rehearsal picks another).
    pub fn follow(&mut self, plan: Plan) {
        self.captain.follow(plan);
    }

    /// What its last round of rehearsals decided (Warlord only).
    pub fn verdict(&self) -> Option<&Verdict> {
        self.verdict.as_ref()
    }

    /// Look (when it's time to) and append the orders due now to `out`.
    /// `world` is what `view` was captured from; a Warlord borrows its map
    /// to rehearse on.
    pub fn tick(
        &mut self,
        view: &Perception,
        terrain: &Arc<Terrain>,
        world: &World,
        out: &mut Vec<PlayerCommand>,
    ) {
        if let Some(realm) = &mut self.realm {
            let ctx = captain::Ctx {
                view,
                terrain,
                party: self.party,
                knobs: &self.knobs,
                persona: self.persona,
                rng: &mut self.rng,
                journal: &mut self.journal,
                next_slip: &mut self.next_slip,
            };
            realm.tick(world, ctx, out);
            self.torches(view);
            self.orders.release(view.tick, out);
            return;
        }
        if self.knobs.rehearse {
            self.rehearse(view, terrain, world);
        }
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
        self.torches(view);
        self.orders.release(view.tick, out);
    }

    /// Once a second, in the dark, after its other orders: light the
    /// torches of its melee regiments that are fighting (they are seen
    /// anyway, and the light shows what is near), and put out every
    /// other's, so the rest march unseen (docs/32-changing-land.md, stage
    /// 3b).
    fn torches(&mut self, view: &Perception) {
        if !view.air.dark || !view.tick.is_multiple_of(gc_sim::TICKS_PER_SECOND) {
            return;
        }
        for r in view.regiments.iter().filter(|r| r.owner == self.party) {
            let lit = r.engaged && view.unit(r).class != Class::Ranged;
            let slot = r.handle.index as usize;
            if self.torches_given.len() <= slot {
                self.torches_given.resize(slot + 1, None);
            }
            if r.torches == lit || self.torches_given[slot] == Some((r.handle, lit)) {
                continue;
            }
            // Each costs an order, as a person's would.
            if !self.orders.give(
                view.tick,
                Command::Torches {
                    regiment: r.handle,
                    lit,
                },
            ) {
                break;
            }
            self.torches_given[slot] = Some((r.handle, lit));
        }
    }

    /// Take the answer of the rehearsals under way once it is due, and
    /// start the next round when it is time: every [`rehearsal::EVERY`]
    /// while the battle is still to be decided.
    fn rehearse(&mut self, view: &Perception, terrain: &Arc<Terrain>, world: &World) {
        let answered = |r: &rehearsal::Round| view.tick >= r.due && (!self.live || r.ready());
        if self.round.as_ref().is_some_and(answered) {
            let round = self.round.take().unwrap();
            let v = round.finish(self.captain.plan);
            if self.captain.plan != Some(v.plan) {
                let runner_up = v
                    .plans
                    .iter()
                    .find(|(p, _)| *p != v.plan)
                    .map_or(String::new(), |(p, s)| {
                        format!("; next best {} ({s:+.2})", p.describe())
                    });
                self.journal.note(
                    view.tick,
                    format!(
                        "Rehearsed {} plans: {} comes out best ({:+.2}){runner_up}",
                        v.plans.len(),
                        v.plan.describe(),
                        v.score
                    ),
                );
            }
            self.captain.follow(v.plan);
            self.verdict = Some(v);
        }
        let party = self.party;
        self.read(view, terrain, world);
        let contested = view
            .regiments
            .iter()
            .any(|r| view.hostile(party, r.owner) && !r.broken());
        if self.round.is_none()
            && view.tick >= self.next_round
            && self.captain.started()
            && !self.captain.finishing
            && contested
        {
            self.next_round = view.tick + rehearsal::EVERY;
            let mut foes = Vec::new();
            for x in &mut self.readings {
                let Some(g) = x.guess() else { continue };
                if x.news(&g) {
                    self.journal.note(
                        view.tick,
                        format!(
                            "Reads the enemy as {} ({:.0}% fit)",
                            g.persona.name,
                            g.fit * 100.0
                        ),
                    );
                }
                foes.push(g);
            }
            self.round = rehearsal::Round::start(world, view, terrain, self, foes);
        }
    }

    /// Watch every enemy for which commander it seems to be.
    fn read(&mut self, view: &Perception, terrain: &Arc<Terrain>, world: &World) {
        let party = self.party;
        for r in &view.regiments {
            if view.hostile(party, r.owner) && !self.readings.iter().any(|x| x.party() == r.owner) {
                let seed = u64::from(r.owner) << 8 | u64::from(party);
                self.readings
                    .push(reading::Reading::new(r.owner, Difficulty::General, seed));
            }
        }
        for x in &mut self.readings {
            x.tick(view, terrain, world);
        }
    }

    /// One line on what it is doing, for the sandbox panel.
    pub fn summary(&self) -> String {
        if let Some(realm) = &self.realm {
            return format!(
                "{} ({}): {}",
                self.persona.name,
                self.difficulty.name(),
                realm.summary()
            );
        }
        let c = &self.captain;
        let expects = match c.estimate {
            Some(e) => format!(", expects {}", captain::describe_margin(e.margin)),
            None => String::new(),
        };
        let doing = if c.finishing {
            "finishing them off"
        } else if c.fetching {
            "going to fetch them"
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

/// The minds of a match, sharing one look at the world per tick (on a map
/// with cover, each takes its own look at what its side sees).
pub struct Minds {
    view: Perception,
    terrain: Arc<Terrain>,
    minds: Vec<Mind>,
    live: bool,
}

impl Minds {
    pub fn new(terrain: Terrain) -> Minds {
        Minds::sharing(Arc::new(terrain))
    }

    /// Minds planning on ground another set of minds plans on too (a
    /// rehearsal's).
    pub(crate) fn sharing(terrain: Arc<Terrain>) -> Minds {
        Minds {
            view: Perception::new(),
            terrain,
            minds: Vec::new(),
            live: false,
        }
    }

    /// Minds playing in real time: a Warlord takes its rehearsals' answer
    /// as soon as it is ready after it is due, rather than make the game
    /// wait on a machine too slow to finish in time. Without this a battle
    /// plays out the same way on any machine, which tests rely on.
    pub fn live(mut self) -> Minds {
        self.live = true;
        self
    }

    /// Put `mind` in charge of its party (replacing any mind already there).
    pub fn add(&mut self, mut mind: Mind) {
        mind.live = self.live;
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

    /// The last look at the world, all of it (minds on a map with cover
    /// think on what their side sees).
    pub fn view(&self) -> &Perception {
        &self.view
    }

    /// Look at `world` and append every mind's orders for its next tick
    /// to `out`.
    pub fn tick(&mut self, world: &World, out: &mut Vec<PlayerCommand>) {
        // The computer players keep the standard deployment and are ready
        // from the start; they think once the battle begins.
        if self.minds.is_empty() || world.deploying() {
            return;
        }
        // Ground cleared since the last look is seen at once.
        if self.terrain.stale(world.terrain()) {
            Arc::make_mut(&mut self.terrain).follow(world.terrain());
        }
        self.view.capture(world);
        // Regiments hidden in cover or fog of war are hidden from the
        // computer players too (docs/20-ambushes.md).
        let fair = world.terrain().has_cover() || world.fog();
        for m in &mut self.minds {
            if fair {
                let mut sight = std::mem::take(&mut m.sight);
                sight.capture_for(world, m.party);
                m.tick(&sight, &self.terrain, world, out);
                m.sight = sight;
            } else {
                m.tick(&self.view, &self.terrain, world, out);
            }
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
