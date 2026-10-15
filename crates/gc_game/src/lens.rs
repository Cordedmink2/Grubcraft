//! The AI lens (`L`): what the computer players are thinking, drawn over
//! the field and listed in the panel (docs/18-ai.md, "Testing, tuning and
//! explaining").
//!
//! On the ground, for every computer player: its front as a dashed line in
//! its team's colour, with a post at an end it has anchored on ground
//! nobody crosses; a square at each regiment's place in its plan, in the
//! colour of the regiment's role; a dashed line from each regiment to the
//! enemy it is going for (bright), or to its place when it is off it
//! (faint); and a magenta plate under its point of attack.

use gc_ai::lens::Lens;
use gc_ai::{Minds, Role};
use gc_render::BoxInstance;
use gc_sim::{PlayerId, RegimentHandle};
use glam::Vec2;

/// Dashes: this long, one every this many tiles.
const DASH: f32 = 0.9;
const DASH_EVERY: f32 = 1.6;
const LINE_WIDTH: f32 = 0.22;
const MARK_HEIGHT: f32 = 0.06;
/// The square at a regiment's place, tiles across.
const PLACE: f32 = 1.1;
/// A regiment further than this from its place has a line to it.
const OFF_PLACE: f32 = 3.0;
/// The post at an anchored end of the front: width and height, tiles.
const POST: [f32; 3] = [0.5, 3.5, 0.5];
const FOCUS: [u8; 3] = [255, 60, 200];
/// Glow: how strongly lens marks light up (the box shader's highlight).
const BRIGHT: u8 = 230;
const FAINT: u8 = 70;
/// Most lens boxes in a frame.
const MAX_BOXES: usize = 3000;
/// Marks grow as the camera pulls back past this distance, so they stay
/// readable, up to four times their size.
const NEAR: f32 = 80.0;

/// How much bigger to draw the marks with the camera `distance` away.
pub fn scale_for(distance: f32) -> f32 {
    (distance / NEAR).clamp(1.0, 4.0)
}

/// The colour of a role, as the panel's legend names it.
pub fn role_color(role: Role) -> [u8; 3] {
    match role {
        Role::Anchor => [235, 235, 225],
        Role::Wing => [110, 185, 255],
        Role::Hammer => [255, 150, 40],
        Role::Screen => [130, 225, 110],
        Role::Battery => [250, 220, 70],
        Role::Reserve => [185, 120, 255],
        Role::Hunter => [255, 70, 60],
        Role::Recover => [150, 150, 150],
        Role::Ambusher => [95, 150, 70],
    }
}

pub const LEGEND: &str = "AI LENS (L hides)   dashes: to its target, bright, or its place, faint   \
squares: places   posts: anchored ends   magenta: point of attack\n\
  anchor white  wing blue  hammer orange  screen green  battery yellow  \
reserve violet  hunter red  recovering grey  ambusher moss";

/// Every computer player's lens.
pub fn lenses(minds: &Minds) -> Vec<Lens> {
    minds.iter().map(|m| m.lens(minds.view())).collect()
}

/// Draw `lenses` on the ground, marks `scale` times their size. `team`
/// gives a player's colour; only regiments the player `sees` are marked,
/// so the lens gives away nobody hiding in the woods.
pub fn push(
    lenses: &[Lens],
    minds: &Minds,
    scale: f32,
    team: impl Fn(PlayerId) -> [u8; 3],
    sees: impl Fn(RegimentHandle) -> bool,
    out: &mut Vec<BoxInstance>,
) {
    let full = minds.view();
    let get = |h: RegimentHandle| full.get(h).filter(|_| sees(h));
    let start = out.len();
    let room = |out: &Vec<BoxInstance>| out.len() - start < MAX_BOXES;
    for lens in lenses {
        if let Some(f) = lens.front {
            let right = Vec2::new(-f.forward.y, f.forward.x);
            let half = right * (f.width * 0.5);
            dashes(
                f.centre - half,
                f.centre + half,
                team(lens.party),
                BRIGHT,
                scale,
                out,
            );
            if f.covered != 0.0 {
                out.push(BoxInstance {
                    pos: (f.centre + half * f.covered).to_array(),
                    facing: f.forward.to_array(),
                    size: POST.map(|v| v * scale),
                    color: with_glow(team(lens.party), BRIGHT),
                    lift: 0.0,
                });
            }
        }
        if let Some(r) = lens.focus.and_then(get) {
            out.push(BoxInstance {
                pos: r.pos.to_array(),
                facing: r.facing.to_array(),
                size: [r.width + 1.2, MARK_HEIGHT, r.depth + 1.2],
                color: with_glow(FOCUS, BRIGHT),
                lift: 0.0,
            });
        }
        for m in &lens.marks {
            if !room(out) {
                return;
            }
            let Some(r) = get(m.regiment) else {
                continue;
            };
            let color = role_color(m.role);
            if let Some(place) = m.place {
                out.push(BoxInstance {
                    pos: place.to_array(),
                    facing: facing_of(lens, r.facing).to_array(),
                    size: [PLACE * scale, MARK_HEIGHT, PLACE * scale],
                    color: with_glow(color, BRIGHT),
                    lift: 0.0,
                });
            }
            match (m.target.and_then(get), m.place) {
                (Some(t), _) => dashes(r.pos, t.pos, color, BRIGHT, scale, out),
                (None, Some(place)) if place.distance(r.pos) > OFF_PLACE => {
                    dashes(r.pos, place, color, FAINT, scale, out);
                }
                _ => {}
            }
        }
    }
}

/// The way the lens's front faces, or `fallback` before it has one.
fn facing_of(lens: &Lens, fallback: Vec2) -> Vec2 {
    lens.front.map_or(fallback, |f| f.forward)
}

fn with_glow([r, g, b]: [u8; 3], glow: u8) -> [u8; 4] {
    [r, g, b, glow]
}

/// A dashed line on the ground from `a` to `b`, `scale` times the usual
/// size, each dash draped on the ground where it lies.
fn dashes(a: Vec2, b: Vec2, color: [u8; 3], glow: u8, scale: f32, out: &mut Vec<BoxInstance>) {
    let (dash, every) = (DASH * scale, DASH_EVERY * scale);
    let d = b - a;
    let len = d.length();
    if len < dash {
        return;
    }
    let dir = d / len;
    let n = (len / every) as usize;
    for k in 0..=n {
        let s = (k as f32 * every + dash * 0.5).min(len - dash * 0.5);
        out.push(BoxInstance {
            pos: (a + dir * s).to_array(),
            facing: dir.to_array(),
            size: [LINE_WIDTH * scale, MARK_HEIGHT, dash],
            color: with_glow(color, glow),
            lift: 0.0,
        });
    }
}

/// The panel: the legend, then each computer player's lines under its name.
pub fn panel(lenses: &[Lens], name: impl Fn(PlayerId) -> String) -> String {
    let mut lines = vec![LEGEND.to_string()];
    if lenses.is_empty() {
        lines.push("No computer players. I hands the side you command to one.".to_string());
    }
    for lens in lenses {
        let mut text = lens.text.iter();
        if let Some(first) = text.next() {
            lines.push(format!("{}: {first}", name(lens.party)));
        }
        lines.extend(text.cloned());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashes_cover_the_line_and_stay_on_it() {
        let mut out = Vec::new();
        let (a, b) = (Vec2::new(10.0, 10.0), Vec2::new(10.0, 20.0));
        dashes(a, b, [1, 2, 3], BRIGHT, 1.0, &mut out);
        assert!(out.len() >= 6, "{}", out.len());
        for d in &out {
            assert_eq!(d.pos[0], 10.0);
            assert!((10.0..=20.0).contains(&d.pos[1]));
        }
        // The last dash ends at the far end.
        let last = out.last().unwrap().pos[1];
        assert!((last + DASH * 0.5 - 20.0).abs() < 1e-4, "{last}");
        out.clear();
        dashes(a, a, [1, 2, 3], BRIGHT, 1.0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn every_role_has_its_own_colour() {
        let roles = [
            Role::Anchor,
            Role::Wing,
            Role::Hammer,
            Role::Screen,
            Role::Battery,
            Role::Reserve,
            Role::Hunter,
            Role::Recover,
            Role::Ambusher,
        ];
        for (i, a) in roles.iter().enumerate() {
            for b in &roles[i + 1..] {
                assert_ne!(role_color(*a), role_color(*b), "{a:?} {b:?}");
            }
        }
    }
}
