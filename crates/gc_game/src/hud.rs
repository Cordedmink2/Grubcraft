//! The battle HUD's glue: describe the match to `gc_ui::hud`, and carry out
//! what it reports. Docs: docs/08-ui-ux.md#screen-layout-1080p-reference.

use crate::{MatchKind, UnitTable, roster};
use gc_sim::events::Event;
use gc_sim::formation::GroupOrder;
use gc_sim::gates::GateMode;
use gc_sim::{MoraleState, PlayerId, StructureHandle};
use gc_ui::DrawList;
use gc_ui::Icon;
use gc_ui::alerts::{Alert, AlertData, AlertKind};
use gc_ui::conquest::{
    Block, NodeDot, QUEUE_SLOTS, QueueItem, StructurePanel, TipLine, TipsCard, TopBar, Tuning,
};
use gc_ui::hero::{HeroAbility, HeroPanel};
use gc_ui::hud::{
    Card, CardState, DeployStatus, Dot, GRID_SLOTS, GridSlot, HERO_SLOT_FIRST, HandOverButton,
    HelpScroll, Hud, HudData, HudEvent, HudInput, MinimapData, MinimapTerrain, Selection,
    ShapeChip, SlotState, StatusStrip,
};
use gc_ui::placer::{PlaceLater, PlacePicker, PlaceRow, PlaceSide};
use gc_ui::powers::{POWER_KEYS, PowerButton};
use glam::{Vec2, Vec3};

use crate::conquest_card::Action;
use crate::{Running, from_fx_vec, fx_vec, sounds};
use gc_map::{Kind, Map};
use gc_match::battle::party_name;

/// Cells per side of the minimap's ground picture: 48 up to a 384-tile
/// map, then one for every 8 tiles, up to 80 (a 768 map's cells are
/// 9.6 tiles across).
fn terrain_cells(tiles: u32) -> usize {
    (tiles as usize / 8).clamp(48, 80)
}

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
    /// Conquest: where a worker lays this building.
    Lay(gc_sim::BuildingId),
    /// Conquest: a wall of this building, drawn by clicking its start and
    /// then each place it bends or ends. The tile is the last click (the
    /// tile the wall goes on from), once there is one.
    Wall(gc_sim::BuildingId, Option<(i32, i32)>),
    /// Conquest: the ground a worker clears, chosen by a left-drag.
    Clear,
    /// Conquest: the target of this faction power (its place in the
    /// powers table).
    Power(u8),
}

/// Slots of the command grid, row by row (docs/08-ui-ux.md#screen-layout-1080p-reference).
pub const SLOT_MOVE: usize = 0;
pub const SLOT_ATTACK: usize = 1;
pub const SLOT_FORMATION: usize = 3;
/// Brace (Z), sharing its slot with Skirmish when that order arrives, and
/// Fall back (V).
pub const SLOT_BRACE: usize = 8;
pub const SLOT_FALL_BACK: usize = 11;
/// The first of the four slots (A, S, D, F) a hero's abilities use.
// Run (C) shares its key with the sandbox's credits tool, which moves when
// the order arrives.
const LABELS: [&str; GRID_SLOTS] = [
    "Move",
    "Attack",
    "Hold",
    "Formation",
    "Skill 1",
    "Skill 2",
    "Skill 3",
    "Ultimate",
    "Brace",
    "Charge",
    "Run",
    "Fall back",
];

/// The picture on each battle slot.
const ICONS: [Icon; GRID_SLOTS] = [
    Icon::Move,
    Icon::Attack,
    Icon::Hold,
    Icon::Formation,
    Icon::Ring,
    Icon::Ring,
    Icon::Ring,
    Icon::Ultimate,
    Icon::Brace,
    Icon::Charge,
    Icon::Run,
    Icon::FallBack,
];

/// One line on each battle slot's tooltip.
const TIPS: [&str; GRID_SLOTS] = [
    "March the selected regiments to a spot. Then click the ground.",
    "Send the selection against an enemy. Then click the enemy.",
    "Stand and fight where you are. Not in the game yet.",
    "Change the formation: Line, Tight, Loose, Shield wall.",
    "A hero ability. Needs a hero.",
    "A hero ability. Needs a hero.",
    "A hero ability. Needs a hero.",
    "A hero's ultimate. Needs a hero.",
    "Spears and pikes brace against a charge. Press again to stand.",
    "Charge at the run for a heavier first blow. Not in the game yet.",
    "Run at full pace, tiring as you go. Not in the game yet.",
    "Back out of the fight in good order, taking one parting blow.",
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

#[derive(Clone, Copy)]
struct Readout {
    tick: u32,
    side: PlayerId,
    top: TopBar,
    idle_workers: Option<u32>,
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
    /// Conquest: the wall as it would be drawn now, worked out once a
    /// frame for both its ghost and its hint.
    pub wall_preview: Option<crate::wall_tool::Preview>,
    /// Conquest: the workers' build menu is open on the grid.
    pub build_menu: bool,
    /// Conquest: the selected building's tips are showing (its info button
    /// is on). Kept between selections and matches.
    tips_open: bool,
    /// The Conquest card's slot names and prices, kept for their room.
    card_labels: [String; GRID_SLOTS],
    card_details: [String; GRID_SLOTS],
    /// The faction power buttons' names and second lines, likewise.
    power_names: [String; POWER_KEYS.len()],
    power_details: [String; POWER_KEYS.len()],
    /// A button went down on the bar (or the minimap drag), so its release
    /// is the bar's, wherever the mouse is by then.
    left_held: bool,
    right_held: bool,
    /// Indices of the selected regiments, sorted; scratch for each frame.
    picked: Vec<u32>,
    terrain: MinimapTerrain,
    dots: Vec<Dot>,
    nodes: Vec<NodeDot>,
    blocks: Vec<Block>,
    /// Conquest's top bar and idle-worker count, and the tick and side
    /// they were read for: they only change on a tick.
    readout: Option<Readout>,
    cards: Vec<Card>,
    view: Vec<[f32; 2]>,
    events: Vec<HudEvent>,
    notes: Vec<Note>,
    next_note: u32,
    /// Whose regiments `notes` are about.
    watched_side: Option<PlayerId>,
    /// The tick the events were last read at.
    watched_tick: u32,
    alerts: Vec<Alert>,
    /// Dev preview of the hero panel (Shift+H); there are no heroes yet.
    hero_preview: bool,
    /// The Marshal's Scroll is open (H, F1 or the Help button).
    help: bool,
    /// The Place panel is open (the Place button, or B in the sandbox).
    pub placer: bool,
    /// The Place panel's tab: 0 the Field types, 1 all factions (the roster).
    pub place_tab: u8,
    /// The faction chip chosen on the roster tab, an index into
    /// `gc_ai::Faction::ALL`.
    pub place_faction: u8,
    /// The Place panel's rows for the tab, faction and table showing, built
    /// once ([`Running::place_body`]).
    place_body: std::cell::RefCell<Option<std::rc::Rc<PlaceBody>>>,
}

impl HudState {
    pub fn set_terrain(&mut self, hm: &Map) {
        self.terrain = minimap_terrain(hm);
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

    pub fn help_open(&self) -> bool {
        self.help
    }

    pub fn toggle_hero_preview(&mut self) {
        self.hero_preview = !self.hero_preview;
    }

    /// A new match: what the old one's top bar read is stale.
    pub fn forget_readout(&mut self) {
        self.readout = None;
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

    /// Notice your regiments losing men or breaking, and your buildings
    /// struck, from what the simulation told of the last tick (`events`).
    /// Called once a sim tick.
    pub fn watch(&mut self, world: &gc_sim::World, side: PlayerId, events: &[Event]) {
        let tick = world.tick_count();
        // A new side, or a new world (its clock starts over): forget the old.
        if self.watched_side != Some(side) || tick < self.watched_tick {
            self.watched_side = Some(side);
            self.notes.clear();
        }
        self.watched_tick = tick;
        self.notes
            .retain(|n| tick.saturating_sub(n.last) <= NOTE_LIFE);
        for e in events {
            // A building of ours under attack points where it stands.
            if let Event::Struck { structure, .. } = *e {
                let ours = (world.structures().and_then(|all| all.get(structure)))
                    .filter(|b| b.owner == side);
                if let Some(b) = ours {
                    let at = crate::buildings::centre_of(b.tile, b.width, b.depth);
                    self.note(AlertKind::Attack, at, tick);
                }
                continue;
            }
            let (regiment, kind) = match *e {
                Event::Losses { regiment, .. } => (regiment, AlertKind::Attack),
                Event::Morale { regiment, from, to } if to.broken() && !from.broken() => {
                    (regiment, AlertKind::Rout)
                }
                _ => continue,
            };
            let Some(r) = world.regiments.get(regiment).filter(|r| r.owner == side) else {
                continue;
            };
            // A regiment already running has its own alert.
            if kind == AlertKind::Attack && r.morale.broken() {
                continue;
            }
            self.note(kind, from_fx_vec(r.position), tick);
        }
    }

    /// The alert that happened most recently.
    fn latest_note(&self) -> Option<Note> {
        self.notes.iter().max_by_key(|n| n.last).copied()
    }
}

/// The colour a site stands out in on a Field Battle's minimap: villages
/// amber, outposts and lairs dark red, standing stones pale blue. (On a
/// Conquest map only its trade posts, the villages, towns and bazaar,
/// are marked, in the same amber; the rest of its marks come from the
/// Conquest mode.)
fn site_colour(kind: Kind) -> Option<[u8; 4]> {
    match kind {
        Kind::Village | Kind::Town | Kind::Bazaar => Some([222, 168, 64, 255]),
        Kind::CreepCamp => Some([150, 40, 50, 255]),
        Kind::Shrine => Some([170, 214, 230, 255]),
        _ => None,
    }
}

/// The ground as colours: water, woods, grass and rising hide, and on a
/// Field Battle map its sites, a cell each.
pub(crate) fn minimap_terrain(hm: &Map) -> MinimapTerrain {
    let tiles = hm.tiles;
    let n = tiles as f32;
    let mut picture = MinimapTerrain::from_fn(terrain_cells(tiles), |u, v| {
        let (x, z) = (u * n, v * n);
        let h = hm.height_at(x, z);
        if h < hm.water_level() {
            return [44, 84, 128, 255];
        }
        let tile = z as usize * tiles as usize + x as usize;
        let cover = hm.cover.get(tile).copied().map(gc_sim::Cover::from_u8);
        match cover {
            Some(gc_sim::Cover::Wood) if hm.is_thicket(tile) => [20, 50, 30, 255],
            Some(gc_sim::Cover::Wood) => [34, 78, 44, 255],
            Some(gc_sim::Cover::Grass) => [104, 140, 70, 255],
            _ => {
                // A few shades, so neighbouring cells merge into one rectangle.
                let t = ((h / hm.max_height()).clamp(0.0, 1.0) * 6.0).round() / 6.0;
                if let Some(land) = hm.land_at(tile) {
                    // The land's own colour, lighter on high ground.
                    let c = land.colour();
                    let k = 0.85 + 0.35 * t;
                    let shade = |v: u8| (f32::from(v) * k).min(255.0) as u8;
                    return [shade(c[0]), shade(c[1]), shade(c[2]), 255];
                }
                let lerp = |a: f32, b: f32| (a + (b - a) * t) as u8;
                [
                    lerp(120.0, 168.0),
                    lerp(132.0, 148.0),
                    lerp(86.0, 116.0),
                    255,
                ]
            }
        }
    });
    let conquest = hm.is_conquest();
    for o in &hm.objects {
        let post = matches!(o.kind, Kind::Village | Kind::Town | Kind::Bazaar);
        if let Some(c) = site_colour(o.kind).filter(|_| !conquest || post) {
            let k = picture.cell_at(tile_uv(o.at.0, n), tile_uv(o.at.1, n));
            picture.colors[k] = c;
        }
    }
    picture
}

/// The middle of tile `t` across a map `n` tiles wide, from 0 to 1.
fn tile_uv(t: i32, n: f32) -> f32 {
    (t as f32 + 0.5) / n
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

/// What building `s` tunes now, if it is a tuning building.
pub(crate) fn tuning_of(
    world: &gc_sim::World,
    table: &crate::buildings::Table,
    s: &gc_sim::StructureView,
) -> Option<Tuning> {
    let (resource, _) = table.stats.get(usize::from(s.building))?.tunes?;
    let economy = world.economy()?;
    let (mut nodes, mut harmony) = (0usize, 0);
    for n in (economy.nodes()).filter(|n| n.tuner == Some(s.handle)) {
        nodes += 1;
        harmony = harmony.max(n.harmony);
    }
    Some(Tuning {
        vein: resource == gc_sim::Resource::Marrow,
        nodes: nodes.min(usize::from(u8::MAX)) as u8,
        per_minute: economy.tuned_per_minute(s.handle),
        harmony,
    })
}

// The top bar's Resonance meter tops out where the simulation's does.
const _: () = assert!(gc_ui::conquest::RESONANCE_FULL == gc_sim::chords::MAX_RESONANCE);
// So does its Honor meter.
const _: () = assert!(gc_ui::conquest::HONOR_FULL == gc_sim::honor::MAX_HONOR);

impl Running {
    /// The mode, for the top bar's status strip.
    fn mode_label(&self) -> &'static str {
        if self.watching.is_some() {
            "Replay"
        } else if self.battle.is_none() && self.online.is_none() {
            "Sandbox"
        } else if self.conquest_match() {
            "Conquest"
        } else {
            "Field Battle"
        }
    }

    /// Open or close the Marshal's Scroll.
    pub fn toggle_help(&mut self) {
        self.hud.help = !self.hud.help;
        self.sandbox.dirty = true;
    }

    /// F1: closes the scroll if it is open, else the next idle worker while
    /// there is one, else opens the scroll.
    pub fn f1_key(&mut self) {
        // With the scroll open, F1 closes it like the scroll's own hint says.
        let idle = !self.hud.help
            && self.conquest_match()
            && self.watching.is_none()
            && self.idle_workers(self.sandbox.side).next().is_some();
        if idle {
            self.next_idle_worker();
        } else {
            self.toggle_help();
        }
    }

    /// The selected regiments as one panel: the banner's name, numbers
    /// and picture. `None` with nothing selected.
    fn selection_info(&self, side: PlayerId) -> Option<(String, Selection<'static>)> {
        let mut n = 0u16;
        let (mut models, mut max_models, mut morale) = (0u32, 0u32, 0u32);
        let mut state = CardState::Steady;
        let (mut unit, mut same, mut icon, mut same_icon) = (None, true, Icon::Sword, true);
        let mut color = crate::color_of(&self.parties, side);
        let mut ground = "";
        for &h in &self.selected {
            let Some(r) = self.world.regiments.get(h).filter(|r| r.owner == side) else {
                continue;
            };
            let stats = &self.units.stats[r.unit as usize];
            let this = troop_icon(stats);
            if n == 0 {
                (unit, icon) = (Some(r.unit), this);
                color = crate::color_of(&self.parties, r.owner);
                ground = ground_word(self.world.terrain(), r.position);
            } else {
                // Shown only when every selected regiment shares it.
                if ground_word(self.world.terrain(), r.position) != ground {
                    ground = "";
                }
                same &= unit == Some(r.unit);
                same_icon &= icon == this;
            }
            n += 1;
            models += u32::from(r.models);
            max_models += u32::from(stats.models);
            morale += u32::from(r.morale_points);
            state = match (state, r.morale) {
                (_, MoraleState::Routing | MoraleState::Shattered) | (CardState::Routing, _) => {
                    CardState::Routing
                }
                (_, MoraleState::Wavering) | (CardState::Wavering, _) => CardState::Wavering,
                _ => CardState::Steady,
            };
        }
        if n == 0 {
            return None;
        }
        let name = match unit {
            Some(u) if same => self.units.names[u as usize].clone(),
            _ => format!("{n} regiments"),
        };
        let selection = Selection {
            name: "",
            color,
            icon: if same_icon { icon } else { Icon::Flag },
            regiments: n,
            models,
            max_models,
            morale: (morale / u32::from(n)) as u8,
            state,
            formation: match self.selection_formation() {
                Some(Some(f)) => f.name(),
                _ => "",
            },
            ground,
            shapes: &[],
        };
        Some((name, selection))
    }

    /// `side`'s idle workers. A worker that only builds (the Choir's Tuner)
    /// has nothing to gather, so standing about is not idling.
    fn idle_workers(&self, side: PlayerId) -> impl Iterator<Item = gc_sim::RegimentHandle> + '_ {
        (self.world.idle_workers(side)).filter(|&h| !self.world.builds_only(h))
    }

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
        let map = self.heightmap.tiles as f32;
        self.selected
            .retain(|&h| self.world.regiments.get(h).is_some());
        self.drop_stale_power();
        // A building stays selected while it stands and is ours, and until
        // a regiment is.
        if !self.selected.is_empty()
            || self
                .selected_structure
                .is_some_and(|h| self.world.structures().and_then(|s| s.owner(h)) != Some(side))
        {
            self.selected_structure = None;
        }
        let tick = self.world.tick_count();
        if self
            .hud
            .readout
            .is_none_or(|r| (r.tick, r.side) != (tick, side))
        {
            // Workers are Conquest's own, and a replay gives nobody orders.
            let workers = self.conquest_match() && self.watching.is_none();
            self.hud.readout = self.top_bar().map(|top| Readout {
                tick,
                side,
                top,
                idle_workers: workers.then(|| self.idle_workers(side).count() as u32),
            });
        }

        let now_ms = self.now_ms();
        // A replay shows what happened to nobody in particular.
        if self.watching.is_some() {
            self.hud.notes.clear();
        }
        let picked_structure = self.picked_structure();
        if self.selected_workers().next().is_none() {
            self.set_build_menu(false);
            if self.hud.armed == Some(Armed::Clear) {
                self.hud.armed = None;
            }
        }
        let mut card_labels = std::mem::take(&mut self.hud.card_labels);
        let mut card_details = std::mem::take(&mut self.hud.card_details);
        let card = (self.conquest_card()).map(|c| {
            (
                c,
                self.describe_card(&c, &mut card_labels, &mut card_details),
            )
        });
        let mut power_names = std::mem::take(&mut self.hud.power_names);
        let mut power_details = std::mem::take(&mut self.hud.power_details);
        let (power_states, powers_shown) =
            self.describe_powers(&mut power_names, &mut power_details);
        let camera = self.active_camera();
        let (view_proj, plane) = (camera.view_proj(), camera.ground_plane());

        // The rubber band of a left-drag that will box select on release
        // (not one that places, builds or carries out an armed order).
        let band = self
            .selection_box()
            .filter(|_| {
                (self.hud.armed.is_none() || self.hud_wants_mouse())
                    && !self.sandbox.placing
                    && !self.sandbox.building_tool
            })
            .map(|(lo, hi)| {
                let (x0, y0) = ((lo.x / scale) as i32, (lo.y / scale) as i32);
                let (x1, y1) = ((hi.x / scale) as i32, (hi.y / scale) as i32);
                gc_ui::Rect::new(x0, y0, (x1 - x0).max(1), (y1 - y0).max(1))
            });

        let selection = (!self.selected.is_empty())
            .then(|| self.selection_info(side))
            .flatten();
        // Army shapes to choose from (the tray shows them for two or more
        // regiments); none for a replay, which plays its recorded orders.
        let shape_chips: Vec<ShapeChip> = if self.watching.is_none() && selection.is_some() {
            let on = self.army_shapes.choice;
            (self.army_shapes.chips().into_iter())
                .map(|c| ShapeChip {
                    label: c.label(),
                    tip: c.tip(),
                    on: c == on,
                })
                .collect()
        } else {
            Vec::new()
        };
        let handover = self.handover_text(side);
        let help_notes = if self.hud.help {
            self.help_notes()
        } else {
            Vec::new()
        };
        let place = self.place_text();
        let place_view = place.as_ref().map(PlaceText::view);
        let placing = self.sandbox.placing && !self.sandbox.building_tool;
        let mode = self.mode_label();
        let side_name = party_name(&self.parties, side);
        let speed = format!("{}x", crate::SPEEDS[self.speed]);
        let status = StatusStrip {
            mode,
            side: &side_name,
            side_color: crate::color_of(&self.parties, side),
            speed: &speed,
            paused: self.paused,
            clock_secs: Some(tick / gc_sim::TICKS_PER_SECOND),
        };
        let hud = &mut self.hud;
        // A power being aimed needs no selection, unless it shifts one (see
        // `drop_stale_power`).
        if self.selected.is_empty() && !matches!(hud.armed, Some(Armed::Power(_))) {
            hud.armed = None;
        }
        hud.dots.clear();
        hud.cards.clear();
        hud.picked.clear();
        hud.picked.extend(self.selected.iter().map(|h| h.index));
        hud.picked.sort_unstable();
        let picked = |h: gc_sim::RegimentHandle| hud.picked.binary_search(&h.index).is_ok();
        // The tray shows the selection, or else the group last recalled; a
        // selected building shows its own panel instead.
        let (shown, title): (&[_], _) = match self.last_group_key {
            _ if picked_structure.is_some() => (&[], String::new()),
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
            let p = crate::drawn_position(&self.prev_positions, self.alpha, &r);
            hud.dots.push(Dot {
                pos: [p.x, p.y],
                color: crate::color_of(&self.parties, r.owner),
                selected: picked(r.handle),
            });
        }
        hud.nodes.clear();
        if let Some(economy) = self.world.economy() {
            hud.nodes.extend(economy.nodes().map(|n| NodeDot {
                pos: from_fx_vec(n.at).to_array(),
                marrow: n.resource == gc_sim::Resource::Marrow,
                dry: n.left == 0,
            }));
        }
        hud.blocks.clear();
        if let Some(structures) = self.world.structures() {
            let shown = crate::buildings::shown_to(side, self.reveal_all);
            hud.blocks
                .extend(structures.iter().filter(|s| shown(s)).map(|s| Block {
                    rect: [
                        s.tile.0 as f32,
                        s.tile.1 as f32,
                        f32::from(s.width),
                        f32::from(s.depth),
                    ],
                    color: crate::color_of(&self.parties, s.owner),
                    selected: picked_structure == Some(s.handle),
                }));
        }
        if let Some(heart) = self.world.heart() {
            let at = from_fx_vec(heart.at);
            let width = crate::buildings::HEART_BASE;
            hud.blocks.push(Block {
                rect: [at.x - width / 2.0, at.y - width / 2.0, width, width],
                color: crate::buildings::heart_color(&heart, &self.parties),
                selected: false,
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
                name: self.units.names[r.unit as usize].clone(),
                color: crate::color_of(&self.parties, r.owner),
                models: r.models,
                max_models: self.units.stats[r.unit as usize].models,
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
        // The minimap's outline is the ground the camera in use shows.
        hud.view.extend(view_outline(view_proj, plane, map));

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
            icon: ICONS[i],
            tip: TIPS[i],
            ..GridSlot::default()
        });
        grid[SLOT_MOVE].state = ready(has_orders);
        grid[SLOT_ATTACK].state = ready(has_orders);
        grid[SLOT_FORMATION].state = ready(has_orders);
        let has_spears = self.selected.iter().any(|&h| {
            self.world
                .regiments
                .get(h)
                .is_some_and(|r| self.units.stats[r.unit as usize].reach)
        });
        grid[SLOT_BRACE].state = ready(has_orders && has_spears);
        grid[SLOT_FALL_BACK].state = ready(has_orders);
        // A hero's abilities take the A, S, D and F slots.
        if let Some(h) = &hero {
            for (i, a) in h.abilities.iter().enumerate() {
                let slot = &mut grid[HERO_SLOT_FIRST + i];
                slot.label = a.short;
                slot.icon = if i == 3 { Icon::Ultimate } else { Icon::Skill };
                slot.tip = "A hero ability. Needs a rank to use.";
                slot.detail = &ability_status[i];
                slot.state =
                    ready(a.usable() && h.revive_secs.is_none() && self.watching.is_none());
            }
        }
        match hud.armed {
            Some(Armed::Move) => grid[SLOT_MOVE].state = SlotState::Armed,
            Some(Armed::Attack) => grid[SLOT_ATTACK].state = SlotState::Armed,
            Some(Armed::Lay(_) | Armed::Wall(..) | Armed::Clear | Armed::Power(_)) | None => {}
        }
        if let Some(Some(f)) = self.shown_formation {
            grid[SLOT_FORMATION].detail = f.name();
        }
        // Conquest's card goes over the battle grid, or takes all of it.
        if let Some((card, states)) = card {
            for (i, slot) in grid.iter_mut().enumerate() {
                if card.slots[i].is_some() {
                    let (icon, mut tip) = card_look(card.slots[i]);
                    if let Some(Action::Lay(b)) = card.slots[i]
                        && self.buildings.is_wall(b)
                    {
                        tip = wall_tip(self.buildings.stats[usize::from(b)].flags);
                    }
                    *slot = GridSlot {
                        label: &card_labels[i],
                        detail: &card_details[i],
                        state: states[i],
                        icon,
                        tip,
                    };
                } else if card.whole {
                    *slot = GridSlot::default();
                }
            }
        }

        let power_buttons: [PowerButton; POWER_KEYS.len()] = std::array::from_fn(|i| PowerButton {
            name: &power_names[i],
            detail: &power_details[i],
            state: power_states[i],
        });
        hud.alerts.clear();
        if !hud.notes.is_empty() {
            hud.alerts.extend(hud.notes.iter().map(|n| Alert {
                id: n.id,
                kind: n.kind,
                at: project(view_proj, Vec3::new(n.at.x, plane, n.at.y), screen),
                fresh: tick.saturating_sub(n.born) <= NOTE_FRESH,
            }));
        }

        // Conquest's own: the stores, the idle workers and the selected building.
        let mut queue = [QueueItem {
            name: "",
            progress: 0,
        }; QUEUE_SLOTS];
        let mut queued = 0;
        if let Some((h, p)) = picked_structure.zip(self.world.production()) {
            for (i, &u) in p.queue_of(h).iter().take(QUEUE_SLOTS).enumerate() {
                queue[i] = QueueItem {
                    name: self
                        .units
                        .names
                        .get(usize::from(u))
                        .map_or("", String::as_str),
                    progress: self.world.queue_percent(h, i).unwrap_or(0),
                };
                queued = i + 1;
            }
        }
        // Tips: on the building being placed, else on the selected one when
        // its info button is on.
        let laying = match hud.armed {
            Some(Armed::Lay(b) | Armed::Wall(b, _)) => Some(self.buildings.name(b)),
            _ => None,
        };
        let selected = picked_structure
            .and_then(|h| self.world.structures()?.get(h))
            // Tips are for your own buildings.
            .filter(|s| s.owner == self.sandbox.side)
            .map(|s| self.buildings.name(s.building));
        let tips_open = selected
            .filter(|n| self.tip_book.contains_key(*n))
            .map(|_| hud.tips_open);
        let tip_name = laying.or(selected.filter(|_| hud.tips_open));
        let tip_lines: Vec<TipLine> =
            tip_name
                .and_then(|n| self.tip_book.get(n))
                .map_or_else(Vec::new, |tips| {
                    tips.iter()
                        .map(|(icon, text)| TipLine { icon: *icon, text })
                        .collect()
                });
        let structure = picked_structure
            .and_then(|h| self.world.structures()?.get(h))
            .map(|s| {
                let mine = s.owner == self.sandbox.side;
                let mine_up = mine && s.finished();
                StructurePanel {
                    name: self.buildings.name(s.building),
                    color: crate::color_of(&self.parties, s.owner),
                    hp: s.hp,
                    max_hp: s.max_hp,
                    building: (!s.finished()).then_some(s.built_percent),
                    upgrading: s
                        .upgrading
                        .map(|(to, percent)| (self.buildings.name(to), percent)),
                    queue: &queue[..queued],
                    // Your own finished ones only: a foundation grows none, and
                    // an enemy's larvae are not yours to count.
                    larvae: mine_up
                        .then(|| self.world.larvae_in(s.handle))
                        .flatten()
                        .map(|n| (n, gc_sim::larvae::LARVA_CAP)),
                    // Your own finished living buildings regrow on your veins.
                    rooted: mine.then(|| self.world.rooted(s.handle)).flatten(),
                    // Your own finished Choir buildings need a field of power.
                    powered: mine_up.then(|| self.world.powered(s.handle)).flatten(),
                    // Your own finished Resonators and Siphons tune nodes.
                    tuning: mine_up
                        .then(|| tuning_of(&self.world, &self.buildings, &s))
                        .flatten(),
                    // Your own tower told to shoot a regiment first.
                    shooting: mine_up
                        .then(|| self.world.structures()?.focus_of(s.handle))
                        .flatten()
                        .and_then(|h| self.world.regiments.get(h))
                        .map(|r| self.units.names[r.unit as usize].as_str()),
                    // Your own gate: how it is set, and whether it is open.
                    gate: mine
                        .then_some(s.gate)
                        .flatten()
                        .map(|m| (crate::conquest_card::gate_line(m), gate_now(s.open))),
                    tips_open,
                }
            });

        let mut input = std::mem::take(&mut hud.input);
        input.mouse = cursor;
        input.shift = self.input.shift;
        input.now_ms = now_ms;
        hud.draw.clear();
        // Drawn first, so the bar and panels stay on top of it.
        if let Some(r) = band {
            hud.draw.rect(r, [120, 255, 140, 36]);
            hud.draw.border(r, 1, [170, 255, 180, 230]);
        }
        hud.events.clear();
        let data = HudData {
            grid: &grid,
            status,
            selection: selection.as_ref().map(|(name, s)| Selection {
                name,
                shapes: &shape_chips,
                ..*s
            }),
            help: hud.help.then_some(HelpScroll { notes: &help_notes }),
            picker: place_view.as_ref().map(|v| v.picker(hud.placer, placing)),
            handover: handover.as_ref().map(|(label, tip, lit)| HandOverButton {
                label,
                tip,
                lit: *lit,
            }),
            hero,
            minimap: MinimapData {
                map_tiles: map,
                terrain: &hud.terrain,
                dots: &hud.dots,
                nodes: &hud.nodes,
                blocks: &hud.blocks,
                view: &hud.view,
            },
            cards: &hud.cards,
            structure,
            tips: (!tip_lines.is_empty()).then_some(TipsCard {
                title: tip_name.unwrap_or_default(),
                lines: &tip_lines,
                closable: laying.is_none(),
            }),
            top: hud.readout.map(|r| r.top),
            tray_title: &title,
            powers: &power_buttons[..powers_shown],
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
                idle_workers: hud.readout.and_then(|r| r.idle_workers),
            },
        };
        hud.hud
            .frame(&mut hud.draw, screen, input, &data, &mut hud.events);
        self.hud.card_labels = card_labels;
        self.hud.card_details = card_details;
        self.hud.power_names = power_names;
        self.hud.power_details = power_details;
        // What to click for a power being aimed, else the ride's way out.
        if let Some(text) = (self.power_hint())
            .or_else(|| self.ride_hint())
            .or_else(|| self.place_hint())
            .or_else(|| self.clearing_hint())
            .or_else(|| self.wall_hint())
        {
            draw_hint(&mut self.hud.draw, screen, &text);
        }
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
            HudEvent::Power(slot) => self.power_press(slot),
            HudEvent::Alert(id) => {
                if let Some(n) = self.hud.notes.iter().find(|n| n.id == id) {
                    self.look_at(n.at);
                }
            }
            HudEvent::IdleWorker => self.next_idle_worker(),
            HudEvent::ToggleHelp => self.toggle_help(),
            HudEvent::ToggleTips => self.hud.tips_open = !self.hud.tips_open,
            HudEvent::OpenGuide => self.open_guide(),
            HudEvent::TogglePlacer => self.toggle_placer(),
            HudEvent::PlaceUnit(i) => self.place_unit(i),
            HudEvent::PlaceSide(i) => self.set_side(i as PlayerId),
            HudEvent::PlaceTab(i) => self.place_tab_to(i),
            HudEvent::PlaceFaction(i) => self.place_faction_to(i),
            HudEvent::ArmyShape(i) => self.pick_army_shape(i),
            HudEvent::HandOver => self.hand_over_next(),
            // The queue gives back its last regiment, whichever is clicked.
            HudEvent::Unqueue(_) => self.untrain(),
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
        if self.watching.is_some() || self.sandbox.placing || self.card_press(slot) {
            return;
        }
        self.battle_press(slot);
    }

    /// The allies `side` could hand its army to: same team, still in it.
    fn allies_of(&self, side: PlayerId) -> Vec<PlayerId> {
        (0..self.parties.len() as PlayerId)
            .filter(|&p| p != side && !self.world.hostile(side, p) && !self.world.is_out(p))
            .collect()
    }

    /// The Hand over button's label, tooltip and whether the army is with
    /// an ally now; `None` when there is nobody to hand it to.
    fn handover_text(&self, side: PlayerId) -> Option<(String, String, bool)> {
        if self.watching.is_some() {
            return None;
        }
        let allies = self.allies_of(side);
        let with = self.world.stances().commander(side);
        if allies.is_empty() && with == side {
            return None;
        }
        let next = self.next_commander(side, &allies);
        let name = |p| party_name(&self.parties, p);
        let tip = if next == side {
            format!("Your army is with {}. Click to take it back.", name(with))
        } else if with == side {
            format!(
                "Give your whole army to {}, who then commands it. Click again to cycle, and once more to take it back.",
                name(next)
            )
        } else {
            format!(
                "Your army is with {}. Click to pass it to {}.",
                name(with),
                name(next)
            )
        };
        let label = if with == side {
            "Hand over".to_string()
        } else {
            format!("With {}", name(with))
        };
        Some((label, tip, with != side))
    }

    /// Who a click on Hand over gives the army to: the first ally, then each
    /// next one, then back to `side` itself.
    fn next_commander(&self, side: PlayerId, allies: &[PlayerId]) -> PlayerId {
        let with = self.world.stances().commander(side);
        if with == side {
            return allies.first().copied().unwrap_or(side);
        }
        let next = allies.iter().position(|&a| a == with).map(|i| i + 1);
        next.and_then(|i| allies.get(i)).copied().unwrap_or(side)
    }

    /// The Hand over button: pass the army to the next ally, or take it back.
    fn hand_over_next(&mut self) {
        if self.watching.is_some() {
            return;
        }
        let side = self.sandbox.side;
        let to = self.next_commander(side, &self.allies_of(side));
        self.human_orders.push(gc_sim::PlayerCommand {
            player: side,
            command: gc_sim::Command::HandOver { to },
        });
        self.ui_sound(sounds::UiSound::Move);
        let msg = if to == side {
            "You command your army again".to_string()
        } else {
            format!("Your army now answers to {}", party_name(&self.parties, to))
        };
        self.say(msg);
    }

    /// A battle grid slot: Move, Attack, Formation, Brace or Fall back.
    pub fn battle_press(&mut self, slot: usize) {
        if self.watching.is_some() || self.sandbox.placing || self.selected.is_empty() {
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
            SLOT_BRACE => self.toggle_brace(),
            SLOT_FALL_BACK => self.fall_back(),
            _ => {}
        }
    }

    /// The click after Q or W: send the selection there, or at the enemy there.
    pub fn finish_armed_order(&mut self) {
        let Some(armed) = self.hud.armed else {
            return;
        };
        // A click that finds nothing to attack keeps the order armed.
        match armed {
            Armed::Lay(b) => return self.finish_laying(b),
            Armed::Wall(b, _) => return self.finish_walling(b),
            Armed::Clear => return self.finish_clearing(),
            Armed::Power(power) => return self.finish_power(power),
            Armed::Move => match self.ground_under_cursor() {
                Some(p) => self.issue_group_order(move_order(p)),
                None => return,
            },
            Armed::Attack => match self.enemy_under_cursor() {
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

    /// Conquest's top bar for the side you command, on a field with an
    /// economy while the HUD is up.
    pub fn top_bar(&self) -> Option<TopBar> {
        if !self.match_started || self.showroom.is_some() {
            return None;
        }
        let b = self.world.bank(self.sandbox.side)?;
        Some(TopBar {
            grub: b.grub,
            marrow: b.marrow,
            supply_used: b.supply_used,
            supply_cap: b.supply_cap,
            upkeep_percent: b.upkeep_percent(),
            larvae: b.larvae,
            larva_room: b.larva_room,
            resonance: b.resonance,
            chords: b.chords,
            honor: (self.world.meter(self.sandbox.side) == gc_sim::Meter::Honor).then_some(b.honor),
            clock_secs: self.world.tick_count() / gc_sim::TICKS_PER_SECOND,
        })
    }

    /// The selected building: only while no regiment is selected.
    pub fn picked_structure(&self) -> Option<StructureHandle> {
        self.selected_structure.filter(|_| self.selected.is_empty())
    }

    /// F1: select the next idle worker and look at it.
    pub fn next_idle_worker(&mut self) {
        if !self.conquest_match() || self.watching.is_some() {
            return;
        }
        let idle: Vec<gc_sim::RegimentHandle> = self.idle_workers(self.sandbox.side).collect();
        let Some(&h) = idle
            .iter()
            .find(|h| self.idle_from.is_some_and(|from| h.index > from))
            .or_else(|| idle.first())
        else {
            return;
        };
        self.idle_from = Some(h.index);
        self.selected.clear();
        self.selected.push(h);
        self.selected_structure = None;
        if let Some(p) = self.world.regiments.position(h) {
            self.look_at(from_fx_vec(p));
        }
        self.ui_sound(sounds::UiSound::Select);
    }

    /// Space: look at the latest alert.
    pub fn jump_to_alert(&mut self) {
        if let Some(n) = self.hud.latest_note() {
            self.look_at(n.at);
        }
    }

    /// The line shown while riding with a regiment.
    fn ride_hint(&self) -> Option<String> {
        let ride = self.ride.as_ref()?;
        let name = (self.world.regiments.get(ride.regiment)).map_or("the regiment", |r| {
            self.units.names[r.unit as usize].as_str()
        });
        Some(format!("Riding with {name} - Y or Esc to return"))
    }

    /// Whether the Place panel and button are on offer: wherever B puts down
    /// regiments (the sandbox and offline Field Battles).
    pub fn can_place(&self) -> bool {
        !self.conquest_match() && self.online.is_none() && self.watching.is_none()
    }

    /// The Place button, or B turning placing on: open or close the panel.
    pub fn toggle_placer(&mut self) {
        if !self.can_place() {
            return;
        }
        self.hud.placer = !self.hud.placer;
        self.sandbox.dirty = true;
    }

    /// Whether the Place panel offers its tabs: in the sandbox, which can run
    /// on either unit table. (A Field Battle has the Field types only.)
    fn place_tabs_on(&self) -> bool {
        self.battle.is_none()
    }

    /// The tab showing: 0 the Field types, 1 all factions.
    fn place_tab(&self) -> usize {
        if self.place_tabs_on() {
            usize::from(self.hud.place_tab.min(1))
        } else {
            0
        }
    }

    /// The faction chosen on the roster tab.
    fn place_faction(&self) -> gc_ai::Faction {
        gc_ai::Faction::ALL[usize::from(self.hud.place_faction).min(gc_ai::Faction::ALL.len() - 1)]
    }

    /// The roster's table (`roster`) or the Field types', whichever slot
    /// `use_play` left it in.
    fn table(&self, roster: bool) -> &UnitTable {
        if roster == self.play.roster_table() {
            &self.units
        } else {
            &self.other_units
        }
    }

    /// The Place panel's rows: every Field type, or the chosen faction's
    /// placeable roster troops. Rebuilt only when the tab, faction or table
    /// changes, or the sandbox tunes a troop shown.
    fn place_body(&self) -> std::rc::Rc<PlaceBody> {
        let roster = self.place_tab() == 1;
        let faction = if roster {
            self.place_faction()
        } else {
            gc_ai::Faction::ALL[0]
        };
        let key = (roster, faction, self.play);
        let mut cache = self.hud.place_body.borrow_mut();
        if let Some(body) = cache.as_ref() {
            let table = self.table(roster);
            let fresh = body.key == key
                && body
                    .ids
                    .iter()
                    .zip(&body.stats)
                    .all(|(&i, s)| table.stats[i] == *s);
            if fresh {
                return body.clone();
            }
        }
        let body = std::rc::Rc::new(if roster {
            self.roster_body(key, faction)
        } else {
            self.field_body(key)
        });
        *cache = Some(body.clone());
        body
    }

    /// The Field types, one row each.
    fn field_body(&self, key: PlaceKey) -> PlaceBody {
        let table = self.table(false);
        let ids: Vec<usize> = (0..table.stats.len()).collect();
        let rows = ids
            .iter()
            .map(|&i| place_row(table, i, format!("{} points", table.points[i])))
            .collect();
        PlaceBody {
            key,
            stats: ids.iter().map(|&i| table.stats[i]).collect(),
            ids,
            rows,
            later: Vec::new(),
            chips: Vec::new(),
            reason: "",
        }
    }

    /// `faction`'s placeable roster troops, what the sandbox leaves out, and
    /// the chips to choose among the factions.
    fn roster_body(&self, key: PlaceKey, faction: gc_ai::Faction) -> PlaceBody {
        let table = self.table(true);
        let units = self.data.conquest_units;
        let ids = roster::placeable(units, faction);
        let rows = ids
            .iter()
            .map(|&i| place_row(table, i, roster::cost_text(&units[i])))
            .collect();
        let (count, names) = roster::left_out_names(units, faction);
        PlaceBody {
            key,
            stats: ids.iter().map(|&i| table.stats[i]).collect(),
            ids,
            rows,
            later: (count > 0)
                .then(|| (faction.name(), faction_dye(faction), count, names))
                .into_iter()
                .collect(),
            chips: (gc_ai::Faction::ALL.iter())
                .map(|&f| (roster::short_name(f), faction_dye(f)))
                .collect(),
            reason: PLACE_REASON_ROSTER,
        }
    }

    /// A tab of the Place panel: show that table's troops.
    fn place_tab_to(&mut self, tab: usize) {
        if self.can_place() && self.place_tabs_on() {
            self.hud.place_tab = tab.min(1) as u8;
            self.sandbox.dirty = true;
            self.ui_sound(sounds::UiSound::Select);
        }
    }

    /// A faction chip of the roster tab.
    fn place_faction_to(&mut self, i: usize) {
        if self.can_place() && i < gc_ai::Faction::ALL.len() {
            self.hud.place_faction = i as u8;
            self.sandbox.dirty = true;
            self.ui_sound(sounds::UiSound::Select);
        }
    }

    /// Start the sandbox again on the roster table (`roster`) or the Field
    /// types, with the same side to command. The field is cleared: the two
    /// tables number their troops differently, so one world cannot hold both.
    fn restart_sandbox(&mut self, roster: bool) {
        let (side, faction) = (self.sandbox.side, self.hud.place_faction);
        self.restart_match(MatchKind::Sandbox { roster });
        self.sandbox.side = side;
        self.hud.place_faction = faction;
        self.say(if roster {
            "Sandbox started again on the roster table: troops of every faction. The field is clear."
        } else {
            "Sandbox started again on the Field types. The field is clear."
        }
        .into());
    }

    /// A row of the Place panel: choose that type and arm placing. A troop
    /// of the table the sandbox is not running on restarts it on that one.
    fn place_unit(&mut self, i: usize) {
        if !self.can_place() {
            return;
        }
        let want_roster = self.place_tab() == 1;
        let Some(&unit) = self.place_body().ids.get(i) else {
            return;
        };
        if want_roster != self.play.roster_table() {
            if !self.place_tabs_on() {
                return;
            }
            self.restart_sandbox(want_roster);
        }
        if unit >= self.units.stats.len() {
            return;
        }
        self.sandbox.unit = unit as gc_sim::UnitId;
        self.sandbox.placing = true;
        self.sandbox.building_tool = false;
        self.hud.armed = None;
        self.sandbox.dirty = true;
        self.ui_sound(sounds::UiSound::Select);
    }

    /// "Placing Yari Ashigaru for Red", under the cursor while placing.
    fn place_hint(&self) -> Option<String> {
        (self.can_place() && self.sandbox.placing && !self.sandbox.building_tool).then(|| {
            format!(
                "Placing {} for {} - click the field to place - B or Esc to stop",
                self.units.names[self.sandbox.unit as usize],
                party_name(&self.parties, self.sandbox.side)
            )
        })
    }

    /// The words of the Place panel, owned so the HUD can borrow them for a
    /// frame; `None` where the panel is not on offer.
    fn place_text(&self) -> Option<PlaceText> {
        if !self.can_place() {
            return None;
        }
        if !self.hud.placer {
            return Some(PlaceText::default());
        }
        let tab = self.place_tab();
        let body = self.place_body();
        let sides = (0..self.parties.len() as PlayerId)
            .map(|side| {
                (
                    party_name(&self.parties, side),
                    crate::color_of(&self.parties, side),
                )
            })
            .collect();
        // Whether the table showing is the one the sandbox runs on.
        let in_play = (tab == 1) == self.play.roster_table();
        let current = if in_play {
            (body.ids.iter())
                .position(|&i| i == self.sandbox.unit as usize)
                .unwrap_or(usize::MAX)
        } else {
            usize::MAX
        };
        Some(PlaceText {
            body: Some(body),
            sides,
            tabs: if self.place_tabs_on() {
                &PLACE_TABS
            } else {
                &[]
            },
            tab,
            chip: usize::from(self.hud.place_faction),
            current,
            side: self.sandbox.side as usize,
        })
    }

    pub fn look_at(&mut self, p: Vec2) {
        let focus = p.clamp(Vec2::ZERO, Vec2::splat(self.camera.map_size));
        let height = self.heightmap.height_at(focus.x, focus.y);
        let camera = self.camera_mut();
        camera.focus = focus;
        camera.focus_height = height;
    }
}

/// What a Place panel body was built for: roster tab or not, the faction,
/// and which regiment types are in play.
type PlaceKey = (bool, gc_ai::Faction, crate::Play);

/// The Place panel's tab-dependent words, built once and shared by frames.
struct PlaceBody {
    key: PlaceKey,
    /// The stats of the troops in `ids` as the rows were built, to see a
    /// tuned one.
    stats: Vec<gc_sim::UnitStats>,
    /// The rows as indices into the table they belong to.
    ids: Vec<usize>,
    /// Name, second line, faction and picture of each placeable type.
    rows: Vec<(String, String, &'static str, Icon)>,
    /// Faction, its dye, how many troops and their names.
    later: Vec<(&'static str, [u8; 3], usize, String)>,
    /// The roster tab's faction chips.
    chips: Vec<(&'static str, [u8; 3])>,
    /// Why the greyed troops are greyed.
    reason: &'static str,
}

/// One row of the Place panel for type `i` of `table`, with its `cost` words.
fn place_row(table: &UnitTable, i: usize, cost: String) -> (String, String, &'static str, Icon) {
    let s = &table.stats[i];
    let speed = s.speed.raw() as f32 / 65536.0;
    let men = if s.models == 1 { "soldier" } else { "soldiers" };
    (
        table.names[i].clone(),
        format!("{} {men}  {speed:.1} tiles/s  {cost}", s.models),
        table.factions[i].name(),
        troop_icon(s),
    )
}

/// The Place panel's words for one frame; the default is a closed panel.
#[derive(Default)]
struct PlaceText {
    body: Option<std::rc::Rc<PlaceBody>>,
    sides: Vec<(String, [u8; 3])>,
    /// The tabs on offer (none outside the sandbox), and which is showing.
    tabs: &'static [&'static str],
    tab: usize,
    chip: usize,
    current: usize,
    side: usize,
}

/// The Place panel's tabs.
const PLACE_TABS: [&str; 2] = ["Field types", "All factions"];

/// Why the roster's workers and caravan are greyed.
const PLACE_REASON_ROSTER: &str = "Not regiments to fight with: workers gather and build, a caravan hauls Grub. Both need Conquest's economy.";

/// [`PlaceText`] as the HUD borrows it.
struct PlaceView<'a> {
    rows: Vec<PlaceRow<'a>>,
    sides: Vec<PlaceSide<'a>>,
    later: Vec<PlaceLater<'a>>,
    chips: Vec<PlaceSide<'a>>,
    tabs: &'a [&'a str],
    tab: usize,
    chip: usize,
    reason: &'a str,
    current: usize,
    side: usize,
}

impl PlaceText {
    fn view(&self) -> PlaceView<'_> {
        let (rows, later, chips, reason) = self
            .body
            .as_deref()
            .map_or((&[][..], &[][..], &[][..], ""), |b| {
                (&b.rows[..], &b.later[..], &b.chips[..], b.reason)
            });
        PlaceView {
            rows: rows
                .iter()
                .map(|(name, detail, faction, icon)| PlaceRow {
                    name,
                    detail,
                    faction,
                    icon: *icon,
                })
                .collect(),
            sides: (self.sides.iter())
                .map(|(name, color)| PlaceSide {
                    name,
                    color: *color,
                })
                .collect(),
            later: later
                .iter()
                .map(|(faction, color, count, names)| PlaceLater {
                    faction,
                    color: *color,
                    count: *count,
                    names,
                })
                .collect(),
            chips: chips
                .iter()
                .map(|(name, color)| PlaceSide {
                    name,
                    color: *color,
                })
                .collect(),
            tabs: self.tabs,
            tab: self.tab,
            chip: self.chip,
            reason,
            current: self.current,
            side: self.side,
        }
    }
}

impl PlaceView<'_> {
    fn picker(&self, open: bool, placing: bool) -> PlacePicker<'_> {
        PlacePicker {
            open,
            placing,
            rows: &self.rows,
            current: self.current,
            sides: &self.sides,
            side: self.side,
            reason: self.reason,
            later: &self.later,
            tabs: self.tabs,
            tab: self.tab,
            chips: &self.chips,
            chip: self.chip,
        }
    }
}

/// A faction's colour on the Place panel: the tapestry dyes.
fn faction_dye(f: gc_ai::Faction) -> [u8; 3] {
    let [r, g, b, _] = match f {
        gc_ai::Faction::EmberClans => gc_ui::ui::theme::MADDER,
        gc_ai::Faction::Brood => gc_ui::ui::theme::WODE,
        gc_ai::Faction::Choir => gc_ui::ui::theme::WOAD,
        gc_ai::Faction::CarrionConcord => [112, 84, 120, 255],
    };
    [r, g, b]
}

/// The picture for a troop: a bow, a spear or a sword by how it fights.
fn troop_icon(s: &gc_sim::UnitStats) -> Icon {
    if s.range.raw() > 0 {
        Icon::Bow
    } else if s.reach {
        Icon::Spear
    } else {
        Icon::Sword
    }
}

/// What the ground at `p` does to a regiment, in a few words for the selection panel (capitalised, as it stands on its own line): the readable side
/// of docs/32-changing-land.md, "Reading the ground". Empty on plain ground.
fn ground_word(terrain: &gc_sim::Terrain, p: gc_fixed::FxVec2) -> &'static str {
    if terrain.wet(p) {
        "In a ford: slow, no charges"
    } else if terrain.soft(p) {
        "In mud: slow, no charges"
    } else if terrain.cover(p) == gc_sim::Cover::Wood {
        "In woods: slower"
    } else if terrain.on_slope(p) {
        "On a slope: slower uphill"
    } else {
        ""
    }
}

/// Whether a gate is open right now, for its card.
fn gate_now(open: bool) -> &'static str {
    if open {
        "Open right now"
    } else {
        "Shut right now"
    }
}

/// The tooltip of a wall on the build menu (it replaces a building's).
const WALL_TIP: &str = "Draw this wall: click its start, then click each end or bend, a tile at a time. Right-click ends it; Shift locks 15-degree steps.";
/// The Light Curtain's and the Resin Ridge's tips say what is different.
const WALL_TIP_PASS: &str = "Draw this wall: click its start, then each end or bend. Your own troops walk through it; it drops when power fails. Right-click ends it; Shift locks 15-degree steps.";
const WALL_TIP_GROWN: &str = "Draw this wall: click its start, then each end or bend. It grows on its own along your root-veins, with no Gnawer. Right-click ends it; Shift locks 15-degree steps.";

/// The build-menu tooltip for the wall `flags` describe.
fn wall_tip(flags: u16) -> &'static str {
    use gc_sim::structures::{GROWN, PASS};
    if flags & PASS != 0 {
        WALL_TIP_PASS
    } else if flags & GROWN != 0 {
        WALL_TIP_GROWN
    } else {
        WALL_TIP
    }
}

/// The picture and tooltip line of a Conquest card slot.
fn card_look(action: Option<Action>) -> (Icon, &'static str) {
    match action {
        Some(Action::OpenBuild) => (Icon::Hammer, "Open the build menu for your workers."),
        Some(Action::Clear) => (
            Icon::Hammer,
            "Workers clear scrub, woods and thicket, each faction its own way (the worker's guide entry says how). Felled timber can pay a little Grub. Drag over the ground, or click one tile. Cleared ground no longer hides troops or slows them.",
        ),
        Some(Action::Lay(_)) => (
            Icon::Hammer,
            "A worker lays this building. Then click where. Woods and scrub are cleared first, which takes longer.",
        ),
        Some(Action::Train(_)) => (Icon::Flag, "Train this regiment at the building."),
        Some(Action::Upgrade(_)) => (Icon::Ultimate, "Raise the building to its next tier."),
        Some(Action::SetGate(mode)) => (
            match mode {
                GateMode::Auto => Icon::Flag,
                GateMode::Locked => Icon::FallBack,
                GateMode::Open => Icon::Move,
            },
            crate::conquest_card::gate_line(mode),
        ),
        Some(Action::CallOff) => (
            Icon::FallBack,
            "Call it off and get a part of the cost back.",
        ),
        None => (Icon::None, ""),
    }
}

/// A line of help, centred just above the bar.
fn draw_hint(draw: &mut DrawList, screen: (i32, i32), text: &str) {
    let scale = 1;
    let w = gc_ui::draw::text_width(text, scale);
    let h = gc_ui::draw::text_height(scale);
    let pad = 8;
    let panel = gc_ui::Rect::new(
        (screen.0 - w) / 2 - pad,
        screen.1 - gc_ui::hud::BAR_HEIGHT - h - 3 * pad,
        w + 2 * pad,
        h + pad,
    );
    draw.shadow(panel, 3);
    draw.boxed(panel, gc_ui::ui::theme::LINEN, 2);
    draw.text(
        panel.x + pad,
        panel.y + pad / 2,
        scale,
        gc_ui::ui::theme::INK,
        text,
    );
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

/// Where the four screen corners look at the ground, in tiles, clamped to
/// the map, through the camera with this `view_proj`. The ground is taken as
/// flat at height `plane`.
/// How far out the minimap outline puts a screen corner that looks at the
/// sky, in tiles: about what a low camera shows clearly.
const SKY_OUTLINE_TILES: f32 = 120.0;

fn view_outline(view_proj: glam::Mat4, plane: f32, map: f32) -> [[f32; 2]; 4] {
    let inv = view_proj.inverse();
    [(-1.0, 1.0), (1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)].map(|(x, y)| {
        let (origin, dir) = crate::camera::unproject(inv, Vec2::new(x, y));
        let hit: Vec3 = if dir.y < -1e-4 {
            origin + dir * ((plane - origin.y) / dir.y)
        } else {
            // Looking at the sky (riding, the top corners do): a little way
            // out along the ray's heading instead.
            origin + Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero() * SKY_OUTLINE_TILES
        };
        [hit.x.clamp(0.0, map), hit.z.clamp(0.0, map)]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::RtsCamera;

    /// A Field Battle's villages, outposts and stones show on the minimap
    /// in their own colours; a Conquest map's only mark its trade posts.
    #[test]
    fn the_minimap_marks_a_field_maps_sites() {
        let field = gc_map::FieldMap {
            choice: gc_map::WILDS + 1,
            tiles: 512,
            seed: 3,
        }
        .make();
        let amber = site_colour(Kind::Village).unwrap();
        let picture = minimap_terrain(&field);
        let site = field
            .objects
            .iter()
            .find(|o| o.kind == Kind::Village)
            .expect("a village");
        let n = field.tiles as f32;
        let k = picture.cell_at(tile_uv(site.at.0, n), tile_uv(site.at.1, n));
        assert_eq!(picture.colors[k], amber);
        let conquest = Map::from_def(
            &gc_map::conquest::describe_wilds(512, 3, gc_map::wilds::Wildness::Plains).unwrap(),
        );
        let marked = |m: &Map| {
            minimap_terrain(m)
                .colors
                .iter()
                .filter(|&&c| c == amber)
                .count()
        };
        // Only its trade posts are marked, not its lairs or stones.
        let posts = conquest
            .objects
            .iter()
            .filter(|o| matches!(o.kind, Kind::Village | Kind::Town | Kind::Bazaar))
            .count();
        assert!(posts > 0, "a Conquest map has trade posts");
        assert!((1..=posts).contains(&marked(&conquest)));
        let lair = site_colour(Kind::CreepCamp).unwrap();
        assert!(!minimap_terrain(&conquest).colors.contains(&lair));
    }

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
