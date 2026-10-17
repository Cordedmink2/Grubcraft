//! Building a map from its description: stamping the shapes into the height
//! samples and the cover, and adding the rotated copies of shapes and
//! objects.
//!
//! The stamps are drawn at sample resolution (a quarter tile), so a
//! described map's slopes are as smooth on screen as a generated one's;
//! the tile heights the simulation reads are the samples at tile centres.
//! Everything is integer arithmetic, so every machine builds the same bytes
//! from the same file.

use crate::def::{CoverKind, MapDef, Mode, Noise, Pos, Shape, Symmetry};
use crate::map::{Map, Placed, SAMPLES_PER_TILE, sample_of_sim};
use gc_fixed::isqrt_u64;
use gc_sim::terrain::HEIGHT_UNITS;

/// Simulation height units in one sixteenth of a tile.
const SIXTEENTH: i32 = HEIGHT_UNITS as i32 / 16;
/// Positions in the stamping maths are in samples: this many per tile.
const UNIT: i64 = SAMPLES_PER_TILE as i64;

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

/// The squared distance between two tiles.
pub(crate) fn dist2(a: Pos, b: Pos) -> i32 {
    let (dx, dy) = (a.0 - b.0, a.1 - b.1);
    dx * dx + dy * dy
}

/// A tile and its turned copies under the symmetry, in the order the
/// loader places them (a tile that turns onto itself appears again).
pub(crate) fn tile_copies(p: Pos, sym: Symmetry, tiles: i32) -> Vec<Pos> {
    let turns = step_turns(sym);
    (0..sym.copies() as u32)
        .map(|k| turn_tile(p, tiles, k * turns))
        .collect()
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

/// The distance in samples from sample (px, py) to the shape's centre
/// line, and the shape's half-width in samples.
fn distance(s: Shape, px: i64, py: i64) -> (i64, i64) {
    let scaled = |p: Pos| (UNIT * i64::from(p.0), UNIT * i64::from(p.1));
    match s {
        Shape::Disc { at, r } => {
            let (cx, cy) = scaled(at);
            let (dx, dy) = (px - cx, py - cy);
            (
                isqrt_u64((dx * dx + dy * dy) as u64) as i64,
                UNIT * i64::from(r),
            )
        }
        Shape::Line { a, b, half_width } => {
            let ((ax, ay), (bx, by)) = (scaled(a), scaled(b));
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
            (isqrt_u64(d2 as u64) as i64, UNIT * i64::from(half_width))
        }
    }
}

/// How strongly a stamp with the given edge (in tiles) takes hold at a
/// sample: 256 inside the shape, fading to nothing over the edge.
fn weight(s: Shape, edge: i32, px: i64, py: i64) -> i64 {
    let (d, core) = distance(s, px, py);
    let fade = UNIT * i64::from(edge.max(0));
    if d <= core {
        256
    } else if d >= core + fade {
        0
    } else {
        256 * (core + fade - d) / fade
    }
}

/// The samples a stamp can touch, as inclusive ranges clamped to the map:
/// (x0, x1, y0, y1).
fn reach(s: Shape, edge: i32, last: i64) -> (i64, i64, i64, i64) {
    let ((x0, y0), (x1, y1), r) = match s {
        Shape::Disc { at, r } => (at, at, r),
        Shape::Line { a, b, half_width } => (
            (a.0.min(b.0), a.1.min(b.1)),
            (a.0.max(b.0), a.1.max(b.1)),
            half_width,
        ),
    };
    let pad = i64::from(r.max(0) + edge.max(0)) + 1;
    let lo = |v: i32| ((i64::from(v) - pad) * UNIT).clamp(0, last);
    let hi = |v: i32| ((i64::from(v) + pad) * UNIT).clamp(0, last);
    (lo(x0), hi(x1), lo(y0), hi(y1))
}

impl Map {
    /// The map a description makes.
    pub fn from_def(def: &MapDef) -> Map {
        let tiles = def.size.tiles();
        let side = i64::from(tiles) * UNIT + 1;
        let last = side - 1;
        let at = |x: i64, y: i64| (y * side + x) as usize;
        // Heights in simulation units, one per sample: the base, the
        // rolling ground, then the stamps.
        let mut field = vec![def.base * SIXTEENTH; (side * side) as usize];
        if let Some(noise) = def.noise {
            roll(&mut field, noise, def.symmetry, side);
        }
        // Each stamp's strength per sample, the strongest of its copies;
        // cleared again as the stamp is applied.
        let mut strength = vec![0u16; field.len()];
        for op in &def.terrain {
            let copies = images(op.shape, def.symmetry, tiles);
            let boxes: Vec<_> = copies.iter().map(|&s| reach(s, op.edge, last)).collect();
            for (&s, &(x0, x1, y0, y1)) in copies.iter().zip(&boxes) {
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let w = weight(s, op.edge, x, y) as u16;
                        let cell = &mut strength[at(x, y)];
                        *cell = (*cell).max(w);
                    }
                }
            }
            let target = op.height * SIXTEENTH;
            for &(x0, x1, y0, y1) in &boxes {
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let i = at(x, y);
                        let w = std::mem::take(&mut strength[i]);
                        if w == 0 {
                            continue;
                        }
                        let h = &mut field[i];
                        let blended = *h + (i64::from(target - *h) * i64::from(w) / 256) as i32;
                        *h = match op.mode {
                            Mode::Set => blended,
                            Mode::Raise => (*h).max(blended),
                            Mode::Lower => (*h).min(blended),
                        };
                    }
                }
            }
        }
        let samples = field.iter().map(|&h| sample_of_sim(i64::from(h))).collect();
        let water = sample_of_sim(i64::from(def.water * SIXTEENTH));
        let mut map = Map::from_samples(tiles as u32, samples, water);

        // Cover per tile, from the shape at the tile's centre.
        let n = tiles as usize;
        let mut cover = vec![0u8; n * n];
        for op in &def.cover {
            let byte = match op.kind {
                CoverKind::Open => gc_sim::Cover::Open as u8,
                CoverKind::Grass => gc_sim::Cover::Grass as u8,
                CoverKind::Wood => gc_sim::Cover::Wood as u8,
            };
            for s in images(op.shape, def.symmetry, tiles) {
                let (x0, x1, y0, y1) = reach(s, 0, last);
                let tile_range = |a: i64, b: i64| (a / UNIT).max(0)..(b / UNIT + 1).min(n as i64);
                for y in tile_range(y0, y1) {
                    for x in tile_range(x0, x1) {
                        let (px, py) = (x * UNIT + UNIT / 2, y * UNIT + UNIT / 2);
                        if weight(s, 0, px, py) > 0 {
                            cover[y as usize * n + x as usize] = byte;
                        }
                    }
                }
            }
        }
        map.cover = cover;
        map.name.clone_from(&def.name);
        map.size = Some(def.size);
        map.players = def.players;
        map.objects = place_objects(def, tiles);
        map.set_pieces.clone_from(&def.set_pieces);
        map
    }
}

/// Octaves of rolling ground: each finer and half as strong.
const NOISE_OCTAVES: usize = 4;

/// Add the rolling ground to the heights: noise averaged over the map's
/// rotations, so it is as symmetric as the stamps.
fn roll(field: &mut [i32], noise: Noise, symmetry: Symmetry, side: i64) {
    let fbm = crate::generate::Noise { seed: noise.seed };
    let one = crate::generate::ONE;
    let last = side - 1;
    let per_swell = UNIT * i64::from(noise.scale.max(1));
    let copies = symmetry.copies() as i64;
    let turns = step_turns(symmetry);
    // The noise at every tile, worked out once; a tile then averages it
    // over its turned copies, so they all rise alike.
    let mut swells = Vec::with_capacity((side * side) as usize);
    for y in 0..side {
        for x in 0..side {
            swells.push(fbm.fbm(x * one / per_swell, y * one / per_swell, NOISE_OCTAVES));
        }
    }
    for y in 0..side {
        for x in 0..side {
            let mut sum = 0;
            let mut p = (x, y);
            for _ in 0..copies {
                sum += swells[(p.1 * side + p.0) as usize];
                for _ in 0..turns {
                    p = (last - p.1, p.0);
                }
            }
            let v = sum / copies - one / 2;
            let rise = i64::from(noise.amplitude * SIXTEENTH) * v / one;
            field[(y * side + x) as usize] += rise as i32;
        }
    }
}

fn place_objects(def: &MapDef, tiles: i32) -> Vec<Placed> {
    let mut out: Vec<Placed> = Vec::new();
    for o in &def.objects {
        let copies = if o.once {
            vec![o.at]
        } else {
            tile_copies(o.at, def.symmetry, tiles)
        };
        let mut spots: Vec<Pos> = Vec::new();
        for (k, at) in (0u32..).zip(copies) {
            // A tile that turns onto itself (or a copy of it) is kept once.
            if spots.contains(&at) {
                continue;
            }
            spots.push(at);
            // Each turned copy belongs to the players a turn further round:
            // with six players under Rot2, a start for player 0, 1 or 2 is
            // also one for 3, 4 or 5. (A file with no players or a bad
            // owner builds without panicking; the map checker reports it.)
            let players = u32::from(def.players.max(1));
            let step = (players / def.symmetry.copies() as u32).max(1);
            let owner = o.owner.map(|p| ((u32::from(p) + k * step) % players) as u8);
            out.push(Placed {
                kind: o.kind,
                at,
                owner,
                label: o.label.clone(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::Kind;

    fn tiny(symmetry: Symmetry, players: u8) -> MapDef {
        MapDef::parse(&format!(
            "MapDef(name: \"Tiny\", size: Duel, symmetry: {symmetry:?}, players: {players}, base: 40, water: 24,
              terrain: [(shape: Disc(at: (32, 32), r: 6), mode: Raise, height: 120, edge: 3)],
              cover: [(shape: Disc(at: (20, 100), r: 4), kind: Wood)],
              objects: [(kind: Start, at: (20, 20), owner: Some(0)), (kind: Heart, at: (64, 64), once: true)])"
        ))
        .unwrap()
    }

    /// Six players under Rot2: the three starts written on one half give
    /// the other three their turned copies (0, 1, 2 to 3, 4, 5).
    #[test]
    fn turned_copies_go_to_the_players_a_turn_round() {
        let def = MapDef::parse(
            "MapDef(name: \"Six\", size: Grand, symmetry: Rot2, players: 6, base: 40, water: 24,
              terrain: [], cover: [],
              objects: [(kind: Start, at: (40, 40), owner: Some(0)), (kind: Start, at: (160, 40), owner: Some(1)),
                        (kind: Start, at: (280, 40), owner: Some(2))])",
        )
        .unwrap();
        let map = Map::from_def(&def);
        assert_eq!(
            map.starts(),
            [
                (40, 40),
                (160, 40),
                (280, 40),
                (279, 279),
                (159, 279),
                (39, 279)
            ]
        );
    }

    #[test]
    fn rotated_copies_are_exactly_symmetric() {
        let map = Map::from_def(&tiny(Symmetry::Rot2, 2));
        let n = map.tiles as usize;
        for y in 0..n {
            for x in 0..n {
                let (tx, ty) = (n - 1 - x, n - 1 - y);
                assert_eq!(map.heights[y * n + x], map.heights[ty * n + tx]);
                assert_eq!(map.cover[y * n + x], map.cover[ty * n + tx]);
            }
        }
        let side = map.side() as usize;
        for i in 0..side * side {
            assert_eq!(map.samples[i], map.samples[side * side - 1 - i]);
        }
        assert_eq!(map.starts(), vec![(20, 20), (107, 107)]);
        assert_eq!(map.objects_of(Kind::Heart).count(), 1);
    }

    #[test]
    fn stamps_reach_the_tile_heights_the_sim_reads() {
        let map = Map::from_def(&tiny(Symmetry::None, 2));
        let n = map.tiles as usize;
        let unit = HEIGHT_UNITS as u16 / 16;
        // Inside the hill, outside it, and the water line, exactly.
        assert_eq!(map.heights[32 * n + 32], 120 * unit);
        assert_eq!(map.heights[100 * n + 100], 40 * unit);
        assert_eq!(map.water, 24 * unit);
        // The hill's edge fades: lower than the top, higher than the plain.
        let rim = map.heights[32 * n + 39];
        assert!(rim > 40 * unit && rim < 120 * unit, "{rim}");
        assert_eq!(map.cover[100 * n + 20], gc_sim::Cover::Wood as u8);
        assert_eq!(map.cover[100 * n + 30], gc_sim::Cover::Open as u8);
    }

    #[test]
    fn a_hill_is_ringed_by_cliffs_and_steep_ground() {
        let map = Map::from_def(&tiny(Symmetry::None, 2));
        let ground = map.tile_ground(&map.nav_costs());
        let n = map.tiles as usize;
        let at = |x: usize, y: usize| ground[y * n + x];
        assert_eq!(at(32, 32), crate::Ground::Open, "the hill top");
        assert_eq!(at(100, 100), crate::Ground::Open, "the plain");
        assert!(
            (36..42).any(|x| at(x, 32) == crate::Ground::Blocked),
            "a cliff on the hill's side"
        );
    }
}
