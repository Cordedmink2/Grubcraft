//! A Field Battle over the network (`gc_net`), as this machine sees it.
//!
//! What changes from a match on one machine: turns come from the host
//! (`Lan::next_turn`) instead of this machine's clock, this machine's
//! orders go to the host, the computer players think only on the host, the
//! sim keeps running behind the menu, and the sandbox tools (pause, speed,
//! switching sides, placing, tuning) are off because they would change one
//! machine's game and not the others'.

use std::time::{Duration, Instant};

/// How long a notice ("Alex lost connection") stays on screen.
const NOTICE_TIME: Duration = Duration::from_secs(8);

/// The build, as players compare it in the lobby: the version and the
/// commit it was built from.
pub fn version() -> String {
    format!("{} {}", env!("CARGO_PKG_VERSION"), env!("GRUBCRAFT_COMMIT"))
}

/// A fingerprint of a map's walkable ground (FNV-1a), so a joiner whose
/// map came out differently finds out before the battle starts.
pub fn map_hash(nav: &[u8], ground: &gc_sim::Ground) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |b: u8| {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    };
    nav.iter().for_each(|&b| eat(b));
    for v in ground.heights.iter().chain([&ground.water_level]) {
        v.to_le_bytes().into_iter().for_each(&mut eat);
    }
    h ^ nav.len() as u64
}

/// A networked match under way.
pub struct Online {
    /// This machine is the host (it runs the computer players).
    pub host: bool,
    /// The latest notice and when it arrived.
    pub notice: Option<(String, Instant)>,
    /// Set once a state hash disagreed with the host's.
    pub desync: Option<u32>,
    /// Set when the match is over for this machine, with why.
    pub ended: Option<String>,
    /// When the last turn was simulated, for smooth movement in between.
    pub last_turn: Instant,
}

impl Online {
    pub fn new(host: bool) -> Online {
        Online {
            host,
            notice: None,
            desync: None,
            ended: None,
            last_turn: Instant::now(),
        }
    }

    pub fn note(&mut self, text: String) {
        eprintln!("grubcraft: {text}");
        self.notice = Some((text, Instant::now()));
    }

    /// Lines for the top of the battle panel.
    pub fn lines(&self, status: Option<String>, waiting: Option<String>) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(why) = &self.ended {
            lines.push(format!(
                "NETWORK  the match is over here: {why}   Esc: menu"
            ));
            return lines;
        }
        lines.push(format!(
            "NETWORK  {}",
            status.unwrap_or_else(|| "not connected".into())
        ));
        if let Some(w) = waiting {
            lines.push(w);
        }
        if let Some(tick) = self.desync {
            lines.push(format!(
                "Out of sync with the host since turn {tick}: this battle may look different on each machine."
            ));
        }
        if let Some((text, at)) = &self.notice
            && at.elapsed() < NOTICE_TIME
        {
            lines.push(text.clone());
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_hash_sees_every_cell() {
        let flat = gc_sim::Ground::FLAT;
        let a = vec![1u8; 1000];
        let mut b = a.clone();
        b[999] = 2;
        assert_ne!(map_hash(&a, &flat), map_hash(&b, &flat));
        assert_ne!(map_hash(&a, &flat), map_hash(&a[..999], &flat));
        assert_eq!(map_hash(&a, &flat), map_hash(&a.clone(), &flat));
        // Heights and water count too.
        let heights = vec![7u16; 1000];
        let mut raised = heights.clone();
        raised[3] = 8;
        let ground = |heights, water_level| gc_sim::Ground {
            heights,
            water_level,
        };
        let base = map_hash(&a, &ground(&heights, 2));
        assert_ne!(base, map_hash(&a, &ground(&raised, 2)));
        assert_ne!(base, map_hash(&a, &ground(&heights, 3)));
        assert_ne!(base, map_hash(&a, &flat));
    }

    #[test]
    fn version_names_the_commit() {
        assert!(version().starts_with(env!("CARGO_PKG_VERSION")));
        assert!(version().split(' ').nth(1).is_some_and(|c| !c.is_empty()));
    }
}
