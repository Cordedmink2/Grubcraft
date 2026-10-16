//! The main menu, Play (host or join), lobby, replays and settings screens.
//! See docs/08-ui-ux.md#menus-and-lobby.

use std::path::PathBuf;

use crate::draw::{Rect, fit};
use crate::lobby::{
    AI_LEVELS, Budget, Faction, GameMode, LobbySettings, LocalNetwork, MAPS, MAX_ARMY, MatchStart,
    NetEvent, Network, OnDrop, PLAYER_COLORS, Profile, RosterUnit, SlotEdit, SlotKind, Tempo,
    army_points, thousands,
};
use crate::settings::{FRAME_CAPS, GraphicsPreset, Settings, UI_SCALES};
use crate::ui::{ButtonStyle, Ui, theme};

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
    Quit,
}

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
    selected_game: Option<usize>,
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
        }
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
        ui.reset();
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
        for event in self.net.poll() {
            match event {
                NetEvent::Started(start) => return MenuAction::StartMatch(start),
                NetEvent::Left(why) => {
                    self.screen = Screen::Play;
                    self.message = Some((why, true));
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
        let button_h = 52;
        let count = if self.in_match { 6 } else { 5 };
        let total = 120 + count * (button_h + 14);
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
            gap: 14,
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
        let panel = centered(ui.screen, 880, sp(470));
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
        ui.label(
            s.x,
            s.y,
            1,
            theme::MUTED,
            "Friends on your Wi-Fi see it automatically.",
        );
        s.skip(sp(34));
        let mut map = self.host.map;
        if ui.stepper(s.next(sp(36)), "Map", &mut map, &MAPS) {
            self.host.map = map;
        }
        let mode_names: Vec<String> = GameMode::ALL
            .iter()
            .map(|m| {
                if m.playable() {
                    m.name().to_string()
                } else {
                    format!("{} (soon)", m.name())
                }
            })
            .collect();
        let mode_refs: Vec<&str> = mode_names.iter().map(String::as_str).collect();
        let mut mode = GameMode::ALL
            .iter()
            .position(|&m| m == self.host.mode)
            .unwrap_or(0);
        if ui.stepper(s.next(sp(36)), "Mode", &mut mode, &mode_refs) {
            self.host.mode = GameMode::ALL[mode];
        }
        let tempo_names: Vec<&str> = Tempo::ALL.iter().map(|t| t.name()).collect();
        let mut tempo = Tempo::ALL
            .iter()
            .position(|&t| t == self.host.tempo)
            .unwrap_or(0);
        if ui.stepper(s.next(sp(36)), "Tempo", &mut tempo, &tempo_names) {
            self.host.tempo = Tempo::ALL[tempo];
        }
        let mut password = std::mem::take(&mut self.host_password);
        ui.text_field(s.next(sp(36)), &mut password, 24, "Password (optional)");
        self.host_password = password;
        s.skip(sp(8));
        let host_btn = Rect::new(s.x, panel.bottom() - sp(64) - 60, card_w, 52);
        let playable = self.host.mode.playable();
        if ui.button(host_btn, "Host", ButtonStyle::Primary, playable) {
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
        for (i, g) in games.iter().take(3).enumerate() {
            let row = Rect::new(
                list.x + 4,
                list.y + 4 + i as i32 * sp(34),
                list.w - 8,
                sp(30),
            );
            if ui.row(row, self.selected_game == Some(i)) {
                self.selected_game = Some(i);
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
            let bad = games
                .get(i)
                .is_some_and(|g| !crate::lobby::plays_with(&g.version, &self.version));
            if bad {
                ui.label_right(row.right() - 8, ty, 1, theme::BAD, &g.version);
            }
        }
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
        // A game listed without a password is joined without one, whatever
        // is left in the box.
        let (target, password) = match self.selected_game.and_then(|i| games.get(i)) {
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
        let panel = centered(ui.screen, 880, sp(560));
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
        const CHAMPIONS: &str = "Champions before AI battles";
        let mut changed = false;
        if lobby.is_host {
            let mut map = settings.map;
            if ui.stepper(slot_rect(0), "Map", &mut map, &MAPS) {
                settings.map = map;
                changed = true;
            }
            let mode_names: Vec<String> = GameMode::ALL
                .iter()
                .map(|m| {
                    if m.playable() {
                        m.name().to_string()
                    } else {
                        format!("{} (soon)", m.name())
                    }
                })
                .collect();
            let mode_refs: Vec<&str> = mode_names.iter().map(String::as_str).collect();
            let mut mode = GameMode::ALL
                .iter()
                .position(|&m| m == settings.mode)
                .unwrap_or(0);
            if ui.stepper(slot_rect(1), "Mode", &mut mode, &mode_refs) {
                settings.mode = GameMode::ALL[mode];
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
            changed |= ui.toggle(champions_r, CHAMPIONS, &mut settings.champions);
        } else {
            let map = MAPS.get(settings.map).copied().unwrap_or("?");
            for (i, text) in [
                format!("Map: {map}"),
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
            let on = if settings.champions { "on" } else { "off" };
            ui.label_centered(champions_r, 1, theme::MUTED, &format!("{CHAMPIONS}: {on}"));
        }
        if changed {
            self.net.edit_settings(settings);
        }

        // Player rows.
        let row_h = sp(56);
        let rows_top = y + sp(36) + sp(20);
        for (i, slot) in lobby.slots.iter().enumerate() {
            let row = Rect::new(
                panel.x + 24,
                rows_top + i as i32 * (row_h + sp(8)),
                panel.w - 48,
                row_h,
            );
            ui.draw.rect(row, [30, 35, 46, 255]);
            let is_me = i == lobby.local_slot;
            let swatch_sz = row_h - 16;
            let swatch = Rect::new(row.x + 8, row.y + 8, swatch_sz, swatch_sz);
            let [r, g, b] = PLAYER_COLORS[slot.color as usize % 4].1;
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
                let team_names = ["Team 1", "Team 2", "Team 3", "Team 4"];
                let mut ti = slot.team as usize;
                if ui.stepper(team_r, "", &mut ti, &team_names) {
                    self.net.edit_slot(i, SlotEdit::Team(ti as u8));
                }
            } else {
                ui.label_centered(faction_r, 1, theme::TEXT, slot.faction.name());
                ui.label_centered(team_r, 1, theme::TEXT, &format!("Team {}", slot.team + 1));
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
        let footer_y = rows_top + 4 * (row_h + sp(8)) + sp(4);
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
    use crate::draw::{DrawList, SOLID};
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
    fn leaving_the_lobby_returns_to_play() {
        let mut h = Harness::new();
        h.click("Play");
        h.click_last("Host");
        h.click("Leave");
        assert_eq!(h.menu.screen(), Screen::Play);
        assert!(h.menu.net.lobby().is_none());
    }

    #[test]
    fn conquest_is_shown_but_cannot_be_hosted() {
        let mut h = Harness::new();
        h.click("Play");
        // Step the mode forward to Conquest (soon).
        h.click("Mode: Field Battle");
        assert!(h.has("Conquest (soon)"));
        h.click_last("Host");
        assert_eq!(
            h.menu.screen(),
            Screen::Play,
            "Host is disabled for Conquest"
        );
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

    #[test]
    fn every_screen_draws_inside_the_window() {
        for size in [(1280, 760), (853, 506), (1024, 600)] {
            let mut h = Harness::new();
            h.menu.set_roster(vec![RosterUnit {
                name: "Yari Ashigaru".into(),
                faction: h.menu.settings.faction,
                points: 400,
            }]);
            h.menu
                .net
                .host(&h.menu.profile(), LobbySettings::default(), "")
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
                        "{screen:?} {size:?} overflows: {q:?}"
                    );
                }
            }
        }
    }
}
