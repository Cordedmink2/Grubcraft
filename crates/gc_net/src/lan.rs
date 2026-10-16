//! [`Lan`]: hosting and joining over the local network, the lobby protocol,
//! and the host-sequenced lockstep turns of a match.

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use gc_sim::{PlayerCommand, PlayerId};
use gc_ui::lobby::{
    DEFAULT_AI_LEVEL, DiscoveredGame, LobbySettings, LobbyState, MAPS, MAX_ARMY, MAX_PLAYERS,
    MatchStart, NetError, NetEvent, Network, OnDrop, Profile, Slot, SlotEdit, SlotKind, apply_edit,
    plays_with,
};

use crate::crypt::{Cipher, KeyPair};
use crate::joincode;
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
/// everywhere (docs/11-networking.md, "Input delay") when a match starts:
/// 100 ms, right for home Wi-Fi. From then on it follows the measured
/// round trips, between [`MIN_DELAY`] and [`MAX_DELAY`].
pub const INPUT_DELAY: u32 = 2;
pub const MIN_DELAY: u32 = 1;
pub const MAX_DELAY: u32 = 8;
/// Added to the slowest player's half round trip and jitter when choosing
/// the delay, for the host's own frame and the receiver's.
const DELAY_MARGIN: Duration = Duration::from_millis(10);
/// The delay is looked at again every this many turns (a second).
const DELAY_EVERY: u32 = 20;
/// It goes up a turn as soon as it is too short, and down a turn only
/// after being longer than needed this many looks in a row.
const DELAY_LOWER_AFTER: u32 = 3;

/// Players send the host a state hash every this many ticks.
pub const HASH_EVERY: u32 = 10;

/// A player silent this long has lost the connection.
pub const PLAYER_TIMEOUT: Duration = Duration::from_secs(3);
/// A host silent this long has gone.
pub const HOST_TIMEOUT: Duration = Duration::from_secs(5);
/// With "Pause and wait", how long a match waits for a player who lost
/// the connection to come back, and a joiner for the host.
pub const RECONNECT_WAIT: Duration = Duration::from_secs(90);
/// How long the host waits for everyone to build their world.
pub const LOAD_TIMEOUT: Duration = Duration::from_secs(60);
/// How long Join waits for the host to answer.
pub const JOIN_TIMEOUT: Duration = Duration::from_secs(3);

const LOBBY_KEEPALIVE: Duration = Duration::from_millis(200);
const MATCH_KEEPALIVE: Duration = TURN;
const QUERY_EVERY: Duration = Duration::from_secs(1);
/// A joiner repeats its key until the host answers.
const HI_EVERY: Duration = Duration::from_millis(100);
const DISCOVERED_FOR: Duration = Duration::from_secs(3);
/// Joiners the host keeps keys for before they are let in; more Hi
/// packets are ignored until some are dropped.
const MAX_WAITING: usize = 8;
/// Wrong-password answers one address gets per GUESS_WINDOW; past that
/// its guesses go unanswered and it gets no new key exchange.
const MAX_GUESSES: usize = 5;
const GUESS_WINDOW: Duration = Duration::from_secs(60);
/// Commands per turn bundle are capped so a bundle fits a packet; the rest
/// go in the next turn.
const MAX_BUNDLE_BYTES: usize = 900;
/// Most commands one joiner may have waiting for a turn; more are dropped.
/// A group order is one command per regiment, so this holds 16 orders to a
/// whole army, more than anyone gives while a match is paused for a player.
const MAX_QUEUED_ORDERS: usize = 16 * MAX_ARMY;
/// If the host falls this many turns behind its clock (a long hitch), it
/// stops trying to catch up and carries on from now.
const MAX_HOST_LAG: u32 = 20;
/// A player with more messages than this waiting (one catching up on a
/// whole match after rejoining) gets several packets a round, up to
/// [`CATCH_UP_BURST`].
const CATCH_UP_BACKLOG: usize = 32;
const CATCH_UP_BURST: usize = 24;
/// Bursts are at least this far apart (and a round trip), so the other
/// side's acknowledgement (sent at least every [`MATCH_KEEPALIVE`]) comes
/// back before the next.
const BURST_GAP: Duration = Duration::from_millis(60);

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
    /// A player's world no longer matches the host's from `tick`; time to
    /// save a desync report (the game saves its recording of the match).
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
    /// Their public key and the host's answer to it, kept to answer a
    /// repeated Hi the same way.
    their_key: [u8; 32],
    host_key: [u8; 32],
    /// Secured from the start (the keys come before the link).
    link: Link,
    /// The lobby slot this player holds; `None` while being turned away.
    slot: Option<usize>,
    /// Their player number in the match, once it starts.
    player: Option<PlayerId>,
    name: String,
    loaded: bool,
    /// When the connection went quiet, while the match waits for them.
    lost_at: Option<Instant>,
    /// Lost or left; nothing more is sent or taken.
    gone: bool,
    /// Forget this peer at this time (after a refusal or a goodbye has had
    /// a chance to arrive).
    drop_at: Option<Instant>,
}

impl Peer {
    /// Connected: neither gone nor lost.
    fn active(&self) -> bool {
        !self.gone && self.lost_at.is_none()
    }
}

struct HostGame {
    /// When turn 0 began; `None` until everyone has loaded.
    go_at: Option<Instant>,
    started_at: Instant,
    host_loaded: bool,
    /// Turns handed to this machine's simulation.
    handed: u32,
    /// The next turn to close; turns below it are closed.
    next_close: u32,
    /// The input delay, in turns.
    delay: u32,
    /// Looks in a row that found the delay longer than needed.
    lower_votes: u32,
    /// Who the match is paused for, if anyone (as sent to the players).
    waiting_for: String,
    /// Every bundle closed so far, by turn: this machine simulates them
    /// from `handed`, and a player who rejoins after a restart gets them
    /// all.
    history: Vec<Vec<PlayerCommand>>,
    /// The match as it started, to tell a player who rejoins.
    start: MatchStart,
    /// Each player's lobby slot.
    slots: Vec<usize>,
    /// The address each human player plays from: only that computer may
    /// take their place back after a restart.
    ips: Vec<Option<IpAddr>>,
    /// Commands waiting for a turn, in arrival order.
    inbox: VecDeque<PlayerCommand>,
    /// This machine's state hashes, oldest first.
    hashes: VecDeque<(u32, u64)>,
    /// Players' hashes not yet compared: (player, tick, hash).
    theirs: Vec<(PlayerId, u32, u64)>,
    desynced: Vec<PlayerId>,
}

struct Host {
    /// Empty for none.
    password: String,
    /// How friends reach this lobby, for the lobby screen.
    invite: Option<String>,
    /// Recent wrong passwords: from where, and when.
    guesses: Vec<(IpAddr, Instant)>,
    lobby: LobbyState,
    peers: Vec<Peer>,
    sent: Option<LobbyWire>,
    game: Option<HostGame>,
}

struct ClientGame {
    map: HostMap,
    me: PlayerId,
    delay: u32,
    on_drop: OnDrop,
    /// Paused by the host while someone reconnects: who, and until when.
    wait: Option<(String, Instant)>,
    /// Turn bundles received and not yet simulated, in turn order.
    ready: VecDeque<Vec<PlayerCommand>>,
    next_turn: u32,
    /// When the next turn is due on this machine's clock.
    due: Option<Instant>,
    outbox: Vec<PlayerCommand>,
}

struct Client {
    host: SocketAddr,
    keys: KeyPair,
    password: String,
    hi_sent: Option<Instant>,
    /// Secured once the host has answered our key; until then we only
    /// send that key.
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
                if !g.waiting_for.is_empty() {
                    let left = h
                        .peers
                        .iter()
                        .filter_map(|p| p.lost_at.filter(|_| !p.gone))
                        .map(|t| RECONNECT_WAIT.saturating_sub(t.elapsed()))
                        .min()
                        .unwrap_or_default();
                    return Some(format!(
                        "Paused: waiting for {} to reconnect ({} s)",
                        g.waiting_for,
                        left.as_secs()
                    ));
                }
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
                let quiet = c.link.last_heard.elapsed();
                if quiet > Duration::from_secs(1) {
                    let left = client_timeout(c).saturating_sub(quiet);
                    return Some(format!(
                        "Lost contact with the host, trying to reconnect ({} s)",
                        left.as_secs()
                    ));
                }
                if let Some((who, until)) = &g.wait {
                    let left = until.saturating_duration_since(Instant::now());
                    return Some(format!(
                        "Paused: waiting for {who} to reconnect ({} s)",
                        left.as_secs()
                    ));
                }
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
                    h.game.as_ref().map_or(INPUT_DELAY, |g| g.delay) * TURN.as_millis() as u32,
                    if peers.is_empty() { "" } else { ": " },
                    peers.join(", ")
                ))
            }
            Role::Client(c) => Some(format!(
                "joined {}, round trip {} ms{}",
                c.host,
                ms(&c.link),
                c.game.as_ref().map_or(String::new(), |g| format!(
                    ", delay {} ms, {} turns ahead",
                    g.delay * TURN.as_millis() as u32,
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

    /// Whether this machine is more than a few turns behind the host (it
    /// rejoined, or stalled): the game may then run more turns a frame.
    pub fn catching_up(&self) -> bool {
        match &self.role {
            Role::Client(c) => c.game.as_ref().is_some_and(ClientGame::behind),
            _ => false,
        }
    }

    /// The input delay the host has set, in turns, during a match.
    pub fn input_delay(&self) -> Option<u32> {
        match &self.role {
            Role::Host(h) => h.game.as_ref().map(|g| g.delay),
            Role::Client(c) => c.game.as_ref().map(|g| g.delay),
            Role::Idle => None,
        }
    }

    /// Whether the host is holding the match for someone to reconnect
    /// (it can [`Lan::carry_on`] without them).
    pub fn paused_for_player(&self) -> bool {
        match &self.role {
            Role::Host(h) => h.game.as_ref().is_some_and(|g| !g.waiting_for.is_empty()),
            _ => false,
        }
    }

    /// The host stops waiting for the players who lost the connection:
    /// the match carries on and their regiments stand and defend.
    pub fn carry_on(&mut self) {
        let now = Instant::now();
        let Role::Host(h) = &self.role else { return };
        let lost: Vec<usize> = (0..h.peers.len())
            .filter(|&i| h.peers[i].lost_at.is_some() && !h.peers[i].gone)
            .collect();
        for i in lost {
            let text = format!("Carrying on without {}.", h_name(&self.role, i));
            self.peer_gone(i, text, now);
        }
        self.update_wait(now);
        self.flush(now, false);
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
                Err(_) => {}
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
                    let due = due_packets(&mut p.link, now, all, keepalive);
                    out.extend(due.into_iter().map(|b| (b, p.addr)));
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
                if c.link.is_secure() {
                    out.extend(
                        due_packets(&mut c.link, now, all, keepalive)
                            .into_iter()
                            .map(|b| (b, c.host)),
                    );
                } else if all || c.hi_sent.is_none_or(|t| now - t >= HI_EVERY) {
                    c.hi_sent = Some(now);
                    out.push((key_packet(Kind::Hi, &c.keys.public), c.host));
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
            if self.is_host() && (kind == Kind::Link as u8 || kind == Kind::Hi as u8) {
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
            k if k == Kind::Link as u8 => self.on_link(packet, from, now),
            k if k == Kind::Hi as u8 => {
                if let Some(key) = r.array()
                    && r.is_empty()
                {
                    self.on_hi(key, from, now);
                }
            }
            k if k == Kind::KeyReply as u8 => {
                if let Role::Client(c) = &mut self.role
                    && c.host == from
                    && !c.link.is_secure()
                    && let Some(key) = r.array()
                {
                    match Cipher::new(&c.keys, key, false, &c.password) {
                        Some(cipher) => {
                            c.link.secure(cipher);
                            self.flush(now, true);
                        }
                        None => self.lose_host("The host's key was not valid.".into()),
                    }
                }
            }
            k if k == Kind::WrongPassword as u8 => {
                if let Role::Client(c) = &self.role
                    && c.host == from
                    && c.lobby.is_none()
                {
                    let locked = r.bool().unwrap_or(true);
                    let why = match (locked, c.password.is_empty()) {
                        (true, true) => "That game needs a password: type it in the Password box.",
                        (true, false) => "Wrong password.",
                        (false, _) => "That game has no password: clear the Password box.",
                    };
                    self.lose_host(why.into());
                }
            }
            k if k == Kind::WrongProtocol as u8 => {
                // Plain and unsigned, so only heeded before the host has
                // answered our key: after that it has spoken our protocol.
                if let Role::Client(c) = &self.role
                    && c.host == from
                    && !c.link.is_secure()
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
            locked: !h.password.is_empty(),
        })
    }

    /// A joiner's key: answer with ours, and keep the keys for their link.
    fn on_hi(&mut self, theirs: [u8; 32], from: SocketAddr, now: Instant) {
        let Role::Host(h) = &mut self.role else {
            return;
        };
        let known = h.peers.iter().position(|p| p.addr == from);
        // An address out of wrong-password guesses gets no key exchange to
        // guess with, unless it holds a place in the match (a player whose
        // game restarted, rejoining).
        let has_place = known.is_some_and(|i| h.peers[i].player.is_some());
        if !has_place && h.out_of_guesses(from.ip(), now) {
            return;
        }
        let reply_key = match known {
            Some(i) if h.peers[i].their_key == theirs => h.peers[i].host_key,
            // A new key from someone already in: a restarted game on the
            // same port. They can join again once their old place is freed.
            Some(i) if h.peers[i].slot.is_some() && h.peers[i].active() => return,
            _ => {
                match known {
                    // A lost player's place stays theirs (and the match
                    // waits) until this newcomer proves to be them.
                    Some(i) if h.peers[i].player.is_some() && !h.peers[i].gone => {
                        h.peers[i].addr = SocketAddr::from(([0, 0, 0, 0], 0));
                    }
                    Some(i) => {
                        h.peers.remove(i);
                    }
                    None => {}
                }
                let waiting = h.peers.iter().filter(|p| p.slot.is_none() && !p.gone);
                if waiting.count() >= MAX_WAITING {
                    return;
                }
                let Some(mine) = KeyPair::new() else { return };
                let Some(cipher) = Cipher::new(&mine, theirs, true, &h.password) else {
                    return;
                };
                let mut link = Link::new(now);
                link.secure(cipher);
                h.peers.push(Peer {
                    addr: from,
                    their_key: theirs,
                    host_key: mine.public,
                    link,
                    slot: None,
                    player: None,
                    name: String::new(),
                    loaded: false,
                    lost_at: None,
                    gone: false,
                    drop_at: Some(now + Duration::from_secs(5)),
                });
                mine.public
            }
        };
        self.send_raw(&key_packet(Kind::KeyReply, &reply_key), from);
    }

    fn on_link(&mut self, packet: &[u8], from: SocketAddr, now: Instant) {
        match &mut self.role {
            Role::Host(h) => {
                // Links begin with a key exchange: strangers are ignored.
                let Some(index) = h.peers.iter().position(|p| p.addr == from) else {
                    return;
                };
                let Some(msgs) = h.peers[index].link.receive(packet, now) else {
                    // A joiner whose first packet we can't open typed a
                    // different password. Forget their keys, and answer
                    // only a few guesses a minute from one address.
                    let peer = &mut h.peers[index];
                    if peer.slot.is_some() || peer.gone || peer.link.is_proven() {
                        return;
                    }
                    peer.gone = true;
                    peer.drop_at = Some(now + Duration::from_secs(2));
                    let ip = from.ip();
                    if h.out_of_guesses(ip, now) {
                        return;
                    }
                    // Without a password there is nothing to guess.
                    if !h.password.is_empty() {
                        h.guesses.push((ip, now));
                    }
                    let mut w = Writer::new();
                    header(&mut w, Kind::WrongPassword);
                    w.bool(!h.password.is_empty());
                    self.send_raw(&w.buf, from);
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
                if !c.link.is_secure() {
                    return;
                }
                let Some(msgs) = c.link.receive(packet, now) else {
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
                if h.game.is_some() && plays_with(&theirs, &version) && their_data == data_hash {
                    let name = short_name(name.trim());
                    if self.rejoin(index, &name, now) {
                        return;
                    }
                }
                let Role::Host(h) = &mut self.role else {
                    return;
                };
                let refuse = if h.game.is_some() {
                    Some("That game has already started.".to_string())
                } else if their_data != data_hash && plays_with(&theirs, &version) {
                    // Different rules are let in and held at the lobby's
                    // version line; the same rules with different data would
                    // start and desync at once.
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
                        peer.name.clone_from(&name);
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
                            army: Vec::new(),
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
                    // The host raises every army and sets out the
                    // deployment; a joiner only orders its own regiments.
                    // Every machine would apply the sandbox's spawning,
                    // removing and tuning alike, so nothing would flag it.
                    let queued = g.inbox.iter().filter(|c| c.player == player).count();
                    g.inbox.extend(
                        commands
                            .into_iter()
                            .filter(|c| c.command.is_player_order())
                            .take(MAX_QUEUED_ORDERS.saturating_sub(queued))
                            .map(|c| PlayerCommand { player, ..c }),
                    );
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
                    delay: u32::from(input_delay),
                    on_drop: settings.on_drop,
                    wait: None,
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
            Msg::Delay { turns } => {
                if let Some(g) = &mut c.game {
                    g.delay = u32::from(turns);
                }
            }
            Msg::Wait { who, seconds } => {
                if let Some(g) = &mut c.game {
                    g.wait = (!who.is_empty()).then(|| {
                        (
                            who,
                            Instant::now() + Duration::from_secs(u64::from(seconds)),
                        )
                    });
                }
            }
            Msg::Desync { tick } if c.game.is_some() => {
                self.match_events.push(MatchEvent::Desync { tick });
            }
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

    /// A player whose game restarted joins the match again in their old
    /// place: the one human place named `name` that nobody holds now (its
    /// player lost connection or left). They are sent the match's start
    /// and every turn so far, and catch up from the first. Returns false
    /// when there is no such place.
    fn rejoin(&mut self, index: usize, name: &str, now: Instant) -> bool {
        let map_hash = self.id.map_hash;
        let Role::Host(h) = &mut self.role else {
            return false;
        };
        let Some(g) = &h.game else { return false };
        let ip = h.peers[index].addr.ip();
        // The host is player 0 and never rejoins. A place is taken back
        // from the computer that held it, under the name it had (an empty
        // name was "Player N" in the lobby).
        let Some(player) = (1..g.start.players.len()).find(|&i| {
            let s = &g.start.players[i];
            let named = if name.is_empty() {
                g.slots
                    .get(i)
                    .is_some_and(|slot| s.name == format!("Player {}", slot + 1))
            } else {
                s.name == name
            };
            s.kind == SlotKind::Human
                && named
                && g.ips.get(i) == Some(&Some(ip))
                && !(h.peers.iter()).any(|p| p.active() && p.player == Some(i as PlayerId))
        }) else {
            return false;
        };
        // Whoever held it before (a lost connection) is let go quietly.
        for p in &mut h.peers {
            if p.player == Some(player as PlayerId) && !p.gone {
                p.gone = true;
                p.lost_at = None;
                p.slot = None;
                p.drop_at = Some(now);
            }
        }
        let peer = &mut h.peers[index];
        peer.slot = g.slots.get(player).copied();
        peer.player = Some(player as PlayerId);
        peer.name.clone_from(&g.start.players[player].name);
        peer.drop_at = None;
        peer.loaded = true;
        peer.link.send(&g.start_msg(map_hash, player as PlayerId));
        for (turn, commands) in g.history.iter().enumerate() {
            peer.link.send(&Msg::Bundle {
                turn: turn as u32,
                commands: commands.clone(),
            });
        }
        let name = peer.name.clone();
        self.tell_everyone(format!("{name} rejoined and is catching up."));
        self.update_wait(now);
        // Still waiting for someone else: say so, as the others were told.
        if let Role::Host(h) = &mut self.role
            && let Some(g) = &h.game
            && !g.waiting_for.is_empty()
        {
            let seconds = reconnect_left(&h.peers, now).as_secs() as u16;
            let who = g.waiting_for.clone();
            h.peers[index].link.send(&Msg::Wait { who, seconds });
        }
        true
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
        self.tell_everyone(text);
    }

    /// A notice for every player and this machine.
    fn tell_everyone(&mut self, text: String) {
        if let Role::Host(h) = &mut self.role {
            for p in h.peers.iter_mut().filter(|p| !p.gone) {
                p.link.send(&Msg::Notice { text: text.clone() });
            }
        }
        self.match_events.push(MatchEvent::Notice(text));
    }

    /// Pause the match while anyone is reconnecting, and carry on (from
    /// now, without rushing) once nobody is; tell the players either way.
    fn update_wait(&mut self, now: Instant) {
        let length = self.turn;
        let Role::Host(h) = &mut self.role else {
            return;
        };
        let Some(g) = &mut h.game else { return };
        let lost: Vec<&Peer> = h
            .peers
            .iter()
            .filter(|p| p.lost_at.is_some() && !p.gone)
            .collect();
        let who = lost
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if who == g.waiting_for {
            return;
        }
        let left = reconnect_left(&h.peers, now);
        if who.is_empty() && g.go_at.is_some() {
            // Turns restart from now: none piled up during the pause.
            g.go_at = now.checked_sub(length * g.handed);
        }
        g.waiting_for.clone_from(&who);
        for p in h.peers.iter_mut().filter(|p| !p.gone) {
            p.link.send(&Msg::Wait {
                who: who.clone(),
                seconds: left.as_secs() as u16,
            });
        }
    }

    fn check_silence(&mut self, now: Instant) {
        match &mut self.role {
            Role::Host(h) => {
                h.peers
                    .retain(|p| p.drop_at.is_none_or(|t| now < t) || (p.slot.is_some() && !p.gone));
                // Once the match is under way, "Pause and wait" holds it for
                // a player who goes quiet instead of playing on without them.
                let wait = h.lobby.settings.on_drop == OnDrop::Wait
                    && h.game.as_ref().is_some_and(|g| g.go_at.is_some());
                let mut silent = Vec::new();
                let mut lost = Vec::new();
                let mut back = Vec::new();
                for (i, p) in h.peers.iter_mut().enumerate() {
                    if p.gone || p.slot.is_none() {
                        continue;
                    }
                    let quiet = now - p.link.last_heard > PLAYER_TIMEOUT;
                    if !(wait && p.player.is_some()) {
                        if quiet {
                            silent.push(i);
                        }
                    } else if let Some(t) = p.lost_at {
                        if !quiet {
                            p.lost_at = None;
                            back.push(i);
                        } else if now - t > RECONNECT_WAIT {
                            silent.push(i);
                        }
                    } else if quiet {
                        p.lost_at = Some(now);
                        lost.push(i);
                    }
                }
                let late: Vec<usize> = match &h.game {
                    Some(g) if g.go_at.is_none() && now - g.started_at > LOAD_TIMEOUT => {
                        (0..h.peers.len())
                            .filter(|&i| h.peers[i].player.is_some() && !h.peers[i].loaded)
                            .collect()
                    }
                    _ => Vec::new(),
                };
                for (list, what) in [
                    (lost, "lost connection. Waiting for them to come back."),
                    (back, "is back."),
                ] {
                    for i in list {
                        let text = format!("{} {what}", h_name(&self.role, i));
                        self.tell_everyone(text);
                    }
                }
                for i in silent {
                    let text = format!("{} lost connection.", h_name(&self.role, i));
                    self.peer_gone(i, text, now);
                }
                for i in late {
                    let text = format!("{} took too long to load.", h_name(&self.role, i));
                    self.peer_gone(i, text, now);
                }
                self.update_wait(now);
                self.sync_lobby();
                if let Role::Host(h) = &mut self.role {
                    start_turns(h, now);
                }
            }
            Role::Client(c) => {
                if now - c.link.last_heard > client_timeout(c) {
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
                p.link.send(&Msg::Desync { tick });
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
            let game = DiscoveredGame {
                host: b.host,
                map: b.map,
                players: b.players,
                max_players: b.max_players,
                version: b.version,
                locked: b.locked,
                playing: b.playing,
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

/// A Hi or KeyReply packet: the header and a public key.
fn key_packet(kind: Kind, key: &[u8; 32]) -> Vec<u8> {
    let mut w = Writer::new();
    header(&mut w, kind);
    w.buf.extend_from_slice(key);
    w.buf
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
    for turn in 0..g.delay {
        close_turn(h, turn);
    }
}

impl Host {
    /// Whether `ip` has used up its wrong passwords for now: its guesses go
    /// unanswered, and it gets no key exchange to guess with.
    fn out_of_guesses(&mut self, ip: IpAddr, now: Instant) -> bool {
        self.guesses.retain(|&(_, at)| now < at + GUESS_WINDOW);
        self.guesses.iter().filter(|&&(g, _)| g == ip).count() >= MAX_GUESSES
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
    g.history.push(commands);
    g.next_close = turn + 1;
}

/// Look at the round trips again and move the input delay a turn toward
/// what the slowest player needs (docs/11-networking.md, "Input delay").
fn adapt_delay(h: &mut Host, length: Duration) {
    let Some(g) = &mut h.game else { return };
    let worst = h
        .peers
        .iter()
        .filter(|p| p.player.is_some() && p.active())
        .filter_map(|p| Some(p.link.rtt()? / 2 + p.link.jitter() * 2))
        .max();
    let Some(worst) = worst else { return };
    let wanted = ((worst + DELAY_MARGIN)
        .as_nanos()
        .div_ceil(length.as_nanos()) as u32)
        .clamp(MIN_DELAY, MAX_DELAY);
    let before = g.delay;
    if wanted > g.delay {
        g.delay += 1;
        g.lower_votes = 0;
    } else if wanted < g.delay {
        g.lower_votes += 1;
        if g.lower_votes >= DELAY_LOWER_AFTER {
            g.delay -= 1;
            g.lower_votes = 0;
        }
    } else {
        g.lower_votes = 0;
    }
    if g.delay != before {
        for p in h.peers.iter_mut().filter(|p| !p.gone) {
            p.link.send(&Msg::Delay {
                turns: g.delay as u8,
            });
        }
    }
}

impl HostGame {
    /// The match's start as player `you` is told it, with the input delay
    /// as it is now.
    fn start_msg(&self, map_hash: u64, you: PlayerId) -> Msg {
        Msg::Start {
            seed: self.start.seed.unwrap_or_default(),
            map_hash,
            settings: self.start.settings.clone(),
            players: self.start.players.clone(),
            you,
            input_delay: self.delay as u8,
        }
    }
}

/// How long the match will wait for the first of the lost players.
fn reconnect_left(peers: &[Peer], now: Instant) -> Duration {
    peers
        .iter()
        .filter(|p| !p.gone)
        .filter_map(|p| p.lost_at)
        .map(|t| RECONNECT_WAIT.saturating_sub(now - t))
        .min()
        .unwrap_or_default()
}

fn host_turn(h: &mut Host, now: Instant, length: Duration) -> Option<Vec<PlayerCommand>> {
    let g = h.game.as_mut()?;
    if !g.waiting_for.is_empty() {
        return None;
    }
    let go = g.go_at?;
    let due = ((now - go).as_nanos() / length.as_nanos()) as u32 + 1;
    if g.handed >= due {
        return None;
    }
    if due - g.handed > MAX_HOST_LAG {
        // A long hitch: carry on from now rather than rush everyone.
        g.go_at = now.checked_sub(length * g.handed);
    }
    g.handed += 1;
    if g.handed.is_multiple_of(DELAY_EVERY) {
        adapt_delay(h, length);
    }
    // Close turns up to the delay ahead. When the delay has just shortened
    // none closes this turn; when it has lengthened an extra (empty) one
    // does.
    loop {
        let g = h.game.as_ref()?;
        if g.next_close >= g.handed + g.delay {
            break;
        }
        close_turn(h, g.next_close);
    }
    let g = h.game.as_ref()?;
    g.history.get(g.handed as usize - 1).cloned()
}

/// How long a joiner waits on a silent host before giving up.
fn client_timeout(c: &Client) -> Duration {
    match &c.game {
        Some(g) if g.on_drop == OnDrop::Wait => RECONNECT_WAIT,
        _ => HOST_TIMEOUT,
    }
}

/// What `link` should send now: while it has a long backlog (a player
/// catching up on a whole match, or the hashes of one) and the other side
/// is answering, a burst once a round trip; else a packet when there is
/// news or the keepalive is due.
fn due_packets(link: &mut Link, now: Instant, all: bool, keepalive: Duration) -> Vec<Vec<u8>> {
    let answering = now - link.last_heard < Duration::from_secs(1);
    if answering && link.backlog() > CATCH_UP_BACKLOG && !all {
        let round = link.rtt().unwrap_or(BURST_GAP).max(BURST_GAP);
        if now - link.last_sent >= round {
            link.packets(now, CATCH_UP_BURST)
        } else {
            Vec::new()
        }
    } else if all || link.fresh || now - link.last_sent >= keepalive {
        vec![link.packet(now)]
    } else {
        Vec::new()
    }
}

impl ClientGame {
    /// More turns waiting than the input delay accounts for.
    fn behind(&self) -> bool {
        self.ready.len() as u32 > self.delay + 1
    }
}

fn client_turn(g: &mut ClientGame, now: Instant, length: Duration) -> Option<Vec<PlayerCommand>> {
    if g.ready.is_empty() {
        return None;
    }
    let due = *g.due.get_or_insert(now);
    // Behind: catch up a turn at a time.
    if now < due && !g.behind() {
        return None;
    }
    // After a stall, don't rush the turns that piled up on the clock.
    let base = if now > due + length * 2 { now } else { due };
    g.due = Some(base + length);
    g.next_turn += 1;
    g.ready.pop_front()
}

impl Network for Lan {
    fn host(
        &mut self,
        me: &Profile,
        settings: LobbySettings,
        password: &str,
    ) -> Result<(), NetError> {
        self.leave();
        let socket = Lan::open(Lan::bind_host()?)?;
        let port = socket.local_addr().map_or(DEFAULT_PORT, |a| a.port());
        let invite = joincode::invite(port, joincode::tailscale_ip(), joincode::local_ip());
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
            army: Vec::new(),
        };
        self.socket = Some(socket);
        self.role = Role::Host(Box::new(Host {
            password: password.trim().to_string(),
            invite,
            guesses: Vec::new(),
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

    fn invite(&self) -> Option<&str> {
        match &self.role {
            Role::Host(h) => h.invite.as_deref(),
            _ => None,
        }
    }

    fn join(&mut self, me: &Profile, target: &str, password: &str) -> Result<(), NetError> {
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
        let keys = KeyPair::new()
            .ok_or_else(|| NetError::Unavailable("No randomness for a secure link".into()))?;
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
            keys,
            password: password.trim().to_string(),
            hi_sent: None,
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
                // A player rejoining a match in progress gets its start.
                Role::Client(c) if c.lobby.is_some() || c.game.is_some() => return Ok(()),
                Role::Client(_) if now - start > JOIN_TIMEOUT => {
                    self.leave();
                    return Err(NetError::Refused(format!(
                        "No answer from {addr}. Is a game hosted there, on the same network and build?"
                    )));
                }
                Role::Client(_) => std::thread::sleep(Duration::from_millis(5)),
                _ => {
                    let why = self.events.iter().rev().find_map(|e| match e {
                        NetEvent::Left(why) => Some(why.clone()),
                        NetEvent::Started(_) => None,
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
        let mut g = HostGame {
            go_at: None,
            started_at: Instant::now(),
            host_loaded: false,
            handed: 0,
            next_close: 0,
            delay: INPUT_DELAY,
            lower_votes: 0,
            waiting_for: String::new(),
            history: Vec::new(),
            start: start.clone(),
            slots: (0..MAX_PLAYERS)
                .filter(|&i| h.lobby.slots[i].occupied())
                .collect(),
            ips: vec![None; start.players.len()],
            inbox: VecDeque::new(),
            hashes: VecDeque::new(),
            theirs: Vec::new(),
            desynced: Vec::new(),
        };
        for p in h.peers.iter_mut().filter(|p| !p.gone) {
            let Some(slot) = p.slot else { continue };
            let you = g.slots.iter().position(|&s| s == slot).unwrap_or(0) as PlayerId;
            p.player = Some(you);
            if let Some(ip) = g.ips.get_mut(usize::from(you)) {
                *ip = Some(p.addr.ip());
            }
            p.link.send(&g.start_msg(map_hash, you));
        }
        h.game = Some(g);
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

/// `ip:port`, a join code, or an IP or host name on the default port.
fn parse_address(target: &str) -> Result<SocketAddr, NetError> {
    if let Ok(a) = target.parse::<SocketAddr>() {
        return Ok(a);
    }
    if let Some(a) = joincode::decode(target) {
        return Ok(a);
    }
    if let Ok(ip) = target.parse::<IpAddr>() {
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
        .and_then(|mut a| a.find(std::net::SocketAddr::is_ipv4))
        .ok_or_else(|| {
            NetError::BadAddress(format!(
                "\"{target}\" is not an address or join code. Type the code the host's lobby shows, or their IP, like 192.168.1.20."
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
