//! Several machines in one process, talking over real UDP on this machine:
//! joining a lobby, starting a match, and playing it in lockstep with lost
//! packets, every world ending identical.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use gc_net::{HASH_EVERY, INPUT_DELAY, Identity, Lan, MIN_DELAY, MatchEvent};
use gc_replay::{Recorder, Replay};
use gc_sim::scenario::{DRILL_UNITS, Drill};
use gc_sim::{Capacities, MatchSetup, PlayerCommand, World};
use gc_ui::lobby::{
    Faction, LobbySettings, NetError, NetEvent, Network, OnDrop, Profile, SlotEdit, SlotKind,
};

/// Hosts listen on a fixed few ports, so these tests take turns.
static PORTS: Mutex<()> = Mutex::new(());

/// Take the game ports for one test (a test that failed while holding them
/// still lets the next one in).
fn ports() -> MutexGuard<'static, ()> {
    PORTS.lock().unwrap_or_else(PoisonError::into_inner)
}

const SEED: u64 = 0x6772_7562;

fn id(version: &str) -> Identity {
    Identity {
        version: version.into(),
        data_hash: 42,
        seed: SEED,
        map_hash: 7,
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

fn lan() -> Lan {
    let mut l = Lan::new(id("test"));
    l.set_turn_length(Duration::from_millis(5));
    l
}

fn gc_net_player_timeout() -> Duration {
    gc_net::PLAYER_TIMEOUT
}

fn address(host: &Lan) -> String {
    format!("127.0.0.1:{}", host.port().unwrap())
}

/// `joiner` joins `host`, which answers meanwhile (on another machine it
/// would be running on its own).
fn join(joiner: &mut Lan, name: &str, host: &mut Lan) -> Result<(), NetError> {
    let target = address(host);
    join_at(joiner, name, &target, host)
}

fn join_at(joiner: &mut Lan, name: &str, target: &str, host: &mut Lan) -> Result<(), NetError> {
    join_with(joiner, name, target, "", host)
}

fn join_with(
    joiner: &mut Lan,
    name: &str,
    target: &str,
    password: &str,
    host: &mut Lan,
) -> Result<(), NetError> {
    std::thread::scope(|s| {
        let joining = s.spawn(|| joiner.join(&profile(name), target, password));
        while !joining.is_finished() {
            host.pump(Instant::now());
            std::thread::sleep(Duration::from_millis(1));
        }
        joining.join().unwrap()
    })
}

/// Pump every machine for a moment.
fn settle(lans: &mut [&mut Lan]) {
    for _ in 0..20 {
        for l in lans.iter_mut() {
            l.pump(Instant::now());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
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
};

fn world() -> World {
    World::new(&SETUP)
}

/// `n` machines' worlds, each recorded the way the game records a match.
fn machines(n: usize) -> (Vec<World>, Vec<Recorder>) {
    let worlds: Vec<World> = (0..n).map(|_| world()).collect();
    let recorders = worlds
        .iter()
        .map(|w| Recorder::new(&SETUP, w, "LAN test").checkpoint_every(HASH_EVERY))
        .collect();
    (worlds, recorders)
}

/// A match under way between `lans`, each playing the drill for its own
/// players (`mine[i]`) on its own copy of the world.
struct Match {
    lans: Vec<Lan>,
    worlds: Vec<World>,
    /// What each machine played, as it would save it for a desync report.
    recorders: Vec<Recorder>,
    drills: Vec<Drill>,
    mine: Vec<Vec<u8>>,
    cmds: Vec<PlayerCommand>,
}

impl Match {
    /// Host a lobby, let `names` join and get ready, fill `ai` slots with
    /// computer players, and start, with turns `turn_ms` long.
    fn start(names: &[&str], ai: &[usize], on_drop: OnDrop, turn_ms: u64) -> Match {
        let lan = || {
            let mut l = Lan::new(id("test"));
            l.set_turn_length(Duration::from_millis(turn_ms));
            l
        };
        let mut host = lan();
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
            let mut l = lan();
            join(&mut l, name, &mut lans[0]).unwrap();
            lans.push(l);
        }
        for &slot in ai {
            lans[0].edit_slot(slot, SlotEdit::Kind(SlotKind::Ai));
        }
        for l in &mut lans[1..] {
            let me = l.lobby().unwrap().local_slot;
            l.edit_slot(me, SlotEdit::Ready(true));
        }
        settle(&mut lans.iter_mut().collect::<Vec<_>>());
        let start = lans[0].start_match().unwrap();
        assert_eq!(start.seed, Some(SEED));
        settle(&mut lans.iter_mut().collect::<Vec<_>>());
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
        let n = lans.len();
        let (worlds, recorders) = machines(n);
        let mut m = Match {
            lans,
            worlds,
            recorders,
            drills: (0..n)
                .map(|_| {
                    let mut d = Drill::new(SEED);
                    d.regiments_per_player = 20;
                    d
                })
                .collect(),
            mine,
            cmds: Vec::new(),
        };
        // The armies, ordered before the first turn.
        for i in 0..n {
            m.order(i);
        }
        m
    }

    fn order(&mut self, i: usize) {
        self.drills[i].commands_for(&self.worlds[i], &mut self.cmds);
        let mine = &self.mine[i];
        self.lans[i].submit(self.cmds.drain(..).filter(|c| mine.contains(&c.player)));
    }

    /// Run every machine except `away` (cut off from the network) for a
    /// while, or until every running world reaches `until` ticks.
    fn run(&mut self, away: &[usize], until: u32, for_at_most: Duration) {
        let deadline = Instant::now() + for_at_most;
        let running: Vec<usize> = (0..self.lans.len()).filter(|i| !away.contains(i)).collect();
        while Instant::now() < deadline
            && running.iter().any(|&i| self.worlds[i].tick_count() < until)
        {
            for &i in &running {
                let now = Instant::now();
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
                    self.order(i);
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn ticks(&self) -> Vec<u32> {
        self.worlds.iter().map(gc_sim::World::tick_count).collect()
    }

    /// Let the last hashes reach the host.
    fn settle(&mut self) {
        for l in &mut self.lans {
            l.simulate_loss(0);
        }
        settle(&mut self.lans.iter_mut().collect::<Vec<_>>());
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

#[test]
fn three_machines_play_in_lockstep_through_lost_packets() {
    let _turn = ports();
    let mut host = lan();
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    let mut alex = lan();
    let mut sam = lan();
    join(&mut alex, "Alex", &mut host).unwrap();
    join(&mut sam, "Sam", &mut host).unwrap();
    settle(&mut [&mut host, &mut alex, &mut sam]);

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
    settle(&mut [&mut host, &mut alex, &mut sam]);
    let lobby = sam.lobby().unwrap();
    assert_eq!(lobby.slots[1].team, 0);
    assert_eq!(lobby.slots[2].team, 2);
    assert_eq!(lobby.slots[3].kind, SlotKind::Ai);

    let start = host.start_match().unwrap();
    assert_eq!(start.players.len(), 4);
    assert_eq!(start.seed, Some(SEED));
    settle(&mut [&mut host, &mut alex, &mut sam]);
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

    for l in [&mut host, &mut alex, &mut sam] {
        l.simulate_loss(15);
        l.loaded(Ok(()));
    }

    // Host plays players 0 and 3 (the AI), Alex 1 and Sam 2.
    let (worlds, recorders) = machines(3);
    let mut m = Match {
        lans: vec![host, alex, sam],
        worlds,
        recorders,
        drills: (0..3)
            .map(|_| {
                let mut d = Drill::new(SEED);
                d.regiments_per_player = 20;
                d
            })
            .collect(),
        mine: vec![vec![0, 3], vec![1], vec![2]],
        cmds: Vec::new(),
    };
    for i in 0..3 {
        m.order(i);
    }
    const TURNS: u32 = 300;
    m.run(&[], TURNS, Duration::from_secs(60));
    assert_eq!(m.ticks(), [TURNS; 3], "match stalled");
    m.settle();
    let hashes: Vec<u64> = m.worlds.iter().map(gc_sim::World::state_hash).collect();
    assert_eq!(hashes[0], hashes[1]);
    assert_eq!(hashes[0], hashes[2]);
    // Every player's regiments made it into the match.
    for p in 0..4u8 {
        let n = m.worlds[0]
            .regiments
            .iter()
            .filter(|r| r.owner == p)
            .count();
        assert_eq!(n, 20, "player {p}");
    }
    assert!(TURNS.is_multiple_of(HASH_EVERY));
    assert_eq!(m.desyncs(), []);
    // 5 ms turns are shorter than the 10 ms margin: the delay rose from
    // where it started, and everyone heard.
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
    assert_eq!(back.state_hash(), hashes[2]);
    assert_eq!(m.recorders[0].replay().first_difference(&replay), None);
}

#[test]
fn the_delay_drops_on_a_fast_network() {
    let _turn = ports();
    // With 30 ms turns, a round trip on this machine plus the margin fits
    // in one turn.
    let mut m = Match::start(&["Alex"], &[], OnDrop::Wait, 30);
    assert_eq!(m.lans[0].input_delay(), Some(INPUT_DELAY));
    m.run(&[], 100, Duration::from_secs(20));
    assert_eq!(m.ticks(), [100, 100]);
    assert_eq!(m.lans[0].input_delay(), Some(MIN_DELAY));
    m.settle();
    assert_eq!(m.lans[1].input_delay(), Some(MIN_DELAY));
    assert_eq!(m.worlds[0].state_hash(), m.worlds[1].state_hash());
    assert_eq!(m.desyncs(), []);
}

#[test]
fn the_match_waits_for_a_player_who_drops() {
    let _turn = ports();
    let mut m = Match::start(&["Alex", "Sam"], &[], OnDrop::Wait, 5);
    m.run(&[], 100, Duration::from_secs(20));
    assert_eq!(m.ticks(), [100; 3]);

    // Alex's Wi-Fi goes: after a few quiet seconds the host pauses.
    m.run(
        &[1],
        u32::MAX,
        gc_net_player_timeout() + Duration::from_millis(500),
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
    m.run(&[], target, Duration::from_secs(20));
    assert_eq!(m.ticks(), [target; 3]);
    assert!(!m.lans[0].paused_for_player());
    assert_eq!(m.lans[2].waiting(), None);

    // Sam goes for good, and the host carries on without them.
    m.run(
        &[2],
        u32::MAX,
        gc_net_player_timeout() + Duration::from_millis(500),
    );
    assert!(m.lans[0].paused_for_player());
    m.lans[0].carry_on();
    let target = m.ticks()[0] + 50;
    m.run(&[2], target, Duration::from_secs(20));
    assert_eq!(&m.ticks()[..2], [target; 2]);
    m.settle();
    assert_eq!(m.worlds[0].state_hash(), m.worlds[1].state_hash());
    let notices: Vec<MatchEvent> = m.lans[1].match_events();
    assert!(
        notices
            .iter()
            .any(|e| matches!(e, MatchEvent::Notice(t) if t == "Carrying on without Sam.")),
        "{notices:?}"
    );
    assert!(
        !notices
            .iter()
            .any(|e| matches!(e, MatchEvent::Desync { .. }))
    );
    assert_eq!(m.desyncs(), []);
}

#[test]
fn carry_on_plays_through_a_drop() {
    let _turn = ports();
    let mut m = Match::start(&["Alex"], &[2], OnDrop::CarryOn, 5);
    m.run(&[], 50, Duration::from_secs(20));
    let before = m.ticks()[0];
    m.run(
        &[1],
        u32::MAX,
        gc_net_player_timeout() + Duration::from_millis(500),
    );
    assert!(!m.lans[0].paused_for_player());
    assert!(m.ticks()[0] > before + 100, "{:?}", m.ticks());
    let events = m.lans[0].match_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, MatchEvent::Notice(t) if t == "Alex lost connection.")),
        "{events:?}"
    );
}

#[test]
fn a_desync_is_found_and_reported_everywhere() {
    let _turn = ports();
    let mut m = Match::start(&["Alex"], &[], OnDrop::Wait, 5);
    m.run(&[], 40, Duration::from_secs(20));
    // Something only Alex's machine does: an extra tick of its own.
    m.recorders[1].tick(&mut m.worlds[1], &[]);
    m.run(&[], 100, Duration::from_secs(20));
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
    let _turn = ports();
    let mut m = Match::start(&["Alex", "Sam"], &[], OnDrop::Wait, 5);
    m.run(&[], 300, Duration::from_secs(20));

    // Alex's game closes without a goodbye: the host pauses for them.
    m.run(
        &[1],
        u32::MAX,
        gc_net_player_timeout() + Duration::from_millis(500),
    );
    assert!(m.lans[0].paused_for_player());

    // A stranger still can't join the match.
    let err = join(&mut lan(), "Kim", &mut m.lans[0]).unwrap_err();
    assert_eq!(
        err,
        NetError::Refused("That game has already started.".into())
    );

    // Alex starts the game again and joins: they get the match's start and
    // every turn so far, and the match carries on.
    let mut again = lan();
    join(&mut again, "Alex", &mut m.lans[0]).unwrap();
    match again.poll().as_slice() {
        [NetEvent::Started(s)] => assert_eq!(s.local_player, 1),
        other => panic!("{other:?}"),
    }
    again.loaded(Ok(()));
    std::mem::forget(std::mem::replace(&mut m.lans[1], again));
    let (mut worlds, mut recorders) = machines(1);
    m.worlds[1] = worlds.pop().unwrap();
    m.recorders[1] = recorders.pop().unwrap();
    m.drills[1] = Drill::new(SEED);
    m.drills[1].regiments_per_player = 20;
    assert!(!m.lans[0].paused_for_player());

    let target = m.ticks()[0] + 200;
    m.run(&[], target, Duration::from_secs(30));
    assert_eq!(m.ticks(), [target; 3]);
    m.settle();
    let hash = m.worlds[0].state_hash();
    assert!(m.worlds.iter().all(|w| w.state_hash() == hash));
    let events = m.lans[2].match_events();
    assert!(
        events.iter().any(
            |e| matches!(e, MatchEvent::Notice(t) if t == "Alex rejoined and is catching up.")
        ),
        "{events:?}"
    );
    assert_eq!(m.desyncs(), []);
}

#[test]
fn joiners_are_refused_when_full_or_on_different_data() {
    let _turn = ports();
    let mut host = lan();
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    for slot in 1..4 {
        host.edit_slot(slot, SlotEdit::Kind(SlotKind::Ai));
    }
    let mut late = lan();
    let err = join(&mut late, "Late", &mut host).unwrap_err();
    assert_eq!(err, NetError::Refused("That lobby is full.".into()));
    assert!(late.lobby().is_none());

    host.edit_slot(3, SlotEdit::Kind(SlotKind::Open));
    let mut odd = Lan::new(Identity {
        data_hash: 1,
        ..id("test")
    });
    let err = join(&mut odd, "Odd", &mut host).unwrap_err();
    assert!(err.to_string().contains("game data differs"), "{err}");

    // A different build is let in, and the lobby says who differs.
    let mut old = Lan::new(id("0.0.9"));
    join(&mut old, "Old", &mut host).unwrap();
    settle(&mut [&mut host, &mut old]);
    let why = host.lobby().unwrap().start_blocker().unwrap();
    assert_eq!(why, "Old is on 0.0.9, host is on test");
}

#[test]
fn leaving_and_being_removed() {
    let _turn = ports();
    let mut host = lan();
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    let mut alex = lan();
    join(&mut alex, "Alex", &mut host).unwrap();
    settle(&mut [&mut host, &mut alex]);
    assert_eq!(host.lobby().unwrap().slots[1].name, "Alex");

    // Alex leaves: the slot opens again.
    alex.leave();
    settle(&mut [&mut host, &mut alex]);
    assert_eq!(host.lobby().unwrap().slots[1].kind, SlotKind::Open);

    // Back in, then the host gives the place to an AI.
    join(&mut alex, "Alex", &mut host).unwrap();
    settle(&mut [&mut host, &mut alex]);
    host.edit_slot(1, SlotEdit::Kind(SlotKind::Ai));
    settle(&mut [&mut host, &mut alex]);
    let events = alex.poll();
    assert!(
        matches!(events.as_slice(), [NetEvent::Left(why)] if why.contains("gave your place")),
        "{events:?}"
    );
    assert!(alex.lobby().is_none());

    // The host closing the lobby sends everyone back.
    let mut sam = lan();
    join(&mut sam, "Sam", &mut host).unwrap();
    host.leave();
    settle(&mut [&mut sam]);
    assert!(matches!(sam.poll().as_slice(), [NetEvent::Left(_)]));
}

#[test]
fn the_join_screen_finds_a_hosted_game() {
    let _turn = ports();
    let mut host = lan();
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    let mut finder = lan();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        host.pump(Instant::now());
        let games = finder.discovered();
        if let Some(g) = games.iter().find(|g| g.host == "Mario") {
            assert_eq!(g.players, 1);
            assert_eq!(g.version, "test");
            let addr = g.address.clone();
            join_at(&mut finder, "Alex", &addr, &mut host).unwrap();
            break;
        }
        assert!(Instant::now() < deadline, "no game found");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(finder.lobby().unwrap().slots[0].name, "Mario");
}

#[test]
fn nobody_answering_is_a_clear_error() {
    let _turn = ports();
    let mut alex = lan();
    // A port nothing listens on.
    let err = alex.join(&profile("Alex"), "127.0.0.1:9", "").unwrap_err();
    assert!(err.to_string().contains("No answer"), "{err}");
}

#[test]
fn a_password_keeps_strangers_out() {
    let _turn = ports();
    let mut host = lan();
    host.host(&profile("Mario"), LobbySettings::default(), "moles")
        .unwrap();
    let target = address(&host);
    let mut alex = lan();
    let err = join_with(&mut alex, "Alex", &target, "", &mut host).unwrap_err();
    assert!(err.to_string().contains("needs a password"), "{err}");
    let err = join_with(&mut alex, "Alex", &target, "voles", &mut host).unwrap_err();
    assert_eq!(err, NetError::Refused("Wrong password.".into()));
    join_with(&mut alex, "Alex", &target, "moles", &mut host).unwrap();
    settle(&mut [&mut host, &mut alex]);
    assert_eq!(host.lobby().unwrap().slots[1].name, "Alex");

    // The Join list says a password is needed.
    let mut finder = lan();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        host.pump(Instant::now());
        if let Some(g) = finder.discovered().iter().find(|g| g.host == "Mario") {
            assert!(g.locked);
            break;
        }
        assert!(Instant::now() < deadline, "no game found");
        std::thread::sleep(Duration::from_millis(5));
    }

    // A password where none is set is caught too.
    let mut open = lan();
    open.host(&profile("Sam"), LobbySettings::default(), "")
        .unwrap();
    let mut late = lan();
    let target = address(&open);
    let err = join_with(&mut late, "Late", &target, "moles", &mut open).unwrap_err();
    assert!(err.to_string().contains("no password"), "{err}");
}

#[test]
fn friends_join_with_a_code() {
    let _turn = ports();
    let mut host = lan();
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    // The lobby shows a code for this machine's network address (when it
    // has one); here, a code for this machine itself.
    if let Some(invite) = host.invite() {
        assert!(invite.starts_with("Join code "), "{invite}");
    }
    let code =
        gc_net::joincode::encode(std::net::Ipv4Addr::LOCALHOST, host.port().unwrap()).unwrap();
    let mut alex = lan();
    join_at(&mut alex, "Alex", &code.to_lowercase(), &mut host).unwrap();
    settle(&mut [&mut host, &mut alex]);
    assert_eq!(host.lobby().unwrap().slots[1].name, "Alex");
}

#[test]
fn nothing_readable_crosses_the_network() {
    let _turn = ports();
    // A listener between the two sees every packet the joiner sends.
    let mut host = lan();
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();
    let relay = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    relay.set_nonblocking(true).unwrap();
    let host_addr: std::net::SocketAddr = address(&host).parse().unwrap();
    let mut seen: Vec<Vec<u8>> = Vec::new();
    let mut alex = lan();
    let target = relay.local_addr().unwrap().to_string();
    std::thread::scope(|s| {
        let joining = s.spawn(|| alex.join(&profile("Zanzibar"), &target, ""));
        let mut joiner = None;
        let mut buf = [0u8; 2048];
        while !joining.is_finished() {
            host.pump(Instant::now());
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
            std::thread::sleep(Duration::from_millis(1));
        }
        joining.join().unwrap()
    })
    .unwrap();
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
