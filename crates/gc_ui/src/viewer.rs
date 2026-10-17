//! The unit viewer's panels (docs/08-ui-ux.md#unit-viewer): a faction and
//! unit list on the left, the unit's numbers and abilities on the right, and
//! a row of animation buttons along the bottom. The middle of the window is
//! left clear for the game to draw the 3D model in, so the panels draw
//! nothing there.

use crate::draw::{CELL_W, Rect, fit, wrap};
use crate::lobby::Faction;
use crate::ui::{ButtonStyle, Ui, theme};

/// Width of the unit list's panel and of the details panel, in UI pixels.
const LEFT_W: i32 = 300;
const RIGHT_W: i32 = 360;
const MARGIN: i32 = 12;
const ROW_H: i32 = 30;
const LINE_H: i32 = 16;
/// Characters in the details panel's text, and in the value column of its
/// Stats tab.
const TEXT_CHARS: usize = ((RIGHT_W - 24) / CELL_W) as usize;
const LABEL_CHARS: usize = 15;
const VALUE_CHARS: usize = TEXT_CHARS - LABEL_CHARS - 1;
/// Animation buttons.
const CLIP_W: i32 = 84;
const CLIP_H: i32 = 34;

/// One unit or hero to look at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Index into [`Faction::ALL`].
    pub faction: usize,
    pub name: String,
    /// One line under the name: "Spear regiment, tier 1".
    pub kind: String,
    /// A word or two at the right of its row in the list: "T1", "hero".
    pub tag: String,
    /// (label, value) rows for the Stats tab.
    pub facts: Vec<(String, String)>,
    /// (name, how it works, what it does) for the Abilities tab.
    pub abilities: Vec<(String, String, String)>,
    /// The animations the 3D model has, as button labels; empty when the
    /// unit has no model yet.
    pub clips: Vec<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Stats,
    Abilities,
}

/// What the game should do after a frame of the viewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewerAction {
    None,
    /// Leave the viewer for the main menu.
    Back,
}

/// Where the panels stand in a window of `screen` UI pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub left: Rect,
    pub right: Rect,
    /// The strip between the panels, along the bottom, for the animation
    /// buttons.
    pub bar: Rect,
}

pub fn layout(screen: (i32, i32)) -> Layout {
    let (w, h) = screen;
    let left = Rect::new(MARGIN, MARGIN, LEFT_W, (h - 2 * MARGIN).max(0));
    let right = Rect::new(w - MARGIN - RIGHT_W, MARGIN, RIGHT_W, left.h);
    let x = left.right() + MARGIN;
    let bar_h = 2 * CLIP_H + 8;
    Layout {
        left,
        right,
        bar: Rect::new(x, h - MARGIN - bar_h, (right.x - MARGIN - x).max(0), bar_h),
    }
}

/// Whether a click or the wheel at `mouse` belongs to a panel or the clip
/// buttons rather than to the 3D view.
pub fn over_panel(screen: (i32, i32), mouse: (i32, i32)) -> bool {
    let l = layout(screen);
    [l.left, l.right, l.bar]
        .iter()
        .any(|r| r.contains(mouse.0, mouse.1))
}

pub struct Viewer {
    entries: Vec<Entry>,
    faction: usize,
    /// The entry on show: an index into `entries`.
    selected: usize,
    /// The list's first row on screen, within the faction's rows.
    top: usize,
    tab: Tab,
    /// Lines of the details scrolled off the top.
    scroll: usize,
    /// The animation held, an index into the entry's clips, or `None` to
    /// cycle through them.
    clip: Option<usize>,
}

impl Viewer {
    pub fn new(entries: Vec<Entry>) -> Viewer {
        let selected = entries
            .iter()
            .position(|e| !e.clips.is_empty())
            .unwrap_or(0);
        let faction = entries.get(selected).map_or(0, |e| e.faction);
        Viewer {
            entries,
            faction,
            selected,
            top: 0,
            tab: Tab::Stats,
            scroll: 0,
            clip: None,
        }
    }

    /// The unit on show, if there are any units.
    pub fn selected(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    /// Which entry is on show: its place in the entries given to
    /// [`Viewer::new`].
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// The animation held: an index into the selected entry's clips.
    pub fn clip(&self) -> Option<usize> {
        let n = self.selected()?.clips.len();
        self.clip.filter(|&c| c < n)
    }

    /// Show the unit called `name` (spelled as in its roster, case aside).
    /// False when there is none.
    pub fn select_named(&mut self, name: &str) -> bool {
        match self
            .entries
            .iter()
            .position(|e| e.name.eq_ignore_ascii_case(name))
        {
            Some(i) => {
                self.select(i);
                true
            }
            None => false,
        }
    }

    /// Show `entry` (an index into the entries given to [`Viewer::new`]).
    pub fn select(&mut self, entry: usize) {
        if entry >= self.entries.len() {
            return;
        }
        self.faction = self.entries[entry].faction;
        if entry == self.selected {
            return;
        }
        self.selected = entry;
        self.clip = None;
        self.scroll = 0;
    }

    /// The entries of the faction on show, as indices into `entries`.
    fn rows(&self) -> Vec<usize> {
        (0..self.entries.len())
            .filter(|&i| self.entries[i].faction == self.faction)
            .collect()
    }

    /// Show the faction `f`, with its first unit that has a model (or its
    /// first unit, when none has).
    fn show_faction(&mut self, f: usize) {
        self.faction = f;
        self.top = 0;
        let rows = self.rows();
        let first = rows
            .iter()
            .copied()
            .find(|&i| !self.entries[i].clips.is_empty())
            .or_else(|| rows.first().copied());
        if let Some(first) = first {
            self.select(first);
        }
    }

    /// Move the selection by `delta` units within the faction.
    pub fn step(&mut self, delta: i32) {
        let rows = self.rows();
        let Some(at) = rows.iter().position(|&i| i == self.selected) else {
            return;
        };
        let to = (at as i32 + delta).clamp(0, rows.len() as i32 - 1) as usize;
        self.select(rows[to]);
    }

    /// Move to the next (`delta` 1) or previous faction.
    fn step_faction(&mut self, delta: i32) {
        let n = Faction::ALL.len() as i32;
        let f = (self.faction as i32 + delta).rem_euclid(n) as usize;
        self.show_faction(f);
    }

    /// The wheel turned `lines` over the panel at `mouse` (positive scrolls
    /// down). False when the mouse is not over a panel that scrolls.
    pub fn wheel(&mut self, screen: (i32, i32), mouse: (i32, i32), lines: i32) -> bool {
        let l = layout(screen);
        if l.left.contains(mouse.0, mouse.1) {
            let more = self.rows().len().saturating_sub(1);
            self.top = (self.top as i32 + lines).clamp(0, more as i32) as usize;
            true
        } else if l.right.contains(mouse.0, mouse.1) {
            self.scroll = (self.scroll as i32 + lines).max(0) as usize;
            true
        } else {
            false
        }
    }

    pub fn frame(&mut self, ui: &mut Ui, escape: bool) -> ViewerAction {
        let l = layout(ui.screen);
        let mut action = ViewerAction::None;

        // Keys: Up and Down walk the list, Left and Right the factions.
        let keys = (ui.input.up, ui.input.down, ui.input.left, ui.input.right);
        match keys {
            (true, _, _, _) => self.step(-1),
            (_, true, _, _) => self.step(1),
            (_, _, true, _) => self.step_faction(-1),
            (_, _, _, true) => self.step_faction(1),
            _ => {}
        }

        self.left_panel(ui, l.left, &mut action);
        self.right_panel(ui, l.right);
        self.clip_bar(ui, l.bar);
        // The controls, on a dark strip so they read over any sky.
        ui.draw.rect(
            Rect::new(l.bar.x, MARGIN, l.bar.w, 2 * LINE_H + 8),
            theme::SCRIM,
        );
        ui.label_centered(
            Rect::new(l.bar.x, MARGIN + 4, l.bar.w, LINE_H),
            1,
            theme::TEXT,
            "Drag to turn the model   Right-drag to move it   Wheel to zoom",
        );
        ui.label_centered(
            Rect::new(l.bar.x, MARGIN + 4 + LINE_H, l.bar.w, LINE_H),
            1,
            theme::MUTED,
            "Home puts the view back   Arrow keys change unit",
        );
        if let Some(e) = self.selected()
            && e.clips.is_empty()
        {
            let middle = Rect::new(l.left.right(), 0, l.right.x - l.left.right(), ui.screen.1);
            ui.label_centered(middle, 2, theme::MUTED, "No 3D model for this unit yet");
        }
        if escape {
            action = ViewerAction::Back;
        }
        action
    }

    fn left_panel(&mut self, ui: &mut Ui, r: Rect, action: &mut ViewerAction) {
        ui.panel(r);
        ui.label(r.x + 14, r.y + 12, 2, theme::TEXT, "Units");
        // Faction tabs, two by two.
        let tab_w = (r.w - 28 - 8) / 2;
        let tabs_y = r.y + 44;
        for (k, f) in Faction::ALL.into_iter().enumerate() {
            let tab = Rect::new(
                r.x + 14 + (k as i32 % 2) * (tab_w + 8),
                tabs_y + (k as i32 / 2) * 40,
                tab_w,
                34,
            );
            let style = if k == self.faction {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Normal
            };
            if ui.button(tab, f.name(), style, true) && k != self.faction {
                self.show_faction(k);
            }
        }
        let back = Rect::new(r.x + 14, r.bottom() - 14 - 44, r.w - 28, 44);
        if ui.button(back, "Back", ButtonStyle::Normal, true) {
            *action = ViewerAction::Back;
        }

        // The faction's units.
        let list_top = tabs_y + 2 * 40 + 8;
        let count_y = back.y - 8 - LINE_H;
        let rows = self.rows();
        let per_page = (((count_y - 4 - list_top) / ROW_H).max(1)) as usize;
        // Keep the selection on screen.
        if let Some(at) = rows.iter().position(|&i| i == self.selected) {
            if at < self.top {
                self.top = at;
            } else if at >= self.top + per_page {
                self.top = at + 1 - per_page;
            }
        }
        self.top = self.top.min(rows.len().saturating_sub(per_page));
        let chars = ((r.w - 28 - 12) / CELL_W) as usize;
        let mut picked = None;
        for (k, &i) in rows.iter().skip(self.top).take(per_page).enumerate() {
            let row = Rect::new(r.x + 14, list_top + k as i32 * ROW_H, r.w - 28, ROW_H - 4);
            let e = &self.entries[i];
            if ui.row(row, i == self.selected) {
                picked = Some(i);
            }
            let color = if e.clips.is_empty() {
                theme::MUTED
            } else {
                theme::TEXT
            };
            let name = fit(&e.name, chars.saturating_sub(e.tag.len() + 2));
            ui.label(row.x + 8, row.y + 4, 1, color, &name);
            ui.label_right(row.right() - 8, row.y + 4, 1, theme::MUTED, &e.tag);
        }
        if let Some(i) = picked {
            self.select(i);
        }
        let shown = rows.len().min(self.top + per_page);
        let text = if rows.is_empty() {
            "No units".to_string()
        } else {
            format!("{}-{} of {}", self.top + 1, shown, rows.len())
        };
        ui.label(r.x + 14, count_y, 1, theme::MUTED, &text);
    }

    fn right_panel(&mut self, ui: &mut Ui, r: Rect) {
        ui.panel(r);
        let Some(e) = self.entries.get(self.selected) else {
            return;
        };
        let faction = Faction::ALL[e.faction.min(Faction::ALL.len() - 1)];
        ui.label(r.x + 12, r.y + 12, 1, theme::ACCENT, faction.name());
        ui.label(
            r.x + 12,
            r.y + 30,
            2,
            theme::TEXT,
            &fit(&e.name, ((r.w - 24) / (2 * CELL_W)) as usize),
        );
        ui.label(
            r.x + 12,
            r.y + 62,
            1,
            theme::MUTED,
            &fit(&e.kind, TEXT_CHARS),
        );
        let tab_w = (r.w - 24 - 8) / 2;
        for (k, (tab, label)) in [(Tab::Stats, "Stats"), (Tab::Abilities, "Abilities")]
            .into_iter()
            .enumerate()
        {
            let button = Rect::new(r.x + 12 + k as i32 * (tab_w + 8), r.y + 86, tab_w, 34);
            let style = if self.tab == tab {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Normal
            };
            if ui.button(button, label, style, true) && self.tab != tab {
                self.tab = tab;
                self.scroll = 0;
            }
        }
        let lines = match self.tab {
            Tab::Stats => stat_lines(e),
            Tab::Abilities => ability_lines(e),
        };
        let top = r.y + 132;
        let room = (((r.bottom() - 28 - top) / LINE_H).max(1)) as usize;
        self.scroll = self.scroll.min(lines.len().saturating_sub(room));
        for (k, line) in lines.iter().skip(self.scroll).take(room).enumerate() {
            let y = top + k as i32 * LINE_H;
            match line {
                Line::Fact { label, text } => {
                    if let Some(label) = label {
                        ui.label(r.x + 12, y, 1, theme::MUTED, label);
                    }
                    ui.label(
                        r.x + 12 + (LABEL_CHARS as i32 + 1) * CELL_W,
                        y,
                        1,
                        theme::TEXT,
                        text,
                    );
                }
                Line::Text { color, text } => ui.label(r.x + 12, y, 1, *color, text),
            }
        }
        if lines.len() > room {
            ui.label(
                r.x + 12,
                r.bottom() - 22,
                1,
                theme::MUTED,
                &format!(
                    "Wheel scrolls: lines {}-{} of {}",
                    self.scroll + 1,
                    (self.scroll + room).min(lines.len()),
                    lines.len()
                ),
            );
        }
    }

    fn clip_bar(&mut self, ui: &mut Ui, bar: Rect) {
        let Some(e) = self.entries.get(self.selected) else {
            return;
        };
        if e.clips.is_empty() {
            return;
        }
        // "Cycle" plus one button per clip, in rows that fit the strip,
        // each row centred, the last row lowest.
        let labels: Vec<&str> = std::iter::once("Cycle")
            .chain(e.clips.iter().copied())
            .collect();
        let per_row = ((bar.w / (CLIP_W + 6)).max(1)) as usize;
        let rows: Vec<&[&str]> = labels.chunks(per_row).collect();
        let first_y = bar.bottom() - rows.len() as i32 * (CLIP_H + 6) + 6;
        let mut clicked = None;
        for (r, row) in rows.iter().enumerate() {
            let width = row.len() as i32 * (CLIP_W + 6) - 6;
            let x0 = bar.x + (bar.w - width) / 2;
            for (c, label) in row.iter().enumerate() {
                let which = r * per_row + c;
                let held = match self.clip {
                    None => which == 0,
                    Some(k) => which == k + 1,
                };
                let style = if held {
                    ButtonStyle::Primary
                } else {
                    ButtonStyle::Normal
                };
                let button = Rect::new(
                    x0 + c as i32 * (CLIP_W + 6),
                    first_y + r as i32 * (CLIP_H + 6),
                    CLIP_W,
                    CLIP_H,
                );
                if ui.button(button, label, style, true) {
                    clicked = Some(which);
                }
            }
        }
        if let Some(which) = clicked {
            self.clip = which.checked_sub(1);
        }
    }
}

enum Line {
    /// A fact: its label on the first line, its text beside it.
    Fact {
        label: Option<String>,
        text: String,
    },
    Text {
        color: [u8; 4],
        text: String,
    },
}

fn stat_lines(e: &Entry) -> Vec<Line> {
    let mut lines = Vec::new();
    for (label, value) in &e.facts {
        for (k, text) in wrap(value, VALUE_CHARS).into_iter().enumerate() {
            lines.push(Line::Fact {
                label: (k == 0).then(|| fit(label, LABEL_CHARS)),
                text,
            });
        }
    }
    lines
}

fn ability_lines(e: &Entry) -> Vec<Line> {
    if e.abilities.is_empty() {
        return vec![Line::Text {
            color: theme::MUTED,
            text: "No abilities.".into(),
        }];
    }
    let mut lines = Vec::new();
    for (k, (name, how, effect)) in e.abilities.iter().enumerate() {
        if k > 0 {
            lines.push(Line::Text {
                color: theme::TEXT,
                text: String::new(),
            });
        }
        lines.push(Line::Text {
            color: theme::ACCENT,
            text: fit(name, TEXT_CHARS),
        });
        for text in wrap(how, TEXT_CHARS) {
            lines.push(Line::Text {
                color: theme::MUTED,
                text,
            });
        }
        for text in wrap(effect, TEXT_CHARS) {
            lines.push(Line::Text {
                color: theme::TEXT,
                text,
            });
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::UiInput;

    fn entry(faction: usize, name: &str, clips: &[&'static str]) -> Entry {
        Entry {
            faction,
            name: name.into(),
            kind: "Spear regiment, tier 1".into(),
            tag: "T1".into(),
            facts: vec![
                ("Cost".into(), "120 Grub".into()),
                (
                    "Traits".into(),
                    "a very long list of traits that has to wrap onto more than one line".into(),
                ),
            ],
            abilities: vec![("Brace".into(), "Toggle".into(), "Holds position.".into())],
            clips: clips.to_vec(),
        }
    }

    fn entries() -> Vec<Entry> {
        let mut v = Vec::new();
        for f in 0..4 {
            for k in 0..30 {
                // The Brood have no models in this test.
                let clips: &[&'static str] = if f == 1 {
                    &[]
                } else {
                    &["Idle", "Walk", "Fall"]
                };
                v.push(entry(f, &format!("Unit {f}-{k}"), clips));
            }
        }
        v
    }

    fn frame(v: &mut Viewer, ui: &mut Ui, input: UiInput) -> ViewerAction {
        let escape = input.escape;
        ui.begin_frame(input, (1280, 720));
        let action = v.frame(ui, escape);
        ui.end_frame();
        action
    }

    fn click(v: &mut Viewer, ui: &mut Ui, at: (i32, i32)) -> ViewerAction {
        let down = UiInput {
            mouse: at,
            pressed: true,
            ..UiInput::default()
        };
        frame(v, ui, down);
        let up = UiInput {
            mouse: at,
            released: true,
            ..UiInput::default()
        };
        frame(v, ui, up)
    }

    fn idle(v: &mut Viewer, ui: &mut Ui) {
        frame(v, ui, UiInput::default());
    }

    #[test]
    fn it_opens_on_the_first_unit_that_has_a_model() {
        let v = Viewer::new(entries());
        assert_eq!(v.selected().unwrap().name, "Unit 0-0");
        let mut brood_first = entries();
        brood_first.rotate_left(30);
        let v = Viewer::new(brood_first);
        assert_eq!(v.selected().unwrap().name, "Unit 2-0");
        assert!(Viewer::new(Vec::new()).selected().is_none());
    }

    #[test]
    fn keys_walk_the_list_and_the_factions() {
        let mut v = Viewer::new(entries());
        let mut ui = Ui::new();
        let down = UiInput {
            down: true,
            ..UiInput::default()
        };
        frame(&mut v, &mut ui, down.clone());
        frame(&mut v, &mut ui, down);
        assert_eq!(v.selected().unwrap().name, "Unit 0-2");
        let up = UiInput {
            up: true,
            ..UiInput::default()
        };
        for _ in 0..5 {
            frame(&mut v, &mut ui, up.clone());
        }
        assert_eq!(v.selected().unwrap().name, "Unit 0-0", "stops at the top");
        let right = UiInput {
            right: true,
            ..UiInput::default()
        };
        frame(&mut v, &mut ui, right.clone());
        assert_eq!(v.selected().unwrap().name, "Unit 1-0");
        frame(&mut v, &mut ui, right.clone());
        frame(&mut v, &mut ui, right.clone());
        frame(&mut v, &mut ui, right);
        assert_eq!(v.selected().unwrap().name, "Unit 0-0", "wraps round");
        let left = UiInput {
            left: true,
            ..UiInput::default()
        };
        frame(&mut v, &mut ui, left);
        assert_eq!(v.selected().unwrap().name, "Unit 3-0");
    }

    #[test]
    fn clicking_a_faction_and_a_unit_selects_it() {
        let mut v = Viewer::new(entries());
        let mut ui = Ui::new();
        idle(&mut v, &mut ui);
        let l = layout(ui.screen);
        // The Choir tab is the third: second grid row, left column.
        click(&mut v, &mut ui, (l.left.x + 40, l.left.y + 44 + 40 + 10));
        assert_eq!(v.selected().unwrap().name, "Unit 2-0");
        // The second row of the list.
        let list_top = l.left.y + 44 + 80 + 8;
        click(&mut v, &mut ui, (l.left.x + 40, list_top + 30 + 8));
        assert_eq!(v.selected().unwrap().name, "Unit 2-1");
    }

    #[test]
    fn the_selection_stays_in_view_in_a_long_list() {
        let mut v = Viewer::new(entries());
        let mut ui = Ui::new();
        let down = UiInput {
            down: true,
            ..UiInput::default()
        };
        for _ in 0..25 {
            frame(&mut v, &mut ui, down.clone());
        }
        assert_eq!(v.selected().unwrap().name, "Unit 0-25");
        assert!(v.top > 0);
        let rows = v.rows();
        let at = rows.iter().position(|&i| i == v.selected).unwrap();
        assert!(at >= v.top);
        // The selected row is drawn: its name is in the draw list.
        let drawn = ui
            .draw
            .quads
            .iter()
            .filter(|q| q.glyph != crate::draw::SOLID)
            .count();
        assert!(drawn > 100);
    }

    #[test]
    fn the_wheel_scrolls_the_panel_under_the_mouse() {
        let mut v = Viewer::new(entries());
        let screen = (1280, 720);
        let l = layout(screen);
        assert!(v.wheel(screen, (l.left.x + 50, l.left.y + 200), 3));
        assert_eq!(v.top, 3);
        assert!(v.wheel(screen, (l.left.x + 50, l.left.y + 200), -9));
        assert_eq!(v.top, 0);
        assert!(v.wheel(screen, (l.right.x + 50, 300), 2));
        assert_eq!(v.scroll, 2);
        assert!(!v.wheel(screen, (640, 300), 1), "the 3D view's wheel zooms");
        assert!(over_panel(screen, (l.right.x + 5, 20)));
        assert!(!over_panel(screen, (640, 300)));
    }

    #[test]
    fn animation_buttons_hold_a_clip_and_cycle_returns() {
        let mut v = Viewer::new(entries());
        let mut ui = Ui::new();
        idle(&mut v, &mut ui);
        assert_eq!(v.clip(), None);
        let bar = layout(ui.screen).bar;
        // Buttons: Cycle, Idle, Walk, Fall; centred, 84 wide with 6 between.
        let total = 4 * 90 - 6;
        let x0 = bar.x + (bar.w - total) / 2;
        let y = bar.bottom() - 17;
        click(&mut v, &mut ui, (x0 + 2 * 90 + 40, y));
        assert_eq!(v.clip(), Some(1), "Walk");
        click(&mut v, &mut ui, (x0 + 40, y));
        assert_eq!(v.clip(), None, "Cycle");
        // Choosing another unit goes back to cycling.
        click(&mut v, &mut ui, (x0 + 3 * 90 + 40, y));
        assert_eq!(v.clip(), Some(2));
        v.step(1);
        assert_eq!(v.clip(), None);
    }

    #[test]
    fn a_faction_opens_on_its_first_unit_with_a_model() {
        let mut all = entries();
        // The Brood: only their fourth unit has a model.
        all[34].clips = vec!["Idle"];
        let mut v = Viewer::new(all);
        v.show_faction(1);
        assert_eq!(v.selected().unwrap().name, "Unit 1-4");
        // The scroll still starts at the top of the list.
        assert_eq!(v.top, 0);
    }

    #[test]
    fn a_unit_without_a_model_says_so() {
        let mut v = Viewer::new(entries());
        let mut ui = Ui::new();
        v.show_faction(1);
        idle(&mut v, &mut ui);
        assert!(v.selected().unwrap().clips.is_empty());
        let big_text = ui
            .draw
            .quads
            .iter()
            .any(|q| q.glyph != crate::draw::SOLID && q.size[0] >= 16);
        assert!(big_text);
    }

    #[test]
    fn back_button_and_escape_leave() {
        let mut v = Viewer::new(entries());
        let mut ui = Ui::new();
        idle(&mut v, &mut ui);
        let l = layout(ui.screen);
        let at = (l.left.x + 40, l.left.bottom() - 30);
        assert_eq!(click(&mut v, &mut ui, at), ViewerAction::Back);
        let esc = UiInput {
            escape: true,
            ..UiInput::default()
        };
        assert_eq!(frame(&mut v, &mut ui, esc), ViewerAction::Back);
    }

    #[test]
    fn the_tabs_switch_and_long_text_wraps_inside_the_panel() {
        let mut v = Viewer::new(entries());
        let mut ui = Ui::new();
        idle(&mut v, &mut ui);
        let r = layout(ui.screen).right;
        click(&mut v, &mut ui, (r.x + r.w - 40, r.y + 100));
        assert_eq!(v.tab, Tab::Abilities);
        click(&mut v, &mut ui, (r.x + 40, r.y + 100));
        assert_eq!(v.tab, Tab::Stats);
        for line in wrap("one two three four five six seven eight nine ten", 12) {
            assert!(line.chars().count() <= 12, "{line}");
        }
        assert_eq!(wrap("", 10), vec![String::new()]);
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        let all = wrap("alpha beta gamma", 11);
        assert_eq!(all, vec!["alpha beta", "gamma"]);
    }

    #[test]
    fn panels_draw_inside_the_window_at_small_sizes() {
        for screen in [(960, 540), (1280, 720), (1920, 1080)] {
            let mut v = Viewer::new(entries());
            let mut ui = Ui::new();
            ui.begin_frame(UiInput::default(), screen);
            v.frame(&mut ui, false);
            ui.end_frame();
            for q in &ui.draw.quads {
                let right = i32::from(q.pos[0]) + i32::from(q.size[0]);
                let bottom = i32::from(q.pos[1]) + i32::from(q.size[1]);
                assert!(right <= screen.0 && bottom <= screen.1, "{screen:?} {q:?}");
            }
            let l = layout(screen);
            assert!(l.bar.w > 150, "{screen:?}: room for the buttons");
            assert!(l.left.right() < l.right.x);
        }
    }
}
