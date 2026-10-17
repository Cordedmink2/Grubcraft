//! The real rosters, baked into the pack's building and Conquest unit tables
//! and read back the way the game reads them.

use std::path::{Path, PathBuf};

use gc_assets::Pack;
use gc_data::conquest::{
    BUILDING_DROP_OFF_GRUB, BUILDING_HONOR, Class, METER_HONOR, METER_RESONANCE, NONE, POWER_GRANT,
    POWER_HASTE, POWER_MARCH, POWER_NOT_BUILT, POWER_SHIFT, POWER_STEADFAST,
};
use gc_data::{Faction, GameData};

fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

/// Bake the real data and run `check` on what `GameData::from_pack` reads.
fn with_real_data(check: impl FnOnce(&GameData)) {
    let baked = gc_bake::bake(&data_dir()).unwrap_or_else(|e| panic!("{e}"));
    // A pack must be 16-byte aligned to be read in place.
    let mut store = vec![0u128; baked.pack.len().div_ceil(16)];
    let bytes: &mut [u8] = bytemuck::cast_slice_mut(&mut store);
    bytes[..baked.pack.len()].copy_from_slice(&baked.pack);
    let pack = Pack::parse(&bytes[..baked.pack.len()]).unwrap();
    let data = GameData::from_pack(&pack).unwrap();
    assert_eq!(data.hash, baked.data_hash);
    assert_eq!(data.buildings.len(), baked.buildings);
    assert_eq!(data.conquest_units.len(), baked.conquest_units);
    check(&data);
}

#[test]
fn both_tables_read_back() {
    with_real_data(|data| {
        for faction in Faction::ALL {
            assert!(data.buildings.iter().any(|b| b.faction() == faction));
            assert!(data.conquest_units.iter().any(|u| u.faction() == faction));
        }
        // Factions in `Faction` order, each one's entries together.
        assert!(data.buildings.is_sorted_by_key(|b| b.faction));
        assert!(data.conquest_units.is_sorted_by_key(|u| u.stats.faction));
    });
}

#[test]
fn references_stay_inside_the_faction() {
    with_real_data(|data| {
        let mut upgrades = 0;
        for b in data.buildings {
            if b.upgrades_from != NONE {
                upgrades += 1;
                let from = &data.buildings[usize::from(b.upgrades_from)];
                assert_eq!(from.faction(), b.faction(), "{}", b.name());
            }
        }
        // Each faction's main building upgrades twice.
        assert_eq!(upgrades, 2 * Faction::ALL.len());
        for u in data.conquest_units {
            if u.trained_at != NONE {
                let at = &data.buildings[usize::from(u.trained_at)];
                assert_eq!(at.faction(), u.faction(), "{}", u.name());
            }
            if u.evolves_from != NONE {
                let from = &data.conquest_units[usize::from(u.evolves_from)];
                assert_eq!(from.faction(), u.faction(), "{}", u.name());
                assert_eq!(u.trained_at, NONE, "{}: evolved, not trained", u.name());
            }
        }
    });
}

#[test]
fn starting_state_matches_the_economy_doc() {
    with_real_data(|data| {
        let hall = data
            .buildings
            .iter()
            .find(|b| b.name() == "Clan Hall")
            .unwrap();
        assert_eq!(hall.start_count, 1);
        assert_eq!(hall.flags & BUILDING_DROP_OFF_GRUB, BUILDING_DROP_OFF_GRUB);
        assert_eq!(hall.faction(), Faction::EmberClans);

        let resonator = data
            .buildings
            .iter()
            .find(|b| b.name() == "Resonator")
            .unwrap();
        assert_eq!(resonator.start_count, 2);
        assert_eq!(resonator.faction(), Faction::Choir);

        let hands = data
            .conquest_units
            .iter()
            .find(|u| u.name() == "Hands")
            .unwrap();
        assert_eq!(hands.class(), Class::Worker);
        assert_eq!(hands.start_count, 5);
        assert_eq!(
            data.buildings[usize::from(hands.trained_at)].name(),
            "Clan Hall"
        );

        // docs/04-economy.md, "Starting state (Conquest)".
        for (name, count) in [
            ("Hands", 5),
            ("Gnawer", 4),
            ("Tuner", 3),
            ("Ledger Clerk", 4),
            ("Caravan", 1),
        ] {
            let u = data
                .conquest_units
                .iter()
                .find(|u| u.name() == name)
                .unwrap_or_else(|| panic!("no unit {name}"));
            assert_eq!(u.start_count, count, "{name}");
        }
        // Everything else starts with none.
        let starters: u32 = data
            .conquest_units
            .iter()
            .map(|u| u32::from(u.start_count))
            .sum();
        assert_eq!(starters, 5 + 4 + 3 + 4 + 1);
    });
}

#[test]
fn stats_match_units_ron() {
    with_real_data(|data| {
        let mut shared = 0;
        for u in data.conquest_units {
            if let Some((_, live)) = data.unit_by_name(u.name()) {
                shared += 1;
                assert_eq!(&u.stats, live, "{}", u.name());
            }
        }
        assert!(shared > 0, "no Conquest unit is also in units.ron");
    });
}

#[test]
fn baking_twice_gives_the_same_bytes() {
    let a = gc_bake::bake(&data_dir()).unwrap();
    let b = gc_bake::bake(&data_dir()).unwrap();
    assert_eq!(a.pack, b.pack);
}

/// Overclock and Phase Shift bake as the Choir's Resonance powers, with their
/// costs, tiers and times, and Phase Shift steps to the Choir's Resonator.
#[test]
fn overclock_and_phase_shift_bake_as_the_choirs_resonance_powers() {
    with_real_data(|data| {
        // docs/17-unit-rosters.md: Overclock 20 points, tier 1, 20 s
        // cooldown and duration; Phase Shift 60 points, tier 2, 90 s cooldown.
        let overclock = data
            .powers
            .iter()
            .find(|p| p.name() == "Overclock")
            .expect("no Overclock");
        assert_eq!(overclock.faction(), Faction::Choir);
        assert_eq!(overclock.meter, METER_RESONANCE);
        assert_eq!(overclock.effect, POWER_HASTE);
        assert_eq!(overclock.arg, 50, "percent");
        assert_eq!(overclock.cost, 20);
        assert_eq!(overclock.tier, 1);
        assert_eq!(overclock.cooldown_ticks, 400);
        assert_eq!(overclock.duration_ticks, 400);
        assert!(overclock.built());

        let resonator = data
            .buildings
            .iter()
            .position(|b| b.name() == "Resonator" && b.faction() == Faction::Choir)
            .expect("no Choir Resonator");
        let shift = data
            .powers
            .iter()
            .find(|p| p.name() == "Phase Shift")
            .expect("no Phase Shift");
        assert_eq!(shift.faction(), Faction::Choir);
        assert_eq!(shift.meter, METER_RESONANCE);
        assert_eq!(shift.effect, POWER_SHIFT);
        assert_eq!(usize::from(shift.arg), resonator);
        assert_eq!(shift.cost, 60);
        assert_eq!(shift.tier, 2);
        assert_eq!(shift.cooldown_ticks, 1800);
        assert_eq!(shift.duration_ticks, 0);
        assert!(shift.built());
    });
}

/// Rice Tax, Forced March and Banner of the Ancestors bake as the Clans'
/// Honor Edicts; Ancestral Return is not built yet.
#[test]
fn the_clans_edicts_bake_as_honor_powers() {
    with_real_data(|data| {
        let edict = |name: &str| {
            let p = (data.powers.iter())
                .find(|p| p.name() == name)
                .unwrap_or_else(|| panic!("no {name}"));
            assert_eq!(p.faction(), Faction::EmberClans, "{name}");
            p
        };
        let tax = edict("Rice Tax");
        assert_eq!(
            (tax.effect, tax.arg, tax.meter),
            (POWER_GRANT, 300, METER_HONOR)
        );
        assert_eq!(
            (tax.cost, tax.tier, tax.cooldown_ticks, tax.duration_ticks),
            (25, 1, 1200, 0)
        );
        let march = edict("Forced March");
        assert_eq!(
            (march.effect, march.arg, march.meter),
            (POWER_MARCH, 30, METER_HONOR)
        );
        assert_eq!(
            (
                march.cost,
                march.tier,
                march.cooldown_ticks,
                march.duration_ticks
            ),
            (20, 1, 900, 600)
        );
        let banner = edict("Banner of the Ancestors");
        assert_eq!(
            (banner.effect, banner.arg, banner.meter),
            (POWER_STEADFAST, 12, METER_HONOR)
        );
        assert_eq!(
            (
                banner.cost,
                banner.tier,
                banner.cooldown_ticks,
                banner.duration_ticks
            ),
            (35, 2, 1800, 400)
        );
        let ret = edict("Ancestral Return");
        assert!(!ret.built());
        assert_eq!(ret.meter_name(), "");
        assert_eq!(banner.meter_name(), "Honor");
        let shrine = (data.buildings.iter())
            .find(|b| b.name() == "Ancestor Shrine")
            .expect("no Ancestor Shrine");
        assert_ne!(shrine.flags & BUILDING_HONOR, 0, "a Shrine earns Honor");
    });
}

/// Every other meter ability but the Clans' Edicts is listed as not built
/// yet.
#[test]
fn every_other_faction_meter_ability_is_not_built_yet() {
    with_real_data(|data| {
        let mut others = 0;
        for p in data.powers {
            if p.faction() == Faction::EmberClans
                || p.name() == "Overclock"
                || p.name() == "Phase Shift"
            {
                continue;
            }
            others += 1;
            assert_eq!(p.effect, POWER_NOT_BUILT, "{}", p.name());
            assert!(!p.built(), "{}", p.name());
        }
        assert!(others > 0, "no other meter ability is listed");
        let silence = data
            .powers
            .iter()
            .find(|p| p.name() == "Silence the Chord")
            .expect("no Silence the Chord");
        assert_eq!(silence.faction(), Faction::Choir);
        assert_eq!(silence.effect, POWER_NOT_BUILT);
    });
}
