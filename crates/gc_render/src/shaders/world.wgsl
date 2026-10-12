// Terrain and regiment-block shaders for M0.
//
// Neither pipeline uses a vertex buffer for geometry: terrain vertices are
// generated from the vertex index over a shared chunk grid, and heights come
// from an R16Uint heightmap; regiment boxes are generated from the vertex index.
// Only per-instance data (chunk coords, regiment transforms) is uploaded.

struct Globals {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    // xyz: direction toward the sun (normalised)
    sun_dir: vec4<f32>,
    // x: heightmap samples per tile, y: height of the max u16 value in tiles,
    // z: tiles per chunk side, w: quads per chunk side
    map: vec4<f32>,
    // rgb: fog colour, a: fog density
    fog: vec4<f32>,
    // x: water level in tiles, y: time in seconds
    misc: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var heightmap: texture_2d<u32>;

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

fn terrain_normal(xz: vec2<f32>) -> vec3<f32> {
    let e = 1.0 / g.map.x;
    let hl = height_at(xz - vec2<f32>(e, 0.0));
    let hr = height_at(xz + vec2<f32>(e, 0.0));
    let hu = height_at(xz - vec2<f32>(0.0, e));
    let hd = height_at(xz + vec2<f32>(0.0, e));
    return normalize(vec3<f32>(hl - hr, 2.0 * e, hu - hd));
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

// Soft filmic curve so bright sunlit slopes don't clip to white.
fn tonemap(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(1.0) - exp(-c * 1.25);
}

fn shade(albedo: vec3<f32>, n: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let sun = g.sun_dir.xyz;
    let ndl = max(dot(n, sun), 0.0);
    let sun_col = vec3<f32>(1.0, 0.9, 0.74) * 1.9;
    let sky = vec3<f32>(0.36, 0.45, 0.62);
    let ground = vec3<f32>(0.24, 0.18, 0.12);
    let ambient = mix(ground, sky, n.y * 0.5 + 0.5) * 0.45;
    var c = albedo * (sun_col * ndl + ambient);
    let dist = distance(world, g.camera_pos.xyz);
    let fog = 1.0 - exp(-dist * g.fog.a);
    c = mix(tonemap(c), srgb_to_linear(g.fog.rgb), clamp(fog, 0.0, 1.0));
    return c;
}

// ---------------------------------------------------------------- terrain

struct TerrainOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_terrain(@builtin(vertex_index) vi: u32, @location(0) chunk: vec2<u32>) -> TerrainOut {
    let quads = u32(g.map.w);
    let side = quads + 1u;
    let local = vec2<f32>(f32(vi % side), f32(vi / side));
    let chunk_tiles = g.map.z;
    let xz = vec2<f32>(chunk) * chunk_tiles + local * (chunk_tiles / f32(quads));
    let world = vec3<f32>(xz.x, height_at(xz), xz.y);
    var out: TerrainOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    return out;
}

@fragment
fn fs_terrain(in: TerrainOut) -> @location(0) vec4<f32> {
    let xz = in.world.xz;
    let n = terrain_normal(xz);
    let h = in.world.y / g.map.y;
    let slope = 1.0 - n.y;

    // Painted palette: Grub-soaked soil low, grass mid, bone-white ridges high.
    // Albedos are authored in sRGB and converted to linear for lighting.
    let soil = srgb_to_linear(vec3<f32>(0.42, 0.31, 0.2));
    let grass = srgb_to_linear(vec3<f32>(0.4, 0.5, 0.22));
    let dry = srgb_to_linear(vec3<f32>(0.62, 0.55, 0.33));
    let bone = srgb_to_linear(vec3<f32>(0.85, 0.8, 0.68));
    var albedo = mix(soil, grass, smoothstep(0.16, 0.26, h));
    albedo = mix(albedo, dry, smoothstep(0.45, 0.6, h));
    albedo = mix(albedo, bone, smoothstep(0.62, 0.78, h));
    albedo = mix(albedo, bone * 0.8, smoothstep(0.25, 0.5, slope));
    // Large-scale painterly variation so flat areas aren't flat colour.
    let v = vnoise(xz * 0.08) * 0.16 + vnoise(xz * 0.5) * 0.08;
    albedo = albedo * (0.88 + v);

    var c = shade(albedo, n, in.world);

    // Water below the water level: deep teal with a soft shoreline.
    let depth = g.misc.x - in.world.y;
    if depth > 0.0 {
        let water = srgb_to_linear(vec3<f32>(0.16, 0.36, 0.4));
        let wn = vec3<f32>(0.0, 1.0, 0.0);
        let wc = shade(water, wn, vec3<f32>(in.world.x, g.misc.x, in.world.z));
        c = mix(c, wc, clamp(0.5 + depth * 1.5, 0.0, 0.92));
    }
    return vec4<f32>(c, 1.0);
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
    return vec4<f32>(c + albedo * in.color.a * 0.8, 1.0);
}

// ---------------------------------------------------------------- soldiers
//
// Real soldier models (M1 step 8): each vertex is rigidly bound to one bone,
// and the bone texture holds, per animation frame, a 3 × 4 matrix per bone
// that moves the rest pose into that frame's pose. Each instance names two
// frame rows and how far to blend between them.

@group(1) @binding(0) var bones: texture_2d<f32>;

struct SoldierOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) highlight: f32,
};

fn bone_texel(row: u32, bone: u32, r: u32) -> vec4<f32> {
    return textureLoad(bones, vec2<i32>(i32(bone * 3u + r), i32(row)), 0);
}

@vertex
fn vs_soldier(
    // x, y, z in 1/4096 tiles; w packs the bone (low byte) and team weight.
    @location(0) vpos: vec4<i32>,
    @location(1) vnormal: vec4<f32>,
    @location(2) vcolor: vec4<f32>,
    @location(3) pos: vec2<f32>,
    @location(4) facing: vec2<f32>,
    @location(5) frames: vec2<u32>,
    // x: blend toward frames.y, y: selection highlight, z: lies fallen
    @location(6) flags: vec4<f32>,
    @location(7) team_color: vec4<f32>,
    @location(8) lift: f32,
    @location(9) scale: f32,
) -> SoldierOut {
    let packed = bitcast<u32>(vpos.w);
    let bone = packed & 0xffu;
    let team = f32((packed >> 8u) & 0xffu) / 255.0;
    let p = vec4<f32>(vec3<f32>(vpos.xyz) / 4096.0, 1.0);
    let t = flags.x;
    let r0 = mix(bone_texel(frames.x, bone, 0u), bone_texel(frames.y, bone, 0u), t);
    let r1 = mix(bone_texel(frames.x, bone, 1u), bone_texel(frames.y, bone, 1u), t);
    let r2 = mix(bone_texel(frames.x, bone, 2u), bone_texel(frames.y, bone, 2u), t);
    var local = vec3<f32>(dot(r0, p), dot(r1, p), dot(r2, p)) * scale;
    let n = vnormal.xyz;
    var nl = vec3<f32>(dot(r0.xyz, n), dot(r1.xyz, n), dot(r2.xyz, n));
    if flags.z > 0.5 {
        // No death clip: lay the soldier on his back.
        local = vec3<f32>(local.x, -local.z + 0.12 * scale, local.y);
        nl = vec3<f32>(nl.x, -nl.z, nl.y);
    }
    // Model space is x to the soldier's left, y up, z forward (glTF's
    // convention), so x maps to the left of the facing.
    let fwd = facing;
    let left = vec2<f32>(facing.y, -facing.x);
    let xz = pos + left * local.x + fwd * local.z;
    let ground = max(height_at(pos), g.misc.x - 0.05);
    let world = vec3<f32>(xz.x, ground + lift + local.y, xz.y);
    var out: SoldierOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    let nxz = left * nl.x + fwd * nl.z;
    out.normal = normalize(vec3<f32>(nxz.x, nl.y, nxz.y) + vec3<f32>(0.0, 1e-4, 0.0));
    // Team parts take the team colour, keeping the painted light and shade.
    let shade_of = dot(vcolor.rgb, vec3<f32>(0.3, 0.59, 0.11));
    let tinted = team_color.rgb * (0.45 + shade_of * 1.1);
    out.color = mix(vcolor.rgb, tinted, team);
    out.highlight = flags.y;
    return out;
}

@fragment
fn fs_soldier(in: SoldierOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let albedo = srgb_to_linear(in.color);
    // Thin double-sided parts (cloaks, blades) light from whichever side shows.
    let n = select(-in.normal, in.normal, front);
    let c = shade(albedo, n, in.world);
    return vec4<f32>(c + albedo * in.highlight * 0.8, 1.0);
}
