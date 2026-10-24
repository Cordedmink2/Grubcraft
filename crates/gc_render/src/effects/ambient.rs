//! Recipes for the living world (docs/34-ambient-life.md): work you can see
//! in the land, not fighting.

use super::*;

impl Effects {
    /// Leaves shaken loose over `spread` tiles round `pos`: a tree coming
    /// down, a bush torn up. They tumble down on the wind and fade.
    pub fn leaves(&mut self, pos: Vec3, spread: f32, color: [u8; 3], amount: f32) {
        for _ in 0..self.count(amount) {
            let at = self.rng.disc() * spread;
            let c = self.jitter(color, 0.25);
            let s = self.rng.between(0.04, 0.08);
            let spin = self.spin(40);
            self.out.push(Particle {
                pos: pos + Vec3::new(at.x, self.rng.between(0.0, 0.5), at.y),
                vel: Vec3::new(
                    self.rng.between(-0.5, 0.5),
                    self.rng.between(0.0, 0.8),
                    self.rng.between(-0.5, 0.5),
                ),
                life: self.rng.between(1.8, 3.2),
                size: [s, s * 0.8],
                gravity: 1.4,
                drag: 1.5,
                shape: Shape::Shard,
                flags: flags::GROUND | flags::WIND,
                spin,
                seed: self.rng.byte(),
                ..fading(c, 235)
            });
        }
    }

    /// Chips of wood or torn turf flying from a tool's bite at `pos`,
    /// thrown toward `dir` (unit, on the ground plane): a few light bits
    /// that skid to rest.
    pub fn cut_chips(&mut self, pos: Vec3, dir: Vec2, color: [u8; 3], amount: f32) {
        for _ in 0..self.count(amount) {
            let side = self.rng.disc() * 0.5;
            let c = self.jitter(color, 0.2);
            let s = self.rng.between(0.025, 0.05);
            let spin = self.spin(80);
            self.out.push(Particle {
                pos,
                vel: Vec3::new(
                    dir.x * 0.9 + side.x,
                    self.rng.between(0.9, 1.7),
                    dir.y * 0.9 + side.y,
                ),
                life: self.rng.between(0.9, 1.6),
                size: [s, s],
                gravity: 9.0,
                drag: 0.5,
                shape: Shape::Shard,
                flags: flags::GROUND,
                spin,
                seed: self.rng.byte(),
                ..fading(c, 255)
            });
        }
    }

    /// A speck of pollen or a drifting seed at `pos`: tiny, pale gold, it
    /// wanders on the wind for several seconds, catching the light.
    pub fn pollen(&mut self, pos: Vec3, amount: f32) {
        for _ in 0..self.count(amount) {
            let c = self.jitter([255, 238, 170], 0.1);
            let s = self.rng.between(0.018, 0.032);
            self.out.push(Particle {
                pos,
                vel: Vec3::new(
                    self.rng.between(-0.2, 0.2),
                    self.rng.between(0.02, 0.2),
                    self.rng.between(-0.2, 0.2),
                ),
                life: self.rng.between(5.0, 9.0),
                size: [s, s * 0.7],
                drag: 0.5,
                lift: 0.03,
                shape: Shape::Glow,
                flags: flags::ADDITIVE | flags::WIND | flags::FLICKER,
                seed: self.rng.byte(),
                ..fading(c, 150)
            });
        }
    }

    /// A firefly at `pos`: a warm green-yellow point that drifts slowly and
    /// pulses, bright enough to bloom at dusk.
    pub fn firefly(&mut self, pos: Vec3, amount: f32) {
        for _ in 0..self.count(amount) {
            let c = self.jitter([190, 255, 110], 0.08);
            self.out.push(Particle {
                pos,
                vel: Vec3::new(
                    self.rng.between(-0.25, 0.25),
                    self.rng.between(-0.05, 0.15),
                    self.rng.between(-0.25, 0.25),
                ),
                life: self.rng.between(3.5, 6.5),
                size: [0.05, 0.035],
                drag: 0.7,
                shape: Shape::Glow,
                flags: flags::ADDITIVE | flags::FLICKER | flags::WIND,
                seed: self.rng.byte(),
                ..fading(c, 255)
            });
        }
    }

    /// Dust thrown up by a whirlwind standing at `centre`, for `dt` seconds
    /// of effects time: puffs that climb in a column that spreads as it rises,
    /// with a sideways push to each so it looks wound up.
    /// `centre` moves between calls to make the devil wander.
    pub fn dust_devil(&mut self, centre: Vec3, dt: f32, color: [u8; 3]) {
        for _ in 0..self.count(34.0 * dt) {
            let a = self.rng.between(0.0, std::f32::consts::TAU);
            let (sin, cos) = a.sin_cos();
            let c = self.jitter(color, 0.15);
            let shape = self.puff();
            let (seed, spin) = (self.rng.byte(), self.spin(14));
            let r = self.rng.between(0.08, 0.3);
            self.out.push(Particle {
                pos: centre + Vec3::new(cos * r, 0.05, sin * r),
                // A little tangent to the turn, and up.
                vel: Vec3::new(-sin * 0.8, self.rng.between(1.8, 3.2), cos * 0.8),
                life: self.rng.between(1.6, 2.6),
                size: [0.12, self.rng.between(0.5, 0.9)],
                drag: 0.9,
                lift: 0.5,
                shape,
                flags: flags::SOFT | flags::WIND,
                spin,
                seed,
                ..fading(c, 130)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recipes_make_particles_that_live_and_fade() {
        let mut fx = Effects::new(3);
        fx.leaves(Vec3::new(5.0, 1.0, 5.0), 1.0, [70, 110, 50], 20.0);
        fx.cut_chips(Vec3::new(5.0, 1.0, 5.0), Vec2::X, [200, 160, 100], 10.0);
        fx.pollen(Vec3::new(5.0, 1.0, 5.0), 5.0);
        fx.firefly(Vec3::new(5.0, 1.0, 5.0), 5.0);
        fx.dust_devil(Vec3::new(5.0, 0.0, 5.0), 1.0, [196, 172, 134]);
        let made = fx.take();
        assert_eq!(made.len(), 30 + 5 + 5 + 34);
        for p in &made {
            assert!(p.life > 0.0 && p.size[0] > 0.0);
            assert_eq!(p.color_end[3], 0);
        }
    }
}
