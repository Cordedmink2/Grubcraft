//! How a Conquest mind's level plays the long game (docs/18-ai.md,
//! "Difficulty"): the economy it keeps up and how it picks its fights. The
//! battle side of a level is in [`crate::difficulty::Knobs`].

use super::opening::Opening;
use super::spymaster::Read;
use crate::difficulty::Difficulty;
use crate::perception::TICKS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Strategy {
    /// Share of the workers its nodes take that it keeps, percent: a
    /// Recruit leaves seeps part worked. (The Choir's workers only build,
    /// so it keeps its few whatever the level.)
    pub workers_percent: u8,
    /// Production buildings it keeps per tier.
    pub producers_per_tier: u8,
    /// Weighs its army against the enemy's, as far as it knows it, before
    /// it sets out, and goes for the enemy it has the best odds against.
    pub weighs_odds: bool,
    /// Trains towards what does well against the enemy army it has seen.
    pub counters: bool,
    /// Counts only the enemies near the target, so it strikes a base
    /// whose army is away.
    pub opportunist: bool,
    /// How soon it takes its first expansion, percent of the time its
    /// greed sets; `None` never expands until its seeps run low.
    pub expand_percent: Option<u8>,
    /// How it sends a scout to look at its enemies.
    pub scouting: Scouting,
    /// Changes its opening to answer what its scout has seen of an
    /// enemy's build (docs/18-ai.md, "Difficulty": "counters what it
    /// scouts").
    pub adapts: bool,
    /// In a game of several rival sides, goes for the leader first, or
    /// one that attacked it, and leaves the others be.
    pub gangs_up: bool,
    /// Plans timing pushes: arms for a set time, then goes with
    /// everything.
    pub pushes: bool,
    /// Sends a small fast raiding party at the enemy's workers once its
    /// army is big enough to spare one.
    pub raids: bool,
    /// What works it puts up (docs/26-fortifications.md, stage 8).
    pub fortifies: Fortify,
    /// What its opening changes while it lasts (docs/18-ai.md, "Steward"):
    /// the usual play outside one, see [`Strategy::opening`].
    pub early: Early,
}

/// The supply of the attack it sends at an enemy it has seen expand
/// early.
const PUNISH_WAVE: u8 = 10;

/// How long it keeps answering an enemy's early build, ticks: past it, the
/// usual rules.
pub const ANSWER_UNTIL: u32 = (8.0 * 60.0 * TICKS) as u32;

/// What works a level puts up against its enemies (see the Steward's
/// Engineer).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fortify {
    /// None at all.
    #[default]
    None,
    /// Field works only: a barricade at its rally point, and stakes there
    /// against riders and monsters.
    Field,
    /// Field works, a front wall with a gate (raised once it is rich),
    /// traps on the approach, and a bridge when water cuts it off.
    Full,
}

/// How a level scouts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scouting {
    /// Never sends a scout: it learns what its army runs into.
    #[default]
    Never,
    /// Sends one early look at the enemy starts, then stops.
    Early,
    /// Keeps a scout out all game, looking where it knows least.
    Always,
}

/// The early-game rules an opening sets, for its first minutes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Early {
    /// Workers it has before its first production building.
    pub workers_first: u8,
    /// Production buildings it puts up whatever its worker count, and
    /// while it is arming.
    pub producers: u8,
    /// Most workers it keeps.
    pub worker_cap: Option<u8>,
    /// Tick from which it expands, its seeps full or not.
    pub expand_at: Option<u32>,
    /// Defence towers it puts up by its hall.
    pub towers: u8,
    /// Supply its first attack sets out with.
    pub first_wave: Option<u8>,
}

impl Early {
    /// The usual play: six workers before production, nothing else.
    pub const USUAL: Early = Early {
        workers_first: 6,
        producers: 0,
        worker_cap: None,
        expand_at: None,
        towers: 0,
        first_wave: None,
    };
}

impl Strategy {
    /// The tick from which it expands once its seeps are full: greedy
    /// minds sooner, after 2.5 to 5 minutes at the usual pace.
    pub fn expand_after(&self, greed: u8) -> u32 {
        let Some(percent) = self.expand_percent else {
            return u32::MAX;
        };
        let seconds = 300.0 - 150.0 * f32::from(greed) / 100.0;
        (seconds * f32::from(percent) / 100.0 * TICKS) as u32
    }

    /// This plan with `opening`'s early rules, while it lasts.
    pub fn opening(self, opening: Option<&Opening>) -> Strategy {
        let Some(o) = opening else { return self };
        Strategy {
            early: Early {
                workers_first: o.workers_first,
                producers: o.producers,
                worker_cap: o.worker_cap,
                expand_at: o.expand_at.map(|m| (f32::from(m) * 60.0 * TICKS) as u32),
                towers: o.towers,
                first_wave: o.first_wave,
            },
            ..self
        }
    }

    /// This plan answering `read`, an enemy's early build its scout saw
    /// at tick `since`: towers and production against a coming attack, an
    /// early attack against a fast expansion, an expansion of its own
    /// against towers.
    pub fn answer(self, read: Option<Read>, since: u32) -> Strategy {
        let mut early = self.early;
        match read {
            None => return self,
            Some(Read::Rush) => {
                early.towers = early.towers.max(1);
                early.producers = early.producers.max(2);
                early.expand_at = None;
                early.first_wave = None;
            }
            Some(Read::Greedy) => {
                early.producers = early.producers.max(2);
                early.expand_at = None;
                early.first_wave =
                    Some(early.first_wave.map_or(PUNISH_WAVE, |w| w.min(PUNISH_WAVE)));
            }
            Some(Read::Walls) => {
                early.expand_at = Some(early.expand_at.map_or(since, |t| t.min(since)));
                early.first_wave = None;
            }
        }
        Strategy { early, ..self }
    }

    pub fn of(level: Difficulty) -> Strategy {
        match level {
            Difficulty::Recruit => Strategy {
                workers_percent: 60,
                producers_per_tier: 1,
                weighs_odds: false,
                counters: false,
                opportunist: false,
                expand_percent: None,
                scouting: Scouting::Never,
                adapts: false,
                gangs_up: false,
                pushes: false,
                raids: false,
                fortifies: Fortify::None,
                early: Early::USUAL,
            },
            Difficulty::Veteran => Strategy {
                workers_percent: 70,
                producers_per_tier: 2,
                weighs_odds: true,
                counters: false,
                opportunist: false,
                expand_percent: Some(100),
                scouting: Scouting::Early,
                adapts: false,
                gangs_up: false,
                pushes: false,
                raids: false,
                fortifies: Fortify::Field,
                early: Early::USUAL,
            },
            Difficulty::General => Strategy {
                workers_percent: 85,
                producers_per_tier: 2,
                weighs_odds: true,
                counters: true,
                opportunist: false,
                expand_percent: Some(100),
                scouting: Scouting::Always,
                adapts: true,
                gangs_up: true,
                pushes: false,
                raids: false,
                fortifies: Fortify::Full,
                early: Early::USUAL,
            },
            Difficulty::Warlord => Strategy {
                workers_percent: 100,
                producers_per_tier: 3,
                weighs_odds: true,
                counters: true,
                opportunist: true,
                expand_percent: Some(70),
                scouting: Scouting::Always,
                adapts: true,
                gangs_up: true,
                pushes: true,
                raids: true,
                fortifies: Fortify::Full,
                early: Early::USUAL,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_answers_a_rush_with_towers_and_a_fast_expansion_with_an_attack() {
        let plan = Strategy::of(Difficulty::General);
        assert_eq!(plan.answer(None, 100), plan);
        let rush = plan.answer(Some(Read::Rush), 100).early;
        assert!(rush.towers >= 1 && rush.producers >= 2 && rush.expand_at.is_none());
        let greedy = plan.answer(Some(Read::Greedy), 100).early;
        assert_eq!(greedy.first_wave, Some(PUNISH_WAVE));
        let walls = plan.answer(Some(Read::Walls), 100).early;
        assert_eq!(walls.expand_at, Some(100));
        // An opening's own early expansion stays if it is sooner.
        let mut early = plan;
        early.early.expand_at = Some(50);
        assert_eq!(
            early.answer(Some(Read::Walls), 100).early.expand_at,
            Some(50)
        );
    }
}
