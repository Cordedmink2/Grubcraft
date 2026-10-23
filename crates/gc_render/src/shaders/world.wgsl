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
};

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
// Linear: soft fog edges, and the ground tint blended between texels.
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

// Dim and drain the colour of ground out of sight. Tile (i, j) is the texel
// at (i, j), so a position in tiles divided by the map size is its uv.
fn veil(c: vec3<f32>, xz: vec2<f32>) -> vec3<f32> {
    let size = vec2<f32>(textureDimensions(fog_map));
    let seen = textureSampleLevel(fog_map, soft_sampler, xz / size, 0.0).r;
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

const SUN_COLOR = vec3<f32>(1.0, 0.9, 0.74) * 1.9;

fn ambient_light(n: vec3<f32>) -> vec3<f32> {
    let sky = vec3<f32>(0.36, 0.45, 0.62);
    let ground = vec3<f32>(0.24, 0.18, 0.12);
    return mix(ground, sky, n.y * 0.5 + 0.5) * 0.45;
}

// Fade lit colour into the fog with distance. `fog.rgb` is already in the
// HDR scale (the colour that tone-maps to the sky).
fn finish(c: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let dist = distance(world, g.camera_pos.xyz);
    let fog = 1.0 - exp(-dist * g.fog.a);
    return mix(c, g.fog.rgb, clamp(fog, 0.0, 1.0));
}

// How much sunlight reaches a surface: 0 in shadow, 1 in full sun. Four
// filtered taps soften the edge over about three shadow texels, and the
// shadow fades out at the edge of the area the map covers.
fn sunlight(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if g.shadow.y < 0.5 {
        return 1.0;
    }
    let c = g.sun_view_proj * vec4<f32>(world + n * g.shadow.x, 1.0);
    let edge = max(abs(c.x), abs(c.y));
    if edge >= 1.0 || c.z >= 1.0 {
        return 1.0;
    }
    let uv = c.xy * vec2<f32>(0.5, -0.5) + 0.5;
    let t = 0.5 / f32(textureDimensions(shadow_map).x);
    var lit = 0.0;
    for (var i = 0u; i < 4u; i++) {
        let o = vec2<f32>(select(-t, t, (i & 1u) == 1u), select(-t, t, i >= 2u));
        lit += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + o, c.z);
    }
    return mix(lit * 0.25, 1.0, smoothstep(g.shadow.z, 1.0, edge));
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
    return finish(albedo * (SUN_COLOR * ndl + ambient_light(n)), world);
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
    let c = diffuse * (SUN_COLOR * ndl + ambient) + spec * SUN_COLOR * ndl + f0 * ambient * 0.6;
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
    let dry = srgb_to_linear(vec3<f32>(0.62, 0.55, 0.33));
    let bone = srgb_to_linear(vec3<f32>(0.85, 0.8, 0.68));
    var albedo = mix(soil, grass, smoothstep(0.16, 0.26, h));
    albedo = mix(albedo, dry, smoothstep(0.45, 0.6, h));
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

    var c: vec3<f32>;
    if rock > 0.0 {
        let r = rock_surface(in.world, n, dpx, dpy);
        // Strata: bands of colour by height, wobbled sideways.
        let band = vnoise(vec2<f32>(in.world.y * 1.5 + vnoise(xz * 0.25) * 3.0, 3.0));
        let warm = vnoise(vec2<f32>(in.world.y * 0.4, 9.0));
        // Rock035's own colour is near black (mean 0.01 linear), so it gives the
        // pattern only: its brightness, relative to its mean, scales a stone
        // colour from the game's palette.
        let tex_l = dot(r.albedo, vec3<f32>(0.2126, 0.7152, 0.0722));
        let pattern = pow(clamp(tex_l / 0.0095, 0.02, 6.0), 0.45);
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
        let ns = normalize(mix(n, r.n, rock));
        let rough = mix(1.0, max(r.rough, 0.75), rock);
        let ao = mix(1.0, cavity * tall, rock);
        c = shade_material(mix(albedo, rc, rock), ns, in.world, rough, 0.0, ao, vec3<f32>(0.0));
    } else {
        c = shade(albedo, n, in.world);
    }

    // Water below the water level: deep teal with a soft shoreline.
    let depth = g.misc.x - ground;
    if depth > 0.0 {
        let water = srgb_to_linear(vec3<f32>(0.16, 0.36, 0.4));
        let wn = vec3<f32>(0.0, 1.0, 0.0);
        let wc = shade(water, wn, vec3<f32>(in.world.x, g.misc.x, in.world.z));
        c = mix(c, wc, clamp(0.5 + depth * 1.5, 0.0, 0.92));
    }
    return vec4<f32>(veil(c, xz), 1.0);
}

// ---------------------------------------------------------------- regiments

struct BlockOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
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
    let corner = BOX_POS[BOX_IDX[vi]];
    let n_local = BOX_NORMAL[vi / 6u];
    let fwd = vec2<f32>(facing.x, facing.y);
    let right = vec2<f32>(-facing.y, facing.x);
    let xz = pos + right * (corner.x * size.x) + fwd * (corner.z * size.z);
    // Drape over the terrain: each column sits on the ground beneath it.
    let ground = max(height_at(xz), g.misc.x - 0.05);
    let world = vec3<f32>(xz.x, ground + lift + corner.y * size.y, xz.y);
    var out: BlockOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    let nxz = right * n_local.x + fwd * n_local.z;
    out.normal = normalize(vec3<f32>(nxz.x, n_local.y, nxz.y));
    out.color = color;
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

// The skinning pass's own bindings, in the same group of its own pipeline.
@group(1) @binding(8) var bones: texture_2d<f32>;
@group(1) @binding(9) var<storage, read> set_vertices: array<SetVertex>;
@group(1) @binding(10) var<storage, read> instances: array<u32>;
@group(1) @binding(11) var<uniform> jobs: Jobs;
@group(1) @binding(12) var<storage, read_write> out_pos: array<f32>;
@group(1) @binding(13) var<storage, read_write> out_shade: array<u32>;
// The model vertices past `jobs.count.z`, when a GPU binds too few at once.
@group(1) @binding(14) var<storage, read> set_vertices_hi: array<SetVertex>;

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

// A run of soldiers of one model at one level of detail: their first
// instance, the first vertex of the level's vertices and how many there are,
// and the first output slot. Thread `n` of the pass belongs to the last job
// whose slot is at or before it.
struct Job {
    run: vec4<u32>,
};

struct Jobs {
    // x: jobs, y: threads, z: model vertices in `set_vertices` (the rest
    // are in `set_vertices_hi`)
    count: vec4<u32>,
    job: array<Job, 1023>,
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
};

struct SoldierOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) highlight: f32,
};

struct TexturedOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) highlight: f32,
    // World-space tangent; w is the bitangent's handedness.
    @location(3) tangent: vec4<f32>,
    @location(4) uv: vec2<f32>,
    // The material's first layer in the array.
    @location(5) @interpolate(flat) layer: i32,
    @location(6) @interpolate(flat) team_color: vec3<f32>,
    // The soldier's tone, from -1 to 1 (0 leaves the model as painted).
    @location(7) @interpolate(flat) tone: f32,
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

// Pose one vertex of one soldier and keep it for the passes that draw it.
@compute @workgroup_size(128)
fn cs_skin(@builtin(global_invocation_id) id: vec3<u32>) {
    let n = id.x;
    if n >= jobs.count.y {
        return;
    }
    var lo = 0u;
    var hi = jobs.count.x;
    while hi - lo > 1u {
        let mid = (lo + hi) / 2u;
        if jobs.job[mid].run.w <= n {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let run = jobs.job[lo].run;
    let at = n - run.w;
    let soldier = run.x + at / run.z;
    let sv = set_vertex(run.y + at % run.z);

    // The instance (`SoldierInstance`), nine words.
    let i = soldier * 9u;
    let pos = vec2<f32>(bitcast<f32>(instances[i]), bitcast<f32>(instances[i + 1u]));
    let facing = vec2<f32>(bitcast<f32>(instances[i + 2u]), bitcast<f32>(instances[i + 3u]));
    let frames = vec2<u32>(instances[i + 4u] & 0xffffu, instances[i + 4u] >> 16u);
    let flags = instances[i + 5u];
    let lift = bitcast<f32>(instances[i + 7u]);
    let scale = bitcast<f32>(instances[i + 8u]);
    let t = f32(flags & 0xffu) / 255.0;
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
    var m = bone_rows(frames, sv.bones & 0xffu, t) * (f32(sv.weights & 0xffu) / 255.0);
    for (var b = 1u; b < 4u; b++) {
        let w = f32((sv.weights >> (8u * b)) & 0xffu) / 255.0;
        if w <= 0.0 {
            break;
        }
        m += bone_rows(frames, (sv.bones >> (8u * b)) & 0xffu, t) * w;
    }
    var local = vec3<f32>(dot(m[0], p), dot(m[1], p), dot(m[2], p)) * scale;
    if fallen {
        // No death clip: lay the soldier on his back.
        local = vec3<f32>(local.x, -local.z + 0.12 * scale, local.y);
    }
    let left = vec2<f32>(facing.y, -facing.x);
    let xz = pos + left * local.x + facing * local.z;
    let ground = max(height_at(pos), g.misc.x - 0.05);
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
    let c = shade(albedo, n, in.world);
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
    out.highlight = v.flags.y;
    return out;
}

@fragment
fn fs_soldier_textured(in: TexturedOut) -> @location(0) vec4<f32> {
    let a = textureSample(materials_srgb, material_sampler, in.uv, in.layer);
    let b = textureSample(materials_linear, material_sampler, in.uv, in.layer + 1);
    let e = textureSample(materials_srgb, material_sampler, in.uv, in.layer + 2);
    // Team parts are painted in greys, light grey (0.8 linear) taking the
    // whole team colour and darker greys (folds, occlusion) less of it.
    let grey = dot(a.rgb, vec3<f32>(0.3, 0.59, 0.11));
    let albedo = mix(
        a.rgb * tone_mul(in.tone),
        in.team_color * min(grey * 1.25, 1.2) * tone_mul(in.tone / 3.0),
        a.a,
    );
    let n0 = in.normal;
    let t = normalize(in.tangent.xyz - n0 * dot(n0, in.tangent.xyz) + vec3<f32>(1e-5, 0.0, 0.0));
    let bt = cross(n0, t) * select(-1.0, 1.0, in.tangent.w >= 0.0);
    let nxy = b.xy * 2.0 - 1.0;
    let nt = vec3<f32>(nxy, sqrt(max(1.0 - dot(nxy, nxy), 0.0)));
    let n = normalize(t * nt.x + bt * nt.y + n0 * nt.z);
    let c = shade_material(albedo, n, in.world, b.w, e.a, b.z, e.rgb);
    return vec4<f32>(c + albedo * in.highlight * HIGHLIGHT, 1.0);
}
