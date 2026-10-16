//! `gc_mapcheck check [FILE|DIR]...` validates maps (default `data/maps`);
//! `gc_mapcheck png FILE OUT.png` draws one; `gc_mapcheck gen RECIPE SIZE
//! SEED OUT.ron` writes the map a recipe makes from a seed, so a good one
//! can become a hand-tuned map: a Field Battle recipe (Pass, Ford, Hill or
//! Open) takes a size in tiles, a Conquest recipe (Teeth, Bowl or Fen)
//! takes Duel, Skirmish or Warfield and rerolls the seed until the checker
//! passes the map.

use gc_map::conquest::{CONQUEST_SIZES, ConquestRecipe};
use gc_mapcheck::{Map, MapDef, check, generate, preview};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn png_cmd(src: &str, out: &str) -> Result<(), String> {
    let map = Map::build(&MapDef::load(Path::new(src))?);
    let (side, rgb) = preview::render(&map, if map.tiles > 192 { 3 } else { 4 });
    std::fs::write(out, preview::png(side, &rgb)).map_err(|e| format!("{out}: {e}"))
}

fn gen_cmd(recipe: &str, size: &str, seed: &str, out: &str) -> Result<(), String> {
    let seed = seed.parse().map_err(|_| format!("{seed} isn't a seed"))?;
    let named = |name: &str| name.eq_ignore_ascii_case(recipe);
    let def = if let Some(field) = gc_map::recipe::Recipe::ALL
        .into_iter()
        .find(|r| named(r.name()))
    {
        let tiles = size
            .parse()
            .map_err(|_| format!("{size} isn't a size in tiles"))?;
        gc_map::recipe::describe(field, tiles, seed)
    } else if let Some(conquest) = ConquestRecipe::ALL.into_iter().find(|r| named(r.name())) {
        let size = CONQUEST_SIZES
            .into_iter()
            .find(|s| format!("{s:?}").eq_ignore_ascii_case(size))
            .ok_or_else(|| format!("{size} isn't Duel, Skirmish or Warfield"))?;
        let (used, def) = generate(conquest, size, seed).map_err(|e| e.join("\n"))?;
        if used != seed {
            eprintln!("seed {seed} failed the checker; seed {used} passed");
        }
        def
    } else {
        return Err(format!("no recipe called {recipe}"));
    };
    std::fs::write(out, def.to_ron()).map_err(|e| format!("{out}: {e}"))
}

fn files(args: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let roots: Vec<PathBuf> = if args.is_empty() {
        vec!["data/maps".into()]
    } else {
        args.iter().map(PathBuf::from).collect()
    };
    for root in roots {
        if root.is_dir() {
            let entries = match std::fs::read_dir(&root) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("{}: {e}", root.display());
                    continue;
                }
            };
            let mut found: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "ron"))
                .collect();
            found.sort();
            out.extend(found);
        } else {
            out.push(root);
        }
    }
    out
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") | None => {
            let paths = files(args.get(1..).unwrap_or(&[]));
            if paths.is_empty() {
                eprintln!("no maps found");
                return ExitCode::FAILURE;
            }
            let mut failed = false;
            for path in paths {
                match MapDef::load(&path) {
                    Err(e) => {
                        println!("{e}");
                        failed = true;
                    }
                    Ok(def) => {
                        let r = check(&def);
                        println!("{} ({}) hash {}", r.name, path.display(), r.hash);
                        for n in &r.notes {
                            println!("    {n}");
                        }
                        for w in &r.warnings {
                            println!("  warning: {w}");
                        }
                        for e in &r.errors {
                            println!("  ERROR: {e}");
                        }
                        println!("  {}", if r.ok() { "ok" } else { "FAILED" });
                        failed |= !r.ok();
                    }
                }
            }
            if failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Some("png") if args.len() == 3 => match png_cmd(&args[1], &args[2]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Some("gen") if args.len() == 5 => match gen_cmd(&args[1], &args[2], &args[3], &args[4]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!(
                "usage: gc_mapcheck check [FILE|DIR]...\n       gc_mapcheck png FILE OUT.png\n       gc_mapcheck gen RECIPE SIZE SEED OUT.ron"
            );
            ExitCode::FAILURE
        }
    }
}
