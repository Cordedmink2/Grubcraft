//! Grubcraft replays (docs/11-networking.md, "Save, load and replays").
//!
//! The simulation is deterministic, so a match is fully described by how it
//! started and the commands of every tick. A [`Replay`] stores exactly that:
//! the match setup (seed, unit table, teams, walkable ground, heights,
//! woods and tall grass) and
//! the command stream, plus the simulation's state hash once a second of
//! game time. Playing it back rebuilds the same world tick for tick; the
//! hashes say whether it still does.
//!
//! That makes replays the **golden replay** tests (docs/10-architecture.md,
//! "Testing strategy"): recorded battles kept in `replays/` that CI plays
//! back on macOS, Windows and Linux. A hash that changes means the
//! simulation behaves differently: a bug, or an intended change whose
//! replays are re-blessed (`gc_replay bless`) in the same pull request.
//!
//! The unit table and the map are stored in the file, not looked up, so
//! retuning `data/units.ron` or changing the map generator leaves recorded
//! replays valid; only changes to the simulation itself move their hashes.
//!
//! File layout: the magic `GCRP`, the format number (two bytes, little
//! endian), then the body compressed with LZ4 (size-prefixed block). The
//! body is written by [`codec`].

pub mod codec;

use std::fmt;
use std::path::Path;

use gc_sim::{
    Capacities, Ground, MatchSetup, PlayerCommand, TICKS_PER_SECOND, TickStats, UnitStats, World,
};

use codec::{Reader, Writer};

pub const MAGIC: [u8; 4] = *b"GCRP";
/// Bumped whenever the file layout changes. Older replays are refused
/// with [`Error::Format`], except format 2, which is format 3 without the
/// cover layer (read as a map with no woods or grass).
pub const FORMAT: u16 = 3;
/// The oldest format this build still reads.
const OLDEST_FORMAT: u16 = 2;
/// Replay files end in `.gcr`.
pub const EXTENSION: &str = "gcr";
/// State hashes are kept every this many ticks (one second of game time).
pub const CHECKPOINT_EVERY: u32 = TICKS_PER_SECOND;
/// A replay body larger than this is refused rather than allocated.
const MAX_BODY: usize = 256 << 20;

/// A recorded match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replay {
    /// What was recorded and how, for people (`gc_replay info`).
    pub note: String,
    pub seed: u64,
    /// Regiment capacity the world was created with.
    pub regiments: u32,
    pub units: Vec<UnitStats>,
    pub map_tiles: u32,
    pub teams: Vec<u8>,
    /// Walkable-ground costs, as [`MatchSetup::nav`].
    pub nav: Vec<u8>,
    /// Heights and water line, as [`Ground`].
    pub heights: Vec<u16>,
    pub water_level: u16,
    /// Woods and tall grass, as [`Ground::cover`] (empty: none).
    pub cover: Vec<u8>,
    /// Ticks recorded.
    pub ticks: u32,
    /// The commands of each tick that had any, in tick order.
    pub turns: Vec<(u32, Vec<PlayerCommand>)>,
    /// `(tick, state hash)` at tick 0 and every [`CHECKPOINT_EVERY`]
    /// ticks after.
    pub checkpoints: Vec<(u32, u64)>,
}

/// Where a replay stopped matching its recorded hashes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Divergence {
    pub tick: u32,
    pub expected: u64,
    pub got: u64,
    /// The last checkpoint that still matched, if any.
    pub last_good: Option<u32>,
}

impl fmt::Display for Divergence {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "state hash at tick {} is {:#018x}, recorded {:#018x}",
            self.tick, self.got, self.expected
        )?;
        match self.last_good {
            Some(t) => write!(f, " (last match at tick {t})"),
            None => write!(f, " (the starting state already differs)"),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// The file doesn't start with the replay magic.
    NotAReplay,
    /// Written in another format version.
    Format(u16),
    /// Truncated or damaged.
    Corrupt(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::NotAReplay => write!(f, "not a Grubcraft replay"),
            Error::Format(v) => write!(
                f,
                "replay format {v}, this build reads format {FORMAT}; record it again"
            ),
            Error::Corrupt(what) => write!(f, "damaged replay: {what}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

impl Replay {
    /// The setup to create the recorded match's world from.
    pub fn setup(&self) -> MatchSetup<'_> {
        MatchSetup {
            seed: self.seed,
            capacities: Capacities {
                regiments: self.regiments,
            },
            units: &self.units,
            map_tiles: self.map_tiles,
            nav: &self.nav,
            teams: &self.teams,
            ground: Ground {
                heights: &self.heights,
                water_level: self.water_level,
                cover: &self.cover,
            },
        }
    }

    /// A fresh world at tick 0 of the recorded match.
    pub fn new_world(&self) -> World {
        World::new(&self.setup())
    }

    /// The commands recorded for `tick` (empty if none).
    pub fn commands_at(&self, tick: u32) -> &[PlayerCommand] {
        match self.turns.binary_search_by_key(&tick, |(t, _)| *t) {
            Ok(i) => &self.turns[i].1,
            Err(_) => &[],
        }
    }

    /// Number of commands recorded.
    pub fn command_count(&self) -> usize {
        self.turns.iter().map(|(_, c)| c.len()).sum()
    }

    /// Play the whole replay and compare every checkpoint. Returns the
    /// final world when all of them match.
    pub fn verify(&self) -> Result<World, Divergence> {
        let mut world = self.new_world();
        let mut last_good = None;
        let mut checkpoints = self.checkpoints.iter().peekable();
        let mut turns = self.turns.iter().peekable();
        loop {
            let tick = world.tick_count();
            while let Some(&&(t, expected)) = checkpoints.peek() {
                if t > tick {
                    break;
                }
                checkpoints.next();
                let got = world.state_hash();
                if t < tick || got != expected {
                    return Err(Divergence {
                        tick: t,
                        expected,
                        got,
                        last_good,
                    });
                }
                last_good = Some(t);
            }
            if tick >= self.ticks {
                return Ok(world);
            }
            let commands = match turns.peek() {
                Some((t, c)) if *t == tick => {
                    turns.next();
                    c.as_slice()
                }
                _ => &[],
            };
            world.tick(commands);
        }
    }

    /// Play the replay with its recorded commands and store the hashes the
    /// simulation produces now. For when a change to the simulation is
    /// meant to change how recorded matches play out.
    pub fn bless(&mut self) {
        let mut world = self.new_world();
        let mut checkpoints = vec![(0, world.state_hash())];
        for tick in 0..self.ticks {
            world.tick(self.commands_at(tick));
            if (tick + 1).is_multiple_of(CHECKPOINT_EVERY) {
                checkpoints.push((tick + 1, world.state_hash()));
            }
        }
        self.checkpoints = checkpoints;
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.bytes(self.note.as_bytes());
        w.u64(self.seed);
        w.var(self.regiments as u64);
        w.var(self.map_tiles as u64);
        w.var(self.units.len() as u64);
        for u in &self.units {
            w.stats(u);
        }
        w.bytes(&self.teams);
        w.bytes(&self.nav);
        // Heights as differences from the one before: neighbouring heights
        // are close, so most take one byte.
        w.var(self.heights.len() as u64);
        let mut prev = 0u16;
        for &h in &self.heights {
            w.ivar(h.wrapping_sub(prev) as i16 as i64);
            prev = h;
        }
        w.var(self.water_level as u64);
        w.bytes(&self.cover);
        w.var(self.ticks as u64);
        w.var(self.turns.len() as u64);
        let mut prev = 0;
        for (tick, commands) in &self.turns {
            w.var((tick - prev) as u64);
            prev = *tick;
            w.var(commands.len() as u64);
            for c in commands {
                w.command(c);
            }
        }
        w.var(self.checkpoints.len() as u64);
        let mut prev = 0;
        for &(tick, hash) in &self.checkpoints {
            w.var((tick - prev) as u64);
            prev = tick;
            w.u64(hash);
        }
        let mut out = Vec::with_capacity(w.buf.len() / 2);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&FORMAT.to_le_bytes());
        out.extend_from_slice(&lz4_flex::compress_prepend_size(&w.buf));
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Replay, Error> {
        if bytes.len() < 6 || bytes[..4] != MAGIC {
            return Err(Error::NotAReplay);
        }
        let format = u16::from_le_bytes([bytes[4], bytes[5]]);
        if !(OLDEST_FORMAT..=FORMAT).contains(&format) {
            return Err(Error::Format(format));
        }
        let packed = &bytes[6..];
        let size = packed
            .get(..4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
            .ok_or(Error::Corrupt("no body"))?;
        if size > MAX_BODY {
            return Err(Error::Corrupt("body too large"));
        }
        let body = lz4_flex::decompress_size_prepended(packed)
            .map_err(|_| Error::Corrupt("compressed body"))?;
        Replay::read_body(&mut Reader::new(&body), format).ok_or(Error::Corrupt("body"))
    }

    fn read_body(r: &mut Reader, format: u16) -> Option<Replay> {
        // Counts are checked against the bytes left before allocating.
        let note = String::from_utf8(r.bytes()?.to_vec()).ok()?;
        let seed = r.u64()?;
        let regiments = r.var_as()?;
        let map_tiles: u32 = r.var_as()?;
        let n: usize = r.var_as()?;
        let mut units = Vec::with_capacity(n.min(r.remaining()));
        for _ in 0..n {
            units.push(r.stats()?);
        }
        let teams = r.bytes()?.to_vec();
        let nav = r.bytes()?.to_vec();
        let side = map_tiles as usize * gc_sim::nav::CELLS_PER_TILE as usize;
        if !nav.is_empty() && nav.len() != side * side {
            return None;
        }
        let n: usize = r.var_as()?;
        if n != 0 && n != (map_tiles as usize).pow(2) || n > r.remaining() {
            return None;
        }
        let mut heights = Vec::with_capacity(n);
        let mut prev = 0u16;
        for _ in 0..n {
            prev = prev.wrapping_add(i16::try_from(r.ivar()?).ok()? as u16);
            heights.push(prev);
        }
        let water_level = r.var_as()?;
        let cover = if format >= 3 {
            r.bytes()?.to_vec()
        } else {
            Vec::new()
        };
        if !cover.is_empty() && cover.len() != (map_tiles as usize).pow(2) {
            return None;
        }
        let ticks = r.var_as()?;
        let n: usize = r.var_as()?;
        let mut turns = Vec::with_capacity(n.min(r.remaining()));
        let mut tick = 0u32;
        for i in 0..n {
            let delta: u32 = r.var_as()?;
            if i > 0 && delta == 0 {
                return None;
            }
            tick = tick.checked_add(delta)?;
            let count: usize = r.var_as()?;
            let mut commands = Vec::with_capacity(count.min(r.remaining()));
            for _ in 0..count {
                commands.push(r.command()?);
            }
            turns.push((tick, commands));
        }
        let n: usize = r.var_as()?;
        let mut checkpoints = Vec::with_capacity(n.min(r.remaining()));
        let mut tick = 0u32;
        for _ in 0..n {
            tick = tick.checked_add(r.var_as()?)?;
            checkpoints.push((tick, r.u64()?));
        }
        if r.remaining() != 0 {
            return None;
        }
        Some(Replay {
            note,
            seed,
            regiments,
            units,
            map_tiles,
            teams,
            nav,
            heights,
            water_level,
            cover,
            ticks,
            turns,
            checkpoints,
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Replay, Error> {
        Replay::decode(&std::fs::read(path)?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        Ok(std::fs::write(path, self.encode())?)
    }
}

/// Records a match as it is played: create the world from the same setup,
/// then run every tick through [`Recorder::tick`].
pub struct Recorder {
    replay: Replay,
}

impl Recorder {
    /// Start recording the match `setup` describes. `world` must be the
    /// match's fresh world, at tick 0.
    pub fn new(setup: &MatchSetup, world: &World, note: impl Into<String>) -> Recorder {
        assert_eq!(world.tick_count(), 0, "recording starts at tick 0");
        Recorder {
            replay: Replay {
                note: note.into(),
                seed: setup.seed,
                regiments: setup.capacities.regiments,
                units: setup.units.to_vec(),
                map_tiles: setup.map_tiles,
                teams: setup.teams.to_vec(),
                nav: setup.nav.to_vec(),
                heights: setup.ground.heights.to_vec(),
                water_level: setup.ground.water_level,
                // Cover that doesn't fit the map is none, as in the sim.
                cover: if setup.ground.cover.len() == (setup.map_tiles as usize).pow(2) {
                    setup.ground.cover.to_vec()
                } else {
                    Vec::new()
                },
                ticks: 0,
                turns: Vec::new(),
                checkpoints: vec![(0, world.state_hash())],
            },
        }
    }

    /// Advance `world` one tick with `commands`, recording them.
    pub fn tick(&mut self, world: &mut World, commands: &[PlayerCommand]) -> TickStats {
        debug_assert_eq!(world.tick_count(), self.replay.ticks);
        if !commands.is_empty() {
            self.replay
                .turns
                .push((self.replay.ticks, commands.to_vec()));
        }
        let stats = world.tick(commands);
        self.replay.ticks += 1;
        if self.replay.ticks.is_multiple_of(CHECKPOINT_EVERY) {
            self.replay
                .checkpoints
                .push((self.replay.ticks, world.state_hash()));
        }
        stats
    }

    /// The recording so far.
    pub fn replay(&self) -> &Replay {
        &self.replay
    }

    pub fn finish(self) -> Replay {
        self.replay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_sim::scenario::{DRILL_UNITS, Drill, ribs_map};
    use gc_sim::{Command, Formation, Mass, TICKS_PER_SECOND};

    /// A minute of the scripted drill on the ridged map, with sandbox
    /// tuning, a formation change and removal, so every kind of command is
    /// recorded.
    fn record() -> Replay {
        let nav = ribs_map(64);
        let heights: Vec<u16> = (0..64u32 * 64).map(|i| (i * 37 % 900) as u16).collect();
        // Woods and tall grass in 8-tile squares, so hiding is recorded too.
        let cover: Vec<u8> = (0..64u32 * 64)
            .map(|i| ((i % 64 / 8 + i / 64 / 8) % 3) as u8)
            .collect();
        let setup = MatchSetup {
            seed: 7,
            capacities: Capacities { regiments: 64 },
            units: &DRILL_UNITS,
            map_tiles: 64,
            nav: &nav,
            teams: &[0, 1, 0, 1],
            ground: Ground {
                heights: &heights,
                water_level: 200,
                cover: &cover,
            },
        };
        let mut world = World::new(&setup);
        let mut rec = Recorder::new(&setup, &world, "test drill");
        let mut drill = Drill::new(7);
        drill.regiments_per_player = 6;
        drill.map_tiles = 64;
        let mut commands = Vec::new();
        for _ in 0..60 * TICKS_PER_SECOND {
            drill.commands_for(&world, &mut commands);
            match world.tick_count() {
                300 => commands.push(PlayerCommand {
                    player: 0,
                    command: Command::TuneUnit {
                        unit: 1,
                        stats: UnitStats {
                            melee_attack: 70,
                            mass: Mass::Heavy,
                            reach: true,
                            shield: true,
                            ..DRILL_UNITS[1]
                        },
                    },
                }),
                350 => {
                    let regiment = world.regiments.iter().find(|r| r.owner == 0).unwrap();
                    commands.push(PlayerCommand {
                        player: 0,
                        command: Command::SetFormation {
                            regiment: regiment.handle,
                            formation: Formation::Tight,
                        },
                    });
                }
                400 => {
                    let regiment = world.regiments.iter().next().unwrap().handle;
                    commands.push(PlayerCommand {
                        player: 2,
                        command: Command::RemoveRegiment { regiment },
                    });
                }
                _ => {}
            }
            rec.tick(&mut world, &commands);
        }
        rec.finish()
    }

    #[test]
    fn round_trips_and_verifies() {
        let replay = record();
        assert_eq!(replay.ticks, 1200);
        assert_eq!(replay.checkpoints.len(), 61);
        let bytes = replay.encode();
        let back = Replay::decode(&bytes).unwrap();
        assert_eq!(back, replay);
        back.verify().expect("the replay plays back identically");
    }

    #[test]
    fn a_different_command_is_caught() {
        let mut replay = record();
        let (tick, commands) = replay.turns.iter_mut().find(|(t, _)| *t > 100).unwrap();
        let tick = *tick;
        for c in commands.iter_mut() {
            if let Command::MoveTo { target, .. } = &mut c.command {
                target.x += gc_fixed::Fx::from_int(5);
            }
        }
        let Err(d) = replay.verify() else {
            panic!("the changed move went unnoticed");
        };
        assert!(d.tick > tick && d.tick <= tick + CHECKPOINT_EVERY, "{d}");
        assert_eq!(d.last_good, Some(d.tick - CHECKPOINT_EVERY));
        // Blessing accepts the new behaviour.
        replay.bless();
        replay.verify().unwrap();
    }

    #[test]
    fn refuses_what_is_not_a_replay() {
        assert!(matches!(
            Replay::decode(b"hello world"),
            Err(Error::NotAReplay)
        ));
        let mut bytes = record().encode();
        bytes[4] = 99;
        assert!(matches!(Replay::decode(&bytes), Err(Error::Format(99))));
        bytes[4] = FORMAT as u8;
        bytes.truncate(bytes.len() - 10);
        assert!(matches!(Replay::decode(&bytes), Err(Error::Corrupt(_))));
    }
}
