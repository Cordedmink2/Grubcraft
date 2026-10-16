//! Edge alerts and the idle-worker button, drawn over the battlefield above
//! the bottom bar. Docs: docs/08-ui-ux.md#screen-layout-1080p-reference.
//!
//! The game says where on the screen each event is (it may be far outside the
//! window) and this draws a small badge at the nearest edge, with an arrow
//! pointing outwards. Events that are in view get no badge.

use crate::draw::{DrawList, Rect};
use crate::hud::{HudEvent, HudInput, WIDGET_ALERT, WIDGET_IDLE};
use crate::ui::{Press, theme};

/// Side of an alert badge, in UI pixels.
const BADGE: i32 = 28;
/// Room between a badge and the screen edge, for the arrow.
const EDGE: i32 = 10;
/// How far the arrow's tip sticks out of the badge.
const ARROW: i32 = 4;
const IDLE_W: i32 = 112;
const IDLE_H: i32 = 28;
const MARGIN: i32 = 8;
/// A pale lilac, for regiments that broke.
const ROUT: [u8; 4] = [214, 206, 240, 255];

/// What happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertKind {
    /// One of your regiments is taking losses.
    Attack,
    /// An ambush was sprung on one of your regiments.
    Ambush,
    /// One of your regiments broke and ran.
    Rout,
}

impl AlertKind {
    fn letter(self) -> &'static str {
        match self {
            AlertKind::Attack => "!",
            AlertKind::Ambush => "A",
            AlertKind::Rout => "R",
        }
    }

    fn color(self) -> [u8; 4] {
        match self {
            AlertKind::Attack => theme::BAD,
            AlertKind::Ambush => theme::ACCENT,
            AlertKind::Rout => ROUT,
        }
    }
}

/// One event to point at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alert {
    /// Comes back in [`HudEvent::Alert`] when the badge is clicked.
    pub id: u32,
    pub kind: AlertKind,
    /// Where the event is on the screen, in UI pixels. May be outside it.
    pub at: [f32; 2],
    /// Just happened: the badge is drawn filled so it catches the eye.
    pub fresh: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Up,
    Down,
}

/// Where the badge for an event at `at` sits on a `screen` whose bottom
/// `bar_height` is the bar, and which edge it is on. `None` while the event
/// is in view.
fn badge_for(at: [f32; 2], screen: (i32, i32), bar_height: i32) -> Option<(Rect, Side)> {
    let (w, h) = (screen.0 as f32, (screen.1 - bar_height) as f32);
    let reach = (BADGE / 2 + EDGE) as f32;
    let (hw, hh) = (w / 2.0 - reach, h / 2.0 - reach);
    if hw <= 0.0 || hh <= 0.0 {
        return None;
    }
    let (dx, dy) = (at[0] - w / 2.0, at[1] - h / 2.0);
    if dx.abs() <= hw && dy.abs() <= hh {
        return None;
    }
    // Scale the offset from the middle back until it touches the border.
    let (sx, sy) = (hw / dx.abs().max(1e-3), hh / dy.abs().max(1e-3));
    let s = sx.min(sy);
    let side = match (sx < sy, dx > 0.0, dy > 0.0) {
        (true, true, _) => Side::Right,
        (true, false, _) => Side::Left,
        (false, _, true) => Side::Down,
        (false, _, false) => Side::Up,
    };
    let (cx, cy) = (w / 2.0 + dx * s, h / 2.0 + dy * s);
    let r = Rect::new(cx as i32 - BADGE / 2, cy as i32 - BADGE / 2, BADGE, BADGE);
    Some((r, side))
}

/// A triangle just outside `r` on `side`, pointing away from it.
fn arrow(draw: &mut DrawList, r: Rect, side: Side, color: [u8; 4]) {
    let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
    for k in 0..ARROW {
        let e = ARROW - 1 - k;
        let (a, b) = (2 * k, 2 * e + 1);
        let piece = match side {
            Side::Right => Rect::new(r.right() + a, cy - e, 2, b),
            Side::Left => Rect::new(r.x - a - 2, cy - e, 2, b),
            Side::Down => Rect::new(cx - e, r.bottom() + a, b, 2),
            Side::Up => Rect::new(cx - e, r.y - a - 2, b, 2),
        };
        draw.rect(piece, color);
    }
}

/// What to point at and what to offer, each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct AlertData<'a> {
    pub alerts: &'a [Alert],
    /// How many workers stand idle; `None` hides the button (no workers).
    pub idle_workers: Option<u32>,
}

/// Where the idle-worker button sits.
fn idle_button(screen: (i32, i32), bar_height: i32) -> Rect {
    Rect::new(
        screen.0 - MARGIN - IDLE_W,
        screen.1 - bar_height - MARGIN - IDLE_H,
        IDLE_W,
        IDLE_H,
    )
}

/// Draw the alert badges and the idle-worker button, report clicks, and note
/// in `over` every rectangle that now takes the mouse from the battlefield.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    draw: &mut DrawList,
    screen: (i32, i32),
    bar_height: i32,
    input: HudInput,
    press: &mut Press,
    data: AlertData,
    over: &mut Vec<Rect>,
    events: &mut Vec<HudEvent>,
) {
    // Badges can overlap; the one drawn last is on top and takes the click.
    let mut clicked = None;
    for a in data.alerts {
        let Some((r, side)) = badge_for(a.at, screen, bar_height) else {
            continue;
        };
        let hot = r.contains(input.mouse.0, input.mouse.1);
        let (hit, _) = press.widget(
            WIDGET_ALERT + (a.id & 0xFF_FFFF),
            r,
            input.mouse,
            input.left_pressed,
            input.left_released,
        );
        if hit {
            clicked = Some(a.id);
        }
        let color = a.kind.color();
        let filled = a.fresh || hot;
        draw.rect(r, if filled { color } else { theme::PANEL });
        draw.border(r, 2, color);
        let ink = if filled { theme::FIELD } else { color };
        draw.text_centered(r, 1, ink, a.kind.letter());
        arrow(draw, r, side, color);
        over.push(r);
    }
    events.extend(clicked.map(HudEvent::Alert));
    if let Some(n) = data.idle_workers {
        let r = idle_button(screen, bar_height);
        let hot = n > 0 && r.contains(input.mouse.0, input.mouse.1);
        let (hit, _) = press.widget(
            WIDGET_IDLE,
            r,
            input.mouse,
            n > 0 && input.left_pressed,
            input.left_released,
        );
        if hit && n > 0 {
            events.push(HudEvent::IdleWorker);
        }
        draw.rect(
            r,
            if hot {
                theme::BUTTON_HOT
            } else {
                theme::BUTTON
            },
        );
        draw.border(r, 1, theme::PANEL_EDGE);
        let ink = if n > 0 { theme::TEXT } else { theme::DISABLED };
        draw.text_centered(r, 1, ink, &format!("F1 Idle {n}"));
        over.push(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Press;

    const SCREEN: (i32, i32) = (1280, 760);
    const BAR: i32 = 168;

    fn alert(at: [f32; 2]) -> Alert {
        Alert {
            id: 7,
            kind: AlertKind::Attack,
            at,
            fresh: true,
        }
    }

    fn frame(alerts: &[Alert], idle: Option<u32>, input: HudInput) -> (Vec<HudEvent>, Vec<Rect>) {
        let (mut d, mut over, mut events) = (DrawList::default(), Vec::new(), Vec::new());
        let data = AlertData {
            alerts,
            idle_workers: idle,
        };
        let mut press = Press::default();
        draw(
            &mut d,
            SCREEN,
            BAR,
            input,
            &mut press,
            data,
            &mut over,
            &mut events,
        );
        (events, over)
    }

    #[test]
    fn events_in_view_get_no_badge() {
        assert!(badge_for([640.0, 300.0], SCREEN, BAR).is_none());
        assert!(badge_for([100.0, 100.0], SCREEN, BAR).is_none());
        let (_, over) = frame(&[alert([640.0, 300.0])], None, HudInput::default());
        assert!(over.is_empty());
    }

    #[test]
    fn a_badge_sits_on_the_edge_the_event_is_past() {
        let area_h = SCREEN.1 - BAR;
        let sides = [
            ([-500.0, 250.0], Side::Left),
            ([2000.0, 250.0], Side::Right),
            ([600.0, -400.0], Side::Up),
            ([600.0, 900.0], Side::Down),
        ];
        for (at, want) in sides {
            let (r, side) = badge_for(at, SCREEN, BAR).unwrap();
            assert_eq!(side, want, "{at:?}");
            // Whole badge on the playing field, arrow room to spare.
            assert!(r.x >= EDGE - 1 && r.right() <= SCREEN.0 - EDGE + 1, "{r:?}");
            assert!(r.y >= EDGE - 1 && r.bottom() <= area_h - EDGE + 1, "{r:?}");
        }
    }

    #[test]
    fn a_corner_event_picks_the_nearer_wall() {
        // Far to the right and only a little above: right edge, near the top.
        let (r, side) = badge_for([3000.0, -50.0], SCREEN, BAR).unwrap();
        assert_eq!(side, Side::Right);
        assert!(r.bottom() < (SCREEN.1 - BAR) / 2, "above the middle: {r:?}");
    }

    #[test]
    fn clicking_a_badge_reports_it_and_keeps_the_click() {
        let (r, _) = badge_for([-500.0, 250.0], SCREEN, BAR).unwrap();
        let click = HudInput {
            mouse: (r.x + 3, r.y + 3),
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        };
        let (events, over) = frame(&[alert([-500.0, 250.0])], None, click);
        assert_eq!(events, vec![HudEvent::Alert(7)]);
        assert!(
            over.iter()
                .any(|o| o.contains(click.mouse.0, click.mouse.1))
        );
        // A click elsewhere is not the badge's.
        let (events, _) = frame(
            &[alert([-500.0, 250.0])],
            None,
            HudInput {
                mouse: (640, 300),
                left_pressed: true,
                left_released: true,
                ..HudInput::default()
            },
        );
        assert!(events.is_empty());
    }

    #[test]
    fn a_click_on_stacked_badges_goes_to_the_top_one() {
        let (r, _) = badge_for([-500.0, 250.0], SCREEN, BAR).unwrap();
        let mut under = alert([-500.0, 250.0]);
        under.id = 1;
        let mut over = alert([-500.0, 250.0]);
        over.id = 2;
        let click = HudInput {
            mouse: (r.x + 3, r.y + 3),
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        };
        assert_eq!(
            frame(&[under, over], None, click).0,
            vec![HudEvent::Alert(2)]
        );
    }

    #[test]
    fn the_idle_worker_button_needs_an_idle_worker() {
        let r = idle_button(SCREEN, BAR);
        assert!(r.bottom() <= SCREEN.1 - BAR && r.right() <= SCREEN.0);
        let click = HudInput {
            mouse: (r.x + 5, r.y + 5),
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        };
        assert_eq!(frame(&[], Some(3), click).0, vec![HudEvent::IdleWorker]);
        let (events, over) = frame(&[], Some(0), click);
        assert!(events.is_empty(), "nothing to select");
        assert_eq!(over, vec![r], "still takes the click");
        assert!(
            frame(&[], None, click).1.is_empty(),
            "hidden without workers"
        );
    }
}
