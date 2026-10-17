//! Faction power buttons (Conquest): a short row over the battlefield, just
//! under the top bar's faction meter (Resonance or Honor). Docs: docs/08-ui-ux.md#screen-layout-1080p-reference.
//!
//! The game says what each button is called, what its second line says
//! (the price, or why it can't be used now) and whether it can be pressed;
//! this draws the row and reports which one was clicked. A button that is
//! [`SlotState::Blocked`] is dim but still clicks, so the game can say why.

use crate::draw::{CELL_W, DrawList, Rect, fit};
use crate::hud::{HudEvent, HudInput, SlotState, WIDGET_POWER, muted_ink, slot_colors};
use crate::ui::{Press, theme};

/// The key on each button, first to last (the game binds the same keys).
pub const POWER_KEYS: [&str; 4] = ["F2", "F3", "F4", "F5"];
/// Wide enough for "Banner of the Ancestors" beside its key.
const BUTTON_W: i32 = 216;
const BUTTON_H: i32 = 38;
const GAP: i32 = 4;
const MARGIN: i32 = 8;
/// The row starts this far in from the screen's left edge, clear of an
/// alert badge on it.
const INDENT: i32 = 40;

/// One power's button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PowerButton<'a> {
    pub name: &'a str,
    /// The second line: its price, or why it can't be used now.
    pub detail: &'a str,
    pub state: SlotState,
}

/// Where button `i` sits: a row from the `field`'s top-left corner.
fn button_rect(field: Rect, i: usize) -> Rect {
    Rect::new(
        field.x + INDENT + i as i32 * (BUTTON_W + GAP),
        field.y + MARGIN,
        BUTTON_W,
        BUTTON_H,
    )
}

/// Draw the buttons (at most [`POWER_KEYS`]' length), report a click as
/// [`HudEvent::Power`], and note in `over` each rectangle that now takes
/// the mouse from the battlefield. `field` is the screen between the bars.
pub(crate) fn draw(
    draw: &mut DrawList,
    field: Rect,
    input: HudInput,
    press: &mut Press,
    buttons: &[PowerButton],
    over: &mut Vec<Rect>,
    events: &mut Vec<HudEvent>,
) {
    for (i, b) in buttons.iter().take(POWER_KEYS.len()).enumerate() {
        let r = button_rect(field, i);
        let hot = b.state == SlotState::Ready && r.contains(input.mouse.0, input.mouse.1);
        let (clicked, _) = press.widget(
            WIDGET_POWER + i as u32,
            r,
            input.mouse,
            b.state != SlotState::Off && input.left_pressed,
            input.left_released,
        );
        if clicked && b.state != SlotState::Off {
            events.push(HudEvent::Power(i));
        }
        let (back, edge, text) = slot_colors(b.state, hot);
        draw.boxed(r, back, 2);
        let _ = edge;
        let inner = r.inset(4);
        let key = POWER_KEYS[i];
        let key_color = if b.state == SlotState::Off || b.state == SlotState::Blocked {
            muted_ink()
        } else {
            theme::MADDER
        };
        draw.text(inner.x, inner.y, 1, key_color, key);
        let x = inner.x + (key.len() as i32 + 1) * CELL_W;
        let chars = ((inner.right() - x) / CELL_W).max(1) as usize;
        draw.text(x, inner.y, 1, text, &fit(b.name, chars));
        let chars = (inner.w / CELL_W).max(1) as usize;
        draw.text(inner.x, inner.y + 16, 1, muted_ink(), &fit(b.detail, chars));
        over.push(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIELD: Rect = Rect::new(0, 26, 1280, 566);

    fn click_at(r: Rect) -> HudInput {
        HudInput {
            mouse: (r.x + 5, r.y + 5),
            left_pressed: true,
            left_released: true,
            ..HudInput::default()
        }
    }

    fn frame(buttons: &[PowerButton], input: HudInput) -> (Vec<HudEvent>, Vec<Rect>, DrawList) {
        let (mut d, mut over, mut events) = (DrawList::default(), Vec::new(), Vec::new());
        let mut press = Press::default();
        draw(
            &mut d,
            FIELD,
            input,
            &mut press,
            buttons,
            &mut over,
            &mut events,
        );
        (events, over, d)
    }

    fn button(state: SlotState) -> PowerButton<'static> {
        PowerButton {
            name: "Overclock",
            detail: "20 Resonance",
            state,
        }
    }

    #[test]
    fn buttons_sit_in_a_row_under_the_top_bar() {
        let (a, b) = (button_rect(FIELD, 0), button_rect(FIELD, 1));
        assert!(a.y >= FIELD.y && a.y == b.y);
        assert!(a.right() < b.x, "side by side with a gap");
        assert!(a.x > 0 && b.right() < FIELD.right());
    }

    #[test]
    fn a_ready_or_blocked_button_clicks_and_an_off_one_does_not() {
        let buttons = [
            button(SlotState::Ready),
            button(SlotState::Blocked),
            button(SlotState::Off),
        ];
        for (i, want) in [
            (0, vec![HudEvent::Power(0)]),
            (1, vec![HudEvent::Power(1)]),
            (2, vec![]),
        ] {
            let (events, _, _) = frame(&buttons, click_at(button_rect(FIELD, i)));
            assert_eq!(events, want, "button {i}");
        }
        // Not on the press alone.
        let mut press_only = click_at(button_rect(FIELD, 0));
        press_only.left_released = false;
        assert!(frame(&buttons, press_only).0.is_empty());
    }

    #[test]
    fn the_buttons_take_the_mouse_from_the_battlefield() {
        let buttons = [button(SlotState::Ready), button(SlotState::Ready)];
        let (_, over, _) = frame(&buttons, HudInput::default());
        assert_eq!(over, [button_rect(FIELD, 0), button_rect(FIELD, 1)]);
        assert!(frame(&[], HudInput::default()).1.is_empty());
    }

    #[test]
    fn a_button_says_its_key_name_and_second_line_and_armed_stands_out() {
        let ready = frame(&[button(SlotState::Ready)], HudInput::default()).2;
        let armed = frame(&[button(SlotState::Armed)], HudInput::default()).2;
        let blocked = frame(&[button(SlotState::Blocked)], HudInput::default()).2;
        // Back, four border sides, and a quad a letter of "F2", "Overclock"
        // and "20 Resonance".
        let letters = "F2Overclock20Resonance".len();
        assert!(ready.quads.len() > letters, "{}", ready.quads.len());
        let has = |d: &DrawList, c| d.quads.iter().any(|q| q.color == c);
        assert!(has(&armed, theme::WELD) && !has(&ready, theme::WELD));
        let dim = slot_colors(SlotState::Blocked, false).2;
        assert!(has(&blocked, dim) && !has(&ready, dim));
    }

    #[test]
    fn the_longest_power_name_fits_its_button_whole() {
        let long = PowerButton {
            name: "Banner of the Ancestors",
            detail: "Needs 35 Honor",
            state: SlotState::Ready,
        };
        let (a, b) = (
            frame(&[long], HudInput::default()).2,
            frame(&[PowerButton { name: "", ..long }], HudInput::default()).2,
        );
        assert_eq!(
            a.quads.len() - b.quads.len(),
            "Banner of the Ancestors"
                .chars()
                .filter(|c| *c != ' ')
                .count()
        );
    }

    #[test]
    fn only_as_many_buttons_as_there_are_keys_are_drawn() {
        let many = [button(SlotState::Ready); 6];
        let (_, over, _) = frame(&many, HudInput::default());
        assert_eq!(over.len(), POWER_KEYS.len());
    }
}
