//! The Place panel (Sandbox): a linen cloth over the battlefield that lists
//! the regiment types the sandbox can put down, a row of the sandbox's sides,
//! and the roster troops it cannot place yet, greyed with the reason.
//! Docs: docs/08-ui-ux.md#the-place-panel.
//!
//! The game says what each row is called and what its second line says; this
//! draws the panel and reports a click on a row ([`HudEvent::PlaceUnit`]), on
//! a side ([`HudEvent::PlaceSide`]) or on the close box
//! ([`HudEvent::TogglePlacer`]). When the screen is too small for everything
//! the panel sheds detail in steps ([`Tier`]) rather than leave the screen.

use crate::draw::{CELL_W, DrawList, Rect, fit, mix, text_width, wrap};
use crate::hud::{HudEvent, HudInput, WIDGET_PLACE_ROW, WIDGET_PLACE_SIDE, muted_ink, rgb};
use crate::icons::{self, Icon};
use crate::ui::{Press, theme};

/// Panel width when there is room, and the least worth drawing.
const WIDTH: i32 = 440;
const MIN_WIDTH: i32 = 220;
const PAD: i32 = 14;
const MARGIN: i32 = 8;
/// The panel starts this far in from the field's left edge, clear of an alert
/// badge on it.
const INDENT: i32 = 40;
const ROW_GAP: i32 = 3;
const SIDE_SIZE: i32 = 24;
/// How many lines of troop names one faction may take, per [`Tier`].
const NAME_LINES: [usize; 6] = [2, 1, 0, 0, 0, 0];
/// Height of the tab strip and of the faction chips under it.
const STRIP_H: i32 = 28;

/// One regiment type the sandbox can place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaceRow<'a> {
    pub name: &'a str,
    /// The second line: soldiers, speed, cost.
    pub detail: &'a str,
    /// Which faction fields it, e.g. "Ember Clans".
    pub faction: &'a str,
    pub icon: Icon,
}

/// A side of the sandbox, to place for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaceSide<'a> {
    pub name: &'a str,
    pub color: [u8; 3],
}

/// A faction's roster troops that exist in the data but are not placeable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaceLater<'a> {
    pub faction: &'a str,
    pub color: [u8; 3],
    /// How many there are.
    pub count: usize,
    /// Their names, comma-separated.
    pub names: &'a str,
}

/// What the Place panel shows. `None` in [`HudData::picker`](crate::hud::HudData)
/// hides the button too.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlacePicker<'a> {
    /// The panel is open (the button is lit).
    pub open: bool,
    /// Left-click on the field places a regiment.
    pub placing: bool,
    pub rows: &'a [PlaceRow<'a>],
    /// The row that is the chosen type.
    pub current: usize,
    pub sides: &'a [PlaceSide<'a>],
    pub side: usize,
    /// Why `later` cannot be placed, in a line or two.
    pub reason: &'a str,
    pub later: &'a [PlaceLater<'a>],
    /// The tabs over the rows ("Field types", "All factions"); none, no strip.
    pub tabs: &'a [&'a str],
    /// The tab that is showing.
    pub tab: usize,
    /// The factions of the roster tab, as chips under the tabs (the name is
    /// short, the colour its dye); none on the other tab.
    pub chips: &'a [PlaceSide<'a>],
    pub chip: usize,
}

impl PlacePicker<'_> {
    /// How far below the panel's top the first row starts.
    fn rows_top(&self) -> i32 {
        let strips = i32::from(!self.tabs.is_empty()) + i32::from(!self.chips.is_empty());
        PAD + 26 + 34 + strips * STRIP_H
    }
}

/// How much of the panel is drawn, from everything to the least.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tier {
    /// Two-line rows, the reason, and each faction's troop names.
    Full = 0,
    /// One line of names a faction.
    Names = 1,
    /// Counts only.
    Counts = 2,
    /// Tighter two-line rows; nothing greyed.
    Tight = 3,
    /// One-line rows and nothing greyed.
    Bare = 4,
    /// One-line rows packed close, for the smallest screens.
    Tiny = 5,
}

impl Tier {
    const ALL: [Tier; 6] = [
        Tier::Full,
        Tier::Names,
        Tier::Counts,
        Tier::Tight,
        Tier::Bare,
        Tier::Tiny,
    ];

    fn row_h(self) -> i32 {
        match self {
            Tier::Tiny => 20,
            Tier::Bare => 24,
            Tier::Tight => 32,
            _ => 40,
        }
    }

    /// Rows of a name alone, with nothing greyed under them.
    fn one_line(self) -> bool {
        matches!(self, Tier::Bare | Tier::Tiny)
    }

    /// Whether the greyed section (the reason and each faction's troops) is
    /// drawn: the three fullest tiers.
    fn greyed(self) -> bool {
        (self as usize) <= Tier::Counts as usize
    }

    fn gap(self) -> i32 {
        if self == Tier::Tiny { 1 } else { ROW_GAP }
    }

    fn lines(self) -> usize {
        NAME_LINES[self as usize]
    }
}

/// What a tooltip should say about a row or side: its id, where, and words.
pub(crate) struct PlaceTip {
    pub id: u32,
    pub anchor: Rect,
    pub name: String,
    pub key: String,
    pub text: String,
}

/// The panel's geometry for one tier.
struct Plan {
    panel: Rect,
    tier: Tier,
    reason: Vec<String>,
    /// Each faction's lines of names (empty for none), with its heading.
    later: Vec<Vec<String>>,
}

fn plan(field: Rect, p: &PlacePicker) -> Option<Plan> {
    let w = WIDTH.min(field.w - INDENT - MARGIN);
    if w < MIN_WIDTH {
        return None;
    }
    let chars = ((w - 2 * PAD - 2) / CELL_W).max(1) as usize;
    // The troop names are indented under their faction's swatch.
    let name_chars = chars.saturating_sub(2).max(1);
    let room = field.h - 2 * MARGIN;
    for tier in Tier::ALL {
        let reason = if !tier.greyed() {
            Vec::new()
        } else {
            wrap(p.reason, chars)
        };
        let later: Vec<Vec<String>> = p
            .later
            .iter()
            .map(|g| ellipsis(wrap(g.names, name_chars), tier.lines(), name_chars))
            .collect();
        let rows = p.rows.len() as i32 * (tier.row_h() + tier.gap());
        let mut h = p.rows_top() + PAD + rows;
        if tier.greyed() && !p.later.is_empty() {
            h += 28 + reason.len() as i32 * 16;
            for l in &later {
                h += 20 + l.len() as i32 * 16;
            }
        }
        if h <= room {
            let panel = Rect::new(field.x + INDENT, field.y + MARGIN, w, h);
            return Some(Plan {
                panel,
                tier,
                reason,
                later,
            });
        }
    }
    None
}

/// The first `keep` of `lines`; when some are left out, the last kept one ends
/// in "..." instead of a dangling comma.
fn ellipsis(mut lines: Vec<String>, keep: usize, chars: usize) -> Vec<String> {
    if lines.len() <= keep {
        return lines;
    }
    lines.truncate(keep);
    if let Some(last) = lines.last_mut() {
        let mut text = last.trim_end_matches([',', ' ']).to_string();
        while text.chars().count() + 4 > chars {
            match text.rfind(' ') {
                Some(i) => text.truncate(i),
                None => break,
            }
            text = text.trim_end_matches([',', ' ']).to_string();
        }
        *last = format!("{text}, ...");
    }
    lines
}

/// Where the panel sits, if it would be drawn (for tests and the game).
pub fn panel_rect(field: Rect, p: &PlacePicker) -> Option<Rect> {
    plan(field, p).map(|plan| plan.panel)
}

/// Where row `i` sits inside the panel `r` at the tier that fits.
fn row_rect(r: Rect, tier: Tier, p: &PlacePicker, i: usize) -> Rect {
    Rect::new(
        r.x + PAD,
        r.y + p.rows_top() + i as i32 * (tier.row_h() + tier.gap()),
        r.w - 2 * PAD,
        tier.row_h(),
    )
}

fn side_rect(r: Rect, i: usize) -> Rect {
    Rect::new(
        r.x + PAD + 52 + i as i32 * (SIDE_SIZE + 6),
        r.y + PAD + 26 + 2,
        SIDE_SIZE,
        SIDE_SIZE,
    )
}

/// What the tabs say when the mouse rests on them.
const TAB_TIPS: [&str; 2] = [
    "The five Field Battle types, as in a Field Battle army.",
    "Every fighting troop of the four rosters (workers and the Caravan are left out). Picking one starts the sandbox again on the roster table: the field is cleared.",
];

/// Button `i` of `n` in a strip at `y`, each `w` wide.
fn tab_rect(r: Rect, y: i32, n: usize, i: usize, w: i32) -> Rect {
    let gap = if n > 3 { 4 } else { 6 };
    Rect::new(r.x + PAD + i as i32 * (w + gap), y, w, STRIP_H - 6)
}

/// The close box at the panel's top right.
fn close_rect(r: Rect) -> Rect {
    Rect::new(r.right() - PAD - 18, r.y + PAD - 4, 18, 18)
}

/// Draw the panel when it is open, report clicks, and note in `over` the
/// rectangle that now takes the mouse from the battlefield. Returns a tooltip
/// to show if the mouse rests on something.
pub(crate) fn draw(
    draw: &mut DrawList,
    field: Rect,
    input: HudInput,
    press: &mut Press,
    p: &PlacePicker,
    over: &mut Vec<Rect>,
    events: &mut Vec<HudEvent>,
) -> Option<PlaceTip> {
    if !p.open {
        return None;
    }
    let plan = plan(field, p)?;
    let (r, tier) = (plan.panel, plan.tier);
    let mouse = input.mouse;
    let mut tip = None;
    draw.shadow(r, 4);
    draw.cloth(r, theme::LINEN);
    over.push(r);

    // Title, with a close box.
    let y = r.y + PAD - 4;
    draw.text_bold(r.x + PAD, y, 1, theme::MADDER, "PLACE REGIMENTS");
    let close = close_rect(r);
    let (clicked, _) = press.widget(
        WIDGET_PLACE_SIDE + 1000,
        close,
        mouse,
        input.left_pressed,
        input.left_released,
    );
    if clicked {
        events.push(HudEvent::TogglePlacer);
    }
    let hot = close.contains(mouse.0, mouse.1);
    draw.boxed(
        close,
        if hot {
            mix(theme::WELD, [255, 240, 190, 255], 0.4)
        } else {
            theme::LINEN_DK
        },
        2,
    );
    let x = close.x + 5;
    for k in 0..8 {
        draw.rect(Rect::new(x + k, close.y + 5 + k, 2, 2), theme::INK);
        draw.rect(Rect::new(x + 7 - k, close.y + 5 + k, 2, 2), theme::INK);
    }
    if hot {
        tip = Some(PlaceTip {
            id: 300,
            anchor: close,
            name: "Close".into(),
            key: String::new(),
            text: "Put the panel away. Placing carries on until Esc.".into(),
        });
    }

    // Sides.
    let sy = r.y + PAD + 26;
    draw.text_bold(r.x + PAD, sy + 6, 1, theme::WOAD, "Side");
    for (i, s) in p.sides.iter().enumerate() {
        let b = side_rect(r, i);
        if b.right() > r.right() - PAD {
            break;
        }
        let (clicked, down) = press.widget(
            WIDGET_PLACE_SIDE + i as u32,
            b,
            mouse,
            input.left_pressed,
            input.left_released,
        );
        if clicked {
            events.push(HudEvent::PlaceSide(i));
        }
        let hot = b.contains(mouse.0, mouse.1);
        let chosen = i == p.side;
        if chosen {
            draw.rect(b.inset(-3), theme::WELD);
            draw.border(b.inset(-3), 2, theme::INK);
        }
        let fill = if down {
            mix(rgb(s.color), theme::INK, 0.3)
        } else if hot {
            mix(rgb(s.color), [255, 250, 230, 255], 0.25)
        } else {
            rgb(s.color)
        };
        draw.boxed(b, fill, 2);
        if hot {
            tip = Some(PlaceTip {
                id: 310 + i as u32,
                anchor: b,
                name: s.name.to_string(),
                key: "Tab".into(),
                text: "Regiments you place belong to this side, and it is the side you command."
                    .into(),
            });
        }
    }
    if let Some(s) = p.sides.get(p.side) {
        let after = side_rect(r, p.sides.len()).x + 4;
        let chars = ((r.right() - PAD - after) / CELL_W).max(0) as usize;
        if chars >= 3 {
            draw.text(after, sy + 6, 1, theme::INK, &fit(s.name, chars));
        }
    }

    // The tabs, and under them the roster's faction chips.
    let mut strip_y = r.y + PAD + 26 + 34;
    if !p.tabs.is_empty() {
        let w = (r.w - 2 * PAD - 6 * (p.tabs.len() as i32 - 1)) / p.tabs.len() as i32;
        for (i, name) in p.tabs.iter().enumerate() {
            let b = tab_rect(r, strip_y, p.tabs.len(), i, w);
            let (clicked, down) = press.widget(
                WIDGET_PLACE_SIDE + 2000 + i as u32,
                b,
                mouse,
                input.left_pressed,
                input.left_released,
            );
            if clicked {
                events.push(HudEvent::PlaceTab(i));
            }
            let hot = b.contains(mouse.0, mouse.1);
            let fill = match (down, hot, i == p.tab) {
                (true, _, _) => mix(theme::WELD, theme::INK, 0.2),
                (_, _, true) => theme::WELD,
                (_, true, _) => mix(theme::LINEN_DK, theme::LINEN, 0.5),
                _ => theme::LINEN_DK,
            };
            draw.boxed(b, fill, 2);
            let label = fit(name, ((b.w - 8) / CELL_W).max(1) as usize);
            let tw = text_width(&label, 1);
            draw.text_bold(b.x + (b.w - tw) / 2, b.y + 5, 1, theme::INK, &label);
            if hot {
                tip = Some(PlaceTip {
                    id: 340 + i as u32,
                    anchor: b,
                    name: (*name).to_string(),
                    key: "click".into(),
                    text: TAB_TIPS.get(i).copied().unwrap_or("").into(),
                });
            }
        }
        strip_y += STRIP_H;
    }
    if !p.chips.is_empty() {
        let w = (r.w - 2 * PAD - 4 * (p.chips.len() as i32 - 1)) / p.chips.len() as i32;
        for (i, c) in p.chips.iter().enumerate() {
            let b = tab_rect(r, strip_y, p.chips.len(), i, w);
            let (clicked, down) = press.widget(
                WIDGET_PLACE_SIDE + 3000 + i as u32,
                b,
                mouse,
                input.left_pressed,
                input.left_released,
            );
            if clicked {
                events.push(HudEvent::PlaceFaction(i));
            }
            let hot = b.contains(mouse.0, mouse.1);
            if i == p.chip {
                draw.rect(b.inset(-3), theme::WELD);
                draw.border(b.inset(-3), 2, theme::INK);
            }
            let dye = rgb(c.color);
            let fill = if down {
                mix(dye, theme::INK, 0.3)
            } else if hot {
                mix(dye, [255, 250, 230, 255], 0.25)
            } else {
                dye
            };
            draw.boxed(b, fill, 2);
            let label = fit(c.name, ((b.w - 6) / CELL_W).max(1) as usize);
            let tw = text_width(&label, 1);
            draw.text_bold(
                b.x + (b.w - tw) / 2,
                b.y + 5,
                1,
                [255, 250, 235, 255],
                &label,
            );
            if hot {
                tip = Some(PlaceTip {
                    id: 350 + i as u32,
                    anchor: b,
                    name: c.name.to_string(),
                    key: "click".into(),
                    text: "Show this faction's troops.".into(),
                });
            }
        }
    }

    // The types.
    let chars_row = ((r.w - 2 * PAD - 16 - 24) / CELL_W).max(1) as usize;
    for (i, row) in p.rows.iter().enumerate() {
        let b = row_rect(r, tier, p, i);
        if b.bottom() > r.bottom() - 4 {
            break;
        }
        let (clicked, down) = press.widget(
            WIDGET_PLACE_ROW + i as u32,
            b,
            mouse,
            input.left_pressed,
            input.left_released,
        );
        if clicked {
            events.push(HudEvent::PlaceUnit(i));
        }
        let hot = b.contains(mouse.0, mouse.1);
        let chosen = i == p.current;
        let fill = match (down, hot, chosen) {
            (true, _, _) => mix(theme::WELD, theme::INK, 0.2),
            (_, _, true) => theme::WELD,
            (_, true, _) => mix(theme::LINEN_DK, theme::LINEN, 0.5),
            _ => theme::LINEN_DK,
        };
        draw.boxed(b, fill, 2);
        let isz = if tier == Tier::Tiny { 16 } else { 24 };
        let ib = Rect::new(b.x + 8, b.y + (b.h - isz) / 2, isz, isz);
        icons::icon(draw, ib, row.icon, theme::INK);
        let tx = ib.right() + 8;
        let name_y = match tier {
            Tier::Bare => b.y + 4,
            Tier::Tiny => b.y + 2,
            Tier::Tight => b.y + 3,
            _ => b.y + 5,
        };
        draw.text_bold(tx, name_y, 1, theme::INK, &fit(row.name, chars_row));
        if chosen {
            let tag = if p.placing { "placing" } else { "chosen" };
            let tw = text_width(tag, 1);
            draw.text(b.right() - 8 - tw, name_y, 1, theme::MADDER, tag);
        }
        if !tier.one_line() {
            let detail_y = if tier == Tier::Tight {
                b.y + 17
            } else {
                b.y + 22
            };
            draw.text(tx, detail_y, 1, muted_ink(), &fit(row.detail, chars_row));
        }
        if hot {
            tip = Some(PlaceTip {
                id: 330 + i as u32,
                anchor: b,
                name: row.name.to_string(),
                key: "click".into(),
                text: format!(
                    "{}. Choose it, then click the field to place a regiment.",
                    row.faction
                ),
            });
        }
    }

    // What exists but cannot be placed yet.
    if tier.greyed() && !p.later.is_empty() {
        let mut y = row_rect(r, tier, p, p.rows.len()).y + 2;
        draw.text_bold(r.x + PAD, y, 1, theme::WOAD, "Not placeable yet");
        draw.rect(Rect::new(r.x + PAD, y + 17, r.w - 2 * PAD, 1), theme::INK);
        y += 24;
        for line in &plan.reason {
            draw.text(r.x + PAD, y, 1, muted_ink(), line);
            y += 16;
        }
        let grey = mix(theme::INK, theme::LINEN, 0.45);
        for (g, lines) in p.later.iter().zip(&plan.later) {
            if y + 16 > r.bottom() - 6 {
                break;
            }
            draw.boxed(Rect::new(r.x + PAD, y + 2, 10, 10), rgb(g.color), 1);
            let head = format!("{} ({})", g.faction, g.count);
            draw.text_bold(r.x + PAD + 16, y, 1, grey, &head);
            y += 20;
            for line in lines {
                if y + 16 > r.bottom() - 6 {
                    break;
                }
                draw.text(r.x + PAD + 16, y - 2, 1, grey, line);
                y += 16;
            }
        }
    }
    tip
}

/// How much detail the panel keeps on this field, for tests.
#[cfg(test)]
fn tier_of(field: Rect, p: &PlacePicker) -> Option<u8> {
    plan(field, p).map(|plan| plan.tier as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIELD: Rect = Rect::new(0, 36, 1280, 576);

    const ROWS: [PlaceRow<'static>; 5] = [
        PlaceRow {
            name: "Yari Ashigaru",
            detail: "28 soldiers  3.0 tiles/s  400 points",
            faction: "Ember Clans",
            icon: Icon::Spear,
        },
        PlaceRow {
            name: "Skitterlings",
            detail: "32 soldiers  4.0 tiles/s  400 points",
            faction: "Brood",
            icon: Icon::Sword,
        },
        PlaceRow {
            name: "Shardlings",
            detail: "14 soldiers  400 points",
            faction: "Choir",
            icon: Icon::Bow,
        },
        PlaceRow {
            name: "Bone Thralls",
            detail: "30 soldiers  400 points",
            faction: "Carrion Concord",
            icon: Icon::Sword,
        },
        PlaceRow {
            name: "Carrion Crows",
            detail: "20 soldiers  400 points",
            faction: "Carrion Concord",
            icon: Icon::Bow,
        },
    ];
    const SIDES: [PlaceSide<'static>; 4] = [
        PlaceSide {
            name: "Red",
            color: [200, 60, 50],
        },
        PlaceSide {
            name: "Blue",
            color: [60, 90, 200],
        },
        PlaceSide {
            name: "Green",
            color: [60, 160, 70],
        },
        PlaceSide {
            name: "Yellow",
            color: [220, 190, 50],
        },
    ];
    const LATER: [PlaceLater<'static>; 2] = [
        PlaceLater {
            faction: "Ember Clans",
            color: [200, 60, 50],
            count: 9,
            names: "Hands, Ronin, Naginata Guard, Archers, Cavalry, Cannon, Monks, Samurai, Spirit",
        },
        PlaceLater {
            faction: "Brood",
            color: [60, 160, 70],
            count: 11,
            names: "Drones, Spitters, Brutes, Wyrms",
        },
    ];

    fn picker(current: usize) -> PlacePicker<'static> {
        PlacePicker {
            open: true,
            placing: false,
            rows: &ROWS,
            current,
            sides: &SIDES,
            side: 1,
            reason: "These are Conquest troops: you train them at buildings there. The sandbox fields only the Field Battle types.",
            later: &LATER,
            tabs: &[],
            tab: 0,
            chips: &[],
            chip: 0,
        }
    }

    fn click_at(r: Rect) -> HudInput {
        HudInput {
            mouse: (r.x + r.w / 2, r.y + r.h / 2),
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        }
    }

    type Out = (Vec<HudEvent>, Vec<Rect>, DrawList);

    fn frame(field: Rect, p: &PlacePicker, input: HudInput) -> Out {
        let (mut d, mut over, mut events) = (DrawList::default(), Vec::new(), Vec::new());
        let mut press = Press::default();
        draw(&mut d, field, input, &mut press, p, &mut over, &mut events);
        (events, over, d)
    }

    fn tier(field: Rect, p: &PlacePicker) -> Tier {
        Tier::ALL[tier_of(field, p).unwrap() as usize]
    }

    #[test]
    fn clicking_a_row_chooses_that_type() {
        let p = picker(0);
        let panel = panel_rect(FIELD, &p).unwrap();
        let t = tier(FIELD, &p);
        for i in 0..ROWS.len() {
            let (events, _, _) = frame(FIELD, &p, click_at(row_rect(panel, t, &p, i)));
            assert_eq!(events, vec![HudEvent::PlaceUnit(i)], "row {i}");
        }
        // The press alone does nothing; the click fires on release.
        let mut press_only = click_at(row_rect(panel, t, &p, 2));
        press_only.left_released = false;
        assert!(frame(FIELD, &p, press_only).0.is_empty());
    }

    #[test]
    fn clicking_a_side_or_the_close_box_says_so() {
        let p = picker(0);
        let panel = panel_rect(FIELD, &p).unwrap();
        for i in 0..SIDES.len() {
            let (events, _, _) = frame(FIELD, &p, click_at(side_rect(panel, i)));
            assert_eq!(events, vec![HudEvent::PlaceSide(i)]);
        }
        let (events, _, _) = frame(FIELD, &p, click_at(close_rect(panel)));
        assert_eq!(events, vec![HudEvent::TogglePlacer]);
    }

    /// Whether a weld-coloured quad fills most of row `i`.
    fn lit(d: &DrawList, panel: Rect, t: Tier, i: usize) -> bool {
        let r = row_rect(panel, t, &PlacePicker::default(), i);
        d.quads.iter().any(|q| {
            let (x, y) = (i32::from(q.pos[0]), i32::from(q.pos[1]));
            q.color == theme::WELD
                && x >= r.x
                && y >= r.y
                && x < r.right()
                && y < r.bottom()
                && i32::from(q.size[0]) >= r.w - 4
        })
    }

    #[test]
    fn the_current_row_is_lit_weld_and_the_others_are_not() {
        let p = picker(3);
        let d = frame(FIELD, &p, HudInput::default()).2;
        let panel = panel_rect(FIELD, &p).unwrap();
        let t = tier(FIELD, &p);
        assert!(lit(&d, panel, t, 3));
        assert!(!lit(&d, panel, t, 0) && !lit(&d, panel, t, 2) && !lit(&d, panel, t, 4));
        // It says "chosen", or "placing" once placing is armed.
        let armed = PlacePicker { placing: true, ..p };
        let d2 = frame(FIELD, &armed, HudInput::default()).2;
        assert!(
            d2.quads.len() > d.quads.len(),
            "'placing' is longer than 'chosen'"
        );
    }

    #[test]
    fn closed_it_draws_and_takes_nothing() {
        let p = PlacePicker {
            open: false,
            ..picker(0)
        };
        let (events, over, d) = frame(FIELD, &p, click_at(Rect::new(100, 100, 10, 10)));
        assert!(events.is_empty() && over.is_empty() && d.quads.is_empty());
    }

    #[test]
    fn the_panel_takes_the_mouse_from_the_battlefield() {
        let p = picker(0);
        let (_, over, _) = frame(FIELD, &p, HudInput::default());
        assert_eq!(over, vec![panel_rect(FIELD, &p).unwrap()]);
    }

    #[test]
    fn the_panel_fits_every_screen_by_shedding_detail() {
        let p = picker(0);
        let mut last = 0;
        for (w, h) in [
            (1920, 1080),
            (1280, 760),
            (1024, 600),
            (800, 480),
            (640, 400),
        ] {
            // The field: the screen between the top and bottom bars.
            let field = Rect::new(0, 36, w, h - 36 - 168);
            let Some(panel) = panel_rect(field, &p) else {
                // Too small to be worth drawing is only for tiny fields.
                assert!(field.h < 240 || field.w < 270, "{w}x{h}");
                continue;
            };
            assert!(
                panel.x >= field.x
                    && panel.y >= field.y
                    && panel.right() <= field.right()
                    && panel.bottom() <= field.bottom(),
                "{w}x{h}: {panel:?} not in {field:?}"
            );
            let t = tier_of(field, &p).unwrap();
            assert!(t >= last, "smaller screens never show more");
            last = t;
            // Nothing is drawn outside the panel (a lit side's halo aside).
            let d = frame(field, &p, HudInput::default()).2;
            for q in &d.quads {
                let (x, y) = (i32::from(q.pos[0]), i32::from(q.pos[1]));
                let (qr, qb) = (x + i32::from(q.size[0]), y + i32::from(q.size[1]));
                assert!(
                    x >= panel.x - 4
                        && y >= panel.y - 4
                        && qr <= panel.right() + 6
                        && qb <= panel.bottom() + 6,
                    "{w}x{h}: stray quad at {x},{y}"
                );
            }
            // Every type can still be clicked.
            let t = Tier::ALL[t as usize];
            for i in 0..ROWS.len() {
                assert!(
                    row_rect(panel, t, &p, i).bottom() <= panel.bottom(),
                    "{w}x{h}: row {i} clipped"
                );
            }
        }
        assert_eq!(tier_of(Rect::new(0, 36, 1920, 800), &p), Some(0));
        assert!(last >= 2, "the smallest screens shed the greyed names");
    }

    #[test]
    fn the_greyed_section_draws_the_reason_and_the_factions() {
        let p = picker(0);
        let with = frame(FIELD, &p, HudInput::default()).2;
        let without = frame(FIELD, &PlacePicker { later: &[], ..p }, HudInput::default()).2;
        assert!(with.quads.len() > without.quads.len() + 40);
    }

    const TABS: [&str; 2] = ["Field types", "All factions"];
    const CHIPS: [PlaceSide<'static>; 4] = [
        PlaceSide {
            name: "Ember",
            color: [180, 60, 50],
        },
        PlaceSide {
            name: "Brood",
            color: [90, 130, 60],
        },
        PlaceSide {
            name: "Choir",
            color: [50, 90, 170],
        },
        PlaceSide {
            name: "Carrion",
            color: [112, 84, 120],
        },
    ];

    /// Eleven roster rows under the tabs and chips, as the Ember Clans fill.
    fn roster_rows() -> Vec<PlaceRow<'static>> {
        (0..11)
            .map(|_| PlaceRow {
                name: "Yumi Ashigaru",
                detail: "16 soldiers   3.0 tiles/s   120 Grub 40 Marrow",
                faction: "Ember Clans",
                icon: Icon::Bow,
            })
            .collect()
    }

    #[test]
    fn the_tabs_and_faction_chips_report_their_clicks_and_move_the_rows_down() {
        let rows = roster_rows();
        let p = PlacePicker {
            rows: &rows,
            tabs: &TABS,
            tab: 1,
            chips: &CHIPS,
            chip: 2,
            later: &[],
            reason: "",
            ..picker(0)
        };
        let panel = panel_rect(FIELD, &p).unwrap();
        let top = panel.y + PAD + 26 + 34;
        for i in 0..TABS.len() {
            let w = (panel.w - 2 * PAD - 6) / 2;
            let b = tab_rect(panel, top, 2, i, w);
            let (events, _, _) = frame(FIELD, &p, click_at(b));
            assert_eq!(events, vec![HudEvent::PlaceTab(i)]);
        }
        for i in 0..CHIPS.len() {
            let w = (panel.w - 2 * PAD - 12) / 4;
            let b = tab_rect(panel, top + STRIP_H, 4, i, w);
            let (events, _, _) = frame(FIELD, &p, click_at(b));
            assert_eq!(events, vec![HudEvent::PlaceFaction(i)]);
        }
        // The rows start below both strips, and still click.
        let t = tier(FIELD, &p);
        assert!(row_rect(panel, t, &p, 0).y >= top + 2 * STRIP_H);
        for i in 0..rows.len() {
            let b = row_rect(panel, t, &p, i);
            assert!(b.bottom() <= panel.bottom(), "row {i} clipped");
            let (events, _, _) = frame(FIELD, &p, click_at(b));
            assert_eq!(events, vec![HudEvent::PlaceUnit(i)], "row {i}");
        }
    }

    #[test]
    fn a_faction_of_troops_fits_every_screen_by_shedding_detail() {
        let rows = roster_rows();
        let later = [PlaceLater {
            faction: "Ember Clans",
            color: [200, 60, 50],
            count: 1,
            names: "Hands",
        }];
        let p = PlacePicker {
            rows: &rows,
            tabs: &TABS,
            tab: 1,
            chips: &CHIPS,
            later: &later,
            reason: "Not regiments to fight with.",
            ..picker(0)
        };
        for (w, h) in [(1920, 1080), (1280, 720), (1024, 600), (800, 480)] {
            let field = Rect::new(0, 36, w, h - 36 - 168);
            let Some(panel) = panel_rect(field, &p) else {
                assert!(field.h < 300 || field.w < 270, "{w}x{h}");
                continue;
            };
            assert!(panel.bottom() <= field.bottom(), "{w}x{h}");
            let t = tier(field, &p);
            for i in 0..rows.len() {
                assert!(
                    row_rect(panel, t, &p, i).bottom() <= panel.bottom(),
                    "{w}x{h}: row {i}"
                );
            }
        }
        // A tall screen keeps the two-line rows.
        assert_eq!(tier_of(Rect::new(0, 36, 1920, 1000), &p), Some(0));
    }
}
