//! [`Lan`]: hosting and joining over the local network, the lobby protocol,
//! and the host-sequenced lockstep turns of a match.

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use gc_sim::{PlayerCommand, PlayerId};
use gc_ui::lobby::{
    DEFAULT_AI_LEVEL, DiscoveredGame, LobbySettings, LobbyState, MAPS, MAX_PLAYERS, MatchStart,
    NetError, NetEvent, Network, Profile, Slot, SlotEdit, SlotKind, apply_edit,
};

use crate::link::Link;
use crate::proto::{
    Beacon, Kind, LobbyWire, MAX_PACKET, Msg, PROTOCOL, header, read_header, short_name,
};
use crate::wire::{Reader, Writer, command_size};

/// The port a host listens on. A second game hosted on the same machine
/// takes the next one, up to [`PORTS`] of them.
pub const DEFAULT_PORT: u16 = 47_820;
pub const PORTS: u16 = 4;

/// One lockstep turn: one simulation tick.
pub const TURN: Duration = Duration::from_millis(1000 / gc_sim::TICKS_PER_SECOND as u64);

/// Turns between a command reaching the host and being simulated
/// everywhere (docs/11-networking.md, "Input delay"): 100 ms, right for
/// home Wi-Fi. Adapting it to measured conditions comes next.
pub const INPUT_DELAY: u32 = 2;

/// Players send the host a state hash every this many ticks.
pub const HASH_EVERY: u32 = 10;

/// A player silent this long has lost the connection.
pub const PLAYER_TIMEOUT: Duration = Duration::from_secs(3);
/// A host silent this long has gone.
pub const HOST_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the host waits for everyone to build their world.
pub const LOAD_TIMEOUT: Duration = Duration::from_secs(60);
/// How long Join waits for the host to answer.
pub const JOIN_TIMEOUT: Duration = Duration::from_secs(3);

const LOBBY_KEEPALIVE: Duration = Duration::from_millis(200);
const MATCH_KEEPALIVE: Duration = TURN;
const QUERY_EVERY: Duration = Duration::from_secs(1);
const DISCOVERED_FOR: Duration = Duration::from_secs(3);
/// Commands per turn bundle are capped so a bundle fits a packet; the rest
/// go in the next turn.
const MAX_BUNDLE_BYTES: usize = 900;
/// If the host falls this many turns behind its clock (a long hitch), it
/// stops trying to catch up and carries on from now.
const MAX_HOST_LAG: u32 = 20;

/// What this machine runs, for the version gate and the map check.
#[derive(Clone, Debug)]
pub struct Identity {
    /// Shown in the lobby; must match the host's.
    pub version: String,
    /// Hash of the game data; a joiner whose data differs is refused.
    pub data_hash: u64,
    /// The seed of this machine's match world and map.
    pub seed: u64,
    /// Hash of this machine's map (its walkable ground), so a joiner whose
    /// map came out differently finds out before the match, not as a desync.
    pub map_hash: u64,
}

/// Things that happen during a match, for the game to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchEvent {
    /// A line to show the players ("Alex lost connection").
    Notice(String),
    /// A player's world no longer matches the host's.
    Desync { tick: u32 },
    /// The match is over for this machine (host gone, kicked).
    Ended(String),
}

/// The host's map, as a joiner must reproduce it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostMap {
    pub seed: u64,
    pub map_hash: u64,
}

struct Peer {
    addr: SocketAddr,
    link: Link,
    /// The lobby slot this player holds; `None` while being turned away.
    slot: Option<usize>,
    /// Their player number in the match, once it starts.
    player: Option<PlayerId>,
    name: String,
    loaded: bool,
    /// Lost or left; nothing more is sent or taken.
    gone: bool,
    /// Forget this peer at this time (after a refusal or a goodbye has had
    /// a chance to arrive).
    drop_at: Option<Instant>,
}

struct HostGame {
    /// When turn 0 began; `None` until everyone has loaded.
    go_at: Option<Instant>,
    started_at: Instant,
    host_loaded: bool,
    /// Turns handed to this machine's simulation.
    handed: u32,
    /// Bundles closed but not yet simulated here; the front is `handed`.
    closed: VecDeque<Vec<PlayerCommand>>,
    /// Commands waiting for a turn, in arrival order.
    inbox: VecDeque<PlayerCommand>,
    /// This machine's state hashes, oldest first.
    hashes: VecDeque<(u32, u64)>,
    /// Players' hashes not yet compared: (player, tick, hash).
    theirs: Vec<(PlayerId, u32, u64)>,
    desynced: Vec<PlayerId>,
}

struct Host {
    lobby: LobbyState,
    peers: Vec<Peer>,
    sent: Option<LobbyWire>,
    game: Option<HostGame>,
}

struct ClientGame {
    map: HostMap,
    me: PlayerId,
    delay: u32,
    /// Turn bundles received and not yet simulated, in turn order.
    ready: VecDeque<Vec<PlayerCommand>>,
    next_turn: u32,
    /// When the next turn is due on this machine's clock.
    due: Option<Instant>,
    outbox: Vec<PlayerCommand>,
}

struct Client {
    host: SocketAddr,
    link: Link,
    slot: Option<usize>,
    lobby: Option<LobbyState>,
    game: Option<ClientGame>,
}

enum Role {
    Idle,
    Host(Box<Host>),
    Client(Box<Client>),
}

/// LAN play for the menus ([`Network`]) and for the match (turns in,
/// orders out).
pub struct Lan {
    id: Identity,
    socket: Option<UdpSocket>,
    role: Role,
    /// The Join screen's socket and what it has heard.
    finder: Option<UdpSocket>,
    last_query: Option<Instant>,
    found: Vec<(DiscoveredGame, Instant)>,
    games: Vec<DiscoveredGame>,
    events: Vec<NetEvent>,
    match_events: Vec<MatchEvent>,
    /// Length of a turn: [`TURN`], shorter in tests.
    turn: Duration,
    /// Test hook: drop this percentage of outgoing packets.
    loss: u32,
    loss_rng: u32,
}

impl Lan {
    pub fn new(id: Identity) -> Lan {
        Lan {
            id,
            socket: None,
            role: Role::Idle,
            finder: None,
            last_query: None,
            found: Vec::new(),
            games: Vec::new(),
            events: Vec::new(),
            match_events: Vec::new(),
            turn: TURN,
            loss: 0,
            loss_rng: 0x9e37_79b9,
        }
    }

    /// For tests: lose `percent` of the packets this machine sends.
    pub fn simulate_loss(&mut self, percent: u32) {
        self.loss = percent.min(100);
    }

    /// For tests: run turns this long instead of [`TURN`]. Every machine in
    /// a match must use the same length.
    pub fn set_turn_length(&mut self, turn: Duration) {
        self.turn = turn;
    }

    /// The port this machine's game socket is on, if any.
    pub fn port(&self) -> Option<u16> {
        self.socket
            .as_ref()
            .and_then(|s| s.local_addr().ok())
            .map(|a| a.port())
    }

    /// Whether a networked match is under way.
    pub fn in_match(&self) -> bool {
        match &self.role {
            Role::Host(h) => h.game.is_some(),
            Role::Client(c) => c.game.is_some(),
            Role::Idle => false,
        }
    }

    pub fn is_host(&self) -> bool {
        matches!(self.role, Role::Host(_))
    }

    /// A joiner's view of the host's map, to check against its own before
    /// saying it has loaded.
    pub fn host_map(&self) -> Option<HostMap> {
        match &self.role {
            Role::Client(c) => c.game.as_ref().map(|g| g.map),
            _ => None,
        }
    }

    /// Tell the host this machine's world is built (or why it couldn't be).
    pub fn loaded(&mut self, result: Result<(), String>) {
        match &mut self.role {
            Role::Host(h) => {
                if let Some(g) = &mut h.game {
                    g.host_loaded = true;
                }
            }
            Role::Client(c) => {
                c.link.send(&Msg::Loaded {
                    error: result.err(),
                });
            }
            Role::Idle => {}
        }
        self.flush(Instant::now(), true);
    }

    /// While the match waits for players to load, what it waits for.
    pub fn waiting(&self) -> Option<String> {
        match &self.role {
            Role::Host(h) => {
                let g = h.game.as_ref()?;
                if g.go_at.is_some() {
                    return None;
                }
                let names: Vec<&str> = h
                    .peers
                    .iter()
                    .filter(|p| p.player.is_some() && !p.loaded && !p.gone)
                    .map(|p| p.name.as_str())
                    .collect();
                Some(if names.is_empty() {
                    "Starting...".into()
                } else {
                    format!("Waiting for {} to load...", names.join(", "))
                })
            }
            Role::Client(c) => {
                let g = c.game.as_ref()?;
                (g.next_turn == 0 && g.ready.is_empty())
                    .then(|| "Waiting for the other players...".to_string())
            }
            Role::Idle => None,
        }
    }

    /// One line on the connection, for the overlay.
    pub fn status(&self) -> Option<String> {
        let ms = |l: &Link| {
            l.rtt()
                .map_or("?".to_string(), |d| d.as_millis().to_string())
        };
        match &self.role {
            Role::Host(h) => {
                let peers: Vec<String> = h
                    .peers
                    .iter()
                    .filter(|p| p.slot.is_some() && !p.gone)
                    .map(|p| format!("{} {} ms", p.name, ms(&p.link)))
                    .collect();
                Some(format!(
                    "hosting on port {}, delay {} ms{}{}",
                    self.port().unwrap_or(0),
                    INPUT_DELAY * TURN.as_millis() as u32,
                    if peers.is_empty() { "" } else { ": " },
                    peers.join(", ")
                ))
            }
            Role::Client(c) => Some(format!(
                "joined {}, round trip {} ms{}",
                c.host,
                ms(&c.link),
                c.game.as_ref().map_or(String::new(), |g| format!(
                    ", {} turns ahead",
                    g.ready.len()
                ))
            )),
            Role::Idle => None,
        }
    }

    /// Orders from this machine (its player, and the host's AI players) for
    /// a coming turn. The host keeps the player numbers given; a joiner's
    /// orders are always its own player's.
    pub fn submit(&mut self, commands: impl IntoIterator<Item = PlayerCommand>) {
        match &mut self.role {
            Role::Host(h) => {
                if let Some(g) = &mut h.game {
                    g.inbox.extend(commands);
                }
            }
            Role::Client(c) => {
                if let Some(g) = &mut c.game {
                    let me = g.me;
                    g.outbox.extend(
                        commands
                            .into_iter()
                            .map(|c| PlayerCommand { player: me, ..c }),
                    );
                }
            }
            Role::Idle => {}
        }
    }

    /// The next turn to simulate, when one is due: its commands, in order.
    /// Call until it returns `None` each frame (it paces itself to 20 turns a
    /// second, catching up when behind).
    pub fn next_turn(&mut self, now: Instant) -> Option<Vec<PlayerCommand>> {
        let length = self.turn;
        let turn = match &mut self.role {
            Role::Host(h) => host_turn(h, now, length),
            Role::Client(c) => {
                let g = c.game.as_mut()?;
                client_turn(g, now, length)
            }
            Role::Idle => None,
        };
        if turn.is_some() {
            self.flush(now, false);
        }
        turn
    }

    /// This machine's state hash after `tick` ticks. Call after every tick;
    /// only every [`HASH_EVERY`]th is used.
    pub fn record_hash(&mut self, tick: u32, hash: u64) {
        if !tick.is_multiple_of(HASH_EVERY) {
            return;
        }
        match &mut self.role {
            Role::Host(h) => {
                if let Some(g) = &mut h.game {
                    g.hashes.push_back((tick, hash));
                    if g.hashes.len() > 256 {
                        g.hashes.pop_front();
                    }
                }
                self.compare_hashes();
            }
            Role::Client(c) => c.link.send(&Msg::Hash { tick, hash }),
            Role::Idle => {}
        }
    }

    /// Things that happened in the match since the last call.
    pub fn match_events(&mut self) -> Vec<MatchEvent> {
        std::mem::take(&mut self.match_events)
    }

    /// Receive everything waiting, deal with silence, and send what is due.
    /// Call once a frame.
    pub fn pump(&mut self, now: Instant) {
        let mut buf = [0u8; 2048];
        while let Some(socket) = &self.socket {
            match socket.recv_from(&mut buf) {
                Ok((n, from)) => {
                    let packet = buf[..n].to_vec();
                    self.on_packet(&packet, from, now);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                // ICMP "port unreachable" from a peer that left shows up as
                // an error on some systems; silence handles it.
                Err(_) => continue,
            }
        }
        self.check_silence(now);
        self.flush(now, false);
        self.find(now);
    }

    fn send_raw(&mut self, bytes: &[u8], to: SocketAddr) {
        if self.loss > 0 {
            self.loss_rng ^= self.loss_rng << 13;
            self.loss_rng ^= self.loss_rng >> 17;
            self.loss_rng ^= self.loss_rng << 5;
            if self.loss_rng % 100 < self.loss {
                return;
            }
        }
        if let Some(s) = &self.socket {
            let _ = s.send_to(bytes, to);
        }
    }

    /// Send a packet on every link with something new, or quiet too long.
    /// `all` sends on every link at once.
    fn flush(&mut self, now: Instant, all: bool) {
        let mut out: Vec<(Vec<u8>, SocketAddr)> = Vec::new();
        match &mut self.role {
            Role::Host(h) => {
                let keepalive = if h.game.is_some() {
                    MATCH_KEEPALIVE
                } else {
                    LOBBY_KEEPALIVE
                };
                for p in h.peers.iter_mut().filter(|p| !p.gone) {
                    if all || p.link.fresh || now - p.link.last_sent >= keepalive {
                        out.push((p.link.packet(now), p.addr));
                    }
                }
            }
            Role::Client(c) => {
                if let Some(g) = &mut c.game {
                    for msg in orders_messages(&mut g.outbox) {
                        c.link.send(&msg);
                    }
                }
                let keepalive = if c.game.is_some() {
                    MATCH_KEEPALIVE
                } else {
                    LOBBY_KEEPALIVE
                };
                if all || c.link.fresh || now - c.link.last_sent >= keepalive {
                    out.push((c.link.packet(now), c.host));
                }
            }
            Role::Idle => {}
        }
        for (bytes, to) in out {
            self.send_raw(&bytes, to);
        }
    }

    fn on_packet(&mut self, packet: &[u8], from: SocketAddr, now: Instant) {
        let mut r = Reader::new(packet);
        let Some((protocol, kind)) = read_header(&mut r) else {
            return;
        };
        if protocol != PROTOCOL {
            // Tell a joiner on another protocol which build we run, in the
            // one packet every version understands.
            if self.is_host() && kind == Kind::Link as u8 {
                let mut w = Writer::new();
                header(&mut w, Kind::WrongProtocol);
                w.str(&self.id.version);
                self.send_raw(&w.buf, from);
            }
            return;
        }
        match kind {
            k if k == Kind::Query as u8 => {
                if let Some(beacon) = self.beacon() {
                    let mut w = Writer::new();
                    header(&mut w, Kind::Beacon);
                    beacon.write(&mut w);
                    self.send_raw(&w.buf, from);
                }
            }
            k if k == Kind::Link as u8 => self.on_link(&mut r, from, now),
            k if k == Kind::WrongProtocol as u8 => {
                if let Role::Client(c) = &self.role
                    && c.host == from
                {
                    let theirs = r.str().unwrap_or_default();
                    let why = format!(
                        "The host is on build {theirs}, you are on {}. Use the same build.",
                        self.id.version
                    );
                    self.lose_host(why);
                }
            }
            _ => {}
        }
    }

    fn beacon(&self) -> Option<Beacon> {
        let Role::Host(h) = &self.role else {
            return None;
        };
        let host = h.lobby.slots[0].name.clone();
        Some(Beacon {
            host,
            map: MAPS
                .get(h.lobby.settings.map)
                .copied()
                .unwrap_or("?")
                .to_string(),
            players: h.lobby.slots.iter().filter(|s| s.occupied()).count() as u8,
            max_players: MAX_PLAYERS as u8,
            version: self.id.version.clone(),
            playing: h.game.is_some(),
        })
    }

    fn on_link(&mut self, r: &mut Reader, from: SocketAddr, now: Instant) {
        match &mut self.role {
            Role::Host(h) => {
                let index = match h.peers.iter().position(|p| p.addr == from) {
                    Some(i) => i,
                    None => {
                        h.peers.push(Peer {
                            addr: from,
                            link: Link::new(now),
                            slot: None,
                            player: None,
                            name: String::new(),
                            loaded: false,
                            gone: false,
                            drop_at: Some(now + Duration::from_secs(5)),
                        });
                        h.peers.len() - 1
                    }
                };
                let Some(msgs) = h.peers[index].link.receive(r, now) else {
                    return;
                };
                if h.peers[index].gone {
                    return;
                }
                for msg in msgs {
                    self.host_message(index, msg, now);
                }
                self.sync_lobby();
            }
            Role::Client(c) => {
                if c.host != from {
                    return;
                }
                let Some(msgs) = c.link.receive(r, now) else {
                    return;
                };
                for msg in msgs {
                    self.client_message(msg);
                }
            }
            Role::Idle => {}
        }
    }

    fn host_message(&mut self, index: usize, msg: Msg, now: Instant) {
        let version = self.id.version.clone();
        let data_hash = self.id.data_hash;
        let Role::Host(h) = &mut self.role else {
            return;
        };
        match msg {
            Msg::Hello {
                version: theirs,
                data_hash: their_data,
                name,
                faction,
                color,
            } => {
                if h.peers[index].slot.is_some() {
                    return;
                }
                let refuse = if h.game.is_some() {
                    Some("That game has already started.".to_string())
                } else if their_data != data_hash && theirs == version {
                    Some("Your game data differs from the host's. Use the same build.".into())
                } else {
                    None
                };
                let free = (1..MAX_PLAYERS).find(|&i| h.lobby.slots[i].kind == SlotKind::Open);
                let peer = &mut h.peers[index];
                match (refuse, free) {
                    (Some(reason), _) => peer.link.send(&Msg::Refuse { reason }),
                    (None, None) => peer.link.send(&Msg::Refuse {
                        reason: "That lobby is full.".into(),
                    }),
                    (None, Some(slot)) => {
                        let name = short_name(name.trim());
                        let name = if name.is_empty() {
                            format!("Player {}", slot + 1)
                        } else {
                            name
                        };
                        peer.slot = Some(slot);
                        peer.name = name.clone();
                        peer.drop_at = None;
                        let taken: Vec<u8> = h
                            .lobby
                            .slots
                            .iter()
                            .filter(|s| s.occupied())
                            .map(|s| s.color)
                            .collect();
                        let color = if taken.contains(&(color % 4)) {
                            (0..4u8).find(|c| !taken.contains(c)).unwrap_or(0)
                        } else {
                            color % 4
                        };
                        h.lobby.slots[slot] = Slot {
                            kind: SlotKind::Human,
                            name,
                            faction,
                            color,
                            team: slot as u8,
                            ready: false,
                            version: short_name(&theirs),
                            ai_level: DEFAULT_AI_LEVEL,
                        };
                        peer.link.send(&Msg::Welcome { slot: slot as u8 });
                    }
                }
            }
            Msg::Edit { slot, edit } => {
                if h.game.is_none()
                    && let Some(mine) = h.peers[index].slot
                {
                    apply_edit(&mut h.lobby, mine, false, slot as usize, edit);
                }
            }
            Msg::Loaded { error } => {
                let Some(g) = &mut h.game else { return };
                let _ = g;
                match error {
                    None => h.peers[index].loaded = true,
                    Some(why) => {
                        let text = format!("{} could not start: {why}", h.peers[index].name);
                        self.peer_gone(index, text, now);
                    }
                }
            }
            Msg::Orders { commands } => {
                let peer = &h.peers[index];
                if let (Some(g), Some(player)) = (&mut h.game, peer.player) {
                    g.inbox
                        .extend(commands.into_iter().map(|c| PlayerCommand { player, ..c }));
                }
            }
            Msg::Hash { tick, hash } => {
                let player = h.peers[index].player;
                if let (Some(g), Some(player)) = (&mut h.game, player) {
                    if g.theirs.len() < 1024 {
                        g.theirs.push((player, tick, hash));
                    }
                    self.compare_hashes();
                }
            }
            Msg::Bye { .. } => {
                let text = format!("{} left.", h.peers[index].name);
                self.peer_gone(index, text, now);
            }
            // Only the host sends the rest.
            _ => {}
        }
    }

    fn client_message(&mut self, msg: Msg) {
        let Role::Client(c) = &mut self.role else {
            return;
        };
        match msg {
            Msg::Welcome { slot } => c.slot = Some(slot as usize),
            Msg::Lobby(w) => {
                let w = *w;
                if let Some(slot) = c.slot {
                    c.lobby = Some(LobbyState {
                        slots: w.slots,
                        settings: w.settings,
                        local_slot: slot,
                        is_host: false,
                        host_version: w.host_version,
                    });
                }
            }
            Msg::Refuse { reason } => self.lose_host(reason),
            Msg::Start {
                seed,
                map_hash,
                settings,
                players,
                you,
                input_delay,
            } => {
                if c.game.is_some() {
                    return;
                }
                c.game = Some(ClientGame {
                    map: HostMap { seed, map_hash },
                    me: you,
                    delay: input_delay as u32,
                    ready: VecDeque::new(),
                    next_turn: 0,
                    due: None,
                    outbox: Vec::new(),
                });
                self.events.push(NetEvent::Started(MatchStart {
                    settings,
                    players,
                    local_player: you as usize,
                    seed: Some(seed),
                }));
            }
            Msg::Bundle { turn, commands } => {
                if let Some(g) = &mut c.game {
                    // The stream is ordered; the check guards against a
                    // host bug rather than the network.
                    if turn == g.next_turn + g.ready.len() as u32 {
                        g.ready.push_back(commands);
                    }
                }
            }
            Msg::Notice { text } => {
                if c.game.is_some() {
                    self.match_events.push(MatchEvent::Notice(text));
                }
            }
            Msg::Bye { reason } => self.lose_host(reason),
            _ => {}
        }
    }

    /// The host is gone or turned us away: back to idle, telling whoever
    /// is listening (the menus, or the match).
    fn lose_host(&mut self, why: String) {
        let Role::Client(c) = &self.role else {
            return;
        };
        if c.game.is_some() {
            self.match_events.push(MatchEvent::Ended(why));
        } else {
            self.events.push(NetEvent::Left(why));
        }
        self.role = Role::Idle;
        self.socket = None;
    }

    /// A player left or lost the connection: free their slot in the lobby,
    /// or tell everyone in a match (their regiments stand and defend).
    fn peer_gone(&mut self, index: usize, text: String, now: Instant) {
        let Role::Host(h) = &mut self.role else {
            return;
        };
        let peer = &mut h.peers[index];
        if peer.gone {
            return;
        }
        peer.gone = true;
        peer.drop_at = Some(now + Duration::from_secs(2));
        let slot = peer.slot.take();
        if h.game.is_none() {
            if let Some(s) = slot {
                h.lobby.slots[s] = Slot::open(s);
            }
            return;
        }
        for p in h.peers.iter_mut().filter(|p| !p.gone) {
            p.link.send(&Msg::Notice { text: text.clone() });
        }
        self.match_events.push(MatchEvent::Notice(text));
    }

    fn check_silence(&mut self, now: Instant) {
        match &mut self.role {
            Role::Host(h) => {
                h.peers
                    .retain(|p| p.drop_at.is_none_or(|t| now < t) || (p.slot.is_some() && !p.gone));
                let silent: Vec<usize> = (0..h.peers.len())
                    .filter(|&i| {
                        let p = &h.peers[i];
                        !p.gone && p.slot.is_some() && now - p.link.last_heard > PLAYER_TIMEOUT
                    })
                    .collect();
                let late: Vec<usize> = match &h.game {
                    Some(g) if g.go_at.is_none() && now - g.started_at > LOAD_TIMEOUT => {
                        (0..h.peers.len())
                            .filter(|&i| h.peers[i].player.is_some() && !h.peers[i].loaded)
                            .collect()
                    }
                    _ => Vec::new(),
                };
                for i in silent {
                    let text = format!("{} lost connection.", h_name(&self.role, i));
                    self.peer_gone(i, text, now);
                }
                for i in late {
                    let text = format!("{} took too long to load.", h_name(&self.role, i));
                    self.peer_gone(i, text, now);
                }
                self.sync_lobby();
                if let Role::Host(h) = &mut self.role {
                    start_turns(h, now);
                }
            }
            Role::Client(c) => {
                if now - c.link.last_heard > HOST_TIMEOUT {
                    self.lose_host("Lost connection to the host.".into());
                }
            }
            Role::Idle => {}
        }
    }

    /// Send the lobby to everyone if it changed.
    fn sync_lobby(&mut self) {
        let Role::Host(h) = &mut self.role else {
            return;
        };
        if h.game.is_some() {
            return;
        }
        // A human whose slot the host gave away is turned out.
        let now = Instant::now();
        for p in h.peers.iter_mut().filter(|p| !p.gone) {
            if let Some(s) = p.slot
                && h.lobby.slots[s].kind != SlotKind::Human
            {
                p.slot = None;
                p.gone = true;
                p.drop_at = Some(now + Duration::from_secs(2));
                p.link.send(&Msg::Bye {
                    reason: "The host gave your place to someone else.".into(),
                });
                // Gone peers get no more packets: send this one now.
                let bytes = p.link.packet(now);
                if let Some(s) = &self.socket {
                    let _ = s.send_to(&bytes, p.addr);
                }
            }
        }
        let wire = LobbyWire {
            slots: h.lobby.slots.clone(),
            settings: h.lobby.settings.clone(),
            host_version: h.lobby.host_version.clone(),
        };
        if h.sent.as_ref() == Some(&wire) {
            return;
        }
        for p in h.peers.iter_mut().filter(|p| p.slot.is_some() && !p.gone) {
            p.link.send(&Msg::Lobby(Box::new(wire.clone())));
        }
        h.sent = Some(wire);
    }

    fn compare_hashes(&mut self) {
        let Role::Host(h) = &mut self.role else {
            return;
        };
        let Some(g) = &mut h.game else { return };
        let oldest = g.hashes.front().map_or(0, |&(t, _)| t);
        let mut found = Vec::new();
        g.theirs.retain(|&(player, tick, hash)| {
            if tick < oldest {
                return false;
            }
            match g.hashes.iter().find(|&&(t, _)| t == tick) {
                Some(&(_, mine)) => {
                    if mine != hash && !g.desynced.contains(&player) {
                        found.push((player, tick));
                    }
                    false
                }
                None => true,
            }
        });
        for (player, tick) in found {
            g.desynced.push(player);
            let name = h
                .peers
                .iter()
                .find(|p| p.player == Some(player))
                .map_or("A player", |p| p.name.as_str())
                .to_string();
            let text = format!("{name} is out of sync with the host (turn {tick}).");
            for p in h.peers.iter_mut().filter(|p| !p.gone) {
                p.link.send(&Msg::Notice { text: text.clone() });
            }
            self.match_events.push(MatchEvent::Notice(text));
            self.match_events.push(MatchEvent::Desync { tick });
        }
    }

    /// The Join screen's search: ask every host on the network, and on this
    /// machine, to describe its lobby.
    fn find(&mut self, now: Instant) {
        if self.finder.is_none() {
            return;
        }
        if self.last_query.is_none_or(|t| now - t >= QUERY_EVERY) {
            self.last_query = Some(now);
            let mut w = Writer::new();
            header(&mut w, Kind::Query);
            let finder = self.finder.as_ref().unwrap();
            for port in DEFAULT_PORT..DEFAULT_PORT + PORTS {
                for ip in [Ipv4Addr::BROADCAST, Ipv4Addr::LOCALHOST] {
                    let _ = finder.send_to(&w.buf, SocketAddrV4::new(ip, port));
                }
            }
        }
        let mut buf = [0u8; MAX_PACKET];
        let finder = self.finder.as_ref().unwrap();
        while let Ok((n, from)) = finder.recv_from(&mut buf) {
            let mut r = Reader::new(&buf[..n]);
            let Some((protocol, kind)) = read_header(&mut r) else {
                continue;
            };
            if protocol != PROTOCOL || kind != Kind::Beacon as u8 {
                continue;
            }
            let Some(b) = Beacon::read(&mut r) else {
                continue;
            };
            if b.playing {
                continue;
            }
            let game = DiscoveredGame {
                host: b.host,
                map: b.map,
                players: b.players,
                max_players: b.max_players,
                version: b.version,
                address: from.to_string(),
            };
            // The same host heard on loopback and on the network is one game.
            let same = |g: &DiscoveredGame| {
                g.address == game.address
                    || (g.host == game.host && g.address.ends_with(&format!(":{}", from.port())))
            };
            self.found.retain(|(g, _)| !same(g));
            self.found.push((game, now));
        }
        self.found.retain(|(_, t)| now - *t < DISCOVERED_FOR);
        self.found
            .sort_by(|a, b| a.0.host.cmp(&b.0.host).then(a.0.address.cmp(&b.0.address)));
        self.games = self.found.iter().map(|(g, _)| g.clone()).collect();
    }

    fn bind_host() -> Result<UdpSocket, NetError> {
        for port in DEFAULT_PORT..DEFAULT_PORT + PORTS {
            if let Ok(s) = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)) {
                return Ok(s);
            }
        }
        Err(NetError::Unavailable(format!(
            "Could not open a network port ({DEFAULT_PORT} to {}): is another program using them?",
            DEFAULT_PORT + PORTS - 1
        )))
    }

    fn open(socket: UdpSocket) -> Result<UdpSocket, NetError> {
        socket
            .set_nonblocking(true)
            .map_err(|e| NetError::Unavailable(format!("Network error: {e}")))?;
        let _ = socket.set_broadcast(true);
        Ok(socket)
    }
}

fn h_name(role: &Role, index: usize) -> String {
    match role {
        Role::Host(h) => h.peers[index].name.clone(),
        _ => String::new(),
    }
}

/// Split queued orders into messages that each fit a packet.
fn orders_messages(outbox: &mut Vec<PlayerCommand>) -> Vec<Msg> {
    let mut msgs = Vec::new();
    let mut batch = Vec::new();
    let mut bytes = 0;
    for c in outbox.drain(..) {
        let n = command_size(&c);
        if bytes + n > MAX_BUNDLE_BYTES && !batch.is_empty() {
            msgs.push(Msg::Orders {
                commands: std::mem::take(&mut batch),
            });
            bytes = 0;
        }
        bytes += n;
        batch.push(c);
    }
    if !batch.is_empty() {
        msgs.push(Msg::Orders { commands: batch });
    }
    msgs
}

/// Once everyone has loaded, begin: close the first turns (the first holds
/// every army's deployment).
fn start_turns(h: &mut Host, now: Instant) {
    let Some(g) = &mut h.game else { return };
    if g.go_at.is_some() || !g.host_loaded {
        return;
    }
    let waiting = h
        .peers
        .iter()
        .any(|p| p.player.is_some() && !p.loaded && !p.gone);
    if waiting {
        return;
    }
    g.go_at = Some(now);
    for turn in 0..INPUT_DELAY {
        close_turn(h, turn);
    }
}

/// Take commands from the inbox for `turn` and send the bundle to everyone.
fn close_turn(h: &mut Host, turn: u32) {
    let Some(g) = &mut h.game else { return };
    let mut commands = Vec::new();
    let mut bytes = 0;
    while let Some(c) = g.inbox.front() {
        let n = command_size(c);
        if bytes + n > MAX_BUNDLE_BYTES && !commands.is_empty() {
            break;
        }
        bytes += n;
        commands.extend(g.inbox.pop_front());
    }
    for p in h.peers.iter_mut().filter(|p| p.player.is_some() && !p.gone) {
        p.link.send(&Msg::Bundle {
            turn,
            commands: commands.clone(),
        });
    }
    g.closed.push_back(commands);
}

fn host_turn(h: &mut Host, now: Instant, length: Duration) -> Option<Vec<PlayerCommand>> {
    let g = h.game.as_mut()?;
    let go = g.go_at?;
    let due = ((now - go).as_nanos() / length.as_nanos()) as u32 + 1;
    if g.handed >= due {
        return None;
    }
    if due - g.handed > MAX_HOST_LAG {
        // A long hitch: carry on from now rather than rush everyone.
        g.go_at = now.checked_sub(length * g.handed);
    }
    let turn = g.handed;
    g.handed += 1;
    close_turn(h, turn + INPUT_DELAY);
    h.game.as_mut()?.closed.pop_front()
}

fn client_turn(g: &mut ClientGame, now: Instant, length: Duration) -> Option<Vec<PlayerCommand>> {
    if g.ready.is_empty() {
        return None;
    }
    let due = *g.due.get_or_insert(now);
    // Behind by more than the input delay: catch up a turn at a time.
    let catching_up = g.ready.len() as u32 > g.delay + 1;
    if now < due && !catching_up {
        return None;
    }
    // After a stall, don't rush the turns that piled up on the clock.
    let base = if now > due + length * 2 { now } else { due };
    g.due = Some(base + length);
    g.next_turn += 1;
    g.ready.pop_front()
}

impl Network for Lan {
    fn host(&mut self, me: &Profile, settings: LobbySettings) -> Result<(), NetError> {
        self.leave();
        let socket = Lan::open(Lan::bind_host()?)?;
        let mut slots = [Slot::open(0), Slot::open(1), Slot::open(2), Slot::open(3)];
        slots[0] = Slot {
            kind: SlotKind::Human,
            name: short_name(&me.name),
            faction: me.faction,
            color: me.color % 4,
            team: 0,
            ready: true,
            version: self.id.version.clone(),
            ai_level: DEFAULT_AI_LEVEL,
        };
        self.socket = Some(socket);
        self.role = Role::Host(Box::new(Host {
            lobby: LobbyState {
                slots,
                settings,
                local_slot: 0,
                is_host: true,
                host_version: self.id.version.clone(),
            },
            peers: Vec::new(),
            sent: None,
            game: None,
        }));
        Ok(())
    }

    fn discovered(&mut self) -> &[DiscoveredGame] {
        if matches!(self.role, Role::Idle) && self.finder.is_none() {
            self.finder = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))
                .ok()
                .and_then(|s| Lan::open(s).ok());
        }
        self.find(Instant::now());
        &self.games
    }

    fn join(&mut self, me: &Profile, target: &str) -> Result<(), NetError> {
        let target = target.trim();
        if target.is_empty() {
            return Err(NetError::BadAddress("Type an address or join code".into()));
        }
        let addr = parse_address(target)?;
        self.leave();
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))
            .map_err(|e| NetError::Unavailable(format!("Network error: {e}")))?;
        self.socket = Some(Lan::open(socket)?);
        let start = Instant::now();
        let mut link = Link::new(start);
        link.send(&Msg::Hello {
            version: self.id.version.clone(),
            data_hash: self.id.data_hash,
            name: short_name(&me.name),
            faction: me.faction,
            color: me.color,
        });
        self.role = Role::Client(Box::new(Client {
            host: addr,
            link,
            slot: None,
            lobby: None,
            game: None,
        }));
        // Wait for the host's answer: the lobby, or a refusal.
        loop {
            let now = Instant::now();
            self.flush(now, false);
            self.pump(now);
            match &self.role {
                Role::Client(c) if c.lobby.is_some() => return Ok(()),
                Role::Client(_) if now - start > JOIN_TIMEOUT => {
                    self.leave();
                    return Err(NetError::Refused(format!(
                        "No answer from {addr}. Is a game hosted there, on the same network?"
                    )));
                }
                Role::Client(_) => std::thread::sleep(Duration::from_millis(5)),
                _ => {
                    let why = self.events.iter().rev().find_map(|e| match e {
                        NetEvent::Left(why) => Some(why.clone()),
                        _ => None,
                    });
                    self.events.clear();
                    return Err(NetError::Refused(why.unwrap_or_else(|| "Refused".into())));
                }
            }
        }
    }

    fn lobby(&self) -> Option<&LobbyState> {
        match &self.role {
            Role::Host(h) => Some(&h.lobby),
            Role::Client(c) => c.lobby.as_ref(),
            Role::Idle => None,
        }
    }

    fn edit_slot(&mut self, slot: usize, edit: SlotEdit) {
        match &mut self.role {
            Role::Host(h) if h.game.is_none() => apply_edit(&mut h.lobby, 0, true, slot, edit),
            Role::Client(c) if c.game.is_none() => {
                c.link.send(&Msg::Edit {
                    slot: slot as u8,
                    edit,
                });
            }
            _ => return,
        }
        self.sync_lobby();
        self.flush(Instant::now(), false);
    }

    fn edit_settings(&mut self, settings: LobbySettings) {
        if let Role::Host(h) = &mut self.role
            && h.game.is_none()
        {
            h.lobby.settings = settings;
            self.sync_lobby();
        }
    }

    fn start_match(&mut self) -> Result<MatchStart, NetError> {
        let seed = self.id.seed;
        let map_hash = self.id.map_hash;
        let Role::Host(h) = &mut self.role else {
            return Err(NetError::Refused("Only the host starts the match".into()));
        };
        if h.game.is_some() {
            return Err(NetError::Refused("The match has already started".into()));
        }
        if let Some(why) = h.lobby.start_blocker() {
            return Err(NetError::Refused(why));
        }
        // With nobody else on the network, the match runs on this machine
        // alone, with no input delay.
        if !h.peers.iter().any(|p| p.slot.is_some() && !p.gone) {
            return Ok(h.lobby.match_start(None));
        }
        let start = h.lobby.match_start(Some(seed));
        let occupied: Vec<usize> = (0..MAX_PLAYERS)
            .filter(|&i| h.lobby.slots[i].occupied())
            .collect();
        for p in h.peers.iter_mut().filter(|p| !p.gone) {
            let Some(slot) = p.slot else { continue };
            let you = occupied.iter().position(|&s| s == slot).unwrap_or(0) as PlayerId;
            p.player = Some(you);
            p.link.send(&Msg::Start {
                seed,
                map_hash,
                settings: start.settings.clone(),
                players: start.players.clone(),
                you,
                input_delay: INPUT_DELAY as u8,
            });
        }
        h.game = Some(HostGame {
            go_at: None,
            started_at: Instant::now(),
            host_loaded: false,
            handed: 0,
            closed: VecDeque::new(),
            inbox: VecDeque::new(),
            hashes: VecDeque::new(),
            theirs: Vec::new(),
            desynced: Vec::new(),
        });
        self.flush(Instant::now(), true);
        Ok(start)
    }

    fn leave(&mut self) {
        let now = Instant::now();
        match &mut self.role {
            Role::Host(h) => {
                let reason = if h.game.is_some() {
                    "The host left the match."
                } else {
                    "The host closed the lobby."
                };
                for p in h.peers.iter_mut().filter(|p| !p.gone) {
                    p.link.send(&Msg::Bye {
                        reason: reason.into(),
                    });
                }
            }
            Role::Client(c) => c.link.send(&Msg::Bye {
                reason: "left".into(),
            }),
            Role::Idle => {}
        }
        // Goodbyes are not waited on: send them twice in case one is lost.
        self.flush(now, true);
        self.flush(now, true);
        self.role = Role::Idle;
        self.socket = None;
        self.finder = None;
        self.found.clear();
        self.games.clear();
    }

    fn poll(&mut self) -> Vec<NetEvent> {
        self.pump(Instant::now());
        std::mem::take(&mut self.events)
    }
}

impl Drop for Lan {
    /// Quitting the game says goodbye, so the others hear at once rather
    /// than after a silence.
    fn drop(&mut self) {
        if !matches!(self.role, Role::Idle) {
            self.leave();
        }
    }
}

/// `ip:port`, or an IP or host name on the default port.
fn parse_address(target: &str) -> Result<SocketAddr, NetError> {
    if let Ok(a) = target.parse::<SocketAddr>() {
        return Ok(a);
    }
    if let Ok(ip) = target.parse::<std::net::IpAddr>() {
        return Ok(SocketAddr::new(ip, DEFAULT_PORT));
    }
    let with_port = if target.contains(':') {
        target.to_string()
    } else {
        format!("{target}:{DEFAULT_PORT}")
    };
    with_port
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.find(|a| a.is_ipv4()))
        .ok_or_else(|| {
            NetError::BadAddress(format!(
                "\"{target}\" is not an address. Type the host's IP, like 192.168.1.20."
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_parse() {
        assert_eq!(
            parse_address("192.168.1.20").unwrap(),
            "192.168.1.20:47820".parse().unwrap()
        );
        assert_eq!(
            parse_address("10.0.0.2:5000").unwrap(),
            "10.0.0.2:5000".parse().unwrap()
        );
        assert!(matches!(
            parse_address("not an address!"),
            Err(NetError::BadAddress(_))
        ));
    }

    #[test]
    fn orders_split_to_fit_packets() {
        let c = PlayerCommand {
            player: 1,
            command: gc_sim::Command::RemoveRegiment {
                regiment: gc_sim::RegimentHandle {
                    index: 1000,
                    generation: 3,
                },
            },
        };
        let mut outbox = vec![c; 500];
        let msgs = orders_messages(&mut outbox);
        assert!(msgs.len() > 1);
        assert!(outbox.is_empty());
        let total: usize = msgs
            .iter()
            .map(|m| {
                assert!(m.encode().len() <= crate::proto::MAX_MESSAGE);
                match m {
                    Msg::Orders { commands } => commands.len(),
                    _ => 0,
                }
            })
            .sum();
        assert_eq!(total, 500);
    }
}
