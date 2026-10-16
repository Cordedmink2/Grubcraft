//! Conquest maps from a seed that the checker passes: generate, check,
//! and reroll to the next seed when the map fails.

use crate::check::check;
use gc_map::conquest::{ConquestRecipe, describe};
use gc_map::{MapDef, SizeClass};

/// How many seeds [`generate`] tries before giving up.
pub const BUDGET: u32 = 16;

/// The first map from `seed`, `seed + 1`, ... (up to [`BUDGET`] seeds) that
/// the checker passes, with the seed that made it. The error lists why
/// each seed failed (its first error), or says the recipe doesn't make
/// that size.
pub fn generate(
    recipe: ConquestRecipe,
    size: SizeClass,
    seed: u64,
) -> Result<(u64, MapDef), Vec<String>> {
    let mut why = Vec::new();
    for k in 0..u64::from(BUDGET) {
        let tried = seed.wrapping_add(k);
        let Some(def) = describe(recipe, size, tried) else {
            return Err(vec![format!(
                "{} maps aren't made at {size:?} size",
                recipe.name()
            )]);
        };
        let report = check(&def);
        match report.errors.first() {
            None => return Ok((tried, def)),
            Some(first) => why.push(format!("seed {tried}: {first}")),
        }
    }
    Err(why)
}
