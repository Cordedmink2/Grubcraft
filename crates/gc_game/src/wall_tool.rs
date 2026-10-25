//! The wall tool (docs/26-fortifications.md, "Drawn walls"; docs/08-ui-ux.md):
//! a worker draws a wall from one tile to another. The first click sets the
//! start, the ghost follows the mouse, and the next click sends the order
//! and starts the next piece from that end, so the wall bends. The rules,
//! prices and the check are the simulation's (`Command::BuildWall`,
//! `World::wall_plan`); this is the picking of the two ends and the ghost.
//!
//! Two modes sit on top of that, switched with keys while the tool is armed.
//! `B` plans the wall as a blueprint (`Command::PlanWall`, `World::blueprint`):
//! nothing is paid and no worker is named; workers set to the plans pay for
//! each tile as they reach it. `O` draws a ring instead: each click adds a
//! corner, and clicking the first corner again (or Enter) orders every side
//! of the closed loop (see `ring_tool` for the cutting). Your own planned
//! tiles are drawn as faint blue slabs, and a right-click on one sends the
//! selected workers to work the plans; Ctrl+right-click drops its line.
//!
//! A bridge (docs/26-fortifications.md, "Bridges") is drawn with the same
//! tool, but only as a straight line from bank to bank: no blueprints, no
//! ring, the far end is turned to the nearer axis, and it snaps to the ends
//! of your own bridges rather than to walls, towers and gates.

use gc_sim::blueprints::MAX_PLANS;
use gc_sim::structures::{GATE, Role};
use gc_sim::walls::{MAX_WALL_TILES, WallTile};
use gc_sim::{BuildingId, BuildingStats, Command, Refusal};
use glam::Vec2;
use winit::keyboard::KeyCode;

use crate::buildings::{self, Fit};
use crate::hud::Armed;
use crate::ring_tool::{self, GateSpot, Piece, Tile};
use crate::{Running, color_of, from_fx_vec};

/// A seep or vein this near a main building of yours (in tiles) is one the
/// ring suggests a gate on the way to; and how many of them it looks at.
const GATE_ROUTE_REACH: f32 = 45.0;
const GATE_ROUTES: usize = 4;

/// How near, in tiles, the mouse must be to a wall end, tower or gate for
/// the wall to start or end on it.
pub const SNAP_REACH: f32 = 1.5;
/// The step of the angle lock (Shift), in degrees.
pub const ANGLE_STEP: f32 = 15.0;

/// A place a wall likes to start or end: a block of tiles `[x, y, w, d]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub rect: [i32; 4],
    /// The wall may end on a tile of the block itself (a wall end or a
    /// gate, which joins it); else only on a tile beside it (a tower).
    pub joins: bool,
}

/// How far point `p` is from the block `rect`, in tiles (0 inside it).
fn distance_to(p: Vec2, [x, y, w, d]: [i32; 4]) -> f32 {
    let nearest = Vec2::new(
        p.x.clamp(x as f32, (x + w) as f32),
        p.y.clamp(y as f32, (y + d) as f32),
    );
    nearest.distance(p)
}

/// The tile a wall starts or ends on for the mouse at ground point `p`: the
/// tile under it, unless an anchor is within [`SNAP_REACH`], then a tile
/// of the nearest one (the tile of it nearest the mouse; for a tower, the
/// tile beside it nearest the mouse).
pub fn snap_tile(p: Vec2, anchors: &[Anchor]) -> (i32, i32) {
    let under = (p.x.floor() as i32, p.y.floor() as i32);
    let Some(a) = anchors
        .iter()
        .map(|a| (distance_to(p, a.rect), a))
        .filter(|&(d, _)| d <= SNAP_REACH)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, a)| a)
    else {
        return under;
    };
    let [x, y, w, d] = a.rect;
    if a.joins {
        return (under.0.clamp(x, x + w - 1), under.1.clamp(y, y + d - 1));
    }
    // The ring of tiles round the block; the one nearest the mouse.
    let mut best = (f32::MAX, under);
    for ty in y - 1..=y + d {
        for tx in x - 1..=x + w {
            let inside = (x..x + w).contains(&tx) && (y..y + d).contains(&ty);
            if inside {
                continue;
            }
            let dist = Vec2::new(tx as f32 + 0.5, ty as f32 + 0.5).distance(p);
            if dist < best.0 {
                best = (dist, (tx, ty));
            }
        }
    }
    best.1
}

/// `to` with the line from `from` turned to the nearest multiple of
/// [`ANGLE_STEP`] degrees, as long as it was.
pub fn lock_angle(from: (i32, i32), to: (i32, i32)) -> (i32, i32) {
    let v = Vec2::new((to.0 - from.0) as f32, (to.1 - from.1) as f32);
    let len = v.length();
    if len < 0.5 {
        return to;
    }
    let step = ANGLE_STEP.to_radians();
    let angle = (v.y.atan2(v.x) / step).round() * step;
    (
        from.0 + (angle.cos() * len).round() as i32,
        from.1 + (angle.sin() * len).round() as i32,
    )
}

/// `to` moved onto the axis the line from `from` runs nearer to, so a bridge
/// is always straight (a tie goes along x).
pub fn axis_lock(from: (i32, i32), to: (i32, i32)) -> (i32, i32) {
    if (to.0 - from.0).abs() >= (to.1 - from.1).abs() {
        (to.0, from.1)
    } else {
        (from.0, to.1)
    }
}

/// `to` pulled in along the line from `from` so a wall between them covers
/// at most [`MAX_WALL_TILES`] tiles.
pub fn clamp_end(from: (i32, i32), to: (i32, i32)) -> (i32, i32) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let most = MAX_WALL_TILES as i32 - 1;
    let far = dx.abs().max(dy.abs());
    if far <= most {
        return to;
    }
    let scale = most as f32 / far as f32;
    (
        from.0 + (dx as f32 * scale).round() as i32,
        from.1 + (dy as f32 * scale).round() as i32,
    )
}

/// How a planned tile sits on the ground; `open` is how one that fits is
/// drawn (the side's colour for a wall, pale blue for a blueprint).
fn fit_of(tile: &WallTile, open: Fit) -> Fit {
    match tile.fits {
        Ok(()) => open,
        Err(Refusal::Overgrown) => Fit::Overgrown,
        Err(_) => Fit::Refused,
    }
}

/// What the wall tool is set to while it is armed (`hud.wall_mode`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WallMode {
    /// Plan the wall as a blueprint instead of ordering a worker to build it.
    pub blueprint: bool,
    /// Drawing a ring, with the corners clicked so far; `None` is the line
    /// tool.
    pub ring: Option<Vec<Tile>>,
}

/// What the ring preview was worked out from; while it stays the same, so
/// does the preview.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RingKey {
    corners: Vec<Tile>,
    closing: bool,
    blueprint: bool,
    building: BuildingId,
    tick: u32,
}

/// What the tool shows for the ring as it would be ordered now: the corners
/// clicked and the one under the mouse, closed.
pub struct RingPreview {
    pub pieces: Vec<Piece>,
    /// How many tiles the sendable pieces lay, each once.
    pub laid: usize,
    /// Where gates would suit (drawn, not placed).
    pub gates: Vec<GateSpot>,
    /// A click now would close the ring.
    pub closing: bool,
    key: RingKey,
}

impl Running {
    /// The tile the wall being drawn goes on from, once its start is
    /// clicked (in a ring, the last corner).
    fn wall_from(&self) -> Option<(i32, i32)> {
        match (&self.hud.wall_mode.ring, self.hud.armed) {
            (Some(ring), Some(Armed::Wall(..))) => ring.last().copied(),
            (None, Some(Armed::Wall(_, from))) => from,
            _ => None,
        }
    }

    /// Whether the wall tool is armed with a bridge.
    fn bridge_armed(&self) -> bool {
        matches!(self.hud.armed, Some(Armed::Wall(b, _)) if self.buildings.is_bridge(b))
    }

    /// The places a wall of yours likes to end on: the ends of your own
    /// walls, your towers and your gates; for a bridge, the ends of your own
    /// bridges.
    fn wall_anchors(&self) -> Vec<Anchor> {
        let Some(all) = self.world.structures() else {
            return Vec::new();
        };
        let side = self.sandbox.side;
        let bridge = self.bridge_armed();
        let mut out = Vec::new();
        for s in all.iter().filter(|s| s.owner == side) {
            let Some(stats) = self.buildings.stats.get(usize::from(s.building)) else {
                continue;
            };
            if (stats.role == Role::Bridge) != bridge {
                continue;
            }
            let rect = [s.tile.0, s.tile.1, i32::from(s.width), i32::from(s.depth)];
            if let Some([x0, y0, x1, y1]) = s.line {
                // Each end once, from the tile that is the end.
                for end in [(x0, y0), (x1, y1)] {
                    if s.tile == end {
                        out.push(Anchor { rect, joins: true });
                    }
                }
            } else if stats.flags & GATE != 0 {
                out.push(Anchor { rect, joins: true });
            } else if stats.role == Role::Defence {
                out.push(Anchor { rect, joins: false });
            }
        }
        // The ends of your planned lines, too (blueprints are never bridges).
        for p in (self.world.plans().iter()).filter(|p| p.owner == side && !bridge) {
            let [x0, y0, x1, y1] = p.line.map(i32::from);
            if p.tile == (x0, y0) || p.tile == (x1, y1) {
                out.push(Anchor {
                    rect: [p.tile.0, p.tile.1, 1, 1],
                    joins: true,
                });
            }
        }
        out
    }

    /// The tile the mouse picks for an end of the wall.
    fn wall_tile_now(&self, anchors: &[Anchor]) -> Option<(i32, i32)> {
        let at = self.ground_under_cursor()?;
        let tiles = self.heightmap.tiles as i32;
        let (x, y) = if self.input.shift && self.wall_from().is_some() {
            (at.x.floor() as i32, at.y.floor() as i32)
        } else {
            snap_tile(at, anchors)
        };
        Some((x.clamp(0, tiles - 1), y.clamp(0, tiles - 1)))
    }

    /// The two ends of the wall as it would be drawn now: the start
    /// (or the tile under the mouse, before there is one) and the end.
    fn wall_ends_now(&self) -> Option<((i32, i32), (i32, i32))> {
        let anchors = self.wall_anchors();
        let tile = self.wall_tile_now(&anchors)?;
        let Some(from) = self.wall_from() else {
            return Some((tile, tile));
        };
        let to = if self.bridge_armed() {
            // Only straight lines cross water.
            axis_lock(from, tile)
        } else if self.input.shift {
            lock_angle(from, tile)
        } else {
            tile
        };
        let max = self.heightmap.tiles as i32 - 1;
        // A ring's sides are cut to fit; a line stops short.
        let to = if self.hud.wall_mode.ring.is_some() {
            to
        } else {
            clamp_end(from, to)
        };
        Some((from, (to.0.clamp(0, max), to.1.clamp(0, max))))
    }

    /// The simulation's check of a wall from `from` to `to`: as drawn by a
    /// worker, or as a blueprint.
    fn wall_check(
        &self,
        building: BuildingId,
        blueprint: bool,
        from: Tile,
        to: Tile,
    ) -> Result<Vec<WallTile>, Refusal> {
        let side = self.sandbox.side;
        if blueprint {
            self.world.blueprint(side, building, from, to)
        } else {
            self.world.wall_plan(side, building, from, to)
        }
    }

    fn wall_preview(&self, building: BuildingId) -> Option<Piece> {
        let (from, to) = self.wall_ends_now()?;
        Some(Piece {
            line: [from.0, from.1, to.0, to.1],
            plan: self.wall_check(building, self.hud.wall_mode.blueprint, from, to),
        })
    }

    /// The sides of a ring through `corners`, cut into orders: each
    /// side's own price is left to `order_ring`, which asks for the whole.
    fn ring_pieces(&self, building: BuildingId, blueprint: bool, corners: &[Tile]) -> Vec<Piece> {
        let side = self.sandbox.side;
        // A ring is bought whole, so a drawn side is checked for tier and
        // ground only; `order_ring` prices the lot and checks the room.
        let check = |from, to| {
            if blueprint {
                self.world.blueprint(side, building, from, to)
            } else {
                self.world.wall_fits(side, building, from, to)
            }
        };
        ring_tool::ring_pieces(corners, &check)
    }

    /// Whether a click now would close the ring: it has three corners and
    /// the mouse is on the first.
    fn closes_ring(&self) -> bool {
        let Some(ring) = self.hud.wall_mode.ring.as_ref().filter(|r| r.len() >= 3) else {
            return false;
        };
        self.ground_under_cursor()
            .is_some_and(|at| at.distance(buildings::centre_of(ring[0], 1, 1)) <= SNAP_REACH)
    }

    /// The straight runs from each main building of yours to its nearest
    /// seeps and veins, which a ring crosses where a gate would help.
    fn gate_routes(&self) -> Vec<(Vec2, Vec2)> {
        let (Some(all), Some(economy)) = (self.world.structures(), self.world.economy()) else {
            return Vec::new();
        };
        let side = self.sandbox.side;
        let mains: Vec<Vec2> = (all.iter())
            .filter(|s| s.owner == side)
            .filter(|s| {
                (self.buildings.stats.get(usize::from(s.building)))
                    .is_some_and(|b| b.role == Role::Main)
            })
            .map(|s| buildings::centre_of(s.tile, s.width, s.depth))
            .collect();
        let mut routes: Vec<(f32, Vec2, Vec2)> = economy
            .nodes()
            .filter(|n| n.left > 0)
            .filter_map(|n| {
                let at = from_fx_vec(n.at);
                (mains.iter())
                    .map(|&m| (m.distance(at), m, at))
                    .min_by(|a, b| a.0.total_cmp(&b.0))
            })
            .filter(|r| r.0 <= GATE_ROUTE_REACH)
            .collect();
        routes.sort_by(|a, b| a.0.total_cmp(&b.0));
        routes
            .into_iter()
            .take(GATE_ROUTES)
            .map(|(_, from, to)| (from, to))
            .collect()
    }

    /// The ring as it would be ordered now. `old` is the last frame's, kept
    /// as it is while nothing it was worked out from has changed.
    fn ring_preview(&self, building: BuildingId, old: Option<RingPreview>) -> Option<RingPreview> {
        let points = self.hud.wall_mode.ring.as_ref()?;
        let (_, cursor) = self.wall_ends_now()?;
        let closing = self.closes_ring();
        let mut corners = points.clone();
        if !closing && corners.last() != Some(&cursor) {
            corners.push(cursor);
        }
        let blueprint = self.hud.wall_mode.blueprint;
        let key = RingKey {
            corners,
            closing,
            blueprint,
            building,
            // Rebuilt once a second: the ground changes slowly enough.
            tick: self.world.tick_count() / gc_sim::TICKS_PER_SECOND,
        };
        if let Some(old) = old.filter(|o| o.key == key) {
            return Some(old);
        }
        let pieces = self.ring_pieces(building, blueprint, &key.corners);
        let laid = ring_tool::laid_tiles(&pieces);
        let gates = match self.buildings.gate_for(building) {
            Some(gate) if key.corners.len() >= 3 => {
                let stats = &self.buildings.stats[usize::from(gate)];
                ring_tool::gate_spots(
                    &ring_tool::ring_segments(&key.corners),
                    &self.gate_routes(),
                    &laid,
                    stats.width >= stats.depth,
                )
            }
            _ => Vec::new(),
        };
        Some(RingPreview {
            pieces,
            laid: laid.len(),
            gates,
            closing,
            key,
        })
    }

    /// Choose the wall tool for `building`.
    pub fn arm_wall(&mut self, building: BuildingId) {
        self.hud.wall_mode = WallMode::default();
        self.hud.armed = Some(Armed::Wall(building, None));
    }

    /// A key while the wall tool is armed: `B` switches between building and
    /// planning blueprints, `O` between lines and the ring, and Enter closes
    /// a ring. Whether the key was the tool's.
    pub fn wall_key(&mut self, code: KeyCode) -> bool {
        let Some(Armed::Wall(building, _)) = self.hud.armed else {
            return false;
        };
        if self.buildings.is_bridge(building) && matches!(code, KeyCode::KeyB | KeyCode::KeyO) {
            self.say(if code == KeyCode::KeyB {
                "A bridge is built by workers at once: it has no blueprints.".into()
            } else {
                "A bridge is a straight line from bank to bank: no rings.".into()
            });
            return true;
        }
        match code {
            KeyCode::KeyB => {
                if self.buildings.is_grown(building) {
                    self.say("A grown wall takes no worker, so it has no blueprints.".into());
                } else {
                    let on = !self.hud.wall_mode.blueprint;
                    self.hud.wall_mode.blueprint = on;
                    self.say(if on {
                        "Blueprints: nothing is paid until a worker builds each tile. B goes back."
                            .into()
                    } else {
                        "Building: a worker is sent and the wall is paid for at once.".into()
                    });
                }
            }
            KeyCode::KeyO => {
                let on = self.hud.wall_mode.ring.is_none();
                self.hud.wall_mode.ring = on.then(Vec::new);
                self.hud.armed = Some(Armed::Wall(building, None));
                self.say(if on {
                    "Ring: click each corner, then the first one again (or Enter) to close it."
                        .into()
                } else {
                    "Lines: click where the wall starts.".into()
                });
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                if self.hud.wall_mode.ring.is_none() {
                    return false;
                }
                self.order_ring(building);
            }
            _ => return false,
        }
        true
    }

    /// The ghost of the wall: one tile before it has a start, then every
    /// tile of the line (or of the ring, with the gates it suggests). This
    /// also works out the preview for the frame, which the hint reads too.
    pub fn push_wall_ghost(&mut self) {
        let Some(Armed::Wall(id, _)) = self.hud.armed else {
            self.hud.wall_preview = None;
            self.hud.ring_preview = None;
            self.hud.wall_mode = WallMode::default();
            return;
        };
        let colour = color_of(&self.parties, self.sandbox.side);
        let hm = &self.heightmap;
        let ground = |x, z| hm.height_at(x, z).max(hm.water_level());
        let blueprint = self.hud.wall_mode.blueprint;
        let open = if blueprint { Fit::Blueprint } else { Fit::Open };
        let look = self.buildings.wall_look(id, false);
        if self.hud.wall_mode.ring.is_some() {
            self.hud.wall_preview = None;
            let old = self.hud.ring_preview.take();
            self.hud.ring_preview = self.ring_preview(id, old);
            let Some(ring) = &self.hud.ring_preview else {
                return;
            };
            // The corners and joins are shared by two pieces: drawn once.
            let mut drawn = std::collections::HashSet::new();
            for piece in &ring.pieces {
                let tiles = ghost_tiles(piece, open)
                    .into_iter()
                    .filter(|&(t, _)| drawn.insert(t));
                buildings::push_wall_ghost(
                    piece.line,
                    look,
                    tiles,
                    colour,
                    ground,
                    &mut self.instances,
                );
            }
            if let Some(gate) = self.buildings.gate_for(id)
                && let Some(stats) = self.buildings.stats.get(usize::from(gate))
            {
                for spot in &ring.gates {
                    let (w, d) = stats.size(spot.turn);
                    let at = buildings::centre_of(spot.tile, 1, 1);
                    let tile = buildings::tile_for(at, w, d);
                    buildings::push_ghost(
                        stats,
                        tile,
                        spot.turn,
                        Fit::Suggested,
                        colour,
                        ground,
                        &mut self.instances,
                    );
                }
            }
            return;
        }
        self.hud.ring_preview = None;
        self.hud.wall_preview = self.wall_preview(id);
        let Some(piece) = &self.hud.wall_preview else {
            return;
        };
        buildings::push_wall_ghost(
            piece.line,
            look,
            ghost_tiles(piece, open).into_iter(),
            colour,
            ground,
            &mut self.instances,
        );
    }

    /// Your own planned tiles, as faint blue slabs along the lines they were
    /// planned in. Only yours: another side's plans are never drawn.
    pub fn push_plan_ghosts(&mut self) {
        if !self.conquest_match() || self.watching.is_some() {
            return;
        }
        let side = self.sandbox.side;
        let colour = color_of(&self.parties, side);
        let hm = &self.heightmap;
        let ground = |x, z| hm.height_at(x, z).max(hm.water_level());
        for p in self.world.plans().iter().filter(|p| p.owner == side) {
            buildings::push_wall_ghost(
                p.line.map(i32::from),
                self.buildings.wall_look(p.building, false),
                std::iter::once((p.tile, Fit::Planned)),
                colour,
                ground,
                &mut self.instances,
            );
        }
    }

    /// The line of the planned tile of yours under the mouse, if there is
    /// one: the ray through the mouse against each tile's slab (so a click
    /// on the slab's top finds it, not the ground behind it), then the
    /// tile of the ground under the mouse.
    pub fn plan_under_cursor(&self) -> Option<[i16; 4]> {
        let side = self.sandbox.side;
        let plans = || self.world.plans().iter().filter(move |p| p.owner == side);
        if let Some((origin, dir)) = self.cursor_ray() {
            let hm = &self.heightmap;
            let ground = |x, z| hm.height_at(x, z).max(hm.water_level());
            let hit = plans()
                .filter_map(|p| {
                    let (x, z) = (p.tile.0 as f32, p.tile.1 as f32);
                    let low = ground(x + 0.5, z + 0.5) - 0.3;
                    let high = low + 0.3 + self.buildings.wall_look(p.building, false).height;
                    let (min, max) = (
                        glam::Vec3::new(x, low, z),
                        glam::Vec3::new(x + 1.0, high, z + 1.0),
                    );
                    buildings::ray_box(origin, dir, min, max).map(|t| (t, p.line))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, line)) = hit {
                return Some(line);
            }
        }
        let at = self.ground_under_cursor()?;
        let tile = (at.x.floor() as i32, at.y.floor() as i32);
        plans().find(|p| p.tile == tile).map(|p| p.line)
    }

    /// The click while the wall tool is armed: the first sets the start;
    /// the next sends the order (or plans the wall), and the wall goes on
    /// from its end. In a ring, each click is a corner.
    pub fn finish_walling(&mut self, building: BuildingId) {
        if self.hud.wall_mode.ring.is_some() {
            return self.click_ring(building);
        }
        let Some((from, to)) = self.wall_ends_now() else {
            return;
        };
        if self.wall_from().is_none() {
            self.hud.armed = Some(Armed::Wall(building, Some(from)));
            return;
        }
        let blueprint = self.hud.wall_mode.blueprint;
        let plan = match self.wall_check(building, blueprint, from, to) {
            Ok(plan) => plan,
            Err(why) => return self.say(self.why_now(building, why).into()),
        };
        if let Some(why) = plan.iter().find_map(|t| t.fits.err()) {
            return self.say(self.why_now(building, why).into());
        }
        if plan.is_empty() {
            self.say(if blueprint {
                "Those tiles are already walled or planned.".into()
            } else {
                "Your walls already stand there.".into()
            });
            return;
        }
        // The tool keeps both ends within the map, so these fit.
        let (from16, to16) = ((from.0 as i16, from.1 as i16), (to.0 as i16, to.1 as i16));
        if blueprint {
            // Planning needs no worker, and none is named.
            self.orders([Command::PlanWall {
                building,
                from: from16,
                to: to16,
            }]);
            self.hud.armed = Some(Armed::Wall(building, Some(to)));
            return;
        }
        // A wall grown along veins (the Brood's) takes no worker.
        let regiment = if self.buildings.is_grown(building) {
            None
        } else {
            let Some(w) = self.nearest_worker(buildings::centre_of(from, 1, 1)) else {
                return;
            };
            Some(w)
        };
        self.orders([Command::BuildWall {
            regiment,
            building,
            from: from16,
            to: to16,
        }]);
        self.hud.armed = Some(Armed::Wall(building, Some(to)));
    }

    /// A click while drawing a ring: the first corner again closes and
    /// orders it; any other adds a corner.
    fn click_ring(&mut self, building: BuildingId) {
        let Some((_, to)) = self.wall_ends_now() else {
            return;
        };
        if self.closes_ring() {
            return self.order_ring(building);
        }
        if let Some(ring) = &mut self.hud.wall_mode.ring
            && ring.last() != Some(&to)
        {
            ring.push(to);
        }
    }

    /// Order the closed ring: one command per piece that can go (a worker
    /// draws every piece, or the pieces are planned as blueprints), leaving
    /// out what can't. The tool stays on, ready for the next ring.
    fn order_ring(&mut self, building: BuildingId) {
        let Some(corners) = self.hud.wall_mode.ring.clone() else {
            return;
        };
        if corners.len() < 3 {
            return self.say("A ring needs at least three corners.".into());
        }
        let blueprint = self.hud.wall_mode.blueprint;
        let pieces = self.ring_pieces(building, blueprint, &corners);
        let sendable = pieces.iter().filter(|p| p.sendable()).count();
        if sendable == 0 {
            let why = match (pieces.is_empty(), pieces.iter().find_map(Piece::why)) {
                (true, _) => Refusal::Rough,
                (false, Some(why)) => why,
                (false, None) => {
                    return self.say("Your walls already stand there.".into());
                }
            };
            return self.say(buildings::why(why).into());
        }
        let tiles = ring_tool::laid_tiles(&pieces).len();
        if blueprint {
            let side = self.sandbox.side;
            let planned = self
                .world
                .plans()
                .iter()
                .filter(|p| p.owner == side)
                .count();
            if planned + tiles > MAX_PLANS {
                return self.say(format!(
                    "A side can plan at most {MAX_PLANS} tiles at once."
                ));
            }
        } else if let Some(stats) = self.buildings.stats.get(usize::from(building)) {
            let (grub, marrow) = wall_price(stats, tiles as u32);
            let bank = self.world.bank(self.sandbox.side);
            if bank.is_some_and(|b| b.grub < grub || b.marrow < marrow) {
                return self.say(format!(
                    "You can't afford the whole ring: it costs {}.",
                    buildings::price_text(grub, marrow)
                ));
            }
        }
        // Every piece names the same worker, the one nearest where it starts.
        let worker = if blueprint || self.buildings.is_grown(building) {
            None
        } else {
            let first = pieces
                .iter()
                .find(|p| p.sendable())
                .map_or(corners[0], |p| (p.line[0], p.line[1]));
            let Some(w) = self.nearest_worker(buildings::centre_of(first, 1, 1)) else {
                return self.say("Select a worker to build the ring.".into());
            };
            Some(w)
        };
        let commands = ring_tool::ring_commands(&pieces, building, blueprint, worker);
        self.orders(commands);
        self.hud.wall_mode.ring = Some(Vec::new());
        let left_out = pieces.iter().filter(|p| p.red()).count();
        let what = if blueprint { "planned" } else { "ordered" };
        let mut line = format!("Ring {what}: {tiles} tiles in {sendable} pieces");
        if left_out > 0 {
            line.push_str(&format!(", {left_out} left out (red)"));
        }
        self.say(line);
    }

    /// `why` in words, for a bridge when one is armed (the wall words for
    /// rough ground and a crooked line don't fit it).
    fn why_now(&self, building: BuildingId, why: Refusal) -> &'static str {
        if self.buildings.is_bridge(building) {
            bridge_why(why)
        } else {
            buildings::why(why)
        }
    }

    /// The line shown while drawing a wall: the start to choose, or the
    /// length and price of the wall (or why it can't go up).
    pub fn wall_hint(&self) -> Option<String> {
        let Some(Armed::Wall(id, from)) = self.hud.armed else {
            return None;
        };
        let blueprint = self.hud.wall_mode.blueprint;
        if let Some(ring) = &self.hud.wall_mode.ring {
            let Some(preview) = self.hud.ring_preview.as_ref().filter(|_| !ring.is_empty()) else {
                return Some(ring_start_hint(blueprint));
            };
            let stats = self.buildings.stats.get(usize::from(id))?;
            let tiles = preview.laid;
            let (grub, marrow) = wall_price(stats, tiles as u32);
            let red = preview.pieces.iter().filter(|p| p.red()).count();
            return Some(ring_hint(
                tiles,
                (!blueprint)
                    .then(|| buildings::price_text(grub, marrow))
                    .as_deref(),
                red,
                ring.len() >= 3,
                preview.closing,
            ));
        }
        let bridge = self.buildings.is_bridge(id);
        if from.is_none() {
            return Some(if bridge {
                bridge_start_hint()
            } else {
                start_hint(blueprint)
            });
        }
        let stats = self.buildings.stats.get(usize::from(id))?;
        let Piece { plan, .. } = self.hud.wall_preview.as_ref()?;
        let plan = match plan {
            Ok(plan) => plan,
            Err(why) => return Some(self.why_now(id, *why).into()),
        };
        if let Some(why) = plan.iter().find_map(|t| t.fits.err())
            && why != Refusal::Overgrown
        {
            return Some(self.why_now(id, why).into());
        }
        let n = plan.len();
        let (grub, marrow) = wall_price(stats, n as u32);
        let price = (!blueprint).then(|| buildings::price_text(grub, marrow));
        let clear = if plan.iter().any(|t| t.fits.is_err()) {
            ", ground to clear"
        } else {
            ""
        };
        Some(hint_line(
            n,
            price.as_deref(),
            clear,
            self.buildings.is_grown(id),
        ))
    }

    /// A line for the status bar while some of the selected workers are
    /// working your planned walls.
    pub fn plans_hint(&self) -> Option<String> {
        let n = self
            .selected_workers()
            .filter(|&h| self.world.works_plans(h))
            .count();
        (n > 0).then(|| {
            format!(
                "{n} {} working your planned walls - any other order sends {} elsewhere",
                buildings::plural(n, "worker is", "workers are"),
                buildings::plural(n, "it", "them"),
            )
        })
    }
}

/// What `n` tiles of the wall `stats` cost, as (Grub, Marrow).
fn wall_price(stats: &BuildingStats, n: u32) -> (u32, u32) {
    (
        u32::from(stats.cost_grub) * n,
        u32::from(stats.cost_marrow) * n,
    )
}

/// The tiles of a checked line to draw, each with how it sits. A line that
/// only crosses your own walls plans no tile: show where it lies. A line
/// that can't be planned is all red.
fn ghost_tiles(piece: &Piece, open: Fit) -> Vec<(Tile, Fit)> {
    match &piece.plan {
        Ok(plan) => plan.iter().map(|t| (t.tile, fit_of(t, open))).collect(),
        Err(_) => {
            let [x0, y0, x1, y1] = piece.line;
            (gc_sim::walls::wall_tiles((x0, y0), (x1, y1)).unwrap_or_default())
                .into_iter()
                .map(|t| (t, Fit::Refused))
                .collect()
        }
    }
}

/// The hint before a bridge has a start.
fn bridge_start_hint() -> String {
    "Draw a straight bridge from bank to bank: click the near bank - right-click or Esc to stop"
        .into()
}

/// A refusal in a bridge's words.
fn bridge_why(why: Refusal) -> &'static str {
    match why {
        Refusal::Rough => "A bridge needs dry land at both ends and some water between them.",
        Refusal::Built => "Something is already built or bridged there.",
        other => buildings::why(other),
    }
}

/// The hint before a line has a start.
fn start_hint(blueprint: bool) -> String {
    if blueprint {
        "Blueprint: click where the wall starts - B builds instead, O ring - right-click or Esc to stop"
    } else {
        "Click where the wall starts - B plans a blueprint, O ring - right-click or Esc to stop"
    }
    .into()
}

/// The hint before a ring has a first corner.
fn ring_start_hint(blueprint: bool) -> String {
    let what = if blueprint { "Ring blueprint" } else { "Ring" };
    format!("{what}: click the first corner - O goes back to lines - right-click or Esc to stop")
}

/// The hint while a ring is drawn: its size and price (or that blueprints
/// are paid as workers build), the sides left out, and how to close it.
///
/// `price` is `None` for a blueprint.
fn ring_hint(
    tiles: usize,
    price: Option<&str>,
    red: usize,
    closable: bool,
    closing: bool,
) -> String {
    let word = buildings::plural(tiles, "tile", "tiles");
    let mut line = match price {
        None => format!("Ring blueprint: {tiles} {word}, paid as workers build them"),
        Some(price) => format!("Ring: {tiles} {word}, {price}"),
    };
    if red > 0 {
        let pieces = buildings::plural(red, "piece", "pieces");
        line.push_str(&format!(" - {red} red {pieces} won't go"));
    }
    line.push_str(if closing {
        " - click to close the ring"
    } else if closable {
        " - click the first corner or press Enter to close it"
    } else {
        " - click the next corner"
    });
    line
}

/// The hint while a wall is drawn: a worker builds most walls, a grown one
/// grows on its own, and a blueprint is paid for as workers build it.
///
/// `price` is `None` for a blueprint.
fn hint_line(n: usize, price: Option<&str>, clear: &str, grown: bool) -> String {
    let tiles = buildings::plural(n, "tile", "tiles");
    let Some(price) = price else {
        return format!(
            "Blueprint: {n} {tiles}{clear}, paid as workers build them - click to plan and go on - right-click to finish"
        );
    };
    let then = if grown {
        "grows on its own - click to go on"
    } else {
        "click to build and go on"
    };
    format!("{n} {tiles}, {price}{clear} - {then} - right-click to finish")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile_anchor(x: i32, y: i32) -> Anchor {
        Anchor {
            rect: [x, y, 1, 1],
            joins: true,
        }
    }

    #[test]
    fn a_grown_wall_hint_names_no_worker() {
        let grown = hint_line(3, Some("30 Grub"), "", true);
        assert!(grown.contains("grows on its own"), "{grown}");
        assert!(!grown.contains("build"), "{grown}");
        let built = hint_line(3, Some("30 Grub"), "", false);
        assert!(built.contains("click to build and go on"), "{built}");
    }

    #[test]
    fn a_blueprint_hint_names_no_price_and_says_workers_pay() {
        let plan = hint_line(12, None, "", false);
        assert!(plan.contains("paid as workers build"), "{plan}");
        assert!(!plan.contains("120 Grub"), "{plan}");
        let ring = ring_hint(40, None, 0, true, false);
        assert!(
            ring.contains("paid as workers build") && !ring.contains("400"),
            "{ring}"
        );
        let built = ring_hint(40, Some("400 Grub"), 2, true, false);
        assert!(
            built.contains("400 Grub") && built.contains("2 red pieces"),
            "{built}"
        );
        assert!(ring_hint(3, Some("30 Grub"), 0, false, false).contains("next corner"));
    }

    #[test]
    fn hints_fit_on_one_line() {
        // The bar is about a hundred and fifty characters wide at the
        // smallest window.
        for line in [
            start_hint(false),
            start_hint(true),
            bridge_start_hint(),
            ring_start_hint(true),
            hint_line(32, Some("320 Grub, 100 Marrow"), ", ground to clear", false),
            hint_line(32, None, ", ground to clear", false),
            ring_hint(300, Some("3000 Grub, 100 Marrow"), 12, true, false),
        ] {
            assert!(line.len() <= 150, "{} chars: {line}", line.len());
            assert!(line.is_ascii(), "{line}");
        }
    }

    #[test]
    fn a_bridge_end_is_turned_onto_the_nearer_axis() {
        let from = (10, 10);
        assert_eq!(axis_lock(from, (20, 12)), (20, 10));
        assert_eq!(axis_lock(from, (8, 3)), (10, 3));
        assert_eq!(axis_lock(from, (14, 14)), (14, 10));
        assert_eq!(axis_lock(from, from), from);
        for dx in -9..=9 {
            for dy in -9..=9 {
                let (x, y) = axis_lock(from, (10 + dx, 10 + dy));
                assert!(x == 10 || y == 10, "{dx},{dy}");
            }
        }
    }

    #[test]
    fn open_ground_picks_the_tile_under_the_mouse() {
        assert_eq!(snap_tile(Vec2::new(10.7, 20.2), &[]), (10, 20));
        let far = [tile_anchor(30, 30)];
        assert_eq!(snap_tile(Vec2::new(10.7, 20.2), &far), (10, 20));
    }

    #[test]
    fn a_wall_end_in_reach_takes_the_wall() {
        let ends = [tile_anchor(12, 20)];
        // A tile and a half from the end: still snaps.
        assert_eq!(snap_tile(Vec2::new(10.6, 20.5), &ends), (12, 20));
        // Beyond reach does not.
        assert_eq!(snap_tile(Vec2::new(9.0, 20.5), &ends), (9, 20));
    }

    #[test]
    fn the_nearest_anchor_wins() {
        let ends = [tile_anchor(12, 20), tile_anchor(10, 21)];
        assert_eq!(snap_tile(Vec2::new(11.0, 21.0), &ends), (10, 21));
    }

    #[test]
    fn a_gate_is_joined_on_the_tile_nearest_the_mouse() {
        let gate = [Anchor {
            rect: [20, 20, 3, 1],
            joins: true,
        }];
        assert_eq!(snap_tile(Vec2::new(22.4, 21.6), &gate), (22, 20));
        assert_eq!(snap_tile(Vec2::new(19.0, 20.5), &gate), (20, 20));
    }

    #[test]
    fn a_tower_is_met_on_the_tile_beside_it() {
        let tower = [Anchor {
            rect: [20, 20, 2, 2],
            joins: false,
        }];
        // Never one of its own tiles.
        for p in [
            Vec2::new(21.0, 21.0),
            Vec2::new(20.1, 20.1),
            Vec2::new(19.4, 22.0),
            Vec2::new(23.0, 23.0),
        ] {
            let (x, y) = snap_tile(p, &tower);
            let inside = (20..22).contains(&x) && (20..22).contains(&y);
            assert!(!inside, "{p:?} gave {x},{y}");
        }
        assert_eq!(snap_tile(Vec2::new(18.9, 20.5), &tower), (19, 20));
        assert_eq!(snap_tile(Vec2::new(21.5, 23.4), &tower), (21, 22));
    }

    #[test]
    fn the_angle_lock_goes_to_fifteen_degree_steps() {
        let from = (10, 10);
        // Flat stays flat, however it wobbles.
        assert_eq!(lock_angle(from, (20, 11)), (20, 10));
        assert_eq!(lock_angle(from, (10, 4)), (10, 4));
        // A true diagonal is kept.
        assert_eq!(lock_angle(from, (16, 16)), (16, 16));
        // 30 degrees from flat: 10 tiles out lands on 8.7 across, 5 down.
        let (x, y) = lock_angle(from, (19, 16));
        let angle = ((y - 10) as f32).atan2((x - 10) as f32).to_degrees();
        assert!((angle - 30.0).abs() < 4.0, "{angle}");
        // The start itself has no angle.
        assert_eq!(lock_angle(from, from), from);
    }

    #[test]
    fn every_locked_angle_is_near_a_step() {
        let from = (30, 30);
        for dx in -20..=20 {
            for dy in -20..=20 {
                if dx * dx + dy * dy < 100 {
                    continue;
                }
                let (x, y) = lock_angle(from, (30 + dx, 30 + dy));
                let angle = ((y - 30) as f32).atan2((x - 30) as f32).to_degrees();
                let off = (angle / ANGLE_STEP - (angle / ANGLE_STEP).round()).abs() * ANGLE_STEP;
                // Rounding to whole tiles blurs it by a little.
                assert!(off < 3.5, "{dx},{dy} -> {x},{y}: {angle}");
            }
        }
    }

    #[test]
    fn a_long_wall_stops_at_the_most_tiles() {
        let from = (10, 10);
        assert_eq!(clamp_end(from, (30, 10)), (30, 10));
        let last = 10 + MAX_WALL_TILES as i32 - 1;
        assert_eq!(clamp_end(from, (last, 10)), (last, 10));
        assert_eq!(clamp_end(from, (last + 5, 10)), (last, 10));
        assert_eq!(clamp_end(from, (10, 10 - 100)), (10, 10 - (last - 10)));
        for to in [(200, 77), (-90, 40), (10, 500), (-60, -60)] {
            let end = clamp_end(from, to);
            let tiles = gc_sim::walls::wall_tiles(from, end);
            assert!(tiles.is_some_and(|t| t.len() <= MAX_WALL_TILES), "{to:?}");
        }
        // The direction survives the shortening.
        let (x, y) = clamp_end(from, (110, 60));
        assert!((((y - 10) as f32 / (x - 10) as f32) - 0.5).abs() < 0.05);
    }
}
