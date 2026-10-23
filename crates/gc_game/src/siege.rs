//! What a siege looks like on screen (docs/21-buildings.md, "Sieges on
//! screen"): the blows the simulation tells (`Event::Struck`) shake the
//! building and knock chips and dust off where they land; archers' arrows
//! and siege engines' shots fly to it; a damaged building smokes, then
//! burns; a building brought down sinks in a cloud of dust and leaves
//! rubble.
//!
//! Only a picture. Every blow drawn is one the simulation struck, but where
//! the chips fly, how a building shakes and how long rubble lies is chosen
//! here and changes nothing in the match. Dust, smoke, flames, sparks and
//! small chips are particles (`gc_render`'s [`Effects`], handed over by
//! [`Siege::take_particles`]); the shots in the air, the big chunks of a
//! collapse, arrows stuck in a wall and the heaps of rubble are boxes.

use std::f32::consts::{PI, TAU};

use gc_data::Faction;
use gc_render::{BoxInstance, Effects, GHOST, ICHOR, Particle};
use gc_sim::structures::{Armour, Breaker};
use gc_sim::{PlayerId, RegimentHandle, StructureHandle};
use glam::{Vec2, Vec3};

use crate::field::{ARROW_COLOR, ARROW_SIZE, ARROWS_PER_VOLLEY};

/// Chunks, shots in the air and arrows stuck in walls at once, at most:
/// when full, a new one takes the place of one at random.
const MAX_CHIPS: usize = 384;
const MAX_SHOTS: usize = 256;
const MAX_STUCK: usize = 192;
/// Fallen buildings sinking or lying in rubble at once, at most.
const MAX_FALLS: usize = 48;
/// Wall points kept per regiment, where its soldiers last swung.
const TOUCHES: usize = 6;

/// Pull of the ground on chips, in tiles a second a second (a soldier is
/// under a tile tall).
const GRAVITY: f32 = 6.0;
/// A chunk lies this long once down, then sinks out of sight over the last
/// [`CHIP_SINK`] seconds.
const CHIP_LIFE: f32 = 7.0;
const CHIP_SINK: f32 = 1.5;
/// An arrow stays in the wall this long.
const STUCK_SECONDS: f32 = 40.0;
/// The longest frame the continuous effects (smoke, fire, trails) make up
/// for, so a stall or a seek does not release a flood of particles.
const MAX_DT: f32 = 0.25;
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
/// Dust thrown up by a building going down, in puffs a second.
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

/// What kind of thing a building is made of: how blows on it look, and how
/// it falls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Timber and thatch (Ember Clans): splinters, and it burns as it falls.
    Timber,
    /// Dressed stone: chips and sparks, and a heavy fall.
    Stone,
    /// Bone and iron (Carrion Concord): bone scatters, ghost-lights escape.
    Bone,
    /// Crystal (the Choir): it shatters in glittering shards.
    Crystal,
    /// Chitin and flesh (the Brood): ichor sprays and it deflates.
    Shell,
}

/// What a building is made of, for its chips, dust and rubble.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stuff {
    pub style: Style,
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
            style: Style::Shell,
            colours: [[92, 58, 70, 0], [128, 96, 64, 0], [70, 48, 60, 0]],
            dust: [120, 104, 84, 0],
        },
        (Some(Faction::Choir), _) => Stuff {
            style: Style::Crystal,
            colours: [[150, 190, 214, 70], [196, 216, 230, 70], stone],
            dust: [190, 200, 206, 0],
        },
        (Some(Faction::CarrionConcord), _) => Stuff {
            style: Style::Bone,
            colours: [[214, 204, 178, 0], [96, 92, 86, 0], wood],
            dust: [150, 146, 136, 0],
        },
        (_, Armour::Fortified) => Stuff {
            style: Style::Stone,
            colours: [stone, [104, 100, 94, 0], [150, 146, 136, 0]],
            dust,
        },
        _ => Stuff {
            style: Style::Timber,
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

    /// Its footprint, in square tiles.
    fn area(&self) -> f32 {
        let size = self.size();
        size.x * size.y
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

/// A colour without its glow.
fn rgb([r, g, b, _]: [u8; 4]) -> [u8; 3] {
    [r, g, b]
}

/// Point `at` on the map, `y` high: where the renderer puts a particle.
fn up(at: Vec2, y: f32) -> Vec3 {
    Vec3::new(at.x, y, at.y)
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

/// An arrow stuck in a wall.
#[derive(Clone, Copy, Debug)]
struct Stuck {
    structure: StructureHandle,
    pos: Vec2,
    y: f32,
    dir: Vec2,
    born: f32,
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
    stuck: Vec<Stuck>,
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
    /// Dust, sparks, smoke and fire collected for the frame.
    fx: Effects,
    /// The last frame's length, for the effects that go on and on.
    dt: f32,
}

impl Siege {
    pub fn new(regiments: usize) -> Siege {
        Siege {
            time: 0.0,
            seed: 0x5EED_0001,
            looks: Vec::new(),
            jolts: Vec::new(),
            chips: Vec::with_capacity(MAX_CHIPS),
            stuck: Vec::with_capacity(MAX_STUCK),
            shots: Vec::with_capacity(MAX_SHOTS),
            falls: Vec::with_capacity(MAX_FALLS),
            struck: vec![None; regiments],
            lean: vec![0.0; regiments],
            touches: vec![None; regiments],
            noises: Vec::new(),
            fx: Effects::new(0x51e6_e000),
            dt: 0.0,
        }
    }

    /// How much of every particle effect to spawn (the Graphics preset's
    /// choice), 1 for all of it.
    pub fn set_density(&mut self, density: f32) {
        self.fx.set_density(density);
    }

    /// The dust, sparks, smoke and fire made since this was last called,
    /// ready for `Renderer::spawn_particles`.
    pub fn take_particles(&mut self) -> Vec<Particle> {
        self.fx.take()
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
        self.blow_fx(look, kind, at, look.base + height, dir);
    }

    /// What a blow of `kind` throws up at `at`, `y` high on the wall,
    /// travelling along `dir`: dust, chips and, by what the wall is made
    /// of, sparks, splinters, shards or ichor.
    fn blow_fx(&mut self, look: &Look, kind: Kind, at: Vec2, y: f32, dir: Vec2) {
        let force = kind.force();
        let back = -dir;
        let off = at + back * 0.15;
        let p = up(off, y);
        let away = up(back, 0.3);
        let n = force.chips as f32;
        let size = if force.big { 0.2 } else { 0.1 };
        let colours = look.stuff.colours;
        match look.stuff.style {
            Style::Crystal => self.fx.shards(p, away, n + 2.0),
            Style::Shell => {
                self.fx.spray(p, away, ICHOR, n * 0.8);
                self.fx.chips(p, back, rgb(colours[0]), n * 0.4, size);
            }
            style => {
                for (k, c) in colours.iter().enumerate() {
                    self.fx
                        .chips(p, back, rgb(*c), n / 3.0 + 0.4 * k as f32, size);
                }
                match style {
                    Style::Stone => self.fx.sparks(p, away, if force.big { 6.0 } else { 2.0 }),
                    Style::Bone => self.fx.motes(p, 0.2, GHOST, 1.5),
                    _ => {}
                }
            }
        }
        // A heavy blow knocks real chunks off, which lie where they fall.
        if force.big {
            for _ in 0..(force.chips / 3).max(1) {
                let vel = back * (1.0 + self.roll().abs() * 2.5) + dir.perp() * (self.roll() * 2.0);
                let vy = 1.5 + self.roll().abs() * 3.0;
                let size = 0.12 + self.roll().abs() * 0.22;
                let colour = look.stuff.colours[self.pick(3)];
                let spin = self.roll() * 12.0;
                self.chip(off, y, vel, vy, size, colour, spin);
            }
        }
        let (radius, puffs) = if force.big { (0.8, 1.5) } else { (0.35, 1.0) };
        self.fx
            .dust_cloud(p, radius, force.puffs as f32 * puffs, rgb(look.stuff.dust));
        if kind == Kind::Fire {
            // The shot bursts in flame.
            self.fx.flames(p, 0.35, 0.4);
            self.fx.embers(p, 0.2, 7.0);
            self.fx.smoke(p, 0.4, 0.9, 0.3);
        }
        // Arrows stay in timber, flesh and bone.
        if kind == Kind::Arrows
            && matches!(look.stuff.style, Style::Timber | Style::Shell | Style::Bone)
            // Not into a building already coming down.
            && self.look(look.handle).is_some()
        {
            let k = self.pick(MAX_STUCK);
            let stuck = Stuck {
                structure: look.handle,
                pos: at,
                y,
                dir,
                born: self.time,
            };
            push_capped(&mut self.stuck, MAX_STUCK, k, stuck);
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
        self.stuck.retain(|a| a.structure != structure);
    }

    /// A building starts down: a burst of chunks and dust from every side,
    /// by what it is made of, and the crash.
    fn collapse(&mut self, look: Look) {
        let centre = look.centre();
        let area = look.area();
        let n = (6.0 + area / 2.0).min(16.0) as usize;
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
        let middle = up(centre, look.base + look.top * 0.5);
        let foot = up(centre, look.base);
        let reach = look.size().max_element().clamp(1.5, 6.0);
        let pieces = (12.0 + area).min(48.0);
        let colours = look.stuff.colours;
        self.fx.dust_burst(
            foot,
            reach * 0.8,
            10.0 + area.min(20.0),
            rgb(look.stuff.dust),
        );
        match look.stuff.style {
            Style::Timber => {
                for c in colours {
                    self.fx.debris(middle, rgb(c), pieces / 3.0, 1.2);
                }
                self.fx.embers(middle, reach * 0.4, 30.0);
            }
            Style::Stone => {
                for c in colours {
                    self.fx.debris(middle, rgb(c), pieces / 2.0, 1.4);
                }
                self.fx.sparks(middle, Vec3::Y, 8.0);
            }
            Style::Bone => {
                for c in colours {
                    self.fx.debris(middle, rgb(c), pieces / 3.0, 1.2);
                }
                self.fx.motes(middle, reach * 0.4, GHOST, 24.0);
            }
            Style::Crystal => {
                // It shatters: shards in every direction.
                for k in 0..6 {
                    let dir = up(Vec2::from_angle(k as f32 * 1.05), 0.6);
                    self.fx.shards(middle, dir, pieces);
                }
            }
            Style::Shell => {
                for k in 0..4 {
                    let dir = up(Vec2::from_angle(k as f32 * 1.6), 0.7);
                    self.fx.spray(middle, dir, ICHOR, pieces / 3.0);
                }
                self.fx.debris(middle, rgb(colours[0]), pieces / 3.0, 1.0);
            }
        }
        self.noise(Sound::Collapse, centre);
    }

    /// A random point over `look`'s footprint, `y` high.
    fn over(&mut self, look: &Look, y: f32) -> Vec3 {
        let at = look.min + (Vec2::new(self.roll(), self.roll()) * 0.5 + 0.5) * look.size();
        up(at, y)
    }

    /// A building going down, for the frame just played: dust rolling off
    /// all along its footprint (a wall's length too), and by what it is
    /// made of, flames, ghost-lights, glitter or ichor.
    fn falling(&mut self, look: &Look) {
        let ground = look.base;
        let dt = self.dt;
        for _ in 0..self.spawns(COLLAPSE_DUST_RATE, dt) {
            let (rise, swell) = (self.roll().abs(), self.roll().abs());
            let at = self.over(look, ground + rise * look.top * 0.4);
            self.fx
                .dust_cloud(at, 0.9 + swell * 0.5, 1.0, rgb(look.stuff.dust));
        }
        let top = ground + look.top * 0.5;
        for _ in 0..self.spawns(14.0, dt) {
            let at = self.over(look, top);
            match look.stuff.style {
                Style::Timber => {
                    self.fx.flames(at, 0.7, 0.2);
                    self.fx.embers(at, 0.2, 2.0);
                }
                Style::Bone => self.fx.motes(at, 0.3, GHOST, 2.0),
                Style::Crystal => self.fx.shards(at, Vec3::Y, 3.0),
                Style::Shell => self.fx.spray(at, Vec3::Y, ICHOR, 2.0),
                Style::Stone => {}
            }
        }
    }

    /// The smoke and fire of the damaged buildings among `shown` that
    /// `visible` (a sphere in the camera's view) takes in, and of rubble
    /// still smouldering, for the frame just played: more columns as a
    /// building gets worse, flames and embers once it burns.
    pub fn emit_damage(
        &mut self,
        shown: impl Iterator<Item = StructureHandle>,
        visible: impl Fn(Vec3, f32) -> bool,
    ) {
        let dt = self.dt;
        for h in shown {
            let Some(look) = self.look(h).filter(|l| l.hp < SMOKES_BELOW).copied() else {
                continue;
            };
            let roof = look.base + look.top * 0.8;
            let centre = look.centre();
            if !visible(up(centre, roof), look.top + 6.0) {
                continue;
            }
            let area = look.area();
            let spots = 1 + u32::from(area >= 4.0) + u32::from(area >= 12.0);
            let fire = look.hp < BURNS_BELOW;
            let damage = (SMOKES_BELOW - look.hp) / SMOKES_BELOW;
            let seed = h.index.wrapping_mul(0x2545_F491) ^ u32::from(h.generation);
            let thick = 0.35 + damage * if fire { 0.55 } else { 0.3 };
            // The second and third column only once worse hurt.
            for spot in (0..spots).filter(|&k| damage >= 0.25 * k as f32) {
                let off = Vec2::new(hash(seed, spot, 4), hash(seed, spot, 5)) * look.size() * 0.3;
                let at = up(centre + off, roof);
                self.fx
                    .smoke(at, thick * 0.7, if fire { 0.85 } else { 0.3 }, dt);
                if fire {
                    self.fx.flames(at - Vec3::Y * 0.15, 0.45, dt);
                    self.fx.embers(at, 0.3, 2.5 * dt);
                }
            }
        }
        for f in &self.falls {
            let age = self.time - f.born - COLLAPSE_SECONDS * 0.5;
            if !(0.0..SMOULDER_SECONDS).contains(&age) {
                continue;
            }
            let at = up(f.look.centre(), f.look.base + 0.4);
            if !visible(at, 3.0) {
                continue;
            }
            let thick = 0.7 * (1.0 - age / SMOULDER_SECONDS);
            self.fx.smoke(at, thick * 0.7, 0.8, dt);
            if f.look.stuff.style == Style::Timber && age < SMOULDER_SECONDS * 0.4 {
                self.fx.flames(at, 0.4, dt);
            }
        }
    }

    /// Start a frame `dt` seconds after the last; `ground` is the height
    /// of the ground at a point.
    pub fn advance(&mut self, dt: f32, ground: impl Fn(Vec2) -> f32) {
        self.time += dt;
        self.dt = dt.min(MAX_DT);
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
        self.stuck.retain(|s| now - s.born < STUCK_SECONDS);
        // Shots that came down land; fire-pots trail smoke.
        let mut k = 0;
        while k < self.shots.len() {
            let s = self.shots[k];
            let t = (now - s.born) / s.flight;
            if t < 1.0 {
                if s.kind == Kind::Fire {
                    let (pos, y) = s.at(0, t);
                    self.fx.trail(up(pos, y), self.dt);
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
            self.falling(&look);
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

    /// Draw the chunks, shots, stuck arrows and rubble, as boxes; `ground`
    /// is the height of the ground at a point. Adds nothing past `limit`
    /// boxes in `out`.
    pub fn push(&self, ground: impl Fn(Vec2) -> f32, limit: usize, out: &mut Vec<BoxInstance>) {
        let limit = limit.max(out.len());
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
        for a in &self.stuck {
            out.push(BoxInstance {
                pos: a.pos.to_array(),
                facing: a.dir.to_array(),
                size: ARROW_SIZE,
                color: ARROW_COLOR,
                lift: a.y - ground(a.pos),
            });
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
        for f in &self.falls {
            self.rubble(f, out);
        }
        out.truncate(limit);
    }

    /// A fallen building's rubble, once its model is half sunk: a heap,
    /// smouldering at first, that sinks away in the end.
    fn rubble(&self, f: &Fall, out: &mut Vec<BoxInstance>) {
        let age = self.time - f.born - COLLAPSE_SECONDS * 0.5;
        if age < 0.0 {
            return;
        }
        let sink = ((age - (RUBBLE_SECONDS - RUBBLE_SINK)) / RUBBLE_SINK).clamp(0.0, 1.0);
        let look = &f.look;
        let size = look.size();
        let n = (look.area() * 1.2).clamp(8.0, 30.0) as usize;
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

    fn shards(s: &mut Siege) -> Vec<Particle> {
        let mut all = s.take_particles();
        all.retain(|p| p.shape == gc_render::Shape::Shard);
        all
    }

    /// How many of `ps` glow (fire, sparks, shards, ghost-lights).
    fn additive(ps: &[Particle]) -> usize {
        ps.iter()
            .filter(|p| p.flags & gc_render::particle_flags::ADDITIVE != 0)
            .count()
    }

    #[test]
    fn a_blow_shakes_the_building_and_chips_fly_off_toward_the_attacker() {
        let mut s = Siege::new(8);
        let h = handle(0);
        s.remember(look(h, 1.0));
        s.struck(h, blade(Vec2::new(12.0, 16.0)));
        assert!(s.flash(h) > 0.9);
        // A small blow throws splinters and dust, no chunks.
        assert!(s.chips.is_empty());
        let made = shards(&mut s);
        assert!(!made.is_empty());
        assert!(
            made.iter().all(|p| p.vel.z > 0.0),
            "back toward the attacker"
        );
        assert_eq!(
            s.drain_noises().next().map(|n| n.sound),
            Some(Sound::WallHit)
        );
        s.advance(0.05, flat);
        assert_ne!(s.jolt(h), (Vec2::ZERO, 0.0));
        // It settles.
        for _ in 0..100 {
            s.advance(0.05, flat);
        }
        assert_eq!(s.jolt(h), (Vec2::ZERO, 0.0));
        assert_eq!(s.flash(h), 0.0);
    }

    #[test]
    fn a_heavy_blow_knocks_chunks_off_that_lie_then_go() {
        let mut s = Siege::new(8);
        let h = handle(0);
        s.remember(look(h, 1.0));
        let mut ram = blade(Vec2::new(12.0, 16.0));
        ram.kind = Kind::Ram;
        s.struck(h, ram);
        assert_eq!(s.chips.len(), 3);
        for _ in 0..100 {
            s.advance(0.05, flat);
        }
        assert!(s.chips.iter().all(|c| c.rest.is_some() && c.y == 0.0));
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
        let near = |p: &Particle| (p.pos.x - 11.0).abs() < 0.01 && (p.pos.z - 13.15).abs() < 0.01;
        let made = shards(&mut s);
        assert!(!made.is_empty() && made.iter().all(near));
        // Swings noted at another building's wall don't count here.
        let other = handle(1);
        s.remember(look(other, 1.0));
        s.untouch(a.handle, h);
        s.touch(a.handle, Vec2::new(11.0, 13.0));
        s.struck(other, a);
        assert!(!shards(&mut s).iter().any(near));
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
        assert!(s.take_particles().is_empty());
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
        s.push(flat, 1000, &mut out);
        assert_eq!(out.len(), ARROWS_PER_VOLLEY);
        for _ in 0..30 {
            s.advance(0.05, flat);
        }
        assert!(s.shots.is_empty());
        // They stay in the timber.
        assert_eq!(s.stuck.len(), ARROWS_PER_VOLLEY);
        assert!(s.drain_noises().any(|n| n.sound == Sound::Arrows));
        s.advance(STUCK_SECONDS, flat);
        assert!(s.stuck.is_empty());
        for _ in 0..30 {
            s.advance(0.05, flat);
        }
        assert!(!s.shooting(a.handle));
    }

    #[test]
    fn arrows_do_not_hang_in_the_air_over_a_fallen_building() {
        let mut s = Siege::new(8);
        let h = handle(0);
        s.remember(look(h, 1.0));
        let mut a = blade(Vec2::new(12.0, 16.0));
        a.kind = Kind::Arrows;
        a.seen = false;
        s.struck(h, a);
        assert_eq!(s.stuck.len(), ARROWS_PER_VOLLEY);
        s.fallen(h);
        assert!(s.stuck.is_empty());
        // Shots landing while it goes down stick in nothing.
        s.struck(h, a);
        assert!(s.stuck.is_empty());
    }

    #[test]
    fn a_building_smokes_once_hurt_and_burns_once_badly_hurt() {
        let mut s = Siege::new(8);
        let h = handle(0);
        let count = |s: &mut Siege, hp: f32| {
            s.remember(look(h, hp));
            s.advance(0.5, flat);
            let _ = s.take_particles();
            s.emit_damage([h].into_iter(), |_, _| true);
            let made = s.take_particles();
            (made.len(), additive(&made))
        };
        assert_eq!(count(&mut s, 0.9), (0, 0));
        let (smoke, fire) = count(&mut s, 0.5);
        assert!(smoke > 0 && fire == 0);
        let (more, fire) = count(&mut s, 0.1);
        assert!(more > smoke && fire > 0);
        // None for a building the camera does not see.
        s.emit_damage([h].into_iter(), |_, _| false);
        assert!(s.take_particles().is_empty());
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
        s.push(flat, 10_000, &mut out);
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
    fn every_kind_of_building_falls_its_own_way() {
        let fall = |faction, armour| {
            let mut s = Siege::new(8);
            let h = handle(0);
            let mut l = look(h, 0.1);
            l.stuff = stuff(Some(faction), armour);
            s.remember(l);
            s.fallen(h);
            s.advance(0.1, flat);
            s.take_particles()
        };
        let clans = fall(Faction::EmberClans, Armour::Structure);
        let stone = fall(Faction::EmberClans, Armour::Fortified);
        let bones = fall(Faction::CarrionConcord, Armour::Structure);
        let crystal = fall(Faction::Choir, Armour::Structure);
        let shell = fall(Faction::Brood, Armour::Living);
        for made in [&clans, &stone, &bones, &crystal, &shell] {
            assert!(made.len() > 30, "{} particles", made.len());
        }
        // Timbers burn, crystal glitters, stone only throws dust and rock.
        assert!(additive(&clans) > additive(&stone));
        assert!(additive(&crystal) > additive(&stone));
        assert!(additive(&bones) > additive(&stone));
        assert_eq!(additive(&shell), 0);
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
