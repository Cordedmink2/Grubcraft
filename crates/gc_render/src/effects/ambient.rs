//! Recipes for the living world (docs/34-ambient-life.md): work you can see
//! in the land, not fighting.

use super::*;

/// How far a lit window's light reaches on the walls and ground, in tiles.
const LAMP_REACH: f32 = 2.4;

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

    /// A lit window or lantern at `pos`, kept up by calling it every frame
    /// with `dt` seconds of effects time: a small glow of `color` that
    /// breathes a little, at `level` (0 to 1) of full brightness. `size` is
    /// its radius in tiles. Lamps are not thinned by the density: a lamp
    /// made of a third of its specks would flicker out, and there are few.
    pub fn lamp(&mut self, pos: Vec3, color: [u8; 3], size: f32, level: f32, dt: f32) {
        self.glow(super::Glow {
            pos,
            color,
            radius: LAMP_REACH,
            strength: level.clamp(0.0, 1.0),
            needs_dark: false,
            seed: None,
            placed: false,
        });
        let n = 10.0 * dt;
        let n = n.floor() as u32 + u32::from(self.rng.unit() < n.fract());
        for _ in 0..n {
            let c = self.jitter(color, 0.06);
            self.out.push(Particle {
                pos,
                life: self.rng.between(0.6, 0.9),
                size: [size, size * 0.8],
                drag: 1.0,
                shape: Shape::Glow,
                flags: flags::ADDITIVE | flags::FLICKER,
                seed: self.rng.byte(),
                ..fading(c, (130.0 * level.clamp(0.0, 1.0)) as u8)
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

    /// A fish rising at `pos` on the water's surface: two rings spreading
    /// over a second or two and a bead or two thrown up. `size` scales how
    /// far the rings reach (1 is about half to one tile across at most).
    /// Not `SOFT`, which fades a particle near the surface it sits on (it
    /// would leave only an arc: the ring lies flat on the water).
    pub fn rise(&mut self, pos: Vec3, size: f32) {
        for _ in 0..self.count(1.0) {
            let size = size * self.rng.between(0.75, 1.3);
            for (reach, life, alpha) in [(0.5, 1.9, 140), (0.28, 1.2, 120)] {
                self.out.push(Particle {
                    pos: pos + Vec3::new(0.0, 0.02, 0.0),
                    life,
                    size: [0.03, reach * size],
                    color: [222, 234, 246, alpha],
                    color_end: [222, 234, 246, 0],
                    shape: Shape::Ring,
                    seed: self.rng.byte(),
                    ..Particle::default()
                });
            }
            // The beads are counted by the density here, not again in `spray`.
            self.spray(pos, Vec3::Y, [226, 238, 250], 2.0 / self.density);
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
        fx.lamp(Vec3::new(5.0, 1.0, 5.0), [255, 190, 100], 0.15, 1.0, 1.0);
        let made = fx.take();
        assert_eq!(made.len(), 30 + 5 + 5 + 34 + 10);
        for p in &made {
            assert!(p.life > 0.0 && p.size[0] > 0.0);
            assert_eq!(p.color_end[3], 0);
        }
    }

    #[test]
    fn a_rise_is_rings_on_the_water_and_a_few_beads() {
        let mut fx = Effects::new(3);
        fx.rise(Vec3::new(10.0, 2.0, 20.0), 1.0);
        let made = fx.take();
        let rings: Vec<&Particle> = made.iter().filter(|p| p.shape == Shape::Ring).collect();
        assert_eq!(rings.len(), 2);
        for p in &rings {
            assert!(p.pos.y > 2.0 && p.pos.y < 2.1 && p.size[1] > p.size[0]);
            assert!(p.life > 1.0 && p.life < 2.0);
        }
        assert!(made.len() > 2, "beads");
        let mut none = Effects::new(3);
        none.set_density(0.0);
        none.rise(Vec3::ZERO, 1.0);
        assert!(none.take().is_empty());
    }
}
