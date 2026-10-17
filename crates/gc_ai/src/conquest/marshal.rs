//! The Marshal: a Conquest mind's army (docs/18-ai.md, "Marshal"). New
//! regiments gather at a rally point in front of home; once the army is
//! big enough it marches on the nearest enemy building it knows of, or
//! goes to look at an enemy start, and knocks down what it finds. It turns
//! back to defend when its own buildings are attacked, and falls back to
//! rebuild when an attack has cost it most of its army. Fighting itself is
//! the Captain's ([`crate::captain`]): whenever enemies come near, a
//! Captain takes the army through the battle on a look that holds only
//! that army and the enemies about it, so the battle layers play exactly
//! as they do in a Field Battle.

use gc_sim::economy::SUPPLY_LIMIT;
use gc_sim::structures::{Role, StructureHandle};
use gc_sim::{PlayerId, RegimentHandle, TICKS_PER_SECOND};
use glam::Vec2;

use super::{Sense, spymaster};
use crate::captain::{Captain, Ctx};
use crate::difficulty::Knobs;
use crate::orders::{Act, Orders, ROUTINE, Want};
use crate::perception::{Perception, Seen, right_of, v2};
use crate::personality::Personality;

/// Ticks between the Marshal's looks.
const THINK_TICKS: u32 = TICKS_PER_SECOND;
/// Enemies within this many tiles of the army start a battle.
const ENGAGE: f32 = 36.0;
/// A battle takes in everyone within this many tiles of the army's middle.
const BATTLE_REACH: f32 = 60.0;
/// A battle ends after this many ticks with no enemy standing near.
const QUIET_TICKS: u32 = 8 * TICKS_PER_SECOND;
/// Ticks between looks for enemies near the army.
const LOOK_TICKS: u32 = 4;
/// How long it gathers after falling back before setting out again.
const REGROUP_TICKS: u32 = 30 * TICKS_PER_SECOND;
/// Ticks it contests the Titan's Heart, never holding it,
/// before it gives the Heart a rest and raids a base instead.
const HEART_PATIENCE: u32 = 10 * 60 * TICKS_PER_SECOND;
/// Ticks such a raid lasts, Heart or no Heart.
const RAID_TICKS: u32 = 8 * 60 * TICKS_PER_SECOND;
/// Odds (from the estimator, -1 to +1) a level that weighs them wants
/// before it attacks, and how much more a fully cautious one wants.
const ODDS_NEEDED: f32 = 0.05;
const ODDS_CAUTION: f32 = 0.3;
/// Odds so good it attacks before its wave is full.
const ODDS_STRIKE: f32 = 0.5;
/// Tiles of distance that cost as much as 1 of odds, when choosing whom to
/// attack.
const ODDS_PER_DISTANCE: f32 = 400.0;
/// Supply short of its current cap that counts as capped.
const CAP_SLACK: u32 = 8;
/// Enemies within this many tiles of its buildings are a threat to home.
const THREAT: f32 = 30.0;
/// Enemies at an outlying building call the whole army home once they are
/// this share of its soldiers.
const THREAT_SHARE: f32 = 0.25;
/// Regiments holding the Heart stand this far from its middle; the rest
/// screen them further out.
const HEART_RING: f32 = 3.5;
const HEART_SCREEN: f32 = 11.0;
/// Regiments in the Heart's inner ring.
const HEART_HOLDERS: usize = 6;
/// The rally point stands this far from home towards the enemy.
const RALLY_OUT: f32 = 16.0;
/// Within this many tiles of its target the army goes for the buildings.
const AT_TARGET: f32 = 20.0;
/// A start counts as looked at once a regiment comes this near it.
const SCOUTED: f32 = 14.0;
/// Tiles between regiments marching abreast.
const ABREAST: f32 = 7.0;
/// An attack falls back once the army is down to this share of the
/// soldiers it set out with.
const FALL_BACK_SHARE: f32 = 0.35;
/// Reinforcements at the rally point set out to join the attack once there
/// are this many regiments.
const REINFORCE: usize = 3;
/// Supply in fighters it attacks with at the start, what it adds each
/// minute, and the most it waits for.
const FIRST_WAVE: f32 = 16.0;
const PER_MINUTE: f32 = 2.5;
const BIGGEST_WAVE: f32 = 70.0;

/// What the army is for now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Goal {
    /// Gathering at the rally point.
    Gather,
    /// Going to meet enemies at its buildings.
    Defend(Vec2),
    /// Marching on an enemy building, or to look at a start.
    Attack(Vec2),
    /// Taking and holding the Titan's Heart.
    Heart(Vec2),
}

impl Goal {
    pub fn name(&self) -> &'static str {
        match self {
            Goal::Gather => "gathering",
            Goal::Defend(_) => "defending",
            Goal::Attack(_) => "attacking",
            Goal::Heart(_) => "holding the Heart",
        }
    }
}

/// A fight under way: a Captain and the look it fights on.
struct Battle {
    captain: Captain,
    view: Perception,
    /// Where the enemies it fights were last seen together.
    at: Vec2,
    quiet_since: Option<u32>,
    next_think: u32,
}

pub struct Marshal {
    next_think: u32,
    next_look: u32,
    pub goal: Goal,
    /// Where new regiments gather.
    pub rally: Option<Vec2>,
    /// The regiments out on the attack (the rest wait at the rally point).
    out: Vec<RegimentHandle>,
    /// Soldiers it set out with.
    set_out_with: u32,
    /// Starts looked at and found empty, by index.
    scouted: Vec<usize>,
    battle: Option<Battle>,
    /// The regiments standing on the Titan's Heart: the Marshal keeps them
    /// there, out of the Captain's battle, which would draw them off it.
    holders: Vec<RegimentHandle>,
    /// After falling back, it gathers until this tick before setting out.
    regroup_until: u32,
    /// Since when it has gone for the Heart without holding it.
    contesting_since: Option<u32>,
    /// Until this tick it raids and leaves the Heart alone.
    raid_until: u32,
    /// It has set out on an attack or for the Heart: its opening's first
    /// wave is spent.
    attacked: bool,
    orders: Orders,
    wants: Vec<Want>,
    battle_wants: Vec<Want>,
    /// What it believes of each enemy's army, as of its last think. Only
    /// for the AI lens to show.
    pub belief: Vec<spymaster::Belief>,
    /// How it last weighed its army against each enemy's (-1 to +1), by
    /// owner, and the tick it did. Only for the AI lens to show.
    pub odds: Vec<(u8, f32, u32)>,
    /// The supply it has gathered, the wave it waits for, and whether it
    /// is ready to set out (its wave gathered, or its supply full), as of
    /// its last think. Only for the AI lens to show.
    pub wave: Option<(u32, u32, bool)>,
}

impl Marshal {
    pub fn new(party: PlayerId, reaction_ticks: u32, orders_per_minute: u32) -> Marshal {
        Marshal {
            next_think: 0,
            next_look: 0,
            goal: Goal::Gather,
            rally: None,
            out: Vec::new(),
            set_out_with: 0,
            scouted: Vec::new(),
            battle: None,
            holders: Vec::new(),
            regroup_until: 0,
            contesting_since: None,
            raid_until: 0,
            attacked: false,
            orders: Orders::new(party, reaction_ticks, orders_per_minute),
            wants: Vec::new(),
            battle_wants: Vec::new(),
            belief: Vec::new(),
            odds: Vec::new(),
            wave: None,
        }
    }

    /// The Captain of the battle under way.
    pub fn captain(&self) -> Option<&Captain> {
        self.battle.as_ref().map(|b| &b.captain)
    }

    /// Army orders sent so far.
    pub fn orders(&self) -> &Orders {
        &self.orders
    }

    /// Look, decide, and append the army's orders due now to `out`.
    pub fn tick(
        &mut self,
        c: &mut Ctx,
        s: &Sense,
        home: Option<Vec2>,
        out: &mut Vec<gc_sim::PlayerCommand>,
    ) {
        let party = s.party;
        let view = s.view;
        let due = s.now >= self.next_look
            || s.now >= self.next_think
            || self.battle.as_ref().is_some_and(|b| s.now >= b.next_think);
        if !due {
            self.orders.release(s.now, out);
            return;
        }
        self.next_look = s.now + LOOK_TICKS;
        if self.rally.is_none()
            && let Some(home) = home
        {
            self.rally = Some(rally_point(s, home));
        }
        let army: Vec<&Seen> = view
            .regiments
            .iter()
            .filter(|r| r.owner == party && s.kit.is_fighter(r.unit) && !r.broken())
            .collect();
        self.out.retain(|h| army.iter().any(|r| r.handle == *h));
        if army.is_empty() {
            self.battle = None;
        } else {
            self.battle(c, s, &army);
        }
        if s.now >= self.next_think {
            self.next_think = s.now + THINK_TICKS;
            self.wants.clear();
            self.decide(c, s, &army, home);
            // The Captain has those in the battle (in a lull too: the army
            // waits a moment to see if the enemy comes back).
            if let Some(b) = self.battle.as_ref() {
                self.wants.retain(|w| b.view.get(w.regiment).is_none());
            }
            self.orders.decide(view, &mut self.wants);
        }
        self.orders.release(s.now, out);
    }

    /// Start, run or end a battle where enemies come near the army.
    fn battle(&mut self, c: &mut Ctx, s: &Sense, army: &[&Seen]) {
        let view = s.view;
        let party = s.party;
        let near = |p: Vec2| army.iter().any(|r| r.pos.distance(p) <= ENGAGE);
        let mut foes: Vec<&Seen> = view
            .regiments
            .iter()
            .filter(|r| is_foe(s, r) && !r.broken() && near(r.pos))
            .collect();
        // One battle at a time: the enemies about the one nearest the army.
        let mid = centroid(army);
        if let Some(first) = foes
            .iter()
            .min_by(|a, b| a.pos.distance(mid).total_cmp(&b.pos.distance(mid)))
            .map(|r| r.pos)
        {
            foes.retain(|r| r.pos.distance(first) <= BATTLE_REACH);
        }
        if self.battle.is_none() {
            if foes.is_empty() {
                return;
            }
            c.journal
                .note(s.now, "Enemies close: forming for battle".to_string());
            self.battle = Some(Battle {
                captain: Captain::new(),
                view: Perception::new(),
                at: centroid(&foes),
                quiet_since: None,
                next_think: s.now,
            });
        }
        let Some(b) = self.battle.as_mut() else {
            return;
        };
        b.captain.press(!matches!(self.goal, Goal::Gather));
        if foes.is_empty() {
            b.quiet_since.get_or_insert(s.now);
        } else {
            b.quiet_since = None;
            b.at = centroid(&foes);
        }
        if b.quiet_since.is_some_and(|t| s.now >= t + QUIET_TICKS) {
            c.journal
                .note(s.now, "No enemy near: marching on".to_string());
            self.battle = None;
            self.next_think = 0;
            return;
        }
        // Nobody to fight for a moment: it waits to see if they come back.
        if s.now < b.next_think || foes.is_empty() {
            return;
        }
        b.next_think = s.now + c.knobs.think_ticks.max(1);
        // The regiments of the army about the enemy, and whoever else
        // stands there; the rest of the army goes on as the Marshal says.
        let at = b.at;
        let holders = &self.holders;
        view.subset(&mut b.view, |r| {
            let fights = if r.owner == party {
                s.kit.is_fighter(r.unit) && !holders.contains(&r.handle)
            } else {
                !s.kit.all_workers.contains(&r.unit)
            };
            fights && r.pos.distance(at) <= BATTLE_REACH
        });
        self.battle_wants.clear();
        // Every army in a Conquest battle is about this one fight: the team
        // rules for a field of several armies (letting two others fight
        // first) are for a Field Battle.
        let knobs = Knobs {
            team_play: false,
            ..*c.knobs
        };
        let mut ctx = Ctx {
            view: &b.view,
            terrain: c.terrain,
            party,
            knobs: &knobs,
            persona: c.persona,
            rng: &mut *c.rng,
            journal: &mut *c.journal,
            next_slip: &mut *c.next_slip,
        };
        b.captain.think(&mut ctx, &mut self.battle_wants);
        self.orders.decide(&b.view, &mut self.battle_wants);
    }

    /// Choose the army's goal and march it there.
    fn decide(&mut self, c: &mut Ctx, s: &Sense, army: &[&Seen], home: Option<Vec2>) {
        let Some(rally) = self.rally else { return };
        // It sets out once its wave is gathered, or with what it has when
        // its supply is full and waiting gains nothing.
        let have = s.kit.supply_of(army.iter().copied());
        let want = self.wave_size(s, c.persona);
        let ready = have >= want || (capped(s) && have >= FIRST_WAVE);
        let enough = have >= FIRST_WAVE;
        self.belief = spymaster::beliefs(s);
        self.wave = Some((have.round() as u32, want.ceil() as u32, ready));
        // Armies brawling on the Heart and nobody holding it long is a
        // stalemate (games of four or more): an army that has fought for it
        // a long while goes to break a base instead, or to find one on a
        // big map where it has seen none, and leaves the Heart be until it
        // is back.
        if we_hold_heart(s) || !s.world.heart().is_some_and(|h| h.open) {
            self.contesting_since = None;
        }
        if ready
            && s.now >= self.regroup_until
            && self
                .contesting_since
                .is_some_and(|t| s.now >= t + HEART_PATIENCE)
            && let Some((t, note)) = raid_target(s, centroid(army))
                .map(|t| (t, "going to break a base"))
                .or_else(|| {
                    self.target(s, centroid(army))
                        .map(|t| (t, "going to find a base"))
                })
        {
            c.journal
                .note(s.now, format!("The Heart is a stalemate: {note}"));
            self.contesting_since = None;
            self.raid_until = s.now + RAID_TICKS;
            self.goal = Goal::Attack(t);
            self.out = army.iter().map(|r| r.handle).collect();
            self.set_out_with = army.iter().map(|r| u32::from(r.models)).sum();
        }
        let raiding = s.now < self.raid_until;
        let heart_to_take = |s: &Sense| heart_to_take(s).filter(|_| !raiding);
        let enemy_holds_heart = |s: &Sense| enemy_holds_heart(s) && !raiding;
        // Home first, but a threat to outlying buildings doesn't pull the
        // army off the Titan's Heart: a game of armies guarding sprawling
        // bases never ends otherwise.
        let threat = threat_to_home(s, army, home).filter(|&(_, at_home)| {
            at_home
                || heart_to_take(s).is_none() && !we_hold_heart(s)
                || !(matches!(self.goal, Goal::Heart(_)) || enemy_holds_heart(s) || ready)
        });
        if let Some((threat, _)) = threat {
            if !matches!(self.goal, Goal::Defend(_)) {
                c.journal.note(
                    s.now,
                    "Enemies at our buildings: going to defend".to_string(),
                );
            }
            self.goal = Goal::Defend(threat);
            self.out = army.iter().map(|r| r.handle).collect();
        } else if matches!(self.goal, Goal::Defend(_)) {
            self.goal = Goal::Gather;
            self.out.clear();
        }
        let soldiers = |hs: &[RegimentHandle]| -> u32 {
            hs.iter()
                .filter_map(|h| s.view.get(*h))
                .map(|r| u32::from(r.models))
                .sum()
        };
        match self.goal {
            Goal::Gather => {
                // Beaten back, it gathers a while before going again; and it
                // goes to stop an enemy on the Heart only with an army.
                let regrouped = s.now >= self.regroup_until;
                let heart = heart_to_take(s)
                    .filter(|_| regrouped && (ready || enemy_holds_heart(s) && enough));
                let ready = ready && regrouped;
                let set_out = match heart {
                    Some(h) => Some((
                        Goal::Heart(h),
                        format!(
                            "Going to take the Titan's Heart with {} regiments",
                            army.len()
                        ),
                    )),
                    None if regrouped => self
                        .attack_plan(s, c.persona, army, ready, enough)
                        .map(|(t, note)| (Goal::Attack(t), note)),
                    None => None,
                };
                if let Some((goal, note)) = set_out {
                    self.out = army.iter().map(|r| r.handle).collect();
                    self.set_out_with = soldiers(&self.out);
                    if matches!(goal, Goal::Heart(_)) {
                        self.contesting_since.get_or_insert(s.now);
                    }
                    self.attacked = true;
                    self.goal = goal;
                    c.journal.note(s.now, note);
                }
            }
            Goal::Attack(_) | Goal::Heart(_) => {
                let left = soldiers(&self.out);
                if (left as f32) < FALL_BACK_SHARE * self.set_out_with as f32 {
                    c.journal.note(
                        s.now,
                        "The attack has cost too much: falling back".to_string(),
                    );
                    self.goal = Goal::Gather;
                    self.out.clear();
                    self.regroup_until = s.now + REGROUP_TICKS;
                } else if let Some(heart) = heart_to_take(s).filter(|_| {
                    // An enemy holding the Heart wins unless it is stopped;
                    // otherwise an attack under way goes on.
                    matches!(self.goal, Goal::Heart(_)) || enemy_holds_heart(s)
                }) {
                    if !matches!(self.goal, Goal::Heart(_)) {
                        c.journal.note(
                            s.now,
                            "They hold the Heart: going to take it back".to_string(),
                        );
                    }
                    self.contesting_since.get_or_insert(s.now);
                    self.goal = Goal::Heart(heart);
                } else if matches!(self.goal, Goal::Heart(_)) {
                    // Ours now, or shut: back to gathering.
                    if !we_hold_heart(s) {
                        self.goal = Goal::Gather;
                        self.out.clear();
                    }
                } else {
                    let mid = centroid_of(s.view, &self.out).unwrap_or(rally);
                    let t = raiding
                        .then(|| raid_target(s, mid))
                        .flatten()
                        .or_else(|| self.target(s, mid));
                    match t {
                        Some(t) => self.goal = Goal::Attack(t),
                        None => {
                            c.journal
                                .note(s.now, "Nothing left to find: coming home".to_string());
                            self.goal = Goal::Gather;
                            self.out.clear();
                            self.scouted.clear();
                        }
                    }
                }
            }
            Goal::Defend(_) => {}
        }
        // Those out march on the goal; the rest wait at the rally point,
        // and join an attack once there are enough of them.
        let waiting: Vec<&Seen> = army
            .iter()
            .copied()
            .filter(|r| !self.out.contains(&r.handle))
            .collect();
        if matches!(self.goal, Goal::Attack(_) | Goal::Heart(_))
            && waiting.len() >= REINFORCE
            && waiting.iter().all(|r| r.pos.distance(rally) <= 20.0)
        {
            self.out.extend(waiting.iter().map(|r| r.handle));
            self.set_out_with += waiting.iter().map(|r| u32::from(r.models)).sum::<u32>();
        }
        let (going, staying): (Vec<&Seen>, Vec<&Seen>) =
            army.iter().partition(|r| self.out.contains(&r.handle));
        let face = home.map_or(Vec2::Y, |h| (rally - h).normalize_or(Vec2::Y));
        march(s, &staying, rally, face, &mut self.wants, 1);
        if !matches!(self.goal, Goal::Heart(_)) {
            self.holders.clear();
        }
        match self.goal {
            Goal::Gather => {}
            Goal::Defend(at) => {
                let mid = centroid(&going);
                march(
                    s,
                    &going,
                    at,
                    (at - mid).normalize_or(face),
                    &mut self.wants,
                    2,
                );
            }
            Goal::Heart(at) => self.hold_heart(s, &going, at),
            Goal::Attack(at) => {
                let mid = centroid(&going);
                // There once its middle is, or its van: a big army crowds
                // round a base in a corner and its middle may never get
                // close.
                let there = mid.distance(at) <= AT_TARGET
                    || going.iter().any(|r| r.pos.distance(at) <= AT_TARGET / 2.0);
                if !there {
                    march(
                        s,
                        &going,
                        at,
                        (at - mid).normalize_or(face),
                        &mut self.wants,
                        3,
                    );
                } else {
                    self.knock_down(s, &going, at, s.now < self.raid_until);
                }
            }
        }
    }

    /// Where to attack: the enemy building it knows of nearest `from`,
    /// else the nearest enemy start it hasn't looked at.
    fn target(&mut self, s: &Sense, from: Vec2) -> Option<Vec2> {
        let seen = enemy_buildings(s);
        if let Some(b) = seen
            .iter()
            .min_by(|a, b| a.at.distance(from).total_cmp(&b.at.distance(from)))
        {
            return Some(b.at);
        }
        let home = s.view.regiments.iter().filter(|r| r.owner == s.party);
        // Look at starts as regiments pass them.
        for (i, &st) in s.kit.starts.iter().enumerate() {
            if !self.scouted.contains(&i) && home.clone().any(|r| r.pos.distance(st) <= SCOUTED) {
                self.scouted.push(i);
            }
        }
        // Its own and its friends' starts have their buildings on them.
        let friendly: Vec<Vec2> = s.world.structures().map_or(Vec::new(), |st| {
            st.iter()
                .filter(|v| !s.view.hostile(s.party, v.owner))
                .map(|v| v2(v.centre()))
                .collect()
        });
        s.kit
            .starts
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.scouted.contains(i))
            .map(|(_, &p)| p)
            .filter(|p| friendly.iter().all(|f| f.distance(*p) > SCOUTED * 2.0))
            .min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)))
    }

    /// Where to attack now, if anywhere, and the journal line saying why.
    /// A level that weighs the odds goes for the enemy it has the best odds
    /// against, and sets out only when the odds are good enough for its
    /// caution (or its supply is full, and waiting gains nothing); with
    /// odds that good it goes before its wave is full. The others go when
    /// the wave is `ready`, at the nearest enemy building.
    fn attack_plan(
        &mut self,
        s: &Sense,
        persona: &Personality,
        army: &[&Seen],
        ready: bool,
        enough: bool,
    ) -> Option<(Vec2, String)> {
        let from = centroid(army);
        let full = capped(s);
        if !ready && !enough {
            return None;
        }
        let odds = if s.plan.weighs_odds {
            self.best_odds(s, army, from)
        } else {
            None
        };
        let Some((t, margin)) = odds else {
            // It doesn't weigh odds, or knows nothing of theirs to weigh:
            // the nearest building it knows of, or look for them.
            return ready
                .then(|| self.target(s, from))
                .flatten()
                .map(|t| (t, format!("Attacking with {} regiments", army.len())));
        };
        let needed = ODDS_NEEDED + ODDS_CAUTION * persona.caution();
        let go = ready && (margin >= needed || full) || enough && margin >= ODDS_STRIKE;
        go.then(|| {
            (
                t,
                format!("Attacking with {} regiments: odds {margin:+.2}", army.len()),
            )
        })
    }

    /// The enemy building to attack with the best odds: for each enemy it
    /// knows a building of, the nearest such building, scored by how a
    /// fight against the army defending it would go, less a little for
    /// distance.
    fn best_odds(&mut self, s: &Sense, army: &[&Seen], from: Vec2) -> Option<(Vec2, f32)> {
        let mut nearest: Vec<(u8, Vec2)> = Vec::new();
        // Only enemies whose army it has seen: odds against an army it
        // knows nothing of would be a guess.
        for b in enemy_buildings(s) {
            if spymaster::army_of(s, b.owner).next().is_none() {
                continue;
            }
            match nearest.iter_mut().find(|(o, _)| *o == b.owner) {
                Some((_, at)) if at.distance(from) > b.at.distance(from) => *at = b.at,
                Some(_) => {}
                None => nearest.push((b.owner, b.at)),
            }
        }
        let weighed: Vec<(u8, Vec2, f32)> = nearest
            .into_iter()
            .map(|(o, at)| {
                let margin =
                    spymaster::odds(s, army.iter().copied(), spymaster::defenders(s, o, at));
                (o, at, margin)
            })
            .collect();
        // Keep the odds for the lens.
        for &(o, _, margin) in &weighed {
            match self.odds.iter_mut().find(|(owner, ..)| *owner == o) {
                Some(kept) => *kept = (o, margin, s.now),
                None => self.odds.push((o, margin, s.now)),
            }
        }
        weighed
            .into_iter()
            .map(|(_, at, margin)| (at, margin, margin - at.distance(from) / ODDS_PER_DISTANCE))
            .max_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(at, margin, _)| (at, margin))
    }

    /// The supply it waits for before setting out: the usual wave, or its
    /// opening's smaller first wave until it has attacked once.
    fn wave_size(&self, s: &Sense, persona: &Personality) -> f32 {
        let usual = usual_wave(s, persona);
        match s.plan.early.first_wave.filter(|_| !self.attacked) {
            Some(w) => usual.min(f32::from(w)),
            None => usual,
        }
    }

    /// What the army is waiting for or doing beyond what its goal says, in
    /// a few words, for the AI lens.
    pub fn plan_note(&self, now: u32) -> Option<String> {
        let wave = self.wave;
        match self.goal {
            Goal::Gather if now < self.regroup_until => Some(format!(
                "regrouping after a costly attack, {}s more",
                (self.regroup_until - now).div_ceil(TICKS_PER_SECOND)
            )),
            Goal::Gather => wave.map(|(have, want, ready)| {
                if ready && have < want {
                    format!("supply is full at {have}: waiting for a target or better odds")
                } else if ready {
                    format!("wave of {want} supply is ready: waiting for a target or better odds")
                } else {
                    format!("gathering until its wave is ready: {have} of {want} supply")
                }
            }),
            Goal::Attack(_) if now < self.raid_until => {
                Some("raiding enemy main buildings, leaving the Heart alone".to_string())
            }
            Goal::Heart(_) => self.contesting_since.map(|t| {
                format!(
                    "has fought for the Heart for {} min",
                    (now - t) / (60 * TICKS_PER_SECOND)
                )
            }),
            _ => None,
        }
    }

    /// At the target: every regiment goes for one of the nearest enemy
    /// buildings, towers first, spread over the few nearest.
    /// On a raid, main buildings next.
    fn knock_down(&mut self, s: &Sense, going: &[&Seen], at: Vec2, raiding: bool) {
        let mut targets = enemy_buildings(s);
        targets.retain(|t| t.at.distance(at) <= AT_TARGET * 2.0);
        if targets.is_empty() {
            return;
        }
        targets.sort_by(|a, b| {
            let key = |t: &Sighted| {
                (
                    t.role != Role::Defence,
                    raiding && t.role != Role::Main,
                    t.at.distance(at) as i32,
                )
            };
            key(a).cmp(&key(b))
        });
        targets.truncate(3);
        for (k, r) in going.iter().enumerate() {
            if r.engaged {
                continue;
            }
            // Keep at one it has already been sent to while it stands; one
            // that has stopped may be sent again.
            let current = s.world.siege_target(r.handle);
            if current.is_none() {
                self.orders.forget_landed(r.handle, s.now);
            }
            let t = match current.filter(|h| targets.iter().any(|t| t.handle == *h)) {
                Some(h) => h,
                None => targets[k % targets.len()].handle,
            };
            self.wants.push(Want {
                regiment: r.handle,
                act: Act::Siege(t),
                priority: 2,
                group: 0,
            });
        }
    }

    /// Stand on the Heart: the few nearest it close round its middle, the
    /// rest in a ring further out facing away from it.
    fn hold_heart(&mut self, s: &Sense, going: &[&Seen], at: Vec2) {
        // Holders keep their places; the nearest others fill any gaps.
        self.holders
            .retain(|h| going.iter().any(|r| r.handle == *h));
        let mut rest: Vec<&Seen> = going
            .iter()
            .copied()
            .filter(|r| !self.holders.contains(&r.handle))
            .collect();
        rest.sort_by(|a, b| a.pos.distance(at).total_cmp(&b.pos.distance(at)));
        let gaps = HEART_HOLDERS.saturating_sub(self.holders.len());
        self.holders
            .extend(rest.iter().take(gaps).map(|r| r.handle));
        let order = self
            .holders
            .iter()
            .filter_map(|h| going.iter().find(|r| r.handle == *h))
            .chain(rest.iter().skip(gaps));
        let n = going.len();
        for (k, r) in order.enumerate() {
            let (radius, i, of) = if k < HEART_HOLDERS {
                (HEART_RING, k, n.min(HEART_HOLDERS))
            } else {
                (HEART_SCREEN, k - HEART_HOLDERS, n - HEART_HOLDERS)
            };
            let angle = std::f32::consts::TAU * i as f32 / of.max(1) as f32;
            let out = Vec2::from_angle(angle);
            let spot = s.terrain.clamp(at + out * radius, 2.0);
            self.wants.push(Want {
                regiment: r.handle,
                act: Act::Move {
                    to: spot,
                    facing: Some(out),
                    pace: 0.0,
                },
                priority: 2,
                group: 0,
            });
        }
    }
}

/// The supply in fighters it waits for before setting out: a wave that
/// grows with the game and shrinks with aggression.
fn usual_wave(s: &Sense, persona: &Personality) -> f32 {
    let minutes = s.now as f32 / (60.0 * TICKS_PER_SECOND as f32);
    let boldness = 1.3 - 0.6 * persona.aggression();
    ((FIRST_WAVE + PER_MINUTE * minutes) * boldness).min(BIGGEST_WAVE)
}

/// Whether its supply is used up to within a regiment or so of its cap:
/// waiting for a bigger army gains nothing.
fn capped(s: &Sense) -> bool {
    s.world
        .bank(s.party)
        .is_some_and(|b| b.supply_used + CAP_SLACK >= b.supply_cap.clamp(1, SUPPLY_LIMIT))
}

/// Where to raid: the nearest enemy main building it has seen, as
/// breaking those is what wins.
fn raid_target(s: &Sense, from: Vec2) -> Option<Vec2> {
    enemy_buildings(s)
        .into_iter()
        .filter(|b| b.role == Role::Main)
        .map(|b| b.at)
        .min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)))
}

/// The Titan's Heart, if it is open and not held by its own side.
fn heart_to_take(s: &Sense) -> Option<Vec2> {
    let h = s.world.heart().filter(|h| h.open)?;
    (heart_held_by_enemy(s) != Some(false)).then(|| v2(h.at))
}

/// Who holds the Heart: `Some(true)` an enemy, `Some(false)` its own side
/// or a friend, `None` nobody.
fn heart_held_by_enemy(s: &Sense) -> Option<bool> {
    let (p, _) = s.world.heart()?.held?;
    Some(s.view.hostile(s.party, p))
}

fn enemy_holds_heart(s: &Sense) -> bool {
    heart_held_by_enemy(s) == Some(true)
}

fn we_hold_heart(s: &Sense) -> bool {
    heart_held_by_enemy(s) == Some(false)
}

/// An enemy regiment worth fighting: not a worker.
fn is_foe(s: &Sense, r: &Seen) -> bool {
    s.view.hostile(s.party, r.owner) && !s.kit.all_workers.contains(&r.unit)
}

/// The rally point: in front of home, towards the nearest other start.
fn rally_point(s: &Sense, home: Vec2) -> Vec2 {
    let toward = s
        .kit
        .starts
        .iter()
        .filter(|p| p.distance(home) > 30.0)
        .min_by(|a, b| a.distance(home).total_cmp(&b.distance(home)))
        .copied()
        .unwrap_or(home + Vec2::Y);
    let p = home + (toward - home).normalize_or(Vec2::Y) * RALLY_OUT;
    s.terrain.clamp(p, 4.0)
}

/// An enemy building its side has seen and still stands.
struct Sighted {
    handle: StructureHandle,
    at: Vec2,
    role: Role,
    owner: u8,
}

/// Every enemy building its side has seen and still stands.
fn enemy_buildings(s: &Sense) -> Vec<Sighted> {
    let Some(st) = s.world.structures() else {
        return Vec::new();
    };
    let table = st.buildings();
    st.iter()
        .filter(|v| s.view.hostile(s.party, v.owner) && v.seen_by(s.party))
        .filter_map(|v| {
            let role = table.get(usize::from(v.building))?.role;
            Some(Sighted {
                handle: v.handle,
                at: v2(v.centre()),
                role,
                owner: v.owner,
            })
        })
        .collect()
}

/// The middle of the enemies it sees at its buildings, if they are a
/// threat worth the whole army: at a main building, or a good part of the
/// army's strength. A few passing by an outlying building are left to the
/// buildings and the regiments at the rally point.
fn threat_to_home(s: &Sense, army: &[&Seen], home: Option<Vec2>) -> Option<(Vec2, bool)> {
    let st = s.world.structures()?;
    // Each of its buildings, and whether it is at home (an expansion
    // hall, even the Brood's Hive, is not).
    let mine: Vec<(Vec2, bool)> = st
        .iter()
        .filter(|v| v.owner == s.party)
        .map(|v| {
            let at = v2(v.centre());
            (at, home.is_some_and(|h| h.distance(at) <= THREAT))
        })
        .collect();
    let mut at_home = false;
    let near: Vec<&Seen> = s
        .view
        .regiments
        .iter()
        .filter(|r| is_foe(s, r) && !r.broken())
        .filter(|r| {
            let mut close = false;
            for &(b, home) in &mine {
                if b.distance(r.pos) <= THREAT {
                    close = true;
                    at_home |= home;
                }
            }
            close
        })
        .collect();
    let theirs: u32 = near.iter().map(|r| u32::from(r.models)).sum();
    let ours: u32 = army.iter().map(|r| u32::from(r.models)).sum();
    let serious = at_home || theirs as f32 >= THREAT_SHARE * ours as f32;
    (!near.is_empty() && serious).then(|| (centroid(&near), at_home))
}

/// March `regiments` abreast to `to`, facing `face`, at the pace of the
/// slowest.
fn march(s: &Sense, regiments: &[&Seen], to: Vec2, face: Vec2, wants: &mut Vec<Want>, group: u32) {
    if regiments.is_empty() {
        return;
    }
    let pace = regiments
        .iter()
        .map(|r| s.view.pace(r))
        .fold(f32::MAX, f32::min);
    let right = right_of(face);
    let n = regiments.len() as f32;
    // Keep each regiment's place in the line by how far right it stands.
    let mut order: Vec<&&Seen> = regiments.iter().collect();
    order.sort_by(|a, b| a.pos.dot(right).total_cmp(&b.pos.dot(right)));
    for (i, r) in order.into_iter().enumerate() {
        let across = (i as f32 - (n - 1.0) / 2.0) * ABREAST;
        let spot = s.terrain.clamp(to + right * across, 2.0);
        wants.push(Want {
            regiment: r.handle,
            act: Act::Move {
                to: spot,
                facing: Some(face),
                pace,
            },
            priority: ROUTINE,
            group: u32::MAX - group,
        });
    }
}

fn centroid(rs: &[&Seen]) -> Vec2 {
    crate::captain::helpers::centroid(rs)
}

fn centroid_of(view: &Perception, hs: &[RegimentHandle]) -> Option<Vec2> {
    let rs: Vec<&Seen> = hs.iter().filter_map(|h| view.get(*h)).collect();
    (!rs.is_empty()).then(|| centroid(&rs))
}
