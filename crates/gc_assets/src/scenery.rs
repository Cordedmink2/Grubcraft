//! Baked scenery models: trees, bushes, grass and rocks, painted in vertex
//! colours and drawn instanced by the thousand (see
//! `docs/10-architecture.md#scenery`).
//!
//! A scenery pack holds three tables:
//!
//! - `scenery/index`: one [`SceneryModelRecord`] per model.
//! - `scenery/vertices`: every model's [`SceneryVertex`]es, back to back.
//! - `scenery/indices`: `u16` triangle indices, relative to the model's first
//!   vertex; each model has [`LODS`] levels of detail, from full to fewest
//!   triangles, all sharing the model's vertices.
//!
//! Models are baked facing +Z (glTF's front), y up, base at the origin. The
//! wind weight of each vertex lives in the colour's alpha byte: it already
//! holds the model's sway, so the shader needs nothing per model (see
//! [`SceneryVertex::color`]).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use bytemuck::{Pod, Zeroable};

use crate::{Pack, read_table};

/// Bumped whenever a record's layout or meaning changes.
pub const SCENERY_SCHEMA: u32 = 1;
pub const INDEX_PATH: &str = "scenery/index";
pub const VERTICES_PATH: &str = "scenery/vertices";
pub const INDICES_PATH: &str = "scenery/indices";

/// Levels of detail per model: full, then fewer triangles at each step.
pub const LODS: usize = 4;
pub const NAME_BYTES: usize = 24;
/// Vertex positions are stored in 1/`POSITION_SCALE` tiles, so a model
/// reaches at most `32768 / POSITION_SCALE` = 16 tiles from its origin.
pub const POSITION_SCALE: f32 = 2048.0;
/// The most a vertex moves in the wind, in tiles, when its alpha is 255 (at
/// scale 1). A model's own sway, times its vertex's weight, is stored as a
/// share of this.
pub const SWAY_MAX: f32 = 0.25;

/// What a model is, for the placement code (and the baker's budgets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Family {
    Tree = 0,
    Bush = 1,
    Grass = 2,
    Rock = 3,
}

impl Family {
    /// The family called `name` in the baker's config ("tree", "bush",
    /// "grass" or "rock").
    pub fn from_name(name: &str) -> Option<Family> {
        match name {
            "tree" => Some(Family::Tree),
            "bush" => Some(Family::Bush),
            "grass" => Some(Family::Grass),
            "rock" => Some(Family::Rock),
            _ => None,
        }
    }

    pub fn from_u8(v: u8) -> Option<Family> {
        match v {
            0 => Some(Family::Tree),
            1 => Some(Family::Bush),
            2 => Some(Family::Grass),
            3 => Some(Family::Rock),
            _ => None,
        }
    }
}

/// One model. 80 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct SceneryModelRecord {
    /// Zero-padded UTF-8.
    pub name: [u8; NAME_BYTES],
    pub first_vertex: u32,
    pub vertex_count: u32,
    /// First index and index count of each level of detail (0 is full).
    pub lods: [[u32; 2]; LODS],
    /// Height of the model at full detail, in tiles.
    pub height: f32,
    /// Farthest any vertex of full detail lies from the vertical axis, in
    /// tiles (for culling and spacing).
    pub radius: f32,
    /// How far the top moves in a gust, in tiles (already in the vertices).
    pub sway: f32,
    /// A [`Family`].
    pub family: u8,
    pub _pad: [u8; 3],
}

impl SceneryModelRecord {
    pub fn name(&self) -> &str {
        let len = self.name.iter().position(|&b| b == 0).unwrap_or(NAME_BYTES);
        core::str::from_utf8(&self.name[..len]).unwrap_or("?")
    }

    pub fn family(&self) -> Option<Family> {
        Family::from_u8(self.family)
    }

    /// Triangles at a level of detail.
    pub fn triangles(&self, lod: usize) -> u32 {
        self.lods[lod][1] / 3
    }
}

/// One vertex. 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Pod, Zeroable)]
pub struct SceneryVertex {
    /// Position in 1/[`POSITION_SCALE`] tiles; x across, y up, z forward.
    pub pos: [i16; 3],
    /// Still spare.
    pub _pad: i16,
    /// Unit normal scaled to ±127; the fourth byte is close-range ambient
    /// occlusion, 0 (open) to 127 (shut), which the shader darkens by.
    pub normal: [i8; 4],
    /// sRGB colour with ambient occlusion painted in. Alpha is the wind: how
    /// far this vertex moves in a full gust, in 1/255 of [`SWAY_MAX`] tiles.
    pub color: [u8; 4],
}

/// The scenery in a pack, borrowed in place.
#[derive(Clone, Copy, Debug)]
pub struct Scenery<'a> {
    pub models: &'a [SceneryModelRecord],
    pub vertices: &'a [SceneryVertex],
    pub indices: &'a [u16],
}

impl<'a> Scenery<'a> {
    /// Read and check every table. After this, no record points out of range.
    pub fn from_pack(pack: &Pack<'a>) -> Result<Scenery<'a>, String> {
        fn table<'a, T: Pod>(pack: &Pack<'a>, path: &'static str) -> Result<&'a [T], String> {
            let entry = pack
                .get_path(path)
                .ok_or_else(|| format!("scenery pack has no {path}"))?;
            read_table(SCENERY_SCHEMA, entry.bytes).map_err(|e| format!("{path}: {e}"))
        }
        let s = Scenery {
            models: table(pack, INDEX_PATH)?,
            vertices: table(pack, VERTICES_PATH)?,
            indices: table(pack, INDICES_PATH)?,
        };
        for r in s.models {
            let bad = |why: &str| format!("scenery model {:?} {why}", r.name());
            let end = r.first_vertex as usize + r.vertex_count as usize;
            if end > s.vertices.len() || r.vertex_count > u32::from(u16::MAX) + 1 {
                return Err(bad("has vertices outside the pack"));
            }
            if r.family().is_none() {
                return Err(bad("has an unknown family"));
            }
            if r.height.is_nan() || r.radius.is_nan() || r.sway.is_nan() || r.sway < 0.0 {
                return Err(bad("has a bad size"));
            }
            for &[first, count] in &r.lods {
                let (first, count) = (first as usize, count as usize);
                let Some(tris) = s.indices.get(first..first.saturating_add(count)) else {
                    return Err(bad("has indices outside the pack"));
                };
                if count == 0 || count % 3 != 0 {
                    return Err(bad("has a level without whole triangles"));
                }
                if tris.iter().any(|&i| u32::from(i) >= r.vertex_count) {
                    return Err(bad("has an index past its vertices"));
                }
            }
        }
        Ok(s)
    }

    pub fn by_name(&self, name: &str) -> Option<usize> {
        self.models.iter().position(|m| m.name() == name)
    }
}

/// Everything a scenery pack holds, for writing one.
#[derive(Default)]
pub struct SceneryTables {
    pub models: Vec<SceneryModelRecord>,
    pub vertices: Vec<SceneryVertex>,
    pub indices: Vec<u16>,
}

impl SceneryTables {
    pub fn write(&self, pack: &mut crate::PackWriter) {
        use crate::{Kind, table_bytes};
        let s = SCENERY_SCHEMA;
        pack.add(INDEX_PATH, Kind::Table, &table_bytes(s, &self.models));
        pack.add(VERTICES_PATH, Kind::Table, &table_bytes(s, &self.vertices));
        pack.add(INDICES_PATH, Kind::Table, &table_bytes(s, &self.indices));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackWriter;

    fn one_model() -> SceneryTables {
        let mut name = [0; NAME_BYTES];
        name[..4].copy_from_slice(b"test");
        let v = SceneryVertex {
            normal: [0, 127, 0, 0],
            color: [200, 100, 50, 255],
            ..Zeroable::zeroed()
        };
        SceneryTables {
            models: alloc::vec![SceneryModelRecord {
                name,
                first_vertex: 0,
                vertex_count: 3,
                lods: [[0, 3], [3, 3], [6, 3], [9, 3]],
                height: 1.0,
                radius: 0.5,
                sway: 0.1,
                family: Family::Bush as u8,
                _pad: [0; 3],
            }],
            vertices: alloc::vec![v; 3],
            indices: alloc::vec![0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2],
        }
    }

    fn parsed<R>(t: &SceneryTables, f: impl FnOnce(Result<Scenery<'_>, String>) -> R) -> R {
        let mut w = PackWriter::new();
        t.write(&mut w);
        parsed_bytes(&w.finish(), f)
    }

    fn parsed_bytes<R>(bytes: &[u8], f: impl FnOnce(Result<Scenery<'_>, String>) -> R) -> R {
        let store = crate::aligned_copy(bytes);
        let view = &bytemuck::cast_slice::<u128, u8>(&store)[..bytes.len()];
        f(Scenery::from_pack(&Pack::parse(view).unwrap()))
    }

    #[test]
    fn record_sizes_are_stable() {
        assert_eq!(size_of::<SceneryModelRecord>(), 80);
        assert_eq!(size_of::<SceneryVertex>(), 16);
    }

    #[test]
    fn a_scenery_pack_reads_back() {
        parsed(&one_model(), |s| {
            let s = s.unwrap();
            assert_eq!(s.by_name("test"), Some(0));
            assert_eq!(s.models[0].family(), Some(Family::Bush));
            assert_eq!(s.models[0].triangles(3), 1);
            assert_eq!(s.vertices.len(), 3);
            assert_eq!(s.vertices[0].color, [200, 100, 50, 255]);
            assert_eq!(s.indices.len(), 12);
        });
    }

    #[test]
    fn bad_records_are_refused() {
        let cases: [fn(&mut SceneryTables); 6] = [
            |t| t.indices[2] = 3,
            |t| t.models[0].lods[3] = [0, 0],
            |t| t.models[0].lods[3] = [9, 4],
            |t| t.models[0].lods[3] = [10, 3],
            |t| t.models[0].vertex_count = 4,
            |t| t.models[0].family = 9,
        ];
        for (k, f) in cases.iter().enumerate() {
            let mut t = one_model();
            f(&mut t);
            parsed(&t, |s| assert!(s.is_err(), "case {k} was accepted"));
        }
    }

    #[test]
    fn a_pack_of_the_wrong_schema_is_refused() {
        let t = one_model();
        let mut w = PackWriter::new();
        // The index table written under another schema; the rest as usual.
        w.add(
            INDEX_PATH,
            crate::Kind::Table,
            &crate::table_bytes(SCENERY_SCHEMA + 1, &t.models),
        );
        w.add(
            VERTICES_PATH,
            crate::Kind::Table,
            &crate::table_bytes(SCENERY_SCHEMA, &t.vertices),
        );
        w.add(
            INDICES_PATH,
            crate::Kind::Table,
            &crate::table_bytes(SCENERY_SCHEMA, &t.indices),
        );
        parsed_bytes(&w.finish(), |s| assert!(s.is_err()));
    }
}
