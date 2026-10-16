//! The manifest that sits beside a baked pack, recording what made it.
//!
//! A pack is a binary blob, so nothing in it says which sources or baker it
//! came from. Each baker writes a small text file next to its pack (a line
//! per fact, `key = value`) and CI checks that the committed pack, its
//! sources and the baker's version still agree with it. Hashes are
//! [`hash_hex`]: XXH3 128-bit, which is for noticing a stale file, not for
//! security.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

/// An ordered list of `key = value` facts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    entries: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestError {
    /// A line (1-based) that is neither blank, a `#` comment nor `key = value`.
    BadLine(usize),
    DuplicateKey(String),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::BadLine(n) => write!(f, "line {n} is not `key = value`"),
            ManifestError::DuplicateKey(k) => write!(f, "`{k}` appears twice"),
        }
    }
}

impl core::error::Error for ManifestError {}

impl Manifest {
    pub fn new() -> Manifest {
        Manifest::default()
    }

    /// Append a fact. Keys must be unique and single-line.
    pub fn set(&mut self, key: &str, value: &str) {
        assert!(
            self.get(key).is_none() && !key.contains(['\n', '=']) && !value.contains('\n'),
            "bad or repeated manifest key {key:?}"
        );
        self.entries.push((key.to_string(), value.to_string()));
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Every fact in order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// The file's text, led by `header` as `#` comment lines.
    pub fn render(&self, header: &str) -> String {
        let mut out = String::new();
        for line in header.lines() {
            out.push_str(&format!("# {line}\n"));
        }
        for (k, v) in &self.entries {
            out.push_str(&format!("{k} = {v}\n"));
        }
        out
    }

    pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
        let mut m = Manifest::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (k, v) = line
                .split_once(" = ")
                .ok_or(ManifestError::BadLine(n + 1))?;
            if m.get(k).is_some() {
                return Err(ManifestError::DuplicateKey(k.to_string()));
            }
            m.entries.push((k.to_string(), v.to_string()));
        }
        Ok(m)
    }
}

/// A content hash as 32 lowercase hex digits (XXH3, 128-bit).
pub fn hash_hex(bytes: &[u8]) -> String {
    format!("{:032x}", xxhash_rust::xxh3::xxh3_128(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_then_parse() {
        let mut m = Manifest::new();
        m.set("baker", "gc_artbake 1");
        m.set("source repo:a/b.glb", &hash_hex(b"bytes"));
        let text = m.render("Written by a baker.\nDo not edit.");
        assert!(text.starts_with("# Written by a baker.\n# Do not edit.\nbaker = gc_artbake 1\n"));
        assert_eq!(Manifest::parse(&text), Ok(m));
    }

    #[test]
    fn rejects_bad_text() {
        assert_eq!(
            Manifest::parse("a = 1\nnonsense\n"),
            Err(ManifestError::BadLine(2))
        );
        assert_eq!(
            Manifest::parse("a = 1\na = 2\n"),
            Err(ManifestError::DuplicateKey("a".into()))
        );
    }

    #[test]
    fn hashes_differ_by_content() {
        assert_eq!(hash_hex(b"x").len(), 32);
        assert_ne!(hash_hex(b"x"), hash_hex(b"y"));
        assert_eq!(hash_hex(b"x"), hash_hex(b"x"));
    }
}
