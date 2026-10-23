//! Difficulty levels (docs/18-ai.md, "Difficulty").
//!
//! Every level plays by exactly the same rules and sees exactly what its
//! player could see. Levels differ only in how well they think: how quickly
//! they react, how many orders a minute they can give, which tactics they
//! know, and how often they slip. The fourth, Warlord, also rehearses its
//! big decisions in a scratch copy of the battle (see [`crate::rehearsal`]).

use crate::captain::Doctrine;
use gc_sim::TICKS_PER_SECOND;

use crate::perception::TICKS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Difficulty {
    Recruit,
    Veteran,
    General,
    Warlord,
}

impl Difficulty {
    pub const ALL: [Difficulty; 4] = [
        Difficulty::Recruit,
        Difficulty::Veteran,
        Difficulty::General,
        Difficulty::Warlord,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Recruit => "Recruit",
            Difficulty::Veteran => "Veteran",
            Difficulty::General => "General",
            Difficulty::Warlord => "Warlord",
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
                ground: false,
                read_ground: false,
                team_play: true,
                engage_tiles: 14.0,
                hunt_leash: 120.0,
                slip_percent: 35,
                slip_gap_ticks: 10 * TICKS_PER_SECOND,
                rehearse: false,
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
                ground: true,
                read_ground: true,
                team_play: true,
                engage_tiles: 10.0,
                hunt_leash: 55.0,
                slip_percent: 10,
                slip_gap_ticks: 30 * TICKS_PER_SECOND,
                rehearse: false,
            },
            Difficulty::General => Knobs {
                reaction_ticks: 16,
                orders_per_minute: 90,
                think_ticks: 4,
                doctrines: &Doctrine::ALL,
                reserve: true,
                morale_targeting: true,
                timed_hammer: true,
                save_wavering: true,
                focus_fire: true,
                ground: true,
                read_ground: true,
                team_play: true,
                engage_tiles: 12.0,
                hunt_leash: 40.0,
                slip_percent: 2,
                slip_gap_ticks: 90 * TICKS_PER_SECOND,
                rehearse: false,
            },
            Difficulty::Warlord => Knobs {
                reaction_ticks: 5,
                orders_per_minute: 150,
                think_ticks: 4,
                doctrines: &Doctrine::ALL,
                reserve: true,
                morale_targeting: true,
                timed_hammer: true,
                save_wavering: true,
                focus_fire: true,
                ground: true,
                read_ground: true,
                team_play: true,
                engage_tiles: 12.0,
                hunt_leash: 40.0,
                slip_percent: 0,
                slip_gap_ticks: 90 * TICKS_PER_SECOND,
                rehearse: true,
            },
        }
    }
}

/// What a difficulty level changes.
#[derive(Clone, Copy, Debug)]
pub struct Knobs {
    /// Ticks between deciding an order and sending it (1.5 s to 0.25 s).
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
    /// Several regiments gang up on one enemy to make local odds, each
    /// where there is room for it beside the others (rather than pushing
    /// into them, which throws both out of order).
    pub focus_fire: bool,
    /// Picks its ground: anchors a flank on a cliff, takes high ground,
    /// waits behind water for the enemy to cross.
    pub ground: bool,
    /// Reads what the ground does in a fight (docs/18-ai.md, "Reading the
    /// ground"): holds a hill rather than attack up one, shelters from
    /// better archers in a wood, keeps its cavalry out of the trees, and
    /// crosses water at a ford, together.
    pub read_ground: bool,
    /// Plays its part in a team game (docs/18-ai.md, "Team games"): weighs
    /// the fight against the enemies coming at its side, not every army on
    /// the field; takes its own stretch of a front shared with its allies;
    /// lets two other sides wear each other down.
    pub team_play: bool,
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
    /// Rehearses its plan in a scratch copy of the battle before committing
    /// to it (see [`crate::rehearsal`]).
    pub rehearse: bool,
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
