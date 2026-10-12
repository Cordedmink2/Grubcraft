//! Raising and placing an army for a Field Battle (docs/06-game-modes.md).
//!
//! Until production arrives with M3, a Field Battle army is a fixed number
//! of regiments picked from its faction's roster by a preset mix, and set
//! out in a standard deployment: the line in front, archers behind it, fast
//! troops on the wings. The same code raises the AI's army and the player's,
//! so neither side starts better placed.

use gc_sim::{Command, PlayerCommand, PlayerId, UnitId, UnitStats};
use glam::Vec2;
use serde::Deserialize;

use crate::perception::{Class, fx, right_of, to_fx_vec, vec_angle};
use crate::personality::Faction;
use crate::terrain::Terrain;

/// Regiments in a Field Battle army.
pub const ARMY_REGIMENTS: usize = 10;

/// The army mixes a personality can build.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Preset {
    /// Half line, the rest split between fast troops and archers.
    Balanced,
    /// Heavy on fast troops.
    Aggressive,
    /// Heavy on line and archers.
    Defensive,
}

impl Preset {
    /// Shares of line, shock and ranged regiments, out of 12.
    fn shares(self) -> [u32; 3] {
        match self {
            Preset::Balanced => [6, 3, 3],
            Preset::Aggressive => [5, 5, 2],
            Preset::Defensive => [6, 2, 4],
        }
    }
}

const CLASSES: [Class; 3] = [Class::Line, Class::Shock, Class::Ranged];

/// Pick `size` regiments for an army of `faction` from the unit table
/// (`factions[i]` is unit `i`'s faction). Classes the faction doesn't have
/// are made up from the ones it does; types within a class take turns.
pub fn pick_army(
    units: &[UnitStats],
    factions: &[Faction],
    faction: Faction,
    preset: Preset,
    size: usize,
) -> Vec<UnitId> {
    let mut mine: Vec<UnitId> = (0..units.len())
        .filter(|&i| factions.get(i) == Some(&faction))
        .map(|i| i as UnitId)
        .collect();
    if mine.is_empty() {
        // A faction with no regiments of its own yet borrows everyone's.
        mine = (0..units.len() as UnitId).collect();
    }
    let of_class = |c: Class| -> Vec<UnitId> {
        mine.iter()
            .copied()
            .filter(|&u| Class::of(&units[u as usize]) == c)
            .collect()
    };
    let types: [Vec<UnitId>; 3] = CLASSES.map(of_class);
    let shares = preset.shares();
    let mut counts = [0usize; 3];
    for c in 0..3 {
        if !types[c].is_empty() {
            counts[c] = (size * shares[c] as usize + 6) / 12;
        }
    }
    // Round to exactly `size`, adding to (or taking from) the line first.
    let order = [0usize, 1, 2];
    while counts.iter().sum::<usize>() < size {
        let c = order.iter().copied().find(|&c| !types[c].is_empty());
        match c {
            Some(c) => counts[c] += 1,
            None => break,
        }
    }
    while counts.iter().sum::<usize>() > size {
        let c = order
            .iter()
            .copied()
            .rev()
            .find(|&c| counts[c] > 0)
            .unwrap();
        counts[c] -= 1;
    }
    let mut army = Vec::with_capacity(size);
    for c in 0..3 {
        for k in 0..counts[c] {
            army.push(types[c][k % types[c].len()]);
        }
    }
    army
}

/// Where one regiment of an army starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub unit: UnitId,
    pub position: Vec2,
    pub facing: Vec2,
}

/// Space between regiments side by side, and between the rows, tiles.
const GAP: f32 = 2.0;
const ROW_GAP: f32 = 4.0;

/// Set `army` out in the standard deployment around `centre`, facing
/// `forward`: line regiments in a row, archers behind them, fast troops on
/// the wings. Places that aren't standable ground are nudged onto some.
pub fn deploy(
    units: &[UnitStats],
    army: &[UnitId],
    centre: Vec2,
    forward: Vec2,
    terrain: &Terrain,
) -> Vec<Placement> {
    let forward = forward.normalize_or(Vec2::Y);
    let right = right_of(forward);
    let size = |u: UnitId| {
        let s = &units[u as usize];
        let (w, d) = s.block_size(s.files, s.models);
        (fx(w), fx(d))
    };
    let class = |u: UnitId| Class::of(&units[u as usize]);
    let mut line: Vec<UnitId> = army
        .iter()
        .copied()
        .filter(|&u| class(u) == Class::Line)
        .collect();
    let mut shock: Vec<UnitId> = army
        .iter()
        .copied()
        .filter(|&u| class(u) == Class::Shock)
        .collect();
    let ranged: Vec<UnitId> = army
        .iter()
        .copied()
        .filter(|&u| class(u) == Class::Ranged)
        .collect();
    // All fast troops: the middle ones stand in the line.
    if line.is_empty() && shock.len() >= 3 {
        let wings = shock.len() / 3;
        line = shock.split_off(wings);
    }
    let row_width = |row: &[UnitId]| {
        row.iter().map(|&u| size(u).0 + GAP).sum::<f32>() - GAP.min(row.len() as f32 * GAP)
    };
    let line_depth = line.iter().map(|&u| size(u).1).fold(0.0, f32::max);
    let mut out = Vec::with_capacity(army.len());
    let mut put = |unit: UnitId, across: f32, back: f32| {
        let p = centre + right * across - forward * back;
        let p = terrain
            .nearest_walkable(p, 12.0)
            .unwrap_or(terrain.clamp(p, 4.0));
        out.push(Placement {
            unit,
            position: p,
            facing: forward,
        });
    };
    // The line.
    let width = row_width(&line);
    let mut x = -width * 0.5;
    for &u in &line {
        let w = size(u).0;
        put(u, x + w * 0.5, 0.0);
        x += w + GAP;
    }
    // Archers behind.
    let rw = row_width(&ranged);
    let mut x = -rw * 0.5;
    for &u in &ranged {
        let (w, d) = size(u);
        put(u, x + w * 0.5, line_depth * 0.5 + ROW_GAP + d * 0.5);
        x += w + GAP;
    }
    // Fast troops on the wings, alternating sides from the line outward.
    let (mut left, mut right_edge) = (-width * 0.5 - GAP * 2.0, width * 0.5 + GAP * 2.0);
    for (k, &u) in shock.iter().enumerate() {
        let w = size(u).0;
        if k % 2 == 0 {
            put(u, right_edge + w * 0.5, 0.0);
            right_edge += w + GAP;
        } else {
            put(u, left - w * 0.5, 0.0);
            left -= w + GAP;
        }
    }
    out
}

/// The commands that put a deployed army on the field for `party`.
pub fn spawn_commands(party: PlayerId, placements: &[Placement], out: &mut Vec<PlayerCommand>) {
    out.extend(placements.iter().map(|p| PlayerCommand {
        player: party,
        command: Command::SpawnRegiment {
            unit: p.unit,
            position: to_fx_vec(p.position),
            facing: vec_angle(p.facing),
        },
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_sim::scenario::DRILL_UNITS;

    fn factions() -> Vec<Faction> {
        // Spears, fast troops and archers for the Clans; pikes for the Concord.
        vec![
            Faction::EmberClans,
            Faction::EmberClans,
            Faction::EmberClans,
            Faction::CarrionConcord,
        ]
    }

    #[test]
    fn presets_mix_the_classes() {
        let f = factions();
        let balanced = pick_army(&DRILL_UNITS, &f, Faction::EmberClans, Preset::Balanced, 12);
        assert_eq!(balanced.len(), 12);
        let n = |a: &[UnitId], u: UnitId| a.iter().filter(|&&x| x == u).count();
        assert_eq!(
            (n(&balanced, 0), n(&balanced, 1), n(&balanced, 2)),
            (6, 3, 3)
        );
        let defensive = pick_army(&DRILL_UNITS, &f, Faction::EmberClans, Preset::Defensive, 10);
        assert_eq!(defensive.len(), 10);
        assert!(n(&defensive, 2) > n(&defensive, 1));
    }

    #[test]
    fn a_faction_with_one_type_fields_all_of_it() {
        let army = pick_army(
            &DRILL_UNITS,
            &factions(),
            Faction::CarrionConcord,
            Preset::Aggressive,
            ARMY_REGIMENTS,
        );
        assert_eq!(army, vec![3; ARMY_REGIMENTS]);
    }

    #[test]
    fn deployment_puts_archers_behind_and_nobody_on_top_of_anyone() {
        let army = pick_army(
            &DRILL_UNITS,
            &factions(),
            Faction::EmberClans,
            Preset::Balanced,
            12,
        );
        let centre = Vec2::new(100.0, 150.0);
        let forward = Vec2::new(0.0, -1.0);
        let placed = deploy(&DRILL_UNITS, &army, centre, forward, &Terrain::open(256));
        assert_eq!(placed.len(), 12);
        for p in &placed {
            let ahead = (p.position - centre).dot(forward);
            if p.unit == 2 {
                assert!(ahead < -4.0, "archers behind the line: {p:?}");
            } else {
                assert!(ahead.abs() < 0.01, "{p:?}");
            }
        }
        for (i, a) in placed.iter().enumerate() {
            for b in &placed[i + 1..] {
                assert!(a.position.distance(b.position) > 4.0, "{a:?} {b:?}");
            }
        }
    }
}
