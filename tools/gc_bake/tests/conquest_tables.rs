//! The real rosters, baked into the pack's building and Conquest unit tables
//! and read back the way the game reads them.

use std::path::{Path, PathBuf};

use gc_assets::Pack;
use gc_data::conquest::{BUILDING_DROP_OFF_GRUB, Class, NONE};
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
