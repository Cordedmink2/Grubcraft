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
//! endian), then (since format 10) the length of the **info block** (four
//! bytes) and the info block itself: the note, [`Header`], seed, map size,
//! unit count, teams and ticks, which is all the game's replay list shows,
//! so [`Replay::read_header`] reads just it. Last comes the rest of the
//! replay compressed with LZ4 (size-prefixed block). Before format 10 the
//! whole body was the one compressed block. Both are written by [`codec`].

pub mod codec;

use std::collections::VecDeque;
use std::fmt;
use std::path::Path;

use gc_fixed::FxVec2;
use gc_sim::powers::MAX_POWERS;
use gc_sim::{
    BuildingStats, Capacities, ConquestSetup, ConquestUnit, Ground, MAX_PARTIES, MatchSetup, Meter,
    NodeSetup, PlayerCommand, PowerStats, Sky, Spell, TICKS_PER_SECOND, TickStats, UnitStats,
    Verdict, Victories, Weather, WeatherKind, Win, World,
};

use codec::{Reader, Writer};

pub const MAGIC: [u8; 4] = *b"GCRP";
/// Bumped whenever the file layout changes. Older formats still read,
/// each as the next one up without:
/// - 37: raising walls a level (tags 34 and 35; nothing to read in older files);
/// - 36: the blueprint commands (tags 31 to 33; nothing to read in older files);
/// - 35: the sky (read as a fair afternoon whose clock stands still);
/// - 34: the generals flag (read as generals on);
/// - 33: the footing layer (read as firm ground everywhere);
/// - 32: grown walls (a build-wall command with no worker);
/// - 31: not used;
/// - 30: the Conquest units' clearing styles (read as the standard one);
/// - 29: drawn walls (the build-wall command);
/// - 28: gates (the set-gate command);
/// - 27: the growth layer and the Clear command (read as growth that
///   follows the cover);
/// - 26: turned buildings (their own tags for the build and spawn commands);
/// - 25: the tower focus command;
/// - 24: the buildings' 16-bit flags (read as the 8 bits they had), the
///   Conquest units' trade pay, the map's neutral trade posts (read as
///   none) and the trade command;
/// - 23: the summon power (22's powers have none) and the Clans' formation
///   discipline bit among a unit's weapon flags (read as off);
/// - 22: each power's meter, each player's meter, and the Edicts' powers
///   and targets (21's powers read as paid from Resonance, and its players
///   as all on Resonance);
/// - 21: the faction powers and the orders that use them (read as none);
/// - 20: the buildings' chord links (read as none);
/// - 19: the buildings' fields and tuning (root-veins read as reaching 15
///   tiles from a main building and 8 from any other) and the Conquest
///   units' builds-only trait (read as none);
/// - 18: the Conquest units' burrowing trait (read as none);
/// - 17: the Conquest units' larva and used-up-worker traits (read as
///   neither);
/// - 16: the hand-over command;
/// - 15: the header's unit names (read as none);
/// - 14: how the battle was won (read as holding the field) and the
///   Conquest victories and Titan's Heart (read as a sandbox: no victory);
/// - 13: the fall back and brace commands;
/// - 12: the Conquest setup (read as a Field Battle);
/// - 11: the header's map choice (read as the Proving Ground);
/// - 10: the separate info block (read from the one compressed body);
/// - 8: the per-system hashes of a desync report (read as none);
/// - 7: the header's battle result (read as undecided);
/// - 6: the header's sim version (read as unknown);
/// - 5: the [`Header`] (read as an empty one);
/// - 4: the deployment commands;
/// - 3: the champions flag (read as no champions);
/// - 2: the cover layer (read as a map with no woods or grass).
///
/// Anything older is refused with [`Error::Format`].
pub const FORMAT: u16 = 37;
/// The oldest format this build still reads.
const OLDEST_FORMAT: u16 = 2;
/// Replay files end in `.gcr`.
pub const EXTENSION: &str = "gcr";
/// State hashes are kept every this many ticks (one second of game time).
pub const CHECKPOINT_EVERY: u32 = TICKS_PER_SECOND;
/// A desync report keeps each system's hashes for this many of the last
/// checkpoints: as many as the host keeps to compare with (gc_net's
/// `HASHES_KEPT`), so a report covers any tick a desync is found at.
pub const SYSTEM_TRAIL: usize = 256;
/// A replay body larger than this is refused rather than allocated.
const MAX_BODY: usize = 256 << 20;
/// An info block longer than this is refused rather than allocated.
const MAX_INFO: usize = 1 << 20;
/// More buildings than a Conquest record may make room for.
const MAX_STRUCTURES: u32 = 1 << 16;
/// The most resource nodes a recorded Conquest match may have: orders
/// name a node by a `u16`.
const MAX_NODES: usize = 1 << 16;

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
    /// How the battle was decided, if it was by the end of the recording
    /// ([`World::verdict`]); `None` too for files from before format 8.
    pub result: Option<Verdict>,
    /// The map it was played on, as the lobby's choice: in a Field Battle
    /// an index into `gc_map::FIELD_MAPS`, made from the match's seed, and
    /// in Conquest an index into the lobby's Conquest maps; `None` for the
    /// Proving Ground, as every replay before format 11 was. The replay
    /// plays on the map it stores either way; this is for drawing that
    /// map.
    pub map: Option<u8>,
    /// The names of the game's unit types when it was recorded, in table
    /// order, so the game can tell whether its own table still names the
    /// same regiments by the same ids (a reorder keeps the size); empty
    /// for files from before format 15, and for test recordings.
    pub unit_names: Vec<String>,
}

/// What the replay list needs of a file, which format 10 keeps in a block
/// of its own ([`Replay::read_header`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    pub note: String,
    pub header: Header,
    pub seed: u64,
    /// Regiment capacity the world was created with.
    pub regiments: u32,
    pub map_tiles: u32,
    /// The size of the unit table.
    pub units: usize,
    pub teams: Vec<u8>,
    /// Ticks recorded.
    pub ticks: u32,
    /// Whether it is a Conquest match ([`Replay::conquest`]).
    pub conquest: bool,
}

/// What made a recorded match a Conquest one: the [`ConquestSetup`] its
/// world was given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConquestRecord {
    pub buildings: Vec<BuildingStats>,
    pub units: Vec<ConquestUnit>,
    pub structures: u32,
    pub nodes: Vec<NodeSetup>,
    /// The map's neutral trade posts (format 24 on).
    pub posts: Vec<FxVec2>,
    pub start: [(u32, u32); MAX_PARTIES],
    pub gather_percent: u32,
    pub build_percent: u32,
    pub victories: Victories,
    pub heart: Option<FxVec2>,
    pub powers: Vec<PowerStats>,
    pub meters: [Meter; MAX_PARTIES],
}

impl ConquestRecord {
    pub fn of(setup: &ConquestSetup) -> ConquestRecord {
        ConquestRecord {
            buildings: setup.buildings.to_vec(),
            units: setup.units.to_vec(),
            structures: setup.structures,
            nodes: setup.nodes.to_vec(),
            posts: setup.posts.to_vec(),
            start: setup.start,
            gather_percent: setup.gather_percent,
            build_percent: setup.build_percent,
            victories: setup.victories,
            heart: setup.heart,
            // The simulation keeps no more than this, and reading takes no
            // more.
            powers: setup.powers.iter().take(MAX_POWERS).copied().collect(),
            meters: setup.meters,
        }
    }

    pub fn setup(&self) -> ConquestSetup<'_> {
        ConquestSetup {
            buildings: &self.buildings,
            units: &self.units,
            structures: self.structures,
            nodes: &self.nodes,
            posts: &self.posts,
            start: self.start,
            gather_percent: self.gather_percent,
            build_percent: self.build_percent,
            victories: self.victories,
            heart: self.heart,
            powers: &self.powers,
            meters: self.meters,
        }
    }

    fn write(&self, w: &mut Writer) {
        w.var(self.buildings.len() as u64);
        for b in &self.buildings {
            w.building(b);
        }
        w.var(self.units.len() as u64);
        for u in &self.units {
            w.conquest_unit(u);
        }
        w.var(u64::from(self.structures));
        w.var(self.nodes.len() as u64);
        for n in &self.nodes {
            w.node(n);
        }
        w.var(self.posts.len() as u64);
        for &at in &self.posts {
            w.vec(at);
        }
        for &(grub, marrow) in &self.start {
            w.var(u64::from(grub));
            w.var(u64::from(marrow));
        }
        w.var(u64::from(self.gather_percent));
        w.var(u64::from(self.build_percent));
        w.bool(self.victories.annihilation);
        w.bool(self.victories.heart);
        w.bool(self.heart.is_some());
        if let Some(at) = self.heart {
            w.vec(at);
        }
        w.var(self.powers.len() as u64);
        for p in &self.powers {
            w.power(p);
        }
        for &m in &self.meters {
            w.meter(m);
        }
    }

    fn read(r: &mut Reader, format: u16) -> Option<ConquestRecord> {
        // Counts are checked against the bytes left before allocating.
        let n: usize = r.var_as()?;
        let buildings = (0..n.min(r.remaining()))
            .map(|_| r.building(format).map(BuildingStats::sanitized))
            .collect::<Option<Vec<_>>>()
            .filter(|b| b.len() == n)?;
        let n: usize = r.var_as()?;
        let units = (0..n.min(r.remaining()))
            .map(|_| r.conquest_unit(format))
            .collect::<Option<Vec<_>>>()
            .filter(|u| u.len() == n)?;
        let structures = r.var_as().filter(|&s| s <= MAX_STRUCTURES)?;
        let n: usize = r.var_as().filter(|&n| n <= MAX_NODES)?;
        let nodes = (0..n.min(r.remaining()))
            .map(|_| r.node())
            .collect::<Option<Vec<_>>>()
            .filter(|v| v.len() == n)?;
        let posts = if format >= 24 {
            let n: usize = r.var_as().filter(|&n| n <= MAX_NODES)?;
            (0..n.min(r.remaining()))
                .map(|_| r.vec())
                .collect::<Option<Vec<_>>>()
                .filter(|v| v.len() == n)?
        } else {
            Vec::new()
        };
        let mut start = [(0, 0); MAX_PARTIES];
        for s in &mut start {
            *s = (r.var_as()?, r.var_as()?);
        }
        let (gather_percent, build_percent) = (r.var_as()?, r.var_as()?);
        let (victories, heart) = if format >= 14 {
            let victories = Victories {
                annihilation: r.bool()?,
                heart: r.bool()?,
            };
            (victories, if r.bool()? { Some(r.vec()?) } else { None })
        } else {
            (Victories::default(), None)
        };
        let powers = if format >= 21 {
            let n: usize = r.var_as().filter(|&n| n <= MAX_POWERS)?;
            (0..n.min(r.remaining()))
                .map(|_| r.power(format).map(PowerStats::sanitized))
                .collect::<Option<Vec<_>>>()
                .filter(|p| p.len() == n)?
        } else {
            Vec::new()
        };
        let mut meters = [Meter::Resonance; MAX_PARTIES];
        if format >= 22 {
            for m in &mut meters {
                *m = r.meter()?;
            }
        }
        Some(ConquestRecord {
            buildings,
            units,
            structures,
            nodes,
            posts,
            start,
            gather_percent,
            build_percent,
            victories,
            heart,
            powers,
            meters,
        })
    }
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
    /// What grows on each tile, as [`Ground::growth`] (empty: it follows
    /// the cover).
    pub growth: Vec<u8>,
    /// How each tile holds a soldier's feet, as [`Ground::footing`] (empty:
    /// firm everywhere).
    pub footing: Vec<u8>,
    /// The weather and the time of day, as [`MatchSetup::sky`].
    pub sky: SkyRecord,
    /// Whether champions may duel before battle, as [`MatchSetup::champions`].
    pub champions: bool,
    /// Whether each side has a general, as [`MatchSetup::generals`].
    pub generals: bool,
    /// What made it a Conquest match; `None` for a Field Battle.
    pub conquest: Option<ConquestRecord>,
    /// Ticks recorded.
    pub ticks: u32,
    /// The commands of each tick that had any, in tick order.
    pub turns: Vec<(u32, Vec<PlayerCommand>)>,
    /// `(tick, state hash)` at tick 0 and every [`CHECKPOINT_EVERY`]
    /// ticks after.
    pub checkpoints: Vec<(u32, u64)>,
    /// A desync report's last checkpoints again, with each system's own
    /// hash ([`World::system_hashes`], in the order of
    /// [`gc_sim::SYSTEMS`]), so two machines' reports name the system that
    /// parted first. Empty in other recordings.
    pub systems: Vec<(u32, Vec<u64>)>,
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

/// A match's sky, as [`MatchSetup::sky`] (see [`gc_sim::sky`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyRecord {
    pub start: u32,
    pub day_ticks: u32,
    pub runs: bool,
    pub spells: Vec<Spell>,
}

impl Default for SkyRecord {
    fn default() -> Self {
        SkyRecord::of(&Sky::STILL)
    }
}

/// Most spells a replay may hold: a day of three-minute spells and more.
const MAX_SPELLS: usize = 4096;

impl SkyRecord {
    pub fn of(sky: &Sky) -> SkyRecord {
        SkyRecord {
            start: sky.start,
            day_ticks: sky.day_ticks,
            runs: sky.runs,
            spells: {
                let mut spells = sky.spells.to_vec();
                spells.sort_by_key(|s| s.at);
                spells
            },
        }
    }

    pub fn sky(&self) -> Sky<'_> {
        Sky {
            start: self.start,
            day_ticks: self.day_ticks,
            runs: self.runs,
            spells: &self.spells,
        }
    }

    fn write(&self, w: &mut Writer) {
        w.var(u64::from(self.start));
        w.var(u64::from(self.day_ticks));
        w.bool(self.runs);
        w.var(self.spells.len() as u64);
        debug_assert!(
            self.spells.is_sorted_by_key(|s| s.at),
            "a sky's spells are kept in tick order"
        );
        let mut prev = 0;
        for s in &self.spells {
            // Spells are kept in tick order, so each starts after the last.
            w.var(u64::from(s.at.saturating_sub(prev)));
            prev = s.at;
            let wx = s.weather;
            w.u8(wx.kind as u8);
            w.u8(wx.strength);
            w.u8(wx.cloud);
            w.u8(wx.wind.0 as u8);
            w.u8(wx.wind.1 as u8);
        }
    }

    fn read(r: &mut Reader) -> Option<SkyRecord> {
        let start = r.var_as()?;
        let day_ticks: u32 = r.var_as()?;
        if day_ticks == 0 {
            return None;
        }
        let runs = r.bool()?;
        let n: usize = r.var_as()?;
        if n > MAX_SPELLS || n > r.remaining() {
            return None;
        }
        let mut spells = Vec::with_capacity(n);
        let mut at = 0u32;
        for _ in 0..n {
            at = at.checked_add(r.var_as()?)?;
            let kind = WeatherKind::ALL.get(r.u8()? as usize).copied()?;
            spells.push(Spell {
                at,
                weather: Weather {
                    kind,
                    strength: r.u8()?,
                    cloud: r.u8()?,
                    wind: (r.u8()? as i8, r.u8()? as i8),
                },
            });
        }
        Some(SkyRecord {
            start,
            day_ticks,
            runs,
            spells,
        })
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
                growth: &self.growth,
                footing: &self.footing,
            },
            champions: self.champions,
            sky: self.sky.sky(),
            generals: self.generals,
        }
    }

    /// A fresh world at tick 0 of the recorded match.
    pub fn new_world(&self) -> World {
        let mut world = World::new(&self.setup());
        if let Some(c) = &self.conquest {
            world.conquest(&c.setup());
        }
        world
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

    /// Where two desync reports' system trails first disagree: the tick,
    /// and the names of the systems ([`gc_sim::SYSTEMS`]) whose hashes
    /// differ there. `None` when the trails share no differing tick (or
    /// come from builds with other systems).
    pub fn first_system_difference(&self, other: &Replay) -> Option<(u32, Vec<&'static str>)> {
        self.systems.iter().find_map(|(tick, mine)| {
            let (_, theirs) = other.systems.iter().find(|(t, _)| t == tick)?;
            let differ: Vec<&'static str> = (mine.iter().zip(theirs))
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, _)| gc_sim::SYSTEMS.get(i).copied().unwrap_or("unknown"))
                .collect();
            (!differ.is_empty()).then_some((*tick, differ))
        })
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
        // A desync report's system hashes were the old rules'.
        self.systems.clear();
        self.header.result = world.verdict();
    }

    /// The note, header, seed and map size: how the info block starts, and
    /// how the body of formats 2 to 9 did.
    fn write_head(&self, w: &mut Writer) {
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
        match h.result {
            Some(v) => {
                w.var(u64::from(v.holder) + 1);
                w.var(u64::from(v.tick));
                w.u8(v.by as u8);
            }
            None => w.var(0),
        }
        w.var(h.map.map_or(0, |c| u64::from(c) + 1));
        w.var(h.unit_names.len() as u64);
        for name in &h.unit_names {
            w.bytes(name.as_bytes());
        }
        w.u64(self.seed);
        w.var(u64::from(self.regiments));
        w.var(u64::from(self.map_tiles));
    }

    fn write_units(&self, w: &mut Writer) {
        w.var(self.units.len() as u64);
        for u in &self.units {
            w.stats(u);
        }
    }

    fn write_map(&self, w: &mut Writer) {
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
    }

    /// The growth layer, just after the map (format 27 on), then the
    /// footing (format 33 on).
    fn write_layers(&self, w: &mut Writer) {
        w.bytes(&self.growth);
        w.bytes(&self.footing);
    }

    /// The generals flag, just after the growth and footing layers (format
    /// 34 on).
    fn write_generals(&self, w: &mut Writer) {
        w.bool(self.generals);
    }

    /// The sky, just after the generals flag (format 35 on).
    fn write_sky(&self, w: &mut Writer) {
        self.sky.write(w);
    }

    /// The Conquest record, if any; the info block says whether there is
    /// one.
    fn write_conquest(&self, w: &mut Writer) {
        if let Some(c) = &self.conquest {
            c.write(w);
        }
    }

    fn write_tail(&self, w: &mut Writer) {
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
        w.var(self.systems.len() as u64);
        for (tick, hashes) in &self.systems {
            w.var(u64::from(*tick));
            w.var(hashes.len() as u64);
            for &hash in hashes {
                w.u64(hash);
            }
        }
    }

    /// The fields the replay list shows.
    pub fn info(&self) -> Info {
        Info {
            note: self.note.clone(),
            header: self.header.clone(),
            seed: self.seed,
            regiments: self.regiments,
            map_tiles: self.map_tiles,
            units: self.units.len(),
            teams: self.teams.clone(),
            ticks: self.ticks,
            conquest: self.conquest.is_some(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut info = Writer::default();
        self.write_head(&mut info);
        info.var(self.units.len() as u64);
        info.bytes(&self.teams);
        info.var(u64::from(self.ticks));
        info.bool(self.conquest.is_some());
        let mut w = Writer::default();
        self.write_units(&mut w);
        self.write_map(&mut w);
        self.write_layers(&mut w);
        self.write_generals(&mut w);
        self.write_sky(&mut w);
        self.write_conquest(&mut w);
        self.write_tail(&mut w);
        let packed = lz4_flex::compress_prepend_size(&w.buf);
        let mut out = Vec::with_capacity(10 + info.buf.len() + packed.len());
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&FORMAT.to_le_bytes());
        out.extend_from_slice(&(info.buf.len() as u32).to_le_bytes());
        out.extend_from_slice(&info.buf);
        out.extend_from_slice(&packed);
        out
    }

    /// The body as formats 2 to 9 stored it, as the one compressed block:
    /// for the tests of the older formats.
    #[cfg(test)]
    fn legacy_body(&self) -> Vec<u8> {
        let mut w = Writer::default();
        self.write_head(&mut w);
        self.write_units(&mut w);
        w.bytes(&self.teams);
        self.write_map(&mut w);
        w.var(u64::from(self.ticks));
        self.write_tail(&mut w);
        w.buf
    }

    pub fn decode(bytes: &[u8]) -> Result<Replay, Error> {
        let (format, info, packed) = split(bytes)?;
        let size = packed
            .get(..4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
            .ok_or(Error::Corrupt("no body"))?;
        if size > MAX_BODY {
            return Err(Error::Corrupt("body too large"));
        }
        let info = info.map(|b| Replay::parse_info(b, format)).transpose()?;
        let body = lz4_flex::decompress_size_prepended(packed)
            .map_err(|_| Error::Corrupt("compressed body"))?;
        Replay::read_body(&mut Reader::new(&body), format, info).ok_or(Error::Corrupt("body"))
    }

    /// Only what the replay list shows, without reading the rest: from
    /// format 10 on that is one small block at the start of the file. An
    /// older file has to be read and decoded whole.
    pub fn read_header(path: impl AsRef<Path>) -> Result<Info, Error> {
        use std::io::Read;
        let path = path.as_ref();
        let mut file = std::fs::File::open(path)?;
        let mut start = Vec::with_capacity(10);
        (&mut file).take(10).read_to_end(&mut start)?;
        if start.len() < 6 || start[..4] != MAGIC {
            return Err(Error::NotAReplay);
        }
        let format = u16::from_le_bytes([start[4], start[5]]);
        if !(OLDEST_FORMAT..=FORMAT).contains(&format) {
            return Err(Error::Format(format));
        }
        if format < 10 {
            return Ok(Replay::load(path)?.info());
        }
        let len = start
            .get(6..10)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
            .ok_or(Error::Corrupt("no info block"))?;
        if len > MAX_INFO {
            return Err(Error::Corrupt("info block too large"));
        }
        // Not allocated up front: a lying length in a short file costs
        // only the bytes there are.
        let mut block = Vec::new();
        file.take(len as u64).read_to_end(&mut block)?;
        if block.len() != len {
            return Err(Error::Corrupt("info block"));
        }
        Replay::parse_info(&block, format)
    }

    fn parse_info(block: &[u8], format: u16) -> Result<Info, Error> {
        let corrupt = || Error::Corrupt("info block");
        let mut r = Reader::new(block);
        let info = Replay::read_info(&mut r, format).ok_or_else(corrupt)?;
        if r.remaining() != 0 {
            return Err(corrupt());
        }
        Ok(info)
    }

    /// The info block (format 10 and later).
    fn read_info(r: &mut Reader, format: u16) -> Option<Info> {
        let mut info = Replay::read_head(r, format)?;
        info.units = r.var_as()?;
        info.teams = r.bytes()?.to_vec();
        if info.teams.len() > gc_sim::MAX_PARTIES {
            return None;
        }
        info.ticks = r.var_as()?;
        info.conquest = format >= 12 && r.bool()?;
        Some(info)
    }

    /// The note, header, seed and map size, which the body of formats up to
    /// 9 starts with and the info block of format 10 does.
    fn read_head(r: &mut Reader, format: u16) -> Option<Info> {
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
        Some(Info {
            note,
            header,
            seed,
            regiments,
            map_tiles,
            ..Info::default()
        })
    }

    /// `info` is the info block, for format 10 and later; older formats
    /// have it at the start of the body.
    fn read_body(r: &mut Reader, format: u16, info: Option<Info>) -> Option<Replay> {
        // Counts are checked against the bytes left before allocating.
        let mut info = match info {
            Some(info) => info,
            None => Replay::read_head(r, format)?,
        };
        let map_tiles = info.map_tiles;
        let n: usize = r.var_as()?;
        let mut units = Vec::with_capacity(n.min(r.remaining()));
        for _ in 0..n {
            units.push(r.stats()?);
        }
        if format >= 10 {
            if n != info.units {
                return None;
            }
        } else {
            info.teams = r.bytes()?.to_vec();
            if info.teams.len() > gc_sim::MAX_PARTIES {
                return None;
            }
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
        // A tile layer written from format `since` on: empty, or one byte a
        // tile.
        let layer = |r: &mut Reader, since: u16| -> Option<Vec<u8>> {
            let bytes = if format >= since {
                r.bytes()?.to_vec()
            } else {
                Vec::new()
            };
            (bytes.is_empty() || bytes.len() == (map_tiles as usize).pow(2)).then_some(bytes)
        };
        let cover = layer(r, 3)?;
        let champions = format >= 4 && r.bool()?;
        let growth = layer(r, 27)?;
        let footing = layer(r, 33)?;
        let generals = format < 34 || r.bool()?;
        let sky = if format >= 35 {
            SkyRecord::read(r)?
        } else {
            SkyRecord::default()
        };
        let conquest = if format >= 12 && info.conquest {
            Some(ConquestRecord::read(r, format)?)
        } else {
            None
        };
        if format < 10 {
            info.ticks = r.var_as()?;
        }
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
        let mut systems = Vec::new();
        if format >= 9 {
            let n: usize = r.var_as()?;
            if n > SYSTEM_TRAIL {
                return None;
            }
            for _ in 0..n {
                let tick = r.var_as()?;
                let k: usize = r.var_as()?;
                if k > 4 * gc_sim::SYSTEMS.len() {
                    return None;
                }
                let hashes = (0..k).map(|_| r.u64()).collect::<Option<_>>()?;
                systems.push((tick, hashes));
            }
        }
        if r.remaining() != 0 {
            return None;
        }
        Some(Replay {
            note: info.note,
            header: info.header,
            seed: info.seed,
            regiments: info.regiments,
            units,
            map_tiles,
            teams: info.teams,
            nav,
            heights,
            water_level,
            cover,
            growth,
            footing,
            sky,
            champions,
            generals,
            conquest,
            ticks: info.ticks,
            turns,
            checkpoints,
            systems,
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Replay, Error> {
        Replay::decode(&std::fs::read(path)?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        Ok(gc_platform::fsutil::write_atomic(
            path.as_ref(),
            &self.encode(),
        )?)
    }
}

/// The magic and format number, then (format 10 and later) the info block
/// and what follows it.
type Split<'a> = (u16, Option<&'a [u8]>, &'a [u8]);

fn split(bytes: &[u8]) -> Result<Split<'_>, Error> {
    if bytes.len() < 6 || bytes[..4] != MAGIC {
        return Err(Error::NotAReplay);
    }
    let format = u16::from_le_bytes([bytes[4], bytes[5]]);
    if !(OLDEST_FORMAT..=FORMAT).contains(&format) {
        return Err(Error::Format(format));
    }
    let rest = &bytes[6..];
    if format < 10 {
        return Ok((format, None, rest));
    }
    let len = rest
        .get(..4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
        .ok_or(Error::Corrupt("no info block"))?;
    if len > MAX_INFO {
        return Err(Error::Corrupt("info block too large"));
    }
    let info = rest.get(4..4 + len).ok_or(Error::Corrupt("info block"))?;
    Ok((format, Some(info), &rest[4 + len..]))
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
    let result = if format >= 8 {
        match maybe(r)? {
            Some(holder) => Some(Verdict {
                holder: u8::try_from(holder)
                    .ok()
                    .filter(|&p| usize::from(p) < gc_sim::MAX_PARTIES)?,
                tick: r.var_as()?,
                by: if format >= 14 {
                    Win::from_u8(r.u8()?)?
                } else {
                    Win::Field
                },
            }),
            None => None,
        }
    } else {
        None
    };
    let map = if format >= 11 {
        maybe(r)?.map(u8::try_from).transpose().ok()?
    } else {
        None
    };
    let mut unit_names = Vec::new();
    if format >= 15 {
        let n: usize = r.var_as()?;
        unit_names.reserve(n.min(r.remaining()));
        for _ in 0..n {
            unit_names.push(text(r)?);
        }
    }
    Some(Header {
        build,
        data_hash,
        players,
        recorded_by,
        desync_tick,
        sim_version,
        result,
        map,
        unit_names,
    })
}

/// Records a match as it is played: create the world from the same setup,
/// then run every tick through [`Recorder::tick`].
pub struct Recorder {
    replay: Replay,
    /// Ticks between state hashes ([`CHECKPOINT_EVERY`] unless set).
    every: u32,
    /// Each system's hashes at the last checkpoints, when kept.
    trail: Option<VecDeque<(u32, Vec<u64>)>>,
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
                growth: if setup.ground.growth.len() == (setup.map_tiles as usize).pow(2) {
                    setup.ground.growth.to_vec()
                } else {
                    Vec::new()
                },
                // Footing that doesn't fit the map, or is firm everywhere,
                // is none, as in the sim.
                footing: gc_sim::Footing::layer(setup.ground.footing, setup.map_tiles).to_vec(),
                sky: SkyRecord::of(&setup.sky),
                champions: setup.champions,
                generals: setup.generals,
                conquest: None,
                ticks: 0,
                turns: Vec::new(),
                checkpoints: vec![(0, world.state_hash())],
                systems: Vec::new(),
            },
            every: CHECKPOINT_EVERY,
            trail: None,
        }
    }

    /// Record that the match is a Conquest one, made so by `record`'s
    /// setup. The world given to [`Recorder::new`] must already have been
    /// ([`World::conquest`]), so the first state hash counts it.
    #[must_use]
    pub fn conquest(mut self, record: ConquestRecord) -> Recorder {
        assert_eq!(self.replay.ticks, 0, "recording starts at tick 0");
        self.replay.conquest = Some(record);
        self
    }

    /// Also keep each system's hashes at the last [`SYSTEM_TRAIL`]
    /// checkpoints, for [`Recorder::desync_report`] (network matches).
    #[must_use]
    pub fn with_system_trail(mut self) -> Recorder {
        self.trail = Some(VecDeque::with_capacity(SYSTEM_TRAIL));
        self
    }

    /// The recording so far as a desync report: marked as out of sync from
    /// `tick`, with the system trail if one is kept.
    pub fn desync_report(&self, tick: u32) -> Replay {
        let mut report = self.replay.clone();
        report.header.desync_tick = Some(tick);
        report.systems = self.trail.iter().flatten().cloned().collect();
        report
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
        self.replay.header.result = world.verdict();
        if self.replay.ticks.is_multiple_of(self.every) {
            self.replay
                .checkpoints
                .push((self.replay.ticks, world.state_hash()));
            if let Some(trail) = &mut self.trail {
                if trail.len() == SYSTEM_TRAIL {
                    trail.pop_front();
                }
                trail.push_back((self.replay.ticks, world.system_hashes().to_vec()));
            }
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
                growth: &[],
                footing: &[],
            },
            champions: false,
            sky: gc_sim::Sky::STILL,
            generals: true,
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
            result: None,
            map: None,
            unit_names: vec!["Spears".into(), "Bows".into()],
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
        let info_len = u32::from_le_bytes(bytes[6..10].try_into().unwrap()) as usize;
        let (head, packed) = bytes.split_at(10 + info_len);
        let body = lz4_flex::decompress_size_prepended(packed).unwrap();
        let file = |body: &[u8]| {
            let mut out = head.to_vec();
            out.extend_from_slice(&lz4_flex::compress_prepend_size(body));
            out
        };
        assert!(Replay::decode(&file(&body)).is_ok());
        // Every byte of the info block too.
        for i in 10..head.len() {
            for flip in [0x01, 0xff] {
                let mut bad = bytes.clone();
                bad[i] ^= flip;
                let _ = Replay::decode(&bad);
            }
        }
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

    /// Two minutes of a small Conquest match on an open map: a hall and a
    /// worker each, gathering, a house built, regiments trained, a rally
    /// point, an upgrade called off.
    fn record_conquest() -> Replay {
        use gc_fixed::FxVec2;
        use gc_sim::structures::{Armour, DROP_OFF_GRUB, Role};
        use gc_sim::{BuildingStats, ConquestUnit, NodeSetup, Resource};
        let nav = vec![
            1u8;
            64 * 64
                * gc_sim::nav::CELLS_PER_TILE as usize
                * gc_sim::nav::CELLS_PER_TILE as usize
        ];
        let setup = MatchSetup {
            seed: 3,
            capacities: Capacities { regiments: 32 },
            units: &DRILL_UNITS,
            map_tiles: 64,
            nav: &nav,
            teams: &[],
            ground: Ground::FLAT,
            champions: false,
            sky: gc_sim::Sky::STILL,
            generals: true,
        };
        let hall = BuildingStats {
            role: Role::Main,
            tier: 1,
            upgrades_from: None,
            cost_grub: 400,
            cost_marrow: 0,
            build_ticks: 600,
            hp: 3000,
            armour: Armour::Fortified,
            width: 5,
            depth: 5,
            supply: 10,
            flags: DROP_OFF_GRUB,
            attack: None,
            field: 0,
            tunes: None,
            links: 0,
        };
        let buildings = [
            hall,
            BuildingStats {
                role: Role::Housing,
                cost_grub: 50,
                build_ticks: 200,
                hp: 500,
                armour: Armour::Structure,
                width: 2,
                depth: 2,
                flags: 0,
                ..hall
            },
            BuildingStats {
                tier: 2,
                upgrades_from: Some(0),
                cost_grub: 100,
                cost_marrow: 0,
                build_ticks: 900,
                ..hall
            },
        ];
        let units = [
            ConquestUnit {
                worker: true,
                supply: 1,
                cost_grub: 50,
                train_ticks: 200,
                trained_at: Some(0),
                tier: 1,
                // A Clerk: no Grub drop-off, and burns what it clears.
                cannot_bank: gc_sim::structures::DROP_OFF_GRUB,
                clearing: gc_sim::clearing::ClearStyle {
                    pace: [50, 60, 70],
                    pays: [1, 2, 3],
                    burns: true,
                },
                ..ConquestUnit::default()
            },
            ConquestUnit {
                supply: 2,
                cost_grub: 80,
                train_ticks: 300,
                trained_at: Some(0),
                tier: 1,
                ..ConquestUnit::default()
            },
            // A caravan.
            ConquestUnit {
                supply: 2,
                trade_base: 40,
                trade_tenths: 12,
                ..ConquestUnit::default()
            },
        ];
        let posts = [FxVec2::from_ints(5, 60), FxVec2::from_ints(58, 62)];
        let nodes = [
            NodeSetup {
                resource: Resource::Grub,
                at: FxVec2::from_ints(16, 8),
                amount: 5000,
            },
            NodeSetup {
                resource: Resource::Grub,
                at: FxVec2::from_ints(48, 56),
                amount: 5000,
            },
        ];
        let mut start = [(0, 0); MAX_PARTIES];
        start[0] = (600, 0);
        start[1] = (600, 0);
        let powers = [
            gc_sim::PowerStats {
                tier: 1,
                cost: 20,
                meter: Meter::Resonance,
                cooldown_ticks: 400,
                duration_ticks: 400,
                effect: gc_sim::PowerEffect::Haste { percent: 50 },
            },
            gc_sim::PowerStats {
                tier: 2,
                cost: 60,
                meter: Meter::Resonance,
                cooldown_ticks: 1800,
                duration_ticks: 0,
                effect: gc_sim::PowerEffect::Shift { to: 0 },
            },
            gc_sim::PowerStats::default(),
            gc_sim::PowerStats {
                tier: 1,
                cost: 25,
                meter: Meter::Honor,
                cooldown_ticks: 1200,
                duration_ticks: 0,
                effect: gc_sim::PowerEffect::Grant { grub: 300 },
            },
            gc_sim::PowerStats {
                tier: 1,
                cost: 20,
                meter: Meter::Honor,
                cooldown_ticks: 900,
                duration_ticks: 600,
                effect: gc_sim::PowerEffect::March { percent: 30 },
            },
            gc_sim::PowerStats {
                tier: 2,
                cost: 35,
                meter: Meter::Honor,
                cooldown_ticks: 1800,
                duration_ticks: 400,
                effect: gc_sim::PowerEffect::Steadfast { tiles: 12 },
            },
            gc_sim::PowerStats {
                tier: 3,
                cost: 80,
                meter: Meter::Honor,
                cooldown_ticks: 3600,
                duration_ticks: 1200,
                effect: gc_sim::PowerEffect::Summon { unit: 1 },
            },
        ];
        let mut meters = [Meter::NotBuilt; MAX_PARTIES];
        meters[0] = Meter::Resonance;
        meters[1] = Meter::Honor;
        let conquest = ConquestSetup {
            buildings: &buildings,
            units: &units,
            structures: 64,
            nodes: &nodes,
            posts: &posts,
            start,
            gather_percent: 150,
            build_percent: 75,
            victories: Victories::BOTH,
            heart: Some(FxVec2::from_ints(32, 32)),
            powers: &powers,
            meters,
        };
        let mut world = World::new(&setup);
        assert!(world.conquest(&conquest));
        let mut rec =
            Recorder::new(&setup, &world, "test conquest").conquest(ConquestRecord::of(&conquest));
        let say = |player: u8, command: Command| PlayerCommand { player, command };
        let structure = |w: &World, player: u8, x: i32| {
            w.structures()
                .unwrap()
                .iter()
                .find(|v| v.owner == player && v.tile.0 == x)
                .map(|v| v.handle)
        };
        let worker = |w: &World, player: u8| {
            w.regiments
                .iter()
                .find(|r| r.owner == player)
                .map(|r| r.handle)
        };
        for _ in 0..120 * TICKS_PER_SECOND {
            let t = world.tick_count();
            let mut commands = Vec::new();
            for p in 0..2u8 {
                let (hx, wy): (i16, i16) = if p == 0 { (8, 16) } else { (52, 44) };
                match t {
                    0 => {
                        commands.push(say(
                            p,
                            Command::SpawnStructure {
                                building: 0,
                                tile: (hx, if p == 0 { 8 } else { 52 }),
                                turn: 0,
                            },
                        ));
                    }
                    1 => commands.push(say(
                        p,
                        Command::SpawnRegiment {
                            unit: 0,
                            position: FxVec2::from_ints(i32::from(hx) + 2, i32::from(wy)),
                            facing: gc_fixed::Angle(0),
                        },
                    )),
                    2 => commands.push(say(
                        p,
                        Command::Gather {
                            regiment: worker(&world, p).unwrap(),
                            node: u16::from(p),
                        },
                    )),
                    3 => commands.push(say(
                        p,
                        Command::SpawnRegiment {
                            unit: 2,
                            position: FxVec2::from_ints(i32::from(hx) + 6, i32::from(wy)),
                            facing: gc_fixed::Angle(0),
                        },
                    )),
                    4 => commands.push(say(
                        p,
                        Command::Trade {
                            regiment: world
                                .regiments
                                .iter()
                                .find(|r| r.owner == p && r.unit == 2)
                                .map(|r| r.handle)
                                .unwrap(),
                            post: gc_sim::TradePost::Hamlet(u16::from(p)),
                        },
                    )),
                    400 => commands.push(say(
                        p,
                        Command::Build {
                            regiment: worker(&world, p).unwrap(),
                            building: 1,
                            tile: (hx + 8, wy),
                            turn: 0,
                        },
                    )),
                    401 => {
                        let h = structure(&world, p, i32::from(hx)).unwrap();
                        commands.push(say(
                            p,
                            Command::Train {
                                structure: h,
                                unit: 1,
                            },
                        ));
                        commands.push(say(
                            p,
                            Command::Train {
                                structure: h,
                                unit: 0,
                            },
                        ));
                        commands.push(say(
                            p,
                            Command::Rally {
                                structure: h,
                                // Each in front of its own hall, apart.
                                at: FxVec2::from_ints(32, if p == 0 { 16 } else { 44 }),
                            },
                        ));
                    }
                    402 => {
                        let h = structure(&world, p, i32::from(hx)).unwrap();
                        commands.push(say(p, Command::Untrain { structure: h }));
                        commands.push(say(p, Command::Upgrade { structure: h }));
                    }
                    600 => {
                        let h = structure(&world, p, i32::from(hx)).unwrap();
                        commands.push(say(p, Command::CallOff { structure: h }));
                    }
                    900 => {
                        if let (Some(r), Some(h)) =
                            (worker(&world, p), structure(&world, p, i32::from(hx) + 8))
                        {
                            commands.push(say(
                                p,
                                Command::Construct {
                                    regiment: r,
                                    structure: h,
                                },
                            ));
                        }
                    }
                    _ => {}
                }
            }
            rec.tick(&mut world, &commands);
        }
        rec.finish()
    }

    #[test]
    fn a_conquest_match_round_trips_and_verifies() {
        let replay = record_conquest();
        assert!(replay.conquest.is_some());
        let world = replay.verify().expect("the replay plays back identically");
        // It was a match: a house went up and regiments were trained.
        let s = world.structures().unwrap();
        assert_eq!(s.len(), 4, "a hall and a house each");
        assert!(s.iter().all(|v| v.finished()));
        assert_eq!(world.regiments.iter().count(), 6, "and a caravan each");
        let bytes = replay.encode();
        let back = Replay::decode(&bytes).unwrap();
        assert_eq!(back, replay);
        assert!(back.info().conquest);
        // Without its Conquest record it doesn't play the same.
        let mut field = replay.clone();
        field.conquest = None;
        assert!(field.verify().is_err());
    }

    #[test]
    fn damaged_conquest_files_never_panic() {
        let replay = record_conquest();
        let bytes = replay.encode();
        let info_len = u32::from_le_bytes(bytes[6..10].try_into().unwrap()) as usize;
        let (head, packed) = bytes.split_at(10 + info_len);
        let body = lz4_flex::decompress_size_prepended(packed).unwrap();
        let mut w = Writer::default();
        replay.write_units(&mut w);
        replay.write_map(&mut w);
        replay.write_layers(&mut w);
        replay.write_generals(&mut w);
        replay.write_sky(&mut w);
        // Every byte of the Conquest record.
        let mut c = Writer::default();
        replay.write_conquest(&mut c);
        for i in w.buf.len()..w.buf.len() + c.buf.len() {
            for flip in [0x01, 0x7f, 0xff] {
                let mut bad = body.clone();
                bad[i] ^= flip;
                let mut file = head.to_vec();
                file.extend_from_slice(&lz4_flex::compress_prepend_size(&bad));
                let _ = Replay::decode(&file);
            }
        }
        // A flag saying Conquest with a body that isn't is refused.
        let mut lying = head.to_vec();
        let mut body = Writer::default();
        replay.write_units(&mut body);
        replay.write_map(&mut body);
        replay.write_tail(&mut body);
        lying.extend_from_slice(&lz4_flex::compress_prepend_size(&body.buf));
        assert!(Replay::decode(&lying).is_err());
    }

    #[test]
    fn a_match_without_generals_plays_back_without_them() {
        let mut replay = record();
        assert!(replay.generals);
        replay.generals = false;
        let back = Replay::decode(&replay.encode()).unwrap();
        assert!(!back.generals && !back.setup().generals);
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
        let full = replay.legacy_body();
        replay.header = Header::default();
        let empty = replay.legacy_body();
        // The empty header: no build, data hash 0, no players, nobody, no
        // desync, no sim version, no result, no map choice, no unit names.
        let header_len = 1 + 8 + 1 + 1 + 1 + 1 + 1 + 1 + 1;
        body.extend_from_slice(&empty[body.len() + header_len..]);
        // Nor, from format 8 down, the system trail's count.
        assert_eq!(body.pop(), Some(0));
        assert!(full.len() > empty.len());
        let mut old = MAGIC.to_vec();
        old.extend_from_slice(&5u16.to_le_bytes());
        old.extend_from_slice(&lz4_flex::compress_prepend_size(&body));
        assert_eq!(Replay::decode(&old).unwrap(), replay);
    }

    /// `replay` with an empty header, written as `format` (8 or older): the
    /// current format less the system trail's count at the end, and less
    /// the header's last `dropped` bytes (each 0 for none),
    /// which follow the note and the empty header's no build, data hash 0,
    /// no players, nobody and no desync.
    fn older(replay: &mut Replay, format: u16, dropped: usize) -> Vec<u8> {
        replay.header = Header::default();
        let mut body = replay.legacy_body();
        let mut w = Writer::default();
        w.bytes(replay.note.as_bytes());
        let at = w.buf.len() + 1 + 8 + 1 + 1 + 1;
        // Every format older than 15 has no unit names, and older than 11
        // no map choice (after the sim version and result).
        assert_eq!(body.remove(at + 3), 0);
        assert_eq!(body.remove(at + 2), 0);
        assert!(body.drain(at..at + dropped).all(|b| b == 0));
        assert_eq!(body.pop(), Some(0));
        let mut old = MAGIC.to_vec();
        old.extend_from_slice(&format.to_le_bytes());
        old.extend_from_slice(&lz4_flex::compress_prepend_size(&body));
        old
    }

    /// A temporary file named for the test and process, removed at the end.
    fn temp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("gc-replay-{}-{name}.gcr", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn the_header_reads_without_the_body() {
        let replay = record();
        let path = temp("header", &replay.encode());
        let info = Replay::read_header(&path).unwrap();
        assert_eq!(info, replay.info());
        assert_eq!(info, Replay::load(&path).unwrap().info());
        assert_eq!(info.header, replay.header);
        assert_eq!(info.teams, [0, 1, 0, 1]);
        assert_eq!(info.units, DRILL_UNITS.len());
        // Damage to the body leaves the header readable: it never looks.
        let mut bytes = replay.encode();
        let n = bytes.len();
        bytes.truncate(n - 20);
        let cut = temp("header-cut-body", &bytes);
        assert_eq!(Replay::read_header(&cut).unwrap(), replay.info());
        assert!(Replay::load(&cut).is_err());
        // Save is atomic and reads back.
        replay.save(&path).unwrap();
        assert_eq!(Replay::load(&path).unwrap(), replay);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(cut);
    }

    #[test]
    fn an_older_file_reads_its_header_whole() {
        let mut replay = record();
        let old = older(&mut replay, 8, 0);
        let path = temp("header-old", &old);
        assert_eq!(Replay::read_header(&path).unwrap(), replay.info());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_truncated_or_lying_info_block_is_refused() {
        let bytes = record().encode();
        let len = u32::from_le_bytes(bytes[6..10].try_into().unwrap()) as usize;
        for cut in [0, 3, 5, 8, 10, 10 + len / 2, 10 + len - 1] {
            let path = temp("header-cut", &bytes[..cut]);
            assert!(Replay::read_header(&path).is_err(), "cut at {cut}");
            assert!(Replay::decode(&bytes[..cut]).is_err(), "cut at {cut}");
            let _ = std::fs::remove_file(path);
        }
        // A length that is huge is refused without allocating it; one that
        // promises more than the file has is truncation.
        for lie in [u32::MAX, MAX_INFO as u32 + 1, len as u32 + 1_000_000] {
            let mut bad = bytes[..10 + len].to_vec();
            bad[6..10].copy_from_slice(&lie.to_le_bytes());
            let path = temp("header-lie", &bad);
            assert!(Replay::read_header(&path).is_err());
            assert!(Replay::decode(&bad).is_err());
            let _ = std::fs::remove_file(path);
        }
        // A block that doesn't end where its fields do is refused.
        let mut long = bytes[..10 + len].to_vec();
        long.push(0);
        long[6..10].copy_from_slice(&(len as u32 + 1).to_le_bytes());
        let path = temp("header-long", &long);
        assert!(matches!(Replay::read_header(&path), Err(Error::Corrupt(_))));
        let _ = std::fs::remove_file(path);
        assert!(matches!(
            Replay::read_header("/nonexistent/x.gcr"),
            Err(Error::Io(_))
        ));
    }

    /// `replay`'s info block as format 14 wrote it: format 15 without the
    /// unit names, which `replay` must not have (a count of 0 just before
    /// the seed, map size and the rest of the info block).
    fn info_14(replay: &Replay) -> Vec<u8> {
        assert!(replay.header.unit_names.is_empty());
        let new = replay.encode();
        let len = u32::from_le_bytes(new[6..10].try_into().unwrap()) as usize;
        let mut info = new[10..10 + len].to_vec();
        let mut tail = Writer::default();
        tail.u64(replay.seed);
        tail.var(u64::from(replay.regiments));
        tail.var(u64::from(replay.map_tiles));
        let at = info
            .windows(tail.buf.len())
            .position(|w| w == tail.buf)
            .unwrap()
            - 1;
        assert_eq!(info.remove(at), 0);
        info
    }

    #[test]
    fn the_footing_layer_round_trips() {
        let mut replay = record();
        replay.footing = (0..64u32 * 64).map(|i| u8::from(i % 7 == 0)).collect();
        assert_eq!(Replay::decode(&replay.encode()).unwrap(), replay);
        assert_eq!(replay.setup().ground.footing, &replay.footing[..]);
    }

    #[test]
    fn the_sky_round_trips() {
        let mut replay = record();
        replay.sky = SkyRecord {
            start: gc_sim::sky::at_hour(22, 30),
            day_ticks: gc_sim::sky::DAY_TICKS,
            runs: true,
            spells: gc_sim::sky::changing(9, gc_sim::sky::Season::Cold, 20_000),
        };
        assert!(replay.sky.spells.len() > 1);
        assert_eq!(Replay::decode(&replay.encode()).unwrap(), replay);
        assert_eq!(replay.setup().sky, replay.sky.sky());
    }

    #[test]
    fn a_format_14_replay_has_no_unit_names() {
        let mut replay = record();
        replay.header.unit_names.clear();
        let new = replay.encode();
        let len = u32::from_le_bytes(new[6..10].try_into().unwrap()) as usize;
        let info = info_14(&replay);
        let mut old = MAGIC.to_vec();
        old.extend_from_slice(&14u16.to_le_bytes());
        old.extend_from_slice(&(info.len() as u32).to_le_bytes());
        old.extend_from_slice(&info);
        // The body as it was then: no growth or footing layer, generals
        // flag or sky after the map.
        let body = lz4_flex::decompress_size_prepended(&new[10 + len..]).unwrap();
        let mut map = Writer::default();
        replay.write_units(&mut map);
        replay.write_map(&mut map);
        let mut layers = Writer::default();
        replay.write_layers(&mut layers);
        replay.write_generals(&mut layers);
        replay.write_sky(&mut layers);
        let (at, layers) = (map.buf.len(), layers.buf.len());
        let mut old_body = body[..at].to_vec();
        assert_eq!(body[at..at + 2], [0, 0], "no growth or footing");
        assert_eq!(body[at + 2], 1, "generals on");
        old_body.extend_from_slice(&body[at + layers..]);
        old.extend_from_slice(&lz4_flex::compress_prepend_size(&old_body));
        assert_eq!(Replay::decode(&old).unwrap(), replay);
        let path = temp("header-14", &old);
        assert_eq!(Replay::read_header(&path).unwrap(), replay.info());
        let _ = std::fs::remove_file(path);
    }

    /// `replay` (a Field Battle with no unit names) as format 11 wrote it:
    /// format 12 without the Conquest flags at the end of the info block
    /// and after the map.
    fn format_11(replay: &Replay) -> Vec<u8> {
        assert!(replay.conquest.is_none());
        let info = info_14(replay);
        assert_eq!(info.last(), Some(&0));
        let mut body = Writer::default();
        replay.write_units(&mut body);
        replay.write_map(&mut body);
        replay.write_tail(&mut body);
        let mut old = MAGIC.to_vec();
        old.extend_from_slice(&11u16.to_le_bytes());
        old.extend_from_slice(&(info.len() as u32 - 1).to_le_bytes());
        old.extend_from_slice(&info[..info.len() - 1]);
        old.extend_from_slice(&lz4_flex::compress_prepend_size(&body.buf));
        old
    }

    #[test]
    fn a_format_11_replay_is_a_field_battle() {
        let mut replay = record();
        replay.header.unit_names.clear();
        let old = format_11(&replay);
        assert_eq!(Replay::decode(&old).unwrap(), replay);
        let path = temp("header-11", &old);
        assert_eq!(Replay::read_header(&path).unwrap(), replay.info());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_format_10_replay_has_no_map_choice() {
        let mut replay = record();
        replay.header = Header::default();
        let new = format_11(&replay);
        // Format 10 is format 11 without the map choice: the last byte of
        // the header, before the seed, map size and the rest of the info
        // block.
        let len = u32::from_le_bytes(new[6..10].try_into().unwrap()) as usize;
        let info = &new[10..10 + len];
        let mut tail = Writer::default();
        tail.u64(replay.seed);
        tail.var(u64::from(replay.regiments));
        tail.var(u64::from(replay.map_tiles));
        let at = info
            .windows(tail.buf.len())
            .position(|w| w == tail.buf)
            .unwrap()
            - 1;
        assert_eq!(info[at], 0);
        let mut old = MAGIC.to_vec();
        old.extend_from_slice(&10u16.to_le_bytes());
        old.extend_from_slice(&(len as u32 - 1).to_le_bytes());
        old.extend_from_slice(&info[..at]);
        old.extend_from_slice(&info[at + 1..]);
        old.extend_from_slice(&new[10 + len..]);
        assert_eq!(Replay::decode(&old).unwrap(), replay);
    }

    #[test]
    fn the_map_choice_round_trips() {
        let mut replay = record();
        replay.header.map = Some(3);
        assert_eq!(Replay::decode(&replay.encode()).unwrap(), replay);
    }

    #[test]
    fn a_format_8_replay_has_no_system_trail() {
        let mut replay = record();
        let old = older(&mut replay, 8, 0);
        assert_eq!(Replay::decode(&old).unwrap(), replay);
    }

    #[test]
    fn desync_reports_name_the_system_that_parted_first() {
        let setup = MatchSetup {
            seed: 3,
            capacities: Capacities { regiments: 16 },
            units: &DRILL_UNITS,
            map_tiles: 64,
            nav: &[],
            teams: &[],
            ground: Ground::FLAT,
            champions: false,
            sky: gc_sim::Sky::STILL,
            generals: true,
        };
        let mut world = World::new(&setup);
        let mut rec = Recorder::new(&setup, &world, "trail")
            .checkpoint_every(10)
            .with_system_trail();
        let spawn = PlayerCommand {
            player: 0,
            command: Command::SpawnRegiment {
                unit: 0,
                position: gc_fixed::FxVec2::from_ints(32, 32),
                facing: gc_fixed::Angle(0),
            },
        };
        rec.tick(&mut world, &[spawn]);
        for _ in 0..(SYSTEM_TRAIL as u32 + 5) * 10 {
            rec.tick(&mut world, &[]);
        }
        let mine = rec.desync_report(500);
        assert_eq!(mine.header.desync_tick, Some(500));
        assert_eq!(mine.systems.len(), SYSTEM_TRAIL);
        assert_eq!(rec.replay().systems.len(), 0, "only reports carry it");
        let back = Replay::decode(&mine.encode()).unwrap();
        assert_eq!(back, mine);
        // The other machine's regiments went astray at one checkpoint.
        let mut theirs = mine.clone();
        let regiments = gc_sim::SYSTEMS
            .iter()
            .position(|&s| s == "regiments")
            .unwrap();
        let (tick, hashes) = &mut theirs.systems[10];
        hashes[regiments] ^= 1;
        let tick = *tick;
        assert_eq!(
            mine.first_system_difference(&theirs),
            Some((tick, vec!["regiments"]))
        );
        assert_eq!(mine.first_system_difference(&mine), None);
    }

    #[test]
    fn a_format_7_replay_has_no_result() {
        let mut replay = record();
        // Format 7 is format 8 without the battle result.
        let old = older(&mut replay, 7, 1);
        assert_eq!(Replay::decode(&old).unwrap(), replay);
    }

    #[test]
    fn a_format_6_replay_has_no_sim_version() {
        let mut replay = record();
        // Format 6 is format 7 without the sim version.
        let old = older(&mut replay, 6, 2);
        assert_eq!(Replay::decode(&old).unwrap(), replay);
    }

    #[test]
    fn the_result_round_trips() {
        let mut replay = record();
        replay.header.result = Some(Verdict {
            holder: 1,
            tick: 1234,
            by: Win::Heart,
        });
        assert_eq!(Replay::decode(&replay.encode()).unwrap(), replay);
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
