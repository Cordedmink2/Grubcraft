// After the world is drawn in linear HDR: the glow (bloom) and the film
// curve and colour grade that bring the frame to the screen.
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

// What the composite grades the frame with (`Grade` in post.rs).
struct Grade {
    // x: exposure, y: contrast, z: saturation, w: vignette
    tone: vec4<f32>,
    // rgb: tint of the dark end
    shadow_tint: vec4<f32>,
    // rgb: tint of the bright end
    light_tint: vec4<f32>,
    // x: film grain, y: time in seconds
    film: vec4<f32>,
};
@group(0) @binding(3) var<uniform> grade: Grade;

// The film curve: Narkowicz's fit of the ACES curve (2015). look.rs has its
// inverse, which turns an authored colour into the HDR value that draws as it.
fn film(x: vec3<f32>) -> vec3<f32> {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

const LUMA = vec3<f32>(0.2126, 0.7152, 0.0722);

fn hash12(p: vec2<f32>) -> f32 {
    var q = fract(vec3<f32>(p.xyx) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

@fragment
fn fs_composite(in: Fullscreen) -> @location(0) vec4<f32> {
    var c = textureSampleLevel(src, src_sampler, in.uv, 0.0).rgb;
    if BLOOM_STRENGTH > 0.0 {
        c += textureSampleLevel(bloom, src_sampler, in.uv, 0.0).rgb * BLOOM_STRENGTH;
    }
    // Film curve, then split toning: cool shade, warm light.
    var t = film(c * grade.tone.x);
    t = t * mix(grade.shadow_tint.rgb, grade.light_tint.rgb, smoothstep(0.05, 0.7, dot(t, LUMA)));
    // Saturation, then contrast round mid grey in display space.
    t = max(mix(vec3<f32>(dot(t, LUMA)), t, grade.tone.z), vec3<f32>(0.0));
    var p = pow(t, vec3<f32>(1.0 / 2.2));
    p = clamp((p - 0.45) * grade.tone.y + 0.45, vec3<f32>(0.0), vec3<f32>(1.0));
    // Vignette.
    let q = in.uv - 0.5;
    p = p * (1.0 - grade.tone.w * smoothstep(0.15, 0.75, dot(q, q) * 2.2));
    // Film grain, strongest in the mid tones, and dither (triangular noise
    // of one display step) so the sky's gradient does not band.
    // Both change every frame.
    let seed = in.clip.xy + fract(grade.film.y * 7.13) * 91.7;
    let grain = hash12(seed) - 0.5;
    let mid = 4.0 * p.g * (1.0 - p.g);
    let dither = (hash12(seed + 17.1) + hash12(seed + 43.7) - 1.0) * (1.0 / 255.0);
    p = clamp(p + grain * grade.film.x * mid + dither, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(pow(p, vec3<f32>(2.2)), 1.0);
}
