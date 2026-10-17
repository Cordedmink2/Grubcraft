//! The Spymaster (docs/18-ai.md, "Spymaster: perception and belief"): what
//! a Conquest mind believes about each enemy's army. It counts the enemy
//! regiments its side sees now and those it has seen and not seen fall,
//! where it last saw them; it never knows of one it hasn't seen. The
//! battle estimator then weighs that army against its own.

use glam::Vec2;

use super::{Sense, steward};
use crate::estimator::{Approach, Force, estimate};
use crate::perception::Seen;

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

/// The part of `owner`'s army it expects to meet attacking its building
/// at `at`: all of it, or for an opportunist only the regiments within
/// [`DEFENDERS_REACH`] of it (where it last saw them), so it strikes a
/// base whose army is away.
pub fn defenders<'a>(s: &'a Sense, owner: u8, at: Vec2) -> impl Iterator<Item = &'a Seen> + 'a {
    let reach = if s.plan.opportunist {
        DEFENDERS_REACH
    } else {
        f32::INFINITY
    };
    army_of(s, owner).filter(move |r| r.pos.distance(at) <= reach)
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
