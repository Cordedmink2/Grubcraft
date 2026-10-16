//! Immediate-mode widgets: call `button(..)` each frame, get back whether it
//! was clicked. Widget identity is the order of calls within a frame, so a
//! screen must lay out its widgets the same way every frame: a list whose
//! length changes under the player (games found on the network) calls
//! `reserve_ids` for the rows it does not draw.

use crate::draw::{Color, DrawList, Rect, fit, text_width};

pub mod theme {
    use crate::draw::Color;

    pub const SCRIM: Color = [6, 8, 12, 190];
    pub const PANEL: Color = [22, 26, 34, 240];
    pub const PANEL_EDGE: Color = [70, 78, 94, 255];
    pub const TEXT: Color = [236, 238, 242, 255];
    pub const MUTED: Color = [150, 158, 172, 255];
    pub const DISABLED: Color = [96, 102, 114, 255];
    pub const ACCENT: Color = [232, 132, 50, 255];
    pub const GOOD: Color = [110, 200, 120, 255];
    pub const BAD: Color = [240, 96, 84, 255];
    pub const BUTTON: Color = [44, 52, 68, 255];
    pub const BUTTON_HOT: Color = [64, 74, 96, 255];
    pub const BUTTON_DOWN: Color = [34, 40, 54, 255];
    pub const PRIMARY: Color = [190, 92, 30, 255];
    pub const PRIMARY_HOT: Color = [222, 118, 46, 255];
    pub const PRIMARY_DOWN: Color = [150, 70, 22, 255];
    pub const FIELD: Color = [12, 14, 20, 255];
}

#[derive(Clone, Debug, Default)]
pub struct UiInput {
    /// Mouse position in UI pixels.
    pub mouse: (i32, i32),
    /// Left button went down / came up since the last frame.
    pub pressed: bool,
    pub released: bool,
    /// Characters typed since the last frame.
    pub typed: String,
    pub backspace: bool,
    pub enter: bool,
    pub escape: bool,
}

impl UiInput {
    /// Forget one-shot events; keep the mouse position.
    pub fn clear_events(&mut self) {
        self.pressed = false;
        self.released = false;
        self.typed.clear();
        self.backspace = false;
        self.enter = false;
        self.escape = false;
    }
}

/// The rule for a click, shared by the menu widgets and the HUD: pressing
/// on a widget arms it, and releasing on that same widget is the click.
/// Releasing anywhere else cancels it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Press {
    armed: Option<u32>,
}

impl Press {
    /// Widget `id` over `r`, given the mouse and this frame's button events.
    /// Returns (clicked, held down). Widgets drawn later win a press they
    /// overlap, so the top one takes the click.
    pub fn widget(
        &mut self,
        id: u32,
        r: Rect,
        mouse: (i32, i32),
        pressed: bool,
        released: bool,
    ) -> (bool, bool) {
        let hot = r.contains(mouse.0, mouse.1);
        if hot && pressed {
            self.armed = Some(id);
        }
        let held = self.armed == Some(id) && hot;
        (released && held, held)
    }

    /// Call once per frame, after every widget: a release ends any press.
    pub fn end_frame(&mut self, released: bool) {
        if released {
            self.armed = None;
        }
    }

    fn arm(&mut self, id: u32) {
        self.armed = Some(id);
    }

    fn is(&self, id: u32) -> bool {
        self.armed == Some(id)
    }

    fn clear(&mut self) {
        self.armed = None;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonStyle {
    Normal,
    /// The one thing to do on this screen; Enter presses it.
    Primary,
}

pub struct Ui {
    pub draw: DrawList,
    pub input: UiInput,
    /// Screen size in UI pixels.
    pub screen: (i32, i32),
    active: Press,
    focus: Option<u32>,
    next_id: u32,
    enter_taken: bool,
    /// A text field drew this frame with the focus.
    focus_claimed: bool,
}

impl Default for Ui {
    fn default() -> Self {
        Ui::new()
    }
}

impl Ui {
    pub fn new() -> Ui {
        Ui {
            draw: DrawList::default(),
            input: UiInput::default(),
            screen: (1280, 760),
            active: Press::default(),
            focus: None,
            next_id: 0,
            enter_taken: false,
            focus_claimed: false,
        }
    }

    pub fn begin_frame(&mut self, input: UiInput, screen: (i32, i32)) {
        self.input = input;
        self.screen = screen;
        self.draw.clear();
        self.next_id = 0;
        self.enter_taken = false;
        self.focus_claimed = false;
    }

    pub fn end_frame(&mut self) {
        self.active.end_frame(self.input.released);
        // The field that had the focus is gone (the screen changed under
        // it): don't leave typing and Enter pointed at nothing.
        if !self.focus_claimed {
            self.focus = None;
        }
    }

    /// Drop keyboard focus and any half-finished click, e.g. on a new screen.
    pub fn reset(&mut self) {
        self.active.clear();
        self.focus = None;
    }

    pub fn text_focused(&self) -> bool {
        self.focus.is_some()
    }

    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// Skip `n` ids, so widgets after a list that can be shorter than its
    /// maximum keep the ids they have when it is full.
    pub fn reserve_ids(&mut self, n: usize) {
        self.next_id += n as u32;
    }

    fn hovered(&self, r: Rect) -> bool {
        r.contains(self.input.mouse.0, self.input.mouse.1)
    }

    /// Press and release handling shared by clickable widgets.
    fn click(&mut self, id: u32, r: Rect) -> (bool, bool) {
        let (mouse, pressed, released) =
            (self.input.mouse, self.input.pressed, self.input.released);
        if self.hovered(r) && pressed {
            self.focus = None;
        }
        self.active.widget(id, r, mouse, pressed, released)
    }

    pub fn panel(&mut self, r: Rect) {
        self.draw.rect(r, theme::PANEL);
        self.draw.border(r, 2, theme::PANEL_EDGE);
    }

    pub fn label(&mut self, x: i32, y: i32, scale: i32, color: Color, text: &str) {
        self.draw.text(x, y, scale, color, text);
    }

    pub fn label_centered(&mut self, r: Rect, scale: i32, color: Color, text: &str) {
        self.draw.text_centered(r, scale, color, text);
    }

    /// Text right-aligned to `right`.
    pub fn label_right(&mut self, right: i32, y: i32, scale: i32, color: Color, text: &str) {
        self.draw
            .text(right - text_width(text, scale), y, scale, color, text);
    }

    pub fn button(&mut self, r: Rect, label: &str, style: ButtonStyle, enabled: bool) -> bool {
        let id = self.id();
        let (clicked, down) = if enabled {
            self.click(id, r)
        } else {
            (false, false)
        };
        let hot = enabled && self.hovered(r);
        let primary = style == ButtonStyle::Primary;
        let fill = match (down, hot, primary) {
            (_, _, _) if !enabled => theme::BUTTON_DOWN,
            (true, _, false) => theme::BUTTON_DOWN,
            (true, _, true) => theme::PRIMARY_DOWN,
            (_, true, false) => theme::BUTTON_HOT,
            (_, true, true) => theme::PRIMARY_HOT,
            (_, _, false) => theme::BUTTON,
            (_, _, true) => theme::PRIMARY,
        };
        self.draw.rect(r, fill);
        if hot {
            self.draw.border(r, 2, theme::TEXT);
        }
        let color = if enabled {
            theme::TEXT
        } else {
            theme::DISABLED
        };
        let scale = if r.h >= 44 { 2 } else { 1 };
        let label = fit(label, (r.w / (8 * scale)).max(1) as usize);
        self.draw.text_centered(r, scale, color, &label);
        let by_enter =
            primary && enabled && self.input.enter && !self.enter_taken && self.focus.is_none();
        if by_enter {
            self.enter_taken = true;
        }
        clicked || by_enter
    }

    /// A check box with a label to its right. True when it was toggled.
    pub fn toggle(&mut self, r: Rect, label: &str, value: &mut bool) -> bool {
        let id = self.id();
        let (clicked, _) = self.click(id, r);
        let hot = self.hovered(r);
        let boxr = Rect::new(r.x, r.y + (r.h - 22) / 2, 22, 22);
        self.draw.rect(boxr, theme::FIELD);
        self.draw
            .border(boxr, 2, if hot { theme::TEXT } else { theme::PANEL_EDGE });
        if *value {
            self.draw.rect(boxr.inset(5), theme::ACCENT);
        }
        self.draw
            .text(r.x + 32, r.y + (r.h - 16) / 2, 1, theme::TEXT, label);
        if clicked {
            *value = !*value;
        }
        clicked
    }

    /// A horizontal slider holding 0..=100 with the label left and the
    /// value right. True when the value changed.
    pub fn slider(&mut self, r: Rect, label: &str, value: &mut u8) -> bool {
        let id = self.id();
        let hot = self.hovered(r);
        if hot && self.input.pressed {
            self.active.arm(id);
            self.focus = None;
        }
        let track = Rect::new(
            r.x + 150,
            r.y + (r.h - 10) / 2,
            (r.w - 150 - 56).max(10),
            10,
        );
        let mut changed = false;
        if self.active.is(id) {
            let t = (self.input.mouse.0 - track.x) as f32 / track.w as f32;
            let v = (t.clamp(0.0, 1.0) * 100.0).round() as u8;
            if v != *value {
                *value = v;
                changed = true;
            }
        }
        self.draw
            .text(r.x, r.y + (r.h - 16) / 2, 1, theme::TEXT, label);
        self.draw.rect(track, theme::FIELD);
        self.draw.rect(
            Rect::new(track.x, track.y, track.w * i32::from(*value) / 100, track.h),
            theme::ACCENT,
        );
        let knob_x = track.x + track.w * i32::from(*value) / 100;
        let knob = Rect::new(knob_x - 5, r.y + (r.h - 22) / 2, 10, 22);
        self.draw.rect(
            knob,
            if hot || self.active.is(id) {
                theme::TEXT
            } else {
                theme::MUTED
            },
        );
        self.label_right(
            r.right(),
            r.y + (r.h - 16) / 2,
            1,
            theme::MUTED,
            &format!("{}%", *value),
        );
        changed
    }

    /// `< value >`: clicking the left arrow steps back, anywhere else
    /// steps forward, wrapping. Returns true when the index changed.
    pub fn stepper(&mut self, r: Rect, label: &str, index: &mut usize, options: &[&str]) -> bool {
        if options.is_empty() {
            return false;
        }
        let id = self.id();
        let (clicked, down) = self.click(id, r);
        let hot = self.hovered(r);
        let n = options.len();
        *index = (*index).min(n - 1);
        self.draw.rect(
            r,
            if down {
                theme::BUTTON_DOWN
            } else if hot {
                theme::BUTTON_HOT
            } else {
                theme::BUTTON
            },
        );
        let arrow = r.h.min(36);
        self.draw
            .text_centered(Rect::new(r.x, r.y, arrow, r.h), 1, theme::MUTED, "<");
        self.draw.text_centered(
            Rect::new(r.right() - arrow, r.y, arrow, r.h),
            1,
            theme::MUTED,
            ">",
        );
        let mid = Rect::new(r.x + arrow, r.y, r.w - 2 * arrow, r.h);
        let text = if label.is_empty() {
            options[*index].to_string()
        } else {
            format!("{label}: {}", options[*index])
        };
        let text = fit(&text, (mid.w / 8).max(1) as usize);
        self.draw.text_centered(mid, 1, theme::TEXT, &text);
        if !clicked {
            return false;
        }
        let back = self.input.mouse.0 < r.x + arrow;
        *index = if back {
            (*index + n - 1) % n
        } else {
            (*index + 1) % n
        };
        true
    }

    /// A one-line text box. Printable ASCII only. True when the text
    /// changed.
    pub fn text_field(
        &mut self,
        r: Rect,
        value: &mut String,
        max_chars: usize,
        placeholder: &str,
    ) -> bool {
        self.field(r, value, max_chars, placeholder, false).0
    }

    /// [`Ui::text_field`] for a box whose text is submitted: also true when
    /// Enter was pressed in it. That Enter is used up, so it doesn't press
    /// the screen's primary button too.
    pub fn text_entry(
        &mut self,
        r: Rect,
        value: &mut String,
        max_chars: usize,
        placeholder: &str,
    ) -> (bool, bool) {
        self.field(r, value, max_chars, placeholder, true)
    }

    /// The text box behind [`Ui::text_field`] and [`Ui::text_entry`]:
    /// `(changed, submitted)`. With `owns_enter`, a submitting Enter is not
    /// left for the primary button.
    fn field(
        &mut self,
        r: Rect,
        value: &mut String,
        max_chars: usize,
        placeholder: &str,
        owns_enter: bool,
    ) -> (bool, bool) {
        let id = self.id();
        let hot = self.hovered(r);
        if hot && self.input.pressed {
            self.focus = Some(id);
            self.active.clear();
        } else if self.input.pressed && self.focus == Some(id) {
            self.focus = None;
        }
        let focused = self.focus == Some(id);
        self.focus_claimed |= focused;
        let mut changed = false;
        let mut submitted = false;
        if focused {
            for ch in self.input.typed.chars() {
                if (' '..='~').contains(&ch) && value.chars().count() < max_chars {
                    value.push(ch);
                    changed = true;
                }
            }
            if self.input.backspace && value.pop().is_some() {
                changed = true;
            }
            if self.input.enter {
                self.focus = None;
                submitted = true;
                self.enter_taken |= owns_enter;
            }
        }
        self.draw.rect(r, theme::FIELD);
        self.draw.border(
            r,
            2,
            if focused {
                theme::ACCENT
            } else if hot {
                theme::MUTED
            } else {
                theme::PANEL_EDGE
            },
        );
        let y = r.y + (r.h - 16) / 2;
        let visible = (r.w - 20) / 8;
        if value.is_empty() && !focused {
            self.draw.text(
                r.x + 10,
                y,
                1,
                theme::DISABLED,
                &fit(placeholder, visible.max(1) as usize),
            );
        } else {
            // Show the tail so the cursor end is always visible.
            let count = value.chars().count() as i32;
            let skip = (count - (visible - 1)).max(0) as usize;
            let shown: String = value.chars().skip(skip).collect();
            self.draw.text(r.x + 10, y, 1, theme::TEXT, &shown);
            if focused {
                let cursor_x = r.x + 10 + shown.chars().count() as i32 * 8;
                self.draw.rect(Rect::new(cursor_x, y, 2, 16), theme::ACCENT);
            }
        }
        (changed, submitted)
    }

    /// A selectable row in a list.
    pub fn row(&mut self, r: Rect, selected: bool) -> bool {
        let id = self.id();
        let (clicked, _) = self.click(id, r);
        let hot = self.hovered(r);
        let fill = if selected {
            [60, 52, 44, 255]
        } else if hot {
            theme::BUTTON_HOT
        } else {
            theme::BUTTON
        };
        self.draw.rect(r, fill);
        if selected {
            self.draw.border(r, 2, theme::ACCENT);
        }
        clicked
    }

    /// A small colour square; clicking it returns true.
    pub fn swatch(&mut self, r: Rect, color: Color, clickable: bool) -> bool {
        let id = self.id();
        let (clicked, _) = if clickable {
            self.click(id, r)
        } else {
            (false, false)
        };
        self.draw.rect(r, color);
        let hot = clickable && self.hovered(r);
        self.draw
            .border(r, 2, if hot { theme::TEXT } else { theme::PANEL_EDGE });
        clicked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click_at(ui: &mut Ui, x: i32, y: i32, f: impl FnMut(&mut Ui) -> bool) -> bool {
        let mut f = f;
        let mut input = UiInput {
            mouse: (x, y),
            pressed: true,
            released: true,
            ..Default::default()
        };
        ui.begin_frame(input.clone(), (800, 600));
        let hit = f(ui);
        ui.end_frame();
        input.clear_events();
        hit
    }

    #[test]
    fn button_click_inside_and_outside() {
        let mut ui = Ui::new();
        let r = Rect::new(10, 10, 100, 40);
        assert!(click_at(&mut ui, 20, 20, |ui| ui.button(
            r,
            "Go",
            ButtonStyle::Normal,
            true
        )));
        assert!(!click_at(&mut ui, 200, 20, |ui| ui.button(
            r,
            "Go",
            ButtonStyle::Normal,
            true
        )));
        assert!(!click_at(&mut ui, 20, 20, |ui| ui.button(
            r,
            "Go",
            ButtonStyle::Normal,
            false
        )));
    }

    #[test]
    fn press_then_release_across_frames_clicks() {
        let mut ui = Ui::new();
        let r = Rect::new(0, 0, 50, 50);
        let down = UiInput {
            mouse: (5, 5),
            pressed: true,
            ..Default::default()
        };
        ui.begin_frame(down, (800, 600));
        assert!(!ui.button(r, "x", ButtonStyle::Normal, true));
        ui.end_frame();
        let up = UiInput {
            mouse: (5, 5),
            released: true,
            ..Default::default()
        };
        ui.begin_frame(up, (800, 600));
        assert!(ui.button(r, "x", ButtonStyle::Normal, true));
    }

    #[test]
    fn release_elsewhere_cancels_the_click() {
        let mut ui = Ui::new();
        let r = Rect::new(0, 0, 50, 50);
        ui.begin_frame(
            UiInput {
                mouse: (5, 5),
                pressed: true,
                ..Default::default()
            },
            (800, 600),
        );
        ui.button(r, "x", ButtonStyle::Normal, true);
        ui.end_frame();
        ui.begin_frame(
            UiInput {
                mouse: (300, 5),
                released: true,
                ..Default::default()
            },
            (800, 600),
        );
        assert!(!ui.button(r, "x", ButtonStyle::Normal, true));
    }

    #[test]
    fn press_clicks_only_on_release_over_the_same_widget() {
        let r = Rect::new(0, 0, 50, 50);
        let other = Rect::new(100, 0, 50, 50);
        let mut p = Press::default();
        assert_eq!(p.widget(1, r, (5, 5), true, false), (false, true));
        assert_eq!(p.widget(1, r, (5, 5), false, false), (false, true));
        assert_eq!(p.widget(1, r, (5, 5), false, true), (true, true));
        p.end_frame(true);
        // Released on another widget, or off the first one: no click.
        p.widget(1, r, (5, 5), true, false);
        assert!(!p.widget(2, other, (105, 5), false, true).0);
        assert!(!p.widget(1, r, (200, 5), false, true).0);
        p.end_frame(true);
        assert!(!p.widget(1, r, (5, 5), false, true).0, "press was cleared");
        // Overlapping widgets: the one asked last takes the press.
        p.widget(1, r, (5, 5), true, false);
        p.widget(2, r, (5, 5), true, false);
        assert!(!p.widget(1, r, (5, 5), false, true).0);
        assert!(p.widget(2, r, (5, 5), false, true).0);
    }

    #[test]
    fn enter_presses_the_primary_button_once() {
        let mut ui = Ui::new();
        ui.begin_frame(
            UiInput {
                enter: true,
                ..Default::default()
            },
            (800, 600),
        );
        let a = ui.button(Rect::new(0, 0, 50, 50), "a", ButtonStyle::Primary, true);
        let b = ui.button(Rect::new(60, 0, 50, 50), "b", ButtonStyle::Primary, true);
        assert!(a && !b);
    }

    #[test]
    fn enter_in_a_text_entry_submits_it_and_not_the_primary_button() {
        let mut ui = Ui::new();
        let r = Rect::new(0, 0, 300, 30);
        let mut s = String::from("abc");
        let click = UiInput {
            mouse: (5, 5),
            pressed: true,
            released: true,
            ..Default::default()
        };
        ui.begin_frame(click.clone(), (800, 600));
        assert_eq!(ui.text_entry(r, &mut s, 8, ""), (false, false));
        ui.end_frame();
        let enter = UiInput {
            enter: true,
            ..Default::default()
        };
        ui.begin_frame(enter.clone(), (800, 600));
        assert_eq!(ui.text_entry(r, &mut s, 8, ""), (false, true));
        let go = ui.button(Rect::new(0, 40, 50, 50), "go", ButtonStyle::Primary, true);
        assert!(!go, "the Enter was used up by the text box");
        ui.end_frame();
        // A plain text field leaves its Enter for the primary button.
        ui.begin_frame(click, (800, 600));
        ui.text_field(r, &mut s, 8, "");
        ui.end_frame();
        ui.begin_frame(enter, (800, 600));
        ui.text_field(r, &mut s, 8, "");
        assert!(ui.button(Rect::new(0, 40, 50, 50), "go", ButtonStyle::Primary, true));
    }

    #[test]
    fn stepper_wraps_both_ways() {
        let mut ui = Ui::new();
        let r = Rect::new(0, 0, 200, 30);
        let mut i = 0usize;
        let opts = ["a", "b", "c"];
        assert!(click_at(&mut ui, 5, 10, |ui| ui.stepper(r, "", &mut i, &opts)));
        assert_eq!(i, 2);
        assert!(click_at(&mut ui, 150, 10, |ui| ui.stepper(r, "", &mut i, &opts)));
        assert_eq!(i, 0);
    }

    #[test]
    fn toggle_flips() {
        let mut ui = Ui::new();
        let mut v = false;
        click_at(&mut ui, 5, 5, |ui| {
            ui.toggle(Rect::new(0, 0, 200, 30), "x", &mut v)
        });
        assert!(v);
    }

    #[test]
    fn slider_follows_the_mouse() {
        let mut ui = Ui::new();
        let r = Rect::new(0, 0, 400, 30);
        let mut v = 0u8;
        // Track runs from x=150 to x=150+194.
        click_at(&mut ui, 150 + 97, 10, |ui| ui.slider(r, "Vol", &mut v));
        assert_eq!(v, 50);
        click_at(&mut ui, 5000, 10, |ui| ui.slider(r, "Vol", &mut v));
        assert_eq!(v, 50, "a click outside the slider does not move it");
    }

    #[test]
    fn text_field_takes_focus_and_types() {
        let mut ui = Ui::new();
        let r = Rect::new(0, 0, 300, 30);
        let mut s = String::new();
        ui.begin_frame(
            UiInput {
                mouse: (5, 5),
                pressed: true,
                released: true,
                ..Default::default()
            },
            (800, 600),
        );
        ui.text_field(r, &mut s, 8, "hint");
        ui.end_frame();
        assert!(ui.text_focused());
        ui.begin_frame(
            UiInput {
                typed: "abc\u{e9}".into(),
                ..Default::default()
            },
            (800, 600),
        );
        assert!(ui.text_field(r, &mut s, 8, "hint"));
        assert_eq!(s, "abc");
        ui.begin_frame(
            UiInput {
                backspace: true,
                ..Default::default()
            },
            (800, 600),
        );
        ui.text_field(r, &mut s, 8, "hint");
        assert_eq!(s, "ab");
        ui.begin_frame(
            UiInput {
                typed: "123456789".into(),
                ..Default::default()
            },
            (800, 600),
        );
        ui.text_field(r, &mut s, 8, "hint");
        assert_eq!(s.chars().count(), 8);
    }

    #[test]
    fn clicking_away_drops_text_focus() {
        let mut ui = Ui::new();
        let r = Rect::new(0, 0, 300, 30);
        let mut s = String::new();
        ui.begin_frame(
            UiInput {
                mouse: (5, 5),
                pressed: true,
                ..Default::default()
            },
            (800, 600),
        );
        ui.text_field(r, &mut s, 8, "");
        assert!(ui.text_focused());
        ui.begin_frame(
            UiInput {
                mouse: (500, 500),
                pressed: true,
                ..Default::default()
            },
            (800, 600),
        );
        ui.text_field(r, &mut s, 8, "");
        assert!(!ui.text_focused());
    }
}
