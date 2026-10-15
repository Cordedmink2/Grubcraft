//! The battle AI's drills (docs/18-ai.md, "Testing, tuning and explaining"):
//! eight tactical puzzles, each with what a good general would do. Each sets
//! out a small battle on made-up ground, lets a General command one side
//! (party 0) against an opponent that stands still or charges straight in,
//! and checks what it did.
//!
//! The AI faces north (towards smaller y), so its right is +x.
//!
//! `AI_TRACE=1 AI_PERSONA=<n> cargo test -p gc_ai --test drills <name> -- --nocapture`
//! prints the field every two seconds, with personality `n` only.

use std::ops::Range;

use gc_ai::army::{Placement, spawn_commands};
use gc_ai::captain::gap_between;
use gc_ai::ground::Line;
use gc_ai::lens::Lens;
use gc_ai::perception::{Seen, Side};
use gc_ai::scripted::ChargeBot;
use gc_ai::{Difficulty, Mind, MindConfig, Minds, Perception, Role, Terrain};
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{
    Capacities, MatchSetup, MoraleState, PlayerCommand, PlayerId, UnitId, UnitStats, World,
};
use glam::Vec2;

const MAP: u32 = 256;
const AI: PlayerId = 0;
const FOE: PlayerId = 1;
const NORTH: Vec2 = Vec2::new(0.0, -1.0);
const SOUTH: Vec2 = Vec2::new(0.0, 1.0);
const WEST: Vec2 = Vec2::new(-1.0, 0.0);
/// Ground height, and the water line below it.
const LAND: f32 = 2.0;
const WATER_LEVEL: f32 = 1.0;

// The drills' unit table: the four drill units, then spears with less nerve.
const SPEARS: UnitId = 0;
const CAVALRY: UnitId = 1;
const ARCHERS: UnitId = 2;
const PIKES: UnitId = 3;
/// Morale 20: wavering from the start, and close to running.
const FAINT: UnitId = 4;
/// Morale 24: wavering from the start (regiments waver below 25).
const SHAKEN: UnitId = 5;
/// Morale 30: steady until the first real blow.
const NERVOUS: UnitId = 6;

fn units() -> Vec<UnitStats> {
    let mut table = DRILL_UNITS.to_vec();
    for morale in [20, 24, 30] {
        table.push(UnitStats {
            morale,
            ..DRILL_UNITS[SPEARS as usize]
        });
    }
    table
}

fn width(unit: UnitId) -> f32 {
    let s = &units()[unit as usize];
    s.block_size_in(gc_sim::Formation::Line, s.files, s.models)
        .0
        .raw() as f32
        / 65536.0
}

/// One regiment at (`x`, `y`) facing `facing`.
fn at(unit: UnitId, x: f32, y: f32, facing: Vec2) -> Placement {
    Placement {
        unit,
        position: Vec2::new(x, y),
        facing,
    }
}

/// Regiments side by side, two tiles apart, centred on (`x`, `y`); the
/// first is on the left as seen looking along `facing`.
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

/// The ground a drill is fought on: open grass unless cliffs or a river
/// are added.
struct Ground {
    nav: Vec<u8>,
    heights: Vec<f32>,
}

impl Ground {
    fn open() -> Ground {
        let side = (MAP as i32 * CELLS_PER_TILE) as usize;
        Ground {
            nav: vec![1; side * side],
            heights: vec![LAND; (MAP * MAP) as usize],
        }
    }

    fn cells(&mut self, x: &Range<f32>, y: &Range<f32>, cost: u8) {
        let side = (MAP as i32 * CELLS_PER_TILE) as usize;
        for cy in 0..side {
            for cx in 0..side {
                let p = Vec2::new(cx as f32 + 0.5, cy as f32 + 0.5) / CELLS_PER_TILE as f32;
                if x.contains(&p.x) && y.contains(&p.y) {
                    self.nav[cy * side + cx] = cost;
                }
            }
        }
    }

    /// A cliff no regiment climbs.
    fn cliff(mut self, x: Range<f32>, y: Range<f32>) -> Ground {
        self.cells(&x, &y, BLOCKED);
        self
    }

    /// A river across the whole map between `y.start` and `y.end`, too deep
    /// to wade except at a ford between `ford.start` and `ford.end`.
    fn river(mut self, y: Range<f32>, ford: Range<f32>) -> Ground {
        self.cells(&(0.0..MAP as f32), &y, BLOCKED);
        self.cells(&ford, &y, 3);
        for ty in 0..MAP {
            if y.contains(&(ty as f32 + 0.5)) {
                for tx in 0..MAP {
                    self.heights[(ty * MAP + tx) as usize] = 0.0;
                }
            }
        }
        self
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Foe {
    /// Stands where it was put; only the simulation's own reflexes fight.
    Idle,
    /// Every regiment attacks the nearest enemy.
    Charge,
}

struct Drill {
    ground: Ground,
    ai: Vec<Placement>,
    foe: Vec<Placement>,
    foe_plays: Foe,
    /// Index into the personalities (data/ai/personalities.ron).
    personality: usize,
}

/// What a drill's check sees each tick.
struct Field<'a> {
    view: &'a Perception,
    mind: &'a Mind,
    terrain: &'a Terrain,
}

impl Field<'_> {
    fn seconds(&self) -> f32 {
        self.view.tick as f32 / 20.0
    }

    fn of(&self, party: PlayerId) -> impl Iterator<Item = &Seen> {
        self.view.regiments.iter().filter(move |r| r.owner == party)
    }

    /// The enemy regiment `r` is most likely locked with: the nearest.
    fn opposite(&self, r: &Seen) -> Option<&Seen> {
        self.view
            .regiments
            .iter()
            .filter(|f| self.view.hostile(r.owner, f.owner) && !f.broken())
            .min_by(|a, b| gap_between(r, a).total_cmp(&gap_between(r, b)))
    }
}

/// Fight `drill` for up to `seconds`, calling `watch` after every tick.
/// Stops early once one side has held the field for ten seconds, and
/// returns that side.
fn run(drill: Drill, seconds: u32, mut watch: impl FnMut(&Field)) -> Option<PlayerId> {
    let units = units();
    // The simulation's terrain rules read the same heights, in its units.
    let per_tile = gc_sim::terrain::HEIGHT_UNITS as f32;
    let sim_heights: Vec<u16> = drill
        .ground
        .heights
        .iter()
        .map(|h| (h * per_tile) as u16)
        .collect();
    let terrain =
        Terrain::new(&drill.ground.nav, MAP).with_heights(drill.ground.heights, WATER_LEVEL);
    let mut world = World::new(&MatchSetup {
        seed: 7,
        capacities: Capacities { regiments: 32 },
        units: &units,
        map_tiles: MAP,
        nav: &drill.ground.nav,
        teams: &[],
        ground: gc_sim::Ground {
            heights: &sim_heights,
            water_level: (WATER_LEVEL * per_tile) as u16,
            cover: &[],
        },
        champions: false,
    });
    let mut out: Vec<PlayerCommand> = Vec::new();
    spawn_commands(AI, &drill.ai, &mut out);
    spawn_commands(FOE, &drill.foe, &mut out);
    world.tick(&out);
    let mut minds = Minds::new(terrain);
    minds.add(Mind::new(MindConfig {
        party: AI,
        difficulty: Difficulty::General,
        personality: drill.personality,
        seed: 11,
    }));
    let mut bot = ChargeBot::new(FOE);
    let mut view = Perception::new();
    view.capture(&world);
    let trace = std::env::var_os("AI_TRACE").is_some();
    let mut held: Option<(PlayerId, u32)> = None;
    while world.tick_count() < seconds * 20 {
        out.clear();
        if drill.foe_plays == Foe::Charge {
            bot.tick(&view, &mut out);
        }
        minds.tick(&world, &mut out);
        world.tick(&out);
        view.capture(&world);
        let mind = minds.get(AI).unwrap();
        watch(&Field {
            view: &view,
            mind,
            terrain: minds.terrain(),
        });
        if trace && view.tick.is_multiple_of(40) {
            print_field(&view, mind);
        }
        if view.tick.is_multiple_of(20) {
            match (gc_ai::holding_the_field(&view), held) {
                (Some(p), Some((q, since))) if p == q => {
                    if view.tick >= since + 200 {
                        return Some(p);
                    }
                }
                (Some(p), _) => held = Some((p, view.tick)),
                (None, _) => held = None,
            }
        }
    }
    if trace {
        for e in minds.get(AI).unwrap().journal().entries() {
            println!("  {:>5}: {}", e.tick, e.text);
        }
    }
    None
}

fn print_field(view: &Perception, mind: &Mind) {
    println!("t={}s  {}", view.tick / 20, mind.summary());
    for r in &view.regiments {
        let role = mind
            .captain()
            .members
            .iter()
            .find(|m| m.handle == r.handle)
            .map_or(String::new(), |m| {
                let target = m
                    .target
                    .map_or(String::new(), |t| format!(" ->{}", t.index));
                format!("{}{target}", m.role.name())
            });
        println!(
            "  {}{:>2} u{} ({:5.1},{:5.1}) f({:4.1},{:4.1}) {:>2}/{:>2} {:?}{}{} m{} {}",
            if r.owner == AI { "A" } else { "b" },
            r.handle.index,
            r.unit,
            r.pos.x,
            r.pos.y,
            r.facing.x,
            r.facing.y,
            r.models,
            r.full,
            r.morale,
            if r.engaged { " ENG" } else { "" },
            if r.moving { " mov" } else { "" },
            r.morale_points,
            role,
        );
    }
}

/// Tracks which regiments were in melee last tick, to spot new contacts.
#[derive(Default)]
struct Contacts {
    was: Vec<bool>,
}

impl Contacts {
    /// Regiments that have just come into contact this tick.
    fn new_ones<'a>(&mut self, f: &'a Field) -> Vec<&'a Seen> {
        let mut fresh = Vec::new();
        for r in &f.view.regiments {
            let i = r.handle.index as usize;
            if self.was.len() <= i {
                self.was.resize(i + 1, false);
            }
            if r.engaged && !self.was[i] {
                fresh.push(r);
            }
            self.was[i] = r.engaged;
        }
        fresh
    }

    fn was_engaged(&self, r: &Seen) -> bool {
        self.was
            .get(r.handle.index as usize)
            .copied()
            .unwrap_or(false)
    }
}

/// The AI's line regiments (spears and pikes, and the spears with less
/// nerve) as a battle line, as they stand.
fn ai_line(f: &Field) -> Line {
    let line: Vec<&Seen> = f
        .of(AI)
        .filter(|r| r.unit != CAVALRY && r.unit != ARCHERS && !r.broken())
        .collect();
    let left = line
        .iter()
        .map(|r| r.pos.x - r.width * 0.5)
        .fold(f32::MAX, f32::min);
    let right = line
        .iter()
        .map(|r| r.pos.x + r.width * 0.5)
        .fold(f32::MIN, f32::max);
    let y = line.iter().map(|r| r.pos.y).sum::<f32>() / line.len() as f32;
    let depth = line.iter().map(|r| r.depth).fold(0.0, f32::max);
    Line {
        centre: Vec2::new((left + right) * 0.5, y),
        forward: NORTH,
        width: right - left,
        depth,
    }
}

/// The personalities a drill runs with: all of `these`, or just the one
/// `AI_PERSONA=<n>` names.
fn only(these: impl IntoIterator<Item = usize>) -> Vec<usize> {
    let one: Option<usize> = std::env::var("AI_PERSONA")
        .ok()
        .and_then(|v| v.parse().ok());
    these
        .into_iter()
        .filter(|&p| one.is_none_or(|o| o == p))
        .collect()
}

/// Personalities that hold strong ground rather than marching out to meet
/// an enemy in the open: The Iron Lord, The Deep Hive, The Grand Chord and
/// The Auditor.
const STEADY: [usize; 4] = [0, 3, 4, 6];

#[test]
fn spears_turn_to_meet_cavalry_and_its_cavalry_keeps_off_spears() {
    for personality in only(0..8) {
        // Two spears facing north with cavalry on the far wing; enemy
        // cavalry comes from the east, at the spears' flank, and enemy
        // spears from the north.
        let mut ai = row(&[SPEARS, SPEARS], 126.0, 150.0, NORTH);
        ai.push(at(CAVALRY, 106.0, 152.0, NORTH));
        let foe = vec![
            at(CAVALRY, 174.0, 147.0, WEST),
            at(CAVALRY, 178.0, 156.0, WEST),
            at(SPEARS, 124.0, 104.0, SOUTH),
        ];
        let drill = Drill {
            ground: Ground::open(),
            ai,
            foe,
            foe_plays: Foe::Charge,
            personality,
        };
        let mut contacts = Contacts::default();
        let mut charges = 0;
        let mut faults: Vec<String> = Vec::new();
        run(drill, 60, |f| {
            let before = Contacts {
                was: contacts.was.clone(),
            };
            for r in contacts.new_ones(f) {
                let Some(o) = f.opposite(r) else {
                    continue;
                };
                let (ours, theirs) = if r.owner == AI { (r, o) } else { (o, r) };
                if theirs.unit == CAVALRY && ours.unit == SPEARS && r.owner == FOE {
                    charges += 1;
                    if ours.side_facing(theirs.pos) != Side::Front {
                        faults.push(format!(
                            "{:.1}s: enemy cavalry hit our spears' {:?}",
                            f.seconds(),
                            ours.side_facing(theirs.pos)
                        ));
                    }
                }
                // Riding down the last remnant of a beaten army is mopping
                // up, not feeding cavalry into spears.
                let mopping_up = f.mind.captain().finishing || theirs.strength() < 0.5;
                if ours.unit == CAVALRY
                    && theirs.unit == SPEARS
                    && r.owner == AI
                    && !mopping_up
                    && !before.was_engaged(theirs)
                    && theirs.morale == MoraleState::Steady
                    && theirs.side_facing(ours.pos) == Side::Front
                {
                    faults.push(format!(
                        "{:.1}s: our cavalry charged the front of steady spears",
                        f.seconds()
                    ));
                }
            }
        });
        assert!(faults.is_empty(), "personality {personality}: {faults:?}");
        let _ = charges;
    }
}

#[test]
fn outnumbered_two_to_one_it_falls_back_or_takes_a_flank_anchor() {
    for personality in only(0..8) {
        // Five regiments against ten standing 70 tiles off, with a cliff a
        // little way beyond the right of the line.
        let mut ai = row(&[SPEARS, SPEARS, PIKES, SPEARS], 128.0, 150.0, NORTH);
        ai.push(at(ARCHERS, 128.0, 157.0, NORTH));
        let mut foe = row(
            &[SPEARS, SPEARS, PIKES, SPEARS, SPEARS, PIKES, SPEARS],
            128.0,
            80.0,
            SOUTH,
        );
        foe.push(at(CAVALRY, 80.0, 80.0, SOUTH));
        foe.push(at(CAVALRY, 176.0, 80.0, SOUTH));
        foe.push(at(ARCHERS, 128.0, 73.0, SOUTH));
        let drill = Drill {
            ground: Ground::open().cliff(152.0..200.0, 125.0..185.0),
            ai,
            foe,
            foe_plays: Foe::Idle,
            personality,
        };
        let mut nearest = f32::MAX;
        let mut fought = false;
        let mut end: Option<(Line, [Option<f32>; 2])> = None;
        run(drill, 60, |f| {
            for r in f.of(AI) {
                nearest = nearest.min(r.pos.y);
                fought |= r.engaged;
            }
            let line = ai_line(f);
            end = Some((line, line.cover(f.terrain)));
        });
        let (line, cover) = end.unwrap();
        let fell_back = line.centre.y >= 150.0 + 15.0;
        let anchored = cover[1].is_some_and(|k| k <= 3.0);
        assert!(
            !fought && nearest >= 150.0 - 5.0 - 3.0,
            "personality {personality} went forward to y = {nearest} (fought: {fought})"
        );
        assert!(
            fell_back || anchored,
            "personality {personality}: line at {:?}, cover {cover:?}",
            line.centre
        );
    }
}

#[test]
fn its_hammer_hits_an_exposed_flank_within_a_minute() {
    for personality in only(0..8) {
        let mut ai = row(&[SPEARS, SPEARS, SPEARS], 128.0, 150.0, NORTH);
        ai.push(at(CAVALRY, 106.0, 152.0, NORTH));
        ai.push(at(CAVALRY, 150.0, 152.0, NORTH));
        let foe = row(&[SPEARS, SPEARS, SPEARS], 128.0, 110.0, SOUTH);
        let drill = Drill {
            ground: Ground::open(),
            ai,
            foe,
            foe_plays: Foe::Idle,
            personality,
        };
        let mut contacts = Contacts::default();
        let mut hit: Option<f32> = None;
        run(drill, 60, |f| {
            for r in contacts.new_ones(f) {
                if r.owner != AI || r.unit != CAVALRY {
                    continue;
                }
                if let Some(o) = f.opposite(r)
                    && o.side_facing(r.pos) != Side::Front
                    && hit.is_none()
                {
                    hit = Some(f.seconds());
                }
            }
        });
        assert!(
            hit.is_some(),
            "personality {personality}: no flank or rear hit in 60 s"
        );
    }
}

#[test]
fn it_breaks_the_wavering_regiment_standing_beside_shaken_ones() {
    for personality in only(0..8) {
        // From the AI's left: two steady regiments, then a shaken one, the
        // faint one and another shaken one.
        let foe = vec![
            at(SPEARS, 148.0, 112.0, SOUTH),
            at(SPEARS, 140.0, 112.0, SOUTH),
            at(SHAKEN, 116.0, 112.0, SOUTH),
            at(FAINT, 108.0, 112.0, SOUTH),
            at(SHAKEN, 100.0, 112.0, SOUTH),
        ];
        let mut ai = row(&[SPEARS, SPEARS, SPEARS, SPEARS], 124.0, 150.0, NORTH);
        ai.push(at(CAVALRY, 150.0, 152.0, NORTH));
        let drill = Drill {
            ground: Ground::open(),
            ai,
            foe,
            foe_plays: Foe::Idle,
            personality,
        };
        let mut contacts = Contacts::default();
        let mut first_order: Option<f32> = None;
        let mut faint_targeted: Option<f32> = None;
        let mut first_contact: Option<f32> = None;
        let mut routs: Vec<f32> = Vec::new();
        let mut routed = [false; 32];
        run(drill, 120, |f| {
            for t in f
                .mind
                .captain()
                .members
                .iter()
                .filter_map(|m| m.target.and_then(|t| f.view.get(t)))
            {
                first_order.get_or_insert(f.seconds());
                if t.unit == FAINT {
                    faint_targeted.get_or_insert(f.seconds());
                }
            }
            for r in contacts.new_ones(f) {
                if r.owner == AI && first_contact.is_none() {
                    first_contact = Some(f.seconds());
                }
            }
            for r in f.of(FOE) {
                let i = r.handle.index as usize;
                if r.broken() && !routed[i] {
                    routed[i] = true;
                    routs.push(f.seconds());
                }
            }
        });
        // Among its first targets, as the lines meet.
        let t1 = first_order.expect("never attacked");
        assert!(
            faint_targeted.is_some_and(|t| t <= t1 + 5.0),
            "personality {personality}: attacked at {t1}s, the faint regiment at {faint_targeted:?}"
        );
        let t0 = first_contact.expect("never fought");
        let quick = routs.iter().filter(|&&t| t <= t0 + 30.0).count();
        assert!(
            quick >= 2,
            "personality {personality}: {quick} routs within 30 s of contact at {t0}s ({routs:?})"
        );
    }
}

#[test]
fn a_wavering_regiment_is_pulled_out_of_the_line_and_its_place_filled() {
    for personality in only(0..8) {
        // Nine line regiments (eight in the line and a reserve), the third
        // from the left shaken; eight enemy regiments charge from 60 tiles.
        let army = [
            SPEARS, SPEARS, SHAKEN, SPEARS, SPEARS, SPEARS, SPEARS, SPEARS, SPEARS,
        ];
        let ai = row(&army, 128.0, 150.0, NORTH);
        let start = ai[2].position;
        let foe = row(&[SPEARS; 8], 128.0, 90.0, SOUTH);
        let drill = Drill {
            ground: Ground::open(),
            ai,
            foe,
            foe_plays: Foe::Charge,
            personality,
        };
        let mut contacts = Contacts::default();
        let mut first_contact: Option<f32> = None;
        let mut back_at_contact = 0.0;
        let mut filled = false;
        let mut ran = false;
        run(drill, 60, |f| {
            let shaken = f.of(AI).find(|r| r.unit == SHAKEN);
            // Until the lines have been at it a while: once the battle is
            // lost, everyone runs.
            if first_contact.is_none_or(|t0| f.seconds() <= t0 + 15.0) {
                ran |= shaken.is_none_or(gc_ai::perception::Seen::broken);
            }
            let fresh = contacts.new_ones(f);
            if first_contact.is_none() && !fresh.is_empty() {
                first_contact = Some(f.seconds());
                back_at_contact = shaken.map_or(0.0, |r| r.pos.y - start.y);
            }
            if let Some(t0) = first_contact
                && f.seconds() <= t0 + 10.0
                && let Some(place) = shaken.and_then(|r| f.mind.captain().place_of(r.handle))
            {
                // Someone else standing in its place in the line.
                filled |= f
                    .of(AI)
                    .any(|r| r.unit != SHAKEN && !r.broken() && r.pos.distance(place) < 5.0);
            }
        });
        assert!(
            first_contact.is_some(),
            "personality {personality}: no fight"
        );
        assert!(
            back_at_contact >= 8.0,
            "personality {personality}: the shaken regiment was {back_at_contact} tiles back at contact"
        );
        assert!(
            filled,
            "personality {personality}: nobody stood in its place"
        );
        assert!(
            !ran,
            "personality {personality}: the shaken regiment ran as the lines met"
        );
    }
}

#[test]
fn routers_are_chased_by_fast_troops_only_and_within_the_leash() {
    for personality in only(0..8) {
        let mut ai = row(&[SPEARS, SPEARS, SPEARS], 128.0, 150.0, NORTH);
        ai.push(at(CAVALRY, 106.0, 152.0, NORTH));
        ai.push(at(CAVALRY, 150.0, 152.0, NORTH));
        ai.push(at(ARCHERS, 128.0, 157.0, NORTH));
        let start: Vec<(UnitId, Vec2)> = ai.iter().map(|p| (p.unit, p.position)).collect();
        let foe = row(&[NERVOUS, NERVOUS, NERVOUS], 128.0, 100.0, SOUTH);
        let drill = Drill {
            ground: Ground::open(),
            ai,
            foe,
            foe_plays: Foe::Charge,
            personality,
        };
        let leash = Difficulty::General.knobs().hunt_leash;
        let mut routed = false;
        let mut chased = false;
        let mut line_wandered: f32 = 0.0;
        let mut hunter_wandered: f32 = 0.0;
        run(drill, 90, |f| {
            let routers: Vec<&Seen> = f.of(FOE).filter(|r| r.broken()).collect();
            routed |= !routers.is_empty();
            let line: Vec<&Seen> = f.of(AI).filter(|r| r.unit == SPEARS).collect();
            let centre = line.iter().map(|r| r.pos).sum::<Vec2>() / line.len().max(1) as f32;
            for r in f.of(AI) {
                match r.unit {
                    CAVALRY => {
                        chased |= routers.iter().any(|q| gap_between(r, q) < 4.0);
                        hunter_wandered = hunter_wandered.max(r.pos.distance(centre));
                    }
                    SPEARS => {
                        let from = start
                            .iter()
                            .filter(|(u, _)| *u == SPEARS)
                            .map(|(_, p)| p.distance(r.pos))
                            .fold(f32::MAX, f32::min);
                        line_wandered = line_wandered.max(from);
                    }
                    _ => {}
                }
            }
        });
        assert!(routed, "personality {personality}: nobody routed");
        assert!(
            chased,
            "personality {personality}: no cavalry chased the routers"
        );
        assert!(
            hunter_wandered <= leash + 15.0,
            "personality {personality}: cavalry {hunter_wandered} tiles from the line"
        );
        assert!(
            line_wandered <= 30.0,
            "personality {personality}: a line regiment chased {line_wandered} tiles"
        );
    }
}

/// A cliff 16 tiles beyond the right of the AI's line; the enemy charges
/// from 90 tiles off.
fn cliff_drill(personality: usize) -> Drill {
    let mut ai = row(&[SPEARS, SPEARS, PIKES, SPEARS], 128.0, 150.0, NORTH);
    ai.push(at(ARCHERS, 120.0, 157.0, NORTH));
    ai.push(at(ARCHERS, 136.0, 157.0, NORTH));
    ai.push(at(CAVALRY, 104.0, 150.0, NORTH));
    ai.push(at(CAVALRY, 152.0, 150.0, NORTH));
    let foe = row(
        &[SPEARS, SPEARS, PIKES, SPEARS, CAVALRY, CAVALRY],
        128.0,
        60.0,
        SOUTH,
    );
    Drill {
        ground: Ground::open().cliff(158.0..210.0, 115.0..190.0),
        ai,
        foe,
        foe_plays: Foe::Charge,
        personality,
    }
}

#[test]
fn it_anchors_an_end_of_its_line_on_a_cliff() {
    for personality in only(STEADY) {
        let drill = cliff_drill(personality);
        let mut contacts = Contacts::default();
        let mut at_contact: Option<(f32, Line, [Option<f32>; 2], bool)> = None;
        run(drill, 60, |f| {
            if at_contact.is_none() && !contacts.new_ones(f).is_empty() {
                let line = ai_line(f);
                let cavalry_left = f
                    .of(AI)
                    .filter(|r| r.unit == CAVALRY)
                    .all(|r| r.pos.x < line.centre.x);
                at_contact = Some((f.seconds(), line, line.cover(f.terrain), cavalry_left));
            }
        });
        let (t, line, cover, cavalry_left) = at_contact.expect("never fought");
        assert!(
            cover[1].is_some_and(|k| k <= 3.0),
            "personality {personality} at {t}s: line at {:?} {} wide, cover {cover:?}",
            line.centre,
            line.width
        );
        assert!(
            cavalry_left,
            "personality {personality}: its cavalry should wait on the open flank"
        );
    }
}

/// A river 10 tiles wide across the field, with a ford in the middle wide
/// enough for one regiment at a time.
///
/// A column wading out of the ford arrives one regiment at a time.
fn ford_drill(personality: usize) -> Drill {
    let mut ai = row(&[SPEARS, SPEARS, PIKES, SPEARS], 128.0, 145.0, NORTH);
    ai.push(at(ARCHERS, 120.0, 152.0, NORTH));
    ai.push(at(ARCHERS, 136.0, 152.0, NORTH));
    let mut foe = row(&[SPEARS, SPEARS, PIKES, SPEARS], 128.0, 88.0, SOUTH);
    foe.push(at(ARCHERS, 120.0, 81.0, SOUTH));
    foe.push(at(ARCHERS, 136.0, 81.0, SOUTH));
    Drill {
        ground: Ground::open().river(112.0..122.0, 124.0..132.0),
        ai,
        foe,
        foe_plays: Foe::Idle,
        personality,
    }
}

#[test]
fn it_waits_for_the_enemy_to_cross_the_ford() {
    for personality in only(0..8) {
        // The enemy stands on the far bank; its archers would shoot anyone
        // wading across.
        let mut crossed: Option<f32> = None;
        let mut line: Option<Line> = None;
        run(ford_drill(personality), 90, |f| {
            if crossed.is_none() && f.of(AI).any(|r| r.pos.y < 123.0 || f.terrain.wet(r.pos)) {
                crossed = Some(f.seconds());
            }
            line = Some(ai_line(f));
        });
        assert_eq!(
            crossed, None,
            "personality {personality} went into the river"
        );
        // Waiting where they would come out: facing the ford, a few tiles
        // back from the bank.
        let at = line.unwrap().centre;
        assert!(
            (at.x - 128.0).abs() <= 8.0 && (128.0..=142.0).contains(&at.y),
            "personality {personality} waits at {at}"
        );
    }
}

/// Share of each side's soldiers left standing.
fn soldiers_left(view: &Perception) -> [f32; 2] {
    [AI, FOE].map(|p| {
        let (left, full) = view
            .regiments
            .iter()
            .filter(|r| r.owner == p && !r.broken())
            .fold((0, 0), |(l, f), r| {
                (l + u32::from(r.models), f + u32::from(r.full))
            });
        left as f32 / full.max(1) as f32
    })
}

/// Not a drill: waiting at the ford pays. The enemy charges across into
/// the AI's line. Its regiments come out of the ford one at a time (they
/// queue rather than stack), slowed and without a charge, and the AI wins
/// with every personality.
#[test]
fn holding_the_ford_pays_off() {
    for personality in only(0..8) {
        let mut drill = ford_drill(personality);
        drill.foe_plays = Foe::Charge;
        let mut left = [0.0; 2];
        let winner = run(drill, 180, |f| left = soldiers_left(f.view));
        println!("personality {personality}: {winner:?}, left {left:?}");
        assert_eq!(
            winner,
            Some(AI),
            "personality {personality} lost the ford ({left:?} left)"
        );
    }
}

/// Outnumbered two to one by an enemy that only stands, the AI waits on
/// its ground for a while (it expects to lose), then goes to fetch it
/// rather than wait for ever.
#[test]
fn it_goes_to_fetch_an_enemy_that_never_comes() {
    for personality in only(0..8) {
        let drill = Drill {
            ground: Ground::open(),
            ai: row(&[SPEARS, SPEARS, SPEARS], 128.0, 150.0, NORTH),
            foe: row(&[SPEARS; 6], 128.0, 60.0, SOUTH),
            foe_plays: Foe::Idle,
            personality,
        };
        let mut contact: Option<f32> = None;
        run(drill, 300, |f| {
            if contact.is_none() && f.of(AI).any(|r| r.engaged) {
                contact = Some(f.seconds());
            }
        });
        let first = contact.map_or("never".to_string(), |s| format!("at {s:.0} s"));
        assert!(
            contact.is_some_and(|s| s >= 60.0),
            "personality {personality}: first contact {first}"
        );
    }
}

/// Not a drill: the AI lens (step 3) shows what the cliff drill checks.
#[test]
fn the_lens_shows_the_anchored_line_and_every_regiment() {
    let mut lens: Option<Lens> = None;
    let mut standing = 0;
    run(cliff_drill(0), 20, |f| {
        lens = Some(f.mind.lens(f.view));
        standing = f.of(AI).filter(|r| !r.broken()).count();
    });
    let lens = lens.unwrap();
    let front = lens.front.expect("no front");
    assert_eq!(front.covered, 1.0, "{front:?}");
    assert!(front.width > 20.0, "{front:?}");
    assert_eq!(lens.marks.len(), standing);
    assert!(lens.marks.iter().all(|m| m.place.is_some()));
    assert!(lens.marks.iter().any(|m| m.role == Role::Hammer));
    let text = lens.text.join("\n");
    println!("{text}");
    for line in [
        "odds",
        "right end anchored",
        "roles",
        "orders",
        "Taking ground",
    ] {
        assert!(text.contains(line), "no {line:?} in:\n{text}");
    }
}
