//! How a Conquest mind's level plays the long game (docs/18-ai.md,
//! "Difficulty"): the economy it keeps up and how it picks its fights. The
//! battle side of a level is in [`crate::difficulty::Knobs`].

use super::opening::Opening;
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
    /// What its opening changes while it lasts (docs/18-ai.md, "Steward"):
    /// the usual play outside one, see [`Strategy::opening`].
    pub early: Early,
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

    pub fn of(level: Difficulty) -> Strategy {
        match level {
            Difficulty::Recruit => Strategy {
                workers_percent: 60,
                producers_per_tier: 1,
                weighs_odds: false,
                counters: false,
                opportunist: false,
                expand_percent: None,
                early: Early::USUAL,
            },
            Difficulty::Veteran => Strategy {
                workers_percent: 70,
                producers_per_tier: 2,
                weighs_odds: true,
                counters: false,
                opportunist: false,
                expand_percent: Some(100),
                early: Early::USUAL,
            },
            Difficulty::General => Strategy {
                workers_percent: 85,
                producers_per_tier: 2,
                weighs_odds: true,
                counters: true,
                opportunist: false,
                expand_percent: Some(100),
                early: Early::USUAL,
            },
            Difficulty::Warlord => Strategy {
                workers_percent: 100,
                producers_per_tier: 3,
                weighs_odds: true,
                counters: true,
                opportunist: true,
                expand_percent: Some(70),
                early: Early::USUAL,
            },
        }
    }
}
