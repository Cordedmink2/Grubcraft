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
/// with [`Error::Format`], except format 6, which is format 7 without the
/// header's sim version (read as unknown), format 5, which is format 6 without the
/// [`Header`] (read as an empty one), format 4, which is format 5 without
/// the deployment commands, format 3, which is format 4 without the champions
/// flag (read as no champions), and format 2, which is format 3 without
/// the cover layer (read as a map with no woods or grass).
pub const FORMAT: u16 = 7;
/// The oldest format this build still reads.
const OLDEST_FORMAT: u16 = 2;
/// Replay files end in `.gcr`.
pub const EXTENSION: &str = "gcr";
/// State hashes are kept every this many ticks (one second of game time).
pub const CHECKPOINT_EVERY: u32 = TICKS_PER_SECOND;
/// A replay body larger than this is refused rather than allocated.
const MAX_BODY: usize = 256 << 20;

/// One side of a recorded match, as the game showed it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Player {
    /// "Red (Mario)".
    pub name: String,
    pub color: [u8; 3],
    /// The faction's index in the lobby's list, when it had one.
    pub faction: Option<u8>,
}

/// Who recorded a match and on what, for the game's replay list. The
/// simulation doesn't need any of it to play the match back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    /// The build that recorded it (version and commit; empty if unknown).
    pub build: String,
    /// The game data's short hash (`gc_data::GameData::short_hash`; 0 if
    /// unknown). Unit names and models are looked up in the game's own
    /// data, so they only match the recording when this does.
    pub data_hash: u64,
    /// The players, in player order.
    pub players: Vec<Player>,
    /// The player whose machine recorded it, if a player's did.
    pub recorded_by: Option<u8>,
    /// Where the recording machine's world first disagreed with the
    /// host's, for a desync report.
    pub desync_tick: Option<u32>,
    /// The [`gc_sim::SIM_VERSION`] it was recorded (or last blessed) with;
    /// `None` for files from before format 7. Under another version it may
    /// play out differently.
    pub sim_version: Option<u32>,
}

/// A recorded match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replay {
    /// What was recorded and how, for people (`gc_replay info`).
    pub note: String,
    pub header: Header,
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
    /// Whether champions may duel before battle, as [`MatchSetup::champions`].
    pub champions: bool,
    /// Ticks recorded.
    pub ticks: u32,
    /// The commands of each tick that had any, in tick order.
    pub turns: Vec<(u32, Vec<PlayerCommand>)>,
    /// `(tick, state hash)` at tick 0 and every [`CHECKPOINT_EVERY`]
    /// ticks after.
    pub checkpoints: Vec<(u32, u64)>,
}

/// The replay files in `dir`, sorted by name.
pub fn files_in(dir: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|f| f.extension().is_some_and(|x| x == EXTENSION))
        .collect();
    found.sort();
    Ok(found)
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
            champions: self.champions,
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

    /// The state hash recorded for `tick`, if it is a checkpoint.
    pub fn checkpoint_at(&self, tick: u32) -> Option<u64> {
        self.checkpoints
            .binary_search_by_key(&tick, |&(t, _)| t)
            .ok()
            .map(|i| self.checkpoints[i].1)
    }

    /// The first checkpoint tick both recordings have where their state
    /// hashes differ: where two machines' worlds of one match parted.
    pub fn first_difference(&self, other: &Replay) -> Option<u32> {
        self.checkpoints
            .iter()
            .find(|&&(tick, hash)| other.checkpoint_at(tick).is_some_and(|h| h != hash))
            .map(|&(tick, _)| tick)
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
    /// simulation produces now, under this build's sim version. For when a
    /// change to the simulation is meant to change how recorded matches
    /// play out.
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
        self.header.sim_version = Some(gc_sim::SIM_VERSION);
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.bytes(self.note.as_bytes());
        let h = &self.header;
        w.bytes(h.build.as_bytes());
        w.u64(h.data_hash);
        w.var(h.players.len() as u64);
        for p in &h.players {
            w.bytes(p.name.as_bytes());
            w.bytes(&p.color);
            w.var(p.faction.map_or(0, |f| u64::from(f) + 1));
        }
        w.var(h.recorded_by.map_or(0, |p| u64::from(p) + 1));
        w.var(h.desync_tick.map_or(0, |t| u64::from(t) + 1));
        w.var(h.sim_version.map_or(0, |v| u64::from(v) + 1));
        w.u64(self.seed);
        w.var(u64::from(self.regiments));
        w.var(u64::from(self.map_tiles));
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
            w.ivar(i64::from(h.wrapping_sub(prev) as i16));
            prev = h;
        }
        w.var(u64::from(self.water_level));
        w.bytes(&self.cover);
        w.bool(self.champions);
        w.var(u64::from(self.ticks));
        w.var(self.turns.len() as u64);
        let mut prev = 0;
        for (tick, commands) in &self.turns {
            w.var(u64::from(tick - prev));
            prev = *tick;
            w.var(commands.len() as u64);
            for c in commands {
                w.command(c);
            }
        }
        w.var(self.checkpoints.len() as u64);
        let mut prev = 0;
        for &(tick, hash) in &self.checkpoints {
            w.var(u64::from(tick - prev));
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
        let header = if format >= 6 {
            read_header(r, format)?
        } else {
            Header::default()
        };
        let seed = r.u64()?;
        let regiments: u32 = r.var_as()?;
        let map_tiles: u32 = r.var_as()?;
        if regiments > Capacities::MAX_REGIMENTS || map_tiles > gc_sim::MAX_MAP_TILES {
            return None;
        }
        let n: usize = r.var_as()?;
        let mut units = Vec::with_capacity(n.min(r.remaining()));
        for _ in 0..n {
            units.push(r.stats()?);
        }
        let teams = r.bytes()?.to_vec();
        if teams.len() > gc_sim::MAX_PARTIES {
            return None;
        }
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
        let champions = format >= 4 && r.bool()?;
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
            header,
            seed,
            regiments,
            units,
            map_tiles,
            teams,
            nav,
            heights,
            water_level,
            cover,
            champions,
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

fn read_header(r: &mut Reader, format: u16) -> Option<Header> {
    let text = |r: &mut Reader| String::from_utf8(r.bytes()?.to_vec()).ok();
    // 0 for none, else the value plus one.
    let maybe = |r: &mut Reader| -> Option<Option<u32>> {
        let v: u32 = r.var_as()?;
        Some(v.checked_sub(1))
    };
    let build = text(r)?;
    let data_hash = r.u64()?;
    let n: usize = r.var_as()?;
    let mut players = Vec::with_capacity(n.min(r.remaining()));
    for _ in 0..n {
        let name = text(r)?;
        let color = r.bytes()?.try_into().ok()?;
        let faction = maybe(r)?.map(u8::try_from).transpose().ok()?;
        players.push(Player {
            name,
            color,
            faction,
        });
    }
    let recorded_by = maybe(r)?.map(u8::try_from).transpose().ok()?;
    let desync_tick = maybe(r)?;
    let sim_version = if format >= 7 { maybe(r)? } else { None };
    Some(Header {
        build,
        data_hash,
        players,
        recorded_by,
        desync_tick,
        sim_version,
    })
}

/// Records a match as it is played: create the world from the same setup,
/// then run every tick through [`Recorder::tick`].
pub struct Recorder {
    replay: Replay,
    /// Ticks between state hashes ([`CHECKPOINT_EVERY`] unless set).
    every: u32,
}

impl Recorder {
    /// Start recording the match `setup` describes. `world` must be the
    /// match's fresh world, at tick 0.
    pub fn new(setup: &MatchSetup, world: &World, note: impl Into<String>) -> Recorder {
        assert_eq!(world.tick_count(), 0, "recording starts at tick 0");
        Recorder {
            replay: Replay {
                note: note.into(),
                header: Header {
                    sim_version: Some(gc_sim::SIM_VERSION),
                    ..Header::default()
                },
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
                champions: setup.champions,
                ticks: 0,
                turns: Vec::new(),
                checkpoints: vec![(0, world.state_hash())],
            },
            every: CHECKPOINT_EVERY,
        }
    }

    /// Keep a state hash every `ticks` ticks instead, such as as often as a
    /// network match compares them, so a desync report pins the tick.
    #[must_use]
    pub fn checkpoint_every(mut self, ticks: u32) -> Recorder {
        self.every = ticks.max(1);
        self
    }

    /// The state hash taken after the last tick, if one was.
    pub fn hash_now(&self) -> Option<u64> {
        self.replay
            .checkpoints
            .last()
            .filter(|&&(t, _)| t == self.replay.ticks)
            .map(|&(_, h)| h)
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
        if self.replay.ticks.is_multiple_of(self.every) {
            self.replay
                .checkpoints
                .push((self.replay.ticks, world.state_hash()));
        }
        stats
    }

    /// Who is recording and on what (see [`Header`]).
    pub fn header_mut(&mut self) -> &mut Header {
        &mut self.replay.header
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
            champions: false,
        };
        let mut world = World::new(&setup);
        let mut rec = Recorder::new(&setup, &world, "test drill");
        *rec.header_mut() = Header {
            build: "0.1.0 abc1234".into(),
            data_hash: 42,
            players: vec![
                Player {
                    name: "Red (Mario)".into(),
                    color: [200, 40, 40],
                    faction: Some(2),
                },
                Player::default(),
            ],
            recorded_by: Some(0),
            desync_tick: None,
            sim_version: Some(gc_sim::SIM_VERSION),
        };
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

    /// A damaged file is refused or read as something, never a panic,
    /// however its body is damaged (damage to the compressed bytes mostly
    /// fails decompression, so the body is damaged before compressing).
    #[test]
    fn damaged_files_never_panic() {
        let bytes = record().encode();
        let body = lz4_flex::decompress_size_prepended(&bytes[6..]).unwrap();
        let file = |body: &[u8]| {
            let mut out = bytes[..6].to_vec();
            out.extend_from_slice(&lz4_flex::compress_prepend_size(body));
            out
        };
        assert!(Replay::decode(&file(&body)).is_ok());
        // Every byte of the header and counts, a sample of the rest.
        let positions = (0..body.len().min(256)).chain((256..body.len()).step_by(1021));
        for i in positions {
            for flip in [0x01, 0xff] {
                let mut bad = body.clone();
                bad[i] ^= flip;
                let _ = Replay::decode(&file(&bad));
            }
        }
        // A map too big to hold is refused, not allocated.
        let mut big = record();
        big.map_tiles = u32::MAX;
        (big.nav, big.heights, big.cover) = (Vec::new(), Vec::new(), Vec::new());
        assert!(Replay::decode(&big.encode()).is_err());
        // So are more parties than a world holds.
        let mut crowd = record();
        crowd.teams = vec![0; gc_sim::MAX_PARTIES + 1];
        assert!(Replay::decode(&crowd.encode()).is_err());
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
    fn two_machines_recordings_show_where_they_parted() {
        let a = record();
        let mut b = a.clone();
        assert_eq!(a.first_difference(&b), None);
        b.checkpoints[7].1 ^= 1;
        assert_eq!(a.first_difference(&b), Some(7 * CHECKPOINT_EVERY));
        // A shorter recording is compared as far as it goes.
        b.checkpoints.truncate(5);
        assert_eq!(a.first_difference(&b), None);
    }

    #[test]
    fn a_format_5_replay_has_an_empty_header() {
        let mut replay = record();
        // Format 5 is format 6 without the header.
        let mut w = Writer::default();
        w.bytes(replay.note.as_bytes());
        let mut body = w.buf;
        let full = lz4_flex::decompress_size_prepended(&replay.encode()[6..]).unwrap();
        replay.header = Header::default();
        let empty = lz4_flex::decompress_size_prepended(&replay.encode()[6..]).unwrap();
        // The empty header: no build, data hash 0, no players, nobody, no
        // desync, no sim version.
        let header_len = 1 + 8 + 1 + 1 + 1 + 1;
        body.extend_from_slice(&empty[body.len() + header_len..]);
        assert!(full.len() > empty.len());
        let mut old = MAGIC.to_vec();
        old.extend_from_slice(&5u16.to_le_bytes());
        old.extend_from_slice(&lz4_flex::compress_prepend_size(&body));
        assert_eq!(Replay::decode(&old).unwrap(), replay);
    }

    #[test]
    fn a_format_6_replay_has_no_sim_version() {
        let mut replay = record();
        replay.header = Header::default();
        let mut body = lz4_flex::decompress_size_prepended(&replay.encode()[6..]).unwrap();
        // Format 6 is format 7 without the header's last byte, the sim
        // version (0 for none), after the note and the empty header's no
        // build, data hash 0, no players, nobody and no desync.
        let mut w = Writer::default();
        w.bytes(replay.note.as_bytes());
        assert_eq!(body.remove(w.buf.len() + 1 + 8 + 1 + 1 + 1), 0);
        let mut old = MAGIC.to_vec();
        old.extend_from_slice(&6u16.to_le_bytes());
        old.extend_from_slice(&lz4_flex::compress_prepend_size(&body));
        assert_eq!(Replay::decode(&old).unwrap(), replay);
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
