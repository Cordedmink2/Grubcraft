//! The living world around the armies (docs/34-ambient-life.md): the work
//! of clearing land, and in time wildlife, villagers and busy bases. All of
//! it is a picture. It reads the simulation's world and events and draws,
//! and nothing here feeds back into the simulation, replays or the network,
//! so it may use floats, frame time and its own random numbers.

pub mod felling;
pub mod touches;

use glam::Vec2;

pub use felling::Felling;
pub use touches::Touches;

use crate::scenery::{Scenery, Stood};

/// Everything ambient the game keeps between frames.
#[derive(Default)]
pub struct Ambient {
    pub felling: Felling,
    pub touches: Touches,
    /// Seconds until `--fell-demo` fells the trees round the camera.
    fell_demo: Option<f32>,
}

/// How long `--fell-demo` waits before it fells, in seconds.
const FELL_DEMO_AFTER: f32 = 4.0;
/// How far from the thickest wood `--fell-demo` fells, in tiles, and how
/// far it looks for that wood.
const FELL_DEMO_REACH: i32 = 5;
const FELL_DEMO_SEARCH: i32 = 30;

impl Ambient {
    pub fn new(fell_demo: bool) -> Ambient {
        Ambient {
            felling: Felling::default(),
            touches: Touches::default(),
            fell_demo: fell_demo.then_some(FELL_DEMO_AFTER),
        }
    }

    /// `--fell-demo`: a few seconds in, fell everything near the thickest
    /// wood within thirty tiles of `focus`, moving `focus` (the camera)
    /// there, so the trees come down for a look without a worker.
    pub fn fell_demo(&mut self, dt: f32, focus: &mut Vec2, scenery: &mut Scenery) {
        let Some(left) = self.fell_demo else {
            return;
        };
        if left > dt {
            self.fell_demo = Some(left - dt);
            return;
        }
        self.fell_demo = None;
        let (fx, fy) = (focus.x as i32, focus.y as i32);
        let mut best = (0, (fx, fy));
        for y in (fy - FELL_DEMO_SEARCH..=fy + FELL_DEMO_SEARCH).step_by(2) {
            for x in (fx - FELL_DEMO_SEARCH..=fx + FELL_DEMO_SEARCH).step_by(2) {
                let trees = (0..2)
                    .flat_map(|dy| (0..2).map(move |dx| (x + dx, y + dy)))
                    .flat_map(|t| scenery.standing(t))
                    .filter(|s| s.tree)
                    .count();
                if trees > best.0 {
                    best = (trees, (x, y));
                }
            }
        }
        let (cx, cy) = best.1;
        *focus = Vec2::new(cx as f32 + 0.5, cy as f32 + 0.5);
        let reach = FELL_DEMO_REACH;
        let tiles =
            (-reach..=reach).flat_map(|dy| (-reach..=reach).map(move |dx| (cx + dx, cy + dy)));
        for tile in tiles {
            // The demo's cutter stands south of the wood.
            self.felling.cutting(tile, *focus + Vec2::new(0.0, 9.0));
            let tall = |s: &Stood| scenery.height_of(s.model);
            self.felling.clear(tile, scenery.standing(tile), tall);
            scenery.fell(std::iter::once(tile));
        }
    }
}
