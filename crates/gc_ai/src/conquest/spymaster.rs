//! The Spymaster (docs/18-ai.md, "Spymaster: perception and belief"): what
//! a Conquest mind believes about each enemy's army. It counts the enemy
//! regiments its side sees now and those it has seen and not seen fall,
//! where it last saw them; it never knows of one it hasn't seen. The
//! battle estimator then weighs that army against its own.

use gc_sim::structures::Role;
use glam::Vec2;

use super::{Sense, steward};
use crate::estimator::{Approach, Force, estimate};
use crate::perception::{Seen, TICKS};
use crate::personality::Faction;

/// How a fight is taken to start, for weighing armies before one: the
/// fronts a little apart, closing at a walk.
const APPROACH: Approach = Approach {
    gap: 15.0,
    closing: 1.5,
};

/// Every fighting regiment it knows of, of any side: those in sight now
/// and those it remembers, where it last saw them. Workers and the broken
/// don't count.
fn known<'a>(s: &'a Sense) -> impl Iterator<Item = &'a Seen> + 'a {
    s.view
        .regiments
        .iter()
        .chain(s.view.last_seen())
        .filter(|r| !r.broken() && !s.kit.all_workers.contains(&r.unit))
}

/// How far from a base an opportunist counts its defenders, tiles.
const DEFENDERS_REACH: f32 = 50.0;

/// The fighting regiments of `owner` it knows of.
pub fn army_of<'a>(s: &'a Sense, owner: u8) -> impl Iterator<Item = &'a Seen> + 'a {
    known(s).filter(move |r| r.owner == owner)
}

/// The fighting regiments of all its enemies that it knows of.
pub fn enemy_army<'a>(s: &'a Sense) -> impl Iterator<Item = &'a Seen> + 'a {
    known(s).filter(|r| s.view.hostile(s.party, r.owner))
}

/// The enemies whose army it knows any of.
pub fn enemies(s: &Sense) -> Vec<u8> {
    let mut owners: Vec<u8> = enemy_army(s).map(|r| r.owner).collect();
    owners.sort_unstable();
    owners.dedup();
    owners
}

/// The longest it follows a lost regiment along the way it was going,
/// seconds: past that it could have turned anywhere.
const FOLLOW_SECONDS: f32 = 45.0;
/// How far a lost regiment may have strayed from where it guesses, as a
/// share of the ground it could have covered since.
const STRAY: f32 = 0.5;

/// Where it guesses a regiment it has lost sight of is now, and how far
/// off that guess may be, tiles: where it was going, at its pace, for a
/// while, and anywhere about that the longer it has been. One in sight
/// is where it is.
pub fn guess(s: &Sense, r: &Seen) -> (Vec2, f32) {
    let Some(at) = s.view.seen_at(r.handle).filter(|&t| t < s.now) else {
        return (r.pos, 0.0);
    };
    let seconds = (s.now - at) as f32 / TICKS;
    let pace = s.view.pace(r);
    let ahead = if r.moving {
        r.facing * pace * seconds.min(FOLLOW_SECONDS)
    } else {
        Vec2::ZERO
    };
    (s.terrain.clamp(r.pos + ahead, 0.0), pace * seconds * STRAY)
}

/// The part of `owner`'s army it expects to meet attacking its building
/// at `at`: all of it, or for an opportunist only the regiments that may
/// be within [`DEFENDERS_REACH`] of it (where it guesses those it has lost
/// sight of went), so it strikes a base whose army is away.
pub fn defenders<'a>(s: &'a Sense, owner: u8, at: Vec2) -> impl Iterator<Item = &'a Seen> + 'a {
    let opportunist = s.plan.opportunist;
    army_of(s, owner).filter(move |r| {
        if !opportunist {
            return true;
        }
        let (near, off) = guess(s, r);
        near.distance(at) - off <= DEFENDERS_REACH
    })
}

/// How a fight between `ours` and `theirs` would go, from -1 (a rout of
/// ours) through 0 (a coin toss) to +1 (a rout of theirs).
pub fn odds<'a>(
    s: &Sense,
    ours: impl IntoIterator<Item = &'a Seen>,
    theirs: impl IntoIterator<Item = &'a Seen>,
) -> f32 {
    let ours = Force::of(s.view, ours);
    let theirs = Force::of(s.view, theirs);
    estimate(&ours, &theirs, APPROACH).margin
}

/// The share of `regiments`' hit points in each class: line, shock,
/// ranged.
pub fn classes<'a>(s: &Sense, regiments: impl IntoIterator<Item = &'a Seen>) -> [f32; 3] {
    let mut hp = [0.0f32; 3];
    for r in regiments {
        let u = s.view.unit(r);
        hp[steward::class_slot(u.class)] += f32::from(r.models) * f32::from(u.stats.hp_per_model);
    }
    let all: f32 = hp.iter().sum();
    if all > 0.0 { hp.map(|h| h / all) } else { hp }
}

/// What it believes of one enemy's army, kept for the AI lens to show:
/// nothing here is read back to decide anything.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Belief {
    pub owner: u8,
    /// Its faction, told by the units it has seen.
    pub faction: Option<Faction>,
    /// The supply of the regiments it knows of.
    pub supply: u32,
}

/// What it believes now of every enemy whose army it knows any of.
pub fn beliefs(s: &Sense) -> Vec<Belief> {
    enemies(s)
        .into_iter()
        .map(|owner| Belief {
            owner,
            faction: army_of(s, owner)
                .find_map(|r| s.kit.all_units.get(usize::from(r.unit)))
                .map(|&(f, _)| f),
            supply: s.kit.supply_of(army_of(s, owner)).round() as u32,
        })
        .collect()
}

/// How it reads an enemy's early build, from the buildings its side has
/// seen of theirs: what it answers when it adapts its opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    /// A second production building on one hall, early: an attack is
    /// coming soon.
    Rush,
    /// A second hall early: few soldiers for now.
    Greedy,
    /// Towers by the hall: it means to sit tight.
    Walls,
}

impl Read {
    /// What it does about it, in a few words for the journal.
    pub fn answer_name(self) -> &'static str {
        match self {
            Read::Rush => "a tower and a second production building",
            Read::Greedy => "an early attack",
            Read::Walls => "an expansion of its own",
        }
    }

    /// A few words for the journal and the lens.
    pub fn name(self) -> &'static str {
        match self {
            Read::Rush => "an early attack",
            Read::Greedy => "a fast expansion",
            Read::Walls => "towers and a slow build",
        }
    }

    /// Which to answer first when enemies differ: a coming attack before
    /// a chance.
    fn urgency(self) -> u8 {
        match self {
            Read::Rush => 2,
            Read::Greedy => 1,
            Read::Walls => 0,
        }
    }
}

/// The seconds of game time by which a build counts as early, for each
/// read: a second production building by 3:30, a second hall or two
/// towers by 5:00. Later, every build has them.
const RUSH_BY: f32 = 210.0;
const GREEDY_BY: f32 = 300.0;
const WALLS_BY: f32 = 300.0;

/// What it has made of one enemy's build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scouted {
    pub owner: u8,
    pub read: Read,
    /// The tick it made it.
    pub at: u32,
}

/// Read the early builds of the enemies whose buildings its side sees now,
/// adding to `reads` (one for each enemy, kept once made). Returns the
/// reads made this time.
pub fn read_builds(s: &Sense, reads: &mut Vec<Scouted>) -> Vec<Scouted> {
    let mut new = Vec::new();
    let Some(st) = s.world.structures() else {
        return new;
    };
    let seconds = s.now as f32 / TICKS;
    if seconds > RUSH_BY.max(GREEDY_BY).max(WALLS_BY) {
        return new;
    }
    let table = st.buildings();
    // Halls, production buildings and towers of each enemy it has seen.
    let mut counts: Vec<(u8, [u8; 3])> = Vec::new();
    for v in st
        .iter()
        .filter(|v| s.view.hostile(s.party, v.owner) && v.seen_by(s.party))
    {
        let Some(b) = table.get(usize::from(v.building)) else {
            continue;
        };
        let slot = match b.role {
            Role::Main | Role::Outpost => 0,
            Role::Production => 1,
            Role::Defence => 2,
            _ => continue,
        };
        match counts.iter_mut().find(|(o, _)| *o == v.owner) {
            Some((_, c)) => c[slot] += 1,
            None => {
                let mut c = [0; 3];
                c[slot] = 1;
                counts.push((v.owner, c));
            }
        }
    }
    for (owner, [halls, producers, towers]) in counts {
        if reads.iter().any(|r| r.owner == owner) {
            continue;
        }
        let read = if producers >= 2 && halls <= 1 && seconds <= RUSH_BY {
            Read::Rush
        } else if halls >= 2 && seconds <= GREEDY_BY {
            Read::Greedy
        } else if towers >= 2 && seconds <= WALLS_BY {
            Read::Walls
        } else {
            continue;
        };
        let r = Scouted {
            owner,
            read,
            at: s.now,
        };
        reads.push(r);
        new.push(r);
    }
    new
}

/// The read to answer of those made: the most urgent.
pub fn to_answer(reads: &[Scouted]) -> Option<Scouted> {
    reads.iter().copied().max_by_key(|r| r.read.urgency())
}
