//! Close-range ambient occlusion, per vertex, for the shader to darken the
//! creases between a crown's lobes, the inside of a bush and the foot of a
//! tuft of grass.
//!
//! The models already carry occlusion in their colours from the Blender kit
//! (`bake_ao`, tuned per model). This one is separate and tighter (a few
//! lobes' reach), and the shader applies it on top, so the creases read at
//! the scale the textures work at. It goes in the normal's spare
//! byte, not in the colour, because the game tells bark from leaf by colour and
//! a darkened leaf would read as bark.

use glam::Vec3;

use super::Mesh;

/// Rays cast from each vertex over its normal's hemisphere.
const RAYS: usize = 32;
/// Hits closer than this share of the reach are the surface's own
/// neighbouring facets.
const SELF_HIT: f32 = 0.08;
/// Scales the share of rays that hit: a vertex with about 45% of its rays
/// blocked at the edge of their reach reads fully shut (the result is clamped).
const GAIN: f32 = 2.2;

/// How shut in each of `mesh`'s vertices is, 0 (open) to 1, by geometry within
/// `reach` tiles of it.
pub fn bake(mesh: &Mesh, reach: f32) -> Vec<f32> {
    let tris: Vec<Tri> = mesh
        .indices
        .chunks_exact(3)
        .map(|t| Tri::new([t[0], t[1], t[2]].map(|i| mesh.pos[i as usize])))
        .collect();
    let directions = hemisphere();
    let n = mesh.pos.len();
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let per = n.div_ceil(threads).max(1);
    let mut out = vec![0.0; n];
    std::thread::scope(|s| {
        for (k, chunk) in out.chunks_mut(per).enumerate() {
            let (tris, directions) = (&tris, &directions);
            s.spawn(move || {
                for (j, slot) in chunk.iter_mut().enumerate() {
                    *slot = vertex(mesh, k * per + j, tris, directions, reach);
                }
            });
        }
    });
    out
}

/// A triangle and the box round it, for a quick look at what is near a vertex.
struct Tri {
    corners: [Vec3; 3],
    min: Vec3,
    max: Vec3,
}

impl Tri {
    fn new(corners: [Vec3; 3]) -> Self {
        let [a, b, c] = corners;
        Self {
            corners,
            min: a.min(b).min(c),
            max: a.max(b).max(c),
        }
    }

    /// Whether a point within `reach` of the vertex could be on it.
    fn near(&self, p: Vec3, reach: f32) -> bool {
        p.cmpge(self.min - reach).all() && p.cmple(self.max + reach).all()
    }
}

/// Cosine-weighted directions over the +z hemisphere (Hammersley points).
fn hemisphere() -> Vec<Vec3> {
    (0..RAYS)
        .map(|k| {
            let u = (k as f32 + 0.5) / RAYS as f32;
            let v = (k as u32).reverse_bits() as f32 / 4_294_967_296.0;
            let r = u.sqrt();
            let phi = std::f32::consts::TAU * v;
            Vec3::new(r * phi.cos(), r * phi.sin(), (1.0 - u).sqrt())
        })
        .collect()
}

fn vertex(mesh: &Mesh, i: usize, tris: &[Tri], directions: &[Vec3], reach: f32) -> f32 {
    let n = mesh.normal[i];
    let origin = mesh.pos[i];
    // Only these can be hit within reach, whichever way the ray goes.
    let near: Vec<&Tri> = tris.iter().filter(|t| t.near(origin, reach)).collect();
    // A frame about the normal, turned a little by vertex so the same
    // pattern does not band across neighbours.
    let t = if n.y.abs() < 0.9 { Vec3::Y } else { Vec3::X }
        .cross(n)
        .normalize();
    let b = n.cross(t);
    let turn =
        ((i as u32).wrapping_mul(2_654_435_761) >> 8) as f32 / 16_777_216.0 * std::f32::consts::TAU;
    let (s, c) = turn.sin_cos();
    let (t, b) = (t * c + b * s, b * c - t * s);
    let mut shut = 0.0;
    for d in directions {
        let dir = t * d.x + b * d.y + n * d.z;
        // A ray that hits nothing within reach counts as reaching it.
        let nearest = near
            .iter()
            .filter_map(|tri| hit(origin, dir, &tri.corners))
            .filter(|&h| h > SELF_HIT * reach)
            .fold(reach, f32::min);
        shut += 1.0 - nearest / reach;
    }
    (shut / directions.len() as f32 * GAIN).clamp(0.0, 1.0)
}

/// Möller–Trumbore: where along the ray it meets the triangle, if it does.
fn hit(origin: Vec3, dir: Vec3, tri: &[Vec3; 3]) -> Option<f32> {
    let e1 = tri[1] - tri[0];
    let e2 = tri[2] - tri[0];
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - tri[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat square (two triangles) facing up, and a wall rising from its
    /// middle: the point beside the wall is shut in more than a far corner.
    #[test]
    fn a_vertex_beside_a_wall_is_darker_than_an_open_one() {
        let pos = vec![
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(-1.0, 0.0, 1.0),
            // The wall, in the plane x = 0.1, from z = -1 to 1, 0.6 high.
            Vec3::new(0.1, 0.0, -1.0),
            Vec3::new(0.1, 0.6, -1.0),
            Vec3::new(0.1, 0.6, 1.0),
            Vec3::new(0.1, 0.0, 1.0),
            // A point on the floor just beside the wall.
            Vec3::new(0.0, 0.0, 0.0),
        ];
        let mesh = Mesh {
            normal: vec![Vec3::Y; pos.len()],
            color: vec![[0.5; 3]; pos.len()],
            indices: vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7],
            pos,
        };
        let occ = bake(&mesh, 0.5);
        // Vertex 3 is a far corner (x = -1); vertex 8 is beside the wall.
        assert!(occ[8] > occ[3], "{occ:?}");
        assert_eq!(occ[3], 0.0);
    }
}
