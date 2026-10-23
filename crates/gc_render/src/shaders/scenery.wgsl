// Scenery: trees, bushes, grass and rocks. Appended to world.wgsl (see
// `scenery.rs`), so it shares `Globals`, `height_at`, `shade`, `veil` and the
// rest. Each model is an indexed vertex-coloured mesh in vertex buffer 0;
// each instance (`SceneryInstance`) is in vertex buffer 1. Group 0 only.

// `SWAY_MAX` (the most a vertex moves in the wind at scale 1 when its weight
// is 1, in tiles; the baker has already multiplied the model's own sway into
// the weight) and `POSITION_SCALE` (positions are stored in 1/POSITION_SCALE
// tiles) are prepended by `shader_source()` from gc_assets::scenery.
// The prevailing wind blows toward this direction (x east, y south).
const WIND_DIR = vec2<f32>(0.8, 0.6);

struct SceneryIn {
    // x, y, z in 1/POSITION_SCALE tiles (the fourth is padding).
    @location(0) pos: vec4<i32>,
    @location(1) normal: vec4<f32>,
    // sRGB colour; alpha is the wind weight.
    @location(2) color: vec4<f32>,
    @location(3) at: vec2<f32>,
    @location(4) facing: vec2<f32>,
    @location(5) scale: f32,
    // sRGB multiplier (0.5 leaves the colour as baked); alpha is the wind phase.
    @location(6) tint: vec4<f32>,
};

struct SceneryOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    // 1 for leaves and blades (what moves in the wind), 0 for rocks and bases.
    @location(3) leafy: f32,
};

// How the wind pushes things at `xz` right now: a unit-ish vector along the
// prevailing direction. Slow patches of stronger wind roll across the map
// with a travelling swell inside them, so a forest moves like a field of
// wheat; `phase` (0 to 1) adds a small flutter of each plant's own.
fn gust(xz: vec2<f32>, phase: f32) -> vec2<f32> {
    let t = g.misc.y;
    let across = vec2<f32>(-WIND_DIR.y, WIND_DIR.x);
    let along = dot(xz, WIND_DIR);
    let patches = vnoise(xz * 0.035 - WIND_DIR * t * 0.9);
    let swell = 0.5 + 0.5 * sin(along * 0.45 - t * 1.4);
    let lean = 0.15 + patches * (0.35 + 0.5 * swell);
    let flutter = sin(t * 3.1 + phase * 6.2832) * 0.06 + sin(t * 7.3 + phase * 12.566) * 0.025;
    return WIND_DIR * (lean + flutter) + across * (flutter * 0.8);
}

// Where one vertex of one instance stands, and its normal.
fn place(v: SceneryIn) -> vec3<f32> {
    let left = vec2<f32>(v.facing.y, -v.facing.x);
    let local = vec3<f32>(v.pos.xyz) * (v.scale / POSITION_SCALE);
    let flat_xz = v.at + left * local.x + v.facing * local.z;
    // The whole model stands at the height under its base; it is not draped.
    let ground = max(surface_height(v.at), g.misc.x - 0.05);
    var sway = vec2<f32>(0.0);
    // Rocks and bases do not move: skip the wind for them.
    if v.color.a > 0.0 {
        sway = gust(v.at, v.tint.a) * (v.color.a * SWAY_MAX * v.scale);
    }
    return vec3<f32>(flat_xz.x + sway.x, ground + local.y, flat_xz.y + sway.y);
}

@vertex
fn vs_scenery(v: SceneryIn) -> SceneryOut {
    let world = place(v);
    let left = vec2<f32>(v.facing.y, -v.facing.x);
    let n_xz = left * v.normal.x + v.facing * v.normal.z;
    var out: SceneryOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = normalize(vec3<f32>(n_xz.x, v.normal.y, n_xz.y) + vec3<f32>(0.0, 1e-4, 0.0));
    out.color = min(v.color.rgb * v.tint.rgb * 2.0, vec3<f32>(1.0));
    out.leafy = select(0.0, 1.0, v.color.a > 0.0);
    return out;
}

// Position only, for the shadow map.
@vertex
fn vs_scenery_depth(v: SceneryIn) -> @builtin(position) vec4<f32> {
    return g.view_proj * vec4<f32>(place(v), 1.0);
}

@fragment
fn fs_scenery(in: SceneryOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // A little colour noise by place, so a wood is not one flat green.
    let wobble = vnoise(in.world.xz * 1.7 + vec2<f32>(in.world.y * 0.9, 0.0));
    let albedo = srgb_to_linear(in.color) * (0.91 + 0.18 * wobble);
    // Leaves and blades may be single planes: light them from the side shown.
    let n = select(-in.normal, in.normal, front);
    var c = shade(albedo, n, in.world);
    // Leaves and blades pass light on: with the sun behind them they glow,
    // yellower than they are lit from the front.
    let behind = max(dot(-n, g.sun_dir.xyz), 0.0);
    if in.leafy > 0.5 && behind > 0.0 {
        let glow = albedo * vec3<f32>(1.2, 1.1, 0.7) * g.sun_color.rgb * behind * sunlight(in.world, -n);
        c += glow * g.sun_color.a * (1.0 - air_amount(in.world, distance(in.world, g.camera_pos.xyz)));
    }
    return vec4<f32>(veil(c, in.world.xz), 1.0);
}
