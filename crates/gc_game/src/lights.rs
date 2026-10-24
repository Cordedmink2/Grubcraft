//! Point lights from what shines at night. Lit windows and fires report a
//! [`Glow`] each frame they are on (`gc_render::Effects::lamp` and
//! `flames`); this keeps each one lit for a short while after its last
//! report, so a fire that is only stoked every few frames does not blink,
//! picks the ones that matter most to the camera and turns them into the
//! renderer's [`PointLight`]s. In daylight there are none, so the shaders pay
//! nothing. Presentation only: nothing here touches the simulation, and only
//! glows the viewer can see this frame are kept (the caller filters them), so
//! a light never shows a fire in the fog.

use gc_render::{Glow, MAX_LIGHTS, PointLight};
use glam::Vec2;

/// A glow not reported again for this many seconds of effects time goes out,
/// fading over the last of it.
const KEEP: f32 = 0.5;
/// Glows within this many tiles of each other are the one light, or within
/// this share of the larger one's reach if that is more (a collapsing
/// building throws flames all over its footprint; they are one fire).
const MERGE: f32 = 0.45;
const MERGE_REACH: f32 = 0.5;
/// The most glows tracked at once.
const MOST: usize = 512;
/// How strong a light of full strength is: HDR, where about 1.7 is sunlit
/// white paint. Ground and walls are dark and the light falls off with the
/// square of the way to its edge, so a lamp needs this much to show.
const BRIGHTNESS: f32 = 5.0;
/// No lights from a camera higher than `REACH`, fading out over the 30 tiles
/// below it (they are a few pixels across from there).
const REACH: f32 = 140.0;
const REACH_FADE: f32 = 30.0;
/// Below this share of darkness a fire adds no light at all (a lamp is
/// already dim by day: the caller passes it scaled by dusk).
const DARK_ENOUGH: f32 = 0.05;
/// How many of the last lights in the ranking fade out rather than cut, so
/// panning does not pop a light on or off.
const FADE_RANKS: usize = 4;

struct Item {
    glow: Glow,
    /// Seconds since it was last reported.
    quiet: f32,
}

#[derive(Default)]
pub struct Lights {
    items: Vec<Item>,
}

impl Lights {
    /// Forget everything (a new match).
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// The lights to draw. `dt` seconds of effects time have passed since
    /// the last call, and `glows` are the ones that shone in them and are
    /// in the viewer's sight (a glow in the fog is not passed). The camera
    /// is at `focus` and `distance` high; `darkness` runs from 0 (day) to 1.
    pub fn frame(
        &mut self,
        dt: f32,
        glows: impl IntoIterator<Item = Glow>,
        focus: Vec2,
        distance: f32,
        darkness: f32,
    ) -> Vec<PointLight> {
        // Age first, so a glow reported this frame is full strength however
        // long the frame was.
        for item in &mut self.items {
            item.quiet += dt.max(0.0);
        }
        self.items.retain(|i| i.quiet < KEEP);
        self.note(glows);
        let far = 1.0 - ((distance - (REACH - REACH_FADE)) / REACH_FADE).clamp(0.0, 1.0);
        if far <= 0.0 {
            return Vec::new();
        }
        let mut ranked: Vec<(f32, &Item, f32)> = self
            .items
            .iter()
            .filter_map(|i| {
                let dark = if i.glow.needs_dark {
                    if darkness < DARK_ENOUGH {
                        return None;
                    }
                    darkness
                } else {
                    1.0
                };
                let fade = 1.0 - (i.quiet / KEEP).powi(2);
                let level = i.glow.strength * fade * dark * far;
                if level <= 0.02 {
                    return None;
                }
                let away = Vec2::new(i.glow.pos.x, i.glow.pos.z).distance_squared(focus);
                // Big, bright and near first.
                let score = level * i.glow.radius * i.glow.radius / (25.0 + away);
                Some((score, i, level))
            })
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        ranked
            .into_iter()
            .take(MAX_LIGHTS)
            .enumerate()
            .map(|(rank, (_, i, level))| {
                let last = MAX_LIGHTS - rank;
                let rank_fade = if last <= FADE_RANKS {
                    last as f32 / (FADE_RANKS + 1) as f32
                } else {
                    1.0
                };
                let [r, g, b] = i.glow.color.map(|c| (f32::from(c) / 255.0).powf(2.2));
                let k = level * rank_fade * BRIGHTNESS;
                PointLight {
                    pos: i.glow.pos,
                    radius: i.glow.radius,
                    color: [r * k, g * k, b * k],
                    seed: seed_of(i.glow.pos.x, i.glow.pos.z),
                }
            })
            .collect()
    }

    /// Glows reported this frame. One near an item that was already
    /// reported this frame joins it (the brighter and wider of the two wins,
    /// at the stronger one's place); one near an older item replaces it.
    fn note(&mut self, glows: impl IntoIterator<Item = Glow>) {
        for glow in glows {
            let near = |i: &Item| {
                let reach = MERGE.max(MERGE_REACH * i.glow.radius.max(glow.radius));
                Vec2::new(i.glow.pos.x, i.glow.pos.z).distance(Vec2::new(glow.pos.x, glow.pos.z))
                    < reach
                    && (i.glow.pos.y - glow.pos.y).abs() < reach
            };
            match self.items.iter().position(near) {
                Some(at) => {
                    let item = &mut self.items[at];
                    if item.quiet > 0.0 {
                        *item = Item { glow, quiet: 0.0 };
                    } else {
                        let stronger = glow.strength > item.glow.strength;
                        let (strength, radius) = (
                            glow.strength.max(item.glow.strength),
                            glow.radius.max(item.glow.radius),
                        );
                        if stronger {
                            item.glow = glow;
                        }
                        item.glow.strength = strength;
                        item.glow.radius = radius;
                    }
                }
                None if self.items.len() < MOST => self.items.push(Item { glow, quiet: 0.0 }),
                None => {}
            }
        }
    }
}

/// Keep only the glows on tiles `sees` says the viewer sees now.
pub fn in_sight(glows: &mut Vec<Glow>, sees: impl Fn((i32, i32)) -> bool) {
    glows.retain(|g| sees((g.pos.x.floor() as i32, g.pos.z.floor() as i32)));
}

/// A steady number for a place, so a light flickers in its own way.
fn seed_of(x: f32, z: f32) -> u8 {
    let h = (x * 73.0).floor() as i32 * 31 + (z * 57.0).floor() as i32 * 17;
    (h & 0xff) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn glow(x: f32, z: f32) -> Glow {
        Glow {
            pos: Vec3::new(x, 1.0, z),
            color: [255, 160, 70],
            radius: 3.0,
            strength: 1.0,
            needs_dark: true,
        }
    }

    const HERE: Vec2 = Vec2::new(5.0, 5.0);

    fn lit(lights: &mut Lights, dt: f32, glows: &[Glow], darkness: f32) -> Vec<PointLight> {
        lights.frame(dt, glows.iter().copied(), HERE, 40.0, darkness)
    }

    #[test]
    fn a_fire_gives_no_light_by_day_and_a_lamp_follows_what_it_was_given() {
        let mut lights = Lights::default();
        assert!(lit(&mut lights, 0.016, &[glow(5.0, 5.0)], 0.0).is_empty());
        assert_eq!(lit(&mut lights, 0.016, &[glow(5.0, 5.0)], 1.0).len(), 1);
        // A lamp is already dimmed by dusk when it is reported: it is not
        // dimmed again, and it does not need the dark to be on.
        let mut lamps = Lights::default();
        let lamp = Glow {
            needs_dark: false,
            strength: 0.5,
            ..glow(5.0, 5.0)
        };
        let half = lit(&mut lamps, 0.0, &[lamp], 0.2);
        let full = lit(&mut Lights::default(), 0.0, &[glow(5.0, 5.0)], 1.0);
        assert!((half[0].color[0] / full[0].color[0] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn a_light_stays_a_moment_after_its_last_report_then_fades_out() {
        let mut lights = Lights::default();
        let first = lit(&mut lights, 0.0, &[glow(5.0, 5.0)], 1.0);
        let later = lit(&mut lights, 0.3, &[], 1.0);
        assert_eq!(later.len(), 1);
        assert!(
            later[0].color[0] < first[0].color[0],
            "dimming as it goes quiet"
        );
        assert!(
            lit(&mut lights, 0.3, &[], 1.0).is_empty(),
            "gone after half a second"
        );
    }

    #[test]
    fn a_light_reported_this_frame_is_full_strength_however_long_the_frame() {
        let mut lights = Lights::default();
        let first = lit(&mut lights, 0.0, &[glow(5.0, 5.0)], 1.0);
        // A hitch, or four times speed: the effects time between frames
        // is longer than a light is kept, but this frame reported it.
        let after = lit(&mut lights, 0.8, &[glow(5.0, 5.0)], 1.0);
        assert_eq!(after.len(), 1);
        assert!((after[0].color[0] - first[0].color[0]).abs() < 1e-5);
    }

    #[test]
    fn a_light_reported_again_is_one_light_not_two() {
        let mut lights = Lights::default();
        for _ in 0..10 {
            lit(&mut lights, 0.05, &[glow(5.0, 5.0), glow(5.1, 5.0)], 1.0);
        }
        assert_eq!(lights.items.len(), 1);
        lit(&mut lights, 0.0, &[glow(15.0, 5.0)], 1.0);
        assert_eq!(lights.items.len(), 2);
    }

    #[test]
    fn near_glows_in_one_frame_keep_the_brightest_not_the_last() {
        let mut lights = Lights::default();
        let big = Glow {
            strength: 1.4,
            radius: 6.0,
            ..glow(5.0, 5.0)
        };
        let small = Glow {
            strength: 0.3,
            radius: 2.0,
            ..glow(5.2, 5.0)
        };
        for order in [[big, small], [small, big]] {
            let mut lights = Lights::default();
            let sent = lit(&mut lights, 0.0, &order, 1.0);
            assert_eq!(sent.len(), 1);
            assert_eq!(sent[0].radius, 6.0);
            assert_eq!(sent[0].pos, big.pos);
        }
        // Next frame the old fire is not held up by the old brighter report.
        lit(&mut lights, 0.0, &[big], 1.0);
        let calmer = lit(&mut lights, 0.1, &[small], 1.0);
        assert_eq!(calmer[0].radius, 2.0);
    }

    #[test]
    fn a_building_falling_is_one_fire_not_a_dozen() {
        // Flames thrown at random points over a footprint, a dozen a frame.
        let mut lights = Lights::default();
        for f in 0..20 {
            let glows: Vec<Glow> = (0..12)
                .map(|k| {
                    let a = (f * 12 + k) as f32 * 2.399;
                    Glow {
                        radius: 5.3,
                        strength: 1.4,
                        ..glow(10.0 + a.cos() * 1.2, 10.0 + a.sin() * 1.2)
                    }
                })
                .collect();
            lights.frame(0.07, glows, HERE, 40.0, 1.0);
        }
        assert!(lights.items.len() <= 2, "{}", lights.items.len());
    }

    #[test]
    fn the_biggest_nearest_lights_are_sent_and_the_last_few_fade_in_rank() {
        let mut lights = Lights::default();
        let glows: Vec<Glow> = (0..100).map(|i| glow(i as f32 * 2.0, 0.0)).collect();
        let sent = lights.frame(0.0, glows, Vec2::new(100.0, 0.0), 40.0, 1.0);
        assert_eq!(sent.len(), MAX_LIGHTS);
        for l in &sent {
            assert!(
                (l.pos.x - 100.0).abs() <= MAX_LIGHTS as f32 + 2.0,
                "{}",
                l.pos.x
            );
        }
        let peak = sent.iter().map(|l| l.color[0]).fold(0.0, f32::max);
        let weakest = sent.iter().map(|l| l.color[0]).fold(f32::MAX, f32::min);
        assert!(
            weakest < peak * 0.4,
            "the last ranks are faded: {weakest} {peak}"
        );
        // A big fire a little farther off beats a dim lamp at the middle.
        let mut lights = Lights::default();
        let fire = Glow {
            radius: 7.0,
            strength: 1.4,
            ..glow(30.0, 5.0)
        };
        let lamps: Vec<Glow> = (0..30)
            .map(|i| Glow {
                strength: 0.1,
                ..glow(5.0 + i as f32 * 0.7, 5.0)
            })
            .chain([fire])
            .collect();
        let sent = lights.frame(0.0, lamps, HERE, 40.0, 1.0);
        assert!(sent.iter().any(|l| l.radius == 7.0));
    }

    #[test]
    fn lights_are_hdr_colour_and_far_cameras_get_none_with_a_fade_before() {
        let mut lights = Lights::default();
        let sent = lights.frame(0.0, [glow(5.0, 5.0)], HERE, 40.0, 1.0);
        let c = sent[0].color;
        assert!(c[0] > c[1] && c[1] > c[2] && c[2] > 0.0, "{c:?}");
        assert!(
            c[0] > 1.0,
            "a lamp is brighter than sunlit paint's half: {c:?}"
        );
        let high = lit_at(&mut lights, 125.0);
        assert!(high[0].color[0] < c[0] && high[0].color[0] > 0.0);
        assert!(lit_at(&mut lights, 141.0).is_empty());
    }

    fn lit_at(lights: &mut Lights, distance: f32) -> Vec<PointLight> {
        lights.frame(0.0, [glow(5.0, 5.0)], HERE, distance, 1.0)
    }

    #[test]
    fn a_fire_in_the_fog_is_not_a_light() {
        let mut glows = vec![glow(5.5, 5.5), glow(40.5, 7.5), glow(-0.5, 3.0)];
        in_sight(&mut glows, |(x, _)| (0..20).contains(&x));
        assert_eq!(glows.len(), 1);
        assert_eq!(glows[0].pos.x, 5.5);
    }

    #[test]
    fn a_new_match_starts_dark() {
        let mut lights = Lights::default();
        lit(&mut lights, 0.0, &[glow(5.0, 5.0)], 1.0);
        lights.clear();
        assert!(lit(&mut lights, 0.0, &[], 1.0).is_empty());
    }
}
