//! The Conquest layers of a mind (docs/18-ai.md, "Inside a mind"): the
//! [`Steward`](steward::Steward) runs the economy, production and building;
//! the [`Marshal`](marshal::Marshal) runs the army and hands each battle to
//! a [`Captain`](crate::Captain). The [`spymaster`] says what it believes
//! of each enemy's army, its level's [`Strategy`] how it plays the long
//! game, and its [`opening`] how it spends its first minutes.
//!
//! A Conquest mind plays by the same rules as any other: it reads what its
//! side sees (fog of war), and acts only through the commands a player's
//! clicks would send, with its level's reaction time and attention.

pub mod kit;
pub mod marshal;
pub mod opening;
pub mod placing;
pub mod raider;
pub mod scout;
pub mod sovereign;
pub mod spymaster;
pub mod steward;
pub mod strategy;

use std::sync::Arc;

use gc_sim::{PlayerCommand, PlayerId, RegimentHandle, World};
use glam::Vec2;

use crate::difficulty::Knobs;
use crate::orders::{Act, Hands, Want};
use crate::perception::Perception;
use crate::terrain::Terrain;
use kit::Kit;
use marshal::Marshal;
use opening::Opening;
use sovereign::Sovereign;
use steward::Steward;
use strategy::Strategy;

/// The share of its orders a minute (percent) that goes on the economy;
/// the army has the rest.
const ECONOMY_SHARE: u32 = 40;

/// What the Conquest layers read at each look.
pub struct Sense<'a> {
    pub world: &'a World,
    /// What its side sees.
    pub view: &'a Perception,
    pub kit: &'a Kit,
    pub party: PlayerId,
    pub now: u32,
    /// Its level's reaction time, in ticks.
    pub reaction: u32,
    pub terrain: &'a Terrain,
    /// The tick from which it expands once its seeps are full.
    pub expand_after: u32,
    /// How its level plays the long game, with its opening's early rules
    /// while that lasts.
    pub plan: Strategy,
    /// The enemy its Sovereign goes for first.
    pub focus: Option<u8>,
    /// When its Sovereign's next timing push sets out.
    pub push: Option<u32>,
    /// Its personality's caution, 0 to 1: how early and how much it
    /// fortifies.
    pub caution: f32,
}

impl Sense<'_> {
    /// Where its Sovereign's timing push stands now.
    pub fn push_phase(&self) -> sovereign::PushPhase {
        sovereign::push_phase(self.push, self.now)
    }

    /// An order for `regiment` to walk to `to` (or the nearest ground it
    /// can stand on), for a party out on its own.
    fn walk(&self, regiment: RegimentHandle, to: Vec2, priority: u8, group: u32) -> Want {
        let to = self
            .terrain
            .nearest_walkable(to, 6.0)
            .unwrap_or_else(|| self.terrain.clamp(to, 2.0));
        Want {
            regiment,
            act: Act::Move {
                to,
                facing: None,
                pace: 0.0,
            },
            priority,
            group,
        }
    }
}

/// A start further than this from home, tiles, is someone else's.
const OTHER_START: f32 = 30.0;

/// The nearest start to `home` that is someone else's: where the nearest
/// enemy comes from (the rally point and the works face it).
fn nearest_other_start(s: &Sense, home: Vec2) -> Option<Vec2> {
    other_starts(s, home).min_by(|a, b| a.distance(home).total_cmp(&b.distance(home)))
}

/// The starts on the map that are someone else's.
fn other_starts<'a>(s: &'a Sense, home: Vec2) -> impl Iterator<Item = Vec2> + 'a {
    s.kit
        .starts
        .iter()
        .copied()
        .filter(move |p| p.distance(home) > OTHER_START)
}

/// A Conquest mind's economy and army.
pub struct Realm {
    kit: Arc<Kit>,
    pub steward: Steward,
    pub marshal: Marshal,
    pub sovereign: Sovereign,
    clicks: Hands,
    /// The tick from which it expands once its seeps are full.
    expand_after: u32,
    plan: Strategy,
    opening: &'static Opening,
    /// What it has made of its enemies' early builds.
    reads: Vec<spymaster::Scouted>,
    next_read: u32,
}

/// Ticks between its looks at its enemies' builds.
const READ_TICKS: u32 = 2 * gc_sim::TICKS_PER_SECOND;

impl Realm {
    pub fn new(
        kit: Arc<Kit>,
        party: PlayerId,
        plan: Strategy,
        opening: &'static Opening,
        knobs: &Knobs,
        expand_after: u32,
    ) -> Realm {
        // One budget of orders a minute, split between the army and the
        // economy as a player splits their attention.
        let opm = knobs.orders_per_minute;
        let economy = opm * ECONOMY_SHARE / 100;
        Realm {
            kit,
            steward: Steward::new(),
            marshal: Marshal::new(party, knobs.reaction_ticks, opm - economy),
            sovereign: Sovereign::default(),
            clicks: Hands::new(party, knobs.reaction_ticks, economy, 3.0),
            expand_after,
            plan,
            opening,
            reads: Vec::new(),
            next_read: 0,
        }
    }

    /// The enemy build it answers at tick `now`, while it does.
    fn answering(&self, now: u32) -> Option<spymaster::Scouted> {
        if !self.plan.adapts || now >= strategy::ANSWER_UNTIL {
            return None;
        }
        spymaster::to_answer(&self.reads)
    }

    /// The opening it plays, while it lasts at tick `now`.
    fn opening_at(&self, now: u32) -> Option<&'static Opening> {
        (now < self.opening.ends()).then_some(self.opening)
    }

    /// Economy orders sent so far.
    pub fn clicks(&self) -> &Hands {
        &self.clicks
    }

    /// Look and append the orders due now to `out`.
    pub fn tick(
        &mut self,
        world: &World,
        mut captain: crate::captain::Ctx,
        out: &mut Vec<PlayerCommand>,
    ) {
        let kit = Arc::clone(&self.kit);
        let mut sense = Sense {
            world,
            view: captain.view,
            kit: &kit,
            party: captain.party,
            now: captain.view.tick,
            reaction: captain.knobs.reaction_ticks,
            terrain: captain.terrain,
            expand_after: self.expand_after,
            plan: self.plan.opening(self.opening_at(captain.view.tick)),
            focus: None,
            push: None,
            caution: captain.persona.caution(),
        };
        // A level that adapts reads its enemies' early builds and answers
        // the most pressing.
        if self.plan.adapts && sense.now >= self.next_read {
            self.next_read = sense.now + READ_TICKS;
            for r in spymaster::read_builds(&sense, &mut self.reads) {
                captain.journal.note(
                    sense.now,
                    format!(
                        "Scouted {}: {}; answering with {}",
                        enemy_name(&self.marshal.belief, r.owner),
                        r.read.name(),
                        r.read.answer_name()
                    ),
                );
            }
        }
        // Answered from the tick it was read, so an answer's own early
        // expansion is due from then, not from every tick anew.
        if let Some(a) = self.answering(sense.now) {
            sense.plan = sense.plan.answer(Some(a.read), a.at);
        }
        let belief = &self.marshal.belief;
        for note in self
            .sovereign
            .think(&sense, captain.persona, |o| enemy_name(belief, o))
        {
            captain.journal.note(sense.now, note);
        }
        sense.focus = self.sovereign.focus.map(|(o, _)| o);
        sense.push = self.sovereign.push;
        self.clicks.refresh(sense.now);
        self.steward
            .think(&sense, &mut self.clicks, self.marshal.rally);
        self.clicks.release(sense.now, out);
        let home = self.steward.home;
        self.marshal.tick(&mut captain, &sense, home, out);
    }

    /// One line on what it is doing.
    pub fn summary(&self) -> String {
        let l = &self.steward.ledger;
        let fight = match self.marshal.captain() {
            Some(c) => format!(
                ", in battle ({}, {}{})",
                c.doctrine.name(),
                c.posture.name(),
                if c.finishing { ", finishing" } else { "" }
            ),
            None => String::new(),
        };
        let at = match self.marshal.goal {
            marshal::Goal::Attack(p) | marshal::Goal::Defend(p) | marshal::Goal::Heart(p) => {
                format!(" at ({:.0}, {:.0})", p.x, p.y)
            }
            marshal::Goal::Gather => String::new(),
        };
        format!(
            "tier {}, {}/{} workers, {} producing, army {}{at}{fight}",
            l.tier,
            l.workers,
            l.want_workers,
            l.producers,
            self.marshal.goal.name()
        )
    }

    /// The panel lines for the AI lens, from what it has already decided
    /// (nothing here reads the field or changes a decision): its economy,
    /// what it believes of each enemy's army, and what the army waits for.
    pub fn lens_lines(&self, now: u32) -> Vec<String> {
        let line = |label: &str, text: String| format!("  {label:<7} {text}");
        let saving = match self.steward.ledger.saving {
            (0, 0) => "nothing".to_string(),
            (g, 0) => format!("{g} Grub"),
            (0, m) => format!("{m} Marrow"),
            (g, m) => format!("{g} Grub, {m} Marrow"),
        };
        let producers = self.plan.producers_per_tier;
        let arming = if self.steward.arming() {
            "short of an army"
        } else {
            "army is enough for now"
        };
        let opening = if now < self.opening.ends() {
            format!(
                "opens with {} until {}",
                self.opening.name,
                crate::lens::clock(self.opening.ends())
            )
        } else {
            format!("opened with {}", self.opening.name)
        };
        let answer = self.answering(now).map_or(String::new(), |a| {
            format!(
                "; answering {}'s {}",
                enemy_name(&self.marshal.belief, a.owner),
                a.read.name()
            )
        });
        let mut lines = vec![line(
            "economy",
            format!(
                "{opening}{answer}; keeps {}% workers, {producers} producer{} a tier   saving: {saving}   arming: {arming}",
                self.plan.workers_percent,
                if producers == 1 { "" } else { "s" }
            ),
        )];
        let beliefs = &self.marshal.belief;
        let known: Vec<String> = beliefs
            .iter()
            .map(|b| {
                let odds = self
                    .marshal
                    .odds
                    .iter()
                    .find(|(o, ..)| *o == b.owner)
                    .map_or(String::new(), |&(_, o, at)| {
                        format!(", odds {o:+.2} as of {}", crate::lens::clock(at))
                    });
                format!("{} {} supply{odds}", enemy_name(beliefs, b.owner), b.supply)
            })
            .collect();
        lines.push(line(
            "belief",
            if known.is_empty() {
                "no enemy army seen yet".to_string()
            } else {
                known.join("; ")
            },
        ));
        if let Some(note) = self.marshal.plan_note(now, self.sovereign.push) {
            lines.push(line("plan", note));
        }
        let mut aims = Vec::new();
        if let Some((o, why)) = self.sovereign.focus {
            let name = enemy_name(&self.marshal.belief, o);
            aims.push(match why {
                sovereign::Why::Leader => format!("going for {name} first: they lead"),
                sovereign::Why::Grudge => format!("going for {name} first: they attacked us"),
            });
        }
        if let Some(t) = self.sovereign.push {
            aims.push(format!("timing push at {}", crate::lens::clock(t)));
        }
        if !aims.is_empty() {
            lines.push(line("aims", aims.join("; ")));
        }
        lines.push(line("scout", self.marshal.scout.note(now)));
        if self.plan.raids {
            let note = self
                .marshal
                .raider
                .note(now, |o| enemy_name(&self.marshal.belief, o));
            lines.push(line("raid", note));
        }
        lines
    }
}

/// An enemy by its faction if it knows it, with its player's number when
/// two enemies share a faction, else by its seat.
fn enemy_name(beliefs: &[spymaster::Belief], owner: u8) -> String {
    let faction = beliefs
        .iter()
        .find(|b| b.owner == owner)
        .and_then(|b| b.faction);
    match faction {
        Some(f) if beliefs.iter().filter(|b| b.faction == faction).count() == 1 => {
            f.name().to_string()
        }
        Some(f) => format!("{} (player {})", f.name(), owner + 1),
        None => format!("player {}", owner + 1),
    }
}
