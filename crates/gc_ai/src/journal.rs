//! The decision journal (docs/18-ai.md, "The AI lens"): a short running log
//! of what a mind decided and why, in plain words, for the AI lens overlay,
//! the sandbox panel and tests.

use std::collections::VecDeque;

/// Entries kept; older ones fall off the front.
const KEEP: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Simulation tick it was decided on.
    pub tick: u32,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct Journal {
    entries: VecDeque<Entry>,
}

impl Journal {
    pub fn new() -> Journal {
        Journal::default()
    }

    pub fn note(&mut self, tick: u32, text: String) {
        if self.entries.len() == KEEP {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry { tick, text });
    }

    /// Oldest first.
    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &Entry> {
        self.entries.iter()
    }

    pub fn latest(&self) -> Option<&Entry> {
        self.entries.back()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_most_recent_entries() {
        let mut j = Journal::new();
        for t in 0..100 {
            j.note(t, format!("entry {t}"));
        }
        assert_eq!(j.entries().count(), KEEP);
        assert_eq!(j.entries().next().unwrap().tick, 100 - KEEP as u32);
        assert_eq!(j.latest().unwrap().text, "entry 99");
    }
}
