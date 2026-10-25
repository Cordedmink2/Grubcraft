//! The data behind the in-game Guide (docs/08-ui-ux.md#guide).
//!
//! The numbers come from the faction rosters (`data/rosters/*.ron`, the same
//! files the baker turns into the building and Conquest tables); the prose
//! comes from `data/guide/*.ron`, one short paragraph per building and unit
//! (format: docs/17-unit-rosters.md#guide-data). Both are compiled in, and
//! [`load`] joins them into the view models `gc_ui::guide` draws. Nothing is
//! written twice: a cost in the Guide is the cost in the roster.
//!
//! The rosters are read with small, lenient types of our own (unknown fields
//! are ignored) so this file stays independent of the baker's strict ones;
//! the tests below fail if a roster and its guide file drift apart.

use std::collections::HashMap;

use gc_data::Faction;
use gc_ui::Icon;
use gc_ui::guide::{FactionGuide, GuideData, GuideEntry, GuideUpgrade};
use serde::Deserialize;

/// The most characters in a guide summary, so it fits the card without
/// scrolling a long way, and in a tagline.
#[cfg_attr(not(test), allow(dead_code))]
pub const MAX_SUMMARY: usize = 350;
#[cfg_attr(not(test), allow(dead_code))]
pub const MAX_TAGLINE: usize = 80;
/// The most characters in one building tip, so the tips card stays short.
#[cfg_attr(not(test), allow(dead_code))]
pub const MAX_TIP: usize = 100;

/// What the roster files say about hiring a hero (docs/17 "Shared rules").
/// They have no field for it yet, so the Guide labels it a design value.
const HERO_RULES: &str = "Design values: 200 Grub and 50 Marrow, 40 s to hire, 5 supply";

// ---- the roster files, as much of them as the Guide reads ----------------

#[derive(Deserialize)]
struct Roster {
    buildings: Vec<RBuilding>,
    units: Vec<RUnit>,
    heroes: Vec<RHero>,
}

#[derive(Deserialize)]
struct RBuilding {
    name: String,
    role: Role,
    tier: u8,
    #[serde(default)]
    upgrades_from: Option<String>,
    cost: Cost,
    build_time: f64,
    hp: u16,
    armour: Armour,
    footprint: (u8, u8),
    #[serde(default)]
    supply: u8,
    #[serde(default)]
    flags: Vec<BuildingFlag>,
    #[serde(default)]
    attack: Option<Attack>,
    #[serde(default)]
    field: u8,
    #[serde(default)]
    tunes: Option<Tunes>,
    #[serde(default)]
    links: u8,
    effect: String,
}

#[derive(Deserialize)]
struct Tunes {
    node: Node,
    tiles: u8,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
enum Node {
    Seeps,
    Veins,
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Main,
    Outpost,
    Housing,
    Economy,
    Production,
    Tech,
    Heroes,
    Defence,
    Wall,
    Bridge,
    Stakes,
    Barricade,
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
enum Armour {
    Fortified,
    Structure,
    Living,
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
enum BuildingFlag {
    DropOffGrub,
    DropOffMarrow,
    PowersField,
    AntiAir,
    Larvae,
    Veins,
    NeedsPower,
    Honor,
    Toll,
    Silences,
    Gate,
    Line,
    Pass,
    Grown,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Attack {
    range: f64,
    damage: u16,
    reload: f64,
    shots: u8,
}

#[derive(Deserialize, Clone, Copy)]
struct Cost {
    grub: u16,
    marrow: u16,
}

#[derive(Deserialize)]
struct RUnit {
    tier: u8,
    class: Class,
    cost: Cost,
    supply: u8,
    train_time: f64,
    from: From,
    #[serde(default)]
    flags: Vec<Flag>,
    abilities: Vec<Ability>,
    stats: Stats,
}

#[derive(Deserialize)]
struct RHero {
    role: String,
    hired_at: String,
    per_level: Growth,
    abilities: Vec<Ability>,
    stats: Stats,
}

#[derive(Deserialize)]
struct Growth {
    hp: u16,
    melee_damage: u16,
    #[serde(default)]
    missile_damage: u16,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
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
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
enum From {
    Building(String),
    Evolves(String),
    Summoned(String),
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
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
    Hatches,
    Consumed,
    BuildsOnly,
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Passive,
    Aura,
    Toggle,
    Active,
    Autocast,
    Order,
    Ultimate,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Ability {
    name: String,
    kind: Kind,
    effect: String,
    #[serde(default)]
    cooldown: f64,
    #[serde(default)]
    range: f64,
    #[serde(default)]
    radius: f64,
    #[serde(default)]
    duration: f64,
}

#[derive(Deserialize)]
struct Stats {
    name: String,
    models: u16,
    hp_per_model: u16,
    speed: f64,
    melee_attack: u8,
    melee_defence: u8,
    melee_damage: u16,
    charge_bonus: u8,
    #[serde(default)]
    morale: u8,
    #[serde(default)]
    never_routs: bool,
    #[serde(default)]
    ranged: Option<Ranged>,
    #[serde(default)]
    mass: Mass,
    #[serde(default)]
    reach: bool,
    #[serde(default)]
    swing_room: bool,
    #[serde(default)]
    shield: bool,
}

#[derive(Deserialize)]
struct Ranged {
    range: f64,
    damage: u16,
    reload: f64,
    accuracy: u8,
}

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Mass {
    Light,
    #[default]
    Medium,
    Heavy,
}

// ---- the guide files ------------------------------------------------------

/// `data/guide/<faction>.ron`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuideFile {
    pub faction: Faction,
    pub buildings: Vec<BuildingText>,
    pub units: Vec<UnitText>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildingText {
    pub name: String,
    pub summary: String,
    /// (what, benefit): the main building's tier upgrade (named by the
    /// building it makes) or "Research".
    #[serde(default)]
    pub notes: Vec<(String, String)>,
    /// Two or three succinct tips on using the building, shown while it is
    /// being placed and behind the info button on its card.
    #[serde(default)]
    pub tips: Vec<TipText>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TipText {
    pub icon: Icon,
    pub text: String,
}

/// A building's tips: (picture, sentence), keyed by the building's name.
pub type TipBook = HashMap<String, Vec<(Icon, String)>>;

fn tips_of(text: &BuildingText) -> Vec<(Icon, String)> {
    text.tips.iter().map(|t| (t.icon, ascii(&t.text))).collect()
}

/// Every faction's building tips, for the game to show while placing and
/// selecting. Building names are unique across factions (a test checks).
pub fn tip_book() -> TipBook {
    let mut book = TipBook::new();
    for src in EMBEDDED.guides {
        let file = parse_guide(src).unwrap_or_else(|e| panic!("data/guide: {e}"));
        for b in &file.buildings {
            book.insert(b.name.clone(), tips_of(b));
        }
    }
    book
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitText {
    pub name: String,
    pub tagline: String,
    pub summary: String,
}

/// `data/guide/classes.ron`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassFile {
    classes: Vec<ClassText>,
    multipliers: Vec<(String, String, f32)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassText {
    name: String,
    #[allow(dead_code)]
    blurb: String,
}

/// The sources, as text: a roster and a guide file for each faction in
/// [`Faction::ALL`] order, and the class table.
pub struct Sources<'a> {
    pub rosters: [&'a str; 4],
    pub guides: [&'a str; 4],
    pub classes: &'a str,
}

/// Read a guide file.
fn parse_guide(src: &str) -> Result<GuideFile, String> {
    ron::from_str(src).map_err(|e| e.to_string())
}

/// The files compiled into the executable.
pub const EMBEDDED: Sources<'static> = Sources {
    rosters: [
        include_str!("../../../data/rosters/ember_clans.ron"),
        include_str!("../../../data/rosters/brood.ron"),
        include_str!("../../../data/rosters/choir.ron"),
        include_str!("../../../data/rosters/carrion_concord.ron"),
    ],
    guides: [
        include_str!("../../../data/guide/ember_clans.ron"),
        include_str!("../../../data/guide/brood.ron"),
        include_str!("../../../data/guide/choir.ron"),
        include_str!("../../../data/guide/carrion_concord.ron"),
    ],
    classes: include_str!("../../../data/guide/classes.ron"),
};

/// The names on the faction buttons (the Choir is the Shard Choir in the
/// fiction; `Faction::name` says "Choir").
pub fn faction_label(f: Faction) -> &'static str {
    match f {
        Faction::EmberClans => "Ember Clans",
        Faction::Brood => "Brood",
        Faction::Choir => "Shard Choir",
        Faction::CarrionConcord => "Carrion Concord",
    }
}

/// The Guide's content for the game. The files are compiled in and checked
/// by tests, so a failure here means the build itself is broken.
pub fn load() -> GuideData {
    build(&EMBEDDED).unwrap_or_else(|e| panic!("guide data: {e}"))
}

/// Join rosters, guide text and class table into a [`GuideData`].
pub fn build(src: &Sources<'_>) -> Result<GuideData, String> {
    let classes: ClassFile =
        ron::from_str(src.classes).map_err(|e| format!("data/guide/classes.ron: {e}"))?;
    let mut factions = Vec::new();
    for (i, f) in Faction::ALL.into_iter().enumerate() {
        let roster: Roster = ron::from_str(src.rosters[i])
            .map_err(|e| format!("data/rosters/{}: {e}", faction_label(f)))?;
        let text = parse_guide(src.guides[i])
            .map_err(|e| format!("data/guide/{}: {e}", faction_label(f)))?;
        if text.faction != f {
            return Err(format!(
                "guide file {i} is for {:?}, not {f:?}",
                text.faction
            ));
        }
        factions.push(faction_guide(f, &roster, &text, &classes));
    }
    Ok(GuideData { factions })
}

// ---- formatting -----------------------------------------------------------

/// Keep to what the 8x16 font can draw (printable ASCII).
fn ascii(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{2013}' | '\u{2014}' => '-',
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201c}' | '\u{201d}' => '"',
            '\u{00d7}' => 'x',
            c if (' '..='~').contains(&c) => c,
            _ => '?',
        })
        .collect()
}

fn num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn thousands(v: u32) -> String {
    let s = v.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn cost_text(c: Cost) -> String {
    crate::buildings::price_text(u32::from(c.grub), u32::from(c.marrow))
}

fn tiles(v: f64) -> String {
    format!("{} tiles", num(v))
}

fn secs(v: f64) -> String {
    format!("{} s", num(v))
}

fn join(names: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    names
        .into_iter()
        .map(|s| s.as_ref().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn building_flag_text(f: BuildingFlag, b: &RBuilding) -> Option<String> {
    Some(match f {
        BuildingFlag::DropOffGrub => "Grub drop-off".into(),
        BuildingFlag::DropOffMarrow => "Marrow drop-off".into(),
        BuildingFlag::PowersField => format!("Powers buildings within {} tiles", b.field),
        BuildingFlag::AntiAir => "Shoots air".into(),
        BuildingFlag::Larvae => "Holds larvae".into(),
        BuildingFlag::Veins => format!("Spreads root-veins {} tiles", b.field),
        BuildingFlag::NeedsPower => "Needs power".into(),
        BuildingFlag::Honor => "Earns Honor".into(),
        BuildingFlag::Toll => "Earns 1 Grub per 4 damage it deals".into(),
        BuildingFlag::Silences => "Tolls: silences enemy powers within 10 tiles".into(),
        BuildingFlag::Gate if b.upgrades_from.is_some() => {
            "Gatehouse: your side walks through, enemies don't; foot and archers can man its top"
                .into()
        }
        BuildingFlag::Gate => {
            "Gate: your side walks through, enemies don't; select it to set Auto, Locked or Open"
                .into()
        }
        BuildingFlag::Line if b.role == Role::Stakes => {
            "Drawn like a wall at any angle; anyone walks through at half pace, nobody charges through. Spearmen and archers can plant it free".into()
        }
        BuildingFlag::Line if b.role == Role::Barricade => {
            "Drawn like a wall at any angle; blocks walking, and troops of yours within 2 tiles take less damage".into()
        }
        BuildingFlag::Line if b.role == Role::Bridge => {
            "Drawn straight across water, bank to bank; anyone can cross it, and troops on a tile that falls drop in the river".into()
        }
        BuildingFlag::Line => {
            "Drawn wall: drag it out at any angle, paid and built a tile at a time".into()
        }
        BuildingFlag::Pass => {
            "Your side walks through it; without power it drops and anyone can".into()
        }
        BuildingFlag::Grown => {
            "Grown along your root-veins, no Gnawer needed; withers off them".into()
        }
        BuildingFlag::Other => return None,
    })
}

fn unit_flag_text(f: Flag) -> Option<&'static str> {
    Some(match f {
        Flag::Flying => "Flies",
        Flag::AntiAir => "Shoots air",
        Flag::Crystal => "Crystal",
        Flag::Undead => "Undead",
        Flag::Burrows => "Burrows",
        Flag::SiegeOnly => "Harms only structures",
        Flag::FriendlyFire => "Hits friends",
        Flag::DirectFire => "Direct fire",
        Flag::Detector => "Detects hidden units",
        Flag::Terror => "Causes terror",
        Flag::Hatches => "Hatches from a larva",
        Flag::Consumed => "Used up by what it builds",
        Flag::BuildsOnly => "Builds, does not gather",
        Flag::Other => return None,
    })
}

fn role_name(r: Role) -> &'static str {
    match r {
        Role::Main => "Main building",
        Role::Outpost => "Outpost",
        Role::Housing => "Housing",
        Role::Economy => "Economy building",
        Role::Production => "Production building",
        Role::Tech => "Tech building",
        Role::Heroes => "Hero building",
        Role::Defence => "Defence",
        Role::Wall => "Wall",
        Role::Bridge => "Bridge",
        Role::Stakes => "Stakes",
        Role::Barricade => "Barricade",
        Role::Other => "Building",
    }
}

fn class_name(c: Class) -> &'static str {
    match c {
        Class::Worker => "Worker",
        Class::Light => "Light",
        Class::Spear => "Spear",
        Class::Heavy => "Heavy",
        Class::Ranged => "Ranged",
        Class::Cavalry => "Cavalry",
        Class::Air => "Air",
        Class::Artillery => "Artillery",
        Class::Siege => "Siege",
        Class::Support => "Support",
        Class::Monster => "Monster",
        Class::Other => "Other",
    }
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Passive => "Passive",
        Kind::Aura => "Aura",
        Kind::Toggle => "Toggle",
        Kind::Active => "Active",
        Kind::Autocast => "Autocast",
        Kind::Order => "Order",
        Kind::Ultimate => "Ultimate",
        Kind::Other => "Ability",
    }
}

type Facts = Vec<(String, String)>;

fn fact(facts: &mut Facts, label: &str, value: impl Into<String>) {
    facts.push((label.to_string(), value.into()));
}

/// The numbers of a regiment or hero, from its stats block.
fn stat_facts(facts: &mut Facts, s: &Stats, hero: bool) {
    if s.models > 1 {
        fact(
            facts,
            "Regiment",
            format!(
                "{} models, {} hp each ({} in all)",
                s.models,
                s.hp_per_model,
                thousands(u32::from(s.models) * u32::from(s.hp_per_model))
            ),
        );
    } else {
        fact(facts, "Hit points", thousands(u32::from(s.hp_per_model)));
    }
    fact(facts, "Speed", format!("{} tiles/s", num(s.speed)));
    fact(
        facts,
        "Melee",
        format!(
            "attack {}, defence {}, damage {}",
            s.melee_attack, s.melee_defence, s.melee_damage
        ),
    );
    if s.charge_bonus > 0 {
        fact(facts, "Charge bonus", format!("+{}%", s.charge_bonus));
    }
    if let Some(r) = &s.ranged {
        fact(
            facts,
            "Ranged",
            format!(
                "range {} tiles, damage {}, a volley every {}, {}% accuracy",
                num(r.range),
                r.damage,
                secs(r.reload),
                r.accuracy
            ),
        );
    }
    if s.never_routs || hero {
        fact(facts, "Morale", "Never routs");
    } else {
        fact(facts, "Morale", s.morale.to_string());
    }
    let mut traits = Vec::new();
    if s.mass != Mass::Medium {
        traits.push(format!("{:?} mass", s.mass));
    }
    if s.reach {
        traits.push("Reach: the second rank fights".into());
    }
    if s.swing_room {
        traits.push("Big swings need room".into());
    }
    if s.shield {
        traits.push("Can form a shield wall".into());
    }
    if !traits.is_empty() {
        fact(facts, "Weapons", join(traits));
    }
}

fn ability_cost(a: &Ability) -> String {
    let mut parts = Vec::new();
    match a.kind {
        Kind::Passive | Kind::Aura => parts.push("always on".to_string()),
        Kind::Toggle => parts.push("switched on and off".to_string()),
        Kind::Order => parts.push("a command".to_string()),
        Kind::Autocast => parts.push("fires by itself".to_string()),
        Kind::Active | Kind::Ultimate | Kind::Other => {}
    }
    if a.cooldown > 0.0 {
        parts.push(format!("cooldown {}", secs(a.cooldown)));
    }
    if a.range > 0.0 {
        parts.push(format!("range {}", tiles(a.range)));
    }
    if a.radius > 0.0 {
        parts.push(format!("radius {}", tiles(a.radius)));
    }
    if a.duration > 0.0 {
        parts.push(format!("lasts {}", secs(a.duration)));
    }
    join(parts)
}

fn ability_upgrade(a: &Ability, hero: bool) -> GuideUpgrade {
    let mut tag = kind_name(a.kind).to_string();
    if hero && a.kind == Kind::Ultimate {
        tag.push_str(" (level 6)");
    }
    let mut facts = Vec::new();
    if hero && a.effect.contains('/') {
        fact(
            &mut facts,
            "Ranks",
            "1/2/3 in the text are the values at ranks 1, 2 and 3",
        );
    }
    GuideUpgrade {
        name: a.name.clone(),
        tag,
        cost: ability_cost(a),
        benefit: a.effect.clone(),
        facts,
    }
}

// ---- the join -------------------------------------------------------------

struct Counters {
    /// (attacker, target) -> multiplier.
    mult: HashMap<(String, String), f32>,
}

impl Counters {
    fn new(f: &ClassFile) -> Counters {
        Counters {
            mult: f
                .multipliers
                .iter()
                .map(|(a, t, m)| ((a.clone(), t.clone()), *m))
                .collect(),
        }
    }

    /// (strong against with multipliers, weak against), for class `c`.
    fn lines(&self, all: &[ClassText], c: &str) -> (Vec<String>, Vec<String>) {
        let mut strong: Vec<(&str, f32)> = Vec::new();
        let mut weak: Vec<&str> = Vec::new();
        for other in all.iter().map(|k| k.name.as_str()) {
            if let Some(&m) = self.mult.get(&(c.to_string(), other.to_string())) {
                if m > 1.0 {
                    strong.push((other, m));
                } else if m < 1.0 {
                    weak.push(other);
                }
            }
            if self
                .mult
                .get(&(other.to_string(), c.to_string()))
                .is_some_and(|&m| m > 1.0)
                && !weak.contains(&other)
            {
                weak.push(other);
            }
        }
        strong.sort_by(|a, b| b.1.total_cmp(&a.1));
        (
            strong
                .into_iter()
                .map(|(n, m)| format!("{n} (x{})", num(f64::from(m))))
                .collect(),
            weak.into_iter().map(str::to_string).collect(),
        )
    }
}

fn faction_guide(f: Faction, r: &Roster, t: &GuideFile, classes: &ClassFile) -> FactionGuide {
    let counters = Counters::new(classes);
    let text_b: HashMap<&str, &BuildingText> =
        t.buildings.iter().map(|b| (b.name.as_str(), b)).collect();
    let text_u: HashMap<&str, &UnitText> = t.units.iter().map(|u| (u.name.as_str(), u)).collect();
    let ctx = Ctx {
        r,
        text_u: &text_u,
        counters: &counters,
        classes,
    };
    let buildings = r
        .buildings
        .iter()
        .map(|b| ctx.building(b, text_b.get(b.name.as_str()).copied()))
        .collect();
    let mut troops: Vec<GuideEntry> = r.units.iter().map(|u| ctx.unit(u)).collect();
    troops.extend(r.heroes.iter().map(|h| ctx.hero(h)));
    FactionGuide {
        name: faction_label(f).to_string(),
        buildings,
        troops,
    }
}

struct Ctx<'a> {
    r: &'a Roster,
    text_u: &'a HashMap<&'a str, &'a UnitText>,
    counters: &'a Counters,
    classes: &'a ClassFile,
}

impl Ctx<'_> {
    fn tagline(&self, name: &str) -> String {
        self.text_u
            .get(name)
            .map_or_else(String::new, |u| ascii(&u.tagline))
    }

    fn main_at(&self, tier: u8) -> Option<&RBuilding> {
        self.r
            .buildings
            .iter()
            .find(|b| b.role == Role::Main && b.tier == tier)
    }

    fn trained_at(&self, building: &str) -> Vec<&RUnit> {
        self.r
            .units
            .iter()
            .filter(|u| matches!(&u.from, From::Building(b) if b == building))
            .collect()
    }

    fn evolutions_of(&self, parent: &str) -> Vec<&RUnit> {
        self.r
            .units
            .iter()
            .filter(|u| matches!(&u.from, From::Evolves(p) if p == parent))
            .collect()
    }

    fn has_evolutions(&self) -> bool {
        self.r
            .units
            .iter()
            .any(|u| matches!(u.from, From::Evolves(_)))
    }

    fn unit_summary_facts(&self, u: &RUnit) -> Facts {
        let mut f = Facts::new();
        fact(
            &mut f,
            "Class",
            format!("{}, tier {}", class_name(u.class), u.tier),
        );
        stat_facts(&mut f, &u.stats, false);
        f
    }

    /// What reaching `tier` of the main building unlocks.
    fn unlocks(&self, tier: u8) -> (Vec<&str>, Vec<&str>) {
        let b = self
            .r
            .buildings
            .iter()
            .filter(|b| b.tier == tier && b.role != Role::Main)
            .map(|b| b.name.as_str())
            .collect();
        let u = self
            .r
            .units
            .iter()
            .filter(|u| {
                u.tier == tier && u.class != Class::Worker && matches!(u.from, From::Building(_))
            })
            .map(|u| u.stats.name.as_str())
            .collect();
        (b, u)
    }

    fn building(&self, b: &RBuilding, text: Option<&BuildingText>) -> GuideEntry {
        let mut facts = Facts::new();
        fact(&mut facts, "Role", role_name(b.role));
        match (b.role, b.upgrades_from.as_deref(), self.main_at(b.tier)) {
            // A gatehouse goes up on its gate, as a tier on the main building.
            (r, Some(from), _) if r == Role::Main || b.flags.contains(&BuildingFlag::Gate) => {
                fact(&mut facts, "Needs", format!("An upgrade of the {from}"));
            }
            (Role::Main, None, _) => fact(&mut facts, "Needs", "You start with it"),
            (_, _, Some(main)) if b.tier > 1 => fact(
                &mut facts,
                "Needs",
                format!("Main building at tier {} ({})", b.tier, main.name),
            ),
            _ => fact(&mut facts, "Needs", "Nothing: available from the start"),
        }
        fact(&mut facts, "Cost", cost_text(b.cost));
        fact(&mut facts, "Build time", secs(b.build_time));
        fact(&mut facts, "Hit points", thousands(u32::from(b.hp)));
        fact(&mut facts, "Armour", format!("{:?}", b.armour));
        fact(
            &mut facts,
            "Size",
            format!("{} x {} tiles", b.footprint.0, b.footprint.1),
        );
        if b.supply > 0 {
            fact(&mut facts, "Supply", format!("+{}", b.supply));
        }
        let feats: Vec<String> = b
            .flags
            .iter()
            .filter_map(|&f| building_flag_text(f, b))
            .collect();
        if !feats.is_empty() {
            fact(&mut facts, "Features", join(feats));
        }
        if let Some(a) = &b.attack {
            fact(
                &mut facts,
                "Attack",
                format!(
                    "range {} tiles, {} x {} damage every {}",
                    num(a.range),
                    a.shots,
                    a.damage,
                    secs(a.reload)
                ),
            );
        }
        if let Some(t) = &b.tunes {
            let what = match t.node {
                Node::Seeps => "seeps",
                Node::Veins => "veins",
                Node::Other => "nodes",
            };
            fact(
                &mut facts,
                "Tunes",
                format!("{what} within {} tiles", t.tiles),
            );
        }
        if b.links > 0 {
            fact(
                &mut facts,
                "Links",
                format!("with its kind within {} tiles", b.links),
            );
        }

        // Links to other entries.
        let mut links = Facts::new();
        if let Some(from) = &b.upgrades_from {
            fact(&mut links, "Upgrades from", from.clone());
        }
        let next = self
            .r
            .buildings
            .iter()
            .find(|n| n.upgrades_from.as_deref() == Some(&b.name));
        if let Some(n) = next {
            fact(&mut links, "Upgrades to", n.name.clone());
        }
        let trains = self.trained_at(&b.name);
        if !trains.is_empty() {
            fact(
                &mut links,
                "Trains",
                join(trains.iter().map(|u| &u.stats.name)),
            );
        }
        let hires: Vec<&RHero> = self
            .r
            .heroes
            .iter()
            .filter(|h| h.hired_at == b.name)
            .collect();
        if !hires.is_empty() {
            fact(
                &mut links,
                "Hires",
                join(hires.iter().map(|h| &h.stats.name)),
            );
        }
        let evolves = b.role == Role::Tech && self.has_evolutions();
        if evolves {
            let kids: Vec<_> = self
                .r
                .units
                .iter()
                .filter(|u| matches!(u.from, From::Evolves(_)))
                .map(|u| u.stats.name.clone())
                .collect();
            fact(&mut links, "Evolutions", join(kids));
        }
        if b.upgrades_from.is_some() && b.role == Role::Main {
            let (bs, us) = self.unlocks(b.tier);
            if !bs.is_empty() {
                fact(&mut links, "Unlocks (buildings)", join(bs));
            }
            if !us.is_empty() {
                fact(&mut links, "Unlocks (troops)", join(us));
            }
        }
        fact(&mut links, "In the data", ascii(&b.effect));

        // Upgrades tab.
        let notes: HashMap<&str, &str> = text
            .map(|t| {
                t.notes
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        let mut upgrades = Vec::new();
        if let Some(n) = next {
            let mut uf = Facts::new();
            fact(&mut uf, "Hit points", thousands(u32::from(n.hp)));
            if n.supply > 0 {
                fact(&mut uf, "Supply cap", format!("+{}", n.supply));
            }
            fact(&mut uf, "Armour", format!("{:?}", n.armour));
            if n.role == Role::Main {
                let (bs, us) = self.unlocks(n.tier);
                if !bs.is_empty() {
                    fact(&mut uf, "Unlocks buildings", join(bs));
                }
                if !us.is_empty() {
                    fact(&mut uf, "Unlocks troops", join(us));
                }
            }
            fact(&mut uf, "In the data", ascii(&n.effect));
            upgrades.push(GuideUpgrade {
                name: n.name.clone(),
                tag: if n.flags.contains(&BuildingFlag::Gate) {
                    "Gatehouse".into()
                } else {
                    format!("Tier {} upgrade", n.tier)
                },
                cost: format!("{}, {}", cost_text(n.cost), secs(n.build_time)),
                benefit: ascii(notes.get(n.name.as_str()).copied().unwrap_or(&n.effect)),
                facts: uf,
            });
        }
        if let Some(note) = notes.get("Research") {
            upgrades.push(GuideUpgrade {
                name: "Research".into(),
                tag: "Planned".into(),
                cost: "not designed yet".into(),
                benefit: ascii(note),
                facts: Facts::new(),
            });
        }
        if evolves {
            for u in self
                .r
                .units
                .iter()
                .filter(|u| matches!(u.from, From::Evolves(_)))
            {
                upgrades.push(self.evolution(u));
            }
        }
        for u in trains {
            upgrades.push(GuideUpgrade {
                name: u.stats.name.clone(),
                tag: format!("Trains, tier {}", u.tier),
                cost: format!(
                    "{}, {}, {} supply",
                    cost_text(u.cost),
                    secs(u.train_time),
                    u.supply
                ),
                benefit: self.tagline(&u.stats.name),
                facts: self.unit_summary_facts(u),
            });
        }
        for h in hires {
            upgrades.push(self.hero_upgrade(h));
        }

        let name = b.name.clone();
        GuideEntry {
            kind: format!("{}, tier {}", role_name(b.role), b.tier),
            tag: format!("T{}", b.tier),
            summary: text.map_or_else(String::new, |t| ascii(&t.summary)),
            tips: text.map_or_else(Vec::new, tips_of),
            facts,
            links,
            upgrades,
            no_upgrades: "Nothing to upgrade or train here.".into(),
            name,
        }
    }

    fn evolution(&self, u: &RUnit) -> GuideUpgrade {
        let parent = match &u.from {
            From::Evolves(p) => p.as_str(),
            _ => "",
        };
        let mut facts = self.unit_summary_facts(u);
        fact(&mut facts, "Evolves from", parent);
        GuideUpgrade {
            name: u.stats.name.clone(),
            tag: format!("Evolution of {parent}"),
            cost: format!(
                "{}, {} in the cocoon",
                cost_text(u.cost),
                secs(u.train_time)
            ),
            benefit: self.tagline(&u.stats.name),
            facts,
        }
    }

    fn hero_upgrade(&self, h: &RHero) -> GuideUpgrade {
        let mut facts = Facts::new();
        fact(&mut facts, "Role", h.role.clone());
        fact(
            &mut facts,
            "Hit points",
            thousands(u32::from(h.stats.hp_per_model)),
        );
        fact(&mut facts, "Hire", HERO_RULES);
        GuideUpgrade {
            name: h.stats.name.clone(),
            tag: "Hires, hero".into(),
            cost: format!("hired at the {}", h.hired_at),
            benefit: self.tagline(&h.stats.name),
            facts,
        }
    }

    fn class_links(&self, links: &mut Facts, class: &str) {
        let (strong, weak) = self.counters.lines(&self.classes.classes, class);
        fact(
            links,
            "Strong against",
            if strong.is_empty() {
                "no class in particular".to_string()
            } else {
                join(strong)
            },
        );
        fact(
            links,
            "Weak against",
            if weak.is_empty() {
                "no class in particular".to_string()
            } else {
                join(weak)
            },
        );
    }

    fn unit(&self, u: &RUnit) -> GuideEntry {
        let name = u.stats.name.clone();
        let summoned = matches!(u.from, From::Summoned(_));
        let mut facts = Facts::new();
        fact(&mut facts, "Class", class_name(u.class));
        if summoned {
            fact(&mut facts, "Cost", "none: summoned, no supply");
        } else {
            fact(&mut facts, "Tier", u.tier.to_string());
            fact(&mut facts, "Cost", cost_text(u.cost));
            fact(&mut facts, "Supply", u.supply.to_string());
            let verb = if matches!(u.from, From::Evolves(_)) {
                "Cocoon time"
            } else {
                "Train time"
            };
            fact(&mut facts, verb, secs(u.train_time));
        }
        stat_facts(&mut facts, &u.stats, false);
        let flags: Vec<&str> = u.flags.iter().filter_map(|&f| unit_flag_text(f)).collect();
        if !flags.is_empty() {
            fact(&mut facts, "Traits", join(flags));
        }

        let mut links = Facts::new();
        match &u.from {
            From::Building(b) => fact(&mut links, "Trained at", b.clone()),
            From::Evolves(p) => {
                let at = self
                    .r
                    .buildings
                    .iter()
                    .find(|b| b.role == Role::Tech)
                    .map_or(String::new(), |b| format!(" (unlocked by the {})", b.name));
                fact(&mut links, "Evolves from", format!("{p}{at}"));
            }
            From::Summoned(by) => fact(&mut links, "Summoned by", by.clone()),
        }
        let kids = self.evolutions_of(&name);
        if !kids.is_empty() {
            fact(
                &mut links,
                "Evolves into",
                join(kids.iter().map(|k| &k.stats.name)),
            );
        }
        self.class_links(&mut links, class_name(u.class));

        let mut upgrades: Vec<GuideUpgrade> = kids.iter().map(|k| self.evolution(k)).collect();
        upgrades.extend(u.abilities.iter().map(|a| ability_upgrade(a, false)));

        let tag = if summoned {
            "summon".to_string()
        } else {
            format!("T{}", u.tier)
        };
        let kind = if summoned {
            format!("Summoned {} regiment", class_name(u.class))
        } else if u.class == Class::Worker {
            format!("Worker, tier {}", u.tier)
        } else {
            format!("{} regiment, tier {}", class_name(u.class), u.tier)
        };
        GuideEntry {
            kind,
            tag,
            summary: self
                .text_u
                .get(name.as_str())
                .map_or_else(String::new, |t| ascii(&t.summary)),
            facts,
            links,
            upgrades,
            tips: Vec::new(),
            no_upgrades: "No evolutions or abilities.".into(),
            name,
        }
    }

    fn hero(&self, h: &RHero) -> GuideEntry {
        let name = h.stats.name.clone();
        let mut facts = Facts::new();
        fact(&mut facts, "Role", h.role.clone());
        fact(&mut facts, "Hired at", h.hired_at.clone());
        fact(&mut facts, "Hire", HERO_RULES);
        stat_facts(&mut facts, &h.stats, true);
        let g = &h.per_level;
        let mut per = format!("+{} hp, +{} melee damage", g.hp, g.melee_damage);
        if g.missile_damage > 0 {
            per.push_str(&format!(", +{} missile damage", g.missile_damage));
        }
        fact(&mut facts, "Per level", per);

        let mut links = Facts::new();
        fact(&mut links, "Hired at", h.hired_at.clone());
        fact(
            &mut links,
            "Against classes",
            "Heroes take and deal x1.0 against every class",
        );

        GuideEntry {
            kind: format!("Hero, {}", h.role.to_lowercase()),
            tag: "hero".into(),
            summary: self
                .text_u
                .get(name.as_str())
                .map_or_else(String::new, |t| ascii(&t.summary)),
            facts,
            links,
            upgrades: h
                .abilities
                .iter()
                .map(|a| ability_upgrade(a, true))
                .collect(),
            tips: Vec::new(),
            no_upgrades: "No abilities.".into(),
            name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rosters() -> Vec<Roster> {
        EMBEDDED
            .rosters
            .iter()
            .map(|s| ron::from_str(s).expect("roster parses"))
            .collect()
    }

    fn guides() -> Vec<GuideFile> {
        EMBEDDED
            .guides
            .iter()
            .map(|s| parse_guide(s).expect("guide file parses"))
            .collect()
    }

    fn dupes(names: &[&str]) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        names
            .iter()
            .filter(|n| !seen.insert(**n))
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn every_building_and_unit_has_a_guide_entry_and_nothing_is_stale() {
        for ((r, g), f) in rosters().iter().zip(guides()).zip(Faction::ALL) {
            assert_eq!(g.faction, f, "guide files are in faction order");
            let bs: Vec<&str> = r.buildings.iter().map(|b| b.name.as_str()).collect();
            let gb: Vec<&str> = g.buildings.iter().map(|b| b.name.as_str()).collect();
            let us: Vec<&str> = r
                .units
                .iter()
                .map(|u| u.stats.name.as_str())
                .chain(r.heroes.iter().map(|h| h.stats.name.as_str()))
                .collect();
            let gu: Vec<&str> = g.units.iter().map(|u| u.name.as_str()).collect();
            for (what, roster, guide) in [("building", &bs, &gb), ("unit", &us, &gu)] {
                for n in roster {
                    assert!(guide.contains(n), "{f:?}: no guide entry for {what} {n}");
                }
                for n in guide {
                    assert!(
                        roster.contains(n),
                        "{f:?}: guide entry {n} is not a {what} in the roster"
                    );
                }
                assert!(
                    dupes(guide).is_empty(),
                    "{f:?}: repeated {what} entries {:?}",
                    dupes(guide)
                );
            }
        }
    }

    #[test]
    fn the_tip_book_has_every_building_once() {
        let book = tip_book();
        let total: usize = rosters().iter().map(|r| r.buildings.len()).sum();
        assert_eq!(book.len(), total, "building names repeat across factions");
        for (name, tips) in &book {
            assert!(!tips.is_empty(), "{name} has no tips");
        }
    }

    #[test]
    fn guide_text_is_plain_ascii_and_fits() {
        for g in guides() {
            for b in &g.buildings {
                check_text(&b.name, &b.summary, MAX_SUMMARY);
                assert!(
                    (2..=3).contains(&b.tips.len()),
                    "{}: wants 2 or 3 tips, has {}",
                    b.name,
                    b.tips.len()
                );
                for t in &b.tips {
                    check_text(&b.name, &t.text, MAX_TIP);
                }
                for (what, benefit) in &b.notes {
                    check_text(what, benefit, 220);
                }
            }
            for u in &g.units {
                check_text(&u.name, &u.summary, MAX_SUMMARY);
                check_text(&u.name, &u.tagline, MAX_TAGLINE);
                assert!(
                    u.summary.len() > 60 && u.tagline.len() > 10,
                    "{} is too thin",
                    u.name
                );
            }
        }
    }

    fn check_text(who: &str, text: &str, max: usize) {
        assert!(!text.trim().is_empty(), "{who}: empty text");
        assert!(
            text.len() <= max,
            "{who}: {} characters, at most {max}",
            text.len()
        );
        assert!(
            text.chars().all(|c| (' '..='~').contains(&c)),
            "{who}: the font draws only printable ASCII"
        );
        assert!(
            !text.contains("  ") && text == text.trim(),
            "{who}: stray spaces"
        );
    }

    #[test]
    fn notes_name_real_upgrades_and_every_tier_upgrade_has_one() {
        for ((r, g), f) in rosters().iter().zip(guides()).zip(Faction::ALL) {
            for b in &g.buildings {
                let rb = r.buildings.iter().find(|x| x.name == b.name).unwrap();
                let next = r
                    .buildings
                    .iter()
                    .find(|n| n.upgrades_from.as_deref() == Some(&rb.name));
                for (what, _) in &b.notes {
                    let ok = what == "Research" && matches!(rb.role, Role::Tech | Role::Economy)
                        || next.is_some_and(|n| &n.name == what);
                    assert!(
                        ok,
                        "{f:?}: {}'s note {what:?} names nothing it upgrades",
                        b.name
                    );
                }
                if let Some(n) = next {
                    assert!(
                        b.notes.iter().any(|(w, _)| w == &n.name),
                        "{f:?}: {} needs a benefit line for its upgrade {}",
                        b.name,
                        n.name
                    );
                }
                // Research is not designed yet (docs/21-buildings.md): any
                // building whose rules text promises it says so honestly.
                let promises = rb.effect.contains("Researches") || rb.effect.contains("research");
                assert_eq!(
                    b.notes.iter().any(|(w, _)| w == "Research"),
                    promises,
                    "{f:?}: {} research note",
                    b.name
                );
            }
        }
    }

    #[test]
    fn the_guide_joins_every_faction() {
        let d = load();
        assert_eq!(d.factions.len(), 4);
        let names: Vec<_> = d.factions.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            ["Ember Clans", "Brood", "Shard Choir", "Carrion Concord"]
        );
        let rs = rosters();
        for (f, r) in d.factions.iter().zip(&rs) {
            assert_eq!(f.buildings.len(), r.buildings.len());
            assert_eq!(f.troops.len(), r.units.len() + r.heroes.len());
            for e in f.buildings.iter().chain(&f.troops) {
                assert!(!e.summary.is_empty(), "{} has no summary", e.name);
                assert!(
                    !e.facts.is_empty() && !e.links.is_empty(),
                    "{} is bare",
                    e.name
                );
                for u in &e.upgrades {
                    assert!(
                        !u.benefit.is_empty(),
                        "{}: {} has no benefit line",
                        e.name,
                        u.name
                    );
                    assert!(!u.tag.is_empty() && !u.cost.is_empty());
                }
                let all = [&e.name, &e.kind, &e.tag, &e.summary, &e.no_upgrades]
                    .into_iter()
                    .cloned()
                    .chain(e.facts.iter().flat_map(|(a, b)| [a.clone(), b.clone()]))
                    .chain(e.links.iter().flat_map(|(a, b)| [a.clone(), b.clone()]))
                    .chain(e.upgrades.iter().flat_map(|u| {
                        [
                            u.name.clone(),
                            u.tag.clone(),
                            u.cost.clone(),
                            u.benefit.clone(),
                        ]
                        .into_iter()
                        .chain(u.facts.iter().flat_map(|(a, b)| [a.clone(), b.clone()]))
                    }));
                for s in all {
                    assert!(
                        s.chars().all(|c| (' '..='~').contains(&c)),
                        "{}: {s:?} has characters the font cannot draw",
                        e.name
                    );
                    assert!(!s.contains("Other"), "{}: an unknown enum in {s:?}", e.name);
                }
            }
        }
        // Counts to report: 15/10/10/11 buildings, 15/15/11/14 troops.
        let counts: Vec<_> = d
            .factions
            .iter()
            .map(|f| (f.buildings.len(), f.troops.len()))
            .collect();
        eprintln!("guide entries (buildings, troops) per faction: {counts:?}");
    }

    fn find<'a>(d: &'a GuideData, f: usize, tab_buildings: bool, name: &str) -> &'a GuideEntry {
        let fg = &d.factions[f];
        let list = if tab_buildings {
            &fg.buildings
        } else {
            &fg.troops
        };
        list.iter().find(|e| e.name == name).unwrap()
    }

    fn fact_of<'a>(rows: &'a [(String, String)], label: &str) -> &'a str {
        &rows.iter().find(|(l, _)| l == label).unwrap().1
    }

    #[test]
    fn each_gate_guides_the_player_to_its_gatehouse() {
        let d = load();
        for (f, gate, house) in [
            (0, "Gate", "Yagura Gatehouse"),
            (3, "Iron Gate", "Gate of Debts"),
            (1, "Maw Gate", "Chitin Maw"),
        ] {
            let g = find(&d, f, true, gate);
            assert_eq!(fact_of(&g.links, "Upgrades to"), house);
            let up = g.upgrades.iter().find(|u| u.name == house).unwrap();
            assert_eq!(up.tag, "Gatehouse");
            assert!(
                up.facts
                    .iter()
                    .all(|(l, _)| !l.starts_with("Unlocks") && l != "Supply cap")
            );
            assert!(g.tips.iter().any(|(_, t)| t.contains(house)), "{gate}");
            let h = find(&d, f, true, house);
            assert_eq!(
                fact_of(&h.facts, "Needs"),
                format!("An upgrade of the {gate}")
            );
            assert_eq!(fact_of(&h.links, "Upgrades from"), gate);
            assert!(fact_of(&h.facts, "Features").starts_with("Gatehouse"));
            assert_eq!(fact_of(&h.facts, "Size"), "1 x 3 tiles");
        }
    }

    #[test]
    fn numbers_are_the_rosters_numbers() {
        let d = load();
        let hall = find(&d, 0, true, "Clan Hall");
        assert_eq!(fact_of(&hall.facts, "Cost"), "400 Grub");
        assert_eq!(fact_of(&hall.facts, "Hit points"), "3,000");
        let keep = hall
            .upgrades
            .iter()
            .find(|u| u.name == "Castle Keep")
            .unwrap();
        assert_eq!(keep.cost, "300 Grub, 100 Marrow, 60 s");
        assert_eq!(keep.tag, "Tier 2 upgrade");
        let unlocks = fact_of(&keep.facts, "Unlocks troops");
        assert!(
            unlocks.contains("Katana Samurai") && unlocks.contains("War Kite"),
            "{unlocks}"
        );
        assert!(fact_of(&keep.facts, "Unlocks buildings").contains("Stables"));
        // The Brood's Chrysalis Chamber lists the four evolutions with costs.
        let chrysalis = find(&d, 1, true, "Chrysalis Chamber");
        let evo: Vec<_> = chrysalis
            .upgrades
            .iter()
            .filter(|u| u.tag.starts_with("Evolution"))
            .collect();
        assert_eq!(evo.len(), 4);
        let spit = evo.iter().find(|u| u.name == "Spitters").unwrap();
        assert_eq!(spit.cost, "60 Grub, 20 Marrow, 15 s in the cocoon");
        // A unit.
        let yari = find(&d, 0, false, "Yari Ashigaru");
        assert_eq!(fact_of(&yari.facts, "Cost"), "120 Grub");
        assert_eq!(
            fact_of(&yari.facts, "Regiment"),
            "28 models, 8 hp each (224 in all)"
        );
        assert!(fact_of(&yari.facts, "Weapons").contains("Reach"));
        assert_eq!(fact_of(&yari.links, "Trained at"), "Dojo");
        assert!(fact_of(&yari.links, "Strong against").starts_with("Cavalry (x1.5)"));
        assert!(fact_of(&yari.links, "Weak against").contains("Ranged"));
        assert!(
            yari.upgrades
                .iter()
                .any(|u| u.name == "Brace" && u.tag == "Toggle")
        );
        // Skitterlings evolve into Spitters and Burrowers.
        let skit = find(&d, 1, false, "Skitterlings");
        assert_eq!(fact_of(&skit.links, "Evolves into"), "Spitters, Burrowers");
        assert_eq!(
            skit.upgrades
                .iter()
                .filter(|u| u.tag.starts_with("Evolution"))
                .count(),
            2
        );
        // Heroes carry their ultimate at level 6.
        let daimyo = find(&d, 0, false, "The Daimyo");
        assert!(
            daimyo
                .upgrades
                .iter()
                .any(|u| u.tag == "Ultimate (level 6)")
        );
        assert_eq!(daimyo.upgrades.len(), 4);
        // Research is honest about not existing.
        let forge = find(&d, 0, true, "Forge");
        let research = forge
            .upgrades
            .iter()
            .find(|u| u.name == "Research")
            .unwrap();
        assert_eq!(research.cost, "not designed yet");
        // A production building lists what it trains.
        let dojo = find(&d, 0, true, "Dojo");
        assert_eq!(dojo.upgrades.len(), 3);
        assert_eq!(
            fact_of(&dojo.links, "Trains"),
            "Yari Ashigaru, Yumi Ashigaru, Katana Samurai"
        );
    }

    #[test]
    fn a_missing_guide_entry_or_broken_file_is_an_error_not_a_panic() {
        let mut bad = Sources { ..EMBEDDED };
        bad.classes = "(oops";
        assert!(build(&bad).is_err());
        let mut swapped = Sources { ..EMBEDDED };
        swapped.guides = [
            EMBEDDED.guides[1],
            EMBEDDED.guides[0],
            EMBEDDED.guides[2],
            EMBEDDED.guides[3],
        ];
        assert!(build(&swapped).unwrap_err().contains("is for"));
    }

    #[test]
    fn formatting_helpers() {
        assert_eq!(thousands(3000), "3,000");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1234567), "1,234,567");
        assert_eq!(num(3.0), "3");
        assert_eq!(num(2.75), "2.75");
        assert_eq!(num(0.5), "0.5");
        assert_eq!(ascii("a\u{2014}b\u{00e9}"), "a-b?");
    }
}
