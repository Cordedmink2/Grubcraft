//! Effect recipes: how dust, smoke, fire, sparks, debris and the factions'
//! wounds are made from [`Particle`]s. [`Effects`] is a list of particles
//! being collected: call a recipe wherever something happens, then hand the
//! list to [`crate::Renderer::spawn_particles`] once a frame
//! (`Effects::take`). Recipes are cheap to call and thin themselves with
//! [`Effects::set_density`], which the Graphics preset sets, so a caller
//! never has to ask whether there is room.
//!
//! Positions are tiles, `y` up, as in the renderer; soldiers stand about
//! 0.95 tiles tall. Randomness comes from a small generator of its own
//! ([`Rng`]), not the simulation's: effects are only a picture, so they
//! may differ between machines and must never change a match.

use glam::{Vec2, Vec3};

use crate::particles::{Particle, Shape, flags};

/// A small, fast pseudo-random generator for effects (xorshift32).
#[derive(Clone, Debug)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed.max(1))
    }

    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// 0 up to (not including) 1.
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1 << 24) as f32
    }

    pub fn between(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// A random direction on the sphere.
    pub fn direction(&mut self) -> Vec3 {
        let z = self.between(-1.0, 1.0);
        let a = self.between(0.0, std::f32::consts::TAU);
        let r = (1.0 - z * z).sqrt();
        Vec3::new(r * a.cos(), z, r * a.sin())
    }

    /// A random point in the unit disc.
    pub fn disc(&mut self) -> Vec2 {
        let a = self.between(0.0, std::f32::consts::TAU);
        let r = self.unit().sqrt();
        Vec2::new(r * a.cos(), r * a.sin())
    }

    fn byte(&mut self) -> u8 {
        (self.next() >> 24) as u8
    }
}

/// Whose wound it is, which decides what a blow throws up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitStyle {
    /// Blood: Ember Clans and Iron Concord soldiers.
    Flesh,
    /// Yellow-green ichor: the Brood.
    Ichor,
    /// Glittering crystal shards: the Choir.
    Crystal,
    /// Pale ghost-light: the Concord's summoned dead.
    Ghost,
    /// Glowing embers and ash.
    Ember,
}

/// A list of particles being collected, with a generator and a density.
#[derive(Clone, Debug)]
pub struct Effects {
    out: Vec<Particle>,
    rng: Rng,
    density: f32,
}

impl Default for Effects {
    fn default() -> Effects {
        Effects::new(0x5eed_f00d)
    }
}

/// Dust: warm tan, a little lighter than the ground it was kicked from.
pub const DUST: [u8; 3] = [196, 172, 134];
/// The Brood's ichor: yellow-green.
pub const ICHOR: [u8; 3] = [150, 190, 40];
/// The Concord's ghost-light: pale green-white.
pub const GHOST: [u8; 3] = [150, 235, 235];

impl Effects {
    pub fn new(seed: u32) -> Effects {
        Effects {
            out: Vec::new(),
            rng: Rng::new(seed),
            density: 1.0,
        }
    }

    /// How much of every effect to spawn: 1 for all of it, down to 0 for
    /// none. Counts are thinned at random, so half density halves a crowd of
    /// small puffs and still sometimes makes a single spark.
    pub fn set_density(&mut self, density: f32) {
        self.density = density.clamp(0.0, 1.0);
    }

    /// Hand over the particles collected, leaving the list empty.
    pub fn take(&mut self) -> Vec<Particle> {
        std::mem::take(&mut self.out)
    }

    /// How many particles are collected so far, to pass to
    /// [`Effects::age_since`].
    pub fn mark(&self) -> usize {
        self.out.len()
    }

    /// Make the particles collected since `mark` that much older, as if they
    /// had begun `age` seconds ago: to show an effect part-way through.
    pub fn age_since(&mut self, mark: usize, age: f32) {
        for p in &mut self.out[mark..] {
            p.age += age;
        }
    }

    /// True with probability `p`: for effects that happen at a rate (a
    /// foot every so often) rather than in a number. It does not thin by
    /// the density; the recipe it gates does.
    pub fn chance(&mut self, p: f32) -> bool {
        self.rng.unit() < p
    }

    /// A count of `n` thinned by the density.
    fn count(&mut self, n: f32) -> u32 {
        let scaled = n * self.density;
        scaled.floor() as u32 + u32::from(self.rng.unit() < scaled.fract())
    }

    fn puff(&mut self) -> Shape {
        Shape::PUFFS[(self.rng.next() % 4) as usize]
    }

    fn spin(&mut self, most: i8) -> i8 {
        let most = f32::from(most);
        self.rng.between(-most, most) as i8
    }

    fn jitter(&mut self, c: [u8; 3], by: f32) -> [u8; 3] {
        let k = 1.0 + self.rng.between(-by, by);
        c.map(|v| (f32::from(v) * k).clamp(0.0, 255.0) as u8)
    }

    // ------------------------------------------------------------ dust

    /// The dust a soldier's foot kicks up at `pos`, moving along `dir`
    /// (unit, on the ground plane) at `speed` tiles a second: a small puff
    /// that hangs behind him. Call it for some of the walking soldiers each
    /// frame; faster feet throw more.
    pub fn footstep(&mut self, pos: Vec3, dir: Vec2, speed: f32, color: [u8; 3]) {
        let hard = (speed / 3.0).clamp(0.6, 1.6);
        for _ in 0..self.count(1.0) {
            let behind = -dir * (speed * 0.12);
            let drift = self.rng.disc() * 0.12;
            let c = self.jitter(color, 0.12);
            let shape = self.puff();
            let (seed, spin) = (self.rng.byte(), self.spin(4));
            self.out.push(Particle {
                pos: pos + Vec3::new(0.0, 0.05, 0.0),
                vel: Vec3::new(
                    behind.x + drift.x,
                    self.rng.between(0.12, 0.3),
                    behind.y + drift.y,
                ),
                life: self.rng.between(1.4, 2.4),
                size: [0.14 * hard, self.rng.between(0.6, 0.9) * hard],
                drag: 1.6,
                lift: 0.07,
                shape,
                flags: flags::SOFT | flags::WIND,
                spin,
                seed,
                ..fading(c, (150.0 * hard).min(255.0) as u8)
            });
        }
    }

    /// A burst of dust at `pos` that rolls outward: a charge hitting home, a
    /// building falling, a heavy foot coming down. `radius` is how far it
    /// reaches in tiles, `amount` roughly how many puffs (before density).
    pub fn dust_burst(&mut self, pos: Vec3, radius: f32, amount: f32, color: [u8; 3]) {
        self.dust_cloud(pos, radius, amount, color);
        // A thin shock ring on the ground for the first moment.
        if self.density > 0.0 && amount >= 3.0 {
            let pale = brighten(color, 1.15);
            self.out.push(Particle {
                pos: pos + Vec3::new(0.0, 0.08, 0.0),
                life: 0.55,
                size: [radius * 0.25, radius * 1.3],
                color: [pale[0], pale[1], pale[2], 120],
                color_end: [color[0], color[1], color[2], 0],
                shape: Shape::Ring,
                flags: flags::SOFT,
                seed: self.rng.byte(),
                ..Particle::default()
            });
        }
    }

    /// Puffs of dust rolling outward from `pos` like [`Effects::dust_burst`],
    /// without its ring on the ground: for dust off a wall, in the air.
    pub fn dust_cloud(&mut self, pos: Vec3, radius: f32, amount: f32, color: [u8; 3]) {
        for _ in 0..self.count(amount) {
            let ring = self.rng.disc();
            let out = ring.normalize_or_zero() * self.rng.between(0.3, 1.0);
            let c = self.jitter(color, 0.15);
            let shape = self.puff();
            let (seed, spin) = (self.rng.byte(), self.spin(5));
            self.out.push(Particle {
                pos: pos + Vec3::new(ring.x, 0.1, ring.y) * (radius * 0.3),
                vel: Vec3::new(out.x, self.rng.between(0.15, 0.6), out.y) * (radius * 1.1),
                life: self.rng.between(1.6, 3.0),
                size: [radius * 0.2, radius * self.rng.between(0.6, 0.95)],
                drag: 2.2,
                lift: 0.12,
                shape,
                flags: flags::SOFT | flags::WIND,
                spin,
                seed,
                ..fading(c, 200)
            });
        }
    }

    // ------------------------------------------------------------ smoke and fire

    /// Smoke rising from `pos` for `dt` seconds of burning: call it every
    /// frame (`dt` the frame time). `size` is the fire's radius in tiles
    /// (a house fire is about 1) and `dark` how black it is, 0 to 1.
    pub fn smoke(&mut self, pos: Vec3, size: f32, dark: f32, dt: f32) {
        for _ in 0..self.count(14.0 * size * dt) {
            let at = self.rng.disc() * (size * 0.5);
            let shade = (self.rng.between(0.8, 1.15) * mix(205.0, 95.0, dark)).min(255.0) as u8;
            let shape = self.puff();
            let (seed, spin) = (self.rng.byte(), self.spin(3));
            self.out.push(Particle {
                pos: pos + Vec3::new(at.x, 0.0, at.y),
                vel: Vec3::new(0.0, self.rng.between(0.7, 1.4), 0.0),
                life: self.rng.between(3.5, 6.0),
                size: [size * 0.3, size * self.rng.between(1.1, 1.9)],
                color: [shade, shade.saturating_sub(2), shade.saturating_sub(5), 170],
                color_end: [shade, shade, shade, 0],
                drag: 0.7,
                lift: 0.35,
                shape,
                flags: flags::WIND,
                spin,
                seed,
                ..Particle::default()
            });
        }
    }

    /// Flames licking up from `pos` for `dt` seconds. Pair it with
    /// [`Effects::smoke`] and a few [`Effects::embers`].
    pub fn flames(&mut self, pos: Vec3, size: f32, dt: f32) {
        for _ in 0..self.count(40.0 * size * dt) {
            let at = self.rng.disc() * (size * 0.6);
            let shape = if self.rng.unit() < 0.5 {
                Shape::Glow
            } else {
                self.puff()
            };
            let (seed, spin) = (self.rng.byte(), self.spin(6));
            self.out.push(Particle {
                pos: pos + Vec3::new(at.x, 0.05, at.y),
                vel: Vec3::new(0.0, self.rng.between(0.5, 1.2), 0.0),
                life: self.rng.between(0.5, 1.0),
                size: [size * self.rng.between(0.3, 0.45), size * 0.08],
                color: [255, 176, 62, 190],
                color_end: [255, 48, 8, 0],
                drag: 1.0,
                lift: 1.6,
                shape,
                flags: flags::ADDITIVE | flags::FLICKER,
                spin,
                seed,
                ..Particle::default()
            });
        }
    }

    /// Sparks of ash and ember that float up and away from a fire at `pos`,
    /// glowing orange and going out.
    pub fn embers(&mut self, pos: Vec3, spread: f32, amount: f32) {
        for _ in 0..self.count(amount) {
            let at = self.rng.disc() * spread;
            let up = self.rng.between(0.6, 1.8);
            self.out.push(Particle {
                pos: pos + Vec3::new(at.x, 0.1, at.y),
                vel: Vec3::new(self.rng.between(-0.4, 0.4), up, self.rng.between(-0.4, 0.4)),
                life: self.rng.between(1.8, 4.0),
                size: [0.035, 0.015],
                color: [255, 150, 50, 255],
                color_end: [200, 40, 10, 0],
                drag: 0.5,
                lift: 0.5,
                shape: Shape::Glow,
                flags: flags::ADDITIVE | flags::FLICKER | flags::WIND,
                seed: self.rng.byte(),
                ..Particle::default()
            });
        }
    }

    // ------------------------------------------------------------ blows

    /// Sparks flying from steel on steel at `pos`, mostly along `dir` (unit,
    /// any plane).
    pub fn sparks(&mut self, pos: Vec3, dir: Vec3, amount: f32) {
        for _ in 0..self.count(amount) {
            let v = (dir.normalize_or_zero() * 0.6 + self.rng.direction()).normalize_or_zero()
                * self.rng.between(2.5, 6.5);
            self.out.push(Particle {
                pos,
                vel: v,
                life: self.rng.between(0.25, 0.55),
                size: [0.05, 0.02],
                color: [255, 238, 170, 255],
                color_end: [255, 110, 30, 0],
                gravity: 9.0,
                drag: 0.9,
                shape: Shape::Streak,
                flags: flags::STRETCH | flags::ADDITIVE,
                seed: self.rng.byte(),
                ..Particle::default()
            });
        }
    }

    /// Chips and clods thrown up at `pos`: stone, earth, splinters. They
    /// fall, skid and come to rest on the ground, then fade.
    pub fn debris(&mut self, pos: Vec3, color: [u8; 3], amount: f32, power: f32) {
        for _ in 0..self.count(amount) {
            let flat = self.rng.disc() * self.rng.between(0.5, 1.0);
            let c = self.jitter(color, 0.3);
            let s = self.rng.between(0.04, 0.09);
            let spin = self.spin(60);
            self.out.push(Particle {
                pos,
                vel: Vec3::new(flat.x, self.rng.between(0.8, 2.0), flat.y) * (power * 2.0),
                life: self.rng.between(1.4, 2.6),
                size: [s, s],
                gravity: 11.0,
                drag: 0.35,
                shape: Shape::Shard,
                flags: flags::GROUND,
                spin,
                seed: self.rng.byte(),
                ..fading(c, 255)
            });
        }
    }

    /// Chips knocked off a wall at `pos` and flung back along `back` (unit,
    /// on the ground plane, toward whoever struck it): stone, splinters,
    /// bone. They fall, skid and come to rest, then fade. `size` is the
    /// biggest chip's width in tiles.
    pub fn chips(&mut self, pos: Vec3, back: Vec2, color: [u8; 3], amount: f32, size: f32) {
        for _ in 0..self.count(amount) {
            let aside = Vec2::new(-back.y, back.x) * self.rng.between(-1.4, 1.4);
            let out = back * self.rng.between(0.6, 2.6) + aside;
            let c = self.jitter(color, 0.25);
            let s = size * self.rng.between(0.4, 1.0);
            let spin = self.spin(60);
            self.out.push(Particle {
                pos,
                vel: Vec3::new(out.x, self.rng.between(1.2, 3.4), out.y),
                life: self.rng.between(1.8, 3.4),
                size: [s, s * 0.8],
                gravity: 11.0,
                drag: 0.3,
                shape: Shape::Shard,
                flags: flags::GROUND,
                spin,
                seed: self.rng.byte(),
                ..fading(c, 255)
            });
        }
    }

    /// The smoke and glow a fire-pot leaves behind it in flight, for `dt`
    /// seconds: call it every frame with the pot's position.
    pub fn trail(&mut self, pos: Vec3, dt: f32) {
        for _ in 0..self.count(34.0 * dt) {
            let at = self.rng.disc() * 0.06;
            let shade = self.rng.between(70.0, 120.0) as u8;
            let shape = self.puff();
            let (seed, spin) = (self.rng.byte(), self.spin(3));
            self.out.push(Particle {
                pos: pos + Vec3::new(at.x, 0.0, at.y),
                vel: Vec3::new(0.0, self.rng.between(0.1, 0.4), 0.0),
                life: self.rng.between(0.9, 1.6),
                size: [0.07, self.rng.between(0.22, 0.34)],
                color: [shade, shade, shade, 150],
                color_end: [shade, shade, shade, 0],
                drag: 1.2,
                lift: 0.15,
                shape,
                flags: flags::WIND,
                spin,
                seed,
                ..Particle::default()
            });
        }
        for _ in 0..self.count(16.0 * dt) {
            self.out.push(Particle {
                pos,
                vel: Vec3::new(0.0, self.rng.between(0.0, 0.3), 0.0),
                life: self.rng.between(0.2, 0.4),
                size: [0.12, 0.04],
                color: [255, 170, 60, 220],
                color_end: [255, 60, 10, 0],
                shape: Shape::Glow,
                flags: flags::ADDITIVE | flags::FLICKER,
                seed: self.rng.byte(),
                ..Particle::default()
            });
        }
    }

    /// Drops thrown from a wound at `pos` along `dir`.
    pub fn spray(&mut self, pos: Vec3, dir: Vec3, color: [u8; 3], amount: f32) {
        for _ in 0..self.count(amount) {
            let v = (dir.normalize_or_zero() + self.rng.direction() * 0.6).normalize_or_zero()
                * self.rng.between(1.2, 3.4);
            self.out.push(Particle {
                pos,
                vel: v,
                life: self.rng.between(0.35, 0.7),
                size: [0.05, 0.03],
                gravity: 9.0,
                drag: 0.6,
                shape: Shape::Streak,
                flags: flags::STRETCH,
                seed: self.rng.byte(),
                ..fading(color, 255)
            });
        }
    }

    /// Glittering crystal shards from a Choir soldier hit at `pos`.
    pub fn shards(&mut self, pos: Vec3, dir: Vec3, amount: f32) {
        for _ in 0..self.count(amount) {
            let v = (dir.normalize_or_zero() * 0.7 + self.rng.direction()).normalize_or_zero()
                * self.rng.between(1.0, 3.2);
            let tint = self.rng.between(0.0, 1.0);
            let s = self.rng.between(0.03, 0.06);
            let spin = self.spin(70);
            self.out.push(Particle {
                pos,
                vel: v + Vec3::Y * 0.8,
                life: self.rng.between(0.7, 1.4),
                size: [s, s * 0.6],
                color: [
                    mix(150.0, 235.0, tint) as u8,
                    mix(215.0, 190.0, tint) as u8,
                    255,
                    255,
                ],
                color_end: [120, 160, 255, 0],
                gravity: 6.0,
                drag: 0.8,
                shape: Shape::Shard,
                flags: flags::ADDITIVE | flags::FLICKER,
                spin,
                seed: self.rng.byte(),
                ..Particle::default()
            });
        }
    }

    /// Floating points of light: pale ghost-light round the dead, fireflies,
    /// pollen. They rise slowly, wander on the wind and flicker.
    pub fn motes(&mut self, centre: Vec3, spread: f32, color: [u8; 3], amount: f32) {
        for _ in 0..self.count(amount) {
            let at = self.rng.disc() * spread;
            self.out.push(Particle {
                pos: centre + Vec3::new(at.x, self.rng.between(0.0, 0.6), at.y),
                vel: Vec3::new(
                    self.rng.between(-0.15, 0.15),
                    self.rng.between(0.1, 0.4),
                    self.rng.between(-0.15, 0.15),
                ),
                life: self.rng.between(2.0, 3.8),
                size: [0.05, 0.03],
                drag: 0.6,
                lift: 0.05,
                shape: Shape::Glow,
                flags: flags::ADDITIVE | flags::FLICKER | flags::WIND,
                seed: self.rng.byte(),
                ..fading(color, 200)
            });
        }
    }

    /// Specks of pollen and dust hanging in the light over a disc of
    /// `radius` round `centre` (a place on the map, with `ground` giving the
    /// height of the land), `amount` of them: warm, tiny, drifting up
    /// and with the wind for several seconds, each flickering as it turns
    /// to the sun. They make the air visible when the camera is low.
    pub fn light_motes(
        &mut self,
        centre: Vec2,
        radius: f32,
        amount: f32,
        ground: impl Fn(Vec2) -> f32,
    ) {
        for _ in 0..self.count(amount) {
            let at = centre + self.rng.disc() * radius;
            self.out.push(Particle {
                pos: Vec3::new(at.x, ground(at) + self.rng.between(0.1, 1.8), at.y),
                vel: Vec3::new(
                    self.rng.between(-0.08, 0.08),
                    self.rng.between(0.02, 0.12),
                    self.rng.between(-0.08, 0.08),
                ),
                life: self.rng.between(5.0, 8.0),
                size: [0.09, 0.06],
                drag: 0.8,
                shape: Shape::Glow,
                flags: flags::ADDITIVE | flags::FLICKER | flags::WIND,
                seed: self.rng.byte(),
                ..fading([255, 228, 170], 230)
            });
        }
    }

    /// What a blow throws up at `pos`, flying along `dir`: sparks when it
    /// rang on armour (`armoured`), then the faction's wound. `power` is 1
    /// for an ordinary blow, more for a crushing one.
    pub fn hit(&mut self, pos: Vec3, dir: Vec3, style: HitStyle, armoured: bool, power: f32) {
        if armoured {
            self.sparks(pos, dir, 6.0 * power);
        }
        match style {
            HitStyle::Flesh => self.spray(pos, dir, [150, 20, 18], 7.0 * power),
            HitStyle::Ichor => {
                self.spray(pos, dir, ICHOR, 8.0 * power);
                self.motes(pos, 0.15, [190, 230, 80], 2.0 * power);
            }
            HitStyle::Crystal => self.shards(pos, dir, 8.0 * power),
            HitStyle::Ghost => self.motes(pos, 0.2, GHOST, 6.0 * power),
            HitStyle::Ember => {
                self.embers(pos, 0.15, 8.0 * power);
                self.sparks(pos, dir, 3.0 * power);
            }
        }
    }
}

/// A particle that fades to nothing from `rgb` at opacity `alpha`, to build
/// the rest of it on.
fn fading(rgb: [u8; 3], alpha: u8) -> Particle {
    Particle {
        color: [rgb[0], rgb[1], rgb[2], alpha],
        color_end: [rgb[0], rgb[1], rgb[2], 0],
        ..Particle::default()
    }
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn brighten(c: [u8; 3], k: f32) -> [u8; 3] {
    c.map(|v| (f32::from(v) * k).min(255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_recipes(fx: &mut Effects) {
        let p = Vec3::new(10.0, 2.0, 10.0);
        fx.footstep(p, Vec2::X, 3.0, DUST);
        fx.dust_burst(p, 2.0, 12.0, DUST);
        fx.dust_cloud(p, 1.0, 6.0, DUST);
        fx.smoke(p, 1.0, 0.8, 0.5);
        fx.flames(p, 1.0, 0.5);
        fx.embers(p, 0.5, 10.0);
        fx.sparks(p, Vec3::X, 10.0);
        fx.debris(p, [120, 110, 100], 10.0, 1.0);
        fx.chips(p, Vec2::X, [120, 110, 100], 10.0, 0.1);
        fx.trail(p, 0.5);
        fx.spray(p, Vec3::X, [150, 20, 20], 10.0);
        fx.shards(p, Vec3::X, 10.0);
        fx.motes(p, 1.0, [150, 230, 230], 10.0);
        fx.light_motes(Vec2::new(10.0, 10.0), 5.0, 10.0, |_| 2.0);
        for style in [
            HitStyle::Flesh,
            HitStyle::Ichor,
            HitStyle::Crystal,
            HitStyle::Ghost,
            HitStyle::Ember,
        ] {
            fx.hit(p, Vec3::X, style, true, 1.0);
        }
    }

    #[test]
    fn every_recipe_makes_sane_particles() {
        let mut fx = Effects::new(7);
        all_recipes(&mut fx);
        let particles = fx.take();
        assert!(particles.len() > 100);
        assert!(fx.take().is_empty());
        for p in &particles {
            assert!(p.pos.is_finite() && p.vel.is_finite(), "{p:?}");
            assert!(p.life > 0.05 && p.life < 10.0, "{p:?}");
            assert!(p.size[0] > 0.0 && p.size[1] > 0.0, "{p:?}");
            assert!(p.drag >= 0.0 && p.gravity >= 0.0, "{p:?}");
            // Everything fades out by its end.
            assert_eq!(p.color_end[3], 0, "{p:?}");
        }
    }

    #[test]
    fn density_thins_and_zero_makes_nothing() {
        let count = |density: f32| {
            let mut fx = Effects::new(3);
            fx.set_density(density);
            for _ in 0..200 {
                fx.sparks(Vec3::ZERO, Vec3::X, 4.0);
            }
            fx.take().len()
        };
        assert_eq!(count(0.0), 0);
        let (full, half) = (count(1.0), count(0.5));
        assert_eq!(full, 800);
        assert!((350..450).contains(&half), "half density made {half}");
    }

    #[test]
    fn the_same_seed_makes_the_same_particles() {
        let run = || {
            let mut fx = Effects::new(99);
            all_recipes(&mut fx);
            fx.take()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn the_generator_covers_its_range() {
        let mut rng = Rng::new(1);
        let samples: Vec<f32> = (0..1000).map(|_| rng.unit()).collect();
        assert!(samples.iter().all(|v| (0.0..1.0).contains(v)));
        let mean = samples.iter().sum::<f32>() / 1000.0;
        assert!((0.45..0.55).contains(&mean), "mean {mean}");
        assert!((rng.direction().length() - 1.0).abs() < 1e-4);
        assert!(rng.disc().length() <= 1.0);
    }
}
