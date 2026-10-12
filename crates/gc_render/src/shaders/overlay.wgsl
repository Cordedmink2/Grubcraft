// Dev overlay: one-bit bitmap text and translucent panels in screen space.

struct OverlayGlobals {
    // Target width and height in pixels, pixel scale, unused.
    screen: vec4<f32>,
};

@group(0) @binding(0) var<uniform> og: OverlayGlobals;
@group(0) @binding(1) var font: texture_2d<f32>;

const GLYPH: vec2<f32> = vec2<f32>(8.0, 16.0);
const SOLID: u32 = 0xffffffffu;

struct GlyphIn {
    // Top-left corner in unscaled pixels.
    @location(0) pos: vec2<u32>,
    // Size in unscaled pixels; a glyph's 8 x 16 cell is stretched to fit.
    @location(1) size: vec2<u32>,
    // Glyph index into the font strip, or SOLID for a filled panel.
    @location(2) glyph: u32,
    @location(3) color: vec4<f32>,
};

struct GlyphOut {
    @builtin(position) clip: vec4<f32>,
    // Position inside the glyph cell, in font texels.
    @location(0) texel: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) glyph: u32,
};

@vertex
fn vs_glyph(@builtin(vertex_index) vi: u32, g: GlyphIn) -> GlyphOut {
    let corner = vec2<f32>(f32(vi & 1u), f32((vi >> 1u) & 1u));
    let px = (vec2<f32>(g.pos) + corner * vec2<f32>(g.size)) * og.screen.z;
    var out: GlyphOut;
    out.clip = vec4<f32>(px.x / og.screen.x * 2.0 - 1.0, 1.0 - px.y / og.screen.y * 2.0, 0.0, 1.0);
    out.texel = corner * GLYPH;
    // Colours are authored in sRGB; the surface is sRGB, so write linear.
    out.color = vec4<f32>(pow(g.color.rgb, vec3<f32>(2.2)), g.color.a);
    out.glyph = g.glyph;
    return out;
}

@fragment
fn fs_glyph(in: GlyphOut) -> @location(0) vec4<f32> {
    if in.glyph == SOLID {
        return in.color;
    }
    let local = clamp(vec2<i32>(floor(in.texel)), vec2<i32>(0, 0), vec2<i32>(7, 15));
    let coverage = textureLoad(font, vec2<i32>(i32(in.glyph) * 8 + local.x, local.y), 0).r;
    if coverage < 0.5 {
        discard;
    }
    return in.color;
}
