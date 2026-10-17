//! The faction roster files (`data/rosters/*.ron`, docs/17-unit-rosters.md and
//! docs/21-buildings.md) as Rust types. The baker turns them into the building
//! and Conquest unit tables; `tests/rosters.rs` checks them against the design
//! rules. `Stats` is also the shape of a `data/units.ron` entry.

use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Roster {
    pub faction: FactionDef,
    pub meter: Meter,
    /// What a player starts Conquest with (docs/04-economy.md).
    pub starting_buildings: Vec<String>,
    /// Unit name and count: workers, and a Concord Caravan.
    pub starting_units: Vec<(String, u8)>,
    pub buildings: Vec<Building>,
    pub units: Vec<Unit>,
    pub heroes: Vec<Hero>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meter {
    pub name: String,
    pub max: u16,
    pub abilities: Vec<Ability>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Building {
    pub name: String,
    pub role: Role,
    /// The main-building tier needed to build it.
    pub tier: u8,
    /// Main-building tiers 2 and 3: the building this one upgrades.
    #[serde(default)]
    pub upgrades_from: Option<String>,
    pub cost: Cost,
    /// Seconds at Standard tempo.
    pub build_time: f64,
    pub hp: u16,
    pub armour: Armour,
    /// Tiles across and deep.
    pub footprint: (u8, u8),
    #[serde(default)]
    pub supply: u8,
    #[serde(default)]
    pub flags: Vec<BuildingFlag>,
    #[serde(default)]
    pub attack: Option<Attack>,
    /// How far its field of power or its root-veins reach from its edge,
    /// in tiles (`PowersField` and `Veins` buildings).
    #[serde(default)]
    pub field: u8,
    /// Choir: the nodes it works with no workers.
    #[serde(default)]
    pub tunes: Option<Tunes>,
    /// Choir: how far apart, centre to centre in tiles, two of these link
    /// (three mutually linked form a chord); 0 for none.
    #[serde(default)]
    pub links: u8,
    pub effect: String,
}

/// The nodes a building tunes, and how far from its edge, in tiles.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tunes {
    pub node: Node,
    pub tiles: u8,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Node {
    Seeps,
    Veins,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Main,
    Outpost,
    Housing,
    Economy,
    Production,
    Tech,
    Heroes,
    Defence,
    Wall,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Armour {
    Fortified,
    Structure,
    Living,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildingFlag {
    DropOffGrub,
    DropOffMarrow,
    /// Choir: powers buildings around it. Every other Choir building needs
    /// power to work.
    PowersField,
    AntiAir,
    /// Brood: holds larvae and grows them (docs/04, "Larvae").
    Larvae,
    /// Brood: spreads root-veins (docs/03, "the vein network").
    Veins,
    /// Choir: works only inside a `PowersField` building's field.
    NeedsPower,
    /// Clans: earns its owner Honor while it stands (docs/03, "Honor").
    Honor,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attack {
    pub range: f64,
    pub damage: u16,
    pub reload: f64,
    pub shots: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unit {
    /// 1 to 3; 0 for summoned units.
    pub tier: u8,
    pub class: Class,
    pub cost: Cost,
    pub supply: u8,
    /// Seconds to train, or to cocoon for a Brood evolution.
    pub train_time: f64,
    pub from: From,
    #[serde(default)]
    pub flags: Vec<Flag>,
    pub abilities: Vec<Ability>,
    pub stats: Stats,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hero {
    pub role: String,
    pub hired_at: String,
    pub per_level: Growth,
    #[serde(default)]
    pub flags: Vec<Flag>,
    pub abilities: Vec<Ability>,
    pub stats: Stats,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Growth {
    pub hp: u16,
    pub melee_damage: u16,
    #[serde(default)]
    pub missile_damage: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cost {
    pub grub: u16,
    pub marrow: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ability {
    pub name: String,
    pub kind: Kind,
    /// Plain-language description, with hero ranks written 1/2/3.
    pub effect: String,
    pub primitives: Vec<Primitive>,
    #[serde(default)]
    pub cooldown: f64,
    #[serde(default)]
    pub range: f64,
    #[serde(default)]
    pub radius: f64,
    #[serde(default)]
    pub duration: f64,
    /// Faction-meter abilities only: meter cost and tier.
    #[serde(default)]
    pub cost: u16,
    #[serde(default)]
    pub tier: u8,
    /// Faction-meter abilities the game has built: what the simulation does.
    #[serde(default)]
    pub built: Option<Built>,
}

/// What a faction-meter ability the game has built does.
#[derive(Deserialize, Clone, PartialEq, Eq, Debug)]
pub enum Built {
    /// A building or regiment works, reloads and strikes this many percent
    /// faster for the duration.
    Haste(u8),
    /// Regiments step to one of the caster's buildings of this name.
    Shift(String),
    /// The caster gains this much Grub at once.
    Grant(u16),
    /// The chosen regiments march this many percent faster for the
    /// duration.
    March(u8),
    /// The caster's regiments within the ability's radius of the chosen
    /// point can't rout for the duration.
    Steadfast,
    /// A regiment of the named summoned unit takes the field beside one of
    /// the caster's Shrines for the duration.
    Summon(String),
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
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
pub enum From {
    Building(String),
    Evolves(String),
    Summoned(String),
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flag {
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
    /// Brood: hatches from a larva (docs/04, "Larvae").
    Hatches,
    /// Brood: a worker used up by the building it starts.
    Consumed,
    /// Choir: a worker that builds and mends but does not gather.
    BuildsOnly,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
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
pub enum Primitive {
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
pub struct Stats {
    pub name: String,
    pub faction: FactionDef,
    pub models: u16,
    pub hp_per_model: u16,
    pub files: u16,
    pub speed: f64,
    pub turn_rate: f64,
    pub file_spacing: f64,
    pub rank_spacing: f64,
    pub melee_attack: u8,
    pub melee_defence: u8,
    pub melee_damage: u16,
    pub charge_bonus: u8,
    #[serde(default)]
    pub morale: u8,
    #[serde(default)]
    pub never_routs: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ranged: Option<Ranged>,
    #[serde(default, skip_serializing_if = "Mass::is_medium")]
    pub mass: Mass,
    #[serde(default, skip_serializing_if = "is_false")]
    pub reach: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub swing_room: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub shield: bool,
    /// Clans: more melee defence standing still in close order. A roster
    /// unit gets it from its "Formation Discipline" ability instead.
    #[serde(default, skip_serializing_if = "is_false")]
    pub discipline: bool,
    /// Cost in a Field Battle army's points budget. `data/units.ron` sets it
    /// (above 0); the rosters leave it out, which reads as 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub points: u16,
}

fn is_false(v: &bool) -> bool {
    !v
}

fn is_zero(v: &u16) -> bool {
    *v == 0
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mass {
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
pub struct Ranged {
    pub range: f64,
    pub damage: u16,
    pub reload: f64,
    pub accuracy: u8,
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum FactionDef {
    EmberClans,
    Brood,
    Choir,
    CarrionConcord,
}
