//! How a Conquest mind's level plays the long game (docs/18-ai.md,
//! "Difficulty"): the economy it keeps up and how it picks its fights. The
//! battle side of a level is in [`crate::difficulty::Knobs`].

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

    pub fn of(level: Difficulty) -> Strategy {
        match level {
            Difficulty::Recruit => Strategy {
                workers_percent: 60,
                producers_per_tier: 1,
                weighs_odds: false,
                counters: false,
                opportunist: false,
                expand_percent: None,
            },
            Difficulty::Veteran => Strategy {
                workers_percent: 70,
                producers_per_tier: 2,
                weighs_odds: true,
                counters: false,
                opportunist: false,
                expand_percent: Some(100),
            },
            Difficulty::General => Strategy {
                workers_percent: 85,
                producers_per_tier: 2,
                weighs_odds: true,
                counters: true,
                opportunist: false,
                expand_percent: Some(100),
            },
            Difficulty::Warlord => Strategy {
                workers_percent: 100,
                producers_per_tier: 3,
                weighs_odds: true,
                counters: true,
                opportunist: true,
                expand_percent: Some(70),
            },
        }
    }
}
