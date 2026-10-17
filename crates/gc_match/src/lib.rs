//! A match without a window (review fix 13): everything a Field Battle needs
//! that has no window, GPU or sound, so the game, the golden replays, the
//! computer players' ladders and the benchmarks all set out, play and judge
//! battles the same way. The map itself comes from `gc_map`.
//!
//! - [`battle`]: raising and placing each side's army from the lobby,
//!   deployment zones, and judging who holds the field.
//! - [`terrain_view`]: the float views of a map that deployment and the
//!   computer players use.
//! - [`play`]: a whole battle driven headless, tick by tick.
//! - [`Tables`]: the unit tables a battle is set out with.

pub mod battle;
#[cfg(test)]
mod battle_tables;
#[cfg(test)]
mod online_tests;
pub mod play;
pub mod terrain_view;

use gc_ai::Faction;
use gc_data::{GameData, UnitRecord};
use gc_sim::{Mass, UnitStats};

/// The simulation's stats for a unit type from the game data.
pub fn stats_of(u: &UnitRecord) -> UnitStats {
    UnitStats {
        speed: u.speed(),
        turn_rate: u.turn_rate,
        models: u.models,
        hp_per_model: u.hp_per_model,
        files: u.files,
        file_spacing: u.file_spacing(),
        rank_spacing: u.rank_spacing(),
        melee_attack: u.melee_attack,
        melee_defence: u.melee_defence,
        melee_damage: u.melee_damage,
        charge_bonus: u.charge_bonus,
        range: u.range(),
        missile_damage: u.missile_damage,
        reload_ticks: u.reload_ticks,
        accuracy: u.accuracy,
        morale: u.morale,
        mass: Mass::from_u8(u.mass).unwrap_or_default(),
        reach: u.reach(),
        swing_room: u.swing_room(),
        shield: u.shield(),
    }
}

/// The game data's unit types as a battle is set out with them, by unit
/// index: the simulation's stats, each type's faction and its points.
pub struct Tables {
    pub units: Vec<UnitStats>,
    pub factions: Vec<Faction>,
    pub points: Vec<u32>,
}

impl Tables {
    pub fn new(data: &GameData) -> Tables {
        Tables {
            units: data.units.iter().map(stats_of).collect(),
            factions: (data.units.iter())
                .map(|u| Faction::from_index(u.faction() as u8))
                .collect(),
            points: data.units.iter().map(|u| u32::from(u.points)).collect(),
        }
    }
}

/// The real game data, baked from `data/` the way the game's build bakes
/// it, for tests.
#[cfg(test)]
pub(crate) fn test_data() -> &'static GameData<'static> {
    use std::sync::OnceLock;
    static DATA: OnceLock<GameData<'static>> = OnceLock::new();
    DATA.get_or_init(|| {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let pack = gc_bake::bake(&dir).expect("bake data/").pack;
        // A pack is read in place, so it must start 16-byte aligned.
        let words: &'static mut [u128] = vec![0; pack.len().div_ceil(16)].leak();
        let bytes: &'static mut [u8] = bytemuck::cast_slice_mut(words);
        bytes[..pack.len()].copy_from_slice(&pack);
        let pack = gc_assets::Pack::parse(&bytes[..pack.len()]).expect("baked pack");
        GameData::from_pack(&pack).expect("baked data")
    })
}
