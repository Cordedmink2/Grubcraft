// Particles. Appended to world.wgsl (see `particles.rs`), so it shares
// `Globals`, `surface_height`, `sunlight`, `finish` and the fog map. The
// `FX_*` flag constants are prepended from `particles::flags`. Group 1 is
// the atlas; every particle is one instance (`ParticleInstance`) drawn as a
// four-vertex strip, and the effects clock is `g.misc.z`.

@group(1) @binding(0) var fx_atlas: texture_2d<f32>;
@group(1) @binding(1) var fx_sampler: sampler;

// How much brighter than its colour an additive particle shines (HDR; the
// bloom takes the rest).
const FX_GAIN: f32 = 2.6;
// Pictures across and down the atlas (`FX_COLUMNS` and `FX_ROWS` are
// prepended from `particles::CELLS`).
const FX_CELLS = vec2<f32>(f32(FX_COLUMNS), f32(FX_ROWS));

struct FxIn {
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec3<f32>,
    @location(1) vel: vec3<f32>,
    @location(2) start: f32,
    @location(3) life: f32,
    // Radius at birth and at death.
    @location(4) size: vec2<f32>,
    @location(5) color: vec4<f32>,
    @location(6) color_end: vec4<f32>,
    // Gravity, drag and lift.
    @location(7) motion: vec3<f32>,
    // Shape, flags, spin and seed.
    @location(8) bits: vec4<u32>,
};

struct FxOut {
    @builtin(position) clip: vec4<f32>,
    // The picture's coordinates (xy) and the unspun ones the lighting uses (zw).
    @location(0) uv: vec4<f32>,
    // Where its picture is in the atlas (xy); z is 1 for additive particles.
    @location(1) @interpolate(flat) cell: vec3<f32>,
    // Lit colour (rgb, linear) and opacity.
    @location(2) color: vec4<f32>,
    // The sun in the particle's own view basis (xyz), and the extra light a
    // particle between the camera and the sun passes on (w).
    @location(3) sun_local: vec4<f32>,
    @location(4) sun: vec3<f32>,
    @location(5) amb: vec3<f32>,
    // The air in front: its colour to add (rgb) and the share of the
    // particle's own light that gets through (w).
    @location(6) air: vec4<f32>,
    // Height of this corner, of the ground under the particle, and how far
    // above the ground it fades in (0 for no fade).
    @location(7) height: vec3<f32>,
};

// Where a particle has got to after `t` seconds from its spawn point: its
// push, a constant acceleration (lift less gravity, up) and drag, in closed
// form. A tiny drag stands in for none.
fn fx_displacement(p: FxIn, t: f32) -> vec3<f32> {
    let k = max(p.motion.y, 0.02);
    let a = vec3<f32>(0.0, p.motion.z - p.motion.x, 0.0);
    let e = (1.0 - exp(-k * t)) / k;
    return p.vel * e + a * ((t - e) / k);
}

fn fx_velocity(p: FxIn, t: f32) -> vec3<f32> {
    let k = max(p.motion.y, 0.02);
    let a = vec3<f32>(0.0, p.motion.z - p.motion.x, 0.0);
    return (p.vel - a / k) * exp(-k * t) + a / k;
}

@vertex
fn vs_particle(p: FxIn) -> FxOut {
    var out: FxOut;
    // Outside the clip volume: a degenerate quad, nothing drawn.
    out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    let age = g.misc.z - p.start;
    if age < 0.0 || age >= p.life {
        return out;
    }
    let u = age / p.life;
    let flags = p.bits.y;
    let seed = f32(p.bits.w);

    var world = p.pos + fx_displacement(p, age);
    if (flags & FX_WIND) != 0u {
        world += vec3<f32>(WIND_DIR.x, 0.0, WIND_DIR.y) * (0.45 * age * smoothstep(0.0, 1.5, age));
    }
    let radius = mix(p.size.x, p.size.y, 1.0 - (1.0 - u) * (1.0 - u));
    // The ground's height only matters to particles that rest on it or fade
    // out toward it.
    var ground = 0.0;
    if (flags & (FX_GROUND | FX_SOFT)) != 0u {
        ground = max(surface_height(world.xz), g.misc.x - 0.05);
    }
    if (flags & FX_GROUND) != 0u {
        world.y = max(world.y, ground + radius * 0.4);
    }

    // A quad facing the camera.
    let fwd = normalize(world - g.camera_pos.xyz);
    var right = cross(fwd, vec3<f32>(0.0, 1.0, 0.0));
    let right_len = length(right);
    right = select(vec3<f32>(1.0, 0.0, 0.0), right / max(right_len, 1e-4), right_len > 1e-3);
    let up = cross(right, fwd);
    let corner = vec2<f32>(f32(p.vi & 1u) * 2.0 - 1.0, f32(p.vi >> 1u) * 2.0 - 1.0);
    var offset: vec3<f32>;
    var lit_uv: vec2<f32>;
    if (flags & FX_STRETCH) != 0u {
        // A streak along the velocity as seen from the camera, its head at
        // the particle and its tail behind.
        let v = fx_velocity(p, age);
        let across = v - fwd * dot(v, fwd);
        let l = length(across);
        let axis = select(right, across / max(l, 1e-4), l > 1e-3);
        let side = cross(fwd, axis);
        let half_len = radius + length(v) * 0.06;
        offset = axis * ((corner.x - 0.6) * half_len) + side * (corner.y * radius);
        lit_uv = vec2<f32>(0.0);
    } else {
        let spin = f32(i32(p.bits.z) - select(0, 256, p.bits.z > 127u)) * 0.25;
        let angle = seed * (6.2832 / 256.0) + spin * age;
        let cs = cos(angle);
        let sn = sin(angle);
        let turned = vec2<f32>(corner.x * cs - corner.y * sn, corner.x * sn + corner.y * cs);
        offset = (right * turned.x + up * turned.y) * radius;
        lit_uv = turned;
    }
    let corner_world = world + offset;
    out.clip = g.view_proj * vec4<f32>(corner_world, 1.0);
    out.uv = vec4<f32>(corner, lit_uv);

    // Colour and opacity along its life: in fast, out slowly.
    let col = mix(p.color, p.color_end, u);
    var alpha = col.a * smoothstep(0.0, 0.1, u) * (1.0 - smoothstep(0.35, 1.0, u));
    if (flags & FX_FLICKER) != 0u {
        alpha *= 0.8 + 0.2 * sin(g.misc.z * (11.0 + seed * 0.05) + seed * 1.7);
    }
    // Out of sight in the fog of war: hidden where the viewer cannot see.
    let seen = seen_at(world.xz);
    alpha *= smoothstep(0.55, 0.9, seen);
    let rgb = srgb_to_linear(col.rgb) * (0.1 + 0.9 * seen);
    out.color = vec4<f32>(rgb, alpha);

    // Light at the particle's centre; the puff is shaded across its face
    // like a sphere in the fragment shader.
    let sun = g.sun_dir.xyz;
    out.sun_local = vec4<f32>(
        dot(sun, right),
        dot(sun, up),
        dot(sun, -fwd),
        // Dust between the eye and a low sun glows.
        pow(max(dot(fwd, sun), 0.0), 5.0) * 1.2,
    );
    // Added light is not shaded.
    if (flags & FX_ADDITIVE) == 0u {
        out.sun = g.sun_color.rgb * sunlight_soft(world, vec3<f32>(0.0, 1.0, 0.0));
        out.amb = ambient_light(vec3<f32>(0.0, 0.5, 0.0));
    }

    // The air in front. `finish` is linear in its colour, so two probes
    // give what it adds and what it lets through.
    let f0 = finish(vec3<f32>(0.0), world);
    let f1 = finish(vec3<f32>(1.0), world);
    out.air = vec4<f32>(f0, f1.x - f0.x);

    let soft = select(0.0, radius * 0.8 + 0.05, (flags & FX_SOFT) != 0u);
    out.height = vec3<f32>(corner_world.y, ground, soft);
    out.cell = vec3<f32>(
        f32(p.bits.x % FX_COLUMNS) / FX_CELLS.x,
        f32(p.bits.x / FX_COLUMNS) / FX_CELLS.y,
        select(0.0, 1.0, (flags & FX_ADDITIVE) != 0u),
    );
    return out;
}

@fragment
fn fs_particle(in: FxOut) -> @location(0) vec4<f32> {
    let at = in.cell.xy + (in.uv.xy * 0.5 + 0.5) * (vec2<f32>(1.0) / FX_CELLS);
    let density = textureSample(fx_atlas, fx_sampler, at).r;
    var a = in.color.a * density;
    if in.height.z > 0.0 {
        a *= smoothstep(0.0, in.height.z, in.height.x - in.height.y);
    }
    if a < 0.004 {
        discard;
    }
    if in.cell.z > 0.5 {
        // Light added to the frame, dimmed by the air.
        return vec4<f32>(in.color.rgb * (a * FX_GAIN * in.air.w), 0.0);
    }
    let n = vec3<f32>(in.uv.zw, sqrt(max(1.0 - dot(in.uv.zw, in.uv.zw), 0.0)));
    let ndl = dot(n, in.sun_local.xyz);
    let direct = in.sun * (smoothstep(-0.35, 1.0, ndl) + in.sun_local.w);
    // The sky fills a puff more than it would a flat surface.
    let lit = in.color.rgb * (direct + in.amb * 1.6);
    let c = lit * in.air.w + in.air.rgb;
    return vec4<f32>(c * a, a);
}
