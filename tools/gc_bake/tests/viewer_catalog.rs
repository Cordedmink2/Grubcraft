//! The unit viewer's catalog (`gc_bake::viewer`): every unit and hero of the
//! four rosters, in words, and the Rust source `gc_game`'s build script
//! writes from it.

use std::path::Path;

use gc_bake::rosters::Roster;
use gc_bake::viewer::{Entry, catalog, catalog_source};

fn data_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

fn entries() -> Vec<Entry> {
    catalog(&data_dir()).expect("the rosters bake into a catalog")
}

#[test]
fn every_unit_and_hero_is_in_the_catalog_in_faction_order() {
    let mut expected = 0;
    for file in [
        "ember_clans",
        "brood",
        "choir",
        "carrion_concord",
        "oathbound",
    ] {
        let path = data_dir().join("rosters").join(format!("{file}.ron"));
        let roster: Roster = ron::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        expected += roster.units.len() + roster.heroes.len();
    }
    let all = entries();
    assert_eq!(all.len(), expected);
    let factions: Vec<u8> = all.iter().map(|e| e.faction).collect();
    assert!(factions.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(factions.first(), Some(&0));
    assert_eq!(factions.last(), Some(&4));
}

#[test]
fn names_are_unique_and_the_known_models_are_there() {
    let all = entries();
    let mut names: Vec<&str> = all.iter().map(|e| e.name.as_str()).collect();
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(names.len(), count, "a name appears twice");
    for n in [
        "Yari Ashigaru",
        "The Daimyo",
        "War Kite",
        "Ledger Clerk",
        "The Usurer",
    ] {
        assert!(names.contains(&n), "{n}");
    }
}

#[test]
fn entries_carry_facts_and_the_right_labels() {
    let all = entries();
    let by = |n: &str| all.iter().find(|e| e.name == n).unwrap();
    let yari = by("Yari Ashigaru");
    assert_eq!(yari.kind, "Spear regiment, tier 1");
    assert_eq!(yari.tag, "T1");
    assert!(!yari.flying);
    let fact = |e: &Entry, l: &str| {
        e.facts
            .iter()
            .find(|(label, _)| label == l)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(fact(yari, "Cost").as_deref(), Some("120 Grub"));
    assert_eq!(fact(yari, "Trained at").as_deref(), Some("Dojo"));
    assert_eq!(fact(yari, "Soldiers").as_deref(), Some("28 in 8 files"));
    assert!(fact(yari, "Traits").unwrap().contains("long reach"));
    assert!(yari.abilities.iter().any(|(n, _, _)| n == "Brace"));

    let kite = by("War Kite");
    assert!(kite.flying);
    assert!(fact(kite, "Traits").unwrap().contains("flies"));

    let daimyo = by("The Daimyo");
    assert_eq!(daimyo.tag, "hero");
    assert_eq!(daimyo.kind, "Hero, commander, mounted");
    assert_eq!(fact(daimyo, "Hired at").as_deref(), Some("Ancestor Shrine"));
    assert_eq!(fact(daimyo, "Morale").as_deref(), Some("never routs"));
    let ultimate = daimyo
        .abilities
        .iter()
        .find(|(n, _, _)| n == "Shogun's Banner")
        .unwrap();
    assert!(
        ultimate.1.starts_with("Ultimate, 120 s cooldown"),
        "{}",
        ultimate.1
    );

    // Summoned units say who calls them and cost nothing to train.
    let summoned = all.iter().find(|e| e.tag == "summon").unwrap();
    assert!(summoned.kind.starts_with("Summoned "));
    assert!(summoned.facts.iter().any(|(l, _)| l == "Summoned by"));
}

#[test]
fn the_source_is_a_rust_slice_with_every_entry_escaped() {
    let all = entries();
    let src = catalog_source(&all);
    assert!(src.starts_with("&[\n"));
    assert!(src.trim_end().ends_with(']'));
    assert_eq!(src.matches("CatalogEntry {").count(), all.len());
    // A name with a quote or backslash would be escaped, not break the file.
    let tricky = Entry {
        faction: 1,
        name: "A \"quoted\" \\ name".into(),
        kind: "k".into(),
        tag: "T1".into(),
        flying: false,
        facts: vec![("a".into(), "b\nc".into())],
        abilities: vec![],
    };
    let src = catalog_source(&[tricky]);
    assert!(src.contains(r#""A \"quoted\" \\ name""#), "{src}");
    assert!(src.contains(r#""b\nc""#));
}
