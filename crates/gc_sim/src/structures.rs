//! Buildings on the field (Conquest, docs/21-buildings.md): where they
//! stand, what is left of them, and regiments knocking them down.
//!
//! A building stands on a rectangle of whole tiles, its footprint, which no
//! regiment can walk on while it stands (see `Nav::set_built`): armies march
//! round it. Regiments ordered against one ([`crate::Command::AttackStructure`])
//! close to its wall, or come into range, and hit it. A blow on a building
//! always lands, since buildings don't parry; its damage is scaled by the
//! attacker's [`Breaker`] class against the building's [`Armour`]:
//!
//! | Attacker | Structure | Fortified | Living |
//! |---|---|---|---|
//! | Regiments | ×1.0 | ×0.5 | ×1.0 |
//! | Heroes | ×1.0 | ×1.0 | ×1.0 |
//! | Siege | ×2.0 | ×3.0 | ×2.0 |
//! | Artillery | ×1.5 | ×1.0 | ×1.5 |
//!
//! Melee regiments strike once a second with their front rank (and spears
//! their second, as against regiments); shooters loose a volley each
//! reload in which every arrow hits. A building brought to nothing is gone,
//! and its ground is free again.
//!
//! Only a Conquest world has buildings (see [`crate::World::conquest`]);
//! a Field Battle's state and hash are as they were before buildings
//! existed.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::combat::Block;
use crate::conquest::ConquestUnit;
use crate::nav::{CELLS_PER_TILE, CLEARANCE, Nav};
use crate::regiments::{Order, RegimentHandle, Regiments};
use crate::verdict::Mains;
use crate::{PlayerId, StateHasher, TICKS_PER_SECOND, UnitStats};

/// Index of a building type in the match's building table.
pub type BuildingId = u16;

/// What a building is for (docs/21-buildings.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Role {
    /// The main building (a Hall, Hive, Spire or Counting House, at any
    /// tier): the ones Annihilation counts.
    Main = 0,
    /// A cheaper hall for expansions.
    Outpost = 1,
    Housing = 2,
    Economy = 3,
    Production = 4,
    Tech = 5,
    Heroes = 6,
    Defence = 7,
    Wall = 8,
}

impl Role {
    /// How far a building of this role sees in fog of war, edge to edge,
    /// in tiles (see [`crate::vision`]): towers see past their reach, a
    /// wall hardly at all.
    pub const fn sight(self) -> Fx {
        Fx::from_int(match self {
            Role::Main => 32,
            Role::Outpost => 28,
            Role::Defence => 36,
            Role::Wall => 8,
            _ => 20,
        })
    }

    pub const ALL: [Role; 9] = [
        Role::Main,
        Role::Outpost,
        Role::Housing,
        Role::Economy,
        Role::Production,
        Role::Tech,
        Role::Heroes,
        Role::Defence,
        Role::Wall,
    ];

    pub fn from_u8(v: u8) -> Option<Role> {
        Role::ALL.get(usize::from(v)).copied()
    }
}

/// A building's armour type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Armour {
    Structure = 0,
    Fortified = 1,
    Living = 2,
}

/// How hard a unit's blows fall on buildings (the rows of the table above).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Breaker {
    #[default]
    Regiment = 0,
    Hero = 1,
    /// Battering rams and the like.
    Siege = 2,
    /// Rocket carts and the like.
    Artillery = 3,
}

impl Armour {
    pub fn from_u8(v: u8) -> Option<Armour> {
        [Armour::Structure, Armour::Fortified, Armour::Living]
            .get(usize::from(v))
            .copied()
    }
}

impl Breaker {
    pub fn from_u8(v: u8) -> Option<Breaker> {
        [
            Breaker::Regiment,
            Breaker::Hero,
            Breaker::Siege,
            Breaker::Artillery,
        ]
        .get(usize::from(v))
        .copied()
    }

    /// Percent of its damage a blow of this class does to `armour`.
    pub fn percent(self, armour: Armour) -> u32 {
        match (self, armour) {
            (Breaker::Regiment, Armour::Fortified) => 50,
            (Breaker::Regiment | Breaker::Hero, _) | (Breaker::Artillery, Armour::Fortified) => 100,
            (Breaker::Siege, Armour::Fortified) => 300,
            (Breaker::Siege, _) => 200,
            (Breaker::Artillery, _) => 150,
        }
    }
}

/// Why a building can't go up where it was asked to, or a building can't
/// train or upgrade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not a Conquest match.
    NoBuildings,
    /// No such building type.
    Unknown,
    /// Some of it would be off the map.
    OffMap,
    /// Ground no regiment can walk on.
    Rough,
    /// Another building is there.
    Built,
    /// A regiment is standing there.
    Crowded,
    /// Its owner's tier is too low for it.
    Tier,
    /// Its owner can't pay for it.
    Price,
    /// A Grub seep or Marrow vein is there.
    Node,
    /// The building is going up, already upgrading, or its queue is full.
    Busy,
    /// It hatches from a larva, and its owner has none ready.
    Larva,
}

/// Bits of [`BuildingStats::flags`].
pub const DROP_OFF_GRUB: u8 = 1;
pub const DROP_OFF_MARROW: u8 = 2;
/// Choir: powers buildings around it.
pub const POWERS_FIELD: u8 = 4;
/// Its attack can hit flying units.
pub const ANTI_AIR: u8 = 8;
/// Brood: holds larvae and grows them (see [`crate::larvae`]).
pub const LARVAE: u8 = 16;
/// Brood: spreads root-veins (see [`crate::veins`]).
pub const VEINS: u8 = 32;

/// A tower's shooting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TowerAttack {
    pub range: Fx,
    /// Hit points each shot takes.
    pub damage: u16,
    pub reload_ticks: u16,
    pub shots: u8,
}

/// What the simulation needs from a building type. The game builds these
/// from the baked building table (`gc_data`); tests build them by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildingStats {
    pub role: Role,
    /// Main-building tier needed to place it (for an upgrade, the tier it
    /// gives).
    pub tier: u8,
    /// For a main building's tier 2 and 3: the building it upgrades.
    pub upgrades_from: Option<BuildingId>,
    pub cost_grub: u16,
    pub cost_marrow: u16,
    /// Ticks to build at Standard tempo.
    pub build_ticks: u16,
    pub hp: u16,
    pub armour: Armour,
    /// Footprint in tiles, across (x) and deep (y), each 1 to
    /// [`MAX_FOOTPRINT`].
    pub width: u8,
    pub depth: u8,
    /// What it adds to its owner's supply cap.
    pub supply: u8,
    /// `DROP_OFF_*`, [`POWERS_FIELD`], [`ANTI_AIR`] and [`LARVAE`] bits.
    pub flags: u8,
    pub attack: Option<TowerAttack>,
}

/// Largest footprint side, in tiles.
pub const MAX_FOOTPRINT: u8 = 5;

impl BuildingStats {
    /// The same stats forced into the ranges the simulation relies on.
    pub fn sanitized(self) -> BuildingStats {
        BuildingStats {
            hp: self.hp.max(1),
            width: self.width.clamp(1, MAX_FOOTPRINT),
            depth: self.depth.clamp(1, MAX_FOOTPRINT),
            build_ticks: self.build_ticks.max(1),
            attack: self.attack.map(|a| TowerAttack {
                range: a.range.max(Fx::ZERO),
                reload_ticks: a.reload_ticks.max(1),
                ..a
            }),
            ..self
        }
    }

    fn hash_into(&self, h: &mut StateHasher) {
        h.u8(self.role as u8);
        h.u8(self.tier);
        h.u16(self.upgrades_from.unwrap_or(u16::MAX));
        h.u16(self.cost_grub);
        h.u16(self.cost_marrow);
        h.u16(self.build_ticks);
        h.u16(self.hp);
        h.u8(self.armour as u8);
        h.u8(self.width);
        h.u8(self.depth);
        h.u8(self.supply);
        h.u8(self.flags);
        if let Some(a) = self.attack {
            h.i32(a.range.raw());
            h.u16(a.damage);
            h.u16(a.reload_ticks);
            h.u8(a.shots);
        }
    }
}

/// Generational handle to a building, like [`crate::RegimentHandle`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StructureHandle {
    pub index: u32,
    pub generation: u16,
}

/// A building as the game sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructureView {
    pub handle: StructureHandle,
    pub owner: PlayerId,
    pub building: BuildingId,
    /// The footprint's north-west tile.
    pub tile: (i32, i32),
    pub width: u8,
    pub depth: u8,
    /// Hit points left (rounded up: a building with any left shows 1).
    pub hp: u32,
    pub max_hp: u32,
    /// Percent built: 100 once finished.
    pub built_percent: u8,
    /// While its main building's tier upgrade goes on: the building it
    /// becomes, and percent done.
    pub upgrading: Option<(BuildingId, u8)>,
    /// It is going up on its own, having used up the worker that started
    /// it (see [`ConquestUnit::consumed`]).
    pub growing: bool,
    /// The parties that have seen it: every party outside fog of war.
    pub seen: u8,
}

impl StructureView {
    /// Whether `party` has seen it.
    pub fn seen_by(&self, party: PlayerId) -> bool {
        self.seen & (1 << party) != 0
    }

    pub fn finished(&self) -> bool {
        self.built_percent == 100
    }

    /// Centre of the footprint.
    pub fn centre(&self) -> FxVec2 {
        let (x, y) = self.tile;
        footprint_centre([x, y, i32::from(self.width), i32::from(self.depth)])
    }
}

/// Centre of the footprint `[x, y, width, depth]` (tiles).
fn footprint_centre([x, y, w, d]: [i32; 4]) -> FxVec2 {
    FxVec2::new(
        Fx::from_int(x) + Fx::from_ratio(w, 2),
        Fx::from_int(y) + Fx::from_ratio(d, 2),
    )
}

/// The point of a footprint nearest `here`.
pub(crate) fn nearest_point([x, y, w, d]: [i32; 4], here: FxVec2) -> FxVec2 {
    FxVec2::new(
        here.x.clamp(Fx::from_int(x), Fx::from_int(x + w)),
        here.y.clamp(Fx::from_int(y), Fx::from_int(y + d)),
    )
}

/// A regiment and a building's footprint: how far apart they are, and
/// where it would stand against it. Shared by sieges and by workers
/// bringing in what they gathered ([`crate::economy`]).
pub(crate) struct Approach {
    /// The nearest point of the footprint, and its centre.
    near: FxVec2,
    centre: FxVec2,
    /// From the regiment's centre to `near`.
    pub(crate) gap: Fx,
    /// Half the regiment's block depth.
    half: Fx,
}

impl Approach {
    pub(crate) fn of(regs: &Regiments, unit: &UnitStats, r: usize, foot: [i32; 4]) -> Approach {
        let here = regs.position[r];
        let near = nearest_point(foot, here);
        let (_, depth) = unit.block_size_in(regs.formation[r], regs.files[r], regs.models[r]);
        Approach {
            near,
            centre: footprint_centre(foot),
            gap: (near - here).length(),
            half: Fx::from_raw(depth.raw() / 2),
        }
    }

    /// How near its front rank must come to touch the wall.
    pub(crate) fn reach(&self) -> Fx {
        self.half + MELEE_REACH
    }

    /// Order regiment slot `r` to stand just off the nearest point of the
    /// footprint, facing the building.
    pub(crate) fn walk(&self, regs: &mut Regiments, r: usize) {
        let here = regs.position[r];
        let out = if self.near == here {
            here - self.centre
        } else {
            here - self.near
        }
        .normalized();
        let stand = self.near + out * (self.half + Fx::ONE);
        let h = regs.handle(r);
        regs.order(
            h,
            Order {
                target: stand,
                facing: Some(Angle::of_vec(self.centre - stand)),
                files: 0,
                pace: Fx::ZERO,
            },
        );
    }

    /// Regiment slot `r` is there: it stops and faces the building.
    pub(crate) fn arrive(&self, regs: &mut Regiments, r: usize) {
        if regs.target[r].is_some() {
            regs.target[r] = None;
            regs.target_facing[r] = Some(Angle::of_vec(self.centre - regs.position[r]));
        }
    }
}

/// Hit points are kept in hundredths, so a halved blow of 1 still counts.
const HP_SCALE: u32 = 100;
/// How far beyond its block's front a melee regiment reaches a wall, in
/// tiles: a regiment's centre keeps about a tile and a quarter from
/// blocked ground, so this lets the front rank touch it.
const MELEE_REACH: Fx = Fx::from_int(2);
/// Ticks a regiment that can't get at its building waits before trying
/// again.
const RETRY_TICKS: u16 = TICKS_PER_SECOND as u16;
/// Tries in a row that bring a regiment no nearer before it gives up.
const STALLS: u8 = 3;
/// A building goes up at this percent of its hit points and fills up as
/// it is built.
const FOUNDATION_PERCENT: u32 = 10;
/// Work a builder does a tick, in hundredths of a tick of build time: the
/// first at full speed, each other at half, up to [`CREW`] at once.
const FIRST_HAND: u32 = 100;
const OTHER_HAND: u32 = 50;
const CREW: u8 = 4;
/// Hit points a builder mends a second on a finished building.
const REPAIR_HP: u32 = 8;
/// Percent of its cost a building still going up gives back when
/// cancelled.
pub const CANCEL_REFUND_PERCENT: u32 = 75;

/// What a regiment does at a building it was sent to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Work {
    /// Knock it down (an enemy's).
    Attack = 0,
    /// Build it, or mend it once it is up (its owner's).
    Build = 1,
}

/// Every building on the field, and which regiments are attacking which.
pub struct Structures {
    buildings: Vec<BuildingStats>,
    generation: Vec<u16>,
    owner: Vec<PlayerId>,
    building: Vec<BuildingId>,
    tile: Vec<(i32, i32)>,
    /// Hit points left, in hundredths; 0 for a free slot.
    hp: Vec<u32>,
    free: Vec<u32>,
    /// Build work left and in all, in hundredths of a tick (see
    /// [`FIRST_HAND`]); `work_left` is 0 once finished.
    work_left: Vec<u32>,
    work_full: Vec<u32>,
    /// A main building's tier upgrade going on: what it becomes, and ticks
    /// left and in all.
    upgrade_to: Vec<Option<BuildingId>>,
    upgrade_left: Vec<u32>,
    upgrade_full: Vec<u32>,
    /// Builders who worked on each building this tick.
    crew: Vec<u8>,
    /// A building going up on its own, having used up the worker that
    /// started it (the Brood's, see [`ConquestUnit::consumed`]).
    growing: Vec<bool>,
    /// Brood: larvae each building holds, and ticks until its next one
    /// (0 while full); see [`crate::larvae`].
    pub(crate) larvae: Vec<u8>,
    pub(crate) larva_clock: Vec<u32>,
    /// Root-veins, worked out each tick (see [`crate::veins`]): the slot
    /// each linked veins building links through, the linked slots, and the
    /// unlinked veins buildings looked at on the way.
    pub(crate) vein_from: Vec<u32>,
    pub(crate) vein_linked: Vec<u32>,
    pub(crate) vein_cands: Vec<u32>,
    /// Tempo: build and upgrade times at this percent of Standard.
    build_percent: u32,
    /// By regiment slot: the building it is attacking, and the generation
    /// of the regiment the order was given to (a slot reused by a new
    /// regiment doesn't inherit it).
    siege: Vec<Option<StructureHandle>>,
    siege_gen: Vec<u16>,
    /// By regiment slot: what it does there.
    work: Vec<Work>,
    /// By regiment slot: ticks until its next blow, or its next try to
    /// reach the building.
    siege_clock: Vec<u16>,
    /// By regiment slot: how far it was from the building when it last set
    /// off for it, and how many times in a row that brought it no nearer.
    siege_gap: Vec<Fx>,
    siege_stalls: Vec<u8>,
    /// Buildings brought down in the last tick, for the events.
    pub(crate) fallen: Vec<StructureHandle>,
    /// Workers used up in the last tick by the buildings they started.
    pub(crate) consumed: Vec<(RegimentHandle, StructureHandle)>,
    /// The parties that have seen each building, its owner first: in fog of
    /// war a building stays known once seen (see [`crate::vision`]).
    seen: Vec<u8>,
    /// Fog of war: buildings are known only once seen.
    fog: bool,
}

/// The match's building table, each made sane, and with no tier upgrade
/// from a building of another footprint (it would grow over its ground).
fn sanitized(buildings: &[BuildingStats]) -> Vec<BuildingStats> {
    let mut table: Vec<BuildingStats> = buildings.iter().map(|b| b.sanitized()).collect();
    for i in 0..table.len() {
        let size = |b: &BuildingStats| (b.width, b.depth);
        if let Some(from) = table[i].upgrades_from
            && table
                .get(usize::from(from))
                .is_none_or(|f| size(f) != size(&table[i]))
        {
            table[i].upgrades_from = None;
        }
    }
    table
}

impl Structures {
    pub(crate) fn new(
        buildings: &[BuildingStats],
        capacity: u32,
        regiments: usize,
        build_percent: u32,
    ) -> Structures {
        let n = capacity as usize;
        Structures {
            buildings: sanitized(buildings),
            generation: vec![0; n],
            owner: vec![0; n],
            building: vec![0; n],
            tile: vec![(0, 0); n],
            hp: vec![0; n],
            free: (0..capacity).rev().collect(),
            work_left: vec![0; n],
            work_full: vec![0; n],
            upgrade_to: vec![None; n],
            upgrade_left: vec![0; n],
            upgrade_full: vec![0; n],
            crew: vec![0; n],
            growing: vec![false; n],
            larvae: vec![0; n],
            larva_clock: vec![0; n],
            vein_from: vec![crate::veins::UNLINKED; n],
            vein_linked: Vec::with_capacity(n),
            vein_cands: Vec::with_capacity(n),
            build_percent: build_percent.clamp(10, 1000),
            siege: vec![None; regiments],
            siege_gen: vec![0; regiments],
            work: vec![Work::Attack; regiments],
            siege_clock: vec![0; regiments],
            siege_gap: vec![Fx::ZERO; regiments],
            siege_stalls: vec![0; regiments],
            fallen: Vec::with_capacity(n),
            consumed: Vec::with_capacity(regiments),
            seen: vec![0; n],
            fog: false,
        }
    }

    /// The match's building types, indexed by [`BuildingId`].
    pub fn buildings(&self) -> &[BuildingStats] {
        &self.buildings
    }

    pub fn len(&self) -> usize {
        self.hp.len() - self.free.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn resolve(&self, h: StructureHandle) -> Option<usize> {
        let i = h.index as usize;
        (self.hp.get(i).is_some_and(|&hp| hp > 0) && self.generation[i] == h.generation)
            .then_some(i)
    }

    pub(crate) fn handle(&self, i: usize) -> StructureHandle {
        StructureHandle {
            index: i as u32,
            generation: self.generation[i],
        }
    }

    pub fn owner(&self, h: StructureHandle) -> Option<PlayerId> {
        self.resolve(h).map(|i| self.owner[i])
    }

    /// Building `h`'s slot, if it stands and is `player`'s.
    pub(crate) fn owned(&self, h: StructureHandle, player: PlayerId) -> Option<usize> {
        self.resolve(h).filter(|&i| self.owner[i] == player)
    }

    pub fn get(&self, h: StructureHandle) -> Option<StructureView> {
        self.resolve(h).map(|i| self.view(i))
    }

    fn view(&self, i: usize) -> StructureView {
        let b = &self.buildings[self.building[i] as usize];
        StructureView {
            handle: self.handle(i),
            owner: self.owner[i],
            building: self.building[i],
            tile: self.tile[i],
            width: b.width,
            depth: b.depth,
            hp: self.hp[i].div_ceil(HP_SCALE),
            max_hp: u32::from(b.hp),
            built_percent: if self.finished(i) {
                100
            } else {
                // Never 100 until it is.
                (99 - u64::from(self.work_left[i]) * 99 / u64::from(self.work_full[i].max(1))) as u8
            },
            upgrading: self.upgrade_to[i].map(|to| {
                let done = self.upgrade_full[i] - self.upgrade_left[i];
                (to, (done * 100 / self.upgrade_full[i].max(1)) as u8)
            }),
            growing: self.growing(i),
            seen: self.seen(i),
        }
    }

    /// Whether building slot `i` is finished.
    pub(crate) fn finished(&self, i: usize) -> bool {
        self.work_left[i] == 0
    }

    /// The slots of `owner`'s finished buildings, in index order.
    fn finished_of(&self, owner: PlayerId) -> impl Iterator<Item = usize> + '_ {
        self.standing()
            .filter(move |&i| self.owner[i] == owner && self.finished(i))
    }

    /// The slots of `owner`'s finished buildings whose flags have `flag`,
    /// in index order.
    pub(crate) fn finished_with(
        &self,
        owner: PlayerId,
        flag: u8,
    ) -> impl Iterator<Item = usize> + '_ {
        self.finished_of(owner)
            .filter(move |&i| self.stats_of(i).flags & flag != 0)
    }

    /// `owner`'s tier: the highest of their finished main buildings', 0
    /// with none.
    pub fn tier_of(&self, owner: PlayerId) -> u8 {
        self.finished_of(owner)
            .map(|i| self.stats_of(i))
            .filter(|b| b.role == Role::Main)
            .map(|b| b.tier.max(1))
            .max()
            .unwrap_or(0)
    }

    /// Whether building type `building` is `of` or one of its tier
    /// upgrades.
    pub fn grew_from(&self, mut building: BuildingId, of: BuildingId) -> bool {
        // Upgrades chain at most a few deep; the bound keeps a bad table
        // from looping.
        for _ in 0..self.buildings.len() {
            if building == of {
                return true;
            }
            match self
                .buildings
                .get(building as usize)
                .and_then(|b| b.upgrades_from)
            {
                Some(from) => building = from,
                None => return false,
            }
        }
        false
    }

    /// The building type that upgrades building type `building`, if any.
    pub fn upgrade_of(&self, building: BuildingId) -> Option<BuildingId> {
        self.buildings
            .iter()
            .position(|b| b.upgrades_from == Some(building))
            .map(|i| i as BuildingId)
    }

    /// The type of building slot `i`.
    pub(crate) fn kind_of(&self, i: usize) -> BuildingId {
        self.building[i]
    }

    /// The flags of building slot `i`'s type.
    pub(crate) fn flags_of(&self, i: usize) -> u8 {
        self.stats_of(i).flags
    }

    /// Whether a building stands in slot `i`.
    pub(crate) fn stands(&self, i: usize) -> bool {
        self.hp[i] > 0
    }

    /// `ticks` at Standard tempo, at the match's tempo.
    pub(crate) fn tempo_ticks(&self, ticks: u32) -> u32 {
        crate::conquest::at_tempo(ticks, self.build_percent)
    }

    /// Whether building slot `i` is going up on its own (see
    /// [`ConquestUnit::consumed`]).
    pub(crate) fn growing(&self, i: usize) -> bool {
        self.growing[i] && !self.finished(i)
    }

    /// Where what building slot `i` trains comes out: two tiles off the
    /// middle of its south side, or of its east, west or north side where
    /// that is blocked.
    pub(crate) fn door(&self, i: usize, nav: &Nav) -> FxVec2 {
        let [x, y, w, d] = self.footprint(i);
        let (mid_x, mid_y) = (
            Fx::from_int(x) + Fx::from_ratio(w, 2),
            Fx::from_int(y) + Fx::from_ratio(d, 2),
        );
        let off = Fx::from_int(2);
        let south = FxVec2::new(mid_x, Fx::from_int(y + d) + off);
        [
            FxVec2::new(Fx::from_int(x + w) + off, mid_y),
            FxVec2::new(Fx::from_int(x) - off, mid_y),
            FxVec2::new(mid_x, Fx::from_int(y) - off),
        ]
        .into_iter()
        .find(|&p| !nav.walkable(south) && nav.walkable(p))
        .unwrap_or(south)
    }

    /// Live buildings in index order.
    pub fn iter(&self) -> impl Iterator<Item = StructureView> + '_ {
        self.standing().map(|i| self.view(i))
    }

    /// The parties with a main building finished, and with one going up.
    pub(crate) fn mains(&self) -> Mains {
        let mut m = Mains::default();
        for i in self.standing() {
            if self.stats_of(i).role == Role::Main {
                let bit = 1 << self.owner[i];
                if self.finished(i) {
                    m.built |= bit;
                } else {
                    m.rising |= bit;
                }
            }
        }
        m
    }

    /// The slots of the buildings standing, in index order.
    pub(crate) fn standing(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.hp.len()).filter(|&i| self.hp[i] > 0)
    }

    pub(crate) fn stats_of(&self, i: usize) -> &BuildingStats {
        &self.buildings[self.building[i] as usize]
    }

    /// Building slot `i`'s footprint as a block, for sight.
    pub(crate) fn block(&self, i: usize) -> Block {
        let [x, y, w, d] = self.footprint(i);
        Block::square(
            footprint_centre([x, y, w, d]),
            Fx::from_ratio(w, 2),
            Fx::from_ratio(d, 2),
        )
    }

    /// How far building slot `i` sees (see [`Role::sight`]).
    pub(crate) fn sight(&self, i: usize) -> Fx {
        self.stats_of(i).role.sight()
    }

    pub(crate) fn owner_of(&self, i: usize) -> PlayerId {
        self.owner[i]
    }

    /// Play in fog of war from now on.
    pub(crate) fn fog(&mut self) {
        self.fog = true;
    }

    /// The parties that have seen building slot `i`: all of them outside
    /// fog of war.
    pub(crate) fn seen(&self, i: usize) -> u8 {
        self.seen[i]
    }

    /// Parties in `mask` have now seen building slot `i`.
    pub(crate) fn mark_seen(&mut self, i: usize, mask: u8) {
        self.seen[i] |= mask;
    }

    /// Centre of building slot `i`'s footprint.
    pub(crate) fn centre(&self, i: usize) -> FxVec2 {
        footprint_centre(self.footprint(i))
    }

    /// The footprint of building slot `i`: x, y, width, depth in tiles.
    pub(crate) fn footprint(&self, i: usize) -> [i32; 4] {
        let b = &self.buildings[self.building[i] as usize];
        let (x, y) = self.tile[i];
        [x, y, i32::from(b.width), i32::from(b.depth)]
    }

    /// The footprint of the standing building of `owner`'s nearest `here`
    /// whose flags have `flag` (a drop-off for what a worker carries).
    pub(crate) fn nearest_drop_off(
        &self,
        owner: PlayerId,
        flag: u8,
        here: FxVec2,
    ) -> Option<[i32; 4]> {
        self.finished_with(owner, flag)
            .map(|i| self.footprint(i))
            .min_by_key(|&foot| (nearest_point(foot, here) - here).length_sq_raw())
    }

    /// `owner`'s supply cap: what their finished buildings add, up to
    /// [`crate::economy::SUPPLY_LIMIT`].
    pub(crate) fn supply_cap(&self, owner: PlayerId) -> u32 {
        self.supply_of(owner).min(crate::economy::SUPPLY_LIMIT)
    }

    /// Whether building slot `i` is below its full hit points.
    pub(crate) fn hurt(&self, i: usize) -> bool {
        self.hp[i] < u32::from(self.stats_of(i).hp) * HP_SCALE
    }

    /// Building slot `i` regrows `per_second` hit points a second, up to
    /// its full.
    pub(crate) fn heal(&mut self, i: usize, per_second: u32) {
        let max = u32::from(self.stats_of(i).hp) * HP_SCALE;
        self.hp[i] = (self.hp[i] + per_second * HP_SCALE / TICKS_PER_SECOND).min(max);
    }

    /// Whether building slot `i` has a tier upgrade going on.
    pub(crate) fn upgrading(&self, i: usize) -> bool {
        self.upgrade_to[i].is_some()
    }

    /// Building slot `i`'s owner, if the building of generation
    /// `generation` still stands there.
    pub(crate) fn live_slot(&self, i: usize, generation: u16) -> Option<PlayerId> {
        (self.hp[i] > 0 && self.generation[i] == generation).then(|| self.owner[i])
    }

    /// What `owner`'s finished buildings add to their supply cap.
    pub(crate) fn supply_of(&self, owner: PlayerId) -> u32 {
        self.finished_of(owner)
            .map(|i| u32::from(self.stats_of(i).supply))
            .sum()
    }

    /// Whether a building of type `building` may stand with its north-west
    /// tile at `tile`: on the map, on ground regiments can walk, clear of
    /// other buildings (see `Nav::footing`), and with no regiment standing
    /// on it or node (`nodes`) within it.
    pub(crate) fn check(
        &self,
        building: BuildingId,
        tile: (i32, i32),
        nav: &Nav,
        regs: &Regiments,
        nodes: &[FxVec2],
    ) -> Result<(), Refusal> {
        let b = self
            .buildings
            .get(building as usize)
            .ok_or(Refusal::Unknown)?;
        let (w, d) = (i32::from(b.width), i32::from(b.depth));
        nav.footing([tile.0, tile.1, w, d])?;
        // No regiment's centre on ground it couldn't stand on once the
        // building is up.
        let margin = Fx::from_ratio(i32::from(CLEARANCE), CELLS_PER_TILE);
        let (lo, hi) = (
            FxVec2::from_ints(tile.0, tile.1) - FxVec2::new(margin, margin),
            FxVec2::from_ints(tile.0 + w, tile.1 + d) + FxVec2::new(margin, margin),
        );
        let near = |p: FxVec2| p.x > lo.x && p.y > lo.y && p.x < hi.x && p.y < hi.y;
        if (0..regs.alive.len()).any(|i| regs.alive[i] && near(regs.position[i])) {
            Err(Refusal::Crowded)
        } else if nodes.iter().any(|&at| near(at)) {
            // Workers must be able to stand at it.
            Err(Refusal::Node)
        } else {
            Ok(())
        }
    }

    /// Put up a finished building for `owner`. `None` when the pool is
    /// full; the caller has checked [`Structures::check`].
    pub(crate) fn spawn(
        &mut self,
        owner: PlayerId,
        building: BuildingId,
        tile: (i32, i32),
        nav: &mut Nav,
        regs: &Regiments,
    ) -> Option<StructureHandle> {
        let index = self.free.pop()?;
        let i = index as usize;
        self.owner[i] = owner;
        self.building[i] = building;
        self.tile[i] = tile;
        self.hp[i] = u32::from(self.buildings[building as usize].hp) * HP_SCALE;
        self.work_left[i] = 0;
        self.upgrade_to[i] = None;
        self.growing[i] = false;
        self.larvae[i] = Structures::whole_larvae(self.buildings[building as usize].flags);
        self.larva_clock[i] = 0;
        self.seen[i] = if self.fog { 1 << owner } else { u8::MAX };
        nav.set_built(self.footprint(i), true, regs);
        Some(self.handle(i))
    }

    /// Lay the foundation of a building for `owner`: it stands at
    /// [`FOUNDATION_PERCENT`] of its hit points until built. `None` when
    /// the pool is full; the caller has checked [`Structures::check`].
    pub(crate) fn found(
        &mut self,
        owner: PlayerId,
        building: BuildingId,
        tile: (i32, i32),
        nav: &mut Nav,
        regs: &Regiments,
    ) -> Option<StructureHandle> {
        let h = self.spawn(owner, building, tile, nav, regs)?;
        let i = h.index as usize;
        let b = self.buildings[building as usize];
        self.hp[i] = (u32::from(b.hp) * HP_SCALE * FOUNDATION_PERCENT / 100).max(1);
        self.work_full[i] = u32::from(b.build_ticks) * self.build_percent;
        self.work_left[i] = self.work_full[i];
        // A larvae building built from a foundation starts empty.
        self.larvae[i] = 0;
        Some(h)
    }

    /// Start building slot `i`'s tier upgrade to building type `to`.
    pub(crate) fn start_upgrade(&mut self, i: usize, to: BuildingId) {
        let ticks = self.tempo_ticks(u32::from(self.buildings[to as usize].build_ticks));
        self.upgrade_to[i] = Some(to);
        self.upgrade_full[i] = ticks;
        self.upgrade_left[i] = self.upgrade_full[i];
    }

    /// What building slot `i` cost to start: a foundation's full price, or
    /// its upgrade's, while going on; `None` when there is nothing to call
    /// off. Calling it off stops it.
    pub(crate) fn call_off(
        &mut self,
        i: usize,
        nav: &mut Nav,
        regs: &Regiments,
    ) -> Option<(u32, u32)> {
        let cost = |b: &BuildingStats| (u32::from(b.cost_grub), u32::from(b.cost_marrow));
        if let Some(to) = self.upgrade_to[i].take() {
            return Some(cost(&self.buildings[to as usize]));
        }
        if self.finished(i) {
            return None;
        }
        let paid = cost(self.stats_of(i));
        self.remove(i, nav, regs);
        Some(paid)
    }

    /// Take a building off the field; its ground is free again.
    pub(crate) fn despawn(&mut self, h: StructureHandle, nav: &mut Nav, regs: &Regiments) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        self.remove(i, nav, regs);
        true
    }

    /// Take every building in `all` away at once (a player put out of the
    /// match), freeing their ground together: much cheaper than one by one.
    pub(crate) fn despawn_all(&mut self, all: &[StructureHandle], nav: &mut Nav, regs: &Regiments) {
        let slots: Vec<usize> = all.iter().filter_map(|&h| self.resolve(h)).collect();
        let feet: Vec<[i32; 4]> = slots.iter().map(|&i| self.footprint(i)).collect();
        nav.set_built_all(&feet, false, regs);
        for i in slots {
            self.free_slot(i);
        }
    }

    /// Free building slot `i` (standing, or just brought to nothing).
    fn remove(&mut self, i: usize, nav: &mut Nav, regs: &Regiments) {
        nav.set_built(self.footprint(i), false, regs);
        self.free_slot(i);
    }

    /// Mark slot `i` free, its ground already given back.
    fn free_slot(&mut self, i: usize) {
        self.generation[i] = self.generation[i].wrapping_add(1);
        self.hp[i] = 0;
        self.work_left[i] = 0;
        self.upgrade_to[i] = None;
        self.free.push(i as u32);
    }

    /// Order regiment slot `r` to attack building `target`. The caller has
    /// checked the regiment is the issuer's and the building an enemy's.
    pub(crate) fn besiege(&mut self, regs: &mut Regiments, r: usize, target: StructureHandle) {
        self.send(regs, r, target, Work::Attack);
    }

    /// Send regiment slot `r` to building `target` to do `work` there. The
    /// caller has checked the regiment is the issuer's, and the building an
    /// enemy's to attack or the issuer's own to build.
    pub(crate) fn send(
        &mut self,
        regs: &mut Regiments,
        r: usize,
        target: StructureHandle,
        work: Work,
    ) {
        // Whatever it was doing, it stops to see to this.
        regs.halt(r);
        self.work[r] = work;
        self.siege[r] = Some(target);
        self.siege_gen[r] = regs.generation[r];
        self.siege_clock[r] = 0;
        self.siege_gap[r] = Fx::MAX;
        self.siege_stalls[r] = 0;
    }

    /// Regiment slot `r` was given another order: it leaves off attacking.
    pub(crate) fn stop(&mut self, r: usize) {
        self.siege[r] = None;
    }

    /// The building regiment slot `r` was sent to, and what to do there.
    pub(crate) fn job_of(&self, regs: &Regiments, r: usize) -> Option<(StructureHandle, Work)> {
        Some((self.siege_of(regs, r)?, self.work[r]))
    }

    /// The building regiment slot `r` was sent to, if any (and if both
    /// still stand).
    pub(crate) fn siege_of(&self, regs: &Regiments, r: usize) -> Option<StructureHandle> {
        self.siege[r].filter(|&s| {
            regs.alive[r] && regs.generation[r] == self.siege_gen[r] && self.resolve(s).is_some()
        })
    }

    /// One tick of regiments attacking buildings: close in, then hit.
    pub(crate) fn update(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        kinds: &[ConquestUnit],
        nav: &mut Nav,
    ) {
        self.fallen.clear();
        self.consumed.clear();
        self.upgrades();
        self.crew.fill(0);
        // A building that used up its worker grows as one builder would
        // build it.
        for s in 0..self.hp.len() {
            if self.hp[s] > 0 && self.growing(s) {
                self.build(s);
            }
        }
        for r in 0..self.siege.len() {
            let Some(target) = self.siege_of(regs, r) else {
                // No order, or the building or the regiment is gone.
                self.siege[r] = None;
                continue;
            };
            if regs.morale_state[r].broken() {
                self.siege[r] = None;
                continue;
            }
            // Fighting a regiment comes first.
            if regs.engaged[r].is_some() {
                continue;
            }
            self.siege_clock[r] = self.siege_clock[r].saturating_sub(1);
            // Standing (in reach or not) until its next blow or try.
            if self.siege_clock[r] > 0 && regs.target[r].is_none() {
                continue;
            }
            let s = target.index as usize;
            let unit = &units[regs.unit[r] as usize];
            let way = Approach::of(regs, unit, r, self.footprint(s));
            let building = self.work[r] == Work::Build;
            let reach = if unit.range > Fx::ZERO && !building {
                unit.range
            } else {
                way.reach()
            };
            if way.gap > reach {
                // Close in, unless already on the way.
                if regs.target[r].is_none() {
                    self.close_in(regs, r, &way);
                }
                continue;
            }
            way.arrive(regs, r);
            if building
                && kinds
                    .get(usize::from(regs.unit[r]))
                    .is_some_and(|k| k.consumed)
            {
                // Used up to start it growing, if nothing has yet.
                if !self.finished(s) && !self.growing(s) {
                    self.growing[s] = true;
                    let worker = regs.handle(r);
                    self.consumed.push((worker, target));
                    regs.despawn(worker);
                }
                self.siege[r] = None;
                continue;
            }
            if building {
                if !self.build(s) {
                    // Built and whole: its work is done.
                    self.siege[r] = None;
                }
                continue;
            }
            if self.siege_clock[r] > 0 {
                continue;
            }
            let breaker = kinds
                .get(usize::from(regs.unit[r]))
                .map_or(Breaker::Regiment, |k| k.breaker);
            self.blow(regs, unit, breaker, r, s);
            if self.hp[s] == 0 {
                self.fallen.push(target);
                self.remove(s, nav, regs);
                self.siege[r] = None;
            }
        }
    }

    /// One builder's tick on building slot `s`: build it, or once it is up,
    /// mend it. Returns false when there is nothing left to do (or the crew
    /// is full, which the builder waits out).
    fn build(&mut self, s: usize) -> bool {
        let max = u32::from(self.stats_of(s).hp) * HP_SCALE;
        if self.crew[s] >= CREW {
            return true;
        }
        if self.finished(s) {
            if self.hp[s] >= max {
                return false;
            }
            self.crew[s] += 1;
            self.heal(s, REPAIR_HP);
            return true;
        }
        let hand = if self.crew[s] == 0 {
            FIRST_HAND
        } else {
            OTHER_HAND
        };
        self.crew[s] += 1;
        // Hit points rise with the work done, from the foundation's to
        // full, so damage taken while building stays taken.
        let full = u64::from(self.work_full[s].max(1));
        let rise = u64::from(max) * u64::from(100 - FOUNDATION_PERCENT) / 100;
        let risen = |left: u32| (rise * (full - u64::from(left)) / full) as u32;
        let before = risen(self.work_left[s]);
        self.work_left[s] = self.work_left[s].saturating_sub(hand);
        self.hp[s] = (self.hp[s] + risen(self.work_left[s]) - before).min(max);
        true
    }

    /// Tier upgrades go on by themselves; one done turns the building into
    /// its upgrade, keeping the damage it has taken.
    fn upgrades(&mut self) {
        for i in 0..self.hp.len() {
            let Some(to) = self.upgrade_to[i].filter(|_| self.hp[i] > 0) else {
                continue;
            };
            self.upgrade_left[i] -= 1;
            if self.upgrade_left[i] > 0 {
                continue;
            }
            let gain = u32::from(self.buildings[to as usize].hp)
                .saturating_sub(u32::from(self.stats_of(i).hp));
            self.hp[i] += gain * HP_SCALE;
            self.building[i] = to;
            self.upgrade_to[i] = None;
        }
    }

    /// Send regiment slot `r` up to its building (see [`Approach::walk`]).
    /// After [`STALLS`] tries in a row that brought it no nearer (no way
    /// there), it gives up.
    fn close_in(&mut self, regs: &mut Regiments, r: usize, way: &Approach) {
        let gap = way.gap;
        if gap >= self.siege_gap[r] {
            self.siege_stalls[r] += 1;
            if self.siege_stalls[r] >= STALLS {
                self.siege[r] = None;
                return;
            }
        } else {
            self.siege_stalls[r] = 0;
        }
        self.siege_gap[r] = gap;
        way.walk(regs, r);
        self.siege_clock[r] = RETRY_TICKS;
    }

    /// Regiment slot `r` (of type `unit`) strikes building slot `s`: a
    /// volley each reload, or its front rank's blows once a second.
    fn blow(&mut self, regs: &Regiments, unit: &UnitStats, breaker: Breaker, r: usize, s: usize) {
        let (base, every) = if unit.range > Fx::ZERO {
            (
                u64::from(regs.models[r]) * u64::from(unit.missile_damage) * u64::from(HP_SCALE),
                unit.reload_ticks.max(1),
            )
        } else {
            // As against regiments: who can reach, and room to swing.
            (
                unit.front_fighters_percent(regs.files[r], regs.models[r])
                    * u64::from(unit.melee_damage)
                    * unit.weapon_room_percent(regs.formation[r])
                    / 100,
                TICKS_PER_SECOND as u16,
            )
        };
        self.siege_clock[r] = every;
        let armour = self.buildings[self.building[s] as usize].armour;
        let damage = base * u64::from(breaker.percent(armour)) / 100;
        self.hp[s] = self.hp[s].saturating_sub(u32::try_from(damage).unwrap_or(u32::MAX));
    }

    pub(crate) fn hash_into(&self, regs: &Regiments, h: &mut StateHasher) {
        h.u32(self.buildings.len() as u32);
        for b in &self.buildings {
            b.hash_into(h);
        }
        for i in 0..self.hp.len() {
            h.u16(self.generation[i]);
            h.u32(self.hp[i]);
            if self.hp[i] == 0 {
                continue;
            }
            h.u8(self.owner[i]);
            h.u16(self.building[i]);
            h.i32(self.tile[i].0);
            h.i32(self.tile[i].1);
            h.u32(self.work_left[i]);
            if self.work_left[i] > 0 {
                h.u32(self.work_full[i]);
                h.u8(u8::from(self.growing[i]));
            }
            h.u8(self.seen[i]);
            if self.stats_of(i).flags & LARVAE != 0 {
                h.u8(self.larvae[i]);
                h.u32(self.larva_clock[i]);
            }
            if let Some(to) = self.upgrade_to[i] {
                h.u16(to);
                h.u32(self.upgrade_left[i]);
                h.u32(self.upgrade_full[i]);
            }
        }
        h.u32(self.build_percent);
        for r in 0..self.siege.len() {
            match self.siege_of(regs, r) {
                Some(s) => {
                    h.u8(1 + self.work[r] as u8);
                    h.u32(s.index);
                    h.u16(s.generation);
                    h.u16(self.siege_clock[r]);
                    h.i32(self.siege_gap[r].raw());
                    h.u8(self.siege_stalls[r]);
                }
                None => h.u8(0),
            }
        }
    }
}
