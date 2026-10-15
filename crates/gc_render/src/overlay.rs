//! The dev overlay: monospaced bitmap text over a translucent panel, drawn
//! at the end of the main pass. One instanced draw call; the font is a
//! 760 × 16 one-byte-per-texel strip (12 KB).

use bytemuck::{Pod, Zeroable};

use crate::debug_font::{FIRST_CHAR, GLYPH_HEIGHT, GLYPH_WIDTH, GLYPHS};

/// Most characters (plus one panel) the overlay can show at once.
pub const MAX_OVERLAY_CHARS: usize = 4096;
const SOLID: u32 = u32::MAX;
const MARGIN: u16 = 8;

/// One rectangle or glyph of screen-space UI (and of the dev overlay).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct UiQuad {
    /// Top-left corner, in UI pixels (times the draw scale on screen).
    pub pos: [u16; 2],
    /// Size in UI pixels. A glyph's cell is stretched to fit.
    pub size: [u16; 2],
    /// Index into the font strip (`code - 32`), or `u32::MAX` for a solid
    /// rectangle.
    pub glyph: u32,
    pub color: [u8; 4],
}

pub(crate) type GlyphInstance = UiQuad;

/// Lay out `text` (lines separated by `\n`) at the top-left corner over a
/// panel. Characters outside printable ASCII show as `?`.
pub(crate) fn layout(text: &str, out: &mut Vec<UiQuad>) {
    out.clear();
    let lines = text.lines().count() as u16;
    let columns = text.lines().map(|l| l.chars().count()).max().unwrap_or(0) as u16;
    if lines == 0 || columns == 0 {
        return;
    }
    // The panel is one cell larger than the text in each direction, giving
    // half a cell of padding on the sides and top and bottom.
    out.push(GlyphInstance {
        pos: [MARGIN, MARGIN],
        size: [
            (columns + 2) * GLYPH_WIDTH as u16,
            (lines + 1) * GLYPH_HEIGHT as u16,
        ],
        glyph: SOLID,
        color: [10, 12, 16, 190],
    });
    let left = MARGIN + GLYPH_WIDTH as u16;
    let top = MARGIN + GLYPH_HEIGHT as u16 / 2;
    for (row, line) in text.lines().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            if out.len() >= MAX_OVERLAY_CHARS {
                return;
            }
            if ch == ' ' {
                continue;
            }
            let code = if (' '..='~').contains(&ch) {
                ch as u8
            } else {
                b'?'
            };
            out.push(GlyphInstance {
                pos: [
                    left + col as u16 * GLYPH_WIDTH as u16,
                    top + row as u16 * GLYPH_HEIGHT as u16,
                ],
                size: [GLYPH_WIDTH as u16, GLYPH_HEIGHT as u16],
                glyph: u32::from(code - FIRST_CHAR),
                color: [235, 238, 242, 255],
            });
        }
    }
}

/// The font strip as one byte per texel (0 or 255).
pub(crate) fn font_texels() -> Vec<u8> {
    let width = GLYPHS.len() * GLYPH_WIDTH as usize;
    let mut texels = vec![0u8; width * GLYPH_HEIGHT as usize];
    for (g, rows) in GLYPHS.iter().enumerate() {
        for (y, bits) in rows.iter().enumerate() {
            for x in 0..GLYPH_WIDTH as usize {
                if bits & (0x80 >> x) != 0 {
                    texels[y * width + g * GLYPH_WIDTH as usize + x] = 255;
                }
            }
        }
    }
    texels
}

pub(crate) fn font_width() -> u32 {
    GLYPHS.len() as u32 * GLYPH_WIDTH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_places_panel_then_glyphs() {
        let mut out = Vec::new();
        layout("ab\nc d", &mut out);
        // Panel + 'a' 'b' 'c' 'd' (space skipped).
        assert_eq!(out.len(), 5);
        assert_eq!(out[0].glyph, SOLID);
        assert_eq!(out[0].size, [5 * 8, 3 * 16]);
        assert_eq!(out[1].glyph, u32::from(b'a' - FIRST_CHAR));
        assert_eq!(out[4].pos[0] - out[3].pos[0], 2 * GLYPH_WIDTH as u16);
        assert_eq!(out[3].pos[1] - out[1].pos[1], GLYPH_HEIGHT as u16);
    }

    #[test]
    fn non_ascii_becomes_question_mark() {
        let mut out = Vec::new();
        layout("é", &mut out);
        assert_eq!(out[1].glyph, u32::from(b'?' - FIRST_CHAR));
    }

    #[test]
    fn empty_text_draws_nothing() {
        let mut out = Vec::new();
        layout("", &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn font_strip_has_ink() {
        let t = font_texels();
        assert_eq!(t.len(), (font_width() * GLYPH_HEIGHT) as usize);
        assert!(t.iter().filter(|&&v| v == 255).count() > 1000);
    }
}
