//! What a Conquest mind knows about its own faction before the match
//! starts, the way a player knows their faction's build menu: its building
//! and unit types, and where the map's starts are. The game builds a
//! [`Kit`] from the baked data and the map; the simulation itself knows
//! nothing of factions.

use gc_sim::structures::{BuildingStats, DROP_OFF_GRUB, Role};
use gc_sim::{BuildingId, ConquestUnit, UnitId, World};
use glam::Vec2;

use crate::personality::Faction;

/// One of the faction's building types.
#[derive(Clone, Copy, Debug)]
pub struct KitBuilding {
    pub id: BuildingId,
    pub stats: BuildingStats,
}

/// One of the faction's unit types.
#[derive(Clone, Copy, Debug)]
pub struct KitUnit {
    pub id: UnitId,
    pub unit: ConquestUnit,
    /// A fighting regiment the army is made of: not a worker, a caravan, a
    /// healer or anything else that keeps out of the line.
    pub fighter: bool,
}

/// How a faction's economy works, for the Steward's faction module
/// (docs/18-ai.md, "Steward"). Every faction plays the shared economy
/// (workers carry Grub and Marrow to drop-offs) until its own mechanics
/// reach the simulation; each then gets its own variant here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EconomyStyle {
    #[default]
    Shared,
}

/// A faction's build menu and the map's starts.
#[derive(Clone, Debug)]
pub struct Kit {
    pub faction: Faction,
    pub economy: EconomyStyle,
    pub buildings: Vec<KitBuilding>,
    pub units: Vec<KitUnit>,
    /// The worker types of every faction: anyone can tell a worker when
    /// they see one, and an army doesn't stop to fight them.
    pub all_workers: Vec<UnitId>,
    /// Every start on the map, in tiles (centre of the start tile), in the
    /// map's order. Public knowledge: the map shows them.
    pub starts: Vec<Vec2>,
}

impl Kit {
    pub fn unit(&self, id: UnitId) -> Option<&KitUnit> {
        self.units.iter().find(|u| u.id == id)
    }

    /// Whether `id` is one of this faction's worker types.
    pub fn is_worker(&self, id: UnitId) -> bool {
        self.unit(id).is_some_and(|u| u.unit.worker)
    }

    /// Whether `id` is one of this faction's fighting regiment types.
    pub fn is_fighter(&self, id: UnitId) -> bool {
        self.unit(id).is_some_and(|u| u.fighter)
    }

    /// The tier-1 main building (the one the match starts with).
    pub fn main(&self) -> Option<&KitBuilding> {
        self.buildings
            .iter()
            .filter(|b| b.stats.role == Role::Main && b.stats.upgrades_from.is_none())
            .min_by_key(|b| b.stats.tier)
    }

    /// What the main building `from` upgrades into, if anything.
    pub fn upgrade_of(&self, from: BuildingId) -> Option<&KitBuilding> {
        self.buildings
            .iter()
            .find(|b| b.stats.upgrades_from == Some(from))
    }

    /// The worker type the main building trains.
    pub fn worker(&self) -> Option<&KitUnit> {
        self.units.iter().find(|u| u.unit.worker)
    }

    /// The cheapest building that raises the supply cap, Grub for supply
    /// (main buildings and outposts aside): Farmstead, Pustule, Tenement,
    /// or the Choir's Resonator.
    pub fn housing(&self) -> Option<&KitBuilding> {
        self.buildings
            .iter()
            .filter(|b| {
                b.stats.supply > 0
                    && b.stats.upgrades_from.is_none()
                    && !matches!(b.stats.role, Role::Main | Role::Outpost)
            })
            .min_by_key(|b| {
                (u32::from(b.stats.cost_grub) + 2 * u32::from(b.stats.cost_marrow)) * 100
                    / u32::from(b.stats.supply)
            })
    }

    /// What to put up by a far seep: an outpost hall if the faction has
    /// one, else a second main building.
    pub fn expansion(&self) -> Option<&KitBuilding> {
        let takes_grub = |b: &&KitBuilding| b.stats.flags & DROP_OFF_GRUB != 0;
        self.buildings
            .iter()
            .filter(|b| b.stats.role == Role::Outpost)
            .filter(takes_grub)
            .min_by_key(|b| b.stats.cost_grub)
            .or_else(|| self.main().filter(takes_grub))
    }

    /// The units building type `b` (or anything it grew from) trains.
    pub fn trained_at<'a>(
        &'a self,
        world: &'a World,
        b: BuildingId,
    ) -> impl Iterator<Item = &'a KitUnit> + 'a {
        let table = world.structures();
        self.units.iter().filter(move |u| {
            u.unit
                .trained_at
                .is_some_and(|at| at == b || table.is_some_and(|t| t.grew_from(b, at)))
        })
    }
}
