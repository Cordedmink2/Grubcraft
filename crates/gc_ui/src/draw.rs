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

    /// The part of `self` inside `other`, if any.
    pub fn intersect(&self, other: Rect) -> Option<Rect> {
        let (x0, y0) = (self.x.max(other.x), self.y.max(other.y));
        let (x1, y1) = (
            self.right().min(other.right()),
            self.bottom().min(other.bottom()),
        );
        (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
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

/// `text` broken into lines of at most `width` characters, at spaces where
/// it can (a word longer than a line is cut).
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word = word;
        loop {
            let used = line.chars().count();
            let need = word.chars().count() + usize::from(used > 0);
            if used + need <= width {
                if used > 0 {
                    line.push(' ');
                }
                line.push_str(word);
                break;
            }
            if used > 0 {
                lines.push(std::mem::take(&mut line));
                continue;
            }
            // A word wider than the line: cut it.
            let cut = word
                .char_indices()
                .nth(width)
                .map_or(word.len(), |(at, _)| at);
            lines.push(word[..cut].to_string());
            word = &word[cut..];
            if word.is_empty() {
                break;
            }
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
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

/// `a` blended towards `b` by `t` (0 is `a`, 1 is `b`), alpha kept from `a`.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let ch = |i: usize| (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * t).round() as u8;
    [ch(0), ch(1), ch(2), a[3]]
}

/// Length and gap of a stitch, in pixels.
const STITCH: i32 = 6;
const STITCH_GAP: i32 = 3;

impl DrawList {
    /// A row of embroidery stitches `t` pixels thick just inside `r`: dashes
    /// in madder, woad, weld and wode wool in turn, as along the Bayeux
    /// tapestry's borders.
    pub fn stitched_border(&mut self, r: Rect, t: i32) {
        use crate::ui::theme;
        let dyes = [theme::MADDER, theme::WOAD, theme::WELD, theme::WODE];
        let mut n = 0usize;
        let mut run = |d: &mut DrawList, x: i32, y: i32, len: i32, horizontal: bool| {
            let mut at = 0;
            while at < len {
                let l = STITCH.min(len - at);
                let rect = if horizontal {
                    Rect::new(x + at, y, l, t)
                } else {
                    Rect::new(x, y + at, t, l)
                };
                d.rect(rect, dyes[n % dyes.len()]);
                n += 1;
                at += STITCH + STITCH_GAP;
            }
        };
        run(self, r.x, r.y, r.w, true);
        run(self, r.x, r.y + t, r.h - 2 * t, false);
        run(self, r.x, r.bottom() - t, r.w, true);
        run(self, r.right() - t, r.y + t, r.h - 2 * t, false);
    }

    /// One horizontal row of stitches `t` thick from `x` across `len`.
    pub fn stitch_row(&mut self, x: i32, y: i32, len: i32, t: i32) {
        use crate::ui::theme;
        let dyes = [theme::MADDER, theme::WOAD, theme::WELD, theme::WODE];
        let (mut at, mut n) = (0, 0usize);
        while at < len {
            let l = STITCH.min(len - at);
            self.rect(Rect::new(x + at, y, l, t), dyes[n % dyes.len()]);
            n += 1;
            at += STITCH + STITCH_GAP;
        }
    }

    /// A linen cloth panel: `fill`, a dark ink outline and a stitched strip
    /// just inside it. Returns the room left for content.
    pub fn cloth(&mut self, r: Rect, fill: Color) -> Rect {
        use crate::ui::theme;
        self.rect(r, fill);
        self.border(r, 2, theme::INK);
        self.stitched_border(r.inset(4), 2);
        r.inset(9)
    }

    /// A flat button or card: `fill` with an ink outline `t` thick.
    pub fn boxed(&mut self, r: Rect, fill: Color, t: i32) {
        self.rect(r, fill);
        self.border(r, t, crate::ui::theme::INK);
    }

    /// Text drawn twice, a pixel apart, so it reads as heavier.
    pub fn text_bold(&mut self, x: i32, y: i32, scale: i32, color: Color, text: &str) {
        self.text(x, y, scale, color, text);
        self.text(x + 1, y, scale, color, text);
    }

    /// A hard drop shadow under `r`, offset down and right by `d`.
    pub fn shadow(&mut self, r: Rect, d: i32) {
        self.rect(Rect::new(r.x + d, r.y + d, r.w, r.h), [20, 12, 6, 90]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersecting_keeps_the_overlap_and_nothing_else() {
        let r = Rect::new(0, 0, 10, 10);
        assert_eq!(
            r.intersect(Rect::new(5, 4, 10, 10)),
            Some(Rect::new(5, 4, 5, 6))
        );
        assert_eq!(
            r.intersect(Rect::new(2, 2, 3, 3)),
            Some(Rect::new(2, 2, 3, 3))
        );
        // Touching edges share no pixels.
        assert_eq!(r.intersect(Rect::new(10, 0, 5, 5)), None);
        assert_eq!(r.intersect(Rect::new(-9, -9, 5, 5)), None);
    }

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
    fn a_stitched_border_cycles_the_four_dyes_inside_its_rect() {
        use crate::ui::theme;
        let r = Rect::new(10, 10, 100, 40);
        let mut d = DrawList::default();
        d.stitched_border(r, 2);
        assert!(d.quads.len() > 20);
        let colours: std::collections::HashSet<Color> = d.quads.iter().map(|q| q.color).collect();
        for dye in [theme::MADDER, theme::WOAD, theme::WELD, theme::WODE] {
            assert!(colours.contains(&dye));
        }
        for q in &d.quads {
            let q = Rect::new(
                i32::from(q.pos[0]),
                i32::from(q.pos[1]),
                i32::from(q.size[0]),
                i32::from(q.size[1]),
            );
            assert_eq!(q.intersect(r), Some(q));
        }
    }

    #[test]
    fn cloth_returns_the_room_inside_its_stitching() {
        let r = Rect::new(0, 0, 100, 50);
        let mut d = DrawList::default();
        assert_eq!(d.cloth(r, [1; 4]), r.inset(9));
        assert_eq!(
            mix([0, 0, 0, 255], [100, 200, 50, 0], 0.5),
            [50, 100, 25, 255]
        );
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
