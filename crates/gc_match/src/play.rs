//! A lobby Field Battle played headless: the map made from the seed, the
//! armies set out as the game sets them out, the computer players thinking
//! every tick and the battle judged as it goes. Tests, ladders and tools
//! play battles through this rather than each wiring the world, the minds
//! and the judge together themselves.

use std::time::{Duration, Instant};

use gc_ai::Minds;
use gc_map::Map;
use gc_sim::{Capacities, MatchSetup, PlayerCommand, World};
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
    /// The commands for the next tick, kept to reuse their room.
    out: Vec<PlayerCommand>,
}

impl Match {
    /// Set out `setup` on `map` with `seed`, as a match on this machine
    /// alone, and play the first tick, which puts the armies on the field.
    /// `live` lets the minds think on their own thread as in the game (the
    /// battle then may not play out the same way twice).
    pub fn new(setup: &MatchStart, seed: u64, map: &Map, tables: &Tables, live: bool) -> Match {
        let nav = map.nav_costs();
        let teams: Vec<u8> = setup.players.iter().map(|s| s.team).collect();
        let mut world = World::new(&MatchSetup {
            seed,
            capacities: Capacities::default(),
            units: &tables.units,
            map_tiles: map.tiles,
            nav: &nav,
            teams: &teams,
            ground: map.ground(),
            champions: champions_allowed(setup),
        });
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
        world.tick(&out);
        let battle = Battle::new(Some(set.me));
        Match {
            world,
            minds,
            set,
            battle,
            minds_time: Duration::ZERO,
            minds_worst: Duration::ZERO,
            out,
        }
    }

    /// Play one tick: the computer players' commands, then `extra` (a
    /// person's orders, say), then judge. Returns true once the battle is
    /// decided.
    pub fn step(&mut self, extra: &[PlayerCommand]) -> bool {
        self.out.clear();
        let t0 = Instant::now();
        self.minds.tick(&self.world, &mut self.out);
        let took = t0.elapsed();
        self.minds_time += took;
        self.minds_worst = self.minds_worst.max(took);
        self.out.extend_from_slice(extra);
        self.world.tick(&self.out);
        self.battle.judge(&self.world, &self.set.parties);
        self.battle.outcome.is_some()
    }
}
