//! Where a Conquest mind puts its buildings: near where it wants them, a
//! tile apart from its other buildings, and off the paths its workers walk
//! between its halls and their seeps and veins, so a base never walls in
//! its own workers or armies.
//!
//! Every spot is checked with [`World::can_lay`] (tier, price, ground and,
//! for a building that needs power, a field of power there), the check the
//! simulation makes when the order lands; the ground may still change in
//! the reaction time between (someone stands there), and then the Steward
//! simply tries again.

use gc_sim::structures::{BuildingStats, Role};
use gc_sim::{BuildingId, PlayerId, World};
use glam::Vec2;

/// Tiles kept free between two small buildings (houses, towers, walls)
/// set side by side.
pub const GAP: i32 = 1;
/// Tiles kept free round any other building: the narrowest ground a
/// regiment can walk through ([`gc_sim::nav::CLEARANCE`] from blocked
/// ground on both sides), so a production building's regiments can always
/// get out.
pub const WIDE_GAP: i32 = 3;
/// Half-width of the lane kept free from a hall to each of its nodes.
pub const LANE: f32 = 2.5;
/// How far out from `near` a spot is looked for, in tiles.
pub const REACH: i32 = 18;

/// Footprint `[x, y, width, depth]` in tiles.
pub type Foot = [i32; 4];

/// The north-west tile for a building of type `building` (`stats`) that
/// `party` could lay now, as near `near` as can be found within [`REACH`]
/// tiles, keeping [`GAP`] tiles between two small buildings,
/// [`WIDE_GAP`] round any other of its own, and [`LANE`] from each of
/// `lanes` (a hall's middle to a node it serves). `None` when nothing fits.
pub fn spot_for(
    world: &World,
    party: PlayerId,
    building: BuildingId,
    stats: &BuildingStats,
    near: Vec2,
    lanes: &[(Vec2, Vec2)],
) -> Option<(i32, i32)> {
    // Its tier and price don't change from tile to tile.
    world.may_lay(party, building).ok()?;
    let (w, d) = (i32::from(stats.width), i32::from(stats.depth));
    let small = is_small(stats.role);
    // Each footprint with the gap to keep from it.
    let mut taken: Vec<(Foot, i32)> = Vec::new();
    if let Some(s) = world.structures() {
        let table = s.buildings();
        taken.extend(s.iter().filter(|v| v.owner == party).map(|v| {
            let theirs = table
                .get(usize::from(v.building))
                .is_some_and(|b| is_small(b.role));
            let gap = if small && theirs { GAP } else { WIDE_GAP };
            (
                [v.tile.0, v.tile.1, i32::from(v.width), i32::from(v.depth)],
                gap,
            )
        }));
    }
    let cx = (near.x - w as f32 / 2.0).round() as i32;
    let cy = (near.y - d as f32 / 2.0).round() as i32;
    let mid = |t: (i32, i32)| Vec2::new(t.0 as f32 + w as f32 / 2.0, t.1 as f32 + d as f32 / 2.0);
    for r in 0..=REACH {
        // The cheap checks first, then the ground (which looks at every
        // regiment) only until the nearest spot that passes.
        let mut open: Vec<((i32, i32), f32)> = ring(cx, cy, r)
            .into_iter()
            .filter(|&t| {
                let foot = [t.0, t.1, w, d];
                !taken.iter().any(|&(f, gap)| overlaps(f, foot, gap))
                    && !lanes.iter().any(|&(a, b)| crosses(foot, a, b))
            })
            .map(|t| (t, mid(t).distance(near)))
            .collect();
        open.sort_by(|a, b| a.1.total_cmp(&b.1));
        if let Some(&(t, _)) = open
            .iter()
            .find(|(t, _)| world.can_lay(party, building, *t).is_ok())
        {
            return Some(t);
        }
    }
    None
}

/// Tiles on the square ring `r` steps out from `(x, y)`.
fn ring(x: i32, y: i32, r: i32) -> Vec<(i32, i32)> {
    if r == 0 {
        return vec![(x, y)];
    }
    let mut out: Vec<(i32, i32)> = (-r..=r)
        .flat_map(|i| [(x + i, y - r), (x + i, y + r)])
        .collect();
    out.extend((1 - r..r).flat_map(|i| [(x - r, y + i), (x + r, y + i)]));
    out
}

/// Houses, towers and walls: buildings nothing comes out of, which may
/// stand a tile apart.
fn is_small(role: Role) -> bool {
    matches!(role, Role::Housing | Role::Defence | Role::Wall)
}

/// Whether footprints `a` and `b` come within `gap` tiles of each other.
pub fn overlaps(a: Foot, b: Foot, gap: i32) -> bool {
    a[0] < b[0] + b[2] + gap
        && b[0] < a[0] + a[2] + gap
        && a[1] < b[1] + b[3] + gap
        && b[1] < a[1] + a[3] + gap
}

/// Whether footprint `f` comes within [`LANE`] of the segment `a`-`b`.
fn crosses(f: Foot, a: Vec2, b: Vec2) -> bool {
    let min = Vec2::new(f[0] as f32, f[1] as f32);
    let max = min + Vec2::new(f[2] as f32, f[3] as f32);
    // Nowhere near the lane's box: no need to walk it.
    if a.min(b).cmpgt(max + LANE).any() || a.max(b).cmplt(min - LANE).any() {
        return false;
    }
    // Sample along the lane, close enough to be within a tenth of a tile;
    // lanes are short (a hall to its seeps).
    let steps = (a.distance(b) / 0.2).ceil().max(1.0) as i32;
    (0..=steps).any(|i| {
        let p = a.lerp(b, i as f32 / steps as f32);
        let q = p.clamp(min, max);
        q.distance(p) < LANE
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_cover_each_tile_once() {
        let mut seen = std::collections::HashSet::new();
        for r in 0..4 {
            for t in ring(0, 0, r) {
                assert!(seen.insert(t), "{t:?} twice");
            }
        }
        assert_eq!(seen.len(), 49);
    }

    #[test]
    fn a_lane_keeps_buildings_off_it() {
        let lane = (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
        assert!(crosses([4, -1, 2, 2], lane.0, lane.1));
        assert!(!crosses([4, 5, 2, 2], lane.0, lane.1));
    }
}
