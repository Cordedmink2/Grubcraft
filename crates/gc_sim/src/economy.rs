//! The Conquest economy (docs/04-economy.md): Grub seeps and Marrow veins,
//! workers carrying what they gather to a drop-off, each player's store,
//! the supply cap and upkeep.
//!
//! - **Nodes.** A seep starts with [`SEEP_GRUB`] and a vein with
//!   [`VEIN_MARROW`] (the map may give others). Each takes so many workers
//!   at once ([`GRUB_SLOTS`], [`MARROW_SLOTS`]); a worker who finds it full
//!   waits beside it for a place. A node run dry turns to **dregs**: it
//!   still yields, at [`DREGS_PERCENT`] of the rate, forever.
//! - **Workers** are regiments of a worker type (`ConquestUnit::worker`).
//!   Ordered to a node ([`crate::Command::Gather`]) a worker walks there,
//!   fills its load over [`GRUB_TICKS`] (or [`MARROW_TICKS`]), carries it
//!   to the nearest of its owner's buildings that takes that resource,
//!   puts it in the store and goes back for more, until given another
//!   order. A worker given another order keeps what it carries, and drops
//!   it off first the next time it is sent to gather.
//! - **Burrowing workers** (`ConquestUnit::burrows`, the Brood's Gnawers)
//!   stay at their node: each load takes [`VEIN_FLOW_TICKS`] longer, and
//!   then comes into the store through their owner's root-veins, if the
//!   node is on them; off them, it waits there until it is (see
//!   [`crate::veins`]).
//! - **Tuned nodes** (the Choir's): a node within reach of a finished
//!   building that tunes its kind (a Resonator for seeps, a Siphon for
//!   veins, see [`crate::power`]) yields a load every [`TUNED_GRUB_TICKS`]
//!   (or [`TUNED_MARROW_TICKS`]) into that building's owner's store, with
//!   no workers, as it runs dry and to dregs like any node. Workers can
//!   still gather there too.
//! - **Supply.** A player's cap is what their standing buildings add, up to
//!   [`SUPPLY_LIMIT`]; what their regiments take counts against it.
//! - **Upkeep.** Grub brought in is cut by how much supply is in use: all
//!   of it up to 60, 75% up to 100, 55% beyond. Marrow is never cut.
//! - **Tempo** scales gathering: a worker fills its load in
//!   `100 / gather_percent` of the time.
//!
//! Five workers on a seep seven tiles from their hall bring in about 200
//! Grub a minute, and three on a vein about 50 Marrow, as docs/04 sets; a
//! tuned seep or vein brings in the same.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2};

use crate::conquest::{ConquestSetup, ConquestUnit};
use crate::regiments::{Order, Regiments};
use crate::structures::{Approach, DROP_OFF_GRUB, DROP_OFF_MARROW, StructureHandle, Structures};
use crate::veins::VEIN_FLOW_TICKS;
use crate::{MAX_PARTIES, PlayerId, StateHasher, TICKS_PER_SECOND, UnitStats};

/// What a node yields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Resource {
    Grub = 0,
    Marrow = 1,
}

impl Resource {
    pub fn from_u8(v: u8) -> Option<Resource> {
        [Resource::Grub, Resource::Marrow]
            .get(usize::from(v))
            .copied()
    }

    /// Workers who can gather at one node at once.
    pub fn slots(self) -> u8 {
        match self {
            Resource::Grub => GRUB_SLOTS,
            Resource::Marrow => MARROW_SLOTS,
        }
    }

    /// A worker's load.
    pub fn load(self) -> u32 {
        match self {
            Resource::Grub => GRUB_LOAD,
            Resource::Marrow => MARROW_LOAD,
        }
    }

    /// Ticks to fill a load at Standard tempo.
    pub fn fill_ticks(self) -> u32 {
        match self {
            Resource::Grub => GRUB_TICKS,
            Resource::Marrow => MARROW_TICKS,
        }
    }

    /// Ticks a tuned node takes to yield a load at Standard tempo.
    pub fn tune_ticks(self) -> u32 {
        match self {
            Resource::Grub => TUNED_GRUB_TICKS,
            Resource::Marrow => TUNED_MARROW_TICKS,
        }
    }

    /// The building flag of a drop-off that takes it.
    pub fn drop_off(self) -> u8 {
        match self {
            Resource::Grub => DROP_OFF_GRUB,
            Resource::Marrow => DROP_OFF_MARROW,
        }
    }
}

/// A fresh seep's Grub and a fresh vein's Marrow.
pub const SEEP_GRUB: u32 = 7_500;
pub const VEIN_MARROW: u32 = 3_000;
/// Workers who can gather at a seep, and at a vein, at once.
pub const GRUB_SLOTS: u8 = 5;
pub const MARROW_SLOTS: u8 = 3;
/// A worker's load.
pub const GRUB_LOAD: u32 = 10;
pub const MARROW_LOAD: u32 = 5;
/// Ticks to fill a load at Standard tempo.
pub const GRUB_TICKS: u32 = 9 * TICKS_PER_SECOND;
pub const MARROW_TICKS: u32 = 12 * TICKS_PER_SECOND;
/// Ticks a tuned seep, and a tuned vein, takes to yield a load at
/// Standard tempo: about 200 Grub and 50 Marrow a minute.
pub const TUNED_GRUB_TICKS: u32 = 3 * TICKS_PER_SECOND;
pub const TUNED_MARROW_TICKS: u32 = 6 * TICKS_PER_SECOND;
/// A dry node still yields at this percent of its rate.
pub const DREGS_PERCENT: u32 = 25;
/// The most supply a player can have.
pub const SUPPLY_LIMIT: u32 = 150;
/// Upkeep: (supply in use up to, percent of Grub kept).
pub const UPKEEP: [(u32, u32); 3] = [(60, 100), (100, 75), (SUPPLY_LIMIT, 55)];
/// How near a worker's centre must come to a node's to gather from it.
pub const NODE_REACH: Fx = Fx::from_int(2);
/// Ticks a worker that can't get on waits before trying again.
const RETRY_TICKS: u16 = TICKS_PER_SECOND as u16;
/// Tries in a row that bring a worker no nearer before it gives up.
const STALLS: u8 = 3;
/// Grub is kept in hundredths, so upkeep's cut of a small load isn't lost.
const GRUB_SCALE: u32 = 100;

/// A seep or vein as the map places it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeSetup {
    pub resource: Resource,
    /// Its centre.
    pub at: FxVec2,
    /// What it holds before it runs to dregs.
    pub amount: u32,
}

/// A node as the game sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeView {
    pub index: u16,
    pub resource: Resource,
    pub at: FxVec2,
    /// Left before dregs.
    pub left: u32,
    pub full: u32,
    /// Workers filling a load there, as of the last tick.
    pub workers: u8,
    /// The building tuning it, as of the last tick.
    pub tuner: Option<StructureHandle>,
}

/// A player's store and supply, as the game's top bar shows them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bank {
    pub grub: u32,
    pub marrow: u32,
    pub supply_used: u32,
    pub supply_cap: u32,
    /// Brood: larvae ready in their finished Hives and Hatchery Pits, and
    /// how many those can hold (both 0 for a player with none).
    pub larvae: u32,
    pub larva_room: u32,
}

impl Bank {
    /// Percent of Grub brought in that the player keeps.
    pub fn upkeep_percent(&self) -> u32 {
        upkeep_percent(self.supply_used)
    }
}

fn upkeep_percent(supply_used: u32) -> u32 {
    UPKEEP
        .iter()
        .find(|&&(up_to, _)| supply_used <= up_to)
        .map_or(UPKEEP[UPKEEP.len() - 1].1, |&(_, keep)| keep)
}

/// Where a worker is in its round.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
enum Step {
    /// Not gathering.
    #[default]
    Idle = 0,
    /// Walking to its node, or waiting beside it for a place.
    ToNode = 1,
    /// Filling its load; the clock counts down.
    Filling = 2,
    /// Carrying its load to a drop-off.
    Carrying = 3,
}

/// The match's nodes, each player's store and every worker's round.
pub struct Economy {
    gather_percent: u32,
    node_resource: Vec<Resource>,
    node_at: Vec<FxVec2>,
    node_left: Vec<u32>,
    node_full: Vec<u32>,
    /// Workers filling a load at each node; counted afresh each tick.
    node_busy: Vec<u8>,
    /// The building tuning each node, worked out again whenever the
    /// tuners change (the [`Structures::tuner_stamp`] last seen), and
    /// ticks it has worked towards its next load (back to 0 when another
    /// takes over).
    node_tuner: Vec<Option<StructureHandle>>,
    tuners_seen: Option<u32>,
    tune_clock: Vec<u32>,
    /// Grub in hundredths, and Marrow, by player.
    grub: [u32; MAX_PARTIES],
    marrow: [u32; MAX_PARTIES],
    /// By regiment slot: its round, the node it works, the generation of
    /// the regiment sent (a slot reused by a new regiment doesn't inherit
    /// it), what it carries, and ticks until its load is full or its next
    /// try.
    step: Vec<Step>,
    node: Vec<u16>,
    work_gen: Vec<u16>,
    load: Vec<Option<(Resource, u8)>>,
    clock: Vec<u16>,
    /// By regiment slot, while filling: what it took from the node when it
    /// began (0 from dregs), given back if it stops before its load is full.
    taking: Vec<u8>,
    /// By regiment slot: how far it was from where it was going when it
    /// last set off, and how many times in a row that brought it no nearer.
    gap: Vec<Fx>,
    stalls: Vec<u8>,
}

impl Economy {
    pub(crate) fn new(setup: &ConquestSetup, regiments: usize) -> Economy {
        let n = regiments;
        Economy {
            gather_percent: setup.gather_percent.clamp(10, 1000),
            node_resource: setup.nodes.iter().map(|n| n.resource).collect(),
            node_at: setup.nodes.iter().map(|n| n.at).collect(),
            node_left: setup.nodes.iter().map(|n| n.amount).collect(),
            node_full: setup.nodes.iter().map(|n| n.amount).collect(),
            node_busy: vec![0; setup.nodes.len()],
            node_tuner: vec![None; setup.nodes.len()],
            tuners_seen: None,
            tune_clock: vec![0; setup.nodes.len()],
            grub: setup.start.map(|(g, _)| g.saturating_mul(GRUB_SCALE)),
            marrow: setup.start.map(|(_, m)| m),
            step: vec![Step::Idle; n],
            node: vec![0; n],
            work_gen: vec![0; n],
            load: vec![None; n],
            clock: vec![0; n],
            taking: vec![0; n],
            gap: vec![Fx::MAX; n],
            stalls: vec![0; n],
        }
    }

    /// Player `p`'s Grub and Marrow.
    pub(crate) fn store(&self, p: PlayerId) -> (u32, u32) {
        let i = usize::from(p);
        if i >= MAX_PARTIES {
            return (0, 0);
        }
        (self.grub[i] / GRUB_SCALE, self.marrow[i])
    }

    /// Each player's supply in use: what their regiments take. (Their cap
    /// is what their standing buildings add, up to [`SUPPLY_LIMIT`].)
    pub(crate) fn supply_in_use(
        &self,
        units: &[ConquestUnit],
        regs: &Regiments,
    ) -> [u32; MAX_PARTIES] {
        let mut used = [0; MAX_PARTIES];
        for r in 0..regs.alive.len() {
            if regs.alive[r]
                && let Some(u) = used.get_mut(usize::from(regs.owner[r]))
            {
                *u += units
                    .get(usize::from(regs.unit[r]))
                    .map_or(0, |unit| u32::from(unit.supply));
            }
        }
        used
    }

    /// Whether `p` has `grub` and `marrow` in store.
    pub(crate) fn can_afford(&self, p: PlayerId, grub: u32, marrow: u32) -> bool {
        let i = usize::from(p);
        i < MAX_PARTIES && self.grub[i] >= grub * GRUB_SCALE && self.marrow[i] >= marrow
    }

    /// Take `grub` and `marrow` from `p`'s store, if they have it.
    pub(crate) fn spend(&mut self, p: PlayerId, grub: u32, marrow: u32) -> bool {
        if !self.can_afford(p, grub, marrow) {
            return false;
        }
        let i = usize::from(p);
        self.grub[i] -= grub * GRUB_SCALE;
        self.marrow[i] -= marrow;
        true
    }

    /// Give `grub` and `marrow` back to `p`'s store.
    pub(crate) fn refund(&mut self, p: PlayerId, grub: u32, marrow: u32) {
        let i = usize::from(p);
        if i < MAX_PARTIES {
            self.grub[i] = self.grub[i].saturating_add(grub * GRUB_SCALE);
            self.marrow[i] = self.marrow[i].saturating_add(marrow);
        }
    }

    /// Every node, in index order.
    pub fn nodes(&self) -> impl Iterator<Item = NodeView> + '_ {
        (0..self.node_at.len()).map(|i| NodeView {
            index: i as u16,
            resource: self.node_resource[i],
            at: self.node_at[i],
            left: self.node_left[i],
            full: self.node_full[i],
            workers: self.node_busy[i],
            tuner: self.node_tuner[i],
        })
    }

    /// Where each node is, by index.
    pub fn node_positions(&self) -> &[FxVec2] {
        &self.node_at
    }

    /// Whether the regiment in slot `r` is the one this economy's record
    /// for the slot is about.
    fn ours(&self, regs: &Regiments, r: usize) -> bool {
        regs.alive[r] && regs.generation[r] == self.work_gen[r]
    }

    /// The node regiment slot `r` is working, if it is.
    pub(crate) fn work_of(&self, regs: &Regiments, r: usize) -> Option<u16> {
        (self.step[r] != Step::Idle && self.ours(regs, r)).then_some(self.node[r])
    }

    /// What regiment slot `r` carries: how much of what.
    pub(crate) fn carried(&self, regs: &Regiments, r: usize) -> Option<(Resource, u32)> {
        let (kind, amount) = self.load[r].filter(|_| self.ours(regs, r))?;
        Some((kind, u32::from(amount)))
    }

    /// Whether the regiment in slot `r` could be sent to gather at `node`.
    pub(crate) fn can_gather(&self, node: u16) -> bool {
        usize::from(node) < self.node_at.len()
    }

    /// Send worker slot `r` to gather at `node`; the caller has checked
    /// [`Economy::can_gather`]. A worker still carrying a load takes it
    /// home first.
    pub(crate) fn gather(&mut self, regs: &mut Regiments, r: usize, node: u16) {
        if !self.ours(regs, r) {
            // A new regiment in the slot: nothing carried over.
            self.load[r] = None;
        }
        regs.halt(r);
        self.step[r] = if self.load[r].is_some() {
            Step::Carrying
        } else {
            Step::ToNode
        };
        self.node[r] = node;
        self.work_gen[r] = regs.generation[r];
        self.clock[r] = 0;
        self.set_off(r);
    }

    /// Regiment slot `r` was given another order: it stops gathering, but
    /// keeps what it carries.
    pub(crate) fn stop(&mut self, r: usize) {
        self.leave_node(r);
        self.step[r] = Step::Idle;
    }

    /// A worker filling a load stops short: what it took goes back to the
    /// node, and it is back on its way there.
    fn leave_node(&mut self, r: usize) {
        if self.step[r] == Step::Filling {
            let n = usize::from(self.node[r]);
            self.node_left[n] += u32::from(self.taking[r]);
            self.taking[r] = 0;
            self.step[r] = Step::ToNode;
        }
    }

    /// Slot `r` sets off somewhere new: no tries counted yet.
    fn set_off(&mut self, r: usize) {
        self.gap[r] = Fx::MAX;
        self.stalls[r] = 0;
    }

    /// Slot `r`, `gap` from where it is going, sets off for it again.
    /// Returns false, and gives up gathering, once tries in a row have
    /// brought it no nearer.
    fn try_again(&mut self, r: usize, gap: Fx) -> bool {
        if gap >= self.gap[r] {
            self.stalls[r] += 1;
        } else {
            self.stalls[r] = 0;
        }
        self.gap[r] = gap;
        if self.stalls[r] >= STALLS {
            self.step[r] = Step::Idle;
            return false;
        }
        self.clock[r] = RETRY_TICKS;
        true
    }

    /// One tick of the economy: every worker on its round.
    pub(crate) fn update(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        kinds: &[ConquestUnit],
        structures: &Structures,
    ) {
        self.node_busy.fill(0);
        for r in 0..self.step.len() {
            if self.step[r] == Step::Filling && self.ours(regs, r) {
                self.node_busy[usize::from(self.node[r])] += 1;
            }
        }
        // Each player's supply in use, counted at the first Grub brought in.
        let mut used = None;
        self.tune(regs, kinds, structures, &mut used);
        for r in 0..self.step.len() {
            if self.step[r] == Step::Idle {
                continue;
            }
            if !self.ours(regs, r) {
                self.stop(r);
                continue;
            }
            if regs.morale_state[r].broken() {
                // Running for it; back to work once rallied.
                self.leave_node(r);
                continue;
            }
            if regs.engaged[r].is_some() {
                continue;
            }
            self.clock[r] = self.clock[r].saturating_sub(1);
            let n = usize::from(self.node[r]);
            let burrows = kinds
                .get(usize::from(regs.unit[r]))
                .is_some_and(|u| u.burrows);
            match self.step[r] {
                Step::ToNode => self.walk_to_node(regs, r, n, burrows),
                Step::Filling if self.clock[r] == 0 => self.fill(r, n),
                Step::Carrying if burrows => self.send_home(regs, kinds, structures, r, &mut used),
                Step::Carrying => self.carry(regs, units, kinds, structures, r, &mut used),
                Step::Idle | Step::Filling => {}
            }
        }
    }

    /// Walk to node `n`, and start filling once there and a place is free
    /// (a worker that `burrows` takes its load's flow home with it).
    fn walk_to_node(&mut self, regs: &mut Regiments, r: usize, n: usize, burrows: bool) {
        if !self.at_node(regs, r, n) {
            return;
        }
        let kind = self.node_resource[n];
        if self.node_busy[n] >= kind.slots() {
            return;
        }
        self.node_busy[n] += 1;
        self.step[r] = Step::Filling;
        self.set_off(r);
        // The load is taken from the node now, so workers filling at once
        // can't take more than it holds. A dry node gives a whole load,
        // only slower.
        let left = self.node_left[n];
        self.taking[r] = kind.load().min(left) as u8;
        self.node_left[n] = left - u32::from(self.taking[r]);
        let mut ticks = kind.fill_ticks();
        if left == 0 {
            ticks = ticks * 100 / DREGS_PERCENT;
        }
        if burrows {
            ticks += VEIN_FLOW_TICKS;
        }
        ticks = ticks * 100 / self.gather_percent;
        self.clock[r] = u16::try_from(ticks.max(1)).unwrap_or(u16::MAX);
    }

    /// Whether worker slot `r` is at node `n`; if not, it heads there.
    fn at_node(&mut self, regs: &mut Regiments, r: usize, n: usize) -> bool {
        let at = self.node_at[n];
        let here = regs.position[r];
        let reach = u64::from(NODE_REACH.raw().unsigned_abs());
        if (at - here).length_sq_raw() > reach * reach {
            if regs.target[r].is_none()
                && self.clock[r] == 0
                && self.try_again(r, (at - here).length())
            {
                // A tile short on the side it comes from, so workers
                // arriving from different ways don't all crowd one point.
                let stand = at + (here - at).normalized();
                let h = regs.handle(r);
                regs.order(
                    h,
                    Order {
                        target: stand,
                        facing: None,
                        files: 0,
                        pace: Fx::ZERO,
                    },
                );
            }
            return false;
        }
        regs.target[r] = None;
        true
    }

    /// The load from node `n` is full: take it and head for a drop-off.
    fn fill(&mut self, r: usize, n: usize) {
        let kind = self.node_resource[n];
        let take = match self.taking[r] {
            0 => kind.load(),
            some => u32::from(some),
        };
        self.taking[r] = 0;
        self.node_busy[n] -= 1;
        self.load[r] = Some((kind, take as u8));
        self.step[r] = Step::Carrying;
        self.clock[r] = 0;
    }

    /// Carry the load to the nearest drop-off and put it in the store.
    fn carry(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        kinds: &[ConquestUnit],
        structures: &Structures,
        r: usize,
        used: &mut Option<[u32; MAX_PARTIES]>,
    ) {
        if regs.target[r].is_some() || self.clock[r] > 0 {
            // On the way, or waiting to try again.
            return;
        }
        let Some((kind, _)) = self.load[r] else {
            self.step[r] = Step::ToNode;
            return;
        };
        let owner = regs.owner[r];
        let Some(foot) = structures.nearest_drop_off(owner, kind.drop_off(), regs.position[r])
        else {
            // Nowhere to take it: wait for one to go up.
            self.clock[r] = RETRY_TICKS;
            return;
        };
        let way = Approach::of(regs, &units[usize::from(regs.unit[r])], r, foot);
        if way.gap > way.reach() {
            if self.try_again(r, way.gap) {
                way.walk(regs, r);
            }
            return;
        }
        self.bank(regs, kinds, r, used);
    }

    /// A burrowed worker's load comes home through the root-veins once it
    /// is at its node and the node is on its owner's veins; else it tries
    /// again in a while. (A worker sent elsewhere with a load takes it to
    /// its new node first.)
    fn send_home(
        &mut self,
        regs: &mut Regiments,
        kinds: &[ConquestUnit],
        structures: &Structures,
        r: usize,
        used: &mut Option<[u32; MAX_PARTIES]>,
    ) {
        let n = usize::from(self.node[r]);
        if self.clock[r] > 0 || !self.at_node(regs, r, n) {
            return;
        }
        if !structures.on_veins(regs.owner[r], self.node_at[n]) {
            self.clock[r] = RETRY_TICKS;
            return;
        }
        self.bank(regs, kinds, r, used);
    }

    /// Every tuned node works towards its next load, and yields it into
    /// its tuner's owner's store once done.
    fn tune(
        &mut self,
        regs: &Regiments,
        kinds: &[ConquestUnit],
        structures: &Structures,
        used: &mut Option<[u32; MAX_PARTIES]>,
    ) {
        if self.tuners_seen != Some(structures.tuner_stamp) {
            self.tuners_seen = Some(structures.tuner_stamp);
            for n in 0..self.node_at.len() {
                let tuner = structures.tuner_of(self.node_resource[n], self.node_at[n]);
                let handle = tuner.map(|s| structures.handle(s));
                if handle != self.node_tuner[n] {
                    self.node_tuner[n] = handle;
                    self.tune_clock[n] = 0;
                }
            }
        }
        for n in 0..self.node_at.len() {
            let kind = self.node_resource[n];
            // A tuner fallen since this tick's links works nothing.
            let Some(s) = self.node_tuner[n].and_then(|h| structures.resolve(h)) else {
                continue;
            };
            let left = self.node_left[n];
            self.tune_clock[n] += 1;
            if self.tune_clock[n] < self.tune_period(n) {
                continue;
            }
            self.tune_clock[n] = 0;
            // A dry node gives a whole load, only slower.
            let take = if left == 0 {
                kind.load()
            } else {
                kind.load().min(left)
            };
            self.node_left[n] = left.saturating_sub(take);
            self.deposit(structures.owner_of(s), kind, take, regs, kinds, used);
        }
    }

    /// Ticks between loads from tuned node `n`: slower once it runs dry,
    /// and scaled by tempo.
    fn tune_period(&self, n: usize) -> u32 {
        let mut ticks = self.node_resource[n].tune_ticks();
        if self.node_left[n] == 0 {
            ticks = ticks * 100 / DREGS_PERCENT;
        }
        (ticks * 100 / self.gather_percent).max(1)
    }

    /// How much the nodes tuned by building `h` yield a minute, as things
    /// stand: dry nodes and tempo counted, before any upkeep cut to Grub.
    pub fn tuned_per_minute(&self, h: StructureHandle) -> u32 {
        (0..self.node_at.len())
            .filter(|&n| self.node_tuner[n] == Some(h))
            .map(|n| self.node_resource[n].load() * 60 * TICKS_PER_SECOND / self.tune_period(n))
            .sum()
    }

    /// `amount` of `kind` goes into player `p`'s store: Grub cut by their
    /// upkeep.
    fn deposit(
        &mut self,
        p: PlayerId,
        kind: Resource,
        amount: u32,
        regs: &Regiments,
        kinds: &[ConquestUnit],
        used: &mut Option<[u32; MAX_PARTIES]>,
    ) {
        let p = usize::from(p);
        if p >= MAX_PARTIES {
            return;
        }
        match kind {
            Resource::Grub => {
                let used = used.get_or_insert_with(|| self.supply_in_use(kinds, regs))[p];
                let kept = amount * upkeep_percent(used) * GRUB_SCALE / 100;
                self.grub[p] = self.grub[p].saturating_add(kept);
            }
            Resource::Marrow => self.marrow[p] = self.marrow[p].saturating_add(amount),
        }
    }

    /// Worker slot `r`'s load goes into its owner's store, and it goes back
    /// for more.
    fn bank(
        &mut self,
        regs: &Regiments,
        kinds: &[ConquestUnit],
        r: usize,
        used: &mut Option<[u32; MAX_PARTIES]>,
    ) {
        let Some((kind, amount)) = self.load[r] else {
            self.step[r] = Step::ToNode;
            return;
        };
        self.deposit(regs.owner[r], kind, u32::from(amount), regs, kinds, used);
        self.load[r] = None;
        self.step[r] = Step::ToNode;
        self.set_off(r);
    }

    pub(crate) fn hash_into(&self, regs: &Regiments, h: &mut StateHasher) {
        h.u32(self.gather_percent);
        h.u32(self.node_at.len() as u32);
        for i in 0..self.node_at.len() {
            h.u8(self.node_resource[i] as u8);
            h.i32(self.node_at[i].x.raw());
            h.i32(self.node_at[i].y.raw());
            h.u32(self.node_left[i]);
            h.u32(self.node_full[i]);
            h.u32(self.tune_clock[i]);
        }
        for p in 0..MAX_PARTIES {
            h.u32(self.grub[p]);
            h.u32(self.marrow[p]);
        }
        for r in 0..self.step.len() {
            if !self.ours(regs, r) || (self.step[r] == Step::Idle && self.load[r].is_none()) {
                h.u8(0);
                continue;
            }
            h.u8(1 + self.step[r] as u8);
            h.u16(self.node[r]);
            let (kind, amount) = self.load[r].map_or((0, 0), |(k, a)| (k as u8 + 1, a));
            h.u8(kind);
            h.u8(amount);
            h.u16(self.clock[r]);
            h.u8(self.taking[r]);
            h.i32(self.gap[r].raw());
            h.u8(self.stalls[r]);
        }
    }
}
