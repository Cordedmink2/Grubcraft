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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recipes_make_particles_that_live_and_fade() {
        let mut fx = Effects::new(3);
        fx.leaves(Vec3::new(5.0, 1.0, 5.0), 1.0, [70, 110, 50], 20.0);
        fx.cut_chips(Vec3::new(5.0, 1.0, 5.0), Vec2::X, [200, 160, 100], 10.0);
        let made = fx.take();
        assert_eq!(made.len(), 30);
        for p in &made {
            assert!(p.life > 0.0 && p.size[0] > 0.0);
            assert_eq!(p.color_end[3], 0);
        }
    }
}
