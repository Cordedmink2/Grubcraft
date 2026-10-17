//! The AI's stage-3 tables (docs/18-ai.md, "Team games" and "Reading the
//! ground"): the AI with team play and ground reading against the same
//! commanders without them, in six-player battles and on battlefields built
//! around a hill, a river or woods. Slow; run by hand in release:
//!
//! - `cargo test -p gc_match --release team_table -- --ignored --nocapture`
//! - `cargo test -p gc_match --release terrain_table -- --ignored --nocapture`
//!
//! Both read `AI_LEVEL` (default General), `AI_BATTLES` and `AI_FROM` (the
//! first seed, default 1). `AI_OFF=team_play` or `AI_OFF=read_ground` takes
//! only that away from the old side (`AI_OFF=none`: nothing, new against
//! new); `AI_OLD=1` plays old against old, to see where the field stands
//! without them. `AI_JOURNAL=1` and `AI_TRACE=1` print each battle's
//! journals and the field every twenty seconds.

use crate::battle::tests::{MAP, level, lobby, play_on};
use gc_ai::{Difficulty, Mind};
use gc_map::Map as Heightmap;
use gc_map::def::{CoverKind, CoverOp, Mode, Shape, TerrainOp};
use gc_map::{MapDef, SizeClass, Symmetry};
use gc_sim::PlayerId;
use gc_ui::lobby::MatchStart;

/// The table's settings, from the environment.
struct Table {
    level: Difficulty,
    battles: u64,
    from: u64,
    off: String,
    both_old: bool,
}

impl Table {
    fn from_env(battles: u64) -> Table {
        let var = |name: &str| std::env::var(name).ok();
        Table {
            level: var("AI_LEVEL").map_or(Difficulty::General, |n| level(&n)),
            battles: var("AI_BATTLES").map_or(battles, |v| v.parse().unwrap()),
            from: var("AI_FROM").map_or(1, |v| v.parse().unwrap()),
            off: var("AI_OFF").unwrap_or_default(),
            both_old: var("AI_OLD").is_some(),
        }
    }

    /// Take stage 3 away from `knobs`.
    fn old(&self, knobs: &mut gc_ai::Knobs) {
        if self.off.is_empty() || self.off == "team_play" {
            knobs.team_play = false;
        }
        if self.off.is_empty() || self.off == "read_ground" {
            knobs.read_ground = false;
        }
    }

    /// Play every seed, four at a time: `battle(seed)` gives the lobby, the
    /// map and which team plays new; returns each seed's result for the
    /// new team (None: undecided) and length in seconds.
    fn run(
        &self,
        battle: impl Fn(u64) -> (MatchStart, Heightmap, u8) + Sync,
    ) -> Vec<(u64, Option<bool>, u32)> {
        let seeds: Vec<u64> = (self.from..self.from + self.battles).collect();
        std::thread::scope(|s| {
            let jobs: Vec<_> = seeds
                .chunks(seeds.len().div_ceil(4).max(1))
                .map(|chunk| {
                    let battle = &battle;
                    s.spawn(move || {
                        chunk
                            .iter()
                            .map(|&seed| {
                                let (setup, map, new_team) = battle(seed);
                                let teams: Vec<u8> = setup.players.iter().map(|p| p.team).collect();
                                let played = play_on(&setup, seed, 12, map, |minds| {
                                    for (p, &team) in teams.iter().enumerate() {
                                        if team == new_team && !self.both_old {
                                            continue;
                                        }
                                        let Some(m) = minds.remove(p as PlayerId) else {
                                            continue;
                                        };
                                        let mut knobs = *m.knobs();
                                        self.old(&mut knobs);
                                        minds.add(Mind::with_knobs(m.config(), knobs));
                                    }
                                });
                                let won = played.holder.map(|h| teams[h as usize] == new_team);
                                (seed, won, played.ticks / gc_sim::TICKS_PER_SECOND)
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            jobs.into_iter().flat_map(|j| j.join().unwrap()).collect()
        })
    }

    /// Print each battle and the totals, with the new side's wins by
    /// `place` (a seed's place is `seed % places`).
    fn report(&self, what: &str, results: &[(u64, Option<bool>, u32)], places: u64) {
        let mut by_place = vec![(0, 0); places as usize];
        for &(seed, won, secs) in results {
            println!("seed {seed:>3}: {won:?} after {secs} s");
            let place = &mut by_place[(seed % places) as usize];
            place.1 += 1;
            if won == Some(true) {
                place.0 += 1;
            }
        }
        let won = results.iter().filter(|r| r.1 == Some(true)).count();
        let lost = results.iter().filter(|r| r.1 == Some(false)).count();
        println!(
            "{} {what}: new side won {won}, lost {lost}, undecided {} of {} ({:.0}%); by place {by_place:?}",
            self.level.name(),
            results.len() - won - lost,
            results.len(),
            100.0 * won as f64 / results.len().max(1) as f64,
        );
    }
}

/// Six-player team games. `AI_MODE=3v3` (the default): three against
/// three, the new team taking the south and the north in turn;
/// `AI_MODE=2v2v2`: three pairs, the new pair taking each column in turn
/// (the middle one is between the other two). Default 24 battles.
#[test]
#[ignore = "slow: dozens of six-player battles, run by hand"]
fn team_table() {
    let table = Table::from_env(24);
    let pairs = std::env::var("AI_MODE").as_deref() == Ok("2v2v2");
    let (teams, places): (Vec<u8>, u64) = if pairs {
        (vec![0, 1, 2, 0, 1, 2], 3)
    } else {
        (vec![0, 1, 0, 1, 0, 1], 2)
    };
    let results = table.run(|seed| {
        let setup = lobby(&[Some(table.level); 6], &teams);
        (setup, Heightmap::generate(seed, MAP), (seed % places) as u8)
    });
    table.report(if pairs { "2v2v2" } else { "3v3" }, &results, places);
}

/// What a terrain battlefield is built around.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ground {
    /// A hill somewhere between the armies, nearer one side or the other.
    Hill,
    /// A river across the field, crossed only at a ford or two.
    River,
    /// A river with two fords far apart.
    Fords,
    /// Woods scattered between the armies.
    Woods,
    /// Some of each.
    Mixed,
    /// The generated map every Field Battle has played on.
    Generated,
}

impl Ground {
    fn named(name: &str) -> Ground {
        match name {
            "hill" => Ground::Hill,
            "river" => Ground::River,
            "fords" => Ground::Fords,
            "woods" => Ground::Woods,
            "mixed" => Ground::Mixed,
            "generated" => Ground::Generated,
            _ => panic!("no ground called {name:?}: hill, river, fords, woods, mixed or generated"),
        }
    }
}

/// A battlefield for two armies starting in the west of the map at the
/// north-west and south-west plateaus, 128 tiles apart, with `ground`
/// between them, laid out by `seed`.
fn battlefield(ground: Ground, seed: u64) -> Heightmap {
    if ground == Ground::Generated {
        return Heightmap::generate(seed, MAP);
    }
    let mut rng = gc_fixed::Pcg32::new(seed, 0x7e44);
    let mut roll = |lo: i32, hi: i32| lo + rng.below((hi - lo + 1) as u32) as i32;
    let (base, water) = (40, 24);
    let mut terrain = Vec::new();
    let mut cover = Vec::new();
    let disc = |at, r| Shape::Disc { at, r };
    // Between the armies: x about 64 (both start there), y from 64 to 192.
    let hill = matches!(ground, Ground::Hill | Ground::Mixed);
    let river = matches!(ground, Ground::River | Ground::Fords | Ground::Mixed);
    let woods = matches!(ground, Ground::Woods | Ground::Mixed);
    if hill {
        // A hill two tiles high, a little nearer one side than the other.
        let at = (64 + roll(-30, 30), 128 + roll(-36, 36));
        terrain.push(TerrainOp {
            shape: disc(at, roll(6, 12)),
            mode: Mode::Raise,
            height: base + roll(24, 40),
            edge: roll(6, 10),
        });
    }
    if river {
        // Deep water across the middle, wadeable at one or two fords.
        let (y, drift) = (128 + roll(-12, 12), roll(-10, 10));
        terrain.push(TerrainOp {
            shape: Shape::Line {
                a: (0, y),
                b: (256, y + drift),
                half_width: roll(3, 5),
            },
            mode: Mode::Set,
            height: 6,
            edge: 3,
        });
        let fords: Vec<i32> = if ground == Ground::Fords {
            let at = roll(-10, 10);
            vec![64 + at - 40, 64 + at + 40]
        } else {
            (0..roll(1, 2)).map(|_| 64 + roll(-40, 40)).collect()
        };
        for x in fords {
            terrain.push(TerrainOp {
                // On the river where it runs past x.
                shape: disc((x, y + drift * x / 256), roll(4, 7)),
                mode: Mode::Set,
                height: water - 4,
                edge: 3,
            });
        }
    }
    if woods {
        for _ in 0..roll(3, 6) {
            cover.push(CoverOp {
                shape: disc((64 + roll(-45, 45), 128 + roll(-40, 40)), roll(5, 10)),
                kind: CoverKind::Wood,
            });
        }
    }
    let def = MapDef {
        name: format!("{ground:?} {seed}"),
        size: SizeClass::Warfield,
        symmetry: Symmetry::None,
        players: 2,
        base,
        water,
        noise: None,
        terrain,
        cover,
        objects: Vec::new(),
        set_pieces: Vec::new(),
    };
    Heightmap::from_def(&def)
}

/// One against one on terrain battlefields: `AI_GROUND=hill` (the
/// default), `river`, `fords` (a river with two fords far apart),
/// `woods`, `mixed` or `generated`. The new side takes
/// the south and the north in turn. Default 60 battles.
#[test]
#[ignore = "slow: dozens of battles, run by hand"]
fn terrain_table() {
    let table = Table::from_env(60);
    let ground = Ground::named(&std::env::var("AI_GROUND").unwrap_or_else(|_| "hill".into()));
    let results = table.run(|seed| {
        let setup = lobby(&[Some(table.level); 2], &[0, 1]);
        (setup, battlefield(ground, seed), (seed % 2) as u8)
    });
    table.report(&format!("{ground:?}"), &results, 2);
}
