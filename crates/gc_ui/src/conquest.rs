//! The Conquest pieces of the HUD: the top bar with the economy and the
//! clock, the gathering spots and buildings on the minimap, and a selected
//! building's panel in the army tray.
//! Docs: docs/08-ui-ux.md#screen-layout-1080p-reference.
//!
//! Like the rest of the HUD this is plain data in, quads out: the game
//! describes what to show and [`Hud::frame`](crate::hud::Hud::frame) calls
//! these to draw it.

use std::fmt::Write as _;

use crate::draw::{CELL_H, CELL_W, Color, DrawList, Rect, fit, text_width, wrap};
use crate::hud::{HudEvent, HudInput, WIDGET_QUEUE, draw_portrait, health_color, rgb};
use crate::lobby::thousands;
use crate::ui::{Press, theme};

const MARGIN: i32 = 8;
/// Room between the items of the top bar.
const TOP_GAP: i32 = 24;
/// A Grub gathering spot on the minimap: a warm gold.
pub const GRUB_NODE: [u8; 3] = [230, 190, 60];
/// A Marrow gathering spot on the minimap: bone white.
pub const MARROW_NODE: [u8; 3] = [235, 225, 200];
/// The least a building covers on the minimap, in pixels a side.
const BLOCK_MIN: i32 = 2;
/// How many queue slots the tray shows.
pub const QUEUE_SLOTS: usize = 5;
pub(crate) const QUEUE_SLOT_MAX_W: i32 = 64;
/// Width of the building's card in the tray.
pub(crate) const STRUCT_CARD_W: i32 = 232;
pub(crate) const CARD_GAP: i32 = 4;

/// The economy and the clock, for the bar across the top of the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TopBar {
    pub grub: u32,
    pub marrow: u32,
    pub supply_used: u32,
    pub supply_cap: u32,
    /// Percent of gathered Grub kept (100 = no upkeep).
    pub upkeep_percent: u32,
    /// Brood: larvae ready and how many the Hives and Hatchery Pits hold.
    /// Shown only when the room is above 0.
    pub larvae: u32,
    pub larva_room: u32,
    /// Choir: the Resonance meter (0 to 100) and the chords charging it.
    /// Shown only while either is above 0.
    pub resonance: u32,
    pub chords: u32,
    /// Clans: the Honor meter (0 to 100), shown whenever there is one: a
    /// side whose meter is Honor shows it even at 0.
    pub honor: Option<u32>,
    /// Match time.
    pub clock_secs: u32,
}

/// A gathering spot on the minimap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeDot {
    /// Map position, in tiles.
    pub pos: [f32; 2],
    pub marrow: bool,
    /// Run dry (dregs): drawn dim.
    pub dry: bool,
}

/// A building on the minimap, as its footprint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    /// North-west corner and size, in tiles.
    pub rect: [f32; 4],
    pub color: [u8; 3],
    pub selected: bool,
}

/// One unit in a building's training queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueItem<'a> {
    pub name: &'a str,
    /// Percent trained. The front one trains, and so does every unit that
    /// hatches (all at once); the others show 0.
    pub progress: u8,
}

/// A selected building, for the army tray.
#[derive(Clone, Copy, Debug)]
pub struct StructurePanel<'a> {
    pub name: &'a str,
    pub color: [u8; 3],
    pub hp: u32,
    pub max_hp: u32,
    /// Under construction: percent built (None once finished).
    pub building: Option<u8>,
    /// Its tier upgrade: what it becomes and percent done.
    pub upgrading: Option<(&'a str, u8)>,
    /// Its training queue, front first (at most 5 are shown).
    pub queue: &'a [QueueItem<'a>],
    /// Larvae ready here and the most it holds (None for a building with none).
    pub larvae: Option<(u8, u8)>,
    /// A living building of yours: whether it stands on your linked
    /// root-veins, where it regrows (None for any other building).
    pub rooted: Option<bool>,
    /// A Choir building of yours that needs power: whether a field of
    /// power reaches it (None for any other building).
    pub powered: Option<bool>,
    /// A building of yours that tunes seeps or veins (None for any other).
    pub tuning: Option<Tuning>,
}

/// What a tuning building (a Resonator or Siphon) is working.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tuning {
    /// It tunes Marrow veins, not Grub seeps.
    pub vein: bool,
    /// How many nodes it tunes now.
    pub nodes: u8,
    /// What they yield together, a minute, at the match tempo (before any
    /// upkeep cut to Grub).
    pub per_minute: u32,
    /// How many percent more its seeps yield under a chord (0 for none).
    pub harmony: u8,
}

impl Tuning {
    /// The panel's line about it.
    fn line(self) -> String {
        let (node, what) = if self.vein {
            ("vein", "Marrow")
        } else {
            ("seep", "Grub")
        };
        let chord = match self.harmony {
            0 => String::new(),
            h => format!(" (chord +{h}%)"),
        };
        match self.nodes {
            0 => format!("No {node} in reach"),
            1 => format!(
                "Tuning a {node}: about {} {what} a minute{chord}",
                self.per_minute
            ),
            n => format!(
                "Tuning {n} {node}s: about {} {what} a minute{chord}",
                self.per_minute
            ),
        }
    }
}

/// The Resonance meter's top (the simulation's `MAX_RESONANCE`).
pub const RESONANCE_FULL: u32 = 100;
/// The Honor meter's top (the simulation's `MAX_HONOR`).
pub const HONOR_FULL: u32 = 100;

/// A building that needs power, with none.
const UNPOWERED: &str = "Unpowered: stopped until a field of power reaches it";

/// Match time as "14:32", or "1:02:05" from the first hour.
pub(crate) fn clock(secs: u32) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// A faction meter's item: its points out of `full`, green once full.
fn meter_item(draw: &mut DrawList, x: i32, y: i32, label: &str, points: u32, full: u32) -> i32 {
    let color = if points >= full {
        theme::GOOD
    } else {
        theme::TEXT
    };
    top_item(draw, x, y, label, &format!("{points}/{full}"), color)
}

/// A label in grey and its value after it. Returns where the next item goes.
fn top_item(draw: &mut DrawList, x: i32, y: i32, label: &str, value: &str, color: Color) -> i32 {
    draw.text(x, y, 1, theme::MUTED, label);
    let vx = x + text_width(label, 1) + CELL_W;
    draw.text(vx, y, 1, color, value);
    vx + text_width(value, 1) + TOP_GAP
}

/// Draw the top bar `bar` (see [`HudLayout::top`](crate::hud::HudLayout::top)).
pub(crate) fn draw_top_bar(draw: &mut DrawList, bar: Rect, t: &TopBar) {
    draw.rect(
        bar,
        [theme::PANEL[0], theme::PANEL[1], theme::PANEL[2], 255],
    );
    draw.rect(Rect::new(0, bar.bottom() - 2, bar.w, 2), theme::PANEL_EDGE);
    let y = (bar.h - 2 - 16) / 2;
    let mut x = MARGIN;
    x = top_item(draw, x, y, "Grub", &thousands(t.grub), theme::TEXT);
    x = top_item(draw, x, y, "Marrow", &thousands(t.marrow), theme::TEXT);
    let supply = format!("{}/{}", t.supply_used, t.supply_cap);
    let supply_color = if t.supply_used >= t.supply_cap {
        theme::BAD
    } else {
        theme::TEXT
    };
    x = top_item(draw, x, y, "Supply", &supply, supply_color);
    if t.larva_room > 0 {
        let larvae = format!("{}/{}", t.larvae, t.larva_room);
        let color = if t.larvae == 0 {
            theme::WARN
        } else {
            theme::TEXT
        };
        x = top_item(draw, x, y, "Larvae", &larvae, color);
    }
    if t.resonance > 0 || t.chords > 0 {
        x = meter_item(draw, x, y, "Resonance", t.resonance, RESONANCE_FULL);
        if t.chords > 0 {
            x = top_item(draw, x, y, "Chords", &t.chords.to_string(), theme::TEXT);
        }
    }
    if let Some(honor) = t.honor {
        x = meter_item(draw, x, y, "Honor", honor, HONOR_FULL);
    }
    if t.upkeep_percent < 100 {
        top_item(
            draw,
            x,
            y,
            "Upkeep",
            &format!("{}%", t.upkeep_percent),
            theme::WARN,
        );
    }
    let time = clock(t.clock_secs);
    draw.text(
        bar.right() - MARGIN - text_width(&time, 1),
        y,
        1,
        theme::TEXT,
        &time,
    );
}

/// Buildings on the minimap `r`: filled footprints, a white rim when selected.
pub(crate) fn draw_blocks(
    draw: &mut DrawList,
    r: Rect,
    to_screen: impl Fn([f32; 2]) -> (i32, i32),
    blocks: &[Block],
) {
    let bounds = r.inset(1);
    for b in blocks {
        let (x0, y0) = to_screen([b.rect[0], b.rect[1]]);
        let (x1, y1) = to_screen([b.rect[0] + b.rect[2], b.rect[1] + b.rect[3]]);
        let body = Rect::new(x0, y0, (x1 - x0).max(BLOCK_MIN), (y1 - y0).max(BLOCK_MIN));
        if b.selected
            && let Some(rim) = body.inset(-1).intersect(bounds)
        {
            draw.rect(rim, theme::TEXT);
        }
        if let Some(fill) = body.intersect(bounds) {
            draw.rect(fill, rgb(b.color));
        }
    }
}

/// Gathering spots on the minimap `r`: small points, dim once run dry.
pub(crate) fn draw_nodes(
    draw: &mut DrawList,
    r: Rect,
    to_screen: impl Fn([f32; 2]) -> (i32, i32),
    nodes: &[NodeDot],
) {
    let bounds = r.inset(1);
    for n in nodes {
        let (x, y) = to_screen(n.pos);
        let color = node_color(n.marrow, n.dry);
        if let Some(dot) = Rect::new(x - 1, y - 1, 3, 3).intersect(bounds) {
            draw.rect(dot, rgb(color));
        }
    }
}

/// A gathering spot's colour, on the minimap and the field alike: gold
/// for Grub, bone white for Marrow, dimmed once it has run dry.
pub fn node_color(marrow: bool, dry: bool) -> [u8; 3] {
    let base = if marrow { MARROW_NODE } else { GRUB_NODE };
    if dry {
        base.map(|c| (u16::from(c) * 9 / 20) as u8)
    } else {
        base
    }
}

/// A selected building in the tray row `row`: its card, then the queue slots.
pub(crate) fn draw_structure(
    draw: &mut DrawList,
    press: &mut Press,
    row: Rect,
    input: HudInput,
    s: &StructurePanel,
    events: &mut Vec<HudEvent>,
) {
    let card = Rect::new(row.x, row.y, STRUCT_CARD_W.min(row.w / 2), row.h.min(126));
    draw_structure_card(draw, card, s);

    let left = card.right() + CARD_GAP;
    let gaps = (QUEUE_SLOTS as i32 - 1) * CARD_GAP;
    let slot_w = ((row.right() - left - gaps) / QUEUE_SLOTS as i32).min(QUEUE_SLOT_MAX_W);
    if slot_w < 24 {
        return;
    }
    let slot_h = slot_w.min(row.h);
    for i in 0..QUEUE_SLOTS {
        let rect = Rect::new(left + i as i32 * (slot_w + CARD_GAP), row.y, slot_w, slot_h);
        let Some(item) = s.queue.get(i) else {
            draw.rect(rect, theme::FIELD);
            draw.border(rect, 1, theme::PANEL_EDGE);
            continue;
        };
        let hot = rect.contains(input.mouse.0, input.mouse.1);
        let (clicked, _) = press.widget(
            WIDGET_QUEUE + i as u32,
            rect,
            input.mouse,
            input.left_pressed,
            input.left_released,
        );
        if clicked {
            events.push(HudEvent::Unqueue(i));
        }
        draw.rect(
            rect,
            if hot {
                theme::BUTTON_HOT
            } else {
                theme::BUTTON
            },
        );
        draw.border(rect, 1, theme::PANEL_EDGE);
        let inner = rect.inset(3);
        let chars = (inner.w / CELL_W).max(1) as usize;
        draw.text(inner.x, inner.y, 1, theme::TEXT, &fit(item.name, chars));
        if let Some(initial) = item.name.chars().next() {
            let mid = Rect::new(inner.x, inner.y + 18, inner.w, (inner.h - 18 - 8).max(0));
            draw.text_centered(mid, 2, theme::MUTED, &initial.to_string());
        }
        if i == 0 || item.progress > 0 {
            let bar = Rect::new(inner.x, inner.bottom() - 6, inner.w, 6);
            draw.meter(
                bar,
                f32::from(item.progress.min(100)) / 100.0,
                theme::ACCENT,
                theme::FIELD,
            );
        }
    }
}

/// The building's own card: portrait, name, hit points and what it is doing.
fn draw_structure_card(draw: &mut DrawList, r: Rect, s: &StructurePanel) {
    draw.rect(r, theme::BUTTON);
    draw.border(r, 1, theme::PANEL_EDGE);
    let inner = r.inset(3);
    let side = inner.h.min(56);
    let portrait = Rect::new(inner.x, inner.y, side, side);
    draw_portrait(draw, portrait, s.color, s.name, 2);
    let x = portrait.right() + 6;
    let chars = ((inner.right() - x) / CELL_W).max(1) as usize;
    draw.text(x, inner.y, 1, theme::TEXT, &fit(s.name, chars));
    let hp = s.hp as f32 / s.max_hp.max(1) as f32;
    draw.meter(
        Rect::new(x, inner.y + 20, (inner.right() - x).max(0), 8),
        hp,
        health_color(hp),
        theme::FIELD,
    );
    let mut numbers = format!("{}/{}", s.hp, s.max_hp);
    if let Some((held, cap)) = s.larvae {
        let _ = write!(numbers, "  Larvae {held}/{cap}");
    }
    draw.text(x, inner.y + 32, 1, theme::MUTED, &fit(&numbers, chars));

    let state = if let Some(done) = s.building {
        Some((format!("Building {}%", done.min(100)), theme::ACCENT))
    } else if let Some((to, done)) = s.upgrading {
        Some((
            format!("Upgrading to {to} {}%", done.min(100)),
            theme::ACCENT,
        ))
    } else if let Some(on) = s.rooted {
        Some(if on {
            ("On root-veins".to_string(), theme::GOOD)
        } else {
            ("Cut off: no regrowth".to_string(), theme::WARN)
        })
    } else {
        s.powered.map(|on| {
            if on {
                ("Powered".to_string(), theme::GOOD)
            } else {
                (UNPOWERED.to_string(), theme::WARN)
            }
        })
    };
    // What it is doing, then what it tunes (once it stands), each wrapped
    // to the card and cut off where the card ends.
    let tuning = s.tuning.filter(|_| s.building.is_none()).map(|t| {
        let color = if t.nodes == 0 {
            theme::MUTED
        } else {
            theme::TEXT
        };
        (t.line(), color)
    });
    let chars = (inner.w / CELL_W).max(1) as usize;
    let mut y = portrait.bottom() + 6;
    for (text, color) in state.into_iter().chain(tuning) {
        for line in wrap(&text, chars) {
            if y + CELL_H > inner.bottom() {
                return;
            }
            draw.text(inner.x, y, 1, color, &line);
            y += CELL_H;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tuning_building_says_what_it_tunes() {
        let line = |vein, nodes, per_minute| {
            Tuning {
                vein,
                nodes,
                per_minute,
                harmony: 0,
            }
            .line()
        };
        assert_eq!(line(false, 0, 0), "No seep in reach");
        assert_eq!(line(true, 0, 0), "No vein in reach");
        assert_eq!(
            line(false, 1, 200),
            "Tuning a seep: about 200 Grub a minute"
        );
        assert_eq!(line(true, 1, 50), "Tuning a vein: about 50 Marrow a minute");
        assert_eq!(
            line(false, 2, 400),
            "Tuning 2 seeps: about 400 Grub a minute"
        );
    }

    #[test]
    fn a_chord_adds_its_bonus_to_the_tuning_line() {
        let line = |nodes, harmony| {
            Tuning {
                vein: false,
                nodes,
                per_minute: 240,
                harmony,
            }
            .line()
        };
        assert_eq!(
            line(2, 35),
            "Tuning 2 seeps: about 240 Grub a minute (chord +35%)"
        );
        assert_eq!(
            line(1, 10),
            "Tuning a seep: about 240 Grub a minute (chord +10%)"
        );
        assert_eq!(line(2, 0), "Tuning 2 seeps: about 240 Grub a minute");
        // Nothing tuned, nothing to sing over.
        assert_eq!(line(0, 35), "No seep in reach");
    }

    #[test]
    fn long_lines_wrap_at_spaces_to_the_card() {
        let lines = wrap(UNPOWERED, 28);
        assert!(lines.len() >= 2 && lines.iter().all(|l| l.chars().count() <= 28));
        assert_eq!(lines.join(" "), UNPOWERED);
        assert_eq!(wrap("Powered", 28), ["Powered"]);
    }

    #[test]
    fn the_clock_shows_hours_only_when_there_are_some() {
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(65), "1:05");
        assert_eq!(clock(14 * 60 + 32), "14:32");
        assert_eq!(clock(3599), "59:59");
        assert_eq!(clock(3600 + 2 * 60 + 5), "1:02:05");
    }

    #[test]
    fn a_block_is_at_least_two_pixels_and_stays_on_the_minimap() {
        let r = Rect::new(10, 10, 100, 100);
        let to_screen = |p: [f32; 2]| (10 + p[0] as i32 * 100 / 256, 10 + p[1] as i32 * 100 / 256);
        let mut draw = DrawList::default();
        let tiny = Block {
            rect: [100.0, 100.0, 1.0, 1.0],
            color: [200, 60, 50],
            selected: false,
        };
        draw_blocks(&mut draw, r, to_screen, &[tiny]);
        assert_eq!(draw.quads.len(), 1);
        assert!(draw.quads[0].size.iter().all(|&s| s >= 2));
        // Off the minimap: nothing.
        let away = Block {
            rect: [900.0, 900.0, 4.0, 4.0],
            ..tiny
        };
        let mut draw = DrawList::default();
        draw_blocks(&mut draw, r, to_screen, &[away]);
        assert!(draw.quads.is_empty());
    }
}
