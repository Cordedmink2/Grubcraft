//! Several machines in one process, talking over real UDP on this machine:
//! joining a lobby, starting a match, and playing it in lockstep with lost
//! packets, every world ending identical.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use gc_net::{HASH_EVERY, Identity, Lan, MatchEvent};
use gc_sim::scenario::{DRILL_UNITS, Drill};
use gc_sim::{Capacities, MatchSetup, PlayerCommand, World};
use gc_ui::lobby::{
    Faction, LobbySettings, NetError, NetEvent, Network, Profile, SlotEdit, SlotKind,
};

/// Hosts listen on a fixed few ports, so these tests take turns.
static PORTS: Mutex<()> = Mutex::new(());

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
    std::thread::scope(|s| {
        let joining = s.spawn(|| joiner.join(&profile(name), target));
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

fn world() -> World {
    World::new(&MatchSetup {
        seed: SEED,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        teams: &[],
    })
}

#[test]
fn three_machines_play_in_lockstep_through_lost_packets() {
    let _turn = PORTS.lock().unwrap_or_else(|e| e.into_inner());
    let mut host = lan();
    host.host(&profile("Mario"), LobbySettings::default())
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
    let mine: [&[u8]; 3] = [&[0, 3], &[1], &[2]];
    let mut lans = [host, alex, sam];
    let mut worlds = [world(), world(), world()];
    let mut drills: Vec<Drill> = (0..3)
        .map(|_| {
            let mut d = Drill::new(SEED);
            d.regiments_per_player = 20;
            d
        })
        .collect();
    let mut cmds: Vec<PlayerCommand> = Vec::new();
    // The armies, ordered before the first turn.
    for i in 0..3 {
        drills[i].commands_for(&worlds[i], &mut cmds);
        lans[i].submit(cmds.drain(..).filter(|c| mine[i].contains(&c.player)));
    }
    const TURNS: u32 = 300;
    let deadline = Instant::now() + Duration::from_secs(60);
    while worlds.iter().any(|w| w.tick_count() < TURNS) {
        assert!(Instant::now() < deadline, "match stalled");
        for i in 0..3 {
            let now = Instant::now();
            lans[i].pump(now);
            while worlds[i].tick_count() < TURNS
                && let Some(turn) = lans[i].next_turn(now)
            {
                worlds[i].tick(&turn);
                lans[i].record_hash(worlds[i].tick_count(), worlds[i].state_hash());
                drills[i].commands_for(&worlds[i], &mut cmds);
                lans[i].submit(cmds.drain(..).filter(|c| mine[i].contains(&c.player)));
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    // Let the last hashes reach the host.
    for l in lans.iter_mut() {
        l.simulate_loss(0);
    }
    settle(&mut lans.iter_mut().collect::<Vec<_>>());
    let hashes: Vec<u64> = worlds.iter().map(|w| w.state_hash()).collect();
    assert_eq!(hashes[0], hashes[1]);
    assert_eq!(hashes[0], hashes[2]);
    // Every player's regiments made it into the match.
    for p in 0..4u8 {
        let n = worlds[0].regiments.iter().filter(|r| r.owner == p).count();
        assert_eq!(n, 20, "player {p}");
    }
    assert!(TURNS.is_multiple_of(HASH_EVERY));
    for l in lans.iter_mut() {
        let events = l.match_events();
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, MatchEvent::Desync { .. })),
            "{events:?}"
        );
    }
}

#[test]
fn joiners_are_refused_when_full_or_on_different_data() {
    let _turn = PORTS.lock().unwrap_or_else(|e| e.into_inner());
    let mut host = lan();
    host.host(&profile("Mario"), LobbySettings::default())
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
    let _turn = PORTS.lock().unwrap_or_else(|e| e.into_inner());
    let mut host = lan();
    host.host(&profile("Mario"), LobbySettings::default())
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
    let _turn = PORTS.lock().unwrap_or_else(|e| e.into_inner());
    let mut host = lan();
    host.host(&profile("Mario"), LobbySettings::default())
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
    let _turn = PORTS.lock().unwrap_or_else(|e| e.into_inner());
    let mut alex = lan();
    // A port nothing listens on.
    let err = alex.join(&profile("Alex"), "127.0.0.1:9").unwrap_err();
    assert!(err.to_string().contains("No answer"), "{err}");
}
