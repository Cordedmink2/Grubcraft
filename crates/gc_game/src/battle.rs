//! Field Battles from the lobby (docs/06-game-modes.md): raising and placing
//! each side's army, seating a computer player in each AI slot, and judging
//! who holds the field at the end.

use gc_ai::army::{self, Preset};
use gc_ai::{Difficulty, Faction, Mind, MindConfig, Minds};
use gc_sim::champions::{HEART, Outcome};
use gc_sim::{Challenge, PlayerCommand, PlayerId, UnitStats, World};
use gc_ui::Settings;
use gc_ui::lobby::{LobbySettings, MatchStart, PLAYER_COLORS, Slot, SlotKind};
use glam::Vec2;

/// Whether champions may duel before this battle: the lobby allows it and
/// a computer player takes part (docs/05-combat.md, "Champions before
/// battle").
pub fn champions_allowed(setup: &MatchStart) -> bool {
    setup.settings.champions && setup.players.iter().any(|s| s.kind == SlotKind::Ai)
}

/// A Field Battle is won once one side alone has held the field (everyone
/// still fighting is on its team) for this long, so a rout that might
/// rally doesn't end it early.
pub const HOLD_TICKS: u32 = 10 * gc_sim::TICKS_PER_SECOND;

/// The champions' result stays on screen this long after the duel ends.
const NEWS_TICKS: u32 = 8 * gc_sim::TICKS_PER_SECOND;

/// One side of a match.
pub struct Party {
    /// "red" in the sandbox, "Red (Mario)" in a lobby battle.
    pub name: String,
    pub color: [u8; 3],
    /// Its faction, when the lobby chose one.
    pub faction: Option<Faction>,
}

/// A side's name, or its number past the party list.
pub fn party_name(parties: &[Party], side: PlayerId) -> String {
    parties
        .get(side as usize)
        .map_or_else(|| format!("player {}", side + 1), |p| p.name.clone())
}

/// A battle set out: who plays each side, and where this machine's player
/// stands and faces.
pub struct SetOut {
    pub parties: Vec<Party>,
    pub me: PlayerId,
    pub my_start: Vec2,
    pub my_forward: Vec2,
}

/// Raise and deploy every side's army for `setup`, appending the commands
/// that put them on the field to `out`, and add a mind for each AI slot to
/// `minds`. Until the army builder and its points budget arrive, every army
/// is ten regiments in a preset mix from its faction's roster. `units` and
/// `factions` describe the regiment types;
/// `plateaus` are the map's start positions (see [`battle_starts`]);
/// `roll` varies the AI personalities from match to match. `deploy` is
/// false on a joiner in a network match: the host raises every army and
/// runs the computer players, and the deployment reaches each machine in
/// the first turn, so this only works out who stands where.
///
/// Who starts where is worked out from this machine's point of view (it
/// takes the south), except in a network match, where every machine must
/// agree and the host's view is used: the host takes the south, and a
/// joiner may find itself in the north.
#[allow(clippy::too_many_arguments)]
pub fn set_out(
    setup: &MatchStart,
    units: &[UnitStats],
    factions: &[Faction],
    plateaus: [Vec2; 4],
    roll: u64,
    deploy: bool,
    minds: &mut Minds,
    out: &mut Vec<PlayerCommand>,
) -> SetOut {
    let players = &setup.players;
    let me = setup.local_player.min(players.len().saturating_sub(1));
    let teams: Vec<u8> = players.iter().map(|s| s.team).collect();
    let perspective = if setup.seed.is_some() { 0 } else { me };
    let starts = battle_starts(&teams, perspective, plateaus);
    let middle = plateaus.iter().copied().sum::<Vec2>() / 4.0;
    let mut set = SetOut {
        parties: Vec::with_capacity(players.len()),
        me: me as PlayerId,
        my_start: starts.get(me).copied().unwrap_or(middle),
        my_forward: Vec2::NEG_Y,
    };
    for (i, slot) in players.iter().enumerate() {
        let party = i as PlayerId;
        let faction = Faction::from_index(slot.faction.index() as u8);
        let (colour, color) = PLAYER_COLORS[slot.color as usize % PLAYER_COLORS.len()];
        set.parties.push(Party {
            name: format!("{colour} ({})", slot.name),
            color,
            faction: Some(faction),
        });
        let mind = (deploy && slot.kind == SlotKind::Ai).then(|| {
            let pick = gc_fixed::Pcg32::new(roll, i as u64).next_u32();
            Mind::new(MindConfig {
                party,
                difficulty: Difficulty::from_index(slot.ai_level as usize),
                personality: gc_ai::personality::pick(faction, pick),
                seed: roll ^ i as u64,
            })
        });
        let preset = mind
            .as_ref()
            .map_or(Preset::Balanced, |m| m.personality().army);
        let raised = army::pick_army(units, factions, faction, preset, army::ARMY_REGIMENTS);
        // Face the nearest enemy start (the middle of the map if none).
        let foe = (0..players.len())
            .filter(|&j| teams[j] != teams[i])
            .map(|j| starts[j])
            .min_by(|a, b| a.distance(starts[i]).total_cmp(&b.distance(starts[i])))
            .unwrap_or(middle);
        let forward = (foe - starts[i]).normalize_or(Vec2::NEG_Y);
        if i == me {
            set.my_forward = forward;
        }
        if deploy {
            let placed = army::deploy(units, &raised, starts[i], forward, minds.terrain());
            army::spawn_commands(party, &placed, out);
        }
        if let Some(mind) = mind {
            minds.add(mind);
        }
    }
    set
}

/// Where each party of a Field Battle starts, from the map's four start
/// plateaus (`plateaus` in the order north-west, north-east, south-west,
/// south-east). The camera looks north, so this machine's player and its
/// allies take the south and everyone else the north, then what is left.
/// The lobby has four slots, so there is a plateau for everyone.
pub fn battle_starts(teams: &[u8], me: usize, plateaus: [Vec2; 4]) -> Vec<Vec2> {
    let mut south = vec![plateaus[2], plateaus[3]];
    let mut north = vec![plateaus[0], plateaus[1]];
    let mine = teams.get(me).copied();
    // Me first, then my allies, then everyone else, each in slot order.
    let mut order: Vec<usize> = (0..teams.len()).collect();
    order.sort_by_key(|&p| (p != me, Some(teams[p]) != mine, p));
    let mut starts = vec![plateaus[0]; teams.len()];
    for p in order {
        let (near, far) = if Some(teams[p]) == mine {
            (&mut south, &mut north)
        } else {
            (&mut north, &mut south)
        };
        let side = if near.is_empty() { far } else { near };
        if !side.is_empty() {
            starts[p] = side.remove(0);
        }
    }
    starts
}

/// The side left holding the field: some side with regiments still
/// fighting in sight, when every enemy still fighting is out of its sight.
/// None while enemies still face each other, or nobody stands.
///
/// Regiments hidden in woods or tall grass don't hold the field
/// (docs/20-ambushes.md): an army whose regiments in sight have all broken
/// has lost it, however many it still has hiding, so a beaten army's
/// survivors rallying in a wood don't drag a battle on. Two armies that
/// are both out of each other's sight are still facing each other. With no
/// enemy left standing at all, hiding doesn't matter.
pub fn holding_the_field(world: &World) -> Option<PlayerId> {
    let standing = || {
        world
            .regiments
            .iter()
            .filter(|r| r.models > 0 && !r.morale.broken())
    };
    standing().map(|r| r.owner).find(|&p| {
        let foes = || standing().filter(|e| world.hostile(p, e.owner));
        let alone = foes().next().is_none();
        standing().any(|r| r.owner == p && (alone || !world.hidden(r.handle)))
            && foes().all(|e| !world.sees(p, e.handle))
    })
}

/// A lobby Field Battle under way: whose side this machine plays, and how
/// it ended.
pub struct Battle {
    pub me: PlayerId,
    /// The side holding the field and the tick it took it, while one does.
    held: Option<(PlayerId, u32)>,
    /// "VICTORY" or "DEFEAT" and what happened, once it is over.
    pub outcome: Option<String>,
    /// Whether this machine's side won, once it is over.
    pub won: Option<bool>,
    /// What the champions are doing, while there is news of them.
    pub champions: Option<String>,
    /// The state of the duel that news tells.
    heard: Option<Outcome>,
}

impl Battle {
    pub fn new(me: PlayerId) -> Battle {
        Battle {
            me,
            held: None,
            outcome: None,
            won: None,
            champions: None,
            heard: None,
        }
    }

    /// Follow the champions' duel: who steps out, then how it ended, for a
    /// few seconds. Returns true when the news changed.
    pub fn herald(&mut self, world: &World, parties: &[Party], unit_names: &[String]) -> bool {
        let now = world.tick_count();
        let duel = world
            .challenge()
            .filter(|c| now <= c.decided() + NEWS_TICKS)
            .map(|c| (c, c.outcome(now)));
        let heard = duel.map(|(_, o)| o);
        if heard == self.heard {
            return false;
        }
        self.heard = heard;
        self.champions = duel.map(|(c, o)| champion_news(c, o, world, parties, unit_names));
        true
    }

    /// Look at the field after a tick; once one side alone has held it
    /// long enough, decide the battle. Returns true when it was just decided.
    pub fn judge(&mut self, world: &World, parties: &[Party]) -> bool {
        if self.outcome.is_some() {
            return false;
        }
        let now = world.tick_count();
        self.held = match (self.held, holding_the_field(world)) {
            // Allies take turns being the first regiment standing.
            (Some((p, since)), Some(h)) if !world.hostile(p, h) => Some((p, since)),
            (_, Some(h)) => Some((h, now)),
            (_, None) => None,
        };
        let Some((p, since)) = self.held else {
            return false;
        };
        if now - since < HOLD_TICKS {
            return false;
        }
        let secs = now / gc_sim::TICKS_PER_SECOND;
        let time = format!("{}:{:02}", secs / 60, secs % 60);
        let won = !world.hostile(self.me, p);
        self.won = Some(won);
        self.outcome = Some(if won {
            format!("VICTORY  the field is yours after {time}. Esc: menu")
        } else {
            format!(
                "DEFEAT  {} holds the field after {time}. Esc: menu",
                party_name(parties, p)
            )
        });
        true
    }
}

/// The line about the champions' duel `c`, which stands at `outcome`.
fn champion_news(
    c: &Challenge,
    outcome: Outcome,
    world: &World,
    parties: &[Party],
    unit_names: &[String],
) -> String {
    let [a, b] = c.sides.map(|s| party_name(parties, s));
    match outcome {
        Outcome::Fighting => {
            // The regiments they step out of are there while they fight.
            let [x, y] = c.champions.map(|h| {
                world
                    .regiments
                    .get(h)
                    .and_then(|r| unit_names.get(r.unit as usize))
                    .map_or("regiment", String::as_str)
            });
            format!(
                "CHAMPIONS  {a}'s {x} and {b}'s {y} each send out a champion to fight between the lines"
            )
        }
        Outcome::CutShort => "CHAMPIONS  The battle swallows the duel before it is decided".into(),
        Outcome::Killed { winner } => {
            let (w, l) = if winner == 0 { (a, b) } else { (b, a) };
            format!(
                "CHAMPIONS  {w}'s champion kills {l}'s. {w}'s side takes heart (+{HEART} morale), {l}'s loses it (-{HEART})"
            )
        }
        Outcome::Parted => {
            "CHAMPIONS  Neither champion can beat the other, and both go back to their lines".into()
        }
    }
}

/// `--battle`: you against one AI of `level`, as the lobby would set it up.
/// The AI plays your faction.
pub fn quick_battle(level: u8, settings: &Settings) -> MatchStart {
    let mut me = Slot::open(0);
    me.kind = SlotKind::Human;
    me.name.clone_from(&settings.player_name);
    me.faction = settings.faction;
    me.color = settings.player_color % 4;
    let mut ai = Slot::open(1);
    ai.kind = SlotKind::Ai;
    ai.name = "AI 2".into();
    ai.faction = me.faction;
    ai.color = (me.color + 1) % 4;
    ai.ai_level = level;
    MatchStart {
        settings: LobbySettings::default(),
        players: vec![me, ai],
        local_player: 0,
        seed: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain_gen::{Heightmap, start_positions};
    use gc_sim::{Capacities, MatchSetup};

    const MAP: u32 = 256;

    #[test]
    fn you_and_your_allies_start_in_the_south_facing_the_rest() {
        let p = start_positions(MAP);
        let (nw, ne, sw, se) = (p[0], p[1], p[2], p[3]);
        // One against one.
        assert_eq!(battle_starts(&[0, 1], 0, p), vec![sw, nw]);
        // You in the second slot.
        assert_eq!(battle_starts(&[0, 1], 1, p), vec![nw, sw]);
        // Two against two, allies in slots 0 and 2.
        assert_eq!(battle_starts(&[0, 1, 0, 1], 0, p), vec![sw, nw, se, ne]);
        // Everyone for themselves: four different plateaus.
        let ffa = battle_starts(&[0, 1, 2, 3], 2, p);
        assert_eq!(ffa[2], sw);
        for (i, a) in ffa.iter().enumerate() {
            for b in &ffa[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    /// A lobby setup: slot `k` is `who[k]` (None for you), on team
    /// `teams[k]`, everyone Ember Clans so the armies match.
    fn lobby(who: &[Option<Difficulty>], teams: &[u8]) -> MatchStart {
        let players = who
            .iter()
            .zip(teams)
            .enumerate()
            .map(|(k, (ai, &team))| {
                let mut s = Slot::open(k);
                s.team = team;
                s.faction = gc_ui::lobby::Faction::EmberClans;
                match ai {
                    Some(level) => {
                        s.kind = SlotKind::Ai;
                        s.name = format!("AI {}", k + 1);
                        s.ai_level = level.index() as u8;
                    }
                    None => {
                        s.kind = SlotKind::Human;
                        s.name = "You".into();
                    }
                }
                s
            })
            .collect();
        MatchStart {
            settings: LobbySettings::default(),
            players,
            local_player: 0,
            seed: None,
        }
    }

    struct Played {
        battle: Battle,
        ticks: u32,
        /// Regiments each party fielded.
        fielded: Vec<usize>,
        /// Time all the minds spent thinking, and the longest single tick.
        ai_time: std::time::Duration,
        ai_worst: std::time::Duration,
    }

    /// Play `setup` on the generated map with the real game data, the human
    /// side standing still, for at most `minutes`.
    fn play(setup: &MatchStart, seed: u64, minutes: u32) -> Played {
        let data = crate::data::load();
        let units: Vec<UnitStats> = data.units.iter().map(crate::sandbox::stats_of).collect();
        let factions: Vec<Faction> = data
            .units
            .iter()
            .map(|u| Faction::from_index(u.faction() as u8))
            .collect();
        let heightmap = Heightmap::generate(seed, MAP);
        let nav = heightmap.nav_costs();
        let teams: Vec<u8> = setup.players.iter().map(|s| s.team).collect();
        let mut world = World::new(&MatchSetup {
            seed,
            capacities: Capacities::default(),
            units: &units,
            map_tiles: MAP,
            nav: &nav,
            teams: &teams,
            ground: heightmap.ground(),
            champions: champions_allowed(setup),
        });
        let mut minds = Minds::new(heightmap.ai_terrain(&nav));
        let mut out = Vec::new();
        let set = set_out(
            setup,
            &units,
            &factions,
            start_positions(MAP),
            seed,
            true,
            &mut minds,
            &mut out,
        );
        world.tick(&out);
        let fielded = (0..setup.players.len())
            .map(|p| {
                world
                    .regiments
                    .iter()
                    .filter(|r| r.owner == p as PlayerId)
                    .count()
            })
            .collect();
        let mut battle = Battle::new(set.me);
        let limit = minutes * 60 * gc_sim::TICKS_PER_SECOND;
        let (mut ai_time, mut ai_worst) = Default::default();
        while world.tick_count() < limit && battle.outcome.is_none() {
            out.clear();
            let t0 = std::time::Instant::now();
            minds.tick(&world, &mut out);
            let took = t0.elapsed();
            ai_time += took;
            ai_worst = took.max(ai_worst);
            world.tick(&out);
            battle.judge(&world, &set.parties);
            if std::env::var_os("AI_TRACE").is_some() && world.tick_count().is_multiple_of(400) {
                println!("t={}s", world.tick_count() / 20);
                for m in minds.iter() {
                    println!("  {}", m.summary());
                }
                for r in world.regiments.iter() {
                    let p = crate::from_fx_vec(r.position);
                    println!(
                        "  {} u{} ({:5.1},{:5.1}) {:>2} {:?}{}",
                        r.owner,
                        r.unit,
                        p.x,
                        p.y,
                        r.models,
                        r.morale,
                        if r.engaged { " ENG" } else { "" }
                    );
                }
            }
        }
        if std::env::var_os("AI_JOURNAL").is_some() {
            for m in minds.iter() {
                for e in m.journal().entries() {
                    println!("{} {:>5}: {}", m.party(), e.tick, e.text);
                }
            }
        }
        Played {
            battle,
            ticks: world.tick_count(),
            fielded,
            ai_time,
            ai_worst,
        }
    }

    #[test]
    fn the_ai_marches_out_and_beats_a_side_that_stands_still() {
        let setup = lobby(&[None, Some(Difficulty::Veteran)], &[0, 1]);
        let played = play(&setup, 0x6772_7562, 8);
        assert_eq!(played.fielded, vec![army::ARMY_REGIMENTS; 2]);
        println!(
            "{:?} after {} s",
            played.battle.outcome,
            played.ticks / gc_sim::TICKS_PER_SECOND
        );
        assert_eq!(
            played.battle.won,
            Some(false),
            "{:?}",
            played.battle.outcome
        );
    }

    /// Each faction's Veteran against each faction standing still.
    #[test]
    #[ignore = "slow: 64 full battles, run by hand"]
    fn matchups() {
        use gc_ui::lobby::Faction as F;
        for ai in F::ALL {
            for idle in F::ALL {
                let mut setup = lobby(&[None, Some(Difficulty::Veteran)], &[0, 1]);
                setup.players[0].faction = idle;
                setup.players[1].faction = ai;
                let mut won = 0;
                let mut sizes = Vec::new();
                for seed in 1..=4 {
                    let played = play(&setup, seed, 8);
                    won += u32::from(played.battle.won == Some(false));
                    sizes = played.fielded;
                }
                println!(
                    "{:>15} AI ({}) v {:>15} ({}) standing still: AI won {won} of 4",
                    ai.name(),
                    sizes[1],
                    idle.name(),
                    sizes[0]
                );
            }
        }
    }

    /// The balance table (docs/17-unit-rosters.md, "Field Battle balance"): each pair of
    /// factions, two Veterans, `AI_BATTLES` battles a pair (default 24),
    /// swapping ends every battle, with champions allowed unless
    /// `AI_CHAMPIONS=off`. Prints the first faction's wins, losses and
    /// battles nobody had won after ten minutes.
    /// `cargo test -p gc_game --release faction_table -- --ignored --nocapture`
    #[test]
    #[ignore = "slow: the full faction balance table, run by hand"]
    fn faction_table() {
        use gc_ui::lobby::Faction as F;
        let n: u64 = std::env::var("AI_BATTLES").map_or(24, |v| v.parse().unwrap());
        let from: u64 = std::env::var("AI_FROM").map_or(1, |v| v.parse().unwrap());
        let mut pairs = Vec::new();
        for (i, &a) in F::ALL.iter().enumerate() {
            for &b in &F::ALL[i + 1..] {
                pairs.push((a, b));
            }
        }
        // `AI_PAIR=Brood` plays only the pairs with the Brood in them,
        // `AI_PAIR=Brood,Choir` only that one.
        if let Ok(only) = std::env::var("AI_PAIR") {
            pairs.retain(|(a, b)| only.split(',').all(|f| f == a.name() || f == b.name()));
        }
        let rows: Vec<String> = std::thread::scope(|s| {
            let jobs: Vec<_> = pairs
                .iter()
                .map(|&(a, b)| {
                    s.spawn(move || {
                        let (mut won, mut lost) = (0, 0);
                        for seed in from..from + n {
                            let mut setup =
                                lobby(&[Some(Difficulty::Veteran), Some(Difficulty::Veteran)], &[0, 1]);
                            // `a` takes slot 0 (whose result `won` is) on even seeds.
                            let a_first = seed % 2 == 0;
                            setup.players[0].faction = if a_first { a } else { b };
                            setup.players[1].faction = if a_first { b } else { a };
                            // `AI_CHAMPIONS=off` plays them without champions.
                            setup.settings.champions =
                                std::env::var("AI_CHAMPIONS").as_deref() != Ok("off");
                            match play(&setup, seed, 10).battle.won {
                                Some(w) if w == a_first => won += 1,
                                Some(_) => lost += 1,
                                None => {}
                            }
                        }
                        format!(
                            "{:>15} v {:<15} won {won:>3}, lost {lost:>3}, undecided {:>3}  ({:.0}%)",
                            a.name(),
                            b.name(),
                            n - won - lost,
                            100.0 * won as f64 / n as f64
                        )
                    })
                })
                .collect();
            jobs.into_iter().map(|j| j.join().unwrap()).collect()
        });
        for row in rows {
            println!("{row}");
        }
    }

    #[test]
    fn a_winner_hidden_in_a_wood_holds_the_field_once_nobody_stands_against_it() {
        // A wood over the west half of a small map.
        let cover: Vec<u8> = (0..64 * 64)
            .map(|i| {
                if i % 64 < 32 {
                    gc_sim::Cover::Wood as u8
                } else {
                    0
                }
            })
            .collect();
        let units = gc_sim::scenario::DRILL_UNITS;
        let mut world = World::new(&MatchSetup {
            seed: 1,
            capacities: Capacities::default(),
            units: &units,
            map_tiles: 64,
            nav: &[],
            teams: &[],
            ground: gc_sim::Ground {
                heights: &[],
                water_level: 0,
                cover: &cover,
            },
            champions: false,
        });
        let spawn = |player, x| PlayerCommand {
            player,
            command: gc_sim::Command::SpawnRegiment {
                unit: 0,
                position: gc_fixed::FxVec2::new(
                    gc_fixed::Fx::from_int(x),
                    gc_fixed::Fx::from_int(32),
                ),
                facing: gc_fixed::Angle(0),
            },
        };
        world.tick(&[spawn(0, 12), spawn(1, 52)]);
        // The enemy in the open sees nothing of the hidden army: it holds.
        assert_eq!(holding_the_field(&world), Some(1));
        let enemy = world
            .regiments
            .iter()
            .find(|r| r.owner == 1)
            .unwrap()
            .handle;
        world.tick(&[PlayerCommand {
            player: 1,
            command: gc_sim::Command::RemoveRegiment { regiment: enemy },
        }]);
        // Nobody left against it: hiding doesn't matter.
        assert_eq!(holding_the_field(&world), Some(0));
    }

    #[test]
    #[ignore = "opens a window to watch a battle"]
    fn watch() {
        let level = |v: &str| match v {
            "Recruit" => Difficulty::Recruit,
            "General" => Difficulty::General,
            _ => Difficulty::Veteran,
        };
        let a = std::env::var("AI_A").map_or(None, |v| Some(level(&v)));
        let b = level(&std::env::var("AI_B").unwrap_or_default());
        let from = std::env::var("AI_FROM").map_or(1, |v| v.parse().unwrap());
        let n: u64 = std::env::var("AI_BATTLES").map_or(4, |v| v.parse().unwrap());
        for seed in from..from + n {
            let played = play(&lobby(&[a, Some(b)], &[0, 1]), seed, 10);
            println!(
                "seed {seed}: {:?} after {} s; minds {:.3} ms a tick on average, {:.2} ms at worst",
                played.battle.outcome,
                played.ticks / gc_sim::TICKS_PER_SECOND,
                played.ai_time.as_secs_f64() * 1000.0 / f64::from(played.ticks.max(1)),
                played.ai_worst.as_secs_f64() * 1000.0,
            );
        }
    }
}
