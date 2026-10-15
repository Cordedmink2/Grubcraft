//! Baked soldier models: skinned meshes (each vertex follows up to four
//! bones), and bone animation rows (see
//! `docs/10-architecture.md#units-instancing-and-vertex-animation-textures`).
//!
//! A model pack holds five tables:
//!
//! - `models/index`: one [`ModelRecord`] per model.
//! - `models/vertices`: every model's [`ModelVertex`]es, back to back.
//! - `models/indices`: `u16` triangle indices, relative to the model's first
//!   vertex; each model has a full and a reduced level of detail.
//! - `models/clips`: one [`ClipRecord`] per animation clip.
//! - `models/bones`: the bone animation texture, [`BONE_TEXELS_PER_ROW`]
//!   texels per row, one row per frame of every clip. Each texel is four
//!   half floats; a bone takes three texels (the rows of a 3 × 4 matrix that
//!   moves a vertex from where it is stored to that frame's pose).

use alloc::vec::Vec;
use core::fmt;

use bytemuck::{Pod, Zeroable};

use crate::{Pack, TableError, read_table};

/// Bumped whenever a record's layout or meaning changes. 2: vertices follow
/// up to four weighted bones and carry a texture coordinate and tangent.
pub const MODEL_SCHEMA: u32 = 2;
pub const INDEX_PATH: &str = "models/index";
pub const VERTICES_PATH: &str = "models/vertices";
pub const INDICES_PATH: &str = "models/indices";
pub const CLIPS_PATH: &str = "models/clips";
pub const BONES_PATH: &str = "models/bones";

/// Most bones a model may have (one more is added for parts that never move).
/// Room for a full humanoid rig with fingers (the Quaternius rigs have 62);
/// every bone costs every animation frame three texels, used or not.
pub const MAX_BONES: u32 = 63;
/// Bones a vertex may follow.
pub const BONES_PER_VERTEX: usize = 4;
/// Texels in a bone texture row: three per bone, for `MAX_BONES + 1` bones.
pub const BONE_TEXELS_PER_ROW: u32 = 3 * (MAX_BONES + 1);
/// Vertex positions are stored in 1/`POSITION_SCALE` tiles.
pub const POSITION_SCALE: f32 = 4096.0;
pub const NAME_BYTES: usize = 24;

/// What a clip shows. The game picks clips by kind, never by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ClipKind {
    Idle = 0,
    Walk = 1,
    Run = 2,
    Attack = 3,
    Attack2 = 4,
    Hit = 5,
    Die = 6,
    Cheer = 7,
    Shoot = 8,
}

impl ClipKind {
    pub const ALL: [ClipKind; 9] = [
        ClipKind::Idle,
        ClipKind::Walk,
        ClipKind::Run,
        ClipKind::Attack,
        ClipKind::Attack2,
        ClipKind::Hit,
        ClipKind::Die,
        ClipKind::Cheer,
        ClipKind::Shoot,
    ];

    pub fn from_u8(v: u8) -> Option<ClipKind> {
        ClipKind::ALL.get(v as usize).copied()
    }

    /// Whether the clip repeats (walking) or plays once and holds (dying).
    pub fn loops(self) -> bool {
        !matches!(self, ClipKind::Die)
    }
}

/// One model. 64 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct ModelRecord {
    /// Zero-padded UTF-8.
    pub name: [u8; NAME_BYTES],
    pub first_vertex: u32,
    pub vertex_count: u32,
    /// First index and index count of each level of detail (0 is full).
    pub lods: [[u32; 2]; 2],
    pub first_clip: u32,
    pub clip_count: u32,
    /// Height of the rest pose, in tiles.
    pub height: f32,
    pub bones: u32,
}

impl ModelRecord {
    pub fn name(&self) -> &str {
        let len = self.name.iter().position(|&b| b == 0).unwrap_or(NAME_BYTES);
        core::str::from_utf8(&self.name[..len]).unwrap_or("?")
    }
}

/// One vertex, following up to four bones. 32 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Pod, Zeroable)]
pub struct ModelVertex {
    /// Rest-pose position in 1/[`POSITION_SCALE`] tiles; x across, y up,
    /// z forward (the way the soldier faces).
    pub pos: [i16; 3],
    /// How much of the team colour replaces the vertex colour (0 to 255).
    pub team: u8,
    pub _pad: u8,
    /// The bones this vertex follows, heaviest first.
    pub bones: [u8; BONES_PER_VERTEX],
    /// Each bone's share of the vertex, summing to 255. Unused slots are 0.
    pub weights: [u8; BONES_PER_VERTEX],
    /// Unit normal scaled to ±127; the fourth byte is padding.
    pub normal: [i8; 4],
    /// Unit tangent (along the texture's u) scaled to ±127; the fourth byte
    /// is ±127, the handedness of the bitangent. Copied from the source
    /// model; all zero when it has none (generating them comes with normal
    /// maps).
    pub tangent: [i8; 4],
    /// sRGB colour; alpha is padding.
    pub color: [u8; 4],
    /// Texture coordinate, 0 to 65,535 across the texture (coordinates
    /// outside it, for a repeating texture, are clamped for now).
    pub uv: [u16; 2],
}

/// One animation clip. 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct ClipRecord {
    pub kind: u8,
    pub _pad: [u8; 3],
    /// Bone texture row of the first frame.
    pub first_row: u32,
    pub frames: u32,
    /// Frames per second the clip was sampled at.
    pub fps: f32,
}

impl ClipRecord {
    pub fn kind(&self) -> Option<ClipKind> {
        ClipKind::from_u8(self.kind)
    }

    pub fn seconds(&self) -> f32 {
        self.frames as f32 / self.fps
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelError {
    Missing(&'static str),
    Table(&'static str, TableError),
    /// A model's vertices, indices, clips or bone rows point outside their tables.
    OutOfRange(usize),
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::Missing(p) => write!(f, "model pack has no {p}"),
            ModelError::Table(p, e) => write!(f, "{p}: {e}"),
            ModelError::OutOfRange(m) => write!(f, "model {m} points outside the pack's tables"),
        }
    }
}

impl core::error::Error for ModelError {}

/// The models in a pack, borrowed in place.
#[derive(Clone, Copy, Debug)]
pub struct Models<'a> {
    pub models: &'a [ModelRecord],
    pub vertices: &'a [ModelVertex],
    pub indices: &'a [u16],
    pub clips: &'a [ClipRecord],
    /// Bone texture texels, four half floats each, row by row.
    pub bones: &'a [[u16; 4]],
}

impl<'a> Models<'a> {
    /// Read and check every table. After this, no record points out of range.
    pub fn from_pack(pack: &Pack<'a>) -> Result<Models<'a>, ModelError> {
        fn table<'a, T: Pod>(pack: &Pack<'a>, path: &'static str) -> Result<&'a [T], ModelError> {
            let entry = pack.get_path(path).ok_or(ModelError::Missing(path))?;
            read_table(MODEL_SCHEMA, entry.bytes).map_err(|e| ModelError::Table(path, e))
        }
        let m = Models {
            models: table(pack, INDEX_PATH)?,
            vertices: table(pack, VERTICES_PATH)?,
            indices: table(pack, INDICES_PATH)?,
            clips: table(pack, CLIPS_PATH)?,
            bones: table(pack, BONES_PATH)?,
        };
        let rows = m.bones.len() / BONE_TEXELS_PER_ROW as usize;
        for (k, r) in m.models.iter().enumerate() {
            let bad = ModelError::OutOfRange(k);
            let verts_end = r.first_vertex as usize + r.vertex_count as usize;
            if verts_end > m.vertices.len()
                || r.vertex_count > u16::MAX as u32 + 1
                || r.bones > MAX_BONES + 1
            {
                return Err(bad);
            }
            for &[first, count] in &r.lods {
                let (first, count) = (first as usize, count as usize);
                let Some(tris) = m.indices.get(first..first + count) else {
                    return Err(bad);
                };
                if count % 3 != 0 || tris.iter().any(|&i| i as u32 >= r.vertex_count) {
                    return Err(bad);
                }
            }
            let verts = &m.vertices[r.first_vertex as usize..verts_end];
            // The shader reads the first bone always and stops at the first
            // empty slot, so weights must run heaviest first and sum to 255.
            if verts.iter().any(|v| {
                v.bones.iter().any(|&b| b as u32 >= r.bones)
                    || v.weights.iter().map(|&w| w as u32).sum::<u32>() != 255
                    || v.weights.windows(2).any(|w| w[1] > w[0])
            }) {
                return Err(bad);
            }
            let clips_end = r.first_clip as usize + r.clip_count as usize;
            let Some(clips) = m.clips.get(r.first_clip as usize..clips_end) else {
                return Err(bad);
            };
            for c in clips {
                if c.kind().is_none()
                    || c.frames == 0
                    || c.fps.is_nan()
                    || c.fps <= 0.0
                    || c.first_row as usize + c.frames as usize > rows
                {
                    return Err(bad);
                }
            }
        }
        Ok(m)
    }

    /// Rows in the bone texture.
    pub fn bone_rows(&self) -> u32 {
        self.bones.len() as u32 / BONE_TEXELS_PER_ROW
    }

    pub fn by_name(&self, name: &str) -> Option<usize> {
        self.models.iter().position(|m| m.name() == name)
    }

    /// A model's clip of the given kind, if it has one.
    pub fn clip(&self, model: usize, kind: ClipKind) -> Option<&'a ClipRecord> {
        let r = &self.models[model];
        self.clips[r.first_clip as usize..(r.first_clip + r.clip_count) as usize]
            .iter()
            .find(|c| c.kind == kind as u8)
    }
}

/// Everything a model pack holds, for writing one.
#[derive(Default)]
pub struct ModelTables {
    pub models: Vec<ModelRecord>,
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u16>,
    pub clips: Vec<ClipRecord>,
    pub bones: Vec<[u16; 4]>,
}

impl ModelTables {
    pub fn write(&self, pack: &mut crate::PackWriter) {
        use crate::{Kind, table_bytes};
        pack.add(
            INDEX_PATH,
            Kind::Table,
            &table_bytes(MODEL_SCHEMA, &self.models),
        );
        pack.add(
            VERTICES_PATH,
            Kind::Table,
            &table_bytes(MODEL_SCHEMA, &self.vertices),
        );
        pack.add(
            INDICES_PATH,
            Kind::Table,
            &table_bytes(MODEL_SCHEMA, &self.indices),
        );
        pack.add(
            CLIPS_PATH,
            Kind::Table,
            &table_bytes(MODEL_SCHEMA, &self.clips),
        );
        pack.add(
            BONES_PATH,
            Kind::Table,
            &table_bytes(MODEL_SCHEMA, &self.bones),
        );
    }
}

/// Half-float bits for an `f32` (round to nearest; overflow becomes
/// infinity, tiny values become zero).
pub fn f16_bits(v: f32) -> u16 {
    let x = v.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xff) as i32;
    let mant = x & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if mant != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        // Subnormal half.
        let m = (mant | 0x80_0000) >> (1 - e);
        let rounded = (m + 0x1000) >> 13;
        return sign | rounded as u16;
    }
    let rounded = (((e as u32) << 10) | (mant >> 13)) + ((mant >> 12) & 1);
    sign | rounded as u16
}

/// The `f32` value of half-float bits.
pub fn f16_value(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let mant = (h & 0x3ff) as f32;
    match exp {
        0 => sign * mant * (1.0 / (1u32 << 24) as f32),
        0x1f => {
            if mant == 0.0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => {
            let scale = if exp >= 15 {
                (1u32 << (exp - 15)) as f32
            } else {
                1.0 / (1u32 << (15 - exp)) as f32
            };
            sign * (1.0 + mant / 1024.0) * scale
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackWriter;

    #[test]
    fn halves_round_trip() {
        for v in [0.0f32, 1.0, -1.0, 0.5, 2.75, -0.333, 1e-3, 1000.0, 65504.0] {
            let back = f16_value(f16_bits(v));
            assert!((back - v).abs() <= v.abs() * 1e-3 + 1e-6, "{v} -> {back}");
        }
        assert_eq!(f16_bits(1.0), 0x3c00);
        assert_eq!(f16_bits(-2.0), 0xc000);
        assert!(f16_value(f16_bits(1e9)).is_infinite());
    }

    #[test]
    fn record_sizes_are_stable() {
        assert_eq!(size_of::<ModelRecord>(), 64);
        assert_eq!(size_of::<ModelVertex>(), 32);
        assert_eq!(size_of::<ClipRecord>(), 16);
    }

    fn one_model() -> ModelTables {
        let mut name = [0; NAME_BYTES];
        name[..4].copy_from_slice(b"test");
        let v = ModelVertex {
            normal: [0, 127, 0, 0],
            color: [255; 4],
            weights: [255, 0, 0, 0],
            ..Zeroable::zeroed()
        };
        ModelTables {
            models: alloc::vec![ModelRecord {
                name,
                first_vertex: 0,
                vertex_count: 3,
                lods: [[0, 3], [0, 3]],
                first_clip: 0,
                clip_count: 1,
                height: 1.0,
                bones: 1,
            }],
            vertices: alloc::vec![v; 3],
            indices: alloc::vec![0, 1, 2],
            clips: alloc::vec![ClipRecord {
                kind: ClipKind::Walk as u8,
                _pad: [0; 3],
                first_row: 0,
                frames: 2,
                fps: 10.0,
            }],
            bones: alloc::vec![[0; 4]; 2 * BONE_TEXELS_PER_ROW as usize],
        }
    }

    fn parsed<R>(tables: &ModelTables, f: impl FnOnce(Result<Models<'_>, ModelError>) -> R) -> R {
        let mut w = PackWriter::new();
        tables.write(&mut w);
        let bytes = w.finish();
        let mut store = alloc::vec![0u128; bytes.len().div_ceil(16)];
        bytemuck::cast_slice_mut::<u128, u8>(&mut store)[..bytes.len()].copy_from_slice(&bytes);
        let view = &bytemuck::cast_slice::<u128, u8>(&store)[..bytes.len()];
        f(Models::from_pack(&Pack::parse(view).unwrap()))
    }

    #[test]
    fn a_model_pack_reads_back_and_is_checked() {
        let t = one_model();
        parsed(&t, |m| {
            let m = m.unwrap();
            assert_eq!(m.by_name("test"), Some(0));
            assert_eq!(m.bone_rows(), 2);
            assert_eq!(m.clip(0, ClipKind::Walk).unwrap().frames, 2);
            assert!(m.clip(0, ClipKind::Die).is_none());
        });
        let mut bad = one_model();
        bad.indices[2] = 3;
        parsed(&bad, |m| {
            assert_eq!(m.err(), Some(ModelError::OutOfRange(0)))
        });
        let mut bad = one_model();
        bad.clips[0].frames = 3;
        parsed(&bad, |m| {
            assert_eq!(m.err(), Some(ModelError::OutOfRange(0)))
        });
        let mut bad = one_model();
        bad.vertices[1].bones = [0, 1, 0, 0];
        bad.vertices[1].weights = [200, 55, 0, 0];
        parsed(&bad, |m| {
            assert_eq!(m.err(), Some(ModelError::OutOfRange(0)))
        });
        let mut bad = one_model();
        bad.vertices[1].bones = [0, 9, 0, 0];
        parsed(&bad, |m| {
            assert_eq!(m.err(), Some(ModelError::OutOfRange(0)))
        });
        for weights in [[0, 0, 0, 0], [200, 0, 55, 0], [55, 200, 0, 0]] {
            let mut bad = one_model();
            bad.vertices[1].weights = weights;
            parsed(&bad, |m| {
                assert_eq!(m.err(), Some(ModelError::OutOfRange(0)))
            });
        }
    }
}
