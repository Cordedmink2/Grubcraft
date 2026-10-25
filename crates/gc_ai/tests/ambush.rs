//! The battle AI with woods and tall grass on the map (docs/20-ambushes.md):
//! minds see only what their side sees, and a cunning general hides its
//! fast regiments in a wood off its flank and springs them on an enemy that
//! comes on.
//!
//! The AI (party 0) stands in the south facing north, its right is +x.
//!
//! `AI_TRACE=1 cargo test -p gc_ai --test ambush <name> -- --nocapture`
//! prints the field every two seconds.

use std::ops::Range;

use gc_ai::army::{Placement, spawn_commands};
use gc_ai::scripted::ChargeBot;
use gc_ai::{Difficulty, Doctrine, Mind, MindConfig, Minds, Perception, Role, Terrain};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{
    Capacities, Formation, MatchSetup, PlayerCommand, PlayerId, RegimentHandle, UnitId, World,
};
use glam::Vec2;

const MAP: u32 = 256;
const AI: PlayerId = 0;
const FOE: PlayerId = 1;
const NORTH: Vec2 = Vec2::new(0.0, -1.0);
const SOUTH: Vec2 = Vec2::new(0.0, 1.0);

const SPEARS: UnitId = 0;
const CAVALRY: UnitId = 1;
const ARCHERS: UnitId = 2;

const GRASS: u8 = 1;
const WOOD: u8 = 2;

fn width(unit: UnitId) -> f32 {
    let s = &DRILL_UNITS[unit as usize];
    s.block_size_in(Formation::Line, s.files, s.models).0.raw() as f32 / 65536.0
}

/// Regiments side by side, two tiles apart, centred on (`x`, `y`).
fn row(regiments: &[UnitId], x: f32, y: f32, facing: Vec2) -> Vec<Placement> {
    let right = Vec2::new(-facing.y, facing.x);
    let total: f32 = regiments.iter().map(|&u| width(u) + 2.0).sum::<f32>() - 2.0;
    let mut across = -total * 0.5;
    regiments
        .iter()
        .map(|&u| {
            let w = width(u);
            let p = Vec2::new(x, y) + right * (across + w * 0.5);
            across += w + 2.0;
            Placement {
                unit: u,
                position: p,
                facing,
            }
        })
        .collect()
}

/// Cover over tiles `x` by `y`.
fn patch(cover: &mut [u8], kind: u8, x: Range<u32>, y: Range<u32>) {
    for ty in y {
        for tx in x.clone() {
            cover[(ty * MAP + tx) as usize] = kind;
        }
    }
}

/// The AI's army: a line of spears with archers behind and cavalry behind
/// both ends, facing north from y = 200.
fn ai_army() -> Vec<Placement> {
    let mut army = row(&[SPEARS; 4], 128.0, 200.0, NORTH);
    army.extend(row(&[ARCHERS], 128.0, 208.0, NORTH));
    army.extend(row(&[CAVALRY], 100.0, 206.0, NORTH));
    army.extend(row(&[CAVALRY], 156.0, 206.0, NORTH));
    army
}

fn foe_army() -> Vec<Placement> {
    row(&[SPEARS; 4], 128.0, 70.0, SOUTH)
}

/// A wood off the AI's right, a little in front of its line.
fn wood_on_the_right() -> Vec<u8> {
    let mut cover = vec![0; (MAP * MAP) as usize];
    patch(&mut cover, WOOD, 150..174, 172..196);
    patch(&mut cover, GRASS, 146..150, 172..196);
    cover
}

#[derive(Clone, Copy, PartialEq)]
enum Foe {
    Idle,
    /// Charges the nearest regiment it can see, from `after` seconds.
    ChargeAfter(u32),
}

struct Battle {
    cover: Vec<u8>,
    army: Vec<Placement>,
    foe: Foe,
}

fn world(cover: &[u8]) -> World {
    World::new(&MatchSetup {
        seed: 7,
        capacities: Capacities { regiments: 32 },
        units: &DRILL_UNITS,
        map_tiles: MAP,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground {
            heights: &[],
            water_level: 0,
            cover,
            growth: &[],
            footing: &[],
            flow: &[],
        },
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    })
}

/// What a check sees each tick.
struct Field<'a> {
    world: &'a World,
    mind: &'a Mind,
}

impl Field<'_> {
    fn seconds(&self) -> f32 {
        self.world.tick_count() as f32 / 20.0
    }

    fn role(&self, h: RegimentHandle) -> Option<Role> {
        let m = self.mind.captain().members.iter().find(|m| m.handle == h)?;
        Some(m.role)
    }

    /// The AI's regiments of `unit`.
    fn ours(&self, unit: UnitId) -> Vec<RegimentHandle> {
        self.world
            .regiments
            .iter()
            .filter(|r| r.owner == AI && r.unit == unit)
            .map(|r| r.handle)
            .collect()
    }

    /// How many of the AI's `unit` regiments lie hidden as ambushers.
    fn hidden_ambushers(&self, unit: UnitId) -> usize {
        self.ours(unit)
            .into_iter()
            .filter(|&h| self.role(h) == Some(Role::Ambusher) && self.world.hidden(h))
            .count()
    }
}

fn run(battle: Battle, seconds: u32, mut watch: impl FnMut(&Field)) {
    let mut world = world(&battle.cover);
    let mut out: Vec<PlayerCommand> = Vec::new();
    spawn_commands(AI, &battle.army, &mut out);
    spawn_commands(FOE, &foe_army(), &mut out);
    world.tick(&out);
    let terrain = Terrain::new(&[], MAP).with_cover(battle.cover.clone());
    let mut minds = Minds::new(terrain);
    minds.add(Mind::new(MindConfig {
        party: AI,
        difficulty: Difficulty::General,
        personality: gc_ai::personality::find("The Deep Hive").unwrap(),
        seed: 11,
    }));
    let mut bot = ChargeBot::new(FOE);
    let mut foe_view = Perception::new();
    let trace = std::env::var_os("AI_TRACE").is_some();
    while world.tick_count() < seconds * 20 {
        out.clear();
        if let Foe::ChargeAfter(s) = battle.foe
            && world.tick_count() >= s * 20
        {
            foe_view.capture_for(&world, FOE);
            bot.tick(&foe_view, &mut out);
        }
        minds.tick(&world, &mut out);
        world.tick(&out);
        let mind = minds.get(AI).unwrap();
        let field = Field {
            world: &world,
            mind,
        };
        if trace && world.tick_count().is_multiple_of(40) {
            println!("t={}s  {}", world.tick_count() / 20, mind.summary());
            for r in world.regiments.iter() {
                let p = gc_ai::perception::v2(r.position);
                println!(
                    "  {} u{} ({:5.1},{:5.1}) {:>2} {:?}{}{} {}",
                    r.owner,
                    r.unit,
                    p.x,
                    p.y,
                    r.models,
                    r.morale,
                    if r.engaged { " ENG" } else { "" },
                    if world.hidden(r.handle) {
                        " hidden"
                    } else {
                        ""
                    },
                    field.role(r.handle).map_or("", |r| r.name()),
                );
            }
        }
        watch(&field);
    }
    if trace {
        for e in minds.get(AI).unwrap().journal().entries() {
            println!("  {:>5}: {}", e.tick, e.text);
        }
    }
}

#[test]
fn a_cunning_general_hides_its_cavalry_in_a_wood_off_its_flank() {
    let mut doctrine = None;
    let mut hidden_ambushers = 0;
    let mut in_the_wood = false;
    run(
        Battle {
            cover: wood_on_the_right(),
            army: ai_army(),
            foe: Foe::Idle,
        },
        40,
        |f| {
            doctrine = Some(f.mind.captain().doctrine);
            hidden_ambushers = f.hidden_ambushers(CAVALRY);
            in_the_wood = f.ours(CAVALRY).into_iter().any(|h| {
                let r = f.world.regiments.get(h).unwrap();
                let p = gc_ai::perception::v2(r.position);
                (150.0..174.0).contains(&p.x) && (172.0..196.0).contains(&p.y)
            });
        },
    );
    assert_eq!(doctrine, Some(Doctrine::Ambush));
    assert!(hidden_ambushers >= 1, "no cavalry is hiding");
    assert!(in_the_wood, "the hidden cavalry is not in the wood");
}

#[test]
fn an_army_without_fast_troops_hides_the_ends_of_its_line() {
    let mut army = row(&[SPEARS; 6], 140.0, 200.0, NORTH);
    army.extend(row(&[ARCHERS; 2], 140.0, 208.0, NORTH));
    let mut doctrine = None;
    let mut hidden_spears = 0;
    run(
        Battle {
            cover: wood_on_the_right(),
            army,
            foe: Foe::Idle,
        },
        40,
        |f| {
            doctrine = Some(f.mind.captain().doctrine);
            hidden_spears = f.hidden_ambushers(SPEARS);
        },
    );
    assert_eq!(doctrine, Some(Doctrine::Ambush));
    // A third of a line of six, if the wood has room for them.
    assert!(
        (1..=2).contains(&hidden_spears),
        "{hidden_spears} spear regiments hiding"
    );
}

#[test]
fn without_cover_nobody_lays_an_ambush() {
    let mut ever = false;
    run(
        Battle {
            cover: Vec::new(),
            army: ai_army(),
            foe: Foe::Idle,
        },
        40,
        |f| {
            ever |= f.mind.captain().doctrine == Doctrine::Ambush
                || f.mind
                    .captain()
                    .members
                    .iter()
                    .any(|m| m.role == Role::Ambusher);
        },
    );
    assert!(!ever);
}

#[test]
fn the_ambush_springs_on_an_enemy_that_comes_on() {
    let mut sprung: Vec<(f32, RegimentHandle)> = Vec::new();
    let mut hidden_when_it_came = false;
    run(
        Battle {
            cover: wood_on_the_right(),
            army: ai_army(),
            foe: Foe::ChargeAfter(30),
        },
        110,
        |f| {
            if f.seconds() > 30.0 && f.seconds() < 31.0 {
                hidden_when_it_came |= f.ours(CAVALRY).into_iter().any(|h| f.world.hidden(h));
            }
            for a in f.world.ambushes() {
                if f.world
                    .regiments
                    .get(a.ambusher)
                    .is_some_and(|r| r.owner == AI)
                {
                    sprung.push((f.seconds(), a.ambusher));
                }
            }
        },
    );
    assert!(hidden_when_it_came, "nobody was hiding when the enemy came");
    println!("ambushes sprung by the AI: {sprung:?}");
    assert!(!sprung.is_empty(), "the ambush never sprang");
}

#[test]
fn minds_see_only_what_their_side_sees() {
    // The foe hides in the wood in front of the AI from the start.
    let mut cover = vec![0; (MAP * MAP) as usize];
    patch(&mut cover, WOOD, 110..146, 120..150);
    let mut world = world(&cover);
    let mut out = Vec::new();
    spawn_commands(AI, &ai_army(), &mut out);
    spawn_commands(FOE, &row(&[SPEARS; 2], 128.0, 135.0, SOUTH), &mut out);
    world.tick(&out);
    let mut view = Perception::new();
    view.capture_for(&world, AI);
    assert_eq!(view.regiments.iter().filter(|r| r.owner == FOE).count(), 0);
    assert_eq!(view.regiments.iter().filter(|r| r.owner == AI).count(), 7);
    view.capture_for(&world, FOE);
    let hidden = view.regiments.iter().filter(|r| r.owner == FOE && r.hidden);
    assert_eq!(
        hidden.count(),
        2,
        "the foe's own view marks its hidden regiments"
    );
    view.capture(&world);
    assert_eq!(view.regiments.len(), 9, "the whole view still has everyone");

    // A mind never orders an attack on a regiment its side can't see.
    let mut minds = Minds::new(Terrain::new(&[], MAP).with_cover(cover));
    minds.add(Mind::new(MindConfig {
        party: AI,
        difficulty: Difficulty::General,
        personality: 0,
        seed: 3,
    }));
    let hidden: Vec<RegimentHandle> = world
        .regiments
        .iter()
        .filter(|r| r.owner == FOE)
        .map(|r| r.handle)
        .collect();
    for _ in 0..20 * 20 {
        out.clear();
        minds.tick(&world, &mut out);
        for c in &out {
            if let gc_sim::Command::Attack { target, .. } = c.command {
                assert!(
                    !hidden.contains(&target) || world.sees(AI, target),
                    "ordered an attack on a hidden regiment"
                );
            }
        }
        world.tick(&out);
    }
}

#[test]
fn a_mind_remembers_enemies_it_lost_sight_of() {
    // A foe regiment in the open walks into a wood: the AI's side loses
    // sight of it but remembers it where it was last seen, for a while.
    let mut cover = vec![0; (MAP * MAP) as usize];
    patch(&mut cover, WOOD, 110..146, 100..120);
    let mut world = world(&cover);
    let mut out = Vec::new();
    spawn_commands(AI, &ai_army(), &mut out);
    spawn_commands(FOE, &row(&[SPEARS], 128.0, 140.0, SOUTH), &mut out);
    world.tick(&out);
    let foe = world
        .regiments
        .iter()
        .find(|r| r.owner == FOE)
        .unwrap()
        .handle;
    out.clear();
    out.push(PlayerCommand {
        player: FOE,
        command: gc_sim::Command::MoveTo {
            regiment: foe,
            target: gc_ai::perception::to_fx_vec(Vec2::new(128.0, 108.0)),
            facing: None,
            files: 0,
            pace: gc_fixed::Fx::ZERO,
        },
    });
    let mut view = Perception::new();
    let mut last_seen = None;
    let mut lost_at = None;
    while world.tick_count() < 120 * 20 {
        world.tick(&out);
        out.clear();
        view.capture_for(&world, AI);
        match view.get(foe) {
            Some(r) => {
                assert_eq!(view.remembered().count(), 0);
                last_seen = Some(r.pos);
            }
            None if lost_at.is_none() => lost_at = Some(world.tick_count()),
            None => {}
        }
        if let Some(t) = lost_at {
            let since = world.tick_count() - t;
            let remembered = view.remembered().find(|r| r.handle == foe);
            if since < gc_ai::perception::REMEMBER_TICKS {
                let r = remembered.expect("it remembers the foe it lost sight of");
                assert_eq!(Some(r.pos), last_seen, "where it was last seen");
            } else if since > gc_ai::perception::REMEMBER_TICKS {
                assert!(remembered.is_none(), "and forgets it in the end");
            }
        }
    }
    assert!(lost_at.is_some(), "the foe never got out of sight");
    assert!(world.hidden(foe));
}
