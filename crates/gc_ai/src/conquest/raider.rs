//! The Raider: a small, fast raiding party the Marshal lends out to
//! harass an enemy's economy (docs/18-ai.md, "Marshal"). Once the army is
//! big enough to spare it, the two fastest regiments waiting at the rally
//! point go for the enemy workers they have seen, those of the enemy the
//! Sovereign is going for first, and away from towers and soldiers. They
//! stay while they are killing workers, run home when enemy fighters
//! outweigh them or half of them are lost, rest, and go again. Like the
//! scout they are not part of the army while they are out (the Marshal
//! leaves them out of the army, its battles and its waves), and the army
//! has them back when it must defend home or is going on a timing push.

use gc_sim::structures::Role;
use gc_sim::{RegimentHandle, TICKS_PER_SECOND};
use glam::Vec2;

use super::{Sense, spymaster};
use crate::orders::{Act, ROUTINE, Want, attack_group};
use crate::perception::Seen;

/// Fighting regiments the army has (those out and those waiting) before a
/// party is taken from it, and how many regiments the party takes, all of
/// them from the ones waiting at the rally point.
const MIN_ARMY: usize = 6;
const PARTY: usize = 2;
/// No party is sent before this tick.
const FIRST_RAID: u32 = 5 * 60 * TICKS_PER_SECOND;
/// Ticks it rests at the rally point after a retreat, and the ticks before
/// a new party is taken once one has ended.
const REST: u32 = 45 * TICKS_PER_SECOND;
const COOLDOWN: u32 = 90 * TICKS_PER_SECOND;
/// They fall back once they are down to this share of the soldiers they
/// set out with.
const LOST_SHARE: f32 = 0.5;
/// They go back to the army for good if they reach the rally point with
/// less than this share of the soldiers they were first taken with.
const KEEP_SHARE: f32 = 0.5;
/// Workers in sight this near the party are attacked.
const CHASE: f32 = 40.0;
/// A place this much further from an enemy tower than its range is still
/// covered by it (workers run, raiders chase).
const TOWER_MARGIN: f32 = 4.0;
/// A place this near an enemy fighter it knows of is guarded.
const GUARD_REACH: f32 = 22.0;
/// A worker it remembers is not worth a trip once a raider has come this
/// near where it was and found it gone.
const LOOKED: f32 = 12.0;
/// How long a worker last seen out of sight is still worth a trip.
const MEMORY: u32 = 180 * TICKS_PER_SECOND;
/// A hall this near a start is that start's main: not an expansion.
const START_REACH: f32 = 20.0;
/// Ticks it leaves a place alone after it was driven off from it or found
/// nothing there, and how far round it that is.
const AVOID: u32 = 90 * TICKS_PER_SECOND;
const AVOID_NEAR: f32 = 25.0;
/// They are back at the rally point within this many tiles.
const HOME: f32 = 12.0;
/// The longest the run home is given before they count as home.
const HOME_LIMIT: u32 = 90 * TICKS_PER_SECOND;
/// The group of the raiders' moves (the army's are `u32::MAX - 1` to `- 3`,
/// the scout's `- 9`).
const GROUP: u32 = u32::MAX - 10;

/// A place worth a raid: where an enemy's workers are, or an expansion
/// hall with workers about it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub at: Vec2,
    pub owner: u8,
    /// Workers seen or remembered there (else an expansion hall).
    pub workers: bool,
}

impl Spot {
    /// What is there, in a word.
    fn what(self) -> &'static str {
        if self.workers { "workers" } else { "expansion" }
    }
}

/// Where the party is in its raid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    /// No party out.
    #[default]
    Idle,
    /// Going for workers, or at them.
    Out,
    /// Running home, until the tick given at the latest.
    Falling(u32),
    /// Resting at the rally point until the tick given.
    Resting(u32),
}

#[derive(Default)]
pub struct Raider {
    regiments: Vec<RegimentHandle>,
    phase: Phase,
    /// Soldiers at the first taking, and at the last setting out.
    original: u32,
    started_with: u32,
    target: Option<Spot>,
    /// The way to its target is given up on at this tick.
    trip_ends: u32,
    /// Places left alone for a while.
    avoid: Vec<(Vec2, u32)>,
    /// No new party is taken before this tick.
    next_party: u32,
    rally: Option<Vec2>,
    /// Worker regiments seen near the party and not yet counted as gone.
    watching: Vec<RegimentHandle>,
    /// The workers in sight it goes for, as of its last update.
    prey: Vec<RegimentHandle>,
    /// Raids set out so far, and worker regiments seen go while a party
    /// was near them (for the tests).
    raids: u32,
    kills: u32,
}

impl Raider {
    /// The regiments it has taken, if any: the army leaves them alone.
    pub fn regiments(&self) -> &[RegimentHandle] {
        &self.regiments
    }

    /// Raids set out so far (for the tests).
    pub fn raids(&self) -> u32 {
        self.raids
    }

    /// Enemy worker regiments seen to go while the party was near them
    /// (for the tests).
    pub fn kills(&self) -> u32 {
        self.kills
    }

    /// Keep the party and its plan up to date and say what is new for the
    /// journal. `army` is how many fighting regiments the side has (the
    /// scout aside), `waiting` those of them standing at the rally point,
    /// free to go; `recall` is set when the army needs them back.
    pub fn update(
        &mut self,
        s: &Sense,
        army: usize,
        waiting: &[&Seen],
        rally: Option<Vec2>,
        recall: bool,
        name: impl Fn(u8) -> String,
    ) -> Vec<String> {
        let mut notes = Vec::new();
        if !s.plan.raids {
            return notes;
        }
        self.rally = rally.or(self.rally);
        self.prey.clear();
        self.avoid.retain(|&(_, until)| s.now < until);
        self.count_kills(s);
        self.regiments.retain(|h| s.view.get(*h).is_some());
        if self.regiments.is_empty() {
            if self.phase != Phase::Idle {
                notes.push("The raiding party is lost".to_string());
                self.end(s.now);
            }
        } else if recall {
            notes.push("Raiders called back to the army".to_string());
            self.end(s.now);
        }
        match self.phase {
            Phase::Idle => self.take(s, army, waiting, recall, &name, &mut notes),
            Phase::Out => self.out(s, &name, &mut notes),
            Phase::Falling(limit) => self.fall_back(s, limit, &mut notes),
            Phase::Resting(until) => self.rest(s, until, &name, &mut notes),
        }
        notes
    }

    /// The party is over: its regiments go back to the army, and no new
    /// party is taken for a while.
    fn end(&mut self, now: u32) {
        self.regiments.clear();
        self.phase = Phase::Idle;
        self.target = None;
        self.watching.clear();
        self.next_party = now + COOLDOWN;
    }

    /// Take the fastest regiments waiting at the rally point, if there is
    /// an army to spare them from, it is time, and there is somewhere to
    /// go.
    fn take(
        &mut self,
        s: &Sense,
        army: usize,
        waiting: &[&Seen],
        recall: bool,
        name: &impl Fn(u8) -> String,
        notes: &mut Vec<String>,
    ) {
        if recall || s.now < FIRST_RAID || s.now < self.next_party {
            return;
        }
        if waiting.len() < PARTY || army < MIN_ARMY {
            return;
        }
        let mut free: Vec<&Seen> = waiting.to_vec();
        let from = self.rally.unwrap_or(free[0].pos);
        let Some(target) = self.choose(s, from) else {
            return;
        };
        free.sort_by(|a, b| s.view.pace(b).total_cmp(&s.view.pace(a)));
        free.truncate(PARTY);
        self.regiments = free.iter().map(|r| r.handle).collect();
        self.original = free.iter().map(|r| u32::from(r.models)).sum();
        self.set_out(s, target, name, notes);
    }

    /// Set out on a raid on `target` with the regiments it has.
    fn set_out(
        &mut self,
        s: &Sense,
        target: Spot,
        name: &impl Fn(u8) -> String,
        notes: &mut Vec<String>,
    ) {
        self.phase = Phase::Out;
        self.raids += 1;
        self.started_with = self.soldiers(s);
        self.go_to(s, target);
        notes.push(format!(
            "Raiding {} {} with {} regiment{}",
            name(target.owner),
            target.what(),
            self.regiments.len(),
            if self.regiments.len() == 1 { "" } else { "s" }
        ));
    }

    /// Head for `target`, giving up on the way there after a while.
    fn go_to(&mut self, s: &Sense, target: Spot) {
        let mid = self.middle(s).unwrap_or(target.at);
        let pace = self
            .members(s)
            .map(|r| s.view.pace(r))
            .fold(f32::MAX, f32::min)
            .clamp(0.5, 50.0);
        self.target = Some(target);
        self.trip_ends =
            s.now + ((mid.distance(target.at) / pace) * 2.0 + 30.0) as u32 * TICKS_PER_SECOND;
    }

    /// Going for workers: keep on while there are some, choose where next
    /// when there are none, and fall back when it must.
    fn out(&mut self, s: &Sense, name: &impl Fn(u8) -> String, notes: &mut Vec<String>) {
        let Some(mid) = self.middle(s) else { return };
        // Soldiers lost, or enemy fighters about: run home.
        if (self.soldiers(s) as f32) <= LOST_SHARE * self.started_with as f32 {
            notes.push("The raid has cost too much: raiders falling back".to_string());
            self.fall_back_now(s, true);
            return;
        }
        if self.in_danger(s) {
            notes.push("Enemy fighters too strong: raiders falling back".to_string());
            self.fall_back_now(s, true);
            return;
        }
        let prey = self.prey_in_sight(s, mid);
        for r in &prey {
            if !self.watching.contains(&r.handle) && self.watching.len() < 32 {
                self.watching.push(r.handle);
            }
        }
        self.prey = prey.iter().map(|r| r.handle).collect();
        // Where it was going: reached with no workers about, or given up
        // on. Workers in sight are taken wherever they are.
        if let Some(t) = self.target
            && prey.is_empty()
        {
            let reached = self.members(s).any(|r| r.pos.distance(t.at) <= LOOKED);
            if reached || s.now >= self.trip_ends {
                self.avoid.push((t.at, s.now + AVOID / 2));
                self.target = None;
            }
        }
        if self.target.is_none() && prey.is_empty() {
            match self.choose(s, mid) {
                Some(t) => {
                    notes.push(format!(
                        "Raiders moving on to {} {}",
                        name(t.owner),
                        t.what()
                    ));
                    self.go_to(s, t);
                }
                None => {
                    notes.push("No workers left to raid: raiders falling back".to_string());
                    self.fall_back_now(s, false);
                }
            }
        }
    }

    /// Start the run home; if the enemy drove them off the place they
    /// were going to, it is left alone for a while.
    fn fall_back_now(&mut self, s: &Sense, driven_off: bool) {
        if driven_off && let Some(t) = self.target {
            self.avoid.push((t.at, s.now + AVOID));
        }
        if driven_off && let Some(mid) = self.middle(s) {
            self.avoid.push((mid, s.now + AVOID));
        }
        self.target = None;
        // Workers that go from now on were not its doing.
        self.watching.clear();
        self.phase = Phase::Falling(s.now + HOME_LIMIT);
    }

    /// On the run home: at the rally point they rest, or go back to the
    /// army for good if they are too few.
    fn fall_back(&mut self, s: &Sense, limit: u32, notes: &mut Vec<String>) {
        let home = self
            .rally
            .is_none_or(|rally| self.members(s).all(|r| r.pos.distance(rally) <= HOME));
        if !home && s.now < limit {
            return;
        }
        if (self.soldiers(s) as f32) < KEEP_SHARE * self.original as f32 {
            notes.push("The raiders are too few: back to the army".to_string());
            self.end(s.now);
        } else {
            self.phase = Phase::Resting(s.now + REST);
        }
    }

    /// Resting at the rally point: when rested they go again if there is
    /// somewhere to go, else go back to the army.
    fn rest(
        &mut self,
        s: &Sense,
        until: u32,
        name: &impl Fn(u8) -> String,
        notes: &mut Vec<String>,
    ) {
        if s.now < until {
            return;
        }
        let from = self.middle(s).or(self.rally).unwrap_or(Vec2::ZERO);
        match self.choose(s, from) {
            Some(t) => self.set_out(s, t, name, notes),
            None => {
                notes.push("No workers to raid: raiders back to the army".to_string());
                self.end(s.now);
            }
        }
    }

    /// The party's regiments in sight now (its own always are).
    fn members<'a>(&'a self, s: &'a Sense) -> impl Iterator<Item = &'a Seen> + 'a {
        self.regiments.iter().filter_map(|h| s.view.get(*h))
    }

    /// Soldiers in the party now.
    fn soldiers(&self, s: &Sense) -> u32 {
        self.members(s).map(|r| u32::from(r.models)).sum()
    }

    /// The middle of the party.
    fn middle(&self, s: &Sense) -> Option<Vec2> {
        let rs: Vec<&Seen> = self.members(s).collect();
        (!rs.is_empty()).then(|| crate::captain::helpers::centroid(&rs))
    }

    /// Whether enemy fighters it can see near the party outweigh it.
    fn in_danger(&self, s: &Sense) -> bool {
        let mine: Vec<&Seen> = self.members(s).collect();
        spymaster::outmatched(s, &mine).is_some()
    }

    /// Enemy workers in sight within [`CHASE`] of `from` that are worth
    /// attacking: not at a place it was driven from, and those out of
    /// reach of the enemy's towers if there are any.
    fn prey_in_sight<'a>(&self, s: &'a Sense, from: Vec2) -> Vec<&'a Seen> {
        let towers = towers(s);
        let mut prey: Vec<&Seen> = s
            .view
            .regiments
            .iter()
            .filter(|r| {
                s.view.hostile(s.party, r.owner)
                    && !r.broken()
                    && s.kit.all_workers.contains(&r.unit)
                    && r.pos.distance(from) <= CHASE
                    && !self.avoided(r.pos)
            })
            .collect();
        let open = |r: &&Seen| towers.iter().all(|t| !t.covers(r.pos));
        if prey.iter().any(open) {
            prey.retain(|r| open(r));
        }
        prey
    }

    fn avoided(&self, at: Vec2) -> bool {
        self.avoid
            .iter()
            .any(|&(p, _)| p.distance(at) <= AVOID_NEAR)
    }

    /// Count the worker regiments it has seen near the party that are
    /// gone from the field now.
    fn count_kills(&mut self, s: &Sense) {
        let mut gone = 0;
        self.watching.retain(|h| {
            let there = s.view.seen_at(*h).is_some();
            gone += u32::from(!there);
            there
        });
        self.kills += gone;
    }

    /// The best place to raid from `from` now, if any.
    fn choose(&self, s: &Sense, from: Vec2) -> Option<Spot> {
        let mine: Vec<Vec2> = self.members(s).map(|r| r.pos).collect();
        let mut spots: Vec<Spot> = Vec::new();
        // Workers in sight, and those it has seen and lost sight of, unless
        // the party has been by where they were and they are not there.
        let seen = s.view.regiments.iter().filter(|r| {
            s.view.hostile(s.party, r.owner) && !r.broken() && s.kit.all_workers.contains(&r.unit)
        });
        let lost = s.view.last_seen_within(MEMORY).filter(|r| {
            s.kit.all_workers.contains(&r.unit) && mine.iter().all(|m| m.distance(r.pos) > LOOKED)
        });
        for r in seen.chain(lost) {
            spots.push(Spot {
                at: r.pos,
                owner: r.owner,
                workers: true,
            });
        }
        // With no workers known, the halls that are not an enemy's first:
        // the workers are there.
        if spots.is_empty() {
            let table = spymaster::enemy_buildings(s);
            spots.extend(
                table
                    .iter()
                    .filter(|b| is_expansion(b.role, b.at, &s.kit.starts))
                    .map(|b| Spot {
                        at: b.at,
                        owner: b.owner,
                        workers: false,
                    }),
            );
        }
        spots.retain(|p| !self.avoided(p.at));
        let foes: Vec<Vec2> = spymaster::enemy_army(s).map(|r| r.pos).collect();
        choose(&spots, &towers(s), &foes, s.focus, from).map(|i| spots[i])
    }

    /// The orders for the party now, if one is out.
    pub fn steer(&self, s: &Sense, wants: &mut Vec<Want>) {
        let walk = |wants: &mut Vec<Want>, to: Vec2, priority: u8| {
            wants.extend(
                self.regiments
                    .iter()
                    .map(|&h| s.walk(h, to, priority, GROUP)),
            );
        };
        match self.phase {
            Phase::Idle => {}
            Phase::Falling(_) | Phase::Resting(_) => {
                // Running home comes before anything else it was told.
                let priority = ROUTINE + u8::from(matches!(self.phase, Phase::Falling(_)));
                if let Some(rally) = self.rally {
                    walk(wants, rally, priority);
                }
            }
            Phase::Out if !self.prey.is_empty() => {
                // Each goes for the worker nearest it.
                for me in self.members(s) {
                    let nearest = self
                        .prey
                        .iter()
                        .filter_map(|h| s.view.get(*h))
                        .min_by(|a, b| a.pos.distance(me.pos).total_cmp(&b.pos.distance(me.pos)));
                    if let Some(p) = nearest {
                        wants.push(Want {
                            regiment: me.handle,
                            act: Act::Attack(p.handle),
                            priority: ROUTINE + 1,
                            group: attack_group(p.handle),
                        });
                    }
                }
            }
            Phase::Out => {
                if let Some(t) = self.target {
                    walk(wants, t.at, ROUTINE);
                }
            }
        }
    }

    /// A few plain words on what it is doing, for the AI lens: nothing
    /// for a level that doesn't raid.
    pub fn note(&self, now: u32, name: impl Fn(u8) -> String) -> String {
        let n = self.regiments.len();
        let regiments = format!("{n} regiment{}", if n == 1 { "" } else { "s" });
        let seconds = |t: u32| t.saturating_sub(now).div_ceil(TICKS_PER_SECOND);
        match (self.phase, self.target) {
            (Phase::Out, Some(t)) => format!(
                "{regiments} {} {} {} at ({:.0}, {:.0})",
                if t.workers {
                    "raiding"
                } else {
                    "going for the"
                },
                name(t.owner),
                t.what(),
                t.at.x,
                t.at.y
            ),
            (Phase::Out, None) => format!("{regiments} raiding, choosing where next"),
            (Phase::Falling(_), _) => format!("{regiments} falling back to the rally point"),
            (Phase::Resting(until), _) => {
                format!("{regiments} resting at the rally, {}s more", seconds(until))
            }
            (Phase::Idle, _) if now < self.next_party => {
                format!("none out, sends another in {}s", seconds(self.next_party))
            }
            (Phase::Idle, _) => "none out".to_string(),
        }
    }
}

/// An enemy tower it has seen: where it stands, and how far round it is
/// covered by its shooting.
#[derive(Clone, Copy, Debug)]
struct Tower {
    at: Vec2,
    reach: f32,
}

impl Tower {
    fn covers(&self, at: Vec2) -> bool {
        self.at.distance(at) <= self.reach
    }
}

/// The enemy towers it has seen that shoot.
fn towers(s: &Sense) -> Vec<Tower> {
    spymaster::enemy_buildings(s)
        .iter()
        .filter(|b| b.attack.is_some())
        .map(|b| Tower {
            at: b.at,
            // As far as it could shoot from high ground.
            reach: b.range() * gc_sim::terrain::DOWNHILL_RANGE_PERCENT as f32 / 100.0
                + TOWER_MARGIN,
        })
        .collect()
}

/// Whether a building of `role` at `at` is an expansion hall: an outpost,
/// or a main building not at one of the map's starts.
fn is_expansion(role: Role, at: Vec2, starts: &[Vec2]) -> bool {
    match role {
        Role::Outpost => true,
        Role::Main => starts.iter().all(|st| st.distance(at) > START_REACH),
        _ => false,
    }
}

/// The spot to raid from `from`: those of the enemy `focus` if any are
/// known, the ones with no tower or enemy fighter near them first, nearest
/// first; failing those, the least covered. Returns its index in `spots`.
fn choose(
    spots: &[Spot],
    towers: &[Tower],
    foes: &[Vec2],
    focus: Option<u8>,
    from: Vec2,
) -> Option<usize> {
    let chosen: Vec<usize> = (0..spots.len()).collect();
    let focused: Vec<usize> = chosen
        .iter()
        .copied()
        .filter(|&i| focus == Some(spots[i].owner))
        .collect();
    let pool = if focused.is_empty() { chosen } else { focused };
    // How far a spot is clear of every tower's and fighter's reach.
    let clearance = |at: Vec2| -> f32 {
        let t = towers
            .iter()
            .map(|t| t.at.distance(at) - t.reach)
            .fold(f32::MAX, f32::min);
        let f = foes
            .iter()
            .map(|p| p.distance(at) - GUARD_REACH)
            .fold(f32::MAX, f32::min);
        t.min(f)
    };
    let nearest = |a: &usize, b: &usize| {
        let d = |i: &usize| spots[*i].at.distance(from);
        d(a).total_cmp(&d(b))
    };
    let safe = pool
        .iter()
        .copied()
        .filter(|&i| clearance(spots[i].at) > 0.0)
        .min_by(nearest);
    safe.or_else(|| {
        pool.iter().copied().max_by(|a, b| {
            clearance(spots[*a].at)
                .total_cmp(&clearance(spots[*b].at))
                .then(nearest(b, a))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spot(x: f32, y: f32, owner: u8) -> Spot {
        Spot {
            at: Vec2::new(x, y),
            owner,
            workers: true,
        }
    }

    const HOME_AT: Vec2 = Vec2::new(0.0, 0.0);

    /// A tower covering 18 tiles round it.
    fn tower(x: f32, y: f32) -> Tower {
        Tower {
            at: Vec2::new(x, y),
            reach: 18.0,
        }
    }

    #[test]
    fn a_shooting_tower_covers_its_range_and_a_little_more() {
        let t = Tower {
            at: HOME_AT,
            reach: 30.0 + TOWER_MARGIN,
        };
        assert!(t.covers(Vec2::new(33.0, 0.0)));
        assert!(!t.covers(Vec2::new(35.0, 0.0)));
    }

    #[test]
    fn it_goes_for_the_nearest_workers_when_nothing_covers_them() {
        let spots = [spot(100.0, 0.0, 1), spot(40.0, 0.0, 1), spot(70.0, 0.0, 1)];
        assert_eq!(choose(&spots, &[], &[], None, HOME_AT), Some(1));
        assert_eq!(choose(&[], &[], &[], None, HOME_AT), None);
    }

    #[test]
    fn it_prefers_the_enemy_its_sovereign_goes_for() {
        let spots = [spot(40.0, 0.0, 1), spot(150.0, 0.0, 2)];
        assert_eq!(choose(&spots, &[], &[], Some(2), HOME_AT), Some(1));
        // A focus whose workers it knows nothing of changes nothing.
        assert_eq!(choose(&spots, &[], &[], Some(3), HOME_AT), Some(0));
    }

    #[test]
    fn it_keeps_away_from_towers_and_fighters() {
        let spots = [spot(40.0, 0.0, 1), spot(70.0, 0.0, 1), spot(120.0, 0.0, 1)];
        // A tower covers the nearest: the next is taken.
        let covers_near = [tower(45.0, 0.0)];
        assert_eq!(choose(&spots, &covers_near, &[], None, HOME_AT), Some(1));
        // Soldiers by that one too: the far one.
        let foes = [Vec2::new(75.0, 5.0)];
        assert_eq!(choose(&spots, &covers_near, &foes, None, HOME_AT), Some(2));
        // Safe workers of another enemy don't outrank the focus's covered ones.
        let two = [spot(40.0, 0.0, 1), spot(120.0, 0.0, 2)];
        assert_eq!(
            choose(&two, &[tower(120.0, 0.0)], &[], Some(2), HOME_AT),
            Some(1)
        );
    }

    #[test]
    fn with_everything_covered_it_takes_the_least_covered() {
        let spots = [spot(40.0, 0.0, 1), spot(80.0, 0.0, 1)];
        let towers = [tower(40.0, 5.0), tower(80.0, 15.0)];
        assert_eq!(choose(&spots, &towers, &[], None, HOME_AT), Some(1));
    }

    #[test]
    fn only_an_outpost_or_a_main_off_the_starts_is_an_expansion() {
        let starts = [Vec2::new(10.0, 10.0), Vec2::new(200.0, 200.0)];
        assert!(is_expansion(Role::Outpost, Vec2::new(10.0, 10.0), &starts));
        assert!(!is_expansion(Role::Main, Vec2::new(14.0, 12.0), &starts));
        assert!(is_expansion(Role::Main, Vec2::new(120.0, 90.0), &starts));
        assert!(!is_expansion(
            Role::Production,
            Vec2::new(120.0, 90.0),
            &starts
        ));
    }

    #[test]
    fn a_raider_with_no_party_out_says_so() {
        let r = Raider::default();
        assert_eq!(r.note(0, |_| String::new()), "none out");
        assert!(r.regiments().is_empty());
    }
}
