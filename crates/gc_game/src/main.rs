//! Grubcraft. M1 sandbox: a rendered map with four players' regiments driven
//! by the deterministic simulation. You command player 1 (red) to start
//! with; the other three wander the map on their own (`--sandbox` starts on
//! an empty field instead, with nobody moving unless you order it).
//!
//! Sandbox: Tab switches which side you command (all four can be ordered).
//! B turns placing on and off: left-click puts down a regiment of the chosen
//! type for that side, facing away from the camera, and [ and ] choose the
//! type. Delete (or Backspace) takes the selected regiments off the field;
//! Shift + Delete clears it. P pauses; - and = slow the game down to a tenth
//! or speed it up to four times. T opens the stats panel for the chosen type:
//! Up and Down pick a stat, Left and Right change it (Shift for ten steps),
//! and regiments on the field change at once. R reloads `data/units.ron`
//! when the game runs from the repository. H hides the sandbox panel; C
//! shows the credits for the art. I hands the side you command to the
//! battle AI (Recruit, then Veteran, then General, then back to you); the
//! panel shows what each computer player is doing and expects.
//!
//! Soldiers are low-poly models (baked from free packs by `gc_artbake`) that
//! walk, run, fight, shoot and fall with their own animations; parts of each
//! take the team colour. Zoom far out and the Commander View replaces them
//! with one block and banner per regiment.
//!
//! Orders: left-click one of your regiments to select it, or left-drag a box around
//! several; Shift adds or removes. Right-click the ground to march the
//! selection there side by side, facing the way they travel; right-drag to
//! draw the line they form instead (its length sets their frontage; they face
//! away from the camera side of the line); a big selection forms several
//! lines. Right-click an enemy regiment to attack it; archers (each army's
//! back two rows) also shoot enemies in range on their own, and blocks that
//! touch fight. Banners show morale: blinking amber is wavering, white is
//! routing (no orders until it rallies), grey is shattered. Regiments find
//! their own way around cliffs and deep water; a selected regiment's route
//! shows as a dotted line, and G shows the ground no regiment can cross.
//! Ctrl (or Cmd) + a number key saves the selection as a control group; the
//! number recalls it (Shift adds it), and pressing it twice quickly centres
//! the camera on it.
//!
//! The game opens on the main menu (Play, Sandbox, Settings); Esc brings it
//! back during a match. Play hosts or joins a lobby over the local network
//! (`gc_net`, see `online.rs`), with AI players at Recruit, Veteran or
//! General in any slot. Starting it opens a Field
//! Battle (`battle.rs`): ten regiments a side on the start plateaus, the
//! computer players commanding theirs, and VICTORY or DEFEAT once one side
//! is left holding the field.
//
//! Sound: effects and music play through the default output device; they
//! follow the camera (quieter and panned by distance). M mutes everything, N
//! turns the music off or on. Battle music fades in as regiments engage.
//!
//! Camera: arrow keys / screen edges / two-finger scroll pan; mouse wheel or
//! pinch zooms; Alt + middle-drag or two-finger rotate turns the view;
//! middle-click or Home resets the rotation; ` (backquote) or F11 toggles the
//! dev overlay and prints memory by subsystem;
//! Esc leaves placing, the stats panel or the credits, and otherwise opens
//! the menu.
//!
//! Flags: `--sandbox` starts on an empty field; `--battle recruit|veteran|general`
//! starts a Field Battle against one computer player of that level;
//! `--host` opens a LAN lobby at once and `--join ADDRESS` joins one; `--no-menu` skips the main menu
//! (`--frames` implies it, `--menu` overrides); `--seed N` picks the match seed; `--frames N` quits after N frames
//! and prints a final stats line (for measuring memory on real machines);
//! `--overlay` starts with the dev overlay shown; `--no-audio` never opens
//! the sound card; `--latency 1|2|3` sets
//! frames in flight; `--render-scale F` renders at a
//! fraction of the window's pixels; `--draw all|terrain|blocks|none` turns
//! parts of the frame off to measure what they cost.

mod battle;
mod camera;
mod credits;
mod data;
#[cfg(test)]
mod goldens;
mod lens;
mod models;
mod online;
mod profile;
mod puppets;
mod sandbox;
mod sounds;
mod terrain_gen;

use std::sync::Arc;
use std::time::{Duration, Instant};

use battle::{Battle, Party, party_name};
use gc_ai::{Difficulty, Mind, MindConfig, Minds};
use gc_net::Lan;
use gc_platform::tagged_alloc::{self, Tag, TaggedAlloc};
use gc_render::{
    BoxInstance, FrameParams, Renderer, RendererOptions, SoldierBatch, SoldierInstance, Terrain,
    UiQuad,
};
use gc_sim::formation::{self, GroupMember, GroupOrder};
use gc_sim::scenario::Drill;
use gc_sim::{
    Capacities, MatchSetup, MoraleState, PlayerCommand, PlayerId, RegimentHandle, UnitId,
    UnitStats, World,
};
use gc_ui::lobby::MatchStart;
use gc_ui::{Menu, MenuAction, Network, Screen, Settings, Ui, UiInput};
use glam::Vec2;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use camera::RtsCamera;
use models::Action;
use puppets::{Block, Puppets, Soldier};
use terrain_gen::Heightmap;

#[global_allocator]
static ALLOC: TaggedAlloc = TaggedAlloc;

const MAP_TILES: u32 = 256;
const TICK: Duration = Duration::from_millis(1000 / gc_sim::TICKS_PER_SECOND as u64);
const HUMAN: PlayerId = 0;
/// Team colours: Ember red, Brood violet, Choir cyan, Concord bone.
const TEAM_COLORS: [[u8; 3]; 4] = [
    [178, 38, 30],
    [104, 52, 140],
    [52, 160, 186],
    [214, 204, 170],
];
const EDGE_SCROLL_PX: f64 = 6.0;
/// Height of a placeholder box soldier, for unit types without a model.
const SOLDIER_HEIGHT: f32 = 0.95;
/// Soldiers further than this from the camera, in tiles, use the reduced mesh.
const LOD_DISTANCE: f32 = 70.0;
/// Commander View: past this camera distance soldiers give way to one
/// banner block per regiment, and come back below the second distance.
const COMMANDER_IN: f32 = 175.0;
const COMMANDER_OUT: f32 = 160.0;
/// Commander View block height and banner, in tiles.
const COMMANDER_BLOCK_HEIGHT: f32 = 0.6;
const COMMANDER_POLE: [f32; 3] = [0.3, 5.0, 0.3];
const COMMANDER_FLAG: [f32; 3] = [2.6, 1.8, 0.25];
/// A regiment whose volley left this many ticks ago or less is shooting.
const SHOOTING_TICKS: u32 = 24;
/// Thickness of the ground plate drawn under a selected regiment.
const PLATE_HEIGHT: f32 = 0.05;
/// A right-drag shorter than this on the ground is a click.
const MIN_LINE_TILES: f32 = 2.0;
/// A left-drag shorter than this on screen is a click.
const MIN_BOX_PIXELS: f32 = 6.0;
/// Two presses of a control-group key within this time centre the camera.
const DOUBLE_TAP: Duration = Duration::from_millis(350);
/// A right-click this close to an enemy block's edge, in tiles, attacks it.
const ATTACK_PICK_TILES: f32 = 1.5;
/// How long the marker under an attacked regiment shows.
const ATTACK_MARK: Duration = Duration::from_millis(700);
/// Banner over a shaken regiment: wavering (blinking amber), routing (white)
/// or shattered (grey). Height above the ground and size, in tiles.
const BANNER_LIFT: f32 = 1.9;
const BANNER_SIZE: [f32; 3] = [0.12, 0.8, 0.7];
/// Fallen soldiers kept on the field at once.
const MAX_CORPSES: usize = 8192;
/// Dots along a selected regiment's route: spacing and size, in tiles.
const ROUTE_DOT_SPACING: f32 = 1.2;
const ROUTE_DOT: [f32; 3] = [0.22, 0.04, 0.22];
const ROUTE_END: [f32; 3] = [0.7, 0.05, 0.7];
const MAX_ROUTE_DOTS: usize = 400;
/// Colour of ground no regiment can cross, when shown (G).
const BLOCKED_COLOR: [u8; 4] = [70, 16, 22, 0];
const BLOCKED_MARK: [f32; 3] = [0.3, 0.04, 0.3];
/// Raised a little so steep ground doesn't hide it.
const BLOCKED_LIFT: f32 = 0.1;
/// Arrows drawn per volley, and how far apart they fly, in tiles.
const ARROWS_PER_VOLLEY: usize = 6;
const ARROW_SPREAD: f32 = 1.6;
/// Arrows leave the bow and land at about this height above the ground.
const ARROW_HEIGHT: f32 = 0.9;
/// Peak of an arrow's arc as a fraction of the distance it flies.
const ARROW_ARC: f32 = 0.22;

/// Game speeds the sandbox steps through with - and =, and the normal one.
const SPEEDS: [f32; 6] = [0.1, 0.25, 0.5, 1.0, 2.0, 4.0];
const NORMAL_SPEED: usize = 3;
/// How long a sandbox message (such as the result of reloading data) shows.
const MESSAGE_TIME: Duration = Duration::from_secs(5);
/// Sides in the sandbox and the drill; Tab cycles through them.
const PLAYERS: u8 = 4;
const TEAM_NAMES: [&str; 4] = ["red", "violet", "cyan", "bone"];
/// Most ticks run in one frame, so a slow frame can't snowball (4x speed at
/// 30 fps needs about 3).
const MAX_TICKS_PER_FRAME: u32 = 8;

const _: () = assert!(gc_data::MAX_MODELS == gc_sim::formation::MAX_MODELS);

struct Options {
    seed: u64,
    /// Start on an empty field with nobody moving on their own.
    sandbox: bool,
    frames: Option<u64>,
    overlay: bool,
    /// Whether to open on the main menu. Defaults to yes, except for
    /// `--frames` runs, which measure the sandbox itself.
    menu: Option<bool>,
    audio: bool,
    /// Start straight into a Field Battle against one AI of this level.
    battle: Option<u8>,
    /// Open a LAN lobby at once (`--host`), or join one (`--join ADDRESS`
    /// or join code), with `--password` if it has one.
    host: bool,
    join: Option<String>,
    password: String,
    render: RendererOptions,
}

fn parse_args() -> Options {
    let mut opts = Options {
        seed: 0x6772_7562,
        sandbox: false,
        frames: None,
        overlay: false,
        menu: None,
        audio: true,
        battle: None,
        host: false,
        join: None,
        password: String::new(),
        render: RendererOptions::default(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--overlay" {
            opts.overlay = true;
            continue;
        }
        if flag == "--menu" || flag == "--no-menu" {
            opts.menu = Some(flag == "--menu");
            continue;
        }
        if flag == "--sandbox" {
            opts.sandbox = true;
            continue;
        }
        if flag == "--host" {
            opts.host = true;
            continue;
        }
        if flag == "--no-audio" {
            opts.audio = false;
            continue;
        }
        let value = args.next().unwrap_or_default();
        let ok = match flag.as_str() {
            "--seed" => value.parse().map(|v| opts.seed = v).is_ok(),
            "--join" => {
                opts.join = Some(value);
                true
            }
            "--password" => {
                opts.password = value;
                true
            }
            "--frames" => value.parse().map(|v| opts.frames = Some(v)).is_ok(),
            "--battle" => {
                let level = gc_ui::lobby::AI_LEVELS
                    .iter()
                    .position(|l| l.eq_ignore_ascii_case(&value));
                level.map(|l| opts.battle = Some(l as u8)).is_some()
            }
            "--latency" => match value.parse() {
                Ok(v @ 1..=3) => {
                    opts.render.frame_latency = v;
                    true
                }
                _ => false,
            },
            "--render-scale" => match value.parse::<f32>() {
                Ok(v) if (0.25..=1.0).contains(&v) => {
                    opts.render.render_scale = v;
                    true
                }
                _ => false,
            },
            "--draw" => {
                let (terrain, blocks) = match value.as_str() {
                    "all" => (true, true),
                    "terrain" => (true, false),
                    "blocks" => (false, true),
                    "none" => (false, false),
                    _ => usage(),
                };
                opts.render.draw_terrain = terrain;
                opts.render.draw_blocks = blocks;
                true
            }
            _ => false,
        };
        if !ok {
            usage();
        }
    }
    opts
}

fn usage() -> ! {
    eprintln!(
        "usage: grubcraft [--menu|--no-menu] [--sandbox] [--battle recruit|veteran|general] [--host] [--join ADDRESS] [--seed N] [--frames N] [--overlay] [--no-audio] [--latency 1|2|3] \
         [--render-scale 0.25..1] [--draw all|terrain|blocks|none]"
    );
    std::process::exit(2);
}

/// Everything that exists once the window and GPU are up.
struct Running {
    window: Arc<Window>,
    renderer: Renderer,
    camera: RtsCamera,
    heightmap: Heightmap,
    world: World,
    /// Orders the other sides about; None in an empty-field sandbox or a
    /// lobby battle.
    drill: Option<Drill>,
    /// The computer players (lobby AI slots, and sides handed over with I).
    minds: Minds,
    /// Who plays each side, by party id.
    parties: Vec<Party>,
    /// A lobby Field Battle under way; None in the sandbox and the drill.
    battle: Option<Battle>,
    /// Matches started this run, so each gets its own AI personalities.
    matches: u64,
    /// Commands collected for the next tick (from the drill, the minds and
    /// the human).
    pending: Vec<PlayerCommand>,
    /// Orders from this machine's player, and a new match's deployment.
    human_orders: Vec<PlayerCommand>,
    /// Position and facing at the previous tick, by pool index, for interpolation.
    prev_positions: Vec<Option<(RegimentHandle, Vec2, Vec2)>>,
    instances: Vec<BoxInstance>,
    puppets: Puppets,
    /// Soldier models; None if the embedded pack failed to load (boxes then).
    art: Option<Art>,
    /// Soldiers have given way to one banner block per regiment (zoomed out).
    commander: bool,
    /// Regiments that loosed a volley lately, by pool index.
    shooting: Vec<bool>,
    /// The match's regiment types, as the simulation sees them, and their names.
    unit_stats: Vec<UnitStats>,
    /// The regiment types as built, before any sandbox tuning.
    data_stats: Vec<UnitStats>,
    unit_names: Vec<String>,
    /// Each regiment type's faction, as the AI names them.
    unit_factions: Vec<gc_ai::Faction>,
    sandbox: Sandbox,
    selected: Vec<RegimentHandle>,
    /// Scratch space for building group orders.
    group: Vec<GroupMember>,
    group_commands: Vec<gc_sim::Command>,
    /// Regiments inside a box select, live while dragging.
    boxed: Vec<RegimentHandle>,
    control_groups: [Vec<RegimentHandle>; 10],
    last_group_key: Option<(usize, Instant)>,
    /// The enemy regiment last ordered attacked, marked on the ground briefly.
    attack_mark: Option<(RegimentHandle, Instant)>,
    sim_clock: Duration,
    /// Game speed, an index into `SPEEDS`, and whether the game is paused (P).
    speed: usize,
    paused: bool,
    last_frame: Instant,
    started: Instant,
    input: Input,
    stats: Stats,
    memory_timeline: MemoryTimeline,
    /// Whether the dev overlay is showing.
    overlay: bool,
    /// Whether the AI lens is showing (L).
    lens: bool,
    /// Marks over ground no regiment can cross, and whether to show them (G).
    blocked_marks: Vec<BoxInstance>,
    show_blocked: bool,
    /// Data hash and size, for the overlay.
    data_label: String,
    seed: u64,
    capacities: Capacities,
    /// Menus: the screens, the immediate-mode UI they draw with, and the
    /// mouse and keys collected since the last frame.
    menu: Menu<Lan>,
    ui: Ui,
    ui_input: UiInput,
    ui_quads: Vec<UiQuad>,
    menu_open: bool,
    /// Whether a match has started (the sim is frozen until then).
    match_started: bool,
    /// A match over the network, whose turns come from the host.
    online: Option<online::Online>,
    quit: bool,
    /// None with `--no-audio` or when there is no sound output.
    sounds: Option<sounds::Sounds>,
}

/// Soldier models and the lists that draw them, rebuilt every frame.
struct Art {
    clips: models::Clips,
    /// The model of each unit type, if it has one.
    unit_model: Vec<Option<usize>>,
    /// Instances per model and level of detail (model * 2 + lod).
    buckets: Vec<Vec<SoldierInstance>>,
    soldiers: Vec<SoldierInstance>,
    batches: Vec<SoldierBatch>,
}

impl Art {
    fn clear(&mut self) {
        for b in &mut self.buckets {
            b.clear();
        }
    }

    /// Gather the buckets into one instance list with a batch per bucket.
    fn finish(&mut self) {
        self.soldiers.clear();
        self.batches.clear();
        for (k, b) in self.buckets.iter().enumerate() {
            if b.is_empty() {
                continue;
            }
            self.batches.push(SoldierBatch {
                model: (k / 2) as u16,
                lod: (k % 2) as u8,
                first: self.soldiers.len() as u32,
                count: b.len() as u32,
            });
            self.soldiers.extend_from_slice(b);
        }
    }
}

impl Sandbox {
    fn new(placing: bool) -> Sandbox {
        Sandbox {
            side: HUMAN,
            placing,
            unit: 0,
            tuning: false,
            row: 0,
            panel: true,
            credits: false,
            message: None,
            dirty: true,
        }
    }
}

/// What a soldier is doing, from his puppet and his regiment.
fn soldier_action(s: &Soldier, block: &Block, shooting: bool) -> Action {
    if block.routing {
        Action::Run
    } else if s.striking && !s.dueling && s.phase > 0.6 {
        // Behind the fighting line, some wait their turn.
        Action::Idle
    } else if s.striking {
        if s.phase < 0.5 {
            Action::Attack
        } else {
            Action::Attack2
        }
    } else if s.speed > block.speed * 1.25 + 0.3 {
        Action::Run
    } else if s.speed > 0.25 {
        Action::Walk
    } else if shooting {
        Action::Shoot
    } else {
        Action::Idle
    }
}

/// What the sandbox tools are doing.
struct Sandbox {
    /// The side you select and order (Tab).
    side: PlayerId,
    /// Left-click places regiments of `unit` instead of selecting (B).
    placing: bool,
    /// The type placed and tuned ([ and ]).
    unit: UnitId,
    /// The stats panel is open (T), with this row chosen.
    tuning: bool,
    row: usize,
    /// The sandbox panel is showing (H).
    panel: bool,
    /// The credits are showing (C), in place of the panel.
    credits: bool,
    message: Option<(String, Instant)>,
    /// The panel needs redrawing.
    dirty: bool,
}

/// The sandbox's and the drill's four sides, named by colour.
fn sandbox_parties() -> Vec<Party> {
    (0..PLAYERS as usize)
        .map(|p| Party {
            name: TEAM_NAMES[p].to_string(),
            color: TEAM_COLORS[p],
            faction: None,
        })
        .collect()
}

/// A side's colour (a side past the party list borrows a team colour).
fn color_of(parties: &[Party], owner: PlayerId) -> [u8; 3] {
    parties
        .get(owner as usize)
        .map_or(TEAM_COLORS[owner as usize % TEAM_COLORS.len()], |p| p.color)
}

/// What kind of match to start from the menu. (The scripted four-army
/// drill runs only when the game starts straight into a match.)
enum MatchKind {
    /// An empty field, nobody moving unless ordered.
    Sandbox,
    /// A Field Battle from the lobby.
    Battle(MatchStart),
}

/// Process memory recorded at startup milestones, to see what each costs.
#[derive(Default)]
struct MemoryTimeline(Vec<(&'static str, Option<u64>)>);

impl MemoryTimeline {
    fn mark(&mut self, stage: &'static str) {
        self.0
            .push((stage, gc_platform::memory::process_memory().current));
    }

    fn print(&self) {
        eprintln!("process memory by stage (MB, change from previous):");
        let mut prev: Option<u64> = None;
        for (stage, bytes) in &self.0 {
            match (bytes, prev) {
                (Some(b), Some(p)) => {
                    let delta = (*b as f64 - p as f64) / (1024.0 * 1024.0);
                    eprintln!("  {stage:<26} {:>8.1}  ({delta:+.1})", mb(*b as usize));
                }
                (Some(b), None) => eprintln!("  {stage:<26} {:>8.1}", mb(*b as usize)),
                (None, _) => eprintln!("  {stage:<26}        ?"),
            }
            prev = *bytes;
        }
    }
}

#[derive(Default)]
struct Input {
    cursor: Option<PhysicalPosition<f64>>,
    focused: bool,
    alt: bool,
    shift: bool,
    /// Ctrl, or Cmd on a Mac.
    command: bool,
    arrows: [bool; 4], // up, down, left, right
    /// Where a left-drag (box select) started.
    left_down: Option<PhysicalPosition<f64>>,
    /// Where a right-drag (line order) started: the cursor and the ground under it.
    right_down: Option<(PhysicalPosition<f64>, Vec2)>,
    middle_down: Option<PhysicalPosition<f64>>,
    middle_dragged: bool,
}

struct Stats {
    frames: u64,
    window_frames: u32,
    window_start: Instant,
    /// Longest gap between frames in the current window.
    frame_max: Duration,
    tick_time: Duration,
    tick_max: Duration,
    ticks: u32,
    /// Time the computer players spent thinking in this window.
    ai_time: Duration,
}

#[derive(Default)]
struct App {
    opts: Option<Options>,
    running: Option<Running>,
    memory_timeline: MemoryTimeline,
}

impl App {
    fn start(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn std::error::Error>> {
        let opts = self.opts.as_ref().expect("options");
        let attrs = Window::default_attributes()
            .with_title("Grubcraft")
            .with_inner_size(LogicalSize::new(1280.0, 760.0));
        let window = Arc::new(event_loop.create_window(attrs)?);
        let size = window.inner_size();
        self.memory_timeline.mark("window open");

        let mut renderer = tagged_alloc::scope(Tag::Render, || {
            pollster::block_on(Renderer::new(
                window.clone(),
                size.width,
                size.height,
                opts.render,
            ))
        })?;
        self.memory_timeline.mark("gpu device + pipelines");
        let heightmap = tagged_alloc::scope(Tag::Map, || Heightmap::generate(opts.seed, MAP_TILES));
        tagged_alloc::scope(Tag::Render, || {
            renderer.set_terrain(&Terrain {
                size: heightmap.size,
                samples_per_tile: heightmap.samples_per_tile,
                max_height: heightmap.max_height,
                water_level: heightmap.water_level,
                heights: &heightmap.heights,
            })
        });
        self.memory_timeline.mark("map generated + uploaded");

        let game_data = data::load();
        let art = match models::load() {
            Ok(models) => match renderer.set_models(&models) {
                Ok(()) => Some(Art {
                    clips: models::Clips::new(&models),
                    unit_model: game_data
                        .units
                        .iter()
                        .map(|u| models.by_name(u.name()))
                        .collect(),
                    buckets: vec![Vec::new(); models.models.len() * 2],
                    soldiers: Vec::with_capacity(gc_render::MAX_SOLDIER_INSTANCES),
                    batches: Vec::with_capacity(models.models.len() * 2),
                }),
                Err(e) => {
                    eprintln!("grubcraft: soldier models off: {e}");
                    None
                }
            },
            Err(e) => {
                eprintln!("grubcraft: soldier models off: {e}");
                None
            }
        };
        self.memory_timeline.mark("models uploaded");
        let unit_stats: Vec<UnitStats> = game_data.units.iter().map(sandbox::stats_of).collect();
        let unit_names: Vec<String> = game_data
            .units
            .iter()
            .map(|u| u.name().to_string())
            .collect();
        let unit_factions: Vec<gc_ai::Faction> = game_data
            .units
            .iter()
            .map(|u| gc_ai::Faction::from_index(u.faction() as u8))
            .collect();
        let capacities = Capacities::default();
        let nav = tagged_alloc::scope(Tag::Map, || heightmap.nav_costs());
        let (heights, water_level) = tagged_alloc::scope(Tag::Map, || heightmap.sim_ground());
        let blocked_marks = blocked_marks(&nav, MAP_TILES);
        let (world, prev_positions, pending) = tagged_alloc::scope(Tag::Sim, || {
            (
                World::new(&MatchSetup {
                    seed: opts.seed,
                    capacities,
                    units: &unit_stats,
                    map_tiles: MAP_TILES,
                    nav: &nav,
                    teams: &[],
                    ground: gc_sim::Ground {
                        heights: &heights,
                        water_level,
                    },
                }),
                vec![None; capacities.regiments as usize],
                Vec::with_capacity(1024),
            )
        });
        let drill = (!opts.sandbox).then(|| make_drill(opts.seed));
        let minds = tagged_alloc::scope(Tag::Ai, || Minds::new(heightmap.ai_terrain(&nav)));

        let start = Vec2::splat(MAP_TILES as f32 * 0.25);
        let mut camera = RtsCamera::new(MAP_TILES as f32, start + Vec2::new(10.0, 20.0));
        camera.focus_height = heightmap.height_at(camera.focus.x, camera.focus.y);
        camera.aspect = renderer.aspect_ratio();

        self.memory_timeline.mark("simulation world");
        let info = renderer.adapter_info();
        eprintln!(
            "grubcraft: {} via {:?}, {}x{} pixels, frame latency {}, render scale {}, draw {}",
            info.name,
            info.backend,
            size.width,
            size.height,
            opts.render.frame_latency,
            opts.render.render_scale,
            match (opts.render.draw_terrain, opts.render.draw_blocks) {
                (true, true) => "all",
                (true, false) => "terrain",
                (false, true) => "blocks",
                (false, false) => "none",
            }
        );
        eprintln!(
            "grubcraft: game data {:016x}, {} unit types ({})",
            game_data.short_hash(),
            game_data.units.len(),
            game_data
                .units
                .iter()
                .map(|u| u.name())
                .collect::<Vec<_>>()
                .join(", ")
        );

        let settings_path = gc_ui::settings::settings_path();
        let settings = settings_path
            .as_deref()
            .map_or_else(Settings::default, Settings::load);
        let lan = Lan::new(gc_net::Identity {
            version: online::version(),
            data_hash: game_data.short_hash(),
            seed: opts.seed,
            map_hash: online::map_hash(
                &nav,
                &gc_sim::Ground {
                    heights: &heights,
                    water_level,
                },
            ),
        });
        let menu = Menu::new(lan, settings, settings_path, &online::version());
        let show_menu = opts
            .menu
            .unwrap_or(opts.frames.is_none() && opts.battle.is_none());
        let sounds = if opts.audio {
            tagged_alloc::scope(Tag::Audio, || {
                sounds::Sounds::start(capacities.regiments as usize)
            })
        } else {
            None
        };

        let now = Instant::now();
        self.running = Some(Running {
            window,
            renderer,
            camera,
            heightmap,
            world,
            drill,
            minds,
            parties: sandbox_parties(),
            battle: None,
            matches: 0,
            pending,
            human_orders: Vec::with_capacity(64),
            prev_positions,
            instances: Vec::with_capacity(gc_render::MAX_BOX_INSTANCES),
            puppets: Puppets::new(capacities.regiments as usize, MAX_CORPSES),
            art,
            commander: false,
            shooting: vec![false; capacities.regiments as usize],
            data_stats: unit_stats.clone(),
            unit_stats,
            unit_names,
            unit_factions,
            sandbox: Sandbox::new(opts.sandbox),
            selected: Vec::with_capacity(64),
            group: Vec::with_capacity(64),
            group_commands: Vec::with_capacity(64),
            boxed: Vec::with_capacity(64),
            control_groups: Default::default(),
            last_group_key: None,
            attack_mark: None,
            sim_clock: Duration::ZERO,
            speed: NORMAL_SPEED,
            paused: false,
            last_frame: now,
            started: now,
            input: Input {
                focused: true,
                ..Default::default()
            },
            stats: Stats {
                frames: 0,
                window_frames: 0,
                window_start: now,
                frame_max: Duration::ZERO,
                tick_time: Duration::ZERO,
                tick_max: Duration::ZERO,
                ticks: 0,
                ai_time: Duration::ZERO,
            },
            overlay: opts.overlay,
            lens: false,
            blocked_marks,
            show_blocked: false,
            data_label: format!(
                "{:016x}   {} unit types",
                game_data.short_hash(),
                game_data.units.len()
            ),
            memory_timeline: std::mem::take(&mut self.memory_timeline),
            seed: opts.seed,
            capacities,
            menu,
            ui: Ui::new(),
            ui_input: UiInput::default(),
            ui_quads: Vec::with_capacity(512),
            menu_open: show_menu,
            match_started: !show_menu,
            online: None,
            quit: false,
            sounds,
        });
        if let Some(r) = self.running.as_mut() {
            r.menu.set_in_match(r.match_started);
            if opts.host || opts.join.is_some() {
                let me = r.menu.profile();
                let result = match &opts.join {
                    Some(addr) => r.menu.net().join(&me, addr, &opts.password),
                    None => r.menu.net().host(
                        &me,
                        gc_ui::lobby::LobbySettings::default(),
                        &opts.password,
                    ),
                };
                match result {
                    Ok(()) => r.menu.show(Screen::Lobby),
                    Err(e) => eprintln!("grubcraft: {e}"),
                }
            }
            if let Some(level) = opts.battle {
                let setup = battle::quick_battle(level, r.menu.settings());
                r.start_match(MatchKind::Battle(setup));
            }
        }
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        if let Err(e) = self.start(event_loop) {
            eprintln!("grubcraft: failed to start: {e}");
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(r) = self.running.as_mut() else {
            return;
        };
        if r.menu_open {
            match &event {
                WindowEvent::MouseInput { state, button, .. } => {
                    if *button == MouseButton::Left {
                        match state {
                            ElementState::Pressed => r.ui_input.pressed = true,
                            ElementState::Released => r.ui_input.released = true,
                        }
                    }
                    return;
                }
                WindowEvent::KeyboardInput { event: key, .. } => {
                    r.menu_key(key);
                    return;
                }
                WindowEvent::MouseWheel { .. }
                | WindowEvent::PinchGesture { .. }
                | WindowEvent::RotationGesture { .. } => return,
                _ => {}
            }
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                r.renderer.resize(size.width, size.height);
                r.camera.aspect = r.renderer.aspect_ratio();
            }
            WindowEvent::Focused(f) => r.input.focused = f,
            WindowEvent::ModifiersChanged(m) => {
                r.input.alt = m.state().alt_key();
                r.input.shift = m.state().shift_key();
                r.input.command = m.state().control_key() || m.state().super_key();
            }
            WindowEvent::CursorLeft { .. } => r.input.cursor = None,
            WindowEvent::CursorMoved { position, .. } => {
                if let (Some(prev), Some(_)) = (r.input.cursor, r.input.middle_down) {
                    let size = r.window.inner_size();
                    let dx = (position.x - prev.x) as f32 / size.width as f32;
                    let dy = (position.y - prev.y) as f32 / size.height as f32;
                    if r.input.alt {
                        r.camera.rotate(dx * 3.0);
                    } else {
                        r.camera.pan(-dx, dy);
                    }
                    r.input.middle_dragged = true;
                }
                r.input.cursor = Some(position);
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(_, y) => r.camera.zoom(0.88f32.powf(y)),
                // Trackpads send pixel deltas: two-finger scroll pans, as in
                // every Mac map app.
                MouseScrollDelta::PixelDelta(p) => {
                    let size = r.window.inner_size();
                    r.camera.pan(
                        -p.x as f32 / size.width as f32,
                        p.y as f32 / size.height as f32,
                    );
                }
            },
            WindowEvent::PinchGesture { delta, .. } => r.camera.zoom(1.0 - delta as f32),
            WindowEvent::RotationGesture { delta, .. } => r.camera.rotate(delta.to_radians()),
            WindowEvent::MouseInput { state, button, .. } => r.mouse_button(state, button),
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    if r.sandbox.tuning && down && r.tune_key(code) {
                        return;
                    }
                    // The sandbox tools would change this machine's game and
                    // nobody else's.
                    let sandbox_tool = matches!(
                        code,
                        KeyCode::KeyP
                            | KeyCode::Pause
                            | KeyCode::Minus
                            | KeyCode::NumpadSubtract
                            | KeyCode::Equal
                            | KeyCode::NumpadAdd
                            | KeyCode::Tab
                            | KeyCode::KeyI
                            | KeyCode::KeyB
                            | KeyCode::Delete
                            | KeyCode::Backspace
                            | KeyCode::KeyT
                            | KeyCode::KeyR
                    );
                    // P carries on without a player the match is paused for.
                    if code == KeyCode::KeyP
                        && down
                        && !event.repeat
                        && r.online.is_some()
                        && r.menu.network().paused_for_player()
                    {
                        r.menu.net().carry_on();
                        return;
                    }
                    if r.online.is_some() && sandbox_tool {
                        if down && !event.repeat {
                            r.say("That sandbox tool is off in a network battle.".into());
                        }
                        return;
                    }
                    match code {
                        KeyCode::ArrowUp => r.input.arrows[0] = down,
                        KeyCode::ArrowDown => r.input.arrows[1] = down,
                        KeyCode::ArrowLeft => r.input.arrows[2] = down,
                        KeyCode::ArrowRight => r.input.arrows[3] = down,
                        KeyCode::Home if down => r.camera.reset_rotation(),
                        KeyCode::F11 | KeyCode::Backquote if down && !event.repeat => {
                            r.overlay = !r.overlay;
                            r.refresh_overlay();
                            r.memory_timeline.print();
                            print_memory(&r.renderer);
                        }
                        KeyCode::KeyP | KeyCode::Pause if down && !event.repeat => {
                            r.paused = !r.paused;
                            r.sandbox.dirty = true;
                        }
                        KeyCode::Minus | KeyCode::NumpadSubtract if down => {
                            r.speed = r.speed.saturating_sub(1);
                            r.sandbox.dirty = true;
                        }
                        KeyCode::Equal | KeyCode::NumpadAdd if down => {
                            r.speed = (r.speed + 1).min(SPEEDS.len() - 1);
                            r.sandbox.dirty = true;
                        }
                        KeyCode::Tab if down && !event.repeat => r.switch_side(),
                        KeyCode::KeyI if down && !event.repeat => r.cycle_mind(),
                        KeyCode::KeyB if down && !event.repeat => {
                            r.sandbox.placing = !r.sandbox.placing;
                            r.sandbox.dirty = true;
                        }
                        KeyCode::BracketLeft if down => r.choose_unit(-1),
                        KeyCode::BracketRight if down => r.choose_unit(1),
                        KeyCode::Delete | KeyCode::Backspace if down && !event.repeat => {
                            r.remove_regiments(r.input.shift);
                        }
                        KeyCode::KeyT if down && !event.repeat => {
                            r.sandbox.tuning = !r.sandbox.tuning;
                            r.input.arrows = [false; 4];
                            r.sandbox.dirty = true;
                        }
                        KeyCode::KeyR if down && !event.repeat => r.reload_data(),
                        KeyCode::KeyC if down && !event.repeat => {
                            r.sandbox.credits = !r.sandbox.credits;
                            r.sandbox.dirty = true;
                        }
                        KeyCode::KeyH if down && !event.repeat => {
                            r.sandbox.panel = !r.sandbox.panel;
                            r.sandbox.dirty = true;
                        }
                        KeyCode::KeyG if down && !event.repeat => {
                            r.show_blocked = !r.show_blocked;
                        }
                        KeyCode::KeyL if down && !event.repeat => {
                            r.lens = !r.lens;
                            r.sandbox.dirty = true;
                        }
                        KeyCode::KeyM if down && !event.repeat => {
                            if let Some(s) = &mut r.sounds {
                                s.toggle_mute();
                            }
                        }
                        KeyCode::KeyN if down && !event.repeat => {
                            if let Some(s) = &mut r.sounds {
                                s.toggle_music();
                            }
                        }
                        KeyCode::Escape
                            if down
                                && (r.sandbox.placing || r.sandbox.tuning || r.sandbox.credits) =>
                        {
                            r.sandbox.placing = false;
                            r.sandbox.tuning = false;
                            r.sandbox.credits = false;
                            r.sandbox.dirty = true;
                        }
                        KeyCode::Escape if down && !event.repeat => r.open_menu(),
                        _ if down && !event.repeat && digit(code).is_some() => {
                            r.control_group(digit(code).unwrap());
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                r.frame();
                if r.quit {
                    event_loop.exit();
                }
                match r.stats.frames {
                    1 => r.memory_timeline.mark("frame 1"),
                    2 => r.memory_timeline.mark("frame 2"),
                    10 => r.memory_timeline.mark("frame 10"),
                    120 => r.memory_timeline.mark("frame 120"),
                    _ => {}
                }
                let limit = self.opts.as_ref().and_then(|o| o.frames);
                if limit.is_some_and(|n| r.stats.frames >= n) {
                    r.memory_timeline.mark("exit");
                    println!("{}", r.stats_line());
                    r.memory_timeline.print();
                    print_memory(&r.renderer);
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(r) = &self.running {
            r.window.request_redraw();
        }
    }
}

impl Running {
    fn mouse_button(&mut self, state: ElementState, button: MouseButton) {
        match (button, state) {
            (MouseButton::Middle, ElementState::Pressed) => {
                self.input.middle_down = self.input.cursor;
                self.input.middle_dragged = false;
            }
            (MouseButton::Middle, ElementState::Released) => {
                if !self.input.middle_dragged {
                    self.camera.reset_rotation();
                }
                self.input.middle_down = None;
            }
            (MouseButton::Left, ElementState::Pressed) => {
                self.input.left_down = self.input.cursor;
            }
            (MouseButton::Left, ElementState::Released) if self.sandbox.placing => {
                self.place_regiment();
                self.input.left_down = None;
            }
            (MouseButton::Left, ElementState::Released) => {
                if !self.input.shift {
                    self.selected.clear();
                }
                if let Some(rect) = self.selection_box() {
                    self.regiments_in_box(rect);
                    for i in 0..self.boxed.len() {
                        let h = self.boxed[i];
                        if !self.selected.contains(&h) {
                            self.selected.push(h);
                        }
                    }
                } else if let Some(h) = self
                    .ground_under_cursor()
                    .and_then(|p| self.nearest_own(p, 6.0))
                {
                    match self.selected.iter().position(|&s| s == h) {
                        Some(i) => {
                            self.selected.swap_remove(i);
                        }
                        None => self.selected.push(h),
                    }
                }
                if !self.selected.is_empty() {
                    self.ui_sound(sounds::UiSound::Select);
                }
                self.input.left_down = None;
            }
            (MouseButton::Right, ElementState::Pressed) => {
                self.input.right_down = self.input.cursor.zip(self.ground_under_cursor());
            }
            (MouseButton::Right, ElementState::Released) => {
                let drag = self.drag_order();
                let enemy = drag
                    .is_none()
                    .then(|| self.ground_under_cursor())
                    .flatten()
                    .and_then(|p| self.enemy_at(p));
                if let Some(target) = enemy {
                    if !self.selected.is_empty() {
                        self.attack_mark = Some((target, Instant::now()));
                        self.ui_sound(sounds::UiSound::Attack);
                    }
                    self.human_orders
                        .extend(self.selected.iter().map(|&regiment| PlayerCommand {
                            player: self.sandbox.side,
                            command: gc_sim::Command::Attack { regiment, target },
                        }));
                } else if let Some(order) = drag.or_else(|| {
                    self.ground_under_cursor().map(|p| GroupOrder {
                        target: fx_vec(p),
                        facing: None,
                        width: None,
                    })
                }) {
                    self.plan_order(order);
                    if !self.group_commands.is_empty() {
                        self.ui_sound(sounds::UiSound::Move);
                    }
                    self.human_orders
                        .extend(self.group_commands.iter().map(|&command| PlayerCommand {
                            player: self.sandbox.side,
                            command,
                        }));
                }
                self.input.right_down = None;
            }
            _ => {}
        }
    }

    fn ui_sound(&self, sound: sounds::UiSound) {
        if let Some(s) = &self.sounds {
            s.ui(sound);
        }
    }

    /// The map point under the mouse, found by marching the camera ray over
    /// the CPU copy of the heightmap.
    fn ground_under_cursor(&self) -> Option<Vec2> {
        let cursor = self.input.cursor?;
        let size = self.window.inner_size();
        let ndc = Vec2::new(
            (cursor.x as f32 / size.width as f32) * 2.0 - 1.0,
            1.0 - (cursor.y as f32 / size.height as f32) * 2.0,
        );
        let (origin, dir) = self.camera.ray(ndc);
        let mut t = 0.0;
        let step = 0.5;
        let mut prev_t = 0.0;
        for _ in 0..4000 {
            let p = origin + dir * t;
            let ground = self
                .heightmap
                .height_at(p.x, p.z)
                .max(self.heightmap.water_level);
            if p.y <= ground {
                // Refine between the last point above ground and this one.
                let (mut lo, mut hi) = (prev_t, t);
                for _ in 0..16 {
                    let mid = (lo + hi) * 0.5;
                    let q = origin + dir * mid;
                    let g = self
                        .heightmap
                        .height_at(q.x, q.z)
                        .max(self.heightmap.water_level);
                    if q.y <= g { hi = mid } else { lo = mid }
                }
                let hit = origin + dir * hi;
                let n = MAP_TILES as f32;
                let inside = (0.0..=n).contains(&hit.x) && (0.0..=n).contains(&hit.z);
                return inside.then(|| Vec2::new(hit.x, hit.z));
            }
            prev_t = t;
            t += step;
        }
        None
    }

    /// Expand a group order for the current selection into
    /// `self.group_commands` (one move per regiment).
    fn plan_order(&mut self, order: GroupOrder) {
        let regiments = &self.world.regiments;
        self.selected.retain(|&h| regiments.get(h).is_some());
        self.group.clear();
        for &h in &self.selected {
            let r = self.world.regiments.get(h).unwrap();
            let stats = &self.world.units()[r.unit as usize];
            self.group.push(GroupMember {
                handle: h,
                position: r.position,
                facing: r.facing,
                files: r.files,
                models: r.models,
                file_spacing: stats.file_spacing,
                rank_spacing: stats.rank_spacing,
                speed: stats.speed,
            });
        }
        self.group_commands.clear();
        formation::group_move(&mut self.group, order, &mut self.group_commands);
    }

    /// Ghost plates where each regiment of a planned order will stand.
    fn push_order_preview(&mut self) {
        let [cr, cg, cb] = color_of(&self.parties, self.sandbox.side);
        for c in &self.group_commands {
            let gc_sim::Command::MoveTo {
                regiment,
                target,
                facing,
                files,
                ..
            } = *c
            else {
                continue;
            };
            let Some(r) = self.world.regiments.get(regiment) else {
                continue;
            };
            let stats = &self.unit_stats[r.unit as usize];
            let files = if files == 0 { r.files } else { files };
            let (width, depth) = stats.block_size(files, r.models);
            self.instances.push(BoxInstance {
                pos: from_fx_vec(target).to_array(),
                facing: angle_vec(facing.unwrap_or(r.facing)).to_array(),
                size: [fx_to_f32(width), PLATE_HEIGHT, fx_to_f32(depth)],
                color: [cr / 2, cg / 2, cb / 2, 160],
                lift: 0.0,
            });
        }
    }

    /// The line order a right-drag in progress describes, once it is long
    /// enough to be a drag rather than a click.
    fn drag_order(&self) -> Option<GroupOrder> {
        let (_, start) = self.input.right_down?;
        let end = self.ground_under_cursor()?;
        let line = end - start;
        if line.length() < MIN_LINE_TILES {
            return None;
        }
        // Face away from the camera's side: dragging left to right faces up the screen.
        let facing = Vec2::new(line.y, -line.x);
        Some(GroupOrder {
            target: fx_vec((start + end) * 0.5),
            facing: Some(gc_fixed::Angle::of_vec(fx_vec(facing))),
            width: Some(to_fx(line.length())),
        })
    }

    /// The screen rectangle of a left-drag in progress (min, max), once the
    /// mouse has moved far enough for it to be a drag.
    fn selection_box(&self) -> Option<(Vec2, Vec2)> {
        let a = self.input.left_down?;
        let b = self.input.cursor?;
        let (a, b) = (
            Vec2::new(a.x as f32, a.y as f32),
            Vec2::new(b.x as f32, b.y as f32),
        );
        let d = (a - b).abs();
        (d.x.max(d.y) >= MIN_BOX_PIXELS).then(|| (a.min(b), a.max(b)))
    }

    /// Fill `self.boxed` with your regiments whose centres are inside a
    /// screen rectangle.
    fn regiments_in_box(&mut self, (lo, hi): (Vec2, Vec2)) {
        self.boxed.clear();
        let size = self.window.inner_size();
        let view_proj = self.camera.view_proj();
        for r in self
            .world
            .regiments
            .iter()
            .filter(|r| r.owner == self.sandbox.side)
        {
            let p = from_fx_vec(r.position);
            let ground = self.heightmap.height_at(p.x, p.y);
            let clip = view_proj * glam::Vec4::new(p.x, ground + 0.5, p.y, 1.0);
            if clip.w <= 0.0 {
                continue;
            }
            let ndc = clip.truncate() / clip.w;
            let screen = Vec2::new(
                (ndc.x * 0.5 + 0.5) * size.width as f32,
                (0.5 - ndc.y * 0.5) * size.height as f32,
            );
            if screen.cmpge(lo).all() && screen.cmple(hi).all() {
                self.boxed.push(r.handle);
            }
        }
    }

    /// Number keys: Ctrl/Cmd + n saves the selection as group n, n recalls it
    /// (Shift adds it), and n twice quickly centres the camera on it.
    fn control_group(&mut self, n: usize) {
        if self.input.command {
            self.control_groups[n].clear();
            self.control_groups[n].extend_from_slice(&self.selected);
            return;
        }
        let (regiments, side) = (&self.world.regiments, self.sandbox.side);
        self.control_groups[n].retain(|&h| regiments.get(h).is_some_and(|r| r.owner == side));
        if !self.input.shift {
            self.selected.clear();
        }
        for &h in &self.control_groups[n] {
            if !self.selected.contains(&h) {
                self.selected.push(h);
            }
        }
        if !self.selected.is_empty() {
            self.ui_sound(sounds::UiSound::Select);
        }
        let now = Instant::now();
        let double = matches!(self.last_group_key, Some((k, t)) if k == n && now - t < DOUBLE_TAP);
        self.last_group_key = Some((n, now));
        if double && !self.control_groups[n].is_empty() {
            let sum: Vec2 = self.control_groups[n]
                .iter()
                .filter_map(|&h| self.world.regiments.position(h))
                .map(from_fx_vec)
                .sum();
            let centre = sum / self.control_groups[n].len() as f32;
            self.camera.focus = centre;
            self.camera.focus_height = self.heightmap.height_at(centre.x, centre.y);
        }
    }

    /// The enemy regiment whose block is under (or within a step of) `p`.
    fn enemy_at(&self, p: Vec2) -> Option<RegimentHandle> {
        self.world
            .regiments
            .iter()
            .filter(|r| r.owner != self.sandbox.side)
            .filter_map(|r| {
                let stats = &self.unit_stats[r.unit as usize];
                let (width, depth) = stats.block_size(r.files, r.models);
                let to = p - from_fx_vec(r.position);
                let facing = angle_vec(r.facing);
                let ahead = to.dot(facing).abs() - fx_to_f32(depth) / 2.0;
                let across = to.perp_dot(facing).abs() - fx_to_f32(width) / 2.0;
                let outside = ahead.max(across);
                (outside <= ATTACK_PICK_TILES).then_some((r.handle, outside))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(h, _)| h)
    }

    fn nearest_own(&self, p: Vec2, radius: f32) -> Option<RegimentHandle> {
        self.world
            .regiments
            .iter()
            .filter(|r| r.owner == self.sandbox.side)
            .map(|r| (r.handle, from_fx_vec(r.position).distance(p)))
            .filter(|(_, d)| *d <= radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(h, _)| h)
    }

    fn frame(&mut self) {
        profile::zone!("frame");
        let now = Instant::now();
        let gap = now - self.last_frame;
        if self.stats.frames > 0 {
            self.stats.frame_max = self.stats.frame_max.max(gap);
        }
        let dt = gap.min(Duration::from_millis(250));
        self.last_frame = now;
        self.update_camera(dt.as_secs_f32());

        // Fixed-rate simulation; render interpolates between the last two
        // ticks. Pause and slow motion scale game time, not the camera.
        let game_dt = if self.paused {
            Duration::ZERO
        } else {
            dt.mul_f32(SPEEDS[self.speed])
        };
        // The sim stands still behind the menu, except in a network match,
        // which runs on the host's clock.
        if !self.menu_open && self.online.is_none() {
            self.sim_clock += game_dt;
        }
        let mut ticks_this_frame = if self.online.is_some() {
            self.online_turns(now)
        } else {
            0
        };
        while self.sim_clock >= TICK && ticks_this_frame < MAX_TICKS_PER_FRAME {
            self.sim_clock -= TICK;
            ticks_this_frame += 1;
            self.pending.clear();
            if let Some(drill) = &mut self.drill {
                drill.commands_for(&self.world, &mut self.pending);
                // A side the computer plays takes its orders from its mind.
                let minds = &self.minds;
                self.pending.retain(|c| {
                    matches!(c.command, gc_sim::Command::SpawnRegiment { .. })
                        || minds.get(c.player).is_none()
                });
            }
            let t_ai = Instant::now();
            tagged_alloc::scope(Tag::Ai, || self.minds.tick(&self.world, &mut self.pending));
            self.stats.ai_time += t_ai.elapsed();
            self.pending.append(&mut self.human_orders);
            self.step();
        }
        if ticks_this_frame > 0 {
            // Tuned stats take effect in the tick after they are sent.
            self.unit_stats.clone_from_slice(self.world.units());
            self.judge_battle();
        }
        let alpha = match &self.online {
            // Turns arrive on the host's clock: move smoothly towards the
            // latest one.
            Some(o) => (o.last_turn.elapsed().as_secs_f32() / TICK.as_secs_f32()).min(1.0),
            None => self.sim_clock.as_secs_f32() / TICK.as_secs_f32(),
        };

        self.instances.clear();
        self.boxed.clear();
        if let Some(rect) = self.selection_box() {
            self.regiments_in_box(rect);
        }
        if let Some(order) = self.drag_order() {
            self.plan_order(order);
            self.push_order_preview();
        }
        self.push_placement_ghost();
        self.puppets.advance(game_dt.as_secs_f32());
        let world = &self.world;
        self.puppets.pair(|a, b| world.hostile(a, b));
        let marked = self
            .attack_mark
            .filter(|(_, at)| at.elapsed() < ATTACK_MARK)
            .map(|(h, _)| h);
        if let Some(art) = &mut self.art {
            art.clear();
        }
        self.mark_shooting();
        for r in self.world.regiments.iter() {
            let cur = from_fx_vec(r.position);
            let cur_facing = angle_vec(r.facing);
            let (pos, facing) = match self.prev_positions[r.handle.index as usize] {
                Some((h, prev, prev_facing)) if h == r.handle => (
                    prev.lerp(cur, alpha),
                    prev_facing.lerp(cur_facing, alpha).normalize_or(cur_facing),
                ),
                _ => (cur, cur_facing),
            };
            let [cr, cg, cb] = color_of(&self.parties, r.owner);
            let selected = self.selected.contains(&r.handle) || self.boxed.contains(&r.handle);
            let stats = &self.unit_stats[r.unit as usize];
            let (file_spacing, rank_spacing) =
                (fx_to_f32(stats.file_spacing), fx_to_f32(stats.rank_spacing));
            if selected || marked == Some(r.handle) {
                let (width, depth) = stats.block_size(r.files, r.models);
                self.instances.push(BoxInstance {
                    pos: pos.to_array(),
                    facing: facing.to_array(),
                    size: [fx_to_f32(width) + 0.3, PLATE_HEIGHT, fx_to_f32(depth) + 0.3],
                    color: if selected {
                        // Darker than the team colour so the soldiers on it stand out.
                        [cr / 2 + 20, cg / 2 + 20, cb / 2 + 20, 255]
                    } else {
                        [255, 40, 20, 255]
                    },
                    lift: 0.0,
                });
            }
            let block = Block {
                handle: r.handle,
                unit: r.unit,
                centre: pos,
                facing,
                files: r.files,
                models: r.models,
                file_spacing,
                rank_spacing,
                speed: fx_to_f32(stats.speed),
                engaged: r.engaged,
                routing: r.morale.broken(),
            };
            let blink = self.started.elapsed().as_millis() % 600 < 300;
            let banner = match r.morale {
                MoraleState::Steady => None,
                MoraleState::Wavering => blink.then_some([235, 170, 40, 200]),
                MoraleState::Routing => Some([245, 245, 240, 200]),
                MoraleState::Shattered => Some([110, 110, 110, 120]),
            };
            if self.commander {
                // One block per regiment with a tall banner: the flag shows
                // the team, or the morale colour when shaken.
                let (width, depth) = stats.block_size(r.files, r.models);
                let (width, depth) = (fx_to_f32(width), fx_to_f32(depth));
                let shade = if selected { 255 } else { 210 };
                let tint = |c: u8| (c as u32 * shade / 255) as u8;
                self.instances.push(BoxInstance {
                    pos: pos.to_array(),
                    facing: facing.to_array(),
                    size: [width, COMMANDER_BLOCK_HEIGHT, depth],
                    color: [tint(cr), tint(cg), tint(cb), if selected { 90 } else { 0 }],
                    lift: 0.0,
                });
                // A pale strip along the front edge shows which way it faces.
                self.instances.push(BoxInstance {
                    pos: (pos + facing * (depth / 2.0 - 0.3)).to_array(),
                    facing: facing.to_array(),
                    size: [width, COMMANDER_BLOCK_HEIGHT + 0.1, 0.6],
                    color: [235, 230, 210, 0],
                    lift: 0.0,
                });
                // Banners grow as the camera pulls back, so they stay readable.
                let k = (self.camera.distance / COMMANDER_IN).max(1.0);
                let side = Vec2::new(facing.y, -facing.x);
                let pole = COMMANDER_POLE.map(|v| v * k);
                let flag_size = COMMANDER_FLAG.map(|v| v * k);
                self.instances.push(BoxInstance {
                    pos: pos.to_array(),
                    facing: facing.to_array(),
                    size: pole,
                    color: [60, 48, 36, 0],
                    lift: 0.0,
                });
                let flag = banner.unwrap_or([cr, cg, cb, 0]);
                self.instances.push(BoxInstance {
                    pos: (pos + side * (flag_size[0] / 2.0)).to_array(),
                    facing: facing.to_array(),
                    size: flag_size,
                    color: [flag[0], flag[1], flag[2], 0],
                    lift: pole[1] - flag_size[1],
                });
                // Keep the puppets marching (and falling) out of sight.
                self.puppets
                    .update(&block, r.owner, game_dt.as_secs_f32(), |_| {});
                continue;
            }
            if let Some(color) = banner {
                self.instances.push(BoxInstance {
                    pos: pos.to_array(),
                    facing: facing.to_array(),
                    size: BANNER_SIZE,
                    color,
                    lift: BANNER_LIFT,
                });
            }
            let model = self
                .art
                .as_ref()
                .and_then(|a| a.unit_model.get(r.unit as usize).copied().flatten());
            if let (Some(model), Some(art)) = (model, self.art.as_mut()) {
                let far = self.camera.eye().distance(glam::Vec3::new(
                    pos.x,
                    self.camera.focus_height,
                    pos.y,
                )) > LOD_DISTANCE;
                let bucket = &mut art.buckets[model * 2 + far as usize];
                let clips = &art.clips;
                let shooting = self.shooting[r.handle.index as usize];
                let time = self.puppets.time();
                let highlight = if selected { 70 } else { 0 };
                self.puppets
                    .update(&block, r.owner, game_dt.as_secs_f32(), |s| {
                        let action = soldier_action(&s, &block, shooting);
                        let Some(pose) = clips.pose(model, action, time + s.phase * 4.0) else {
                            return;
                        };
                        bucket.push(SoldierInstance {
                            pos: s.pos.to_array(),
                            facing: s.facing.to_array(),
                            frames: pose.rows,
                            flags: [(pose.blend * 255.0) as u8, highlight, 0, 0],
                            color: [cr, cg, cb, 255],
                            lift: 0.0,
                            scale: 1.0,
                        });
                    });
                continue;
            }
            let size = [
                (file_spacing * 0.6).min(0.45),
                SOLDIER_HEIGHT,
                (rank_spacing * 0.42).min(0.35),
            ];
            let color = [cr, cg, cb, if selected { 70 } else { 0 }];
            let instances = &mut self.instances;
            self.puppets
                .update(&block, r.owner, game_dt.as_secs_f32(), |s| {
                    instances.push(BoxInstance {
                        pos: s.pos.to_array(),
                        facing: s.facing.to_array(),
                        size,
                        color,
                        lift: 0.0,
                    });
                });
        }
        if !self.commander {
            self.push_corpses();
        }
        if let Some(art) = &mut self.art {
            art.finish();
        }
        self.push_arrows(alpha);
        self.push_routes();
        if self.lens && !self.minds.is_empty() {
            let lenses = lens::lenses(&self.minds);
            let parties = &self.parties;
            lens::push(
                &lenses,
                &self.minds,
                lens::scale_for(self.camera.distance),
                |p| color_of(parties, p),
                &mut self.instances,
            );
        }
        if self.show_blocked {
            self.instances.extend_from_slice(&self.blocked_marks);
        }

        if self.menu_open {
            self.run_menu();
        } else {
            self.renderer.clear_ui();
        }

        let frame = FrameParams {
            view_proj: self.camera.view_proj(),
            camera_pos: self.camera.eye(),
            time_seconds: self.started.elapsed().as_secs_f32(),
        };
        {
            profile::zone!("render");
            tagged_alloc::scope(Tag::Render, || {
                let (soldiers, batches) = match &self.art {
                    Some(a) => (a.soldiers.as_slice(), a.batches.as_slice()),
                    None => (&[][..], &[][..]),
                };
                self.renderer
                    .render(&frame, &self.instances, soldiers, batches)
            });
        }
        profile::frame_mark();
        if self.sandbox.dirty {
            self.refresh_overlay();
        }

        self.stats.frames += 1;
        self.stats.window_frames += 1;
        if self.stats.window_start.elapsed() >= Duration::from_millis(500) {
            self.window.set_title(&self.stats_line());
            if self
                .sandbox
                .message
                .as_ref()
                .is_some_and(|(_, at)| at.elapsed() > MESSAGE_TIME)
            {
                self.sandbox.message = None;
                self.sandbox.dirty = true;
            }
            // What the minds are thinking changes as the battle goes.
            if !self.minds.is_empty() || self.battle.is_some() {
                self.sandbox.dirty = true;
            }
            if self.overlay || self.sandbox.dirty {
                self.refresh_overlay();
            }
            self.stats.window_frames = 0;
            self.stats.window_start = Instant::now();
            self.stats.frame_max = Duration::ZERO;
            self.stats.tick_time = Duration::ZERO;
            self.stats.tick_max = Duration::ZERO;
            self.stats.ticks = 0;
            self.stats.ai_time = Duration::ZERO;
        }
    }

    /// Remember where every regiment stands, then simulate one tick with
    /// the commands in `pending`.
    fn step(&mut self) {
        for r in self.world.regiments.iter() {
            self.prev_positions[r.handle.index as usize] =
                Some((r.handle, from_fx_vec(r.position), angle_vec(r.facing)));
        }
        let t0 = Instant::now();
        profile::zone!("sim tick");
        tagged_alloc::scope(Tag::Sim, || self.world.tick(&self.pending));
        if let Some(sounds) = &mut self.sounds {
            tagged_alloc::scope(Tag::Audio, || sounds.observe(&self.world, &self.camera));
        }
        let took = t0.elapsed();
        self.stats.tick_time += took;
        self.stats.tick_max = self.stats.tick_max.max(took);
        self.stats.ticks += 1;
    }

    /// A network match: send this machine's orders, then simulate every
    /// turn the host has sent that is due. Returns the turns simulated.
    fn online_turns(&mut self, now: Instant) -> u32 {
        let lan = self.menu.net();
        lan.pump(now);
        let events = lan.match_events();
        let in_match = lan.in_match();
        let Some(o) = &mut self.online else {
            return 0;
        };
        for e in events {
            match e {
                gc_net::MatchEvent::Notice(text) => o.note(text),
                gc_net::MatchEvent::Desync { tick } => {
                    if o.desync.is_none() {
                        o.desync = Some(tick);
                        o.report = lan.desync_report().map(online::save_report);
                    }
                }
                gc_net::MatchEvent::Ended(why) => {
                    eprintln!("grubcraft: {why}");
                    o.ended = Some(why);
                }
            }
            self.sandbox.dirty = true;
        }
        if o.ended.is_none() && !in_match {
            o.ended = Some("you left it.".into());
            self.sandbox.dirty = true;
        }
        if o.ended.is_some() {
            self.human_orders.clear();
            return 0;
        }
        let host = o.host;
        let lan = self.menu.net();
        lan.submit(self.human_orders.drain(..));
        let mut turns = 0;
        while turns < MAX_TICKS_PER_FRAME {
            let Some(commands) = self.menu.net().next_turn(now) else {
                break;
            };
            self.pending = commands;
            self.step();
            let tick = self.world.tick_count();
            if tick.is_multiple_of(gc_net::HASH_EVERY) {
                let hash = self.world.state_hash();
                self.menu.net().record_hash(tick, hash);
            }
            // The computer players think on the host only; their orders
            // reach everyone through the turns like a human's.
            if host {
                self.pending.clear();
                let t_ai = Instant::now();
                tagged_alloc::scope(Tag::Ai, || self.minds.tick(&self.world, &mut self.pending));
                self.stats.ai_time += t_ai.elapsed();
                self.menu.net().submit(self.pending.drain(..));
            }
            turns += 1;
        }
        if turns > 0
            && let Some(o) = &mut self.online
        {
            o.last_turn = now;
        }
        turns
    }

    /// Note which regiments loosed a volley lately, so their soldiers play
    /// the shooting animation.
    fn mark_shooting(&mut self) {
        self.shooting.fill(false);
        let now = self.world.tick_count();
        for v in self.world.volleys() {
            if v.launch_tick + SHOOTING_TICKS < now {
                continue;
            }
            let from = from_fx_vec(v.from);
            let shooter = self
                .world
                .regiments
                .iter()
                .filter(|r| r.owner == v.owner)
                .min_by(|a, b| {
                    let da = from_fx_vec(a.position).distance_squared(from);
                    let db = from_fx_vec(b.position).distance_squared(from);
                    da.total_cmp(&db)
                });
            if let Some(r) = shooter
                && from_fx_vec(r.position).distance(from) < 2.0
            {
                self.shooting[r.handle.index as usize] = true;
            }
        }
    }

    /// UI pixels to physical pixels: the display's scale times the player's
    /// UI scale setting.
    fn ui_scale(&self) -> f32 {
        self.window.scale_factor() as f32 * self.menu.settings().effective_ui_scale()
    }

    fn open_menu(&mut self) {
        if self.menu_open {
            return;
        }
        self.menu_open = true;
        self.sandbox.dirty = true;
        self.menu.show(Screen::Main);
        self.menu.set_in_match(self.match_started);
        self.ui.reset();
        self.ui_input = UiInput::default();
        // Drop any half-made drag so it doesn't fire when the menu closes.
        self.input.left_down = None;
        self.input.right_down = None;
        self.input.middle_down = None;
    }

    fn menu_key(&mut self, key: &winit::event::KeyEvent) {
        if key.state != ElementState::Pressed {
            return;
        }
        if !self.input.command
            && let Some(text) = &key.text
        {
            self.ui_input
                .typed
                .extend(text.chars().filter(|c| (' '..='~').contains(c)));
        }
        match key.physical_key {
            PhysicalKey::Code(KeyCode::Backspace) => self.ui_input.backspace = true,
            PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => self.ui_input.enter = true,
            PhysicalKey::Code(KeyCode::Escape) if !key.repeat => self.ui_input.escape = true,
            _ => {}
        }
    }

    /// Draw this frame's menu and carry out what it asks for.
    fn run_menu(&mut self) {
        let scale = self.ui_scale();
        let size = self.window.inner_size();
        let screen = (
            (size.width as f32 / scale) as i32,
            (size.height as f32 / scale) as i32,
        );
        let mut input = std::mem::take(&mut self.ui_input);
        input.mouse = self.input.cursor.map_or((-1, -1), |c| {
            ((c.x as f32 / scale) as i32, (c.y as f32 / scale) as i32)
        });
        self.ui.begin_frame(input, screen);
        let action = self.menu.frame(&mut self.ui);
        self.ui.end_frame();
        self.ui_quads.clear();
        self.ui_quads
            .extend(self.ui.draw.quads.iter().map(|q| UiQuad {
                pos: q.pos,
                size: q.size,
                glyph: q.glyph,
                color: q.color,
            }));
        self.renderer.set_ui(&self.ui_quads, scale);
        match action {
            MenuAction::None | MenuAction::SettingsChanged => {}
            MenuAction::Resume => self.close_menu(),
            MenuAction::Quit => self.quit = true,
            MenuAction::StartSandbox => self.start_match(MatchKind::Sandbox),
            MenuAction::StartMatch(setup) => {
                eprintln!(
                    "grubcraft: starting {} on {} with {} players",
                    setup.settings.mode.name(),
                    gc_ui::lobby::MAPS[setup.settings.map],
                    setup.players.len()
                );
                self.start_match(MatchKind::Battle(setup));
            }
        }
    }

    fn close_menu(&mut self) {
        self.menu_open = false;
        self.sandbox.dirty = true;
        self.renderer.clear_ui();
        self.input.left_down = None;
        self.input.right_down = None;
    }

    /// Begin a match from a fresh world: the empty-field sandbox or a Field
    /// Battle from the lobby.
    fn start_match(&mut self, kind: MatchKind) {
        self.camera.face(Vec2::NEG_Y);
        self.restart_match(kind);
        self.match_started = true;
        self.menu.set_in_match(true);
        self.close_menu();
    }

    fn restart_match(&mut self, kind: MatchKind) {
        // A network match plays on the host's map, with the game data as
        // built (whatever the sandbox tuned stays in the sandbox).
        let net_seed = match &kind {
            MatchKind::Battle(setup) => setup.seed,
            _ => None,
        };
        if net_seed.is_none() && self.menu.net().in_match() {
            // Starting something else leaves the network match.
            self.menu.net().leave();
        }
        if let Some(seed) = net_seed {
            if seed != self.seed {
                self.use_map(seed);
            }
            self.unit_stats.clone_from(&self.data_stats);
        }
        self.online = net_seed.map(|_| online::Online::new(self.menu.net().is_host()));
        let nav = tagged_alloc::scope(Tag::Map, || self.heightmap.nav_costs());
        let (heights, water_level) = tagged_alloc::scope(Tag::Map, || self.heightmap.sim_ground());
        let teams: Vec<u8> = match &kind {
            MatchKind::Battle(setup) => setup.players.iter().map(|s| s.team).collect(),
            _ => Vec::new(),
        };
        self.world = tagged_alloc::scope(Tag::Sim, || {
            World::new(&MatchSetup {
                seed: self.seed,
                capacities: self.capacities,
                units: &self.unit_stats,
                map_tiles: MAP_TILES,
                nav: &nav,
                teams: &teams,
                ground: gc_sim::Ground {
                    heights: &heights,
                    water_level,
                },
            })
        });
        self.minds = tagged_alloc::scope(Tag::Ai, || Minds::new(self.heightmap.ai_terrain(&nav)));
        self.matches += 1;
        self.drill = None;
        self.sandbox = Sandbox::new(matches!(kind, MatchKind::Sandbox));
        self.parties = sandbox_parties();
        self.battle = None;
        self.speed = NORMAL_SPEED;
        self.paused = false;
        self.prev_positions.iter_mut().for_each(|p| *p = None);
        self.pending.clear();
        self.human_orders.clear();
        self.selected.clear();
        self.boxed.clear();
        self.control_groups = Default::default();
        self.last_group_key = None;
        self.attack_mark = None;
        self.sim_clock = Duration::ZERO;
        self.puppets = Puppets::new(self.capacities.regiments as usize, MAX_CORPSES);
        if let MatchKind::Battle(setup) = kind {
            self.set_out_battle(&setup);
        }
        if self.online.is_some() {
            // A joiner checks its map came out the same as the host's
            // before saying it is ready.
            let mine = online::map_hash(
                &nav,
                &gc_sim::Ground {
                    heights: &heights,
                    water_level,
                },
            );
            let lan = self.menu.net();
            let result = match lan.host_map() {
                Some(host) if host.map_hash != mine => Err(format!(
                    "the map came out differently on this machine (seed {}, map {:016x}, host's {:016x})",
                    host.seed, mine, host.map_hash
                )),
                _ => Ok(()),
            };
            lan.loaded(result.clone());
            if let (Err(why), Some(o)) = (result, &mut self.online) {
                o.ended = Some(why);
                self.menu.net().leave();
            }
        }
    }

    /// Generate the map for `seed` and put it on screen.
    fn use_map(&mut self, seed: u64) {
        eprintln!("grubcraft: switching to the host's map (seed {seed})");
        self.seed = seed;
        self.heightmap = tagged_alloc::scope(Tag::Map, || Heightmap::generate(seed, MAP_TILES));
        let hm = &self.heightmap;
        tagged_alloc::scope(Tag::Render, || {
            self.renderer.set_terrain(&Terrain {
                size: hm.size,
                samples_per_tile: hm.samples_per_tile,
                max_height: hm.max_height,
                water_level: hm.water_level,
                heights: &hm.heights,
            })
        });
        let nav = self.heightmap.nav_costs();
        self.blocked_marks = blocked_marks(&nav, MAP_TILES);
    }

    /// Raise and deploy every side's army for a lobby Field Battle, and put
    /// a mind in charge of each AI player. Armies are a fixed mix from the
    /// faction's roster until production arrives (docs/06-game-modes.md).
    fn set_out_battle(&mut self, setup: &MatchStart) {
        let set = tagged_alloc::scope(Tag::Ai, || {
            battle::set_out(
                setup,
                &self.unit_stats,
                &self.unit_factions,
                terrain_gen::start_positions(MAP_TILES),
                self.seed ^ self.matches.wrapping_mul(0x9E37_79B9_7F4A_7C15),
                self.online.as_ref().is_none_or(|o| o.host),
                &mut self.minds,
                &mut self.human_orders,
            )
        });
        for m in self.minds.iter() {
            eprintln!(
                "grubcraft: {} plays {}",
                m.summary(),
                party_name(&set.parties, m.party())
            );
        }
        self.parties = set.parties;
        self.sandbox.side = set.me;
        self.battle = Some(Battle::new(set.me));
        // Look over your army towards the enemy.
        let focus = set.my_start + set.my_forward * 18.0;
        self.camera.focus = focus;
        self.camera.focus_height = self.heightmap.height_at(focus.x, focus.y);
        self.camera.distance = 120.0;
        // A joiner who starts in the north looks south, towards the enemy.
        if self.online.is_some() && set.my_forward.y > 0.0 {
            self.camera.face(Vec2::Y);
        }
    }

    /// How the battle ended, once it has.
    fn battle_outcome(&self) -> Option<&str> {
        self.battle.as_ref()?.outcome.as_deref()
    }

    /// Once one side alone has held the field long enough, say who won.
    fn judge_battle(&mut self) {
        let Some(battle) = &mut self.battle else {
            return;
        };
        if battle.judge(&self.world, &self.parties) {
            if let Some(outcome) = &battle.outcome {
                eprintln!("grubcraft: {outcome}");
            }
            self.sandbox.dirty = true;
        }
    }

    /// Dotted lines along the routes of the selected regiments, with a plate
    /// where each will stop.
    fn push_routes(&mut self) {
        let mut dots = 0;
        for &h in &self.selected {
            let Some(r) = self.world.regiments.get(h) else {
                continue;
            };
            let [cr, cg, cb] = color_of(&self.parties, r.owner);
            let color = [cr, cg, cb, 160];
            let mut from = from_fx_vec(r.position);
            let route = self.world.route(h);
            for (k, &p) in route.iter().enumerate() {
                let to = from_fx_vec(p);
                let leg = to - from;
                let steps = (leg.length() / ROUTE_DOT_SPACING) as usize;
                for s in 1..=steps {
                    if dots == MAX_ROUTE_DOTS {
                        return;
                    }
                    dots += 1;
                    self.instances.push(BoxInstance {
                        pos: (from + leg * (s as f32 / (steps + 1) as f32)).to_array(),
                        facing: [0.0, 1.0],
                        size: ROUTE_DOT,
                        color,
                        lift: 0.0,
                    });
                }
                if k + 1 == route.len() {
                    self.instances.push(BoxInstance {
                        pos: to.to_array(),
                        facing: [0.0, 1.0],
                        size: ROUTE_END,
                        color,
                        lift: 0.0,
                    });
                }
                from = to;
            }
        }
    }

    /// Fallen soldiers lie flat in a darker shade of their team's colour.
    fn push_corpses(&mut self) {
        for c in self.puppets.corpses() {
            let [r, g, b] = color_of(&self.parties, c.owner);
            let model = self
                .art
                .as_ref()
                .and_then(|a| a.unit_model.get(c.unit as usize).copied().flatten());
            if let (Some(model), Some(art)) = (model, self.art.as_mut()) {
                // The death clip, held on its last frame; models without
                // one lie down in their idle pose.
                let (pose, fallen) = match art.clips.death(model, c.age) {
                    Some(p) => (Some(p), 0),
                    None => (art.clips.pose(model, Action::Idle, 0.0), 255),
                };
                if let Some(pose) = pose {
                    let far = self.camera.eye().distance(glam::Vec3::new(
                        c.pos.x,
                        self.camera.focus_height,
                        c.pos.y,
                    )) > LOD_DISTANCE;
                    art.buckets[model * 2 + far as usize].push(SoldierInstance {
                        pos: c.pos.to_array(),
                        facing: c.facing.to_array(),
                        frames: pose.rows,
                        flags: [(pose.blend * 255.0) as u8, 0, fallen, 0],
                        color: [r, g, b, 255],
                        lift: -0.5 * c.sink,
                        scale: 1.0,
                    });
                    continue;
                }
            }
            self.instances.push(BoxInstance {
                pos: c.pos.to_array(),
                facing: c.facing.to_array(),
                size: [0.4, 0.14, SOLDIER_HEIGHT * 0.8],
                color: [r / 2, g / 2, b / 2, 0],
                lift: -0.16 * c.sink,
            });
        }
    }

    /// Volleys in flight: a few arrows each, arcing from the archers to where
    /// they aimed. Only a picture; the simulation decides what they hit.
    fn push_arrows(&mut self, alpha: f32) {
        let now = self.world.tick_count() as f32 - 1.0 + alpha;
        for v in self.world.volleys() {
            let flight = (v.impact_tick - v.launch_tick).max(1) as f32;
            let t = ((now - v.launch_tick as f32) / flight).clamp(0.0, 1.0);
            let (from, aim) = (from_fx_vec(v.from), from_fx_vec(v.aim));
            let travel = aim - from;
            let length = travel.length();
            let dir = travel.normalize_or(Vec2::X);
            let arc = (t * std::f32::consts::PI).sin() * length * ARROW_ARC;
            let seed = v.launch_tick.wrapping_mul(0x9E37_79B9) ^ v.from.x.raw() as u32;
            for k in 0..ARROWS_PER_VOLLEY.min(v.shots as usize) {
                let spread = Vec2::new(spread_hash(seed, k, 0), spread_hash(seed, k, 1));
                let pos = from.lerp(aim, t) + spread * ARROW_SPREAD;
                self.instances.push(BoxInstance {
                    pos: pos.to_array(),
                    facing: dir.to_array(),
                    size: [0.05, 0.05, 0.7],
                    color: [70, 52, 36, 0],
                    lift: ARROW_HEIGHT + arc,
                });
            }
        }
    }

    fn update_camera(&mut self, dt: f32) {
        if self.menu_open && !self.match_started {
            // A slow sway behind the main menu.
            self.camera.yaw = 0.3 * (self.started.elapsed().as_secs_f32() * 0.12).sin();
        }
        let mut dx = 0.0;
        let mut dy = 0.0;
        let [up, down, left, right] = if self.menu_open {
            [false; 4]
        } else {
            self.input.arrows
        };
        if up {
            dy += 1.0
        }
        if down {
            dy -= 1.0
        }
        if left {
            dx -= 1.0
        }
        if right {
            dx += 1.0
        }
        if let (Some(c), true, false) = (self.input.cursor, self.input.focused, self.menu_open) {
            let size = self.window.inner_size();
            if c.x <= EDGE_SCROLL_PX {
                dx -= 1.0
            }
            if c.x >= size.width as f64 - EDGE_SCROLL_PX {
                dx += 1.0
            }
            if c.y <= EDGE_SCROLL_PX {
                dy += 1.0
            }
            if c.y >= size.height as f64 - EDGE_SCROLL_PX {
                dy -= 1.0
            }
        }
        if dx != 0.0 || dy != 0.0 {
            self.camera.pan(dx * dt * 1.2, dy * dt * 1.2);
        }
        let commander = if self.commander {
            self.camera.distance > COMMANDER_OUT
        } else {
            self.camera.distance > COMMANDER_IN
        };
        if commander != self.commander {
            self.commander = commander;
            self.sandbox.dirty = true;
        }
        // Follow the ground smoothly so the view doesn't jolt over ridges.
        let ground = self
            .heightmap
            .height_at(self.camera.focus.x, self.camera.focus.y);
        let k = 1.0 - (-dt * 6.0).exp();
        self.camera.focus_height += (ground - self.camera.focus_height) * k;
    }

    /// Rebuild the overlay: the sandbox panel, and the dev overlay's text
    /// from the current stats window when it is on.
    fn refresh_overlay(&mut self) {
        if !self.overlay {
            self.show_overlay(None);
            return;
        }
        let secs = self.stats.window_start.elapsed().as_secs_f32().max(1e-3);
        let frames = self.stats.window_frames.max(1);
        let fps = self.stats.window_frames as f32 / secs;
        let frame_avg = secs * 1000.0 / frames as f32;
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let tick_avg = if self.stats.ticks > 0 {
            ms(self.stats.tick_time) / self.stats.ticks as f64
        } else {
            0.0
        };
        let ai_avg = if self.stats.ticks > 0 {
            ms(self.stats.ai_time) / self.stats.ticks as f64
        } else {
            0.0
        };
        let rs = self.renderer.stats();
        let process = gc_platform::memory::process_memory()
            .current
            .map_or("?".to_string(), |b| format!("{:.1} MB", mb(b as usize)));
        let heap_tags: Vec<String> = Tag::ALL
            .iter()
            .filter(|&&t| tagged_alloc::live_bytes(t) > 0)
            .map(|&t| format!("{} {:.2}", t.name(), mb(tagged_alloc::live_bytes(t))))
            .collect();
        let info = self.renderer.adapter_info();
        let text = format!(
            "GRUBCRAFT DEV OVERLAY                          ` or F11 hides\n\
             frame   {fps:5.0} fps   {frame_avg:6.2} ms avg   {:6.2} ms worst\n\
             sim     tick {:<7} {tick_avg:6.3} ms avg   {:6.3} ms worst\n\
             routes  {} portals   {} waiting\n\
             ai      {} computer players   {ai_avg:6.3} ms a tick\n\
             render  {} draw calls   {}/{} chunks   {} boxes   {} soldiers   {}k triangles\n\
             memory  process {process}   heap {:.1} MB   gpu {:.1} MB\n\
             heap    {}\n\
             data    {}\n\
             audio   {}\n\
             gpu     {} via {:?}",
            ms(self.stats.frame_max),
            self.world.tick_count(),
            ms(self.stats.tick_max),
            self.world.nav().portal_count(),
            self.world.nav().queued(),
            self.minds.iter().count(),
            rs.draw_calls,
            rs.chunks_drawn,
            rs.chunks_total,
            rs.boxes_drawn,
            rs.soldiers_drawn,
            rs.triangles / 1000,
            mb(tagged_alloc::total_live_bytes()),
            mb(self.renderer.gpu_bytes() as usize),
            heap_tags.join("  "),
            self.data_label,
            self.sounds
                .as_ref()
                .map_or("off".to_string(), |s| s.overlay_line()),
            info.name,
            info.backend,
        );
        self.show_overlay(Some(&text));
    }

    /// Put the sandbox panel, then `dev` (the dev overlay's text), on screen.
    fn show_overlay(&mut self, dev: Option<&str>) {
        // The menu covers the field, so the sandbox panel waits behind it.
        let mut text = if self.menu_open {
            String::new()
        } else if self.sandbox.credits {
            credits::lines().join("\n")
        } else if self.sandbox.panel {
            self.sandbox_panel()
        } else {
            self.battle_outcome().unwrap_or_default().to_string()
        };
        if self.lens && !self.menu_open {
            let lenses = lens::lenses(&self.minds);
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(&lens::panel(&lenses, |p| party_name(&self.parties, p)));
        }
        if let Some(dev) = dev {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(dev);
        }
        let scale = self.window.scale_factor() as f32;
        self.renderer
            .set_overlay((!text.is_empty()).then_some(text.as_str()), scale);
        self.sandbox.dirty = false;
    }

    /// Tab: command the next side. The selection belongs to the old side,
    /// so it is dropped; with the drill running, it leaves the new side alone.
    fn switch_side(&mut self) {
        let sides = self.parties.len().max(1) as PlayerId;
        self.sandbox.side = (self.sandbox.side + 1) % sides;
        self.selected.clear();
        if let Some(drill) = &mut self.drill {
            drill.skip_player = Some(self.sandbox.side);
        }
        self.sandbox.dirty = true;
    }

    /// I: hand the side you command to the computer, raise its level
    /// (Recruit, Veteran, General), or take the side back.
    fn cycle_mind(&mut self) {
        let side = self.sandbox.side;
        let name = party_name(&self.parties, side);
        let (next, persona) = match self.minds.get(side) {
            None => (Some(Difficulty::Recruit), None),
            Some(m) => {
                let i = m.difficulty().index() + 1;
                let persona = gc_ai::personality::find(&m.personality().name);
                (
                    (i < Difficulty::ALL.len()).then(|| Difficulty::from_index(i)),
                    persona,
                )
            }
        };
        let Some(level) = next else {
            self.minds.remove(side);
            self.say(format!("You command {name} again."));
            return;
        };
        let faction = self.faction_of(side);
        let personality = persona.unwrap_or_else(|| {
            gc_ai::personality::pick(faction, self.world.tick_count() ^ side as u32)
        });
        let mind = Mind::new(MindConfig {
            party: side,
            difficulty: level,
            personality,
            seed: self.seed ^ (self.matches << 8) ^ side as u64,
        });
        let who = mind.personality().name.clone();
        tagged_alloc::scope(Tag::Ai, || self.minds.add(mind));
        self.say(format!(
            "The computer ({}, {who}) now plays {name}. I: next level.",
            level.name()
        ));
    }

    /// A side's faction: the lobby's choice, else what most of its
    /// regiments are.
    fn faction_of(&self, side: PlayerId) -> gc_ai::Faction {
        if let Some(f) = self.parties.get(side as usize).and_then(|p| p.faction) {
            return f;
        }
        let mut counts = [0u32; 4];
        for r in self.world.regiments.iter().filter(|r| r.owner == side) {
            if let Some(f) = self.unit_factions.get(r.unit as usize) {
                counts[f.index() as usize] += 1;
            }
        }
        let best = (0..4).max_by_key(|&f| (counts[f], 4 - f)).unwrap_or(0);
        if counts[best] == 0 {
            gc_ai::Faction::from_index(side)
        } else {
            gc_ai::Faction::from_index(best as u8)
        }
    }

    /// [ and ]: the previous or next unit type, for placing and tuning.
    fn choose_unit(&mut self, step: i32) {
        let types = self.unit_stats.len() as i32;
        self.sandbox.unit = (self.sandbox.unit as i32 + step).rem_euclid(types.max(1)) as UnitId;
        self.sandbox.dirty = true;
    }

    /// Where a regiment placed now would stand, and its facing: under the
    /// cursor, facing away from the camera.
    fn placement(&self) -> Option<(Vec2, Vec2)> {
        let at = self.ground_under_cursor()?;
        let eye = self.camera.eye();
        let facing = (self.camera.focus - Vec2::new(eye.x, eye.z)).normalize_or(Vec2::NEG_Y);
        Some((at, facing))
    }

    fn place_regiment(&mut self) {
        let Some((at, facing)) = self.placement() else {
            return;
        };
        if !self.world.nav().walkable(fx_vec(at)) {
            self.say("No regiment can stand there.".into());
            return;
        }
        self.human_orders.push(PlayerCommand {
            player: self.sandbox.side,
            command: gc_sim::Command::SpawnRegiment {
                unit: self.sandbox.unit,
                position: fx_vec(at),
                facing: gc_fixed::Angle::of_vec(fx_vec(facing)),
            },
        });
        self.ui_sound(sounds::UiSound::Move);
    }

    /// A plate under the cursor the size of the regiment B would place.
    fn push_placement_ghost(&mut self) {
        if !self.sandbox.placing {
            return;
        }
        let Some((at, facing)) = self.placement() else {
            return;
        };
        let stats = &self.unit_stats[self.sandbox.unit as usize];
        let (width, depth) = stats.block_size(stats.files, stats.models);
        let [cr, cg, cb] = color_of(&self.parties, self.sandbox.side);
        let color = if self.world.nav().walkable(fx_vec(at)) {
            [cr, cg, cb, 200]
        } else {
            BLOCKED_COLOR
        };
        self.instances.push(BoxInstance {
            pos: at.to_array(),
            facing: facing.to_array(),
            size: [fx_to_f32(width), PLATE_HEIGHT, fx_to_f32(depth)],
            color,
            lift: 0.0,
        });
    }

    /// Delete: take the selected regiments off the field, or with `all`
    /// every regiment of every side.
    fn remove_regiments(&mut self, all: bool) {
        let side = self.sandbox.side;
        let doomed: Vec<RegimentHandle> = if all {
            self.world.regiments.iter().map(|r| r.handle).collect()
        } else {
            std::mem::take(&mut self.selected)
        };
        self.human_orders
            .extend(doomed.into_iter().map(|regiment| PlayerCommand {
                player: side,
                command: gc_sim::Command::RemoveRegiment { regiment },
            }));
        self.selected.clear();
    }

    /// Up and Down pick a stat, Left and Right change it (Shift: ten steps).
    /// Returns whether the key was used.
    fn tune_key(&mut self, code: KeyCode) -> bool {
        let rows = sandbox::TUNABLES.len();
        let step = match code {
            KeyCode::ArrowUp => {
                self.sandbox.row = (self.sandbox.row + rows - 1) % rows;
                0
            }
            KeyCode::ArrowDown => {
                self.sandbox.row = (self.sandbox.row + 1) % rows;
                0
            }
            KeyCode::ArrowLeft => -1,
            KeyCode::ArrowRight => 1,
            _ => return false,
        };
        if step != 0 {
            let unit = self.sandbox.unit;
            let mut stats = self.unit_stats[unit as usize];
            let steps = if self.input.shift { step * 10 } else { step };
            (sandbox::TUNABLES[self.sandbox.row].nudge)(&mut stats, steps);
            self.tune(unit, stats.sanitized());
        }
        self.sandbox.dirty = true;
        true
    }

    /// Send new stats for a unit type, and show them straight away.
    fn tune(&mut self, unit: UnitId, stats: UnitStats) {
        self.unit_stats[unit as usize] = stats;
        self.human_orders.push(PlayerCommand {
            player: self.sandbox.side,
            command: gc_sim::Command::TuneUnit { unit, stats },
        });
    }

    /// R: read `data/units.ron` again and tune every type that changed.
    fn reload_data(&mut self) {
        let Some(dir) = sandbox::data_dir() else {
            self.say(
                "R reloads data/units.ron, but there is none here (run from the repository)."
                    .into(),
            );
            return;
        };
        let (names, stats) = match sandbox::reload(&dir) {
            Ok(fresh) => fresh,
            Err(e) => {
                eprintln!("grubcraft: reloading data: {e}");
                self.say(format!("data/units.ron has a problem: {e}"));
                return;
            }
        };
        let mut changed = 0;
        for (unit, new) in stats.iter().enumerate().take(self.unit_stats.len()) {
            let new = new.sanitized();
            if new != self.unit_stats[unit] {
                self.tune(unit as UnitId, new);
                changed += 1;
            }
        }
        for (old, new) in self.unit_names.iter_mut().zip(names) {
            *old = new;
        }
        let mut message = format!("Reloaded data/units.ron: {changed} unit types changed.");
        if stats.len() != self.unit_stats.len() {
            message += &format!(
                " It now has {} types; restart to add or remove types.",
                stats.len()
            );
        }
        self.say(message);
    }

    /// Show a line in the sandbox panel for a few seconds.
    fn say(&mut self, message: String) {
        eprintln!("grubcraft: {message}");
        self.sandbox.message = Some((message, Instant::now()));
        self.sandbox.dirty = true;
    }

    /// The sandbox panel's text.
    fn sandbox_panel(&self) -> String {
        let sb = &self.sandbox;
        let unit = sb.unit as usize;
        let name = &self.unit_names[unit];
        let speed = if self.paused {
            "paused".to_string()
        } else if self.speed == NORMAL_SPEED {
            "normal speed".to_string()
        } else {
            format!("{}x speed", SPEEDS[self.speed])
        };
        let mut lines = Vec::new();
        if let Some(o) = &self.online {
            let lan = self.menu.network();
            lines.extend(o.lines(lan.status(), lan.waiting()));
            lines.push(String::new());
        }
        if let Some(outcome) = self.battle_outcome() {
            lines.push(outcome.to_string());
            lines.push(String::new());
        }
        if self.online.is_some() {
            lines.push(format!(
                "FIELD BATTLE over the network   commanding {} (player {})   H: hide",
                party_name(&self.parties, sb.side),
                sb.side + 1
            ));
            for m in self.minds.iter() {
                lines.push(format!(
                    "  {}: {}",
                    party_name(&self.parties, m.party()),
                    m.summary()
                ));
            }
            lines.push(if self.commander {
                "view     Commander View   L: AI lens   C: credits   Esc: menu (the battle goes on)"
                    .to_string()
            } else {
                "view     soldiers   L: AI lens   C: credits   Esc: menu (the battle goes on)"
                    .to_string()
            });
            if let Some((message, _)) = &sb.message {
                lines.push(String::new());
                lines.push(message.clone());
            }
            return lines.join("\n");
        }
        let mode = if self.battle.is_some() {
            "FIELD BATTLE"
        } else {
            "SANDBOX"
        };
        lines.push(format!(
            "{mode}  commanding {} (player {})   Tab: next side   H: hide",
            party_name(&self.parties, sb.side),
            sb.side + 1
        ));
        // The computer players, and what the one on this side has in mind.
        for m in self.minds.iter() {
            lines.push(format!(
                "  {}: {}",
                party_name(&self.parties, m.party()),
                m.summary()
            ));
        }
        lines.push(match self.minds.get(sb.side) {
            None => "I: hand this side to the computer   L: AI lens".to_string(),
            Some(m) => {
                let next = Difficulty::ALL
                    .get(m.difficulty().index() + 1)
                    .map_or("take it back", |d| d.name());
                let thinking = m
                    .journal()
                    .latest()
                    .map_or(String::new(), |e| format!(": {}", e.text));
                format!("I: {next}   L: AI lens   the computer plays this side{thinking}")
            }
        });
        lines.extend([
            if sb.placing {
                format!("placing  {name}   [ ]: type   click: place   B: stop")
            } else {
                format!("B: place regiments   [ ]: type ({name})")
            },
            format!("game     {speed}   P: pause   - =: slower, faster"),
            "Delete: remove selected   Shift+Delete: clear field   T: tune stats   R: reload data"
                .to_string(),
            if self.commander {
                "view     Commander View: one banner per regiment (zoom in for soldiers)   C: credits"
                    .to_string()
            } else {
                "view     soldiers (zoom far out for the Commander View)   C: credits".to_string()
            },
        ]);
        if sb.tuning {
            let on_field = self
                .world
                .regiments
                .iter()
                .filter(|r| r.unit == sb.unit)
                .count();
            lines.push(String::new());
            lines.push(format!(
                "TUNE {name} ({on_field} on the field)   Up/Down: pick   Left/Right: change   T: close"
            ));
            let stats = &self.unit_stats[unit];
            for (k, t) in sandbox::TUNABLES.iter().enumerate() {
                let mark = if k == sb.row { '>' } else { ' ' };
                lines.push(format!("{mark} {:<15} {}", t.label, (t.show)(stats)));
            }
        }
        if let Some((message, _)) = &sb.message {
            lines.push(String::new());
            lines.push(message.clone());
        }
        lines.join("\n")
    }

    /// " · paused" or " · speed 0.25x"; empty at normal speed.
    fn speed_label(&self) -> String {
        if self.paused {
            " · paused".into()
        } else if self.speed != NORMAL_SPEED {
            format!(" · speed {}x", SPEEDS[self.speed])
        } else {
            String::new()
        }
    }

    fn stats_line(&self) -> String {
        let secs = self.stats.window_start.elapsed().as_secs_f32().max(1e-3);
        let fps = self.stats.window_frames as f32 / secs;
        let tick_ms = if self.stats.ticks > 0 {
            self.stats.tick_time.as_secs_f32() * 1000.0 / self.stats.ticks as f32
        } else {
            0.0
        };
        let mem = gc_platform::memory::process_memory();
        let rs = self.renderer.stats();
        format!(
            "Grubcraft{} · {fps:.0} fps · sim {tick_ms:.3} ms/tick · {} regiments · {}/{} chunks · {} MB process · {} MB heap · {} MB gpu",
            self.speed_label(),
            self.world.regiments.len(),
            rs.chunks_drawn,
            rs.chunks_total,
            mem.current
                .map_or("?".into(), |b| format!("{:.1}", mb(b as usize))),
            format_args!("{:.1}", mb(tagged_alloc::total_live_bytes())),
            format_args!("{:.1}", mb(self.renderer.gpu_bytes() as usize)),
        )
    }
}

/// The scripted opponents of the sandbox.
fn make_drill(seed: u64) -> Drill {
    let mut drill = Drill::new(seed);
    drill.skip_player = Some(HUMAN);
    // The last unit type is the archers: every army's back two rows.
    drill.unit_of = |player, k, types| {
        if types > 1 && k >= 40 {
            (types - 1) as UnitId
        } else {
            (player as usize % (types - 1).max(1)) as UnitId
        }
    };
    drill
}

fn print_memory(renderer: &Renderer) {
    let mem = gc_platform::memory::process_memory();
    eprintln!("memory by subsystem (heap, MB):");
    for tag in Tag::ALL {
        eprintln!(
            "  {:<8} {:>8.2}",
            tag.name(),
            mb(tagged_alloc::live_bytes(tag))
        );
    }
    eprintln!("  {:<8} {:>8.2}", "gpu", mb(renderer.gpu_bytes() as usize));
    if let Some(c) = mem.current {
        eprintln!("  process  {:>8.2}", mb(c as usize));
    }
}

/// A small square over every cell of ground no regiment can cross.
fn blocked_marks(nav: &[u8], map_tiles: u32) -> Vec<BoxInstance> {
    let per_tile = gc_sim::nav::CELLS_PER_TILE as usize;
    let side = map_tiles as usize * per_tile;
    let cell = 1.0 / per_tile as f32;
    let mut marks = Vec::new();
    for (k, _) in nav
        .iter()
        .enumerate()
        .filter(|&(_, &c)| c == gc_sim::nav::BLOCKED)
    {
        let (x, y) = (k % side, k / side);
        marks.push(BoxInstance {
            pos: [(x as f32 + 0.5) * cell, (y as f32 + 0.5) * cell],
            facing: [0.0, 1.0],
            size: BLOCKED_MARK,
            color: BLOCKED_COLOR,
            lift: BLOCKED_LIFT,
        });
    }
    marks
}

fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn to_fx(v: f32) -> gc_fixed::Fx {
    gc_fixed::Fx::from_raw((v * 65536.0) as i32)
}

fn fx_to_f32(v: gc_fixed::Fx) -> f32 {
    v.raw() as f32 / 65536.0
}

fn from_fx_vec(v: gc_fixed::FxVec2) -> Vec2 {
    Vec2::new(fx_to_f32(v.x), fx_to_f32(v.y))
}

fn fx_vec(v: Vec2) -> gc_fixed::FxVec2 {
    gc_fixed::FxVec2::new(to_fx(v.x), to_fx(v.y))
}

/// The number on a digit key, for control groups.
/// A stable value in [-1, 1) per volley, arrow and axis.
fn spread_hash(seed: u32, k: usize, axis: u32) -> f32 {
    let mut x = seed ^ (k as u32).wrapping_mul(0x85EB_CA6B) ^ axis.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x >> 8) as f32 / (1u32 << 23) as f32 - 1.0
}

fn digit(code: KeyCode) -> Option<usize> {
    const DIGITS: [KeyCode; 10] = [
        KeyCode::Digit0,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    DIGITS.iter().position(|&d| d == code)
}

/// Unit vector for a simulation angle (0 = +x, a quarter turn = +y).
fn angle_vec(a: gc_fixed::Angle) -> Vec2 {
    Vec2::from_angle(a.0 as f32 / 65536.0 * std::f32::consts::TAU)
}

fn main() {
    let opts = parse_args();
    profile::start();
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("grubcraft: cannot open a window: {e}");
            std::process::exit(1);
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        opts: Some(opts),
        ..Default::default()
    };
    app.memory_timeline.mark("process start");
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("grubcraft: {e}");
        std::process::exit(1);
    }
}
