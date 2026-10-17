//! The in-battle HUD: the bottom bar with the minimap, the army tray and the
//! command grid, the alerts over the battlefield and, in Conquest, the top
//! bar.
//! Docs: docs/08-ui-ux.md#screen-layout-1080p-reference.
//!
//! Like the menus this knows nothing about the GPU or the simulation. Each
//! frame the game describes what to show ([`HudData`]) and what the mouse
//! did ([`HudInput`]); [`Hud::frame`] draws into a [`DrawList`] and returns
//! what the player asked for ([`HudEvent`]), which the game turns into
//! camera moves, selections and orders. Map positions are in tiles, with
//! north (smaller z) at the top of the minimap.

use crate::alerts::{self, AlertData};
use crate::conquest::{self, Block, NodeDot, StructurePanel, TopBar};
use crate::draw::{CELL_W, Color, DrawList, Rect, fit, text_width};
use crate::hero::{self, HeroPanel};
use crate::ui::{Press, theme};

/// Height of the bottom bar, in UI pixels.
pub const BAR_HEIGHT: i32 = 168;
/// Height of the top bar (Conquest), in UI pixels.
pub const TOP_BAR_HEIGHT: i32 = 26;
const MARGIN: i32 = 8;
/// Width kept free at the bar's right end for the command grid: room for
/// 7 characters a slot ("300/100").
const GRID_WIDTH: i32 = 276;
const CARD_MIN_W: i32 = 48;
const CARD_MAX_W: i32 = 84;
const CARD_GAP: i32 = 4;
/// The command grid is 4 columns by 3 rows, row by row.
pub const GRID_COLS: usize = 4;
pub const GRID_ROWS: usize = 3;
pub const GRID_SLOTS: usize = GRID_COLS * GRID_ROWS;
/// The key on each slot (docs/08-ui-ux.md: one grid, the same keys everywhere).
pub const GRID_KEYS: [char; GRID_SLOTS] =
    ['Q', 'W', 'E', 'R', 'A', 'S', 'D', 'F', 'Z', 'X', 'C', 'V'];
/// A hero's four abilities take the A, S, D and F slots.
pub const HERO_SLOT_FIRST: usize = 4;
const GRID_GAP: i32 = 4;
/// Two clicks on a card within this long are a double-click.
const DOUBLE_CLICK_MS: u64 = 400;

/// Where the pieces of the HUD sit on a screen of a given size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudLayout {
    pub bar: Rect,
    /// The top bar (Conquest); None while it is hidden.
    pub top: Option<Rect>,
    /// The screen between the top bar, if any, and the bottom bar.
    pub field: Rect,
    pub minimap: Rect,
    pub tray: Rect,
    /// Reserved for the command grid.
    pub grid: Rect,
}

impl HudLayout {
    pub fn new(screen: (i32, i32), top_bar: bool) -> HudLayout {
        let (w, h) = screen;
        let bar = Rect::new(0, (h - BAR_HEIGHT).max(0), w, BAR_HEIGHT.min(h));
        let top = top_bar.then(|| Rect::new(0, 0, w, TOP_BAR_HEIGHT));
        let field_y = top.map_or(0, |r| r.bottom());
        let field = Rect::new(0, field_y, w, (bar.y - field_y).max(0));
        let side = bar.h - 2 * MARGIN;
        let minimap = Rect::new(bar.x + MARGIN, bar.y + MARGIN, side, side);
        let grid_w = GRID_WIDTH.min((w - minimap.right()) / 2).max(0);
        let grid = Rect::new(bar.right() - grid_w - MARGIN, bar.y + MARGIN, grid_w, side);
        let tray_x = minimap.right() + MARGIN;
        let tray = Rect::new(
            tray_x,
            bar.y + MARGIN,
            (grid.x - MARGIN - tray_x).max(0),
            side,
        );
        HudLayout {
            bar,
            top,
            field,
            minimap,
            tray,
            grid,
        }
    }
}

/// A coarse picture of the ground for the minimap: `side` x `side` colours,
/// row by row, north first.
#[derive(Clone, Debug, Default)]
pub struct MinimapTerrain {
    pub side: usize,
    pub colors: Vec<Color>,
}

impl MinimapTerrain {
    /// Sample `colour(u, v)` at the middle of each cell, `u` and `v` running
    /// from 0 (west, north) to 1 (east, south).
    pub fn from_fn(side: usize, colour: impl Fn(f32, f32) -> Color) -> MinimapTerrain {
        let mut colors = Vec::with_capacity(side * side);
        for row in 0..side {
            for col in 0..side {
                let u = (col as f32 + 0.5) / side as f32;
                let v = (row as f32 + 0.5) / side as f32;
                colors.push(colour(u, v));
            }
        }
        MinimapTerrain { side, colors }
    }

    /// The cell [`MinimapTerrain::from_fn`] samples at `(u, v)` in, an
    /// index into `colors`.
    pub fn cell_at(&self, u: f32, v: f32) -> usize {
        let cell = |t: f32| ((t * self.side as f32) as usize).min(self.side - 1);
        cell(v) * self.side + cell(u)
    }
}

/// One regiment on the minimap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dot {
    /// Map position, in tiles.
    pub pos: [f32; 2],
    pub color: [u8; 3],
    pub selected: bool,
}

pub struct MinimapData<'a> {
    /// Side of the map, in tiles.
    pub map_tiles: f32,
    pub terrain: &'a MinimapTerrain,
    pub dots: &'a [Dot],
    /// The ground the camera sees, as a polygon in tiles, clamped to the map
    /// (empty draws nothing).
    pub view: &'a [[f32; 2]],
    /// Gathering spots (Conquest), drawn under the regiments.
    pub nodes: &'a [NodeDot],
    /// Buildings, as their footprints, drawn under the regiments.
    pub blocks: &'a [Block],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CardState {
    #[default]
    Steady,
    Wavering,
    Routing,
}

/// One regiment's card in the army tray.
#[derive(Clone, Debug, Default)]
pub struct Card {
    /// The game's name for this regiment, handed back in events.
    pub id: u32,
    pub name: String,
    pub color: [u8; 3],
    pub models: u16,
    pub max_models: u16,
    /// Morale points, 0 to 100.
    pub morale: u8,
    pub state: CardState,
    pub engaged: bool,
    pub selected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SlotState {
    /// Not available now (no selection, or the game can't do it yet).
    #[default]
    Off,
    Ready,
    /// Waiting for the click that says where.
    Armed,
    /// Shown dim like [`SlotState::Off`], but a click still comes through,
    /// so the game can say why it can't (too poor, tier too low).
    Blocked,
}

/// One button of the command grid.
#[derive(Clone, Debug, Default)]
pub struct GridSlot<'a> {
    /// At most 6 characters.
    pub label: &'a str,
    /// A second, smaller line, e.g. the current formation.
    pub detail: &'a str,
    pub state: SlotState,
}

pub struct HudData<'a> {
    pub grid: &'a [GridSlot<'a>; GRID_SLOTS],
    /// The selected hero, once there are heroes (docs/05-combat.md#heroes).
    pub hero: Option<HeroPanel<'a>>,
    pub minimap: MinimapData<'a>,
    pub cards: &'a [Card],
    /// Shown above the cards, e.g. "Selected" or "Group 2".
    pub tray_title: &'a str,
    /// While the armies deploy (docs/06-game-modes.md).
    pub deployment: Option<DeployStatus>,
    pub alerts: AlertData<'a>,
    /// The economy and the clock across the top of the screen (Conquest).
    pub top: Option<TopBar>,
    /// A selected building: its card and queue replace the regiment cards.
    pub structure: Option<StructurePanel<'a>>,
}

/// The deployment phase, as this player sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeployStatus {
    /// Until the battle starts whatever happens.
    pub seconds_left: u32,
    /// This player has pressed Ready.
    pub ready: bool,
}

/// Widget ids for [`Press`]: a kind in the top byte, the widget's number below.
pub(crate) const WIDGET_READY: u32 = 1 << 24;
pub(crate) const WIDGET_IDLE: u32 = 2 << 24;
const WIDGET_SLOT: u32 = 3 << 24;
const WIDGET_CARD: u32 = 4 << 24;
pub(crate) const WIDGET_ALERT: u32 = 5 << 24;
pub(crate) const WIDGET_QUEUE: u32 = 6 << 24;

/// What the mouse did since the last frame, in UI pixels.
#[derive(Clone, Copy, Debug, Default)]
pub struct HudInput {
    pub mouse: (i32, i32),
    pub left_pressed: bool,
    pub left_released: bool,
    pub right_pressed: bool,
    /// Shift is held.
    pub shift: bool,
    /// Milliseconds on any steady clock, for double-clicks.
    pub now_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HudEvent {
    /// Move the camera to this map position, in tiles.
    Jump([f32; 2]),
    /// Order the selection to this map position.
    Order([f32; 2]),
    /// Select this regiment; with `add`, toggle it in the selection.
    Select { id: u32, add: bool },
    /// Move the camera to this regiment.
    Centre(u32),
    /// A command grid button was clicked (its index, row by row).
    Command(usize),
    /// Done deploying.
    Ready,
    /// An alert badge was clicked (the [`Alert::id`](crate::alerts::Alert::id) it carries).
    Alert(u32),
    /// The idle-worker button was clicked.
    IdleWorker,
    /// Take this unit (its index in the queue) out of the selected building's
    /// training queue.
    Unqueue(usize),
}

#[derive(Default)]
pub struct Hud {
    /// Dragging across the minimap with the left button.
    dragging: bool,
    last_card_click: Option<(u32, u64)>,
    /// Which button the mouse went down on. Buttons fire on release, as the
    /// menus' do; the minimap, like a slider, follows the press.
    press: Press,
    /// The top bar, alert badges and buttons over the battlefield as last drawn.
    over: Vec<Rect>,
}

pub(crate) fn rgb(c: [u8; 3]) -> Color {
    [c[0], c[1], c[2], 255]
}

impl Hud {
    /// Whether a click at `mouse` belongs to the HUD rather than the world.
    pub fn wants(&self, screen: (i32, i32), mouse: (i32, i32)) -> bool {
        self.dragging
            || HudLayout::new(screen, false).bar.contains(mouse.0, mouse.1)
            || self.over.iter().any(|r| r.contains(mouse.0, mouse.1))
    }

    /// Lay out and draw the bar, and report what was clicked.
    pub fn frame(
        &mut self,
        draw: &mut DrawList,
        screen: (i32, i32),
        input: HudInput,
        data: &HudData,
        events: &mut Vec<HudEvent>,
    ) {
        let layout = HudLayout::new(screen, data.top.is_some());
        draw.rect(
            layout.bar,
            [theme::PANEL[0], theme::PANEL[1], theme::PANEL[2], 255],
        );
        draw.rect(
            Rect::new(0, layout.bar.y, layout.bar.w, 2),
            theme::PANEL_EDGE,
        );
        self.minimap(draw, &layout, input, &data.minimap, events);
        self.tray(draw, &layout, input, data, events);
        if let Some(deploy) = data.deployment {
            deploy_panel(draw, &mut self.press, layout.grid, input, deploy, events);
        } else {
            self.grid(draw, &layout, input, data.grid, events);
        }
        self.over.clear();
        if let Some((bar, t)) = layout.top.zip(data.top.as_ref()) {
            conquest::draw_top_bar(draw, bar, t);
            self.over.push(bar);
        }
        if let Some(h) = &data.hero {
            hero::draw(draw, layout.bar.y, h, &mut self.over);
        }
        alerts::draw(
            draw,
            layout.field,
            input,
            &mut self.press,
            data.alerts,
            &mut self.over,
            events,
        );
        self.press.end_frame(input.left_released);
    }

    fn grid(
        &mut self,
        draw: &mut DrawList,
        layout: &HudLayout,
        input: HudInput,
        slots: &[GridSlot; GRID_SLOTS],
        events: &mut Vec<HudEvent>,
    ) {
        let r = layout.grid;
        let cell_w = (r.w - (GRID_COLS as i32 - 1) * GRID_GAP) / GRID_COLS as i32;
        let cell_h = (r.h - (GRID_ROWS as i32 - 1) * GRID_GAP) / GRID_ROWS as i32;
        if cell_w < 24 || cell_h < 24 {
            return;
        }
        for (i, slot) in slots.iter().enumerate() {
            let (col, row) = ((i % GRID_COLS) as i32, (i / GRID_COLS) as i32);
            let cell = Rect::new(
                r.x + col * (cell_w + GRID_GAP),
                r.y + row * (cell_h + GRID_GAP),
                cell_w,
                cell_h,
            );
            let on = slot.state != SlotState::Off;
            let hot = slot.state == SlotState::Ready && cell.contains(input.mouse.0, input.mouse.1);
            let (clicked, _) = self.press.widget(
                WIDGET_SLOT + i as u32,
                cell,
                input.mouse,
                on && input.left_pressed,
                input.left_released,
            );
            // A slot that went off between the press and the release is no click.
            if clicked && on {
                events.push(HudEvent::Command(i));
            }
            let (back, edge, text) = match slot.state {
                SlotState::Off | SlotState::Blocked => {
                    (theme::FIELD, theme::PANEL_EDGE, theme::DISABLED)
                }
                SlotState::Ready if hot => (theme::BUTTON_HOT, theme::PANEL_EDGE, theme::TEXT),
                SlotState::Ready => (theme::BUTTON, theme::PANEL_EDGE, theme::TEXT),
                SlotState::Armed => (theme::PRIMARY, theme::ACCENT, theme::TEXT),
            };
            draw.rect(cell, back);
            draw.border(cell, 1, edge);
            let inner = cell.inset(3);
            let chars = (inner.w / 8).max(1) as usize;
            let mut key = [0u8; 4];
            draw.text(
                inner.x,
                inner.y,
                1,
                theme::MUTED,
                GRID_KEYS[i].encode_utf8(&mut key),
            );
            let label_y = inner.y + 16;
            draw.text(inner.x, label_y, 1, text, &fit(slot.label, chars));
            if !slot.detail.is_empty() {
                draw.text(
                    inner.x,
                    label_y + 14,
                    1,
                    theme::MUTED,
                    &fit(slot.detail, chars),
                );
            }
        }
    }

    fn minimap(
        &mut self,
        draw: &mut DrawList,
        layout: &HudLayout,
        input: HudInput,
        m: &MinimapData,
        events: &mut Vec<HudEvent>,
    ) {
        let r = layout.minimap;
        let (mx, my) = input.mouse;
        let at = |mx: i32, my: i32| -> [f32; 2] {
            let u = ((mx - r.x) as f32 / r.w as f32).clamp(0.0, 1.0);
            let v = ((my - r.y) as f32 / r.h as f32).clamp(0.0, 1.0);
            [u * m.map_tiles, v * m.map_tiles]
        };
        if input.left_pressed && r.contains(mx, my) {
            self.dragging = true;
        }
        if self.dragging {
            events.push(HudEvent::Jump(at(mx, my)));
            if input.left_released {
                self.dragging = false;
            }
        } else if input.right_pressed && r.contains(mx, my) {
            events.push(HudEvent::Order(at(mx, my)));
        }

        draw_terrain(draw, r, m.terrain);

        let to_screen = |p: [f32; 2]| -> (i32, i32) {
            (
                r.x + (p[0] / m.map_tiles * r.w as f32) as i32,
                r.y + (p[1] / m.map_tiles * r.h as f32) as i32,
            )
        };
        let inside = |(x, y): (i32, i32)| r.inset(1).contains(x, y);
        conquest::draw_blocks(draw, r, to_screen, m.blocks);
        conquest::draw_nodes(draw, r, to_screen, m.nodes);
        // Selected regiments are drawn last and larger, so they stay on top.
        for pass in [false, true] {
            for d in m.dots.iter().filter(|d| d.selected == pass) {
                let (x, y) = to_screen(d.pos);
                if !inside((x, y)) {
                    continue;
                }
                let size = if pass { 5 } else { 3 };
                if pass {
                    draw.rect(
                        Rect::new(x - size / 2 - 1, y - size / 2 - 1, size + 2, size + 2),
                        theme::TEXT,
                    );
                }
                draw.rect(
                    Rect::new(x - size / 2, y - size / 2, size, size),
                    rgb(d.color),
                );
            }
        }
        for (i, &a) in m.view.iter().enumerate() {
            let b = m.view[(i + 1) % m.view.len()];
            line(draw, r, to_screen(a), to_screen(b), theme::TEXT);
        }
        draw.border(r, 2, theme::PANEL_EDGE);
    }

    fn tray(
        &mut self,
        draw: &mut DrawList,
        layout: &HudLayout,
        input: HudInput,
        data: &HudData,
        events: &mut Vec<HudEvent>,
    ) {
        let r = layout.tray;
        let inner = r.inset(6);
        if inner.w < CARD_MIN_W {
            return;
        }
        draw.border(r, 2, theme::PANEL_EDGE);
        if let Some(s) = &data.structure {
            draw.text(inner.x, inner.y, 1, theme::MUTED, data.tray_title);
            let row = Rect::new(inner.x, inner.y + 20, inner.w, inner.h - 20);
            conquest::draw_structure(draw, &mut self.press, row, input, s, events);
            return;
        }
        if data.cards.is_empty() {
            draw.text_centered(r, 1, theme::DISABLED, "Select a regiment");
            return;
        }
        draw.text(inner.x, inner.y, 1, theme::MUTED, data.tray_title);
        let row = Rect::new(inner.x, inner.y + 20, inner.w, inner.h - 20);
        let (card_w, shown) = card_fit(row.w, data.cards.len());
        for (i, card) in data.cards.iter().take(shown).enumerate() {
            let rect = Rect::new(
                row.x + i as i32 * (card_w + CARD_GAP),
                row.y,
                card_w,
                row.h.min(CARD_MAX_W * 3 / 2),
            );
            let hot = rect.contains(input.mouse.0, input.mouse.1);
            let (clicked, _) = self.press.widget(
                WIDGET_CARD + i as u32,
                rect,
                input.mouse,
                input.left_pressed,
                input.left_released,
            );
            if clicked {
                let double = self.last_card_click.is_some_and(|(id, at)| {
                    id == card.id && input.now_ms.wrapping_sub(at) < DOUBLE_CLICK_MS
                });
                self.last_card_click = Some((card.id, input.now_ms));
                // The first click of a double-click already selected it.
                events.push(if double {
                    HudEvent::Centre(card.id)
                } else {
                    HudEvent::Select {
                        id: card.id,
                        add: input.shift,
                    }
                });
            }
            draw_card(draw, rect, card, hot);
        }
        if shown < data.cards.len() {
            let more = format!("+{}", data.cards.len() - shown);
            let x = row.right() - text_width(&more, 1);
            draw.text(x.max(inner.x), inner.y, 1, theme::ACCENT, &more);
        }
    }
}

/// The deployment phase, where the command grid will be: time left, what
/// to do, and the Ready button.
fn deploy_panel(
    draw: &mut DrawList,
    press: &mut Press,
    r: Rect,
    input: HudInput,
    deploy: DeployStatus,
    events: &mut Vec<HudEvent>,
) {
    let inner = r.inset(6);
    // The Ready button shows on any screen; the words only where they fit.
    if inner.w < CELL_W * 6 {
        return;
    }
    draw.border(r, 2, theme::ACCENT);
    let left = format!(
        "{}:{:02} left",
        deploy.seconds_left / 60,
        deploy.seconds_left % 60
    );
    let chars = (inner.w / CELL_W).max(1) as usize;
    draw.text(
        inner.x,
        inner.y,
        1,
        theme::ACCENT,
        &fit("DEPLOYMENT", chars),
    );
    draw.text(inner.x, inner.y + 20, 1, theme::TEXT, &fit(&left, chars));
    let hint = if deploy.ready {
        "Waiting for the others."
    } else {
        "Right-drag to place."
    };
    draw.text(inner.x, inner.y + 40, 1, theme::MUTED, &fit(hint, chars));
    let button = Rect::new(inner.x, inner.bottom() - 44, inner.w, 44);
    if deploy.ready {
        draw.rect(button, theme::BUTTON_DOWN);
        draw.text_centered(button, 1, theme::GOOD, "Ready");
        return;
    }
    let hot = button.contains(input.mouse.0, input.mouse.1);
    let (clicked, down) = press.widget(
        WIDGET_READY,
        button,
        input.mouse,
        input.left_pressed,
        input.left_released,
    );
    draw.rect(
        button,
        if down {
            theme::PRIMARY_DOWN
        } else if hot {
            theme::PRIMARY_HOT
        } else {
            theme::PRIMARY
        },
    );
    let scale = if inner.w >= CELL_W * 2 * 6 { 2 } else { 1 };
    draw.text_centered(button, scale, theme::TEXT, "Ready");
    if clicked {
        events.push(HudEvent::Ready);
    }
}

/// How wide each of `n` cards is in a row `width` wide, and how many fit.
fn card_fit(width: i32, n: usize) -> (i32, usize) {
    let n = n.max(1) as i32;
    let ideal = (width - (n - 1) * CARD_GAP) / n;
    if ideal >= CARD_MIN_W {
        return (ideal.min(CARD_MAX_W), n as usize);
    }
    let fit = ((width + CARD_GAP) / (CARD_MIN_W + CARD_GAP)).max(1);
    (CARD_MIN_W, fit as usize)
}

fn draw_card(draw: &mut DrawList, r: Rect, c: &Card, hot: bool) {
    let back = if hot {
        theme::BUTTON_HOT
    } else {
        theme::BUTTON
    };
    draw.rect(r, back);
    let edge = if c.selected {
        theme::ACCENT
    } else {
        theme::PANEL_EDGE
    };
    draw.border(r, if c.selected { 2 } else { 1 }, edge);
    let inner = r.inset(3);

    // The portrait: a team-coloured square with the unit's initial, until
    // there is art for it.
    let portrait = Rect::new(inner.x, inner.y, inner.w, inner.w.min(inner.h / 2));
    draw_portrait(draw, portrait, c.color, &c.name, 2);
    if c.engaged {
        draw.border(portrait, 2, theme::BAD);
    }
    let chars = (inner.w / 8).max(1) as usize;
    let mut y = portrait.bottom() + 3;
    draw.text(inner.x, y, 1, theme::TEXT, &fit(&c.name, chars));
    y += 18;

    // Model count, then morale.
    let bar = |draw: &mut DrawList, y: i32, frac: f32, color: Color| {
        draw.meter(Rect::new(inner.x, y, inner.w, 6), frac, color, theme::FIELD);
    };
    let health = f32::from(c.models) / f32::from(c.max_models.max(1));
    bar(draw, y, health, health_color(health));
    y += 9;
    let morale_color = match c.state {
        CardState::Steady => [96, 156, 230, 255],
        CardState::Wavering => theme::WARN,
        CardState::Routing => theme::BAD,
    };
    bar(draw, y, f32::from(c.morale) / 100.0, morale_color);
    y += 9;
    if c.state == CardState::Routing {
        let label = Rect::new(portrait.x, portrait.bottom() - 18, portrait.w, 16);
        draw.rect(label, [20, 22, 28, 220]);
        draw.text_centered(label, 1, theme::BAD, "ROUT");
    }
    let count = c.models.to_string();
    draw.text(
        inner.right() - text_width(&count, 1),
        y - 2,
        1,
        theme::MUTED,
        &count,
    );
}

pub(crate) fn luma(c: [u8; 3]) -> u32 {
    (u32::from(c[0]) * 3 + u32::from(c[1]) * 6 + u32::from(c[2])) / 10
}

/// A team-coloured square `r` with the first letter of `name` on it, until
/// there is art. The letter is dark on a light colour.
pub(crate) fn draw_portrait(draw: &mut DrawList, r: Rect, color: [u8; 3], name: &str, scale: i32) {
    draw.rect(r, rgb(color));
    if let Some(initial) = name.chars().next() {
        let ink = if luma(color) > 140 {
            [20, 22, 28, 255]
        } else {
            theme::TEXT
        };
        draw.text_centered(r, scale, ink, &initial.to_string());
    }
}

/// Green for a healthy meter, red once `frac` drops under about a third.
pub(crate) fn health_color(frac: f32) -> Color {
    if frac < 0.35 { theme::BAD } else { theme::GOOD }
}

/// The ground picture filling `r` (a blank field when there is none). The
/// minimap and the lobby's map preview both draw it.
pub(crate) fn draw_terrain(draw: &mut DrawList, r: Rect, terrain: &MinimapTerrain) {
    let side = terrain.side;
    let to_px = |i: usize| (i as i32 * r.w) / side.max(1) as i32;
    if side == 0 || terrain.colors.len() < side * side {
        draw.rect(r, theme::FIELD);
        return;
    }
    for row in 0..side {
        // Merge runs of one colour into a single rectangle.
        let mut col = 0;
        while col < side {
            let c = terrain.colors[row * side + col];
            let mut end = col + 1;
            while end < side && terrain.colors[row * side + end] == c {
                end += 1;
            }
            let (x0, x1) = (r.x + to_px(col), r.x + to_px(end));
            let (y0, y1) = (r.y + to_px(row), r.y + to_px(row + 1));
            draw.rect(Rect::new(x0, y0, x1 - x0, (y1 - y0).max(1)), c);
            col = end;
        }
    }
}

/// A 1-pixel line from `a` to `b`, kept inside `clip`.
fn line(draw: &mut DrawList, clip: Rect, a: (i32, i32), b: (i32, i32), color: Color) {
    let clip = clip.inset(2);
    let steps = (b.0 - a.0).abs().max((b.1 - a.1).abs()).max(1);
    for s in 0..=steps {
        let x = a.0 + (b.0 - a.0) * s / steps;
        let y = a.1 + (b.1 - a.1) * s / steps;
        if clip.contains(x, y) {
            draw.rect(Rect::new(x, y, 1, 1), color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conquest::{GRUB_NODE, QUEUE_SLOT_MAX_W, QueueItem, STRUCT_CARD_W};

    const SCREEN: (i32, i32) = (1280, 760);

    fn terrain() -> MinimapTerrain {
        MinimapTerrain::from_fn(8, |_, _| [40, 90, 40, 255])
    }

    fn card(id: u32) -> Card {
        Card {
            id,
            name: "Spearmen".into(),
            color: [200, 60, 50],
            models: 40,
            max_models: 60,
            morale: 80,
            ..Card::default()
        }
    }

    /// The Conquest extras a test may add to a frame.
    #[derive(Default)]
    struct Extras<'a> {
        top: Option<TopBar>,
        structure: Option<StructurePanel<'a>>,
        nodes: &'a [NodeDot],
        blocks: &'a [Block],
    }

    fn run(
        hud: &mut Hud,
        input: HudInput,
        cards: &[Card],
        dots: &[Dot],
    ) -> (Vec<HudEvent>, DrawList) {
        run_with(hud, input, cards, dots, Extras::default())
    }

    fn run_with(
        hud: &mut Hud,
        input: HudInput,
        cards: &[Card],
        dots: &[Dot],
        extras: Extras,
    ) -> (Vec<HudEvent>, DrawList) {
        let t = terrain();
        let grid: [GridSlot; GRID_SLOTS] = std::array::from_fn(|i| GridSlot {
            label: "Move",
            detail: "",
            state: match i {
                0 => SlotState::Ready,
                2 => SlotState::Blocked,
                _ => SlotState::Off,
            },
        });
        let data = HudData {
            grid: &grid,
            minimap: MinimapData {
                map_tiles: 256.0,
                terrain: &t,
                dots,
                view: &[],
                nodes: extras.nodes,
                blocks: extras.blocks,
            },
            cards,
            tray_title: "Selected",
            deployment: None,
            alerts: AlertData::default(),
            hero: None,
            top: extras.top,
            structure: extras.structure,
        };
        let mut draw = DrawList::default();
        let mut events = Vec::new();
        hud.frame(&mut draw, SCREEN, input, &data, &mut events);
        (events, draw)
    }

    #[test]
    fn the_ready_button_ends_your_deployment() {
        let l = HudLayout::new(SCREEN, false);
        let button = (l.grid.x + l.grid.w / 2, l.grid.bottom() - 20);
        let t = terrain();
        let grid: [GridSlot; GRID_SLOTS] = std::array::from_fn(|_| GridSlot::default());
        let frame = |ready, input| {
            let data = HudData {
                grid: &grid,
                minimap: MinimapData {
                    map_tiles: 256.0,
                    terrain: &t,
                    dots: &[],
                    view: &[],
                    nodes: &[],
                    blocks: &[],
                },
                cards: &[],
                tray_title: "",
                deployment: Some(DeployStatus {
                    seconds_left: 87,
                    ready,
                }),
                alerts: AlertData::default(),
                hero: None,
                top: None,
                structure: None,
            };
            let (mut draw, mut events) = (DrawList::default(), Vec::new());
            Hud::default().frame(&mut draw, SCREEN, input, &data, &mut events);
            events
        };
        let click = HudInput {
            mouse: button,
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        };
        assert_eq!(frame(false, click), vec![HudEvent::Ready]);
        assert!(frame(true, click).is_empty(), "already ready");
        assert!(Hud::default().wants(SCREEN, button));
    }

    #[test]
    fn hud_buttons_fire_on_release_over_the_same_button() {
        let l = HudLayout::new(SCREEN, false);
        let ready = (l.grid.x + l.grid.w / 2, l.grid.bottom() - 20);
        let t = terrain();
        let grid: [GridSlot; GRID_SLOTS] = std::array::from_fn(|_| GridSlot::default());
        let mut hud = Hud::default();
        let mut frame = |input| {
            let data = HudData {
                grid: &grid,
                minimap: MinimapData {
                    map_tiles: 256.0,
                    terrain: &t,
                    dots: &[],
                    view: &[],
                    nodes: &[],
                    blocks: &[],
                },
                cards: &[],
                tray_title: "",
                deployment: Some(DeployStatus {
                    seconds_left: 87,
                    ready: false,
                }),
                alerts: AlertData::default(),
                hero: None,
                top: None,
                structure: None,
            };
            let (mut draw, mut events) = (DrawList::default(), Vec::new());
            hud.frame(&mut draw, SCREEN, input, &data, &mut events);
            events
        };
        let at = |mouse, left_pressed, left_released| HudInput {
            mouse,
            left_pressed,
            left_released,
            ..HudInput::default()
        };
        assert!(frame(at(ready, true, false)).is_empty(), "not on press");
        assert!(frame(at(ready, false, false)).is_empty(), "nor while held");
        assert_eq!(frame(at(ready, false, true)), vec![HudEvent::Ready]);
        // Pressed on Ready, released elsewhere: no click, and the next
        // release over Ready is not one either.
        frame(at(ready, true, false));
        assert!(frame(at((5, 5), false, true)).is_empty());
        assert!(frame(at(ready, false, true)).is_empty());
        // Pressed elsewhere, released over Ready.
        frame(at((5, 5), true, false));
        assert!(frame(at(ready, false, true)).is_empty());
    }

    #[test]
    fn a_grid_button_fires_on_release_not_press() {
        let mut hud = Hud::default();
        let l = HudLayout::new(SCREEN, false);
        let spot = (l.grid.x + 6, l.grid.y + 6);
        let at = |left_pressed, left_released| HudInput {
            mouse: spot,
            left_pressed,
            left_released,
            ..HudInput::default()
        };
        let (ev, _) = run(&mut hud, at(true, false), &[], &[]);
        assert!(ev.is_empty());
        let (ev, _) = run(&mut hud, at(false, true), &[], &[]);
        assert_eq!(ev, vec![HudEvent::Command(0)]);
    }

    #[test]
    fn the_hero_panel_takes_the_mouse_from_the_battlefield() {
        let grid: [GridSlot; GRID_SLOTS] = std::array::from_fn(|_| GridSlot::default());
        let t = terrain();
        let hero = HeroPanel {
            name: "The Daimyo",
            role: "Commander",
            color: [200, 60, 50],
            level: 1,
            hp: 10,
            max_hp: 10,
            xp: 0.0,
            abilities: Default::default(),
            items: [None; 3],
            revive_secs: None,
        };
        let data = HudData {
            grid: &grid,
            hero: Some(hero),
            minimap: MinimapData {
                map_tiles: 256.0,
                terrain: &t,
                dots: &[],
                view: &[],
                nodes: &[],
                blocks: &[],
            },
            cards: &[],
            tray_title: "",
            deployment: None,
            alerts: AlertData::default(),
            top: None,
            structure: None,
        };
        let l = HudLayout::new(SCREEN, false);
        let over_panel = (30, l.bar.y - 40);
        let mut hud = Hud::default();
        assert!(!hud.wants(SCREEN, over_panel), "no panel drawn yet");
        let (mut draw, mut events) = (DrawList::default(), Vec::new());
        hud.frame(&mut draw, SCREEN, HudInput::default(), &data, &mut events);
        assert!(hud.wants(SCREEN, over_panel));
        assert!(
            !hud.wants(SCREEN, (600, l.bar.y - 40)),
            "the field beside it"
        );
    }

    #[test]
    fn layout_pieces_do_not_overlap_and_stay_on_screen() {
        for screen in [(1280, 760), (1920, 1080), (1024, 600)] {
            let l = HudLayout::new(screen, false);
            for r in [l.minimap, l.tray, l.grid] {
                assert!(r.x >= 0 && r.bottom() <= screen.1 && r.right() <= screen.0);
                assert!(l.bar.contains(r.x, r.y));
            }
            assert!(l.minimap.right() <= l.tray.x);
            assert!(l.tray.right() <= l.grid.x);
        }
    }

    #[test]
    fn minimap_click_jumps_to_that_spot() {
        let mut hud = Hud::default();
        let l = HudLayout::new(SCREEN, false);
        let mid = (l.minimap.x + l.minimap.w / 2, l.minimap.y + l.minimap.h / 2);
        let (ev, _) = run(
            &mut hud,
            HudInput {
                mouse: mid,
                left_pressed: true,
                ..HudInput::default()
            },
            &[],
            &[],
        );
        let [x, z] = match ev[0] {
            HudEvent::Jump(p) => p,
            e => panic!("{e:?}"),
        };
        assert!((x - 128.0).abs() < 2.0 && (z - 128.0).abs() < 2.0);
        assert!(hud.wants(SCREEN, (0, 0)));
    }

    #[test]
    fn dragging_follows_the_mouse_off_the_minimap_until_release() {
        let mut hud = Hud::default();
        let l = HudLayout::new(SCREEN, false);
        let at = |mouse| HudInput {
            mouse,
            ..HudInput::default()
        };
        run(
            &mut hud,
            HudInput {
                left_pressed: true,
                ..at((l.minimap.x + 4, l.minimap.y + 4))
            },
            &[],
            &[],
        );
        // Far above the bar: the jump is clamped to the map's edge.
        let (ev, _) = run(&mut hud, at((l.minimap.x + 4, 0)), &[], &[]);
        assert_eq!(
            ev,
            vec![HudEvent::Jump([256.0 * 4.0 / l.minimap.w as f32, 0.0])]
        );
        assert!(hud.wants(SCREEN, (0, 0)));
        run(
            &mut hud,
            HudInput {
                left_released: true,
                ..at((0, 0))
            },
            &[],
            &[],
        );
        assert!(!hud.wants(SCREEN, (0, 0)));
    }

    #[test]
    fn right_click_on_the_minimap_orders() {
        let mut hud = Hud::default();
        let l = HudLayout::new(SCREEN, false);
        let (ev, _) = run(
            &mut hud,
            HudInput {
                mouse: (l.minimap.x + 1, l.minimap.y + 1),
                right_pressed: true,
                ..HudInput::default()
            },
            &[],
            &[],
        );
        assert!(matches!(ev[..], [HudEvent::Order(_)]));
    }

    #[test]
    fn clicks_outside_the_bar_are_the_worlds() {
        let hud = Hud::default();
        assert!(hud.wants(SCREEN, (10, SCREEN.1 - 5)));
        assert!(!hud.wants(SCREEN, (10, 10)));
    }

    #[test]
    fn clicking_a_card_selects_and_double_click_centres() {
        let mut hud = Hud::default();
        let l = HudLayout::new(SCREEN, false);
        let cards = [card(7), card(9)];
        let click = |ms, shift| HudInput {
            mouse: (l.tray.x + 20, l.tray.y + 40),
            left_pressed: true,
            left_released: true,
            shift,
            now_ms: ms,
            ..HudInput::default()
        };
        let (ev, _) = run(&mut hud, click(1000, false), &cards, &[]);
        assert_eq!(ev, vec![HudEvent::Select { id: 7, add: false }]);
        let (ev, _) = run(&mut hud, click(1200, true), &cards, &[]);
        assert_eq!(ev, vec![HudEvent::Centre(7)]);
        // Too slow to be a double-click.
        let (ev, _) = run(&mut hud, click(2000, false), &cards, &[]);
        assert_eq!(ev.len(), 1);
    }

    #[test]
    fn a_crowd_of_cards_shrinks_then_shows_a_count() {
        assert_eq!(card_fit(600, 2).1, 2);
        let (w, n) = card_fit(600, 40);
        assert_eq!(w, CARD_MIN_W);
        assert!(n < 40 && n as i32 * (w + CARD_GAP) - CARD_GAP <= 600);
        let (_, n) = card_fit(10, 5);
        assert_eq!(n, 1);
    }

    #[test]
    fn draws_dots_and_stays_under_the_quad_cap() {
        let mut hud = Hud::default();
        let dots: Vec<Dot> = (0..400)
            .map(|i| Dot {
                pos: [(i % 20) as f32 * 12.0, (i / 20) as f32 * 12.0],
                color: [200, 60, 50],
                selected: i % 7 == 0,
            })
            .collect();
        let cards: Vec<Card> = (0..30).map(card).collect();
        let (_, draw) = run(&mut hud, HudInput::default(), &cards, &dots);
        assert!(draw.quads.len() < 3000, "{}", draw.quads.len());
        assert!(!draw.quads.is_empty());
    }

    #[test]
    fn only_a_ready_or_blocked_button_reports_a_click() {
        let mut hud = Hud::default();
        let l = HudLayout::new(SCREEN, false);
        let cell_w = (l.grid.w - 3 * GRID_GAP) / GRID_COLS as i32;
        let cell_h = (l.grid.h - 2 * GRID_GAP) / GRID_ROWS as i32;
        let at = |col: i32, row: i32| HudInput {
            mouse: (
                l.grid.x + 6 + col * (cell_w + GRID_GAP),
                l.grid.y + 6 + row * (cell_h + GRID_GAP),
            ),
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        };
        let (ev, _) = run(&mut hud, at(0, 0), &[], &[]);
        assert_eq!(ev, vec![HudEvent::Command(0)]);
        // The next slot is off in this test, and clicks on it do nothing.
        let (ev, _) = run(&mut hud, at(1, 0), &[], &[]);
        assert!(ev.is_empty());
        let (ev, _) = run(&mut hud, at(3, 2), &[], &[]);
        assert!(ev.is_empty());
        // A blocked one looks off but still reports, so the game can say why.
        let (ev, _) = run(&mut hud, at(2, 0), &[], &[]);
        assert_eq!(ev, vec![HudEvent::Command(2)]);
    }

    #[test]
    fn empty_tray_says_so() {
        let mut hud = Hud::default();
        let (_, draw) = run(&mut hud, HudInput::default(), &[], &[]);
        assert!(draw.quads.len() > 10);
    }

    fn top_bar() -> TopBar {
        TopBar {
            grub: 1240,
            marrow: 185,
            supply_used: 64,
            supply_cap: 90,
            upkeep_percent: 100,
            larvae: 0,
            larva_room: 0,
            clock_secs: 14 * 60 + 32,
        }
    }

    /// How many quads of `colour` sit inside the top bar.
    fn in_top_bar(draw: &DrawList, colour: Color) -> usize {
        draw.quads
            .iter()
            .filter(|q| i32::from(q.pos[1]) < TOP_BAR_HEIGHT && q.color == colour)
            .count()
    }

    fn with_top(top: TopBar) -> Extras<'static> {
        Extras {
            top: Some(top),
            ..Extras::default()
        }
    }

    #[test]
    fn the_top_bar_is_drawn_only_when_given_and_keeps_the_mouse() {
        let mouse = (400, 5);
        let mut hud = Hud::default();
        let (_, draw) = run(&mut hud, HudInput::default(), &[], &[]);
        assert!(
            draw.quads
                .iter()
                .all(|q| i32::from(q.pos[1]) >= TOP_BAR_HEIGHT)
        );
        assert!(!hud.wants(SCREEN, mouse));

        let (_, draw) = run_with(&mut hud, HudInput::default(), &[], &[], with_top(top_bar()));
        assert!(
            draw.quads
                .iter()
                .any(|q| q.pos == [0, 0] && q.size == [SCREEN.0 as u16, TOP_BAR_HEIGHT as u16])
        );
        assert!(hud.wants(SCREEN, mouse));
        assert!(hud.wants(SCREEN, (0, TOP_BAR_HEIGHT - 1)));
        assert!(!hud.wants(SCREEN, (400, TOP_BAR_HEIGHT + 4)), "the field");
        // Hidden again: the click is the world's.
        run(&mut hud, HudInput::default(), &[], &[]);
        assert!(!hud.wants(SCREEN, mouse));
    }

    #[test]
    fn supply_at_the_cap_turns_red_and_upkeep_shows_in_amber() {
        let mut hud = Hud::default();
        let mut bar = |top| run_with(&mut hud, HudInput::default(), &[], &[], with_top(top)).1;
        let roomy = bar(top_bar());
        assert_eq!(in_top_bar(&roomy, theme::BAD), 0);
        assert_eq!(in_top_bar(&roomy, theme::WARN), 0);
        let full = bar(TopBar {
            supply_used: 90,
            ..top_bar()
        });
        assert_eq!(in_top_bar(&full, theme::BAD), "90/90".len());
        let taxed = bar(TopBar {
            upkeep_percent: 80,
            ..top_bar()
        });
        assert_eq!(in_top_bar(&taxed, theme::WARN), "80%".len());
    }

    #[test]
    fn larvae_show_in_the_top_bar_only_with_room_for_them() {
        let mut hud = Hud::default();
        let mut bar = |top| run_with(&mut hud, HudInput::default(), &[], &[], with_top(top)).1;
        let plain = bar(top_bar());
        let brood = bar(TopBar {
            larvae: 2,
            larva_room: 6,
            ..top_bar()
        });
        let text_quads = |d: &DrawList| in_top_bar(d, theme::TEXT) + in_top_bar(d, theme::MUTED);
        assert_eq!(
            text_quads(&brood),
            text_quads(&plain) + "Larvae".len() + "2/6".len()
        );
        // None ready: the count turns amber.
        let empty = bar(TopBar {
            larvae: 0,
            larva_room: 3,
            ..top_bar()
        });
        assert_eq!(in_top_bar(&empty, theme::WARN), "0/3".len());
        assert_eq!(in_top_bar(&plain, theme::WARN), 0);
    }

    #[test]
    fn the_minimap_draws_nothing_extra_without_marks() {
        let (_, plain) = run(&mut Hud::default(), HudInput::default(), &[], &[]);
        let nodes = [
            NodeDot {
                pos: [64.0, 64.0],
                marrow: false,
                dry: false,
            },
            NodeDot {
                pos: [192.0, 64.0],
                marrow: true,
                dry: true,
            },
        ];
        let blocks = [Block {
            rect: [100.0, 150.0, 8.0, 8.0],
            color: [30, 120, 200],
            selected: true,
        }];
        let (_, marked) = run_with(
            &mut Hud::default(),
            HudInput::default(),
            &[],
            &[],
            Extras {
                nodes: &nodes,
                blocks: &blocks,
                ..Extras::default()
            },
        );
        // A quad a node, and a fill and a rim for the selected block.
        assert_eq!(marked.quads.len(), plain.quads.len() + 2 + 2);
        let drawn = |colour: [u8; 3]| marked.quads.iter().any(|q| q.color == rgb(colour));
        assert!(drawn(GRUB_NODE), "Grub");
        assert!(drawn([105, 101, 90]), "dry Marrow, dimmed");
        assert!(drawn([30, 120, 200]), "the block");
        assert!(!plain.quads.iter().any(|q| q.color == rgb([30, 120, 200])));
    }

    fn queue<'a>() -> [QueueItem<'a>; 2] {
        [
            QueueItem {
                name: "Spearmen",
                progress: 40,
            },
            QueueItem {
                name: "Archers",
                progress: 0,
            },
        ]
    }

    fn panel<'a>(queue: &'a [QueueItem<'a>]) -> Extras<'a> {
        Extras {
            structure: Some(StructurePanel {
                name: "Clan Hall",
                color: [30, 120, 200],
                hp: 900,
                max_hp: 1200,
                building: None,
                upgrading: Some(("Clan Keep", 25)),
                queue,
                larvae: None,
            }),
            ..Extras::default()
        }
    }

    /// A point inside queue slot `i` of the tray.
    fn queue_slot(i: i32) -> (i32, i32) {
        let row = HudLayout::new(SCREEN, false).tray.inset(6);
        let step = QUEUE_SLOT_MAX_W + conquest::CARD_GAP;
        (
            row.x + STRUCT_CARD_W + conquest::CARD_GAP + 10 + i * step,
            row.y + 20 + 10,
        )
    }

    #[test]
    fn a_selected_building_replaces_the_cards_with_its_queue() {
        let items = queue();
        let cards = [card(7)];
        let blue = rgb([30, 120, 200]);
        let (_, with_cards) = run(&mut Hud::default(), HudInput::default(), &cards, &[]);
        let (_, with_panel) = run_with(
            &mut Hud::default(),
            HudInput::default(),
            &cards,
            &[],
            panel(&items),
        );
        assert!(!with_cards.quads.iter().any(|q| q.color == blue));
        assert!(with_panel.quads.iter().any(|q| q.color == blue));
        // The regiment's red portrait is gone.
        let red = rgb([200, 60, 50]);
        assert!(with_cards.quads.iter().any(|q| q.color == red));
        assert!(!with_panel.quads.iter().any(|q| q.color == red));
        // A click where the card was selects nothing now.
        let l = HudLayout::new(SCREEN, false);
        let click = HudInput {
            mouse: (l.tray.x + 20, l.tray.y + 40),
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        };
        let (ev, _) = run_with(&mut Hud::default(), click, &cards, &[], panel(&items));
        assert!(ev.is_empty(), "{ev:?}");
    }

    #[test]
    fn clicking_a_filled_queue_slot_unqueues_it_and_an_empty_one_does_nothing() {
        let items = queue();
        let mut hud = Hud::default();
        let at = |mouse, left_pressed, left_released| HudInput {
            mouse,
            left_pressed,
            left_released,
            ..HudInput::default()
        };
        let mut frame = |input| run_with(&mut hud, input, &[], &[], panel(&items)).0;
        // Fires on the release, not the press.
        assert!(frame(at(queue_slot(1), true, false)).is_empty());
        assert_eq!(
            frame(at(queue_slot(1), false, true)),
            vec![HudEvent::Unqueue(1)]
        );
        assert_eq!(
            frame(at(queue_slot(0), true, true)),
            vec![HudEvent::Unqueue(0)]
        );
        // Empty slots do nothing.
        assert!(frame(at(queue_slot(2), true, true)).is_empty());
        assert!(frame(at(queue_slot(4), true, true)).is_empty());
        // Pressed on one slot, let go over another: no click.
        frame(at(queue_slot(0), true, false));
        assert!(frame(at(queue_slot(1), false, true)).is_empty());
    }
}
