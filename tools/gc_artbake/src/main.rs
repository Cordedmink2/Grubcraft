//! Grubcraft art baker: turns rigged, animated glTF characters into the
//! soldier model pack (`assets/models.pak`) the game embeds.
//!
//! Each model is listed in `assets-src/models.ron`: its glTF file, the unit
//! type it dresses, its height, which colours take the team colour, and which
//! of its animations play as idle, walk, attack and so on. For each model the
//! baker:
//!
//! 1. Colours every vertex from the material's texture (the low-poly packs
//!    paint flat colour swatches, so one sample per vertex keeps the look).
//! 2. Skins every vertex to its four heaviest bones (or binds a rigid prop,
//!    such as a bow, to the bone it hangs from), and keeps its texture
//!    coordinate and tangent.
//! 3. Welds, simplifies to four levels of detail (each from the one before)
//!    and orders the triangles for the GPU's vertex cache.
//! 4. Samples each clip at a fixed rate into bone texture rows: for every
//!    frame, a 3 × 4 matrix per bone that moves a stored vertex to that pose.
//!
//! Raw files from some packs (Quaternius) may not be redistributed, so they
//! live in the project's shared folder, not the repository; only the baked
//! pack is committed (docs/16-art-sourcing.md).
//!
//! Next to the pack the baker writes `models.manifest`: the baker and model
//! schema versions, hashes of `models.ron`, of every source and of the pack.
//! `--check` compares the committed pack with that manifest and the sources
//! the repository holds, so CI notices a pack that no longer matches
//! (`manifest.rs`).
//!
//! Usage: gc_artbake [--config assets-src/models.ron] [--shared DIR]
//!                   [--out assets/models.pak] [--report | --check]
//!        gc_artbake --detail [--out assets/detail.pak] [--check]  (the shared detail textures)
//! `--report` prints each model's most common colours, for choosing team colours.
//! `--check` bakes nothing: it checks `--out` and its manifest.

mod detail;
mod manifest;
mod textures;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use gc_assets::detail::{DetailTag, tiles_code};
use gc_assets::model::{
    BONE_TEXELS_PER_ROW, BONES_PER_VERTEX, ClipKind, ClipRecord, LODS, MAX_BONES, MAX_MATERIALS,
    ModelRecord, ModelTables, ModelVertex, NAME_BYTES, POSITION_SCALE, f16_bits,
};
use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use serde::Deserialize;

/// Frames per second clips are sampled at. The shader blends between frames.
const SAMPLE_FPS: f32 = 12.0;
/// How much moving a texture coordinate across the whole atlas counts,
/// against moving a vertex across the whole model, when a textured model's
/// smaller levels of detail collapse across texture seams.
const UV_WEIGHT: f32 = 1.0;
/// When a textured model's smaller levels of detail are cut part by part,
/// parts at least this big (their box's longest side, against the model's
/// height) keep [`MIN_PART_TRIANGLES`], so a hat brim stays a brim.
const BIG_PART: f32 = 0.1;
const MIN_PART_TRIANGLES: usize = 8;
/// How far, against its own size, a part may move as it is cut. Thin
/// shells (a crown, a brim, armour plates) stop short of their share
/// rather than flatten into nothing.
const PART_ERROR: f32 = 0.2;

#[derive(Deserialize)]
struct Config {
    models: Vec<ModelConfig>,
}

#[derive(Deserialize)]
struct ModelConfig {
    /// The unit type this model dresses, exactly as named in data/units.ron.
    name: String,
    /// `shared:` + a path under the shared assets folder, or `repo:` + a
    /// path under the repository's `assets-src/`.
    source: String,
    /// Height of the rest pose, in tiles.
    height: f32,
    /// Turn the model this many degrees about the vertical so it faces +z.
    #[serde(default)]
    turn: f32,
    /// Colours (`#rrggbb`) that take the team colour, and how close a
    /// vertex colour must be to count (0 to 441, RGB distance).
    #[serde(default)]
    team: Vec<String>,
    #[serde(default = "default_team_tolerance")]
    team_tolerance: f32,
    /// Also take the team colour: every vertex whose hue (degrees) lies in
    /// this range and whose saturation is at least the third value (0 to 1).
    /// For painted textures with many shades of one cloth colour.
    #[serde(default)]
    team_hue: Option<(f32, f32, f32)>,
    /// Clip kind (`Idle`, `Walk`, ...) to the glTF animation's name.
    clips: HashMap<String, String>,
    /// Triangle budgets for each level of detail, from full to fewest.
    lod_triangles: [usize; LODS],
    /// Painted texture maps, for models whose detail is in textures rather
    /// than vertex colours. Models naming the very same maps share one
    /// material (a building's construction stages).
    #[serde(default)]
    textures: Option<textures::TextureConfig>,
    /// Keep the source's own positions, in tiles, with the origin at the
    /// middle of the footprint on the ground: no re-centring or scaling to
    /// `height`, so a building's construction stages line up with each
    /// other and with its footprint. `height` is then only the size the
    /// game culls and picks a level of detail by.
    #[serde(default)]
    keep_origin: bool,
    /// Surface detail (docs/31): read each face's surface classes from the
    /// glTF's `COLOR_0` (red and green are the two classes, a fifteenth
    /// each, blue the mix) and repeat the shared detail textures about this
    /// many tiles (metres) apart. Left out, the model wears none.
    #[serde(default)]
    detail_tile: Option<f32>,
}

fn default_team_tolerance() -> f32 {
    40.0
}

struct Options {
    config: PathBuf,
    shared: PathBuf,
    repo_src: PathBuf,
    out: PathBuf,
    report: bool,
    check: bool,
    /// Bake or check the shared detail textures instead of a model pack.
    detail: bool,
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
        config: root.join("assets-src/models.ron"),
        shared: PathBuf::from("/mnt/project-files/assets-src"),
        repo_src: root.join("assets-src"),
        out: root.join("assets/models.pak"),
        report: false,
        check: false,
        detail: false,
    };
    let mut out_given = false;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--report" => {
                opts.report = true;
                continue;
            }
            "--check" => {
                opts.check = true;
                continue;
            }
            "--detail" => {
                opts.detail = true;
                continue;
            }
            _ => {}
        }
        let Some(value) = args.next() else { usage() };
        match flag.as_str() {
            "--config" => opts.config = value.into(),
            "--shared" => opts.shared = value.into(),
            "--out" => {
                opts.out = value.into();
                out_given = true;
            }
            _ => usage(),
        }
    }
    if opts.detail && !out_given {
        opts.out = root.join("assets/detail.pak");
    }
    let detail_dir = opts.repo_src.join("detail");
    let result = if opts.detail && opts.check {
        detail::check(&detail_dir, &opts.out)
    } else if opts.detail {
        detail::run(&detail_dir, &opts.out)
    } else if opts.check {
        manifest::check(&opts)
    } else {
        run(&opts)
    };
    if let Err(e) = result {
        eprintln!("gc_artbake: {e}");
        std::process::exit(1);
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: gc_artbake [--config FILE] [--shared DIR] [--out FILE] [--report | --check]\n       gc_artbake --detail [--out FILE] [--check]"
    );
    std::process::exit(2);
}

fn run(opts: &Options) -> Result<(), String> {
    let text = std::fs::read_to_string(&opts.config)
        .map_err(|e| format!("{}: {e}", opts.config.display()))?;
    let config: Config =
        ron::from_str(&text).map_err(|e| format!("{}: {e}", opts.config.display()))?;
    let (bytes, tables) = bake_pack(&config, opts)?;
    if let Some(dir) = opts.out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&opts.out, &bytes).map_err(|e| format!("{}: {e}", opts.out.display()))?;
    manifest::write(opts, &text, &config, &bytes)?;
    println!(
        "wrote {} ({} KB): {} models, {} vertices, {} triangles at full detail, {} bone rows, {} materials",
        opts.out.display(),
        bytes.len() / 1024,
        tables.models.len(),
        tables.vertices.len(),
        tables.models.iter().map(|m| m.lods[0][1] / 3).sum::<u32>(),
        tables.bones.len() / BONE_TEXELS_PER_ROW as usize,
        tables.materials(),
    );
    Ok(())
}

/// Bake every model in `config` into pack bytes, without writing anything.
fn bake_pack(config: &Config, opts: &Options) -> Result<(Vec<u8>, ModelTables), String> {
    let resolve = |src: &str| opts.resolve(src);
    let mut tables = ModelTables::default();
    // Materials already baked, by the maps they were baked from.
    let mut materials: HashMap<String, usize> = HashMap::new();
    for m in &config.models {
        let path = resolve(&m.source).map_err(|e| format!("{}: {e}", m.name))?;
        // A painted albedo map also gives the vertex colours (for the report
        // and anything drawn without textures).
        let albedo = match &m.textures {
            Some(t) => {
                let path = resolve(&t.albedo).map_err(|e| format!("{}: {e}", m.name))?;
                let img = textures::load_png(&path).map_err(|e| format!("{}: {e}", m.name))?;
                Some((img, path))
            }
            None => None,
        };
        let mut baked = bake_model(m, &path, albedo.as_ref().map(|a| &a.0))
            .map_err(|e| format!("{} ({}): {e}", m.name, path.display()))?;
        if opts.report {
            report(&baked);
        }
        if let (Some(cfg), Some((albedo, albedo_path))) = (&m.textures, &albedo) {
            let key = format!("{cfg:?}");
            let material = match materials.get(&key) {
                Some(&material) => material,
                None => {
                    let material = tables.materials();
                    if material >= MAX_MATERIALS {
                        return Err(format!("{}: more than {MAX_MATERIALS} materials", m.name));
                    }
                    let texels = textures::bake(cfg, albedo, albedo_path, resolve)
                        .map_err(|e| format!("{}: {e}", m.name))?;
                    tables.textures.extend_from_slice(&texels);
                    materials.insert(key, material);
                    material
                }
            };
            for v in &mut baked.verts {
                v.material = material as u8 + 1;
            }
        }
        baked.append_to(&mut tables, &m.name)?;
    }
    let mut pack = gc_assets::PackWriter::new();
    tables.write(&mut pack);
    let bytes = pack.finish();
    // The game turns soldier models off for a pack its loader rejects, so
    // load it here first.
    let mut store = vec![0u128; bytes.len().div_ceil(16)];
    bytemuck::cast_slice_mut::<u128, u8>(&mut store)[..bytes.len()].copy_from_slice(&bytes);
    let view = &bytemuck::cast_slice::<u128, u8>(&store)[..bytes.len()];
    let loaded = gc_assets::Pack::parse(view).map_err(|e| e.to_string());
    loaded
        .and_then(|p| gc_assets::model::Models::from_pack(&p).map_err(|e| e.to_string()))
        .map_err(|e| format!("the baked pack does not load: {e}"))?;
    Ok((bytes, tables))
}

/// A vertex before quantisation, in tiles.
#[derive(Clone, Copy)]
struct Vert {
    pos: Vec3,
    normal: Vec3,
    /// xyz along the texture's u, w the bitangent's handedness; zero if the
    /// source has none.
    tangent: Vec4,
    uv: Vec2,
    color: [u8; 3],
    /// The surface classes and mix, from the source's `COLOR_0` (the repeat
    /// code is added once the model's size is known).
    detail: DetailTag,
    /// Heaviest bone first; weights sum to 1.
    bones: [(u8, f32); BONES_PER_VERTEX],
}

struct BakedModel {
    verts: Vec<ModelVertex>,
    lods: [Vec<u16>; LODS],
    height: f32,
    bones: u32,
    /// Clip kind, and its frames: per frame, `bones` matrices (stored vertex
    /// to pose).
    clips: Vec<(ClipKind, Vec<Vec<Mat4>>)>,
    /// Most common colours and how many vertices have each.
    palette: Vec<([u8; 3], usize, bool)>,
}

impl BakedModel {
    fn append_to(&self, t: &mut ModelTables, name: &str) -> Result<(), String> {
        if name.len() > NAME_BYTES {
            return Err(format!("name {name:?} is longer than {NAME_BYTES} bytes"));
        }
        let mut name_bytes = [0; NAME_BYTES];
        name_bytes[..name.len()].copy_from_slice(name.as_bytes());
        let first_vertex = t.vertices.len() as u32;
        t.vertices.extend_from_slice(&self.verts);
        let mut lods = [[0; 2]; LODS];
        for (lod, indices) in self.lods.iter().enumerate() {
            lods[lod] = [t.indices.len() as u32, indices.len() as u32];
            t.indices.extend_from_slice(indices);
        }
        let first_clip = t.clips.len() as u32;
        for (kind, frames) in &self.clips {
            let first_row = (t.bones.len() / BONE_TEXELS_PER_ROW as usize) as u32;
            for frame in frames {
                let mut row = vec![[0u16; 4]; BONE_TEXELS_PER_ROW as usize];
                for (b, m) in frame.iter().enumerate() {
                    // Rows of the 3 × 4 matrix: row r is (m[0][r], m[1][r], m[2][r], m[3][r]).
                    for r in 0..3 {
                        let v = Vec4::new(m.x_axis[r], m.y_axis[r], m.z_axis[r], m.w_axis[r]);
                        row[b * 3 + r] = v.to_array().map(f16_bits);
                    }
                }
                t.bones.extend_from_slice(&row);
            }
            t.clips.push(ClipRecord {
                kind: *kind as u8,
                _pad: [0; 3],
                first_row,
                frames: frames.len() as u32,
                fps: SAMPLE_FPS,
            });
        }
        t.models.push(ModelRecord {
            name: name_bytes,
            first_vertex,
            vertex_count: self.verts.len() as u32,
            lods,
            first_clip,
            clip_count: self.clips.len() as u32,
            height: self.height,
            bones: self.bones,
        });
        Ok(())
    }
}

fn report(m: &BakedModel) {
    let tris: Vec<String> = m.lods.iter().map(|l| (l.len() / 3).to_string()).collect();
    println!(
        "  {} vertices, {} triangles, {} bones, {} clips; colours (team marked *):",
        m.verts.len(),
        tris.join(" / "),
        m.bones,
        m.clips.len()
    );
    for (c, n, team) in m.palette.iter().take(12) {
        println!(
            "    #{:02x}{:02x}{:02x} {n:>5}{}",
            c[0],
            c[1],
            c[2],
            if *team { " *" } else { "" }
        );
    }
}

/// The glTF document with its buffers and decoded images.
struct Source {
    doc: gltf::Document,
    buffers: Vec<Vec<u8>>,
    images: Vec<Option<Image>>,
}

struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Image {
    /// Nearest texel at a UV, wrapping.
    fn sample(&self, uv: Vec2) -> [u8; 3] {
        let x = (uv.x.rem_euclid(1.0) * self.width as f32) as u32 % self.width;
        let y = (uv.y.rem_euclid(1.0) * self.height as f32) as u32 % self.height;
        let i = ((y * self.width + x) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]]
    }
}

fn load(path: &Path) -> Result<Source, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let gltf = gltf::Gltf::from_slice(&bytes).map_err(|e| e.to_string())?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut buffers = Vec::new();
    for b in gltf.document.buffers() {
        let data = match b.source() {
            gltf::buffer::Source::Bin => gltf.blob.clone().ok_or("missing GLB binary chunk")?,
            gltf::buffer::Source::Uri(uri) => read_uri(dir, uri)?,
        };
        buffers.push(data);
    }
    let mut images = Vec::new();
    for img in gltf.document.images() {
        let data = match img.source() {
            gltf::image::Source::View { view, .. } => {
                let buf = &buffers[view.buffer().index()];
                buf[view.offset()..view.offset() + view.length()].to_vec()
            }
            gltf::image::Source::Uri { uri, .. } => read_uri(dir, uri)?,
        };
        images.push(decode_png(&data).ok());
    }
    Ok(Source {
        doc: gltf.document,
        buffers,
        images,
    })
}

fn read_uri(dir: &Path, uri: &str) -> Result<Vec<u8>, String> {
    if let Some(rest) = uri.strip_prefix("data:") {
        let (_, data) = rest.split_once(";base64,").ok_or("unsupported data URI")?;
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| e.to_string())
    } else {
        let path = dir.join(uri.replace("%20", " "));
        std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

fn decode_png(data: &[u8]) -> Result<Image, String> {
    let mut decoder = png::Decoder::new(data);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let px = (info.width * info.height) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..px * 4].to_vec(),
        png::ColorType::Rgb => buf[..px * 3]
            .chunks(3)
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf[..px * 2]
            .chunks(2)
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        png::ColorType::Grayscale => buf[..px].iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG not expanded".into()),
    };
    Ok(Image {
        width: info.width,
        height: info.height,
        rgba,
    })
}

/// A node's local transform as translation, rotation, scale.
#[derive(Clone, Copy)]
struct Trs {
    t: Vec3,
    r: Quat,
    s: Vec3,
}

impl Trs {
    fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.s, self.r, self.t)
    }
}

/// The scene's nodes: parents, rest transforms and globals.
struct Skeleton {
    parent: Vec<Option<usize>>,
    rest: Vec<Trs>,
    /// Nodes in an order where every parent comes before its children.
    order: Vec<usize>,
}

impl Skeleton {
    fn new(doc: &gltf::Document) -> Skeleton {
        let n = doc.nodes().count();
        let mut parent = vec![None; n];
        let mut rest = Vec::with_capacity(n);
        for node in doc.nodes() {
            for child in node.children() {
                parent[child.index()] = Some(node.index());
            }
            let (t, r, s) = node.transform().decomposed();
            rest.push(Trs {
                t: Vec3::from(t),
                r: Quat::from_array(r),
                s: Vec3::from(s),
            });
        }
        let mut order = Vec::with_capacity(n);
        let mut stack: Vec<usize> = (0..n).filter(|&i| parent[i].is_none()).collect();
        while let Some(i) = stack.pop() {
            order.push(i);
            stack.extend(doc.nodes().nth(i).unwrap().children().map(|c| c.index()));
        }
        Skeleton {
            parent,
            rest,
            order,
        }
    }

    fn globals(&self, local: &[Trs]) -> Vec<Mat4> {
        let mut g = vec![Mat4::IDENTITY; local.len()];
        for &i in &self.order {
            let m = local[i].matrix();
            g[i] = match self.parent[i] {
                Some(p) => g[p] * m,
                None => m,
            };
        }
        g
    }
}

fn bake_model(
    cfg: &ModelConfig,
    path: &Path,
    albedo: Option<&Image>,
) -> Result<BakedModel, String> {
    let src = load(path)?;
    let doc = &src.doc;
    let get = |b: gltf::Buffer<'_>| src.buffers.get(b.index()).map(std::vec::Vec::as_slice);
    let skel = Skeleton::new(doc);
    let rest_globals = skel.globals(&skel.rest);

    // A model without a skin (a building) never moves: every part hangs
    // from the one still bone.
    let skin = doc.skins().next();
    if skin.is_none() && !cfg.clips.is_empty() {
        return Err("clips named, but the model is not rigged".into());
    }
    let joints: Vec<usize> = skin
        .iter()
        .flat_map(|s| s.joints().map(|j| j.index()))
        .collect();
    if joints.len() > MAX_BONES as usize {
        return Err(format!("{} bones, more than {MAX_BONES}", joints.len()));
    }
    let inverse_bind: Vec<Mat4> = match skin
        .as_ref()
        .and_then(|s| s.reader(get).read_inverse_bind_matrices())
    {
        Some(r) => r.map(|m| Mat4::from_cols_array_2d(&m)).collect(),
        None => vec![Mat4::IDENTITY; joints.len()],
    };
    // Exporters may bind a mesh in a different pose from the skeleton's
    // rest pose (a T-pose bind under an A-pose rest). Skinned vertices are
    // kept in the bind pose, seen from the first joint, and each bone's
    // matrices carry its own bind-to-rest difference relative to that, so
    // the shader's weighted blend is exact skinning. `bind_fix` is that
    // difference per bone (the identity when bind and rest agree, and for
    // the still bone).
    let bind_to_rest: Vec<Mat4> = (0..joints.len())
        .map(|b| rest_globals[joints[b]] * inverse_bind[b])
        .collect();
    let bind_space = bind_to_rest.first().copied().unwrap_or(Mat4::IDENTITY);
    let bind_fix: Vec<Mat4> = bind_to_rest
        .iter()
        .map(|m| *m * bind_space.inverse())
        .chain([Mat4::IDENTITY])
        .collect();
    let joint_of_node: HashMap<usize, usize> =
        joints.iter().enumerate().map(|(b, &n)| (n, b)).collect();
    // Parts that hang from no bone use one extra bone that never moves.
    let still_bone = joints.len();

    // 1. Gather every triangle, with the bones it follows, in the rest pose
    // (skinned meshes in their bind pose, see above).
    let mut verts: Vec<Vert> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for node in doc.nodes() {
        let Some(mesh) = node.mesh() else { continue };
        let skinned = node.skin().is_some();
        // The bone a rigid part hangs from: the nearest ancestor joint.
        let mut holder = None;
        let mut at = Some(node.index());
        while let Some(i) = at {
            if let Some(&b) = joint_of_node.get(&i) {
                holder = Some(b);
                break;
            }
            at = skel.parent[i];
        }
        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = prim.reader(get);
            let positions: Vec<Vec3> = reader
                .read_positions()
                .ok_or("primitive without positions")?
                .map(Vec3::from)
                .collect();
            let normals: Vec<Vec3> = match reader.read_normals() {
                Some(n) => n.map(Vec3::from).collect(),
                None => vec![Vec3::Y; positions.len()],
            };
            let uvs: Vec<Vec2> = match reader.read_tex_coords(0) {
                Some(t) => t.into_f32().map(Vec2::from).collect(),
                None => vec![Vec2::ZERO; positions.len()],
            };
            let tangents: Option<Vec<Vec4>> =
                reader.read_tangents().map(|t| t.map(Vec4::from).collect());
            let tags: Option<Vec<DetailTag>> = cfg
                .detail_tile
                .and_then(|_| reader.read_colors(0))
                .map(|c| c.into_rgba_f32().map(detail_tag).collect());
            // Every vertex's (joint, weight) pairs, from both sets of four an
            // exporter may write.
            let mut influences: Vec<Vec<(u16, f32)>> = Vec::new();
            for set in 0..2 {
                let (Some(j), Some(w)) = (reader.read_joints(set), reader.read_weights(set)) else {
                    break;
                };
                influences.resize(positions.len(), Vec::new());
                for (k, (j, w)) in j.into_u16().zip(w.into_f32()).enumerate() {
                    influences[k].extend(j.into_iter().zip(w));
                }
            }
            let material = prim.material();
            let pbr = material.pbr_metallic_roughness();
            let factor = Vec4::from(pbr.base_color_factor());
            let texture = albedo.or_else(|| {
                pbr.base_color_texture()
                    .and_then(|t| src.images.get(t.texture().source().index()))
                    .and_then(|i| i.as_ref())
            });

            let base = verts.len() as u32;
            let mut mirrored = false;
            for k in 0..positions.len() {
                let (bones, to_model) = match influences.get(k) {
                    Some(inf) if skinned => (top_weights(inf), bind_space),
                    _ => {
                        let bone = holder.unwrap_or(still_bone);
                        let to_model = bind_fix[bone].inverse() * rest_globals[node.index()];
                        (rigid_skin(bone as u8), to_model)
                    }
                };
                mirrored = to_model.determinant() < 0.0;
                let tangent = tangents.as_ref().map_or(Vec4::ZERO, |t| {
                    let dir = to_model
                        .transform_vector3(t[k].truncate())
                        .normalize_or_zero();
                    // A mirroring transform flips the bitangent too.
                    let mirrored = to_model.determinant() < 0.0;
                    dir.extend(if (t[k].w < 0.0) != mirrored {
                        -1.0
                    } else {
                        1.0
                    })
                });
                let mut color = Vec3::splat(255.0) * factor.truncate();
                if let Some(img) = texture {
                    let c = img.sample(uvs[k]);
                    color *= Vec3::new(f32::from(c[0]), f32::from(c[1]), f32::from(c[2])) / 255.0;
                }
                verts.push(Vert {
                    pos: to_model.transform_point3(positions[k]),
                    normal: to_model.transform_vector3(normals[k]).normalize_or(Vec3::Y),
                    tangent,
                    uv: uvs[k],
                    color: color.round().to_array().map(|c| c.clamp(0.0, 255.0) as u8),
                    detail: tags.as_ref().map_or(DetailTag::default(), |t| t[k]),
                    bones,
                });
            }
            let first = indices.len();
            match reader.read_indices() {
                Some(i) => indices.extend(i.into_u32().map(|i| base + i)),
                None => indices.extend(base..verts.len() as u32),
            }
            if mirrored {
                // Keep the outside counter-clockwise, as back-face culling expects.
                for tri in indices[first..].as_chunks_mut::<3>().0 {
                    tri.swap(1, 2);
                }
            }
        }
    }
    if indices.is_empty() {
        return Err("no triangles".into());
    }
    if cfg.textures.is_some() && verts.iter().any(|v| v.tangent == Vec4::ZERO) {
        return Err("a textured model needs tangents (export them with the glTF)".into());
    }

    // 2. Into game space: face +z, feet on the ground, `height` tall.
    let turn = Mat4::from_rotation_y(cfg.turn.to_radians());
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in &verts {
        // Measured in the rest pose, where skinned vertices may not be.
        let rest = v.bones.iter().fold(Vec3::ZERO, |p, &(b, w)| {
            p + bind_fix[b as usize].transform_point3(v.pos) * w
        });
        let p = turn.transform_point3(rest);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let to_game = if cfg.keep_origin {
        // Stored positions reach this far from the origin, in tiles.
        let room = f32::from(i16::MAX) / POSITION_SCALE;
        if lo.min_element() < -room || hi.max_element() > room {
            return Err(format!(
                "reaches {lo} to {hi}, beyond the {room:.2} tiles a position can store"
            ));
        }
        turn
    } else {
        let scale = cfg.height / (hi.y - lo.y).max(1e-6);
        Mat4::from_scale(Vec3::splat(scale))
            * Mat4::from_translation(Vec3::new(-(lo.x + hi.x) / 2.0, -lo.y, -(lo.z + hi.z) / 2.0))
            * turn
    };
    let from_game = to_game.inverse();
    for v in &mut verts {
        v.pos = to_game.transform_point3(v.pos);
        v.normal = turn.transform_vector3(v.normal).normalize_or(Vec3::Y);
        let t = turn
            .transform_vector3(v.tangent.truncate())
            .normalize_or_zero();
        v.tangent = t.extend(v.tangent.w);
    }

    if cfg.detail_tile.is_some() && verts.iter().all(|v| v.detail.class_a == 0) {
        return Err(
            "detail_tile is set but no face names a surface class (the glTF needs COLOR_0)".into(),
        );
    }
    // Detail repeats: how many metres of surface one unit of the texture
    // atlas covers, over how far one repeat reaches.
    if let Some(tile) = cfg.detail_tile {
        let code = tiles_code(atlas_metres(&verts, &indices) / tile.max(1e-3));
        for v in &mut verts {
            v.detail.tiles = code;
        }
    }

    // 3. Quantise and weld identical vertices.
    let team_colors: Vec<Vec3> = cfg
        .team
        .iter()
        .map(|h| parse_hex(h))
        .collect::<Result<_, _>>()?;
    let mut welded: Vec<ModelVertex> = Vec::new();
    let mut seen: HashMap<ModelVertex, u32> = HashMap::new();
    let remap: Vec<u32> = verts
        .iter()
        .map(|v| {
            let q = quantise(v, &team_colors, cfg.team_tolerance, cfg.team_hue);
            *seen.entry(q).or_insert_with(|| {
                welded.push(q);
                welded.len() as u32 - 1
            })
        })
        .collect();
    let indices: Vec<u32> = indices.iter().map(|&i| remap[i as usize]).collect();

    // 4. The levels of detail, each ordered for the vertex cache.
    let positions: Vec<[f32; 3]> = welded
        .iter()
        .map(|v| v.pos.map(|c| f32::from(c) / POSITION_SCALE))
        .collect();
    let adapter = meshopt::VertexDataAdapter::new(bytemuck::cast_slice(&positions), 12, 0)
        .map_err(|e| e.to_string())?;
    let textured = cfg.textures.is_some();
    // Texture coordinates, for the permissive pass to weigh.
    let uvs: Vec<[f32; 2]> = welded
        .iter()
        .map(|v| v.uv.map(|c| f32::from(c) / 65_535.0))
        .collect();
    let unlocked = vec![false; welded.len()];
    let simplify = |indices: &[u32], tris: usize| -> Vec<u32> {
        if indices.len() / 3 <= tris {
            return indices.to_vec();
        }
        // Texture seams hold first, or reduced triangles would span
        // unrelated parts of the atlas; vertex-coloured models may collapse
        // across them.
        let options = if textured {
            meshopt::SimplifyOptions::None
        } else {
            meshopt::SimplifyOptions::Permissive
        };
        let out = meshopt::simplify(indices, &adapter, tris * 3, 0.05, options, None);
        if out.len() / 3 <= tris * 5 / 4 {
            return out;
        }
        if !textured {
            // Attribute seams can stop the careful simplifier early.
            return meshopt::simplify_sloppy(indices, &adapter, tris * 3, 0.1, None);
        }
        // The seams stop it: cut each separate part (a hat's crown, its
        // brim) to its share, letting collapses cross seams where the
        // texture coordinates barely move. Thin shells would otherwise
        // flatten away whole.
        let ratio = tris as f32 / (out.len() / 3) as f32;
        let mut cut = Vec::new();
        for part in parts(&out, &welded) {
            let n = part.len() / 3;
            let (lo, hi) = part.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), &i| {
                let p = Vec3::from(positions[i as usize]);
                (lo.min(p), hi.max(p))
            });
            let mut target = (n as f32 * ratio).round() as usize;
            if (hi - lo).max_element() >= BIG_PART * cfg.height {
                target = target.max(MIN_PART_TRIANGLES.min(n));
            }
            cut.extend(meshopt::simplify_with_attributes_and_locks(
                &part,
                &adapter,
                bytemuck::cast_slice(&uvs),
                &[UV_WEIGHT; 2],
                8,
                &unlocked,
                target * 3,
                PART_ERROR,
                meshopt::SimplifyOptions::Permissive | meshopt::SimplifyOptions::Sparse,
                None,
            ));
        }
        cut
    };
    let mut chain: Vec<Vec<u32>> = Vec::with_capacity(LODS);
    for &tris in &cfg.lod_triangles {
        let finer = chain.last().unwrap_or(&indices);
        chain.push(meshopt::optimize_vertex_cache(
            &simplify(finer, tris),
            welded.len(),
        ));
    }
    // Keep only vertices a level of detail uses, in first-use order.
    let mut new_index = vec![u32::MAX; welded.len()];
    let mut verts_out = Vec::new();
    for &i in chain.iter().flatten() {
        if new_index[i as usize] == u32::MAX {
            new_index[i as usize] = verts_out.len() as u32;
            verts_out.push(welded[i as usize]);
        }
    }
    if verts_out.len() > u16::MAX as usize + 1 {
        return Err(format!("{} vertices, more than 65,536", verts_out.len()));
    }
    let lods = core::array::from_fn(|l| {
        chain[l]
            .iter()
            .map(|&i| new_index[i as usize] as u16)
            .collect()
    });

    // 5. Sample the clips into bone matrices (in game space).
    let mut clips = Vec::new();
    for (kind_name, anim_name) in &cfg.clips {
        let kind = ClipKind::ALL
            .into_iter()
            .find(|k| format!("{k:?}") == *kind_name)
            .ok_or_else(|| format!("unknown clip kind {kind_name:?}"))?;
        let anim = doc
            .animations()
            .find(|a| a.name() == Some(anim_name.as_str()))
            .ok_or_else(|| format!("no animation named {anim_name:?}"))?;
        let frames = sample_clip(&anim, kind, &skel, &rest_globals, &joints, get)?
            .into_iter()
            .map(|bones| {
                bones
                    .iter()
                    .chain([&Mat4::IDENTITY])
                    .zip(&bind_fix)
                    .map(|(m, fix)| to_game * *m * *fix * from_game)
                    .collect()
            })
            .collect();
        clips.push((kind, frames));
    }
    if skin.is_none() {
        // A still model stands in one pose: one frame where nothing moves.
        clips.push((ClipKind::Idle, vec![vec![Mat4::IDENTITY]]));
    }
    clips.sort_by_key(|(k, _)| *k as u8);

    let mut counts: HashMap<([u8; 3], bool), usize> = HashMap::new();
    for v in &verts_out {
        *counts
            .entry(([v.color[0], v.color[1], v.color[2]], v.team > 0))
            .or_default() += 1;
    }
    let mut palette: Vec<_> = counts.into_iter().map(|((c, t), n)| (c, n, t)).collect();
    palette.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    Ok(BakedModel {
        verts: verts_out,
        lods,
        height: cfg.height,
        bones: joints.len() as u32 + 1,
        clips,
        palette,
    })
}

/// A face's detail tag from its `COLOR_0` (see `ModelConfig::detail_tile`).
fn detail_tag(rgba: [f32; 4]) -> DetailTag {
    let fifteenth = |c: f32| (c.clamp(0.0, 1.0) * 15.0).round() as u8;
    DetailTag {
        class_a: fifteenth(rgba[0]),
        class_b: fifteenth(rgba[1]),
        mix: fifteenth(rgba[2]),
        tiles: 0,
    }
}

/// The metres of surface one whole texture atlas covers across, typically:
/// the median over the model's triangles, weighted by their area, of the
/// square root of a triangle's area against its area in texture space. The
/// median keeps the faces the baker shrinks (a plinth below the ground, which
/// is almost never seen) from deciding it.
fn atlas_metres(verts: &[Vert], indices: &[u32]) -> f32 {
    let mut ratios: Vec<(f32, f32)> = indices
        .chunks_exact(3)
        .filter_map(|t| {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| &verts[i as usize]);
            let area = (b.pos - a.pos).cross(c.pos - a.pos).length() * 0.5;
            let (e1, e2) = (b.uv - a.uv, c.uv - a.uv);
            let uv_area = (e1.x * e2.y - e1.y * e2.x).abs() * 0.5;
            (area > 1e-9 && uv_area > 1e-12).then(|| ((area / uv_area).sqrt(), area))
        })
        .collect();
    ratios.sort_by(|a, b| a.0.total_cmp(&b.0));
    let half: f32 = ratios.iter().map(|r| r.1).sum::<f32>() * 0.5;
    let mut seen = 0.0;
    for (ratio, area) in &ratios {
        seen += area;
        if seen >= half {
            return *ratio;
        }
    }
    0.0
}

/// The triangles of `indices` grouped into separate parts: sets joined by
/// shared positions (texture seams don't split a part).
fn parts(indices: &[u32], verts: &[ModelVertex]) -> Vec<Vec<u32>> {
    let mut slot: HashMap<[i16; 3], usize> = HashMap::new();
    let ids: Vec<usize> = indices
        .iter()
        .map(|&i| {
            let n = slot.len();
            *slot.entry(verts[i as usize].pos).or_insert(n)
        })
        .collect();
    let mut parent: Vec<usize> = (0..slot.len()).collect();
    fn root(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for t in ids.chunks(3) {
        let a = root(&mut parent, t[0]);
        for &v in &t[1..] {
            let b = root(&mut parent, v);
            parent[b] = a;
        }
    }
    let mut part_of: HashMap<usize, usize> = HashMap::new();
    let mut out: Vec<Vec<u32>> = Vec::new();
    for (t, tri) in ids.chunks(3).zip(indices.chunks(3)) {
        let r = root(&mut parent, t[0]);
        let k = *part_of.entry(r).or_insert_with(|| {
            out.push(Vec::new());
            out.len() - 1
        });
        out[k].extend_from_slice(tri);
    }
    out
}

fn quantise(
    v: &Vert,
    team: &[Vec3],
    tolerance: f32,
    team_hue: Option<(f32, f32, f32)>,
) -> ModelVertex {
    let q = |c: f32| {
        (c * POSITION_SCALE)
            .round()
            .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
    };
    let n = |c: f32| (c * 127.0).round().clamp(-127.0, 127.0) as i8;
    let unorm16 = |c: f32| (c.clamp(0.0, 1.0) * 65535.0).round() as u16;
    let color = Vec3::new(
        f32::from(v.color[0]),
        f32::from(v.color[1]),
        f32::from(v.color[2]),
    );
    let (hue, saturation) = hue_saturation(color / 255.0);
    let is_team = team.iter().any(|t| t.distance(color) <= tolerance)
        || team_hue.is_some_and(|(lo, hi, sat)| hue >= lo && hue <= hi && saturation >= sat);
    let (bones, weights) = quantise_weights(&v.bones);
    ModelVertex {
        pos: [q(v.pos.x), q(v.pos.y), q(v.pos.z)],
        team: if is_team { 255 } else { 0 },
        material: 0,
        bones,
        weights,
        normal: [
            n(v.normal.x),
            n(v.normal.y),
            n(v.normal.z),
            v.detail.classes_byte() as i8,
        ],
        tangent: [
            n(v.tangent.x),
            n(v.tangent.y),
            n(v.tangent.z),
            n(v.tangent.w),
        ],
        color: [v.color[0], v.color[1], v.color[2], v.detail.params_byte()],
        uv: [unorm16(v.uv.x), unorm16(v.uv.y)],
    }
}

/// Bones with less of a vertex than this are dropped: they barely move it,
/// and every bone a vertex follows costs the shader six texture reads.
const MIN_WEIGHT: f32 = 0.05;

/// A vertex's skin from its (joint, weight) pairs: the four heaviest
/// bones, heaviest first, weights normalised to sum to 1. Slots past the
/// last real bone have weight 0.
fn top_weights(influences: &[(u16, f32)]) -> [(u8, f32); BONES_PER_VERTEX] {
    let total: f32 = influences.iter().map(|p| p.1.max(0.0)).sum();
    let mut pairs: Vec<(u16, f32)> = influences
        .iter()
        .map(|&(j, w)| (j, w.max(0.0)))
        .filter(|p| p.1 > 0.0 && p.1 >= MIN_WEIGHT * total)
        .collect();
    pairs.sort_by(|a, b| b.1.total_cmp(&a.1));
    pairs.truncate(BONES_PER_VERTEX);
    let sum: f32 = pairs.iter().map(|p| p.1).sum();
    if sum <= 0.0 {
        // No weights at all: follow the first listed joint.
        return rigid_skin(influences.first().map_or(0, |p| p.0) as u8);
    }
    core::array::from_fn(|i| pairs.get(i).map_or((0, 0.0), |&(j, w)| (j as u8, w / sum)))
}

/// A skin that follows one bone completely.
fn rigid_skin(bone: u8) -> [(u8, f32); BONES_PER_VERTEX] {
    core::array::from_fn(|i| if i == 0 { (bone, 1.0) } else { (0, 0.0) })
}

/// Weights as bytes summing exactly to 255, so a vertex never shrinks
/// toward the model's origin. Bones too light to register are dropped.
fn quantise_weights(
    bones: &[(u8, f32); BONES_PER_VERTEX],
) -> ([u8; BONES_PER_VERTEX], [u8; BONES_PER_VERTEX]) {
    let mut b = [0u8; BONES_PER_VERTEX];
    let mut w = [0u8; BONES_PER_VERTEX];
    // The lighter bones round down and the heaviest takes the rest, so the
    // weights still run heaviest first (the shader stops at the first 0).
    for (i, &(bone, weight)) in bones.iter().enumerate().skip(1) {
        let q = (weight * 255.0) as u8;
        if q > 0 {
            b[i] = bone;
            w[i] = q;
        }
    }
    b[0] = bones[0].0;
    w[0] = 255 - w[1..].iter().map(|&x| u32::from(x)).sum::<u32>().min(255) as u8;
    (b, w)
}

/// Hue in degrees (0 to 360) and HSV saturation (0 to 1) of an RGB colour.
fn hue_saturation(c: Vec3) -> (f32, f32) {
    let max = c.max_element();
    let min = c.min_element();
    let d = max - min;
    if d <= 1e-6 {
        return (0.0, 0.0);
    }
    let h = if max == c.x {
        60.0 * ((c.y - c.z) / d).rem_euclid(6.0)
    } else if max == c.y {
        60.0 * ((c.z - c.x) / d + 2.0)
    } else {
        60.0 * ((c.x - c.y) / d + 4.0)
    };
    (h, d / max)
}

fn parse_hex(h: &str) -> Result<Vec3, String> {
    let h = h.trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).map_err(|_| format!("bad colour {h:?}"))?;
    if h.len() != 6 {
        return Err(format!("bad colour {h:?}"));
    }
    Ok(Vec3::new(
        (v >> 16) as f32,
        ((v >> 8) & 0xff) as f32,
        (v & 0xff) as f32,
    ))
}

/// One animation channel's keys.
enum Keys {
    Translation(Vec<Vec3>),
    Rotation(Vec<Quat>),
    Scale(Vec<Vec3>),
}

struct Channel {
    node: usize,
    times: Vec<f32>,
    keys: Keys,
    step: bool,
    cubic: bool,
}

impl Channel {
    /// Index of the key before `t` and how far toward the next.
    fn locate(&self, t: f32) -> (usize, usize, f32) {
        let last = self.times.len() - 1;
        if t <= self.times[0] {
            return (0, 0, 0.0);
        }
        if t >= self.times[last] {
            return (last, last, 0.0);
        }
        let k = self.times.partition_point(|&x| x <= t) - 1;
        let span = (self.times[k + 1] - self.times[k]).max(1e-9);
        let f = if self.step {
            0.0
        } else {
            (t - self.times[k]) / span
        };
        (k, k + 1, f)
    }

    /// Key value `k`, skipping cubic-spline tangents.
    fn key<T: Copy>(&self, values: &[T], k: usize) -> T {
        if self.cubic {
            values[k * 3 + 1]
        } else {
            values[k]
        }
    }
}

/// Bone matrices (rest pose to posed, in the source's space) for each frame.
fn sample_clip<'a>(
    anim: &gltf::Animation<'a>,
    kind: ClipKind,
    skel: &Skeleton,
    rest_globals: &[Mat4],
    joints: &[usize],
    get: impl Clone + Fn(gltf::Buffer<'a>) -> Option<&'a [u8]>,
) -> Result<Vec<Vec<Mat4>>, String> {
    use gltf::animation::util::ReadOutputs;
    let mut channels = Vec::new();
    let mut duration: f32 = 0.0;
    for ch in anim.channels() {
        let reader = ch.reader(get.clone());
        let times: Vec<f32> = reader
            .read_inputs()
            .ok_or("channel without times")?
            .collect();
        let keys = match reader.read_outputs().ok_or("channel without values")? {
            ReadOutputs::Translations(t) => Keys::Translation(t.map(Vec3::from).collect()),
            ReadOutputs::Rotations(r) => {
                Keys::Rotation(r.into_f32().map(Quat::from_array).collect())
            }
            ReadOutputs::Scales(s) => Keys::Scale(s.map(Vec3::from).collect()),
            ReadOutputs::MorphTargetWeights(_) => continue,
        };
        if times.is_empty() {
            continue;
        }
        duration = duration.max(*times.last().unwrap());
        let interp = ch.sampler().interpolation();
        channels.push(Channel {
            node: ch.target().node().index(),
            times,
            keys,
            step: interp == gltf::animation::Interpolation::Step,
            cubic: interp == gltf::animation::Interpolation::CubicSpline,
        });
    }
    // Looping clips skip the last frame (it equals the first); one-shot clips
    // end on it and hold.
    let frames = if kind.loops() {
        ((duration * SAMPLE_FPS).round() as usize).max(1)
    } else {
        (duration * SAMPLE_FPS).ceil() as usize + 1
    };
    let rest_inverse: Vec<Mat4> = joints.iter().map(|&n| rest_globals[n].inverse()).collect();
    let mut out = Vec::with_capacity(frames);
    for f in 0..frames {
        let t = (f as f32 / SAMPLE_FPS).min(duration);
        let mut local = skel.rest.clone();
        for ch in &channels {
            let (a, b, x) = ch.locate(t);
            let trs = &mut local[ch.node];
            match &ch.keys {
                Keys::Translation(v) => trs.t = ch.key(v, a).lerp(ch.key(v, b), x),
                Keys::Scale(v) => trs.s = ch.key(v, a).lerp(ch.key(v, b), x),
                Keys::Rotation(v) => {
                    trs.r = ch.key(v, a).normalize().slerp(ch.key(v, b).normalize(), x);
                }
            }
        }
        let globals = skel.globals(&local);
        out.push(
            joints
                .iter()
                .enumerate()
                .map(|(b, &n)| globals[n] * rest_inverse[b])
                .collect(),
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_bones_are_dropped_and_weights_sum_to_255() {
        let skin = top_weights(&[(3, 0.2), (7, 0.7), (9, 0.08), (2, 0.02)]);
        let (bones, weights) = quantise_weights(&skin);
        assert_eq!(bones, [7, 3, 9, 0]);
        assert_eq!(weights.iter().map(|&w| u32::from(w)).sum::<u32>(), 255);
        assert_eq!(weights[3], 0);
        let (_, even) = quantise_weights(&top_weights(&[(1, 0.5), (2, 0.5)]));
        assert!(even[0] >= even[1] && u32::from(even[0]) + u32::from(even[1]) == 255);
    }

    #[test]
    fn the_heaviest_four_of_eight_influences_are_kept() {
        let inf: Vec<(u16, f32)> = (0..8).map(|j| (j, f32::from(j) + 1.0)).collect();
        let (bones, _) = quantise_weights(&top_weights(&inf));
        assert_eq!(bones, [7, 6, 5, 4]);
    }

    #[test]
    fn a_vertex_without_weights_follows_its_first_joint() {
        let (bones, weights) = quantise_weights(&top_weights(&[(5, 0.0), (1, 0.0)]));
        assert_eq!((bones, weights), ([5, 0, 0, 0], [255, 0, 0, 0]));
    }

    #[test]
    fn a_colour_names_two_classes_and_a_mix() {
        let tag = detail_tag([3.0 / 15.0, 11.0 / 15.0, 0.5, 1.0]);
        assert_eq!(
            (tag.class_a, tag.class_b, tag.mix, tag.tiles),
            (3, 11, 8, 0)
        );
        assert_eq!(detail_tag([0.0; 4]), DetailTag::default());
        assert_eq!(detail_tag([2.0, -1.0, 9.0, 0.0]).class_a, 15);
    }

    #[test]
    fn the_atlas_size_is_the_median_not_the_plinth() {
        let quad = |size: f32, uv: f32, at: f32| -> Vec<Vert> {
            [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)]
                .iter()
                .map(|&(x, y)| Vert {
                    pos: Vec3::new(x * size, y * size, at),
                    normal: Vec3::Z,
                    tangent: Vec4::ZERO,
                    uv: Vec2::new(x * uv, y * uv),
                    color: [0; 3],
                    detail: DetailTag::default(),
                    bones: rigid_skin(0),
                })
                .collect()
        };
        // Two big faces covering 2 metres per 0.5 of atlas, and one tiny
        // shrunken face (the plinth) that would say 100.
        let mut verts = quad(2.0, 0.5, 0.0);
        verts.extend(quad(2.0, 0.5, 1.0));
        verts.extend(quad(1.0, 0.01, 2.0));
        let indices: Vec<u32> = (0..9).collect();
        let m = atlas_metres(&verts, &indices);
        assert!((m - 4.0).abs() < 1e-3, "{m}");
    }

    /// The repo's own CC0 alien bug (it was the Skitterlings' stand-in until
    /// the Brood's own models replaced it; it stays as the baker's test model),
    /// baked in memory with the budgets it had in `models.ron`.
    fn bake_alien_bug() -> (Vec<u8>, ModelTables, ModelConfig) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let opts = Options {
            config: root.join("assets-src/models.ron"),
            shared: PathBuf::from("/nonexistent"),
            repo_src: root.join("assets-src"),
            out: PathBuf::new(),
            report: false,
            check: false,
            detail: false,
        };
        let mut config: Config = ron::from_str(
            r#"(models: [(
                name: "Alien Bug",
                source: "repo:cc0/alien-bug-cdmir/alien-bug.glb",
                height: 0.7,
                team: [],
                team_hue: Some((0.0, 30.0, 0.5)),
                clips: {
                    "Idle": "Idle", "Run": "Run", "Attack": "Attack.000", "Attack2": "Attack.001",
                },
                lod_triangles: (1200, 500, 220, 90),
            )])"#,
        )
        .unwrap();
        let (bytes, tables) = bake_pack(&config, &opts).unwrap();
        (bytes, tables, config.models.remove(0))
    }

    #[test]
    fn the_alien_bug_bakes_within_its_budgets() {
        let (bytes, tables, cfg) = bake_alien_bug();
        let pack = gc_assets::Pack::parse(&bytes).unwrap();
        let models = gc_assets::model::Models::from_pack(&pack).unwrap();
        let model = &models.models[0];
        assert_eq!(model.name(), cfg.name);
        for (lod, budget) in cfg.lod_triangles.iter().enumerate() {
            let tris = model.lods[lod][1] as usize / 3;
            assert!(
                tris > 0 && tris <= *budget,
                "level {lod}: {tris} > {budget}"
            );
        }
        assert!(!tables.vertices.is_empty());
        for v in &tables.vertices {
            assert_eq!(v.weights.iter().map(|&w| u32::from(w)).sum::<u32>(), 255);
        }
        for kind in [
            ClipKind::Idle,
            ClipKind::Run,
            ClipKind::Attack,
            ClipKind::Attack2,
        ] {
            assert!(models.clip(0, kind).is_some(), "no {kind:?} clip");
        }
    }

    #[test]
    fn two_bakes_of_the_alien_bug_are_byte_identical() {
        let (a, _, _) = bake_alien_bug();
        let (b, _, _) = bake_alien_bug();
        assert!(a == b, "two bakes differ");
    }

    fn vert(pos: [i16; 3], uv: [u16; 2]) -> ModelVertex {
        ModelVertex {
            pos,
            uv,
            ..bytemuck::Zeroable::zeroed()
        }
    }

    #[test]
    fn separate_triangles_are_separate_parts() {
        let verts = [
            vert([0, 0, 0], [0, 0]),
            vert([10, 0, 0], [0, 0]),
            vert([0, 10, 0], [0, 0]),
            vert([100, 0, 0], [0, 0]),
            vert([110, 0, 0], [0, 0]),
            vert([100, 10, 0], [0, 0]),
        ];
        let got = parts(&[0, 1, 2, 3, 4, 5], &verts);
        assert_eq!(got, vec![vec![0, 1, 2], vec![3, 4, 5]]);
    }

    #[test]
    fn a_texture_seam_does_not_split_a_part() {
        // Vertices 3 and 4 repeat 1 and 2 with other uvs.
        let verts = [
            vert([0, 0, 0], [0, 0]),
            vert([10, 0, 0], [100, 0]),
            vert([0, 10, 0], [0, 100]),
            vert([10, 0, 0], [900, 0]),
            vert([0, 10, 0], [900, 900]),
            vert([10, 10, 0], [1000, 1000]),
        ];
        let got = parts(&[0, 1, 2, 3, 5, 4], &verts);
        assert_eq!(got, vec![vec![0, 1, 2, 3, 5, 4]]);
    }

    #[test]
    fn parts_keep_every_index() {
        let verts = [
            vert([0, 0, 0], [0, 0]),
            vert([10, 0, 0], [0, 0]),
            vert([0, 10, 0], [0, 0]),
            vert([10, 10, 0], [5, 5]),
            vert([100, 0, 0], [0, 0]),
            vert([110, 0, 0], [0, 0]),
            vert([100, 10, 0], [0, 0]),
        ];
        let indices = [0, 1, 2, 1, 3, 2, 4, 5, 6];
        let got = parts(&indices, &verts);
        assert_eq!(got.len(), 2);
        assert_eq!(got.iter().map(Vec::len).sum::<usize>(), indices.len());
        assert_eq!(got.concat(), indices);
    }
}
