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

use gc_fixed::Pcg32;
use gc_sim::{MoraleState, PlayerId, RegimentHandle};
use glam::Vec2;

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
const FORM_TICKS: u32 = 15 * 20;
/// Shortest time between switching from holding to advancing or back.
const POSTURE_TICKS: u32 = 3 * 20;
/// Falling back: how far, and how long losing must last before it does.
const WITHDRAW_TILES: f32 = 25.0;
/// Only with the enemy front at least this far off, tiles.
const WITHDRAW_GAP: f32 = 35.0;
const WITHDRAW_AFTER_TICKS: u32 = 4 * 20;
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
const GROUND_TICKS: u32 = 3 * 20;
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
    /// Since when it has been waiting, expecting to lose, with the enemy
    /// not coming and nobody fighting.
    waiting_since: Option<u32>,
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
            next_ground: 0,
            ground_note: String::new(),
            focus: None,
            ambush: ambush::Plan::default(),
            taken: Vec::new(),
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
            Some((then, was)) if now >= then + 2 * 20 => {
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
        let allies = view
            .regiments
            .iter()
            .filter(|r| r.owner != ctx.party && !r.broken() && !view.hostile(ctx.party, r.owner));
        let mine = Force::of(view, ours.iter().copied().chain(allies));
        let theirs = Force::of(view, fighting.iter().copied());
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

    /// Tiles from a line standing at `from` to the nearest enemy front.
    fn front_gap_from(&self, from: Vec2, fighting: &[&Seen]) -> f32 {
        let gap = fighting
            .iter()
            .map(|f| (f.pos - from).dot(self.forward) - f.depth * 0.5)
            .fold(f32::MAX, f32::min)
            - self.line_depth * 0.5;
        gap.max(0.0)
    }

    fn choose_doctrine(&mut self, ctx: &mut Ctx, ours: &[&Seen], fighting: &[&Seen]) {
        let view = ctx.view;
        let count = |class: Class| ours.iter().filter(|r| view.unit(r).class == class).count();
        let (line, shock, ranged) = (
            count(Class::Line),
            count(Class::Shock),
            count(Class::Ranged),
        );
        let their_shock = fighting
            .iter()
            .filter(|r| view.unit(r).class == Class::Shock)
            .count();
        let mine = Force::of(view, ours.iter().copied());
        let theirs = Force::of(view, fighting.iter().copied());
        let margin = self.estimate.map_or(0.0, |e| e.margin);
        let p = ctx.persona;
        let hammers = (shock > 0 && line + ranged > 0).then(|| {
            0.4 + 0.4 * (shock as f32 / 3.0).min(1.0)
                + if shock > their_shock { 0.2 } else { 0.0 }
                + 0.3 * p.cunning()
        });
        let mut best: Option<(f32, Doctrine)> = None;
        for &d in ctx.knobs.doctrines {
            // An ambush has its own test, which keeps one already laid going
            // whatever the army has lost since.
            if d != Doctrine::Ambush && !fits(d, line, shock, ranged) {
                continue;
            }
            let fit = match d {
                Doctrine::HoldTheLine => {
                    0.5 + if mine.ranged > theirs.ranged {
                        0.3
                    } else {
                        0.0
                    } + if margin < 0.0 { 0.2 } else { 0.0 }
                        + 0.2 * p.caution()
                }
                Doctrine::HammerAndAnvil => match hammers {
                    Some(fit) => fit,
                    None => continue,
                },
                Doctrine::RefusedFlank => {
                    0.3 + if margin < -0.05 { 0.4 } else { 0.0 } + 0.3 * p.cunning()
                }
                Doctrine::SkirmishScreen => {
                    0.3 + if mine.ranged > theirs.ranged * 1.3 {
                        0.5
                    } else {
                        0.0
                    }
                }
                // Hammer and anvil with the hammers hidden (or, with no fast
                // troops, the ends of the line): scores on top of it when
                // there is somewhere to hide.
                Doctrine::Ambush => match self.ambush_fit(ctx, ours, fighting, (line, shock)) {
                    Some(extra) => hammers.unwrap_or(0.4 + 0.3 * p.cunning()) + extra,
                    None => continue,
                },
            };
            let jitter = ctx.rng.below(1000) as f32 / 1000.0 * 0.15;
            let stick = if !self.doctrine_chosen || d != self.doctrine {
                0.0
            } else {
                COMMITMENT
            };
            // A plan's doctrine wins, when this army can fight that way.
            let planned = self.plan.is_some_and(|plan| plan.doctrine == d);
            let score = if planned {
                f32::INFINITY
            } else {
                fit + 0.6 * p.doctrines.of(d) + jitter + stick
            };
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, d));
            }
        }
        let Some((_, pick)) = best else {
            return;
        };
        if pick != self.doctrine || !self.doctrine_chosen {
            self.doctrine_chosen = true;
            let why = match self.estimate {
                Some(e) => format!("expects {}", describe_margin(e.margin)),
                None => String::new(),
            };
            ctx.journal
                .note(view.tick, format!("{}: {why}", pick.name()));
            self.doctrine = pick;
            if pick == Doctrine::RefusedFlank {
                self.strong_side = self.weaker_enemy_side(fighting);
            }
            self.relayout = true;
        }
    }

    /// The side (+1 right, -1 left) where the enemy has fewer soldiers.
    fn weaker_enemy_side(&self, fighting: &[&Seen]) -> f32 {
        let right = right_of(self.forward);
        let lean: f32 = fighting
            .iter()
            .map(|f| (f.pos - self.anchor).dot(right).signum() * f32::from(f.models))
            .sum();
        if lean > 0.0 { -1.0 } else { 1.0 }
    }

    /// Give every regiment a role and a place in the formation.
    fn layout(&mut self, ctx: &mut Ctx) {
        self.relayout = false;
        let view = ctx.view;
        let right = right_of(self.forward);
        // Each member's size and where it stands across the frame now.
        let n = self.members.len();
        let mut width = vec![4.0; n];
        let mut depth = vec![2.0; n];
        let mut lateral = vec![0.0; n];
        for (k, m) in self.members.iter().enumerate() {
            if let Some(r) = view.get(m.handle) {
                width[k] = r.width;
                depth[k] = r.depth;
                lateral[k] = (r.pos - self.anchor).dot(right);
            }
        }
        let by_lateral = |a: &usize, b: &usize| lateral[*a].total_cmp(&lateral[*b]);
        let by_middle = |a: &usize, b: &usize| lateral[*a].abs().total_cmp(&lateral[*b].abs());
        let mut line: Vec<usize> = Vec::new();
        let mut shock: Vec<usize> = Vec::new();
        let mut ranged: Vec<usize> = Vec::new();
        for (k, m) in self.members.iter().enumerate() {
            if m.lost {
                continue;
            }
            match m.class {
                Class::Line => line.push(k),
                Class::Shock => shock.push(k),
                Class::Ranged => ranged.push(k),
            }
        }
        // An army of fast troops only (the Brood's Skitterlings) still needs
        // a line: the outermost third are hammers, the rest stand in it.
        if line.is_empty() && !shock.is_empty() {
            let keep = if shock.len() >= 3 { shock.len() / 3 } else { 0 };
            shock.sort_by(by_middle);
            line = shock.split_off(0);
            shock = line.split_off(line.len() - keep);
        }
        // The Ambush doctrine with no fast troops hides line regiments
        // instead: those already sent into hiding, then the outermost.
        if self.doctrine == Doctrine::Ambush && shock.is_empty() {
            let hiding = |k: usize| self.ambush.has(self.members[k].handle);
            line.sort_by(|&a, &b| hiding(b).cmp(&hiding(a)).then(by_middle(&b, &a)));
            let n = ambush::line_ambushers(line.len());
            shock = line.drain(..n).collect();
        }
        // The reserve: a line regiment from near the middle, if there are
        // enough to spare one.
        let mut reserve: Option<usize> = None;
        if ctx.knobs.reserve
            && line.len() >= RESERVE_FROM
            && self.doctrine != Doctrine::SkirmishScreen
        {
            let existing = line
                .iter()
                .copied()
                .find(|&k| self.members[k].home == Role::Reserve);
            let pick = existing.or_else(|| line.iter().copied().min_by(by_middle));
            if let Some(k) = pick {
                line.retain(|&x| x != k);
                reserve = Some(k);
            }
        }

        // The line, left to right as they stand now so nobody crosses.
        line.sort_by(by_lateral);
        let total: f32 = line.iter().map(|&k| width[k]).sum::<f32>()
            + LINE_GAP * line.len().saturating_sub(1) as f32;
        let line_depth = line.iter().map(|&k| depth[k]).fold(0.0, f32::max);
        self.line_depth = line_depth;
        self.line_width = total;
        let refused = self.doctrine == Doctrine::RefusedFlank;
        // A line with one end covered leads with that end, and refuses the
        // open one.
        let strong_side = if self.covered != 0.0 {
            self.covered
        } else {
            self.strong_side
        };
        let mut x = -total * 0.5;
        for (i, &k) in line.iter().enumerate() {
            let cx = x + width[k] * 0.5;
            x += width[k] + LINE_GAP;
            // Refused flank: the weak wing steps back at an angle.
            let back = if refused && cx * strong_side < 0.0 {
                cx.abs() * 0.55
            } else {
                0.0
            };
            let edge = i == 0 || i + 1 == line.len();
            let role = if edge && line.len() >= 3 {
                Role::Wing
            } else {
                Role::Anchor
            };
            self.assign(k, role, Vec2::new(cx, back));
        }

        // Archers: spread behind the line, or in front of it as a screen.
        ranged.sort_by(by_lateral);
        let rw: f32 = ranged.iter().map(|&k| width[k]).sum::<f32>()
            + 2.0 * LINE_GAP * ranged.len().saturating_sub(1) as f32;
        let spread = (total * 0.8).max(rw);
        let (first, step) = match (ranged.first(), ranged.last()) {
            (Some(&a), Some(&b)) if ranged.len() > 1 => {
                let first = -spread * 0.5 + width[a] * 0.5;
                let last = spread * 0.5 - width[b] * 0.5;
                (first, (last - first) / (ranged.len() - 1) as f32)
            }
            _ => (0.0, 0.0),
        };
        let screen = self.doctrine == Doctrine::SkirmishScreen;
        self.rear = ranged
            .iter()
            .map(|&k| BATTERY_BACK + depth[k])
            .fold(0.0, f32::max);
        // How far behind the line the archers standing behind it reach (not
        // a screen out in front).
        let mut archers_back: f32 = 0.0;
        for (i, &k) in ranged.iter().enumerate() {
            let cx = first + step * i as f32;
            let behind = Vec2::new(cx, line_depth * 0.5 + BATTERY_BACK + depth[k] * 0.5);
            let ahead = Vec2::new(cx, -(line_depth * 0.5 + SCREEN_AHEAD + depth[k] * 0.5));
            let m = &mut self.members[k];
            m.fallback = behind;
            if !screen {
                m.fell_back = false;
            }
            // A screen that already fell back stays back.
            if screen && !m.fell_back {
                self.assign(k, Role::Screen, ahead);
            } else {
                self.assign(k, Role::Battery, behind);
                archers_back = archers_back.max(BATTERY_BACK + depth[k]);
            }
        }

        // Hammers wait behind the ends of the line, where the line shields
        // them, ready to swing round a flank. Hammer and anvil holds them
        // further out, already round the ends. A refused flank puts them all
        // behind the strong wing, and a line with a covered end puts them
        // all on the open side (nobody gets round the covered one, either
        // way).
        shock.sort_by(by_lateral);
        let half = shock.len().div_ceil(2);
        let anvil = self.doctrine == Doctrine::HammerAndAnvil;
        let (mut left_x, mut right_x) = if anvil {
            (-total * 0.5 - HAMMER_OUT, total * 0.5 + HAMMER_OUT)
        } else {
            (-total * 0.5, total * 0.5)
        };
        for (i, &k) in shock.iter().enumerate() {
            let w = width[k];
            let to_right = if self.covered != 0.0 {
                self.covered < 0.0
            } else if refused {
                strong_side > 0.0
            } else {
                i >= shock.len() - half
            };
            // Outward from the line's end with Hammer and anvil, inward from
            // it (behind the wing) otherwise.
            let cx = match (to_right, anvil) {
                (true, true) => {
                    right_x += w + LINE_GAP;
                    right_x - LINE_GAP - w * 0.5
                }
                (false, true) => {
                    left_x -= w + LINE_GAP;
                    left_x + LINE_GAP + w * 0.5
                }
                (true, false) => {
                    right_x -= w + LINE_GAP;
                    right_x + LINE_GAP + w * 0.5
                }
                (false, false) => {
                    left_x += w + LINE_GAP;
                    left_x - LINE_GAP - w * 0.5
                }
            };
            // Behind the archers, not among them.
            let back = if anvil {
                line_depth * 0.5
            } else {
                line_depth * 0.5 + HAMMER_BACK.max(archers_back + LINE_GAP) + depth[k] * 0.5
            };
            self.assign(k, Role::Hammer, Vec2::new(cx, back));
        }
        if let Some(k) = reserve {
            let slot = Vec2::new(0.0, line_depth + RESERVE_BACK + depth[k] * 0.5);
            self.assign(k, Role::Reserve, slot);
        }
        self.lay_ambush(ctx);

        // The formation marches at the pace of its slowest foot regiment.
        let slowest = line
            .iter()
            .chain(&ranged)
            .chain(reserve.iter())
            .filter_map(|&k| view.get(self.members[k].handle))
            .map(|r| view.pace(r))
            .fold(f32::MAX, f32::min);
        self.pace = if slowest == f32::MAX { 0.0 } else { slowest };
    }

    /// Give member `k` a role and slot, keeping a temporary role (hunting,
    /// recovering) until it ends.
    fn assign(&mut self, k: usize, role: Role, slot: Vec2) {
        let m = &mut self.members[k];
        m.home = role;
        m.slot = slot;
        if !matches!(m.role, Role::Hunter | Role::Recover) {
            if m.role != role {
                m.target = None;
                m.striking = false;
            }
            m.role = role;
        }
    }

    /// Where a slot is on the map, nudged onto ground a regiment can stand on.
    fn place(&self, ctx: &Ctx, slot: Vec2) -> Vec2 {
        let right = right_of(self.forward);
        let p = ctx
            .terrain
            .clamp(self.anchor + right * slot.x - self.forward * slot.y, 3.0);
        if ctx.terrain.walkable(p) {
            p
        } else {
            ctx.terrain.nearest_walkable(p, 8.0).unwrap_or(p)
        }
    }

    /// While forming up or holding, every few seconds: is there better ground
    /// close by to receive the enemy on? It scores the places the line could
    /// stand within reach by cover at its ends, rise over the ground in
    /// front and, with water between the armies, keeping back from the
    /// bank, against the distance to get there; and moves the line if one
    /// is clearly better (docs/18-ai.md, "Using ground").
    fn choose_ground(&mut self, ctx: &mut Ctx, ours: &[&Seen], fighting: &[&Seen]) {
        let view = ctx.view;
        let now = view.tick;
        let t = ctx.terrain;
        if !ctx.knobs.ground || now < self.next_ground || self.line_width <= 0.0 {
            return;
        }
        // On a beat of its own, so several AIs don't all look in one tick.
        let phase = u32::from(ctx.party) * 13 % GROUND_TICKS;
        self.next_ground = (now / GROUND_TICKS + 1) * GROUND_TICKS + phase;
        let river = ground::crossing(t, self.anchor, self.enemy_at);
        if river.is_some() != self.river.is_some() && !self.crossing {
            let note = if river.is_some() {
                "Water between us: waiting for them to cross"
            } else {
                "No water between us now"
            };
            ctx.journal.note(now, note.to_string());
        }
        self.river = river;
        if !matches!(self.posture, Posture::Form | Posture::Hold)
            || self.front_gap(fighting) < GROUND_GAP
        {
            return;
        }
        let mine = Force::of(view, ours.iter().copied());
        let theirs = Force::of(view, fighting.iter().copied());
        let margin = self.estimate.map_or(0.0, |e| e.margin);
        // Cover is worth more the worse the odds.
        let cover_worth = COVER * (1.0 + 2.0 * (-margin).max(0.0));
        // Behind water, out of reach of archers on the far bank if they
        // shoot better.
        let bank_back = match river {
            Some(r) if theirs.ranged > mine.ranged * 1.15 => {
                BANK_BACK.max(theirs.range - (r.far - r.near) + 3.0)
            }
            _ => BANK_BACK,
        };
        let (anchor, forward, enemy_at) = (self.anchor, self.forward, self.enemy_at);
        let (width, depth, rear) = (self.line_width, self.line_depth, self.rear);
        // Where they would come out of the water: wait just behind it, to
        // fall on each regiment as it wades out.
        let wait_at = river
            .and_then(|r| ground::ford(t, anchor, forward, r, FORD_REACH))
            .map(|exit| exit - forward * (bank_back + depth * 0.5));
        let score = |p: Vec2| -> Option<(f32, f32, f32)> {
            let line = Line {
                centre: p,
                forward,
                width,
                depth,
            };
            if line.footing(t, rear) < FOOTING {
                return None;
            }
            let mut s = 0.0;
            let mut side = 0.0;
            for (cover, sign) in line.cover(t).into_iter().zip([-1.0, 1.0]) {
                if let Some(k) = cover {
                    let snug = if k <= 2.0 {
                        1.0
                    } else if k <= 4.0 {
                        0.5
                    } else {
                        0.25
                    };
                    s += cover_worth * snug;
                    side += sign;
                }
            }
            let rise = line.rise(t, 12.0).clamp(0.0, 3.0);
            s += HIGH_GROUND * rise;
            if river.is_some()
                && let Some(c) = ground::crossing(t, p, enemy_at)
                && c.near < bank_back
            {
                s -= (bank_back - c.near) / 4.0;
            }
            let moved = p - anchor;
            s -= moved.length() / MOVE_TILES;
            match wait_at {
                // Up to the ford is safe ground: they must cross to reach it.
                Some(w) => s -= p.distance(w) / 5.0,
                None => s -= moved.dot(forward).max(0.0) / FORWARD_TILES,
            }
            Some((s, side, rise))
        };
        let right = right_of(forward);
        let mut best = score(anchor).map(|(s, side, rise)| (s + STAY, anchor, side, rise));
        let mut look = |centre: Vec2, wide: f32, ahead: f32, behind: f32, step: f32| {
            let mut back = -ahead;
            while back <= behind {
                let mut across = -wide;
                while across <= wide {
                    let p = centre + right * across - forward * back;
                    if t.inside(p, 6.0)
                        && let Some((s, side, rise)) = score(p)
                        && best.is_none_or(|b| s > b.0)
                    {
                        best = Some((s, p, side, rise));
                    }
                    across += step;
                }
                back += step;
            }
            best
        };
        // A rough look around where the line stands and around the ford,
        // then a closer one around the best place found.
        let mut found = look(
            anchor,
            GROUND_ACROSS,
            GROUND_FORWARD,
            GROUND_BACK,
            GROUND_STEP,
        );
        if let Some(w) = wait_at {
            found = look(w, 8.0, 6.0, 6.0, GROUND_STEP);
        }
        let near = GROUND_STEP - 1.0;
        if let Some((_, p, _, _)) = found
            && p != anchor
        {
            found = look(p, near, near, near, 1.0);
        }
        let Some((_, to, side, rise)) = found else {
            return;
        };
        if to != anchor {
            self.anchor = to;
            let what = if side != 0.0 {
                format!(
                    "anchoring the {} on ground nobody crosses",
                    if side > 0.0 { "right" } else { "left" }
                )
            } else if wait_at.is_some() {
                "facing the ford".to_string()
            } else if river.is_some() {
                "standing back from the water".to_string()
            } else if rise >= 0.5 {
                "the high ground".to_string()
            } else {
                "a better place".to_string()
            };
            let note = format!("Taking ground: {what}");
            // A small step to better ground for the same reason isn't news.
            if note != self.ground_note || to.distance(anchor) > GROUND_NEWS {
                ctx.journal.note(now, note.clone());
                self.ground_note = note;
            }
        }
        if side != self.covered {
            self.covered = side;
            self.relayout = true;
        }
    }

    /// Whether `f` stands beyond water the line is waiting behind, from
    /// `r`'s side of it: not to be attacked across the water.
    fn beyond_water(&self, terrain: &Terrain, r: &Seen, f: &Seen) -> bool {
        self.river.is_some()
            && !self.crossing
            && !terrain.wet(f.pos)
            && ground::crossing(terrain, r.pos, f.pos).is_some()
    }

    fn choose_posture(&mut self, ctx: &mut Ctx, ours: &[&Seen], fighting: &[&Seen]) {
        let view = ctx.view;
        let now = view.tick;
        let p = ctx.persona;
        let before = self.posture;
        // Only kept while it goes on waiting, below.
        let was_waiting = self.waiting_since.take();
        // How far the enemy is from where the line stands now (the anchor
        // is where it is going, which while advancing is near the enemy).
        let line_now = line_centre(&self.members, ours, centroid(ours));
        let gap = self.front_gap_from(line_now, fighting);
        let mine = Force::of(view, ours.iter().copied());
        let theirs = Force::of(view, fighting.iter().copied());
        let margin = self.estimate.map_or(0.0, |e| e.margin);

        if self.posture == Posture::Form {
            let placed = self
                .members
                .iter()
                .filter(|m| !m.lost)
                .filter_map(|m| view.get(m.handle).map(|r| (m, r)))
                .filter(|(m, r)| r.engaged || r.pos.distance(self.place(ctx, m.slot)) < 4.0)
                .count();
            let total = self.members.iter().filter(|m| !m.lost).count().max(1);
            let formed = placed as f32 / total as f32 >= FORMED_SHARE;
            if formed || now >= self.formed_at + FORM_TICKS || gap < 25.0 {
                self.posture = Posture::Hold;
                self.standoff_since = now;
            }
        }

        // Losing badly while the enemy is still well off and not coming on:
        // fall back to better odds rather than fight on. Never with the
        // enemy close, where turning away only invites a charge in the back.
        let threshold = -(0.15 + 0.35 * p.caution());
        let well_off = gap > WITHDRAW_GAP && self.closing < 1.0;
        if margin < threshold && well_off {
            let since = *self.losing_since.get_or_insert(now);
            let recently = self.withdrawn_at.is_some_and(|t| now < t + 30 * 20);
            if now >= since + WITHDRAW_AFTER_TICKS
                && !recently
                && self.withdrawals < WITHDRAWALS
                && self.posture != Posture::Withdraw
            {
                self.posture = Posture::Withdraw;
                self.withdrawn_at = Some(now);
                self.withdrawals += 1;
                let back = ctx
                    .terrain
                    .clamp(self.anchor - self.forward * WITHDRAW_TILES, 8.0);
                self.anchor = back;
                ctx.journal.note(
                    now,
                    format!("Falling back: expects {}", describe_margin(margin)),
                );
            }
        } else {
            self.losing_since = None;
        }
        if self.posture == Posture::Withdraw {
            let home = ours
                .iter()
                .filter(|r| r.pos.distance(self.anchor) < self.line_width * 0.5 + 10.0)
                .count();
            let done =
                home * 3 >= ours.len() * 2 || now >= self.withdrawn_at.unwrap_or(now) + 20 * 20;
            if done || gap < WITHDRAW_GAP * 0.5 {
                self.posture = Posture::Hold;
                self.standoff_since = now;
            }
        }

        if matches!(self.posture, Posture::Hold | Posture::Advance) {
            let our_range = self.battery_reach(ctx);
            let only_archers = theirs.melee_regiments == 0.0;
            // They are coming: receive them where we stand, unless keen to
            // meet them in the open.
            let coming = self.closing > 1.0 && gap < 60.0;
            let meet = p.aggression() >= 0.7 && gap > 45.0 && !self.lying_in_wait();
            // Outshot only matters once their archers can reach us.
            let under_fire = theirs.ranged > 0.0 && gap < theirs.range + 2.0;
            let outshot = theirs.ranged > mine.ranged * 1.15 && under_fire;
            let out_of_range = mine.ranged > 0.0 && gap > our_range;
            let losing = margin < LOSING;
            let patience = 3.0 + 25.0 * p.patience() - 10.0 * p.aggression()
                + if margin > 0.25 { -5.0 } else { 0.0 }
                + if self.lying_in_wait() {
                    ambush::WAIT_LONGER
                } else {
                    0.0
                };
            // Its side's champion is fighting between the lines: stand and
            // watch. The time spent watching doesn't use up its patience.
            let watching = view
                .champions
                .is_some_and(|sides| sides.iter().any(|&s| !view.hostile(ctx.party, s)));
            match (watching, self.watching_since) {
                (true, None) => self.watching_since = Some(now),
                (false, Some(since)) => {
                    self.standoff_since = (self.standoff_since + (now - since)).min(now);
                    self.watching_since = None;
                }
                _ => {}
            }
            let waited = (now - self.standoff_since) as f32 / TICKS;
            // Water between the armies: whoever crosses comes out of it a
            // regiment at a time, so wait for them, unless clearly the
            // stronger and tired of waiting.
            if self.river.is_some()
                && !self.crossing
                && margin > CROSS_MARGIN
                && waited > 2.0 * patience.max(5.0)
            {
                self.crossing = true;
                ctx.journal.note(
                    now,
                    format!("Crossing the water: expects {}", describe_margin(margin)),
                );
            }
            let behind_water = self.river.is_some() && !self.crossing;
            // Behind water it waits for them; while its champion fights it
            // watches; whatever the plan.
            let halted = behind_water || watching;
            let advance = if halted {
                false
            } else if only_archers {
                true
            } else if coming {
                meet && !losing
            } else if losing {
                // Wait on our own ground for them to come. Being shot to
                // pieces with no falling back left is the exception. An
                // enemy that never comes has to be fetched in the end, or
                // the battle never ends. Only time with nobody coming and
                // nobody fighting counts.
                if !ours.iter().any(|r| r.engaged) {
                    let since = *self.waiting_since.insert(was_waiting.unwrap_or(now));
                    if (now - since) as f32 / TICKS > TIRED_OF_WAITING {
                        self.fetching = true;
                        ctx.journal
                            .note(now, "Tired of waiting: going to fetch them".to_string());
                    }
                }
                outshot && self.withdrawals >= WITHDRAWALS
            } else {
                // Ambushers lying in wait need the enemy to come to them,
                // archers in range or not.
                outshot || (out_of_range && !self.lying_in_wait()) || waited > patience.max(0.0)
            };
            let advance = match self.plan.map(|plan| plan.stance) {
                Some(Stance::Advance) => !halted,
                Some(Stance::Hold) => {
                    // Holding still gives up on an enemy that never comes,
                    // or the battle never ends: once nobody has come on or
                    // fought for longer than anyone waits before going to
                    // fetch an enemy, so it never gives up a standoff first.
                    let quiet = self.closing <= 1.0 && !ours.iter().any(|r| r.engaged);
                    if !halted && !only_archers && quiet && !self.fetching {
                        let since = *self.waiting_since.insert(was_waiting.unwrap_or(now));
                        if (now - since) as f32 / TICKS > 2.0 * TIRED_OF_WAITING {
                            self.fetching = true;
                            ctx.journal
                                .note(now, "Tired of waiting: going to fetch them".to_string());
                        }
                    }
                    !halted && only_archers
                }
                _ => advance,
            };
            let want = if advance {
                Posture::Advance
            } else {
                Posture::Hold
            };
            // Give each posture a moment before changing it again, so the
            // line doesn't stop and start with every look.
            let settled = now >= self.posture_since + POSTURE_TICKS;
            if want != self.posture && (settled || watching) {
                self.posture = want;
                if want == Posture::Advance && self.covered != 0.0 {
                    // Leaving its cover behind: hammers on both wings again.
                    self.covered = 0.0;
                    self.relayout = true;
                }
                if want == Posture::Hold {
                    // Halt where the line stands now; one halting to watch
                    // keeps the patience it had left.
                    if watching {
                        ctx.journal
                            .note(now, "Halting to watch the champions".to_string());
                    } else {
                        self.standoff_since = now;
                    }
                    let our_c = centroid(ours);
                    self.anchor = ctx
                        .terrain
                        .clamp(line_centre(&self.members, ours, our_c), 4.0);
                }
            }
            if self.posture == Posture::Advance {
                // March until the archers are in range and let them work,
                // if they shoot better than the enemy's and there is still
                // patience for it. Otherwise right up to the enemy, and the
                // charges take it from there.
                let shoot = mine.ranged > 0.0
                    && mine.ranged > theirs.ranged * 1.15
                    && !only_archers
                    && waited < 2.0 * patience.max(5.0);
                let shoot_from = if shoot {
                    (our_range - 2.0).max(STAND_OFF)
                } else {
                    STAND_OFF
                };
                let step = (self.front_gap(fighting) - shoot_from).max(0.0);
                let goal = ctx.terrain.clamp(self.anchor + self.forward * step, 6.0);
                if goal.distance(self.anchor) >= ANCHOR_SLACK || step < ANCHOR_SLACK {
                    self.anchor = goal;
                }
            }
        }
        if self.posture != before {
            self.posture_since = now;
            ctx.journal.note(
                now,
                format!(
                    "{} ({})",
                    capitalised(self.posture.name()),
                    self.doctrine.name()
                ),
            );
        }
    }

    /// How far in front of the line its archers can reach, tiles.
    fn battery_reach(&self, ctx: &Ctx) -> f32 {
        self.members
            .iter()
            .filter(|m| matches!(m.role, Role::Battery | Role::Screen))
            .filter_map(|m| ctx.view.get(m.handle).map(|r| (m, r)))
            .map(|(m, r)| ctx.view.unit(r).range - m.slot.y - self.line_depth * 0.5)
            .fold(0.0, f32::max)
    }

    /// Nobody left standing against us: hunters chase routers, everyone
    /// else stands in formation.
    fn mop_up(&mut self, ctx: &mut Ctx, ours: &[&Seen], foes: &[&Seen], wants: &mut Vec<Want>) {
        if self.relayout && self.started {
            self.layout(ctx);
        }
        let our_c = centroid(ours);
        for k in 0..self.members.len() {
            let m = self.members[k];
            let Some(r) = ctx.view.get(m.handle) else {
                continue;
            };
            // Ambushers stay hidden while nothing is in sight.
            if r.broken() || r.engaged || m.lost || m.role == Role::Ambusher {
                continue;
            }
            let mut m = m;
            // Fast troops chase; so does everyone at the easiest level.
            let hunts = m.class != Class::Ranged
                && (m.class == Class::Shock || ctx.knobs.slip_percent >= 30)
                && self.hunt(ctx, &mut m, r, our_c, foes, wants);
            if !hunts && self.started {
                m.target = None;
                let to = self.place(ctx, m.slot);
                wants.push(slot_move(m.handle, to, self.forward, 0.0));
            }
            self.members[k] = m;
        }
    }

    /// Chase the nearest router within the leash. Returns whether it hunts.
    fn hunt(
        &self,
        ctx: &Ctx,
        m: &mut Member,
        r: &Seen,
        our_c: Vec2,
        foes: &[&Seen],
        wants: &mut Vec<Want>,
    ) -> bool {
        let leash = ctx.knobs.hunt_leash;
        let quarry = foes
            .iter()
            // Routers can rally; shattered regiments are out for good.
            .filter(|f| f.morale == MoraleState::Routing)
            .filter(|f| f.pos.distance(our_c) <= leash)
            .min_by(|a, b| a.pos.distance(r.pos).total_cmp(&b.pos.distance(r.pos)));
        match quarry {
            Some(q) => {
                if m.role != Role::Hunter {
                    m.home = if m.role == Role::Recover {
                        m.home
                    } else {
                        m.role
                    };
                    m.role = Role::Hunter;
                }
                m.target = Some(q.handle);
                wants.push(Want {
                    regiment: m.handle,
                    act: Act::Attack(q.handle),
                    priority: 2,
                    group: 0,
                });
                true
            }
            None => {
                if m.role == Role::Hunter {
                    m.role = m.home;
                    m.target = None;
                }
                false
            }
        }
    }

    fn act(
        &mut self,
        ctx: &mut Ctx,
        ours: &[&Seen],
        fighting: &[&Seen],
        foes: &[&Seen],
        wants: &mut Vec<Want>,
    ) {
        let view = ctx.view;
        let our_c = centroid(ours);
        // The enemy line is locked once most of its melee regiments are
        // fighting: then it can't turn to meet a hammer.
        let melee = fighting
            .iter()
            .filter(|f| view.unit(f).class != Class::Ranged);
        let (fighting_melee, engaged_melee) =
            melee.fold((0, 0), |(n, e), f| (n + 1, e + usize::from(f.engaged)));
        let locked = engaged_melee * 2 >= fighting_melee.max(1);
        let pace = if self.posture == Posture::Advance {
            self.pace
        } else {
            0.0
        };
        // Who is already going for whom, so friends can gang up (or not).
        let mut on: Vec<(RegimentHandle, u8)> = Vec::new();
        for m in &self.members {
            if let Some(t) = m.target
                && view.get(m.handle).is_some_and(|r| !r.broken())
            {
                bump(&mut on, t);
            }
        }
        let routers = foes.iter().any(|f| f.broken());
        self.taken = self.taken_ground(view, ctx.party);
        self.focus = if ctx.knobs.morale_targeting {
            point_of_attack(view, fighting, self.anchor)
        } else {
            None
        };
        for k in 0..self.members.len() {
            let mut m = self.members[k];
            let Some(r) = view.get(m.handle) else {
                continue;
            };
            if m.lost || r.broken() {
                continue;
            }
            if r.engaged {
                // Locked in melee: nothing an order can change. When the
                // fight ends the role picks up from here.
                continue;
            }
            // Out of the fight while wavering: pull it back to recover.
            if ctx.knobs.save_wavering
                && r.morale == MoraleState::Wavering
                && !matches!(m.role, Role::Recover | Role::Hunter)
                && !(m.role == Role::Hammer && m.striking)
            {
                m.home = m.role;
                m.role = Role::Recover;
                m.target = None;
                // One place to fall back to, not a new one every look.
                m.retreat_to = Some(self.place(ctx, m.slot + Vec2::new(0.0, RECOVER_BACK)));
            }
            if m.role == Role::Recover && r.morale == MoraleState::Steady {
                m.role = m.home;
                m.retreat_to = None;
            }
            let before_target = m.target;
            match m.role {
                Role::Recover => {
                    let back = m
                        .retreat_to
                        .unwrap_or_else(|| self.place(ctx, m.slot + Vec2::new(0.0, RECOVER_BACK)));
                    wants.push(Want {
                        regiment: m.handle,
                        act: Act::Move {
                            to: back,
                            facing: Some(self.forward),
                            pace: 0.0,
                        },
                        priority: 4,
                        group: 0,
                    });
                }
                Role::Hunter => {
                    if !self.hunt(ctx, &mut m, r, our_c, foes, wants) {
                        // The quarry got away (or rallied): back to work.
                        let to = self.place(ctx, m.slot);
                        wants.push(slot_move(m.handle, to, self.forward, pace));
                    }
                }
                Role::Anchor | Role::Wing => {
                    self.line_role(ctx, &mut m, r, fighting, foes, &on, pace, wants);
                }
                Role::Reserve => {
                    self.reserve_role(ctx, &mut m, r, fighting, &on, pace, wants);
                }
                Role::Hammer => {
                    let idle = !m.striking
                        && routers
                        && !fighting.iter().any(|f| f.pos.distance(r.pos) < 30.0);
                    if !(idle && self.hunt(ctx, &mut m, r, our_c, foes, wants)) {
                        self.hammer_role(ctx, &mut m, r, fighting, locked, wants);
                    }
                }
                Role::Battery | Role::Screen => {
                    self.archer_role(ctx, &mut m, r, fighting, foes, &on, pace, wants);
                }
                Role::Ambusher => {
                    self.ambusher_role(ctx, &mut m, r, ours, fighting, wants);
                }
            }
            if m.target != before_target {
                if let Some(t) = before_target {
                    unbump(&mut on, t);
                }
                if let Some(t) = m.target {
                    bump(&mut on, t);
                }
            }
            self.members[k] = m;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn line_role(
        &self,
        ctx: &mut Ctx,
        m: &mut Member,
        r: &Seen,
        fighting: &[&Seen],
        foes: &[&Seen],
        on: &[(RegimentHandle, u8)],
        pace: f32,
        wants: &mut Vec<Want>,
    ) {
        let view = ctx.view;
        let home = self.place(ctx, m.slot);
        let reach = ctx.knobs.engage_tiles + 3.0 * ctx.persona.aggression();
        let leash = LINE_LEASH + reach;
        let right = right_of(self.forward);
        let help = ctx.knobs.focus_fire;
        // The wing holding the end of the line against cover.
        let anchored = m.role == Role::Wing && m.slot.x * self.covered > 0.0;
        let terrain = ctx.terrain;
        // Only archers still standing against us: go and get them.
        let only_archers = fighting.iter().all(|f| view.unit(f).class == Class::Ranged);
        let wanted = |f: &Seen, slack: f32| {
            let gap = gap_between(r, f) - slack;
            if gap <= 3.0 {
                // Close enough to hit us: always an answer.
                return true;
            }
            if self.beyond_water(terrain, r, f) {
                return false;
            }
            if only_archers {
                return true;
            }
            let from_home = f.pos.distance(home) - slack;
            // Coming at our stretch of the line (or the point of attack,
            // a little further).
            let across = (f.pos - home).dot(right).abs();
            let reach = if Some(f.handle) == self.focus {
                reach + FOCUS_REACH
            } else {
                reach
            };
            // An anchored wing only meets what comes straight at it, rather
            // than leave the end of the line open.
            let side_slack = if anchored { 0.0 } else { 8.0 };
            if gap <= reach
                && from_home <= leash
                && across <= r.width * 0.5 + f.width * 0.5 + side_slack
            {
                return true;
            }
            // Locked in a fight with a friend close by: join in, where there
            // is room beside the friends already fighting it.
            if help
                && !anchored
                && f.engaged
                && gap <= HELP_TILES
                && from_home <= leash
                && self.room_at(r, f)
            {
                return true;
            }
            // Archers left unguarded close by, with nothing in the way.
            view.unit(f).class == Class::Ranged
                && gap <= 2.0 * reach
                && exposed(view, fighting, f)
                && path_clear(view, fighting, r.pos, f.pos, f.handle)
        };
        let keep = m
            .target
            .and_then(|t| view.get(t))
            .filter(|t| !t.broken() && wanted(t, 6.0))
            .map(|t| t.handle);
        let candidates = fighting.iter().filter(|f| wanted(f, 0.0));
        match self.best_melee_target(ctx, r, candidates, on, keep) {
            Some(t) => {
                m.target = Some(t);
                wants.push(attack(m.handle, t, 3));
            }
            None => {
                // Nobody to fight: see off a router close by, so it can't
                // rally and come back, then return to its place.
                match self.pursuit(ctx, r, home, foes) {
                    Some(q) => {
                        m.target = Some(q);
                        wants.push(attack(m.handle, q, 2));
                    }
                    None => {
                        m.target = None;
                        wants.push(slot_move(m.handle, home, self.forward, pace));
                    }
                }
            }
        }
    }

    /// A router close to `r` and its place in the line, worth chasing so it
    /// can't rally (routers rally once no enemy is near for a while).
    fn pursuit(&self, ctx: &Ctx, r: &Seen, home: Vec2, foes: &[&Seen]) -> Option<RegimentHandle> {
        if !ctx.knobs.focus_fire {
            return None;
        }
        foes.iter()
            .filter(|f| f.morale == MoraleState::Routing)
            .filter(|f| {
                gap_between(r, f) <= PURSUE_TILES
                    && f.pos.distance(home) <= LINE_LEASH + PURSUE_TILES
            })
            .min_by(|a, b| a.pos.distance(r.pos).total_cmp(&b.pos.distance(r.pos)))
            .map(|f| f.handle)
    }

    #[allow(clippy::too_many_arguments)]
    fn reserve_role(
        &self,
        ctx: &mut Ctx,
        m: &mut Member,
        r: &Seen,
        fighting: &[&Seen],
        on: &[(RegimentHandle, u8)],
        pace: f32,
        wants: &mut Vec<Want>,
    ) {
        let view = ctx.view;
        let back_of_line = self.line_depth * 0.5;
        // A breach: an enemy that isn't fighting is behind our line or close
        // to our archers.
        let archers: Vec<&Seen> = self
            .members
            .iter()
            .filter(|m| matches!(m.role, Role::Battery | Role::Screen))
            .filter_map(|m| view.get(m.handle))
            .collect();
        let breach = fighting.iter().filter(|f| {
            !f.engaged
                && f.pos.distance(r.pos) < 40.0
                && ((self.anchor - f.pos).dot(self.forward) > back_of_line
                    || archers.iter().any(|a| gap_between(a, f) < 12.0))
        });
        // Or a gap: a line regiment gone, broken or recovering, with the
        // enemy near its place.
        let gap_slot = self
            .members
            .iter()
            .filter(|o| matches!(o.home, Role::Anchor | Role::Wing))
            .filter(|o| {
                o.lost || o.role == Role::Recover || view.get(o.handle).is_none_or(Seen::broken)
            })
            .map(|o| self.place(ctx, o.slot))
            .find(|p| fighting.iter().any(|f| f.pos.distance(*p) < 22.0));
        // Or a wavering enemy locked in melee nearby, to finish.
        let finish = fighting.iter().filter(|f| {
            ctx.knobs.morale_targeting
                && f.engaged
                && f.morale == MoraleState::Wavering
                && f.pos.distance(r.pos) < 22.0
        });
        // Or a friend in a fight it is losing on numbers: weigh in there.
        let party = ctx.party;
        let support = fighting.iter().filter(|f| {
            f.engaged && f.pos.distance(r.pos) < SUPPORT_TILES && {
                let (ours, theirs) = local_soldiers(view, party, f.pos, r.handle);
                theirs >= ours * 0.9
            }
        });
        let keep = m
            .target
            .and_then(|t| view.get(t))
            .filter(|t| !t.broken() && t.pos.distance(r.pos) < 45.0)
            .map(|t| t.handle);
        // A breach first; then a gap in the line with the enemy coming at
        // it, before helping anywhere else.
        let breach: Vec<&&Seen> = breach.collect();
        let target = match gap_slot {
            Some(_) if breach.is_empty() => None,
            _ => self.best_melee_target(
                ctx,
                r,
                breach.into_iter().chain(finish).chain(support),
                on,
                keep,
            ),
        };
        if let Some(t) = target {
            m.target = Some(t);
            wants.push(attack(m.handle, t, 3));
            return;
        }
        let to = match gap_slot {
            Some(p) => {
                // Standing in the gap, it fights like the line.
                let near = fighting.iter().filter(|f| {
                    gap_between(r, f) <= ctx.knobs.engage_tiles && f.pos.distance(p) < 16.0
                });
                let keep = keep.filter(|&t| view.get(t).is_some_and(|t| t.pos.distance(p) < 16.0));
                if let Some(t) = self.best_melee_target(ctx, r, near, on, keep) {
                    m.target = Some(t);
                    wants.push(attack(m.handle, t, 3));
                    return;
                }
                p
            }
            None => self.place(ctx, m.slot),
        };
        m.target = None;
        wants.push(slot_move(m.handle, to, self.forward, pace));
    }

    fn hammer_role(
        &self,
        ctx: &mut Ctx,
        m: &mut Member,
        r: &Seen,
        fighting: &[&Seen],
        locked: bool,
        wants: &mut Vec<Want>,
    ) {
        let view = ctx.view;
        let timed = ctx.knobs.timed_hammer;
        // A strike in progress: keep at it while the target stands.
        if m.striking {
            match m.target.and_then(|t| view.get(t)).filter(|t| !t.broken()) {
                Some(t) => {
                    self.strike(ctx, m, r, t, wants);
                    return;
                }
                None => {
                    m.striking = false;
                    m.target = None;
                }
            }
        }
        let side = m.slot.x.signum();
        let right = right_of(self.forward);
        let exposed = |f: &Seen| exposed(view, fighting, f);
        // Something coming at it while it waits: meet it head on if it can
        // win that fight (both then charge), or get out of its way.
        if let Some(threat) = self.threat(ctx, r, fighting) {
            let mine = Force::of(view, [r]);
            let theirs = Force::of(view, [threat]);
            let duel = estimator::estimate(&mine, &theirs, Approach::CONTACT);
            // Running from something as fast only gets it charged in the back.
            let outruns = view.pace(r) > view.pace(threat) * 1.1;
            if duel.margin > 0.2 || !outruns || ctx.slip() {
                m.target = Some(threat.handle);
                wants.push(attack(m.handle, threat.handle, 4));
            } else {
                // Behind our own line, where the line can take it in the flank.
                let behind = Vec2::new(m.slot.x * 0.6, self.line_depth + RESERVE_BACK);
                let to = self.place(ctx, behind);
                m.target = None;
                wants.push(Want {
                    regiment: m.handle,
                    act: Act::Move {
                        to,
                        facing: Some(self.forward),
                        pace: 0.0,
                    },
                    priority: 4,
                    group: 0,
                });
            }
            return;
        }
        let terrain = ctx.terrain;
        let go = if !timed {
            fighting.iter().any(|f| gap_between(r, f) < 30.0)
        } else {
            locked
                || fighting.iter().any(|f| {
                    exposed(f)
                        && f.pos.distance(r.pos) < 45.0
                        && path_clear(view, fighting, r.pos, f.pos, f.handle)
                        && !self.beyond_water(terrain, r, f)
                })
        };
        if go {
            let reckless = !timed || ctx.slip();
            let target = fighting
                .iter()
                .filter(|f| !self.beyond_water(terrain, r, f))
                .filter(|f| {
                    // Never fed into the front of a steady enemy that is
                    // free to meet it.
                    let can_reach = locked || path_clear(view, fighting, r.pos, f.pos, f.handle);
                    can_reach
                        && (reckless
                            || f.engaged
                            || exposed(f)
                            || f.side_facing(r.pos) != Side::Front
                            || view.unit(f).class == Class::Ranged)
                })
                .map(|f| {
                    let mut s = -f.pos.distance(r.pos) / 15.0;
                    if f.engaged {
                        s += 2.0;
                    }
                    if exposed(f) {
                        s += 1.5;
                    }
                    if (f.pos - self.anchor).dot(right) * side > 0.0 {
                        s += 1.0;
                    }
                    if ctx.knobs.morale_targeting {
                        if f.morale == MoraleState::Wavering {
                            s += 1.5 + ctx.persona.opportunism();
                        }
                        if view.unit(f).stalwart {
                            s -= 1.0;
                        }
                    }
                    (s, f.handle)
                })
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, h)| h);
            if let Some(t) = target.and_then(|t| view.get(t)) {
                m.striking = true;
                m.target = Some(t.handle);
                self.strike(ctx, m, r, t, wants);
                return;
            }
        }
        // Not yet: wait in its place, keeping level with the line on the
        // march rather than racing ahead of it.
        let pace = if self.posture == Posture::Advance {
            self.pace
        } else {
            0.0
        };
        let to = self.place(ctx, m.slot);
        wants.push(slot_move(m.handle, to, self.forward, pace));
    }

    /// The nearest enemy melee regiment marching at `r` and about to reach it.
    fn threat<'a>(&self, ctx: &Ctx, r: &Seen, fighting: &[&'a Seen]) -> Option<&'a Seen> {
        let view = ctx.view;
        fighting
            .iter()
            .copied()
            .filter(|f| !f.engaged && f.moving && view.unit(f).class != Class::Ranged)
            .filter(|f| gap_between(r, f) <= COUNTER_TILES)
            .filter(|f| {
                let to_us = (r.pos - f.pos).normalize_or_zero();
                f.facing.dot(to_us) > 0.8
            })
            .min_by(|a, b| a.pos.distance(r.pos).total_cmp(&b.pos.distance(r.pos)))
    }

    /// Swing round to the target's flank or rear, then charge it.
    fn strike(&self, ctx: &mut Ctx, m: &mut Member, r: &Seen, t: &Seen, wants: &mut Vec<Want>) {
        let timed = ctx.knobs.timed_hammer;
        let across = right_of(t.facing);
        let side = {
            let s = (r.pos - t.pos).dot(across);
            if s == 0.0 { 1.0 } else { s.signum() }
        };
        let stage = t.pos + across * side * (t.width * 0.5 + STAGE_OUT)
            - t.facing * (t.depth * 0.5 + STAGE_BACK);
        let level = (r.pos - t.pos).dot(t.facing) < t.depth * 0.5;
        let flank = t.side_facing(r.pos) != Side::Front;
        let charge = !timed
            || (flank
                && level
                && (r.pos.distance(t.pos) < 16.0 || (r.pos - t.pos).dot(t.facing) < 0.0))
            || r.pos.distance(stage) < 4.0;
        if charge {
            wants.push(attack(m.handle, t.handle, 3));
        } else {
            let to = ctx.terrain.clamp(stage, 3.0);
            wants.push(Want {
                regiment: m.handle,
                act: Act::Move {
                    to,
                    facing: Some((t.pos - to).normalize_or(self.forward)),
                    pace: 0.0,
                },
                priority: 3,
                group: 0,
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn archer_role(
        &self,
        ctx: &mut Ctx,
        m: &mut Member,
        r: &Seen,
        fighting: &[&Seen],
        foes: &[&Seen],
        on: &[(RegimentHandle, u8)],
        pace: f32,
        wants: &mut Vec<Want>,
    ) {
        let view = ctx.view;
        let range = view.unit(r).range;
        let danger_at = if m.role == Role::Screen {
            SCREEN_DANGER
        } else {
            BATTERY_DANGER
        };
        let danger = fighting
            .iter()
            .filter(|f| !f.engaged && view.unit(f).class != Class::Ranged)
            .filter(|f| gap_between(r, f) < danger_at)
            .min_by(|a, b| gap_between(r, a).total_cmp(&gap_between(r, b)));
        if let Some(d) = danger {
            if m.role == Role::Screen {
                // The screen's job is done: back through the line.
                m.role = Role::Battery;
                m.home = Role::Battery;
                m.slot = m.fallback;
                m.fell_back = true;
                m.target = None;
                let to = self.place(ctx, m.slot);
                wants.push(Want {
                    regiment: m.handle,
                    act: Act::Move {
                        to,
                        facing: Some(self.forward),
                        pace: 0.0,
                    },
                    priority: 4,
                    group: 0,
                });
                return;
            }
            if !ctx.slip() {
                // Step back out of reach, away from the threat. Keep going
                // to the same place while it is still out of reach, rather
                // than giving a new order every look.
                let safe = |p: Vec2| p.distance(d.pos) - d.radius() > danger_at + 4.0;
                let to = match m.retreat_to.filter(|&p| safe(p)) {
                    Some(p) => p,
                    None => {
                        let away = (r.pos - d.pos).normalize_or(-self.forward);
                        let to = ctx.terrain.clamp(r.pos + away * 14.0, 3.0);
                        ctx.terrain.nearest_walkable(to, 6.0).unwrap_or(to)
                    }
                };
                m.retreat_to = Some(to);
                m.target = None;
                wants.push(Want {
                    regiment: m.handle,
                    act: Act::Move {
                        to,
                        facing: Some(self.forward),
                        pace: 0.0,
                    },
                    priority: 4,
                    group: 0,
                });
                return;
            }
        }
        m.retreat_to = None;
        // Shoot what helps most among those in range.
        let in_range = |f: &&&Seen| f.pos.distance(r.pos) <= range - 0.5;
        let score = |f: &Seen| {
            let mut s = -f.pos.distance(r.pos) / range.max(1.0);
            if f.engaged {
                s += 1.5;
            }
            if ctx.knobs.morale_targeting {
                if f.morale == MoraleState::Wavering {
                    s += 1.5 + ctx.persona.opportunism();
                }
                s += 0.4 * shaken_near(fighting, f) as f32;
                if view.unit(f).stalwart {
                    s -= 0.5;
                }
            }
            if view.unit(f).class == Class::Ranged {
                s += 0.5;
            }
            // Denser blocks take more arrows.
            s += 0.3 * (f32::from(f.models) / (f.width * f.depth).max(1.0)).min(2.0);
            if ctx.knobs.focus_fire {
                s += 0.2 * f32::from(count(on, f.handle));
            }
            s
        };
        let keep = m
            .target
            .and_then(|t| view.get(t))
            .filter(|t| !t.broken() && t.pos.distance(r.pos) <= range - 0.5);
        let best = fighting
            .iter()
            .filter(in_range)
            .map(|f| (score(f), f.handle))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        let pick = match (keep, best) {
            (Some(k), Some((s, h))) if s > score(k) + 1.5 => Some(h),
            (Some(k), _) => Some(k.handle),
            (None, b) => b.map(|(_, h)| h),
        };
        // With nothing standing in range, thin out routers before they rally.
        let pick = pick.or_else(|| {
            foes.iter()
                .filter(|f| f.broken() && f.pos.distance(r.pos) <= range - 0.5)
                .min_by(|a, b| a.pos.distance(r.pos).total_cmp(&b.pos.distance(r.pos)))
                .map(|f| f.handle)
        });
        match pick {
            Some(t) => {
                m.target = Some(t);
                wants.push(attack(m.handle, t, 2));
            }
            None => {
                m.target = None;
                let to = self.place(ctx, m.slot);
                wants.push(slot_move(m.handle, to, self.forward, pace));
            }
        }
    }

    /// Whether regiment `r` can meet enemy `f` straight in without
    /// overlapping a friend there: one fighting, or one nearer its own enemy
    /// and closing on it (where it will meet that enemy). Two regiments on
    /// one face of an enemy push into each other and both fall out of order;
    /// a second does better on another face, or elsewhere.
    fn room_at(&self, r: &Seen, f: &Seen) -> bool {
        let at = meeting(r, f);
        let near = r.clearance(f);
        let reach = r.pos.distance(f.pos) + f.radius() + r.radius();
        self.taken.iter().all(|t| {
            t.handle == r.handle
                || t.pos.distance(f.pos) > reach
                || t.closing.is_some_and(|c| c >= near)
                || at.overlap(&t.block) <= ROOM
        })
    }

    /// Friends' ground in the fight as they stand now, for
    /// [`Captain::room_at`]: allies' fighting regiments, and ours fighting
    /// or closing on an enemy.
    fn taken_ground(&self, view: &Perception, party: PlayerId) -> Vec<Taken> {
        view.regiments
            .iter()
            .filter(|g| !g.broken() && !view.hostile(party, g.owner))
            .filter_map(|g| {
                let (block, closing) = if g.engaged {
                    (*g, None)
                } else {
                    let e = self
                        .members
                        .iter()
                        .find(|m| m.handle == g.handle)
                        .and_then(|m| view.get(m.target?))
                        .filter(|e| !e.broken() && view.hostile(party, e.owner))?;
                    (meeting(g, e), Some(g.clearance(e)))
                };
                Some(Taken {
                    handle: g.handle,
                    pos: g.pos,
                    block,
                    closing,
                })
            })
            .collect()
    }

    /// The best enemy for a melee regiment to go for among `candidates`.
    /// `keep` is its current target, which it sticks with unless something
    /// is clearly better.
    fn best_melee_target<'a>(
        &self,
        ctx: &mut Ctx,
        r: &Seen,
        candidates: impl Iterator<Item = &'a &'a Seen>,
        on: &[(RegimentHandle, u8)],
        keep: Option<RegimentHandle>,
    ) -> Option<RegimentHandle> {
        let view = ctx.view;
        let me = view.unit(r);
        let knobs = ctx.knobs;
        let party = ctx.party;
        let opportunism = ctx.persona.opportunism();
        let score = |f: &Seen| {
            let them = view.unit(f);
            let mut s = -f.pos.distance(r.pos) / 8.0;
            s += match f.side_facing(r.pos) {
                Side::Front => 0.0,
                Side::Flank => 1.0,
                Side::Rear => 1.8,
            };
            if f.engaged {
                s += 1.0;
            }
            let wavering = f.morale == MoraleState::Wavering;
            if Some(f.handle) == self.focus {
                s += 1.0;
            }
            if knobs.morale_targeting {
                let shaken = shaken_near_all(view, f) as f32;
                if wavering {
                    // About to break, and its rout costs every friend
                    // within 16 tiles 8 points: worth most beside others
                    // already shaken, which it may take with it.
                    s += 1.5 + opportunism + 0.8 * shaken;
                } else {
                    s += 0.5 * shaken;
                }
                if them.stalwart {
                    s -= 1.0;
                }
                s += 1.0 - f.strength();
            }
            if knobs.focus_fire {
                // Local odds: worth most where joining makes them two or
                // three to one (which breaks nerve), little where they
                // already are, and walking alone into a crowd is a poor idea
                // (less so if the one it hits is about to run).
                let (ours, theirs) = local_soldiers(view, party, f.pos, r.handle);
                let theirs = theirs.max(1.0);
                let before = ours / theirs;
                let after = (ours + f32::from(r.models)) / theirs;
                s += if before >= 2.0 {
                    -0.5
                } else if after >= 2.0 {
                    1.0
                } else if after >= 1.2 {
                    0.4
                } else if after >= 0.8 {
                    0.0
                } else if wavering && knobs.morale_targeting {
                    -0.2
                } else {
                    -0.8
                };
            }
            // Two on one is the aim; a third is better spent elsewhere.
            let n = i32::from(count(on, f.handle)) - i32::from(keep == Some(f.handle));
            s += if knobs.focus_fire {
                match n {
                    0 => 0.0,
                    1 => 0.8,
                    2 => -0.6,
                    _ => -2.0,
                }
            } else {
                -0.5 * n.max(0) as f32
            };
            if knobs.focus_fire && !self.room_at(r, f) {
                s -= NO_ROOM;
            }
            let hit = (50.0 + f32::from(me.stats.melee_attack)
                - f32::from(them.stats.melee_defence))
            .clamp(10.0, 90.0);
            s += (hit - 50.0) / 25.0;
            if them.class == Class::Ranged {
                s += 0.8;
            }
            s
        };
        let mut best: Option<(f32, RegimentHandle)> = None;
        let mut kept: Option<f32> = None;
        for f in candidates {
            let s = score(f);
            if Some(f.handle) == keep {
                kept = Some(s);
            }
            if best.is_none_or(|(b, _)| s > b) {
                best = Some((s, f.handle));
            }
        }
        match (kept, best) {
            (Some(k), Some((b, h))) => Some(if b > k + 1.0 { h } else { keep.unwrap() }),
            (None, b) => b.map(|(_, h)| h),
            (Some(_), None) => keep,
        }
    }
}

/// `r`'s block standing against `f`, having come straight at its centre.
fn meeting(r: &Seen, f: &Seen) -> Seen {
    let dir = (f.pos - r.pos).normalize_or(-f.facing);
    Seen {
        pos: f.pos - dir * (f.half_extent(dir) + 0.5 * r.depth + MEET_GAP),
        facing: dir,
        ..*r
    }
}

/// Whether an army with this many line, shock and ranged regiments can
/// fight as `d` asks: a hammer needs fast troops and something to pin the
/// enemy with, a refused flank a line long enough to bend, a screen archers
/// and a line to fall back behind, an ambush regiments to hide (and, on
/// the field, somewhere to hide them: see `Captain::may_ambush`).
pub fn fits(d: Doctrine, line: usize, shock: usize, ranged: usize) -> bool {
    match d {
        Doctrine::HoldTheLine => true,
        Doctrine::HammerAndAnvil => shock > 0 && line + ranged > 0,
        Doctrine::RefusedFlank => line >= 4,
        Doctrine::SkirmishScreen => ranged >= 2 && line > 0,
        Doctrine::Ambush => ambush::ambusher_class(line, shock).is_some(),
    }
}

/// Tiles between two blocks' edges, roughly (their circles).
pub fn gap_between(a: &Seen, b: &Seen) -> f32 {
    a.pos.distance(b.pos) - a.depth * 0.5 - b.radius() * 0.8
}

fn centroid(rs: &[&Seen]) -> Vec2 {
    let mut sum = Vec2::ZERO;
    let mut n = 0.0;
    for r in rs {
        let w = f32::from(r.models.max(1));
        sum += r.pos * w;
        n += w;
    }
    if n > 0.0 { sum / n } else { Vec2::ZERO }
}

/// The centre of the hostile army nearest `from` (the one the line faces
/// when several enemies are on the field).
fn nearest_army(_party: PlayerId, from: Vec2, fighting: &[&Seen]) -> Vec2 {
    let mut best: Option<(f32, Vec2)> = None;
    let mut owners: Vec<PlayerId> = fighting.iter().map(|f| f.owner).collect();
    owners.sort_unstable();
    owners.dedup();
    for o in owners {
        let army: Vec<&Seen> = fighting.iter().copied().filter(|f| f.owner == o).collect();
        let c = centroid(&army);
        let d = c.distance(from);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, c));
        }
    }
    best.map_or(from, |(_, c)| c)
}

/// The middle of the line as it stands (its line regiments, or everyone).
fn line_centre(members: &[Member], ours: &[&Seen], fallback: Vec2) -> Vec2 {
    let line: Vec<&Seen> = ours
        .iter()
        .copied()
        .filter(|r| {
            members
                .iter()
                .any(|m| m.handle == r.handle && matches!(m.home, Role::Anchor | Role::Wing))
        })
        .collect();
    if line.is_empty() {
        fallback
    } else {
        centroid(&line)
    }
}

/// Soldiers still fighting within [`LOCAL_TILES`] of `p`: on `party`'s
/// side (leaving out `skip`) and against it.
fn local_soldiers(view: &Perception, party: PlayerId, p: Vec2, skip: RegimentHandle) -> (f32, f32) {
    let (mut ours, mut theirs) = (0.0, 0.0);
    for g in &view.regiments {
        if g.handle == skip || g.broken() || g.pos.distance(p) > LOCAL_TILES {
            continue;
        }
        if view.hostile(party, g.owner) {
            theirs += f32::from(g.models);
        } else {
            ours += f32::from(g.models);
        }
    }
    (ours, theirs)
}

/// Whether the way from `from` to `to` passes clear of enemy melee
/// regiments that are free to turn on whoever goes by.
fn path_clear(
    view: &Perception,
    fighting: &[&Seen],
    from: Vec2,
    to: Vec2,
    skip: RegimentHandle,
) -> bool {
    let d = to - from;
    let len2 = d.length_squared().max(1e-6);
    fighting.iter().all(|f| {
        if f.handle == skip || f.engaged || view.unit(f).class == Class::Ranged {
            return true;
        }
        let t = ((f.pos - from).dot(d) / len2).clamp(0.0, 1.0);
        (from + d * t).distance(f.pos) > f.radius() + 5.0
    })
}

/// Archers with no melee regiment of theirs close enough to protect them.
fn exposed(view: &Perception, fighting: &[&Seen], f: &Seen) -> bool {
    view.unit(f).class == Class::Ranged
        && !fighting.iter().any(|g| {
            g.handle != f.handle
                && g.owner == f.owner
                && view.unit(g).class != Class::Ranged
                && g.pos.distance(f.pos) < 12.0
        })
}

/// The enemy regiment whose rout would do most harm, among the wavering
/// within [`FOCUS_TILES`] of the line at `anchor`: the one with most shaken
/// friends beside it, closest to breaking. None if none is wavering.
fn point_of_attack(view: &Perception, fighting: &[&Seen], anchor: Vec2) -> Option<RegimentHandle> {
    fighting
        .iter()
        .filter(|f| f.morale == MoraleState::Wavering && !view.unit(f).stalwart)
        .filter(|f| f.pos.distance(anchor) <= FOCUS_TILES)
        .map(|f| {
            let s = shaken_near_all(view, f) as f32 + (1.0 - f.strength())
                - f32::from(f.morale_points) / 25.0
                - f.pos.distance(anchor) / FOCUS_TILES;
            (s, f.handle)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, h)| h)
}

/// Shaken enemies within 16 tiles of `f` (a rout there would cascade).
fn shaken_near(fighting: &[&Seen], f: &Seen) -> usize {
    fighting
        .iter()
        .filter(|g| g.handle != f.handle && g.shaken() && g.pos.distance(f.pos) < 16.0)
        .count()
}

fn shaken_near_all(view: &Perception, f: &Seen) -> usize {
    view.regiments
        .iter()
        .filter(|g| g.owner == f.owner && g.handle != f.handle && g.shaken() && !g.broken())
        .filter(|g| g.pos.distance(f.pos) < 16.0)
        .count()
}

fn count(on: &[(RegimentHandle, u8)], h: RegimentHandle) -> u8 {
    on.iter().find(|(x, _)| *x == h).map_or(0, |(_, n)| *n)
}

fn bump(on: &mut Vec<(RegimentHandle, u8)>, h: RegimentHandle) {
    match on.iter_mut().find(|(x, _)| *x == h) {
        Some((_, n)) => *n += 1,
        None => on.push((h, 1)),
    }
}

fn unbump(on: &mut [(RegimentHandle, u8)], h: RegimentHandle) {
    if let Some((_, n)) = on.iter_mut().find(|(x, _)| *x == h) {
        *n = n.saturating_sub(1);
    }
}

/// An attack. Regiments sent at the same enemy in the same look go
/// together as one order, like selecting them all and clicking it once.
fn attack(regiment: RegimentHandle, target: RegimentHandle, priority: u8) -> Want {
    Want {
        regiment,
        act: Act::Attack(target),
        priority,
        group: crate::orders::attack_group(target),
    }
}

/// A move to a place in the formation. These go out together as one order.
fn slot_move(regiment: RegimentHandle, to: Vec2, facing: Vec2, pace: f32) -> Want {
    Want {
        regiment,
        act: Act::Move {
            to,
            facing: Some(facing),
            pace,
        },
        priority: 1,
        group: 1,
    }
}

/// "a close fight", "a clear win", ... for the journal.
pub fn describe_margin(m: f32) -> &'static str {
    match m {
        m if m > 0.5 => "a clear win",
        m if m > 0.15 => "to win",
        m if m > -0.15 => "a close fight",
        m if m > -0.5 => "to lose",
        _ => "a heavy defeat",
    }
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}
