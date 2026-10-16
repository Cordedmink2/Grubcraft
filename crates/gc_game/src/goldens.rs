//! Records the golden replays in `replays/` (see `gc_replay`): headless
//! Field Battles between computer players, on the game's generated map
//! with the real unit data, the way the lobby would set them up.
//!
//! CI never runs this; it plays the stored replays back. Record again only
//! to add a golden or to replace one whose battle no longer makes sense:
//!
//! ```text
//! cargo test --release -p gc_game record_golden_replays -- --ignored --nocapture
//! GOLDEN=field-1v1 cargo test ...   # just that one
//! ```
//!
//! `AI_TRACE=1` prints what each computer player is doing every 20
//! seconds; `ENGAGED_TRACE=1` prints how many soldiers are in melee every
//! 5 seconds.
//!
//! When a simulation change only moves the hashes, bless the existing
//! replays instead (`cargo run --release -p gc_replay -- bless replays`):
//! that keeps the recorded commands.

use gc_ai::army::{self, Placement};
use gc_ai::{Difficulty, Faction, Mind, MindConfig, Minds};
use gc_fixed::{Angle, Fx, FxVec2};
use gc_replay::Recorder;
use gc_sim::champions::Outcome;
use gc_sim::deploy::DEPLOY_SECONDS;
use gc_sim::{
    Capacities, Command, Formation, MatchSetup, PlayerCommand, PlayerId, UnitStats, World,
};
use glam::Vec2;

use crate::battle::{battle_starts, zone_order};
use crate::terrain_gen::{Heightmap, start_positions};

/// One side of a recorded battle: a computer player.
struct Side {
    faction: Faction,
    level: Difficulty,
    team: u8,
}

struct Golden {
    name: &'static str,
    seed: u64,
    map_tiles: u32,
    sides: &'static [Side],
    /// Soldiers each army is raised to (at least), or zero for the
    /// lobby's standard army of [`army::ARMY_REGIMENTS`] regiments.
    soldiers: u32,
    /// Rows of regiments each army stands in: big armies are set out as
    /// this many standard deployments one behind the other.
    rows: usize,
    minutes: u32,
    /// Whether computer players command the armies; if not, `extra` does.
    minds: bool,
    /// Commands added to the computer players' at some ticks.
    extra: fn(&World, &mut Vec<PlayerCommand>),
    /// Whether champions may duel before battle, as the lobby allows by
    /// default in battles with computer players.
    champions: bool,
    /// Whether the battle opens with a deployment phase, as when people
    /// play: every side gets a zone, side 0 deploys as a person would
    /// (placing through `extra`, never pressing Ready, so the time runs
    /// out; its computer player takes over once the battle begins) and the
    /// other computer players are ready at once.
    deployment: bool,
}

const fn side(faction: Faction, level: Difficulty, team: u8) -> Side {
    Side {
        faction,
        level,
        team,
    }
}

fn no_extra(_: &World, _: &mut Vec<PlayerCommand>) {}

/// Every second each regiment that isn't already fighting is ordered
/// to attack the nearest enemy: four players who only ever charge, so
/// nearly every soldier ends up in the fight.
fn charge(world: &World, out: &mut Vec<PlayerCommand>) {
    if world.tick_count() % 20 != 1 {
        return;
    }
    for r in world.regiments.iter() {
        if r.engaged || r.morale.broken() {
            continue;
        }
        let distance = |e: &gc_sim::RegimentView| {
            let (dx, dy) = (
                i64::from((e.position.x - r.position.x).raw()),
                i64::from((e.position.y - r.position.y).raw()),
            );
            dx * dx + dy * dy
        };
        let nearest = world
            .regiments
            .iter()
            .filter(|e| world.hostile(r.owner, e.owner) && !e.morale.broken())
            .min_by_key(distance);
        if let Some(e) = nearest {
            out.push(PlayerCommand {
                player: r.owner,
                command: Command::Attack {
                    regiment: r.handle,
                    target: e.handle,
                },
            });
        }
    }
}

/// Half a minute in, each side changes one regiment's formation (Ember to
/// Tight, Concord to Loose); a minute in, someone in the sandbox makes
/// Ember spearmen hit harder; at a minute and a half, they take the first
/// regiment off the field.
fn tuning(world: &World, out: &mut Vec<PlayerCommand>) {
    match world.tick_count() {
        600 => {
            for (player, formation) in [(0, Formation::Tight), (1, Formation::Loose)] {
                if let Some(r) = world.regiments.iter().find(|r| r.owner == player) {
                    out.push(PlayerCommand {
                        player,
                        command: Command::SetFormation {
                            regiment: r.handle,
                            formation,
                        },
                    });
                }
            }
        }
        1200 => {
            let mut stats = world.units()[0];
            stats.melee_attack += 15;
            stats.melee_damage += 1;
            out.push(PlayerCommand {
                player: 0,
                command: Command::TuneUnit { unit: 0, stats },
            });
        }
        1800 => {
            if let Some(r) = world.regiments.iter().next() {
                out.push(PlayerCommand {
                    player: 0,
                    command: Command::RemoveRegiment { regiment: r.handle },
                });
            }
        }
        _ => {}
    }
}

/// Side 0 deploys by hand (see [`Golden::deployment`]), with orders the
/// phase accepts and some it refuses, and gives one paced move once the
/// battle begins.
fn deploying(world: &World, out: &mut Vec<PlayerCommand>) {
    let mine: Vec<_> = world.regiments.iter().filter(|r| r.owner == 0).collect();
    let enemy = world.regiments.iter().find(|r| r.owner == 1);
    let order = |command| PlayerCommand { player: 0, command };
    let (Some(&first), Some(enemy)) = (mine.first(), enemy) else {
        return;
    };
    let tick = world.tick_count();
    if tick == BATTLE_BEGINS + 20 {
        // The battle has begun: a slow, wide advance.
        out.push(order(Command::MoveTo {
            regiment: first.handle,
            target: first.position + FxVec2::from_ints(0, -20),
            facing: None,
            files: 10,
            pace: Fx::from_int(2),
        }));
        return;
    }
    let (Some(zone), [_, second, third, fourth, ..]) = (world.deploy_zone(0), mine.as_slice())
    else {
        return;
    };
    let inside = zone.center + FxVec2::from_ints(6, 4);
    let outside = zone.center + FxVec2::new(zone.radius * 2, Fx::ZERO);
    let plain_move = |regiment, target| Command::MoveTo {
        regiment,
        target,
        facing: None,
        files: 0,
        pace: Fx::ZERO,
    };
    // What each batch should do is in DEPLOY_ORDERS; `record` checks it.
    match tick {
        // Placing: one regiment moved and turned within the zone; one sent
        // outside it and an enemy regiment, both refused.
        20 => out.extend([
            order(Command::MoveTo {
                regiment: first.handle,
                target: inside,
                facing: Some(Angle(0x2000)),
                files: 6,
                pace: Fx::ZERO,
            }),
            order(plain_move(second.handle, outside)),
            order(plain_move(enemy.handle, inside)),
        ]),
        21 => assert_eq!(first.position, inside, "placed inside the zone"),
        // Formations: close order taken; a shield wall without shields and
        // an attack before the battle begins, both refused.
        40 => out.extend([
            order(Command::SetFormation {
                regiment: fourth.handle,
                formation: Formation::Tight,
            }),
            order(Command::SetFormation {
                regiment: third.handle,
                formation: Formation::ShieldWall,
            }),
            order(Command::Attack {
                regiment: third.handle,
                target: enemy.handle,
            }),
        ]),
        // A second zone for side 0 and a regiment raised outside its zone,
        // both refused.
        60 => out.extend([
            order(Command::DeployZone {
                center: outside,
                radius: zone.radius,
            }),
            order(Command::SpawnRegiment {
                unit: first.unit,
                position: outside,
                facing: Angle(0),
            }),
        ]),
        _ => {}
    }
}

/// The tick `field-deploy-1v1`'s battle begins: its zones come in the first
/// turn, and side 0 never readies, so the deployment time runs out.
const BATTLE_BEGINS: u32 = DEPLOY_SECONDS * gc_sim::TICKS_PER_SECOND;

/// The orders [`deploying`] gives while deploying, by tick: how many the
/// phase takes and how many it refuses.
const DEPLOY_ORDERS: [(u32, u32, u32); 3] = [(20, 1, 2), (40, 1, 2), (60, 0, 2)];

use Difficulty::{General, Veteran};
use Faction::{Brood, CarrionConcord, Choir, EmberClans};

const GOLDENS: &[Golden] = &[
    Golden {
        name: "field-1v1",
        seed: 0x1d1,
        map_tiles: 256,
        sides: &[side(EmberClans, Veteran, 0), side(Brood, General, 1)],
        soldiers: 0,
        rows: 1,
        minutes: 5,
        minds: true,
        extra: no_extra,
        champions: false,
        deployment: false,
    },
    Golden {
        name: "field-2v2",
        seed: 0x2d2,
        map_tiles: 256,
        sides: &[
            side(EmberClans, General, 0),
            side(Brood, Veteran, 1),
            side(CarrionConcord, Veteran, 0),
            side(Choir, General, 1),
        ],
        soldiers: 0,
        rows: 1,
        minutes: 5,
        minds: true,
        extra: no_extra,
        champions: false,
        deployment: false,
    },
    Golden {
        name: "sandbox-tuning",
        seed: 0x5a2d,
        map_tiles: 256,
        sides: &[
            side(EmberClans, Veteran, 0),
            side(CarrionConcord, Veteran, 1),
        ],
        soldiers: 0,
        rows: 1,
        minutes: 3,
        minds: true,
        extra: tuning,
        champions: false,
        deployment: false,
    },
    // Woods and tall grass in play (docs/20-ambushes.md): regiments hide,
    // are spotted, and spring ambushes, so the vision rules are checked on
    // every platform too.
    Golden {
        name: "field-woods-1v1",
        seed: 0xd9,
        map_tiles: 256,
        sides: &[side(Brood, General, 0), side(Choir, General, 1)],
        soldiers: 0,
        rows: 1,
        minutes: 5,
        minds: true,
        extra: no_extra,
        champions: false,
        deployment: false,
    },
    // The `gc_bench` grand battle: about 6,000 soldiers, all four armies
    // charging into one brawl in the middle. Until the army builder brings
    // points, an "8,000 point" army is 1,500 soldiers.
    Golden {
        name: "field-8000pt-2v2",
        seed: 0x8000,
        map_tiles: 384,
        sides: &[
            side(EmberClans, General, 0),
            side(Brood, General, 1),
            side(CarrionConcord, General, 0),
            side(EmberClans, General, 1),
        ],
        soldiers: 1500,
        rows: 4,
        minutes: 5,
        minds: false,
        extra: charge,
        champions: false,
        deployment: false,
    },
    // Champions come out before the armies close, and one kills the other.
    Golden {
        name: "field-champions-1v1",
        seed: 0xc4a1,
        map_tiles: 256,
        sides: &[side(EmberClans, Veteran, 0), side(Choir, Veteran, 1)],
        soldiers: 0,
        rows: 1,
        minutes: 4,
        minds: true,
        extra: no_extra,
        champions: true,
        deployment: false,
    },
    // The deployment phase every battle with people opens with (docs/06,
    // "Deployment"): placing inside and outside the zone, orders the phase
    // refuses, a formation change, one side ready at once and the other
    // running out the clock, then a paced move once the battle begins.
    Golden {
        name: "field-deploy-1v1",
        seed: 0xde9,
        map_tiles: 256,
        sides: &[
            side(EmberClans, Veteran, 0),
            side(CarrionConcord, Veteran, 1),
        ],
        soldiers: 0,
        rows: 1,
        minutes: 5,
        minds: true,
        extra: deploying,
        champions: false,
        deployment: true,
    },
];

#[test]
#[ignore = "records the golden replays; run by hand (see the module docs)"]
fn record_golden_replays() {
    let only = std::env::var("GOLDEN").ok();
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../replays");
    for golden in GOLDENS {
        if only.as_deref().is_some_and(|o| o != golden.name) {
            continue;
        }
        let replay = record(golden);
        let path = folder.join(format!("{}.{}", golden.name, gc_replay::EXTENSION));
        replay.save(&path).unwrap();
        replay.verify().expect("the recording plays back");
        println!("wrote {}", path.display());
    }
}

fn record(golden: &Golden) -> gc_replay::Replay {
    let data = crate::data::load();
    let units: Vec<UnitStats> = data.units.iter().map(crate::sandbox::stats_of).collect();
    let factions: Vec<Faction> = data
        .units
        .iter()
        .map(|u| Faction::from_index(u.faction() as u8))
        .collect();
    let heightmap = Heightmap::generate(golden.seed, golden.map_tiles);
    let nav = heightmap.nav_costs();
    let teams: Vec<u8> = golden.sides.iter().map(|s| s.team).collect();
    let setup = MatchSetup {
        seed: golden.seed,
        capacities: Capacities::default(),
        units: &units,
        map_tiles: golden.map_tiles,
        nav: &nav,
        teams: &teams,
        ground: heightmap.ground(),
        champions: golden.champions,
    };
    let mut world = World::new(&setup);
    let mut minds = Minds::new(heightmap.ai_terrain(&nav));
    let mut out = Vec::new();

    let starts = battle_starts(&teams, 0, start_positions(golden.map_tiles));
    let middle = Vec2::splat(golden.map_tiles as f32 * 0.5);
    let mut fielded = Vec::new();
    // As `battle::set_out` sends them: every zone, then the armies, then
    // the computer players' Ready.
    let (mut zones, mut ready) = (Vec::new(), Vec::new());
    for (i, s) in golden.sides.iter().enumerate() {
        let party = i as PlayerId;
        let roll = golden.seed ^ i as u64;
        let mind = Mind::new(MindConfig {
            party,
            difficulty: s.level,
            personality: gc_ai::personality::pick(
                s.faction,
                gc_fixed::Pcg32::new(golden.seed, i as u64).next_u32(),
            ),
            seed: roll,
        });
        let preset = if golden.minds {
            mind.personality().army
        } else {
            army::Preset::Balanced
        };
        let raised = raise(&units, &factions, s.faction, preset, golden.soldiers);
        let foe = (0..golden.sides.len())
            .filter(|&j| teams[j] != s.team)
            .map(|j| starts[j])
            .min_by(|a, b| a.distance(starts[i]).total_cmp(&b.distance(starts[i])))
            .unwrap_or(middle);
        let forward = (foe - starts[i]).normalize_or(Vec2::NEG_Y);
        let placed = deploy(&units, &raised, starts[i], forward, golden.rows, &minds);
        if golden.deployment {
            zones.push(zone_order(party, &placed, starts[i]));
            if i > 0 {
                ready.push(PlayerCommand {
                    player: party,
                    command: Command::Ready,
                });
            }
        }
        army::spawn_commands(party, &placed, &mut out);
        fielded.push((
            raised.len(),
            raised
                .iter()
                .map(|&u| u32::from(units[u as usize].models))
                .sum::<u32>(),
        ));
        if golden.minds {
            minds.add(mind);
        }
    }

    let first: Vec<_> = zones
        .into_iter()
        .chain(out.drain(..))
        .chain(ready)
        .collect();
    let mut rec = Recorder::new(&setup, &world, String::new());
    rec.tick(&mut world, &first);
    let limit = golden.minutes * 60 * gc_sim::TICKS_PER_SECOND;
    let mut most_engaged = (0, 0);
    let mut ambushes = 0;
    while world.tick_count() < limit {
        out.clear();
        minds.tick(&world, &mut out);
        (golden.extra)(&world, &mut out);
        let tick = world.tick_count();
        let stats = rec.tick(&mut world, &out);
        if golden.deployment {
            if let Some(&(_, applied, refused)) = DEPLOY_ORDERS.iter().find(|o| o.0 == tick) {
                assert_eq!(
                    (stats.commands_applied, stats.commands_rejected),
                    (applied, refused),
                    "{}: the orders at tick {tick}",
                    golden.name
                );
            }
            assert_eq!(
                world.deploying(),
                tick + 1 < BATTLE_BEGINS,
                "at tick {tick}"
            );
        }
        ambushes += world.ambushes().len();
        if std::env::var_os("AI_TRACE").is_some() && world.tick_count().is_multiple_of(400) {
            println!("t={}s", world.tick_count() / 20);
            for m in minds.iter() {
                println!("  {}", m.summary());
            }
        }
        let engaged: u32 = world
            .regiments
            .iter()
            .filter(|r| r.engaged)
            .map(|r| u32::from(r.models))
            .sum();
        if std::env::var_os("ENGAGED_TRACE").is_some() && world.tick_count().is_multiple_of(100) {
            let alive: u32 = world.regiments.iter().map(|r| u32::from(r.models)).sum();
            println!(
                "t={:>3}s melee {engaged:>5} of {alive:>5}",
                world.tick_count() / 20
            );
        }
        if engaged > most_engaged.0 {
            most_engaged = (engaged, world.tick_count());
        }
        if world.verdict().is_some() {
            break;
        }
    }

    let secs = world.tick_count() / gc_sim::TICKS_PER_SECOND;
    let armies: Vec<String> = golden
        .sides
        .iter()
        .zip(&fielded)
        .map(|(s, (regiments, soldiers))| {
            let who = if golden.minds {
                s.level.name()
            } else {
                "charging"
            };
            format!(
                "{:?} {who} (team {}, {regiments} regiments, {soldiers} soldiers)",
                s.faction, s.team
            )
        })
        .collect();
    let ending = match world.verdict() {
        Some(v) => {
            format!(
                "team {} holds the field",
                golden.sides[v.holder as usize].team
            )
        }
        _ => "cut at the time limit".into(),
    };
    let champions = world.challenge().map_or(String::new(), |c| {
        let at = gc_ai::lens::clock(c.decided());
        match c.outcome(u32::MAX) {
            Outcome::Killed { .. } => format!("; champions duel to a kill at {at}"),
            Outcome::Parted => format!("; champions duel to a draw at {at}"),
            _ => format!("; the champions' duel is cut short at {at}"),
        }
    });
    assert_eq!(
        champions.is_empty(),
        !golden.champions,
        "{}: champions should come out exactly when allowed",
        golden.name
    );
    let note = format!(
        "{}: Field Battle recorded from gc_game's goldens.rs, seed {:#x}, {}-tile map; {}; \
         {}:{:02}, {ending}; most soldiers in melee at once {} (at {}:{:02}); \
         ambushes sprung {ambushes}{champions}",
        golden.name,
        golden.seed,
        golden.map_tiles,
        armies.join(" vs "),
        secs / 60,
        secs % 60,
        most_engaged.0,
        most_engaged.1 / 1200,
        most_engaged.1 / 20 % 60,
    );
    println!("{note}");
    let mut replay = rec.finish();
    replay.note = note;
    replay
}

/// The army a side raises: the lobby's standard army, or enough regiments
/// for `soldiers`.
fn raise(
    units: &[UnitStats],
    factions: &[Faction],
    faction: Faction,
    preset: army::Preset,
    soldiers: u32,
) -> Vec<gc_sim::UnitId> {
    let mut size = army::ARMY_REGIMENTS;
    loop {
        let raised = army::pick_army(units, factions, faction, preset, size);
        let total: u32 = raised
            .iter()
            .map(|&u| u32::from(units[u as usize].models))
            .sum();
        if total >= soldiers {
            return raised;
        }
        size += 1;
    }
}

/// The standard deployment, in `rows` blocks one behind the other.
fn deploy(
    units: &[UnitStats],
    raised: &[gc_sim::UnitId],
    start: Vec2,
    forward: Vec2,
    rows: usize,
    minds: &Minds,
) -> Vec<Placement> {
    const ROW_DEPTH: f32 = 18.0;
    let rows = rows.clamp(1, raised.len().max(1));
    let mut placed = Vec::with_capacity(raised.len());
    for k in 0..rows {
        // Every row gets its share of each kind of regiment.
        let row: Vec<_> = raised.iter().copied().skip(k).step_by(rows).collect();
        let centre = start - forward * (k as f32 * ROW_DEPTH);
        placed.extend(army::deploy(units, &row, centre, forward, minds.terrain()));
    }
    placed
}
