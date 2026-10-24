//! The battlefield's particle effects: where the game asks `gc_render`'s
//! recipes for dust, sparks and smoke. It only collects what to spawn; the
//! frame hands the list to the renderer (`Field::frame`). Nothing here
//! feeds back into the simulation, and effects of what the viewer cannot see
//! are never made, so the dust never shows an army the fog hides.

use gc_render::{DUST, Effects, Glow, HitStyle, Particle, Weather};
use glam::{Vec2, Vec3};

use crate::ambient::Style;
use crate::fx_demo::{Demo, Mode};
use crate::puppets::Hit;

/// How often a soldier on the move kicks up dust, a puff a second at a
/// walk, more at a run.
const FOOTSTEP_RATE: f32 = 0.25;
const FOOTSTEP_RATE_PER_SPEED: f32 = 0.3;
/// Splinters off a shield that took a blow.
const SHIELD_WOOD: [u8; 3] = [130, 96, 60];
/// Light motes show when the camera is nearer than this many tiles, fading
/// in from `MOTES_FAR`, at this many a second for each tile of ground.
const MOTES_NEAR: f32 = 25.0;
const MOTES_FAR: f32 = 60.0;
const MOTES_RATE: f32 = 0.04;
/// The most a second, wherever the camera is.
const MOTES_MOST: f32 = 40.0;
/// Rain and snow: drops or flakes a second for each tile of ground under
/// the fall, the most a second, and how wide it falls round the camera's
/// focus (nearest and farthest radius, in tiles).
const RAIN_PER_TILE: f32 = 0.8;
const RAIN_MOST: f32 = 1200.0;
const SNOW_PER_TILE: f32 = 0.25;
const SNOW_MOST: f32 = 350.0;
const FALL_RADIUS: (f32, f32) = (14.0, 60.0);
/// The camera heights between which the fall fades out.
const FALL_FADE: (f32, f32) = (110.0, 175.0);
/// Rain striking the ground: splashes a second for each tile it covers, the
/// most a second, how wide round the focus (a share of the camera's
/// distance, then the least and most in tiles) and the camera heights
/// between which they fade out (they are too small to see from afar).
const SPLASH_PER_TILE: f32 = 0.35;
const SPLASH_MOST: f32 = 300.0;
const SPLASH_REACH: f32 = 0.4;
const SPLASH_RADIUS: (f32, f32) = (6.0, 26.0);
const SPLASH_FADE: (f32, f32) = (35.0, 90.0);
/// How far from the camera's focus a bolt may strike (nearest and farthest
/// radius, in tiles).
const STRIKE_RADIUS: (f32, f32) = (15.0, 70.0);
/// The colour of clods thrown up from the ground.
const CLOD: [u8; 3] = [92, 74, 52];
/// Fresh-cut wood, and leaves and torn scrub.
const CHIP: [u8; 3] = [208, 170, 112];
const LEAF: [u8; 3] = [74, 112, 52];
/// How often a worker at his tool throws chips, a second.
const CHOP_RATE: f32 = 1.6;
/// Charred ash, crystal glints and the Brood's spores.
const ASH: [u8; 3] = [70, 66, 62];
const ICE: [u8; 3] = [120, 220, 255];
const SPORE: [u8; 3] = [196, 214, 150];

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
        self.fx.take_glows();
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

    /// A soldier at `pos` (his chest, over the ground) took a blow of kind
    /// `hit` travelling along `dir`: sparks when it rang on a weapon or
    /// shield, else his side's wound (`style`), more for a heavy blow and
    /// most for a killing one. `armoured` has his armour ring too.
    pub fn blow(&mut self, pos: Vec3, dir: Vec2, hit: Hit, style: HitStyle, armoured: bool) {
        let along = Vec3::new(dir.x, 0.15, dir.y);
        match hit {
            Hit::Parried => self.fx.sparks(pos, along, 5.0),
            Hit::Blocked => {
                self.fx.sparks(pos, along, 2.0);
                self.fx.chips(pos, -dir, SHIELD_WOOD, 3.0, 0.04);
            }
            Hit::Wounded => self.fx.hit(pos, along, style, armoured, 1.0),
            Hit::Crushed => {
                self.fx.hit(pos, along, style, armoured, 1.6);
                self.fx.dust_cloud(pos - Vec3::Y * 0.5, 0.5, 2.0, DUST);
            }
            Hit::Killed => {
                self.fx.hit(pos, along, style, armoured, 2.2);
                self.fx.dust_cloud(pos - Vec3::Y * 0.5, 0.6, 2.0, DUST);
            }
        }
    }

    /// A worker at `pos` bites into what he is clearing with his tool,
    /// toward `dir`, for `dt` seconds of game time, the way his faction
    /// clears: chips of wood from a tree, leaves and turf from scrub; the
    /// Concord sets `target` (the tile's ground) alight, the Choir cracks
    /// it into glints, the Brood's Gnawers chew it into spores and crumbs.
    pub fn chop(&mut self, dt: f32, pos: Vec3, dir: Vec2, wood: bool, style: Style, target: Vec3) {
        match style {
            Style::Fell => {
                if self.fx.chance(dt * CHOP_RATE) {
                    if wood {
                        self.fx.cut_chips(pos, dir, CHIP, 2.0);
                    } else {
                        self.fx.cut_chips(pos, dir, LEAF, 1.5);
                        self.fx.leaves(pos, 0.3, LEAF, 1.0);
                    }
                }
            }
            Style::Burn => {
                self.fx.flames(target, 0.4, dt);
                self.fx.smoke(target + Vec3::Y * 0.3, 0.45, 0.6, dt);
                if self.fx.chance(dt * 3.0) {
                    self.fx.embers(target, 0.4, 2.0);
                }
            }
            Style::Shatter => {
                if self.fx.chance(dt * CHOP_RATE * 1.5) {
                    self.fx.shards(
                        pos,
                        Vec3::new(dir.x, 0.3, dir.y),
                        if wood { 4.0 } else { 2.5 },
                    );
                    self.fx.motes(pos, 0.3, ICE, 1.0);
                }
            }
            Style::Gnaw => {
                if self.fx.chance(dt * CHOP_RATE * 1.5) {
                    self.fx.cut_chips(pos, dir, SPORE, 1.5);
                    self.fx.motes(pos, 0.3, SPORE, 1.0);
                }
            }
        }
    }

    /// A tree or bush that stands where `pos` is burns for `dt` seconds:
    /// `tall` tiles tall, as a trunk of flame and smoke.
    pub fn blaze(&mut self, pos: Vec3, tall: f32, dt: f32) {
        self.fx
            .flames(pos + Vec3::Y * tall * 0.35, 0.3 + 0.1 * tall, dt);
        self.fx
            .smoke(pos + Vec3::Y * tall * 0.7, 0.35 + 0.1 * tall, 0.8, dt);
        if self.fx.chance(dt * 2.0) {
            self.fx.embers(pos + Vec3::Y * tall * 0.4, 0.3, 2.0);
        }
    }

    /// What a burnt tree leaves as it crumbles at `pos`: a puff of ash.
    pub fn ash_down(&mut self, pos: Vec3, tall: f32) {
        self.fx.dust_burst(pos, 0.5 + 0.1 * tall, 3.0 + tall, ASH);
        self.fx.embers(pos, 0.4, 4.0);
    }

    /// A tree or bush `tall` tiles tall bursts into crystal at `pos`.
    pub fn shatter(&mut self, pos: Vec3, tall: f32) {
        let at = pos + Vec3::Y * tall * 0.5;
        self.fx.shards(at, Vec3::Y, 10.0 + 4.0 * tall);
        self.fx.motes(at, 0.5, ICE, 4.0 + tall);
        self.fx.dust_burst(pos, 0.4 + 0.1 * tall, 2.0, ICE);
    }

    /// A tree or bush being eaten at `pos`, `tall` tiles tall, for `dt`
    /// seconds: crumbs and spores drifting off it.
    pub fn gnawed(&mut self, pos: Vec3, tall: f32, dt: f32) {
        if self.fx.chance(dt * 6.0) {
            self.fx.leaves(pos + Vec3::Y * tall * 0.3, 0.35, SPORE, 2.0);
            self.fx.motes(pos + Vec3::Y * tall * 0.3, 0.3, SPORE, 1.0);
        }
    }

    /// A tree of height `tall` came down with its crown at `crown`: a
    /// ring of dust and a shower of leaves and broken twigs.
    pub fn tree_down(&mut self, crown: Vec3, tall: f32) {
        self.fx
            .dust_burst(crown, 0.8 + 0.2 * tall, 6.0 + 2.0 * tall, DUST);
        self.fx.leaves(
            crown + Vec3::Y * 0.4,
            0.7 + 0.15 * tall,
            LEAF,
            10.0 + 3.0 * tall,
        );
        self.fx.debris(crown, CHIP, 3.0, 0.6);
    }

    /// A bush torn up at `pos`.
    pub fn bush_up(&mut self, pos: Vec3) {
        self.fx.dust_burst(pos, 0.5, 2.0, DUST);
        self.fx.leaves(pos + Vec3::Y * 0.2, 0.4, LEAF, 5.0);
        self.fx.debris(pos, CLOD, 2.0, 0.4);
    }

    /// A speck of pollen at `pos`.
    pub fn pollen(&mut self, pos: Vec3) {
        self.fx.pollen(pos, 1.0);
    }

    /// A firefly at `pos`.
    pub fn firefly(&mut self, pos: Vec3) {
        self.fx.firefly(pos, 1.0);
    }

    /// A leaf of `colour` coming off a tree at `pos`.
    pub fn leaf_fall(&mut self, pos: Vec3, colour: [u8; 3]) {
        self.fx.leaves(pos, 0.1, colour, 1.0);
    }

    /// A whirlwind standing at `centre` throws up dust for `dt` seconds.
    pub fn dust_devil(&mut self, dt: f32, centre: Vec3) {
        self.fx.dust_devil(centre, dt, DUST);
    }

    /// Wood smoke of a hearth at `pos`, `size` tiles wide, for `dt` seconds.
    pub fn hearth(&mut self, pos: Vec3, size: f32, dt: f32) {
        self.fx.smoke(pos, size, 0.05, dt);
    }

    /// A lit window or lantern at `pos`, at `level` of full brightness, for
    /// `dt` seconds.
    pub fn lamp(&mut self, pos: Vec3, colour: [u8; 3], size: f32, level: f32, dt: f32) {
        self.fx.lamp(pos, colour, size, level, dt);
    }

    /// Floating lights of `colour` over `pos`, `amount` of them.
    pub fn drift(&mut self, pos: Vec3, colour: [u8; 3], amount: f32) {
        self.fx.motes(pos, 0.4, colour, amount);
    }

    /// Dust rolling out to `radius` from `puffs`, and `clods` thrown up with
    /// `power`.
    fn impact(&mut self, pos: Vec3, radius: f32, puffs: f32, clods: f32, power: f32) {
        self.fx.dust_burst(pos, radius, puffs, DUST);
        self.fx.debris(pos, CLOD, clods, power);
    }

    /// Light motes over the ground the camera looks at (`distance` is how
    /// far it is from it), when it is close enough to see them.
    fn ambient(&mut self, dt: f32, focus: Vec2, distance: f32, ground: &impl Fn(Vec2) -> f32) {
        let near = 1.0 - ((distance - MOTES_NEAR) / (MOTES_FAR - MOTES_NEAR)).clamp(0.0, 1.0);
        if near <= 0.0 {
            return;
        }
        let radius = distance * 0.45;
        let rate = (MOTES_RATE * std::f32::consts::PI * radius * radius * near).min(MOTES_MOST);
        self.fx
            .light_motes(focus, radius, rate * dt.min(0.25), ground);
    }

    /// Rain or snow over the ground the camera looks at, `sky` being the
    /// weather and `amount` of the full fall (0 none, 1 the heaviest). It falls over a disc round the
    /// focus that grows with the camera's height, and a heavy fall is capped
    /// so a battle's own effects keep room in the ring.
    pub fn weather(
        &mut self,
        dt: f32,
        density: f32,
        (weather, amount, wind): (Weather, f32, Vec2),
        focus: Vec2,
        distance: f32,
        ground: impl Fn(Vec2) -> f32,
    ) {
        if matches!(weather, Weather::Clear | Weather::Fog) || amount <= 0.0 {
            return;
        }
        let (per_tile, most) = match weather {
            Weather::Snow => (SNOW_PER_TILE, SNOW_MOST),
            _ => (RAIN_PER_TILE, RAIN_MOST),
        };
        // Seen from high up the streaks are under a pixel wide, so the fall
        // thins out with the camera's height and is gone in Commander View.
        let near = 1.0 - ((distance - FALL_FADE.0) / (FALL_FADE.1 - FALL_FADE.0)).clamp(0.0, 1.0);
        let radius = (distance * 0.5).clamp(FALL_RADIUS.0, FALL_RADIUS.1);
        let rate = (per_tile * std::f32::consts::PI * radius * radius).min(most) * amount * near;
        self.fx.set_density(density);
        self.fx
            .precipitation(weather, focus, radius, rate * dt.min(0.25), wind, &ground);
        if weather == Weather::Rain {
            let near = 1.0
                - ((distance - SPLASH_FADE.0) / (SPLASH_FADE.1 - SPLASH_FADE.0)).clamp(0.0, 1.0);
            let radius = (distance * SPLASH_REACH).clamp(SPLASH_RADIUS.0, SPLASH_RADIUS.1);
            let rate = (SPLASH_PER_TILE * std::f32::consts::PI * radius * radius).min(SPLASH_MOST)
                * amount
                * near;
            self.fx
                .splashes(focus, radius, rate * dt.min(0.25), &ground);
        }
    }

    /// A bolt of lightning somewhere in view of the camera at `focus`,
    /// `distance` from the ground and looking along `heading` (a unit
    /// vector over the map): taller and wider the further the camera is,
    /// so it can be seen from up high. It falls ahead of the camera, never
    /// at its feet, and only where `fits` (on the map and seen, say).
    pub fn strike(
        &mut self,
        focus: Vec2,
        distance: f32,
        heading: Vec2,
        ground: impl Fn(Vec2) -> f32,
        fits: impl Fn(Vec2) -> bool,
    ) {
        let radius = (distance * 0.6).clamp(STRIKE_RADIUS.0, STRIKE_RADIUS.1);
        let top = 35.0 + distance * 0.2;
        let width = 0.05 + distance * 0.0025;
        // The camera hangs at least a dozen tiles behind its focus (eight
        // when riding), so a little behind the focus is still well ahead.
        let ahead = |at: Vec2| (at - focus).dot(heading) > -radius * 0.1;
        self.fx.lightning(focus, radius, top, width, ground, |at| {
            ahead(at) && fits(at)
        });
    }

    /// The lamps and fires that shone since this was last called.
    pub fn take_glows(&mut self) -> Vec<Glow> {
        self.fx.take_glows()
    }

    /// Everything collected this frame, ready for
    /// `Renderer::spawn_particles`. `dt` is the effects time since the last
    /// frame (the frame time, scaled by game speed and `time_scale`), `focus`
    /// the camera's focus (where the demo plays) and `distance` how far the
    /// camera is from it, `ground` the terrain height, and `density` how
    /// much to spawn (the Graphics preset's choice).
    pub fn frame(
        &mut self,
        dt: f32,
        density: f32,
        focus: Vec2,
        distance: f32,
        ground: impl Fn(Vec2) -> f32,
    ) -> Vec<Particle> {
        self.fx.set_density(density);
        self.ambient(dt, focus, distance, &ground);
        if let Some(demo) = &mut self.demo {
            demo.play(&mut self.fx, dt, focus, &ground);
        }
        self.fx.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_blow_throws_something_and_a_kill_more_than_a_parry() {
        let made = |hit, style| {
            let mut f = Field::new(None);
            f.blow(Vec3::new(5.0, 1.0, 5.0), Vec2::X, hit, style, false);
            f.frame(0.0, 1.0, Vec2::ZERO, 40.0, |_| 0.0).len()
        };
        for hit in [
            Hit::Parried,
            Hit::Blocked,
            Hit::Wounded,
            Hit::Crushed,
            Hit::Killed,
        ] {
            assert!(made(hit, HitStyle::Flesh) > 0, "{hit:?}");
        }
        assert!(made(Hit::Killed, HitStyle::Flesh) > made(Hit::Wounded, HitStyle::Flesh));
        assert!(made(Hit::Wounded, HitStyle::Ichor) > 0);
    }

    fn motes(distance: f32) -> usize {
        let mut field = Field::new(None);
        // A second of effects time, in frames.
        (0..60)
            .map(|_| {
                field
                    .frame(1.0 / 60.0, 1.0, Vec2::new(50.0, 50.0), distance, |_| 3.0)
                    .len()
            })
            .sum()
    }

    fn fall(weather: Weather, amount: f32, distance: f32) -> usize {
        let mut field = Field::new(None);
        // A second of effects time, in frames.
        for _ in 0..60 {
            field.weather(
                1.0 / 60.0,
                1.0,
                (weather, amount, Vec2::ZERO),
                Vec2::new(50.0, 50.0),
                distance,
                |_| 3.0,
            );
        }
        field.frame(0.0, 1.0, Vec2::ZERO, 200.0, |_| 0.0).len()
    }

    #[test]
    fn rain_and_snow_fall_and_a_clear_sky_and_fog_do_not() {
        assert!(fall(Weather::Rain, 1.0, 40.0) > 300);
        assert!(fall(Weather::Snow, 1.0, 40.0) > 100);
        assert_eq!(fall(Weather::Clear, 1.0, 40.0), 0);
        assert_eq!(fall(Weather::Fog, 1.0, 40.0), 0);
        assert_eq!(fall(Weather::Rain, 0.0, 40.0), 0);
    }

    #[test]
    fn a_light_fall_is_thinner_and_a_high_camera_does_not_flood_the_ring() {
        let light = fall(Weather::Rain, 0.3, 40.0);
        let heavy = fall(Weather::Rain, 1.0, 40.0);
        assert!(light < heavy);
        // The widest fall is capped, so a battle's own effects keep room in
        // the ring: everything together spawns under 4,000 a second, which
        // the ring holds for its longest life (8 seconds).
        assert!(fall(Weather::Rain, 1.0, 100.0) <= RAIN_MOST as usize + 60);
        assert!(fall(Weather::Snow, 1.0, 100.0) <= SNOW_MOST as usize + 20);
        assert!(RAIN_MOST + SPLASH_MOST + 400.0 < gc_render::MAX_PARTICLES as f32 / 8.0);
        // High up, in Commander View, there is none.
        assert_eq!(fall(Weather::Rain, 1.0, 180.0), 0);
        assert!(fall(Weather::Rain, 1.0, 140.0) < fall(Weather::Rain, 1.0, 100.0));
    }

    #[test]
    fn rain_splashes_where_it_lands_only_near_the_camera_and_snow_does_not() {
        let splashes = |weather, distance| {
            let mut field = Field::new(None);
            for _ in 0..60 {
                field.weather(
                    1.0 / 60.0,
                    1.0,
                    (weather, 1.0, Vec2::ZERO),
                    Vec2::new(50.0, 50.0),
                    distance,
                    |_| 3.0,
                );
            }
            field
                .frame(0.0, 1.0, Vec2::ZERO, 200.0, |_| 0.0)
                .iter()
                .filter(|p| p.life < 0.4 && p.pos.y < 3.2)
                .count()
        };
        assert!(splashes(Weather::Rain, 40.0) > 100);
        assert!(splashes(Weather::Rain, 40.0) as f32 <= SPLASH_MOST + 20.0);
        assert_eq!(splashes(Weather::Rain, 100.0), 0);
        assert_eq!(splashes(Weather::Snow, 40.0), 0);
    }

    #[test]
    fn a_strike_lands_near_the_camera_and_is_bigger_from_far_up() {
        let bolt = |distance| {
            let mut field = Field::new(None);
            field.strike(Vec2::new(50.0, 50.0), distance, Vec2::X, |_| 3.0, |_| true);
            field.frame(0.0, 1.0, Vec2::ZERO, 200.0, |_| 0.0)
        };
        let (near, far) = (bolt(30.0), bolt(150.0));
        assert!(!near.is_empty());
        let flash = near.last().unwrap();
        // Within the strike disc (0.6 × the distance, 15 to 70 tiles).
        assert!(Vec2::new(flash.pos.x, flash.pos.z).distance(Vec2::new(50.0, 50.0)) <= 18.01);
        assert!(far.last().unwrap().size[1] > flash.size[1] * 2.0);
        let height =
            |parts: &[gc_render::Particle]| parts.iter().map(|p| p.pos.y).fold(0.0, f32::max);
        assert!(height(&far) > height(&near));
    }

    #[test]
    fn a_strike_falls_ahead_of_the_camera_and_where_it_fits() {
        let (focus, heading) = (Vec2::new(50.0, 50.0), Vec2::new(0.0, 1.0));
        for _ in 0..30 {
            let mut field = Field::new(None);
            field.strike(focus, 20.0, heading, |_| 3.0, |at| at.x < 55.0);
            let parts = field.frame(0.0, 1.0, Vec2::ZERO, 200.0, |_| 0.0);
            let flash = parts.last().unwrap();
            assert!(flash.pos.z > 50.0 - 15.0 * 0.1 - 0.01, "{}", flash.pos.z);
            assert!(flash.pos.x < 55.0);
        }
        // Nowhere to strike: no bolt.
        let mut field = Field::new(None);
        field.strike(focus, 20.0, heading, |_| 3.0, |_| false);
        assert!(field.frame(0.0, 1.0, Vec2::ZERO, 200.0, |_| 0.0).is_empty());
    }

    #[test]
    fn a_slow_frame_makes_no_more_than_a_quarter_second_of_rain() {
        let mut field = Field::new(None);
        field.weather(
            10.0,
            1.0,
            (Weather::Rain, 1.0, Vec2::ZERO),
            Vec2::ZERO,
            40.0,
            |_| 0.0,
        );
        let n = field.frame(0.0, 1.0, Vec2::ZERO, 200.0, |_| 0.0).len();
        assert!(n as f32 <= (RAIN_MOST + SPLASH_MOST) * 0.26 + 5.0, "{n}");
    }

    #[test]
    fn light_motes_hang_in_the_air_near_a_close_camera() {
        assert!(motes(25.0) > 0);
        assert!(motes(40.0) > 0);
    }

    #[test]
    fn a_far_camera_makes_none() {
        assert_eq!(motes(MOTES_FAR), 0);
        assert_eq!(motes(150.0), 0);
    }

    #[test]
    fn a_slow_frame_makes_no_more_than_a_quarter_second_of_motes() {
        let mut field = Field::new(None);
        let n = field
            .frame(30.0, 1.0, Vec2::new(50.0, 50.0), 25.0, |_| 0.0)
            .len();
        assert!(n as f32 <= MOTES_MOST * 0.25 + 1.0);
    }
}
