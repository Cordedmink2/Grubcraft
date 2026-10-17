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
        && record::MAX_REACH_TILES == gc_sim::structures::MAX_REACH
        && record::MAX_FOOTPRINT == gc_sim::structures::MAX_FOOTPRINT
);

/// Height of a wall, of most buildings and of a main building, in tiles.
const WALL_HEIGHT: f32 = 1.5;
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
/// The glow on a ghost that fits.
const GHOST_GLOW: u8 = 110;
/// A gathering spot is a low mound this tall and, while full, this wide
/// (in tiles); it shrinks to `NODE_LEAST` percent as it runs down.
const NODE_HEIGHT: f32 = 0.35;
const NODE_WIDTH: f32 = 1.8;
const NODE_LEAST: f32 = 40.0;
/// Seeps and veins glow in their minimap colours; dry ones don't.
const NODE_GLOW: u8 = 90;
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
    }
}

/// Every building type in the game data, as Conquest worlds read them.
pub struct Table {
    pub stats: Vec<BuildingStats>,
    names: Vec<String>,
    factions: Vec<Faction>,
    /// What the sandbox's Conquest world knows of each regiment type. The
    /// sandbox plays with the Field Battle units, which are all plain
    /// regiments (no workers, no supply); Conquest matches use their own
    /// unit table (`conquest::plan`).
    units: Vec<ConquestUnit>,
}

impl Table {
    pub fn new(data: &GameData) -> Table {
        Table {
            stats: data.buildings.iter().map(stats_of).collect(),
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

    /// A type's name.
    pub fn name(&self, id: BuildingId) -> &str {
        self.names.get(usize::from(id)).map_or("", String::as_str)
    }

    /// What a worker of `faction` lays from the build menu: every building
    /// of theirs but the main building and its upgrades.
    pub fn buildable(&self, faction: Faction) -> impl Iterator<Item = BuildingId> + '_ {
        (self.stats.iter().zip(&self.factions).enumerate())
            .filter(move |(_, (s, f))| **f == faction && s.role != Role::Main)
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
}

impl Looks {
    pub fn new(table: &Table, models: &Models<'_>) -> Looks {
        Looks {
            stages: (table.names.iter())
                .map(|name| core::array::from_fn(|k| models.by_name(&stage_name(name, k))))
                .collect(),
            crews: (table.factions.iter())
                .map(|&f| models.by_name(worker_of(f)))
                .collect(),
            tops: (models.models.iter())
                .map(|m| {
                    let first = m.first_vertex as usize;
                    let vertices = &models.vertices[first..first + m.vertex_count as usize];
                    m.is_still().then(|| {
                        let top = vertices.iter().map(|v| v.pos[1]).max().unwrap_or(0);
                        f32::from(top.max(0)) / gc_assets::model::POSITION_SCALE
                    })
                })
                .collect(),
        }
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

    /// The worker model shown at work on `s`'s tier upgrade, if any.
    pub fn crew_of(&self, s: &StructureView) -> Option<usize> {
        s.upgrading?;
        *self.crews.get(usize::from(s.building))?
    }
}

/// Where the upgrade crew of a `width` by `depth` footprint centred on
/// `centre` stand, and the way each faces: one at the front (south) edge and
/// one on each side, turned to the walls.
pub fn crew_spots(centre: Vec2, width: u8, depth: u8) -> [(Vec2, Vec2); UPGRADE_CREW] {
    let (hw, hd) = (f32::from(width) / 2.0 + 0.4, f32::from(depth) / 2.0 + 0.4);
    [
        (centre + Vec2::new(-hw * 0.3, hd), Vec2::NEG_Y),
        (centre + Vec2::new(hw, hd * 0.2), Vec2::NEG_X),
        (centre + Vec2::new(-hw, -hd * 0.3), Vec2::X),
    ]
}

/// How tall a building of this role is drawn, in tiles.
fn height_of(role: Role) -> f32 {
    match role {
        Role::Wall => WALL_HEIGHT,
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

/// The middle of a footprint, in tiles.
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
    let (lo, hi) = ground_range(ground, s);
    // The box drapes over the terrain, each corner on the ground below it:
    // sunk by the slope under it, so its top follows the ground at `height`.
    BoxInstance {
        pos: centre_of(s.tile, s.width, s.depth).to_array(),
        facing: [0.0, 1.0],
        size: [f32::from(s.width), height + (hi - lo), f32::from(s.depth)],
        color,
        lift: lo - hi,
    }
}

/// Whether `side` is shown a building: in fog of war, enemy buildings show
/// once seen; with `reveal_all`, everything shows.
pub fn shown_to(side: PlayerId, reveal_all: bool) -> impl Fn(&StructureView) -> bool + Copy {
    move |s| reveal_all || s.seen_by(side)
}

/// Draw every standing building that `shown` allows: a box in its owner's
/// colour, dimmer as its hit points fall, and a bar over it once it is
/// damaged. `across` is the way the camera looks along the ground, so the
/// bars face it.
#[allow(clippy::too_many_arguments)]
pub fn push(
    world: &World,
    table: &Table,
    ground: impl Fn(f32, f32) -> f32,
    team: impl Fn(PlayerId) -> [u8; 3],
    shown: impl Fn(&StructureView) -> bool,
    modelled: impl Fn(&StructureView) -> Option<f32>,
    across: Vec2,
    out: &mut Vec<BoxInstance>,
) {
    let Some(structures) = world.structures() else {
        return;
    };
    let right = Vec2::new(-across.y, across.x);
    for s in structures.iter().filter(|s| shown(s)) {
        if out.len() + 3 > MAX_BOX_INSTANCES {
            return;
        }
        let Some(stats) = table.stats.get(usize::from(s.building)) else {
            continue;
        };
        // A building with a model is drawn with the soldiers (its bar
        // floats over the model's roof).
        let height = modelled(&s).unwrap_or_else(|| {
            let [r, g, b] = shade(team(s.owner), s.hp, s.max_hp);
            let height = height_of(stats.role);
            out.push(block(&s, height, [r, g, b, 0], &ground));
            height
        });
        if s.hp >= s.max_hp {
            continue;
        }
        let (lo, hi) = ground_range(&ground, &s);
        let len = f32::from(s.width.max(s.depth)).max(BAR_MIN);
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
            color: bar_colour(frac),
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
        let left = n.left as f32 / n.full.max(1) as f32;
        let width = NODE_WIDTH * (NODE_LEAST + (100.0 - NODE_LEAST) * left) / 100.0;
        let glow = if n.left == 0 { 0 } else { NODE_GLOW };
        out.push(BoxInstance {
            pos: crate::from_fx_vec(n.at).to_array(),
            facing: [0.0, 1.0],
            size: [width, NODE_HEIGHT, width],
            color: [r, g, b, glow],
            lift: 0.0,
        });
    }
}

/// Root-veins lie as a flat ribbon this thick (tiles), in segments this
/// long so each follows the ground it crosses, a hair above it.
const VEIN_THICK: f32 = 0.06;
const VEIN_SEGMENT: f32 = 3.0;
const VEIN_LIFT: f32 = 0.05;
/// The links between buildings, the spokes round each, and the flow home
/// from a node, as widths in tiles.
const VEIN_LINK_WIDTH: f32 = 0.9;
const VEIN_SPOKE_WIDTH: (f32, f32) = (0.7, 0.15);
const VEIN_FLOW_WIDTH: f32 = 0.5;
/// Spokes a veins building sends out (a main building one more), ending
/// this share of its reach out.
const VEIN_SPOKES: usize = 5;
const VEIN_SPOKE_REACH: f32 = 0.85;
/// Veins are this flesh-violet, a quarter of the way to their owner's
/// colour; the flow of Grub is lighter.
const VEIN_COLOR: [u8; 3] = [84, 30, 78];
const VEIN_FLOW_COLOR: [u8; 3] = [200, 80, 170];
const VEIN_GLOW: u8 = 45;
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
    let n = ((len / VEIN_SEGMENT).ceil() as usize).max(1);
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
                VEIN_THICK,
                len / n as f32 + 0.1,
            ],
            color: [r, g, b, glow(i)],
            lift,
        });
    }
}

/// Draw the Brood's root-veins (Conquest): from each linked veins building
/// that `side` is shown, a ribbon along the ground to the building it
/// links through (if that is shown too) and spokes out to the edge of its
/// veins. Where `side`'s burrowed workers work a node on the veins, a
/// brighter ribbon pulses from the node to the building that carries the
/// load home; a node off the veins gets a dull stub and a red cross, so
/// the break shows. `reveal_all` shows everything, every side's workers
/// too.
pub fn push_veins(
    world: &World,
    side: PlayerId,
    reveal_all: bool,
    team: impl Fn(PlayerId) -> [u8; 3],
    out: &mut Vec<BoxInstance>,
) {
    let (Some(structures), Some(economy)) = (world.structures(), world.economy()) else {
        return;
    };
    let shown = shown_to(side, reveal_all);
    // Only the veins whose building is shown.
    let veins = world.veins().filter_map(|v| {
        let s = structures.get(v.building).filter(|s| shown(s))?;
        Some((v, s))
    });
    for (v, s) in veins {
        if out.len() >= MAX_BOX_INSTANCES {
            return;
        }
        let colour = tint(VEIN_COLOR, team(v.owner), 25);
        let at = crate::from_fx_vec(v.at);
        let from = crate::from_fx_vec(v.from);
        // The link: a thicker ribbon between buildings, when both are
        // shown (a link must not give away a building in the fog).
        if structures.get(v.parent).is_some_and(|p| shown(&p)) {
            push_ribbon(
                at,
                from,
                (VEIN_LINK_WIDTH, VEIN_LINK_WIDTH),
                colour,
                VEIN_LIFT,
                |_| VEIN_GLOW,
                out,
            );
        }
        // Spokes at angles of their own: spread evenly, turned by where
        // the building is, so neighbours do not mirror each other.
        let spokes = VEIN_SPOKES + usize::from(v.at == v.from);
        let turn = (s.handle.index.wrapping_mul(2_654_435_761) >> 8) as f32 * 1e-3;
        let start = f32::from(s.width.max(s.depth)) / 2.0;
        let length = start + v.reach as f32 * VEIN_SPOKE_REACH;
        for k in 0..spokes {
            let wobble = ((k as f32 * 7.31 + turn).sin()) * 0.25;
            let angle = turn + (k as f32 + wobble) * std::f32::consts::TAU / spokes as f32;
            let end = at + Vec2::new(angle.cos(), angle.sin()) * length;
            push_ribbon(
                at,
                end,
                VEIN_SPOKE_WIDTH,
                colour,
                VEIN_LIFT - 0.01,
                |_| VEIN_GLOW,
                out,
            );
        }
    }

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
                VEIN_LIFT + 0.01,
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
                VEIN_LIFT + 0.01,
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
                lift: NODE_HEIGHT + 0.1,
            });
        }
    }
}

/// A tuning beam: a thin, glowing crystal-blue ribbon from a tuning
/// building to each node it works, in tiles.
const BEAM_WIDTH: f32 = 0.16;
const BEAM_COLOR: [u8; 3] = [140, 210, 255];
const BEAM_GLOW: u8 = 150;
const BEAM_LIFT: f32 = VEIN_LIFT + 0.02;
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

/// The ghost of a building about to be placed with its north-west tile at
/// `tile`: its full size in the side's colour, or red where it won't go.
pub fn push_ghost(
    stats: &BuildingStats,
    tile: (i32, i32),
    fits: bool,
    colour: [u8; 3],
    ground: impl Fn(f32, f32) -> f32,
    out: &mut Vec<BoxInstance>,
) {
    let view = StructureView {
        handle: StructureHandle {
            index: 0,
            generation: 0,
        },
        owner: 0,
        building: 0,
        tile,
        width: stats.width,
        depth: stats.depth,
        hp: 1,
        max_hp: 1,
        built_percent: 100,
        upgrading: None,
        growing: false,
        seen: u8::MAX,
    };
    let [r, g, b] = colour;
    let colour = if fits { [r, g, b, GHOST_GLOW] } else { REFUSED };
    out.push(block(&view, height_of(stats.role), colour, &ground));
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
/// near as whole tiles allow.
pub fn tile_for(at: Vec2, width: u8, depth: u8) -> (i32, i32) {
    (
        (at.x - f32::from(width) / 2.0).round() as i32,
        (at.y - f32::from(depth) / 2.0).round() as i32,
    )
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            dressed, still,
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
        let id = |name: &str| {
            (0..table.len() as BuildingId)
                .find(|&b| table.name(b) == name)
                .unwrap()
        };
        let (hall, keep) = (id("Clan Hall"), id("Castle Keep"));
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
            hp: 10,
            max_hp: 100,
            built_percent: 30,
            upgrading: None,
            growing: false,
            seen: 1,
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
        });
        table.conquer(&mut world);
        let farm = table
            .stats
            .iter()
            .position(|s| s.role == Role::Housing)
            .unwrap();
        let farm = farm as BuildingId;
        assert_eq!(world.can_build(farm, (8, 8)), Ok(()));
        assert!(world.can_build(farm, (-1, 8)).is_err());
        assert!(world.can_build(farm, (tiles as i32 - 1, 8)).is_err());
        // Cell (20, 6) is tile (10, 3).
        assert!(world.can_build(farm, (9, 2)).is_err());
        // What the check allows, the simulation accepts, and then the
        // spot is taken.
        let place = |world: &mut World, tile: (i32, i32)| {
            let command = gc_sim::Command::SpawnStructure {
                building: farm,
                tile: (tile.0 as i16, tile.1 as i16),
            };
            world
                .tick(&[gc_sim::PlayerCommand { player: 0, command }])
                .commands_applied
                == 1
        };
        assert!(place(&mut world, (8, 8)));
        assert_eq!(world.structures().unwrap().len(), 1);
        assert!(world.can_build(farm, (9, 9)).is_err());
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
        let id = |name: &str| {
            (0..table.len() as BuildingId)
                .find(|&b| table.name(b) == name)
                .unwrap()
        };
        for f in Faction::ALL {
            let menu: Vec<BuildingId> = table.buildable(f).collect();
            assert!(
                !menu.is_empty() && menu.len() <= 12,
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
                .any(|b| b == id("Farmstead"))
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
}
