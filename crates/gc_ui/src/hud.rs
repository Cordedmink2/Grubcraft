//! The in-battle HUD: the bottom bar with the minimap, the army tray and the
//! command grid, the alerts over the battlefield and, in Conquest, the top
//! bar.
//! Docs: docs/08-ui-ux.md#screen-layout-1080p-reference.
//!
//! The look is a Bayeux tapestry: linen cloth, ink outlines and stitched
//! borders in madder, woad, weld and wode wool (docs/08-ui-ux.md#visual-style).
//!
//! Like the menus this knows nothing about the GPU or the simulation. Each
//! frame the game describes what to show ([`HudData`]) and what the mouse
//! did ([`HudInput`]); [`Hud::frame`] draws into a [`DrawList`] and returns
//! what the player asked for ([`HudEvent`]), which the game turns into
//! camera moves, selections and orders. Map positions are in tiles, with
//! north (smaller z) at the top of the minimap.

use crate::alerts::{self, AlertData};
use crate::conquest::{self, Block, NodeDot, StructurePanel, TipsCard, TopBar};
use crate::draw::{CELL_W, Color, DrawList, Rect, fit, mix, text_height, text_width, wrap};
use crate::hero::{self, HeroPanel};
use crate::icons::{self, Icon};
use crate::placer::{self, PlacePicker};
use crate::powers::{self, PowerButton};
use crate::ui::{Press, theme};

/// Height of the bottom bar, in UI pixels.
pub const BAR_HEIGHT: i32 = 168;
/// Height of the top bar (Conquest), in UI pixels.
pub const TOP_BAR_HEIGHT: i32 = 36;
const MARGIN: i32 = 8;
/// Width kept free at the bar's right end for the command grid: room for
/// 9 characters a slot ("Formation").
const GRID_WIDTH: i32 = 348;
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
/// A button must be hovered this long before its tooltip shows, in ms.
pub const TOOLTIP_DELAY_MS: u64 = 350;
/// Two clicks on a card within this long are a double-click.
const DOUBLE_CLICK_MS: u64 = 400;

/// Where the pieces of the HUD sit on a screen of a given size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudLayout {
    pub bar: Rect,
    /// The top bar: the status strip, the Help button and, in Conquest, the
    /// economy.
    pub top: Rect,
    /// The screen between the top bar, if any, and the bottom bar.
    pub field: Rect,
    pub minimap: Rect,
    pub tray: Rect,
    /// Reserved for the command grid.
    pub grid: Rect,
}

impl HudLayout {
    pub fn new(screen: (i32, i32)) -> HudLayout {
        let (w, h) = screen;
        let bar = Rect::new(0, (h - BAR_HEIGHT).max(0), w, BAR_HEIGHT.min(h));
        let top = Rect::new(0, 0, w, TOP_BAR_HEIGHT.min(h));
        let field_y = top.bottom();
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
    /// The button's readable name ("Formation"); at most 9 characters fit.
    pub label: &'a str,
    /// A second, smaller line, e.g. the current formation.
    pub detail: &'a str,
    pub state: SlotState,
    /// The picture on the button.
    pub icon: Icon,
    /// One line for the tooltip; empty gives the name and key alone.
    pub tip: &'a str,
}

/// What the selected regiments are, for the selection panel in the tray.
#[derive(Clone, Copy, Debug, Default)]
pub struct Selection<'a> {
    /// The regiment's name, or "3 regiments" for a mixed selection.
    pub name: &'a str,
    /// The side's colour: the banner's cloth.
    pub color: [u8; 3],
    /// The picture on the banner.
    pub icon: Icon,
    /// How many regiments are selected.
    pub regiments: u16,
    /// Soldiers standing and soldiers at full strength, summed.
    pub models: u32,
    pub max_models: u32,
    /// Morale points, 0 to 100 (the mean of a selection).
    pub morale: u8,
    pub state: CardState,
    /// The formation they stand in; empty when it varies or is not known.
    pub formation: &'a str,
    /// The army shapes to choose from, as a row of chips along the tray's top
    /// edge. Empty for none, and the row only shows for two or more
    /// regiments.
    pub shapes: &'a [ShapeChip<'a>],
}

/// One army shape to choose from: a preset, or a shape you saved.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShapeChip<'a> {
    /// "Line", "Mine 2" ...
    pub label: &'a str,
    /// One line on what it does, for the tooltip.
    pub tip: &'a str,
    /// This is the shape group orders use now.
    pub on: bool,
}

/// The top bar's status strip: what you are playing and how fast.
#[derive(Clone, Copy, Debug, Default)]
pub struct StatusStrip<'a> {
    /// "Sandbox", "Field Battle", "Replay" ...
    pub mode: &'a str,
    /// The side commanded, e.g. "Red".
    pub side: &'a str,
    pub side_color: [u8; 3],
    /// "1x", "0.5x" ...
    pub speed: &'a str,
    pub paused: bool,
    /// Match time.
    pub clock_secs: Option<u32>,
}

/// One key and what it does, for the Marshal's Scroll.
pub type HelpKey = (&'static str, &'static str);

/// The keys of the Marshal's Scroll, in groups. The scroll reads this.
pub const HELP_GROUPS: [(&str, &[HelpKey]); 3] = [
    (
        "Orders",
        &[
            ("L-click", "Select a regiment"),
            ("L-drag", "Box select"),
            ("R-click", "March or attack"),
            ("R-drag", "Draw the line"),
            ("Q", "Move, then click"),
            ("W", "Attack, then click"),
            ("R or F", "Next formation"),
            ("O", "Next army shape"),
            ("Shift+O", "Save this army's shape"),
            ("Z", "Brace spears"),
            ("V", "Fall back"),
            ("Ctrl+1-9", "Save a group"),
            ("1-9", "Recall a group"),
            ("Space", "Jump to alert"),
            ("R, Wheel", "Turn a building you place"),
            ("Esc", "Cancel, or menu"),
        ],
    ),
    (
        "View",
        &[
            ("Arrows", "Pan the camera"),
            ("Wheel", "Zoom"),
            ("Alt+Mid", "Rotate the view"),
            (", .", "Turn the view"),
            ("Home", "Reset rotation"),
            ("Y", "Ride with banner"),
            ("G", "Show blocked ground"),
            ("F9", "Change the light"),
            ("L", "AI lens"),
            ("C", "Credits"),
            ("M  N", "Mute  music"),
            ("F11", "Dev overlay"),
            ("H or F1", "This scroll"),
            ("U", "Guide"),
            ("F1", "Idle worker"),
        ],
    ),
    (
        "Sandbox",
        &[
            ("Tab", "Next side"),
            ("I", "Computer plays side"),
            ("P", "Pause"),
            ("- =", "Slower, faster"),
            ("B", "Place panel: Field types or any faction"),
            ("[ ]", "Choose the type"),
            ("K", "Place buildings"),
            ("Del", "Remove under mouse"),
            ("Shift+Del", "Clear the field"),
            ("T", "Tune stats"),
            ("Shift+R", "Reload Field types"),
            ("Shift+V", "Show hidden"),
            ("Shift+H", "Hero preview"),
        ],
    ),
];

/// The Marshal's Scroll, when it is open.
#[derive(Clone, Copy, Debug, Default)]
pub struct HelpScroll<'a> {
    /// Live lines under the keys: what the computer players are doing, the
    /// formation, what is being placed.
    pub notes: &'a [String],
}

pub struct HudData<'a> {
    pub grid: &'a [GridSlot<'a>; GRID_SLOTS],
    /// The top bar's mode, side, speed and clock.
    pub status: StatusStrip<'a>,
    /// The selected regiments, for the panel in the tray.
    pub selection: Option<Selection<'a>>,
    /// The Marshal's Scroll; `None` while it is closed.
    pub help: Option<HelpScroll<'a>>,
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
    /// The faction powers you can use (Conquest), as a row of buttons over
    /// the battlefield; none draws nothing.
    pub powers: &'a [PowerButton<'a>],
    /// The Place panel and its top-bar button (Sandbox); `None` hides both.
    pub picker: Option<PlacePicker<'a>>,
    /// Tips on the building being placed or selected (Conquest); `None`
    /// draws nothing.
    pub tips: Option<TipsCard<'a>>,
    /// The Hand over button (Field Battle and Conquest with allies); `None`
    /// hides it.
    pub handover: Option<HandOverButton<'a>>,
}

/// The top bar's Hand over button: who your army answers to now.
#[derive(Clone, Copy, Debug, Default)]
pub struct HandOverButton<'a> {
    /// The button's text: "Hand over", or "With <ally>" while the army is away.
    pub label: &'a str,
    /// What a click does next, for the tooltip.
    pub tip: &'a str,
    /// Your army is with an ally now.
    pub lit: bool,
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
pub(crate) const WIDGET_POWER: u32 = 7 << 24;
pub(crate) const WIDGET_HELP: u32 = 8 << 24;
pub(crate) const WIDGET_GUIDE: u32 = 9 << 24;
pub(crate) const WIDGET_PLACE: u32 = 10 << 24;
pub(crate) const WIDGET_PLACE_ROW: u32 = 11 << 24;
pub(crate) const WIDGET_PLACE_SIDE: u32 = 12 << 24;
const WIDGET_SHAPE: u32 = 13 << 24;
pub(crate) const WIDGET_TIPS: u32 = 14 << 24;
const WIDGET_HANDOVER: u32 = 15 << 24;

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
    /// A faction power's button was clicked (its place in
    /// [`HudData::powers`]).
    Power(usize),
    /// The `? Help` button was clicked: open or close the Marshal's Scroll.
    ToggleHelp,
    /// The Guide button was clicked: open the Guide.
    OpenGuide,
    /// The Place button (or the panel's close box) was clicked: open or close
    /// the Place panel.
    TogglePlacer,
    /// A troop row of the Place panel was clicked (its place in
    /// [`PlacePicker::rows`]): choose the type and arm placing.
    PlaceUnit(usize),
    /// A side of the Place panel was clicked (its place in
    /// [`PlacePicker::sides`]): place for, and command, that side.
    PlaceSide(usize),
    /// A tab of the Place panel was clicked (its place in
    /// [`PlacePicker::tabs`]): show that table's troops.
    PlaceTab(usize),
    /// A faction chip of the Place panel's roster tab was clicked (its place
    /// in [`PlacePicker::chips`]).
    PlaceFaction(usize),
    /// The info button of a selected building (or its tips card's close
    /// box) was clicked: show or hide the building's tips.
    ToggleTips,
    /// An army shape chip of the selection panel was clicked (its place in
    /// [`Selection::shapes`]).
    ArmyShape(usize),
    /// The Hand over button was clicked: pass your army to the next ally, or
    /// take it back after the last.
    HandOver,
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
    /// The button the mouse rests on and since when (ms), for tooltips.
    hover: Option<(u32, u64)>,
    /// A tooltip to show once the mouse has rested long enough, this frame.
    tip: Option<Tip>,
    /// Where the tooltip was last drawn.
    tooltip: Option<Rect>,
}

/// Widget id of the Help button's tooltip; grid slots use their index.
const TIP_HELP: u32 = 100;
const TIP_GUIDE: u32 = 101;
const TIP_PLACE: u32 = 102;
const TIP_HANDOVER: u32 = 103;
/// Army shape chips' tooltips use this and their place.
const TIP_SHAPE: u32 = 110;

/// What a tooltip says, about the button `id` at `anchor`.
#[derive(Clone, Debug)]
struct Tip {
    id: u32,
    anchor: Rect,
    name: String,
    key: String,
    text: String,
    note: String,
}

/// A button's background, edge and text colours: dim while it can't be
/// used, lit under the mouse (`hot`) and weld yellow while it waits for a
/// click.
pub(crate) fn slot_colors(state: SlotState, hot: bool) -> (Color, Color, Color) {
    match state {
        SlotState::Off | SlotState::Blocked => (
            mix(theme::LINEN, theme::LINEN_DK, 0.35),
            dim_ink(),
            mix(theme::INK, theme::LINEN, 0.5),
        ),
        SlotState::Ready if hot => (
            mix(theme::LINEN_DK, theme::LINEN, 0.5),
            theme::INK,
            theme::INK,
        ),
        SlotState::Ready => (theme::LINEN_DK, theme::INK, theme::INK),
        SlotState::Armed => (theme::WELD, theme::INK, theme::INK),
    }
}

/// Ink faded into the linen: a disabled outline.
pub(crate) fn dim_ink() -> Color {
    mix(theme::INK, theme::LINEN, 0.6)
}

/// Ink faded a little: secondary text on linen.
pub(crate) fn muted_ink() -> Color {
    mix(theme::INK, theme::LINEN, 0.3)
}

/// The lighter linen of panels set into the bar.
pub(crate) fn linen_light() -> Color {
    mix(theme::LINEN, [255, 250, 230, 255], 0.45)
}

pub(crate) fn rgb(c: [u8; 3]) -> Color {
    [c[0], c[1], c[2], 255]
}

/// A meter with an ink outline: `frac` of it filled in `color`.
pub(crate) fn ink_meter(draw: &mut DrawList, r: Rect, frac: f32, color: Color) {
    draw.rect(r, theme::INK);
    draw.meter(r.inset(2), frac, color, theme::LINEN_DK);
}

impl Hud {
    /// Whether a click at `mouse` belongs to the HUD rather than the world.
    pub fn wants(&self, screen: (i32, i32), mouse: (i32, i32)) -> bool {
        self.dragging
            || HudLayout::new(screen).bar.contains(mouse.0, mouse.1)
            || self.over.iter().any(|r| r.contains(mouse.0, mouse.1))
    }

    /// Where the tooltip was drawn this frame, if there is one.
    pub fn tooltip_rect(&self) -> Option<Rect> {
        self.tooltip
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
        let layout = HudLayout::new(screen);
        self.tip = None;
        draw.rect(layout.bar, theme::LINEN);
        draw.rect(Rect::new(0, layout.bar.y, layout.bar.w, 3), theme::INK);
        draw.stitch_row(0, layout.bar.y + 5, layout.bar.w, 2);
        self.minimap(draw, &layout, input, &data.minimap, events);
        self.tray(draw, &layout, input, data, events);
        if let Some(deploy) = data.deployment {
            deploy_panel(draw, &mut self.press, layout.grid, input, deploy, events);
        } else {
            self.grid(draw, &layout, input, data.grid, events);
        }
        self.over.clear();
        self.top_bar(draw, &layout, input, data, events);
        if let Some(h) = &data.hero {
            hero::draw(draw, layout.bar.y, h, &mut self.over);
        }
        powers::draw(
            draw,
            layout.field,
            input,
            &mut self.press,
            data.powers,
            &mut self.over,
            events,
        );
        alerts::draw(
            draw,
            layout.field,
            input,
            &mut self.press,
            data.alerts,
            &mut self.over,
            events,
        );
        let place_tip = (data.picker.as_ref()).and_then(|picker| {
            placer::draw(
                draw,
                layout.field,
                input,
                &mut self.press,
                picker,
                &mut self.over,
                events,
            )
        });
        if let Some(t) = place_tip {
            self.tip = Some(Tip {
                id: t.id,
                anchor: t.anchor,
                name: t.name,
                key: t.key,
                text: t.text,
                note: String::new(),
            });
        }
        if let Some(card) = &data.tips {
            conquest::draw_tips(
                draw,
                &mut self.press,
                layout.field,
                layout.tray,
                input,
                card,
                &mut self.over,
                events,
            );
        }
        if let Some(scroll) = &data.help {
            let top = if data.powers.is_empty() { 8 } else { 54 };
            if let Some(r) = draw_scroll(draw, layout.field, top, scroll) {
                self.over.push(r);
            }
        }
        self.finish_tooltip(draw, screen, input);
        self.press.end_frame(input.left_released);
    }

    /// Show the tooltip a button asked for once the mouse has rested on it
    /// for [`TOOLTIP_DELAY_MS`]; forget the rest when it moves away.
    fn finish_tooltip(&mut self, draw: &mut DrawList, screen: (i32, i32), input: HudInput) {
        self.tooltip = None;
        let Some(tip) = self.tip.take() else {
            self.hover = None;
            return;
        };
        let since = match self.hover {
            Some((id, since)) if id == tip.id => since,
            _ => {
                self.hover = Some((tip.id, input.now_ms));
                input.now_ms
            }
        };
        if input.now_ms.wrapping_sub(since) < TOOLTIP_DELAY_MS {
            return;
        }
        const WRAP: usize = 30;
        const PAD: i32 = 9;
        let body: Vec<String> = if tip.text.is_empty() {
            Vec::new()
        } else {
            wrap(&tip.text, WRAP)
        };
        let head = format!("{}  [{}]", tip.name, tip.key);
        let chars = body
            .iter()
            .map(|l| l.chars().count())
            .chain([head.chars().count(), tip.note.chars().count()])
            .max()
            .unwrap_or(1);
        let lines = 1 + body.len() + usize::from(!tip.note.is_empty());
        let w = chars as i32 * CELL_W + 2 * PAD;
        let h = lines as i32 * 18 + 2 * PAD - 2;
        // Above the button; below it when there is no room above; always
        // wholly on the screen.
        let mut y = tip.anchor.y - h - 6;
        if y < 4 {
            y = tip.anchor.bottom() + 6;
        }
        let x = (tip.anchor.right() - w).clamp(4, (screen.0 - w - 4).max(4));
        let y = y.clamp(4, (screen.1 - h - 4).max(4));
        let r = Rect::new(x, y, w, h);
        draw.shadow(r, 3);
        draw.boxed(r, linen_light(), 2);
        draw.border(r.inset(3), 1, theme::WELD);
        let mut ty = r.y + PAD - 1;
        draw.text_bold(r.x + PAD, ty, 1, theme::MADDER, &tip.name);
        let key = format!("[{}]", tip.key);
        draw.text(
            r.right() - PAD - text_width(&key, 1),
            ty,
            1,
            theme::WOAD,
            &key,
        );
        for line in &body {
            ty += 18;
            draw.text(r.x + PAD, ty, 1, theme::INK, line);
        }
        if !tip.note.is_empty() {
            ty += 18;
            draw.text(r.x + PAD, ty, 1, muted_ink(), &tip.note);
        }
        self.tooltip = Some(r);
    }

    /// The top bar: the economy (Conquest), then the status chips and the
    /// Help button from the right.
    fn top_bar(
        &mut self,
        draw: &mut DrawList,
        layout: &HudLayout,
        input: HudInput,
        data: &HudData,
        events: &mut Vec<HudEvent>,
    ) {
        let bar = layout.top;
        self.over.push(bar);
        draw.rect(bar, theme::LINEN);
        draw.rect(Rect::new(0, bar.bottom() - 3, bar.w, 3), theme::INK);
        draw.stitch_row(0, bar.bottom() - 8, bar.w, 2);
        let band = Rect::new(0, 0, bar.w, bar.h - 8);
        let chip_h = 24;
        let chip_y = band.y + (band.h - chip_h) / 2;
        let text_y = chip_y + (chip_h - 16) / 2;
        let mut right = bar.right() - MARGIN;
        let gap = 6;

        // Help: always the right-most.
        let help = Rect::new(right - 76, chip_y, 76, chip_h);
        right = help.x - gap;
        let hot = help.contains(input.mouse.0, input.mouse.1);
        let (clicked, down) = self.press.widget(
            WIDGET_HELP,
            help,
            input.mouse,
            input.left_pressed,
            input.left_released,
        );
        if clicked {
            events.push(HudEvent::ToggleHelp);
        }
        let open = data.help.is_some();
        let fill = match (down, hot || open) {
            (true, _) => mix(theme::WELD, theme::INK, 0.2),
            (_, true) => mix(theme::WELD, [255, 240, 190, 255], 0.4),
            _ => theme::WELD,
        };
        draw.boxed(help, fill, 2);
        icons::icon(
            draw,
            Rect::new(help.x + 7, help.y + 6, 12, 12),
            Icon::Help,
            theme::INK,
        );
        draw.text_bold(help.x + 24, text_y, 1, theme::INK, "Help");
        // Open, the scroll itself is the explanation (and sits just below).
        if hot && !open {
            self.tip = Some(Tip {
                id: TIP_HELP,
                anchor: help,
                name: "Help".into(),
                key: "H or F1".into(),
                text: "Open or close the Marshal's Scroll, the list of keys.".into(),
                note: String::new(),
            });
        }

        // Guide: just left of Help.
        let guide = Rect::new(right - 84, chip_y, 84, chip_h);
        right = guide.x - gap;
        let hot = guide.contains(input.mouse.0, input.mouse.1);
        let (clicked, down) = self.press.widget(
            WIDGET_GUIDE,
            guide,
            input.mouse,
            input.left_pressed,
            input.left_released,
        );
        if clicked {
            events.push(HudEvent::OpenGuide);
        }
        let fill = match (down, hot) {
            (true, _) => mix(theme::WELD, theme::INK, 0.2),
            (_, true) => mix(theme::WELD, [255, 240, 190, 255], 0.4),
            _ => theme::WELD,
        };
        draw.boxed(guide, fill, 2);
        icons::icon(
            draw,
            Rect::new(guide.x + 7, guide.y + 6, 12, 12),
            Icon::Book,
            theme::INK,
        );
        draw.text_bold(guide.x + 24, text_y, 1, theme::INK, "Guide");
        if hot {
            self.tip = Some(Tip {
                id: TIP_GUIDE,
                anchor: guide,
                name: "Guide".into(),
                key: "U".into(),
                text: "Every building and troop: its role, stats and upgrades.".into(),
                note: String::new(),
            });
        }

        // Place (Sandbox): just left of Guide.
        if let Some(picker) = &data.picker {
            let place = Rect::new(right - 80, chip_y, 80, chip_h);
            right = place.x - gap;
            let hot = place.contains(input.mouse.0, input.mouse.1);
            let (clicked, down) = self.press.widget(
                WIDGET_PLACE,
                place,
                input.mouse,
                input.left_pressed,
                input.left_released,
            );
            if clicked {
                events.push(HudEvent::TogglePlacer);
            }
            let lit = picker.open || picker.placing;
            let fill = match (down, hot || lit) {
                (true, _) => mix(theme::WELD, theme::INK, 0.2),
                (_, true) => mix(theme::WELD, [255, 240, 190, 255], 0.4),
                _ => theme::WELD,
            };
            draw.boxed(place, fill, 2);
            icons::icon(
                draw,
                Rect::new(place.x + 7, place.y + 6, 12, 12),
                Icon::Flag,
                theme::INK,
            );
            draw.text_bold(place.x + 24, text_y, 1, theme::INK, "Place");
            if hot && !picker.open {
                self.tip = Some(Tip {
                    id: TIP_PLACE,
                    anchor: place,
                    name: "Place".into(),
                    key: "B".into(),
                    text: "Choose a troop type and a side, then click the field to put down regiments."
                        .into(),
                    note: String::new(),
                });
            }
        }

        // Hand over (with allies): just left of Place.
        if let Some(h) = data.handover {
            let w = 34 + text_width(h.label, 1);
            let rect = Rect::new(right - w, chip_y, w, chip_h);
            right = rect.x - gap;
            let hot = rect.contains(input.mouse.0, input.mouse.1);
            let (clicked, down) = self.press.widget(
                WIDGET_HANDOVER,
                rect,
                input.mouse,
                input.left_pressed,
                input.left_released,
            );
            if clicked {
                events.push(HudEvent::HandOver);
            }
            let fill = match (down, hot || h.lit) {
                (true, _) => mix(theme::WELD, theme::INK, 0.2),
                (_, true) => mix(theme::WELD, [255, 240, 190, 255], 0.4),
                _ => theme::WELD,
            };
            draw.boxed(rect, fill, 2);
            icons::icon(
                draw,
                Rect::new(rect.x + 7, rect.y + 6, 12, 12),
                Icon::Flag,
                theme::INK,
            );
            draw.text_bold(rect.x + 24, text_y, 1, theme::INK, h.label);
            if hot {
                self.tip = Some(Tip {
                    id: TIP_HANDOVER,
                    anchor: rect,
                    name: "Hand over".into(),
                    key: String::new(),
                    text: h.tip.to_string(),
                    note: String::new(),
                });
            }
        }

        // Status chips, from the right.
        let st = data.status;
        let chip = |draw: &mut DrawList, right: &mut i32, icon: Icon, text: &str, dye: Color| {
            let w = 12 + 8 * 2 + 4 + text_width(text, 1) + if icon == Icon::None { -16 } else { 0 };
            let r = Rect::new(*right - w, chip_y, w, chip_h);
            draw.boxed(r, theme::LINEN_DK, 2);
            let mut x = r.x + 8;
            if icon != Icon::None {
                icons::icon(draw, Rect::new(x, r.y + 6, 12, 12), icon, dye);
                x += 16;
            }
            draw.text_bold(x, text_y, 1, theme::INK, text);
            *right = r.x - gap;
        };
        if let Some(secs) = st.clock_secs {
            chip(
                draw,
                &mut right,
                Icon::None,
                &conquest::clock(secs),
                theme::INK,
            );
        }
        if !st.speed.is_empty() || st.paused {
            let (icon, text) = if st.paused {
                (Icon::Pause, "paused")
            } else {
                (Icon::Play, st.speed)
            };
            chip(
                draw,
                &mut right,
                icon,
                text,
                if st.paused {
                    theme::MADDER
                } else {
                    theme::WOAD
                },
            );
        }
        if !st.mode.is_empty() {
            let text = if st.side.is_empty() {
                st.mode.to_string()
            } else {
                format!("{}: {}", st.mode, st.side)
            };
            let w = 8 + 14 + 6 + text_width(&text, 1) + 8;
            let r = Rect::new(right - w, chip_y, w, chip_h);
            draw.boxed(r, theme::LINEN_DK, 2);
            let swatch = Rect::new(r.x + 8, r.y + 5, 14, 14);
            draw.boxed(swatch, rgb(st.side_color), 2);
            draw.text_bold(swatch.right() + 6, text_y, 1, theme::INK, &text);
            right = r.x - gap;
        }

        if let Some(t) = &data.top {
            let area = Rect::new(MARGIN, band.y, (right - MARGIN).max(0), band.h);
            conquest::draw_resources(draw, area, t);
        }
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
            let over = cell.contains(input.mouse.0, input.mouse.1);
            let hot = slot.state == SlotState::Ready && over;
            let (clicked, down) = self.press.widget(
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
            if over && !slot.label.is_empty() {
                self.tip = Some(Tip {
                    id: i as u32,
                    anchor: cell,
                    name: slot.label.to_string(),
                    key: GRID_KEYS[i].to_string(),
                    text: slot.tip.to_string(),
                    note: match slot.state {
                        SlotState::Off => "Not available now".to_string(),
                        SlotState::Blocked | SlotState::Ready | SlotState::Armed => {
                            slot.detail.to_string()
                        }
                    },
                });
            }
            let (mut back, edge, text) = slot_colors(slot.state, hot);
            if down && on {
                back = mix(back, theme::INK, 0.18);
            }
            draw.boxed(cell, back, 2);
            let inner = cell.inset(3);
            let chars = (inner.w / 8).max(1) as usize;
            let mut key = [0u8; 4];
            let key_color = if on { theme::MADDER } else { edge };
            draw.text(
                inner.x,
                inner.y - 2,
                1,
                key_color,
                GRID_KEYS[i].encode_utf8(&mut key),
            );
            if slot.detail.is_empty() {
                // Icon over name.
                let room = Rect::new(inner.x, inner.y, inner.w, inner.h - 18);
                let mid = Rect::new(room.x + (room.w - 24) / 2, room.y + 1, 24, room.h.min(24));
                icons::icon(draw, mid, slot.icon, text);
                let label = fit(slot.label, chars);
                let ly = inner.bottom() - 16;
                let lx = inner.x + (inner.w - text_width(&label, 1)) / 2;
                draw.text(lx, ly, 1, text, &label);
            } else {
                // A second line: a small icon in the corner.
                icons::icon(
                    draw,
                    Rect::new(inner.right() - 12, inner.y, 12, 12),
                    slot.icon,
                    text,
                );
                let label = fit(slot.label, chars);
                let lx = inner.x + (inner.w - text_width(&label, 1)) / 2;
                draw.text(lx, inner.y + 12, 1, text, &label);
                let detail = fit(slot.detail, chars);
                let dx = inner.x + (inner.w - text_width(&detail, 1)) / 2;
                draw.text(dx, inner.y + 26, 1, mix(text, theme::LINEN, 0.3), &detail);
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

        // A weld-yellow frame round the map, like a framed panel of cloth.
        draw.rect(r.inset(-5), theme::WELD);
        draw.border(r.inset(-5), 2, theme::INK);
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
            line(draw, r, to_screen(a), to_screen(b), [255, 250, 230, 255]);
        }
        draw.border(r, 2, theme::INK);
    }

    /// The army shape chips along `strip`: one button each, the chosen one lit.
    fn shape_chips(
        &mut self,
        draw: &mut DrawList,
        strip: Rect,
        input: HudInput,
        chips: &[ShapeChip],
        events: &mut Vec<HudEvent>,
    ) {
        let (label_x, rects) = shape_chip_rects(strip, chips);
        if let Some(x) = label_x {
            draw.text_bold(
                x,
                strip.y + (strip.h - text_height(1)) / 2,
                1,
                theme::MADDER,
                SHAPE_LABEL,
            );
        }
        for (i, (chip, &rect)) in chips.iter().zip(&rects).enumerate() {
            let over = rect.contains(input.mouse.0, input.mouse.1);
            let (clicked, down) = self.press.widget(
                WIDGET_SHAPE + i as u32,
                rect,
                input.mouse,
                input.left_pressed,
                input.left_released,
            );
            if clicked {
                events.push(HudEvent::ArmyShape(i));
            }
            if over {
                self.tip = Some(Tip {
                    id: TIP_SHAPE + i as u32,
                    anchor: rect,
                    name: format!("Army shape: {}", chip.label),
                    key: "O".to_string(),
                    text: chip.tip.to_string(),
                    note: String::new(),
                });
            }
            let state = if chip.on {
                SlotState::Armed
            } else {
                SlotState::Ready
            };
            let (mut back, _, text) = slot_colors(state, over);
            if down {
                back = mix(back, theme::INK, 0.18);
            }
            draw.boxed(rect, back, 2);
            if chip.on {
                let x = rect.x + (rect.w - text_width(chip.label, 1)) / 2;
                draw.text_bold(
                    x,
                    rect.y + (rect.h - text_height(1)) / 2,
                    1,
                    text,
                    chip.label,
                );
            } else {
                draw.text_centered(rect, 1, text, chip.label);
            }
        }
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
        let mut inner = r.inset(8);
        if inner.w < CARD_MIN_W {
            return;
        }
        draw.boxed(r, linen_light(), 2);
        if data.structure.is_none()
            && let Some(sel) = data
                .selection
                .filter(|s| s.regiments > 1 && !s.shapes.is_empty())
        {
            let strip = Rect::new(inner.x, inner.y, inner.w, SHAPE_CHIP_H);
            self.shape_chips(draw, strip, input, sel.shapes, events);
            let used = SHAPE_CHIP_H + 4;
            inner = Rect::new(inner.x, inner.y + used, inner.w, inner.h - used);
        }
        if let Some(s) = &data.structure {
            draw.text_bold(inner.x, inner.y - 2, 1, theme::MADDER, data.tray_title);
            let row = Rect::new(inner.x, inner.y + 20, inner.w, inner.h - 20);
            conquest::draw_structure(draw, &mut self.press, row, input, s, events);
            return;
        }
        let mut cards_x = inner.x;
        let show_cards =
            !data.cards.is_empty() && (data.selection.is_none() || data.cards.len() > 1);
        if let Some(sel) = &data.selection {
            let want = if show_cards { 310 } else { 460 };
            let w = want.min(if show_cards {
                inner.w * 45 / 100
            } else {
                inner.w
            });
            draw_selection(draw, Rect::new(inner.x, inner.y, w, inner.h), sel);
            cards_x = inner.x + w + 10;
        } else if data.cards.is_empty() {
            draw.text_centered(r, 1, muted_ink(), "Click a regiment to command it");
            return;
        }
        if !show_cards || inner.right() - cards_x < CARD_MIN_W {
            return;
        }
        draw.text_bold(cards_x, inner.y - 2, 1, theme::MADDER, data.tray_title);
        let row = Rect::new(cards_x, inner.y + 20, inner.right() - cards_x, inner.h - 20);
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
            draw.text_bold(x.max(cards_x), inner.y - 2, 1, theme::WOAD, &more);
        }
    }
}

/// Height of the army shape chips' row.
const SHAPE_CHIP_H: i32 = 20;
const SHAPE_LABEL: &str = "Army shape";

/// Where the army shape chips sit along `strip`, and where the "Army shape"
/// label goes, if there is room for it. Tries the roomy way first, then
/// drops the label, then squeezes the chips; chips that still do not fit
/// are left off the end.
fn shape_chip_rects(strip: Rect, chips: &[ShapeChip]) -> (Option<i32>, Vec<Rect>) {
    const GAP: i32 = 4;
    let mut out = (None, Vec::new());
    for (label, pad) in [(true, 6), (false, 6), (false, 3)] {
        let mut x = strip.x;
        let label_x = label.then_some(x);
        if label {
            x += text_width(SHAPE_LABEL, 1) + 10;
        }
        let mut rects = Vec::with_capacity(chips.len());
        for chip in chips {
            let w = text_width(chip.label, 1) + 2 * pad;
            if x + w > strip.right() {
                break;
            }
            rects.push(Rect::new(x, strip.y, w, strip.h));
            x += w + GAP;
        }
        let all = rects.len() == chips.len();
        if all || pad == 3 {
            out = (label_x, rects);
        }
        if all {
            break;
        }
    }
    out
}

/// A pennant: `fill` cloth with a swallowtail point, outlined in ink.
fn banner(draw: &mut DrawList, r: Rect, fill: Color) {
    let shape = |draw: &mut DrawList, r: Rect, color: Color| {
        let tail = (r.w * 2 / 3).min(r.h / 2);
        draw.rect(Rect::new(r.x, r.y, r.w, r.h - tail), color);
        for t in 0..tail {
            let half = r.w * (tail - t) / (2 * tail);
            let cx = r.x + r.w / 2;
            draw.rect(
                Rect::new(cx - half, r.y + r.h - tail + t, half * 2, 1),
                color,
            );
        }
    };
    shape(draw, r, theme::INK);
    shape(draw, r.inset(3), fill);
}

/// The selection panel: a banner in the side's colour with the unit's
/// picture, its name and numbers, and bars for health and morale.
fn draw_selection(draw: &mut DrawList, r: Rect, s: &Selection) {
    let banner_rect = Rect::new(r.x, r.y, 50, r.h.min(124));
    banner(draw, banner_rect, rgb(s.color));
    let ink = if luma(s.color) > 140 {
        theme::INK
    } else {
        theme::LINEN
    };
    icons::icon(
        draw,
        Rect::new(banner_rect.x + 7, banner_rect.y + 12, 36, 28),
        s.icon,
        ink,
    );
    let x = banner_rect.right() + 12;
    let w = (r.right() - x).max(0);
    if w < 80 {
        return;
    }
    let chars = (w / CELL_W) as usize;
    let mut y = r.y;
    // The name big when it fits, otherwise at normal size.
    if (s.name.chars().count() as i32) * CELL_W * 2 <= w {
        draw.text_bold(x, y, 2, theme::INK, s.name);
        y += 34;
    } else {
        draw.text_bold(x, y + 8, 1, theme::INK, &fit(s.name, chars));
        y += 34;
    }
    let count = if s.regiments > 1 {
        format!("{} regiments, {} soldiers", s.regiments, s.models)
    } else {
        format!("{} of {} soldiers", s.models, s.max_models)
    };
    draw.text(x, y - 4, 1, muted_ink(), &fit(&count, chars));
    y += 20;

    let label_w = 9 * CELL_W;
    let pct_w = 5 * CELL_W;
    let bar_x = x + label_w;
    let bar_w = (w - label_w - pct_w).max(20);
    let health = s.models as f32 / s.max_models.max(1) as f32;
    let (morale_label, morale_color) = match s.state {
        CardState::Steady => ("Morale", theme::WOAD),
        CardState::Wavering => ("Wavering", theme::WELD),
        CardState::Routing => ("Routing", theme::MADDER),
    };
    for (label, frac, color) in [
        ("Health", health, health_color(health)),
        (morale_label, f32::from(s.morale) / 100.0, morale_color),
    ] {
        draw.text_bold(x, y - 3, 1, theme::INK, label);
        ink_meter(draw, Rect::new(bar_x, y, bar_w, 12), frac, color);
        let pct = format!("{}%", (frac * 100.0).round() as i32);
        draw.text(bar_x + bar_w + 8, y - 3, 1, muted_ink(), &pct);
        y += 22;
    }
    if !s.formation.is_empty() {
        draw.text(
            x,
            y,
            1,
            muted_ink(),
            &fit(&format!("Formation: {}", s.formation), chars),
        );
    }
}

/// The Marshal's Scroll: the keys in groups, then live notes. Drawn at the
/// right of `field`, `top` pixels down. Returns its rectangle.
fn draw_scroll(draw: &mut DrawList, field: Rect, top: i32, scroll: &HelpScroll) -> Option<Rect> {
    const PAD: i32 = 18;
    const ROW: i32 = 18;
    const GAP: i32 = 22;
    let widths: Vec<(i32, i32)> = HELP_GROUPS
        .iter()
        .map(|(_, keys)| {
            let k = keys
                .iter()
                .map(|(k, _)| k.chars().count())
                .max()
                .unwrap_or(1);
            let d = keys
                .iter()
                .map(|(_, d)| d.chars().count())
                .max()
                .unwrap_or(1);
            ((k as i32 + 1) * CELL_W, d as i32 * CELL_W)
        })
        .collect();
    let col_w: Vec<i32> = widths.iter().map(|(k, d)| k + d).collect();
    let avail = field.w - 16;
    // As many columns as fit: three, two or one.
    let columns = [3usize, 2, 1]
        .into_iter()
        .find(|&c| {
            let rows_w = (0..c)
                .map(|i| {
                    (i..HELP_GROUPS.len())
                        .step_by(c)
                        .map(|g| col_w[g])
                        .max()
                        .unwrap_or(0)
                })
                .sum::<i32>();
            rows_w + (c as i32 - 1) * GAP + 2 * PAD <= avail
        })
        .unwrap_or(1);
    let column_w: Vec<i32> = (0..columns)
        .map(|i| {
            (i..HELP_GROUPS.len())
                .step_by(columns)
                .map(|g| col_w[g])
                .max()
                .unwrap_or(0)
        })
        .collect();
    let inner_w = column_w.iter().sum::<i32>() + (columns as i32 - 1) * GAP;
    let w = (inner_w + 2 * PAD).min(avail);
    if w < 120 {
        return None;
    }
    // Leave the idle-worker button's corner clear.
    let room = field.h - top - 8 - 44;
    let r = Rect::new(field.right() - 8 - w, field.y + top, w, room.max(0));
    // The tallest column decides the height: its groups stacked.
    let heights: Vec<i32> = (0..columns)
        .map(|i| {
            (i..HELP_GROUPS.len())
                .step_by(columns)
                .map(|g| 24 + HELP_GROUPS[g].1.len() as i32 * ROW)
                .sum()
        })
        .collect();
    let keys_h = heights.iter().copied().max().unwrap_or(0);
    let chars = ((inner_w) / CELL_W).max(1) as usize;
    // The keys come first; the notes get what room is left.
    let spare = room - (2 * PAD + 26 + keys_h + 4) - 24;
    let fits = (spare / 16).clamp(0, 8) as usize;
    let notes: Vec<String> = scroll
        .notes
        .iter()
        .flat_map(|n| wrap(n, chars))
        .take(fits)
        .collect();
    let notes_h = if notes.is_empty() {
        0
    } else {
        24 + notes.len() as i32 * 16
    };
    let h = (2 * PAD + 26 + keys_h + notes_h + 4).min(room);
    if h < 80 {
        return None;
    }
    let r = Rect::new(r.x, r.y, w, h);
    draw.shadow(r, 4);
    let content = draw.cloth(r, theme::LINEN);
    let _ = content;
    let x0 = r.x + PAD;
    let mut y = r.y + PAD - 4;
    draw.text_bold(x0, y, 1, theme::MADDER, "THE MARSHAL'S SCROLL");
    let hint = "H or F1 closes";
    draw.text(
        r.right() - PAD - text_width(hint, 1),
        y,
        1,
        muted_ink(),
        hint,
    );
    y += 26;
    let limit = r.bottom() - PAD - notes_h;
    let mut col_x = x0;
    for (c, cw) in column_w.iter().enumerate() {
        let mut cy = y;
        for g in (c..HELP_GROUPS.len()).step_by(columns) {
            let (name, keys) = HELP_GROUPS[g];
            draw.text_bold(col_x, cy, 1, theme::WOAD, name);
            draw.rect(Rect::new(col_x, cy + 17, *cw, 1), theme::INK);
            cy += 24;
            for (key, what) in keys {
                if cy + 16 > limit {
                    break;
                }
                draw.text_bold(col_x, cy, 1, theme::MADDER, key);
                draw.text(col_x + widths[g].0, cy, 1, theme::INK, what);
                cy += ROW;
            }
        }
        col_x += cw + GAP;
    }
    if !notes.is_empty() {
        let mut ny = r.bottom() - PAD - notes_h + 4;
        draw.text_bold(x0, ny, 1, theme::WOAD, "Now");
        draw.rect(Rect::new(x0, ny + 17, inner_w, 1), theme::INK);
        ny += 24;
        for line in &notes {
            draw.text(x0, ny, 1, theme::INK, line);
            ny += 16;
        }
    }
    Some(r)
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
    let inner = r.inset(8);
    // The Ready button shows on any screen; the words only where they fit.
    if inner.w < CELL_W * 6 {
        return;
    }
    draw.boxed(r, linen_light(), 2);
    draw.border(r.inset(3), 1, theme::WELD);
    let left = format!(
        "{}:{:02} left",
        deploy.seconds_left / 60,
        deploy.seconds_left % 60
    );
    let chars = (inner.w / CELL_W).max(1) as usize;
    draw.text_bold(
        inner.x,
        inner.y,
        1,
        theme::MADDER,
        &fit("DEPLOYMENT", chars),
    );
    draw.text(inner.x, inner.y + 20, 1, theme::INK, &fit(&left, chars));
    let hint = if deploy.ready {
        "Waiting for the others."
    } else {
        "Right-drag to place."
    };
    draw.text(inner.x, inner.y + 40, 1, muted_ink(), &fit(hint, chars));
    let button = Rect::new(inner.x, inner.bottom() - 44, inner.w, 44);
    if deploy.ready {
        draw.boxed(button, theme::LINEN_DK, 2);
        draw.text_centered(button, 1, theme::WODE, "Ready");
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
    draw.boxed(
        button,
        if down {
            mix(theme::WELD, theme::INK, 0.2)
        } else if hot {
            mix(theme::WELD, [255, 240, 190, 255], 0.4)
        } else {
            theme::WELD
        },
        2,
    );
    let scale = if inner.w >= CELL_W * 2 * 6 { 2 } else { 1 };
    draw.text_centered(button, scale, theme::INK, "Ready");
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
        mix(theme::LINEN_DK, theme::LINEN, 0.5)
    } else {
        theme::LINEN_DK
    };
    draw.boxed(r, back, 2);
    if c.selected {
        draw.border(r.inset(2), 2, theme::WELD);
    }
    let inner = r.inset(5);

    // The portrait: a team-coloured square with the unit's initial, until
    // there is art for it.
    let portrait = Rect::new(inner.x, inner.y, inner.w, inner.w.min(inner.h / 2));
    draw_portrait(draw, portrait, c.color, &c.name, 2);
    if c.engaged {
        draw.border(portrait, 2, theme::MADDER);
    }
    let chars = (inner.w / 8).max(1) as usize;
    let mut y = portrait.bottom() + 3;
    draw.text(inner.x, y, 1, theme::INK, &fit(&c.name, chars));
    y += 18;

    // Model count, then morale.
    let health = f32::from(c.models) / f32::from(c.max_models.max(1));
    ink_meter(
        draw,
        Rect::new(inner.x, y, inner.w, 8),
        health,
        health_color(health),
    );
    y += 10;
    let morale_color = match c.state {
        CardState::Steady => theme::WOAD,
        CardState::Wavering => theme::WELD,
        CardState::Routing => theme::MADDER,
    };
    ink_meter(
        draw,
        Rect::new(inner.x, y, inner.w, 8),
        f32::from(c.morale) / 100.0,
        morale_color,
    );
    y += 10;
    if c.state == CardState::Routing {
        let label = Rect::new(portrait.x, portrait.bottom() - 18, portrait.w, 16);
        draw.rect(label, theme::INK);
        draw.text_centered(label, 1, theme::LINEN, "ROUT");
    }
    let count = c.models.to_string();
    draw.text(
        inner.right() - text_width(&count, 1),
        y - 2,
        1,
        muted_ink(),
        &count,
    );
}

pub(crate) fn luma(c: [u8; 3]) -> u32 {
    (u32::from(c[0]) * 3 + u32::from(c[1]) * 6 + u32::from(c[2])) / 10
}

/// A team-coloured square `r` with the first letter of `name` on it, until
/// there is art, in an ink frame. The letter is dark on a light colour.
pub(crate) fn draw_portrait(draw: &mut DrawList, r: Rect, color: [u8; 3], name: &str, scale: i32) {
    draw.rect(r, rgb(color));
    if let Some(initial) = name.chars().next() {
        let ink = if luma(color) > 140 {
            theme::INK
        } else {
            theme::LINEN
        };
        draw.text_centered(r, scale, ink, &initial.to_string());
    }
    draw.border(r, 2, theme::INK);
}

/// Wode green for a healthy meter, madder red once `frac` drops under about
/// a third.
pub(crate) fn health_color(frac: f32) -> Color {
    if frac < 0.35 {
        theme::MADDER
    } else {
        theme::WODE
    }
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
    use crate::draw::SOLID;

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
        powers: &'a [PowerButton<'a>],
        picker: Option<PlacePicker<'a>>,
        tips: Option<TipsCard<'a>>,
        handover: Option<HandOverButton<'a>>,
        selection: Option<Selection<'a>>,
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
            ..GridSlot::default()
        });
        let data = HudData {
            grid: &grid,
            status: StatusStrip::default(),
            selection: extras.selection,
            help: None,
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
            powers: extras.powers,
            picker: extras.picker,
            tips: extras.tips,
            handover: extras.handover,
        };
        let mut draw = DrawList::default();
        let mut events = Vec::new();
        hud.frame(&mut draw, SCREEN, input, &data, &mut events);
        (events, draw)
    }

    #[test]
    fn the_hand_over_button_shows_only_with_allies_and_sends_its_event() {
        // It sits where Place would, left of the Guide button.
        let click = HudInput {
            mouse: (SCREEN.0 - 260 + 10, 12),
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        };
        let with_ally = Extras {
            handover: Some(HandOverButton {
                label: "Hand over",
                tip: "Give your army to an ally.",
                lit: false,
            }),
            ..Extras::default()
        };
        let (events, _) = run_with(&mut Hud::default(), click, &[], &[], with_ally);
        assert_eq!(events, vec![HudEvent::HandOver]);
        let (events, _) = run_with(&mut Hud::default(), click, &[], &[], Extras::default());
        assert!(events.is_empty());
    }

    #[test]
    fn the_place_button_shows_only_in_the_sandbox_and_opens_the_panel() {
        let place = (SCREEN.0 - 260 + 10, 12);
        let click = HudInput {
            mouse: place,
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        };
        let sandbox = Extras {
            picker: Some(PlacePicker::default()),
            ..Extras::default()
        };
        let (events, _) = run_with(&mut Hud::default(), click, &[], &[], sandbox);
        assert_eq!(events, vec![HudEvent::TogglePlacer]);
        let (events, _) = run(&mut Hud::default(), click, &[], &[]);
        assert!(events.is_empty(), "no picker, no button");
    }

    #[test]
    fn an_open_place_panel_keeps_the_mouse_and_a_row_click_reaches_the_game() {
        use crate::placer::{PlaceRow, panel_rect};
        let rows = [PlaceRow {
            name: "Yari Ashigaru",
            ..PlaceRow::default()
        }; 2];
        let picker = PlacePicker {
            open: true,
            rows: &rows,
            ..PlacePicker::default()
        };
        let field = HudLayout::new(SCREEN).field;
        let panel = panel_rect(field, &picker).unwrap();
        let mut hud = Hud::default();
        let extras = || Extras {
            picker: Some(picker),
            ..Extras::default()
        };
        let at = |x, y| HudInput {
            mouse: (x, y),
            ..HudInput::default()
        };
        run_with(&mut hud, at(panel.x + 20, panel.y + 20), &[], &[], extras());
        assert!(hud.wants(SCREEN, (panel.x + 20, panel.y + 20)));
        let row = (panel.x + 60, panel.y + 14 + 26 + 34 + 20);
        let click = HudInput {
            mouse: row,
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        };
        let (events, _) = run_with(&mut Hud::default(), click, &[], &[], extras());
        assert_eq!(events, vec![HudEvent::PlaceUnit(0)]);
    }

    #[test]
    fn the_ready_button_ends_your_deployment() {
        let l = HudLayout::new(SCREEN);
        let button = (l.grid.x + l.grid.w / 2, l.grid.bottom() - 20);
        let t = terrain();
        let grid: [GridSlot; GRID_SLOTS] = std::array::from_fn(|_| GridSlot::default());
        let frame = |ready, input| {
            let data = HudData {
                grid: &grid,
                status: StatusStrip::default(),
                selection: None,
                help: None,
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
                powers: &[],
                picker: None,
                tips: None,
                handover: None,
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
        let l = HudLayout::new(SCREEN);
        let ready = (l.grid.x + l.grid.w / 2, l.grid.bottom() - 20);
        let t = terrain();
        let grid: [GridSlot; GRID_SLOTS] = std::array::from_fn(|_| GridSlot::default());
        let mut hud = Hud::default();
        let mut frame = |input| {
            let data = HudData {
                grid: &grid,
                status: StatusStrip::default(),
                selection: None,
                help: None,
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
                powers: &[],
                picker: None,
                tips: None,
                handover: None,
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
        let l = HudLayout::new(SCREEN);
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
            status: StatusStrip::default(),
            selection: None,
            help: None,
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
            powers: &[],
            picker: None,
            tips: None,
            handover: None,
        };
        let l = HudLayout::new(SCREEN);
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
            let l = HudLayout::new(screen);
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
        let l = HudLayout::new(SCREEN);
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
        let l = HudLayout::new(SCREEN);
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
        let field = (0, TOP_BAR_HEIGHT + 20);
        assert!(hud.wants(SCREEN, field));
        run(
            &mut hud,
            HudInput {
                left_released: true,
                ..at(field)
            },
            &[],
            &[],
        );
        assert!(!hud.wants(SCREEN, field));
    }

    #[test]
    fn right_click_on_the_minimap_orders() {
        let mut hud = Hud::default();
        let l = HudLayout::new(SCREEN);
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
        let l = HudLayout::new(SCREEN);
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
        let l = HudLayout::new(SCREEN);
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
            resonance: 0,
            chords: 0,
            honor: None,
            clock_secs: 14 * 60 + 32,
        }
    }

    /// How many glyphs of `colour` sit inside the top bar.
    fn in_top_bar(draw: &DrawList, colour: Color) -> usize {
        draw.quads
            .iter()
            .filter(|q| {
                i32::from(q.pos[1]) < TOP_BAR_HEIGHT && q.color == colour && q.glyph != SOLID
            })
            .count()
    }

    /// Values are drawn bold, which is the glyph twice.
    fn bold(text: &str) -> usize {
        2 * text.len()
    }

    /// The top bar's ink text: labels in muted ink, values in ink.
    fn text_quads(d: &DrawList) -> usize {
        in_top_bar(d, theme::INK) + in_top_bar(d, muted_ink())
    }

    fn with_top(top: TopBar) -> Extras<'static> {
        Extras {
            top: Some(top),
            ..Extras::default()
        }
    }

    #[test]
    fn the_top_bar_is_always_there_but_the_economy_only_when_given() {
        let mouse = (400, 5);
        let mut hud = Hud::default();
        let (_, plain) = run(&mut hud, HudInput::default(), &[], &[]);
        assert!(
            plain
                .quads
                .iter()
                .any(|q| q.pos == [0, 0] && q.size == [SCREEN.0 as u16, TOP_BAR_HEIGHT as u16])
        );
        assert!(hud.wants(SCREEN, mouse));
        assert!(hud.wants(SCREEN, (0, TOP_BAR_HEIGHT - 1)));
        assert!(!hud.wants(SCREEN, (400, TOP_BAR_HEIGHT + 4)), "the field");

        let (_, rich) = run_with(&mut hud, HudInput::default(), &[], &[], with_top(top_bar()));
        assert!(
            text_quads(&rich) > text_quads(&plain),
            "Grub, Marrow, Supply"
        );
        assert!(hud.wants(SCREEN, mouse));
    }

    #[test]
    fn supply_at_the_cap_turns_red() {
        let mut hud = Hud::default();
        let mut bar = |top| run_with(&mut hud, HudInput::default(), &[], &[], with_top(top)).1;
        let roomy = bar(top_bar());
        assert_eq!(in_top_bar(&roomy, theme::MADDER), 0);
        let full = bar(TopBar {
            supply_used: 90,
            ..top_bar()
        });
        assert_eq!(in_top_bar(&full, theme::MADDER), bold("90/90"));
        let taxed = bar(TopBar {
            upkeep_percent: 80,
            ..top_bar()
        });
        assert_eq!(in_top_bar(&taxed, theme::MADDER), bold("80%"));
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
        assert_eq!(
            text_quads(&brood),
            text_quads(&plain) + "Larvae".len() + bold("2/6")
        );
        // None ready: the count turns red.
        let empty = bar(TopBar {
            larvae: 0,
            larva_room: 3,
            ..top_bar()
        });
        assert_eq!(in_top_bar(&empty, theme::MADDER), bold("0/3"));
        assert_eq!(in_top_bar(&plain, theme::MADDER), 0);
    }

    #[test]
    fn resonance_shows_in_the_top_bar_only_with_a_chord_or_a_charge() {
        let mut hud = Hud::default();
        let mut bar = |top| run_with(&mut hud, HudInput::default(), &[], &[], with_top(top)).1;
        let plain = bar(top_bar());
        let choir = bar(TopBar {
            resonance: 37,
            chords: 2,
            ..top_bar()
        });
        assert_eq!(
            text_quads(&choir),
            text_quads(&plain) + "Resonance".len() + bold("37/100") + "Chords".len() + bold("2")
        );
        // A charge left with no chord standing still shows, without a count.
        let spent = bar(TopBar {
            resonance: 5,
            ..top_bar()
        });
        assert_eq!(
            text_quads(&spent),
            text_quads(&plain) + "Resonance".len() + bold("5/100")
        );
        // A full meter turns green.
        let full = bar(TopBar {
            resonance: 100,
            chords: 1,
            ..top_bar()
        });
        assert_eq!(in_top_bar(&full, theme::WODE), bold("100/100"));
        assert_eq!(in_top_bar(&plain, theme::WODE), 0);
    }

    #[test]
    fn honor_shows_in_the_top_bar_for_a_side_with_the_meter_even_at_zero() {
        let mut hud = Hud::default();
        let mut bar = |top| run_with(&mut hud, HudInput::default(), &[], &[], with_top(top)).1;
        let plain = bar(top_bar());
        let clans = bar(TopBar {
            honor: Some(40),
            ..top_bar()
        });
        assert_eq!(
            text_quads(&clans),
            text_quads(&plain) + "Honor".len() + bold("40/100")
        );
        let empty = bar(TopBar {
            honor: Some(0),
            ..top_bar()
        });
        assert_eq!(
            text_quads(&empty),
            text_quads(&plain) + "Honor".len() + bold("0/100")
        );
        // A full meter turns green.
        let full = bar(TopBar {
            honor: Some(100),
            ..top_bar()
        });
        assert_eq!(in_top_bar(&full, theme::WODE), bold("100/100"));
        assert_eq!(in_top_bar(&clans, theme::WODE), 0);
    }

    /// A frame with the mouse at `mouse` and the clock at `now_ms`.
    fn hover(hud: &mut Hud, mouse: (i32, i32), now_ms: u64) -> DrawList {
        let input = HudInput {
            mouse,
            now_ms,
            ..HudInput::default()
        };
        run(hud, input, &[], &[]).1
    }

    #[test]
    fn a_tooltip_waits_for_the_mouse_to_rest_then_names_the_button() {
        let mut hud = Hud::default();
        let l = HudLayout::new(SCREEN);
        // The Move button (slot 0), whose label is "Move".
        let spot = (l.grid.x + 10, l.grid.y + 10);
        hover(&mut hud, spot, 1_000);
        assert!(hud.tooltip_rect().is_none(), "not at once");
        hover(&mut hud, spot, 1_000 + TOOLTIP_DELAY_MS - 1);
        assert!(hud.tooltip_rect().is_none(), "not yet");
        let draw = hover(&mut hud, spot, 1_000 + TOOLTIP_DELAY_MS);
        let tip = hud.tooltip_rect().expect("shown after the delay");
        assert!(tip.x >= 0 && tip.y >= 0 && tip.right() <= SCREEN.0 && tip.bottom() <= SCREEN.1);
        assert!(
            !tip.contains(spot.0, spot.1) || tip.y < l.grid.y,
            "clear of the button"
        );
        // The key in brackets is drawn, in the tooltip's heading colour.
        let glyph = |c: char| u32::from(c as u8 - b' ');
        assert!(draw.quads.windows(3).any(|w| w[0].glyph == glyph('[')
            && w[1].glyph == glyph('Q')
            && w[2].glyph == glyph(']')));
        // Moving to another button starts the wait again.
        let other = (l.grid.x + l.grid.w - 10, l.grid.y + 10);
        hover(&mut hud, other, 5_000);
        assert!(hud.tooltip_rect().is_none());
        // Off the buttons: gone.
        hover(&mut hud, (400, 100), 6_000);
        hover(&mut hud, (400, 100), 9_000);
        assert!(hud.tooltip_rect().is_none());
    }

    #[test]
    fn a_tooltip_at_a_screen_corner_stays_on_the_screen() {
        for screen in [(1280, 760), (640, 400), (1920, 1080)] {
            let l = HudLayout::new(screen);
            let t = terrain();
            let grid: [GridSlot; GRID_SLOTS] = std::array::from_fn(|_| GridSlot {
                label: "Fall back",
                tip: "Back out of the fight in good order, taking one parting blow.",
                state: SlotState::Ready,
                ..GridSlot::default()
            });
            let mut hud = Hud::default();
            for now in [0, TOOLTIP_DELAY_MS + 1] {
                let data = HudData {
                    grid: &grid,
                    status: StatusStrip::default(),
                    selection: None,
                    help: None,
                    hero: None,
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
                    powers: &[],
                    picker: None,
                    tips: None,
                    handover: None,
                };
                let input = HudInput {
                    // The last button, in the bar's bottom-right corner.
                    mouse: (l.grid.right() - 4, l.grid.bottom() - 4),
                    now_ms: now,
                    ..HudInput::default()
                };
                let (mut draw, mut events) = (DrawList::default(), Vec::new());
                hud.frame(&mut draw, screen, input, &data, &mut events);
            }
            let tip = hud.tooltip_rect().expect("a tooltip");
            assert!(
                tip.x >= 0 && tip.y >= 0 && tip.right() <= screen.0 && tip.bottom() <= screen.1,
                "{tip:?} on {screen:?}"
            );
        }
    }

    #[test]
    fn the_help_button_reports_a_click_and_the_scroll_takes_the_mouse() {
        let mut hud = Hud::default();
        // The button is the right-most thing in the top bar.
        let button = (SCREEN.0 - 30, TOP_BAR_HEIGHT / 2 - 4);
        let click = HudInput {
            mouse: button,
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        };
        let (ev, _) = run(&mut hud, click, &[], &[]);
        assert_eq!(ev, vec![HudEvent::ToggleHelp]);

        let field_spot = (SCREEN.0 - 100, 150);
        assert!(!hud.wants(SCREEN, field_spot), "scroll closed: the world's");
        let mut data_with_scroll = |notes: &[String]| {
            let t = terrain();
            let grid: [GridSlot; GRID_SLOTS] = std::array::from_fn(|_| GridSlot::default());
            let data = HudData {
                grid: &grid,
                status: StatusStrip::default(),
                selection: None,
                help: Some(HelpScroll { notes }),
                hero: None,
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
                powers: &[],
                picker: None,
                tips: None,
                handover: None,
            };
            let (mut draw, mut events) = (DrawList::default(), Vec::new());
            hud.frame(&mut draw, SCREEN, HudInput::default(), &data, &mut events);
            draw
        };
        let notes = ["SANDBOX, commanding red (player 1)".to_string()];
        let open = data_with_scroll(&notes);
        assert!(
            hud.wants(SCREEN, field_spot),
            "scroll open: it takes clicks"
        );
        assert!(!hud.wants(SCREEN, (20, 300)), "the field beside it");
        // The title and every group heading are on it.
        let has = |d: &DrawList, text: &str| {
            let g: Vec<u32> = text
                .chars()
                .filter(|c| *c != ' ')
                .map(|c| u32::from(c as u8 - b' '))
                .collect();
            d.quads
                .windows(g.len())
                .any(|w| w.iter().map(|q| q.glyph).eq(g.iter().copied()))
        };
        assert!(has(&open, "THE MARSHAL'S SCROLL"));
        for (name, keys) in HELP_GROUPS {
            assert!(has(&open, name), "{name}");
            for (key, what) in keys {
                assert!(has(&open, key.trim()) && has(&open, what), "{key} {what}");
            }
        }
        assert!(has(&open, "commanding red"), "the live notes");
    }

    #[test]
    fn help_lists_every_key_once_per_group_and_fits_a_720p_field() {
        for (name, keys) in HELP_GROUPS {
            let mut seen = std::collections::HashSet::new();
            assert!(
                keys.iter().all(|(k, _)| seen.insert(*k)),
                "{name} repeats a key"
            );
        }
        // At 1280x720 the scroll leaves the bar and top bar alone.
        let mut hud = Hud::default();
        let t = terrain();
        let grid: [GridSlot; GRID_SLOTS] = std::array::from_fn(|_| GridSlot::default());
        let data = HudData {
            grid: &grid,
            status: StatusStrip::default(),
            selection: None,
            help: Some(HelpScroll::default()),
            hero: None,
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
            powers: &[],
            picker: None,
            tips: None,
            handover: None,
        };
        let screen = (1280, 720);
        let (mut draw, mut events) = (DrawList::default(), Vec::new());
        hud.frame(&mut draw, screen, HudInput::default(), &data, &mut events);
        let l = HudLayout::new(screen);
        let scroll = hud
            .over
            .iter()
            .copied()
            .find(|r| r.w > 400 && r.y >= l.top.bottom());
        let scroll = scroll.expect("the scroll");
        assert!(scroll.bottom() <= l.bar.y && scroll.right() <= screen.0 && scroll.x >= 0);
    }

    fn selection<'a>() -> Selection<'a> {
        Selection {
            name: "Yari Ashigaru",
            color: [200, 60, 50],
            icon: Icon::Spear,
            regiments: 1,
            models: 40,
            max_models: 60,
            morale: 80,
            state: CardState::Steady,
            formation: "Line",
            shapes: &[],
        }
    }

    /// A frame with `selection` and `cards`; its quads.
    fn with_selection(sel: Option<Selection>, cards: &[Card]) -> DrawList {
        let t = terrain();
        let grid: [GridSlot; GRID_SLOTS] = std::array::from_fn(|_| GridSlot::default());
        let data = HudData {
            grid: &grid,
            status: StatusStrip::default(),
            selection: sel,
            help: None,
            hero: None,
            minimap: MinimapData {
                map_tiles: 256.0,
                terrain: &t,
                dots: &[],
                view: &[],
                nodes: &[],
                blocks: &[],
            },
            cards,
            tray_title: "Selected",
            deployment: None,
            alerts: AlertData::default(),
            top: None,
            structure: None,
            powers: &[],
            picker: None,
            tips: None,
            handover: None,
        };
        let (mut draw, mut events) = (DrawList::default(), Vec::new());
        Hud::default().frame(&mut draw, SCREEN, HudInput::default(), &data, &mut events);
        draw
    }

    #[test]
    fn a_selection_shows_a_banner_in_the_sides_colour_with_its_name_and_count() {
        let tray = HudLayout::new(SCREEN).tray;
        let none = with_selection(None, &[]);
        let one = with_selection(Some(selection()), &[card(1)]);
        let in_tray = |d: &DrawList, c: Color| {
            d.quads
                .iter()
                .filter(|q| {
                    q.glyph == SOLID
                        && q.color == c
                        && tray.contains(i32::from(q.pos[0]), i32::from(q.pos[1]))
                })
                .count()
        };
        let side = rgb([200, 60, 50]);
        assert_eq!(in_tray(&none, side), 0);
        assert!(
            in_tray(&one, side) > 5,
            "the banner's cloth is in the side's colour"
        );
        let has = |d: &DrawList, text: &str| {
            let g: Vec<u32> = text
                .chars()
                .filter(|c| *c != ' ')
                .map(|c| u32::from(c as u8 - b' '))
                .collect();
            d.quads
                .windows(g.len())
                .any(|w| w.iter().map(|q| q.glyph).eq(g.iter().copied()))
        };
        assert!(has(&one, "Yari") && has(&one, "Ashigaru"));
        assert!(has(&one, "40of60soldiers"));
        assert!(has(&one, "Health") && has(&one, "Morale") && has(&one, "Formation:Line"));
        assert!(has(&none, "Clickaregimenttocommandit"));
        assert!(!has(&one, "Clickaregimenttocommandit"));
        // A mixed selection counts regiments and soldiers.
        let many = Selection {
            name: "3 regiments",
            regiments: 3,
            models: 120,
            ..selection()
        };
        let d = with_selection(Some(many), &[card(1), card(2), card(3)]);
        assert!(has(&d, "3regiments,120soldiers"));
        // Routing says so on the morale bar's label.
        let broken = Selection {
            state: CardState::Routing,
            ..selection()
        };
        assert!(has(&with_selection(Some(broken), &[card(1)]), "Routing"));
    }

    const NAMES: [&str; 8] = [
        "Line", "Deep", "Hammer", "Square", "Plain", "Mine 1", "Mine 2", "Mine 3",
    ];

    fn chips(count: usize, on: usize) -> Vec<ShapeChip<'static>> {
        NAMES[..count]
            .iter()
            .enumerate()
            .map(|(i, &label)| ShapeChip {
                label,
                tip: "A tip.",
                on: i == on,
            })
            .collect()
    }

    fn army<'a>(shapes: &'a [ShapeChip<'a>]) -> Selection<'a> {
        Selection {
            regiments: 3,
            name: "3 regiments",
            shapes,
            ..selection()
        }
    }

    #[test]
    fn the_army_shape_chips_fit_the_tray_without_covering_the_cards() {
        let all = chips(8, 0);
        for screen in [(1280, 720), (1920, 1080)] {
            let l = HudLayout::new(screen);
            let strip = Rect::new(l.tray.x + 8, l.tray.y + 8, l.tray.w - 16, SHAPE_CHIP_H);
            let (label, rects) = shape_chip_rects(strip, &all);
            assert_eq!(rects.len(), 8, "every chip fits at {screen:?}");
            assert!(label.is_some(), "and the label too");
            for (i, r) in rects.iter().enumerate() {
                assert!(r.x >= strip.x && r.right() <= strip.right());
                assert!(r.y == strip.y && r.bottom() <= l.tray.bottom());
                assert!(rects[i + 1..].iter().all(|o| o.x >= r.right()));
            }
        }
        // A cramped tray (UI scale 150% at 1280 wide) keeps what fits.
        let strip = Rect::new(0, 0, 300, SHAPE_CHIP_H);
        let (_, rects) = shape_chip_rects(strip, &all);
        assert!(rects.len() >= 5 && rects.iter().all(|r| r.right() <= 300));
    }

    #[test]
    fn clicking_an_army_shape_chip_sends_its_place_and_the_row_needs_two_regiments() {
        let shapes = chips(6, 1);
        let l = HudLayout::new(SCREEN);
        let strip = Rect::new(l.tray.x + 8, l.tray.y + 8, l.tray.w - 16, SHAPE_CHIP_H);
        let (_, rects) = shape_chip_rects(strip, &shapes);
        let at = rects[2];
        let click = HudInput {
            mouse: (at.x + 3, at.y + 3),
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        };
        let extras = |selection| Extras {
            selection: Some(selection),
            ..Extras::default()
        };
        let cards = [card(1), card(2), card(3)];
        let (ev, _) = run_with(
            &mut Hud::default(),
            click,
            &cards,
            &[],
            extras(army(&shapes)),
        );
        assert_eq!(ev, vec![HudEvent::ArmyShape(2)]);
        // One regiment has no row: the same spot is not a chip.
        let one = Selection {
            regiments: 1,
            ..army(&shapes)
        };
        let (ev, _) = run_with(&mut Hud::default(), click, &cards[..1], &[], extras(one));
        assert!(ev.is_empty());
        // The chosen chip is weld yellow.
        let (_, d) = run_with(
            &mut Hud::default(),
            HudInput::default(),
            &cards,
            &[],
            extras(army(&shapes)),
        );
        let lit = rects[1];
        assert!(d.quads.iter().any(|q| q.glyph == SOLID
            && q.color == theme::WELD
            && lit.contains(i32::from(q.pos[0]), i32::from(q.pos[1]))));
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
                rooted: None,
                powered: None,
                tuning: None,
                shooting: None,
                tips_open: None,
            }),
            ..Extras::default()
        }
    }

    /// A point inside queue slot `i` of the tray.
    fn queue_slot(i: i32) -> (i32, i32) {
        let row = HudLayout::new(SCREEN).tray.inset(6);
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
        let l = HudLayout::new(SCREEN);
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

    #[test]
    fn a_power_button_reports_its_click_and_takes_the_mouse() {
        let buttons = [PowerButton {
            name: "Overclock",
            detail: "20 Resonance",
            state: SlotState::Ready,
        }];
        let extras = || Extras {
            powers: &buttons,
            ..Extras::default()
        };
        let mut hud = Hud::default();
        let top = Some(TopBar::default());
        let field = HudLayout::new(SCREEN).field;
        let spot = (field.x + 60, field.y + 20);
        let click = HudInput {
            mouse: spot,
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        };
        assert!(!hud.wants(SCREEN, spot), "nothing drawn yet");
        let with_top = || Extras { top, ..extras() };
        let (events, _) = run_with(&mut hud, click, &[], &[], with_top());
        assert_eq!(events, vec![HudEvent::Power(0)]);
        assert!(hud.wants(SCREEN, spot));
        // No powers, no button, and the click is the battlefield's.
        let (events, _) = run_with(&mut hud, click, &[], &[], Extras::default());
        assert!(events.is_empty());
        assert!(!hud.wants(SCREEN, spot));
    }
}
