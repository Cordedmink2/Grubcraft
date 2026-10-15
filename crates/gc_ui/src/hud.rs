//! The in-battle HUD: the bottom bar with the minimap and the army tray.
//! Docs: docs/08-ui-ux.md#screen-layout-1080p-reference.
//!
//! Like the menus this knows nothing about the GPU or the simulation. Each
//! frame the game describes what to show ([`HudData`]) and what the mouse
//! did ([`HudInput`]); [`Hud::frame`] draws into a [`DrawList`] and returns
//! what the player asked for ([`HudEvent`]), which the game turns into
//! camera moves, selections and orders. Map positions are in tiles, with
//! north (smaller z) at the top of the minimap.

use crate::draw::{Color, DrawList, Rect, fit, text_width};
use crate::ui::theme;

/// Height of the bottom bar, in UI pixels.
pub const BAR_HEIGHT: i32 = 168;
const MARGIN: i32 = 8;
/// Width kept free at the bar's right end for the command grid.
const GRID_WIDTH: i32 = 212;
const CARD_MIN_W: i32 = 48;
const CARD_MAX_W: i32 = 84;
const CARD_GAP: i32 = 4;
/// Two clicks on a card within this long are a double-click.
const DOUBLE_CLICK_MS: u64 = 400;

/// Where the pieces of the HUD sit on a screen of a given size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudLayout {
    pub bar: Rect,
    pub minimap: Rect,
    pub tray: Rect,
    /// Reserved for the command grid.
    pub grid: Rect,
}

impl HudLayout {
    pub fn new(screen: (i32, i32)) -> HudLayout {
        let (w, h) = screen;
        let bar = Rect::new(0, (h - BAR_HEIGHT).max(0), w, BAR_HEIGHT.min(h));
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

pub struct HudData<'a> {
    pub minimap: MinimapData<'a>,
    pub cards: &'a [Card],
    /// Shown above the cards, e.g. "Selected" or "Group 2".
    pub tray_title: &'a str,
}

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
}

#[derive(Default)]
pub struct Hud {
    /// Dragging across the minimap with the left button.
    dragging: bool,
    last_card_click: Option<(u32, u64)>,
}

fn rgb(c: [u8; 3]) -> Color {
    [c[0], c[1], c[2], 255]
}

impl Hud {
    /// Whether a click at `mouse` belongs to the HUD rather than the world.
    pub fn wants(&self, screen: (i32, i32), mouse: (i32, i32)) -> bool {
        self.dragging || HudLayout::new(screen).bar.contains(mouse.0, mouse.1)
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

        let side = m.terrain.side;
        let to_px = |i: usize| (i as i32 * r.w) / side.max(1) as i32;
        if side == 0 || m.terrain.colors.len() < side * side {
            draw.rect(r, theme::FIELD);
        } else {
            for row in 0..side {
                // Merge runs of one colour into a single rectangle.
                let mut col = 0;
                while col < side {
                    let c = m.terrain.colors[row * side + col];
                    let mut end = col + 1;
                    while end < side && m.terrain.colors[row * side + end] == c {
                        end += 1;
                    }
                    let (x0, x1) = (r.x + to_px(col), r.x + to_px(end));
                    let (y0, y1) = (r.y + to_px(row), r.y + to_px(row + 1));
                    draw.rect(Rect::new(x0, y0, x1 - x0, (y1 - y0).max(1)), c);
                    col = end;
                }
            }
        }

        let to_screen = |p: [f32; 2]| -> (i32, i32) {
            (
                r.x + (p[0] / m.map_tiles * r.w as f32) as i32,
                r.y + (p[1] / m.map_tiles * r.h as f32) as i32,
            )
        };
        let inside = |(x, y): (i32, i32)| r.inset(1).contains(x, y);
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
            if hot && input.left_pressed {
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
    draw.rect(portrait, rgb(c.color));
    if let Some(initial) = c.name.chars().next() {
        let tint = if luma(c.color) > 140 {
            [20, 22, 28, 255]
        } else {
            theme::TEXT
        };
        draw.text_centered(portrait, 2, tint, &initial.to_string());
    }
    if c.engaged {
        draw.border(portrait, 2, theme::BAD);
    }
    let chars = (inner.w / 8).max(1) as usize;
    let mut y = portrait.bottom() + 3;
    draw.text(inner.x, y, 1, theme::TEXT, &fit(&c.name, chars));
    y += 18;

    // Model count, then morale.
    let bar = |draw: &mut DrawList, y: i32, frac: f32, color: Color| {
        let track = Rect::new(inner.x, y, inner.w, 6);
        draw.rect(track, theme::FIELD);
        let w = (track.w as f32 * frac.clamp(0.0, 1.0)).round() as i32;
        let w = if frac > 0.0 { w.max(1) } else { 0 };
        draw.rect(Rect::new(track.x, track.y, w, track.h), color);
    };
    let health = c.models as f32 / c.max_models.max(1) as f32;
    let health_color = if health < 0.35 {
        theme::BAD
    } else {
        theme::GOOD
    };
    bar(draw, y, health, health_color);
    y += 9;
    let morale_color = match c.state {
        CardState::Steady => [96, 156, 230, 255],
        CardState::Wavering => [236, 190, 60, 255],
        CardState::Routing => theme::BAD,
    };
    bar(draw, y, c.morale as f32 / 100.0, morale_color);
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

fn luma(c: [u8; 3]) -> u32 {
    (c[0] as u32 * 3 + c[1] as u32 * 6 + c[2] as u32) / 10
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

    fn run(
        hud: &mut Hud,
        input: HudInput,
        cards: &[Card],
        dots: &[Dot],
    ) -> (Vec<HudEvent>, DrawList) {
        let t = terrain();
        let data = HudData {
            minimap: MinimapData {
                map_tiles: 256.0,
                terrain: &t,
                dots,
                view: &[],
            },
            cards,
            tray_title: "Selected",
        };
        let mut draw = DrawList::default();
        let mut events = Vec::new();
        hud.frame(&mut draw, SCREEN, input, &data, &mut events);
        (events, draw)
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
    fn empty_tray_says_so() {
        let mut hud = Hud::default();
        let (_, draw) = run(&mut hud, HudInput::default(), &[], &[]);
        assert!(draw.quads.len() > 10);
    }
}
