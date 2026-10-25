//! Buildings in the sandbox (Conquest, docs/21-buildings.md): the baked
//! table as the simulation reads it, and how the field draws, places and
//! picks them. A building with models shows its construction stage
//! ([`Looks`]); one without is a placeholder box.

use gc_assets::model::Models;
use gc_data::conquest::{self as record, BuildingRecord};
use gc_data::{Faction, GameData};
use gc_render::{BoxInstance, MAX_BOX_INSTANCES};
use gc_sim::structures::{Armour, Role, TowerAttack};
use gc_sim::{
    BuildingId, BuildingStats, ConquestSetup, ConquestUnit, HeartView, PlayerId, Refusal, Resource,
    StructureHandle, StructureView, World,
};

use crate::Party;
use crate::scenery::{Extra, Model};
use gc_ui::conquest::node_color;
use glam::{Vec2, Vec3};

// The baked flag bits mean the same to the simulation.
const _: () = assert!(
    record::BUILDING_DROP_OFF_GRUB == gc_sim::structures::DROP_OFF_GRUB
        && record::BUILDING_DROP_OFF_MARROW == gc_sim::structures::DROP_OFF_MARROW
        && record::BUILDING_POWERS_FIELD == gc_sim::structures::POWERS_FIELD
        && record::BUILDING_ANTI_AIR == gc_sim::structures::ANTI_AIR
        && record::BUILDING_LARVAE == gc_sim::structures::LARVAE
        && record::BUILDING_VEINS == gc_sim::structures::VEINS
        && record::BUILDING_NEEDS_POWER == gc_sim::structures::NEEDS_POWER
        && record::BUILDING_HONOR == gc_sim::structures::HONOR
        && record::BUILDING_TRADE_POST == gc_sim::structures::TRADE_POST
        && record::BUILDING_TOLL == gc_sim::structures::TOLL
        && record::BUILDING_SILENCES == gc_sim::structures::SILENCES
        && record::BUILDING_GATE == gc_sim::structures::GATE
        && record::BUILDING_LINE == gc_sim::structures::LINE
        && record::BUILDING_PASS == gc_sim::structures::PASS
        && record::BUILDING_GROWN == gc_sim::structures::GROWN
        && record::MAX_REACH_TILES == gc_sim::structures::MAX_REACH
        && record::MAX_FOOTPRINT == gc_sim::structures::MAX_FOOTPRINT
);

/// How many steps back an upgrade chain runs from `stats[i]`, plus one.
fn level_in(stats: &[BuildingStats], i: usize) -> u8 {
    let (mut level, mut at) = (1u8, i);
    // A bounded walk, in case a chain loops.
    while let Some(from) = stats.get(at).and_then(|s| s.upgrades_from) {
        level += 1;
        at = usize::from(from);
        if level >= 8 {
            break;
        }
    }
    level
}

/// Height of a wall, of most buildings and of a main building, in tiles.
const WALL_HEIGHT: f32 = 1.5;
/// How thick a drawn wall is, in tiles, and how far a piece reaches past
/// its share of the line at a joint. At an end of the line it reaches half
/// a step, to the edge of its tile.
const LINE_THICK: f32 = 0.5;
const LINE_JOINT: f32 = 0.05;
/// Bone Fence: pale bone, a little lower than a palisade.
const BONE_COLOR: [u8; 3] = [214, 206, 180];
const BONE_HEIGHT: f32 = 1.2;
/// Light Curtain: thin and tall, glowing pale cyan. Dropped, it is a faint
/// low line on the ground.
const CURTAIN_COLOR: [u8; 3] = [150, 220, 255];
const CURTAIN_THICK: f32 = 0.16;
const CURTAIN_HEIGHT: f32 = 1.7;
const CURTAIN_GLOW: u8 = 200;
const CURTAIN_DROPPED_HEIGHT: f32 = 0.06;
const CURTAIN_DROPPED_GLOW: u8 = 60;
/// Resin Ridge: amber resin, lower and wider than a palisade.
const RESIN_COLOR: [u8; 3] = [196, 126, 40];
const RESIN_HEIGHT: f32 = 1.0;
const RESIN_THICK: f32 = 0.8;
const RESIN_GLOW: u8 = 30;
/// A bridge deck: a flat slab a full tile wide, this thick, floating this
/// far above the ground the shader draws (the water's surface over a river),
/// so men crossing it stand on `DECK_LIFT + BRIDGE_THICK`.
pub const DECK_LIFT: f32 = 0.1;
const BRIDGE_THICK: f32 = 0.18;
/// How high a man crossing a bridge stands above the ground the shader
/// draws there.
pub const DECK_TOP: f32 = DECK_LIFT + BRIDGE_THICK;
/// The railings of a finished bridge: a rail along each edge and a post at
/// the middle of each tile, this high above the deck and this thin.
const RAIL_HEIGHT: f32 = 0.42;
const RAIL_THIN: f32 = 0.07;
const POST_THIN: f32 = 0.12;
/// An unfinished bridge tile shows two posts, this high at the start and
/// taller as it goes up to the railing's height.
const FRAME_LOW: f32 = 0.2;
/// Deck colours: Ember timber, Concord bone, Choir glass, Brood chitin.
const TIMBER_DECK: [u8; 3] = [150, 106, 62];
const BONE_DECK: [u8; 3] = [226, 218, 194];
const GLASS_DECK: [u8; 3] = [150, 226, 242];
const GLASS_DECK_GLOW: u8 = 90;
const CHITIN_DECK: [u8; 3] = [92, 62, 70];
/// Field works: a stake line is a row of thin stakes (this many to a tile,
/// each a low post and a tip leaning out across the line), a barricade a
/// chunky waist-high slab with a beam and two posts. Heights in tiles.
const STAKE_HEIGHT: f32 = 0.62;
const STAKES_PER_TILE: usize = 4;
const STAKE_LOW: f32 = 0.38;
const STAKE_THIN: f32 = 0.09;
/// How far a stake's foot stands off the line, and how much further its
/// tip leans out, alternately to each side.
const STAKE_STAGGER: f32 = 0.13;
const STAKE_LEAN: f32 = 0.07;
/// The cord that binds a finished stake line, this high.
const STAKE_CORD: f32 = 0.22;
/// A hidden trap lies nearly flush with the ground; its health bar floats
/// this high.
const TRAP_HEIGHT: f32 = 0.16;
const BARRICADE_HEIGHT: f32 = 0.68;
const BARRICADE_BASE: f32 = 0.44;
const BARRICADE_THICK: f32 = 0.55;
/// A field work tile shares this much of the side's colour.
const FIELDWORK_TEAM_TINT: u32 = 25;
/// Each wall level past the first stands this much taller and thicker.
const LEVEL_RISE: f32 = 0.45;
const LEVEL_THICKEN: f32 = 0.1;
/// The coping slab on a raised wall, and the merlons on a third level: how
/// tall they stand and how much of a tile's length a merlon takes.
const COPING_HEIGHT: f32 = 0.12;
const MERLON_HEIGHT: f32 = 0.3;
const MERLON_SHARE: f32 = 0.3;
/// A raised wall's colours per faction, level 2 then level 3: its body
/// and its coping (and merlons).
const EMBER_RAISED: [([u8; 3], [u8; 3]); 2] = [
    ([198, 190, 172], [128, 118, 104]),
    ([152, 148, 142], [66, 58, 64]),
];
const CONCORD_RAISED: [([u8; 3], [u8; 3]); 2] = [
    ([192, 188, 172], [108, 112, 120]),
    ([88, 86, 96], [218, 172, 72]),
];
const CHOIR_RAISED: [([u8; 3], [u8; 3]); 2] = [
    ([110, 222, 236], [170, 245, 255]),
    ([196, 240, 255], [255, 255, 255]),
];
const BROOD_RAISED: [([u8; 3], [u8; 3]); 2] = [
    ([104, 72, 90], [150, 112, 120]),
    ([62, 42, 66], [206, 176, 124]),
];
/// How much glow each wall level past the first adds.
const LEVEL_GLOW: u8 = 20;
/// How much of the owner's colour (out of 100) a raised wall's body keeps, so
/// it still reads as that side's.
const LEVEL_TEAM_TINT: u32 = 35;
/// A building model's walls are what stands in its lowest share.
const WALL_SHARE: f32 = 0.3;
/// A gate's jamb posts stand a little over its walls' height.
const POST_RISE: f32 = 0.3;
/// How high above the ground a gatehouse's deck is, in tiles: where men
/// stand on it, and where a gatehouse drawn as boxes is roofed. The
/// gatehouse models are built to this height.
pub const GATEHOUSE_DECK: f32 = 2.0;
/// How far outside a building its upgrade crew stand, in tiles.
const CREW_GAP: f32 = 0.4;
/// How thick the deck slab of a gatehouse drawn as boxes is, in tiles.
const DECK_THICK: f32 = 0.25;
/// How far a ladder's foot stands from the middle of the wall it leans on
/// (half a tile, and a hair more).
const LADDER_STANDOFF: f32 = 0.52;
/// How far apart the three ladders against a wall tile stand, in tiles.
const LADDER_APART: f32 = 0.9;
/// How far along the wall a gate's jamb post reaches, in tiles.
const POST_LEN: f32 = 0.5;
/// How thick a gate's door leaves are, in tiles.
const LEAF_THICK: f32 = 0.15;
/// How tall a gate's door leaves are drawn (a bit under the posts).
const LEAF_HEIGHT: f32 = WALL_HEIGHT * 0.85;
const LEAF_COLOR: [u8; 4] = [112, 78, 46, 0];
const HEIGHT: f32 = 3.0;
const MAIN_HEIGHT: f32 = 5.0;
/// A building at no hit points would be this much of its colour; at full
/// hit points, all of it (in percent).
const DARKEST: u32 = 40;
/// The hit bar floats this far above the roof; thickness, depth and the
/// frame round the fill, in tiles. It is never shorter than `BAR_MIN`.
const BAR_LIFT: f32 = 0.9;
const BAR_THICK: f32 = 0.2;
const BAR_DEPTH: f32 = 0.3;
const BAR_FRAME: f32 = 0.06;
const BAR_MIN: f32 = 2.0;
const BAR_BACK: [u8; 4] = [18, 18, 18, 0];
/// A ghost with nowhere to stand.
const REFUSED: [u8; 4] = [210, 40, 30, 110];
/// A ghost on overgrown ground, which the worker clears first (amber).
const OVERGROWN: [u8; 4] = [235, 170, 40, 120];
/// The glow on a ghost that fits.
const GHOST_GLOW: u8 = 110;
/// A wall being planned as a blueprint (pale blue), and a planned tile
/// already down (the same, fainter).
const PLAN_GHOST: [u8; 4] = [130, 185, 245, 120];
const PLANNED: [u8; 4] = [120, 165, 225, 20];
/// A gate the ring tool suggests (pale gold).
const SUGGESTED: [u8; 4] = [240, 214, 110, 150];
/// The glow of a blueprint waiting on clearing.
const BLUEPRINT_GLOW: u8 = 70;
/// How much of its full height a blueprint stands.
const BLUEPRINT_HEIGHT: f32 = 0.25;
/// A gathering spot is a low mound this tall and, while full, this wide
/// (in tiles); it shrinks to `NODE_LEAST` percent as it runs down.
const NODE_HEIGHT: f32 = 0.35;
const NODE_WIDTH: f32 = 1.8;
/// A temporary Grub pile's height, against a node's.
const PILE_HEIGHT: f32 = 0.5;
const NODE_LEAST: f32 = 40.0;
/// Seeps and veins glow in their minimap colours; dry ones don't.
const NODE_GLOW: u8 = 90;
/// As models, a full or part-used spot is drawn this bright (128 as
/// baked) and a dry one this dark.
const NODE_SHADE: u8 = 140;
const NODE_DRY_SHADE: u8 = 62;
/// A full spot's model is this much bigger than baked (about 2.5 tiles
/// across), so it reads from the RTS camera; workers gather within 2.
const NODE_MODEL_SCALE: f32 = 1.2;
/// The red cross on a spot the veins do not reach floats this high, above
/// the seep's pool and through the vein's bones.
const CUT_MARK_LIFT: f32 = 0.6;
/// The Titan's Heart: its stone and the plinth under it, in tiles.
const HEART_WIDTH: f32 = 1.6;
const HEART_HEIGHT: f32 = 3.2;
pub const HEART_BASE: f32 = 3.5;
const HEART_COLOR: [u8; 3] = [150, 20, 70];
const HEART_GLOW: u8 = 140;
/// The selected building's white rim: one tile beyond its footprint, this
/// thick (tiles).
const PLATE: f32 = 0.08;
const RIM_COLOR: [u8; 4] = [240, 240, 240, 120];
/// The selected building's rally point: a pole this tall with a flag in
/// its owner's colour (tiles).
const POLE: [f32; 3] = [0.12, 2.4, 0.12];
const FLAG: [f32; 3] = [0.9, 0.55, 0.06];
const POLE_COLOR: [u8; 4] = [70, 55, 40, 0];

/// A building's stats as the simulation reads them.
pub fn stats_of(b: &BuildingRecord) -> BuildingStats {
    BuildingStats {
        role: match b.role() {
            record::Role::Main => Role::Main,
            record::Role::Outpost => Role::Outpost,
            record::Role::Housing => Role::Housing,
            record::Role::Economy => Role::Economy,
            record::Role::Production => Role::Production,
            record::Role::Tech => Role::Tech,
            record::Role::Heroes => Role::Heroes,
            record::Role::Defence => Role::Defence,
            record::Role::Wall => Role::Wall,
            record::Role::Bridge => Role::Bridge,
            record::Role::Stakes => Role::Stakes,
            record::Role::Barricade => Role::Barricade,
            record::Role::Trap => Role::Trap,
        },
        tier: b.tier,
        upgrades_from: (b.upgrades_from != record::NONE).then_some(b.upgrades_from),
        cost_grub: b.cost_grub,
        cost_marrow: b.cost_marrow,
        build_ticks: b.build_ticks,
        hp: b.hp,
        armour: match b.armour() {
            record::Armour::Structure => Armour::Structure,
            record::Armour::Fortified => Armour::Fortified,
            record::Armour::Living => Armour::Living,
        },
        width: b.width,
        depth: b.depth,
        supply: b.supply,
        flags: b.flags,
        attack: (b.attack_range > 0).then(|| TowerAttack {
            range: b.attack_range(),
            damage: b.attack_damage,
            reload_ticks: b.attack_reload_ticks,
            shots: b.attack_shots,
        }),
        field: b.field_tiles,
        tunes: match b.tunes {
            record::TUNES_SEEPS => Some((Resource::Grub, b.tune_tiles)),
            record::TUNES_VEINS => Some((Resource::Marrow, b.tune_tiles)),
            _ => None,
        },
        links: b.link_tiles,
        // Validated when the table is loaded.
        twist: gc_sim::structures::WallTwist::from_u8(b.twist).unwrap_or_default(),
        // Validated when the table is loaded.
        trap: gc_sim::structures::Trap::from_kind(b.trap, b.trap_unit).unwrap_or_default(),
    }
}

/// Every building type in the game data, as Conquest worlds read them.
pub struct Table {
    pub stats: Vec<BuildingStats>,
    names: Vec<String>,
    factions: Vec<Faction>,
    /// Each type's level: 1 for a first-level building, one more for each
    /// type it upgrades from.
    levels: Vec<u8>,
    /// What the sandbox's Conquest world knows of each regiment type. The
    /// sandbox plays with the Field Battle units, which are all plain
    /// regiments (no workers, no supply); Conquest matches use their own
    /// unit table (`conquest::plan`).
    units: Vec<ConquestUnit>,
}

impl Table {
    pub fn new(data: &GameData) -> Table {
        let stats: Vec<BuildingStats> = data.buildings.iter().map(stats_of).collect();
        let levels = (0..stats.len()).map(|i| level_in(&stats, i)).collect();
        Table {
            levels,
            stats,
            names: data
                .buildings
                .iter()
                .map(|b| b.name().to_string())
                .collect(),
            factions: data.buildings.iter().map(BuildingRecord::faction).collect(),
            units: vec![ConquestUnit::default(); data.units.len()],
        }
    }

    /// Make `world`, fresh from `World::new`, the sandbox's Conquest world
    /// with these buildings (the sandbox is never recorded).
    pub fn conquer(&self, world: &mut World) {
        let done = world.conquest(&ConquestSetup {
            buildings: &self.stats,
            units: &self.units,
            ..ConquestSetup::default()
        });
        debug_assert!(done, "conquest() on a world that has already begun");
    }

    pub fn len(&self) -> usize {
        self.stats.len()
    }

    /// The building type `step` places on from `id`, wrapping round.
    pub fn step(&self, id: BuildingId, step: i32) -> BuildingId {
        (i32::from(id) + step).rem_euclid(self.len().max(1) as i32) as BuildingId
    }

    /// Building `id`'s faction.
    pub fn faction(&self, id: BuildingId) -> Option<Faction> {
        self.factions.get(usize::from(id)).copied()
    }

    /// `building`'s level: 1 unless it upgrades from another type.
    pub fn level(&self, building: BuildingId) -> u8 {
        self.levels.get(usize::from(building)).copied().unwrap_or(1)
    }

    /// Whether `building` is a wall drawn as a line.
    pub fn is_wall(&self, building: BuildingId) -> bool {
        (self.stats.get(usize::from(building)))
            .is_some_and(|s| s.flags & gc_sim::structures::LINE != 0)
    }

    /// Whether `building` is a bridge (a wall-like line over water).
    pub fn is_bridge(&self, building: BuildingId) -> bool {
        (self.stats.get(usize::from(building))).is_some_and(|s| s.role == Role::Bridge)
    }

    /// Whether `building` is a line of stakes or a barricade (field works,
    /// drawn with the wall tool but never a wall slab).
    pub fn is_fieldwork(&self, building: BuildingId) -> bool {
        (self.stats.get(usize::from(building)))
            .is_some_and(|s| matches!(s.role, Role::Stakes | Role::Barricade))
    }

    /// Whether `building` is a hidden trap (seen by its owner alone).
    pub fn is_trap(&self, building: BuildingId) -> bool {
        (self.stats.get(usize::from(building))).is_some_and(|s| s.role == Role::Trap)
    }

    /// The faction's building of the stakes kind, if it has one.
    pub fn stakes_of(&self, faction: Faction) -> Option<BuildingId> {
        (self.stats.iter().zip(&self.factions))
            .position(|(s, &f)| f == faction && s.role == Role::Stakes)
            .map(|i| i as BuildingId)
    }

    /// Whether `building` is a gate or a gatehouse.
    pub fn is_gate(&self, building: BuildingId) -> bool {
        (self.stats.get(usize::from(building)))
            .is_some_and(|s| s.flags & gc_sim::structures::GATE != 0)
    }

    /// Whether `building` is a gatehouse: a gate that upgrades from
    /// another, with a walkway on top.
    pub fn is_gatehouse(&self, building: BuildingId) -> bool {
        (self.stats.get(usize::from(building)))
            .is_some_and(|s| s.flags & gc_sim::structures::GATE != 0 && s.upgrades_from.is_some())
    }

    /// Whether `building` is a wall grown along veins with no worker.
    pub fn is_grown(&self, building: BuildingId) -> bool {
        (self.stats.get(usize::from(building)))
            .is_some_and(|s| s.flags & gc_sim::structures::GROWN != 0)
    }

    /// The gate that goes with the wall `building`: the first gate of its
    /// faction (not a gatehouse), if the faction has one (the Choir has
    /// none).
    pub fn gate_for(&self, building: BuildingId) -> Option<BuildingId> {
        let faction = self.faction(building)?;
        (0..self.stats.len() as BuildingId).find(|&i| {
            self.factions[usize::from(i)] == faction && self.is_gate(i) && !self.is_gatehouse(i)
        })
    }

    /// How high above the ground a man stands on a wall piece of type
    /// `building` (`open`: its door is open to everyone).
    pub fn stand_height(&self, building: BuildingId, open: bool) -> f32 {
        if self.is_gatehouse(building) {
            GATEHOUSE_DECK
        } else {
            wall_stand_height(self.wall_look(building, open))
        }
    }

    /// How a drawn wall of type `building` looks (`open`: its door is open
    /// to everyone).
    pub fn wall_look(&self, building: BuildingId, open: bool) -> WallLook {
        match self.stats.get(usize::from(building)) {
            Some(s) if s.role == Role::Bridge => bridge_look(self.faction(building)),
            Some(s) if matches!(s.role, Role::Stakes | Role::Barricade) => {
                fieldwork_look(s.role, self.faction(building))
            }
            Some(s) => wall_look(self.faction(building), s.flags, self.level(building), open),
            None => WallLook::PLAIN,
        }
    }

    /// A type's name.
    pub fn name(&self, id: BuildingId) -> &str {
        self.names.get(usize::from(id)).map_or("", String::as_str)
    }

    /// What a worker of `faction` lays from the build menu: every building
    /// of theirs but the main building and its upgrades.
    pub fn buildable(&self, faction: Faction) -> impl Iterator<Item = BuildingId> + Clone + '_ {
        (self.stats.iter().zip(&self.factions).enumerate())
            // Upgrades (a wall's next level) go up on what they upgrade.
            .filter(move |(_, (s, f))| {
                **f == faction && s.role != Role::Main && s.upgrades_from.is_none()
            })
            .map(|(i, _)| i as BuildingId)
    }

    /// The main building a worker of `faction` lays to expand: its first
    /// tier, not an upgrade.
    pub fn expansion(&self, faction: Faction) -> Option<BuildingId> {
        (self.stats.iter().zip(&self.factions))
            .position(|(s, &f)| f == faction && s.role == Role::Main && s.upgrades_from.is_none())
            .map(|i| i as BuildingId)
    }

    /// The panel's description of a type: name, faction and size.
    pub fn describe(&self, id: BuildingId) -> String {
        let i = usize::from(id);
        let (Some(name), Some(faction), Some(s)) =
            (self.names.get(i), self.factions.get(i), self.stats.get(i))
        else {
            return "no buildings".to_string();
        };
        format!(
            "{name} ({}, {}x{} tiles, {} hit points)",
            faction.name(),
            s.width,
            s.depth,
            s.hp
        )
    }
}

/// Looks a building model has: three construction stages (foundation,
/// frame, nearly done) and the finished building.
pub const STAGES: usize = 4;
/// Percent built at which each stage after the foundation begins.
const STAGE_FROM: [u8; STAGES - 1] = [25, 60, 100];
/// The finished building.
const FINISHED: usize = STAGES - 1;
/// The stage a tier upgrade shows in its second half: the upgrade nearly
/// done, scaffolding still up.
const UPGRADE_STAGE: usize = 2;
/// Workers shown on a main building while its tier upgrade goes up (no
/// builders are sent to one).
pub const UPGRADE_CREW: usize = 3;
/// Each faction's worker, who builds its buildings, by model name.
fn worker_of(faction: Faction) -> &'static str {
    match faction {
        Faction::EmberClans => "Hands",
        Faction::CarrionConcord => "Ledger Clerk",
        Faction::Choir => "Tuner",
        Faction::Brood => "Gnawer",
    }
}

/// The construction stage a building `built_percent` built shows (0 is the
/// foundation, [`STAGES`] - 1 the finished building).
pub fn stage_of(built_percent: u8) -> usize {
    STAGE_FROM
        .iter()
        .take_while(|&&p| built_percent >= p)
        .count()
}

/// The model name of a building's stage: its own name when finished, and
/// `#1` to `#3` after it for the stages going up.
pub fn stage_name(name: &str, stage: usize) -> String {
    if stage == FINISHED {
        name.to_string()
    } else {
        format!("{name} #{}", stage + 1)
    }
}

/// Which models dress the buildings (docs/21-buildings.md, Models).
pub struct Looks {
    /// By building type, the model of each stage, if it has one.
    stages: Vec<[Option<usize>; STAGES]>,
    /// By building type, its faction's worker model.
    crews: Vec<Option<usize>>,
    /// By model, the top of a still model (a building's look) in tiles
    /// above its floor, or `None` for a rigged soldier.
    tops: Vec<Option<f32>>,
    /// By model, how far a still model's walls reach from its middle
    /// across and along the map, in tiles: the eaves and roof left out.
    walls: Vec<Option<Vec2>>,
}

impl Looks {
    pub fn new(table: &Table, models: &Models<'_>) -> Looks {
        let scale = gc_assets::model::POSITION_SCALE;
        // A still model's top, and how far its walls (what stands in the
        // lowest part of it) reach from its middle.
        let (tops, walls) = (models.models.iter())
            .map(|m| {
                let first = m.first_vertex as usize;
                let vertices = &models.vertices[first..first + m.vertex_count as usize];
                if !m.is_still() {
                    return (None, None);
                }
                let top = vertices.iter().map(|v| v.pos[1]).max().unwrap_or(0).max(0);
                let low = (f32::from(top) * WALL_SHARE) as i16;
                let half = (vertices.iter().filter(|v| v.pos[1] <= low))
                    .fold(Vec2::ZERO, |h, v| {
                        h.max(Vec2::new(f32::from(v.pos[0]), f32::from(v.pos[2])).abs())
                    });
                (Some(f32::from(top) / scale), Some(half / scale))
            })
            .unzip();
        Looks {
            // A drawn wall is a slab along its line, never a model.
            stages: (table.names.iter().zip(&table.stats))
                .map(|(name, stats)| {
                    core::array::from_fn(|k| {
                        (stats.flags & gc_sim::structures::LINE == 0)
                            .then(|| models.by_name(&stage_name(name, k)))
                            .flatten()
                    })
                })
                .collect(),
            crews: (table.factions.iter())
                .map(|&f| models.by_name(worker_of(f)))
                .collect(),
            tops,
            walls,
        }
    }

    /// How far still model `model`'s walls reach from its middle, across
    /// and along the map, in tiles.
    pub fn walls(&self, model: usize) -> Option<Vec2> {
        self.walls.get(model).copied().flatten()
    }

    /// Whether `model` is a still model (a building's look).
    pub fn is_still(&self, model: usize) -> bool {
        self.tops.get(model).is_some_and(Option::is_some)
    }

    /// How tall the look `model` stands above its floor, in tiles: the
    /// foundation is low, the finished building tall.
    pub fn top(&self, model: usize) -> f32 {
        self.tops.get(model).copied().flatten().unwrap_or(0.0)
    }

    fn stage(&self, building: BuildingId, stage: usize) -> Option<usize> {
        self.stages.get(usize::from(building))?[stage]
    }

    /// The model `s` shows now: its construction stage while it goes up,
    /// and in the second half of a tier upgrade the upgrade nearly done.
    /// `None` draws it as a box.
    pub fn model_of(&self, s: &StructureView) -> Option<usize> {
        if !s.finished() {
            return self.stage(s.building, stage_of(s.built_percent));
        }
        match s.upgrading {
            Some((to, percent)) if percent >= 50 => self
                .stage(to, UPGRADE_STAGE)
                .or_else(|| self.stage(s.building, FINISHED)),
            _ => self.stage(s.building, FINISHED),
        }
    }

    /// The worker model of a building type's faction, if it has one.
    pub fn worker_of(&self, building: BuildingId) -> Option<usize> {
        *self.crews.get(usize::from(building))?
    }

    /// The worker model shown at work on `s`'s tier upgrade, if any.
    pub fn crew_of(&self, s: &StructureView) -> Option<usize> {
        s.upgrading?;
        self.worker_of(s.building)
    }
}

/// Where the upgrade crew of a `width` by `depth` footprint centred on
/// `centre` stand, and the way each faces: one at the front (south) edge and
/// one on each side, turned to the walls. A long thin footprint (a gate in
/// its wall) has them on its two long sides, as its ends are wall.
pub fn crew_spots(centre: Vec2, width: u8, depth: u8) -> [(Vec2, Vec2); UPGRADE_CREW] {
    let (hw, hd) = (
        f32::from(width) / 2.0 + CREW_GAP,
        f32::from(depth) / 2.0 + CREW_GAP,
    );
    if depth >= width.saturating_mul(3) || width >= depth.saturating_mul(3) {
        let along = long_axis(width, depth);
        // Across the way, to the side of the building.
        let out = Vec2::new(along.y, along.x);
        let (long, thin) = (width.max(depth), width.min(depth));
        let (half_long, half_thin) = (f32::from(long) / 2.0, f32::from(thin) / 2.0);
        return [(1.0, -0.5), (-1.0, 0.5), (1.0, 0.5)].map(|(sign, at)| {
            let spot = out * (sign * (half_thin + CREW_GAP)) + along * (at * half_long);
            (centre + spot, -out * sign)
        });
    }
    [
        (centre + Vec2::new(-hw * 0.3, hd), Vec2::NEG_Y),
        (centre + Vec2::new(hw, hd * 0.2), Vec2::NEG_X),
        (centre + Vec2::new(-hw, -hd * 0.3), Vec2::X),
    ]
}

/// How tall a building of this role is drawn, in tiles.
pub fn height_of(role: Role) -> f32 {
    match role {
        Role::Wall => WALL_HEIGHT,
        Role::Bridge => DECK_TOP + RAIL_HEIGHT,
        Role::Stakes => STAKE_HEIGHT,
        Role::Barricade => BARRICADE_HEIGHT,
        Role::Trap => TRAP_HEIGHT,
        Role::Main => MAIN_HEIGHT,
        _ => HEIGHT,
    }
}

/// `color` as dim as a building with `hp` of `max_hp` hit points looks.
fn shade([r, g, b]: [u8; 3], hp: u32, max_hp: u32) -> [u8; 3] {
    let left = (DARKEST + (100 - DARKEST) * hp.min(max_hp) / max_hp.max(1)).min(100);
    [r, g, b].map(|c| (u32::from(c) * left / 100) as u8)
}

/// The lowest and highest ground under a footprint, sampled at its corners
/// and middle.
fn ground_range(ground: &impl Fn(f32, f32) -> f32, s: &StructureView) -> (f32, f32) {
    let (x, y) = (s.tile.0 as f32, s.tile.1 as f32);
    let (w, d) = (f32::from(s.width), f32::from(s.depth));
    [(0.0, 0.0), (w, 0.0), (0.0, d), (w, d), (w / 2.0, d / 2.0)]
        .into_iter()
        .map(|(dx, dy)| ground(x + dx, y + dy))
        .fold((f32::MAX, f32::MIN), |(lo, hi), g| (lo.min(g), hi.max(g)))
}

/// The way a building's door faces when turned `turn` quarter turns: south
/// as drawn, then west, north and east (clockwise seen from above, with x
/// east and y south), as the facing a model instance takes.
pub fn facing_of(turn: u8) -> Vec2 {
    let (x, y) = gc_sim::structures::front(turn);
    Vec2::new(x as f32, y as f32)
}

/// Where a building's model stands: the middle of its footprint, the
/// height of its floor, and how far that is above the ground there. It stands on the highest ground
/// under it; the plinth every model has below its floor hides the slope.
pub fn model_spot(s: &StructureView, ground: &impl Fn(f32, f32) -> f32) -> (Vec2, f32, f32) {
    let centre = centre_of(s.tile, s.width, s.depth);
    let (_, hi) = ground_range(ground, s);
    (centre, hi, hi - ground(centre.x, centre.y))
}

/// Whether a building of `world` that `shown` allows stands over `p`
/// (always false outside Conquest, the only mode with buildings).
pub fn covers(world: &World, shown: impl Fn(&StructureView) -> bool, p: Vec2) -> bool {
    world.structures().is_some_and(|all| {
        all.iter().filter(|b| shown(b)).any(|b| {
            let (x, y) = (b.tile.0 as f32, b.tile.1 as f32);
            (x..x + f32::from(b.width)).contains(&p.x) && (y..y + f32::from(b.depth)).contains(&p.y)
        })
    })
}

/// A price as "30 Grub", "5 Marrow" or "30 Grub, 5 Marrow" ("free" for none).
pub(crate) fn price_text(grub: u32, marrow: u32) -> String {
    match (grub, marrow) {
        (0, 0) => "free".into(),
        (g, 0) => format!("{g} Grub"),
        (0, m) => format!("{m} Marrow"),
        (g, m) => format!("{g} Grub, {m} Marrow"),
    }
}

/// `one` when `n` is 1, else `many`.
pub(crate) fn plural(n: usize, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 { one } else { many }
}

/// The middle of a `width` by `depth` footprint whose north-west tile is
/// `tile`: the inverse of [`tile_for`].
pub fn centre_of(tile: (i32, i32), width: u8, depth: u8) -> Vec2 {
    Vec2::new(
        tile.0 as f32 + f32::from(width) / 2.0,
        tile.1 as f32 + f32::from(depth) / 2.0,
    )
}

/// One box over `s`'s footprint, standing on the lowest ground under it.
fn block(
    s: &StructureView,
    height: f32,
    color: [u8; 4],
    ground: &impl Fn(f32, f32) -> f32,
) -> BoxInstance {
    let slab = Slab {
        centre: centre_of(s.tile, s.width, s.depth),
        size: Vec2::new(f32::from(s.width), f32::from(s.depth)),
    };
    draped(slab, height, color, ground_range(ground, s))
}

/// A box over `slab`, `height` tall, on ground running from `lo` to `hi`
/// under it. The box drapes over the terrain, each corner on the ground
/// below it: sunk by the slope under it, so its top follows the ground at
/// `height`.
fn draped(slab: Slab, height: f32, color: [u8; 4], (lo, hi): (f32, f32)) -> BoxInstance {
    BoxInstance {
        pos: slab.centre.to_array(),
        facing: [0.0, 1.0],
        size: [slab.size.x, height + (hi - lo), slab.size.y],
        color,
        lift: lo - hi,
    }
}

/// The part of a drawn wall one of its tiles shows: a straight piece
/// along the line, `len` long, centred on `centre`, pointing along `dir`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallPiece {
    pub centre: Vec2,
    pub dir: Vec2,
    pub len: f32,
}

/// The piece of the wall drawn along `line` (`[x0, y0, x1, y1]`, tile
/// coordinates) that its tile `tile` shows. The line runs from the middle
/// of its first tile to the middle of its last; each tile has the share of
/// it for its step along the longer axis, so neighbours meet end to end
/// with no gap however shallow the line is (a tile of a shallow line sits
/// up to half a tile off the line, as the walking grid's staircase does).
/// Each piece reaches a hair past its share so they overlap, and the two
/// ends of the line are capped out to the edge of their tiles. `None` for a line of one tile, which is
/// drawn as the usual block.
pub fn wall_piece(line: [i32; 4], tile: (i32, i32)) -> Option<WallPiece> {
    let [x0, y0, x1, y1] = line;
    let (dx, dy) = (x1 - x0, y1 - y0);
    let steps = dx.abs().max(dy.abs());
    if steps == 0 {
        return None;
    }
    let k = if dx.abs() >= dy.abs() {
        (tile.0 - x0).abs()
    } else {
        (tile.1 - y0).abs()
    }
    .min(steps);
    let from = Vec2::new(x0 as f32 + 0.5, y0 as f32 + 0.5);
    let along = Vec2::new(dx as f32, dy as f32);
    let (lo, hi) = (
        ((k as f32 - 0.5) / steps as f32).max(0.0),
        ((k as f32 + 0.5) / steps as f32).min(1.0),
    );
    let total = along.length();
    let cap = total / steps as f32 / 2.0;
    let reach = |end: bool| if end { cap } else { LINE_JOINT };
    let before = reach(k == 0);
    let after = reach(k == steps);
    let dir = along / total;
    // The middle of the piece including what it reaches past each side.
    let mid = (lo + hi) / 2.0 * total + (after - before) / 2.0;
    Some(WallPiece {
        centre: from + dir * mid,
        dir,
        len: (hi - lo) * total + before + after,
    })
}

/// How a wall tile is drawn: its height and thickness in tiles, a colour of
/// its own (else its owner's) and how much it glows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallLook {
    pub height: f32,
    pub thick: f32,
    pub colour: Option<[u8; 3]>,
    pub glow: u8,
    /// The colour of the coping on top of a raised wall, if it has one.
    pub cap: Option<[u8; 3]>,
    /// Whether a battlement of merlons stands on the coping.
    pub merlons: bool,
    /// How far its underside floats above the ground (a bridge's deck).
    pub lift: f32,
}

impl WallLook {
    /// The plain wall: the owner's colour, no glow.
    pub const PLAIN: WallLook = WallLook {
        height: WALL_HEIGHT,
        thick: LINE_THICK,
        colour: None,
        glow: 0,
        cap: None,
        merlons: false,
        lift: 0.0,
    };
}

/// The look of a bridge deck of `faction`: a flat slab a tile wide.
pub fn bridge_look(faction: Option<Faction>) -> WallLook {
    let (colour, glow) = match faction {
        Some(Faction::CarrionConcord) => (BONE_DECK, 0),
        Some(Faction::Choir) => (GLASS_DECK, GLASS_DECK_GLOW),
        Some(Faction::Brood) => (CHITIN_DECK, 0),
        _ => (TIMBER_DECK, 0),
    };
    WallLook {
        height: BRIDGE_THICK,
        thick: 1.0,
        colour: Some(colour),
        glow,
        lift: DECK_LIFT,
        ..WallLook::PLAIN
    }
}

/// What a field work of `role` is made of for `faction`: its body colour,
/// the trim (stake tips, the beam of a barricade) and how much it glows.
fn fieldwork_colours(role: Role, faction: Option<Faction>) -> ([u8; 3], [u8; 3], u8) {
    let stakes = role == Role::Stakes;
    match (faction, stakes) {
        (Some(Faction::CarrionConcord), true) => ([226, 218, 194], [246, 242, 226], 0),
        (Some(Faction::CarrionConcord), false) => ([92, 76, 70], [220, 212, 186], 0),
        (Some(Faction::Choir), true) => ([140, 222, 240], [214, 246, 255], 70),
        (Some(Faction::Choir), false) => ([128, 206, 226], [208, 244, 255], 60),
        (Some(Faction::Brood), true) => ([64, 44, 52], [120, 92, 80], 0),
        (Some(Faction::Brood), false) => ([78, 52, 60], [122, 88, 70], 0),
        (_, true) => ([140, 98, 58], [190, 150, 98], 0),
        (_, false) => ([118, 80, 48], [168, 124, 76], 0),
    }
}

/// The look of a field work (stakes or a barricade) of `faction`: low, in
/// its material's colours. Used for the ghost; the finished work is drawn
/// by [`push_fieldwork_tile`].
pub fn fieldwork_look(role: Role, faction: Option<Faction>) -> WallLook {
    let (colour, _, glow) = fieldwork_colours(role, faction);
    let (height, thick) = if role == Role::Stakes {
        (STAKE_HEIGHT, 0.5)
    } else {
        (BARRICADE_HEIGHT, BARRICADE_THICK)
    };
    WallLook {
        height,
        thick,
        colour: Some(colour),
        glow,
        ..WallLook::PLAIN
    }
}

/// The look of a drawn wall of `faction` with building `flags` at `level`
/// (1 to 3); `open` is true while its door is open to everyone (a dropped
/// Light Curtain).
pub fn wall_look(faction: Option<Faction>, flags: u16, level: u8, open: bool) -> WallLook {
    use gc_sim::structures::{GROWN, PASS};
    // A dropped curtain stays a faint line however high it was raised.
    let dropped = flags & PASS != 0 && open;
    let mut look = if flags & PASS != 0 {
        WallLook {
            height: if dropped {
                CURTAIN_DROPPED_HEIGHT
            } else {
                CURTAIN_HEIGHT
            },
            thick: CURTAIN_THICK,
            colour: Some(CURTAIN_COLOR),
            glow: if dropped {
                CURTAIN_DROPPED_GLOW
            } else {
                CURTAIN_GLOW
            },
            ..WallLook::PLAIN
        }
    } else if flags & GROWN != 0 {
        WallLook {
            height: RESIN_HEIGHT,
            thick: RESIN_THICK,
            colour: Some(RESIN_COLOR),
            glow: RESIN_GLOW,
            ..WallLook::PLAIN
        }
    } else if faction == Some(Faction::CarrionConcord) {
        WallLook {
            height: BONE_HEIGHT,
            colour: Some(BONE_COLOR),
            ..WallLook::PLAIN
        }
    } else {
        WallLook::PLAIN
    };
    let raised = usize::from(level.clamp(1, 3)) - 1;
    if raised == 0 || dropped {
        return look;
    }
    let palette = match faction {
        Some(Faction::CarrionConcord) => &CONCORD_RAISED,
        Some(Faction::Choir) => &CHOIR_RAISED,
        Some(Faction::Brood) => &BROOD_RAISED,
        _ => &EMBER_RAISED,
    };
    let (body, cap) = palette[raised - 1];
    let steps = raised as f32;
    look.height += LEVEL_RISE * steps;
    look.thick += LEVEL_THICKEN * steps;
    look.colour = Some(body);
    look.glow = look.glow.saturating_add(LEVEL_GLOW * raised as u8);
    look.cap = Some(cap);
    look.merlons = raised >= 2;
    look
}

/// The colour a wall of `look` is drawn in for a side of colour `team`: a
/// raised wall's level colour keeps a share of the side's own.
fn wall_colour(look: &WallLook, team: [u8; 3]) -> [u8; 3] {
    match look.colour {
        None => team,
        Some(body) if look.cap.is_some() => tint(body, team, LEVEL_TEAM_TINT),
        Some(body) => body,
    }
}

/// One box for the wall tile `s`: a straight slab along the line it was
/// drawn in, or the usual block if it has no line or the line is one tile.
fn wall_block(
    s: &StructureView,
    height: f32,
    thick: f32,
    color: [u8; 4],
    ground: &impl Fn(f32, f32) -> f32,
) -> BoxInstance {
    let Some(piece) = s.line.and_then(|line| wall_piece(line, s.tile)) else {
        return block(s, height, color, ground);
    };
    let slab = Slab {
        centre: piece.centre,
        size: Vec2::new(thick, piece.len),
    };
    BoxInstance {
        facing: piece.dir.to_array(),
        ..draped(slab, height, color, ground_range(ground, s))
    }
}

/// One tile of a bridge: a flat deck slab along its line, a full tile wide
/// and floating just above the ground the shader draws (the water over a
/// river), with a rail along each edge and a post at the middle of each;
/// while it goes up, only two posts that grow with the work done. `deck` is
/// the deck's colour (with its glow), `team` the owner's, which tints the
/// posts and rails.
fn push_bridge_tile(s: &StructureView, deck: [u8; 4], team: [u8; 3], out: &mut Vec<BoxInstance>) {
    let (centre, dir, len) = match s.line.and_then(|line| wall_piece(line, s.tile)) {
        Some(p) => (p.centre, p.dir, p.len),
        None => (centre_of(s.tile, s.width, s.depth), Vec2::X, 1.0),
    };
    let side = Vec2::new(-dir.y, dir.x);
    let [r, g, b, _] = deck;
    let [r, g, b] = tint([r, g, b].map(|c| (u32::from(c) * 3 / 4) as u8), team, 35);
    let wood = [r, g, b, 0];
    let top = DECK_TOP;
    let edge = 0.5 - POST_THIN / 2.0;
    let post = |height: f32, lift: f32, at: Vec2| BoxInstance {
        pos: at.to_array(),
        facing: dir.to_array(),
        size: [POST_THIN, height, POST_THIN],
        color: wood,
        lift,
    };
    if !s.finished() {
        let done = f32::from(s.built_percent) / 100.0;
        let height = FRAME_LOW + (top + RAIL_HEIGHT - FRAME_LOW) * done;
        for sign in [-1.0, 1.0] {
            out.push(post(height, 0.0, centre + side * (sign * edge)));
        }
        return;
    }
    out.push(BoxInstance {
        pos: centre.to_array(),
        facing: dir.to_array(),
        size: [1.0, BRIDGE_THICK, len],
        color: deck,
        lift: DECK_LIFT,
    });
    for sign in [-1.0, 1.0] {
        let at = centre + side * (sign * edge);
        out.push(post(RAIL_HEIGHT, top, at));
        out.push(BoxInstance {
            pos: at.to_array(),
            facing: dir.to_array(),
            size: [RAIL_THIN, RAIL_THIN, len],
            color: wood,
            lift: top + RAIL_HEIGHT - RAIL_THIN,
        });
    }
}

/// What one tile of a field work is drawn in.
struct Fieldwork {
    /// Stakes, or else a barricade.
    stakes: bool,
    /// The body colour with its glow, already dimmed with the hit points.
    body: [u8; 4],
    /// The stake tips or the beam.
    trim: [u8; 3],
    /// The owner's colour, dimmed too.
    team: [u8; 3],
}

/// One tile of a hidden trap, low and subtle (its owner is the only one
/// who is shown it): a dug patch while it goes up, then by kind a woven
/// lid over a pit with a few stake tips, a disturbed grave mound with bone
/// fingers, a glinting half-buried shard, or a resin-crusted burrow mound.
/// `dim` shades a colour with the hit points.
fn push_trap_tile(
    s: &StructureView,
    trap: gc_sim::structures::Trap,
    dim: impl Fn([u8; 3]) -> [u8; 3],
    out: &mut Vec<BoxInstance>,
) {
    use gc_sim::structures::Trap;
    let centre = centre_of(s.tile, s.width, s.depth);
    let facing = Vec2::new(0.8, 0.6);
    let part = |at: Vec2, size: [f32; 3], c: [u8; 3], glow: u8, lift: f32| BoxInstance {
        pos: (centre + at).to_array(),
        facing: facing.to_array(),
        size,
        color: {
            let c = dim(c);
            [c[0], c[1], c[2], glow]
        },
        lift,
    };
    if !s.finished() {
        // Freshly dug earth.
        out.push(part(Vec2::ZERO, [0.7, 0.05, 0.7], [96, 72, 52], 0, 0.0));
        out.push(part(
            Vec2::new(0.22, 0.2),
            [0.2, 0.08, 0.16],
            [84, 62, 44],
            0,
            0.0,
        ));
        return;
    }
    match trap {
        Trap::Pit | Trap::None => {
            out.push(part(Vec2::ZERO, [0.76, 0.04, 0.76], [74, 54, 38], 0, 0.0));
            // The woven lid, laid in two crossing layers.
            out.push(part(Vec2::ZERO, [0.7, 0.03, 0.14], [156, 120, 70], 0, 0.04));
            out.push(part(
                Vec2::new(0.0, 0.18),
                [0.7, 0.03, 0.14],
                [148, 112, 64],
                0,
                0.04,
            ));
            out.push(part(
                Vec2::new(0.0, -0.18),
                [0.7, 0.03, 0.14],
                [148, 112, 64],
                0,
                0.04,
            ));
            out.push(part(Vec2::ZERO, [0.12, 0.03, 0.7], [132, 98, 56], 0, 0.07));
            for (x, z) in [(-0.2, -0.12), (0.18, 0.1), (0.02, 0.26)] {
                out.push(part(
                    Vec2::new(x, z),
                    [0.06, 0.13, 0.06],
                    [196, 160, 104],
                    0,
                    0.07,
                ));
            }
        }
        Trap::Hands => {
            out.push(part(Vec2::ZERO, [0.72, 0.11, 0.6], [74, 60, 54], 0, 0.0));
            out.push(part(
                Vec2::new(-0.05, 0.04),
                [0.46, 0.08, 0.38],
                [88, 72, 64],
                0,
                0.11,
            ));
            for (x, z, h) in [
                (-0.2, -0.18, 0.22),
                (0.0, -0.22, 0.28),
                (0.2, -0.16, 0.2),
                (0.26, 0.1, 0.16),
            ] {
                out.push(part(
                    Vec2::new(x, z),
                    [0.05, h, 0.05],
                    [226, 218, 194],
                    0,
                    0.06,
                ));
            }
        }
        Trap::Mine => {
            out.push(part(Vec2::ZERO, [0.62, 0.04, 0.62], [92, 96, 104], 0, 0.0));
            out.push(part(
                Vec2::new(-0.04, 0.0),
                [0.1, 0.3, 0.1],
                [150, 226, 244],
                90,
                0.0,
            ));
            out.push(part(
                Vec2::new(0.12, 0.08),
                [0.07, 0.2, 0.07],
                [214, 246, 255],
                120,
                0.0,
            ));
            out.push(part(
                Vec2::new(-0.18, -0.12),
                [0.07, 0.15, 0.07],
                [128, 206, 226],
                70,
                0.0,
            ));
        }
        Trap::Nest(_) => {
            out.push(part(Vec2::ZERO, [0.78, 0.12, 0.78], [84, 54, 52], 0, 0.0));
            out.push(part(Vec2::ZERO, [0.5, 0.1, 0.5], [112, 78, 62], 8, 0.12));
            // The mouth of the burrow, dark and ringed with crusted resin.
            out.push(part(Vec2::ZERO, [0.2, 0.04, 0.2], [20, 12, 16], 0, 0.22));
            out.push(part(
                Vec2::new(0.2, -0.2),
                [0.14, 0.14, 0.14],
                [168, 120, 70],
                20,
                0.1,
            ));
        }
    }
}

/// One tile of a field work, from boxes (no model).
///
/// Stakes: a row of thin posts, each with a tip stepped out across the
/// line, to one side and the other in turn, bound by a cord when finished.
/// Barricade: a chunky slab along the line with a lighter beam on top and
/// two posts standing proud, as of carts or coffins piled up. Either grows
/// with `built_percent`: the stakes shorter and fewer, the barricade lower
/// and bare.
fn push_fieldwork_tile(
    s: &StructureView,
    works: Fieldwork,
    ground: &impl Fn(f32, f32) -> f32,
    out: &mut Vec<BoxInstance>,
) {
    let (centre, dir, len) = match s.line.and_then(|line| wall_piece(line, s.tile)) {
        Some(p) => (p.centre, p.dir, p.len),
        None => (centre_of(s.tile, s.width, s.depth), Vec2::X, 1.0),
    };
    let side = Vec2::new(-dir.y, dir.x);
    let done = f32::from(s.built_percent.min(100)) / 100.0;
    let grow = 0.35 + 0.65 * done;
    let [r, g, b, glow] = works.body;
    let wood = tint([r, g, b], works.team, FIELDWORK_TEAM_TINT);
    let tip = tint(works.trim, works.team, FIELDWORK_TEAM_TINT);
    let wood = [wood[0], wood[1], wood[2], glow];
    let tip = [tip[0], tip[1], tip[2], glow];
    if works.stakes {
        let shown = if s.finished() {
            STAKES_PER_TILE
        } else {
            ((STAKES_PER_TILE as f32 * done).ceil() as usize).clamp(1, STAKES_PER_TILE)
        };
        for i in 0..shown {
            let along = (i as f32 + 0.5) / STAKES_PER_TILE as f32 - 0.5;
            let lean = if i % 2 == 0 { 1.0 } else { -1.0 };
            let foot = centre + dir * (along * len) + side * (lean * STAKE_STAGGER);
            let low = STAKE_LOW * grow;
            out.push(BoxInstance {
                pos: foot.to_array(),
                facing: dir.to_array(),
                size: [STAKE_THIN, low, STAKE_THIN],
                color: wood,
                lift: 0.0,
            });
            out.push(BoxInstance {
                pos: (foot + side * (lean * STAKE_LEAN)).to_array(),
                facing: dir.to_array(),
                size: [
                    STAKE_THIN * 0.7,
                    (STAKE_HEIGHT - STAKE_LOW) * grow,
                    STAKE_THIN * 0.7,
                ],
                color: tip,
                lift: low,
            });
        }
        if s.finished() {
            out.push(BoxInstance {
                pos: centre.to_array(),
                facing: dir.to_array(),
                size: [0.05, 0.05, len],
                color: wood,
                lift: STAKE_CORD,
            });
        }
        return;
    }
    let base_height = BARRICADE_BASE * grow;
    let base = wall_block(s, base_height, BARRICADE_THICK, wood, ground);
    out.push(base);
    if !s.finished() {
        return;
    }
    let top = base.lift + base.size[1];
    out.push(BoxInstance {
        size: [
            BARRICADE_THICK * 0.7,
            BARRICADE_HEIGHT - BARRICADE_BASE,
            base.size[2],
        ],
        color: tip,
        lift: top,
        ..base
    });
    for sign in [-1.0, 1.0] {
        out.push(BoxInstance {
            pos: (centre + dir * (sign * len * 0.3)).to_array(),
            facing: dir.to_array(),
            size: [0.14, BARRICADE_HEIGHT + 0.08 - BARRICADE_BASE, 0.14],
            color: wood,
            lift: top,
        });
    }
}

/// The top of a raised wall tile drawn as `body`: a coping slab along it
/// in `cap`, and on a third level a pair of merlons standing on that.
fn push_wall_top(body: &BoxInstance, look: WallLook, out: &mut Vec<BoxInstance>) {
    let Some(cap) = look.cap else {
        return;
    };
    let [x, height, z] = body.size;
    let top = body.lift + height;
    let color = [cap[0], cap[1], cap[2], look.glow];
    out.push(BoxInstance {
        size: [x + 0.08, COPING_HEIGHT, z],
        color,
        lift: top,
        ..*body
    });
    if !look.merlons {
        return;
    }
    let along = Vec2::from_array(body.facing);
    let at = Vec2::from_array(body.pos);
    for side in [-1.0, 1.0] {
        out.push(BoxInstance {
            pos: (at + along * (side * z * 0.25)).to_array(),
            size: [x + 0.08, MERLON_HEIGHT, z * MERLON_SHARE],
            color,
            lift: top + COPING_HEIGHT,
            ..*body
        });
    }
}

/// A flat box seen from above: its middle and its extent along x and y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slab {
    pub centre: Vec2,
    pub size: Vec2,
}

/// The pieces a gate is drawn from: a jamb post at each end of its wall
/// line and two door leaves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GateParts {
    pub posts: [Slab; 2],
    pub leaves: [Slab; 2],
}

/// The way a `width` by `depth` footprint runs: along y if it is at least
/// as deep as wide, else along x.
pub fn long_axis(width: u8, depth: u8) -> Vec2 {
    if depth >= width { Vec2::Y } else { Vec2::X }
}

/// Lay out a gate whose turned footprint is `width` by `depth` tiles and
/// centred on `centre`. Its wall line runs along the longer side and troops
/// pass across the shorter. Shut, the leaves meet in the middle across the
/// doorway between the posts; open, each is swung back to lie along the
/// passage against the inner side of its post.
pub fn gate_parts(centre: Vec2, width: u8, depth: u8, open: bool) -> GateParts {
    let (w, d) = (f32::from(width), f32::from(depth));
    // Work with the wall line along "long" and the passage along "thin",
    // then turn the pieces back onto x and y.
    let along_y = depth >= width;
    let (long, thin) = if along_y { (d, w) } else { (w, d) };
    let post = POST_LEN.min(long / 2.0);
    let door = long - 2.0 * post;
    let slab = |long_at: f32, thin_at: f32, long_len: f32, thin_len: f32| {
        let (at, size) = if along_y {
            (Vec2::new(thin_at, long_at), Vec2::new(thin_len, long_len))
        } else {
            (Vec2::new(long_at, thin_at), Vec2::new(long_len, thin_len))
        };
        Slab {
            centre: centre + at,
            size,
        }
    };
    let end = (long - post) / 2.0;
    let posts = [-end, end].map(|at| slab(at, 0.0, post, thin));
    let leaves = [-1.0, 1.0].map(|side| {
        if open {
            // Lying along the passage, tight against the post.
            let len = (door / 2.0).min(thin);
            slab(
                side * (door / 2.0 - LEAF_THICK / 2.0),
                (len - thin) / 2.0,
                LEAF_THICK,
                len,
            )
        } else {
            slab(side * door / 4.0, 0.0, door / 2.0, LEAF_THICK)
        }
    });
    GateParts { posts, leaves }
}

/// A gate as boxes: jamb posts in its owner's colour (unless a model
/// stands for them) and door leaves, open or shut as it is. A gatehouse
/// (`deck`) has its posts up to the deck and a slab across them.
fn push_gate(
    s: &StructureView,
    posts: Option<[u8; 3]>,
    deck: bool,
    ground: &impl Fn(f32, f32) -> f32,
    out: &mut Vec<BoxInstance>,
) {
    let range = ground_range(ground, s);
    let centre = centre_of(s.tile, s.width, s.depth);
    let parts = gate_parts(centre, s.width, s.depth, s.open);
    let slab = |slab: Slab, height: f32, color: [u8; 4]| draped(slab, height, color, range);
    if let Some([r, g, b]) = posts {
        let top = if deck {
            GATEHOUSE_DECK
        } else {
            WALL_HEIGHT + POST_RISE
        };
        for post in parts.posts {
            out.push(slab(post, top, [r, g, b, 0]));
        }
        if deck {
            let roof = block(s, GATEHOUSE_DECK, [r, g, b, 0], ground);
            out.push(BoxInstance {
                lift: roof.lift + roof.size[1] - DECK_THICK,
                size: [roof.size[0], DECK_THICK, roof.size[2]],
                ..roof
            });
        }
    }
    for leaf in parts.leaves {
        out.push(slab(leaf, LEAF_HEIGHT, LEAF_COLOR));
    }
}

/// Whether `side` is shown a building: in fog of war, enemy buildings show
/// once seen; with `reveal_all`, everything shows. A hidden trap shows to
/// its owner alone, fog of war or not.
pub fn shown_to(
    table: &Table,
    side: PlayerId,
    reveal_all: bool,
) -> impl Fn(&StructureView) -> bool + Copy + '_ {
    move |s| (reveal_all && !table.is_trap(s.building)) || s.seen_by(side)
}

/// Draw every standing building that `shown` allows: a box in its owner's
/// colour, dimmer as its hit points fall, and a bar over it once it is
/// damaged, white for a moment as a blow lands (`flash`, 0 to 1).
/// `across` is the way the camera looks along the ground, so the bars face
/// it.
#[allow(clippy::too_many_arguments)]
pub fn push(
    world: &World,
    table: &Table,
    ground: impl Fn(f32, f32) -> f32,
    team: impl Fn(PlayerId) -> [u8; 3],
    shown: impl Fn(&StructureView) -> bool,
    modelled: impl Fn(&StructureView) -> Option<f32>,
    flash: impl Fn(StructureHandle) -> f32,
    across: Vec2,
    out: &mut Vec<BoxInstance>,
) {
    let Some(structures) = world.structures() else {
        return;
    };
    let right = Vec2::new(-across.y, across.x);
    for s in structures.iter().filter(|s| shown(s)) {
        if out.len() + 12 > MAX_BOX_INSTANCES {
            return;
        }
        let Some(stats) = table.stats.get(usize::from(s.building)) else {
            continue;
        };
        // A building with a model is drawn with the soldiers (its bar
        // floats over the model's roof).
        // A gate standing is drawn as posts and door leaves, not a block.
        let model = modelled(&s);
        let look = if s.line.is_some() {
            table.wall_look(s.building, s.open)
        } else {
            WallLook::PLAIN
        };
        let height = match (model, s.line) {
            (Some(h), _) => h,
            (None, Some(_)) => look.height,
            (None, None) => height_of(stats.role),
        };
        let [r, g, b] = shade(wall_colour(&look, team(s.owner)), s.hp, s.max_hp);
        if stats.role == Role::Bridge {
            let team = shade(team(s.owner), s.hp, s.max_hp);
            push_bridge_tile(&s, [r, g, b, look.glow], team, out);
        } else if matches!(stats.role, Role::Stakes | Role::Barricade) {
            let (_, trim, _) = fieldwork_colours(stats.role, table.faction(s.building));
            let team = shade(team(s.owner), s.hp, s.max_hp);
            let works = Fieldwork {
                stakes: stats.role == Role::Stakes,
                body: [r, g, b, look.glow],
                trim: shade(trim, s.hp, s.max_hp),
                team,
            };
            push_fieldwork_tile(&s, works, &ground, out);
        } else if stats.role == Role::Trap {
            let dim = |c: [u8; 3]| shade(c, s.hp, s.max_hp);
            push_trap_tile(&s, stats.trap, dim, out);
        } else {
            match (s.gate.is_some() && s.finished(), model) {
                (true, _) => push_gate(
                    &s,
                    model.is_none().then_some([r, g, b]),
                    table.is_gatehouse(s.building),
                    &ground,
                    out,
                ),
                (false, None) => {
                    let body = wall_block(&s, height, look.thick, [r, g, b, look.glow], &ground);
                    out.push(body);
                    if s.line.is_some() {
                        push_wall_top(&body, look, out);
                    }
                }
                (false, Some(_)) => {}
            }
        }
        if s.hp >= s.max_hp {
            continue;
        }
        let (lo, hi) = ground_range(&ground, &s);
        // A wall's tiles are side by side, so each bar is only a tile long.
        let len = if s.line.is_some() {
            1.0
        } else {
            f32::from(s.width.max(s.depth)).max(BAR_MIN)
        };
        let frac = s.hp as f32 / s.max_hp.max(1) as f32;
        let fill = len * frac;
        let lift = height + (hi - lo) + BAR_LIFT;
        let centre = centre_of(s.tile, s.width, s.depth);
        out.push(BoxInstance {
            pos: centre.to_array(),
            facing: across.to_array(),
            size: [len + BAR_FRAME * 2.0, BAR_THICK, BAR_DEPTH],
            color: BAR_BACK,
            lift,
        });
        out.push(BoxInstance {
            pos: (centre - right * ((len - fill) / 2.0)).to_array(),
            facing: across.to_array(),
            size: [fill, BAR_THICK + BAR_FRAME, BAR_DEPTH + BAR_FRAME],
            color: flashed(bar_colour(frac), flash(s.handle)),
            lift: lift - BAR_FRAME / 2.0,
        });
    }
}

/// Draw every gathering spot (Conquest): a glowing mound, smaller as it
/// runs down and dim once only dregs are left.
pub fn push_nodes(world: &World, out: &mut Vec<BoxInstance>) {
    let Some(economy) = world.economy() else {
        return;
    };
    for n in economy.nodes() {
        if out.len() >= MAX_BOX_INSTANCES {
            return;
        }
        let [r, g, b] = node_color(n.resource == Resource::Marrow, n.left == 0);
        let width = NODE_WIDTH * node_fill(&n);
        let glow = if n.left == 0 { 0 } else { NODE_GLOW };
        // A Grub pile (a strongbox spilled) is a low heap, not a mound.
        let height = if n.temporary {
            NODE_HEIGHT * PILE_HEIGHT
        } else {
            NODE_HEIGHT
        };
        out.push(BoxInstance {
            pos: crate::from_fx_vec(n.at).to_array(),
            facing: [0.0, 1.0],
            size: [width, height, width],
            color: [r, g, b, glow],
            lift: 0.0,
        });
    }
}

/// A gathering spot's size as a share of full: 1 while full, down to
/// `NODE_LEAST` percent as it runs down.
fn node_fill(n: &gc_sim::NodeView) -> f32 {
    let left = n.left as f32 / n.full.max(1) as f32;
    (NODE_LEAST + (100.0 - NODE_LEAST) * left) / 100.0
}

/// How a gathering spot looks as a scenery model ([`crate::scenery`]):
/// shrunk from `NODE_MODEL_SCALE` to `NODE_LEAST` percent of it as it runs down, a touch
/// bright while it holds (so it stands out on any land), dim when dry.
pub fn node_look(n: &gc_sim::NodeView, ground: impl Fn(Vec2) -> f32) -> Extra {
    let at = crate::from_fx_vec(n.at);
    let shade = if n.left == 0 {
        NODE_DRY_SHADE
    } else {
        NODE_SHADE
    };
    Extra {
        model: if n.resource == Resource::Marrow {
            Model::MarrowVein
        } else {
            Model::GrubSeep
        },
        at: Vec3::new(at.x, ground(at), at.y),
        // Turned by where it lies, so no two look the same way.
        turn: (at.x * 37.0 + at.y * 91.0) as i64 as u8,
        scale: NODE_MODEL_SCALE * node_fill(n),
        tint: [shade; 3],
        // Wind phase from the turn, so neighbours sway apart.
        phase: (at.x * 37.0 + at.y * 91.0) as i64 as u8,
        lean: 0,
        fall: 0,
    }
}

/// Ribbons (the flow along root-veins, beams, auras) lie flat this thick
/// (tiles), in segments this long so each follows the ground it crosses, a
/// hair above it.
const RIBBON_THICK: f32 = 0.06;
const RIBBON_SEGMENT: f32 = 3.0;
const RIBBON_LIFT: f32 = 0.05;
/// The flow home from a node, as a width in tiles.
const VEIN_FLOW_WIDTH: f32 = 0.5;
/// The flow of Grub is this violet, a quarter of the way to its owner's
/// colour.
const VEIN_FLOW_COLOR: [u8; 3] = [200, 80, 170];
/// Flow pulses along its ribbon, a pulse every this many ticks per
/// segment (and so every few seconds along the whole ribbon).
const FLOW_PULSE_TICKS: u32 = 12;
/// A node the veins do not reach: a dull stub towards the network and a
/// red cross on the node.
const CUT_COLOR: [u8; 3] = [92, 62, 62];
const CUT_MARK_COLOR: [u8; 3] = [210, 50, 40];
const CUT_GLOW: u8 = 60;
const CUT_STUB: f32 = 3.0;

/// Colour `c` with `share` percent of `towards` mixed in.
fn tint(c: [u8; 3], towards: [u8; 3], share: u32) -> [u8; 3] {
    let mix = |a: u8, b: u8| ((u32::from(a) * (100 - share) + u32::from(b) * share) / 100) as u8;
    [
        mix(c[0], towards[0]),
        mix(c[1], towards[1]),
        mix(c[2], towards[2]),
    ]
}

/// A flat ribbon from `from` to `to` on the ground, in segments that
/// taper from width `widths.0` to `widths.1`, each as bright as `glow`
/// says of its place along the ribbon.
fn push_ribbon(
    from: Vec2,
    to: Vec2,
    widths: (f32, f32),
    colour: [u8; 3],
    lift: f32,
    glow: impl Fn(usize) -> u8,
    out: &mut Vec<BoxInstance>,
) {
    let d = to - from;
    let len = d.length();
    if len < 0.05 {
        return;
    }
    let dir = d / len;
    let n = ((len / RIBBON_SEGMENT).ceil() as usize).max(1);
    for i in 0..n {
        if out.len() >= MAX_BOX_INSTANCES {
            return;
        }
        let mid = (i as f32 + 0.5) / n as f32;
        let [r, g, b] = colour;
        out.push(BoxInstance {
            pos: (from + d * mid).to_array(),
            facing: dir.to_array(),
            // A little longer than its share, so segments meet.
            size: [
                widths.0 + (widths.1 - widths.0) * mid,
                RIBBON_THICK,
                len / n as f32 + 0.1,
            ],
            color: [r, g, b, glow(i)],
            lift,
        });
    }
}

/// Draw what moves along the Brood's root-veins (Conquest); the veins
/// themselves are the ground's (`land.rs`). Where `side`'s burrowed workers
/// work a node on the veins, a bright ribbon pulses from the node to the
/// building that carries the load home; a node off the veins gets a dull
/// stub and a red cross, so the break shows. `reveal_all` shows every
/// side's workers too.
pub fn push_veins(
    world: &World,
    side: PlayerId,
    reveal_all: bool,
    team: impl Fn(PlayerId) -> [u8; 3],
    out: &mut Vec<BoxInstance>,
) {
    let Some(economy) = world.economy() else {
        return;
    };
    // The nodes burrowed workers are at, each once, with who works it:
    // `side`'s own only, unless everything is shown (an enemy's workers
    // can be in the fog).
    let mut worked: Vec<(PlayerId, u16)> = Vec::new();
    for r in world.regiments.iter() {
        if (r.owner == side || reveal_all)
            && let Some(node) = world.burrowed_at(r.handle)
            && !worked.contains(&(r.owner, node))
        {
            worked.push((r.owner, node));
        }
    }
    let tick = world.tick_count();
    let nodes = economy.node_positions();
    for &(owner, node) in &worked {
        if out.len() >= MAX_BOX_INSTANCES {
            return;
        }
        let Some(&node_at) = nodes.get(usize::from(node)) else {
            continue;
        };
        let at = crate::from_fx_vec(node_at);
        if let Some(v) = world.vein_home(owner, node_at) {
            // The Grub flows: pulses run from the node to the building
            // that carries it home.
            push_ribbon(
                at,
                crate::from_fx_vec(v.at),
                (VEIN_FLOW_WIDTH, VEIN_FLOW_WIDTH),
                tint(VEIN_FLOW_COLOR, team(owner), 25),
                RIBBON_LIFT + 0.01,
                |i| {
                    let phase = (tick / FLOW_PULSE_TICKS) as usize % 4;
                    if (i + 4 - phase).is_multiple_of(4) {
                        150
                    } else {
                        80
                    }
                },
                out,
            );
            continue;
        }
        // Cut off: a dull stub towards the nearest veins and a red cross on
        // the node.
        let nearest = world
            .veins()
            .filter(|v| v.owner == owner)
            .min_by_key(|v| (v.at - node_at).length_sq_raw());
        if let Some(v) = nearest {
            let d = crate::from_fx_vec(v.at) - at;
            let stub = d.normalize_or_zero() * CUT_STUB.min(d.length() / 2.0);
            push_ribbon(
                at,
                at + stub,
                (VEIN_FLOW_WIDTH, VEIN_FLOW_WIDTH * 0.5),
                CUT_COLOR,
                RIBBON_LIFT + 0.01,
                |_| 0,
                out,
            );
        }
        for facing in [Vec2::new(1.0, 1.0), Vec2::new(1.0, -1.0)] {
            if out.len() >= MAX_BOX_INSTANCES {
                return;
            }
            let [r, g, b] = CUT_MARK_COLOR;
            out.push(BoxInstance {
                pos: at.to_array(),
                facing: facing.normalize().to_array(),
                size: [0.25, 0.1, 1.5],
                color: [r, g, b, CUT_GLOW],
                lift: CUT_MARK_LIFT,
            });
        }
    }
}

/// A tuning beam: a thin, glowing crystal-blue ribbon from a tuning
/// building to each node it works, in tiles.
const BEAM_WIDTH: f32 = 0.16;
const BEAM_COLOR: [u8; 3] = [140, 210, 255];
const BEAM_GLOW: u8 = 150;
const BEAM_LIFT: f32 = RIBBON_LIFT + 0.02;
/// A field of power's edge is a faint ring on the ground, brighter and
/// wider while a building that needs power is being placed.
const RING_COLOR: [u8; 3] = [70, 115, 160];
const RING_GLOW: u8 = 20;
const RING_WIDTH: f32 = 0.1;
const RING_LIT_COLOR: [u8; 3] = [150, 215, 255];
const RING_LIT_GLOW: u8 = 130;
const RING_LIT_WIDTH: f32 = 0.22;
const RING_LIFT: f32 = 0.04;
/// Ring pieces round a corner are about this long (tiles).
const RING_STEP: f32 = 1.2;

/// Draw the Choir's tuning (Conquest): a beam from each of `side`'s tuning
/// buildings to every node it tunes. `reveal_all` shows every side's. (An
/// enemy's are never drawn from what was once seen: which nodes it tunes
/// changes in the fog.)
pub fn push_tuning(world: &World, side: PlayerId, reveal_all: bool, out: &mut Vec<BoxInstance>) {
    let (Some(structures), Some(economy)) = (world.structures(), world.economy()) else {
        return;
    };
    for n in economy.nodes() {
        let Some(s) =
            (n.tuner.and_then(|h| structures.get(h))).filter(|s| reveal_all || s.owner == side)
        else {
            continue;
        };
        if out.len() >= MAX_BOX_INSTANCES {
            return;
        }
        push_ribbon(
            centre_of(s.tile, s.width, s.depth),
            crate::from_fx_vec(n.at),
            (BEAM_WIDTH, BEAM_WIDTH),
            BEAM_COLOR,
            BEAM_LIFT,
            |_| BEAM_GLOW,
            out,
        );
    }
}

/// A chord's edge: a glowing Choir cyan-violet ribbon, a little wider and
/// brighter than a tuning beam, brighter still for a stronger chord. A link
/// that is no chord's edge is a thin, faint line.
const CHORD_WIDTH: f32 = 0.34;
const CHORD_COLOR: [u8; 3] = [150, 150, 255];
const CHORD_GLOW: u8 = 170;
const CHORD_LIFT: f32 = BEAM_LIFT + 0.02;
const LINK_WIDTH: f32 = 0.08;
const LINK_COLOR: [u8; 3] = [120, 170, 235];
const LINK_GLOW: u8 = 55;
const LINK_LIFT: f32 = BEAM_LIFT - 0.01;

/// Draw the Choir's chords and links (Conquest): each link of `side`'s
/// between two of its buildings as a ribbon, a glowing one if it is a
/// chord's edge (brighter for a stronger chord), else a thin faint line.
/// `reveal_all` shows every side's. (An enemy's are never drawn from what
/// was once seen: they change in the fog.)
pub fn push_chords(world: &World, side: PlayerId, reveal_all: bool, out: &mut Vec<BoxInstance>) {
    for l in world.links() {
        if l.owner != side && !reveal_all {
            continue;
        }
        if out.len() >= MAX_BOX_INSTANCES {
            return;
        }
        let (width, colour, lift, glow) = if l.harmony > 0 {
            let glow = CHORD_GLOW.saturating_add(l.harmony);
            (CHORD_WIDTH, CHORD_COLOR, CHORD_LIFT, glow)
        } else {
            (LINK_WIDTH, LINK_COLOR, LINK_LIFT, LINK_GLOW)
        };
        let [from, to] = l.at.map(crate::from_fx_vec);
        push_ribbon(from, to, (width, width), colour, lift, |_| glow, out);
    }
}

/// Draw the edge of each field of power (Conquest) as a ring on the ground
/// round its building's footprint: `side`'s own, or every side's with
/// `reveal_all`. Faint, unless `lit` (a building that needs power is being
/// placed), when the rings stand out.
pub fn push_fields(
    world: &World,
    side: PlayerId,
    reveal_all: bool,
    lit: bool,
    out: &mut Vec<BoxInstance>,
) {
    let (colour, glow, width) = if lit {
        (RING_LIT_COLOR, RING_LIT_GLOW, RING_LIT_WIDTH)
    } else {
        (RING_COLOR, RING_GLOW, RING_WIDTH)
    };
    for f in world.power_fields() {
        if f.owner != side && !reveal_all {
            continue;
        }
        push_ring(
            f.foot.map(|v| v as f32),
            f.reach as f32,
            (width, colour, glow),
            RING_LIFT,
            out,
        );
    }
}

/// The glow on whatever a power works on: a ribbon, as wide as a chord's
/// edge, round a building's footprint or a regiment's block (cyan for a
/// haste). How far outside it stands, in tiles.
const HASTE_COLOR: [u8; 3] = [90, 230, 255];
const AURA_WIDTH: f32 = 0.2;
const AURA_REACH: f32 = 0.6;
const AURA_LIFT: f32 = RING_LIFT + 0.02;

/// Ring each building that `shown` allows and a power has hasted (Conquest)
/// with a cyan ribbon `glow` bright.
pub fn push_haste(
    world: &World,
    shown: impl Fn(&StructureView) -> bool,
    glow: u8,
    out: &mut Vec<BoxInstance>,
) {
    let Some(structures) = world.structures() else {
        return;
    };
    let hasted = |s: &StructureView| world.structure_haste(s.handle) > 0 && shown(s);
    for s in structures.iter().filter(hasted) {
        let foot = [s.tile.0, s.tile.1, i32::from(s.width), i32::from(s.depth)];
        let look = (AURA_WIDTH, HASTE_COLOR, glow);
        push_ring(foot.map(|v| v as f32), AURA_REACH, look, AURA_LIFT, out);
    }
}

/// What a power has done to a regiment, drawn as a ribbon round its block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aura {
    /// Hasted: cyan.
    Haste,
    /// Forced march: amber, a little further out.
    March,
    /// Steadfast: pale gold, further out still, so they show at once.
    Steadfast,
    /// A summoned regiment: spectral violet, furthest out.
    Spectral,
    /// Slowed by a Singing Wall: pale violet, outermost, faint.
    Sung,
}

impl Aura {
    /// Its ribbon's colour and how far outside the block it stands.
    fn look(self) -> ([u8; 3], f32) {
        match self {
            Aura::Haste => (HASTE_COLOR, AURA_REACH),
            Aura::March => (MARCH_COLOR, AURA_REACH + AURA_STEP),
            Aura::Steadfast => (STEADFAST_COLOR, AURA_REACH + 2.0 * AURA_STEP),
            Aura::Spectral => (SPECTRAL_COLOR, AURA_REACH + 3.0 * AURA_STEP),
            Aura::Sung => (SUNG_COLOR, AURA_REACH + 4.0 * AURA_STEP),
        }
    }

    /// Its ribbon's brightness for a `base` glow: a summoned regiment's
    /// aura is the faintest, since it shows the whole time.
    fn glow(self, base: u8) -> u8 {
        match self {
            Aura::Spectral | Aura::Sung => base / 2,
            _ => base,
        }
    }
}

/// The colours of the other regiment auras, and how much further out each
/// stands than the one before.
const MARCH_COLOR: [u8; 3] = [255, 165, 50];
const STEADFAST_COLOR: [u8; 3] = [255, 225, 130];
const SPECTRAL_COLOR: [u8; 3] = [170, 140, 255];
const SUNG_COLOR: [u8; 3] = [215, 200, 255];
const AURA_STEP: f32 = 0.35;

/// Outline a regiment's block (`width` across its `facing`, `depth` along
/// it, round `centre`) with the ribbon of `aura`, `glow` bright.
pub fn push_regiment_aura(
    aura: Aura,
    centre: Vec2,
    facing: Vec2,
    (width, depth): (f32, f32),
    glow: u8,
    out: &mut Vec<BoxInstance>,
) {
    let (colour, reach) = aura.look();
    let glow = aura.glow(glow);
    let half = (width / 2.0 + reach, depth / 2.0 + reach);
    let corners = block_corners(centre, facing, half);
    for (i, &from) in corners.iter().enumerate() {
        let to = corners[(i + 1) % corners.len()];
        let width = (AURA_WIDTH, AURA_WIDTH);
        push_ribbon(from, to, width, colour, AURA_LIFT, |_| glow, out);
    }
}

/// The ring a Banner of the Ancestors would cover: `tiles` round `at`, in
/// the steadfast colour, `glow` bright.
pub fn push_aim_ring(at: Vec2, tiles: f32, glow: u8, out: &mut Vec<BoxInstance>) {
    let look = (RING_LIT_WIDTH, STEADFAST_COLOR, glow);
    push_ring([at.x, at.y, 0.0, 0.0], tiles, look, AURA_LIFT, out);
}

/// The corners of a regiment's block round `centre`, `half` its width
/// across its `facing` and its depth along it: in order round the block,
/// front left first.
pub fn block_corners(centre: Vec2, facing: Vec2, (hw, hd): (f32, f32)) -> [Vec2; 4] {
    let right = Vec2::new(-facing.y, facing.x);
    [
        centre + facing * hd - right * hw,
        centre + facing * hd + right * hw,
        centre - facing * hd + right * hw,
        centre - facing * hd - right * hw,
    ]
}

/// A closed ring on the ground `reach` tiles outside the rectangle `foot`
/// (`[x, y, width, depth]`): straight along its sides, round at its
/// corners. `look` is its width, colour and glow.
fn push_ring(
    foot: [f32; 4],
    reach: f32,
    (width, colour, glow): (f32, [u8; 3], u8),
    lift: f32,
    out: &mut Vec<BoxInstance>,
) {
    let [x, y, w, d] = foot;
    let steps = ((reach * std::f32::consts::FRAC_PI_2 / RING_STEP).ceil() as usize).max(2);
    // Clockwise from the north-east corner, a quarter turn at each.
    let corners = [
        (Vec2::new(x + w, y), -90.0f32),
        (Vec2::new(x + w, y + d), 0.0),
        (Vec2::new(x, y + d), 90.0),
        (Vec2::new(x, y), 180.0),
    ];
    let point = |c: Vec2, from: f32, k: usize| {
        let a = (from + 90.0 * k as f32 / steps as f32).to_radians();
        c + Vec2::new(a.cos(), a.sin()) * reach
    };
    let mut piece = |from, to| push_ribbon(from, to, (width, width), colour, lift, |_| glow, out);
    let start = point(corners[0].0, corners[0].1, 0);
    let mut prev = start;
    for (c, from) in corners {
        for k in 0..=steps {
            let to = point(c, from, k);
            piece(prev, to);
            prev = to;
        }
    }
    piece(prev, start);
}

/// The Titan's Heart's colour: its holder's while held, else red.
pub fn heart_color(heart: &HeartView, parties: &[Party]) -> [u8; 3] {
    (heart.held).map_or(HEART_COLOR, |(p, _)| crate::color_of(parties, p))
}

/// The Titan's Heart: a stone, dim until it can be held, glowing after.
pub fn push_heart(world: &World, parties: &[Party], out: &mut Vec<BoxInstance>) {
    let Some(heart) = world.heart() else {
        return;
    };
    if out.len() + 2 > MAX_BOX_INSTANCES {
        return;
    }
    let [r, g, b] = heart_color(&heart, parties);
    let pos = crate::from_fx_vec(heart.at).to_array();
    out.push(BoxInstance {
        pos,
        facing: [0.0, 1.0],
        size: [HEART_BASE, 0.3, HEART_BASE],
        color: [60, 40, 40, 0],
        lift: 0.0,
    });
    out.push(BoxInstance {
        pos,
        facing: [0.0, 1.0],
        size: [HEART_WIDTH, HEART_HEIGHT, HEART_WIDTH],
        color: [r, g, b, if heart.open { HEART_GLOW } else { 0 }],
        lift: 0.0,
    });
}

/// A flag where building `h` sends what it trains, if it has a rally point.
pub fn push_rally(world: &World, h: StructureHandle, colour: [u8; 3], out: &mut Vec<BoxInstance>) {
    let Some(at) = world.production().and_then(|p| p.rally_of(h)) else {
        return;
    };
    if out.len() + 2 > MAX_BOX_INSTANCES {
        return;
    }
    let at = crate::from_fx_vec(at);
    let [r, g, b] = colour;
    out.push(BoxInstance {
        pos: at.to_array(),
        facing: [0.0, 1.0],
        size: POLE,
        color: POLE_COLOR,
        lift: 0.0,
    });
    out.push(BoxInstance {
        pos: (at + Vec2::new(FLAG[0] / 2.0, 0.0)).to_array(),
        facing: [0.0, 1.0],
        size: FLAG,
        color: [r, g, b, 40],
        lift: POLE[1] - FLAG[1],
    });
}

/// A white rim round building `h`'s footprint, to show it is selected.
pub fn push_rim(
    world: &World,
    h: StructureHandle,
    ground: impl Fn(f32, f32) -> f32,
    out: &mut Vec<BoxInstance>,
) {
    let Some(s) = world.structures().and_then(|s| s.get(h)) else {
        return;
    };
    if out.len() >= MAX_BOX_INSTANCES {
        return;
    }
    // Laid over the wider footprint, so it sits on the ground it covers.
    let rim = StructureView {
        tile: (s.tile.0 - 1, s.tile.1 - 1),
        width: s.width.saturating_add(2),
        depth: s.depth.saturating_add(2),
        ..s
    };
    out.push(block(&rim, PLATE, RIM_COLOR, &ground));
}

fn bar_colour(frac: f32) -> [u8; 4] {
    if frac > 0.5 {
        [90, 200, 70, 40]
    } else if frac > 0.25 {
        [230, 180, 50, 40]
    } else {
        [225, 60, 40, 40]
    }
}

/// `colour` lit toward white by `flash` (0 to 1), glowing as it goes.
fn flashed([r, g, b, a]: [u8; 4], flash: f32) -> [u8; 4] {
    let up = |c: u8| (f32::from(c) + (255.0 - f32::from(c)) * flash.clamp(0.0, 1.0)) as u8;
    [up(r), up(g), up(b), up(a)]
}

/// How a building about to be placed sits on the ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Open ground: it goes up as it is.
    Open,
    /// Overgrown: the worker clears the footprint first.
    Overgrown,
    /// It won't go here.
    Refused,
    /// A wall tile being planned as a blueprint (pale blue).
    Blueprint,
    /// A tile already planned, waiting for a worker (faint pale blue).
    Planned,
    /// A gate the ring tool suggests (pale gold).
    Suggested,
}

/// A footprint of `width` × `depth` tiles from `tile`, turned `turn`
/// quarter turns, as a view, for the flat shapes drawn over the ground.
fn footprint_view(tile: (i32, i32), width: u8, depth: u8, turn: u8) -> StructureView {
    StructureView {
        handle: StructureHandle {
            index: 0,
            generation: 0,
        },
        owner: 0,
        building: 0,
        tile,
        width,
        depth,
        turn,
        hp: 1,
        max_hp: 1,
        built_percent: 100,
        upgrading: None,
        growing: false,
        seen: u8::MAX,
        gate: None,
        open: false,
        line: None,
    }
}

/// The colour of a ghost: the side's own where it fits, amber where the
/// ground must be cleared first, red where it won't go.
fn ghost_colour(fit: Fit, [r, g, b]: [u8; 3]) -> [u8; 4] {
    match fit {
        Fit::Open => [r, g, b, GHOST_GLOW],
        Fit::Overgrown => OVERGROWN,
        Fit::Refused => REFUSED,
        Fit::Blueprint => PLAN_GHOST,
        Fit::Planned => PLANNED,
        Fit::Suggested => SUGGESTED,
    }
}

/// The ghost of a building about to be placed with its north-west tile at
/// `tile`, turned `turn` quarter turns: its full size in the side's colour,
/// amber where the ground must be cleared first, or red where it won't go,
/// with a step out of its front.
pub fn push_ghost(
    stats: &BuildingStats,
    tile: (i32, i32),
    turn: u8,
    fit: Fit,
    colour: [u8; 3],
    ground: impl Fn(f32, f32) -> f32,
    out: &mut Vec<BoxInstance>,
) {
    let (width, depth) = stats.size(turn);
    let view = footprint_view(tile, width, depth, turn);
    let colour = ghost_colour(fit, colour);
    out.push(block(&view, height_of(stats.role), colour, &ground));
    // A low step out of its front, so the player sees which way it faces.
    let facing = facing_of(turn);
    let reach = if facing.x == 0.0 { depth } else { width };
    let centre = centre_of(tile, width, depth) + facing * (f32::from(reach) / 2.0 + 0.5);
    out.push(BoxInstance {
        pos: centre.to_array(),
        facing: facing.to_array(),
        size: [1.0, 0.3, 1.0],
        color: colour,
        lift: 0.0,
    });
}

/// The ghost of a wall about to be drawn along `line`: each of `tiles` in
/// the colour its `Fit` gives, as a slab along the line like the finished
/// wall (a line of one tile is a plain block).
pub fn push_wall_ghost(
    line: [i32; 4],
    look: WallLook,
    tiles: impl Iterator<Item = ((i32, i32), Fit)>,
    colour: [u8; 3],
    ground: impl Fn(f32, f32) -> f32,
    out: &mut Vec<BoxInstance>,
) {
    for (tile, fit) in tiles {
        if out.len() >= MAX_BOX_INSTANCES {
            return;
        }
        let colour = ghost_colour(fit, colour);
        let mut view = footprint_view(tile, 1, 1, 0);
        view.line = Some(line);
        let mut slab = wall_block(&view, look.height, look.thick, colour, &ground);
        slab.lift += look.lift;
        out.push(slab);
    }
}

/// A flat plate over the tiles `[x, y, width, depth]`, lying on the ground
/// in `colour` (the last number is how much it glows).
pub fn push_plate(
    area: [i32; 4],
    colour: [u8; 4],
    ground: impl Fn(f32, f32) -> f32,
    out: &mut Vec<BoxInstance>,
) {
    let [x, y, w, d] = area;
    let (Ok(w), Ok(d)) = (u8::try_from(w), u8::try_from(d)) else {
        return;
    };
    if out.len() >= MAX_BOX_INSTANCES {
        return;
    }
    out.push(block(
        &footprint_view((x, y), w, d, 0),
        PLATE,
        colour,
        &ground,
    ));
}

/// The faint blueprint of a building paid for and waiting on its ground
/// being cleared: its footprint, in its owner's colour.
pub fn push_blueprint(
    stats: &BuildingStats,
    tile: (i32, i32),
    turn: u8,
    colour: [u8; 3],
    ground: impl Fn(f32, f32) -> f32,
    out: &mut Vec<BoxInstance>,
) {
    if out.len() >= MAX_BOX_INSTANCES {
        return;
    }
    let (width, depth) = stats.size(turn);
    let view = footprint_view(tile, width, depth, turn);
    let [r, g, b] = colour;
    // Low and faint, so the cleared ground still shows through.
    let height = height_of(stats.role) * BLUEPRINT_HEIGHT;
    out.push(block(&view, height, [r, g, b, BLUEPRINT_GLOW], &ground));
}

/// The nearest building `accept` allows that the ray from `origin` along
/// `dir` passes through.
pub fn pick(
    world: &World,
    table: &Table,
    ground: impl Fn(f32, f32) -> f32,
    (origin, dir): (Vec3, Vec3),
    accept: impl Fn(&StructureView) -> bool,
) -> Option<StructureHandle> {
    let mut best: Option<(f32, StructureHandle)> = None;
    for s in world.structures()?.iter().filter(|s| accept(s)) {
        let role = table.stats.get(usize::from(s.building))?.role;
        let (lo, _) = ground_range(&ground, &s);
        let min = Vec3::new(s.tile.0 as f32, lo, s.tile.1 as f32);
        let max = min + Vec3::new(f32::from(s.width), height_of(role), f32::from(s.depth));
        if let Some(t) = ray_box(origin, dir, min, max)
            && best.is_none_or(|(b, _)| t < b)
        {
            best = Some((t, s.handle));
        }
    }
    best.map(|(_, h)| h)
}

/// How far along the ray it enters the box (0 when it starts inside), if it
/// does at all.
pub(crate) fn ray_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let (mut near, mut far) = (0.0f32, f32::MAX);
    for k in 0..3 {
        if dir[k].abs() < 1e-9 {
            if origin[k] < min[k] || origin[k] > max[k] {
                return None;
            }
            continue;
        }
        let (a, b) = ((min[k] - origin[k]) / dir[k], (max[k] - origin[k]) / dir[k]);
        near = near.max(a.min(b));
        far = far.min(a.max(b));
    }
    (near <= far).then_some(near)
}

/// The north-west tile of a `width` × `depth` footprint centred on `at` as
/// near as whole tiles allow (pass the turned size for a turned building).
pub fn tile_for(at: Vec2, width: u8, depth: u8) -> (i32, i32) {
    (
        (at.x - f32::from(width) / 2.0).round() as i32,
        (at.y - f32::from(depth) / 2.0).round() as i32,
    )
}

/// How high above the ground a man stands on a wall drawn with `look`:
/// its top, and the coping on it if it has one.
pub fn wall_stand_height(look: WallLook) -> f32 {
    look.lift + look.height + look.cap.map_or(0.0, |_| COPING_HEIGHT)
}

/// Where men stand on the wall piece `s`: the middle of its footprint, how
/// high its top is, and for a piece longer than a tile (a gatehouse) the
/// half of its length along it.
pub fn wall_slot(table: &Table, s: &StructureView) -> crate::puppets::WallSlot {
    let long = f32::from(s.width.max(s.depth));
    crate::puppets::WallSlot {
        centre: centre_of(s.tile, s.width, s.depth),
        lift: table.stand_height(s.building, s.open),
        half: if long > 1.0 {
            long_axis(s.width, s.depth) * (long / 2.0)
        } else {
            Vec2::ZERO
        },
    }
}

/// The places regiment `h`'s soldiers stand on the wall it is manning
/// (see [`World::wall_tiles`]): piece centres, with how high each top is.
/// Empty unless it is on a wall.
pub fn wall_slots<'a>(
    world: &'a World,
    table: &'a Table,
    h: gc_sim::RegimentHandle,
) -> impl Iterator<Item = crate::puppets::WallSlot> + 'a {
    let structures = world.structures();
    world
        .wall_tiles(h)
        .iter()
        .filter_map(move |&t| structures?.get(t))
        .map(move |s| wall_slot(table, &s))
}

/// Where the three ladders against the wall piece regiment `h` climbs go:
/// centre, facing (into the wall) and across unit vector, with the height
/// of the wall there. `from` is where the regiment stands.
pub fn ladder_props(
    world: &World,
    table: &Table,
    h: gc_sim::RegimentHandle,
    from: Vec2,
) -> Option<([Vec2; 3], Vec2, f32)> {
    let slot = wall_slots(world, table, h).next()?;
    ladders_against(&slot, from)
}

/// The ladders for `slot` as `ladder_props` gives them. Against a gatehouse
/// they lean on its long side nearer `from`, spread along its length.
fn ladders_against(slot: &crate::puppets::WallSlot, from: Vec2) -> Option<([Vec2; 3], Vec2, f32)> {
    let to_from = from - slot.centre;
    let len = slot.half.length();
    let (out, apart) = if len > 0.0 {
        let side = Vec2::new(-slot.half.y, slot.half.x) / len;
        (
            if side.dot(to_from) >= 0.0 {
                side
            } else {
                -side
            },
            len * 2.0 / 3.0,
        )
    } else {
        (to_from.try_normalize()?, LADDER_APART)
    };
    let facing = -out;
    let across = Vec2::new(-facing.y, facing.x);
    let at = slot.centre + out * LADDER_STANDOFF;
    Some((
        [at - across * apart, at, at + across * apart],
        facing,
        slot.lift,
    ))
}

/// A refusal in the panel's words.
pub fn why(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::NoBuildings => "There are no buildings here.",
        Refusal::Unknown => "That can't be done here.",
        Refusal::OffMap => "That would hang over the edge of the map.",
        Refusal::Rough => "The ground there is too rough to build on.",
        Refusal::Built => "Another building is in the way.",
        Refusal::Crowded => "A regiment is standing in the way.",
        Refusal::Node => "Workers need room at a seep or vein.",
        Refusal::Tier => "Your tier is too low for that.",
        Refusal::Price => "You can't afford that.",
        Refusal::Busy => "That building is busy.",
        Refusal::Larva => "No larvae are ready.",
        Refusal::Unpowered => "It must stand inside a field of power.",
        Refusal::Cooling => "That power isn't ready again yet.",
        Refusal::NoShrine => "Edicts are cast from a finished Ancestor Shrine.",
        Refusal::Overgrown => "Clear the ground there first.",
        Refusal::NoRoom => "There's no room on the map for more buildings.",
        Refusal::NoVeins => "It grows only where your root-veins reach.",
        Refusal::NoWalkway => {
            "Raise the wall a level (or a gate to a gatehouse) first: only a walkway can be manned."
        }
        Refusal::Crooked => "A bridge must run straight across, not at a slant.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The building type called `name`.
    fn id(table: &Table, name: &str) -> BuildingId {
        (0..table.len() as BuildingId)
            .find(|&b| table.name(b) == name)
            .unwrap_or_else(|| panic!("no {name}"))
    }

    #[test]
    fn the_baked_twist_numbers_are_the_sims() {
        use gc_sim::structures::WallTwist;
        for (byte, twist) in [
            (record::TWIST_NONE, WallTwist::None),
            (record::TWIST_STONES, WallTwist::Stones),
            (record::TWIST_MENDS, WallTwist::Mends),
            (record::TWIST_SINGS, WallTwist::Sings),
            (record::TWIST_THORNS, WallTwist::Thorns),
        ] {
            assert_eq!(WallTwist::from_u8(byte), Some(twist));
        }
        assert_eq!(WallTwist::from_u8(record::TWIST_THORNS + 1), None);
    }

    #[test]
    fn men_stand_on_the_coping_of_a_raised_wall() {
        let plain = wall_look(Some(Faction::EmberClans), 0, 1, false);
        let raised = wall_look(Some(Faction::EmberClans), 0, 2, false);
        assert_eq!(wall_stand_height(plain), plain.height);
        assert!(raised.cap.is_some());
        assert_eq!(wall_stand_height(raised), raised.height + COPING_HEIGHT);
        assert!(wall_stand_height(raised) > wall_stand_height(plain));
    }

    #[test]
    fn each_faction_wall_has_its_own_look() {
        use gc_sim::structures::{GROWN, LINE, PASS};
        let plain = wall_look(Some(Faction::EmberClans), LINE, 1, false);
        assert_eq!(plain, WallLook::PLAIN);
        let bone = wall_look(Some(Faction::CarrionConcord), LINE, 1, false);
        assert!(bone.colour.is_some() && bone.height < plain.height);
        let up = wall_look(Some(Faction::Choir), LINE | PASS, 1, false);
        let down = wall_look(Some(Faction::Choir), LINE | PASS, 1, true);
        assert!(up.thick < plain.thick && up.height > down.height);
        assert!(up.glow > down.glow);
        let resin = wall_look(Some(Faction::Brood), LINE | GROWN, 1, false);
        assert!(resin.thick > plain.thick && resin.height < plain.height);
        // A dropped curtain stays flat and bare however high it was raised.
        let flat = wall_look(Some(Faction::Choir), LINE | PASS, 3, true);
        assert_eq!(flat, down);
    }

    #[test]
    fn wall_levels_follow_the_upgrade_chain() {
        let data = crate::data::load();
        let table = Table::new(&data);
        let chains = [
            ["Palisade", "Stone Wall", "Castle Wall"],
            ["Bone Fence", "Ossuary Wall", "Mausoleum Wall"],
            ["Light Curtain", "Glass Wall", "Singing Wall"],
            ["Resin Ridge", "Chitin Wall", "Thorn Bulwark"],
        ];
        for chain in chains {
            let looks = chain.map(|n| {
                let b = id(&table, n);
                let level = table.level(b);
                (level, table.wall_look(b, false))
            });
            assert_eq!(looks.map(|l| l.0), [1, 2, 3], "{chain:?}");
            let [a, b, c] = looks.map(|l| l.1);
            assert!(a.cap.is_none() && !a.merlons, "{chain:?}");
            assert!(
                b.height > a.height && b.thick > a.thick && b.cap.is_some(),
                "{chain:?}"
            );
            assert!(c.height > b.height && c.thick > b.thick, "{chain:?}");
            assert!(c.merlons && !b.merlons, "{chain:?}");
            assert_ne!(a.colour, b.colour);
            assert_ne!(b.colour, c.colour);
        }
        assert_eq!(table.level(id(&table, "Clan Hall")), 1);
    }

    #[test]
    fn a_raised_wall_gets_a_coping_and_merlons() {
        let look = |level| {
            wall_look(
                Some(Faction::EmberClans),
                gc_sim::structures::LINE,
                level,
                false,
            )
        };
        let body = BoxInstance {
            pos: [4.0, 4.0],
            facing: [1.0, 0.0],
            size: [0.6, 2.0, 1.0],
            color: [1, 2, 3, 0],
            lift: 0.0,
        };
        let count = |level| {
            let l = look(level);
            let mut out = Vec::new();
            push_wall_top(&body, l, &mut out);
            out.len()
        };
        assert_eq!((count(1), count(2), count(3)), (0, 1, 3));
    }

    #[test]
    fn every_baked_building_converts_whole() {
        let data = crate::data::load();
        let table = Table::new(&data);
        assert_eq!(table.len(), data.buildings.len());
        assert!(table.len() >= 40, "{}", table.len());
        for (b, s) in data.buildings.iter().zip(&table.stats) {
            // Nothing needed forcing into range.
            assert_eq!(*s, s.sanitized(), "{}", b.name());
            assert_eq!((s.width, s.depth, s.hp), (b.width, b.depth, b.hp));
            assert_eq!(s.attack.is_some(), b.attack_range > 0, "{}", b.name());
            assert_eq!(s.upgrades_from.is_some(), b.upgrades_from != record::NONE);
        }
        // A Clan Hall is a main building and a Palisade a wall.
        let role = |name: &str| {
            let i = data.buildings.iter().position(|b| b.name() == name);
            table.stats[i.unwrap_or_else(|| panic!("no {name}"))].role
        };
        assert_eq!(role("Clan Hall"), Role::Main);
        assert_eq!(role("Palisade"), Role::Wall);
        assert_eq!(role("Watchtower"), Role::Defence);
        assert_eq!(table.units.len(), data.units.len());
        assert!(table.describe(0).contains("Clan Hall"));
    }

    #[test]
    fn each_faction_has_a_flat_bridge_that_is_in_its_build_menu() {
        let data = crate::data::load();
        let table = Table::new(&data);
        let mut seen = Vec::new();
        for b in 0..table.len() as BuildingId {
            if !table.is_bridge(b) {
                continue;
            }
            let faction = table.faction(b).expect("a bridge has a faction");
            assert!(!seen.contains(&faction), "two bridges for {faction:?}");
            seen.push(faction);
            assert!(table.is_wall(b), "a bridge is drawn as a line");
            assert!(table.buildable(faction).any(|x| x == b));
            let look = table.wall_look(b, false);
            // A flat deck a tile wide, not a wall slab, and men stand on it.
            assert!(look.height < 0.3 && look.thick == 1.0 && look.lift > 0.0);
            assert!(look.cap.is_none() && !look.merlons);
            assert!((table.stand_height(b, false) - DECK_TOP).abs() < 1e-6);
            assert!(table.stand_height(b, false) < WALL_HEIGHT);
        }
        assert_eq!(seen.len(), 4, "{seen:?}");
    }

    #[test]
    fn a_bridge_tile_is_a_deck_with_rails_and_a_frame_while_it_goes_up() {
        let mut view = footprint_view((10, 5), 1, 1, 0);
        view.line = Some([10, 3, 10, 9]);
        view.built_percent = 100;
        let mut out = Vec::new();
        push_bridge_tile(&view, [150, 106, 62, 0], [200, 40, 40], &mut out);
        // Deck, two rails, two posts; nothing taller than the railing.
        assert_eq!(out.len(), 5);
        let top = out.iter().map(|b| b.lift + b.size[1]).fold(0.0, f32::max);
        assert!((top - (DECK_TOP + RAIL_HEIGHT)).abs() < 1e-5, "{top}");
        assert!(top < WALL_HEIGHT);
        // The deck runs a tile wide along the line (y here).
        assert_eq!((out[0].size[0], out[0].facing[0]), (1.0, 0.0));
        view.built_percent = 20;
        let mut frame = Vec::new();
        push_bridge_tile(&view, [150, 106, 62, 0], [200, 40, 40], &mut frame);
        assert_eq!(frame.len(), 2);
        view.built_percent = 80;
        let mut later = Vec::new();
        push_bridge_tile(&view, [150, 106, 62, 0], [200, 40, 40], &mut later);
        assert!(later[0].size[1] > frame[0].size[1]);
    }

    #[test]
    fn each_faction_has_stakes_and_a_barricade_that_are_low_and_not_walls() {
        let data = crate::data::load();
        let table = Table::new(&data);
        let (mut stakes, mut barricades) = (0, 0);
        for b in 0..table.len() as BuildingId {
            if !table.is_fieldwork(b) {
                continue;
            }
            let faction = table.faction(b).expect("a field work has a faction");
            assert!(table.is_wall(b) && !table.is_bridge(b));
            assert!(table.buildable(faction).any(|x| x == b));
            let role = table.stats[usize::from(b)].role;
            if role == Role::Stakes {
                stakes += 1;
                assert_eq!(table.stakes_of(faction), Some(b));
            } else {
                barricades += 1;
            }
            let look = table.wall_look(b, false);
            assert!(
                look.height < WALL_HEIGHT * 0.6 && look.height > 0.5,
                "{look:?}"
            );
            assert!(look.cap.is_none() && !look.merlons && look.lift == 0.0);
            assert_eq!(height_of(role), look.height);
        }
        assert_eq!((stakes, barricades), (4, 4));
    }

    #[test]
    fn field_work_tiles_are_boxes_that_stay_low_and_grow_while_going_up() {
        let works = |stakes| Fieldwork {
            stakes,
            body: [140, 98, 58, 0],
            trim: [190, 150, 98],
            team: [200, 40, 40],
        };
        let flat = |_: f32, _: f32| 0.0;
        for stakes in [true, false] {
            let mut view = footprint_view((10, 5), 1, 1, 0);
            view.line = Some([10, 3, 10, 9]);
            let mut done = Vec::new();
            push_fieldwork_tile(&view, works(stakes), &flat, &mut done);
            assert!(done.len() >= 4 && done.len() <= 10, "{}", done.len());
            let top = done.iter().map(|b| b.lift + b.size[1]).fold(0.0, f32::max);
            assert!(top <= BARRICADE_HEIGHT + 0.1 && top > 0.5, "{top}");
            view.built_percent = 20;
            let mut early = Vec::new();
            push_fieldwork_tile(&view, works(stakes), &flat, &mut early);
            let early_top = early.iter().map(|b| b.lift + b.size[1]).fold(0.0, f32::max);
            assert!(!early.is_empty() && early.len() < done.len(), "{stakes}");
            assert!(early_top < top, "{early_top} {top}");
        }
    }

    /// The extent of `slabs` along one axis (`x` or not), as (low, high).
    fn span(slabs: &[Slab], x: bool) -> (f32, f32) {
        let side = |s: &Slab| {
            if x {
                (s.centre.x, s.size.x)
            } else {
                (s.centre.y, s.size.y)
            }
        };
        slabs.iter().fold((f32::MAX, f32::MIN), |(lo, hi), s| {
            let (c, len) = side(s);
            (lo.min(c - len / 2.0), hi.max(c + len / 2.0))
        })
    }

    #[test]
    fn shut_leaves_span_the_doorway_and_open_ones_lie_along_the_jambs() {
        // As drawn: 1 wide (x), 3 deep (y), so the wall runs along y.
        let centre = Vec2::new(10.5, 21.5);
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        let shut = gate_parts(centre, 1, 3, false);
        // The posts take half a tile at each end; the leaves meet between.
        let (lo, hi) = span(&shut.posts, false);
        assert!(near(lo, 20.0) && near(hi, 23.0), "{lo} {hi}");
        let (lo, hi) = span(&shut.leaves, false);
        assert!(near(lo, 20.5) && near(hi, 22.5), "{lo} {hi}");
        assert!(near(
            shut.leaves[0].centre.y + shut.leaves[0].size.y / 2.0,
            shut.leaves[1].centre.y - shut.leaves[1].size.y / 2.0
        ));
        for leaf in shut.leaves {
            // Thin across the passage, on the footprint's middle line.
            assert!(near(leaf.size.x, LEAF_THICK) && near(leaf.centre.x, centre.x));
        }
        let open = gate_parts(centre, 1, 3, true);
        for (leaf, post) in open.leaves.iter().zip(&open.posts) {
            // Along the passage, inside the footprint, beside its post.
            assert!(near(leaf.size.y, LEAF_THICK) && near(leaf.size.x, 1.0));
            assert!(leaf.centre.x - leaf.size.x / 2.0 >= 10.0 - 1e-4);
            assert!(leaf.centre.x + leaf.size.x / 2.0 <= 11.0 + 1e-4);
            let gap = (leaf.centre.y - post.centre.y).abs() - (leaf.size.y + post.size.y) / 2.0;
            assert!(near(gap, 0.0), "{gap}");
        }
        // The doorway is clear between the open leaves.
        let (a, b) = (open.leaves[0], open.leaves[1]);
        assert!(b.centre.y - a.centre.y > 1.5);
        // Turned a quarter, the wall runs along x and the same holds.
        let turned = gate_parts(centre, 3, 1, false);
        let (lo, hi) = span(&turned.leaves, true);
        assert!(near(lo, 9.5) && near(hi, 11.5), "{lo} {hi}");
        assert!(turned.leaves.iter().all(|l| near(l.size.y, LEAF_THICK)));
        let turned_open = gate_parts(centre, 3, 1, true);
        assert!(
            turned_open
                .leaves
                .iter()
                .all(|l| near(l.size.x, LEAF_THICK) && near(l.size.y, 1.0))
        );
    }

    #[test]
    fn stages_follow_the_work_done() {
        assert_eq!(stage_of(0), 0);
        assert_eq!(stage_of(24), 0);
        assert_eq!(stage_of(25), 1);
        assert_eq!(stage_of(59), 1);
        assert_eq!(stage_of(60), 2);
        assert_eq!(stage_of(99), 2);
        assert_eq!(stage_of(100), FINISHED);
        assert_eq!(stage_name("Clan Hall", 0), "Clan Hall #1");
        assert_eq!(stage_name("Clan Hall", FINISHED), "Clan Hall");
    }

    /// Every building model in the pack dresses a building (or one of its
    /// stages), fits the name field and its footprint, and stands still.
    #[test]
    fn building_models_dress_buildings() {
        let data = crate::data::load();
        let table = Table::new(&data);
        let models = crate::models::load().unwrap();
        let looks = Looks::new(&table, &models);
        let names: Vec<String> = (table.names.iter())
            .flat_map(|n| (0..STAGES).map(move |k| stage_name(n, k)))
            .collect();
        for n in &names {
            assert!(n.len() <= gc_assets::model::NAME_BYTES, "{n}");
        }
        let mut dressed = 0;
        for (b, stages) in looks.stages.iter().enumerate() {
            let stats = table.stats[b];
            for (stage, m) in stages.iter().enumerate() {
                let Some(m) = m else { continue };
                let r = models.models[*m];
                assert_eq!(r.bones, 1, "{} is rigged", r.name());
                // Inside its footprint, give or take the eaves (and the
                // scaffolding, while it goes up).
                let over = if stage == FINISHED { 0.45 } else { 0.6 };
                let reach = f32::from(stats.width.max(stats.depth)) / 2.0 + over;
                let first = r.first_vertex as usize;
                for v in &models.vertices[first..first + r.vertex_count as usize] {
                    let [x, _, z] = v
                        .pos
                        .map(|c| f32::from(c) / gc_assets::model::POSITION_SCALE);
                    assert!(
                        x.abs() <= reach && z.abs() <= reach,
                        "{} at {x}, {z}",
                        r.name()
                    );
                }
                dressed += 1;
            }
            // A finished model needs every stage a worker can lay.
            if stages[FINISHED].is_some() && stats.upgrades_from.is_none() {
                assert!(
                    stages.iter().all(Option::is_some),
                    "{} lacks a stage",
                    table.name(b as u16)
                );
            }
        }
        let still = models.models.iter().filter(|m| m.is_still()).count();
        // The models a drawn wall's name would have found are left unused.
        let unused = (table.names.iter().zip(&table.stats))
            .filter(|(_, s)| s.flags & gc_sim::structures::LINE != 0)
            .flat_map(|(n, _)| (0..STAGES).map(move |k| stage_name(n, k)))
            .filter(|n| models.by_name(n).is_some())
            .count();
        assert_eq!(
            dressed + unused,
            still,
            "a building model's name matches no building"
        );
        assert!(
            looks.stages[0][FINISHED].is_some(),
            "the Clan Hall has no model"
        );
        assert!(
            looks.crews.iter().all(Option::is_some),
            "a faction lacks its worker"
        );
    }

    #[test]
    fn a_building_shows_its_stage_and_its_upgrade() {
        let data = crate::data::load();
        let table = Table::new(&data);
        let models = crate::models::load().unwrap();
        let looks = Looks::new(&table, &models);
        let (hall, keep) = (id(&table, "Clan Hall"), id(&table, "Castle Keep"));
        let mut s = StructureView {
            handle: StructureHandle {
                index: 0,
                generation: 0,
            },
            owner: 0,
            building: hall,
            tile: (0, 0),
            width: 5,
            depth: 5,
            turn: 0,
            hp: 10,
            max_hp: 100,
            built_percent: 30,
            upgrading: None,
            growing: false,
            seen: 1,
            gate: None,
            open: false,
            line: None,
        };
        let model = |s: &StructureView| looks.model_of(s).map(|m| models.models[m].name());
        assert_eq!(model(&s), Some("Clan Hall #2"));
        assert_eq!(looks.crew_of(&s), None);
        s.built_percent = 100;
        assert_eq!(model(&s), Some("Clan Hall"));
        // An upgrade shows the old hall, then the upgrade going up (or the
        // old hall while the upgrade has no model), with a crew at work.
        s.upgrading = Some((keep, 20));
        assert_eq!(model(&s), Some("Clan Hall"));
        assert_eq!(
            looks.crew_of(&s).map(|m| models.models[m].name()),
            Some("Hands")
        );
        s.upgrading = Some((keep, 70));
        let later = model(&s).unwrap();
        assert!(later == "Castle Keep #3" || later == "Clan Hall", "{later}");
    }

    /// A gate-sized piece at `tile`, as drawn (1 wide, 3 deep) or turned.
    fn piece(building: BuildingId, tile: (i32, i32), turned: bool) -> StructureView {
        let (width, depth) = if turned { (3, 1) } else { (1, 3) };
        StructureView {
            handle: StructureHandle {
                index: 0,
                generation: 0,
            },
            owner: 0,
            building,
            tile,
            width,
            depth,
            turn: u8::from(turned),
            hp: 10,
            max_hp: 10,
            built_percent: 100,
            upgrading: None,
            growing: false,
            seen: 1,
            gate: Some(gc_sim::gates::GateMode::Auto),
            open: false,
            line: None,
        }
    }

    #[test]
    fn each_gate_has_its_gatehouse_and_only_those_are_gatehouses() {
        let data = crate::data::load();
        let table = Table::new(&data);
        let mut found = Vec::new();
        for (gate, house) in [
            ("Gate", "Yagura Gatehouse"),
            ("Iron Gate", "Gate of Debts"),
            ("Maw Gate", "Chitin Maw"),
        ] {
            let (g, h) = (id(&table, gate), id(&table, house));
            assert!(table.is_gate(g) && !table.is_gatehouse(g));
            assert!(table.is_gatehouse(h));
            assert_eq!(table.stats[usize::from(h)].upgrades_from, Some(g));
            assert_eq!(table.level(h), 2);
            // The ring tool still suggests the plain gate.
            assert_eq!(table.gate_for(g), Some(g));
            assert_eq!(table.gate_for(h), Some(g));
            found.push(h);
        }
        let all: Vec<BuildingId> = (0..table.len() as BuildingId)
            .filter(|&b| table.is_gatehouse(b))
            .collect();
        found.sort_unstable();
        assert_eq!(all, found);
    }

    #[test]
    fn a_gatehouse_slot_has_the_deck_lift_and_its_length() {
        let data = crate::data::load();
        let table = Table::new(&data);
        let house = id(&table, "Gate of Debts");
        let stats = table.stats[usize::from(house)];
        assert_eq!((stats.width, stats.depth), (1, 3));
        let slot = wall_slot(&table, &piece(house, (10, 20), false));
        assert_eq!(slot.lift, GATEHOUSE_DECK);
        assert_eq!(slot.half, Vec2::new(0.0, 1.5));
        assert_eq!(slot.centre, Vec2::new(10.5, 21.5));
        // Turned, it runs along x.
        let turned = wall_slot(&table, &piece(house, (10, 20), true));
        assert_eq!(turned.half, Vec2::new(1.5, 0.0));
        assert_eq!(turned.centre, Vec2::new(11.5, 20.5));
        // The deck is above a plain wall's top.
        assert!(GATEHOUSE_DECK > wall_stand_height(WallLook::PLAIN));
    }

    #[test]
    fn ladders_lean_on_the_long_side_of_a_gatehouse_at_the_deck() {
        let data = crate::data::load();
        let table = Table::new(&data);
        let house = id(&table, "Yagura Gatehouse");
        let slot = wall_slot(&table, &piece(house, (10, 20), false));
        for (from, side) in [(Vec2::new(4.0, 23.0), -1.0), (Vec2::new(17.0, 19.0), 1.0)] {
            let (at, facing, top) = ladders_against(&slot, from).unwrap();
            assert_eq!(top, GATEHOUSE_DECK);
            // Square on to the long side, whichever way the man stands.
            assert_eq!(facing, Vec2::new(-side, 0.0));
            for a in at {
                assert!((a.x - (10.5 + side * LADDER_STANDOFF)).abs() < 1e-5, "{a}");
            }
            // Along its length: one at each end and one in the middle.
            let ys = at.map(|a| a.y);
            assert!(ys.iter().all(|y| (20.0..=23.0).contains(y)), "{ys:?}");
            assert!((ys[1] - 21.5).abs() < 1e-5 && (ys[2] - ys[0]).abs() > 1.5);
        }
        // A tile of wall is as before: ladders a tile apart, square-ish.
        let tile = crate::puppets::WallSlot {
            centre: Vec2::new(5.5, 5.5),
            lift: 2.0,
            half: Vec2::ZERO,
        };
        let (at, facing, _) = ladders_against(&tile, Vec2::new(5.5, 9.5)).unwrap();
        assert_eq!(facing, Vec2::NEG_Y);
        assert!((at[1] - Vec2::new(5.5, 5.5 + LADDER_STANDOFF)).length() < 1e-5);
        assert!((at[2].x - at[0].x - 2.0 * LADDER_APART).abs() < 1e-5);
    }

    #[test]
    fn a_gate_upgrading_shows_its_crew_and_its_third_look() {
        let data = crate::data::load();
        let table = Table::new(&data);
        let models = crate::models::load().unwrap();
        let looks = Looks::new(&table, &models);
        let (gate, house) = (id(&table, "Gate"), id(&table, "Yagura Gatehouse"));
        let mut s = piece(gate, (10, 20), false);
        let name = |s: &StructureView| looks.model_of(s).map(|m| models.models[m].name());
        assert_eq!(name(&s), Some("Gate"));
        assert_eq!(looks.crew_of(&s), None);
        s.upgrading = Some((house, 20));
        assert_eq!(name(&s), Some("Gate"));
        let crew = looks.crew_of(&s).map(|m| models.models[m].name());
        assert_eq!(crew, Some("Hands"));
        // The upgrade's third look, or the gate while it has none.
        s.upgrading = Some((house, 70));
        let later = name(&s).unwrap();
        assert!(later == "Yagura Gatehouse #3" || later == "Gate", "{later}");
        // A worker never lays a gatehouse: no foundation or frame.
        assert!(looks.stage(house, 0).is_none() && looks.stage(house, 1).is_none());
        // Its door is drawn as the gate's is: the same leaves, open or shut.
        s.upgrading = None;
        s.building = house;
        let ground = |_: f32, _: f32| 0.0;
        let (mut shut, mut open) = (Vec::new(), Vec::new());
        push_gate(&s, None, true, &ground, &mut shut);
        s.open = true;
        push_gate(&s, None, true, &ground, &mut open);
        assert_eq!((shut.len(), open.len()), (2, 2));
        assert_ne!(shut[0].size, open[0].size);
    }

    #[test]
    fn a_gatehouse_without_a_model_is_roofed_at_the_deck() {
        let data = crate::data::load();
        let table = Table::new(&data);
        let house = (0..table.len() as BuildingId)
            .find(|&b| table.is_gatehouse(b))
            .unwrap();
        let s = piece(house, (10, 20), false);
        let ground = |_: f32, _: f32| 0.0;
        let mut boxes = Vec::new();
        push_gate(&s, Some([1, 2, 3]), true, &ground, &mut boxes);
        // Two posts, the deck slab and two leaves; the deck tops at the deck.
        assert_eq!(boxes.len(), 5);
        let top = |b: &BoxInstance| b.lift + b.size[1];
        assert!(
            boxes[..3]
                .iter()
                .all(|b| (top(b) - GATEHOUSE_DECK).abs() < 1e-5)
        );
        assert_eq!(boxes[2].size[0], 1.0);
        assert_eq!(boxes[2].size[2], 3.0);
    }

    #[test]
    fn the_crew_of_a_gate_stands_on_its_long_sides() {
        for (w, d) in [(1, 3), (3, 1)] {
            let centre = Vec2::new(20.0, 30.0);
            let (hw, hd) = (f32::from(w) / 2.0, f32::from(d) / 2.0);
            for (at, facing) in crew_spots(centre, w, d) {
                let off = at - centre;
                // Beside a long side, not beyond the ends, turned to it.
                let ((thin, long), (across, along)) = if w < d {
                    ((hw, hd), (off.x, off.y))
                } else {
                    ((hd, hw), (off.y, off.x))
                };
                assert!(across.abs() > thin && along.abs() <= long, "{off}");
                assert!(facing.dot(-off) > 0.0, "{off} {facing}");
            }
        }
    }

    #[test]
    fn the_upgrade_crew_stands_outside_the_walls() {
        let centre = Vec2::new(20.0, 30.0);
        for (at, facing) in crew_spots(centre, 5, 4) {
            let off = at - centre;
            assert!(off.x.abs() > 2.5 || off.y.abs() > 2.0, "{off}");
            // Facing the building.
            assert!(facing.dot(-off) > 0.0, "{off} {facing}");
        }
    }

    #[test]
    fn building_types_wrap_round() {
        let table = Table::new(&crate::data::load());
        let last = (table.len() - 1) as BuildingId;
        assert_eq!(table.step(0, -1), last);
        assert_eq!(table.step(last, 1), 0);
        assert_eq!(table.step(3, 2), 5);
    }

    #[test]
    fn buildings_darken_as_they_fall() {
        let full = shade([200, 100, 50], 100, 100);
        assert_eq!(full, [200, 100, 50]);
        let half = shade([200, 100, 50], 50, 100);
        let low = shade([200, 100, 50], 1, 100);
        assert!(half[0] < full[0] && low[0] < half[0], "{half:?} {low:?}");
        assert!(low[0] > 0, "a building with hit points left still shows");
    }

    #[test]
    fn the_footprint_centres_on_the_cursor() {
        // A 3x3 centred on the middle of tile (10, 10) takes tiles 9 to 11.
        assert_eq!(tile_for(Vec2::new(10.5, 10.5), 3, 3), (9, 9));
        // A 2x2 centred on a tile corner.
        assert_eq!(tile_for(Vec2::new(10.0, 10.0), 2, 2), (9, 9));
        assert_eq!(tile_for(Vec2::new(10.4, 10.6), 1, 2), (10, 10));
    }

    #[test]
    fn a_turned_ghost_swaps_its_sides_and_keeps_its_middle() {
        let table = Table::new(&crate::data::load());
        let stats = *table
            .stats
            .iter()
            .find(|s| s.width != s.depth)
            .expect("a building that is not square");
        let at = Vec2::new(20.0, 20.0);
        for turn in 0..gc_sim::structures::TURNS {
            let (w, d) = stats.size(turn);
            assert_eq!((w, d) == (stats.depth, stats.width), turn % 2 == 1);
            let tile = tile_for(at, w, d);
            let mut out = Vec::new();
            push_ghost(
                &stats,
                tile,
                turn,
                Fit::Open,
                [1, 2, 3],
                |_, _| 0.0,
                &mut out,
            );
            let [ghost, step] = out[..] else {
                panic!("the building and its front step");
            };
            assert_eq!(ghost.size[0], f32::from(w));
            assert_eq!(ghost.size[2], f32::from(d));
            assert!((Vec2::from(ghost.pos) - at).length() <= 0.5 + 1e-3);
            // The step sits just outside its front edge.
            let out_of = (Vec2::from(step.pos) - Vec2::from(ghost.pos)).dot(facing_of(turn));
            let half = if turn % 2 == 0 { d } else { w };
            assert!((out_of - (f32::from(half) / 2.0 + 0.5)).abs() < 1e-3);
        }
    }

    #[test]
    fn a_trap_is_drawn_low_and_inside_its_tile() {
        use gc_sim::structures::Trap;
        for built in [40, 100] {
            for trap in [Trap::Pit, Trap::Hands, Trap::Mine, Trap::Nest(0)] {
                let view = StructureView {
                    handle: StructureHandle {
                        index: 0,
                        generation: 0,
                    },
                    owner: 0,
                    building: 0,
                    tile: (4, 6),
                    width: 1,
                    depth: 1,
                    turn: 0,
                    hp: 60,
                    max_hp: 60,
                    built_percent: built,
                    upgrading: None,
                    growing: false,
                    seen: 1,
                    gate: None,
                    open: false,
                    line: None,
                };
                let mut out = Vec::new();
                push_trap_tile(&view, trap, |c| c, &mut out);
                assert!(!out.is_empty());
                for b in &out {
                    assert!(b.lift + b.size[1] <= 0.4, "{trap:?} too tall");
                    let off = Vec2::from(b.pos) - Vec2::new(4.5, 6.5);
                    assert!(off.abs().max_element() <= 0.5, "{trap:?} off its tile");
                }
            }
        }
    }

    #[test]
    fn a_wall_line_of_one_tile_is_a_block() {
        assert_eq!(wall_piece([5, 5, 5, 5], (5, 5)), None);
    }

    #[test]
    fn wall_pieces_meet_end_to_end_along_the_line() {
        for line in [
            [2, 2, 12, 2],
            [2, 2, 2, 9],
            [2, 2, 9, 9],
            [2, 2, 14, 7],
            [20, 3, 4, 9],
        ] {
            let tiles = gc_sim::walls::wall_tiles((line[0], line[1]), (line[2], line[3])).unwrap();
            let pieces: Vec<WallPiece> = tiles
                .iter()
                .map(|&t| wall_piece(line, t).unwrap())
                .collect();
            let end = |p: &WallPiece| p.centre + p.dir * (p.len / 2.0);
            let start = |p: &WallPiece| p.centre - p.dir * (p.len / 2.0);
            for pair in pieces.windows(2) {
                // The next starts before the last ends: they overlap a little.
                let gap = (start(&pair[1]) - end(&pair[0])).dot(pair[0].dir);
                assert!(gap < 0.0 && gap > -0.2, "{line:?} {gap}");
            }
            // The line's own ends are capped out to their tile's edge.
            let first = Vec2::new(line[0] as f32 + 0.5, line[1] as f32 + 0.5);
            let behind = (first - start(&pieces[0])).dot(pieces[0].dir);
            let step = Vec2::new((line[2] - line[0]) as f32, (line[3] - line[1]) as f32).length()
                / (line[2] - line[0]).abs().max((line[3] - line[1]).abs()) as f32;
            assert!((behind - step / 2.0).abs() < 1e-3, "{line:?} {behind}");
        }
    }

    #[test]
    fn a_wall_piece_stays_by_its_tile() {
        for line in [[2, 2, 14, 7], [20, 3, 4, 9], [3, 3, 3, 20], [10, 10, 1, 1]] {
            for t in gc_sim::walls::wall_tiles((line[0], line[1]), (line[2], line[3])).unwrap() {
                let piece = wall_piece(line, t).unwrap();
                let tile_mid = Vec2::new(t.0 as f32 + 0.5, t.1 as f32 + 0.5);
                let off = (piece.centre - tile_mid).abs();
                assert!(off.x <= 0.8 && off.y <= 0.8, "{line:?} {t:?} {off:?}");
                assert!(piece.len > 0.9 && piece.len < 1.7, "{line:?} {t:?}");
            }
        }
    }

    #[test]
    fn a_diagonal_wall_piece_runs_corner_to_corner() {
        let p = wall_piece([0, 0, 4, 4], (2, 2)).unwrap();
        assert!((p.centre - Vec2::splat(2.5)).length() < 1e-4);
        assert!((p.len - (2f32.sqrt() + 2.0 * LINE_JOINT)).abs() < 1e-4);
    }

    #[test]
    fn a_door_turns_clockwise_from_south() {
        // Clockwise seen from above, y south: south, west, north, east.
        let turns: Vec<Vec2> = (0..4).map(facing_of).collect();
        assert_eq!(turns, [Vec2::Y, Vec2::NEG_X, Vec2::NEG_Y, Vec2::X]);
    }

    #[test]
    fn rays_find_the_box_they_cross() {
        let (min, max) = (Vec3::new(10.0, 0.0, 10.0), Vec3::new(13.0, 3.0, 13.0));
        let down = Vec3::new(0.0, -1.0, 0.0);
        // From above, straight down through the roof.
        let t = ray_box(Vec3::new(11.0, 10.0, 11.0), down, min, max).unwrap();
        assert!((t - 7.0).abs() < 1e-4, "{t}");
        // Beside it.
        assert!(ray_box(Vec3::new(5.0, 10.0, 11.0), down, min, max).is_none());
        // A slanting look at its side, below the roof line.
        let slant = Vec3::new(1.0, -0.2, 0.0).normalize();
        assert!(ray_box(Vec3::new(5.0, 2.0, 11.0), slant, min, max).is_some());
        // Behind the eye.
        assert!(ray_box(Vec3::new(5.0, 10.0, 11.0), -down, min, max).is_none());
    }

    #[test]
    fn placement_is_refused_for_the_reasons_the_simulation_has() {
        let data = crate::data::load();
        let table = Table::new(&data);
        let units: Vec<gc_sim::UnitStats> = data.units.iter().map(gc_match::stats_of).collect();
        let tiles = 32u32;
        let side = tiles as usize * gc_sim::nav::CELLS_PER_TILE as usize;
        let mut costs = vec![1u8; side * side];
        // A cliff along the east edge of the map's top left corner tile.
        costs[side * 6 + 20] = gc_sim::nav::BLOCKED;
        let mut world = World::new(&gc_sim::MatchSetup {
            seed: 1,
            capacities: gc_sim::Capacities { regiments: 8 },
            units: &units,
            map_tiles: tiles,
            nav: &costs,
            teams: &[],
            ground: gc_sim::Ground::FLAT,
            champions: false,
            sky: gc_sim::Sky::STILL,
            generals: true,
        });
        table.conquer(&mut world);
        let farm = table
            .stats
            .iter()
            .position(|s| s.role == Role::Housing)
            .unwrap();
        let farm = farm as BuildingId;
        assert_eq!(world.can_build(farm, (8, 8), 0), Ok(()));
        assert!(world.can_build(farm, (-1, 8), 0).is_err());
        assert!(world.can_build(farm, (tiles as i32 - 1, 8), 0).is_err());
        // Cell (20, 6) is tile (10, 3).
        assert!(world.can_build(farm, (9, 2), 0).is_err());
        // What the check allows, the simulation accepts, and then the
        // spot is taken.
        let place = |world: &mut World, tile: (i32, i32)| {
            let command = gc_sim::Command::SpawnStructure {
                building: farm,
                tile: (tile.0 as i16, tile.1 as i16),
                turn: 0,
            };
            world
                .tick(&[gc_sim::PlayerCommand { player: 0, command }])
                .commands_applied
                == 1
        };
        assert!(place(&mut world, (8, 8)));
        assert_eq!(world.structures().unwrap().len(), 1);
        assert!(world.can_build(farm, (9, 9), 0).is_err());
        assert!(!place(&mut world, (9, 9)));
        let standing = world.structures().unwrap().iter().next().unwrap();
        assert_eq!(standing.tile, (8, 8));
    }

    /// Conquest's command card has 12 slots: a worker's build menu and a
    /// building's units (the last slot is its upgrade) must fit.
    #[test]
    fn every_build_menu_and_training_card_fits_the_grid() {
        let data = crate::data::load();
        let table = Table::new(&data);
        for f in Faction::ALL {
            let menu: Vec<BuildingId> = table.buildable(f).collect();
            // More than 12 take two pages, the last slot flipping them.
            assert!(
                !menu.is_empty() && menu.len() <= 2 * 11,
                "{f:?}: {}",
                menu.len()
            );
            assert!(
                menu.iter()
                    .all(|&b| table.stats[usize::from(b)].role != Role::Main)
            );
            assert!(table.expansion(f).is_some(), "{f:?} can't expand");
        }
        assert!(
            table
                .buildable(Faction::EmberClans)
                .any(|b| b == id(&table, "Farmstead"))
        );
        // A building trains what its whole upgrade chain does.
        for b in 0..table.len() as BuildingId {
            let mut chain = vec![b];
            while let Some(from) = table.stats[usize::from(*chain.last().unwrap())].upgrades_from {
                chain.push(from);
            }
            let trains = (data.conquest_units.iter())
                .filter(|u| chain.contains(&u.trained_at))
                .count();
            assert!(trains <= 11, "{} trains {trains}", table.name(b));
        }
    }

    #[test]
    fn a_hasted_regiment_is_outlined_round_its_block_whichever_way_it_faces() {
        let draw = |facing: Vec2| {
            let mut out = Vec::new();
            push_regiment_aura(
                Aura::Haste,
                Vec2::new(50.0, 40.0),
                facing,
                (10.0, 4.0),
                180,
                &mut out,
            );
            out
        };
        let north = draw(Vec2::NEG_Y);
        assert!(north.len() >= 4);
        assert!(
            north
                .iter()
                .all(|b| b.color[3] == 180 && b.color[2] > b.color[0])
        );
        // Round the block: some pieces beyond its width and some beyond its
        // depth, but none far outside it.
        let reach = |out: &[BoxInstance], axis: usize| {
            let at = |b: &BoxInstance| (b.pos[axis] - [50.0, 40.0][axis]).abs();
            out.iter().map(at).fold(0.0f32, f32::max)
        };
        assert!(reach(&north, 0) >= 5.0 && reach(&north, 1) >= 2.0);
        assert!(reach(&north, 0) < 7.0 && reach(&north, 1) < 4.0);
        // Turned to face east, the long way runs north-south.
        let east = draw(Vec2::X);
        assert!(reach(&east, 1) >= 5.0 && reach(&east, 0) < 4.0);
        // A full list draws nothing more.
        let mut full = vec![BoxInstance::default(); MAX_BOX_INSTANCES];
        push_regiment_aura(Aura::Haste, Vec2::ZERO, Vec2::X, (4.0, 4.0), 100, &mut full);
        assert_eq!(full.len(), MAX_BOX_INSTANCES);
    }

    #[test]
    fn each_aura_has_its_own_colour_and_stands_further_out() {
        let outline = |aura| {
            let mut out = Vec::new();
            push_regiment_aura(aura, Vec2::ZERO, Vec2::NEG_Y, (6.0, 2.0), 150, &mut out);
            out
        };
        let reach = |out: &[BoxInstance]| out.iter().map(|b| b.pos[0].abs()).fold(0.0f32, f32::max);
        let (haste, march, steady, ghost) = (
            outline(Aura::Haste),
            outline(Aura::March),
            outline(Aura::Steadfast),
            outline(Aura::Spectral),
        );
        assert!(reach(&haste) < reach(&march) && reach(&march) < reach(&steady));
        assert!(reach(&steady) < reach(&ghost));
        let colour = |out: &[BoxInstance]| out[0].color;
        let colours = [&haste, &march, &steady, &ghost].map(|o| colour(o));
        for (i, a) in colours.iter().enumerate() {
            for b in &colours[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn the_aim_ring_circles_the_point_at_the_radius() {
        let mut out = Vec::new();
        push_aim_ring(Vec2::new(30.0, 20.0), 12.0, 140, &mut out);
        assert!(out.len() >= 16);
        for b in &out {
            let d = (Vec2::from(b.pos) - Vec2::new(30.0, 20.0)).length();
            assert!((d - 12.0).abs() < 0.5, "{d}");
        }
        let mut full = vec![BoxInstance::default(); MAX_BOX_INSTANCES];
        push_aim_ring(Vec2::ZERO, 12.0, 140, &mut full);
        assert_eq!(full.len(), MAX_BOX_INSTANCES);
    }
}
