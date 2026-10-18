//! Concord caravans (docs/03-factions.md, "Economy: trade routes and
//! tithes"): a unit that **trades** ([`ConquestUnit::trades`]), sent to a
//! trade post with [`crate::Command::Trade`], shuttles between its owner's
//! nearest main building and the post until given another order:
//!
//! 1. It walks to the post and takes on cargo there ([`LOAD_TICKS`], at
//!    the match's tempo).
//! 2. The cargo is worth the unit's base pay plus its pay for each whole
//!    tile between the post and the owner's main building nearest it, in a
//!    straight line. At a **Waystation** (the owner's own building with
//!    the [`TRADE_POST`] flag) it is [`WAYSTATION_PERCENT`] of that.
//! 3. It carries the cargo to the owner's nearest main building and banks
//!    it as Grub, cut by upkeep like any Grub brought in, and goes back.
//!
//! A trade post is one of the map's neutral posts (a Village, Town or
//! Bazaar, [`crate::ConquestSetup::posts`]), a Waystation, or an ally's
//! finished main building: the last pays the full rate, and its owner
//! gains [`ALLY_PERCENT`] of each delivery besides, banked when the
//! caravan gets home, if the ally is still in the match and no enemy of
//! the owner's by then.
//!
//! **Strongbox.** A caravan that is destroyed carrying cargo drops half
//! of it as a Grub pile: a temporary node at the spot, in one of
//! [`MAX_PILES`] slots (with none free the cargo is lost), that anyone's
//! workers can gather at and that is gone once empty.

use gc_fixed::{Fx, FxVec2};

use super::{Economy, Resource, Step};
use crate::conquest::ConquestUnit;
use crate::regiments::Regiments;
use crate::stances::Stances;
use crate::structures::{Role, StructureHandle, Structures, TRADE_POST, footprint_centre};
use crate::{MAX_PARTIES, PlayerId, StateHasher, TICKS_PER_SECOND, UnitStats};

/// Percent of a delivery's pay a Waystation, a trade post of the caravan
/// owner's own, gives.
pub const WAYSTATION_PERCENT: u32 = 50;
/// Percent of a delivery's pay an ally whose main building was the trade
/// post gains besides, as a bonus.
pub const ALLY_PERCENT: u32 = 20;
/// Ticks a caravan takes loading at a post at Standard tempo.
pub const LOAD_TICKS: u32 = 8 * TICKS_PER_SECOND;
/// How near a caravan's centre must come to a neutral post's to trade
/// there, in tiles.
pub const POST_REACH: Fx = Fx::from_int(3);
/// Grub piles that can lie on the field at once.
pub const MAX_PILES: usize = 16;
/// Ticks a caravan that can't get on waits before trying again.
const RETRY_TICKS: u16 = TICKS_PER_SECOND as u16;

/// Where a caravan trades.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradePost {
    /// One of the map's neutral trade posts, by its index in
    /// [`crate::ConquestSetup::posts`].
    Hamlet(u16),
    /// A building: its owner's Waystation, or an ally's main building.
    Structure(StructureHandle),
}

/// How a trade post pays a caravan of one player's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rate {
    /// A neutral post, or an ally's main building when the ally's share
    /// is the bonus on top (`Some(ally)`).
    Full(Option<PlayerId>),
    /// The caravan's owner's own Waystation.
    Half,
}

/// What a caravan's round reads of the world beyond the economy.
pub(super) struct Field<'a> {
    pub units: &'a [UnitStats],
    pub kinds: &'a [ConquestUnit],
    pub structures: &'a Structures,
    pub stances: &'a Stances,
    /// The parties out of the match, as a bitmask.
    pub out: u8,
}

impl Field<'_> {
    /// Whether `ally`, whose main building a caravan of `owner`'s traded
    /// at, still gets its share.
    fn shares(&self, owner: PlayerId, ally: PlayerId) -> bool {
        self.out & (1 << ally) == 0 && !self.stances.hostile(owner, ally)
    }
}

/// Where a trade post stands.
enum Spot {
    /// A neutral post's point.
    Point(FxVec2),
    /// A building's footprint.
    Building([i32; 4]),
}

impl Spot {
    fn centre(&self) -> FxVec2 {
        match *self {
            Spot::Point(at) => at,
            Spot::Building(foot) => footprint_centre(foot),
        }
    }
}

impl Economy {
    /// The map's neutral trade posts, in tiles.
    pub fn posts(&self) -> &[FxVec2] {
        &self.posts
    }

    /// How `post` would pay a caravan of `player`'s, or `None` if it is
    /// no trade post for them: a neutral post that isn't there, a building
    /// that is gone or unfinished, an enemy's, or one of their own that
    /// isn't a Waystation.
    pub(crate) fn rate_of(
        &self,
        structures: &Structures,
        stances: &Stances,
        player: PlayerId,
        post: TradePost,
    ) -> Option<Rate> {
        match post {
            TradePost::Hamlet(i) => (usize::from(i) < self.posts.len()).then_some(Rate::Full(None)),
            TradePost::Structure(h) => {
                let i = structures.resolve(h).filter(|&i| structures.finished(i))?;
                let owner = structures.owner_of(i);
                let stats = structures.stats_of(i);
                if owner == player {
                    (stats.flags & TRADE_POST != 0).then_some(Rate::Half)
                } else {
                    (stats.role == Role::Main && !stances.hostile(player, owner))
                        .then_some(Rate::Full(Some(owner)))
                }
            }
        }
    }

    /// How caravan slot `r`'s post pays it and where it stands, if it is
    /// still a trade post for it.
    fn post_spot(&self, field: &Field, r: usize, owner: PlayerId) -> Option<(Rate, Spot)> {
        let post = self.post[r]?;
        let rate = self.rate_of(field.structures, field.stances, owner, post)?;
        let spot = match post {
            TradePost::Hamlet(i) => Spot::Point(*self.posts.get(usize::from(i))?),
            TradePost::Structure(h) => {
                Spot::Building(field.structures.footprint(field.structures.resolve(h)?))
            }
        };
        Some((rate, spot))
    }

    /// The post caravan slot `r` is trading at, if it is.
    pub(crate) fn trade_of(&self, regs: &Regiments, r: usize) -> Option<TradePost> {
        if self.step[r].trading() && self.ours(regs, r) {
            self.post[r]
        } else {
            None
        }
    }

    /// What caravan slot `r` carries, in whole Grub (0 for nothing).
    pub(crate) fn cargo_of(&self, regs: &Regiments, r: usize) -> u32 {
        if self.ours(regs, r) {
            self.cargo[r] / super::GRUB_SCALE
        } else {
            0
        }
    }

    /// Send caravan slot `r` to trade at `post`; the caller has checked
    /// [`Economy::rate_of`] and the unit, and stopped its other work. A
    /// caravan still carrying cargo takes it home first.
    pub(crate) fn trade(&mut self, regs: &mut Regiments, r: usize, post: TradePost) {
        if !self.ours(regs, r) {
            // A new regiment in the slot: nothing carried over.
            self.take_cargo(r);
            self.load[r] = None;
        }
        regs.halt(r);
        self.post[r] = Some(post);
        self.work_gen[r] = regs.generation[r];
        self.step[r] = if self.cargo[r] > 0 {
            Step::Hauling
        } else {
            Step::ToPost
        };
        self.clock[r] = 0;
        self.set_off(r);
    }

    /// Regiment slot `r` is gone from the match for good (its player is
    /// out): whatever it carried is lost with it, with no pile.
    pub(crate) fn forget(&mut self, r: usize) {
        self.take_cargo(r);
        self.stop(r);
    }

    /// Slot `r`'s cargo, in hundredths of a Grub, and the ally with a share
    /// of it: it is gone from the slot.
    fn take_cargo(&mut self, r: usize) -> (u32, Option<PlayerId>) {
        (
            core::mem::take(&mut self.cargo[r]),
            self.cargo_ally[r].take(),
        )
    }

    /// A caravan destroyed with cargo drops half of it where it fell.
    pub(super) fn strongboxes(&mut self, regs: &Regiments) {
        for r in 0..self.cargo.len() {
            if self.cargo[r] == 0 || self.ours(regs, r) {
                continue;
            }
            let (cargo, _) = self.take_cargo(r);
            self.go_idle(r);
            self.drop_pile(regs.position[r], cargo / (2 * super::GRUB_SCALE));
        }
    }

    /// `amount` Grub lies at `at`, in the first free pile slot; with none
    /// free, or nothing to lay down, it is lost.
    fn drop_pile(&mut self, at: FxVec2, amount: u32) {
        if amount == 0 {
            return;
        }
        let Some(n) = (self.real..self.node_full.len()).find(|&n| self.node_full[n] == 0) else {
            return;
        };
        self.node_resource[n] = Resource::Grub;
        self.node_at[n] = at;
        self.node_left[n] = amount;
        self.node_full[n] = amount;
        self.node_busy[n] = 0;
        self.node_tuner[n] = None;
        self.node_harmony[n] = 0;
        self.tune_clock[n] = 0;
    }

    /// One tick of caravan slot `r`'s round.
    pub(super) fn trade_round(
        &mut self,
        regs: &mut Regiments,
        field: &Field,
        r: usize,
        used: &mut Option<[u32; MAX_PARTIES]>,
    ) {
        match self.step[r] {
            Step::ToPost => self.walk_to_post(regs, field, r),
            Step::Loading if self.clock[r] == 0 => self.pick_up(regs, field, r),
            Step::Hauling => self.haul(regs, field, r, used),
            _ => {}
        }
    }

    /// Walk to the post, and start loading once there. A post that is no
    /// longer one ends the run.
    fn walk_to_post(&mut self, regs: &mut Regiments, field: &Field, r: usize) {
        let Some((_, spot)) = self.post_spot(field, r, regs.owner[r]) else {
            self.go_idle(r);
            return;
        };
        let there = match spot {
            Spot::Point(at) => self.at_point(regs, r, at, POST_REACH),
            Spot::Building(foot) => self.reach(regs, field.units, r, foot),
        };
        if !there {
            return;
        }
        self.step[r] = Step::Loading;
        self.set_off(r);
        self.clock[r] = self.clock_for(LOAD_TICKS);
    }

    /// The cargo is loaded: it is worth what the route pays, and the
    /// caravan sets off home with it.
    fn pick_up(&mut self, regs: &mut Regiments, field: &Field, r: usize) {
        let owner = regs.owner[r];
        let Some((rate, spot)) = self.post_spot(field, r, owner) else {
            self.go_idle(r);
            return;
        };
        let at = spot.centre();
        let Some(home) = field.structures.nearest_main(owner, at) else {
            // No main building to carry it to: wait for one.
            self.clock[r] = RETRY_TICKS;
            return;
        };
        let tiles = (at - footprint_centre(home)).length().floor_int().max(0) as u32;
        let unit = field
            .kinds
            .get(usize::from(regs.unit[r]))
            .copied()
            .unwrap_or_default();
        // In hundredths of a Grub: the base, and tenths of a Grub a tile.
        let full = u32::from(unit.trade_base) * 100 + u32::from(unit.trade_tenths) * 10 * tiles;
        let (pay, ally) = match rate {
            Rate::Full(ally) => (full, ally),
            Rate::Half => (full * WAYSTATION_PERCENT / 100, None),
        };
        self.cargo[r] = pay;
        self.cargo_ally[r] = ally;
        self.step[r] = Step::Hauling;
        self.set_off(r);
        self.clock[r] = 0;
    }

    /// Bring the cargo to the owner's nearest main building and bank it
    /// (and an ally's share), then go back to the post.
    fn haul(
        &mut self,
        regs: &mut Regiments,
        field: &Field,
        r: usize,
        used: &mut Option<[u32; MAX_PARTIES]>,
    ) {
        if self.travelling(regs, r) {
            return;
        }
        let owner = regs.owner[r];
        let Some(foot) = field.structures.nearest_main(owner, regs.position[r]) else {
            // Nowhere to take it: wait for a main building.
            self.clock[r] = RETRY_TICKS;
            return;
        };
        if !self.reach(regs, field.units, r, foot) {
            return;
        }
        let (cargo, ally) = self.take_cargo(r);
        self.deposit_grub(owner, cargo, regs, field.kinds, used);
        // The ally's share is decided now, not when the cargo was loaded.
        if let Some(ally) = ally.filter(|&a| field.shares(owner, a)) {
            let bonus = cargo * ALLY_PERCENT / 100;
            self.deposit_grub(ally, bonus, regs, field.kinds, used);
        }
        self.step[r] = Step::ToPost;
        self.set_off(r);
    }

    /// Caravan slot `r`'s trade state, into the state hash.
    pub(super) fn hash_trade(&self, r: usize, cargo: u32, h: &mut StateHasher) {
        match self.post[r] {
            None => h.u8(0),
            Some(TradePost::Hamlet(i)) => {
                h.u8(1);
                h.u16(i);
            }
            Some(TradePost::Structure(s)) => {
                h.u8(2);
                h.u32(s.index);
                h.u16(s.generation);
            }
        }
        h.u32(cargo);
        h.u8(self.cargo_ally[r].unwrap_or(PlayerId::MAX));
    }
}
