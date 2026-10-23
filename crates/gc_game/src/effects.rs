//! The battlefield's particle effects: where the game asks `gc_render`'s
//! recipes for dust, sparks and smoke. It only collects what to spawn; the
//! frame hands the list to the renderer (`Field::frame`). Nothing here
//! feeds back into the simulation, and effects of what the viewer cannot see
//! are never made, so the dust never shows an army the fog hides.

use gc_render::{DUST, Effects, Particle};
use glam::{Vec2, Vec3};

use crate::fx_demo::{Demo, Mode};

/// How often a soldier on the move kicks up dust, a puff a second at a
/// walk, more at a run.
const FOOTSTEP_RATE: f32 = 0.25;
const FOOTSTEP_RATE_PER_SPEED: f32 = 0.3;
/// The colour of clods thrown up from the ground.
const CLOD: [u8; 3] = [92, 74, 52];

pub struct Field {
    fx: Effects,
    demo: Option<Demo>,
}

impl Field {
    pub fn new(demo: Option<Mode>) -> Field {
        Field {
            fx: Effects::default(),
            demo: demo.map(Demo::new),
        }
    }

    /// Whether the renderer should forget its particles before each frame's
    /// (the still gallery is spawned afresh every frame).
    pub fn still(&self) -> bool {
        self.demo.as_ref().is_some_and(Demo::is_still)
    }

    /// How fast effects run against game time: 1, or the demo's own pace.
    pub fn time_scale(&self) -> f32 {
        self.demo.as_ref().map_or(1.0, Demo::time_scale)
    }

    /// Drop what was collected and not yet handed over.
    pub fn clear(&mut self) {
        self.fx.take();
    }

    /// A soldier at `pos` moved along `dir` at `speed` tiles a second for
    /// `dt` seconds of game time.
    pub fn footstep(&mut self, dt: f32, pos: Vec3, dir: Vec2, speed: f32) {
        if speed > 0.5
            && self
                .fx
                .chance(dt * (FOOTSTEP_RATE + FOOTSTEP_RATE_PER_SPEED * speed))
        {
            self.fx.footstep(pos, dir, speed, DUST);
        }
    }

    /// A volley's arrows came down at `pos` and `lost` soldiers fell: a puff
    /// of dust and a few clods, more for a bloodier volley.
    pub fn arrows_landed(&mut self, pos: Vec3, lost: u16) {
        let n = f32::from(lost.clamp(1, 12));
        self.impact(pos, 0.5 + 0.08 * n, 2.0 + 0.5 * n, 1.5 + 0.3 * n, 0.5);
    }

    /// Two regiments met at `pos` with a charge.
    pub fn charge_hit(&mut self, pos: Vec3) {
        self.impact(pos, 2.2, 16.0, 8.0, 1.0);
    }

    /// Dust rolling out to `radius` from `puffs`, and `clods` thrown up with
    /// `power`.
    fn impact(&mut self, pos: Vec3, radius: f32, puffs: f32, clods: f32, power: f32) {
        self.fx.dust_burst(pos, radius, puffs, DUST);
        self.fx.debris(pos, CLOD, clods, power);
    }

    /// Everything collected this frame, ready for
    /// `Renderer::spawn_particles`. `dt` is the effects time since the last
    /// frame (the frame time, scaled by game speed and `time_scale`), `focus`
    /// the camera's focus (where the demo plays), `ground` the terrain
    /// height, and `density` how much to spawn (the Graphics preset's
    /// choice).
    pub fn frame(
        &mut self,
        dt: f32,
        density: f32,
        focus: Vec2,
        ground: impl Fn(Vec2) -> f32,
    ) -> Vec<Particle> {
        self.fx.set_density(density);
        if let Some(demo) = &mut self.demo {
            demo.play(&mut self.fx, dt, focus, &ground);
        }
        self.fx.take()
    }
}
