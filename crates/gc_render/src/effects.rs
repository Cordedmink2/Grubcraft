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

mod ambient;

use crate::look::Weather;
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
    /// Places that gave light this frame (lamps, flames), for the game to
    /// turn into point lights at night.
    glows: Vec<Glow>,
    rng: Rng,
    density: f32,
}

/// A place that shines: a lit window, a fire. Recorded by [`Effects::lamp`]
/// and [`Effects::flames`] whenever they are called (a frame at a time), so
/// the game can keep a light up for as long as the thing burns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    pub pos: Vec3,
    /// sRGB, as the particles' colour.
    pub color: [u8; 3],
    /// How far its light reaches, in tiles.
    pub radius: f32,
    /// How bright it is, 0 to about 1.5.
    pub strength: f32,
    /// Whether it should be scaled by how dark it is: a fire is, a lit
    /// window already is (it is only reported at dusk).
    pub needs_dark: bool,
}

/// The most glows kept between two [`Effects::take_glows`], so a caller that
/// never takes them does not grow a list for ever.
const MAX_GLOWS: usize = 256;

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
            glows: Vec::new(),
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

    /// Hand over the glows recorded since the last call.
    pub fn take_glows(&mut self) -> Vec<Glow> {
        std::mem::take(&mut self.glows)
    }

    /// Record a glow. One within half a tile of one already recorded this
    /// frame is the same light (a squad chopping at a tree reports the same
    /// flame once each): the stronger one stays, so a crowd cannot fill the
    /// list and crowd out the lamps.
    fn glow(&mut self, glow: Glow) {
        if let Some(same) = self
            .glows
            .iter_mut()
            .find(|g| g.pos.distance_squared(glow.pos) < 0.25)
        {
            if glow.strength > same.strength {
                *same = glow;
            }
        } else if self.glows.len() < MAX_GLOWS {
            self.glows.push(glow);
        }
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
        self.glow(Glow {
            pos: pos + Vec3::Y * (0.3 + size * 0.5),
            color: [255, 150, 60],
            radius: 2.5 + size * 4.0,
            strength: (size * 2.0).min(1.5),
            needs_dark: true,
        });
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

    /// Rain or snow falling over a disc of `radius` round `centre` (a place
    /// on the map, `ground` giving the land's height), `amount` of drops or
    /// flakes. Each begins up to `FALL_HEIGHT` tiles over the ground and is
    /// gone before it reaches it (a drop is a streak along its fall, a flake
    /// a soft speck that sways on the wind). Clear weather and fog drop
    /// nothing.
    pub fn precipitation(
        &mut self,
        weather: Weather,
        centre: Vec2,
        radius: f32,
        amount: f32,
        wind: Vec2,
        ground: impl Fn(Vec2) -> f32,
    ) {
        // A gale has no more business here than a breeze: a drop's lean stays
        // within what a replay's wind byte can say sensibly.
        let wind = wind.clamp_length_max(MAX_WIND);
        for _ in 0..self.count(amount) {
            // Where the drop lands; it starts upwind so the slant carries it
            // there and the disc stays evenly covered.
            let at = centre + self.rng.disc() * radius;
            let low = ground(at);
            match weather {
                Weather::Rain => {
                    // Slanted by the wind, and falling fast.
                    let fall = self.rng.between(10.0, 13.0);
                    let top = self.rng.between(0.6, 1.0) * FALL_HEIGHT;
                    let life = (top - 0.1) / fall;
                    let drift = wind * RAIN_DRIFT;
                    let from = at - drift * life;
                    self.out.push(Particle {
                        pos: Vec3::new(from.x, low + top, from.y),
                        vel: Vec3::new(drift.x, -fall, drift.y),
                        life,
                        size: [0.03, 0.03],
                        shape: Shape::Streak,
                        flags: flags::STRETCH,
                        seed: self.rng.byte(),
                        ..fading([196, 212, 232], 150)
                    });
                }
                Weather::Snow => {
                    let fall = self.rng.between(1.1, 1.9);
                    // Gone before the ring's longest life (8 s) cuts it short.
                    let top = (self.rng.between(0.4, 1.0) * FALL_HEIGHT).min(fall * 7.4);
                    let life = (top - 0.1) / fall;
                    let drift = wind * SNOW_DRIFT
                        + Vec2::new(self.rng.between(-0.3, 0.3), self.rng.between(-0.3, 0.3));
                    let from = at - drift * life;
                    self.out.push(Particle {
                        pos: Vec3::new(from.x, low + top, from.y),
                        vel: Vec3::new(drift.x, -fall, drift.y),
                        life,
                        size: [0.075, 0.06],
                        spin: self.rng.byte() as i8,
                        shape: Shape::Glow,
                        seed: self.rng.byte(),
                        ..fading([240, 244, 250], 215)
                    });
                }
                Weather::Clear | Weather::Fog => return,
            }
        }
    }

    /// Rain striking the ground: `amount` of small rings spreading over
    /// the land of a disc of `radius` round `centre`, each gone in a moment.
    pub fn splashes(
        &mut self,
        centre: Vec2,
        radius: f32,
        amount: f32,
        ground: impl Fn(Vec2) -> f32,
    ) {
        for _ in 0..self.count(amount) {
            let at = centre + self.rng.disc() * radius;
            let reach = self.rng.between(0.07, 0.12);
            self.out.push(Particle {
                pos: Vec3::new(at.x, ground(at) + 0.03, at.y),
                life: self.rng.between(0.2, 0.32),
                size: [0.012, reach],
                color: [214, 226, 240, 150],
                color_end: [214, 226, 240, 0],
                shape: Shape::Ring,
                seed: self.rng.byte(),
                ..Particle::default()
            });
        }
    }

    /// A bolt of lightning at a random place of the disc of `radius` round
    /// `centre` that `fits` (somewhere worth striking: on the map, in view),
    /// `top` tiles tall (see [`Effects::bolt`]). A dozen tries are made;
    /// the answer is whether one fit and the bolt was drawn.
    pub fn lightning(
        &mut self,
        centre: Vec2,
        radius: f32,
        top: f32,
        width: f32,
        ground: impl Fn(Vec2) -> f32,
        fits: impl Fn(Vec2) -> bool,
    ) -> bool {
        for _ in 0..12 {
            let at = centre + self.rng.disc() * radius;
            if fits(at) {
                self.bolt(Vec3::new(at.x, ground(at), at.y), top, width);
                return true;
            }
        }
        false
    }

    /// A bolt of lightning from the cloud over `ground` (a point on the
    /// land, `top` tiles above it) down to it: a jagged line of glowing
    /// points that pulls toward the strike and flares and fades in a
    /// third of a second, one branch off its middle, and a flash where it
    /// lands. `width` is the line's radius in tiles (a bolt seen from far
    /// up has to be wider to be seen).
    pub fn bolt(&mut self, ground: Vec3, top: f32, width: f32) {
        // Draws a path and gives back the point halfway along it.
        let path = |this: &mut Effects, from: Vec3, to: Vec3, steps: u32, wander: f32| -> Vec3 {
            let mut at = from;
            let mut halfway = from;
            for i in 0..steps {
                if i == steps / 2 {
                    halfway = at;
                }
                let left = (steps - i) as f32;
                // A step toward the end, and a sideways stumble.
                let mut next = at + (to - at) / left;
                if i + 1 < steps {
                    next.x += this.rng.between(-wander, wander);
                    next.z += this.rng.between(-wander, wander);
                }
                // Points along the step so the line has no gaps.
                for k in 0..BOLT_POINTS {
                    let pos = at.lerp(next, k as f32 / BOLT_POINTS as f32);
                    this.out.push(Particle {
                        pos,
                        life: this.rng.between(0.28, 0.4),
                        size: [width * 1.8, width * 1.2],
                        color: [214, 226, 255, 255],
                        color_end: [150, 175, 255, 0],
                        shape: Shape::Glow,
                        flags: flags::ADDITIVE,
                        seed: this.rng.byte(),
                        ..Particle::default()
                    });
                }
                at = next;
            }
            halfway
        };
        let high = ground
            + Vec3::new(
                self.rng.between(-4.0, 4.0),
                top,
                self.rng.between(-4.0, 4.0),
            );
        // A branch off the middle, reaching down and out.
        let fork = path(self, high, ground, 22, top * 0.03);
        let end = fork
            + Vec3::new(
                self.rng.between(-1.0, 1.0) * top * 0.25,
                -top * 0.3,
                self.rng.between(-1.0, 1.0) * top * 0.25,
            );
        path(self, fork, end, 8, top * 0.03);
        // The flash where it lands.
        self.out.push(Particle {
            pos: ground + Vec3::Y * 0.3,
            life: 0.45,
            size: [width * 6.0, width * 14.0],
            color: [200, 215, 255, 200],
            color_end: [150, 175, 255, 0],
            shape: Shape::Glow,
            flags: flags::ADDITIVE | flags::SOFT,
            seed: self.rng.byte(),
            ..Particle::default()
        });
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

/// Glowing points to a step of a lightning bolt's path.
const BOLT_POINTS: u32 = 8;

/// How far rain and snow lean with the wind: tiles a second of sideways
/// travel for each tile a second of wind (a rain drop is fast, so the wind
/// bends its path more than it moves it).
const RAIN_DRIFT: f32 = 2.5;
const SNOW_DRIFT: f32 = 1.5;
/// The strongest wind (tiles a second) rain and snow lean to; the
/// simulation's own winds stay inside it.
const MAX_WIND: f32 = 2.0;

/// How high above the ground rain and snow begin to fall, in tiles.
pub const FALL_HEIGHT: f32 = 14.0;

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

    fn fall(weather: Weather, amount: f32) -> Vec<Particle> {
        let mut fx = Effects::default();
        fx.precipitation(
            weather,
            Vec2::new(50.0, 50.0),
            10.0,
            amount,
            Vec2::ZERO,
            |_| 3.0,
        );
        fx.take()
    }

    #[test]
    fn rain_and_snow_fall_and_clear_air_and_fog_do_not() {
        assert!(fall(Weather::Rain, 200.0).len() >= 190);
        assert!(fall(Weather::Snow, 200.0).len() >= 190);
        assert!(fall(Weather::Clear, 200.0).is_empty());
        assert!(fall(Weather::Fog, 200.0).is_empty());
    }

    #[test]
    fn drops_and_flakes_are_gone_before_they_reach_the_ground() {
        for weather in [Weather::Rain, Weather::Snow] {
            for p in fall(weather, 300.0) {
                assert!(p.vel.y < 0.0);
                let end = p.pos.y + p.vel.y * p.life;
                assert!(end >= 3.0, "{weather:?}: ends at {end}");
                assert!(p.pos.y <= 3.0 + FALL_HEIGHT);
                assert!(p.life > 0.0 && p.life <= 8.0, "{weather:?}: {}", p.life);
                // It lands inside the disc it was asked for.
                let land = Vec2::new(p.pos.x + p.vel.x * p.life, p.pos.z + p.vel.z * p.life);
                assert!(land.distance(Vec2::new(50.0, 50.0)) <= 10.01);
            }
        }
    }

    #[test]
    fn splashes_are_short_rings_on_the_ground_inside_the_disc() {
        let mut fx = Effects::default();
        fx.splashes(Vec2::new(50.0, 50.0), 8.0, 200.0, |_| 3.0);
        let splashes = fx.take();
        assert!(splashes.len() >= 190);
        for p in splashes {
            assert_eq!(p.shape, Shape::Ring);
            assert!(p.life < 0.4 && p.pos.y > 3.0 && p.pos.y < 3.1);
            assert!(p.size[1] > p.size[0], "a splash spreads");
            assert!(Vec2::new(p.pos.x, p.pos.z).distance(Vec2::new(50.0, 50.0)) <= 8.01);
        }
    }

    #[test]
    fn the_wind_leans_rain_and_snow_its_way() {
        let fall = |weather, wind| {
            let mut fx = Effects::default();
            fx.precipitation(weather, Vec2::ZERO, 10.0, 200.0, wind, |_| 0.0);
            let drops = fx.take();
            assert!(drops.len() >= 190, "{weather:?}");
            drops
        };
        let lean = |drops: &[Particle], along: fn(&Particle) -> f32| {
            drops.iter().map(|p| along(p) / -p.vel.y).sum::<f32>() / drops.len() as f32
        };
        let x: fn(&Particle) -> f32 = |p| p.vel.x;
        let z: fn(&Particle) -> f32 = |p| p.vel.z;
        for weather in [Weather::Rain, Weather::Snow] {
            let calm = fall(weather, Vec2::ZERO);
            let east = fall(weather, Vec2::new(1.0, 0.0));
            let west = fall(weather, Vec2::new(-1.0, 0.0));
            let south = fall(weather, Vec2::new(0.0, 1.0));
            assert!(lean(&east, x) > lean(&calm, x) + 0.05, "{weather:?}");
            assert!(lean(&west, x) < lean(&calm, x) - 0.05, "{weather:?}");
            assert!(lean(&south, z) > lean(&calm, z) + 0.05, "{weather:?}");
            assert!(lean(&south, x).abs() < 0.1, "{weather:?}");
            // Snow is no longer pushed by anything but the simulation's wind.
            assert!(calm.iter().all(|p| p.flags & flags::WIND == 0));
        }
        // A lobby's rain wind (10, 4 sixteenths) is the slant it always had.
        let mut fx = Effects::default();
        fx.precipitation(
            Weather::Rain,
            Vec2::ZERO,
            10.0,
            50.0,
            Vec2::new(0.625, 0.25),
            |_| 0.0,
        );
        let drops = fx.take();
        assert!(!drops.is_empty());
        assert!(drops.iter().all(|p| (p.vel.x - 1.5625).abs() < 1e-3));
    }

    #[test]
    fn a_slanted_fall_still_lands_inside_the_disc() {
        for weather in [Weather::Rain, Weather::Snow] {
            let mut fx = Effects::default();
            fx.precipitation(
                weather,
                Vec2::new(20.0, 20.0),
                10.0,
                300.0,
                Vec2::new(1.5, -1.0),
                |_| 0.0,
            );
            let drops = fx.take();
            assert!(drops.len() >= 280);
            for p in drops {
                // Where it would land, if it were left to fall.
                let t = p.pos.y / -p.vel.y;
                let land = Vec2::new(p.pos.x + p.vel.x * t, p.pos.z + p.vel.z * t);
                assert!(land.distance(Vec2::new(20.0, 20.0)) <= 10.5, "{weather:?}");
            }
        }
    }

    #[test]
    fn a_gale_leans_no_further_than_the_cap() {
        let mut fx = Effects::default();
        fx.precipitation(
            Weather::Rain,
            Vec2::ZERO,
            10.0,
            100.0,
            Vec2::new(8.0, 0.0),
            |_| 0.0,
        );
        let drops = fx.take();
        assert!(!drops.is_empty());
        assert!(
            drops
                .iter()
                .all(|p| p.vel.x <= MAX_WIND * RAIN_DRIFT + 1e-3)
        );
    }

    #[test]
    fn a_bolt_runs_from_the_cloud_to_the_ground_and_flashes_where_it_lands() {
        let mut fx = Effects::default();
        fx.bolt(Vec3::new(30.0, 2.0, 40.0), 40.0, 0.2);
        let parts = fx.take();
        assert!(parts.len() > 200 && parts.len() < 300, "{}", parts.len());
        assert!(
            parts
                .iter()
                .all(|p| p.life <= 0.5 && p.flags & flags::ADDITIVE != 0)
        );
        let top = parts.iter().map(|p| p.pos.y).fold(0.0, f32::max);
        assert!(top > 38.0, "{top}");
        // The line comes down into the strike.
        assert!(parts.iter().filter(|p| p.pos.y < 3.0).count() >= 2);
        let flash = parts.last().unwrap();
        assert!(flash.size[1] > 1.5 && (flash.pos.x - 30.0).abs() < 1e-3);
        // Nothing underground.
        assert!(parts.iter().all(|p| p.pos.y > 1.9));
        // The branch leaves the main line rather than hanging in the air:
        // every point has another within a step of it.
        let step = 40.0 / 22.0 * 1.5;
        for (i, p) in parts[..parts.len() - 1].iter().enumerate() {
            let near = parts
                .iter()
                .enumerate()
                .any(|(j, q)| i != j && q.pos.distance(p.pos) < step / BOLT_POINTS as f32 * 2.5);
            assert!(near, "point {i} floats alone");
        }
    }

    #[test]
    fn the_bolt_the_disc_picks_stands_on_the_land_it_strikes() {
        let mut fx = Effects::default();
        let slope = |p: Vec2| p.x * 0.5;
        for _ in 0..20 {
            assert!(fx.lightning(Vec2::new(40.0, 40.0), 20.0, 30.0, 0.1, slope, |_| true));
        }
        let parts = fx.take();
        // Each flash (the last particle of a bolt) sits on its own ground.
        for flash in parts.chunks(parts.len() / 20).map(|c| c.last().unwrap()) {
            assert!((flash.pos.y - 0.3 - slope(Vec2::new(flash.pos.x, flash.pos.z))).abs() < 1e-3);
        }
    }

    #[test]
    fn a_strike_only_lands_where_it_fits_and_is_skipped_when_nowhere_does() {
        let mut fx = Effects::default();
        let east = |p: Vec2| p.x > 45.0;
        for _ in 0..20 {
            assert!(fx.lightning(Vec2::new(40.0, 40.0), 20.0, 30.0, 0.1, |_| 0.0, east));
        }
        for p in fx.take().iter().filter(|p| p.size[1] > 1.0) {
            assert!(p.pos.x > 45.0, "{}", p.pos.x);
        }
        assert!(!fx.lightning(Vec2::new(40.0, 40.0), 20.0, 30.0, 0.1, |_| 0.0, |_| false));
        assert!(fx.take().is_empty());
    }

    #[test]
    fn a_thin_graphics_density_thins_the_fall() {
        let mut fx = Effects::default();
        fx.set_density(0.25);
        fx.precipitation(Weather::Rain, Vec2::ZERO, 10.0, 400.0, Vec2::ZERO, |_| 0.0);
        let n = fx.take().len();
        assert!((60..=140).contains(&n), "{n}");
    }

    #[test]
    fn lamps_and_flames_record_a_glow_each_time_and_it_is_handed_over_once() {
        let mut fx = Effects::default();
        fx.lamp(Vec3::new(5.0, 1.0, 5.0), [255, 200, 120], 0.13, 0.5, 0.016);
        fx.flames(Vec3::new(8.0, 1.0, 8.0), 0.5, 0.016);
        let glows = fx.take_glows();
        assert_eq!(glows.len(), 2);
        assert_eq!(glows[0].strength, 0.5);
        assert!(!glows[0].needs_dark && glows[1].needs_dark);
        assert!(glows[1].pos.y > 1.0, "a fire's light is above its base");
        assert!(glows[1].radius > glows[0].radius);
        assert!(fx.take_glows().is_empty());
    }

    #[test]
    fn a_caller_that_never_takes_glows_does_not_grow_the_list() {
        let mut fx = Effects::default();
        for i in 0..10_000 {
            fx.flames(Vec3::new(i as f32, 0.0, 0.0), 0.4, 0.016);
        }
        assert!(fx.take_glows().len() <= 256);
    }

    #[test]
    fn a_crowd_at_one_flame_is_one_glow_and_cannot_crowd_out_a_lamp() {
        let mut fx = Effects::default();
        for _ in 0..300 {
            fx.flames(Vec3::new(5.0, 0.0, 5.0), 0.2, 0.016);
        }
        fx.flames(Vec3::new(5.1, 0.0, 5.0), 0.5, 0.016);
        fx.lamp(
            Vec3::new(20.0, 1.0, 20.0),
            [255, 200, 120],
            0.13,
            1.0,
            0.016,
        );
        let glows = fx.take_glows();
        assert_eq!(glows.len(), 2);
        assert_eq!(glows[0].strength, 1.0, "the stronger flame stayed");
        assert!(!glows[1].needs_dark);
    }
}
