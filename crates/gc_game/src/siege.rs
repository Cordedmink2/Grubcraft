//! What a siege looks like on screen (docs/21-buildings.md, "On screen"):
//! the blows the simulation tells (`Event::Struck`) shake the building and
//! knock chips and dust off where they land; archers' arrows and siege
//! engines' shots fly to it; a damaged building smokes, then burns; a
//! building brought down sinks in a cloud of dust and leaves rubble.
//!
//! Only a picture. Every blow drawn is one the simulation struck, but where
//! the chips fly, how a building shakes and how long rubble lies is chosen
//! here and changes nothing in the match.

use gc_data::Faction;
use gc_render::BoxInstance;
use gc_sim::structures::{Armour, Breaker};
use gc_sim::{RegimentHandle, StructureHandle};
use glam::Vec2;

/// Chips, dust puffs and shots in the air at once, at most: the oldest
/// make way.
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
/// How fast soldiers close up to the wall once at a siege, as a share a
/// second, and how near it they stand to swing.
const LEAN_RATE: f32 = 2.5;
pub const WALL_GAP: f32 = 0.5;
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
/// Arrows drawn per volley at a building.
const ARROWS: usize = 6;
/// How fast shots fly, in tiles a second, and how high they arc for the
/// length of their flight.
const ARROW_SPEED: f32 = 22.0;
const ARROW_ARC: f32 = 0.12;
const BOULDER_SPEED: f32 = 13.0;
const BOULDER_ARC: f32 = 0.32;

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

    /// How hard it lands: chips knocked off, dust raised, the jolt in tiles.
    fn force(self) -> (usize, usize, f32) {
        match self {
            Kind::Blade => (2, 1, 0.025),
            Kind::Arrows => (1, 0, 0.012),
            Kind::Ram => (9, 3, 0.11),
            Kind::Fire => (8, 3, 0.1),
            Kind::Boulder => (14, 5, 0.17),
        }
    }
}

/// What a building is made of, for its chips, dust and rubble.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stuff {
    /// Chip and rubble colours, picked among.
    pub colours: [[u8; 3]; 3],
    /// Dust colour.
    pub dust: [u8; 3],
    /// Glow of its chips (the Choir's crystal shines).
    pub glow: u8,
}

/// What a building of `faction` with `armour` is made of.
pub fn stuff(faction: Option<Faction>, armour: Armour) -> Stuff {
    let wood = [116, 82, 52];
    let stone = [128, 124, 116];
    let plaster = [176, 160, 132];
    let dust = [168, 156, 136];
    match (faction, armour) {
        (Some(Faction::Brood), _) | (_, Armour::Living) => Stuff {
            colours: [[92, 58, 70], [128, 96, 64], [70, 48, 60]],
            dust: [120, 104, 84],
            glow: 0,
        },
        (Some(Faction::Choir), _) => Stuff {
            colours: [[150, 190, 214], [196, 216, 230], stone],
            dust: [190, 200, 206],
            glow: 70,
        },
        (Some(Faction::CarrionConcord), _) => Stuff {
            colours: [[214, 204, 178], [96, 92, 86], wood],
            dust: [150, 146, 136],
            glow: 0,
        },
        (_, Armour::Fortified) => Stuff {
            colours: [stone, [104, 100, 94], [150, 146, 136]],
            dust,
            glow: 0,
        },
        _ => Stuff {
            colours: [wood, stone, plaster],
            dust,
            glow: 0,
        },
    }
}

/// A building as it was last drawn: what a blow on it, or its fall, is
/// shown against.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub handle: StructureHandle,
    /// The footprint's corners on the map, in tiles.
    pub min: Vec2,
    pub max: Vec2,
    /// The height of the ground its model stands on, and the model's top
    /// above that.
    pub base: f32,
    pub top: f32,
    /// Its model, if it has one, drawn this high over the ground at the
    /// footprint's centre.
    pub model: Option<usize>,
    pub lift: f32,
    pub colour: [u8; 3],
    pub stuff: Stuff,
    /// Share of its hit points left.
    pub hp: f32,
    /// Finished, not still going up (one going up has few hit points yet,
    /// and doesn't smoke for it).
    pub finished: bool,
}

impl Look {
    pub fn centre(&self) -> Vec2 {
        (self.min + self.max) / 2.0
    }

    /// The nearest point of the footprint's edge to `p` (from outside).
    pub fn nearest(&self, p: Vec2) -> Vec2 {
        p.clamp(self.min, self.max)
    }

    /// How far `p` is from the footprint (0 inside).
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
    /// 0 to 1.
    pub gain: f32,
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
    born: f32,
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
    /// Across the wall, for spreading a volley's arrows.
    across: Vec2,
    born: f32,
    flight: f32,
    seed: u32,
}

impl Shot {
    /// Where arrow (or stone) `k` is at `t` (0 to 1) of its flight, and how
    /// high.
    fn at(&self, k: usize, t: f32) -> (Vec2, f32) {
        let spread = if self.kind == Kind::Arrows {
            self.across * hash(self.seed, k as u32, 0) * 1.2
        } else {
            Vec2::ZERO
        };
        let rise = if self.kind == Kind::Arrows {
            hash(self.seed, k as u32, 1) * 0.45
        } else {
            0.0
        };
        let length = self.from.distance(self.to);
        let arc = if self.kind == Kind::Arrows {
            ARROW_ARC
        } else {
            BOULDER_ARC
        };
        let pos = self.from.lerp(self.to + spread, t);
        let y = self.from_y
            + (self.to_y + rise - self.from_y) * t
            + (t * std::f32::consts::PI).sin() * length * arc;
        (pos, y)
    }

    fn count(&self) -> usize {
        if self.kind == Kind::Arrows { ARROWS } else { 1 }
    }
}

/// A building brought down: sinking, then rubble.
#[derive(Clone, Copy, Debug)]
struct Fall {
    look: Look,
    born: f32,
    seed: u32,
}

/// A fallen building's model, sinking, for the game to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sinking {
    pub model: usize,
    pub pos: Vec2,
    pub lift: f32,
    pub colour: [u8; 3],
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
    next_chip: usize,
    puffs: Vec<Puff>,
    next_puff: usize,
    shots: Vec<Shot>,
    falls: Vec<Fall>,
    /// By regiment slot: when it last struck a building, and the kind of
    /// its blows.
    struck: Vec<Option<(RegimentHandle, f32)>>,
    /// By regiment slot: how far its front rank has closed to the wall.
    lean: Vec<f32>,
    /// By regiment slot: where its soldiers swung at the wall last frame.
    touches: Vec<Option<(RegimentHandle, [Vec2; TOUCHES], u8)>>,
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
            next_chip: 0,
            puffs: Vec::with_capacity(MAX_PUFFS),
            next_puff: 0,
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
    pub fn look(&self, h: StructureHandle) -> Option<Look> {
        self.looks
            .get(h.index as usize)
            .copied()
            .flatten()
            .filter(|l| l.handle == h)
    }

    /// Regiment `a` struck building `structure` (an `Event::Struck`). Shots
    /// set off for it; other blows land at once.
    pub fn struck(&mut self, structure: StructureHandle, a: Attacker) {
        let Some(look) = self.look(structure) else {
            return;
        };
        let r = a.handle.index as usize;
        if let Some(s) = self.struck.get_mut(r) {
            *s = Some((a.handle, self.time));
        }
        let wall = look.nearest(a.at);
        let toward = (wall - a.at).normalize_or(Vec2::X);
        let across = Vec2::new(-toward.y, toward.x);
        if a.kind.shot() {
            if !a.seen {
                // Unseen archers: their shots land unannounced.
                self.land(&look, a.kind, wall, 0.8, toward);
                return;
            }
            let to_y = look.base + (look.top * 0.45).clamp(0.6, 2.5);
            let from_y = look.base.max(0.0) + 0.9;
            let speed = if a.kind == Kind::Arrows {
                ARROW_SPEED
            } else {
                BOULDER_SPEED
            };
            let length = a.at.distance(wall);
            self.seed = self.seed.wrapping_add(0x9E37_79B9);
            if self.shots.len() >= MAX_SHOTS {
                self.shots.remove(0);
            }
            self.shots.push(Shot {
                structure,
                kind: a.kind,
                from: a.at,
                from_y,
                to: wall - toward * 0.1,
                to_y,
                across,
                born: self.time,
                flight: (length / speed).clamp(0.25, 2.5),
                seed: self.seed,
            });
            if a.kind != Kind::Arrows {
                self.noises.push(Noise {
                    sound: Sound::Launch,
                    at: a.at,
                    gain: 0.9,
                });
            }
            return;
        }
        // A blow from the wall: where its soldiers swung, if they were seen
        // to, or along its front.
        let touches = self.touches.get(r).copied().flatten();
        let point = match touches.filter(|t| t.0 == a.handle && t.2 > 0) {
            Some((_, points, n)) => points[self.roll_index(usize::from(n))],
            None => look.nearest(wall + across * (self.roll() * a.frontage * 0.5)),
        };
        let height = if a.kind == Kind::Ram {
            0.55
        } else {
            0.5 + self.roll().abs() * 0.5
        };
        self.land(&look, a.kind, point, height, toward);
        let sound = if a.kind == Kind::Ram {
            Sound::Impact
        } else {
            Sound::WallHit
        };
        self.noises.push(Noise {
            sound,
            at: point,
            gain: 1.0,
        });
    }

    /// A blow of `kind` lands on building `look` at `at`, `height` over the
    /// ground, travelling along `dir`: it jolts, and chips and dust fly.
    fn land(&mut self, look: &Look, kind: Kind, at: Vec2, height: f32, dir: Vec2) {
        let (chips, puffs, jolt) = kind.force();
        match self.jolts.iter_mut().find(|j| j.handle == look.handle) {
            Some(j) => {
                // A fresh blow restarts the shake, as hard as the harder.
                let left = j.strength * (1.0 - (self.time - j.at) / JOLT_SECONDS).max(0.0);
                *j = Jolt {
                    handle: look.handle,
                    dir,
                    strength: jolt.max(left),
                    at: self.time,
                };
            }
            None => self.jolts.push(Jolt {
                handle: look.handle,
                dir,
                strength: jolt,
                at: self.time,
            }),
        }
        let y = look.base + height;
        let big = matches!(kind, Kind::Boulder | Kind::Ram | Kind::Fire);
        for _ in 0..chips {
            let out = -dir * (1.0 + self.roll().abs() * 2.5)
                + Vec2::new(-dir.y, dir.x) * (self.roll() * 2.0);
            let size = if big {
                0.12 + self.roll().abs() * 0.22
            } else {
                0.06 + self.roll().abs() * 0.08
            };
            let colour = look.stuff.colours[self.roll_index(3)];
            let made = Chip {
                pos: at - dir * 0.15,
                y,
                vel: out,
                vy: 1.5 + self.roll().abs() * 3.0,
                size,
                colour: [colour[0], colour[1], colour[2], look.stuff.glow],
                facing: Vec2::from_angle(self.roll() * 3.2),
                spin: self.roll() * 12.0,
                born: self.time,
                rest: None,
            };
            self.chip(made);
        }
        for _ in 0..puffs {
            let d = look.stuff.dust;
            let drift = -dir * (0.4 + self.roll().abs() * 0.6)
                + Vec2::new(-dir.y, dir.x) * (self.roll() * 0.6);
            let made = Puff {
                pos: at - dir * 0.3,
                y: y - 0.2,
                vel: drift,
                rise: 0.35,
                size: if big { 0.8 } else { 0.45 },
                colour: [d[0], d[1], d[2], 0],
                facing: Vec2::from_angle(self.roll() * 3.2),
                born: self.time,
                life: PUFF_LIFE * (0.8 + self.roll().abs() * 0.4),
            };
            self.puff(made);
        }
        if kind == Kind::Fire {
            // The shot bursts in flame.
            for _ in 0..4 {
                let drift = Vec2::new(self.roll(), self.roll()) * 0.5;
                let glow = [255, 150 + (self.roll().abs() * 70.0) as u8, 40, 230];
                let made = Puff {
                    pos: at - dir * 0.2,
                    y,
                    vel: drift,
                    rise: 0.9,
                    size: 0.5,
                    colour: glow,
                    facing: Vec2::from_angle(self.roll() * 3.2),
                    born: self.time,
                    life: 0.6,
                };
                self.puff(made);
            }
        }
    }

    /// Building `structure` was brought down (an `Event::Fallen`): it
    /// sinks into a cloud of dust and leaves rubble.
    pub fn fallen(&mut self, structure: StructureHandle) {
        let Some(look) = self.look(structure) else {
            return;
        };
        self.looks[structure.index as usize] = None;
        self.jolts.retain(|j| j.handle != structure);
        if self.falls.len() >= MAX_FALLS {
            self.falls.remove(0);
        }
        self.seed = self.seed.wrapping_add(0x9E37_79B9);
        self.falls.push(Fall {
            look,
            born: self.time,
            seed: self.seed,
        });
        // A burst of chips from every side.
        let centre = look.centre();
        let n = (12.0 + look.size().x * look.size().y).min(40.0) as usize;
        for k in 0..n {
            let dir = Vec2::from_angle(k as f32 * 2.4 + self.roll());
            let edge = look.nearest(centre + dir * 100.0);
            let colour = look.stuff.colours[k % 3];
            let made = Chip {
                pos: edge,
                y: look.base + look.top * (0.3 + self.roll().abs() * 0.5),
                vel: dir * (1.0 + self.roll().abs() * 3.0),
                vy: 1.0 + self.roll().abs() * 3.0,
                size: 0.14 + self.roll().abs() * 0.26,
                colour: [colour[0], colour[1], colour[2], look.stuff.glow],
                facing: Vec2::from_angle(self.roll() * 3.2),
                spin: self.roll() * 8.0,
                born: self.time,
                rest: None,
            };
            self.chip(made);
        }
        self.noises.push(Noise {
            sound: Sound::Collapse,
            at: centre,
            gain: 1.0,
        });
    }

    /// Start a frame `dt` seconds after the last; `ground` is the height
    /// of the ground at a point.
    pub fn advance(&mut self, dt: f32, ground: impl Fn(Vec2) -> f32) {
        self.time += dt;
        let now = self.time;
        for c in &mut self.chips {
            if c.rest.is_some() {
                continue;
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
        }
        self.chips
            .retain(|c| now - c.rest.unwrap_or(now) < CHIP_LIFE && now - c.born < CHIP_LIFE * 2.0);
        if self.next_chip > self.chips.len() {
            self.next_chip = 0;
        }
        self.puffs.retain(|p| now - p.born < p.life);
        if self.next_puff > self.puffs.len() {
            self.next_puff = 0;
        }
        // Shots that came down land; fire-pots trail smoke.
        let mut k = 0;
        while k < self.shots.len() {
            let s = self.shots[k];
            let t = (now - s.born) / s.flight;
            if t < 1.0 {
                if s.kind == Kind::Fire && self.roll() > 0.2 {
                    let (pos, y) = s.at(0, t);
                    let made = Puff {
                        pos,
                        y,
                        vel: Vec2::ZERO,
                        rise: 0.3,
                        size: 0.3,
                        colour: [70, 66, 62, 0],
                        facing: Vec2::from_angle(self.roll() * 3.2),
                        born: now,
                        life: 0.9,
                    };
                    self.puff(made);
                }
                k += 1;
                continue;
            }
            self.shots.swap_remove(k);
            let Some(look) = self.look(s.structure) else {
                continue;
            };
            let dir = (s.to - s.from).normalize_or(Vec2::X);
            for a in 0..s.count() {
                let (pos, y) = s.at(a, 1.0);
                self.land(
                    &look,
                    s.kind,
                    look.nearest(pos),
                    (y - look.base).max(0.3),
                    dir,
                );
            }
            let sound = if s.kind == Kind::Arrows {
                Sound::Arrows
            } else {
                Sound::Impact
            };
            self.noises.push(Noise {
                sound,
                at: s.to,
                gain: 1.0,
            });
        }
        // Collapsing buildings throw up dust as they go down.
        let n = self.falls.len();
        for f in 0..n {
            let fall = self.falls[f];
            let age = now - fall.born;
            if age >= COLLAPSE_SECONDS || self.roll() < 0.4 {
                continue;
            }
            let look = fall.look;
            let around = Vec2::from_angle(self.roll() * 3.2);
            let edge = look.nearest(look.centre() + around * 100.0);
            let d = look.stuff.dust;
            let made = Puff {
                pos: edge + around * 0.3,
                y: look.base + self.roll().abs() * look.top * 0.4,
                vel: around * (0.6 + self.roll().abs()),
                rise: 0.5,
                size: 0.9 + self.roll().abs() * 0.8,
                colour: [d[0], d[1], d[2], 0],
                facing: Vec2::from_angle(self.roll() * 3.2),
                born: now,
                life: 2.2,
            };
            self.puff(made);
        }
        self.falls
            .retain(|f| now - f.born < COLLAPSE_SECONDS + RUBBLE_SECONDS);
        self.jolts.retain(|j| now - j.at < JOLT_SECONDS);
    }

    /// The sounds of the frames since this was last called.
    pub fn drain_noises(&mut self) -> impl Iterator<Item = Noise> + '_ {
        self.noises.drain(..)
    }

    /// How far building `h` is shaken off its place now, along the ground
    /// and up.
    pub fn jolt(&self, h: StructureHandle) -> (Vec2, f32) {
        let Some(j) = self.jolts.iter().find(|j| j.handle == h) else {
            return (Vec2::ZERO, 0.0);
        };
        let t = (self.time - j.at) / JOLT_SECONDS;
        if t >= 1.0 {
            return (Vec2::ZERO, 0.0);
        }
        // Pushed along the blow, then rocking back and settling.
        let rock = (t * 3.0 * std::f32::consts::TAU).cos() * (1.0 - t) * (1.0 - t);
        (j.dir * (j.strength * rock), -j.strength * 0.3 * rock.abs())
    }

    /// How white building `h`'s health bar shows from a fresh blow, 0 to 1.
    pub fn flash(&self, h: StructureHandle) -> f32 {
        self.jolts
            .iter()
            .find(|j| j.handle == h)
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
        self.struck
            .get(h.index as usize)
            .copied()
            .flatten()
            .is_some_and(|(s, at)| s == h && self.time - at < SHOOTING_SECONDS)
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

    /// Forget where regiment `h`'s soldiers swung at the wall, before this
    /// frame's are noted.
    pub fn untouch(&mut self, h: RegimentHandle) {
        if let Some(t) = self.touches.get_mut(h.index as usize) {
            *t = Some((h, [Vec2::ZERO; TOUCHES], 0));
        }
    }

    /// A soldier of regiment `h` swings at the wall at `p`.
    pub fn touch(&mut self, h: RegimentHandle, p: Vec2) {
        if let Some(Some(t)) = self.touches.get_mut(h.index as usize)
            && t.0 == h
            && usize::from(t.2) < TOUCHES
        {
            t.1[usize::from(t.2)] = p;
            t.2 += 1;
        }
    }

    /// Fallen buildings' models still sinking.
    pub fn sinking(&self) -> impl Iterator<Item = Sinking> + '_ {
        let now = self.time;
        self.falls.iter().filter_map(move |f| {
            let model = f.look.model?;
            let t = (now - f.born) / COLLAPSE_SECONDS;
            if t >= 1.0 {
                return None;
            }
            // Slow to give, then gone in a rush, shuddering as it goes.
            let down = t * t * (3.0 - 2.0 * t);
            let shudder = (now * 41.0).sin() * 0.06 * (1.0 - t);
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
    /// of the ground at a point. Stops at `limit` boxes.
    pub fn push(
        &self,
        shown: impl Iterator<Item = StructureHandle>,
        ground: impl Fn(Vec2) -> f32,
        limit: usize,
        out: &mut Vec<BoxInstance>,
    ) {
        let now = self.time;
        let room = |out: &Vec<BoxInstance>| out.len() < limit;
        for s in &self.shots {
            let t = ((now - s.born) / s.flight).clamp(0.0, 1.0);
            let dir = (s.to - s.from).normalize_or(Vec2::X);
            for k in 0..s.count() {
                if !room(out) {
                    return;
                }
                let (pos, y) = s.at(k, t);
                let (size, colour, facing) = match s.kind {
                    Kind::Arrows => ([0.05, 0.05, 0.7], [70, 52, 36, 0], dir),
                    Kind::Fire => ([0.3, 0.3, 0.45], [255, 140, 40, 255], dir),
                    _ => (
                        [0.42, 0.38, 0.42],
                        [96, 92, 86, 0],
                        Vec2::from_angle(now * 9.0),
                    ),
                };
                out.push(BoxInstance {
                    pos: pos.to_array(),
                    facing: facing.to_array(),
                    size,
                    color: colour,
                    lift: y - ground(pos),
                });
            }
        }
        for c in &self.chips {
            if !room(out) {
                return;
            }
            let sink = c.rest.map_or(0.0, |r| {
                ((now - r - (CHIP_LIFE - CHIP_SINK)) / CHIP_SINK).max(0.0)
            });
            out.push(BoxInstance {
                pos: c.pos.to_array(),
                facing: c.facing.to_array(),
                size: [c.size, c.size * 0.7, c.size * 0.9],
                color: c.colour,
                lift: c.y - ground(c.pos) - c.size * sink,
            });
        }
        for p in &self.puffs {
            if !room(out) {
                return;
            }
            let t = (now - p.born) / p.life;
            // Swells, then thins away.
            let size = p.size * (0.5 + t * 1.2) * (1.0 - t * t * t);
            let pos = p.pos + p.vel * (now - p.born);
            out.push(BoxInstance {
                pos: pos.to_array(),
                facing: p.facing.to_array(),
                size: [size, -size * 0.8, size],
                color: p.colour,
                lift: p.y + p.rise * (now - p.born) - ground(pos),
            });
        }
        for f in &self.falls {
            let age = now - f.born;
            if age < COLLAPSE_SECONDS * 0.5 {
                continue;
            }
            let rubble = age - COLLAPSE_SECONDS * 0.5;
            let sink = ((rubble - (RUBBLE_SECONDS - RUBBLE_SINK)) / RUBBLE_SINK).clamp(0.0, 1.0);
            let look = f.look;
            let size = look.size();
            let n = (size.x * size.y * 1.2).clamp(8.0, 30.0) as usize;
            for k in 0..n {
                if !room(out) {
                    return;
                }
                let at = look.min
                    + Vec2::new(
                        (hash(f.seed, k as u32, 0) * 0.5 + 0.5) * size.x,
                        (hash(f.seed, k as u32, 1) * 0.5 + 0.5) * size.y,
                    );
                // A heap: higher in the middle.
                let middle = 1.0
                    - ((at - look.centre()) / (size * 0.5).max(Vec2::ONE))
                        .length()
                        .min(1.0);
                let w = 0.35 + (hash(f.seed, k as u32, 2) * 0.5 + 0.5) * 0.7;
                let h = (0.2 + middle * (look.top * 0.18).min(1.2)) * (1.0 - sink);
                let colour = look.stuff.colours[k % 3];
                out.push(BoxInstance {
                    pos: at.to_array(),
                    facing: Vec2::from_angle(hash(f.seed, k as u32, 3) * 3.2).to_array(),
                    size: [w, h.max(0.02), w * 0.8],
                    color: [colour[0], colour[1], colour[2], 0],
                    lift: -0.05 - 0.3 * sink,
                });
            }
            if rubble < SMOULDER_SECONDS {
                let thin = 1.0 - rubble / SMOULDER_SECONDS;
                self.smoke(
                    f.seed,
                    look.centre(),
                    look.base + 0.4,
                    0.7 * thin,
                    1,
                    &ground,
                    out,
                    limit,
                );
            }
        }
        for h in shown {
            let Some(look) = self.look(h) else { continue };
            // A building going up has few hit points yet: it doesn't burn.
            if look.hp >= SMOKES_BELOW || !look.finished {
                continue;
            }
            let area = look.size().x * look.size().y;
            let spots = if area >= 12.0 {
                3
            } else if area >= 4.0 {
                2
            } else {
                1
            };
            let fire = look.hp < BURNS_BELOW;
            let damage = (SMOKES_BELOW - look.hp) / SMOKES_BELOW;
            for spot in 0..spots {
                let seed = h.index.wrapping_mul(0x2545_F491) ^ u32::from(h.generation);
                let off = Vec2::new(hash(seed, spot, 4), hash(seed, spot, 5)) * look.size() * 0.3;
                let at = look.centre() + off;
                let roof = look.base + look.top * 0.8;
                // The second and third column only once badly hurt.
                if spot > 0 && damage < 0.25 * spot as f32 {
                    continue;
                }
                let thick = 0.35 + damage * if fire { 0.55 } else { 0.3 };
                self.smoke(
                    seed ^ spot,
                    at,
                    roof,
                    thick,
                    if fire { 2 } else { 1 },
                    &ground,
                    out,
                    limit,
                );
                if fire {
                    self.flames(seed ^ spot, at, roof, &ground, out, limit);
                }
            }
        }
    }

    /// A column of smoke from `at`, `y` high: `thick` sizes its puffs, and
    /// `dark` (1 or 2) darkens them (a fire's smoke is black).
    #[allow(clippy::too_many_arguments)]
    fn smoke(
        &self,
        seed: u32,
        at: Vec2,
        y: f32,
        thick: f32,
        dark: u8,
        ground: &impl Fn(Vec2) -> f32,
        out: &mut Vec<BoxInstance>,
        limit: usize,
    ) {
        let grey: u8 = if dark > 1 { 64 } else { 110 };
        for k in 0..SMOKE_PUFFS {
            if out.len() >= limit {
                return;
            }
            let offset = k as f32 / SMOKE_PUFFS as f32 + hash(seed, k as u32, 6) * 0.05;
            let t = (self.time / SMOKE_LIFE + offset).fract();
            let size = thick * (0.45 + t * 1.3) * (1.0 - t * t);
            let pos = at
                + WIND * (t * SMOKE_LIFE)
                + Vec2::new(hash(seed, k as u32, 7), hash(seed, k as u32, 8)) * 0.2;
            let shade = grey.saturating_add((t * 60.0) as u8);
            out.push(BoxInstance {
                pos: pos.to_array(),
                facing: Vec2::from_angle(t * 2.0 + k as f32).to_array(),
                size: [size, -size * 0.85, size],
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
        limit: usize,
    ) {
        for k in 0..FLAMES {
            if out.len() >= limit {
                return;
            }
            let phase = hash(seed, k as u32, 9) * 6.0;
            let flicker =
                0.75 + 0.25 * (self.time * 11.0 + phase).sin() * (self.time * 7.3 + phase).cos();
            let size = (0.3 + 0.15 * k as f32 % 0.45) * flicker;
            let pos = at + Vec2::new(hash(seed, k as u32, 10), hash(seed, k as u32, 11)) * 0.35;
            let colour = if k % 2 == 0 {
                [255, 150, 40, 255]
            } else {
                [255, 206, 90, 255]
            };
            out.push(BoxInstance {
                pos: pos.to_array(),
                facing: Vec2::from_angle(self.time * 2.0 + phase).to_array(),
                size: [size, -size * 1.6, size],
                color: colour,
                lift: y - 0.15 + 0.1 * (self.time * 5.0 + phase).sin() - ground(pos),
            });
        }
    }

    fn chip(&mut self, c: Chip) {
        if self.chips.len() < MAX_CHIPS {
            self.chips.push(c);
        } else {
            self.chips[self.next_chip] = c;
            self.next_chip = (self.next_chip + 1) % MAX_CHIPS;
        }
    }

    fn puff(&mut self, p: Puff) {
        if self.puffs.len() < MAX_PUFFS {
            self.puffs.push(p);
        } else {
            self.puffs[self.next_puff] = p;
            self.next_puff = (self.next_puff + 1) % MAX_PUFFS;
        }
    }

    /// A value in [-1, 1), different each call.
    fn roll(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }

    /// One of `0..n`, different each call.
    fn roll_index(&mut self, n: usize) -> usize {
        (((self.roll() + 1.0) * 0.5 * n as f32) as usize).min(n.saturating_sub(1))
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
            colour: [200, 0, 0],
            stuff: stuff(Some(Faction::EmberClans), Armour::Structure),
            hp,
            finished: true,
        }
    }

    fn blade(at: Vec2) -> Attacker {
        Attacker {
            handle: regiment(3),
            at,
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
        s.untouch(a.handle);
        s.touch(a.handle, Vec2::new(11.0, 13.0));
        s.struck(h, a);
        assert!(
            s.chips
                .iter()
                .all(|c| (c.pos - Vec2::new(11.0, 13.15)).length() < 0.01)
        );
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
        assert_eq!(out.len(), ARROWS);
        for _ in 0..30 {
            s.advance(0.05, flat);
        }
        assert!(s.shots.is_empty());
        assert_eq!(s.chips.len(), ARROWS);
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
