//! What the ground does in a fight (docs/05-combat.md, "Terrain"): high
//! ground and fords, and the woods and tall grass regiments hide in
//! (docs/20-ambushes.md; the hiding itself is [`crate::vision`]).
//!
//! The match hands the simulation one height per tile, taken at the tile's
//! centre, and the water line. From those:
//! - **High ground:** a regiment standing at least [`HIGH_GROUND`] above its
//!   foe deals [`DOWNHILL_MELEE_PERCENT`] melee damage, and an archer that
//!   far above its target shoots [`DOWNHILL_RANGE_PERCENT`] as far.
//! - **Fords:** a regiment whose centre is in water wades at
//!   [`WADING_SPEED_PERCENT`] of its pace, takes [`WADING_DAMAGE_PERCENT`]
//!   melee damage, and can't charge or be charged.
//!
//! - **Cover:** each tile is open, tall grass or wood ([`Cover`]).
//! - **Woods:** a regiment whose centre is in a wood marches at
//!   [`WOODS_SPEED_PERCENT`] of its pace ([`WOODS_HEAVY_SPEED_PERCENT`] for
//!   heavy troops: cavalry and monsters), and the trees catch
//!   [`TREES_CATCH_PERCENT`] of the arrows shot out of a wood, and as many
//!   again of those shot at a regiment standing in one.
//!
//! - **Slopes** (docs/32-changing-land.md, "Reading the ground"): a
//!   regiment climbing marches slower, by [`CLIMB_SLOW_PERCENT`] of its pace
//!   per tile of height it climbs per tile, at most [`CLIMB_SLOW_MAX`]; a
//!   charge hitting home from high ground lands [`CHARGE_DOWNHILL_PERCENT`]
//!   as hard, and one up onto high ground [`CHARGE_UPHILL_PERCENT`].
//! - **Footing:** each tile is firm or soft ([`Footing`]): bog and wet
//!   woodland. A regiment in soft ground marches at [`SOFT_SPEED_PERCENT`]
//!   ([`SOFT_HEAVY_SPEED_PERCENT`] for heavy troops), and nobody charges out
//!   of it or into it ([`Terrain::bears_charge`]).
//!
//! - **Soaking** (docs/32-changing-land.md, "Rain soaks the ground"):
//!   low, flat, open ground near water turns soft in long rain and firms
//!   up again in a dry spell ([`Terrain::soak`]), nearest the water first.
//!
//! - **Growth:** what grows on each tile, for building and clearing
//!   ([`Growth`], docs/32-changing-land.md): scrub, woods and thicket must
//!   be cleared before a building can rise there. Clearing changes the
//!   tile at runtime ([`Terrain::clear`]); its cover becomes open.
//!
//! Heights are integers ([`HEIGHT_UNITS`] per tile), so every machine
//! agrees. A match with no heights is flat, dry ground; one with no cover
//! has nowhere to hide.

use alloc::sync::Arc;
use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2};

use crate::Mass;
use crate::sky::WET_FULL;

/// Height units per tile of height.
pub const HEIGHT_UNITS: u32 = 256;
/// How much higher a regiment must stand than its foe to fight downhill:
/// half a tile.
pub const HIGH_GROUND: i32 = HEIGHT_UNITS as i32 / 2;
/// Melee damage dealt fighting downhill, percent.
pub const DOWNHILL_MELEE_PERCENT: u64 = 110;
/// Shooting range from high ground, percent.
pub const DOWNHILL_RANGE_PERCENT: i32 = 115;
/// Marching pace in water, percent.
pub const WADING_SPEED_PERCENT: i32 = 60;
/// Melee damage taken standing in water, percent.
pub const WADING_DAMAGE_PERCENT: u64 = 120;
/// Marching pace in woods, percent, for soldiers on foot and swarms.
pub const WOODS_SPEED_PERCENT: i32 = 85;
/// Marching pace in woods, percent, for heavy troops (cavalry and
/// monsters), who have to pick their way between the trunks.
pub const WOODS_HEAVY_SPEED_PERCENT: i32 = 70;
/// Arrows the trees catch, percent, at each end of a volley's flight that
/// lies in a wood (both ends: about half the volley).
pub const TREES_CATCH_PERCENT: u64 = 30;

/// Pace lost climbing, percent, per tile of height climbed per tile.
pub const CLIMB_SLOW_PERCENT: i32 = 50;
/// The most pace a climb can take, percent.
pub const CLIMB_SLOW_MAX: i32 = 40;
/// Charge bonus hitting home from high ground (see [`Terrain::downhill`]),
/// percent of the regiment's own.
pub const CHARGE_DOWNHILL_PERCENT: u32 = 150;
/// Charge bonus hitting home up onto high ground, percent.
pub const CHARGE_UPHILL_PERCENT: u32 = 50;
/// Marching pace in soft ground, percent, for soldiers on foot and swarms.
pub const SOFT_SPEED_PERCENT: i32 = 75;
/// Marching pace in soft ground, percent, for heavy troops.
pub const SOFT_HEAVY_SPEED_PERCENT: i32 = 60;

/// Ground soaks only this far above the water line: half a tile.
pub const SOAK_RISE: i32 = HEIGHT_UNITS as i32 / 2;
/// ... only where it is flat: no neighbour on land this much higher or
/// lower.
pub const SOAK_SLOPE: i32 = HEIGHT_UNITS as i32 / 2;
/// ... and only this many tiles from water.
pub const SOAK_REACH: u8 = 5;
/// How wet the ground must be before a tile goes soft: `SOAK_FROM`, and
/// `SOAK_PER_TILE` more for each tile from water, and up to `SOAK_HIGH`
/// more for its height above it (at [`SOAK_RISE`]).
const SOAK_FROM: u32 = 150;
const SOAK_PER_TILE: u32 = 150;
const SOAK_HIGH: u32 = 300;
/// How long the rubble of a fallen building stays soft underfoot, in ticks:
/// three minutes.
pub const RUBBLE_TICKS: u16 = 180 * crate::TICKS_PER_SECOND as u16;
/// A soaked tile firms up again once the ground is this much drier than
/// it took to soak it, so tiles don't flicker as the rain eases.
pub const SOAK_DRY_MARGIN: u16 = 100;

/// How a tile holds a soldier's feet (see [`Terrain::soft`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Footing {
    /// Ground that bears a charge.
    Firm = 0,
    /// Bog and wet woodland (mud): slow going, and no charges.
    Soft = 1,
}

impl Footing {
    /// `footing` as a layer for a map `tiles` a side: itself when it has one
    /// byte per tile and any soft ground, else empty (firm everywhere).
    /// The sim, the replay and the map all keep the layer by this rule.
    pub fn layer(footing: &[u8], tiles: u32) -> &[u8] {
        let n = tiles as usize * tiles as usize;
        if n > 0
            && footing.len() == n
            && footing
                .iter()
                .any(|&f| Footing::from_u8(f) != Footing::Firm)
        {
            footing
        } else {
            &[]
        }
    }

    /// The footing a byte stands for (anything unknown is firm).
    pub fn from_u8(v: u8) -> Footing {
        match v {
            1 => Footing::Soft,
            _ => Footing::Firm,
        }
    }
}

/// What covers a tile, for hiding (see [`crate::vision`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Cover {
    Open = 0,
    /// Hides regiments standing still.
    Grass = 1,
    /// Hides regiments standing or marching.
    Wood = 2,
}

impl Cover {
    /// The cover a map byte stands for; anything unknown is open ground.
    pub fn from_u8(v: u8) -> Cover {
        match v {
            1 => Cover::Grass,
            2 => Cover::Wood,
            _ => Cover::Open,
        }
    }
}

/// What grows on a tile, for building on it and clearing it
/// (docs/32-changing-land.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Growth {
    /// Nothing to clear: buildings can go up here.
    Open = 0,
    /// Brush and young growth: quick to clear (what clearing it pays is the
    /// worker's `ClearStyle`).
    Scrub = 1,
    /// Trees: slower, and most workers get a little timber from them.
    Woods = 2,
    /// Woods no one marches through: slowest, and once cleared, open to
    /// walking (it stands on the walkable grid like a building).
    Thicket = 3,
}

impl Growth {
    /// The growth a map byte stands for; anything unknown is open ground.
    pub fn from_u8(v: u8) -> Growth {
        match v {
            1 => Growth::Scrub,
            2 => Growth::Woods,
            3 => Growth::Thicket,
            _ => Growth::Open,
        }
    }
}

/// The map's heights, water and cover, as the match setup gives them.
#[derive(Clone, Copy, Debug)]
pub struct Ground<'a> {
    /// Ground height at each tile's centre, row by row, in
    /// [`HEIGHT_UNITS`] per tile. Empty means flat; otherwise there must be
    /// one per tile of the map, or the ground is taken as flat.
    pub heights: &'a [u16],
    /// Ground lower than this is water (deep water is also blocked in
    /// the nav grid; the rest is fords and shallows).
    pub water_level: u16,
    /// What covers each tile, row by row, as [`Cover`] bytes. Empty means
    /// open ground everywhere; otherwise there must be one per tile of the
    /// map, or the map is taken as having no cover.
    pub cover: &'a [u8],
    /// What grows on each tile, row by row, as [`Growth`] bytes. Empty
    /// means it follows the cover: woods where there are woods, open
    /// elsewhere. Otherwise there must be one per tile of the map (as for
    /// `cover`), or the map is taken as having none.
    pub growth: &'a [u8],
    /// How each tile holds a soldier's feet, row by row, as [`Footing`]
    /// bytes. Empty means firm everywhere; otherwise there must be one per
    /// tile of the map, or the map is taken as firm.
    pub footing: &'a [u8],
}

impl Ground<'static> {
    /// Flat, dry, open ground: no terrain effects.
    pub const FLAT: Ground<'static> = Ground {
        heights: &[],
        water_level: 0,
        cover: &[],
        growth: &[],
        footing: &[],
    };
}

/// The ground during a match. Built when the match starts; clearing
/// changes its growth and cover ([`Terrain::clear`]), and rain soaks its
/// footing ([`Terrain::soak`]).
///
/// The layers are shared (a scratch world shares its match's), so cloning
/// one is cheap; only clearing copies the cover and growth, and soaking
/// changes nothing but a count.
#[derive(Clone)]
pub struct Terrain {
    tiles: i32,
    height: Arc<[u16]>,
    water_level: u16,
    /// Map edge in tiles when the map has cover, else 0.
    cover_tiles: i32,
    cover: Arc<Vec<u8>>,
    /// One [`Growth`] byte per tile when the map has cover, else empty.
    growth: Arc<Vec<u8>>,
    /// One [`Footing`] byte per tile of `footing_tiles` a side, or empty:
    /// firm everywhere. The ground as the map made it: soaking is apart.
    footing: Arc<[u8]>,
    footing_tiles: i32,
    /// The tiles rain can soak, as (index, how wet the ground must be),
    /// the soonest first; the first `soaked` of them are soft now.
    soak: Arc<[(u32, u16)]>,
    /// How wet each tile of the map must be to soak, or 0 if it never
    /// does (empty when none does): where a tile is in `soak`.
    soak_need: Arc<[u16]>,
    soaked: u32,
    /// The rubble of buildings brought down: tile index and ticks left,
    /// sorted by index. Soft ground until it settles.
    rubble: Vec<(u32, u16)>,
    /// How many times a tile has come to be rubble or stopped being, so
    /// readers of the soft ground know when to look again.
    rubble_moves: u32,
    /// Map edge in tiles, which the rubble is indexed by (`tiles` is 0 on
    /// flat ground).
    side: i32,
    /// Height units a flood has raised the water above `water_level`
    /// (see [`crate::flood`]).
    rise: u16,
    /// Whether the water is frozen (a winter with no flood, see
    /// [`crate::sky::Season`]): troops cross it as dry ground.
    frozen: bool,
    /// A running digest of every change made to the ground mid-match, for
    /// the state hash (the ground as the match started is the setup's).
    changes: u64,
}

/// The index of tile `(x, y)` in a layer `side` tiles a side, row by row;
/// `None` off it or for an empty layer (`side` 0).
fn index_in(side: i32, x: i32, y: i32) -> Option<usize> {
    ((0..side).contains(&x) && (0..side).contains(&y)).then(|| (y * side + x) as usize)
}

impl Terrain {
    pub(crate) fn new(ground: &Ground, map_tiles: u32) -> Terrain {
        let n = map_tiles as usize;
        let (cover_tiles, cover) = if n > 0 && ground.cover.len() == n * n {
            (map_tiles as i32, ground.cover.to_vec())
        } else {
            (0, Vec::new())
        };
        let growth = if cover.is_empty() {
            Vec::new()
        } else if ground.growth.len() == n * n {
            ground
                .growth
                .iter()
                .map(|&g| Growth::from_u8(g) as u8)
                .collect()
        } else {
            let woods = |&c: &u8| Cover::from_u8(c) == Cover::Wood;
            cover
                .iter()
                .map(|c| if woods(c) { Growth::Woods as u8 } else { 0 })
                .collect()
        };
        let footing: Vec<u8> = (Footing::layer(ground.footing, map_tiles).iter())
            .map(|&f| Footing::from_u8(f) as u8)
            .collect();
        let footing_tiles = if footing.is_empty() {
            0
        } else {
            map_tiles as i32
        };
        let flat = n == 0 || ground.heights.len() != n * n;
        let mut terrain = Terrain {
            tiles: if flat { 0 } else { map_tiles as i32 },
            height: if flat {
                Arc::from([])
            } else {
                ground.heights.into()
            },
            water_level: if flat { 0 } else { ground.water_level },
            cover_tiles,
            cover: Arc::new(cover),
            growth: Arc::new(growth),
            footing: footing.into(),
            footing_tiles,
            soak: Arc::from([]),
            soak_need: Arc::from([]),
            soaked: 0,
            rubble: Vec::with_capacity(64),
            rubble_moves: 0,
            side: map_tiles as i32,
            rise: 0,
            frozen: false,
            changes: 0,
        };
        let soak = terrain.soakable();
        if !soak.is_empty() {
            let mut need = alloc::vec![0; n * n];
            for &(i, wet) in &soak {
                need[i as usize] = wet;
            }
            terrain.soak = soak.into();
            terrain.soak_need = need.into();
        }
        terrain
    }

    /// The tiles rain can soak, the soonest first: firm, open (or grassy)
    /// ground with nothing growing that needs clearing, flat, at most
    /// [`SOAK_RISE`] above the water and [`SOAK_REACH`] tiles from it.
    fn soakable(&self) -> Vec<(u32, u16)> {
        let n = self.tiles;
        if n == 0 || self.water_level == 0 {
            return Vec::new();
        }
        let h = |x: i32, y: i32| i32::from(self.height[(y * n + x) as usize]);
        let water = i32::from(self.water_level);
        // Tiles from the nearest water, counting diagonal steps as one, out
        // to the reach, spreading from the water's edge.
        let mut from_water = alloc::vec![u8::MAX; (n * n) as usize];
        let mut edge: Vec<(i32, i32)> = Vec::new();
        for y in 0..n {
            for x in 0..n {
                if h(x, y) < water {
                    from_water[(y * n + x) as usize] = 0;
                    let shore = crate::nav::DIRS.iter().any(|(dx, dy)| {
                        index_in(n, x + dx, y + dy).is_some_and(|_| h(x + dx, y + dy) >= water)
                    });
                    if shore {
                        edge.push((x, y));
                    }
                }
            }
        }
        let mut next = Vec::new();
        for step in 1..=SOAK_REACH {
            for &(x, y) in &edge {
                for (dx, dy) in crate::nav::DIRS {
                    let (nx, ny) = (x + dx, y + dy);
                    if let Some(i) = index_in(n, nx, ny).filter(|&i| from_water[i] == u8::MAX) {
                        from_water[i] = step;
                        next.push((nx, ny));
                    }
                }
            }
            core::mem::swap(&mut edge, &mut next);
            next.clear();
        }
        let mut soak = Vec::new();
        for y in 0..n {
            for x in 0..n {
                let i = (y * n + x) as usize;
                let rise = h(x, y) - water;
                let near = from_water[i];
                if !(0..=SOAK_RISE).contains(&rise) || !(1..=SOAK_REACH).contains(&near) {
                    continue;
                }
                let open = self
                    .tile_of(x, y)
                    .is_none_or(|t| Cover::from_u8(self.cover[t]) != Cover::Wood)
                    && matches!(self.growth(x, y), Growth::Open | Growth::Scrub);
                let firm = index_in(self.footing_tiles, x, y)
                    .is_none_or(|t| Footing::from_u8(self.footing[t]) == Footing::Firm);
                // Flat towards the land around it (the bank may drop into
                // the water).
                let flat = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().all(|(dx, dy)| {
                    let (nx, ny) = ((x + dx).clamp(0, n - 1), (y + dy).clamp(0, n - 1));
                    h(nx, ny) < water || (h(nx, ny) - h(x, y)).abs() <= SOAK_SLOPE
                });
                if open && firm && flat {
                    let wet = SOAK_FROM
                        + SOAK_PER_TILE * u32::from(near)
                        + SOAK_HIGH * rise as u32 / SOAK_RISE as u32;
                    soak.push((i as u32, wet.min(u32::from(WET_FULL)) as u16));
                }
            }
        }
        soak.sort_unstable_by_key(|&(i, wet)| (wet, i));
        soak
    }

    /// Whether ground `wet` soaks the next tile in line.
    fn wets_next(&self, wet: u16) -> bool {
        (self.soak.get(self.soaked as usize)).is_some_and(|&(_, need)| need <= wet)
    }

    /// Whether ground `wet` has dried the last soaked tile
    /// [`SOAK_DRY_MARGIN`] below its need.
    fn dries_last(&self, wet: u16) -> bool {
        (self.soaked as usize)
            .checked_sub(1)
            .is_some_and(|k| self.soak[k].1 > wet.saturating_add(SOAK_DRY_MARGIN))
    }

    /// Soak the ground to `wet` (see [`WET_FULL`]): tiles that need no
    /// more than that go soft, the nearest the water first, and soaked
    /// ones the ground has dried [`SOAK_DRY_MARGIN`] below their need firm
    /// up again, the furthest first. Only a count changes: cheap enough
    /// for every tick.
    pub(crate) fn soak(&mut self, wet: u16) {
        while self.wets_next(wet) {
            self.soaked += 1;
        }
        while self.dries_last(wet) {
            self.soaked -= 1;
        }
    }

    /// How many tiles rain has soaked soft (0 when dry): it changes
    /// whenever the soaked ground does.
    pub fn soaked(&self) -> u32 {
        self.soaked
    }

    /// The tiles rain soaks, as (column, row), in the order it soaks them:
    /// the first [`Terrain::soaked`] of them are soft now.
    pub fn soak_order(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let n = self.tiles;
        (self.soak.iter()).map(move |&(i, _)| (i as i32 % n, i as i32 / n))
    }

    /// A building stood on `[x, y, w, d]` and is brought down: its ground is
    /// rubble for [`RUBBLE_TICKS`] (again, from now, where it still lay).
    pub(crate) fn spread_rubble(&mut self, [x0, y0, w, d]: [i32; 4]) {
        for y in y0..y0 + d {
            for x in x0..x0 + w {
                let Some(i) = index_in(self.side, x, y) else {
                    continue;
                };
                let i = i as u32;
                match self.rubble.binary_search_by_key(&i, |&(t, _)| t) {
                    Ok(at) => self.rubble[at].1 = RUBBLE_TICKS,
                    Err(at) => {
                        self.rubble.insert(at, (i, RUBBLE_TICKS));
                        self.rubble_moves = self.rubble_moves.wrapping_add(1);
                    }
                }
            }
        }
    }

    /// One tick on: rubble settles, and firm ground it was.
    pub(crate) fn settle_rubble(&mut self) {
        let before = self.rubble.len();
        self.rubble.retain_mut(|(_, left)| {
            *left -= 1;
            *left > 0
        });
        if self.rubble.len() != before {
            self.rubble_moves = self.rubble_moves.wrapping_add(1);
        }
    }

    /// The rubble lying now: tile index and ticks left, by index. Empty
    /// until a building falls.
    pub fn rubble(&self) -> &[(u32, u16)] {
        &self.rubble
    }

    /// A count that changes whenever a tile comes to be rubble or stops
    /// being (with [`Terrain::soaked`], what changes the soft ground).
    pub fn rubble_moves(&self) -> u32 {
        self.rubble_moves
    }

    /// Whether the tile `p` is on is rubble (see [`Terrain::rubble`]).
    pub fn rubbled(&self, p: FxVec2) -> bool {
        self.rubbled_at(p.x.floor_int(), p.y.floor_int())
    }

    fn rubbled_at(&self, x: i32, y: i32) -> bool {
        index_in(self.side, x, y).is_some_and(|i| {
            self.rubble
                .binary_search_by_key(&(i as u32), |&(t, _)| t)
                .is_ok()
        })
    }

    /// The tiles rain has soaked soft, as (column, row).
    pub fn soaked_tiles(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.soak_order().take(self.soaked as usize)
    }

    /// The tile `p` is on, as an index into the cover and growth layers;
    /// `None` off the map or with no cover.
    fn tile_of(&self, x: i32, y: i32) -> Option<usize> {
        index_in(self.cover_tiles, x, y)
    }

    /// What grows on the tile `(x, y)` (open off the map or with no cover).
    pub fn growth(&self, x: i32, y: i32) -> Growth {
        self.tile_of(x, y)
            .map_or(Growth::Open, |i| Growth::from_u8(self.growth[i]))
    }

    /// Tiles per side of the growth layer (0 with no cover).
    pub fn growth_side(&self) -> u32 {
        self.cover_tiles as u32
    }

    /// Every tile of thicket, row by row (empty with no cover), for the
    /// walkable grid it stands on.
    pub(crate) fn thicket(&self) -> impl Iterator<Item = [i32; 4]> + '_ {
        let side = self.cover_tiles.max(1);
        (self.growth.iter().enumerate())
            .filter(|(_, g)| **g == Growth::Thicket as u8)
            .map(move |(i, _)| [i as i32 % side, i as i32 / side, 1, 1])
    }

    /// Clear the tile `(x, y)`: nothing grows there and nothing covers it
    /// any more. Returns what grew there, or `None` (and no change) if
    /// nothing did. The one place growth and cover change mid-match (see
    /// `World::change_ground`); rain soaking footing is [`Terrain::soak`].
    pub(crate) fn clear(&mut self, x: i32, y: i32) -> Option<Growth> {
        let i = self.tile_of(x, y)?;
        let was = Growth::from_u8(self.growth[i]);
        if was == Growth::Open {
            return None;
        }
        Arc::make_mut(&mut self.growth)[i] = Growth::Open as u8;
        Arc::make_mut(&mut self.cover)[i] = Cover::Open as u8;
        // Folded in order, so the same changes in the same order give the
        // same digest everywhere.
        self.changes = (self.changes ^ (i as u64) << 2 ^ was as u64)
            .wrapping_mul(0x0000_0100_0000_01B3)
            .rotate_left(5);
        Some(was)
    }

    /// The digest of every change made to the ground so far: it differs
    /// whenever the ground has changed since it was last read.
    pub fn changes(&self) -> u64 {
        self.changes
    }

    /// Whether the map has any cover to hide in.
    pub fn has_cover(&self) -> bool {
        self.cover_tiles != 0
    }

    /// What covers the tile `p` is on (open off the map or with no cover).
    pub fn cover(&self, p: FxVec2) -> Cover {
        self.tile_of(p.x.floor_int(), p.y.floor_int())
            .map_or(Cover::Open, |i| Cover::from_u8(self.cover[i]))
    }

    /// Whether the match has heights at all.
    pub fn is_flat(&self) -> bool {
        self.tiles == 0
    }

    /// Ground height at `p`, in [`HEIGHT_UNITS`] per tile: the height of the
    /// tile `p` is on (0 on flat ground).
    pub fn height(&self, p: FxVec2) -> i32 {
        if self.tiles == 0 {
            return 0;
        }
        let x = p.x.floor_int().clamp(0, self.tiles - 1);
        let y = p.y.floor_int().clamp(0, self.tiles - 1);
        i32::from(self.height[(y * self.tiles + x) as usize])
    }

    /// How the tile `p` is on holds a soldier's feet.
    /// (Firm off the map, as cover is open there.)
    pub fn footing(&self, p: FxVec2) -> Footing {
        let (x, y) = (p.x.floor_int(), p.y.floor_int());
        let made = index_in(self.footing_tiles, x, y)
            .map_or(Footing::Firm, |i| Footing::from_u8(self.footing[i]));
        if made == Footing::Firm && (self.soaked_at(x, y) || self.rubbled_at(x, y)) {
            Footing::Soft
        } else {
            made
        }
    }

    /// Whether rain has soaked the tile `(x, y)`: it is in line no later
    /// than the last tile soaked.
    fn soaked_at(&self, x: i32, y: i32) -> bool {
        let Some(last) = (self.soaked as usize).checked_sub(1).map(|k| self.soak[k]) else {
            return false;
        };
        let Some(i) = index_in(self.tiles, x, y).filter(|_| !self.soak_need.is_empty()) else {
            return false;
        };
        let need = self.soak_need[i];
        need != 0 && (need, i as u32) <= (last.1, last.0)
    }

    /// Whether `p` is in soft ground (see [`Footing::Soft`]).
    pub fn soft(&self, p: FxVec2) -> bool {
        self.footing(p) == Footing::Soft
    }

    /// Whether a charge can start from `p`, or be made onto a regiment at
    /// `p`: not in water and not in soft ground.
    pub fn bears_charge(&self, p: FxVec2) -> bool {
        !self.wet(p) && !self.soft(p)
    }

    /// Pace kept climbing from `p` a tile along `heading`, percent (100 on
    /// the level or going down).
    pub fn climb_percent(&self, p: FxVec2, heading: FxVec2) -> i32 {
        // A step along the heading long enough to reach the next tile along
        // its main axis, so a diagonal climb is seen as a straight one is.
        let most = heading.x.abs().max(heading.y.abs());
        if self.tiles == 0 || most == Fx::ZERO {
            return 100;
        }
        let ahead = FxVec2::new(heading.x / most, heading.y / most);
        let rise = self.height(p + ahead) - self.height(p);
        let slow = rise.max(0) * CLIMB_SLOW_PERCENT / HEIGHT_UNITS as i32;
        100 - slow.min(CLIMB_SLOW_MAX)
    }

    /// Whether `p` is on a slope that slows a climb: one that rises a
    /// quarter of a tile or more to a neighbouring tile.
    pub fn on_slope(&self, p: FxVec2) -> bool {
        let here = self.height(p);
        self.tiles != 0
            && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(x, y)| {
                self.height(p + FxVec2::from_ints(x, y)) - here >= HEIGHT_UNITS as i32 / 4
            })
    }

    /// The charge bonus `bonus` of a regiment at `from` hitting home on one
    /// at `to`: more from high ground, less up onto it.
    pub fn charge_bonus(&self, bonus: u8, from: FxVec2, to: FxVec2) -> u8 {
        let percent = if self.downhill(from, to) {
            CHARGE_DOWNHILL_PERCENT
        } else if self.downhill(to, from) {
            CHARGE_UPHILL_PERCENT
        } else {
            return bonus;
        };
        (u32::from(bonus) * percent / 100).min(255) as u8
    }

    /// Whether `p` is in water: under the water line, raised by any flood.
    pub fn wet(&self, p: FxVec2) -> bool {
        self.tiles != 0
            && !self.frozen
            && self.height(p) < i32::from(self.water_level) + i32::from(self.rise)
    }

    /// Whether the water is frozen over: walked as dry ground.
    pub fn frozen(&self) -> bool {
        self.frozen
    }

    /// Freeze the water over, or thaw it.
    pub(crate) fn set_frozen(&mut self, frozen: bool) {
        self.frozen = frozen;
    }

    /// The map's water line, in height units (0 with no water), before
    /// any flood.
    pub fn water_level(&self) -> u16 {
        self.water_level
    }

    /// Height units a flood has raised the water (0 with none).
    pub fn rise(&self) -> u16 {
        self.rise
    }

    /// Raise the water to `rise` height units above its line.
    pub(crate) fn set_rise(&mut self, rise: u16) {
        self.rise = rise;
    }

    /// Whether the tile `(x, y)` lies under the map's water line (before
    /// any flood); never off the map.
    pub(crate) fn tile_under_water(&self, x: i32, y: i32) -> bool {
        index_in(self.tiles, x, y).is_some_and(|i| self.height[i] < self.water_level)
    }

    /// Whether something at `from` fights downhill against something at `to`.
    pub fn downhill(&self, from: FxVec2, to: FxVec2) -> bool {
        self.tiles != 0 && self.height(from) - self.height(to) >= HIGH_GROUND
    }

    /// How far a regiment with `range` at `from` shoots at `to`.
    pub fn range(&self, range: Fx, from: FxVec2, to: FxVec2) -> Fx {
        if self.downhill(from, to) {
            Self::range_from_high_ground(range)
        } else {
            range
        }
    }

    /// Whether `to` is within `range` of a shooter at `from`, further
    /// downhill (see [`Terrain::range`]).
    pub fn in_range(&self, range: Fx, from: FxVec2, to: FxVec2) -> bool {
        let r = i64::from(self.range(range, from, to).raw());
        (to - from).length_sq_raw() <= (r * r) as u64
    }

    /// How far round a shooter with `range` a target could be on this
    /// ground: as far as high ground could carry, on a map with any.
    pub fn reach(&self, range: Fx) -> Fx {
        if self.is_flat() {
            range
        } else {
            Self::range_from_high_ground(range)
        }
    }

    /// The longest `range` can reach, from high ground.
    pub fn range_from_high_ground(range: Fx) -> Fx {
        Fx::from_raw((i64::from(range.raw()) * i64::from(DOWNHILL_RANGE_PERCENT) / 100) as i32)
    }

    /// Melee damage percent for a blow from `from` landing on `to`.
    pub fn melee_percent(&self, from: FxVec2, to: FxVec2) -> u64 {
        let mut percent = 100;
        if self.downhill(from, to) {
            percent = percent * DOWNHILL_MELEE_PERCENT / 100;
        }
        if self.wet(to) {
            percent = percent * WADING_DAMAGE_PERCENT / 100;
        }
        percent
    }

    /// `step` slowed by the ground at `p` for troops of `mass` going along
    /// `heading` (a unit vector, or zero): water, woods, soft ground and
    /// the climb.
    pub fn going(&self, step: Fx, p: FxVec2, heading: FxVec2, mass: Mass) -> Fx {
        let heavy = mass == Mass::Heavy;
        let wading = if self.wet(p) {
            WADING_SPEED_PERCENT
        } else {
            100
        };
        let woods = match self.cover(p) {
            Cover::Wood if heavy => WOODS_HEAVY_SPEED_PERCENT,
            Cover::Wood => WOODS_SPEED_PERCENT,
            _ => 100,
        };
        let soft = match self.soft(p) {
            true if heavy => SOFT_HEAVY_SPEED_PERCENT,
            true => SOFT_SPEED_PERCENT,
            false => 100,
        };
        let climb = self.climb_percent(p, heading);
        let percents = [wading, woods, soft, climb];
        if percents == [100; 4] {
            return step;
        }
        let raw =
            (percents.iter()).fold(i64::from(step.raw()), |raw, &pc| raw * i64::from(pc) / 100);
        Fx::from_raw((raw as i32).max(1))
    }

    /// Percent of a volley's arrows that get through the trees, loosed from
    /// `from` at `aim`.
    pub fn through_trees(&self, from: FxVec2, aim: FxVec2) -> u64 {
        let through = |p| {
            if self.cover(p) == Cover::Wood {
                100 - TREES_CATCH_PERCENT
            } else {
                100
            }
        };
        through(from) * through(aim) / 100
    }
}
