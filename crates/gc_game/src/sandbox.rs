//! Sandbox tools: the unit stats you can tune live, and reloading
//! `data/units.ron` from disk while the game runs.

use std::path::{Path, PathBuf};

use gc_assets::Pack;
use gc_data::{GameData, UnitRecord};
use gc_fixed::Fx;
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

/// One row of the tuning panel: how to show a stat and how to change it by
/// a number of steps (negative lowers it).
pub struct Tunable {
    pub label: &'static str,
    pub show: fn(&UnitStats) -> String,
    pub nudge: fn(&mut UnitStats, i32),
}

const QUARTER_TILE: i32 = 1 << 14;
const TILE: i32 = 1 << 16;

pub const TUNABLES: &[Tunable] = &[
    Tunable {
        label: "speed",
        show: |s| format!("{:.2} tiles/s", fx(s.speed)),
        nudge: |s, d| s.speed = Fx::from_raw((s.speed.raw() + d * QUARTER_TILE).max(0)),
    },
    Tunable {
        label: "turn rate",
        show: |s| format!("{:.0} deg/s", s.turn_rate as f32 * 360.0 / 65536.0),
        nudge: |s, d| {
            let deg = s.turn_rate as f32 * 360.0 / 65536.0 + 15.0 * d as f32;
            s.turn_rate = (deg.max(15.0) * 65536.0 / 360.0).round() as u32;
        },
    },
    Tunable {
        label: "melee attack",
        show: |s| s.melee_attack.to_string(),
        nudge: |s, d| s.melee_attack = percent(s.melee_attack, d * 5),
    },
    Tunable {
        label: "melee defence",
        show: |s| s.melee_defence.to_string(),
        nudge: |s, d| s.melee_defence = percent(s.melee_defence, d * 5),
    },
    Tunable {
        label: "melee damage",
        show: |s| s.melee_damage.to_string(),
        nudge: |s, d| s.melee_damage = count(s.melee_damage, d),
    },
    Tunable {
        label: "charge bonus",
        show: |s| format!("{}%", s.charge_bonus),
        nudge: |s, d| s.charge_bonus = (i32::from(s.charge_bonus) + d * 5).clamp(0, 255) as u8,
    },
    Tunable {
        label: "range",
        show: |s| format!("{:.0} tiles", fx(s.range)),
        nudge: |s, d| s.range = Fx::from_raw((s.range.raw() + d * TILE).max(0)),
    },
    Tunable {
        label: "missile damage",
        show: |s| s.missile_damage.to_string(),
        nudge: |s, d| s.missile_damage = count(s.missile_damage, d),
    },
    Tunable {
        label: "reload",
        show: |s| {
            let secs = f32::from(s.reload_ticks) / gc_sim::TICKS_PER_SECOND as f32;
            format!("{secs:.2} s")
        },
        nudge: |s, d| s.reload_ticks = count(s.reload_ticks, d * 5).max(1),
    },
    Tunable {
        label: "accuracy",
        show: |s| format!("{}%", s.accuracy),
        nudge: |s, d| s.accuracy = percent(s.accuracy, d * 5),
    },
    Tunable {
        label: "morale",
        show: |s| match s.morale {
            0 => "never routs".into(),
            m => m.to_string(),
        },
        nudge: |s, d| s.morale = percent(s.morale, d * 5),
    },
    Tunable {
        label: "mass",
        show: |s| s.mass.name().to_string(),
        nudge: |s, d| {
            let m = (s.mass as i32 + d).clamp(0, Mass::Heavy as i32);
            s.mass = Mass::from_u8(m as u8).unwrap_or_default();
        },
    },
    Tunable {
        label: "spears (reach)",
        show: |s| yes_no(s.reach),
        nudge: |s, d| s.reach = d > 0,
    },
    Tunable {
        label: "swinging blades",
        show: |s| yes_no(s.swing_room),
        nudge: |s, d| s.swing_room = d > 0,
    },
    Tunable {
        label: "shields",
        show: |s| yes_no(s.shield),
        nudge: |s, d| s.shield = d > 0,
    },
    Tunable {
        label: "hit points",
        show: |s| format!("{} (new regiments)", s.hp_per_model),
        nudge: |s, d| s.hp_per_model = count(s.hp_per_model, d).max(1),
    },
    Tunable {
        label: "soldiers",
        show: |s| format!("{} (new regiments)", s.models),
        nudge: |s, d| {
            s.models = (i32::from(s.models) + d * 5)
                .clamp(1, i32::from(gc_sim::formation::MAX_MODELS)) as u16;
            s.files = s.files.min(s.models);
        },
    },
];

fn yes_no(v: bool) -> String {
    if v { "yes" } else { "no" }.to_string()
}

fn fx(v: Fx) -> f32 {
    v.raw() as f32 / 65536.0
}

fn percent(v: u8, d: i32) -> u8 {
    (i32::from(v) + d).clamp(0, 100) as u8
}

fn count(v: u16, d: i32) -> u16 {
    (i32::from(v) + d).clamp(0, i32::from(u16::MAX)) as u16
}

/// Where `data/` is when the game runs from a checkout of the repository:
/// `GRUBCRAFT_DATA` if set, else `data/` under the working directory, else
/// the one this build was made from.
pub fn data_dir() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("GRUBCRAFT_DATA").map(PathBuf::from),
        Some(PathBuf::from("data")),
        Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|d| d.join("units.ron").is_file())
}

/// Unit types read fresh from `dir`, checked by the same rules as the build:
/// their names and stats.
pub fn reload(dir: &Path) -> Result<(Vec<String>, Vec<UnitStats>), String> {
    let baked = gc_bake::bake(dir).map_err(|e| e.to_string())?;
    // A pack must be 16-byte aligned to be read in place.
    let mut store = vec![0u128; baked.pack.len().div_ceil(16)];
    let bytes: &mut [u8] = bytemuck::cast_slice_mut(&mut store);
    bytes[..baked.pack.len()].copy_from_slice(&baked.pack);
    let pack = Pack::parse(&bytes[..baked.pack.len()]).map_err(|e| e.to_string())?;
    let data = GameData::from_pack(&pack).map_err(|e| e.to_string())?;
    Ok((
        data.units.iter().map(|u| u.name().to_string()).collect(),
        data.units.iter().map(stats_of).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nudges_stay_in_range() {
        let mut s = stats_of(&crate::data::load().units[0]);
        for t in TUNABLES {
            (t.nudge)(&mut s, -1000);
            assert_eq!(s, s.sanitized(), "{} went out of range", t.label);
            (t.nudge)(&mut s, 1000);
            assert_eq!(s, s.sanitized(), "{} went out of range", t.label);
            assert!(!(t.show)(&s).is_empty());
        }
    }

    #[test]
    fn speed_steps_by_a_quarter_tile() {
        let mut s = stats_of(&crate::data::load().units[0]);
        let before = s.speed;
        (TUNABLES[0].nudge)(&mut s, 2);
        assert_eq!(s.speed.raw() - before.raw(), 1 << 15);
    }

    #[test]
    fn reload_matches_the_baked_data() {
        let dir = data_dir().expect("data/ from the repository");
        let (names, stats) = reload(&dir).expect("data reloads");
        let data = crate::data::load();
        assert_eq!(names.len(), data.units.len());
        for (u, s) in data.units.iter().zip(&stats) {
            assert_eq!(stats_of(u), *s);
        }
    }
}
