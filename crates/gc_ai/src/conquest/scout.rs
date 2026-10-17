//! The Scout: one fast regiment the Marshal lends to look at the map
//! (docs/18-ai.md, "Marshal"). It looks at every start an enemy might hold,
//! nearest first; a level that scouts all game then goes on to the place it
//! knows least about, among the starts and the middles of the seep clusters
//! (both are public on the map). It keeps out of the army's way (the Marshal
//! leaves it out of the army, its battles and its waves), falls back from
//! enemy fighters it sees, and is replaced after a while if it dies.

use gc_sim::{RegimentHandle, TICKS_PER_SECOND};
use glam::Vec2;

use super::strategy::Scouting;
use super::{Sense, spymaster};
use crate::orders::{ROUTINE, Want};
use crate::perception::{Seen, v2};

/// The scout needs an army of at least this many fighters (itself counted)
/// before it is taken from it: a level that scouts all game can spare one
/// early; one that sends a single early look waits for a bigger army, as
/// the first waves decide the early game.
const MIN_ARMY: usize = 2;
const MIN_ARMY_EARLY: usize = 4;
/// How far a regiment sees a base from.
const SIGHT: f32 = 38.0;
/// A place counts as looked at once a regiment of its own comes this near.
const LOOKED: f32 = 14.0;
/// An enemy this near a place guards it.
const GUARDED: f32 = 32.0;
/// An enemy building seen this near a start means someone holds it.
const HELD: f32 = 16.0;
/// Ticks it falls back for, and the ticks it then leaves the place it was
/// going to alone.
const FALL_BACK: u32 = 6 * TICKS_PER_SECOND;
const AVOID: u32 = 90 * TICKS_PER_SECOND;
/// A place within this many tiles of its own or a friend's building is not
/// worth a look (and is not an enemy's start).
const NEAR_OWN: f32 = 28.0;
/// Seeps within this many tiles of each other make one cluster.
const CLUSTER: f32 = 14.0;
/// Clusters within this many tiles of a start are that start's seeps.
const START_SEEPS: f32 = 24.0;
/// Ticks before a level that scouts all game sends another after losing one.
const REPLACE: u32 = 60 * TICKS_PER_SECOND;
/// The longest an early scout stays out, from the tick it was taken, and
/// the longest its way home may take before it gives its regiment back
/// wherever it is.
const EARLY_LIMIT: u32 = 6 * 60 * TICKS_PER_SECOND;
const HOME_LIMIT: u32 = 90 * TICKS_PER_SECOND;
/// It is back at the rally point within this many tiles.
const HOME: f32 = 9.0;
/// A place seen within this many seconds is not worth another look.
const STALE: f32 = 45.0;
/// How much of the time a place would take to reach counts against it, as a
/// share of the time since it was last seen.
const TRAVEL_COST: f32 = 0.5;
/// The group of the scout's orders (the army's are `u32::MAX - 1` to `- 3`).
const GROUP: u32 = u32::MAX - 9;

/// A place on the map worth a look.
#[derive(Clone, Debug)]
struct Place {
    at: Vec2,
    start: bool,
    /// The tick any of its regiments last had it in sight, if ever.
    seen: Option<u32>,
    /// Left alone until this tick (enemies were near the way there).
    avoid_until: u32,
    /// Next to its own or a friend's building, as of the last look.
    own: bool,
}

#[derive(Default)]
pub struct Scout {
    level: Scouting,
    regiment: Option<RegimentHandle>,
    places: Vec<Place>,
    /// The place it is going to, an index into `places`.
    target: Option<usize>,
    /// Its way there is given up on at this tick.
    trip_ends: u32,
    /// The place it was last driven back from on the way to it.
    driven_back: Option<usize>,
    /// It falls back until this tick.
    fall_back_until: u32,
    /// The tick it took its regiment (the first time).
    taken: Option<u32>,
    /// The tick it lost its regiment.
    lost_at: Option<u32>,
    /// It will not send another.
    done: bool,
    /// An early scout has seen what it was for and is coming back.
    returning: bool,
    /// Since when it has been coming back.
    returning_since: Option<u32>,
    home: Option<Vec2>,
    /// The tick its regiment first came to look at a start an enemy might
    /// hold.
    first_enemy_start: Option<u32>,
}

impl Scout {
    /// The regiment it has taken, if any: the army leaves it alone.
    pub fn regiment(&self) -> Option<RegimentHandle> {
        self.regiment
    }

    /// The tick its regiment first came to look at a start an enemy might
    /// hold (for the tests).
    pub fn first_enemy_start_looked_at(&self) -> Option<u32> {
        self.first_enemy_start
    }

    /// Keep its regiment and route up to date. `fighters` are all the
    /// fighting regiments of its side; `waiting` those of them standing at
    /// the rally point, free to go.
    pub fn update(
        &mut self,
        s: &Sense,
        fighters: &[&Seen],
        waiting: &[&Seen],
        rally: Option<Vec2>,
        home: Option<Vec2>,
    ) {
        self.level = s.plan.scouting;
        if self.level == Scouting::Never {
            return;
        }
        self.home = home.or(self.home);
        if self.places.is_empty() {
            self.places = places(s);
        }
        self.look(s);
        // Has it lost its regiment?
        if let Some(h) = self.regiment
            && s.view.get(h).is_none()
        {
            self.regiment = None;
            self.target = None;
            self.returning = false;
            self.lost_at = Some(s.now);
            if self.level == Scouting::Early {
                self.done = true;
            }
        }
        let Some(h) = self.regiment else {
            self.take(s, fighters, waiting);
            return;
        };
        let Some(me) = s.view.get(h) else { return };
        let (pos, pace) = (me.pos, s.view.pace(me));
        let early = self.level == Scouting::Early;
        // An early scout has a limit on how long it stays out.
        if early && !self.returning && self.taken.is_some_and(|t| s.now >= t + EARLY_LIMIT) {
            self.returning = true;
            self.returning_since = Some(s.now);
            self.target = None;
        }
        // Enemy fighters near: fall back, and leave the place it was going
        // to alone for a while.
        if s.now >= self.fall_back_until
            && let Some(foe) = spymaster::outmatched(s, &[me])
        {
            self.fall_back_until = s.now + FALL_BACK;
            // Leave the place alone for a while only if the enemy stands by
            // it; one met on the way is only passing.
            if let Some(t) = self.target.take() {
                // Driven back twice on the way to the same place: leave it.
                let again = self.driven_back == Some(t);
                self.driven_back = (!again).then_some(t);
                if again || self.places[t].at.distance(foe) <= GUARDED {
                    self.places[t].avoid_until = s.now + AVOID;
                    self.driven_back = None;
                }
            }
        }
        if s.now < self.fall_back_until {
            return;
        }
        // Where it was going: reached, given up on, or its place now has a
        // friend's building by it.
        if let Some(t) = self.target {
            let p = &self.places[t];
            let reached = p.at.distance(pos) <= LOOKED;
            if reached || s.now >= self.trip_ends || p.own {
                if !reached && !p.own {
                    self.places[t].avoid_until = s.now + AVOID;
                }
                self.target = None;
            }
        }
        if self.returning {
            // Back at the rally point it gives the regiment back, for good.
            let late = self
                .returning_since
                .is_some_and(|t| s.now >= t + HOME_LIMIT);
            if late || rally.is_none_or(|r| r.distance(pos) <= HOME) {
                self.regiment = None;
                self.returning = false;
                self.done = true;
            }
            return;
        }
        if self.target.is_none() {
            self.target = self.next(s, pos);
            if let Some(t) = self.target {
                let d = self.places[t].at.distance(pos);
                self.trip_ends =
                    s.now + ((d / pace.max(0.5)) * 2.0 + 30.0) as u32 * TICKS_PER_SECOND;
            } else if early && self.starts_left() == 0 {
                // It has seen what it was sent for.
                self.returning = true;
                self.returning_since = Some(s.now);
            }
        }
    }

    /// The orders for its regiment now, if it has one.
    pub fn steer(&self, s: &Sense, rally: Option<Vec2>, wants: &mut Vec<Want>) {
        let Some(h) = self.regiment else { return };
        let (to, priority) = if s.now < self.fall_back_until {
            (self.home.or(rally), ROUTINE + 1)
        } else if let Some(t) = self.target {
            (Some(self.places[t].at), ROUTINE)
        } else if self.level == Scouting::Early || self.returning {
            (rally.or(self.home), ROUTINE)
        } else {
            // Nothing worth a look just now: it stays where it is.
            return;
        };
        let Some(to) = to else { return };
        wants.push(s.walk(h, to, priority, GROUP));
    }

    /// Note which places its regiments have in sight now.
    fn look(&mut self, s: &Sense) {
        let scout = self.regiment.and_then(|h| s.view.get(h)).map(|r| r.pos);
        let ours: Vec<Vec2> = s
            .view
            .regiments
            .iter()
            .filter(|r| r.owner == s.party)
            .map(|r| r.pos)
            .collect();
        let friendly: Vec<Vec2> = s.world.structures().map_or(Vec::new(), |st| {
            st.iter()
                .filter(|v| !s.view.hostile(s.party, v.owner))
                .map(|v| v2(v.centre()))
                .collect()
        });
        // Enemy buildings it has seen: a start with one by it is looked at
        // (a scout sees a base from further than it can safely go), and the
        // scout's regiment has found it if it is in sight of it.
        let theirs: Vec<Vec2> = spymaster::enemy_buildings(s)
            .into_iter()
            .map(|b| b.at)
            .collect();
        for p in &mut self.places {
            if p.start && theirs.iter().any(|b| b.distance(p.at) <= HELD) {
                p.seen.get_or_insert(s.now);
                if scout.is_some_and(|o| o.distance(p.at) <= SIGHT) {
                    self.first_enemy_start.get_or_insert(s.now);
                }
            }
            p.own = friendly.iter().any(|f| f.distance(p.at) <= NEAR_OWN);
            if ours.iter().any(|o| o.distance(p.at) <= LOOKED) {
                p.seen = Some(s.now);
            }
            if p.start && !p.own && scout.is_some_and(|o| o.distance(p.at) <= LOOKED) {
                self.first_enemy_start.get_or_insert(s.now);
            }
        }
    }

    /// Take the fastest of the regiments waiting at the rally point, if
    /// there is an army to spare one from, and it is time.
    fn take(&mut self, s: &Sense, fighters: &[&Seen], waiting: &[&Seen]) {
        let (min_army, unknown_starts) = match self.level {
            Scouting::Early => (MIN_ARMY_EARLY, 2),
            _ => (MIN_ARMY, 1),
        };
        if self.done || fighters.len() < min_army {
            return;
        }
        // One early look is for finding where the enemies are: with a single
        // start they could hold, the map has told it already.
        if self.level == Scouting::Early && self.starts_left() < unknown_starts {
            return;
        }
        if self.lost_at.is_some_and(|t| s.now < t + REPLACE) {
            return;
        }
        // Nothing to look at: everything is its own.
        if self.places.iter().all(|p| p.own) {
            return;
        }
        let fastest = waiting
            .iter()
            .max_by(|a, b| s.view.pace(a).total_cmp(&s.view.pace(b)));
        if let Some(r) = fastest {
            self.regiment = Some(r.handle);
            self.taken.get_or_insert(s.now);
            self.target = None;
            self.returning = false;
            self.fall_back_until = 0;
        }
    }

    /// How many starts an enemy might hold that none of its regiments has
    /// seen.
    fn starts_left(&self) -> usize {
        self.places
            .iter()
            .filter(|p| p.start && !p.own && p.seen.is_none())
            .count()
    }

    /// The next place to go to from `from`: every start an enemy might hold
    /// first, nearest first; then, for a level that scouts all game, the
    /// place it knows least about.
    fn next(&self, s: &Sense, from: Vec2) -> Option<usize> {
        let free = |p: &Place| !p.own && s.now >= p.avoid_until;
        let unseen_start = |p: &Place| p.start && p.seen.is_none();
        let nearest = self
            .places
            .iter()
            .enumerate()
            .filter(|(_, p)| free(p) && unseen_start(p))
            .min_by(|a, b| a.1.at.distance(from).total_cmp(&b.1.at.distance(from)))
            .map(|(i, _)| i);
        if nearest.is_some() || self.level != Scouting::Always {
            return nearest;
        }
        let ticks = TICKS_PER_SECOND as f32;
        let pace = self
            .regiment
            .and_then(|h| s.view.get(h))
            .map_or(3.0, |r| s.view.pace(r))
            .max(0.5);
        let age = |p: &Place| (s.now - p.seen.unwrap_or(0).min(s.now)) as f32 / ticks;
        let score = |p: &Place| age(p) - TRAVEL_COST * p.at.distance(from) / pace;
        self.places
            .iter()
            .enumerate()
            .filter(|(_, p)| free(p) && age(p) >= STALE)
            .max_by(|a, b| score(a.1).total_cmp(&score(b.1)))
            .map(|(i, _)| i)
    }

    /// A few plain words on what it is doing, for the AI lens.
    pub fn note(&self, now: u32) -> String {
        let starts = self.places.iter().filter(|p| p.start).count();
        let seen = self
            .places
            .iter()
            .filter(|p| p.start && p.seen.is_some())
            .count();
        let tally = format!("{seen} of {starts} starts seen");
        if self.level == Scouting::Never {
            "none, this level sends none".to_string()
        } else if self.regiment.is_some() {
            if now < self.fall_back_until {
                format!("falling back from enemy fighters, {tally}")
            } else if self.returning {
                format!("coming home, {tally}")
            } else if let Some(t) = self.target {
                let p = &self.places[t];
                let what = if p.start { "start" } else { "seeps" };
                match self.home.map(|h| compass(h, p.at)) {
                    Some(Some(dir)) => {
                        format!("looking at the {what} in the {dir}, {tally}")
                    }
                    _ => format!("looking at a {what} near home, {tally}"),
                }
            } else {
                format!("choosing where to look, {tally}")
            }
        } else if self.done {
            format!("done, {tally}")
        } else if let Some(t) = self.lost_at {
            // An early scout lost is done: only one that scouts all game
            // gets here.
            let left = (t + REPLACE).saturating_sub(now).div_ceil(TICKS_PER_SECOND);
            if left > 0 {
                format!("lost, sends another in {left}s, {tally}")
            } else {
                format!("lost, waiting for a spare regiment, {tally}")
            }
        } else {
            "none yet".to_string()
        }
    }
}

/// Every place worth a look: the map's starts, then the middles of its
/// seep clusters away from any start.
fn places(s: &Sense) -> Vec<Place> {
    let place = |at: Vec2, start: bool| Place {
        at,
        start,
        seen: None,
        avoid_until: 0,
        own: false,
    };
    let mut out: Vec<Place> = s.kit.starts.iter().map(|&p| place(p, true)).collect();
    let seeps: Vec<Vec2> = s.world.economy().map_or(Vec::new(), |e| {
        e.nodes()
            .filter(|n| n.resource == gc_sim::Resource::Grub)
            .map(|n| v2(n.at))
            .collect()
    });
    for c in clusters(&seeps, CLUSTER) {
        if s.kit.starts.iter().all(|st| st.distance(c) > START_SEEPS) {
            out.push(place(c, false));
        }
    }
    out
}

/// The middles of groups of `points` that lie within `radius` of each
/// other (each joins the first group whose middle is within it).
fn clusters(points: &[Vec2], radius: f32) -> Vec<Vec2> {
    let mut groups: Vec<(Vec2, u32)> = Vec::new();
    for &p in points {
        match groups.iter_mut().find(|(c, _)| c.distance(p) <= radius) {
            Some((c, n)) => {
                *c = (*c * *n as f32 + p) / (*n + 1) as f32;
                *n += 1;
            }
            None => groups.push((p, 1)),
        }
    }
    groups.into_iter().map(|(c, _)| c).collect()
}

/// Which way `to` lies from `from` as a compass direction (north is up the
/// map, towards smaller y), or None if it is close by.
fn compass(from: Vec2, to: Vec2) -> Option<&'static str> {
    let d = to - from;
    if d.length() < 20.0 {
        return None;
    }
    const NAMES: [&str; 8] = [
        "east",
        "north-east",
        "north",
        "north-west",
        "west",
        "south-west",
        "south",
        "south-east",
    ];
    let angle = (-d.y).atan2(d.x).to_degrees();
    let sector = (angle / 45.0).round() as i32;
    Some(NAMES[sector.rem_euclid(8) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeps_close_together_make_one_place() {
        let seeps = [
            Vec2::new(10.0, 10.0),
            Vec2::new(14.0, 12.0),
            Vec2::new(12.0, 8.0),
            Vec2::new(100.0, 100.0),
        ];
        let found = clusters(&seeps, CLUSTER);
        assert_eq!(found.len(), 2);
        assert!(found[0].distance(Vec2::new(12.0, 10.0)) < 0.1);
        assert_eq!(found[1], Vec2::new(100.0, 100.0));
    }

    #[test]
    fn compass_has_north_up_the_map() {
        let home = Vec2::new(100.0, 100.0);
        assert_eq!(compass(home, home + Vec2::new(0.0, -50.0)), Some("north"));
        assert_eq!(compass(home, home + Vec2::new(50.0, 0.0)), Some("east"));
        assert_eq!(
            compass(home, home + Vec2::new(40.0, 40.0)),
            Some("south-east")
        );
        assert_eq!(
            compass(home, home + Vec2::new(-40.0, -40.0)),
            Some("north-west")
        );
        assert_eq!(compass(home, home + Vec2::new(-50.0, 0.0)), Some("west"));
        assert_eq!(compass(home, home + Vec2::new(5.0, 5.0)), None);
    }

    #[test]
    fn a_scout_that_never_goes_out_says_so() {
        let scout = Scout::default();
        assert_eq!(scout.note(0), "none, this level sends none");
        assert_eq!(scout.regiment(), None);
    }
}
