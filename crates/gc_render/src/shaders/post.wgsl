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
    var c = downsample(in.uv);
    // A pixel that came out not-a-number or infinite must not spread over
    // the screen through the blur.
    if any(c != c) || any(abs(c) > vec3<f32>(1e6)) {
        c = vec3<f32>(0.0);
    }
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
    // xy: the shake (an offset of the picture), z: the zoom that hides its
    // edges, w: the height of each letterbox bar, as a share of the screen
    camera: vec4<f32>,
    // x: tilt-shift strength, y: sun shaft strength, zw: where the sun is on
    // the screen (a share of its width and height)
    focus: vec4<f32>,
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

// The tilt-shift look blurs the picture up to this many texels at its top and
// bottom edges, over a spiral of this many taps.
const TILT_RADIUS: f32 = 9.0;
const TILT_TAPS: i32 = 12;
// The row of the screen that stays sharp: the middle of the field, which is
// above the middle of the window (the command bar fills the bottom).
const TILT_CENTRE: f32 = 0.4;
// Sun shafts: steps of a march from a pixel toward the sun, how much of the
// way it goes, how much each step counts less, and how bright (in the HDR
// scale) the sky must be to send light down a shaft.
const SHAFT_STEPS: i32 = 14;
const SHAFT_REACH: f32 = 0.85;
const SHAFT_FADE: f32 = 0.93;
const SHAFT_LOW: f32 = 1.0;
const SHAFT_HIGH: f32 = 2.2;

@fragment
fn fs_composite(in: Fullscreen) -> @location(0) vec4<f32> {
    // The picture, shaken: zoomed in about the middle and moved.
    let uv = (in.uv - 0.5) / grade.camera.z + 0.5 + grade.camera.xy;
    var c: vec3<f32>;
    // Tilt-shift: out of focus toward the top and bottom edges, so the
    // battlefield looks like a table of miniatures. The centre stays sharp.
    let blur = grade.focus.x * smoothstep(0.12, 0.4, abs(in.uv.y - TILT_CENTRE));
    if blur > 0.03 {
        let texel = 1.0 / vec2<f32>(textureDimensions(src));
        var acc = vec3<f32>(0.0);
        for (var i = 0; i < TILT_TAPS; i++) {
            let a = f32(i) * 2.3999632;
            let r = sqrt((f32(i) + 0.5) / f32(TILT_TAPS)) * blur * TILT_RADIUS;
            acc += textureSampleLevel(src, src_sampler, uv + vec2<f32>(cos(a), sin(a)) * r * texel, 0.0).rgb;
        }
        c = acc / f32(TILT_TAPS);
    } else {
        c = textureSampleLevel(src, src_sampler, uv, 0.0).rgb;
    }
    if BLOOM_STRENGTH > 0.0 {
        c += textureSampleLevel(bloom, src_sampler, uv, 0.0).rgb * BLOOM_STRENGTH;
    }
    // Sun shafts: march from here toward the sun, gathering the bright sky
    // seen through gaps (what is in the way is dark and gathers nothing).
    if grade.focus.y > 0.0 {
        let step = (grade.focus.zw - uv) * (SHAFT_REACH / f32(SHAFT_STEPS));
        var p = uv;
        var w = 1.0;
        var shaft = vec3<f32>(0.0);
        for (var i = 0; i < SHAFT_STEPS; i++) {
            p += step;
            let s = textureSampleLevel(src, src_sampler, p, 0.0).rgb;
            // Past the edge of the picture there is nothing to gather (the
            // edge texel would smear down the screen).
            let inside = f32(all(p == clamp(p, vec2<f32>(0.0), vec2<f32>(1.0))));
            shaft += s * smoothstep(SHAFT_LOW, SHAFT_HIGH, dot(s, LUMA)) * w * inside;
            w *= SHAFT_FADE;
        }
        c += shaft / f32(SHAFT_STEPS) * grade.focus.y * grade.light_tint.rgb;
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
    // Letterbox bars, with a soft edge of a pixel or two.
    let bar = grade.camera.w;
    if bar > 0.0 {
        let edge = fwidth(in.uv.y) * 1.5;
        p = p * smoothstep(bar - edge, bar + edge, min(in.uv.y, 1.0 - in.uv.y));
    }
    return vec4<f32>(pow(p, vec3<f32>(2.2)), 1.0);
}
