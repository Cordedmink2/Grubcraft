//! Several machines in one process on a network inside it
//! ([`MemoryNet`]), on a clock the test moves a millisecond at a time:
//! joining a lobby, starting a match, and playing it in lockstep through
//! lost, late, doubled and reordered packets, every world ending identical.
//! Every run plays out the same way. `udp.rs` checks the real sockets.

use std::net::SocketAddr;
use std::time::Duration;

use gc_fixed::{Angle, Fx, FxVec2};
use gc_net::net::{MemoryNet, Net};
use gc_net::proto::{HEADER_LEN, Kind, header};
use gc_net::wire::Writer;
use gc_net::{HASH_EVERY, INPUT_DELAY, Identity, Lan, MIN_DELAY, MatchEvent, TURN};
use gc_replay::{Recorder, Replay};
use gc_sim::scenario::{DRILL_UNITS, Drill};
use gc_sim::{Capacities, Command, MatchSetup, PlayerCommand, World};
use gc_ui::lobby::{
    DiscoveredGame, Faction, LobbySettings, MAX_PLAYERS, NetError, NetEvent, Network, OnDrop,
    Profile, SlotEdit, SlotKind,
};

const SEED: u64 = 0x6772_7562;

/// How far the clock moves between looks at the network: finer than any
/// game's frame.
const STEP: Duration = Duration::from_millis(1);

fn id(version: &str) -> Identity {
    Identity {
        version: version.into(),
        data_hash: 42,
        map_hash: |_, _| 7,
    }
}

fn profile(name: &str) -> Profile {
    Profile {
        name: name.into(),
        color: 0,
        faction: Faction::EmberClans,
        version: "test".into(),
    }
}

fn lan_as(net: &MemoryNet, id: Identity) -> Lan {
    Lan::on(id, Box::new(net.clone()))
}

fn lan(net: &MemoryNet) -> Lan {
    lan_as(net, id("test"))
}

fn address(host: &Lan) -> String {
    format!("127.0.0.1:{}", host.port().unwrap())
}

/// `joiner` joins `host`, which answers meanwhile.
fn join(net: &MemoryNet, joiner: &mut Lan, name: &str, host: &mut Lan) -> Result<(), NetError> {
    let target = address(host);
    join_with(net, joiner, name, &target, "", host)
}

fn join_with(
    net: &MemoryNet,
    joiner: &mut Lan,
    name: &str,
    target: &str,
    password: &str,
    host: &mut Lan,
) -> Result<(), NetError> {
    joiner.start_join(&profile(name), target, password)?;
    loop {
        net.advance(STEP);
        host.pump(net.now());
        if let Some(done) = joiner.joining() {
            return done;
        }
    }
}

/// Run every machine for `time`.
fn pass(net: &MemoryNet, lans: &mut [&mut Lan], time: Duration) {
    let end = net.now() + time;
    while net.now() < end {
        net.advance(STEP);
        for l in lans.iter_mut() {
            l.pump(net.now());
        }
    }
}

/// Run every machine for a moment.
fn settle(net: &MemoryNet, lans: &mut [&mut Lan]) {
    pass(net, lans, Duration::from_millis(300));
}

const SETUP: MatchSetup = MatchSetup {
    seed: SEED,
    capacities: Capacities { regiments: 1536 },
    units: &DRILL_UNITS,
    map_tiles: 256,
    nav: &[],
    teams: &[],
    ground: gc_sim::Ground::FLAT,
    champions: false,
    generals: true,
};

/// `n` machines' worlds, each recorded the way the game records a match.
fn machines(n: usize) -> (Vec<World>, Vec<Recorder>) {
    let worlds: Vec<World> = (0..n).map(|_| World::new(&SETUP)).collect();
    let recorders = worlds
        .iter()
        .map(|w| Recorder::new(&SETUP, w, "LAN test").checkpoint_every(HASH_EVERY))
        .collect();
    (worlds, recorders)
}

fn drill(players: u8) -> Drill {
    let mut d = Drill::new(SEED);
    d.regiments_per_player = 20;
    d.players = players;
    d
}

/// A match under way between `lans`, each playing the drill for its own
/// players (`mine[i]`) on its own copy of the world.
struct Match {
    net: MemoryNet,
    lans: Vec<Lan>,
    worlds: Vec<World>,
    /// What each machine played, as it would save it for a desync report.
    recorders: Vec<Recorder>,
    drills: Vec<Drill>,
    mine: Vec<Vec<u8>>,
    /// The most regiments each player has had on the host's field.
    peak: Vec<usize>,
    cmds: Vec<PlayerCommand>,
}

impl Match {
    /// Host a lobby on `net`, let `names` join and get ready, fill `ai`
    /// slots with computer players, and start.
    fn on(net: &MemoryNet, names: &[&str], ai: &[usize], on_drop: OnDrop) -> Match {
        let mut host = lan(net);
        host.host(
            &profile("Mario"),
            LobbySettings {
                on_drop,
                ..LobbySettings::default()
            },
            "",
        )
        .unwrap();
        let mut lans = vec![host];
        for name in names {
            let mut l = lan(net);
            join(net, &mut l, name, &mut lans[0]).unwrap();
            lans.push(l);
        }
        for &slot in ai {
            lans[0].edit_slot(slot, SlotEdit::Kind(SlotKind::Ai));
        }
        for l in &mut lans[1..] {
            let me = l.lobby().unwrap().local_slot;
            l.edit_slot(me, SlotEdit::Ready(true));
        }
        settle(net, &mut lans.iter_mut().collect::<Vec<_>>());
        let start = lans[0].start_match().unwrap();
        assert_eq!(start.seed, Some(SEED));
        settle(net, &mut lans.iter_mut().collect::<Vec<_>>());
        let mut mine = vec![vec![0u8]];
        mine[0].extend((names.len() + 1..start.players.len()).map(|p| p as u8));
        for l in &mut lans[1..] {
            match l.poll().as_slice() {
                [NetEvent::Started(s)] => mine.push(vec![s.local_player as u8]),
                other => panic!("{other:?}"),
            }
            assert_eq!(l.host_map().unwrap().map_hash, 7);
        }
        for l in &mut lans {
            l.loaded(Ok(()));
        }
        // The drill fields four players unless the match has more.
        let players = start.players.len().max(4) as u8;
        Match::playing(net, lans, mine, players)
    }

    fn start(names: &[&str], ai: &[usize], on_drop: OnDrop) -> Match {
        Match::on(&MemoryNet::new(SEED), names, ai, on_drop)
    }

    /// `lans`, their match started and loaded, play it.
    fn playing(net: &MemoryNet, lans: Vec<Lan>, mine: Vec<Vec<u8>>, players: u8) -> Match {
        let n = lans.len();
        let (worlds, recorders) = machines(n);
        let mut m = Match {
            net: net.clone(),
            lans,
            worlds,
            recorders,
            drills: (0..n).map(|_| drill(players)).collect(),
            mine,
            peak: vec![0; usize::from(players)],
            cmds: Vec::new(),
        };
        // The armies, ordered before the first turn.
        for i in 0..n {
            m.order(i);
        }
        m
    }

    /// Machine `i`'s orders for its own players. As in the game, the host
    /// raises every army.
    fn order(&mut self, i: usize) {
        self.drills[i].commands_for(&self.worlds[i], &mut self.cmds);
        let mine = &self.mine[i];
        self.lans[i].submit(self.cmds.drain(..).filter(|c| {
            if c.command.is_player_order() {
                mine.contains(&c.player)
            } else {
                i == 0
            }
        }));
    }

    /// Run every machine except `away` (cut off from the network and not
    /// playing) for `time` at most, or until every running world reaches
    /// `until` ticks.
    fn run(&mut self, away: &[usize], until: u32, time: Duration) {
        let end = self.net.now() + time;
        let running: Vec<usize> = (0..self.lans.len()).filter(|i| !away.contains(i)).collect();
        for (i, l) in self.lans.iter().enumerate() {
            self.net.cut(l.port().unwrap(), away.contains(&i));
        }
        while self.net.now() < end && running.iter().any(|&i| self.worlds[i].tick_count() < until) {
            self.net.advance(STEP);
            for &i in &running {
                let now = self.net.now();
                self.lans[i].pump(now);
                while self.worlds[i].tick_count() < until
                    && let Some(turn) = self.lans[i].next_turn(now)
                {
                    self.recorders[i].tick(&mut self.worlds[i], &turn);
                    let tick = self.worlds[i].tick_count();
                    // The recorder hashes as often as the host compares.
                    if let Some(hash) = self.recorders[i].hash_now() {
                        self.lans[i].record_hash(tick, hash);
                    }
                    if i == 0 {
                        for (p, peak) in self.peak.iter_mut().enumerate() {
                            let n = self.worlds[0]
                                .regiments
                                .iter()
                                .filter(|r| usize::from(r.owner) == p)
                                .count();
                            *peak = (*peak).max(n);
                        }
                    }
                    self.order(i);
                }
            }
        }
        for l in &self.lans {
            self.net.cut(l.port().unwrap(), false);
        }
    }

    /// Long enough to play `turns` turns with time to spare.
    fn turns(turns: u32) -> Duration {
        TURN * turns * 2 + Duration::from_secs(2)
    }

    fn ticks(&self) -> Vec<u32> {
        self.worlds.iter().map(gc_sim::World::tick_count).collect()
    }

    /// Let the last hashes reach the host, on a network that loses nothing.
    fn settle(&mut self) {
        self.net.set_loss(0);
        settle(&self.net, &mut self.lans.iter_mut().collect::<Vec<_>>());
    }

    fn same_worlds(&self) -> bool {
        let hash = self.worlds[0].state_hash();
        self.worlds.iter().all(|w| w.state_hash() == hash)
    }

    /// Every player's regiments made it into the match.
    fn every_army_raised(&self) {
        for (p, &n) in self.peak.iter().enumerate() {
            assert_eq!(n, 20, "player {p}");
        }
    }

    fn desyncs(&mut self) -> Vec<u32> {
        self.lans
            .iter_mut()
            .flat_map(gc_net::Lan::match_events)
            .filter_map(|e| match e {
                MatchEvent::Desync { tick } => Some(tick),
                _ => None,
            })
            .collect()
    }
}

fn notice(events: &[MatchEvent], text: &str) -> bool {
    events
        .iter()
        .any(|e| matches!(e, MatchEvent::Notice(t) if t == text))
}

#[test]
fn three_machines_play_in_lockstep_through_lost_late_and_doubled_packets() {
    let net = MemoryNet::new(SEED);
    let mut host = lan(&net);
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    let mut alex = lan(&net);
    let mut sam = lan(&net);
    join(&net, &mut alex, "Alex", &mut host).unwrap();
    join(&net, &mut sam, "Sam", &mut host).unwrap();
    settle(&net, &mut [&mut host, &mut alex, &mut sam]);

    // Everyone sees everyone; the joiners get ready and pick teams.
    for l in [&alex, &sam, &host] {
        let names: Vec<&str> = l
            .lobby()
            .unwrap()
            .slots
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names[..3], ["Mario", "Alex", "Sam"]);
    }
    host.edit_slot(3, SlotEdit::Kind(SlotKind::Ai));
    alex.edit_slot(1, SlotEdit::Team(0));
    // Alex can't touch Sam's slot.
    alex.edit_slot(2, SlotEdit::Team(0));
    for l in [&mut alex, &mut sam] {
        let me = l.lobby().unwrap().local_slot;
        l.edit_slot(me, SlotEdit::Ready(true));
    }
    settle(&net, &mut [&mut host, &mut alex, &mut sam]);
    let lobby = sam.lobby().unwrap();
    assert_eq!(lobby.slots[1].team, 0);
    assert_eq!(lobby.slots[2].team, 2);
    assert_eq!(lobby.slots[3].kind, SlotKind::Ai);

    let start = host.start_match().unwrap();
    assert_eq!(start.players.len(), 4);
    assert_eq!(start.seed, Some(SEED));
    settle(&net, &mut [&mut host, &mut alex, &mut sam]);
    let mut starts = Vec::new();
    for l in [&mut alex, &mut sam] {
        match l.poll().as_slice() {
            [NetEvent::Started(s)] => starts.push(s.clone()),
            other => panic!("{other:?}"),
        }
        assert_eq!(l.host_map().unwrap().map_hash, 7);
    }
    assert_eq!(starts[0].local_player, 1);
    assert_eq!(starts[1].local_player, 2);

    // A poor network: a packet in seven lost, one in twenty doubled, and
    // each taking 30 to 90 ms, so they overtake each other.
    net.set_loss(15);
    net.set_duplicate(5);
    net.set_latency(Duration::from_millis(30), Duration::from_millis(90));
    for l in [&mut host, &mut alex, &mut sam] {
        l.loaded(Ok(()));
    }

    // Host plays players 0 and 3 (the AI), Alex 1 and Sam 2.
    let mut m = Match::playing(
        &net,
        vec![host, alex, sam],
        vec![vec![0, 3], vec![1], vec![2]],
        4,
    );
    const TURNS: u32 = 300;
    m.run(&[], TURNS, Match::turns(TURNS));
    assert_eq!(m.ticks(), [TURNS; 3], "match stalled");
    m.settle();
    assert!(m.same_worlds());
    m.every_army_raised();
    assert!(TURNS.is_multiple_of(HASH_EVERY));
    assert_eq!(m.desyncs(), []);
    // Round trips longer than a turn: the delay rose from where it
    // started, and everyone heard.
    let delay = m.lans[0].input_delay().unwrap();
    assert!(delay > INPUT_DELAY, "{delay}");
    for l in &m.lans {
        assert_eq!(l.input_delay(), Some(delay));
    }

    // Sam's recording of the network match, saved and loaded, plays back
    // to the same world.
    let replay = Replay::decode(&m.recorders[2].replay().encode()).unwrap();
    assert_eq!(replay.ticks, TURNS);
    let back = replay.verify().expect("the recording plays back");
    assert_eq!(back.state_hash(), m.worlds[2].state_hash());
    assert_eq!(m.recorders[0].replay().first_difference(&replay), None);
}

#[test]
fn the_delay_drops_on_a_fast_network() {
    let mut m = Match::start(&["Alex"], &[], OnDrop::Wait);
    assert_eq!(m.lans[0].input_delay(), Some(INPUT_DELAY));
    m.run(&[], 100, Match::turns(100));
    assert_eq!(m.ticks(), [100, 100]);
    assert_eq!(m.lans[0].input_delay(), Some(MIN_DELAY));
    m.settle();
    assert_eq!(m.lans[1].input_delay(), Some(MIN_DELAY));
    assert!(m.same_worlds());
    assert_eq!(m.desyncs(), []);
}

#[test]
fn the_delay_follows_a_slow_spell_up_and_back_down() {
    let mut m = Match::start(&["Alex"], &[], OnDrop::Wait);
    m.run(&[], 100, Match::turns(100));
    assert_eq!(m.lans[0].input_delay(), Some(MIN_DELAY));

    // Round trips of a quarter of a second: a few turns of delay.
    m.net
        .set_latency(Duration::from_millis(120), Duration::from_millis(130));
    m.run(&[], 300, Match::turns(200));
    let slow = m.lans[0].input_delay().unwrap();
    assert!(slow >= 3, "{slow}");
    assert_eq!(m.ticks(), [300, 300]);

    // Fast again: back down, a turn at a time.
    m.net
        .set_latency(Duration::from_millis(1), Duration::from_millis(1));
    m.run(&[], 600, Match::turns(300));
    assert_eq!(m.ticks(), [600, 600]);
    assert_eq!(m.lans[0].input_delay(), Some(MIN_DELAY));
    m.settle();
    assert_eq!(m.lans[1].input_delay(), Some(MIN_DELAY));
    assert!(m.same_worlds());
    assert_eq!(m.desyncs(), []);
}

#[test]
fn the_match_waits_for_a_player_who_drops() {
    let mut m = Match::start(&["Alex", "Sam"], &[], OnDrop::Wait);
    m.run(&[], 100, Match::turns(100));
    assert_eq!(m.ticks(), [100; 3]);

    // Alex's Wi-Fi goes: after a few quiet seconds the host pauses.
    m.run(
        &[1],
        u32::MAX,
        gc_net::PLAYER_TIMEOUT + Duration::from_millis(500),
    );
    let paused_at = m.ticks();
    assert!(m.lans[0].paused_for_player());
    let waiting = m.lans[0].waiting().unwrap();
    assert!(waiting.starts_with("Paused: waiting for Alex"), "{waiting}");
    let sam_sees = m.lans[2].waiting().unwrap();
    assert!(
        sam_sees.starts_with("Paused: waiting for Alex"),
        "{sam_sees}"
    );
    m.run(&[1], u32::MAX, Duration::from_millis(300));
    assert_eq!(m.ticks()[0], paused_at[0], "the host played on");

    // Alex comes back: everyone carries on, in step.
    let target = paused_at[0] + 100;
    m.run(&[], target, Match::turns(200));
    assert_eq!(m.ticks(), [target; 3]);
    assert!(!m.lans[0].paused_for_player());
    assert_eq!(m.lans[2].waiting(), None);

    // Sam goes for good, and the host carries on without them.
    m.run(
        &[2],
        u32::MAX,
        gc_net::PLAYER_TIMEOUT + Duration::from_millis(500),
    );
    assert!(m.lans[0].paused_for_player());
    m.lans[0].carry_on();
    let target = m.ticks()[0] + 50;
    m.run(&[2], target, Match::turns(50));
    assert_eq!(&m.ticks()[..2], [target; 2]);
    m.settle();
    assert_eq!(m.worlds[0].state_hash(), m.worlds[1].state_hash());
    let notices: Vec<MatchEvent> = m.lans[1].match_events();
    assert!(notice(&notices, "Carrying on without Sam."), "{notices:?}");
    assert!(
        !notices
            .iter()
            .any(|e| matches!(e, MatchEvent::Desync { .. }))
    );
    assert_eq!(m.desyncs(), []);
}

#[test]
fn carry_on_plays_through_a_drop() {
    let mut m = Match::start(&["Alex"], &[2], OnDrop::CarryOn);
    m.run(&[], 50, Match::turns(50));
    let before = m.ticks()[0];
    let quiet = gc_net::PLAYER_TIMEOUT + Duration::from_millis(500);
    m.run(&[1], u32::MAX, quiet);
    assert!(!m.lans[0].paused_for_player());
    // The host never stopped: it played nearly every turn of that time.
    let played = m.ticks()[0] - before;
    let all = (quiet.as_millis() / TURN.as_millis()) as u32;
    assert!(played * 10 >= all * 9, "{played} of {all}");
    let events = m.lans[0].match_events();
    assert!(notice(&events, "Alex lost connection."), "{events:?}");
}

#[test]
fn a_joiner_cannot_use_the_sandbox_on_everyone() {
    let mut m = Match::start(&["Alex"], &[], OnDrop::Wait);
    m.run(&[], 20, Match::turns(20));
    let units = m.worlds[0].units().to_vec();
    // A tampered client tries every command only the host may send: take
    // the host's army off the field, retune a unit for everyone, raise more
    // troops and claim a deployment zone.
    let mut stats = units[0];
    stats.melee_damage += 50;
    let at = FxVec2::from_ints(128, 128);
    let mut cheats: Vec<Command> = m.worlds[1]
        .regiments
        .iter()
        .filter(|r| r.owner == 0)
        .map(|r| Command::RemoveRegiment { regiment: r.handle })
        .collect();
    cheats.push(Command::TuneUnit { unit: 0, stats });
    cheats.extend((0..10).map(|_| Command::SpawnRegiment {
        unit: 0,
        position: at,
        facing: Angle(0),
    }));
    cheats.push(Command::DeployZone {
        center: at,
        radius: Fx::from_int(200),
    });
    m.lans[1].submit(
        cheats
            .into_iter()
            .map(|command| PlayerCommand { player: 0, command }),
    );
    m.run(&[], 100, Match::turns(100));
    assert_eq!(m.ticks(), [100, 100]);
    m.settle();
    assert!(m.same_worlds());
    for w in &m.worlds {
        for p in 0..2u8 {
            let n = w.regiments.iter().filter(|r| r.owner == p).count();
            assert_eq!(n, 20, "player {p}");
        }
        assert_eq!(w.units(), units.as_slice());
        assert!(!w.deploying());
    }
    assert_eq!(m.desyncs(), []);
}

#[test]
fn a_desync_is_found_and_reported_everywhere() {
    let mut m = Match::start(&["Alex"], &[], OnDrop::Wait);
    m.run(&[], 40, Match::turns(40));
    // Something only Alex's machine does: an extra tick of its own.
    m.recorders[1].tick(&mut m.worlds[1], &[]);
    m.run(&[], 100, Match::turns(100));
    m.settle();
    let found = m.desyncs();
    // The host and Alex both hear, at the first hash after the split.
    assert_eq!(found, [50, 50], "{found:?}");
    // Their recordings, the desync reports, show where they parted.
    let (host, alex) = (m.recorders[0].replay(), m.recorders[1].replay());
    assert_eq!(host.first_difference(alex), Some(50));
}

#[test]
fn a_player_whose_game_restarts_rejoins_and_catches_up() {
    let mut m = Match::start(&["Alex", "Sam"], &[], OnDrop::Wait);
    m.run(&[], 300, Match::turns(300));

    // Alex's game closes without a goodbye: the host pauses for them.
    m.run(
        &[1],
        u32::MAX,
        gc_net::PLAYER_TIMEOUT + Duration::from_millis(500),
    );
    assert!(m.lans[0].paused_for_player());

    // A stranger still can't join the match.
    let net = m.net.clone();
    let err = join(&net, &mut lan(&net), "Kim", &mut m.lans[0]).unwrap_err();
    assert_eq!(
        err,
        NetError::Refused("That game has already started.".into())
    );

    // Alex starts the game again and joins: they get the match's start and
    // every turn so far, and the match carries on.
    let mut again = lan(&net);
    join(&net, &mut again, "Alex", &mut m.lans[0]).unwrap();
    match again.poll().as_slice() {
        [NetEvent::Started(s)] => assert_eq!(s.local_player, 1),
        other => panic!("{other:?}"),
    }
    again.loaded(Ok(()));
    // The old game is gone without a word.
    let old = std::mem::replace(&mut m.lans[1], again);
    net.cut(old.port().unwrap(), true);
    std::mem::forget(old);
    let (mut worlds, mut recorders) = machines(1);
    m.worlds[1] = worlds.pop().unwrap();
    m.recorders[1] = recorders.pop().unwrap();
    m.drills[1] = drill(4);
    assert!(!m.lans[0].paused_for_player());

    let target = m.ticks()[0] + 200;
    m.run(&[], target, Match::turns(500));
    assert_eq!(m.ticks(), [target; 3]);
    m.settle();
    assert!(m.same_worlds());
    let events = m.lans[2].match_events();
    assert!(
        notice(&events, "Alex rejoined and is catching up."),
        "{events:?}"
    );
    assert_eq!(m.desyncs(), []);
}

#[test]
fn six_machines_play_in_lockstep() {
    let joiners = ["Alex", "Sam", "Kai", "Jo", "Ren"];
    let net = MemoryNet::new(SEED);
    net.set_latency(Duration::from_millis(2), Duration::from_millis(20));
    net.set_loss(5);
    let mut m = Match::on(&net, &joiners, &[], OnDrop::Wait);
    assert_eq!(m.lans.len(), MAX_PLAYERS);
    const TURNS: u32 = 200;
    m.run(&[], TURNS, Match::turns(TURNS));
    assert_eq!(m.ticks(), [TURNS; MAX_PLAYERS], "match stalled");
    m.settle();
    assert!(m.same_worlds());
    m.every_army_raised();
    assert_eq!(m.desyncs(), []);
}

#[test]
fn garbage_and_forged_packets_change_nothing() {
    let mut m = Match::start(&["Alex"], &[], OnDrop::Wait);
    m.run(&[], 40, Match::turns(40));
    let at = |port| SocketAddr::from(([127, 0, 0, 1], port));
    let host = at(m.lans[0].port().unwrap());
    let alex = at(m.lans[1].port().unwrap());
    let stranger = at(9999);
    let kinds = [
        Kind::Query,
        Kind::Beacon,
        Kind::Link,
        Kind::WrongProtocol,
        Kind::Hi,
        Kind::KeyReply,
    ];
    let mut rng = 0x1234_5678_9abc_def0u64;
    for round in 0..200usize {
        let mut packet: Vec<u8> = (0..(round * 7) % 1400)
            .map(|_| {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                rng as u8
            })
            .collect();
        // Half of them pass the header check: the right magic, protocol and
        // a kind of packet, with garbage after.
        if round % 2 == 0 && packet.len() >= HEADER_LEN {
            let mut w = Writer::new();
            header(&mut w, kinds[round / 2 % kinds.len()]);
            packet[..HEADER_LEN].copy_from_slice(&w.buf);
        }
        // To each side, from a stranger and forged as from the other side.
        m.net.inject(&packet, stranger, host);
        m.net.inject(&packet, alex, host);
        m.net.inject(&packet, stranger, alex);
        m.net.inject(&packet, host, alex);
        m.run(&[], u32::MAX, STEP * 3);
    }
    let target = m.ticks()[0] + 100;
    m.run(&[], target, Match::turns(100));
    assert_eq!(m.ticks(), [target; 2]);
    m.settle();
    assert!(m.same_worlds());
    assert_eq!(m.desyncs(), []);
    assert!(!m.lans[0].paused_for_player());
}

#[test]
fn the_same_network_plays_out_the_same_way() {
    let play = || {
        let net = MemoryNet::new(99);
        net.set_latency(Duration::from_millis(5), Duration::from_millis(60));
        net.set_loss(10);
        let began = net.now();
        let mut m = Match::on(&net, &["Alex"], &[], OnDrop::Wait);
        m.run(&[], 150, Match::turns(150));
        (
            net.now() - began,
            m.lans[0].input_delay(),
            m.worlds[0].state_hash(),
        )
    };
    assert_eq!(play(), play());
}

#[test]
fn joiners_are_refused_when_full_or_on_different_data() {
    let net = MemoryNet::new(SEED);
    let mut host = lan(&net);
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    for slot in 1..MAX_PLAYERS {
        host.edit_slot(slot, SlotEdit::Kind(SlotKind::Ai));
    }
    let mut late = lan(&net);
    let err = join(&net, &mut late, "Late", &mut host).unwrap_err();
    assert_eq!(err, NetError::Refused("That lobby is full.".into()));
    assert!(late.lobby().is_none());

    host.edit_slot(MAX_PLAYERS - 1, SlotEdit::Kind(SlotKind::Open));
    let mut odd = lan_as(
        &net,
        Identity {
            data_hash: 1,
            ..id("test")
        },
    );
    let err = join(&net, &mut odd, "Odd", &mut host).unwrap_err();
    assert!(err.to_string().contains("game data differs"), "{err}");
    // Builds that differ only in commit play by the same rules, so their
    // data must match too.
    let mut retuned = lan_as(
        &net,
        Identity {
            data_hash: 1,
            ..id("test (other commit)")
        },
    );
    let err = join(&net, &mut retuned, "Retuned", &mut host).unwrap_err();
    assert!(err.to_string().contains("game data differs"), "{err}");

    // A different build is let in, and the lobby says who differs.
    let mut old = lan_as(&net, id("0.0.9"));
    join(&net, &mut old, "Old", &mut host).unwrap();
    settle(&net, &mut [&mut host, &mut old]);
    let why = host.lobby().unwrap().start_blocker().unwrap();
    assert_eq!(why, "Old is on 0.0.9, host is on test");
}

#[test]
fn leaving_and_being_removed() {
    let net = MemoryNet::new(SEED);
    let mut host = lan(&net);
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    let mut alex = lan(&net);
    join(&net, &mut alex, "Alex", &mut host).unwrap();
    settle(&net, &mut [&mut host, &mut alex]);
    assert_eq!(host.lobby().unwrap().slots[1].name, "Alex");

    // Alex leaves: the slot opens again.
    alex.leave();
    settle(&net, &mut [&mut host, &mut alex]);
    assert_eq!(host.lobby().unwrap().slots[1].kind, SlotKind::Open);

    // Back in, then the host gives the place to an AI.
    join(&net, &mut alex, "Alex", &mut host).unwrap();
    settle(&net, &mut [&mut host, &mut alex]);
    host.edit_slot(1, SlotEdit::Kind(SlotKind::Ai));
    settle(&net, &mut [&mut host, &mut alex]);
    let events = alex.poll();
    assert!(
        matches!(events.as_slice(), [NetEvent::Left(why)] if why.contains("gave your place")),
        "{events:?}"
    );
    assert!(alex.lobby().is_none());

    // The host closing the lobby sends everyone back.
    let mut sam = lan(&net);
    join(&net, &mut sam, "Sam", &mut host).unwrap();
    host.leave();
    settle(&net, &mut [&mut sam]);
    assert!(matches!(sam.poll().as_slice(), [NetEvent::Left(_)]));
}

#[test]
fn a_host_that_goes_quiet_is_given_up_on() {
    let net = MemoryNet::new(SEED);
    let mut host = lan(&net);
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    let mut alex = lan(&net);
    join(&net, &mut alex, "Alex", &mut host).unwrap();
    // The host's machine stops answering, without a goodbye.
    net.cut(host.port().unwrap(), true);
    pass(
        &net,
        &mut [&mut alex],
        gc_net::HOST_TIMEOUT - Duration::from_millis(500),
    );
    assert!(alex.lobby().is_some(), "gave up too soon");
    pass(&net, &mut [&mut alex], Duration::from_secs(1));
    assert!(matches!(alex.poll().as_slice(), [NetEvent::Left(_)]));
    assert!(alex.lobby().is_none());
}

/// Look for games on the Join screen until one hosted by `name` shows up.
fn find(net: &MemoryNet, finder: &mut Lan, name: &str, host: &mut Lan) -> DiscoveredGame {
    let give_up = net.now() + Duration::from_secs(3);
    loop {
        net.advance(STEP);
        host.pump(net.now());
        if let Some(g) = finder.discovered().iter().find(|g| g.host == name) {
            return g.clone();
        }
        assert!(net.now() < give_up, "no game found");
    }
}

#[test]
fn the_join_screen_finds_a_hosted_game() {
    let net = MemoryNet::new(SEED);
    let mut host = lan(&net);
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    let mut finder = lan(&net);
    let g = find(&net, &mut finder, "Mario", &mut host);
    assert_eq!(g.players, 1);
    assert_eq!(g.version, "test");
    join_with(&net, &mut finder, "Alex", &g.address, "", &mut host).unwrap();
    assert_eq!(finder.lobby().unwrap().slots[0].name, "Mario");
}

#[test]
fn nobody_answering_is_a_clear_error() {
    let net = MemoryNet::new(SEED);
    let mut alex = lan(&net);
    // A port nothing listens on.
    alex.start_join(&profile("Alex"), "127.0.0.1:9", "")
        .unwrap();
    let began = net.now();
    let err = loop {
        net.advance(STEP);
        if let Some(done) = alex.joining() {
            break done.unwrap_err();
        }
    };
    assert!(err.to_string().contains("No answer"), "{err}");
    assert!(net.now() - began > gc_net::JOIN_TIMEOUT);
}

#[test]
fn a_password_keeps_strangers_out() {
    let net = MemoryNet::new(SEED);
    let mut host = lan(&net);
    host.host(&profile("Mario"), LobbySettings::default(), "moles")
        .unwrap();
    let target = address(&host);
    let mut alex = lan(&net);
    let err = join_with(&net, &mut alex, "Alex", &target, "", &mut host).unwrap_err();
    assert!(err.to_string().contains("needs a password"), "{err}");
    let err = join_with(&net, &mut alex, "Alex", &target, "voles", &mut host).unwrap_err();
    assert_eq!(err, NetError::Refused("Wrong password.".into()));
    join_with(&net, &mut alex, "Alex", &target, "moles", &mut host).unwrap();
    settle(&net, &mut [&mut host, &mut alex]);
    assert_eq!(host.lobby().unwrap().slots[1].name, "Alex");

    // The Join list says a password is needed.
    let mut finder = lan(&net);
    assert!(find(&net, &mut finder, "Mario", &mut host).locked);

    // A password where none is set is caught too.
    let mut open = lan(&net);
    open.host(&profile("Sam"), LobbySettings::default(), "")
        .unwrap();
    let mut late = lan(&net);
    let target = address(&open);
    let err = join_with(&net, &mut late, "Late", &target, "moles", &mut open).unwrap_err();
    assert!(err.to_string().contains("no password"), "{err}");

    // Five wrong guesses from one address in a minute, and it gets no more
    // tries, not even with the right password...
    drop(host);
    let mut host = lan(&net);
    host.host(&profile("Mario"), LobbySettings::default(), "moles")
        .unwrap();
    let mut guesser = lan(&net);
    let target = address(&host);
    for _ in 0..5 {
        let err = join_with(&net, &mut guesser, "Guess", &target, "voles", &mut host).unwrap_err();
        assert_eq!(err, NetError::Refused("Wrong password.".into()));
    }
    let err = join_with(&net, &mut guesser, "Guess", &target, "moles", &mut host).unwrap_err();
    assert!(err.to_string().starts_with("No answer"), "{err}");
    // ...until the minute is up.
    pass(&net, &mut [&mut host], Duration::from_secs(60));
    join_with(&net, &mut guesser, "Guess", &target, "moles", &mut host).unwrap();
}

#[test]
fn friends_join_with_a_code() {
    let net = MemoryNet::new(SEED);
    let mut host = lan(&net);
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    // The lobby shows a code for this machine's network address (when it
    // has one); here, a code for this machine itself.
    if let Some(invite) = host.invite() {
        assert!(
            invite.starts_with("Join code ") || invite.starts_with("Tailscale code "),
            "{invite}"
        );
    }
    let code =
        gc_net::joincode::encode(std::net::Ipv4Addr::LOCALHOST, host.port().unwrap()).unwrap();
    let mut alex = lan(&net);
    join_with(&net, &mut alex, "Alex", &code.to_lowercase(), "", &mut host).unwrap();
    settle(&net, &mut [&mut host, &mut alex]);
    assert_eq!(host.lobby().unwrap().slots[1].name, "Alex");
}

#[test]
fn nothing_readable_crosses_the_network() {
    let net = MemoryNet::new(SEED);
    // A listener between the two sees every packet the joiner sends.
    let mut host = lan(&net);
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    let relay = net.bind(0).unwrap();
    let host_addr: SocketAddr = address(&host).parse().unwrap();
    let mut seen: Vec<Vec<u8>> = Vec::new();
    let mut alex = lan(&net);
    let target = relay.local_addr().unwrap().to_string();
    alex.start_join(&profile("Zanzibar"), &target, "").unwrap();
    let mut joiner = None;
    let mut buf = [0u8; 2048];
    let joined = loop {
        net.advance(STEP);
        host.pump(net.now());
        // Pass packets both ways, keeping a copy.
        while let Ok((n, from)) = relay.recv_from(&mut buf) {
            seen.push(buf[..n].to_vec());
            if from == host_addr {
                if let Some(j) = joiner {
                    relay.send_to(&buf[..n], j).unwrap();
                }
            } else {
                joiner = Some(from);
                relay.send_to(&buf[..n], host_addr).unwrap();
            }
        }
        if let Some(done) = alex.joining() {
            break done;
        }
    };
    joined.unwrap();
    assert!(seen.len() > 2);
    // The host only knows the relay, but the name made it through, sealed.
    let lobby = host.lobby().unwrap();
    assert_eq!(lobby.slots[1].name, "Zanzibar");
    let found = |needle: &[u8]| {
        seen.iter()
            .any(|p| p.windows(needle.len()).any(|w| w == needle))
    };
    assert!(!found(b"Zanzibar"), "a name crossed in the clear");
    assert!(!found(b"Mario"), "the lobby crossed in the clear");
}

// Desync reports reach back as far as the host compares hashes.
const _: () = assert!(gc_replay::SYSTEM_TRAIL >= gc_net::HASHES_KEPT);
