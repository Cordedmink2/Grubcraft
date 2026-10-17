//! Conquest matches from the lobby (docs/06-game-modes.md): the maps, the
//! unit table the simulation plays them with, the [`ConquestSetup`] every
//! machine makes alike from the lobby's settings and the baked data, and
//! the opening the host sends: each side's main building and workers at its
//! start.

use gc_data::conquest::{
    Class, ConquestUnitRecord, NONE, UNIT_BUILDS_ONLY, UNIT_BURROWS, UNIT_CONSUMED, UNIT_HATCHES,
};
use gc_data::{Faction, GameData};
use gc_fixed::{Angle, Fx, FxVec2};
use gc_map::{Kind, Map};
use gc_replay::ConquestRecord;
use gc_sim::economy::{SEEP_GRUB, VEIN_MARROW};
use gc_sim::structures::Breaker;
use gc_sim::{
    Command, ConquestSetup, ConquestUnit, MAX_PARTIES, NodeSetup, PlayerCommand, PlayerId,
    Resource, Victories, World,
};
use gc_ui::Settings;
use gc_ui::lobby::{
    COLORS, CONQUEST_MAP_PLAYERS, GameMode, LobbySettings, MatchStart, Slot, SlotKind, Tempo,
    Victory,
};
use glam::Vec2;

use crate::buildings::{self, Table};

/// How many tiles a side the six-player Wilds Conquest maps are: the
/// Standard Field size for six, several minutes' march across.
pub const WILDS_TILES: i32 = 768;

/// The Conquest map the lobby's choice `choice` names, made from `seed`
/// (`gc_ui::lobby::conquest_map_seed`). The shipped maps come first, in
/// the order the lobby lists them (`gc_ui::lobby::CONQUEST_MAPS`), then
/// the Wilds maps: as wild as the seed picks, then plains, mixed and wild
/// land. A choice past the end (from another build's lobby or replay)
/// gets the last; a replay still plays on the map it stored.
pub fn make_map(choice: usize, seed: u64) -> Map {
    let shipped = gc_map::SHIPPED.len();
    let lands = gc_map::wilds::Wildness::ALL;
    let choice = choice.min(shipped + lands.len());
    let Some(land) = choice.checked_sub(shipped) else {
        let def = gc_map::shipped(gc_map::SHIPPED[choice].0)
            // The shipped maps parse: gc_map's tests check every one.
            .unwrap_or_else(|| panic!("no shipped Conquest map {choice}"));
        return Map::from_def(&def);
    };
    let wildness = match land.checked_sub(1) {
        None => lands[(seed % lands.len() as u64) as usize],
        Some(k) => lands[k],
    };
    let def = gc_map::conquest::describe_wilds(WILDS_TILES, seed, wildness)
        .expect("WILDS_TILES is a size describe_wilds makes");
    Map::from_def(&def)
}

/// Each of `players` players' start on `map`, in player order. With every
/// start taken, player `p` has start `p`. A six-start Wilds map with fewer
/// players spreads them round it: its starts run round the map 0 to 5
/// (north-west, north, north-east, then their turned copies south-east,
/// south, south-west), and the players take every other one where they
/// can, so two start across the map, three in a triangle and four in the
/// corners.
pub fn player_starts(map: &Map, players: usize) -> Vec<(i32, i32)> {
    const SPREAD: [&[usize]; 6] = [
        &[0],
        &[0, 3],
        &[0, 2, 4],
        &[0, 2, 3, 5],
        &[0, 2, 3, 5, 1],
        &[],
    ];
    let starts = map.starts();
    if starts.len() != SPREAD.len() || players == 0 || players >= starts.len() {
        return starts;
    }
    SPREAD[players - 1].iter().map(|&i| starts[i]).collect()
}

/// The lobby's Conquest map choice for a shipped map's file name.
pub fn choice_of(file: &str) -> Option<usize> {
    gc_map::SHIPPED.iter().position(|&(name, _)| name == file)
}

/// What a faction power record is to the simulation.
pub fn power_of(p: &gc_data::PowerRecord) -> gc_sim::PowerStats {
    use gc_data::conquest::{POWER_HASTE, POWER_SHIFT};
    gc_sim::PowerStats {
        tier: p.tier,
        cost: p.cost,
        cooldown_ticks: p.cooldown_ticks,
        duration_ticks: p.duration_ticks,
        effect: match p.effect {
            POWER_HASTE => gc_sim::PowerEffect::Haste { percent: p.percent },
            POWER_SHIFT => gc_sim::PowerEffect::Shift { to: p.to_building },
            _ => gc_sim::PowerEffect::NotBuilt,
        },
    }
}

/// What a Conquest unit record is to the simulation.
pub fn unit_of(u: &ConquestUnitRecord) -> ConquestUnit {
    ConquestUnit {
        breaker: match u.class() {
            Class::Siege => Breaker::Siege,
            Class::Artillery => Breaker::Artillery,
            _ => Breaker::Regiment,
        },
        worker: u.worker(),
        supply: u.supply,
        cost_grub: u.cost_grub,
        cost_marrow: u.cost_marrow,
        train_ticks: u.train_ticks,
        trained_at: (u.trained_at != NONE).then_some(u.trained_at),
        tier: u.tier,
        hatches: u.flags & UNIT_HATCHES != 0,
        consumed: u.flags & UNIT_CONSUMED != 0,
        burrows: u.worker() && u.flags & UNIT_BURROWS != 0,
        builds_only: u.worker() && u.flags & UNIT_BUILDS_ONLY != 0,
    }
}

/// The battle stats of every Conquest unit type, by unit type: the unit
/// table a Conquest world is made with.
/// The Conquest unit table's battle stats, as a Conquest world is made
/// with them.
#[cfg(test)]
pub(crate) fn unit_stats(data: &GameData) -> Vec<gc_sim::UnitStats> {
    data.conquest_units
        .iter()
        .map(|u| gc_match::stats_of(&u.stats))
        .collect()
}

/// Grub and Marrow each faction starts with (docs/04-economy.md,
/// "Starting state").
pub fn start_stores(f: Faction) -> (u32, u32) {
    match f {
        Faction::CarrionConcord => (400, 0),
        _ => (300, 0),
    }
}

/// Each lobby player's faction, in player order.
pub fn factions(start: &MatchStart) -> Vec<Faction> {
    start.players.iter().map(|s| s.faction).collect()
}

/// `--conquest MAP`: Conquest on the lobby's Conquest map `choice`, you at
/// the first start and an idle side at each of the others, the factions
/// in turn from yours so a four-player map shows all four.
pub fn quick_conquest(choice: usize, settings: &Settings, map_seed: u64) -> MatchStart {
    let players = CONQUEST_MAP_PLAYERS.get(choice).copied().unwrap_or(2);
    let slots = (0..players)
        .map(|i| {
            let mut slot = Slot::open(i);
            slot.kind = if i == 0 {
                SlotKind::Human
            } else {
                SlotKind::Ai
            };
            slot.name = if i == 0 {
                settings.player_name.clone()
            } else {
                format!("AI {}", i + 1)
            };
            slot.faction = Faction::ALL[(settings.faction.index() + i) % Faction::ALL.len()];
            slot.color = (settings.player_color + i as u8) % COLORS;
            slot.team = i as u8;
            slot
        })
        .collect();
    MatchStart {
        settings: LobbySettings {
            mode: GameMode::Conquest,
            map: choice,
            map_seed,
            ..LobbySettings::default()
        },
        players: slots,
        local_player: 0,
        seed: None,
    }
}

/// What a Conquest world is made with beyond its battle setup, made alike
/// on every machine from the lobby's settings, the map and the baked data:
/// the world takes its [`ConquestRecord::setup`], and the recording keeps
/// it. `factions` are the players', in player order.
pub fn plan(
    table: &Table,
    data: &GameData,
    map: &Map,
    factions: &[Faction],
    tempo: Tempo,
    victories: Victories,
) -> ConquestRecord {
    let mut start = [(0, 0); MAX_PARTIES];
    for (s, &f) in start.iter_mut().zip(factions) {
        *s = start_stores(f);
    }
    ConquestRecord {
        buildings: table.stats.clone(),
        units: data.conquest_units.iter().map(unit_of).collect(),
        structures: ConquestSetup::default().structures,
        nodes: nodes_of(map),
        start,
        gather_percent: tempo.gather_percent(),
        build_percent: tempo.build_percent(),
        victories,
        heart: map
            .objects_of(Kind::Heart)
            .next()
            .map(|o| tile_centre(o.at)),
        powers: data.powers.iter().map(power_of).collect(),
    }
}

/// What the simulation plays a lobby's Victory setting for.
pub fn victories(v: Victory) -> Victories {
    Victories {
        annihilation: v.annihilation(),
        heart: v.heart(),
    }
}

/// The middle of tile `at`.
fn tile_centre((x, y): (i32, i32)) -> FxVec2 {
    FxVec2::from_ints(x, y) + FxVec2::new(Fx::HALF, Fx::HALF)
}

/// The map's Grub seeps and Marrow veins, in the map's order, each at its
/// tile's centre.
pub fn nodes_of(map: &Map) -> Vec<NodeSetup> {
    map.objects
        .iter()
        .filter_map(|o| {
            let (resource, amount) = match o.kind {
                Kind::GrubSeep => (Resource::Grub, SEEP_GRUB),
                Kind::MarrowVein => (Resource::Marrow, VEIN_MARROW),
                _ => return None,
            };
            Some(NodeSetup {
                resource,
                at: tile_centre(o.at),
                amount,
            })
        })
        .collect()
}

/// A footprint, `[x, y, width, depth]` in tiles.
pub(crate) type Foot = [i32; 4];

/// Whether `p` is within `margin` tiles of footprint `f`.
fn near(f: Foot, p: (f32, f32), margin: f32) -> bool {
    let [x, y, w, d] = f.map(|v| v as f32);
    p.0 > x - margin && p.0 < x + w + margin && p.1 > y - margin && p.1 < y + d + margin
}

fn overlaps(a: Foot, b: Foot, gap: i32) -> bool {
    a[0] < b[0] + b[2] + gap
        && b[0] < a[0] + a[2] + gap
        && a[1] < b[1] + b[3] + gap
        && b[1] < a[1] + a[3] + gap
}

/// The commands that put every side's starting buildings and workers on
/// the field: each faction's buildings and units with a start count, round
/// its start on `map` (player `p` at its start from [`player_starts`]). A
/// building that tunes nodes (the Choir's Resonators) goes where it
/// reaches the nearest node of its kind no other starting one tunes. The host sends
/// them in the match's first turn, as it does a Field Battle's armies;
/// `world` is the match's fresh world, to check the ground with. Works in
/// floats, which is safe because only the host does it.
pub fn opening(
    data: &GameData,
    table: &Table,
    map: &Map,
    world: &World,
    factions: &[Faction],
) -> Vec<PlayerCommand> {
    let starts = player_starts(map, factions.len());
    let mut out = Vec::new();
    let mut taken: Vec<Foot> = Vec::new();
    let mut tuned: Vec<(i32, i32)> = Vec::new();
    for (p, (&faction, &(sx, sy))) in factions.iter().zip(&starts).enumerate() {
        let player = p as PlayerId;
        let centre = (sx as f32 + 0.5, sy as f32 + 0.5);
        let order = |command| PlayerCommand { player, command };
        // The main building on the start, then the rest round it, nearest
        // first.
        let mut first: Option<Foot> = None;
        let mut kinds: Vec<usize> = (0..data.buildings.len())
            .filter(|&i| {
                let b = &data.buildings[i];
                b.faction() == faction && b.start_count > 0
            })
            .collect();
        kinds.sort_by_key(|&i| table.stats[i].role != gc_sim::structures::Role::Main);
        for i in kinds {
            let s = table.stats[i];
            let (w, d) = (i32::from(s.width), i32::from(s.depth));
            for _ in 0..data.buildings[i].start_count {
                let at =
                    |(x, y): (f32, f32)| buildings::tile_for(Vec2::new(x, y), s.width, s.depth);
                let fits = |t: (i32, i32)| {
                    let foot = [t.0, t.1, w, d];
                    !taken.iter().any(|&f| overlaps(f, foot, 1))
                        && world.can_build(i as u16, t).is_ok()
                };
                let tuning = s.tunes.zip(first).and_then(|((resource, tiles), main)| {
                    let tile = tuning_spot(map, main, (resource, tiles), (w, d), &tuned, &fits)?;
                    // Every node it reaches is taken, not just the one it
                    // was sent for.
                    let kind = node_kind(resource);
                    tuned.extend((map.objects.iter()).filter_map(|o| {
                        (o.kind == kind && tunes_node([tile.0, tile.1, w, d], tiles, o.at))
                            .then_some(o.at)
                    }));
                    Some(tile)
                });
                let tile = match (tuning, first) {
                    (Some(tile), _) => Some(tile),
                    (None, None) => Some(at(centre)).filter(|&t| fits(t)),
                    (None, Some(main)) => ring(main, 2).find(|&t| fits(t)),
                }
                .or_else(|| ring([sx, sy, 1, 1], 2).find(|&t| fits(t)));
                let Some(tile) = tile else {
                    continue;
                };
                let foot = [tile.0, tile.1, w, d];
                taken.push(foot);
                first.get_or_insert(foot);
                out.push(order(Command::SpawnStructure {
                    building: i as u16,
                    tile: (tile.0 as i16, tile.1 as i16),
                }));
            }
        }
        // Workers and the rest beside the main building, on the side
        // towards the nearest node, a little apart.
        let main = first.unwrap_or([sx, sy, 1, 1]);
        let mid = (
            main[0] as f32 + main[2] as f32 / 2.0,
            main[1] as f32 + main[3] as f32 / 2.0,
        );
        let toward = map
            .objects
            .iter()
            .filter(|o| matches!(o.kind, Kind::GrubSeep))
            .map(|o| (o.at.0 as f32 + 0.5 - mid.0, o.at.1 as f32 + 0.5 - mid.1))
            .min_by(|a, b| (a.0.hypot(a.1)).total_cmp(&b.0.hypot(b.1)))
            .map_or(0.0, |(dx, dy)| dy.atan2(dx));
        let radius = main[2].max(main[3]) as f32 / 2.0 + 3.0;
        let mut spots: Vec<(f32, f32)> = Vec::new();
        for (u, rec) in data.conquest_units.iter().enumerate() {
            if rec.faction() != faction {
                continue;
            }
            for _ in 0..rec.start_count {
                let spot = (0..64).find_map(|k| {
                    // Out from the side towards the node, alternating left
                    // and right of it, a ring further out each lap.
                    let lap = (k / 16) as f32;
                    let step = k % 16;
                    let turn = if step % 2 == 0 {
                        step / 2
                    } else {
                        -(step + 1) / 2
                    };
                    let angle = toward + turn as f32 * std::f32::consts::TAU / 16.0;
                    let r = radius + 2.5 * lap;
                    let p = (mid.0 + r * angle.cos(), mid.1 + r * angle.sin());
                    let fx = crate::fx_vec(Vec2::new(p.0, p.1));
                    let clear = world.nav().walkable(fx)
                        && !taken.iter().any(|&f| near(f, p, 1.5))
                        && spots.iter().all(|s| (s.0 - p.0).hypot(s.1 - p.1) >= 2.0);
                    clear.then_some(p)
                });
                let Some(p) = spot else {
                    continue;
                };
                spots.push(p);
                out.push(order(Command::SpawnRegiment {
                    unit: u as u16,
                    position: crate::fx_vec(Vec2::new(p.0, p.1)),
                    facing: Angle::of_vec(crate::fx_vec(Vec2::from_angle(toward))),
                }));
            }
        }
    }
    out
}

/// How far from its main building a starting building looks for a node to
/// tune, in tiles.
const TUNE_SEARCH_TILES: i32 = 24;

/// For a starting building `w` by `d` tiles that tunes `resource` nodes
/// within `tiles` of its edge: where it goes to tune the node of that kind
/// nearest main building `main` that `tuned` doesn't hold and that is
/// within [`TUNE_SEARCH_TILES`]: the spot nearest the main building of
/// those that `fits`. Worked in half tiles, in whole numbers.
fn tuning_spot(
    map: &Map,
    main: Foot,
    (resource, tiles): (Resource, u8),
    (w, d): (i32, i32),
    tuned: &[(i32, i32)],
    fits: &impl Fn((i32, i32)) -> bool,
) -> Option<(i32, i32)> {
    let kind = node_kind(resource);
    let sq = |x: i32, y: i32| i64::from(x) * i64::from(x) + i64::from(y) * i64::from(y);
    // Main building's centre, and distances from it, in half tiles.
    let mid = (2 * main[0] + main[2], 2 * main[1] + main[3]);
    let from_main = |(x, y): (i32, i32)| sq(x - mid.0, y - mid.1);
    let mut nodes: Vec<(i32, i32)> = (map.objects.iter())
        .filter(|o| o.kind == kind && !tuned.contains(&o.at))
        .map(|o| o.at)
        .filter(|&(x, y)| from_main((2 * x + 1, 2 * y + 1)) <= sq(2 * TUNE_SEARCH_TILES, 0))
        .collect();
    nodes.sort_by_key(|&(x, y)| (from_main((2 * x + 1, 2 * y + 1)), (x, y)));
    let reach = i32::from(tiles);
    nodes.into_iter().find_map(|(nx, ny)| {
        // Every corner whose footprint comes within reach of the node,
        // nearest the main building first.
        (nx - reach - w..=nx + reach + 1)
            .flat_map(|x| (ny - reach - d..=ny + reach + 1).map(move |y| (x, y)))
            .filter(|&(x, y)| tunes_node([x, y, w, d], tiles, (nx, ny)) && fits((x, y)))
            .min_by_key(|&(x, y)| (from_main((2 * x + w, 2 * y + d)), (x, y)))
    })
}

/// The map object for nodes of `resource`.
fn node_kind(resource: Resource) -> Kind {
    match resource {
        Resource::Grub => Kind::GrubSeep,
        Resource::Marrow => Kind::MarrowVein,
    }
}

/// Whether a building on footprint `foot` that tunes nodes within `tiles`
/// of its edge reaches the node on tile `node`: the sim's test, worked in
/// half tiles (a node sits at its tile's centre).
fn tunes_node([x, y, w, d]: Foot, tiles: u8, (nx, ny): (i32, i32)) -> bool {
    let point = (2 * nx + 1, 2 * ny + 1);
    let gap_x = i64::from(point.0.clamp(2 * x, 2 * (x + w)) - point.0);
    let gap_y = i64::from(point.1.clamp(2 * y, 2 * (y + d)) - point.1);
    let reach = 2 * i64::from(tiles);
    gap_x * gap_x + gap_y * gap_y <= reach * reach
}

/// Tiles round footprint `f` for a building's north-west corner, a ring
/// further out each lap, starting `gap` tiles off: candidates for the
/// opening's extra buildings.
pub(crate) fn ring(f: Foot, gap: i32) -> impl Iterator<Item = (i32, i32)> {
    let [x, y, w, d] = f;
    (0..8).flat_map(move |lap| {
        let r = gap + lap * 2;
        let (x0, y0, x1, y1) = (x - r - 2, y - r - 2, x + w + r, y + d + r);
        (x0..=x1)
            .flat_map(move |tx| [(tx, y0), (tx, y1)])
            .chain((y0 + 1..y1).flat_map(move |ty| [(x0, ty), (x1, ty)]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_sim::{Capacities, MatchSetup};
    use gc_ui::lobby::SHIPPED_CONQUEST_MAPS as SHIPPED_CONQUEST;

    fn data() -> GameData<'static> {
        crate::data::load()
    }

    /// A fresh Conquest world on `map` for `factions`, and its plan.
    fn world_on(map: &Map, factions: &[Faction], tempo: Tempo) -> (World, ConquestRecord, Table) {
        world_for(map, factions, tempo, Victories::default())
    }

    /// The same, played for `victories` (any at all means fog of war).
    fn world_for(
        map: &Map,
        factions: &[Faction],
        tempo: Tempo,
        victories: Victories,
    ) -> (World, ConquestRecord, Table) {
        let data = data();
        let table = Table::new(&data);
        let stats = unit_stats(&data);
        let nav = map.nav_costs();
        let mut world = World::new(&MatchSetup {
            seed: 11,
            capacities: Capacities::default(),
            units: &stats,
            map_tiles: map.tiles,
            nav: &nav,
            teams: &[],
            ground: map.ground(),
            champions: false,
        });
        let plan = plan(&table, &data, map, factions, tempo, victories);
        assert!(world.conquest(&plan.setup()));
        (world, plan, table)
    }

    /// On every Conquest map a Brood player's opening Hive reaches its base's
    /// two seeps and Marrow vein with its root-veins, so Gnawers burrowed
    /// there bring their loads home before any Pustule is grown.
    #[test]
    fn a_brood_hives_veins_reach_its_base_nodes() {
        let data = data();
        // Every shipped map, and a Wilds map of every kind of land.
        let maps = gc_map::SHIPPED.len() + gc_map::wilds::Wildness::ALL.len();
        for choice in 0..=maps {
            let map = make_map(choice, 7);
            let starts = map.starts();
            let factions = vec![Faction::Brood; starts.len()];
            let (mut world, _, table) = world_on(&map, &factions, Tempo::Standard);
            let opening = opening(&data, &table, &map, &world, &factions);
            world.tick(&opening);
            for (p, &(x, y)) in starts.iter().enumerate() {
                let start = gc_fixed::FxVec2::from_ints(x, y);
                let mut nodes: Vec<_> = world.economy().unwrap().nodes().collect();
                nodes.sort_by_key(|n| (n.at - start).length_sq_raw());
                for n in &nodes[..3] {
                    assert!(
                        world.on_veins(p as PlayerId, n.at),
                        "map {choice}, player {p}: {n:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_conquest_unit_converts_and_trains_at_its_own_factions_building() {
        let data = data();
        for f in Faction::ALL {
            let workers = data
                .conquest_units
                .iter()
                .filter(|u| u.faction() == f && u.worker())
                .count();
            assert!(workers >= 1, "{f:?} has a worker type");
        }
        for u in data.conquest_units {
            let c = unit_of(u);
            if let Some(b) = c.trained_at {
                let b = &data.buildings[usize::from(b)];
                assert_eq!(
                    b.faction(),
                    u.faction(),
                    "{} trains at {}",
                    u.name(),
                    b.name()
                );
            }
            assert_eq!(c.worker, u.worker());
        }
        assert_eq!(unit_stats(&data).len(), data.conquest_units.len());
    }

    /// The lobby's Conquest maps are gc_map's shipped maps, in its order,
    /// then one Wilds map for each choice of land.
    #[test]
    fn the_lobby_lists_the_shipped_maps_then_the_wilds() {
        use gc_ui::lobby::{CONQUEST_MAP_PLAYERS, CONQUEST_MAPS};
        assert_eq!(gc_map::SHIPPED.len(), SHIPPED_CONQUEST);
        for (i, &(file, _)) in gc_map::SHIPPED.iter().enumerate() {
            assert_eq!(gc_map::shipped(file).unwrap().name, CONQUEST_MAPS[i]);
            assert_eq!(make_map(i, 0).players as usize, CONQUEST_MAP_PLAYERS[i]);
            assert_eq!(choice_of(file), Some(i));
        }
        let lands = gc_map::wilds::Wildness::ALL;
        assert_eq!(CONQUEST_MAPS[SHIPPED_CONQUEST], "Land: Random");
        for (k, land) in lands.iter().enumerate() {
            let name = format!("Land: {}", land.name());
            assert_eq!(CONQUEST_MAPS[SHIPPED_CONQUEST + 1 + k], name);
        }
        assert_eq!(CONQUEST_MAPS.len(), SHIPPED_CONQUEST + 1 + lands.len());
        assert_eq!(CONQUEST_MAP_PLAYERS.len(), CONQUEST_MAPS.len());
        // A Wilds map is the big six-player Conquest map, and a choice past
        // the end gets the last.
        let wild = make_map(CONQUEST_MAPS.len() - 1, 5);
        assert_eq!(wild.players, 6);
        assert_eq!(wild.tiles, WILDS_TILES as u32);
        // The Standard size for six, as the lobby sizes Field Battles.
        let standard = gc_ui::lobby::field_map_tiles(gc_ui::lobby::MapSize::Standard, 6);
        assert_eq!(WILDS_TILES as u32, standard);
        assert!(wild.is_conquest());
        assert_eq!(make_map(99, 5).hash(), wild.hash(), "never panics");
    }

    /// Fewer players than a Wilds map's six starts are spread round it;
    /// a full map, or a map with fewer starts, keeps the map's order.
    #[test]
    fn players_are_spread_round_a_six_start_map() {
        let wild = make_map(SHIPPED_CONQUEST, 9);
        let starts = wild.starts();
        assert_eq!(player_starts(&wild, 6), starts);
        assert_eq!(player_starts(&wild, 2), vec![starts[0], starts[3]]);
        assert_eq!(
            player_starts(&wild, 3),
            vec![starts[0], starts[2], starts[4]]
        );
        assert_eq!(
            player_starts(&wild, 4),
            vec![starts[0], starts[2], starts[3], starts[5]]
        );
        let teeth = make_map(1, 0);
        assert_eq!(player_starts(&teeth, 2), teeth.starts());
    }

    #[test]
    fn ribcage_pass_has_two_starts_each_with_its_seeps_and_vein() {
        let map = make_map(0, 0);
        assert_eq!(map.players, 2);
        assert_eq!(map.starts().len(), 2);
        assert_eq!(make_map(1, 0).starts().len(), 4);
        let nodes = nodes_of(&map);
        for (sx, sy) in map.starts() {
            let near = |r: Resource| {
                nodes
                    .iter()
                    .filter(|n| n.resource == r)
                    .filter(|n| {
                        let (x, y) = (
                            crate::fx_to_f32(n.at.x) - sx as f32,
                            crate::fx_to_f32(n.at.y) - sy as f32,
                        );
                        x.hypot(y) < 20.0
                    })
                    .count()
            };
            assert!(near(Resource::Grub) >= 2, "two seeps by ({sx}, {sy})");
            assert!(near(Resource::Marrow) >= 1, "a vein by ({sx}, {sy})");
        }
    }

    #[test]
    fn every_faction_opens_with_its_hall_workers_and_stores() {
        let data = data();
        let map = make_map(1, 0);
        let factions = Faction::ALL;
        let (mut world, _, table) = world_on(&map, &factions, Tempo::Standard);
        let opening = opening(&data, &table, &map, &world, &factions);
        let stats = world.tick(&opening);
        assert_eq!(
            stats.commands_applied as usize,
            opening.len(),
            "every command applies"
        );
        for (p, &f) in factions.iter().enumerate() {
            let p = p as PlayerId;
            let s = world.structures().unwrap();
            let mains = s
                .iter()
                .filter(|v| {
                    v.owner == p
                        && table.stats[usize::from(v.building)].role
                            == gc_sim::structures::Role::Main
                })
                .count();
            assert_eq!(mains, 1, "{f:?} has its main building");
            let buildings: usize = data
                .buildings
                .iter()
                .filter(|b| b.faction() == f)
                .map(|b| usize::from(b.start_count))
                .sum();
            assert_eq!(
                s.iter().filter(|v| v.owner == p).count(),
                buildings,
                "{f:?}"
            );
            let units: usize = data
                .conquest_units
                .iter()
                .filter(|u| u.faction() == f)
                .map(|u| usize::from(u.start_count))
                .sum();
            assert_eq!(
                world.regiments.iter().filter(|r| r.owner == p).count(),
                units,
                "{f:?}"
            );
            let bank = world.bank(p).unwrap();
            assert_eq!((bank.grub, bank.marrow), start_stores(f), "{f:?}");
            assert!(bank.supply_cap > 0, "{f:?}'s hall gives supply");
        }
    }

    #[test]
    fn workers_stand_idle_until_they_gather() {
        let data = data();
        let map = make_map(0, 0);
        let factions = [Faction::EmberClans, Faction::CarrionConcord];
        let (mut world, _, table) = world_on(&map, &factions, Tempo::Standard);
        let opening = opening(&data, &table, &map, &world, &factions);
        world.tick(&opening);
        let idle: Vec<_> = world.idle_workers(0).collect();
        let workers = data
            .conquest_units
            .iter()
            .filter(|u| u.faction() == Faction::EmberClans && u.worker())
            .map(|u| usize::from(u.start_count))
            .sum();
        assert_eq!(idle.len(), workers);
        let node = world
            .economy()
            .unwrap()
            .nodes()
            .find(|n| n.resource == Resource::Grub)
            .unwrap()
            .index;
        world.tick(&[PlayerCommand {
            player: 0,
            command: Command::Gather {
                regiment: idle[0],
                node,
            },
        }]);
        let now: Vec<_> = world.idle_workers(0).collect();
        assert_eq!(now.len(), workers - 1);
        assert!(!now.contains(&idle[0]));
    }

    /// `--conquest four_teeth` seats four sides, one of each faction, as
    /// a lobby would.
    #[test]
    fn a_quick_conquest_start_is_a_lobby_conquest_start() {
        let start = quick_conquest(1, &Settings::default(), 7);
        assert_eq!(start.settings.mode, GameMode::Conquest);
        assert_eq!(start.settings.map_name(), "The Four Teeth");
        assert_eq!(start.players.len(), 4);
        assert!(start.players.len() <= start.settings.max_players());
        let mut factions = factions(&start);
        factions.sort_by_key(|&f| f as u8);
        assert_eq!(factions, Faction::ALL);
    }

    #[test]
    fn tempo_sets_the_rates() {
        let map = make_map(0, 0);
        let (_, plan, _) = world_on(&map, &[Faction::EmberClans, Faction::Brood], Tempo::Swift);
        let s = plan.setup();
        assert_eq!((s.gather_percent, s.build_percent), (150, 75));
        assert_eq!(s.nodes.len(), nodes_of(&map).len());
        assert!(s.nodes.len() >= 6);
    }

    #[test]
    fn every_conquest_map_has_its_heart_in_the_middle() {
        // The shipped maps and one Wilds map (each Wilds map is made the
        // same way).
        for choice in 0..=SHIPPED_CONQUEST {
            let map = make_map(choice, 3);
            let both = Victories::BOTH;
            let data = data();
            let plan = plan(&Table::new(&data), &data, &map, &[], Tempo::Epic, both);
            let at = plan.heart.expect("a Heart");
            let middle = map.tiles as i32 / 2;
            assert!((at.x.floor_int() - middle).abs() <= 2, "{at:?}");
            assert!((at.y.floor_int() - middle).abs() <= 2, "{at:?}");
            assert_eq!(plan.victories, both);
        }
    }

    /// The Brood's root-veins are drawn for the buildings a side is shown
    /// (all of them with `reveal_all`), with flow along a worked node on
    /// the veins and a cut-off mark on one off them.
    #[test]
    fn root_veins_are_drawn_for_the_buildings_a_side_is_shown() {
        let data = data();
        let map = make_map(0, 0);
        let factions = [Faction::Brood, Faction::Brood];
        let (mut world, _, table) = world_on(&map, &factions, Tempo::Standard);
        let opening = opening(&data, &table, &map, &world, &factions);
        world.tick(&opening);
        let team = |p: PlayerId| if p == 0 { [200, 50, 50] } else { [50, 50, 200] };
        let draw = |world: &World, side: PlayerId, reveal_all: bool| {
            let mut out = Vec::new();
            buildings::push_veins(world, side, reveal_all, team, &mut out);
            out
        };
        // Boxes lying within `r` tiles of `at`.
        let near = |out: &[gc_render::BoxInstance], at: gc_fixed::FxVec2, r: f32| {
            let at = crate::from_fx_vec(at);
            out.iter()
                .filter(|b| (glam::Vec2::from(b.pos) - at).length() <= r)
                .count()
        };

        let hive = |p: PlayerId| world.veins().find(|v| v.owner == p).expect("a Hive");
        let (mine, theirs) = (hive(0), hive(1));
        let seen = |p: PlayerId| {
            let s = world.structures().unwrap().get(hive(p).building).unwrap();
            s.seen_by(0)
        };
        assert!(seen(0), "you see your own Hive");
        let plain = draw(&world, 0, false);
        assert!(near(&plain, mine.at, 2.0) >= 6, "spokes leave your Hive");
        // Ribbons stay flat on the ground and within the Hive's reach.
        let far = (mine.reach + 8) as f32;
        assert!(plain.iter().all(|b| b.size[1] < 0.2 && b.lift < 0.5));
        assert_eq!(
            near(&plain, mine.at, far),
            plain.len() - near(&plain, theirs.at, far)
        );
        // The enemy's veins show only once its Hive has been seen.
        let all = draw(&world, 0, true);
        assert!(all.len() > near(&all, mine.at, far));
        if !seen(1) {
            assert_eq!(near(&plain, theirs.at, 2.0), 0, "no veins in the fog");
            assert!(all.len() > plain.len());
        }

        // A Gnawer burrowed at a seep on the veins sends flow home (none
        // while it walks there); at one off them, the seep is marked cut
        // off.
        let gnawer = world
            .regiments
            .iter()
            .find(|r| r.owner == 0 && world.burrows(r.handle))
            .expect("a Gnawer")
            .handle;
        let seeps: Vec<_> = world
            .economy()
            .unwrap()
            .nodes()
            .filter(|n| n.resource == Resource::Grub)
            .collect();
        let on = seeps.iter().find(|n| world.on_veins(0, n.at)).unwrap();
        let off = seeps.iter().find(|n| !world.on_veins(0, n.at)).unwrap();
        let work = |world: &mut World, node: u16| {
            world.tick(&[PlayerCommand {
                player: 0,
                command: Command::Gather {
                    regiment: gnawer,
                    node,
                },
            }]);
            assert_eq!(world.gathers_at(gnawer), Some(node));
            // Flow shows only once it is burrowed in at the node.
            for _ in 0..120 * gc_sim::TICKS_PER_SECOND {
                if world.burrowed_at(gnawer) == Some(node) {
                    return;
                }
                world.tick(&[]);
            }
            panic!("the Gnawer never reached node {node}");
        };
        work(&mut world, on.index);
        let flowing = draw(&world, 0, false);
        assert!(flowing.len() > plain.len());
        assert!(near(&flowing, on.at, 2.0) > near(&plain, on.at, 2.0));
        work(&mut world, off.index);
        let cut = draw(&world, 0, false);
        let marks = |out: &[gc_render::BoxInstance]| {
            out.iter().filter(|b| b.size == [0.25, 0.1, 1.5]).count()
        };
        assert_eq!(marks(&plain) + marks(&flowing), 0);
        assert_eq!(marks(&cut), 2, "a cross on the cut-off seep");
        assert!(near(&cut, off.at, 0.1) >= 2);
    }

    /// A Choir opening tunes the two nearest seeps with its two starting
    /// Resonators, and a beam is drawn to each from the Resonator, for the
    /// buildings a side is shown (all with `reveal_all`).
    #[test]
    fn tuning_beams_are_drawn_for_the_resonators_a_side_is_shown() {
        let data = data();
        let map = make_map(0, 0);
        let factions = [Faction::Choir, Faction::Choir];
        let (mut world, _, table) = world_for(&map, &factions, Tempo::Standard, Victories::BOTH);
        let opening = opening(&data, &table, &map, &world, &factions);
        world.tick(&opening);
        world.tick(&[]);
        let draw = |side: PlayerId, reveal_all: bool| {
            let mut out = Vec::new();
            buildings::push_tuning(&world, side, reveal_all, &mut out);
            out
        };
        // Boxes lying within `r` tiles of `at`.
        let near = |out: &[gc_render::BoxInstance], at: gc_fixed::FxVec2, r: f32| {
            let at = crate::from_fx_vec(at);
            out.iter()
                .filter(|b| (glam::Vec2::from(b.pos) - at).length() <= r)
                .count()
        };
        let structures = world.structures().unwrap();
        // Each tuned node, with who owns its tuner and whether `0` has
        // seen it.
        let tuned: Vec<_> = world
            .economy()
            .unwrap()
            .nodes()
            .filter_map(|n| {
                let s = structures.get(n.tuner?)?;
                Some((n.at, s.owner, s.seen_by(0)))
            })
            .collect();
        for p in 0..2 {
            let mine = tuned.iter().filter(|t| t.1 == p).count();
            assert_eq!(mine, 2, "player {p}'s two Resonators tune two seeps");
        }

        // A beam ends on each node you tune; the enemy's only with
        // reveal_all.
        let plain = draw(0, false);
        let all = draw(0, true);
        // The first piece of a beam's last segment ends at the node.
        let beam = |out: &[gc_render::BoxInstance], at| near(out, at, 1.6);
        for &(at, owner, _) in &tuned {
            assert!(beam(&all, at) >= 1, "a beam to every node with reveal_all");
            assert_eq!(beam(&plain, at) >= 1, owner == 0, "player {owner}'s beam");
        }
        assert!(
            tuned.iter().any(|t| t.1 == 0 && t.2),
            "you see your own Resonators"
        );
        // Thin, glowing, flat on the ground.
        assert!(
            all.iter()
                .all(|b| b.size[0] < 0.3 && b.size[1] < 0.2 && b.lift < 0.5 && b.color[3] > 100)
        );
        assert!(
            tuned.iter().any(|t| t.1 == 1 && !t.2),
            "the enemy's Resonators start in the fog"
        );
        assert!(all.len() > plain.len(), "the enemy's beams hide");
    }

    /// Fields of power show as rings for your own buildings (everyone's
    /// with `reveal_all`), brighter while a building that needs power is
    /// being placed.
    #[test]
    fn power_fields_are_drawn_as_rings_round_their_buildings() {
        let data = data();
        let map = make_map(0, 0);
        let factions = [Faction::Choir, Faction::Choir];
        let (mut world, _, table) = world_on(&map, &factions, Tempo::Standard);
        let opening = opening(&data, &table, &map, &world, &factions);
        world.tick(&opening);
        let draw = |side: PlayerId, reveal_all: bool, lit: bool| {
            let mut out = Vec::new();
            buildings::push_fields(&world, side, reveal_all, lit, &mut out);
            out
        };
        let fields: Vec<_> = world.power_fields().collect();
        assert!(fields.iter().any(|f| f.owner == 0) && fields.iter().any(|f| f.owner == 1));
        let plain = draw(0, false, false);
        let all = draw(0, true, false);
        let lit = draw(0, false, true);
        assert!(!plain.is_empty() && all.len() > plain.len());
        assert_eq!(lit.len(), plain.len());
        // Only your own buildings' rings, each within its field's reach.
        let around = |out: &[gc_render::BoxInstance], owner: PlayerId| {
            out.iter()
                .filter(|b| {
                    fields.iter().filter(|f| f.owner == owner).any(|f| {
                        let [x, y, w, d] = f.foot.map(|v| v as f32);
                        let at = glam::Vec2::new(x + w / 2.0, y + d / 2.0);
                        (glam::Vec2::from(b.pos) - at).length() <= f.reach as f32 + 6.0
                    })
                })
                .count()
        };
        assert_eq!(around(&plain, 0), plain.len());
        assert_eq!(around(&plain, 1), 0);
        assert_eq!(around(&all, 1), all.len() - plain.len());
        // Faint, flat and thin, and brighter and wider when lit.
        assert!(plain.iter().all(|b| b.size[1] < 0.2 && b.color[3] < 50));
        assert!(lit.iter().all(|b| b.color[3] > 100));
        assert!(lit[0].size[0] > plain[0].size[0]);
    }

    /// How far point `p` lies from the segment `a` to `b`.
    fn off_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
        let d = b - a;
        let t = ((p - a).dot(d) / d.length_squared()).clamp(0.0, 1.0);
        (p - (a + d * t)).length()
    }

    fn resonator_id(table: &Table) -> gc_sim::BuildingId {
        (0..table.len() as gc_sim::BuildingId)
            .find(|&b| table.name(b) == "Resonator")
            .unwrap()
    }

    /// Put finished Resonators of `owner`'s centred on `at` (by the sandbox
    /// spawn command) and let the world settle.
    fn spawn_resonators(world: &mut World, table: &Table, owner: PlayerId, at: &[Vec2]) {
        let building = resonator_id(table);
        let spawns: Vec<_> = at
            .iter()
            .map(|&c| {
                let tile = buildings::tile_for(c, 2, 2);
                PlayerCommand {
                    player: owner,
                    command: Command::SpawnStructure {
                        building,
                        tile: (tile.0 as i16, tile.1 as i16),
                    },
                }
            })
            .collect();
        assert_eq!(world.tick(&spawns).commands_applied, at.len() as u32);
        world.tick(&[]);
        world.tick(&[]);
    }

    /// The seeps `owner`'s buildings tune, as points on the ground.
    fn tuned_seeps(world: &World, owner: PlayerId) -> Vec<Vec2> {
        let all = world.structures().unwrap();
        (world.economy().unwrap().nodes())
            .filter(|n| {
                n.tuner
                    .and_then(|h| all.get(h))
                    .is_some_and(|s| s.owner == owner)
            })
            .map(|n| crate::from_fx_vec(n.at))
            .collect()
    }

    /// Three spots for Resonators of `owner`'s that make a chord round one
    /// of the seeps they tune: the corners of a triangle 5 to 8 tiles out
    /// from it, all free to build on.
    fn chord_spots(world: &World, table: &Table, owner: PlayerId) -> Option<[Vec2; 3]> {
        let id = resonator_id(table);
        let free = |c: Vec2| {
            let t = buildings::tile_for(c, 2, 2);
            world.can_build(id, t).is_ok()
        };
        for seep in tuned_seeps(world, owner) {
            for radius in [7.0f32, 6.0, 5.0, 8.0] {
                for turn in 0..24 {
                    let start = turn as f32 * std::f32::consts::TAU / 72.0;
                    let corners = [0.0f32, 1.0, 2.0].map(|k| {
                        let a = start + k * std::f32::consts::TAU / 3.0;
                        seep + Vec2::new(a.cos(), a.sin()) * radius
                    });
                    if corners.iter().all(|&c| free(c)) {
                        return Some(corners);
                    }
                }
            }
        }
        None
    }

    /// Three spots for Resonators of `owner`'s that link with each other
    /// (10 to 12 tiles apart, in a proper triangle) but with none of its
    /// buildings that link already (at least 15 tiles from each).
    fn lonely_spots(world: &World, table: &Table, owner: PlayerId) -> Option<[Vec2; 3]> {
        let id = resonator_id(table);
        let all = world.structures().unwrap();
        let linkers: Vec<Vec2> = all
            .iter()
            .filter(|s| s.owner == owner && table.stats[usize::from(s.building)].links > 0)
            .map(|s| buildings::centre_of(s.tile, s.width, s.depth))
            .collect();
        let from = *linkers.first()?;
        let (x0, y0) = (from.x as i32, from.y as i32);
        let spots: Vec<Vec2> = (y0 - 45..=y0 + 45)
            .flat_map(|y| (x0 - 45..=x0 + 45).map(move |x| (x, y)))
            .filter(|&t| world.can_build(id, t).is_ok())
            .map(|t| buildings::centre_of(t, 2, 2))
            .filter(|c| linkers.iter().all(|l| (*l - *c).length() >= 15.0))
            .collect();
        let apart = |p: Vec2, q: Vec2| (10.0..=12.0).contains(&(p - q).length());
        for &a in &spots {
            for &b in spots.iter().filter(|&&b| apart(a, b)) {
                let third = spots
                    .iter()
                    .find(|&&c| apart(a, c) && apart(b, c) && (b - a).perp_dot(c - a).abs() > 40.0);
                if let Some(&c) = third {
                    return Some([a, b, c]);
                }
            }
        }
        None
    }

    /// A two-player Choir world, its opening laid and settled.
    fn opened_choir_world() -> (World, Table) {
        let data = data();
        let map = make_map(0, 0);
        let factions = [Faction::Choir, Faction::Choir];
        let (mut world, _, table) = world_for(&map, &factions, Tempo::Standard, Victories::BOTH);
        let opening = opening(&data, &table, &map, &world, &factions);
        world.tick(&opening);
        world.tick(&[]);
        (world, table)
    }

    /// Whether a drawn piece is a chord's edge, not a plain link.
    fn chord_piece(b: &gc_render::BoxInstance) -> bool {
        b.size[0] > 0.2
    }

    /// A Choir world, opened, where each of `owners` has three more
    /// Resonators round one of its seeps. Returns the corners each got.
    fn chord_world(owners: &[PlayerId]) -> (World, Table, Vec<[Vec2; 3]>) {
        let (mut world, table) = opened_choir_world();
        assert!(world.chords().is_empty(), "no chords in the opening");
        let mut spots = Vec::new();
        for &owner in owners {
            let at = chord_spots(&world, &table, owner).expect("room for a chord");
            spawn_resonators(&mut world, &table, owner, &at);
            spots.push(at);
        }
        (world, table, spots)
    }

    /// Chords are drawn along their triangle's edges for the side that has
    /// them (every side's with `reveal_all`, never an enemy's from the fog),
    /// as glowing ribbons a little wider and brighter than tuning beams.
    #[test]
    fn chords_are_drawn_for_your_own_side_and_not_an_enemys_without_reveal_all() {
        let (world, _, spots) = chord_world(&[0, 1]);
        let own = |o: PlayerId| world.chords().iter().filter(move |c| c.owner == o);
        assert!(own(0).count() >= 1 && own(1).count() >= 1);
        // The chords' edges (links that are no chord's edge are drawn too,
        // thinner).
        let draw = |side: PlayerId, reveal_all: bool| {
            let mut out = Vec::new();
            buildings::push_chords(&world, side, reveal_all, &mut out);
            out.retain(chord_piece);
            out
        };
        // Every chord edge of `owner`'s, as a pair of points.
        let edges = |o: PlayerId| {
            own(o)
                .flat_map(|c| {
                    let at = c.at.map(crate::from_fx_vec);
                    [(at[0], at[1]), (at[1], at[2]), (at[2], at[0])]
                })
                .collect::<Vec<_>>()
        };
        let on_edges = |out: &[gc_render::BoxInstance], o: PlayerId| {
            let edges = edges(o);
            out.iter()
                .filter(|b| {
                    let p = Vec2::from(b.pos);
                    edges.iter().any(|&(x, y)| off_segment(p, x, y) < 0.05)
                })
                .count()
        };
        let plain = draw(0, false);
        let all = draw(0, true);
        let theirs = draw(1, false);
        assert!(!plain.is_empty() && !theirs.is_empty());
        // Yours only, unless everything is shown.
        assert_eq!(on_edges(&plain, 0), plain.len());
        assert_eq!(on_edges(&plain, 1), 0);
        assert_eq!(on_edges(&theirs, 1), theirs.len());
        assert_eq!(all.len(), plain.len() + theirs.len());
        assert_eq!(on_edges(&all, 1), theirs.len());
        // The pieces lie along the triangle: every edge of the one we built
        // has a piece within half a segment of its middle, and they run
        // along it.
        let [a, b, c] = spots[0];
        for (x, y) in [(a, b), (b, c), (c, a)] {
            let mid = (x + y) / 2.0;
            let near = |b: &&gc_render::BoxInstance| (Vec2::from(b.pos) - mid).length() <= 1.6;
            let piece = plain.iter().find(near).expect("a piece mid-edge");
            let along = (y - x).normalize();
            assert!(Vec2::from(piece.facing).dot(along).abs() > 0.99);
        }
        // Wider and brighter than a tuning beam, still flat on the ground.
        assert!(
            all.iter()
                .all(|b| b.size[0] > 0.2 && b.size[1] < 0.2 && b.lift < 0.5 && b.color[3] > 160)
        );
        // The sandbox's own side sees nothing of a world with no chords.
        let mut none = Vec::new();
        let map = make_map(0, 0);
        let factions = [Faction::Choir, Faction::Choir];
        let (empty, _, _) = world_for(&map, &factions, Tempo::Standard, Victories::BOTH);
        buildings::push_chords(&empty, 0, true, &mut none);
        assert!(none.is_empty());
    }

    /// An edge two chords share is drawn once.
    #[test]
    fn an_edge_two_chords_share_is_drawn_once() {
        let (mut world, table, spots) = chord_world(&[0]);
        // A fourth Resonator on the far side of the first edge links with
        // its two ends, making a second chord on it.
        let [a, b, c] = spots[0];
        let d = a + b - c;
        spawn_resonators(&mut world, &table, 0, &[d]);
        let chords: Vec<_> = world.chords().iter().filter(|c| c.owner == 0).collect();
        assert!(chords.len() >= 2, "{} chords", chords.len());
        let mut edges: Vec<(u32, u32, f32)> = Vec::new();
        for c in &chords {
            for k in 0..3 {
                let (i, j) = (c.corners[k].index, c.corners[(k + 1) % 3].index);
                let len =
                    (crate::from_fx_vec(c.at[k]) - crate::from_fx_vec(c.at[(k + 1) % 3])).length();
                let key = (i.min(j), i.max(j), len);
                if !edges.iter().any(|e| (e.0, e.1) == (key.0, key.1)) {
                    edges.push(key);
                }
            }
        }
        assert!(edges.len() < 3 * chords.len(), "chords share an edge");
        // As many pieces as the distinct edges need, three tiles each.
        let pieces: usize = edges.iter().map(|e| (e.2 / 3.0).ceil() as usize).sum();
        let mut out = Vec::new();
        buildings::push_chords(&world, 0, false, &mut out);
        out.retain(chord_piece);
        assert_eq!(out.len(), pieces);
    }

    /// Links that are not part of a chord are thin, faint lines between
    /// your Resonators, which a chord then replaces.
    #[test]
    fn links_are_drawn_between_resonators_that_are_not_a_chord() {
        let (mut world, table) = opened_choir_world();
        let [a, b, c] = lonely_spots(&world, &table, 0).expect("room away from the rest");
        let links = |world: &World, side: PlayerId| {
            let mut out = Vec::new();
            buildings::push_chords(world, side, false, &mut out);
            out.retain(|b| !chord_piece(b));
            out
        };
        let on = |out: &[gc_render::BoxInstance], x: Vec2, y: Vec2| {
            out.iter()
                .filter(|b| off_segment(Vec2::from(b.pos), x, y) < 0.05)
                .count()
        };
        let before = links(&world, 0);
        // Two Resonators 12 tiles or so apart link, and are no chord.
        spawn_resonators(&mut world, &table, 0, &[a, b]);
        assert!(world.chords().is_empty());
        let pair = links(&world, 0);
        assert!(on(&pair, a, b) >= 1, "a link between them");
        assert!(pair.len() > before.len());
        assert!(pair.iter().all(|p| p.size[0] < 0.12 && p.color[3] < 100));
        // Yours only.
        assert_eq!(links(&world, 1).len(), 0);
        // The third makes a chord of the three, so no thin link is left on
        // its edges (they are drawn as the chord).
        spawn_resonators(&mut world, &table, 0, &[c]);
        assert!(!world.chords().is_empty());
        let chord = links(&world, 0);
        for (x, y) in [(a, b), (b, c), (c, a)] {
            assert_eq!(on(&chord, x, y), 0, "{x} to {y}");
        }
    }

    /// Chords charge the Resonance meter the top bar shows, for their
    /// owner only.
    #[test]
    fn resonance_charges_in_the_bank_for_the_side_with_chords() {
        let (mut world, _, _) = chord_world(&[0]);
        let mine = world.chords().iter().filter(|c| c.owner == 0).count() as u32;
        let bank = world.bank(0).unwrap();
        assert_eq!((bank.chords, bank.resonance), (mine, 0));
        for _ in 0..gc_sim::TICKS_PER_SECOND * 7 {
            world.tick(&[]);
        }
        let bank = world.bank(0).unwrap();
        assert!(bank.resonance >= 2, "{}", bank.resonance);
        assert_eq!(bank.chords, mine);
        let enemy = world.bank(1).unwrap();
        assert_eq!((enemy.chords, enemy.resonance), (0, 0));
    }

    /// A Resonator inside a chord says so on its card: its tuned seeps
    /// yield more, by the best of their harmonies.
    #[test]
    fn a_resonators_card_carries_its_chords_harmony() {
        let (mut world, table) = opened_choir_world();
        let tuner = |world: &World| {
            let all = world.structures().unwrap();
            all.iter()
                .find(|s| s.owner == 0 && s.finished() && table.name(s.building) == "Resonator")
                .unwrap()
        };
        let card = |world: &World| crate::hud::tuning_of(world, &table, &tuner(world)).unwrap();
        let before = card(&world);
        assert!(before.nodes >= 1);
        assert_eq!(before.harmony, 0);
        let at = chord_spots(&world, &table, 0).expect("room for a chord");
        spawn_resonators(&mut world, &table, 0, &at);
        let after = card(&world);
        assert!(
            (10..=60).contains(&after.harmony),
            "harmony {}",
            after.harmony
        );
        let best = (world.economy().unwrap().nodes())
            .filter(|n| n.tuner == Some(tuner(&world).handle))
            .map(|n| n.harmony)
            .max();
        assert_eq!(best, Some(after.harmony));
        assert!(after.per_minute > before.per_minute);
    }

    /// The baker and the simulation cap power times alike.
    #[test]
    fn power_times_have_one_limit() {
        assert_eq!(
            gc_data::conquest::MAX_POWER_TICKS,
            gc_sim::powers::MAX_POWER_TICKS
        );
    }

    /// A building a power has hasted glows cyan for the side that may see
    /// it, and only while it is hasted.
    #[test]
    fn hasted_buildings_are_ringed_for_those_who_can_see_them() {
        let (mut world, table, _) = chord_world(&[0]);
        let overclock = data().powers.iter().position(|p| p.name() == "Overclock");
        let power = overclock.expect("the Choir's Overclock") as u8;
        let ring = |world: &World, shown: bool| {
            let mut out = Vec::new();
            buildings::push_haste(world, |_| shown, 200, &mut out);
            out
        };
        assert!(ring(&world, true).is_empty(), "nothing hasted yet");
        // Chords charge the Resonance it costs.
        let mut ticks = 0;
        while world.bank(0).unwrap().resonance < 20 {
            world.tick(&[]);
            ticks += 1;
            assert!(ticks < 5 * 60 * gc_sim::TICKS_PER_SECOND, "never charged");
        }
        let all = world.structures().unwrap();
        let resonator = all
            .iter()
            .find(|s| s.owner == 0 && s.finished() && table.name(s.building) == "Resonator");
        let target = resonator.expect("an opening Resonator").handle;
        world.tick(&[PlayerCommand {
            player: 0,
            command: Command::UsePower {
                power,
                target: gc_sim::PowerTarget::Structure(target),
            },
        }]);
        assert!(world.structure_haste(target) > 0);
        let lit = ring(&world, true);
        assert!(!lit.is_empty());
        // Thin, flat, glowing and cyan; a building the viewer can't see
        // isn't ringed.
        assert!(lit.iter().all(|b| b.size[1] < 0.2 && b.color[3] == 200));
        assert!(lit.iter().all(|b| b.color[2] > b.color[0]));
        assert!(ring(&world, false).is_empty());
    }
}
