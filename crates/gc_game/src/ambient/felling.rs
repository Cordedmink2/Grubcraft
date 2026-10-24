//! Clearing land as it looks (docs/34-ambient-life.md, docs/32): workers
//! cut at what they clear, and when the simulation says a tile is cleared
//! (`Event::Cleared`) its trees come down, tipping away from the cutters,
//! to lie a while before they are hauled off, and its bushes are torn up.
//! Each is the scenery model it was, drawn as an [`Extra`] tipped over about
//! its base (the scenery shader's `lean`), turned and swaying as it stood.
//! Each faction has its own way ([`Style`]): the Ember Clans fell and haul,
//! the Concord burns, the Choir shatters, the Brood's Gnawers eat.

use std::collections::VecDeque;

use glam::{Vec2, Vec3};

use crate::effects;
use crate::scenery::{Extra, Stood, facing_of, fall_of, turn_of};

/// A tree takes this long to go over, in seconds; it then lies this long
/// and takes this long to be carried off (shrinking out of sight).
const FALL_SECONDS: f32 = 1.7;
const LIE_SECONDS: f32 = 14.0;
const HAUL_SECONDS: f32 = 2.5;
/// A bush is torn up in this long.
const UPROOT_SECONDS: f32 = 0.6;
/// A burning tree stands this long, chars in the first of it, and crumbles
/// in the last (a bush flares out in `BURN_BUSH`).
const BURN_SECONDS: f32 = 3.6;
const BURN_CHAR: f32 = 1.2;
const BURN_CRUMBLE: f32 = 1.8;
const BURN_BUSH: f32 = 1.2;
/// Trees and bushes alight at once that get flames of their own; the rest
/// only char, so a whole burnt wood does not flood the particles.
const MOST_BLAZING: usize = 48;
/// A tree stiffens to crystal for this long, then bursts (a bush: half).
const SHATTER_SECONDS: f32 = 0.9;
/// A tree is eaten away in this long (a bush: `GNAW_BUSH`).
const GNAW_SECONDS: f32 = 4.5;
const GNAW_BUSH: f32 = 1.6;
/// How much of its colour a tree keeps when charred, 0 to 1, and the pale
/// crystal tint a shattering one takes (the shader doubles a tint).
const CHARRED: f32 = 0.12;
const CRYSTAL: [f32; 3] = [150.0, 235.0, 255.0];
/// The trees and bushes coming down or lying at once; a new one pushes out
/// the oldest.
const MOST_DOWN: usize = 512;
/// A cutter not seen for this long (seconds) has stopped.
const CUTTER_SEEN: f32 = 1.5;
/// How far a tree's fall strays from straight away from its cutter, in
/// 256ths of a turn either way.
const STRAY: f32 = 20.0;
/// How much of a tree's height its crown is from its base when it lies.
const CROWN_REACH: f32 = 0.75;

/// How a faction clears land, as it looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// Fell it and haul it off (Ember Clans, and anyone else).
    #[default]
    Fell,
    /// Set it alight (Carrion Concord).
    Burn,
    /// Crack it into crystal that bursts (Choir).
    Shatter,
    /// Gnawers eat it (Brood).
    Gnaw,
}

impl Style {
    /// How `faction`'s workers clear land (docs/32's faction twists).
    pub fn of(faction: gc_ai::Faction) -> Style {
        match faction {
            gc_ai::Faction::EmberClans => Style::Fell,
            gc_ai::Faction::CarrionConcord => Style::Burn,
            gc_ai::Faction::Choir => Style::Shatter,
            gc_ai::Faction::Brood => Style::Gnaw,
        }
    }
}

/// A regiment of workers seen cutting a tile this frame.
#[derive(Clone, Copy, Debug)]
struct Cutter {
    tile: (i32, i32),
    from: Vec2,
    style: Style,
    seen: f32,
}

/// A tree or bush on its way down, lying, or being taken away.
#[derive(Clone, Copy, Debug)]
struct Down {
    stood: Stood,
    /// Its height as a model, in tiles, before scaling.
    height: f32,
    /// The way it falls on the map, in 256ths of a turn.
    way: u8,
    /// When it began, on the felling's clock.
    born: f32,
    landed: bool,
    style: Style,
}

impl Down {
    /// Seconds since it began.
    fn age(&self, clock: f32) -> f32 {
        clock - self.born
    }

    fn lasts(&self) -> f32 {
        match (self.style, self.stood.tree) {
            (Style::Fell, true) => FALL_SECONDS + LIE_SECONDS + HAUL_SECONDS,
            (Style::Fell, false) => UPROOT_SECONDS,
            (Style::Burn, true) => BURN_SECONDS,
            (Style::Burn, false) => BURN_BUSH,
            (Style::Shatter, true) => SHATTER_SECONDS,
            (Style::Shatter, false) => SHATTER_SECONDS * 0.5,
            (Style::Gnaw, true) => GNAW_SECONDS,
            (Style::Gnaw, false) => GNAW_BUSH,
        }
    }

    /// How tipped over it is (0 to 255), and how big, by its age.
    fn pose(&self, age: f32) -> (u8, f32) {
        let scale = self.stood.scale;
        if age < 0.0 {
            // Not yet begun: it still stands, as the scenery drew it.
            return (0, scale);
        }
        let f = (age / self.lasts()).clamp(0.0, 1.0);
        match self.style {
            Style::Fell => self.pose_fell(age),
            // Stands alight, then slumps and crumbles away.
            Style::Burn => {
                let crumble =
                    ((age - (self.lasts() - BURN_CRUMBLE)) / BURN_CRUMBLE).clamp(0.0, 1.0);
                let crumble = if self.stood.tree { crumble } else { f * f };
                ((crumble * 40.0) as u8, scale * (1.0 - crumble))
            }
            // Whole until it bursts.
            Style::Shatter => (0, scale),
            // Sinks as it is eaten, shivering under the Gnawers.
            Style::Gnaw => {
                let shiver = (age * 23.0).sin() * 6.0 * (1.0 - f);
                ((shiver.abs()) as u8, scale * (1.0 - f * f))
            }
        }
    }

    /// The tint it is drawn in at `age`: as it stood, charring while it
    /// burns and going pale and hard as it turns to crystal.
    fn tint(&self, age: f32) -> [u8; 3] {
        let [r, g, b, _] = self.stood.tint;
        let from = [r, g, b];
        match self.style {
            Style::Burn => {
                let f = (age / BURN_CHAR).clamp(0.0, 1.0);
                from.map(|c| (f32::from(c) * (1.0 - f * (1.0 - CHARRED))) as u8)
            }
            Style::Shatter => {
                let f = (age / self.lasts()).clamp(0.0, 1.0);
                let mut out = from;
                for (k, c) in out.iter_mut().enumerate() {
                    *c = (f32::from(from[k]) * (1.0 - f) + CRYSTAL[k] * f) as u8;
                }
                out
            }
            _ => from,
        }
    }

    fn pose_fell(&self, age: f32) -> (u8, f32) {
        if !self.stood.tree {
            let f = (age / UPROOT_SECONDS).clamp(0.0, 1.0);
            return ((f * 90.0) as u8, self.stood.scale * (1.0 - f * f));
        }
        let over = (age / FALL_SECONDS).clamp(0.0, 1.0);
        // Slow to start, then faster as it goes.
        let mut tip = over.powf(2.2);
        // A small bounce as the crown lands.
        let since = age - FALL_SECONDS;
        if (0.0..0.5).contains(&since) {
            tip -= 0.025 * (since * 12.0).sin().abs() * (1.0 - since * 2.0);
        }
        let haul = ((age - FALL_SECONDS - LIE_SECONDS) / HAUL_SECONDS).clamp(0.0, 1.0);
        (
            (tip.clamp(0.0, 1.0) * 255.0) as u8,
            self.stood.scale * (1.0 - haul),
        )
    }
}

#[derive(Default)]
pub struct Felling {
    clock: f32,
    cutters: Vec<Cutter>,
    down: VecDeque<Down>,
}

impl Felling {
    /// A regiment of workers at `from` is clearing `tile` their way this
    /// frame.
    pub fn cutting(&mut self, tile: (i32, i32), from: Vec2, style: Style) {
        let seen = self.clock;
        match self.cutters.iter_mut().find(|c| c.tile == tile) {
            Some(c) => (c.from, c.style, c.seen) = (from, style, seen),
            None => self.cutters.push(Cutter {
                tile,
                from,
                style,
                seen,
            }),
        }
    }

    /// `tile` was cleared (`burnt` if the simulation burnt it): what stood
    /// on it comes down, the way the workers who cleared it do it. Call it
    /// with [`crate::scenery::Scenery::standing`], before the tile is felled.
    pub fn clear(
        &mut self,
        tile: (i32, i32),
        burnt: bool,
        stood: impl Iterator<Item = Stood>,
        height: impl Fn(&Stood) -> f32,
    ) {
        let centre = Vec2::new(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5);
        let cutter = self.cutters.iter().find(|c| c.tile == tile);
        let style = if burnt {
            Style::Burn
        } else {
            cutter.map_or(Style::Fell, |c| c.style)
        };
        let away = cutter
            .map(|c| (centre - c.from).normalize_or_zero())
            .filter(|d| *d != Vec2::ZERO);
        for s in stood {
            // Straight away from the cutter, give or take; with no one to
            // push it, the way the tree was already turned.
            let way = match away {
                Some(d) => {
                    let wobble = (f32::from(s.tint[3]) / 255.0 - 0.5) * 2.0 * STRAY;
                    turn_of(d).wrapping_add(wobble as i32 as u8)
                }
                None => s.turn,
            };
            if self.down.len() >= MOST_DOWN {
                self.down.pop_front();
            }
            let height = height(&s);
            // Not all at once: a few trees on a tile go over one after
            // another.
            let lag = f32::from(s.tint[3]) / 255.0 * 0.5;
            self.down.push_back(Down {
                stood: s,
                height,
                way,
                born: self.clock + lag,
                landed: false,
                style,
            });
        }
    }

    /// Move on by `dt` seconds of game time: forget cutters who stopped,
    /// raise the dust of what landed, and drop what is gone or has a
    /// building over it now (`covered` says which tiles do). `ground` is
    /// the terrain height.
    pub fn advance(
        &mut self,
        dt: f32,
        fx: &mut effects::Field,
        ground: impl Fn(Vec2) -> f32,
        covered: impl Fn((i32, i32)) -> bool,
    ) {
        self.clock += dt;
        let clock = self.clock;
        self.cutters.retain(|c| clock - c.seen < CUTTER_SEEN);
        let mut blazing = 0;
        // Newest first, so a crowd of fires lights the trees that have only
        // just caught before the ones crumbling.
        for d in self.down.iter_mut().rev() {
            let age = d.age(clock);
            let at = d.stood.at;
            let foot = Vec3::new(at.x, ground(at), at.y);
            let alive = (0.0..d.lasts()).contains(&age);
            match d.style {
                Style::Burn if alive && blazing < MOST_BLAZING => {
                    blazing += 1;
                    // As tall as it is now, as it crumbles.
                    fx.blaze(foot, d.height * d.pose(age).1, dt);
                }
                Style::Gnaw if alive => fx.gnawed(foot, d.height * d.pose(age).1, dt),
                _ => {}
            }
            // The one thing each does when it is over (a felled tree: when
            // it lands, a bush: halfway up).
            let over = match (d.style, d.stood.tree) {
                (Style::Fell, true) => FALL_SECONDS,
                (Style::Fell, false) => UPROOT_SECONDS * 0.5,
                _ => d.lasts(),
            };
            if d.landed || age < over {
                continue;
            }
            d.landed = true;
            let tall = d.height * d.stood.scale;
            match (d.style, d.stood.tree) {
                (Style::Fell, true) => {
                    let at = at + facing_of(d.way) * (tall * CROWN_REACH);
                    fx.tree_down(Vec3::new(at.x, ground(at), at.y), tall);
                }
                (Style::Fell, false) => fx.bush_up(foot),
                (Style::Burn, _) => fx.ash_down(foot, tall),
                (Style::Shatter, _) => fx.shatter(foot, tall),
                (Style::Gnaw, _) => fx.bush_up(foot),
            }
        }
        self.down.retain(|d| {
            let tile = (d.stood.at.x.floor() as i32, d.stood.at.y.floor() as i32);
            d.age(clock) < d.lasts() && !covered(tile)
        });
    }

    /// The scenery to draw for what is down, this frame.
    pub fn extras<'a>(
        &'a self,
        ground: impl Fn(Vec2) -> f32 + 'a,
    ) -> impl Iterator<Item = Extra> + 'a {
        let clock = self.clock;
        self.down.iter().map(move |d| {
            let age = d.age(clock);
            let (lean, scale) = d.pose(age);
            let phase = d.stood.tint[3];
            Extra {
                model: d.stood.model,
                at: Vec3::new(d.stood.at.x, ground(d.stood.at), d.stood.at.y),
                // As it stood, so nothing snaps when it begins to go.
                turn: d.stood.turn,
                scale,
                tint: d.tint(age),
                phase,
                lean,
                fall: fall_of(d.stood.turn, facing_of(d.way)),
            }
        })
    }

    /// Forget everything (a new match).
    pub fn reset(&mut self) {
        *self = Felling::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenery::Model;

    fn tree(at: Vec2) -> Stood {
        Stood {
            model: Model::OakA,
            at,
            turn: 40,
            scale: 1.0,
            tint: [128, 128, 128, 0],
            tree: true,
        }
    }

    #[test]
    fn a_tree_tips_away_from_its_cutter_and_lies_flat() {
        let mut f = Felling::default();
        f.cutting((10, 10), Vec2::new(8.5, 10.5), Style::Fell);
        f.clear(
            (10, 10),
            false,
            [tree(Vec2::new(10.5, 10.5))].into_iter(),
            |_| 4.0,
        );
        // Cut from the west, it goes over toward the east: turn near 0.
        let t = i32::from(f.down[0].way as i8);
        assert!(t.abs() < 30, "turn {t}");
        let mut fx = effects::Field::new(None);
        let ground = |_: Vec2| 0.0;
        let open = |_: (i32, i32)| false;
        let tip = |f: &Felling| f.extras(ground).next().map(|e| (e.lean, e.scale, e.turn));
        // Still standing, turned as it was.
        assert_eq!(tip(&f).unwrap(), (0, 1.0, 40));
        f.advance(FALL_SECONDS * 0.5, &mut fx, ground, open);
        let mid = tip(&f).unwrap().0;
        assert!((1..128).contains(&mid), "{mid}");
        f.advance(FALL_SECONDS, &mut fx, ground, open);
        let (lean, scale, turn) = tip(&f).unwrap();
        assert!(lean > 240 && scale == 1.0 && turn == 40, "{lean} {scale}");
        // Hauled away in the end.
        f.advance(LIE_SECONDS + HAUL_SECONDS, &mut fx, ground, open);
        assert!(f.extras(ground).next().is_none());
    }

    #[test]
    fn it_falls_toward_the_same_place_whatever_way_the_tree_faced() {
        for turn in [0u8, 40, 130, 250] {
            let mut f = Felling::default();
            f.cutting((10, 10), Vec2::new(8.5, 10.5), Style::Fell);
            let mut t = tree(Vec2::new(10.5, 10.5));
            t.turn = turn;
            f.clear((10, 10), false, [t].into_iter(), |_| 4.0);
            let e = f.extras(|_| 0.0).next().unwrap();
            // `fall` is the way in the model's own space: from +z toward
            // +x. Turned by the facing, it is the way on the map.
            let front = facing_of(turn);
            let left = Vec2::new(front.y, -front.x);
            let angle = f32::from(e.fall) / 256.0 * std::f32::consts::TAU;
            let world = left * angle.sin() + front * angle.cos();
            let way = facing_of(f.down[0].way);
            assert!(world.dot(way) > 0.99, "turn {turn}: {world} vs {way}");
        }
    }

    #[test]
    fn a_bush_is_torn_up_quickly() {
        let mut f = Felling::default();
        let mut bush = tree(Vec2::new(3.5, 3.5));
        bush.tree = false;
        f.clear((3, 3), false, [bush].into_iter(), |_| 1.0);
        let mut fx = effects::Field::new(None);
        f.advance(UPROOT_SECONDS * 2.0, &mut fx, |_| 0.0, |_| false);
        assert!(f.down.is_empty());
    }

    #[test]
    fn trunks_under_a_new_building_go() {
        let mut f = Felling::default();
        f.clear(
            (3, 3),
            false,
            [tree(Vec2::new(3.5, 3.5))].into_iter(),
            |_| 4.0,
        );
        let mut fx = effects::Field::new(None);
        f.advance(1.0, &mut fx, |_| 0.0, |t| t == (3, 3));
        assert!(f.down.is_empty());
    }

    #[test]
    fn the_oldest_make_room() {
        let mut f = Felling::default();
        let many = (0..MOST_DOWN + 20).map(|i| tree(Vec2::new(i as f32 * 0.01, 1.0)));
        f.clear((0, 1), false, many, |_| 3.0);
        assert_eq!(f.down.len(), MOST_DOWN);
    }

    fn cleared_by(style: Style, burnt: bool) -> Felling {
        let mut f = Felling::default();
        f.cutting((10, 10), Vec2::new(8.5, 10.5), style);
        f.clear(
            (10, 10),
            burnt,
            [tree(Vec2::new(10.5, 10.5))].into_iter(),
            |_| 4.0,
        );
        f
    }

    #[test]
    fn the_concords_trees_burn_where_they_stand_and_char() {
        let mut f = cleared_by(Style::Burn, true);
        let mut fx = effects::Field::new(None);
        f.advance(BURN_CHAR, &mut fx, |_| 0.0, |_| false);
        let e = f.extras(|_| 0.0).next().unwrap();
        // Upright, black, still whole; then it crumbles to nothing.
        assert_eq!((e.lean, e.scale), (0, 1.0));
        assert!(e.tint[0] < 30, "{:?}", e.tint);
        f.advance(BURN_SECONDS, &mut fx, |_| 0.0, |_| false);
        assert!(f.extras(|_| 0.0).next().is_none());
    }

    #[test]
    fn a_burnt_tile_burns_whoever_cleared_it() {
        let f = cleared_by(Style::Fell, true);
        assert_eq!(f.down[0].style, Style::Burn);
        let f = cleared_by(Style::Gnaw, false);
        assert_eq!(f.down[0].style, Style::Gnaw);
    }

    #[test]
    fn the_choirs_trees_go_pale_and_burst_all_at_once() {
        let mut f = cleared_by(Style::Shatter, false);
        let mut fx = effects::Field::new(None);
        f.advance(SHATTER_SECONDS * 0.9, &mut fx, |_| 0.0, |_| false);
        let e = f.extras(|_| 0.0).next().unwrap();
        assert_eq!((e.lean, e.scale), (0, 1.0));
        assert!(e.tint[2] > 200, "{:?}", e.tint);
        f.advance(SHATTER_SECONDS * 0.2, &mut fx, |_| 0.0, |_| false);
        assert!(f.down.is_empty());
    }

    #[test]
    fn the_broods_trees_sink_as_they_are_eaten() {
        let mut f = cleared_by(Style::Gnaw, false);
        let mut fx = effects::Field::new(None);
        f.advance(GNAW_SECONDS * 0.5, &mut fx, |_| 0.0, |_| false);
        let e = f.extras(|_| 0.0).next().unwrap();
        assert!(e.scale > 0.5 && e.scale < 0.9, "{}", e.scale);
        assert!(e.lean < 10);
        f.advance(GNAW_SECONDS, &mut fx, |_| 0.0, |_| false);
        assert!(f.down.is_empty());
    }
}
