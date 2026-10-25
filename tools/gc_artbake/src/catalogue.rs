//! `gc_artbake --catalogue assets-src/catalogue.csv`: checks the art catalogue
//! (research design pack 04) against the baker configs, so each generated asset
//! later drops in as a file swap.
//!
//! Every catalogue row names the model entry it lands in (`model`, in the baker
//! config `pack`) and a `status`:
//! - `placeholder`: a stand-in model is wired; the Meshy asset replaces its `source`.
//! - `retheme-pending`: the existing model entry stays until the re-themed asset replaces it.
//! - `unmodelled`, `new-unmodelled`: no entry yet; the unit draws as before. `model` and `pack` are empty.
//!
//! The check fails when a row names a model its pack does not hold, when a
//! status contradicts the row, or when an ID repeats. It never calls Meshy.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

const STATUSES: [&str; 4] = [
    "placeholder",
    "retheme-pending",
    "unmodelled",
    "new-unmodelled",
];

#[derive(Debug, PartialEq)]
pub(crate) struct Row {
    pub id: String,
    pub name: String,
    pub model: String,
    pub pack: String,
    pub status: String,
}

/// The rows of a catalogue file (`#` lines and the header skipped).
pub(crate) fn parse(text: &str) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') || line.starts_with("id,") {
            continue;
        }
        let f: Vec<&str> = line.split(',').collect();
        if f.len() != 8 {
            return Err(format!("line {}: {} fields, want 8", n + 1, f.len()));
        }
        rows.push(Row {
            id: f[0].into(),
            name: f[2].into(),
            model: f[5].into(),
            pack: f[6].into(),
            status: f[7].into(),
        });
    }
    Ok(rows)
}

/// The names of the model entries in a baker config's text.
pub(crate) fn model_names(config: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut last: Option<String> = None;
    for line in config.lines() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        if let Some(rest) = t.strip_prefix("name: \"") {
            last = rest.split('"').next().map(str::to_string);
        } else if t.starts_with("source:")
            && let Some(n) = last.take()
        {
            names.insert(n);
        }
    }
    names
}

/// Every problem in `rows`, given the model names each pack file holds.
pub(crate) fn problems(
    rows: &[Row],
    packs: &dyn Fn(&str) -> Option<BTreeSet<String>>,
) -> Vec<String> {
    let mut bad = Vec::new();
    let mut seen = HashSet::new();
    for r in rows {
        if !seen.insert(&r.id) {
            bad.push(format!("{}: id repeated", r.id));
        }
        if !STATUSES.contains(&r.status.as_str()) {
            bad.push(format!("{}: unknown status {:?}", r.id, r.status));
            continue;
        }
        let wired = r.status == "placeholder" || r.status == "retheme-pending";
        if wired != !r.model.is_empty() {
            bad.push(format!(
                "{} {}: status {} but model {:?}",
                r.id, r.name, r.status, r.model
            ));
            continue;
        }
        if wired {
            match packs(&r.pack) {
                None => bad.push(format!("{} {}: no config {}", r.id, r.name, r.pack)),
                Some(names) if !names.contains(&r.model) => bad.push(format!(
                    "{} {}: {} has no model {:?}",
                    r.id, r.name, r.pack, r.model
                )),
                Some(_) => {}
            }
        }
    }
    bad
}

/// Runs the check: the catalogue file against every config under `src`.
pub(crate) fn check(file: &Path, src: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let rows = parse(&text)?;
    let read = |p: &Path| std::fs::read_to_string(p).ok().map(|t| model_names(&t));
    let packs = |name: &str| {
        if name == "models.ron" {
            read(&src.join(name))
        } else {
            read(&src.join("buildings").join(name))
        }
    };
    let bad = problems(&rows, &packs);
    if bad.is_empty() {
        let wired = rows.iter().filter(|r| !r.model.is_empty()).count();
        println!(
            "{}: {} catalogue rows, {} wired to a model entry, {} not modelled yet",
            file.display(),
            rows.len(),
            wired,
            rows.len() - wired
        );
        Ok(())
    } else {
        Err(format!("catalogue problems:\n  {}", bad.join("\n  ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = "# c\nid,faction,name,kind,phase,model,pack,status\n\
        V-01,Oathbound,Seedtender,soldier,1,Seedtender,oathbound.ron,placeholder\n\
        V-02,Oathbound,Wardens,soldier,1,,,unmodelled\n";

    fn packs(name: &str) -> Option<BTreeSet<String>> {
        (name == "oathbound.ron").then(|| model_names("name: \"Seedtender\",\n source: \"x\",\n"))
    }

    #[test]
    fn parses_rows_and_skips_comments() {
        let rows = parse(CSV).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].model, "Seedtender");
        assert!(parse("a,b").is_err());
    }

    #[test]
    fn a_consistent_catalogue_has_no_problems() {
        assert!(problems(&parse(CSV).unwrap(), &packs).is_empty());
    }

    #[test]
    fn missing_models_repeats_and_contradictions_are_found() {
        let mut rows = parse(CSV).unwrap();
        rows[0].model = "Gone".into();
        rows.push(Row {
            id: "V-02".into(),
            ..parse(CSV).unwrap().remove(1)
        });
        rows[1].status = "placeholder".into();
        let bad = problems(&rows, &packs);
        assert!(
            bad.iter().any(|b| b.contains("no model \"Gone\"")),
            "{bad:?}"
        );
        assert!(bad.iter().any(|b| b.contains("id repeated")), "{bad:?}");
        assert!(
            bad.iter().any(|b| b.contains("status placeholder")),
            "{bad:?}"
        );
    }

    #[test]
    fn model_names_ignore_comments_and_entries_without_a_source() {
        let t = "// name: \"Not\",\n name: \"Is\",\n source: \"a\",\n stats: (name: \"X\",\n";
        assert_eq!(model_names(t), BTreeSet::from(["Is".to_string()]));
    }
}
