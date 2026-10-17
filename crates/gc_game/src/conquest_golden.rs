//! The Conquest golden replay (`replays/conquest-ribcage-1v1.gcr`): five
//! minutes on Ribcage Pass, set up as the lobby sets up Conquest, between
//! two scripted sides that use every Conquest order. Each gathers, trains
//! workers (and takes one back off the queue), lays a building that trains
//! soldiers and helps build it, lays a second and calls it off, upgrades
//! its main building, trains soldiers, rallies them and sends them at the
//! other side's main building; at the end the host takes a building away.
//!
//! The test below plays it through a recorder and checks it plays back
//! alike after a trip through the file format. To record the golden again
//! (only when the script or the data change what it does; a simulation
//! change that only moves the hashes is blessed instead):
//!
//! ```text
//! GOLDEN=conquest-ribcage-1v1 cargo test --release -p gc_game record_golden_replays -- --ignored --nocapture
//! ```

use gc_data::{Faction, GameData};
use gc_fixed::{Angle, FxVec2};
use gc_replay::{Recorder, Replay};
use gc_sim::structures::Role;
use gc_sim::{
    BuildingId, Capacities, Command, MatchSetup, PlayerCommand, PlayerId, RegimentHandle, Resource,
    TICKS_PER_SECOND, UnitId, Victories, World,
};
use gc_ui::lobby::Tempo;

use crate::buildings::Table;
use crate::conquest::{self, Foot};

pub const NAME: &str = "conquest-ribcage-1v1";
const SEED: u64 = 0x00C0_9E57;
const MINUTES: u32 = 5;
const FACTIONS: [Faction; 2] = [Faction::EmberClans, Faction::CarrionConcord];

/// When each side does what, in ticks.
const GATHER_AT: u32 = 1;
const TRAIN_AT: u32 = 2;
const UNTRAIN_AT: u32 = 41;
const LAY_FROM: u32 = 20 * TICKS_PER_SECOND;
const SPARE_FROM: u32 = 80 * TICKS_PER_SECOND;
const UPGRADE_FROM: u32 = 120 * TICKS_PER_SECOND;
const CHARGE_AT: u32 = 250 * TICKS_PER_SECOND;
const REMOVE_AT: u32 = 295 * TICKS_PER_SECOND;
/// Soldiers and workers each side trains up to.
const FIGHTERS: usize = 6;
const WORKERS: usize = 8;

/// What a faction's script plays with: its main building and what it
/// upgrades to, its worker, a building that trains soldiers and the
/// soldier, a house, and the cheapest other building (laid to be called
/// off).
#[derive(Clone, Copy, Debug)]
struct Kit {
    main: BuildingId,
    keep: Option<BuildingId>,
    worker: UnitId,
    barracks: BuildingId,
    fighter: UnitId,
    house: BuildingId,
    spare: BuildingId,
    /// What a worker, a soldier and the upgrade cost: Grub, Marrow and
    /// supply.
    worker_cost: [u32; 3],
    fighter_cost: [u32; 3],
    keep_cost: [u32; 3],
}

impl Kit {
    fn of(data: &GameData, table: &Table, faction: Faction) -> Kit {
        let ids = |f: &dyn Fn(usize) -> bool| -> Vec<u16> {
            (0..data.buildings.len())
                .filter(|&i| data.buildings[i].faction() == faction && f(i))
                .map(|i| i as u16)
                .collect()
        };
        let stats = |b: u16| table.stats[usize::from(b)];
        let main = ids(&|i| table.stats[i].role == Role::Main && table.stats[i].tier <= 1)[0];
        let trains = |b: u16, worker: bool| {
            data.conquest_units
                .iter()
                .position(|u| u.trained_at == b && u.worker() == worker && u.tier <= 1)
                .map(|u| u as UnitId)
        };
        let worker = trains(main, true).expect("the main building trains a worker");
        let plain = ids(&|i| {
            let s = table.stats[i];
            s.role != Role::Main && s.upgrades_from.is_none() && s.tier <= 1
        });
        let barracks = *plain
            .iter()
            .find(|&&b| trains(b, false).is_some())
            .expect("a tier-1 building trains soldiers");
        let house = *plain
            .iter()
            .filter(|&&b| stats(b).role == Role::Housing)
            .min_by_key(|&&b| (stats(b).cost_grub, b))
            .expect("a house");
        let spare = *plain
            .iter()
            .filter(|&&b| b != barracks && b != house)
            .min_by_key(|&&b| (stats(b).cost_grub, b))
            .expect("another tier-1 building");
        let keep = ids(&|i| table.stats[i].upgrades_from == Some(main))
            .first()
            .copied();
        let fighter = trains(barracks, false).unwrap_or(worker);
        let unit_cost = |u: UnitId| {
            let u = &data.conquest_units[usize::from(u)];
            [
                u32::from(u.cost_grub),
                u32::from(u.cost_marrow),
                u32::from(u.supply),
            ]
        };
        Kit {
            main,
            keep,
            worker,
            barracks,
            fighter,
            house,
            spare,
            worker_cost: unit_cost(worker),
            fighter_cost: unit_cost(fighter),
            keep_cost: keep.map_or([u32::MAX; 3], |b| {
                let s = stats(b);
                [u32::from(s.cost_grub), u32::from(s.cost_marrow), 0]
            }),
        }
    }
}

/// What one side has done so far.
#[derive(Default)]
struct Side {
    helped: bool,
    called_off: bool,
    rallied: bool,
    charged: bool,
}

/// The match: its world, its recorder and both sides' scripts.
pub struct Match {
    pub world: World,
    rec: Recorder,
    kits: [Kit; 2],
    sides: [Side; 2],
    starts: Vec<(i32, i32)>,
}

impl Match {
    pub fn new() -> Match {
        let data = crate::data::load();
        let table = Table::new(&data);
        let map = conquest::make_map(0);
        let units = conquest::unit_stats(&data);
        let nav = map.nav_costs();
        let setup = MatchSetup {
            seed: SEED,
            capacities: Capacities::default(),
            units: &units,
            map_tiles: map.tiles,
            nav: &nav,
            teams: &[0, 1],
            ground: map.ground(),
            champions: false,
        };
        let plan = conquest::plan(
            &table,
            &data,
            &map,
            &FACTIONS,
            Tempo::Swift,
            Victories::BOTH,
        );
        let mut world = World::new(&setup);
        assert!(world.conquest(&plan.setup()));
        let rec = Recorder::new(&setup, &world, String::new()).conquest(plan);
        let opening = conquest::opening(&data, &table, &map, &world, &FACTIONS);
        let mut m = Match {
            kits: FACTIONS.map(|f| Kit::of(&data, &table, f)),
            sides: Default::default(),
            starts: map.starts(),
            world,
            rec,
        };
        let stats = m.rec.tick(&mut m.world, &opening);
        assert_eq!(stats.commands_applied as usize, opening.len());
        m
    }

    /// Play to the end of the script, or `ticks` more.
    pub fn play(&mut self, ticks: u32) {
        let end =
            (self.world.tick_count().saturating_add(ticks)).min(MINUTES * 60 * TICKS_PER_SECOND);
        let mut out = Vec::new();
        while self.world.tick_count() < end {
            out.clear();
            for p in 0..2 {
                self.script(p, &mut out);
            }
            let stats = self.rec.tick(&mut self.world, &out);
            // The script only gives orders that can be carried out.
            assert_eq!(
                stats.commands_rejected,
                0,
                "at tick {}: {out:?}",
                self.world.tick_count() - 1
            );
        }
    }

    pub fn finish(self) -> Replay {
        let mut replay = self.rec.finish();
        replay.note = format!(
            "{NAME}: Conquest recorded from gc_game's conquest_golden.rs, seed {SEED:#x}, \
             Ribcage Pass; {:?} v {:?}, scripted, {MINUTES} minutes",
            FACTIONS[0], FACTIONS[1]
        );
        replay
    }

    /// Side `p`'s buildings of type `b` (or any type, for `None`).
    fn own(&self, p: PlayerId, b: Option<BuildingId>) -> Vec<gc_sim::structures::StructureView> {
        self.world
            .structures()
            .map(|s| {
                s.iter()
                    .filter(|v| v.owner == p && b.is_none_or(|b| v.building == b))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Side `p`'s main building, kept or not.
    fn main_of(&self, p: PlayerId) -> Option<gc_sim::structures::StructureView> {
        let k = self.kits[usize::from(p)];
        self.own(p, None)
            .into_iter()
            .find(|v| v.building == k.main || Some(v.building) == k.keep)
    }

    fn workers(&self, p: PlayerId) -> Vec<RegimentHandle> {
        let w = self.kits[usize::from(p)].worker;
        self.world
            .regiments
            .iter()
            .filter(|r| r.owner == p && r.unit == w)
            .map(|r| r.handle)
            .collect()
    }

    /// Where side `p` can lay building `b` near its main building, the
    /// nearest first.
    fn site(&self, p: PlayerId, b: BuildingId) -> Option<(i16, i16)> {
        let main = self.main_of(p)?;
        let foot: Foot = [
            main.tile.0,
            main.tile.1,
            i32::from(main.width),
            i32::from(main.depth),
        ];
        conquest::ring(foot, 3)
            .find(|&t| self.world.can_lay(p, b, t).is_ok())
            .map(|(x, y)| (x as i16, y as i16))
    }

    /// The nodes of `resource`, nearest side `p`'s start first.
    fn nodes_near(&self, p: PlayerId, resource: Resource) -> Vec<u16> {
        let (sx, sy) = self.starts[usize::from(p)];
        let start = FxVec2::from_ints(sx, sy);
        let mut nodes: Vec<(u64, u16)> = self
            .world
            .economy()
            .into_iter()
            .flat_map(gc_sim::Economy::nodes)
            .filter(|n| n.resource == resource && n.left > 0)
            .map(|n| ((n.at - start).length_sq_raw(), n.index))
            .collect();
        nodes.sort_unstable();
        nodes.into_iter().map(|(_, i)| i).collect()
    }

    fn script(&mut self, p: PlayerId, out: &mut Vec<PlayerCommand>) {
        let t = self.world.tick_count();
        let k = self.kits[usize::from(p)];
        let order = |command| PlayerCommand { player: p, command };
        let workers = self.workers(p);
        let main = self.main_of(p);
        let barracks = self.own(p, Some(k.barracks)).first().copied();
        let spare = self.own(p, Some(k.spare)).first().copied();
        // The first two workers build; the rest gather Grub, the last of
        // them Marrow, and a new or idle one goes back to it.
        let grub = self.nodes_near(p, Resource::Grub);
        let marrow = self.nodes_near(p, Resource::Marrow);
        let builders = 2.min(workers.len().saturating_sub(1));
        let bank = self.world.bank(p).unwrap_or_default();
        let affords = |[grub, marrow, supply]: [u32; 3]| {
            bank.grub >= grub
                && bank.marrow >= marrow
                && bank.supply_used + supply <= bank.supply_cap
        };
        if t == GATHER_AT || t.is_multiple_of(10 * TICKS_PER_SECOND) {
            for (i, &w) in workers.iter().enumerate() {
                let building =
                    i < builders && t >= LAY_FROM && !barracks.is_some_and(|b| b.finished())
                        || i == 0 && self.own(p, Some(k.house)).iter().any(|h| !h.finished());
                if building || self.world.gathers_at(w).is_some() {
                    continue;
                }
                let node = if i == 3 || i == 4 {
                    marrow.first()
                } else {
                    grub.get(i % 2)
                };
                if let Some(&node) = node {
                    out.push(order(Command::Gather { regiment: w, node }));
                }
            }
        }
        if let Some(main) = main {
            let structure = main.handle;
            if t == TRAIN_AT {
                let at = self.ahead(p, 4);
                out.push(order(Command::Rally { structure, at }));
            }
            if t == TRAIN_AT || t == UNTRAIN_AT - 1 {
                out.push(order(Command::Train {
                    structure,
                    unit: k.worker,
                }));
            }
            if t == UNTRAIN_AT {
                out.push(order(Command::Untrain { structure }));
            }
            let queue = self
                .world
                .production()
                .map_or(0, |q| q.queue_of(structure).len());
            if t > UNTRAIN_AT
                && t < UPGRADE_FROM
                && queue == 0
                && workers.len() < WORKERS
                && affords(k.worker_cost)
                && t.is_multiple_of(TICKS_PER_SECOND)
            {
                out.push(order(Command::Train {
                    structure,
                    unit: k.worker,
                }));
            }
            let upgrading = main.upgrading.is_some() || Some(main.building) == k.keep;
            if t >= UPGRADE_FROM
                && !upgrading
                && affords(k.keep_cost)
                && t.is_multiple_of(TICKS_PER_SECOND)
            {
                out.push(order(Command::Upgrade { structure }));
            }
        }
        // The barracks: laid by the first worker, helped by the second,
        // then training soldiers who rally ahead.
        match barracks {
            None if t >= LAY_FROM && t.is_multiple_of(TICKS_PER_SECOND) => {
                if let (Some(&w), Some(tile)) = (workers.first(), self.site(p, k.barracks)) {
                    out.push(order(Command::Build {
                        regiment: w,
                        building: k.barracks,
                        tile,
                    }));
                }
            }
            Some(b) if !b.finished() && !self.sides[usize::from(p)].helped => {
                if let Some(&w) = workers.get(1) {
                    out.push(order(Command::Construct {
                        regiment: w,
                        structure: b.handle,
                    }));
                    self.sides[usize::from(p)].helped = true;
                }
            }
            Some(b) if b.finished() => {
                let side = &self.sides[usize::from(p)];
                if !side.rallied {
                    let at = self.ahead(p, 16);
                    out.push(order(Command::Rally {
                        structure: b.handle,
                        at,
                    }));
                    self.sides[usize::from(p)].rallied = true;
                }
                let queue = self
                    .world
                    .production()
                    .map_or(0, |q| q.queue_of(b.handle).len());
                // Saving up for the upgrade comes first.
                let saving = t >= UPGRADE_FROM
                    && main.is_some_and(|m| m.upgrading.is_none() && Some(m.building) != k.keep);
                if self.count(p, k.fighter) + queue < FIGHTERS
                    && queue == 0
                    && !saving
                    && affords(k.fighter_cost)
                    && t.is_multiple_of(TICKS_PER_SECOND)
                {
                    out.push(order(Command::Train {
                        structure: b.handle,
                        unit: k.fighter,
                    }));
                }
                // A house whenever supply runs short, laid by the first
                // worker.
                let one_going_up = self.own(p, Some(k.house)).iter().any(|h| !h.finished());
                if bank.supply_cap < bank.supply_used + 6
                    && !one_going_up
                    && t.is_multiple_of(TICKS_PER_SECOND)
                    && let (Some(&w), Some(tile)) = (workers.first(), self.site(p, k.house))
                {
                    out.push(order(Command::Build {
                        regiment: w,
                        building: k.house,
                        tile,
                    }));
                }
            }
            _ => {}
        }
        // The spare building: laid by the third worker and called off.
        match spare {
            None if t >= SPARE_FROM
                && !self.sides[usize::from(p)].called_off
                && t.is_multiple_of(TICKS_PER_SECOND) =>
            {
                if let (Some(&w), Some(tile)) = (workers.get(builders), self.site(p, k.spare)) {
                    out.push(order(Command::Build {
                        regiment: w,
                        building: k.spare,
                        tile,
                    }));
                }
            }
            Some(s) if !self.sides[usize::from(p)].called_off => {
                out.push(order(Command::CallOff {
                    structure: s.handle,
                }));
                self.sides[usize::from(p)].called_off = true;
            }
            _ => {}
        }
        // Every soldier at the other side's main building.
        if t >= CHARGE_AT && !self.sides[usize::from(p)].charged {
            if let Some(target) = self.main_of(1 - p) {
                for r in self.world.regiments.iter() {
                    if r.owner == p && r.unit == k.fighter && k.fighter != k.worker {
                        out.push(order(Command::AttackStructure {
                            regiment: r.handle,
                            structure: target.handle,
                        }));
                    }
                }
            }
            self.sides[usize::from(p)].charged = true;
        }
        // The host takes the first side's barracks away.
        if t == REMOVE_AT
            && p == 0
            && let Some(b) = barracks
        {
            out.push(order(Command::RemoveStructure {
                structure: b.handle,
            }));
        }
    }

    /// A point `tiles` from side `p`'s start towards the other's.
    fn ahead(&self, p: PlayerId, tiles: i32) -> FxVec2 {
        let (a, b) = (self.starts[usize::from(p)], self.starts[usize::from(1 - p)]);
        let from = FxVec2::from_ints(a.0, a.1);
        let to = FxVec2::from_ints(b.0, b.1);
        let along = Angle::of_vec(to - from).to_vec();
        from + along * gc_fixed::Fx::from_int(tiles)
    }

    /// How many regiments of type `unit` side `p` has.
    fn count(&self, p: PlayerId, unit: UnitId) -> usize {
        self.world
            .regiments
            .iter()
            .filter(|r| r.owner == p && r.unit == unit)
            .count()
    }
}

/// The golden as recorded.
pub fn record() -> Replay {
    let mut m = Match::new();
    m.play(u32::MAX);
    m.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scripted_conquest_match_plays_back_alike() {
        let mut m = Match::new();
        m.play(u32::MAX);
        for p in 0..2u8 {
            let k = m.kits[usize::from(p)];
            let start = conquest::start_stores(FACTIONS[usize::from(p)]);
            let bank = m.world.bank(p).unwrap();
            assert!(
                m.count(p, k.fighter) >= 3,
                "side {p} trained soldiers: {k:?}"
            );
            assert!(m.count(p, k.worker) > 4, "side {p} trained workers: {k:?}");
            assert!(bank.grub + bank.marrow > 0 || start.0 > 0, "side {p}");
            let main = m.main_of(p).expect("its main building stands");
            if let Some(keep) = k.keep {
                assert!(
                    main.building == keep || main.upgrading.is_some(),
                    "side {p} upgraded its main building"
                );
            }
            assert!(m.own(p, Some(k.spare)).is_empty(), "side {p} called off");
            assert!(m.sides[usize::from(p)].helped && m.sides[usize::from(p)].called_off);
        }
        assert!(m.own(0, Some(m.kits[0].barracks)).is_empty(), "removed");
        let played = m.world.tick_count();
        let replay = m.finish();
        let back = Replay::decode(&replay.encode()).expect("it reads back");
        assert!(back.conquest.is_some());
        let world = back.verify().expect("it plays back alike");
        assert_eq!(world.tick_count(), played);
        assert_eq!(back.first_difference(&replay), None);
    }
}
