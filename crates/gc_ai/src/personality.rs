//! Personalities (docs/18-ai.md, "Personalities and faction doctrine"):
//! a handful of traits that make each AI opponent recognisable. They live in
//! `data/ai/personalities.ron`, embedded here, so they can be tuned without
//! touching code.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::army::Preset;
use crate::captain::Doctrine;

/// The factions, in the same order as the game data's (`gc_data::Faction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Faction {
    EmberClans,
    Brood,
    Choir,
    CarrionConcord,
}

impl Faction {
    pub const ALL: [Faction; 4] = [
        Faction::EmberClans,
        Faction::Brood,
        Faction::Choir,
        Faction::CarrionConcord,
    ];

    pub fn index(self) -> u8 {
        self as u8
    }

    pub fn from_index(i: u8) -> Faction {
        Faction::ALL[i as usize % Faction::ALL.len()]
    }
}

/// How much a personality likes each battle plan, 0 to 100.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoctrineLikes {
    pub hold_the_line: u8,
    pub hammer_and_anvil: u8,
    pub refused_flank: u8,
    pub skirmish_screen: u8,
    pub ambush: u8,
}

impl DoctrineLikes {
    pub fn of(&self, d: Doctrine) -> f32 {
        let v = match d {
            Doctrine::HoldTheLine => self.hold_the_line,
            Doctrine::HammerAndAnvil => self.hammer_and_anvil,
            Doctrine::RefusedFlank => self.refused_flank,
            Doctrine::SkirmishScreen => self.skirmish_screen,
            Doctrine::Ambush => self.ambush,
        };
        v as f32 / 100.0
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Personality {
    pub name: String,
    pub faction: Faction,
    pub summary: String,
    pub aggression: u8,
    pub caution: u8,
    pub patience: u8,
    pub cunning: u8,
    pub opportunism: u8,
    pub greed: u8,
    pub honour: u8,
    pub vengefulness: u8,
    pub doctrines: DoctrineLikes,
    pub army: Preset,
}

impl Personality {
    /// A trait as 0 to 1.
    pub fn aggression(&self) -> f32 {
        self.aggression as f32 / 100.0
    }

    pub fn caution(&self) -> f32 {
        self.caution as f32 / 100.0
    }

    pub fn patience(&self) -> f32 {
        self.patience as f32 / 100.0
    }

    pub fn cunning(&self) -> f32 {
        self.cunning as f32 / 100.0
    }

    pub fn opportunism(&self) -> f32 {
        self.opportunism as f32 / 100.0
    }
}

const SOURCE: &str = include_str!("../../../data/ai/personalities.ron");

/// Every personality, in the order of the data file.
pub fn all() -> &'static [Personality] {
    static ALL: OnceLock<Vec<Personality>> = OnceLock::new();
    ALL.get_or_init(|| parse(SOURCE).expect("data/ai/personalities.ron"))
}

/// Parse a personalities file, checking every trait is in range.
pub fn parse(source: &str) -> Result<Vec<Personality>, String> {
    let list: Vec<Personality> = ron::from_str(source).map_err(|e| e.to_string())?;
    for p in &list {
        let traits = [
            p.aggression,
            p.caution,
            p.patience,
            p.cunning,
            p.opportunism,
            p.greed,
            p.honour,
            p.vengefulness,
        ];
        let d = &p.doctrines;
        let likes = [
            d.hold_the_line,
            d.hammer_and_anvil,
            d.refused_flank,
            d.skirmish_screen,
            d.ambush,
        ];
        if traits.iter().chain(&likes).any(|&v| v > 100) {
            return Err(format!("{}: traits run from 0 to 100", p.name));
        }
    }
    if list.is_empty() {
        return Err("no personalities".into());
    }
    Ok(list)
}

/// Index of the personality called `name`.
pub fn find(name: &str) -> Option<usize> {
    all().iter().position(|p| p.name == name)
}

/// The personalities of a faction, as indices into [`all`].
pub fn of_faction(faction: Faction) -> impl Iterator<Item = usize> {
    all()
        .iter()
        .enumerate()
        .filter(move |(_, p)| p.faction == faction)
        .map(|(i, _)| i)
}

/// A personality of `faction` picked by `roll` (any number; the same roll
/// picks the same one). Falls back to any personality if the faction has none.
pub fn pick(faction: Faction, roll: u32) -> usize {
    let mine: Vec<usize> = of_faction(faction).collect();
    if mine.is_empty() {
        roll as usize % all().len()
    } else {
        mine[roll as usize % mine.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_file_parses_with_two_per_faction() {
        let all = all();
        assert_eq!(all.len(), 8);
        for f in Faction::ALL {
            assert_eq!(of_faction(f).count(), 2, "{f:?}");
        }
        assert_eq!(
            find("The Hungering").map(|i| all[i].faction),
            Some(Faction::Brood)
        );
    }

    #[test]
    fn out_of_range_traits_are_rejected() {
        let bad = SOURCE.replacen("aggression: 30", "aggression: 130", 1);
        assert!(parse(&bad).is_err());
    }
}
