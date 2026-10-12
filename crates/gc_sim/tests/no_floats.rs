//! Floating point is banned from the simulation and its math library, because
//! float results can differ between CPUs and compilers and would desync
//! lockstep multiplayer. Test modules (after `#[cfg(test)]`) may use floats to
//! check results against a reference.

use std::path::Path;

fn check_dir(dir: &Path, offenders: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            check_dir(&path, offenders);
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
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
}

#[test]
fn simulation_crates_contain_no_floats() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();
    check_dir(&root.join("src"), &mut offenders);
    check_dir(&root.join("../gc_fixed/src"), &mut offenders);
    assert!(
        offenders.is_empty(),
        "floating point in simulation code:\n{}",
        offenders.join("\n")
    );
}
