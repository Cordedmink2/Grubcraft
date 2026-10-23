//! What a siege looks like on screen (docs/21-buildings.md, "Sieges on
//! screen"): the blows the simulation tells (`Event::Struck`) shake the
//! building and knock chips and dust off where they land; archers' arrows
//! and siege engines' shots fly to it; a damaged building smokes, then
//! burns; a building brought down sinks in a cloud of dust and leaves
//! rubble.
//!
//! Only a picture. Every blow drawn is one the simulation struck, but where
//! the chips fly, how a building shakes and how long rubble lies is chosen
//! here and changes nothing in the match. Smoke, dust and flames are boxes
//! that cast no shadow (a negative height, see `gc_render::BoxInstance`).

use std::f32::consts::{PI, TAU};

use gc_data::Faction;
use gc_render::BoxInstance;
use gc_sim::structures::{Armour, Breaker};
use gc_sim::{PlayerId, RegimentHandle, StructureHandle};
use glam::Vec2;

use crate::field::{ARROW_COLOR, ARROW_SIZE, ARROWS_PER_VOLLEY};

/// Chips, dust puffs and shots in the air at once, at most: when full, a
/// new one takes the place of one at random.
const MAX_CHIPS: usize = 1536;
const MAX_PUFFS: usize = 1024;
const MAX_SHOTS: usize = 256;
/// Fallen buildings sinking or lying in rubble at once, at most.
const MAX_FALLS: usize = 48;
/// Wall points kept per regiment, where its soldiers last swung.
const TOUCHES: usize = 6;

/// Pull of the ground on chips, in tiles a second a second (a soldier is
/// under a tile tall).
const GRAVITY: f32 = 6.0;
/// A chip lies this long once down, then sinks out of sight over the last
/// [`CHIP_SINK`] seconds.
const CHIP_LIFE: f32 = 7.0;
const CHIP_SINK: f32 = 1.5;
/// A dust puff lasts this long.
const PUFF_LIFE: f32 = 1.4;
/// A building jolted by a blow settles over this long.
const JOLT_SECONDS: f32 = 0.35;
/// A health bar shows white this long after a blow.
const FLASH_SECONDS: f32 = 0.14;
/// A building brought down takes this long to sink.
const COLLAPSE_SECONDS: f32 = 2.4;
/// Its rubble lies this long, smoulders for the first part of it, and
/// sinks away over the last part.
const RUBBLE_SECONDS: f32 = 90.0;
const SMOULDER_SECONDS: f32 = 25.0;
const RUBBLE_SINK: f32 = 6.0;
/// A regiment that loosed at a building this recently is still shooting.
const SHOOTING_SECONDS: f32 = 1.2;
/// Smoke trailed by a fire-pot in flight, and dust thrown up by a building
/// going down, in puffs a second.
const TRAIL_RATE: f32 = 48.0;
const COLLAPSE_DUST_RATE: f32 = 36.0;
/// Sounds kept for the game to play at most (more at once are lost in the
/// din, and none pile up while nothing plays them).
const MAX_NOISES: usize = 32;
/// How fast soldiers close up to the wall once at a siege, as a share a
/// second; how near it they stand to swing, and how far at most they step
/// up to it on screen.
const LEAN_RATE: f32 = 2.5;
const WALL_GAP: f32 = 0.5;
const WALL_STEP: f32 = 2.4;
/// Below these shares of its hit points a building smokes, and burns.
const SMOKES_BELOW: f32 = 0.7;
const BURNS_BELOW: f32 = 0.35;
/// Smoke puffs per column, and how long each takes to rise and fade.
const SMOKE_PUFFS: usize = 9;
const SMOKE_LIFE: f32 = 3.2;
/// Flames per fire.
const FLAMES: usize = 4;
/// A smoke column's lean with the wind, in tiles a second.
const WIND: Vec2 = Vec2::new(0.35, 0.18);
/// Arrows leave the bow this high (as `ARROW_HEIGHT` in the field).
const RELEASE_HEIGHT: f32 = crate::ARROW_HEIGHT;

/// What a blow on a building is, from the unit that struck it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Blades, clubs and claws on the wall.
    Blade,
    /// A ram, or a siege engine at close quarters.
    Ram,
    /// A volley of arrows.
    Arrows,
    /// A siege engine's stone.
    Boulder,
    /// Rockets and fire-pots: a burning shot.
    Fire,
}

/// How hard a blow lands.
struct Force {
    chips: usize,
    puffs: usize,
    /// How far it jolts the building, in tiles.
    jolt: f32,
    /// Heavy: bigger chips and dust.
    big: bool,
}

impl Kind {
    /// The blow of a unit that shoots (`ranged`) or not, of breaker class
    /// `breaker`.
    pub fn of(ranged: bool, breaker: Breaker) -> Kind {
        match (ranged, breaker) {
            (false, Breaker::Siege | Breaker::Artillery) => Kind::Ram,
            (false, _) => Kind::Blade,
            (true, Breaker::Siege) => Kind::Boulder,
            (true, Breaker::Artillery) => Kind::Fire,
            (true, _) => Kind::Arrows,
        }
    }

    /// Whether it flies to the building.
    pub fn shot(self) -> bool {
        matches!(self, Kind::Arrows | Kind::Boulder | Kind::Fire)
    }

    fn force(self) -> Force {
        let (chips, puffs, jolt) = match self {
            Kind::Blade => (2, 1, 0.025),
            Kind::Arrows => (1, 0, 0.012),
            Kind::Ram => (9, 3, 0.11),
            Kind::Fire => (8, 3, 0.1),
            Kind::Boulder => (14, 5, 0.17),
        };
        Force {
            chips,
            puffs,
            jolt,
            big: !matches!(self, Kind::Blade | Kind::Arrows),
        }
    }

    /// How fast its shot flies, in tiles a second, and how high it arcs
    /// for the length of its flight (arrows at a wall fly flatter than a
    /// volley dropped on a regiment).
    fn flight(self) -> (f32, f32) {
        match self {
            Kind::Arrows => (22.0, 0.12),
            _ => (13.0, 0.32),
        }
    }

    /// Shots drawn per blow.
    fn count(self) -> usize {
        if self == Kind::Arrows {
            ARROWS_PER_VOLLEY
        } else {
            1
        }
    }

    /// What it sounds like landing.
    fn sound(self) -> Sound {
        match self {
            Kind::Blade => Sound::WallHit,
            Kind::Arrows => Sound::Arrows,
            _ => Sound::Impact,
        }
    }
}

/// What a building is made of, for its chips, dust and rubble.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stuff {
    /// Chip and rubble colours, picked among, with their glow (the Choir's
    /// crystal shines).
    pub colours: [[u8; 4]; 3],
    pub dust: [u8; 4],
}

/// What a building of `faction` with `armour` is made of.
pub fn stuff(faction: Option<Faction>, armour: Armour) -> Stuff {
    let wood = [116, 82, 52, 0];
    let stone = [128, 124, 116, 0];
    let dust = [168, 156, 136, 0];
    match (faction, armour) {
        (Some(Faction::Brood), _) | (_, Armour::Living) => Stuff {
            colours: [[92, 58, 70, 0], [128, 96, 64, 0], [70, 48, 60, 0]],
            dust: [120, 104, 84, 0],
        },
        (Some(Faction::Choir), _) => Stuff {
            colours: [[150, 190, 214, 70], [196, 216, 230, 70], stone],
            dust: [190, 200, 206, 0],
        },
        (Some(Faction::CarrionConcord), _) => Stuff {
            colours: [[214, 204, 178, 0], [96, 92, 86, 0], wood],
            dust: [150, 146, 136, 0],
        },
        (_, Armour::Fortified) => Stuff {
            colours: [stone, [104, 100, 94, 0], [150, 146, 136, 0]],
            dust,
        },
        _ => Stuff {
            colours: [wood, stone, [176, 160, 132, 0]],
            dust,
        },
    }
}

/// A building as it was last drawn: what a blow on it, or its fall, is
/// shown against.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub handle: StructureHandle,
    /// The corners of its walls on the map, in tiles.
    pub min: Vec2,
    pub max: Vec2,
    /// The height of the ground its model stands on, and the model's top
    /// above that.
    pub base: f32,
    pub top: f32,
    /// Its model, if it has one, drawn this high over the ground at the
    /// middle.
    pub model: Option<usize>,
    pub lift: f32,
    pub owner: PlayerId,
    pub colour: [u8; 3],
    pub stuff: Stuff,
    /// Share of its hit points left, as the damage shows (1 while it goes
    /// up: it has few hit points yet, and doesn't smoke for it).
    pub hp: f32,
}

impl Look {
    pub fn centre(&self) -> Vec2 {
        (self.min + self.max) / 2.0
    }

    /// The nearest point of the walls to `p` (from outside).
    pub fn nearest(&self, p: Vec2) -> Vec2 {
        p.clamp(self.min, self.max)
    }

    /// How far `p` is from the walls (0 inside).
    pub fn gap(&self, p: Vec2) -> f32 {
        p.distance(self.nearest(p))
    }

    fn size(&self) -> Vec2 {
        self.max - self.min
    }
}

/// Who struck a blow, as the screen shows them.
#[derive(Clone, Copy, Debug)]
pub struct Attacker {
    pub handle: RegimentHandle,
    pub at: Vec2,
    /// The height of the ground it stands on.
    pub ground: f32,
    pub kind: Kind,
    /// Width of its front, in tiles.
    pub frontage: f32,
    /// The viewer can see it: its shots are drawn.
    pub seen: bool,
}

/// A sound for the game to play where something happened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Noise {
    pub sound: Sound,
    pub at: Vec2,
}

/// The siege's sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// A blade on a wall.
    WallHit,
    /// Arrows into a wall.
    Arrows,
    /// A siege engine fires.
    Launch,
    /// A heavy shot or a ram lands.
    Impact,
    /// A building comes down.
    Collapse,
}

/// A soldier at a building's wall on screen: where he stands, the way he
/// faces, and the point of the wall he swings at, if he can reach it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stand {
    pub pos: Vec2,
    pub facing: Vec2,
    pub swing: Option<Vec2>,
}

/// A soldier of a melee regiment at `wall`, standing at `pos` as his
/// regiment places him: he steps up to the wall as far as `lean` (0 to 1)
/// has his rank closed, less `draw_back` (a ram between blows), and faces
/// it. The front rank reaches it and swings; those behind wait.
pub fn stand_at(wall: &Look, pos: Vec2, facing: Vec2, lean: f32, draw_back: f32) -> Stand {
    let near = wall.nearest(pos);
    let to = (near - pos).normalize_or(facing);
    let gap = pos.distance(near);
    let step = (gap - WALL_GAP).clamp(0.0, WALL_STEP) * lean;
    Stand {
        pos: pos + to * (step - draw_back),
        facing: to,
        swing: (gap - step <= WALL_GAP + 0.4).then_some(near),
    }
}

/// A box that casts no shadow: smoke, dust and flames.
fn shadowless([w, h, d]: [f32; 3]) -> [f32; 3] {
    [w, -h.abs(), d]
}

#[derive(Clone, Copy, Debug)]
struct Chip {
    pos: Vec2,
    y: f32,
    vel: Vec2,
    vy: f32,
    size: f32,
    colour: [u8; 4],
    facing: Vec2,
    spin: f32,
    /// When it came to rest, once it has.
    rest: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
struct Puff {
    pos: Vec2,
    y: f32,
    vel: Vec2,
    rise: f32,
    size: f32,
    colour: [u8; 4],
    facing: Vec2,
    born: f32,
    life: f32,
}

#[derive(Clone, Copy, Debug)]
struct Shot {
    structure: StructureHandle,
    kind: Kind,
    from: Vec2,
    from_y: f32,
    to: Vec2,
    to_y: f32,
    /// From `from` to `to`, and across it, for spreading a volley.
    dir: Vec2,
    born: f32,
    flight: f32,
    seed: u32,
}

impl Shot {
    /// Where arrow (or stone) `k` is at `t` (0 to 1) of its flight, and how
    /// high.
    fn at(&self, k: usize, t: f32) -> (Vec2, f32) {
        let (spread, rise) = if self.kind == Kind::Arrows {
            let across = Vec2::new(-self.dir.y, self.dir.x);
            (
                across * hash(self.seed, k as u32, 0) * 1.2,
                hash(self.seed, k as u32, 1) * 0.45,
            )
        } else {
            (Vec2::ZERO, 0.0)
        };
        let length = self.from.distance(self.to);
        let (_, arc) = self.kind.flight();
        let pos = self.from.lerp(self.to + spread, t);
        let y = self.from_y + (self.to_y + rise - self.from_y) * t + (t * PI).sin() * length * arc;
        (pos, y)
    }
}

/// A building brought down: sinking, then rubble.
#[derive(Clone, Copy, Debug)]
struct Fall {
    look: Look,
    /// When it starts down: once the shots already on their way to it
    /// have landed.
    born: f32,
    seed: u32,
    /// Whether its burst of chips and its crash have gone off yet.
    burst: bool,
}

/// A fallen building's model, sinking, for the game to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sinking {
    pub model: usize,
    pub pos: Vec2,
    pub lift: f32,
    pub colour: [u8; 3],
}

/// Where a regiment's soldiers swung at a building's wall.
#[derive(Clone, Copy, Debug)]
struct Touches {
    by: RegimentHandle,
    at: StructureHandle,
    points: [Vec2; TOUCHES],
    n: u8,
}

/// A building shaken by the last blow on it.
#[derive(Clone, Copy, Debug)]
struct Jolt {
    handle: StructureHandle,
    dir: Vec2,
    strength: f32,
    at: f32,
}

/// Every siege on the field, as drawn.
pub struct Siege {
    time: f32,
    seed: u32,
    looks: Vec<Option<Look>>,
    jolts: Vec<Jolt>,
    chips: Vec<Chip>,
    puffs: Vec<Puff>,
    shots: Vec<Shot>,
    falls: Vec<Fall>,
    /// By regiment slot: when it last struck a building.
    struck: Vec<Option<(RegimentHandle, f32)>>,
    /// By regiment slot: how far its front rank has closed to the wall.
    lean: Vec<f32>,
    /// By regiment slot: where its soldiers swung at the wall of which
    /// building last frame.
    touches: Vec<Option<Touches>>,
    noises: Vec<Noise>,
}

impl Siege {
    pub fn new(regiments: usize) -> Siege {
        Siege {
            time: 0.0,
            seed: 0x5EED_0001,
            looks: Vec::new(),
            jolts: Vec::new(),
            chips: Vec::with_capacity(MAX_CHIPS),
            puffs: Vec::with_capacity(MAX_PUFFS),
            shots: Vec::with_capacity(MAX_SHOTS),
            falls: Vec::with_capacity(MAX_FALLS),
            struck: vec![None; regiments],
            lean: vec![0.0; regiments],
            touches: vec![None; regiments],
            noises: Vec::new(),
        }
    }

    /// Forget everything (a new match, or a seek).
    pub fn clear(&mut self) {
        let regiments = self.struck.len();
        *self = Siege::new(regiments);
    }

    /// Note how building `look.handle` is drawn this frame.
    pub fn remember(&mut self, look: Look) {
        let i = look.handle.index as usize;
        if self.looks.len() <= i {
            self.looks.resize(i + 1, None);
        }
        self.looks[i] = Some(look);
    }

    /// Building `h` as last drawn, if it is still there.
    pub fn look(&self, h: StructureHandle) -> Option<&Look> {
        self.looks
            .get(h.index as usize)?
            .as_ref()
            .filter(|l| l.handle == h)
    }

    /// Regiment `a` struck building `structure` (an `Event::Struck`). Shots
    /// set off for it; other blows land at once.
    pub fn struck(&mut self, structure: StructureHandle, a: Attacker) {
        let Some(look) = self.look(structure).copied() else {
            return;
        };
        let r = a.handle.index as usize;
        if let Some(s) = self.struck.get_mut(r) {
            *s = Some((a.handle, self.time));
        }
        let wall = look.nearest(a.at);
        let toward = (wall - a.at).normalize_or(Vec2::X);
        if a.kind.shot() {
            if !a.seen {
                // Unseen archers: their shots land unannounced.
                let across = Vec2::new(-toward.y, toward.x);
                for _ in 0..a.kind.count() {
                    let at = look.nearest(wall + across * self.roll() * 1.2);
                    self.land(&look, a.kind, at, 0.8, toward);
                }
                self.noise(a.kind.sound(), wall);
                return;
            }
            let (speed, _) = a.kind.flight();
            self.seed = self.seed.wrapping_add(0x9E37_79B9);
            let shot = Shot {
                structure,
                kind: a.kind,
                from: a.at,
                from_y: a.ground + RELEASE_HEIGHT,
                to: wall - toward * 0.1,
                to_y: look.base + (look.top * 0.45).clamp(0.6, 2.5),
                dir: toward,
                born: self.time,
                flight: (a.at.distance(wall) / speed).clamp(0.25, 2.5),
                seed: self.seed,
            };
            let k = self.pick(MAX_SHOTS);
            push_capped(&mut self.shots, MAX_SHOTS, k, shot);
            if a.kind != Kind::Arrows {
                self.noise(Sound::Launch, a.at);
            }
            return;
        }
        // A blow from the wall: where its soldiers swung, if they were seen
        // to, or along its front.
        let touches = self.touches.get(r).copied().flatten();
        let swung = touches.filter(|t| t.by == a.handle && t.at == structure && t.n > 0);
        let point = match swung {
            Some(t) => t.points[self.pick(usize::from(t.n))],
            None => {
                let across = Vec2::new(-toward.y, toward.x);
                look.nearest(wall + across * (self.roll() * a.frontage * 0.5))
            }
        };
        let height = if a.kind == Kind::Ram {
            0.55
        } else {
            0.5 + self.roll().abs() * 0.5
        };
        self.land(&look, a.kind, point, height, toward);
        self.noise(a.kind.sound(), point);
    }

    fn noise(&mut self, sound: Sound, at: Vec2) {
        if self.noises.len() < MAX_NOISES {
            self.noises.push(Noise { sound, at });
        }
    }

    /// How many of something that comes `rate` times a second come in `dt`
    /// seconds: as many on average at any frame rate, and none while still.
    fn spawns(&mut self, rate: f32, dt: f32) -> usize {
        (rate * dt + (self.roll() + 1.0) / 2.0) as usize
    }

    /// A blow of `kind` lands on building `look` at `at`, `height` over the
    /// ground, travelling along `dir`: it jolts, and chips and dust fly.
    fn land(&mut self, look: &Look, kind: Kind, at: Vec2, height: f32, dir: Vec2) {
        let force = kind.force();
        // A fresh blow restarts the shake, as hard as the harder.
        let old = self.jolts.iter().position(|j| j.handle == look.handle);
        let left = old.map_or(0.0, |k| {
            let j = self.jolts[k];
            j.strength * (1.0 - (self.time - j.at) / JOLT_SECONDS).max(0.0)
        });
        let jolt = Jolt {
            handle: look.handle,
            dir,
            strength: force.jolt.max(left),
            at: self.time,
        };
        match old {
            Some(k) => self.jolts[k] = jolt,
            None => self.jolts.push(jolt),
        }
        let y = look.base + height;
        let side = Vec2::new(-dir.y, dir.x);
        for _ in 0..force.chips {
            let vel = -dir * (1.0 + self.roll().abs() * 2.5) + side * (self.roll() * 2.0);
            let vy = 1.5 + self.roll().abs() * 3.0;
            let size = if force.big {
                0.12 + self.roll().abs() * 0.22
            } else {
                0.06 + self.roll().abs() * 0.08
            };
            let colour = look.stuff.colours[self.pick(3)];
            let spin = self.roll() * 12.0;
            self.chip(at - dir * 0.15, y, vel, vy, size, colour, spin);
        }
        for _ in 0..force.puffs {
            let drift = -dir * (0.4 + self.roll().abs() * 0.6) + side * (self.roll() * 0.6);
            let size = if force.big { 0.8 } else { 0.45 };
            let life = PUFF_LIFE * (0.8 + self.roll().abs() * 0.4);
            self.puff(
                at - dir * 0.3,
                y - 0.2,
                drift,
                0.35,
                size,
                look.stuff.dust,
                life,
            );
        }
        if kind == Kind::Fire {
            // The shot bursts in flame.
            for _ in 0..4 {
                let drift = Vec2::new(self.roll(), self.roll()) * 0.5;
                let glow = [255, 150 + (self.roll().abs() * 70.0) as u8, 40, 230];
                self.puff(at - dir * 0.2, y, drift, 0.9, 0.5, glow, 0.6);
            }
        }
    }

    /// Building `structure` was brought down (an `Event::Fallen`): it
    /// sinks into a cloud of dust and leaves rubble, once the shots on
    /// their way to it (the last of them brought it down) have landed.
    pub fn fallen(&mut self, structure: StructureHandle) {
        let Some(look) = self.look(structure).copied() else {
            return;
        };
        self.forget(structure);
        let due = (self.shots.iter())
            .filter(|s| s.structure == structure)
            .map(|s| s.born + s.flight)
            .fold(self.time, f32::max);
        let now = due <= self.time;
        self.seed = self.seed.wrapping_add(0x9E37_79B9);
        let fall = Fall {
            look,
            born: due,
            seed: self.seed,
            burst: now,
        };
        let k = self.pick(MAX_FALLS);
        push_capped(&mut self.falls, MAX_FALLS, k, fall);
        if now {
            self.collapse(look);
        }
    }

    /// Building `structure` is gone unseen: drop it without a show.
    pub fn forget(&mut self, structure: StructureHandle) {
        if let Some(l) = self.looks.get_mut(structure.index as usize) {
            *l = None;
        }
        self.jolts.retain(|j| j.handle != structure);
    }

    /// A building starts down: a burst of chips from every side, and the
    /// crash.
    fn collapse(&mut self, look: Look) {
        let centre = look.centre();
        let n = (12.0 + look.size().x * look.size().y).min(40.0) as usize;
        for k in 0..n {
            let dir = Vec2::from_angle(k as f32 * 2.4 + self.roll());
            let y = look.base + look.top * (0.3 + self.roll().abs() * 0.5);
            let vel = dir * (1.0 + self.roll().abs() * 3.0);
            let vy = 1.0 + self.roll().abs() * 3.0;
            let size = 0.14 + self.roll().abs() * 0.26;
            let spin = self.roll() * 8.0;
            let edge = look.nearest(centre + dir * 100.0);
            self.chip(edge, y, vel, vy, size, look.stuff.colours[k % 3], spin);
        }
        self.noise(Sound::Collapse, centre);
    }

    /// Start a frame `dt` seconds after the last; `ground` is the height
    /// of the ground at a point.
    pub fn advance(&mut self, dt: f32, ground: impl Fn(Vec2) -> f32) {
        self.time += dt;
        let now = self.time;
        self.chips.retain_mut(|c| {
            if let Some(rest) = c.rest {
                return now - rest < CHIP_LIFE;
            }
            c.vy -= GRAVITY * dt;
            c.pos += c.vel * dt;
            c.y += c.vy * dt;
            c.facing = c.facing.rotate(Vec2::from_angle(c.spin * dt));
            let floor = ground(c.pos);
            if c.y <= floor {
                c.y = floor;
                if c.vy < -1.5 {
                    // One small bounce, then it lies.
                    c.vy = -c.vy * 0.3;
                    c.vel *= 0.4;
                    c.spin *= 0.5;
                } else {
                    c.rest = Some(now);
                }
            }
            true
        });
        self.puffs.retain(|p| now - p.born < p.life);
        // Shots that came down land; fire-pots trail smoke.
        let mut k = 0;
        while k < self.shots.len() {
            let s = self.shots[k];
            let t = (now - s.born) / s.flight;
            if t < 1.0 {
                if s.kind == Kind::Fire {
                    let (pos, y) = s.at(0, t);
                    for _ in 0..self.spawns(TRAIL_RATE, dt) {
                        self.puff(pos, y, Vec2::ZERO, 0.3, 0.3, [70, 66, 62, 0], 0.9);
                    }
                }
                k += 1;
                continue;
            }
            self.shots.swap_remove(k);
            // The shot that brought it down lands on it as it starts down.
            let fell = (self.falls.iter()).find(|f| f.look.handle == s.structure);
            let Some(look) = self.look(s.structure).copied().or(fell.map(|f| f.look)) else {
                continue;
            };
            for a in 0..s.kind.count() {
                let (pos, y) = s.at(a, 1.0);
                let height = (y - look.base).max(0.3);
                self.land(&look, s.kind, look.nearest(pos), height, s.dir);
            }
            self.noise(s.kind.sound(), s.to);
        }
        // Collapsing buildings throw up dust as they go down.
        for f in 0..self.falls.len() {
            let Fall {
                look, born, burst, ..
            } = self.falls[f];
            if now < born || now - born >= COLLAPSE_SECONDS {
                continue;
            }
            if !burst {
                self.falls[f].burst = true;
                self.collapse(look);
            }
            for _ in 0..self.spawns(COLLAPSE_DUST_RATE, dt) {
                let around = Vec2::from_angle(self.roll() * PI);
                let edge = look.nearest(look.centre() + around * 100.0);
                let y = look.base + self.roll().abs() * look.top * 0.4;
                let vel = around * (0.6 + self.roll().abs());
                let size = 0.9 + self.roll().abs() * 0.8;
                self.puff(edge + around * 0.3, y, vel, 0.5, size, look.stuff.dust, 2.2);
            }
        }
        self.falls
            .retain(|f| now - f.born < COLLAPSE_SECONDS + RUBBLE_SECONDS);
        self.jolts.retain(|j| now - j.at < JOLT_SECONDS);
    }

    /// The sounds of the frames since this was last called.
    pub fn drain_noises(&mut self) -> impl Iterator<Item = Noise> + '_ {
        self.noises.drain(..)
    }

    fn jolt_of(&self, h: StructureHandle) -> Option<&Jolt> {
        self.jolts.iter().find(|j| j.handle == h)
    }

    /// How far building `h` is shaken off its place now, along the ground
    /// and up.
    pub fn jolt(&self, h: StructureHandle) -> (Vec2, f32) {
        let Some(j) = self.jolt_of(h) else {
            return (Vec2::ZERO, 0.0);
        };
        let t = ((self.time - j.at) / JOLT_SECONDS).min(1.0);
        // Pushed along the blow, then rocking back and settling.
        let rock = (t * 3.0 * TAU).cos() * (1.0 - t) * (1.0 - t);
        (j.dir * (j.strength * rock), -j.strength * 0.3 * rock.abs())
    }

    /// How white building `h`'s health bar shows from a fresh blow, 0 to 1.
    pub fn flash(&self, h: StructureHandle) -> f32 {
        self.jolt_of(h)
            .map_or(0.0, |j| (1.0 - (self.time - j.at) / FLASH_SECONDS).max(0.0))
    }

    /// Seconds since regiment `h` last struck a building, if it has.
    pub fn since_struck(&self, h: RegimentHandle) -> Option<f32> {
        let (s, at) = self.struck.get(h.index as usize).copied().flatten()?;
        (s == h).then_some(self.time - at)
    }

    /// Whether regiment `h` loosed at a building lately: its archers keep
    /// shooting.
    pub fn shooting(&self, h: RegimentHandle) -> bool {
        self.since_struck(h).is_some_and(|t| t < SHOOTING_SECONDS)
    }

    /// How far regiment `h`'s front rank has closed to the wall (0 to 1),
    /// stepping it on by `dt` toward closed while `at_wall`.
    pub fn lean(&mut self, h: RegimentHandle, at_wall: bool, dt: f32) -> f32 {
        let Some(l) = self.lean.get_mut(h.index as usize) else {
            return 0.0;
        };
        let step = LEAN_RATE * dt;
        *l = if at_wall {
            (*l + step).min(1.0)
        } else {
            (*l - step).max(0.0)
        };
        *l
    }

    /// Forget where regiment `h`'s soldiers swung, before this frame's
    /// swings at building `at` are noted.
    pub fn untouch(&mut self, h: RegimentHandle, at: StructureHandle) {
        if let Some(t) = self.touches.get_mut(h.index as usize) {
            *t = Some(Touches {
                by: h,
                at,
                points: [Vec2::ZERO; TOUCHES],
                n: 0,
            });
        }
    }

    /// A soldier of regiment `h` swings at the wall at `p`.
    pub fn touch(&mut self, h: RegimentHandle, p: Vec2) {
        if let Some(Some(t)) = self.touches.get_mut(h.index as usize)
            && t.by == h
            && usize::from(t.n) < TOUCHES
        {
            t.points[usize::from(t.n)] = p;
            t.n += 1;
        }
    }

    /// Fallen buildings' models still sinking.
    pub fn sinking(&self) -> impl Iterator<Item = Sinking> + '_ {
        let now = self.time;
        self.falls.iter().filter_map(move |f| {
            let model = f.look.model?;
            // Standing until its last shot lands.
            let t = ((now - f.born) / COLLAPSE_SECONDS).max(0.0);
            if t >= 1.0 {
                return None;
            }
            // Slow to give, then gone in a rush, shuddering as it goes.
            let down = t * t * (3.0 - 2.0 * t);
            let going = if now < f.born { 0.0 } else { 1.0 - t };
            let shudder = (now * 41.0).sin() * 0.06 * going;
            let across = Vec2::new((now * 37.0).cos(), (now * 29.0).sin()) * shudder;
            Some(Sinking {
                model,
                pos: f.look.centre() + across,
                lift: f.look.lift - f.look.top * 1.05 * down,
                colour: f.look.colour,
            })
        })
    }

    /// Draw the chips, dust, shots, rubble and the smoke and fire of the
    /// damaged buildings among `shown`, as boxes; `ground` is the height
    /// of the ground at a point. Adds nothing past `limit` boxes in `out`;
    /// smoke and fire first, as they show the damage from afar.
    pub fn push(
        &self,
        shown: impl Iterator<Item = StructureHandle>,
        ground: impl Fn(Vec2) -> f32,
        limit: usize,
        out: &mut Vec<BoxInstance>,
    ) {
        let limit = limit.max(out.len());
        for h in shown {
            if let Some(look) = self.look(h).filter(|l| l.hp < SMOKES_BELOW) {
                self.burn(look, &ground, out);
            }
        }
        let now = self.time;
        for s in &self.shots {
            let t = ((now - s.born) / s.flight).clamp(0.0, 1.0);
            let (size, color, facing) = match s.kind {
                Kind::Arrows => (ARROW_SIZE, ARROW_COLOR, s.dir),
                Kind::Fire => ([0.3, 0.3, 0.45], [255, 140, 40, 255], s.dir),
                _ => (
                    [0.42, 0.38, 0.42],
                    [96, 92, 86, 0],
                    Vec2::from_angle(now * 9.0),
                ),
            };
            for k in 0..s.kind.count() {
                let (pos, y) = s.at(k, t);
                out.push(BoxInstance {
                    pos: pos.to_array(),
                    facing: facing.to_array(),
                    size,
                    color,
                    lift: y - ground(pos),
                });
            }
        }
        for c in &self.chips {
            // One lying still is on the ground already.
            let lift = match c.rest {
                Some(rest) => {
                    -c.size * ((now - rest - (CHIP_LIFE - CHIP_SINK)) / CHIP_SINK).max(0.0)
                }
                None => c.y - ground(c.pos),
            };
            out.push(BoxInstance {
                pos: c.pos.to_array(),
                facing: c.facing.to_array(),
                size: [c.size, c.size * 0.7, c.size * 0.9],
                color: c.colour,
                lift,
            });
        }
        for p in &self.puffs {
            let age = now - p.born;
            let t = age / p.life;
            // Swells, then thins away.
            let size = p.size * (0.5 + t * 1.2) * (1.0 - t * t * t);
            let pos = p.pos + p.vel * age;
            out.push(BoxInstance {
                pos: pos.to_array(),
                facing: p.facing.to_array(),
                size: shadowless([size, size * 0.8, size]),
                color: p.colour,
                lift: p.y + p.rise * age - ground(pos),
            });
        }
        for f in &self.falls {
            self.rubble(f, &ground, out);
        }
        out.truncate(limit);
    }

    /// A fallen building's rubble, once its model is half sunk: a heap,
    /// smouldering at first, that sinks away in the end.
    fn rubble(&self, f: &Fall, ground: &impl Fn(Vec2) -> f32, out: &mut Vec<BoxInstance>) {
        let age = self.time - f.born - COLLAPSE_SECONDS * 0.5;
        if age < 0.0 {
            return;
        }
        let sink = ((age - (RUBBLE_SECONDS - RUBBLE_SINK)) / RUBBLE_SINK).clamp(0.0, 1.0);
        let look = &f.look;
        let size = look.size();
        let n = (size.x * size.y * 1.2).clamp(8.0, 30.0) as usize;
        for k in 0..n {
            let r = |axis| hash(f.seed, k as u32, axis) * 0.5 + 0.5;
            let at = look.min + Vec2::new(r(0), r(1)) * size;
            // Higher in the middle.
            let middle = 1.0
                - ((at - look.centre()) / (size * 0.5).max(Vec2::ONE))
                    .length()
                    .min(1.0);
            let w = 0.35 + r(2) * 0.7;
            let h = (0.2 + middle * (look.top * 0.18).min(1.2)) * (1.0 - sink);
            out.push(BoxInstance {
                pos: at.to_array(),
                facing: Vec2::from_angle(r(3) * TAU).to_array(),
                size: [w, h.max(0.02), w * 0.8],
                color: look.stuff.colours[k % 3],
                lift: -0.05 - 0.3 * sink,
            });
        }
        if age < SMOULDER_SECONDS {
            let thick = 0.7 * (1.0 - age / SMOULDER_SECONDS);
            let at = look.centre();
            self.smoke(f.seed, at, look.base + 0.4, thick, false, ground, out);
        }
    }

    /// The smoke from a damaged building's roof, and its fire once it is
    /// badly damaged: more columns as it gets worse.
    fn burn(&self, look: &Look, ground: &impl Fn(Vec2) -> f32, out: &mut Vec<BoxInstance>) {
        let h = look.handle;
        let area = look.size().x * look.size().y;
        let spots = 1 + u32::from(area >= 4.0) + u32::from(area >= 12.0);
        let fire = look.hp < BURNS_BELOW;
        let damage = (SMOKES_BELOW - look.hp) / SMOKES_BELOW;
        let seed = h.index.wrapping_mul(0x2545_F491) ^ u32::from(h.generation);
        let roof = look.base + look.top * 0.8;
        let thick = 0.35 + damage * if fire { 0.55 } else { 0.3 };
        // The second and third column only once worse hurt.
        for spot in (0..spots).filter(|&k| damage >= 0.25 * k as f32) {
            let off = Vec2::new(hash(seed, spot, 4), hash(seed, spot, 5)) * look.size() * 0.3;
            let at = look.centre() + off;
            self.smoke(seed ^ spot, at, roof, thick, fire, ground, out);
            if fire {
                self.flames(seed ^ spot, at, roof, ground, out);
            }
        }
    }

    /// A column of smoke from `at`, `y` high: `thick` sizes its puffs, and
    /// a fire's is darker.
    #[allow(clippy::too_many_arguments)]
    fn smoke(
        &self,
        seed: u32,
        at: Vec2,
        y: f32,
        thick: f32,
        fire: bool,
        ground: &impl Fn(Vec2) -> f32,
        out: &mut Vec<BoxInstance>,
    ) {
        let grey: u8 = if fire { 64 } else { 110 };
        for k in 0..SMOKE_PUFFS {
            let r = |axis| hash(seed, k as u32, axis);
            let offset = k as f32 / SMOKE_PUFFS as f32 + r(6) * 0.05;
            let t = (self.time / SMOKE_LIFE + offset).fract();
            let size = thick * (0.45 + t * 1.3) * (1.0 - t * t);
            let pos = at + WIND * (t * SMOKE_LIFE) + Vec2::new(r(7), r(8)) * 0.2;
            let shade = grey.saturating_add((t * 60.0) as u8);
            out.push(BoxInstance {
                pos: pos.to_array(),
                facing: Vec2::from_angle(t * 2.0 + k as f32).to_array(),
                size: shadowless([size, size * 0.85, size]),
                color: [shade, shade, shade.saturating_add(4), 0],
                lift: y + t * SMOKE_LIFE * 0.8 - ground(pos),
            });
        }
    }

    /// Flames licking up from `at`, `y` high.
    fn flames(
        &self,
        seed: u32,
        at: Vec2,
        y: f32,
        ground: &impl Fn(Vec2) -> f32,
        out: &mut Vec<BoxInstance>,
    ) {
        let now = self.time;
        for k in 0..FLAMES {
            let r = |axis| hash(seed, k as u32, axis);
            let phase = r(9) * 6.0;
            let flicker = 0.75 + 0.25 * (now * 11.0 + phase).sin() * (now * 7.3 + phase).cos();
            let size = (0.3 + 0.15 * k as f32 % 0.45) * flicker;
            let pos = at + Vec2::new(r(10), r(11)) * 0.35;
            let color = if k % 2 == 0 {
                [255, 150, 40, 255]
            } else {
                [255, 206, 90, 255]
            };
            out.push(BoxInstance {
                pos: pos.to_array(),
                facing: Vec2::from_angle(now * 2.0 + phase).to_array(),
                size: shadowless([size, size * 1.6, size]),
                color,
                lift: y - 0.15 + 0.1 * (now * 5.0 + phase).sin() - ground(pos),
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn chip(
        &mut self,
        pos: Vec2,
        y: f32,
        vel: Vec2,
        vy: f32,
        size: f32,
        colour: [u8; 4],
        spin: f32,
    ) {
        let chip = Chip {
            pos,
            y,
            vel,
            vy,
            size,
            colour,
            facing: Vec2::from_angle(self.roll() * PI),
            spin,
            rest: None,
        };
        let k = self.pick(MAX_CHIPS);
        push_capped(&mut self.chips, MAX_CHIPS, k, chip);
    }

    #[allow(clippy::too_many_arguments)]
    fn puff(
        &mut self,
        pos: Vec2,
        y: f32,
        vel: Vec2,
        rise: f32,
        size: f32,
        colour: [u8; 4],
        life: f32,
    ) {
        let puff = Puff {
            pos,
            y,
            vel,
            rise,
            size,
            colour,
            facing: Vec2::from_angle(self.roll() * PI),
            born: self.time,
            life,
        };
        let k = self.pick(MAX_PUFFS);
        push_capped(&mut self.puffs, MAX_PUFFS, k, puff);
    }

    /// A value in [-1, 1), different each call.
    fn roll(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }

    /// One of `0..n`, different each call.
    fn pick(&mut self, n: usize) -> usize {
        (((self.roll() + 1.0) * 0.5 * n as f32) as usize).min(n.saturating_sub(1))
    }
}

/// Add `item` to `pool`, or once it holds `cap`, put it in place of item
/// `k` (one chosen at random).
fn push_capped<T>(pool: &mut Vec<T>, cap: usize, k: usize, item: T) {
    if pool.len() < cap {
        pool.push(item);
    } else {
        pool[k] = item;
    }
}

/// A stable value in [-1, 1) per seed, item and axis.
fn hash(seed: u32, k: u32, axis: u32) -> f32 {
    crate::spread_hash(seed, k as usize, axis)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle(index: u32) -> StructureHandle {
        StructureHandle {
            index,
            generation: 1,
        }
    }

    fn regiment(index: u32) -> RegimentHandle {
        RegimentHandle {
            index,
            generation: 1,
        }
    }

    fn look(h: StructureHandle, hp: f32) -> Look {
        Look {
            handle: h,
            min: Vec2::new(10.0, 10.0),
            max: Vec2::new(14.0, 13.0),
            base: 0.0,
            top: 3.0,
            model: Some(2),
            lift: 0.0,
            owner: 1,
            colour: [200, 0, 0],
            stuff: stuff(Some(Faction::EmberClans), Armour::Structure),
            hp,
        }
    }

    fn blade(at: Vec2) -> Attacker {
        Attacker {
            handle: regiment(3),
            at,
            ground: 0.0,
            kind: Kind::Blade,
            frontage: 4.0,
            seen: true,
        }
    }

    fn flat(_: Vec2) -> f32 {
        0.0
    }

    #[test]
    fn a_blow_shakes_the_building_and_chips_fly_off_toward_the_attacker() {
        let mut s = Siege::new(8);
        let h = handle(0);
        s.remember(look(h, 1.0));
        s.struck(h, blade(Vec2::new(12.0, 16.0)));
        assert!(s.flash(h) > 0.9);
        assert_eq!(s.chips.len(), 2);
        assert!(
            s.chips.iter().all(|c| c.vel.y > 0.0),
            "back toward the attacker"
        );
        assert_eq!(
            s.drain_noises().next().map(|n| n.sound),
            Some(Sound::WallHit)
        );
        s.advance(0.05, flat);
        assert_ne!(s.jolt(h), (Vec2::ZERO, 0.0));
        // It settles, and the chips come to rest on the ground.
        for _ in 0..100 {
            s.advance(0.05, flat);
        }
        assert_eq!(s.jolt(h), (Vec2::ZERO, 0.0));
        assert_eq!(s.flash(h), 0.0);
        assert!(s.chips.iter().all(|c| c.rest.is_some() && c.y == 0.0));
        // And are gone once they have lain long enough.
        for _ in 0..200 {
            s.advance(0.05, flat);
        }
        assert!(s.chips.is_empty());
    }

    #[test]
    fn a_blow_lands_where_the_soldiers_swung() {
        let mut s = Siege::new(8);
        let h = handle(0);
        s.remember(look(h, 1.0));
        let a = blade(Vec2::new(12.0, 16.0));
        s.untouch(a.handle, h);
        s.touch(a.handle, Vec2::new(11.0, 13.0));
        s.struck(h, a);
        assert!(
            s.chips
                .iter()
                .all(|c| (c.pos - Vec2::new(11.0, 13.15)).length() < 0.01)
        );
        // Swings noted at another building's wall don't count here.
        let other = handle(1);
        s.remember(look(other, 1.0));
        s.chips.clear();
        s.untouch(a.handle, h);
        s.touch(a.handle, Vec2::new(11.0, 13.0));
        s.struck(other, a);
        assert!(
            s.chips
                .iter()
                .all(|c| (c.pos - Vec2::new(11.0, 13.15)).length() > 0.01)
        );
    }

    #[test]
    fn the_killing_shot_lands_before_the_building_goes_down() {
        let mut s = Siege::new(8);
        let h = handle(0);
        s.remember(look(h, 1.0));
        let mut a = blade(Vec2::new(12.0, 30.0));
        a.kind = Kind::Boulder;
        s.struck(h, a);
        s.fallen(h);
        let _ = s.drain_noises().count();
        // Still standing while the stone flies; nothing piles up while
        // paused.
        for _ in 0..20 {
            s.advance(0.0, flat);
        }
        assert!(s.puffs.is_empty());
        assert_eq!(s.sinking().next().map(|f| f.lift), Some(0.0));
        assert!(s.drain_noises().next().is_none());
        for _ in 0..60 {
            s.advance(0.05, flat);
        }
        let heard: Vec<_> = s.drain_noises().map(|n| n.sound).collect();
        assert!(heard.contains(&Sound::Impact) && heard.contains(&Sound::Collapse));
        assert!(s.sinking().next().is_some_and(|f| f.lift < 0.0));
    }

    #[test]
    fn a_volley_flies_to_the_wall_and_lands_there() {
        let mut s = Siege::new(8);
        let h = handle(0);
        s.remember(look(h, 1.0));
        let mut a = blade(Vec2::new(12.0, 30.0));
        a.kind = Kind::Arrows;
        s.struck(h, a);
        assert!(s.shooting(a.handle));
        assert_eq!(s.shots.len(), 1);
        assert!(s.chips.is_empty(), "not there yet");
        let mut out = Vec::new();
        s.push(core::iter::empty(), flat, 1000, &mut out);
        assert_eq!(out.len(), ARROWS_PER_VOLLEY);
        for _ in 0..30 {
            s.advance(0.05, flat);
        }
        assert!(s.shots.is_empty());
        assert_eq!(s.chips.len(), ARROWS_PER_VOLLEY);
        assert!(s.drain_noises().any(|n| n.sound == Sound::Arrows));
        for _ in 0..30 {
            s.advance(0.05, flat);
        }
        assert!(!s.shooting(a.handle));
    }

    #[test]
    fn a_building_smokes_once_hurt_and_burns_once_badly_hurt() {
        let mut s = Siege::new(8);
        let h = handle(0);
        let count = |s: &mut Siege, hp: f32| {
            s.remember(look(h, hp));
            let mut out = Vec::new();
            s.push([h].into_iter(), flat, 10_000, &mut out);
            let glowing = out.iter().filter(|b| b.color[3] == 255).count();
            (out.len(), glowing)
        };
        assert_eq!(count(&mut s, 0.9), (0, 0));
        let (smoke, fire) = count(&mut s, 0.5);
        assert!(smoke > 0 && fire == 0);
        let (more, fire) = count(&mut s, 0.1);
        assert!(more > smoke && fire > 0);
    }

    #[test]
    fn a_fallen_building_sinks_then_lies_in_rubble_that_goes_in_the_end() {
        let mut s = Siege::new(8);
        let h = handle(0);
        s.remember(look(h, 0.1));
        s.fallen(h);
        assert!(s.drain_noises().any(|n| n.sound == Sound::Collapse));
        let first = s.sinking().next().expect("sinking");
        s.advance(1.0, flat);
        let later = s.sinking().next().expect("still sinking");
        assert!(later.lift < first.lift);
        s.advance(COLLAPSE_SECONDS, flat);
        assert_eq!(s.sinking().count(), 0);
        let mut out = Vec::new();
        s.push(core::iter::empty(), flat, 10_000, &mut out);
        assert!(out.len() >= 8, "rubble");
        // A blow told after it fell finds nothing to hit.
        s.struck(h, blade(Vec2::new(12.0, 16.0)));
        assert!(s.jolts.is_empty());
        for _ in 0..100 {
            s.advance(1.0, flat);
        }
        assert!(s.falls.is_empty());
    }

    #[test]
    fn the_front_rank_closes_to_the_wall_and_falls_back_smoothly() {
        let mut s = Siege::new(8);
        let r = regiment(2);
        assert!(s.lean(r, true, 0.1) > 0.0);
        let mut l = 0.0;
        for _ in 0..20 {
            l = s.lean(r, true, 0.1);
        }
        assert_eq!(l, 1.0);
        assert!(s.lean(r, false, 0.1) < 1.0);
    }

    #[test]
    fn kinds_follow_the_unit() {
        assert_eq!(Kind::of(false, Breaker::Regiment), Kind::Blade);
        assert_eq!(Kind::of(false, Breaker::Siege), Kind::Ram);
        assert_eq!(Kind::of(true, Breaker::Regiment), Kind::Arrows);
        assert_eq!(Kind::of(true, Breaker::Siege), Kind::Boulder);
        assert_eq!(Kind::of(true, Breaker::Artillery), Kind::Fire);
    }
}
