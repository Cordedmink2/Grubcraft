//! Buildings in the sandbox (Conquest, docs/21-buildings.md): the baked
//! table as the simulation reads it, and how the field draws, places and
//! picks them. Each building is a placeholder box until models arrive.

use gc_data::conquest::{self as record, BuildingRecord};
use gc_data::{Faction, GameData};
use gc_render::{BoxInstance, MAX_BOX_INSTANCES};
use gc_sim::structures::{Armour, Role, TowerAttack};
use gc_sim::{
    BuildingId, BuildingStats, ConquestSetup, ConquestUnit, PlayerId, Refusal, Resource,
    StructureHandle, StructureView, World,
};
use gc_ui::conquest::node_color;
use glam::{Vec2, Vec3};

// The baked flag bits mean the same to the simulation.
const _: () = assert!(
    record::BUILDING_DROP_OFF_GRUB == gc_sim::structures::DROP_OFF_GRUB
        && record::BUILDING_DROP_OFF_MARROW == gc_sim::structures::DROP_OFF_MARROW
        && record::BUILDING_POWERS_FIELD == gc_sim::structures::POWERS_FIELD
        && record::BUILDING_ANTI_AIR == gc_sim::structures::ANTI_AIR
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

/// Draw every standing building: a box in its owner's colour, dimmer as
/// its hit points fall, and a bar over it once it is damaged. `across`
/// is the way the camera looks along the ground, so the bars face it.
pub fn push(
    world: &World,
    table: &Table,
    ground: impl Fn(f32, f32) -> f32,
    team: impl Fn(PlayerId) -> [u8; 3],
    across: Vec2,
    out: &mut Vec<BoxInstance>,
) {
    let Some(structures) = world.structures() else {
        return;
    };
    let right = Vec2::new(-across.y, across.x);
    for s in structures.iter() {
        if out.len() + 3 > MAX_BOX_INSTANCES {
            return;
        }
        let Some(stats) = table.stats.get(usize::from(s.building)) else {
            continue;
        };
        let height = height_of(stats.role);
        let [r, g, b] = shade(team(s.owner), s.hp, s.max_hp);
        out.push(block(&s, height, [r, g, b, 0], &ground));
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
fn ray_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
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
        let units: Vec<gc_sim::UnitStats> =
            data.units.iter().map(crate::sandbox::stats_of).collect();
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
