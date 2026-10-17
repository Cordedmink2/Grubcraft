//! The computer players of a lobby Conquest match: what each one knows of
//! its faction before the match starts (its [`Kit`]), and seating a
//! Conquest mind on every AI slot. Like Field Battle's minds they run on
//! the host (or alone) and act only through ordinary commands, so nothing
//! here touches the simulation (docs/18-ai.md, "The big decision").
//!
//! The tests below also hold a headless runner that plays whole Conquest
//! matches between minds with no window, to watch them and to check they
//! can finish a game.

use std::sync::Arc;

use gc_ai::conquest::kit::{EconomyStyle, Kit, KitBuilding, KitUnit};
use gc_ai::conquest::opening::Opening;
use gc_ai::{Difficulty, Mind, MindConfig, Minds};
use gc_data::conquest::Class;
use gc_data::{Faction, GameData};
use gc_fixed::Pcg32;
use gc_map::Map;
use gc_ui::lobby::{MatchStart, SlotKind};
use glam::Vec2;

use crate::buildings::Table;
use crate::conquest;

/// What a mind of `faction` knows before the match starts, the way a
/// player knows their faction's build menu: its building and unit types
/// (ids are the baked tables' indexes) and where the map's starts are.
pub fn kit(data: &GameData, table: &Table, map: &Map, faction: Faction) -> Kit {
    let buildings = (0..data.buildings.len())
        .filter(|&i| data.buildings[i].faction() == faction)
        .map(|i| KitBuilding {
            id: i as u16,
            stats: table.stats[i],
        })
        .collect();
    let units: Vec<KitUnit> = data
        .conquest_units
        .iter()
        .enumerate()
        .filter(|(_, rec)| rec.faction() == faction)
        .map(|(u, rec)| {
            let unit = conquest::unit_of(rec);
            // A regiment for the line: trained at a building, costing
            // something and taking supply, and not a worker, caravan,
            // healer or Reclaimer.
            let fighter = !unit.worker
                && rec.class() != Class::Support
                && unit.trained_at.is_some()
                && unit.supply > 0
                && unit.cost_grub + unit.cost_marrow > 0;
            KitUnit {
                id: u as u16,
                unit,
                fighter,
            }
        })
        .collect();
    Kit {
        // gc_data and gc_ai list the factions in the same order.
        faction,
        economy: units
            .iter()
            .find(|u| u.unit.worker)
            .map_or(EconomyStyle::Shared, |w| EconomyStyle::of(&w.unit)),
        buildings,
        units,
        all_workers: data
            .conquest_units
            .iter()
            .enumerate()
            .filter(|(_, rec)| rec.worker())
            .map(|(u, _)| u as u16)
            .collect(),
        all_units: data
            .conquest_units
            .iter()
            .map(|rec| (rec.faction(), conquest::unit_of(rec).supply))
            .collect(),
        starts: map
            .starts()
            .into_iter()
            .map(|(x, y)| Vec2::new(x as f32 + 0.5, y as f32 + 0.5))
            .collect(),
    }
}

/// The temper (personality roll) of player `i`'s mind in a game rolled
/// `roll`.
fn temper(roll: u64, i: usize) -> u32 {
    Pcg32::new(roll, i as u64).next_u32()
}

/// Put a Conquest mind in charge of every AI slot of `setup` (player `i`
/// is slot `i`), picking each one's temper and seed from `roll` as Field
/// Battle does. The caller does this on the host or alone only.
pub fn seat(
    setup: &MatchStart,
    data: &GameData,
    table: &Table,
    map: &Map,
    roll: u64,
    minds: &mut Minds,
) {
    let sides: Vec<(Faction, u8)> = setup.players.iter().map(|s| (s.faction, s.team)).collect();
    for (i, slot) in setup.players.iter().enumerate() {
        if slot.kind != SlotKind::Ai {
            continue;
        }
        let level = Difficulty::from_index(usize::from(slot.ai_level));
        let temper = temper(roll, i);
        minds.add(mind_for(
            data, table, map, i, &sides, level, temper, roll, None,
        ));
    }
}

/// The Conquest mind for player `i` of `sides` (each player's faction and
/// team) at `level`, its personality picked by `temper` and its seed from
/// `roll`; it picks its own opening unless given one.
#[allow(clippy::too_many_arguments)]
fn mind_for(
    data: &GameData,
    table: &Table,
    map: &Map,
    i: usize,
    sides: &[(Faction, u8)],
    level: Difficulty,
    temper: u32,
    roll: u64,
    opening: Option<&'static Opening>,
) -> Mind {
    let (faction, team) = sides[i];
    let config = MindConfig {
        party: i as u8,
        difficulty: level,
        personality: gc_ai::personality::pick(faction, temper),
        seed: roll ^ i as u64,
    };
    // The factions it faces: its allies are no rivals.
    let rivals: Vec<Faction> = sides
        .iter()
        .filter(|&&(_, t)| t != team)
        .map(|&(f, _)| f)
        .collect();
    let kit = Arc::new(kit(data, table, map, faction));
    Mind::conquest(config, kit, &rivals, opening)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_ai::conquest::opening;
    use gc_sim::economy::Bank;
    use gc_sim::{
        Capacities, MatchSetup, PlayerId, TICKS_PER_SECOND, Verdict, Victories, Win, World,
    };
    use gc_ui::lobby::Tempo;

    /// How one side stood when a headless match ended.
    pub(crate) struct Standing {
        pub faction: Faction,
        /// The mind's one-line summary of what it is doing.
        pub summary: String,
        /// The AI lens's panel for the mind, after the summary line.
        pub lens: Vec<String>,
        pub bank: Bank,
        pub buildings: usize,
        pub regiments: usize,
        /// Orders the mind sent: its battle layer's, and its economy
        /// clicks.
        pub orders: u32,
        pub clicks: u32,
    }

    /// A headless Conquest match played to its end (or the time limit).
    pub(crate) struct Played {
        pub verdict: Option<Verdict>,
        pub ticks: u32,
        pub sides: Vec<Standing>,
    }

    impl Played {
        /// The side that won, if the match was decided and not drawn.
        pub fn winner(&self) -> Option<PlayerId> {
            self.verdict.filter(|v| v.by != Win::Draw).map(|v| v.holder)
        }

        /// The result and how each side stood, one line apiece.
        pub fn report(&self) -> String {
            let clock = |t: u32| {
                format!(
                    "{}:{:02}",
                    t / 60 / TICKS_PER_SECOND,
                    t / TICKS_PER_SECOND % 60
                )
            };
            let mut out = match self.verdict {
                Some(v) if v.by == Win::Draw => {
                    format!("draw ({:?}) at {}", v.by, clock(v.tick))
                }
                Some(v) => format!("side {} wins by {:?} at {}", v.holder, v.by, clock(v.tick)),
                None => format!("undecided after {}", clock(self.ticks)),
            };
            for (p, s) in self.sides.iter().enumerate() {
                out += &format!("\n  {}", s.line(p));
            }
            out
        }
    }

    impl Standing {
        /// "side 0 EmberClans: bank 300/0, supply 8/10, ..." and, below,
        /// the mind's summary and what its lens panel says.
        pub fn line(&self, p: usize) -> String {
            let b = &self.bank;
            let lens: String = self.lens.iter().map(|l| format!("\n  {l}")).collect();
            format!(
                "side {p} {:?}: bank {}/{}, supply {}/{}, {} buildings, {} regiments, \
                 {} orders ({} economy)\n    {}{lens}",
                self.faction,
                b.grub,
                b.marrow,
                b.supply_used,
                b.supply_cap,
                self.buildings,
                self.regiments,
                self.orders,
                self.clicks,
                self.summary
            )
        }
    }

    /// One side's numbers now: its bank and what it owns.
    fn standing(world: &World, minds: &Minds, p: PlayerId, faction: Faction) -> Standing {
        let mind = minds.get(p).expect("a mind plays every side");
        Standing {
            faction,
            summary: mind.summary(),
            lens: mind.lens(minds.view()).text.split_off(1),
            bank: world.bank(p).unwrap_or_default(),
            buildings: world
                .structures()
                .map_or(0, |s| s.iter().filter(|v| v.owner == p).count()),
            regiments: world.regiments.iter().filter(|r| r.owner == p).count(),
            orders: mind.orders_sent(),
            clicks: mind.realm().map_or(0, |r| r.clicks().count),
        }
    }

    /// Play Conquest map `choice` between Conquest minds, `factions[i]` at
    /// start `i` thinking at `levels[i]` (the last level covers any
    /// more), with no window, until a side wins or `max_minutes` of game
    /// time pass. Fog of war and victories are as the lobby's defaults.
    /// With `AI_TRACE=1` it prints every side's progress each two game
    /// minutes.
    pub(crate) fn play(
        choice: usize,
        factions: &[Faction],
        levels: &[Difficulty],
        seed: u64,
        max_minutes: u32,
    ) -> Played {
        play_with(choice, factions, levels, seed, max_minutes, None, &[])
    }

    /// [`play`], every side with the personality `temper` picks for its
    /// faction if given (for the arena, so only the level differs).
    pub(crate) fn play_with(
        choice: usize,
        factions: &[Faction],
        levels: &[Difficulty],
        seed: u64,
        max_minutes: u32,
        temper: Option<u32>,
        openings: &[Option<&'static Opening>],
    ) -> Played {
        let trace = std::env::var("AI_TRACE").unwrap_or_default();
        // AI_TRACE=2 also prints every journal entry as it is written.
        let journal = trace == "2";
        let trace = journal || trace == "1";
        let data = crate::data::load();
        let table = Table::new(&data);
        let map = conquest::make_map(choice, seed);
        let units = conquest::unit_stats(&data);
        let nav = map.nav_costs();
        let teams: Vec<u8> = (0..factions.len() as u8).collect();
        let sides: Vec<(Faction, u8)> = factions.iter().copied().zip(teams.clone()).collect();
        let setup = MatchSetup {
            seed,
            capacities: Capacities::default(),
            units: &units,
            map_tiles: map.tiles,
            nav: &nav,
            teams: &teams,
            ground: map.ground(),
            champions: false,
        };
        let defaults = gc_ui::lobby::LobbySettings::default();
        let plan = conquest::plan(
            &table,
            &data,
            &map,
            factions,
            Tempo::Standard,
            conquest::victories(defaults.victory),
        );
        assert!(plan.victories.any() && plan.victories != Victories::default());
        let mut world = World::new(&setup);
        assert!(world.conquest(&plan.setup()));
        let mut minds = Minds::new(gc_match::terrain_view::ai_terrain(&map, &nav));
        for i in 0..factions.len() {
            let level = levels[i.min(levels.len() - 1)];
            let temper = temper.unwrap_or_else(|| super::temper(seed, i));
            let opening = openings.get(i).copied().flatten();
            minds.add(mind_for(
                &data, &table, &map, i, &sides, level, temper, seed, opening,
            ));
        }
        let opening = conquest::opening(&data, &table, &map, &world, factions);
        let stats = world.tick(&opening);
        assert_eq!(stats.commands_applied as usize, opening.len());
        let end = max_minutes * 60 * TICKS_PER_SECOND;
        let every = 120 * TICKS_PER_SECOND;
        let mut cmds = Vec::new();
        let mut noted = vec![0u32; factions.len()];
        while world.verdict().is_none() && world.tick_count() < end {
            cmds.clear();
            minds.tick(&world, &mut cmds);
            world.tick(&cmds);
            let t = world.tick_count();
            if journal {
                for (p, last) in noted.iter_mut().enumerate() {
                    let Some(m) = minds.get(p as PlayerId) else {
                        continue;
                    };
                    let from = *last;
                    for e in m.journal().entries().filter(|e| e.tick > from) {
                        println!(
                            "{}:{:02} side {p}: {}",
                            e.tick / 1200,
                            e.tick / 20 % 60,
                            e.text
                        );
                        *last = e.tick;
                    }
                }
            }
            if trace && t.is_multiple_of(every) {
                println!("-- {}:00", t / 60 / TICKS_PER_SECOND);
                for (p, &f) in factions.iter().enumerate() {
                    let s = standing(&world, &minds, p as PlayerId, f);
                    let last = minds
                        .get(p as PlayerId)
                        .and_then(|m| m.journal().latest())
                        .map_or(String::new(), |e| format!("[{}] {}", e.tick, e.text));
                    println!("{}\n    last: {last}", s.line(p));
                }
            }
        }
        Played {
            verdict: world.verdict(),
            ticks: world.tick_count(),
            sides: factions
                .iter()
                .enumerate()
                .map(|(p, &f)| standing(&world, &minds, p as PlayerId, f))
                .collect(),
        }
    }

    /// The levels named by `AI_LEVEL`, a name or an index each, one per
    /// side and comma-separated (the last covers any more sides); Veteran
    /// if unset.
    fn levels_from_env() -> Vec<Difficulty> {
        let var = std::env::var("AI_LEVEL").unwrap_or_default();
        var.split(',')
            .map(|v| level_named(v).unwrap_or(Difficulty::Veteran))
            .collect()
    }

    /// Openings named in `var`, a comma list, one a side (unset or
    /// unknown: the side picks its own).
    fn openings_from_env(var: &str) -> Vec<Option<&'static Opening>> {
        let v = std::env::var(var).unwrap_or_default();
        v.split(',').map(opening::find).collect()
    }

    /// A level by its name or its number.
    fn level_named(v: &str) -> Option<Difficulty> {
        let v = v.trim();
        Difficulty::ALL
            .into_iter()
            .find(|d| d.name().eq_ignore_ascii_case(v))
            .or_else(|| v.parse().ok().map(Difficulty::from_index))
    }

    fn env_number(name: &str, default: u64) -> u64 {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    #[test]
    fn a_kit_holds_the_factions_own_build_menu_and_the_maps_starts() {
        let data = crate::data::load();
        let table = Table::new(&data);
        let map = conquest::make_map(1, 0);
        for f in Faction::ALL {
            let k = kit(&data, &table, &map, f);
            assert_eq!(k.faction as u8, f as u8);
            assert!(k.main().is_some(), "{f:?} has a main building");
            assert!(k.worker().is_some(), "{f:?} has a worker");
            assert!(k.housing().is_some(), "{f:?} has housing");
            assert!(k.units.iter().any(|u| u.fighter), "{f:?} has fighters");
            assert!(k.buildings.iter().all(|b| {
                data.buildings[usize::from(b.id)].faction() == f
                    && b.stats == table.stats[usize::from(b.id)]
            }));
            assert!(k.units.iter().all(|u| !(u.fighter && u.unit.worker)));
            assert!(k.all_workers.len() >= Faction::ALL.len());
            assert_eq!(k.starts.len(), 4);
            assert_eq!(
                k.starts[0],
                Vec2::new(
                    map.starts()[0].0 as f32 + 0.5,
                    map.starts()[0].1 as f32 + 0.5
                )
            );
        }
    }

    /// Watch a match: `AI_MAP`, `AI_SEED`, `AI_MINUTES` and `AI_LEVEL`
    /// choose it, `AI_TRACE=1` prints progress.
    ///
    /// ```text
    /// AI_TRACE=1 AI_MINUTES=12 cargo test -p gc_game --release conquest_ai_watch -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "a watching aid, run by hand"]
    fn conquest_ai_watch() {
        let choice = env_number("AI_MAP", 0) as usize;
        let seed = env_number("AI_SEED", 7);
        let minutes = env_number("AI_MINUTES", 40) as u32;
        let levels = levels_from_env();
        let players = gc_ui::lobby::CONQUEST_MAP_PLAYERS[choice];
        // AI_FACTIONS picks them by their index in `Faction::ALL`, such as
        // "12" for the Brood against the Choir.
        let picked = std::env::var("AI_FACTIONS").unwrap_or_default();
        let factions: Vec<Faction> = (0..players)
            .map(|i| match (picked.as_bytes().get(i), players) {
                (Some(c), _) => Faction::ALL[usize::from(c - b'0') % Faction::ALL.len()],
                (None, 2) => [Faction::EmberClans, Faction::CarrionConcord][i],
                (None, _) => Faction::ALL[i % 4],
            })
            .collect();
        let started = std::time::Instant::now();
        // AI_SAME=1 gives every side the same personality, as the arena does.
        let same = std::env::var("AI_SAME").is_ok_and(|v| v == "1");
        let temper = same.then(|| super::temper(seed, 0));
        let openings = openings_from_env("AI_OPENINGS");
        let played = play_with(choice, &factions, &levels, seed, minutes, temper, &openings);
        println!("{}", played.report());
        println!(
            "{} game minutes in {:.1?}",
            played.ticks / 60 / TICKS_PER_SECOND,
            started.elapsed()
        );
    }

    /// The Conquest arena (docs/18-ai.md, "Difficulty"): how often level
    /// `AI_A` beats `AI_B` one-on-one on Ribcage Pass, in mirror matches
    /// (both sides the same faction and personality, so only the level
    /// counts),
    /// each faction on each side, over `AI_GAMES` seeds per pairing.
    /// `cargo test --release -p gc_game conquest_arena -- --ignored --nocapture`
    #[test]
    #[ignore = "an arena run, not a check: run it by hand with --release"]
    fn conquest_arena() {
        let find = |var: &str, or: Difficulty| {
            level_named(&std::env::var(var).unwrap_or_default()).unwrap_or(or)
        };
        let a = find("AI_A", Difficulty::General);
        let b = find("AI_B", Difficulty::Veteran);
        // AI_OPEN_A and AI_OPEN_B set each level's opening, by name.
        let (open_a, open_b) = (
            openings_from_env("AI_OPEN_A"),
            openings_from_env("AI_OPEN_B"),
        );
        let games = env_number("AI_GAMES", 3);
        let (mut won, mut lost, mut drawn) = (0, 0, 0);
        for f in Faction::ALL {
            let (mut fw, mut fl) = (0, 0);
            for seed in 1..=games {
                for a_side in 0..2u8 {
                    let levels = if a_side == 0 { [a, b] } else { [b, a] };
                    // The same game with the sides swapped.
                    let roll = seed * 31;
                    let temper = Some(super::temper(roll, 0));
                    let (oa, ob) = (open_a[0], open_b[0]);
                    let openings = if a_side == 0 { [oa, ob] } else { [ob, oa] };
                    let played = play_with(0, &[f, f], &levels, roll, 45, temper, &openings);
                    match played.winner() {
                        Some(w) if w == a_side => (won, fw) = (won + 1, fw + 1),
                        Some(_) => (lost, fl) = (lost + 1, fl + 1),
                        None => drawn += 1,
                    }
                }
            }
            println!("{f:?}: {} won {fw}, {} won {fl}", a.name(), b.name());
        }
        let all = (won + lost + drawn).max(1);
        println!(
            "{} beat {} in {won} of {all} ({}%), lost {lost}, {drawn} undecided",
            a.name(),
            b.name(),
            won * 100 / all
        );
    }

    /// The lens panel of a Conquest mind has its economy and belief lines
    /// after the battle lines, and what the army waits for once it is
    /// gathering.
    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_lens_panel_shows_its_economy_belief_and_plan() {
        let factions = [Faction::EmberClans, Faction::CarrionConcord];
        let played = play(0, &factions, &[Difficulty::Veteran], 7, 8);
        let mut weighed = false;
        for (p, side) in played.sides.iter().enumerate() {
            let find = |label: &str| {
                let start = format!("  {label:<7} ");
                side.lens.iter().position(|l| l.starts_with(&start))
            };
            let roles = find("roles").expect("battle lines come first");
            let economy = find("economy").unwrap_or_else(|| panic!("side {p}: {:?}", side.lens));
            assert!(economy > roles, "{:?}", side.lens);
            assert!(
                side.lens[economy].contains("keeps 70% workers, 2 producers a tier"),
                "{}",
                side.lens[economy]
            );
            assert!(side.lens[economy].contains("saving: "));
            assert!(side.lens[economy].contains("arming: "));
            assert_eq!(find("belief"), Some(economy + 1), "{:?}", side.lens);
            weighed |= side.lens[economy + 1].contains(", odds ");
            if let Some(plan) = find("plan") {
                assert_eq!(plan, economy + 2, "{:?}", side.lens);
            }
            assert!(side.lens.iter().all(|l| l.starts_with("  ")));
        }
        // By eight minutes the armies have met, and somebody weighed up
        // the other's.
        assert!(weighed, "{:#?}", played.sides[0].lens);
    }

    // A game takes seconds in a release build and minutes in a debug one,
    // so these run in CI's release step.
    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_ribcage_1v1_ends_in_a_win() {
        let pairs = [
            [Faction::EmberClans, Faction::CarrionConcord],
            [Faction::Brood, Faction::Choir],
        ];
        for (factions, seed) in pairs.iter().zip([7, 8]) {
            let played = play(0, factions, &[Difficulty::Veteran], seed, 45);
            assert!(played.winner().is_some(), "{}", played.report());
        }
    }

    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_four_teeth_ends_in_a_win() {
        for seed in [7, 8] {
            let played = play(1, &Faction::ALL, &[Difficulty::Veteran], seed, 90);
            assert!(played.winner().is_some(), "{}", played.report());
        }
    }

    /// Six computer players on a big Wilds map (the first choice past the
    /// shipped maps).
    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_six_player_wilds_ends_in_a_win() {
        use gc_ai::personality::Faction::*;
        let factions = [EmberClans, Brood, Choir, CarrionConcord, EmberClans, Brood];
        let wilds = gc_map::SHIPPED.len();
        let played = play(wilds, &factions, &[Difficulty::Veteran], 1, 120);
        assert!(played.winner().is_some(), "{}", played.report());
    }
}
