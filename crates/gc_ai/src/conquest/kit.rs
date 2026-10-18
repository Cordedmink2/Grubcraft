//! What a Conquest mind knows about its own faction before the match
//! starts, the way a player knows their faction's build menu: its building
//! and unit types, and where the map's starts are. The game builds a
//! [`Kit`] from the baked data and the map; the simulation itself knows
//! nothing of factions.

use gc_sim::structures::{BuildingStats, DROP_OFF_GRUB, LARVAE, Role, TRADE_POST, VEINS};
use gc_sim::{BuildingId, ConquestUnit, Resource, UnitId, World};
use glam::Vec2;

use crate::perception::Seen;
use crate::personality::Faction;

/// One of the faction's building types.
#[derive(Clone, Copy, Debug)]
pub struct KitBuilding {
    pub id: BuildingId,
    pub stats: BuildingStats,
}

impl KitBuilding {
    /// What it costs, Marrow counted at twice Grub as the rarer of the
    /// two: for choosing the cheapest of several.
    pub fn price(&self) -> u32 {
        u32::from(self.stats.cost_grub) + 2 * u32::from(self.stats.cost_marrow)
    }
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

/// How a faction's economy works, for the Steward (docs/18-ai.md,
/// "Steward"; docs/03-factions.md for the factions' economies).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EconomyStyle {
    /// Workers carry Grub and Marrow to drop-offs (the Ember Clans and the
    /// Carrion Concord).
    #[default]
    Shared,
    /// Workers burrow in at nodes on the root-veins, which carry what they
    /// gather home; Pustules and Hives spread the veins (the Brood).
    Veins,
    /// Buildings work the nodes near them with no workers, and workers
    /// only build (the Choir's Resonators and Siphons).
    Tuned,
    /// Grub comes by trade: caravans shuttle between the main building and
    /// a trade post, and workers mine Marrow only (the Carrion Concord).
    Caravans,
}

impl EconomyStyle {
    /// The style of a faction whose workers are of type `worker`, and that
    /// has a unit that trades, `caravan`, or none.
    pub fn of(worker: &ConquestUnit, caravan: Option<&ConquestUnit>) -> EconomyStyle {
        if worker.burrows {
            EconomyStyle::Veins
        } else if worker.builds_only {
            EconomyStyle::Tuned
        } else if caravan.is_some() {
            EconomyStyle::Caravans
        } else {
            EconomyStyle::Shared
        }
    }
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
    /// Every unit type of every faction, by id: whose it is and what it
    /// takes of supply. A player knows these by sight, and the Spymaster
    /// sizes an enemy army by them.
    pub all_units: Vec<(Faction, u8)>,
    /// Every start on the map, in tiles (centre of the start tile), in the
    /// map's order. Public knowledge: the map shows them.
    pub starts: Vec<Vec2>,
}

impl Kit {
    /// The supply `regiments` take up, whoever's they are: how big an
    /// army they are.
    pub fn supply_of<'a>(&self, regiments: impl IntoIterator<Item = &'a Seen>) -> f32 {
        regiments
            .into_iter()
            .filter_map(|r| self.all_units.get(usize::from(r.unit)))
            .map(|&(_, supply)| f32::from(supply))
            .sum()
    }

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

    /// The unit type that trades: the Concord's Caravan.
    pub fn caravan(&self) -> Option<&KitUnit> {
        self.units.iter().find(|u| u.unit.trades())
    }

    /// Whether `id` is one of this faction's trading unit types.
    pub fn is_caravan(&self, id: UnitId) -> bool {
        self.unit(id).is_some_and(|u| u.unit.trades())
    }

    /// The building that is a trade post of its owner's own: the Concord's
    /// Waystation.
    pub fn waystation(&self) -> Option<&KitBuilding> {
        self.buildings
            .iter()
            .find(|b| b.stats.flags & TRADE_POST != 0)
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
            .min_by_key(|b| b.price() * 100 / u32::from(b.stats.supply))
    }

    /// What to put up by a far seep: an outpost hall if the faction has
    /// one, else a second main building (for the Brood, a Hive, whose own
    /// veins carry the seep's Grub home). None for the Choir, whose
    /// expansions are tuners ([`Kit::tuner`]).
    pub fn expansion(&self) -> Option<&KitBuilding> {
        let takes_grub = |b: &&KitBuilding| b.stats.flags & (DROP_OFF_GRUB | VEINS) != 0;
        self.buildings
            .iter()
            .filter(|b| b.stats.role == Role::Outpost)
            .filter(takes_grub)
            .min_by_key(|b| b.stats.cost_grub)
            .or_else(|| self.main().filter(takes_grub))
    }

    /// The cheapest building that works nodes of `resource` with no
    /// workers: the Choir's Resonator for seeps, Siphon for veins.
    pub fn tuner(&self, resource: Resource) -> Option<&KitBuilding> {
        self.buildings
            .iter()
            .filter(|b| b.stats.tunes.is_some_and(|(r, _)| r == resource))
            .min_by_key(|b| b.stats.cost_grub)
    }

    /// The cheapest defence tower it can put up at `tier`.
    pub fn tower(&self, tier: u8) -> Option<&KitBuilding> {
        self.buildings
            .iter()
            .filter(|b| b.stats.role == Role::Defence && b.stats.tier <= tier.max(1))
            .filter(|b| b.stats.upgrades_from.is_none())
            .min_by_key(|b| b.price())
    }

    /// The building that adds larvae, main buildings aside: the Brood's
    /// Hatchery Pit.
    pub fn hatchery(&self) -> Option<&KitBuilding> {
        self.buildings
            .iter()
            .find(|b| b.stats.flags & LARVAE != 0 && b.stats.role != Role::Main)
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
