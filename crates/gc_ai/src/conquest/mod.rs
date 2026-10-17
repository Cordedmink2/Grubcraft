//! The Conquest layers of a mind (docs/18-ai.md, "Inside a mind"): the
//! [`Steward`](steward::Steward) runs the economy, production and building;
//! the [`Marshal`](marshal::Marshal) runs the army and hands each battle to
//! a [`Captain`](crate::Captain). The Sovereign's choice of intents and the
//! Spymaster's beliefs come in the next stage; for now the Marshal's few
//! rules (gather, attack when strong enough, defend home, fall back when
//! beaten) stand in for them.
//!
//! A Conquest mind plays by the same rules as any other: it reads what its
//! side sees (fog of war), and acts only through the commands a player's
//! clicks would send, with its level's reaction time and attention.

pub mod kit;
pub mod marshal;
pub mod placing;
pub mod steward;

use std::sync::Arc;

use gc_sim::{PlayerCommand, PlayerId, World};

use crate::difficulty::Knobs;
use crate::orders::Hands;
use crate::perception::Perception;
use crate::terrain::Terrain;
use kit::Kit;
use marshal::Marshal;
use steward::Steward;

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
}

/// A Conquest mind's economy and army.
pub struct Realm {
    kit: Arc<Kit>,
    pub steward: Steward,
    pub marshal: Marshal,
    clicks: Hands,
    /// The tick from which it expands once its seeps are full.
    expand_after: u32,
}

impl Realm {
    pub fn new(kit: Arc<Kit>, party: PlayerId, knobs: &Knobs, expand_after: u32) -> Realm {
        // One budget of orders a minute, split between the army and the
        // economy as a player splits their attention.
        let opm = knobs.orders_per_minute;
        let economy = opm * ECONOMY_SHARE / 100;
        Realm {
            kit,
            steward: Steward::new(),
            marshal: Marshal::new(party, knobs.reaction_ticks, opm - economy),
            clicks: Hands::new(party, knobs.reaction_ticks, economy, 3.0),
            expand_after,
        }
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
        let sense = Sense {
            world,
            view: captain.view,
            kit: &kit,
            party: captain.party,
            now: captain.view.tick,
            reaction: captain.knobs.reaction_ticks,
            terrain: captain.terrain,
            expand_after: self.expand_after,
        };
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
}
