//! The credits screen: every table row of `CREDITS.md`, as plain lines.

const CREDITS: &str = include_str!("../../../CREDITS.md");

/// Lines for the credits panel: "Asset, by Author (licence)" for each
/// credited asset, grouped under the file's headings.
pub fn lines() -> Vec<String> {
    let mut out = vec!["CREDITS   C closes".to_string()];
    for line in CREDITS.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            out.push(String::new());
            out.push(ascii(heading).to_uppercase());
            continue;
        }
        let Some(row) = line.strip_prefix('|') else {
            continue;
        };
        let cells: Vec<&str> = row.split('|').map(str::trim).collect();
        if cells.len() < 3 || cells[0].starts_with("---") || matches!(cells[0], "Asset" | "Model") {
            continue;
        }
        // A licence column holds "CC..." (possibly as a Markdown link).
        let licence = cells
            .iter()
            .skip(2)
            .find(|c| c.contains("CC"))
            .map(|c| strip_link(c));
        let mut text = format!("{}, by {}", strip_link(cells[0]), strip_link(cells[1]));
        if let Some(l) = licence {
            text += &format!(" ({l})");
        }
        out.push(ascii(&text));
    }
    out
}

/// "[text](url)" becomes "text".
fn strip_link(s: &str) -> String {
    match (s.find('['), s.find("](")) {
        (Some(a), Some(b)) if a < b => s[a + 1..b].to_string(),
        _ => s.to_string(),
    }
}

/// The overlay font is ASCII only.
fn ascii(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{2013}' | '\u{2014}' => '-',
            '\u{2018}' | '\u{2019}' => '\'',
            c if c.is_ascii() => c,
            _ => '?',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn credits_name_every_cc_by_author() {
        let lines = super::lines();
        for author in ["null-painter-error", "PantherOne", "Quaternius", "CDmir"] {
            assert!(
                lines.iter().any(|l| l.contains(author)),
                "{author} missing from {lines:#?}"
            );
        }
        assert!(lines.iter().all(|l| l.is_ascii()));
        assert!(lines.iter().any(|l| l.contains("(CC-BY 4.0)")));
    }
}
