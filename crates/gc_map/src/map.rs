//! The map itself and the terrain rules: what is a cliff, a ford or steep
//! going, and what the simulation reads. One copy of the rules, used by the
//! game, the map checker and (later) the campaign, whichever way the map
//! was made.

use crate::def::{Kind, Land, Pos, SetPiece, SizeClass};

/// Height samples per tile side. The renderer draws the ground from the
/// samples; the walkable-ground grid reads two cells per tile from them.
pub const SAMPLES_PER_TILE: u32 = 4;
/// The highest ground, in tiles: a stored sample of 65,535.
pub const MAX_HEIGHT: i64 = 14;
/// Ground steeper than this many hundredths of a tile of rise per tile is
/// a cliff no regiment climbs; over half of it is slow going that routes
/// avoid.
const CLIFF_RISE_PER_TILE: i64 = 120;
/// Water deeper than this (hundredths of a tile) can't be waded;
/// shallower water is a ford.
const FORD_DEPTH: i64 = 35;
/// Nav costs: open ground, steep ground, fords (see `gc_sim::nav`).
const OPEN: u8 = 1;
const STEEP: u8 = 2;
const FORD: u8 = 3;

/// What a tile is like to walk on: the worst of its walkable-ground cells
/// (see [`Map::tile_ground`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ground {
    Open,
    Steep,
    Ford,
    Blocked,
}

impl Ground {
    /// Cost of stepping onto the tile, ten for open ground (a blocked tile is
    /// never stepped on).
    pub fn cost(self) -> u32 {
        match self {
            Ground::Blocked | Ground::Open => 10,
            Ground::Steep => 14,
            Ground::Ford => 20,
        }
    }
}

/// An object on the map, after the loader added the rotated copies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub kind: Kind,
    /// The tile it stands on.
    pub at: Pos,
    /// The player whose area it belongs to (a start, its own seeps and
    /// vein, its natural expansion).
    pub owner: Option<u8>,
    /// The author's note: which creeps live in a camp, a Shrine's name.
    pub label: String,
}

/// A battlefield, made from a seed ([`Map::generate`]) or a description
/// ([`Map::from_def`]).
#[derive(Clone, Debug)]
pub struct Map {
    /// The map's name, or "Seed N" for a generated one.
    pub name: String,
    /// The description's size class; None for a generated map.
    pub size: Option<SizeClass>,
    /// Tiles per side.
    pub tiles: u32,
    /// How many players it is made for (one start each).
    pub players: u8,
    /// Ground heights, `side() × side()` samples row by row,
    /// [`SAMPLES_PER_TILE`] per tile, 0 to 65,535 for 0 to [`MAX_HEIGHT`]
    /// tiles. What the renderer draws and the rules read.
    pub samples: Vec<u16>,
    /// One height per tile in `gc_sim::terrain::HEIGHT_UNITS`: the sample at
    /// the tile's centre (see [`Map::ground`]).
    pub heights: Vec<u16>,
    /// The water line in `HEIGHT_UNITS`.
    pub water: u16,
    /// The water line as a sample.
    pub(crate) sample_water: u16,
    /// Woods and tall grass, one `gc_sim::Cover` byte per tile, row by row.
    pub cover: Vec<u8>,
    /// Tiles of thicket, woods no one marches through, row by row (empty
    /// if the map has none). Their cover is woods; [`Map::nav_costs`]
    /// blocks them.
    pub thicket: Vec<bool>,
    /// The look of the ground, one [`crate::def::Land`] per tile as a byte,
    /// row by row (empty: meadow everywhere).
    pub land: Vec<u8>,
    /// Starts, seeps, veins, hamlets and the rest. A generated map has its
    /// four starts and nothing else.
    pub objects: Vec<Placed>,
    /// Places the author says make a good Field Battle.
    pub set_pieces: Vec<SetPiece>,
}

/// Simulation height units in [`MAX_HEIGHT`].
const SIM_MAX: u64 = MAX_HEIGHT as u64 * gc_sim::terrain::HEIGHT_UNITS as u64;

/// A sample as a height in simulation units (rounding down).
pub(crate) fn sim_of_sample(s: u16) -> u16 {
    (u64::from(s) * SIM_MAX / u64::from(u16::MAX)) as u16
}

/// The sample for a height in simulation units, rounding up, so
/// [`sim_of_sample`] gives the same height back.
pub(crate) fn sample_of_sim(h: i64) -> u16 {
    let h = h.clamp(0, SIM_MAX as i64) as u64;
    (h * u64::from(u16::MAX)).div_ceil(SIM_MAX) as u16
}

impl Map {
    /// A map with the given ground and nothing on it yet: the tile heights
    /// and water line are derived from the samples.
    pub(crate) fn from_samples(tiles: u32, samples: Vec<u16>, sample_water: u16) -> Map {
        let side = tiles * SAMPLES_PER_TILE + 1;
        assert_eq!(samples.len(), (side * side) as usize, "samples per side");
        let half = SAMPLES_PER_TILE / 2;
        let heights = (0..tiles)
            .flat_map(|y| (0..tiles).map(move |x| (x, y)))
            .map(|(x, y)| {
                let s = (y * SAMPLES_PER_TILE + half) * side + x * SAMPLES_PER_TILE + half;
                sim_of_sample(samples[s as usize])
            })
            .collect();
        Map {
            name: String::new(),
            size: None,
            tiles,
            players: 0,
            samples,
            heights,
            water: sim_of_sample(sample_water),
            sample_water,
            cover: Vec::new(),
            thicket: Vec::new(),
            land: Vec::new(),
            objects: Vec::new(),
            set_pieces: Vec::new(),
        }
    }

    /// Samples per side: `tiles * SAMPLES_PER_TILE + 1`.
    pub fn side(&self) -> u32 {
        self.tiles * SAMPLES_PER_TILE + 1
    }

    /// The ground the simulation plays on: tile heights, the water line,
    /// and the woods and tall grass.
    pub fn ground(&self) -> gc_sim::Ground<'_> {
        gc_sim::Ground {
            heights: &self.heights,
            water_level: self.water,
            cover: &self.cover,
        }
    }

    /// The ground cost of each half-tile cell, row by row, for the
    /// simulation's pathfinding: cliffs, deep water and thicket are
    /// blocked, steep slopes and fords cost more. Integer rules over the samples, so the
    /// same samples give the same grid everywhere.
    pub fn nav_costs(&self) -> Vec<u8> {
        let cells_per_tile = gc_sim::nav::CELLS_PER_TILE as u32;
        let step = SAMPLES_PER_TILE / cells_per_tile; // samples per cell
        let size = self.side();
        let side = self.tiles * cells_per_tile;
        // Samples per hundredth of a tile is u16::MAX / (MAX_HEIGHT * 100).
        let hundredths = |n: i64| (n * i64::from(u16::MAX) / (MAX_HEIGHT * 100)) as u32;
        let cliff = hundredths(CLIFF_RISE_PER_TILE) / cells_per_tile;
        let steep = cliff / 2;
        let water = u32::from(self.sample_water);
        let deep = water.saturating_sub(hundredths(FORD_DEPTH));
        let mut cells = Vec::with_capacity((side * side) as usize);
        for cy in 0..side {
            for cx in 0..side {
                // The (step + 1)^2 samples covering the cell.
                let (mut lo, mut hi, mut sum, mut n) = (u32::MAX, 0, 0, 0);
                for sz in cy * step..=(cy + 1) * step {
                    for sx in cx * step..=(cx + 1) * step {
                        let h = u32::from(self.samples[(sz * size + sx) as usize]);
                        lo = lo.min(h);
                        hi = hi.max(h);
                        sum += h;
                        n += 1;
                    }
                }
                let mean = sum / n;
                let rise = hi - lo;
                let tile = (cy / cells_per_tile) * self.tiles + cx / cells_per_tile;
                let thicket = self.is_thicket(tile as usize);
                cells.push(if mean < deep || rise > cliff || thicket {
                    gc_sim::nav::BLOCKED
                } else if mean < water {
                    FORD
                } else if rise > steep {
                    STEEP
                } else {
                    OPEN
                });
            }
        }
        cells
    }

    /// Each tile's ground, row by row, from the walkable-ground grid
    /// (`nav`, from [`Map::nav_costs`]): the worst of its cells, blocked
    /// over ford over steep over open, and the outermost ring of tiles
    /// blocked so nothing is placed on the map's edge. For tile-sized
    /// questions (can a building or a seep go here, how far is it to walk)
    /// that agree with where regiments can go.
    pub fn tile_ground(&self, nav: &[u8]) -> Vec<Ground> {
        let per = gc_sim::nav::CELLS_PER_TILE as usize;
        let tiles = self.tiles as usize;
        let side = tiles * per;
        assert_eq!(nav.len(), side * side, "nav grid for this map");
        let ground = |c: u8| match c {
            gc_sim::nav::BLOCKED => Ground::Blocked,
            FORD => Ground::Ford,
            STEEP => Ground::Steep,
            _ => Ground::Open,
        };
        let mut out = Vec::with_capacity(tiles * tiles);
        for y in 0..tiles {
            for x in 0..tiles {
                let worst = (0..per)
                    .flat_map(|dy| (0..per).map(move |dx| (dx, dy)))
                    .map(|(dx, dy)| ground(nav[(y * per + dy) * side + x * per + dx]))
                    .max();
                let edge = x == 0 || y == 0 || x == tiles - 1 || y == tiles - 1;
                out.push(if edge {
                    Ground::Blocked
                } else {
                    worst.unwrap_or(Ground::Open)
                });
            }
        }
        out
    }

    /// Whether a tile (row by row) is thicket.
    pub fn is_thicket(&self, tile: usize) -> bool {
        self.thicket.get(tile) == Some(&true)
    }

    /// A tile's land (row by row), or `None` on a map without lands.
    pub fn land_at(&self, tile: usize) -> Option<Land> {
        self.land.get(tile).map(|&b| Land::from_u8(b))
    }

    pub fn objects_of(&self, kind: Kind) -> impl Iterator<Item = &Placed> {
        self.objects.iter().filter(move |o| o.kind == kind)
    }

    /// Whether the map has a Conquest economy (seeps, veins, a Heart),
    /// which a Field Battle map doesn't.
    pub fn is_conquest(&self) -> bool {
        self.objects
            .iter()
            .any(|o| matches!(o.kind, Kind::GrubSeep | Kind::MarrowVein | Kind::Heart))
    }

    /// Each player's start tile, by player.
    pub fn starts(&self) -> Vec<Pos> {
        let mut starts: Vec<&Placed> = self.objects_of(Kind::Start).collect();
        starts.sort_by_key(|o| o.owner);
        starts.into_iter().map(|o| o.at).collect()
    }

    /// Tiles a regiment's centre can use, from [`Map::tile_ground`]: not
    /// blocked and not touching a blocked tile (the sim keeps regiments
    /// about a tile off cliffs).
    pub fn standable(&self, ground: &[Ground]) -> Vec<bool> {
        let n = self.tiles as i32;
        let blocked = |x: i32, y: i32| {
            !((0..n).contains(&x) && (0..n).contains(&y))
                || ground[(y * n + x) as usize] == Ground::Blocked
        };
        let mut out = Vec::with_capacity(ground.len());
        for y in 0..n {
            for x in 0..n {
                out.push((-1..=1).all(|dy| (-1..=1).all(|dx| !blocked(x + dx, y + dy))));
            }
        }
        out
    }

    /// Whether an army can walk from every start to every other, keeping
    /// a tile clear of blocked ground as regiments do.
    pub fn starts_joined(&self) -> bool {
        let n = self.tiles as i32;
        let standable = self.standable(&self.tile_ground(&self.nav_costs()));
        let open = |x: i32, y: i32| {
            (0..n).contains(&x) && (0..n).contains(&y) && standable[(y * n + x) as usize]
        };
        let starts = self.starts();
        let Some(&first) = starts.first() else {
            return true;
        };
        let mut seen = vec![false; standable.len()];
        let mut todo = vec![first];
        seen[(first.1 * n + first.0) as usize] = true;
        while let Some((x, y)) = todo.pop() {
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (px, py) = (x + dx, y + dy);
                if open(px, py) && !seen[(py * n + px) as usize] {
                    seen[(py * n + px) as usize] = true;
                    todo.push((px, py));
                }
            }
        }
        starts.iter().all(|&(x, y)| seen[(y * n + x) as usize])
    }

    /// The fingerprint machines compare before a network match
    /// ([`map_hash`] over this map's grid and ground).
    pub fn hash(&self) -> u64 {
        map_hash(&self.nav_costs(), &self.ground())
    }
}

/// A fingerprint of a map's walkable ground, hills, water and cover
/// (FNV-1a), so a joiner whose map came out differently finds out before
/// the battle starts.
pub fn map_hash(nav: &[u8], ground: &gc_sim::Ground) -> u64 {
    let heights = ground.heights.iter().chain([&ground.water_level]);
    let bytes = (nav.iter().copied())
        .chain(heights.flat_map(|v| v.to_le_bytes()))
        .chain(ground.cover.iter().copied());
    fnv1a(bytes) ^ nav.len() as u64
}

/// FNV-1a over a stream of bytes.
pub fn fnv1a(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_survive_the_trip_through_samples() {
        for h in [0, 1, 255, 256, 640, 1919, 3583, 3584] {
            assert_eq!(i64::from(sim_of_sample(sample_of_sim(h))), h, "{h}");
        }
        assert_eq!(sample_of_sim(-5), 0);
        assert_eq!(sample_of_sim(99_999), u16::MAX);
    }
}
