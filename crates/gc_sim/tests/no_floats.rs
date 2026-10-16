//! Floating point is banned from the simulation and its math library, because
//! float results can differ between CPUs and compilers and would desync
//! lockstep multiplayer. Test modules (after `#[cfg(test)]`) may use floats to
//! check results against a reference. Map making (`gc_map`, apart from its
//! drawing views in `view.rs`) is held to the same rule.

use std::path::Path;

fn check_file(path: &Path, offenders: &mut Vec<String>) {
    let src = std::fs::read_to_string(path).unwrap();
    let non_test = src.split("#[cfg(test)]").next().unwrap();
    for (n, line) in non_test.lines().enumerate() {
        let code = line.split("//").next().unwrap();
        let has_float = code
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .any(|tok| {
                tok == "f32" || tok == "f64" || tok.ends_with("_f32") || tok.ends_with("_f64")
            });
        if has_float {
            offenders.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
        }
    }
}

/// Every `.rs` file under `dir` except those named in `skip`.
fn check_dir(dir: &Path, skip: &[&str], offenders: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            check_dir(&path, skip, offenders);
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs")
            || path
                .file_name()
                .is_some_and(|n| skip.iter().any(|s| n == *s))
        {
            continue;
        }
        check_file(&path, offenders);
    }
}

#[test]
fn simulation_crates_contain_no_floats() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();
    check_dir(&root.join("src"), &[], &mut offenders);
    check_dir(&root.join("../gc_fixed/src"), &[], &mut offenders);
    // Map making must make the same map on every machine; its drawing
    // views are floats on purpose.
    check_dir(&root.join("../gc_map/src"), &["view.rs"], &mut offenders);
    assert!(
        offenders.is_empty(),
        "floating point in simulation code:\n{}",
        offenders.join("\n")
    );
}
