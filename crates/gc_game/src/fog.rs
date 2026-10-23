//! Drawing fog of war on the ground (docs/08-ui-ux.md, "Fog of war").
//!
//! The simulation says where the viewer's side can see, as circles
//! (`World::sight`). A few times a second this turns them into a grid with
//! one byte a tile for the renderer, which blurs the edges: 255 where the
//! side sees now, [`EXPLORED`] where it has seen before, [`UNEXPLORED`]
//! where it has not. What has been seen is remembered here only; it is not
//! part of the match and is forgotten when a new one starts.

use std::ops::Range;
use std::time::{Duration, Instant};

use gc_render::Renderer;
use gc_sim::{PlayerId, World};
use glam::Vec2;

/// How much of the ground shows on a tile seen before but not in sight now.
pub const EXPLORED: u8 = 140;
/// ... and on a tile never seen.
pub const UNEXPLORED: u8 = 51;
/// The grid is redrawn this often.
const EVERY: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct FogView {
    canvas: Canvas,
    /// `World::sight`'s circles, kept to save allocating.
    sight: Vec<(gc_fixed::FxVec2, gc_fixed::Fx)>,
    /// When the grid was last drawn, and for which side.
    last: Option<(Instant, PlayerId)>,
}

/// The grid and what has been seen on it.
#[derive(Default)]
struct Canvas {
    /// Tiles a side; 0 before the first update.
    tiles: usize,
    /// Per tile, the sides that have seen it at some point this match, a
    /// bit each (so taking command of another side shows what it has seen).
    explored: Vec<u8>,
    /// What the renderer is given.
    grid: Vec<u8>,
}

impl FogView {
    /// Forget what has been seen, for a new match.
    pub fn reset(&mut self) {
        self.canvas.explored.fill(0);
        self.last = None;
    }

    /// Whether the viewer sees `tile` now: always when there is no fog (or
    /// it has not been drawn yet), else when the tile's centre is inside one
    /// of its sight circles as of the last redraw.
    pub fn sees_tile(&self, tile: (i32, i32)) -> bool {
        if self.last.is_none() {
            return true;
        }
        let centre = Vec2::new(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5);
        self.sight
            .iter()
            .any(|&(c, r)| crate::from_fx_vec(c).distance(centre) <= crate::fx_to_f32(r))
    }

    /// Draw the ground for `side` if the match is in fog and it is
    /// `viewing` through its eyes, else without fog. Call it every frame;
    /// it only redraws a few times a second.
    pub fn update(
        &mut self,
        now: Instant,
        renderer: &mut Renderer,
        world: &World,
        side: PlayerId,
        tiles: u32,
        viewing: bool,
    ) {
        if !(viewing && world.fog()) {
            renderer.set_fog(None);
            self.last = None;
            return;
        }
        if self
            .last
            .is_some_and(|(t, s)| s == side && now.duration_since(t) < EVERY)
        {
            return;
        }
        self.last = Some((now, side));
        world.sight(side, &mut self.sight);
        let circles =
            (self.sight.iter()).map(|&(c, r)| (crate::from_fx_vec(c), crate::fx_to_f32(r)));
        self.canvas.paint(tiles as usize, side, circles);
        renderer.set_fog(Some(&self.canvas.grid));
    }
}

impl Canvas {
    /// Fill `grid` for a map `tiles` across and the circles `side` has in
    /// sight (centre and radius, in tiles), and remember what they cover.
    fn paint(
        &mut self,
        tiles: usize,
        side: PlayerId,
        circles: impl IntoIterator<Item = (Vec2, f32)>,
    ) {
        if self.tiles != tiles {
            self.tiles = tiles;
            self.explored.clear();
            self.explored.resize(tiles * tiles, 0);
            self.grid.clear();
            self.grid.resize(tiles * tiles, 0);
        }
        self.grid.fill(0);
        for (c, r) in circles {
            // Light the tiles whose centres (x + 0.5, y + 0.5) lie inside
            // the circle, a row at a time.
            for y in span(c.y, r, tiles) {
                let dy = y as f32 + 0.5 - c.y;
                let half = (r * r - dy * dy).max(0.0).sqrt();
                let xs = span(c.x, half, tiles);
                let row = y * tiles;
                self.grid[row + xs.start..row + xs.end].fill(255);
            }
        }
        let bit = 1u8 << side.min(7);
        for (cell, seen) in self.grid.iter_mut().zip(&mut self.explored) {
            if *cell == 255 {
                *seen |= bit;
            } else {
                *cell = if *seen & bit != 0 {
                    EXPLORED
                } else {
                    UNEXPLORED
                };
            }
        }
    }
}

/// The tiles along one axis of a map `tiles` across whose centres lie
/// within `half` of `centre`; empty if none do.
fn span(centre: f32, half: f32, tiles: usize) -> Range<usize> {
    let lo = (centre - half - 0.5).ceil().max(0.0);
    let hi = (centre + half - 0.5).floor().min(tiles as f32 - 1.0);
    if lo <= hi {
        lo as usize..hi as usize + 1
    } else {
        0..0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_circle_lights_the_tiles_whose_centres_it_holds() {
        let mut f = Canvas::default();
        f.paint(8, 0, [(Vec2::new(4.0, 4.0), 1.0)]);
        // Centres (3.5, 3.5) and the like are 0.71 away; (2.5, 3.5) is 1.58.
        let lit: Vec<usize> = (0..64).filter(|&i| f.grid[i] == 255).collect();
        assert_eq!(lit, vec![27, 28, 35, 36]);
        assert_eq!(f.grid[0], UNEXPLORED);
    }

    #[test]
    fn what_was_seen_is_remembered() {
        let mut f = Canvas::default();
        f.paint(8, 0, [(Vec2::new(4.0, 4.0), 1.0)]);
        f.paint(8, 0, [(Vec2::new(1.0, 1.0), 1.0)]);
        assert_eq!(f.grid[0], 255);
        assert_eq!(f.grid[27], EXPLORED);
        assert_eq!(f.grid[63], UNEXPLORED);
        f.explored.fill(0);
        f.paint(8, 0, []);
        assert!(f.grid.iter().all(|&v| v == UNEXPLORED));
    }

    #[test]
    fn circles_off_the_edge_are_clipped() {
        let mut f = Canvas::default();
        f.paint(
            8,
            0,
            [
                (Vec2::new(-10.0, 4.0), 3.0),
                (Vec2::new(20.0, 4.0), 3.0),
                (Vec2::new(7.5, 7.5), 100.0),
            ],
        );
        assert!(f.grid.iter().all(|&v| v == 255));
    }

    #[test]
    fn each_side_remembers_its_own() {
        let mut f = Canvas::default();
        f.paint(8, 0, [(Vec2::new(4.0, 4.0), 1.0)]);
        f.paint(8, 1, []);
        assert_eq!(f.grid[27], UNEXPLORED, "side 1 never saw it");
        f.paint(8, 0, []);
        assert_eq!(f.grid[27], EXPLORED);
    }
}
