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
//! blocked, staggers its man or crushes through his guard: as the
//! simulation's own blow by that man came out, when it told one, else with
//! the odds the simulation uses; a heavier charge knocks men off their feet;
//! and the man the simulation's blow killed falls to his killer's blow, the
//! two playing a matched kill when their models have one made for them. When the
//! simulation calls out champions before a battle, one man steps out of each
//! champion's regiment, runs to the spot between the lines and plays out
//! the duel the simulation rolled, blow by blow. None of this feeds back
//! into the simulation, so it may use floats and frame time.

use gc_sim::combat::{Blow, CHARGE_TICKS, Outcome, hit_percent};
use gc_sim::formation::MAX_MODELS;
use gc_sim::{Challenge, General, Mass, RegimentHandle, TICKS_PER_SECOND};
use glam::Vec2;
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use crate::models::down_depth;

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
/// How long, and from how far, men turn their heads to a comrade falling
/// beside them (seconds, tiles; fading out over the last half tile), how
/// long the turn takes to come and to go, the most a man is slow to react,
/// and how far down they look at him (radians).
const FALL_GLANCE_SECONDS: f32 = 1.2;
const FALL_GLANCE_REACH: f32 = 2.5;
const FALL_GLANCE_IN: f32 = 0.15;
const FALL_GLANCE_OUT: f32 = 0.5;
const FALL_GLANCE_LATE: f32 = 0.25;
const FALL_GLANCE_DOWN: f32 = 0.2;
/// How fast a man's look follows where he means to look (per second), so
/// it never snaps.
const GLANCE_EASE: f32 = 10.0;
/// How far a blow taken snaps a man's head back (radians, by what it did
/// to him), how much of that it turns aside, and when through the move it
/// is furthest.
const HEAD_FLINCH_STAGGER: f32 = 0.45;
const HEAD_FLINCH_BLOCK: f32 = 0.22;
const HEAD_FLINCH_PARRY: f32 = 0.14;
const HEAD_FLINCH_ASIDE: f32 = 0.5;
const HEAD_FLINCH_PEAK: f32 = 0.15;
const HEAD_FLINCH_ENDS: f32 = 0.6;
/// Duel pairs: a front-rank soldier looks for an enemy within this reach, in
/// tiles, and keeps him until he is this far away.
const DUEL_REACH: f32 = 2.6;
const DUEL_KEEP: f32 = 3.4;
/// Paired soldiers close to this gap, in tiles: the usual one, a spear's
/// length, big bodies (cavalry, monsters and great blades) and small ones
/// (swarms) that get right on top of their man. A pair stands apart by the
/// mean of its two men's gaps, each body taking its own half.
const DUEL_GAP: f32 = 0.85;
const SPEAR_GAP: f32 = 1.4;
const BIG_GAP: f32 = 1.05;
const SMALL_GAP: f32 = 0.6;
/// A dueling soldier strays no further than his leash from his own slot:
/// this much, plus up to the extra for the bolder men, so the meeting line
/// breaks up into a ragged band.
const DUEL_LEASH: f32 = 0.6;
const LEASH_BOLD: f32 = 0.6;
/// Choosing a new man, the soldiers already fighting him make him seem
/// further away by this much times their number squared, in tiles squared,
/// so men spread along the enemy line and only gang up two (or three) on
/// one where their line overlaps his.
const CROWD: f32 = 1.6;
/// In melee the ranks behind the fighting ones close up by this fraction
/// of their spacing, pressing in behind the men in front.
const PRESS: f32 = 0.25;
/// Men waiting their turn behind the fighting ranks shift their weight this
/// much more than men at rest, and about half of them shout now and then:
/// for this long, in seconds, once in this many.
const RESTLESS: f32 = 2.0;
const SHOUT_SECONDS: f32 = 1.2;
const SHOUT_EVERY: f32 = 7.0;
/// The step in and back as a pair trades blows, in tiles, and its rate.
const DUEL_STEP: f32 = 0.16;
const DUEL_RATE: f32 = 0.8;
/// A swing takes this long, in seconds, and its blow lands this far into it.
const SWING_SECONDS: f32 = 0.7;
const BLOW_AT: f32 = gc_assets::anim::BLOW_AT;
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
/// A charge that ploughs into the ranks knocks the men it hits off their
/// feet, whatever their weight, and this share of the men either side of
/// them (the rest stagger); the line dents back by up to this many rank
/// spacings where it went in, the front rank most.
const PLOUGH_KNOCKDOWN: f32 = 0.3;
const DENT_RANKS: f32 = 1.5;
/// The dent is full while this many ticks of the plough are left, then
/// springs back as the line holds again.
pub const DENT_EASE_TICKS: f32 = 10.0;
/// Soldiers who pair off while their regiment has more than this many ticks
/// of charge bonus left (within 0.6 s of the charge hitting home) deliver
/// its impact.
const IMPACT_TICKS: u16 = CHARGE_TICKS - 12;
/// How hard a blow shoves its man back, in tiles per second.
const STAGGER_SHOVE: f32 = 0.8;
const DOWN_SHOVE: f32 = 2.4;
/// Regiments this far apart, centre to centre, are never paired.
const DUEL_REGIMENT_RANGE: f32 = 24.0;
/// How finely a soldier off firm ground looks for it on the way in to the
/// middle of his block, in tiles.
const GROUND_STEP: f32 = 0.25;
/// A regiment not updated for this long, in seconds, is gone.
const STALE: f32 = 0.5;
/// How long a blow the simulation told waits for the striker's next swing
/// to land on the man it struck, in seconds: a little over a round. The sim
/// strikes a little more often than men swing on screen, so a swing plays
/// the latest blow, or a wound if one of two drew blood.
const TOLD_KEEP: f32 = 1.5;
/// No partner.
const NONE: u32 = u32::MAX;
/// [`Puppets::general`] of a regiment without one.
const NO_GENERAL: u8 = u8::MAX;

/// A general's speech (see [`Puppets::generals`]): he holds his weapon up
/// this long, and his men cheer in turn, each for [`CHEER_TURN`] seconds,
/// starting up to [`CHEER_SPREAD`] seconds after he begins.
const SPEECH_SHOW: f32 = 4.0;
const CHEER_TURN: f32 = 1.6;
const CHEER_SPREAD: f32 = 2.4;
/// The share of men near him who cheer.
const CHEERING: f32 = 0.7;
/// When a general falls, the men of his side near him flinch as one, a
/// moment after.
const FLINCH_AFTER: f32 = 0.3;

/// What the generals mean to one block this frame (see
/// [`Puppets::heeding`]).
#[derive(Clone, Copy, Debug, Default)]
struct Heed {
    /// How far through the flinch at their general's fall, if they are in
    /// one.
    flinch: Option<f32>,
    /// Seconds since their general's speech, if they heard it.
    speech: Option<f32>,
}

impl Heed {
    /// What soldier `k` of block `h` does for it, when nothing else has
    /// him busy: the general raises his weapon through his speech, most of
    /// his men cheer in turn, and all flinch as one when he falls.
    fn of(self, h: RegimentHandle, k: usize, general: bool) -> Option<Move> {
        if let Some(f) = self.flinch {
            return Some(Move::Stagger(f));
        }
        let since = self.speech?;
        let cheers = if general {
            (0.0..SPEECH_SHOW).contains(&since)
        } else {
            unit_hash(h, k, 21) < CHEERING
                && (0.0..CHEER_TURN).contains(&(since - unit_hash(h, k, 22) * CHEER_SPREAD))
        };
        cheers.then_some(Move::Cheer)
    }
}
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
/// The fallen lie where they fell for the whole battle. When the field is
/// full (the corpse capacity `Puppets::new` was given) the oldest sinks into
/// the ground over this many seconds to make room, and at most this many
/// sink at once; more than that, in a burst, just go.
const CORPSE_SINK_SECONDS: f32 = 3.0;
const MAX_SINKING: usize = 256;
/// An arrow that killed a soldier sticks in him anywhere within this much
/// of his length (along the way he lies) and of his width, in tiles.
const LODGED_ALONG: f32 = 0.5;
const LODGED_ASIDE: f32 = 0.2;
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
    /// How hard a charge that ploughed into its ranks still drives it
    /// back: 1 while more than [`DENT_EASE_TICKS`] of it are left, falling
    /// to 0 as the regiment holds again (`RegimentView::ploughed`), and
    /// which regiment charged: the line dents where that charge went in.
    pub ploughed: f32,
    pub ploughed_by: Option<RegimentHandle>,
    /// The slot its side's general stands in, if he rides with it.
    pub general: Option<u8>,
    pub fighter: Fighter,
}

/// A wall piece a manning regiment stands on: its centre, how high above
/// the ground there its top is, and for a piece longer than a tile (a
/// gatehouse) the half of its length along it, else zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallSlot {
    pub centre: Vec2,
    pub lift: f32,
    pub half: Vec2,
}

/// Across how much of each tile of a piece men sharing it spread, in tiles.
const WALL_SPREAD: f32 = 0.8;

/// How fast a soldier climbs to or down from the wall top, in tiles per
/// second.
const CLIMB_RATE: f32 = 1.5;

/// How many men a wall slot has room for in one rank: one a tile of its
/// length.
fn slot_room(slot: &WallSlot) -> usize {
    ((slot.half.length() * 2.0).round() as usize).max(1)
}

/// Where soldier `k` of `models` stands on `slots` (never empty): the slots
/// give one place a tile of their length, in order, and soldier `k` takes
/// place `k % places`. Men beyond the places make further ranks. On a
/// single tile they spread along `across` (a unit vector along the wall,
/// across the way they face); on a long slot, across its depth. Also the
/// height of the top there.
fn wall_stand(slots: &[WallSlot], k: usize, models: usize, across: Vec2) -> (Vec2, f32) {
    let places: usize = slots.iter().map(slot_room).sum();
    let (place, nth) = (k % places, k / places);
    // How many men share this place.
    let sharing = (models.saturating_sub(place)).div_ceil(places).max(1);
    let step = WALL_SPREAD / sharing as f32;
    let off = (nth as f32 - (sharing - 1) as f32 / 2.0) * step;
    let (mut left, mut slot) = (place, slots[0]);
    for s in slots {
        slot = *s;
        if left < slot_room(s) {
            break;
        }
        left -= slot_room(s);
    }
    let len = slot.half.length();
    if len > 0.0 {
        let along = slot.half / len;
        let tile = left as f32 - (slot_room(&slot) - 1) as f32 / 2.0;
        let side = Vec2::new(-along.y, along.x);
        (slot.centre + along * tile + side * off, slot.lift)
    } else {
        (slot.centre + across * off, slot.lift)
    }
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
    /// Spears and pikes: the second rank fights too, over the shoulders of
    /// the first, and men hold a spear's length from their foe.
    pub reach: bool,
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
            reach: false,
        }
    }
}

impl Fighter {
    /// The chance (0 to 1) a blow gets through a guard of `defence`: the
    /// simulation's chance to hit.
    fn lands(&self, defence: i32) -> f32 {
        hit_percent(self.attack, defence) as f32 * self.sharpness as f32 / 10_000.0
    }

    /// The gap this man keeps from his foe, in tiles.
    fn gap(&self) -> f32 {
        match () {
            _ if self.reach => SPEAR_GAP,
            _ if self.heavy_blade || self.mass == Mass::Heavy => BIG_GAP,
            _ if self.mass == Mass::Light => SMALL_GAP,
            _ => DUEL_GAP,
        }
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

/// A charge's dent in a line (see [`Puppets::dent`]), in the line's own
/// frame: where the charger stands and the way it drives, how far either
/// side of it the line gives, where the line starts along that way and how
/// deep it is, and how far it dents at most, in tiles.
struct Dent {
    from: Vec2,
    push: Vec2,
    half: f32,
    near: f32,
    span: f32,
    depth: f32,
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
    /// In a fighting rank (the front one, and the second for spears),
    /// striking at the enemy.
    pub striking: bool,
    /// Waiting his turn behind the fighting ranks and standing still, and
    /// shouting at the enemy: how many seconds into his shout.
    pub shouting: Option<f32>,
    /// What he is doing in a duel: paired with an enemy soldier and trading
    /// blows, or still reeling from one. `None` out of a duel.
    pub fight: Option<Move>,
    /// A stable value in [0, 1) per soldier, to keep animations out of step.
    pub phase: f32,
    /// A stable value per soldier for his look (`models::look`), which he
    /// keeps when he falls.
    pub look: u8,
    /// His side's general (see [`gc_sim::generals`]), drawn to stand out.
    pub general: bool,
    /// How far he bends to strike down at a man lying at his feet (a
    /// finisher), 0 (not at all) to 1.
    pub stoop: f32,
    /// How high above the ground he stands: on a wall top, else 0.
    pub lift: f32,
    /// Where he turns his head (and chest) on top of his move, for the
    /// detail near the camera: a flinch from a blow, or a look at a comrade
    /// falling beside him.
    pub head: Option<Head>,
}

/// A turn of a soldier's upper body on top of his move.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Head {
    /// Radians toward his left.
    pub yaw: f32,
    /// Radians down (negative: back).
    pub pitch: f32,
}

/// A dueling soldier's move, with how far through it he is (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Move {
    /// Weapon up, waiting for an opening.
    Guard,
    /// Swinging at his man, `at` of the way through: `blow` counts his
    /// rounds of blows (one more each swing), so each may be another of his
    /// attacks; `None` for a killing blow, always his weapon's first.
    Swing { at: f32, blow: Option<u32> },
    /// Striking down at a man on the ground with a move made for it (a
    /// finisher), `at` through it: its blow lands at `BLOW_AT`. `seed`
    /// picks which of his finishers, the same all through one. (A man
    /// without one swings, bent over him: see [`Soldier::stoop`].)
    Finish { at: f32, seed: u32 },
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
    /// Playing his half of matched kill `pair` (an index into the table
    /// given to [`Puppets::set_kills`]), `at` of the way through.
    Kill { pair: u16, at: f32 },
    /// A champion playing the victim's half of matched kill `pair`, `at` of
    /// the way through, until he falls.
    Killed { pair: u16, at: f32 },
}

/// A matched kill, as the puppets play it: its length in seconds (both
/// halves play together, at their own speed), how far through it the blow
/// lands, and how far apart the two men stand, in tiles. The moves
/// themselves stay with whoever draws them, under the same index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KillTiming {
    pub seconds: f32,
    pub contact: f32,
    pub reach: f32,
}

/// The matched kills each pair of unit types can play, by their index in
/// one table of [`KillTiming`]s, and each unit type's finisher.
#[derive(Default)]
pub struct Kills {
    timing: Vec<KillTiming>,
    /// The indices each pair of unit types picks among, in runs.
    choices: Vec<u16>,
    /// Per (killer's unit, victim's unit), where its run of choices starts
    /// and how long it is.
    by_units: HashMap<(u16, u16), (u32, u16)>,
    /// Per unit type with a move made for striking down at a man on the
    /// ground, how far from him its men stand to strike (tiles) and how
    /// long the move is (seconds; its blow lands at `BLOW_AT` of it).
    finishers: Vec<Option<(f32, f32)>>,
    /// Per unit type and look ([`Soldier::look`]), where a man of it lying
    /// dead has his chest from his position (`x` to his left, `y` ahead
    /// along the facing he fell with), for a finisher to strike at.
    chests: Vec<Vec<Option<Vec2>>>,
}

impl Kills {
    /// No unit types play any of the kills `timing` holds, yet.
    pub fn new(timing: Vec<KillTiming>) -> Kills {
        Kills {
            timing,
            ..Kills::default()
        }
    }

    /// A man of unit `killer` may play the kills `ids` on one of unit
    /// `victim`.
    pub fn add(&mut self, killer: u16, victim: u16, ids: &[u16]) {
        let Ok(n) = u16::try_from(ids.len()) else {
            return;
        };
        if n > 0 {
            let first = self.choices.len() as u32;
            self.choices.extend_from_slice(ids);
            self.by_units.insert((killer, victim), (first, n));
        }
    }

    /// Men of unit `unit` finish a man on the ground with a move made for
    /// it, `seconds` long, standing `reach` tiles from him.
    pub fn add_finisher(&mut self, unit: u16, reach: f32, seconds: f32) {
        let u = usize::from(unit);
        if self.finishers.len() <= u {
            self.finishers.resize(u + 1, None);
        }
        self.finishers[u] = Some((reach, seconds));
    }

    /// Men of unit `unit` lying dead have their chests at `by_look[look]`
    /// from their position, `x` to the left and `y` ahead along the facing
    /// they fell with.
    pub fn add_chests(&mut self, unit: u16, by_look: Vec<Option<Vec2>>) {
        let u = usize::from(unit);
        if self.chests.len() <= u {
            self.chests.resize(u + 1, Vec::new());
        }
        self.chests[u] = by_look;
    }

    /// Where a man of unit `unit` and look `look` lying dead has his chest
    /// from his position, if known.
    fn chest(&self, unit: u16, look: u8) -> Option<Vec2> {
        *self.chests.get(usize::from(unit))?.get(usize::from(look))?
    }

    /// The finisher men of unit `unit` strike down with, if they have one:
    /// its reach and length.
    fn finisher(&self, unit: u16) -> Option<(f32, f32)> {
        self.finishers.get(usize::from(unit)).copied().flatten()
    }

    /// How far from a man on the ground one of unit `unit` stands to finish
    /// him: his finisher's reach, else [`FINISH_GAP`] for the bent swing.
    pub(crate) fn finish_gap(&self, unit: u16) -> f32 {
        self.finisher(unit).map_or(FINISH_GAP, |(reach, _)| reach)
    }

    /// The kill `roll` (0 to 1) picks for a man of `killer` on one of
    /// `victim`, if they have any.
    pub(crate) fn pick(&self, killer: u16, victim: u16, roll: f32) -> Option<(u16, KillTiming)> {
        let &(first, n) = self.by_units.get(&(killer, victim))?;
        let nth = ((roll * f32::from(n)) as u16).min(n - 1);
        let id = self.choices[first as usize + usize::from(nth)];
        Some((id, *self.timing.get(usize::from(id))?))
    }
}

/// A killing blow a puppet is dealing (while his act is [`Act::Kill`]).
#[derive(Clone, Copy, Debug)]
struct Killing {
    /// Where the man he is killing stood: he steps in to strike there,
    /// whoever fills the gap, this far from it.
    spot: Vec2,
    gap: f32,
    /// The matched kill the two play, if any, and its timing.
    pair: Option<(u16, KillTiming)>,
    /// Finishing a man lying at his feet: he stands over him and strikes
    /// down.
    finish: bool,
    /// He was in the front rank when it began, and is drawn onto its
    /// distance (motion warping); one behind strikes from his place.
    front: bool,
    /// The way from the spot he strikes from, if set (a general coming in
    /// beside his man); else from wherever he is.
    from: Vec2,
    /// Finishing with a move made for it: its length (seconds), and the
    /// seed that picks which of his finishers.
    strike: Option<(f32, u32)>,
}

impl Killing {
    const NONE: Killing = Killing {
        spot: Vec2::ZERO,
        gap: 0.0,
        pair: None,
        finish: false,
        front: false,
        from: Vec2::ZERO,
        strike: None,
    };
}

/// A man finishing one knocked down stands this far from him, in tiles,
/// over his body.
pub(crate) const FINISH_GAP: f32 = 0.55;
/// He bends to it over this share of his killing blow, and straightens
/// over this last share.
const STOOP_IN: f32 = 0.3;
const STOOP_OUT: f32 = 0.25;

/// How far a man finishing one on the ground bends to it, `f` of the way
/// through his killing blow.
fn stoop(f: f32) -> f32 {
    (f / STOOP_IN).min((1.0 - f) / STOOP_OUT).clamp(0.0, 1.0)
}

/// How a man flinches from a blow he is taking (`act`, `f` through it): his
/// head snaps back and aside, furthest early on, and comes back as he
/// recovers. `aside` (-1 to 1) picks which way.
fn flinch(act: Act, f: f32, aside: f32) -> Head {
    let back = match act {
        Act::Stagger => HEAD_FLINCH_STAGGER,
        Act::Block => HEAD_FLINCH_BLOCK,
        Act::Parry => HEAD_FLINCH_PARRY,
        _ => return Head::default(),
    };
    let weight = (f / HEAD_FLINCH_PEAK)
        .min((HEAD_FLINCH_ENDS - f) / (HEAD_FLINCH_ENDS - HEAD_FLINCH_PEAK))
        .clamp(0.0, 1.0);
    Head {
        yaw: back * HEAD_FLINCH_ASIDE * aside * weight,
        pitch: -back * weight,
    }
}

/// Where a man at `pos` facing `facing` means to look `since` seconds after
/// a comrade fell at `at`, `late` seconds slow to react: turned his way
/// and a little down, eased in and out, and not at all once he is out of
/// reach, behind him or at his very feet.
fn fall_glance(pos: Vec2, facing: Vec2, at: Vec2, since: f32, late: f32) -> Head {
    let since = since - late;
    let ease = (since / FALL_GLANCE_IN).min((FALL_GLANCE_SECONDS - since) / FALL_GLANCE_OUT);
    if ease <= 0.0 {
        return Head::default();
    }
    let to = at - pos;
    let d = to.length();
    let ahead = to.dot(facing) / d.max(1e-3);
    let weight = ease
        .min((FALL_GLANCE_REACH - d) * 2.0)
        .min((d - 0.3) / 0.3)
        .min((ahead + 0.2) / 0.4)
        .min(1.0);
    if weight <= 0.0 {
        return Head::default();
    }
    let left = Vec2::new(facing.y, -facing.x);
    Head {
        yaw: to
            .dot(left)
            .atan2(to.dot(facing))
            .clamp(-FRAC_PI_2, FRAC_PI_2)
            * weight,
        pitch: FALL_GLANCE_DOWN * weight,
    }
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
            Act::Kill => SWING_SECONDS + KILL_HOLD,
        }
    }

    /// Whether it knocks him out of his next swing.
    fn reels(self) -> bool {
        matches!(self, Act::Stagger | Act::Down)
    }
}

/// A man killed by a blow is driven back along it as he falls, by up to
/// this far in tiles for each step of how hard it hit him (see
/// [`Fallen::owner`]), over this many seconds from the moment it lands.
const RECOIL: f32 = 0.15;
const RECOIL_SECONDS: f32 = 0.35;
/// A man a much heavier man's blow or a charge kills is thrown instead
/// (see [`THROWN`]): this far back along the blow, in tiles, over this many
/// seconds, the first part of it in the air, rising this high, as he
/// turns over by this much, in radians (four tenths of a turn), one way or
/// the other ([`SPIN_LEFT`]).
const THROW: f32 = 0.9;
const THROW_SECONDS: f32 = 0.6;
const FLIGHT_SECONDS: f32 = 0.4;
const THROW_HEIGHT: f32 = 0.3;
const THROW_TURN: f32 = 0.8 * core::f32::consts::PI;
/// The turn's cosine and sine, for a man who has come down.
const THROW_TURNED: Vec2 = Vec2::new(-0.809_017, 0.587_785);
/// A killer holds his follow-through this long, in seconds, before he
/// looks for a new man.
const KILL_HOLD: f32 = 0.3;
/// A general takes the killing blow of a man of his own regiment who kills
/// within this many tiles of him, stepping through the ranks to deal it,
/// and brandishes his weapon this many seconds after.
const HERO_REACH: f32 = 2.5;
const HERO_BRANDISH: f32 = 1.0;
/// How fast a general may cover the ground to his blow (tiles a second),
/// the seconds he rests after brandishing before he takes another, and
/// how far off his man's line (radians) he comes in, beside him.
const HERO_STRIDE: f32 = 4.0;
const HERO_REST: f32 = 4.0;
const HERO_ASIDE: f32 = 0.5;
/// How far through a swing a held follow-through stands: its last frame.
const HELD: f32 = 0.999;

/// How far through his swing a man is who is `f` of the way through his
/// killing blow: the swing, then its follow-through, held (a swing at 1.0
/// would start over).
fn kill_swing(f: f32) -> f32 {
    (f * Act::Kill.seconds() / SWING_SECONDS).min(HELD)
}

/// A soldier lying where he fell, as the puppets keep him: 28 bytes, so a
/// field of 32,768 of them is 896 KB.
#[derive(Clone, Copy, Debug)]
struct Fallen {
    pos: Vec2,
    facing: Vec2,
    /// When he fell, in puppet time (later than now while he still stands,
    /// waiting for the killing blow).
    fell: f32,
    unit: u16,
    /// His player in the low bits, above [`RECOIL_SHIFT`] how hard the
    /// blow that killed him drove him back: 0 for none (arrows, and men who
    /// simply drop), 1 for a blow, 2 for a heavier man's, [`FINISHED`] for
    /// one finished where he lay; or [`THROWN`] for a man thrown by it, and
    /// [`SPIN_LEFT`] for which way he turns.
    owner: u8,
    look: u8,
    /// The matched kill he fell to, if any.
    pair: Option<u16>,
}

const RECOIL_SHIFT: u32 = 4;
/// [`Fallen::owner`]'s bit for a man thrown by the blow that killed him
/// (a much heavier man's, or a charge's): he tumbles through the air and
/// lands a way off, turned over; and the bit for turning to his left.
const THROWN: u8 = 1 << 6;
const SPIN_LEFT: u8 = 1 << 7;
/// [`Fallen::owner`]'s recoil step for a man killed lying down, knocked
/// off his feet: he lies as he was, and isn't driven anywhere.
const FINISHED: u8 = 3 << RECOIL_SHIFT;

/// How far through `seconds` `age` is, eased out: 0 before, 1 after.
fn ease_out(age: f32, seconds: f32) -> f32 {
    let f = (age / seconds).clamp(0.0, 1.0);
    1.0 - (1.0 - f) * (1.0 - f)
}

impl Fallen {
    fn player(&self) -> u8 {
        self.owner & ((1 << RECOIL_SHIFT) - 1)
    }

    /// Where he is `age` seconds after he fell, driven back (or thrown)
    /// along the blow from where it struck him, the way he faces (a thrown
    /// man turns over in the air) and how high he is off the ground (only
    /// while a thrown man flies).
    fn pose(&self, age: f32) -> (Vec2, Vec2, f32) {
        if self.owner & THROWN == 0 {
            let steps = (self.owner >> RECOIL_SHIFT) & 3;
            // A man finished where he lay isn't driven anywhere.
            if steps == 0 || steps == FINISHED >> RECOIL_SHIFT || age <= 0.0 {
                return (self.pos, self.facing, 0.0);
            }
            let back = RECOIL * f32::from(steps) * ease_out(age, RECOIL_SECONDS);
            return (self.pos - self.facing * back, self.facing, 0.0);
        }
        let pos = self.pos - self.facing * (THROW * ease_out(age, THROW_SECONDS));
        let left = self.owner & SPIN_LEFT != 0;
        let (turn, lift) = match age {
            _ if age <= 0.0 => return (self.pos, self.facing, 0.0),
            _ if age >= FLIGHT_SECONDS => (THROW_TURNED, 0.0),
            _ => {
                let f = age / FLIGHT_SECONDS;
                let turn = Vec2::from_angle(THROW_TURN * ease_out(age, FLIGHT_SECONDS));
                (turn, 4.0 * THROW_HEIGHT * f * (1.0 - f))
            }
        };
        let turn = if left {
            turn
        } else {
            Vec2::new(turn.x, -turn.y)
        };
        (pos, turn.rotate(self.facing), lift)
    }

    /// Whether he was killed lying down (see [`FINISHED`]).
    fn lying(&self) -> bool {
        self.owner & (THROWN | FINISHED) == FINISHED
    }

    /// Where he comes to rest.
    fn rest(&self) -> Vec2 {
        self.pose(f32::INFINITY).0
    }
}

/// A soldier lying where he fell.
#[derive(Clone, Copy, Debug)]
pub struct Corpse {
    pub pos: Vec2,
    pub facing: Vec2,
    pub owner: u8,
    pub unit: u16,
    /// His look, as he had it standing ([`Soldier::look`]).
    pub look: u8,
    /// How high he is off the ground, in tiles: a man thrown by the blow
    /// that killed him, in the air.
    pub lift: f32,
    /// Seconds since he fell; negative while he still stands, waiting for
    /// the killing blow.
    pub age: f32,
    /// How far it has sunk into the ground, from 0 (lying on it) to 1 (gone),
    /// once it is the oldest on a full field and makes room for a new one.
    pub sink: f32,
    /// Killed lying down, knocked off his feet: he lies as he was, in the
    /// last of his death move.
    pub lying: bool,
    /// The matched kill he fell to (as for [`Move::Kill`]): he plays its
    /// victim's half, which starts with his killer's, `age` + its contact
    /// seconds ago. `None` for his own death.
    pub kill: Option<u16>,
}

/// An arrow that has just killed a soldier and stays in his body: where it
/// stuck, and the way it flew.
#[derive(Clone, Copy, Debug)]
pub struct Lodged {
    pub pos: Vec2,
    pub dir: Vec2,
}

/// What a soldier's blow did, for the sparks and wounds the game shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// Turned aside on a weapon.
    Parried,
    /// Taken on a shield.
    Blocked,
    /// It got through and staggered him.
    Wounded,
    /// It crushed through and knocked him down.
    Crushed,
    /// It killed him.
    Killed,
}

/// A blow landing on a soldier, told once as it lands (`Puppets::drain_impacts`).
#[derive(Clone, Copy, Debug)]
pub struct Impact {
    /// Where it landed on the ground plane, and the way it travelled.
    pub pos: Vec2,
    pub dir: Vec2,
    pub hit: Hit,
    /// The unit type of the man struck.
    pub victim: u16,
    /// The unit type of the man who struck him.
    pub striker: u16,
}

/// Blows told and not yet collected, at most; later ones are dropped.
const MAX_IMPACTS: usize = 256;

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
    /// The matched kill the winner ends it with, if the two have one that
    /// fits between champions (see [`CHAMPION_REACH`]).
    kill: Option<(u16, KillTiming)>,
}

/// The longest matched kill champions play, in tiles of reach: the winner
/// steps back from the spot to it, and a spear's would take him too far.
const CHAMPION_REACH: f32 = 1.6;

impl Duel {
    /// Whether champion `side` is out between the lines: until the duel is
    /// abandoned, the loser falls, they part, or the winner has cheered.
    fn out(&self, side: usize) -> bool {
        let c = &self.challenge;
        let ends = c.ends() as f32;
        c.abandoned.is_none()
            && match c.winner {
                Some(w) if w as usize == side => {
                    self.now < ends + self.tail() + CHEER_SECONDS * TICKS_PER_SECOND as f32
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
        if since_meet < 0.0 || self.now >= c.ends() as f32 || self.killing().is_some() {
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

    /// Whether champion `side` has won and is cheering, once his kill has
    /// played out.
    fn cheering(&self, side: usize) -> bool {
        self.won(side) && self.now >= self.challenge.ends() as f32 + self.tail()
    }

    /// Whether champion `side` wins the duel.
    fn won(&self, side: usize) -> bool {
        self.challenge.winner == Some(side as u8)
    }

    /// Seconds until the duel's last blow lands (negative after).
    fn blow_in(&self) -> f32 {
        (self.challenge.ends() as f32 - self.now) / TICKS_PER_SECOND as f32
    }

    /// The matched kill the duel ends in, once it has begun: which, its
    /// timing and how far through it the two are (1 and on once played).
    fn kill_begun(&self) -> Option<(u16, KillTiming, f32)> {
        let (pair, k) = self.kill.filter(|_| self.challenge.abandoned.is_none())?;
        let at = k.contact - self.blow_in() / k.seconds;
        (at >= 0.0).then_some((pair, k, at))
    }

    /// The matched kill the duel ends in, while it plays: which, its timing
    /// and how far through it the two are (it ends held on its last frame).
    /// It starts its contact's share of its length before the killing blow
    /// lands, so the blow meets the man as the simulation has it.
    fn killing(&self) -> Option<(u16, KillTiming, f32)> {
        self.kill_begun().filter(|&(_, _, at)| at < 1.0)
    }

    /// Ticks the winner's matched kill plays on after its blow lands.
    fn tail(&self) -> f32 {
        self.kill.map_or(0.0, |(_, k)| {
            (1.0 - k.contact) * k.seconds * TICKS_PER_SECOND as f32
        })
    }

    /// Champion `side`'s half of the matched kill, while it plays.
    fn kill_move(&self, side: usize) -> Option<Move> {
        let (pair, _, at) = self.killing()?;
        // Not without the man who plays the other half.
        if self.champion[1 - side] == NONE {
            return None;
        }
        Some(if self.won(side) {
            Move::Kill { pair, at }
        } else {
            Move::Killed { pair, at }
        })
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
    ploughed_by: Vec<Option<RegimentHandle>>,
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
    /// Per puppet, the killing blow he is dealing (while his act is
    /// [`Act::Kill`]).
    killing: Vec<Killing>,
    /// The matched kills each pair of unit types plays.
    kills: Kills,
    reeling: Vec<f32>,
    /// Each soldier's look ([`Soldier::look`]), which goes with him when he
    /// steps into a gap.
    looks: Vec<u8>,
    /// How many soldiers have picked each soldier as their man, counted
    /// afresh every frame.
    marked: Vec<u8>,
    /// Scratch for pairing: regiments in melee, and the enemies near one.
    fighting: Vec<u32>,
    near: Vec<u32>,
    /// The unit type of each regiment, for the colours and model of anyone
    /// who falls when it is wiped out.
    unit: Vec<u16>,
    /// Fallen soldiers, a ring buffer; the oldest are replaced first, and
    /// sink away while they go.
    corpses: Vec<Fallen>,
    corpse_capacity: usize,
    next_corpse: usize,
    /// Soldiers sinking out of sight to make room, and when each began.
    sinking: Vec<(Fallen, f32)>,
    /// Soldiers each regiment has lost to arrows this frame that have yet to
    /// fall, and the way the volley that took them flew.
    shot: Vec<u8>,
    shot_dir: Vec<Vec2>,
    /// Arrows that stuck in the bodies of soldiers who fell this frame.
    lodged: Vec<Lodged>,
    /// Blows that have landed, or will when the puppet time reaches the
    /// first of each pair (a killing blow lands as its victim falls).
    impacts: Vec<(f32, Impact)>,
    /// Scratch for [`Puppets::drain_impacts`]: those that are due.
    ready: Vec<Impact>,
    /// Soldiers the simulation's blows have killed and the puppets have yet
    /// to fell, in the order they fell: the regiment, his slot then, and
    /// the puppet who struck him (or `NONE`).
    slain: Vec<(RegimentHandle, u8, u32)>,
    /// Per puppet, the simulation's last blow by him that didn't kill: the
    /// puppet it fell on, whether it wounded him, and when it was told.
    told: Vec<Option<(u32, bool, f32)>>,
    /// Per puppet slot, when a man last stepped into it (or left it): blows
    /// told on that slot before then fell on someone else.
    filled: Vec<f32>,
    time: f32,
    /// The last frame's step, in seconds.
    dt: f32,
    /// Per regiment, the wall tiles it stands on (empty: in its block), set
    /// by [`Puppets::stand_on_wall`]; and per puppet how high he stands.
    wall: Vec<Vec<WallSlot>>,
    lift: Vec<f32>,
    /// Per regiment, how high its soldiers stand in their block at the
    /// head of a ladder (0: not on one), set by [`Puppets::stand_on_ladder`].
    ladder: Vec<f32>,
    /// Per regiment, where and when its last man fell (or will, as the blow
    /// lands), for the men beside him to look.
    last_fall: Vec<(Vec2, f32)>,
    /// Per puppet, where he looks (see [`Soldier::head`]), eased.
    glance: Vec<Head>,
    /// Per regiment, the slot its general stands in, or `NO_GENERAL`.
    general: Vec<u8>,
    /// Per puppet, until when he brandishes his weapon over a man he
    /// killed (a general's flourish).
    brandish: Vec<f32>,
    duel: Option<Duel>,
    /// The tick the duel the puppets are done with was called.
    duel_over: Option<u32>,
    /// Each side's general, as the simulation last told it, and its time
    /// then, in ticks (interpolated).
    generals: Vec<General>,
    general_now: f32,
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
            ploughed_by: vec![None; regiment_capacity],
            heading: vec![Vec2::NEG_Y; regiment_capacity],
            half: vec![Vec2::ONE; regiment_capacity],
            seen: vec![f32::NEG_INFINITY; regiment_capacity],
            pos: vec![Vec2::ZERO; regiment_capacity * SLOTS],
            vel: vec![Vec2::ZERO; regiment_capacity * SLOTS],
            facing: vec![Vec2::NEG_Y; regiment_capacity * SLOTS],
            partner: vec![NONE; regiment_capacity * SLOTS],
            act: vec![(Act::None, 0.0); regiment_capacity * SLOTS],
            killing: vec![Killing::NONE; regiment_capacity * SLOTS],
            kills: Kills::default(),
            reeling: vec![f32::NEG_INFINITY; regiment_capacity * SLOTS],
            looks: vec![0; regiment_capacity * SLOTS],
            marked: vec![0; regiment_capacity * SLOTS],
            fighting: Vec::with_capacity(regiment_capacity),
            near: Vec::with_capacity(regiment_capacity),
            unit: vec![0; regiment_capacity],
            corpses: Vec::with_capacity(corpse_capacity.max(1)),
            corpse_capacity: corpse_capacity.max(1),
            next_corpse: 0,
            sinking: Vec::with_capacity(MAX_SINKING),
            shot: vec![0; regiment_capacity],
            shot_dir: vec![Vec2::X; regiment_capacity],
            lodged: Vec::with_capacity(MAX_SINKING),
            impacts: Vec::with_capacity(MAX_IMPACTS),
            ready: Vec::with_capacity(MAX_IMPACTS),
            slain: Vec::with_capacity(MAX_SINKING),
            told: vec![None; regiment_capacity * SLOTS],
            filled: vec![f32::NEG_INFINITY; regiment_capacity * SLOTS],
            time: 0.0,
            dt: 0.0,
            wall: vec![Vec::new(); regiment_capacity],
            ladder: vec![0.0; regiment_capacity],
            lift: vec![0.0; regiment_capacity * SLOTS],
            last_fall: vec![(Vec2::ZERO, f32::NEG_INFINITY); regiment_capacity],
            glance: vec![Head::default(); regiment_capacity * SLOTS],
            general: vec![NO_GENERAL; regiment_capacity],
            brandish: vec![f32::NEG_INFINITY; regiment_capacity * SLOTS],
            duel: None,
            duel_over: None,
            generals: Vec::with_capacity(gc_sim::MAX_PARTIES),
            general_now: 0.0,
        }
    }

    /// The matched kills soldiers play, by their unit types (see [`Kills`]).
    pub fn set_kills(&mut self, kills: Kills) {
        self.kills = kills;
    }

    /// Put regiment `handle`'s soldiers on `slots` from now on (soldier `k`
    /// on slot `k % slots.len()`), or back in their block if `slots` is
    /// empty. Call it every frame before the regiment's update; it does not
    /// allocate once warm.
    pub fn stand_on_wall(&mut self, handle: RegimentHandle, slots: impl Iterator<Item = WallSlot>) {
        let Some(held) = self.wall.get_mut(handle.index as usize) else {
            return;
        };
        held.clear();
        held.extend(slots);
        self.ladder[handle.index as usize] = 0.0;
    }

    /// Lift regiment `handle`'s soldiers to `height` where they stand in
    /// their block, as at the head of a ladder against a wall that high
    /// (0 puts them down). Call it every frame before the regiment's
    /// update.
    pub fn stand_on_ladder(&mut self, handle: RegimentHandle, height: f32) {
        let i = handle.index as usize;
        if let (Some(w), Some(l)) = (self.wall.get_mut(i), self.ladder.get_mut(i)) {
            w.clear();
            *l = height;
        }
    }

    /// Whether regiment `handle`'s soldiers are lifted on a ladder.
    pub fn on_ladder(&self, handle: RegimentHandle) -> bool {
        (self.ladder.get(handle.index as usize)).is_some_and(|&h| h > 0.0)
    }

    /// Whether regiment `handle`'s soldiers are set to stand on a wall.
    pub fn on_wall(&self, handle: RegimentHandle) -> bool {
        self.wall
            .get(handle.index as usize)
            .is_some_and(|w| !w.is_empty())
    }

    /// How high the top of the wall regiment `handle` stands on is (the
    /// highest of its tiles), or 0 if it is not on one.
    pub fn wall_top(&self, handle: RegimentHandle) -> f32 {
        let i = handle.index as usize;
        let slots =
            (self.wall.get(i)).map_or(0.0, |w| w.iter().map(|s| s.lift).fold(0.0, f32::max));
        slots.max(self.ladder.get(i).copied().unwrap_or(0.0))
    }

    /// Follow the generals (see [`gc_sim::generals`]) at the simulation's
    /// time `now`, in ticks (interpolated).
    pub fn generals<'a>(&mut self, generals: impl Iterator<Item = &'a General>, now: f32) {
        self.generals.clear();
        self.generals.extend(generals.copied());
        self.general_now = now;
    }

    /// Seconds since tick `t`, by the generals' clock.
    fn since(&self, t: u32) -> f32 {
        (self.general_now - t as f32) / TICKS_PER_SECOND as f32
    }

    /// What the generals of `owner`'s side mean to a block of theirs
    /// standing at `centre`: the moment of a flinch at his fall, or seconds
    /// since his speech, if they are near enough to be part of it.
    fn heeding(&self, owner: u8, centre: Vec2) -> Heed {
        let mut heed = Heed::default();
        for g in self.generals.iter().filter(|g| g.party == owner) {
            let d = crate::from_fx_vec(g.at).distance_squared(centre);
            let within = |tiles: i32| d <= (tiles * tiles) as f32;
            match (g.fell, g.spoke) {
                (Some(t), _) if within(gc_sim::generals::PANIC_TILES) => {
                    let f = (self.since(t) - FLINCH_AFTER) / STAGGER_SECONDS;
                    heed.flinch = heed.flinch.or((0.0..1.0).contains(&f).then_some(f));
                }
                (None, Some(t)) if within(gc_sim::generals::LEAD_TILES) => {
                    heed.speech = heed.speech.or(Some(self.since(t)));
                }
                _ => {}
            }
        }
        heed
    }

    /// Game seconds since the puppets started (stops while paused).
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Start a new frame `dt` seconds after the last one.
    pub fn advance(&mut self, dt: f32) {
        self.time += dt;
        self.dt = dt;
        let time = self.time;
        self.sinking
            .retain(|&(_, from)| time - from < CORPSE_SINK_SECONDS);
    }

    /// Arrows from a volley flying along `dir` killed `lost` of regiment
    /// `target`'s soldiers; the first that fall in the next update each keep
    /// an arrow in them (see [`Puppets::drain_lodged`]).
    pub fn shot_down(&mut self, target: RegimentHandle, dir: Vec2, lost: u16) {
        let r = target.index as usize;
        if lost > 0 && self.owner[r] == Some(target) {
            self.shot[r] = self.shot[r].saturating_add(lost.min(u16::from(u8::MAX)) as u8);
            self.shot_dir[r] = dir;
        }
    }

    /// A melee blow the simulation struck (`World::blows`): the striker
    /// takes its target as his man, his next swing wounds him or is turned
    /// aside as the blow was, and a man it killed is the one who falls, to
    /// his blow. Call for each tick's blows, in order.
    pub fn struck(&mut self, b: &Blow) {
        let (s, r) = (b.striker.index as usize, b.target.index as usize);
        let striker = if self.owner[s] == Some(b.striker) {
            (s * SLOTS + b.striker_slot as usize) as u32
        } else {
            NONE
        };
        match b.outcome {
            Outcome::Killed { men } if self.owner[r] == Some(b.target) => {
                if striker != NONE {
                    // His killing blow is played; what he struck before is done.
                    self.told[striker as usize] = None;
                }
                if self.slain.len() + usize::from(men) > self.slain.capacity() {
                    // Kills for regiments since gone that never fell here.
                    let owner = &self.owner;
                    self.slain
                        .retain(|&(h, ..)| owner[h.index as usize] == Some(h));
                }
                // The first falls to the blow; any more who step up into
                // his place go down with him.
                for m in 0..men {
                    if self.slain.len() < self.slain.capacity() {
                        let by = if m == 0 { striker } else { NONE };
                        self.slain.push((b.target, b.target_slot, by));
                    }
                }
            }
            _ if striker != NONE && self.owner[r] == Some(b.target) => {
                let target = (r * SLOTS + b.target_slot as usize) as u32;
                self.partner[striker as usize] = target;
                let told = &mut self.told[striker as usize];
                // Two blows before his next swing: it shows the wound, if
                // either drew blood on that man.
                let bled =
                    told.is_some_and(|(t, w, at)| w && t == target && self.time - at <= TOLD_KEEP);
                *told = Some((target, b.outcome == Outcome::Wounded || bled, self.time));
            }
            _ => {}
        }
    }

    /// The blows that landed since the last call, for sparks and wounds.
    pub fn drain_impacts(&mut self) -> impl Iterator<Item = Impact> + '_ {
        let now = self.time;
        let due = self.impacts.extract_if(.., |&mut (when, _)| when <= now);
        self.ready.extend(due.map(|(_, i)| i));
        self.ready.drain(..)
    }

    /// Note that `striker`'s blow lands on `victim` (puppet indices) at
    /// `when`, doing `hit`.
    fn impact(&mut self, when: f32, striker: usize, victim: usize, hit: Hit) {
        // A full queue keeps the kills, the blows most worth showing, by
        // giving up a lesser blow for one.
        let room = if self.impacts.len() < MAX_IMPACTS {
            Some(self.impacts.len())
        } else if hit == Hit::Killed {
            self.impacts.iter().position(|(_, i)| i.hit != Hit::Killed)
        } else {
            None
        };
        let Some(slot) = room else {
            return;
        };
        let dir = (self.pos[victim] - self.pos[striker]).normalize_or(self.facing[striker]);
        let told = (
            when,
            Impact {
                pos: self.pos[victim] - dir * 0.2,
                dir,
                hit,
                victim: self.unit[victim / SLOTS],
                striker: self.unit[striker / SLOTS],
            },
        );
        match self.impacts.get_mut(slot) {
            Some(old) => *old = told,
            None => self.impacts.push(told),
        }
    }

    /// The arrows that stuck in soldiers who fell since the last call.
    pub fn drain_lodged(&mut self) -> impl Iterator<Item = Lodged> + '_ {
        self.lodged.drain(..)
    }

    /// Regiment `h` was wiped out: its last soldiers (the ones its final
    /// update never saw fall, as the regiment is gone from the simulation
    /// before the next one) drop where they stood. A regiment that is gone
    /// for any other reason, taken off the field by the player, leaves
    /// nobody. Tells whose it was, if it had been on the field.
    pub fn wiped(&mut self, h: RegimentHandle) -> Option<u8> {
        let r = h.index as usize;
        if self.owner[r] != Some(h) {
            return None;
        }
        let base = r * SLOTS;
        for k in 0..(self.models[r] as usize).min(SLOTS) {
            let fallen = Fallen {
                pos: self.pos[base + k],
                facing: self.facing[base + k],
                fell: self.time,
                unit: self.unit[r],
                owner: self.player[r],
                look: self.looks[base + k],
                pair: None,
            };
            self.lay(fallen);
            self.lodge(h, k, &fallen);
        }
        self.owner[r] = None;
        self.models[r] = 0;
        self.shot[r] = 0;
        self.slain.retain(|&(s, ..)| s != h);
        Some(self.player[r])
    }

    /// If an arrow of regiment `h`'s last volley is still to find a man,
    /// it stays in `fallen` (its `k`th to fall), somewhere along his length
    /// and a little to one side.
    fn lodge(&mut self, h: RegimentHandle, k: usize, fallen: &Fallen) {
        let r = h.index as usize;
        if self.shot[r] == 0 {
            return;
        }
        self.shot[r] -= 1;
        let along = unit_hash(h, k, 11) - 0.5;
        let aside = unit_hash(h, k, 12) - 0.5;
        let side = Vec2::new(-fallen.facing.y, fallen.facing.x);
        // In his body where it comes to rest.
        let at = fallen.rest();
        self.lodged.push(Lodged {
            pos: at + fallen.facing * (along * LODGED_ALONG) + side * (aside * LODGED_ASIDE),
            dir: self.shot_dir[r],
        });
    }

    /// Lay a soldier on the field. When it is full the oldest sinks away to
    /// make room.
    fn lay(&mut self, fallen: Fallen) {
        if self.corpses.len() < self.corpse_capacity {
            self.corpses.push(fallen);
            return;
        }
        let oldest = core::mem::replace(&mut self.corpses[self.next_corpse], fallen);
        self.next_corpse = (self.next_corpse + 1) % self.corpse_capacity;
        if self.sinking.len() < MAX_SINKING {
            self.sinking.push((oldest, self.time));
        }
    }

    /// Pair the fighting ranks of regiments in melee (the front rank, and
    /// the second too for spears) with enemy front-rank soldiers across the
    /// line, from last frame's positions, and play out the blows that land
    /// this frame. A soldier keeps his man while the man stands within
    /// reach. A new man is the nearest, counting each soldier already
    /// fighting him as a little further off, so men spread along the enemy
    /// line and gang up two on one where their line overlaps his. `hostile`
    /// says whether two players fight. Call once a frame, before the
    /// updates.
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
        // First the pairs that hold, so everyone choosing a new man knows
        // who is already taken.
        for &a in &fighting {
            let a = a as usize;
            self.foes_near(a, &fighting, &hostile, &mut near);
            let band = if near.is_empty() { 0 } else { self.band(a) };
            for k in 0..SLOTS {
                let i = a * SLOTS + k;
                let j = self.partner[i];
                if k >= band || j == NONE || !self.can_duel(a, j as usize, self.pos[i], &near) {
                    self.partner[i] = NONE;
                } else {
                    self.mark(j as usize);
                }
            }
        }
        for &a in &fighting {
            let a = a as usize;
            self.foes_near(a, &fighting, &hostile, &mut near);
            if near.is_empty() {
                continue;
            }
            let impact = self.charge[a] > IMPACT_TICKS;
            for k in 0..self.band(a) {
                let i = a * SLOTS + k;
                if self.partner[i] == NONE {
                    let foe = self.nearest_foe(self.pos[i], &near);
                    self.partner[i] = foe;
                    if foe != NONE {
                        self.mark(foe as usize);
                        if impact {
                            self.charge_home(i, foe as usize);
                        }
                    }
                }
                self.strike(i);
            }
        }
        // Counted afresh next frame.
        for &a in &fighting {
            let a = a as usize;
            self.marked[a * SLOTS..(a + 1) * SLOTS].fill(0);
        }
        self.fighting = fighting;
        self.near = near;
    }

    /// One more soldier has puppet `j` as his man.
    fn mark(&mut self, j: usize) {
        self.marked[j] = self.marked[j].saturating_add(1);
    }

    /// The regiments of `fighting` hostile to regiment `a` and close enough
    /// to pair with, into `near`.
    fn foes_near(
        &self,
        a: usize,
        fighting: &[u32],
        hostile: &impl Fn(u8, u8) -> bool,
        near: &mut Vec<u32>,
    ) {
        near.clear();
        near.extend(fighting.iter().copied().filter(|&b| {
            let b = b as usize;
            b != a
                && hostile(self.player[a], self.player[b])
                && self.centre[a].distance_squared(self.centre[b])
                    <= DUEL_REGIMENT_RANGE * DUEL_REGIMENT_RANGE
        }));
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
                kill: None,
            },
        };
        // The winner ends it with a matched kill if the two have one.
        let duel = match (self.duel, challenge.winner) {
            (None, Some(w)) if !duel.champion.contains(&NONE) => {
                let unit = |side: usize| self.unit[challenge.champions[side].index as usize];
                let roll = mix(challenge.called, 0, 0, 18);
                let w = usize::from(w);
                Duel {
                    kill: (self.kills)
                        .pick(unit(w), unit(1 - w), roll)
                        .filter(|&(_, k)| k.reach <= CHAMPION_REACH),
                    ..duel
                }
            }
            _ => duel,
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
    /// The winner of a duel ending in a matched kill stands at its reach
    /// (warped onto it as he winds up).
    fn stance(&self, d: &Duel, side: usize) -> Vec2 {
        let [a, b] = d.challenge.champions.map(|h| self.centre[h.index as usize]);
        let toward_b = (b - a).normalize_or(Vec2::Y);
        let half = toward_b * (DUEL_GAP / 2.0);
        let stand = [d.spot - half, d.spot + half];
        match d.kill_begun() {
            Some((_, k, _)) if d.won(side) => {
                let them = match d.champion[1 - side] {
                    NONE => stand[1 - side],
                    other => self.pos[other as usize],
                };
                them + (stand[side] - them).normalize_or(half * (2.0 * side as f32 - 1.0)) * k.reach
            }
            _ => stand[side],
        }
    }

    /// The nearest front-rank soldier of the regiments `near` within reach
    /// of `me`, those others already fight seeming further off (`CROWD`),
    /// or `NONE`.
    fn nearest_foe(&self, me: Vec2, near: &[u32]) -> u32 {
        let reach = DUEL_REACH * DUEL_REACH;
        let mut best = (NONE, f32::INFINITY);
        for &b in near {
            let b = b as usize;
            for j in b * SLOTS..b * SLOTS + self.front(b) {
                let d = me.distance_squared(self.pos[j]);
                if d >= reach {
                    continue;
                }
                let m = f32::from(self.marked[j]);
                let score = d + CROWD * m * m;
                if score < best.1 {
                    best = (j as u32, score);
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

    /// The matched kill puppet `i` is playing, if he is dealing a killing
    /// blow with one.
    fn pair_of(&self, i: usize) -> Option<(u16, KillTiming)> {
        if self.act[i].0 != Act::Kill {
            return None;
        }
        self.killing[i].pair
    }

    /// The timing of the move puppet `i`'s killing blow plays, if not his
    /// swing: a matched kill, or a finisher made for it.
    fn kill_timing(&self, i: usize) -> Option<KillTiming> {
        if self.act[i].0 != Act::Kill {
            return None;
        }
        let k = &self.killing[i];
        k.pair
            .map(|(_, t)| t)
            .or(k.strike.map(|(seconds, _)| KillTiming {
                seconds,
                contact: BLOW_AT,
                reach: k.gap,
            }))
    }

    /// Seconds until puppet `i`'s killing blow, `f` of the way through,
    /// lands: none or less once it has, and he is only following through,
    /// and may be struck, or strike again.
    fn kill_lands_in(&self, i: usize, f: f32) -> f32 {
        match self.kill_timing(i) {
            Some(k) => (k.contact - f) * k.seconds,
            None => (BLOW_AT - kill_swing(f)) * SWING_SECONDS,
        }
    }

    /// What puppet `i` is doing outside his round of swings, and how far
    /// through it he is.
    fn acting(&self, i: usize) -> Option<(Act, f32)> {
        let (act, at) = self.act[i];
        if act == Act::None {
            return None;
        }
        let seconds = self.kill_timing(i).map_or(act.seconds(), |k| k.seconds);
        let f = (self.time - at) / seconds;
        (0.0..1.0).contains(&f).then_some((act, f))
    }

    /// If puppet `i`'s swing landed on his man since last frame, it gets
    /// through and staggers him, or a heavier striker crushes through his
    /// guard and knocks him down, or he parries it or takes it on his
    /// shield. When the simulation told his last blow lately, it is played
    /// as it came out: a wound gets through (or crushes through), anything
    /// else is parried or blocked. Else it rolls the simulation's odds.
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
        let crushes = || roll(i, j, blow, 2) < CRUSH_PER_CLASS * me.outweighs(&them, true);
        let guard = if them.shield { Act::Block } else { Act::Parry };
        let told = self.told[i].take().filter(|&(t, _, at)| {
            t == j as u32 && self.time - at <= TOLD_KEEP && at > self.filled[j]
        });
        let act = match told {
            // The simulation's blow: a wound gets through (a heavier man's
            // crushing through to knock him down), anything else is turned.
            Some((_, false, _)) => guard,
            Some(_) if crushes() => Act::Down,
            Some(_) => Act::Stagger,
            None if roll(i, j, blow, 1) < me.lands(defence) => Act::Stagger,
            None if crushes() => Act::Down,
            None => guard,
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

    /// How regiment `r`'s ranks dent in from the charge that ploughed into
    /// it, in its own frame (x along its right, y toward its rear): None
    /// when nothing ploughs it or the charger isn't on screen.
    fn dent(&self, r: usize, block: &Block, right: Vec2) -> Option<Dent> {
        let by = block
            .ploughed_by
            .filter(|_| block.ploughed > 0.0 && !block.routing)?;
        let b = by.index as usize;
        if self.owner.get(b) != Some(&Some(by)) || !self.live(b) {
            return None;
        }
        let to = self.centre[b] - block.centre;
        let from = Vec2::new(to.dot(right), -to.dot(block.facing));
        let push = (-from).normalize_or(Vec2::Y);
        let half = self.half[r];
        let extent = (half.x * push.x).abs() + (half.y * push.y).abs();
        Some(Dent {
            from,
            push,
            half: self.half[b].x + block.file_spacing,
            near: from.length() - extent,
            span: 2.0 * extent.max(0.1),
            depth: DENT_RANKS * block.rank_spacing * block.ploughed,
        })
    }

    /// Puppet `i`'s charge hits home on `j`: a heavier charger may knock him
    /// off his feet, and may stagger him if not.
    fn charge_home(&mut self, i: usize, j: usize) {
        let (me, them) = (self.fighter[i / SLOTS], self.fighter[j / SLOTS]);
        let b = j / SLOTS;
        if self.ploughed_by[b].is_some() && self.ploughed_by[b] == self.owner[i / SLOTS] {
            // Driven through by this charge: he goes down whatever his
            // weight, and the men either side of him in his rank stagger or
            // fall too.
            self.suffer(j, i, Act::Down);
            let (k, files) = (j % SLOTS, usize::from(self.files[b]));
            let left = (k % files > 0).then(|| k - 1);
            let right = (k % files + 1 < files).then_some(k + 1);
            for n in [left, right].into_iter().flatten() {
                if n < usize::from(self.models[b]) {
                    let n = b * SLOTS + n;
                    let act = if roll(i, n, 0, 4) < PLOUGH_KNOCKDOWN {
                        Act::Down
                    } else {
                        Act::Stagger
                    };
                    self.suffer(n, i, act);
                }
            }
            return;
        }
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
        if self.acting(j).is_some_and(|(now, f)| {
            (now > act || now == Act::Down)
                && !(now == Act::Kill && self.kill_lands_in(j, f) <= 0.0)
        }) {
            return;
        }
        self.act[j] = (act, self.time);
        let hit = match act {
            Act::Parry => Some(Hit::Parried),
            Act::Block => Some(Hit::Blocked),
            Act::Stagger => Some(Hit::Wounded),
            Act::Down => Some(Hit::Crushed),
            Act::None | Act::Kill => None,
        };
        if let Some(hit) = hit {
            self.impact(self.time, from, j, hit);
        }
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

    /// The general who deals `killer`'s killing blow on `victim`: his
    /// regiment's (or the killer himself, if he is the general), standing
    /// within [`HERO_REACH`] of the man, free to strike and rested since
    /// his last.
    fn hero_for(&self, killer: usize, victim: usize) -> Option<usize> {
        let kr = killer / SLOTS;
        let g = usize::from(self.general[kr]);
        let gi = kr * SLOTS + g;
        (g < usize::from(self.models[kr])
            && self.acting(gi).is_none()
            && self.brandish[gi] + HERO_REST <= self.time
            && self.champion_of(kr).is_none_or(|(.., c)| c != gi)
            && self.pos[gi].distance_squared(self.pos[victim]) <= HERO_REACH * HERO_REACH)
            .then_some(gi)
    }

    /// Where a man of regiment `kr` strikes at `victim`: where he stands,
    /// or, finishing him `lying` with a move made for it, his chest (where
    /// his model lies it, which for most is behind his feet).
    fn strike_spot(&self, kr: usize, victim: usize, lying: bool) -> Vec2 {
        let (pos, facing) = (self.pos[victim], self.facing[victim]);
        let chest = (lying && self.kills.finisher(self.unit[kr]).is_some())
            .then(|| {
                let unit = self.unit[victim / SLOTS];
                self.kills.chest(unit, self.looks[victim])
            })
            .flatten();
        chest.map_or(pos, |c| {
            pos + facing * c.y + Vec2::new(facing.y, -facing.x) * c.x
        })
    }

    /// How many of regiment `r`'s soldiers stand in its front rank.
    fn front(&self, r: usize) -> usize {
        (self.files[r] as usize).min(self.models[r] as usize)
    }

    /// How many of regiment `r`'s soldiers stand in its fighting ranks: the
    /// front one, and the second too for spears.
    fn band(&self, r: usize) -> usize {
        let ranks = if self.fighter[r].reach { 2 } else { 1 };
        (self.front(r) * ranks).min(self.models[r] as usize)
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
    #[cfg(test)]
    pub fn update(&mut self, block: &Block, owner: u8, dt: f32, out: impl FnMut(Soldier)) {
        self.update_on(block, owner, dt, |_| true, |_| 0.0, out);
    }

    /// [`Puppets::update`] on a map where soldiers may stand only where
    /// `firm` says (off deep water and cliffs): a soldier whose place in
    /// the block falls off firm ground stands at the nearest firm ground on
    /// the way in toward the middle of his block instead, so a block
    /// brushing the bank crowds along it rather than wading off the edge.
    /// `deck` is how high a soldier standing at a point is lifted off the
    /// ground by a bridge under him (0 off any).
    pub fn update_on(
        &mut self,
        block: &Block,
        owner: u8,
        dt: f32,
        firm: impl Fn(Vec2) -> bool,
        deck: impl Fn(Vec2) -> f32,
        mut out: impl FnMut(Soldier),
    ) {
        let r = block.handle.index as usize;
        let models = block.models.min(MAX_MODELS) as usize;
        let fresh = self.owner[r] != Some(block.handle);
        self.owner[r] = Some(block.handle);
        if fresh {
            self.partner[r * SLOTS..(r + 1) * SLOTS].fill(NONE);
            self.act[r * SLOTS..(r + 1) * SLOTS].fill((Act::None, 0.0));
            self.reeling[r * SLOTS..(r + 1) * SLOTS].fill(f32::NEG_INFINITY);
            self.told[r * SLOTS..(r + 1) * SLOTS].fill(None);
            self.filled[r * SLOTS..(r + 1) * SLOTS].fill(self.time);
            self.lift[r * SLOTS..(r + 1) * SLOTS].fill(0.0);
            self.last_fall[r] = (Vec2::ZERO, f32::NEG_INFINITY);
            self.glance[r * SLOTS..(r + 1) * SLOTS].fill(Head::default());
            self.brandish[r * SLOTS..(r + 1) * SLOTS].fill(f32::NEG_INFINITY);
            for k in 0..SLOTS {
                self.looks[r * SLOTS + k] = look_of(block.handle, k);
            }
        } else {
            self.fall(block, owner, models);
        }
        self.shot[r] = 0;
        self.unit[r] = block.unit;
        self.charge[r] = block.charge;
        self.ploughed_by[r] = block.ploughed_by.filter(|_| block.ploughed > 0.0);
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
        let general = block.general.map(usize::from);
        self.general[r] = block.general.unwrap_or(NO_GENERAL);
        let heed = if block.engaged || block.routing {
            Heed::default()
        } else {
            self.heeding(owner, block.centre)
        };
        let band = self.band(r);
        let pressing = block.engaged && !block.routing;
        let dent = self.dent(r, block, right);
        for k in 0..models {
            let i = r * SLOTS + k;
            let rank = f32::from(k as u16 / files);
            // On a wall each man stands on his tile, and climbs to its top.
            let held =
                (!self.wall[r].is_empty()).then(|| wall_stand(&self.wall[r], k, models, right));
            let mut local = slot_offset(
                k as u16,
                files,
                block.models,
                block.file_spacing,
                block.rank_spacing,
            ) * spread
                + jitter(block.handle, k) * scatter;
            if pressing {
                // The ranks behind press in on the fighting ones.
                local.y -= rank * PRESS * block.rank_spacing;
            }
            if let Some(d) = &dent {
                // Driven in where the charge hit, the men further from it
                // giving way less.
                let rel = local - d.from;
                let off = rel.perp_dot(d.push) / d.half;
                let fade = 1.0 - 0.5 * ((rel.dot(d.push) - d.near) / d.span).clamp(0.0, 1.0);
                local += d.push * (d.depth * (1.0 - off * off).max(0.0) * fade);
            }
            let mut slot = block.centre + right * local.x + back * local.y;
            if mess > 0.0 {
                // Jostled about: each soldier wanders round his slot.
                let p = unit_hash(block.handle, k, 7) * tau;
                let wander = Vec2::from_angle(self.time * DISORDER_RATE * tau + p);
                slot += wander * (DISORDER_DRIFT * mess * (0.5 + unit_hash(block.handle, k, 8)));
            }
            let slot = match held {
                Some((spot, _)) => spot,
                None => onto_ground(slot, block.centre, &firm),
            };
            // Up to the top of the wall or ladder, or onto a bridge's deck.
            let up = held.map_or_else(|| self.ladder[r].max(deck(slot)), |(_, lift)| lift);
            let climb = CLIMB_RATE * dt.max(0.0);
            self.lift[i] = if fresh {
                up
            } else {
                self.lift[i] + (up - self.lift[i]).clamp(-climb, climb)
            };
            // A champion out between the lines faces the other; a dueling
            // soldier in the front rank closes on his man, on a leash from
            // his slot, and a spearman behind him thrusts from his place;
            // one knocked down lies where he fell.
            let champion = champion_here
                .filter(|&(_, _, c)| c == i)
                .map(|(d, side, _)| (d, side));
            let j = self.partner[i];
            // A champion's moves come from the rolled duel.
            let duel = champion.map(|(d, side)| d.moves(side, block.fighter.shield));
            let acting = match duel {
                Some((act, _)) => act,
                None => self.acting(i).filter(|_| !block.routing),
            };
            // Dealing a killing blow, he strikes where his man stood; a
            // general brandishes his weapon over him after.
            let killing = matches!(acting, Some((Act::Kill, _)));
            let brandishing = self.brandish[i] > self.time && !killing && !block.routing;
            let foe = match champion {
                Some((d, side)) => match d.champion[1 - side] {
                    NONE => Some(d.spot),
                    other => Some(self.pos[other as usize]),
                },
                None if killing || brandishing => Some(self.killing[i].spot),
                None => (j != NONE && !block.routing).then(|| self.pos[j as usize]),
            };
            let down = matches!(acting, Some((Act::Down, _)));
            // Motion warping: a man of the front rank dealing a killing blow
            // makes for the distance it needs from where his man stood,
            // whatever his leash from his place, and gets there as it lands.
            // A champion ending his duel with a matched kill is warped onto
            // its reach the same way.
            let warp = match (champion, acting) {
                (Some((d, side)), _) => d
                    .kill_begun()
                    .filter(|_| d.won(side))
                    .map(|_| (self.stance(&d, side), d.blow_in())),
                (None, Some((Act::Kill, f))) if self.killing[i].front => {
                    let kill = self.killing[i];
                    let from = match kill.from {
                        Vec2::ZERO => (self.pos[i] - kill.spot).normalize_or(back),
                        from => from,
                    };
                    let exact = kill.spot + from * kill.gap;
                    Some((exact, self.kill_lands_in(i, f)))
                }
                _ => None,
            };
            let target = match (champion, foe) {
                (Some((d, side)), _) => self.stance(&d, side),
                // Down, or brandishing over his man, he stays where he is.
                _ if down || brandishing => self.pos[i],
                _ if warp.is_some() => warp.map_or(slot, |(exact, _)| exact),
                (None, Some(_)) if k >= files as usize => slot,
                (None, Some(f)) => {
                    let gap = if killing {
                        self.killing[i].gap
                    } else {
                        (block.fighter.gap() + self.fighter[j as usize / SLOTS].gap()) / 2.0
                    };
                    let spot = f + (self.pos[i] - f).normalize_or(back) * gap;
                    let leash = DUEL_LEASH + LEASH_BOLD * unit_hash(block.handle, k, 13);
                    slot + (spot - slot).clamp_length_max(leash)
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
                // Warped, he covers the rest of the way to his blow's
                // distance in his share of the time left until it lands, and
                // holds there as he follows through: his feet keep the pace.
                match warp {
                    Some((exact, left)) if left <= dt => {
                        self.pos[i] = exact;
                        self.vel[i] = Vec2::ZERO;
                    }
                    Some((exact, left)) => {
                        self.vel[i] = (exact - self.pos[i]) / left;
                        self.pos[i] += self.vel[i] * dt;
                    }
                    None => {
                        self.vel[i] = (v + push.clamp_length_max(accel) * dt).clamp_length_max(top);
                        self.pos[i] += self.vel[i] * dt;
                    }
                }
                // Face your man, or where you walk when it's more than a
                // shuffle, else the regiment's way; lie still when down. A
                // man warped onto his blow faces his man.
                let walking = warp.is_none() && self.vel[i].length() > 0.6 && dist > 0.4;
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
            let striking = block.engaged && k < band && champion.is_none();
            let waiting = pressing && !striking && champion.is_none();
            // About half the men waiting their turn shout now and then.
            let shout = (self.time / SHOUT_EVERY + unit_hash(block.handle, k, 15)).fract();
            let shouting = (waiting
                && speed < 0.15
                && unit_hash(block.handle, k, 14) < 0.5
                && shout * SHOUT_EVERY < SHOUT_SECONDS)
                .then_some(shout * SHOUT_EVERY);
            let phase = unit_hash(block.handle, k, 5);
            let dueling = foe.is_some() && striking && j != NONE;
            let beat = dueling.then(|| Self::beat(i, j as usize, self.time));
            let swing = match duel {
                Some((_, swing)) => swing,
                None => beat.and_then(|b| Some((self.swing(i, b)?, b))),
            };
            // A champion stands on guard once he has reached his place.
            let on_guard =
                dueling || champion.is_some() && self.pos[i].distance_squared(target) < 0.25;
            let duel_kill = champion.and_then(|(d, side)| d.kill_move(side));
            let fight = match (acting, swing) {
                _ if duel_kill.is_some() => duel_kill,
                _ if champion.is_some_and(|(d, side)| d.cheering(side)) => Some(Move::Cheer),
                (Some((Act::Kill, f)), _) => Some(match self.pair_of(i) {
                    Some((pair, _)) => Move::Kill { pair, at: f },
                    // A finisher made for it plays once through.
                    None => match self.killing[i].strike {
                        Some((_, seed)) => Move::Finish { at: f, seed },
                        None => Move::Swing {
                            at: kill_swing(f),
                            blow: None,
                        },
                    },
                }),
                (Some((Act::Down, f)), _) => Some(Move::Down(f)),
                (Some((Act::Stagger, f)), _) => Some(Move::Stagger(f)),
                (_, Some((at, beat))) => Some(Move::Swing {
                    at,
                    blow: Some(beat as u32),
                }),
                (Some((Act::Block, f)), _) => Some(Move::Block(f)),
                (Some((Act::Parry, f)), _) => Some(Move::Parry(f)),
                _ if brandishing => Some(Move::Cheer),
                _ if on_guard => Some(Move::Guard),
                // Not fighting: a speech to cheer, or their general's fall.
                _ if champion.is_none() => heed.of(block.handle, k, general == Some(k)),
                _ => None,
            };
            let reeling = acting.is_some_and(|(act, _)| act.reels());
            if let Some(beat) = beat.filter(|_| !reeling && !killing) {
                // The pair shares a rhythm, half a round apart: one steps in
                // to swing as the other gives ground.
                pos += facing * DUEL_STEP * (beat.fract() * tau).sin();
            } else if striking && fight.is_none() {
                let lunge = unit_hash(block.handle, k, 4);
                let swing = ((self.time * LUNGE_RATE + lunge) * tau).sin();
                pos += facing * LUNGE * swing.max(0.0);
            } else if speed < 0.15
                && !block.routing
                && champion.is_none()
                && held.is_none()
                && !killing
            {
                // At rest: shift weight and glance about; restlessly, waiting
                // to fight.
                let p = unit_hash(block.handle, k, 6) * tau;
                let sway_by = if waiting { SWAY * RESTLESS } else { SWAY };
                let sway = (self.time * 0.9 + p).sin() + 0.5 * (self.time * 2.3 + p * 1.7).sin();
                pos += right * sway_by * sway;
                let glance = (self.time * 0.31 + p).sin() * (self.time * 0.13 + p * 0.6).sin();
                facing = Vec2::from_angle(GLANCE * glance).rotate(facing);
            }
            // Not fighting, he looks at a comrade falling beside him, easing
            // toward it; and on top he flinches from a blow he takes.
            let (at, fell) = self.last_fall[r];
            let goal = if fight.is_none() && !fresh {
                let late = FALL_GLANCE_LATE * unit_hash(block.handle, k, 16);
                fall_glance(pos, facing, at, self.time - fell, late)
            } else {
                Head::default()
            };
            let ease = if fresh {
                1.0
            } else {
                (GLANCE_EASE * dt).min(1.0)
            };
            let g = &mut self.glance[i];
            g.yaw += (goal.yaw - g.yaw) * ease;
            g.pitch += (goal.pitch - g.pitch) * ease;
            if goal == Head::default() && g.yaw.abs() + g.pitch.abs() < 1e-3 {
                *g = Head::default();
            }
            let jolt = match acting {
                Some((act, f)) => flinch(act, f, 2.0 * phase - 1.0),
                None => Head::default(),
            };
            let head =
                (g.yaw != 0.0 || g.pitch != 0.0 || jolt != Head::default()).then_some(Head {
                    yaw: g.yaw + jolt.yaw,
                    pitch: g.pitch + jolt.pitch,
                });
            out(Soldier {
                pos,
                facing,
                speed,
                // Clearly backwards, so a sideways shuffle doesn't flip
                // the walk back and forth.
                backing: self.vel[i].dot(facing) < -0.5 * speed,
                striking,
                shouting,
                fight,
                phase,
                look: self.looks[i],
                general: general == Some(k),
                stoop: match acting {
                    Some((Act::Kill, f))
                        if self.killing[i].finish && self.killing[i].strike.is_none() =>
                    {
                        stoop(f)
                    }
                    _ => 0.0,
                },
                lift: self.lift[i],
                head,
            });
        }
    }

    /// Soldiers lost since last frame fall where they stand, and the soldier
    /// from the emptied last slot takes the place of each and walks up to
    /// fill the gap. A man the simulation's blows killed (see
    /// [`Puppets::struck`]) is the one who falls, to his striker's killing
    /// blow. For other losses the men who fall are the ones fighting: a
    /// front-rank soldier (any soldier, for a block under arrows), in melee
    /// one locked in a duel if any are, whose man swings the killing blow;
    /// a man struck down stands until the blow lands. A champion who has
    /// lost his duel falls to the other's last blow; one still fighting is
    /// never taken from the ranks.
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
            // The next man the simulation's blows killed, unless it is the
            // champion's turn to fall.
            let blow = slain
                .is_none()
                .then(|| self.slain.iter().position(|&(h, ..)| h == block.handle))
                .flatten()
                .map(|n| self.slain.remove(n));
            let champion = self.champion_of(r).map(|(_, _, c)| c);
            let out = |k: usize| champion == Some(base + k);
            let blow = blow.filter(|&(_, k, _)| usize::from(k) <= gone && !out(usize::from(k)));
            // Best a man and his partner fighting each other, then any man
            // with a partner, else anyone not out as a champion.
            let partner = |k: usize| self.partner[base + k] as usize;
            let pick = slain
                .map(|(champion, _)| champion - base)
                .or(blow.map(|(_, k, _)| usize::from(k)))
                .or_else(|| {
                    nth_of(pool, roll, |k| {
                        partner(k) != NONE as usize && self.partner[partner(k)] as usize == base + k
                    })
                })
                .or_else(|| nth_of(pool, roll, |k| partner(k) != NONE as usize && !out(k)))
                .or_else(|| nth_of(pool, roll, |k| !out(k)))
                .unwrap_or(gone);
            let (victim, last) = (base + pick, base + gone);
            let killer = match (slain, blow) {
                (Some((_, killer)), _) => killer,
                (None, Some((.., by))) if by != NONE => by as usize,
                _ => self.partner[victim] as usize,
            };
            // The matched kill he falls to, if his killer starts one now.
            let mut paired = None;
            // Knocked off his feet and down: he dies where he lies, and his
            // killer stands over him to finish him.
            let lying = slain.is_none()
                && matches!(self.acting(victim), Some((Act::Down, f)) if down_depth(f) > 0.5);
            let salt = self.time.to_bits() ^ 16;
            // His general, near, takes the blow from his man, coming in
            // beside him, with a matched kill if the two have one: if he can
            // get there by the time it lands.
            let hero = (slain.is_none() && killer != NONE as usize)
                .then(|| self.hero_for(killer, victim))
                .flatten()
                .and_then(|gi| {
                    let kr = gi / SLOTS;
                    let roll = unit_hash(block.handle, pick, salt);
                    let pair = (!lying)
                        .then(|| self.kills.pick(self.unit[kr], block.unit, roll))
                        .flatten();
                    let (gap, lands) = match pair {
                        Some((_, k)) => (k.reach, k.contact * k.seconds),
                        None if lying => match self.kills.finisher(self.unit[kr]) {
                            Some((reach, seconds)) => (reach, BLOW_AT * seconds),
                            None => (FINISH_GAP, BLOW_AT * SWING_SECONDS),
                        },
                        None => (
                            (self.fighter[kr].gap() + block.fighter.gap()) / 2.0,
                            BLOW_AT * SWING_SECONDS,
                        ),
                    };
                    let spot = self.strike_spot(kr, victim, lying);
                    let toward = (self.pos[gi] - spot).normalize_or(Vec2::Y);
                    let man = self.pos[killer] - spot;
                    let away = if toward.perp_dot(man) > 0.0 {
                        -1.0
                    } else {
                        1.0
                    };
                    let from = if gi == killer {
                        toward
                    } else {
                        Vec2::from_angle(HERO_ASIDE * away).rotate(toward)
                    };
                    let way = self.pos[gi].distance(spot + from * gap);
                    (way <= HERO_STRIDE * lands).then_some((gi, pair, from))
                });
            let killer = hero.map_or(killer, |(gi, ..)| gi);
            let blow_in = if killer == NONE as usize {
                None
            } else if slain.is_some() {
                // The other champion's last blow has landed, as their matched
                // kill's has if they play one: he lies as its half goes on.
                match self.duel.and_then(|d| Some((d.killing()?.0, d.blow_in()))) {
                    Some((pair, blow_in)) => {
                        paired = Some(pair);
                        Some(blow_in)
                    }
                    None => Some(0.0),
                }
            } else {
                match self.acting(killer) {
                    // His man is reeling or down and can't strike: he just
                    // drops.
                    Some((act, _)) if act.reels() => None,
                    // His man is already swinging a killing blow: he falls to
                    // it too.
                    Some((Act::Kill, f)) if self.kill_lands_in(killer, f) > 0.0 => {
                        // Unless one is on the ground and the blow is
                        // aimed at a standing man, or the other way about:
                        // then he just drops.
                        (self.killing[killer].finish == lying)
                            .then(|| self.kill_lands_in(killer, f))
                    }
                    // He is playing out a matched kill (or a finisher)
                    // that has landed: this man just drops, rather than cut
                    // it short.
                    Some((Act::Kill, _)) if self.kill_timing(killer).is_some() => None,
                    _ => {
                        self.act[killer] = (Act::Kill, self.time);
                        // He steps in to strike where his man stands, at
                        // the distance the two keep, or the matched kill's
                        // reach when they play one: only a man of the front
                        // rank, who can close to that reach from where he
                        // stands (one behind strikes from his place).
                        let kr = killer / SLOTS;
                        let apart = self.pos[killer].distance(self.pos[victim]);
                        let front = hero.is_some() || killer % SLOTS < self.front(kr);
                        let pair = match hero {
                            Some((_, pair, _)) => pair,
                            None => (!lying && front)
                                .then(|| {
                                    let roll = unit_hash(block.handle, pick, salt);
                                    self.kills.pick(self.unit[kr], block.unit, roll)
                                })
                                .flatten()
                                .filter(|&(_, k)| (apart - k.reach).abs() <= DUEL_LEASH),
                        };
                        let gap = match pair {
                            Some((_, k)) => k.reach,
                            None if lying => self.kills.finish_gap(self.unit[kr]),
                            None => (self.fighter[kr].gap() + block.fighter.gap()) / 2.0,
                        };
                        self.killing[killer] = Killing {
                            spot: self.strike_spot(kr, victim, lying),
                            gap,
                            pair,
                            finish: lying,
                            front,
                            from: hero.map_or(Vec2::ZERO, |(.., from)| from),
                            strike: lying
                                .then(|| self.kills.finisher(self.unit[kr]))
                                .flatten()
                                .map(|(_, seconds)| {
                                    let seed = unit_hash(block.handle, pick, salt ^ 1);
                                    (seconds, (seed * u32::MAX as f32) as u32)
                                }),
                        };
                        if hero.is_some() {
                            let length = self
                                .kill_timing(killer)
                                .map_or(Act::Kill.seconds(), |k| k.seconds);
                            self.brandish[killer] = self.time + length + HERO_BRANDISH;
                        }
                        paired = pair.map(|(id, _)| id);
                        Some(self.kill_lands_in(killer, 0.0))
                    }
                }
            };
            let (fell, facing) = match blow_in {
                Some(wait) if lying => (self.time + wait, self.facing[victim]),
                Some(wait) => (
                    self.time + wait,
                    (self.pos[killer] - self.pos[victim]).normalize_or(self.facing[victim]),
                ),
                None => (self.time, self.facing[victim]),
            };
            // A blow drives him back as he falls, a heavier man's further,
            // and a much heavier man's or a charge's throws him; a matched
            // kill plays his fall itself.
            let driven = match blow_in {
                _ if lying => FINISHED,
                _ if paired.is_some() => 0,
                Some(_) => {
                    let kr = killer / SLOTS;
                    let outweighs = self.fighter[kr].outweighs(&self.fighter[r], true);
                    let charging = (block.ploughed_by)
                        .filter(|_| block.ploughed > 0.0)
                        .is_some_and(|h| self.owner[kr] == Some(h));
                    if outweighs >= 2.0 || charging {
                        let left = unit_hash(block.handle, pick, self.time.to_bits() ^ 17) < 0.5;
                        THROWN | if left { SPIN_LEFT } else { 0 }
                    } else {
                        (1 + outweighs as u8) << RECOIL_SHIFT
                    }
                }
                None => 0,
            };
            let fallen = Fallen {
                pos: self.pos[victim],
                facing,
                fell,
                unit: block.unit,
                owner: {
                    debug_assert!(owner < 1 << RECOIL_SHIFT, "player {owner}");
                    owner | driven
                },
                look: self.looks[victim],
                pair: paired,
            };
            self.lay(fallen);
            // Men already looking at one man keep to him.
            let since = self.time - self.last_fall[r].1;
            if !(0.0..FALL_GLANCE_SECONDS + FALL_GLANCE_LATE).contains(&since) {
                self.last_fall[r] = (fallen.pos, fell);
            }
            if blow_in.is_some() {
                self.impact(fell, killer, victim, Hit::Killed);
            }
            if fell <= self.time {
                self.lodge(block.handle, gone, &fallen);
            }
            self.pos[victim] = self.pos[last];
            self.vel[victim] = self.vel[last];
            self.facing[victim] = self.facing[last];
            self.act[victim] = self.act[last];
            self.act[last] = (Act::None, 0.0);
            self.killing[victim] = self.killing[last];
            self.reeling[victim] = self.reeling[last];
            self.reeling[last] = f32::NEG_INFINITY;
            self.looks[victim] = self.looks[last];
            self.glance[victim] = self.glance[last];
            self.brandish[victim] = self.brandish[last];
            self.brandish[last] = f32::NEG_INFINITY;
            // Whoever steps into the gap finds a new man, so the blows he
            // struck from the last slot are done with.
            // Blows told on whoever stood in either slot are not his.
            self.filled[victim] = self.time;
            self.filled[last] = self.time;
            self.told[victim] = None;
            self.told[last] = None;
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

    /// Every fallen soldier still on the field, those making room for newer
    /// ones last.
    pub fn corpses(&self) -> impl Iterator<Item = Corpse> + '_ {
        let corpse = move |f: &Fallen, sink: f32| {
            let age = self.time - f.fell;
            let (pos, facing, lift) = f.pose(age);
            Corpse {
                pos,
                facing,
                lift,
                owner: f.player(),
                unit: f.unit,
                look: f.look,
                age,
                sink,
                kill: f.pair,
                lying: f.lying(),
            }
        };
        self.corpses.iter().map(move |f| corpse(f, 0.0)).chain(
            self.sinking
                .iter()
                .map(move |(f, from)| corpse(f, (self.time - from) / CORPSE_SINK_SECONDS)),
        )
    }

    /// How many fallen soldiers are on the field, sinking ones included.
    pub fn corpse_count(&self) -> usize {
        self.corpses.len() + self.sinking.len()
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

/// A stable look per soldier (see [`Soldier::look`]).
fn look_of(h: RegimentHandle, k: usize) -> u8 {
    (unit_hash(h, k, 16) * 256.0) as u8
}

/// A stable value in [0, 1) for blow `blow` between two puppets, one per
/// `salt`.
fn roll(i: usize, j: usize, blow: u32, salt: u32) -> f32 {
    mix(i as u32, j as u32, blow, salt)
}

/// A stable value in [0, 1) from four numbers.
pub(crate) fn mix(a: u32, b: u32, c: u32, d: u32) -> f32 {
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

/// `slot`, or if that is off `firm` ground, the first firm ground on the
/// way from it to `centre` (in quarter tiles); `slot` itself if there is
/// none (the whole block is in the water).
fn onto_ground(slot: Vec2, centre: Vec2, firm: &impl Fn(Vec2) -> bool) -> Vec2 {
    if firm(slot) {
        return slot;
    }
    let to = centre - slot;
    let steps = (to.length() / GROUND_STEP).ceil() as i32;
    (1..=steps)
        .map(|k| slot + to * (k as f32 / steps as f32))
        .find(|&p| firm(p))
        .unwrap_or(slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_fixed::Fx;

    fn slot(x: f32, lift: f32) -> WallSlot {
        WallSlot {
            centre: Vec2::new(x, 5.5),
            lift,
            half: Vec2::ZERO,
        }
    }

    #[test]
    fn soldiers_stand_one_to_a_wall_tile_when_there_are_tiles_enough() {
        let slots = [slot(1.5, 2.0), slot(2.5, 2.0), slot(3.5, 2.5)];
        for k in 0..3 {
            let (at, lift) = wall_stand(&slots, k, 3, Vec2::X);
            assert_eq!(at, slots[k].centre);
            assert_eq!(lift, slots[k].lift);
        }
    }

    #[test]
    fn men_sharing_a_wall_tile_spread_along_it() {
        // Five men on two tiles: tile 0 holds soldiers 0, 2 and 4, tile 1
        // holds 1 and 3.
        let slots = [slot(1.5, 2.0), slot(2.5, 2.0)];
        let stand = |k| wall_stand(&slots, k, 5, Vec2::X).0;
        let (a, b, c) = (stand(0), stand(2), stand(4));
        assert!(a.x < b.x && b.x < c.x);
        assert!((b.x - 1.5).abs() < 1e-5, "the middle man is on the centre");
        assert!(c.x - a.x < 1.0, "all within the tile");
        assert!((stand(1).x + stand(3).x - 5.0).abs() < 1e-5);
        assert!(stand(1).x < stand(3).x);
    }

    #[test]
    fn men_on_a_gatehouse_spread_along_its_length_at_the_deck() {
        // Three tiles long, running along y.
        let house = WallSlot {
            centre: Vec2::new(10.5, 21.5),
            lift: 2.0,
            half: Vec2::new(0.0, 1.5),
        };
        let stand = |k, models| wall_stand(&[house], k, models, Vec2::X);
        let three: Vec<_> = (0..3).map(|k| stand(k, 3)).collect();
        for (at, lift) in &three {
            assert_eq!((at.x, *lift), (10.5, 2.0));
        }
        // One to a tile, the middle man on the centre.
        let ys: Vec<f32> = three.iter().map(|(at, _)| at.y).collect();
        assert!(ys[0] < ys[1] && ys[1] < ys[2]);
        assert!((ys[1] - 21.5).abs() < 1e-5);
        assert!(ys[2] - ys[0] > 1.5, "{ys:?}");
        // However many, all stay on the gatehouse in ranks of three, and
        // no two share a spot.
        let mut seen: Vec<Vec2> = Vec::new();
        for k in 0..24 {
            let (at, _) = stand(k, 24);
            assert!(
                (20.0..=23.0).contains(&at.y) && (10.0..=11.0).contains(&at.x),
                "{at}"
            );
            assert!((at.y - 20.5 - (k % 3) as f32).abs() < 1e-5, "{at}");
            assert!(seen.iter().all(|p| p.distance(at) > 1e-3));
            seen.push(at);
        }
        // A tile of wall beside it still spreads across the way given.
        let tile = slot(11.5, 2.0);
        assert!((wall_stand(&[tile], 0, 2, Vec2::X).0.x - 11.5).abs() > 0.1);
    }

    #[test]
    fn a_regiment_on_a_wall_climbs_to_its_top_and_comes_down() {
        let mut p = Puppets::new(4, 16);
        let b = block(1, Vec2::new(10.0, 10.0), Vec2::NEG_Y, false);
        let slots = [slot(10.5, 2.0), slot(11.5, 2.0)];
        let mut last = Vec::new();
        p.stand_on_wall(b.handle, slots.into_iter());
        assert!(p.on_wall(b.handle) && p.wall_top(b.handle) == 2.0);
        for _ in 0..300 {
            last.clear();
            p.update(&b, 0, 0.016, |s| last.push(s));
        }
        for (k, s) in last.iter().enumerate() {
            assert!((s.lift - 2.0).abs() < 1e-3, "{}", s.lift);
            assert!(s.pos.distance(slots[k % 2].centre) < 0.5, "{k}: {}", s.pos);
        }
        // Down again: the men leave the wall.
        p.stand_on_wall(b.handle, std::iter::empty());
        assert!(!p.on_wall(b.handle) && p.wall_top(b.handle) == 0.0);
        p.stand_on_ladder(b.handle, 1.5);
        assert!(p.on_ladder(b.handle) && p.wall_top(b.handle) == 1.5);
        p.stand_on_wall(b.handle, std::iter::empty());
        assert!(!p.on_ladder(b.handle));
        for _ in 0..600 {
            last.clear();
            p.update(&b, 0, 0.016, |s| last.push(s));
        }
        assert!(last.iter().all(|s| s.lift.abs() < 1e-3));
    }

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
            ploughed: 0.0,
            ploughed_by: None,
            general: None,
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
            ploughed: 0.0,
            ploughed_by: None,
            general: None,
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
            ploughed: 0.0,
            ploughed_by: None,
            general: None,
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

        // The fallen stay as long as the battle lasts, however long that is.
        p.advance(3600.0);
        assert_eq!(p.corpses().count(), 2);
        assert!(p.corpses().all(|c| c.sink == 0.0));

        // Once the field is full (three here) the oldest sink away to make
        // room for the newest, and are gone three seconds later.
        block.models = 2;
        p.advance(0.016);
        p.update(&block, 2, 0.016, |_| {});
        assert_eq!(p.corpse_count(), 6);
        p.advance(CORPSE_SINK_SECONDS * 0.5);
        let mut sinks: Vec<f32> = p.corpses().map(|c| c.sink).collect();
        sinks.sort_by(f32::total_cmp);
        assert_eq!(sinks[..3], [0.0; 3]);
        assert!(
            sinks[3..].iter().all(|s| (s - 0.5).abs() < 0.01),
            "{sinks:?}"
        );
        p.advance(CORPSE_SINK_SECONDS);
        assert_eq!(p.corpse_count(), 3);
        assert!(p.corpses().all(|c| c.sink == 0.0));
    }

    #[test]
    fn the_last_soldiers_of_a_wiped_out_regiment_fall() {
        let mut p = Puppets::new(2, 16);
        let block = Block {
            handle: RegimentHandle {
                index: 1,
                generation: 0,
            },
            unit: 3,
            centre: Vec2::new(5.0, 5.0),
            facing: Vec2::NEG_Y,
            files: 4,
            models: 5,
            file_spacing: 1.0,
            rank_spacing: 1.0,
            speed: 3.0,
            engaged: true,
            routing: false,
            disorder: 0.0,
            charge: 0,
            ploughed: 0.0,
            ploughed_by: None,
            general: None,
            fighter: Fighter::default(),
        };
        p.update(&block, 2, 0.016, |_| {});
        // Gone from the simulation before the puppets saw a casualty: all
        // five lie where they stood, and a second report adds nobody.
        p.wiped(block.handle);
        p.wiped(block.handle);
        let fallen: Vec<Corpse> = p.corpses().collect();
        assert_eq!(fallen.len(), 5);
        assert!(fallen.iter().all(|c| c.owner == 2 && c.unit == 3));
        assert!(fallen.iter().all(|c| c.pos.distance(block.centre) < 3.0));
    }

    #[test]
    fn arrows_that_kill_stay_in_the_fallen() {
        let mut p = Puppets::new(1, 16);
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
            engaged: false,
            routing: false,
            disorder: 0.0,
            charge: 0,
            ploughed: 0.0,
            ploughed_by: None,
            general: None,
            fighter: Fighter::default(),
        };
        p.update(&block, 1, 0.016, |_| {});
        // A volley from the east kills two of the three who fall; the third
        // went to something else.
        p.shot_down(block.handle, Vec2::NEG_X, 2);
        block.models = 5;
        p.advance(0.016);
        p.update(&block, 1, 0.016, |_| {});
        let lodged: Vec<Lodged> = p.drain_lodged().collect();
        assert_eq!(lodged.len(), 2);
        for l in &lodged {
            assert_eq!(l.dir, Vec2::NEG_X);
            assert!(p.corpses().any(|c| c.pos.distance(l.pos) < 0.6), "{l:?}");
        }
        assert_eq!(p.drain_lodged().count(), 0);
        // Arrows that killed nobody this frame don't wait for later deaths.
        p.shot_down(block.handle, Vec2::X, 3);
        p.advance(0.016);
        p.update(&block, 1, 0.016, |_| {});
        block.models = 4;
        p.advance(0.016);
        p.update(&block, 1, 0.016, |_| {});
        assert_eq!(p.drain_lodged().count(), 0);
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
        blows_told(a, b, facing, seconds, None)
    }

    /// The same, with the simulation telling a blow by each of `a`'s front
    /// rank on his man, four times a second, that came out as
    /// `told`.
    fn blows_told(
        a: Fighter,
        b: Fighter,
        facing: Vec2,
        seconds: f32,
        told: Option<Outcome>,
    ) -> [u32; 6] {
        let mut p = Puppets::new(2, 64);
        let (x, mut y) = melee_pair(a, b);
        y.facing = facing;
        let mut seen = [(Act::None, 0.0); 4];
        let mut count = [0; 6];
        let every = (0.25 / FRAME) as usize;
        for f in 0..(seconds / FRAME) as usize {
            // From the second frame on, when the puppets know the blocks.
            if let Some(outcome) = told.filter(|_| f % every == 1) {
                // On the man he is fighting, as the simulation's man across
                // from him would be.
                for k in 0..4 {
                    let man = p.partner[k as usize];
                    p.struck(&Blow {
                        striker: x.handle,
                        striker_slot: k,
                        target: y.handle,
                        target_slot: if man == NONE {
                            k
                        } else {
                            (man as usize % SLOTS) as u8
                        },
                        outcome,
                    });
                }
            }
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

    /// Two regiments locked in melee, `a` facing north and `b` south, after
    /// two seconds of fighting.
    fn scrum(a: Block, b: Block) -> Puppets {
        let mut p = Puppets::new(2, 64);
        for _ in 0..(2.0 / FRAME) as usize {
            frame(&mut p, &a, &b);
        }
        p
    }

    #[test]
    fn spears_fight_from_the_second_rank() {
        let spears = Fighter {
            reach: true,
            ..Fighter::default()
        };
        let (a, b) = melee_pair(spears, Fighter::default());
        let p = scrum(a, b);
        let paired = |r: usize, ranks: core::ops::Range<usize>| {
            ranks.filter(|k| p.partner[r * SLOTS + k] != NONE).count()
        };
        assert_eq!(paired(0, 0..8), 8, "both spear ranks fight");
        assert_eq!(paired(1, 0..4), 4);
        assert_eq!(paired(1, 4..8), 0, "swords fight from the front rank only");
        // The second rank strikes from its place, over the shoulders of
        // the first: it is still behind it.
        for k in 0..4 {
            assert!(p.pos[4 + k].y > p.pos[k].y + 0.3, "slot {}", 4 + k);
        }
    }

    #[test]
    fn spearmen_hold_their_foe_off() {
        // How far apart the front ranks stand once they have closed.
        let apart = |a: Fighter| {
            let (x, y) = melee_pair(a, Fighter::default());
            let p = scrum(x, y);
            (0..4)
                .map(|k| p.pos[k].distance(p.pos[p.partner[k] as usize]))
                .sum::<f32>()
                / 4.0
        };
        let spears = apart(Fighter {
            reach: true,
            ..Fighter::default()
        });
        let swarm = apart(Fighter {
            mass: Mass::Light,
            ..Fighter::default()
        });
        let swords = apart(Fighter::default());
        assert!(spears > swords + 0.15, "spears {spears} swords {swords}");
        assert!(swarm < swords - 0.05, "swarm {swarm} swords {swords}");
    }

    #[test]
    fn men_spread_along_the_line_and_gang_up_where_it_overlaps() {
        // Level fronts: one man each.
        let (a, b) = melee_pair(Fighter::default(), Fighter::default());
        let p = scrum(a, b);
        let mut men: Vec<u32> = (0..4).map(|k| p.partner[k]).collect();
        men.sort_unstable();
        men.dedup();
        assert_eq!(men.len(), 4, "{men:?}");
        // Four men against two: two on each.
        let b = Block {
            files: 2,
            models: 4,
            ..block(1, Vec2::new(0.0, -2.6), Vec2::Y, true)
        };
        let p = scrum(block(0, Vec2::ZERO, Vec2::NEG_Y, true), b);
        let mut on = [0; 2];
        for k in 0..4 {
            let j = p.partner[k];
            assert_ne!(j, NONE, "slot {k} has no man");
            on[j as usize - SLOTS] += 1;
        }
        assert_eq!(on, [2, 2]);
    }

    #[test]
    fn men_waiting_their_turn_shout() {
        let (a, b) = melee_pair(Fighter::default(), Fighter::default());
        let mut p = Puppets::new(2, 64);
        let (mut front, mut rear) = (0, 0);
        for _ in 0..(SHOUT_EVERY * 2.0 / FRAME) as usize {
            p.advance(FRAME);
            p.pair(|x, y| x != y);
            let mut k = 0;
            p.update(&a, 0, FRAME, |s| {
                if s.shouting.is_some() {
                    if k < 4 { front += 1 } else { rear += 1 }
                }
                k += 1;
            });
            p.update(&b, 1, FRAME, |_| {});
        }
        assert_eq!(front, 0);
        assert!(rear > 0);
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
    fn swings_play_the_simulations_blows() {
        // At 90% odds, blows the simulation says were parried are parried...
        let sure = fighter(80, 20);
        let c = blows_told(sure, fighter(0, 20), Vec2::Y, 60.0, Some(Outcome::Parried));
        let total: u32 = c.iter().sum();
        assert!(total > 100, "{c:?}");
        assert!(c[Act::Parry as usize] * 100 > total * 80, "{c:?}");
        // ...and at 10%, blows it says wounded stagger their man.
        let c = blows_told(
            fighter(0, 20),
            fighter(0, 60),
            Vec2::Y,
            60.0,
            Some(Outcome::Wounded),
        );
        let total: u32 = c.iter().sum();
        assert!(total > 100, "{c:?}");
        assert!(c[Act::Stagger as usize] * 100 > total * 80, "{c:?}");
        // A heavy blade the simulation says was parried never crushes
        // through; one it says wounded often does.
        let heavy = Fighter {
            mass: Mass::Heavy,
            heavy_blade: true,
            ..fighter(0, 20)
        };
        let light = Fighter {
            mass: Mass::Light,
            ..fighter(0, 60)
        };
        let c = blows_told(heavy, light, Vec2::Y, 60.0, Some(Outcome::Parried));
        assert!(c.iter().sum::<u32>() > 100, "{c:?}");
        assert_eq!(c[Act::Down as usize], 0, "{c:?}");
        let c = blows_told(heavy, light, Vec2::Y, 60.0, Some(Outcome::Wounded));
        assert!(c[Act::Down as usize] > 0, "{c:?}");
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
    fn blows_are_told_as_they_land_and_a_kill_as_its_man_falls() {
        let mut p = Puppets::new(2, 4);
        let (a, mut b) = melee_pair(Fighter::default(), Fighter::default());
        let mut hits = Vec::new();
        for _ in 0..600 {
            frame(&mut p, &a, &b);
            hits.extend(p.drain_impacts().map(|i| i.hit));
        }
        assert!(hits.contains(&Hit::Parried) || hits.contains(&Hit::Wounded));
        assert!(!hits.contains(&Hit::Killed));
        // The man who is killed stands until the blow lands: told then.
        b.models -= 1;
        frame(&mut p, &a, &b);
        assert!(p.drain_impacts().all(|i| i.hit != Hit::Killed));
        p.advance(BLOW_AT * SWING_SECONDS);
        let told: Vec<Impact> = p.drain_impacts().collect();
        assert_eq!(told.iter().filter(|i| i.hit == Hit::Killed).count(), 1);
        // Collected once.
        assert_eq!(p.drain_impacts().count(), 0);
        // The blow travels from the killer toward the man struck.
        let killed = told.iter().find(|i| i.hit == Hit::Killed).unwrap();
        assert!((killed.dir.length() - 1.0).abs() < 1e-3);
        // `melee_pair` stands the man struck south of his killer.
        assert!(killed.dir.y < 0.0, "{:?}", killed.dir);
    }

    #[test]
    fn a_blow_taken_snaps_the_head_back_and_it_comes_back() {
        let at = |f| flinch(Act::Stagger, f, 1.0);
        let peak = at(HEAD_FLINCH_PEAK);
        assert!(peak.pitch < 0.0 && peak.yaw > 0.0, "{peak:?}");
        assert_eq!(peak.pitch, -HEAD_FLINCH_STAGGER);
        assert!(at(0.05).pitch > peak.pitch);
        assert_eq!(at(HEAD_FLINCH_ENDS), Head::default());
        // A blow caught jolts less than one taken, and turns him aside less.
        let block = flinch(Act::Block, HEAD_FLINCH_PEAK, 1.0);
        assert!(block.pitch > peak.pitch && block.yaw < peak.yaw);
        assert_eq!(flinch(Act::Kill, 0.2, 1.0), Head::default());
    }

    #[test]
    fn a_man_looks_at_a_comrade_falling_beside_him_but_not_behind() {
        let (me, north) = (Vec2::ZERO, Vec2::Y);
        let left = Vec2::new(north.y, -north.x);
        // Ahead and to his left: turned left and looking down, eased in.
        let g = fall_glance(me, north, Vec2::new(0.0, 1.0) + left, 0.5, 0.0);
        assert!(g.yaw > 0.7 && g.pitch == FALL_GLANCE_DOWN, "{g:?}");
        assert!(fall_glance(me, north, left * 1.5, 0.05, 0.0).yaw < g.yaw);
        // Not before he fell, nor once a slow man has yet to react, nor after.
        assert_eq!(fall_glance(me, north, left, -0.1, 0.0), Head::default());
        assert_eq!(fall_glance(me, north, left, 0.1, 0.2), Head::default());
        assert_eq!(
            fall_glance(me, north, left, FALL_GLANCE_SECONDS, 0.0),
            Head::default()
        );
        // Not behind him, at his feet or out of reach; fading toward each.
        assert_eq!(
            fall_glance(me, north, -north * 1.5, 0.5, 0.0),
            Head::default()
        );
        assert_eq!(
            fall_glance(me, north, north * 0.2, 0.5, 0.0),
            Head::default()
        );
        assert_eq!(
            fall_glance(me, north, north * 3.0, 0.5, 0.0),
            Head::default()
        );
        let edge = fall_glance(me, north, north * (FALL_GLANCE_REACH - 0.25), 0.5, 0.0);
        assert!(
            (edge.pitch - FALL_GLANCE_DOWN / 2.0).abs() < 1e-4,
            "{edge:?}"
        );
    }

    #[test]
    fn men_turn_their_heads_to_a_comrade_falling_beside_them() {
        let mut p = Puppets::new(2, 4);
        let (a, mut b) = melee_pair(Fighter::default(), Fighter::default());
        for _ in 0..120 {
            frame(&mut p, &a, &b);
        }
        b.models -= 1;
        frame(&mut p, &a, &b);
        let (at, fell) = p.last_fall[1];
        assert!(fell.is_finite());
        // Once he is down, the men not fighting turn his way, easing round.
        p.time = fell + FALL_GLANCE_LATE + 0.2;
        let mut looked = 0;
        for _ in 0..20 {
            looked = 0;
            p.update(&b, 1, FRAME, |s| {
                if let Some(h) = s.head.filter(|_| s.fight.is_none()) {
                    let left = Vec2::new(s.facing.y, -s.facing.x);
                    // Clearly to one side, he looks that way.
                    let side = (at - s.pos).dot(left);
                    assert!(side.abs() < 0.5 || h.yaw * side > 0.0, "{h:?} {side}");
                    assert!(h.pitch > 0.0);
                    looked += 1;
                }
            });
            p.advance(FRAME);
        }
        assert!(looked > 0);
        // And look away again, easing back.
        p.advance(FALL_GLANCE_SECONDS);
        for _ in 0..60 {
            p.update(&b, 1, FRAME, |_| {});
            p.advance(FRAME);
        }
        p.update(&b, 1, FRAME, |s| {
            assert!(s.fight.is_some() || s.head.is_none(), "{:?}", s.head);
        });
    }

    /// A melee where `a`'s general (slot 5, second rank) stands near
    /// `b`'s front, run until the two lines have met.
    fn a_general_in_melee(p: &mut Puppets) -> (Block, Block) {
        let (mut a, b) = melee_pair(Fighter::default(), Fighter::default());
        a.general = Some(5);
        for _ in 0..120 {
            frame(p, &a, &b);
        }
        (a, b)
    }

    #[test]
    fn a_general_steps_through_to_deal_a_matched_kill_and_brandishes() {
        const KILL: KillTiming = KillTiming {
            seconds: 1.8,
            contact: 0.5,
            reach: 1.2,
        };
        let mut p = with_a_kill(KILL);
        let (a, mut b) = a_general_in_melee(&mut p);
        // A man of theirs falls near him: he deals the blow, not his man.
        b.models -= 1;
        frame(&mut p, &a, &b);
        let (at, fell) = p.last_fall[1];
        assert!(
            p.pos[5].distance(at) <= HERO_REACH,
            "this seed's man falls near him"
        );
        assert!(
            matches!(p.acting(5), Some((Act::Kill, _))),
            "{:?}",
            p.act[5]
        );
        assert!(p.pair_of(5).is_some(), "with their matched kill");
        let striker = (0..4).find(|&k| p.partner[k] as usize >= SLOTS);
        assert!((0..4).all(|k| !matches!(p.acting(k), Some((Act::Kill, _)))));
        // He reaches the kill's reach as it lands, beside his man.
        while p.time < fell {
            frame(&mut p, &a, &b);
        }
        assert!((p.pos[5].distance(at) - KILL.reach).abs() < 0.05);
        if let Some(k) = striker {
            assert!(p.pos[5].distance(p.pos[k]) > 0.3, "not inside his man");
        }
        // Played out, he stays where he struck, brandishing, facing the man.
        let struck_from = p.pos[5];
        let mut cheered = 0;
        for _ in 0..((KILL.seconds * (1.0 - KILL.contact) + HERO_BRANDISH) / FRAME) as usize {
            p.advance(FRAME);
            p.pair(|x, y| x != y);
            p.update(&a, 0, FRAME, |s| {
                if s.general && s.fight == Some(Move::Cheer) {
                    cheered += 1;
                    assert!(s.facing.dot((at - s.pos).normalize()) > 0.7, "{s:?}");
                }
            });
            p.update(&b, 1, FRAME, |_| {});
        }
        assert!(cheered > 20, "{cheered}");
        assert!(p.pos[5].distance(struck_from) < 0.3);
        // And rests before he takes another.
        b.models -= 1;
        frame(&mut p, &a, &b);
        assert!(!matches!(p.acting(5), Some((Act::Kill, _))));
    }

    #[test]
    fn a_general_too_far_to_reach_a_single_blow_leaves_it_to_his_man() {
        let mut p = Puppets::new(2, 4);
        let (a, mut b) = a_general_in_melee(&mut p);
        b.models -= 1;
        frame(&mut p, &a, &b);
        // From the second rank he can't cover the ground by its landing.
        assert!(!matches!(p.acting(5), Some((Act::Kill, _))));
        assert!((0..4).any(|k| matches!(p.acting(k), Some((Act::Kill, _)))));
    }

    #[test]
    fn a_general_stops_brandishing_when_his_men_run() {
        let mut p = Puppets::new(2, 4);
        let (mut a, b) = a_general_in_melee(&mut p);
        p.brandish[5] = p.time + 5.0;
        frame(&mut p, &a, &b);
        let mut fight = None;
        p.update(&a, 0, 0.0, |s| {
            fight = fight.or(s.general.then_some(s.fight));
        });
        assert_eq!(fight, Some(Some(Move::Cheer)));
        a.routing = true;
        a.engaged = false;
        p.update(&a, 0, FRAME, |s| {
            if s.general {
                assert_ne!(s.fight, Some(Move::Cheer));
            }
        });
    }

    #[test]
    fn a_full_queue_gives_up_lesser_blows_for_kills() {
        let mut p = Puppets::new(2, 4);
        let (a, b) = melee_pair(Fighter::default(), Fighter::default());
        frame(&mut p, &a, &b);
        p.impacts.clear();
        for _ in 0..MAX_IMPACTS + 10 {
            p.impact(0.0, 0, SLOTS, Hit::Parried);
        }
        assert_eq!(p.impacts.len(), MAX_IMPACTS);
        p.impact(0.0, 0, SLOTS, Hit::Killed);
        assert_eq!(p.impacts.len(), MAX_IMPACTS);
        assert_eq!(
            p.impacts
                .iter()
                .filter(|(_, i)| i.hit == Hit::Killed)
                .count(),
            1
        );
        // Further lesser blows are turned away, and kills keep their place.
        p.impact(0.0, 0, SLOTS, Hit::Wounded);
        assert!(p.impacts.iter().all(|(_, i)| i.hit != Hit::Wounded));
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
            matches!(fights[killers[0]], Some(Move::Swing { at, blow: None }) if at < 0.1),
            "{:?}",
            fights[killers[0]]
        );
        p.advance(BLOW_AT * SWING_SECONDS);
        assert!(p.corpses().next().unwrap().age >= 0.0);
    }

    #[test]
    fn a_kill_lines_up_drives_its_man_back_and_holds_the_follow_through() {
        // How far back the dead man was driven, and the killer's fights.
        let kill = |killer: Mass| {
            let mut p = Puppets::new(2, 4);
            let (a, mut b) = melee_pair(
                Fighter {
                    mass: killer,
                    ..Fighter::default()
                },
                Fighter::default(),
            );
            for _ in 0..30 {
                frame(&mut p, &a, &b);
            }
            b.models -= 1;
            frame(&mut p, &a, &b);
            let k = (0..4).find(|&k| p.act[k].0 == Act::Kill).expect("a killer");
            let stood = p.corpses().next().unwrap().pos;
            assert_eq!(p.killing[k].spot, stood);
            let mut fights = Vec::new();
            let mut t = 0.0;
            while t < SWING_SECONDS + KILL_HOLD + RECOIL_SECONDS {
                p.advance(FRAME);
                p.pair(|x, y| x != y);
                p.update(&a, 0, FRAME, |s| fights.push((t, s.fight, s.pos)));
                p.update(&b, 1, FRAME, |_| {});
                t += FRAME;
            }
            let lies = p.corpses().next().unwrap();
            let men = fights.iter().filter(|(t, ..)| *t == 0.0).count();
            let killer_at = fights[fights.len() - men + k].2;
            // He fell away from his killer, along the blow.
            assert!(
                lies.pos.distance(killer_at) > stood.distance(killer_at),
                "{lies:?} {stood} {killer_at}"
            );
            // The killer swung, then held his follow-through.
            let held = fights
                .iter()
                .skip(k)
                .step_by(men)
                .filter(|(t, ..)| {
                    *t > SWING_SECONDS + 0.05 && *t < SWING_SECONDS + KILL_HOLD - 0.05
                })
                .all(|(_, f, _)| matches!(f, Some(Move::Swing { at, .. }) if *at == HELD));
            assert!(held, "{fights:?}");
            lies.pos.distance(stood)
        };
        let (light, heavy) = (kill(Mass::Light), kill(Mass::Heavy));
        assert!((light - RECOIL).abs() < 0.02, "{light}");
        assert!(heavy > light + RECOIL / 2.0, "{heavy} vs {light}");
    }

    #[test]
    fn a_matched_kill_plays_both_halves_together() {
        const KILL: KillTiming = KillTiming {
            seconds: 1.8,
            contact: 0.2,
            reach: 1.2,
        };
        let mut p = with_a_kill(KILL);
        let (a, mut b) = melee_pair(Fighter::default(), Fighter::default());
        for _ in 0..30 {
            frame(&mut p, &a, &b);
        }
        b.models -= 1;
        frame(&mut p, &a, &b);
        let k = (0..4).find(|&k| p.act[k].0 == Act::Kill).expect("a killer");
        // He stands at the kill's reach, and the two start their halves at
        // once: the blow lands as the victim's half reaches its contact.
        assert_eq!(p.killing[k].pair, Some((0, KILL)));
        assert_eq!(p.killing[k].gap, KILL.reach);
        let stood = p.corpses().next().unwrap();
        assert_eq!(stood.kill, Some(0));
        let lands = KILL.contact * KILL.seconds;
        assert!((stood.age + lands).abs() < 1e-4, "{stood:?}");
        assert!(p.drain_impacts().all(|i| i.hit != Hit::Killed));
        let mut fights = Vec::new();
        let mut t = 0.0;
        let mut told = None;
        while t < KILL.seconds + 0.1 {
            p.advance(FRAME);
            t += FRAME;
            p.pair(|x, y| x != y);
            p.update(&a, 0, FRAME, |s| fights.push((t, s.fight, s.pos)));
            p.update(&b, 1, FRAME, |_| {});
            if p.drain_impacts().any(|i| i.hit == Hit::Killed) {
                told.get_or_insert(t);
            }
        }
        let told = told.expect("the blow was told");
        assert!((told - lands).abs() <= FRAME, "{told} vs {lands}");
        // He plays his half through to its end, then fights on.
        let men = fights.iter().filter(|(t, ..)| *t == FRAME).count();
        let his: Vec<_> = fights.iter().skip(k).step_by(men).collect();
        for (t, fight, _) in &his {
            match fight {
                Some(Move::Kill { pair: 0, at }) => {
                    assert!((at - t / KILL.seconds).abs() < 0.02, "{t}: {at}");
                }
                _ => assert!(*t > KILL.seconds - FRAME, "{t}: {fight:?}"),
            }
        }
        // He closed to the kill's reach as the blow landed, and the victim
        // was not driven back: his half plays his fall.
        let at_contact = his.iter().find(|(t, ..)| *t >= lands).unwrap().2;
        let lies = p.corpses().next().unwrap();
        assert_eq!(lies.pos, stood.pos);
        let apart = lies.pos.distance(at_contact);
        assert!((apart - KILL.reach).abs() < 0.01, "{apart}");
    }

    /// Puppets whose men (unit 0) all play one matched kill on each other.
    fn with_a_kill(kill: KillTiming) -> Puppets {
        let mut p = Puppets::new(2, 4);
        let mut kills = Kills::new(vec![kill]);
        kills.add(0, 0, &[0]);
        p.set_kills(kills);
        p
    }

    #[test]
    fn a_matched_kill_is_played_out_and_only_from_the_front_rank() {
        const KILL: KillTiming = KillTiming {
            seconds: 1.8,
            contact: 0.2,
            reach: 1.2,
        };
        let mut p = with_a_kill(KILL);
        let (a, mut b) = melee_pair(Fighter::default(), Fighter::default());
        for _ in 0..30 {
            frame(&mut p, &a, &b);
        }
        // A man of the second rank strikes the blow: his own swing, from
        // his place.
        let rear = 5;
        assert!(rear >= p.front(0));
        p.slain.push((b.handle, 0, rear as u32));
        b.models -= 1;
        frame(&mut p, &a, &b);
        assert_eq!(p.act[rear].0, Act::Kill);
        assert_eq!(p.killing[rear].pair, None);
        assert_eq!(p.corpses().last().unwrap().kill, None);
        // A front-rank man plays the kill; a man who falls to him once it
        // has landed just drops, and he plays it out.
        b.models -= 1;
        frame(&mut p, &a, &b);
        let k = (0..4).find(|&k| p.act[k].0 == Act::Kill).expect("a killer");
        let started = p.act[k].1;
        assert!(p.pair_of(k).is_some());
        while p.time - started < KILL.contact * KILL.seconds + 0.1 {
            frame(&mut p, &a, &b);
        }
        let next = p.partner[k];
        assert_ne!(next, NONE, "he has a new man");
        p.slain
            .push((b.handle, (next as usize % SLOTS) as u8, k as u32));
        b.models -= 1;
        frame(&mut p, &a, &b);
        let dropped = p.corpses().last().unwrap();
        assert!(dropped.age >= 0.0 && dropped.kill.is_none(), "{dropped:?}");
        assert_eq!(p.act[k].1, started);
        assert!(p.pair_of(k).is_some());
    }

    #[test]
    fn a_much_heavier_blow_or_a_charge_throws_its_man() {
        // How the dead man flew: his lift and facing over his first second,
        // and where he started and came down.
        let thrown = |killer: Mass, victim: Mass, charge: bool| {
            let mut p = Puppets::new(2, 4);
            let (a, mut b) = melee_pair(
                Fighter {
                    mass: killer,
                    ..Fighter::default()
                },
                Fighter {
                    mass: victim,
                    ..Fighter::default()
                },
            );
            for _ in 0..30 {
                frame(&mut p, &a, &b);
            }
            if charge {
                b.ploughed_by = Some(a.handle);
                b.ploughed = 1.0;
            }
            b.models -= 1;
            frame(&mut p, &a, &b);
            let stood = p.corpses().next().unwrap();
            let mut flight = Vec::new();
            while p.corpses().next().unwrap().age < 1.0 {
                frame(&mut p, &a, &b);
                let c = p.corpses().next().unwrap();
                flight.push((c.age, c.lift, c.facing));
            }
            (stood, flight, p.corpses().next().unwrap())
        };
        // A blow one class heavier only drives its man back.
        let (stood, flight, lies) = thrown(Mass::Heavy, Mass::Medium, false);
        assert!(
            flight
                .iter()
                .all(|&(_, lift, facing)| lift == 0.0 && facing == stood.facing)
        );
        assert!(lies.pos.distance(stood.pos) < THROW / 2.0, "{lies:?}");
        for charge in [false, true] {
            let (killer, victim) = if charge {
                (Mass::Medium, Mass::Medium)
            } else {
                (Mass::Heavy, Mass::Light)
            };
            let (stood, flight, lies) = thrown(killer, victim, charge);
            // In the air after the blow lands, and down again soon after.
            assert!(
                flight
                    .iter()
                    .any(|&(age, lift, _)| age > 0.1 && age < 0.3 && lift > 0.2),
                "{charge}: {flight:?}"
            );
            // Down before the blow lands and once he comes down.
            assert!(
                (flight.iter())
                    .filter(|&&(age, ..)| age <= 0.0 || age >= FLIGHT_SECONDS)
                    .all(|&(_, lift, _)| lift == 0.0)
            );
            // Most of a tile back along the blow, turned over.
            assert!(
                (lies.pos.distance(stood.pos) - THROW).abs() < 1e-3,
                "{charge}: {lies:?}"
            );
            let turned = lies.facing.angle_to(stood.facing).abs();
            assert!(
                (turned - THROW_TURN).abs() < 1e-3,
                "{charge}: {lies:?} {stood:?}"
            );
            assert!((lies.facing.length() - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn a_man_killed_on_the_ground_is_finished_where_he_lies() {
        finished_where_he_lies(Puppets::new(2, 4), FINISH_GAP, false);
    }

    #[test]
    fn a_finisher_made_for_it_is_struck_from_its_own_reach() {
        let mut p = Puppets::new(2, 4);
        let mut kills = Kills::new(Vec::new());
        kills.add_finisher(0, 1.6, 1.5);
        // His chest lies behind his feet, a little to his left.
        for unit in 0..2 {
            kills.add_chests(unit, vec![Some(CHEST); 256]);
        }
        p.set_kills(kills);
        finished_where_he_lies(p, 1.6, true);
    }

    /// Where the lying men of [`a_finisher_made_for_it_is_struck_from_its_own_reach`]
    /// have their chests.
    const CHEST: Vec2 = Vec2::new(0.2, -0.8);

    /// A man knocked down and killed lying is finished by his killer from
    /// `gap` away, striking down at him: with a move made for it (`strike`),
    /// else bent over his swing.
    fn finished_where_he_lies(mut p: Puppets, gap: f32, strike: bool) {
        let (a, mut b) = melee_pair(Fighter::default(), Fighter::default());
        for _ in 0..30 {
            frame(&mut p, &a, &b);
        }
        // Knocked down, and lying still a moment later.
        let k = (0..4).find(|&k| p.partner[k] != NONE).expect("a pair");
        let down = p.partner[k] as usize;
        p.suffer(down, k, Act::Down);
        while !matches!(p.acting(down), Some((Act::Down, f)) if down_depth(f) >= 1.0) {
            p.advance(FRAME);
        }
        let (lay, facing) = (p.pos[down], p.facing[down]);
        p.slain.push((b.handle, (down % SLOTS) as u8, k as u32));
        b.models -= 1;
        frame(&mut p, &a, &b);
        // His killer stands over him to strike down at him...
        assert_eq!(p.act[k].0, Act::Kill);
        assert!(p.killing[k].finish && p.killing[k].gap == gap);
        let lies = p.corpses().last().unwrap();
        assert!(lies.lying && lies.age < 0.0, "{lies:?}");
        let (mut stooped, mut landed, mut struck) = (0.0_f32, None, false);
        while p.corpses().last().unwrap().age < 1.0 {
            p.advance(FRAME);
            p.pair(|x, y| x != y);
            p.update(&a, 0, FRAME, |s| {
                stooped = stooped.max(s.stoop);
                struck |= matches!(s.fight, Some(Move::Finish { .. }));
            });
            p.update(&b, 1, FRAME, |_| {});
            if p.corpses().last().unwrap().age >= 0.0 {
                // A finishing move strikes at his chest, behind his feet.
                let chest = if strike {
                    lay + facing * CHEST.y + Vec2::new(facing.y, -facing.x) * CHEST.x
                } else {
                    lay
                };
                landed.get_or_insert(p.pos[k].distance(chest));
            }
        }
        assert_eq!(struck, strike);
        if strike {
            assert_eq!(stooped, 0.0);
        } else {
            assert!(stooped > 0.99, "{stooped}");
        }
        // ...and he lies as he was, not driven or turned.
        let lies = p.corpses().last().unwrap();
        assert!(lies.lying);
        assert!(lies.pos.distance(lay) < 0.2, "{lies:?} {lay}");
        assert_eq!(lies.facing, facing);
        // He stood over him as the blow landed.
        let landed = landed.unwrap();
        assert!((landed - gap).abs() < 0.01, "{landed}");
    }

    #[test]
    fn a_killer_is_warped_onto_his_blow_even_beyond_his_leash() {
        let mut p = Puppets::new(2, 4);
        let (a, mut b) = melee_pair(Fighter::default(), Fighter::default());
        for _ in 0..30 {
            frame(&mut p, &a, &b);
        }
        b.models -= 1;
        frame(&mut p, &a, &b);
        let k = (0..4).find(|&k| p.act[k].0 == Act::Kill).expect("a killer");
        // His man stood further off than his leash lets him go.
        let spot = p.pos[k] + Vec2::new(0.0, -3.0);
        p.killing[k].spot = spot;
        let gap = p.killing[k].gap;
        let lands = p.kill_lands_in(k, 0.0);
        let (mut t, mut at_blow, mut held) = (0.0, None, Vec::new());
        while t < Act::Kill.seconds() - 2.0 * FRAME {
            p.advance(FRAME);
            t += FRAME;
            p.pair(|x, y| x != y);
            let mut men = Vec::new();
            p.update(&a, 0, FRAME, |s| men.push(s));
            p.update(&b, 1, FRAME, |_| {});
            let his = men[k];
            if t >= lands {
                at_blow.get_or_insert(his.pos.distance(spot));
                held.push(his.speed);
            }
        }
        let at_blow = at_blow.unwrap();
        assert!((at_blow - gap).abs() < 0.01, "{at_blow} vs {gap}");
        // Held there, standing, as he follows through.
        assert!(held.iter().skip(1).all(|&v| v < 0.15), "{held:?}");
    }

    #[test]
    fn a_man_killed_during_the_follow_through_waits_for_a_new_blow() {
        let mut p = Puppets::new(2, 4);
        let (a, mut b) = melee_pair(Fighter::default(), Fighter::default());
        for _ in 0..30 {
            frame(&mut p, &a, &b);
        }
        b.models -= 1;
        frame(&mut p, &a, &b);
        let k = (0..4).find(|&k| p.act[k].0 == Act::Kill).expect("a killer");
        // The blow lands; he follows through.
        while !matches!(p.acting(k), Some((Act::Kill, f)) if kill_swing(f) == HELD) {
            frame(&mut p, &a, &b);
        }
        // His next man falls to a fresh swing of his, not on the spot.
        let partner = p.partner[k];
        assert_ne!(partner, NONE, "he has a new man");
        p.slain
            .push((b.handle, (partner as usize % SLOTS) as u8, k as u32));
        b.models -= 1;
        frame(&mut p, &a, &b);
        let newest = p.corpses().last().unwrap();
        assert!(newest.age < 0.0, "{newest:?}");
        assert!(matches!(p.acting(k), Some((Act::Kill, f)) if kill_swing(f) < BLOW_AT));
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

    #[test]
    fn a_charge_that_ploughs_in_dents_the_line_and_bowls_men_over() {
        // Narrow chargers into the middle of a wide line: where the front
        // rank stands after settling, along the line's facing, and how many
        // of them were knocked down.
        let line = |ploughed: f32| {
            let mut p = Puppets::new(2, 64);
            let mut a = block(0, Vec2::ZERO, Vec2::NEG_Y, true);
            (a.files, a.models) = (2, 4);
            let mut b = block(1, Vec2::new(0.0, -3.2), Vec2::Y, true);
            (b.files, b.models) = (8, 16);
            (a.engaged, b.engaged) = (false, false);
            frame(&mut p, &a, &b);
            // The charge hits home, ploughing in or not.
            (a.engaged, b.engaged, a.charge) = (true, true, 60);
            (b.ploughed, b.ploughed_by) = (ploughed, Some(a.handle));
            for _ in 0..3 {
                frame(&mut p, &a, &b);
            }
            let downed = (SLOTS..SLOTS + 8)
                .filter(|&i| p.act[i].0 == Act::Down)
                .count();
            for _ in 0..120 {
                frame(&mut p, &a, &b);
            }
            let mut front = Vec::new();
            p.update(&b, 1, 0.0, |s| front.push(s.pos.y));
            front.truncate(8);
            (front, downed)
        };
        let ((held, none), (dented, downed)) = (line(0.0), line(1.0));
        let back: Vec<f32> = held.iter().zip(&dented).map(|(h, d)| h - d).collect();
        // Driven in where the charge hit, and not at the ends of the line.
        for k in [3, 4] {
            assert!(back[k] > 1.0, "file {k} gave {back:?}");
        }
        for k in [0, 7] {
            assert!(back[k].abs() < 0.3, "file {k} gave {back:?}");
        }
        // Men of a weight are knocked down only when the charge ploughs in.
        assert_eq!(none, 0);
        assert!(downed >= 2, "{downed}");
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
    fn a_general_stands_out_his_men_cheer_his_speech_and_flinch_when_he_falls() {
        let mut p = Puppets::new(2, 4);
        let mut a = block(0, Vec2::ZERO, Vec2::NEG_Y, false);
        a.general = Some(5);
        let b = block(1, Vec2::new(10.0, 0.0), Vec2::NEG_Y, false);
        let mut g = General {
            party: 0,
            regiment: a.handle,
            at: gc_fixed::FxVec2::ZERO,
            picked: 0,
            spoke: None,
            fell: None,
        };
        let tick = TICKS_PER_SECOND as f32 * FRAME;
        let frame = |p: &mut Puppets, a: &Block, g: &General, now: f32| {
            p.advance(FRAME);
            p.generals(core::iter::once(g), now);
            let mut out = Vec::new();
            p.update(a, 0, FRAME, |s| out.push(s));
            p.update(&b, 0, FRAME, |s| out.push(s));
            out
        };
        let out = frame(&mut p, &a, &g, 0.0);
        let generals: Vec<usize> = (0..out.len()).filter(|&k| out[k].general).collect();
        assert_eq!(generals, [5], "only he is drawn as the general");
        assert!(out.iter().all(|s| s.fight.is_none()));
        // He speaks: he raises his weapon at once, and most of the men of
        // both his regiments (they are near him) cheer within a few seconds.
        g.spoke = Some(20);
        let mut now = 20.0;
        let mut cheered = [false; 16];
        while now < 20.0 + 5.0 * TICKS_PER_SECOND as f32 {
            let out = frame(&mut p, &a, &g, now);
            if now < 20.0 + tick {
                assert_eq!(out[5].fight, Some(Move::Cheer));
            }
            for (k, s) in out.iter().enumerate() {
                cheered[k] |= s.fight == Some(Move::Cheer);
            }
            now += tick;
        }
        let count = cheered.iter().filter(|&&c| c).count();
        assert!((8..16).contains(&count), "{count} cheered");
        // He falls: a moment later every man near him flinches.
        g.fell = Some(400);
        a.general = None;
        let out = frame(&mut p, &a, &g, 400.0 + 0.5 * TICKS_PER_SECOND as f32);
        assert!(
            out.iter()
                .all(|s| matches!(s.fight, Some(Move::Stagger(_))) && !s.general)
        );
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
    fn a_champion_can_end_his_duel_with_a_matched_kill() {
        const KILL: KillTiming = KillTiming {
            seconds: 1.8,
            contact: 0.3,
            reach: 1.4,
        };
        let mut p = with_a_kill(KILL);
        let (a, mut b, c) = champions_called();
        let tick = TICKS_PER_SECOND as f32 * FRAME;
        let mut now = -tick;
        let step = |p: &mut Puppets, a: &Block, b: &Block, now: f32| {
            p.advance(FRAME);
            p.champions((now >= 0.0).then_some(&c), now);
            let mut out = Vec::new();
            p.update(a, 0, FRAME, |s| out.push(s));
            p.update(b, 1, FRAME, |s| out.push(s));
            out
        };
        let lead = KILL.contact * KILL.seconds * TICKS_PER_SECOND as f32;
        let ends = c.ends() as f32;
        while now < c.meet as f32 {
            step(&mut p, &a, &b, now);
            now += tick;
        }
        let [x, y] = p.duel.unwrap().champion.map(|i| i as usize % SLOTS);
        let mut last = Vec::new();
        while now < ends {
            let out = step(&mut p, &a, &b, now);
            let (w, l) = (out[x].fight, out[8 + y].fight);
            if now >= ends - lead + tick {
                // The winner winds up his half and the loser plays his.
                assert!(
                    matches!(w, Some(Move::Kill { pair: 0, .. })),
                    "{now}: {w:?}"
                );
                assert!(
                    matches!(l, Some(Move::Killed { pair: 0, .. })),
                    "{now}: {l:?}"
                );
            } else if now < ends - lead - tick {
                assert!(!matches!(w, Some(Move::Kill { .. })), "{now}: {w:?}");
            }
            last = vec![out[x], out[8 + y]];
            now += tick;
        }
        // He stepped back to the kill's reach as he wound up.
        let apart = last[0].pos.distance(last[1].pos);
        assert!((apart - KILL.reach).abs() < 0.02, "{apart}");
        // The loser falls as the blow lands, playing on the kill's half.
        b.models -= 1;
        let out = step(&mut p, &a, &b, now);
        let corpse = p.corpses().next().expect("the loser fell");
        assert_eq!(corpse.kill, Some(0));
        assert!(corpse.age.abs() < 2.0 * FRAME, "{corpse:?}");
        let x = p.duel.unwrap().champion[0] as usize % SLOTS;
        assert!(
            matches!(out[x].fight, Some(Move::Kill { .. })),
            "{:?}",
            out[x].fight
        );
        // He plays his half out, then cheers.
        let tail = (1.0 - KILL.contact) * KILL.seconds;
        for _ in 0..((tail / FRAME) as usize + 2) {
            now += tick;
            step(&mut p, &a, &b, now);
        }
        let out = step(&mut p, &a, &b, now + tick);
        assert_eq!(out[x].fight, Some(Move::Cheer));
        // And cheers where he stands, not walking back up to the body.
        let body = p.corpses().next().expect("the loser lies").pos;
        assert!((out[x].pos.distance(body) - KILL.reach).abs() < 0.05);
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
