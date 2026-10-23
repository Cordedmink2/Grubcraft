//! Baked soldier models: skinned meshes (each vertex follows up to four
//! bones), and bone animation rows (see
//! `docs/10-architecture.md#units-instancing-and-vertex-animation-textures`).
//!
//! A model pack holds six tables:
//!
//! - `models/index`: one [`ModelRecord`] per model.
//! - `models/vertices`: every model's [`ModelVertex`]es, back to back.
//! - `models/indices`: `u16` triangle indices, relative to the model's first
//!   vertex; each model has [`LODS`] levels of detail, from full to fewest
//!   triangles.
//! - `models/clips`: one [`ClipRecord`] per animation clip.
//! - `models/bones`: the bone animation texture, [`BONE_TEXELS_PER_ROW`]
//!   texels per row, one row per frame of every clip. Each texel is four
//!   half floats; a bone takes three texels (the rows of a 3 × 4 matrix that
//!   moves a vertex from where it is stored to that frame's pose).
//! - `models/textures`: [`LAYERS_PER_MATERIAL`] texture layers per material,
//!   each [`TEXTURE_SIZE`] square, BC7-compressed (16 bytes per 4 × 4 block
//!   of RGBA texels) with its mips down to one block (see [`MaterialLayer`]).
//!   Models painted in vertex colours have none.

use alloc::vec::Vec;
use core::fmt;

use bytemuck::{Pod, Zeroable};

use crate::{Pack, TableError, read_table};

/// Bumped whenever a record's layout or meaning changes. 2: vertices follow
/// up to four weighted bones and carry a texture coordinate and tangent.
/// 3: textured materials. 4: four levels of detail, BC7 textures.
pub const MODEL_SCHEMA: u32 = 4;
pub const INDEX_PATH: &str = "models/index";
pub const VERTICES_PATH: &str = "models/vertices";
pub const INDICES_PATH: &str = "models/indices";
pub const CLIPS_PATH: &str = "models/clips";
pub const TEXTURES_PATH: &str = "models/textures";
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

/// Levels of detail per model: full, then fewer triangles at each step.
pub const LODS: usize = 4;

/// Side of a material texture layer, in texels.
pub const TEXTURE_SIZE: u32 = 512;
/// Texels on a side of a compressed block.
pub const BLOCK_SIDE: u32 = 4;
/// One BC7 block: 4 × 4 RGBA texels in 16 bytes.
pub type Block = [u8; 16];
/// Mip levels per layer, down to a single block (4 × 4).
pub const TEXTURE_MIPS: u32 = TEXTURE_SIZE.ilog2() - BLOCK_SIDE.ilog2() + 1;
/// Blocks in one layer with all its mips (a geometric series, the side
/// being a power of two).
pub const BLOCKS_PER_LAYER: usize = (4 * ((TEXTURE_SIZE / BLOCK_SIDE) as usize).pow(2) - 1) / 3;
pub const LAYERS_PER_MATERIAL: usize = 3;
pub const BLOCKS_PER_MATERIAL: usize = BLOCKS_PER_LAYER * LAYERS_PER_MATERIAL;
/// Most materials a pack may hold: a vertex names its material plus one in
/// a byte. Three layers each, that is 765 texture array layers: every
/// desktop GPU we target allows 2,048, and the renderer falls back to vertex
/// colours on one that allows fewer.
pub const MAX_MATERIALS: usize = u8::MAX as usize;

/// The layers of one material, in order. Normals use the OpenGL convention
/// (green points along +v's bitangent), as glTF does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialLayer {
    /// sRGB base colour; alpha is how much the team colour replaces it.
    AlbedoTeam = 0,
    /// Linear: tangent-space normal x and y (0 to 1 for -1 to 1), ambient
    /// occlusion, roughness.
    NormalAoRough = 1,
    /// sRGB emitted light; alpha (linear) is metalness.
    EmissiveMetal = 2,
}

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
    /// Guarding against a blow (a shield raised, or a duck).
    Block = 9,
}

impl ClipKind {
    pub const ALL: [ClipKind; 10] = [
        ClipKind::Idle,
        ClipKind::Walk,
        ClipKind::Run,
        ClipKind::Attack,
        ClipKind::Attack2,
        ClipKind::Hit,
        ClipKind::Die,
        ClipKind::Cheer,
        ClipKind::Shoot,
        ClipKind::Block,
    ];

    pub fn from_u8(v: u8) -> Option<ClipKind> {
        ClipKind::ALL.get(v as usize).copied()
    }

    /// Whether the clip repeats (walking) or plays once and holds (dying).
    pub fn loops(self) -> bool {
        !matches!(self, ClipKind::Die)
    }
}

/// One model. 80 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct ModelRecord {
    /// Zero-padded UTF-8.
    pub name: [u8; NAME_BYTES],
    pub first_vertex: u32,
    pub vertex_count: u32,
    /// First index and index count of each level of detail (0 is full).
    pub lods: [[u32; 2]; LODS],
    pub first_clip: u32,
    pub clip_count: u32,
    /// Height of the rest pose, in tiles.
    pub height: f32,
    pub bones: u32,
}

impl ModelRecord {
    /// A model with one bone and no animation: a building, not a soldier.
    pub fn is_still(&self) -> bool {
        self.bones == 1
    }

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
    /// The material this vertex is painted with, plus one; 0 means its
    /// colour is all in `color`.
    pub material: u8,
    /// The bones this vertex follows, heaviest first.
    pub bones: [u8; BONES_PER_VERTEX],
    /// Each bone's share of the vertex, summing to 255. Unused slots are 0.
    pub weights: [u8; BONES_PER_VERTEX],
    /// Unit normal scaled to ±127; the fourth byte holds the two detail
    /// classes the surface wears, four bits each (see
    /// [`DetailTag`](crate::detail::DetailTag)). Zero means none.
    pub normal: [i8; 4],
    /// Unit tangent (along the texture's u) scaled to ±127; the fourth byte
    /// is ±127, the handedness of the bitangent. Copied from the source
    /// model; all zero when it has none (generating them comes with normal
    /// maps).
    pub tangent: [i8; 4],
    /// sRGB colour; alpha holds the detail mix (low four bits) and how
    /// finely it repeats (high four), see [`DetailTag`](crate::detail::DetailTag).
    /// Packs baked before detail existed hold 255, which has no classes.
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
    /// A model's vertices, indices, clips, bone rows or materials point
    /// outside their tables.
    OutOfRange(usize),
    /// The texture table is not a whole number of materials, or too many.
    BadTextures,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::Missing(p) => write!(f, "model pack has no {p}"),
            ModelError::Table(p, e) => write!(f, "{p}: {e}"),
            ModelError::OutOfRange(m) => write!(f, "model {m} points outside the pack's tables"),
            ModelError::BadTextures => write!(f, "model pack textures are not whole materials"),
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
    /// Material texture blocks: [`BLOCKS_PER_LAYER`] per layer, mip by mip,
    /// each mip row by row.
    pub textures: &'a [Block],
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
            textures: table(pack, TEXTURES_PATH)?,
        };
        let rows = m.bones.len() / BONE_TEXELS_PER_ROW as usize;
        if !m.textures.len().is_multiple_of(BLOCKS_PER_MATERIAL) || m.materials() > MAX_MATERIALS {
            return Err(ModelError::BadTextures);
        }
        for (k, r) in m.models.iter().enumerate() {
            let bad = ModelError::OutOfRange(k);
            let verts_end = r.first_vertex as usize + r.vertex_count as usize;
            if verts_end > m.vertices.len()
                || r.vertex_count > u32::from(u16::MAX) + 1
                || r.bones > MAX_BONES + 1
            {
                return Err(bad);
            }
            // Every level has triangles, no more than the level before.
            if r.lods.iter().any(|l| l[1] == 0) || r.lods.windows(2).any(|w| w[1][1] > w[0][1]) {
                return Err(bad);
            }
            for &[first, count] in &r.lods {
                let (first, count) = (first as usize, count as usize);
                let Some(tris) = m.indices.get(first..first + count) else {
                    return Err(bad);
                };
                if count % 3 != 0 || tris.iter().any(|&i| u32::from(i) >= r.vertex_count) {
                    return Err(bad);
                }
            }
            let verts = &m.vertices[r.first_vertex as usize..verts_end];
            // The shader reads the first bone always and stops at the first
            // empty slot, so weights must run heaviest first and sum to 255.
            // A model is drawn with one material.
            if verts.iter().any(|v| {
                v.bones.iter().any(|&b| u32::from(b) >= r.bones)
                    || v.weights.iter().map(|&w| u32::from(w)).sum::<u32>() != 255
                    || v.weights.windows(2).any(|w| w[1] > w[0])
                    || v.material as usize > m.materials()
                    || v.material != verts[0].material
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

    /// Materials in the texture table.
    pub fn materials(&self) -> usize {
        self.textures.len() / BLOCKS_PER_MATERIAL
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
    pub textures: Vec<Block>,
}

impl ModelTables {
    /// Add every model of `m` after the ones already here, with its clips,
    /// bone rows and materials (used to join the soldier pack and the
    /// building packs into the one the game embeds).
    pub fn append(&mut self, m: &Models<'_>) -> Result<(), ModelError> {
        if self.materials() + m.materials() > MAX_MATERIALS {
            return Err(ModelError::BadTextures);
        }
        let (vertices, indices, clips) = (
            self.vertices.len() as u32,
            self.indices.len() as u32,
            self.clips.len() as u32,
        );
        let (rows, materials) = (
            (self.bones.len() / BONE_TEXELS_PER_ROW as usize) as u32,
            self.materials() as u8,
        );
        self.models.extend(m.models.iter().map(|r| {
            let mut r = *r;
            r.first_vertex += vertices;
            r.first_clip += clips;
            for lod in &mut r.lods {
                lod[0] += indices;
            }
            r
        }));
        self.vertices.extend(m.vertices.iter().map(|v| {
            let mut v = *v;
            if v.material > 0 {
                v.material += materials;
            }
            v
        }));
        self.indices.extend_from_slice(m.indices);
        self.clips.extend(m.clips.iter().map(|c| {
            let mut c = *c;
            c.first_row += rows;
            c
        }));
        self.bones.extend_from_slice(m.bones);
        self.textures.extend_from_slice(m.textures);
        Ok(())
    }

    /// Materials in the texture table.
    pub fn materials(&self) -> usize {
        self.textures.len() / BLOCKS_PER_MATERIAL
    }

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
        pack.add(
            TEXTURES_PATH,
            Kind::Table,
            &table_bytes(MODEL_SCHEMA, &self.textures),
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
    let exp = i32::from((h >> 10) & 0x1f);
    let mant = f32::from(h & 0x3ff);
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
    fn a_layer_holds_every_mip() {
        let sum: usize = (0..TEXTURE_MIPS)
            .map(|i| ((TEXTURE_SIZE >> i) / BLOCK_SIDE).pow(2) as usize)
            .sum();
        assert_eq!(BLOCKS_PER_LAYER, sum);
        assert_eq!(TEXTURE_SIZE >> (TEXTURE_MIPS - 1), BLOCK_SIDE);
    }

    #[test]
    fn record_sizes_are_stable() {
        assert_eq!(size_of::<ModelRecord>(), 80);
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
                lods: [[0, 3]; LODS],
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
            textures: Vec::new(),
        }
    }

    #[test]
    fn packs_join_with_their_tables_moved_up() {
        let mut textured = one_model();
        for v in &mut textured.vertices {
            v.material = 1;
        }
        textured.textures = alloc::vec![[7; 16]; BLOCKS_PER_MATERIAL];
        let mut joined = ModelTables::default();
        for part in [&textured, &one_model(), &textured] {
            parsed(part, |m| joined.append(&m.unwrap()).unwrap());
        }
        parsed(&joined, |m| {
            let m = m.unwrap();
            assert_eq!((m.models.len(), m.materials(), m.bone_rows()), (3, 2, 6));
            let last = m.models[2];
            assert_eq!((last.first_vertex, last.first_clip), (6, 2));
            assert_eq!(last.lods[0], [6, 3]);
            assert_eq!(m.clips[2].first_row, 4);
            let material = |k: usize| m.vertices[m.models[k].first_vertex as usize].material;
            assert_eq!([material(0), material(1), material(2)], [1, 0, 2]);
        });
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
            assert_eq!(m.err(), Some(ModelError::OutOfRange(0)));
        });
        let mut bad = one_model();
        bad.models[0].lods[3] = [0, 0];
        parsed(&bad, |m| {
            assert_eq!(m.err(), Some(ModelError::OutOfRange(0)));
        });
        let mut bad = one_model();
        bad.indices.extend_from_slice(&[0, 2, 1]);
        bad.models[0].lods[2] = [0, 6];
        parsed(&bad, |m| {
            assert_eq!(m.err(), Some(ModelError::OutOfRange(0)));
        });
        let mut bad = one_model();
        bad.clips[0].frames = 3;
        parsed(&bad, |m| {
            assert_eq!(m.err(), Some(ModelError::OutOfRange(0)));
        });
        let mut bad = one_model();
        bad.vertices[1].bones = [0, 1, 0, 0];
        bad.vertices[1].weights = [200, 55, 0, 0];
        parsed(&bad, |m| {
            assert_eq!(m.err(), Some(ModelError::OutOfRange(0)));
        });
        let mut bad = one_model();
        bad.vertices[1].bones = [0, 9, 0, 0];
        parsed(&bad, |m| {
            assert_eq!(m.err(), Some(ModelError::OutOfRange(0)));
        });
        let mut bad = one_model();
        bad.vertices[1].material = 1;
        parsed(&bad, |m| {
            assert_eq!(m.err(), Some(ModelError::OutOfRange(0)));
        });
        let mut mixed = one_model();
        mixed.vertices[1].material = 1;
        mixed.textures = alloc::vec![[0; 16]; BLOCKS_PER_MATERIAL];
        parsed(&mixed, |m| {
            assert_eq!(m.err(), Some(ModelError::OutOfRange(0)));
        });
        let mut fine = mixed;
        for v in &mut fine.vertices {
            v.material = 1;
        }
        parsed(&fine, |m| assert_eq!(m.unwrap().materials(), 1));
        let mut bad = one_model();
        bad.textures = alloc::vec![[0; 16]; BLOCKS_PER_LAYER];
        parsed(&bad, |m| assert_eq!(m.err(), Some(ModelError::BadTextures)));
        for weights in [[0, 0, 0, 0], [200, 0, 55, 0], [55, 200, 0, 0]] {
            let mut bad = one_model();
            bad.vertices[1].weights = weights;
            parsed(&bad, |m| {
                assert_eq!(m.err(), Some(ModelError::OutOfRange(0)));
            });
        }
    }
}
