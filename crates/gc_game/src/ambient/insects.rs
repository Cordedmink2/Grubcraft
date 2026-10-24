//! Insects, stage 7 of docs/34-ambient-life.md: butterflies flit over
//! flower meadows by day, pale moths over wet ground at dusk, and dragonflies
//! dart and hover over the shallows. A picture only: not in the simulation,
//! not saved, not sent, in nobody's way.
//!
//! They keep no state. The map is cut into small cells; a hash of a cell says
//! whether it has a few insects and where their home is, and the effects
//! clock says where each is: every insect goes from one waypoint (a hash
//! again) near its home to the next, so a place keeps the same insects, they
//! are where the clock puts them whichever way the camera has been, and a
//! frame costs a hash and a habitat look-up for each cell near the camera.

use gc_map::Map;
use gc_map::def::Land;
use gc_map::grown::Habitat;
use glam::Vec2;

use super::touches::dusk_of;
use super::wildlife::hash;

/// The sort of insect, for the model and the way of going about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Butterfly,
    Moth,
    Dragonfly,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Butterfly, Kind::Moth, Kind::Dragonfly];

    /// The model's name in the pack.
    pub fn model(self) -> &'static str {
        match self {
            Kind::Butterfly => "Butterfly",
            Kind::Moth => "Moth",
            Kind::Dragonfly => "Dragonfly",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// How fast the wing-beat clip (a second long) is played.
    pub fn beat_rate(self) -> f32 {
        match self {
            Kind::Butterfly => 3.2,
            Kind::Moth => 4.5,
            Kind::Dragonfly => 9.0,
        }
    }

    /// How much bigger than the model one is drawn, so that it reads.
    fn size(self) -> f32 {
        match self {
            Kind::Butterfly | Kind::Moth => 1.6,
            Kind::Dragonfly => 1.8,
        }
    }

    /// The seconds one hop to the next waypoint takes, the least and how
    /// much more at most.
    fn hop(self) -> (f32, f32) {
        match self {
            Kind::Butterfly => (1.3, 1.4),
            Kind::Moth => (1.8, 2.0),
            Kind::Dragonfly => (2.4, 2.4),
        }
    }

    /// How far from its home an insect strays, in tiles.
    fn roam(self) -> f32 {
        match self {
            Kind::Butterfly => 4.0,
            Kind::Moth => 3.0,
            Kind::Dragonfly => 5.0,
        }
    }
}

/// The side of a cell, in tiles.
const CELL: i32 = 8;
/// Insects in a cell that has some, the least and how many more at most.
const SIZE: (u32, u32) = (1, 2);
/// How far from the camera's focus cells are looked at: this at no
/// distance, this much more for each tile of camera distance, at most.
const REACH_NEAR: f32 = 12.0;
const REACH_PER_ZOOM: f32 = 0.9;
const REACH_MOST: f32 = 40.0;
/// How far water is from a dragonfly's home, in tiles, at the most, and how
/// far above the water that home may be.
const SHORE: f32 = 3.0;
const HIGH_BANK: f32 = 2.5;
/// The share of a hop a butterfly or moth rests at the end of (when it rests
/// at all), and of a dragonfly's hop that is the dart (the rest is hover).
const REST: f32 = 0.5;
const DART: f32 = 0.4;
/// The share of a hop it takes a resting insect to settle or to lift off.
/// (It rests on the ground.)
const SETTLE: f32 = 0.15;

/// An insect as it is drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub kind: Kind,
    pub pos: Vec2,
    /// Tiles above the ground the game draws things on (the surface, over
    /// deep water).
    pub alt: f32,
    pub facing: Vec2,
    /// Whether the wings are beating (else it rests, wings up).
    pub beating: bool,
    pub phase: f32,
    pub scale: f32,
}

#[derive(Default)]
pub struct Insects {
    seen: Vec<Seen>,
}

/// Which insects a habitat has, as a chance for each kind.
fn chances(h: Habitat) -> (f32, f32) {
    use Habitat::*;
    let butterfly = match h {
        FlowerMeadow => 0.9,
        Glade | HeatherMoor => 0.5,
        Grassland => 0.35,
        _ => 0.0,
    };
    let moth = match h {
        WetMeadow | SedgeTussock | AlderCarr | PeatBog | FernHollow | Glade => 0.45,
        _ => 0.0,
    };
    (butterfly, moth)
}

/// The habitat of the tile `p` is in, on the map. A map with no habitats (a
/// Conquest map, painted in lands) gets the nearest habitat to its land.
pub(crate) fn habitat_at(map: &Map, p: Vec2) -> Option<Habitat> {
    let tiles = map.tiles as i32;
    let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
    if !(0..tiles).contains(&x) || !(0..tiles).contains(&y) {
        return None;
    }
    let tile = y as usize * tiles as usize + x as usize;
    if let Some(&b) = map.habitat.get(tile) {
        return Some(Habitat::from_u8(b));
    }
    Some(match map.land_at(tile).unwrap_or(Land::Meadow) {
        Land::Meadow => Habitat::Grassland,
        Land::Valley => Habitat::FlowerMeadow,
        Land::Fen => Habitat::WetMeadow,
        Land::Highland => Habitat::HeatherMoor,
        Land::Forest => Habitat::OldGrowth,
        Land::Waste => Habitat::CrackedFlat,
    })
}

/// Whether there is water within `SHORE` tiles of `p`, and `p` is not far
/// above it (a lake, a river or a pool, not a mountain stream).
fn near_water(map: &Map, p: Vec2) -> bool {
    let tiles = map.tiles as f32;
    let low = map.water_level();
    let wet = |q: Vec2| {
        q.x > 1.0
            && q.y > 1.0
            && q.x < tiles - 1.0
            && q.y < tiles - 1.0
            && map.height_at(q.x, q.y) < low
    };
    map.height_at(p.x, p.y) < low + HIGH_BANK
        && (wet(p)
            || [SHORE * 0.5, SHORE].iter().any(|&r| {
                (0..8)
                    .any(|k| wet(p + Vec2::from_angle(k as f32 * std::f32::consts::FRAC_PI_4) * r))
            }))
}

/// A waypoint of an insect: where it hops to after `hop` hops, near `home`.
fn waypoint(home: Vec2, roam: f32, seed: u32, hop: i64) -> Vec2 {
    let salt = seed.wrapping_add((hop as u32).wrapping_mul(0x9e37_79b9));
    let a = hash((hop as i32, 5), salt) * std::f32::consts::TAU;
    let r = hash((hop as i32, 6), salt).sqrt() * roam;
    home + Vec2::from_angle(a) * r
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Where the insect `seed` of `kind` homed at `home` is at `time` seconds:
/// its place, the way it faces, how high it is above its ground and whether
/// it is beating its wings. A butterfly or moth only rests where `ground`
/// is dry (a `home` away from water): it would settle on the water else.
fn flit(kind: Kind, home: Vec2, seed: u32, time: f32, dry: bool) -> (Vec2, Vec2, f32, bool) {
    let (lo, more) = kind.hop();
    let length = lo + more * hash((seed as i32, 1), seed);
    let t = time / length + hash((seed as i32, 2), seed) * 64.0;
    let j = t.floor();
    let frac = t - j;
    let j = j as i64;
    let roam = kind.roam();
    let (a, b, c) = (
        waypoint(home, roam, seed, j),
        waypoint(home, roam, seed, j + 1),
        waypoint(home, roam, seed, j + 2),
    );
    let way = |p: Vec2, q: Vec2| (q - p).normalize_or(Vec2::X);
    let (now, next) = (way(a, b), way(b, c));
    // How far along the hop it has got: a dragonfly darts then hovers; a
    // butterfly goes the whole hop, and now and then rests at the end of it.
    // `rests(h)`: whether hop `h` ends with a rest on the ground.
    let rests =
        |h: i64| dry && kind != Kind::Dragonfly && hash((seed as i32, 3), (h as u32) ^ seed) < 0.3;
    // A resting hop goes the first half of it and then sits.
    let glide = if rests(j) { 1.0 - REST } else { 1.0 };
    let go = match kind {
        Kind::Dragonfly => smooth(frac / DART),
        _ => smooth(frac / glide),
    };
    let pos = a.lerp(b, go);
    // Turn toward the next hop's way over the end of the glide, so that a
    // resting one is settled facing the way it will go.
    // Turned by the angle, the short way (blending the two ways would pass
    // through nothing when they are opposite).
    let (turn_from, turn_by) = (now.to_angle(), now.angle_to(next));
    let facing = Vec2::from_angle(turn_from + turn_by * smooth((frac - (glide - 0.3)) / 0.3));
    // A phase of its own for the bob (a big seed would add a jump).
    let phase = hash((seed as i32, 4), seed) * std::f32::consts::TAU;
    let bob = match kind {
        Kind::Dragonfly => 0.7 + 0.2 * (time * 1.7 + phase).sin(),
        _ => (0.45 + 0.3 * (time * 2.3 + phase).sin() + 0.2 * (frac * std::f32::consts::TAU).sin())
            .max(0.15),
    };
    // It comes down to rest over the end of the glide and lifts off at the
    // start of the next hop.
    let mut alt = bob;
    if rests(j - 1) {
        alt *= smooth(frac / SETTLE);
    }
    let down = (frac - glide) / SETTLE;
    if rests(j) {
        alt *= 1.0 - smooth(down);
    }
    let resting = rests(j) && down >= 1.0;
    (pos, facing, alt, !resting)
}

impl Insects {
    /// Draw no insects (the game is behind a menu), keeping nothing.
    pub fn hush(&mut self) {
        self.seen.clear();
    }

    /// The insects to draw, as of the last `advance`.
    pub fn seen(&self) -> &[Seen] {
        &self.seen
    }

    /// Gather the insects near the camera's `focus` (`distance` away) at
    /// effects `time` seconds, for a sun at `sun` (its y, normalised) and a
    /// Graphics preset's `density` (1 shows them all, 0 none).
    pub fn advance(
        &mut self,
        time: f32,
        focus: Vec2,
        distance: f32,
        sun: f32,
        density: f32,
        map: &Map,
    ) {
        self.seen.clear();
        if density <= 0.0 {
            return;
        }
        let dusk = dusk_of(sun);
        let day = 1.0 - dusk;
        let reach = (REACH_NEAR + distance * REACH_PER_ZOOM).min(REACH_MOST);
        let cell = CELL as f32;
        let lo = ((focus - Vec2::splat(reach)) / cell).floor().as_ivec2();
        let hi = ((focus + Vec2::splat(reach)) / cell).floor().as_ivec2();
        let tiles = map.tiles as f32;
        for cx in lo.x..=hi.x {
            for cy in lo.y..=hi.y {
                let c = (cx, cy);
                let home = Vec2::new(
                    (cx as f32 + 0.15 + 0.7 * hash(c, 11)) * cell,
                    (cy as f32 + 0.15 + 0.7 * hash(c, 12)) * cell,
                );
                if home.distance(focus) > reach
                    || home.x < 1.0
                    || home.y < 1.0
                    || home.x > tiles - 1.0
                    || home.y > tiles - 1.0
                {
                    continue;
                }
                let Some(habitat) = habitat_at(map, home) else {
                    continue;
                };
                // The kind is the cell's whatever the light, so that dusk
                // fades one in and out and never swaps it for another.
                let (b, m) = chances(habitat);
                let wet = near_water(map, home);
                let kind = if hash(c, 13) < b {
                    Kind::Butterfly
                } else if hash(c, 16) < m {
                    Kind::Moth
                } else if wet && hash(c, 14) < 0.6 {
                    Kind::Dragonfly
                } else {
                    continue;
                };
                let fade = if kind == Kind::Moth { dusk } else { day };
                if fade < 0.02 {
                    continue;
                }
                let n = SIZE.0 + (hash(c, 15) * (SIZE.1 - SIZE.0 + 1) as f32) as u32;
                for k in 0..n.min(SIZE.1) {
                    let seed = (cx as u32).wrapping_mul(7919)
                        ^ (cy as u32).wrapping_mul(104_729)
                        ^ (k + 1).wrapping_mul(0x85eb_ca6b);
                    if hash(c, 20 + k) >= density {
                        continue;
                    }
                    let (pos, facing, fly, beating) = flit(kind, home, seed, time, !wet);
                    self.seen.push(Seen {
                        kind,
                        pos,
                        alt: fly,
                        facing,
                        beating,
                        phase: hash(c, 30 + k) * 8.0,
                        scale: kind.size() * fade,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ambient::hamlets::tests::flat;

    const NOON: f32 = 0.9;
    const NIGHT: f32 = 0.0;

    /// A meadow of flowers with a pond in one corner.
    fn meadow() -> Map {
        let (mut map, _) = flat();
        let n = map.tiles as usize;
        map.habitat = vec![Habitat::FlowerMeadow as u8; n * n];
        map
    }

    fn all(map: &Map, time: f32, sun: f32) -> Vec<Seen> {
        let mut i = Insects::default();
        let at = Vec2::splat(map.tiles as f32 / 2.0);
        i.advance(time, at, 30.0, sun, 1.0, map);
        i.seen().to_vec()
    }

    #[test]
    fn flower_meadows_have_butterflies_by_day_and_none_at_night() {
        let map = meadow();
        let day = all(&map, 5.0, NOON);
        assert!(day.len() > 5, "{}", day.len());
        assert!(day.iter().all(|s| s.kind == Kind::Butterfly));
        assert!(day.iter().all(|s| s.scale > 0.0));
        assert!(all(&map, 5.0, NIGHT).is_empty());
    }

    #[test]
    fn a_map_with_lands_and_no_habitats_gets_the_nearest() {
        let (mut map, _) = flat();
        assert!(map.habitat.is_empty());
        let n = map.tiles as usize;
        map.land = vec![Land::Valley as u8; n * n];
        let valley = all(&map, 5.0, NOON);
        assert!(valley.iter().any(|s| s.kind == Kind::Butterfly));
        map.land = vec![Land::Fen as u8; n * n];
        assert!(all(&map, 5.0, NIGHT).iter().any(|s| s.kind == Kind::Moth));
        map.land = vec![Land::Waste as u8; n * n];
        assert!(all(&map, 5.0, NOON).is_empty());
    }

    #[test]
    fn wet_ground_has_moths_at_dusk_only() {
        let mut map = meadow();
        let n = map.tiles as usize;
        map.habitat = vec![Habitat::WetMeadow as u8; n * n];
        let night = all(&map, 5.0, NIGHT);
        assert!(night.len() > 3);
        assert!(night.iter().all(|s| s.kind == Kind::Moth));
        assert!(all(&map, 5.0, NOON).is_empty());
    }

    #[test]
    fn dragonflies_are_over_and_above_water_by_day() {
        let mut map = meadow();
        // Sink a pond into the middle of the map.
        let n = map.tiles as usize;
        let c = n / 2;
        let side = map.side() as usize;
        let per = (side - 1) / n;
        for y in (c - 12) * per..(c + 12) * per {
            for x in (c - 12) * per..(c + 12) * per {
                map.samples[y * side + x] = 0;
            }
        }
        map.habitat = vec![Habitat::BareRock as u8; n * n];
        let seen = all(&map, 9.0, NOON);
        let flies: Vec<&Seen> = seen.iter().filter(|s| s.kind == Kind::Dragonfly).collect();
        assert!(!flies.is_empty());
        // They keep to the pond: over it or on the bank, a roam and a shore
        // from its edge at the most.
        let (centre, edge) = (c as f32, 12.0 + Kind::Dragonfly.roam() + SHORE);
        for s in flies {
            assert!(s.alt >= 0.5, "{s:?}");
            assert!(
                (s.pos - Vec2::splat(centre)).abs().max_element() <= edge,
                "{s:?}"
            );
        }
        assert!(all(&map, 9.0, NIGHT).is_empty());
    }

    #[test]
    fn they_are_where_the_clock_says_and_the_same_every_time() {
        let map = meadow();
        assert_eq!(all(&map, 7.25, NOON), all(&map, 7.25, NOON));
        assert_ne!(all(&map, 7.25, NOON), all(&map, 7.75, NOON));
    }

    #[test]
    fn they_stay_near_home_and_move_smoothly() {
        let home = Vec2::new(50.0, 50.0);
        // Seeds as `advance` makes them, big ones among them.
        for seed in [
            77,
            0xdead_beef,
            0x85eb_ca6b ^ 7919 ^ (104_729 * 31),
            u32::MAX,
        ] {
            for kind in Kind::ALL {
                let step = 0.02;
                let (mut last, mut last_face, mut last_alt, _) = flit(kind, home, seed, 0.0, true);
                let mut t = 0.0;
                let mut fastest = 0.0f32;
                while t < 60.0 {
                    t += step;
                    let (p, f, alt, _) = flit(kind, home, seed, t, true);
                    assert!(p.distance(home) <= kind.roam() + 1e-3);
                    assert!((f.length() - 1.0).abs() < 1e-3);
                    assert!((0.0..2.0).contains(&alt));
                    fastest = fastest.max(p.distance(last) / step);
                    // Nor do they drop or jump in the air, nor snap round.
                    assert!(
                        (alt - last_alt).abs() / step < 6.0,
                        "{kind:?} {seed} jumps at {t}"
                    );
                    assert!(f.dot(last_face) > 0.9, "{kind:?} {seed} snaps round at {t}");
                    last = p;
                    last_face = f;
                    last_alt = alt;
                }
                // Darts are fast, but nobody teleports.
                assert!(fastest < 16.0, "{kind:?} reaches {fastest} tiles a second");
            }
        }
    }

    #[test]
    fn some_rest_and_the_rest_beat_their_wings() {
        let home = Vec2::new(50.0, 50.0);
        let beating: Vec<bool> = (0..4000)
            .map(|i| flit(Kind::Butterfly, home, 5, i as f32 * 0.05, true).3)
            .collect();
        let resting = beating.iter().filter(|b| !**b).count();
        assert!(resting > 100 && resting < 2500, "{resting}");
        assert!(
            (0..4000).all(|i| flit(Kind::Dragonfly, home, 5, i as f32 * 0.05, true).3),
            "dragonflies hover"
        );
        // Nor do they sit down on water.
        assert!(
            (0..4000).all(|i| flit(Kind::Butterfly, home, 5, i as f32 * 0.05, false).3),
            "wet ground, no rests"
        );
    }

    #[test]
    fn a_resting_one_sits_on_the_ground_facing_the_way_it_goes() {
        let home = Vec2::new(50.0, 50.0);
        let mut sat = 0;
        for i in 0..4000 {
            let t = i as f32 * 0.05;
            let (p, f, alt, beating) = flit(Kind::Butterfly, home, 5, t, true);
            if !beating {
                sat += 1;
                assert_eq!(alt, 0.0);
                // Still, and settled in the turn before the next hop.
                let (q, g, _, _) = flit(Kind::Butterfly, home, 5, t + 0.01, true);
                assert!(p.distance(q) < 1e-3 && f.dot(g) > 0.999, "{t}");
            }
        }
        assert!(sat > 100);
    }

    #[test]
    fn density_thins_them_and_zero_shows_none() {
        let map = meadow();
        let mut i = Insects::default();
        let at = Vec2::splat(map.tiles as f32 / 2.0);
        i.advance(3.0, at, 30.0, NOON, 1.0, &map);
        let full = i.seen().len();
        i.advance(3.0, at, 30.0, NOON, 0.3, &map);
        let thin = i.seen().len();
        assert!(thin < full && thin > 0, "{thin} of {full}");
        i.advance(3.0, at, 30.0, NOON, 0.0, &map);
        assert!(i.seen().is_empty());
        i.hush();
        assert!(i.seen().is_empty());
    }

    #[test]
    fn dusk_fades_the_same_insects_in_and_out_and_never_swaps_them() {
        let mut map = meadow();
        let n = map.tiles as usize;
        map.habitat = vec![Habitat::Glade as u8; n * n];
        let mut kinds = std::collections::HashMap::new();
        let mut halfway = false;
        for step in 0..=20 {
            let sun = step as f32 * 0.05;
            for s in all(&map, 4.0, sun) {
                // The same place at the same time is the same insect.
                let key = (s.pos.x.to_bits(), s.pos.y.to_bits());
                assert_eq!(
                    *kinds.entry(key).or_insert(s.kind),
                    s.kind,
                    "swapped at {sun}"
                );
                halfway |= s.scale > 0.0 && s.scale < s.kind.size() - 0.05;
            }
        }
        assert!(halfway, "nothing faded");
        assert!(kinds.values().any(|k| *k == Kind::Butterfly));
        assert!(kinds.values().any(|k| *k == Kind::Moth));
    }

    #[test]
    fn a_map_with_no_lands_either_is_meadow() {
        let (map, _) = flat();
        assert!(map.habitat.is_empty() && map.land.is_empty());
        assert!(!all(&map, 5.0, NOON).is_empty());
    }

    #[test]
    fn only_cells_near_the_camera_are_looked_at() {
        let map = meadow();
        let mut i = Insects::default();
        i.advance(3.0, Vec2::splat(-1.0e6), 30.0, NOON, 1.0, &map);
        assert!(i.seen().is_empty());
        let at = Vec2::splat(map.tiles as f32 / 2.0);
        i.advance(3.0, at, 30.0, NOON, 1.0, &map);
        let reach = (REACH_NEAR + 30.0 * REACH_PER_ZOOM).min(REACH_MOST);
        assert!(
            i.seen()
                .iter()
                .all(|s| s.pos.distance(at) <= reach + 5.0 + 1.0)
        );
    }
}
