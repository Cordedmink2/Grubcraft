//! Soldier puppets: the presentation layer that turns each regiment block from
//! the simulation into individual soldiers (docs/10-architecture.md, "The
//! two-tier simulation").
//!
//! Every soldier steers toward its formation slot with a little lag and its
//! own pace, so blocks ripple and jostle instead of sliding rigidly. Soldiers
//! speed up and slow down rather than snapping to full pace, the front rank
//! sets off before the rear, a halting block overshoots a touch and settles,
//! and a regiment at rest sways and looks about. In melee each front-rank
//! soldier pairs off with the nearest enemy across the line and the two trade
//! blows (docs/09-art-and-audio.md, "Animation"). Each blow is parried,
//! blocked, staggers its man or crushes through his guard, with the odds the
//! simulation uses; a heavier charge knocks men off their feet; and when the
//! simulation takes a soldier, his man deals the killing blow. When the
//! simulation calls out champions before a battle, one man steps out of each
//! champion's regiment, runs to the spot between the lines and plays out
//! the duel the simulation rolled, blow by blow. None of this feeds back
//! into the simulation, so it may use floats and frame time.

use gc_sim::combat::{CHARGE_TICKS, hit_percent};
use gc_sim::formation::MAX_MODELS;
use gc_sim::{Challenge, Mass, RegimentHandle, TICKS_PER_SECOND};
use glam::Vec2;

const SLOTS: usize = MAX_MODELS as usize;
/// How far a soldier may stand from its exact slot, in tiles, so ranks don't
/// look ruled with a straight edge.
const SLOT_JITTER: f32 = 0.07;
/// Soldiers jog faster than the regiment marches so they can catch up.
const CATCH_UP: f32 = 1.35;
/// Extra speed per tile of distance from the slot, so stragglers close gaps.
const PULL_PER_TILE: f32 = 1.5;
/// Below this distance a soldier is standing in its slot.
const SETTLED: f32 = 0.03;
/// Turn speed for a soldier's facing, radians per second.
const TURN_RATE: f32 = 6.0;
/// How far a soldier in a fighting front rank lunges forward, in tiles, and
/// how many lunges a second.
const LUNGE: f32 = 0.18;
const LUNGE_RATE: f32 = 1.6;
/// How quickly a front-rank soldier gains or sheds speed, in tiles per second
/// squared; each rank further back is slower to react, so a block stretches a
/// little as it sets off and bunches as it halts.
const ACCEL: f32 = 10.0;
const ACCEL_FALLOFF_PER_RANK: f32 = 0.3;
/// Near his slot a soldier is pulled in like a spring that is a little short
/// of fully damped (natural frequency in radians per second, and damping
/// ratio), which gives the small overshoot and settle.
const SPRING: f32 = 4.0;
const DAMPING: f32 = 0.75;
/// How much of the block's own pace a soldier matches outright; the rest he
/// makes up by trailing his slot slightly, which softens the halt.
const LEAD: f32 = 0.6;
/// At rest: side-to-side sway in tiles, and how far a soldier glances aside,
/// in radians.
const SWAY: f32 = 0.025;
const GLANCE: f32 = 0.22;
/// Duel pairs: a front-rank soldier looks for an enemy within this reach, in
/// tiles, and keeps him until he is this far away.
const DUEL_REACH: f32 = 2.6;
const DUEL_KEEP: f32 = 3.4;
/// Paired soldiers close to this gap, in tiles, but stray no further than
/// the leash from their own slot.
const DUEL_GAP: f32 = 0.85;
const DUEL_LEASH: f32 = 0.6;
/// The step in and back as a pair trades blows, in tiles, and its rate.
const DUEL_STEP: f32 = 0.16;
const DUEL_RATE: f32 = 0.8;
/// A swing takes this long, in seconds, and its blow lands this far into it.
const SWING_SECONDS: f32 = 0.7;
const BLOW_AT: f32 = 0.45;
/// The same as fractions of a pair's round of blows.
const SWING_ROUND: f32 = SWING_SECONDS * DUEL_RATE;
const BLOW_ROUND: f32 = SWING_ROUND * BLOW_AT;
/// How long a parry or shield block, a stagger and a knockdown (falling,
/// lying and getting up) last, in seconds.
const PARRY_SECONDS: f32 = 0.4;
const STAGGER_SECONDS: f32 = 0.6;
const DOWN_SECONDS: f32 = 1.8;
/// For each class of mass a heavy striker has over his man (a big swinging
/// blade counts one class heavier), the chance a blow he blocks crushes
/// through and knocks him down.
const CRUSH_PER_CLASS: f32 = 0.25;
/// For each class of mass a heavy charging regiment has over its foe, the
/// chance its impact knocks a man off his feet; a man left standing
/// staggers this often.
const KNOCKDOWN_PER_CLASS: f32 = 0.4;
const CHARGE_STAGGER: f32 = 0.5;
/// Soldiers who pair off while their regiment has more than this many ticks
/// of charge bonus left (within 0.6 s of the charge hitting home) deliver
/// its impact.
const IMPACT_TICKS: u16 = CHARGE_TICKS - 12;
/// How hard a blow shoves its man back, in tiles per second.
const STAGGER_SHOVE: f32 = 0.8;
const DOWN_SHOVE: f32 = 2.4;
/// Regiments this far apart, centre to centre, are never paired.
const DUEL_REGIMENT_RANGE: f32 = 24.0;
/// A regiment not updated for this long, in seconds, is gone.
const STALE: f32 = 0.5;
/// No partner.
const NONE: u32 = u32::MAX;
/// A routing regiment's soldiers spread this much further apart (as a
/// fraction of their spacing) and this much further off their slots, in tiles.
const ROUT_SPREAD: f32 = 0.8;
const ROUT_JITTER: f32 = 0.45;
/// A regiment fully out of order (pushing through friends): ranks open up by
/// this fraction, soldiers stand this much further off their slots, in
/// tiles, and drift about them by this much more, at this rate, looking
/// this far either way (radians).
const DISORDER_SPREAD: f32 = 0.25;
const DISORDER_JITTER: f32 = 0.3;
const DISORDER_DRIFT: f32 = 0.22;
const DISORDER_RATE: f32 = 0.7;
const DISORDER_LOOK: f32 = 0.9;
const DISORDER_LOOK_RATE: f32 = 0.8;
/// Fallen soldiers stay on the field this long, in seconds, then sink away.
const CORPSE_SECONDS: f32 = 20.0;
const CORPSE_SINK_SECONDS: f32 = 3.0;
/// Champions run out at this pace, in tiles per second: a little faster
/// than the simulation allows for, so they are there when the duel starts.
const CHAMPION_RUN: f32 = 5.5;
/// The winning champion cheers this long before he walks back, in seconds.
const CHEER_SECONDS: f32 = 2.5;

/// One regiment as the renderer sees it this frame, already interpolated.
#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub handle: RegimentHandle,
    /// Unit type, so the fallen keep their model.
    pub unit: u16,
    pub centre: Vec2,
    /// Unit vector of the regiment's facing.
    pub facing: Vec2,
    pub files: u16,
    pub models: u16,
    pub file_spacing: f32,
    pub rank_spacing: f32,
    /// Marching speed in tiles per second.
    pub speed: f32,
    /// Fighting in melee: the front ranks lunge at the enemy.
    pub engaged: bool,
    /// Running away: the ranks come apart.
    pub routing: bool,
    /// Out of order from pushing through friends, 0 (in order) to 1 (fully
    /// disordered): the ranks break up, soldiers mill and look about.
    pub disorder: f32,
    /// Ticks of charge bonus left (`RegimentView::charge`), counting down
    /// from `CHARGE_TICKS` as a charge hits home.
    pub charge: u16,
    pub fighter: Fighter,
}

/// How a regiment's soldiers fight, from its unit's stats and state, which
/// decides what their blows do on screen.
#[derive(Clone, Copy, Debug)]
pub struct Fighter {
    /// Melee attack, and melee defence in its formation against blows on
    /// its front and on its flanks or rear.
    pub attack: i32,
    pub defence: i32,
    pub flank_defence: i32,
    /// Percent of the usual share of blows that land: fewer for a wavering
    /// regiment or one without room for its weapons.
    pub sharpness: u32,
    pub shield: bool,
    pub mass: Mass,
    /// Big swinging blades, which hit a guard like a heavier man.
    pub heavy_blade: bool,
}

impl Default for Fighter {
    fn default() -> Fighter {
        Fighter {
            attack: 0,
            defence: 0,
            flank_defence: 0,
            sharpness: 100,
            shield: false,
            mass: Mass::Medium,
            heavy_blade: false,
        }
    }
}

impl Fighter {
    /// The chance (0 to 1) a blow gets through a guard of `defence`: the
    /// simulation's chance to hit.
    fn lands(&self, defence: i32) -> f32 {
        hit_percent(self.attack, defence) as f32 * self.sharpness as f32 / 10_000.0
    }

    /// Classes of mass a heavy man has over `them`; with `blade`, a big
    /// swinging blade makes its wielder heavy, and one class heavier. None
    /// for lighter men: only monsters, cavalry and great blades bowl men
    /// over.
    fn outweighs(&self, them: &Fighter, blade: bool) -> f32 {
        let blade = blade && self.heavy_blade;
        if self.mass < Mass::Heavy && !blade {
            return 0.0;
        }
        (self.mass as i32 + i32::from(blade) - them.mass as i32).max(0) as f32
    }
}

/// One soldier ready to draw.
#[derive(Clone, Copy, Debug)]
pub struct Soldier {
    pub pos: Vec2,
    pub facing: Vec2,
    /// How fast he moved this frame, in tiles per second.
    pub speed: f32,
    /// Moving against the way he faces: giving ground with his face to the
    /// enemy, so he steps backwards.
    pub backing: bool,
    /// In a fighting front rank, striking at the enemy.
    pub striking: bool,
    /// What he is doing in a duel: paired with an enemy soldier and trading
    /// blows, or still reeling from one. `None` out of a duel.
    pub fight: Option<Move>,
    /// A stable value in [0, 1) per soldier, to keep animations out of step.
    pub phase: f32,
}

/// A dueling soldier's move, with how far through it he is (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Move {
    /// Weapon up, waiting for an opening.
    Guard,
    /// Swinging at his man; `alt` picks his second attack.
    Swing { at: f32, alt: bool },
    /// Turning a blow aside with his weapon.
    Parry(f32),
    /// Catching a blow on his shield.
    Block(f32),
    /// Hit and reeling, which costs him his next swing.
    Stagger(f32),
    /// Knocked off his feet by a charge or a crushing blow, and getting up.
    Down(f32),
    /// A champion who has won his duel.
    Cheer,
}

/// What a blow did to a soldier, or the killing blow he is dealing, which
/// overrides his usual round of swings until it ends. Later variants
/// outrank earlier ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Act {
    None,
    Parry,
    Block,
    Stagger,
    Down,
    Kill,
}

impl Act {
    fn seconds(self) -> f32 {
        match self {
            Act::None => 0.0,
            Act::Parry | Act::Block => PARRY_SECONDS,
            Act::Stagger => STAGGER_SECONDS,
            Act::Down => DOWN_SECONDS,
            Act::Kill => SWING_SECONDS,
        }
    }

    /// Whether it knocks him out of his next swing.
    fn reels(self) -> bool {
        matches!(self, Act::Stagger | Act::Down)
    }
}

/// A soldier lying where he fell.
#[derive(Clone, Copy, Debug)]
pub struct Corpse {
    pub pos: Vec2,
    pub facing: Vec2,
    pub owner: u8,
    pub unit: u16,
    /// Seconds since he fell; negative while he still stands, waiting for
    /// the killing blow.
    pub age: f32,
    /// How far it has sunk into the ground, in tiles (0 while fresh).
    pub sink: f32,
}

/// A champion's moves at a moment of his duel: what he is doing outside his
/// swings, as [`Puppets::acting`] gives it, and his swing (how far through
/// it, and which of his swings it is).
type DuelMoves = (Option<(Act, f32)>, Option<(f32, f32)>);

/// The champions' duel the simulation called, as the puppets play it.
#[derive(Clone, Copy, Debug)]
struct Duel {
    challenge: Challenge,
    /// Where they fight.
    spot: Vec2,
    /// The puppet each side sent out, or `NONE` (none to send, or fallen).
    champion: [u32; 2],
    /// The simulation's time, in ticks (interpolated between them).
    now: f32,
}

impl Duel {
    /// Whether champion `side` is out between the lines: until the duel is
    /// abandoned, the loser falls, they part, or the winner has cheered.
    fn out(&self, side: usize) -> bool {
        let c = &self.challenge;
        let ends = c.ends() as f32;
        c.abandoned.is_none()
            && match c.winner {
                Some(w) if w as usize == side => {
                    self.now < ends + CHEER_SECONDS * TICKS_PER_SECOND as f32
                }
                // He falls when the simulation takes him (see `fall`).
                Some(_) => self.now < ends + TICKS_PER_SECOND as f32,
                None => self.now < ends,
            }
    }

    /// What champion `side` is doing now, by the rolled duel, as a soldier
    /// in a melee duel would: reeling from each of the other's blows that
    /// lands and parrying (or, with a shield, blocking) the rest, and his
    /// own swing on each of his beats (how far through it, and which of his
    /// swings it is).
    fn moves(&self, side: usize, shield: bool) -> DuelMoves {
        let c = &self.challenge;
        let (mut act, mut swing) = (None, None);
        let since_meet = self.now - c.meet as f32;
        if since_meet < 0.0 || self.now >= c.ends() as f32 {
            return (act, swing);
        }
        let beat = (since_meet / gc_sim::champions::BEAT_TICKS as f32) as u8;
        // A blow's moves last less than the two beats between one man's
        // swings, so only the beats either side of this one matter.
        for b in beat.saturating_sub(1)..=beat.saturating_add(1) {
            if !c.swings(b) {
                continue;
            }
            // Seconds since the blow of beat `b` landed (negative before).
            let since = (self.now - c.blow_tick(b) as f32) / TICKS_PER_SECOND as f32;
            if b as usize % 2 == side {
                let at = since / SWING_SECONDS + BLOW_AT;
                if (0.0..1.0).contains(&at) {
                    swing = Some((at, f32::from(b / 2)));
                }
            } else {
                let a = match () {
                    _ if c.lands(b) => Act::Stagger,
                    _ if shield => Act::Block,
                    _ => Act::Parry,
                };
                let f = since / a.seconds();
                if (0.0..1.0).contains(&f) {
                    act = Some((a, f));
                }
            }
        }
        (act, swing)
    }

    /// Whether champion `side` has won and is cheering.
    fn cheering(&self, side: usize) -> bool {
        self.challenge.winner == Some(side as u8) && self.now >= self.challenge.ends() as f32
    }
}

/// Puppet state for every regiment slot in the pool: `MAX_MODELS` soldiers
/// per regiment, by pool index. Allocated once.
pub struct Puppets {
    owner: Vec<Option<RegimentHandle>>,
    /// Soldiers each regiment had last frame, to notice casualties.
    models: Vec<u16>,
    /// Last frame's centre, player, fighting state and files, for pairing.
    centre: Vec<Vec2>,
    /// Each regiment's own velocity, smoothed, so soldiers keep pace with
    /// a marching block instead of trailing it.
    drift: Vec<Vec2>,
    player: Vec<u8>,
    engaged: Vec<bool>,
    files: Vec<u16>,
    fighter: Vec<Fighter>,
    charge: Vec<u16>,
    /// Each regiment's facing, and half its width and depth, to tell which
    /// side a blow falls on.
    heading: Vec<Vec2>,
    half: Vec<Vec2>,
    /// When each regiment was last updated, to skip ones that are gone.
    seen: Vec<f32>,
    pos: Vec<Vec2>,
    vel: Vec<Vec2>,
    facing: Vec<Vec2>,
    /// Each soldier's duel partner, a puppet index, or `NONE`.
    partner: Vec<u32>,
    /// What each soldier last suffered or dealt outside his round of swings,
    /// and when; and until when a blow has him reeling, which costs him any
    /// swing begun before then.
    act: Vec<(Act, f32)>,
    reeling: Vec<f32>,
    /// Scratch for pairing: regiments in melee, and the enemies near one.
    fighting: Vec<u32>,
    near: Vec<u32>,
    /// Fallen soldiers, a ring buffer; the oldest are reused first.
    corpses: Vec<(Vec2, Vec2, u8, u16, f32)>,
    corpse_capacity: usize,
    next_corpse: usize,
    time: f32,
    /// The last frame's step, in seconds.
    dt: f32,
    duel: Option<Duel>,
    /// The tick the duel the puppets are done with was called.
    duel_over: Option<u32>,
}

impl Puppets {
    pub fn new(regiment_capacity: usize, corpse_capacity: usize) -> Puppets {
        Puppets {
            owner: vec![None; regiment_capacity],
            models: vec![0; regiment_capacity],
            centre: vec![Vec2::ZERO; regiment_capacity],
            drift: vec![Vec2::ZERO; regiment_capacity],
            player: vec![0; regiment_capacity],
            engaged: vec![false; regiment_capacity],
            files: vec![1; regiment_capacity],
            fighter: vec![Fighter::default(); regiment_capacity],
            charge: vec![0; regiment_capacity],
            heading: vec![Vec2::NEG_Y; regiment_capacity],
            half: vec![Vec2::ONE; regiment_capacity],
            seen: vec![f32::NEG_INFINITY; regiment_capacity],
            pos: vec![Vec2::ZERO; regiment_capacity * SLOTS],
            vel: vec![Vec2::ZERO; regiment_capacity * SLOTS],
            facing: vec![Vec2::NEG_Y; regiment_capacity * SLOTS],
            partner: vec![NONE; regiment_capacity * SLOTS],
            act: vec![(Act::None, 0.0); regiment_capacity * SLOTS],
            reeling: vec![f32::NEG_INFINITY; regiment_capacity * SLOTS],
            fighting: Vec::with_capacity(regiment_capacity),
            near: Vec::with_capacity(regiment_capacity),
            corpses: Vec::with_capacity(corpse_capacity.max(1)),
            corpse_capacity: corpse_capacity.max(1),
            next_corpse: 0,
            time: 0.0,
            dt: 0.0,
            duel: None,
            duel_over: None,
        }
    }

    /// Game seconds since the puppets started (stops while paused).
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Start a new frame `dt` seconds after the last one.
    pub fn advance(&mut self, dt: f32) {
        self.time += dt;
        self.dt = dt;
    }

    /// Pair front-rank soldiers of fighting regiments with the nearest enemy
    /// across the line, from last frame's positions, and play out the blows
    /// that land this frame. A soldier keeps his partner while the partner
    /// stands within reach; several may pick the same man. `hostile` says
    /// whether two players fight. Call once a frame, before the updates.
    pub fn pair(&mut self, hostile: impl Fn(u8, u8) -> bool) {
        let mut fighting = core::mem::take(&mut self.fighting);
        let mut near = core::mem::take(&mut self.near);
        fighting.clear();
        for r in 0..self.owner.len() {
            if self.live(r) && self.engaged[r] {
                fighting.push(r as u32);
            } else {
                self.partner[r * SLOTS..(r + 1) * SLOTS].fill(NONE);
            }
        }
        for &a in &fighting {
            let a = a as usize;
            near.clear();
            near.extend(fighting.iter().copied().filter(|&b| {
                let b = b as usize;
                b != a
                    && hostile(self.player[a], self.player[b])
                    && self.centre[a].distance_squared(self.centre[b])
                        <= DUEL_REGIMENT_RANGE * DUEL_REGIMENT_RANGE
            }));
            let front = self.front(a);
            let impact = self.charge[a] > IMPACT_TICKS;
            for k in 0..SLOTS {
                let i = a * SLOTS + k;
                if k >= front || near.is_empty() {
                    self.partner[i] = NONE;
                    continue;
                }
                let j = self.partner[i];
                if j == NONE || !self.can_duel(a, j as usize, self.pos[i], &near) {
                    let foe = self.nearest_foe(self.pos[i], &near);
                    self.partner[i] = foe;
                    if foe != NONE && impact {
                        self.charge_home(i, foe as usize);
                    }
                }
                self.strike(i);
            }
        }
        self.fighting = fighting;
        self.near = near;
    }

    /// Follow the champions' duel, if the simulation has called one; `now`
    /// is its time in ticks, interpolated. Each side sends out the man in
    /// the middle of its regiment's front rank. Call once a frame, before
    /// the updates.
    pub fn champions(&mut self, challenge: Option<&Challenge>, now: f32) {
        let Some(&challenge) = challenge else {
            (self.duel, self.duel_over) = (None, None);
            return;
        };
        if self.duel_over == Some(challenge.called) {
            return;
        }
        let duel = match self.duel {
            Some(d) => Duel {
                challenge,
                now,
                ..d
            },
            None => Duel {
                challenge,
                spot: crate::from_fx_vec(challenge.spot),
                champion: challenge.champions.map(|h| {
                    let r = h.index as usize;
                    if self.owner[r] != Some(h) || self.models[r] == 0 {
                        return NONE;
                    }
                    (r * SLOTS + self.front(r) / 2) as u32
                }),
                now,
            },
        };
        // Once neither is out between the lines, the puppets are done with it.
        if duel.out(0) || duel.out(1) {
            self.duel = Some(duel);
        } else {
            self.duel = None;
            self.duel_over = Some(challenge.called);
        }
    }

    /// Regiment `r`'s champion while he is out between the lines: the
    /// duel, his side and his puppet.
    fn champion_of(&self, r: usize) -> Option<(Duel, usize, usize)> {
        let d = self.duel?;
        let side = d
            .challenge
            .champions
            .iter()
            .position(|h| h.index as usize == r)?;
        let i = d.champion[side];
        (i != NONE && d.out(side)).then_some((d, side, i as usize))
    }

    /// Where champion `side` stands to fight: beside the spot, on his own
    /// side of it.
    fn stance(&self, d: &Duel, side: usize) -> Vec2 {
        let [a, b] = d.challenge.champions.map(|h| self.centre[h.index as usize]);
        let toward_b = (b - a).normalize_or(Vec2::Y);
        let half = toward_b * (DUEL_GAP / 2.0);
        if side == 0 {
            d.spot - half
        } else {
            d.spot + half
        }
    }

    /// The nearest front-rank soldier of the regiments `near` within reach
    /// of `me`, or `NONE`.
    fn nearest_foe(&self, me: Vec2, near: &[u32]) -> u32 {
        let mut best = (NONE, DUEL_REACH * DUEL_REACH);
        for &b in near {
            let b = b as usize;
            for j in b * SLOTS..b * SLOTS + self.front(b) {
                let d = me.distance_squared(self.pos[j]);
                if d < best.1 {
                    best = (j as u32, d);
                }
            }
        }
        best.0
    }

    /// Where puppet `i`, dueling puppet `j`, is at time `t` in his rounds of
    /// blows, one swing a round: the whole part counts rounds, the fraction
    /// is how much of this one has gone. A pair keeps half a round out of
    /// step.
    fn beat(i: usize, j: usize, t: f32) -> f32 {
        let (lo, hi) = (i.min(j), i.max(j));
        let half = if i == lo { 0.0 } else { 0.5 };
        t * DUEL_RATE + roll(lo, hi, 0, 0) + half
    }

    /// How far through a swing puppet `i` is at `beat`, or `None` between
    /// swings or when a blow has knocked him out of this one.
    fn swing(&self, i: usize, beat: f32) -> Option<f32> {
        let gone = beat.fract();
        let began = self.time - gone / DUEL_RATE;
        let at = gone / SWING_ROUND;
        (at < 1.0 && self.reeling[i] <= began).then_some(at)
    }

    /// What puppet `i` is doing outside his round of swings, and how far
    /// through it he is.
    fn acting(&self, i: usize) -> Option<(Act, f32)> {
        let (act, at) = self.act[i];
        if act == Act::None {
            return None;
        }
        let f = (self.time - at) / act.seconds();
        (0.0..1.0).contains(&f).then_some((act, f))
    }

    /// If puppet `i`'s swing landed on his man since last frame, it does so
    /// with the simulation's odds: it gets through and staggers him, or a
    /// heavier striker crushes through his guard and knocks him down, or he
    /// parries it or takes it on his shield.
    fn strike(&mut self, i: usize) {
        let j = self.partner[i];
        if j == NONE || matches!(self.acting(i), Some((Act::Kill, _))) {
            return;
        }
        let j = j as usize;
        let beat = Self::beat(i, j, self.time);
        let blow = (beat - BLOW_ROUND).floor();
        let landed = blow > (beat - self.dt * DUEL_RATE - BLOW_ROUND).floor();
        if !landed || self.swing(i, beat).is_none() {
            return;
        }
        let blow = blow as i32 as u32;
        let (me, them) = (self.fighter[i / SLOTS], self.fighter[j / SLOTS]);
        let defence = if self.on_front(j / SLOTS, self.centre[i / SLOTS]) {
            them.defence
        } else {
            them.flank_defence
        };
        let act = if roll(i, j, blow, 1) < me.lands(defence) {
            Act::Stagger
        } else if roll(i, j, blow, 2) < CRUSH_PER_CLASS * me.outweighs(&them, true) {
            Act::Down
        } else if them.shield {
            Act::Block
        } else {
            Act::Parry
        };
        self.suffer(j, i, act);
    }

    /// Whether `from` lies off regiment `r`'s front rather than its flanks
    /// or rear, by the simulation's rule: the side of the block facing it.
    fn on_front(&self, r: usize, from: Vec2) -> bool {
        let d = from - self.centre[r];
        let along = d.dot(self.heading[r]);
        let across = d.perp_dot(self.heading[r]);
        along >= 0.0 && along * self.half[r].x >= across.abs() * self.half[r].y
    }

    /// Puppet `i`'s charge hits home on `j`: a heavier charger may knock him
    /// off his feet, and may stagger him if not.
    fn charge_home(&mut self, i: usize, j: usize) {
        let (me, them) = (self.fighter[i / SLOTS], self.fighter[j / SLOTS]);
        let knock = KNOCKDOWN_PER_CLASS * me.outweighs(&them, false);
        let r = roll(i, j, 0, 3);
        if r < knock {
            self.suffer(j, i, Act::Down);
        } else if r < knock + (1.0 - knock) * CHARGE_STAGGER {
            self.suffer(j, i, Act::Stagger);
        }
    }

    /// Puppet `j` takes `act` from puppet `from`'s blow, unless he is already
    /// in the middle of something stronger or lying down; a stagger or
    /// knockdown shoves him back.
    fn suffer(&mut self, j: usize, from: usize, act: Act) {
        if self
            .acting(j)
            .is_some_and(|(now, _)| now > act || now == Act::Down)
        {
            return;
        }
        self.act[j] = (act, self.time);
        if act.reels() {
            self.reeling[j] = self.time + act.seconds();
        }
        let shove = match act {
            Act::Stagger => STAGGER_SHOVE,
            Act::Down => DOWN_SHOVE,
            _ => return,
        };
        self.vel[j] += (self.pos[j] - self.pos[from]).normalize_or_zero() * shove;
    }

    /// Whether regiment `r` was updated recently and is still in play.
    fn live(&self, r: usize) -> bool {
        self.owner[r].is_some() && self.time - self.seen[r] <= STALE
    }

    /// How many of regiment `r`'s soldiers stand in its front rank.
    fn front(&self, r: usize) -> usize {
        (self.files[r] as usize).min(self.models[r] as usize)
    }

    /// Whether the soldier at `me` in regiment `a` can go on fighting puppet
    /// `j`: still standing in the front rank of a nearby enemy regiment
    /// (`near`), and within reach.
    fn can_duel(&self, a: usize, j: usize, me: Vec2, near: &[u32]) -> bool {
        let b = j / SLOTS;
        b != a
            && near.contains(&(b as u32))
            && j % SLOTS < self.front(b)
            && me.distance_squared(self.pos[j]) <= DUEL_KEEP * DUEL_KEEP
    }

    /// Advance one regiment's soldiers and hand each to `out`. `owner` is the
    /// regiment's player, to colour anyone who falls.
    pub fn update(&mut self, block: &Block, owner: u8, dt: f32, mut out: impl FnMut(Soldier)) {
        let r = block.handle.index as usize;
        let models = block.models.min(MAX_MODELS) as usize;
        let fresh = self.owner[r] != Some(block.handle);
        self.owner[r] = Some(block.handle);
        if fresh {
            self.partner[r * SLOTS..(r + 1) * SLOTS].fill(NONE);
            self.act[r * SLOTS..(r + 1) * SLOTS].fill((Act::None, 0.0));
            self.reeling[r * SLOTS..(r + 1) * SLOTS].fill(f32::NEG_INFINITY);
        } else {
            self.fall(block, owner, models);
        }
        self.charge[r] = block.charge;
        self.fighter[r] = block.fighter;
        self.models[r] = models as u16;
        self.drift[r] = if fresh || dt <= 0.0 {
            Vec2::ZERO
        } else {
            let now = (block.centre - self.centre[r]) / dt;
            self.drift[r].lerp(now, (dt * 8.0).min(1.0))
        };
        self.centre[r] = block.centre;
        self.player[r] = owner;
        self.engaged[r] = block.engaged;
        let files = block.files.clamp(1, block.models.max(1));
        self.files[r] = files;
        self.seen[r] = self.time;
        self.heading[r] = block.facing;
        let ranks = block.models.div_ceil(files).max(1);
        self.half[r] = Vec2::new(
            f32::from(files) * block.file_spacing,
            f32::from(ranks) * block.rank_spacing,
        ) / 2.0;
        let right = Vec2::new(-block.facing.y, block.facing.x);
        let back = -block.facing;
        let tau = core::f32::consts::TAU;
        // The sim never has a router in disorder.
        let mess = block.disorder.clamp(0.0, 1.0);
        let (spread, scatter) = if block.routing {
            (1.0 + ROUT_SPREAD, SLOT_JITTER + ROUT_JITTER)
        } else {
            (
                1.0 + DISORDER_SPREAD * mess,
                SLOT_JITTER + DISORDER_JITTER * mess,
            )
        };
        let champion_here = self.champion_of(r);
        for k in 0..models {
            let i = r * SLOTS + k;
            let local = slot_offset(
                k as u16,
                files,
                block.models,
                block.file_spacing,
                block.rank_spacing,
            ) * spread
                + jitter(block.handle, k) * scatter;
            let mut slot = block.centre + right * local.x + back * local.y;
            if mess > 0.0 {
                // Jostled about: each soldier wanders round his slot.
                let p = unit_hash(block.handle, k, 7) * tau;
                let wander = Vec2::from_angle(self.time * DISORDER_RATE * tau + p);
                slot += wander * (DISORDER_DRIFT * mess * (0.5 + unit_hash(block.handle, k, 8)));
            }
            // A champion out between the lines faces the other; a dueling
            // soldier closes on his man, on a leash from his slot; one
            // knocked down lies where he fell.
            let champion = champion_here
                .filter(|&(_, _, c)| c == i)
                .map(|(d, side, _)| (d, side));
            let j = self.partner[i];
            let foe = match champion {
                Some((d, side)) => match d.champion[1 - side] {
                    NONE => Some(d.spot),
                    other => Some(self.pos[other as usize]),
                },
                None => (j != NONE && !block.routing).then(|| self.pos[j as usize]),
            };
            // A champion's moves come from the rolled duel.
            let duel = champion.map(|(d, side)| d.moves(side, block.fighter.shield));
            let acting = match duel {
                Some((act, _)) => act,
                None => self.acting(i).filter(|_| !block.routing),
            };
            let down = matches!(acting, Some((Act::Down, _)));
            let target = match (champion, foe) {
                (Some((d, side)), _) => self.stance(&d, side),
                _ if down => self.pos[i],
                (None, Some(f)) => {
                    let gap = (self.pos[i] - f).normalize_or(back);
                    let spot = f + gap * DUEL_GAP;
                    slot + (spot - slot).clamp_length_max(DUEL_LEASH)
                }
                (None, None) => slot,
            };
            if fresh {
                self.pos[i] = target;
                self.vel[i] = Vec2::ZERO;
                self.facing[i] = block.facing;
            } else if dt > 0.0 {
                let to = target - self.pos[i];
                let dist = to.length();
                let pace = 0.9 + 0.2 * unit_hash(block.handle, k, 1);
                let top = if champion.is_some() {
                    CHAMPION_RUN
                } else {
                    (block.speed * CATCH_UP + dist * PULL_PER_TILE) * pace
                };
                let rank = f32::from(k as u16 / files);
                let accel = ACCEL * pace / (1.0 + rank * ACCEL_FALLOFF_PER_RANK);
                let accel = if block.routing { accel * 1.5 } else { accel };
                let v = self.vel[i];
                let pull = if dist > SETTLED { to } else { Vec2::ZERO };
                let carry = if down || champion.is_some() {
                    Vec2::ZERO
                } else {
                    self.drift[r] * LEAD
                };
                let push = pull * (SPRING * SPRING) - (v - carry) * (2.0 * DAMPING * SPRING);
                self.vel[i] = (v + push.clamp_length_max(accel) * dt).clamp_length_max(top);
                self.pos[i] += self.vel[i] * dt;
                // Face your man, or where you walk when it's more than a
                // shuffle, else the regiment's way; lie still when down.
                let walking = self.vel[i].length() > 0.6 && dist > 0.4;
                let look = match foe {
                    _ if down => self.facing[i],
                    Some(f) if !walking => (f - self.pos[i]).normalize_or(block.facing),
                    _ if walking => self.vel[i].normalize_or(block.facing),
                    _ => block.facing,
                };
                // Out of order, men look this way and that for their place.
                let look = if mess > 0.0 && foe.is_none() {
                    let p = unit_hash(block.handle, k, 9) * tau;
                    let turn = (self.time * DISORDER_LOOK_RATE + p).sin() * DISORDER_LOOK * mess;
                    Vec2::from_angle(turn).rotate(look)
                } else {
                    look
                };
                self.facing[i] = turn_toward(self.facing[i], look, TURN_RATE * dt);
            }
            let speed = if fresh { 0.0 } else { self.vel[i].length() };
            let mut pos = self.pos[i];
            let mut facing = self.facing[i];
            let striking = block.engaged && (k as u16) < files * 2 && champion.is_none();
            let phase = unit_hash(block.handle, k, 5);
            let dueling = foe.is_some() && striking;
            let beat = dueling.then(|| Self::beat(i, j as usize, self.time));
            let swing = match duel {
                Some((_, swing)) => swing,
                None => beat.and_then(|b| Some((self.swing(i, b)?, b))),
            };
            // A champion stands on guard once he has reached his place.
            let on_guard =
                dueling || champion.is_some() && self.pos[i].distance_squared(target) < 0.25;
            let fight = match (acting, swing) {
                _ if champion.is_some_and(|(d, side)| d.cheering(side)) => Some(Move::Cheer),
                (Some((Act::Kill, f)), _) => Some(Move::Swing { at: f, alt: false }),
                (Some((Act::Down, f)), _) => Some(Move::Down(f)),
                (Some((Act::Stagger, f)), _) => Some(Move::Stagger(f)),
                (_, Some((at, beat))) => Some(Move::Swing {
                    at,
                    alt: beat as i32 % 2 != 0,
                }),
                (Some((Act::Block, f)), _) => Some(Move::Block(f)),
                (Some((Act::Parry, f)), _) => Some(Move::Parry(f)),
                _ if on_guard => Some(Move::Guard),
                _ => None,
            };
            let reeling = acting.is_some_and(|(act, _)| act.reels());
            if let Some(beat) = beat.filter(|_| !reeling) {
                // The pair shares a rhythm, half a round apart: one steps in
                // to swing as the other gives ground.
                pos += facing * DUEL_STEP * (beat.fract() * tau).sin();
            } else if striking && fight.is_none() {
                let lunge = unit_hash(block.handle, k, 4);
                let swing = ((self.time * LUNGE_RATE + lunge) * tau).sin();
                pos += facing * LUNGE * swing.max(0.0);
            } else if speed < 0.15 && !block.routing && champion.is_none() {
                // At rest: shift weight and glance about.
                let p = unit_hash(block.handle, k, 6) * tau;
                let sway = (self.time * 0.9 + p).sin() + 0.5 * (self.time * 2.3 + p * 1.7).sin();
                pos += right * SWAY * sway;
                let glance = (self.time * 0.31 + p).sin() * (self.time * 0.13 + p * 0.6).sin();
                facing = Vec2::from_angle(GLANCE * glance).rotate(facing);
            }
            out(Soldier {
                pos,
                facing,
                speed,
                // Clearly backwards, so a sideways shuffle doesn't flip
                // the walk back and forth.
                backing: self.vel[i].dot(facing) < -0.5 * speed,
                striking,
                fight,
                phase,
            });
        }
    }

    /// Soldiers lost since last frame fall where they stand. The simulation
    /// takes casualties off the last slots, but on screen the men who fall
    /// are the ones fighting: a front-rank soldier (any soldier, for a block
    /// under arrows) drops, and the soldier from the emptied slot takes his
    /// place and walks up to fill the gap. In melee the man who falls is one
    /// locked in a duel, if any are, and his man swings the killing blow: he
    /// stands until it lands. A champion who has lost his duel falls to the
    /// other's last blow; one still fighting is never taken from the ranks.
    fn fall(&mut self, block: &Block, owner: u8, models: usize) {
        let r = block.handle.index as usize;
        let base = r * SLOTS;
        let before = (self.models[r] as usize).min(SLOTS);
        let files = block.files.clamp(1, block.models.max(1)) as usize;
        for gone in (models..before).rev() {
            let pool = if block.engaged {
                files.min(gone + 1)
            } else {
                gone + 1
            };
            let roll = unit_hash(block.handle, gone, self.time.to_bits());
            let slain = self.slain_champion(r);
            let champion = self.champion_of(r).map(|(_, _, c)| c);
            let out = |k: usize| champion == Some(base + k);
            // Best a man and his partner fighting each other, then any man
            // with a partner, else anyone not out as a champion.
            let partner = |k: usize| self.partner[base + k] as usize;
            let pick = slain
                .map(|(champion, _)| champion - base)
                .or_else(|| {
                    nth_of(pool, roll, |k| {
                        partner(k) != NONE as usize && self.partner[partner(k)] as usize == base + k
                    })
                })
                .or_else(|| nth_of(pool, roll, |k| partner(k) != NONE as usize && !out(k)))
                .or_else(|| nth_of(pool, roll, |k| !out(k)))
                .unwrap_or(gone);
            let (victim, last) = (base + pick, base + gone);
            let killer = slain.map_or(self.partner[victim] as usize, |(_, killer)| killer);
            let blow_in = if killer == NONE as usize {
                None
            } else if slain.is_some() {
                // The other champion's last blow has landed.
                Some(0.0)
            } else {
                match self.acting(killer) {
                    // His man is reeling or down and can't strike: he just
                    // drops.
                    Some((act, _)) if act.reels() => None,
                    // His man is already swinging a killing blow: he falls to
                    // it too.
                    Some((Act::Kill, f)) => Some((BLOW_AT - f).max(0.0) * SWING_SECONDS),
                    _ => {
                        self.act[killer] = (Act::Kill, self.time);
                        Some(BLOW_AT * SWING_SECONDS)
                    }
                }
            };
            let (fell, facing) = match blow_in {
                Some(wait) => (
                    self.time + wait,
                    (self.pos[killer] - self.pos[victim]).normalize_or(self.facing[victim]),
                ),
                None => (self.time, self.facing[victim]),
            };
            let corpse = (self.pos[victim], facing, owner, block.unit, fell);
            if self.corpses.len() < self.corpse_capacity {
                self.corpses.push(corpse);
            } else {
                self.corpses[self.next_corpse] = corpse;
                self.next_corpse = (self.next_corpse + 1) % self.corpse_capacity;
            }
            self.pos[victim] = self.pos[last];
            self.vel[victim] = self.vel[last];
            self.facing[victim] = self.facing[last];
            self.act[victim] = self.act[last];
            self.act[last] = (Act::None, 0.0);
            self.reeling[victim] = self.reeling[last];
            self.reeling[last] = f32::NEG_INFINITY;
            // Whoever steps into the gap finds his own man next frame.
            self.partner[victim] = NONE;
            self.partner[last] = NONE;
            if let Some(d) = &mut self.duel {
                for c in &mut d.champion {
                    if *c == victim as u32 {
                        *c = NONE;
                    } else if *c == last as u32 {
                        // A champion in the last slot moved into the gap.
                        *c = victim as u32;
                    }
                }
            }
        }
    }

    /// The champion of regiment `r` who has lost his duel and is due to
    /// fall, and the champion who killed him: (puppet, killer puppet).
    fn slain_champion(&self, r: usize) -> Option<(usize, usize)> {
        let d = self.duel.as_ref()?;
        let c = &d.challenge;
        let w = c.winner? as usize;
        let lost = d.champion[1 - w];
        (c.abandoned.is_none()
            && c.champions[1 - w].index as usize == r
            && lost != NONE
            && d.now >= c.ends() as f32 - 1.0)
            .then_some((lost as usize, d.champion[w] as usize))
    }

    /// Every fallen soldier still on the field.
    pub fn corpses(&self) -> impl Iterator<Item = Corpse> + '_ {
        self.corpses
            .iter()
            .filter_map(move |&(pos, facing, owner, unit, fell)| {
                let age = self.time - fell;
                let sink = (age - CORPSE_SECONDS) / CORPSE_SINK_SECONDS;
                (sink < 1.0).then_some(Corpse {
                    pos,
                    facing,
                    owner,
                    unit,
                    age,
                    sink: sink.max(0.0),
                })
            })
    }
}

/// The index below `pool` that `roll` (0 to 1) picks among those `keep`
/// accepts, if any.
fn nth_of(pool: usize, roll: f32, keep: impl Fn(usize) -> bool) -> Option<usize> {
    let n = (0..pool).filter(|&k| keep(k)).count();
    (0..pool)
        .filter(|&k| keep(k))
        .nth((roll * n as f32) as usize)
}

/// Local slot position, x to the right and y toward the rear, matching
/// `gc_sim::formation::slot_offset` (a partial rear rank is centred).
fn slot_offset(k: u16, files: u16, models: u16, file_spacing: f32, rank_spacing: f32) -> Vec2 {
    let ranks = models.div_ceil(files).max(1);
    let rank = k / files;
    let file = k % files;
    let in_rank = if rank == ranks - 1 {
        models - (ranks - 1) * files
    } else {
        files
    };
    Vec2::new(
        (f32::from(file) - (f32::from(in_rank) - 1.0) / 2.0) * file_spacing,
        (f32::from(rank) - (f32::from(ranks) - 1.0) / 2.0) * rank_spacing,
    )
}

/// A stable pseudo-random value in [0, 1) per soldier and purpose.
fn unit_hash(h: RegimentHandle, k: usize, salt: u32) -> f32 {
    mix(h.index, u32::from(h.generation), k as u32, salt)
}

/// A stable value in [0, 1) for blow `blow` between two puppets, one per
/// `salt`.
fn roll(i: usize, j: usize, blow: u32, salt: u32) -> f32 {
    mix(i as u32, j as u32, blow, salt)
}

/// A stable value in [0, 1) from four numbers.
fn mix(a: u32, b: u32, c: u32, d: u32) -> f32 {
    let mut x = a.wrapping_mul(0x9E37_79B9)
        ^ b.wrapping_mul(0x85EB_CA6B)
        ^ c.wrapping_mul(0xC2B2_AE35)
        ^ d.wrapping_mul(0x27D4_EB2F);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

fn jitter(h: RegimentHandle, k: usize) -> Vec2 {
    Vec2::new(
        unit_hash(h, k, 2) * 2.0 - 1.0,
        unit_hash(h, k, 3) * 2.0 - 1.0,
    )
}

fn turn_toward(from: Vec2, to: Vec2, max_angle: f32) -> Vec2 {
    let angle = from.angle_to(to);
    if angle.abs() <= max_angle {
        to
    } else {
        Vec2::from_angle(max_angle.copysign(angle)).rotate(from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_fixed::Fx;

    #[test]
    fn slots_match_the_simulation() {
        let fs = Fx::from_ratio(3, 4);
        let rs = Fx::from_ratio(17, 20);
        for (files, models) in [(8, 24), (6, 12), (7, 20), (24, 24), (5, 1)] {
            for k in 0..models {
                let a = gc_sim::formation::slot_offset(k, files, models, fs, rs);
                let b = slot_offset(k, files.min(models), models, 0.75, 0.85);
                let a = Vec2::new(a.x.raw() as f32, a.y.raw() as f32) / 65536.0;
                assert!(
                    a.distance(b) < 1e-3,
                    "{files}x{models} slot {k}: {a} vs {b}"
                );
            }
        }
    }

    #[test]
    fn soldiers_settle_into_their_slots() {
        let mut p = Puppets::new(4, 16);
        let mut block = Block {
            handle: RegimentHandle {
                index: 2,
                generation: 0,
            },
            unit: 0,
            centre: Vec2::new(10.0, 10.0),
            facing: Vec2::NEG_Y,
            files: 4,
            models: 8,
            file_spacing: 1.0,
            rank_spacing: 1.0,
            speed: 3.0,
            engaged: false,
            routing: false,
            disorder: 0.0,
            charge: 0,
            fighter: Fighter::default(),
        };
        let mut first = Vec::new();
        p.update(&block, 0, 0.016, |s| first.push(s.pos));
        // The block jumps 5 tiles north; soldiers follow and settle.
        block.centre.y -= 5.0;
        let mut last = Vec::new();
        for _ in 0..300 {
            last.clear();
            p.update(&block, 0, 0.016, |s| last.push(s));
        }
        for (a, b) in first.iter().zip(&last) {
            let moved = *a - b.pos;
            // Settled, give or take the sway at rest.
            assert!(
                (moved - Vec2::new(0.0, 5.0)).length() < 0.05 + 3.0 * SWAY,
                "{moved}"
            );
            assert!(b.facing.angle_to(Vec2::NEG_Y).abs() <= GLANCE + 1e-3);
        }
    }

    #[test]
    fn a_disordered_block_loses_its_ranks() {
        // How far, on average, soldiers stand from their exact slots after
        // settling, and how far they look away from the regiment's facing.
        let scatter = |disorder: f32| -> (f32, f32) {
            let mut p = Puppets::new(4, 16);
            let b = Block {
                disorder,
                ..block(1, Vec2::new(10.0, 10.0), Vec2::NEG_Y, false)
            };
            let mut off = (0.0, 0.0);
            for step in 0..400 {
                let mut k = 0;
                p.update(&b, 0, 0.016, |s| {
                    if step >= 200 {
                        // Facing north: right is +x and back is +y.
                        let slot = b.centre + slot_offset(k, 4, 8, 1.0, 1.0);
                        off.0 += s.pos.distance(slot);
                        off.1 += s.facing.angle_to(Vec2::NEG_Y).abs();
                    }
                    k += 1;
                });
            }
            (off.0 / 1600.0, off.1 / 1600.0)
        };
        let (calm_off, calm_look) = scatter(0.0);
        let (mess_off, mess_look) = scatter(1.0);
        assert!(
            mess_off > calm_off + 0.15,
            "{mess_off} vs {calm_off} tiles off"
        );
        assert!(
            mess_look > calm_look + 0.2,
            "{mess_look} vs {calm_look} radians"
        );
    }

    fn block(index: u32, centre: Vec2, facing: Vec2, engaged: bool) -> Block {
        Block {
            handle: RegimentHandle {
                index,
                generation: 0,
            },
            unit: 0,
            centre,
            facing,
            files: 4,
            models: 8,
            file_spacing: 1.0,
            rank_spacing: 1.0,
            speed: 3.0,
            engaged,
            routing: false,
            disorder: 0.0,
            charge: 0,
            fighter: Fighter::default(),
        }
    }

    #[test]
    fn soldiers_gather_speed_and_the_front_leads() {
        let mut p = Puppets::new(1, 4);
        let mut b = block(0, Vec2::ZERO, Vec2::NEG_Y, false);
        p.update(&b, 0, 0.016, |_| {});
        b.centre.y -= 10.0;
        let mut speeds = Vec::new();
        p.update(&b, 0, 0.016, |s| speeds.push(s.speed));
        // One frame in, nobody is near marching pace.
        assert!(speeds.iter().all(|&v| v < 0.5), "{speeds:?}");
        let mut at = Vec::new();
        for _ in 0..20 {
            at.clear();
            p.update(&b, 0, 0.016, |s| at.push(s.pos.y));
        }
        // The front rank (slots 0..4) has gone further than the rear.
        let front: f32 = at[..4].iter().sum::<f32>() / 4.0;
        let rear: f32 = at[4..].iter().sum::<f32>() / 4.0 - 1.0;
        assert!(front < rear - 0.05, "front {front} rear {rear}");
    }

    #[test]
    fn a_halting_block_overshoots_and_settles() {
        let mut p = Puppets::new(1, 4);
        let mut b = block(0, Vec2::ZERO, Vec2::NEG_Y, false);
        b.models = 1;
        b.files = 1;
        p.update(&b, 0, 0.016, |_| {});
        // March north at 3 tiles a second for two seconds, then halt.
        let mut furthest = 0.0f32;
        let mut last = Vec2::ZERO;
        for f in 0..400 {
            if f < 125 {
                b.centre.y -= 3.0 * 0.016;
            }
            p.update(&b, 0, 0.016, |s| last = s.pos);
            furthest = furthest.min(last.y);
        }
        // He settles on his slot (within its jitter), having gone past it.
        assert!((last.y + 6.0).abs() < SLOT_JITTER + 0.05, "{last}");
        let past = last.y - furthest;
        assert!(past > 0.02, "no overshoot: {past}");
        assert!(past < 0.25, "too much overshoot: {past}");
    }

    #[test]
    fn front_ranks_pair_off_and_face_their_man() {
        for hostile in [true, false] {
            let mut p = Puppets::new(2, 4);
            // Two blocks face to face, fronts 1.2 tiles apart.
            let a = block(0, Vec2::new(0.0, 0.0), Vec2::NEG_Y, true);
            let b = block(1, Vec2::new(0.3, -3.2), Vec2::Y, true);
            for _ in 0..60 {
                p.advance(0.016);
                p.pair(|x, y| hostile && x != y);
                p.update(&a, 0, 0.016, |_| {});
                p.update(&b, 1, 0.016, |_| {});
            }
            let mut out = Vec::new();
            p.update(&a, 0, 0.016, |s| out.push(s));
            let dueling = out.iter().filter(|s| s.fight.is_some()).count();
            if !hostile {
                assert_eq!(dueling, 0);
                continue;
            }
            assert_eq!(dueling, 4, "the whole front rank is paired");
            for (k, s) in out[..4].iter().enumerate() {
                let foe = p.pos[p.partner[k] as usize];
                assert_eq!(p.partner[k] as usize / SLOTS, 1);
                let to = (foe - p.pos[k]).normalize();
                assert!(p.facing[k].dot(to) > 0.95, "slot {k} faces away");
                assert!(s.striking);
            }
            // The rear rank is not paired.
            assert!(out[4..].iter().all(|s| s.fight.is_none()));
        }
    }

    #[test]
    fn casualties_fall_and_the_rest_close_up() {
        let mut p = Puppets::new(1, 3);
        let mut block = Block {
            handle: RegimentHandle {
                index: 0,
                generation: 0,
            },
            unit: 0,
            centre: Vec2::ZERO,
            facing: Vec2::NEG_Y,
            files: 4,
            models: 8,
            file_spacing: 1.0,
            rank_spacing: 1.0,
            speed: 3.0,
            engaged: true,
            routing: false,
            disorder: 0.0,
            charge: 0,
            fighter: Fighter::default(),
        };
        let mut soldiers = Vec::new();
        p.update(&block, 2, 0.016, |s| soldiers.push(s.pos));
        let front: Vec<Vec2> = soldiers[..4].to_vec();
        block.models = 6;
        p.advance(0.016);
        soldiers.clear();
        p.update(&block, 2, 0.016, |s| soldiers.push(s.pos));
        assert_eq!(soldiers.len(), 6);
        let corpses: Vec<Corpse> = p.corpses().collect();
        assert_eq!(corpses.len(), 2);
        for c in &corpses {
            // Engaged: the fallen are front-rank men.
            assert!(front.iter().any(|f| f.distance(c.pos) < 0.3), "{c:?}");
            assert_eq!((c.owner, c.sink), (2, 0.0));
        }

        // The ring buffer keeps the newest; old corpses sink away.
        block.models = 2;
        p.advance(0.016);
        p.update(&block, 2, 0.016, |_| {});
        assert_eq!(p.corpses().count(), 3);
        p.advance(CORPSE_SECONDS + CORPSE_SINK_SECONDS);
        assert_eq!(p.corpses().count(), 0);
    }

    const FRAME: f32 = 1.0 / 60.0;

    /// Two regiments facing each other across a melee line, fronts 1.2 tiles
    /// apart: `a` (pool index 0) faces north, `b` (1) south.
    fn melee_pair(a: Fighter, b: Fighter) -> (Block, Block) {
        let mut x = block(0, Vec2::new(0.0, 0.0), Vec2::NEG_Y, true);
        let mut y = block(1, Vec2::new(0.3, -3.2), Vec2::Y, true);
        x.fighter = a;
        y.fighter = b;
        (x, y)
    }

    fn frame(p: &mut Puppets, a: &Block, b: &Block) {
        p.advance(FRAME);
        p.pair(|x, y| x != y);
        p.update(a, 0, FRAME, |_| {});
        p.update(b, 1, FRAME, |_| {});
    }

    /// What `a`'s blows did to `b`'s front rank over `seconds` of melee: how
    /// many times each act began.
    fn blows(a: Fighter, b: Fighter, seconds: f32) -> [u32; 6] {
        blows_facing(a, b, Vec2::Y, seconds)
    }

    /// The same with `b` facing `facing`.
    fn blows_facing(a: Fighter, b: Fighter, facing: Vec2, seconds: f32) -> [u32; 6] {
        let mut p = Puppets::new(2, 64);
        let (x, mut y) = melee_pair(a, b);
        y.facing = facing;
        let mut seen = [(Act::None, 0.0); 4];
        let mut count = [0; 6];
        for _ in 0..(seconds / FRAME) as usize {
            frame(&mut p, &x, &y);
            for (k, last) in seen.iter_mut().enumerate() {
                let now = p.act[SLOTS + k];
                if now != *last {
                    count[now.0 as usize] += 1;
                    *last = now;
                }
            }
        }
        count
    }

    fn fighter(attack: i32, defence: i32) -> Fighter {
        Fighter {
            attack,
            defence,
            ..Fighter::default()
        }
    }

    #[test]
    fn blows_land_at_the_simulations_odds() {
        // 50 + 80 - 20 caps at 90%: nearly every blow staggers its man.
        let c = blows(fighter(80, 20), fighter(0, 20), 60.0);
        let total: u32 = c.iter().sum();
        assert!(total > 150, "{c:?}");
        assert!(c[Act::Stagger as usize] * 100 > total * 80, "{c:?}");
        assert_eq!(c[Act::Down as usize] + c[Act::Block as usize], 0, "{c:?}");

        // 10%: nearly every blow is parried.
        let c = blows(fighter(0, 20), fighter(0, 60), 60.0);
        let total: u32 = c.iter().sum();
        assert!(c[Act::Parry as usize] * 100 > total * 80, "{c:?}");

        // A shield takes the blows a blade would parry.
        let c = blows(
            fighter(0, 20),
            Fighter {
                shield: true,
                ..fighter(0, 60)
            },
            60.0,
        );
        let total: u32 = c.iter().sum();
        assert_eq!(c[Act::Parry as usize], 0, "{c:?}");
        assert!(c[Act::Block as usize] * 100 > total * 80, "{c:?}");

        // A wavering regiment lands fewer.
        let sure = blows(fighter(45, 0), fighter(0, 45), 60.0)[Act::Stagger as usize];
        let shaken = Fighter {
            sharpness: 50,
            ..fighter(45, 0)
        };
        let unsure = blows(shaken, fighter(0, 45), 60.0)[Act::Stagger as usize];
        assert!(unsure * 3 < sure * 2, "{unsure} of {sure}");
    }

    #[test]
    fn blows_on_a_flank_meet_its_flank_defence() {
        // A shield wall: 60 to the front, nothing off it.
        let wall = Fighter {
            flank_defence: 0,
            ..fighter(0, 60)
        };
        let share = |c: [u32; 6]| c[Act::Stagger as usize] as f32 / c.iter().sum::<u32>() as f32;
        let front = share(blows_facing(fighter(10, 0), wall, Vec2::Y, 60.0));
        let flank = share(blows_facing(fighter(10, 0), wall, Vec2::X, 60.0));
        assert!(front < 0.2, "{front}");
        assert!(flank > 0.4, "{flank}");
    }

    #[test]
    fn heavy_blades_crush_through_a_light_mans_guard() {
        let heavy = Fighter {
            mass: Mass::Heavy,
            heavy_blade: true,
            ..fighter(0, 20)
        };
        let light = Fighter {
            mass: Mass::Light,
            ..fighter(0, 60)
        };
        let c = blows(heavy, light, 60.0);
        let total: u32 = c.iter().sum();
        assert!(c[Act::Down as usize] * 2 > total, "{c:?}");
        // Men of a weight parry each other, and so do medium men against
        // light ones.
        let c = blows(fighter(0, 20), fighter(0, 60), 60.0);
        assert_eq!(c[Act::Down as usize], 0, "{c:?}");
        let c = blows(fighter(0, 20), light, 60.0);
        assert_eq!(c[Act::Down as usize], 0, "{c:?}");
    }

    #[test]
    fn a_staggered_man_loses_his_next_swing() {
        let mut p = Puppets::new(2, 4);
        let (a, b) = melee_pair(Fighter::default(), Fighter::default());
        for _ in 0..30 {
            frame(&mut p, &a, &b);
        }
        let j = p.partner[0] as usize;
        assert_ne!(p.partner[0], NONE);
        // Just after his swing begins.
        let next = Puppets::beat(0, j, p.time).floor() + 1.0;
        p.time += (next - Puppets::beat(0, j, p.time)) / DUEL_RATE + 0.05;
        let beat = Puppets::beat(0, j, p.time);
        let swing = p.swing(0, beat).expect("swinging");
        assert!(swing < 0.2, "{swing}");
        // Staggered just before it began, he reels through it...
        let now = p.time;
        p.time = now - 0.1;
        p.suffer(0, j, Act::Stagger);
        p.time = now;
        assert_eq!(p.swing(0, beat), None);
        // ...even once the stagger is over and a later blow is parried.
        p.time = now + 0.55;
        p.suffer(0, j, Act::Parry);
        assert_eq!(p.act[0].0, Act::Parry);
        assert_eq!(p.swing(0, Puppets::beat(0, j, p.time)), None);
        // A parry alone doesn't cost him the swing.
        p.time = now;
        p.reeling[0] = f32::NEG_INFINITY;
        assert!(p.swing(0, beat).is_some());
    }

    #[test]
    fn the_killing_blow_comes_from_his_man() {
        let mut p = Puppets::new(2, 4);
        let (a, mut b) = melee_pair(Fighter::default(), Fighter::default());
        for _ in 0..30 {
            frame(&mut p, &a, &b);
        }
        b.models -= 1;
        frame(&mut p, &a, &b);
        let corpse = p.corpses().next().expect("a man fell");
        // He stands until the blow lands...
        assert!(corpse.age < 0.0, "{corpse:?}");
        // ...dealt by one of the front rank facing him, who swings it now.
        let killers: Vec<usize> = (0..4).filter(|&k| p.act[k].0 == Act::Kill).collect();
        assert_eq!(killers.len(), 1, "{:?}", &p.act[..4]);
        // He falls facing his killer.
        let to = (p.pos[killers[0]] - corpse.pos).normalize();
        assert!(corpse.facing.dot(to) > 0.99, "{:?} {to}", corpse.facing);
        let mut fights = Vec::new();
        p.advance(FRAME);
        p.update(&a, 0, FRAME, |s| fights.push(s.fight));
        assert!(
            matches!(fights[killers[0]], Some(Move::Swing { at, alt: false }) if at < 0.1),
            "{:?}",
            fights[killers[0]]
        );
        p.advance(BLOW_AT * SWING_SECONDS);
        assert!(p.corpses().next().unwrap().age >= 0.0);
    }

    #[test]
    fn a_heavy_charge_knocks_men_off_their_feet() {
        let downed = |charger: Mass, foe: Mass| {
            let mut p = Puppets::new(2, 4);
            let (mut a, mut b) = melee_pair(
                Fighter {
                    mass: charger,
                    ..Fighter::default()
                },
                Fighter {
                    mass: foe,
                    ..Fighter::default()
                },
            );
            (a.engaged, b.engaged) = (false, false);
            frame(&mut p, &a, &b);
            (a.engaged, b.engaged, a.charge) = (true, true, 60);
            for _ in 0..3 {
                frame(&mut p, &a, &b);
            }
            (SLOTS..SLOTS + 4)
                .filter(|&i| p.act[i].0 == Act::Down)
                .count()
        };
        use Mass::*;
        // Heavy into light: most of the front rank goes down.
        assert!(downed(Heavy, Light) >= 2, "{}", downed(Heavy, Light));
        // Men of a weight only stagger, and so do light men charged by
        // medium ones.
        assert_eq!(downed(Medium, Medium), 0);
        assert_eq!(downed(Medium, Light), 0);
        assert_eq!(downed(Heavy, Heavy), 0);
    }

    /// Two regiments 30 tiles apart whose champions fight: champion 0
    /// lands the blows of beats 0, 2 and 4 (each costing the other his
    /// next swing) and kills champion 1.
    fn champions_called() -> (Block, Block, Challenge) {
        let a = block(0, Vec2::new(0.0, 30.0), Vec2::NEG_Y, false);
        let b = block(1, Vec2::ZERO, Vec2::Y, false);
        let c = Challenge {
            champions: [a.handle, b.handle],
            spot: gc_fixed::FxVec2::from_ints(0, 15),
            sides: [0, 1],
            called: 0,
            meet: 100,
            beats: 5,
            landed: 0b10101,
            winner: Some(0),
            abandoned: None,
        };
        (a, b, c)
    }

    #[test]
    fn champions_run_out_fight_and_the_loser_falls() {
        let mut p = Puppets::new(2, 4);
        let (a, mut b, c) = champions_called();
        let tick = TICKS_PER_SECOND as f32 * FRAME;
        let mut now = -tick;
        let mut seen = Vec::new();
        let step = |p: &mut Puppets, a: &Block, b: &Block, now: f32| {
            p.advance(FRAME);
            p.champions((now >= 0.0).then_some(&c), now);
            let mut out = Vec::new();
            p.update(a, 0, FRAME, |s| out.push(s));
            p.update(b, 1, FRAME, |s| out.push(s));
            out
        };
        // Settle in the ranks, then the challenge is called.
        while now < c.meet as f32 {
            seen = step(&mut p, &a, &b, now);
            now += tick;
        }
        let [x, y] = p.duel.unwrap().champion.map(|i| i as usize);
        assert_eq!((x / SLOTS, y / SLOTS), (0, 1));
        // They have run out and face each other at the spot.
        let sx = seen[x];
        assert!(sx.pos.distance(Vec2::new(0.0, 15.5)) < 0.3, "{:?}", sx.pos);
        assert!(
            p.pos[y].distance(Vec2::new(0.0, 14.5)) < 0.3,
            "{:?}",
            p.pos[y]
        );
        assert!(sx.facing.dot(Vec2::NEG_Y) > 0.9);
        // Beat by beat: 0 swings and lands, 1 reels and loses his swing, ...
        let mut moves = [vec![None], vec![None]];
        while now < c.ends() as f32 {
            let out = step(&mut p, &a, &b, now);
            moves[0].push(out[x % SLOTS].fight);
            moves[1].push(out[8 + y % SLOTS].fight);
            now += tick;
        }
        let swings = |m: &[Option<Move>]| {
            m.windows(2)
                .filter(|w| {
                    matches!(w[1], Some(Move::Swing { .. }))
                        && !matches!(w[0], Some(Move::Swing { .. }))
                })
                .count()
        };
        assert_eq!(swings(&moves[0]), 3, "{:?}", moves[0]);
        assert_eq!(swings(&moves[1]), 0, "a reeling man swung: {:?}", moves[1]);
        assert!(moves[1].iter().any(|m| matches!(m, Some(Move::Stagger(_)))));
        // The simulation takes the loser as the last blow lands: he falls at
        // the spot, facing his killer, and the winner cheers.
        b.models -= 1;
        let out = step(&mut p, &a, &b, now);
        let corpse = p.corpses().next().expect("the loser fell");
        assert!(
            corpse.pos.distance(Vec2::new(0.0, 14.5)) < 0.3,
            "{corpse:?}"
        );
        assert_eq!(corpse.owner, 1);
        assert!(corpse.facing.dot(Vec2::Y) > 0.9, "{corpse:?}");
        assert_eq!(out[x % SLOTS].fight, Some(Move::Cheer));
        // Then he walks back to his place in the ranks.
        for _ in 0..(CHEER_SECONDS / FRAME) as usize + 10 {
            now += tick;
            step(&mut p, &a, &b, now);
        }
        assert!(p.champion_of(0).is_none());
        for _ in 0..600 {
            now += tick;
            step(&mut p, &a, &b, now);
        }
        assert!(p.pos[x].distance(a.centre) < 3.0, "{:?}", p.pos[x]);
    }

    #[test]
    fn soldiers_fall_around_a_champion_still_fighting() {
        let mut p = Puppets::new(2, 4);
        let (a, b, c) = champions_called();
        let mut a = a;
        p.champions(None, 0.0);
        p.advance(FRAME);
        p.update(&a, 0, FRAME, |_| {});
        p.update(&b, 1, FRAME, |_| {});
        p.champions(Some(&c), 110.0);
        let x = p.duel.unwrap().champion[0];
        for lost in 1..=4 {
            a.models -= 1;
            p.advance(FRAME);
            p.update(&a, 0, FRAME, |_| {});
            assert_eq!(p.corpses().count(), lost);
            assert!(
                p.corpses()
                    .all(|c| c.pos.distance(Vec2::new(0.0, 15.5)) > 1.0)
            );
            assert_eq!(p.duel.unwrap().champion[0], x);
        }
    }
}
