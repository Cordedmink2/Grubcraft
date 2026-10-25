//! The Engineer: the works a Conquest mind puts up against its enemies
//! (docs/18-ai.md, "Engineer"; docs/26-fortifications.md, stage 8). It
//! is part of the Steward, spending the economy's attention, a spare
//! worker, and only the money left over after what the Steward is saving
//! for, every couple of seconds:
//!
//! - a **front wall** across the way the nearest enemy comes, with a gate
//!   in the middle, drawn a few tiles at a time out from the gate, and the
//!   stretches by the gate raised a level once it is rich;
//! - a **barricade** in front of its rally point, mid-game;
//! - **stakes** planted by spearmen and archers waiting at the rally point
//!   once it has seen enemy riders or monsters;
//! - a few hidden **traps** on the approach once Grub piles up;
//! - a **bridge** over the narrowest water on the way to its enemies when
//!   water cuts it off from all of them.
//!
//! Its level says which of these it does ([`Fortify`]); its personality's
//! caution how soon and how much. Manning the walls and assaulting the
//! enemy's are the Marshal's.

use gc_sim::structures::{GATE, GROWN, Refusal, Role, StructureHandle};
use gc_sim::walls::WallTile;
use gc_sim::{BuildingId, Command, Mass, RegimentHandle, TICKS_PER_SECOND};
use glam::Vec2;

use super::{Look, Sense, Steward};
use crate::conquest::placing;
use crate::conquest::strategy::Fortify;
use crate::orders::Hands as Clicks;

/// Ticks between its looks.
const THINK_TICKS: u32 = 2 * TICKS_PER_SECOND;
/// When it starts on the front wall without tier 2, ticks: 10 minutes for
/// the boldest, 6 for the most cautious.
const WALL_AFTER_BOLD: u32 = 10 * 60 * TICKS_PER_SECOND;
const WALL_AFTER_CAUTIOUS: u32 = 6 * 60 * TICKS_PER_SECOND;
/// How far out from home the front wall goes, tiles: as far as fits, from
/// the first to the last. Inside the rally point (16 tiles out), so the
/// army gathers in front of its wall.
const WALL_OUT: [i32; 5] = [12, 11, 10, 9, 8];
/// Tiles of wall each side of the gate, and a piece drawn at a time.
const WALL_HALF: i32 = 8;
const PIECE: i32 = 3;
/// The share of a line's tiles (percent) that must fit for it to be the
/// front wall's line.
const LINE_FITS: usize = 60;
/// Grub it keeps in hand before it spends on works, and the most of what
/// the Steward is saving for that it also keeps: works are cheap, and a
/// mind always saving for its next hall would never put any up.
const RESERVE: u32 = 120;
const SAVING_KEPT: u32 = 150;
/// Grub in hand beyond its saving from which it raises the wall.
const RICH: u32 = 500;
/// How long to wait before looking again for a front wall line where none
/// fitted (a Choir's power fields grow), ticks.
const LINE_RETRY: u32 = 60 * TICKS_PER_SECOND;
/// The barricade: tiles across, and how far in front of the rally point.
const BARRICADE_HALF: i32 = 2;
const BARRICADE_OUT: f32 = 4.0;
/// Looks it tries for a barricade at a rally point before giving it up
/// (troops may stand where it would go).
const BARRICADE_TRIES: u8 = 5;
/// Stakes: planted by regiments this near the rally point, at most this
/// many lines.
const STAKES_NEAR: f32 = 8.0;
const STAKE_LINES: usize = 2;
/// Traps: laid from this many tiles out, once it has this much Grub to
/// spare; two, or three for a cautious mind.
const TRAPS_OUT: f32 = 22.0;
const TRAP_GRUB: u32 = 300;
const TRAPS: u8 = 2;
/// Longest bridge it draws, tiles.
const BRIDGE_LONGEST: i32 = 16;

/// A straight line of tiles, from one tile to another.
type Line = ((i32, i32), (i32, i32));

/// The front wall it has planned.
#[derive(Clone, Debug)]
struct Front {
    /// The pieces still to draw, nearest the gate first, each from tile
    /// to tile.
    pieces: Vec<Line>,
    /// Where the gate goes and its turn, until laid.
    gate: Option<((i32, i32), u8)>,
    /// The gate's three tiles along the line: walled instead if the gate
    /// can't go there.
    middle: Line,
    /// A wall tile of each stretch it raises: either side of the gate, or
    /// the middle of a line with no gate.
    raise: Vec<(i32, i32)>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Engineer {
    next_think: u32,
    front: Option<Front>,
    /// No line fitted: look again from then.
    next_line: u32,
    raised: usize,
    /// The rally point it has barricaded, or tried to.
    barricaded: Option<Vec2>,
    /// Looks spent trying for a barricade where none fitted.
    barricade_tries: u8,
    /// Regiments sent to plant stakes.
    staked: Vec<RegimentHandle>,
    traps: u8,
    /// The bridge it means to draw: `None` until it has looked, then the
    /// crossing, or none where it needs or can draw none (or has drawn it).
    bridge: Option<Option<Line>>,
}

impl Steward {
    /// The Engineer's look: put up whatever works are due, with what
    /// Grub is spare, in front of `rally`. One order a look, given while
    /// the economy has attention for it.
    pub(super) fn fortify(
        &mut self,
        s: &Sense,
        clicks: &mut Clicks,
        look: &Look,
        rally: Option<Vec2>,
    ) {
        if s.plan.fortifies == Fortify::None || s.now < self.engineer.next_think {
            return;
        }
        self.engineer.next_think = s.now + THINK_TICKS;
        let (Some(home), Some(bank)) = (self.home, s.world.bank(s.party)) else {
            return;
        };
        let Some(toward) =
            super::super::nearest_other_start(s, home).and_then(|e| (e - home).try_normalize())
        else {
            return;
        };
        if !clicks.free() {
            return;
        }
        let kept = RESERVE + self.ledger.saving.0.min(SAVING_KEPT);
        let spare = bank.grub.saturating_sub(kept);
        let full = s.plan.fortifies == Fortify::Full;
        let late = s.now >= wall_after(s.caution);
        // Most pressing first: a way to the enemy, then field works, then
        // the wall, then traps.
        let order = (full)
            .then(|| self.bridge(s, look, home, spare))
            .flatten()
            .or_else(|| {
                let rally = rally?;
                ((look.tier >= 2 || late)
                    && self
                        .engineer
                        .barricaded
                        .is_none_or(|b| b.distance(rally) >= 4.0))
                .then(|| self.barricade(s, look, rally, toward, spare))
                .flatten()
                .or_else(|| self.stakes(s, look, rally))
            })
            // Short of an army, soldiers come before walls and traps.
            .or_else(|| {
                if self.arming || !full {
                    return None;
                }
                (look.tier >= 2 || late)
                    .then(|| self.wall(s, look, home, toward, spare))
                    .flatten()
                    .or_else(|| self.traps(s, look, home, toward, spare))
            });
        if let Some((order, worker)) = order {
            // There is attention for it (checked above), so it is given.
            clicks.give(s.now, order);
            if let Some(w) = worker {
                self.sent.push((w, s.now));
            }
        }
    }

    /// Who builds a work at `at`: nobody for one that grows on its own,
    /// else a spare worker; `None` when it needs one and none is spare.
    fn hands(
        &self,
        s: &Sense,
        look: &Look,
        grown: bool,
        at: (i32, i32),
    ) -> Option<Option<RegimentHandle>> {
        if grown {
            return Some(None);
        }
        let at = Vec2::new(at.0 as f32 + 0.5, at.1 as f32 + 0.5);
        self.builder(s, look, at).map(Some)
    }

    /// The front wall's next order: its gate first, then the next piece
    /// that fits drawn out from the gate, then raising the stretches by
    /// the gate once rich. The order and the worker it sends, if any.
    fn wall(
        &mut self,
        s: &Sense,
        look: &Look,
        home: Vec2,
        toward: Vec2,
        spare: u32,
    ) -> Option<Order> {
        let wall = s.kit.wall()?;
        if self.engineer.front.is_none() && s.now >= self.engineer.next_line {
            self.engineer.front = plan_front(s, look, self, home, toward);
            if self.engineer.front.is_none() {
                self.engineer.next_line = s.now + LINE_RETRY;
            }
        }
        let gate_at = self.engineer.front.as_ref()?.gate;
        if let Some((tile, turn)) = gate_at {
            let gate = s.kit.gate()?;
            if spare < u32::from(gate.stats.cost_grub) {
                return None;
            }
            // Its ground taken meanwhile: the wall is drawn through the
            // middle instead.
            if s.world.can_lay(s.party, gate.id, tile, turn).is_err() {
                let front = self.engineer.front.as_mut()?;
                front.gate = None;
                front.pieces.insert(0, front.middle);
                return None;
            }
            let w = self.builder_at(s, look, tile)?;
            self.engineer.front.as_mut()?.gate = None;
            let order = Command::Build {
                regiment: w,
                building: gate.id,
                tile: (tile.0 as i16, tile.1 as i16),
                turn,
            };
            return Some((order, Some(w)));
        }
        // A piece needs at most this to draw: none at all while poorer.
        if spare < u32::from(wall.stats.cost_grub) * PIECE as u32 {
            return None;
        }
        while let Some(&(from, to)) = self.engineer.front.as_ref()?.pieces.first() {
            let tiles = s
                .world
                .wall_fits(s.party, wall.id, from, to)
                .unwrap_or_default();
            let front = self.engineer.front.as_mut()?;
            // Someone standing there: tried again after the rest.
            let crowded = |t: &WallTile| t.fits == Err(Refusal::Crowded);
            if tiles.iter().any(crowded) && tiles.iter().all(|t| t.fits.is_ok() || crowded(t)) {
                front.pieces.rotate_left(1);
                return None;
            }
            // Walled already, or not all of it fits: on to the next.
            if tiles.is_empty() || tiles.iter().any(|t| t.fits.is_err()) {
                front.pieces.remove(0);
                continue;
            }
            let worker = self.hands(s, look, wall.stats.flags & GROWN != 0, from)?;
            self.engineer.front.as_mut()?.pieces.remove(0);
            let order = Command::BuildWall {
                regiment: worker,
                building: wall.id,
                from: (from.0 as i16, from.1 as i16),
                to: (to.0 as i16, to.1 as i16),
            };
            return Some((order, worker));
        }
        // All drawn: raise the stretches by the gate once rich, each once.
        let raise = &self.engineer.front.as_ref()?.raise;
        if self.engineer.raised >= raise.len() || spare < RICH {
            return None;
        }
        let tile = raise[self.engineer.raised];
        let h = own_wall_at(look, tile);
        let raise = h.and_then(|h| s.world.wall_level(s.party, h).ok());
        let Some((h, raise)) = h.zip(raise).filter(|(_, r)| r.afford && spare >= r.grub) else {
            self.engineer.raised += 1;
            return None;
        };
        let grown = s
            .kit
            .building(raise.to)
            .is_some_and(|b| b.stats.flags & GROWN != 0);
        let worker = self.hands(s, look, grown, tile)?;
        self.engineer.raised += 1;
        let order = Command::RaiseWall {
            structure: h,
            regiment: worker,
        };
        Some((order, worker))
    }

    /// The nearest spare worker to tile `at`.
    fn builder_at(&self, s: &Sense, look: &Look, at: (i32, i32)) -> Option<RegimentHandle> {
        self.hands(s, look, false, at).flatten()
    }

    /// A barricade across the front of the rally point, once for each
    /// rally point: out in front, a little further, or nearer, whichever
    /// fits first.
    fn barricade(
        &mut self,
        s: &Sense,
        look: &Look,
        rally: Vec2,
        toward: Vec2,
        spare: u32,
    ) -> Option<Order> {
        let b = s.kit.works(Role::Barricade)?;
        let n = 2 * BARRICADE_HALF + 1;
        if spare < u32::from(b.stats.cost_grub) * n as u32 {
            return None;
        }
        let w = self.builder(s, look, rally)?;
        let line = [BARRICADE_OUT, BARRICADE_OUT + 2.0, BARRICADE_OUT - 2.0]
            .into_iter()
            .map(|out| across(rally + toward * out, upright(toward), BARRICADE_HALF))
            .find(|&(from, to)| {
                s.world
                    .wall_fits(s.party, b.id, from, to)
                    .is_ok_and(|t| t.len() as i32 == n && t.iter().all(|t| t.fits.is_ok()))
            });
        // Troops in the way: tried again a few looks later, then given up.
        self.engineer.barricade_tries += 1;
        if line.is_some() || self.engineer.barricade_tries >= BARRICADE_TRIES {
            self.engineer.barricaded = Some(rally);
            self.engineer.barricade_tries = 0;
        }
        let (from, to) = line?;
        let order = Command::BuildWall {
            regiment: Some(w),
            building: b.id,
            from: (from.0 as i16, from.1 as i16),
            to: (to.0 as i16, to.1 as i16),
        };
        Some((order, Some(w)))
    }

    /// Against enemy riders or monsters it has seen, a spear or archer
    /// regiment waiting at the rally point plants a line of stakes, up to
    /// [`STAKE_LINES`].
    fn stakes(&mut self, s: &Sense, look: &Look, rally: Vec2) -> Option<Order> {
        if self.engineer.staked.len() >= STAKE_LINES || !heavy_foes(s) {
            return None;
        }
        let b = s.kit.works(Role::Stakes)?;
        let r = look.army.iter().find(|r| {
            !r.moving
                && !r.engaged
                && r.pos.distance(rally) <= STAKES_NEAR
                && !self.engineer.staked.contains(&r.handle)
                && s.world.plant_check(r.handle, b.id).is_ok()
        })?;
        self.engineer.staked.push(r.handle);
        let order = Command::PlantStakes {
            regiment: r.handle,
            building: b.id,
        };
        Some((order, None))
    }

    /// A hidden trap on the approach, up to [`TRAPS`] (one more for a
    /// cautious mind), once Grub piles up; none where laying one would use
    /// up the worker (the Brood's Gnawers).
    fn traps(
        &mut self,
        s: &Sense,
        look: &Look,
        home: Vec2,
        toward: Vec2,
        spare: u32,
    ) -> Option<Order> {
        let most = TRAPS + u8::from(s.caution > 0.5);
        if self.engineer.traps >= most || spare < TRAP_GRUB {
            return None;
        }
        let b = s.kit.works(Role::Trap)?;
        if s.kit.uses_up_builder(b) {
            return None;
        }
        let k = f32::from(self.engineer.traps);
        let side = Vec2::new(-toward.y, toward.x);
        // Spread across the way in and a little further out each time.
        let at = home + toward * (TRAPS_OUT + 3.0 * k) + side * (k - 1.0) * 3.0;
        let tile = placing::spot_for(s.world, s.party, b.id, &b.stats, at, &self.lanes(s, look));
        let Some(tile) = tile else {
            // Nowhere near there: that one is given up.
            self.engineer.traps += 1;
            return None;
        };
        let w = self.builder_at(s, look, tile)?;
        self.engineer.traps += 1;
        let order = Command::Build {
            regiment: w,
            building: b.id,
            tile: (tile.0 as i16, tile.1 as i16),
            turn: 0,
        };
        Some((order, Some(w)))
    }

    /// When water cuts it off from every enemy start, bridge the narrowest
    /// crossing near the straight way to the nearest. It looks once.
    fn bridge(&mut self, s: &Sense, look: &Look, home: Vec2, spare: u32) -> Option<Order> {
        let b = s.kit.works(Role::Bridge)?;
        let line = (*self.engineer.bridge.get_or_insert_with(|| {
            let target = super::super::nearest_other_start(s, home)?;
            if !s.terrain.cut_off(home, super::super::other_starts(s, home)) {
                return None;
            }
            narrowest_crossing(s, b.id, home, target)
        }))?;
        let (from, to) = line;
        let len = (to.0 - from.0).abs().max((to.1 - from.1).abs()) as u32 + 1;
        if spare < u32::from(b.stats.cost_grub) * len {
            return None;
        }
        let w = self.builder_at(s, look, from)?;
        self.engineer.bridge = Some(None);
        let order = Command::BuildWall {
            regiment: Some(w),
            building: b.id,
            from: (from.0 as i16, from.1 as i16),
            to: (to.0 as i16, to.1 as i16),
        };
        Some((order, Some(w)))
    }
}

/// An order for the economy's hands, and the worker it sends (to leave
/// alone a while), if any.
type Order = (Command, Option<RegimentHandle>);

/// When a mind with `caution` starts on its wall without tier 2.
fn wall_after(caution: f32) -> u32 {
    let c = caution.clamp(0.0, 1.0);
    let span = (WALL_AFTER_BOLD - WALL_AFTER_CAUTIOUS) as f32;
    WALL_AFTER_BOLD - (span * c) as u32
}

/// Whether a line across `toward` runs upright (north to south): when
/// the way is more east-west than north-south.
fn upright(toward: Vec2) -> bool {
    toward.x.abs() >= toward.y.abs()
}

/// The tiles from and to of a straight line `half` tiles each side of
/// `mid`, upright or level.
fn across(mid: Vec2, upright: bool, half: i32) -> Line {
    let (x, y) = (mid.x.floor() as i32, mid.y.floor() as i32);
    if upright {
        ((x, y - half), (x, y + half))
    } else {
        ((x - half, y), (x + half, y))
    }
}

/// The front wall's line: across the way the enemy comes, as far out from
/// home as [`LINE_FITS`] of its tiles fit off the lanes to its seeps and
/// veins (and its gate, where the faction has one, fits in the middle).
/// Upright or level, whichever is more across the way first, and shifted
/// a little along itself where the ground is rough.
fn plan_front(
    s: &Sense,
    look: &Look,
    steward: &Steward,
    home: Vec2,
    toward: Vec2,
) -> Option<Front> {
    let wall = s.kit.wall()?;
    let gate = s.kit.gate();
    let lanes = steward.lanes(s, look);
    let clear = |t: (i32, i32)| {
        !lanes
            .iter()
            .any(|&(a, b)| placing::crosses([t.0, t.1, 1, 1], a, b))
    };
    let first = upright(toward);
    // The other axis only for a way near the diagonal.
    let ratio = toward.x.abs().min(toward.y.abs()) / toward.x.abs().max(toward.y.abs());
    let axes: &[bool] = if ratio > 0.5 {
        &[first, !first]
    } else {
        &[first]
    };
    for out in WALL_OUT {
        for &up in axes {
            // Out along the way to the enemy, as far as `out` on the axis
            // the line stands across.
            let reach = if up { toward.x.abs() } else { toward.y.abs() };
            let centre = home + toward / reach * out as f32;
            for shift in [0, -3, 3] {
                let along = if up { Vec2::Y } else { Vec2::X };
                let mid = centre + along * shift as f32;
                let (from, to) = across(mid, up, WALL_HALF);
                let Ok(tiles) = s.world.wall_fits(s.party, wall.id, from, to) else {
                    continue;
                };
                let fit = tiles
                    .iter()
                    .filter(|t| t.fits.is_ok() && clear(t.tile))
                    .count();
                if tiles.is_empty() || fit * 100 < tiles.len() * LINE_FITS {
                    continue;
                }
                let (mx, my) = (mid.x.floor() as i32, mid.y.floor() as i32);
                // A gate (1 by 3) along the line: as it stands across an
                // upright line, turned a quarter across a level one.
                let gate_at = match gate {
                    Some(g) => {
                        let at = if up {
                            ((mx, my - 1), 0)
                        } else {
                            ((mx - 1, my), 1)
                        };
                        if s.world.can_lay(s.party, g.id, at.0, at.1).is_err() || !clear((mx, my)) {
                            continue;
                        }
                        Some(at)
                    }
                    None => None,
                };
                // Along the line, `d` tiles from its middle.
                let at = |d: i32| if up { (mx, my + d) } else { (mx + d, my) };
                let gap = if gate.is_some() { 2 } else { 0 };
                let mut pieces = Vec::new();
                for sign in [-1, 1] {
                    let mut d = gap;
                    while d <= WALL_HALF {
                        let end = (d + PIECE - 1).min(WALL_HALF);
                        let (a, b) = (at(sign * d), at(sign * end));
                        if clear(a) && clear(b) {
                            pieces.push((a, b));
                        }
                        d = end + 1;
                    }
                }
                // Nearest the gate first, side by side.
                pieces.sort_by_key(|&(a, _)| (a.0 - mx).abs() + (a.1 - my).abs());
                return Some(Front {
                    pieces,
                    gate: gate_at,
                    middle: (at(-1), at(1)),
                    raise: if gate.is_some() {
                        vec![at(-gap), at(gap)]
                    } else {
                        vec![at(0)]
                    },
                });
            }
        }
    }
    None
}

/// Its own finished wall tile (not a gate) at `tile`, if one stands
/// there.
fn own_wall_at(look: &Look, tile: (i32, i32)) -> Option<StructureHandle> {
    look.mine
        .iter()
        .find(|m| {
            m.view.tile == tile
                && m.finished()
                && m.stats.role == Role::Wall
                && m.stats.flags & GATE == 0
        })
        .map(|m| m.view.handle)
}

/// Whether it has seen enemy riders or monsters (heavy regiments, which
/// stakes stop).
fn heavy_foes(s: &Sense) -> bool {
    crate::conquest::spymaster::enemy_army(s).any(|r| s.view.unit(r).stats.mass == Mass::Heavy)
}

/// The shortest bridge (from bank to bank) that fits over the water on
/// the way from `home` to `target`: crossings straight along that way and
/// along each axis, from points either side of it.
fn narrowest_crossing(s: &Sense, bridge: BuildingId, home: Vec2, target: Vec2) -> Option<Line> {
    let way = (target - home).try_normalize()?;
    let side = Vec2::new(-way.y, way.x);
    let len = home.distance(target);
    let mut best: Option<(i32, Line)> = None;
    for k in -6..=6 {
        let start = home + side * (k as f32 * 3.0);
        // Where this walk first meets water it can't wade.
        let Some(wet) = (0..len as i32)
            .map(|d| start + way * d as f32)
            .find(|&p| s.terrain.water(p) && s.terrain.blocked(p))
        else {
            continue;
        };
        let axes = [
            way,
            Vec2::new(way.x.signum(), 0.0),
            Vec2::new(0.0, way.y.signum()),
        ];
        for dir in axes.into_iter().filter(|d| d.length() > 0.5) {
            let dir = dir.normalize();
            let Some((from, to)) = banks(s, wet, dir) else {
                continue;
            };
            let n = (to.0 - from.0).abs().max((to.1 - from.1).abs());
            if n > BRIDGE_LONGEST || best.is_some_and(|(b, _)| b <= n) {
                continue;
            }
            let fits = s
                .world
                .wall_fits(s.party, bridge, from, to)
                .is_ok_and(|t| !t.is_empty() && t.iter().all(|t| t.fits.is_ok()));
            if fits {
                best = Some((n, (from, to)));
            }
        }
    }
    best.map(|(_, line)| line)
}

/// The last land tile before the water at `wet` going back along `dir`,
/// and the first land tile after it going on: a bridge's two ends.
fn banks(s: &Sense, wet: Vec2, dir: Vec2) -> Option<Line> {
    let tile = |p: Vec2| (p.x.floor() as i32, p.y.floor() as i32);
    let land = |p: Vec2| !s.terrain.water(p) && !s.terrain.blocked(p);
    let back = (1..=BRIDGE_LONGEST)
        .map(|d| wet - dir * d as f32)
        .find(|&p| land(p))?;
    let on = (1..=BRIDGE_LONGEST)
        .map(|d| wet + dir * d as f32)
        .find(|&p| land(p))?;
    Some((tile(back), tile(on)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_across_the_way_runs_on_the_axis_more_across_it() {
        assert!(upright(Vec2::new(1.0, 0.2)));
        assert!(!upright(Vec2::new(0.3, -1.0)));
        assert_eq!(across(Vec2::new(10.5, 20.5), true, 2), ((10, 18), (10, 22)));
        assert_eq!(across(Vec2::new(10.5, 20.5), false, 2), ((8, 20), (12, 20)));
    }

    #[test]
    fn the_cautious_wall_sooner() {
        assert_eq!(wall_after(0.0), WALL_AFTER_BOLD);
        assert_eq!(wall_after(1.0), WALL_AFTER_CAUTIOUS);
        assert!(wall_after(0.5) < WALL_AFTER_BOLD && wall_after(0.5) > WALL_AFTER_CAUTIOUS);
    }
}
