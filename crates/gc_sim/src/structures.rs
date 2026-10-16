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

use crate::conquest::ConquestUnit;
use crate::nav::{CELLS_PER_TILE, CLEARANCE, Nav};
use crate::regiments::{Order, Regiments};
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

impl Breaker {
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

/// Why a building can't go up where it was asked to.
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
    /// A Grub seep or Marrow vein is there.
    Node,
}

/// Bits of [`BuildingStats::flags`].
pub const DROP_OFF_GRUB: u8 = 1;
pub const DROP_OFF_MARROW: u8 = 2;
/// Choir: powers buildings around it.
pub const POWERS_FIELD: u8 = 4;
/// Its attack can hit flying units.
pub const ANTI_AIR: u8 = 8;

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
    /// `DROP_OFF_*`, [`POWERS_FIELD`] and [`ANTI_AIR`] bits.
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
}

impl StructureView {
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
fn nearest_point([x, y, w, d]: [i32; 4], here: FxVec2) -> FxVec2 {
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

/// Every building on the field, and which regiments are attacking which.
pub struct Structures {
    buildings: Vec<BuildingStats>,
    /// Each regiment type's class against buildings, by [`crate::UnitId`].
    breakers: Vec<Breaker>,
    generation: Vec<u16>,
    owner: Vec<PlayerId>,
    building: Vec<BuildingId>,
    tile: Vec<(i32, i32)>,
    /// Hit points left, in hundredths; 0 for a free slot.
    hp: Vec<u32>,
    free: Vec<u32>,
    /// By regiment slot: the building it is attacking, and the generation
    /// of the regiment the order was given to (a slot reused by a new
    /// regiment doesn't inherit it).
    siege: Vec<Option<StructureHandle>>,
    siege_gen: Vec<u16>,
    /// By regiment slot: ticks until its next blow, or its next try to
    /// reach the building.
    siege_clock: Vec<u16>,
    /// By regiment slot: how far it was from the building when it last set
    /// off for it, and how many times in a row that brought it no nearer.
    siege_gap: Vec<Fx>,
    siege_stalls: Vec<u8>,
    /// Buildings brought down in the last tick, for the events.
    pub(crate) fallen: Vec<StructureHandle>,
}

impl Structures {
    pub(crate) fn new(
        buildings: &[BuildingStats],
        units: &[ConquestUnit],
        capacity: u32,
        regiments: usize,
    ) -> Structures {
        let n = capacity as usize;
        Structures {
            buildings: buildings.iter().map(|b| b.sanitized()).collect(),
            breakers: units.iter().map(|u| u.breaker).collect(),
            generation: vec![0; n],
            owner: vec![0; n],
            building: vec![0; n],
            tile: vec![(0, 0); n],
            hp: vec![0; n],
            free: (0..capacity).rev().collect(),
            siege: vec![None; regiments],
            siege_gen: vec![0; regiments],
            siege_clock: vec![0; regiments],
            siege_gap: vec![Fx::ZERO; regiments],
            siege_stalls: vec![0; regiments],
            fallen: Vec::with_capacity(n),
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

    fn handle(&self, i: usize) -> StructureHandle {
        StructureHandle {
            index: i as u32,
            generation: self.generation[i],
        }
    }

    pub fn owner(&self, h: StructureHandle) -> Option<PlayerId> {
        self.resolve(h).map(|i| self.owner[i])
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
        }
    }

    /// Live buildings in index order.
    pub fn iter(&self) -> impl Iterator<Item = StructureView> + '_ {
        self.standing().map(|i| self.view(i))
    }

    /// The slots of the buildings standing, in index order.
    fn standing(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.hp.len()).filter(|&i| self.hp[i] > 0)
    }

    fn stats_of(&self, i: usize) -> &BuildingStats {
        &self.buildings[self.building[i] as usize]
    }

    /// The footprint of building slot `i`: x, y, width, depth in tiles.
    fn footprint(&self, i: usize) -> [i32; 4] {
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
        self.standing()
            .filter(|&i| self.owner[i] == owner && self.stats_of(i).flags & flag != 0)
            .map(|i| self.footprint(i))
            .min_by_key(|&foot| (nearest_point(foot, here) - here).length_sq_raw())
    }

    /// What `owner`'s standing buildings add to their supply cap.
    pub(crate) fn supply_of(&self, owner: PlayerId) -> u32 {
        self.standing()
            .filter(|&i| self.owner[i] == owner)
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
        nav.set_built(self.footprint(i), true, regs);
        Some(self.handle(i))
    }

    /// Take a building off the field; its ground is free again.
    pub(crate) fn despawn(&mut self, h: StructureHandle, nav: &mut Nav, regs: &Regiments) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        self.remove(i, nav, regs);
        true
    }

    /// Free building slot `i` (standing, or just brought to nothing).
    fn remove(&mut self, i: usize, nav: &mut Nav, regs: &Regiments) {
        nav.set_built(self.footprint(i), false, regs);
        self.generation[i] = self.generation[i].wrapping_add(1);
        self.hp[i] = 0;
        self.free.push(i as u32);
    }

    /// Order regiment slot `r` to attack building `target`. The caller has
    /// checked the regiment is the issuer's and the building an enemy's.
    pub(crate) fn besiege(&mut self, regs: &mut Regiments, r: usize, target: StructureHandle) {
        // Whatever it was doing, it stops to see to this.
        regs.halt(r);
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

    /// The building regiment slot `r` is attacking, if any (and if both
    /// still stand).
    pub(crate) fn siege_of(&self, regs: &Regiments, r: usize) -> Option<StructureHandle> {
        self.siege[r].filter(|&s| {
            regs.alive[r] && regs.generation[r] == self.siege_gen[r] && self.resolve(s).is_some()
        })
    }

    /// One tick of regiments attacking buildings: close in, then hit.
    pub(crate) fn update(&mut self, regs: &mut Regiments, units: &[UnitStats], nav: &mut Nav) {
        self.fallen.clear();
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
            let reach = if unit.range > Fx::ZERO {
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
            if self.siege_clock[r] > 0 {
                continue;
            }
            self.blow(regs, unit, r, s);
            if self.hp[s] == 0 {
                self.fallen.push(target);
                self.remove(s, nav, regs);
                self.siege[r] = None;
            }
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
    fn blow(&mut self, regs: &Regiments, unit: &UnitStats, r: usize, s: usize) {
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
        let breaker = self
            .breakers
            .get(regs.unit[r] as usize)
            .copied()
            .unwrap_or_default();
        let armour = self.buildings[self.building[s] as usize].armour;
        let damage = base * u64::from(breaker.percent(armour)) / 100;
        self.hp[s] = self.hp[s].saturating_sub(u32::try_from(damage).unwrap_or(u32::MAX));
    }

    pub(crate) fn hash_into(&self, regs: &Regiments, h: &mut StateHasher) {
        h.u32(self.buildings.len() as u32);
        for b in &self.buildings {
            b.hash_into(h);
        }
        h.u32(self.breakers.len() as u32);
        for b in &self.breakers {
            h.u8(*b as u8);
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
        }
        for r in 0..self.siege.len() {
            match self.siege_of(regs, r) {
                Some(s) => {
                    h.u8(1);
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
