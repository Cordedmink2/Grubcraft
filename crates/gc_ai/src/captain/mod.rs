//! Battle tactics: the Captain (docs/18-ai.md, "Captain: battle tactics").
//!
//! The Captain runs one army in one battle. Each time it looks it:
//! 1. **Frames the battle:** the axis from its army to the enemy army it
//!    faces, and where its line stands on it.
//! 2. **Weighs the fight** with the [battle estimator](crate::estimator).
//! 3. **Picks a doctrine** (a battle-plan template) by scoring each it knows
//!    against the situation and its personality, with a bonus for sticking
//!    to the one it has, so it doesn't dither.
//! 4. **Gives every regiment a role** in that doctrine and a place in the
//!    formation: the line (anchors and wings), batteries of archers behind
//!    it or a screen in front, hammers on the flanks, a reserve.
//! 5. **Picks its ground** while forming up or holding: an end of the line
//!    against a cliff or deep water, higher ground, a stand back from a
//!    river the enemy must cross (see [`crate::ground`]).
//! 6. **Chooses a posture:** form up, hold and receive, advance, or fall
//!    back. Expecting to lose, it waits for the enemy rather than marching
//!    into the fight; with water between the armies, it waits for them to
//!    cross.
//! 7. **Plays each role**, which turns into ordinary move and attack orders:
//!    line regiments counter-charge enemies that come close, ganging up to
//!    make local odds; hammers wait until the enemy line is locked in melee
//!    and then swing round to hit a flank or the rear; batteries shoot what
//!    helps most and fall back from danger; the reserve plugs gaps and
//!    catches flankers; wavering regiments are pulled back before they
//!    break; fast regiments chase routers, within a leash.
//!
//! The simulation's own reflexes (fighting back, pressing into a melee,
//! routing) run underneath for everyone; the Captain only gives orders.
//!
//! On a map with woods and tall grass the Captain sees only what its side
//! sees, and can lay an ambush ([`ambush`]).

mod ambush;
mod doctrine;
mod helpers;
mod hunt;
mod posture;
mod roles;
mod site;
mod team;

use helpers::*;
pub use helpers::{describe_margin, fits, gap_between};

use gc_fixed::Pcg32;
use gc_sim::{MoraleState, PlayerId, RegimentHandle, TICKS_PER_SECOND};
use glam::Vec2;
use std::cmp::Ordering;

use crate::difficulty::Knobs;
use crate::estimator::{self, Approach, Estimate, Force};
use crate::ground::{self, Crossing, Line};
use crate::journal::Journal;
use crate::orders::{Act, Want};
use crate::perception::{Class, Perception, Seen, Side, TICKS, right_of};
use crate::personality::Personality;
use crate::terrain::Terrain;

/// Battle-plan templates (docs/18-ai.md, "Doctrines"). The first five are
/// built; the rest of the table arrives with the factions and terrain that
/// need them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Doctrine {
    /// A continuous line, archers behind, fast troops on the wings, a reserve.
    HoldTheLine,
    /// The line pins the enemy front while the fast troops swing round to
    /// hit its flank and rear.
    HammerAndAnvil,
    /// One strong wing leads; the weak wing hangs back at an angle.
    RefusedFlank,
    /// Archers in front, falling back through the line as the enemy closes.
    SkirmishScreen,
    /// Hammer and anvil with the hammers hidden in woods or tall grass,
    /// striking once the enemy comes on (docs/20-ambushes.md).
    Ambush,
}

impl Doctrine {
    pub const ALL: [Doctrine; 5] = [
        Doctrine::HoldTheLine,
        Doctrine::HammerAndAnvil,
        Doctrine::RefusedFlank,
        Doctrine::SkirmishScreen,
        Doctrine::Ambush,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Doctrine::HoldTheLine => "Hold the line",
            Doctrine::HammerAndAnvil => "Hammer and anvil",
            Doctrine::RefusedFlank => "Refused flank",
            Doctrine::SkirmishScreen => "Skirmish screen",
            Doctrine::Ambush => "Ambush",
        }
    }
}

/// A regiment's job in the plan (docs/18-ai.md, "Roles").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// Holds the centre of the line; doesn't chase.
    Anchor,
    /// Holds an end of the line.
    Wing,
    /// Waits out of reach, then hits a flank or the rear once the enemy
    /// line is locked.
    Hammer,
    /// Shoots from in front of the line, falls back at contact.
    Screen,
    /// Shoots from behind the line.
    Battery,
    /// Waits behind the line to plug a gap, catch a flanker or finish a
    /// wavering enemy.
    Reserve,
    /// Chases routers, within a leash.
    Hunter,
    /// Pulled back to get its nerve back.
    Recover,
    /// Hides in cover and waits to spring an ambush; a hammer once sprung.
    Ambusher,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Anchor => "anchor",
            Role::Wing => "wing",
            Role::Hammer => "hammer",
            Role::Screen => "screen",
            Role::Battery => "battery",
            Role::Reserve => "reserve",
            Role::Hunter => "hunter",
            Role::Recover => "recovering",
            Role::Ambusher => "ambusher",
        }
    }
}

/// What the army as a whole is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Posture {
    /// Taking up the formation.
    Form,
    /// Standing to receive the enemy.
    Hold,
    /// Marching on the enemy in formation.
    Advance,
    /// Falling back to better odds.
    Withdraw,
}

impl Posture {
    pub fn name(self) -> &'static str {
        match self {
            Posture::Form => "forming up",
            Posture::Hold => "holding",
            Posture::Advance => "advancing",
            Posture::Withdraw => "falling back",
        }
    }
}

/// Seconds a side expecting to lose waits for an enemy that doesn't come
/// before going to fetch it.
const TIRED_OF_WAITING: f32 = 30.0;
/// Seconds an advancing line stands at the enemy with nobody coming to
/// blows before it gives up on the line and goes to fetch them.
const NOBODY_FIGHTS: f32 = 15.0;
/// Tiles between regiments sweeping for an enemy lost from sight, and how
/// near the place it vanished one of them must come to have searched it
/// (near enough to spot a regiment hiding there).
const SWEEP: f32 = 8.0;
const SEARCHED: f32 = 4.0;

/// Whether the army waits for the enemy or goes to meet it, when a plan
/// says (see [`Plan`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stance {
    /// Left to the Captain's own judgement, as without a plan.
    Own,
    /// March on the enemy as soon as formed (still never into water it is
    /// waiting behind, nor while its side's champion fights).
    Advance,
    /// Stand and receive them (but go to fetch an enemy that doesn't come).
    Hold,
}

impl Stance {
    pub const ALL: [Stance; 3] = [Stance::Own, Stance::Advance, Stance::Hold];

    pub fn name(self) -> &'static str {
        match self {
            Stance::Own => "as it sees fit",
            Stance::Advance => "advancing",
            Stance::Hold => "holding",
        }
    }
}

/// A battle plan chosen ahead (by rehearsing it, see [`crate::rehearsal`]):
/// the doctrine to fight with and whether to wait or go. The Captain follows
/// it until given another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Plan {
    pub doctrine: Doctrine,
    pub stance: Stance,
}

impl Plan {
    /// "Hammer and anvil, advancing".
    pub fn describe(&self) -> String {
        match self.stance {
            Stance::Own => self.doctrine.name().to_string(),
            s => format!("{}, {}", self.doctrine.name(), s.name()),
        }
    }
}

/// Space left between regiments side by side in the line, tiles.
const LINE_GAP: f32 = 1.5;
/// A regiment shouldn't meet an enemy where it would overlap a friend's
/// block by more than this, tiles: the simulation slows regiments pushing
/// into friends and throws both out of order (see [`Captain::room_at`]).
const ROOM: f32 = 0.5;
/// How much worse an enemy looks, to a level that gangs up, when there is
/// no room to meet it.
const NO_ROOM: f32 = 2.0;
/// Tiles between the fronts of two blocks meeting in melee.
const MEET_GAP: f32 = 0.25;
/// Archers stand this far behind the line's rear edge, and a screen this
/// far in front of its front edge.
const BATTERY_BACK: f32 = 3.0;
const SCREEN_AHEAD: f32 = 7.0;
/// Hammers wait this far beyond the ends of the line (Hammer and anvil),
/// or this far behind it (otherwise).
const HAMMER_OUT: f32 = 4.0;
const HAMMER_BACK: f32 = 3.0;
/// A reserve is kept back only from a line this long: taking one from a
/// shorter line weakens it more than the reserve helps (measured against
/// the charge-straight-in opponent).
const RESERVE_FROM: usize = 8;
/// The reserve stands this far behind the line.
const RESERVE_BACK: f32 = 7.0;
/// A wavering regiment falls back this far behind its place.
const RECOVER_BACK: f32 = 16.0;
/// When advancing, the line halts this far short of the enemy front and
/// the charges take it from there.
const STAND_OFF: f32 = 5.0;
/// Formation anchors that move less than this stay put (fewer orders).
const ANCHOR_SLACK: f32 = 6.0;
/// The frame turns to a new enemy direction past these angles (cosines):
/// readily while marching, reluctantly while standing in line.
const REFACE_ADVANCING: f32 = 0.978; // 12 degrees
const REFACE_HOLDING: f32 = 0.906; // 25 degrees
/// Looks between re-weighing the fight, and between re-thinking the plan.
const ESTIMATE_TICKS: u32 = 20;
const DOCTRINE_TICKS: u32 = 100;
/// Bonus for keeping the plan it has, so it doesn't flip-flop.
const COMMITMENT: f32 = 0.25;
/// A battery falls back when an enemy that isn't fighting gets this close
/// (tiles between blocks); a screen falls back sooner.
const BATTERY_DANGER: f32 = 8.0;
const SCREEN_DANGER: f32 = 14.0;
/// Where a hammer swings round to: beside the target and behind its front.
const STAGE_OUT: f32 = 6.0;
const STAGE_BACK: f32 = 5.0;
/// A line regiment doesn't chase further than this from its place.
const LINE_LEASH: f32 = 18.0;
/// A free line regiment joins a fight a friend is in this close, tiles
/// between blocks.
const HELP_TILES: f32 = 25.0;
/// The reserve weighs in on a fight a friend is losing this close, tiles.
const SUPPORT_TILES: f32 = 35.0;
/// A reserve filling a gap in the line is there within this many tiles of
/// the gap's place.
const GAP_THERE: f32 = 4.0;
/// The simulation counts soldiers within this many tiles when it judges
/// whether a regiment is outnumbered.
const LOCAL_TILES: f32 = 12.0;
/// A free line regiment chases a router this close, so it can't rally.
const PURSUE_TILES: f32 = 12.0;
/// The point of attack is picked from wavering enemies this close to the
/// line, and line regiments reach this much further for it.
const FOCUS_TILES: f32 = 40.0;
const FOCUS_REACH: f32 = 6.0;
/// A waiting hammer meets an enemy coming at it this close: charging back
/// if it can win, getting out of the way if not.
const COUNTER_TILES: f32 = 12.0;
/// Forming up ends when this share of the army is in place, or after this long.
const FORMED_SHARE: f32 = 0.75;
const FORM_TICKS: u32 = 15 * TICKS_PER_SECOND;
/// Shortest time between switching from holding to advancing or back.
const POSTURE_TICKS: u32 = 3 * TICKS_PER_SECOND;
/// Falling back: how far, and how long losing must last before it does.
const WITHDRAW_TILES: f32 = 25.0;
/// Only with the enemy front at least this far off, tiles.
const WITHDRAW_GAP: f32 = 35.0;
const WITHDRAW_AFTER_TICKS: u32 = 4 * TICKS_PER_SECOND;
/// Keeping out of two other sides' fight, it falls back only with the
/// enemy front at least this far off, tiles.
const SHY_GAP: f32 = 20.0;
/// The enemy is beaten once what still stands against us is this share of
/// our soldiers or less: the line breaks up to finish the rest wherever they
/// stand. It forms again if they gather past the second share.
const FINISH_SHARE: f32 = 0.35;
const REFORM_SHARE: f32 = 0.5;
/// Finishing off: an enemy regiment already set upon counts this many
/// tiles further away, so the hunters spread out.
const FINISH_SPREAD: f32 = 20.0;
/// Falling back at most this often in a battle: past that, it stands and
/// fights rather than retreat across the map from an enemy who waits.
const WITHDRAWALS: u32 = 2;
/// Expecting to lose by this much, it doesn't march on the enemy: it waits
/// on its ground for them to come.
const LOSING: f32 = -0.25;
/// Picking ground: how often it looks (ticks), and only with the enemy
/// front at least this far off (tiles).
const GROUND_TICKS: u32 = 3 * TICKS_PER_SECOND;
const GROUND_GAP: f32 = 20.0;
/// How far it looks for better ground: tiles to either side, back and
/// forward from where the line stands.
const GROUND_ACROSS: f32 = 30.0;
const GROUND_BACK: f32 = 16.0;
const GROUND_FORWARD: f32 = 4.0;
/// Tiles between the places it weighs on its first, rough look.
const GROUND_STEP: f32 = 3.0;
/// Moving the line further than this for the same reason as last time is
/// worth a new journal entry, tiles.
const GROUND_NEWS: f32 = 8.0;
/// Share of the formation's ground that must be standable and dry.
const FOOTING: f32 = 0.9;
/// What a covered end of the line is worth, before odds; and a tile of rise
/// over the ground in front (capped at three).
const COVER: f32 = 1.0;
const HIGH_GROUND: f32 = 0.15;
/// Moving costs a point for every this many tiles, and more for every tile
/// towards the enemy. A new place must beat the current one by this much.
const MOVE_TILES: f32 = 25.0;
const FORWARD_TILES: f32 = 6.0;
const STAY: f32 = 0.3;
/// Waiting behind water, the line stands this far back from it (further
/// if the enemy's archers on the far bank would reach it), facing the
/// nearest ford within this many tiles to either side.
const BANK_BACK: f32 = 8.0;
const FORD_REACH: f32 = 40.0;
/// Behind water, it crosses only when expecting to win by this much and
/// after twice its usual patience.
const CROSS_MARGIN: f32 = 0.3;
/// It draws back from the water to tempt the enemy across only expecting
/// a closer fight than this.
const BAIT_MARGIN: f32 = -0.15;
/// Longest it waits for two other sides to come to blows, ticks.
const ONLOOK_TICKS: u32 = 90 * TICKS_PER_SECOND;

/// One of the Captain's regiments.
#[derive(Clone, Copy, Debug)]
pub struct Member {
    pub handle: RegimentHandle,
    pub class: Class,
    pub role: Role,
    /// The role it returns to after hunting or recovering.
    pub home: Role,
    /// Its place in the formation: tiles to the right of the anchor, and
    /// tiles back from it (toward its own side).
    slot: Vec2,
    /// A screen's place behind the line, once it falls back.
    fallback: Vec2,
    /// The enemy it was last told to attack.
    pub target: Option<RegimentHandle>,
    /// A hammer that has committed to a strike.
    striking: bool,
    /// A screen that has fallen back behind the line.
    fell_back: bool,
    /// Where an archer is stepping back to, out of an enemy's reach.
    retreat_to: Option<Vec2>,
    /// Shattered: will never take orders again.
    lost: bool,
}

/// What a mind hands its Captain each look.
pub struct Ctx<'a> {
    pub view: &'a Perception,
    pub terrain: &'a Terrain,
    pub party: PlayerId,
    pub knobs: &'a Knobs,
    pub persona: &'a Personality,
    pub rng: &'a mut Pcg32,
    pub journal: &'a mut Journal,
    /// The earliest tick the next deliberate slip may happen.
    pub next_slip: &'a mut u32,
}

impl Ctx<'_> {
    /// Whether to make a deliberate slip at this chance to make one (easier
    /// levels slip more, and more often). Slips are spaced out, so a level
    /// slips about as often however often it looks.
    fn slip(&mut self) -> bool {
        let k = self.knobs;
        if k.slip_percent == 0 || self.view.tick < *self.next_slip {
            return false;
        }
        if self.rng.below(100) >= k.slip_percent {
            return false;
        }
        *self.next_slip = self.view.tick + k.slip_gap_ticks;
        true
    }
}

#[derive(Clone)]
pub struct Captain {
    pub doctrine: Doctrine,
    pub posture: Posture,
    /// The plan it was told to follow, if any; without one it picks its own
    /// doctrine and posture as it goes.
    pub plan: Option<Plan>,
    /// The enemy is beaten and the regiments are finishing off the rest.
    pub finishing: bool,
    /// Expecting to lose, it waited for an enemy that never came, and has
    /// gone to fetch it: every regiment after the nearest enemy.
    pub fetching: bool,
    pub estimate: Option<Estimate>,
    /// What it expected when it first weighed the fight up.
    pub first_estimate: Option<Estimate>,
    pub members: Vec<Member>,
    /// The battle frame: which way is the enemy, and where the line stands.
    forward: Vec2,
    anchor: Vec2,
    started: bool,
    formed_at: u32,
    next_estimate: u32,
    next_doctrine: u32,
    losing_since: Option<u32>,
    withdrawn_at: Option<u32>,
    withdrawals: u32,
    standoff_since: u32,
    /// Tick it began watching its side's champion fight, while it does.
    watching_since: Option<u32>,
    /// Since when it has been waiting, expecting to lose or holding, or
    /// stood at the enemy, with the enemy not coming and nobody fighting.
    waiting_since: Option<u32>,
    /// With nothing standing in sight: the enemy regiment it is looking
    /// for and where it vanished, and the places it has looked already.
    search: Option<(RegimentHandle, Vec2)>,
    searched: Vec<(RegimentHandle, Vec2)>,
    /// Tick the posture last changed.
    posture_since: u32,
    /// Where the enemy army was a while ago, to tell whether it is coming:
    /// (tick, its centre).
    distance_then: Option<(u32, Vec2)>,
    /// How fast the enemy army comes towards ours, tiles a second (our own
    /// advance doesn't count).
    closing: f32,
    /// Refused flank: +1 when the strong wing is the right, -1 the left.
    strong_side: f32,
    relayout: bool,
    /// Whether a plan has been picked yet (the first pick has nothing to
    /// stick to).
    doctrine_chosen: bool,
    /// Pace the line marches at when advancing (its slowest member's).
    pace: f32,
    /// Depth of the line, front to back, tiles.
    line_depth: f32,
    line_width: f32,
    /// Tiles behind the line's back edge that its archers need.
    rear: f32,
    /// Where the enemy army it faces stands (its centre).
    enemy_at: Vec2,
    /// The end of the line with cover beside it: -1 left, +1 right, 0
    /// neither. Its hammers wait on the open side.
    covered: f32,
    /// Water between the line and the enemy.
    river: Option<Crossing>,
    /// It has decided to cross the water after all.
    crossing: bool,
    /// Where it is crossing to: beyond a ford the enemy isn't watching, and
    /// where that ford comes out.
    bridgehead: Option<(Vec2, Vec2)>,
    /// The enemy watches every ford: it stands well back from the water,
    /// to tempt them across.
    baiting: bool,
    /// When it next looks along the bank for a ford to cross at.
    next_ford_look: u32,
    next_ground: u32,
    /// The journal's last word on the ground.
    ground_note: String,
    /// The point of attack: the enemy regiment whose rout would do most
    /// harm (wavering, beside others shaken). Line regiments in reach go
    /// for it first.
    pub focus: Option<RegimentHandle>,
    /// Where its ambushers hide, with the Ambush doctrine.
    ambush: ambush::Plan,
    /// Where friends stand in the fight, as of this look (see
    /// [`Captain::room_at`]).
    taken: Vec<Taken>,
    /// In a team game: the parties it is really fighting, and those
    /// fighting them with it (see [`team`]); empty when it weighs everyone.
    against: Vec<PlayerId>,
    with: Vec<PlayerId>,
    /// Two other sides about to fight each other, while it waits for them
    /// to come to blows (see [`team::Field::others_fighting`]).
    onlooking: Option<(PlayerId, PlayerId)>,
    /// When it began waiting on them.
    onlook_since: Option<u32>,
    /// They came to blows and it has gone in.
    pouncing: bool,
    /// One of them is coming at it before they have fought each other, or
    /// it stands nearer than the other: it keeps out of their way, falling
    /// back this way.
    shy: Option<Vec2>,
}

/// A friend's ground in the fight: its own block while it fights, or where
/// it will meet the enemy it is closing on, with how far from that enemy
/// it still is.
#[derive(Clone, Copy, Debug)]
struct Taken {
    handle: RegimentHandle,
    pos: Vec2,
    block: Seen,
    closing: Option<f32>,
}

impl Default for Captain {
    fn default() -> Self {
        Captain::new()
    }
}

impl Captain {
    pub fn new() -> Captain {
        Captain {
            doctrine: Doctrine::HoldTheLine,
            posture: Posture::Form,
            plan: None,
            finishing: false,
            fetching: false,
            estimate: None,
            first_estimate: None,
            members: Vec::new(),
            forward: Vec2::Y,
            anchor: Vec2::ZERO,
            started: false,
            formed_at: 0,
            next_estimate: 0,
            next_doctrine: 0,
            losing_since: None,
            withdrawn_at: None,
            withdrawals: 0,
            standoff_since: 0,
            watching_since: None,
            waiting_since: None,
            search: None,
            searched: Vec::new(),
            posture_since: 0,
            distance_then: None,
            closing: 0.0,
            strong_side: 1.0,
            relayout: true,
            doctrine_chosen: false,
            pace: 0.0,
            line_depth: 0.0,
            line_width: 0.0,
            rear: 0.0,
            enemy_at: Vec2::ZERO,
            covered: 0.0,
            river: None,
            crossing: false,
            bridgehead: None,
            baiting: false,
            next_ford_look: 0,
            next_ground: 0,
            ground_note: String::new(),
            focus: None,
            ambush: ambush::Plan::default(),
            taken: Vec::new(),
            against: Vec::new(),
            with: Vec::new(),
            onlooking: None,
            onlook_since: None,
            pouncing: false,
            shy: None,
        }
    }

    /// Follow `plan` from the next look: switch to its doctrine (if this
    /// army can fight that way) and wait or advance as it says.
    pub fn follow(&mut self, plan: Plan) {
        if self.plan != Some(plan) {
            self.plan = Some(plan);
            self.next_doctrine = 0;
        }
    }

    /// Whether the battle has started for it: it has seen the enemy and laid
    /// out its line.
    pub fn started(&self) -> bool {
        self.started
    }

    /// Where the line stands and which way it faces.
    pub fn frame(&self) -> (Vec2, Vec2) {
        (self.anchor, self.forward)
    }

    /// Where it wants its front, once the battle has started: the middle of
    /// its front edge, which way it faces, how wide it is, and the end
    /// anchored on ground nobody crosses (-1 left, +1 right, 0 neither).
    pub fn front(&self) -> Option<(Vec2, Vec2, f32, f32)> {
        self.started
            .then_some((self.anchor, self.forward, self.line_width, self.covered))
    }

    /// Whether it is waiting behind water for the enemy to cross.
    pub fn waiting_behind_water(&self) -> bool {
        self.river.is_some() && !self.crossing
    }

    /// Where regiment `h`'s place in the formation is on the map (before
    /// it is nudged onto standable ground).
    pub fn place_of(&self, h: RegimentHandle) -> Option<Vec2> {
        let m = self.members.iter().find(|m| m.handle == h)?;
        Some(self.anchor + right_of(self.forward) * m.slot.x - self.forward * m.slot.y)
    }

    /// Look at the battle and decide what every regiment should do.
    pub fn think(&mut self, ctx: &mut Ctx, wants: &mut Vec<Want>) {
        let view = ctx.view;
        self.sync_members(ctx);
        // Once anyone has come to blows it is too late to slip into hiding.
        self.ambush.joined |= view.regiments.iter().any(|r| r.engaged);
        let ours: Vec<&Seen> = self
            .members
            .iter()
            .filter_map(|m| view.get(m.handle))
            .filter(|r| !r.broken())
            .collect();
        if ours.is_empty() {
            return;
        }
        let foes: Vec<&Seen> = view
            .regiments
            .iter()
            .filter(|r| view.hostile(ctx.party, r.owner))
            .collect();
        let fighting: Vec<&Seen> = foes.iter().copied().filter(|r| !r.broken()).collect();
        if fighting.is_empty() {
            self.mop_up(ctx, &ours, &foes, wants);
            return;
        }
        self.search = None;
        self.searched.clear();
        self.read_sides(ctx);
        self.update_frame(ctx, &ours, &fighting);
        let now = view.tick;
        if now >= self.next_estimate {
            self.next_estimate = now + ESTIMATE_TICKS;
            self.weigh(ctx, &ours, &fighting);
        }
        if now >= self.next_doctrine {
            self.next_doctrine = now + DOCTRINE_TICKS;
            self.choose_doctrine(ctx, &ours, &fighting);
        }
        if self.relayout {
            self.layout(ctx);
        }
        self.choose_ground(ctx, &ours, &fighting);
        if self.relayout {
            self.layout(ctx);
        }
        if self.beaten(ctx, &ours, &fighting) || self.fetching {
            self.waiting_since = None;
            self.finish(ctx, &fighting, wants);
            return;
        }
        self.choose_posture(ctx, &ours, &fighting);
        self.hold_bridgehead(ctx, &ours, &fighting);
        self.act(ctx, &ours, &fighting, &foes, wants);
    }

    /// Whether the enemy is beaten: few left standing against many. A
    /// scattered remnant that has rallied would otherwise sit out of reach
    /// of the line for ever.
    fn beaten(&mut self, ctx: &mut Ctx, ours: &[&Seen], fighting: &[&Seen]) -> bool {
        let view = ctx.view;
        let mine = Force::of(view, ours.iter().copied()).soldiers;
        // Not while enemies it lost sight of may still be standing.
        let theirs = Force::of(view, fighting.iter().copied().chain(view.remembered())).soldiers;
        let share = if self.finishing {
            REFORM_SHARE
        } else {
            FINISH_SHARE
        };
        let beaten = theirs <= mine * share;
        if beaten != self.finishing {
            self.finishing = beaten;
            let note = if beaten {
                format!("Finishing them off: {} regiments left", fighting.len())
            } else {
                "They have rallied: forming up again".to_string()
            };
            ctx.journal.note(view.tick, note);
            if !beaten {
                self.relayout = true;
            }
        }
        beaten
    }

    /// Every regiment goes for the nearest enemy still standing, spreading
    /// out so each remnant gets its share.
    fn finish(&mut self, ctx: &mut Ctx, fighting: &[&Seen], wants: &mut Vec<Want>) {
        let view = ctx.view;
        let standing = |t: RegimentHandle| fighting.iter().any(|f| f.handle == t);
        let mut on: Vec<(RegimentHandle, u8)> = Vec::new();
        for m in &self.members {
            if let Some(t) = m.target.filter(|&t| standing(t)) {
                bump(&mut on, t);
            }
        }
        for k in 0..self.members.len() {
            let mut m = self.members[k];
            let Some(r) = view.get(m.handle) else {
                continue;
            };
            if m.lost || r.broken() || r.engaged {
                continue;
            }
            // Someone charging at us comes first, or they take us in the
            // flank on the way to the last stragglers.
            let threat = self.threat(ctx, r, fighting).map(|f| f.handle);
            let target = match threat.or(m.target.filter(|&t| standing(t))) {
                Some(t) => t,
                None => {
                    let score = |f: &Seen| {
                        f.pos.distance(r.pos) + FINISH_SPREAD * f32::from(count(&on, f.handle))
                    };
                    let Some(t) = fighting
                        .iter()
                        .min_by(|a, b| score(a).total_cmp(&score(b)))
                        .map(|f| f.handle)
                    else {
                        continue;
                    };
                    bump(&mut on, t);
                    t
                }
            };
            m.target = Some(target);
            m.striking = false;
            wants.push(attack(m.handle, target, 2));
            self.members[k] = m;
        }
    }

    /// Follow regiments joining (placed in the sandbox) and leaving.
    fn sync_members(&mut self, ctx: &Ctx) {
        let view = ctx.view;
        let before = self.members.len();
        self.members.retain(|m| view.get(m.handle).is_some());
        if self.members.len() != before {
            self.relayout = true;
        }
        for r in view.regiments.iter().filter(|r| r.owner == ctx.party) {
            if self.members.iter().any(|m| m.handle == r.handle) {
                continue;
            }
            let class = view.unit(r).class;
            let role = match class {
                Class::Line => Role::Anchor,
                Class::Shock => Role::Hammer,
                Class::Ranged => Role::Battery,
            };
            self.members.push(Member {
                handle: r.handle,
                class,
                role,
                home: role,
                slot: Vec2::ZERO,
                fallback: Vec2::ZERO,
                target: None,
                striking: false,
                fell_back: false,
                retreat_to: None,
                lost: false,
            });
            self.relayout = true;
        }
        for m in &mut self.members {
            if !m.lost
                && view
                    .get(m.handle)
                    .is_some_and(|r| r.morale == MoraleState::Shattered)
            {
                m.lost = true;
                self.relayout = true;
            }
        }
    }

    fn update_frame(&mut self, ctx: &mut Ctx, ours: &[&Seen], fighting: &[&Seen]) {
        let our_c = centroid(ours);
        let their_c = nearest_army(ctx.party, our_c, fighting);
        self.enemy_at = their_c;
        let dir = their_c - our_c;
        let distance = dir.length();
        if !self.started {
            self.started = true;
            self.forward = dir.normalize_or(self.forward);
            self.anchor = ctx.terrain.clamp(our_c, 4.0);
            self.formed_at = ctx.view.tick;
            self.standoff_since = ctx.view.tick;
            self.relayout = true;
            return;
        }
        if distance > 0.5 {
            let want = dir / distance;
            let limit = if self.posture == Posture::Advance {
                REFACE_ADVANCING
            } else {
                REFACE_HOLDING
            };
            if want.dot(self.forward) < limit {
                self.forward = want;
                // Stand where the army is now, facing the new way.
                self.anchor = ctx
                    .terrain
                    .clamp(line_centre(&self.members, ours, our_c), 4.0);
                self.relayout = true;
            }
        }
        // Is the enemy coming? See how far it moved our way in the last
        // two seconds.
        let now = ctx.view.tick;
        match self.distance_then {
            Some((then, was)) if now >= then + 2 * TICKS_PER_SECOND => {
                let towards_us = -dir.normalize_or_zero();
                self.closing = (their_c - was).dot(towards_us) / ((now - then) as f32 / TICKS);
                self.distance_then = Some((now, their_c));
            }
            None => self.distance_then = Some((now, their_c)),
            _ => {}
        }
    }

    fn weigh(&mut self, ctx: &mut Ctx, ours: &[&Seen], fighting: &[&Seen]) {
        let view = ctx.view;
        // Allies fight the same enemy: without them every side of a 2v2
        // expects to lose to both enemies at once, and waits.
        // In a team game, only the enemies turned on its side count, and the
        // friends turned on them (see [`team`]).
        let allies = view.regiments.iter().filter(|r| {
            r.owner != ctx.party
                && !r.broken()
                && !view.hostile(ctx.party, r.owner)
                && self.counts_with(r.owner)
        });
        let mine = Force::of(view, ours.iter().copied().chain(allies));
        let theirs = Force::of(
            view,
            fighting
                .iter()
                .copied()
                .filter(|f| self.counts_against(f.owner)),
        );
        // Whoever stands higher where the lines will meet fights downhill.
        let (mine, theirs) = match self.slope(ctx, fighting) {
            Ordering::Greater => (mine.downhill(), theirs),
            Ordering::Less => (mine, theirs.downhill()),
            Ordering::Equal => (mine, theirs),
        };
        let gap = self.front_gap(fighting);
        let closing = (mine.speed + theirs.speed) * 0.5;
        let est = estimator::estimate(&mine, &theirs, Approach { gap, closing });
        self.estimate = Some(est);
        self.first_estimate.get_or_insert(est);
    }

    /// Tiles between the front of the formation and the nearest enemy front.
    /// Tiles from the line's place (the anchor) to the nearest enemy front.
    fn front_gap(&self, fighting: &[&Seen]) -> f32 {
        self.front_gap_from(self.anchor, fighting)
    }

    /// Tiles from a line standing at `from` to the nearest enemy front. In
    /// a three-sided fight, only the enemies it is really fighting count,
    /// and one behind the line is as far as it is.
    fn front_gap_from(&self, from: Vec2, fighting: &[&Seen]) -> f32 {
        let gap = fighting
            .iter()
            .filter(|f| self.counts_against(f.owner))
            .map(|f| {
                let ahead = (f.pos - from).dot(self.forward);
                if ahead < 0.0 && !self.against.is_empty() {
                    f.pos.distance(from) - f.radius()
                } else {
                    ahead - f.depth * 0.5
                }
            })
            .fold(f32::MAX, f32::min)
            - self.line_depth * 0.5;
        gap.max(0.0)
    }
}
