//! What a Conquest match adds to a battle (see [`crate::World::conquest`]):
//! buildings ([`crate::structures`]) and the economy ([`crate::economy`]).

use crate::economy::{Economy, NodeSetup};
use crate::nav::Nav;
use crate::regiments::Regiments;
use crate::structures::{Breaker, BuildingId, BuildingStats, Refusal, Structures};
use crate::{MAX_PARTIES, StateHasher};

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
}

impl ConquestUnit {
    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        h.u8(self.breaker as u8);
        h.u8(u8::from(self.worker));
        h.u8(self.supply);
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
        }
    }
}

/// A Conquest match's own systems, made together by
/// [`crate::World::conquest`].
pub(crate) struct Conquest {
    pub(crate) structures: Structures,
    pub(crate) economy: Economy,
}

impl Conquest {
    pub(crate) fn new(setup: &ConquestSetup, regiments: usize) -> Conquest {
        Conquest {
            structures: Structures::new(setup.buildings, setup.units, setup.structures, regiments),
            economy: Economy::new(setup, regiments),
        }
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

    /// Regiment slot `r` was given another order: it stops attacking any
    /// building and stops gathering. Every order that sets a regiment to
    /// work calls this first.
    pub(crate) fn stop_work(&mut self, r: usize) {
        self.structures.stop(r);
        self.economy.stop(r);
    }
}
