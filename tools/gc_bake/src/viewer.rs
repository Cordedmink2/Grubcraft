//! The unit viewer's catalog: every unit and hero of the four rosters
//! (`data/rosters/*.ron`) flattened into the words the viewer's panels show.
//! `gc_game`'s build script writes it as Rust source (`catalog_source`) that
//! the game includes, so the executable carries the roster facts without
//! carrying a RON parser. Buildings are not units and are left out.

use std::fmt::Write as _;
use std::path::Path;

use crate::rosters::{self, Ability, Flag, Hero, Kind, Mass, Roster, Stats, Unit};
use crate::{BakeError, conquest::ROSTER_FILES};

/// One thing to look at: a regiment, a summoned unit, a worker or a hero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Index into `gc_data::Faction::ALL`.
    pub faction: u8,
    /// As spelled in the roster, and in `assets-src/models.ron`.
    pub name: String,
    /// One line under the name: "Spear regiment, tier 1", "Hero, Duelist".
    pub kind: String,
    /// A word or two for the unit list: "T1", "hero", "summon", "worker".
    pub tag: String,
    /// The unit flies, so its view needs headroom above its feet.
    pub flying: bool,
    /// (label, value) rows for the Stats tab.
    pub facts: Vec<(String, String)>,
    /// (name, how it works, what it does) for the Abilities tab.
    pub abilities: Vec<(String, String, String)>,
}

/// The catalog: the factions in `Faction` order, and within one the
/// regiments, then the heroes, each in file order.
pub fn catalog(data_dir: &Path) -> Result<Vec<Entry>, BakeError> {
    let mut entries = Vec::new();
    for (path, faction) in ROSTER_FILES {
        let file = data_dir.join(path);
        let text = std::fs::read_to_string(&file).map_err(|e| BakeError::Read(file.clone(), e))?;
        let roster: Roster = ron::from_str(&text).map_err(|e| BakeError::Parse(file.clone(), e))?;
        if roster.faction != faction {
            return Err(crate::invalid(
                &file,
                "faction",
                format!("must be {faction:?}, as the file's place in the pack says"),
            ));
        }
        let index = gc_data::Faction::from(roster.faction) as u8;
        entries.extend(roster.units.iter().map(|u| unit_entry(index, u)));
        entries.extend(roster.heroes.iter().map(|h| hero_entry(index, h)));
    }
    Ok(entries)
}

fn unit_entry(faction: u8, u: &Unit) -> Entry {
    let class = format!("{:?}", u.class);
    let kind = match (u.tier, u.class) {
        (0, _) => format!("Summoned {}", class.to_lowercase()),
        (t, rosters::Class::Worker) => format!("Worker, tier {t}"),
        (t, _) => format!("{class} regiment, tier {t}"),
    };
    let mut facts = vec![
        ("Cost".to_string(), cost(u.cost.grub, u.cost.marrow)),
        ("Supply".to_string(), u.supply.to_string()),
    ];
    match &u.from {
        rosters::From::Building(b) => {
            facts.push(("Trained at".into(), b.clone()));
            facts.push(("Train time".into(), format!("{} s", u.train_time)));
        }
        rosters::From::Evolves(p) => {
            facts.push(("Evolves from".into(), p.clone()));
            facts.push(("Cocoon time".into(), format!("{} s", u.train_time)));
        }
        rosters::From::Summoned(by) => facts.push(("Summoned by".into(), by.clone())),
    }
    facts.extend(stat_facts(&u.stats, &u.flags));
    let tag = match (u.tier, u.class) {
        (0, _) => "summon".to_string(),
        (_, rosters::Class::Worker) => "worker".to_string(),
        (t, _) => format!("T{t}"),
    };
    Entry {
        faction,
        name: u.stats.name.clone(),
        kind,
        tag,
        flying: u.flags.contains(&Flag::Flying),
        facts,
        abilities: u.abilities.iter().map(ability).collect(),
    }
}

fn hero_entry(faction: u8, h: &Hero) -> Entry {
    let mut facts = vec![
        ("Hired at".to_string(), h.hired_at.clone()),
        (
            "Per level".into(),
            format!(
                "+{} hit points, +{} melee damage{}",
                h.per_level.hp,
                h.per_level.melee_damage,
                if h.per_level.missile_damage > 0 {
                    format!(", +{} missile damage", h.per_level.missile_damage)
                } else {
                    String::new()
                }
            ),
        ),
    ];
    facts.extend(stat_facts(&h.stats, &h.flags));
    Entry {
        faction,
        name: h.stats.name.clone(),
        kind: format!("Hero, {}", h.role.to_lowercase()),
        tag: "hero".into(),
        flying: h.flags.contains(&Flag::Flying),
        facts,
        abilities: h.abilities.iter().map(ability).collect(),
    }
}

fn cost(grub: u16, marrow: u16) -> String {
    match (grub, marrow) {
        (0, 0) => "Free".into(),
        (g, 0) => format!("{g} Grub"),
        (0, m) => format!("{m} Marrow"),
        (g, m) => format!("{g} Grub, {m} Marrow"),
    }
}

fn stat_facts(s: &Stats, flags: &[Flag]) -> Vec<(String, String)> {
    let mut facts = Vec::new();
    let mut add = |label: &str, value: String| facts.push((label.to_string(), value));
    add(
        "Soldiers",
        if s.models == 1 {
            "1 (a single figure)".into()
        } else {
            format!("{} in {} files", s.models, s.files)
        },
    );
    add("Hit points each", s.hp_per_model.to_string());
    add("Speed", format!("{} tiles a second", s.speed));
    add("Melee attack", s.melee_attack.to_string());
    add("Melee defence", s.melee_defence.to_string());
    add("Melee damage", s.melee_damage.to_string());
    if s.charge_bonus > 0 {
        add("Charge bonus", format!("+{}%", s.charge_bonus));
    }
    if let Some(r) = &s.ranged {
        add(
            "Ranged",
            format!(
                "{} damage, range {} tiles, reload {} s, accuracy {}%",
                r.damage, r.range, r.reload, r.accuracy
            ),
        );
    }
    add(
        "Morale",
        if s.never_routs || s.morale == 0 {
            "never routs".into()
        } else {
            s.morale.to_string()
        },
    );
    if s.mass != Mass::Medium {
        add("Mass", format!("{:?}", s.mass).to_lowercase());
    }
    let mut traits: Vec<String> = flags.iter().map(|f| flag_name(*f).to_string()).collect();
    for (on, name) in [
        (s.reach, "long reach"),
        (s.swing_room, "needs room to swing"),
        (s.shield, "shield"),
    ] {
        if on {
            traits.push(name.into());
        }
    }
    if !traits.is_empty() {
        add("Traits", traits.join(", "));
    }
    facts
}

fn flag_name(f: Flag) -> &'static str {
    match f {
        Flag::Flying => "flies",
        Flag::AntiAir => "shoots at fliers",
        Flag::Crystal => "crystal",
        Flag::Undead => "undead",
        Flag::Burrows => "burrows",
        Flag::SiegeOnly => "only hurts structures",
        Flag::FriendlyFire => "friendly fire",
        Flag::DirectFire => "direct fire",
        Flag::Detector => "sees hidden units",
        Flag::Terror => "causes terror",
        Flag::Hatches => "hatches from a larva",
        Flag::Consumed => "used up to grow buildings",
        Flag::BuildsOnly => "builds but does not gather",
    }
}

fn ability(a: &Ability) -> (String, String, String) {
    let mut how = vec![format!("{:?}", a.kind)];
    if a.kind == Kind::Ultimate || a.cooldown > 0.0 {
        how.push(format!("{} s cooldown", a.cooldown));
    }
    if a.range > 0.0 {
        how.push(format!("range {}", a.range));
    }
    if a.radius > 0.0 {
        how.push(format!("radius {}", a.radius));
    }
    if a.duration > 0.0 {
        how.push(format!("lasts {} s", a.duration));
    }
    (a.name.clone(), how.join(", "), a.effect.clone())
}

/// The catalog as Rust source: a `&[CatalogEntry]` expression for
/// `gc_game::viewer` to include.
pub fn catalog_source(entries: &[Entry]) -> String {
    let mut out = String::from("&[\n");
    for e in entries {
        let _ = writeln!(
            out,
            "    CatalogEntry {{ faction: {}, name: {:?}, kind: {:?}, tag: {:?}, flying: {}, facts: &[",
            e.faction, e.name, e.kind, e.tag, e.flying
        );
        for (label, value) in &e.facts {
            let _ = writeln!(out, "        ({label:?}, {value:?}),");
        }
        out.push_str("    ], abilities: &[\n");
        for (name, how, effect) in &e.abilities {
            let _ = writeln!(out, "        ({name:?}, {how:?}, {effect:?}),");
        }
        out.push_str("    ] },\n");
    }
    out.push_str("]\n");
    out
}
