//! The Conquest golden replay (`replays/conquest-ribcage-1v1.gcr`): six
//! minutes on Ribcage Pass, set up as the lobby sets up Conquest, between
//! two scripted sides that use every Conquest order. Each gathers, trains
//! workers (and takes one back off the queue), lays a building that trains
//! soldiers and helps build it, lays a second and calls it off, upgrades
//! its main building, trains soldiers, rallies them and sends them at the
//! other side's main building; at the end the host takes a building away.
//! Each side's host puts up a tower by its main building, and the tower is
//! told to shoot the first enemy regiment it can see in range.
//! The second side also puts up three of the Choir's Resonators in a chord
//! and spends the Resonance it charges on the Choir's powers: Overclock on
//! its main building, Phase Shift for two soldiers, and Overclock on a
//! soldier (the script gives this Concord side the Choir's meter). The
//! first side, the Clans, puts up Ancestor Shrines and spends the Honor
//! they earn on its Edicts: Rice Tax, Forced March for its soldiers, and
//! the Banner of the Ancestors over them. The Concord side's Grub comes by
//! trade: its caravan runs to the nearest trade post (a Trade order) and
//! its Clerks mine only Marrow. Its third worker, once the spare building
//! is called off, clears the overgrown ground nearest its main building (a
//! Clear order, docs/32-changing-land.md).
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
use gc_fixed::{Angle, Fx, FxVec2};
use gc_replay::{Recorder, Replay};
use gc_sim::gates::GateMode;
use gc_sim::structures::{Role, StructureView};
use gc_sim::{
    BuildingId, Capacities, Command, MatchSetup, Meter, PlayerCommand, PlayerId, PowerTarget,
    RegimentHandle, Resource, TICKS_PER_SECOND, TradePost, UnitId, Victories, World,
};
use gc_ui::lobby::Tempo;

use crate::buildings::Table;
use crate::conquest::{self, Foot};

pub const NAME: &str = "conquest-ribcage-1v1";
const SEED: u64 = 0x00C0_9E57;
const MINUTES: u32 = 6;
const FACTIONS: [Faction; 2] = [Faction::EmberClans, Faction::CarrionConcord];

/// When each side does what, in ticks.
const GATHER_AT: u32 = 1;
const TRAIN_AT: u32 = 2;
const UNTRAIN_AT: u32 = 41;
const LAY_FROM: u32 = 20 * TICKS_PER_SECOND;
const SPARE_FROM: u32 = 80 * TICKS_PER_SECOND;
const UPGRADE_FROM: u32 = 120 * TICKS_PER_SECOND;
/// When the third worker clears ground near its main building.
const CLEAR_AT: u32 = 100 * TICKS_PER_SECOND;
/// Early enough that the soldiers fight through and see the other side's
/// main building before the six minutes are up (moved from 150 s when
/// battles were slowed, `gc_sim::combat::BATTLE_PACE_PERCENT`).
const MARCH_AT: u32 = 145 * TICKS_PER_SECOND;
/// How much later side 1 marches.
const MARCH_LATER: u32 = 30 * TICKS_PER_SECOND;
const REMOVE_AT: u32 = (MINUTES * 60 - 5) * TICKS_PER_SECOND;
/// When the second side's chord goes up.
const CHORD_AT: u32 = 10 * TICKS_PER_SECOND;
/// When the first side's Shrines go up, one a tick, and how many.
const SHRINES_AT: u32 = 5 * TICKS_PER_SECOND;
const SHRINES: u8 = 12;
/// When each side's tower goes up.
const TOWER_AT: u32 = 15 * TICKS_PER_SECOND;
/// The quarter turns each side's tower is put up at.
const TOWER_TURN: u8 = 1;
/// When a side with a gate puts one up by its main building, turned a
/// quarter, and then locks it, holds it open and sets it back to Auto.
const GATE_AT: u32 = 16 * TICKS_PER_SECOND;
const LOCK_AT: u32 = 20 * TICKS_PER_SECOND;
const HOLD_OPEN_AT: u32 = 40 * TICKS_PER_SECOND;
const AUTO_AT: u32 = 60 * TICKS_PER_SECOND;
/// From when a side with a drawn wall has its last worker draw a short one
/// by its main building, as soon as it can pay for it, and how many tiles
/// long.
const WALL_AT: u32 = 22 * TICKS_PER_SECOND;
const WALL_TILES: i32 = 4;
/// Soldiers and workers each side trains up to.
const FIGHTERS: usize = 6;
const WORKERS: usize = 8;
/// Caravans a trading side trains up to.
const CARAVANS: usize = 3;

/// What a faction's script plays with: its main building and what it
/// upgrades to, its worker, a building that trains soldiers and the
/// soldier, a house, and the cheapest other building (laid to be called
/// off).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Kit {
    pub main: BuildingId,
    pub keep: Option<BuildingId>,
    pub worker: UnitId,
    /// A unit that trades, for a faction whose Grub comes by caravan.
    pub caravan: Option<UnitId>,
    pub barracks: BuildingId,
    pub fighter: UnitId,
    pub house: BuildingId,
    pub spare: BuildingId,
    /// Its cheapest tier-1 tower.
    pub tower: Option<BuildingId>,
    /// Its gate, if it builds one.
    pub gate: Option<BuildingId>,
    /// Its wall drawn as a line, if it builds one.
    pub wall: Option<BuildingId>,
    /// What a worker, a soldier and the upgrade cost: Grub, Marrow and
    /// supply.
    pub worker_cost: [u32; 3],
    pub caravan_cost: [u32; 3],
    /// What the barracks, the house and the spare building cost.
    pub barracks_cost: [u32; 3],
    pub house_cost: [u32; 3],
    pub spare_cost: [u32; 3],
    pub fighter_cost: [u32; 3],
    pub keep_cost: [u32; 3],
}

impl Kit {
    pub fn of(data: &GameData, table: &Table, faction: Faction) -> Kit {
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
        let caravan = data
            .conquest_units
            .iter()
            .position(|u| u.trades() && u.faction() == faction)
            .map(|u| u as UnitId);
        let plain = ids(&|i| {
            let s = table.stats[i];
            // Not walls or gates: drawn walls are laid by the line, and the
            // host puts the gate up.
            s.role != Role::Main && s.role != Role::Wall && s.upgrades_from.is_none() && s.tier <= 1
        });
        let barracks = *plain
            .iter()
            .find(|&&b| trains(b, false).is_some())
            .expect("a tier-1 building trains soldiers");
        // A house, or for a faction without one (the Choir) the cheapest
        // other building that adds supply.
        let house = *plain
            .iter()
            .filter(|&&b| b != barracks && stats(b).supply > 0)
            .min_by_key(|&&b| (stats(b).role != Role::Housing, stats(b).cost_grub, b))
            .expect("a building that adds supply");
        let spare = *plain
            .iter()
            .filter(|&&b| b != barracks && b != house)
            .min_by_key(|&&b| (stats(b).cost_grub, b))
            .expect("another tier-1 building");
        let keep = ids(&|i| table.stats[i].upgrades_from == Some(main))
            .first()
            .copied();
        let fighter = trains(barracks, false).unwrap_or(worker);
        let tower = ids(&|i| table.stats[i].attack.is_some() && table.stats[i].tier <= 1)
            .into_iter()
            .min_by_key(|&b| (stats(b).cost_grub, b));
        let gate = ids(&|i| table.stats[i].flags & gc_sim::structures::GATE != 0)
            .first()
            .copied();
        let wall = ids(&|i| table.stats[i].flags & gc_sim::structures::LINE != 0)
            .first()
            .copied();
        let building_cost = |b: u16| {
            [
                u32::from(stats(b).cost_grub),
                u32::from(stats(b).cost_marrow),
                0,
            ]
        };
        let unit_cost = |u: UnitId| {
            let u = &data.conquest_units[usize::from(u)];
            [
                u32::from(u.cost_grub),
                u32::from(u.cost_marrow),
                u32::from(u.supply),
            ]
        };
        Kit {
            gate,
            wall,
            main,
            keep,
            worker,
            caravan,
            barracks,
            fighter,
            house,
            spare,
            tower,
            worker_cost: unit_cost(worker),
            caravan_cost: caravan.map_or([u32::MAX; 3], unit_cost),
            barracks_cost: building_cost(barracks),
            house_cost: building_cost(house),
            spare_cost: building_cost(spare),
            fighter_cost: unit_cost(fighter),
            keep_cost: keep.map_or([u32::MAX; 3], |b| {
                let s = stats(b);
                [u32::from(s.cost_grub), u32::from(s.cost_marrow), 0]
            }),
        }
    }
}

/// What one side has done so far.
#[derive(Clone, Copy, Default)]
struct Side {
    helped: bool,
    called_off: bool,
    rallied: bool,
    charged: bool,
    /// The chord is up, and how many of its powers' uses are done.
    chord: bool,
    powers: u8,
    /// Shrines put up, and how many Edicts are done.
    shrines: u8,
    edicts: u8,
    /// Its tower has been told whom to shoot.
    focused: bool,
    /// The worker drawing its wall, left to it until the wall is up.
    waller: Option<RegimentHandle>,
}

/// The Clans' Shrine and their three built Edicts, as the first side plays
/// them.
#[derive(Clone, Copy, Debug)]
struct Clans {
    shrine: BuildingId,
    tax: u8,
    march: u8,
    banner: u8,
}

impl Clans {
    fn of(data: &GameData) -> Clans {
        let shrine = (data.buildings.iter())
            .position(|b| b.name() == "Ancestor Shrine")
            .expect("the Clans' Shrine") as BuildingId;
        let power = |name: &str| {
            (data.powers.iter())
                .position(|p| p.name() == name && p.built())
                .expect("the Clans' Edict") as u8
        };
        Clans {
            shrine,
            tax: power("Rice Tax"),
            march: power("Forced March"),
            banner: power("Banner of the Ancestors"),
        }
    }
}

/// The Choir's Resonator and its two powers, as the second side plays them.
#[derive(Clone, Copy, Debug)]
struct Choir {
    resonator: BuildingId,
    overclock: u8,
    shift: u8,
}

impl Choir {
    fn of(data: &GameData) -> Choir {
        let building = |name: &str| {
            (data.buildings.iter())
                .position(|b| b.name() == name)
                .expect("the Choir's building") as BuildingId
        };
        let power = |name: &str| {
            (data.powers.iter())
                .position(|p| p.name() == name && p.built())
                .expect("the Choir's power") as u8
        };
        Choir {
            resonator: building("Resonator"),
            overclock: power("Overclock"),
            shift: power("Phase Shift"),
        }
    }
}

/// The match: its world, its recorder and both sides' scripts.
pub struct Match {
    pub world: World,
    rec: Recorder,
    kits: [Kit; 2],
    sides: [Side; 2],
    choir: Choir,
    clans: Clans,
    starts: Vec<(i32, i32)>,
}

impl Match {
    pub fn new() -> Match {
        let data = crate::data::load();
        let table = Table::new(&data);
        let map = conquest::make_map(0, 0);
        let units = conquest::unit_stats(&data);
        let nav = map.sim_nav_costs();
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
        let mut plan = conquest::plan(
            &table,
            &data,
            &map,
            &FACTIONS,
            Tempo::Swift,
            Victories::BOTH,
        );
        // The Concord side plays the Choir's powers.
        plan.meters[1] = Meter::Resonance;
        // Its Grub comes by caravan, slowly at first: a bigger purse to open
        // with, so the script's sides arm alike.
        plan.start[1] = (900, 100);
        let mut world = World::new(&setup);
        assert!(world.conquest(&plan.setup()));
        let rec = Recorder::new(&setup, &world, String::new()).conquest(plan);
        let opening = conquest::opening(&data, &table, &map, &world, &FACTIONS);
        let mut m = Match {
            kits: FACTIONS.map(|f| Kit::of(&data, &table, f)),
            sides: Default::default(),
            choir: Choir::of(&data),
            clans: Clans::of(&data),
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

    fn own(&self, p: PlayerId, b: Option<BuildingId>) -> Vec<StructureView> {
        own(&self.world, p, b)
    }

    fn main_of(&self, p: PlayerId) -> Option<StructureView> {
        main_of(&self.world, &self.kits[usize::from(p)], p)
    }

    fn workers(&self, p: PlayerId) -> Vec<RegimentHandle> {
        regiments_of(&self.world, p, self.kits[usize::from(p)].worker)
    }

    fn site(&self, p: PlayerId, b: BuildingId) -> Option<(i16, i16)> {
        site(&self.world, &self.kits[usize::from(p)], p, b)
    }

    /// The north-west tile of a 3 by 3 area round the overgrown tile
    /// nearest side `p`'s main building (`None` on a map with none).
    fn overgrown_near(&self, p: PlayerId) -> Option<(i16, i16)> {
        let main = self.main_of(p)?;
        let (cx, cy) = (main.tile.0, main.tile.1);
        let side = self.world.terrain().growth_side() as i32;
        (0..side * side)
            .map(|i| (i % side, i / side))
            .filter(|&(x, y)| self.world.overgrown([x, y, 1, 1]))
            .min_by_key(|&(x, y)| ((x - cx).pow(2) + (y - cy).pow(2), y, x))
            .map(|(x, y)| ((x - 1) as i16, (y - 1) as i16))
    }

    fn nodes_near(&self, p: PlayerId, resource: Resource) -> Vec<u16> {
        nodes_near(&self.world, self.starts[usize::from(p)], resource)
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
        // Its caravans trade at the post nearest its start.
        let trading = k.caravan.is_some();
        if trading && (t == GATHER_AT || t.is_multiple_of(10 * TICKS_PER_SECOND)) {
            out.extend(trade_orders(&self.world, p, self.starts[usize::from(p)]));
        }
        // Each order's cost comes off what the later ones can spend.
        let mut bank = self.world.bank(p).unwrap_or_default();
        if t == GATHER_AT || t.is_multiple_of(10 * TICKS_PER_SECOND) {
            for (i, &w) in workers.iter().enumerate() {
                let building =
                    i < builders && t >= LAY_FROM && !barracks.is_some_and(|b| b.finished())
                        || i == 0 && self.own(p, Some(k.house)).iter().any(|h| !h.finished());
                if building || self.walling(p, w) || self.world.gathers_at(w).is_some() {
                    continue;
                }
                let node = if trading {
                    marrow.get(i % 2)
                } else if i == 3 || i == 4 {
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
            // A trading faction's first hires are its caravans.
            let (unit, cost) = match k.caravan {
                Some(c) if self.count(p, c) < CARAVANS => (c, k.caravan_cost),
                _ => (k.worker, k.worker_cost),
            };
            if t > UNTRAIN_AT
                && t < UPGRADE_FROM
                && queue == 0
                && (unit != k.worker || workers.len() < WORKERS)
                && affords(&bank, cost)
                && t.is_multiple_of(TICKS_PER_SECOND)
            {
                out.push(order(Command::Train { structure, unit }));
                spend(&mut bank, cost);
            }
            let upgrading = main.upgrading.is_some() || Some(main.building) == k.keep;
            if t >= UPGRADE_FROM
                && !upgrading
                && affords(&bank, k.keep_cost)
                && t.is_multiple_of(TICKS_PER_SECOND)
            {
                out.push(order(Command::Upgrade { structure }));
                spend(&mut bank, k.keep_cost);
            }
        }
        // The barracks: laid by the first worker, helped by the second,
        // then training soldiers who rally ahead (and not laid again once
        // the host takes it away).
        match barracks {
            None if (LAY_FROM..REMOVE_AT).contains(&t)
                && t.is_multiple_of(TICKS_PER_SECOND)
                && affords(&bank, k.barracks_cost) =>
            {
                if let (Some(&w), Some(tile)) = (workers.first(), self.site(p, k.barracks)) {
                    out.push(order(Command::Build {
                        regiment: w,
                        building: k.barracks,
                        tile,
                        turn: 0,
                    }));
                    spend(&mut bank, k.barracks_cost);
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
                    && affords(&bank, k.fighter_cost)
                    && t.is_multiple_of(TICKS_PER_SECOND)
                {
                    out.push(order(Command::Train {
                        structure: b.handle,
                        unit: k.fighter,
                    }));
                    spend(&mut bank, k.fighter_cost);
                }
                // A house whenever supply runs short, laid by the first
                // worker.
                let one_going_up = self.own(p, Some(k.house)).iter().any(|h| !h.finished());
                if bank.supply_cap < bank.supply_used + 6
                    && !one_going_up
                    && t.is_multiple_of(TICKS_PER_SECOND)
                    && affords(&bank, k.house_cost)
                    && let (Some(&w), Some(tile)) = (workers.first(), self.site(p, k.house))
                {
                    out.push(order(Command::Build {
                        regiment: w,
                        building: k.house,
                        tile,
                        // A half turn keeps the footprint the site was
                        // found for (format 26's turned tags).
                        turn: 2,
                    }));
                    spend(&mut bank, k.house_cost);
                }
            }
            _ => {}
        }
        // The spare building: laid by the third worker and called off.
        match spare {
            None if t >= SPARE_FROM
                && !self.sides[usize::from(p)].called_off
                && t.is_multiple_of(TICKS_PER_SECOND)
                && affords(&bank, k.spare_cost) =>
            {
                if let (Some(&w), Some(tile)) = (workers.get(builders), self.site(p, k.spare)) {
                    out.push(order(Command::Build {
                        regiment: w,
                        building: k.spare,
                        tile,
                        turn: 0,
                    }));
                    spend(&mut bank, k.spare_cost);
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
        if t == CLEAR_AT
            && let (Some(&w), Some(tile)) = (workers.get(builders), self.overgrown_near(p))
        {
            out.push(order(Command::Clear {
                regiment: w,
                tile,
                size: (3, 3),
            }));
        }
        // The soldiers march on the other side's main building (each idle
        // one, once a second), and once they have seen it through the fog,
        // every soldier attacks it. Side 1 sets out later, so the armies
        // meet near its base, where side 0 sees the main building (where
        // they met halfway, whichever won kept the other from getting
        // there).
        let march_at = if p == 0 {
            MARCH_AT
        } else {
            MARCH_AT + MARCH_LATER
        };
        if t >= march_at && !self.sides[usize::from(p)].charged {
            let target = self.main_of(1 - p);
            let seen = target.is_some_and(|m| m.seen_by(p));
            if let Some(target) = target.filter(|_| seen || t.is_multiple_of(TICKS_PER_SECOND)) {
                // Routers take no orders.
                let fighters = (self.world.regiments.iter()).filter(|r| {
                    r.owner == p
                        && r.unit == k.fighter
                        && k.fighter != k.worker
                        && !r.morale.broken()
                });
                for r in fighters {
                    if seen {
                        out.push(order(Command::AttackStructure {
                            regiment: r.handle,
                            structure: target.handle,
                        }));
                    } else if !r.moving && !r.engaged {
                        out.push(order(Command::MoveTo {
                            regiment: r.handle,
                            target: target.centre(),
                            facing: None,
                            files: 0,
                            pace: Fx::ZERO,
                        }));
                    }
                }
            }
            self.sides[usize::from(p)].charged = seen || target.is_none();
        }
        self.tower(p, out);
        self.gate(p, out);
        self.wall(p, &workers, out);
        if p == 1 {
            self.powers(p, out);
        } else {
            self.edicts(p, out);
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

    /// Side `p`'s tower: put up by the host by its main building, then
    /// told to shoot the first enemy regiment it sees in range.
    fn tower(&mut self, p: PlayerId, out: &mut Vec<PlayerCommand>) {
        let t = self.world.tick_count();
        let k = self.kits[usize::from(p)];
        let Some(b) = k.tower else {
            return;
        };
        let order = |command| PlayerCommand { player: p, command };
        if t == TOWER_AT {
            out.push(self.by_main(p, b, 3));
        }
        if self.sides[usize::from(p)].focused {
            return;
        }
        let Some(tower) = own(&self.world, p, Some(b)).first().copied() else {
            return;
        };
        let range = (self.world.structures())
            .and_then(|s| s.buildings()[usize::from(b)].attack)
            .map_or(Fx::ZERO, |a| a.range);
        let foe = (self.world.regiments.iter()).find(|r| {
            r.owner != p
                && self.world.sees(p, r.handle)
                && (r.position - tower.centre()).length() < range
        });
        if let Some(foe) = foe {
            out.push(order(Command::FocusTower {
                structure: tower.handle,
                target: Some(foe.handle),
            }));
            self.sides[usize::from(p)].focused = true;
        }
    }

    /// The host putting up building `b` for side `p`, turned
    /// [`TOWER_TURN`], on the first free spot `ring` tiles out from its main
    /// building.
    fn by_main(&self, p: PlayerId, b: BuildingId, ring: i32) -> PlayerCommand {
        let main = self.main_of(p).expect("a main building");
        let foot: Foot = [
            main.tile.0,
            main.tile.1,
            i32::from(main.width),
            main.depth.into(),
        ];
        let tile = conquest::ring(foot, ring)
            .find(|&t| self.world.can_build(b, t, TOWER_TURN).is_ok())
            .expect("room by the main building");
        PlayerCommand {
            player: p,
            command: Command::SpawnStructure {
                building: b,
                tile: (tile.0 as i16, tile.1 as i16),
                turn: TOWER_TURN,
            },
        }
    }

    /// Side `p`'s gate: put up by the host by its main building, then
    /// locked, held open and set back to Auto.
    fn gate(&mut self, p: PlayerId, out: &mut Vec<PlayerCommand>) {
        let t = self.world.tick_count();
        let Some(b) = self.kits[usize::from(p)].gate else {
            return;
        };
        let order = |command| PlayerCommand { player: p, command };
        if t == GATE_AT {
            // Out by the gathering ground, where workers going past open and
            // shut it, but clear of the main fight.
            out.push(self.by_main(p, b, 8));
        }
        let mode = match t {
            LOCK_AT => GateMode::Locked,
            HOLD_OPEN_AT => GateMode::Open,
            AUTO_AT => GateMode::Auto,
            _ => return,
        };
        if let Some(gate) = own(&self.world, p, Some(b)).first() {
            out.push(order(Command::SetGate {
                structure: gate.handle,
                mode,
            }));
        }
    }

    /// Worker `w` is drawing side `p`'s wall and some of it isn't up yet.
    fn walling(&self, p: PlayerId, w: RegimentHandle) -> bool {
        self.sides[usize::from(p)].waller == Some(w)
            && self
                .own(p, self.kits[usize::from(p)].wall)
                .iter()
                .any(|s| !s.finished())
    }

    /// Side `p`'s drawn wall: once it can pay for it, its last worker draws
    /// [`WALL_TILES`] tiles of it, west to east, on the first line that fits a few tiles out from
    /// its main building.
    fn wall(&mut self, p: PlayerId, workers: &[RegimentHandle], out: &mut Vec<PlayerCommand>) {
        let Some(b) = self.kits[usize::from(p)].wall else {
            return;
        };
        // After the spare building is called off, so as not to take its
        // worker or its Grub.
        let side = self.sides[usize::from(p)];
        if self.world.tick_count() < WALL_AT || !side.called_off || side.waller.is_some() {
            return;
        }
        let (Some(main), Some(&w)) = (self.main_of(p), workers.last()) else {
            return;
        };
        let foot: Foot = [
            main.tile.0,
            main.tile.1,
            i32::from(main.width),
            main.depth.into(),
        ];
        let fits = |from: (i32, i32), to: (i32, i32)| {
            self.world.wall_plan(p, b, from, to).is_ok_and(|plan| {
                plan.len() == WALL_TILES as usize && plan.iter().all(|t| t.fits.is_ok())
            })
        };
        let Some(from) =
            conquest::ring(foot, 6).find(|&(x, y)| fits((x, y), (x + WALL_TILES - 1, y)))
        else {
            return;
        };
        let to = (from.0 + WALL_TILES - 1, from.1);
        out.push(PlayerCommand {
            player: p,
            command: Command::BuildWall {
                regiment: w,
                building: b,
                from: (from.0 as i16, from.1 as i16),
                to: (to.0 as i16, to.1 as i16),
            },
        });
        self.sides[usize::from(p)].waller = Some(w);
    }

    /// Side `p`'s chord and powers: three Resonators put up, then
    /// Overclock on its main building, Phase Shift for two soldiers to a
    /// Resonator (once it is tier 2) and Overclock on a soldier, each as
    /// soon as it has the Resonance.
    fn powers(&mut self, p: PlayerId, out: &mut Vec<PlayerCommand>) {
        let t = self.world.tick_count();
        let (c, k) = (self.choir, self.kits[usize::from(p)]);
        let order = |command| PlayerCommand { player: p, command };
        let ready = |power| self.world.can_use_power(p, power).is_ok();
        let fighters: Vec<RegimentHandle> = (self.world.regiments.iter())
            .filter(|r| r.owner == p && r.unit == k.fighter && !r.morale.broken())
            .map(|r| r.handle)
            .collect();
        let side = &mut self.sides[usize::from(p)];
        if !side.chord && t >= CHORD_AT {
            let sites = chord_sites(&self.world, &self.kits[usize::from(p)], p, c.resonator)
                .expect("room for a chord near the main building");
            for tile in sites {
                out.push(order(Command::SpawnStructure {
                    building: c.resonator,
                    tile,
                    turn: 0,
                }));
            }
            side.chord = true;
        }
        let main = main_of(&self.world, &k, p);
        let resonator = own(&self.world, p, Some(c.resonator)).first().copied();
        match side.powers {
            0 if ready(c.overclock) => {
                if let Some(main) = main {
                    out.push(order(Command::UsePower {
                        power: c.overclock,
                        target: PowerTarget::Structure(main.handle),
                    }));
                    side.powers = 1;
                }
            }
            1 if ready(c.shift) && fighters.len() >= 2 => {
                if let Some(to) = resonator {
                    for &regiment in &fighters[..2] {
                        out.push(order(Command::UsePower {
                            power: c.shift,
                            target: PowerTarget::Shift {
                                regiment,
                                to: to.handle,
                            },
                        }));
                    }
                    side.powers = 2;
                }
            }
            2 if ready(c.overclock) => {
                if let Some(&regiment) = fighters.first() {
                    out.push(order(Command::UsePower {
                        power: c.overclock,
                        target: PowerTarget::Regiment(regiment),
                    }));
                    side.powers = 3;
                }
            }
            _ => {}
        }
    }

    /// Side `p`'s Shrines and Edicts: Shrines put up one a tick, then Rice
    /// Tax, Forced March for all its soldiers once they have seen the
    /// other's main building (marched sooner, they reach the fight strung
    /// out and rout), and the Banner of the Ancestors over its first
    /// soldier (once it is tier 2), each as soon as it has the Honor.
    fn edicts(&mut self, p: PlayerId, out: &mut Vec<PlayerCommand>) {
        let t = self.world.tick_count();
        let c = self.clans;
        let order = |command| PlayerCommand { player: p, command };
        let side = self.sides[usize::from(p)];
        if side.shrines < SHRINES && t >= SHRINES_AT {
            // Behind the base, out of its workers' way.
            let behind = self.ahead(p, -20);
            let (bx, by) = (behind.x.floor_int(), behind.y.floor_int());
            let tile = conquest::ring([bx, by, 1, 1], 0)
                .take(400)
                .find(|&t| self.world.can_build(c.shrine, t, 0).is_ok())
                .map(|(x, y)| (x as i16, y as i16))
                .expect("room for a Shrine behind the main building");
            out.push(order(Command::SpawnStructure {
                building: c.shrine,
                tile,
                turn: 0,
            }));
            self.sides[usize::from(p)].shrines += 1;
        }
        let power = match side.edicts {
            0 => c.tax,
            1 if side.charged => c.march,
            2 => c.banner,
            _ => return,
        };
        if self.world.can_use_power(p, power).is_err() {
            return;
        }
        let k = self.kits[usize::from(p)];
        let fighters: Vec<(RegimentHandle, FxVec2)> = (self.world.regiments.iter())
            .filter(|r| r.owner == p && r.unit == k.fighter && !r.morale.broken())
            .map(|r| (r.handle, r.position))
            .collect();
        let targets: Vec<PowerTarget> = match side.edicts {
            0 => vec![PowerTarget::Caster],
            1 => fighters
                .iter()
                .map(|&(r, _)| PowerTarget::Regiment(r))
                .collect(),
            _ => fighters
                .first()
                .map(|&(_, at)| PowerTarget::Point(at))
                .into_iter()
                .collect(),
        };
        if targets.is_empty() {
            return;
        }
        for target in targets {
            out.push(order(Command::UsePower { power, target }));
        }
        self.sides[usize::from(p)].edicts += 1;
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

/// Side `p`'s buildings of type `b` (or any type, for `None`).
pub(crate) fn own(world: &World, p: PlayerId, b: Option<BuildingId>) -> Vec<StructureView> {
    world
        .structures()
        .map(|s| {
            s.iter()
                .filter(|v| v.owner == p && b.is_none_or(|b| v.building == b))
                .collect()
        })
        .unwrap_or_default()
}

/// Side `p`'s main building, kept or not.
pub(crate) fn main_of(world: &World, k: &Kit, p: PlayerId) -> Option<StructureView> {
    own(world, p, None)
        .into_iter()
        .find(|v| v.building == k.main || Some(v.building) == k.keep)
}

/// Side `p`'s regiments of type `unit`.
pub(crate) fn regiments_of(world: &World, p: PlayerId, unit: UnitId) -> Vec<RegimentHandle> {
    world
        .regiments
        .iter()
        .filter(|r| r.owner == p && r.unit == unit)
        .map(|r| r.handle)
        .collect()
}

/// Where side `p` can lay building `b` near its main building, the
/// nearest first.
pub(crate) fn site(world: &World, k: &Kit, p: PlayerId, b: BuildingId) -> Option<(i16, i16)> {
    let main = main_of(world, k, p)?;
    let foot: Foot = [
        main.tile.0,
        main.tile.1,
        i32::from(main.width),
        i32::from(main.depth),
    ];
    conquest::ring(foot, 3)
        .find(|&t| world.can_lay(p, b, t, 0).is_ok())
        .map(|(x, y)| (x as i16, y as i16))
}

/// Three tiles round side `p`'s main building where buildings of type
/// `b` can go up and make a chord: a triangle about 8 tiles a side,
/// 10 tiles off one side of the main building.
fn chord_sites(world: &World, k: &Kit, p: PlayerId, b: BuildingId) -> Option<[(i16, i16); 3]> {
    let main = main_of(world, k, p)?;
    let (cx, cy) = (
        main.tile.0 + i32::from(main.width) / 2,
        main.tile.1 + i32::from(main.depth) / 2,
    );
    let near = |(x, y): (i32, i32)| {
        conquest::ring([x, y, 1, 1], 0)
            .take(64)
            .find(|&t| world.can_build(b, t, 0).is_ok())
    };
    [(0, 1), (0, -1), (1, 0), (-1, 0)]
        .into_iter()
        .find_map(|(dx, dy)| {
            let base = (cx + dx * 10, cy + dy * 10);
            let (px, py) = (dy, dx);
            let sites = [
                near((base.0 - px * 4, base.1 - py * 4))?,
                near((base.0 + px * 4, base.1 + py * 4))?,
                near((base.0 + dx * 8, base.1 + dy * 8))?,
            ];
            let apart = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs().max((a.1 - b.1).abs());
            let fits = (0..3).all(|i| {
                let (a, b) = (sites[i], sites[(i + 1) % 3]);
                (4..=9).contains(&apart(a, b))
            });
            fits.then(|| sites.map(|(x, y)| (x as i16, y as i16)))
        })
}

/// The nodes of `resource` with something left, nearest `start` first.
pub(crate) fn nodes_near(world: &World, (sx, sy): (i32, i32), resource: Resource) -> Vec<u16> {
    let start = FxVec2::from_ints(sx, sy);
    let mut nodes: Vec<(u64, u16)> = world
        .economy()
        .into_iter()
        .flat_map(gc_sim::Economy::nodes)
        .filter(|n| n.resource == resource && n.left > 0)
        .map(|n| ((n.at - start).length_sq_raw(), n.index))
        .collect();
    nodes.sort_unstable();
    nodes.into_iter().map(|(_, i)| i).collect()
}

/// Orders for side `p`'s idle caravans to trade at the post nearest
/// `start` (in tiles) that they can: none if there is none.
pub(crate) fn trade_orders(
    world: &World,
    p: PlayerId,
    (sx, sy): (i32, i32),
) -> impl Iterator<Item = PlayerCommand> + '_ {
    let posts = world.trade_posts();
    let post = (0..posts.len())
        .map(|i| TradePost::Hamlet(i as u16))
        .filter(|&post| world.is_trade_post(p, post))
        .min_by_key(|&post| {
            let TradePost::Hamlet(i) = post else {
                return i32::MAX;
            };
            let at = posts[usize::from(i)];
            let (dx, dy) = (at.x.floor_int() - sx, at.y.floor_int() - sy);
            dx * dx + dy * dy
        });
    post.into_iter().flat_map(move |post| {
        world.idle_traders(p).map(move |regiment| PlayerCommand {
            player: p,
            command: Command::Trade { regiment, post },
        })
    })
}

/// Take `cost` (Grub, Marrow, supply) out of `bank`, which affords it.
fn spend(bank: &mut gc_sim::Bank, [grub, marrow, supply]: [u32; 3]) {
    bank.grub -= grub;
    bank.marrow -= marrow;
    bank.supply_used += supply;
}

/// Whether `bank` holds `cost`'s Grub and Marrow and has room for its
/// supply.
pub(crate) fn affords(bank: &gc_sim::Bank, [grub, marrow, supply]: [u32; 3]) -> bool {
    bank.grub >= grub && bank.marrow >= marrow && bank.supply_used + supply <= bank.supply_cap
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
            assert!(m.count(p, k.worker) >= 4, "side {p} trained workers: {k:?}");
            assert!(bank.grub + bank.marrow > 0 || start.0 > 0, "side {p}");
            let main = m.main_of(p).expect("its main building stands");
            if let Some(keep) = k.keep {
                assert!(
                    main.building == keep || main.upgrading.is_some(),
                    "side {p} upgraded its main building"
                );
            }
            assert!(m.own(p, Some(k.spare)).is_empty(), "side {p} called off");
            let side = m.sides[usize::from(p)];
            assert!(side.helped && side.called_off);
            assert!(side.charged, "side {p} found the other's main building");
            if let Some(wall) = k.wall {
                let tiles = m.own(p, Some(wall));
                assert!(side.waller.is_some(), "side {p} drew its wall");
                assert!(
                    tiles.len() == WALL_TILES as usize && tiles.iter().all(StructureView::finished),
                    "side {p} built all its wall: {tiles:?}"
                );
            }
        }
        assert_eq!(m.sides[1].powers, 3, "side 1 used its powers");
        assert_eq!(m.sides[0].edicts, 3, "side 0 used its Edicts");
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
