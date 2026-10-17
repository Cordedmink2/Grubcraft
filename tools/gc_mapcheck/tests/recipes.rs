//! Recipes must make maps the checker passes. This sweeps a few seeds of
//! each Field Battle recipe, at both sizes, of each Conquest recipe at
//! each size, and of every kind of campaign battle map, through the
//! checker. When a recipe is tuned, the build also looks at a few by hand,
//! with `gc_mapcheck gen RECIPE SIZE SEED OUT.ron` (or `gc_mapcheck
//! battle`), `gc_mapcheck check` and `gc_mapcheck png`.

use gc_map::Kind;
use gc_map::SizeClass;
use gc_map::battle::{DEFENDER, FORT_RADIUS, Region, battle_map};
use gc_map::conquest::{self, CONQUEST_SIZES, ConquestRecipe};
use gc_map::recipe::{Recipe, describe};
use gc_mapcheck::Map;
use gc_mapcheck::map::Ground;
use gc_mapcheck::{check, generate};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

#[test]
fn every_recipe_makes_maps_the_checker_passes() {
    let mut failures = Vec::new();
    for recipe in Recipe::ALL {
        for tiles in [256, 320] {
            for seed in 1..=12 {
                let report = check(&describe(recipe, tiles, seed));
                if let Some(first) = report.errors.first() {
                    failures.push(format!("{} {tiles} seed {seed}: {first}", recipe.name()));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} recipe maps failed the checker:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The share of seeds of each Conquest recipe and size that must make a
/// map the checker passes, before any reroll.
const CONQUEST_PASS_PERCENT: usize = 80;

/// How many seeds the pass-rate test checks at a size: fewer on the
/// bigger maps, which take longer to check. A broken recipe fails most
/// seeds, so a dozen is enough to catch one.
fn seeds_at(size: SizeClass) -> u64 {
    match size {
        SizeClass::Duel => 50,
        SizeClass::Skirmish => 25,
        _ => 12,
    }
}

/// `fail` for every job, on every core, each taking the next job until
/// none are left: the failures it reports.
fn failures<J: Sync, F: Fn(&J) -> Option<T> + Sync, T: Send>(jobs: &[J], fail: F) -> Vec<T> {
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    std::thread::scope(|scope| {
        let running: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut failed = Vec::new();
                    while let Some(job) = jobs.get(next.fetch_add(1, Relaxed)) {
                        failed.extend(fail(job));
                    }
                    failed
                })
            })
            .collect();
        running
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    })
}

/// Most seeds of every Conquest recipe make a map the checker passes, so
/// a change that breaks a recipe's fairness fails the build.
#[test]
fn conquest_recipes_pass_the_checker_for_most_seeds() {
    let jobs: Vec<(ConquestRecipe, SizeClass, u64)> = ConquestRecipe::ALL
        .into_iter()
        .flat_map(|r| CONQUEST_SIZES.map(|s| (r, s)))
        .flat_map(|(r, s)| (1..=seeds_at(s)).map(move |seed| (r, s, seed)))
        .collect();
    // Each failure as (recipe and size, its first error).
    let failures: Vec<(String, String)> = failures(&jobs, |&(recipe, size, seed)| {
        let report = check(&conquest::describe(recipe, size, seed).unwrap());
        let e = report.errors.first()?;
        Some((
            format!("{} {size:?}", recipe.name()),
            format!("seed {seed}: {e}"),
        ))
    });
    let low: Vec<String> = ConquestRecipe::ALL
        .into_iter()
        .flat_map(|r| CONQUEST_SIZES.map(|s| (r, s)))
        .filter_map(|(recipe, size)| {
            let what = format!("{} {size:?}", recipe.name());
            let seeds = seeds_at(size) as usize;
            let failed: Vec<&String> = failures
                .iter()
                .filter(|f| f.0 == what)
                .map(|f| &f.1)
                .collect();
            let passed = seeds - failed.len();
            (passed * 100 < seeds * CONQUEST_PASS_PERCENT)
                .then(|| format!("{what}: {passed}/{seeds} ({:?})", failed.first()))
        })
        .collect();
    assert!(
        low.is_empty(),
        "recipes below {CONQUEST_PASS_PERCENT}%:\n{}",
        low.join("\n")
    );
}

/// The reroll loop hands back a map the checker passes, from the first
/// seed that makes one, and refuses a size the recipes don't make.
#[test]
fn generate_rerolls_to_a_map_the_checker_passes() {
    for recipe in ConquestRecipe::ALL {
        let (seed, def) = generate(recipe, SizeClass::Duel, 7).unwrap();
        assert!(seed >= 7, "{} seed {seed}", recipe.name());
        assert_eq!(def.size, SizeClass::Duel);
        let report = check(&def);
        assert!(
            report.ok(),
            "{} seed {seed}: {:?}",
            recipe.name(),
            report.errors
        );
    }
    let err = generate(ConquestRecipe::Fen, SizeClass::Grand, 1).unwrap_err();
    assert!(err[0].contains("Grand"), "{err:?}");
}

/// Every kind of campaign battle map (each area, route and feature, with
/// and without a Fort) makes maps the checker passes: the starts reach
/// each other, nothing stands on a cliff or in the water, and the hills
/// are hills. Campaign battles aren't rerolled, so every seed must pass;
/// a wider sweep of 20 seeds of each passed when the maps were made
/// Field Battle size.
#[test]
fn every_campaign_battle_map_passes_the_checker() {
    let jobs: Vec<(Region, u64)> = Region::all()
        .enumerate()
        // A different seed for each, so the sweep covers more of them.
        .map(|(k, region)| (region, k as u64))
        .collect();
    let failed = failures(&jobs, |&(region, seed)| {
        let report = check(&battle_map(region, seed));
        let e = report.errors.first()?;
        Some(format!("{region:?} seed {seed}: {e}"))
    });
    assert!(
        failed.is_empty(),
        "{} battle maps failed the checker:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

/// A Fort's palisade closes round the defender's start but for its gate:
/// with the gate shut, nothing walks out. (The checker can't see this: the
/// defender still reaches the attacker through the gate.)
#[test]
fn a_forts_palisade_is_shut_but_for_its_gate() {
    let jobs: Vec<(Region, u64)> = Region::all()
        .filter(|r| r.fort)
        .enumerate()
        .map(|(k, region)| (region, k as u64))
        .collect();
    let failed = failures(&jobs, |&(region, seed)| {
        let def = battle_map(region, seed);
        let map = Map::build(&def);
        let n = map.n;
        let (cx, cy) = def
            .objects
            .iter()
            .find(|o| o.kind == Kind::Start && o.owner == Some(DEFENDER))?
            .at;
        let r = FORT_RADIUS * n / 256;
        // The gate's opening faces south, about 0.38 of the radius either
        // side of the middle.
        let gate = |(x, y): (i32, i32)| y >= cy + r * 4 / 5 && (x - cx).abs() <= r / 2;
        let mut seen = vec![false; (n * n) as usize];
        let mut todo = vec![(cx, cy)];
        while let Some((x, y)) = todo.pop() {
            if (x - cx).pow(2) + (y - cy).pow(2) > (r + 4).pow(2) {
                return Some(format!("{region:?} seed {seed}: out at ({x}, {y})"));
            }
            for p in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                let inside = (0..n).contains(&p.0) && (0..n).contains(&p.1);
                if !inside || gate(p) {
                    continue;
                }
                let i = (p.1 * n + p.0) as usize;
                if !seen[i] && map.ground_at(p) != Ground::Blocked {
                    seen[i] = true;
                    todo.push(p);
                }
            }
        }
        None
    });
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
