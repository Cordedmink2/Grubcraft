// After the world is drawn in linear HDR: the glow (bloom) and the tone curve
// that brings the frame to the screen.
//
// Glow: the bright parts of the frame (sun glints on metal, emitted light)
// are kept and shrunk down a chain of half-size images, then grown back up,
// each level adding its blur to the one above, so a highlight spreads into a
// soft halo (the method of Jimenez, "Next generation post processing in Call
// of Duty: Advanced Warfare", 2014).

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;

// Composite only.
@group(0) @binding(2) var bloom: texture_2d<f32>;

// How strongly the glow adds to the frame; 0 with the glow off.
override BLOOM_STRENGTH: f32 = 0.0;
// The tone curve's exposure (post.rs's EXPOSURE, which also inverts it).
override EXPOSURE: f32 = 1.25;

// Light brighter than this (in the HDR scale, where sunlit white paint is
// about 1.7) starts to glow, easing in over `KNEE` below it.
const THRESHOLD: f32 = 2.2;
const KNEE: f32 = 0.8;

struct Fullscreen {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One triangle that covers the screen.
@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> Fullscreen {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var out: Fullscreen;
    out.clip = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}

fn tap(uv: vec2<f32>, texel: vec2<f32>, x: f32, y: f32) -> vec3<f32> {
    return textureSampleLevel(src, src_sampler, uv + texel * vec2<f32>(x, y), 0.0).rgb;
}

// Thirteen filtered taps: a 6 × 6 texel footprint without the blockiness of
// a plain 2 × 2 average.
fn downsample(uv: vec2<f32>) -> vec3<f32> {
    let t = 1.0 / vec2<f32>(textureDimensions(src));
    let inner = tap(uv, t, -1.0, -1.0) + tap(uv, t, 1.0, -1.0) + tap(uv, t, -1.0, 1.0)
        + tap(uv, t, 1.0, 1.0);
    let corners = tap(uv, t, -2.0, -2.0) + tap(uv, t, 2.0, -2.0) + tap(uv, t, -2.0, 2.0)
        + tap(uv, t, 2.0, 2.0);
    let edges = tap(uv, t, 0.0, -2.0) + tap(uv, t, -2.0, 0.0) + tap(uv, t, 2.0, 0.0)
        + tap(uv, t, 0.0, 2.0);
    return tap(uv, t, 0.0, 0.0) * 0.125 + inner * 0.125 + corners * 0.03125 + edges * 0.0625;
}

// The first step down keeps only light above the threshold.
@fragment
fn fs_bloom_prefilter(in: Fullscreen) -> @location(0) vec4<f32> {
    let c = downsample(in.uv);
    let bright = max(c.r, max(c.g, c.b));
    var soft = clamp(bright - THRESHOLD + KNEE, 0.0, 2.0 * KNEE);
    soft = soft * soft / (4.0 * KNEE + 1e-4);
    let keep = max(soft, bright - THRESHOLD) / max(bright, 1e-4);
    // Clamp single blazing pixels so they don't flicker as they move.
    return vec4<f32>(min(c * keep, vec3<f32>(32.0)), 1.0);
}

@fragment
fn fs_bloom_down(in: Fullscreen) -> @location(0) vec4<f32> {
    return vec4<f32>(downsample(in.uv), 1.0);
}

// A 3 × 3 tent blur of the smaller level, added onto the larger one.
@fragment
fn fs_bloom_up(in: Fullscreen) -> @location(0) vec4<f32> {
    let t = 1.0 / vec2<f32>(textureDimensions(src));
    var c = tap(in.uv, t, 0.0, 0.0) * 4.0;
    c += (tap(in.uv, t, -1.0, 0.0) + tap(in.uv, t, 1.0, 0.0) + tap(in.uv, t, 0.0, -1.0)
        + tap(in.uv, t, 0.0, 1.0)) * 2.0;
    c += tap(in.uv, t, -1.0, -1.0) + tap(in.uv, t, 1.0, -1.0) + tap(in.uv, t, -1.0, 1.0)
        + tap(in.uv, t, 1.0, 1.0);
    return vec4<f32>(c / 16.0, 1.0);
}

// Soft filmic curve so bright sunlit slopes don't clip to white.
fn tonemap(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(1.0) - exp(-c * EXPOSURE);
}

@fragment
fn fs_composite(in: Fullscreen) -> @location(0) vec4<f32> {
    var c = textureSampleLevel(src, src_sampler, in.uv, 0.0).rgb;
    if BLOOM_STRENGTH > 0.0 {
        c += textureSampleLevel(bloom, src_sampler, in.uv, 0.0).rgb * BLOOM_STRENGTH;
    }
    return vec4<f32>(tonemap(c), 1.0);
}
