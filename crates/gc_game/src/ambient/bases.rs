//! Busy bases (docs/34-ambient-life.md, stage 3): a finished building
//! shows that someone lives in it. Ember Clans' halls and houses smoke from
//! their hearths and light their windows at dusk; the Carrion Concord's
//! buildings give off ghost-lights, the Choir's hum with drifting light, the
//! Brood's breathe out spores. All of it is a picture drawn from the
//! simulation's buildings and hashed nowhere.

use gc_sim::structures::Role;
use glam::{Vec2, Vec3};

use crate::effects::Field;
use crate::siege::{Look, SMOKES_BELOW, Style};

/// Warm window light, and the colour of each faction's own glow.
const WINDOW: [u8; 3] = [255, 184, 96];
const GHOST: [u8; 3] = [150, 255, 190];
const RESONANCE: [u8; 3] = [150, 226, 255];
const SPORE: [u8; 3] = [214, 204, 116];
/// Windows are under a pixel from a camera farther than this many tiles of
/// distance, and fade out from `LAMPS_NEAR`.
const LAMPS_NEAR: f32 = 35.0;
const LAMPS_FAR: f32 = 75.0;
/// Smoke and drifting lights can be seen farther; past `SMALL_FAR` only the
/// big buildings make any, so a crowded town doesn't flood the particles.
pub const REACH: f32 = 110.0;
const SMALL_FAR: f32 = 60.0;
/// The most effects time one frame makes smoke and light for, in seconds.
const MOST_DT: f32 = 0.25;
/// How far outside its walls a lamp hangs, in tiles.
const OUTSIDE: f32 = 0.1;

/// Whether a building of this role is lived or worked in.
fn lived_in(role: Role) -> bool {
    !matches!(role, Role::Wall | Role::Defence)
}

/// A number from 0 to 1 that stays the same for a building and a `salt`.
pub(super) fn hash(look: &Look, salt: u32) -> f32 {
    let mut x = look
        .handle
        .index
        .wrapping_mul(0x9e37_79b1)
        .wrapping_add(u32::from(look.handle.generation).wrapping_mul(0x85eb_ca6b))
        .wrapping_add(salt.wrapping_mul(0xc2b2_ae35));
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    (x >> 8) as f32 / (1 << 24) as f32
}

/// Make this frame's smoke and light for building `look`, which the viewer
/// can see. `dt` is the effects time since the last frame, `dusk` how dark
/// the light is (0 by day to 1), `distance` how far the camera is.
pub fn emit(look: &Look, dt: f32, dusk: f32, distance: f32, fx: &mut Field) {
    // A long frame makes no more than a quarter second of smoke and light.
    let dt = dt.min(MOST_DT);
    if !look.finished || look.hp < SMOKES_BELOW || !lived_in(look.role) {
        return;
    }
    let roof = look.top * 0.8;
    let centre = look.centre();
    let size = look.max - look.min;
    let big = matches!(look.role, Role::Main | Role::Outpost);
    if distance > SMALL_FAR && !big {
        return;
    }
    let on = |up: f32, along: f32, across: f32| {
        Vec3::new(
            centre.x + (along - 0.5) * size.x * 0.8,
            look.base + up,
            centre.y + (across - 0.5) * size.y * 0.8,
        )
    };
    let colour = match look.stuff.style {
        Style::Timber | Style::Stone => WINDOW,
        Style::Bone => GHOST,
        Style::Crystal => RESONANCE,
        Style::Shell => SPORE,
    };
    match look.stuff.style {
        Style::Timber | Style::Stone => {
            // A column for the hall, a thinner one for a house; a forge
            // or a kitchen smokes harder.
            let thick = match look.role {
                Role::Main | Role::Outpost => 0.42,
                Role::Production | Role::Economy => 0.34,
                _ => 0.26,
            };
            let at = on(roof, hash(look, 1), hash(look, 2));
            fx.hearth(at, thick, dt);
            if big {
                let at = on(roof, hash(look, 3), hash(look, 4));
                fx.hearth(at, thick * 0.8, dt);
            }
        }
        _ => {
            let rate = if big { 2.0 } else { 1.0 };
            fx.drift(on(look.top, 0.5, 0.5), colour, rate * dt);
        }
    }
    // Lit windows at dusk, one to three a building; the Choir's crystal
    // shines through the day too.
    let glow = match look.stuff.style {
        Style::Crystal => 0.35 + 0.65 * dusk,
        _ => dusk,
    };
    let lamps = 1 + u32::from(size.x * size.y >= 4.0) + u32::from(size.x * size.y >= 12.0);
    let fade = 1.0 - ((distance - LAMPS_NEAR) / (LAMPS_FAR - LAMPS_NEAR)).clamp(0.0, 1.0);
    if glow * fade > 0.0 {
        for k in 0..lamps {
            let (a, b) = (hash(look, 10 + 2 * k), hash(look, 11 + 2 * k));
            // On the walls: pick a side, then a place along it, just outside.
            let along = (b - 0.5) * 0.8;
            let out = size / 2.0 + Vec2::splat(OUTSIDE);
            let (dx, dz) = match (a * 4.0) as u32 {
                0 => (along * size.x, -out.y),
                1 => (along * size.x, out.y),
                2 => (-out.x, along * size.y),
                _ => (out.x, along * size.y),
            };
            let height = look.top * (0.3 + 0.3 * hash(look, 20 + k));
            let at = Vec3::new(centre.x + dx, look.base + height, centre.y + dz);
            fx.lamp(at, colour, 0.13, glow * fade, dt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::siege::stuff;
    use gc_data::Faction;
    use gc_sim::structures::Armour;

    fn look(faction: Faction, role: Role, finished: bool, hp: f32) -> Look {
        Look {
            handle: gc_sim::StructureHandle {
                index: 7,
                generation: 1,
            },
            min: Vec2::new(10.0, 10.0),
            max: Vec2::new(14.0, 13.0),
            base: 0.0,
            top: 3.0,
            model: None,
            lift: 0.0,
            owner: 0,
            colour: [200, 0, 0],
            stuff: stuff(Some(faction), Armour::Structure),
            role,
            finished,
            hp,
        }
    }

    /// Particles made over `seconds` of frames at `dusk`.
    fn made(l: &Look, dusk: f32, distance: f32, seconds: f32) -> usize {
        let mut fx = Field::new(None);
        let mut n = 0;
        for _ in 0..(seconds * 30.0) as usize {
            emit(l, 1.0 / 30.0, dusk, distance, &mut fx);
            n += fx
                .frame(1.0 / 30.0, 1.0, glam::Vec2::ZERO, 200.0, |_| 0.0)
                .len();
        }
        n
    }

    #[test]
    fn a_finished_clan_hall_smokes_by_day_and_is_lit_at_dusk() {
        let hall = look(Faction::EmberClans, Role::Main, true, 1.0);
        let day = made(&hall, 0.0, 20.0, 10.0);
        let dusk = made(&hall, 1.0, 20.0, 10.0);
        assert!(day > 50, "{day}");
        assert!(dusk > day, "{dusk} {day}");
    }

    #[test]
    fn unfinished_hurt_and_wall_buildings_make_nothing() {
        for l in [
            look(Faction::EmberClans, Role::Main, false, 1.0),
            look(Faction::EmberClans, Role::Main, true, 0.3),
            look(Faction::EmberClans, Role::Wall, true, 1.0),
            look(Faction::EmberClans, Role::Defence, true, 1.0),
        ] {
            assert_eq!(made(&l, 1.0, 20.0, 5.0), 0);
        }
    }

    #[test]
    fn windows_fade_with_distance_but_the_smoke_does_not() {
        let house = look(Faction::EmberClans, Role::Main, true, 1.0);
        let near = made(&house, 1.0, 20.0, 10.0);
        let far = made(&house, 1.0, LAMPS_FAR, 10.0);
        assert!(far > 0 && far < near, "{far} {near}");
    }

    #[test]
    fn lamps_hang_outside_the_walls() {
        let hall = look(Faction::EmberClans, Role::Main, true, 1.0);
        let mut fx = Field::new(None);
        // Dusk, near, a second of frames; the smoke is above the roof, so
        // anything low and glowing is a lamp.
        for _ in 0..30 {
            emit(&hall, 1.0 / 30.0, 1.0, 20.0, &mut fx);
        }
        let made = fx.frame(0.0, 1.0, Vec2::ZERO, 200.0, |_| 0.0);
        let lamps: Vec<_> = made.iter().filter(|p| p.pos.y < hall.top * 0.65).collect();
        assert!(!lamps.is_empty());
        for p in lamps {
            let inside = p.pos.x > hall.min.x
                && p.pos.x < hall.max.x
                && p.pos.z > hall.min.y
                && p.pos.z < hall.max.y;
            assert!(!inside, "{:?}", p.pos);
        }
    }

    #[test]
    fn far_off_only_the_big_buildings_smoke() {
        let house = look(Faction::EmberClans, Role::Housing, true, 1.0);
        let hall = look(Faction::EmberClans, Role::Main, true, 1.0);
        assert_eq!(made(&house, 0.0, SMALL_FAR + 5.0, 5.0), 0);
        assert!(made(&hall, 0.0, SMALL_FAR + 5.0, 5.0) > 0);
    }

    #[test]
    fn the_other_factions_glow_instead_of_smoking() {
        for f in [Faction::CarrionConcord, Faction::Choir, Faction::Brood] {
            let l = look(f, Role::Housing, true, 1.0);
            assert!(made(&l, 0.0, 20.0, 20.0) > 0, "{f:?}");
        }
    }
}
