// Scenery: trees, bushes, grass and rocks. Appended to world.wgsl (see
// `scenery.rs`), so it shares `Globals`, `height_at`, `shade`, `veil` and the
// rest. Each model is an indexed vertex-coloured mesh in vertex buffer 0;
// each instance (`SceneryInstance`) is in vertex buffer 1. Group 0 only.

// `SWAY_MAX` (the most a vertex moves in the wind at scale 1 when its weight
// is 1, in tiles; the baker has already multiplied the model's own sway into
// the weight) and `POSITION_SCALE` (positions are stored in 1/POSITION_SCALE
// tiles) are prepended by `shader_source()` from gc_assets::scenery.

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
    // What surface detail to paint (`detail` in scenery.rs).
    @location(7) kind: u32,
};

struct SceneryOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(perspective, centroid) world: vec3<f32>,
    @location(1) @interpolate(perspective, centroid) normal: vec3<f32>,
    @location(2) @interpolate(perspective, centroid) color: vec3<f32>,
    // 1 for leaves and blades (what moves in the wind), 0 for rocks and bases.
    @location(3) @interpolate(perspective, centroid) leafy: f32,
    // The position and normal in the model's own space (scaled to tiles, not
    // turned), which the detail textures stick to, the way the model faces,
    // and its kind.
    @location(4) @interpolate(perspective, centroid) local: vec3<f32>,
    @location(5) @interpolate(perspective, centroid) normal_local: vec3<f32>,
    @location(6) @interpolate(perspective, centroid) facing: vec2<f32>,
    @location(7) @interpolate(flat) kind: u32,
    // How green the baked colour is, 0 (bark) to 1 (leaf), before the tint.
    @location(8) @interpolate(perspective, centroid) leaf: f32,
    // Close-range occlusion baked into the vertex, 0 (open) to 1 (shut).
    @location(9) @interpolate(perspective, centroid) occ: f32,
    // How far the instance is tipped over (radians), and the way it falls
    // in the model's own space as (x, z): see `tip_of`.
    @location(10) @interpolate(flat) tip: vec3<f32>,
    // The instance's base on the map, for colour that stays put as it tips.
    @location(11) @interpolate(flat) at: vec2<f32>,
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

// How far an instance is tipped over about its base (radians) and the way
// it falls, for trees coming down: `kind`'s second byte is the tip, 0
// (upright) to 255 (lying down), and its third is the direction of the fall
// in the model's own space, as a turn from the way the model faces (256ths).
// Returned as (tip, x, z) with (x, z) a unit vector.
fn tip_of(kind: u32) -> vec3<f32> {
    let tip = f32((kind >> 8u) & 255u) * (1.5707964 / 255.0);
    let way = f32((kind >> 16u) & 255u) * (6.2831855 / 256.0);
    return vec3<f32>(tip, sin(way), cos(way));
}

// (`tipped` is in world.wgsl, shared with the soldiers and buildings.)

// Where one vertex of one instance stands, and its normal.
fn place(v: SceneryIn) -> vec3<f32> {
    let left = vec2<f32>(v.facing.y, -v.facing.x);
    var local = vec3<f32>(v.pos.xyz) * (v.scale / POSITION_SCALE);
    let t = tip_of(v.kind);
    if t.x > 0.0 {
        local = tipped(local, t);
    }
    let flat_xz = v.at + left * local.x + v.facing * local.z;
    // The whole model stands at the height under its base; it is not draped.
    let ground = max(surface_height(v.at), g.misc.x - 0.05);
    if t.x > 0.0 {
        // A crown lying down rests on the ground under it, not in it.
        local.y = max(local.y, max(surface_height(flat_xz), g.misc.x - 0.05) - ground);
    }
    var sway = vec2<f32>(0.0);
    // Rocks and bases do not move: skip the wind for them. A tree lying
    // down does not either.
    if v.color.a > 0.0 {
        sway = gust(v.at, v.tint.a) * (v.color.a * SWAY_MAX * v.scale * cos(t.x));
    }
    return vec3<f32>(flat_xz.x + sway.x, ground + local.y, flat_xz.y + sway.y);
}

@vertex
fn vs_scenery(v: SceneryIn) -> SceneryOut {
    let world = place(v);
    let left = vec2<f32>(v.facing.y, -v.facing.x);
    var nl = v.normal.xyz;
    let t = tip_of(v.kind);
    if t.x > 0.0 {
        nl = tipped(nl, t);
    }
    let n_xz = left * nl.x + v.facing * nl.z;
    var out: SceneryOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = normalize(vec3<f32>(n_xz.x, nl.y, n_xz.y) + vec3<f32>(0.0, 1e-4, 0.0));
    out.color = min(v.color.rgb * v.tint.rgb * 2.0, vec3<f32>(1.0));
    out.leafy = select(0.0, 1.0, v.color.a > 0.0);
    out.local = vec3<f32>(v.pos.xyz) * (v.scale / POSITION_SCALE);
    out.normal_local = v.normal.xyz;
    out.facing = v.facing;
    out.kind = v.kind & 255u;
    out.tip = t;
    out.at = v.at;
    out.occ = max(v.normal.w, 0.0);
    out.leaf = smoothstep(0.0, 0.06, v.color.g - max(v.color.r, v.color.b) * 0.95);
    return out;
}

// Position only, for the shadow map.
@vertex
fn vs_scenery_depth(v: SceneryIn) -> @builtin(position) vec4<f32> {
    return g.view_proj * vec4<f32>(place(v), 1.0);
}

// ---------------------------------------------------------------- detail
//
// The models are vertex-coloured, with no texture coordinates, so detail
// textures are projected from the model's own space (triplanar, as the cliffs
// are) and stick to it as it turns and scales. As on the ground, a texture's
// colour relative to its own mean multiplies the baked colour (mostly its
// light and dark), and its normal map tilts the surface. Stone is the cliffs'
// Rock035; bark and foliage use the scenery arrays. Nothing is read beyond
// a kind's reach from the eye, where mips would have flattened it.

const DETAIL_NONE: u32 = 0u;
const DETAIL_BROADLEAF: u32 = 1u;
const DETAIL_CONIFER: u32 = 2u;
const DETAIL_FOLIAGE: u32 = 3u;
const DETAIL_STONE: u32 = 4u;
const DETAIL_MOSSY_STONE: u32 = 5u;
// Tiles from the eye: grass and bushes, stone, trees.
const FOLIAGE_REACH: f32 = 36.0;
const STONE_REACH: f32 = 60.0;
const TREE_REACH: f32 = 70.0;
// Texture repeats a tile of model space.
const BARK_SCALE: f32 = 1.4;
const FOLIAGE_SCALE: f32 = 5.0;
// How dark a fully shut vertex goes (the creases between a crown's lobes).
const OCCLUDED_DARK: f32 = 0.85;
// How hard a leaf texture's relief tilts the surface (1 is the map as drawn;
// more makes the crown's grain catch the light harder).
const LEAF_RELIEF: f32 = 1.3;
const STONE_SCALE: f32 = 5.0;
// Linear mean colours of the albedo maps, to turn them into ratios.
const BARK_BROAD_MEAN: vec3<f32> = vec3<f32>(0.2285, 0.2208, 0.2026);
const BARK_CONIFER_MEAN: vec3<f32> = vec3<f32>(0.1994, 0.1389, 0.0858);
const MOSS_MEAN: vec3<f32> = vec3<f32>(0.0723, 0.1366, 0.0107);

struct Detail {
    // Multiplies the baked colour.
    ratio: vec3<f32>,
    // The normal map's x and y, along the plane's two axes.
    tilt: vec2<f32>,
};

fn scenery_sample(layer: i32, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, mean: vec3<f32>) -> Detail {
    var r: Detail;
    r.ratio = min(textureSampleGrad(scenery_color, rock_sampler, uv, layer, dx, dy).rgb / mean, vec3<f32>(3.0));
    r.tilt = textureSampleGrad(scenery_normal, rock_sampler, uv, layer, dx, dy).xy * 2.0 - 1.0;
    return r;
}

// A layer's colour ratio and the model-space normal tilted by its normal map.
struct Surface {
    ratio: vec3<f32>,
    normal: vec3<f32>,
};

// A layer of the scenery arrays projected triplanar from model space at
// `scale` repeats a tile, with `nl` the model-space normal. Rows run down and
// the map's green runs up, so each plane's second coordinate is negated. `top`
// (0 to 1) is how much the horizontal plane counts: bark leaves it out (a
// trunk's ends are tiny).
fn triplanar(layer: i32, mean: vec3<f32>, scale: f32, top: f32, p: vec3<f32>, nl: vec3<f32>, dpx: vec3<f32>, dpy: vec3<f32>) -> Surface {
    var w = vec3<f32>(nl.z * nl.z, nl.x * nl.x, nl.y * nl.y * top) + vec3<f32>(0.02, 0.02, 0.0);
    w = w / (w.x + w.y + w.z);
    var ratio = vec3<f32>(0.0);
    var tilt = vec3<f32>(0.0);
    var used = 0.0;
    // Facing +z or -z: the (x, y) plane.
    if w.x > 0.03 {
        let d = scenery_sample(layer, vec2<f32>(p.x, -p.y) * scale, vec2<f32>(dpx.x, -dpx.y) * scale, vec2<f32>(dpy.x, -dpy.y) * scale, mean);
        ratio += d.ratio * w.x;
        tilt += vec3<f32>(d.tilt.x, d.tilt.y, 0.0) * w.x;
        used += w.x;
    }
    // Facing +x or -x: the (z, y) plane.
    if w.y > 0.03 {
        let d = scenery_sample(layer, vec2<f32>(p.z, -p.y) * scale, vec2<f32>(dpx.z, -dpx.y) * scale, vec2<f32>(dpy.z, -dpy.y) * scale, mean);
        ratio += d.ratio * w.y;
        tilt += vec3<f32>(0.0, d.tilt.y, d.tilt.x) * w.y;
        used += w.y;
    }
    // Facing up or down: the (x, z) plane.
    if w.z > 0.03 {
        let d = scenery_sample(layer, vec2<f32>(p.x, -p.z) * scale, vec2<f32>(dpx.x, -dpx.z) * scale, vec2<f32>(dpy.x, -dpy.z) * scale, mean);
        ratio += d.ratio * w.z;
        tilt += vec3<f32>(d.tilt.x, 0.0, d.tilt.y) * w.z;
        used += w.z;
    }
    // Planes that were skipped leave their share out; the rest stand in.
    var r: Surface;
    r.ratio = ratio / used;
    r.normal = normalize(nl + tilt / used);
    return r;
}

// The same, from the one plane the surface faces most: a third of the cost,
// for foliage, where the leaves are small and busy enough to hide the seams.
fn single_plane(layer: i32, mean: vec3<f32>, scale: f32, p: vec3<f32>, nl: vec3<f32>, dpx: vec3<f32>, dpy: vec3<f32>) -> Surface {
    let a = abs(nl);
    var uv = vec2<f32>(p.x, -p.y);
    var dx = vec2<f32>(dpx.x, -dpx.y);
    var dy = vec2<f32>(dpy.x, -dpy.y);
    var axes = 0;
    if a.y >= a.x && a.y >= a.z {
        uv = vec2<f32>(p.x, -p.z);
        dx = vec2<f32>(dpx.x, -dpx.z);
        dy = vec2<f32>(dpy.x, -dpy.z);
        axes = 1;
    } else if a.x >= a.z {
        uv = vec2<f32>(p.z, -p.y);
        dx = vec2<f32>(dpx.z, -dpx.y);
        dy = vec2<f32>(dpy.z, -dpy.y);
        axes = 2;
    }
    let d = scenery_sample(layer, uv * scale, dx * scale, dy * scale, mean);
    var tilt = vec3<f32>(d.tilt.x, d.tilt.y, 0.0);
    if axes == 1 {
        tilt = vec3<f32>(d.tilt.x, 0.0, d.tilt.y);
    } else if axes == 2 {
        tilt = vec3<f32>(0.0, d.tilt.y, d.tilt.x);
    }
    var r: Surface;
    r.ratio = d.ratio;
    r.normal = normalize(nl + tilt);
    return r;
}

@fragment
fn fs_scenery(in: SceneryOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // Derivatives first, while control flow is uniform.
    let dpx = dpdx(in.local);
    let dpy = dpdy(in.local);
    // A little colour noise by place, so a wood is not one flat green.
    let wobble = vnoise((in.at + in.local.xz) * 1.7 + vec2<f32>(in.local.y * 0.9, 0.0));
    var albedo = srgb_to_linear(in.color) * (0.91 + 0.18 * wobble);
    // Leaves and blades may be single planes: light them from the side shown.
    var n = select(-in.normal, in.normal, front);

    var reach = TREE_REACH;
    if in.kind == DETAIL_FOLIAGE {
        reach = FOLIAGE_REACH;
    } else if in.kind >= DETAIL_STONE {
        reach = STONE_REACH;
    }
    if in.kind != DETAIL_NONE && distance(in.world, g.camera_pos.xyz) < reach {
        let nl = select(-in.normal_local, in.normal_local, front);
        var detailed = nl;
        if in.kind >= DETAIL_STONE {
            let r = rock_surface(in.local * STONE_SCALE, nl, dpx * STONE_SCALE, dpy * STONE_SCALE);
            albedo = albedo * mix(1.0, rock_pattern(r.albedo), 0.85);
            detailed = r.n;
            if in.kind == DETAIL_MOSSY_STONE {
                // Moss settles on what faces up.
                let up = detailed.y;
                let clump = vnoise(in.local.xz * 3.0 + vec2<f32>(in.local.y * 2.0, 4.0));
                let moss = smoothstep(0.45, 0.8, up) * smoothstep(0.25, 0.7, clump);
                let moss_tex = scenery_sample(2, in.local.xz * 2.0, dpx.xz * 2.0, dpy.xz * 2.0, MOSS_MEAN);
                albedo = mix(albedo, srgb_to_linear(vec3<f32>(0.22, 0.34, 0.12)) * moss_tex.ratio, moss * 0.8);
            }
        } else {
            // Trees: bark where the baked colour is not green, leaves where it is.
            var leaf = 1.0;
            if in.kind != DETAIL_FOLIAGE {
                leaf = in.leaf;
            }
            var ratio = vec3<f32>(1.0);
            if leaf > 0.02 {
                let f = single_plane(2, MOSS_MEAN, FOLIAGE_SCALE, in.local, nl, dpx, dpy);
                ratio = mix(ratio, muted(f.ratio, 0.5), leaf);
                detailed = normalize(nl + (f.normal - nl) * LEAF_RELIEF);
            }
            if leaf < 0.98 {
                var b: Surface;
                if in.kind == DETAIL_CONIFER {
                    b = triplanar(1, BARK_CONIFER_MEAN, BARK_SCALE, 0.0, in.local, nl, dpx, dpy);
                } else {
                    b = triplanar(0, BARK_BROAD_MEAN, BARK_SCALE, 0.0, in.local, nl, dpx, dpy);
                }
                ratio = mix(muted(b.ratio, 0.3), ratio, leaf);
                detailed = normalize(mix(b.normal, detailed, leaf));
            }
            albedo = albedo * ratio;
        }
        // Back to the world: the model is turned about the vertical.
        if in.tip.x > 0.0 {
            detailed = tipped(detailed, in.tip);
        }
        let left = vec2<f32>(in.facing.y, -in.facing.x);
        let xz = left * detailed.x + in.facing * detailed.z;
        n = normalize(vec3<f32>(xz.x, detailed.y, xz.y));
    }
    albedo *= 1.0 - OCCLUDED_DARK * in.occ;
    albedo *= 1.0 - WET_DARK * drench(n);
    let unsnowed = albedo;
    albedo = mix(albedo, SNOW_ALBEDO, dusting(n) * SNOW_ON_SCENERY);
    var c = shade(albedo, n, in.world);
    // Leaves and blades pass light on: with the sun behind them they glow,
    // yellower than they are lit from the front.
    let behind = max(dot(-n, g.sun_dir.xyz), 0.0);
    if in.leafy > 0.5 && behind > 0.0 {
        let glow = unsnowed * vec3<f32>(1.2, 1.1, 0.7) * g.sun_color.rgb * behind * sunlight_soft(in.world, -n);
        c += glow * g.sun_color.a * (1.0 - air_amount(in.world, distance(in.world, g.camera_pos.xyz)));
    }
    return vec4<f32>(veil(c, in.world.xz), 1.0);
}
