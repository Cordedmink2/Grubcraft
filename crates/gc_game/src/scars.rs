//! Marks on the ground, as the field shows them (docs/35-ground-marks.md):
//! turned earth where trees were felled (`Event::Cleared`) and bare tracks
//! worn by regiments marching and fighting. A few times a second this fills
//! a grid for the renderer, four bytes a tile: felled, scorched (nothing
//! writes it yet), worn, and one spare. The shader draws the marks; this
//! decides only where. Wear builds where a regiment the viewing side can see moves or fights, and
//! heals over minutes as the grass comes back. Presentation only: it reads
//! the world and writes nothing to it, and it is not part of any replay.

use std::time::{Duration, Instant};

use gc_render::Renderer;
use gc_sim::events::Event;
use gc_sim::{Growth, PlayerId, World};
use glam::Vec2;

/// Wear builds this often.
const EVERY: Duration = Duration::from_millis(250);
/// What a regiment adds to the ground under it each time: marching, and
/// fighting (which churns it up).
const MARCH: f32 = 0.05;
const FIGHT: f32 = 0.15;
/// How much wear heals a second.
const HEAL: f32 = 0.004;
/// Wear heals this often (the whole map is walked, and sent, to do it).
const HEAL_EVERY: Duration = Duration::from_secs(5);
/// How strongly felled ground is marked, by what stood on it.
const SCRUB: u8 = 150;
const WOODS: u8 = 255;

#[derive(Default)]
pub struct ScarView {
    tiles: usize,
    /// Wear per tile, 0 to 1.
    wear: Vec<f32>,
    /// What the renderer is given.
    grid: Vec<u8>,
    /// When wear was last added and last healed.
    last: Option<Instant>,
    healed: Option<Instant>,
    /// The grid differs from what the renderer holds.
    dirty: bool,
}

impl ScarView {
    /// Forget every mark, for a new match or map. The renderer is told at
    /// the next `update`, since it may still hold some.
    pub fn reset(&mut self) {
        *self = Self {
            dirty: true,
            ..Self::default()
        };
    }

    /// Mark the tiles the simulation cleared in the last tick as felled.
    pub fn felled(&mut self, events: &[Event], tiles: u32) {
        let cleared = events.iter().filter_map(|e| match *e {
            Event::Cleared { x, y, was, .. } => Some((x, y, was)),
            _ => None,
        });
        self.size(tiles as usize);
        for (x, y, was) in cleared {
            let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
                continue;
            };
            if x < self.tiles && y < self.tiles {
                let strength = if was == Growth::Scrub { SCRUB } else { WOODS };
                self.grid[(y * self.tiles + x) * 4] = strength;
                self.dirty = true;
            }
        }
    }

    /// Wear the ground under the regiments `side` sees (all with
    /// `reveal_all`) and send the marks to the renderer. `pace` is the
    /// game's speed (0 paused, 1 normal): wear builds with game time and
    /// heals with the clock. Call it every frame; it only works a few times
    /// a second.
    pub fn update(
        &mut self,
        now: Instant,
        renderer: &mut Renderer,
        world: &World,
        (side, reveal_all): (PlayerId, bool),
        tiles: u32,
        pace: f32,
    ) {
        if self.last.is_some_and(|t| now.duration_since(t) < EVERY) {
            return;
        }
        self.last = Some(now);
        self.size(tiles as usize);
        for r in world.regiments.iter().filter(|_| pace > 0.0) {
            let amount = match (r.engaged, r.still) {
                (true, _) => FIGHT,
                (false, false) => MARCH,
                _ => continue,
            };
            if reveal_all || world.sees(side, r.handle) {
                let at = crate::from_fx_vec(r.position);
                let radius = 0.6 + 0.12 * f32::from(r.models).sqrt();
                self.stamp(at, radius, amount * pace);
            }
        }
        if self
            .healed
            .is_none_or(|t| now.duration_since(t) >= HEAL_EVERY)
        {
            let secs = self
                .healed
                .map_or(0.0, |t| now.duration_since(t).as_secs_f32());
            self.healed = Some(now);
            self.heal(HEAL * secs);
        }
        if self.dirty {
            self.dirty = false;
            let marked = self.grid.iter().any(|&b| b != 0);
            renderer.set_scars(marked.then_some(self.grid.as_slice()));
        }
    }

    fn size(&mut self, tiles: usize) {
        if self.tiles != tiles {
            self.tiles = tiles;
            self.wear = vec![0.0; tiles * tiles];
            self.grid = vec![0; tiles * tiles * 4];
        }
    }

    /// Add `amount` of wear to the tiles within `radius` of `at`, most at
    /// the middle.
    fn stamp(&mut self, at: Vec2, radius: f32, amount: f32) {
        let tiles = self.tiles;
        let lo = (at - Vec2::splat(radius)).max(Vec2::ZERO);
        let hi = (at + Vec2::splat(radius)).min(Vec2::splat(tiles as f32));
        for y in lo.y as usize..(hi.y.ceil() as usize).min(tiles) {
            for x in lo.x as usize..(hi.x.ceil() as usize).min(tiles) {
                let gap = Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(at);
                let share = 1.0 - (gap / radius).clamp(0.0, 1.0);
                if share > 0.0 {
                    let i = y * tiles + x;
                    self.wear[i] = (self.wear[i] + amount * share).min(1.0);
                    self.grid[i * 4 + 2] = (self.wear[i] * 255.0).round() as u8;
                    self.dirty = true;
                }
            }
        }
    }

    /// Let every worn tile heal by `by`.
    fn heal(&mut self, by: f32) {
        for (i, w) in self.wear.iter_mut().enumerate().filter(|(_, w)| **w > 0.0) {
            *w = (*w - by).max(0.0);
            self.grid[i * 4 + 2] = (*w * 255.0).round() as u8;
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> ScarView {
        let mut v = ScarView::default();
        v.size(32);
        v
    }

    fn worn(v: &ScarView, x: usize, y: usize) -> u8 {
        v.grid[(y * v.tiles + x) * 4 + 2]
    }

    #[test]
    fn a_stamp_wears_most_at_its_middle_and_nothing_beyond_its_radius() {
        let mut v = view();
        v.stamp(Vec2::new(10.5, 10.5), 2.0, 0.5);
        assert!(v.dirty);
        let (middle, beside, far) = (worn(&v, 10, 10), worn(&v, 11, 10), worn(&v, 14, 10));
        assert!(middle > beside && beside > 0, "{middle} {beside}");
        assert_eq!(far, 0);
        // Wear stops at 1, and the edges of the map are safe.
        for _ in 0..10 {
            v.stamp(Vec2::new(0.0, 0.0), 3.0, 0.5);
        }
        assert_eq!(worn(&v, 0, 0), 255);
        v.stamp(Vec2::new(40.0, 40.0), 3.0, 0.5);
    }

    #[test]
    fn worn_ground_heals_to_nothing() {
        let mut v = view();
        v.stamp(Vec2::new(10.5, 10.5), 2.0, 1.0);
        v.heal(0.5);
        assert!(worn(&v, 10, 10) > 0 && worn(&v, 10, 10) < 255);
        v.heal(1.0);
        assert!((0..32 * 32).all(|i| v.grid[i * 4 + 2] == 0));
    }

    #[test]
    fn cleared_tiles_are_felled_by_what_stood_on_them() {
        let mut v = ScarView::default();
        let cleared = |x, y, was| Event::Cleared {
            x,
            y,
            was,
            burnt: false,
        };
        v.felled(
            &[
                cleared(3, 4, Growth::Woods),
                cleared(5, 6, Growth::Scrub),
                cleared(-1, 0, Growth::Woods),
                cleared(99, 0, Growth::Woods),
            ],
            32,
        );
        assert_eq!(v.grid[(4 * 32 + 3) * 4], WOODS);
        assert_eq!(v.grid[(6 * 32 + 5) * 4], SCRUB);
        assert!(v.dirty);
        v.reset();
        assert!(v.grid.is_empty());
    }
}
