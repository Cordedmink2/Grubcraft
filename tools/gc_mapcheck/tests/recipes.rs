//! Recipes must make maps the checker passes. This sweeps a few seeds of
//! each Field Battle recipe, at both sizes, and of each Conquest recipe at
//! each size, through the checker. When a recipe is tuned, the
//! build also looks at a few by hand, with `gc_mapcheck gen RECIPE SIZE
//! SEED OUT.ron`, `gc_mapcheck check` and `gc_mapcheck png`.

use gc_map::SizeClass;
use gc_map::conquest::{self, CONQUEST_SIZES, ConquestRecipe};
use gc_map::recipe::{Recipe, describe};
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

/// Most seeds of every Conquest recipe make a map the checker passes, so
/// a change that breaks a recipe's fairness fails the build. Every core
/// takes the next map to check until none are left.
#[test]
fn conquest_recipes_pass_the_checker_for_most_seeds() {
    let jobs: Vec<(ConquestRecipe, SizeClass, u64)> = ConquestRecipe::ALL
        .into_iter()
        .flat_map(|r| CONQUEST_SIZES.map(|s| (r, s)))
        .flat_map(|(r, s)| (1..=seeds_at(s)).map(move |seed| (r, s, seed)))
        .collect();
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    // Each failure as (recipe and size, its first error).
    let failures: Vec<(String, String)> = std::thread::scope(|scope| {
        let running: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut failed = Vec::new();
                    while let Some(&(recipe, size, seed)) = jobs.get(next.fetch_add(1, Relaxed)) {
                        let report = check(&conquest::describe(recipe, size, seed).unwrap());
                        if let Some(e) = report.errors.first() {
                            failed.push((
                                format!("{} {size:?}", recipe.name()),
                                format!("seed {seed}: {e}"),
                            ));
                        }
                    }
                    failed
                })
            })
            .collect();
        running
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
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
