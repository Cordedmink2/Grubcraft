// World shaders: terrain, regiment boxes and soldiers, lit by the sun (with
// its shadow map) and the sky, in linear HDR. post.wgsl adds the glow and
// tone-maps the result for the screen.
//
// Neither pipeline uses a vertex buffer for geometry: terrain vertices are
// generated from the vertex index over a shared chunk grid, and heights come
// from an R16Uint heightmap; regiment boxes are generated from the vertex index.
// Only per-instance data (chunk coords and level of detail, regiment
// transforms) is uploaded.

struct Globals {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    // xyz: direction toward the sun (normalised)
    sun_dir: vec4<f32>,
    // x: heightmap samples per tile, y: height of the max u16 value in tiles,
    // z: tiles per chunk side, w: quads per chunk side at level 0
    map: vec4<f32>,
    // rgb: fog colour, a: fog density
    fog: vec4<f32>,
    // x: water level in tiles, y: time in seconds
    misc: vec4<f32>,
    // World to the sun's shadow map (x, y in [-1, 1], z the depth).
    sun_view_proj: mat4x4<f32>,
    // x: how far to push the lookup out along the surface normal (tiles),
    // y: 1 with shadows on, z: where shadows start fading toward the map's
    // edge (`SHADOW_FADE` in shadows.rs)
    shadow: vec4<f32>,
    // The camera's view backwards, for the sky.
    inv_view_proj: mat4x4<f32>,
    // The `Look` (look.rs). rgb: the sun's light (HDR), a: how much
    // leaves and grass glow with the sun behind them
    sun_color: vec4<f32>,
    // rgb: light from the sky, a: rim light on soldiers and buildings
    sky_ambient: vec4<f32>,
    // rgb: light from the ground, a: how dark cloud shadows are
    ground_ambient: vec4<f32>,
    // rgb: air colour toward the sun (HDR)
    haze: vec4<f32>,
    // x: how quickly the air thins with height, y: ground occlusion strength,
    // z: how thick the air is at the camera, relative to sea level
    // (exp(-x * camera height))
    air: vec4<f32>,
    sky_top: vec4<f32>,
    sky_horizon: vec4<f32>,
};

// Every lit varying a fragment shader reads is `@interpolate(perspective,
// centroid)` (flat ones aside). With multisampling a pixel on the edge of a
// thin triangle is otherwise shaded at a point outside it, where a normal or
// a position can be extrapolated to nonsense or not-a-number, and the glow
// would spread that over the screen. Keep to this for any new varying.

// The prevailing wind blows toward this direction (x east, y south).
const WIND_DIR = vec2<f32>(0.8, 0.6);

// The selection highlight's strength. It is added before tone mapping,
// which flattens it, so it is twice what it was when added after.
const HIGHLIGHT: f32 = 1.6;

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var heightmap: texture_2d<u32>;
@group(0) @binding(2) var shadow_map: texture_depth_2d;
@group(0) @binding(3) var shadow_sampler: sampler_comparison;
// Fog of war: one texel per tile, 1 in view, about 0.55 seen before, about
// 0.2 never seen. All 1 when the match has no fog.
@group(0) @binding(4) var fog_map: texture_2d<f32>;
// Linear: soft fog edges, the ground tint blended between texels, and the
// ground occlusion.
@group(0) @binding(5) var soft_sampler: sampler;
// The grass colour by the land it grows in (sRGB, alpha 1 where the map has
// lands), a texel per few tiles over the whole map; one clear texel if not.
@group(0) @binding(6) var ground_tint: texture_2d<f32>;
// Rock035, mipmapped (rock_textures in lib.rs): rgb albedo (sRGB, decoded on
// read) with the roughness in alpha, and the OpenGL tangent-space normal.
@group(0) @binding(7) var rock_color: texture_2d<f32>;
@group(0) @binding(8) var rock_normal: texture_2d<f32>;
// Repeating, anisotropic.
@group(0) @binding(9) var rock_sampler: sampler;
// Cliffs, a texel per half tile: 0 on and around every cell soldiers can
// stand on, rising to 1 three cells into ground too steep to climb (set_terrain
// builds it from gc_map's cliff cells). Relief is weighted by it.
@group(0) @binding(10) var cliff_mask: texture_2d<f32>;
// Ground occlusion, a texel per tile baked from the heights (look.rs): 1 on
// open ground, less in valleys and at the foot of slopes.
@group(0) @binding(11) var ground_occlusion: texture_2d<f32>;
// Grass001 (layer 0) and Ground054 (layer 1), mipmapped (ground.rs): rgb
// albedo (sRGB, decoded on read), and the OpenGL tangent-space normals. They
// share rock_sampler.
@group(0) @binding(12) var ground_color: texture_2d_array<f32>;
@group(0) @binding(13) var ground_normal: texture_2d_array<f32>;
// The same for scenery (scenery.wgsl): Bark001 (layer 0), Bark014 (1) and
// Moss002 (2).
@group(0) @binding(14) var scenery_color: texture_2d_array<f32>;
@group(0) @binding(15) var scenery_normal: texture_2d_array<f32>;
// Where a faction's land has spread over the ground (Renderer::set_land), one
// texel a tile: r how much of it is covered, g whose (a `LAND_*` kind), b the
// distance to its nearest source in quarter tiles.
@group(0) @binding(16) var land_map: texture_2d<f32>;
// How the ground has been marked (Renderer::set_scars), one texel a tile: r
// felled, g scorched, b worn by marching and fighting.
@group(0) @binding(17) var scar_map: texture_2d<f32>;
// How each tile's ground looks (Terrain::look), one texel a tile: rgb multiplies
// the grass (128 leaves it), a is the `GROUND_*` pattern drawn over it.
@group(0) @binding(18) var ground_look: texture_2d<f32>;

fn sample_height(p: vec2<i32>) -> f32 {
    let size = vec2<i32>(textureDimensions(heightmap));
    let c = clamp(p, vec2<i32>(0, 0), size - vec2<i32>(1, 1));
    return f32(textureLoad(heightmap, c, 0).r) * (g.map.y / 65535.0);
}

// Bilinear terrain height at a position in tiles (x east, z south).
fn height_at(xz: vec2<f32>) -> f32 {
    let s = xz * g.map.x;
    let i = vec2<i32>(floor(s));
    let f = fract(s);
    let h00 = sample_height(i);
    let h10 = sample_height(i + vec2<i32>(1, 0));
    let h01 = sample_height(i + vec2<i32>(0, 1));
    let h11 = sample_height(i + vec2<i32>(1, 1));
    return mix(mix(h00, h10, f.x), mix(h01, h11, f.x), f.y);
}

// The height of the ripples at `q`: two layers of noise drifting apart.
fn ripple(q: vec2<f32>, t: f32) -> f32 {
    return vnoise(q) + 0.5 * vnoise(q * 2.3 - vec2<f32>(t * 0.5, 0.0));
}

// The surface of water at `xz`: gentle ripples drifting with time, flatter
// the farther away (`calm` is 1 up close, toward 0 far off).
fn water_normal(xz: vec2<f32>, t: f32, calm: f32) -> vec3<f32> {
    let e = 0.15;
    let q = xz * 2.2 + vec2<f32>(t * 0.35, t * 0.21);
    let h = ripple(q, t);
    let hx = ripple(q + vec2<f32>(e, 0.0), t);
    let hz = ripple(q + vec2<f32>(0.0, e), t);
    let slope = vec2<f32>(hx - h, hz - h) / e * 0.09 * calm;
    return normalize(vec3<f32>(-slope.x, 1.0, -slope.y));
}

// Dim and drain the colour of ground out of sight. Tile (i, j) is the texel
// at (i, j), so a position in tiles divided by the map size is its uv.
fn seen_at(xz: vec2<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(fog_map));
    return textureSampleLevel(fog_map, soft_sampler, xz / size, 0.0).r;
}

fn veil(c: vec3<f32>, xz: vec2<f32>) -> vec3<f32> {
    let seen = seen_at(xz);
    let out = clamp((1.0 - seen) / 0.8, 0.0, 1.0);
    let grey = vec3<f32>(dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)));
    return mix(c, grey, out * 0.5) * (0.1 + 0.9 * seen);
}

fn hash2(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(0.1031, 0.1030));
    let r = q + dot(q, q.yx + 33.33);
    return fract((r.x + r.y) * r.x);
}

// Smooth value noise, for painterly colour variation without visible cells.
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return pow(c, vec3<f32>(2.2));
}

fn ambient_light(n: vec3<f32>) -> vec3<f32> {
    return mix(g.ground_ambient.rgb, g.sky_ambient.rgb, n.y * 0.5 + 0.5);
}

// How much of the way to the air's colour a point is, 0 to 1. The air is
// thick low down and thin on the hills (its density falls off with height,
// integrated along the view ray).
fn air_amount(world: vec3<f32>, dist: f32) -> f32 {
    let k = max(g.air.x, 1e-4);
    let dy = world.y - g.camera_pos.y;
    var along = 1.0;
    if abs(dy) > 0.01 {
        along = (1.0 - exp(-k * dy)) / (k * dy);
    }
    return clamp(1.0 - exp(-dist * g.fog.a * g.air.z * along), 0.0, 1.0);
}

// Fade lit colour into the air, which is warmer toward the sun. Its colours
// are in the HDR scale.
fn finish(c: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let to = world - g.camera_pos.xyz;
    let dist = length(to);
    let facing = max(dot(to / max(dist, 1e-3), g.sun_dir.xyz), 0.0);
    let f2 = facing * facing;
    let f4 = f2 * f2;
    let toward = 0.8 * f4 * f4;
    return mix(c, mix(g.fog.rgb, g.haze.rgb, toward), air_amount(world, dist));
}

// Soft cloud shadows drifting over the land.
fn clouds(xz: vec2<f32>) -> f32 {
    if g.ground_ambient.a <= 0.0 {
        return 1.0;
    }
    let p = xz * 0.018 + vec2<f32>(0.9, 0.35) * g.misc.y * 0.02;
    let n = vnoise(p) * 0.6 + vnoise(p * 2.3 + vec2<f32>(5.0, 1.0)) * 0.3 + vnoise(p * 5.1) * 0.1;
    return 1.0 - g.ground_ambient.a * smoothstep(0.45, 0.68, n);
}

// A back-light rim that lifts figures off the ground: strongest where the
// sun is behind the figure.
fn rim(n: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let v = normalize(g.camera_pos.xyz - world);
    let edge = 1.0 - max(dot(n, v), 0.0);
    let f = edge * edge * edge;
    let back = 0.35 + 0.65 * max(dot(-v, g.sun_dir.xyz), 0.0);
    return (g.sun_color.rgb * back * 0.6 + g.sky_ambient.rgb) * f * g.sky_ambient.a;
}

// How much sunlight reaches a surface: 0 in shadow, 1 in full sun. Eight
// filtered taps soften the edge over about three shadow texels, and the
// shadow fades out at the edge of the area the map covers.
fn sunlight_taps(world: vec3<f32>, n: vec3<f32>, taps: u32) -> f32 {
    if g.shadow.y < 0.5 {
        return clouds(world.xz);
    }
    let c = g.sun_view_proj * vec4<f32>(world + n * g.shadow.x, 1.0);
    let edge = max(abs(c.x), abs(c.y));
    if edge >= 1.0 || c.z >= 1.0 {
        return clouds(world.xz);
    }
    let uv = c.xy * vec2<f32>(0.5, -0.5) + 0.5;
    // Eight filtered taps spread over a disc about three texels across
    // give a soft edge (a fixed pattern, so a moving shadow does not
    // crawl). Light that only tints (a glow, a particle) takes the one tap
    // in the middle.
    var lit = 0.0;
    if taps == 1u {
        lit = textureSampleCompareLevel(shadow_map, shadow_sampler, uv, c.z);
    } else {
        let reach = SHADOW_SOFTNESS / f32(textureDimensions(shadow_map).x);
        for (var i = 0u; i < 8u; i++) {
            lit += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + SHADOW_SPIRAL[i] * reach, c.z);
        }
        lit *= 0.125;
    }
    return mix(lit, 1.0, smoothstep(g.shadow.z, 1.0, edge)) * clouds(world.xz);
}

// The radius of the shadow's soft edge, in shadow-map texels, and eight
// points spread evenly over the unit disc (a sunflower spiral).
const SHADOW_SOFTNESS: f32 = 1.6;
const SHADOW_SPIRAL = array<vec2<f32>, 8>(
    vec2<f32>(0.2500, 0.0000),
    vec2<f32>(-0.3193, 0.2925),
    vec2<f32>(0.0489, -0.5569),
    vec2<f32>(0.4024, 0.5249),
    vec2<f32>(-0.7385, -0.1306),
    vec2<f32>(0.6996, -0.4450),
    vec2<f32>(-0.2340, 0.8705),
    vec2<f32>(-0.4463, -0.8593)
);

// Sunlight with the full soft edge, and with one filtered tap.
fn sunlight(world: vec3<f32>, n: vec3<f32>) -> f32 {
    return sunlight_taps(world, n, 8u);
}

fn sunlight_soft(world: vec3<f32>, n: vec3<f32>) -> f32 {
    return sunlight_taps(world, n, 1u);
}

// Direct sunlight on a surface facing `n`: the cosine term, shadowed.
fn sun_ndl(n: vec3<f32>, world: vec3<f32>) -> f32 {
    let ndl = max(dot(n, g.sun_dir.xyz), 0.0);
    if ndl > 0.0 {
        return ndl * sunlight(world, n);
    }
    return 0.0;
}

fn shade(albedo: vec3<f32>, n: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let ndl = sun_ndl(n, world);
    return finish(albedo * (g.sun_color.rgb * ndl + ambient_light(n)), world);
}

// Textured surfaces: diffuse plus a normalised Blinn-Phong highlight whose
// size follows roughness, metals tinting their highlight by the albedo,
// occlusion darkening the ambient light, and emitted light on top.
fn shade_material(
    albedo: vec3<f32>,
    n: vec3<f32>,
    world: vec3<f32>,
    rough: f32,
    metal: f32,
    ao: f32,
    emit: vec3<f32>,
) -> vec3<f32> {
    let sun = g.sun_dir.xyz;
    let ndl = sun_ndl(n, world);
    let v = normalize(g.camera_pos.xyz - world);
    let h = normalize(sun + v);
    let power = exp2(11.0 * (1.0 - rough) + 1.0);
    let f0 = mix(vec3<f32>(0.04), albedo, metal);
    let fresnel = f0 + (1.0 - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
    let spec = fresnel * pow(max(dot(n, h), 0.0), power) * (power + 8.0) / 25.13;
    let ambient = ambient_light(n) * ao;
    let diffuse = albedo * (1.0 - metal);
    let c = diffuse * (g.sun_color.rgb * ndl + ambient) + spec * g.sun_color.rgb * ndl + f0 * ambient * 0.6;
    return finish(c + emit, world);
}

// ---------------------------------------------------------------- relief
//
// The simulation's heightmap draws mountains as straight ramps. Where the
// ground is a cliff no soldier can climb, the renderer adds relief on top:
// crags, spires and uneven crests, and an irregular edge (a horizontal warp
// of the lookup). `cliff_mask` says where: it is built from the same cells
// the simulation's pathfinding blocks for being too steep, and is 0 on every
// other cell, at its border and a cell beyond it, so ground units, buildings
// and scenery stand on (`height_at`, also on the CPU) is untouched. The
// relief is a pure function of the position, the heightmap and the mask,
// so the vertex shader and the per-pixel normal agree, and it only ever
// raises the ground. SMALL_RELIEF, CRAG_RELIEF, SPIRE_RELIEF and WARP come
// from rock.rs, which also bounds the height they can reach (RELIEF_MAX).

// Value noise stretched to use its whole range.
fn nz(p: vec2<f32>) -> f32 {
    return clamp((vnoise(p) - 0.5) * 1.7 + 0.5, 0.0, 1.0);
}

fn fbm(p: vec2<f32>) -> f32 {
    var q = p;
    var a = 0.5;
    var sum = 0.0;
    for (var i = 0; i < 4; i++) {
        sum += a * nz(q);
        q = mat2x2<f32>(1.6, -1.2, 1.2, 1.6) * q + vec2<f32>(7.1, 3.3);
        a *= 0.5;
    }
    return sum / 0.9375;
}

// Sharp-creased noise in 0..1: 1 along ridges, 0 in the creases between.
fn ridged(p: vec2<f32>) -> f32 {
    var q = p;
    var a = 0.6;
    var sum = 0.0;
    for (var i = 0; i < 3; i++) {
        let r = 1.0 - abs(2.0 * nz(q) - 1.0);
        sum += a * r * r;
        q = mat2x2<f32>(1.6, -1.2, 1.2, 1.6) * q + vec2<f32>(3.9, 8.7);
        a *= 0.5;
    }
    return clamp(sum / 0.875, 0.0, 1.0);
}

// How much relief the ground at `xz` gets: 0 off the cliffs, 1 well into
// them. One read.
fn cliff_weight(xz: vec2<f32>) -> f32 {
    let tiles = f32(textureDimensions(cliff_mask).x) * 0.5;
    return smoothstep(0.0, 1.0, textureSampleLevel(cliff_mask, soft_sampler, xz / tiles, 0.0).r);
}

// How much the ground stands above what is around it, from the
// simulation's heights: 0 at a hill's foot, 1 on a mountain, which is
// where peaks grow tall. Only asked where there is relief.
fn mountain_mass(xz: vec2<f32>, h0: f32) -> f32 {
    let r = 8.0;
    let ring = 0.25 * (
        height_at(xz + vec2<f32>(r, 0.0)) + height_at(xz - vec2<f32>(r, 0.0))
        + height_at(xz + vec2<f32>(0.0, r)) + height_at(xz - vec2<f32>(0.0, r))
    );
    return smoothstep(0.8, 3.0, h0 - ring);
}

// The raised parts of the relief before the weight: x small bumps, y crags
// and spires (0..1 each).
fn relief_parts(xz: vec2<f32>) -> vec2<f32> {
    let small = ridged(xz * 0.45 + vec2<f32>(7.3, 1.9));
    let crag = pow(fbm(xz * 0.2 + vec2<f32>(11.0, 5.0)), 1.5);
    let spire = pow(ridged(xz * 0.11 + vec2<f32>(3.7, 9.1)), 3.0);
    return vec2<f32>(small, crag * (CRAG_RELIEF / (CRAG_RELIEF + SPIRE_RELIEF)) + spire * (SPIRE_RELIEF / (CRAG_RELIEF + SPIRE_RELIEF)));
}

// How far the lookup slides, in tiles.
fn relief_warp(xz: vec2<f32>) -> vec2<f32> {
    return (vec2<f32>(nz(xz * 0.3), nz(xz * 0.3 + vec2<f32>(17.0, 41.0))) - 0.5) * (2.0 * WARP);
}

// The drawn height at `p`, where the ground is `h0` (`height_at`), the
// cliff weight `w`, the mountain mass `mass`, the lookup slides by `warp`
// and the raised parts are `parts`: `h0` itself where `w` is 0, and never
// below it.
fn relief_at(p: vec2<f32>, h0: f32, w: f32, mass: f32, warp: vec2<f32>, parts: vec2<f32>) -> f32 {
    let slid = max(height_at(p + warp) - h0, 0.0);
    return h0 + w * (slid + SMALL_RELIEF * parts.x + mass * (CRAG_RELIEF + SPIRE_RELIEF) * parts.y);
}

// The height terrain is drawn at, and scenery stands on: `height_at`,
// except on cliffs. Flat ground costs one mask read.
fn surface_height(xz: vec2<f32>) -> f32 {
    let h0 = height_at(xz);
    let w = cliff_weight(xz);
    if w <= 0.0 {
        return h0;
    }
    return relief_at(xz, h0, w, mountain_mass(xz, h0), relief_warp(xz), relief_parts(xz));
}

// ---------------------------------------------------------------- terrain

struct TerrainOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};

// Snap `i`, a position along a chunk edge counted in level 0 quads, down to
// the nearest vertex of a neighbour at level `level`.
fn snap_to_level(i: u32, level: u32) -> u32 {
    return (i >> level) << level;
}

// `levels` packs the chunk's level of detail (bits 0-1) and its west, east,
// north and south neighbours' levels (bits 2-3, 4-5, 6-7, 8-9). Level n has
// half the quads per side of level n-1, so its vertices are every second
// vertex of the finer grid. Where a neighbour is coarser, this chunk's edge
// vertices snap onto the neighbour's vertices, so the two edges are the same
// line and leave no crack; the triangles that collapse draw nothing.
@vertex
fn vs_terrain(
    @builtin(vertex_index) vi: u32,
    @location(0) chunk: vec2<u32>,
    @location(1) levels: u32,
) -> TerrainOut {
    let level = levels & 3u;
    let full = u32(g.map.w);
    let quads = full >> level;
    let side = quads + 1u;
    let ix = vi % side;
    let iz = vi / side;
    // Position in level 0 quads from the chunk's corner.
    var p = vec2<u32>(ix << level, iz << level);
    let west = (levels >> 2u) & 3u;
    let east = (levels >> 4u) & 3u;
    let north = (levels >> 6u) & 3u;
    let south = (levels >> 8u) & 3u;
    if ix == 0u && west > level {
        p.y = snap_to_level(p.y, west);
    }
    if ix == quads && east > level {
        p.y = snap_to_level(p.y, east);
    }
    if iz == 0u && north > level {
        p.x = snap_to_level(p.x, north);
    }
    if iz == quads && south > level {
        p.x = snap_to_level(p.x, south);
    }
    let chunk_tiles = g.map.z;
    let xz = vec2<f32>(chunk) * chunk_tiles + vec2<f32>(p) * (chunk_tiles / f32(full));
    let world = vec3<f32>(xz.x, surface_height(xz), xz.y);
    var out: TerrainOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    return out;
}

// Rock035 repeats every 1 / ROCK_SCALE tiles.
const ROCK_SCALE: f32 = 0.2;

struct Rock {
    albedo: vec3<f32>,
    rough: f32,
    n: vec3<f32>,
};

// One planar projection of Rock035: its colour and roughness, and its normal
// in whiteout form for a surface whose normal has components `ng2` along the
// two axes of the plane and `ngn` across it. `dx` and `dy` are the plane
// coordinates' screen derivatives. Image rows run down and the normal map's
// green runs up, so callers negate the second plane coordinate.
struct Plane {
    c: vec4<f32>,
    tn: vec3<f32>,
};

fn rock_plane(uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, ng2: vec2<f32>, ngn: f32) -> Plane {
    var r: Plane;
    r.c = textureSampleGrad(rock_color, rock_sampler, uv * ROCK_SCALE, dx * ROCK_SCALE, dy * ROCK_SCALE);
    let t = textureSampleGrad(rock_normal, rock_sampler, uv * ROCK_SCALE, dx * ROCK_SCALE, dy * ROCK_SCALE).xyz * 2.0 - 1.0;
    r.tn = vec3<f32>(t.xy + ng2, abs(t.z) * ngn);
    return r;
}

// How light or dark Rock035's albedo is, relative to its mean, as a factor for
// a stone colour: its own colour is near black (mean 0.01 linear), so it gives
// the pattern only. Cliffs and scenery rocks share this, to look alike.
fn rock_pattern(albedo: vec3<f32>) -> f32 {
    let l = dot(albedo, vec3<f32>(0.2126, 0.7152, 0.0722));
    return pow(clamp(l / 0.0095, 0.02, 6.0), 0.45);
}

// A detail texture's colour ratio mostly as light and dark: its own hues would
// fight the colour it is laid over. `chroma` is how much of the hue is kept.
fn muted(ratio: vec3<f32>, chroma: f32) -> vec3<f32> {
    return mix(vec3<f32>(dot(ratio, vec3<f32>(0.2126, 0.7152, 0.0722))), ratio, chroma);
}

// Triplanar Rock035 at `p` on a surface facing `ng`: the three projections
// weighted by how squarely they face the surface, each normal map blended in
// whiteout style. `dpx` and `dpy` are the screen derivatives of `p`, taken
// by the caller in uniform control flow so that projections can be skipped.
fn rock_surface(p: vec3<f32>, ng: vec3<f32>, dpx: vec3<f32>, dpy: vec3<f32>) -> Rock {
    var w = pow(abs(ng), vec3<f32>(4.0));
    w = w / (w.x + w.y + w.z);
    var col = vec4<f32>(0.0);
    var nn = vec3<f32>(0.0);
    var used = 0.0;
    if w.x > 0.04 {
        let q = rock_plane(vec2<f32>(p.z, -p.y), vec2<f32>(dpx.z, -dpx.y), vec2<f32>(dpy.z, -dpy.y), ng.zy, ng.x);
        nn += q.tn.zyx * w.x;
        col += q.c * w.x;
        used += w.x;
    }
    if w.y > 0.04 {
        let q = rock_plane(vec2<f32>(p.x, -p.z), vec2<f32>(dpx.x, -dpx.z), vec2<f32>(dpy.x, -dpy.z), ng.xz, ng.y);
        nn += q.tn.xzy * w.y;
        col += q.c * w.y;
        used += w.y;
    }
    if w.z > 0.04 {
        let q = rock_plane(vec2<f32>(p.x, -p.y), vec2<f32>(dpx.x, -dpx.y), vec2<f32>(dpy.x, -dpy.y), ng.xy, ng.z);
        nn += q.tn.xyz * w.z;
        col += q.c * w.z;
        used += w.z;
    }
    var r: Rock;
    r.albedo = col.rgb / used;
    r.rough = col.a / used;
    r.n = normalize(nn);
    return r;
}

// ---------------------------------------------------------------- ground
//
// The painted palette keeps the large-scale colour (the land's grass, the dry
// ridges, the soil) and the ground textures put the small scale on it, as the
// rock does: a texture's colour relative to its own mean multiplies the
// palette, and its normal map tilts the surface. Each repeats every
// 1 / GROUND_SCALE tiles; a second lookup, turned and stretched, is blended in
// by a slow noise so the repeat is not seen, the blend keeping the texture's
// contrast.

const GROUND_SCALE: f32 = 0.25;
// Linear mean colours of the two albedo maps, to turn them into ratios.
const GRASS_MEAN: vec3<f32> = vec3<f32>(0.0603, 0.1091, 0.0226);
const SOIL_MEAN: vec3<f32> = vec3<f32>(0.3331, 0.2588, 0.1562);

struct GroundLayer {
    ratio: vec3<f32>,
    // The normal map's x and y (z is world +z in the ground's plane), at 0 for flat.
    tilt: vec2<f32>,
};

fn ground_sample(layer: i32, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, mean: vec3<f32>) -> GroundLayer {
    var r: GroundLayer;
    r.ratio = min(textureSampleGrad(ground_color, rock_sampler, uv, layer, dx, dy).rgb / mean, vec3<f32>(3.0));
    r.tilt = textureSampleGrad(ground_normal, rock_sampler, uv, layer, dx, dy).xy * 2.0 - 1.0;
    return r;
}

// How much of the turned lookup `ground_layer` uses at world position `p`
// (x, z): a slow noise, exactly 0 or 1 over much of the ground.
fn ground_blend(p: vec2<f32>) -> f32 {
    return smoothstep(0.3, 0.7, vnoise(p * 0.045 + vec2<f32>(5.0, 17.0)));
}

// The second lookup is turned about 52 degrees and stretched 2.7 times.
const GROUND_TURN: mat2x2<f32> = mat2x2<f32>(0.62, 0.78, -0.78, 0.62);
const GROUND_STRETCH: f32 = 0.37;

// `p` has screen derivatives `dx` and `dy`; `m` is `ground_blend(p)`.
fn ground_layer(layer: i32, mean: vec3<f32>, p: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, m: f32) -> GroundLayer {
    // Image rows run down and the normal map's green runs up, so v is -z.
    let flip = vec2<f32>(1.0, -1.0);
    let uv = p * flip * GROUND_SCALE;
    let duv_x = dx * flip * GROUND_SCALE;
    let duv_y = dy * flip * GROUND_SCALE;
    // Where the blend is 0 or 1 only one lookup is needed.
    var a = GroundLayer(vec3<f32>(1.0), vec2<f32>(0.0));
    if m < 1.0 {
        a = ground_sample(layer, uv, duv_x, duv_y, mean);
    }
    var b = GroundLayer(vec3<f32>(1.0), vec2<f32>(0.0));
    if m > 0.0 {
        let turn = GROUND_TURN * GROUND_STRETCH;
        b = ground_sample(layer, turn * uv + vec2<f32>(0.31, 0.57), turn * duv_x, turn * duv_y, mean);
        // Its tilt turns back with it (in the frame whose v runs up, which
        // is the turn itself, not its inverse).
        b.tilt = GROUND_TURN * b.tilt;
    }
    // Variance-preserving blend about the mean (1 for colour, 0 for tilt).
    let k = 1.0 / sqrt((1.0 - m) * (1.0 - m) + m * m);
    var r: GroundLayer;
    r.ratio = max(1.0 + (mix(a.ratio, b.ratio, m) - 1.0) * k, vec3<f32>(0.0));
    r.tilt = mix(a.tilt, b.tilt, m) * k;
    return r;
}

// ---------------------------------------------------------------- habitat ground
//
// What a habitat's ground is like, past the colour it gives the grass: bog
// with pools of black water, mud, flowers in a meadow, heather, gravel, crazed
// crust, ripples of bone dust, and so on. Worked out from the position alone;
// the map decides where (`Terrain::look`). Fine detail fades with `px`, the
// tiles a pixel covers.

struct Habitat {
    albedo: vec3<f32>,
    // 0 none, 1 covers the ground.
    k: f32,
    // 0 dry, 1 standing water: glossy.
    wet: f32,
};

fn habitat_ground(pattern: u32, xz: vec2<f32>, px: f32) -> Habitat {
    var r = Habitat(vec3<f32>(0.0), 0.0, 0.0);
    let fine = 1.0 - smoothstep(0.08, 0.2, px);
    switch pattern {
        case GROUND_BOG: {
            // Hummocks of dark peat between pools of black water.
            let pool = smoothstep(0.5, 0.62, fbm(xz * 0.3 + vec2<f32>(11.0, 4.0)));
            let peat = srgb_to_linear(vec3<f32>(0.2, 0.15, 0.1)) * (0.7 + 0.6 * nz(xz * 1.9));
            r.albedo = mix(peat, srgb_to_linear(vec3<f32>(0.08, 0.12, 0.13)), pool);
            r.k = mix(0.6, 0.92, pool);
            r.wet = pool;
        }
        case GROUND_MUD: {
            // Trampled, sodden mud in patches.
            let m = smoothstep(0.45, 0.65, fbm(xz * 0.45 + vec2<f32>(2.0, 19.0)));
            r.albedo = srgb_to_linear(vec3<f32>(0.24, 0.19, 0.11)) * (0.8 + 0.4 * nz(xz * 2.0));
            r.k = m * 0.7;
            r.wet = m * 0.6;
        }
        case GROUND_DAMP: {
            let m = smoothstep(0.5, 0.7, fbm(xz * 0.4 + vec2<f32>(8.0, 3.0)));
            r.albedo = srgb_to_linear(vec3<f32>(0.2, 0.26, 0.13));
            r.k = m * 0.4;
            r.wet = m * 0.25;
        }
        case GROUND_FLOWERS: {
            // Drifts of flowers: one in the middle of each cell of a grain
            // two thirds of a tile, in the half that fall in a drift, over a
            // faint wash of colour.
            let grain = xz * 1.6;
            let cell = floor(grain);
            let drift = smoothstep(0.3, 0.55, fbm(xz * 0.25 + vec2<f32>(14.0, 6.0)));
            let pick = hash2(cell + vec2<f32>(5.0, 1.0));
            var petal = srgb_to_linear(vec3<f32>(0.95, 0.85, 0.2));
            if pick > 0.66 {
                petal = srgb_to_linear(vec3<f32>(0.95, 0.93, 0.88));
            } else if pick > 0.33 {
                petal = srgb_to_linear(vec3<f32>(0.82, 0.4, 0.66));
            }
            let off = (vec2<f32>(hash2(cell + vec2<f32>(3.0, 9.0)), hash2(cell + vec2<f32>(13.0, 2.0))) - 0.5) * 0.4;
            let bloom = 1.0 - smoothstep(0.1, 0.22, length(fract(grain) - 0.5 - off));
            // The wash is one colour across the drift; only the blooms take
            // their cell's petal, or each cell would show as a tinted square.
            let flower = bloom * step(0.4, hash2(cell)) * (0.5 + 0.5 * fine);
            let wash = srgb_to_linear(vec3<f32>(0.62, 0.6, 0.3));
            r.albedo = mix(wash, petal, smoothstep(0.0, 0.3, flower));
            r.k = max(flower, drift * 0.12) * (0.4 + 0.6 * drift);
        }
        case GROUND_HEATHER: {
            let patch_ = smoothstep(0.45, 0.65, fbm(xz * 0.35 + vec2<f32>(4.0, 13.0)));
            let purple = srgb_to_linear(vec3<f32>(0.42, 0.22, 0.4));
            let dry = srgb_to_linear(vec3<f32>(0.5, 0.42, 0.25));
            r.albedo = mix(dry, purple, patch_);
            r.k = 0.35 + 0.4 * patch_;
        }
        case GROUND_GRAVEL: {
            let stone = srgb_to_linear(vec3<f32>(0.5, 0.46, 0.4));
            let pebble = 0.6 + 0.8 * nz(xz * 3.1);
            r.albedo = stone * pebble * mix(1.0, 0.7, step(0.7, nz(xz * 7.0)) * fine);
            r.k = 0.8;
        }
        case GROUND_CRUST: {
            // Pale crust crazed with dark cracks.
            let crack = crease(xz * 0.9 + vec2<f32>(6.0, 2.0), 0.075) * (0.5 + 0.5 * fine);
            let crust = srgb_to_linear(vec3<f32>(0.62, 0.56, 0.45)) * (0.85 + 0.3 * nz(xz * 1.7));
            r.albedo = mix(crust, srgb_to_linear(vec3<f32>(0.2, 0.17, 0.13)), crack);
            r.k = 0.85;
        }
        case GROUND_RIPPLE: {
            // Bone dust combed into ripples by the wind.
            let rip = 0.5 + 0.5 * sin(dot(xz, vec2<f32>(0.8, 0.6)) * 5.0 + 3.0 * fbm(xz * 0.25));
            r.albedo = srgb_to_linear(vec3<f32>(0.78, 0.74, 0.62)) * (0.85 + 0.2 * mix(0.5, rip, fine));
            r.k = 0.8;
        }
        case GROUND_STRAW: {
            let m = smoothstep(0.35, 0.6, fbm(xz * 0.3 + vec2<f32>(9.0, 5.0)));
            r.albedo = srgb_to_linear(vec3<f32>(0.6, 0.52, 0.3)) * (0.85 + 0.3 * nz(xz * 1.5));
            r.k = m * 0.65;
        }
        case GROUND_LITTER: {
            // Leaf litter and moss under the trees.
            let litter = smoothstep(0.42, 0.62, fbm(xz * 0.6 + vec2<f32>(1.0, 7.0)));
            let moss = smoothstep(0.55, 0.75, fbm(xz * 0.4 + vec2<f32>(17.0, 3.0)));
            let leaf = srgb_to_linear(vec3<f32>(0.3, 0.2, 0.1)) * (0.8 + 0.4 * nz(xz * 2.4));
            r.albedo = mix(leaf, srgb_to_linear(vec3<f32>(0.14, 0.24, 0.1)), moss);
            r.k = max(litter * 0.45, moss * 0.5);
        }
        case GROUND_BONE: {
            r.albedo = srgb_to_linear(vec3<f32>(0.78, 0.74, 0.62)) * (0.8 + 0.3 * nz(xz * 2.5));
            r.k = 0.7;
        }
        case GROUND_MOSS: {
            r.albedo = srgb_to_linear(vec3<f32>(0.12, 0.3, 0.12));
            r.k = smoothstep(0.4, 0.65, fbm(xz * 0.5 + vec2<f32>(5.0, 21.0))) * 0.55;
        }
        default: {}
    }
    return r;
}

// ---------------------------------------------------------------- faction lands
//
// Where a faction's land has spread, the ground is that faction's. Each land is
// worked out from the position alone, so there are no textures to tile, and
// the simulation decides where it is; this only draws it. Every land comes in
// by a ragged edge (tongues and islands, not a disc), then:
//   Brood: living flesh with a network of veins, thickest by its source, and a
//     glow that pulses inwards, the way the Grub flows home.
//   Ember Clans: tilled earth in patches of furrows, each field turned its own
//     way, between yards of trodden earth.
//   Shard Choir: frosted lavender ground with pale cracks, glinting crystal
//     dust and faint rings that spread out from its fields of power.
//   Carrion Concord: grey ash, cracked, with flecks of bone.

struct Land {
    albedo: vec3<f32>,
    // 0 bare ground, 1 fully covered.
    cover: f32,
    // Added to the surface normal's x and z, and emitted light (HDR): both
    // are given for full cover and scaled by it in `faction_land`.
    tilt: vec2<f32>,
    glow: vec3<f32>,
    rough: f32,
};

const NO_LAND = Land(vec3<f32>(0.0), 0.0, vec2<f32>(0.0), vec3<f32>(0.0), 1.0);

// 1 along a crease of the noise `nz` at `p`, 0 off it, `width` wide.
fn crease(p: vec2<f32>, width: f32) -> f32 {
    return 1.0 - smoothstep(0.0, width, abs(nz(p) - 0.5));
}

// 1 along a vein at `p`, 0 off it: the creases of two scales of noise, the
// fine ones half as thick.
fn vein_mask(p: vec2<f32>, width: f32) -> f32 {
    let major = crease(p, width);
    let fine = crease(p * 2.7 + vec2<f32>(3.1, 8.7), width * 0.5);
    return max(major, fine * 0.7);
}

// 1 in a dot at the middle of the cell that `grain` falls in, for the one
// cell in `1 / chance` that hash2 picks, else 0.
fn speck(grain: vec2<f32>, chance: f32, radius: f32) -> f32 {
    return step(1.0 - chance, hash2(floor(grain))) * (1.0 - smoothstep(radius * 0.4, radius, length(fract(grain) - 0.5)));
}

fn brood_land(xz: vec2<f32>, dist: f32) -> Land {
    var l = NO_LAND;
    // Veins, their lines wandering with a warp; thick near the source.
    let warp = vec2<f32>(vnoise(xz * 0.45), vnoise(xz * 0.45 + vec2<f32>(5.2, 1.3))) - 0.5;
    let p = xz * 0.2 + warp * 0.9;
    let width = mix(0.07, 0.035, smoothstep(0.0, 20.0, dist));
    let e = 0.12;
    let vein = vein_mask(p, width);
    let vx = vein_mask(p + vec2<f32>(e * 0.2, 0.0), width);
    let vz = vein_mask(p + vec2<f32>(0.0, e * 0.2), width);
    // Skin: mottled, with soft swellings between the veins.
    let blotch = nz(xz * 0.23);
    var skin = mix(
        srgb_to_linear(vec3<f32>(0.27, 0.12, 0.12)),
        srgb_to_linear(vec3<f32>(0.4, 0.22, 0.19)),
        blotch,
    );
    skin = skin * (0.55 + 0.9 * nz(xz * 0.9));
    l.albedo = mix(skin, srgb_to_linear(vec3<f32>(0.16, 0.05, 0.09)), vein * 0.85);
    let swell = vnoise(xz * 2.2);
    let swell_x = vnoise((xz + vec2<f32>(e, 0.0)) * 2.2);
    let swell_z = vnoise((xz + vec2<f32>(0.0, e)) * 2.2);
    l.tilt = vec2<f32>(vein - vx, vein - vz) * 1.6 + vec2<f32>(swell - swell_x, swell - swell_z) * 0.5;
    // The pulses run inwards (towards smaller distances), each a little off
    // the others' time by place.
    let pulse = 0.5 + 0.5 * sin(dist * 0.7 + g.misc.y * 2.2 + vnoise(xz * 0.35) * 5.0);
    l.glow = vec3<f32>(1.0, 0.16, 0.26) * vein * (0.2 + 0.8 * pulse * pulse) * 1.1;
    // Wet.
    l.rough = 0.45;
    return l;
}

fn clans_land(xz: vec2<f32>, px: f32) -> Land {
    var l = NO_LAND;
    l.rough = 0.95;
    // Fields of about six tiles, each with rows turned its own way; some
    // lie fallow as trodden yards.
    let cell = floor(xz / 6.0);
    let turn = hash2(cell) * 3.14159;
    let along = vec2<f32>(cos(turn), sin(turn));
    let rows = dot(xz, along) * 2.4 * 6.28318;
    let tilled = step(0.4, hash2(cell + vec2<f32>(7.0, 3.0)));
    // Rows finer than a couple of pixels fade out rather than shimmer.
    let fine = 1.0 - smoothstep(0.08, 0.18, px);
    let furrow = 0.5 + 0.5 * sin(rows) * fine;
    let loam = mix(
        srgb_to_linear(vec3<f32>(0.2, 0.12, 0.07)),
        srgb_to_linear(vec3<f32>(0.36, 0.24, 0.13)),
        furrow,
    );
    let yard = mix(
        srgb_to_linear(vec3<f32>(0.34, 0.26, 0.16)),
        srgb_to_linear(vec3<f32>(0.46, 0.38, 0.22)),
        nz(xz * 0.5),
    );
    l.albedo = mix(yard, loam, tilled) * (0.8 + 0.4 * nz(xz * 1.7));
    // Rows rise and fall across the field.
    l.tilt = along * cos(rows) * 0.35 * tilled * fine;
    return l;
}

fn choir_land(xz: vec2<f32>, dist: f32, px: f32) -> Land {
    var l = NO_LAND;
    l.rough = 0.35;
    let frost = mix(
        srgb_to_linear(vec3<f32>(0.4, 0.38, 0.46)),
        srgb_to_linear(vec3<f32>(0.64, 0.61, 0.7)),
        nz(xz * 0.35),
    );
    // Pale cracks between the plates of frost.
    let crack = crease(xz * 0.5 + vec2<f32>(9.1, 2.7), 0.02);
    l.albedo = mix(frost, srgb_to_linear(vec3<f32>(0.75, 0.8, 0.95)), crack * 0.7);
    l.tilt = vec2<f32>(vnoise(xz * 3.1) - 0.5, vnoise(xz * 3.1 + vec2<f32>(4.0, 9.0)) - 0.5) * 0.3;
    // Crystal dust that glints, a few motes a tile, each on its own time.
    let grain = xz * 4.0;
    let glint = speck(grain, 0.1, 0.3) * (1.0 - smoothstep(0.1, 0.25, px)) * (0.5 + 0.5 * sin(g.misc.y * 3.0 + hash2(floor(grain)) * 60.0));
    // Rings spread outwards from the source and die away.
    let ring = smoothstep(0.88, 1.0, sin(dist * 1.1 - g.misc.y * 1.6)) * (1.0 - smoothstep(0.0, 10.0, dist));
    l.glow = vec3<f32>(0.55, 0.75, 1.0) * glint * 1.6 + vec3<f32>(0.3, 0.5, 1.0) * crack * 0.15 + vec3<f32>(0.3, 0.5, 1.0) * ring * 0.2;
    return l;
}

fn concord_land(xz: vec2<f32>, px: f32) -> Land {
    var l = NO_LAND;
    l.rough = 0.92;
    let ash = mix(
        srgb_to_linear(vec3<f32>(0.2, 0.19, 0.19)),
        srgb_to_linear(vec3<f32>(0.4, 0.37, 0.34)),
        nz(xz * 0.4),
    ) * (0.8 + 0.4 * nz(xz * 3.0));
    // The ash has dried and split, with flecks of bone in it.
    let crack = crease(xz * 0.25 + vec2<f32>(1.7, 6.3), 0.03);
    let fleck = speck(xz * 7.0, 0.05, 0.3) * (1.0 - smoothstep(0.06, 0.14, px));
    var albedo = mix(ash, srgb_to_linear(vec3<f32>(0.07, 0.065, 0.065)), crack * 0.7);
    albedo = mix(albedo, srgb_to_linear(vec3<f32>(0.75, 0.72, 0.62)), fleck * 0.8);
    l.albedo = albedo;
    l.tilt = vec2<f32>(0.0, -0.3 * crack);
    return l;
}

// The land at `xz`, if any: `spread` is how much of the tile the land grid
// covers there (bilinear) and `dist` how far in tiles from its nearest
// source, `px` the tiles a pixel spans. A tile's kind is not blended: the
// kind of the most covered of the four nearest tiles is used. `cover` is 0
// where there is none.
fn faction_land(xz: vec2<f32>, spread: f32, dist: f32, px: f32) -> Land {
    let cover = smoothstep(0.2, 0.45, spread + (fbm(xz * 0.3) - 0.5) * 0.5);
    if cover <= 0.0 {
        return NO_LAND;
    }
    let uv = xz / f32(textureDimensions(land_map).x);
    let covers = textureGather(0, land_map, soft_sampler, uv);
    let ids = textureGather(1, land_map, soft_sampler, uv);
    var id = ids.x;
    var most = covers.x;
    if covers.y > most { most = covers.y; id = ids.y; }
    if covers.z > most { most = covers.z; id = ids.z; }
    if covers.w > most { id = ids.w; }
    let kind = u32(round(id * 255.0));
    var l: Land;
    switch kind {
        case LAND_CLANS: { l = clans_land(xz, px); }
        case LAND_CHOIR: { l = choir_land(xz, dist, px); }
        case LAND_CONCORD: { l = concord_land(xz, px); }
        default: { l = brood_land(xz, dist); }
    }
    l.cover = cover;
    l.tilt = l.tilt * cover;
    l.glow = l.glow * cover;
    return l;
}

@fragment
fn fs_terrain(in: TerrainOut) -> @location(0) vec4<f32> {
    // Derivatives first, while control flow is uniform.
    let dpx = dpdx(in.world);
    let dpy = dpdy(in.world);
    let xz = in.world.xz;
    // The ground's true height, not the mesh's: a coarse chunk's triangles sit
    // a little off it, and colour, lighting and shoreline must not change when
    // a chunk changes level, so all of it is worked out here, per pixel.
    // `ground` has the relief on it; `h0` is the simulation's, which colour
    // follows.
    let h0 = height_at(xz);
    let w = cliff_weight(xz);
    var ground = h0;
    var n: vec3<f32>;
    var parts = vec2<f32>(0.0);
    if w > 0.0 {
        // Cliff: forward differences every 0.12 tile, so the crags show; the
        // warp and the mass are the middle's for all three.
        let e = 0.12;
        let mass = mountain_mass(xz, h0);
        let warp = relief_warp(xz);
        parts = relief_parts(xz);
        ground = relief_at(xz, h0, w, mass, warp, parts);
        let px = xz + vec2<f32>(e, 0.0);
        let pz = xz + vec2<f32>(0.0, e);
        let hx = relief_at(px, height_at(px), cliff_weight(px), mass, warp, relief_parts(px));
        let hz = relief_at(pz, height_at(pz), cliff_weight(pz), mass, warp, relief_parts(pz));
        n = normalize(vec3<f32>(ground - hx, e, ground - hz));
    } else {
        // Smooth ground keeps its old normal.
        let e = 1.0 / g.map.x;
        let hl = height_at(xz - vec2<f32>(e, 0.0));
        let hr = height_at(xz + vec2<f32>(e, 0.0));
        let hu = height_at(xz - vec2<f32>(0.0, e));
        let hd = height_at(xz + vec2<f32>(0.0, e));
        n = normalize(vec3<f32>(hl - hr, 2.0 * e, hu - hd));
    }
    let h = h0 / g.map.y;
    let slope = 1.0 - n.y;

    // Painted palette: Grub-soaked soil low, grass mid, bone-white ridges high.
    // Albedos are authored in sRGB and converted to linear for lighting.
    let soil = srgb_to_linear(vec3<f32>(0.42, 0.31, 0.2));
    var grass = srgb_to_linear(vec3<f32>(0.4, 0.5, 0.22));
    // The land's own grass, its borders wobbled so they show neither the
    // tint's texels nor the discs the map was painted with. A map without
    // lands has a single clear texel, and nothing to look up.
    if textureDimensions(ground_tint).x > 1u {
        let tiles = f32(textureDimensions(heightmap).x - 1u) / g.map.x;
        let wobble = vec2<f32>(vnoise(xz * 0.07), vnoise(xz * 0.07 + vec2<f32>(37.0, 11.0))) - 0.5;
        let tint = textureSampleLevel(ground_tint, soft_sampler, (xz + wobble * 10.0) / tiles, 0.0);
        grass = mix(grass, srgb_to_linear(tint.rgb), tint.a);
    }
    // The habitat's own grass colour, its borders wobbled the same way. A map
    // without habitats has a single neutral texel, and nothing to look up.
    let look_tiles = f32(textureDimensions(ground_look).x);
    let has_look = textureDimensions(ground_look).x > 1u;
    var look_at = xz;
    if has_look {
        let look_wobble = (vec2<f32>(vnoise(xz * 0.3), vnoise(xz * 0.3 + vec2<f32>(23.0, 5.0))) - 0.5) * 2.6
            + (vec2<f32>(vnoise(xz * 1.1), vnoise(xz * 1.1 + vec2<f32>(7.0, 31.0))) - 0.5) * 0.8;
        // Kept to the map: the sampler repeats.
        look_at = clamp(xz + look_wobble, vec2<f32>(0.5), vec2<f32>(look_tiles - 0.5));
        let look_mult = textureSampleLevel(ground_look, soft_sampler, look_at / look_tiles, 0.0).rgb;
        grass = grass * look_mult * (255.0 / 128.0);
    }
    let dry = srgb_to_linear(vec3<f32>(0.62, 0.55, 0.33));
    let bone = srgb_to_linear(vec3<f32>(0.85, 0.8, 0.68));
    let grass_in = smoothstep(0.16, 0.26, h);
    let dry_in = smoothstep(0.45, 0.6, h);
    var albedo = mix(soil, grass, grass_in);
    albedo = mix(albedo, dry, dry_in);
    albedo = mix(albedo, bone, smoothstep(0.62, 0.78, h));
    // Large-scale painterly variation so flat areas aren't flat colour.
    let v = vnoise(xz * 0.08) * 0.16 + vnoise(xz * 0.5) * 0.08;
    albedo = albedo * (0.88 + v);

    // Rock: where the surface is steeper than the walkable 0.6 a tile
    // (slope 0.14), its edge broken up by noise, with a dirt fringe around it
    // where the grass gives out. (Gentler ground has no rock, and skips the
    // noise.)
    var jitter = 0.0;
    var rock = 0.0;
    if w > 0.0 || slope > 0.14 {
        jitter = (fbm(xz * 0.9) - 0.5) * 0.06;
        // Ledges and crests inside the cliff zone are rock too.
        rock = max(smoothstep(0.19, 0.32, slope + jitter), smoothstep(0.02, 0.25, w));
    }
    let fringe = smoothstep(0.1, 0.18, slope + jitter) * (1.0 - rock);
    albedo = mix(albedo, soil * (0.75 + 0.5 * vnoise(xz * 2.3)), fringe * 0.7);

    // Grass where the palette is grass, soil elsewhere (dry, bone and the
    // dirt fringe are soil's), the small scale put on the palette. Fully
    // rock ground has none of it.
    var nd = n;
    var wet = 0.0;
    if rock < 1.0 {
        let grassy = grass_in * (1.0 - dry_in) * (1.0 - fringe * 0.7);
        let m = ground_blend(xz);
        var detail = GroundLayer(vec3<f32>(1.0), vec2<f32>(0.0));
        if grassy > 0.02 {
            detail = ground_layer(0, GRASS_MEAN, xz, dpx.xz, dpy.xz, m);
        }
        if grassy < 0.98 {
            let soil_detail = ground_layer(1, SOIL_MEAN, xz, dpx.xz, dpy.xz, m);
            detail = GroundLayer(
                mix(soil_detail.ratio, detail.ratio, grassy),
                mix(soil_detail.tilt, detail.tilt, grassy),
            );
        }
        // Mostly its light and dark: the texture's own hues would fight the land's tint.
        albedo = albedo * muted(detail.ratio, 0.35);
        // The surface, tilted by the ground's normal map (whiteout blend, the
        // map's plane being the ground's).
        // Steeper ground takes less of it, so it cannot turn the normal over.
        nd = normalize(vec3<f32>(n.x + detail.tilt.x * n.y, n.y, n.z + detail.tilt.y * n.y));

        // The habitat's ground over it: bog pools, flowers, gravel and the
        // like, the pattern the tile's own (a nearest lookup), its edge ragged
        // by the wobble.
        if has_look {
            let tile = vec2<i32>(look_at);
            let pattern = u32(textureLoad(ground_look, tile, 0).a * 255.0 + 0.5);
            if pattern > 0u {
                let hg = habitat_ground(pattern, xz, max(length(dpx.xz), length(dpy.xz)));
                albedo = mix(albedo, hg.albedo, hg.k);
                wet = hg.wet;
            }
        }
    }

    // Marks on the ground (and none on rock): worn tracks of bare dirt where
    // armies have marched, turned earth where trees were felled, scorch.
    let scar_uv = xz / f32(textureDimensions(scar_map).x);
    let scar = textureSampleLevel(scar_map, soft_sampler, scar_uv, 0.0);
    if scar.r + scar.g + scar.b > 0.03 && rock < 1.0 {
        let keep = 1.0 - rock;
        // Ragged edges: the noise moves where a mark begins, but not into
        // ground that has none of it (r or g of 0).
        let ragged = (nz(xz * 1.3) - 0.5) * 0.5;
        let worn = smoothstep(0.25, 0.75, scar.b + ragged) * keep;
        let felled = smoothstep(0.2, 0.55, scar.r + ragged) * smoothstep(0.0, 0.1, scar.r) * keep;
        let burnt = smoothstep(0.2, 0.55, scar.g + ragged) * smoothstep(0.0, 0.1, scar.g) * keep;
        let grit = 0.7 + 0.6 * nz(xz * 2.1);
        albedo = mix(albedo, srgb_to_linear(vec3<f32>(0.37, 0.29, 0.18)) * grit, worn * 0.85);
        albedo = mix(albedo, srgb_to_linear(vec3<f32>(0.27, 0.17, 0.1)) * grit, felled);
        albedo = mix(albedo, srgb_to_linear(vec3<f32>(0.05, 0.045, 0.04)) * grit, burnt);
        wet = wet * (1.0 - max(worn, max(felled, burnt)));
        // Turned earth lies in clods.
        let clod = vec2<f32>(vnoise(xz * 3.3) - 0.5, vnoise(xz * 3.3 + vec2<f32>(6.0, 2.0)) - 0.5);
        nd = normalize(nd + vec3<f32>(clod.x, 0.0, clod.y) * 0.45 * max(felled, worn * 0.5));
    }

    // A faction's land over the ground, where it has spread (and the ground
    // is not rock).
    var land_glow = vec3<f32>(0.0);
    var land_rough = 1.0;
    var landed = false;
    let land = textureSampleLevel(land_map, soft_sampler, xz / f32(textureDimensions(land_map).x), 0.0);
    // The noise moves the edge by at most a quarter, so under 0.05 of cover
    // is bare for certain.
    if land.r > 0.05 && rock < 1.0 {
        let keep = 1.0 - rock;
        let l = faction_land(xz, land.r, land.b * 255.0 / 4.0, max(length(dpx.xz), length(dpy.xz)));
        let k = l.cover * keep;
        albedo = mix(albedo, l.albedo, k);
        wet = wet * (1.0 - k);
        nd = normalize(nd + vec3<f32>(l.tilt.x, 0.0, l.tilt.y) * keep);
        land_glow = l.glow * keep;
        land_rough = mix(1.0, l.rough, k);
        landed = k > 0.0;
    }
    // Standing water shines.
    if wet > 0.0 {
        land_rough = min(land_rough, mix(1.0, 0.25, wet));
        landed = true;
    }

    var c: vec3<f32>;
    if rock > 0.0 {
        let r = rock_surface(in.world, n, dpx, dpy);
        // Strata: bands of colour by height, wobbled sideways.
        let band = vnoise(vec2<f32>(in.world.y * 1.5 + vnoise(xz * 0.25) * 3.0, 3.0));
        let warm = vnoise(vec2<f32>(in.world.y * 0.4, 9.0));
        // Rock035's own colour is near black (mean 0.01 linear), so it gives the
        // pattern only: its brightness, relative to its mean, scales a stone
        // colour from the game's palette.
        let pattern = rock_pattern(r.albedo);
        let stone = srgb_to_linear(vec3<f32>(0.5, 0.45, 0.38)) * 0.8;
        var rc = stone * pattern * (0.78 + 0.44 * band) * mix(vec3<f32>(1.0), vec3<f32>(1.14, 0.98, 0.8), warm);
        // Higher stone is paler and cooler.
        rc = rc * mix(vec3<f32>(0.95), vec3<f32>(1.12, 1.14, 1.2), smoothstep(0.4, 0.9, h));
        // Crevices: the low parts of the relief are dark and get less sky.
        // (Steep ground no relief stands on has none of either.)
        let relieved = smoothstep(0.0, 0.2, w);
        let cavity = mix(1.0, mix(0.4, 1.0, smoothstep(0.05, 0.55, parts.x)), relieved);
        let tall = mix(1.0, mix(0.7, 1.0, smoothstep(0.0, 0.6, parts.y)), relieved);
        rc = rc * mix(1.0, cavity, 0.6);
        // Dust and soil settle on the ledges.
        rc = mix(rc, soil * 0.9, smoothstep(0.82, 0.95, n.y) * 0.3);
        let ns = normalize(mix(nd, r.n, rock));
        let rough = mix(1.0, max(r.rough, 0.75), rock);
        let ao = mix(1.0, cavity * tall, rock);
        c = shade_material(mix(albedo, rc, rock), ns, in.world, rough, 0.0, ao, land_glow);
    } else if landed {
        c = shade_material(albedo, nd, in.world, land_rough, 0.0, 1.0, land_glow);
    } else {
        c = shade(albedo, nd, in.world);
    }

    // Valleys and the feet of slopes get less light.
    let skyward = textureSampleLevel(ground_occlusion, soft_sampler, xz / f32(textureDimensions(ground_occlusion).x), 0.0).r;
    // (Fogged ground is the air's colour, not lit ground: it keeps less of this.)
    let unfogged = 1.0 - air_amount(in.world, distance(in.world, g.camera_pos.xyz));
    c = c * mix(1.0, skyward, g.air.y * unfogged);

    // Water below the water level: deep teal with a soft shoreline, a
    // rippled surface that mirrors the sky (more the lower you look) and
    // glints where it catches the sun.
    let depth = g.misc.x - ground;
    if depth > 0.0 {
        let water = srgb_to_linear(vec3<f32>(0.16, 0.36, 0.4));
        let wp = vec3<f32>(in.world.x, g.misc.x, in.world.z);
        let dist = distance(wp, g.camera_pos.xyz);
        let wn = water_normal(wp.xz, g.misc.y, 1.0 / (1.0 + dist * 0.04));
        let v = (g.camera_pos.xyz - wp) / max(dist, 1e-4);
        let sun = sunlight(wp, wn);
        let lit = water * (g.sun_color.rgb * max(dot(wn, g.sun_dir.xyz), 0.0) * sun + ambient_light(wn));
        let r = reflect(-v, wn);
        let mirror = 0.1 + 0.9 * pow(1.0 - max(dot(v, wn), 0.0), 4.0);
        let sky = sky_color(vec3<f32>(r.x, abs(r.y), r.z), false);
        let glint = g.sun_color.rgb * pow(max(dot(r, g.sun_dir.xyz), 0.0), 500.0) * 10.0 * sun;
        let wc = finish(mix(lit, sky * 0.8, mirror) + glint, wp);
        c = mix(c, wc, clamp(0.5 + depth * 1.5, 0.0, 0.92));
    }
    return vec4<f32>(veil(c, xz), 1.0);
}

// ---------------------------------------------------------------- regiments

struct BlockOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(perspective, centroid) world: vec3<f32>,
    @location(1) @interpolate(perspective, centroid) normal: vec3<f32>,
    @location(2) @interpolate(perspective, centroid) color: vec4<f32>,
};

// Unit box corners: x across the frontage, y up, z along the facing.
const BOX_POS = array<vec3<f32>, 8>(
    vec3<f32>(-0.5, 0.0, -0.5), vec3<f32>(0.5, 0.0, -0.5),
    vec3<f32>(0.5, 1.0, -0.5), vec3<f32>(-0.5, 1.0, -0.5),
    vec3<f32>(-0.5, 0.0, 0.5), vec3<f32>(0.5, 0.0, 0.5),
    vec3<f32>(0.5, 1.0, 0.5), vec3<f32>(-0.5, 1.0, 0.5),
);
// Five faces (no bottom), two triangles each.
const BOX_IDX = array<u32, 30>(
    0u, 2u, 1u, 0u, 3u, 2u, // back
    4u, 5u, 6u, 4u, 6u, 7u, // front
    0u, 4u, 7u, 0u, 7u, 3u, // left
    1u, 2u, 6u, 1u, 6u, 5u, // right
    3u, 7u, 6u, 3u, 6u, 2u, // top
);
const BOX_NORMAL = array<vec3<f32>, 5>(
    vec3<f32>(0.0, 0.0, -1.0), vec3<f32>(0.0, 0.0, 1.0),
    vec3<f32>(-1.0, 0.0, 0.0), vec3<f32>(1.0, 0.0, 0.0),
    vec3<f32>(0.0, 1.0, 0.0),
);

@vertex
fn vs_block(
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec2<f32>,
    @location(1) facing: vec2<f32>,
    @location(2) size: vec3<f32>,
    @location(3) color: vec4<f32>,
    @location(4) lift: f32,
) -> BlockOut {
    return block_vertex(vi, pos, facing, size, color, lift);
}

fn block_vertex(
    vi: u32,
    pos: vec2<f32>,
    facing: vec2<f32>,
    size: vec3<f32>,
    color: vec4<f32>,
    lift: f32,
) -> BlockOut {
    let corner = BOX_POS[BOX_IDX[vi]];
    let n_local = BOX_NORMAL[vi / 6u];
    let fwd = vec2<f32>(facing.x, facing.y);
    let right = vec2<f32>(-facing.y, facing.x);
    let xz = pos + right * (corner.x * size.x) + fwd * (corner.z * size.z);
    // Drape over the terrain: each column sits on the ground beneath it.
    // A negative height is drawn as positive (see `vs_block_shadow`).
    let ground = ground_at(xz);
    let world = vec3<f32>(xz.x, ground + lift + corner.y * abs(size.y), xz.y);
    var out: BlockOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    let nxz = right * n_local.x + fwd * n_local.z;
    out.normal = normalize(vec3<f32>(nxz.x, n_local.y, nxz.y));
    out.color = color;
    return out;
}

// The boxes' shadows. A box given a negative height (smoke, dust and
// flames) casts none: it is folded away here.
@vertex
fn vs_block_shadow(
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec2<f32>,
    @location(1) facing: vec2<f32>,
    @location(2) size: vec3<f32>,
    @location(3) color: vec4<f32>,
    @location(4) lift: f32,
) -> BlockOut {
    var out = block_vertex(vi, pos, facing, size, color, lift);
    if size.y < 0.0 {
        // Outside the clip volume (a w of 0 would divide by zero).
        out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    return out;
}

@fragment
fn fs_block(in: BlockOut) -> @location(0) vec4<f32> {
    // color.a is the selection highlight strength.
    let albedo = srgb_to_linear(in.color.rgb);
    let c = shade(albedo, in.normal, in.world);
    // Not veiled: boxes are what the side is shown (remembered buildings,
    // the building ghost, bars and marks), so they stay readable in fog.
    return vec4<f32>(c + albedo * in.color.a * 0.8, 1.0);
}

// ---------------------------------------------------------------- soldiers
//
// Real soldier models: each vertex follows up to four bones, and the bone
// texture holds, per animation frame, a 3 × 4 matrix per bone that moves a
// vertex from where it is stored into that frame's pose. Each instance names
// two frame rows and how far to blend between them; the vertex blends its
// bones' matrices by their weights (linear blend skinning).
//
// Skinning happens once a frame, in `cs_skin`, before any pass draws: it
// poses every vertex of every soldier the frame draws and keeps its world
// position, normal and tangent. The shadow pass and the world pass then read
// those (`skin_pos`, `skin_shade`) and only transform them. A soldier's
// vertices lie together, `skin_origin[instance] + vertex_index` being the
// first (see `soldiers.rs`). Models painted in vertex colours draw with
// `vs_soldier` and `fs_soldier`; textured ones with `vs_soldier_textured`
// and `fs_soldier_textured`, which read three layers of the material array
// (albedo and team mask, normal with occlusion and roughness, emitted light
// with metalness).

@group(1) @binding(0) var<storage, read> skin_pos: array<f32>;
@group(1) @binding(1) var materials_srgb: texture_2d_array<f32>;
@group(1) @binding(2) var materials_linear: texture_2d_array<f32>;
@group(1) @binding(3) var material_sampler: sampler;
@group(1) @binding(4) var<storage, read> skin_shade: array<u32>;
@group(1) @binding(5) var<storage, read> skin_origin: array<u32>;
// The shared surface detail: two layers (albedo and cavity, relief) per
// class, repeating (`DetailClass` in gc_assets/src/detail.rs).
@group(1) @binding(6) var detail_array: texture_2d_array<f32>;
@group(1) @binding(7) var detail_sampler: sampler;

// The skinning pass's own bindings, in the same group of its own pipeline.
@group(1) @binding(8) var bones: texture_2d<f32>;
@group(1) @binding(9) var<storage, read> set_vertices: array<SetVertex>;
@group(1) @binding(10) var<storage, read> instances: array<u32>;
@group(1) @binding(11) var<storage, read> jobs: Jobs;
@group(1) @binding(12) var<storage, read_write> out_pos: array<f32>;
@group(1) @binding(13) var<storage, read_write> out_shade: array<u32>;
// The model vertices past `jobs.count.z`, when a GPU binds too few at once.
@group(1) @binding(14) var<storage, read> set_vertices_hi: array<SetVertex>;
// Every drawn soldier's final bone matrices (three rows of four floats a
// bone), written by `cs_pose` and read by `cs_skin`.
@group(1) @binding(15) var<storage, read_write> palette: array<vec4<f32>>;
// Shared skeletons, their joint poses and each fitted model's fixes (see
// `AnimGpu` in soldiers.rs for the layout), as vectors of four floats.
@group(1) @binding(16) var<storage, read> anim: array<vec4<u32>>;

fn set_vertex(i: u32) -> SetVertex {
    if i < jobs.count.z {
        return set_vertices[i];
    }
    return set_vertices_hi[i - jobs.count.z];
}

// A model vertex as stored (`ModelVertex`), in words.
struct SetVertex {
    // x and y, 16-bit signed, in 1/4096 tiles
    xy: u32,
    // z likewise; the high half holds the team weight and material plus one
    zw: u32,
    bones: u32,
    weights: u32,
    normal: u32,
    tangent: u32,
    color: u32,
    uv: u32,
};

// A run of soldiers of one model at one level of detail. `run`: their first
// instance, the first vertex of the level's vertices and how many there are,
// and the first output slot. `pose`: the first palette entry of the run's
// first soldier, the model's bones (each soldier has that many entries),
// where the model's fit starts in `anim` (0 for a model posed from bone
// rows), and the run's first soldier counted over the frame. A skinning
// thread belongs to the last job whose slot is at or before it, a posing
// workgroup to the last whose first soldier is.
struct Job {
    run: vec4<u32>,
    pose: vec4<u32>,
};

struct Jobs {
    // x: jobs, y: skinning threads, z: model vertices in `set_vertices`
    // (the rest are in `set_vertices_hi`), w: soldiers posed
    count: vec4<u32>,
    job: array<Job>,
};

struct SoldierIn {
    @builtin(vertex_index) index: u32,
    @builtin(instance_index) instance: u32,
    // The team weight (0 to 255) and the material plus one.
    @location(0) team_material: vec2<u32>,
    // The tangent's last two bytes; the second is the bitangent's handedness.
    @location(1) tangent_w: vec2<f32>,
    @location(2) vcolor: vec4<f32>,
    @location(3) vuv: vec2<f32>,
    // y: selection highlight
    @location(4) flags: vec4<f32>,
    @location(5) team_color: vec4<f32>,
    // The normal's z byte, then the detail classes (four bits each).
    @location(6) detail_classes: vec2<u32>,
};

struct SoldierOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(perspective, centroid) world: vec3<f32>,
    @location(1) @interpolate(perspective, centroid) normal: vec3<f32>,
    @location(2) @interpolate(perspective, centroid) color: vec3<f32>,
    @location(3) @interpolate(perspective, centroid) highlight: f32,
};

struct TexturedOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(perspective, centroid) world: vec3<f32>,
    @location(1) @interpolate(perspective, centroid) normal: vec3<f32>,
    @location(2) @interpolate(perspective, centroid) highlight: f32,
    // World-space tangent; w is the bitangent's handedness.
    @location(3) @interpolate(perspective, centroid) tangent: vec4<f32>,
    @location(4) @interpolate(perspective, centroid) uv: vec2<f32>,
    // The material's first layer in the array.
    @location(5) @interpolate(flat) layer: i32,
    @location(6) @interpolate(flat) team_color: vec3<f32>,
    // The soldier's tone, from -1 to 1 (0 leaves the model as painted).
    @location(7) @interpolate(flat) tone: f32,
    // The surface detail: two classes (bits 0 to 3 and 4 to 7), the share of
    // the second (8 to 11) and how finely it repeats (12 to 15).
    @location(8) @interpolate(flat) detail: u32,
};

// The soldier's tone byte (128 neutral) as -1 to 1.
fn tone_of(byte: f32) -> f32 {
    return clamp((byte * 255.0 - 128.0) / 128.0, -1.0, 1.0);
}

// What a tone does to colour: up to 8% lighter or darker with a faint
// warm or cool shift. Team parts take a third of it.
fn tone_mul(tone: f32) -> vec3<f32> {
    return (1.0 + 0.08 * tone) * (1.0 + tone * vec3<f32>(0.04, 0.015, -0.03));
}

// One bone's final matrix rows from the palette.
fn palette_bone(slot: u32) -> mat3x4<f32> {
    return mat3x4<f32>(palette[slot * 3u], palette[slot * 3u + 1u], palette[slot * 3u + 2u]);
}

fn bone_texel(row: u32, bone: u32, r: u32) -> vec4<f32> {
    return textureLoad(bones, vec2<i32>(i32(bone * 3u + r), i32(row)), 0);
}

// One bone's matrix rows, blended between the instance's two frames.
fn bone_rows(frames: vec2<u32>, bone: u32, t: f32) -> mat3x4<f32> {
    return mat3x4<f32>(
        mix(bone_texel(frames.x, bone, 0u), bone_texel(frames.y, bone, 0u), t),
        mix(bone_texel(frames.x, bone, 1u), bone_texel(frames.y, bone, 1u), t),
        mix(bone_texel(frames.x, bone, 2u), bone_texel(frames.y, bone, 2u), t),
    );
}

// Byte `i` of a word as a signed value, 127 being 1.
fn snorm8(w: u32, i: u32) -> f32 {
    return max(f32(bitcast<i32>(w << (24u - 8u * i)) >> 24u) / 127.0, -1.0);
}

// A model-space direction moved by the blended bones, laid down with the
// fallen, and turned to the soldier's facing. Model space is x to the
// soldier's left, y up, z forward (glTF's convention).
fn model_dir_to_world(m: mat3x4<f32>, d: vec3<f32>, facing: vec2<f32>, fallen: bool) -> vec3<f32> {
    var l = vec3<f32>(dot(m[0].xyz, d), dot(m[1].xyz, d), dot(m[2].xyz, d));
    if fallen {
        l = vec3<f32>(l.x, -l.z, l.y);
    }
    let left = vec2<f32>(facing.y, -facing.x);
    let xz = left * l.x + facing * l.z;
    return vec3<f32>(xz.x, l.y, xz.y);
}

// Words in a `SoldierInstance`.
const INSTANCE_WORDS: u32 = 16u;
// Layers a soldier may play on top of its base move (`PoseLayer`).
const POSE_LAYERS: u32 = 3u;
// Joints a skeleton may have, plus the still bone: one posing thread each.
const POSE_GROUP: u32 = 64u;
// Soldiers posed along the dispatch's first dimension.
const POSE_ROW: u32 = 256u;

// One layer of a soldier's pose: two frame rows, the blend between them and
// how much of the layer shows, on which joints and how.
struct Layer {
    frames: vec2<u32>,
    blend: f32,
    weight: f32,
    mask: u32,
    additive: bool,
};

// Layer `k` of the instance at word `i`: 0 is the base move (its weight is
// one), then `POSE_LAYERS` more.
fn instance_layer(i: u32, k: u32) -> Layer {
    var l: Layer;
    if k == 0u {
        l.frames = vec2<u32>(instances[i + 4u] & 0xffffu, instances[i + 4u] >> 16u);
        l.blend = f32(instances[i + 5u] & 0xffu) / 255.0;
        l.weight = 1.0;
        l.mask = 0u;
        l.additive = false;
        return l;
    }
    let at = i + 9u + (k - 1u) * 2u;
    let info = instances[at + 1u];
    l.frames = vec2<u32>(instances[at] & 0xffffu, instances[at] >> 16u);
    l.blend = f32(info & 0xffu) / 255.0;
    l.weight = f32((info >> 8u) & 0xffu) / 255.0;
    l.mask = (info >> 16u) & 0xffu;
    l.additive = ((info >> 24u) & 1u) != 0u;
    return l;
}

// A joint's pose: rotation, then translation and scale.
struct JointPose {
    r: vec4<f32>,
    ts: vec4<f32>,
};

// Where a skeleton's poses start in `anim`, and its joints (a row's length).
struct Poses {
    at: u32,
    joints: u32,
};

// The two half floats in `w` (low first), decoded by hand: `unpack2x16float`
// needs a capability not every GPU has. No infinities or NaNs are stored.
fn halves(w: vec2<u32>) -> vec4<f32> {
    let h = vec4<u32>(w.x & 0xffffu, w.x >> 16u, w.y & 0xffffu, w.y >> 16u);
    let sign = (h & vec4<u32>(0x8000u)) << vec4<u32>(16u);
    let e = (h >> vec4<u32>(10u)) & vec4<u32>(0x1fu);
    let m = h & vec4<u32>(0x3ffu);
    let normal = bitcast<vec4<f32>>(sign | ((e + vec4<u32>(112u)) << vec4<u32>(23u)) | (m << vec4<u32>(13u)));
    let tiny = vec4<f32>(m) * 5.9604645e-8 * select(vec4<f32>(1.0), vec4<f32>(-1.0), sign != vec4<u32>(0u));
    return select(normal, tiny, e == vec4<u32>(0u));
}

// A joint's pose in frame `row`, from eight half floats.
fn joint_pose(poses: Poses, row: u32, joint: u32) -> JointPose {
    let v = anim[poses.at + row * poses.joints + joint];
    return JointPose(halves(v.xy), halves(v.zw));
}

// Normalised blend of two rotations, the short way round.
fn quat_nlerp(a: vec4<f32>, b: vec4<f32>, t: f32) -> vec4<f32> {
    let b2 = select(b, -b, dot(a, b) < 0.0);
    return normalize(mix(a, b2, t));
}

fn quat_mul(a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        a.w * b.xyz + b.w * a.xyz + cross(a.xyz, b.xyz),
        a.w * b.w - dot(a.xyz, b.xyz),
    );
}

// A layer's pose of a joint, between its two frames.
fn layer_pose(poses: Poses, l: Layer, joint: u32) -> JointPose {
    let a = joint_pose(poses, l.frames.x, joint);
    let b = joint_pose(poses, l.frames.y, joint);
    return JointPose(quat_nlerp(a.r, b.r, l.blend), mix(a.ts, b.ts, l.blend));
}

// Rotation, translation and uniform scale as a matrix.
fn pose_matrix(p: JointPose) -> mat4x4<f32> {
    let q = p.r;
    let s = p.ts.w;
    let x2 = q.x + q.x;
    let y2 = q.y + q.y;
    let z2 = q.z + q.z;
    let xx = q.x * x2;
    let yy = q.y * y2;
    let zz = q.z * z2;
    let xy = q.x * y2;
    let xz = q.x * z2;
    let yz = q.y * z2;
    let wx = q.w * x2;
    let wy = q.w * y2;
    let wz = q.w * z2;
    return mat4x4<f32>(
        vec4<f32>(1.0 - (yy + zz), xy + wz, xz - wy, 0.0) * s,
        vec4<f32>(xy - wz, 1.0 - (xx + zz), yz + wx, 0.0) * s,
        vec4<f32>(xz + wy, yz - wx, 1.0 - (xx + yy), 0.0) * s,
        vec4<f32>(p.ts.xyz, 1.0),
    );
}

// Three rows of floats in `anim` from `at` as a matrix.
fn anim_matrix(at: u32) -> mat4x4<f32> {
    return transpose(mat4x4<f32>(
        bitcast<vec4<f32>>(anim[at]),
        bitcast<vec4<f32>>(anim[at + 1u]),
        bitcast<vec4<f32>>(anim[at + 2u]),
        vec4<f32>(0.0, 0.0, 0.0, 1.0),
    ));
}

// The job holding thread `n`, counting threads by `run.w` (skinning) or
// soldiers by `pose.w` (posing): the last job starting at or before it.
fn find_job(n: u32, posing: bool) -> u32 {
    var lo = 0u;
    var hi = jobs.count.x;
    while hi - lo > 1u {
        let mid = (lo + hi) / 2u;
        let start = select(jobs.job[mid].run.w, jobs.job[mid].pose.w, posing);
        if start <= n {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    return lo;
}

fn write_palette(slot: u32, m: mat3x4<f32>) {
    palette[slot * 3u] = m[0];
    palette[slot * 3u + 1u] = m[1];
    palette[slot * 3u + 2u] = m[2];
}

// The rows of a matrix (as `bone_rows` returns them).
fn matrix_rows(m: mat4x4<f32>) -> mat3x4<f32> {
    let t = transpose(m);
    return mat3x4<f32>(t[0], t[1], t[2]);
}

var<workgroup> pose_globals: array<mat4x4<f32>, POSE_GROUP>;

// A soldier's rig fixes (see `rig_of` in soldiers.rs): 0 drops the whole
// body, 1 to 3 bend the left leg (upper, lower, foot), 4 to 6 the right,
// 7 to 9 turn chest, neck and head. Each also drops.
const RIG_FIXES: u32 = 10u;
var<workgroup> pose_fix: array<mat4x4<f32>, RIG_FIXES>;
// A joint missing from the rig.
const NO_JOINT: u32 = 255u;

fn translation(t: vec3<f32>) -> mat4x4<f32> {
    return mat4x4<f32>(
        vec4<f32>(1.0, 0.0, 0.0, 0.0),
        vec4<f32>(0.0, 1.0, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 1.0, 0.0),
        vec4<f32>(t, 1.0),
    );
}

// A turn by `angle` about `axis` (of unit length), right-handed.
fn axis_angle(axis: vec3<f32>, angle: f32) -> mat3x3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    let a = axis;
    let k = 1.0 - c;
    return mat3x3<f32>(
        vec3<f32>(c + a.x * a.x * k, a.y * a.x * k + a.z * s, a.z * a.x * k - a.y * s),
        vec3<f32>(a.x * a.y * k - a.z * s, c + a.y * a.y * k, a.z * a.y * k + a.x * s),
        vec3<f32>(a.x * a.z * k + a.y * s, a.y * a.z * k - a.x * s, c + a.z * a.z * k),
    );
}

// The turn `r` about the point `p`.
fn turn_about(p: vec3<f32>, r: mat3x3<f32>) -> mat4x4<f32> {
    return mat4x4<f32>(vec4<f32>(r[0], 0.0), vec4<f32>(r[1], 0.0), vec4<f32>(r[2], 0.0), vec4<f32>(p - r * p, 1.0));
}

// The shortest turn taking direction `u` onto `w` (both of unit length).
fn turn_onto(u: vec3<f32>, w: vec3<f32>) -> mat3x3<f32> {
    let axis = cross(u, w);
    let sine = length(axis);
    return axis_angle(select(vec3<f32>(0.0, 1.0, 0.0), axis / sine, sine > 1e-7), atan2(sine, dot(u, w)));
}

// Where a soldier looks: yaw about the model's up, then pitch about its left.
fn look_turn(yaw: f32, pitch: f32) -> mat3x3<f32> {
    return axis_angle(vec3<f32>(0.0, 1.0, 0.0), yaw) * axis_angle(vec3<f32>(1.0, 0.0, 0.0), pitch);
}

fn joint_at(j: u32) -> vec3<f32> {
    return pose_globals[j][3].xyz;
}

// The ground where the skinning pass stands a soldier: the terrain, or just
// under the water line.
fn ground_at(xz: vec2<f32>) -> f32 {
    return max(height_at(xz), g.misc.x - 0.05);
}

// Byte `k` of `w`, a rig joint.
fn rig_joint(w: u32, k: u32) -> u32 {
    return (w >> (8u * k)) & 0xffu;
}

// How much a leg's foot is planted, from how high its ankle is over the
// soldier's ground, for a leg `reach` long: a rider's or a leaping foot
// is left alone.
fn planted(ankle: f32, reach: f32) -> f32 {
    return 1.0 - smoothstep(0.3 * reach, 0.6 * reach, ankle);
}

// How much of foot IK the soldier at instance word `i` takes (0 for none):
// none for a rig without legs, a model laid flat, lifted or sinking, or as
// much as `SoldierInstance::feet` keeps the feet as posed.
fn feet_weight(i: u32, rig: vec4<u32>) -> f32 {
    let fallen = ((instances[i + 5u] >> 16u) & 0xffu) > 127u;
    let lift = bitcast<f32>(instances[i + 7u]);
    let scale = bitcast<f32>(instances[i + 8u]);
    let legs = rig_joint(rig.x, 0u) != NO_JOINT;
    let on = legs && !fallen && lift == 0.0 && scale > 0.0;
    return select(0.0, 1.0 - f32(instances[i + 15u] >> 24u) / 255.0, on);
}

// How much the soldier at instance word `i` looks round.
fn look_weight(i: u32) -> f32 {
    return f32((instances[i + 15u] >> 16u) & 0xffu) / 255.0;
}

// Bend one leg (its rig word) so its foot rises by `rise` (model units, the
// body already dropped by `drop`), with the knee always forward: turn the
// knee until hip and foot are as far apart as hip and goal, then swing the
// leg at the hip onto the goal. The foot keeps its own turn.
fn bend_leg(leg: u32, first: u32, rise_by: f32, drop: mat4x4<f32>) {
    let hip = (drop * vec4<f32>(joint_at(rig_joint(leg, 0u)), 1.0)).xyz;
    let knee = (drop * vec4<f32>(joint_at(rig_joint(leg, 1u)), 1.0)).xyz;
    let ankle = (drop * vec4<f32>(joint_at(rig_joint(leg, 2u)), 1.0)).xyz;
    let thigh = knee - hip;
    let shin = ankle - knee;
    let lab = length(thigh);
    let lcb = length(shin);
    let reach = lab + lcb;
    let rise = min(rise_by, 0.4 * reach);
    if rise <= 1e-4 || lab < 1e-5 || lcb < 1e-5 || length(ankle - hip) < 1e-5 {
        return;
    }
    let goal = ankle + vec3<f32>(0.0, rise, 0.0);
    let lat = clamp(length(goal - hip), max(abs(lab - lcb) + 1e-4, 0.25 * reach), reach - 1e-4);
    // The knee's hinge: the leg's own bending plane while the knee is
    // forward of the hip-to-foot line, else across the leg, so a straight or
    // overstraightened knee bends forward.
    let ac = normalize(ankle - hip);
    let across = normalize(cross(ac, vec3<f32>(0.0, 0.0, 1.0)) + vec3<f32>(-1e-6, 0.0, 0.0));
    let plane = cross(ac, thigh / lab);
    let hinge = select(across, normalize(plane), dot(plane, across) > 0.05);
    // The knee's turn: |thigh + R(t) shin| = lat, solved as
    // p cos t + q sin t = c, taking the root nearest the plain bend.
    let along = dot(shin, hinge) * hinge;
    let flat = shin - along;
    let p = dot(thigh, flat);
    let q = dot(thigh, cross(hinge, flat));
    let c = 0.5 * (lat * lat - lab * lab - lcb * lcb) - dot(thigh, along);
    let m = max(length(vec2<f32>(p, q)), 1e-9);
    let phi = atan2(q, p);
    let spread = acos(clamp(c / m, -1.0, 1.0));
    let inner = acos(clamp((lab * lab + lcb * lcb - lat * lat) / (2.0 * lab * lcb), -1.0, 1.0));
    let now = atan2(dot(cross(thigh, shin), hinge), dot(thigh, shin));
    let guess = -(3.1415927 - inner) - now;
    let wrap_a = phi + spread - guess;
    let wrap_b = phi - spread - guess;
    let da = abs(atan2(sin(wrap_a), cos(wrap_a)));
    let db = abs(atan2(sin(wrap_b), cos(wrap_b)));
    let turn = select(phi - spread, phi + spread, da < db);
    let knee_turn = turn_about(knee, axis_angle(hinge, turn));
    let bent = (knee_turn * vec4<f32>(ankle, 1.0)).xyz;
    let upper = turn_about(hip, turn_onto(normalize(bent - hip), normalize(goal - hip)));
    let lower = upper * knee_turn;
    pose_fix[first] = upper * drop;
    pose_fix[first + 1u] = lower * drop;
    let foot = (lower * vec4<f32>(ankle, 1.0)).xyz;
    pose_fix[first + 2u] = translation(foot - ankle) * drop;
}

// Work out the rig fixes of the soldier at instance word `i`, once the
// joints' matrices are built: drop the body to the lower foot's ground (never
// raising it) and bend the legs up to their own, then turn chest, neck and
// head to look.
fn fit_rig(i: u32, rig: vec4<u32>, feet: f32, look: f32) {
    let pos = vec2<f32>(bitcast<f32>(instances[i]), bitcast<f32>(instances[i + 1u]));
    let facing = vec2<f32>(bitcast<f32>(instances[i + 2u]), bitcast<f32>(instances[i + 3u]));
    let scale = bitcast<f32>(instances[i + 8u]);
    let extra = instances[i + 15u];

    // Feet: how far each planted foot's ground is above the soldier's.
    var rise = vec2<f32>(0.0);
    if feet > 0.0 {
        let here = ground_at(pos);
        let left = vec2<f32>(facing.y, -facing.x);
        for (var k = 0u; k < 2u; k++) {
            let leg = rig[k];
            let hip = joint_at(rig_joint(leg, 0u));
            let knee = joint_at(rig_joint(leg, 1u));
            let ankle = joint_at(rig_joint(leg, 2u));
            let reach = length(knee - hip) + length(ankle - knee);
            let xz = pos + (left * ankle.x + facing * ankle.z) * scale;
            let dy = clamp((ground_at(xz) - here) / scale, -0.4 * reach, 0.4 * reach);
            rise[k] = dy * planted(ankle.y, reach) * feet;
        }
    }
    let drop_by = min(0.0, min(rise.x, rise.y));
    let drop = translation(vec3<f32>(0.0, drop_by, 0.0));
    for (var f = 0u; f < RIG_FIXES; f++) {
        pose_fix[f] = drop;
    }
    if feet > 0.0 {
        for (var k = 0u; k < 2u; k++) {
            bend_leg(rig[k], 1u + 3u * k, rise[k] - drop_by, drop);
        }
    }

    // Look: shared out over chest, neck and head, each turning by its share
    // more than the one below, so the head ends looking as asked.
    if look <= 0.0 {
        return;
    }
    let yaw = f32(bitcast<i32>(extra << 24u) >> 24u) / 127.0 * 1.5707964 * look;
    let pitch = f32(bitcast<i32>(extra << 16u) >> 24u) / 127.0 * 0.7853982 * look;
    var shares = vec3<f32>(0.3, 0.3, 0.4);
    for (var k = 0u; k < 3u; k++) {
        if rig_joint(rig.z, k) == NO_JOINT {
            shares[k] = 0.0;
        }
    }
    let total = shares.x + shares.y + shares.z;
    if total <= 0.0 {
        return;
    }
    var turned = drop;
    var done = 0.0;
    var was = look_turn(0.0, 0.0);
    for (var k = 0u; k < 3u; k++) {
        let j = rig_joint(rig.z, k);
        if j == NO_JOINT {
            continue;
        }
        done += shares[k] / total;
        let now = look_turn(yaw * done, pitch * done);
        let pivot = (turned * vec4<f32>(joint_at(j), 1.0)).xyz;
        turned = turn_about(pivot, now * transpose(was)) * turned;
        was = now;
        pose_fix[7u + k] = turned;
    }
}

// Build one soldier's final bone matrices, a thread per bone, into the
// palette, where every vertex that follows a bone then reads it.
//
// A model posed from bone rows blends its layers' matrices (a layer it
// cannot hold, an additive one, is skipped). A model fitted to a skeleton
// mixes each joint's pose from its layers (a layer only moves the joints its
// mask covers; an additive layer adds its change on top), builds the joints'
// matrices parent by parent, depth by depth (the roots taking the model's
// `pre`, from the skeleton's space to its own), and ends each with its
// bone's fix: `pre * global * post`.
@compute @workgroup_size(POSE_GROUP)
fn cs_pose(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) bone: u32,
) {
    let k = group.y * POSE_ROW + group.x;
    if k >= jobs.count.w {
        return;
    }
    let job = jobs.job[find_job(k, true)];
    let nth = k - job.pose.w;
    let soldier = job.run.x + nth;
    let bones = job.pose.y;
    let slot = job.pose.x + nth * bones + bone;
    let i = soldier * INSTANCE_WORDS;
    let fit = job.pose.z;

    if fit == 0u {
        if bone >= bones {
            return;
        }
        let base = instance_layer(i, 0u);
        var m = bone_rows(base.frames, bone, base.blend);
        for (var l = 1u; l <= POSE_LAYERS; l++) {
            let layer = instance_layer(i, l);
            if layer.weight > 0.0 && layer.mask == 0u && !layer.additive {
                let o = bone_rows(layer.frames, bone, layer.blend);
                m = mat3x4<f32>(
                    mix(m[0], o[0], layer.weight),
                    mix(m[1], o[1], layer.weight),
                    mix(m[2], o[2], layer.weight),
                );
            }
        }
        write_palette(slot, m);
        return;
    }

    // The fit: the skeleton's joints, how deep they nest and where its poses
    // start, then the model's fixes.
    let head = anim[fit];
    let joints_at = head.x;
    let joints = head.y;
    let deepest = head.z;
    let poses = Poses(head.w, joints);
    let joint = anim[joints_at + min(bone, joints - 1u)];
    let parent = joint.x;
    let depth = joint.y;
    let masks = joint.z;

    var local = mat4x4<f32>();
    if bone < joints {
        let base = instance_layer(i, 0u);
        var p = layer_pose(poses, base, bone);
        for (var l = 1u; l <= POSE_LAYERS; l++) {
            let layer = instance_layer(i, l);
            if layer.weight <= 0.0 || ((masks >> layer.mask) & 1u) == 0u {
                continue;
            }
            let o = layer_pose(poses, layer, bone);
            if layer.additive {
                // The layer's rows hold changes from its first frame.
                let id = vec4<f32>(0.0, 0.0, 0.0, 1.0);
                p.r = normalize(quat_mul(quat_nlerp(id, o.r, layer.weight), p.r));
                p.ts = vec4<f32>(p.ts.xyz + o.ts.xyz * layer.weight, p.ts.w * mix(1.0, o.ts.w, layer.weight));
            } else {
                p.r = quat_nlerp(p.r, o.r, layer.weight);
                p.ts = mix(p.ts, o.ts, layer.weight);
            }
        }
        local = pose_matrix(p);
    }
    // Parents before children: one depth at a time.
    for (var d = 0u; d <= deepest; d++) {
        if bone < joints && depth == d {
            if parent == 255u {
                pose_globals[bone] = anim_matrix(fit + 2u) * local;
            } else {
                pose_globals[bone] = pose_globals[parent] * local;
            }
        }
        workgroupBarrier();
    }
    // The rig's fixes: the body dropped and the legs bent to stand on the
    // ground, the upper body turned to look. Whether there are any is the
    // same for the whole workgroup.
    let rig = anim[fit + 1u];
    let feet = feet_weight(i, rig);
    let look = look_weight(i);
    let rigged = feet > 0.0 || look > 0.0;
    if rigged {
        if bone == 0u {
            fit_rig(i, rig, feet, look);
        }
        workgroupBarrier();
    }
    if bone > joints || bone >= bones {
        return;
    }
    if bone == joints {
        // The still bone never moves, but drops with the body.
        var still = translation(vec3<f32>(0.0));
        if rigged {
            still = pose_fix[0];
        }
        write_palette(slot, matrix_rows(still));
        return;
    }
    let post = anim_matrix(fit + 5u + bone * 3u);
    var global = pose_globals[bone];
    if rigged {
        global = pose_fix[(joint.w >> 16u) & 0xffu] * global;
    }
    write_palette(slot, matrix_rows(global * post));
}

// Pose one vertex of one soldier and keep it for the passes that draw it.
@compute @workgroup_size(128)
fn cs_skin(@builtin(global_invocation_id) id: vec3<u32>) {
    let n = id.x;
    if n >= jobs.count.y {
        return;
    }
    let job = find_job(n, false);
    let run = jobs.job[job].run;
    let run_pose = jobs.job[job].pose;
    let at = n - run.w;
    let soldier = run.x + at / run.z;
    let sv = set_vertex(run.y + at % run.z);

    // The instance (`SoldierInstance`).
    let i = soldier * INSTANCE_WORDS;
    let pos = vec2<f32>(bitcast<f32>(instances[i]), bitcast<f32>(instances[i + 1u]));
    let facing = vec2<f32>(bitcast<f32>(instances[i + 2u]), bitcast<f32>(instances[i + 3u]));
    let flags = instances[i + 5u];
    let lift = bitcast<f32>(instances[i + 7u]);
    let scale = bitcast<f32>(instances[i + 8u]);
    let fallen = ((flags >> 16u) & 0xffu) > 127u;

    let p = vec4<f32>(
        vec3<f32>(
            f32(bitcast<i32>(sv.xy << 16u) >> 16u),
            f32(bitcast<i32>(sv.xy) >> 16u),
            f32(bitcast<i32>(sv.zw << 16u) >> 16u),
        ) / 4096.0,
        1.0,
    );
    // Most vertices follow one or two bones; stop at the first empty slot.
    let base = run_pose.x + (soldier - run.x) * run_pose.y;
    var m = palette_bone(base + (sv.bones & 0xffu)) * (f32(sv.weights & 0xffu) / 255.0);
    for (var b = 1u; b < 4u; b++) {
        let w = f32((sv.weights >> (8u * b)) & 0xffu) / 255.0;
        if w <= 0.0 {
            break;
        }
        m += palette_bone(base + ((sv.bones >> (8u * b)) & 0xffu)) * w;
    }
    var local = vec3<f32>(dot(m[0], p), dot(m[1], p), dot(m[2], p)) * scale;
    if fallen {
        // No death clip: lay the soldier on his back.
        local = vec3<f32>(local.x, -local.z + 0.12 * scale, local.y);
    }
    let left = vec2<f32>(facing.y, -facing.x);
    let xz = pos + left * local.x + facing * local.z;
    let ground = ground_at(pos);
    let world = vec3<f32>(xz.x, ground + lift + local.y, xz.y);
    let normal = normalize(model_dir_to_world(
        m,
        vec3<f32>(snorm8(sv.normal, 0u), snorm8(sv.normal, 1u), snorm8(sv.normal, 2u)),
        facing,
        fallen,
    ) + vec3<f32>(0.0, 1e-4, 0.0));
    // Textured models only: the pixel shader renormalises it anyway.
    var tangent = model_dir_to_world(
        m,
        vec3<f32>(snorm8(sv.tangent, 0u), snorm8(sv.tangent, 1u), snorm8(sv.tangent, 2u)),
        facing,
        fallen,
    );
    tangent = tangent / max(length(tangent), 1e-6);

    out_pos[n * 3u] = world.x;
    out_pos[n * 3u + 1u] = world.y;
    out_pos[n * 3u + 2u] = world.z;
    out_shade[n * 3u] = pack2x16snorm(normal.xy);
    out_shade[n * 3u + 1u] = pack2x16snorm(vec2<f32>(normal.z, tangent.x));
    out_shade[n * 3u + 2u] = pack2x16snorm(tangent.yz);
}

// A posed vertex in the world.
struct Posed {
    clip: vec4<f32>,
    world: vec3<f32>,
    normal: vec3<f32>,
    tangent: vec3<f32>,
};

// Where `cs_skin` kept the vertex this invocation draws.
fn skin_slot(instance: u32, index: u32) -> u32 {
    return skin_origin[instance] + index;
}

fn skinned_world(slot: u32) -> vec3<f32> {
    return vec3<f32>(skin_pos[slot * 3u], skin_pos[slot * 3u + 1u], skin_pos[slot * 3u + 2u]);
}

// The vertex `cs_skin` kept for this invocation.
fn skinned(v: SoldierIn) -> Posed {
    let slot = skin_slot(v.instance, v.index);
    var out: Posed;
    out.world = skinned_world(slot);
    out.clip = g.view_proj * vec4<f32>(out.world, 1.0);
    let a = unpack2x16snorm(skin_shade[slot * 3u]);
    let b = unpack2x16snorm(skin_shade[slot * 3u + 1u]);
    let c = unpack2x16snorm(skin_shade[slot * 3u + 2u]);
    out.normal = vec3<f32>(a, b.x);
    out.tangent = vec3<f32>(b.y, c);
    return out;
}

@vertex
fn vs_soldier(v: SoldierIn) -> SoldierOut {
    let p = skinned(v);
    var out: SoldierOut;
    out.clip = p.clip;
    out.world = p.world;
    out.normal = p.normal;
    // Team parts take the team colour, keeping the painted light and shade.
    let team = f32(v.team_material.x) / 255.0;
    let shade_of = dot(v.vcolor.rgb, vec3<f32>(0.3, 0.59, 0.11));
    let tinted = v.team_color.rgb * (0.45 + shade_of * 1.1);
    let tone = tone_of(v.team_color.a);
    out.color = mix(
        v.vcolor.rgb * tone_mul(tone),
        tinted * tone_mul(tone / 3.0),
        team,
    );
    out.highlight = v.flags.y;
    return out;
}

// Position only, for the shadow map.
@vertex
fn vs_soldier_depth(
    @builtin(vertex_index) index: u32,
    @builtin(instance_index) instance: u32,
) -> @builtin(position) vec4<f32> {
    return g.view_proj * vec4<f32>(skinned_world(skin_slot(instance, index)), 1.0);
}

@fragment
fn fs_soldier(in: SoldierOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let albedo = srgb_to_linear(in.color);
    // Thin double-sided parts (cloaks, blades) light from whichever side shows.
    let n = select(-in.normal, in.normal, front);
    let c = shade(albedo, n, in.world) + albedo * rim(n, in.world);
    return vec4<f32>(c + albedo * in.highlight * HIGHLIGHT, 1.0);
}

@vertex
fn vs_soldier_textured(v: SoldierIn) -> TexturedOut {
    let p = skinned(v);
    var out: TexturedOut;
    out.clip = p.clip;
    out.world = p.world;
    out.normal = p.normal;
    out.tangent = vec4<f32>(p.tangent, v.tangent_w.y);
    out.uv = v.vuv;
    out.layer = (i32(v.team_material.y) - 1) * 3;
    out.team_color = srgb_to_linear(v.team_color.rgb);
    out.tone = tone_of(v.team_color.a);
    out.detail = v.detail_classes.y | (u32(round(v.vcolor.a * 255.0)) << 8u);
    out.highlight = v.flags.y;
    return out;
}

// Repeats across the texture atlas for each repeat code, 1 to 15 (`TILES` in
// gc_assets/src/detail.rs: 2, then half again more each step).
const DETAIL_TILES = array<f32, 15>(
    2.0, 3.0, 4.5, 6.8, 10.1, 15.2, 22.8, 34.2, 51.3, 76.9, 115.4, 173.1, 259.7, 389.6, 584.4,
);

// The detail fades out between half this many detail repeats under one pixel
// (32 texels of the 256, a little over mip 5) and all of it: past it the grain
// is averaged to grey, so it is skipped.
const DETAIL_FOOTPRINT: f32 = 32.0 / 256.0;

// The two layers of a detail class (albedo and cavity, relief) at `duv`.
struct DetailSample {
    albedo: vec4<f32>,
    relief: vec4<f32>,
};

fn detail_sample(cls: u32, duv: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> DetailSample {
    let layer = i32(cls - 1u) * 2;
    return DetailSample(
        textureSampleGrad(detail_array, detail_sampler, duv, layer, ddx, ddy),
        textureSampleGrad(detail_array, detail_sampler, duv, layer + 1, ddx, ddy),
    );
}

@fragment
fn fs_soldier_textured(in: TexturedOut) -> @location(0) vec4<f32> {
    let a = textureSample(materials_srgb, material_sampler, in.uv, in.layer);
    let b = textureSample(materials_linear, material_sampler, in.uv, in.layer + 1);
    let e = textureSample(materials_srgb, material_sampler, in.uv, in.layer + 2);
    // Team parts are painted in greys, light grey (0.8 linear) taking the
    // whole team colour and darker greys (folds, occlusion) less of it.
    let grey = dot(a.rgb, vec3<f32>(0.3, 0.59, 0.11));
    var albedo = mix(
        a.rgb * tone_mul(in.tone),
        in.team_color * min(grey * 1.25, 1.2) * tone_mul(in.tone / 3.0),
        a.a,
    );
    // Surface detail over the baked atlas: the pattern's grain at its own
    // repeat, which shades the colour, leans the normal, shifts the roughness
    // and darkens the cavities. Mips fade it to nothing with distance.
    var nxy = b.xy * 2.0 - 1.0;
    var rough = b.w;
    var ao = b.z;
    let class_a = in.detail & 15u;
    let class_b = (in.detail >> 4u) & 15u;
    let share_b = f32((in.detail >> 8u) & 15u) / 15.0;
    let repeat = (in.detail >> 12u) & 15u;
    let duv = in.uv * DETAIL_TILES[max(repeat, 1u) - 1u];
    let ddx = dpdx(duv);
    let ddy = dpdy(duv);
    // Full strength up close, fading to nothing as a pixel covers more detail.
    let footprint = sqrt(max(dot(ddx, ddx), dot(ddy, ddy)));
    let fade = 1.0 - smoothstep(0.5 * DETAIL_FOOTPRINT, DETAIL_FOOTPRINT, footprint);
    if class_a != 0u && repeat != 0u && fade > 0.0 {
        var d = detail_sample(class_a, duv, ddx, ddy);
        if class_b != 0u && share_b > 0.0 {
            let second = detail_sample(class_b, duv, ddx, ddy);
            d = DetailSample(
                mix(d.albedo, second.albedo, share_b),
                mix(d.relief, second.relief, share_b),
            );
        }
        albedo = albedo * mix(vec3<f32>(1.0), d.albedo.rgb * 2.0, fade);
        ao = ao * mix(1.0, d.albedo.a, fade);
        nxy = nxy + (d.relief.xy * 2.0 - 1.0) * fade;
        rough = clamp(rough + (d.relief.z - 0.5) * fade, 0.04, 1.0);
        nxy = nxy * min(1.0, 0.95 / max(length(nxy), 1e-5));
    }
    let n0 = in.normal;
    let t = normalize(in.tangent.xyz - n0 * dot(n0, in.tangent.xyz) + vec3<f32>(1e-5, 0.0, 0.0));
    let bt = cross(n0, t) * select(-1.0, 1.0, in.tangent.w >= 0.0);
    let nt = vec3<f32>(nxy, sqrt(max(1.0 - dot(nxy, nxy), 0.0)));
    let n = normalize(t * nt.x + bt * nt.y + n0 * nt.z);
    let c = shade_material(albedo, n, in.world, rough, e.a, ao, e.rgb) + albedo * rim(n, in.world);
    return vec4<f32>(c + albedo * in.highlight * HIGHLIGHT, 1.0);
}

// ---------------------------------------------------------------- contact shadows
//
// Under every soldier a soft dark patch on the ground, so feet sit in the
// grass rather than hover over it, whatever the light: the sun's shadow map
// is too coarse for a soldier's own feet and gone altogether under an
// overcast sky. One four-vertex strip per soldier, drawn after the soldiers.

// How far the patch reaches from the feet (tiles, at scale 1), and its
// darkest.
const CONTACT_RADIUS: f32 = 0.34;
const CONTACT_DARK: f32 = 0.7;

struct ContactIn {
    @builtin(vertex_index) index: u32,
    @location(0) pos: vec2<f32>,
    @location(1) lift: f32,
    @location(2) scale: f32,
};

struct ContactOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(perspective, centroid) uv: vec2<f32>,
    @location(1) @interpolate(flat) strength: f32,
};

@vertex
fn vs_contact(in: ContactIn) -> ContactOut {
    let corner = vec2<f32>(f32(in.index & 1u), f32((in.index >> 1u) & 1u)) * 2.0 - 1.0;
    let xz = in.pos + corner * CONTACT_RADIUS * in.scale;
    let world = vec3<f32>(xz.x, height_at(xz) + 0.015, xz.y);
    var out: ContactOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.uv = corner;
    // Fainter the higher the soldier is lifted off the ground, into the air
    // with distance, and out of sight in the fog of war (all measured at
    // the soldier: the patch is too small for them to vary across it).
    let feet = vec3<f32>(in.pos.x, world.y, in.pos.y);
    let air = 1.0 - air_amount(feet, distance(feet, g.camera_pos.xyz));
    out.strength = CONTACT_DARK * air * seen_at(in.pos) / (1.0 + max(in.lift, 0.0) * 6.0);
    return out;
}

@fragment
fn fs_contact(in: ContactOut) -> @location(0) vec4<f32> {
    let d = dot(in.uv, in.uv);
    let soft = 1.0 - smoothstep(0.0, 1.0, d);
    return vec4<f32>(0.0, 0.0, 0.0, in.strength * soft);
}

// ---------------------------------------------------------------- sky
//
// One triangle over the screen at the far plane, drawn after everything else
// (the depth test keeps it off every pixel something covers, so it shades
// only the pixels it shows). Where nothing is drawn over it (the horizon in a low camera, the void past the map's
// edge) the sky shows: from the horizon colour up to the colour overhead,
// with a glow and a disc round the sun.

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) vi: u32) -> SkyOut {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var out: SkyOut;
    out.ndc = uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);
    // At the far plane, so only what nothing was drawn over shows it.
    out.clip = vec4<f32>(out.ndc, 1.0, 1.0);
    return out;
}

// The sky's colour looking along `d`, with the sun's disc or without it
// (water mirrors the glow, not the disc: it glints instead).
fn sky_color(d: vec3<f32>, disc: bool) -> vec3<f32> {
    var c = mix(g.sky_horizon.rgb, g.sky_top.rgb, pow(clamp(d.y, 0.0, 1.0), 0.45));
    let s = max(dot(d, g.sun_dir.xyz), 0.0);
    c += g.sun_color.rgb * (pow(s, 12.0) * 0.25 + pow(s, 200.0) * 0.8);
    if disc {
        c += g.sun_color.rgb * smoothstep(0.9993, 0.9997, s) * 8.0;
    }
    return c;
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let far = g.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let d = normalize(far.xyz / far.w - g.camera_pos.xyz);
    return vec4<f32>(sky_color(d, true), 1.0);
}
