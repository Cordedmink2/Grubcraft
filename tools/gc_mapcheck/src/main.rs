//! `gc_mapcheck check [FILE|DIR]...` validates maps (default `data/maps`);
//! `gc_mapcheck png FILE OUT.png` draws one.

use gc_mapcheck::{Map, MapDef, check, preview};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn png_cmd(src: &str, out: &str) -> Result<(), String> {
    let map = Map::build(&MapDef::load(Path::new(src))?);
    let (side, rgb) = preview::render(&map, if map.tiles > 192 { 3 } else { 4 });
    std::fs::write(out, preview::png(side, &rgb)).map_err(|e| format!("{out}: {e}"))
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
        _ => {
            eprintln!(
                "usage: gc_mapcheck check [FILE|DIR]...\n       gc_mapcheck png FILE OUT.png"
            );
            ExitCode::FAILURE
        }
    }
}
