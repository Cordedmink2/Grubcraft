//! The factions' lands, drawn over the ground (docs/33-faction-lands.md).
//!
//! The simulation says where each faction's land spreads from: the Brood's
//! linked Hives and Pustules (`World::veins`, with how far their veins
//! reach), the Choir's fields of power, and the finished buildings of the
//! Clans and the Concord. A few times a second this turns that into a grid
//! for the renderer, four bytes a tile: how much of the tile the land
//! covers, whose land it is, and how far it is from the nearest source. The
//! shader draws the ground; this decides only where. The cover grows
//! outwards over a few seconds when a building appears and fades when one is
//! lost, so the land creeps rather than appears. Presentation only: it reads
//! the world and writes nothing to it.

use std::time::{Duration, Instant};

use gc_data::Faction;
use gc_render::{LAND_BROOD, LAND_CHOIR, LAND_CLANS, LAND_CONCORD, Renderer};
use gc_sim::structures::Role;
use gc_sim::{StructureView, World};
use glam::Vec2;

use crate::buildings::{Table, centre_of};

/// The grid is redrawn this often.
const EVERY: Duration = Duration::from_millis(250);
/// How fast the cover spreads and withdraws, in full covers a second.
const GROW: f32 = 0.3;
const FADE: f32 = 0.6;
/// How far the Clans' and the Concord's land reaches from the edge of a
/// main building, an outpost and any other building, in tiles. Walls and
/// towers spread none.
const MAIN_REACH: f32 = 10.0;
const OUTPOST_REACH: f32 = 6.0;
const OTHER_REACH: f32 = 2.5;
/// Half the width of the corridor of land between a building and the one it
/// links through, in tiles.
const CORRIDOR: f32 = 2.6;
/// Distances are stored in quarter tiles, up to this many tiles (the shader
/// scales them back).
const FAR: f32 = 63.75;

/// One building the land spreads from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Source {
    /// Whose land it is (`LAND_BROOD`...).
    pub kind: u8,
    pub centre: Vec2,
    /// Half the footprint's sides.
    pub half: Vec2,
    /// How far from the edge the land reaches, in tiles.
    pub reach: f32,
    /// The centre of the building it links through, when that is shown too.
    pub linked: Option<Vec2>,
}

#[derive(Default)]
pub struct LandView {
    land: Land,
    /// When the grid was last drawn.
    last: Option<Instant>,
    /// What it was drawn from, and whether the cover had stopped moving.
    drawn: Vec<Source>,
    settled: bool,
    /// Whether the renderer holds anything but bare ground.
    shown: bool,
}

/// The land per tile, in the order of the map.
#[derive(Default)]
struct Land {
    tiles: usize,
    /// Where the land is heading and how far from the nearest source (both
    /// worked out afresh each step).
    target: Vec<f32>,
    dist: Vec<f32>,
    /// How much is covered now.
    cover: Vec<f32>,
    /// Whose land each tile last was (0 for none yet, else a `Source::kind`),
    /// kept while it fades.
    owner: Vec<u8>,
    /// What the renderer is given.
    grid: Vec<u8>,
}

/// What a step came to.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Step {
    /// Any land remains.
    any: bool,
    /// Some cover moved, so the grid differs from the last one.
    moved: bool,
}

impl LandView {
    /// Forget the land, for a new match.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Draw the factions' lands for the buildings `shown` allows (see
    /// `shown_to`). Call it every frame; it only redraws a few times a
    /// second, and does nothing where there is no land or it has settled.
    pub fn update(
        &mut self,
        now: Instant,
        renderer: &mut Renderer,
        world: &World,
        shown: impl Fn(&StructureView) -> bool + Copy,
        tiles: u32,
        table: &Table,
    ) {
        let elapsed = self.last.map(|t| now.duration_since(t));
        if elapsed.is_some_and(|e| e < EVERY) {
            return;
        }
        self.last = Some(now);
        let sources = sources(world, shown, table);
        let same = sources == self.drawn;
        if (sources.is_empty() && !self.shown) || (self.settled && same) {
            return;
        }
        // Land that is not yet on screen is shown as it stands, not grown
        // (a game just started or loaded, an enemy base just seen).
        let dt = elapsed
            .filter(|_| self.shown)
            .map_or(f32::INFINITY, |e| e.as_secs_f32());
        let step = self.land.step(tiles as usize, &sources, dt);
        // The distances in the grid change with the sources even when no
        // cover moves.
        if step.moved || !same {
            renderer.set_land(step.any.then_some(self.land.grid.as_slice()));
        }
        self.settled = !step.moved;
        self.shown = step.any;
        self.drawn = sources;
    }
}

/// The buildings `side` is shown that spread land, as sources of it: the
/// Brood's veins, the Choir's fields of power, and what the Clans and the
/// Concord have built and finished.
pub(crate) fn sources(
    world: &World,
    shown: impl Fn(&StructureView) -> bool + Copy,
    table: &Table,
) -> Vec<Source> {
    let Some(structures) = world.structures() else {
        return Vec::new();
    };
    let source = |kind, s: &StructureView, reach: f32, linked| Source {
        kind,
        centre: centre_of(s.tile, s.width, s.depth),
        half: Vec2::new(f32::from(s.width), f32::from(s.depth)) / 2.0,
        reach,
        linked,
    };
    let veins = world.veins().filter_map(|v| {
        let s = structures.get(v.building).filter(|s| shown(s))?;
        let linked = structures
            .get(v.parent)
            .filter(|p| v.parent != v.building && shown(p))
            .map(|_| crate::from_fx_vec(v.from));
        Some(source(LAND_BROOD, &s, v.reach as f32, linked))
    });
    let fields = world.power_fields().filter_map(|f| {
        let s = structures.get(f.building).filter(|s| shown(s))?;
        Some(source(LAND_CHOIR, &s, f.reach as f32, None))
    });
    let settled = structures
        .iter()
        .filter(|s| s.finished() && shown(s))
        .filter_map(|s| {
            let kind = match table.faction(s.building)? {
                Faction::EmberClans => LAND_CLANS,
                Faction::CarrionConcord => LAND_CONCORD,
                _ => return None,
            };
            let reach = settled_reach(table.stats.get(usize::from(s.building))?.role)?;
            Some(source(kind, &s, reach, None))
        });
    veins.chain(fields).chain(settled).collect()
}

/// How far the Clans' and the Concord's land reaches from a building of
/// `role`: none for walls and towers.
fn settled_reach(role: Role) -> Option<f32> {
    match role {
        Role::Main => Some(MAIN_REACH),
        Role::Outpost => Some(OUTPOST_REACH),
        Role::Housing | Role::Economy | Role::Production | Role::Tech | Role::Heroes => {
            Some(OTHER_REACH)
        }
        Role::Defence | Role::Wall | Role::Bridge | Role::Stakes | Role::Barricade | Role::Trap => {
            None
        }
    }
}

impl Land {
    /// Move the cover towards what `sources` ask for on a map `tiles` across,
    /// after `dt` seconds, and fill `grid`.
    fn step(&mut self, tiles: usize, sources: &[Source], dt: f32) -> Step {
        if self.tiles != tiles {
            self.tiles = tiles;
            for v in [&mut self.target, &mut self.dist, &mut self.cover] {
                v.clear();
                v.resize(tiles * tiles, 0.0);
            }
            self.owner.clear();
            self.owner.resize(tiles * tiles, 0);
            self.grid.clear();
            self.grid.resize(tiles * tiles * 4, 0);
        }
        self.target.fill(0.0);
        self.dist.fill(FAR);
        for s in sources {
            self.reach_from(s);
        }
        let mut step = Step {
            any: false,
            moved: false,
        };
        for (i, (cover, &target)) in self.cover.iter_mut().zip(&self.target).enumerate() {
            let before = *cover;
            *cover = if target > before {
                (before + GROW * dt).min(target)
            } else {
                (before - FADE * dt).max(target)
            };
            step.moved |= *cover != before;
            let texel = &mut self.grid[i * 4..i * 4 + 4];
            // Bare tiles keep their distance (far), so the edge of the land
            // does not blend towards a source.
            texel[2] = (self.dist[i] * 4.0).round().min(255.0) as u8;
            if *cover > 0.0 {
                step.any = true;
                texel[0] = (*cover * 255.0).round() as u8;
                texel[1] = self.owner[i];
            } else {
                texel[..2].fill(0);
            }
        }
        step
    }

    /// Ask for land round one source: full cover up to about two thirds of its
    /// reach from the edge, thinning to none at the reach, and a corridor to
    /// the building it links through.
    fn reach_from(&mut self, s: &Source) {
        let pad = Vec2::splat(s.reach + 1.0);
        self.paint(
            s.kind,
            s.centre - s.half - pad,
            s.centre + s.half + pad,
            |p| {
                let outside = ((p - s.centre).abs() - s.half).max(Vec2::ZERO).length();
                (1.0 - smooth(0.7 * s.reach, s.reach, outside), outside)
            },
        );
        if let Some(from) = s.linked {
            let pad = Vec2::splat(CORRIDOR);
            self.paint(
                s.kind,
                s.centre.min(from) - pad,
                s.centre.max(from) + pad,
                |p| {
                    let gap = segment_gap(p, s.centre, from);
                    // Counted from the building's edge, as its own reach is.
                    let dist = (p.distance(s.centre) - s.half.max_element()).max(0.0);
                    (1.0 - smooth(CORRIDOR * 0.45, CORRIDOR, gap), dist)
                },
            );
        }
    }

    /// Ask for the cover and distance `at` gives for the tile centres within
    /// the box `lo` to `hi`, for land of `kind`: where its own overlap it
    /// keeps the most cover and least distance, and where another's does,
    /// whichever has the most cover wins the tile.
    fn paint(&mut self, kind: u8, lo: Vec2, hi: Vec2, at: impl Fn(Vec2) -> (f32, f32)) {
        let tiles = self.tiles;
        let (lo, hi) = (lo.max(Vec2::ZERO), hi.min(Vec2::splat(tiles as f32)));
        for y in lo.y as usize..(hi.y.ceil() as usize).min(tiles) {
            for x in lo.x as usize..(hi.x.ceil() as usize).min(tiles) {
                let (cover, dist) = at(Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
                let i = y * tiles + x;
                if cover > 0.0 && self.owner[i] == kind {
                    self.target[i] = self.target[i].max(cover);
                    self.dist[i] = self.dist[i].min(dist);
                } else if cover > self.target[i] {
                    // Another land has more of the tile.
                    self.owner[i] = kind;
                    self.target[i] = cover;
                    self.dist[i] = dist;
                }
            }
        }
    }
}

/// 0 below `a`, 1 above `b`, smooth between.
pub(crate) fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far `p` is from the segment `a` to `b`.
fn segment_gap(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hive() -> Source {
        Source {
            kind: LAND_BROOD,
            centre: Vec2::new(32.0, 32.0),
            half: Vec2::splat(2.5),
            reach: 15.0,
            linked: None,
        }
    }

    fn texel(land: &Land, x: usize, y: usize) -> [u8; 4] {
        let i = (y * land.tiles + x) * 4;
        land.grid[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn land_spreads_from_a_source_and_stops_at_its_reach() {
        let mut land = Land::default();
        // The first look shows the land as it stands.
        assert!(land.step(64, &[hive()], f32::INFINITY).any);
        let centre = texel(&land, 32, 32);
        assert_eq!((centre[0], centre[1]), (255, LAND_BROOD));
        // 2.5 to the edge, 15 of reach: 20 tiles from the centre is bare.
        assert_eq!(texel(&land, 52, 32)[0], 0);
        // Between, it thins and the distance grows.
        let (near, far) = (texel(&land, 40, 32), texel(&land, 46, 32));
        assert!(near[0] > far[0], "{near:?} {far:?}");
        assert!(near[2] < far[2], "{near:?} {far:?}");
    }

    #[test]
    fn the_cover_creeps_out_over_seconds_and_withdraws_when_the_source_goes() {
        let mut land = Land::default();
        land.step(64, &[hive()], 0.0);
        assert!(land.cover.iter().all(|&c| c == 0.0));
        land.step(64, &[hive()], 1.0);
        let after_a_second = land.cover[32 * 64 + 32];
        assert!((after_a_second - GROW).abs() < 1e-4, "{after_a_second}");
        for _ in 0..8 {
            land.step(64, &[hive()], 1.0);
        }
        assert_eq!(land.cover[32 * 64 + 32], 1.0);
        // The source is lost: it fades, not vanishes, and then goes.
        assert!(land.step(64, &[], 1.0).any);
        assert!(land.cover[32 * 64 + 32] < 1.0);
        for _ in 0..3 {
            land.step(64, &[], 1.0);
        }
        assert!(!land.step(64, &[], 1.0).any);
        // Bare, and far from any source.
        assert_eq!(texel(&land, 32, 32), [0, 0, 255, 0]);
    }

    #[test]
    fn a_linked_building_is_joined_to_its_parent_by_a_corridor() {
        let mut land = Land::default();
        let pustule = Source {
            kind: LAND_BROOD,
            centre: Vec2::new(52.0, 32.0),
            half: Vec2::splat(1.0),
            reach: 3.0,
            linked: Some(Vec2::new(32.0, 32.0)),
        };
        land.step(64, &[hive(), pustule], f32::INFINITY);
        // Midway the two reaches do not meet (15 from the hive's edge and 3
        // from the pustule's leave a gap), but the corridor does.
        assert!(texel(&land, 44, 32)[0] > 200);
        // Off the corridor it is bare.
        assert_eq!(texel(&land, 44, 60)[0], 0);
    }

    #[test]
    fn a_settled_land_reports_no_movement() {
        let mut land = Land::default();
        let first = land.step(64, &[hive()], f32::INFINITY);
        assert_eq!(
            first,
            Step {
                any: true,
                moved: true
            }
        );
        // Nothing asked for has changed, so nothing moves.
        let again = land.step(64, &[hive()], 0.25);
        assert_eq!(
            again,
            Step {
                any: true,
                moved: false
            }
        );
        // A lost source moves it again.
        assert!(land.step(64, &[], 0.25).moved);
    }

    #[test]
    fn where_two_lands_overlap_the_one_with_more_cover_takes_the_tile() {
        let clans = Source {
            kind: LAND_CLANS,
            centre: Vec2::new(52.0, 32.0),
            ..hive()
        };
        for sources in [[hive(), clans], [clans, hive()]] {
            let mut land = Land::default();
            land.step(64, &sources, f32::INFINITY);
            // Each keeps the tiles by its own building, and where one's
            // cover has thinned the other's takes over, in either order.
            assert_eq!(texel(&land, 32, 32)[1], LAND_BROOD);
            assert_eq!(texel(&land, 34, 32)[1], LAND_BROOD);
            assert_eq!(texel(&land, 52, 32)[1], LAND_CLANS);
            assert_eq!(texel(&land, 46, 32)[1], LAND_CLANS);
        }
    }
}
