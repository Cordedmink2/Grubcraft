//! Field Battles from the lobby (docs/06-game-modes.md): raising and placing
//! each side's army, seating a computer player in each AI slot, and judging
//! who holds the field at the end.

use gc_ai::army::{self, Preset};
use gc_ai::perception::{fx, to_fx_vec, v2};
use gc_ai::{Difficulty, Faction, Mind, MindConfig, Minds};
use gc_fixed::Fx;
use gc_sim::champions::{HEART, Outcome};
use gc_sim::{
    Challenge, Command, HEART_HOLD_TICKS, PlayerCommand, PlayerId, TICKS_PER_SECOND, UnitStats,
    Win, World,
};
use gc_ui::Settings;
use gc_ui::lobby::{
    COLORS, GameMode, LobbySettings, MAX_ARMY, MatchStart, PLAYER_COLORS, Slot, SlotKind,
};
use glam::Vec2;

use gc_map::START_PLATEAUS;

// A start plateau for every lobby slot.
const _: () = assert!(START_PLATEAUS == gc_ui::lobby::MAX_PLAYERS);

/// Whether champions may duel before this battle: a Field Battle the lobby
/// allows it in, with a computer player taking part (docs/05-combat.md,
/// "Champions before battle").
pub fn champions_allowed(setup: &MatchStart) -> bool {
    setup.settings.mode == GameMode::FieldBattle
        && setup.settings.champions
        && setup.players.iter().any(|s| s.kind == SlotKind::Ai)
}

/// The champions' result stays on screen this long after the duel ends.
const NEWS_TICKS: u32 = 8 * TICKS_PER_SECOND;

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

/// Who plays lobby slot `slot`: "Red (Mario)", in red, as their faction.
pub fn party_of(slot: &Slot) -> Party {
    let (colour, color) = PLAYER_COLORS[usize::from(slot.color % COLORS)];
    Party {
        name: format!("{colour} ({})", slot.name),
        color,
        faction: Some(Faction::from_index(slot.faction.index() as u8)),
    }
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
/// `minds`. Each army is what its player built in the army builder, held to
/// the lobby's points budget, or if they built none (and for every AI) the
/// largest army in a preset mix from the faction's roster that the budget
/// buys. `units`, `factions` and `points` describe the regiment types;
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
    points: &[u32],
    plateaus: [Vec2; START_PLATEAUS],
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
    let middle = plateaus.iter().copied().sum::<Vec2>() / START_PLATEAUS as f32;
    // People get a deployment phase; computer players alone don't need one.
    let deployment = players.iter().any(|s| s.kind == SlotKind::Human);
    let mut set = SetOut {
        parties: Vec::with_capacity(players.len()),
        me: me as PlayerId,
        my_start: starts.get(me).copied().unwrap_or(middle),
        my_forward: Vec2::NEG_Y,
    };
    // Every zone goes out before any army or Ready, so the deployment can't
    // end before a late zone arrives when the setup spans several turns.
    let (mut spawns, mut ready) = (Vec::new(), Vec::new());
    for (i, slot) in players.iter().enumerate() {
        let party = i as PlayerId;
        let faction = Faction::from_index(slot.faction.index() as u8);
        set.parties.push(party_of(slot));
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
        let budget = setup.settings.budget.points();
        let raised = if slot.army.is_empty() {
            army::pick_army_within(units, points, factions, faction, preset, budget, MAX_ARMY)
        } else {
            army::within_budget(&slot.army, points, factions, faction, budget, MAX_ARMY)
        };
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
            if deployment {
                out.push(zone_order(party, &placed, &starts));
            }
            army::spawn_commands(party, &placed, &mut spawns);
            // The computer keeps the standard deployment.
            if deployment && slot.kind == SlotKind::Ai {
                ready.push(PlayerCommand {
                    player: party,
                    command: Command::Ready,
                });
            }
        }
        if let Some(mind) = mind {
            minds.add(mind);
        }
    }
    out.append(&mut spawns);
    out.append(&mut ready);
    set
}

/// While `player` deploys, slide a planned group order (its `MoveTo`s, which
/// place regiments while deploying) as a whole until every regiment's spot
/// is inside their zone, so a line drawn past the edge keeps its shape
/// instead of being refused.
pub fn shift_into_zone(world: &World, player: PlayerId, commands: &mut [Command]) {
    let Some(zone) = world.deploy_zone(player) else {
        return;
    };
    let centre = v2(zone.center);
    let room = (fx(zone.radius) - 1.0).max(0.0);
    let mut shift = Vec2::ZERO;
    // A few passes settle a line that sticks out on more than one side.
    for _ in 0..4 {
        for c in commands.iter() {
            if let Command::MoveTo { target, .. } = *c {
                let out = v2(target) + shift - centre;
                let excess = out.length() - room;
                if excess > 0.0 {
                    shift -= out.normalize_or_zero() * excess;
                }
            }
        }
    }
    if shift == Vec2::ZERO {
        return;
    }
    for c in commands {
        if let Command::MoveTo { target, .. } = c {
            *target = to_fx_vec(v2(*target) + shift);
        }
    }
}

/// `party`'s deployment zone round `starts[party]`, holding its standard
/// deployment `placed` (the zones of `starts` stay clear of each other).
pub(crate) fn zone_order(
    party: PlayerId,
    placed: &[army::Placement],
    starts: &[Vec2],
) -> PlayerCommand {
    let start = starts[party as usize];
    PlayerCommand {
        player: party,
        command: Command::DeployZone {
            center: to_fx_vec(start),
            radius: Fx::from_int(zone_radius(placed, starts, party as usize)),
        },
    }
}

/// Smallest deployment zone, as a radius in tiles.
const MIN_ZONE: i32 = 24;
/// Room in a deployment zone around the standard deployment, tiles.
const ZONE_MARGIN: f32 = 10.0;

/// A deployment zone round `starts[who]` that holds the standard deployment
/// `placed` with room to spare, but is no wider than half the way to the
/// nearest other start, so that neighbours (the middle plateaus are only a
/// quarter of the map from the corners) never share ground. The deployment
/// itself always fits, whatever the room.
fn zone_radius(placed: &[army::Placement], starts: &[Vec2], who: usize) -> i32 {
    let start = starts[who];
    let reach = placed
        .iter()
        .map(|p| p.position.distance(start))
        .fold(0.0, f32::max);
    let wanted = ((reach + ZONE_MARGIN).ceil() as i32).max(MIN_ZONE);
    let room = starts
        .iter()
        .enumerate()
        .filter(|&(j, _)| j != who)
        .map(|(_, other)| (other.distance(start) / 2.0).floor() as i32 - 1)
        .min()
        .unwrap_or(i32::MAX);
    wanted.min(room).max(reach.ceil() as i32)
}

/// Where each party of a Field Battle starts, from the map's six start
/// plateaus (`plateaus` in the order north-west, north-east, south-west,
/// south-east, north, south). The camera looks north, so this machine's
/// player and its allies take the south and everyone else the north, then
/// what is left. The lobby has six slots, so there is a plateau for everyone.
pub fn battle_starts(teams: &[u8], me: usize, plateaus: [Vec2; START_PLATEAUS]) -> Vec<Vec2> {
    // Four or fewer keep to the corners; the middles are for five or six.
    let crowd = teams.len() > 4;
    let mut south = vec![plateaus[2], plateaus[3]];
    let mut north = vec![plateaus[0], plateaus[1]];
    if crowd {
        south.push(plateaus[5]);
        north.push(plateaus[4]);
    }
    let mine = teams.get(me).copied();
    // Me first, then my allies, then everyone else, each in slot order.
    let mut order: Vec<usize> = (0..teams.len()).collect();
    order.sort_by_key(|&p| (p != me, Some(teams[p]) != mine, p));
    let mut starts = vec![plateaus[0]; teams.len()];
    // Five or six in three teams with pairs among them (2v2v2, 2v2v1): each team
    // takes a column of the map, south first, so allies stand together:
    // mine the west, the next team the east, the last the middle.
    let mut sides: Vec<u8> = Vec::with_capacity(3);
    for &p in &order {
        if !sides.contains(&teams[p]) {
            sides.push(teams[p]);
        }
    }
    let size = |t: u8| teams.iter().filter(|&&x| x == t).count();
    if crowd
        && sides.len() == 3
        && sides.iter().all(|&t| size(t) <= 2)
        && sides.iter().any(|&t| size(t) == 2)
    {
        let columns = [
            [plateaus[2], plateaus[0]],
            [plateaus[3], plateaus[1]],
            [plateaus[5], plateaus[4]],
        ];
        let mut filled = [0; 3];
        for p in order {
            let c = sides.iter().position(|&t| t == teams[p]).unwrap_or(0);
            starts[p] = columns[c][filled[c]];
            filled[c] += 1;
        }
        return starts;
    }
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

/// A lobby Field Battle under way: whose side this machine plays, and how
/// it ended.
pub struct Battle {
    /// None while watching a replay: nobody here won or lost.
    pub me: Option<PlayerId>,
    /// "VICTORY" or "DEFEAT" and what happened, once it is over.
    pub outcome: Option<String>,
    /// Whether this machine's side won, once it is over.
    pub won: Option<bool>,
    /// What the champions are doing, while there is news of them.
    pub champions: Option<String>,
    /// The state of the duel that news tells.
    heard: Option<Outcome>,
    /// Conquest: who is running out of time to lay a main building, and
    /// who holds the Titan's Heart.
    pub throne: Option<String>,
    /// The second of the match and whether it was decided when `throne`
    /// was last written: its clocks count whole seconds.
    looked: Option<(u32, bool)>,
}

impl Battle {
    pub fn new(me: Option<PlayerId>) -> Battle {
        Battle {
            me,
            outcome: None,
            won: None,
            champions: None,
            heard: None,
            throne: None,
            looked: None,
        }
    }

    /// Follow Conquest's countdowns: players with no main building left
    /// and the Titan's Heart. Returns true when the news changed.
    pub fn watch_throne(&mut self, world: &World, parties: &[Party]) -> bool {
        let look = (
            world.tick_count() / TICKS_PER_SECOND,
            self.outcome.is_some(),
        );
        if self.looked.replace(look) == Some(look) {
            return false;
        }
        let news = (self.outcome.is_none())
            .then(|| throne_news(world, parties, self.me))
            .flatten();
        if news == self.throne {
            return false;
        }
        self.throne = news;
        true
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

    /// Once the simulation has decided the battle (see
    /// [`gc_sim::verdict`]), say how. Returns true when it was just decided.
    pub fn judge(&mut self, world: &World, parties: &[Party]) -> bool {
        if self.outcome.is_some() {
            return false;
        }
        let Some(v) = world.verdict() else {
            return false;
        };
        let p = v.holder;
        // A frame can play several ticks: the time it was decided.
        let ticks = world
            .battle_ticks()
            .saturating_sub(world.tick_count() - v.tick);
        let time = gc_ai::lens::clock(ticks);
        let name = party_name(parties, p);
        let how = match v.by {
            Win::Field => "holds the field",
            Win::Annihilation => "is the last standing",
            Win::Heart => "holds the Titan's Heart",
            Win::Draw => "",
        };
        let won = self.me.map(|me| v.by != Win::Draw && !world.hostile(me, p));
        self.won = won;
        let said = match (won, v.by) {
            (_, Win::Draw) => "OVER  nobody is left standing".into(),
            (None, _) => format!("OVER  {name} {how}"),
            (Some(false), _) => format!("DEFEAT  {name} {how}"),
            (Some(true), Win::Field) => "VICTORY  the field is yours".into(),
            (Some(true), Win::Annihilation) => "VICTORY  every enemy is out".into(),
            (Some(true), Win::Heart) => "VICTORY  the Titan's Heart is yours".into(),
        };
        self.outcome = Some(format!("{said} after {time}. Esc: menu"));
        true
    }
}

/// Conquest's countdowns as `me` hears them (nobody, watching a replay):
/// a line for each player with no main building left, and one for the
/// Titan's Heart once it can be held.
fn throne_news(world: &World, parties: &[Party], me: Option<PlayerId>) -> Option<String> {
    let now = world.tick_count();
    let mut lines: Vec<String> = (0..parties.len() as PlayerId)
        .filter_map(|p| {
            let left = gc_ai::lens::clock(world.exiled(p)?.saturating_sub(now));
            Some(if Some(p) == me {
                format!("YOUR LAST MAIN BUILDING IS DOWN  lay one within {left} or you are out")
            } else {
                format!(
                    "{} has {left} to lay a main building or is out",
                    party_name(parties, p)
                )
            })
        })
        .collect();
    if me.is_some_and(|me| world.is_out(me)) {
        lines.push("YOU ARE OUT  your regiments and buildings have left the field".into());
    }
    if let Some(heart) = world.heart() {
        let battle = world.battle_ticks();
        match heart.held {
            Some((p, since)) => {
                let held = gc_ai::lens::clock(now - since);
                let of = gc_ai::lens::clock(HEART_HOLD_TICKS);
                let who = match me {
                    Some(me) if !world.hostile(me, p) => "Your side holds".to_string(),
                    _ => format!("{} holds", party_name(parties, p)),
                };
                lines.push(format!("HEART  {who} the Titan's Heart: {held} of {of}"));
            }
            // Said for a while once it opens.
            None if heart.open && battle < heart.unlock + HEART_NEWS_TICKS => {
                lines.push(format!(
                    "HEART  The Titan's Heart can be held: hold it for {} to win",
                    gc_ai::lens::clock(HEART_HOLD_TICKS)
                ));
            }
            None => {}
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// How long the Heart's opening is news.
const HEART_NEWS_TICKS: u32 = 15 * TICKS_PER_SECOND;

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
/// The AI plays your faction, on the Proving Ground made from `map_seed`.
pub fn quick_battle(level: u8, settings: &Settings, map_seed: u64) -> MatchStart {
    let mut me = Slot::open(0);
    me.kind = SlotKind::Human;
    me.name.clone_from(&settings.player_name);
    me.faction = settings.faction;
    me.color = settings.player_color % COLORS;
    let mut ai = Slot::open(1);
    ai.kind = SlotKind::Ai;
    ai.name = "AI 2".into();
    ai.faction = me.faction;
    ai.color = (me.color + 1) % COLORS;
    ai.ai_level = level;
    MatchStart {
        settings: LobbySettings {
            map_seed,
            ..LobbySettings::default()
        },
        players: vec![me, ai],
        local_player: 0,
        seed: None,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::terrain_view::plateau_positions;
    use gc_map::Map as Heightmap;
    use gc_sim::{Capacities, MatchSetup};

    pub(crate) const MAP: u32 = 256;

    #[test]
    fn you_and_your_allies_start_in_the_south_facing_the_rest() {
        let p = plateau_positions(MAP);
        let (nw, ne, sw, se, n, s) = (p[0], p[1], p[2], p[3], p[4], p[5]);
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
        // Three against three: you and your allies fill the south (you
        // first), the others the north.
        let teams = [0, 1, 0, 1, 0, 1];
        let trio = battle_starts(&teams, 0, p);
        assert_eq!(trio, vec![sw, nw, se, ne, s, n]);
        // The same, seen from the other side's first slot.
        assert_eq!(battle_starts(&teams, 1, p), vec![nw, sw, ne, se, n, s]);
        // Three pairs: each pair holds a column, yours the west.
        let pairs = battle_starts(&[0, 1, 2, 0, 1, 2], 0, p);
        assert_eq!(pairs, vec![sw, se, s, nw, ne, n]);
        // Three alone still face each other across the map, and four or
        // fewer keep to the corners whatever their teams.
        assert_eq!(battle_starts(&[0, 1, 2], 0, p), vec![sw, nw, ne]);
        assert_eq!(battle_starts(&[0, 1, 2, 3], 0, p), vec![sw, nw, ne, se]);
        assert_eq!(battle_starts(&[0, 1, 2, 0], 0, p), vec![sw, nw, ne, se]);
        // Six for themselves: six different plateaus.
        let ffa = battle_starts(&[0, 1, 2, 3, 4, 5], 3, p);
        assert_eq!(ffa[3], sw);
        for (i, a) in ffa.iter().enumerate() {
            assert!(p.contains(a));
            for b in &ffa[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    /// The AI level called `name`, or the Veteran for no name.
    pub(crate) fn level(name: &str) -> Difficulty {
        if name.is_empty() {
            return Difficulty::Veteran;
        }
        Difficulty::ALL
            .into_iter()
            .find(|d| d.name() == name)
            .unwrap_or_else(|| panic!("no AI level called {name:?}"))
    }

    /// The real game data's unit stats, factions and points, as `set_out`
    /// takes them.
    pub(crate) fn tables(data: &gc_data::GameData) -> (Vec<UnitStats>, Vec<Faction>, Vec<u32>) {
        let units = data.units.iter().map(crate::sandbox::stats_of).collect();
        let factions = data
            .units
            .iter()
            .map(|u| Faction::from_index(u.faction() as u8))
            .collect();
        let points = data.units.iter().map(|u| u32::from(u.points)).collect();
        (units, factions, points)
    }

    /// A lobby setup: slot `k` is `who[k]` (None for you), on team
    /// `teams[k]`, everyone Ember Clans so the armies match.
    pub(crate) fn lobby(who: &[Option<Difficulty>], teams: &[u8]) -> MatchStart {
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

    pub(crate) struct Played {
        battle: Battle,
        /// Who holds the field, once someone does.
        pub(crate) holder: Option<PlayerId>,
        pub(crate) ticks: u32,
        /// Regiments each party fielded.
        fielded: Vec<usize>,
        /// Time all the minds spent thinking, and the longest single tick.
        ai_time: std::time::Duration,
        ai_worst: std::time::Duration,
    }

    /// Who fields what: a built army as built (held to the budget), and
    /// the budget's worth of the preset mix for an AI or a player who built
    /// nothing.
    #[test]
    fn armies_are_what_was_built_within_the_budget() {
        let data = crate::data::load();
        let (units, factions, points) = tables(&data);
        let (yari, _) = data.unit_by_name("Yari Ashigaru").unwrap();
        let (yumi, _) = data.unit_by_name("Yumi Ashigaru").unwrap();
        let (pikes, _) = data.unit_by_name("Bone Pikes").unwrap();
        let heightmap = Heightmap::generate(1, MAP);
        let nav = heightmap.nav_costs();
        let fielded = |setup: &MatchStart| {
            let mut minds = Minds::new(crate::terrain_view::ai_terrain(&heightmap, &nav));
            let mut out = Vec::new();
            set_out(
                setup,
                &units,
                &factions,
                &points,
                plateau_positions(MAP),
                1,
                true,
                &mut minds,
                &mut out,
            );
            let mut per = vec![Vec::new(); setup.players.len()];
            for c in &out {
                if let gc_sim::Command::SpawnRegiment { unit, .. } = c.command {
                    per[c.player as usize].push(unit);
                }
            }
            per
        };
        let mut setup = lobby(&[None, Some(Difficulty::Veteran)], &[0, 1]);
        let spent = |army: &[u16]| army.iter().map(|&u| points[u as usize]).sum::<u32>();
        // Nothing built: both sides get the Battle budget's worth.
        for army in fielded(&setup) {
            assert!(
                spent(&army) <= 4_000 && spent(&army) > 4_000 - 600,
                "{army:?}"
            );
        }
        // A built army, with another faction's regiment slipped in, and a
        // Skirmish budget it overspends.
        setup.players[0].army = vec![yari, yari, pikes, yumi, yumi, yari, yari, yari, yari];
        setup.settings.budget = gc_ui::lobby::Budget::Skirmish;
        let armies = fielded(&setup);
        let mut mine = armies[0].clone();
        mine.sort_unstable();
        let mut want = vec![yari, yari, yumi, yumi, yari];
        want.sort_unstable();
        assert_eq!(mine, want, "five fit in 2,000");
        assert!(spent(&armies[1]) <= 2_000);
    }

    /// People get a deployment zone round their army and place regiments
    /// inside it; computer players are ready at once, and a battle of
    /// computers alone has no deployment.
    #[test]
    fn people_deploy_and_computers_are_ready_at_once() {
        let data = crate::data::load();
        let (units, factions, points) = tables(&data);
        let heightmap = Heightmap::generate(3, MAP);
        let nav = heightmap.nav_costs();
        let start = |setup: &MatchStart| {
            let teams: Vec<u8> = setup.players.iter().map(|s| s.team).collect();
            let mut world = World::new(&MatchSetup {
                seed: 3,
                capacities: Capacities::default(),
                units: &units,
                map_tiles: MAP,
                nav: &nav,
                teams: &teams,
                ground: heightmap.ground(),
                champions: false,
            });
            let mut minds = Minds::new(crate::terrain_view::ai_terrain(&heightmap, &nav));
            let mut out = Vec::new();
            set_out(
                setup,
                &units,
                &factions,
                &points,
                plateau_positions(MAP),
                3,
                true,
                &mut minds,
                &mut out,
            );
            let stats = world.tick(&out);
            assert_eq!(stats.commands_rejected, 0);
            world
        };

        let world = start(&lobby(
            &[Some(Difficulty::Recruit), Some(Difficulty::Recruit)],
            &[0, 1],
        ));
        assert!(!world.deploying(), "computers alone don't deploy");

        let mut world = start(&lobby(&[None, Some(Difficulty::Veteran)], &[0, 1]));
        assert!(world.deploying());
        assert!(!world.deploy_ready(0) && world.deploy_ready(1));
        for r in world.regiments.iter() {
            let zone = world.deploy_zone(r.owner).unwrap();
            assert!(zone.contains(r.position), "{r:?}");
        }
        // A line drawn past the zone's edge slides in whole.
        let mine = world.regiments.iter().find(|r| r.owner == 0).unwrap();
        let zone = world.deploy_zone(0).unwrap();
        let edge = v2(zone.center) + Vec2::new(fx(zone.radius) + 5.0, 0.0);
        let mut line = [0.0, 6.0].map(|dy| Command::MoveTo {
            regiment: mine.handle,
            target: to_fx_vec(edge + Vec2::new(0.0, dy)),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        });
        shift_into_zone(&world, 0, &mut line);
        let [a, b] = line.map(|c| match c {
            Command::MoveTo { target, .. } => target,
            _ => unreachable!(),
        });
        assert!(zone.contains(a) && zone.contains(b));
        assert!(((v2(b) - v2(a)) - Vec2::new(0.0, 6.0)).length() < 0.01);
        // While deploying, that order puts the regiment straight down there.
        let (handle, facing) = (mine.handle, mine.facing);
        let open = world.nav().walkable(a);
        world.tick(&[
            PlayerCommand {
                player: 0,
                command: line[0],
            },
            PlayerCommand {
                player: 0,
                command: Command::Ready,
            },
        ]);
        assert!(!world.deploying());
        if open {
            let r = world.regiments.get(handle).unwrap();
            assert_eq!((r.position, r.facing), (a, facing));
        }
    }

    /// With six on the field, no two deployment zones overlap, even the
    /// neighbouring plateaus (64 tiles apart on the 256-tile map).
    #[test]
    fn six_deployment_zones_do_not_overlap() {
        let data = crate::data::load();
        let (units, factions, points) = tables(&data);
        let heightmap = Heightmap::generate(3, MAP);
        let nav = heightmap.nav_costs();
        let ai = Some(Difficulty::Recruit);
        // Grand Battle armies are wider than the room between neighbours on
        // this map: their zones only hold the deployment, and may touch.
        for budget in [gc_ui::lobby::Budget::Skirmish, gc_ui::lobby::Budget::Battle] {
            let mut setup = lobby(&[None, ai, ai, ai, ai, ai], &[0, 1, 0, 1, 0, 1]);
            setup.settings.budget = budget;
            let teams: Vec<u8> = setup.players.iter().map(|s| s.team).collect();
            let mut world = World::new(&MatchSetup {
                seed: 3,
                capacities: Capacities::default(),
                units: &units,
                map_tiles: MAP,
                nav: &nav,
                teams: &teams,
                ground: heightmap.ground(),
                champions: false,
            });
            let mut minds = Minds::new(crate::terrain_view::ai_terrain(&heightmap, &nav));
            let mut out = Vec::new();
            set_out(
                &setup,
                &units,
                &factions,
                &points,
                plateau_positions(MAP),
                3,
                true,
                &mut minds,
                &mut out,
            );
            assert_eq!(world.tick(&out).commands_rejected, 0);
            let zones: Vec<_> = (0..6).map(|p| world.deploy_zone(p).unwrap()).collect();
            for (i, a) in zones.iter().enumerate() {
                for b in &zones[i + 1..] {
                    let apart = (v2(a.center) - v2(b.center)).length();
                    assert!(
                        apart > fx(a.radius) + fx(b.radius),
                        "{budget:?}: {a:?} overlaps {b:?}"
                    );
                }
            }
        }
    }

    /// Play `setup` on the generated map with the real game data, the human
    /// side standing still, for at most `minutes`.
    fn play(setup: &MatchStart, seed: u64, minutes: u32) -> Played {
        play_on(setup, seed, minutes, Heightmap::generate(seed, MAP), |_| {})
    }

    /// [`play`] on `heightmap`, with `tune` free to change the minds once
    /// they are seated.
    pub(crate) fn play_on(
        setup: &MatchStart,
        seed: u64,
        minutes: u32,
        mut heightmap: Heightmap,
        tune: impl FnOnce(&mut Minds),
    ) -> Played {
        let data = crate::data::load();
        let (units, factions, points) = tables(&data);
        // `AI_COVER=off` plays on bare maps, without woods or tall grass.
        if std::env::var("AI_COVER").as_deref() == Ok("off") {
            heightmap.cover.clear();
        }
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
        let mut minds = Minds::new(crate::terrain_view::ai_terrain(&heightmap, &nav));
        // As in a real game (for measuring what the minds cost the game's
        // thread): the battle may not play out the same way twice.
        if std::env::var_os("AI_LIVE").is_some() {
            minds = minds.live();
        }
        let mut out = Vec::new();
        let set = set_out(
            setup,
            &units,
            &factions,
            &points,
            plateau_positions(MAP),
            seed,
            true,
            &mut minds,
            &mut out,
        );
        tune(&mut minds);
        world.tick(&out);
        // The people are happy with the standard deployment.
        let ready: Vec<PlayerCommand> = (0..setup.players.len())
            .filter(|&p| setup.players[p].kind == SlotKind::Human)
            .map(|p| PlayerCommand {
                player: p as PlayerId,
                command: Command::Ready,
            })
            .collect();
        world.tick(&ready);
        assert!(!world.deploying());
        let fielded = (0..setup.players.len())
            .map(|p| {
                world
                    .regiments
                    .iter()
                    .filter(|r| r.owner == p as PlayerId)
                    .count()
            })
            .collect();
        let mut battle = Battle::new(Some(set.me));
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
                        "  {} u{} ({:5.1},{:5.1}) {:>2} {:?}{}{}",
                        r.owner,
                        r.unit,
                        p.x,
                        p.y,
                        r.models,
                        r.morale,
                        if r.engaged { " ENG" } else { "" },
                        if world.hidden(r.handle) {
                            " HIDDEN"
                        } else {
                            ""
                        }
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
            holder: world.verdict().map(|v| v.holder),
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

    /// Three computers against three: every plateau is used, every army is
    /// raised, and the battle plays out to a result.
    #[test]
    fn six_computers_fight_a_three_against_three() {
        let ai = Some(Difficulty::Veteran);
        let setup = lobby(&[ai; 6], &[0, 1, 0, 1, 0, 1]);
        let played = play(&setup, 0x6772_7562, 10);
        assert_eq!(played.fielded, vec![army::ARMY_REGIMENTS; 6]);
        println!(
            "{:?} after {} s",
            played.battle.outcome,
            played.ticks / gc_sim::TICKS_PER_SECOND
        );
        assert!(played.battle.outcome.is_some(), "no result in ten minutes");
    }

    /// Each faction's Veteran against each faction standing still, four
    /// battles a pair: how many the AI wins and loses, and which it leaves
    /// undecided after eight minutes (it never found the last survivors).
    /// `AI_LEVEL=General` (or any level's name) plays that level instead.
    /// `cargo test -p gc_game --release matchups -- --ignored --nocapture`
    #[test]
    #[ignore = "slow: 64 full battles, run by hand"]
    fn matchups() {
        use gc_ui::lobby::Faction as F;
        let ai_level = level(&std::env::var("AI_LEVEL").unwrap_or_default());
        let mut pairs = Vec::new();
        for ai in F::ALL {
            for idle in F::ALL {
                pairs.push((ai, idle));
            }
        }
        println!("{} against armies standing still", ai_level.name());
        let rows: Vec<(u32, u32, Vec<String>)> = std::thread::scope(|s| {
            let jobs: Vec<_> = pairs
                .iter()
                .map(|&(ai, idle)| {
                    s.spawn(move || {
                        let mut setup = lobby(&[None, Some(ai_level)], &[0, 1]);
                        setup.players[0].faction = idle;
                        setup.players[1].faction = ai;
                        let (mut won, mut lost, mut open) = (0, 0, Vec::new());
                        for seed in 1..=4 {
                            match play(&setup, seed, 8).battle.won {
                                Some(false) => won += 1,
                                Some(true) => lost += 1,
                                None => open.push(format!(
                                    "{} v {} seed {seed}",
                                    ai.name(),
                                    idle.name()
                                )),
                            }
                        }
                        (won, lost, open)
                    })
                })
                .collect();
            jobs.into_iter().map(|j| j.join().unwrap()).collect()
        });
        for (&(ai, idle), (won, ..)) in pairs.iter().zip(&rows) {
            println!(
                "{:>15} AI v {:>15} standing still: AI won {won} of 4",
                ai.name(),
                idle.name()
            );
        }
        let won: u32 = rows.iter().map(|r| r.0).sum();
        let lost: u32 = rows.iter().map(|r| r.1).sum();
        let open: Vec<&String> = rows.iter().flat_map(|r| &r.2).collect();
        println!(
            "AI won {won}, lost {lost}, undecided {}: {open:?}",
            open.len()
        );
    }

    /// The balance table (docs/17-unit-rosters.md, "Field Battle balance"): each pair of
    /// factions, two Veterans, `AI_BATTLES` battles a pair (default 24),
    /// swapping ends every battle, with champions allowed unless
    /// `AI_CHAMPIONS=off`, on maps with woods unless `AI_COVER=off`.
    /// Prints the first faction's wins, losses and battles nobody had won
    /// after ten minutes.
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
    #[ignore = "opens a window to watch a battle"]
    fn watch() {
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
