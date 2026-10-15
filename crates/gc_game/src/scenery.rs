//! Woods and tall grass on the field (docs/20-ambushes.md, "Reading it on
//! screen"): a tree on about half the wood tiles (a trunk and two stacked
//! canopy boxes, placed, sized and shaded by a hash of the tile), and a few
//! low tufts on every tall-grass tile. Built once when the map loads, in
//! square chunks, and drawn nearest the camera first.

use gc_render::BoxInstance;
use glam::Vec2;

/// Tiles per chunk side.
const CHUNK: u32 = 16;
/// Share of wood tiles with a tree on them, percent.
const TREE_PERCENT: u32 = 55;
/// Tufts on each tall-grass tile.
const TUFTS: u32 = 3;
/// Trees are drawn this far from the camera's ground point (tiles), and
/// further as it pulls back; grass only nearer.
const TREES_NEAR: f32 = 70.0;
const TREES_PER_ZOOM: f32 = 1.6;
const GRASS_NEAR: f32 = 45.0;
const GRASS_PER_ZOOM: f32 = 0.8;
/// The most scenery boxes drawn in a frame, leaving the rest of the
/// renderer's boxes for soldiers, corpses and arrows.
pub const MAX_SCENERY: usize = 24_000;

pub struct Scenery {
    /// Chunks per side.
    side: u32,
    trees: Vec<Vec<BoxInstance>>,
    grass: Vec<Vec<BoxInstance>>,
    /// Scratch: chunks to draw this frame and how far they are.
    near: Vec<(f32, u32)>,
}

impl Scenery {
    /// The scenery for a map's cover layer (one byte per tile, as
    /// `gc_sim::Ground::cover`); `seed` varies the trees between maps.
    pub fn new(cover: &[u8], map_tiles: u32, seed: u64) -> Scenery {
        let n = map_tiles;
        let side = n.div_ceil(CHUNK);
        let chunks = (side * side) as usize;
        let mut s = Scenery {
            side,
            trees: vec![Vec::new(); chunks],
            grass: vec![Vec::new(); chunks],
            near: Vec::with_capacity(chunks),
        };
        if cover.len() != (n * n) as usize {
            return s;
        }
        for y in 0..n {
            for x in 0..n {
                let chunk = ((y / CHUNK) * side + x / CHUNK) as usize;
                let tile = Vec2::new(x as f32, y as f32);
                match cover[(y * n + x) as usize] {
                    2 => {
                        let h = hash(seed, x, y, 0);
                        if h % 100 < TREE_PERCENT {
                            tree(&mut s.trees[chunk], tile, seed, x, y);
                        }
                    }
                    1 => {
                        for k in 0..TUFTS {
                            tuft(&mut s.grass[chunk], tile, seed, x, y, k);
                        }
                    }
                    _ => {}
                }
            }
        }
        s
    }

    /// Boxes in all the scenery.
    #[cfg(test)]
    pub fn boxes(&self) -> usize {
        self.trees.iter().chain(&self.grass).map(Vec::len).sum()
    }

    /// Append the scenery near the camera's ground point `focus`, the
    /// camera `distance` back, to `out`: trees, and tall grass unless
    /// `grass` is false (the Commander View).
    pub fn push(&mut self, focus: Vec2, distance: f32, grass: bool, out: &mut Vec<BoxInstance>) {
        let trees_reach = TREES_NEAR + distance * TREES_PER_ZOOM;
        let grass_reach = GRASS_NEAR + distance * GRASS_PER_ZOOM;
        self.near.clear();
        for cy in 0..self.side {
            for cx in 0..self.side {
                let k = cy * self.side + cx;
                if self.trees[k as usize].is_empty() && self.grass[k as usize].is_empty() {
                    continue;
                }
                let centre = Vec2::new(
                    (cx * CHUNK) as f32 + CHUNK as f32 * 0.5,
                    (cy * CHUNK) as f32 + CHUNK as f32 * 0.5,
                );
                // The chunk's nearest corner, about.
                let d = (centre.distance(focus) - CHUNK as f32 * 0.71).max(0.0);
                if d <= trees_reach {
                    self.near.push((d, k));
                }
            }
        }
        self.near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut budget = MAX_SCENERY;
        for &(d, k) in &self.near {
            let trees = &self.trees[k as usize];
            let tufts: &[BoxInstance] = if grass && d <= grass_reach {
                &self.grass[k as usize]
            } else {
                &[]
            };
            if trees.len() + tufts.len() > budget {
                break;
            }
            budget -= trees.len() + tufts.len();
            out.extend_from_slice(trees);
            out.extend_from_slice(tufts);
        }
    }
}

/// A tree on the wood tile whose corner is `tile`.
fn tree(out: &mut Vec<BoxInstance>, tile: Vec2, seed: u64, x: u32, y: u32) {
    let h = hash(seed, x, y, 1);
    let at = tile + Vec2::new(0.25 + unit(h) * 0.5, 0.25 + unit(h >> 8) * 0.5);
    let facing = angle(h >> 16);
    let size = 1.3 + unit(h >> 24) * 0.6;
    let shade = (h >> 4) % 24;
    let leaf = [
        44 + shade as u8 / 2,
        80 + shade as u8,
        38 + shade as u8 / 3,
        0,
    ];
    let light = [leaf[0] + 14, leaf[1] + 18, leaf[2] + 8, 0];
    out.push(BoxInstance {
        pos: at.to_array(),
        facing,
        size: [0.22, 1.3, 0.22],
        color: [88, 64, 42, 0],
        lift: 0.0,
    });
    out.push(BoxInstance {
        pos: at.to_array(),
        facing,
        size: [size, 1.1, size],
        color: leaf,
        lift: 1.0,
    });
    out.push(BoxInstance {
        pos: at.to_array(),
        facing,
        size: [size * 0.6, 0.8, size * 0.6],
        color: light,
        lift: 2.1,
    });
}

/// The `k`th tuft on the tall-grass tile whose corner is `tile`.
fn tuft(out: &mut Vec<BoxInstance>, tile: Vec2, seed: u64, x: u32, y: u32, k: u32) {
    let h = hash(seed, x, y, 2 + k);
    let at = tile + Vec2::new(0.1 + unit(h) * 0.8, 0.1 + unit(h >> 8) * 0.8);
    let shade = (h >> 16) % 30;
    out.push(BoxInstance {
        pos: at.to_array(),
        facing: angle(h >> 20),
        size: [0.4, 0.6 + unit(h >> 24) * 0.35, 0.18],
        color: [128 + shade as u8, 150 + shade as u8, 66, 0],
        lift: 0.0,
    });
}

/// A well-mixed hash of a tile (SplitMix64's finaliser).
fn hash(seed: u64, x: u32, y: u32, k: u32) -> u32 {
    let mut z = seed ^ ((x as u64) << 40) ^ ((y as u64) << 16) ^ k as u64;
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) as u32
}

/// 0 to 1 from the low byte of `h`.
fn unit(h: u32) -> f32 {
    (h & 0xff) as f32 / 255.0
}

/// A facing from the low byte of `h`.
fn angle(h: u32) -> [f32; 2] {
    let a = unit(h) * std::f32::consts::TAU;
    [a.cos(), a.sin()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cover() -> Vec<u8> {
        let mut c = vec![0u8; 64 * 64];
        for y in 10..30 {
            for x in 10..30 {
                c[y * 64 + x] = 2;
            }
        }
        for y in 40..44 {
            for x in 40..60 {
                c[y * 64 + x] = 1;
            }
        }
        c
    }

    #[test]
    fn trees_grow_in_woods_and_tufts_in_grass() {
        let s = Scenery::new(&cover(), 64, 7);
        let trees: usize = s.trees.iter().map(Vec::len).sum::<usize>() / 3;
        // About 55% of the 400 wood tiles.
        assert!((160..280).contains(&trees), "{trees} trees");
        let tufts: usize = s.grass.iter().map(Vec::len).sum();
        assert_eq!(tufts, 80 * TUFTS as usize);
        // Every tree stands on its wood tile.
        for b in s.trees.iter().flatten() {
            let [x, y] = b.pos;
            assert!((10.0..30.0).contains(&x) && (10.0..30.0).contains(&y));
        }
        // No cover, no scenery.
        assert_eq!(Scenery::new(&[], 64, 7).boxes(), 0);
    }

    #[test]
    fn draws_what_is_near_the_camera_within_budget() {
        let mut s = Scenery::new(&cover(), 64, 7);
        let mut out = Vec::new();
        s.push(Vec2::new(20.0, 20.0), 10.0, true, &mut out);
        assert_eq!(out.len(), s.boxes());
        out.clear();
        // The Commander View leaves the grass out.
        s.push(Vec2::new(20.0, 20.0), 10.0, false, &mut out);
        assert_eq!(out.len(), s.trees.iter().map(Vec::len).sum::<usize>());
        out.clear();
        // Far away from everything: nothing.
        s.push(Vec2::new(-500.0, -500.0), 0.0, true, &mut out);
        assert!(out.is_empty());
    }
}
