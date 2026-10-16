//! A flat list of coloured rectangles and bitmap glyphs, in UI pixels.

/// Glyph cell of the bitmap font in `gc_render`, at text scale 1.
pub const CELL_W: i32 = 8;
pub const CELL_H: i32 = 16;
/// First printable character in the font strip.
const FIRST_CHAR: u8 = 32;
/// Most quads one frame of UI may use; the rest are dropped.
pub const MAX_QUADS: usize = 8192;
/// Marks a quad as a solid rectangle rather than a glyph.
pub const SOLID: u32 = u32::MAX;

pub type Color = [u8; 4];

/// One drawable thing. Same layout the renderer's UI pass reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quad {
    /// Top-left corner, in UI pixels.
    pub pos: [u16; 2],
    /// Size in UI pixels.
    pub size: [u16; 2],
    /// Index into the font strip, or [`SOLID`].
    pub glyph: u32,
    pub color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }

    pub fn inset(&self, d: i32) -> Rect {
        Rect::new(
            self.x + d,
            self.y + d,
            (self.w - 2 * d).max(0),
            (self.h - 2 * d).max(0),
        )
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    /// Split off `w` pixels from the left: `(left, rest)`.
    pub fn split_left(&self, w: i32) -> (Rect, Rect) {
        let w = w.clamp(0, self.w);
        (
            Rect::new(self.x, self.y, w, self.h),
            Rect::new(self.x + w, self.y, self.w - w, self.h),
        )
    }
}

/// Width in pixels of `text` at integer text `scale`.
pub fn text_width(text: &str, scale: i32) -> i32 {
    text.chars().count() as i32 * CELL_W * scale
}

pub fn text_height(scale: i32) -> i32 {
    CELL_H * scale
}

/// `text` cut to at most `max_chars` characters, ending in `..` if cut.
pub fn fit(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(2);
    let mut s: String = text.chars().take(keep).collect();
    s.push_str("..");
    s.chars().take(max_chars).collect()
}

#[derive(Default)]
pub struct DrawList {
    pub quads: Vec<Quad>,
}

fn clamp16(v: i32) -> u16 {
    v.clamp(0, i32::from(u16::MAX)) as u16
}

impl DrawList {
    pub fn clear(&mut self) {
        self.quads.clear();
    }

    fn push(&mut self, q: Quad) {
        if self.quads.len() < MAX_QUADS {
            self.quads.push(q);
        }
    }

    pub fn rect(&mut self, r: Rect, color: Color) {
        if r.w <= 0 || r.h <= 0 || color[3] == 0 {
            return;
        }
        // Off-screen to the left or top is clipped by clamping the origin.
        let (x, y) = (r.x.max(0), r.y.max(0));
        let (w, h) = (r.w - (x - r.x), r.h - (y - r.y));
        if w <= 0 || h <= 0 {
            return;
        }
        self.push(Quad {
            pos: [clamp16(x), clamp16(y)],
            size: [clamp16(w), clamp16(h)],
            glyph: SOLID,
            color,
        });
    }

    /// An outline `t` pixels thick just inside `r`.
    pub fn border(&mut self, r: Rect, t: i32, color: Color) {
        self.rect(Rect::new(r.x, r.y, r.w, t), color);
        self.rect(Rect::new(r.x, r.bottom() - t, r.w, t), color);
        self.rect(Rect::new(r.x, r.y + t, t, r.h - 2 * t), color);
        self.rect(Rect::new(r.right() - t, r.y + t, t, r.h - 2 * t), color);
    }

    /// Bitmap text with its top-left at `(x, y)`. Characters outside
    /// printable ASCII show as `?`.
    pub fn text(&mut self, x: i32, y: i32, scale: i32, color: Color, text: &str) {
        for (i, ch) in text.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let code = if (' '..='~').contains(&ch) {
                ch as u8
            } else {
                b'?'
            };
            let gx = x + i as i32 * CELL_W * scale;
            if gx < 0 || y < 0 {
                continue;
            }
            self.push(Quad {
                pos: [clamp16(gx), clamp16(y)],
                size: [clamp16(CELL_W * scale), clamp16(CELL_H * scale)],
                glyph: u32::from(code - FIRST_CHAR),
                color,
            });
        }
    }

    /// A horizontal meter: `track` in `empty`, with `frac` (0 to 1) of it
    /// filled in `color`. A little is always visible when `frac` is above 0.
    pub fn meter(&mut self, track: Rect, frac: f32, color: Color, empty: Color) {
        self.rect(track, empty);
        let w = (track.w as f32 * frac.clamp(0.0, 1.0)).round() as i32;
        let w = if frac > 0.0 { w.max(1) } else { 0 };
        self.rect(Rect::new(track.x, track.y, w, track.h), color);
    }

    /// Text centred in `r`.
    pub fn text_centered(&mut self, r: Rect, scale: i32, color: Color, text: &str) {
        let x = r.x + (r.w - text_width(text, scale)) / 2;
        let y = r.y + (r.h - text_height(scale)) / 2;
        self.text(x, y, scale, color, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_makes_one_quad_per_visible_glyph() {
        let mut d = DrawList::default();
        d.text(10, 20, 2, [255; 4], "a b");
        assert_eq!(d.quads.len(), 2);
        assert_eq!(d.quads[0].size, [16, 32]);
        assert_eq!(d.quads[0].glyph, u32::from(b'a' - 32));
        assert_eq!(d.quads[1].pos, [10 + 2 * 16, 20]);
    }

    #[test]
    fn non_ascii_becomes_question_mark() {
        let mut d = DrawList::default();
        d.text(0, 0, 1, [255; 4], "é");
        assert_eq!(d.quads[0].glyph, u32::from(b'?' - 32));
    }

    #[test]
    fn rect_clips_at_the_top_left() {
        let mut d = DrawList::default();
        d.rect(Rect::new(-5, -5, 20, 20), [1, 2, 3, 255]);
        assert_eq!(d.quads[0].pos, [0, 0]);
        assert_eq!(d.quads[0].size, [15, 15]);
        d.rect(Rect::new(-30, 0, 20, 20), [1, 2, 3, 255]);
        assert_eq!(d.quads.len(), 1);
    }

    #[test]
    fn fit_shortens_with_dots() {
        assert_eq!(fit("short", 10), "short");
        assert_eq!(fit("a very long player name", 10), "a very l..");
        assert_eq!(fit("abc", 1).chars().count(), 1);
    }

    #[test]
    fn quad_count_is_capped() {
        let mut d = DrawList::default();
        for _ in 0..MAX_QUADS + 10 {
            d.rect(Rect::new(0, 0, 1, 1), [1; 4]);
        }
        assert_eq!(d.quads.len(), MAX_QUADS);
    }
}
