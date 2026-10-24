//! The picture's showmanship (docs/30-cinematic-lighting.md, stage 4): a
//! screen shake when a charge hits home, an ambush is sprung or a building
//! falls, and letterbox bars sliding in when a battle begins. Presentation
//! only: the renderer moves the finished picture (`Renderer::set_cinema`),
//! so the HUD stays still, clicks land where they were aimed, and nothing
//! reaches the simulation.

use gc_render::Cinema as Frame;
use glam::Vec2;

use crate::camera::DEFAULT_DISTANCE;
use crate::land::smooth;

/// How fast the shake dies away, in full shakes a second.
const DECAY: f32 = 1.5;
/// The most the picture moves, as a share of its size, at full shake.
const MOST: f32 = 0.007;
/// How fast the shake wobbles, in rounds a second.
const WOBBLE: f32 = 2.6;
/// A blow is felt this far from the camera's focus, in tiles, plus half the
/// camera's distance from the ground.
const REACH: f32 = 30.0;
/// How near the camera must be to feel a blow in full, and how much more
/// it feels a blow when closer, at most.
const CLOSEST_FEEL: f32 = 1.6;
const FARTHEST_FEEL: f32 = 0.4;

/// The letterbox bars: each one's height as a share of the screen, and
/// how long they take to slide in, stay and slide out, in seconds.
const BAR_HEIGHT: f32 = 0.085;
const BAR_IN: f32 = 0.7;
const BAR_HOLD: f32 = 2.2;
const BAR_OUT: f32 = 0.9;

/// How hard each kind of blow shakes the screen, in full shakes.
pub const CHARGE_HIT: f32 = 0.35;
pub const AMBUSH: f32 = 0.3;
pub const BUILDING_FELL: f32 = 0.5;

#[derive(Default)]
pub struct Cinema {
    /// How shaken the screen is now, 0 to 1; the picture moves by its
    /// square, so small shakes are small.
    trauma: f32,
    /// Seconds since the first frame, for the wobble.
    clock: f32,
    /// Seconds since the bars started to slide in; `None` when they are
    /// not showing.
    bars: Option<f32>,
}

impl Cinema {
    /// A blow of `strength` (see [`CHARGE_HIT`]) at `at`, felt by a camera
    /// looking at `focus` from `distance`: less the farther off it is, more
    /// the closer the camera.
    pub fn blow(&mut self, at: Vec2, focus: Vec2, distance: f32, strength: f32) {
        let reach = REACH + distance * 0.5;
        let near = (1.0 - at.distance(focus) / reach).clamp(0.0, 1.0);
        let close = (DEFAULT_DISTANCE / distance.max(1.0)).clamp(FARTHEST_FEEL, CLOSEST_FEEL);
        self.trauma = (self.trauma + strength * near * near * close).min(1.0);
    }

    /// Slide the letterbox bars in, hold them, and slide them out: a
    /// battle has begun.
    pub fn battle_begins(&mut self) {
        self.bars = Some(0.0);
    }

    /// The bars and the shake for a frame `dt` seconds on. `still` is a
    /// player who asked for no screen shake (Reduced screen shake);
    /// `bars` is whether bars may show (not with Clarity).
    pub fn frame(&mut self, dt: f32, still: bool, bars: bool) -> Frame {
        // The wobble's clock only runs while there is a shake to wobble, so
        // it stays small.
        if self.trauma > 0.0 {
            self.clock += dt;
        }
        self.trauma = (self.trauma - DECAY * dt).max(0.0);
        let shake = if still {
            [0.0; 2]
        } else {
            let power = self.trauma * self.trauma * MOST;
            let t = self.clock * WOBBLE;
            [power * wobble(t, 0.0), power * wobble(t * 1.31, 5.7)]
        };
        let bars = match self.bars {
            Some(age) if bars => {
                let age = age + dt;
                // Gone once they have slid out.
                self.bars = (age < BAR_IN + BAR_HOLD + BAR_OUT).then_some(age);
                bar_height(age)
            }
            // Not wanted (Clarity): forget them.
            Some(_) => {
                self.bars = None;
                0.0
            }
            None => 0.0,
        };
        Frame { shake, bars }
    }

    /// Start a new match with a still screen and no bars.
    pub fn reset(&mut self) {
        *self = Cinema::default();
    }
}

/// A smooth wobble between -1 and 1 that never quite repeats.
fn wobble(t: f32, phase: f32) -> f32 {
    0.6 * (std::f32::consts::TAU * t + phase).sin()
        + 0.4 * (std::f32::consts::TAU * t * 2.37 + phase * 1.7).sin()
}

/// How tall each bar is `age` seconds after they began to slide in.
fn bar_height(age: f32) -> f32 {
    let slid_in = smooth(0.0, BAR_IN, age);
    let slid_out = smooth(BAR_IN + BAR_HOLD, BAR_IN + BAR_HOLD + BAR_OUT, age);
    BAR_HEIGHT * (slid_in - slid_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Vec2 = Vec2::new(50.0, 50.0);

    #[test]
    fn a_blow_shakes_the_screen_and_the_shake_dies_away() {
        let mut c = Cinema::default();
        c.blow(AT, AT, DEFAULT_DISTANCE, CHARGE_HIT);
        let shaken = c.frame(0.016, false, true).shake;
        assert!(shaken.iter().any(|v| v.abs() > 0.0), "{shaken:?}");
        assert!(shaken.iter().all(|v| v.abs() <= MOST));
        for _ in 0..200 {
            c.frame(0.016, false, true);
        }
        assert_eq!(c.frame(0.016, false, true).shake, [0.0; 2]);
    }

    #[test]
    fn a_far_blow_is_hardly_felt_and_a_near_camera_feels_more() {
        let far = Vec2::new(500.0, 500.0);
        let (mut a, mut b, mut c) = (Cinema::default(), Cinema::default(), Cinema::default());
        a.blow(far, AT, DEFAULT_DISTANCE, 1.0);
        b.blow(AT, AT, 200.0, CHARGE_HIT);
        c.blow(AT, AT, 20.0, CHARGE_HIT);
        assert_eq!(a.trauma, 0.0);
        assert!(c.trauma > b.trauma && b.trauma > 0.0);
        assert!(c.trauma <= 1.0);
    }

    #[test]
    fn reduced_shake_keeps_the_screen_still() {
        let mut c = Cinema::default();
        c.blow(AT, AT, 20.0, 1.0);
        assert_eq!(c.frame(0.016, true, true).shake, [0.0; 2]);
    }

    #[test]
    fn the_bars_slide_in_hold_and_slide_out() {
        let mut c = Cinema::default();
        assert_eq!(c.frame(0.1, false, true).bars, 0.0);
        c.battle_begins();
        let mut tallest = 0.0f32;
        let mut seen = Vec::new();
        for _ in 0..400 {
            let bars = c.frame(0.016, false, true).bars;
            assert!((0.0..=BAR_HEIGHT).contains(&bars));
            tallest = tallest.max(bars);
            seen.push(bars);
        }
        assert!((tallest - BAR_HEIGHT).abs() < 1e-4);
        // Rising, then falling, then gone.
        assert!(seen[10] > 0.0 && seen[10] < tallest);
        assert_eq!(*seen.last().unwrap(), 0.0);
        assert!(c.bars.is_none());
    }

    #[test]
    fn clarity_shows_no_bars() {
        let mut c = Cinema::default();
        c.battle_begins();
        for _ in 0..100 {
            assert_eq!(c.frame(0.016, false, false).bars, 0.0);
        }
    }
}
