//! Clearing land as the game shows it (docs/32-changing-land.md): the area
//! a left-drag chooses for the Clear order, the amber that marks overgrown
//! ground, and which tiles lose their trees when the simulation clears
//! them. The orders and the rules are the simulation's (`Command::Clear`,
//! `Event::Cleared`).

use gc_render::BoxInstance;
use gc_sim::clearing::MAX_CLEAR_SIDE;
use gc_sim::events::Event;
use gc_sim::{BuildingStats, Command, Growth, World};
use glam::Vec2;

use crate::buildings::{self, Fit};
use crate::hud::Armed;
use crate::scenery::Stood;
use crate::{Running, color_of};

/// Overgrown ground that will be cleared: amber.
const AMBER: [u8; 4] = [235, 170, 40, 130];
/// The rest of an area being cleared: a pale green wash.
const WASH: [u8; 4] = [150, 235, 150, 50];
/// The wash over the area a worker is clearing while it is selected.
const WORK: [u8; 4] = [235, 235, 150, 40];

/// The tile `at` is in, as a position along one side of the map.
fn tile_of(at: f32) -> i32 {
    at.floor() as i32
}

/// The start and length of the tiles between `from` and `to` along one
/// side, at most [`MAX_CLEAR_SIDE`] of them and within `0..tiles`. A drag
/// longer than that keeps the tile it began on and stops short of the
/// mouse.
fn run(from: f32, to: f32, tiles: i32) -> (i32, i32) {
    let (a, b) = (tile_of(from), tile_of(to));
    let (mut lo, mut hi) = (a.min(b), a.max(b));
    if hi - lo + 1 > MAX_CLEAR_SIDE {
        if b >= a {
            hi = lo + MAX_CLEAR_SIDE - 1;
        } else {
            lo = hi - MAX_CLEAR_SIDE + 1;
        }
    }
    let lo = lo.clamp(0, (tiles - 1).max(0));
    let hi = hi.clamp(lo, (tiles - 1).max(0));
    (lo, hi - lo + 1)
}

/// The area `[x, y, width, depth]` that a drag from ground point `from` to
/// `to` chooses on a map `tiles` across: every tile between them, at most
/// 16 by 16. A click (both points in one tile) chooses that tile.
pub fn dragged_area(from: Vec2, to: Vec2, tiles: i32) -> [i32; 4] {
    let (x, w) = run(from.x, to.x, tiles);
    let (y, d) = run(from.y, to.y, tiles);
    [x, y, w, d]
}

/// The tiles of `area` (`[x, y, width, depth]`) where something still
/// grows, row by row.
pub fn overgrown_tiles(world: &World, area: [i32; 4]) -> impl Iterator<Item = (i32, i32)> + '_ {
    let [x, y, w, d] = area;
    let terrain = world.terrain();
    (y..y + d)
        .flat_map(move |ty| (x..x + w).map(move |tx| (tx, ty)))
        .filter(|&(tx, ty)| terrain.growth(tx, ty) != Growth::Open)
}

/// Draw `area` on the ground: a wash over all of it (`chosen` for the area
/// being dragged out, else the one a worker is clearing), and amber over the
/// tiles where something still grows.
pub fn push_area(
    world: &World,
    area: [i32; 4],
    chosen: bool,
    ground: impl Fn(f32, f32) -> f32,
    out: &mut Vec<BoxInstance>,
) {
    buildings::push_plate(area, if chosen { WASH } else { WORK }, &ground, out);
    for (x, y) in overgrown_tiles(world, area) {
        buildings::push_plate([x, y, 1, 1], AMBER, &ground, out);
    }
}

/// The tiles the simulation cleared in the last tick, so the trees on them
/// can come down. Replays and network games tick a world too, so they hear
/// of it the same way.
pub fn cleared(events: &[Event]) -> impl Iterator<Item = (i32, i32)> + '_ {
    events.iter().filter_map(|e| match *e {
        Event::Cleared { x, y, .. } => Some((x, y)),
        _ => None,
    })
}

/// The words under the cursor while laying a building on overgrown ground.
pub const OVERGROWN_HINT: &str = "Overgrown: the worker clears it first";

impl Running {
    /// How a building of `stats` at `tile` sits on the ground, given why it
    /// can't go there if it can't.
    pub fn fit_of(&self, stats: &BuildingStats, tile: (i32, i32), why: Option<&str>) -> Fit {
        let foot = stats.footprint(tile, self.sandbox.building_turn);
        if why.is_some() {
            Fit::Refused
        } else if self.conquest_match() && self.world.overgrown(foot) {
            Fit::Overgrown
        } else {
            Fit::Open
        }
    }

    /// The marks of clearing on the field: the area being dragged out for a
    /// Clear order, the area each selected worker is clearing, the
    /// overgrown tiles under a building being placed, and a faint
    /// blueprint of every building of yours waiting on its ground.
    pub fn push_clearing_marks(&mut self) {
        if !self.conquest_match() {
            return;
        }
        let (hm, world) = (&self.heightmap, &self.world);
        let ground = |x, z| hm.height_at(x, z).max(hm.water_level());
        let side = self.sandbox.side;
        let out = &mut self.instances;
        if self.watching.is_none() {
            let mut seen: Vec<[i32; 4]> = Vec::new();
            for h in self.selected.iter().copied() {
                let Some(job) = world.clearing_of(h) else {
                    continue;
                };
                let area = job.area;
                if !seen.contains(&area) {
                    seen.push(area);
                    push_area(world, area, false, ground, out);
                }
            }
        }
        for (owner, building, tile, turn) in world.pledged() {
            let stats = self.buildings.stats.get(usize::from(building));
            if let Some(stats) = stats.filter(|_| owner == side || self.reveal_all) {
                let colour = color_of(&self.parties, owner);
                buildings::push_blueprint(stats, tile, turn, colour, ground, out);
            }
        }
        if matches!(self.hud.armed, Some(Armed::Clear))
            && let Some(from) = self.input.clear_from
            && let Some(to) = self.ground_under_cursor()
        {
            let area = dragged_area(from, to, self.heightmap.tiles as i32);
            push_area(&self.world, area, true, ground, &mut self.instances);
        }
    }

    /// The area a Clear order would take if the mouse were released now: a
    /// drag out from where it began, or the tile under the mouse.
    fn clear_area_now(&self) -> Option<[i32; 4]> {
        let to = self.ground_under_cursor()?;
        let from = self.input.clear_from.unwrap_or(to);
        Some(dragged_area(from, to, self.heightmap.tiles as i32))
    }

    /// The release of the drag after Clear: every selected worker clears
    /// the area. An area with nothing growing in it keeps the order armed.
    /// Shift keeps it armed for the next area too.
    pub fn finish_clearing(&mut self) {
        let Some([x, y, w, d]) = self.clear_area_now() else {
            return;
        };
        self.input.clear_from = None;
        if !self.world.overgrown([x, y, w, d]) {
            self.say("Nothing grows there to clear.".into());
            return;
        }
        // Sides are at most 16, so these fit.
        let (tile, size) = ((x as i16, y as i16), (w as u8, d as u8));
        let crew: Vec<_> = self.selected_workers().collect();
        self.orders(crew.into_iter().map(|regiment| Command::Clear {
            regiment,
            tile,
            size,
        }));
        if !self.input.shift {
            self.hud.armed = None;
        }
    }

    /// The line shown while choosing ground to clear, or laying a building
    /// on overgrown ground.
    pub fn clearing_hint(&self) -> Option<String> {
        match self.hud.armed? {
            Armed::Clear => Some(
                "Drag over the ground to clear, or click one tile - Esc or right-click to cancel"
                    .into(),
            ),
            Armed::Lay(id) => {
                let (stats, tile, why) = self.building_placement(id)?;
                (self.fit_of(&stats, tile, why) == Fit::Overgrown)
                    .then(|| OVERGROWN_HINT.to_string())
            }
            _ => None,
        }
    }

    /// Take the trees and bushes off the tiles cleared in the tick just
    /// played. Replays and network games tick a world too, so they come
    /// here the same way.
    pub fn fell_scenery(&mut self) {
        // Trees come down for the viewer only where it can see, and not
        // for a rejoining player catching up on ticks long past.
        let catching_up = self.online.is_some() && self.menu.net().catching_up();
        let watch = !catching_up;
        for tile in cleared(self.world.events()) {
            let scenery = &self.scenery;
            if watch && (self.reveal_all || self.fog.sees_tile(tile)) {
                let tall = |s: &Stood| scenery.height_of(s.model);
                self.ambient
                    .felling
                    .clear(tile, scenery.standing(tile), tall);
            }
            self.scars.stumps_left(self.scenery.standing(tile));
            self.scenery.fell(std::iter::once(tile));
        }
        self.scars.felled(self.world.events(), self.heightmap.tiles);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_chooses_its_tile() {
        let p = Vec2::new(10.4, 20.9);
        assert_eq!(dragged_area(p, p, 64), [10, 20, 1, 1]);
    }

    #[test]
    fn a_drag_chooses_every_tile_between_in_either_direction() {
        let (a, b) = (Vec2::new(10.5, 20.5), Vec2::new(13.5, 18.5));
        assert_eq!(dragged_area(a, b, 64), [10, 18, 4, 3]);
        assert_eq!(dragged_area(b, a, 64), [10, 18, 4, 3]);
    }

    #[test]
    fn a_long_drag_stops_at_sixteen_tiles_on_the_side_it_began() {
        let side = MAX_CLEAR_SIDE;
        let from = Vec2::new(20.5, 20.5);
        let right = dragged_area(from, Vec2::new(60.5, 20.5), 64);
        assert_eq!(right, [20, 20, side, 1]);
        let left = dragged_area(from, Vec2::new(0.5, 20.5), 64);
        assert_eq!(left, [20 - side + 1, 20, side, 1]);
    }

    #[test]
    fn the_area_stays_on_the_map() {
        let off = dragged_area(Vec2::new(-5.0, 62.0), Vec2::new(2.5, 90.0), 64);
        assert_eq!(off, [0, 62, 3, 2]);
    }

    #[test]
    fn only_cleared_tiles_are_heard_of() {
        let events = [
            Event::Cleared {
                x: 3,
                y: 4,
                was: Growth::Woods,
                burnt: false,
            },
            Event::Cleared {
                x: 5,
                y: 6,
                was: Growth::Scrub,
                burnt: true,
            },
        ];
        assert_eq!(cleared(&events).collect::<Vec<_>>(), [(3, 4), (5, 6)]);
        assert_eq!(cleared(&[]).count(), 0);
    }
}
