//! The Guide's Quick Start page: five short steps, each with a small picture,
//! that take a new player from the first workers to a won match.
//! Docs: docs/08-ui-ux.md#guide.
//!
//! There are no textures in the UI pass, so each picture is drawn from
//! rectangles and the HUD's pixel icons on a 160 by 88 grid scaled to fit.

use crate::conquest::{GRUB_NODE, MARROW_NODE};
use crate::draw::{CELL_H, CELL_W, Color, DrawList, Rect, fit, wrap};
use crate::hud::rgb;
use crate::icons::{self, Icon};
use crate::ui::theme;

/// One step of the page: a title and the line under it.
struct Step {
    title: &'static str,
    text: &'static str,
    /// Paints the step's picture.
    scene: fn(&mut Art),
}

const STEPS: [Step; 5] = [
    Step {
        title: "Gather",
        scene: gather,
        text: "Select your workers and right-click a gold Grub seep. They haul it back to your hall. Keep every worker busy.",
    },
    Step {
        title: "Build",
        scene: build,
        text: "Press B, pick a building, click the ground. Woods get cleared first (S, then drag, clears ground).",
    },
    Step {
        title: "Train",
        scene: train,
        text: "Select a finished building and press a button to hire regiments. Up to five wait in its queue.",
    },
    Step {
        title: "Command",
        scene: command,
        text: "Drag a box round regiments. Right-click ground to move, or an enemy to attack. Archers behind, spears in front.",
    },
    Step {
        title: "Win",
        scene: win,
        text: "Right-click enemy buildings to attack; smoke and fire show the damage. Break every enemy main building. Click i for tips.",
    },
];

/// Pixels between steps, and the picture's size at full height.
const GAP: i32 = 8;
const ART_W: i32 = 160;
const ART_H: i32 = 88;
/// Rows shorter than this skip their picture.
const MIN_ART_ROW: i32 = 60;

/// Draw the page into `area`.
pub(crate) fn draw(draw: &mut DrawList, area: Rect) {
    draw.boxed(area, theme::FIELD_CLOTH, 2);
    let inner = area.inset(10);
    draw.text_bold(inner.x, inner.y, 2, theme::MADDER, "Quick start");
    let top = inner.y + 2 * CELL_H + 6;
    let n = STEPS.len() as i32;
    let row_h = ((inner.bottom() - top - (n - 1) * GAP) / n).clamp(40, ART_H + 12);
    for (i, step) in STEPS.iter().enumerate() {
        let row = Rect::new(inner.x, top + i as i32 * (row_h + GAP), inner.w, row_h);
        if row.bottom() > inner.bottom() {
            break;
        }
        draw_step(draw, row, i, step);
    }
}

fn draw_step(d: &mut DrawList, row: Rect, i: usize, step: &Step) {
    d.boxed(row, theme::LINEN, 2);
    let mut x = row.x + 8;
    if row.h >= MIN_ART_ROW {
        let side = row.h - 12;
        let art = Rect::new(x, row.y + 6, side * ART_W / ART_H, side);
        scene(d, art, step.scene);
        x = art.right() + 14;
    }
    // A numbered stitch-red badge, then the title.
    let badge = Rect::new(x, row.y + 8, 24, 24);
    d.rect(badge, theme::MADDER);
    d.text_centered(badge, 1, theme::LINEN, &(i + 1).to_string());
    d.text_bold(badge.right() + 8, row.y + 12, 2, theme::INK, step.title);
    let chars = ((row.right() - x - 10) / CELL_W).max(1) as usize;
    let lines = wrap(step.text, chars);
    for (k, line) in lines.iter().enumerate() {
        let y = row.y + 12 + 2 * CELL_H + 4 + k as i32 * CELL_H;
        if y + CELL_H > row.bottom() - 2 {
            break;
        }
        d.text(x, y, 1, theme::INK, &fit(line, chars));
    }
}

/// A small coordinate frame: pieces are placed on a 160 by 88 grid.
struct Art<'a> {
    d: &'a mut DrawList,
    r: Rect,
}

impl Art<'_> {
    fn at(&self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        let sx = |v: i32| v * self.r.w / ART_W;
        let sy = |v: i32| v * self.r.h / ART_H;
        Rect::new(
            self.r.x + sx(x),
            self.r.y + sy(y),
            sx(w).max(1),
            sy(h).max(1),
        )
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        let r = self.at(x, y, w, h);
        self.d.rect(r, c);
    }

    /// A building: walls, a darker roof band and a door.
    fn hall(&mut self, x: i32, y: i32, w: i32, h: i32, wall: Color) {
        self.rect(x, y, w, h, theme::INK);
        self.rect(x + 2, y + 2, w - 4, h - 4, wall);
        self.rect(x + 2, y + 2, w - 4, h / 3, theme::INK_MUTED);
        self.rect(x + w / 2 - 4, y + h - 14, 8, 12, theme::INK);
    }

    /// A worker: a head over a body.
    fn worker(&mut self, x: i32, y: i32, c: Color) {
        self.rect(x + 1, y, 5, 5, theme::INK);
        self.rect(x, y + 5, 7, 8, c);
    }

    /// A regiment: a block of troops in rows.
    fn regiment(&mut self, x: i32, y: i32, cols: i32, rows: i32, c: Color) {
        for row in 0..rows {
            for col in 0..cols {
                self.rect(x + col * 8, y + row * 8, 6, 6, c);
            }
        }
    }

    /// A dashed line from `x0` to `x1` along `y`.
    fn dashes(&mut self, x0: i32, x1: i32, y: i32, c: Color) {
        let mut x = x0;
        while x < x1 {
            self.rect(x, y, 5, 2, c);
            x += 9;
        }
    }

    fn icon(&mut self, x: i32, y: i32, side: i32, i: Icon, c: Color) {
        let r = self.at(x, y, side, side);
        icons::icon(self.d, r, i, c);
    }
}

const FOE: Color = theme::MADDER;
const FRIEND: Color = theme::WOAD;

fn scene(d: &mut DrawList, r: Rect, paint: fn(&mut Art)) {
    d.rect(r, [188, 200, 140, 255]);
    d.border(r, 2, theme::INK);
    paint(&mut Art { d, r: r.inset(2) });
}

fn gather(a: &mut Art) {
    // A seep of gold, a hall, and workers walking between them.
    for (x, y) in [(10, 40), (20, 34), (18, 48), (28, 42)] {
        a.rect(x, y, 8, 8, rgb(GRUB_NODE));
        a.rect(x, y, 8, 2, [255, 235, 140, 255]);
    }
    a.rect(52, 60, 6, 6, rgb(MARROW_NODE));
    a.hall(110, 30, 40, 36, theme::LINEN_DK);
    for (x, y) in [(48, 42), (68, 50), (88, 44)] {
        a.worker(x, y, FRIEND);
    }
    a.dashes(36, 106, 70, theme::INK_MUTED);
    a.icon(112, 6, 20, Icon::Grub, theme::INK);
}

fn build(a: &mut Art) {
    a.worker(14, 50, FRIEND);
    // The B key.
    let key = a.at(12, 14, 22, 22);
    a.d.rect(key, theme::INK);
    a.d.rect(key.inset(2), theme::LINEN);
    a.d.text_centered(key, 1, theme::INK, "B");
    a.icon(46, 12, 22, Icon::Hammer, theme::INK);
    // A footprint being marked out, then the foundation rising in it.
    a.rect(84, 36, 62, 2, theme::INK);
    a.rect(84, 36, 2, 40, theme::INK);
    a.rect(144, 36, 2, 40, theme::INK);
    a.rect(84, 74, 62, 2, theme::INK);
    a.hall(92, 46, 46, 28, theme::CLOTH);
    a.rect(92, 62, 46, 12, theme::LINEN_DK);
    a.dashes(36, 80, 60, theme::INK_MUTED);
    a.icon(120, 8, 20, Icon::Supply, theme::INK);
}

fn train(a: &mut Art) {
    a.hall(8, 22, 44, 40, theme::LINEN_DK);
    // The queue: the front slot filling, the rest waiting.
    for k in 0..5 {
        let x = 62 + k * 18;
        a.rect(x, 24, 16, 16, theme::INK);
        a.rect(
            x + 2,
            26,
            12,
            12,
            if k == 0 { theme::WELD } else { theme::LINEN },
        );
    }
    a.rect(62, 44, 16, 4, theme::INK);
    a.rect(63, 45, 9, 2, theme::WODE);
    // Hired regiments march out.
    a.regiment(66, 58, 3, 2, FRIEND);
    a.regiment(104, 58, 3, 2, FRIEND);
    a.icon(12, 66, 14, Icon::Sword, theme::INK);
}

fn command(a: &mut Art) {
    // A selection box round our regiment, an arrow of dashes to the enemy.
    a.regiment(12, 40, 4, 3, FRIEND);
    let sel = a.at(8, 36, 44, 34);
    a.d.border(sel, 1, theme::WODE);
    a.dashes(56, 100, 52, theme::INK_MUTED);
    a.regiment(112, 38, 4, 3, FOE);
    a.icon(76, 24, 14, Icon::Attack, theme::INK);
    a.icon(6, 6, 14, Icon::Move, theme::INK);
    a.icon(24, 6, 14, Icon::Bow, theme::INK);
    a.icon(42, 6, 14, Icon::Spear, theme::INK);
}

fn win(a: &mut Art) {
    // The enemy's hall comes down; ours stands with its info button.
    a.hall(100, 26, 46, 40, FOE);
    a.rect(104, 20, 4, 8, theme::INK);
    a.rect(124, 18, 8, 10, theme::INK);
    a.icon(100, 6, 14, Icon::Flag, theme::INK);
    a.hall(10, 36, 40, 34, FRIEND);
    let info = a.at(34, 30, 16, 16);
    a.d.rect(info, theme::INK);
    a.d.rect(info.inset(1), theme::WELD);
    icons::icon(a.d, info.inset(2), Icon::Info, theme::INK);
    a.dashes(58, 96, 52, theme::INK_MUTED);
    a.regiment(60, 56, 3, 2, FRIEND);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_is_short_plain_ascii() {
        for s in &STEPS {
            assert!(s.text.len() <= 130, "{} is wordy", s.title);
            assert!(s.text.is_ascii() && s.title.is_ascii());
        }
    }

    #[test]
    fn the_page_stays_in_its_area_at_the_guide_size_and_a_small_one() {
        for area in [Rect::new(300, 150, 900, 560), Rect::new(10, 10, 300, 200)] {
            let mut d = DrawList::default();
            draw(&mut d, area);
            assert!(!d.quads.is_empty());
            for q in &d.quads {
                let r = Rect::new(
                    i32::from(q.pos[0]),
                    i32::from(q.pos[1]),
                    i32::from(q.size[0]),
                    i32::from(q.size[1]),
                );
                assert!(area.intersect(r) == Some(r), "{r:?} leaves {area:?}");
            }
        }
    }
}
