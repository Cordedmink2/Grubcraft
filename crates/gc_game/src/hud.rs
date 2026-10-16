//! The battle HUD's glue: describe the match to `gc_ui::hud`, and carry out
//! what it reports. Docs: docs/08-ui-ux.md#screen-layout-1080p-reference.

use gc_sim::formation::GroupOrder;
use gc_sim::{MoraleState, PlayerId, RegimentHandle};
use gc_ui::DrawList;
use gc_ui::alerts::{Alert, AlertData, AlertKind};
use gc_ui::hero::{HeroAbility, HeroPanel};
use gc_ui::hud::{
    Card, CardState, DeployStatus, Dot, GRID_SLOTS, GridSlot, HERO_SLOT_FIRST, Hud, HudData,
    HudEvent, HudInput, MinimapData, MinimapTerrain, SlotState,
};
use glam::{Vec2, Vec3};

use crate::terrain_gen::Heightmap;
use crate::{Running, from_fx_vec, fx_vec, sounds};

/// Cells per side of the minimap's ground picture.
const TERRAIN_CELLS: usize = 48;

/// A plain move to `p`: the units choose their own facing and frontage.
pub fn move_order(p: Vec2) -> GroupOrder {
    GroupOrder {
        target: fx_vec(p),
        facing: None,
        width: None,
    }
}

/// An order that waits for the click that completes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Armed {
    Move,
    Attack,
}

/// Slots of the command grid, row by row (docs/08-ui-ux.md#screen-layout-1080p-reference).
pub const SLOT_MOVE: usize = 0;
pub const SLOT_ATTACK: usize = 1;
pub const SLOT_FORMATION: usize = 3;
/// The first of the four slots (A, S, D, F) a hero's abilities use.
// Run (C) and Retire (V) share keys with the sandbox's credits and reveal-all
// tools; those move when the two orders arrive.
const LABELS: [&str; GRID_SLOTS] = [
    "Move", "Attack", "Hold", "Form", "Skill1", "Skill2", "Skill3", "Ultim", "Skirm", "Charge",
    "Run", "Retire",
];

/// An alert stays this long (in sim ticks, so a paused game keeps it) after
/// the last time it happened.
const NOTE_LIFE: u32 = 12 * gc_sim::TICKS_PER_SECOND;
/// ... and is drawn filled for this long after it first happened.
const NOTE_FRESH: u32 = 2 * gc_sim::TICKS_PER_SECOND;
/// Events of one kind this close (in tiles) are one alert.
const NOTE_MERGE_TILES: f32 = 20.0;
const MAX_NOTES: usize = 8;

/// Something that happened to your side, remembered for a few seconds so an
/// arrow can point at it and Space can jump to it.
#[derive(Clone, Copy, Debug)]
struct Note {
    id: u32,
    kind: AlertKind,
    /// Where, in tiles.
    at: Vec2,
    /// Sim ticks when it first happened and when it last did.
    born: u32,
    last: u32,
}

/// What a regiment of yours looked like last frame.
#[derive(Clone, Copy, Debug)]
struct Seen {
    handle: RegimentHandle,
    models: u16,
    routing: bool,
}

/// Everything the HUD keeps between frames.
#[derive(Default)]
pub struct HudState {
    hud: Hud,
    pub draw: DrawList,
    /// Mouse events collected since the last frame (position is filled in
    /// when the HUD runs).
    input: HudInput,
    /// Q or W was pressed: the next left click says where to go or whom to
    /// attack.
    pub armed: Option<Armed>,
    /// A button went down on the bar (or the minimap drag), so its release
    /// is the bar's, wherever the mouse is by then.
    left_held: bool,
    right_held: bool,
    /// Indices of the selected regiments, sorted; scratch for each frame.
    picked: Vec<u32>,
    terrain: MinimapTerrain,
    dots: Vec<Dot>,
    cards: Vec<Card>,
    view: Vec<[f32; 2]>,
    events: Vec<HudEvent>,
    notes: Vec<Note>,
    next_note: u32,
    /// By regiment index; your side's regiments as of the last frame.
    seen: Vec<Option<Seen>>,
    /// Whose regiments `seen` and `notes` are about.
    watched_side: Option<PlayerId>,
    /// The tick the regiments were last compared at.
    watched_tick: u32,
    alerts: Vec<Alert>,
    /// Dev preview of the hero panel (Shift+H); there are no heroes yet.
    hero_preview: bool,
}

impl HudState {
    pub fn set_terrain(&mut self, hm: &Heightmap, tiles: u32) {
        self.terrain = minimap_terrain(hm, tiles);
    }

    pub fn press_left(&mut self) {
        self.input.left_pressed = true;
        self.left_held = true;
    }

    pub fn release_left(&mut self) {
        self.input.left_released = true;
        self.left_held = false;
    }

    pub fn left_captured(&self) -> bool {
        self.left_held
    }

    pub fn press_right(&mut self) {
        self.input.right_pressed = true;
        self.right_held = true;
    }

    pub fn release_right(&mut self) {
        self.right_held = false;
    }

    pub fn right_captured(&self) -> bool {
        self.right_held
    }

    /// Show or hide the made-up hero (there are no heroes in the game yet).
    /// Whether a hero is on show, and so owns the A, S, D and F keys.
    pub fn hero_shown(&self) -> bool {
        self.hero_preview
    }

    pub fn toggle_hero_preview(&mut self) {
        self.hero_preview = !self.hero_preview;
    }

    /// Forget any half-finished click, e.g. when a menu opens over the bar.
    pub fn reset(&mut self) {
        self.hud = Hud::default();
        self.armed = None;
        self.input = HudInput::default();
        self.left_held = false;
        self.right_held = false;
    }

    pub fn wants(&self, screen: (i32, i32), mouse: (i32, i32)) -> bool {
        self.hud.wants(screen, mouse)
    }

    /// Remember that `kind` happened at `at` (tiles). A repeat of one that
    /// is still showing nearby only keeps it alive.
    pub fn note(&mut self, kind: AlertKind, at: Vec2, tick: u32) {
        let near = |n: &Note| n.at.distance(at) <= NOTE_MERGE_TILES;
        // An ambush explains the losses it causes.
        if kind == AlertKind::Attack
            && let Some(n) = self
                .notes
                .iter_mut()
                .find(|n| n.kind == AlertKind::Ambush && near(n))
        {
            n.last = tick;
            return;
        }
        if kind == AlertKind::Ambush {
            self.notes
                .retain(|n| n.kind != AlertKind::Attack || !near(n));
        }
        if let Some(n) = self.notes.iter_mut().find(|n| n.kind == kind && near(n)) {
            n.at = at;
            n.last = tick;
            return;
        }
        if self.notes.len() >= MAX_NOTES {
            self.notes.remove(0);
        }
        self.next_note += 1;
        self.notes.push(Note {
            id: self.next_note,
            kind,
            at,
            born: tick,
            last: tick,
        });
    }

    /// Notice your regiments losing men or breaking since the last frame.
    /// Called once a sim tick.
    pub fn watch(&mut self, world: &gc_sim::World, side: PlayerId) {
        let tick = world.tick_count();
        // A new side, or a new world (its clock starts over): forget the old.
        if self.watched_side != Some(side) || tick < self.watched_tick {
            self.watched_side = Some(side);
            self.notes.clear();
            self.seen.clear();
        }
        self.watched_tick = tick;
        self.notes
            .retain(|n| tick.saturating_sub(n.last) <= NOTE_LIFE);
        for r in world.regiments.iter().filter(|r| r.owner == side) {
            let routing = r.morale.broken();
            let i = r.handle.index as usize;
            if self.seen.len() <= i {
                self.seen.resize(i + 1, None);
            }
            if let Some(was) = self.seen[i].filter(|w| w.handle == r.handle) {
                // A regiment already running has its own alert.
                if r.models < was.models && !routing {
                    self.note(AlertKind::Attack, from_fx_vec(r.position), tick);
                }
                if routing && !was.routing {
                    self.note(AlertKind::Rout, from_fx_vec(r.position), tick);
                }
            }
            self.seen[i] = Some(Seen {
                handle: r.handle,
                models: r.models,
                routing,
            });
        }
    }

    /// The alert that happened most recently.
    fn latest_note(&self) -> Option<Note> {
        self.notes.iter().max_by_key(|n| n.last).copied()
    }
}

/// The ground as colours: water, woods, grass and rising hide.
fn minimap_terrain(hm: &Heightmap, tiles: u32) -> MinimapTerrain {
    let n = tiles as f32;
    MinimapTerrain::from_fn(TERRAIN_CELLS, |u, v| {
        let (x, z) = (u * n, v * n);
        let h = hm.height_at(x, z);
        if h < hm.water_level() {
            return [44, 84, 128, 255];
        }
        let cover = hm
            .cover
            .get(z as usize * tiles as usize + x as usize)
            .copied()
            .map(gc_sim::Cover::from_u8);
        match cover {
            Some(gc_sim::Cover::Wood) => [34, 78, 44, 255],
            Some(gc_sim::Cover::Grass) => [104, 140, 70, 255],
            _ => {
                // A few shades, so neighbouring cells merge into one rectangle.
                let t = ((h / hm.max_height()).clamp(0.0, 1.0) * 6.0).round() / 6.0;
                let lerp = |a: f32, b: f32| (a + (b - a) * t) as u8;
                [
                    lerp(120.0, 168.0),
                    lerp(132.0, 148.0),
                    lerp(86.0, 116.0),
                    255,
                ]
            }
        }
    })
}

/// Hand a frame of UI quads to the renderer.
pub fn upload_ui(
    renderer: &mut gc_render::Renderer,
    buf: &mut Vec<gc_render::UiQuad>,
    draw: &DrawList,
    scale: f32,
) {
    buf.clear();
    buf.extend(draw.quads.iter().map(|q| gc_render::UiQuad {
        pos: q.pos,
        size: q.size,
        glyph: q.glyph,
        color: q.color,
    }));
    renderer.set_ui(buf, scale);
}

impl Running {
    /// The screen in UI pixels, as the menus see it.
    pub fn ui_screen(&self) -> ((i32, i32), f32) {
        let scale = self.ui_scale();
        let size = self.window.inner_size();
        let screen = (
            (size.width as f32 / scale) as i32,
            (size.height as f32 / scale) as i32,
        );
        (screen, scale)
    }

    /// The cursor in UI pixels; off-screen when the mouse has left the window.
    pub fn ui_cursor(&self) -> (i32, i32) {
        let scale = self.ui_scale();
        self.input.cursor.map_or((-1, -1), |c| {
            ((c.x as f32 / scale) as i32, (c.y as f32 / scale) as i32)
        })
    }

    /// Whether a mouse click now belongs to the HUD, not the battlefield.
    pub fn hud_wants_mouse(&self) -> bool {
        self.match_started
            && !self.menu_open
            && self.hud.wants(self.ui_screen().0, self.ui_cursor())
    }

    /// Draw the HUD over the battle and carry out what was clicked.
    pub fn run_hud(&mut self) {
        let (screen, scale) = self.ui_screen();
        let cursor = self.ui_cursor();
        let side = self.sandbox.side;
        let map =
            self.heightmap.size.saturating_sub(1) as f32 / self.heightmap.samples_per_tile as f32;
        self.selected
            .retain(|&h| self.world.regiments.get(h).is_some());

        let now_ms = self.now_ms();
        // A replay shows what happened to nobody in particular.
        if self.watching.is_some() {
            self.hud.notes.clear();
        }
        let tick = self.world.tick_count();

        let hud = &mut self.hud;
        if self.selected.is_empty() {
            hud.armed = None;
        }
        hud.dots.clear();
        hud.cards.clear();
        hud.picked.clear();
        hud.picked.extend(self.selected.iter().map(|h| h.index));
        hud.picked.sort_unstable();
        let picked = |h: gc_sim::RegimentHandle| hud.picked.binary_search(&h.index).is_ok();
        // The tray shows the selection, or else the group last recalled.
        let (shown, title): (&[_], _) = match self.last_group_key {
            _ if !self.selected.is_empty() => (
                &self.selected,
                format!("Selected ({})", self.selected.len()),
            ),
            Some((n, _)) => (
                &self.control_groups[n],
                format!("Group {}", (n + 9) % 10 + 1),
            ),
            None => (&[], String::new()),
        };
        for r in self.world.regiments.iter() {
            let seen = r.owner == side || self.reveal_all || self.world.sees(side, r.handle);
            if !seen {
                continue;
            }
            let p = from_fx_vec(r.position);
            hud.dots.push(Dot {
                pos: [p.x, p.y],
                color: crate::color_of(&self.parties, r.owner),
                selected: picked(r.handle),
            });
        }
        for &h in shown {
            let Some(r) = self.world.regiments.get(h) else {
                continue;
            };
            if r.owner != side {
                continue;
            }
            hud.cards.push(Card {
                id: h.index,
                name: self.unit_names[r.unit as usize].clone(),
                color: crate::color_of(&self.parties, r.owner),
                models: r.models,
                max_models: self.unit_stats[r.unit as usize].models,
                morale: r.morale_points,
                state: match r.morale {
                    MoraleState::Steady => CardState::Steady,
                    MoraleState::Wavering => CardState::Wavering,
                    MoraleState::Routing | MoraleState::Shattered => CardState::Routing,
                },
                engaged: r.engaged,
                selected: picked(h),
            });
        }
        hud.view.clear();
        for ndc in [(-1.0, 1.0), (1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
            hud.view.push(view_corner(&self.camera, ndc, map));
        }

        let hero = hud
            .hero_preview
            .then(|| preview_hero(crate::color_of(&self.parties, side)));
        let ability_status: [String; 4] = match &hero {
            Some(h) => h.abilities.map(|a| a.status()),
            None => Default::default(),
        };
        // A replay plays its recorded orders and nobody else's.
        let has_orders = !self.selected.is_empty() && self.watching.is_none();
        let ready = |on: bool| if on { SlotState::Ready } else { SlotState::Off };
        let mut grid: [GridSlot; GRID_SLOTS] = std::array::from_fn(|i| GridSlot {
            label: LABELS[i],
            ..GridSlot::default()
        });
        grid[SLOT_MOVE].state = ready(has_orders);
        grid[SLOT_ATTACK].state = ready(has_orders);
        grid[SLOT_FORMATION].state = ready(has_orders);
        // A hero's abilities take the A, S, D and F slots.
        if let Some(h) = &hero {
            for (i, a) in h.abilities.iter().enumerate() {
                let slot = &mut grid[HERO_SLOT_FIRST + i];
                slot.label = a.short;
                slot.detail = &ability_status[i];
                slot.state =
                    ready(a.usable() && h.revive_secs.is_none() && self.watching.is_none());
            }
        }
        match hud.armed {
            Some(Armed::Move) => grid[SLOT_MOVE].state = SlotState::Armed,
            Some(Armed::Attack) => grid[SLOT_ATTACK].state = SlotState::Armed,
            None => {}
        }
        if let Some(Some(f)) = self.shown_formation {
            grid[SLOT_FORMATION].detail = f.name();
        }

        hud.alerts.clear();
        if !hud.notes.is_empty() {
            let view_proj = self.camera.view_proj();
            let plane = self.camera.focus_height;
            hud.alerts.extend(hud.notes.iter().map(|n| Alert {
                id: n.id,
                kind: n.kind,
                at: project(view_proj, Vec3::new(n.at.x, plane, n.at.y), screen),
                fresh: tick.saturating_sub(n.born) <= NOTE_FRESH,
            }));
        }

        let mut input = std::mem::take(&mut hud.input);
        input.mouse = cursor;
        input.shift = self.input.shift;
        input.now_ms = now_ms;
        hud.draw.clear();
        hud.events.clear();
        let data = HudData {
            grid: &grid,
            hero,
            minimap: MinimapData {
                map_tiles: map,
                terrain: &hud.terrain,
                dots: &hud.dots,
                view: &hud.view,
            },
            cards: &hud.cards,
            tray_title: &title,
            // Nobody deploys in a replay.
            deployment: self
                .world
                .deploy_zone(side)
                .filter(|_| self.watching.is_none())
                .map(|_| DeployStatus {
                    seconds_left: self
                        .world
                        .deploy_ticks_left()
                        .div_ceil(gc_sim::TICKS_PER_SECOND),
                    ready: self.world.deploy_ready(side),
                }),
            alerts: AlertData {
                alerts: &hud.alerts,
                // Workers arrive with the Conquest economy.
                idle_workers: None,
            },
        };
        hud.hud
            .frame(&mut hud.draw, screen, input, &data, &mut hud.events);

        upload_ui(
            &mut self.renderer,
            &mut self.ui_quads,
            &self.hud.draw,
            scale,
        );

        let events = std::mem::take(&mut self.hud.events);
        for e in &events {
            self.apply_hud_event(*e);
        }
        self.hud.events = events;
    }

    fn apply_hud_event(&mut self, event: HudEvent) {
        match event {
            HudEvent::Jump([x, z]) => self.look_at(Vec2::new(x, z)),
            HudEvent::Centre(id) => {
                if let Some(p) = self
                    .own_regiment(id)
                    .and_then(|h| self.world.regiments.position(h))
                {
                    self.look_at(from_fx_vec(p));
                }
            }
            HudEvent::Select { id, add } => {
                let Some(h) = self.own_regiment(id) else {
                    return;
                };
                if add {
                    self.toggle_selected(h);
                } else {
                    self.selected.clear();
                    self.selected.push(h);
                }
                self.ui_sound(sounds::UiSound::Select);
            }
            HudEvent::Command(slot) => self.grid_press(slot),
            HudEvent::Alert(id) => {
                if let Some(n) = self.hud.notes.iter().find(|n| n.id == id) {
                    self.look_at(n.at);
                }
            }
            // Nothing sends this until there are workers to find.
            HudEvent::IdleWorker => {}
            HudEvent::Ready => {
                self.human_orders.push(gc_sim::PlayerCommand {
                    player: self.sandbox.side,
                    command: gc_sim::Command::Ready,
                });
                self.ui_sound(sounds::UiSound::Select);
            }
            HudEvent::Order([x, z]) => {
                if !self.selected.is_empty() {
                    self.issue_group_order(move_order(Vec2::new(x, z)));
                }
            }
        }
    }

    /// A grid button clicked or its key pressed. Slots whose orders the
    /// simulation doesn't have yet do nothing.
    pub fn grid_press(&mut self, slot: usize) {
        if self.selected.is_empty() || self.watching.is_some() || self.sandbox.placing {
            return;
        }
        match slot {
            SLOT_MOVE | SLOT_ATTACK => {
                let want = if slot == SLOT_MOVE {
                    Armed::Move
                } else {
                    Armed::Attack
                };
                // Pressing it again puts it down.
                self.hud.armed = (self.hud.armed != Some(want)).then_some(want);
            }
            SLOT_FORMATION => self.cycle_formation(),
            _ => {}
        }
    }

    /// The click after Q or W: send the selection there, or at the enemy there.
    pub fn finish_armed_order(&mut self) {
        let (Some(armed), Some(p)) = (self.hud.armed, self.ground_under_cursor()) else {
            return;
        };
        // A click that finds nothing to attack keeps the order armed.
        match armed {
            Armed::Move => self.issue_group_order(move_order(p)),
            Armed::Attack => match self.enemy_at(p) {
                Some(target) => self.issue_attack(target),
                None => return,
            },
        }
        self.hud.armed = None;
    }

    fn own_regiment(&self, id: u32) -> Option<gc_sim::RegimentHandle> {
        self.world
            .regiments
            .iter()
            .find(|r| r.handle.index == id && r.owner == self.sandbox.side)
            .map(|r| r.handle)
    }

    /// Milliseconds since the game started, on the clock the HUD uses.
    pub fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Space: look at the latest alert.
    pub fn jump_to_alert(&mut self) {
        if let Some(n) = self.hud.latest_note() {
            self.look_at(n.at);
        }
    }

    pub fn look_at(&mut self, p: Vec2) {
        self.camera.focus = p.clamp(Vec2::ZERO, Vec2::splat(self.camera.map_size));
        self.camera.focus_height = self
            .heightmap
            .height_at(self.camera.focus.x, self.camera.focus.y);
    }
}

/// A made-up Daimyo for looking at the panel and the grid it feeds: level 3,
/// wounded, one ability cooling down (docs/17-unit-rosters.md).
fn preview_hero(color: [u8; 3]) -> HeroPanel<'static> {
    let ability = |short, rank, passive, cooldown_secs| HeroAbility {
        short,
        rank,
        max_rank: 3,
        passive,
        cooldown_secs,
    };
    HeroPanel {
        name: "The Daimyo",
        role: "Commander",
        color,
        level: 3,
        hp: 212,
        max_hp: 340,
        xp: 0.4,
        abilities: [
            ability("Rally", 2, false, 0),
            ability("Aura", 1, true, 0),
            ability("Chall", 1, false, 12),
            ability("Banner", 0, false, 0),
        ],
        items: [Some("Jade Charm"), None, None],
        revive_secs: None,
    }
}

/// Where the ground point `p` appears on a screen of `screen` UI pixels, which
/// may be far outside it. A point behind the camera lands on the opposite
/// side of where it projects, so the arrow still points the right way.
fn project(view_proj: glam::Mat4, p: Vec3, screen: (i32, i32)) -> [f32; 2] {
    let clip = view_proj * p.extend(1.0);
    let ndc = if clip.w > 1e-4 {
        Vec2::new(clip.x, clip.y) / clip.w
    } else {
        -Vec2::new(clip.x, clip.y).normalize_or(Vec2::Y) * 1000.0
    };
    [
        (ndc.x * 0.5 + 0.5) * screen.0 as f32,
        (0.5 - ndc.y * 0.5) * screen.1 as f32,
    ]
}

/// Where the screen corner `ndc` looks at the ground, in tiles, clamped to
/// the map. The ground is taken as flat at the focus's height.
fn view_corner(camera: &crate::camera::RtsCamera, ndc: (f32, f32), map: f32) -> [f32; 2] {
    let (origin, dir) = camera.ray(Vec2::new(ndc.0, ndc.1));
    let plane = camera.focus_height;
    let hit: Vec3 = if dir.y < -1e-4 {
        origin + dir * ((plane - origin.y) / dir.y)
    } else {
        // Looking at the sky: far along the ray's heading instead.
        origin + dir * 1000.0
    };
    [hit.x.clamp(0.0, map), hit.z.clamp(0.0, map)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::RtsCamera;

    #[test]
    fn the_focus_is_the_middle_of_the_screen_and_far_ground_is_off_it() {
        let cam = RtsCamera::new(512.0, Vec2::new(100.0, 100.0));
        let screen = (1280, 760);
        let at = |x: f32, z: f32| project(cam.view_proj(), Vec3::new(x, 0.0, z), screen);
        let mid = at(100.0, 100.0);
        assert!(
            (mid[0] - 640.0).abs() < 1.0 && (mid[1] - 380.0).abs() < 1.0,
            "{mid:?}"
        );
        // Along +x the view is to the right of the middle, along -x to the left.
        assert!(at(300.0, 100.0)[0] > 1280.0);
        assert!(at(-100.0, 100.0)[0] < 0.0);
        // Far behind the camera still lands on a definite side, not NaN.
        let behind = at(100.0, 100.0 + 5000.0);
        assert!(behind.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn repeats_nearby_are_one_alert_and_an_ambush_explains_its_losses() {
        let mut h = HudState::default();
        let a = Vec2::new(50.0, 50.0);
        h.note(AlertKind::Attack, a, 0);
        h.note(AlertKind::Attack, a + Vec2::splat(5.0), 500);
        assert_eq!(h.notes.len(), 1, "same fight");
        assert_eq!(h.notes[0].last, 500);
        assert_eq!(h.notes[0].born, 0, "still the old alert");
        h.note(AlertKind::Attack, Vec2::new(300.0, 300.0), 600);
        assert_eq!(h.notes.len(), 2, "another fight");
        h.note(AlertKind::Ambush, a, 700);
        assert!(
            !h.notes
                .iter()
                .any(|n| n.kind == AlertKind::Attack && n.at.distance(a) < 20.0),
            "the ambush replaces the losses it caused"
        );
        h.note(AlertKind::Attack, a, 800);
        assert_eq!(
            h.notes.iter().filter(|n| n.at.distance(a) < 20.0).count(),
            1
        );
        assert_eq!(h.latest_note().unwrap().kind, AlertKind::Ambush);
        assert_eq!(
            h.latest_note().unwrap().last,
            800,
            "the fight keeps it alive"
        );
    }

    #[test]
    fn only_the_newest_few_alerts_are_kept() {
        let mut h = HudState::default();
        for i in 0..MAX_NOTES + 3 {
            h.note(AlertKind::Rout, Vec2::new(i as f32 * 100.0, 0.0), i as u32);
        }
        assert_eq!(h.notes.len(), MAX_NOTES);
        assert_eq!(
            h.latest_note().unwrap().at.x,
            (MAX_NOTES + 2) as f32 * 100.0
        );
    }
}
