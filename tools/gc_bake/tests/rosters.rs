//! Checks the faction rosters in `data/rosters/` (design data for Conquest;
//! see docs/17-unit-rosters.md). The game does not read them yet, but every
//! `stats` block must bake exactly as `data/units.ron` entries do, so the
//! engine can move them across unchanged.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const ROSTERS: &[(&str, FactionDef)] = &[
    ("ember_clans.ron", FactionDef::EmberClans),
    ("brood.ron", FactionDef::Brood),
    ("choir.ron", FactionDef::Choir),
    ("carrion_concord.ron", FactionDef::CarrionConcord),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Roster {
    faction: FactionDef,
    meter: Meter,
    units: Vec<Unit>,
    heroes: Vec<Hero>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Meter {
    name: String,
    max: u16,
    abilities: Vec<Ability>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Unit {
    /// 1 to 3; 0 for summoned units.
    tier: u8,
    class: Class,
    cost: Cost,
    supply: u8,
    /// Seconds to train, or to cocoon for a Brood evolution.
    train_time: f64,
    from: From,
    #[serde(default)]
    flags: Vec<Flag>,
    abilities: Vec<Ability>,
    stats: Stats,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Hero {
    role: String,
    hired_at: String,
    per_level: Growth,
    #[serde(default)]
    flags: Vec<Flag>,
    abilities: Vec<Ability>,
    stats: Stats,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Growth {
    hp: u16,
    melee_damage: u16,
    #[serde(default)]
    missile_damage: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cost {
    grub: u16,
    marrow: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ability {
    name: String,
    kind: Kind,
    /// Plain-language description, with hero ranks written 1/2/3.
    effect: String,
    primitives: Vec<Primitive>,
    #[serde(default)]
    cooldown: f64,
    #[serde(default)]
    range: f64,
    #[serde(default)]
    radius: f64,
    #[serde(default)]
    duration: f64,
    /// Faction-meter abilities only: meter cost and tier.
    #[serde(default)]
    cost: u16,
    #[serde(default)]
    tier: u8,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    Worker,
    Light,
    Spear,
    Heavy,
    Ranged,
    Cavalry,
    Air,
    Artillery,
    Siege,
    Support,
    Monster,
}

#[derive(Deserialize)]
enum From {
    Building(String),
    Evolves(String),
    Summoned(String),
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
enum Flag {
    Flying,
    AntiAir,
    Crystal,
    Undead,
    Burrows,
    SiegeOnly,
    FriendlyFire,
    DirectFire,
    Detector,
    Terror,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Passive,
    Aura,
    Toggle,
    Active,
    Autocast,
    /// A command with no cooldown, such as a worker building.
    Order,
    Ultimate,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
enum Primitive {
    Damage,
    Heal,
    ApplyStatus,
    ModifyMorale,
    ModifyStat,
    Summon,
    Teleport,
    AreaPulse,
    SpawnVolley,
    Transform,
    RaiseCorpses,
    Reveal,
    Stun,
    Knockback,
    Control,
    Economy,
    Construct,
    Gather,
    Burrow,
}

/// Exactly the fields of a `data/units.ron` entry.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Stats {
    name: String,
    faction: FactionDef,
    models: u16,
    hp_per_model: u16,
    files: u16,
    speed: f64,
    turn_rate: f64,
    file_spacing: f64,
    rank_spacing: f64,
    melee_attack: u8,
    melee_defence: u8,
    melee_damage: u16,
    charge_bonus: u8,
    #[serde(default)]
    morale: u8,
    #[serde(default)]
    never_routs: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ranged: Option<Ranged>,
    #[serde(default, skip_serializing_if = "Mass::is_medium")]
    mass: Mass,
    #[serde(default, skip_serializing_if = "is_false")]
    reach: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    swing_room: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    shield: bool,
}

fn is_false(v: &bool) -> bool {
    !v
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Mass {
    Light,
    #[default]
    Medium,
    Heavy,
}

impl Mass {
    fn is_medium(&self) -> bool {
        *self == Mass::Medium
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Ranged {
    range: f64,
    damage: u16,
    reload: f64,
    accuracy: u8,
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
enum FactionDef {
    EmberClans,
    Brood,
    Choir,
    CarrionConcord,
}

fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

fn load(file: &str) -> Roster {
    let path = data_dir().join("rosters").join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{file}: {e}"));
    ron::from_str(&text).unwrap_or_else(|e| panic!("{file}:{e}"))
}

fn check_ability(file: &str, owner: &str, a: &Ability) {
    let at = format!("{file}: {owner}: {}", a.name);
    assert!(
        !a.name.is_empty(),
        "{file}: {owner}: an ability has no name"
    );
    assert!(!a.effect.trim().is_empty(), "{at}: no effect text");
    assert!(!a.primitives.is_empty(), "{at}: no primitives");
    for v in [a.cooldown, a.range, a.radius, a.duration] {
        assert!(v.is_finite() && v >= 0.0, "{at}: negative number");
    }
    if matches!(a.kind, Kind::Active | Kind::Ultimate) {
        assert!(a.cooldown > 0.0, "{at}: active abilities need a cooldown");
    }
}

/// The supply a unit of this class may cost (docs/17 "Classes and counters").
fn supply_range(class: Class) -> std::ops::RangeInclusive<u8> {
    match class {
        Class::Worker => 1..=1,
        Class::Light => 3..=3,
        Class::Spear | Class::Ranged => 3..=4,
        Class::Heavy => 4..=5,
        Class::Cavalry => 5..=5,
        Class::Air => 3..=3,
        Class::Artillery | Class::Siege => 4..=4,
        Class::Support => 2..=3,
        Class::Monster => 6..=6,
    }
}

#[test]
fn rosters_follow_the_design_rules() {
    for &(file, faction) in ROSTERS {
        let r = load(file);
        assert_eq!(r.faction, faction, "{file}: wrong faction");
        assert!(!r.meter.name.is_empty(), "{file}: meter has no name");
        for a in &r.meter.abilities {
            check_ability(file, &r.meter.name, a);
            assert!(
                a.cost > 0 && a.cost <= r.meter.max,
                "{file}: {}: meter cost must be 1 to {}",
                a.name,
                r.meter.max
            );
            assert!((1..=3).contains(&a.tier), "{file}: {}: tier 1 to 3", a.name);
        }

        let trained: BTreeMap<&str, &Unit> = r
            .units
            .iter()
            .filter(|u| matches!(u.from, From::Building(_)))
            .map(|u| (u.stats.name.as_str(), u))
            .collect();
        let mut classes = BTreeSet::new();
        for u in &r.units {
            let name = &u.stats.name;
            let at = format!("{file}: {name}");
            assert_eq!(u.stats.faction, faction, "{at}: wrong faction");
            classes.insert(format!("{:?}", u.class));
            match &u.from {
                From::Building(b) => {
                    assert!(!b.is_empty(), "{at}: no building");
                    assert!((1..=3).contains(&u.tier), "{at}: tier 1 to 3");
                    assert!(u.train_time > 0.0, "{at}: needs a train time");
                    assert!(supply_range(u.class).contains(&u.supply), "{at}: supply");
                    assert!(u.cost.grub > 0, "{at}: costs no Grub");
                }
                From::Evolves(parent) => {
                    let p = trained
                        .get(parent.as_str())
                        .unwrap_or_else(|| panic!("{at}: evolves from unknown {parent}"));
                    assert_eq!(p.tier, 1, "{at}: evolves from a tier-1 regiment");
                    assert_eq!(u.tier, 2, "{at}: evolutions are tier 2");
                    assert!(u.train_time > 0.0, "{at}: needs a cocoon time");
                    assert!(supply_range(u.class).contains(&u.supply), "{at}: supply");
                }
                From::Summoned(by) => {
                    assert!(!by.is_empty(), "{at}: summoned by nothing");
                    assert_eq!(u.tier, 0, "{at}: summoned units have tier 0");
                    assert_eq!(u.supply, 0, "{at}: summoned units use no supply");
                    assert_eq!((u.cost.grub, u.cost.marrow), (0, 0), "{at}: free");
                }
            }
            if u.tier == 1 {
                assert_eq!(u.cost.marrow, 0, "{at}: tier-1 units cost no Marrow");
            }
            assert_eq!(
                u.class == Class::Air,
                u.flags.contains(&Flag::Flying),
                "{at}: air regiments and only air regiments fly"
            );
            if faction == FactionDef::Choir {
                assert!(
                    u.flags.contains(&Flag::Crystal),
                    "{at}: Choir units are Crystal"
                );
            }
            if u.flags.contains(&Flag::Undead) {
                assert!(u.stats.never_routs, "{at}: undead never rout");
            }
            assert!(!u.abilities.is_empty(), "{at}: no abilities");
            for a in &u.abilities {
                check_ability(file, name, a);
                assert_ne!(a.kind, Kind::Ultimate, "{at}: only heroes have ultimates");
            }
        }
        for needed in ["Worker", "Monster"] {
            assert!(classes.contains(needed), "{file}: no {needed}");
        }

        assert_eq!(r.heroes.len(), 3, "{file}: three heroes");
        for h in &r.heroes {
            let at = format!("{file}: {}", h.stats.name);
            assert_eq!(h.stats.faction, faction, "{at}: wrong faction");
            assert!(!h.role.is_empty() && !h.hired_at.is_empty(), "{at}: role");
            assert_eq!(h.stats.models, 1, "{at}: heroes are one model");
            assert!(h.stats.never_routs, "{at}: heroes never rout");
            assert!(h.per_level.hp > 0 && h.per_level.melee_damage > 0, "{at}");
            assert_eq!(
                h.per_level.missile_damage > 0,
                h.stats.ranged.is_some(),
                "{at}: missile growth only for ranged heroes"
            );
            assert!(!h.flags.contains(&Flag::Flying), "{at}: heroes walk");
            assert_eq!(h.abilities.len(), 4, "{at}: four abilities");
            let ultimates = h.abilities.iter().filter(|a| a.kind == Kind::Ultimate);
            assert_eq!(ultimates.count(), 1, "{at}: one ultimate");
            for a in &h.abilities {
                check_ability(file, &h.stats.name, a);
            }
        }
    }
}

/// Every stats block, units and heroes from all four rosters together, must
/// bake: same rules as `data/units.ron`, and names unique across factions.
#[test]
fn every_stats_block_bakes() {
    let mut all = Vec::new();
    for &(file, _) in ROSTERS {
        let r = load(file);
        all.extend(r.units.into_iter().map(|u| u.stats));
        all.extend(r.heroes.into_iter().map(|h| h.stats));
    }
    let text = ron::to_string(&all).unwrap();

    let dir = std::env::temp_dir().join(format!("gc_rosters_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("units.ron"), text).unwrap();
    let baked = gc_bake::bake(&dir).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(baked.units, all.len());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The regiments the game already plays with appear in their rosters.
#[test]
fn live_units_are_in_the_rosters() {
    #[derive(Deserialize)]
    struct Named {
        name: String,
    }
    let text = std::fs::read_to_string(data_dir().join("units.ron")).unwrap();
    let live: Vec<ron::Value> = ron::from_str(&text).unwrap();
    let mut names = BTreeSet::new();
    for &(file, _) in ROSTERS {
        names.extend(load(file).units.into_iter().map(|u| u.stats.name));
    }
    for v in live {
        let n: Named = v.into_rust().unwrap();
        assert!(
            names.contains(&n.name),
            "{} is missing from the rosters",
            n.name
        );
    }
}
