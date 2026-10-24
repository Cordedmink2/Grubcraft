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
use gc_render::{Cull, MAX_SCENERY_INSTANCES, SceneryBatch, SceneryInstance, surface_kind};
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

/// At most this many extra models (gathering spots, morale banners, trees
/// coming down, stumps) are drawn a frame, so they never crowd the scenery out of the buffer.
const MAX_EXTRAS: usize = 16384;

/// Tiles per chunk side.
const CHUNK: u32 = 16;
/// Half a chunk's diagonal, in tiles.
const CHUNK_RADIUS: f32 = CHUNK as f32 * 0.71;
/// Share of wood tiles with a tree on them, percent.
const TREE_PERCENT: u32 = 55;
/// Chunk-local positions are in this many steps a tile.
const STEPS: f32 = 2048.0;
/// How a tile's ground is classed by gc_map's nav rules (`Terrain::ground`):
/// walkable, steep (walkable, slow: a flank), a cliff no regiment climbs
/// (some half-tile cell of it is one), or, only here, a cliff's foot: a
/// walkable tile beside a cliff that rises out of it.
const OPEN: u8 = 0;
const STEEP: u8 = 1;
const CLIFF: u8 = 2;
const FOOT: u8 = 3;
/// A foot is a tile this much lower than its cliff neighbour, in tiles.
const FOOT_RISE: f32 = 0.3;
/// Extra rocks per tile on cliffs: a steep flank, a cliff face, and a
/// cliff's foot (the tile the cliff rises out of).
const FLANK_ROCKS: f32 = 0.3;
const FACE_ROCKS: f32 = 1.2;
const FOOT_ROCKS: f32 = 2.0;
/// Most extra cliff rocks in one 16 by 16 tile chunk, so a big map holds at
/// most about 0.09 of them per tile however much of it is cliff.
const CLIFF_ROCKS_PER_CHUNK: u32 = 24;

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
/// and pebbles: which reach and level-of-detail distances it follows),
/// whether it casts shadows and its surface detail. Sizes come from the pack's own records.
macro_rules! catalogue {
    ($($variant:ident $name:literal $family:ident $class:literal $shadow:literal $detail:ident,)*) => {
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
                detail: surface_kind::$detail,
            },)*];
        }
    };
}

struct Spec {
    name: &'static str,
    family: Family,
    class: u8,
    shadow: bool,
    /// The surface detail the shader paints on it (`gc_render::surface_kind`).
    detail: u32,
}

catalogue! {
    OakA "oak-a" Tree 0 true BROADLEAF,
    OakB "oak-b" Tree 0 true BROADLEAF,
    PineA "pine-a" Tree 0 true CONIFER,
    PineB "pine-b" Tree 0 true CONIFER,
    ThicketFir "thicket-fir" Tree 0 true CONIFER,
    Birch "birch" Tree 0 true BROADLEAF,
    Willow "willow" Tree 0 true BROADLEAF,
    Snag "snag" Tree 0 true CONIFER,
    CragPine "crag-pine" Tree 0 true CONIFER,
    BoneSpire "bone-spire" Tree 0 true NONE,
    LeafyBush "leafy-bush" Bush 1 false FOLIAGE,
    Fern "fern" Bush 1 false FOLIAGE,
    Heather "heather" Bush 1 false FOLIAGE,
    ThornScrub "thorn-scrub" Bush 1 false FOLIAGE,
    Glowcap "glowcap" Bush 1 false FOLIAGE,
    GrassClump "grass-clump" Grass 2 false FOLIAGE,
    TallGrass "tall-grass" Grass 2 false FOLIAGE,
    Reeds "reeds" Grass 2 false FOLIAGE,
    DryGrass "dry-grass" Grass 2 false FOLIAGE,
    Wildflowers "wildflowers" Grass 2 false FOLIAGE,
    Pebbles "pebbles" Rock 2 false NONE,
    Boulder "boulder" Rock 1 true STONE,
    MossyBoulder "mossy-boulder" Rock 1 true MOSSY_STONE,
    Crag "crag" Rock 1 true STONE,
    Slab "slab" Rock 1 true STONE,
    BoneRibs "bone-ribs" Rock 1 true NONE,
    GrubSeep "grub-seep" Rock 1 true NONE,
    MarrowVein "marrow-vein" Rock 1 true NONE,
    StumpBroad "stump-broad" Rock 1 true BROADLEAF,
    StumpConifer "stump-conifer" Rock 1 true CONIFER,
    BannerWavering "banner-wavering" Tree 1 true NONE,
    BannerRouting "banner-routing" Tree 1 true NONE,
    BannerShattered "banner-shattered" Tree 1 true NONE,
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
    detail: u32,
    height: f32,
    radius: f32,
}

/// Where each named model sits in the scenery pack (`None`: the pack
/// lacks it, and nothing of that name is placed).
#[derive(Clone, Debug)]
pub struct SceneryModels {
    ids: [Option<u16>; Model::COUNT],
    /// Per model of the pack.
    info: Vec<Info>,
}

impl Default for SceneryModels {
    fn default() -> Self {
        SceneryModels {
            ids: [None; Model::COUNT],
            info: Vec::new(),
        }
    }
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
                detail: spec.map_or(surface_kind::NONE, |m| m.detail),
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

    /// The named model at `id` in the pack, if it has a name here.
    fn model_of(&self, id: u16) -> Option<Model> {
        Model::ALL.into_iter().find(|&m| self.id(m) == Some(id))
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
/// The rocks that dress cliffs and steep flanks (see `Placer::cliff_rocks`).
const CLIFF_ROCKS: &[(Model, u8)] = &[(Crag, 2), (Slab, 2), (Boulder, 1)];

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
    /// Each tile's ground by the nav rules: 0 walkable, 1 steep, 2 cliff
    /// (empty: all walkable).
    pub ground: Vec<u8>,
    pub sites: Vec<(Site, (i32, i32))>,
}

impl<'a> Terrain<'a> {
    pub fn of(map: &'a Map) -> Terrain<'a> {
        // The cliffs are the half-tile cells the nav grid blocks for rise
        // (the ones the renderer builds its crags on), the steep tiles those
        // it routes round.
        let cliffs = map.cliff_cells();
        let steep = map.tile_ground(&map.nav_costs());
        let n = map.tiles as usize;
        let ground = (0..n * n)
            .map(|i| {
                let (x, y) = (i % n, i / n);
                let at = |dx: usize, dy: usize| cliffs[(2 * y + dy) * 2 * n + 2 * x + dx];
                if at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) > 0 {
                    CLIFF
                } else if steep[i] == gc_map::Ground::Steep {
                    STEEP
                } else {
                    OPEN
                }
            })
            .collect();
        Terrain {
            ground,
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

/// A model the game places this frame on top of the map's own scenery: a
/// Conquest gathering spot, a morale banner or a fallen standard, at `at` (x, ground height,
/// y), turned by `turn` (256ths of a circle), at `scale` and tinted by
/// `tint` (128 as baked, so 255 doubles and a team colour halved shows as
/// itself on white). `lean` tips it over about its base, 0 (upright) to 255
/// (lying down), toward `fall` (a turn from the way it faces, in 256ths,
/// counted as [`fall_of`] does): a tree coming down. `phase` is its wind
/// phase, so neighbours sway apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extra {
    pub model: Model,
    pub at: Vec3,
    pub turn: u8,
    pub scale: f32,
    pub tint: [u8; 3],
    pub phase: u8,
    pub lean: u8,
    pub fall: u8,
}

/// The turn (256ths of a circle) of the direction `dir` on the map, as the
/// scenery's facings read it.
pub fn turn_of(dir: Vec2) -> u8 {
    (dir.y.atan2(dir.x) / std::f32::consts::TAU * 256.0).round() as i32 as u8
}

/// The stump a felled `tree` leaves, if it leaves one.
pub fn stump_of(tree: Model) -> Option<Model> {
    match tree {
        Model::OakA | Model::OakB | Model::Birch | Model::Willow => Some(Model::StumpBroad),
        Model::PineA | Model::PineB | Model::ThicketFir | Model::CragPine | Model::Snag => {
            Some(Model::StumpConifer)
        }
        _ => None,
    }
}

impl Model {
    /// A stump left by a felled tree.
    pub fn is_stump(self) -> bool {
        matches!(self, Model::StumpBroad | Model::StumpConifer)
    }
}

/// How far from the camera's ground point stumps are drawn, in tiles: as
/// far as the bushes and rocks, which stand among them.
pub fn stump_reach(distance: f32) -> f32 {
    REACH_NEAR[1] + distance * REACH_PER_ZOOM[1]
}

/// The unit direction on the map of a turn of `turn` 256ths.
pub fn facing_of(turn: u8) -> Vec2 {
    Vec2::from_angle(f32::from(turn) / 256.0 * std::f32::consts::TAU)
}

/// How a model turned `turn` falls toward the map direction `dir`, as an
/// [`Extra`]'s `fall`: the direction in the model's own space, as a turn
/// from its front, as the scenery shader reads it.
pub fn fall_of(turn: u8, dir: Vec2) -> u8 {
    let front = facing_of(turn);
    let left = Vec2::new(front.y, -front.x);
    let (x, z) = (dir.dot(left), dir.dot(front));
    (x.atan2(z) / std::f32::consts::TAU * 256.0).round() as i32 as u8
}

/// A tree or bush that stood on a tile, as [`Scenery::standing`] finds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stood {
    pub model: Model,
    /// Its base on the map, in tiles.
    pub at: Vec2,
    pub turn: u8,
    pub scale: f32,
    /// As placed: the tint with the wind phase in the last byte.
    pub tint: [u8; 4],
    /// A tree, not a bush.
    pub tree: bool,
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
    /// Tiles cleared of trees and bushes during the match, one bit each,
    /// and the chunks holding any ([`Scenery::fell`]).
    felled: Vec<u64>,
    felled_chunks: Vec<bool>,
    rects: Vec<Rect>,
    spare: Vec<Rect>,
    /// The extra models, set each frame ([`Scenery::set_extras`]), and
    /// scratch for this frame's: the batch bin and index of each one seen.
    extras: Vec<Extra>,
    extra_bins: Vec<(usize, usize)>,
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
            felled: vec![0; (n as usize * n as usize).div_ceil(64)],
            felled_chunks: vec![false; (side * side) as usize],
            rects: Vec::new(),
            spare: Vec::new(),
            extras: Vec::new(),
            extra_bins: Vec::new(),
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

    /// Take the trees and bushes off these tiles, which were cleared (the
    /// simulation's `Event::Cleared`). Rocks and grass stay. Cheap: it only
    /// marks the tiles, and the frame skips what stands on them.
    pub fn fell(&mut self, tiles: impl Iterator<Item = (i32, i32)>) {
        let n = self.tiles as i32;
        for (x, y) in tiles {
            if !(0..n).contains(&x) || !(0..n).contains(&y) {
                continue;
            }
            let i = y as usize * n as usize + x as usize;
            self.felled[i / 64] |= 1 << (i % 64);
            self.felled_chunks[((y as u32 / CHUNK) * self.side + x as u32 / CHUNK) as usize] = true;
        }
    }

    /// The trees and bushes that stand on `tile` now: not yet felled, and
    /// not hidden under a building. Ask before [`Scenery::fell`] takes
    /// them off, to show them coming down.
    pub fn standing(&self, tile: (i32, i32)) -> impl Iterator<Item = Stood> + '_ {
        let n = self.tiles as i32;
        let (x, y) = tile;
        let on = (0..n).contains(&x) && (0..n).contains(&y) && !self.chunks.is_empty();
        let bit = |bits: &[u64]| {
            let i = y as usize * n as usize + x as usize;
            bits[i / 64] >> (i % 64) & 1 != 0
        };
        let gone = !on || bit(&self.felled) || bit(&self.occupied);
        let (cx, cy) = (x.max(0) as u32 / CHUNK, y.max(0) as u32 / CHUNK);
        let origin = [(cx * CHUNK) as f32, (cy * CHUNK) as f32];
        let chunk = (!gone)
            .then(|| self.chunks.get((cy * self.side + cx) as usize))
            .flatten();
        chunk
            .into_iter()
            .flat_map(|c| c.classes[..2].iter().flat_map(|s| s.iter()))
            .filter_map(move |p| {
                let at = Vec2::new(
                    origin[0] + f32::from(p.x) / STEPS,
                    origin[1] + f32::from(p.y) / STEPS,
                );
                let info = &self.models.info[usize::from(p.model)];
                let here = at.x.floor() as i32 == x && at.y.floor() as i32 == y;
                if !here || info.rock {
                    return None;
                }
                Some(Stood {
                    model: self.models.model_of(p.model)?,
                    at,
                    turn: p.angle,
                    scale: f32::from(p.scale) / 100.0,
                    tint: p.tint,
                    tree: info.class == 0,
                })
            })
    }

    /// Whether a building (or the margin round one) covers `tile`.
    pub fn occupied_at(&self, tile: (i32, i32)) -> bool {
        let n = self.tiles as i32;
        let (x, y) = tile;
        if !(0..n).contains(&x) || !(0..n).contains(&y) {
            return false;
        }
        let i = y as usize * n as usize + x as usize;
        self.occupied[i / 64] >> (i % 64) & 1 != 0
    }

    /// Put every felled tree and bush back (a new match on the same map).
    pub fn regrow(&mut self) {
        self.felled.fill(0);
        self.felled_chunks.fill(false);
    }

    /// The extra models to draw with the next frames, replacing the last
    /// list. Those whose model the pack lacks are dropped.
    pub fn set_extras(&mut self, extras: impl Iterator<Item = Extra>) {
        // A stump is hidden under a building, as the scenery is.
        let mut kept = std::mem::take(&mut self.extras);
        kept.clear();
        kept.extend(
            extras
                .filter(|e| {
                    let stump = e.model.is_stump();
                    let tile = (e.at.x.floor() as i32, e.at.z.floor() as i32);
                    self.models.id(e.model).is_some() && !(stump && self.occupied_at(tile))
                })
                .take(MAX_EXTRAS),
        );
        self.extras = kept;
    }

    /// How tall `model` is as baked, in tiles (0 if the pack lacks it).
    pub fn height_of(&self, model: Model) -> f32 {
        self.models
            .id(model)
            .map_or(0.0, |id| self.models.info[usize::from(id)].height)
    }

    /// Whether [`Scenery::frame`] draws `model` as an extra: the pack has
    /// it and this map has scenery at all (else the game draws a box).
    pub fn draws(&self, model: Model) -> bool {
        self.models.id(model).is_some() && !self.instances.is_empty()
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
            felled,
            felled_chunks,
            extras,
            extra_bins,
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
            let felling = felled_chunks[n.chunk as usize];
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
                    // Cleared ground keeps its rocks and grass only.
                    if felling && class < 2 && !models.info[usize::from(p.model)].rock {
                        let tx = ox + u32::from(p.x) / STEPS as u32;
                        let ty = oy + u32::from(p.y) / STEPS as u32;
                        let i = ty as usize * *tiles as usize + tx as usize;
                        if felled[i / 64] >> (i % 64) & 1 != 0 {
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
        // Extras first: few, and they matter for play. Each one the camera
        // or the sun sees goes in its model's bin.
        extra_bins.clear();
        for (k, n) in extras.iter().enumerate() {
            let Some(id) = models.id(n.model).map(usize::from) else {
                continue;
            };
            let info = &models.info[id];
            let d = Vec2::new(n.at.x, n.at.z).distance(focus);
            // A tree is only drawn within the scenery's reach, as the
            // static ones are, so one felled far off does not pop in.
            if info.class == 0 && d > reach[0] {
                continue;
            }
            // A sphere round the whole model, from its base to its top;
            // tipped over, it reaches as far sideways as it was tall.
            let (radius, half) = (info.radius, info.height / 2.0);
            let centre = n.at + Vec3::Y * half * n.scale;
            let round = if n.lean > 0 {
                radius + 2.0 * half
            } else {
                radius + half
            };
            if !cull.sees(centre, round * n.scale) {
                continue;
            }
            let bin = id * LODS + usize::from(lod_for(usize::from(info.class), d.hypot(distance)));
            counts[bin] += 1;
            total += 1;
            extra_bins.push((bin, k));
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
        for &(bin, k) in extra_bins.iter() {
            let n = &extras[k];
            let slot = cursor[bin] as usize;
            cursor[bin] += 1;
            let [r, g, b] = n.tint;
            instances[slot] = SceneryInstance {
                pos: [n.at.x, n.at.z],
                facing: facings[usize::from(n.turn)],
                scale: n.scale,
                tint: [r, g, b, n.phase],
                kind: models.info[bin / LODS].detail
                    | u32::from(n.lean) << 8
                    | u32::from(n.fall) << 16,
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
                    kind: models.info[bin / LODS].detail,
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
    /// `OPEN`, `STEEP`, `CLIFF` or `FOOT`.
    ground: u8,
    shore: bool,
}

struct Placer<'a> {
    models: &'a SceneryModels,
    out: Out,
    origin: (u32, u32),
    /// Extra cliff rocks this chunk may still place.
    cliff_left: u32,
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

    /// Extra rocks on a cliff or steep flank, visual only: a few on a steep
    /// flank, more on a cliff face and most at a cliff's foot. Never on flat
    /// ground, by the shore, or near a site (`clear`), and at most the
    /// chunk's `cliff_left`. The rng is drawn only on steep tiles, so flat
    /// ground places exactly what it did before.
    fn cliff_rocks(&mut self, t: &Tile, clear: bool, rng: &mut Rng) {
        if clear || t.shore || t.ground == OPEN || self.cliff_left == 0 {
            return;
        }
        let per_tile = match t.ground {
            STEEP => FLANK_ROCKS,
            FOOT => FOOT_ROCKS,
            _ => FACE_ROCKS,
        };
        let count = rng.count(per_tile).min(self.cliff_left);
        for _ in 0..count {
            if let Some(m) = self.pick(CLIFF_ROCKS, rng) {
                self.put(m, t, rng, 0.9, 1.6, 0.1);
                self.cliff_left -= 1;
            }
        }
    }

    /// One tile's scenery.
    fn tile(&mut self, t: &Tile, cover: u8, thicket: bool, clear: bool, rng: &mut Rng) {
        self.cliff_rocks(t, clear, rng);
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
        cliff_left: CLIFF_ROCKS_PER_CHUNK,
    };
    let flat = t.heights.len() != (n * n) as usize;
    let tile_height = |x: u32, y: u32| f32::from(t.heights[(y * n + x) as usize]);
    let units = gc_sim::terrain::HEIGHT_UNITS as f32;
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for y in oy..(oy + CHUNK).min(n) {
        for x in ox..(ox + CHUNK).min(n) {
            let i = (y * n + x) as usize;
            let mut slope = 0.0f32;
            let mut ground = t.ground.get(i).copied().unwrap_or(OPEN);
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
                    let nground = t.ground.get((ny as u32 * n + nx as u32) as usize);
                    if ground == OPEN && nground == Some(&CLIFF) && (nh - h) / units > FOOT_RISE {
                        ground = FOOT;
                    }
                    shore |= nh < f32::from(t.water);
                }
            }
            let land = t.land.get(i).map_or(Land::Meadow, |&b| Land::from_u8(b));
            let tile = Tile {
                x,
                y,
                set: set_of(land),
                slope,
                ground,
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
    // Scenery on cliffs stands on the renderer's relief, up to this much
    // higher than the heights here.
    let cliffs = (oy..(oy + CHUNK).min(n)).any(|y| {
        (ox..(ox + CHUNK).min(n)).any(|x| t.ground.get((y * n + x) as usize) == Some(&CLIFF))
    });
    let top = hi + height + if cliffs { gc_render::RELIEF_MAX } else { 0.0 };
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

    /// The tiles' nav ground, as gc_map would class it, from tile heights:
    /// a tile with a neighbour 1.2 tiles a tile different is a cliff, over
    /// 0.6 steep.
    fn ground_of(heights: &[u16]) -> Vec<u8> {
        let units = gc_sim::terrain::HEIGHT_UNITS as f32;
        let at = |x: i32, y: i32| f32::from(heights[(y as u32 * N + x as u32) as usize]) / units;
        (0..N as i32 * N as i32)
            .map(|i| {
                let (x, y) = (i % N as i32, i / N as i32);
                let mut slope = 0.0f32;
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if (0..N as i32).contains(&nx) && (0..N as i32).contains(&ny) {
                        slope = slope.max((at(nx, ny) - at(x, y)).abs());
                    }
                }
                if slope >= 1.2 {
                    CLIFF
                } else if slope >= 0.6 {
                    STEEP
                } else {
                    OPEN
                }
            })
            .collect()
    }

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
                    ground: ground_of(&self.heights),
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

    /// The tiles with a crag or slab, by position (cliff rocks only: meadow
    /// grows boulders and nothing else of these two).
    fn crags_and_slabs(items: &[([f32; 2], &str)]) -> Vec<[f32; 2]> {
        items
            .iter()
            .filter(|(_, n)| *n == "crag" || *n == "slab")
            .map(|(p, _)| *p)
            .collect()
    }

    /// The flat world with the ground stepping up `rise` tiles a tile at
    /// column 32.
    fn ridge(rise: f32) -> World {
        let mut w = World::new();
        let step = (rise * gc_sim::terrain::HEIGHT_UNITS as f32) as u16;
        for y in 0..N {
            for x in 32..N {
                w.heights[(y * N + x) as usize] = 1000 + step;
            }
        }
        w
    }

    #[test]
    fn cliff_rocks_only_where_the_ground_is_steep() {
        let flat = all(&World::new().build(7, &models()));
        assert!(crags_and_slabs(&flat).is_empty());
        // A gentle slope (0.4 a tile) is not steep enough.
        let gentle = all(&ridge(0.4).build(7, &models()));
        assert!(crags_and_slabs(&gentle).is_empty());
        // A cliff (2 tiles a tile) gets rocks at its foot and face only.
        let cliff = all(&ridge(2.0).build(7, &models()));
        let placed = crags_and_slabs(&cliff);
        assert!(placed.len() > 20, "{} cliff rocks", placed.len());
        assert!(placed.iter().all(|[x, _]| (31.0..33.0).contains(x)));
        // No chunk gets more than its budget.
        let mut per_chunk = [0u32; 16];
        for [x, y] in &placed {
            per_chunk[(*y as usize / 16) * 4 + *x as usize / 16] += 1;
        }
        // Boulders are a fifth of the cliff rocks and not counted here, so
        // a full chunk shows as its crags and slabs, a good few of them.
        assert!(per_chunk.iter().all(|&c| c <= CLIFF_ROCKS_PER_CHUNK));
        assert!(per_chunk.iter().any(|&c| c >= 12), "{per_chunk:?}");
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
        assert!(s.draws(Model::GrubSeep) && s.draws(Model::MarrowVein));
        let focus = Vec2::new(20.0, 20.0);
        let before = s.frame(focus, 10.0, true, &Cull::EVERYTHING).0.len();
        let look = |x: f32, model, scale| Extra {
            model,
            at: Vec3::new(x, 0.0, 20.0),
            turn: 9,
            scale,
            tint: [140; 3],
            phase: 9,
            lean: 0,
            fall: 0,
        };
        s.set_extras(
            [
                look(20.0, Model::GrubSeep, 1.0),
                look(24.0, Model::MarrowVein, 0.4),
            ]
            .into_iter(),
        );
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
        bare.set_extras([look(20.0, Model::GrubSeep, 1.0)].into_iter());
        assert!(!bare.draws(Model::GrubSeep) && bare.extras.is_empty());
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
        s.felled_chunks = vec![false; reps];
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

    #[test]
    fn felled_tiles_lose_their_trees_and_come_back_with_a_new_match() {
        let mut w = World::new();
        w.cover = vec![2; w.cover.len()];
        let mut s = w.build(7, &models());
        let focus = Vec2::new(20.0, 20.0);
        let inside = |i: &SceneryInstance| {
            (14.0..20.0).contains(&i.pos[0]) && (14.0..20.0).contains(&i.pos[1])
        };
        let before = s.frame(focus, 10.0, true, &Cull::EVERYTHING).0.to_vec();
        assert!(before.iter().any(inside));
        let tiles = (14..20).flat_map(|y| (14..20).map(move |x| (x, y)));
        s.fell(tiles.chain([(-3, 4), (9999, 2)]));
        let after = s.frame(focus, 10.0, true, &Cull::EVERYTHING).0.to_vec();
        assert!(after.len() < before.len());
        // Only the trees and bushes went: what stands elsewhere is untouched.
        assert_eq!(
            after.len(),
            before.len() - before.iter().filter(|i| inside(i)).count()
                + after.iter().filter(|i| inside(i)).count()
        );
        s.regrow();
        assert_eq!(
            s.frame(focus, 10.0, true, &Cull::EVERYTHING).0.len(),
            before.len()
        );
    }

    #[test]
    fn standing_finds_the_trees_on_a_tile_until_it_is_felled() {
        let mut w = World::new();
        w.cover = vec![2; w.cover.len()];
        let mut s = w.build(7, &models());
        let tiles = (14..20).flat_map(|y| (14..20).map(move |x| (x, y)));
        let found: Vec<_> = tiles
            .clone()
            .flat_map(|t| s.standing(t).map(move |st| (t, st)))
            .collect();
        assert!(found.iter().any(|(_, st)| st.tree), "no trees in a wood");
        for &(t, st) in &found {
            assert_eq!((st.at.x.floor() as i32, st.at.y.floor() as i32), t);
            assert!(s.height_of(st.model) > 0.0);
        }
        s.fell(tiles.clone());
        assert!(
            tiles
                .flat_map(|t| s.standing(t).collect::<Vec<_>>())
                .next()
                .is_none()
        );
        // Off the map, nothing stands.
        assert_eq!(
            s.standing((-1, 3)).count() + s.standing((9999, 3)).count(),
            0
        );
    }

    #[test]
    fn a_tipped_extra_carries_its_lean_in_the_kind() {
        let w = World::new();
        let models = models();
        let mut s = w.build(7, &models);
        let focus = Vec2::new(20.0, 20.0);
        let tip = |lean| Extra {
            model: Model::OakA,
            at: Vec3::new(20.0, 0.0, 20.0),
            turn: 0,
            scale: 1.0,
            tint: [128; 3],
            phase: 0,
            lean,
            fall: 64,
        };
        let kind_of = |s: &mut Scenery| {
            let (i, _) = s.frame(focus, 10.0, true, &Cull::EVERYTHING);
            i.iter()
                .find(|i| i.pos == [20.0, 20.0])
                .map(|i| i.kind)
                .unwrap()
        };
        s.set_extras([tip(0)].into_iter());
        let upright = kind_of(&mut s);
        s.set_extras([tip(200)].into_iter());
        let down = kind_of(&mut s);
        assert_eq!(upright >> 8 & 255, 0);
        assert_eq!(down >> 8 & 255, 200);
        assert_eq!(down >> 16, 64);
        assert_eq!(down & 255, upright & 255);
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
        let cliffs = crags_and_slabs(&all(&s)).len();
        eprintln!(
            "512 tiles: {count} instances ({cliffs} cliff rocks), {:.1} MB; 896 tiles about {:.1} MB",
            bytes as f64 / 1e6,
            big / 1e6
        );
        assert!(count > 100_000);
        assert!(big < 40e6, "{:.1} MB", big / 1e6);
        // The cliff rocks stay within their budget: a chunk each at most.
        let chunks = (512 / CHUNK) as usize * (512 / CHUNK) as usize;
        assert!(cliffs <= chunks * CLIFF_ROCKS_PER_CHUNK as usize);
    }
}
