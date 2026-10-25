//! The lobby model and the [`Network`] trait the screens talk to.
//!
//! This is the contract between the menus and the netcode. `gc_net::Lan`
//! implements [`Network`] over UDP; [`LocalNetwork`] hosts a practice lobby
//! on this machine, which keeps the screens testable without a network.
//! Nothing here is hashed or sent by the simulation: it is setup data only.

use std::fmt;

use serde::{Deserialize, Serialize};

pub const MAX_PLAYERS: usize = 6;

/// Player colours, matching the team colours in the game.
/// One for each lobby slot.
pub const PLAYER_COLORS: [(&str, [u8; 3]); MAX_PLAYERS] = [
    ("Red", [178, 38, 30]),
    ("Violet", [104, 52, 140]),
    ("Cyan", [52, 160, 186]),
    ("Bone", [214, 204, 170]),
    ("Amber", [226, 152, 38]),
    ("Jade", [56, 150, 96]),
];

/// How many colours there are; colour indices wrap round [`PLAYER_COLORS`].
pub const COLORS: u8 = PLAYER_COLORS.len() as u8;

pub use gc_data::Faction;

/// How hard a faction is to play, as docs/08-ui-ux.md rates it.
pub fn faction_difficulty(f: Faction) -> &'static str {
    match f {
        Faction::EmberClans => "easy",
        Faction::Brood => "medium",
        Faction::Choir => "hard",
        Faction::CarrionConcord => "medium-hard",
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
}

/// How fast a Conquest match runs (docs/04-economy.md, "Tempo").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tempo {
    Standard,
    Swift,
    Epic,
}

impl Tempo {
    pub const ALL: [Tempo; 3] = [Tempo::Standard, Tempo::Swift, Tempo::Epic];

    pub fn name(self) -> &'static str {
        match self {
            Tempo::Standard => "Standard",
            Tempo::Swift => "Swift",
            Tempo::Epic => "Epic",
        }
    }

    /// Gathering at this percent of the Standard rate.
    pub fn gather_percent(self) -> u32 {
        match self {
            Tempo::Standard => 100,
            Tempo::Swift => 150,
            Tempo::Epic => 80,
        }
    }

    /// Building, upgrading and training take this percent of their
    /// Standard time.
    pub fn build_percent(self) -> u32 {
        match self {
            Tempo::Standard => 100,
            Tempo::Swift => 75,
            Tempo::Epic => 120,
        }
    }
}

/// Which victories count in a Conquest match (docs/06-game-modes.md,
/// "Victory conditions").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Victory {
    /// Annihilation and the Titan's Heart both win.
    Both,
    /// Only destroying an opponent's main buildings wins.
    Annihilation,
    /// Only holding the Titan's Heart wins.
    Heart,
}

impl Victory {
    pub const ALL: [Victory; 3] = [Victory::Both, Victory::Annihilation, Victory::Heart];

    pub fn name(self) -> &'static str {
        match self {
            Victory::Both => "Both",
            Victory::Annihilation => "Annihilation",
            Victory::Heart => "Titan's Heart",
        }
    }

    /// Whether Annihilation wins the match.
    pub fn annihilation(self) -> bool {
        matches!(self, Victory::Both | Victory::Annihilation)
    }

    /// Whether the Titan's Heart wins the match.
    pub fn heart(self) -> bool {
        matches!(self, Victory::Both | Victory::Heart)
    }
}

/// How well an AI player plays (docs/18-ai.md, "Difficulty"), easiest
/// first. A slot's `ai_level` indexes this.
pub const AI_LEVELS: [&str; 4] = ["Recruit", "Veteran", "General", "Warlord"];
/// The level a newly added AI starts at.
pub const DEFAULT_AI_LEVEL: u8 = 1;

/// How big a Field Battle map is, for its player count
/// ([`field_map_tiles`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapSize {
    /// The old battle maps: a minute and a half's march across.
    Small,
    /// Several minutes' march across, room to manoeuvre and explore.
    Standard,
    /// A step bigger again.
    Large,
}

impl MapSize {
    pub const ALL: [MapSize; 3] = [MapSize::Small, MapSize::Standard, MapSize::Large];

    pub fn name(self) -> &'static str {
        match self {
            MapSize::Small => "Small",
            MapSize::Standard => "Standard",
            MapSize::Large => "Large",
        }
    }
}

/// The light a match starts in, chosen by the host, the same on every
/// screen. Each land then tints it (the Waste's amber haze, the Fen's green
/// mist, the Highland's cold clear air). The simulation's clock starts at
/// its hour (see [`TimeOfDay::hour`]) and the day then passes slowly, so
/// night falls on a long dusk battle and fighting in the dark is harder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeOfDay {
    /// A low warm sun, blue shade, long shadows: the game's look.
    #[default]
    GoldenHour,
    /// A high sun in clear air: short shadows, bright colours.
    Noon,
    /// The sun just above the hills: violet shade, an orange sky.
    Dusk,
    /// Soft grey light and heavy air; few shadows.
    Overcast,
    /// Moonlight: troops see and shoot less far.
    Night,
}

impl TimeOfDay {
    pub const ALL: [TimeOfDay; 5] = [
        TimeOfDay::GoldenHour,
        TimeOfDay::Noon,
        TimeOfDay::Dusk,
        TimeOfDay::Overcast,
        TimeOfDay::Night,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TimeOfDay::GoldenHour => "Golden hour",
            TimeOfDay::Noon => "Noon",
            TimeOfDay::Dusk => "Dusk",
            TimeOfDay::Overcast => "Overcast",
            TimeOfDay::Night => "Night",
        }
    }

    /// The hour and minute the match's clock starts at.
    pub fn hour(self) -> (u32, u32) {
        match self {
            TimeOfDay::GoldenHour => (17, 0),
            TimeOfDay::Noon | TimeOfDay::Overcast => (12, 0),
            TimeOfDay::Dusk => (18, 0),
            TimeOfDay::Night => (23, 0),
        }
    }
}

/// What the weather does in a match, chosen by the host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeatherSetting {
    /// Dry and clear all match.
    #[default]
    Fair,
    /// Spells of rain, fog and clear sky come and go every few minutes.
    Changing,
    /// Rain all match: archers reach a little less far.
    Rain,
    /// Snow all match: everyone marches slower and sees less far.
    Snow,
    /// Fog all match: archers and sight reach less than half as far.
    Fog,
}

impl WeatherSetting {
    pub const ALL: [WeatherSetting; 5] = [
        WeatherSetting::Fair,
        WeatherSetting::Changing,
        WeatherSetting::Rain,
        WeatherSetting::Snow,
        WeatherSetting::Fog,
    ];

    pub fn name(self) -> &'static str {
        match self {
            WeatherSetting::Fair => "Fair",
            WeatherSetting::Changing => "Changing",
            WeatherSetting::Rain => "Rain",
            WeatherSetting::Snow => "Snow",
            WeatherSetting::Fog => "Fog",
        }
    }
}

/// How the year turns in a Conquest match (docs/32-changing-land.md,
/// "Seasons"), chosen by the host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum YearSetting {
    /// Spring all match.
    Off,
    /// Eight minutes a season.
    #[default]
    Short,
    /// Fifteen minutes a season.
    Long,
}

impl YearSetting {
    pub const ALL: [YearSetting; 3] = [YearSetting::Off, YearSetting::Short, YearSetting::Long];

    pub fn name(self) -> &'static str {
        match self {
            YearSetting::Off => "Off",
            YearSetting::Short => "Short",
            YearSetting::Long => "Long",
        }
    }

    /// How long a season lasts, in minutes (`None` when they are off).
    pub fn season_minutes(self) -> Option<u32> {
        match self {
            YearSetting::Off => None,
            YearSetting::Short => Some(8),
            YearSetting::Long => Some(15),
        }
    }
}

/// How many tiles a side a Field Battle map of `size` is for `players`.
/// Foot marches about 3 tiles a second, so a Standard map is about three
/// minutes across for two players and over four for six. Small is the
/// old 256, or 320 for five or six, whose Grand Battle armies don't fit
/// on 256. The same map choice and code make a different map at each
/// size.
pub fn field_map_tiles(size: MapSize, players: usize) -> u32 {
    let step = match players {
        0..=2 => 0,
        3..=4 => 1,
        _ => 2,
    };
    match size {
        MapSize::Small => {
            if players > 4 {
                320
            } else {
                256
            }
        }
        MapSize::Standard => 512 + 128 * step,
        MapSize::Large => 640 + 128 * step,
    }
}

/// Maps the lobby offers: the seeded map, a random map from each recipe,
/// a random recipe, the Classic Wilds maps and grown land (Living Land,
/// mirrored, then with measured fairness), each as wild as the seed picks
/// or as plains, mixed or wild land. The
/// same names in the same order as `gc_map::FIELD_MAPS` (the game checks).
pub const MAPS: [&str; 18] = [
    "Proving Ground",
    "Random: Pass",
    "Random: Ford",
    "Random: Hill",
    "Random: Open",
    "Random: Any",
    "Classic: Random",
    "Classic: Plains",
    "Classic: Mixed",
    "Classic: Wild",
    "Land: Random",
    "Land: Plains",
    "Land: Mixed",
    "Land: Wild",
    "Measured: Random",
    "Measured: Plains",
    "Measured: Mixed",
    "Measured: Wild",
];

/// The map a new lobby starts on: grown land, as wild as the seed picks
/// (`gc_map::GROWN`).
pub const DEFAULT_MAP: usize = 10;

/// The Conquest maps the lobby offers, and how many players each is for:
/// first the shipped maps (the game makes them from the shipped maps of
/// the same names), then the big six-player Wilds maps made from the map
/// seed, as wild as the seed picks or as plains, mixed or wild land.
pub const CONQUEST_MAPS: [&str; 6] = [
    "Ribcage Pass",
    "The Four Teeth",
    "Classic: Random",
    "Classic: Plains",
    "Classic: Mixed",
    "Classic: Wild",
];
pub const CONQUEST_MAP_PLAYERS: [usize; 6] = [2, 4, 6, 6, 6, 6];
/// How many of [`CONQUEST_MAPS`] are shipped maps, which the map seed
/// doesn't change; the rest are made from it.
pub const SHIPPED_CONQUEST_MAPS: usize = 2;

/// The seed Conquest map `choice` is made from: the map seed for a made
/// map, nothing for a shipped one (so a new seed doesn't remake it).
pub fn conquest_map_seed(choice: usize, map_seed: u64) -> u64 {
    if choice < SHIPPED_CONQUEST_MAPS {
        0
    } else {
        map_seed
    }
}

/// The maps a match of `mode` can be played on: [`MAPS`] or
/// [`CONQUEST_MAPS`]. [`LobbySettings::map`] indexes this.
pub fn maps_for(mode: GameMode) -> &'static [&'static str] {
    match mode {
        GameMode::FieldBattle => &MAPS,
        GameMode::Conquest => &CONQUEST_MAPS,
    }
}

/// The map seed a new lobby starts on (the Proving Ground every battle was
/// played on before maps could be picked).
pub const DEFAULT_MAP_SEED: u64 = 0x6772_7562;

/// A new map seed for the host's "New map" button. Each `RandomState` gets
/// fresh random keys from the standard library, so every press differs. It
/// is not secret and the simulation never sees it, which is why gc_ui needs
/// no random number crate.
pub fn fresh_map_seed() -> u64 {
    use std::hash::BuildHasher;
    std::collections::hash_map::RandomState::new().hash_one(0u8)
}

/// A map seed as players share it: all 16 hex digits in four groups,
/// "0000-0000-6772-7562". [`parse_map_code`] reads it back exactly.
pub fn map_code(seed: u64) -> String {
    format!(
        "{:04X}-{:04X}-{:04X}-{:04X}",
        seed >> 48,
        (seed >> 32) & 0xFFFF,
        (seed >> 16) & 0xFFFF,
        seed & 0xFFFF
    )
}

/// The seed a map code stands for. Dashes, spaces and letter case don't
/// matter; anything else, or anything but 16 hex digits, is not a code.
pub fn parse_map_code(text: &str) -> Option<u64> {
    let digits: String = text.chars().filter(|c| !matches!(c, '-' | ' ')).collect();
    if digits.len() != 16 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u64::from_str_radix(&digits, 16).ok()
}

/// How many points each side has to raise its army in a Field Battle
/// (docs/06-game-modes.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget {
    Skirmish,
    Battle,
    GrandBattle,
}

impl Budget {
    pub const ALL: [Budget; 3] = [Budget::Skirmish, Budget::Battle, Budget::GrandBattle];

    pub fn points(self) -> u32 {
        match self {
            Budget::Skirmish => 2_000,
            Budget::Battle => 4_000,
            Budget::GrandBattle => 8_000,
        }
    }
}

/// Most regiments one army can hold, whatever the budget.
pub const MAX_ARMY: usize = 40;

/// A regiment type the army builder offers, from the game's unit table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RosterUnit {
    pub name: String,
    pub faction: Faction,
    pub points: u32,
}

/// What `army` (unit types, indexes into `roster`) costs. Unknown types
/// count nothing.
pub fn army_points(roster: &[RosterUnit], army: &[u16]) -> u32 {
    army.iter()
        .filter_map(|&u| roster.get(u as usize))
        .map(|r| r.points)
        .sum()
}

/// 4000 as "4,000".
pub fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// What a network match does when a player loses the connection
/// (docs/11-networking.md, "Disconnects and rejoining").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnDrop {
    /// Pause everyone for up to 90 seconds while they come back.
    Wait,
    /// Play on at once; their regiments stand and defend.
    CarryOn,
}

impl OnDrop {
    pub const ALL: [OnDrop; 2] = [OnDrop::Wait, OnDrop::CarryOn];

    pub fn name(self) -> &'static str {
        match self {
            OnDrop::Wait => "Wait",
            OnDrop::CarryOn => "Play on",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LobbySettings {
    /// An index into the mode's maps ([`maps_for`]).
    pub map: usize,
    /// The seed the map is made from; the host rerolls it for a new map.
    pub map_seed: u64,
    pub mode: GameMode,
    pub tempo: Tempo,
    /// Conquest only: which victories count.
    pub victory: Victory,
    /// If a player drops out of a network match.
    pub on_drop: OnDrop,
    /// Field Battle: points each side raises its army with.
    pub budget: Budget,
    /// Field Battle: how big the map is.
    pub map_size: MapSize,
    /// Whether champions may duel between the lines before a battle with a
    /// computer player (a chance, not every time).
    pub champions: bool,
    /// The light the match starts in.
    pub time_of_day: TimeOfDay,
    /// What the weather does.
    pub weather: WeatherSetting,
    /// Conquest only: how fast the seasons turn.
    pub seasons: YearSetting,
}

impl Default for LobbySettings {
    fn default() -> Self {
        LobbySettings {
            map: DEFAULT_MAP,
            map_seed: DEFAULT_MAP_SEED,
            mode: GameMode::FieldBattle,
            tempo: Tempo::Standard,
            victory: Victory::Both,
            on_drop: OnDrop::Wait,
            budget: Budget::Battle,
            map_size: MapSize::Standard,
            champions: true,
            time_of_day: TimeOfDay::GoldenHour,
            weather: WeatherSetting::Fair,
            seasons: YearSetting::Short,
        }
    }
}

impl LobbySettings {
    /// The name of the map these settings pick.
    pub fn map_name(&self) -> &'static str {
        maps_for(self.mode).get(self.map).copied().unwrap_or("?")
    }

    /// Change the mode, keeping the map choice in its range.
    /// Switch modes, starting on the new mode's first choice of map: a
    /// Field Battle on [`DEFAULT_MAP`], Conquest on its first map (a map's
    /// place in one mode's list means nothing in the other's).
    pub fn set_mode(&mut self, mode: GameMode) {
        if mode != self.mode {
            self.mode = mode;
            self.map = match mode {
                GameMode::FieldBattle => DEFAULT_MAP,
                GameMode::Conquest => 0,
            };
        }
    }

    /// The most players the picked map has room for.
    pub fn max_players(&self) -> usize {
        match self.mode {
            GameMode::FieldBattle => MAX_PLAYERS,
            GameMode::Conquest => CONQUEST_MAP_PLAYERS.get(self.map).copied().unwrap_or(0),
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
    /// The regiments this player built for a Field Battle, as unit types
    /// in the game's unit table; empty to have one picked for them (the
    /// "Balanced" mix, or an AI's own).
    pub army: Vec<u16>,
}

impl Slot {
    pub fn open(index: usize) -> Slot {
        Slot {
            kind: SlotKind::Open,
            name: String::new(),
            faction: Faction::ALL[index % Faction::ALL.len()],
            color: index as u8 % COLORS,
            team: index as u8,
            ready: false,
            version: String::new(),
            ai_level: DEFAULT_AI_LEVEL,
            army: Vec::new(),
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

/// Whether builds named `a` and `b` can play together. A build's name is
/// "VERSION sim N (COMMIT)": the part before the commit says how it plays
/// (the simulation's rules version), and the commit is only for people,
/// so builds that differ only in it (a docs change, say) match.
pub fn plays_with(a: &str, b: &str) -> bool {
    a.split(" (").next() == b.split(" (").next()
}

impl LobbyState {
    /// Humans whose build can't play with the host's, as `(name, version)`.
    pub fn version_mismatches(&self) -> Vec<(&str, &str)> {
        self.slots
            .iter()
            .filter(|s| s.kind == SlotKind::Human && !plays_with(&s.version, &self.host_version))
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
        let players = self.slots.iter().filter(|s| s.occupied()).count();
        if players < 2 {
            return Some("Add at least one more player".into());
        }
        let most = self.settings.max_players();
        if players > most {
            return Some(format!(
                "{} is for at most {most} players",
                self.settings.map_name()
            ));
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
    /// Joining needs a password.
    pub locked: bool,
    /// The match has started: only a player who was in it can join, to
    /// take their place again.
    pub playing: bool,
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
    /// The regiments of your Field Battle army (see [`Slot::army`]).
    Army(Vec<u16>),
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
    /// Open a lobby on this machine with you in slot 0. A non-empty
    /// `password` must be typed by everyone who joins.
    fn host(
        &mut self,
        me: &Profile,
        settings: LobbySettings,
        password: &str,
    ) -> Result<(), NetError>;
    /// Lobbies heard on the local network, newest state.
    fn discovered(&mut self) -> &[DiscoveredGame];
    /// Join by a discovered game's `address`, or whatever the player typed
    /// (`ip:port` or a join code), with the lobby's password if it has one.
    fn join(&mut self, me: &Profile, target: &str, password: &str) -> Result<(), NetError>;
    /// While hosting: how friends reach this lobby (a join code and the
    /// address), to show in it.
    fn invite(&self) -> Option<&str> {
        None
    }
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
        SlotEdit::Faction(f) => {
            // Another faction's regiments aren't yours to field.
            if lobby.slots[slot].faction != f {
                lobby.slots[slot].army.clear();
            }
            lobby.slots[slot].faction = f;
        }
        SlotEdit::Army(mut army) => {
            army.truncate(MAX_ARMY);
            lobby.slots[slot].army = army;
        }
        SlotEdit::Color(c) => {
            let c = c % COLORS;
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
                // An AI picks its own army.
                s.army.clear();
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
        && let Some(free) = (0..COLORS).find(|c| !taken.contains(c))
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
    fn host(
        &mut self,
        me: &Profile,
        settings: LobbySettings,
        _password: &str,
    ) -> Result<(), NetError> {
        let mut slots = std::array::from_fn(Slot::open);
        slots[0] = Slot {
            kind: SlotKind::Human,
            name: me.name.clone(),
            faction: me.faction,
            color: me.color % COLORS,
            team: 0,
            ready: true,
            version: me.version.clone(),
            ai_level: DEFAULT_AI_LEVEL,
            army: Vec::new(),
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

    fn join(&mut self, _me: &Profile, target: &str, _password: &str) -> Result<(), NetError> {
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
        n.host(&me(), LobbySettings::default(), "").unwrap();
        n
    }

    #[test]
    fn map_codes_round_trip_exactly() {
        for seed in [0, 1, DEFAULT_MAP_SEED, 0xDEAD_BEEF_0123_4567, u64::MAX] {
            let code = map_code(seed);
            assert_eq!(code.len(), 19, "{code}");
            assert_eq!(parse_map_code(&code), Some(seed), "{code}");
        }
        assert_eq!(map_code(DEFAULT_MAP_SEED), "0000-0000-6772-7562");
        assert_eq!(map_code(u64::MAX), "FFFF-FFFF-FFFF-FFFF");
        let mut seed: u64 = 0x1234_5678_9ABC_DEF0;
        for _ in 0..200 {
            seed = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17);
            assert_eq!(parse_map_code(&map_code(seed)), Some(seed));
        }
    }

    #[test]
    fn map_codes_forgive_case_dashes_and_spaces() {
        let want = Some(0xDEAD_BEEF_0123_4567);
        assert_eq!(parse_map_code("DEAD-BEEF-0123-4567"), want);
        assert_eq!(parse_map_code("dead-beef-0123-4567"), want);
        assert_eq!(parse_map_code("deadbeef01234567"), want);
        assert_eq!(parse_map_code("  DEAD BEEF 0123 4567 "), want);
        assert_eq!(parse_map_code("DEADBEEF-01234567"), want);
    }

    #[test]
    fn map_codes_reject_junk() {
        for junk in [
            "",
            "-",
            "hello",
            "DEAD-BEEF-0123-456",
            "DEAD-BEEF-0123-45678",
            "DEAD-BEEF-0123-456G",
            "0xDEADBEEF01234567",
            "DEAD-BEEF-0123-4567-0000",
            "DEAD_BEEF_0123_4567",
            "\u{FF24}EAD-BEEF-0123-4567",
        ] {
            assert_eq!(parse_map_code(junk), None, "{junk:?}");
        }
    }

    #[test]
    fn standard_maps_grow_with_the_players_and_small_keeps_the_old_sizes() {
        let sizes = |size| [2, 3, 4, 5, 6].map(|p| field_map_tiles(size, p));
        assert_eq!(sizes(MapSize::Small), [256, 256, 256, 320, 320]);
        assert_eq!(sizes(MapSize::Standard), [512, 640, 640, 768, 768]);
        assert_eq!(sizes(MapSize::Large), [640, 768, 768, 896, 896]);
        assert_eq!(LobbySettings::default().map_size, MapSize::Standard);
    }

    #[test]
    fn fresh_map_seeds_differ_every_press() {
        let seeds: Vec<u64> = (0..1000).map(|_| fresh_map_seed()).collect();
        let mut sorted = seeds.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), seeds.len());
        assert!(seeds.iter().any(|&s| s > u64::from(u32::MAX)));
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
    fn your_army_goes_with_your_faction() {
        let mut n = hosted();
        n.edit_slot(0, SlotEdit::Faction(Faction::Brood));
        n.edit_slot(0, SlotEdit::Army(vec![1, 1, 1]));
        assert_eq!(n.lobby().unwrap().slots[0].army, vec![1, 1, 1]);
        // Choosing the same faction keeps it; another clears it.
        n.edit_slot(0, SlotEdit::Faction(Faction::Brood));
        assert_eq!(n.lobby().unwrap().slots[0].army.len(), 3);
        n.edit_slot(0, SlotEdit::Faction(Faction::Choir));
        assert!(n.lobby().unwrap().slots[0].army.is_empty());
        // No army is longer than MAX_ARMY.
        n.edit_slot(0, SlotEdit::Army(vec![2; MAX_ARMY + 5]));
        assert_eq!(n.lobby().unwrap().slots[0].army.len(), MAX_ARMY);
        // Nobody edits a human's army but them.
        n.edit_slot(1, SlotEdit::Kind(SlotKind::Ai));
        let mut lobby = n.lobby().unwrap().clone();
        apply_edit(&mut lobby, 1, false, 0, SlotEdit::Army(vec![]));
        assert_eq!(lobby.slots[0].army.len(), MAX_ARMY);
        // A slot handed to an AI starts with no army: it picks its own.
        n.edit_slot(1, SlotEdit::Army(vec![1]));
        n.edit_slot(1, SlotEdit::Kind(SlotKind::Ai));
        assert!(n.lobby().unwrap().slots[1].army.is_empty());
    }

    #[test]
    fn a_conquest_map_holds_only_its_players() {
        let mut n = hosted();
        n.edit_slot(1, SlotEdit::Kind(SlotKind::Ai));
        n.edit_slot(2, SlotEdit::Kind(SlotKind::Ai));
        let mut settings = LobbySettings::default();
        settings.set_mode(GameMode::Conquest);
        n.edit_settings(settings.clone());
        let why = n.lobby().unwrap().start_blocker().unwrap();
        assert_eq!(why, "Ribcage Pass is for at most 2 players");
        settings.map = 1;
        n.edit_settings(settings);
        assert_eq!(n.lobby().unwrap().start_blocker(), None);
    }

    #[test]
    fn a_mode_change_starts_on_the_modes_first_map() {
        let mut s = LobbySettings {
            map: MAPS.len() - 1,
            ..Default::default()
        };
        s.set_mode(GameMode::Conquest);
        assert_eq!(s.map_name(), "Ribcage Pass");
        s.map = 1;
        s.set_mode(GameMode::Conquest);
        assert_eq!(
            s.map_name(),
            "The Four Teeth",
            "the same mode keeps its map"
        );
        s.set_mode(GameMode::FieldBattle);
        assert_eq!(s.map_name(), MAPS[DEFAULT_MAP]);
        assert_eq!(
            (Tempo::Swift.gather_percent(), Tempo::Swift.build_percent()),
            (150, 75)
        );
    }

    #[test]
    fn victory_says_which_victories_count() {
        assert_eq!(LobbySettings::default().victory, Victory::Both);
        let counts = Victory::ALL.map(|v| (v.name(), v.annihilation(), v.heart()));
        assert_eq!(
            counts,
            [
                ("Both", true, true),
                ("Annihilation", true, false),
                ("Titan's Heart", false, true),
            ]
        );
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
        colours.sort_unstable();
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
            n.join(&me(), "", "").unwrap_err(),
            NetError::BadAddress("Type an address or join code".into())
        );
        let e = n.join(&me(), "192.168.1.5:7777", "").unwrap_err();
        assert!(matches!(e, NetError::Unavailable(_)));
        assert!(n.lobby().is_none());
    }

    #[test]
    fn builds_play_together_when_only_the_commit_differs() {
        assert!(plays_with("0.1.0 sim 1 (abc123)", "0.1.0 sim 1 (def456)"));
        assert!(!plays_with("0.1.0 sim 1 (abc123)", "0.1.0 sim 2 (abc123)"));
        assert!(!plays_with("0.1.0 sim 1 (abc123)", "0.1.1 sim 1 (abc123)"));
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
