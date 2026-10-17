//! What a Conquest match adds to a battle (see [`crate::World::conquest`]):
//! buildings and their construction ([`crate::structures`]), the economy
//! ([`crate::economy`]), training ([`crate::production`]) and the Brood's
//! larvae ([`crate::larvae`]).
//!
//! The simulation knows no factions: what sets one apart comes from its
//! units' and buildings' traits ([`ConquestUnit`], `BuildingStats::flags`),
//! which the game sets from the faction rosters.

use alloc::vec::Vec;

use gc_fixed::FxVec2;

use crate::economy::{Bank, Economy, NodeSetup};
use crate::nav::Nav;
use crate::production::Production;
use crate::regiments::Regiments;
use crate::structures::{
    Breaker, BuildingId, BuildingStats, CANCEL_REFUND_PERCENT, Refusal, Role, StructureHandle,
    Structures, Work,
};
use crate::verdict::Victories;
use crate::{MAX_PARTIES, PlayerId, StateHasher, UnitId, UnitStats};

/// `ticks` at Standard tempo, at a tempo where things take `percent` of
/// their Standard time.
pub(crate) fn at_tempo(ticks: u32, percent: u32) -> u32 {
    (ticks * percent / 100).max(1)
}

/// What a Conquest match knows about each regiment type beyond its battle
/// stats, by [`crate::UnitId`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConquestUnit {
    /// How hard its blows fall on buildings.
    pub breaker: Breaker,
    /// It gathers Grub and Marrow ([`crate::Command::Gather`]).
    pub worker: bool,
    /// What it takes of its owner's supply.
    pub supply: u8,
    pub cost_grub: u16,
    pub cost_marrow: u16,
    /// Ticks to train at Standard tempo.
    pub train_ticks: u16,
    /// The building type that trains it (or any of that building's tier
    /// upgrades); `None` for a unit nothing trains.
    pub trained_at: Option<BuildingId>,
    /// The tier its owner needs to train it.
    pub tier: u8,
    /// It hatches from a larva (the Brood's units): training one takes a
    /// larva from its owner's [`crate::larvae`], and a building that
    /// hatches them hatches its whole queue at once.
    pub hatches: bool,
    /// A worker used up by the building it starts (the Brood's Gnawers):
    /// that building then grows on its own, and such a worker can't help
    /// build or mend one.
    pub consumed: bool,
    /// A worker that burrows in where it gathers (the Brood's Gnawers):
    /// it stays at its node and sends each load home through its owner's
    /// root-veins instead of carrying it (see [`crate::veins`]).
    pub burrows: bool,
}

impl ConquestUnit {
    fn hash_into(&self, h: &mut StateHasher) {
        h.u8(self.breaker as u8);
        h.u8(u8::from(self.worker));
        h.u8(self.supply);
        h.u16(self.cost_grub);
        h.u16(self.cost_marrow);
        h.u16(self.train_ticks);
        h.u16(self.trained_at.unwrap_or(u16::MAX));
        h.u8(self.tier);
        h.u8(u8::from(self.hatches) | u8::from(self.consumed) << 1 | u8::from(self.burrows) << 2);
    }
}

/// What a Conquest match adds to [`crate::MatchSetup`]. The default has
/// no buildings, units or nodes, room for [`ConquestSetup::STRUCTURES`]
/// buildings and Standard tempo.
#[derive(Clone, Copy, Debug)]
pub struct ConquestSetup<'a> {
    pub buildings: &'a [BuildingStats],
    /// By [`crate::UnitId`]; a type past the end is a plain regiment that
    /// gathers nothing and takes no supply.
    pub units: &'a [ConquestUnit],
    /// Room for this many buildings.
    pub structures: u32,
    /// The map's Grub seeps and Marrow veins.
    pub nodes: &'a [NodeSetup],
    /// Each player's Grub and Marrow at the start.
    pub start: [(u32, u32); MAX_PARTIES],
    /// Tempo: gathering at this percent of the Standard rate (Swift 150,
    /// Epic 80).
    pub gather_percent: u32,
    /// Tempo: building, upgrading and training take this percent of their
    /// Standard time (Swift 75, Epic 120). The Titan's Heart unlocks after
    /// this percent of its Standard wait too.
    pub build_percent: u32,
    /// How the match is won; with neither, by holding the field as a Field
    /// Battle is (the sandbox).
    pub victories: Victories,
    /// The middle of the Titan's Heart, in tiles, if the map has one.
    pub heart: Option<FxVec2>,
}

impl ConquestSetup<'_> {
    /// Warfield sizing from docs/10-architecture.md.
    pub const STRUCTURES: u32 = 1024;
    /// Standard tempo.
    pub const STANDARD: u32 = 100;
}

impl Default for ConquestSetup<'_> {
    fn default() -> Self {
        ConquestSetup {
            buildings: &[],
            units: &[],
            structures: Self::STRUCTURES,
            nodes: &[],
            start: [(0, 0); MAX_PARTIES],
            gather_percent: Self::STANDARD,
            build_percent: Self::STANDARD,
            victories: Victories::default(),
            heart: None,
        }
    }
}

/// A Conquest match's own systems, made together by
/// [`crate::World::conquest`].
pub(crate) struct Conquest {
    /// By [`UnitId`].
    pub(crate) units: Vec<ConquestUnit>,
    pub(crate) structures: Structures,
    pub(crate) economy: Economy,
    pub(crate) production: Production,
}

impl Conquest {
    pub(crate) fn new(setup: &ConquestSetup, regiments: usize) -> Conquest {
        Conquest {
            units: setup.units.to_vec(),
            structures: Structures::new(
                setup.buildings,
                setup.structures,
                regiments,
                setup.build_percent,
            ),
            economy: Economy::new(setup, regiments),
            production: Production::new(setup.structures as usize, setup.build_percent),
        }
    }

    /// The Conquest facts of unit type `unit`; a type past the end of the
    /// table is a plain regiment.
    pub(crate) fn unit(&self, unit: UnitId) -> ConquestUnit {
        self.units
            .get(usize::from(unit))
            .copied()
            .unwrap_or_default()
    }

    /// One tick: sieges and builders, then root-veins and larvae, then
    /// gathering, then training.
    pub(crate) fn update(&mut self, regs: &mut Regiments, stats: &[UnitStats], nav: &mut Nav) {
        self.structures.update(regs, stats, &self.units, nav);
        self.structures.link_veins();
        self.structures.regrow();
        self.structures.grow_larvae();
        self.economy
            .update(regs, stats, &self.units, &self.structures);
        let (units, economy) = (&self.units, &self.economy);
        self.production.update(&self.structures, nav, units, || {
            economy.supply_in_use(units, regs)
        });
    }

    /// Player `p`'s store and supply. Supply in use counts the regiments in
    /// training.
    pub(crate) fn bank(&self, regs: &Regiments, p: PlayerId) -> Bank {
        let (grub, marrow) = self.economy.store(p);
        let mut used = self.economy.supply_in_use(&self.units, regs);
        self.production
            .in_training(&self.structures, &self.units, &mut used);
        let (larvae, larva_room) = self.structures.larvae_ready(p);
        Bank {
            grub,
            marrow,
            supply_used: used.get(usize::from(p)).copied().unwrap_or(0),
            supply_cap: self.structures.supply_cap(p),
            larvae,
            larva_room,
        }
    }

    /// Whether regiment slot `r` could be sent to gather at `node`.
    pub(crate) fn can_gather(&self, regs: &Regiments, r: usize, node: u16) -> bool {
        self.unit(regs.unit[r]).worker && self.economy.can_gather(node)
    }

    /// Whether a building of type `building` may go up with its north-west
    /// tile at `tile` (see `Structures::check`).
    pub(crate) fn check(
        &self,
        building: BuildingId,
        tile: (i32, i32),
        nav: &Nav,
        regs: &Regiments,
    ) -> Result<(), Refusal> {
        self.structures
            .check(building, tile, nav, regs, self.economy.node_positions())
    }

    /// Whether `player` could have a worker lay a building of type
    /// `building` at `tile` now: placement, tier and price.
    pub(crate) fn can_lay(
        &self,
        player: PlayerId,
        building: BuildingId,
        tile: (i32, i32),
        nav: &Nav,
        regs: &Regiments,
    ) -> Result<(), Refusal> {
        self.check(building, tile, nav, regs)?;
        self.may_lay(player, building)
    }

    /// Whether `player` could lay a building of type `building` somewhere
    /// now: its tier and price, without the ground.
    pub(crate) fn may_lay(&self, player: PlayerId, building: BuildingId) -> Result<(), Refusal> {
        let b = *self
            .structures
            .buildings()
            .get(usize::from(building))
            .ok_or(Refusal::Unknown)?;
        if b.upgrades_from.is_some() {
            // Tier upgrades go up on the building they upgrade.
            return Err(Refusal::Unknown);
        }
        if self.structures.tier_of(player) < b.tier.max(1) && b.role != Role::Main {
            return Err(Refusal::Tier);
        }
        if !self
            .economy
            .can_afford(player, u32::from(b.cost_grub), u32::from(b.cost_marrow))
        {
            return Err(Refusal::Price);
        }
        Ok(())
    }

    /// Worker slot `r`, `player`'s, lays a building of type `building` at
    /// `tile`, pays for it and goes to build it.
    pub(crate) fn lay(
        &mut self,
        player: PlayerId,
        r: usize,
        building: BuildingId,
        tile: (i32, i32),
        nav: &mut Nav,
        regs: &mut Regiments,
    ) -> bool {
        if !self.unit(regs.unit[r]).worker
            || self.can_lay(player, building, tile, nav, regs).is_err()
        {
            return false;
        }
        let Some(h) = self.structures.found(player, building, tile, nav, regs) else {
            return false;
        };
        let b = self.structures.buildings()[usize::from(building)];
        self.economy
            .spend(player, u32::from(b.cost_grub), u32::from(b.cost_marrow));
        self.stop_work(r);
        self.structures.send(regs, r, h, Work::Build);
        true
    }

    /// Worker slot `r`, `player`'s, goes to build or mend `player`'s
    /// building `h`.
    pub(crate) fn construct(
        &mut self,
        player: PlayerId,
        r: usize,
        h: StructureHandle,
        regs: &mut Regiments,
    ) -> bool {
        let unit = self.unit(regs.unit[r]);
        let Some(i) = self.structures.owned(h, player) else {
            return false;
        };
        // A worker used up to start a building can only start one that
        // nothing has started yet.
        if !unit.worker
            || (unit.consumed && (self.structures.finished(i) || self.structures.growing(i)))
        {
            return false;
        }
        self.stop_work(r);
        self.structures.send(regs, r, h, Work::Build);
        true
    }

    /// `player` calls off building `h`'s tier upgrade, or the building
    /// itself while it is going up, for [`CANCEL_REFUND_PERCENT`] of what
    /// it cost.
    pub(crate) fn call_off(
        &mut self,
        player: PlayerId,
        h: StructureHandle,
        nav: &mut Nav,
        regs: &Regiments,
    ) -> bool {
        let Some(i) = self
            .structures
            .resolve(h)
            .filter(|_| self.structures.owner(h) == Some(player))
        else {
            return false;
        };
        let Some((grub, marrow)) = self.structures.call_off(i, nav, regs) else {
            return false;
        };
        let back = |n: u32| n * CANCEL_REFUND_PERCENT / 100;
        self.economy.refund(player, back(grub), back(marrow));
        true
    }

    /// Whether `player`'s building `h` could queue a regiment of type `unit`
    /// now, and if not, why.
    pub(crate) fn can_train(
        &self,
        player: PlayerId,
        h: StructureHandle,
        unit: UnitId,
    ) -> Result<(), Refusal> {
        let i = self.structures.owned(h, player).ok_or(Refusal::Unknown)?;
        let u = self.units.get(usize::from(unit)).ok_or(Refusal::Unknown)?;
        let building = self.structures.kind_of(i);
        if !u
            .trained_at
            .is_some_and(|at| self.structures.grew_from(building, at))
        {
            return Err(Refusal::Unknown);
        }
        if !self.structures.finished(i) || !self.production.has_room(h) {
            return Err(Refusal::Busy);
        }
        if self.structures.tier_of(player) < u.tier {
            return Err(Refusal::Tier);
        }
        if u.hatches && !self.structures.has_larva(player) {
            return Err(Refusal::Larva);
        }
        if !self
            .economy
            .can_afford(player, u32::from(u.cost_grub), u32::from(u.cost_marrow))
        {
            return Err(Refusal::Price);
        }
        Ok(())
    }

    /// `player`'s building `h` queues a regiment of type `unit`, paying
    /// for it, and a larva if it hatches from one.
    pub(crate) fn train(&mut self, player: PlayerId, h: StructureHandle, unit: UnitId) -> bool {
        if self.can_train(player, h, unit).is_err() {
            return false;
        }
        let u = self.unit(unit);
        self.economy
            .spend(player, u32::from(u.cost_grub), u32::from(u.cost_marrow));
        if u.hatches {
            self.structures.take_larva(player, h.index as usize);
        }
        self.production.enqueue(h, unit);
        true
    }

    /// `player` takes the last regiment off building `h`'s queue, for all
    /// it cost.
    pub(crate) fn untrain(&mut self, player: PlayerId, h: StructureHandle) -> bool {
        if self.ready(player, h).is_none() {
            return false;
        }
        let Some(unit) = self.production.dequeue(h) else {
            return false;
        };
        let u = self.unit(unit);
        self.economy
            .refund(player, u32::from(u.cost_grub), u32::from(u.cost_marrow));
        if u.hatches {
            self.structures.give_back_larva(player);
        }
        true
    }

    /// `player`'s building `h`'s slot and what it would upgrade into now,
    /// or why it can't.
    pub(crate) fn can_upgrade(
        &self,
        player: PlayerId,
        h: StructureHandle,
    ) -> Result<(usize, BuildingId), Refusal> {
        let i = self.structures.owned(h, player).ok_or(Refusal::Unknown)?;
        let to = self
            .structures
            .upgrade_of(self.structures.kind_of(i))
            .ok_or(Refusal::Unknown)?;
        if !self.structures.finished(i) || self.structures.upgrading(i) {
            return Err(Refusal::Busy);
        }
        let b = self.structures.buildings()[usize::from(to)];
        if !self
            .economy
            .can_afford(player, u32::from(b.cost_grub), u32::from(b.cost_marrow))
        {
            return Err(Refusal::Price);
        }
        Ok((i, to))
    }

    /// `player`'s main building `h` starts its tier upgrade.
    pub(crate) fn upgrade(&mut self, player: PlayerId, h: StructureHandle) -> bool {
        let Ok((i, to)) = self.can_upgrade(player, h) else {
            return false;
        };
        let b = self.structures.buildings()[usize::from(to)];
        self.economy
            .spend(player, u32::from(b.cost_grub), u32::from(b.cost_marrow));
        self.structures.start_upgrade(i, to);
        true
    }

    /// `player` sets where what building `h` trains goes.
    pub(crate) fn set_rally(&mut self, player: PlayerId, h: StructureHandle, at: FxVec2) -> bool {
        if self.structures.owned(h, player).is_none() {
            return false;
        }
        self.production.set_rally(h, at);
        true
    }

    /// The slot of building `h` if it is `player`'s and finished.
    fn ready(&self, player: PlayerId, h: StructureHandle) -> Option<usize> {
        self.structures
            .owned(h, player)
            .filter(|&i| self.structures.finished(i))
    }

    /// Regiment slot `r` was given another order: it stops attacking or
    /// building, and stops gathering. Every order that sets a regiment to
    /// work calls this first.
    pub(crate) fn stop_work(&mut self, r: usize) {
        self.structures.stop(r);
        self.economy.stop(r);
    }

    /// The parties in `out` leave the field: their regiments, then their
    /// buildings.
    pub(crate) fn eliminate(&mut self, out: u8, regs: &mut Regiments, nav: &mut Nav) {
        for i in 0..regs.alive.len() {
            if regs.alive[i] && out & (1 << regs.owner[i]) != 0 {
                self.stop_work(i);
                regs.despawn(regs.handle(i));
            }
        }
        let gone: Vec<StructureHandle> = (self.structures.iter())
            .filter(|s| out & (1 << s.owner) != 0)
            .map(|s| s.handle)
            .collect();
        self.structures.despawn_all(&gone, nav, regs);
    }

    /// The "structures" part of the state hash: the unit table and the
    /// buildings.
    pub(crate) fn hash_structures(&self, regs: &Regiments, h: &mut StateHasher) {
        h.u32(self.units.len() as u32);
        for u in &self.units {
            u.hash_into(h);
        }
        self.structures.hash_into(regs, h);
    }
}
