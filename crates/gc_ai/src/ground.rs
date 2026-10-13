//! Reading the ground for a battle line (docs/18-ai.md, "Using ground").
//!
//! The Captain asks three things of each place its line could stand:
//! - **Footing:** can the whole formation, archers included, stand there
//!   out of the water?
//! - **Cover:** is there ground no regiment crosses (a cliff, deep water,
//!   the map's edge) right beside an end of the line, all along its depth?
//!   An enemy can't get round an end anchored on it, so the line can only
//!   be fought from the front and the open side.
//! - **Rise:** how much higher does the line stand than the ground in
//!   front of it? The simulation doesn't reward high ground yet (docs/05
//!   plans +10% melee downhill and +15% range), so the AI only leans
//!   towards it a little for now.
//!
//! And two about the way to the enemy:
//! - **Water:** is there a river or lake between the armies? Whoever
//!   crosses it comes out of the water a regiment at a time, so the side
//!   that waits for the other to cross gets to fight them as they do.
//! - **Fords:** where can it be waded? That is where they will come out.

use glam::Vec2;

use crate::perception::right_of;
use crate::terrain::Terrain;

/// How far out from the end of a line ground still counts as cover, tiles.
pub const COVER_REACH: f32 = 6.0;
/// Water narrower than this along the way is a puddle, not a river, tiles.
pub const MIN_WATER: f32 = 2.0;
/// Steps the ground is sampled at, tiles.
const STEP: f32 = 0.5;

/// Where a battle line could stand: its middle, which way it faces, how
/// wide and deep it is (tiles).
#[derive(Clone, Copy, Debug)]
pub struct Line {
    pub centre: Vec2,
    pub forward: Vec2,
    pub width: f32,
    pub depth: f32,
}

impl Line {
    fn at(&self, across: f32, ahead: f32) -> Vec2 {
        self.centre + right_of(self.forward) * across + self.forward * ahead
    }

    /// Points spread across the line, about two tiles apart, `inset` tiles
    /// in from each end.
    fn across(&self, inset: f32) -> impl Iterator<Item = f32> {
        let half = (self.width * 0.5 - inset).max(0.0);
        let steps = half.ceil().max(1.0) as i32;
        (0..=steps).map(move |k| -half + 2.0 * half * k as f32 / steps as f32)
    }

    /// Share of the places under the line, and under a row `behind` tiles
    /// behind it (the archers), where a regiment can stand out of the water.
    pub fn footing(&self, terrain: &Terrain, behind: f32) -> f32 {
        let rows = [
            self.depth * 0.5 - 0.5,
            0.0,
            -self.depth * 0.5 + 0.5,
            -self.depth * 0.5 - behind,
        ];
        let (mut good, mut all) = (0, 0);
        for ahead in rows {
            for across in self.across(2.0) {
                let p = self.at(across, ahead);
                all += 1;
                good += usize::from(terrain.walkable(p) && !terrain.wet(p));
            }
        }
        good as f32 / all.max(1) as f32
    }

    /// For the left and right ends: tiles from the end of the line out to
    /// ground no regiment crosses, if there is some within [`COVER_REACH`]
    /// level with the line's front, its middle and its back, and a couple of
    /// tiles beyond each (the farthest of those).
    pub fn cover(&self, terrain: &Terrain) -> [Option<f32>; 2] {
        [-1.0, 1.0].map(|side| {
            let mut worst: f32 = 0.0;
            for ahead in [self.depth * 0.5 + 2.0, 0.0, -self.depth * 0.5 - 2.0] {
                let mut out = 0.0;
                let found = loop {
                    if out > COVER_REACH {
                        break None;
                    }
                    if terrain.blocked(self.at(side * (self.width * 0.5 + out), ahead)) {
                        break Some(out);
                    }
                    out += STEP;
                };
                worst = worst.max(found?);
            }
            Some(worst)
        })
    }

    /// How much higher the line stands than the ground `ahead` tiles in
    /// front of it, on average across the line, tiles.
    pub fn rise(&self, terrain: &Terrain, ahead: f32) -> f32 {
        let (mut sum, mut n) = (0.0, 0);
        for across in self.across(0.0) {
            sum += terrain.height(self.at(across, 0.0)) - terrain.height(self.at(across, ahead));
            n += 1;
        }
        sum / n.max(1) as f32
    }
}

/// The first stretch of water on the straight way between two places.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crossing {
    /// Tiles from the start of the way to the water, and to the far bank.
    pub near: f32,
    pub far: f32,
}

/// Water (wadeable or not) on the straight way from `from` to `to`, at
/// least [`MIN_WATER`] across, if any. Water `to` itself stands in counts:
/// the enemy is crossing.
pub fn crossing(terrain: &Terrain, from: Vec2, to: Vec2) -> Option<Crossing> {
    let d = to - from;
    let len = d.length();
    if len < 1.0 {
        return None;
    }
    let dir = d / len;
    let mut start: Option<f32> = None;
    let mut s = 0.0;
    while s <= len {
        let water = terrain.water(from + dir * s);
        match (start, water) {
            (None, true) => start = Some(s),
            (Some(a), false) => {
                if s - a >= MIN_WATER {
                    return Some(Crossing { near: a, far: s });
                }
                start = None;
            }
            _ => {}
        }
        s += STEP;
    }
    start.map(|a| Crossing { near: a, far: len })
}

/// The nearest ford to `from` across water lying `water.near` to
/// `water.far` tiles ahead of it along `forward`: a lane straight across
/// with no blocked ground in it, within `reach` tiles to either side.
/// Returns where the lane comes out of the water on `from`'s side.
pub fn ford(
    terrain: &Terrain,
    from: Vec2,
    forward: Vec2,
    water: Crossing,
    reach: f32,
) -> Option<Vec2> {
    let right = right_of(forward);
    let mut best: Option<(f32, Vec2)> = None;
    let mut across = -reach;
    while across <= reach {
        let start = from + right * across;
        if best.is_none_or(|(d, _)| across.abs() < d)
            && let Some(exit) = lane(terrain, start, forward, water.far + MIN_WATER * 4.0)
        {
            best = Some((across.abs(), exit));
        }
        across += 1.0;
    }
    best.map(|(_, exit)| exit)
}

/// Walking `length` tiles from `start` along `forward`: if the first water
/// met can be waded all the way across, where it starts.
fn lane(terrain: &Terrain, start: Vec2, forward: Vec2, length: f32) -> Option<Vec2> {
    let mut entered: Option<Vec2> = None;
    let mut s = 0.0;
    while s <= length {
        let p = start + forward * s;
        if terrain.water(p) {
            if terrain.blocked(p) {
                return None;
            }
            entered.get_or_insert(p);
        } else if let Some(e) = entered {
            return (e.distance(p) >= MIN_WATER).then_some(e);
        } else if terrain.blocked(p) {
            // A cliff before any water: not this way.
            return None;
        }
        s += STEP;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};

    /// A 64-tile map: open ground two tiles high, with a cliff from x = 40
    /// rightwards for y in 20..44, and water one tile deep along y in
    /// 50..56, blocked except for a ford at x in 28..36.
    fn ground() -> Terrain {
        let tiles = 64;
        let side = tiles * CELLS_PER_TILE as usize;
        let mut nav = vec![1u8; side * side];
        let mut heights = vec![2.0f32; tiles * tiles];
        for cy in 0..side {
            for cx in 0..side {
                let (x, y) = (cx as f32 / 2.0, cy as f32 / 2.0);
                let cliff = x >= 40.0 && (20.0..44.0).contains(&y);
                let river = (50.0..56.0).contains(&y);
                let ford = (28.0..36.0).contains(&x);
                if cliff || (river && !ford) {
                    nav[cy * side + cx] = BLOCKED;
                } else if river {
                    nav[cy * side + cx] = 3;
                }
            }
        }
        for y in 50..56 {
            for x in 0..tiles {
                heights[y * tiles + x] = 0.0;
            }
        }
        // A rise in the bottom-left corner.
        for y in 0..16 {
            for x in 0..16 {
                heights[y * tiles + x] = 5.0;
            }
        }
        Terrain::new(&nav, tiles as u32).with_heights(heights, 1.0)
    }

    /// Facing up the map (towards smaller y), so its right is +x.
    fn line(centre: Vec2) -> Line {
        Line {
            centre,
            forward: -Vec2::Y,
            width: 20.0,
            depth: 3.0,
        }
    }

    #[test]
    fn a_line_beside_the_cliff_is_covered_on_that_side_only() {
        let t = ground();
        // Right end at x = 38.5: the cliff is 1.5 tiles out.
        let [left, right] = line(Vec2::new(28.5, 32.0)).cover(&t);
        assert_eq!(left, None);
        let right = right.unwrap();
        assert!((1.0..=2.0).contains(&right), "{right}");
        // Too far from it.
        assert_eq!(line(Vec2::new(20.0, 32.0)).cover(&t), [None, None]);
        // Level with the cliff's end, it doesn't cover the line's front.
        assert_eq!(line(Vec2::new(28.5, 43.0)).cover(&t)[1], None);
    }

    #[test]
    fn footing_keeps_the_line_out_of_the_water_and_off_the_cliff() {
        let t = ground();
        assert_eq!(line(Vec2::new(20.0, 32.0)).footing(&t, 6.0), 1.0);
        assert!(line(Vec2::new(32.0, 53.0)).footing(&t, 6.0) < 0.5);
        assert!(line(Vec2::new(40.0, 32.0)).footing(&t, 6.0) < 0.8);
    }

    #[test]
    fn the_ford_is_wet_and_the_deep_water_blocked() {
        let t = ground();
        assert!(t.wet(Vec2::new(32.0, 53.0)));
        assert!(t.water(Vec2::new(10.0, 53.0)) && t.blocked(Vec2::new(10.0, 53.0)));
        assert!(!t.wet(Vec2::new(10.0, 53.0)));
        assert!(!t.water(Vec2::new(32.0, 40.0)));
    }

    #[test]
    fn water_between_the_armies_is_found_wherever_the_way_crosses_it() {
        let t = ground();
        let c = crossing(&t, Vec2::new(32.0, 40.0), Vec2::new(32.0, 62.0)).unwrap();
        assert!(
            (c.near - 10.0).abs() <= 0.5 && (c.far - 16.0).abs() <= 0.5,
            "{c:?}"
        );
        // Across the deep part too.
        assert!(crossing(&t, Vec2::new(10.0, 40.0), Vec2::new(10.0, 62.0)).is_some());
        // Not when both stand on the same bank.
        assert_eq!(
            crossing(&t, Vec2::new(10.0, 20.0), Vec2::new(30.0, 45.0)),
            None
        );
        // An enemy in the water is crossing.
        let c = crossing(&t, Vec2::new(32.0, 40.0), Vec2::new(32.0, 52.0)).unwrap();
        assert_eq!((c.near, c.far), (10.0, 12.0));
    }

    #[test]
    fn the_ford_is_found_where_the_river_can_be_waded() {
        let t = ground();
        let from = Vec2::new(12.0, 40.0);
        let water = crossing(&t, from, Vec2::new(12.0, 62.0)).unwrap();
        // Facing +y, the ford (x 28 to 36) is 16 tiles to the side.
        let exit = ford(&t, from, Vec2::Y, water, 30.0).unwrap();
        assert!(
            (exit.x - 28.0).abs() <= 1.0 && (exit.y - 50.0).abs() <= 0.5,
            "{exit}"
        );
        assert_eq!(ford(&t, from, Vec2::Y, water, 10.0), None);
    }

    #[test]
    fn the_rise_is_measured_against_the_ground_in_front() {
        let t = ground();
        let on_the_hill = Line {
            centre: Vec2::new(8.0, 12.0),
            forward: Vec2::Y,
            width: 10.0,
            depth: 3.0,
        };
        assert_eq!(on_the_hill.rise(&t, 8.0), 3.0);
        assert_eq!(line(Vec2::new(20.0, 32.0)).rise(&t, 8.0), 0.0);
    }
}
