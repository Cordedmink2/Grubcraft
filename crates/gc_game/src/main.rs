//! Grubcraft. M1 sandbox: a rendered map with four players' regiments driven
//! by the deterministic simulation. You command player 1 (red).
//!
//! Orders: left-click a red regiment to select it, or left-drag a box around
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
//! Camera: arrow keys / screen edges / two-finger scroll pan; mouse wheel or
//! pinch zooms; Alt + middle-drag or two-finger rotate turns the view;
//! middle-click or Home resets the rotation; ` (backquote) or F11 toggles the
//! dev overlay and prints memory by subsystem;
//! Esc quits.
//!
//! Flags: `--seed N` picks the match seed; `--frames N` quits after N frames
//! and prints a final stats line (for measuring memory on real machines);
//! `--overlay` starts with the dev overlay shown; `--latency 1|2|3` sets
//! frames in flight; `--render-scale F` renders at a
//! fraction of the window's pixels; `--draw all|terrain|blocks|none` turns
//! parts of the frame off to measure what they cost.

mod camera;
mod data;
mod profile;
mod puppets;
mod terrain_gen;

use std::sync::Arc;
use std::time::{Duration, Instant};

use gc_platform::tagged_alloc::{self, Tag, TaggedAlloc};
use gc_render::{BoxInstance, FrameParams, Renderer, RendererOptions, Terrain};
use gc_sim::formation::{self, GroupMember, GroupOrder};
use gc_sim::scenario::Drill;
use gc_sim::{
    Capacities, MatchSetup, MoraleState, PlayerCommand, PlayerId, RegimentHandle, UnitId,
    UnitStats, World,
};
use glam::Vec2;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use camera::RtsCamera;
use puppets::{Block, Puppets};
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
/// Placeholder soldier height in tiles (real models come with M1 step 8).
const SOLDIER_HEIGHT: f32 = 0.95;
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
const BANNER_LIFT: f32 = SOLDIER_HEIGHT + 0.7;
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

const _: () = assert!(gc_data::MAX_MODELS == gc_sim::formation::MAX_MODELS);

struct Options {
    seed: u64,
    frames: Option<u64>,
    overlay: bool,
    render: RendererOptions,
}

fn parse_args() -> Options {
    let mut opts = Options {
        seed: 0x6772_7562,
        frames: None,
        overlay: false,
        render: RendererOptions::default(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--overlay" {
            opts.overlay = true;
            continue;
        }
        let value = args.next().unwrap_or_default();
        let ok = match flag.as_str() {
            "--seed" => value.parse().map(|v| opts.seed = v).is_ok(),
            "--frames" => value.parse().map(|v| opts.frames = Some(v)).is_ok(),
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
        "usage: grubcraft [--seed N] [--frames N] [--overlay] [--latency 1|2|3] \
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
    drill: Drill,
    /// Commands collected for the next tick (from the drill and the human).
    pending: Vec<PlayerCommand>,
    human_orders: Vec<PlayerCommand>,
    /// Position and facing at the previous tick, by pool index, for interpolation.
    prev_positions: Vec<Option<(RegimentHandle, Vec2, Vec2)>>,
    instances: Vec<BoxInstance>,
    puppets: Puppets,
    /// The match's regiment types, as the simulation sees them.
    unit_stats: Vec<UnitStats>,
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
    last_frame: Instant,
    started: Instant,
    input: Input,
    stats: Stats,
    memory_timeline: MemoryTimeline,
    /// Whether the dev overlay is showing.
    overlay: bool,
    /// Marks over ground no regiment can cross, and whether to show them (G).
    blocked_marks: Vec<BoxInstance>,
    show_blocked: bool,
    /// Data hash and size, for the overlay.
    data_label: String,
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
        let unit_stats: Vec<UnitStats> = game_data
            .units
            .iter()
            .map(|u| UnitStats {
                speed: u.speed(),
                turn_rate: u.turn_rate,
                models: u.models,
                hp_per_model: u.hp_per_model,
                files: u.files,
                file_spacing: u.file_spacing(),
                rank_spacing: u.rank_spacing(),
                melee_attack: u.melee_attack,
                melee_defence: u.melee_defence,
                melee_damage: u.melee_damage,
                charge_bonus: u.charge_bonus,
                range: u.range(),
                missile_damage: u.missile_damage,
                reload_ticks: u.reload_ticks,
                accuracy: u.accuracy,
                morale: u.morale,
            })
            .collect();
        let capacities = Capacities::default();
        let nav = tagged_alloc::scope(Tag::Map, || heightmap.nav_costs());
        let blocked_marks = blocked_marks(&nav, MAP_TILES);
        let (world, prev_positions, pending) = tagged_alloc::scope(Tag::Sim, || {
            (
                World::new(&MatchSetup {
                    seed: opts.seed,
                    capacities,
                    units: &unit_stats,
                    map_tiles: MAP_TILES,
                    nav: &nav,
                }),
                vec![None; capacities.regiments as usize],
                Vec::with_capacity(1024),
            )
        });
        let mut drill = Drill::new(opts.seed);
        drill.skip_player = Some(HUMAN);
        // The last unit type is the archers: every army's back two rows.
        drill.unit_of = |player, k, types| {
            if types > 1 && k >= 40 {
                (types - 1) as UnitId
            } else {
                (player as usize % (types - 1).max(1)) as UnitId
            }
        };

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

        let now = Instant::now();
        self.running = Some(Running {
            window,
            renderer,
            camera,
            heightmap,
            world,
            drill,
            pending,
            human_orders: Vec::with_capacity(64),
            prev_positions,
            instances: Vec::with_capacity(gc_render::MAX_BOX_INSTANCES),
            puppets: Puppets::new(capacities.regiments as usize, MAX_CORPSES),
            unit_stats,
            selected: Vec::with_capacity(64),
            group: Vec::with_capacity(64),
            group_commands: Vec::with_capacity(64),
            boxed: Vec::with_capacity(64),
            control_groups: Default::default(),
            last_group_key: None,
            attack_mark: None,
            sim_clock: Duration::ZERO,
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
            },
            overlay: opts.overlay,
            blocked_marks,
            show_blocked: false,
            data_label: format!(
                "{:016x}   {} unit types",
                game_data.short_hash(),
                game_data.units.len()
            ),
            memory_timeline: std::mem::take(&mut self.memory_timeline),
        });
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
                    match code {
                        KeyCode::ArrowUp => r.input.arrows[0] = down,
                        KeyCode::ArrowDown => r.input.arrows[1] = down,
                        KeyCode::ArrowLeft => r.input.arrows[2] = down,
                        KeyCode::ArrowRight => r.input.arrows[3] = down,
                        KeyCode::Home if down => r.camera.reset_rotation(),
                        KeyCode::F11 | KeyCode::Backquote if down && !event.repeat => {
                            r.overlay = !r.overlay;
                            if r.overlay {
                                r.refresh_overlay();
                            } else {
                                let scale = r.window.scale_factor() as f32;
                                r.renderer.set_overlay(None, scale);
                            }
                            r.memory_timeline.print();
                            print_memory(&r.renderer);
                        }
                        KeyCode::KeyG if down && !event.repeat => {
                            r.show_blocked = !r.show_blocked;
                        }
                        KeyCode::Escape if down => event_loop.exit(),
                        _ if down && !event.repeat && digit(code).is_some() => {
                            r.control_group(digit(code).unwrap());
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                r.frame();
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
                    }
                    self.human_orders
                        .extend(self.selected.iter().map(|&regiment| PlayerCommand {
                            player: HUMAN,
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
                    self.human_orders
                        .extend(self.group_commands.iter().map(|&command| PlayerCommand {
                            player: HUMAN,
                            command,
                        }));
                }
                self.input.right_down = None;
            }
            _ => {}
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
        let [cr, cg, cb] = TEAM_COLORS[HUMAN as usize];
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
        for r in self.world.regiments.iter().filter(|r| r.owner == HUMAN) {
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
        let regiments = &self.world.regiments;
        self.control_groups[n].retain(|&h| regiments.get(h).is_some());
        if !self.input.shift {
            self.selected.clear();
        }
        for &h in &self.control_groups[n] {
            if !self.selected.contains(&h) {
                self.selected.push(h);
            }
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
            .filter(|r| r.owner != HUMAN)
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
            .filter(|r| r.owner == HUMAN)
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

        // Fixed-rate simulation; render interpolates between the last two ticks.
        self.sim_clock += dt;
        let mut ticks_this_frame = 0;
        while self.sim_clock >= TICK && ticks_this_frame < 5 {
            self.sim_clock -= TICK;
            ticks_this_frame += 1;
            for r in self.world.regiments.iter() {
                self.prev_positions[r.handle.index as usize] =
                    Some((r.handle, from_fx_vec(r.position), angle_vec(r.facing)));
            }
            self.drill.commands_for(&self.world, &mut self.pending);
            self.pending.append(&mut self.human_orders);
            let t0 = Instant::now();
            profile::zone!("sim tick");
            tagged_alloc::scope(Tag::Sim, || self.world.tick(&self.pending));
            let took = t0.elapsed();
            self.stats.tick_time += took;
            self.stats.tick_max = self.stats.tick_max.max(took);
            self.stats.ticks += 1;
        }
        let alpha = self.sim_clock.as_secs_f32() / TICK.as_secs_f32();

        self.instances.clear();
        self.boxed.clear();
        if let Some(rect) = self.selection_box() {
            self.regiments_in_box(rect);
        }
        if let Some(order) = self.drag_order() {
            self.plan_order(order);
            self.push_order_preview();
        }
        self.puppets.advance(dt.as_secs_f32());
        let marked = self
            .attack_mark
            .filter(|(_, at)| at.elapsed() < ATTACK_MARK)
            .map(|(h, _)| h);
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
            let [cr, cg, cb] = TEAM_COLORS[r.owner as usize % TEAM_COLORS.len()];
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
                        [cr, cg, cb, 255]
                    } else {
                        [255, 40, 20, 255]
                    },
                    lift: 0.0,
                });
            }
            let block = Block {
                handle: r.handle,
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
            if let Some(color) = banner {
                self.instances.push(BoxInstance {
                    pos: pos.to_array(),
                    facing: facing.to_array(),
                    size: BANNER_SIZE,
                    color,
                    lift: BANNER_LIFT,
                });
            }
            let size = [
                (file_spacing * 0.6).min(0.45),
                SOLDIER_HEIGHT,
                (rank_spacing * 0.42).min(0.35),
            ];
            let color = [cr, cg, cb, if selected { 70 } else { 0 }];
            let instances = &mut self.instances;
            self.puppets.update(&block, r.owner, dt.as_secs_f32(), |s| {
                instances.push(BoxInstance {
                    pos: s.pos.to_array(),
                    facing: s.facing.to_array(),
                    size,
                    color,
                    lift: 0.0,
                });
            });
        }
        self.push_corpses();
        self.push_arrows(alpha);
        self.push_routes();
        if self.show_blocked {
            self.instances.extend_from_slice(&self.blocked_marks);
        }

        let frame = FrameParams {
            view_proj: self.camera.view_proj(),
            camera_pos: self.camera.eye(),
            time_seconds: self.started.elapsed().as_secs_f32(),
        };
        {
            profile::zone!("render");
            tagged_alloc::scope(Tag::Render, || {
                self.renderer.render(&frame, &self.instances)
            });
        }
        profile::frame_mark();

        self.stats.frames += 1;
        self.stats.window_frames += 1;
        if self.stats.window_start.elapsed() >= Duration::from_millis(500) {
            self.window.set_title(&self.stats_line());
            if self.overlay {
                self.refresh_overlay();
            }
            self.stats.window_frames = 0;
            self.stats.window_start = Instant::now();
            self.stats.frame_max = Duration::ZERO;
            self.stats.tick_time = Duration::ZERO;
            self.stats.tick_max = Duration::ZERO;
            self.stats.ticks = 0;
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
            let [cr, cg, cb] = TEAM_COLORS[r.owner as usize % TEAM_COLORS.len()];
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
            let [r, g, b] = TEAM_COLORS[c.owner as usize % TEAM_COLORS.len()];
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
        let mut dx = 0.0;
        let mut dy = 0.0;
        let [up, down, left, right] = self.input.arrows;
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
        if let (Some(c), true) = (self.input.cursor, self.input.focused) {
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
        // Follow the ground smoothly so the view doesn't jolt over ridges.
        let ground = self
            .heightmap
            .height_at(self.camera.focus.x, self.camera.focus.y);
        let k = 1.0 - (-dt * 6.0).exp();
        self.camera.focus_height += (ground - self.camera.focus_height) * k;
    }

    /// Rebuild the dev overlay's text from the current stats window.
    fn refresh_overlay(&mut self) {
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
             render  {} draw calls   {}/{} chunks   {} boxes\n\
             memory  process {process}   heap {:.1} MB   gpu {:.1} MB\n\
             heap    {}\n\
             data    {}\n\
             gpu     {} via {:?}",
            ms(self.stats.frame_max),
            self.world.tick_count(),
            ms(self.stats.tick_max),
            self.world.nav().portal_count(),
            self.world.nav().queued(),
            rs.draw_calls,
            rs.chunks_drawn,
            rs.chunks_total,
            rs.boxes_drawn,
            mb(tagged_alloc::total_live_bytes()),
            mb(self.renderer.gpu_bytes() as usize),
            heap_tags.join("  "),
            self.data_label,
            info.name,
            info.backend,
        );
        let scale = self.window.scale_factor() as f32;
        self.renderer.set_overlay(Some(&text), scale);
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
            "Grubcraft · {fps:.0} fps · sim {tick_ms:.3} ms/tick · {} regiments · {}/{} chunks · {} MB process · {} MB heap · {} MB gpu",
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
