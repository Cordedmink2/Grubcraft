//! Reading the enemy: which commander the Warlord is facing, judged from how
//! the enemy's army moves (docs/18-ai.md, "Rehearsal").
//!
//! A rehearsal is only as good as its guess at what the enemy will do. The
//! Warlord can't see the enemy's plan, but it can watch the enemy form up
//! and march, as a person would. It keeps a **shadow** of every commander it
//! might be facing: a mind of each known temper, seated on the enemy's side
//! and thinking on the same view, whose orders go nowhere. Each shadow says
//! where it would put every enemy regiment. Once a second the Warlord looks
//! at where the enemy's regiments actually went: a regiment marching
//! towards a shadow's place for it, or standing in it, bears that shadow
//! out; one marching elsewhere, or standing far from it, counts against.
//! Rehearsals seat the enemy as copies of the shadow that fits best, with
//! its plan already made and its line already laid out, rather than as a
//! commander starting from scratch.
//!
//! It is fair: the shadows see only what the Warlord sees, and judge only
//! by where regiments stand and move.

use std::sync::Arc;

use gc_sim::{PlayerCommand, PlayerId, RegimentHandle, World};
use glam::Vec2;

use crate::captain::Captain;
use crate::difficulty::{Difficulty, Knobs};
use crate::perception::{Perception, TICKS};
use crate::personality::{self, Personality};
use crate::terrain::Terrain;
use crate::{Mind, MindConfig};

/// Ticks between two looks at where the enemy's regiments have gone.
const SAMPLE: u32 = TICKS as u32;
/// How much each look counts against everything seen before it.
const FRESH: f32 = 0.2;
/// Moving less than this far between two looks is standing, tiles.
const STILL: f32 = 0.3;
/// Within this of its place, a regiment is in it, tiles.
const THERE: f32 = 1.5;
/// Standing this far from its place counts as a full miss, tiles.
const FAR: f32 = 10.0;

/// What the Warlord makes of one enemy party.
pub struct Reading {
    party: PlayerId,
    shadows: Vec<Shadow>,
    /// Where each of the enemy's regiments stood at the last look.
    last: Vec<(RegimentHandle, Vec2)>,
    next_look: u32,
    /// Orders the shadows give, thrown away.
    out: Vec<PlayerCommand>,
    /// The temper last told of (see [`Reading::news`]).
    told: Option<&'static Personality>,
}

struct Shadow {
    mind: Mind,
    /// How far the enemy has strayed from this shadow's places, lately:
    /// 0 is every regiment where it says, 1 every one elsewhere.
    miss: f32,
    /// Looks it has been judged on.
    looks: u32,
}

/// The commander a rehearsal seats for an enemy party: its temper, and its
/// captain as it stands (plan made, line laid out).
#[derive(Clone)]
pub struct Guess {
    pub party: PlayerId,
    pub persona: &'static Personality,
    pub captain: Captain,
    /// How well it fits, 0 (not at all) to 1 (every regiment where it says).
    pub fit: f32,
}

impl Reading {
    /// Start reading `party`, with a shadow of every known temper at
    /// `level`, their random streams from `seed`.
    pub fn new(party: PlayerId, level: Difficulty, seed: u64) -> Reading {
        let knobs = Knobs {
            slip_percent: 0,
            rehearse: false,
            ..level.knobs()
        };
        let shadows = personality::all()
            .iter()
            .enumerate()
            .map(|(k, persona)| Shadow {
                mind: Mind::seat(
                    MindConfig {
                        party,
                        difficulty: level,
                        personality: k,
                        seed: seed ^ (k as u64) << 40,
                    },
                    knobs,
                    persona,
                    Captain::new(),
                ),
                miss: 0.5,
                looks: 0,
            })
            .collect();
        Reading {
            party,
            shadows,
            last: Vec::new(),
            next_look: 0,
            out: Vec::new(),
            told: None,
        }
    }

    pub fn party(&self) -> PlayerId {
        self.party
    }

    /// Let every shadow think on `view`, and judge them once a second.
    pub fn tick(&mut self, view: &Perception, terrain: &Arc<Terrain>, world: &World) {
        for s in &mut self.shadows {
            self.out.clear();
            s.mind.tick(view, terrain, world, &mut self.out);
        }
        if view.tick >= self.next_look {
            self.next_look = view.tick + SAMPLE;
            self.look(view);
        }
    }

    /// Judge every shadow by where the enemy's regiments have gone since
    /// the last look.
    fn look(&mut self, view: &Perception) {
        let party = self.party;
        let now: Vec<(RegimentHandle, Vec2)> = view
            .regiments
            .iter()
            .filter(|r| r.owner == party && !r.broken())
            .map(|r| (r.handle, r.pos))
            .collect();
        // Once the fighting starts, regiments go where the fight takes
        // them rather than where a plan puts them: judge the approach only,
        // and keep only the shadow that fits best.
        let fighting = view.regiments.iter().any(|r| r.owner == party && r.engaged);
        if fighting {
            if let Some(best) = self.best() {
                self.shadows.swap(0, best);
                self.shadows.truncate(1);
            } else {
                self.shadows.clear();
            }
            self.last = now;
            return;
        }
        for s in &mut self.shadows {
            let captain = s.mind.captain();
            if !captain.started() {
                continue;
            }
            let (mut sum, mut n) = (0.0, 0);
            for &(h, pos) in &now {
                let Some(r) = view.get(h) else { continue };
                // In melee it goes where the fight takes it.
                if r.engaged {
                    continue;
                }
                let Some(place) = captain.place_of(h) else {
                    continue;
                };
                let Some(&(_, before)) = self.last.iter().find(|(k, _)| *k == h) else {
                    continue;
                };
                sum += miss(before, pos, place);
                n += 1;
            }
            if n > 0 {
                let m = sum / n as f32;
                s.miss = if s.looks == 0 {
                    m
                } else {
                    s.miss + FRESH * (m - s.miss)
                };
                s.looks += 1;
            }
        }
        self.last = now;
    }

    /// Which shadow fits best so far, if any has been judged.
    fn best(&self) -> Option<usize> {
        (0..self.shadows.len())
            .filter(|&k| self.shadows[k].looks > 0)
            .min_by(|&a, &b| self.shadows[a].miss.total_cmp(&self.shadows[b].miss))
    }

    /// The commander that fits best so far, if any shadow has been judged.
    pub fn guess(&self) -> Option<Guess> {
        self.best().map(|k| {
            let s = &self.shadows[k];
            Guess {
                party: self.party,
                persona: s.mind.personality(),
                captain: s.mind.captain().clone(),
                fit: 1.0 - s.miss,
            }
        })
    }

    /// Whether `guess` names a different temper from the last one passed
    /// here, so the journal tells of each change once.
    pub fn news(&mut self, guess: &Guess) -> bool {
        let new = !self.told.is_some_and(|t| std::ptr::eq(t, guess.persona));
        self.told = Some(guess.persona);
        new
    }

    /// Every temper and how well it fits, best first (for the journal and
    /// tests).
    pub fn fits(&self) -> Vec<(&'static str, f32)> {
        let mut all: Vec<(&'static str, f32)> = self
            .shadows
            .iter()
            .filter(|s| s.looks > 0)
            .map(|s| (s.mind.personality().name.as_str(), 1.0 - s.miss))
            .collect();
        all.sort_by(|a, b| b.1.total_cmp(&a.1));
        all
    }
}

/// How badly a regiment that went from `before` to `now` bears out a
/// shadow that wants it at `place`: 0 (in its place, or heading straight
/// for it) to 1 (heading straight away, or standing far from it).
fn miss(before: Vec2, now: Vec2, place: Vec2) -> f32 {
    let to = place - now;
    let gap = to.length();
    if gap < THERE {
        return 0.0;
    }
    let step = now - before;
    if step.length() < STILL {
        return (gap / FAR).min(1.0);
    }
    0.5 * (1.0 - step.normalize().dot(to / gap))
}
