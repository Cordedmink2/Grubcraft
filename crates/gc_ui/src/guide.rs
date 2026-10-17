//! The in-game Guide (docs/08-ui-ux.md#guide): every building and troop of
//! every faction, with a short summary, the real numbers and what it
//! upgrades into or trains.
//!
//! The screen is pure UI. The game builds a [`GuideData`] (see
//! `gc_game/src/guide_data.rs`, which joins the faction rosters with the
//! prose in `data/guide/*.ron`), hands it to [`Guide::new`], and calls
//! [`Guide::frame`] each frame while the Guide is open. Layout:
//!
//! - Top row: the Buildings and Troops tabs, then the four faction buttons.
//! - Left: a scrollable list of that faction's entries.
//! - Right: the entry's card, with an Overview tab (summary, numbers, links to
//!   other entries) and an Upgrades tab (a button for each upgrade, evolution,
//!   unit trained or ability, and the selected one's benefit and numbers).
//!
//! Keys: Up and Down walk the list, Left and Right change faction, Escape
//! closes. The wheel scrolls the list or the card under the mouse
//! ([`Guide::wheel`]).

use crate::draw::{CELL_W, Rect, fit, wrap};
use crate::ui::{ButtonStyle, Press, Ui, theme};

/// Everything the Guide shows, one [`FactionGuide`] per faction in the
/// order of the faction buttons.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuideData {
    pub factions: Vec<FactionGuide>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FactionGuide {
    /// On the faction button: "Ember Clans".
    pub name: String,
    pub buildings: Vec<GuideEntry>,
    /// Regiments, summoned regiments and heroes.
    pub troops: Vec<GuideEntry>,
}

/// One building or troop.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuideEntry {
    pub name: String,
    /// One line under the name: "Production building, tier 2".
    pub kind: String,
    /// A word at the right of its row in the list: "T2", "hero".
    pub tag: String,
    /// The short paragraph on its role and when to use it.
    pub summary: String,
    /// (label, value) rows of real numbers.
    pub facts: Vec<(String, String)>,
    /// (label, text) rows of links to other entries: "Trains", "Weak against".
    pub links: Vec<(String, String)>,
    /// The Upgrades tab's buttons.
    pub upgrades: Vec<GuideUpgrade>,
    /// Shown on the Upgrades tab when `upgrades` is empty.
    pub no_upgrades: String,
}

/// One upgrade, evolution, trained unit or ability of an entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuideUpgrade {
    /// On its button: "Castle Keep", "Spitters", "Brace".
    pub name: String,
    /// What kind of thing it is: "Tier upgrade", "Evolution", "Passive".
    pub tag: String,
    /// What it costs or how it is used: "300 Grub, 100 Marrow, 60 s".
    pub cost: String,
    /// What it does for you.
    pub benefit: String,
    /// (label, value) rows of real numbers.
    pub facts: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideTab {
    Buildings,
    Troops,
}

impl GuideTab {
    pub const ALL: [GuideTab; 2] = [GuideTab::Buildings, GuideTab::Troops];

    pub fn name(self) -> &'static str {
        match self {
            GuideTab::Buildings => "Buildings",
            GuideTab::Troops => "Troops",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The card's two sub-tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardTab {
    Overview,
    Upgrades,
}

impl CardTab {
    const ALL: [CardTab; 2] = [CardTab::Overview, CardTab::Upgrades];

    fn name(self) -> &'static str {
        match self {
            CardTab::Overview => "Overview",
            CardTab::Upgrades => "Upgrades",
        }
    }
}

/// What the game should do after a frame of the Guide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideEvent {
    None,
    /// Close the Guide (Close button or Escape).
    Close,
}

const MAX_FACTIONS: usize = 4;
const LIST_W: i32 = 290;
const ROW_H: i32 = 30;
const LINE_H: i32 = 16;
const LABEL_CHARS: usize = 14;
const CHIP_H: i32 = 30;
const CHIP_GAP: i32 = 6;
const SCROLLBAR_W: i32 = 8;

/// Where everything stands in a window of `screen` UI pixels.
#[derive(Clone, Copy, Debug)]
struct Layout {
    panel: Rect,
    close: Rect,
    tabs: [Rect; 2],
    factions: [Rect; MAX_FACTIONS],
    list: Rect,
    card: Rect,
    sub: [Rect; 2],
    /// The card's text and buttons, under its title and sub-tabs.
    body: Rect,
}

fn layout(screen: (i32, i32)) -> Layout {
    let w = (screen.0 - 32).clamp(320, 1240);
    let h = (screen.1 - 40).clamp(300, 760);
    let panel = Rect::new((screen.0 - w) / 2, (screen.1 - h) / 2 + 10, w, h);
    let x0 = panel.x + 24;
    let right = panel.right() - 24;
    let close = Rect::new(right - 96, panel.y + 12, 96, 30);
    let row_y = panel.y + 50;
    let tab_w = 124;
    let tabs = [
        Rect::new(x0, row_y, tab_w, 34),
        Rect::new(x0 + tab_w + 6, row_y, tab_w, 34),
    ];
    let fx = x0 + 2 * tab_w + 6 + 30;
    let fw = ((right - fx - 3 * 6) / MAX_FACTIONS as i32).clamp(60, 170);
    let factions = std::array::from_fn(|i| Rect::new(fx + i as i32 * (fw + 6), row_y, fw, 34));
    let top = row_y + 34 + 12;
    let bottom = panel.bottom() - 32;
    let list = Rect::new(x0, top, LIST_W.min((right - x0) / 3), bottom - top);
    let card = Rect::new(
        list.right() + 12,
        top,
        right - list.right() - 12,
        bottom - top,
    );
    let sub = [
        Rect::new(card.x + 12, card.y + 58, 130, 30),
        Rect::new(card.x + 12 + 136, card.y + 58, 130, 30),
    ];
    let body = Rect::new(
        card.x + 12,
        card.y + 98,
        card.w - 24,
        (card.bottom() - card.y - 98 - 8).max(0),
    );
    Layout {
        panel,
        close,
        tabs,
        factions,
        list,
        card,
        sub,
        body,
    }
}

/// Whether a click or the wheel at `mouse` is on the Guide's panel. While
/// it is open the game should not pass such input to the battle behind it.
pub fn over_panel(screen: (i32, i32), mouse: (i32, i32)) -> bool {
    layout(screen).panel.contains(mouse.0, mouse.1)
}

/// The Guide screen's state: which tab, faction, entry and sub-tab is shown.
pub struct Guide {
    data: GuideData,
    tab: GuideTab,
    faction: usize,
    /// Selected entry and first visible list row, for each tab and faction.
    selected: [[usize; MAX_FACTIONS]; 2],
    top: [[usize; MAX_FACTIONS]; 2],
    card: CardTab,
    upgrade: usize,
    /// First visible line of the card's text.
    scroll: usize,
    /// The most `scroll` can be, as of the last frame.
    max_scroll: usize,
    /// The selection moved: scroll the list to show it on the next frame.
    follow: bool,
    press: Press,
}

impl Guide {
    /// A Guide opening on the first faction's buildings.
    pub fn new(data: GuideData) -> Guide {
        Guide {
            data,
            tab: GuideTab::Buildings,
            faction: 0,
            selected: [[0; MAX_FACTIONS]; 2],
            top: [[0; MAX_FACTIONS]; 2],
            card: CardTab::Overview,
            upgrade: 0,
            scroll: 0,
            max_scroll: 0,
            follow: false,
            press: Press::default(),
        }
    }

    pub fn data(&self) -> &GuideData {
        &self.data
    }

    pub fn tab(&self) -> GuideTab {
        self.tab
    }

    /// Index of the shown faction.
    pub fn faction(&self) -> usize {
        self.faction
    }

    pub fn card_tab(&self) -> CardTab {
        self.card
    }

    /// Index of the upgrade whose detail is shown on the Upgrades tab.
    pub fn upgrade(&self) -> usize {
        self.upgrade
    }

    /// First visible line of the card's text.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// First visible row of the list.
    pub fn list_top(&self) -> usize {
        self.top[self.tab.index()][self.faction]
    }

    pub fn selected_index(&self) -> usize {
        self.selected[self.tab.index()][self.faction]
    }

    fn entries(&self) -> &[GuideEntry] {
        match self.data.factions.get(self.faction) {
            Some(f) => match self.tab {
                GuideTab::Buildings => &f.buildings,
                GuideTab::Troops => &f.troops,
            },
            None => &[],
        }
    }

    /// The entry shown on the card, if the list has any.
    pub fn selected(&self) -> Option<&GuideEntry> {
        self.entries().get(self.selected_index())
    }

    pub fn select_tab(&mut self, tab: GuideTab) {
        if self.tab != tab {
            self.tab = tab;
            self.reset_card();
        }
    }

    pub fn select_faction(&mut self, faction: usize) {
        let faction = faction.min(self.data.factions.len().saturating_sub(1));
        if self.faction != faction {
            self.faction = faction;
            self.reset_card();
        }
    }

    pub fn select_card_tab(&mut self, card: CardTab) {
        if self.card != card {
            self.card = card;
            self.upgrade = 0;
            self.scroll = 0;
        }
    }

    /// Select upgrade `i` on the Upgrades tab.
    pub fn select_upgrade(&mut self, i: usize) {
        let n = self.selected().map_or(0, |e| e.upgrades.len());
        if n > 0 && i < n && self.upgrade != i {
            self.upgrade = i;
            self.scroll = 0;
        }
    }

    /// Select entry `i` of the list.
    pub fn select(&mut self, i: usize) {
        let n = self.entries().len();
        if n == 0 {
            return;
        }
        let i = i.min(n - 1);
        let (t, f) = (self.tab.index(), self.faction);
        if self.selected[t][f] != i {
            self.selected[t][f] = i;
            self.follow = true;
            self.reset_card();
        }
    }

    /// Open on the entry called `name` in `tab` of faction `faction` (a
    /// Guide button on the selected building or regiment in the battle would
    /// use this). False, and nothing changes, when there is no such entry.
    pub fn show(&mut self, tab: GuideTab, faction: usize, name: &str) -> bool {
        let Some(f) = self.data.factions.get(faction) else {
            return false;
        };
        let list = match tab {
            GuideTab::Buildings => &f.buildings,
            GuideTab::Troops => &f.troops,
        };
        let Some(at) = list.iter().position(|e| e.name == name) else {
            return false;
        };
        self.tab = tab;
        self.faction = faction;
        self.selected[tab.index()][faction] = at;
        self.follow = true;
        self.reset_card();
        true
    }

    fn reset_card(&mut self) {
        self.upgrade = 0;
        self.scroll = 0;
    }

    /// Move the selection by `delta` rows.
    fn step(&mut self, delta: i32) {
        let n = self.entries().len();
        if n > 0 {
            let to = (self.selected_index() as i32 + delta).clamp(0, n as i32 - 1);
            self.select(to as usize);
        }
    }

    fn step_faction(&mut self, delta: i32) {
        let n = self.data.factions.len().min(MAX_FACTIONS) as i32;
        if n > 0 {
            self.select_faction((self.faction as i32 + delta).rem_euclid(n) as usize);
        }
    }

    /// The wheel turned `lines` over the Guide at `mouse` (positive scrolls
    /// down). False when the mouse is not over the list or the card.
    pub fn wheel(&mut self, screen: (i32, i32), mouse: (i32, i32), lines: i32) -> bool {
        let l = layout(screen);
        if l.list.contains(mouse.0, mouse.1) {
            let (t, f) = (self.tab.index(), self.faction);
            let rows = rows_visible(l.list);
            let max = self.entries().len().saturating_sub(rows);
            self.top[t][f] = (self.top[t][f] as i32 + lines).clamp(0, max as i32) as usize;
            true
        } else if l.card.contains(mouse.0, mouse.1) {
            self.scroll = (self.scroll as i32 + lines).clamp(0, self.max_scroll as i32) as usize;
            true
        } else {
            false
        }
    }

    /// Draw the Guide and handle this frame's input (`ui.input`).
    pub fn frame(&mut self, ui: &mut Ui) -> GuideEvent {
        let l = layout(ui.screen);
        let mut event = GuideEvent::None;
        if ui.input.escape {
            event = GuideEvent::Close;
        }

        // Keys: Up and Down walk the list, Left and Right the factions.
        if ui.input.up {
            self.step(-1);
        } else if ui.input.down {
            self.step(1);
        } else if ui.input.left {
            self.step_faction(-1);
        } else if ui.input.right {
            self.step_faction(1);
        }

        ui.draw
            .rect(Rect::new(0, 0, ui.screen.0, ui.screen.1), theme::SCRIM);
        ui.panel(l.panel);
        ui.banner(l.panel.x + 24, l.panel.y - 10, 2, "Guide");
        if ui.button(l.close, "Close", ButtonStyle::Normal, true) {
            event = GuideEvent::Close;
        }

        // Tabs and the faction picker.
        for (tab, r) in GuideTab::ALL.into_iter().zip(l.tabs) {
            let style = style_for(self.tab == tab);
            if ui.button(r, tab.name(), style, true) {
                self.select_tab(tab);
            }
        }
        let names: Vec<String> = self.data.factions.iter().map(|f| f.name.clone()).collect();
        for (i, r) in l.factions.iter().enumerate().take(names.len()) {
            if ui.button(*r, &names[i], style_for(self.faction == i), true) {
                self.select_faction(i);
            }
        }
        ui.reserve_ids(MAX_FACTIONS.saturating_sub(names.len()));

        self.list(ui, l.list);
        self.card_panel(ui, &l);

        ui.label(
            l.panel.x + 24,
            l.panel.bottom() - 30,
            1,
            theme::INK_MUTED,
            &fit(
                "Up/Down pick an entry   Left/Right change faction   Wheel scrolls   Esc closes",
                ((l.panel.w - 48) / CELL_W) as usize,
            ),
        );
        self.press.end_frame(ui.input.released);
        event
    }

    /// The scrolling list of entries.
    fn list(&mut self, ui: &mut Ui, area: Rect) {
        ui.draw.boxed(area, theme::FIELD_CLOTH, 2);
        let rows = rows_visible(area);
        let (t, f) = (self.tab.index(), self.faction);
        let (mouse, pressed, released) = (ui.input.mouse, ui.input.pressed, ui.input.released);
        let len = self.entries().len();
        let max_top = len.saturating_sub(rows);
        if self.follow {
            let sel = self.selected[t][f];
            if sel < self.top[t][f] {
                self.top[t][f] = sel;
            } else if sel >= self.top[t][f] + rows {
                self.top[t][f] = sel + 1 - rows;
            }
            self.follow = false;
        }
        self.top[t][f] = self.top[t][f].min(max_top);
        let top = self.top[t][f];
        let selected = self.selected[t][f];
        let mut clicked = None;
        if len == 0 {
            ui.label(
                area.x + 12,
                area.y + 12,
                1,
                theme::INK_MUTED,
                "Nothing here.",
            );
        }
        let text_w = area.w - 2 * 6 - SCROLLBAR_W - 6;
        for (k, i) in (top..len.min(top + rows)).enumerate() {
            let row = Rect::new(area.x + 6, area.y + 6 + k as i32 * ROW_H, text_w, ROW_H - 2);
            let (click, held) = self
                .press
                .widget(i as u32 + 1, row, mouse, pressed, released);
            if click {
                clicked = Some(i);
            }
            let e = &self.data.factions[self.faction];
            let e = match self.tab {
                GuideTab::Buildings => &e.buildings[i],
                GuideTab::Troops => &e.troops[i],
            };
            let hot = row.contains(mouse.0, mouse.1);
            let is_sel = i == selected;
            let (fill, ink, dim) = if is_sel {
                (theme::MADDER, theme::LINEN, theme::LINEN_DK)
            } else if held {
                (theme::CLOTH_DOWN, theme::INK, theme::INK_MUTED)
            } else if hot {
                (theme::CLOTH_HOT, theme::INK, theme::INK_MUTED)
            } else {
                (theme::FIELD_CLOTH, theme::INK, theme::INK_MUTED)
            };
            ui.draw.rect(row, fill);
            if hot && !is_sel {
                ui.draw.border(row, 2, theme::WELD);
            }
            let tag_w = e.tag.chars().count() as i32 * CELL_W;
            let name_chars = ((row.w - 16 - tag_w - 8) / CELL_W).max(1) as usize;
            ui.label(
                row.x + 8,
                row.y + (row.h - 16) / 2,
                1,
                ink,
                &fit(&e.name, name_chars),
            );
            ui.label_right(row.right() - 8, row.y + (row.h - 16) / 2, 1, dim, &e.tag);
        }
        if let Some(i) = clicked {
            self.select(i);
        }
        scrollbar(
            &mut ui.draw,
            Rect::new(
                area.right() - SCROLLBAR_W - 4,
                area.y + 6,
                SCROLLBAR_W,
                area.h - 12,
            ),
            top,
            rows,
            len,
        );
    }

    /// The card: title, sub-tabs and the Overview or Upgrades body.
    fn card_panel(&mut self, ui: &mut Ui, l: &Layout) {
        ui.draw.boxed(l.card, theme::FIELD_CLOTH, 2);
        let Some(e) = self.selected().cloned() else {
            ui.label(
                l.card.x + 14,
                l.card.y + 14,
                1,
                theme::INK_MUTED,
                "Nothing to show.",
            );
            return;
        };
        let chars = ((l.card.w - 24) / CELL_W) as usize;
        ui.label(
            l.card.x + 12 + 1,
            l.card.y + 12,
            2,
            theme::INK,
            &fit(&e.name, chars / 2),
        );
        ui.label(
            l.card.x + 12,
            l.card.y + 12,
            2,
            theme::MADDER,
            &fit(&e.name, chars / 2),
        );
        ui.label(
            l.card.x + 12,
            l.card.y + 38,
            1,
            theme::INK_MUTED,
            &fit(&e.kind, chars),
        );
        for (tab, r) in CardTab::ALL.into_iter().zip(l.sub) {
            let label = match tab {
                CardTab::Upgrades if !e.upgrades.is_empty() => {
                    format!("Upgrades {}", e.upgrades.len())
                }
                _ => tab.name().to_string(),
            };
            if ui.button(r, &label, style_for(self.card == tab), true) {
                self.select_card_tab(tab);
            }
        }

        let mut body = l.body;
        let mut lines;
        match self.card {
            CardTab::Overview => {
                lines = overview_lines(&e, text_chars(body));
            }
            CardTab::Upgrades => {
                if e.upgrades.is_empty() {
                    lines = vec![Line::Text(theme::INK_MUTED, e.no_upgrades.clone())];
                    lines = wrap_lines(lines, text_chars(body));
                } else {
                    self.upgrade = self.upgrade.min(e.upgrades.len() - 1);
                    // A flow of buttons, one for each upgrade.
                    let mut x = body.x;
                    let mut y = body.y;
                    let mut clicked = None;
                    for (i, u) in e.upgrades.iter().enumerate() {
                        let w = (fit(&u.name, 26).chars().count() as i32 * CELL_W + 24).max(96);
                        if x > body.x && x + w > body.right() {
                            x = body.x;
                            y += CHIP_H + CHIP_GAP;
                        }
                        let r = Rect::new(x, y, w, CHIP_H);
                        if ui.button(r, &u.name, style_for(self.upgrade == i), true) {
                            clicked = Some(i);
                        }
                        x += w + CHIP_GAP;
                    }
                    let used = y + CHIP_H + 10 - body.y;
                    body = Rect::new(body.x, body.y + used, body.w, (body.h - used).max(0));
                    if let Some(i) = clicked {
                        self.select_upgrade(i);
                    }
                    lines = upgrade_lines(&e.upgrades[self.upgrade], text_chars(body));
                }
            }
        }
        let visible = (body.h / LINE_H).max(1) as usize;
        self.max_scroll = lines.len().saturating_sub(visible);
        self.scroll = self.scroll.min(self.max_scroll);
        draw_lines(ui, body, &lines, self.scroll);
        scrollbar(
            &mut ui.draw,
            Rect::new(body.right() - SCROLLBAR_W, body.y, SCROLLBAR_W, body.h),
            self.scroll,
            visible,
            lines.len(),
        );
    }
}

fn style_for(on: bool) -> ButtonStyle {
    if on {
        ButtonStyle::Primary
    } else {
        ButtonStyle::Normal
    }
}

fn rows_visible(list: Rect) -> usize {
    ((list.h - 12) / ROW_H).max(1) as usize
}

/// Characters across the card's text, leaving room for the scroll bar.
fn text_chars(body: Rect) -> usize {
    ((body.w - SCROLLBAR_W - 8) / CELL_W).max(10) as usize
}

/// A scroll bar for `len` rows of which `rows` show from `top`; nothing
/// when they all fit.
fn scrollbar(d: &mut crate::draw::DrawList, track: Rect, top: usize, rows: usize, len: usize) {
    if len <= rows || track.h <= 0 {
        return;
    }
    d.rect(track, theme::CLOTH_OFF);
    let thumb_h = (track.h * rows as i32 / len as i32).max(18).min(track.h);
    let span = (len - rows) as i32;
    let y = track.y + (track.h - thumb_h) * top.min(len - rows) as i32 / span.max(1);
    d.rect(Rect::new(track.x, y, track.w, thumb_h), theme::INK_MUTED);
}

/// One line of the card's text.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Line {
    /// A section title.
    Heading(String),
    /// A fact: its label on the first line, its text beside it.
    Fact(Option<String>, String),
    Text([u8; 4], String),
    Blank,
}

fn wrap_lines(lines: Vec<Line>, chars: usize) -> Vec<Line> {
    let mut out = Vec::new();
    for l in lines {
        match l {
            Line::Text(c, t) => out.extend(wrap(&t, chars).into_iter().map(|t| Line::Text(c, t))),
            other => out.push(other),
        }
    }
    out
}

fn facts(lines: &mut Vec<Line>, rows: &[(String, String)], chars: usize) {
    let value_chars = chars.saturating_sub(LABEL_CHARS + 1).max(8);
    for (label, value) in rows {
        for (k, text) in wrap(value, value_chars).into_iter().enumerate() {
            lines.push(Line::Fact((k == 0).then(|| fit(label, LABEL_CHARS)), text));
        }
    }
}

fn overview_lines(e: &GuideEntry, chars: usize) -> Vec<Line> {
    let mut lines = vec![Line::Heading("Summary".into())];
    for t in wrap(&e.summary, chars) {
        lines.push(Line::Text(theme::INK, t));
    }
    if !e.facts.is_empty() {
        lines.push(Line::Blank);
        lines.push(Line::Heading("Numbers".into()));
        facts(&mut lines, &e.facts, chars);
    }
    if !e.links.is_empty() {
        lines.push(Line::Blank);
        lines.push(Line::Heading("Links".into()));
        facts(&mut lines, &e.links, chars);
    }
    lines
}

fn upgrade_lines(u: &GuideUpgrade, chars: usize) -> Vec<Line> {
    let mut lines = vec![Line::Heading(fit(&u.name, chars))];
    let head = [u.tag.as_str(), u.cost.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("  |  ");
    for t in wrap(&head, chars) {
        lines.push(Line::Text(theme::INK_MUTED, t));
    }
    lines.push(Line::Blank);
    for t in wrap(&u.benefit, chars) {
        lines.push(Line::Text(theme::INK, t));
    }
    if !u.facts.is_empty() {
        lines.push(Line::Blank);
        lines.push(Line::Heading("Numbers".into()));
        facts(&mut lines, &u.facts, chars);
    }
    lines
}

/// Draw the lines from `scroll` that fit in `area`.
fn draw_lines(ui: &mut Ui, area: Rect, lines: &[Line], scroll: usize) {
    for (k, line) in lines.iter().skip(scroll).enumerate() {
        let y = area.y + k as i32 * LINE_H;
        if y + LINE_H > area.bottom() {
            break;
        }
        match line {
            Line::Heading(t) => {
                ui.draw.text_bold(area.x, y, 1, theme::MADDER, t);
            }
            Line::Fact(label, text) => {
                if let Some(label) = label {
                    ui.label(area.x, y, 1, theme::INK_MUTED, label);
                }
                ui.label(
                    area.x + (LABEL_CHARS as i32 + 1) * CELL_W,
                    y,
                    1,
                    theme::INK,
                    text,
                );
            }
            Line::Text(c, t) => ui.label(area.x, y, 1, *c, t),
            Line::Blank => {}
        }
    }
}

/// The dev overlay's font, to draw pictures of the screen in tests.
#[cfg(test)]
#[path = "../../gc_render/src/debug_font.rs"]
#[allow(dead_code)]
mod debug_font;

#[cfg(test)]
mod tests {
    use super::debug_font;
    use super::*;
    use crate::draw::SOLID;
    use crate::ui::UiInput;

    fn upgrade(name: &str) -> GuideUpgrade {
        GuideUpgrade {
            name: name.into(),
            tag: "Tier upgrade".into(),
            cost: "300 Grub, 100 Marrow, 60 s".into(),
            benefit: format!("{name} gives you a good deal more than it costs. ").repeat(6),
            facts: vec![("Hit points".into(), "3,800".into())],
        }
    }

    fn entry(faction: usize, i: usize, n_upgrades: usize) -> GuideEntry {
        GuideEntry {
            name: format!("Thing {faction}-{i}"),
            kind: "Production building, tier 2".into(),
            tag: "T2".into(),
            summary: "It does a useful thing at a useful time, and costs a fair amount for it. "
                .repeat(8),
            facts: (0..24)
                .map(|k| (format!("Fact {k}"), format!("value {k}")))
                .collect(),
            links: vec![("Trains".into(), "Alpha, Beta, Gamma".into())],
            upgrades: (0..n_upgrades)
                .map(|k| upgrade(&format!("Up {k}")))
                .collect(),
            no_upgrades: "It has no upgrades.".into(),
        }
    }

    fn data() -> GuideData {
        GuideData {
            factions: (0..4)
                .map(|f| FactionGuide {
                    name: format!("Faction {f}"),
                    // Faction 0 has a long list so it scrolls; entry 1 has no upgrades.
                    buildings: (0..if f == 0 { 40 } else { 5 })
                        .map(|i| entry(f, i, if i == 1 { 0 } else { 3 }))
                        .collect(),
                    troops: (0..6).map(|i| entry(f, 100 + i, 2)).collect(),
                })
                .collect(),
        }
    }

    const SCREEN: (i32, i32) = (1280, 720);

    fn frame(g: &mut Guide, ui: &mut Ui, input: UiInput) -> GuideEvent {
        ui.begin_frame(input, SCREEN);
        let e = g.frame(ui);
        ui.end_frame();
        e
    }

    fn idle(g: &mut Guide, ui: &mut Ui) -> GuideEvent {
        frame(g, ui, UiInput::default())
    }

    fn click(g: &mut Guide, ui: &mut Ui, at: (i32, i32)) -> GuideEvent {
        let down = UiInput {
            mouse: at,
            pressed: true,
            ..UiInput::default()
        };
        frame(g, ui, down);
        let up = UiInput {
            mouse: at,
            released: true,
            ..UiInput::default()
        };
        frame(g, ui, up)
    }

    fn centre(r: Rect) -> (i32, i32) {
        (r.x + r.w / 2, r.y + r.h / 2)
    }

    fn key(g: &mut Guide, ui: &mut Ui, f: impl Fn(&mut UiInput)) -> GuideEvent {
        let mut i = UiInput::default();
        f(&mut i);
        frame(g, ui, i)
    }

    #[test]
    fn it_opens_on_the_first_factions_first_building() {
        let mut g = Guide::new(data());
        let mut ui = Ui::new();
        idle(&mut g, &mut ui);
        assert_eq!(g.tab(), GuideTab::Buildings);
        assert_eq!(g.faction(), 0);
        assert_eq!(g.selected().unwrap().name, "Thing 0-0");
        assert_eq!(g.card_tab(), CardTab::Overview);
    }

    #[test]
    fn the_tabs_and_faction_buttons_switch_what_the_list_shows() {
        let mut g = Guide::new(data());
        let mut ui = Ui::new();
        idle(&mut g, &mut ui);
        let l = layout(SCREEN);
        click(&mut g, &mut ui, centre(l.tabs[1]));
        assert_eq!(g.tab(), GuideTab::Troops);
        assert_eq!(g.selected().unwrap().name, "Thing 0-100");
        click(&mut g, &mut ui, centre(l.factions[2]));
        assert_eq!(g.faction(), 2);
        assert_eq!(g.selected().unwrap().name, "Thing 2-100");
        click(&mut g, &mut ui, centre(l.tabs[0]));
        assert_eq!(g.selected().unwrap().name, "Thing 2-0");
        click(&mut g, &mut ui, centre(l.factions[3]));
        click(&mut g, &mut ui, centre(l.factions[0]));
        assert_eq!((g.faction(), g.tab()), (0, GuideTab::Buildings));
    }

    #[test]
    fn clicking_a_row_selects_it_and_each_list_remembers_its_row() {
        let mut g = Guide::new(data());
        let mut ui = Ui::new();
        idle(&mut g, &mut ui);
        let l = layout(SCREEN);
        click(
            &mut g,
            &mut ui,
            (l.list.x + 60, l.list.y + 6 + 2 * ROW_H + 10),
        );
        assert_eq!(g.selected().unwrap().name, "Thing 0-2");
        click(&mut g, &mut ui, centre(l.factions[1]));
        assert_eq!(g.selected().unwrap().name, "Thing 1-0");
        click(&mut g, &mut ui, centre(l.factions[0]));
        assert_eq!(g.selected().unwrap().name, "Thing 0-2", "kept its row");
    }

    #[test]
    fn a_press_released_elsewhere_does_not_select() {
        let mut g = Guide::new(data());
        let mut ui = Ui::new();
        idle(&mut g, &mut ui);
        let l = layout(SCREEN);
        let on_row = (l.list.x + 60, l.list.y + 6 + 3 * ROW_H + 10);
        frame(
            &mut g,
            &mut ui,
            UiInput {
                mouse: on_row,
                pressed: true,
                ..UiInput::default()
            },
        );
        frame(
            &mut g,
            &mut ui,
            UiInput {
                mouse: (l.card.x + 40, l.card.y + 300),
                released: true,
                ..UiInput::default()
            },
        );
        assert_eq!(g.selected_index(), 0);
    }

    #[test]
    fn upgrade_buttons_pick_the_upgrade_shown() {
        let mut g = Guide::new(data());
        let mut ui = Ui::new();
        idle(&mut g, &mut ui);
        let l = layout(SCREEN);
        click(&mut g, &mut ui, centre(l.sub[1]));
        assert_eq!(g.card_tab(), CardTab::Upgrades);
        assert_eq!(g.upgrade(), 0);
        // The chips start at the top of the body: 3 of them, left to right.
        let chip = |i: i32| (l.body.x + 10 + i * 100 + 40, l.body.y + 10);
        click(&mut g, &mut ui, chip(1));
        assert_eq!(g.upgrade(), 1);
        click(&mut g, &mut ui, chip(2));
        assert_eq!(g.upgrade(), 2);
        // Another entry starts at its first upgrade again.
        g.select(2);
        idle(&mut g, &mut ui);
        assert_eq!(g.upgrade(), 0);
    }

    #[test]
    fn an_entry_without_upgrades_says_so() {
        let mut g = Guide::new(data());
        let mut ui = Ui::new();
        g.select(1);
        g.select_card_tab(CardTab::Upgrades);
        idle(&mut g, &mut ui);
        let drawn = text_of(&ui);
        assert!(drawn.contains("It has no upgrades."), "{drawn}");
    }

    #[test]
    fn the_arrow_keys_walk_the_list_and_the_factions() {
        let mut g = Guide::new(data());
        let mut ui = Ui::new();
        idle(&mut g, &mut ui);
        key(&mut g, &mut ui, |i| i.down = true);
        key(&mut g, &mut ui, |i| i.down = true);
        assert_eq!(g.selected().unwrap().name, "Thing 0-2");
        for _ in 0..3 {
            key(&mut g, &mut ui, |i| i.up = true);
        }
        assert_eq!(g.selected_index(), 0, "stops at the top");
        key(&mut g, &mut ui, |i| i.left = true);
        assert_eq!(g.faction(), 3, "wraps round");
        key(&mut g, &mut ui, |i| i.right = true);
        assert_eq!(g.faction(), 0);
    }

    #[test]
    fn walking_down_a_long_list_keeps_the_selection_in_view() {
        let mut g = Guide::new(data());
        let mut ui = Ui::new();
        idle(&mut g, &mut ui);
        let rows = rows_visible(layout(SCREEN).list);
        assert!(rows < 40);
        for _ in 0..30 {
            key(&mut g, &mut ui, |i| i.down = true);
        }
        assert_eq!(g.selected_index(), 30);
        assert!(g.list_top() > 0);
        assert!(g.selected_index() >= g.list_top());
        assert!(g.selected_index() < g.list_top() + rows);
        assert!(text_of(&ui).contains("Thing 0-30"));
    }

    #[test]
    fn the_wheel_scrolls_the_list_and_the_card_under_the_mouse() {
        let mut g = Guide::new(data());
        let mut ui = Ui::new();
        idle(&mut g, &mut ui);
        let l = layout(SCREEN);
        assert!(g.wheel(SCREEN, (l.list.x + 50, l.list.y + 100), 5));
        assert_eq!(g.list_top(), 5);
        assert!(g.wheel(SCREEN, (l.list.x + 50, l.list.y + 100), -50));
        assert_eq!(g.list_top(), 0);
        assert!(g.wheel(SCREEN, (l.list.x + 50, l.list.y + 100), 500));
        assert_eq!(g.list_top(), 40 - rows_visible(l.list), "stops at the end");
        // The card has more lines than fit, so it scrolls too.
        assert!(g.wheel(SCREEN, (l.card.x + 50, l.card.y + 200), 4));
        assert_eq!(g.scroll(), 4);
        idle(&mut g, &mut ui);
        assert_eq!(g.scroll(), 4);
        assert!(g.wheel(SCREEN, (l.card.x + 50, l.card.y + 200), 999));
        assert!(g.scroll() > 4 && g.scroll() < 999);
        assert!(g.wheel(SCREEN, (l.card.x + 50, l.card.y + 200), -999));
        assert_eq!(g.scroll(), 0);
        assert!(!g.wheel(SCREEN, (3, 3), 1), "outside the panel");
        // A new entry starts at the top of its card.
        g.wheel(SCREEN, (l.card.x + 50, l.card.y + 200), 3);
        g.select(4);
        assert_eq!(g.scroll(), 0);
    }

    #[test]
    fn text_never_leaves_the_card_or_the_list() {
        let mut g = Guide::new(data());
        let mut ui = Ui::new();
        idle(&mut g, &mut ui);
        let l = layout(SCREEN);
        // (The title banner hangs over the panel's top edge on purpose.)
        for q in ui.draw.quads.iter().filter(|q| q.glyph != SOLID) {
            let (x, y) = (i32::from(q.pos[0]), i32::from(q.pos[1]));
            if y < l.panel.y {
                continue;
            }
            let inside = |r: Rect| r.contains(x, y) && r.contains(x + 7, y + 15);
            let ok = inside(l.card)
                || inside(l.list)
                || inside(l.panel) && !l.card.contains(x, y) && !l.list.contains(x, y);
            assert!(ok, "glyph at {x},{y} straddles an edge");
        }
        // And nothing in the card runs below its body.
        for q in ui.draw.quads.iter().filter(|q| q.glyph != SOLID) {
            let (x, y) = (i32::from(q.pos[0]), i32::from(q.pos[1]));
            if l.card.contains(x, y) {
                assert!(y + 16 <= l.card.bottom(), "text below the card at {y}");
            }
        }
    }

    #[test]
    fn escape_and_the_close_button_close_it() {
        let mut g = Guide::new(data());
        let mut ui = Ui::new();
        assert_eq!(idle(&mut g, &mut ui), GuideEvent::None);
        assert_eq!(key(&mut g, &mut ui, |i| i.escape = true), GuideEvent::Close);
        let l = layout(SCREEN);
        assert_eq!(click(&mut g, &mut ui, centre(l.close)), GuideEvent::Close);
    }

    #[test]
    fn show_jumps_to_an_entry_and_scrolls_to_it() {
        let mut g = Guide::new(data());
        let mut ui = Ui::new();
        assert!(g.show(GuideTab::Troops, 2, "Thing 2-104"));
        idle(&mut g, &mut ui);
        assert_eq!(
            (g.tab(), g.faction(), g.selected_index()),
            (GuideTab::Troops, 2, 4)
        );
        assert!(!g.show(GuideTab::Troops, 2, "Nobody"));
        assert!(!g.show(GuideTab::Troops, 9, "Thing 2-104"));
        assert!(g.show(GuideTab::Buildings, 0, "Thing 0-35"));
        idle(&mut g, &mut ui);
        let rows = rows_visible(layout(SCREEN).list);
        assert!(g.list_top() + rows > 35 && g.list_top() <= 35);
        assert!(text_of(&ui).contains("Thing 0-35"));
    }

    #[test]
    fn it_copes_with_no_data_and_a_small_window() {
        let mut g = Guide::new(GuideData::default());
        let mut ui = Ui::new();
        idle(&mut g, &mut ui);
        assert!(g.selected().is_none());
        g.select(3);
        g.select_faction(2);
        assert!(!g.show(GuideTab::Troops, 0, "x"));
        let mut g = Guide::new(data());
        for screen in [(640, 400), (320, 240), (2560, 1440)] {
            ui.begin_frame(UiInput::default(), screen);
            g.frame(&mut ui);
            ui.end_frame();
            assert!(!ui.draw.quads.is_empty());
        }
    }

    #[test]
    fn over_panel_covers_the_panel_only() {
        let l = layout(SCREEN);
        assert!(over_panel(SCREEN, centre(l.panel)));
        assert!(!over_panel(SCREEN, (2, 2)));
    }

    // ---- helpers to see the screen -------------------------------------

    /// All the text drawn, one string per row of glyphs left to right.
    fn text_of(ui: &Ui) -> String {
        let mut glyphs: Vec<_> = ui.draw.quads.iter().filter(|q| q.glyph != SOLID).collect();
        glyphs.sort_by_key(|q| (q.pos[1], q.pos[0]));
        let mut out = String::new();
        let mut last = (u16::MAX, 0u16);
        for q in glyphs {
            let ch = char::from(b' ' + q.glyph as u8);
            if q.pos[1] != last.0 {
                out.push('\n');
            } else if q.pos[0] > last.1 + 8 {
                out.push(' ');
            }
            out.push(ch);
            last = (q.pos[1], q.pos[0]);
        }
        out
    }

    /// Rasterise the draw list into a binary PPM, solid quads and the dev
    /// overlay's 8x16 glyphs at scale 1.
    pub(crate) fn render_ppm(ui: &Ui) -> Vec<u8> {
        let (w, h) = (ui.screen.0 as usize, ui.screen.1 as usize);
        let mut px = vec![[24u8, 70, 100]; w * h];
        let blend = |dst: &mut [u8; 3], c: [u8; 4]| {
            let a = u32::from(c[3]);
            for k in 0..3 {
                dst[k] = ((u32::from(c[k]) * a + u32::from(dst[k]) * (255 - a)) / 255) as u8;
            }
        };
        for q in &ui.draw.quads {
            let (x0, y0) = (q.pos[0] as usize, q.pos[1] as usize);
            let (qw, qh) = (q.size[0] as usize, q.size[1] as usize);
            if q.glyph == SOLID {
                for y in y0..(y0 + qh).min(h) {
                    for x in x0..(x0 + qw).min(w) {
                        blend(&mut px[y * w + x], q.color);
                    }
                }
            } else {
                let rows = &debug_font::GLYPHS[(q.glyph as usize).min(94)];
                for (gy, bits) in rows.iter().enumerate() {
                    for gx in 0..8 {
                        if bits & (0x80 >> gx) != 0 && x0 + gx < w && y0 + gy < h {
                            blend(&mut px[(y0 + gy) * w + x0 + gx], q.color);
                        }
                    }
                }
            }
        }
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        out.extend(px.iter().flatten());
        out
    }

    /// With `GUIDE_PPM_DIR` set, write pictures of the screen there.
    #[test]
    fn it_can_draw_pictures_of_itself() {
        let Ok(dir) = std::env::var("GUIDE_PPM_DIR") else {
            return;
        };
        let mut g = Guide::new(data());
        let mut ui = Ui::new();
        idle(&mut g, &mut ui);
        std::fs::write(format!("{dir}/guide-overview.ppm"), render_ppm(&ui)).unwrap();
        g.select_card_tab(CardTab::Upgrades);
        idle(&mut g, &mut ui);
        std::fs::write(format!("{dir}/guide-upgrades.ppm"), render_ppm(&ui)).unwrap();
    }
}
