//! The land's small living touches (docs/34-ambient-life.md, stage 2):
//! pollen drifting over meadows by day, fireflies over wet ground at dusk,
//! leaves coming off old woods, a whirlwind wandering the waste. None of it
//! is in the simulation. Each is made by trying random points on the ground
//! round the camera and spawning where the land's habitat says it belongs,
//! so a settled screen costs a few dozen probes a second and no state.

use gc_map::Map;
use gc_map::grown::Habitat;
use glam::{Vec2, Vec3};

use crate::effects;

/// How far from the camera's focus touches are made at no distance and per
/// tile of camera distance, and the most, in tiles.
const REACH_NEAR: f32 = 14.0;
const REACH_PER_ZOOM: f32 = 0.9;
const REACH_MOST: f32 = 48.0;
/// Specks made a second per tile of fitting ground, at full density.
const POLLEN_RATE: f32 = 0.010;
const FIREFLY_RATE: f32 = 0.006;
const LEAF_RATE: f32 = 0.004;
/// Specks are under a pixel from a far camera: they fade out between these
/// camera distances, in tiles, as the light motes do.
const FADE_NEAR: f32 = 30.0;
const FADE_FAR: f32 = 70.0;
/// The sun's elevation (its y, normalised) above which pollen drifts and
/// below which fireflies come out, each fading in over `DUSK_SPAN`.
const DUSK_AT: f32 = 0.30;
const DUSK_SPAN: f32 = 0.12;
/// A whirlwind: how often one starts in the waste, a second, how long it
/// lasts and how fast it wanders, in seconds and tiles a second.
const DEVIL_CHANCE: f32 = 0.02;
const DEVIL_SECONDS: f32 = 14.0;
const DEVIL_SPEED: f32 = 2.2;
/// Fish rises: how many a second for each tile of the camera's reach (water
/// or not), at full density (it is doubled at dusk, when they feed), and how
/// far below the surface the bed must be for there to be a fish.
const RISE_RATE: f32 = 0.003;
const RISE_DEPTH: f32 = 0.3;
/// Leaves: how high a tree sheds them.
const LEAF_HEIGHT: [f32; 2] = [2.0, 4.0];

/// How dark the light is for a sun at elevation `sun` (its y, normalised):
/// 0 by day to 1 at dusk, fading over `DUSK_SPAN` about `DUSK_AT`.
pub fn dusk_of(sun: f32) -> f32 {
    ((DUSK_AT - sun) / DUSK_SPAN + 0.5).clamp(0.0, 1.0)
}

/// Which touches a habitat has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Has {
    pollen: bool,
    firefly: bool,
    leaves: bool,
    waste: bool,
}

fn touches_of(h: Habitat) -> Has {
    use Habitat::*;
    Has {
        pollen: matches!(h, FlowerMeadow | Grassland | Glade | HeatherMoor),
        firefly: matches!(
            h,
            WetMeadow | SedgeTussock | AlderCarr | PeatBog | FernHollow | Glade
        ),
        leaves: matches!(h, OldGrowth | BirchFringe | TreeLine | FernHollow),
        waste: matches!(h, CrackedFlat | BoneDrift | DyingGrass),
    }
}

/// The leaves' colour by habitat: birches go gold, the rest stay green.
fn leaf_colour(h: Habitat) -> [u8; 3] {
    match h {
        Habitat::BirchFringe => [214, 178, 62],
        Habitat::TreeLine => [150, 150, 70],
        _ => [74, 112, 52],
    }
}

/// A whirlwind: where it is, which way it drifts, how long it has left.
#[derive(Clone, Copy, Debug)]
struct Devil {
    at: Vec2,
    dir: Vec2,
    left: f32,
}

#[derive(Default)]
pub struct Touches {
    rng: u32,
    devil: Option<Devil>,
}

impl Touches {
    fn unit(&mut self) -> f32 {
        if self.rng == 0 {
            self.rng = 0x2545_f491;
        }
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }

    /// A stochastic count: `n` rounded down, or up with the remainder's
    /// chance.
    fn count(&mut self, n: f32) -> u32 {
        n.floor() as u32 + u32::from(self.unit() < n.fract())
    }

    /// A random point within `reach` tiles of `focus`.
    fn near(&mut self, focus: Vec2, reach: f32) -> Vec2 {
        let a = self.unit() * std::f32::consts::TAU;
        let r = self.unit().sqrt() * reach;
        focus + Vec2::new(a.cos(), a.sin()) * r
    }

    /// Make this frame's touches. `dt` is the effects time since the last
    /// frame (zero while paused), `focus` the camera's focus and `distance`
    /// how far back it is, `sun` the look's sun elevation (its y,
    /// normalised), `density` the Graphics preset's share (zero makes nothing; the recipes do the thinning), `visible` whether
    /// the viewer can see a tile.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        dt: f32,
        focus: Vec2,
        distance: f32,
        sun: f32,
        density: f32,
        map: &Map,
        visible: impl Fn((i32, i32)) -> bool,
        fx: &mut effects::Field,
    ) {
        if dt <= 0.0 || density <= 0.0 || map.habitat.is_empty() {
            return;
        }
        let reach = (REACH_NEAR + distance * REACH_PER_ZOOM).min(REACH_MOST);
        let area = std::f32::consts::PI * reach * reach;
        let day = 1.0 - dusk_of(sun);
        let dusk = dusk_of(sun);
        let near = 1.0 - ((distance - FADE_NEAR) / (FADE_FAR - FADE_NEAR)).clamp(0.0, 1.0);
        let tiles = map.tiles as i32;
        // The habitat on the viewer's ground at `p`; the fog is asked last,
        // as the cheap checks rule most points out.
        let habitat_at = |p: Vec2, wanted: &dyn Fn(Has) -> bool| {
            let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
            if !(0..tiles).contains(&x) || !(0..tiles).contains(&y) {
                return None;
            }
            let h = Habitat::from_u8(*map.habitat.get(y as usize * tiles as usize + x as usize)?);
            (wanted(touches_of(h)) && visible((x, y))).then_some(h)
        };
        // Spawn on the ground (not under water), a little above it.
        let ground = |p: Vec2| map.height_at(p.x, p.y).max(map.water_level());
        let wet_ok = |p: Vec2| map.height_at(p.x, p.y) >= map.water_level();
        for (rate, want) in [
            (POLLEN_RATE * day * near, 0u8),
            (FIREFLY_RATE * dusk * near, 1),
            (LEAF_RATE * near, 2),
        ] {
            // The recipes thin themselves by the density, so trials are not.
            let trials = self.count(rate * area * dt);
            for _ in 0..trials {
                let p = self.near(focus, reach);
                let Some(h) = habitat_at(p, &|has| match want {
                    0 => has.pollen,
                    1 => has.firefly,
                    _ => has.leaves,
                }) else {
                    continue;
                };
                let at = |up: f32| Vec3::new(p.x, ground(p) + up, p.y);
                match want {
                    0 => {
                        if wet_ok(p) {
                            let up = 0.2 + self.unit() * 0.9;
                            fx.pollen(at(up));
                        }
                    }
                    1 => {
                        let up = 0.3 + self.unit() * 1.1;
                        fx.firefly(at(up));
                    }
                    _ => {
                        let up = LEAF_HEIGHT[0] + self.unit() * (LEAF_HEIGHT[1] - LEAF_HEIGHT[0]);
                        fx.leaf_fall(at(up), leaf_colour(h));
                    }
                }
            }
        }
        self.whirlwind(dt, focus, reach, &habitat_at, &ground, fx);
    }

    /// Fish rising on the deeper water near the camera: rings spreading
    /// where one takes a fly. Anywhere there is water (habitats or no), a
    /// little more at dusk. Arguments as for [`Touches::frame`].
    #[allow(clippy::too_many_arguments)]
    pub fn rises(
        &mut self,
        dt: f32,
        focus: Vec2,
        distance: f32,
        sun: f32,
        density: f32,
        map: &Map,
        visible: impl Fn((i32, i32)) -> bool,
        fx: &mut effects::Field,
    ) {
        if dt <= 0.0 || density <= 0.0 {
            return;
        }
        let reach = (REACH_NEAR + distance * REACH_PER_ZOOM).min(REACH_MOST);
        let near = 1.0 - ((distance - FADE_NEAR) / (FADE_FAR - FADE_NEAR)).clamp(0.0, 1.0);
        let rate = RISE_RATE * (1.0 + dusk_of(sun)) * near;
        let area = std::f32::consts::PI * reach * reach;
        let (low, tiles) = (map.water_level(), map.tiles as f32);
        for _ in 0..self.count(rate * area * dt) {
            let p = self.near(focus, reach);
            if p.x < 1.0 || p.y < 1.0 || p.x > tiles - 1.0 || p.y > tiles - 1.0 {
                continue;
            }
            if map.height_at(p.x, p.y) < low - RISE_DEPTH
                && visible((p.x.floor() as i32, p.y.floor() as i32))
            {
                fx.rise(Vec3::new(p.x, low, p.y));
            }
        }
    }

    /// The whirlwind: one at a time, started in the waste near the camera,
    /// wandering a while and throwing up dust.
    fn whirlwind(
        &mut self,
        dt: f32,
        focus: Vec2,
        reach: f32,
        habitat_at: &impl Fn(Vec2, &dyn Fn(Has) -> bool) -> Option<Habitat>,
        ground: &impl Fn(Vec2) -> f32,
        fx: &mut effects::Field,
    ) {
        if self.devil.is_none() && self.unit() < DEVIL_CHANCE * dt {
            let p = self.near(focus, reach);
            if habitat_at(p, &|has| has.waste).is_some() {
                let a = self.unit() * std::f32::consts::TAU;
                self.devil = Some(Devil {
                    at: p,
                    dir: Vec2::new(a.cos(), a.sin()),
                    left: DEVIL_SECONDS,
                });
            }
        }
        let Some(mut d) = self.devil else { return };
        d.at += d.dir * DEVIL_SPEED * dt;
        d.left -= dt;
        // It dies out leaving the waste, going off the map or the camera,
        // or when its time is up.
        let alive = d.left > 0.0
            && d.at.distance(focus) < reach * 1.5
            && habitat_at(d.at, &|has| has.waste).is_some();
        self.devil = alive.then_some(d);
        if alive {
            fx.dust_devil(dt, Vec3::new(d.at.x, ground(d.at), d.at.y));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn habitats_have_their_own_touches() {
        assert!(touches_of(Habitat::FlowerMeadow).pollen);
        assert!(touches_of(Habitat::PeatBog).firefly);
        assert!(!touches_of(Habitat::PeatBog).pollen);
        assert!(touches_of(Habitat::OldGrowth).leaves);
        assert!(touches_of(Habitat::CrackedFlat).waste);
        // Rock and scree are bare.
        for h in [Habitat::BareRock, Habitat::Scree] {
            assert_eq!(touches_of(h), Has::default());
        }
    }

    #[test]
    fn counts_average_out_to_the_rate() {
        let mut t = Touches::default();
        let total: u32 = (0..10_000).map(|_| t.count(0.3)).sum();
        assert!((2700..3300).contains(&total), "{total}");
    }

    #[test]
    fn points_stay_within_reach() {
        let mut t = Touches::default();
        let focus = Vec2::new(40.0, 40.0);
        for _ in 0..1000 {
            assert!(t.near(focus, 10.0).distance(focus) <= 10.001);
        }
    }

    /// A small grown map whose every tile is `h`.
    fn land(h: Habitat) -> Map {
        let mut map = gc_map::FieldMap {
            choice: gc_map::GROWN + 1,
            tiles: 64,
            seed: 5,
        }
        .make();
        map.habitat.fill(h as u8);
        map
    }

    /// What `touches` makes in `seconds` of frames, over the middle of
    /// `map`, with the sun at `sun` and the camera `distance` away.
    fn made(map: &Map, sun: f32, distance: f32, visible: bool, seconds: f32) -> usize {
        let (mut touches, mut fx) = (Touches::default(), effects::Field::new(None));
        let mut n = 0;
        for _ in 0..(seconds * 30.0) as usize {
            touches.frame(
                1.0 / 30.0,
                Vec2::new(32.0, 32.0),
                distance,
                sun,
                1.0,
                map,
                |_| visible,
                &mut fx,
            );
            // Far from its own motes, so only the touches come back.
            n += fx
                .frame(1.0 / 30.0, 1.0, Vec2::new(32.0, 32.0), 200.0, |_| 0.0)
                .len();
        }
        n
    }

    #[test]
    fn pollen_comes_by_day_and_fireflies_at_dusk() {
        let meadow = land(Habitat::FlowerMeadow);
        assert!(made(&meadow, 0.8, 20.0, true, 20.0) > 0);
        assert_eq!(made(&meadow, 0.1, 20.0, true, 20.0), 0);
        let fen = land(Habitat::PeatBog);
        assert_eq!(made(&fen, 0.8, 20.0, true, 20.0), 0);
        assert!(made(&fen, 0.1, 20.0, true, 20.0) > 0);
    }

    #[test]
    fn nothing_is_made_in_the_fog_or_far_off() {
        let meadow = land(Habitat::FlowerMeadow);
        assert_eq!(made(&meadow, 0.8, 20.0, false, 20.0), 0);
        assert_eq!(made(&meadow, 0.8, FADE_FAR, true, 20.0), 0);
    }

    #[test]
    fn bare_ground_has_no_touches_and_a_paused_frame_none() {
        assert_eq!(made(&land(Habitat::BareRock), 0.8, 20.0, true, 20.0), 0);
        let (mut touches, mut fx) = (Touches::default(), effects::Field::new(None));
        let map = land(Habitat::FlowerMeadow);
        touches.frame(
            0.0,
            Vec2::new(32.0, 32.0),
            20.0,
            0.8,
            1.0,
            &map,
            |_| true,
            &mut fx,
        );
        assert!(
            fx.frame(0.0, 1.0, Vec2::new(32.0, 32.0), 200.0, |_| 0.0)
                .is_empty()
        );
    }

    /// What `rises` makes over a map with a pond in its middle: the rings
    /// (their places) in `seconds` of frames.
    fn rings(map: &Map, visible: bool, density: f32, seconds: f32) -> Vec<Vec2> {
        let (mut touches, mut fx) = (Touches::default(), effects::Field::new(None));
        let at = Vec2::splat(map.tiles as f32 / 2.0);
        let mut made = Vec::new();
        for _ in 0..(seconds * 30.0) as usize {
            touches.rises(
                1.0 / 30.0,
                at,
                20.0,
                0.8,
                density,
                map,
                |_| visible,
                &mut fx,
            );
            made.extend(
                fx.frame(1.0 / 30.0, density, at, 200.0, |_| 0.0)
                    .iter()
                    .filter(|p| p.size[1] > 0.1 && p.life > 1.0)
                    .map(|p| Vec2::new(p.pos.x, p.pos.z)),
            );
        }
        made
    }

    /// A flat map with a pond in its middle.
    fn pond() -> Map {
        let (mut map, _) = crate::ambient::hamlets::tests::flat();
        let (n, side) = (map.tiles as usize, map.side() as usize);
        let (lo, hi) = (n / 2 - 12, n / 2 + 12);
        let per = (side - 1) / n;
        for y in lo * per..hi * per {
            for x in lo * per..hi * per {
                map.samples[y * side + x] = 0;
            }
        }
        map
    }

    #[test]
    fn fish_rise_only_on_deep_water_the_viewer_sees() {
        let map = pond();
        let (lo, hi) = (map.tiles as f32 / 2.0 - 12.5, map.tiles as f32 / 2.0 + 12.5);
        let made = rings(&map, true, 1.0, 60.0);
        assert!(made.len() > 10, "{}", made.len());
        for p in &made {
            assert!((lo..hi).contains(&p.x) && (lo..hi).contains(&p.y), "{p}");
        }
        assert!(rings(&map, false, 1.0, 60.0).is_empty());
        assert!(rings(&map, true, 0.0, 60.0).is_empty());
        // On dry land there are none, nor over shallows a fish could not
        // rise in (a tenth of a tile of water) but there are over a deeper
        // pond (half a tile).
        let (dry, _) = crate::ambient::hamlets::tests::flat();
        assert!(rings(&dry, true, 1.0, 60.0).is_empty());
        let filled = |depth: f32| {
            let mut map = pond();
            let top = map.max_height();
            let sample = ((map.water_level() - depth) / top * f32::from(u16::MAX)) as u16;
            map.samples.iter_mut().for_each(|h| *h = (*h).max(sample));
            map
        };
        assert!(rings(&filled(0.1), true, 1.0, 60.0).is_empty());
        assert!(!rings(&filled(0.5), true, 1.0, 60.0).is_empty());
    }

    #[test]
    fn a_whirlwind_blows_over_the_waste_and_dies_out_of_it() {
        let waste = land(Habitat::CrackedFlat);
        // A few minutes is enough for one to start (a chance a second).
        assert!(made(&waste, 0.8, 20.0, true, 300.0) > 0);
        let meadow = land(Habitat::Grassland);
        assert_eq!(made(&meadow, 0.1, 20.0, true, 300.0), 0);
    }
}
