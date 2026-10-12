//! Difficulty levels (docs/18-ai.md, "Difficulty").
//!
//! Every level plays by exactly the same rules and sees exactly what its
//! player could see. Levels differ only in how well they think: how quickly
//! they react, how many orders a minute they can give, which tactics they
//! know, and how often they slip. The fourth level, Warlord, rehearses its
//! battles in a copy of the real simulation and arrives with AI-2.

use crate::captain::Doctrine;
use crate::perception::TICKS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Difficulty {
    Recruit,
    Veteran,
    General,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [
        Difficulty::Recruit,
        Difficulty::Veteran,
        Difficulty::General,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Recruit => "Recruit",
            Difficulty::Veteran => "Veteran",
            Difficulty::General => "General",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: usize) -> Difficulty {
        Difficulty::ALL[i.min(Difficulty::ALL.len() - 1)]
    }

    pub fn knobs(self) -> Knobs {
        match self {
            Difficulty::Recruit => Knobs {
                reaction_ticks: 30,
                orders_per_minute: 20,
                think_ticks: 10,
                doctrines: &[Doctrine::HoldTheLine, Doctrine::HammerAndAnvil],
                reserve: false,
                morale_targeting: false,
                timed_hammer: false,
                save_wavering: false,
                focus_fire: false,
                engage_tiles: 14.0,
                hunt_leash: 120.0,
                slip_percent: 35,
                slip_gap_ticks: 10 * 20,
            },
            Difficulty::Veteran => Knobs {
                reaction_ticks: 16,
                orders_per_minute: 45,
                think_ticks: 7,
                doctrines: &Doctrine::ALL,
                reserve: true,
                morale_targeting: false,
                timed_hammer: true,
                save_wavering: true,
                focus_fire: true,
                engage_tiles: 10.0,
                hunt_leash: 55.0,
                slip_percent: 10,
                slip_gap_ticks: 30 * 20,
            },
            Difficulty::General => Knobs {
                reaction_ticks: 8,
                orders_per_minute: 90,
                think_ticks: 7,
                doctrines: &Doctrine::ALL,
                reserve: true,
                morale_targeting: true,
                timed_hammer: true,
                save_wavering: true,
                focus_fire: true,
                engage_tiles: 9.0,
                hunt_leash: 40.0,
                slip_percent: 2,
                slip_gap_ticks: 90 * 20,
            },
        }
    }
}

/// What a difficulty level changes.
#[derive(Clone, Copy, Debug)]
pub struct Knobs {
    /// Ticks between deciding an order and sending it (1.5 s to 0.4 s).
    pub reaction_ticks: u32,
    /// Orders it can give in a minute. One group order (a line moving
    /// together) counts once, like a player's right-drag.
    pub orders_per_minute: u32,
    /// Ticks between looks at the battle.
    pub think_ticks: u32,
    /// Battle plans it knows.
    pub doctrines: &'static [Doctrine],
    /// Keeps a regiment back to plug gaps and catch flankers.
    pub reserve: bool,
    /// Picks targets to start rout cascades and avoids wasting effort on
    /// regiments that never rout.
    pub morale_targeting: bool,
    /// Holds its hammer until the enemy line is locked in melee.
    pub timed_hammer: bool,
    /// Pulls wavering regiments out of the line before they break.
    pub save_wavering: bool,
    /// Several regiments gang up on one enemy to make local odds.
    pub focus_fire: bool,
    /// How close (tiles between blocks) an enemy gets before a line
    /// regiment charges it.
    pub engage_tiles: f32,
    /// How far from its army a hunter chases routers, tiles.
    pub hunt_leash: f32,
    /// Chance, percent, of a deliberate slip each time it could make one
    /// (charging too early, chasing too far, leaving a flank open).
    pub slip_percent: u32,
    /// Shortest time between two slips, ticks.
    pub slip_gap_ticks: u32,
}

impl Knobs {
    pub fn knows(&self, d: Doctrine) -> bool {
        self.doctrines.contains(&d)
    }

    /// Reaction time in seconds, for display.
    pub fn reaction_seconds(&self) -> f32 {
        self.reaction_ticks as f32 / TICKS
    }
}
