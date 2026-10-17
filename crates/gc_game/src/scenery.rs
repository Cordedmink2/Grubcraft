//! Scenery on the field (docs/20-ambushes.md, "Reading it on screen"):
//! trees, bushes, grass and rocks, each a named 3D model from the scenery
//! pack, placed by the map's land, cover, thicket, heights and sites. Woods
//! get a tree on about half their tiles, thicket a dark fir on every tile
//! with undergrowth, tall grass a few clumps per tile, and open ground a
//! sparse sprinkling of grass, bushes and rocks that thins out near starts
//! and sites. Built once when the map loads, in square chunks, from a hash
//! of the seed and the tile, so it is the same on every machine; purely
//! visual, the simulation never reads it.
//!
//! Each frame the chunks near the camera are culled, given a level of detail
//! by distance, and their instances sorted into one batch per model and
//! level for the renderer ([`Scenery::frame`]).

use gc_assets::scenery::{Family, LODS};
use gc_assets::{Aligned, Pack};
use gc_map::def::Land;
use gc_map::{Kind as Site, Map};
use gc_render::{Cull, MAX_SCENERY_INSTANCES, SceneryBatch, SceneryInstance};
use glam::{Vec2, Vec3};

/// The scenery models, baked by `tools/gc_scenebake` and embedded in the
/// executable.
static PACK: &Aligned<[u8]> = &Aligned(*include_bytes!("../../../assets/scenery.pak"));

/// Load the embedded scenery pack onto the GPU and index its models by
/// name. An error means the committed pack is broken: the game runs
/// without scenery.
pub fn load(renderer: &mut gc_render::Renderer) -> Result<SceneryModels, String> {
    let pack = Pack::parse(&PACK.0).map_err(|e| format!("embedded scenery pack: {e}"))?;
    let scenery = gc_assets::scenery::Scenery::from_pack(&pack)
        .map_err(|e| format!("embedded scenery pack: {e}"))?;
    renderer.set_scenery(&scenery)?;
    Ok(SceneryModels::from_pack(&scenery))
}

/// At most this many gathering spots are drawn (a big map has a few
/// hundred), so they never crowd the scenery out of the buffer.
const MAX_NODES: usize = 4096;

/// Tiles per chunk side.
const CHUNK: u32 = 16;
/// Half a chunk's diagonal, in tiles.
const CHUNK_RADIUS: f32 = CHUNK as f32 * 0.71;
/// Share of wood tiles with a tree on them, percent.
const TREE_PERCENT: u32 = 55;
/// Chunk-local positions are in this many steps a tile.
const STEPS: f32 = 2048.0;

/// Trees are drawn this far from the camera's ground point (tiles), and
/// further as it pulls back; bushes and big rocks a bit less; grass and
/// pebbles only nearer.
const REACH_NEAR: [f32; 3] = [70.0, 55.0, 45.0];
const REACH_PER_ZOOM: [f32; 3] = [1.6, 1.1, 0.8];
/// Distance from the eye (tiles) where each class drops a level of detail.
const LOD_AT: [[f32; LODS - 1]; 3] = [
    [45.0, 100.0, 190.0],
    [30.0, 70.0, 140.0],
    [18.0, 40.0, 80.0],
];

/// The one list of models the placement knows: each one's name in the pack,
/// family (rocks tint less), class (0 trees, 1 bushes and big rocks, 2 grass
/// and pebbles: which reach and level-of-detail distances it follows) and
/// whether it casts shadows. Sizes come from the pack's own records.
macro_rules! catalogue {
    ($($variant:ident $name:literal $family:ident $class:literal $shadow:literal,)*) => {
        /// Every model the placement knows by name.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(usize)]
        pub enum Model {
            $($variant,)*
        }

        impl Model {
            pub const COUNT: usize = [$($name),*].len();
            const ALL: [Model; Model::COUNT] = [$(Model::$variant,)*];
            const SPECS: [Spec; Model::COUNT] = [$(Spec {
                name: $name,
                family: Family::$family,
                class: $class,
                shadow: $shadow,
            },)*];
        }
    };
}

struct Spec {
    name: &'static str,
    family: Family,
    class: u8,
    shadow: bool,
}

catalogue! {
    OakA "oak-a" Tree 0 true,
    OakB "oak-b" Tree 0 true,
    PineA "pine-a" Tree 0 true,
    PineB "pine-b" Tree 0 true,
    ThicketFir "thicket-fir" Tree 0 true,
    Birch "birch" Tree 0 true,
    Willow "willow" Tree 0 true,
    Snag "snag" Tree 0 true,
    CragPine "crag-pine" Tree 0 true,
    BoneSpire "bone-spire" Tree 0 true,
    LeafyBush "leafy-bush" Bush 1 false,
    Fern "fern" Bush 1 false,
    Heather "heather" Bush 1 false,
    ThornScrub "thorn-scrub" Bush 1 false,
    Glowcap "glowcap" Bush 1 false,
    GrassClump "grass-clump" Grass 2 false,
    TallGrass "tall-grass" Grass 2 false,
    Reeds "reeds" Grass 2 false,
    DryGrass "dry-grass" Grass 2 false,
    Wildflowers "wildflowers" Grass 2 false,
    Pebbles "pebbles" Rock 2 false,
    Boulder "boulder" Rock 1 true,
    MossyBoulder "mossy-boulder" Rock 1 true,
    Crag "crag" Rock 1 true,
    Slab "slab" Rock 1 true,
    BoneRibs "bone-ribs" Rock 1 true,
    GrubSeep "grub-seep" Rock 1 true,
    MarrowVein "marrow-vein" Rock 1 true,
}

impl Model {
    fn spec(self) -> &'static Spec {
        &Model::SPECS[self as usize]
    }
}

/// What the frame needs to know about one model of the pack.
#[derive(Clone, Copy, Debug)]
struct Info {
    class: u8,
    shadow: bool,
    rock: bool,
    height: f32,
    radius: f32,
}

/// Where each named model sits in the scenery pack (`None`: the pack
/// lacks it, and nothing of that name is placed).
#[derive(Clone, Debug, Default)]
pub struct SceneryModels {
    ids: [Option<u16>; Model::COUNT],
    /// Per model of the pack.
    info: Vec<Info>,
}

impl SceneryModels {
    /// The index for a loaded pack, with its records' own sizes.
    pub fn from_pack(pack: &gc_assets::scenery::Scenery) -> SceneryModels {
        Self::build(pack.models.iter().map(|m| (m.name(), m.height, m.radius)))
    }

    /// The index for a pack whose models are these (name, height, radius),
    /// in order.
    fn build<'a>(models: impl Iterator<Item = (&'a str, f32, f32)>) -> SceneryModels {
        let mut s = SceneryModels::default();
        for (i, (name, height, radius)) in models.enumerate() {
            let Ok(id) = u16::try_from(i) else { break };
            let known = Model::ALL.iter().position(|m| m.spec().name == name);
            let spec = known.map(|k| Model::ALL[k].spec());
            if let Some(k) = known {
                s.ids[k] = Some(id);
            }
            s.info.push(Info {
                class: spec.map_or(2, |m| m.class),
                shadow: spec.is_some_and(|m| m.shadow),
                rock: spec.is_some_and(|m| m.family == Family::Rock),
                height,
                radius,
            });
        }
        s
    }

    /// Models in the pack.
    pub fn len(&self) -> usize {
        self.info.len()
    }

    pub fn is_empty(&self) -> bool {
        self.info.is_empty()
    }

    fn id(&self, m: Model) -> Option<u16> {
        self.ids[m as usize]
    }

    /// Whether the pack has both gathering-spot models, so Conquest's
    /// seeps and veins are drawn as models (else as the old boxes).
    fn has_nodes(&self) -> bool {
        self.id(Model::GrubSeep).is_some() && self.id(Model::MarrowVein).is_some()
    }

    /// Whether the pack has any model of `list`.
    fn has(&self, list: &[(Model, u8)]) -> bool {
        list.iter().any(|&(m, _)| self.id(m).is_some())
    }
}

/// What a land grows: weighted lists of models and how thick they lie on
/// open ground, per tile.
struct Set {
    /// Woods trees.
    trees: &'static [(Model, u8)],
    /// Undergrowth in woods and bushes on open ground.
    bushes: &'static [(Model, u8)],
    grass: &'static [(Model, u8)],
    /// Dark firs that fill thicket (the land's trees if the pack has none).
    thicket: &'static [(Model, u8)],
    /// The few clumps on tall-grass tiles (the land's grass if missing).
    tall: &'static [(Model, u8)],
    /// Reeds and the like on open shores.
    shore: &'static [(Model, u8)],
    /// Rocks other than pebbles.
    rocks: &'static [(Model, u8)],
    /// Lone trees on open ground.
    lone: &'static [(Model, u8)],
    grass_per_tile: f32,
    bush_per_tile: f32,
    pebbles_per_tile: f32,
    rock_per_tile: f32,
    lone_per_tile: f32,
}

use Model::*;

const FIR: &[(Model, u8)] = &[(ThicketFir, 1)];
const TALL: &[(Model, u8)] = &[(TallGrass, 1)];
const REEDS: &[(Model, u8)] = &[(Reeds, 1)];

static MEADOW: Set = Set {
    thicket: FIR,
    tall: TALL,
    shore: &[],
    trees: &[(OakA, 4), (OakB, 4), (Birch, 1)],
    bushes: &[(LeafyBush, 1)],
    grass: &[(GrassClump, 6), (Wildflowers, 2)],
    rocks: &[(Boulder, 1)],
    lone: &[],
    grass_per_tile: 0.45,
    bush_per_tile: 0.04,
    pebbles_per_tile: 0.03,
    rock_per_tile: 0.006,
    lone_per_tile: 0.0,
};
static FOREST: Set = Set {
    thicket: FIR,
    tall: TALL,
    shore: &[],
    trees: &[(PineA, 4), (PineB, 3), (OakB, 1)],
    bushes: &[(Fern, 4), (Glowcap, 1)],
    grass: &[(GrassClump, 1)],
    rocks: &[(MossyBoulder, 1)],
    lone: &[],
    grass_per_tile: 0.25,
    bush_per_tile: 0.05,
    pebbles_per_tile: 0.03,
    rock_per_tile: 0.006,
    lone_per_tile: 0.0,
};
static FEN: Set = Set {
    thicket: FIR,
    tall: REEDS,
    shore: REEDS,
    trees: &[(Willow, 3), (Snag, 2)],
    bushes: &[(LeafyBush, 2), (Glowcap, 1)],
    grass: &[(Reeds, 3), (TallGrass, 1)],
    rocks: &[(MossyBoulder, 1)],
    lone: &[(Willow, 1), (Snag, 2)],
    grass_per_tile: 0.4,
    bush_per_tile: 0.04,
    pebbles_per_tile: 0.02,
    rock_per_tile: 0.004,
    lone_per_tile: 0.01,
};
static HIGHLAND: Set = Set {
    thicket: FIR,
    tall: TALL,
    shore: &[],
    trees: &[(CragPine, 3), (PineB, 2)],
    bushes: &[(Heather, 1)],
    grass: &[(DryGrass, 1)],
    rocks: &[(Boulder, 4), (Slab, 3), (Crag, 1)],
    lone: &[(CragPine, 1)],
    grass_per_tile: 0.35,
    bush_per_tile: 0.06,
    pebbles_per_tile: 0.08,
    rock_per_tile: 0.025,
    lone_per_tile: 0.012,
};
static WASTE: Set = Set {
    thicket: FIR,
    tall: TALL,
    shore: &[],
    trees: &[(Snag, 3), (BoneSpire, 2)],
    bushes: &[(ThornScrub, 1)],
    grass: &[(DryGrass, 1)],
    rocks: &[(Slab, 3), (BoneRibs, 1)],
    lone: &[(Snag, 3), (BoneSpire, 2)],
    grass_per_tile: 0.15,
    bush_per_tile: 0.06,
    pebbles_per_tile: 0.08,
    rock_per_tile: 0.02,
    lone_per_tile: 0.012,
};
static VALLEY: Set = Set {
    thicket: FIR,
    tall: TALL,
    shore: REEDS,
    trees: &[(Birch, 4), (OakA, 3)],
    bushes: &[(LeafyBush, 1)],
    grass: &[(GrassClump, 3), (Wildflowers, 3)],
    rocks: &[],
    lone: &[],
    grass_per_tile: 0.55,
    bush_per_tile: 0.04,
    pebbles_per_tile: 0.03,
    rock_per_tile: 0.0,
    lone_per_tile: 0.0,
};

fn set_of(land: Land) -> &'static Set {
    match land {
        Land::Meadow => &MEADOW,
        Land::Forest => &FOREST,
        Land::Fen => &FEN,
        Land::Highland => &HIGHLAND,
        Land::Waste => &WASTE,
        Land::Valley => &VALLEY,
    }
}

/// How far round a site (tiles) bushes, rocks and lone trees keep off.
fn clear_radius(site: Site) -> i32 {
    match site {
        Site::Start => 24,
        Site::Village | Site::Town | Site::Bazaar => 8,
        Site::CreepCamp => 6,
        Site::OldMouth => 7,
        Site::Heart => 9,
        Site::Shrine | Site::GrubSeep | Site::MarrowVein => 4,
        Site::Bloodseep | Site::FrontierSlot => 3,
    }
}

/// What the placement reads of a map.
pub struct Terrain<'a> {
    pub tiles: u32,
    /// `gc_sim::Cover` bytes, one per tile (empty: all open).
    pub cover: &'a [u8],
    pub thicket: &'a [bool],
    /// `Land` bytes, one per tile (empty: meadow).
    pub land: &'a [u8],
    /// Tile heights and the water line in the same units (empty: flat,
    /// dry).
    pub heights: &'a [u16],
    pub water: u16,
    pub sites: Vec<(Site, (i32, i32))>,
}

impl<'a> Terrain<'a> {
    pub fn of(map: &'a Map) -> Terrain<'a> {
        Terrain {
            tiles: map.tiles,
            cover: &map.cover,
            thicket: &map.thicket,
            land: &map.land,
            heights: &map.heights,
            water: map.water,
            sites: map.objects.iter().map(|o| (o.kind, o.at)).collect(),
        }
    }
}

/// One piece of scenery as stored: 12 bytes.
#[derive(Clone, Copy, Debug)]
struct Placed {
    /// Position in its chunk, in 1/`STEPS` tiles.
    x: u16,
    y: u16,
    model: u16,
    /// Facing, 256 steps to a turn.
    angle: u8,
    /// Scale in hundredths.
    scale: u8,
    tint: [u8; 4],
}

struct Chunk {
    /// What stands here, by class (see [`Info::class`]), so a frame can skip
    /// a whole class that is out of reach.
    classes: [Box<[Placed]>; 3],
    /// A sphere round everything in it, for culling.
    centre: Vec3,
    radius: f32,
}

/// A building footprint: north-west tile, width and depth.
type Rect = (i32, i32, u8, u8);

/// A Conquest gathering spot to draw as a model this frame: a Grub seep
/// or Marrow vein, at `at` (x, ground height, y), shrunk to `scale` as it
/// runs down and tinted by `shade` (128 as baked, lower darker).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeLook {
    pub marrow: bool,
    pub at: Vec3,
    pub scale: f32,
    pub shade: u8,
}

/// A chunk picked for this frame.
struct Near {
    /// Tiles from the focus to its nearest edge, about.
    d: f32,
    chunk: u32,
    /// Per class: the level of detail, or `None` if the class is out of
    /// reach.
    lods: [Option<u8>; 3],
    /// In the camera's view, not only the sun's.
    screen: bool,
}

pub struct Scenery {
    models: SceneryModels,
    tiles: u32,
    /// Chunks per side.
    side: u32,
    chunks: Vec<Chunk>,
    /// Tiles under a building, one bit each, and the chunks holding any.
    occupied: Vec<u64>,
    occupied_chunks: Vec<bool>,
    rects: Vec<Rect>,
    spare: Vec<Rect>,
    /// The gathering spots, set each frame ([`Scenery::set_nodes`]), and
    /// scratch for this frame's: the batch bin and spot of each one seen.
    nodes: Vec<NodeLook>,
    node_bins: Vec<(usize, usize)>,
    facings: [[f32; 2]; 256],
    // Scratch, kept between frames.
    near: Vec<Near>,
    counts: Vec<u32>,
    cursor: Vec<u32>,
    instances: Vec<SceneryInstance>,
    batches: Vec<SceneryBatch>,
}

impl Scenery {
    /// The scenery for a map; `seed` varies it between maps.
    pub fn new(map: &Map, seed: u64, models: &SceneryModels) -> Scenery {
        Scenery::build(&Terrain::of(map), seed, models)
    }

    /// Nothing at all (the showroom's bare ground).
    pub fn empty(models: &SceneryModels) -> Scenery {
        Scenery::blank(0, models)
    }

    /// A scenery of `n` tiles a side with no chunks yet.
    fn blank(n: u32, models: &SceneryModels) -> Scenery {
        let side = n.div_ceil(CHUNK);
        Scenery {
            models: models.clone(),
            tiles: n,
            side,
            chunks: Vec::new(),
            occupied: vec![0; (n as usize * n as usize).div_ceil(64)],
            occupied_chunks: vec![false; (side * side) as usize],
            rects: Vec::new(),
            spare: Vec::new(),
            nodes: Vec::new(),
            node_bins: Vec::new(),
            facings: std::array::from_fn(|i| {
                let a = i as f32 / 256.0 * std::f32::consts::TAU;
                [a.cos(), a.sin()]
            }),
            near: Vec::new(),
            counts: vec![0; models.len() * LODS],
            cursor: vec![0; models.len() * LODS],
            instances: Vec::new(),
            batches: Vec::new(),
        }
    }

    pub fn build(t: &Terrain, seed: u64, models: &SceneryModels) -> Scenery {
        let mut s = Scenery::blank(t.tiles, models);
        if models.is_empty() || t.tiles == 0 {
            return s;
        }
        // Every frame writes its slots in place; nothing is cleared or grown.
        s.instances = vec![SceneryInstance::default(); MAX_SCENERY_INSTANCES];
        let clear = keep_clear(t);
        s.chunks.reserve((s.side * s.side) as usize);
        for cy in 0..s.side {
            for cx in 0..s.side {
                s.chunks.push(build_chunk(t, &clear, seed, models, cx, cy));
            }
        }
        s
    }

    /// Instances stored, and the bytes they take.
    #[cfg(test)]
    pub fn stored(&self) -> (usize, usize) {
        let count = self
            .chunks
            .iter()
            .map(|c| c.classes.iter().map(|s| s.len()).sum::<usize>())
            .sum();
        (count, count * size_of::<Placed>())
    }

    /// Hide the scenery standing on these building footprints (north-west
    /// tile, width, depth), and a tile's margin round them. Cheap to call
    /// every frame with the same list.
    pub fn set_footprints(&mut self, rects: impl Iterator<Item = Rect>) {
        self.spare.clear();
        self.spare.extend(rects);
        if self.spare == self.rects {
            return;
        }
        std::mem::swap(&mut self.rects, &mut self.spare);
        self.occupied.fill(0);
        self.occupied_chunks.fill(false);
        let n = self.tiles as i32;
        for &(x, y, w, d) in &self.rects {
            for ty in (y - 1).max(0)..(y + i32::from(d) + 1).min(n) {
                for tx in (x - 1).max(0)..(x + i32::from(w) + 1).min(n) {
                    let i = ty as usize * n as usize + tx as usize;
                    self.occupied[i / 64] |= 1 << (i % 64);
                    self.occupied_chunks
                        [((ty as u32 / CHUNK) * self.side + tx as u32 / CHUNK) as usize] = true;
                }
            }
        }
    }

    /// The gathering spots to draw with the next frames, replacing the
    /// last list. Ignored when the pack lacks their models.
    pub fn set_nodes(&mut self, nodes: impl Iterator<Item = NodeLook>) {
        self.nodes.clear();
        if self.models.has_nodes() {
            self.nodes.extend(nodes.take(MAX_NODES));
        }
    }

    /// Whether [`Scenery::frame`] draws the gathering spots: the pack has
    /// their models and this map has scenery at all.
    pub fn draws_nodes(&self) -> bool {
        self.models.has_nodes() && !self.instances.is_empty()
    }

    /// The scenery near the camera's ground point `focus`, the camera
    /// `distance` back, sorted into batches by model and level of detail:
    /// trees, rocks and bushes, and grass and pebbles unless `grass` is
    /// false (the Commander View). The slices are valid until the next call.
    pub fn frame(
        &mut self,
        focus: Vec2,
        distance: f32,
        grass: bool,
        cull: &Cull,
    ) -> (&[SceneryInstance], &[SceneryBatch]) {
        self.batches.clear();
        if self.instances.is_empty() {
            return (&[], &self.batches);
        }
        let reach =
            std::array::from_fn::<f32, 3, _>(|c| REACH_NEAR[c] + distance * REACH_PER_ZOOM[c]);
        let widest = reach[0].max(reach[1]);
        self.near.clear();
        for (k, c) in self.chunks.iter().enumerate() {
            if c.classes.iter().all(|s| s.is_empty()) {
                continue;
            }
            let d = (Vec2::new(c.centre.x, c.centre.z).distance(focus) - CHUNK_RADIUS).max(0.0);
            if d > widest {
                continue;
            }
            // Off screen, only the sun sees it: shadow casters alone.
            let screen = cull.on_screen(c.centre, c.radius);
            if !screen && !cull.sees(c.centre, c.radius) {
                continue;
            }
            let eye = d.hypot(distance);
            let lods = std::array::from_fn(|class| {
                let on = d <= reach[class] && (class < 2 || (grass && screen));
                on.then(|| lod_for(class, eye))
            });
            self.near.push(Near {
                d,
                chunk: k as u32,
                lods,
                screen,
            });
        }
        self.near.sort_unstable_by(|a, b| a.d.total_cmp(&b.d));

        let Scenery {
            models,
            tiles,
            side,
            chunks,
            occupied,
            occupied_chunks,
            nodes,
            node_bins,
            facings,
            near,
            counts,
            cursor,
            instances,
            batches,
            ..
        } = self;
        let visit = |n: &Near, f: &mut dyn FnMut(&Placed, usize, [f32; 2])| {
            let chunk = &chunks[n.chunk as usize];
            let (ox, oy) = ((n.chunk % *side) * CHUNK, (n.chunk / *side) * CHUNK);
            let blocked = occupied_chunks[n.chunk as usize];
            for (class, lod) in n.lods.iter().enumerate() {
                let Some(lod) = *lod else { continue };
                for p in &chunk.classes[class] {
                    if !n.screen && !models.info[usize::from(p.model)].shadow {
                        continue;
                    }
                    if blocked {
                        let tx = ox + u32::from(p.x) / STEPS as u32;
                        let ty = oy + u32::from(p.y) / STEPS as u32;
                        let i = ty as usize * *tiles as usize + tx as usize;
                        if occupied[i / 64] >> (i % 64) & 1 != 0 {
                            continue;
                        }
                    }
                    f(
                        p,
                        usize::from(p.model) * LODS + usize::from(lod),
                        [ox as f32, oy as f32],
                    );
                }
            }
        };

        counts.fill(0);
        let mut total = 0;
        let mut taken = 0;
        // Gathering spots first: few, and they matter for play. Each one
        // the camera or the sun sees goes in its model's bin.
        node_bins.clear();
        for (k, n) in nodes.iter().enumerate() {
            let model = if n.marrow {
                Model::MarrowVein
            } else {
                Model::GrubSeep
            };
            let Some(id) = models.id(model).map(usize::from) else {
                continue;
            };
            // A sphere round the whole model, from its base to its top.
            let (radius, half) = (models.info[id].radius, models.info[id].height / 2.0);
            let centre = n.at + Vec3::Y * half * n.scale;
            if !cull.sees(centre, (radius + half) * n.scale) {
                continue;
            }
            let d = Vec2::new(n.at.x, n.at.z).distance(focus);
            let bin = id * LODS + usize::from(lod_for(1, d.hypot(distance)));
            counts[bin] += 1;
            total += 1;
            node_bins.push((bin, k));
        }
        for (i, n) in near.iter().enumerate() {
            // At most what its classes in reach hold (buildings may hide
            // some): too many, and this chunk and every one farther go.
            let most: usize = n
                .lods
                .iter()
                .zip(&chunks[n.chunk as usize].classes)
                .filter(|(lod, _)| lod.is_some())
                .map(|(_, slice)| slice.len())
                .sum();
            if total + most > MAX_SCENERY_INSTANCES {
                break;
            }
            visit(n, &mut |_, bin, _| {
                counts[bin] += 1;
                total += 1;
            });
            taken = i + 1;
        }
        let mut at = 0;
        for (bin, &count) in counts.iter().enumerate() {
            cursor[bin] = at;
            if count > 0 {
                batches.push(SceneryBatch {
                    model: (bin / LODS) as u16,
                    lod: (bin % LODS) as u8,
                    shadow: models.info[bin / LODS].shadow,
                    first: at,
                    count,
                });
            }
            at += count;
        }
        for &(bin, k) in node_bins.iter() {
            let n = &nodes[k];
            let slot = cursor[bin] as usize;
            cursor[bin] += 1;
            // Turned by where it lies, so no two look the same way.
            let turn = (n.at.x * 37.0 + n.at.z * 91.0) as i64 as u8;
            instances[slot] = SceneryInstance {
                pos: [n.at.x, n.at.z],
                facing: facings[usize::from(turn)],
                scale: n.scale,
                tint: [n.shade, n.shade, n.shade, 0],
            };
        }
        for n in &near[..taken] {
            visit(n, &mut |p, bin, origin| {
                let slot = cursor[bin] as usize;
                cursor[bin] += 1;
                instances[slot] = SceneryInstance {
                    pos: [
                        origin[0] + f32::from(p.x) / STEPS,
                        origin[1] + f32::from(p.y) / STEPS,
                    ],
                    facing: facings[usize::from(p.angle)],
                    scale: f32::from(p.scale) / 100.0,
                    tint: p.tint,
                };
            });
        }
        (&self.instances[..total], &self.batches)
    }
}

/// The level of detail for a class of model `eye` tiles from the camera.
pub fn lod_for(class: usize, eye: f32) -> u8 {
    LOD_AT[class].iter().filter(|&&at| eye > at).count() as u8
}

/// Tiles near a site, where open ground keeps its bushes, rocks and lone
/// trees off.
fn keep_clear(t: &Terrain) -> Vec<bool> {
    let n = t.tiles as i32;
    let mut clear = vec![false; (n as usize) * (n as usize)];
    for &(site, (sx, sy)) in &t.sites {
        let r = clear_radius(site);
        for y in (sy - r).max(0)..=(sy + r).min(n - 1) {
            for x in (sx - r).max(0)..=(sx + r).min(n - 1) {
                if (x - sx) * (x - sx) + (y - sy) * (y - sy) <= r * r {
                    clear[(y * n + x) as usize] = true;
                }
            }
        }
    }
    clear
}

/// A small random stream seeded by a tile.
struct Rng(u64);

impl Rng {
    fn tile(seed: u64, x: u32, y: u32) -> Rng {
        Rng(seed ^ (u64::from(x) << 40) ^ (u64::from(y) << 16) ^ 0x5CE7_E41B)
    }

    /// SplitMix64.
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 32) as u32
    }

    /// 0 up to 1.
    fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }

    fn between(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.unit() * (hi - lo)
    }

    fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    /// A whole number averaging `per_tile`.
    fn count(&mut self, per_tile: f32) -> u32 {
        per_tile as u32 + u32::from(self.chance(per_tile.fract()))
    }
}

/// What a chunk's placement collects.
struct Out {
    classes: [Vec<Placed>; 3],
    /// Tallest and widest thing placed.
    height: f32,
    radius: f32,
}

/// The tile's neighbours' surroundings the rules read.
struct Tile {
    x: u32,
    y: u32,
    set: &'static Set,
    /// Steepness: biggest height step to a neighbour, in tiles.
    slope: f32,
    shore: bool,
}

struct Placer<'a> {
    models: &'a SceneryModels,
    out: Out,
    origin: (u32, u32),
}

impl Placer<'_> {
    /// A model picked from a weighted list, if the pack has any of them.
    fn pick(&self, list: &[(Model, u8)], rng: &mut Rng) -> Option<Model> {
        let total: u32 = list
            .iter()
            .filter(|(m, _)| self.models.id(*m).is_some())
            .map(|&(_, w)| u32::from(w))
            .sum();
        if total == 0 {
            return None;
        }
        let mut roll = rng.next() % total;
        for &(m, w) in list.iter().filter(|(m, _)| self.models.id(*m).is_some()) {
            if roll < u32::from(w) {
                return Some(m);
            }
            roll -= u32::from(w);
        }
        None
    }

    /// Stand `model` somewhere on tile (x, y), at a scale in `lo..hi`.
    fn put(&mut self, model: Model, t: &Tile, rng: &mut Rng, lo: f32, hi: f32, margin: f32) {
        let Some(id) = self.models.id(model) else {
            return;
        };
        let info = self.models.info[usize::from(id)];
        let scale = rng.between(lo, hi);
        let fx = t.x as f32 + rng.between(margin, 1.0 - margin);
        let fy = t.y as f32 + rng.between(margin, 1.0 - margin);
        let lx = ((fx - self.origin.0 as f32) * STEPS).clamp(0.0, 65535.0) as u16;
        let ly = ((fy - self.origin.1 as f32) * STEPS).clamp(0.0, 65535.0) as u16;
        let angle = rng.next() as u8;
        // Rocks hardly vary in colour; leaves and grass more.
        let spread = if info.rock { 8.0 } else { 18.0 };
        let bright = 128.0 + rng.between(-spread, spread * 0.6);
        let hue = rng.between(-spread * 0.4, spread * 0.4);
        let tint = [
            (bright + hue).clamp(0.0, 255.0) as u8,
            (bright + rng.between(-3.0, 3.0)).clamp(0.0, 255.0) as u8,
            (bright - hue).clamp(0.0, 255.0) as u8,
            rng.next() as u8,
        ];
        let p = Placed {
            x: lx,
            y: ly,
            model: id,
            angle,
            scale: (scale * 100.0) as u8,
            tint,
        };
        self.out.classes[usize::from(info.class)].push(p);
        self.out.height = self.out.height.max(info.height * scale);
        self.out.radius = self.out.radius.max(info.radius * scale);
    }

    fn tree(&mut self, t: &Tile, rng: &mut Rng, list: &[(Model, u8)]) {
        if let Some(m) = self.pick(list, rng) {
            self.put(m, t, rng, 0.8, 1.2, 0.2);
        }
    }

    /// `count` models picked from `list`, scattered over the tile.
    fn scatter(&mut self, t: &Tile, rng: &mut Rng, list: &[(Model, u8)], count: u32) {
        for _ in 0..count {
            if let Some(m) = self.pick(list, rng) {
                self.put(m, t, rng, 0.8, 1.3, 0.1);
            }
        }
    }

    /// One tile's scenery.
    fn tile(&mut self, t: &Tile, cover: u8, thicket: bool, clear: bool, rng: &mut Rng) {
        let set = t.set;
        match cover {
            2 if thicket => {
                // A dark fir on every tile, now and then the land's own tree.
                let list = if rng.chance(0.15) || !self.models.has(set.thicket) {
                    set.trees
                } else {
                    set.thicket
                };
                self.tree(t, rng, list);
                let under = 1 + u32::from(rng.chance(0.6));
                self.scatter(t, rng, set.bushes, under);
            }
            2 => {
                if rng.next() % 100 < TREE_PERCENT {
                    self.tree(t, rng, set.trees);
                }
                let under = u32::from(rng.chance(0.25));
                self.scatter(t, rng, set.bushes, under);
            }
            1 => {
                let list = if self.models.has(set.tall) {
                    set.tall
                } else {
                    set.grass
                };
                let count = 3 + u32::from(rng.chance(0.5));
                self.scatter(t, rng, list, count);
            }
            _ => {
                let count = rng.count(set.grass_per_tile);
                self.scatter(t, rng, set.grass, count);
                if t.shore && !set.shore.is_empty() {
                    let count = 1 + u32::from(rng.chance(0.5));
                    self.scatter(t, rng, set.shore, count);
                }
                let count = rng.count(set.pebbles_per_tile);
                for _ in 0..count {
                    self.put(Pebbles, t, rng, 0.7, 1.3, 0.2);
                }
                if clear {
                    return;
                }
                let count = rng.count(set.bush_per_tile);
                self.scatter(t, rng, set.bushes, count);
                let steep = 1.0 + (t.slope * 2.0).min(1.0);
                let count = rng.count(set.rock_per_tile * steep);
                for _ in 0..count {
                    if let Some(m) = self.pick(set.rocks, rng) {
                        self.put(m, t, rng, 0.7, 1.3, 0.25);
                    }
                }
                if rng.chance(set.lone_per_tile) {
                    self.tree(t, rng, set.lone);
                }
            }
        }
    }
}

fn build_chunk(
    t: &Terrain,
    clear: &[bool],
    seed: u64,
    models: &SceneryModels,
    cx: u32,
    cy: u32,
) -> Chunk {
    let n = t.tiles;
    let (ox, oy) = (cx * CHUNK, cy * CHUNK);
    let mut placer = Placer {
        models,
        out: Out {
            classes: Default::default(),
            height: 0.0,
            radius: 0.0,
        },
        origin: (ox, oy),
    };
    let flat = t.heights.len() != (n * n) as usize;
    let tile_height = |x: u32, y: u32| f32::from(t.heights[(y * n + x) as usize]);
    let units = gc_sim::terrain::HEIGHT_UNITS as f32;
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for y in oy..(oy + CHUNK).min(n) {
        for x in ox..(ox + CHUNK).min(n) {
            let i = (y * n + x) as usize;
            let mut slope = 0.0f32;
            let mut shore = false;
            if !flat {
                let h = tile_height(x, y);
                lo = lo.min(h / units);
                hi = hi.max(h / units);
                if h < f32::from(t.water) {
                    continue;
                }
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= n as i32 || ny >= n as i32 {
                        continue;
                    }
                    let nh = tile_height(nx as u32, ny as u32);
                    slope = slope.max((nh - h).abs() / units);
                    shore |= nh < f32::from(t.water);
                }
            }
            let land = t.land.get(i).map_or(Land::Meadow, |&b| Land::from_u8(b));
            let tile = Tile {
                x,
                y,
                set: set_of(land),
                slope,
                shore,
            };
            let cover = t.cover.get(i).copied().unwrap_or(0);
            let thicket = t.thicket.get(i) == Some(&true);
            let mut rng = Rng::tile(seed, x, y);
            placer.tile(&tile, cover, thicket, clear[i], &mut rng);
        }
    }
    if flat {
        (lo, hi) = (0.0, 0.0);
    }
    let Out {
        classes,
        height,
        radius,
    } = placer.out;
    let top = hi + height;
    let (mid, half) = ((lo + top) * 0.5, (top - lo) * 0.5);
    Chunk {
        classes: classes.map(Vec::into_boxed_slice),
        centre: Vec3::new(
            ox as f32 + CHUNK as f32 * 0.5,
            mid,
            oy as f32 + CHUNK as f32 * 0.5,
        ),
        radius: CHUNK_RADIUS + radius + half,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pack of these (name, height, radius) models.
    fn pack(models: &[(&str, f32, f32)]) -> SceneryModels {
        SceneryModels::build(models.iter().copied())
    }

    /// A pack with every model the placement knows.
    fn models() -> SceneryModels {
        let all = Model::ALL.map(|m| (m.spec().name, 1.0, 1.0));
        pack(&all)
    }

    fn family_of(name: &str) -> Family {
        Model::ALL
            .iter()
            .find(|m| m.spec().name == name)
            .unwrap()
            .spec()
            .family
    }

    const N: u32 = 64;

    struct World {
        cover: Vec<u8>,
        thicket: Vec<bool>,
        land: Vec<u8>,
        heights: Vec<u16>,
        sites: Vec<(Site, (i32, i32))>,
    }

    impl World {
        fn new() -> World {
            let mut cover = vec![0u8; (N * N) as usize];
            for y in 10..30 {
                for x in 10..30 {
                    cover[(y * N + x) as usize] = 2;
                }
            }
            for y in 40..44 {
                for x in 40..60 {
                    cover[(y * N + x) as usize] = 1;
                }
            }
            World {
                cover,
                thicket: Vec::new(),
                land: Vec::new(),
                heights: vec![1000; (N * N) as usize],
                sites: Vec::new(),
            }
        }

        fn build(&self, seed: u64, models: &SceneryModels) -> Scenery {
            Scenery::build(
                &Terrain {
                    tiles: N,
                    cover: &self.cover,
                    thicket: &self.thicket,
                    land: &self.land,
                    heights: &self.heights,
                    water: 500,
                    sites: self.sites.clone(),
                },
                seed,
                models,
            )
        }
    }

    /// Every stored instance with its map position and model name.
    fn all(s: &Scenery) -> Vec<([f32; 2], &'static str)> {
        let mut v = Vec::new();
        for (k, c) in s.chunks.iter().enumerate() {
            let (ox, oy) = ((k as u32 % s.side) * CHUNK, (k as u32 / s.side) * CHUNK);
            for p in c.classes.iter().flat_map(|s| s.iter()) {
                v.push((
                    [
                        ox as f32 + f32::from(p.x) / STEPS,
                        oy as f32 + f32::from(p.y) / STEPS,
                    ],
                    Model::ALL[s
                        .models
                        .ids
                        .iter()
                        .position(|&id| id == Some(p.model))
                        .unwrap()]
                    .spec()
                    .name,
                ));
            }
        }
        v
    }

    fn is_tree(name: &str) -> bool {
        family_of(name) == Family::Tree
    }

    #[test]
    fn placement_is_deterministic_for_a_seed() {
        let w = World::new();
        let a = all(&w.build(7, &models()));
        assert_eq!(a, all(&w.build(7, &models())));
        assert_ne!(a, all(&w.build(8, &models())));
        assert!(!a.is_empty());
    }

    #[test]
    fn trees_grow_only_in_woods_and_grass_in_tall_grass() {
        let w = World::new();
        let s = w.build(7, &models());
        let items = all(&s);
        let trees: Vec<_> = items.iter().filter(|(_, n)| is_tree(n)).collect();
        // About 55% of the 400 wood tiles.
        assert!((160..280).contains(&trees.len()), "{} trees", trees.len());
        for ([x, y], name) in &trees {
            assert!(
                (10.0..30.0).contains(x) && (10.0..30.0).contains(y),
                "{name} at {x},{y}"
            );
        }
        // Meadow woods take oaks and birch.
        assert!(
            trees
                .iter()
                .all(|(_, n)| ["oak-a", "oak-b", "birch"].contains(n))
        );
        let tall = items.iter().filter(|(_, n)| *n == "tall-grass").count();
        assert!((80 * 3..=80 * 4).contains(&tall), "{tall} tall grass");
    }

    #[test]
    fn thicket_has_a_fir_on_every_tile() {
        let mut w = World::new();
        w.thicket = vec![false; w.cover.len()];
        for y in 10..20 {
            for x in 10..30 {
                w.thicket[(y * N + x) as usize] = true;
            }
        }
        let items = all(&w.build(7, &models()));
        let dense = items
            .iter()
            .filter(|([x, y], _)| (10.0..30.0).contains(x) && (10.0..20.0).contains(y))
            .collect::<Vec<_>>();
        let firs = dense.iter().filter(|(_, n)| *n == "thicket-fir").count();
        assert!((160..=200).contains(&firs), "{firs} firs on 200 tiles");
        assert!(dense.len() > 200 + 200);
    }

    #[test]
    fn lands_choose_their_models() {
        let mut w = World::new();
        w.land = vec![Land::Waste as u8; w.cover.len()];
        let items = all(&w.build(7, &models()));
        assert!(
            items
                .iter()
                .filter(|(_, n)| is_tree(n))
                .all(|(_, n)| ["snag", "bone-spire"].contains(n))
        );
        assert!(items.iter().any(|(_, n)| *n == "pebbles"));
        assert!(!items.iter().any(|(_, n)| *n == "oak-a" || *n == "crag"));
    }

    #[test]
    fn nothing_stands_under_water() {
        let mut w = World::new();
        for y in 0..N {
            for x in 0..20 {
                w.heights[(y * N + x) as usize] = 100;
            }
        }
        let items = all(&w.build(7, &models()));
        assert!(!items.is_empty());
        assert!(items.iter().all(|([x, _], _)| *x >= 20.0));
    }

    #[test]
    fn sites_stay_clear_of_bushes_and_rocks() {
        let mut w = World::new();
        w.cover = vec![0; w.cover.len()];
        w.land = vec![Land::Highland as u8; w.cover.len()];
        w.sites = vec![(Site::Start, (32, 32))];
        let items = all(&w.build(7, &models()));
        let bare = |name: &str| {
            matches!(family_of(name), Family::Bush | Family::Rock) && name != "pebbles"
                || is_tree(name)
        };
        let near = |[x, y]: [f32; 2]| (x - 32.5).hypot(y - 32.5) < 11.0;
        assert!(items.iter().any(|(_, n)| bare(n)), "some bushes or rocks");
        assert!(!items.iter().any(|(p, n)| bare(n) && near(*p)));
        // Grass still grows there.
        assert!(items.iter().any(|(p, n)| *n == "dry-grass" && near(*p)));
    }

    #[test]
    fn a_partial_pack_places_what_it_has() {
        let w = World::new();
        let some = pack(&[
            ("oak-b", 4.5, 1.4),
            ("tall-grass", 1.0, 0.4),
            ("unknown-thing", 1.0, 1.0),
        ]);
        let items = all(&w.build(7, &some));
        assert!(!items.is_empty());
        assert!(
            items
                .iter()
                .all(|(_, n)| ["oak-b", "tall-grass"].contains(n))
        );
        assert!(w.build(7, &SceneryModels::default()).chunks.is_empty());
    }

    #[test]
    fn level_of_detail_follows_distance() {
        for class in 0..3 {
            let lods: Vec<u8> = [0.0, 20.0, 60.0, 120.0, 400.0]
                .iter()
                .map(|&d| lod_for(class, d))
                .collect();
            assert_eq!(lods[0], 0);
            assert_eq!(lods[4], 3);
            assert!(lods.windows(2).all(|w| w[0] <= w[1]));
        }
        // Grass drops detail sooner than trees.
        assert!(lod_for(2, 60.0) > lod_for(0, 60.0));
    }

    fn check_batches(s: &Scenery, instances: &[SceneryInstance], batches: &[SceneryBatch]) {
        let mut covered = vec![0u8; instances.len()];
        for b in batches {
            assert!(b.count > 0 && usize::from(b.lod) < LODS);
            assert_eq!(b.shadow, s.models.info[usize::from(b.model)].shadow);
            for i in b.first..b.first + b.count {
                covered[i as usize] += 1;
            }
        }
        assert!(covered.iter().all(|&c| c == 1));
    }

    #[test]
    fn frames_batch_every_instance_once() {
        let w = World::new();
        let mut s = w.build(7, &models());
        let focus = Vec2::new(20.0, 20.0);
        let (i, b) = s.frame(focus, 10.0, true, &Cull::EVERYTHING);
        let (n, nb) = (i.len(), b.len());
        assert!(n > 100 && nb > 1);
        let (i, b) = (i.to_vec(), b.to_vec());
        check_batches(&s, &i, &b);
        // The instances are on the map and scaled about 1.
        assert!(i.iter().all(|x| (0.0..64.0).contains(&x.pos[0])));
        assert!(i.iter().all(|x| (0.6..1.5).contains(&x.scale)));
        // A trees-only (Commander View) frame is a part of it.
        let (i2, b2) = s.frame(focus, 10.0, false, &Cull::EVERYTHING);
        assert!(i2.len() < n && !i2.is_empty());
        let (i2, b2) = (i2.to_vec(), b2.to_vec());
        check_batches(&s, &i2, &b2);
        // Far from everything: nothing.
        let (i3, b3) = s.frame(Vec2::new(-900.0, -900.0), 0.0, true, &Cull::EVERYTHING);
        assert!(i3.is_empty() && b3.is_empty());
    }

    #[test]
    fn gathering_spots_are_drawn_as_their_models() {
        let w = World::new();
        let models = models();
        let mut s = w.build(7, &models);
        assert!(s.draws_nodes());
        let focus = Vec2::new(20.0, 20.0);
        let before = s.frame(focus, 10.0, true, &Cull::EVERYTHING).0.len();
        let look = |x: f32, marrow, scale| NodeLook {
            marrow,
            at: Vec3::new(x, 0.0, 20.0),
            scale,
            shade: 140,
        };
        s.set_nodes([look(20.0, false, 1.0), look(24.0, true, 0.4)].into_iter());
        let (i, b) = s.frame(focus, 10.0, true, &Cull::EVERYTHING);
        let (i, b) = (i.to_vec(), b.to_vec());
        assert_eq!(i.len(), before + 2);
        check_batches(&s, &i, &b);
        // Each spot is in its own model's batch, at its size.
        let found = |model: Model, scale: f32| {
            let id = models.id(model).unwrap();
            b.iter().filter(|b| b.model == id).any(|b| {
                i[b.first as usize..(b.first + b.count) as usize]
                    .iter()
                    .any(|x| x.scale == scale && x.tint[0] == 140)
            })
        };
        assert!(found(Model::GrubSeep, 1.0) && found(Model::MarrowVein, 0.4));
        // Even with nothing else near, they are drawn; a pack without
        // their models draws none (the game falls back to boxes).
        assert_eq!(
            s.frame(Vec2::new(-900.0, -900.0), 0.0, true, &Cull::EVERYTHING)
                .0
                .len(),
            2
        );
        let mut bare = w.build(7, &pack(&[("oak-a", 4.5, 1.4)]));
        bare.set_nodes([look(20.0, false, 1.0)].into_iter());
        assert!(!bare.draws_nodes() && bare.nodes.is_empty());
    }

    #[test]
    fn near_chunks_get_finer_levels_than_far_ones() {
        let w = World::new();
        let mut s = w.build(7, &models());
        let (_, b) = s.frame(Vec2::new(10.0, 10.0), 8.0, true, &Cull::EVERYTHING);
        let (min, max) = (
            b.iter().map(|b| b.lod).min().unwrap(),
            b.iter().map(|b| b.lod).max().unwrap(),
        );
        assert_eq!(min, 0);
        assert!(max > min);
    }

    #[test]
    fn the_budget_drops_the_farthest_first() {
        // Many dense chunks: more than the renderer takes.
        let mut w = World::new();
        w.cover = vec![1; w.cover.len()];
        let mut s = w.build(7, &models());
        // Fake a bigger map by repeating chunks' contents.
        let chunk = s.chunks.remove(0);
        let reps = MAX_SCENERY_INSTANCES / chunk.classes.iter().map(|s| s.len()).sum::<usize>() + 8;
        s.chunks.clear();
        s.side = 1;
        for _ in 0..reps {
            s.chunks.push(Chunk {
                classes: chunk.classes.clone(),
                centre: chunk.centre,
                radius: chunk.radius,
            });
        }
        s.side = reps as u32;
        s.occupied_chunks = vec![false; reps];
        let (i, b) = s.frame(Vec2::new(8.0, 8.0), 0.0, true, &Cull::EVERYTHING);
        assert!(i.len() <= MAX_SCENERY_INSTANCES && !i.is_empty());
        let (i, b) = (i.to_vec(), b.to_vec());
        check_batches(&s, &i, &b);
    }

    #[test]
    fn buildings_hide_the_scenery_under_them() {
        let w = World::new();
        let mut s = w.build(7, &models());
        let focus = Vec2::new(20.0, 20.0);
        let before = s.frame(focus, 10.0, true, &Cull::EVERYTHING).0.len();
        s.set_footprints([(14, 14, 6, 6)].into_iter());
        let after = s.frame(focus, 10.0, true, &Cull::EVERYTHING).0.to_vec();
        assert!(after.len() < before);
        assert!(
            !after
                .iter()
                .any(|i| (14.0..20.0).contains(&i.pos[0]) && (14.0..20.0).contains(&i.pos[1]))
        );
        s.set_footprints(std::iter::empty());
        assert_eq!(
            s.frame(focus, 10.0, true, &Cull::EVERYTHING).0.len(),
            before
        );
    }

    /// A 512-tile Wild Wilds map: the instances stay modest, and a 896-tile
    /// one (3 times the tiles) is extrapolated from it.
    #[test]
    fn a_big_map_keeps_its_scenery_small() {
        let def = gc_map::wilds::describe(512, 3, gc_map::wilds::Wildness::Wild);
        let map = Map::from_def(&def);
        let s = Scenery::new(&map, 3, &models());
        let (count, bytes) = s.stored();
        let big = bytes as f64 * (896.0f64 / 512.0).powi(2);
        eprintln!(
            "512 tiles: {count} instances, {:.1} MB; 896 tiles about {:.1} MB",
            bytes as f64 / 1e6,
            big / 1e6
        );
        assert!(count > 100_000);
        assert!(big < 40e6, "{:.1} MB", big / 1e6);
    }
}
