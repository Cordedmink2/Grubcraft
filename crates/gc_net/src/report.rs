//! Desync reports (docs/11-networking.md, "Desync detection and recovery").
//!
//! When a player's world stops matching the host's, every machine saves
//! what it played: the match header and every turn's commands, with the
//! state hashes it recorded on the way. The simulation is deterministic,
//! so the header and turns are a full replay of the match. Played back on
//! the build that wrote it, a report reproduces that machine's hashes;
//! comparing two machines' reports shows the first tick they disagree on,
//! and stepping both from there finds the system that diverged.

use gc_sim::{PlayerCommand, PlayerId};
use gc_ui::lobby::{LobbySettings, MAX_PLAYERS, Slot};

use crate::proto::{read_settings, read_slot, write_settings, write_slot};
use crate::wire::{Reader, Writer, read_player_commands, write_player_commands};

const MAGIC: &[u8; 8] = b"GCDESYNC";
/// Bumped whenever the report layout changes, including the lobby settings
/// it carries (2: the champions option).
const FORMAT: u8 = 2;

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// The build that played (version and commit).
    pub version: String,
    pub data_hash: u64,
    /// The match's seed and the host's map hash.
    pub seed: u64,
    pub map_hash: u64,
    pub settings: LobbySettings,
    /// The players in match order.
    pub players: Vec<Slot>,
    /// Which of them this machine played.
    pub me: PlayerId,
    /// The tick the host found the mismatch at (0 if none).
    pub desync_tick: u32,
    /// Every turn this machine simulated, in order: turn `i` is tick `i`.
    pub turns: Vec<Vec<PlayerCommand>>,
    /// This machine's state hash after these ticks.
    pub hashes: Vec<(u32, u64)>,
}

impl Report {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.buf.extend_from_slice(MAGIC);
        w.u8(FORMAT);
        w.str(&self.version);
        w.u64(self.data_hash);
        w.u64(self.seed);
        w.u64(self.map_hash);
        write_settings(&mut w, &self.settings);
        w.var(self.players.len() as u64);
        for s in &self.players {
            write_slot(&mut w, s);
        }
        w.u8(self.me);
        w.var(u64::from(self.desync_tick));
        w.var(self.turns.len() as u64);
        for t in &self.turns {
            write_player_commands(&mut w, t);
        }
        w.var(self.hashes.len() as u64);
        for &(tick, hash) in &self.hashes {
            w.var(u64::from(tick));
            w.u64(hash);
        }
        w.buf
    }

    /// `None` if the bytes are not a report this build can read.
    pub fn decode(bytes: &[u8]) -> Option<Report> {
        let mut r = Reader::new(bytes);
        if r.take(MAGIC.len())? != MAGIC || r.u8()? != FORMAT {
            return None;
        }
        let version = r.str()?;
        let data_hash = r.u64()?;
        let seed = r.u64()?;
        let map_hash = r.u64()?;
        let settings = read_settings(&mut r)?;
        let n = r.var_as::<usize>()?;
        if n > MAX_PLAYERS {
            return None;
        }
        let players = (0..n).map(|_| read_slot(&mut r)).collect::<Option<_>>()?;
        let me = r.u8()?;
        let desync_tick = r.var_as()?;
        let n = r.var_as::<usize>()?;
        // Each turn takes at least a byte: don't trust a huge count.
        let mut turns = Vec::with_capacity(n.min(bytes.len()));
        for _ in 0..n {
            turns.push(read_player_commands(&mut r)?);
        }
        let n = r.var_as::<usize>()?;
        let mut hashes = Vec::with_capacity(n.min(bytes.len() / 9));
        for _ in 0..n {
            hashes.push((r.var_as()?, r.u64()?));
        }
        r.is_empty().then_some(Report {
            version,
            data_hash,
            seed,
            map_hash,
            settings,
            players,
            me,
            desync_tick,
            turns,
            hashes,
        })
    }

    /// The first recorded tick where `other`'s hash differs from this
    /// report's, comparing the ticks both recorded.
    pub fn first_difference(&self, other: &Report) -> Option<u32> {
        self.hashes
            .iter()
            .find(|&&(tick, hash)| other.hashes.iter().any(|&(t, h)| t == tick && h != hash))
            .map(|&(tick, _)| tick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_fixed::{Angle, Fx, FxVec2};
    use gc_sim::{Command, RegimentHandle};

    fn report() -> Report {
        Report {
            version: "0.1.0 abc1234".into(),
            data_hash: 42,
            seed: 7,
            map_hash: 99,
            settings: LobbySettings::default(),
            players: vec![Slot::open(0), Slot::open(1)],
            me: 1,
            desync_tick: 20,
            turns: vec![
                vec![],
                vec![PlayerCommand {
                    player: 1,
                    command: Command::MoveTo {
                        regiment: RegimentHandle {
                            index: 5,
                            generation: 1,
                        },
                        target: FxVec2 {
                            x: Fx::from_raw(100),
                            y: Fx::from_raw(-100),
                        },
                        facing: Some(Angle(9)),
                        files: 8,
                        pace: Fx::ZERO,
                    },
                }],
                vec![],
            ],
            hashes: vec![(10, 1), (20, 2)],
        }
    }

    #[test]
    fn reports_round_trip() {
        let r = report();
        let bytes = r.encode();
        assert_eq!(Report::decode(&bytes), Some(r));
        // Cut short or garbled, it is refused rather than misread.
        for n in 0..bytes.len() {
            assert_eq!(Report::decode(&bytes[..n]), None);
        }
        let mut odd = bytes.clone();
        odd[8] = FORMAT + 1;
        assert_eq!(Report::decode(&odd), None);
    }

    #[test]
    fn first_difference_finds_the_tick() {
        let a = report();
        let mut b = report();
        assert_eq!(a.first_difference(&b), None);
        b.hashes[1].1 = 3;
        assert_eq!(a.first_difference(&b), Some(20));
    }
}
