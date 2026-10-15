//! The battle HUD's glue: describe the match to `gc_ui::hud`, and carry out
//! what it reports. Docs: docs/08-ui-ux.md#screen-layout-1080p-reference.

use gc_sim::MoraleState;
use gc_sim::formation::GroupOrder;
use gc_ui::DrawList;
use gc_ui::hud::{
    Card, CardState, DeployStatus, Dot, Hud, HudData, HudEvent, HudInput, MinimapData,
    MinimapTerrain,
};
use glam::{Vec2, Vec3};

use crate::terrain_gen::Heightmap;
use crate::{Running, from_fx_vec, fx_vec, sounds};

/// Cells per side of the minimap's ground picture.
const TERRAIN_CELLS: usize = 48;

/// Everything the HUD keeps between frames.
#[derive(Default)]
pub struct HudState {
    hud: Hud,
    pub draw: DrawList,
    /// Mouse events collected since the last frame (position is filled in
    /// when the HUD runs).
    input: HudInput,
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

    /// Forget any half-finished click, e.g. when a menu opens over the bar.
    pub fn reset(&mut self) {
        self.hud = Hud::default();
        self.input = HudInput::default();
        self.left_held = false;
        self.right_held = false;
    }

    pub fn wants(&self, screen: (i32, i32), mouse: (i32, i32)) -> bool {
        self.hud.wants(screen, mouse)
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

        let hud = &mut self.hud;
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

        let mut input = std::mem::take(&mut hud.input);
        input.mouse = cursor;
        input.shift = self.input.shift;
        input.now_ms = self.started.elapsed().as_millis() as u64;
        hud.draw.clear();
        hud.events.clear();
        let data = HudData {
            minimap: MinimapData {
                map_tiles: map,
                terrain: &hud.terrain,
                dots: &hud.dots,
                view: &hud.view,
            },
            cards: &hud.cards,
            tray_title: &title,
            deployment: self.world.deploy_zone(side).map(|_| DeployStatus {
                seconds_left: self
                    .world
                    .deploy_ticks_left()
                    .div_ceil(gc_sim::TICKS_PER_SECOND),
                ready: self.world.deploy_ready(side),
            }),
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
            HudEvent::Ready => {
                self.human_orders.push(gc_sim::PlayerCommand {
                    player: self.sandbox.side,
                    command: gc_sim::Command::Ready,
                });
                self.ui_sound(sounds::UiSound::Select);
            }
            HudEvent::Order([x, z]) => {
                if !self.selected.is_empty() {
                    self.issue_group_order(GroupOrder {
                        target: fx_vec(Vec2::new(x, z)),
                        facing: None,
                        width: None,
                    });
                }
            }
        }
    }

    fn own_regiment(&self, id: u32) -> Option<gc_sim::RegimentHandle> {
        self.world
            .regiments
            .iter()
            .find(|r| r.handle.index == id && r.owner == self.sandbox.side)
            .map(|r| r.handle)
    }

    pub fn look_at(&mut self, p: Vec2) {
        self.camera.focus = p.clamp(Vec2::ZERO, Vec2::splat(self.camera.map_size));
        self.camera.focus_height = self
            .heightmap
            .height_at(self.camera.focus.x, self.camera.focus.y);
    }
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
