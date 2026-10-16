//! Building a map: stamping the shapes into heights and cover, adding the
//! rotated copies, and working out which tiles a regiment can walk.
//!
//! Everything here is integer arithmetic, so every machine builds the same
//! bytes from the same file.
//!
//! The cliff, ford and clearance rules below repeat those of the game's
//! walkable-ground grid (`Heightmap::nav_costs` in `gc_game`), which works on
//! half-tile cells; sharing one implementation is to be done when the game first loads these maps.

use crate::def::*;
use gc_sim::terrain::HEIGHT_UNITS;

/// Sim height units in one sixteenth of a tile.
const SIXTEENTH: i32 = HEIGHT_UNITS as i32 / 16;
/// Water this much below the water line or deeper is impassable (0.35 tiles,
/// as in the game's nav rules).
const DEEP: i32 = HEIGHT_UNITS as i32 * 35 / 100;
/// A rise this steep between neighbouring tiles is a cliff (1.2 tiles per
/// tile); half of it is steep going that routes avoid.
const CLIFF: i32 = HEIGHT_UNITS as i32 * 12 / 10;
const STEEP: i32 = CLIFF / 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    Blocked,
    Open,
    Steep,
    Ford,
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

/// An object after the rotated copies were added.
#[derive(Clone, Debug)]
pub struct Placed {
    pub kind: Kind,
    pub at: Pos,
    pub owner: Option<u8>,
}

pub struct Map {
    pub size: SizeClass,
    pub players: u8,
    pub tiles: i32,
    /// One height per tile in `gc_sim` units, row by row.
    pub heights: Vec<u16>,
    pub water: u16,
    /// One `gc_sim::terrain::Cover` byte per tile.
    pub cover: Vec<u8>,
    pub ground: Vec<Ground>,
    pub objects: Vec<Placed>,
    pub set_pieces: Vec<SetPiece>,
}

/// Quarter turns between one copy and the next.
fn step_turns(sym: Symmetry) -> u32 {
    match sym {
        Symmetry::None => 0,
        Symmetry::Rot2 => 2,
        Symmetry::Rot4 => 1,
    }
}

/// A point turned `n` quarter circles about the centre of a `tiles`-wide map.
fn turn_point(mut p: Pos, tiles: i32, n: u32) -> Pos {
    for _ in 0..n {
        p = (tiles - p.1, p.0);
    }
    p
}

/// A tile turned `n` quarter circles (a tile's index is one less than its far corner).
fn turn_tile(mut p: Pos, tiles: i32, n: u32) -> Pos {
    for _ in 0..n {
        p = (tiles - 1 - p.1, p.0);
    }
    p
}

fn turn_shape(s: Shape, tiles: i32, n: u32) -> Shape {
    match s {
        Shape::Disc { at, r } => Shape::Disc {
            at: turn_point(at, tiles, n),
            r,
        },
        Shape::Line { a, b, half_width } => Shape::Line {
            a: turn_point(a, tiles, n),
            b: turn_point(b, tiles, n),
            half_width,
        },
    }
}

/// The shape and its copies under the symmetry.
fn images(s: Shape, sym: Symmetry, tiles: i32) -> Vec<Shape> {
    (0..sym.copies() as u32)
        .map(|k| turn_shape(s, tiles, k * step_turns(sym)))
        .collect()
}

/// Distance in half tiles from the centre of tile (x, y) to the shape's core,
/// as an integer.
fn distance2(s: Shape, x: i32, y: i32) -> (i64, i64) {
    let (px, py) = (2 * i64::from(x) + 1, 2 * i64::from(y) + 1);
    match s {
        Shape::Disc { at, r } => {
            let (dx, dy) = (px - 2 * i64::from(at.0), py - 2 * i64::from(at.1));
            (
                ((dx * dx + dy * dy) as u64).isqrt() as i64,
                2 * i64::from(r),
            )
        }
        Shape::Line { a, b, half_width } => {
            let (ax, ay) = (2 * i64::from(a.0), 2 * i64::from(a.1));
            let (bx, by) = (2 * i64::from(b.0), 2 * i64::from(b.1));
            let (ex, ey) = (bx - ax, by - ay);
            let (dx, dy) = (px - ax, py - ay);
            let len2 = ex * ex + ey * ey;
            let dot = dx * ex + dy * ey;
            let d2 = if len2 == 0 || dot <= 0 {
                dx * dx + dy * dy
            } else if dot >= len2 {
                let (fx, fy) = (px - bx, py - by);
                fx * fx + fy * fy
            } else {
                let cross = dx * ey - dy * ex;
                cross * cross / len2
            };
            ((d2 as u64).isqrt() as i64, 2 * i64::from(half_width))
        }
    }
}

/// How strongly a stamp with the given edge (in tiles) takes hold of a tile:
/// 256 inside the shape, fading to nothing over the edge.
fn weight(s: Shape, edge: i32, x: i32, y: i32) -> i64 {
    let (d, core) = distance2(s, x, y);
    let fade = 2 * i64::from(edge.max(0));
    if d <= core {
        256
    } else if d >= core + fade {
        0
    } else {
        256 * (core + fade - d) / fade
    }
}

impl Map {
    pub fn build(def: &MapDef) -> Map {
        let tiles = def.size.tiles();
        let n = (tiles * tiles) as usize;
        let mut field = vec![def.base * SIXTEENTH; n];
        for op in &def.terrain {
            let copies = images(op.shape, def.symmetry, tiles);
            let target = op.height * SIXTEENTH;
            for y in 0..tiles {
                for x in 0..tiles {
                    let w = copies
                        .iter()
                        .map(|&s| weight(s, op.edge, x, y))
                        .max()
                        .unwrap_or(0);
                    if w == 0 {
                        continue;
                    }
                    let h = &mut field[(y * tiles + x) as usize];
                    let blended = *h + (i64::from(target - *h) * w / 256) as i32;
                    *h = match op.mode {
                        Mode::Set => blended,
                        Mode::Raise => (*h).max(blended),
                        Mode::Lower => (*h).min(blended),
                    };
                }
            }
        }
        let heights: Vec<u16> = field
            .iter()
            .map(|&h| h.clamp(0, i32::from(u16::MAX)) as u16)
            .collect();
        let water = (def.water * SIXTEENTH).clamp(0, i32::from(u16::MAX)) as u16;

        let mut cover = vec![0u8; n];
        for op in &def.cover {
            let byte = match op.kind {
                CoverKind::Open => 0,
                CoverKind::Grass => gc_sim::terrain::Cover::Grass as u8,
                CoverKind::Wood => gc_sim::terrain::Cover::Wood as u8,
            };
            let copies = images(op.shape, def.symmetry, tiles);
            for y in 0..tiles {
                for x in 0..tiles {
                    if copies.iter().any(|&s| weight(s, 0, x, y) > 0) {
                        cover[(y * tiles + x) as usize] = byte;
                    }
                }
            }
        }

        let ground = classify(&heights, water, tiles);
        Map {
            size: def.size,
            players: def.players,
            tiles,
            heights,
            water,
            cover,
            ground,
            objects: place_objects(def, tiles),
            set_pieces: def.set_pieces.clone(),
        }
    }

    pub fn idx(&self, p: Pos) -> usize {
        (p.1 * self.tiles + p.0) as usize
    }

    pub fn in_bounds(&self, p: Pos) -> bool {
        (0..self.tiles).contains(&p.0) && (0..self.tiles).contains(&p.1)
    }

    pub fn ground_at(&self, p: Pos) -> Ground {
        if self.in_bounds(p) {
            self.ground[self.idx(p)]
        } else {
            Ground::Blocked
        }
    }

    /// Tiles a regiment's centre can use: walkable, and not touching a
    /// blocked tile (the sim keeps blocks about a tile off cliffs).
    pub fn walkable(&self) -> Vec<bool> {
        let mut out = vec![false; self.ground.len()];
        for y in 0..self.tiles {
            for x in 0..self.tiles {
                let clear = self.ground_at((x, y)) != Ground::Blocked
                    && (-1..=1).all(|dy| {
                        (-1..=1).all(|dx| self.ground_at((x + dx, y + dy)) != Ground::Blocked)
                    });
                out[self.idx((x, y))] = clear;
            }
        }
        out
    }

    pub fn objects_of(&self, kind: Kind) -> impl Iterator<Item = &Placed> {
        self.objects.iter().filter(move |o| o.kind == kind)
    }

    pub fn start_of(&self, player: u8) -> Option<Pos> {
        self.objects_of(Kind::Start)
            .find(|o| o.owner == Some(player))
            .map(|o| o.at)
    }

    /// A hash of everything the sim reads from the map.
    pub fn hash(&self) -> String {
        let mut h = blake3::Hasher::new();
        h.update(&self.tiles.to_le_bytes());
        h.update(&self.water.to_le_bytes());
        for v in &self.heights {
            h.update(&v.to_le_bytes());
        }
        h.update(&self.cover);
        h.finalize().to_hex()[..16].to_string()
    }

    /// The ground as `gc_sim`'s match setup takes it.
    pub fn sim_ground(&self) -> gc_sim::Ground<'_> {
        gc_sim::Ground {
            heights: &self.heights,
            water_level: self.water,
            cover: &self.cover,
        }
    }
}

fn classify(heights: &[u16], water: u16, tiles: i32) -> Vec<Ground> {
    let h = |x: i32, y: i32| -> Option<i32> {
        ((0..tiles).contains(&x) && (0..tiles).contains(&y))
            .then(|| i32::from(heights[(y * tiles + x) as usize]))
    };
    let mut out = Vec::with_capacity(heights.len());
    for y in 0..tiles {
        for x in 0..tiles {
            let here = i32::from(heights[(y * tiles + x) as usize]);
            let rise = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .filter_map(|(dx, dy)| h(x + dx, y + dy))
                .map(|o| (o - here).abs())
                .max()
                .unwrap_or(0);
            let edge = x == 0 || y == 0 || x == tiles - 1 || y == tiles - 1;
            let water = i32::from(water);
            out.push(if edge || rise > CLIFF || here < water - DEEP {
                Ground::Blocked
            } else if here < water {
                Ground::Ford
            } else if rise > STEEP {
                Ground::Steep
            } else {
                Ground::Open
            });
        }
    }
    out
}

fn place_objects(def: &MapDef, tiles: i32) -> Vec<Placed> {
    let mut out: Vec<Placed> = Vec::new();
    let turns = step_turns(def.symmetry);
    for o in &def.objects {
        let copies = if o.once {
            1
        } else {
            def.symmetry.copies() as u32
        };
        let mut spots: Vec<Pos> = Vec::new();
        for k in 0..copies {
            let at = turn_tile(o.at, tiles, k * turns);
            // A tile that turns onto itself (or a copy of it) is kept once.
            if spots.contains(&at) {
                continue;
            }
            spots.push(at);
            // Each copy belongs to the next player round.
            let owner = o.owner.map(|p| (p + k as u8) % def.players);
            out.push(Placed {
                kind: o.kind,
                at,
                owner,
            });
        }
    }
    out
}
