//! Grubcraft scenery baker: turns glTF trees, bushes, grass and rocks into
//! the scenery pack (`assets/scenery.pak`) the game embeds.
//!
//! Each model is listed in `assets-src/scenery.ron`: its name, glTF file,
//! family and sway (tiles the top moves in a gust). The glb holds four mesh
//! objects, `<name>_LOD0` to `<name>_LOD3` (see
//! `docs/25-scenery-models.md`). For each model the baker:
//!
//! 1. Reads each level's positions, normals and `COLOR_0` (glTF colours are
//!    linear; they are stored as sRGB bytes).
//! 2. Checks the triangle budgets of its family and that everything lies
//!    within reach of the pack's 16-bit positions.
//! 3. Sets each vertex's wind weight, `clamp(y / height, 0, 1)^1.5`, scaled
//!    by the model's sway, into the colour's alpha byte (`SWAY_MAX` tiles
//!    when 255), so the shader needs nothing per model.
//!
//! Next to the pack it writes `scenery.manifest` (see `manifest.rs`);
//! `--check` verifies the committed pair without baking.
//!
//! Usage: gc_scenebake [--config assets-src/scenery.ron] [--shared DIR]
//!                     [--out assets/scenery.pak] [--check | --allow-missing]
//! `--allow-missing` skips models whose glb does not exist yet (for
//! development; the manifest then lacks them and `--check` fails).

mod manifest;
mod occlusion;

use std::path::{Path, PathBuf};

use gc_assets::PackWriter;
use gc_assets::scenery::{
    Family, LODS, NAME_BYTES, POSITION_SCALE, SWAY_MAX, Scenery, SceneryModelRecord, SceneryTables,
    SceneryVertex,
};
use glam::{Mat3, Mat4, Vec3};
use serde::Deserialize;

/// Models lie within this many tiles of their origin.
const REACH: f32 = 8.0;
/// The highest a vertex can be stored (positions are `i16` steps of
/// 1/`POSITION_SCALE` tile); x and z are held to `REACH`, well inside.
const TOP: f32 = i16::MAX as f32 / POSITION_SCALE;
/// How far, in tiles, the close-range occlusion looks, by family: about the
/// size of a lobe of a crown, a bush's leaf clump, a tuft or a rock's facet.
fn occlusion_reach(family: Family) -> f32 {
    match family {
        Family::Tree => 0.8,
        Family::Bush => 0.5,
        Family::Grass => 0.15,
        Family::Rock => 0.5,
    }
}
/// Parts may sink this far below the ground.
const SINK: f32 = 0.35;
/// Crown width limits, with a little slack (warnings only).
const TREE_WIDTH: f32 = 2.8;
const BUSH_WIDTH: f32 = 1.4;

#[derive(Deserialize)]
struct Config {
    models: Vec<ModelConfig>,
}

/// A family by its lowercase name in the config.
fn family_by_name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Family, D::Error> {
    let name = String::deserialize(d)?;
    Family::from_name(&name).ok_or_else(|| {
        serde::de::Error::custom(format!(
            "unknown family {name:?} (tree, bush, grass or rock)"
        ))
    })
}

#[derive(Deserialize)]
struct ModelConfig {
    /// Also the glb's object name prefix: `<name>_LOD0`.
    name: String,
    /// `shared:` + a path under the shared assets folder, or `repo:` + a
    /// path under the repository's `assets-src/`.
    source: String,
    #[serde(deserialize_with = "family_by_name")]
    family: Family,
    /// Tiles the top of the model moves in a gust, at scale 1.
    sway: f32,
}

struct Options {
    config: PathBuf,
    shared: PathBuf,
    repo_src: PathBuf,
    out: PathBuf,
    check: bool,
    allow_missing: bool,
}

impl Options {
    /// The file a `shared:` or `repo:` source names.
    fn resolve(&self, src: &str) -> Result<PathBuf, String> {
        if let Some(p) = src.strip_prefix("shared:") {
            Ok(self.shared.join(p))
        } else if let Some(p) = src.strip_prefix("repo:") {
            Ok(self.repo_src.join(p))
        } else {
            Err(format!("{src}: must start with shared: or repo:"))
        }
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut opts = Options {
        config: root.join("assets-src/scenery.ron"),
        shared: PathBuf::from("/mnt/project-files/assets-src"),
        repo_src: root.join("assets-src"),
        out: root.join("assets/scenery.pak"),
        check: false,
        allow_missing: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--check" => {
                opts.check = true;
                continue;
            }
            "--allow-missing" => {
                opts.allow_missing = true;
                continue;
            }
            _ => {}
        }
        let Some(value) = args.next() else { usage() };
        match flag.as_str() {
            "--config" => opts.config = value.into(),
            "--shared" => opts.shared = value.into(),
            "--out" => opts.out = value.into(),
            _ => usage(),
        }
    }
    let result = if opts.check {
        manifest::check(&opts)
    } else {
        run(&opts)
    };
    if let Err(e) = result {
        eprintln!("gc_scenebake: {e}");
        std::process::exit(1);
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: gc_scenebake [--config FILE] [--shared DIR] [--out FILE] [--check | --allow-missing]"
    );
    std::process::exit(2);
}

fn run(opts: &Options) -> Result<(), String> {
    let text = std::fs::read_to_string(&opts.config)
        .map_err(|e| format!("{}: {e}", opts.config.display()))?;
    let config: Config =
        ron::from_str(&text).map_err(|e| format!("{}: {e}", opts.config.display()))?;
    let (bytes, tables, sources) = bake_pack(&config, opts)?;
    if let Some(dir) = opts.out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&opts.out, &bytes).map_err(|e| format!("{}: {e}", opts.out.display()))?;
    manifest::write(opts, &text, &sources, &bytes)?;
    println!(
        "wrote {} ({} KB): {} models, {} vertices, {} triangles at full detail",
        opts.out.display(),
        bytes.len() / 1024,
        tables.models.len(),
        tables.vertices.len(),
        tables.models.iter().map(|m| m.lods[0][1] / 3).sum::<u32>(),
    );
    Ok(())
}

/// Bake every model in `config`: the pack bytes, the tables and the sources
/// that were used.
fn bake_pack<'c>(
    config: &'c Config,
    opts: &Options,
) -> Result<(Vec<u8>, SceneryTables, Vec<&'c str>), String> {
    let mut tables = SceneryTables::default();
    let mut used = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for cfg in &config.models {
        if !seen.insert(cfg.name.as_str()) {
            return Err(format!("{} is listed twice", cfg.name));
        }
        let path = opts.resolve(&cfg.source)?;
        if !path.exists() {
            if opts.allow_missing {
                eprintln!("skipping {}: {} does not exist", cfg.name, path.display());
                continue;
            }
            return Err(format!(
                "{}: {} does not exist (the artists' glb is not there yet; `--allow-missing` skips it while developing)",
                cfg.name,
                path.display()
            ));
        }
        let levels = load_levels(&path, &cfg.name).map_err(|e| format!("{}: {e}", cfg.name))?;
        let baked = bake_model(cfg, &levels).map_err(|e| format!("{}: {e}", cfg.name))?;
        baked.append_to(&mut tables);
        used.push(cfg.source.as_str());
    }
    let mut w = PackWriter::new();
    tables.write(&mut w);
    let bytes = w.finish();
    read_back(&bytes)?;
    Ok((bytes, tables, used))
}

/// Parse `pack` the way the game does.
fn read_back(pack: &[u8]) -> Result<(), String> {
    let store = gc_assets::aligned_copy(pack);
    let view = &bytemuck::cast_slice::<u128, u8>(&store)[..pack.len()];
    let pack = gc_assets::Pack::parse(view).map_err(|e| e.to_string())?;
    Scenery::from_pack(&pack).map(|_| ())
}

/// One level of detail as read: positions in tiles, unit normals, linear
/// colours, triangle indices.
#[derive(Default)]
struct Mesh {
    pos: Vec<Vec3>,
    normal: Vec<Vec3>,
    color: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

fn load_levels(path: &Path, name: &str) -> Result<[Mesh; LODS], String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    levels_from_glb(&bytes, path.parent().unwrap_or(Path::new(".")), name)
}

fn levels_from_glb(bytes: &[u8], dir: &Path, name: &str) -> Result<[Mesh; LODS], String> {
    let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| e.to_string())?;
    let mut buffers = Vec::new();
    for b in gltf.document.buffers() {
        buffers.push(match b.source() {
            gltf::buffer::Source::Bin => gltf.blob.clone().ok_or("missing GLB binary chunk")?,
            gltf::buffer::Source::Uri(uri) => {
                let path = dir.join(uri.replace("%20", " "));
                std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?
            }
        });
    }
    let mut levels: [Option<Mesh>; LODS] = Default::default();
    let mut stack: Vec<(gltf::Node<'_>, Mat4)> = gltf
        .document
        .scenes()
        .flat_map(|s| s.nodes())
        .map(|n| (n, Mat4::IDENTITY))
        .collect();
    while let Some((node, parent)) = stack.pop() {
        let global = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        stack.extend(node.children().map(|c| (c, global)));
        let Some(mesh) = node.mesh() else { continue };
        let label = node.name().or(mesh.name()).unwrap_or_default();
        let Some(level) = (0..LODS).find(|k| label == format!("{name}_LOD{k}")) else {
            continue;
        };
        if levels[level].is_some() {
            return Err(format!("{label} appears twice"));
        }
        levels[level] =
            Some(read_mesh(&mesh, &buffers, global).map_err(|e| format!("{label}: {e}"))?);
    }
    let mut out: [Mesh; LODS] = Default::default();
    for (k, level) in levels.into_iter().enumerate() {
        out[k] = level.ok_or_else(|| format!("no mesh object named {name}_LOD{k}"))?;
    }
    Ok(out)
}

fn read_mesh(mesh: &gltf::Mesh<'_>, buffers: &[Vec<u8>], global: Mat4) -> Result<Mesh, String> {
    let get = |b: gltf::Buffer<'_>| buffers.get(b.index()).map(Vec::as_slice);
    let normal_matrix = Mat3::from_mat4(global).inverse().transpose();
    let mut out = Mesh::default();
    for prim in mesh.primitives() {
        if prim.mode() != gltf::mesh::Mode::Triangles {
            return Err("has a primitive that is not triangles".into());
        }
        let reader = prim.reader(get);
        let base = out.pos.len() as u32;
        let positions: Vec<Vec3> = reader
            .read_positions()
            .ok_or("a primitive has no positions")?
            .map(|p| global.transform_point3(Vec3::from(p)))
            .collect();
        let normals: Vec<Vec3> = reader
            .read_normals()
            .ok_or("a primitive has no normals (export with normals)")?
            .map(|n| (normal_matrix * Vec3::from(n)).normalize_or(Vec3::Y))
            .collect();
        let colors: Vec<[f32; 3]> = reader
            .read_colors(0)
            .ok_or("a primitive has no COLOR_0 vertex colours")?
            .into_rgba_f32()
            .map(|c| [c[0], c[1], c[2]])
            .collect();
        if normals.len() != positions.len() || colors.len() != positions.len() {
            return Err("attribute counts differ".into());
        }
        let indices: Vec<u32> = match reader.read_indices() {
            Some(i) => i.into_u32().collect(),
            None => (0..positions.len() as u32).collect(),
        };
        if !indices.len().is_multiple_of(3)
            || indices.iter().any(|&i| i as usize >= positions.len())
        {
            return Err("has bad indices".into());
        }
        out.indices.extend(indices.iter().map(|i| i + base));
        out.pos.extend(positions);
        out.normal.extend(normals);
        out.color.extend(colors);
    }
    if out.indices.is_empty() {
        return Err("has no triangles".into());
    }
    Ok(out)
}

struct Baked {
    record: SceneryModelRecord,
    vertices: Vec<SceneryVertex>,
    indices: Vec<u16>,
}

impl Baked {
    fn append_to(&self, t: &mut SceneryTables) {
        let mut record = self.record;
        record.first_vertex = t.vertices.len() as u32;
        let first_index = t.indices.len() as u32;
        for l in &mut record.lods {
            l[0] += first_index;
        }
        t.models.push(record);
        t.vertices.extend_from_slice(&self.vertices);
        t.indices.extend_from_slice(&self.indices);
    }
}

/// Linear light to an sRGB byte.
fn srgb_byte(l: f32) -> u8 {
    let l = l.clamp(0.0, 1.0);
    let s = if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round() as u8
}

/// Triangles each level of detail may hold, by family (docs: the brief);
/// pebbles are cheaper than other rocks.
fn budget(family: Family, name: &str) -> [usize; LODS] {
    match family {
        Family::Tree => [2500, 900, 300, 80],
        Family::Bush => [700, 250, 90, 30],
        Family::Grass => [160, 60, 24, 8],
        Family::Rock if name == "pebbles" => [300, 120, 40, 12],
        Family::Rock => [1000, 350, 120, 40],
    }
}

fn bake_model(cfg: &ModelConfig, levels: &[Mesh; LODS]) -> Result<Baked, String> {
    if cfg.name.len() > NAME_BYTES {
        return Err(format!("name is longer than {NAME_BYTES} bytes"));
    }
    if !(0.0..=SWAY_MAX).contains(&cfg.sway) {
        return Err(format!("sway {} is outside 0 to {SWAY_MAX}", cfg.sway));
    }
    let budget = budget(cfg.family, &cfg.name);
    let tris: Vec<usize> = levels.iter().map(|m| m.indices.len() / 3).collect();
    for k in 0..LODS {
        if tris[k] > budget[k] {
            return Err(format!(
                "LOD{k} has {} triangles, over its budget of {}",
                tris[k], budget[k]
            ));
        }
        if k > 0 && tris[k] > tris[k - 1] {
            return Err(format!("LOD{k} has more triangles than LOD{}", k - 1));
        }
    }
    let mut height = 0.0f32;
    let mut radius = 0.0f32;
    for p in &levels[0].pos {
        height = height.max(p.y);
        radius = radius.max(p.x.hypot(p.z));
    }
    if height < 0.05 {
        return Err(format!("is {height} tall at LOD0 (is it on the ground?)"));
    }
    for (k, m) in levels.iter().enumerate() {
        for p in &m.pos {
            if !p.is_finite() || p.x.abs() > REACH || p.z.abs() > REACH || p.y > TOP {
                return Err(format!(
                    "LOD{k} reaches {p}, beyond {REACH} tiles across or {TOP} up"
                ));
            }
            if p.y < -SINK {
                return Err(format!("LOD{k} sinks to y={}, below -{SINK}", p.y));
            }
        }
    }
    let width = 2.0 * radius;
    let limit = match cfg.family {
        Family::Tree => Some(TREE_WIDTH),
        Family::Bush => Some(BUSH_WIDTH),
        _ => None,
    };
    if let Some(limit) = limit
        && width > limit * 1.05
    {
        eprintln!(
            "warning: {} is {width:.2} across, over the {limit} footprint",
            cfg.name
        );
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut lods = [[0u32; 2]; LODS];
    for (k, m) in levels.iter().enumerate() {
        let base = vertices.len();
        if base + m.pos.len() > usize::from(u16::MAX) + 1 {
            return Err("has more than 65,536 vertices over its levels".into());
        }
        lods[k] = [indices.len() as u32, m.indices.len() as u32];
        let occlusion = occlusion::bake(m, occlusion_reach(cfg.family));
        indices.extend(m.indices.iter().map(|&i| (base as u32 + i) as u16));
        for (i, &occ) in occlusion.iter().enumerate() {
            let p = m.pos[i];
            let q = |v: f32| (v * POSITION_SCALE).round().clamp(-32768.0, 32767.0) as i16;
            let n = m.normal[i];
            let sn = |v: f32| (v * 127.0).round().clamp(-127.0, 127.0) as i8;
            let c = m.color[i];
            // Wind: more the higher up, as a share of SWAY_MAX.
            let weight = (p.y / height).clamp(0.0, 1.0).powf(1.5);
            let wind = (weight * cfg.sway / SWAY_MAX * 255.0).round() as u8;
            vertices.push(SceneryVertex {
                pos: [q(p.x), q(p.y), q(p.z)],
                _pad: 0,
                normal: [sn(n.x), sn(n.y), sn(n.z), sn(occ)],
                color: [srgb_byte(c[0]), srgb_byte(c[1]), srgb_byte(c[2]), wind],
            });
        }
    }
    let mut name = [0u8; NAME_BYTES];
    name[..cfg.name.len()].copy_from_slice(cfg.name.as_bytes());
    Ok(Baked {
        record: SceneryModelRecord {
            name,
            first_vertex: 0,
            vertex_count: vertices.len() as u32,
            lods,
            height,
            radius,
            sway: cfg.sway,
            family: cfg.family as u8,
            _pad: [0; 3],
        },
        vertices,
        indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mesh object's name and triangles.
    type TestMesh = (String, Vec<[([f32; 3], u8); 3]>);

    /// A GLB with a mesh object per `(name, triangles)`; each triangle is
    /// three corners of (position, linear colour byte).
    fn glb(meshes: &[TestMesh]) -> Vec<u8> {
        let mut bin: Vec<u8> = Vec::new();
        let (mut views, mut accessors, mut nodes, mut mesh_json) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut add = |bin: &mut Vec<u8>, data: &[u8], target: u32| {
            while !bin.len().is_multiple_of(4) {
                bin.push(0);
            }
            views.push(format!(
                r#"{{"buffer":0,"byteOffset":{},"byteLength":{},"target":{target}}}"#,
                bin.len(),
                data.len()
            ));
            bin.extend_from_slice(data);
            views.len() - 1
        };
        for (m, (name, tris)) in meshes.iter().enumerate() {
            let pos: Vec<f32> = tris.iter().flatten().flat_map(|c| c.0).collect();
            let mut normals = Vec::new();
            for _ in 0..tris.len() * 3 {
                normals.extend_from_slice(&[0.0f32, 1.0, 0.0]);
            }
            let colors: Vec<u8> = tris
                .iter()
                .flatten()
                .flat_map(|c| [c.1, c.1, c.1, 255])
                .collect();
            let idx: Vec<u16> = (0..tris.len() as u16 * 3).collect();
            let count = tris.len() * 3;
            let (lo, hi) = pos
                .chunks(3)
                .fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
                    (
                        [lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])],
                        [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])],
                    )
                });
            let v = add(&mut bin, bytemuck::cast_slice(&pos), 34962);
            accessors.push(format!(
                r#"{{"bufferView":{v},"componentType":5126,"count":{count},"type":"VEC3","min":{lo:?},"max":{hi:?}}}"#
            ));
            let v = add(&mut bin, bytemuck::cast_slice(&normals), 34962);
            accessors.push(format!(
                r#"{{"bufferView":{v},"componentType":5126,"count":{count},"type":"VEC3"}}"#
            ));
            let v = add(&mut bin, &colors, 34962);
            accessors.push(format!(
                r#"{{"bufferView":{v},"componentType":5121,"normalized":true,"count":{count},"type":"VEC4"}}"#
            ));
            let v = add(&mut bin, bytemuck::cast_slice(&idx), 34963);
            accessors.push(format!(
                r#"{{"bufferView":{v},"componentType":5123,"count":{count},"type":"SCALAR"}}"#
            ));
            let a = accessors.len() - 4;
            mesh_json.push(format!(
                r#"{{"name":"{name}","primitives":[{{"attributes":{{"POSITION":{a},"NORMAL":{},"COLOR_0":{}}},"indices":{}}}]}}"#,
                a + 1,
                a + 2,
                a + 3
            ));
            nodes.push(format!(r#"{{"name":"{name}","mesh":{m}}}"#));
        }
        let scene_nodes: Vec<String> = (0..meshes.len()).map(|i| i.to_string()).collect();
        let mut json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[{}]}}],"nodes":[{}],"meshes":[{}],"accessors":[{}],"bufferViews":[{}],"buffers":[{{"byteLength":{}}}]}}"#,
            scene_nodes.join(","),
            nodes.join(","),
            mesh_json.join(","),
            accessors.join(","),
            views.join(","),
            bin.len()
        )
        .into_bytes();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let total = 12 + 8 + json.len() + 8 + bin.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json);
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
        out
    }

    /// A tree of triangles 2 tall, a quarter as many at each level (at least one).
    fn tree(name: &str, tris: usize) -> Vec<TestMesh> {
        (0..LODS)
            .map(|k| {
                let tri = [
                    ([-0.5, 0.0, 0.0], 128),
                    ([0.5, 0.0, 0.0], 128),
                    ([0.0, 2.0, 0.0], 255),
                ];
                (
                    format!("{name}_LOD{k}"),
                    vec![tri; (tris >> (2 * k)).max(1)],
                )
            })
            .collect()
    }

    fn cfg(name: &str, family: Family, sway: f32) -> ModelConfig {
        ModelConfig {
            name: name.into(),
            source: format!("shared:{name}.glb"),
            family,
            sway,
        }
    }

    fn bake(name: &str, tris: usize, family: Family) -> Result<Baked, String> {
        let bytes = glb(&tree(name, tris));
        let levels = levels_from_glb(&bytes, Path::new("."), name)?;
        bake_model(&cfg(name, family, 0.125), &levels)
    }

    #[test]
    fn srgb_bytes() {
        assert_eq!(srgb_byte(0.0), 0);
        assert_eq!(srgb_byte(1.0), 255);
        assert_eq!(srgb_byte(0.5), 188);
        assert_eq!(srgb_byte(2.0), 255);
    }

    #[test]
    fn a_model_bakes_and_reads_back() {
        let baked = bake("oak-a", 4, Family::Tree).unwrap();
        let mut t = SceneryTables::default();
        baked.append_to(&mut t);
        baked.append_to(&mut t);
        let mut w = PackWriter::new();
        t.write(&mut w);
        read_back(&w.finish()).unwrap();
        let r = &t.models[1];
        assert_eq!(r.name(), "oak-a");
        assert_eq!(r.first_vertex, 12 + 3 + 3 + 3);
        assert_eq!(r.height, 2.0);
        assert_eq!(r.lods.map(|l| l[1] / 3), [4, 1, 1, 1]);
        assert_eq!(r.lods[0][0], 12 + 3 + 3 + 3);
        // Base is not swayed, top moves the model's whole sway.
        let v = &t.vertices[r.first_vertex as usize..];
        assert_eq!(v[0].color, [188, 188, 188, 0]);
        assert_eq!(v[2].color[3], 128);
        assert_eq!(v[2].pos, [0, 4096, 0]);
        assert_eq!(v[2].normal[..3], [0, 127, 0]);
    }

    #[test]
    fn budgets_and_missing_levels_are_refused() {
        assert!(bake("g", 161, Family::Grass).is_err());
        assert!(bake("g", 160, Family::Grass).is_ok());
        // Pebbles are cheaper than other rocks.
        assert!(bake("pebbles", 301, Family::Rock).is_err());
        assert!(bake("boulder", 301, Family::Rock).is_ok());
        let mut meshes = tree("t", 4);
        meshes.pop();
        let bytes = glb(&meshes);
        let err = levels_from_glb(&bytes, Path::new("."), "t").err().unwrap();
        assert!(err.contains("t_LOD3"), "{err}");
    }

    #[test]
    fn out_of_reach_models_are_refused() {
        let mut meshes = tree("t", 1);
        meshes[2].1[0][0].0 = [9.0, 0.0, 0.0];
        let bytes = glb(&meshes);
        let levels = levels_from_glb(&bytes, Path::new("."), "t").unwrap();
        assert!(bake_model(&cfg("t", Family::Tree, 0.1), &levels).is_err());
        let bytes = glb(&tree("t", 1));
        let levels = levels_from_glb(&bytes, Path::new("."), "t").unwrap();
        assert!(bake_model(&cfg("t", Family::Tree, 0.5), &levels).is_err());
    }

    #[test]
    fn two_bakes_are_identical() {
        let a = bake("oak-a", 3, Family::Tree).unwrap();
        let b = bake("oak-a", 3, Family::Tree).unwrap();
        assert_eq!(a.vertices, b.vertices);
        assert_eq!(a.indices, b.indices);
    }
}
