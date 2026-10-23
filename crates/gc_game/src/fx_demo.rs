//! `--fx-demo` and `--fx-still`: every particle effect at the camera's focus,
//! to look at. Twelve stations 1.3 tiles apart along x: fires, smoke and
//! motes run all the time, and the one-shots go off together every few
//! seconds. The demo plays at a quarter speed so a spark can be seen; the
//! still gallery stops time and sets the whole show out afresh every frame,
//! each effect at several ages, for a picture.

use gc_render::{DUST, Effects, HitStyle};
use glam::{Vec2, Vec3};

/// Seconds (of effects time) between the one-shots.
const EVERY: f32 = 1.5;
/// How fast the live demo plays against game time.
const SPEED: f32 = 0.25;
const STATION: f32 = 1.3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Live,
    Still,
}

pub struct Demo {
    mode: Mode,
    clock: f32,
    next: f32,
}

impl Demo {
    pub fn new(mode: Mode) -> Demo {
        Demo {
            mode,
            clock: 0.0,
            next: 0.0,
        }
    }

    /// Whether each frame's particles replace the last frame's.
    pub fn is_still(&self) -> bool {
        self.mode == Mode::Still
    }

    /// How fast effects run against game time.
    pub fn time_scale(&self) -> f32 {
        match self.mode {
            Mode::Live => SPEED,
            Mode::Still => 0.0,
        }
    }

    /// Set off this frame's effects, `dt` seconds of effects time on.
    pub fn play(&mut self, fx: &mut Effects, dt: f32, focus: Vec2, ground: &impl Fn(Vec2) -> f32) {
        self.clock += dt;
        let burst = self.clock >= self.next;
        if burst {
            self.next = self.clock + EVERY;
        }
        let at = |i: usize, row: f32, dy: f32| {
            let p = focus + Vec2::new((i as f32 - 5.5) * STATION, row);
            Vec3::new(p.x, ground(p) + dy, p.y)
        };
        match self.mode {
            Mode::Live => {
                running(fx, |i| at(i, 0.0, 0.0), dt);
                if burst {
                    // Rows part-way through their lives, so the nearer ones
                    // show each effect later.
                    for (row, age) in [(-1.6, 0.9), (0.0, 0.35), (1.6, 0.1)] {
                        let mark = fx.mark();
                        one_shots(fx, |i, dy| at(i, row, dy));
                        fx.age_since(mark, age);
                    }
                }
            }
            Mode::Still => {
                for (row, age) in [(-1.2, 0.12), (0.0, 0.35), (1.2, 0.8), (2.4, 1.5)] {
                    let mark = fx.mark();
                    one_shots(fx, |i, dy| at(i, row, dy));
                    fx.age_since(mark, age);
                }
                // The running effects as streams already flowing, behind.
                let step = 0.1;
                for k in 0..40 {
                    let mark = fx.mark();
                    running(fx, |i| at(i, -3.0, 0.0), step);
                    fx.age_since(mark, k as f32 * step);
                }
            }
        }
    }
}

/// The effects that run all the time, at stations 2, 3, 7, 10 and 11.
fn running(fx: &mut Effects, at: impl Fn(usize) -> Vec3, dt: f32) {
    fx.smoke(at(2), 1.0, 0.85, dt);
    fx.flames(at(2), 1.0, dt);
    fx.embers(at(3), 0.7, 14.0 * dt);
    fx.smoke(at(7), 1.6, 0.3, dt);
    fx.motes(at(10), 0.8, [150, 235, 235], 8.0 * dt);
}

/// The effects that go off once, at stations 0 and 1 and 4 to 9.
fn one_shots(fx: &mut Effects, at: impl Fn(usize, f32) -> Vec3) {
    // A column of soldiers marching past: eight steps along a line.
    for k in 0..8 {
        let mut p = at(0, 0.0);
        p.x += k as f32 * 0.2 - 0.7;
        fx.footstep(p, Vec2::X, 3.0, DUST);
    }
    fx.dust_burst(at(1, 0.0), 1.4, 14.0, DUST);
    let (chest, dir) = (0.7, Vec3::new(1.0, 0.3, 0.0));
    fx.hit(at(4, chest), dir, HitStyle::Flesh, true, 1.0);
    fx.hit(at(5, chest), dir, HitStyle::Ichor, false, 1.0);
    fx.hit(at(6, chest), dir, HitStyle::Crystal, false, 1.0);
    fx.hit(at(7, chest), dir, HitStyle::Ghost, false, 1.0);
    fx.hit(at(8, chest), dir, HitStyle::Ember, true, 1.0);
    fx.debris(at(9, 0.2), [110, 104, 96], 14.0, 1.0);
}
