//! The wall tool (docs/26-fortifications.md, "Drawn walls"; docs/08-ui-ux.md):
//! a worker draws a wall from one tile to another. The first click sets the
//! start, the ghost follows the mouse, and the next click sends the order
//! and starts the next piece from that end, so the wall bends. The rules,
//! prices and the check are the simulation's (`Command::BuildWall`,
//! `World::wall_plan`); this is the picking of the two ends and the ghost.

use gc_sim::structures::{GATE, Role};
use gc_sim::walls::{MAX_WALL_TILES, WallTile};
use gc_sim::{BuildingId, Command, Refusal};
use glam::Vec2;

use crate::buildings::{self, Fit};
use crate::hud::Armed;
use crate::{Running, color_of};

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

/// How a planned tile sits on the ground.
fn fit_of(tile: &WallTile) -> Fit {
    match tile.fits {
        Ok(()) => Fit::Open,
        Err(Refusal::Overgrown) => Fit::Overgrown,
        Err(_) => Fit::Refused,
    }
}

/// What the tool shows for the wall as it would be drawn now.
pub struct Preview {
    pub line: [i32; 4],
    pub plan: Result<Vec<WallTile>, Refusal>,
}

impl Running {
    /// The tile the wall being drawn goes on from, once its start is
    /// clicked.
    fn wall_from(&self) -> Option<(i32, i32)> {
        match self.hud.armed {
            Some(Armed::Wall(_, from)) => from,
            _ => None,
        }
    }

    /// The places a wall of yours likes to end on: the ends of your own
    /// walls, your towers and your gates.
    fn wall_anchors(&self) -> Vec<Anchor> {
        let Some(all) = self.world.structures() else {
            return Vec::new();
        };
        let side = self.sandbox.side;
        let mut out = Vec::new();
        for s in all.iter().filter(|s| s.owner == side) {
            let Some(stats) = self.buildings.stats.get(usize::from(s.building)) else {
                continue;
            };
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
        let to = if self.input.shift {
            lock_angle(from, tile)
        } else {
            tile
        };
        let max = self.heightmap.tiles as i32 - 1;
        let to = clamp_end(from, to);
        Some((from, (to.0.clamp(0, max), to.1.clamp(0, max))))
    }

    fn wall_preview(&self, building: BuildingId) -> Option<Preview> {
        let (from, to) = self.wall_ends_now()?;
        Some(Preview {
            line: [from.0, from.1, to.0, to.1],
            plan: self.world.wall_plan(self.sandbox.side, building, from, to),
        })
    }

    /// Choose the wall tool for `building`.
    pub fn arm_wall(&mut self, building: BuildingId) {
        self.hud.armed = Some(Armed::Wall(building, None));
    }

    /// The ghost of the wall: one tile before it has a start, then every
    /// tile of the line. This also works out the preview for the frame,
    /// which the hint reads too.
    pub fn push_wall_ghost(&mut self) {
        self.hud.wall_preview = match self.hud.armed {
            Some(Armed::Wall(id, _)) => self.wall_preview(id),
            _ => None,
        };
        let Some(Preview { line, plan }) = &self.hud.wall_preview else {
            return;
        };
        let colour = color_of(&self.parties, self.sandbox.side);
        let hm = &self.heightmap;
        let ground = |x, z| hm.height_at(x, z).max(hm.water_level());
        // A line that only crosses your own walls plans no tile: show
        // where it lies. A line that can't be planned is all red.
        let tiles: Vec<_> = match plan {
            Ok(plan) => plan.iter().map(|t| (t.tile, fit_of(t))).collect(),
            Err(_) => gc_sim::walls::wall_tiles((line[0], line[1]), (line[2], line[3]))
                .unwrap_or_default()
                .into_iter()
                .map(|t| (t, Fit::Refused))
                .collect(),
        };
        buildings::push_wall_ghost(
            *line,
            tiles.into_iter(),
            colour,
            ground,
            &mut self.instances,
        );
    }

    /// The click while the wall tool is armed: the first sets the start;
    /// the next sends the order, and the wall goes on from its end.
    pub fn finish_walling(&mut self, building: BuildingId) {
        let Some((from, to)) = self.wall_ends_now() else {
            return;
        };
        if self.wall_from().is_none() {
            self.hud.armed = Some(Armed::Wall(building, Some(from)));
            return;
        }
        let plan = match self.world.wall_plan(self.sandbox.side, building, from, to) {
            Ok(plan) => plan,
            Err(why) => return self.say(buildings::why(why).into()),
        };
        if let Some(why) = plan.iter().find_map(|t| t.fits.err()) {
            return self.say(buildings::why(why).into());
        }
        if plan.is_empty() {
            self.say("Your walls already stand there.".into());
            return;
        }
        let Some(regiment) = self.nearest_worker(buildings::centre_of(from, 1, 1)) else {
            return;
        };
        // The tool keeps both ends within the map, so these fit.
        self.orders([Command::BuildWall {
            regiment,
            building,
            from: (from.0 as i16, from.1 as i16),
            to: (to.0 as i16, to.1 as i16),
        }]);
        self.hud.armed = Some(Armed::Wall(building, Some(to)));
    }

    /// The line shown while drawing a wall: the start to choose, or the
    /// length and price of the wall (or why it can't go up).
    pub fn wall_hint(&self) -> Option<String> {
        let Some(Armed::Wall(id, from)) = self.hud.armed else {
            return None;
        };
        if from.is_none() {
            return Some("Click where the wall starts - right-click or Esc to stop".into());
        }
        let stats = self.buildings.stats.get(usize::from(id))?;
        let Preview { plan, .. } = self.hud.wall_preview.as_ref()?;
        let plan = match plan {
            Ok(plan) => plan,
            Err(why) => return Some(buildings::why(*why).into()),
        };
        if let Some(why) = plan.iter().find_map(|t| t.fits.err())
            && why != Refusal::Overgrown
        {
            return Some(buildings::why(why).into());
        }
        let n = plan.len() as u32;
        let (grub, marrow) = (
            u32::from(stats.cost_grub) * n,
            u32::from(stats.cost_marrow) * n,
        );
        let tiles = if n == 1 { "tile" } else { "tiles" };
        let price = match marrow {
            0 => format!("{grub} Grub"),
            m => format!("{grub} Grub, {m} Marrow"),
        };
        let clear = if plan.iter().any(|t| t.fits.is_err()) {
            ", ground to clear"
        } else {
            ""
        };
        Some(format!(
            "{n} {tiles}, {price}{clear} - click to build and go on - right-click to finish"
        ))
    }
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
