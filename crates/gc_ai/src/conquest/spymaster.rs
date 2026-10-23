//! The Spymaster (docs/18-ai.md, "Spymaster: perception and belief"): what
//! a Conquest mind believes about each enemy's army. It counts the enemy
//! regiments its side sees now and those it has seen and not seen fall,
//! where it last saw them; it never knows of one it hasn't seen. The
//! battle estimator then weighs that army against its own.

use gc_sim::structures::{Role, StructureHandle, TowerAttack};
use glam::Vec2;

use super::{Sense, steward};
use crate::estimator::{Approach, Force, VOLLEY_PACE, estimate};
use crate::perception::{Seen, TICKS, fx, v2};
use crate::personality::Faction;

/// How a fight is taken to start, for weighing armies before one: the
/// fronts a little apart, closing at a walk.
const APPROACH: Approach = Approach {
    gap: 15.0,
    closing: 1.5,
};

/// A tower this far beyond its range from a building still shoots into a
/// fight for it, in tiles: an army attacking a base spreads round it.
const TOWER_FIGHT: f32 = 8.0;

/// An enemy building its side has seen and still stands.
pub struct Sighted {
    pub handle: StructureHandle,
    pub at: Vec2,
    pub role: Role,
    pub owner: u8,
    /// A finished tower's shooting (towers fire, docs/26-fortifications.md).
    pub attack: Option<TowerAttack>,
    /// Hit points it has left.
    pub hp: u32,
}

impl Sighted {
    /// How far from its middle a tower shoots, in tiles (0 for a building
    /// that doesn't).
    pub fn range(&self) -> f32 {
        self.attack.map_or(0.0, |a| fx(a.range))
    }

    /// Hit points a second its volleys take, at the sim's volley pace.
    pub fn dps(&self) -> f32 {
        self.attack.map_or(0.0, |a| {
            f32::from(a.shots) * f32::from(a.damage) * TICKS / f32::from(a.reload_ticks.max(1))
                * VOLLEY_PACE
        })
    }
}

/// Every enemy building its side has seen and still stands.
pub fn enemy_buildings(s: &Sense) -> Vec<Sighted> {
    let Some(st) = s.world.structures() else {
        return Vec::new();
    };
    let table = st.buildings();
    st.iter()
        .filter(|v| s.view.hostile(s.party, v.owner) && v.seen_by(s.party))
        .filter_map(|v| {
            let b = table.get(usize::from(v.building))?;
            Some(Sighted {
                handle: v.handle,
                at: v2(v.centre()),
                role: b.role,
                owner: v.owner,
                attack: b.attack.filter(|_| v.finished()),
                hp: v.hp,
            })
        })
        .collect()
}

/// The enemy fighters in sight now within `reach` of any of `points`.
pub fn fighters_near<'a>(
    s: &'a Sense,
    points: &'a [Vec2],
    reach: f32,
) -> impl Iterator<Item = &'a Seen> + 'a {
    s.view.regiments.iter().filter(move |r| {
        s.view.hostile(s.party, r.owner)
            && !r.broken()
            && !s.kit.all_workers.contains(&r.unit)
            && points.iter().any(|p| p.distance(r.pos) <= reach)
    })
}

/// How near enemy fighters in sight must be to threaten a small party
/// out on its own (a scout, raiders), tiles.
const DANGER: f32 = 22.0;

/// Where the nearest enemy fighter in sight stands, if those within
/// [`DANGER`] of any of `mine` have more supply than `mine`: a lone enemy
/// no bigger (another side's scout) is let pass, or two scouts would turn
/// each other back for ever.
pub fn outmatched(s: &Sense, mine: &[&Seen]) -> Option<Vec2> {
    let at: Vec<Vec2> = mine.iter().map(|r| r.pos).collect();
    let near: Vec<&Seen> = fighters_near(s, &at, DANGER).collect();
    if s.kit.supply_of(near.iter().copied()) <= s.kit.supply_of(mine.iter().copied()) {
        return None;
    }
    let from = crate::captain::helpers::centroid(mine);
    near.iter()
        .map(|r| r.pos)
        .min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)))
}

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
/// The farthest it reckons a lost regiment may have strayed, tiles: past
/// it, the guess says nothing.
const MOST_STRAY: f32 = 25.0;

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
    let off = (pace * seconds * STRAY).min(MOST_STRAY);
    (s.terrain.clamp(r.pos + ahead, 0.0), off)
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

/// [`odds`] for a fight at enemy building `at`: the enemy towers among
/// `seen` (its [`enemy_buildings`]) that would reach the fight there shoot
/// with `theirs`.
pub fn odds_at<'a>(
    s: &Sense,
    ours: impl IntoIterator<Item = &'a Seen>,
    theirs: impl IntoIterator<Item = &'a Seen>,
    at: Vec2,
    seen: &[Sighted],
) -> f32 {
    let ours = Force::of(s.view, ours);
    let mut theirs = Force::of(s.view, theirs);
    for t in seen {
        if t.attack.is_some() && t.at.distance(at) <= t.range() + TOWER_FIGHT {
            theirs = theirs.with_tower(t.dps(), t.range(), t.hp as f32);
        }
    }
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
/// In order of urgency, the least first: a coming attack is answered
/// before a chance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Read {
    /// Towers by the hall: it means to sit tight.
    Walls,
    /// A second hall early: few soldiers for now.
    Greedy,
    /// A second production building on one hall, early: an attack is
    /// coming soon.
    Rush,
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
}

/// The seconds of game time by which a build counts as early, for each
/// read: a second production building by 3:30, a second hall or two
/// towers by 5:00. Later, every build has them.
const RUSH_BY: f32 = 210.0;
const GREEDY_BY: f32 = 300.0;
const WALLS_BY: f32 = 300.0;
/// Past this, seconds, there is no early build left to read.
pub const READ_UNTIL: f32 = 300.0;

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
    let seconds = s.now as f32 / TICKS;
    if seconds > READ_UNTIL {
        return new;
    }
    // Halls, production buildings and towers of each enemy it has seen.
    let mut counts: Vec<(u8, [u8; 3])> = Vec::new();
    for b in enemy_buildings(s) {
        let slot = match b.role {
            Role::Main | Role::Outpost => 0,
            Role::Production => 1,
            Role::Defence => 2,
            _ => continue,
        };
        match counts.iter_mut().find(|(o, _)| *o == b.owner) {
            Some((_, c)) => c[slot] += 1,
            None => {
                let mut c = [0; 3];
                c[slot] = 1;
                counts.push((b.owner, c));
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
    reads.iter().copied().max_by_key(|r| r.read)
}
