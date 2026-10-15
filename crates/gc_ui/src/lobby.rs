//! The lobby model and the [`Network`] trait the screens talk to.
//!
//! This is the contract between the menus and the netcode. `gc_net::Lan`
//! implements [`Network`] over UDP; [`LocalNetwork`] hosts a practice lobby
//! on this machine, which keeps the screens testable without a network.
//! Nothing here is hashed or sent by the simulation: it is setup data only.

use std::fmt;

use serde::{Deserialize, Serialize};

pub const MAX_PLAYERS: usize = 4;

/// Player colours, matching the team colours in the game.
pub const PLAYER_COLORS: [(&str, [u8; 3]); 4] = [
    ("Red", [178, 38, 30]),
    ("Violet", [104, 52, 140]),
    ("Cyan", [52, 160, 186]),
    ("Bone", [214, 204, 170]),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Faction {
    EmberClans,
    Brood,
    Choir,
    CarrionConcord,
}

impl Faction {
    pub const ALL: [Faction; 4] = [
        Faction::EmberClans,
        Faction::Brood,
        Faction::Choir,
        Faction::CarrionConcord,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Faction::EmberClans => "Ember Clans",
            Faction::Brood => "Brood",
            Faction::Choir => "Choir",
            Faction::CarrionConcord => "Carrion Concord",
        }
    }

    /// How hard the faction is to play, as docs/08-ui-ux.md rates it.
    pub fn difficulty(self) -> &'static str {
        match self {
            Faction::EmberClans => "easy",
            Faction::Brood => "medium",
            Faction::Choir => "hard",
            Faction::CarrionConcord => "medium-hard",
        }
    }

    pub fn index(self) -> usize {
        Faction::ALL.iter().position(|&f| f == self).unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameMode {
    FieldBattle,
    Conquest,
}

impl GameMode {
    pub const ALL: [GameMode; 2] = [GameMode::FieldBattle, GameMode::Conquest];

    pub fn name(self) -> &'static str {
        match self {
            GameMode::FieldBattle => "Field Battle",
            GameMode::Conquest => "Conquest",
        }
    }

    /// Conquest arrives with M3.
    pub fn playable(self) -> bool {
        self == GameMode::FieldBattle
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tempo {
    Normal,
    Fast,
}

impl Tempo {
    pub const ALL: [Tempo; 2] = [Tempo::Normal, Tempo::Fast];

    pub fn name(self) -> &'static str {
        match self {
            Tempo::Normal => "Normal",
            Tempo::Fast => "Fast",
        }
    }
}

/// How well an AI player plays (docs/18-ai.md, "Difficulty"), easiest
/// first. A slot's `ai_level` indexes this.
pub const AI_LEVELS: [&str; 3] = ["Recruit", "Veteran", "General"];
/// The level a newly added AI starts at.
pub const DEFAULT_AI_LEVEL: u8 = 1;

/// Maps the lobby offers. Only the placeholder map exists so far.
pub const MAPS: [&str; 1] = ["Proving Ground"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LobbySettings {
    pub map: usize,
    pub mode: GameMode,
    pub tempo: Tempo,
}

impl Default for LobbySettings {
    fn default() -> Self {
        LobbySettings {
            map: 0,
            mode: GameMode::FieldBattle,
            tempo: Tempo::Normal,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Open,
    Human,
    Ai,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub kind: SlotKind,
    pub name: String,
    pub faction: Faction,
    /// Index into [`PLAYER_COLORS`].
    pub color: u8,
    /// Players with the same team number are allies.
    pub team: u8,
    pub ready: bool,
    /// Build version of this player's game, for the version gate.
    pub version: String,
    /// For an AI player: its difficulty, an index into [`AI_LEVELS`].
    pub ai_level: u8,
}

impl Slot {
    pub fn open(index: usize) -> Slot {
        Slot {
            kind: SlotKind::Open,
            name: String::new(),
            faction: Faction::ALL[index % 4],
            color: index as u8 % 4,
            team: index as u8,
            ready: false,
            version: String::new(),
            ai_level: DEFAULT_AI_LEVEL,
        }
    }

    pub fn occupied(&self) -> bool {
        self.kind != SlotKind::Open
    }
}

/// What the lobby screen shows and edits.
#[derive(Clone, Debug)]
pub struct LobbyState {
    pub slots: [Slot; MAX_PLAYERS],
    pub settings: LobbySettings,
    pub local_slot: usize,
    pub is_host: bool,
    pub host_version: String,
}

impl LobbyState {
    /// Humans whose build differs from the host's, as `(name, version)`.
    pub fn version_mismatches(&self) -> Vec<(&str, &str)> {
        self.slots
            .iter()
            .filter(|s| s.kind == SlotKind::Human && s.version != self.host_version)
            .map(|s| (s.name.as_str(), s.version.as_str()))
            .collect()
    }

    /// The setup a match started from this lobby runs with.
    pub fn match_start(&self, seed: Option<u64>) -> MatchStart {
        let mut players = Vec::new();
        let mut local_player = 0;
        for (i, s) in self.slots.iter().enumerate() {
            if s.occupied() {
                if i == self.local_slot {
                    local_player = players.len();
                }
                players.push(s.clone());
            }
        }
        MatchStart {
            settings: self.settings.clone(),
            players,
            local_player,
            seed,
        }
    }

    /// Why the match can't start yet, or `None` if it can.
    pub fn start_blocker(&self) -> Option<String> {
        if !self.settings.mode.playable() {
            return Some(format!("{} is not playable yet", self.settings.mode.name()));
        }
        if self.slots.iter().filter(|s| s.occupied()).count() < 2 {
            return Some("Add at least one more player".into());
        }
        if let Some((name, version)) = self.version_mismatches().first() {
            return Some(format!(
                "{name} is on {version}, host is on {}",
                self.host_version
            ));
        }
        let waiting = self
            .slots
            .iter()
            .enumerate()
            // The host (slot 0) is ready by pressing Start.
            .filter(|(i, s)| s.kind == SlotKind::Human && *i != 0 && !s.ready)
            .count();
        (waiting > 0).then(|| format!("Waiting for {waiting} player(s) to get ready"))
    }
}

/// Who you are, from the settings screen.
#[derive(Clone, Debug)]
pub struct Profile {
    pub name: String,
    pub color: u8,
    pub faction: Faction,
    pub version: String,
}

/// A lobby seen on the local network.
#[derive(Clone, Debug)]
pub struct DiscoveredGame {
    pub host: String,
    pub map: String,
    pub players: u8,
    pub max_players: u8,
    pub version: String,
    /// What to hand to [`Network::join`].
    pub address: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotEdit {
    Faction(Faction),
    Color(u8),
    Team(u8),
    Ready(bool),
    /// Host only: open a slot, or fill it with an AI player.
    Kind(SlotKind),
    /// Host only: an AI player's difficulty (index into [`AI_LEVELS`]).
    AiLevel(u8),
}

/// The setup a started match runs with.
#[derive(Clone, Debug)]
pub struct MatchStart {
    pub settings: LobbySettings,
    /// Occupied slots only, in slot order.
    pub players: Vec<Slot>,
    /// Position of this machine's player in `players`.
    pub local_player: usize,
    /// Set when the match is played over the network: the host's seed,
    /// which every machine must use. `None` for a match on this machine
    /// alone (a practice lobby, or one where every other player is an AI).
    pub seed: Option<u64>,
}

#[derive(Clone, Debug)]
pub enum NetEvent {
    /// The host started the match.
    Started(MatchStart),
    /// The lobby is gone (host left, connection lost). Carries a message.
    Left(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetError {
    /// Not built yet; the text says what to do instead.
    Unavailable(String),
    /// The address or code could not be understood.
    BadAddress(String),
    /// The other side said no (full, wrong version, wrong password).
    Refused(String),
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::Unavailable(s) | NetError::BadAddress(s) | NetError::Refused(s) => {
                f.write_str(s)
            }
        }
    }
}

/// What the menus need from the network. All calls return at once; slow
/// work (connecting, discovery) happens behind it and shows up through
/// [`Network::lobby`], [`Network::discovered`] and [`Network::poll`].
pub trait Network {
    /// Open a lobby on this machine with you in slot 0.
    fn host(&mut self, me: &Profile, settings: LobbySettings) -> Result<(), NetError>;
    /// Lobbies heard on the local network, newest state.
    fn discovered(&mut self) -> &[DiscoveredGame];
    /// Join by a discovered game's `address`, or whatever the player typed
    /// (`ip:port` or a join code).
    fn join(&mut self, me: &Profile, target: &str) -> Result<(), NetError>;
    /// The lobby we are in, if any.
    fn lobby(&self) -> Option<&LobbyState>;
    /// Change a slot: your own, or any AI slot if you host.
    fn edit_slot(&mut self, slot: usize, edit: SlotEdit);
    /// Host only: map, mode and tempo.
    fn edit_settings(&mut self, settings: LobbySettings);
    /// Host only: start the match for everyone.
    fn start_match(&mut self) -> Result<MatchStart, NetError>;
    /// Leave (or close) the lobby.
    fn leave(&mut self);
    /// Events since the last call.
    fn poll(&mut self) -> Vec<NetEvent>;
}

/// Change `slot` on behalf of the player in slot `editor` (`host` if that
/// is the host), as [`Network::edit_slot`] allows: your own slot, or as the
/// host any AI slot and which slots are open or AI. Edits not allowed are
/// ignored. The netcode runs clients' edits through this on the host.
pub fn apply_edit(lobby: &mut LobbyState, editor: usize, host: bool, slot: usize, edit: SlotEdit) {
    if slot >= MAX_PLAYERS {
        return;
    }
    let local = slot == editor;
    let is_ai = lobby.slots[slot].kind == SlotKind::Ai;
    let allowed = match &edit {
        SlotEdit::Kind(_) => host && !local,
        SlotEdit::AiLevel(_) => host && is_ai,
        _ => local || (host && is_ai),
    };
    if !allowed {
        return;
    }
    match edit {
        SlotEdit::Faction(f) => lobby.slots[slot].faction = f,
        SlotEdit::Color(c) => {
            let c = c % 4;
            // Colours are unique: swapping with whoever has it.
            if let Some(other) = (0..MAX_PLAYERS)
                .find(|&i| i != slot && lobby.slots[i].occupied() && lobby.slots[i].color == c)
            {
                lobby.slots[other].color = lobby.slots[slot].color;
            }
            lobby.slots[slot].color = c;
        }
        SlotEdit::Team(t) => lobby.slots[slot].team = t % MAX_PLAYERS as u8,
        SlotEdit::Ready(r) => lobby.slots[slot].ready = r,
        SlotEdit::AiLevel(level) => {
            lobby.slots[slot].ai_level = level % AI_LEVELS.len() as u8;
        }
        SlotEdit::Kind(kind) => match kind {
            SlotKind::Ai => {
                let version = lobby.host_version.clone();
                let s = &mut lobby.slots[slot];
                s.kind = SlotKind::Ai;
                s.name = format!("AI {}", slot + 1);
                s.ready = true;
                s.version = version;
            }
            SlotKind::Open => lobby.slots[slot] = Slot::open(slot),
            SlotKind::Human => {}
        },
    }
    // An AI joining must not share a colour with someone already there.
    let taken: Vec<u8> = (0..MAX_PLAYERS)
        .filter(|&i| i != slot && lobby.slots[i].occupied())
        .map(|i| lobby.slots[i].color)
        .collect();
    if lobby.slots[slot].occupied()
        && taken.contains(&lobby.slots[slot].color)
        && let Some(free) = (0..4u8).find(|c| !taken.contains(c))
    {
        lobby.slots[slot].color = free;
    }
}

/// A practice lobby on this machine: hosting works, AI players fill slots,
/// and joining other machines says it is not built yet.
#[derive(Default)]
pub struct LocalNetwork {
    lobby: Option<LobbyState>,
}

pub const JOIN_UNAVAILABLE: &str = "Joining friends needs networking, which arrives in milestone 2. Host a practice game with AI for now.";

impl LocalNetwork {
    pub fn new() -> LocalNetwork {
        LocalNetwork::default()
    }
}

impl Network for LocalNetwork {
    fn host(&mut self, me: &Profile, settings: LobbySettings) -> Result<(), NetError> {
        let mut slots = [Slot::open(0), Slot::open(1), Slot::open(2), Slot::open(3)];
        slots[0] = Slot {
            kind: SlotKind::Human,
            name: me.name.clone(),
            faction: me.faction,
            color: me.color % 4,
            team: 0,
            ready: true,
            version: me.version.clone(),
            ai_level: DEFAULT_AI_LEVEL,
        };
        self.lobby = Some(LobbyState {
            slots,
            settings,
            local_slot: 0,
            is_host: true,
            host_version: me.version.clone(),
        });
        Ok(())
    }

    fn discovered(&mut self) -> &[DiscoveredGame] {
        &[]
    }

    fn join(&mut self, _me: &Profile, target: &str) -> Result<(), NetError> {
        if target.trim().is_empty() {
            return Err(NetError::BadAddress("Type an address or join code".into()));
        }
        Err(NetError::Unavailable(JOIN_UNAVAILABLE.into()))
    }

    fn lobby(&self) -> Option<&LobbyState> {
        self.lobby.as_ref()
    }

    fn edit_slot(&mut self, slot: usize, edit: SlotEdit) {
        if let Some(lobby) = self.lobby.as_mut() {
            let (me, host) = (lobby.local_slot, lobby.is_host);
            apply_edit(lobby, me, host, slot, edit);
        }
    }

    fn edit_settings(&mut self, settings: LobbySettings) {
        if let Some(lobby) = self.lobby.as_mut().filter(|l| l.is_host) {
            lobby.settings = settings;
        }
    }

    fn start_match(&mut self) -> Result<MatchStart, NetError> {
        let lobby = self
            .lobby
            .as_ref()
            .ok_or_else(|| NetError::Refused("No lobby".into()))?;
        if let Some(why) = lobby.start_blocker() {
            return Err(NetError::Refused(why));
        }
        Ok(lobby.match_start(None))
    }

    fn leave(&mut self) {
        self.lobby = None;
    }

    fn poll(&mut self) -> Vec<NetEvent> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> Profile {
        Profile {
            name: "Mario".into(),
            color: 0,
            faction: Faction::EmberClans,
            version: "0.1.0".into(),
        }
    }

    fn hosted() -> LocalNetwork {
        let mut n = LocalNetwork::new();
        n.host(&me(), LobbySettings::default()).unwrap();
        n
    }

    #[test]
    fn hosting_puts_you_in_slot_zero() {
        let n = hosted();
        let l = n.lobby().unwrap();
        assert!(l.is_host);
        assert_eq!(l.slots[0].name, "Mario");
        assert!(l.slots[1..].iter().all(|s| s.kind == SlotKind::Open));
    }

    #[test]
    fn alone_you_cannot_start() {
        let mut n = hosted();
        assert_eq!(
            n.lobby().unwrap().start_blocker().as_deref(),
            Some("Add at least one more player")
        );
        assert!(n.start_match().is_err());
    }

    #[test]
    fn host_adds_an_ai_and_starts() {
        let mut n = hosted();
        n.edit_slot(1, SlotEdit::Kind(SlotKind::Ai));
        let start = n.start_match().unwrap();
        assert_eq!(start.players.len(), 2);
        assert_eq!(start.local_player, 0);
        assert_eq!(start.players[1].name, "AI 2");
    }

    #[test]
    fn the_host_sets_an_ais_level() {
        let mut n = hosted();
        n.edit_slot(1, SlotEdit::Kind(SlotKind::Ai));
        assert_eq!(n.lobby().unwrap().slots[1].ai_level, DEFAULT_AI_LEVEL);
        n.edit_slot(1, SlotEdit::AiLevel(2));
        assert_eq!(n.lobby().unwrap().slots[1].ai_level, 2);
        // Humans have no level to set.
        n.edit_slot(0, SlotEdit::AiLevel(2));
        assert_eq!(n.lobby().unwrap().slots[0].ai_level, DEFAULT_AI_LEVEL);
        let start = n.start_match().unwrap();
        assert_eq!(AI_LEVELS[start.players[1].ai_level as usize], "General");
    }

    #[test]
    fn conquest_cannot_start_yet() {
        let mut n = hosted();
        n.edit_slot(1, SlotEdit::Kind(SlotKind::Ai));
        n.edit_settings(LobbySettings {
            mode: GameMode::Conquest,
            ..Default::default()
        });
        let why = n.lobby().unwrap().start_blocker().unwrap();
        assert!(why.contains("Conquest"));
    }

    #[test]
    fn colours_stay_unique() {
        let mut n = hosted();
        n.edit_slot(1, SlotEdit::Kind(SlotKind::Ai));
        n.edit_slot(0, SlotEdit::Color(1));
        let l = n.lobby().unwrap();
        assert_eq!(l.slots[0].color, 1);
        assert_ne!(l.slots[1].color, 1);
        // An AI added onto a taken colour gets a free one.
        n.edit_slot(2, SlotEdit::Kind(SlotKind::Ai));
        let l = n.lobby().unwrap();
        let mut colours: Vec<u8> = l
            .slots
            .iter()
            .filter(|s| s.occupied())
            .map(|s| s.color)
            .collect();
        colours.sort();
        colours.dedup();
        assert_eq!(colours.len(), 3);
    }

    #[test]
    fn only_the_host_changes_slot_kinds_and_not_your_own() {
        let mut n = hosted();
        n.edit_slot(0, SlotEdit::Kind(SlotKind::Open));
        assert_eq!(n.lobby().unwrap().slots[0].kind, SlotKind::Human);
    }

    #[test]
    fn joining_says_it_is_not_built() {
        let mut n = LocalNetwork::new();
        assert_eq!(
            n.join(&me(), "").unwrap_err(),
            NetError::BadAddress("Type an address or join code".into())
        );
        let e = n.join(&me(), "192.168.1.5:7777").unwrap_err();
        assert!(matches!(e, NetError::Unavailable(_)));
        assert!(n.lobby().is_none());
    }

    #[test]
    fn version_mismatch_blocks_with_a_clear_message() {
        let mut n = hosted();
        let l = n.lobby.as_mut().unwrap();
        l.slots[1] = Slot {
            kind: SlotKind::Human,
            name: "Alex".into(),
            version: "0.0.9".into(),
            ready: true,
            ..Slot::open(1)
        };
        let why = n.lobby().unwrap().start_blocker().unwrap();
        assert_eq!(why, "Alex is on 0.0.9, host is on 0.1.0");
    }

    #[test]
    fn unready_humans_block_the_start() {
        let mut n = hosted();
        let l = n.lobby.as_mut().unwrap();
        l.slots[1] = Slot {
            kind: SlotKind::Human,
            name: "Alex".into(),
            version: "0.1.0".into(),
            ready: false,
            ..Slot::open(1)
        };
        assert!(
            n.lobby()
                .unwrap()
                .start_blocker()
                .unwrap()
                .contains("Waiting")
        );
        n.lobby.as_mut().unwrap().slots[1].ready = true;
        assert!(n.start_match().is_ok());
    }

    #[test]
    fn a_joiner_who_is_not_ready_holds_up_the_start_on_their_screen_too() {
        let mut n = hosted();
        let l = n.lobby.as_mut().unwrap();
        l.slots[1] = Slot {
            kind: SlotKind::Human,
            name: "Alex".into(),
            version: "0.1.0".into(),
            ..Slot::open(1)
        };
        // Alex's own view of the same lobby.
        l.local_slot = 1;
        l.is_host = false;
        assert!(l.start_blocker().unwrap().contains("Waiting for 1"));
    }

    #[test]
    fn leaving_closes_the_lobby() {
        let mut n = hosted();
        n.leave();
        assert!(n.lobby().is_none());
    }
}
