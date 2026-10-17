//! Openings (docs/18-ai.md, "Steward: economy and production"): how a
//! Conquest mind spends its first minutes, from `data/ai/openings.ron`.
//! The Steward and the Marshal follow the opening while it lasts, then
//! their usual rules.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::difficulty::Difficulty;
use crate::perception::TICKS;
use crate::personality::{Faction, Personality};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    pub name: String,
    pub summary: String,
    /// The personalities that play it; empty for every one.
    pub who: Vec<String>,
    /// Workers it has before its first production building.
    pub workers_first: u8,
    /// Production buildings it puts up early: the first at `workers_first`,
    /// the others whatever its worker count.
    pub producers: u8,
    /// Most workers while the opening lasts.
    pub worker_cap: Option<u8>,
    /// Minute from which it takes one more base, its seeps full or not.
    pub expand_at: Option<u8>,
    /// Defence towers it puts up by its hall.
    pub towers: u8,
    /// Supply its first attack sets out with.
    pub first_wave: Option<u8>,
    /// How long the opening lasts.
    pub minutes: u8,
    /// Factions it does well against, for a Warlord to pick it.
    pub against: Vec<Faction>,
}

impl Opening {
    /// The tick the opening ends.
    pub fn ends(&self) -> u32 {
        (f32::from(self.minutes) * 60.0 * TICKS) as u32
    }

    fn played_by(&self, persona: &Personality) -> bool {
        self.who.is_empty() || self.who.contains(&persona.name)
    }
}

const SOURCE: &str = include_str!("../../../../data/ai/openings.ron");

/// Every opening, in the order of the data file; the first is the plain
/// one every personality can play.
pub fn all() -> &'static [Opening] {
    static ALL: OnceLock<Vec<Opening>> = OnceLock::new();
    ALL.get_or_init(|| parse(SOURCE).expect("data/ai/openings.ron"))
}

/// The opening called `name`, whatever its case.
pub fn find(name: &str) -> Option<&'static Opening> {
    all()
        .iter()
        .find(|o| o.name.eq_ignore_ascii_case(name.trim()))
}

/// Parse an openings file, checking every personality it names exists and
/// the first opening is for everyone.
pub fn parse(source: &str) -> Result<Vec<Opening>, String> {
    let list: Vec<Opening> = ron::from_str(source).map_err(|e| e.to_string())?;
    if !list.first().is_some_and(|o| o.who.is_empty()) {
        return Err("the first opening must be for every personality".into());
    }
    for o in &list {
        if let Some(n) = o.who.iter().find(|n| crate::personality::find(n).is_none()) {
            return Err(format!("{}: no personality called {n}", o.name));
        }
        if o.workers_first == 0 || o.producers == 0 {
            return Err(format!("{}: it needs workers and production", o.name));
        }
    }
    Ok(list)
}

/// The opening a mind of `persona` at `level` plays, picked by `roll`. A
/// Recruit plays the plain one; a Warlord prefers one that does well
/// against the factions it faces (`rivals`).
pub fn pick(
    persona: &Personality,
    level: Difficulty,
    rivals: &[Faction],
    roll: u64,
) -> &'static Opening {
    let all = all();
    if level == Difficulty::Recruit {
        return &all[0];
    }
    let mine: Vec<&Opening> = all.iter().filter(|o| o.played_by(persona)).collect();
    let fits: Vec<&Opening> = mine
        .iter()
        .copied()
        .filter(|o| o.against.iter().any(|f| rivals.contains(f)))
        .collect();
    let from = if level == Difficulty::Warlord && !fits.is_empty() {
        fits
    } else {
        mine
    };
    from[(roll % from.len() as u64) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personality;

    #[test]
    fn the_data_file_parses_and_every_personality_has_a_choice() {
        let all = all();
        assert_eq!(all[0].name, "Steady");
        for p in personality::all() {
            let n = all.iter().filter(|o| o.played_by(p)).count();
            assert!(n >= 2, "{} has {n} openings", p.name);
        }
    }

    #[test]
    fn a_recruit_plays_steady_and_a_warlord_plays_to_its_rivals() {
        let hungering = &personality::all()[personality::find("The Hungering").unwrap()];
        for roll in 0..8 {
            assert_eq!(
                pick(hungering, Difficulty::Recruit, &[], roll).name,
                "Steady"
            );
            let o = pick(hungering, Difficulty::Warlord, &[Faction::Choir], roll);
            assert!(o.against.contains(&Faction::Choir), "{}", o.name);
        }
        let names: Vec<&str> = (0..8)
            .map(|r| pick(hungering, Difficulty::Veteran, &[], r).name.as_str())
            .collect();
        assert!(names.contains(&"Steady") && names.contains(&"Early pressure"));
    }

    #[test]
    fn a_bad_file_is_refused() {
        let base = SOURCE.replacen("\"The Red Banner\"", "\"Nobody\"", 1);
        assert!(parse(&base).unwrap_err().contains("Nobody"));
    }
}
