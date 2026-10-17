//! The main menu, Play (host or join), lobby, replays and settings screens.
//! See docs/08-ui-ux.md#menus-and-lobby.

use std::path::PathBuf;

use crate::draw::{Rect, fit, text_width};
use crate::hud::{MinimapTerrain, draw_terrain};
use crate::lobby::{
    AI_LEVELS, Budget, COLORS, Faction, GameMode, LobbySettings, LocalNetwork, MAX_ARMY,
    MAX_PLAYERS, MatchStart, NetEvent, Network, OnDrop, PLAYER_COLORS, Profile, RosterUnit,
    SlotEdit, SlotKind, Tempo, Victory, army_points, field_map_tiles, fresh_map_seed, map_code,
    maps_for, parse_map_code, thousands,
};
use crate::settings::{FRAME_CAPS, GraphicsPreset, Settings, UI_SCALES};
use crate::ui::{ButtonStyle, Ui, theme};

/// One team for each lobby slot.
const TEAM_NAMES: [&str; MAX_PLAYERS] =
    ["Team 1", "Team 2", "Team 3", "Team 4", "Team 5", "Team 6"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Main,
    Play,
    Lobby,
    /// The army builder, from the lobby.
    Army,
    /// The saved replays, to watch.
    Replays,
    Settings,
}

/// A saved replay, as the Replays screen lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayEntry {
    /// "Field Battle: Red (Mario) v Blue (AI 2)".
    pub title: String,
    /// "2026-10-15 21:50 UTC, 4:12 long".
    pub detail: String,
    pub path: PathBuf,
    /// Why this build can't play it, if it can't.
    pub unplayable: Option<String>,
}

/// What the game should do after a frame of menu.
#[derive(Debug)]
pub enum MenuAction {
    None,
    /// Close the menu and carry on with the current match.
    Resume,
    /// Start (or restart) the solo sandbox.
    StartSandbox,
    /// The host pressed Start in a lobby.
    StartMatch(MatchStart),
    /// Settings changed; the game should re-read `Menu::settings`.
    SettingsChanged,
    /// The Replays screen opened: the game should look for saved replays
    /// and hand them over with [`Menu::set_replays`].
    ListReplays,
    /// Watch the replay saved at this path.
    WatchReplay(PathBuf),
    /// The Unit viewer button: the game should open the viewer.
    OpenViewer,
    Quit,
}

/// Which map picture a screen wants (see [`Menu::wanted_map_preview`]):
/// the map a lobby's settings pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapPreviewKey {
    /// A Field Battle map: the choice (an index into
    /// [`MAPS`](crate::lobby::MAPS)), its size in tiles
    /// ([`field_map_tiles`]) and the map seed.
    Field {
        choice: usize,
        tiles: u32,
        seed: u64,
    },
    /// A shipped Conquest map, an index into
    /// [`CONQUEST_MAPS`](crate::lobby::CONQUEST_MAPS).
    Conquest(usize),
}

/// The map `settings` pick for `players` players.
pub fn preview_key(settings: &LobbySettings, players: usize) -> MapPreviewKey {
    match settings.mode {
        GameMode::FieldBattle => MapPreviewKey::Field {
            choice: settings.map,
            tiles: field_map_tiles(players),
            seed: settings.map_seed,
        },
        GameMode::Conquest => MapPreviewKey::Conquest(settings.map),
    }
}

/// A picture of a map the game made for the lobby.
struct MapPreview {
    key: MapPreviewKey,
    terrain: MinimapTerrain,
    /// Start positions, as fractions of the map's side (0 to 1).
    starts: Vec<[f32; 2]>,
}

/// What the map code box says when the host types something that isn't a
/// map code.
const NOT_A_CODE: &str = "Not a map code (16 hex digits)";

/// The menus, talking to the network through `N` ([`LocalNetwork`] for a
/// practice lobby on one machine, `gc_net::Lan` for real LAN play).
pub struct Menu<N: Network = LocalNetwork> {
    screen: Screen,
    net: N,
    settings: Settings,
    settings_path: Option<PathBuf>,
    version: String,
    host: LobbySettings,
    host_password: String,
    address: String,
    join_password: String,
    /// The `address` of the listed game picked on the Play screen.
    selected_game: Option<String>,
    message: Option<(String, bool)>,
    in_match: bool,
    /// The regiment types the army builder offers.
    roster: Vec<RosterUnit>,
    /// The Army screen's copy of your army while it is open.
    army_draft: Option<Vec<u16>>,
    /// The saved replays, newest first, and the page of them on screen.
    replays: Vec<ReplayEntry>,
    replay_page: usize,
    /// The game is still reading the replay folder.
    replays_loading: bool,
    /// What the map code box holds: the code of the map seed, or what the
    /// host is typing.
    code_entry: String,
    /// The map seed `code_entry` was made from, so the box follows the seed
    /// when it changes (a new map, or a code that was accepted).
    code_for: Option<u64>,
    /// Why the host's last map code was refused.
    code_note: Option<&'static str>,
    /// The map picture the screen on show wants, if it shows one.
    wanted_preview: Option<MapPreviewKey>,
    /// The latest picture the game made.
    preview: Option<MapPreview>,
}

/// Vertical flow of equally wide rows.
struct Stack {
    x: i32,
    y: i32,
    w: i32,
    gap: i32,
}

impl Stack {
    fn next(&mut self, h: i32) -> Rect {
        let r = Rect::new(self.x, self.y, self.w, h);
        self.y += h + self.gap;
        r
    }

    fn skip(&mut self, h: i32) {
        self.y += h;
    }
}

fn centered(screen: (i32, i32), w: i32, h: i32) -> Rect {
    let w = w.min(screen.0 - 24);
    let h = h.min(screen.1 - 24);
    Rect::new((screen.0 - w) / 2, (screen.1 - h) / 2, w, h)
}

impl<N: Network> Menu<N> {
    pub fn new(
        net: N,
        settings: Settings,
        settings_path: Option<PathBuf>,
        version: &str,
    ) -> Menu<N> {
        Menu {
            screen: Screen::Main,
            net,
            settings,
            settings_path,
            version: version.to_string(),
            host: LobbySettings::default(),
            host_password: String::new(),
            address: String::new(),
            join_password: String::new(),
            selected_game: None,
            message: None,
            in_match: false,
            roster: Vec::new(),
            army_draft: None,
            replays: Vec::new(),
            replay_page: 0,
            replays_loading: false,
            code_entry: String::new(),
            code_for: None,
            code_note: None,
            wanted_preview: None,
            preview: None,
        }
    }

    /// Start the Host a game card on this map seed (`grubcraft --seed`).
    pub fn set_host_map_seed(&mut self, seed: u64) {
        self.host.map_seed = seed;
    }

    /// The map picture the screen on show would like: the map choice, its
    /// size and the map seed (see [`MapPreviewKey`]).
    /// `None` when no screen shows one. It is worked out while a frame
    /// draws, so ask after [`Menu::frame`]; when it changes the game makes
    /// that map (off the frame loop, it takes a moment) and answers with
    /// [`Menu::set_map_preview`]. Until then a grey box stands in.
    pub fn wanted_map_preview(&self) -> Option<MapPreviewKey> {
        self.wanted_preview
    }

    /// The picture for `key`: the ground, and the start positions as
    /// fractions of the map's side. It shows only while `key` is still the
    /// one wanted, so a late answer for a map nobody is looking at is
    /// harmless.
    pub fn set_map_preview(
        &mut self,
        key: MapPreviewKey,
        terrain: MinimapTerrain,
        starts: Vec<[f32; 2]>,
    ) {
        self.preview = Some(MapPreview {
            key,
            terrain,
            starts,
        });
    }

    /// The key of the map picture now on screen, if a real one is (not the
    /// grey box).
    pub fn shown_map_preview(&self) -> Option<MapPreviewKey> {
        self.current_preview().map(|p| p.key)
    }

    /// The map picture, if it is for the map now wanted.
    fn current_preview(&self) -> Option<&MapPreview> {
        self.preview
            .as_ref()
            .filter(|p| Some(p.key) == self.wanted_preview)
    }

    /// The regiment types the army builder offers: the game's unit table,
    /// in its order (an army names them by index).
    pub fn set_roster(&mut self, roster: Vec<RosterUnit>) {
        self.roster = roster;
    }

    /// The saved replays for the Replays screen, newest first.
    pub fn set_replays(&mut self, replays: Vec<ReplayEntry>) {
        self.replays = replays;
        self.replay_page = 0;
        self.replays_loading = false;
    }

    /// The list is on its way: the Replays screen says so instead of
    /// "No replays yet" until [`Menu::set_replays`].
    pub fn replays_loading(&mut self) {
        // The old list would be stale, and a click on it could land on a
        // different replay once the new one arrives.
        self.replays.clear();
        self.replays_loading = true;
    }

    /// Show a problem on the current screen (the Replays screen shows why
    /// a replay wouldn't open).
    pub fn tell(&mut self, text: String) {
        self.message = Some((text, true));
    }

    /// The network the menus talk to.
    pub fn net(&mut self) -> &mut N {
        &mut self.net
    }

    /// The network, to look at.
    pub fn network(&self) -> &N {
        &self.net
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// Open the menu on a screen (the game uses `Main`).
    pub fn show(&mut self, screen: Screen) {
        self.screen = screen;
        self.message = None;
        self.forget_code_entry();
    }

    /// The map code box starts again from the map's own code.
    fn forget_code_entry(&mut self) {
        self.code_for = None;
        self.code_note = None;
    }

    /// Whether a match is running behind the menu, which adds Resume.
    pub fn set_in_match(&mut self, in_match: bool) {
        self.in_match = in_match;
    }

    /// Who this player is, from the settings.
    pub fn profile(&self) -> Profile {
        Profile {
            name: self.settings.player_name.clone(),
            color: self.settings.player_color,
            faction: self.settings.faction,
            version: self.version.clone(),
        }
    }

    fn go(&mut self, ui: &mut Ui, screen: Screen) {
        self.screen = screen;
        self.message = None;
        self.forget_code_entry();
        ui.reset();
    }

    /// The host's map code box (`field`) and the button that uses what was
    /// typed (`use_btn`), for a map on `seed`. Returns the new seed when a
    /// different, valid code was entered; a code that isn't one is
    /// ignored, and `code_note` says so.
    fn code_box(&mut self, ui: &mut Ui, field: Rect, use_btn: Rect, seed: u64) -> Option<u64> {
        if self.code_for != Some(seed) {
            self.code_entry = map_code(seed);
            self.code_for = Some(seed);
            self.code_note = None;
        }
        let (changed, submitted) = ui.text_entry(field, &mut self.code_entry, 24, "Map code");
        if changed {
            self.code_note = None;
        }
        let differs = self.code_entry != map_code(seed);
        let used = ui.button(use_btn, "Use", ButtonStyle::Normal, differs);
        if !(used || (submitted && differs)) {
            return None;
        }
        match parse_map_code(&self.code_entry) {
            Some(code) => {
                // The same map typed another way (lower case, no dashes)
                // shows as its tidy code again.
                self.code_for = None;
                (code != seed).then_some(code)
            }
            None => {
                self.code_note = Some(NOT_A_CODE);
                None
            }
        }
    }

    /// The map picture in `r` (square): the ground and the start
    /// positions when the game has made the map `key`, a grey box until
    /// then. Drawing it is what asks the game for that map.
    fn draw_map_preview(&mut self, ui: &mut Ui, r: Rect, key: MapPreviewKey) {
        self.wanted_preview = Some(key);
        let Some(p) = self.current_preview() else {
            ui.draw.rect(r, theme::FIELD);
            ui.draw.text_centered(r, 1, theme::DISABLED, "...");
            ui.draw.border(r, 2, theme::PANEL_EDGE);
            return;
        };
        draw_terrain(&mut ui.draw, r, &p.terrain);
        let inner = r.inset(3);
        for s in &p.starts {
            let x = r.x + (s[0] * r.w as f32) as i32;
            let y = r.y + (s[1] * r.h as f32) as i32;
            if inner.contains(x, y) {
                ui.draw.rect(Rect::new(x - 3, y - 3, 7, 7), theme::FIELD);
                ui.draw.rect(Rect::new(x - 2, y - 2, 5, 5), theme::ACCENT);
            }
        }
        ui.draw.border(r, 2, theme::PANEL_EDGE);
    }

    fn save_settings(&mut self) {
        self.settings.sanitize();
        if let Some(path) = &self.settings_path
            && let Err(e) = self.settings.save(path)
        {
            gc_platform::logln!(
                "grubcraft: could not save settings to {}: {e}",
                path.display()
            );
        }
    }

    /// Lay out, draw and handle one frame. Call between `Ui::begin_frame`
    /// and `Ui::end_frame`.
    pub fn frame(&mut self, ui: &mut Ui) -> MenuAction {
        self.wanted_preview = None;
        for event in self.net.poll() {
            match event {
                NetEvent::Started(start) => return MenuAction::StartMatch(start),
                NetEvent::Left(why) => {
                    self.screen = Screen::Play;
                    self.message = Some((why, true));
                    self.forget_code_entry();
                }
            }
        }
        let (w, h) = ui.screen;
        ui.draw.rect(Rect::new(0, 0, w, h), theme::SCRIM);
        let mut escape = ui.input.escape;
        if escape && ui.text_focused() {
            ui.reset();
            escape = false;
        }
        match self.screen {
            Screen::Main => self.main_screen(ui, escape),
            Screen::Play => self.play_screen(ui, escape),
            Screen::Lobby => self.lobby_screen(ui, escape),
            Screen::Army => self.army_screen(ui, escape),
            Screen::Replays => self.replays_screen(ui, escape),
            Screen::Settings => self.settings_screen(ui, escape),
        }
    }

    fn main_screen(&mut self, ui: &mut Ui, escape: bool) -> MenuAction {
        let (w, h) = ui.screen;
        let count = if self.in_match { 7 } else { 6 };
        // Up to 66 pixels for each button and its gap, less in a short window.
        let step = ((h - 120 - 24) / count).clamp(40, 66);
        let (button_h, gap) = (step - step / 5, step / 5);
        let total = 120 + count * step;
        let area = centered(ui.screen, 360, total);
        let title = "GRUBCRAFT";
        let tw = crate::draw::text_width(title, 4);
        ui.label(area.x + (area.w - tw) / 2, area.y, 4, theme::ACCENT, title);
        ui.label_centered(
            Rect::new(area.x, area.y + 76, area.w, 16),
            1,
            theme::MUTED,
            "Command regiments across the body of a fallen Titan",
        );
        let mut stack = Stack {
            x: area.x + 30,
            y: area.y + 120,
            w: area.w - 60,
            gap,
        };
        let mut action = MenuAction::None;
        if self.in_match && ui.button(stack.next(button_h), "Resume", ButtonStyle::Primary, true) {
            action = MenuAction::Resume;
        }
        let primary = if self.in_match {
            ButtonStyle::Normal
        } else {
            ButtonStyle::Primary
        };
        if ui.button(stack.next(button_h), "Play", primary, true) {
            self.go(ui, Screen::Play);
        }
        let sandbox = if self.in_match {
            "New sandbox"
        } else {
            "Sandbox"
        };
        if ui.button(stack.next(button_h), sandbox, ButtonStyle::Normal, true) {
            action = MenuAction::StartSandbox;
        }
        if ui.button(stack.next(button_h), "Replays", ButtonStyle::Normal, true) {
            self.go(ui, Screen::Replays);
            action = MenuAction::ListReplays;
        }
        if ui.button(
            stack.next(button_h),
            "Unit viewer",
            ButtonStyle::Normal,
            true,
        ) {
            action = MenuAction::OpenViewer;
        }
        if ui.button(stack.next(button_h), "Settings", ButtonStyle::Normal, true) {
            self.go(ui, Screen::Settings);
        }
        let quit = if cfg!(target_os = "macos") {
            "Quit"
        } else {
            "Exit"
        };
        if ui.button(stack.next(button_h), quit, ButtonStyle::Normal, true) {
            action = MenuAction::Quit;
        }
        ui.label(
            12,
            h - 24,
            1,
            theme::MUTED,
            &format!("Grubcraft {}", self.version),
        );
        let hint = "Esc closes menus";
        ui.label_right(w - 12, h - 24, 1, theme::MUTED, hint);
        if escape && self.in_match {
            action = MenuAction::Resume;
        }
        action
    }

    fn play_screen(&mut self, ui: &mut Ui, escape: bool) -> MenuAction {
        let k = (ui.screen.1 as f32 / 720.0).clamp(0.72, 1.0);
        let sp = |n: i32| (n as f32 * k).round() as i32;
        // Six host rows (map, map code, mode, tempo, victory, password),
        // then the Host and Back buttons, which keep a fixed pixel size.
        let panel = centered(ui.screen, 880, sp(64 + 74 + 6 * 48 + 8) + 124);
        ui.panel(panel);
        ui.label(panel.x + 24, panel.y + 16, 2, theme::TEXT, "Play");
        let card_w = (panel.w - 72) / 2;
        let top = panel.y + sp(64);

        // Host card.
        let mut s = Stack {
            x: panel.x + 24,
            y: top,
            w: card_w,
            gap: sp(12),
        };
        ui.label(s.x, s.y, 2, theme::ACCENT, "Host a game");
        s.skip(sp(40));
        let hint_y = s.y;
        s.skip(sp(34));
        // The map, a button for another one like it, and the map's code
        // (to type in one a friend shared).
        let map_row = s.next(sp(36));
        let new_w = 96;
        let mut map = self.host.map;
        let map_r = Rect::new(map_row.x, map_row.y, map_row.w - new_w - 8, map_row.h);
        if ui.stepper(map_r, "Map", &mut map, maps_for(self.host.mode)) {
            self.host.map = map;
        }
        let new_r = Rect::new(map_row.right() - new_w, map_row.y, new_w, map_row.h);
        if ui.button(new_r, "New map", ButtonStyle::Normal, true) {
            self.host.map_seed = fresh_map_seed();
        }
        let code_row = s.next(sp(36));
        let use_w = 64;
        let field = Rect::new(code_row.x, code_row.y, code_row.w - use_w - 8, code_row.h);
        let use_r = Rect::new(code_row.right() - use_w, code_row.y, use_w, code_row.h);
        if let Some(seed) = self.code_box(ui, field, use_r, self.host.map_seed) {
            self.host.map_seed = seed;
        }
        let (hint, hint_color) = match self.code_note {
            Some(note) => (note, theme::BAD),
            None => ("Friends on your Wi-Fi see it automatically.", theme::MUTED),
        };
        ui.label(s.x, hint_y, 1, hint_color, hint);
        // The map's picture sits beside the three rows below it (mode,
        // tempo and victory); the password has the card's full width under
        // them.
        let pv = 3 * sp(36) + 2 * sp(12);
        let preview_r = Rect::new(s.x + card_w - pv, s.y, pv, pv);
        s.w = card_w - pv - 12;
        let mode_names = GameMode::ALL.map(GameMode::name);
        let mut mode = GameMode::ALL
            .iter()
            .position(|&m| m == self.host.mode)
            .unwrap_or(0);
        if ui.stepper(s.next(sp(36)), "Mode", &mut mode, &mode_names) {
            self.host.set_mode(GameMode::ALL[mode]);
        }
        let tempo_names: Vec<&str> = Tempo::ALL.iter().map(|t| t.name()).collect();
        let mut tempo = Tempo::ALL
            .iter()
            .position(|&t| t == self.host.tempo)
            .unwrap_or(0);
        if ui.stepper(s.next(sp(36)), "Tempo", &mut tempo, &tempo_names) {
            self.host.tempo = Tempo::ALL[tempo];
        }
        let victory_names: Vec<&str> = Victory::ALL.iter().map(|v| v.name()).collect();
        let mut victory = Victory::ALL
            .iter()
            .position(|&v| v == self.host.victory)
            .unwrap_or(0);
        if ui.stepper(s.next(sp(36)), "Victory", &mut victory, &victory_names) {
            self.host.victory = Victory::ALL[victory];
        }
        s.w = card_w;
        let mut password = std::mem::take(&mut self.host_password);
        ui.text_field(s.next(sp(36)), &mut password, 24, "Password (optional)");
        self.host_password = password;
        // A Field Battle map is made for the players in it; the card can't
        // know yet, so it shows the size two players get.
        let key = preview_key(&self.host, 2);
        self.draw_map_preview(ui, preview_r, key);
        s.skip(sp(8));
        let host_btn = Rect::new(s.x, panel.bottom() - sp(64) - 60, card_w, 52);
        if ui.button(host_btn, "Host", ButtonStyle::Primary, true) {
            let me = self.profile();
            match self.net.host(&me, self.host.clone(), &self.host_password) {
                Ok(()) => self.go(ui, Screen::Lobby),
                Err(e) => self.message = Some((e.to_string(), true)),
            }
        }

        // Join card.
        let mut s = Stack {
            x: panel.x + 48 + card_w,
            y: top,
            w: card_w,
            gap: sp(12),
        };
        ui.label(s.x, s.y, 2, theme::ACCENT, "Join a game");
        s.skip(sp(40));
        ui.label(s.x, s.y, 1, theme::MUTED, "Games on your network:");
        s.skip(sp(24));
        let list = Rect::new(s.x, s.y, s.w, sp(112));
        ui.draw.rect(list, theme::FIELD);
        ui.draw.border(list, 2, theme::PANEL_EDGE);
        let games: Vec<_> = self.net.discovered().to_vec();
        if games.is_empty() {
            ui.label_centered(list, 1, theme::DISABLED, "No games found yet");
        }
        const LISTED: usize = 3;
        for (i, g) in games.iter().take(LISTED).enumerate() {
            let row = Rect::new(
                list.x + 4,
                list.y + 4 + i as i32 * sp(34),
                list.w - 8,
                sp(30),
            );
            if ui.row(
                row,
                self.selected_game.as_deref() == Some(g.address.as_str()),
            ) {
                self.selected_game = Some(g.address.clone());
            }
            let text = format!(
                "{}  {}  {}/{}{}{}",
                g.host,
                g.map,
                g.players,
                g.max_players,
                if g.playing { "  (in progress)" } else { "" },
                if g.locked { "  (password)" } else { "" }
            );
            let ty = row.y + (row.h - 16) / 2;
            ui.label(
                row.x + 8,
                ty,
                1,
                theme::TEXT,
                &fit(&text, (row.w / 8 - 12) as usize),
            );
            if !crate::lobby::plays_with(&g.version, &self.version) {
                ui.label_right(row.right() - 8, ty, 1, theme::BAD, &g.version);
            }
        }
        // Games come and go while the player types: the fields below keep
        // the same ids whatever the list shows.
        ui.reserve_ids(LISTED - games.len().min(LISTED));
        s.y = list.bottom() + sp(14);
        let mut address = std::mem::take(&mut self.address);
        if ui.text_field(s.next(sp(36)), &mut address, 40, "Address or join code") {
            self.selected_game = None;
        }
        self.address = address;
        let mut password = std::mem::take(&mut self.join_password);
        ui.text_field(
            s.next(sp(36)),
            &mut password,
            24,
            "Password, if the host set one",
        );
        self.join_password = password;
        let join_btn = Rect::new(s.x, panel.bottom() - sp(64) - 60, card_w, 52);
        // A game that has gone, or fallen off the rows shown, is no longer
        // picked: it would come back unseen if it reappeared.
        let shown = |a: &String| games.iter().take(LISTED).any(|g| &g.address == a);
        if self.selected_game.as_ref().is_some_and(|a| !shown(a)) {
            self.selected_game = None;
        }
        // A game listed without a password is joined without one, whatever
        // is left in the box.
        let picked = self
            .selected_game
            .as_ref()
            .and_then(|a| games.iter().find(|g| &g.address == a));
        let (target, password) = match picked {
            Some(g) if !g.locked => (g.address.clone(), ""),
            Some(g) => (g.address.clone(), self.join_password.as_str()),
            None => (self.address.trim().to_string(), self.join_password.as_str()),
        };
        if ui.button(join_btn, "Join", ButtonStyle::Normal, !target.is_empty()) {
            let me = self.profile();
            match self.net.join(&me, &target, password) {
                Ok(()) => self.go(ui, Screen::Lobby),
                Err(e) => self.message = Some((e.to_string(), true)),
            }
        }

        // Footer.
        let back = Rect::new(panel.x + 24, panel.bottom() - 52, 140, 44);
        if let Some((text, is_error)) = &self.message {
            let color = if *is_error { theme::BAD } else { theme::GOOD };
            let chars = ((panel.w - 48) / 8) as usize;
            // Two lines at most.
            let first: String = text.chars().take(chars).collect();
            let rest: String = text.chars().skip(chars).take(chars).collect();
            ui.label(panel.x + 24, back.y - sp(46), 1, color, &first);
            if !rest.is_empty() {
                ui.label(
                    panel.x + 24,
                    back.y - sp(46) + 18,
                    1,
                    color,
                    &fit(&rest, chars),
                );
            }
        }
        if ui.button(back, "Back", ButtonStyle::Normal, true) || escape {
            self.go(ui, Screen::Main);
        }
        MenuAction::None
    }

    fn lobby_screen(&mut self, ui: &mut Ui, escape: bool) -> MenuAction {
        let Some(lobby) = self.net.lobby().cloned() else {
            self.go(ui, Screen::Play);
            return MenuAction::None;
        };
        let k = (ui.screen.1 as f32 / 720.0).clamp(0.72, 1.0);
        let sp = |n: i32| (n as f32 * k).round() as i32;
        // Header and settings, the map's picture and code, one row per lobby
        // slot, then the footer (three lines of text and the buttons, which
        // keep a fixed pixel size).
        let (row_h, preview_side) = lobby_sizes(ui.screen.1, k);
        let strip_h = preview_side.max(sp(36));
        let row_gap = sp(6);
        let rows_h = MAX_PLAYERS as i32 * (row_h + row_gap);
        let panel = centered(
            ui.screen,
            880,
            sp(64 + 36 + 20 + 4) + strip_h + rows_h + 124,
        );
        ui.panel(panel);
        ui.label(panel.x + 24, panel.y + 16, 2, theme::TEXT, "Lobby");
        let host_name = lobby
            .slots
            .iter()
            .find(|s| s.kind == SlotKind::Human)
            .map_or("", |s| s.name.as_str());
        ui.label_right(
            panel.right() - 24,
            panel.y + 24,
            1,
            theme::MUTED,
            &format!("Host: {host_name}   Build {}", lobby.host_version),
        );

        // Match settings: the host edits, everyone sees.
        let mut settings = lobby.settings.clone();
        let col_w = (panel.w - 48 - 36) / 4;
        let y = panel.y + sp(64);
        let slot_rect = |i: i32| Rect::new(panel.x + 24 + i * (col_w + 12), y, col_w, sp(36));
        // Champions may duel before a battle with a computer player: in the
        // footer, beside Leave.
        let champions_r = Rect::new(panel.x + 24 + 140 + 24, panel.bottom() - 52, 320, 44);
        // Which victories count is a Conquest setting too, but the four
        // columns above have no room for another stepper: it sits in the
        // header, between the title and the host's name, clear of them.
        let victory_r = Rect::new(panel.x + 140, panel.y + 8, 270, sp(36));
        const CHAMPIONS: &str = "Champions before AI battles";
        let mut changed = false;
        if lobby.is_host {
            let mut map = settings.map;
            if ui.stepper(slot_rect(0), "Map", &mut map, maps_for(settings.mode)) {
                settings.map = map;
                changed = true;
            }
            let mode_names = GameMode::ALL.map(GameMode::name);
            let mut mode = GameMode::ALL
                .iter()
                .position(|&m| m == settings.mode)
                .unwrap_or(0);
            if ui.stepper(slot_rect(1), "Mode", &mut mode, &mode_names) {
                settings.set_mode(GameMode::ALL[mode]);
                changed = true;
            }
            // Tempo is for Conquest; a Field Battle has a points budget.
            if settings.mode == GameMode::FieldBattle {
                let names = Budget::ALL.map(|b| thousands(b.points()));
                let names = names.each_ref().map(String::as_str);
                let mut budget = Budget::ALL
                    .iter()
                    .position(|&b| b == settings.budget)
                    .unwrap_or(0);
                if ui.stepper(slot_rect(2), "Points", &mut budget, &names) {
                    settings.budget = Budget::ALL[budget];
                    changed = true;
                }
            } else {
                let tempo_names: Vec<&str> = Tempo::ALL.iter().map(|t| t.name()).collect();
                let mut tempo = Tempo::ALL
                    .iter()
                    .position(|&t| t == settings.tempo)
                    .unwrap_or(0);
                if ui.stepper(slot_rect(2), "Tempo", &mut tempo, &tempo_names) {
                    settings.tempo = Tempo::ALL[tempo];
                    changed = true;
                }
            }
            let drop_names: Vec<&str> = OnDrop::ALL.iter().map(|d| d.name()).collect();
            let mut on_drop = OnDrop::ALL
                .iter()
                .position(|&d| d == settings.on_drop)
                .unwrap_or(0);
            if ui.stepper(slot_rect(3), "On drop", &mut on_drop, &drop_names) {
                settings.on_drop = OnDrop::ALL[on_drop];
                changed = true;
            }
            if settings.mode == GameMode::Conquest {
                let names: Vec<&str> = Victory::ALL.iter().map(|v| v.name()).collect();
                let mut victory = Victory::ALL
                    .iter()
                    .position(|&v| v == settings.victory)
                    .unwrap_or(0);
                if ui.stepper(victory_r, "Victory", &mut victory, &names) {
                    settings.victory = Victory::ALL[victory];
                    changed = true;
                }
            }
            changed |= ui.toggle(champions_r, CHAMPIONS, &mut settings.champions);
        } else {
            for (i, text) in [
                format!("Map: {}", settings.map_name()),
                format!("Mode: {}", settings.mode.name()),
                if settings.mode == GameMode::FieldBattle {
                    format!("Points: {}", thousands(settings.budget.points()))
                } else {
                    format!("Tempo: {}", settings.tempo.name())
                },
                format!("On drop: {}", settings.on_drop.name()),
            ]
            .iter()
            .enumerate()
            {
                ui.label_centered(slot_rect(i as i32), 1, theme::TEXT, text);
            }
            if settings.mode == GameMode::Conquest {
                let text = format!("Victory: {}", settings.victory.name());
                ui.label_centered(victory_r, 1, theme::TEXT, &text);
            }
            let on = if settings.champions { "on" } else { "off" };
            ui.label_centered(champions_r, 1, theme::MUTED, &format!("{CHAMPIONS}: {on}"));
        }

        // The map: its picture (when there is room), its name and its code.
        // The host can ask for a new map or type in a code; everyone sees
        // the code.
        let strip = Rect::new(panel.x + 24, y + sp(36) + sp(10), panel.w - 48, strip_h);
        let (mut x, mut w) = (strip.x, strip.w);
        if preview_side > 0 {
            let occupied = lobby.slots.iter().filter(|s| s.occupied()).count();
            let picture = Rect::new(strip.x, strip.y, preview_side, preview_side);
            let key = preview_key(&settings, occupied);
            self.draw_map_preview(ui, picture, key);
            (x, w) = (strip.x + preview_side + 16, strip.w - preview_side - 16);
            let name = settings.map_name();
            ui.label(x, strip.y, 2, theme::TEXT, &fit(name, (w / 16) as usize));
            // The orange dots on the picture, said once, if there is room.
            if preview_side >= sp(88) {
                let hint = "Orange dots are the start positions.";
                ui.label(x, strip.y + 38, 1, theme::MUTED, hint);
            }
        }
        let code_r = Rect::new(x, strip.bottom() - sp(36), w, sp(36));
        let text_y = code_r.y + (code_r.h - 16) / 2;
        ui.label(code_r.x, text_y, 1, theme::MUTED, "Map code");
        let field_x = code_r.x + text_width("Map code", 1) + 8;
        if lobby.is_host {
            let field = Rect::new(field_x, code_r.y, 180, code_r.h);
            let use_r = Rect::new(field.right() + 8, code_r.y, 52, code_r.h);
            let new_r = Rect::new(use_r.right() + 8, code_r.y, 96, code_r.h);
            if let Some(seed) = self.code_box(ui, field, use_r, settings.map_seed) {
                settings.map_seed = seed;
                changed = true;
            }
            if ui.button(new_r, "New map", ButtonStyle::Normal, true) {
                settings.map_seed = fresh_map_seed();
                changed = true;
            }
            if let Some(note) = self.code_note {
                let note_x = new_r.right() + 12;
                let room = ((code_r.right() - note_x) / 8).max(0) as usize;
                ui.label(note_x, text_y, 1, theme::BAD, &fit(note, room));
            }
        } else {
            let code = map_code(settings.map_seed);
            let scale = if code_r.h >= 32 { 2 } else { 1 };
            let code_y = code_r.y + (code_r.h - 16 * scale) / 2;
            ui.label(field_x, code_y, scale, theme::TEXT, &code);
        }
        if changed {
            self.net.edit_settings(settings);
        }

        // Player rows.
        let rows_top = strip.bottom() + sp(10);
        for (i, slot) in lobby.slots.iter().enumerate() {
            let row = Rect::new(
                panel.x + 24,
                rows_top + i as i32 * (row_h + row_gap),
                panel.w - 48,
                row_h,
            );
            ui.draw.rect(row, [30, 35, 46, 255]);
            let is_me = i == lobby.local_slot;
            let swatch_sz = row_h - 16;
            let swatch = Rect::new(row.x + 8, row.y + 8, swatch_sz, swatch_sz);
            let [r, g, b] = PLAYER_COLORS[usize::from(slot.color % COLORS)].1;
            let editable =
                slot.occupied() && (is_me || (lobby.is_host && slot.kind == SlotKind::Ai));
            if !slot.occupied() {
                ui.draw.rect(swatch, [50, 56, 70, 255]);
            } else if ui.swatch(swatch, [r, g, b, 255], editable) {
                self.net.edit_slot(i, SlotEdit::Color(slot.color + 1));
            }
            let text_y = row.y + (row.h - 16) / 2;
            let name_x = swatch.right() + 12;
            if !slot.occupied() {
                ui.label(name_x, text_y, 1, theme::DISABLED, "Open slot");
                if lobby.is_host
                    && ui.button(
                        Rect::new(row.right() - 148, row.y + 8, 140, row_h - 16),
                        "Add AI",
                        ButtonStyle::Normal,
                        true,
                    )
                {
                    self.net.edit_slot(i, SlotEdit::Kind(SlotKind::Ai));
                }
                continue;
            }
            let you = if is_me { " (you)" } else { "" };
            ui.label(
                name_x,
                text_y,
                1,
                theme::TEXT,
                &fit(&format!("{}{you}", slot.name), 17),
            );
            let ctl_h = row_h - 16;
            let faction_r = Rect::new(row.x + 250, row.y + 8, 200, ctl_h);
            let team_r = Rect::new(faction_r.right() + 8, row.y + 8, 120, ctl_h);
            let status_r = Rect::new(
                team_r.right() + 8,
                row.y + 8,
                row.right() - team_r.right() - 16,
                ctl_h,
            );
            if editable {
                let names: Vec<&str> = Faction::ALL.iter().map(|f| f.name()).collect();
                let mut fi = slot.faction.index();
                if ui.stepper(faction_r, "", &mut fi, &names) {
                    self.net.edit_slot(i, SlotEdit::Faction(Faction::ALL[fi]));
                }
                let mut ti = slot.team as usize;
                if ui.stepper(team_r, "", &mut ti, &TEAM_NAMES) {
                    self.net.edit_slot(i, SlotEdit::Team(ti as u8));
                }
            } else {
                ui.label_centered(faction_r, 1, theme::TEXT, slot.faction.name());
                let team = TEAM_NAMES[usize::from(slot.team) % MAX_PLAYERS];
                ui.label_centered(team_r, 1, theme::TEXT, team);
            }
            match slot.kind {
                SlotKind::Ai if lobby.is_host => {
                    // How well it plays, and a way to send it home.
                    let remove_w = 80;
                    let level_r =
                        Rect::new(status_r.x, status_r.y, status_r.w - remove_w - 8, ctl_h);
                    let remove_r =
                        Rect::new(status_r.right() - remove_w, status_r.y, remove_w, ctl_h);
                    let mut level = slot.ai_level as usize;
                    if ui.stepper(level_r, "", &mut level, &AI_LEVELS) {
                        self.net.edit_slot(i, SlotEdit::AiLevel(level as u8));
                    }
                    if ui.button(remove_r, "Remove", ButtonStyle::Normal, true) {
                        self.net.edit_slot(i, SlotEdit::Kind(SlotKind::Open));
                    }
                }
                SlotKind::Human if is_me && !lobby.is_host => {
                    let mut ready = slot.ready;
                    if ui.toggle(status_r, "Ready", &mut ready) {
                        self.net.edit_slot(i, SlotEdit::Ready(ready));
                    }
                }
                _ => {
                    let (text, color) = if slot.kind == SlotKind::Human && is_me && lobby.is_host {
                        ("Host", theme::ACCENT)
                    } else if slot.kind == SlotKind::Ai {
                        (
                            AI_LEVELS[slot.ai_level as usize % AI_LEVELS.len()],
                            theme::MUTED,
                        )
                    } else if slot.ready {
                        ("Ready", theme::GOOD)
                    } else {
                        ("Not ready", theme::MUTED)
                    };
                    ui.label_centered(status_r, 1, color, text);
                }
            }
        }

        // Footer: why we can't start, plus the faction hint for you.
        let me = &lobby.slots[lobby.local_slot];
        let mut hint = format!(
            "{}: {} to play.",
            me.faction.name(),
            me.faction.difficulty()
        );
        let field_battle = lobby.settings.mode == GameMode::FieldBattle;
        let mut hint_color = theme::MUTED;
        if field_battle {
            let budget = lobby.settings.budget.points();
            let spent = army_points(&self.roster, &me.army);
            hint += &if me.army.is_empty() {
                "   Army: picked for you (Army to build your own).".to_string()
            } else if spent > budget {
                hint_color = theme::BAD;
                format!(
                    "   Army over budget: {}.",
                    army_summary(me.army.len(), spent, budget)
                )
            } else {
                format!("   Army: {}.", army_summary(me.army.len(), spent, budget))
            };
        }
        let footer_y = rows_top + rows_h + sp(4);
        ui.label(panel.x + 24, footer_y, 1, hint_color, &hint);
        let blocker = lobby.start_blocker();
        let mismatch = !lobby.version_mismatches().is_empty();
        if let Some(why) = &blocker {
            let color = if mismatch { theme::BAD } else { theme::MUTED };
            ui.label(
                panel.x + 24,
                footer_y + 22,
                1,
                color,
                &fit(why, ((panel.w - 48) / 8) as usize),
            );
        } else {
            ui.label(
                panel.x + 24,
                footer_y + 22,
                1,
                theme::GOOD,
                "Everyone is ready.",
            );
        }
        if let Some(invite) = self.net.invite() {
            ui.label(panel.x + 24, footer_y + 44, 1, theme::MUTED, invite);
        }
        let btn_y = panel.bottom() - 52;
        let mut action = MenuAction::None;
        let leave = ui.button(
            Rect::new(panel.x + 24, btn_y, 140, 44),
            "Leave",
            ButtonStyle::Normal,
            true,
        );
        if field_battle
            && ui.button(
                Rect::new(panel.x + 24 + 148, btn_y, 140, 44),
                "Army",
                ButtonStyle::Normal,
                true,
            )
        {
            self.go(ui, Screen::Army);
        }
        if lobby.is_host {
            let go = Rect::new(panel.right() - 24 - 200, btn_y - 4, 200, 48);
            if ui.button(go, "Start", ButtonStyle::Primary, blocker.is_none()) {
                match self.net.start_match() {
                    Ok(start) => action = MenuAction::StartMatch(start),
                    Err(e) => self.message = Some((e.to_string(), true)),
                }
            }
        }
        if leave || escape {
            self.net.leave();
            self.go(ui, Screen::Play);
        }
        action
    }

    /// Spend the lobby's points budget on your faction's regiments
    /// (docs/06-game-modes.md, "Field Battle"). The army lives in your lobby
    /// slot, so the host raises exactly it when the battle starts.
    fn army_screen(&mut self, ui: &mut Ui, escape: bool) -> MenuAction {
        let Some(lobby) = self.net.lobby().cloned() else {
            self.go(ui, Screen::Play);
            return MenuAction::None;
        };
        if lobby.settings.mode != GameMode::FieldBattle {
            self.go(ui, Screen::Lobby);
            return MenuAction::None;
        }
        let me = &lobby.slots[lobby.local_slot];
        let budget = lobby.settings.budget.points();
        // Edits go from the army as this screen last left it, not as the
        // lobby shows it: a joiner's lobby only catches up when the host
        // echoes the edit, and a second click before then would be lost.
        let mut army = self.army_draft.take().unwrap_or_else(|| me.army.clone());
        let k = (ui.screen.1 as f32 / 720.0).clamp(0.72, 1.0);
        let sp = |n: i32| (n as f32 * k).round() as i32;
        let panel = centered(ui.screen, 640, sp(560));
        ui.panel(panel);
        ui.label(panel.x + 24, panel.y + 16, 2, theme::TEXT, "Army");
        ui.label_right(
            panel.right() - 24,
            panel.y + 24,
            1,
            theme::MUTED,
            &format!(
                "{}   {} points",
                me.faction.name(),
                thousands(lobby.settings.budget.points())
            ),
        );

        // One row per regiment type of your faction: cost, and how many.
        let spent = army_points(&self.roster, &army);
        let mut changed = false;
        let row_h = sp(44);
        let mut y = panel.y + sp(64);
        for (u, unit) in self.roster.iter().enumerate() {
            if unit.faction != me.faction {
                continue;
            }
            let row = Rect::new(panel.x + 24, y, panel.w - 48, row_h);
            ui.draw.rect(row, [30, 35, 46, 255]);
            let text_y = row.y + (row.h - 16) / 2;
            ui.label(row.x + 12, text_y, 1, theme::TEXT, &fit(&unit.name, 24));
            ui.label(
                row.x + 260,
                text_y,
                1,
                theme::MUTED,
                &format!("{} points", thousands(unit.points)),
            );
            let count = army.iter().filter(|&&a| a as usize == u).count();
            let btn = row_h - 12;
            let plus = Rect::new(row.right() - 6 - btn, row.y + 6, btn, btn);
            let count_r = Rect::new(plus.x - 56, row.y, 56, row.h);
            let minus = Rect::new(count_r.x - btn, row.y + 6, btn, btn);
            ui.label_centered(count_r, 1, theme::TEXT, &count.to_string());
            if ui.button(minus, "-", ButtonStyle::Normal, count > 0) {
                if let Some(at) = army.iter().rposition(|&a| a as usize == u) {
                    army.remove(at);
                }
                changed = true;
            }
            let fits = spent + unit.points <= budget && army.len() < MAX_ARMY;
            if ui.button(plus, "+", ButtonStyle::Normal, fits) {
                // Kept grouped by type, in roster order.
                let at = army
                    .iter()
                    .position(|&a| a as usize > u)
                    .unwrap_or(army.len());
                army.insert(at, u as u16);
                changed = true;
            }
            y += row_h + sp(8);
        }

        let summary_y = y + sp(8);
        if army.is_empty() {
            ui.label(
                panel.x + 24,
                summary_y,
                1,
                theme::MUTED,
                "No regiments chosen: the Balanced mix will be picked for you.",
            );
        } else {
            let color = if spent > budget {
                theme::BAD
            } else {
                theme::TEXT
            };
            ui.label(
                panel.x + 24,
                summary_y,
                1,
                color,
                &army_summary(army.len(), spent, budget),
            );
            if spent > budget {
                ui.label(
                    panel.x + 24,
                    summary_y + 22,
                    1,
                    theme::BAD,
                    "Over budget: the regiments lowest on the list will stay at home.",
                );
            }
        }

        let btn_y = panel.bottom() - 52;
        if ui.button(
            Rect::new(panel.x + 24, btn_y, 180, 44),
            "Pick for me",
            ButtonStyle::Normal,
            !army.is_empty(),
        ) {
            army.clear();
            changed = true;
        }
        let done = ui.button(
            Rect::new(panel.right() - 24 - 160, btn_y - 4, 160, 48),
            "Done",
            ButtonStyle::Primary,
            true,
        );
        if changed {
            self.net
                .edit_slot(lobby.local_slot, SlotEdit::Army(army.clone()));
        }
        if done || escape {
            self.go(ui, Screen::Lobby);
        } else {
            self.army_draft = Some(army);
        }
        MenuAction::None
    }

    /// The saved replays, newest first, a page at a time, each with Watch.
    fn replays_screen(&mut self, ui: &mut Ui, escape: bool) -> MenuAction {
        let k = (ui.screen.1 as f32 / 720.0).clamp(0.72, 1.0);
        let sp = |n: i32| (n as f32 * k).round() as i32;
        let panel = centered(ui.screen, 760, sp(560));
        ui.panel(panel);
        ui.label(panel.x + 24, panel.y + 16, 2, theme::TEXT, "Replays");
        ui.label_right(
            panel.right() - 24,
            panel.y + 24,
            1,
            theme::MUTED,
            "Every battle is saved here",
        );
        let row_h = sp(56);
        let top = panel.y + sp(64);
        let bottom = panel.bottom() - 64;
        let per_page = (((bottom - top) / (row_h + sp(8))).max(1)) as usize;
        let pages = self.replays.len().div_ceil(per_page).max(1);
        self.replay_page = self.replay_page.min(pages - 1);
        let mut action = MenuAction::None;
        if self.replays.is_empty() {
            let text = if self.replays_loading {
                "Looking for replays..."
            } else {
                "No replays yet. Play a battle and it will be here to watch."
            };
            ui.label(panel.x + 24, top, 1, theme::MUTED, text);
        }
        let first = self.replay_page * per_page;
        let mut y = top;
        for entry in self.replays.iter().skip(first).take(per_page) {
            let row = Rect::new(panel.x + 24, y, panel.w - 48, row_h);
            ui.draw.rect(row, [30, 35, 46, 255]);
            let chars = ((row.w - 160) / 8).max(8) as usize;
            ui.label(
                row.x + 12,
                row.y + 10,
                1,
                theme::TEXT,
                &fit(&entry.title, chars),
            );
            let (color, detail) = match &entry.unplayable {
                Some(why) => (theme::BAD, why.as_str()),
                None => (theme::MUTED, entry.detail.as_str()),
            };
            ui.label(
                row.x + 12,
                row.y + row_h - 26,
                1,
                color,
                &fit(detail, chars),
            );
            let watch = Rect::new(row.right() - 128, row.y + 8, 116, row_h - 16);
            if ui.button(
                watch,
                "Watch",
                ButtonStyle::Normal,
                entry.unplayable.is_none(),
            ) {
                action = MenuAction::WatchReplay(entry.path.clone());
            }
            y += row_h + sp(8);
        }
        let btn_y = panel.bottom() - 52;
        if let Some((text, _)) = &self.message {
            let chars = ((panel.w - 48) / 8).max(8) as usize;
            ui.label(panel.x + 24, btn_y - 26, 1, theme::BAD, &fit(text, chars));
        }
        if pages > 1 {
            if ui.button(
                Rect::new(panel.x + 24, btn_y, 120, 44),
                "Newer",
                ButtonStyle::Normal,
                self.replay_page > 0,
            ) {
                self.replay_page -= 1;
            }
            ui.label_centered(
                Rect::new(panel.x + 152, btn_y, 120, 44),
                1,
                theme::MUTED,
                &format!("{} of {pages}", self.replay_page + 1),
            );
            if ui.button(
                Rect::new(panel.x + 280, btn_y, 120, 44),
                "Older",
                ButtonStyle::Normal,
                self.replay_page + 1 < pages,
            ) {
                self.replay_page += 1;
            }
        }
        let back = ui.button(
            Rect::new(panel.right() - 24 - 160, btn_y - 4, 160, 48),
            "Back",
            ButtonStyle::Primary,
            true,
        );
        if back || escape {
            self.go(ui, Screen::Main);
        }
        action
    }

    fn settings_screen(&mut self, ui: &mut Ui, escape: bool) -> MenuAction {
        let k = (ui.screen.1 as f32 / 720.0).clamp(0.62, 1.0);
        let sp = |n: i32| (n as f32 * k).round() as i32;
        let panel = centered(ui.screen, 880, sp(640));
        ui.panel(panel);
        ui.label(panel.x + 24, panel.y + 14, 2, theme::TEXT, "Settings");
        let col_w = (panel.w - 72) / 2;
        let row = sp(34);
        let gap = sp(6);
        let top = panel.y + sp(60);
        let before = self.settings.clone();
        let s = &mut self.settings;

        // Left column: player and display.
        let mut st = Stack {
            x: panel.x + 24,
            y: top,
            w: col_w,
            gap,
        };
        ui.label(st.x, st.y, 1, theme::ACCENT, "PLAYER");
        st.skip(sp(22));
        let mut name = std::mem::take(&mut s.player_name);
        ui.text_field(st.next(row), &mut name, 16, "Your name");
        s.player_name = name;
        let sw = st.next(row);
        ui.label(sw.x, sw.y + (sw.h - 16) / 2, 1, theme::TEXT, "Colour");
        for (i, (_, [r, g, b])) in PLAYER_COLORS.iter().enumerate() {
            let sq = Rect::new(sw.x + 150 + i as i32 * (row + 8), sw.y, row, row);
            if ui.swatch(sq, [*r, *g, *b, 255], true) {
                s.player_color = i as u8;
            }
            if s.player_color as usize == i {
                ui.draw.border(sq.inset(-3), 3, theme::ACCENT);
            }
        }
        let fnames: Vec<&str> = Faction::ALL.iter().map(|f| f.name()).collect();
        let mut fi = s.faction.index();
        if ui.stepper(st.next(row), "Faction", &mut fi, &fnames) {
            s.faction = Faction::ALL[fi];
        }
        st.skip(sp(10));
        ui.label(st.x, st.y, 1, theme::ACCENT, "DISPLAY");
        st.skip(sp(22));
        let gnames: Vec<&str> = GraphicsPreset::ALL.iter().map(|g| g.name()).collect();
        let mut gi = GraphicsPreset::ALL
            .iter()
            .position(|&g| g == s.graphics)
            .unwrap_or(1);
        if ui.stepper(st.next(row), "Graphics", &mut gi, &gnames) {
            s.graphics = GraphicsPreset::ALL[gi];
        }
        let snames: Vec<String> = UI_SCALES.iter().map(|v| format!("{v}%")).collect();
        let srefs: Vec<&str> = snames.iter().map(String::as_str).collect();
        let mut si = UI_SCALES.iter().position(|&v| v == s.ui_scale).unwrap_or(2);
        if ui.stepper(st.next(row), "UI scale", &mut si, &srefs) {
            s.ui_scale = UI_SCALES[si];
        }
        ui.toggle(st.next(row), "V-sync", &mut s.vsync);
        let cnames: Vec<String> = FRAME_CAPS
            .iter()
            .map(|&c| {
                if c == 0 {
                    "No cap".to_string()
                } else {
                    format!("{c} fps")
                }
            })
            .collect();
        let crefs: Vec<&str> = cnames.iter().map(String::as_str).collect();
        let mut ci = FRAME_CAPS
            .iter()
            .position(|&c| c == s.frame_cap)
            .unwrap_or(1);
        if ui.stepper(st.next(row), "Frame cap", &mut ci, &crefs) {
            s.frame_cap = FRAME_CAPS[ci];
        }

        // Right column: audio and accessibility.
        let mut st = Stack {
            x: panel.x + 48 + col_w,
            y: top,
            w: col_w,
            gap,
        };
        ui.label(st.x, st.y, 1, theme::ACCENT, "AUDIO");
        st.skip(sp(22));
        ui.slider(st.next(row), "Master", &mut s.volume_master);
        ui.slider(st.next(row), "Music", &mut s.volume_music);
        ui.slider(st.next(row), "Effects", &mut s.volume_effects);
        ui.slider(st.next(row), "Unit voices", &mut s.volume_voices);
        ui.slider(st.next(row), "Interface", &mut s.volume_ui);
        st.skip(sp(10));
        ui.label(st.x, st.y, 1, theme::ACCENT, "ACCESSIBILITY");
        st.skip(sp(22));
        ui.toggle(
            st.next(row),
            "Colour-blind team palette",
            &mut s.colorblind_palette,
        );
        ui.toggle(
            st.next(row),
            "High-contrast banners",
            &mut s.high_contrast_banners,
        );
        ui.toggle(st.next(row), "Larger text", &mut s.larger_text);
        ui.toggle(st.next(row), "Reduced screen shake", &mut s.reduced_shake);
        ui.toggle(st.next(row), "Contextual hints", &mut s.hints);

        let note_y = panel.bottom() - 52 - 44;
        ui.label(
            panel.x + 24,
            note_y,
            1,
            theme::MUTED,
            "Name, colour, faction, UI scale and the Master and Music volume work now. The",
        );
        ui.label(
            panel.x + 24,
            note_y + 18,
            1,
            theme::MUTED,
            "other audio, graphics and accessibility choices are saved and take effect later.",
        );
        let back = Rect::new(panel.x + 24, panel.bottom() - 52, 140, 44);
        let done = ui.button(back, "Back", ButtonStyle::Primary, true);
        let changed = self.settings != before;
        let mut action = if changed {
            MenuAction::SettingsChanged
        } else {
            MenuAction::None
        };
        if done || escape {
            self.save_settings();
            self.go(ui, Screen::Main);
        }
        if changed && matches!(action, MenuAction::SettingsChanged) {
            action = MenuAction::SettingsChanged;
        }
        action
    }
}

/// How tall the lobby's player rows are and how big the map picture is
/// (its side in pixels, 0 for none), on a screen `screen_h` tall at scale
/// `k`. The picture gets 96 pixels (scaled) when there is room; the rows
/// give up a little height first, then the picture gets smaller, and on a
/// short screen the picture goes and only the map code row remains.
fn lobby_sizes(screen_h: i32, k: f32) -> (i32, i32) {
    let sp = |n: i32| (n as f32 * k).round() as i32;
    let avail = screen_h - 24;
    // Everything but the rows and the map strip: header, settings, gaps
    // and the footer.
    let fixed = sp(64 + 36 + 20 + 4) + 124;
    let (full_row, min_row, gap) = (sp(56), sp(48), sp(6));
    let total = |row: i32, strip: i32| fixed + strip + MAX_PLAYERS as i32 * (row + gap);
    let code_row = sp(36);
    // The tallest rows that fit under a strip `strip` tall.
    let rows_for = |strip: i32| {
        let over = (total(full_row, strip) - avail).max(0);
        let give = (over + MAX_PLAYERS as i32 - 1) / MAX_PLAYERS as i32;
        full_row - give.min(full_row - min_row)
    };
    let want = sp(96);
    let row = rows_for(want);
    let side = want - (total(row, want) - avail).max(0);
    if side >= sp(64) {
        (row, side)
    } else {
        (rows_for(code_row), 0)
    }
}

/// "3 regiments, 1,400 of 4,000 points".
fn army_summary(army_len: usize, spent: u32, budget: u32) -> String {
    format!(
        "{army_len} regiments, {} of {} points",
        thousands(spent),
        thousands(budget)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(choice: usize, tiles: u32, seed: u64) -> MapPreviewKey {
        MapPreviewKey::Field {
            choice,
            tiles,
            seed,
        }
    }

    fn seed_of(key: MapPreviewKey) -> u64 {
        match key {
            MapPreviewKey::Field { seed, .. } => seed,
            MapPreviewKey::Conquest(_) => panic!("a Conquest map has no seed"),
        }
    }
    use crate::draw::{DrawList, SOLID};
    use crate::lobby::{
        DiscoveredGame, LobbyState, MAPS, MatchStart, NetError, NetEvent, Profile, SlotEdit,
    };
    use crate::ui::UiInput;

    struct Harness {
        menu: Menu,
        ui: Ui,
        last: MenuAction,
    }

    /// Where `needle` is drawn (centre of its glyphs), found in the quads.
    fn find_all(draw: &DrawList, needle: &str) -> Vec<(i32, i32)> {
        let mut found = Vec::new();
        let want: Vec<u32> = needle.bytes().map(|b| u32::from(b - 32)).collect();
        let q = &draw.quads;
        'start: for i in 0..q.len() {
            if q[i].glyph == SOLID {
                continue;
            }
            let mut j = i;
            let mut k = 0;
            let mut last_x = None;
            while k < want.len() {
                if want[k] == 0 {
                    // A space: no quad. Skip one cell.
                    k += 1;
                    continue;
                }
                let Some(c) = q.get(j) else { continue 'start };
                if c.glyph != want[k] || last_x.is_some_and(|x| c.pos[0] <= x) {
                    continue 'start;
                }
                last_x = Some(c.pos[0]);
                j += 1;
                k += 1;
            }
            let (a, b) = (q[i], q[j - 1]);
            found.push((
                (i32::from(a.pos[0]) + i32::from(b.pos[0]) + i32::from(b.size[0])) / 2,
                i32::from(a.pos[1]) + i32::from(a.size[1]) / 2,
            ));
        }
        found
    }

    fn find_text(draw: &DrawList, needle: &str) -> Option<(i32, i32)> {
        find_all(draw, needle).first().copied()
    }

    impl Harness {
        fn new() -> Harness {
            let mut h = Harness {
                menu: Menu::new(LocalNetwork::new(), Settings::default(), None, "0.1.0"),
                ui: Ui::new(),
                last: MenuAction::None,
            };
            h.frame(UiInput::default());
            h
        }

        fn frame(&mut self, input: UiInput) -> &MenuAction {
            self.ui.begin_frame(input, (1280, 760));
            self.last = self.menu.frame(&mut self.ui);
            self.ui.end_frame();
            &self.last
        }

        fn text_pos(&self, t: &str) -> (i32, i32) {
            find_text(&self.ui.draw, t).unwrap_or_else(|| panic!("no {t:?} on screen"))
        }

        /// Click the lowest thing labelled `t` (buttons sit under titles).
        fn click_last(&mut self, t: &str) {
            let (x, y) = *find_all(&self.ui.draw, t)
                .last()
                .unwrap_or_else(|| panic!("no {t:?}"));
            self.click_at(x, y);
        }

        fn has(&self, t: &str) -> bool {
            find_text(&self.ui.draw, t).is_some()
        }

        /// Click the thing labelled `t`, then redraw so the new screen shows.
        fn click(&mut self, t: &str) {
            let (x, y) = self.text_pos(t);
            self.click_at(x, y);
        }

        fn click_at(&mut self, x: i32, y: i32) {
            self.frame(UiInput {
                mouse: (x, y),
                pressed: true,
                released: true,
                ..Default::default()
            });
            self.frame(UiInput {
                mouse: (x, y),
                ..Default::default()
            });
        }

        fn key(&mut self, f: impl FnOnce(&mut UiInput)) {
            let mut i = UiInput::default();
            f(&mut i);
            self.frame(i);
            self.frame(UiInput::default());
        }
    }

    #[test]
    fn main_menu_has_the_expected_buttons() {
        let h = Harness::new();
        for t in ["GRUBCRAFT", "Play", "Sandbox", "Settings", "Exit"] {
            assert!(h.has(t) || h.has("Quit"), "{t}");
        }
        assert!(!h.has("Resume"));
    }

    #[test]
    fn the_replays_screen_lists_replays_to_watch() {
        let mut h = Harness::new();
        let (x, y) = h.text_pos("Replays");
        h.frame(UiInput {
            mouse: (x, y),
            pressed: true,
            released: true,
            ..Default::default()
        });
        assert!(matches!(h.last, MenuAction::ListReplays));
        h.menu.replays_loading();
        h.frame(UiInput::default());
        assert!(h.has("Looking for replays") && !h.has("No replays yet"));
        h.menu.set_replays(Vec::new());
        h.frame(UiInput::default());
        assert!(h.has("No replays yet"));
        let entry = |n: usize, unplayable: Option<&str>| ReplayEntry {
            title: format!("Battle {n}"),
            detail: format!("detail {n}"),
            path: PathBuf::from(format!("{n}.gcr")),
            unplayable: unplayable.map(str::to_string),
        };
        let mut list: Vec<ReplayEntry> = (1..=12).map(|n| entry(n, None)).collect();
        list[1] = entry(2, Some("Made with other game data"));
        h.menu.set_replays(list);
        h.frame(UiInput::default());
        assert!(h.has("Battle 1") && h.has("detail 1") && h.has("Made with other game data"));
        // One page at a time.
        assert!(!h.has("Battle 12"));
        h.click("Older");
        assert!(h.has("Battle 12") && !h.has("Battle 3"));
        h.click("Newer");
        let (x, y) = *find_all(&h.ui.draw, "Watch").first().unwrap();
        h.frame(UiInput {
            mouse: (x, y),
            pressed: true,
            released: true,
            ..Default::default()
        });
        assert!(matches!(&h.last, MenuAction::WatchReplay(p) if p == &PathBuf::from("1.gcr")));
        h.menu.tell("Can't open that replay".into());
        h.frame(UiInput::default());
        assert!(h.has("Can't open that replay"));
        h.key(|i| i.escape = true);
        assert_eq!(h.menu.screen(), Screen::Main);
    }

    #[test]
    fn sandbox_and_quit_report_actions() {
        let mut h = Harness::new();
        let (x, y) = h.text_pos("Sandbox");
        h.frame(UiInput {
            mouse: (x, y),
            pressed: true,
            released: true,
            ..Default::default()
        });
        assert!(matches!(h.last, MenuAction::StartSandbox));
        let quit = if cfg!(target_os = "macos") {
            "Quit"
        } else {
            "Exit"
        };
        let (x, y) = h.text_pos(quit);
        h.frame(UiInput {
            mouse: (x, y),
            pressed: true,
            released: true,
            ..Default::default()
        });
        assert!(matches!(h.last, MenuAction::Quit));
    }

    #[test]
    fn the_unit_viewer_button_asks_the_game_to_open_the_viewer() {
        let mut h = Harness::new();
        assert!(h.has("Unit viewer"));
        let (x, y) = h.text_pos("Unit viewer");
        h.frame(UiInput {
            mouse: (x, y),
            pressed: true,
            released: true,
            ..Default::default()
        });
        assert!(matches!(h.last, MenuAction::OpenViewer));
        // The menu stays on the main screen; the game shows the viewer over it.
        assert_eq!(h.menu.screen(), Screen::Main);
    }

    #[test]
    fn enter_opens_play_from_the_main_menu() {
        let mut h = Harness::new();
        h.key(|i| i.enter = true);
        assert_eq!(h.menu.screen(), Screen::Play);
    }

    #[test]
    fn in_a_match_the_menu_offers_resume_and_escape_resumes() {
        let mut h = Harness::new();
        h.menu.set_in_match(true);
        h.frame(UiInput::default());
        assert!(h.has("Resume"));
        h.frame(UiInput {
            escape: true,
            ..Default::default()
        });
        assert!(matches!(h.last, MenuAction::Resume));
    }

    #[test]
    fn host_flow_reaches_a_lobby_and_starts_a_match_with_ai() {
        let mut h = Harness::new();
        h.click("Play");
        assert_eq!(h.menu.screen(), Screen::Play);
        h.click_last("Host");
        assert_eq!(h.menu.screen(), Screen::Lobby);
        assert!(h.has("Mario") || h.has("Player"));
        assert!(h.has("Add at least one more player"));
        // Start is disabled while alone: clicking it does nothing.
        h.click("Start");
        assert!(!matches!(h.last, MenuAction::StartMatch(_)));
        h.click("Add AI");
        assert!(h.has("AI 2"));
        assert!(h.has("Everyone is ready."));
        // The AI starts as a Veteran; the host can make it a General.
        assert!(h.has("Veteran"));
        h.click("Veteran");
        assert!(h.has("General"));
        // Champions are allowed unless the host says otherwise.
        let champions = |h: &Harness| h.menu.net.lobby().unwrap().settings.champions;
        assert!(champions(&h));
        h.click("Champions before AI battles");
        assert!(!champions(&h));
        let (x, y) = h.text_pos("Start");
        h.frame(UiInput {
            mouse: (x, y),
            pressed: true,
            released: true,
            ..Default::default()
        });
        match &h.last {
            MenuAction::StartMatch(m) => {
                assert_eq!(m.players.len(), 2);
                assert_eq!(m.local_player, 0);
                assert_eq!(AI_LEVELS[m.players[1].ai_level as usize], "General");
                assert!(!m.settings.champions);
            }
            other => panic!("expected StartMatch, got {other:?}"),
        }
    }

    #[test]
    fn the_host_picks_which_conquest_victories_count() {
        let mut h = Harness::new();
        h.click("Play");
        // The host card: both count unless the host says otherwise, and
        // the stepper wraps through the three choices.
        assert!(h.has("Victory: Both"));
        h.click("Victory: Both");
        assert!(h.has("Victory: Annihilation"));
        h.click("Victory: Annihilation");
        assert!(h.has("Victory: Titan's Heart"));
        h.click("Victory: Titan's Heart");
        assert!(h.has("Victory: Both"));
        h.click("Victory: Both");
        h.click_last("Host");
        assert_eq!(h.menu.screen(), Screen::Lobby);
        let victory = |h: &Harness| h.menu.net.lobby().unwrap().settings.victory;
        assert_eq!(victory(&h), Victory::Annihilation);
        // A Field Battle lobby has no Victory row; Conquest does.
        assert!(!h.has("Victory: Annihilation"));
        h.click("Mode:");
        assert!(h.has("Mode: Conquest"));
        assert!(h.has("Victory: Annihilation"));
        h.click("Victory: Annihilation");
        assert_eq!(victory(&h), Victory::Heart);
        assert!(h.has("Victory: Titan's Heart"));
        // It sits in the header, left of the host's name and above the
        // settings row.
        let (vx, vy) = h.text_pos("Victory: Titan's Heart");
        let (hx, _) = h.text_pos("Host:");
        let (_, my) = h.text_pos("Mode: Conquest");
        assert!(vx + 88 < hx && vy < my);
    }

    #[test]
    fn a_joiner_sees_which_conquest_victories_count_as_text() {
        let mut local = LocalNetwork::new();
        let settings = LobbySettings {
            mode: GameMode::Conquest,
            victory: Victory::Annihilation,
            ..Default::default()
        };
        let host = Menu::new(LocalNetwork::new(), Settings::default(), None, "0.1.0").profile();
        local.host(&host, settings, "").unwrap();
        let mut lobby = local.lobby().unwrap().clone();
        lobby.is_host = false;
        lobby.slots[1] = {
            let mut s = lobby.slots[0].clone();
            s.name = "Guest".into();
            s
        };
        lobby.local_slot = 1;
        let mut menu = Menu::new(Guest(lobby), Settings::default(), None, "0.1.0");
        let mut ui = Ui::new();
        menu.show(Screen::Lobby);
        ui.begin_frame(UiInput::default(), (1280, 760));
        menu.frame(&mut ui);
        ui.end_frame();
        assert!(find_text(&ui.draw, "Victory: Annihilation").is_some());
        assert!(find_text(&ui.draw, "Tempo: Standard").is_some());
    }

    #[test]
    fn the_army_builder_spends_the_points_budget() {
        let mut h = Harness::new();
        let unit = |name: &str, faction, points| RosterUnit {
            name: name.into(),
            faction,
            points,
        };
        h.menu.set_roster(vec![
            unit("Yari Ashigaru", Faction::EmberClans, 400),
            unit("Bone Pikes", Faction::CarrionConcord, 400),
            unit("Yumi Ashigaru", Faction::EmberClans, 600),
        ]);
        h.menu.settings.faction = Faction::EmberClans;
        h.click("Play");
        h.click_last("Host");
        assert!(h.has("Points: 4,000"));
        assert!(h.has("Army: picked for you"));
        h.click_last("Army");
        assert_eq!(h.menu.screen(), Screen::Army);
        assert!(h.has("Yari Ashigaru") && h.has("Yumi Ashigaru"));
        assert!(!h.has("Bone Pikes"), "only your faction's regiments");
        assert!(h.has("the Balanced mix will be picked for you"));
        let plus = |h: &Harness| find_all(&h.ui.draw, "+");
        for _ in 0..3 {
            let (x, y) = plus(&h)[0];
            h.click_at(x, y);
        }
        let (x, y) = plus(&h)[1];
        h.click_at(x, y);
        assert!(h.has("4 regiments, 1,800 of 4,000 points"), "spent");
        // One spearmen fewer.
        let (x, y) = find_all(&h.ui.draw, "-")[0];
        h.click_at(x, y);
        assert!(h.has("3 regiments, 1,400 of 4,000 points"));
        h.click("Done");
        assert_eq!(h.menu.screen(), Screen::Lobby);
        assert!(h.has("Army: 3 regiments, 1,400 of 4,000 points."));
        h.click("Add AI");
        let (x, y) = h.text_pos("Start");
        h.frame(UiInput {
            mouse: (x, y),
            pressed: true,
            released: true,
            ..Default::default()
        });
        match &h.last {
            MenuAction::StartMatch(m) => {
                assert_eq!(m.players[0].army, vec![0, 0, 2]);
                assert!(m.players[1].army.is_empty(), "the AI picks its own");
                assert_eq!(m.settings.budget, Budget::Battle);
            }
            other => panic!("expected StartMatch, got {other:?}"),
        }
    }

    #[test]
    fn a_budget_that_runs_out_stops_the_builder() {
        let mut h = Harness::new();
        h.menu.set_roster(vec![RosterUnit {
            name: "Yari Ashigaru".into(),
            faction: Faction::EmberClans,
            points: 1_500,
        }]);
        h.menu.settings.faction = Faction::EmberClans;
        h.click("Play");
        h.click_last("Host");
        h.click("Points: 4,000");
        assert!(h.has("Points: 8,000"), "the host steps the budget");
        h.click_last("Army");
        for _ in 0..7 {
            let (x, y) = find_all(&h.ui.draw, "+")[0];
            h.click_at(x, y);
        }
        // 8,000 buys five at 1,500; the sixth doesn't fit.
        assert!(h.has("5 regiments, 7,500 of 8,000 points"));
    }

    #[test]
    fn a_six_player_lobby_fits_all_rows_above_the_footer() {
        for screen in [(1280, 720), (1280, 760), (960, 540)] {
            let mut h = Harness::new();
            h.click("Play");
            h.click_last("Host");
            for _ in 0..MAX_PLAYERS - 1 {
                h.click("Add AI");
            }
            assert!(
                h.menu
                    .net
                    .lobby()
                    .unwrap()
                    .slots
                    .iter()
                    .all(crate::lobby::Slot::occupied)
            );
            h.ui.begin_frame(UiInput::default(), screen);
            h.menu.frame(&mut h.ui);
            h.ui.end_frame();
            assert!(h.has("AI 6"));
            let (_, last_row) = h.text_pos("AI 6");
            let (_, leave) = h.text_pos("Leave");
            let (_, hint) = h.text_pos("Everyone is ready.");
            assert!(last_row < hint && hint < leave, "{screen:?}");
            assert!(leave + 22 <= screen.1, "{screen:?}");
        }
    }

    #[test]
    fn leaving_the_lobby_returns_to_play() {
        let mut h = Harness::new();
        h.click("Play");
        h.click_last("Host");
        h.click("Leave");
        assert_eq!(h.menu.screen(), Screen::Play);
        assert!(h.menu.net.lobby().is_none());
    }

    #[test]
    fn conquest_can_be_hosted_on_its_own_maps() {
        let mut h = Harness::new();
        h.click("Play");
        h.click("Mode:");
        assert!(h.has("Mode: Conquest"));
        assert!(h.has("Map: Ribcage Pass"));
        h.click("Map: Ribcage Pass");
        assert!(h.has("Map: The Four Teeth"));
        // Back to Field Battle: the map stays in range.
        h.click("Mode: Conquest");
        assert!(h.has("Map: Random: Pass"));
        h.click("Mode:");
        h.click_last("Host");
        assert_eq!(h.menu.screen(), Screen::Lobby);
        let lobby = h.menu.net.lobby().unwrap();
        assert_eq!(lobby.settings.mode, GameMode::Conquest);
        assert_eq!(lobby.settings.map_name(), "The Four Teeth");
    }

    #[test]
    fn play_screen_explains_that_joining_is_not_built() {
        let mut h = Harness::new();
        h.click("Play");
        assert!(h.has("No games found yet"));
        // Join is disabled until something is typed.
        h.click_last("Join");
        assert!(!h.has("Joining friends"));
        let (x, y) = h.text_pos("Address or join code");
        h.click_at(x, y);
        h.frame(UiInput {
            typed: "192.168.1.9:7777".into(),
            ..Default::default()
        });
        h.frame(UiInput::default());
        assert!(h.has("192.168.1.9:7777"));
        h.click_last("Join");
        assert!(h.has("Joining friends needs networking"));
        assert_eq!(h.menu.screen(), Screen::Play);
    }

    #[test]
    fn settings_changes_are_reported_and_saved_on_back() {
        let dir = std::env::temp_dir().join(format!("gc_ui_menu_{}", std::process::id()));
        let path = dir.join("settings.ron");
        let mut h = Harness::new();
        h.menu.settings_path = Some(path.clone());
        h.click("Settings");
        h.click("UI scale: 100%");
        assert!(h.menu.settings().ui_scale != 100);
        h.click("Larger text");
        assert!(h.menu.settings().larger_text);
        h.click("Back");
        assert_eq!(h.menu.screen(), Screen::Main);
        let saved = Settings::load(&path);
        assert!(saved.larger_text);
        assert_eq!(saved.ui_scale, h.menu.settings().ui_scale);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn typing_a_name_in_settings_updates_the_profile() {
        let mut h = Harness::new();
        h.click("Settings");
        let (x, y) = h.text_pos("Player");
        h.click_at(x, y);
        // Clicking the text of the field itself: it is the prefilled name.
        h.frame(UiInput {
            backspace: true,
            ..Default::default()
        });
        h.frame(UiInput {
            typed: "x".into(),
            ..Default::default()
        });
        h.frame(UiInput::default());
        let name = h.menu.settings().player_name.clone();
        assert!(name.ends_with('x'), "{name}");
    }

    #[test]
    fn escape_goes_back_one_screen() {
        let mut h = Harness::new();
        h.click("Play");
        h.key(|i| i.escape = true);
        assert_eq!(h.menu.screen(), Screen::Main);
        h.click("Settings");
        assert_eq!(h.menu.screen(), Screen::Settings);
        h.key(|i| i.escape = true);
        assert_eq!(h.menu.screen(), Screen::Main);
    }

    /// A practice network that also hears games on the "LAN".
    #[derive(Default)]
    struct Found {
        inner: LocalNetwork,
        games: Vec<DiscoveredGame>,
    }

    impl Network for Found {
        fn host(&mut self, me: &Profile, s: LobbySettings, pw: &str) -> Result<(), NetError> {
            self.inner.host(me, s, pw)
        }
        fn discovered(&mut self) -> &[DiscoveredGame] {
            &self.games
        }
        fn join(&mut self, me: &Profile, target: &str, pw: &str) -> Result<(), NetError> {
            self.inner.join(me, target, pw)
        }
        fn lobby(&self) -> Option<&LobbyState> {
            self.inner.lobby()
        }
        fn edit_slot(&mut self, slot: usize, edit: SlotEdit) {
            self.inner.edit_slot(slot, edit);
        }
        fn edit_settings(&mut self, s: LobbySettings) {
            self.inner.edit_settings(s);
        }
        fn start_match(&mut self) -> Result<MatchStart, NetError> {
            self.inner.start_match()
        }
        fn leave(&mut self) {
            self.inner.leave();
        }
        fn poll(&mut self) -> Vec<NetEvent> {
            self.inner.poll()
        }
    }

    fn game(n: u8) -> DiscoveredGame {
        DiscoveredGame {
            host: format!("Host{n}"),
            map: "Marsh".into(),
            players: 1,
            max_players: 4,
            version: "0.1.0".into(),
            locked: false,
            playing: false,
            address: format!("10.0.0.{n}:7777"),
        }
    }

    fn found_frame(menu: &mut Menu<Found>, ui: &mut Ui, input: UiInput) {
        ui.begin_frame(input, (1280, 760));
        menu.frame(ui);
        ui.end_frame();
    }

    #[test]
    fn a_game_appearing_or_expiring_does_not_move_text_focus() {
        let mut menu = Menu::new(Found::default(), Settings::default(), None, "0.1.0");
        let mut ui = Ui::new();
        menu.show(Screen::Play);
        found_frame(&mut menu, &mut ui, UiInput::default());
        let (x, y) = find_text(&ui.draw, "Address or join code").unwrap();
        let click = UiInput {
            mouse: (x, y),
            pressed: true,
            released: true,
            ..Default::default()
        };
        found_frame(&mut menu, &mut ui, click);
        let typed = |s: &str| UiInput {
            mouse: (x, y),
            typed: s.into(),
            ..Default::default()
        };
        found_frame(&mut menu, &mut ui, typed("10."));
        // Games appear and expire between keystrokes.
        for games in [1, 3, 0, 2] {
            menu.net.games = (1..=games).map(game).collect();
            found_frame(&mut menu, &mut ui, typed("1"));
        }
        assert_eq!(menu.address, "10.1111");
        assert!(
            menu.join_password.is_empty(),
            "nothing leaked into the password"
        );
        assert!(ui.text_focused());
    }

    #[test]
    fn a_selected_game_is_kept_by_address_when_the_list_changes() {
        let mut menu = Menu::new(Found::default(), Settings::default(), None, "0.1.0");
        let mut ui = Ui::new();
        menu.show(Screen::Play);
        menu.net.games = vec![game(1), game(2)];
        found_frame(&mut menu, &mut ui, UiInput::default());
        let (x, y) = find_text(&ui.draw, "Host2").unwrap();
        let click = UiInput {
            mouse: (x, y),
            pressed: true,
            released: true,
            ..Default::default()
        };
        found_frame(&mut menu, &mut ui, click);
        assert_eq!(menu.selected_game.as_deref(), Some("10.0.0.2:7777"));
        // The first game expires: the pick is still the same game.
        menu.net.games = vec![game(2)];
        found_frame(&mut menu, &mut ui, UiInput::default());
        assert_eq!(menu.selected_game.as_deref(), Some("10.0.0.2:7777"));
    }

    /// Where the picture of a map shows: a flat colour nothing else uses.
    const PICTURE: [u8; 4] = [1, 2, 3, 255];

    fn picture_quads(draw: &DrawList) -> usize {
        draw.quads.iter().filter(|q| q.color == PICTURE).count()
    }

    fn send_picture(menu: &mut Menu) {
        let key = menu.wanted_map_preview().expect("a picture is wanted");
        let terrain = MinimapTerrain::from_fn(8, |_, _| PICTURE);
        menu.set_map_preview(key, terrain, vec![[0.25, 0.25], [0.75, 0.75]]);
    }

    /// Type `text` into the focused field.
    fn type_text(h: &mut Harness, text: &str) {
        h.frame(UiInput {
            typed: text.into(),
            ..Default::default()
        });
    }

    fn lobby_seed(h: &Harness) -> u64 {
        h.menu.net.lobby().unwrap().settings.map_seed
    }

    /// Click in the map code box and empty it.
    fn clear_code_box(h: &mut Harness) {
        let code = map_code(lobby_seed(h));
        h.click(&code);
        for _ in 0..24 {
            h.frame(UiInput {
                backspace: true,
                ..Default::default()
            });
        }
    }

    #[test]
    fn the_lobby_shows_the_map_picture_code_and_new_map_button() {
        let mut h = Harness::new();
        h.click("Play");
        h.click_last("Host");
        assert_eq!(h.menu.screen(), Screen::Lobby);
        assert!(h.has("New map") && h.has("Map code") && h.has("0000-0000-6772-7562"));
        // The lobby asks for the map it is on, sized for the players in it.
        let seed = crate::lobby::DEFAULT_MAP_SEED;
        assert_eq!(h.menu.wanted_map_preview(), Some(field(0, 256, seed)));
        // Up to four players share a map, so a new one changes nothing.
        h.click("Add AI");
        assert_eq!(h.menu.wanted_map_preview(), Some(field(0, 256, seed)));
        // Until the game answers there is a grey box, not a picture.
        assert_eq!(picture_quads(&h.ui.draw), 0);
        assert_eq!(h.menu.shown_map_preview(), None);
        send_picture(&mut h.menu);
        h.frame(UiInput::default());
        assert_eq!(h.menu.shown_map_preview(), Some(field(0, 256, seed)));
        assert!(picture_quads(&h.ui.draw) > 0);
        // A picture of some other map is not shown.
        h.menu
            .set_map_preview(field(1, 256, seed), MinimapTerrain::default(), Vec::new());
        h.frame(UiInput::default());
        assert_eq!(h.menu.shown_map_preview(), None);
        assert_eq!(picture_quads(&h.ui.draw), 0);
    }

    #[test]
    fn the_lobby_picture_follows_the_map_choice_and_seed() {
        let mut h = Harness::new();
        h.click("Play");
        h.click_last("Host");
        h.click("Map: Pro");
        let seed = |h: &Harness| match h.menu.wanted_map_preview() {
            Some(MapPreviewKey::Field { seed, .. }) => seed,
            other => panic!("{other:?}"),
        };
        let wanted = h.menu.wanted_map_preview().unwrap();
        let MapPreviewKey::Field { choice, .. } = wanted else {
            panic!("{wanted:?}")
        };
        assert_eq!(MAPS[choice], "Random: Pass");
        let before = seed(&h);
        h.click("New map");
        assert_ne!(seed(&h), before);
        assert_eq!(seed(&h), lobby_seed(&h));
        // Out of the lobby, nothing is wanted.
        h.click("Leave");
        h.click("Back");
        assert_eq!(h.menu.wanted_map_preview(), None);
    }

    #[test]
    fn new_map_in_the_lobby_changes_the_seed_and_the_code() {
        let mut h = Harness::new();
        h.click("Play");
        h.click_last("Host");
        let before = lobby_seed(&h);
        h.click("New map");
        let after = lobby_seed(&h);
        assert_ne!(before, after);
        assert!(h.has(&map_code(after)));
        assert!(!h.has(&map_code(before)));
        h.click("New map");
        assert_ne!(lobby_seed(&h), after);
    }

    #[test]
    fn the_host_types_in_a_map_code() {
        let mut h = Harness::new();
        h.click("Play");
        h.click_last("Host");
        clear_code_box(&mut h);
        type_text(&mut h, "deadbeef01234567");
        // Nothing happens until the code is used.
        assert_eq!(lobby_seed(&h), crate::lobby::DEFAULT_MAP_SEED);
        h.click("Use");
        assert_eq!(lobby_seed(&h), 0xDEAD_BEEF_0123_4567);
        assert!(h.has("DEAD-BEEF-0123-4567"));
        // Enter does it too.
        clear_code_box(&mut h);
        type_text(&mut h, "0000 0000 0000 00ff");
        h.frame(UiInput {
            enter: true,
            ..Default::default()
        });
        assert_eq!(lobby_seed(&h), 0xFF);
        assert!(!h.has("Not a map code"));
    }

    #[test]
    fn a_map_code_that_is_not_one_is_ignored_with_a_message() {
        let mut h = Harness::new();
        h.click("Play");
        h.click_last("Host");
        h.click("Add AI");
        assert!(h.has("Everyone is ready."));
        clear_code_box(&mut h);
        type_text(&mut h, "not a code");
        // Enter in the box submits the code, not the match.
        h.frame(UiInput {
            enter: true,
            ..Default::default()
        });
        assert!(!matches!(h.last, MenuAction::StartMatch(_)));
        assert_eq!(lobby_seed(&h), crate::lobby::DEFAULT_MAP_SEED);
        assert!(h.has("Not a map code"));
        // Typing again takes the message away.
        h.click("not a code");
        type_text(&mut h, "x");
        assert!(!h.has("Not a map code"));
    }

    /// A lobby that somebody else hosts.
    struct Guest(LobbyState);

    impl Network for Guest {
        fn host(&mut self, _: &Profile, _: LobbySettings, _: &str) -> Result<(), NetError> {
            Ok(())
        }
        fn discovered(&mut self) -> &[DiscoveredGame] {
            &[]
        }
        fn join(&mut self, _: &Profile, _: &str, _: &str) -> Result<(), NetError> {
            Ok(())
        }
        fn lobby(&self) -> Option<&LobbyState> {
            Some(&self.0)
        }
        fn edit_slot(&mut self, _: usize, _: SlotEdit) {}
        fn edit_settings(&mut self, _: LobbySettings) {}
        fn start_match(&mut self) -> Result<MatchStart, NetError> {
            Err(NetError::Unavailable("guest".into()))
        }
        fn leave(&mut self) {}
        fn poll(&mut self) -> Vec<NetEvent> {
            Vec::new()
        }
    }

    #[test]
    fn a_joiner_sees_the_map_code_and_picture_but_cannot_change_them() {
        let mut local = LocalNetwork::new();
        let settings = LobbySettings {
            map: 2,
            map_seed: 0xABCD_EF01_2345_6789,
            ..Default::default()
        };
        let host = Menu::new(LocalNetwork::new(), Settings::default(), None, "0.1.0").profile();
        local.host(&host, settings, "").unwrap();
        let mut lobby = local.lobby().unwrap().clone();
        lobby.is_host = false;
        lobby.slots[1] = {
            let mut s = lobby.slots[0].clone();
            s.name = "Guest".into();
            s
        };
        lobby.local_slot = 1;
        let mut menu = Menu::new(Guest(lobby), Settings::default(), None, "0.1.0");
        let mut ui = Ui::new();
        menu.show(Screen::Lobby);
        ui.begin_frame(UiInput::default(), (1280, 760));
        menu.frame(&mut ui);
        ui.end_frame();
        assert!(find_text(&ui.draw, "ABCD-EF01-2345-6789").is_some());
        assert!(find_text(&ui.draw, "Map code").is_some());
        assert!(find_text(&ui.draw, "Map: Random: Ford").is_some());
        assert!(find_text(&ui.draw, "New map").is_none());
        assert!(find_text(&ui.draw, "Use").is_none());
        assert_eq!(
            menu.wanted_map_preview(),
            Some(field(2, 256, 0xABCD_EF01_2345_6789))
        );
    }

    #[test]
    fn the_host_card_has_new_map_a_code_box_and_a_picture() {
        let mut h = Harness::new();
        h.click("Play");
        let seed = crate::lobby::DEFAULT_MAP_SEED;
        assert!(h.has("New map") && h.has(&map_code(seed)));
        assert_eq!(h.menu.wanted_map_preview(), Some(field(0, 256, seed)));
        send_picture(&mut h.menu);
        h.frame(UiInput::default());
        assert!(picture_quads(&h.ui.draw) > 0);
        h.click("New map");
        let fresh = h.menu.wanted_map_preview().map(seed_of).unwrap();
        assert_ne!(fresh, seed);
        assert!(h.has(&map_code(fresh)));
        // The new map's picture isn't here yet.
        assert_eq!(picture_quads(&h.ui.draw), 0);
        // A code typed in is used, a junk one is refused.
        h.click(&map_code(fresh));
        for _ in 0..24 {
            h.frame(UiInput {
                backspace: true,
                ..Default::default()
            });
        }
        type_text(&mut h, "0123456789abcdef");
        h.click("Use");
        assert_eq!(
            h.menu.wanted_map_preview().map(seed_of).unwrap(),
            0x0123_4567_89AB_CDEF
        );
        h.click("0123-4567-89AB-CDEF");
        h.frame(UiInput {
            backspace: true,
            ..Default::default()
        });
        h.click("Use");
        assert!(h.has("Not a map code"));
        assert_eq!(
            h.menu.wanted_map_preview().map(seed_of).unwrap(),
            0x0123_4567_89AB_CDEF
        );
        // The hosted lobby is on the chosen map.
        h.click_last("Host");
        assert_eq!(lobby_seed(&h), 0x0123_4567_89AB_CDEF);
    }

    #[test]
    fn a_starting_map_seed_can_be_set_for_the_host_card() {
        let mut h = Harness::new();
        h.menu.set_host_map_seed(12345);
        h.click("Play");
        assert_eq!(h.menu.wanted_map_preview(), Some(field(0, 256, 12345)));
        assert!(h.has(&map_code(12345)));
        h.click_last("Host");
        assert_eq!(lobby_seed(&h), 12345);
    }

    #[test]
    fn the_lobby_fits_with_the_picture_at_every_size() {
        for size in [
            (1280, 760),
            (1280, 720),
            (1024, 600),
            (960, 540),
            (853, 506),
        ] {
            let mut h = Harness::new();
            h.click("Play");
            h.click_last("Host");
            for _ in 0..MAX_PLAYERS - 1 {
                h.click("Add AI");
            }
            for round in 0..2 {
                h.ui.begin_frame(UiInput::default(), size);
                h.menu.frame(&mut h.ui);
                h.ui.end_frame();
                let wanted = h.menu.wanted_map_preview();
                if round == 0 {
                    // Six players: a bigger map.
                    assert!(
                        wanted.is_none_or(|w| matches!(w, MapPreviewKey::Field { tiles: 320, .. })),
                        "{size:?}"
                    );
                    if wanted.is_some() {
                        send_picture(&mut h.menu);
                    }
                } else if wanted.is_some() {
                    assert!(picture_quads(&h.ui.draw) > 0, "{size:?}");
                }
                for q in &h.ui.draw.quads {
                    let right = i32::from(q.pos[0]) + i32::from(q.size[0]);
                    let bottom = i32::from(q.pos[1]) + i32::from(q.size[1]);
                    assert!(right <= size.0 && bottom <= size.1, "{size:?}: {q:?}");
                }
                let (_, last_row) = h.text_pos("AI 6");
                let (_, code) = h.text_pos("Map code");
                let (_, leave) = h.text_pos("Leave");
                assert!(code < last_row && last_row < leave, "{size:?}");
            }
            // The picture is there at the common sizes and goes only on the
            // shortest windows.
            if size.1 >= 600 {
                assert!(h.menu.wanted_map_preview().is_some(), "{size:?}");
            }
        }
    }

    #[test]
    fn lobby_sizes_keep_the_rows_tall_enough_and_the_picture_big_enough() {
        for h in [506, 540, 600, 720, 760, 900, 1080, 1440] {
            let k = (h as f32 / 720.0).clamp(0.72, 1.0);
            let sp = |n: i32| (n as f32 * k).round() as i32;
            let (row, side) = lobby_sizes(h, k);
            assert!(row >= sp(48) && row <= sp(56), "{h}: rows {row}");
            assert!(
                side == 0 || (sp(64)..=sp(96)).contains(&side),
                "{h}: {side}"
            );
            let total =
                sp(64 + 36 + 20 + 4) + 124 + side.max(sp(36)) + MAX_PLAYERS as i32 * (row + sp(6));
            assert!(total <= h - 24, "{h}: {total} tall");
        }
        // A tall window gets everything at full size.
        let (row, side) = lobby_sizes(1080, 1.0);
        assert_eq!((row, side), (56, 96));
    }

    #[test]
    fn every_screen_draws_inside_the_window() {
        for (size, mode) in [(1280, 760), (853, 506), (1024, 600)]
            .into_iter()
            .flat_map(|size| [GameMode::FieldBattle, GameMode::Conquest].map(|m| (size, m)))
        {
            let mut h = Harness::new();
            h.menu.set_roster(vec![RosterUnit {
                name: "Yari Ashigaru".into(),
                faction: h.menu.settings.faction,
                points: 400,
            }]);
            h.menu
                .net
                .host(
                    &h.menu.profile(),
                    LobbySettings {
                        mode,
                        ..Default::default()
                    },
                    "",
                )
                .unwrap();
            for screen in [
                Screen::Main,
                Screen::Play,
                Screen::Lobby,
                Screen::Army,
                Screen::Settings,
            ] {
                h.menu.show(screen);
                h.ui.begin_frame(UiInput::default(), size);
                h.menu.frame(&mut h.ui);
                h.ui.end_frame();
                for q in &h.ui.draw.quads {
                    let right = i32::from(q.pos[0]) + i32::from(q.size[0]);
                    let bottom = i32::from(q.pos[1]) + i32::from(q.size[1]);
                    assert!(
                        right <= size.0 && bottom <= size.1,
                        "{screen:?} {size:?} {mode:?} overflows: {q:?}"
                    );
                }
            }
        }
    }
}
