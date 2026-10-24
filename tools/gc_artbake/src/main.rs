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
mod rig;
mod textures;

use std::collections::{BTreeMap, HashMap};
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
pub(crate) const SAMPLE_FPS: f32 = 12.0;
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
    /// Tagged moves several models take, by name (see [`MoveSet`]).
    #[serde(default)]
    move_sets: BTreeMap<String, MoveSet>,
}

/// Tagged moves from one glTF (a fight library) that models on different
/// skeletons take with `move_sets`, as if each named them in its own
/// `moves` with `moves_from`; each gets its own copy, on its own skeleton.
#[derive(Deserialize)]
struct MoveSet {
    from: String,
    moves: Vec<MoveConfig>,
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
    /// Tagged moves for the move picker (docs/33), beyond the plain clips.
    /// Only for a model posed from a shared skeleton; its moves are shared
    /// by every model on that skeleton.
    #[serde(default)]
    moves: Vec<MoveConfig>,
    /// A glTF holding the tagged moves' animations, when they are kept
    /// apart from the model (a fight library): its animations play on the
    /// model's joints by name, so it must carry the model's rig (see
    /// [`library_rig`]). Left out, the moves are the model's own animations.
    #[serde(default)]
    moves_from: Option<String>,
    /// Named [`MoveSet`]s the model takes too.
    #[serde(default)]
    move_sets: Vec<String>,
    /// Joints the model's own clips keep in its hands though they hang
    /// from elsewhere (a Concord pike or shield hangs from the chest, and
    /// the arms reach for it), each to the hand joint that holds it. In
    /// moves from a library, which lacks them, the hand then carries the
    /// joint as the model's Idle holds it, rather than leaving it at rest.
    #[serde(default)]
    held_by: BTreeMap<String, String>,
    /// What the model holds, named as in `gc_assets::anim::Weapon`
    /// ("TwoHanded" for a katana): the picker then prefers the skeleton's
    /// moves made for that weapon over the plain clips. Only for a model
    /// posed from a shared skeleton.
    #[serde(default)]
    weapon: Option<String>,
}

/// One tagged move: a glTF animation and what it is for. Tags are named as
/// in `gc_assets::anim` (`act: "Parry", weapon: "Spear", side: "Left"`).
#[derive(Deserialize)]
struct MoveConfig {
    /// The glTF animation's name.
    anim: String,
    act: String,
    #[serde(default = "any")]
    weapon: String,
    #[serde(default = "any")]
    side: String,
    /// The joints it plays on when layered.
    #[serde(default = "all_joints")]
    mask: String,
    /// Stored as changes from its first frame, to play on top of another.
    #[serde(default)]
    additive: bool,
    /// Left out, the act decides: standing and moving loop, the rest play
    /// once and hold. Written `loops: Some(true)`.
    #[serde(default)]
    loops: Option<bool>,
    /// For an attack, the seconds into the animation where its blow lands:
    /// the move is then retimed so its blow lands at
    /// `gc_assets::anim::BLOW_AT` of it, where the game's swing lands one.
    /// Both halves of a matched kill (`Kill`, `Killed`) must have one, so
    /// the two meet when played together over the same time.
    #[serde(default)]
    contact: Option<f32>,
    /// For a half of a matched kill (`Kill`, `Killed`), the name both
    /// halves share, which pairs them.
    #[serde(default)]
    pair: Option<String>,
    /// For the killer's half (`Kill`), how far apart the two men stand,
    /// origin to origin, in the glTF's units (as the model is before it is
    /// sized); stored in tiles.
    #[serde(default)]
    reach: Option<f32>,
}

fn any() -> String {
    "Any".into()
}

fn all_joints() -> String {
    "All".into()
}

/// The value of `all` whose name is `name`.
fn named<T: Copy + std::fmt::Debug>(all: &[T], name: &str, what: &str) -> Result<T, String> {
    all.iter()
        .copied()
        .find(|v| format!("{v:?}") == name)
        .ok_or_else(|| format!("unknown {what} {name:?}"))
}

impl MoveConfig {
    fn tags(&self) -> Result<rig::Tags, String> {
        use gc_assets::anim::{ADDITIVE, Act, LOOPS, Mask, Side, Weapon};
        let act = named(&Act::ALL, &self.act, "act")?;
        let kill = matches!(act, Act::Kill | Act::Killed);
        if self.contact.is_some() && !(kill || act == Act::Attack) {
            return Err(format!(
                "move {:?}: contact is only for attacks and kills",
                self.anim
            ));
        }
        if kill && self.contact.is_none() {
            // Without one the two halves of a kill would not meet.
            return Err(format!("move {:?}: a kill needs its contact", self.anim));
        }
        if kill && (self.loops == Some(true) || self.additive || self.mask != all_joints()) {
            return Err(format!(
                "move {:?}: a kill plays once, on the whole body",
                self.anim
            ));
        }
        if kill != self.pair.is_some() {
            return Err(format!(
                "move {:?}: a kill, and only a kill, names its pair",
                self.anim
            ));
        }
        let reach_ok = match (act == Act::Kill, self.reach) {
            (true, Some(r)) => r.is_finite() && r > 0.0,
            (killer, reach) => !killer && reach.is_none(),
        };
        if !reach_ok {
            return Err(format!(
                "move {:?}: the killer's half, and only it, has a reach above 0",
                self.anim
            ));
        }
        let loops = self
            .loops
            .unwrap_or(matches!(act, Act::Idle | Act::Walk | Act::Run | Act::Guard));
        Ok(rig::Tags {
            act,
            weapon: named(&Weapon::ALL, &self.weapon, "weapon")?,
            side: named(&Side::ALL, &self.side, "side")?,
            mask: named(&Mask::ALL, &self.mask, "mask")?,
            flags: if loops { LOOPS } else { 0 } | if self.additive { ADDITIVE } else { 0 },
        })
    }
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
    let mut anim = rig::AnimBuilder::default();
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
        let library = match &m.moves_from {
            Some(src) => Some(resolve(src).map_err(|e| format!("{}: {e}", m.name))?),
            None => None,
        };
        let sets = m
            .move_sets
            .iter()
            .map(|name| {
                let set = config
                    .move_sets
                    .get(name)
                    .ok_or_else(|| format!("no move set {name:?}"))?;
                Ok((resolve(&set.from)?, set.moves.as_slice()))
            })
            .collect::<Result<Vec<_>, String>>()
            .map_err(|e| format!("{}: {e}", m.name))?;
        let albedo_img = albedo.as_ref().map(|a| &a.0);
        let mut baked = bake_model(m, &path, albedo_img, library.as_deref(), &sets)
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
        baked.append_to(&mut tables, &mut anim, &m.name)?;
    }
    anim.finish(&mut tables.anim)?;
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
    /// to pose). Only made for a model not `posed` from a skeleton (and in
    /// tests, which compare the two).
    clips: Vec<(ClipKind, Vec<Vec<Mat4>>)>,
    /// The model's skeleton and its clips as joint poses, unless its joints
    /// cannot be stored that way (see `rig::poses`) or it has no skin.
    posed: Option<rig::Posed>,
    /// Most common colours and how many vertices have each.
    palette: Vec<([u8; 3], usize, bool)>,
}

impl BakedModel {
    fn append_to(
        &self,
        t: &mut ModelTables,
        anim: &mut rig::AnimBuilder,
        name: &str,
    ) -> Result<(), String> {
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
        let bone_clips = match &self.posed {
            Some(posed) => {
                let clips = anim.add(&mut t.anim, t.models.len() as u32, posed)?;
                t.clips.extend(clips);
                &[][..]
            }
            None => &self.clips[..],
        };
        for (kind, frames) in bone_clips {
            let first_row = (t.bones.len() / BONE_TEXELS_PER_ROW as usize) as u32;
            for frame in frames {
                let mut row = vec![[0u16; 4]; BONE_TEXELS_PER_ROW as usize];
                for (b, m) in frame.iter().enumerate() {
                    for r in 0..3 {
                        row[b * 3 + r] = m.row(r).to_array().map(f16_bits);
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
            clip_count: t.clips.len() as u32 - first_clip,
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
        m.posed.as_ref().map_or(m.clips.len(), |p| p.clips.len())
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
    names: Vec<String>,
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
        let names = doc
            .nodes()
            .map(|node| node.name().unwrap_or("").to_string())
            .collect();
        let mut order = Vec::with_capacity(n);
        let mut stack: Vec<usize> = (0..n).filter(|&i| parent[i].is_none()).collect();
        while let Some(i) = stack.pop() {
            order.push(i);
            stack.extend(doc.nodes().nth(i).unwrap().children().map(|c| c.index()));
        }
        Skeleton {
            parent,
            names,
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
    library: Option<&Path>,
    sets: &[(PathBuf, &[MoveConfig])],
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
    // Tangents orient a normal map; a model with only a colour map (the
    // creatures) has none, and the flat normal it gets needs no tangent.
    let normal_mapped = cfg.textures.as_ref().is_some_and(|t| t.normal.is_some());
    if normal_mapped && verts.iter().any(|v| v.tangent == Vec4::ZERO) {
        return Err("a normal-mapped model needs tangents (export them with the glTF)".into());
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

    // 5. Sample the clips: each joint's place in every frame.
    let mut clips = Vec::new();
    let mut sampled = Vec::new();
    for (kind_name, anim_name) in &cfg.clips {
        let kind = named(&ClipKind::ALL, kind_name, "clip kind")?;
        let anim = anim_named(doc, anim_name)?;
        let globals = sample_clip(&anim, Some, kind.loops(), None, &skel, &joints, get)?;
        sampled.push((kind, anim_name.clone(), globals));
    }
    // `clips` is a map, so put them in kind order for a repeatable pack.
    sampled.sort_by_key(|(k, _, _)| *k as u8);
    // Every model on a skin is posed from a shared skeleton when its joints
    // allow (`rig::poses`); otherwise, and for still models, it keeps bone
    // rows of its own. Either way a frame's bone matrix is
    // `to_game * global * rest_inverse * bind_fix * from_game`.
    let rest_inverse: Vec<Mat4> = joints.iter().map(|&n| rest_globals[n].inverse()).collect();
    let rig = rig::Rig::new(&joints, &skel.parent, &skel.names, &rest_globals);
    let pose_frames = |frames: &[Vec<Mat4>]| -> Option<Vec<Vec<rig::JointPose>>> {
        frames.iter().map(|g| rig::poses(&rig, g)).collect()
    };
    let posed_clips: Option<Vec<rig::PosedClip>> = skin.as_ref().and_then(|_| {
        sampled
            .iter()
            .map(|(kind, name, frames)| {
                Some(rig::PosedClip {
                    kind: Some(*kind),
                    name: name.clone(),
                    tags: rig::Tags::of_kind(*kind),
                    frames: pose_frames(frames)?,
                    pair: None,
                })
            })
            .collect()
    });
    // Tagged moves come from the model's own glTF (`moves` without
    // `moves_from`), or from a library on its rig with its nodes mapped onto
    // the model's joints: its own `moves`, then its move sets', grouped by
    // the glTF they come from so each is read once.
    let mut by_source: Vec<(Option<&Path>, Vec<&MoveConfig>)> = Vec::new();
    let all = std::iter::once((library, cfg.moves.as_slice()))
        .chain(sets.iter().map(|(p, m)| (Some(p.as_path()), *m)));
    for (from, moves) in all {
        match by_source.iter_mut().find(|(f, _)| *f == from) {
            Some((_, had)) => had.extend(moves),
            None => by_source.push((from, moves.iter().collect())),
        }
    }
    by_source.retain(|(_, moves)| !moves.is_empty());
    let tagged = !by_source.is_empty();
    let held = held_joints(cfg, &rig, &sampled)?;
    let posed_clips = match posed_clips {
        Some(mut posed) => {
            for (from, moves) in by_source {
                let lib = from
                    .map(|p| -> Result<_, String> {
                        let at = |e: String| format!("moves from {}: {e}", p.display());
                        let lib = load(p).map_err(at)?;
                        let lib_skel = Skeleton::new(&lib.doc);
                        let to_model = library_rig(&rig, &joints, &skel, &lib_skel).map_err(at)?;
                        if let Some(&(j, _, _)) = held
                            .iter()
                            .find(|(j, _, _)| to_model.values().any(|&n| n == joints[*j]))
                        {
                            return Err(at(format!(
                                "it moves {:?} itself, which the model's hand holds",
                                rig.names[j]
                            )));
                        }
                        Ok((lib, to_model))
                    })
                    .transpose()?;
                let (moves_src, to_model) = match &lib {
                    Some((lib, to_model)) => (lib, Some(to_model)),
                    None => (&src, None),
                };
                for m in moves {
                    let tags = m.tags()?;
                    let mut globals = sampled_move(m, tags, moves_src, to_model, &skel, &joints)?;
                    if to_model.is_some() {
                        for frame in &mut globals {
                            carry(frame, &held, &rig.parent);
                        }
                    }
                    posed.push(posed_move(m, tags, globals, pose_frames)?);
                }
            }
            Some(posed)
        }
        None if tagged => {
            return Err("tagged moves need a model posed from a shared skeleton".into());
        }
        None => None,
    };
    let weapon = match &cfg.weapon {
        Some(w) if posed_clips.is_none() => {
            return Err(format!(
                "weapon {w:?} needs a model posed from a shared skeleton"
            ));
        }
        Some(w) => named(&gc_assets::anim::Weapon::ALL, w, "weapon")?,
        None => gc_assets::anim::Weapon::Any,
    };
    let posed = posed_clips.map(|clips| rig::Posed {
        rig,
        pre: to_game,
        post: rest_inverse
            .iter()
            .zip(&bind_fix)
            .map(|(inv, fix)| *inv * *fix * from_game)
            .chain([Mat4::IDENTITY])
            .collect(),
        clips,
        weapon,
    });
    // Bone rows only for a model that cannot be posed (and in tests, which
    // check the poses against them).
    if posed.is_none() || cfg!(test) {
        for (kind, _, frames) in sampled {
            let frames = frames
                .into_iter()
                .map(|globals| {
                    globals
                        .iter()
                        .zip(&rest_inverse)
                        .map(|(g, inv)| *g * *inv)
                        .chain([Mat4::IDENTITY])
                        .zip(&bind_fix)
                        .map(|(m, fix)| to_game * m * *fix * from_game)
                        .collect()
                })
                .collect();
            clips.push((kind, frames));
        }
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
        posed,
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

/// The animation named `name`.
fn anim_named<'a>(doc: &'a gltf::Document, name: &str) -> Result<gltf::Animation<'a>, String> {
    doc.animations()
        .find(|a| a.name() == Some(name))
        .ok_or_else(|| format!("no animation named {name:?}"))
}

/// For a move library on a model's rig (`model`, whose joints are the nodes
/// `joints` of `skel`): each library node that is one of its joints, by
/// name, to that joint's node. The joints the two share must hang from one
/// another and rest alike in both, as on models sharing a skeleton
/// (`rig::Rig`), or the library's moves would not fit the model. Beyond
/// them the model may have joints the library lacks (a weapon, a floating
/// crystal), which hold their rest, and the library joints the model lacks
/// (fingers), which are left out, but none that a shared joint hangs from.
fn library_rig(
    model: &rig::Rig,
    joints: &[usize],
    skel: &Skeleton,
    lib: &Skeleton,
) -> Result<HashMap<usize, usize>, String> {
    let mut by_name: HashMap<&str, usize> = HashMap::new();
    for (i, n) in lib.names.iter().enumerate() {
        if by_name.insert(n.as_str(), i).is_some() && model.names.contains(n) {
            return Err(format!("two nodes are named {n:?}"));
        }
    }
    // Per model joint, the library's node of that name.
    let theirs: Vec<Option<usize>> = model
        .names
        .iter()
        .map(|n| by_name.get(n.as_str()).copied())
        .collect();
    let shared: Vec<usize> = (0..joints.len()).filter(|&j| theirs[j].is_some()).collect();
    if shared.is_empty() {
        return Err("it has none of the model's joints".into());
    }
    for &j in &shared {
        let up = model.parent[j].map(|p| theirs[p].ok_or(p));
        match up {
            Some(Err(p)) => {
                return Err(format!(
                    "the model's {:?} hangs from {:?}, which it lacks",
                    model.names[j], model.names[p]
                ));
            }
            Some(Ok(p)) if lib.parent[theirs[j].unwrap()] != Some(p) => {
                return Err(format!(
                    "its {:?} hangs from another joint than the model's",
                    model.names[j]
                ));
            }
            _ => {}
        }
    }
    // The library's channels replace the model's joints' own (local) rest,
    // so those must match too, not only where the joints end up: a root
    // under an armature of another scale would otherwise move wrongly.
    let near = |a: &Trs, b: &Trs| {
        a.t.abs_diff_eq(b.t, 1e-4) && a.r.dot(b.r).abs() > 1.0 - 1e-6 && a.s.abs_diff_eq(b.s, 1e-4)
    };
    if let Some(&j) = shared
        .iter()
        .find(|&&j| !near(&lib.rest[theirs[j].unwrap()], &skel.rest[joints[j]]))
    {
        return Err(format!(
            "its {:?} rests otherwise than the model's under its parent",
            model.names[j]
        ));
    }
    let sub = |nodes: &[usize], s: &Skeleton| {
        rig::Rig::new(nodes, &s.parent, &s.names, &s.globals(&s.rest))
    };
    let lib_nodes: Vec<usize> = shared.iter().map(|&j| theirs[j].unwrap()).collect();
    let model_nodes: Vec<usize> = shared.iter().map(|&j| joints[j]).collect();
    if sub(&lib_nodes, lib) != sub(&model_nodes, skel) {
        return Err("its rig is not the model's (parents or rest pose differ)".into());
    }
    Ok(lib_nodes.into_iter().zip(model_nodes).collect())
}

/// Each joint the model's hands hold (`held_by`), its hand and where the
/// hand holds it in the first frame of the model's Idle.
fn held_joints(
    cfg: &ModelConfig,
    rig: &rig::Rig,
    sampled: &[(ClipKind, String, Vec<Vec<Mat4>>)],
) -> Result<Vec<(usize, usize, Mat4)>, String> {
    let at = |name: &str| {
        rig.names
            .iter()
            .position(|n| n == name)
            .ok_or_else(|| format!("held_by: no joint {name:?}"))
    };
    cfg.held_by
        .iter()
        .map(|(joint, hand)| {
            let (j, h) = (at(joint)?, at(hand)?);
            if hangs_from(&rig.parent, h, j) {
                return Err(format!("held_by: {hand:?} hangs from {joint:?}"));
            }
            let idle = sampled
                .iter()
                .find(|(k, _, _)| *k == ClipKind::Idle)
                .ok_or("held_by needs an Idle clip")?;
            let frame = &idle.2[0];
            Ok((j, h, frame[h].inverse() * frame[j]))
        })
        .collect()
}

/// Whether joint `k` is joint `j` or hangs from it.
fn hangs_from(parent: &[Option<usize>], mut k: usize, j: usize) -> bool {
    loop {
        if k == j {
            return true;
        }
        match parent[k] {
            Some(p) => k = p,
            None => return false,
        }
    }
}

/// Move each held joint, and the joints hanging from it, with its hand.
fn carry(frame: &mut [Mat4], held: &[(usize, usize, Mat4)], parent: &[Option<usize>]) {
    for &(j, hand, grip) in held {
        let change = frame[hand] * grip * frame[j].inverse();
        for (k, m) in frame.iter_mut().enumerate() {
            if hangs_from(parent, k, j) {
                *m = change * *m;
            }
        }
    }
}

/// The joints' globals in each frame of tagged move `m` from `src`
/// (`to_model` maps a library's nodes onto the model's).
fn sampled_move(
    m: &MoveConfig,
    tags: rig::Tags,
    src: &Source,
    to_model: Option<&HashMap<usize, usize>>,
    skel: &Skeleton,
    joints: &[usize],
) -> Result<Vec<Vec<Mat4>>, String> {
    let loops = tags.flags & gc_assets::anim::LOOPS != 0;
    let anim = anim_named(&src.doc, &m.anim)?;
    // A kill plays as made (its pair records where it meets); an attack is
    // retimed so its blow lands at BLOW_AT.
    let kill = matches!(
        tags.act,
        gc_assets::anim::Act::Kill | gc_assets::anim::Act::Killed
    );
    let retime = if kill { None } else { m.contact };
    let node = |n: usize| to_model.map_or(Some(n), |m| m.get(&n).copied());
    let get = |b: gltf::Buffer<'_>| src.buffers.get(b.index()).map(Vec::as_slice);
    sample_clip(&anim, node, loops, retime, skel, joints, get)
}

/// Tagged move `m`, sampled as `globals`, posed on the model's joints.
fn posed_move(
    m: &MoveConfig,
    tags: rig::Tags,
    globals: Vec<Vec<Mat4>>,
    pose_frames: impl Fn(&[Vec<Mat4>]) -> Option<Vec<Vec<rig::JointPose>>>,
) -> Result<rig::PosedClip, String> {
    let pair = match (&m.pair, m.contact) {
        (Some(name), Some(contact)) => {
            let share = contact * SAMPLE_FPS / (globals.len() - 1).max(1) as f32;
            if !(share > 0.0 && share < 1.0) {
                return Err(format!(
                    "move {:?}: contact {contact} s is outside the animation",
                    m.anim
                ));
            }
            Some(rig::PairHalf {
                name: name.clone(),
                reach: m.reach,
                contact: share,
            })
        }
        _ => None,
    };
    let mut frames = pose_frames(&globals)
        .ok_or_else(|| format!("move {:?} squashes or shears a joint", m.anim))?;
    if m.additive {
        rig::make_additive(&mut frames);
    }
    Ok(rig::PosedClip {
        kind: None,
        name: m.anim.clone(),
        tags,
        frames,
        pair,
    })
}

/// Each joint's global matrix (in the source's space) for each frame.
/// `node` gives the skeleton's node an animated node stands for (none: the
/// channel is skipped). With `contact` (seconds into the animation), time
/// is stretched on either side of it so it falls at
/// [`BLOW_AT`](gc_assets::anim::BLOW_AT) of the clip.
fn sample_clip<'a>(
    anim: &gltf::Animation<'a>,
    node: impl Fn(usize) -> Option<usize>,
    loops: bool,
    contact: Option<f32>,
    skel: &Skeleton,
    joints: &[usize],
    get: impl Clone + Fn(gltf::Buffer<'a>) -> Option<&'a [u8]>,
) -> Result<Vec<Vec<Mat4>>, String> {
    use gltf::animation::util::ReadOutputs;
    let mut channels = Vec::new();
    let mut duration: f32 = 0.0;
    for ch in anim.channels() {
        let Some(target) = node(ch.target().node().index()) else {
            continue;
        };
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
            node: target,
            times,
            keys,
            step: interp == gltf::animation::Interpolation::Step,
            cubic: interp == gltf::animation::Interpolation::CubicSpline,
        });
    }
    // Looping clips skip the last frame (it equals the first); one-shot clips
    // end on it and hold.
    let frames = if loops {
        ((duration * SAMPLE_FPS).round() as usize).max(1)
    } else {
        (duration * SAMPLE_FPS).ceil() as usize + 1
    };
    if let Some(c) = contact
        && !(c > 0.0 && c < duration)
    {
        return Err(format!(
            "contact {c} s is outside the animation ({duration} s)"
        ));
    }
    // A frame's time in the animation. Retimed, frame `f` is the share `u`
    // of the clip the game plays it at (`MoveClip::at`), and the share
    // BLOW_AT falls on the contact.
    let time_of = |f: usize| match contact {
        Some(c) => {
            use gc_assets::anim::BLOW_AT;
            let u = f as f32 / if loops { frames } else { frames - 1 } as f32;
            if u <= BLOW_AT {
                u / BLOW_AT * c
            } else {
                c + (u - BLOW_AT) / (1.0 - BLOW_AT) * (duration - c)
            }
        }
        None => (f as f32 / SAMPLE_FPS).min(duration),
    };
    let mut out = Vec::with_capacity(frames);
    for f in 0..frames {
        let t = time_of(f);
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
        out.push(joints.iter().map(|&n| globals[n]).collect());
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
        let (bytes, tables, cfg) = bake_bug_with("[]");
        (bytes.unwrap(), tables, cfg)
    }

    /// The Alien Bug's pack with `moves` (a RON list) tagged on it: the
    /// pack's bytes, or why it would not bake.
    fn bake_bug_with(moves: &str) -> (Result<Vec<u8>, String>, ModelTables, ModelConfig) {
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
        config.models[0].moves = ron::from_str(moves).unwrap();
        let (bytes, tables) = match bake_pack(&config, &opts) {
            Ok((bytes, tables)) => (Ok(bytes), tables),
            Err(e) => (Err(e), ModelTables::default()),
        };
        (bytes, tables, config.models.remove(0))
    }

    #[test]
    fn the_halves_of_a_kill_are_paired_and_played_as_made() {
        use gc_assets::anim::Act;
        let half = |act: &str, extra: &str| {
            format!(
                r#"(anim: "Attack.000", act: "{act}", contact: Some(0.2), pair: Some("Bite"){extra})"#
            )
        };
        let both = format!(
            r#"[{}, {}, (anim: "Attack.000", act: "Hit")]"#,
            half("Kill", ", reach: Some(2.0)"),
            half("Killed", "")
        );
        let (bytes, _, _) = bake_bug_with(&both);
        let pack = gc_assets::Pack::parse(bytes.as_ref().unwrap()).unwrap();
        let models = gc_assets::model::Models::from_pack(&pack).unwrap();
        let anim = &models.anim;
        assert_eq!(anim.pairs.len(), 1);
        let p = anim.pairs[0];
        assert_eq!(anim.moves[p.kill as usize].act(), Act::Kill);
        assert_eq!(anim.moves[p.killed as usize].act(), Act::Killed);
        // Reach stays in the skeleton's space, which the game sizes with
        // the killer; a kill is not retimed, so its frames are those of an
        // untimed move on the same animation (stored once), and it meets
        // where its contact says.
        assert_eq!(p.reach, 2.0);
        let kill = anim.moves[p.kill as usize];
        let hit = anim.moves.iter().find(|m| m.act() == Act::Hit).unwrap();
        assert_eq!((kill.first_row, kill.frames), (hit.first_row, hit.frames));
        let share = 0.2 * SAMPLE_FPS / (kill.frames - 1) as f32;
        assert!((p.contact - share).abs() < 1e-6, "{} vs {share}", p.contact);

        // A pack without kills has no pairs table at all.
        let (plain, _, _) = bake_bug_with("[]");
        let plain = plain.unwrap();
        let plain = gc_assets::Pack::parse(&plain).unwrap();
        assert!(plain.get_path(gc_assets::anim::PAIRS_PATH).is_none());

        let err = |moves: String| {
            let got = bake_bug_with(&moves).0;
            assert!(got.is_err(), "{moves} baked");
            got.unwrap_err()
        };
        assert!(err(format!("[{}]", half("Kill", ", reach: Some(2.0)"))).contains("lacks a half"));
        let long = r#"(anim: "Idle", act: "Killed", contact: Some(0.2), pair: Some("Bite"))"#;
        let e = err(format!("[{}, {long}]", half("Kill", ", reach: Some(2.0)")));
        assert!(e.contains("differ in length"), "{e}");
        let late = r#"(anim: "Attack.000", act: "Killed", contact: Some(0.3), pair: Some("Bite"))"#;
        let e = err(format!("[{}, {late}]", half("Kill", ", reach: Some(2.0)")));
        assert!(e.contains("differ in contact"), "{e}");
        assert!(err(format!("[{}]", half("Kill", ""))).contains("reach"));
        assert!(err(format!("[{}]", half("Killed", ", reach: Some(1.0)"))).contains("reach"));
        let unpaired =
            r#"[(anim: "Attack.000", act: "Kill", contact: Some(0.2), reach: Some(1.0))]"#;
        assert!(err(unpaired.into()).contains("names its pair"));
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
    fn the_alien_bug_is_posed_from_a_skeleton_exactly_as_its_bone_rows() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (_, _, cfg) = bake_alien_bug();
        let path = root.join("assets-src/cc0/alien-bug-cdmir/alien-bug.glb");
        let baked = bake_model(&cfg, &path, None, None, &[]).unwrap();
        let posed = baked
            .posed
            .as_ref()
            .expect("the bug's joints store as poses");
        assert_eq!(posed.clips.len(), baked.clips.len());
        for (p, (kind, frames)) in posed.clips.iter().zip(&baked.clips) {
            assert_eq!(p.kind, Some(*kind));
            assert_eq!(p.frames.len(), frames.len());
            for (pose, bones) in p.frames.iter().zip(frames) {
                for (a, b) in posed.matrices(pose).iter().zip(bones) {
                    assert!(a.abs_diff_eq(*b, 1e-4), "{kind:?}: {a} against {b}");
                }
            }
        }
        // The pack keeps the poses, as half floats, not bone rows.
        let (_, tables, _) = bake_alien_bug();
        assert_eq!(tables.anim.fits.len(), 1);
        assert!(tables.bones.is_empty());
        let skeleton = &tables.anim.skeletons[0];
        let joints = skeleton.joints as usize;
        for (clip, (_, frames)) in tables.clips.iter().zip(&baked.clips) {
            for (f, bones) in frames.iter().enumerate() {
                let at = skeleton.first_pose as usize + (clip.first_row as usize + f) * joints;
                let pose: Vec<rig::JointPose> = tables.anim.frames[at..at + joints]
                    .iter()
                    .map(rig::JointPose::unpacked)
                    .collect();
                for (a, b) in posed.matrices(&pose).iter().zip(bones) {
                    assert!(a.abs_diff_eq(*b, 2e-3), "{a} against {b}");
                }
            }
        }
    }

    #[test]
    fn moves_from_a_library_on_the_same_rig_match_the_models_own() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = root.join("assets-src/cc0/alien-bug-cdmir/alien-bug.glb");
        let (_, _, mut cfg) = bake_alien_bug();
        cfg.moves = ron::from_str(r#"[(anim: "Attack.001", act: "Parry")]"#).unwrap();
        let own = bake_model(&cfg, &path, None, None, &[]).unwrap();
        let from = bake_model(&cfg, &path, None, Some(&path), &[]).unwrap();
        let (own, from) = (own.posed.unwrap(), from.posed.unwrap());
        let (a, b) = (own.clips.last().unwrap(), from.clips.last().unwrap());
        assert_eq!(a.name, b.name);
        assert_eq!(a.frames.len(), b.frames.len());
        for (pa, pb) in a.frames.iter().zip(&b.frames) {
            for (x, y) in own.matrices(pa).iter().zip(from.matrices(pb)) {
                assert!(x.abs_diff_eq(y, 1e-6));
            }
        }
    }

    /// The repo's CC0 Alien Bug: its source, nodes, skin joints and rig.
    fn bug_rig() -> (Source, Skeleton, Vec<usize>, rig::Rig) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let src = load(&root.join("assets-src/cc0/alien-bug-cdmir/alien-bug.glb")).unwrap();
        let skel = Skeleton::new(&src.doc);
        let skin = src.doc.skins().next().unwrap();
        let joints: Vec<usize> = skin.joints().map(|j| j.index()).collect();
        let rig = rig::Rig::new(
            &joints,
            &skel.parent,
            &skel.names,
            &skel.globals(&skel.rest),
        );
        (src, skel, joints, rig)
    }

    #[test]
    fn an_attack_is_retimed_so_its_blow_lands_where_the_swing_lands_one() {
        use gc_assets::anim::BLOW_AT;
        let (src, skel, joints, _) = bug_rig();
        let get = |b: gltf::Buffer<'_>| src.buffers.get(b.index()).map(Vec::as_slice);
        let anim = anim_named(&src.doc, "Attack.000").unwrap();
        let sample = |c| sample_clip(&anim, Some, false, c, &skel, &joints, get).unwrap();
        let plain = sample(None);
        let n = plain.len();
        // Put the blow on the plain clip's frame 2: retimed, the frame the
        // game plays at BLOW_AT of the move shows it, and the ends stay.
        let at = (BLOW_AT * (n - 1) as f32).floor() as usize;
        let contact = 2.0 / SAMPLE_FPS * BLOW_AT * (n - 1) as f32 / at as f32;
        let moved = sample(Some(contact));
        let near = |a: &[Mat4], b: &[Mat4]| a.iter().zip(b).all(|(x, y)| x.abs_diff_eq(*y, 1e-4));
        assert_eq!(moved.len(), n);
        assert!(near(&moved[at], &plain[2]));
        assert!(near(&moved[0], &plain[0]) && near(&moved[n - 1], &plain[n - 1]));
        let late = sample_clip(&anim, Some, false, Some(100.0), &skel, &joints, get);
        assert!(late.is_err());
    }

    #[test]
    fn contacts_go_on_attacks_and_on_both_halves_of_a_kill() {
        use gc_assets::anim::Act;
        let tags = |text: &str| {
            ron::from_str::<MoveConfig>(text)
                .unwrap()
                .tags()
                .map(|t| t.act)
        };
        assert_eq!(
            tags(r#"(anim: "A", act: "Attack", contact: Some(0.3))"#),
            Ok(Act::Attack)
        );
        assert_eq!(tags(r#"(anim: "A", act: "Attack")"#), Ok(Act::Attack));
        assert_eq!(
            tags(
                r#"(anim: "K", act: "Kill", contact: Some(0.5), pair: Some("P"), reach: Some(1.0))"#
            ),
            Ok(Act::Kill)
        );
        assert_eq!(
            tags(r#"(anim: "V", act: "Killed", contact: Some(0.5), pair: Some("P"))"#),
            Ok(Act::Killed)
        );
        assert!(
            tags(r#"(anim: "V", act: "Killed")"#)
                .unwrap_err()
                .contains("needs its contact")
        );
        assert!(
            tags(r#"(anim: "V", act: "Killed", contact: Some(0.5), additive: true)"#)
                .unwrap_err()
                .contains("whole body")
        );
        assert!(
            tags(r#"(anim: "P", act: "Parry", contact: Some(0.2))"#)
                .unwrap_err()
                .contains("only for")
        );
    }

    #[test]
    fn a_library_on_another_rig_is_refused_but_one_lacking_a_loose_joint_is_not() {
        let (_, skel, joints, rig) = bug_rig();
        let to_model = library_rig(&rig, &joints, &skel, &skel).unwrap();
        assert!(joints.iter().all(|j| to_model[j] == *j));
        let mut turned = Skeleton::new(&bug_rig().0.doc);
        turned.rest[joints[1]].r = Quat::from_rotation_y(0.3) * turned.rest[joints[1]].r;
        assert!(
            library_rig(&rig, &joints, &skel, &turned)
                .unwrap_err()
                .contains("rests otherwise")
        );
        // A joint nothing hangs from (a weapon's) may be the model's alone:
        // it holds its rest.
        let leaf = (0..joints.len())
            .find(|&j| !rig.parent.contains(&Some(j)))
            .unwrap();
        let mut loose = Skeleton::new(&bug_rig().0.doc);
        loose.names[joints[leaf]] = "Elsewhere".into();
        let to_model = library_rig(&rig, &joints, &skel, &loose).unwrap();
        assert_eq!(to_model.len(), joints.len() - 1);
        assert!(!to_model.contains_key(&joints[leaf]));
        // But not one the others hang from.
        let mut rootless = Skeleton::new(&bug_rig().0.doc);
        let root = rig.parent.iter().position(Option::is_none).unwrap();
        rootless.names[joints[root]] = "Elsewhere".into();
        assert!(
            library_rig(&rig, &joints, &skel, &rootless)
                .unwrap_err()
                .contains("which it lacks")
        );
        let mut twice = Skeleton::new(&bug_rig().0.doc);
        twice.names[joints[3]] = twice.names[joints[2]].clone();
        assert!(
            library_rig(&rig, &joints, &skel, &twice)
                .unwrap_err()
                .contains("two nodes")
        );
    }

    #[test]
    fn a_held_weapon_moves_with_its_hand_and_takes_its_grip_along() {
        // Chest, hand on it, a weapon hanging from the chest, a grip on it.
        let parent = [None, Some(0), Some(0), Some(2)];
        let at = |x: f32, y: f32| Mat4::from_translation(Vec3::new(x, y, 0.0));
        let grip = at(0.0, -0.1);
        let swing = Mat4::from_rotation_z(0.8);
        let mut frame = [
            Mat4::IDENTITY,
            at(1.0, 2.0) * swing,
            at(0.0, 1.0),
            at(0.0, 1.3),
        ];
        let (weapon, held_grip) = (frame[2], frame[3]);
        carry(&mut frame, &[(2, 1, grip)], &parent);
        assert!(frame[2].abs_diff_eq(frame[1] * grip, 1e-6));
        // The grip keeps its place on the weapon; the hand and chest stay.
        let on_weapon = weapon.inverse() * held_grip;
        assert!(frame[3].abs_diff_eq(frame[2] * on_weapon, 1e-5));
        assert!(frame[0] == Mat4::IDENTITY && frame[1] == at(1.0, 2.0) * swing);
        assert!(hangs_from(&parent, 3, 0) && !hangs_from(&parent, 1, 2));
    }

    #[test]
    fn tagged_moves_are_stored_for_the_picker_and_additive_ones_as_changes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = root.join("assets-src/cc0/alien-bug-cdmir/alien-bug.glb");
        let (_, _, mut cfg) = bake_alien_bug();
        cfg.moves = ron::from_str(
            r#"[
                (anim: "Attack.001", act: "Parry", side: "Left"),
                (anim: "Attack.000", act: "Flinch", mask: "Upper", additive: true),
            ]"#,
        )
        .unwrap();
        let baked = bake_model(&cfg, &path, None, None, &[]).unwrap();
        let mut t = ModelTables::default();
        let mut anim = rig::AnimBuilder::default();
        baked.append_to(&mut t, &mut anim, "Alien Bug").unwrap();
        anim.finish(&mut t.anim).unwrap();
        // The plain clips are still the model's four; the tagged moves are
        // only in the skeleton's moves.
        assert_eq!(t.clips.len(), 4);
        assert_eq!(t.anim.moves.len(), 6);
        let parry = t
            .anim
            .moves
            .iter()
            .find(|m| m.act == gc_assets::anim::Act::Parry as u8)
            .unwrap();
        assert_eq!(parry.kind, gc_assets::anim::NO_KIND);
        assert_eq!(parry.side, gc_assets::anim::Side::Left as u8);
        assert_eq!(parry.flags, 0);
        let flinch = t
            .anim
            .moves
            .iter()
            .find(|m| m.act == gc_assets::anim::Act::Flinch as u8)
            .unwrap();
        assert_eq!(flinch.flags, gc_assets::anim::ADDITIVE);
        assert_eq!(flinch.mask, gc_assets::anim::Mask::Upper as u8);
        // An additive move's first frame changes nothing.
        let joints = t.anim.skeletons[0].joints as usize;
        let first = flinch.first_row as usize * joints;
        for p in &t.anim.frames[first..first + joints] {
            let p = rig::JointPose::unpacked(p);
            assert!(p.r.abs_diff_eq(glam::Quat::IDENTITY, 1e-3), "{p:?}");
            assert!(p.t.length() < 1e-3 && (p.s - 1.0).abs() < 1e-3, "{p:?}");
        }

        cfg.moves[0].act = "Somersault".into();
        let err = bake_model(&cfg, &path, None, None, &[]).err().unwrap();
        assert!(err.contains("unknown act"), "{err}");
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
