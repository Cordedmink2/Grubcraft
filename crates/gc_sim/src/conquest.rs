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

use crate::blueprints::Blueprints;
use crate::clearing::{ClearStyle, Clearing, Pledge};
use crate::economy::{Bank, Economy, NodeSetup, TradePost};
use crate::honor::Honor;
use crate::nav::Nav;
use crate::powers::{Meter, PowerStats, Powers};
use crate::production::Production;
use crate::regiments::Regiments;
use crate::stances::Stances;
use crate::structures::{
    Breaker, BuildingId, BuildingStats, CANCEL_REFUND_PERCENT, GATE, GROWN, LINE, NEEDS_POWER,
    Refusal, Role, StructureHandle, Structures, Work,
};
use crate::terrain::{Growth, Terrain};
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
    /// A worker that builds and mends but does not gather (the Choir's
    /// Tuners: their nodes are tuned, see [`crate::power`]).
    pub builds_only: bool,
    /// A trader (the Concord's Caravan): ordered to a trade post
    /// ([`crate::Command::Trade`]) it shuttles cargo between its owner's
    /// nearest main building and the post (see [`crate::economy`]). What a
    /// delivery pays at a full-rate post: this many Grub and
    /// [`ConquestUnit::trade_tenths`] tenths of a Grub more for each whole
    /// tile of route. Both 0 for a unit that doesn't trade.
    pub trade_base: u16,
    pub trade_tenths: u16,
    /// `DROP_OFF_*` flags that no building of its owner's faction has (the
    /// Concord's buildings take no Grub): a worker that doesn't burrow is
    /// refused a node whose yield it could never bring home. 0 where
    /// every drop-off exists.
    pub cannot_bank: u16,
    /// How it clears land, if it is a worker (see [`crate::clearing`]).
    pub clearing: ClearStyle,
    /// It can man a wall (see [`crate::manning`]): foot soldiers and
    /// archers, not workers, riders, engines or beasts.
    pub mans_walls: bool,
}

impl ConquestUnit {
    /// Whether it trades.
    pub fn trades(&self) -> bool {
        self.trade_base > 0 || self.trade_tenths > 0
    }

    fn hash_into(&self, h: &mut StateHasher) {
        h.u8(self.breaker as u8);
        h.u8(u8::from(self.worker));
        h.u8(self.supply);
        h.u16(self.cost_grub);
        h.u16(self.cost_marrow);
        h.u16(self.train_ticks);
        h.u16(self.trained_at.unwrap_or(u16::MAX));
        h.u8(self.tier);
        h.u8(u8::from(self.hatches)
            | u8::from(self.consumed) << 1
            | u8::from(self.burrows) << 2
            | u8::from(self.builds_only) << 3
            | u8::from(self.mans_walls) << 4);
        h.u16(self.trade_base);
        h.u16(self.trade_tenths);
        h.u16(self.cannot_bank);
        let c = self.clearing;
        for i in 0..3 {
            h.u16(c.pace[i]);
            h.u8(c.pays[i]);
        }
        h.u8(u8::from(c.burns));
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
    /// The map's neutral trade posts (its Villages, Towns and Bazaar), by
    /// where they stand, in tiles: where caravans trade at full rate (see
    /// [`crate::economy`]). Orders name one by its index here.
    pub posts: &'a [FxVec2],
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
    /// The faction meters' powers (see [`crate::powers`]), indexed by
    /// [`crate::Command::UsePower`]'s `power`.
    pub powers: &'a [PowerStats],
    /// Each player's faction meter: what they earn, and the only meter
    /// whose powers they may use.
    pub meters: [Meter; MAX_PARTIES],
}

impl ConquestSetup<'_> {
    /// Warfield sizing from docs/10-architecture.md.
    pub const STRUCTURES: u32 = 2048;
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
            posts: &[],
            start: [(0, 0); MAX_PARTIES],
            gather_percent: Self::STANDARD,
            build_percent: Self::STANDARD,
            victories: Victories::default(),
            heart: None,
            powers: &[],
            meters: [Meter::NotBuilt; MAX_PARTIES],
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
    pub(crate) powers: Powers,
    pub(crate) meters: [Meter; MAX_PARTIES],
    pub(crate) honor: Honor,
    /// Workers clearing land (see [`crate::clearing`]).
    pub(crate) clearing: Clearing,
    /// Walls planned ahead (see [`crate::blueprints`]).
    pub(crate) blueprints: Blueprints,
    /// Regiments on the walls (see [`crate::manning`]).
    pub(crate) manning: crate::manning::Manning,
}

/// Whether the footprints `a` and `b` (`[x, y, w, d]`) share a tile.
pub(crate) fn overlaps(a: [i32; 4], b: [i32; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

/// Whether anything grows on the footprint `[x, y, w, d]` that must be
/// cleared before a building goes up there.
pub(crate) fn overgrown(terrain: &Terrain, [x, y, w, d]: [i32; 4]) -> bool {
    (y..y + d).any(|ty| (x..x + w).any(|tx| terrain.growth(tx, ty) != Growth::Open))
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
            powers: Powers::new(setup.powers),
            meters: setup.meters,
            honor: Honor::default(),
            clearing: Clearing::new(regiments, setup.build_percent),
            blueprints: Blueprints::default(),
            manning: crate::manning::Manning::new(setup.structures as usize),
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

    /// One tick: sieges and builders, then root-veins, power, chords,
    /// Resonance and larvae, then gathering and tuning, then training.
    pub(crate) fn update(
        &mut self,
        regs: &mut Regiments,
        stats: &[UnitStats],
        nav: &mut Nav,
        stances: &Stances,
        out: u8,
        air: crate::Air,
    ) {
        self.structures.update(regs, stats, &self.units, nav, air);
        self.structures.link_veins();
        self.structures.regrow();
        self.structures.wither(nav, regs);
        self.structures.link_power();
        // After the power, which the Choir's walls read.
        self.structures.work_gates(regs, nav, stances);
        // After every building that fell or lost power this tick.
        self.man_walls(regs, stats, nav);
        self.structures.charge_resonance();
        self.structures.grow_larvae();
        self.economy
            .update(regs, stats, &self.units, &self.structures, stances, out);
        let (units, economy) = (&self.units, &self.economy);
        self.production.update(&self.structures, nav, units, || {
            economy.supply_in_use(units, regs)
        });
        // Last, so every part of the tick saw the same haste.
        self.structures.wear_haste();
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
            resonance: self.structures.resonance(p),
            chords: (self.structures.chord_count.get(usize::from(p))).map_or(0, |&c| u32::from(c)),
            honor: self.honor.points(p),
        }
    }

    /// Whether regiment slot `r` could be sent to gather at `node`: a
    /// worker that gathers, a node that is there, and (unless it burrows)
    /// a faction with a drop-off for what the node yields: else it would
    /// fill its load and wait for ever.
    pub(crate) fn can_gather(&self, regs: &Regiments, r: usize, node: u16) -> bool {
        let unit = self.unit(regs.unit[r]);
        let Some(resource) = self.economy.resource_at(node) else {
            return false;
        };
        unit.worker
            && !unit.builds_only
            && (unit.burrows || unit.cannot_bank & resource.drop_off() == 0)
    }

    /// Worker slot `r` is sent to gather at `node`, if it can: one sent
    /// again to the node it works keeps on.
    pub(crate) fn order_gather(&mut self, regs: &mut Regiments, r: usize, node: u16) -> bool {
        if !self.can_gather(regs, r, node) {
            return false;
        }
        if self.economy.work_of(regs, r) != Some(node) {
            self.stop_work(r);
            self.economy.gather(regs, r, node);
        }
        true
    }

    /// Whether `post` is a trade post for `player`'s caravans now.
    pub(crate) fn is_trade_post(
        &self,
        player: PlayerId,
        stances: &Stances,
        post: TradePost,
    ) -> bool {
        (self.economy)
            .rate_of(&self.structures, stances, player, post)
            .is_some()
    }

    /// Caravan slot `r` is sent to trade at `post`, if it can be: a unit
    /// that trades, and a post that pays its owner. One sent again to the
    /// post it works keeps on.
    pub(crate) fn order_trade(
        &mut self,
        regs: &mut Regiments,
        stances: &Stances,
        r: usize,
        post: TradePost,
    ) -> bool {
        let player = regs.owner[r];
        if !self.unit(regs.unit[r]).trades() || !self.is_trade_post(player, stances, post) {
            return false;
        }
        if self.economy.trade_of(regs, r) != Some(post) {
            self.stop_work(r);
            self.economy.trade(regs, r, post);
        }
        true
    }

    /// Whether a building of type `building` may go up with its north-west
    /// tile at `tile`, turned `turn` quarter turns (see `Structures::check`),
    /// once the buildings in slots `replaced` are gone (see
    /// [`Conquest::replaces`]).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check(
        &self,
        replaced: &[usize],
        building: BuildingId,
        tile: (i32, i32),
        turn: u8,
        nav: &Nav,
        regs: &Regiments,
        terrain: &Terrain,
    ) -> Result<(), Refusal> {
        let gone: Vec<[i32; 4]> = replaced
            .iter()
            .map(|&i| self.structures.footprint(i))
            .collect();
        let spare = |x, y| {
            let wall = gone.iter().any(|&f| overlaps(f, [x, y, 1, 1]));
            u8::from(terrain.growth(x, y) == Growth::Thicket) + u8::from(wall)
        };
        self.structures.check(
            building,
            tile,
            turn,
            nav,
            regs,
            self.economy.node_positions(),
            spare,
        )?;
        // Ground paid for and still being cleared is spoken for.
        let foot = self.footprint(building, tile, turn);
        if self
            .clearing
            .pledges()
            .any(|p| overlaps(foot, self.footprint(p.building, p.tile, p.turn)))
        {
            return Err(Refusal::Built);
        }
        Ok(())
    }

    /// The tiles a building of type `building` covers with its north-west
    /// tile at `tile`, turned `turn` quarter turns, as `[x, y, width, depth]`.
    fn footprint(&self, building: BuildingId, tile: (i32, i32), turn: u8) -> [i32; 4] {
        self.structures.buildings()[usize::from(building)].footprint(tile, turn)
    }

    /// Whether `player` could have a worker lay a building of type
    /// `building` at `tile`, turned `turn` quarter turns, now: placement,
    /// tier and price.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn can_lay(
        &self,
        player: PlayerId,
        building: BuildingId,
        tile: (i32, i32),
        turn: u8,
        nav: &Nav,
        regs: &Regiments,
        terrain: &Terrain,
    ) -> Result<(), Refusal> {
        self.lay_plan(player, building, tile, turn, nav, regs, terrain)
            .map(drop)
    }

    /// [`Conquest::can_lay`], with the slots of the buildings laying it
    /// would replace (see [`Conquest::replaces`]).
    #[allow(clippy::too_many_arguments)]
    fn lay_plan(
        &self,
        player: PlayerId,
        building: BuildingId,
        tile: (i32, i32),
        turn: u8,
        nav: &Nav,
        regs: &Regiments,
        terrain: &Terrain,
    ) -> Result<Vec<usize>, Refusal> {
        self.may_lay(player, building)?;
        let b = self.structures.buildings()[usize::from(building)];
        if b.flags & LINE != 0 {
            // Drawn, not laid: see `Conquest::lay_wall`.
            return Err(Refusal::Unknown);
        }
        let foot = b.footprint(tile, turn);
        let replaced = self.replaces(player, b, foot);
        self.check(&replaced, building, tile, turn, nav, regs, terrain)?;
        if !replaced.is_empty() && overgrown(terrain, foot) {
            // The wall would stand open while the rest was cleared: clear
            // it first.
            return Err(Refusal::Overgrown);
        }
        self.powered(player, building, tile, turn)?;
        Ok(replaced)
    }

    /// The slots of the buildings a building `b` that `player` lays on
    /// `foot` replaces: `player`'s drawn wall tiles under
    /// it, if it is a gate.
    fn replaces(&self, player: PlayerId, b: BuildingStats, foot: [i32; 4]) -> Vec<usize> {
        if b.flags & GATE == 0 {
            return Vec::new();
        }
        self.structures.own_in(player, LINE, foot).collect()
    }

    /// Whether a building of type `building` `player` lays at `tile` has
    /// the root-veins it needs under it, if it needs them (a grown wall).
    pub(crate) fn rooted(
        &self,
        player: PlayerId,
        building: BuildingId,
        tile: (i32, i32),
        turn: u8,
    ) -> Result<(), Refusal> {
        let b = self.structures.buildings()[usize::from(building)];
        let foot = b.footprint(tile, turn);
        if b.flags & GROWN != 0 && !self.structures.veins_reach(player, foot) {
            return Err(Refusal::NoVeins);
        }
        Ok(())
    }

    /// Whether a building of type `building` `player` lays at `tile`,
    /// turned `turn`, has the power it needs, if it needs any.
    pub(crate) fn powered(
        &self,
        player: PlayerId,
        building: BuildingId,
        tile: (i32, i32),
        turn: u8,
    ) -> Result<(), Refusal> {
        let b = self.structures.buildings()[usize::from(building)];
        let foot = b.footprint(tile, turn);
        if b.flags & NEEDS_POWER != 0 && !self.structures.power_reaches(player, foot) {
            return Err(Refusal::Unpowered);
        }
        Ok(())
    }

    /// Whether `player` may lay a building of type `building` at all: a
    /// building that goes up on its own ground, of a tier the side has.
    pub(crate) fn may_build(
        &self,
        player: PlayerId,
        building: BuildingId,
    ) -> Result<BuildingStats, Refusal> {
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
        Ok(b)
    }

    /// Whether `player` could lay a building of type `building` somewhere
    /// now: its tier and price, without the ground.
    pub(crate) fn may_lay(&self, player: PlayerId, building: BuildingId) -> Result<(), Refusal> {
        let b = self.may_build(player, building)?;
        if !self
            .economy
            .can_afford(player, u32::from(b.cost_grub), u32::from(b.cost_marrow))
        {
            return Err(Refusal::Price);
        }
        Ok(())
    }

    /// Worker slot `r`, `player`'s, lays a building of type `building` at
    /// `tile`, turned `turn` quarter turns, pays for it and goes to build it.
    /// On overgrown ground it pays now and clears the footprint first (see
    /// [`crate::clearing`]).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lay(
        &mut self,
        player: PlayerId,
        r: usize,
        building: BuildingId,
        tile: (i32, i32),
        turn: u8,
        nav: &mut Nav,
        regs: &mut Regiments,
        terrain: &Terrain,
    ) -> bool {
        if !self.unit(regs.unit[r]).worker {
            return false;
        }
        let Ok(replaced) = self.lay_plan(player, building, tile, turn, nav, regs, terrain) else {
            return false;
        };
        let b = self.structures.buildings()[usize::from(building)];
        let foot = b.footprint(tile, turn);
        let (grub, marrow) = (u32::from(b.cost_grub), u32::from(b.cost_marrow));
        if overgrown(terrain, foot) {
            self.stop_work(r);
            self.economy.spend(player, grub, marrow);
            let pledge = Pledge {
                owner: player,
                building,
                tile,
                turn,
                grub,
                marrow,
            };
            self.clearing.start(regs, r, foot, Some(pledge));
            return true;
        }
        // A gate replaces the stretch of its owner's wall it goes on.
        self.structures.remove_all(&replaced, nav, regs);
        let Some(h) = self
            .structures
            .found(player, building, tile, turn, nav, regs)
        else {
            return false;
        };
        self.economy.spend(player, grub, marrow);
        self.stop_work(r);
        self.structures.send(regs, r, h, Work::Build);
        true
    }

    /// Worker slot `r` has cleared the footprint of `pledge`, paid for when
    /// it was ordered: it lays the foundation and goes to build it, or, if
    /// the ground was taken meanwhile, the price goes back.
    pub(crate) fn lay_pledged(
        &mut self,
        r: usize,
        pledge: Pledge,
        nav: &mut Nav,
        regs: &mut Regiments,
        terrain: &Terrain,
    ) {
        let Pledge {
            owner,
            building,
            tile,
            turn,
            ..
        } = pledge;
        let b = self.structures.buildings()[usize::from(building)];
        let foot = b.footprint(tile, turn);
        let powered = b.flags & NEEDS_POWER == 0 || self.structures.power_reaches(owner, foot);
        let laid = (powered
            && !overgrown(terrain, foot)
            && self
                .check(&[], building, tile, turn, nav, regs, terrain)
                .is_ok())
        .then(|| {
            self.structures
                .found(owner, building, tile, turn, nav, regs)
        })
        .flatten();
        match laid {
            Some(h) => {
                self.stop_work(r);
                self.structures.send(regs, r, h, Work::Build);
            }
            None => self.economy.refund(owner, pledge.grub, pledge.marrow),
        }
    }

    /// Worker slot `r` clears what grows on `area` (see
    /// [`crate::clearing`]). Refused for a unit that isn't a worker, and
    /// for an area with nothing to clear.
    pub(crate) fn clear(
        &mut self,
        regs: &mut Regiments,
        r: usize,
        area: [i32; 4],
        terrain: &Terrain,
    ) -> bool {
        if !self.unit(regs.unit[r]).worker || !overgrown(terrain, area) {
            return false;
        }
        self.stop_work(r);
        self.clearing.start(regs, r, area, None);
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
        if self.structures.is_line(i) {
            // A wall goes up a level a stretch at a time (see
            // [`Conquest::wall_level`]).
            return Err(Refusal::Unknown);
        }
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
    /// building, and stops gathering or trading. Every order that sets a regiment to
    /// work calls this first.
    pub(crate) fn stop_work(&mut self, r: usize) {
        self.blueprints.forget(r);
        self.structures.stop(r);
        self.economy.stop(r);
        // A building paid for but not yet laid is called off in full.
        if let Some(p) = self.clearing.stop(r) {
            self.economy.refund(p.owner, p.grub, p.marrow);
        }
    }

    /// Whether regiment slot `r` has a job: building, gathering, trading
    /// or clearing (working the plans counts only while it is on a tile).
    pub(crate) fn has_job(&self, regs: &Regiments, r: usize) -> bool {
        self.structures.job_of(regs, r).is_some()
            || self.economy.work_of(regs, r).is_some()
            || self.clearing.job_of(regs, r).is_some()
    }

    /// The parties in `out` leave the field: their regiments, then their
    /// buildings.
    pub(crate) fn eliminate(&mut self, out: u8, regs: &mut Regiments, nav: &mut Nav) {
        for i in 0..regs.alive.len() {
            if regs.alive[i] && out & (1 << regs.owner[i]) != 0 {
                self.stop_work(i);
                self.economy.forget(i);
                regs.despawn(regs.handle(i));
            }
        }
        let gone: Vec<StructureHandle> = (self.structures.iter())
            .filter(|s| out & (1 << s.owner) != 0)
            .map(|s| s.handle)
            .collect();
        self.structures.despawn_all(&gone, nav, regs);
        self.blueprints.drop_sides(out);
    }

    /// The "structures" part of the state hash: the unit table and the
    /// buildings.
    pub(crate) fn hash_structures(&self, regs: &Regiments, h: &mut StateHasher) {
        h.u32(self.units.len() as u32);
        for u in &self.units {
            u.hash_into(h);
        }
        self.structures.hash_into(regs, h);
        self.powers.hash_into(h);
        for m in self.meters {
            m.hash_into(h);
        }
        self.honor.hash_into(h);
        self.blueprints.hash_into(regs, h);
        self.manning.hash_into(regs, h);
    }
}
