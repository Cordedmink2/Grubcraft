//! A lobby Field Battle played headless: the map made from the seed, the
//! armies set out as the game sets them out, the computer players thinking
//! every tick and the battle judged as it goes. Tests, ladders and tools
//! play battles through this rather than each wiring the world, the minds
//! and the judge together themselves.

use std::time::{Duration, Instant};

use gc_ai::Minds;
use gc_map::Map;
use gc_replay::{Recorder, Replay};
use gc_sim::{Capacities, MatchSetup, PlayerCommand, TickStats, World};
use gc_ui::lobby::MatchStart;

use crate::Tables;
use crate::battle::{Battle, SetOut, champions_allowed, set_out};
use crate::terrain_view::{ai_terrain, plateau_positions};

/// A battle under way.
pub struct Match {
    pub world: World,
    pub minds: Minds,
    pub set: SetOut,
    pub battle: Battle,
    /// Time the computer players spent thinking, in all and on the
    /// slowest tick (what they cost the game's thread).
    pub minds_time: Duration,
    pub minds_worst: Duration,
    /// What records the match as a replay, when it is being recorded.
    recorder: Option<Recorder>,
    /// The commands for the next tick, kept to reuse their room.
    out: Vec<PlayerCommand>,
}

/// What one tick of a [`Match`] did.
#[derive(Clone, Copy, Debug)]
pub struct TickReport {
    /// The commands the world applied and refused.
    pub stats: TickStats,
    /// The battle is decided (on this tick or before).
    pub decided: bool,
}

impl Match {
    /// Set out `setup` on `map` with `seed`, as a match on this machine
    /// alone, and play the first tick, which puts the armies on the field.
    /// `live` lets the minds think on their own thread as in the game (the
    /// battle then may not play out the same way twice). With `record`, the
    /// match is recorded as a replay with that note (see [`Match::finish`]).
    pub fn new(
        setup: &MatchStart,
        seed: u64,
        map: &Map,
        tables: &Tables,
        live: bool,
        record: Option<String>,
    ) -> Match {
        let nav = map.nav_costs();
        let teams: Vec<u8> = setup.players.iter().map(|s| s.team).collect();
        let match_setup = MatchSetup {
            seed,
            capacities: Capacities::default(),
            units: &tables.units,
            map_tiles: map.tiles,
            nav: &nav,
            teams: &teams,
            ground: map.ground(),
            champions: champions_allowed(setup),
        };
        let world = World::new(&match_setup);
        let recorder = record.map(|note| Recorder::new(&match_setup, &world, note));
        let mut minds = Minds::new(ai_terrain(map, &nav));
        if live {
            minds = minds.live();
        }
        let mut out = Vec::new();
        let set = set_out(
            setup,
            &tables.units,
            &tables.factions,
            &tables.points,
            plateau_positions(map.tiles),
            seed,
            true,
            &mut minds,
            &mut out,
        );
        Match::from_parts(world, minds, set, &out, recorder)
    }

    /// A match set out some other way than a lobby's (a golden replay's
    /// armies, say): `world` fresh at tick 0, `minds` seated, and `first`
    /// the commands that put the armies on the field, played as the first
    /// tick. `recorder`, if any, must have been started on `world`.
    pub fn from_parts(
        world: World,
        minds: Minds,
        set: SetOut,
        first: &[PlayerCommand],
        recorder: Option<Recorder>,
    ) -> Match {
        let battle = Battle::new(Some(set.me));
        let mut m = Match {
            world,
            minds,
            set,
            battle,
            minds_time: Duration::ZERO,
            minds_worst: Duration::ZERO,
            recorder,
            out: Vec::new(),
        };
        m.advance(first);
        m
    }

    /// Play one tick: the computer players' commands, then `extra` (a
    /// person's orders, say), then judge.
    pub fn step(&mut self, extra: &[PlayerCommand]) -> TickReport {
        let mut out = std::mem::take(&mut self.out);
        out.clear();
        let t0 = Instant::now();
        self.minds.tick(&self.world, &mut out);
        let took = t0.elapsed();
        self.minds_time += took;
        self.minds_worst = self.minds_worst.max(took);
        out.extend_from_slice(extra);
        let report = self.advance(&out);
        self.out = out;
        report
    }

    /// Tick the world with `commands`, recording them if recording, and
    /// judge.
    fn advance(&mut self, commands: &[PlayerCommand]) -> TickReport {
        let stats = match &mut self.recorder {
            Some(rec) => rec.tick(&mut self.world, commands),
            None => self.world.tick(commands),
        };
        self.battle.judge(&self.world, &self.set.parties);
        TickReport {
            stats,
            decided: self.battle.outcome.is_some(),
        }
    }

    /// The replay, if the match was recorded.
    pub fn finish(self) -> Option<Replay> {
        self.recorder.map(Recorder::finish)
    }
}
