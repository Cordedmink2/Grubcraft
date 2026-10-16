//! The real thing, briefly: machines in one process on real UDP and the
//! wall clock. Everything else about the netcode is tested on a network
//! inside the process (`lan_match.rs`), where timing can't make a test
//! pass one run and fail the next.

use std::time::{Duration, Instant};

use gc_net::{HASH_EVERY, Identity, Lan};
use gc_sim::scenario::{DRILL_UNITS, Drill};
use gc_sim::{Capacities, MatchSetup, World};
use gc_ui::lobby::{Faction, LobbySettings, NetError, NetEvent, Network, Profile, SlotEdit};

fn lan() -> Lan {
    Lan::new(Identity {
        version: "test".into(),
        data_hash: 42,
        seed: 7,
        map_hash: 7,
    })
}

fn profile(name: &str) -> Profile {
    Profile {
        name: name.into(),
        color: 0,
        faction: Faction::EmberClans,
        version: "test".into(),
    }
}

/// `joiner` joins `target`, while `host` answers on this thread.
fn join(joiner: &mut Lan, name: &str, target: &str, host: &mut Lan) -> Result<(), NetError> {
    std::thread::scope(|s| {
        let joining = s.spawn(|| joiner.join(&profile(name), target, ""));
        while !joining.is_finished() {
            host.pump(Instant::now());
            std::thread::sleep(Duration::from_millis(1));
        }
        joining.join().unwrap()
    })
}

fn settle(lans: &mut [&mut Lan]) {
    for _ in 0..50 {
        for l in lans.iter_mut() {
            l.pump(Instant::now());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn two_machines_find_each_other_and_play_in_lockstep() {
    let mut host = lan();
    host.host(&profile("Mario"), LobbySettings::default(), "")
        .unwrap();

    // The Join screen hears the host's beacon.
    let mut alex = lan();
    let deadline = Instant::now() + Duration::from_secs(5);
    let address = loop {
        host.pump(Instant::now());
        if let Some(g) = alex.discovered().iter().find(|g| g.host == "Mario") {
            break g.address.clone();
        }
        assert!(Instant::now() < deadline, "no game found");
        std::thread::sleep(Duration::from_millis(5));
    };
    join(&mut alex, "Alex", &address, &mut host).unwrap();
    alex.edit_slot(1, SlotEdit::Ready(true));
    settle(&mut [&mut host, &mut alex]);
    host.start_match().unwrap();
    settle(&mut [&mut host, &mut alex]);
    assert!(matches!(alex.poll().as_slice(), [NetEvent::Started(_)]));
    host.loaded(Ok(()));
    alex.loaded(Ok(()));

    let setup = MatchSetup {
        seed: 7,
        capacities: Capacities { regiments: 256 },
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
    };
    let mut lans = [host, alex];
    let mut worlds = [World::new(&setup), World::new(&setup)];
    let mut drills = [Drill::new(7), Drill::new(7)];
    let mut cmds = Vec::new();
    // The armies, raised before the first turn.
    drills[0].commands_for(&worlds[0], &mut cmds);
    lans[0].submit(cmds.drain(..));
    const TURNS: u32 = 60;
    let deadline = Instant::now() + Duration::from_secs(30);
    while worlds.iter().any(|w| w.tick_count() < TURNS) {
        assert!(Instant::now() < deadline, "match stalled");
        for i in 0..2 {
            let now = Instant::now();
            lans[i].pump(now);
            while worlds[i].tick_count() < TURNS
                && let Some(turn) = lans[i].next_turn(now)
            {
                worlds[i].tick(&turn);
                let tick = worlds[i].tick_count();
                if tick.is_multiple_of(HASH_EVERY) {
                    lans[i].record_hash(tick, worlds[i].state_hash());
                }
                // The host raises every army and gives everyone's orders.
                drills[i].commands_for(&worlds[i], &mut cmds);
                if i == 0 {
                    lans[0].submit(cmds.drain(..));
                } else {
                    cmds.clear();
                }
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!worlds[0].regiments.is_empty());
    assert_eq!(worlds[0].state_hash(), worlds[1].state_hash());
}
