//! Conquest matches from the lobby (docs/06-game-modes.md): the maps, the
//! unit table the simulation plays them with, the [`ConquestSetup`] every
//! machine makes alike from the lobby's settings and the baked data, and
//! the opening the host sends: each side's main building and workers at its
//! start.

use gc_data::conquest::{Class, ConquestUnitRecord, NONE};
use gc_data::{Faction, GameData};
use gc_fixed::{Angle, Fx, FxVec2};
use gc_map::{Kind, Map};
use gc_replay::ConquestRecord;
use gc_sim::economy::{SEEP_GRUB, VEIN_MARROW};
use gc_sim::structures::Breaker;
use gc_sim::{
    Command, ConquestSetup, ConquestUnit, MAX_PARTIES, NodeSetup, PlayerCommand, PlayerId,
    Resource, World,
};
use gc_ui::Settings;
use gc_ui::lobby::{
    COLORS, CONQUEST_MAP_PLAYERS, Faction as LobbyFaction, GameMode, LobbySettings, MatchStart,
    Slot, SlotKind, Tempo,
};
use glam::Vec2;

use crate::buildings::{self, Table};

/// The Conquest map the lobby's choice `choice` names: gc_map ships them
/// in the order the lobby lists them (`gc_ui::lobby::CONQUEST_MAPS`). A
/// choice past the end (from another build's lobby or replay) gets the
/// last; a replay still plays on the map it stored.
pub fn make_map(choice: usize) -> Map {
    let last = gc_map::SHIPPED.len() - 1;
    let def = gc_map::SHIPPED
        .get(choice.min(last))
        .and_then(|&(name, _)| gc_map::shipped(name))
        // The shipped maps parse: gc_map's tests check every one.
        .unwrap_or_else(|| panic!("no shipped Conquest map {choice}"));
    Map::from_def(&def)
}

/// The lobby's Conquest map choice for a shipped map's file name.
pub fn choice_of(file: &str) -> Option<usize> {
    gc_map::SHIPPED.iter().position(|&(name, _)| name == file)
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
        .map(|u| crate::sandbox::stats_of(&u.stats))
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
    // gc_data and the lobby list the factions in the same order.
    start
        .players
        .iter()
        .map(|s| Faction::ALL[s.faction.index()])
        .collect()
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
            slot.faction =
                LobbyFaction::ALL[(settings.faction.index() + i) % LobbyFaction::ALL.len()];
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
    }
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
            let (x, y) = o.at;
            Some(NodeSetup {
                resource,
                at: FxVec2::from_ints(x, y) + FxVec2::new(Fx::HALF, Fx::HALF),
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
/// its start on `map` (player `p` at the map's start `p`). The host sends
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
    let starts = map.starts();
    let mut out = Vec::new();
    let mut taken: Vec<Foot> = Vec::new();
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
                let tile = match first {
                    None => Some(at(centre)).filter(|&t| fits(t)),
                    Some(main) => ring(main, 2).find(|&t| fits(t)),
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

    fn data() -> GameData<'static> {
        crate::data::load()
    }

    /// A fresh Conquest world on `map` for `factions`, and its plan.
    fn world_on(map: &Map, factions: &[Faction], tempo: Tempo) -> (World, ConquestRecord, Table) {
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
        let plan = plan(&table, &data, map, factions, tempo);
        assert!(world.conquest(&plan.setup()));
        (world, plan, table)
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

    /// The lobby's Conquest maps are gc_map's shipped maps, in its order.
    #[test]
    fn the_lobby_lists_the_shipped_maps() {
        use gc_ui::lobby::{CONQUEST_MAP_PLAYERS, CONQUEST_MAPS};
        assert_eq!(gc_map::SHIPPED.len(), CONQUEST_MAPS.len());
        assert_eq!(make_map(99).hash(), make_map(1).hash(), "never panics");
        for (i, &(file, _)) in gc_map::SHIPPED.iter().enumerate() {
            assert_eq!(gc_map::shipped(file).unwrap().name, CONQUEST_MAPS[i]);
            assert_eq!(make_map(i).players as usize, CONQUEST_MAP_PLAYERS[i]);
            assert_eq!(choice_of(file), Some(i));
        }
    }

    #[test]
    fn ribcage_pass_has_two_starts_each_with_its_seeps_and_vein() {
        let map = make_map(0);
        assert_eq!(map.players, 2);
        assert_eq!(map.starts().len(), 2);
        assert_eq!(make_map(1).starts().len(), 4);
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
        let map = make_map(1);
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
        let map = make_map(0);
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
        let map = make_map(0);
        let (_, plan, _) = world_on(&map, &[Faction::EmberClans, Faction::Brood], Tempo::Swift);
        let s = plan.setup();
        assert_eq!((s.gather_percent, s.build_percent), (150, 75));
        assert_eq!(s.nodes.len(), nodes_of(&map).len());
        assert!(s.nodes.len() >= 6);
    }
}
