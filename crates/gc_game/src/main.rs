//! Grubcraft. M1 sandbox: a rendered map with four players' regiments driven
//! by the deterministic simulation. You command player 1 (red) to start
//! with; the other three wander the map on their own (`--sandbox` starts on
//! an empty field instead, with nobody moving unless you order it).
//!
//! Sandbox: Tab switches which side you command (all four can be ordered).
//! B turns placing on and off: left-click puts down a regiment of the chosen
//! type for that side, facing away from the camera, and [ and ] choose the
//! type. The top bar's Place button (or B) opens a panel listing the types
//! (the Field types, or any faction's roster troops, which starts the
//! sandbox again on the Conquest table) and sides to click
//! (docs/08-ui-ux.md#the-place-panel). K does the same for buildings (the sandbox only): click puts the
//! chosen building down, centred on the cursor, and [ and ] choose which.
//! While a building is being placed (K, or a worker's build menu in Conquest)
//! R, or the mouse wheel instead of zooming, turns it a quarter turn clockwise
//! (Shift+R back), and the ghost and the order carry the turn.
//! Delete (or Backspace) takes the building under the cursor, else the
//! selected regiments, off the field; Shift + Delete clears it. Right-click
//! an enemy building to send the selected regiments against it. In Conquest, with one of your
//! finished towers selected, right-click an enemy regiment to make the tower shoot it first, or open
//! ground to let it choose again. P pauses; - and = slow the game down to a tenth
//! or speed it up to four times. T opens the stats panel for the chosen type:
//! Up and Down pick a stat, Left and Right change it (Shift for ten steps),
//! and regiments on the field change at once (on whichever table the sandbox
//! runs on). Shift+R reloads `data/units.ron` when the game runs from the
//! repository and was built with `--features dev`; on the roster table it
//! says to choose a Field type first, as the roster is baked, not read
//! from disk.
//! H, F1 or the ? Help button in the top bar opens the Marshal's Scroll: the keys
//! in groups, and what each computer player is doing and expects (F1 finds an
//! idle worker instead while a Conquest match has one). C shows the credits for
//! the art. I hands the side you command to the battle AI (Recruit, then
//! Veteran, then General, then back to you).
//!
//! Soldiers are low-poly models (baked from free packs by `gc_artbake`) that
//! walk, run, fight, shoot and fall with their own animations; parts of each
//! take the team colour. Zoom far out and the Commander View replaces them
//! with one block and banner per regiment.
//!
//! The top bar says what you are playing (mode, side, speed, clock) and, in
//! Conquest, your stores. The bottom bar has a minimap (click or drag to move the camera, right-click
//! to send the selection there) and an army tray of regiment cards (click to
//! select, Shift-click to toggle, double-click to centre). The command grid
//! has Q (move) and W (attack), each followed by a left-click, and R (formation).
//! Badges on the screen edge point at your regiments and buildings under attack,
//! ambushes and routs that are out of view; click one or press Space to look there.
//! Shift+H previews the hero panel (there are no heroes yet).
//! Orders: left-click one of your regiments to select it, or left-drag a box around
//! several; Shift adds or removes. Right-click the ground to march the
//! selection there side by side, facing the way they travel; right-drag to
//! draw the line they form instead (its length sets their frontage; they face
//! away from the camera side of the line); a big selection forms several
//! lines. Right-click an enemy regiment to attack it; archers (each army's
//! back two rows) also shoot enemies in range on their own, and blocks that
//! touch fight. R (or F) changes the selection's formation (not while locked in
//! melee): Line, Tight (stronger to hold, cramped for swinging blades),
//! Loose (gaps for blades to swing and arrows to miss) and, for regiments
//! with shields, Shield wall (turns arrows from the front, marches at half
//! pace, never charges). Z braces spears and pikes against a charge (they
//! stand until given another order; Z again lifts them) and V pulls the
//! selection out of its fights in good order: it backs away 8 tiles facing
//! the enemy, who gets one parting blow and doesn't follow. X lights the
//! selection's torches at night (X again puts them out): seen from afar,
//! but they see further and can't be ambushed. Banners show morale: blinking amber is wavering,
//! white is routing (no orders until it rallies), grey is shattered.
//! Regiments find their own way around cliffs and deep water; a selected
//! regiment's route shows as a dotted line, and G shows the ground no
//! regiment can cross.
//! Woods and tall grass hide regiments (docs/20-ambushes.md): enemies your
//! side can't see aren't drawn, yours that the enemy can't see carry a
//! moss-green flag, and a sprung ambush shows an "AMBUSH!" message with a
//! drum and horn. Shift+V in the sandbox shows every regiment, hidden or not.
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
//! pinch zooms; Alt + middle-drag, two-finger rotate, or `,` and `.` turn the view;
//! middle-click or Home resets the rotation; ` (backquote) or F11 toggles the
//! dev overlay and prints memory by subsystem;
//! Y, with a regiment selected, rides with it: the camera hangs just above
//! head height behind its rear ranks, looking where it faces, so you feel
//! you are in the line, and selecting and ordering work as usual (in a
//! replay too). Y or Esc again, or any pan, zoom or rotate (arrow keys,
//! screen edges, scrolling, the wheel, a pinch or twist, middle-drag, a click
//! on the minimap), brings back the bird's-eye view over that regiment. A
//! regiment that shatters or is destroyed hands the view back by itself; a
//! routing one is still followed;
//! Esc leaves placing, the stats panel or the credits, and otherwise opens
//! the menu.
//!
//! Flags: `--sandbox` starts on an empty field; `--battle recruit|veteran|general|warlord`
//! starts a Field Battle against one computer player of that level;
//! `--conquest ribcage_pass|four_teeth` starts Conquest on that map, every
//! other start held by an idle side;
//! `--host` opens a LAN lobby at once and `--join ADDRESS` joins one; `--no-menu` skips the main menu
//! (`--frames` implies it, `--menu` overrides); `--seed N` picks the map seed the Host a game card and a `--battle` start on; `--frames N` quits after N frames
//! and prints a final stats line (for measuring memory on real machines);
//! `--map NAME|FILE` opens on a shipped map (`ribcage_pass`, `four_teeth`) or
//! a map file instead of the seeded map, to look round it in the sandbox
//! (Field Battles, replays and network matches still play on seeded maps,
//! and switch to one);
//! `--overlay` starts with the dev overlay shown; `--no-audio` never opens
//! the sound card; `--no-shadows`, `--no-glow` and `--no-aa` turn off the sun's
//! shadows, the glow around bright light and the anti-aliasing; `--latency 1|2|3` sets
//! frames in flight; `--render-scale F` renders at a
//! fraction of the window's pixels; `--draw all|terrain|blocks|none` turns
//! parts of the frame off to measure what they cost; `--lod 0..3` draws
//! every soldier at one level of detail, to inspect it;
//! `--showroom all|buildings|NAME[,NAME...]` stands the named models (names as in
//! `assets-src/models.ron` and `assets-src/buildings/*.ron`; `all` is every
//! soldier, `buildings` every building stage) in a row on an empty field under a fixed camera,
//! cycling through idle, walk, attack and death, and `--showroom-clip
//! idle|walk|attack|shoot|cheer|die|march|look|kill|finish|blows|reel` holds one clip instead (`march`
//! walks with the attack on the upper body, `look` idles looking round, `kill`
//! plays a half of a matched kill over and over, `finish` a finisher, `blows`
//! a soldier's blows of a fight in turn, `reel` the ways a regiment reels);
//! `--viewer NAME|start` opens the Unit viewer (main menu) on the named unit,
//! or on the first unit with a model; `--fx-demo` sets off every particle
//! effect at the camera's focus in slow motion, and `--fx-still` freezes them
//! at several ages, to look at them.

mod aftermath;
mod ambient;
mod army_shape;
mod atmosphere;
mod buildings;
mod camera;
mod cinema;
mod clearing;
mod conquest;
mod conquest_ai;
mod conquest_card;
#[cfg(test)]
mod conquest_golden;
#[cfg(test)]
mod conquest_soak;
mod credits;
mod data;
mod director;
mod effects;
mod faction_powers;
mod field;
mod fog;
mod fx_demo;
#[cfg(test)]
mod goldens;
mod ground_look;
mod land;
mod lights;
// The Guide's data (docs/08-ui-ux.md#guide).
mod guide_data;
mod hud;
mod lens;
mod map_preview;
mod models;
mod moves;
mod online;
mod profile;
mod puppets;
mod replays;
mod ring_tool;
mod roster;
mod sandbox;
mod scars;
mod scenery;
mod showroom;
mod siege;
mod sounds;
mod viewer;
mod wall_tool;

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use gc_ai::{Difficulty, Mind, MindConfig, Minds};
use gc_assets::model::LODS;
use gc_match::battle::{self, Battle, Party, party_name};
use gc_match::terrain_view;
use gc_net::Lan;
use gc_platform::logln;
use gc_platform::tagged_alloc::{self, Tag, TaggedAlloc};
use gc_render::{
    BoxInstance, FrameParams, HitStyle, Renderer, RendererOptions, SoldierBatch, SoldierInstance,
    Terrain, UiQuad,
};
use gc_sim::arrange::Shape;
use gc_sim::events::Event;
use gc_sim::formation::{GroupMember, GroupOrder};
use gc_sim::scenario::Drill;
use gc_sim::structures::Breaker;
use gc_sim::{
    BuildingId, BuildingStats, Capacities, Formation, Growth, LOOSE_DEFENCE, MatchSetup,
    MoraleState, PlayerCommand, PlayerId, RegimentHandle, StructureHandle, StructureView,
    TIGHT_DEFENCE, UnitId, UnitStats, World,
};
use gc_ui::lobby::{GameMode, MatchStart};
use gc_ui::menu::MapPreviewKey;
use gc_ui::{Menu, MenuAction, Network, Screen, Settings, Ui, UiInput};
use glam::{Vec2, Vec3};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use camera::{ActiveCamera, FollowCamera, RideTarget, RtsCamera};
use gc_map::{FieldMap, Map};
use models::Action;
use puppets::{Block, Fighter, Puppets, Soldier};

#[global_allocator]
static ALLOC: TaggedAlloc = TaggedAlloc;

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
/// How far above the water line the ground still holds mist (a ford's ramps
/// and a fen's low shores lie within it), how far in from the map's edge it
/// begins, and its colour before the light: pale, for the sun and the sky to
/// colour.
const MIST_BANK: f32 = 0.3;
const MIST_EDGE: f32 = 3.0;
const MIST_TINT: [u8; 3] = [214, 222, 232];
/// A side's general (see `gc_sim::generals`) stands this much taller than
/// his men, in his side's colour lightened by [`GENERAL_LIGHT`].
const GENERAL_SCALE: f32 = 1.15;
/// How far a man finishing one on the ground bends his chest and head down
/// to him, as `SoldierInstance::look` pitches (127 is an eighth of a turn).
const FINISH_PITCH: i8 = 110;
const GENERAL_LIGHT: f32 = 0.35;

/// The colour a general is drawn in: his side's, lightened, with his own
/// tone.
fn general_colour([r, g, b]: [u8; 3], tone: u8) -> [u8; 4] {
    let light = |c: u8| (f32::from(c) + (255.0 - f32::from(c)) * GENERAL_LIGHT) as u8;
    [light(r), light(g), light(b), tone]
}
/// Soldiers drawn at least this many pixels tall use levels of detail 0, 1
/// and 2; smaller ones the last.
const LOD_PIXELS: [f32; LODS - 1] = [100.0, 40.0, 16.0];
/// Screen pixels over which head turns and flinches fade in above the
/// nearest level of detail.
const HEAD_FADE_PIXELS: f32 = 30.0;
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
/// Ground height for the cameras: the terrain, or the water's surface where
/// it is deeper than the bed.
fn ground_of(map: &Map, p: Vec2) -> f32 {
    map.height_at(p.x, p.y).max(map.water_level())
}

/// While riding, a ray picks a block of soldiers this far outside its edges,
/// and from the ground up to this height (soldiers are up to 1.6 tall, and
/// their banners rise above them), in tiles: from a low camera a ground
/// point is a poor guide.
const RIDE_PICK_MARGIN: f32 = 0.5;
const RIDE_PICK_HEIGHT: f32 = 3.0;
/// The most burning tiles drawn ablaze at once: across a bigger fire,
/// every so many (each also lights the ground).
const MOST_FIRES_DRAWN: usize = 96;
/// Thickness of the ground plate drawn under a selected regiment.
const PLATE_HEIGHT: f32 = 0.05;
/// The command grid's keys, slot by slot (`gc_ui::hud::GRID_KEYS`).
const GRID_CODES: [KeyCode; gc_ui::hud::GRID_SLOTS] = [
    KeyCode::KeyQ,
    KeyCode::KeyW,
    KeyCode::KeyE,
    KeyCode::KeyR,
    KeyCode::KeyA,
    KeyCode::KeyS,
    KeyCode::KeyD,
    KeyCode::KeyF,
    KeyCode::KeyZ,
    KeyCode::KeyX,
    KeyCode::KeyC,
    KeyCode::KeyV,
];
/// Dashes of the ragged outline round a regiment out of order: length
/// and spacing, in tiles.
const DISORDER_DASH: f32 = 0.55;
const DISORDER_DASH_SPACING: f32 = 0.9;
const DISORDER_GLOW: u8 = 110;
/// The deployment zone's ring: dash length and spacing, tiles.
const ZONE_DASH: f32 = 1.2;
const ZONE_DASH_SPACING: f32 = 2.0;
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
/// The flag over a regiment of yours that no enemy can see
/// (docs/20-ambushes.md): moss green, above the treetops.
const HIDDEN_FLAG: [f32; 3] = [0.9, 0.5, 0.12];
/// A regiment's burning torches (picture only): one warm light over the
/// block, `TORCH_LIGHT_HEIGHT` tiles up, reaching `TORCH_LIGHT_RADIUS` (what
/// the simulation's torchlight reveals is 10 tiles from the block's edge; the
/// light a little less, since the nearest 24 lights share the frame), one
/// more for every `TORCH_LIGHT_SPACING` tiles of a wide front (at most
/// three); and about a quarter of the soldiers hold a flame at
/// `TORCH_HEIGHT` of their height.
const TORCH_LIGHT_HEIGHT: f32 = 1.4;
const TORCH_LIGHT_RADIUS: f32 = 7.0;
const TORCH_LIGHT_STRENGTH: f32 = 0.8;
const TORCH_LIGHT_SPACING: f32 = 9.0;
const TORCH_HEIGHT: f32 = 0.8;
const TORCH_FIRE: [u8; 3] = [255, 160, 70];
/// The Brood's glow at night (their Grub lime, `docs/24`): a faint, small
/// light over each Brood regiment that is not burning torches.
const GRUB_LIME: [u8; 3] = [150, 235, 70];
const GRUB_LIGHT_RADIUS: f32 = 4.0;
const GRUB_LIGHT_STRENGTH: f32 = 0.4;

/// Whether a soldier with this `phase` (a stable number in [0, 1)) carries
/// a lit torch: a quarter of them.
fn torch_bearer(phase: f32) -> bool {
    (phase * 64.0) as u32 & 3 == 0
}
const HIDDEN_LIFT: f32 = 3.4;
const HIDDEN_COLOR: [u8; 4] = [95, 150, 70, 120];
/// Fallen soldiers kept on the field at once: they lie there for the whole
/// battle, and only on a field this full does the oldest sink away. A
/// fallen soldier costs 24 bytes in the puppets and 36 to draw, so this is
/// 768 KB and over five times the 6,052 soldiers of the grand battle
/// (`field-8000pt-2v2`), the biggest we run. The renderer's own room for
/// soldiers (65,536) is the real limit on what can be drawn.
const MAX_CORPSES: usize = 32_768;
/// Boxes kept free of the dead for what each frame draws after them.
const BOX_ROOM_AFTER_CORPSES: usize = 16_384;
/// How much bigger than the model a crow of the aftermath is drawn: the model
/// is the dove's size, a crow is bigger.
const CROW_SCALE: f32 = 1.25;

/// A body lying down shows about a third of the height of one standing, so
/// it is drawn this many levels of detail coarser than the living; past the
/// coarsest model, a plain box (10 triangles).
const CORPSE_LOD_BIAS: usize = 1;
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
/// How near a building a regiment's centre stands, past half its depth, to
/// be at the wall (the simulation lets its front rank reach about two tiles
/// from it).
const WALL_REACH: f32 = 3.4;
/// How far from the tile they clear workers stand and swing, in tiles.
const CUT_REACH: f32 = 2.4;
/// A building `--siege` puts up: its name, where (from the camera's focus),
/// and its attackers: unit, and how far across and south of it they start.
type SceneBuilding = (&'static str, Vec2, &'static [(&'static str, f32, f32)]);

/// How far off `--siege` looks at its siege.
const SIEGE_SCENE_DISTANCE: f32 = 42.0;
/// How far a ram draws back between its blows, in tiles.
const RAM_DRAW: f32 = 0.5;

/// Blows shown with sparks or wounds in one frame, at most.
const BLOWS_SHOWN: usize = 48;
/// Melee defence from which a soldier's armour rings under a blow.
const ARMOURED_DEFENCE: u8 = 45;

/// Arrows leave the bow and land at about this height above the ground.
const ARROW_HEIGHT: f32 = 0.9;

/// Game speeds the sandbox steps through with - and =, and the normal one.
const SPEEDS: [f32; 6] = [0.1, 0.25, 0.5, 1.0, 2.0, 4.0];
const NORMAL_SPEED: usize = 3;
/// How long a sandbox message (such as the result of reloading data) shows.
const MESSAGE_TIME: Duration = Duration::from_secs(5);
/// Sides in the sandbox and the drill; Tab cycles through them.
const PLAYERS: u8 = 4;
const TEAM_NAMES: [&str; 4] = ["red", "violet", "cyan", "bone"];
/// How much faster a replay plays the deployment phase.
const DEPLOY_REPLAY_SPEED: u32 = 8;
/// Most ticks run in one frame, so a slow frame can't snowball (4x speed at
/// 30 fps needs about 3).
const MAX_TICKS_PER_FRAME: u32 = 8;
/// How long a frame may spend simulating when an online match is catching
/// up after a rejoin.
const CATCH_UP_BUDGET: Duration = Duration::from_millis(25);

const _: () = assert!(gc_data::MAX_MODELS == gc_sim::formation::MAX_MODELS);
// The baked weapon bits and masses mean the same to the simulation.
const _: () = assert!(
    gc_data::WEAPON_REACH == gc_sim::WEAPON_REACH
        && gc_data::WEAPON_SWING_ROOM == gc_sim::WEAPON_SWING_ROOM
        && gc_data::WEAPON_SHIELD == gc_sim::WEAPON_SHIELD
        && gc_data::WEAPON_DISCIPLINE == gc_sim::WEAPON_DISCIPLINE
        && gc_data::MAX_MASS == gc_sim::Mass::Heavy as u8
);

struct Options {
    seed: u64,
    /// A described map to open on (`--map`) instead of the seeded one.
    map: Option<gc_map::MapDef>,
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
    /// Start straight into Conquest on this lobby Conquest map.
    conquest: Option<usize>,
    /// Open a LAN lobby at once (`--host`), or join one (`--join ADDRESS`
    /// or join code), with `--password` if it has one.
    host: bool,
    join: Option<String>,
    password: String,
    /// Panic once the game has started, to check that a crash report is
    /// written (`--crash-test`).
    crash_test: bool,
    /// Draw every soldier at this level of detail (`--lod N`).
    lod: Option<usize>,
    /// Stand these models in a row to look at (`--showroom`), doing this
    /// (`--showroom-clip`).
    showroom: Option<Vec<usize>>,
    /// Open this recorded battle at once, as a film (`--watch FILE`).
    watch: Option<std::path::PathBuf>,
    /// The time of day and the sky a `--battle` or `--conquest` is played
    /// in, as the lobby would set them (`--time-of-day`, `--sky`).
    time_of_day: Option<gc_ui::lobby::TimeOfDay>,
    sky: Option<gc_ui::lobby::WeatherSetting>,
    showroom_clip: Option<showroom::Clip>,
    /// Open the unit viewer at start (`--viewer`): on this unit, or the
    /// first one for "start".
    viewer: Option<String>,
    /// The light and grade every match opens with (`--look golden-hour`), in
    /// place of the lobby's time of day; F9 cycles it.
    look: Option<gc_render::LookPreset>,
    /// The weather every match opens in (`--weather rain|snow|fog`); F12
    /// steps through them.
    weather: Option<gc_render::Weather>,
    /// A land whose air lights the whole map (`--air waste`).
    air: Option<gc_map::def::Land>,
    /// How far the camera starts from the ground (`--distance 14..260`).
    distance: Option<f32>,
    /// Set off every effect at the camera's focus (`--fx-demo`, or
    /// `--fx-still` to stop time).
    fx: Option<fx_demo::Mode>,
    /// Open on a siege staged in the sandbox, to look at (`--siege`).
    siege: bool,
    /// Fell the trees round the camera's focus a few seconds in
    /// (`--fell-demo`), to look at them coming down.
    fell_demo: bool,
    /// Look at the first hamlet (`--hamlet-demo`).
    hamlet_demo: bool,
    render: RendererOptions,
}

fn parse_args() -> Options {
    let mut opts = Options {
        seed: gc_ui::lobby::DEFAULT_MAP_SEED,
        map: None,
        sandbox: false,
        frames: None,
        overlay: false,
        menu: None,
        audio: true,
        battle: None,
        time_of_day: None,
        sky: None,
        conquest: None,
        host: false,
        join: None,
        password: String::new(),
        crash_test: false,
        lod: None,
        showroom: None,
        watch: None,
        showroom_clip: None,
        viewer: None,
        look: None,
        weather: None,
        air: None,
        distance: None,
        fx: None,
        siege: false,
        fell_demo: false,
        hamlet_demo: false,
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
        if flag == "--siege" {
            opts.siege = true;
            opts.menu = Some(false);
            continue;
        }
        if flag == "--host" {
            opts.host = true;
            continue;
        }
        if flag == "--fx-demo" || flag == "--fx-still" {
            opts.fx = Some(if flag == "--fx-demo" {
                fx_demo::Mode::Live
            } else {
                fx_demo::Mode::Still
            });
            continue;
        }
        if flag == "--hamlet-demo" {
            opts.hamlet_demo = true;
            continue;
        }
        if flag == "--fell-demo" {
            opts.fell_demo = true;
            continue;
        }
        if flag == "--crash-test" {
            opts.crash_test = true;
            continue;
        }
        if flag == "--no-audio" {
            opts.audio = false;
            continue;
        }
        if flag == "--no-shadows" {
            opts.render.shadows = false;
            continue;
        }
        if flag == "--no-aa" {
            opts.render.msaa = 1;
            continue;
        }
        if flag == "--no-glow" {
            opts.render.glow = false;
            continue;
        }
        let value = args.next().unwrap_or_default();
        let ok = match flag.as_str() {
            "--seed" => value.parse().map(|v| opts.seed = v).is_ok(),
            "--map" => {
                let def = gc_map::shipped(&value)
                    .map_or_else(|| gc_map::MapDef::load(std::path::Path::new(&value)), Ok);
                if let Err(e) = &def {
                    eprintln!("--map {value}: not a shipped map, and {e}");
                }
                opts.map = def.ok();
                opts.map.is_some()
            }
            "--join" => {
                opts.join = Some(value);
                true
            }
            "--password" => {
                opts.password = value;
                true
            }
            "--frames" => value.parse().map(|v| opts.frames = Some(v)).is_ok(),
            "--conquest" => {
                let choice = conquest::choice_of(&value);
                choice.map(|c| opts.conquest = Some(c)).is_some()
            }
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
            "--lod" => match value.parse() {
                Ok(v) if v < LODS => {
                    opts.lod = Some(v);
                    true
                }
                _ => false,
            },
            "--watch" if !value.trim().is_empty() => {
                opts.watch = Some(value.trim().into());
                true
            }
            "--showroom" if !value.trim().is_empty() => {
                opts.showroom = Some(showroom_models(&value));
                true
            }
            "--viewer" if !value.trim().is_empty() => {
                opts.viewer = Some(value.trim().to_string());
                true
            }
            "--time-of-day" => by_name(
                &gc_ui::lobby::TimeOfDay::ALL,
                gc_ui::lobby::TimeOfDay::name,
                &value,
            )
            .map(|t| opts.time_of_day = Some(t))
            .is_some(),
            "--sky" => by_name(
                &gc_ui::lobby::WeatherSetting::ALL,
                gc_ui::lobby::WeatherSetting::name,
                &value,
            )
            .map(|w| opts.sky = Some(w))
            .is_some(),
            "--look" => gc_render::LookPreset::from_name(&value)
                .map(|p| opts.look = Some(p))
                .is_some(),
            "--weather" => gc_render::Weather::from_name(&value)
                .map(|w| opts.weather = Some(w))
                .is_some(),
            "--air" => gc_map::def::Land::ALL
                .into_iter()
                .find(|l| format!("{l:?}").eq_ignore_ascii_case(value.trim()))
                .map(|l| opts.air = Some(l))
                .is_some(),
            "--distance" => match value.parse::<f32>() {
                Ok(v) if (camera::MIN_DISTANCE..=camera::MAX_DISTANCE).contains(&v) => {
                    opts.distance = Some(v);
                    true
                }
                _ => false,
            },
            "--showroom-clip" => showroom::Clip::parse(&value)
                .map(|c| opts.showroom_clip = Some(c))
                .is_some(),
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
    if opts.showroom_clip.is_some() && opts.showroom.is_none() {
        usage();
    }
    // The showroom is not a match: nothing to fight, host or join.
    if opts.showroom.is_some()
        && (opts.sandbox
            || opts.battle.is_some()
            || opts.conquest.is_some()
            || opts.host
            || opts.join.is_some())
    {
        usage();
    }
    if opts.showroom.is_some() {
        // The showroom goes straight into the field.
        opts.menu = Some(false);
    }
    // The viewer is over the menu: it is not a match, or the showroom.
    if opts.viewer.is_some()
        && (opts.sandbox
            || opts.battle.is_some()
            || opts.conquest.is_some()
            || opts.host
            || opts.join.is_some()
            || opts.showroom.is_some())
    {
        usage();
    }
    if opts.viewer.is_some() {
        opts.menu = Some(true);
    }
    opts
}

/// The models `--showroom` names, or exit with the valid names listed.
fn showroom_models(spec: &str) -> Vec<usize> {
    let models = models::load().unwrap_or_else(|e| {
        eprintln!("grubcraft: {e}");
        std::process::exit(1);
    });
    showroom::choose(spec, &models).unwrap_or_else(|unknown| {
        let names: Vec<&str> = models
            .models
            .iter()
            .map(gc_assets::model::ModelRecord::name)
            .collect();
        eprintln!(
            "grubcraft: --showroom: no model named {unknown:?}; the models are all, or any of: {}",
            names.join(", ")
        );
        std::process::exit(2);
    })
}

fn usage() -> ! {
    eprintln!(
        "usage: grubcraft [--menu|--no-menu] [--sandbox] [--battle recruit|veteran|general|warlord] [--conquest ribcage_pass|four_teeth] [--host] [--join ADDRESS] [--seed N] [--map NAME|FILE] [--frames N] [--overlay] [--no-audio] [--no-shadows] [--no-glow] [--no-aa] [--latency 1|2|3] \
         [--render-scale 0.25..1] [--draw all|terrain|blocks|none] [--lod 0..3] [--look noon|golden-hour|overcast|storm|dusk|night] [--weather clear|rain|snow|fog] [--time-of-day golden-hour|noon|dusk|overcast|night] [--sky fair|changing|rain|snow|fog] [--air meadow|forest|fen|highland|waste|valley] [--distance 14..260] [--showroom all|buildings|NAME[,NAME...]] [--showroom-clip idle|walk|attack|shoot|cheer|die|march|look|kill|finish|blows|reel] [--viewer NAME|start] [--watch REPLAY_FILE] [--fx-demo] [--fx-still]"
    );
    std::process::exit(2);
}

/// Everything that exists once the window and GPU are up.
struct Running {
    window: Arc<Window>,
    renderer: Renderer,
    /// The light the renderer is set to: the look of the match (F9 steps
    /// through them) in the air of the lands, Clarity and the tuning panel.
    atmosphere: atmosphere::Atmosphere,
    /// The picture's screen shake and letterbox bars.
    cinema: cinema::Cinema,
    /// Whether the deployment was still going last frame, to see a battle begin.
    was_deploying: bool,
    /// The camera that finds the fight (F8; starts on for a replay).
    director: director::Director,
    /// How far the camera starts from the ground when `--distance` says.
    start_distance: Option<f32>,
    camera: RtsCamera,
    /// Set while riding with a regiment; that camera is then the one in use.
    ride: Option<Ride>,
    heightmap: Arc<Map>,
    /// Which lobby map `heightmap` is; `None` for one opened with `--map`.
    map_key: Option<MapKey>,
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
    /// How far between the last two ticks the last frame drew, 0 to 1.
    /// Picking uses it to aim at what is on screen.
    alpha: f32,
    instances: Vec<BoxInstance>,
    puppets: Puppets,
    /// Particles collected this frame (dust, sparks, smoke), handed to the
    /// renderer just before it draws.
    effects: effects::Field,
    /// The lamps and fires that light the ground at night.
    lights: lights::Lights,
    /// The living world: trees coming down as land is cleared.
    ambient: ambient::Ambient,
    /// Spent arrows stuck in the ground and the fallen.
    arrows: field::Arrows,
    /// Blows on buildings, shots at them, and buildings coming down.
    siege: siege::Siege,
    /// Fallen standards, crows and grime (see [`aftermath`]).
    aftermath: aftermath::Aftermath,
    /// How far `--siege` has staged its siege: 0 to place it, 1 to order
    /// the attacks.
    siege_scene: Option<u8>,
    /// Soldier models; None if the embedded pack failed to load (boxes then).
    art: Option<Art>,
    /// Soldiers have given way to one banner block per regiment (zoomed out).
    commander: bool,
    /// Regiments that loosed a volley lately, by pool index.
    shooting: Vec<bool>,
    /// The match's regiment types.
    units: UnitTable,
    /// The regiment types not in play: the Conquest ones in a Field Battle
    /// or the sandbox, the Field Battle ones in Conquest. See
    /// [`Running::use_play`].
    other_units: UnitTable,
    /// Which regiment types are in play, and whether it is Conquest.
    play: Play,
    /// The building types, for Conquest worlds.
    buildings: buildings::Table,
    /// The game data as built, which Conquest matches are set up from.
    data: gc_data::GameData<'static>,
    sandbox: Sandbox,
    selected: Vec<RegimentHandle>,
    /// The building selected, when no regiment is (Conquest).
    selected_structure: Option<StructureHandle>,
    /// The idle worker F1 last picked, to go on from there.
    idle_from: Option<u32>,
    /// The formation F last ordered, for which selection and on which tick:
    /// pressing F again soon after cycles on from it, before the orders
    /// have landed.
    formation_pick: Option<(Vec<RegimentHandle>, Formation, u32)>,
    /// The selection's formation as the panel last showed it.
    shown_formation: Option<Option<Formation>>,
    /// The shape a group order arranges the selection in, and your saved ones.
    army_shapes: army_shape::ArmyShapes,
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
    /// The map's trees and tall grass.
    scenery: scenery::Scenery,
    /// Where the scenery pack's models are, by name.
    scenery_models: scenery::SceneryModels,
    /// The models on show instead of a match, with `--showroom`.
    showroom: Option<showroom::Showroom>,
    /// The unit viewer, over the main menu, while it is open.
    viewer: Option<viewer::Viewer>,
    /// Wheel movement over the viewer's panels not yet worth a whole row.
    viewer_scroll: f32,
    /// The Guide (every building and troop), over the menu or the battle.
    guide: Option<gc_ui::guide::Guide>,
    /// Wheel movement over the Guide not yet worth a whole row.
    guide_scroll: f32,
    /// Wheel fractions not yet a whole quarter turn of a building.
    turn_scroll: f32,
    /// Each building's tips, for the HUD's tips card.
    tip_book: guide_data::TipBook,
    /// Sandbox: draw regiments hidden from the side you command too (Shift+V).
    reveal_all: bool,
    /// The fog of war on the ground, when the match has it.
    fog: fog::FogView,
    land: land::LandView,
    /// Turned earth and worn tracks on the ground.
    scars: scars::ScarView,
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
    /// The in-battle bar: minimap and army tray.
    hud: hud::HudState,
    menu_open: bool,
    /// Whether a match has started (the sim is frozen until then).
    match_started: bool,
    /// A match over the network, whose turns come from the host.
    online: Option<online::Online>,
    /// The battle being recorded, or the replay being watched.
    recording: Option<Recording>,
    watching: Option<Watching>,
    /// The game data's short hash, for replays.
    data_hash: u64,
    quit: bool,
    /// None with `--no-audio` or when there is no sound output.
    sounds: Option<sounds::Sounds>,
    /// The replay list being read on another thread, until it arrives.
    replay_list: Option<replays::Listing>,
    /// Makes the pictures of maps the menus show, off the frame loop.
    map_previews: map_preview::MapPreviews,
}

/// A table of regiment types as the game names, draws and prices them.
/// Field Battles and the sandbox play with the Field Battle units, Conquest
/// with its own (`GameData::conquest_units`), whose ids number another
/// table.
struct UnitTable {
    /// As the simulation sees them (the sandbox may have tuned them).
    stats: Vec<UnitStats>,
    /// As built, before any sandbox tuning.
    data_stats: Vec<UnitStats>,
    names: Vec<String>,
    /// Each type's faction, as the AI names them.
    factions: Vec<gc_ai::Faction>,
    /// Each type's cost in a Field Battle army.
    points: Vec<u32>,
    /// Each type's soldier model, if it has one (none without art).
    models: Vec<Option<usize>>,
    /// Each type's place in an arranged army, from its class; empty for the
    /// Field types, which have none (their stats decide).
    roles: Vec<gc_sim::arrange::Role>,
    /// Each type's blows on buildings, from its class; empty for the Field
    /// types, which are all plain regiments.
    breakers: Vec<Breaker>,
}

impl UnitTable {
    fn of<'a>(
        units: impl Iterator<Item = &'a gc_data::UnitRecord>,
        models: Option<&gc_assets::model::Models<'_>>,
    ) -> UnitTable {
        let units: Vec<&gc_data::UnitRecord> = units.collect();
        let stats: Vec<UnitStats> = units.iter().map(|u| gc_match::stats_of(u)).collect();
        UnitTable {
            data_stats: stats.clone(),
            stats,
            names: units.iter().map(|u| u.name().to_string()).collect(),
            factions: units.iter().map(|u| u.faction()).collect(),
            points: units.iter().map(|u| u32::from(u.points)).collect(),
            models: models.map_or_else(Vec::new, |m| {
                units.iter().map(|u| m.by_name(u.name())).collect()
            }),
            roles: Vec::new(),
            breakers: Vec::new(),
        }
    }
}

/// Riding with one regiment: which one, and the close camera following it.
struct Ride {
    regiment: RegimentHandle,
    camera: FollowCamera,
}

/// Soldier models and the lists that draw them, rebuilt every frame.
struct Art {
    clips: models::Clips,
    /// Which models dress the buildings, stage by stage.
    looks: buildings::Looks,
    /// Each model's height, in tiles.
    heights: Vec<f32>,
    /// The deer of the ambient wildlife, if the pack has it.
    deer: Option<usize>,
    /// The dove of the ambient birds, if the pack has it.
    dove: Option<usize>,
    /// The crow of the battlefield's aftermath, if the pack has it (else
    /// the crows are boxes).
    crow: Option<usize>,
    /// The villagers' models, by `ambient::villagers::Kind`, if the pack
    /// has them all.
    folk: Option<[usize; 4]>,
    /// The ox cart of the caravans between hamlets, if the pack has it.
    cart: Option<usize>,
    /// The insects' models, by `ambient::insects::Kind`, if the pack has
    /// them all.
    insects: Option<[usize; 3]>,
    /// This frame's camera, for choosing levels of detail.
    view: LodView,
    /// Instances per model and level of detail (model * LODS + lod).
    buckets: Vec<Vec<SoldierInstance>>,
    soldiers: Vec<SoldierInstance>,
    batches: Vec<SoldierBatch>,
}

/// The level of detail for a model `height` tiles tall, `depth` tiles in
/// front of the eye (`pixel_scale` is from `RtsCamera::pixel_scale`).
fn lod_for(height: f32, depth: f32, pixel_scale: f32) -> usize {
    let pixels = height * pixel_scale / depth.max(1.0);
    LOD_PIXELS.iter().take_while(|&&p| pixels < p).count()
}

/// `angle` radians as a look's i8, where 127 is `full`.
fn look_angle(angle: f32, full: f32) -> i8 {
    (angle / full * 127.0).round().clamp(-127.0, 127.0) as i8
}

/// How much of the detail kept for soldiers near the camera (head turns and
/// flinches) shows on one `height` tiles tall at `depth`: none below the
/// nearest level of detail, fading in over [`HEAD_FADE_PIXELS`] above it.
fn head_detail(height: f32, depth: f32, pixel_scale: f32) -> f32 {
    let pixels = height * pixel_scale / depth.max(1.0);
    ((pixels - LOD_PIXELS[0]) / HEAD_FADE_PIXELS).clamp(0.0, 1.0)
}

/// What choosing a level of detail needs from a frame's camera.
#[derive(Clone, Copy)]
struct LodView {
    eye: glam::Vec3,
    forward: glam::Vec3,
    pixel_scale: f32,
    /// What the frame draws, for skipping soldiers nobody would see.
    cull: gc_render::Cull,
    /// Every soldier at this level, from `--lod`.
    force: Option<usize>,
}

impl LodView {
    /// The level for a model `height` tiles tall standing at `pos`: it
    /// follows how tall the model shows on screen.
    fn lod(&self, height: f32, pos: glam::Vec3) -> usize {
        self.force.unwrap_or_else(|| {
            lod_for(height, (pos - self.eye).dot(self.forward), self.pixel_scale)
        })
    }

    /// How much near-camera detail shows on a model `height` tiles tall at
    /// `pos` (see [`head_detail`]); all of it at a forced level (the
    /// caller draws it at the nearest only).
    fn detail(&self, height: f32, pos: glam::Vec3) -> f32 {
        match self.force {
            Some(_) => 1.0,
            None => head_detail(height, (pos - self.eye).dot(self.forward), self.pixel_scale),
        }
    }

    /// The level for a model `height` tiles tall standing at `pos`, or none
    /// when it is worth no drawing: it is neither on screen nor casting a
    /// shadow where one is drawn.
    fn place(&self, height: f32, pos: glam::Vec3) -> Option<usize> {
        self.cull
            .sees(pos + glam::Vec3::Y * (height * 0.5), height)
            .then(|| self.lod(height, pos))
    }
}

impl Art {
    /// Empty the buckets for a frame seen from `eye` looking along `forward`,
    /// where `pixel_scale` is the camera's (`RtsCamera::pixel_scale` for a
    /// viewport of the window's height).
    fn begin(&mut self, eye: Vec3, forward: Vec3, pixel_scale: f32, cull: gc_render::Cull) {
        for b in &mut self.buckets {
            b.clear();
        }
        self.view = LodView {
            eye,
            forward,
            pixel_scale,
            cull,
            force: self.view.force,
        };
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
                model: (k / LODS) as u16,
                lod: (k % LODS) as u8,
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
            building_tool: false,
            building_turn: 0,
            building: 0,
            tuning: false,
            row: 0,
            credits: false,
            message: None,
            dirty: true,
        }
    }
}

/// What a formation does, in a line for the panel.
fn formation_hint(f: Formation) -> String {
    match f {
        Formation::Line => "each regiment's own spacing".into(),
        Formation::Tight => format!(
            "closer ranks hold better (+{TIGHT_DEFENCE} defence) but arrows find more marks; big swings are cramped"
        ),
        Formation::Loose => format!(
            "fewer arrows hit, but gaps cost {LOOSE_DEFENCE} defence (not for big swings); spears and shields lose their support"
        ),
        Formation::ShieldWall => {
            "arrows from the front mostly glance off; half pace, no charging, no help on the flanks"
                .into()
        }
    }
}

/// How a regiment's soldiers fight, for choosing what their blows do on
/// screen: the simulation's numbers for its unit and state.
fn fighter_of(stats: &UnitStats, r: &gc_sim::RegimentView) -> Fighter {
    Fighter {
        attack: i32::from(stats.melee_attack),
        defence: stats.melee_defence_in(r.formation, true, r.still),
        flank_defence: stats.melee_defence_in(r.formation, false, r.still),
        sharpness: (gc_sim::combat::heart_percent(r.morale)
            * stats.weapon_room_percent(r.formation)
            / 100) as u32,
        shield: stats.shield,
        mass: stats.mass,
        heavy_blade: stats.swing_room,
        reach: stats.reach,
    }
}

/// One of the two strikes, so neighbours swing out of step.
fn swing(first: bool) -> Action {
    if first {
        Action::Attack
    } else {
        Action::Attack2
    }
}

/// Regiment `by` as the siege draws its blow on a building; `seen` says
/// whether the viewer can see it, `ground` the height of the ground at a
/// point.
fn attacker_of(
    world: &World,
    units: &UnitTable,
    by: RegimentHandle,
    seen: bool,
    ground: impl Fn(Vec2) -> f32,
) -> Option<siege::Attacker> {
    let r = world.regiments.get(by)?;
    let stats = units.stats.get(usize::from(r.unit))?;
    let breaker = units.breakers.get(usize::from(r.unit)).copied();
    let (frontage, _) = stats.block_size_in(r.formation, r.files, r.models);
    let at = from_fx_vec(r.position);
    Some(siege::Attacker {
        handle: by,
        at,
        ground: ground(at),
        kind: siege::Kind::of(stats.range.raw() > 0, breaker.unwrap_or_default()),
        frontage: fx_to_f32(frontage),
        seen,
    })
}

/// What a soldier is doing out of a duel, from his puppet and his regiment.
fn soldier_action(s: &Soldier, block: &Block, shooting: bool) -> Action {
    if block.routing {
        Action::Run
    } else if s.shouting.is_some() {
        Action::Cheer
    } else if s.striking && s.phase > 0.6 {
        // Behind the fighting line, some wait their turn.
        Action::Idle
    } else if s.striking {
        swing(s.phase < 0.5)
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
    /// Left-click places buildings of `building` instead of selecting (K);
    /// [ and ] then choose the building.
    building_tool: bool,
    /// Quarter turns clockwise the building being placed is turned (kept
    /// from one placing to the next).
    building_turn: u8,
    building: BuildingId,
    /// The stats panel is open (T), with this row chosen.
    tuning: bool,
    row: usize,
    /// The credits are showing (C), in place of the overlay text.
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

/// Which regiment types the match plays with, and so which game it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Play {
    /// The Field Battle types of `data/units.ron`: the sandbox's default.
    Field,
    /// A lobby Field Battle, on the Conquest roster table so every
    /// faction's full roster can take the field.
    FieldBattle,
    /// The Conquest types, with its economy, workers, HUD and build menu.
    Conquest,
    /// The sandbox on the Conquest roster table, so troops of every faction
    /// can be placed (docs/17-unit-rosters.md). It is still the sandbox: no
    /// Conquest economy, workers, HUD or build menu.
    SandboxRoster,
}

impl Play {
    /// Whether the Conquest roster table is the one in play.
    fn roster_table(self) -> bool {
        self != Play::Field
    }
}

/// What kind of match to start from the menu. (The scripted four-army
/// drill runs only when the game starts straight into a match.)
enum MatchKind {
    /// An empty field, nobody moving unless ordered; on the Conquest roster
    /// table (troops of every faction) when `roster`.
    Sandbox { roster: bool },
    /// A Field Battle from the lobby.
    Battle(MatchStart),
    /// Watching a saved replay.
    Replay(Box<gc_replay::Replay>),
}

impl MatchKind {
    fn is_sandbox(&self) -> bool {
        matches!(self, MatchKind::Sandbox { .. })
    }
}

/// The battle being recorded, and where it is saved.
struct Recording {
    recorder: gc_replay::Recorder,
    path: std::path::PathBuf,
    /// The tick it was last saved at, if it has been.
    saved_at: Option<u32>,
}

/// A replay being watched.
struct Watching {
    replay: gc_replay::Replay,
    title: String,
    /// The unit stats to go back to afterwards.
    stats_before: Vec<UnitStats>,
    /// The first tick whose state differs from the recording's, if any:
    /// the replay was made with other rules (docs/10-replays.md).
    diverged_at: Option<u32>,
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

/// Radians per second the view turns while `,` or `.` is held.
const KEY_TURN_RATE: f32 = 1.0;

#[derive(Default)]
struct Input {
    cursor: Option<PhysicalPosition<f64>>,
    focused: bool,
    alt: bool,
    shift: bool,
    /// Ctrl, or Cmd on a Mac.
    command: bool,
    arrows: [bool; 4], // up, down, left, right
    /// `,` turns the view left, `.` right.
    turn: [bool; 2],
    /// Where a left-drag (box select) started.
    left_down: Option<PhysicalPosition<f64>>,
    /// The ground where a left-drag after the Clear button started.
    clear_from: Option<Vec2>,
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
        // The game opens on the map a two-player battle from a new lobby
        // would be on, or on one opened with `--map`.
        let first_key = battle_map(
            &gc_ui::lobby::LobbySettings {
                map_seed: opts.seed,
                ..Default::default()
            },
            2,
        );
        let heightmap = tagged_alloc::scope(Tag::Map, || match &opts.map {
            Some(def) => Arc::new(Map::from_def(def)),
            None => make_map(first_key),
        });
        tagged_alloc::scope(Tag::Render, || {
            set_terrain(&mut renderer, &heightmap);
        });
        self.memory_timeline.mark("map generated + uploaded");

        let game_data = data::load();
        let detail = models::load_detail()
            .map_err(|e| logln!("grubcraft: surface detail off: {e}"))
            .ok();
        let models = match models::load()
            .and_then(|m| renderer.set_models(&m, detail.as_ref()).map(|()| m))
        {
            Ok(m) => Some(m),
            Err(e) => {
                logln!("grubcraft: soldier models off: {e}");
                None
            }
        };
        let units = UnitTable::of(game_data.units.iter(), models.as_ref());
        let mut other_units = UnitTable::of(
            game_data.conquest_units.iter().map(|u| &u.stats),
            models.as_ref(),
        );
        other_units.roles = game_data
            .conquest_units
            .iter()
            .map(|u| army_shape::role_of(u.class()))
            .collect();
        other_units.breakers = game_data
            .conquest_units
            .iter()
            .map(|u| conquest::unit_of(u).breaker)
            .collect();
        // A Field Battle army buys roster troops at points from their cost.
        other_units.points = game_data
            .conquest_units
            .iter()
            .map(roster::field_points)
            .collect();
        let art = models.map(|models| Art {
            clips: models::Clips::new(&models),
            looks: buildings::Looks::new(&buildings::Table::new(&game_data), &models),
            heights: models.models.iter().map(|m| m.height).collect(),
            deer: models.by_name("Deer"),
            dove: models.by_name("Dove"),
            crow: models.by_name("Crow"),
            folk: ambient::villagers::Kind::ALL
                .map(|k| models.by_name(k.model()))
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .and_then(|v| v.try_into().ok()),
            cart: models.by_name("Oxcart"),
            insects: ambient::insects::Kind::ALL
                .map(|k| models.by_name(k.model()))
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .and_then(|v| v.try_into().ok()),
            view: LodView {
                eye: glam::Vec3::ZERO,
                forward: glam::Vec3::NEG_Z,
                pixel_scale: 0.0,
                cull: gc_render::Cull::EVERYTHING,
                force: opts.lod,
            },
            buckets: vec![Vec::new(); models.models.len() * LODS],
            soldiers: Vec::with_capacity(gc_render::MAX_SOLDIER_INSTANCES),
            batches: Vec::with_capacity(models.models.len() * LODS),
        });
        self.memory_timeline.mark("models uploaded");
        // gc_data and the lobby list the factions in the same order. A
        // Field Battle plays on the roster table, every faction's full
        // roster; the Army screen hides the types that cost 0 points.
        let roster: Vec<gc_ui::lobby::RosterUnit> = game_data
            .conquest_units
            .iter()
            .zip(&other_units.names)
            .zip(&other_units.points)
            .map(|((u, name), &points)| gc_ui::lobby::RosterUnit {
                name: name.clone(),
                faction: u.faction(),
                points,
            })
            .collect();
        let capacities = Capacities::default();
        let nav = tagged_alloc::scope(Tag::Map, || heightmap.sim_nav_costs());
        let blocked_marks = blocked_marks(&nav, heightmap.tiles);
        let scenery_models = scenery::load(&mut renderer).unwrap_or_else(|e| {
            logln!("grubcraft: scenery off: {e}");
            scenery::SceneryModels::default()
        });
        let scenery = tagged_alloc::scope(Tag::Render, || {
            // The showroom stands on bare ground, with no trees in the way.
            if opts.showroom.is_some() {
                scenery::Scenery::empty(&scenery_models)
            } else {
                scenery::Scenery::new(&heightmap, opts.seed, &scenery_models)
            }
        });
        let building_table = buildings::Table::new(&game_data);
        let (world, prev_positions, pending) = tagged_alloc::scope(Tag::Sim, || {
            let mut world = World::new(&MatchSetup {
                seed: opts.seed,
                capacities,
                units: &units.stats,
                map_tiles: heightmap.tiles,
                nav: &nav,
                teams: &[],
                ground: heightmap.ground(),
                champions: false,
                sky: gc_sim::Sky::STILL,
                generals: true,
            });
            // The sandbox is where buildings can be tried so far.
            if opts.sandbox {
                building_table.conquer(&mut world);
            }
            (
                world,
                vec![None; capacities.regiments as usize],
                Vec::with_capacity(1024),
            )
        });
        let drill = (!opts.sandbox && opts.showroom.is_none())
            .then(|| make_drill(opts.seed, heightmap.tiles));
        let minds = tagged_alloc::scope(Tag::Ai, || {
            Minds::new(terrain_view::ai_terrain(&heightmap, &nav)).live()
        });

        let start = heightmap.start_positions().first().copied();
        let start = start.unwrap_or(Vec2::splat(heightmap.tiles as f32 * 0.25));
        let mut camera = RtsCamera::new(heightmap.tiles as f32, start + Vec2::new(10.0, 20.0));
        camera.focus_height = heightmap.height_at(camera.focus.x, camera.focus.y);
        camera.aspect = renderer.aspect_ratio();
        let showroom = art.as_ref().and_then(|a| {
            let models = opts.showroom.clone()?;
            Some(showroom::Showroom::new(
                models,
                &a.heights,
                Vec2::splat(heightmap.tiles as f32 / 2.0),
                opts.showroom_clip,
            ))
        });

        self.memory_timeline.mark("simulation world");
        let info = renderer.adapter_info();
        gc_platform::diag::set_context(
            "gpu",
            format!("{} ({:?}, {:?})", info.name, info.backend, info.driver),
        );
        gc_platform::diag::set_context("data hash", format!("{:016x}", game_data.short_hash()));
        logln!(
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
        logln!(
            "grubcraft: game data {:016x}, {} unit types ({})",
            game_data.short_hash(),
            game_data.units.len(),
            game_data
                .units
                .iter()
                .map(gc_data::UnitRecord::name)
                .collect::<Vec<_>>()
                .join(", ")
        );

        assert!(!opts.crash_test, "--crash-test: a deliberate panic");

        let mut hud_state = hud::HudState::default();
        hud_state.set_terrain(&heightmap);
        // An empty-field sandbox starts with the Place panel open.
        hud_state.placer = opts.sandbox;
        let settings_path = gc_ui::settings::settings_path();
        let settings = settings_path
            .as_deref()
            .map_or_else(Settings::default, Settings::load);
        let volumes = (settings.volume_master, settings.volume_music);
        let lan = Lan::new(gc_net::Identity {
            version: online::version(),
            data_hash: game_data.short_hash(),
            map_hash: battle_map_hash,
        });
        let mut menu = Menu::new(lan, settings, settings_path, &online::version());
        menu.set_roster(roster);
        menu.set_host_map_seed(opts.seed);
        let show_menu = opts.menu.unwrap_or(
            opts.frames.is_none()
                && opts.battle.is_none()
                && opts.conquest.is_none()
                && opts.watch.is_none(),
        );
        let sounds = if opts.audio {
            tagged_alloc::scope(Tag::Audio, || {
                sounds::Sounds::start(capacities.regiments as usize, volumes.0, volumes.1)
            })
        } else {
            None
        };

        let now = Instant::now();
        self.running = Some(Running {
            window,
            renderer,
            atmosphere: atmosphere::Atmosphere::new(opts.look, opts.air).with_weather(opts.weather),
            cinema: cinema::Cinema::default(),
            was_deploying: false,
            director: director::Director::default(),
            start_distance: opts.distance,
            camera,
            ride: None,
            heightmap,
            map_key: opts.map.is_none().then_some(first_key),
            world,
            drill,
            minds,
            parties: sandbox_parties(),
            battle: None,
            matches: 0,
            pending,
            human_orders: Vec::with_capacity(64),
            prev_positions,
            alpha: 1.0,
            instances: Vec::with_capacity(gc_render::MAX_BOX_INSTANCES),
            puppets: Puppets::new(capacities.regiments as usize, MAX_CORPSES),
            effects: effects::Field::new(opts.fx),
            lights: lights::Lights::default(),
            ambient: ambient::Ambient::new(opts.fell_demo, opts.hamlet_demo),
            arrows: field::Arrows::new(field::MAX_STUCK),
            siege: siege::Siege::new(capacities.regiments as usize),
            aftermath: aftermath::Aftermath::default(),
            siege_scene: None,
            art,
            commander: false,
            shooting: vec![false; capacities.regiments as usize],
            units,
            other_units,
            play: Play::Field,
            buildings: building_table,
            // The showroom hides the panel (H shows it).
            sandbox: Sandbox {
                ..Sandbox::new(opts.sandbox)
            },
            selected: Vec::with_capacity(64),
            selected_structure: None,
            idle_from: None,
            formation_pick: None,
            shown_formation: None,
            army_shapes: army_shape::shapes_path()
                .map_or_else(army_shape::ArmyShapes::default, |p| {
                    army_shape::ArmyShapes::load(&p)
                }),
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
            scenery,
            scenery_models,
            showroom,
            viewer: None,
            viewer_scroll: 0.0,
            guide: None,
            guide_scroll: 0.0,
            turn_scroll: 0.0,
            tip_book: guide_data::tip_book(),
            reveal_all: false,
            fog: Default::default(),
            land: Default::default(),
            scars: Default::default(),
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
            hud: hud_state,
            menu_open: show_menu,
            match_started: !show_menu,
            online: None,
            recording: None,
            watching: None,
            data_hash: game_data.short_hash(),
            quit: false,
            sounds,
            replay_list: None,
            map_previews: map_preview::MapPreviews::new(),
            data: game_data,
        });
        if let Some(r) = self.running.as_mut() {
            r.fresh_puppets();
            r.menu.set_in_match(r.match_started);
            if let Some(unit) = &opts.viewer {
                r.open_viewer(unit);
            }
            if opts.host || opts.join.is_some() {
                let me = r.menu.profile();
                let result = match &opts.join {
                    Some(addr) => r.menu.net().join(&me, addr, &opts.password),
                    None => r.menu.net().host(
                        &me,
                        gc_ui::lobby::LobbySettings {
                            map_seed: opts.seed,
                            ..Default::default()
                        },
                        &opts.password,
                    ),
                };
                match result {
                    Ok(()) => r.menu.show(Screen::Lobby),
                    Err(e) => logln!("grubcraft: {e}"),
                }
            }
            if let Some(level) = opts.battle {
                let mut setup = battle::quick_battle(level, r.menu.settings(), opts.seed);
                opts.set_sky(&mut setup);
                r.start_match(MatchKind::Battle(setup));
            }
            if let Some(path) = &opts.watch {
                match gc_replay::Replay::load(path) {
                    Ok(replay) => {
                        match replays::unplayable(&replay.info(), &r.unit_types(), r.capacities) {
                            None => r.start_match(MatchKind::Replay(Box::new(replay))),
                            Some(why) => {
                                logln!("grubcraft: can't watch {}: {why}", path.display());
                                r.menu.tell(why);
                                r.open_menu();
                            }
                        }
                    }
                    Err(e) => {
                        logln!("grubcraft: can't open {}: {e}", path.display());
                        r.menu.tell(format!("Can't open that replay: {e}"));
                        r.open_menu();
                    }
                }
            }
            if let Some(choice) = opts.conquest {
                let mut setup = conquest::quick_conquest(choice, r.menu.settings(), opts.seed);
                opts.set_sky(&mut setup);
                r.start_match(MatchKind::Battle(setup));
            }
            if opts.siege {
                r.start_match(MatchKind::Sandbox { roster: true });
                r.siege_scene = Some(0);
            }
        }
        Ok(())
    }
}

impl ApplicationHandler for App {
    /// However the game ends, the battle being recorded is saved.
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(r) = self.running.as_mut() {
            r.save_recording();
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        if let Err(e) = self.start(event_loop) {
            logln!("grubcraft: failed to start: {e}");
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(r) = self.running.as_mut() else {
            return;
        };
        if r.guide.is_some() && r.guide_event(&event) {
            return;
        }
        if r.menu_open {
            if r.viewer_event(&event) {
                return;
            }
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
            WindowEvent::Focused(f) => {
                r.input.focused = f;
                r.input.turn = [false; 2];
                // A button let go outside the window never reports it.
                r.hud.reset();
            }
            WindowEvent::ModifiersChanged(m) => {
                r.input.alt = m.state().alt_key();
                r.input.shift = m.state().shift_key();
                r.input.command = m.state().control_key() || m.state().super_key();
            }
            WindowEvent::CursorLeft { .. } => {
                r.input.cursor = None;
                r.hud.reset();
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let (Some(prev), Some(_)) = (r.input.cursor, r.input.middle_down) {
                    let size = r.window.inner_size();
                    let dx = (position.x - prev.x) as f32 / size.width as f32;
                    let dy = (position.y - prev.y) as f32 / size.height as f32;
                    if r.input.alt {
                        r.camera_mut().rotate(dx * 3.0);
                    } else {
                        r.camera_mut().pan(-dx, dy);
                    }
                    r.input.middle_dragged = true;
                }
                r.input.cursor = Some(position);
            }
            // Over the bar the wheel is not the camera's.
            WindowEvent::MouseWheel { .. } if r.hud_wants_mouse() => {}
            // While a building is being placed a mouse wheel turns it
            // instead; a trackpad still pans.
            WindowEvent::MouseWheel {
                delta: delta @ MouseScrollDelta::LineDelta(..),
                ..
            } if r.laying().is_some() => {
                let steps = wheel_rows(&mut r.turn_scroll, &delta);
                r.turn_building(-steps);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                match delta {
                    MouseScrollDelta::LineDelta(_, y) => r.camera_mut().zoom(0.88f32.powf(y)),
                    // Trackpads send pixel deltas: two-finger scroll pans, as in
                    // every Mac map app.
                    MouseScrollDelta::PixelDelta(p) => {
                        let size = r.window.inner_size();
                        r.camera_mut().pan(
                            -p.x as f32 / size.width as f32,
                            p.y as f32 / size.height as f32,
                        );
                    }
                }
            }
            WindowEvent::PinchGesture { delta, .. } => {
                r.camera_mut().zoom(1.0 - delta as f32);
            }
            WindowEvent::RotationGesture { delta, .. } => {
                r.camera_mut().rotate(delta.to_radians());
            }
            WindowEvent::MouseInput { state, button, .. } => r.mouse_button(state, button),
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    // The light tuning panel keeps its keys while it is open,
                    // ahead of the sandbox's tools and their refusals.
                    if down
                        && r.atmosphere.panel_open()
                        && r.light_key(code, event.repeat, r.input.shift)
                    {
                        return;
                    }
                    if r.sandbox.tuning && down && r.tune_key(code) {
                        return;
                    }
                    let reload_key = code == KeyCode::KeyR && r.input.shift;
                    // R turns the building being placed (Shift+R the other
                    // way), ahead of the grid's R and the dev reload.
                    if code == KeyCode::KeyR
                        && down
                        && !event.repeat
                        && !r.input.command
                        && !r.input.alt
                        && r.laying().is_some()
                    {
                        r.turn_building(if r.input.shift { -1 } else { 1 });
                        return;
                    }
                    let grid_key =
                        down && !event.repeat && !r.input.shift && !r.input.command && !r.input.alt;
                    // The sandbox's clock and side keys would change this
                    // machine's game and nobody else's; its editing tools
                    // also change what a replay plays.
                    let clock_or_side = matches!(
                        code,
                        KeyCode::KeyP
                            | KeyCode::Pause
                            | KeyCode::Minus
                            | KeyCode::NumpadSubtract
                            | KeyCode::Equal
                            | KeyCode::NumpadAdd
                            | KeyCode::Tab
                    );
                    // In Conquest B opens the workers' build menu instead.
                    let edit_tool = matches!(
                        code,
                        KeyCode::KeyI
                            | KeyCode::KeyK
                            | KeyCode::Delete
                            | KeyCode::Backspace
                            | KeyCode::KeyT
                    ) || (code == KeyCode::KeyB && !r.conquest_match())
                        || reload_key;
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
                    let off = if r.online.is_some() && (clock_or_side || edit_tool) {
                        Some("That sandbox tool is off in a network battle.")
                    } else if r.watching.is_some() && (edit_tool || code == KeyCode::KeyF) {
                        Some("That is off while you watch a replay.")
                    } else if r.conquest_match() && edit_tool {
                        Some("That sandbox tool is off in Conquest.")
                    } else {
                        None
                    };
                    if let Some(why) = off {
                        if down && !event.repeat {
                            r.say(why.into());
                        }
                        return;
                    }
                    // The wall tool owns B (blueprints), O (the ring) and Enter
                    // while it is armed, ahead of the build menu's B and the
                    // army shape's O.
                    if down
                        && !event.repeat
                        && !r.input.shift
                        && !r.input.command
                        && !r.input.alt
                        && r.wall_key(code)
                    {
                        return;
                    }
                    // A building's card or the build menu owns the grid's keys;
                    // what it leaves goes to the battle grid below.
                    if grid_key
                        && let Some(slot) = GRID_CODES.iter().position(|&c| c == code)
                        && r.card_press(slot)
                    {
                        return;
                    }
                    if grid_key && let Some(slot) = faction_powers::key_slot(code) {
                        r.power_press(slot);
                        return;
                    }
                    match code {
                        KeyCode::ArrowUp => r.input.arrows[0] = down,
                        KeyCode::ArrowDown => r.input.arrows[1] = down,
                        KeyCode::ArrowLeft => r.input.arrows[2] = down,
                        KeyCode::ArrowRight => r.input.arrows[3] = down,
                        KeyCode::Comma => r.input.turn[0] = down && !r.input.command,
                        KeyCode::Period => r.input.turn[1] = down && !r.input.command,
                        KeyCode::Home if down => r.camera_mut().reset_rotation(),
                        // Ride with the banner. Chords with Ctrl, Cmd or Alt
                        // are the system's.
                        KeyCode::KeyY if grid_key => r.toggle_ride(),
                        KeyCode::F8 if down && !event.repeat => r.toggle_director(),
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
                        KeyCode::KeyB if down && !event.repeat && r.conquest_match() => {
                            r.set_build_menu(!r.hud.build_menu);
                        }
                        KeyCode::KeyB if down && !event.repeat => {
                            r.sandbox.placing = !r.sandbox.placing;
                            // The Place panel follows: open to pick a troop.
                            r.hud.placer = r.sandbox.placing && r.can_place();
                            r.sandbox.building_tool = false;
                            r.hud.armed = None;
                            r.sandbox.dirty = true;
                        }
                        KeyCode::KeyK if down && !event.repeat => r.toggle_building_tool(),
                        KeyCode::BracketLeft if down => r.choose_type(-1),
                        KeyCode::BracketRight if down => r.choose_type(1),
                        KeyCode::Delete | KeyCode::Backspace if down && !event.repeat => {
                            r.delete_key();
                        }
                        KeyCode::KeyT if down && !event.repeat => {
                            r.sandbox.tuning = !r.sandbox.tuning;
                            r.input.arrows = [false; 4];
                            r.sandbox.dirty = true;
                        }
                        // Shift+R: the dev tool; plain R is the grid's Formation.
                        KeyCode::KeyR if down && !event.repeat && reload_key => r.reload_data(),
                        // The grid's keys leave Cmd/Ctrl/Alt chords to the system.
                        KeyCode::KeyQ if grid_key => r.battle_press(hud::SLOT_MOVE),
                        KeyCode::KeyW if grid_key => r.battle_press(hud::SLOT_ATTACK),
                        // The hero's fourth ability owns F while a hero is shown.
                        KeyCode::KeyF if grid_key && r.hud.hero_shown() => {}
                        // Otherwise F stays as R's twin until the Ultimate slot has an ability.
                        KeyCode::KeyR | KeyCode::KeyF if grid_key => {
                            r.battle_press(hud::SLOT_FORMATION);
                        }
                        // O picks the army's shape for group orders, Shift+O saves
                        // the selection's own.
                        KeyCode::KeyO
                            if down && !event.repeat && !r.input.command && !r.input.alt =>
                        {
                            // Shift is part of the key here, so not `grid_key`.
                            if r.input.shift {
                                r.save_army_shape();
                            } else {
                                r.cycle_army_shape();
                            }
                        }
                        KeyCode::KeyZ if grid_key => r.battle_press(hud::SLOT_BRACE),
                        KeyCode::KeyX if grid_key => r.battle_press(hud::SLOT_TORCHES),
                        KeyCode::KeyE if grid_key => r.battle_press(hud::SLOT_BURN),
                        KeyCode::KeyV if grid_key => r.battle_press(hud::SLOT_FALL_BACK),
                        KeyCode::Space if grid_key => r.jump_to_alert(),
                        // F1 finds an idle worker while there is one to find,
                        // and otherwise opens the Marshal's Scroll (like H).
                        KeyCode::F1 if down && !event.repeat => r.f1_key(),
                        KeyCode::F9 if down && !event.repeat => {
                            r.atmosphere.next_preset();
                            r.sandbox.dirty = true;
                        }
                        KeyCode::F12 if down && !event.repeat => {
                            r.atmosphere.next_weather();
                            r.sandbox.dirty = true;
                        }
                        KeyCode::F10 if down && !event.repeat => {
                            r.atmosphere.toggle_panel();
                            // The panel takes the arrow keys while it is open.
                            r.input.arrows = [false; 4];
                            r.sandbox.dirty = true;
                        }
                        KeyCode::KeyC if down && !event.repeat => {
                            r.sandbox.credits = !r.sandbox.credits;
                            r.sandbox.dirty = true;
                        }
                        KeyCode::KeyH
                            if down && !event.repeat && r.input.shift && r.online.is_none() =>
                        {
                            r.hud.toggle_hero_preview();
                        }
                        KeyCode::KeyH if down && !event.repeat => r.toggle_help(),
                        KeyCode::KeyU if down && !event.repeat => {
                            // A selected wall is raised; otherwise U is the guide.
                            if !r.raise_key() {
                                r.open_guide();
                            }
                        }
                        KeyCode::KeyG if down && !event.repeat => {
                            r.show_blocked = !r.show_blocked;
                        }
                        // Seeing through the woods is for the sandbox only
                        // (Shift+V: V is Fall back).
                        KeyCode::KeyV
                            if down
                                && !event.repeat
                                && r.input.shift
                                && (r.battle.is_none() || r.watching.is_some())
                                && r.online.is_none() =>
                        {
                            r.reveal_all = !r.reveal_all;
                            r.sandbox.dirty = true;
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
                        KeyCode::Escape if down && r.hud.armed.is_some() => {
                            r.hud.armed = None;
                        }
                        KeyCode::Escape if down && r.hud.help_open() => r.toggle_help(),
                        KeyCode::Escape if down && r.hud.build_menu => r.set_build_menu(false),
                        // Esc first gets you out of a ride.
                        KeyCode::Escape if down && r.ride.is_some() => r.leave_ride(),
                        KeyCode::Escape
                            if down
                                && (r.sandbox.placing
                                    || r.hud.placer
                                    || r.sandbox.building_tool
                                    || r.sandbox.tuning
                                    || r.sandbox.credits) =>
                        {
                            r.sandbox.placing = false;
                            r.hud.placer = false;
                            r.sandbox.building_tool = false;
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(r) = &mut self.running {
            // Here rather than in the redraw, so a minimised window notices
            // a lost GPU or sound card too.
            r.check_devices();
            if r.quit {
                event_loop.exit();
            }
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
                    self.camera_mut().reset_rotation();
                }
                self.input.middle_down = None;
            }
            // The bar at the bottom takes its own clicks.
            // A click that starts on the bar stays the bar's until it ends.
            (MouseButton::Left, ElementState::Pressed) if self.hud_wants_mouse() => {
                self.hud.press_left();
            }
            (MouseButton::Left, ElementState::Released) if self.hud.left_captured() => {
                self.hud.release_left();
            }
            (MouseButton::Right, ElementState::Pressed) if self.hud_wants_mouse() => {
                self.hud.press_right();
            }
            (MouseButton::Right, ElementState::Released) if self.hud.right_captured() => {
                self.hud.release_right();
            }
            (MouseButton::Left, ElementState::Pressed) => {
                self.input.left_down = self.input.cursor;
                // Choosing ground to clear: the drag starts on the ground.
                self.input.clear_from = (self.hud.armed == Some(hud::Armed::Clear))
                    .then(|| self.ground_under_cursor())
                    .flatten();
            }
            // Q or W was pressed: this click says where (or at whom).
            (MouseButton::Left, ElementState::Released)
                if self.hud.armed.is_some() && !self.hud_wants_mouse() =>
            {
                self.finish_armed_order();
                self.input.left_down = None;
            }
            (MouseButton::Left, ElementState::Released) if self.sandbox.placing => {
                self.place_regiment();
                self.input.left_down = None;
            }
            (MouseButton::Left, ElementState::Released) if self.sandbox.building_tool => {
                self.place_building();
                self.input.left_down = None;
            }
            (MouseButton::Left, ElementState::Released) => {
                if !self.input.shift {
                    self.selected.clear();
                }
                self.selected_structure = None;
                if let Some(rect) = self.selection_box() {
                    self.regiments_in_box(rect);
                    for i in 0..self.boxed.len() {
                        let h = self.boxed[i];
                        if !self.selected.contains(&h) {
                            self.selected.push(h);
                        }
                    }
                } else {
                    // A click on one of your buildings picks it, unless one
                    // of your regiments stands right there (workers crowd
                    // round their hall); elsewhere the nearest regiment.
                    // Riding, a soldier is picked by the ray through its
                    // body, since the ground behind it is far away.
                    let side = self.sandbox.side;
                    let building = self
                        .selected
                        .is_empty()
                        .then(|| self.building_under_cursor(|o| o == side))
                        .flatten();
                    let reach = if building.is_some() { 1.5 } else { 6.0 };
                    match self.own_regiment_at_cursor(reach) {
                        Some(h) => self.toggle_selected(h),
                        None => self.selected_structure = building,
                    }
                }
                if !self.selected.is_empty() || self.selected_structure.is_some() {
                    self.ui_sound(sounds::UiSound::Select);
                }
                self.input.left_down = None;
            }
            (MouseButton::Right, ElementState::Pressed) => {
                self.input.right_down = self.input.cursor.zip(self.ground_under_cursor());
            }
            (MouseButton::Right, ElementState::Released) => {
                // A right-click puts down a power being aimed, and orders
                // nothing; any other armed order it takes the place of.
                if matches!(
                    self.hud.armed,
                    Some(hud::Armed::Power(_) | hud::Armed::Wall(..))
                ) {
                    self.hud.armed = None;
                    self.input.right_down = None;
                    return;
                }
                self.hud.armed = None;
                let drag = self.drag_order();
                let plain = drag.is_none().then(|| self.ground_under_cursor()).flatten();
                // Riding, the ground point lies far behind a soldier clicked
                // on, so a body the ray passes through comes first.
                let body = drag
                    .is_none()
                    .then(|| self.body_under_cursor(false))
                    .flatten();
                // A tower picks its target; it has nothing to rally.
                if drag.is_none() && self.tower_right_click(body, plain) {
                    self.input.right_down = None;
                    return;
                }
                // Your own wall, with regiments that can man it selected
                // (an enemy at the click is attacked instead).
                if drag.is_none()
                    && body.is_none()
                    && plain.and_then(|p| self.enemy_at(p)).is_none()
                    && self.man_right_click(plain)
                {
                    self.input.right_down = None;
                    return;
                }
                // An enemy's raised wall: the regiments that can climb it do
                // (Ctrl+right-click besieges it instead).
                if drag.is_none()
                    && body.is_none()
                    && plain.and_then(|p| self.enemy_at(p)).is_none()
                    && self.climb_right_click()
                {
                    self.input.right_down = None;
                    return;
                }
                if body.is_none() && plain.is_some_and(|p| self.conquest_right_click(p)) {
                    self.input.right_down = None;
                    return;
                }
                let target = body.or_else(|| plain.and_then(|p| self.enemy_at(p)));
                if let Some(target) = target {
                    self.issue_attack(target);
                } else if let Some(building) = drag
                    .is_none()
                    .then(|| self.enemy_building_under_cursor())
                    .flatten()
                {
                    self.issue_siege(building);
                } else if let Some(order) = drag.or_else(|| plain.map(hud::move_order)) {
                    self.issue_group_order(order);
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

    /// The ray through the mouse, from whichever camera is in use.
    fn cursor_ray(&self) -> Option<(Vec3, Vec3)> {
        let cursor = self.input.cursor?;
        let size = self.window.inner_size();
        let ndc = Vec2::new(
            (cursor.x as f32 / size.width as f32) * 2.0 - 1.0,
            1.0 - (cursor.y as f32 / size.height as f32) * 2.0,
        );
        Some(self.active_camera().ray(ndc))
    }

    /// The camera in use: the ride camera while riding, else the bird's-eye
    /// one.
    fn active_camera(&self) -> ActiveCamera<'_> {
        match &self.ride {
            Some(ride) => ActiveCamera::Ride(&ride.camera, self.camera.aspect),
            None => ActiveCamera::Rts(&self.camera),
        }
    }

    /// The bird's-eye camera, for moving the view: any move of it hands the
    /// view back from a ride first.
    fn camera_mut(&mut self) -> &mut RtsCamera {
        self.leave_ride();
        &mut self.camera
    }

    /// The map point under the mouse, found by marching the camera ray over
    /// the CPU copy of the heightmap.
    fn ground_under_cursor(&self) -> Option<Vec2> {
        let (origin, dir) = self.cursor_ray()?;
        let hit = origin + dir * self.ground_along(origin, dir)?;
        let n = self.heightmap.tiles as f32;
        let inside = (0.0..=n).contains(&hit.x) && (0.0..=n).contains(&hit.z);
        inside.then(|| Vec2::new(hit.x, hit.z))
    }

    /// How far along the ray from `origin` (along `dir`) it meets the
    /// ground or the water, found by marching over the CPU copy of the
    /// heightmap; None if it never comes down within 2,000 tiles.
    fn ground_along(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        let mut t = 0.0;
        let step = 0.5;
        let mut prev_t = 0.0;
        for _ in 0..4000 {
            let p = origin + dir * t;
            let ground = self
                .heightmap
                .height_at(p.x, p.z)
                .max(self.heightmap.water_level());
            if p.y <= ground {
                // Refine between the last point above ground and this one.
                let (mut lo, mut hi) = (prev_t, t);
                for _ in 0..16 {
                    let mid = (lo + hi) * 0.5;
                    let q = origin + dir * mid;
                    let g = self
                        .heightmap
                        .height_at(q.x, q.z)
                        .max(self.heightmap.water_level());
                    if q.y <= g { hi = mid } else { lo = mid }
                }
                return Some(hi);
            }
            prev_t = t;
            t += step;
        }
        None
    }

    /// Send the selection to carry out `order`.
    fn issue_group_order(&mut self, order: GroupOrder) {
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

    /// Send the selection to attack `target`.
    fn issue_attack(&mut self, target: RegimentHandle) {
        if !self.selected.is_empty() {
            self.attack_mark = Some((target, Instant::now()));
            self.ui_sound(sounds::UiSound::Attack);
        }
        self.human_orders
            .extend(self.selected.iter().map(|&regiment| PlayerCommand {
                player: self.sandbox.side,
                command: gc_sim::Command::Attack { regiment, target },
            }));
    }

    /// Send the selection to attack the building `structure`.
    fn issue_siege(&mut self, structure: StructureHandle) {
        if !self.selected.is_empty() {
            self.ui_sound(sounds::UiSound::Attack);
        }
        self.human_orders
            .extend(self.selected.iter().map(|&regiment| PlayerCommand {
                player: self.sandbox.side,
                command: gc_sim::Command::AttackStructure {
                    regiment,
                    structure,
                },
            }));
    }

    /// The building the mouse is over whose owner `accept` allows.
    fn building_under_cursor(&self, accept: impl Fn(PlayerId) -> bool) -> Option<StructureHandle> {
        self.pick_building(|s| accept(s.owner))
    }

    /// The nearest building the mouse is over that `accept` allows.
    fn pick_building(&self, accept: impl Fn(&StructureView) -> bool) -> Option<StructureHandle> {
        let ray = self.cursor_ray()?;
        let hm = &self.heightmap;
        let shown = buildings::shown_to(self.sandbox.side, self.reveal_all);
        buildings::pick(
            &self.world,
            &self.buildings,
            |x, z| hm.height_at(x, z).max(hm.water_level()),
            ray,
            |s| shown(s) && accept(s),
        )
    }

    /// An enemy building of the side you command, under the mouse.
    fn enemy_building_under_cursor(&self) -> Option<StructureHandle> {
        let side = self.sandbox.side;
        self.building_under_cursor(|o| self.world.hostile(side, o))
    }

    /// Add `h` to the selection, or take it out if it is already in.
    fn toggle_selected(&mut self, h: RegimentHandle) {
        match self.selected.iter().position(|&s| s == h) {
            Some(i) => {
                self.selected.swap_remove(i);
            }
            None => self.selected.push(h),
        }
    }

    /// Fill `self.group` with the selection as group orders see it, each
    /// regiment with its place in an arranged army.
    fn fill_group(&mut self) {
        let regiments = &self.world.regiments;
        self.selected.retain(|&h| regiments.get(h).is_some());
        self.group.clear();
        for &h in &self.selected {
            let r = self.world.regiments.get(h).unwrap();
            let mut m = GroupMember::of(&r, &self.world.units()[r.unit as usize]);
            if let Some(&role) = self.units.roles.get(r.unit as usize) {
                m.role = role;
            }
            self.group.push(m);
        }
    }

    /// Expand a group order for the current selection into
    /// `self.group_commands` (one move per regiment), arranged in the
    /// chosen army shape.
    fn plan_order(&mut self, order: GroupOrder) {
        self.fill_group();
        self.group_commands.clear();
        gc_sim::arrange::arranged_move(
            &mut self.group,
            order,
            self.army_shapes.arrangement(),
            &mut self.group_commands,
        );
        battle::shift_into_zone(&self.world, self.sandbox.side, &mut self.group_commands);
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
            let stats = &self.units.stats[r.unit as usize];
            let files = if files == 0 { r.frontage } else { files };
            let (width, depth) = stats.block_size_in(r.formation, files, r.models);
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
        let view_proj = self.active_camera().view_proj();
        for r in self
            .world
            .regiments
            .iter()
            .filter(|r| r.owner == self.sandbox.side)
        {
            let p = drawn_position(&self.prev_positions, self.alpha, &r);
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
            self.look_at(centre);
        }
    }

    /// Ground height for the cameras: the terrain, or the water's surface
    /// where it is deeper than the bed.
    fn ground_at(&self, p: Vec2) -> f32 {
        ground_of(&self.heightmap, p)
    }

    /// Y: start riding with the first selected regiment still on the field,
    /// or come back out if already riding. Does nothing with no selection.
    fn toggle_ride(&mut self) {
        if self.ride.is_some() {
            self.leave_ride();
            return;
        }
        self.director.stop();
        if !self.match_started || self.menu_open || self.showroom.is_some() || self.viewer.is_some()
        {
            return;
        }
        let regiments = &self.world.regiments;
        let Some(r) = self
            .selected
            .iter()
            .filter_map(|&h| regiments.get(h))
            .find(|r| r.morale != MoraleState::Shattered)
        else {
            return;
        };
        let target = self.ride_target(&r);
        let camera = FollowCamera::new(target, |p| self.ground_at(p));
        self.ride = Some(Ride {
            regiment: r.handle,
            camera,
        });
    }

    /// F8: the director camera takes over the view, or gives it back.
    fn toggle_director(&mut self) {
        if self.director.is_on() {
            self.director.stop();
            self.say("Director camera off.".to_string());
            return;
        }
        if !self.match_started || self.menu_open || self.showroom.is_some() || self.viewer.is_some()
        {
            return;
        }
        self.leave_ride();
        self.director.start(&self.camera);
        self.say(
            "Director camera on: it follows the fight. Move the view, or press F8, to take over."
                .to_string(),
        );
    }

    /// Back to the bird's-eye camera, over the regiment we were riding with
    /// (or where the view last was, if it is gone), at the normal distance.
    /// Does nothing when not riding.
    fn leave_ride(&mut self) {
        let Some(ride) = self.ride.take() else {
            return;
        };
        let focus = self
            .world
            .regiments
            .position(ride.regiment)
            .map_or(ride.camera.anchor(), from_fx_vec);
        // The bird's-eye camera follows the land, not the water's surface.
        let ground = self.heightmap.height_at(focus.x, focus.y);
        self.camera
            .resume_from_ride(focus, ground, ride.camera.forward_xz());
    }

    /// What the ride camera should follow: where the renderer draws the
    /// regiment this frame.
    fn ride_target(&self, r: &gc_sim::RegimentView) -> RideTarget {
        let (centre, facing) = drawn_pose(&self.prev_positions, self.alpha, r);
        let stats = &self.units.stats[r.unit as usize];
        let (_, depth) = stats.block_size_in(r.formation, r.files, r.models);
        RideTarget {
            centre,
            facing,
            depth: fx_to_f32(depth),
        }
    }

    /// Move the ride camera towards its regiment. Drops back to the normal
    /// camera if the regiment is gone or has shattered; a routing one is
    /// still followed.
    fn update_ride(&mut self, dt: f32) {
        let Some(handle) = self.ride.as_ref().map(|ride| ride.regiment) else {
            return;
        };
        let target = self
            .world
            .regiments
            .get(handle)
            .filter(|r| r.morale != MoraleState::Shattered)
            .map(|r| self.ride_target(&r));
        let heightmap = &self.heightmap;
        match (target, &mut self.ride) {
            (Some(target), Some(ride)) => {
                ride.camera.update(dt, target, |p| ground_of(heightmap, p));
            }
            _ => self.leave_ride(),
        }
    }

    /// The regiment whose body the mouse ray passes through: one of your
    /// side's (`own`) or one your side can see that is not. Only while
    /// riding: from the low camera a click on a soldier lands on ground far
    /// behind it, so the ground point is a poor guide.
    fn body_under_cursor(&self, own: bool) -> Option<RegimentHandle> {
        let ridden = self.ride.as_ref()?.regiment;
        let (origin, dir) = self.cursor_ray()?;
        // A block hidden behind a ridge is not under the mouse.
        let reach = self.ground_along(origin, dir).unwrap_or(f32::INFINITY);
        let side = self.sandbox.side;
        self.world
            .regiments
            .iter()
            // The eye is inside the ridden block's box: it would take every click.
            .filter(|r| r.handle != ridden && (r.owner == side) == own)
            // Orders can only name regiments your side can see.
            .filter(|r| own || self.world.sees(side, r.handle))
            .filter_map(|r| {
                let stats = &self.units.stats[r.unit as usize];
                let (width, depth) = stats.block_size_in(r.formation, r.files, r.models);
                let (centre, facing) = drawn_pose(&self.prev_positions, self.alpha, &r);
                let ground = self.ground_at(centre);
                let half = Vec2::new(fx_to_f32(depth), fx_to_f32(width)) * 0.5
                    + Vec2::splat(RIDE_PICK_MARGIN);
                camera::ray_hits_block(
                    origin,
                    dir,
                    centre,
                    facing,
                    half,
                    (ground - 0.2, ground + RIDE_PICK_HEIGHT),
                )
                .filter(|&t| t <= reach + RIDE_PICK_MARGIN)
                .map(|t| (r.handle, t))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(h, _)| h)
    }

    /// The enemy regiment under the mouse: the one the ray passes through
    /// (riding), else the one nearest the ground point.
    pub fn enemy_under_cursor(&self) -> Option<RegimentHandle> {
        self.body_under_cursor(false)
            .or_else(|| self.ground_under_cursor().and_then(|p| self.enemy_at(p)))
    }

    /// The enemy regiment whose block is under (or within a step of) `p`.
    fn enemy_at(&self, p: Vec2) -> Option<RegimentHandle> {
        self.world
            .regiments
            .iter()
            .filter(|r| r.owner != self.sandbox.side)
            // Orders can only name regiments your side can see.
            .filter(|r| self.world.sees(self.sandbox.side, r.handle))
            .filter_map(|r| {
                let stats = &self.units.stats[r.unit as usize];
                let (width, depth) = stats.block_size_in(r.formation, r.files, r.models);
                let (at, facing) = drawn_pose(&self.prev_positions, self.alpha, &r);
                let to = p - at;
                let ahead = to.dot(facing).abs() - fx_to_f32(depth) / 2.0;
                let across = to.perp_dot(facing).abs() - fx_to_f32(width) / 2.0;
                let outside = ahead.max(across);
                (outside <= ATTACK_PICK_TILES).then_some((r.handle, outside))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(h, _)| h)
    }

    /// R (or F): the selection's next formation, Line, Tight, Loose, then Shield
    /// wall when any of them carry shields. Regiments without shields form
    /// Tight instead of a shield wall.
    fn cycle_formation(&mut self) {
        let regiments = &self.world.regiments;
        let units = self.world.units();
        let tick = self.world.tick_count();
        // Regiments locked in melee or running can't re-form.
        let ready = || {
            self.selected
                .iter()
                .filter_map(|&h| regiments.get(h))
                .filter(|r| !r.engaged && !r.morale.broken())
        };
        let Some(first) = ready().next() else {
            if !self.selected.is_empty() {
                self.say("Locked in melee or running: no new formation".into());
            }
            return;
        };
        let shielded = |r: &gc_sim::RegimentView| units[r.unit as usize].shield;
        // Cycle on from what F just ordered for this selection (its orders
        // may not have landed yet), else from where it stands now: a shield
        // regiment's formation, since the rest stand in Tight for its wall.
        let current = match &self.formation_pick {
            Some((selection, f, at)) if *selection == self.selected && tick < at + 40 => *f,
            _ => ready().find(shielded).unwrap_or(first).formation,
        };
        let next = match current {
            Formation::Line => Formation::Tight,
            Formation::Tight => Formation::Loose,
            Formation::Loose if ready().any(|r| shielded(&r)) => Formation::ShieldWall,
            Formation::Loose | Formation::ShieldWall => Formation::Line,
        };
        let mut closed_up = false;
        for r in ready() {
            // Regiments without shields close up instead.
            let formation = if units[r.unit as usize].can_form(next) {
                next
            } else {
                closed_up = true;
                Formation::Tight
            };
            self.human_orders.push(PlayerCommand {
                player: self.sandbox.side,
                command: gc_sim::Command::SetFormation {
                    regiment: r.handle,
                    formation,
                },
            });
        }
        self.formation_pick = Some((self.selected.clone(), next, tick));
        self.ui_sound(sounds::UiSound::Move);
        let note = if closed_up {
            " (Tight for those without shields)"
        } else {
            ""
        };
        self.say(format!("{}{note}: {}", next.name(), formation_hint(next)));
    }

    /// Keep the army shapes and the choice in the player's folder;
    /// `durable` for a newly saved shape.
    fn keep_army_shapes(&self, durable: bool) {
        if let Some(path) = army_shape::shapes_path() {
            self.army_shapes.store(&path, durable);
        }
    }

    /// Say which army shape group orders take now.
    fn say_army_shape(&mut self) {
        let choice = self.army_shapes.choice;
        self.ui_sound(sounds::UiSound::Select);
        self.say(format!("Army shape: {}. {}", choice.label(), choice.tip()));
    }

    /// O: the next army shape, the presets and then your own.
    fn cycle_army_shape(&mut self) {
        self.army_shapes.choice = self.army_shapes.next();
        self.keep_army_shapes(false);
        self.say_army_shape();
    }

    /// A chip of the selection panel: take the army shape it names.
    fn pick_army_shape(&mut self, chip: usize) {
        if let Some(&choice) = self.army_shapes.chips().get(chip) {
            self.army_shapes.choice = choice;
            self.keep_army_shapes(false);
            self.say_army_shape();
        }
    }

    /// Shift+O: keep the shape the selected regiments stand in, as one of
    /// your own, and use it from now on.
    fn save_army_shape(&mut self) {
        // A replay's regiments stand where their player put them, not you.
        if self.watching.is_some() {
            self.say("That is off while you watch a replay.".into());
            return;
        }
        self.fill_group();
        let Some(shape) = Shape::capture(&self.group) else {
            self.say("Select two or more regiments to save their shape.".into());
            return;
        };
        let full = self.army_shapes.saved.iter().all(Option::is_some);
        self.army_shapes.save(shape);
        self.keep_army_shapes(true);
        self.ui_sound(sounds::UiSound::Select);
        let name = self.army_shapes.choice.label();
        self.say(if full {
            format!("Saved as {name}, in place of the old one.")
        } else {
            format!("Saved as {name}.")
        });
    }

    /// Z: set the selection's spears and pikes against a charge, or, if
    /// they all stand braced already, lift them. Only regiments with long
    /// weapons out of the fight can brace.
    fn toggle_brace(&mut self) {
        let units = self.world.units();
        let spears: Vec<_> = self
            .selected
            .iter()
            .filter_map(|&h| self.world.regiments.get(h))
            .filter(|r| units[r.unit as usize].reach && !r.morale.broken())
            .collect();
        if spears.is_empty() {
            self.say("Only spears and pikes can brace".into());
            return;
        }
        // Set any that can be set; once none can, lift those that are.
        let settable = |r: &&gc_sim::RegimentView| !r.braced && !r.engaged && !r.falling_back;
        let on = spears.iter().any(|r| settable(&r));
        let ready: Vec<_> = spears
            .iter()
            .filter(|r| if on { settable(r) } else { r.braced })
            .map(|r| r.handle)
            .collect();
        if ready.is_empty() {
            self.say("Locked in melee: no time to set spears".into());
            return;
        }
        for regiment in ready {
            self.human_orders.push(PlayerCommand {
                player: self.sandbox.side,
                command: gc_sim::Command::Brace { regiment, on },
            });
        }
        self.ui_sound(sounds::UiSound::Move);
        self.say(if on {
            "Braced: a charge into their front runs onto the points; they stand until ordered"
                .into()
        } else {
            "Spears lifted".into()
        });
    }

    /// E: set the woods beside each selected regiment alight (the sim
    /// refuses those with none in reach; docs/32-changing-land.md).
    fn burn_woods(&mut self) {
        if self.world.deploying() {
            self.say("Fire can't be set until the battle begins".into());
            return;
        }
        if !self.world.weather_lets_fire() {
            self.say("Too wet to burn: the ground must dry first".into());
            return;
        }
        let before = self.human_orders.len();
        for &regiment in &self.selected {
            if self.world.regiments.owner(regiment).is_some() {
                self.human_orders.push(PlayerCommand {
                    player: self.sandbox.side,
                    command: gc_sim::Command::Burn { regiment },
                });
            }
        }
        if self.human_orders.len() == before {
            return;
        }
        self.ui_sound(sounds::UiSound::Move);
        self.say(
            "Burning the woods beside them, where there are any: fire spreads with the wind".into(),
        );
    }

    /// X: light the selection's torches, or, if they all burn already, put
    /// them out. They burn only in the dark (docs/32-changing-land.md).
    fn toggle_torches(&mut self) {
        let mine: Vec<_> = self
            .selected
            .iter()
            .filter_map(|&h| self.world.regiments.get(h))
            .collect();
        if mine.is_empty() {
            return;
        }
        let lit = mine.iter().any(|r| !r.torches);
        for r in mine.iter().filter(|r| r.torches != lit) {
            self.human_orders.push(PlayerCommand {
                player: self.sandbox.side,
                command: gc_sim::Command::Torches {
                    regiment: r.handle,
                    lit,
                },
            });
        }
        self.ui_sound(sounds::UiSound::Move);
        let dark = self.world.air().dark;
        self.say(match (lit, dark) {
            (true, true) => "Torches lit: they see further and can't be ambushed, but are seen and shot from afar".into(),
            (true, false) => "Torches ready: they burn once it is dark".into(),
            (false, _) => "Torches out: hidden in the dark again".into(),
        });
    }

    /// V: the selection pulls back out of its fights in good order.
    fn fall_back(&mut self) {
        let ready: Vec<_> = self
            .selected
            .iter()
            .filter_map(|&h| self.world.regiments.get(h))
            .filter(|r| !r.morale.broken() && !r.falling_back)
            .map(|r| r.handle)
            .collect();
        if ready.is_empty() {
            return;
        }
        for regiment in ready {
            self.human_orders.push(PlayerCommand {
                player: self.sandbox.side,
                command: gc_sim::Command::FallBack { regiment },
            });
        }
        self.ui_sound(sounds::UiSound::Move);
        self.say("Falling back: they back away facing the enemy, who gets one parting blow".into());
    }

    /// One of your regiments under the mouse: the one the ray passes through
    /// (riding), else the nearest to the ground point within `reach` tiles.
    fn own_regiment_at_cursor(&self, reach: f32) -> Option<RegimentHandle> {
        self.body_under_cursor(true).or_else(|| {
            self.ground_under_cursor()
                .and_then(|p| self.nearest_own(p, reach))
        })
    }

    fn nearest_own(&self, p: Vec2, radius: f32) -> Option<RegimentHandle> {
        self.world
            .regiments
            .iter()
            .filter(|r| r.owner == self.sandbox.side)
            .map(|r| {
                let at = drawn_position(&self.prev_positions, self.alpha, &r);
                (r.handle, at.distance(p))
            })
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
        self.stage_siege();

        // Fixed-rate simulation; render interpolates between the last two
        // ticks. Pause and slow motion scale game time, not the camera.
        let game_dt = if self.paused {
            Duration::ZERO
        } else {
            dt.mul_f32(SPEEDS[self.speed] * self.film_pace())
        };
        // The sim stands still behind the menu, except in a network match,
        // which runs on the host's clock.
        if !self.menu_open && self.online.is_none() {
            self.sim_clock += game_dt;
            // A replay hurries through the deployment, where time stood still.
            if self.watching.is_some() && self.world.deploying() {
                self.sim_clock += game_dt * (DEPLOY_REPLAY_SPEED - 1);
                self.sim_clock = self.sim_clock.min(TICK * MAX_TICKS_PER_FRAME);
            }
        }
        let mut ticks_this_frame = if self.online.is_some() {
            self.online_turns(now)
        } else {
            0
        };
        while self.sim_clock >= TICK && ticks_this_frame < MAX_TICKS_PER_FRAME {
            self.sim_clock -= TICK;
            self.pending.clear();
            let tick = self.world.tick_count();
            if self
                .watching
                .as_ref()
                .is_some_and(|w| tick >= w.replay.ticks)
            {
                self.sim_clock = Duration::ZERO;
                self.paused = true;
                self.sandbox.dirty = true;
                break;
            }
            ticks_this_frame += 1;
            // A replay plays its recorded orders and nobody else's.
            if let Some(w) = &self.watching {
                self.pending.extend_from_slice(w.replay.commands_at(tick));
                self.human_orders.clear();
                self.step();
                self.check_replay_divergence();
                continue;
            }
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
        // A slow frame at high speed can owe more ticks than one frame may
        // run. Owe less than one: more would have the game run flat out for
        // a long while after the slowdown (even through a pause), and draw
        // past the newest tick.
        self.sim_clock = self.sim_clock.min(TICK - Duration::from_nanos(1));
        if ticks_this_frame > 0 {
            // Tuned stats take effect in the tick after they are sent.
            self.units.stats.clone_from_slice(self.world.units());
            self.judge_battle();
            if self.recording.as_ref().is_some_and(|r| {
                self.world.tick_count() >= r.saved_at.unwrap_or(0) + replays::SAVE_EVERY
            }) {
                self.save_recording();
            }
        }
        let alpha = match &self.online {
            // Turns arrive on the host's clock: move smoothly towards the
            // latest one.
            Some(o) => (o.last_turn.elapsed().as_secs_f32() / TICK.as_secs_f32()).min(1.0),
            None => self.sim_clock.as_secs_f32() / TICK.as_secs_f32(),
        };
        self.alpha = alpha;
        // Riding with a regiment: its camera follows where it is drawn now.
        self.update_ride(dt.as_secs_f32());
        let camera = self.active_camera();
        let (view_proj, eye, forward) = (camera.view_proj(), camera.eye(), camera.forward());
        let (scene_focus, scene_distance) = camera.scenery_view();
        let lens_zoom = camera.zoom_distance();
        let pixel_scale = camera.pixel_scale(self.window.inner_size().height as f32);
        // A low camera sees a long strip of ground: spend the shadow map's
        // texels on what is near it.
        self.renderer
            .set_shadow_range(self.ride.is_some().then_some(camera::RIDE_SHADOW_RANGE));

        self.instances.clear();
        self.boxed.clear();
        if let Some(rect) = self.selection_box() {
            self.regiments_in_box(rect);
        }
        if let Some(order) = self.drag_order() {
            self.plan_order(order);
            self.push_order_preview();
        }
        // The unit viewer shows one model on a bare field: none of the
        // battlefield is drawn behind it.
        let viewing = self.viewer.is_some();
        if !viewing {
            self.push_placement_ghost();
            self.push_building_ghost();
            self.push_wall_ghost();
            self.push_plan_ghosts();
            self.push_clearing_marks();
            self.push_power_aim();
            let hm = &self.heightmap;
            self.siege
                .set_density(self.menu.settings().graphics.effects_density());
            // Behind the menu its time stands still, as the particles' do.
            let siege_dt = if self.menu_open && self.online.is_none() {
                0.0
            } else {
                game_dt.as_secs_f32()
            };
            (self.siege).advance(siege_dt, |p| hm.drawn_height(p.x, p.y));
            self.note_building_looks();
            self.push_buildings();
        }
        self.puppets.advance(game_dt.as_secs_f32());
        let world = &self.world;
        self.puppets.pair(|a, b| world.hostile(a, b));
        // The simulation's time, in ticks, interpolated as the regiments are.
        let now = world.tick_count() as f32 - 1.0 + alpha;
        self.puppets.champions(world.challenge(), now);
        self.puppets.generals(world.generals(), now);
        let marked = self
            .attack_mark
            .filter(|(_, at)| at.elapsed() < ATTACK_MARK)
            .map(|(h, _)| h);
        // A selected tower's chosen target stays marked while it holds.
        let tower_target = self.picked_structure().and_then(|h| self.tower_focus(h));
        let cull = self.renderer.cull(view_proj);
        if let Some(art) = &mut self.art {
            art.begin(eye, forward, pixel_scale, cull);
        }
        if !viewing {
            self.push_building_models();
            self.push_crows();
            self.push_creatures();
        }
        self.mark_shooting();
        if viewing {
            self.renderer.set_scenery_frame(&[], &[]);
        } else {
            self.push_scenery(scene_focus, scene_distance, &cull);
        }
        let viewer = self.sandbox.side;
        if let Some(zone) = self.world.deploy_zone(viewer).filter(|_| !viewing) {
            Self::push_zone_ring(
                &mut self.instances,
                from_fx_vec(zone.center),
                fx_to_f32(zone.radius),
            );
        }
        // Flags and banners grow as the Commander View pulls back, so they
        // stay readable.
        let zoom = if self.commander {
            (self.camera.distance / COMMANDER_IN).max(1.0)
        } else {
            1.0
        };
        let ridden = self.ride.as_ref().map(|ride| ride.regiment);
        let haste_glow = self.haste_glow();
        // Soldiers stand only where a regiment of theirs could walk (an
        // open gate is ground to them; a shut one is wall).
        let nav = self.world.nav();
        let firm = |owner| move |p: Vec2| nav.standable(owner, fx_vec(p));
        // Footsteps and wear follow the effects clock: none behind the menu
        // or while it stands still.
        let fx_dt = if self.menu_open && self.online.is_none() {
            0.0
        } else {
            game_dt.as_secs_f32() * self.effects.time_scale()
        };
        for r in self.world.regiments.iter().take_while(|_| !viewing) {
            // Enemies hidden in woods and tall grass aren't drawn (their
            // soldiers keep marching out of sight, below).
            let seen = self.reveal_all || self.world.sees(viewer, r.handle);
            // Fighting dirties soldiers wherever the fight is, drawn or not.
            self.aftermath
                .tend(r.handle, r.engaged, self.world.is_worker(r.handle), fx_dt);
            let (pos, facing) = drawn_pose(&self.prev_positions, self.alpha, &r);
            let [cr, cg, cb] = color_of(&self.parties, r.owner);
            let selected = self.selected.contains(&r.handle) || self.boxed.contains(&r.handle);
            let stats = &self.units.stats[r.unit as usize];
            let (file_spacing, rank_spacing) = stats.spacing(r.formation);
            let (file_spacing, rank_spacing) = (fx_to_f32(file_spacing), fx_to_f32(rank_spacing));
            let (width, depth) = stats.block_size_in(r.formation, r.files, r.models);
            let (width, depth) = (fx_to_f32(width), fx_to_f32(depth));
            // Marks of a regiment on a wall sit on the wall top.
            let up = if r.on_wall {
                self.puppets.wall_top(r.handle)
            } else {
                0.0
            };
            // No plate under the regiment we ride with: the eye is inside it
            // and the plate would fill the view.
            if seen
                && (selected || marked == Some(r.handle) || tower_target == Some(r.handle))
                && ridden != Some(r.handle)
            {
                self.instances.push(BoxInstance {
                    pos: pos.to_array(),
                    facing: facing.to_array(),
                    size: [width + 0.3, PLATE_HEIGHT, depth + 0.3],
                    color: if selected {
                        // Darker than the team colour so the soldiers on it stand out.
                        [cr / 2 + 20, cg / 2 + 20, cb / 2 + 20, 255]
                    } else {
                        [255, 40, 20, 255]
                    },
                    lift: up,
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
                disorder: f32::from(r.disorder) / f32::from(gc_sim::combat::DISORDER_TICKS),
                charge: r.charge,
                ploughed: (f32::from(r.ploughed) / puppets::DENT_EASE_TICKS).min(1.0),
                ploughed_by: r.ploughed_by,
                general: r.general,
                fighter: fighter_of(stats, &r),
            };
            // Manning a wall, the soldiers stand on its tiles.
            // At the head of a ladder they stand in their block, lifted to
            // the wall top, with ladders against the wall below them.
            let ladder = r
                .climber
                .then(|| buildings::ladder_props(&self.world, &self.buildings, r.handle, pos))
                .flatten();
            if let Some((_, _, top)) = ladder {
                self.puppets.stand_on_ladder(r.handle, top);
            } else if r.on_wall
                || self.puppets.on_wall(r.handle)
                || self.puppets.on_ladder(r.handle)
            {
                let slots = buildings::wall_slots(&self.world, &self.buildings, r.handle);
                self.puppets.stand_on_wall(r.handle, slots);
            }
            // How workers clear a tile, and where its trees should fall from
            // (the regiment's centre, when the viewer sees it; else straight
            // as they stood), whether or not they are on screen.
            let clearing_style = (self.units.factions.get(usize::from(r.unit)))
                .map_or(ambient::Style::Fell, |&f| ambient::Style::of(f));
            if let Some(tile) = (self.world.clearing_tile_of(r.handle)).filter(|_| !block.routing) {
                let tile_centre = Vec2::new(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5);
                let from = if seen { pos } else { tile_centre };
                self.ambient.felling.cutting(tile, from, clearing_style);
            }
            if let (true, Some((spots, facing, top))) = (seen, ladder) {
                for at in spots {
                    self.instances.push(BoxInstance {
                        pos: at.to_array(),
                        facing: facing.to_array(),
                        size: [0.5, top + 0.2, 0.1],
                        color: [120, 84, 48, 255],
                        lift: 0.0,
                    });
                }
            }
            if !seen {
                self.puppets.update_on(
                    &block,
                    r.owner,
                    game_dt.as_secs_f32(),
                    firm(r.owner),
                    |_| {},
                );
                continue;
            }
            if block.disorder > 0.0 {
                Self::push_disorder_outline(
                    &mut self.instances,
                    pos,
                    facing,
                    width,
                    depth,
                    block.disorder,
                    r.handle.index,
                );
            }
            // A hasted, marching, steadfast or summoned regiment glows round
            // its block, the summoned ones faintly.
            let auras = [
                (buildings::Aura::Haste, self.world.regiment_haste(r.handle)),
                (buildings::Aura::March, self.world.regiment_march(r.handle)),
                (
                    buildings::Aura::Steadfast,
                    self.world.regiment_steadfast(r.handle),
                ),
                (
                    buildings::Aura::Spectral,
                    self.world.regiment_summoned(r.handle),
                ),
                (buildings::Aura::Sung, u16::from(r.sung)),
            ];
            for (aura, left) in auras {
                if left == 0 {
                    continue;
                }
                buildings::push_regiment_aura(
                    aura,
                    pos,
                    facing,
                    (width, depth),
                    haste_glow,
                    &mut self.instances,
                );
            }
            // Your regiments that no enemy can see carry a moss-green flag
            // above the treetops.
            if !self.world.hostile(viewer, r.owner) && self.world.hidden(r.handle) {
                self.instances.push(BoxInstance {
                    pos: pos.to_array(),
                    facing: facing.to_array(),
                    size: HIDDEN_FLAG.map(|v| v * zoom),
                    color: HIDDEN_COLOR,
                    lift: HIDDEN_LIFT * zoom,
                });
            }
            let blink = self.started.elapsed().as_millis() % 600 < 300;
            // Workers carry no morale banner: their nerve is low by design
            // (they waver all the time) and they are not in the battle line.
            let banner = match r.morale {
                _ if self.world.is_worker(r.handle) => None,
                // Drawn as a standard model with the scenery instead.
                m if !self.commander && banner_model(m).is_some_and(|b| self.scenery.draws(b)) => {
                    None
                }
                MoraleState::Steady => None,
                MoraleState::Wavering => blink.then_some([235, 170, 40, 200]),
                MoraleState::Routing => Some([245, 245, 240, 200]),
                MoraleState::Shattered => Some([110, 110, 110, 120]),
            };
            if self.commander {
                // One block per regiment with a tall banner: the flag shows
                // the team, or the morale colour when shaken.
                let shade = if selected { 255 } else { 210 };
                let tint = |c: u8| (u32::from(c) * shade / 255) as u8;
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
                let side = Vec2::new(facing.y, -facing.x);
                let pole = COMMANDER_POLE.map(|v| v * zoom);
                let flag_size = COMMANDER_FLAG.map(|v| v * zoom);
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
                self.puppets.update_on(
                    &block,
                    r.owner,
                    game_dt.as_secs_f32(),
                    firm(r.owner),
                    |_| {},
                );
                continue;
            }
            if let Some(color) = banner {
                self.instances.push(BoxInstance {
                    pos: pos.to_array(),
                    facing: facing.to_array(),
                    size: BANNER_SIZE,
                    color,
                    lift: BANNER_LIFT + up,
                });
            }
            let model = self.units.models.get(r.unit as usize).copied().flatten();
            if let (Some(model), Some(art)) = (model, self.art.as_mut()) {
                let (view, height) = (art.view, art.heights[model]);
                let buckets = &mut art.buckets;
                let heightmap = &self.heightmap;
                let clips = &art.clips;
                let time = self.puppets.time();
                let highlight = if selected { 70 } else { 0 };
                // Torches burn only in the dark.
                let dark = self.world.air().dark;
                let torchlit = r.torches && dark;
                // The tuning panel's multipliers (F10).
                let (torch_light, torch_flames, brood_glow) = (
                    self.atmosphere.extra(atmosphere::Extra::TorchLight),
                    self.atmosphere.extra(atmosphere::Extra::TorchFlames),
                    self.atmosphere.extra(atmosphere::Extra::BroodGlow),
                );
                let brood =
                    self.units.factions.get(usize::from(r.unit)) == Some(&gc_ai::Faction::Brood);
                let effects = &mut self.effects;
                // Torches give one warm light over the block, and more along
                // a wide front so a long line is lit all along; the Brood's
                // own glow (in the dark alone, and not over their torches,
                // the brighter light) is a faint lime one.
                let light = if torchlit {
                    let count = ((width / TORCH_LIGHT_SPACING).ceil() as i32).clamp(1, 3);
                    Some((
                        TORCH_FIRE,
                        TORCH_LIGHT_RADIUS,
                        TORCH_LIGHT_STRENGTH * torch_light,
                        count,
                    ))
                } else if brood && dark {
                    Some((
                        GRUB_LIME,
                        GRUB_LIGHT_RADIUS,
                        GRUB_LIGHT_STRENGTH * brood_glow,
                        1,
                    ))
                } else {
                    None
                };
                if let Some((colour, radius, strength, count)) = light {
                    // A flicker of its own for each light that stays with the
                    // regiment, so a march does not make it strobe.
                    let seed = (r.handle.index as u8).wrapping_mul(53)
                        ^ (r.handle.generation as u8).wrapping_mul(7);
                    let side = Vec2::new(facing.y, -facing.x);
                    for i in 0..count {
                        let along = (i as f32 - (count - 1) as f32 / 2.0) * TORCH_LIGHT_SPACING;
                        let at = pos + side * along;
                        effects.regiment_light(
                            glam::Vec3::new(
                                at.x,
                                heightmap.height_at(at.x, at.y) + TORCH_LIGHT_HEIGHT,
                                at.y,
                            ),
                            colour,
                            radius,
                            strength,
                            seed.wrapping_add((i as u8).wrapping_mul(29)),
                        );
                    }
                }
                let dirt = self.aftermath.dirt(r.handle);
                // Soldiers at a building they attack face it: blades close
                // up to the wall and swing at it, archers shoot.
                let siege = &mut self.siege;
                let wall = (self.world.siege_target(r.handle))
                    .and_then(|h| siege.look(h).copied())
                    .filter(|_| !block.routing && !r.engaged);
                let melee = stats.range.raw() == 0;
                let at_wall = wall
                    .is_some_and(|w| melee && !r.moving && w.gap(pos) <= depth / 2.0 + WALL_REACH);
                let lean = siege.lean(r.handle, at_wall, game_dt.as_secs_f32());
                // A ram draws back between blows and swings in to land each.
                let breaker = self.units.breakers.get(usize::from(r.unit)).copied();
                let ram = siege::Kind::of(!melee, breaker.unwrap_or_default()) == siege::Kind::Ram;
                let draw_back = siege
                    .since_struck(r.handle)
                    .filter(|_| ram && at_wall)
                    .map_or(0.0, |t| {
                        RAM_DRAW * (t.min(1.0) * std::f32::consts::PI).sin()
                    });
                if let Some(w) = wall {
                    siege.untouch(r.handle, w.handle);
                }
                let shooting = self.shooting[r.handle.index as usize]
                    || (wall.is_some() && !melee && siege.shooting(r.handle));
                // Builders at a building going up (or being mended) turn
                // to it and swing their tools.
                let site = (self.world.building_target(r.handle))
                    .and_then(|h| self.world.structures()?.get(h))
                    .filter(|b| !block.routing && (!b.finished() || b.hp < b.max_hp))
                    .map(|b| {
                        let reach = f32::from(b.width.max(b.depth)) / 2.0 + 2.0;
                        (buildings::centre_of(b.tile, b.width, b.depth), reach)
                    });
                // Workers clearing land swing at the tile they are on.
                let cutting = self
                    .world
                    .clearing_tile_of(r.handle)
                    .filter(|_| !block.routing)
                    .map(|tile| {
                        let wood = self.world.terrain().growth(tile.0, tile.1) != Growth::Scrub;
                        (
                            tile,
                            Vec2::new(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5),
                            wood,
                        )
                    });
                self.puppets.update_on(
                    &block,
                    r.owner,
                    game_dt.as_secs_f32(),
                    firm(r.owner),
                    |s| {
                        let mut s = s;
                        // His head turns are from the way the puppets face him.
                        let puppet_facing = s.facing;
                        // At the wall: the ranks step up to it, and the front
                        // one swings.
                        let mut battering = None;
                        if let Some(w) = wall.filter(|_| s.fight.is_none() && s.speed <= 0.6) {
                            if melee && lean > 0.0 {
                                let stand = siege::stand_at(&w, s.pos, s.facing, lean, draw_back);
                                (s.pos, s.facing) = (stand.pos, stand.facing);
                                if let Some(near) = stand.swing {
                                    battering = Some(near);
                                    siege.touch(r.handle, near);
                                }
                            } else if !melee && shooting {
                                s.facing = (w.centre() - s.pos).normalize_or(s.facing);
                            }
                        }
                        let ground = heightmap.height_at(s.pos.x, s.pos.y);
                        let at = glam::Vec3::new(s.pos.x, ground + s.lift, s.pos.y);
                        let Some(lod) = view.place(height, at) else {
                            return;
                        };
                        let toward = if s.backing { -s.facing } else { s.facing };
                        effects.footstep(fx_dt, at, toward, s.speed);
                        if torchlit && torch_bearer(s.phase) {
                            let hand = glam::Vec3::new(
                                s.pos.x - s.facing.y * 0.12,
                                ground + height * TORCH_HEIGHT,
                                s.pos.y + s.facing.x * 0.12,
                            );
                            effects.torch(hand, fx_dt * torch_flames);
                        }
                        let look = models::look(s.look);
                        let ([wr, wg, wb], tone) = aftermath::worn([cr, cg, cb], look.tone, dirt);
                        // The rate keeps loops out of step; blows are timed by
                        // their own progress, so it leaves fights alone.
                        let seconds = time * look.rate + s.phase * 4.0;
                        // At the site and still: off on the way, they walk.
                        let working = site
                            .filter(|&(c, reach)| {
                                s.fight.is_none() && s.speed <= 0.25 && s.pos.distance(c) <= reach
                            })
                            .map(|(c, _)| c);
                        // At the tile they clear and still: swing at it.
                        let chopping = cutting.filter(|&(_, c, _)| {
                            s.fight.is_none() && s.speed <= 0.25 && s.pos.distance(c) <= CUT_REACH
                        });
                        let working = working.or(chopping.map(|(_, c, _)| c));
                        let facing =
                            working.map_or(s.facing, |c| (c - s.pos).normalize_or(s.facing));
                        if let Some((_, c, wood)) = chopping {
                            let tool = at + Vec3::new(facing.x * 0.5, 0.9, facing.y * 0.5);
                            let target = Vec3::new(c.x, heightmap.height_at(c.x, c.y), c.y);
                            effects.chop(fx_dt, tool, facing, wood, clearing_style, target);
                        }
                        let pose = match s.fight {
                            Some(fight) => clips.fight(model, fight, seconds, s.look),
                            None if working.is_some() || battering.is_some() => clips
                                .pose(model, swing(s.phase < 0.5), seconds)
                                .map(|p| (p, false)),
                            None => {
                                let action = soldier_action(&s, &block, shooting);
                                // Giving ground: the walk played backwards; a
                                // shout plays from its start.
                                let seconds = if let Some(shout) = s.shouting {
                                    shout
                                } else if s.backing && action == Action::Walk {
                                    -seconds
                                } else {
                                    seconds
                                };
                                clips.pose(model, action, seconds).map(|p| (p, false))
                            }
                        };
                        let Some((pose, flat)) = pose else {
                            return;
                        };
                        // Knocked down, his feet go where the fall puts them.
                        let feet = match s.fight {
                            Some(puppets::Move::Down(f)) => {
                                models::feet_as_posed(models::down_depth(f))
                            }
                            _ => 0,
                        };
                        // Finishing a man on the ground, he bends over him
                        // to strike down; else, near the camera, he flinches
                        // or turns his head (not when turned here to a wall,
                        // a tool or a stand on a wall top).
                        let (turn, turn_weight) = match s.head {
                            _ if s.stoop > 0.0 => ([0, FINISH_PITCH], s.stoop),
                            Some(h) if lod == 0 && facing.dot(puppet_facing) > 0.99 => (
                                [
                                    look_angle(h.yaw, std::f32::consts::FRAC_PI_2),
                                    look_angle(h.pitch, std::f32::consts::FRAC_PI_4),
                                ],
                                view.detail(height, at),
                            ),
                            _ => ([0, 0], 0.0),
                        };
                        let turn_weight = (turn_weight * 255.0) as u8;
                        let (color, scale) = if s.general {
                            (
                                general_colour([wr, wg, wb], tone),
                                look.scale * GENERAL_SCALE,
                            )
                        } else {
                            ([wr, wg, wb, tone], look.scale)
                        };
                        buckets[model * LODS + lod].push(SoldierInstance {
                            pos: s.pos.to_array(),
                            facing: facing.to_array(),
                            frames: pose.rows,
                            flags: models::pose_flags(pose, highlight, flat),
                            color,
                            lift: s.lift,
                            scale,
                            feet,
                            look: turn,
                            look_weight: turn_weight,
                            ..Default::default()
                        });
                    },
                );
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
                .update_on(&block, r.owner, game_dt.as_secs_f32(), firm(r.owner), |s| {
                    instances.push(BoxInstance {
                        pos: s.pos.to_array(),
                        facing: s.facing.to_array(),
                        size,
                        color,
                        lift: s.lift,
                    });
                });
        }
        self.keep_arrows_in_the_fallen();
        if !self.commander && !viewing {
            self.push_corpses(&cull);
        }
        if let (Some(stage), Some(art)) = (&self.viewer, &mut self.art) {
            stage.push(art, &mut self.instances, color_of(&self.parties, HUMAN));
        }
        if let (Some(room), Some(art)) = (&self.showroom, &mut self.art) {
            let heightmap = &self.heightmap;
            let ground = |p: Vec2| heightmap.height_at(p.x, p.y);
            room.push(
                art,
                ground,
                self.puppets.time(),
                color_of(&self.parties, HUMAN),
            );
        }
        if let Some(art) = &mut self.art {
            art.finish();
        }
        if !viewing {
            self.push_arrows(now);
            self.push_siege(&cull);
            if !self.commander && self.art.as_ref().is_none_or(|a| a.crow.is_none()) {
                let hm = &self.heightmap;
                self.aftermath.crows(
                    |p| hm.drawn_height(p.x, p.y),
                    |at, radius| cull.sees(at, radius),
                    &mut self.instances,
                    gc_render::MAX_BOX_INSTANCES - BOX_ROOM_AFTER_CORPSES,
                );
            }
            self.push_routes();
        } else {
            // Blows told while the unit viewer is open show nothing.
            drop(self.siege.take_particles());
        }
        if self.lens && !self.minds.is_empty() && !viewing {
            let lenses = lens::lenses(&self.minds);
            let parties = &self.parties;
            let (world, viewer, reveal) = (&self.world, self.sandbox.side, self.reveal_all);
            lens::push(
                &lenses,
                &self.minds,
                lens::scale_for(lens_zoom),
                |p| color_of(parties, p),
                |h| reveal || world.sees(viewer, h),
                &mut self.instances,
            );
        }
        if self.show_blocked && !viewing {
            self.instances.extend_from_slice(&self.blocked_marks);
        }
        // Last, with what room is left: the field's oldest arrows are the
        // least missed.
        if !self.commander && !viewing {
            self.arrows
                .push(&cull, gc_render::MAX_BOX_INSTANCES, &mut self.instances);
        }

        if self.guide.is_some() {
            self.run_guide();
        } else if self.menu_open {
            self.run_menu();
        } else if self.match_started && self.showroom.is_none() {
            self.run_hud();
        } else {
            self.renderer.clear_ui();
        }

        self.fog.update(
            self.last_frame,
            &mut self.renderer,
            &self.world,
            self.sandbox.side,
            self.heightmap.tiles,
            !self.reveal_all,
        );
        self.land.update(
            self.last_frame,
            &mut self.renderer,
            &self.world,
            buildings::shown_to(self.sandbox.side, self.reveal_all),
            self.heightmap.tiles,
            &self.buildings,
        );
        // Dust, sparks and worn ground go with game time: still while paused
        // or behind the menu, slow in slow motion.
        let live = !self.paused && !(self.menu_open && self.online.is_none());
        let pace = if live {
            SPEEDS[self.speed] * self.film_pace()
        } else {
            0.0
        };
        self.scars.update(
            self.last_frame,
            &mut self.renderer,
            &self.world,
            (self.sandbox.side, self.reveal_all),
            self.heightmap.tiles,
            pace,
        );
        let fx_speed = pace * self.effects.time_scale();
        self.renderer.set_effects_speed(fx_speed);
        // Only soldiers the viewer sees scare the crows, so they give no
        // one away.
        let (world, viewer, reveal) = (&self.world, self.sandbox.side, self.reveal_all);
        self.aftermath.advance(
            dt.as_secs_f32() * fx_speed,
            (world.regiments.iter())
                .filter(|r| reveal || world.sees(viewer, r.handle))
                .map(|r| from_fx_vec(r.position)),
        );
        // Blows behind the menu, in Commander View (where the soldiers are
        // blocks) and under the unit viewer show nothing, and don't pile up.
        if viewing || self.commander || !live {
            drop(self.puppets.drain_impacts());
        } else {
            self.show_blows(&cull);
        }
        let density = self.menu.settings().graphics.effects_density();
        self.ambient.fell_demo(
            dt.as_secs_f32() * fx_speed,
            &mut self.camera.focus,
            &mut self.scenery,
        );
        // The demo lifts the fog too, so the villagers show.
        if self.ambient.hamlet_demo(&mut self.camera.focus) && self.online.is_none() {
            self.reveal_all = true;
        }
        let heightmap = &self.heightmap;
        let scenery = &self.scenery;
        self.ambient.felling.advance(
            dt.as_secs_f32() * fx_speed,
            &mut self.effects,
            |p| heightmap.height_at(p.x, p.y),
            |tile| scenery.occupied_at(tile),
        );
        let (view_focus, view_distance) = self.view_focus();
        self.atmosphere.follow(atmosphere::SkyView::of(&self.world));
        let mut look = self.atmosphere.look(
            dt.as_secs_f32(),
            heightmap,
            view_focus,
            view_distance,
            self.menu.settings().clarity,
        );
        // The unit viewer shows a model as made, not as the last rain left it.
        if self.viewer.is_some() {
            look.wet = 0.0;
            look.snow = 0.0;
        }
        self.renderer.set_look(look);
        // The water rises in a flood (docs/32-changing-land.md).
        let rise = f32::from(self.world.terrain().rise()) / gc_sim::terrain::HEIGHT_UNITS as f32;
        self.renderer
            .set_water_level(self.heightmap.water_level() + rise);
        // A battle that has just begun draws the bars; blows shook the screen
        // as they came in. Still while paused or behind the menu.
        // A rejoining player's catch-up replays old fights in a rush: they
        // neither shake the screen nor open a battle that began long ago.
        let deploying = self.world.deploying();
        if self.online.is_some() && self.menu.net().catching_up() {
            self.cinema.reset();
        } else if self.was_deploying && !deploying && self.match_started {
            self.cinema.battle_begins();
        }
        self.was_deploying = deploying;
        let settings = self.menu.settings();
        let picture = self.cinema.frame(
            if live { dt.as_secs_f32() } else { 0.0 },
            settings.reduced_shake,
            !settings.clarity,
            self.watching.is_some() && self.director.is_on(),
        );
        // Clarity keeps the picture plain: no softened edges, no shafts. The
        // shafts are for the ride camera, where the sky is in view.
        let plain = settings.clarity;
        let riding = self.ride.is_some();
        let tilt = if settings.tilt_shift && !plain && !riding && self.viewer.is_none() {
            1.0
        } else {
            0.0
        };
        let shafts =
            (riding && !plain && settings.graphics != gc_ui::settings::GraphicsPreset::Battery)
                .then(|| cinema::sun_shafts(view_proj, look.sun_dir()))
                .flatten();
        let picture = gc_render::Cinema {
            tilt,
            shafts: shafts.map_or(0.0, |(_, strength)| strength),
            sun: shafts.map_or([0.5; 2], |(uv, _)| uv),
            ..picture
        };
        self.renderer.set_cinema(picture);
        // Pollen, fireflies, leaves and whirlwinds, where the land has them;
        // not behind the menu or in the unit viewer, where nobody is looking
        // at the field.
        if live && !self.menu_open && self.viewer.is_none() {
            let sun = look.sun_dir().y;
            let (reveal, fog) = (self.reveal_all, &self.fog);
            self.ambient.touches.frame(
                dt.as_secs_f32() * fx_speed,
                view_focus,
                view_distance,
                sun,
                density,
                &self.heightmap,
                |tile| reveal || fog.sees_tile(tile),
                &mut self.effects,
            );
            self.ambient.touches.rises(
                dt.as_secs_f32() * fx_speed,
                view_focus,
                view_distance,
                sun,
                density,
                &self.heightmap,
                |tile| reveal || fog.sees_tile(tile),
                &mut self.effects,
            );
        }
        // Insects follow the effects clock, which stops while paused, so they
        // hang where they are; behind the menu, in the unit viewer and in
        // Commander View they are not drawn.
        if !self.menu_open && self.viewer.is_none() && !self.commander {
            self.ambient.insects.advance(
                self.puppets.time(),
                view_focus,
                view_distance,
                look.sun_dir().y,
                density,
                &self.heightmap,
            );
        } else {
            self.ambient.insects.hush();
        }
        // Smoke from hearths and lit windows on the buildings the viewer
        // sees, near enough to show.
        if live && !self.menu_open && self.viewer.is_none() && view_distance < ambient::bases::REACH
        {
            let sun = look.sun_dir().y;
            let dusk = ambient::touches::dusk_of(sun);
            let (reveal, fog) = (self.reveal_all, &self.fog);
            let shown = buildings::shown_to(self.sandbox.side, reveal);
            for s in (self.world.structures().into_iter()).flat_map(gc_sim::Structures::iter) {
                let Some(look) = self.siege.look(s.handle).filter(|_| shown(&s)) else {
                    continue;
                };
                let centre = look.centre();
                let at = Vec3::new(centre.x, look.base + look.top * 0.5, centre.y);
                if cull.on_screen(at, look.top + 6.0)
                    && (reveal || fog.sees_tile((centre.x.floor() as i32, centre.y.floor() as i32)))
                {
                    ambient::bases::emit(
                        look,
                        dt.as_secs_f32() * fx_speed,
                        dusk,
                        view_distance,
                        &mut self.effects,
                    );
                }
            }
        }
        // Workers walking between buildings, near the camera.
        {
            let showing = live
                && !self.menu_open
                && self.viewer.is_none()
                && view_distance < ambient::walkers::REACH;
            let homes = if showing {
                self.walker_homes(&cull, view_focus)
            } else {
                Vec::new()
            };
            let (hm, scenery) = (&self.heightmap, &self.scenery);
            let dry = hm.water_level() + ambient::walkers::SHORE;
            self.ambient.walkers.advance(
                dt.as_secs_f32() * fx_speed,
                &homes,
                |p| {
                    hm.height_at(p.x, p.y) > dry
                        && !scenery
                            .standing((p.x.floor() as i32, p.y.floor() as i32))
                            .any(|s| s.tree)
                },
                |a, b| (hm.height_at(b.x, b.y) - hm.height_at(a.x, a.y)).abs(),
            );
        }
        // Deer grazing near the camera, and bolting from soldiers in view.
        {
            let showing = !self.menu_open
                && self.viewer.is_none()
                && !self.commander
                && self.art.as_ref().is_some_and(|a| {
                    a.deer.is_some() || a.dove.is_some() || a.folk.is_some() || a.cart.is_some()
                });
            if showing {
                // Deer and doves stand on ground no building or tree covers, keep away
                // from buildings, and bolt or fly from the soldiers the viewer sees
                // (so unseen armies give nothing away).
                let (world, viewer, reveal) = (&self.world, self.sandbox.side, self.reveal_all);
                let threats: Vec<Vec2> = world
                    .regiments
                    .iter()
                    .filter(|r| reveal || world.sees(viewer, r.handle))
                    .map(|r| from_fx_vec(r.position))
                    .collect();
                let fights: Vec<Vec2> = world
                    .regiments
                    .iter()
                    .filter(|r| r.engaged && (reveal || world.sees(viewer, r.handle)))
                    .map(|r| from_fx_vec(r.position))
                    .collect();
                let shown = buildings::shown_to(viewer, reveal);
                let reach = ambient::wildlife::reach(view_distance)
                    + ambient::wildlife::CELL as f32
                    + ambient::wildlife::TAME;
                let towns: Vec<Vec2> = (world.structures().into_iter())
                    .flat_map(gc_sim::Structures::iter)
                    .filter(|s| shown(s))
                    .map(|s| buildings::centre_of(s.tile, s.width, s.depth))
                    .filter(|c| c.distance(view_focus) <= reach)
                    .collect();
                let step = if live {
                    dt.as_secs_f32() * fx_speed
                } else {
                    0.0
                };
                let clear = |p: Vec2| {
                    let tile = (p.x.floor() as i32, p.y.floor() as i32);
                    !scenery.occupied_at(tile) && !scenery.standing(tile).any(|s| s.tree)
                };
                let tame = |p: Vec2| {
                    towns
                        .iter()
                        .any(|c| c.distance(p) < ambient::wildlife::TAME)
                };
                self.ambient.wildlife.advance(
                    step,
                    view_focus,
                    view_distance,
                    heightmap,
                    clear,
                    tame,
                    &threats,
                );
                self.ambient.birds.advance(
                    step,
                    view_focus,
                    view_distance,
                    heightmap,
                    clear,
                    tame,
                    &threats,
                );
                let eyes = ambient::villagers::Eyes {
                    armies: &threats,
                    fights: &fights,
                };
                self.ambient.villagers.advance(
                    step,
                    view_focus,
                    view_distance,
                    &self.ambient.hamlets,
                    eyes,
                    |t| scenery.occupied_at(t),
                );
                self.ambient
                    .caravans
                    .advance(step, view_focus, view_distance, eyes, |t| {
                        scenery.occupied_at(t)
                    });
            } else {
                self.ambient.wildlife.hush();
                self.ambient.birds.hush();
                self.ambient.villagers.hush();
                self.ambient.caravans.hush();
            }
        }
        let struck = self.atmosphere.take_strike();
        // The thunder that arrives is heard even with the flash out of
        // sight, but not while paused, behind the offline menu or in the
        // unit viewer.
        let thunder_volume = self.atmosphere.extra(atmosphere::Extra::ThunderVolume);
        while let Some(loud) = self.atmosphere.take_thunder() {
            if let Some(sounds) = &self.sounds
                && self.viewer.is_none()
                && !self.paused
                && !(self.menu_open && self.online.is_none())
            {
                sounds.thunder(loud, thunder_volume);
            }
        }
        // Rain and snow, the one weather clearing and the one coming in;
        // nobody is looking at the field behind the menu or in the unit
        // viewer.
        if self.viewer.is_none() && !(self.menu_open && self.online.is_none()) {
            let thin = if self.menu.settings().clarity {
                0.4
            } else {
                1.0
            };
            let water = heightmap.water_level();
            let wind = self.atmosphere.wind();
            // The flash lights the whole sky, but the bolt itself is left
            // out under Clarity (a bright line that blinks in an instant).
            if struck && fx_speed > 0.0 && !self.menu.settings().clarity {
                let (reveal, fog) = (self.reveal_all, &self.fog);
                let side = heightmap.tiles as f32;
                self.effects.strike(
                    view_focus,
                    view_distance,
                    self.view_heading(),
                    |p| heightmap.height_at(p.x, p.y),
                    |p| {
                        p.cmpge(Vec2::ZERO).all()
                            && p.cmplt(Vec2::splat(side)).all()
                            && (reveal || fog.sees_tile((p.x.floor() as i32, p.y.floor() as i32)))
                    },
                );
            }
            // Mist lies on the rivers and pools: not under Clarity, nor from
            // the saddle (a puff round the eye would fill the screen), and
            // only where the viewer sees the water.
            if !self.menu.settings().clarity && self.ride.is_none() {
                let (reveal, fog) = (self.reveal_all, &self.fog);
                let side = heightmap.tiles as f32;
                let eye = self.camera.eye();
                self.effects.mist(
                    dt.as_secs_f32() * fx_speed,
                    density,
                    (self.atmosphere.mist(), MIST_TINT, wind),
                    (view_focus, view_distance, Vec2::new(eye.x, eye.z)),
                    |p| heightmap.height_at(p.x, p.y).max(water),
                    |p| {
                        p.cmpge(Vec2::splat(MIST_EDGE)).all()
                            && p.cmplt(Vec2::splat(side - MIST_EDGE)).all()
                            && heightmap.height_at(p.x, p.y) < water + MIST_BANK
                            && (reveal || fog.sees_tile((p.x.floor() as i32, p.y.floor() as i32)))
                    },
                );
            }
            for (weather, amount) in [self.atmosphere.leaving(), self.atmosphere.weather()] {
                self.effects.weather(
                    dt.as_secs_f32() * fx_speed,
                    density * thin,
                    (weather, amount, wind),
                    view_focus,
                    view_distance,
                    |p| heightmap.height_at(p.x, p.y).max(water),
                );
            }
        }
        // Woods on fire (the sim's burning tiles): a blaze on those the
        // viewer sees, near enough to matter, at most `MOST_FIRES_DRAWN` of
        // them (every so many across a big fire); its flames light the
        // ground.
        if fx_speed > 0.0 && !self.world.fires().is_empty() {
            let (reveal, fog) = (self.reveal_all, &self.fog);
            let fire_dt = dt.as_secs_f32() * fx_speed;
            let centre = |b: &gc_sim::Burn| Vec2::new(b.x as f32 + 0.5, b.y as f32 + 0.5);
            let near: Vec<_> = (self.world.fires().iter())
                .filter(|b| {
                    centre(b).distance_squared(view_focus) <= view_distance * view_distance
                        && (reveal || fog.sees_tile((b.x, b.y)))
                })
                .collect();
            // Thinned by a hash of the tile, so a big fire shows no stripes.
            let every = near.len().div_ceil(MOST_FIRES_DRAWN).max(1) as i32;
            for b in near {
                if (b.x * 7 + b.y * 13).rem_euclid(every) != 0 {
                    continue;
                }
                let at = centre(b);
                let tall = match b.growth {
                    gc_sim::Growth::Scrub => 0.8,
                    gc_sim::Growth::Thicket => 3.0,
                    _ => 2.4,
                };
                let ground = heightmap.height_at(at.x, at.y);
                self.effects
                    .blaze(glam::Vec3::new(at.x, ground, at.y), tall, fire_dt);
            }
        }
        let particles = self.effects.frame(
            dt.as_secs_f32() * fx_speed,
            density,
            view_focus,
            view_distance,
            |p| heightmap.height_at(p.x, p.y),
        );
        if self.effects.still() {
            self.renderer.clear_particles();
        }
        self.renderer.spawn_particles(&particles);
        // Lit windows and fires light the ground round them once it is
        // dark, but only those the viewer sees now (a point light, unlike a
        // particle, would show a fire in the fog), and not behind the unit
        // viewer or while a rejoining player's catch-up replays old fights.
        let mut glows = self.effects.take_glows();
        glows.extend(self.siege.take_glows());
        let (reveal, fog) = (self.reveal_all, &self.fog);
        let showing =
            self.viewer.is_none() && !(self.online.is_some() && self.menu.net().catching_up());
        if showing {
            lights::in_sight(&mut glows, |tile| reveal || fog.sees_tile(tile));
        } else {
            glows.clear();
        }
        let lights = self.lights.frame(
            dt.as_secs_f32() * fx_speed,
            glows,
            view_focus,
            view_distance,
            ambient::touches::dusk_of(look.sun_dir().y),
        );
        self.renderer.set_lights(&lights);
        let frame = FrameParams {
            view_proj,
            camera_pos: eye,
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
        // The panel names the selection's formation, which orders, routs and
        // control groups change.
        let formation = self.selection_formation();
        if formation != self.shown_formation {
            self.shown_formation = formation;
            self.sandbox.dirty = true;
        }
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
            // The light panel's values ease as the air does.
            if self.atmosphere.panel_open() {
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
        gc_platform::diag::set_tick(u64::from(self.world.tick_count()));
        for r in self.world.regiments.iter() {
            self.prev_positions[r.handle.index as usize] =
                Some((r.handle, from_fx_vec(r.position), angle_vec(r.facing)));
        }
        let t0 = Instant::now();
        profile::zone!("sim tick");
        tagged_alloc::scope(Tag::Sim, || match &mut self.recording {
            Some(rec) => rec.recorder.tick(&mut self.world, &self.pending),
            None => self.world.tick(&self.pending),
        });
        // A rejoining player replays the match: the sounds still note what
        // happened but stay quiet, and old ambushes raise no alert.
        let catching_up = self.online.is_some() && self.menu.net().catching_up();
        let listener = self.active_camera().listener();
        if let Some(sounds) = &mut self.sounds {
            let (world, viewer, reveal) = (&self.world, self.sandbox.side, self.reveal_all);
            let units = &self.units;
            tagged_alloc::scope(Tag::Audio, || {
                sounds.observe(
                    world,
                    listener,
                    !catching_up,
                    |h| reveal || world.sees(viewer, h),
                    |h| {
                        reveal
                            || (world.structures().and_then(|s| s.get(h)))
                                .is_some_and(|v| v.seen_by(viewer))
                    },
                    |unit| {
                        let i = usize::from(unit);
                        Some(sounds::Voice {
                            faction: *units.factions.get(i)?,
                            // Regiments that only shoot don't charge.
                            charges: units.stats.get(i)?.range.raw() == 0,
                        })
                    },
                );
            });
        }
        if !catching_up {
            self.alert_ambushes();
        }
        if self.watching.is_none() {
            self.note_losses();
        }
        self.leave_marks();
        self.fell_scenery();
        let took = t0.elapsed();
        self.stats.tick_time += took;
        self.stats.tick_max = self.stats.tick_max.max(took);
        self.stats.ticks += 1;
    }

    /// Point out, at the screen edge, ambushes sprung on this side and its
    /// regiments losing men or breaking in the last tick.
    fn note_losses(&mut self) {
        let side = self.sandbox.side;
        let tick = self.world.tick_count();
        for e in self.world.events() {
            let Event::Ambush(a) = e else { continue };
            if let Some(on) = self
                .world
                .regiments
                .get(a.victim)
                .filter(|r| r.owner == side)
            {
                self.hud.note(
                    gc_ui::alerts::AlertKind::Ambush,
                    from_fx_vec(on.position),
                    tick,
                );
            }
        }
        self.hud.watch(&self.world, side, self.world.events());
    }

    /// Tell the player about an ambush sprung on or by their side in the
    /// last tick (the latest, if several), with a drum roll and a horn.
    fn alert_ambushes(&mut self) {
        let side = self.sandbox.side;
        let name = |u: gc_sim::UnitId| self.units.names[u as usize].as_str();
        let whose = |p: PlayerId| party_name(&self.parties, p) + "'s";
        let news = self.world.events().iter().rev().find_map(|e| {
            let Event::Ambush(a) = e else { return None };
            let by = self.world.regiments.get(a.ambusher)?;
            let on = self.world.regiments.get(a.victim)?;
            let text = if !self.world.hostile(side, on.owner) {
                let how = if a.volley {
                    "shot from hiding at"
                } else {
                    "burst out of hiding on"
                };
                let theirs = if on.owner == side {
                    "your".to_string()
                } else {
                    whose(on.owner)
                };
                format!(
                    "AMBUSH! {} {} {how} {theirs} {}",
                    whose(by.owner),
                    name(by.unit),
                    name(on.unit)
                )
            } else if !self.world.hostile(side, by.owner) {
                let ours = if by.owner == side {
                    "Your".to_string()
                } else {
                    whose(by.owner)
                };
                format!(
                    "AMBUSH! {ours} {} caught {} {} off guard",
                    name(by.unit),
                    whose(on.owner),
                    name(on.unit)
                )
            } else {
                return None;
            };
            Some((text, from_fx_vec(on.position)))
        });
        if let Some((text, at)) = news {
            if let Some(s) = &self.sounds {
                s.ambush(at);
            }
            self.say(text);
        }
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
        let mut new_desync = None;
        for e in events {
            match e {
                gc_net::MatchEvent::Notice(text) => o.note(text),
                gc_net::MatchEvent::Desync { tick } => {
                    if o.desync.is_none() {
                        o.desync = Some(tick);
                        new_desync = Some(tick);
                    }
                }
                gc_net::MatchEvent::Ended(why) => {
                    logln!("grubcraft: {why}");
                    o.ended = Some(why);
                }
            }
            self.sandbox.dirty = true;
        }
        if let Some(tick) = new_desync {
            let line = self.save_desync_report(tick);
            if let Some(o) = &mut self.online {
                o.report = Some(line);
            }
        }
        let Some(o) = &mut self.online else {
            return 0;
        };
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
        // A player who rejoined replays the match so far as fast as a
        // frame's budget allows.
        let started = Instant::now();
        while turns < MAX_TICKS_PER_FRAME
            || (self.menu.net().catching_up() && started.elapsed() < CATCH_UP_BUDGET)
        {
            let Some(commands) = self.menu.net().next_turn(now) else {
                break;
            };
            self.pending = commands;
            self.step();
            let tick = self.world.tick_count();
            if tick.is_multiple_of(gc_net::HASH_EVERY) {
                // The recording hashes as often, so it pins a desync's tick.
                let hash = (self.recording.as_ref())
                    .and_then(|r| r.recorder.hash_now())
                    .unwrap_or_else(|| self.world.state_hash());
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
            // Gone, or its slot taken by another regiment, is not shooting.
            if let Some(r) = v
                .shooter
                .regiment()
                .and_then(|h| self.world.regiments.get(h))
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
        self.leave_ride();
        self.sandbox.dirty = true;
        self.menu.show(Screen::Main);
        self.menu.set_in_match(self.match_started);
        self.ui.reset();
        self.ui_input = UiInput::default();
        // Drop any half-made drag so it doesn't fire when the menu closes.
        self.input.left_down = None;
        self.input.right_down = None;
        self.input.middle_down = None;
        self.hud.reset();
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
        let (screen, scale) = self.ui_screen();
        let mut input = std::mem::take(&mut self.ui_input);
        input.mouse = self.ui_cursor();
        if let Some(listing) = &self.replay_list {
            let arrived = match listing.try_recv() {
                Ok(list) => Some(list),
                // The thread never started or died (the log says which).
                Err(mpsc::TryRecvError::Disconnected) => {
                    logln!("grubcraft: the replay list did not arrive");
                    Some(Vec::new())
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(list) = arrived {
                self.menu.set_replays(list);
                self.replay_list = None;
            }
        }
        self.ui.begin_frame(input, screen);
        if let Some(viewer) = &mut self.viewer {
            // The viewer's panels stand in for the menu while it is open.
            let escape = self.ui.input.escape;
            let leave =
                viewer.panel.frame(&mut self.ui, escape) == gc_ui::viewer::ViewerAction::Back;
            self.ui.end_frame();
            hud::upload_ui(&mut self.renderer, &mut self.ui_quads, &self.ui.draw, scale);
            if leave {
                self.close_viewer();
            }
            return;
        }
        let action = self.menu.frame(&mut self.ui);
        self.map_previews.poll(&mut self.menu);
        self.ui.end_frame();
        hud::upload_ui(&mut self.renderer, &mut self.ui_quads, &self.ui.draw, scale);
        match action {
            MenuAction::None => {}
            MenuAction::SettingsChanged => {
                let s = self.menu.settings();
                if let Some(sounds) = &mut self.sounds {
                    sounds.set_volumes(s.volume_master, s.volume_music);
                }
            }
            MenuAction::Resume => self.close_menu(),
            MenuAction::Quit => self.quit = true,
            MenuAction::StartSandbox => self.start_match(MatchKind::Sandbox { roster: false }),
            MenuAction::OpenViewer => self.open_viewer("start"),
            MenuAction::OpenGuide => self.open_guide(),
            MenuAction::ListReplays => {
                // A replay being saved now belongs in the list.
                self.save_recording();
                self.menu.replays_loading();
                // One read at a time: asking again waits for the one under way.
                if self.replay_list.is_none() {
                    self.replay_list = Some(replays::list_in_background(
                        replays::folder(),
                        self.unit_types(),
                        self.capacities,
                        self.data_hash,
                    ));
                }
            }
            MenuAction::WatchReplay(path) => match gc_replay::Replay::load(&path) {
                Ok(r) => {
                    match replays::unplayable(&r.info(), &self.unit_types(), self.capacities) {
                        None => {
                            logln!("grubcraft: watching {}", path.display());
                            self.start_match(MatchKind::Replay(Box::new(r)));
                        }
                        Some(why) => self.menu.tell(why),
                    }
                }
                Err(e) => self.menu.tell(format!("Can't open that replay: {e}")),
            },
            MenuAction::StartMatch(setup) => {
                logln!(
                    "grubcraft: starting {} on {} with {} players",
                    setup.settings.mode.name(),
                    setup.settings.map_name(),
                    setup.players.len()
                );
                self.start_match(MatchKind::Battle(setup));
            }
        }
    }

    /// Open the unit viewer over the menu, on the unit called `unit` (or the
    /// first one with a model, for "start").
    fn open_viewer(&mut self, unit: &str) {
        let models = match models::load() {
            Ok(m) if self.art.is_some() => m,
            result => {
                if let Err(e) = result {
                    logln!("grubcraft: unit viewer: {e}");
                }
                self.menu
                    .tell("The unit viewer needs the 3D models, which did not load.".into());
                return;
            }
        };
        self.leave_ride();
        let heightmap = &self.heightmap;
        let mut viewer = viewer::Viewer::new(
            &models,
            heightmap.tiles as f32,
            |p| heightmap.height_at(p.x, p.y),
            &self.camera,
        );
        if !unit.eq_ignore_ascii_case("start") && !viewer.panel.select_named(unit) {
            logln!("grubcraft: --viewer: no unit named {unit:?}; showing the first");
        }
        self.viewer = Some(viewer);
        // Nothing of the battlefield view carries over.
        self.commander = false;
        self.reset_ui_input();
    }

    /// Open the Guide over whatever is on screen.
    fn open_guide(&mut self) {
        if self.guide.is_none() {
            let mut guide = gc_ui::guide::Guide::new(guide_data::load());
            // New players start with the short illustrated page.
            guide.select_tab(gc_ui::guide::GuideTab::QuickStart);
            self.guide = Some(guide);
        }
        self.guide_scroll = 0.0;
        self.reset_ui_input();
    }

    /// Draw this frame's Guide and close it when asked.
    fn run_guide(&mut self) {
        let (screen, scale) = self.ui_screen();
        let mut input = std::mem::take(&mut self.ui_input);
        input.mouse = self.ui_cursor();
        self.ui.begin_frame(input, screen);
        let event = match &mut self.guide {
            Some(guide) => guide.frame(&mut self.ui),
            None => gc_ui::guide::GuideEvent::None,
        };
        self.ui.end_frame();
        hud::upload_ui(&mut self.renderer, &mut self.ui_quads, &self.ui.draw, scale);
        if event == gc_ui::guide::GuideEvent::Close {
            self.guide = None;
            self.reset_ui_input();
        }
    }

    /// Mouse, wheel and keys while the Guide is open. True when the event is
    /// dealt with: neither the menu nor the battle sees it.
    fn guide_event(&mut self, event: &WindowEvent) -> bool {
        let (screen, _) = self.ui_screen();
        let at = self.ui_cursor();
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.input.cursor = Some(*position);
                true
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if *button == MouseButton::Left {
                    match state {
                        ElementState::Pressed => self.ui_input.pressed = true,
                        ElementState::Released => self.ui_input.released = true,
                    }
                }
                true
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let rows = wheel_rows(&mut self.guide_scroll, delta);
                if rows != 0
                    && let Some(guide) = &mut self.guide
                {
                    guide.wheel(screen, at, rows);
                }
                true
            }
            WindowEvent::KeyboardInput { event: key, .. } => {
                // Key releases carry on to the battle, so a key held while the
                // Guide opened is not left stuck down.
                if key.state != ElementState::Pressed {
                    return false;
                }
                match key.physical_key {
                    PhysicalKey::Code(KeyCode::ArrowUp) => self.ui_input.up = true,
                    PhysicalKey::Code(KeyCode::ArrowDown) => self.ui_input.down = true,
                    PhysicalKey::Code(KeyCode::ArrowLeft) => self.ui_input.left = true,
                    PhysicalKey::Code(KeyCode::ArrowRight) => self.ui_input.right = true,
                    PhysicalKey::Code(KeyCode::Escape | KeyCode::KeyU) if !key.repeat => {
                        self.ui_input.escape = true;
                    }
                    _ => {}
                }
                true
            }
            _ => false,
        }
    }

    /// Forget a half-made click and anything typed or pressed.
    fn reset_ui_input(&mut self) {
        self.ui.reset();
        self.ui_input = UiInput::default();
    }

    /// Leave the unit viewer for the main menu, with the camera as it was.
    fn close_viewer(&mut self) {
        if let Some(viewer) = self.viewer.take() {
            self.camera = viewer.saved_camera().clone();
            self.camera.aspect = self.renderer.aspect_ratio();
        }
        self.reset_ui_input();
    }

    /// Mouse, wheel and keys while the unit viewer is open. True when the
    /// event is dealt with (the menu never sees it).
    fn viewer_event(&mut self, event: &WindowEvent) -> bool {
        let (screen, _) = self.ui_screen();
        let size = self.window.inner_size();
        let cursor = self.input.cursor.unwrap_or_default();
        let at = self.ui_cursor();
        let over_panel = gc_ui::viewer::over_panel(screen, at);
        let Some(viewer) = self.viewer.as_mut() else {
            return false;
        };
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.input.cursor = Some(*position);
                viewer.drag_to((position.x, position.y), (size.width, size.height));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let slot = match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                    _ => return true,
                };
                let down = *state == ElementState::Pressed;
                if slot == 0 {
                    // The panels' buttons use the left button.
                    if down {
                        self.ui_input.pressed = true;
                    } else {
                        self.ui_input.released = true;
                    }
                }
                if down && !over_panel {
                    viewer.press(slot, self.input.shift, (cursor.x, cursor.y));
                } else if !down {
                    viewer.release(slot);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // Over a panel the wheel scrolls it; over the model it zooms.
                if over_panel {
                    let rows = wheel_rows(&mut self.viewer_scroll, delta);
                    if rows != 0 {
                        viewer.panel.wheel(screen, at, rows);
                    }
                } else {
                    viewer.zoom(0.88f32.powf(wheel_lines(delta)));
                }
            }
            WindowEvent::PinchGesture { delta, .. } => viewer.zoom(1.0 - *delta as f32),
            WindowEvent::RotationGesture { delta, .. } => viewer.turn(delta.to_radians()),
            WindowEvent::KeyboardInput { event: key, .. } => {
                if key.state == ElementState::Pressed {
                    match key.physical_key {
                        PhysicalKey::Code(KeyCode::ArrowUp) => self.ui_input.up = true,
                        PhysicalKey::Code(KeyCode::ArrowDown) => self.ui_input.down = true,
                        PhysicalKey::Code(KeyCode::ArrowLeft) => self.ui_input.left = true,
                        PhysicalKey::Code(KeyCode::ArrowRight) => self.ui_input.right = true,
                        PhysicalKey::Code(KeyCode::Home | KeyCode::KeyR) => viewer.reset_view(),
                        _ => self.menu_key(key),
                    }
                }
            }
            _ => return false,
        }
        true
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
        // The regiment we rode with belongs to the last world.
        self.leave_ride();
        // A match started from the showroom's menu leaves it, and the field
        // gets its trees back.
        if self.showroom.take().is_some() {
            self.scenery = tagged_alloc::scope(Tag::Render, || {
                scenery::Scenery::new(&self.heightmap, self.seed, &self.scenery_models)
            });
        }
        // The last battle's recording is done.
        self.save_recording();
        self.recording = None;
        // A replay played with its own unit table; what was tuned before
        // comes back.
        if let Some(w) = self.watching.take() {
            self.units.stats = w.stats_before;
        }
        // Conquest plays with its own regiment types.
        let conquest = match &kind {
            MatchKind::Battle(setup) => setup.settings.mode == GameMode::Conquest,
            MatchKind::Replay(r) => r.conquest.is_some(),
            MatchKind::Sandbox { .. } => false,
        };
        // The sandbox may run on the roster table, though it is no Conquest;
        // a Field Battle always does, and so does a replay of one recorded
        // since.
        let roster = match &kind {
            MatchKind::Sandbox { roster } => *roster,
            MatchKind::Battle(_) => true,
            MatchKind::Replay(r) => r.header.unit_names == self.unit_names(true),
        };
        self.use_play(if conquest {
            Play::Conquest
        } else if matches!(kind, MatchKind::Sandbox { .. }) {
            if roster {
                Play::SandboxRoster
            } else {
                Play::Field
            }
        } else if roster {
            Play::FieldBattle
        } else {
            Play::Field
        });
        let mut stats_before = Vec::new();
        // A network match plays on the host's map, with the game data as
        // built (whatever the sandbox tuned stays in the sandbox).
        let networked = matches!(&kind, MatchKind::Battle(setup) if setup.seed.is_some());
        if !networked && self.menu.net().in_match() {
            // Starting something else leaves the network match.
            self.menu.net().leave();
        }
        if networked {
            self.units.stats.clone_from(&self.units.data_stats);
        }
        // A lobby match plays on the map the lobby picked, its world seeded
        // with the lobby's map seed (the same on every machine). A replay is
        // shown on the map it was played on, made again from its choice (and
        // a Field Battle's size and seed), and plays on the map it stored. A
        // described map opened with `--map` is for looking round in the
        // sandbox; the first battle or replay leaves it.
        let key = match &kind {
            MatchKind::Battle(setup) => Some((
                battle_map(&setup.settings, setup.players.len()),
                setup.settings.map_seed,
            )),
            MatchKind::Replay(r) => {
                let choice = r.header.map.map_or(0, usize::from);
                let key = if conquest {
                    MapKey::Conquest {
                        choice,
                        seed: gc_ui::lobby::conquest_map_seed(choice, r.seed),
                    }
                } else {
                    MapKey::Field(FieldMap {
                        choice,
                        tiles: r.map_tiles,
                        seed: r.seed,
                    })
                };
                Some((key, r.seed))
            }
            MatchKind::Sandbox { .. } => None,
        };
        if let Some((key, seed)) = key {
            self.seed = seed;
            if self.map_key != Some(key) {
                self.use_map(key);
            }
        }
        let mut map_note = None;
        if let MatchKind::Replay(r) = &kind {
            stats_before = std::mem::replace(&mut self.units.stats, r.units.clone());
        }
        self.online = networked.then(|| online::Online::new(self.menu.net().is_host()));
        let nav = tagged_alloc::scope(Tag::Map, || self.heightmap.sim_nav_costs());
        if let MatchKind::Replay(r) = &kind {
            let (shown, stored) = (self.heightmap.ground(), r.setup().ground);
            if r.map_tiles != self.heightmap.tiles
                || r.nav != nav
                || stored.heights != shown.heights
                || stored.water_level != shown.water_level
                || stored.cover != shown.cover
                || (!stored.growth.is_empty() && stored.growth != shown.growth)
            {
                map_note = Some(
                    "This replay's map was made by another build: the ground may look different from where the soldiers walk.",
                );
            }
        }
        let (teams, champions): (Vec<u8>, bool) = match &kind {
            MatchKind::Battle(setup) => (
                setup.players.iter().map(|s| s.team).collect(),
                battle::champions_allowed(setup),
            ),
            MatchKind::Sandbox { .. } | MatchKind::Replay(_) => (Vec::new(), false),
        };
        // The sky: the lobby's, or the replay's own; the sandbox's stands
        // still.
        let sky = match &kind {
            MatchKind::Battle(start) => battle::sky_of(&start.settings, self.seed),
            MatchKind::Replay(r) => r.sky.clone(),
            MatchKind::Sandbox { .. } => gc_replay::SkyRecord::default(),
        };
        let setup = MatchSetup {
            seed: self.seed,
            capacities: self.capacities,
            units: &self.units.stats,
            map_tiles: self.heightmap.tiles,
            nav: &nav,
            teams: &teams,
            ground: self.heightmap.ground(),
            champions,
            sky: sky.sky(),
            generals: true,
        };
        // A lobby Conquest match: what every machine makes its world with,
        // from the lobby's settings and the game data.
        let plan = match &kind {
            MatchKind::Battle(start) if conquest => Some(conquest::plan(
                &self.buildings,
                &self.data,
                &self.heightmap,
                &conquest::factions(start),
                start.settings.tempo,
                conquest::victories(start.settings.victory),
            )),
            _ => None,
        };
        self.world = tagged_alloc::scope(Tag::Sim, || match &kind {
            MatchKind::Replay(r) => r.new_world(),
            _ => World::new(&setup),
        });
        // A new match on the same map has all its trees again.
        self.scenery.regrow();
        self.ambient.felling.reset();
        // Before the recorder, which hashes the world as it starts.
        if let Some(plan) = &plan {
            let done = self.world.conquest(&plan.setup());
            debug_assert!(done, "conquest() on a world that has already begun");
        }
        // The sandbox has buildings to try. It is never recorded (only
        // `Battle` below makes a recorder).
        if kind.is_sandbox() {
            self.buildings.conquer(&mut self.world);
        }
        // Every battle is recorded from its first tick.
        if let MatchKind::Battle(start) = &kind {
            let dir = replays::folder();
            replays::prune(&dir, replays::KEEP - 1);
            let mode = start.settings.mode.name();
            let map = start.settings.map_name();
            let place = if networked {
                "over the network"
            } else {
                "on this machine"
            };
            let recorder = gc_replay::Recorder::new(
                &setup,
                &self.world,
                format!("{mode} on {map}, played {place}"),
            );
            let recorder = match plan {
                Some(plan) => recorder.conquest(plan),
                None => recorder,
            };
            self.recording = Some(Recording {
                // A network match hashes as often as the machines compare,
                // and keeps each system's hashes for a desync report.
                recorder: if networked {
                    recorder
                        .checkpoint_every(gc_net::HASH_EVERY)
                        .with_system_trail()
                } else {
                    recorder
                },
                path: replays::new_path(&dir, replays::now(), mode),
                saved_at: None,
            });
        }
        self.minds = tagged_alloc::scope(Tag::Ai, || {
            Minds::new(terrain_view::ai_terrain(&self.heightmap, &nav)).live()
        });
        self.matches += 1;
        self.drill = None;
        self.sandbox = Sandbox::new(kind.is_sandbox());
        self.hud.placer = kind.is_sandbox();
        self.hud.place_tab = u8::from(roster);
        self.hud.place_faction = 0;
        // Seeing through the woods stays in the sandbox.
        self.reveal_all &= kind.is_sandbox();
        self.fog.reset();
        self.cinema.reset();
        self.director.stop();
        self.was_deploying = false;
        self.land.reset();
        // The lobby's time of day lights the match, unless `--look` says.
        let time_of_day = match &kind {
            MatchKind::Battle(start) => start.settings.time_of_day,
            MatchKind::Sandbox { .. } | MatchKind::Replay(_) => Default::default(),
        };
        self.atmosphere.start(preset_of(time_of_day));
        self.scars.reset();
        self.parties = sandbox_parties();
        self.battle = None;
        self.speed = NORMAL_SPEED;
        self.paused = false;
        self.prev_positions.iter_mut().for_each(|p| *p = None);
        self.pending.clear();
        self.human_orders.clear();
        self.selected.clear();
        self.selected_structure = None;
        self.idle_from = None;
        self.hud.forget_readout();
        self.boxed.clear();
        self.control_groups = Default::default();
        self.last_group_key = None;
        // Regiment handles are reissued: a leftover pick or cool-down from
        // the last match would match the new match's regiments.
        self.formation_pick = None;
        self.shown_formation = None;
        if let Some(sounds) = &mut self.sounds {
            sounds.new_match();
        }
        self.attack_mark = None;
        self.sim_clock = Duration::ZERO;
        self.fresh_puppets();
        self.effects.clear();
        self.lights.clear();
        self.renderer.clear_particles();
        self.arrows = field::Arrows::new(field::MAX_STUCK);
        self.siege.clear();
        self.aftermath.clear();
        self.ambient.walkers.clear();
        self.ambient.wildlife.clear();
        self.ambient.birds.clear();
        self.ambient.villagers.clear();
        self.ambient.hamlets = ambient::Hamlets::new(&self.heightmap);
        self.ambient.caravans = ambient::Caravans::new(&self.heightmap, &self.ambient.hamlets);
        match kind {
            MatchKind::Battle(setup) if conquest => self.set_out_conquest(&setup),
            MatchKind::Battle(setup) => self.set_out_battle(&setup),
            MatchKind::Replay(r) => self.watch(*r, stats_before),
            MatchKind::Sandbox { .. } => {}
        }
        if let Some(note) = map_note {
            self.say(note.into());
        }
        if self.online.is_some() {
            // A joiner checks its map came out the same as the host's
            // before saying it is ready.
            let mine = gc_map::map_hash(&nav, &self.heightmap.ground());
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

    /// What the panel calls a lobby match.
    fn mode_name(&self) -> &'static str {
        if self.conquest_match() {
            "CONQUEST"
        } else {
            "FIELD BATTLE"
        }
    }

    /// New puppets for a new match, with the matched kills of the unit types
    /// it plays with.
    fn fresh_puppets(&mut self) {
        self.puppets = Puppets::new(self.capacities.regiments as usize, MAX_CORPSES);
        if let Some(art) = &self.art {
            self.puppets
                .set_kills(art.clips.kills_of(&self.units.models));
        }
    }

    /// Whether this is a Conquest match, with its economy, workers, HUD and
    /// build menu. The sandbox on the roster table plays with the same
    /// regiment types but none of that.
    fn conquest_match(&self) -> bool {
        self.play == Play::Conquest
    }

    /// Play with the regiment types of `play`. The table put away keeps what
    /// the sandbox tuned.
    fn use_play(&mut self, play: Play) {
        if self.play.roster_table() != play.roster_table() {
            std::mem::swap(&mut self.units, &mut self.other_units);
        }
        self.play = play;
    }

    /// Each table's regiment types by name, which a replay must match.
    fn unit_types(&self) -> replays::UnitTypes {
        replays::UnitTypes {
            field: self.unit_names(false).to_vec(),
            conquest: self.unit_names(true).to_vec(),
        }
    }

    /// The names of the regiment types of Conquest's table, or the Field
    /// Battle one's.
    fn unit_names(&self, conquest: bool) -> &[String] {
        if conquest == self.play.roster_table() {
            &self.units.names
        } else {
            &self.other_units.names
        }
    }

    /// Make the lobby map `key` names and put it on screen.
    fn use_map(&mut self, key: MapKey) {
        self.map_key = Some(key);
        self.heightmap = make_map(key);
        logln!(
            "grubcraft: switching to {} (seed {:x}, {} tiles)",
            key.name(),
            self.seed,
            self.heightmap.tiles
        );
        let hm = &self.heightmap;
        tagged_alloc::scope(Tag::Render, || {
            set_terrain(&mut self.renderer, hm);
        });
        // The renderer's land is bare again.
        self.land.reset();
        self.scars.reset();
        self.hud.set_terrain(hm);
        self.camera.map_size = hm.tiles as f32;
        let nav = hm.sim_nav_costs();
        self.blocked_marks = blocked_marks(&nav, hm.tiles);
        // A Conquest map's trees are the same in every match on it, and on
        // every machine; a Field Battle map's follow its seed.
        let trees = match key {
            MapKey::Field(k) => k.seed,
            MapKey::Conquest { choice, seed } => seed ^ choice as u64,
        };
        self.scenery = tagged_alloc::scope(Tag::Render, || {
            scenery::Scenery::new(&self.heightmap, trees, &self.scenery_models)
        });
    }

    /// Move the camera out to `default` tiles from the ground, or to where
    /// `--distance` said.
    fn start_zoom(&mut self, default: f32) {
        self.camera
            .set_distance(self.start_distance.unwrap_or(default));
    }

    /// Raise and deploy every side's army for a lobby Field Battle, and put
    /// a mind in charge of each AI player. Armies are a fixed mix from the
    /// faction's roster until production arrives (docs/06-game-modes.md).
    fn set_out_battle(&mut self, setup: &MatchStart) {
        let set = tagged_alloc::scope(Tag::Ai, || {
            battle::set_out(
                setup,
                &self.units.stats,
                &self.units.factions,
                &self.units.points,
                terrain_view::plateau_positions(self.heightmap.tiles),
                self.seed ^ self.matches.wrapping_mul(0x9E37_79B9_7F4A_7C15),
                self.online.as_ref().is_none_or(|o| o.host),
                &mut self.minds,
                &mut self.human_orders,
            )
        });
        for m in self.minds.iter() {
            logln!(
                "grubcraft: {} plays {}",
                m.summary(),
                party_name(&set.parties, m.party())
            );
        }
        self.seat(set.parties, set.me, setup);
        // Look over your army towards the enemy.
        let focus = set.my_start + set.my_forward * 18.0;
        self.camera.focus = focus;
        self.camera.focus_height = self.heightmap.height_at(focus.x, focus.y);
        self.start_zoom(120.0);
        // A joiner who starts in the north looks south, towards the enemy.
        if self.online.is_some() && set.my_forward.y > 0.0 {
            self.camera.face(Vec2::Y);
        }
    }

    /// Seat `parties` for a lobby match, this machine playing `me`, and
    /// note them in the recording.
    fn seat(&mut self, parties: Vec<Party>, me: PlayerId, setup: &MatchStart) {
        self.parties = parties;
        self.sandbox.side = me;
        self.battle = Some(Battle::new(Some(me)));
        let unit_names = self.units.names.clone();
        if let Some(rec) = &mut self.recording {
            let header = rec.recorder.header_mut();
            header.build = online::version();
            header.data_hash = self.data_hash;
            header.players = replays::players(&self.parties);
            header.recorded_by = Some(me);
            header.map = Some(setup.settings.map as u8);
            header.unit_names = unit_names;
        }
    }

    /// Seat every side of a lobby Conquest match and, on the host (or
    /// alone), put each one's start buildings and workers on the field and
    /// a Conquest mind in charge of each AI player (their orders travel in
    /// the turns like anyone's).
    fn set_out_conquest(&mut self, setup: &MatchStart) {
        let me = setup
            .local_player
            .min(setup.players.len().saturating_sub(1)) as PlayerId;
        self.seat(
            setup.players.iter().map(battle::party_of).collect(),
            me,
            setup,
        );
        if self.online.as_ref().is_none_or(|o| o.host) {
            self.human_orders.extend(conquest::opening(
                &self.data,
                &self.buildings,
                &self.heightmap,
                &self.world,
                &conquest::factions(setup),
            ));
            let roll = self.seed ^ self.matches.wrapping_mul(0x9E37_79B9_7F4A_7C15);
            tagged_alloc::scope(Tag::Ai, || {
                conquest_ai::seat(
                    setup,
                    &self.data,
                    &self.buildings,
                    &self.heightmap,
                    roll,
                    &mut self.minds,
                );
            });
            for m in self.minds.iter() {
                logln!(
                    "grubcraft: {} plays {}",
                    m.summary(),
                    party_name(&self.parties, m.party())
                );
            }
        }
        // Look over your start.
        let focus = conquest::player_starts(&self.heightmap, setup.players.len())
            .get(usize::from(me))
            .map_or(Vec2::splat(self.heightmap.tiles as f32 / 2.0), |&(x, y)| {
                Vec2::new(x as f32 + 0.5, y as f32 + 0.5)
            });
        self.camera.focus = focus;
        self.camera.focus_height = self.heightmap.height_at(focus.x, focus.y);
        self.start_zoom(80.0);
    }

    /// Set up watching `replay`: everyone's regiments in view, the sides
    /// named as they were, the camera over the armies.
    fn watch(&mut self, replay: gc_replay::Replay, stats_before: Vec<UnitStats>) {
        let parties = replays::parties(&replay.header);
        if !parties.is_empty() {
            self.parties = parties;
        }
        self.sandbox.placing = false;
        self.sandbox.side = replay.header.recorded_by.unwrap_or(0);
        self.battle = Some(Battle::new(None));
        self.reveal_all = true;
        // Look over the army of the side followed, as when it was played.
        let side = self.sandbox.side;
        let spawns: Vec<Vec2> = replay
            .turns
            .iter()
            .flat_map(|(_, c)| c)
            .filter_map(|c| match c.command {
                gc_sim::Command::SpawnRegiment { position, .. } if c.player == side => {
                    Some(from_fx_vec(position))
                }
                _ => None,
            })
            .collect();
        let focus = if spawns.is_empty() {
            Vec2::splat(self.heightmap.tiles as f32 / 2.0)
        } else {
            spawns.iter().sum::<Vec2>() / spawns.len() as f32
        };
        self.camera.focus = focus;
        self.camera.focus_height = self.heightmap.height_at(focus.x, focus.y);
        self.start_zoom(120.0);
        self.watching = Some(Watching {
            title: replays::title(&replay.info()),
            replay,
            stats_before,
            diverged_at: None,
        });
        // A replay is a film: the director frames it until the viewer moves.
        self.director.start(&self.camera);
    }

    /// After a replay tick: if it was a checkpoint and the state no longer
    /// matches the recording, pause and say so once. A replay from older
    /// rules plays on, but it is no longer the battle that was fought.
    fn check_replay_divergence(&mut self) {
        let tick = self.world.tick_count();
        let Some(w) = &mut self.watching else {
            return;
        };
        if w.diverged_at.is_some() {
            return;
        }
        if w.replay
            .checkpoint_at(tick)
            .is_none_or(|h| h == self.world.state_hash())
        {
            return;
        }
        w.diverged_at = Some(tick);
        self.paused = true;
        self.sim_clock = Duration::ZERO;
        self.say(format!(
            "This replay plays differently from {}: it was made with other rules. P carries on.",
            gc_ai::lens::clock(tick)
        ));
    }

    /// Once a frame: reopen a lost sound output, log what the GPU driver
    /// reported, and if the GPU is gone (nothing can be drawn any more) save
    /// the battle and leave.
    fn check_devices(&mut self) {
        for message in self.renderer.take_gpu_messages() {
            logln!("grubcraft: {message}");
        }
        if let Some(sounds) = &mut self.sounds {
            sounds.reopen_if_lost();
            // The menu hushes the wind and the fighting.
            sounds.set_live(self.match_started && !self.menu_open);
        }
        if self.renderer.device_lost() {
            // Quitting saves the battle (`App::exiting`).
            logln!("grubcraft: the GPU is gone; saving the battle and quitting");
            self.quit = true;
        }
    }

    /// Save the battle's recording as it stands.
    fn save_recording(&mut self) {
        let tick = self.world.tick_count();
        let Some(rec) = self.recording.as_mut().filter(|r| r.saved_at != Some(tick)) else {
            return;
        };
        rec.saved_at = Some(tick);
        if let Err(e) = replays::save(rec.recorder.replay(), &rec.path) {
            logln!(
                "grubcraft: could not save the replay {}: {e}",
                rec.path.display()
            );
        }
    }

    /// Save what this machine played of a network match as a desync
    /// report, returning a line saying where.
    fn save_desync_report(&mut self, tick: u32) -> String {
        let Some(rec) = &mut self.recording else {
            return "Nothing was recorded to report".into();
        };
        // The recording carries on as a replay once the report is out.
        let report = &rec.recorder.desync_report(tick);
        let who = report.header.recorded_by.map_or(0, |p| u32::from(p) + 1);
        let path = replays::new_path(
            &replays::desync_folder(),
            replays::now(),
            &format!("desync player{who}"),
        );
        let saved = replays::save(report, &path);
        let line = match saved {
            Ok(()) => format!(
                "Saved a desync report for the developers: {}",
                path.display()
            ),
            Err(e) => format!("Could not save the desync report ({e})"),
        };
        logln!("grubcraft: {line}");
        line
    }

    /// How the battle ended, once it has, and news of the champions and
    /// the generals.
    fn battle_news(&self) -> Option<String> {
        let battle = self.battle.as_ref()?;
        let lines: Vec<&str> = [
            &battle.outcome,
            &battle.champions,
            &battle.generals,
            &battle.throne,
        ]
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    /// Once one side alone has held the field long enough, say who won.
    fn judge_battle(&mut self) {
        let Some(battle) = &mut self.battle else {
            return;
        };
        let decided = battle.judge(&self.world, &self.parties);
        if decided {
            if let Some(outcome) = &battle.outcome {
                logln!("grubcraft: {outcome}");
            }
            self.sandbox.dirty = true;
        }
        if battle.watch_throne(&self.world, &self.parties) {
            self.sandbox.dirty = true;
        }
        if battle.herald(&self.world, &self.parties, &self.units.names) {
            if let Some(news) = &battle.champions {
                logln!("grubcraft: {news}");
            }
            self.sandbox.dirty = true;
        }
        if battle.muster(&self.world, &self.parties) {
            if let Some(news) = &battle.generals {
                logln!("grubcraft: {news}");
            }
            self.sandbox.dirty = true;
        }
        if decided {
            self.save_recording();
        }
    }

    /// Where you may deploy: a dashed amber ring on the ground.
    fn push_zone_ring(instances: &mut Vec<BoxInstance>, centre: Vec2, radius: f32) {
        let dashes = ((std::f32::consts::TAU * radius / ZONE_DASH_SPACING) as usize).max(8);
        for k in 0..dashes {
            let along = Vec2::from_angle(std::f32::consts::TAU * k as f32 / dashes as f32);
            instances.push(BoxInstance {
                pos: (centre + along * radius).to_array(),
                // Each dash lies along the ring.
                facing: along.to_array(),
                size: [ZONE_DASH, PLATE_HEIGHT, 0.25],
                color: [235, 150, 40, 255],
                lift: 0.01,
            });
        }
    }

    /// A ragged amber outline on the ground round a regiment out of order
    /// (pushing through friends): short dashes knocked askew, shrinking away
    /// as it re-forms (`d` from 1 down to 0). Boxes are opaque (their alpha
    /// is a glow), so the dashes fade by size, just above the selection
    /// plate so the two never fight for the same pixels.
    fn push_disorder_outline(
        instances: &mut Vec<BoxInstance>,
        centre: Vec2,
        facing: Vec2,
        width: f32,
        depth: f32,
        d: f32,
        seed: u32,
    ) {
        let half = (width / 2.0 + 0.4, depth / 2.0 + 0.4);
        let corners = buildings::block_corners(centre, facing, half);
        let length = DISORDER_DASH * (0.3 + 0.7 * d);
        let seed = seed.wrapping_mul(0x9E37_79B9);
        for side in 0..4 {
            let (a, b) = (corners[side], corners[(side + 1) % 4]);
            let edge = b - a;
            let along = edge.normalize_or(facing);
            let out = Vec2::new(along.y, -along.x);
            let dashes = (edge.length() / DISORDER_DASH_SPACING).max(1.0) as usize;
            for k in 0..dashes {
                let t = (k as f32 + 0.5) / dashes as f32;
                let dash = side * 256 + k;
                let skew = Vec2::from_angle(0.5 * d * spread_hash(seed, dash, 0)).rotate(along);
                instances.push(BoxInstance {
                    pos: (a + edge * t + out * (0.25 * d * spread_hash(seed, dash, 1))).to_array(),
                    facing: [skew.y, -skew.x],
                    size: [length, PLATE_HEIGHT, 0.12],
                    color: [235, 150, 40, DISORDER_GLOW],
                    lift: 0.01,
                });
            }
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
            let mut from = drawn_position(&self.prev_positions, self.alpha, &r);
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

    /// The fallen lie where they fell for the whole battle: in the death pose
    /// of their model, or flat in a darker shade of their team's colour
    /// (as a plain box) without one, or when far away. Only those the camera
    /// (or the sun, for shadows) can see are drawn. A body lying down gets a
    /// level of detail coarser than a man standing at the same spot, and
    /// past the coarsest a box, so the dead of a whole battle cost little to
    /// draw. The living take the renderer's room for soldiers first.
    fn push_corpses(&mut self, cull: &gc_render::Cull) {
        profile::zone!("corpses");
        let living = self
            .art
            .as_ref()
            .map_or(0, |a| a.buckets.iter().map(Vec::len).sum::<usize>());
        let mut room = gc_render::MAX_SOLDIER_INSTANCES.saturating_sub(living);
        for c in self.puppets.corpses() {
            let [r, g, b] = color_of(&self.parties, c.owner);
            let ground = self.heightmap.height_at(c.pos.x, c.pos.y);
            let model = self.units.models.get(c.unit as usize).copied().flatten();
            // Whether the model path has already found him on screen.
            let mut seen = false;
            if let (Some(model), Some(art)) = (model, self.art.as_mut()) {
                // The victim's half of the matched kill he fell to, or the
                // death clip, held on its last frame; models without one lie
                // down in their idle pose once the blow lands.
                // One killed lying down lies as he was, at the end of it.
                let age = if c.lying { f32::INFINITY } else { c.age };
                if let Some((pose, flat)) = art.clips.death(model, age, c.kill, c.look) {
                    let at = glam::Vec3::new(c.pos.x, ground, c.pos.y);
                    let Some(lod) = art.view.place(art.heights[model], at) else {
                        continue;
                    };
                    // Still standing, waiting for the blow, he is a man.
                    let lod = lod
                        + if age >= 0.0 && art.view.force.is_none() {
                            CORPSE_LOD_BIAS
                        } else {
                            0
                        };
                    // Out of soldier room, he is drawn as a box instead.
                    if lod < LODS && room > 0 {
                        room -= 1;
                        let look = models::look(c.look);
                        let ([wr, wg, wb], tone) =
                            aftermath::worn([r, g, b], look.tone, aftermath::DEAD_DIRT);
                        art.buckets[model * LODS + lod].push(SoldierInstance {
                            pos: c.pos.to_array(),
                            facing: c.facing.to_array(),
                            frames: pose.rows,
                            flags: models::pose_flags(pose, 0, flat),
                            color: [wr, wg, wb, tone],
                            lift: c.lift - 0.5 * c.sink,
                            scale: look.scale,
                            feet: models::dying_feet(age),
                            ..Default::default()
                        });
                        continue;
                    }
                    seen = true;
                }
            }
            if !seen && !cull.sees(glam::Vec3::new(c.pos.x, ground + 0.1, c.pos.y), 0.5) {
                continue;
            }
            // Leave room for what is drawn after the dead (arrows in the air,
            // routes, overlays).
            if self.instances.len() >= gc_render::MAX_BOX_INSTANCES - BOX_ROOM_AFTER_CORPSES {
                break;
            }
            self.instances.push(BoxInstance {
                pos: c.pos.to_array(),
                facing: c.facing.to_array(),
                size: [0.4, 0.14, SOLDIER_HEIGHT * 0.8],
                color: [r / 2, g / 2, b / 2, 0],
                lift: c.lift - 0.16 * c.sink,
            });
        }
    }

    /// Arrows that killed soldiers who fell this frame stay in their bodies.
    fn keep_arrows_in_the_fallen(&mut self) {
        for l in self.puppets.drain_lodged() {
            // On the ground the body is drawn on (`push_corpses`).
            let ground = self.heightmap.height_at(l.pos.x, l.pos.y);
            self.arrows.stick(l.pos, l.dir, ground, field::IN_BODY);
        }
    }

    /// Put the marks of the tick just played on the field, as the
    /// simulation told it (so a viewer who skipped ticks, catching up or
    /// seeking, leaves none for them): the arrows of a volley that came down
    /// stay in the ground, and in the soldiers it killed; a regiment wiped
    /// out leaves its last soldiers where they stood.
    fn leave_marks(&mut self) {
        let terrain = self.world.terrain();
        let in_wood = |p| terrain.cover(p) == gc_sim::Cover::Wood;
        let (heightmap, water) = (&self.heightmap, self.heightmap.water_level());
        // Arrows that come down in the water are lost, and so are those that
        // land on a building (the picture has no roof or wall to stick them
        // in, and a shaft showing through a wall would look wrong).
        // Only buildings the viewer is shown, so arrows don't trace hidden
        // ones.
        let world = &self.world;
        let shown = buildings::shown_to(self.sandbox.side, self.reveal_all);
        let dry = |p: Vec2| {
            let ground = heightmap.height_at(p.x, p.y);
            (ground >= water && !buildings::covers(world, shown, p)).then_some(ground)
        };
        // Where each volley came from, as the arrows in the air are drawn, so
        // they come down where they were seen to. One pass over the volleys
        // in a tick that loosed any.
        if self
            .world
            .events()
            .iter()
            .any(|e| matches!(e, Event::Loosed { .. }))
        {
            for v in self.world.volleys() {
                self.arrows.loosed(v.id, v.from, v.shots);
            }
        }
        // Buildings whose blows this tick were shown.
        let mut shown = Vec::new();
        let mut flooded = None;
        let (focus, distance) = self.view_focus();
        for e in self.world.events() {
            match *e {
                Event::Landed {
                    volley,
                    target,
                    at,
                    lost,
                } => {
                    if let Some(flight) = self.arrows.landed(volley, at, in_wood, dry) {
                        self.puppets.shot_down(target, flight.dir, lost);
                        if self.reveal_all || self.world.sees(self.sandbox.side, target) {
                            let p = from_fx_vec(at);
                            let ground = self.heightmap.height_at(p.x, p.y);
                            self.effects
                                .arrows_landed(glam::Vec3::new(p.x, ground, p.y), lost.max(2));
                        }
                    }
                }
                Event::Losses { regiment, lost }
                    if self.director.is_on() && self.world.regiments.get(regiment).is_some() =>
                {
                    let seen = self.reveal_all || self.world.sees(self.sandbox.side, regiment);
                    if let (true, Some(r)) = (seen, self.world.regiments.get(regiment)) {
                        self.director.note(
                            from_fx_vec(r.position),
                            f32::from(lost) * director::PER_LOSS,
                        );
                    }
                }
                Event::Morale {
                    regiment,
                    to: MoraleState::Routing,
                    ..
                } if self.director.is_on() => {
                    let seen = self.reveal_all || self.world.sees(self.sandbox.side, regiment);
                    if let (true, Some(r)) = (seen, self.world.regiments.get(regiment)) {
                        self.director.note(from_fx_vec(r.position), director::ROUT);
                    }
                }
                Event::Losses { regiment, .. } if self.world.regiments.get(regiment).is_none() => {
                    let owner = self.puppets.wiped(regiment);
                    // A viewer catching up on a match plays all of its
                    // fights at once; they are not new.
                    if let (Some(owner), Some((_, at, facing)), false) = (
                        owner,
                        self.prev_positions[regiment.index as usize].filter(|p| p.0 == regiment),
                        self.online.is_some() && self.menu.net().catching_up(),
                    ) {
                        let dead = (self.puppets.corpses())
                            .filter(|c| c.pos.distance(at) <= aftermath::DEAD_RADIUS)
                            .count();
                        let tint = color_of(&self.parties, owner);
                        self.aftermath.wiped(regiment, at, facing, tint, dead);
                    }
                }
                Event::Contact { regiment, foe } => {
                    let seen = self.reveal_all || self.world.sees(self.sandbox.side, regiment);
                    if let (true, Some(r), Some(f)) = (
                        seen,
                        self.world.regiments.get(regiment),
                        self.world.regiments.get(foe),
                    ) && r.charge > 0
                        // Two regiments charging each other both report it:
                        // one burst.
                        && !(f.charge > 0 && foe.index < regiment.index)
                    {
                        let p = from_fx_vec(r.position).lerp(from_fx_vec(f.position), 0.5);
                        let ground = self.heightmap.height_at(p.x, p.y);
                        self.effects.charge_hit(glam::Vec3::new(p.x, ground, p.y));
                        self.cinema.blow(p, focus, distance, cinema::CHARGE_HIT);
                        self.director.note(p, director::CHARGE_HIT);
                    }
                }
                Event::Ambush(a) => {
                    // Your own regiments are always seen by you.
                    let seen = self.reveal_all || self.world.sees(self.sandbox.side, a.victim);
                    if let (true, Some(r)) = (seen, self.world.regiments.get(a.victim)) {
                        let at = from_fx_vec(r.position);
                        self.cinema.blow(at, focus, distance, cinema::AMBUSH);
                        self.director.note(at, director::AMBUSH);
                    }
                }
                Event::Struck { structure, by } => {
                    let side = self.sandbox.side;
                    let seen = self.reveal_all || self.world.sees(side, by);
                    // A blow under fog on a building not your side's is
                    // neither seen nor heard.
                    let friendly = (self.siege.look(structure))
                        .is_some_and(|l| !self.world.hostile(side, l.owner));
                    let hm = &self.heightmap;
                    let ground = |p: Vec2| hm.drawn_height(p.x, p.y);
                    let a = attacker_of(&self.world, &self.units, by, seen, ground);
                    if let Some(a) = a.filter(|_| seen || friendly) {
                        self.siege.struck(structure, a);
                        shown.push(structure);
                    }
                }
                // A wall's twist is shown where the regiment it answered is
                // seen, or for a wall of your side's.
                Event::Twist {
                    structure,
                    twist,
                    on,
                } => {
                    let side = self.sandbox.side;
                    let friendly = (self.siege.look(structure))
                        .is_some_and(|l| !self.world.hostile(side, l.owner));
                    let on = on.and_then(|h| self.world.regiments.get(h));
                    let seen = on.as_ref().is_some_and(|r| self.world.sees(side, r.handle));
                    if seen || friendly || self.reveal_all {
                        let front = on.map(|r| from_fx_vec(r.position));
                        self.siege.twisted(structure, twist, front);
                    }
                }
                // Nor is its fall, unless the blow that felled it was.
                Event::Fallen { structure } => {
                    let side = self.sandbox.side;
                    let look = self.siege.look(structure);
                    let friendly = look.is_some_and(|l| !self.world.hostile(side, l.owner));
                    if self.reveal_all || friendly || shown.contains(&structure) {
                        if let Some(l) = look {
                            self.cinema
                                .blow(l.centre(), focus, distance, cinema::BUILDING_FELL);
                            self.director.note(l.centre(), director::BUILDING_FELL);
                        }
                        self.siege.fallen(structure);
                    } else {
                        self.siege.forget(structure);
                    }
                }
                Event::Flooded { closed } => flooded = Some(closed),
                _ => {}
            }
        }
        match flooded {
            Some(true) => self.say("The rivers are in flood: the fords can't be crossed".into()),
            Some(false) => self.say("The rivers have gone down: the fords are open again".into()),
            None => {}
        }
        for b in self.world.blows() {
            self.puppets.struck(b);
        }
    }

    /// Sparks and wounds for the blows soldiers landed on each other since
    /// the last frame, those the camera sees (the fog hides the rest, as it
    /// hides all particles), at most [`BLOWS_SHOWN`] a frame.
    fn show_blows(&mut self, cull: &gc_render::Cull) {
        let hm = &self.heightmap;
        let on_screen = self.puppets.drain_impacts().filter_map(|i| {
            let at = glam::Vec3::new(i.pos.x, hm.height_at(i.pos.x, i.pos.y), i.pos.y);
            cull.on_screen(at, 1.0).then_some((i, at))
        });
        for (i, at) in on_screen.take(BLOWS_SHOWN) {
            let unit = usize::from(i.victim);
            let (Some(stats), Some(faction)) =
                (self.units.stats.get(unit), self.units.factions.get(unit))
            else {
                continue;
            };
            let style = match faction {
                gc_ai::Faction::EmberClans => HitStyle::Flesh,
                gc_ai::Faction::Brood => HitStyle::Ichor,
                gc_ai::Faction::Choir => HitStyle::Crystal,
                // Only the dead, who never rout, bleed ghost-light.
                gc_ai::Faction::CarrionConcord if stats.morale == 0 => HitStyle::Ghost,
                gc_ai::Faction::CarrionConcord => HitStyle::Flesh,
            };
            let armoured = stats.shield || stats.melee_defence >= ARMOURED_DEFENCE;
            let chest = at + glam::Vec3::Y * (SOLDIER_HEIGHT * 0.6);
            self.effects.blow(chest, i.dir, i.hit, style, armoured);
        }
    }

    /// The sieges on screen (see [`siege`]): shots at buildings, chunks,
    /// stuck arrows and rubble as boxes, and dust, smoke, fire and sparks as
    /// particles. Their sounds play here too.
    fn push_siege(&mut self, cull: &gc_render::Cull) {
        let shown = buildings::shown_to(self.sandbox.side, self.reveal_all);
        let standing = self
            .world
            .structures()
            .into_iter()
            .flat_map(|all| all.iter().filter(move |v| shown(v)).map(|v| v.handle));
        let hm = &self.heightmap;
        self.siege.push(
            |p| hm.drawn_height(p.x, p.y),
            gc_render::MAX_BOX_INSTANCES - BOX_ROOM_AFTER_CORPSES,
            &mut self.instances,
        );
        self.siege
            .emit_damage(standing, |at, radius| cull.sees(at, radius));
        let catching_up = self.online.is_some() && self.menu.net().catching_up();
        let particles = self.siege.take_particles();
        // Replayed blows would burst out all at once at stale spots.
        if !catching_up {
            self.renderer.spawn_particles(&particles);
        }
        for noise in self.siege.drain_noises() {
            if let Some(sounds) = self.sounds.as_ref().filter(|_| !catching_up) {
                sounds.siege(noise);
            }
        }
    }

    /// Volleys in flight: a few arrows each, arcing from the archers to where
    /// they aimed, at `now` (the simulation's time in ticks, interpolated).
    /// Arrows loosed out of a wood or into one stick in the branches as often
    /// as the simulation's trees catch them. Only a picture; the simulation
    /// decides what they hit. Those that come down stay (`leave_marks`).
    fn push_arrows(&mut self, now: f32) {
        let terrain = self.world.terrain();
        for v in self.world.volleys() {
            // A tower's arrows leave from the top of its model and come down
            // to the usual height by the end of the flight.
            let start = v
                .shooter
                .tower()
                .and_then(|h| self.world.structures()?.get(h))
                .and_then(|s| {
                    let drawn = self.art.as_ref().and_then(|art| {
                        let look = art.looks.model_of(&s)?;
                        Some(art.looks.top(look))
                    });
                    let role = self.buildings.stats.get(usize::from(s.building));
                    drawn.or(role.map(|b| buildings::height_of(b.role)))
                })
                .map(|top| top.max(ARROW_HEIGHT));
            let flight_ticks = (v.impact_tick - v.launch_tick).max(1) as f32;
            let flown = ((now - v.launch_tick as f32) / flight_ticks).clamp(0.0, 1.0);
            let flight = field::Flight::new(v.launch_tick, v.from, v.aim, |p| {
                terrain.cover(p) == gc_sim::Cover::Wood
            });
            for k in 0..field::ARROWS_PER_VOLLEY.min(v.shots as usize) {
                // A caught arrow stops in the branches over the archers or
                // over its target, and hangs there until the volley lands.
                let t = flown.min(flight.stop(k));
                self.instances.push(BoxInstance {
                    pos: flight.at(k, t).to_array(),
                    facing: flight.dir.to_array(),
                    size: field::ARROW_SIZE,
                    color: field::ARROW_COLOR,
                    lift: start.map_or(ARROW_HEIGHT, |top| top + (ARROW_HEIGHT - top) * t)
                        + flight.arc(t),
                });
            }
        }
    }

    fn update_camera(&mut self, dt: f32) {
        // The showroom's camera stays on the models, whatever the window or
        // the player does.
        if let Some(room) = &self.showroom {
            let heightmap = &self.heightmap;
            room.aim(&mut self.camera, |p| heightmap.height_at(p.x, p.y));
            return;
        }
        // The unit viewer's camera orbits the model on show.
        if let Some(viewer) = &mut self.viewer {
            viewer.update(dt, &mut self.camera);
            return;
        }
        if self.director.is_on() && !self.menu_open {
            // (Riding stops it: `toggle_ride`.)
            let (world, side, reveal) = (&self.world, self.sandbox.side, self.reveal_all);
            self.director
                .step(dt, &mut self.camera, || see_the_field(world, side, reveal));
        }
        if self.menu_open && !self.match_started {
            // A slow sway behind the main menu.
            self.camera.yaw = 0.3 * (self.started.elapsed().as_secs_f32() * 0.12).sin();
        }
        if self.menu_open {
            // The menu eats key releases; don't resume with a key stuck down.
            self.input.turn = [false; 2];
        } else {
            let [l, r] = self.input.turn;
            // Turning is a move of the view: back to the map.
            if l != r {
                self.camera_mut()
                    .rotate((f32::from(u8::from(r)) - f32::from(u8::from(l))) * KEY_TURN_RATE * dt);
            }
        }
        let mut dx = 0.0;
        let mut dy = 0.0;
        let [up, down, left, right] = if self.menu_open {
            [false; 4]
        } else {
            self.input.arrows
        };
        if up {
            dy += 1.0;
        }
        if down {
            dy -= 1.0;
        }
        if left {
            dx -= 1.0;
        }
        if right {
            dx += 1.0;
        }
        if let (Some(c), true, false) = (self.input.cursor, self.input.focused, self.menu_open) {
            let size = self.window.inner_size();
            if c.x <= EDGE_SCROLL_PX {
                dx -= 1.0;
            }
            if c.x >= f64::from(size.width) - EDGE_SCROLL_PX {
                dx += 1.0;
            }
            if c.y <= EDGE_SCROLL_PX {
                dy += 1.0;
            }
            // The bar fills the bottom edge during a match.
            if c.y >= f64::from(size.height) - EDGE_SCROLL_PX && !self.match_started {
                dy -= 1.0;
            }
        }
        if dx != 0.0 || dy != 0.0 {
            self.camera_mut().pan(dx * dt * 1.2, dy * dt * 1.2);
        }
        // Riding with a regiment is a close view whatever the bird's-eye
        // camera's distance was: soldiers stay.
        let limit = if self.commander {
            COMMANDER_OUT
        } else {
            COMMANDER_IN
        };
        let commander = self.ride.is_none() && self.camera.distance > limit;
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
            ms(self.stats.tick_time) / f64::from(self.stats.ticks)
        } else {
            0.0
        };
        let ai_avg = if self.stats.ticks > 0 {
            ms(self.stats.ai_time) / f64::from(self.stats.ticks)
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
        let dropped = if rs.dropped > 0 {
            format!("   {} NOT DRAWN: over capacity", rs.dropped)
        } else {
            String::new()
        };
        let info = self.renderer.adapter_info();
        let text = format!(
            "GRUBCRAFT DEV OVERLAY                          ` or F11 hides\n\
             frame   {fps:5.0} fps   {frame_avg:6.2} ms avg   {:6.2} ms worst\n\
             sim     tick {:<7} {tick_avg:6.3} ms avg   {:6.3} ms worst\n\
             routes  {} portals   {} waiting\n\
             ai      {} computer players   {ai_avg:6.3} ms a tick\n\
             render  {} draw calls   {}/{} chunks   {} boxes   {} soldiers   {}k triangles{dropped}\n\
             field   {} fallen   {} arrows stuck\n\
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
            self.puppets.corpse_count(),
            self.arrows.len(),
            mb(tagged_alloc::total_live_bytes()),
            mb(self.renderer.gpu_bytes() as usize),
            heap_tags.join("  "),
            self.data_label,
            self.sounds
                .as_ref()
                .map_or("off".to_string(), sounds::Sounds::overlay_line),
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
        } else if self.showroom.is_some() {
            String::new()
        } else {
            self.sandbox_panel()
        };
        if self.lens && !self.menu_open {
            let lenses = lens::lenses(&self.minds);
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(&lens::panel(&lenses, |p| party_name(&self.parties, p)));
        }
        if self.atmosphere.panel_open() && !self.menu_open {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(&self.atmosphere.panel_text());
        }
        if let Some(dev) = dev {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(dev);
        }
        let scale = self.window.scale_factor() as f32;
        // Below Conquest's top bar.
        let top = if self.match_started && self.showroom.is_none() {
            gc_ui::hud::TOP_BAR_HEIGHT as f32 * self.ui_scale()
        } else {
            0.0
        };
        self.renderer
            .set_overlay((!text.is_empty()).then_some(text.as_str()), scale, top);
        self.sandbox.dirty = false;
    }

    /// Tab: command the next side. The selection belongs to the old side,
    /// so it is dropped; with the drill running, it leaves the new side alone.
    fn switch_side(&mut self) {
        let sides = self.parties.len().max(1) as PlayerId;
        self.set_side((self.sandbox.side + 1) % sides);
    }

    /// Command (and place for) `side`, if the sandbox has it.
    fn set_side(&mut self, side: PlayerId) {
        if usize::from(side) >= self.parties.len().max(1) {
            return;
        }
        self.sandbox.side = side;
        // The ridden regiment may be one the new side can't see.
        self.leave_ride();
        self.selected.clear();
        self.selected_structure = None;
        if let Some(drill) = &mut self.drill {
            drill.skip_player = Some(self.sandbox.side);
        }
        self.sandbox.dirty = true;
    }

    /// I: hand the side you command to the computer, raise its level
    /// (Recruit, Veteran, General, Warlord), or take the side back.
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
            gc_ai::personality::pick(faction, self.world.tick_count() ^ u32::from(side))
        });
        let mind = Mind::new(MindConfig {
            party: side,
            difficulty: level,
            personality,
            seed: self.seed ^ (self.matches << 8) ^ u64::from(side),
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
        const N: usize = gc_ai::Faction::ALL.len();
        let mut counts = [0u32; N];
        for r in self.world.regiments.iter().filter(|r| r.owner == side) {
            if let Some(f) = self.units.factions.get(r.unit as usize) {
                counts[f.index()] += 1;
            }
        }
        let best = (0..N).max_by_key(|&f| (counts[f], N - f)).unwrap_or(0);
        if counts[best] == 0 {
            gc_ai::Faction::ALL[usize::from(side) % N]
        } else {
            gc_ai::Faction::ALL[best]
        }
    }

    /// [ and ]: the previous or next type of whatever is being placed.
    fn choose_type(&mut self, step: i32) {
        if self.sandbox.building_tool {
            self.sandbox.building = self.buildings.step(self.sandbox.building, step);
            self.sandbox.dirty = true;
        } else {
            self.choose_unit(step);
        }
    }

    /// [ and ]: the previous or next unit type, for placing and tuning.
    fn choose_unit(&mut self, step: i32) {
        let types = self.units.stats.len() as i32;
        let mut unit = i32::from(self.sandbox.unit);
        // On the roster table, step past the troops the Place panel leaves
        // out (workers, the caravan): they need Conquest's economy.
        for _ in 0..types.max(1) {
            unit = (unit + step).rem_euclid(types.max(1));
            let left_out = self.play == Play::SandboxRoster
                && self
                    .data
                    .conquest_units
                    .get(unit as usize)
                    .is_some_and(roster::left_out);
            if !left_out {
                break;
            }
        }
        self.sandbox.unit = unit as UnitId;
        self.sandbox.dirty = true;
    }

    /// Where a regiment placed now would stand, and its facing: under the
    /// cursor, facing away from the camera.
    fn placement(&self) -> Option<(Vec2, Vec2)> {
        let at = self.ground_under_cursor()?;
        // Facing away from the eye: the way the camera looks along the ground.
        let look = self.active_camera().forward();
        let facing = Vec2::new(look.x, look.z).normalize_or(Vec2::NEG_Y);
        Some((at, facing))
    }

    fn place_regiment(&mut self) {
        let Some((at, facing)) = self.placement() else {
            return;
        };
        if !self.world.nav().walkable(self.sandbox.side, fx_vec(at)) {
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

    /// K: place buildings instead of regiments, or stop.
    fn toggle_building_tool(&mut self) {
        if self.world.structures().is_none() {
            self.say("Buildings can only be placed in the sandbox and Conquest.".into());
            return;
        }
        self.sandbox.building_tool = !self.sandbox.building_tool;
        self.sandbox.placing = false;
        self.hud.armed = None;
        self.sandbox.dirty = true;
    }

    /// Where building `id` would stand with the footprint centred on the
    /// cursor, and why it couldn't: the sandbox's check, or in Conquest
    /// whether a worker of yours could lay it.
    fn building_placement(
        &self,
        id: BuildingId,
    ) -> Option<(BuildingStats, (i32, i32), Option<&'static str>)> {
        let at = self.ground_under_cursor()?;
        let stats = *self.buildings.stats.get(usize::from(id))?;
        let turn = self.sandbox.building_turn;
        let (width, depth) = stats.size(turn);
        let tile = buildings::tile_for(at, width, depth);
        // Conquest asks what a worker may lay; the sandbox only the ground.
        let why = if self.conquest_match() {
            self.world.can_lay(self.sandbox.side, id, tile, turn)
        } else {
            self.world.can_build(id, tile, turn)
        }
        .err()
        .map(buildings::why);
        Some((stats, tile, why))
    }

    fn place_building(&mut self) {
        let Some((_, tile, why)) = self.building_placement(self.sandbox.building) else {
            return;
        };
        if let Some(why) = why {
            self.say(why.into());
            return;
        }
        self.human_orders.push(PlayerCommand {
            player: self.sandbox.side,
            command: gc_sim::Command::SpawnStructure {
                building: self.sandbox.building,
                tile: (tile.0 as i16, tile.1 as i16),
                turn: self.sandbox.building_turn,
            },
        });
        self.ui_sound(sounds::UiSound::Move);
    }

    /// Turn the building being placed `quarter_turns` clockwise (negative
    /// the other way).
    fn turn_building(&mut self, quarter_turns: i32) {
        if quarter_turns == 0 {
            return;
        }
        let turns = i32::from(gc_sim::structures::TURNS);
        let turn = i32::from(self.sandbox.building_turn) + quarter_turns;
        self.sandbox.building_turn = turn.rem_euclid(turns) as u8;
        self.sandbox.dirty = true;
    }

    /// The building being placed (one K would place, or a worker lay), if
    /// any.
    fn laying(&self) -> Option<BuildingId> {
        match self.hud.armed {
            Some(hud::Armed::Lay(id)) => Some(id),
            Some(hud::Armed::Wall(..)) => None,
            _ => self.sandbox.building_tool.then_some(self.sandbox.building),
        }
    }

    /// The building K would place, or a worker lay, ghosted under the cursor.
    fn push_building_ghost(&mut self) {
        let Some(id) = self.laying() else {
            return;
        };
        let Some((stats, tile, why)) = self.building_placement(id) else {
            return;
        };
        let colour = color_of(&self.parties, self.sandbox.side);
        let fit = self.fit_of(&stats, tile, why);
        let hm = &self.heightmap;
        buildings::push_ghost(
            &stats,
            tile,
            self.sandbox.building_turn,
            fit,
            colour,
            |x, z| hm.height_at(x, z).max(hm.water_level()),
            &mut self.instances,
        );
    }

    /// Every standing building as a box, the selected one with a rim, and
    /// every gathering spot.
    fn push_buildings(&mut self) {
        let haste_glow = self.haste_glow();
        let look = self.active_camera().forward();
        let across = Vec2::new(look.x, look.z).normalize_or(Vec2::NEG_Y);
        let (hm, parties, art) = (&self.heightmap, &self.parties, &self.art);
        let siege = &self.siege;
        let ground = |x, z| hm.height_at(x, z).max(hm.water_level());
        buildings::push(
            &self.world,
            &self.buildings,
            ground,
            |p| color_of(parties, p),
            buildings::shown_to(self.sandbox.side, self.reveal_all),
            |s| {
                let art = art.as_ref()?;
                Some(art.looks.top(art.looks.model_of(s)?))
            },
            |h| siege.flash(h),
            across,
            &mut self.instances,
        );
        if let Some(h) = self.picked_structure() {
            buildings::push_rim(&self.world, h, ground, &mut self.instances);
            let colour = color_of(parties, self.sandbox.side);
            buildings::push_rally(&self.world, h, colour, &mut self.instances);
        }
        if ![scenery::Model::GrubSeep, scenery::Model::MarrowVein]
            .iter()
            .all(|&m| self.scenery.draws(m))
        {
            buildings::push_nodes(&self.world, &mut self.instances);
        }
        buildings::push_veins(
            &self.world,
            self.sandbox.side,
            self.reveal_all,
            |p| color_of(parties, p),
            &mut self.instances,
        );
        buildings::push_tuning(
            &self.world,
            self.sandbox.side,
            self.reveal_all,
            &mut self.instances,
        );
        buildings::push_chords(
            &self.world,
            self.sandbox.side,
            self.reveal_all,
            &mut self.instances,
        );
        // The fields of power stand out while a building that needs one is
        // being placed.
        let lit = self.laying().is_some_and(|id| {
            (self.buildings.stats.get(usize::from(id)))
                .is_some_and(|s| s.flags & gc_sim::structures::NEEDS_POWER != 0)
        });
        buildings::push_fields(
            &self.world,
            self.sandbox.side,
            self.reveal_all,
            lit,
            &mut self.instances,
        );
        buildings::push_haste(
            &self.world,
            buildings::shown_to(self.sandbox.side, self.reveal_all),
            haste_glow,
            &mut self.instances,
        );
        buildings::push_heart(&self.world, parties, &mut self.instances);
    }

    /// Hand the renderer the scenery near the camera, minus what stands
    /// under a building the viewer can see.
    fn push_scenery(&mut self, focus: Vec2, distance: f32, cull: &gc_render::Cull) {
        let shown = buildings::shown_to(self.sandbox.side, self.reveal_all);
        let rects = self.world.structures().into_iter().flat_map(|all| {
            all.iter()
                .filter(shown)
                .map(|s| (s.tile.0, s.tile.1, s.width, s.depth))
        });
        self.scenery.set_footprints(rects);
        let map = &self.heightmap;
        let nodes = self
            .world
            .economy()
            .into_iter()
            .flat_map(gc_sim::Economy::nodes);
        let nodes = nodes.map(|n| buildings::node_look(&n, |p| ground_of(map, p)));
        // Shaken regiments plant a morale standard at their centre (the
        // Commander View has its own flags).
        let viewer = self.sandbox.side;
        let (prev, alpha, world, parties) =
            (&self.prev_positions, self.alpha, &self.world, &self.parties);
        let banners = world
            .regiments
            .iter()
            .filter(|_| !self.commander)
            .filter(|r| self.reveal_all || world.sees(viewer, r.handle))
            .filter(|r| !world.is_worker(r.handle))
            .filter_map(|r| {
                let model = banner_model(r.morale)?;
                let (pos, facing) = drawn_pose(prev, alpha, &r);
                let turn = scenery::turn_of(facing);
                let team = color_of(parties, r.owner);
                let tint = match r.morale {
                    // The white rag of flight keeps its own colour.
                    MoraleState::Routing => [128; 3],
                    MoraleState::Shattered => team.map(|c| c / 3),
                    _ => team.map(|c| c / 2),
                };
                Some(scenery::Extra {
                    model,
                    at: Vec3::new(pos.x, ground_of(map, pos), pos.y),
                    turn,
                    scale: 1.0,
                    tint,
                    phase: turn,
                    lean: 0,
                    fall: 0,
                })
            });
        let falling = self.ambient.felling.extras(|p| ground_of(map, p));
        let fallen = self.aftermath.standards(|p| ground_of(map, p));
        let piled = self.wood_piles(focus, distance);
        let hamlets: Vec<scenery::Extra> = if self.commander {
            Vec::new()
        } else {
            let reach = scenery::stump_reach(distance);
            let built = |t| self.scenery.occupied_at(t);
            self.ambient.hamlets.near(focus, reach, built).collect()
        };
        let stumps = (!self.commander)
            .then(|| {
                let reach = scenery::stump_reach(distance);
                self.scars.stumps(focus, reach, |p| ground_of(map, p))
            })
            .into_iter()
            .flatten();
        self.scenery.set_extras(
            nodes
                .chain(banners)
                .chain(falling)
                .chain(fallen)
                .chain(stumps)
                .chain(piled)
                .chain(hamlets),
        );
        let (instances, batches) = self.scenery.frame(focus, distance, !self.commander, cull);
        self.renderer.set_scenery_frame(instances, batches);
    }

    /// The stacks and heaps of logs against the finished halls and
    /// workshops the viewer can see near `focus` (docs/34 stage 3d).
    fn wood_piles(&self, focus: Vec2, distance: f32) -> Vec<scenery::Extra> {
        if self.commander || self.viewer.is_some() {
            return Vec::new();
        }
        let reach = scenery::stump_reach(distance);
        let (reveal, fog) = (self.reveal_all, &self.fog);
        let shown = buildings::shown_to(self.sandbox.side, reveal);
        let mut looks = Vec::new();
        for s in (self.world.structures().into_iter()).flat_map(gc_sim::Structures::iter) {
            let Some(look) = self.siege.look(s.handle).filter(|_| shown(&s)) else {
                continue;
            };
            let centre = look.centre();
            if centre.distance(focus) <= reach + 8.0 {
                looks.push((look, buildings::facing_of(s.turn)));
            }
        }
        let hm = &self.heightmap;
        let dry = hm.water_level() + ambient::walkers::SHORE;
        let free = |p: Vec2| {
            let tile = (p.x.floor() as i32, p.y.floor() as i32);
            hm.height_at(p.x, p.y) > dry
                && (reveal || fog.sees_tile(tile))
                && !looks.iter().any(|(l, _)| l.gap(p) < ambient::piles::CLEAR)
        };
        let mut out = Vec::new();
        for (look, facing) in &looks {
            if look.centre().distance(focus) <= reach {
                out.extend(ambient::piles::extras(
                    look,
                    *facing,
                    |p| ground_of(hm, p),
                    free,
                ));
            }
        }
        out
    }

    /// `--siege`: Ember Clans troops of every kind attack buildings in
    /// front of the camera (blades a hall, archers a farmstead, a ram a
    /// watchtower, rocket carts the hall, a Concord engine a tenement).
    fn stage_siege(&mut self) {
        // Building, where (from the camera's focus), and its attackers:
        // unit, and how far south of the building they start.
        const SCENE: [SceneBuilding; 4] = [
            (
                "Clan Hall",
                Vec2::new(0.0, -12.0),
                &[
                    ("Katana Samurai", -3.0, 8.0),
                    ("Yari Ashigaru", 4.0, 8.0),
                    ("Ember Rocket Cart", 0.0, 32.0),
                ],
            ),
            (
                "Farmstead",
                Vec2::new(-18.0, -6.0),
                &[("Yumi Ashigaru", 0.0, 16.0)],
            ),
            (
                "Watchtower",
                Vec2::new(18.0, -6.0),
                &[("Battering Ram", 0.0, 6.0)],
            ),
            (
                "Tenement",
                Vec2::new(-34.0, -10.0),
                &[("Mausoleum Engine", 0.0, 9.0)],
            ),
        ];
        let Some(step) = self.siege_scene else {
            return;
        };
        let unit = |name: &str| self.units.names.iter().position(|n| n == name);
        let building = |name: &str| {
            (0..self.buildings.len() as BuildingId).find(|&id| self.buildings.name(id) == name)
        };
        let focus = self.camera.focus.round();
        let north = gc_fixed::Angle::of_vec(fx_vec(Vec2::NEG_Y));
        match step {
            0 => {
                for (name, at, attackers) in SCENE {
                    let Some(id) = building(name) else { continue };
                    let stats = self.buildings.stats[usize::from(id)];
                    let tile = buildings::tile_for(focus + at, stats.width, stats.depth);
                    self.human_orders.push(PlayerCommand {
                        player: 1,
                        command: gc_sim::Command::SpawnStructure {
                            building: id,
                            tile: (tile.0 as i16, tile.1 as i16),
                            turn: 0,
                        },
                    });
                    for &(soldiers, across, back) in attackers {
                        let Some(unit) = unit(soldiers) else { continue };
                        let half = f32::from(stats.depth) / 2.0;
                        self.human_orders.push(PlayerCommand {
                            player: 0,
                            command: gc_sim::Command::SpawnRegiment {
                                unit: unit as UnitId,
                                position: fx_vec(focus + at + Vec2::new(across, half + back)),
                                facing: north,
                            },
                        });
                    }
                }
                // Close enough to see the blows, and the placer out of the
                // way.
                self.camera.distance = SIEGE_SCENE_DISTANCE;
                self.hud.placer = false;
                self.sandbox.placing = false;
                self.siege_scene = Some(1);
            }
            1 if self.world.structures().is_some_and(|s| !s.is_empty()) => {
                let mut orders = Vec::new();
                for s in self
                    .world
                    .structures()
                    .into_iter()
                    .flat_map(gc_sim::Structures::iter)
                {
                    let name = self.buildings.name(s.building);
                    let Some((_, _, attackers)) = SCENE.iter().find(|(n, ..)| *n == name) else {
                        continue;
                    };
                    for r in self.world.regiments.iter().filter(|r| r.owner == 0) {
                        let soldiers = &self.units.names[usize::from(r.unit)];
                        if attackers.iter().any(|(n, ..)| n == soldiers) {
                            orders.push(PlayerCommand {
                                player: 0,
                                command: gc_sim::Command::AttackStructure {
                                    regiment: r.handle,
                                    structure: s.handle,
                                },
                            });
                        }
                    }
                }
                self.human_orders.extend(orders);
                self.siege_scene = None;
            }
            _ => {}
        }
    }

    /// Note how every building the viewer is shown is drawn, for the blows
    /// on it and its fall (see [`siege`]).
    fn note_building_looks(&mut self) {
        let Some(structures) = self.world.structures() else {
            return;
        };
        let hm = &self.heightmap;
        let ground = |x, z| hm.drawn_height(x, z);
        let shown = buildings::shown_to(self.sandbox.side, self.reveal_all);
        for s in structures.iter().filter(|s| shown(s)) {
            let Some(stats) = self.buildings.stats.get(usize::from(s.building)) else {
                continue;
            };
            let model = self.art.as_ref().and_then(|art| art.looks.model_of(&s));
            let top = match (model, &self.art) {
                (Some(m), Some(art)) => art.looks.top(m),
                _ => buildings::height_of(stats.role),
            };
            let (centre, base, lift) = buildings::model_spot(&s, &ground);
            // Blows land on the model's walls, where they stand within the
            // footprint.
            let footprint = Vec2::new(f32::from(s.width), f32::from(s.depth)) / 2.0;
            let walls = (model.zip(self.art.as_ref()))
                .and_then(|(m, art)| art.looks.walls(m))
                // Turned a quarter, its model's walls run the other way.
                .map(|w| {
                    if s.turn % 2 == 1 {
                        Vec2::new(w.y, w.x)
                    } else {
                        w
                    }
                })
                .filter(|w| w.min_element() > 0.25)
                .map_or(footprint, |w| w.min(footprint));
            self.siege.remember(siege::Look {
                handle: s.handle,
                min: centre - walls,
                max: centre + walls,
                base,
                top,
                model,
                lift,
                owner: s.owner,
                colour: color_of(&self.parties, s.owner),
                stuff: siege::stuff(self.buildings.faction(s.building), stats.armour),
                role: stats.role,
                finished: s.finished(),
                hp: if s.finished() {
                    s.hp as f32 / s.max_hp.max(1) as f32
                } else {
                    1.0
                },
            });
        }
    }

    /// The crows over a battlefield, once the soldiers have moved on, with
    /// the crow model (when the pack has it; else they are boxes). Every
    /// match has them, not only Conquest.
    fn push_crows(&mut self) {
        let Some(art) = &mut self.art else {
            return;
        };
        let hm = &self.heightmap;
        let ground = |x, z| hm.drawn_height(x, z);
        let time = self.puppets.time();
        if let Some(crow) = art.crow
            && !self.commander
            && self.viewer.is_none()
        {
            let mut birds = Vec::new();
            self.aftermath.birds(
                |p| ground(p.x, p.y),
                |at, radius| art.view.cull.sees(at, radius),
                &mut birds,
                usize::MAX,
            );
            for b in birds {
                let foot = Vec3::new(b.pos.x, ground(b.pos.x, b.pos.y) + b.lift, b.pos.y);
                let Some(lod) = art.view.place(art.heights[crow] * CROW_SCALE, foot) else {
                    continue;
                };
                let (action, rate) = if b.aloft > 0.05 {
                    (Action::Cheer, ambient::birds::FLAP_RATE * 0.8)
                } else {
                    (Action::Idle, 1.0)
                };
                let Some(pose) = art.clips.pose(crow, action, (time + b.phase * 4.0) * rate) else {
                    continue;
                };
                art.buckets[crow * LODS + lod].push(SoldierInstance {
                    pos: b.pos.to_array(),
                    facing: b.facing.to_array(),
                    frames: pose.rows,
                    flags: models::pose_flags(pose, 0, false),
                    color: [255, 255, 255, gc_render::TONE_NEUTRAL],
                    lift: b.lift,
                    scale: CROW_SCALE,
                    ..Default::default()
                });
            }
        }
    }

    /// The living things of the ambient pass that are on the ground and in
    /// the air: deer, insects, doves, the villagers of the hamlets and the
    /// caravans between them, as soldier instances. In every kind of match
    /// that makes them (the hamlets are Conquest's; the rest are on any
    /// land).
    fn push_creatures(&mut self) {
        let Some(art) = &mut self.art else {
            return;
        };
        let hm = &self.heightmap;
        let ground = |x, z| hm.drawn_height(x, z);
        let time = self.puppets.time();
        if let Some(deer) = art.deer {
            let (reveal, fog) = (self.reveal_all, &self.fog);
            for d in self.ambient.wildlife.seen() {
                let tile = (d.pos.x.floor() as i32, d.pos.y.floor() as i32);
                if !(reveal || fog.sees_tile(tile)) {
                    continue;
                }
                let foot = Vec3::new(d.pos.x, ground(d.pos.x, d.pos.y), d.pos.y);
                let Some(lod) = art.view.place(art.heights[deer] * d.scale, foot) else {
                    continue;
                };
                let (action, rate) = match d.doing {
                    ambient::wildlife::Doing::Graze => (Action::Cheer, 1.0),
                    ambient::wildlife::Doing::Stand => (Action::Idle, 1.0),
                    ambient::wildlife::Doing::Walk => (Action::Walk, 1.0),
                    ambient::wildlife::Doing::Bolt => (Action::Run, ambient::wildlife::RUN_RATE),
                };
                let Some(pose) = art.clips.pose(deer, action, (time + d.phase * 4.0) * rate) else {
                    continue;
                };
                art.buckets[deer * LODS + lod].push(SoldierInstance {
                    pos: d.pos.to_array(),
                    facing: d.facing.to_array(),
                    frames: pose.rows,
                    flags: models::pose_flags(pose, 0, false),
                    color: [255, 255, 255, gc_render::TONE_NEUTRAL],
                    lift: 0.0,
                    scale: d.scale,
                    ..Default::default()
                });
            }
        }
        if let Some(bugs) = art.insects {
            let (reveal, fog) = (self.reveal_all, &self.fog);
            for i in self.ambient.insects.seen() {
                let tile = (i.pos.x.floor() as i32, i.pos.y.floor() as i32);
                if !(reveal || fog.sees_tile(tile)) {
                    continue;
                }
                let model = bugs[i.kind.index()];
                let foot = Vec3::new(i.pos.x, ground(i.pos.x, i.pos.y) + i.alt, i.pos.y);
                let Some(lod) = art.view.place(art.heights[model] * i.scale, foot) else {
                    continue;
                };
                let (action, rate) = if i.beating {
                    (Action::Cheer, i.kind.beat_rate())
                } else {
                    (Action::Idle, 1.0)
                };
                let Some(pose) = art.clips.pose(model, action, (time + i.phase) * rate) else {
                    continue;
                };
                art.buckets[model * LODS + lod].push(SoldierInstance {
                    pos: i.pos.to_array(),
                    facing: i.facing.to_array(),
                    frames: pose.rows,
                    flags: models::pose_flags(pose, 0, false),
                    color: [255, 255, 255, gc_render::TONE_NEUTRAL],
                    lift: i.alt,
                    scale: i.scale,
                    ..Default::default()
                });
            }
        }
        if let Some(dove) = art.dove {
            let (reveal, fog) = (self.reveal_all, &self.fog);
            for d in self.ambient.birds.seen() {
                let tile = (d.pos.x.floor() as i32, d.pos.y.floor() as i32);
                if !(reveal || fog.sees_tile(tile)) {
                    continue;
                }
                let base = ground(d.pos.x, d.pos.y);
                let foot = Vec3::new(d.pos.x, base + d.alt, d.pos.y);
                let Some(lod) = art.view.place(art.heights[dove] * d.scale, foot) else {
                    continue;
                };
                let (action, rate) = match d.doing {
                    ambient::birds::Doing::Stand => (Action::Idle, 1.0),
                    ambient::birds::Doing::Strut => (Action::Walk, 1.0),
                    ambient::birds::Doing::Fly => (Action::Cheer, ambient::birds::FLAP_RATE),
                };
                let Some(pose) = art.clips.pose(dove, action, (time + d.phase * 4.0) * rate) else {
                    continue;
                };
                art.buckets[dove * LODS + lod].push(SoldierInstance {
                    pos: d.pos.to_array(),
                    facing: d.facing.to_array(),
                    frames: pose.rows,
                    flags: models::pose_flags(pose, 0, false),
                    color: [255, 255, 255, gc_render::TONE_NEUTRAL],
                    lift: d.alt,
                    scale: d.scale,
                    ..Default::default()
                });
            }
        }
        if let Some(folk) = art.folk {
            let (reveal, fog) = (self.reveal_all, &self.fog);
            for v in self.ambient.villagers.seen() {
                let tile = (v.pos.x.floor() as i32, v.pos.y.floor() as i32);
                if !(reveal || fog.sees_tile(tile)) {
                    continue;
                }
                let model = folk[v.kind.index()];
                let foot = Vec3::new(v.pos.x, ground(v.pos.x, v.pos.y), v.pos.y);
                let Some(lod) = art.view.place(art.heights[model], foot) else {
                    continue;
                };
                let action = match v.doing {
                    ambient::villagers::Doing::Stand => Action::Idle,
                    ambient::villagers::Doing::Walk => Action::Walk,
                    ambient::villagers::Doing::Run => Action::Run,
                    ambient::villagers::Doing::Work => Action::Cheer,
                };
                let Some(pose) = art.clips.pose(model, action, time + v.phase) else {
                    continue;
                };
                art.buckets[model * LODS + lod].push(SoldierInstance {
                    pos: v.pos.to_array(),
                    facing: v.facing.to_array(),
                    frames: pose.rows,
                    flags: models::pose_flags(pose, 0, false),
                    color: [255, 255, 255, gc_render::TONE_NEUTRAL],
                    lift: 0.0,
                    scale: 1.0,
                    ..Default::default()
                });
            }
        }
        {
            use ambient::caravans::Piece;
            let (reveal, fog) = (self.reveal_all, &self.fog);
            for c in self.ambient.caravans.seen() {
                let tile = (c.pos.x.floor() as i32, c.pos.y.floor() as i32);
                if !(reveal || fog.sees_tile(tile)) {
                    continue;
                }
                let model = match c.piece {
                    Piece::Cart => art.cart,
                    Piece::Drover => art
                        .folk
                        .map(|f| f[ambient::villagers::Kind::Farmer.index()]),
                    Piece::Porter => art
                        .folk
                        .map(|f| f[ambient::villagers::Kind::Porter.index()]),
                };
                let Some(model) = model else {
                    continue;
                };
                // A drover with no cart to lead is not drawn.
                if c.piece == Piece::Drover && art.cart.is_none() {
                    continue;
                }
                let foot = Vec3::new(c.pos.x, ground(c.pos.x, c.pos.y), c.pos.y);
                let Some(lod) = art.view.place(art.heights[model], foot) else {
                    continue;
                };
                let (action, rate) = if c.moving {
                    (Action::Walk, c.rate)
                } else {
                    (Action::Idle, 1.0)
                };
                let Some(pose) = art.clips.pose(model, action, (time + c.phase) * rate) else {
                    continue;
                };
                art.buckets[model * LODS + lod].push(SoldierInstance {
                    pos: c.pos.to_array(),
                    facing: c.facing.to_array(),
                    frames: pose.rows,
                    flags: models::pose_flags(pose, 0, false),
                    color: [255, 255, 255, gc_render::TONE_NEUTRAL],
                    lift: 0.0,
                    scale: 1.0,
                    ..Default::default()
                });
            }
        }
    }

    /// Every standing building that has models, at its construction
    /// stage, shaken by the blows on it, the crews on tier upgrades, and
    /// fallen buildings sinking (after `Art::begin`).
    fn push_building_models(&mut self) {
        let picked = self.picked_structure();
        let (Some(art), Some(structures)) = (&mut self.art, self.world.structures()) else {
            return;
        };
        let hm = &self.heightmap;
        // As the skinning shader clamps it, so the lift lands where meant.
        let ground = |x, z| hm.drawn_height(x, z);
        let shown = buildings::shown_to(self.sandbox.side, self.reveal_all);
        let time = self.puppets.time();
        for fall in self.siege.sinking() {
            let height = art.looks.top(fall.model);
            let at = Vec3::new(fall.pos.x, ground(fall.pos.x, fall.pos.y), fall.pos.y);
            let still = art.clips.pose(fall.model, Action::Idle, time);
            if let (Some(lod), Some(pose)) = (art.view.place(height, at), still) {
                let [r, g, b] = fall.colour;
                art.buckets[fall.model * LODS + lod].push(SoldierInstance {
                    pos: fall.pos.to_array(),
                    facing: [0.0, 1.0],
                    frames: pose.rows,
                    flags: models::pose_flags(pose, 0, false),
                    color: [r, g, b, gc_render::TONE_NEUTRAL],
                    lift: fall.lift,
                    scale: 1.0,
                    ..Default::default()
                });
            }
        }
        for s in structures.iter().filter(|s| shown(s)) {
            let [r, g, b] = color_of(&self.parties, s.owner);
            if let Some(model) = art.looks.model_of(&s) {
                let (centre, top, lift) = buildings::model_spot(&s, &ground);
                let (shake, rise) = self.siege.jolt(s.handle);
                let (centre, lift) = (centre + shake, lift + rise);
                let at = Vec3::new(centre.x, top, centre.y);
                let still = art.clips.pose(model, Action::Idle, time);
                let height = art.looks.top(model);
                if let (Some(lod), Some(pose)) = (art.view.place(height, at), still) {
                    let highlight = if picked == Some(s.handle) { 40 } else { 0 };
                    art.buckets[model * LODS + lod].push(SoldierInstance {
                        pos: centre.to_array(),
                        // The door, on the model's +z, faces south unturned.
                        facing: buildings::facing_of(s.turn).to_array(),
                        frames: pose.rows,
                        flags: models::pose_flags(pose, highlight, false),
                        color: [r, g, b, gc_render::TONE_NEUTRAL],
                        lift,
                        scale: 1.0,
                        ..Default::default()
                    });
                }
            }
            let Some(worker) = art.looks.crew_of(&s) else {
                continue;
            };
            let centre = buildings::centre_of(s.tile, s.width, s.depth);
            for (k, (at, facing)) in buildings::crew_spots(centre, s.width, s.depth)
                .into_iter()
                .enumerate()
            {
                let foot = Vec3::new(at.x, ground(at.x, at.y), at.y);
                let Some(lod) = art.view.place(art.heights[worker], foot) else {
                    continue;
                };
                let Some(pose) = art
                    .clips
                    .pose(worker, swing(k % 2 == 0), time + k as f32 * 0.37)
                else {
                    continue;
                };
                art.buckets[worker * LODS + lod].push(SoldierInstance {
                    pos: at.to_array(),
                    facing: facing.to_array(),
                    frames: pose.rows,
                    flags: models::pose_flags(pose, 0, false),
                    color: [r, g, b, gc_render::TONE_NEUTRAL],
                    lift: 0.0,
                    scale: 1.0,
                    ..Default::default()
                });
            }
        }
        for w in self.ambient.walkers.seen() {
            let [r, g, b] = color_of(&self.parties, w.owner);
            let foot = Vec3::new(w.pos.x, ground(w.pos.x, w.pos.y), w.pos.y);
            let Some(lod) = art.view.place(art.heights[w.worker], foot) else {
                continue;
            };
            let action = if w.walking {
                Action::Walk
            } else {
                Action::Idle
            };
            let Some(pose) = art.clips.pose(w.worker, action, time + w.phase * 4.0) else {
                continue;
            };
            art.buckets[w.worker * LODS + lod].push(SoldierInstance {
                pos: w.pos.to_array(),
                facing: w.facing.to_array(),
                frames: pose.rows,
                flags: models::pose_flags(pose, 0, false),
                color: [r, g, b, gc_render::TONE_NEUTRAL],
                lift: 0.0,
                scale: 1.0,
                ..Default::default()
            });
        }
    }

    /// The buildings walkers start from, go to and walk round: those the
    /// viewer sees within [`ambient::walkers::REACH`] of `focus`, with the
    /// ones on screen the only ones to live at or be visited.
    fn walker_homes(&self, cull: &gc_render::Cull, focus: Vec2) -> Vec<ambient::walkers::Home> {
        use gc_sim::structures::Role;
        let Some(art) = &self.art else {
            return Vec::new();
        };
        let (reveal, fog) = (self.reveal_all, &self.fog);
        let shown = buildings::shown_to(self.sandbox.side, reveal);
        let mut homes = Vec::new();
        for s in (self.world.structures().into_iter()).flat_map(gc_sim::Structures::iter) {
            let centre = buildings::centre_of(s.tile, s.width, s.depth);
            if !shown(&s)
                || centre.distance(focus)
                    > ambient::walkers::REACH + ambient::walkers::OBSTACLE_MARGIN
            {
                continue;
            }
            let half = Vec2::new(f32::from(s.width), f32::from(s.depth)) / 2.0;
            let facing = buildings::facing_of(s.turn);
            let door = centre + facing * (facing.abs().dot(half) + ambient::walkers::DOOR);
            let look = self.siege.look(s.handle);
            let role = look.map(|l| l.role);
            let on_screen = look.is_some_and(|l| {
                let at = Vec3::new(centre.x, l.base + l.top * 0.5, centre.y);
                cull.on_screen(at, l.top + 6.0)
                    && (reveal || fog.sees_tile((centre.x.floor() as i32, centre.y.floor() as i32)))
            });
            let done = s.finished() && on_screen;
            let worker = art.looks.worker_of(s.building);
            homes.push(ambient::walkers::Home {
                id: u64::from(s.handle.index) | u64::from(s.handle.generation) << 32,
                owner: s.owner,
                lives: done
                    && matches!(
                        role,
                        Some(
                            Role::Main
                                | Role::Outpost
                                | Role::Economy
                                | Role::Production
                                | Role::Tech
                                | Role::Heroes
                        )
                    )
                    && worker.is_some(),
                visit: done && !matches!(role, Some(Role::Wall | Role::Defence)),
                worker: worker.unwrap_or(0),
                centre,
                half,
                door,
            });
        }
        homes
    }

    /// A plate under the cursor the size of the regiment B would place.
    fn push_placement_ghost(&mut self) {
        if !self.sandbox.placing {
            return;
        }
        let Some((at, facing)) = self.placement() else {
            return;
        };
        let stats = &self.units.stats[self.sandbox.unit as usize];
        let (width, depth) = stats.block_size_in(Formation::Line, stats.files, stats.models);
        let [cr, cg, cb] = color_of(&self.parties, self.sandbox.side);
        let color = if self.world.nav().walkable(self.sandbox.side, fx_vec(at)) {
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

    /// Delete: take away the building under the cursor, else the selected
    /// regiments. Shift+Delete clears the field of both.
    fn delete_key(&mut self) {
        let side = self.sandbox.side;
        if self.input.shift {
            let all: Vec<StructureHandle> = self
                .world
                .structures()
                .map(|s| s.iter().map(|v| v.handle).collect())
                .unwrap_or_default();
            self.human_orders
                .extend(all.into_iter().map(|structure| PlayerCommand {
                    player: side,
                    command: gc_sim::Command::RemoveStructure { structure },
                }));
            self.remove_regiments(true);
        } else if let Some(structure) = self.building_under_cursor(|_| true) {
            self.human_orders.push(PlayerCommand {
                player: side,
                command: gc_sim::Command::RemoveStructure { structure },
            });
        } else {
            self.remove_regiments(false);
        }
    }

    /// Take the selected regiments off the field, or with `all`
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
            let mut stats = self.units.stats[unit as usize];
            let steps = if self.input.shift { step * 10 } else { step };
            (sandbox::TUNABLES[self.sandbox.row].nudge)(&mut stats, steps);
            self.tune(unit, stats.sanitized());
        }
        self.sandbox.dirty = true;
        true
    }

    /// Send new stats for a unit type, and show them straight away.
    fn tune(&mut self, unit: UnitId, stats: UnitStats) {
        self.units.stats[unit as usize] = stats;
        self.human_orders.push(PlayerCommand {
            player: self.sandbox.side,
            command: gc_sim::Command::TuneUnit { unit, stats },
        });
    }

    /// Shift+R: read `data/units.ron` again and tune every type that changed.
    #[cfg(feature = "dev")]
    fn reload_data(&mut self) {
        if self.play.roster_table() {
            let why = if self.play == Play::SandboxRoster {
                "Shift+R reloads the Field types in data/units.ron, not the roster table the sandbox runs on now. Choose a Field type in the Place panel first."
            } else {
                "Shift+R reloads the Field Battle units, so not in Conquest."
            };
            self.say(why.into());
            return;
        }
        let Some(dir) = sandbox::disk::data_dir() else {
            self.say(
                "Shift+R reloads data/units.ron, but there is none here (run from the repository)."
                    .into(),
            );
            return;
        };
        let (names, stats) = match sandbox::disk::reload(&dir) {
            Ok(fresh) => fresh,
            Err(e) => {
                logln!("grubcraft: reloading data: {e}");
                self.say(format!("data/units.ron has a problem: {e}"));
                return;
            }
        };
        // By name, so a row added or removed above a type cannot hand it
        // another type's stats.
        let plan = match sandbox::plan_reload(&self.units.names, &names) {
            Ok(plan) => plan,
            Err(e) => {
                self.say(format!("data/units.ron not reloaded: {e}."));
                return;
            }
        };
        let mut changed = 0;
        for &(fresh, running) in &plan.matched {
            let new = stats[fresh].sanitized();
            if new != self.units.stats[running] {
                self.tune(running as UnitId, new);
                changed += 1;
            }
        }
        let mut message = format!("Reloaded data/units.ron: {changed} unit types changed.");
        for (what, names) in [("add", &plan.added), ("remove", &plan.removed)] {
            if !names.is_empty() {
                message += &format!(" Restart to {what}: {}.", names.join(", "));
            }
        }
        self.say(message);
    }

    #[cfg(not(feature = "dev"))]
    fn reload_data(&mut self) {
        self.say("Shift+R reloads data/units.ron in a build made with --features dev.".into());
    }

    /// A key while the light tuning panel is open; true when it took it.
    fn light_key(&mut self, code: KeyCode, repeat: bool, fast: bool) -> bool {
        use atmosphere::TuneKey;
        let key = match code {
            KeyCode::ArrowUp => TuneKey::Up,
            KeyCode::ArrowDown => TuneKey::Down,
            KeyCode::ArrowLeft => TuneKey::Less,
            KeyCode::ArrowRight => TuneKey::More,
            KeyCode::Backspace | KeyCode::Delete => TuneKey::Reset,
            // Holding these down must not repeat them.
            KeyCode::KeyL if repeat => return true,
            KeyCode::KeyL => TuneKey::Land,
            KeyCode::Enter | KeyCode::NumpadEnter if repeat => return true,
            KeyCode::Enter | KeyCode::NumpadEnter => {
                logln!("grubcraft: {}", self.atmosphere.values());
                self.say("The tuned light is in the log.".into());
                return true;
            }
            _ => return false,
        };
        self.atmosphere.tune_key(key, fast);
        self.sandbox.dirty = true;
        true
    }

    /// How fast the director slows a replay for a big moment; a live match
    /// always runs at its own pace.
    fn film_pace(&self) -> f32 {
        if self.watching.is_some() {
            self.director.pace()
        } else {
            1.0
        }
    }

    /// Where the view in use is looking and how far it is from the ground:
    /// the bird's-eye camera's focus, or a little ahead of a ride.
    fn view_focus(&self) -> (Vec2, f32) {
        match &self.ride {
            Some(ride) => {
                let (eye, forward) = (ride.camera.eye(), ride.camera.forward());
                (
                    Vec2::new(eye.x + forward.x * 8.0, eye.z + forward.z * 8.0),
                    camera::MIN_DISTANCE,
                )
            }
            None => (self.camera.focus, self.camera.distance),
        }
    }

    /// The way the camera looks over the map, as a unit vector.
    fn view_heading(&self) -> Vec2 {
        match &self.ride {
            Some(ride) => ride.camera.forward_xz(),
            None => {
                let forward = self.camera.forward();
                Vec2::new(forward.x, forward.z).normalize_or(Vec2::Y)
            }
        }
    }

    /// Show a line in the sandbox panel for a few seconds.
    fn say(&mut self, message: String) {
        logln!("grubcraft: {message}");
        self.sandbox.message = Some((message, Instant::now()));
        self.sandbox.dirty = true;
    }

    /// The overlay box's text: only what needs saying now (the network,
    /// who won, the replay's end, the stats tuner and the last message).
    /// The keys and the standing information are in the Marshal's Scroll
    /// (`help_notes`).
    fn sandbox_panel(&self) -> String {
        let sb = &self.sandbox;
        let unit = sb.unit as usize;
        let name = &self.units.names[unit];
        let mut lines = Vec::new();
        if let Some(o) = &self.online {
            let lan = self.menu.network();
            lines.extend(o.lines(lan.status(), lan.waiting()));
            lines.push(String::new());
        }
        if let Some(news) = self.battle_news() {
            lines.push(news);
            lines.push(String::new());
        }
        if let Some(w) = &self.watching {
            let tick = self.world.tick_count();
            lines.push(if tick >= w.replay.ticks {
                format!(
                    "REPLAY  {}   over at {}   Esc: menu",
                    w.title,
                    gc_ai::lens::clock(tick)
                )
            } else {
                format!(
                    "REPLAY  {}   {} of {}",
                    w.title,
                    gc_ai::lens::clock(tick),
                    gc_ai::lens::clock(w.replay.ticks)
                )
            });
            if let Some(at) = w.diverged_at {
                lines.push(format!(
                    "plays differently from {} on: made with other rules",
                    gc_ai::lens::clock(at)
                ));
            }
        }
        if sb.tuning && self.watching.is_none() {
            let on_field = self
                .world
                .regiments
                .iter()
                .filter(|r| r.unit == sb.unit)
                .count();
            lines.push(format!(
                "TUNE {name} ({on_field} on the field)   Up/Down: pick   Left/Right: change   T: close"
            ));
            let stats = &self.units.stats[unit];
            for (k, t) in sandbox::TUNABLES.iter().enumerate() {
                let mark = if k == sb.row { '>' } else { ' ' };
                lines.push(format!("{mark} {:<15} {}", t.label, (t.show)(stats)));
            }
            lines.push(String::new());
        }
        if let Some((message, _)) = &sb.message {
            lines.push(message.clone());
        }
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines.join("\n")
    }

    /// The live lines at the foot of the Marshal's Scroll: what the
    /// computer players are doing, what is being placed, the formation.
    /// The keys themselves are `gc_ui::hud::HELP_GROUPS`.
    fn help_notes(&self) -> Vec<String> {
        let sb = &self.sandbox;
        let name = &self.units.names[sb.unit as usize];
        let mut lines = Vec::new();
        let commanding = format!(
            "commanding {} (player {})",
            party_name(&self.parties, sb.side),
            sb.side + 1
        );
        let minds = |lines: &mut Vec<String>| {
            for m in self.minds.iter() {
                lines.push(format!(
                    "{}: {}",
                    party_name(&self.parties, m.party()),
                    m.summary()
                ));
            }
        };
        if let Some(w) = &self.watching {
            lines.push(format!("Replay of {}, following {}", w.title, commanding));
            lines.push(if self.reveal_all {
                "Woods: showing everything (Shift+V: only what this side saw)".to_string()
            } else {
                "Woods: Shift+V shows everything".to_string()
            });
            return lines;
        }
        if self.online.is_some() {
            lines.push(format!(
                "{} over the network, {commanding}",
                self.mode_name()
            ));
            minds(&mut lines);
            lines.push(self.formation_line());
            lines.push(self.army_shape_line());
            return lines;
        }
        let mode = if self.battle.is_some() {
            self.mode_name()
        } else {
            "SANDBOX"
        };
        lines.push(format!("{mode}, {commanding}"));
        minds(&mut lines);
        lines.push(match self.minds.get(sb.side) {
            None => "I: hand this side to the computer".to_string(),
            Some(m) => {
                let next = Difficulty::ALL
                    .get(m.difficulty().index() + 1)
                    .map_or("take it back", |d| d.name());
                let thinking = m
                    .journal()
                    .latest()
                    .map_or(String::new(), |e| format!(": {}", e.text));
                format!("I: {next}. The computer plays this side{thinking}")
            }
        });
        lines.push(if sb.building_tool {
            format!(
                "Placing {}: [ ] choose, R turns, click to place, K stops",
                self.buildings.describe(sb.building)
            )
        } else if sb.placing {
            format!("Placing {name}: [ ] type, click to place, B stops")
        } else {
            format!("B: place regiments ([ ] type: {name})")
        });
        if let (Some(standing), false) = (self.world.structures(), sb.building_tool) {
            lines.push(format!(
                "K: place buildings ({} standing). Right-click an enemy one with regiments selected to attack it",
                standing.len()
            ));
        }
        lines.push(self.formation_line());
        lines.push(self.army_shape_line());
        if self.battle.is_none() {
            lines.push(if self.reveal_all {
                "Woods: Shift+V hides what this side can't see (now showing everything)".to_string()
            } else {
                "Woods: Shift+V shows regiments hidden from this side".to_string()
            });
        }
        lines
    }

    /// The selection's formation, for the panel.
    /// The selection's formation: `None` with nothing selected, `Some(None)`
    /// when they stand in different ones.
    fn selection_formation(&self) -> Option<Option<Formation>> {
        let mut formations = self
            .selected
            .iter()
            .filter_map(|&h| self.world.regiments.get(h))
            .map(|r| r.formation);
        let first = formations.next()?;
        Some(formations.all(|f| f == first).then_some(first))
    }

    fn formation_line(&self) -> String {
        match self.selection_formation() {
            None => "R: formation (Line, Tight, Loose, Shield wall) for the selection".into(),
            Some(Some(f)) => format!("Selection stands in {}. R: next formation", f.name()),
            Some(None) => "Selection stands in mixed formations. R: next formation".into(),
        }
    }

    fn army_shape_line(&self) -> String {
        format!(
            "Army shape for group orders: {}. O: next, Shift+O: save the selection's",
            self.army_shapes.choice.label()
        )
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
            "Grubcraft{} · {fps:.0} fps · sim {tick_ms:.3} ms/tick · {} regiments · {} fallen · {} arrows stuck · {}/{} chunks · {} MB process · {} MB heap · {} MB gpu",
            self.speed_label(),
            self.world.regiments.len(),
            self.puppets.corpse_count(),
            self.arrows.len(),
            rs.chunks_drawn,
            rs.chunks_total,
            mem.current
                .map_or("?".into(), |b| format!("{:.1}", mb(b as usize))),
            format_args!("{:.1}", mb(tagged_alloc::total_live_bytes())),
            format_args!("{:.1}", mb(self.renderer.gpu_bytes() as usize)),
        )
    }
}

/// A map the lobby can play on: a Field Battle map made from its choice,
/// size and seed, or a Conquest map made from its choice and seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MapKey {
    Field(FieldMap),
    /// An index into `gc_ui::lobby::CONQUEST_MAPS`, and the seed a Wilds
    /// map is made from (`gc_ui::lobby::conquest_map_seed`).
    Conquest {
        choice: usize,
        seed: u64,
    },
}

impl MapKey {
    fn make(self) -> Map {
        match self {
            MapKey::Field(key) => key.make(),
            MapKey::Conquest { choice, seed } => conquest::make_map(choice, seed),
        }
    }

    fn name(self) -> &'static str {
        match self {
            MapKey::Field(key) => gc_map::FIELD_MAPS.get(key.choice),
            MapKey::Conquest { choice, .. } => gc_ui::lobby::CONQUEST_MAPS.get(choice),
        }
        .copied()
        .unwrap_or("?")
    }
}

impl From<MapPreviewKey> for MapKey {
    fn from(key: MapPreviewKey) -> MapKey {
        match key {
            MapPreviewKey::Field {
                choice,
                tiles,
                seed,
            } => MapKey::Field(FieldMap {
                choice,
                tiles,
                seed,
            }),
            MapPreviewKey::Conquest { choice, seed } => MapKey::Conquest { choice, seed },
        }
    }
}

impl Options {
    /// The lobby's time of day and sky that `--time-of-day` and `--sky`
    /// chose, in a quick match's settings.
    fn set_sky(&self, setup: &mut MatchStart) {
        if let Some(t) = self.time_of_day {
            setup.settings.time_of_day = t;
        }
        if let Some(w) = self.sky {
            setup.settings.weather = w;
        }
    }
}

/// The thing in `all` whose name is `text`, ignoring case, spaces and dashes.
fn by_name<T: Copy>(all: &[T], name: fn(T) -> &'static str, text: &str) -> Option<T> {
    let key = |s: &str| {
        s.chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect::<String>()
    };
    all.iter().copied().find(|&t| key(name(t)) == key(text))
}

/// The prepared look a lobby's time of day stands for.
fn preset_of(time_of_day: gc_ui::lobby::TimeOfDay) -> gc_render::LookPreset {
    use gc_ui::lobby::TimeOfDay;
    match time_of_day {
        TimeOfDay::GoldenHour => gc_render::LookPreset::GoldenHour,
        TimeOfDay::Noon => gc_render::LookPreset::Noon,
        TimeOfDay::Dusk => gc_render::LookPreset::Dusk,
        TimeOfDay::Overcast => gc_render::LookPreset::Overcast,
        TimeOfDay::Night => gc_render::LookPreset::Night,
    }
}

/// The map a lobby match of `players` is played on.
fn battle_map(settings: &gc_ui::lobby::LobbySettings, players: usize) -> MapKey {
    gc_ui::menu::preview_key(settings, players).into()
}

/// The last lobby map made, so the lobby's preview, the host's
/// fingerprint and the match itself make it once between them.
static LAST_MAP: std::sync::Mutex<Option<(MapKey, Arc<Map>)>> = std::sync::Mutex::new(None);

/// Make the lobby map `key`, or share it if it was the last made.
/// The lock is held while a map is made, so a second ask for the same map
/// (the match starting while its picture is being made) waits for it
/// instead of making it again.
fn make_map(key: MapKey) -> Arc<Map> {
    let mut last = LAST_MAP
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((k, map)) = &*last
        && *k == key
    {
        return Arc::clone(map);
    }
    let map = Arc::new(tagged_alloc::scope(Tag::Map, || key.make()));
    *last = Some((key, Arc::clone(&map)));
    map
}

/// The fingerprint of a lobby battle's map, which the host sends to every
/// player when a network match starts ([`gc_net::Identity::map_hash`]).
fn battle_map_hash(settings: &gc_ui::lobby::LobbySettings, players: usize) -> u64 {
    make_map(battle_map(settings, players)).hash()
}

/// The scripted opponents of the sandbox.
fn make_drill(seed: u64, map_tiles: u32) -> Drill {
    let mut drill = Drill::new(seed);
    drill.map_tiles = map_tiles as i32;
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

/// Hand the renderer the ground to draw: its heights, and the grass's colour
/// by land.
fn set_terrain(renderer: &mut gc_render::Renderer, map: &Map) {
    let (tint_side, tint) = terrain_view::ground_tint(map);
    renderer.set_terrain(&Terrain {
        size: map.side(),
        samples_per_tile: gc_map::SAMPLES_PER_TILE,
        max_height: map.max_height(),
        water_level: map.water_level(),
        heights: &map.samples,
        cliff: &map.cliff_cells(),
        tint: &tint,
        tint_side,
        look: &ground_look::ground_look(map),
    });
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

/// What the director sees of the field: where the regiments in melee are, and
/// where every regiment is, those the viewer sees.
fn see_the_field(world: &World, side: gc_sim::PlayerId, reveal: bool) -> (Vec<Vec2>, Vec<Vec2>) {
    let (mut melee, mut everyone) = (Vec::new(), Vec::new());
    for r in world.regiments.iter() {
        if r.morale == MoraleState::Shattered || !(reveal || world.sees(side, r.handle)) {
            continue;
        }
        let at = from_fx_vec(r.position);
        if r.engaged {
            melee.push(at);
        }
        everyone.push(at);
    }
    (melee, everyone)
}

fn from_fx_vec(v: gc_fixed::FxVec2) -> Vec2 {
    Vec2::new(fx_to_f32(v.x), fx_to_f32(v.y))
}

fn fx_vec(v: Vec2) -> gc_fixed::FxVec2 {
    gc_fixed::FxVec2::new(to_fx(v.x), to_fx(v.y))
}

/// The number on a digit key, for control groups.
/// A stable value in [-1, 1) per volley, arrow and axis.
pub(crate) fn spread_hash(seed: u32, k: usize, axis: u32) -> f32 {
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

/// Where a regiment is drawn, and which way it faces: `alpha` of the way
/// from its previous tick to the current one. A regiment with no record of
/// the previous tick (just spawned, or the slot was reused) is drawn where
/// it is. Picking, routes and the minimap use this too, so they agree with
/// what the player sees.
/// The standard a regiment in this morale state plants, if any.
fn banner_model(m: MoraleState) -> Option<scenery::Model> {
    match m {
        MoraleState::Steady => None,
        MoraleState::Wavering => Some(scenery::Model::BannerWavering),
        MoraleState::Routing => Some(scenery::Model::BannerRouting),
        MoraleState::Shattered => Some(scenery::Model::BannerShattered),
    }
}

fn drawn_pose(
    prev_positions: &[Option<(RegimentHandle, Vec2, Vec2)>],
    alpha: f32,
    r: &gc_sim::RegimentView,
) -> (Vec2, Vec2) {
    pose_between(
        prev_positions[r.handle.index as usize],
        r.handle,
        (from_fx_vec(r.position), angle_vec(r.facing)),
        alpha,
    )
}

/// Just the position of [`drawn_pose`], for the many callers that need no
/// facing (the minimap runs it over every regiment each frame).
fn drawn_position(
    prev_positions: &[Option<(RegimentHandle, Vec2, Vec2)>],
    alpha: f32,
    r: &gc_sim::RegimentView,
) -> Vec2 {
    let cur = from_fx_vec(r.position);
    match prev_positions[r.handle.index as usize] {
        Some((h, prev, _)) if h == r.handle => prev.lerp(cur, alpha),
        _ => cur,
    }
}

fn pose_between(
    prev: Option<(RegimentHandle, Vec2, Vec2)>,
    handle: RegimentHandle,
    (cur, cur_facing): (Vec2, Vec2),
    alpha: f32,
) -> (Vec2, Vec2) {
    match prev {
        Some((h, prev, prev_facing)) if h == handle => (
            prev.lerp(cur, alpha),
            prev_facing.lerp(cur_facing, alpha).normalize_or(cur_facing),
        ),
        _ => (cur, cur_facing),
    }
}

/// Unit vector for a simulation angle (0 = +x, a quarter turn = +y).
fn angle_vec(a: gc_fixed::Angle) -> Vec2 {
    Vec2::from_angle(f32::from(a.0) / 65536.0 * std::f32::consts::TAU)
}

/// How far the wheel turned, in rows (up is positive).
fn wheel_lines(delta: &MouseScrollDelta) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => *y,
        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
    }
}

/// Whole rows to scroll down for this wheel turn. A trackpad sends fractions
/// of a row: `rest` collects them until they add up.
fn wheel_rows(rest: &mut f32, delta: &MouseScrollDelta) -> i32 {
    *rest -= wheel_lines(delta);
    let rows = rest.trunc();
    *rest -= rows;
    rows as i32
}

fn main() {
    let opts = parse_args();
    if let Some(dir) = gc_ui::settings::app_dir() {
        gc_platform::diag::init(&dir);
    }
    gc_platform::diag::set_context("build", online::version());
    profile::start();
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            logln!("grubcraft: cannot open a window: {e}");
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
        logln!("grubcraft: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod lod_tests {
    use super::*;

    /// A scale of 1 makes `height / distance` the pixel count directly.
    fn lod_at_pixels(pixels: f32) -> usize {
        lod_for(pixels, 1.0, 1.0)
    }

    #[test]
    fn thresholds_pick_the_expected_level() {
        assert_eq!(lod_at_pixels(500.0), 0);
        assert_eq!(lod_at_pixels(100.0), 0);
        assert_eq!(lod_at_pixels(99.9), 1);
        assert_eq!(lod_at_pixels(40.0), 1);
        assert_eq!(lod_at_pixels(39.9), 2);
        assert_eq!(lod_at_pixels(16.0), 2);
        assert_eq!(lod_at_pixels(15.0), 3);
        assert_eq!(lod_at_pixels(0.0), 3);
    }

    #[test]
    fn nearer_is_never_coarser() {
        let mut last = 0;
        for step in 0..2000 {
            let distance = 0.5 + step as f32 * 0.5;
            let lod = lod_for(2.0, distance, 1000.0);
            assert!(lod >= last, "distance {distance}: {lod} after {last}");
            assert!(lod < LODS);
            last = lod;
        }
        assert_eq!(lod_for(2.0, 1.0, 1000.0), 0);
        assert_eq!(last, LODS - 1);
    }

    #[test]
    fn distance_below_one_counts_as_one() {
        assert_eq!(lod_for(50.0, 0.0, 1.0), lod_for(50.0, 1.0, 1.0));
        assert_eq!(lod_for(100.0, 0.25, 1.0), 0);
        assert_eq!(lod_for(39.0, 0.001, 1.0), lod_for(39.0, 1.0, 1.0));
    }

    #[test]
    fn head_turns_fade_in_above_the_nearest_level() {
        assert_eq!(head_detail(LOD_PIXELS[0], 1.0, 1.0), 0.0);
        assert_eq!(head_detail(LOD_PIXELS[0] - 1.0, 1.0, 1.0), 0.0);
        let half = head_detail(LOD_PIXELS[0] + HEAD_FADE_PIXELS / 2.0, 1.0, 1.0);
        assert!((half - 0.5).abs() < 1e-4);
        assert_eq!(head_detail(LOD_PIXELS[0] + HEAD_FADE_PIXELS, 1.0, 1.0), 1.0);
    }

    #[test]
    fn look_angles_fill_the_byte_and_stop_at_its_ends() {
        assert_eq!(
            look_angle(std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2),
            127
        );
        assert_eq!(look_angle(-9.0, 1.0), -127);
    }

    #[test]
    fn soldiers_at_one_depth_share_a_level_across_the_screen() {
        // At depth 20 a 2-tile model is exactly 100 px: level 0 on the
        // screen's centre line and off to the side alike.
        let view = LodView {
            eye: glam::Vec3::ZERO,
            forward: glam::Vec3::NEG_Z,
            pixel_scale: 1000.0,
            cull: gc_render::Cull::EVERYTHING,
            force: None,
        };
        assert_eq!(view.lod(2.0, glam::Vec3::new(0.0, 0.0, -20.0)), 0);
        assert_eq!(view.lod(2.0, glam::Vec3::new(12.0, -5.0, -20.0)), 0);
        assert_eq!(view.lod(2.0, glam::Vec3::new(0.0, 0.0, -21.0)), 1);
        let forced = LodView {
            force: Some(3),
            ..view
        };
        assert_eq!(forced.lod(2.0, glam::Vec3::new(0.0, 0.0, -1.0)), 3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_codes_are_the_grids_keys() {
        for (code, key) in GRID_CODES.iter().zip(gc_ui::hud::GRID_KEYS) {
            assert_eq!(format!("{code:?}"), format!("Key{key}"));
        }
    }

    const HANDLE: RegimentHandle = RegimentHandle {
        index: 3,
        generation: 1,
    };

    #[test]
    fn about_a_quarter_of_the_soldiers_carry_a_torch() {
        let bearers = (0..1000)
            .filter(|i| torch_bearer(*i as f32 / 1000.0))
            .count();
        assert!((200..=300).contains(&bearers), "{bearers}");
    }

    /// The lobby lists the maps gc_map makes, in its order: a lobby's map
    /// index picks the same map on every machine.
    #[test]
    fn the_lobby_offers_the_maps_gc_map_makes() {
        assert_eq!(gc_ui::lobby::MAPS, gc_map::FIELD_MAPS);
        assert_eq!(gc_ui::lobby::DEFAULT_MAP, gc_map::GROWN);
    }

    /// A new lobby's two-player battle is on grown land, as wild as the
    /// seed picks, at the standard size for two.
    #[test]
    fn a_new_lobby_plays_on_grown_land() {
        let key = battle_map(&gc_ui::lobby::LobbySettings::default(), 2);
        let wilds = FieldMap {
            choice: gc_map::GROWN,
            tiles: 512,
            seed: 0x6772_7562,
        };
        assert_eq!(key, MapKey::Field(wilds));
    }

    #[test]
    fn a_regiment_is_aimed_at_where_it_is_drawn() {
        let prev = Some((HANDLE, Vec2::new(10.0, 0.0), Vec2::X));
        let now = (Vec2::new(12.0, 0.0), Vec2::X);
        assert_eq!(pose_between(prev, HANDLE, now, 0.0).0, Vec2::new(10.0, 0.0));
        assert_eq!(pose_between(prev, HANDLE, now, 0.5).0, Vec2::new(11.0, 0.0));
        assert_eq!(pose_between(prev, HANDLE, now, 1.0).0, Vec2::new(12.0, 0.0));
    }

    #[test]
    fn a_reused_slot_is_drawn_where_it_is() {
        let other = RegimentHandle {
            generation: 2,
            ..HANDLE
        };
        let prev = Some((other, Vec2::new(99.0, 99.0), Vec2::Y));
        let now = (Vec2::new(12.0, 0.0), Vec2::X);
        assert_eq!(pose_between(prev, HANDLE, now, 0.5), now);
        assert_eq!(pose_between(None, HANDLE, now, 0.5), now);
    }
}
