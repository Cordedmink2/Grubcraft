//! A lobby Field Battle played over the network: three machines in one
//! process on a network inside it (`gc_net::net::MemoryNet`), each making
//! the map from the lobby's settings, the host's computer players thinking
//! and sending their orders through the turns over a lossy, uneven network,
//! and every world ending identical. `gc_net`'s own tests check the lockstep on a scripted drill;
//! this one plays the real game data, the real minds and a generated map.

use std::time::Duration;

use gc_map::{FieldMap, Map};
use gc_net::net::{MemoryNet, Net};
use gc_net::{HASH_EVERY, Identity, Lan, MatchEvent};
use gc_sim::{Command, PlayerCommand};
use gc_ui::lobby::{
    Budget, LobbySettings, MapSize, MatchStart, NetEvent, Network, Profile, SlotEdit, SlotKind,
};
use gc_ui::menu::{MapPreviewKey, preview_key};

use crate::Tables;
use crate::play::Match;

const SEED: u64 = 0x6f6e_6c69;

/// Finer than any game's frame.
const STEP: Duration = Duration::from_millis(1);

/// The map `settings` pick for `players` players, as the game makes it.
fn lobby_map(settings: &LobbySettings, players: usize) -> Map {
    match preview_key(settings, players) {
        MapPreviewKey::Field {
            choice,
            tiles,
            seed,
        } => FieldMap {
            choice,
            tiles,
            seed,
        }
        .make(),
        MapPreviewKey::Conquest { .. } => unreachable!("a Field Battle"),
    }
}

fn map_hash(settings: &LobbySettings, players: usize) -> u64 {
    lobby_map(settings, players).hash()
}

fn lan(net: &MemoryNet) -> Lan {
    Lan::on(
        Identity {
            version: "test".into(),
            data_hash: 42,
            map_hash,
        },
        Box::new(net.clone()),
    )
}

fn profile(name: &str) -> Profile {
    Profile {
        name: name.into(),
        color: 0,
        faction: gc_ui::lobby::Faction::EmberClans,
        version: "test".into(),
    }
}

fn pump(net: &MemoryNet, lans: &mut [Lan], time: Duration) {
    let end = net.now() + time;
    while net.now() < end {
        net.advance(STEP);
        for l in lans.iter_mut() {
            l.pump(net.now());
        }
    }
}

/// Mario hosts, Alex and Sam join, and three computer players fill the
/// other slots, two teams of three; then the host starts. Each machine's
/// view of the started match, host first.
fn lobby(net: &MemoryNet) -> (Vec<Lan>, Vec<MatchStart>) {
    let settings = LobbySettings {
        budget: Budget::Skirmish,
        map_size: MapSize::Small,
        champions: false,
        map_seed: SEED,
        ..LobbySettings::default()
    };
    let mut lans = vec![lan(net)];
    lans[0].host(&profile("Mario"), settings, "").unwrap();
    for name in ["Alex", "Sam"] {
        let mut l = lan(net);
        let target = format!("127.0.0.1:{}", lans[0].port().unwrap());
        l.start_join(&profile(name), &target, "").unwrap();
        while l.joining().is_none() {
            net.advance(STEP);
            lans[0].pump(net.now());
        }
        lans.push(l);
    }
    pump(net, &mut lans, Duration::from_millis(300));
    // Slots 0-2 are the people, 3-5 the computer players; Mario, Sam and a
    // computer player against Alex and two.
    for slot in 3..6 {
        lans[0].edit_slot(slot, SlotEdit::Kind(SlotKind::Ai));
    }
    for (slot, team) in [(3, 1), (4, 0), (5, 1)] {
        lans[0].edit_slot(slot, SlotEdit::Team(team));
    }
    for (i, team) in [(1, 1), (2, 0)] {
        let me = lans[i].lobby().unwrap().local_slot;
        lans[i].edit_slot(me, SlotEdit::Team(team));
        lans[i].edit_slot(me, SlotEdit::Ready(true));
    }
    pump(net, &mut lans, Duration::from_millis(300));
    let mut starts = vec![lans[0].start_match().unwrap()];
    pump(net, &mut lans, Duration::from_millis(300));
    for l in &mut lans[1..] {
        match l.poll().as_slice() {
            [NetEvent::Started(s)] => starts.push(s.clone()),
            other => panic!("{other:?}"),
        }
    }
    (lans, starts)
}

#[test]
fn three_machines_play_a_lobby_battle_with_computer_players_on_a_generated_map() {
    let net = MemoryNet::new(SEED);
    let (mut lans, starts) = lobby(&net);
    let players = starts[0].players.len();
    assert_eq!(players, 6);
    let tables = Tables::new(crate::test_data());

    // Every machine makes the map from the lobby's settings, and it is the
    // one the host fingerprinted.
    let map = lobby_map(&starts[0].settings, players);
    for (l, s) in lans[1..].iter().zip(&starts[1..]) {
        let theirs = lobby_map(&s.settings, players);
        assert_eq!(theirs.hash(), map.hash());
        assert_eq!(l.host_map().unwrap().map_hash, map.hash());
    }

    // The host raises the armies and seats the computer players; the
    // joiners only play the turns.
    let seed = starts[0].seed.expect("a network match");
    let mut matches: Vec<Match> = starts
        .iter()
        .enumerate()
        .map(|(i, s)| {
            assert_eq!(s.seed, Some(seed));
            let (m, first) = Match::set_up(s, seed, &map, &tables, false, i == 0, None);
            lans[i].submit(first);
            m
        })
        .collect();
    assert_eq!(matches[0].minds.iter().count(), 3);
    assert!(matches[1..].iter().all(|m| m.minds.is_empty()));
    for (l, s) in lans.iter_mut().zip(&starts) {
        l.loaded(Ok(()));
        // Each person is ready at once, so the battle starts without
        // waiting out the deployment clock.
        l.submit([PlayerCommand {
            player: s.local_player as u8,
            command: Command::Ready,
        }]);
    }

    // Played over an internet that is slow, uneven and drops and doubles
    // packets, like Tailscale between homes on a bad day.
    net.set_latency(Duration::from_millis(20), Duration::from_millis(90));
    net.set_loss(5);
    net.set_duplicate(2);

    // Three minutes of battle at most, or until it is decided everywhere.
    let limit = 3 * 60 * gc_sim::TICKS_PER_SECOND + gc_sim::deploy::DEPLOY_SECONDS * 20;
    let mut out = Vec::new();
    // The most regiments each player had, and soldiers in all, on the host.
    let mut peak = vec![0usize; players];
    let mut most_soldiers = 0u32;
    let end = net.now() + Duration::from_secs(u64::from(limit) / 10);
    while net.now() < end
        && matches
            .iter()
            .any(|m| m.world.tick_count() < limit && m.battle.outcome.is_none())
    {
        net.advance(STEP);
        let now = net.now();
        for (i, (l, m)) in lans.iter_mut().zip(&mut matches).enumerate() {
            l.pump(now);
            while m.world.tick_count() < limit
                && let Some(turn) = l.next_turn(now)
            {
                m.play_turn(&turn);
                let tick = m.world.tick_count();
                if tick.is_multiple_of(HASH_EVERY) {
                    l.record_hash(tick, m.world.state_hash());
                }
                // The computer players think on the host only.
                if i == 0 {
                    for (p, n) in peak.iter_mut().enumerate() {
                        let here = m
                            .world
                            .regiments
                            .iter()
                            .filter(|r| usize::from(r.owner) == p);
                        *n = (*n).max(here.count());
                    }
                    let soldiers = m.world.regiments.iter().map(|r| u32::from(r.models)).sum();
                    most_soldiers = most_soldiers.max(soldiers);
                    out.clear();
                    m.think(&mut out);
                    l.submit(out.drain(..));
                }
            }
        }
    }
    // The last hashes reach the host on a network that loses nothing.
    net.set_loss(0);
    pump(&net, &mut lans, Duration::from_millis(500));

    let ticks: Vec<u32> = matches.iter().map(|m| m.world.tick_count()).collect();
    assert!(ticks.iter().all(|&t| t == ticks[0]), "{ticks:?}");
    assert!(ticks[0] > gc_sim::deploy::DEPLOY_SECONDS * 20, "{ticks:?}");
    let hash = matches[0].world.state_hash();
    assert!(matches.iter().all(|m| m.world.state_hash() == hash));
    let desyncs: Vec<MatchEvent> = lans
        .iter_mut()
        .flat_map(Lan::match_events)
        .filter(|e| matches!(e, MatchEvent::Desync { .. }))
        .collect();
    assert!(desyncs.is_empty(), "{desyncs:?}");
    // Every army took the field and the computer players fought.
    assert!(peak.iter().all(|&n| n > 0), "{peak:?}");
    let soldiers: u32 = matches[0]
        .world
        .regiments
        .iter()
        .map(|r| u32::from(r.models))
        .sum();
    assert!(
        soldiers < most_soldiers,
        "nobody fell: {soldiers} of {most_soldiers}"
    );
    let decided: Vec<bool> = matches.iter().map(|m| m.battle.outcome.is_some()).collect();
    assert!(decided.iter().all(|&d| d == decided[0]), "{decided:?}");
    println!(
        "{} ticks, decided: {:?}, host minds {:.1} ms",
        ticks[0],
        matches[0].battle.outcome,
        matches[0].minds_time.as_secs_f64() * 1000.0
    );
}
