//! Recipes must make maps the checker passes. This sweeps a few seeds of
//! each recipe, at both Field Battle sizes, through the checker. When a
//! recipe is tuned, the build also sweeps 40+ seeds by hand, with
//! `gc_mapcheck gen RECIPE TILES SEED OUT.ron` and `gc_mapcheck check`.

use gc_map::recipe::{Recipe, describe};
use gc_mapcheck::check;

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
