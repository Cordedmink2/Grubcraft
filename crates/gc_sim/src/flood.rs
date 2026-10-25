//! Fords that flood (docs/32-changing-land.md, "Fords that flood"): long
//! rain swells the rivers. The water climbs the banks, so troops wade
//! further out from the river, and once it is up the fords can't be
//! crossed: their cells are shut on the walkable grid, and anyone still
//! in one is swept about and struggles to the nearest bank. When the rain
//! stops the water goes down again and the fords open.
//!
//! With seasons (docs/38-living-waterways.md, stage 1) the rivers follow
//! the year too: the winter's snow piles up and melts in the spring,
//! swelling the rivers as long rain does, and a dry summer drops the water
//! below its line, so deep water at the edge of the channels can be waded:
//! the summer fords.

use alloc::sync::Arc;
use alloc::vec::Vec;

use gc_fixed::FxVec2;

use crate::TICKS_PER_SECOND;
use crate::flash::Flash;
use crate::hash::StateHasher;
use crate::morale;
use crate::nav::Nav;
use crate::regiments::Regiments;
use crate::sky::{Season, SeasonAt, WET_FULL};
use crate::terrain::{HEIGHT_UNITS, Terrain};

/// The rivers rise while the ground is at least this wet: under three
/// minutes of heavy rain on dry ground.
pub const FLOOD_WET: u16 = WET_FULL / 2;
/// They go down again once it has dried below this: a minute and a half of
/// fair weather after the rain stops, more after a long downpour.
pub const EBB_WET: u16 = WET_FULL * 3 / 10;
/// How far the water rises in a flood, in height units: three tenths of a
/// tile, about as deep as a ford was.
pub const FLOOD_RISE: u16 = (HEIGHT_UNITS * 3 / 10) as u16;
/// Height units the water rises a second while it floods (the full rise
/// in about 25 seconds) and falls a second as it goes down (about 75).
pub const RISE_PER_SECOND: u16 = 3;
pub const EBB_PER_SECOND: u16 = 1;
/// Hit points a regiment caught in a flooded ford loses a second.
pub const SWEPT_HP: u32 = 12;
/// Morale points it loses a second.
pub const SWEPT_MORALE: i16 = 4;
/// How far a dry summer drops the water below its line, in height units:
/// a fifth of a tile.
pub const LOW_DROP: u16 = (HEIGHT_UNITS / 5) as u16;
/// How deep below the line a regiment can wade, in height units, as the
/// map's walking grid has it (0.35 of a tile).
pub const WADE_DEPTH: u16 = (HEIGHT_UNITS * 35 / 100) as u16;
/// Height units the water falls a second in a dry summer (the full drop
/// in under a minute), and comes back once the summer ends.
pub const LOW_PER_SECOND: u16 = 1;
/// Snow piled up a second of the heaviest snowfall (at strength 255); the
/// pack melts at [`MELT_PER_SECOND`], so a second of heavy snow is about a
/// second and a half of melt.
const SNOW_PER_SECOND: u32 = 255;
/// Snow melting a second in the spring; what is left at the summer is
/// gone.
pub const MELT_PER_SECOND: u32 = 170;
/// Less snow than a minute of melt, as the spring comes in, melts without
/// swelling the rivers.
pub const MELT_MIN: u32 = 60 * MELT_PER_SECOND;

/// The rivers' fords, and whether a flood has shut them. How far the
/// water has risen is the terrain's ([`Terrain::rise`]).
#[derive(Clone, Debug)]
pub struct Flood {
    /// The tiles under the water line that a regiment can walk on (some
    /// cell of theirs isn't blocked ground), as `(y, x)`, sorted: the
    /// fords. Empty on a map with none.
    fords: Arc<[(i32, i32)]>,
    /// The fords as runs along each row (`[x, y, w, 1]`), to shut or open
    /// on the walkable grid.
    runs: Arc<[[i32; 4]]>,
    /// Whether the fords are shut.
    closed: bool,
    /// The tiles of deep water a dry summer makes wadeable, as `(y, x)`,
    /// sorted: the summer fords. Empty on a map with none.
    shallows: Arc<[(i32, i32)]>,
    /// The summer fords as runs along each row.
    shallow_runs: Arc<[[i32; 4]]>,
    /// Whether the summer fords are open.
    exposed: bool,
    /// The winter's snow, still to melt.
    snowpack: u32,
    /// The rivers, and the flash flood running down one (see
    /// [`crate::flash`]).
    pub(crate) flash: Flash,
}

/// What one second of the rivers changed (see [`Flood::second`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Turn {
    /// The fords just shut (`Some(true)`) or opened again.
    pub(crate) fords: Option<bool>,
    /// The summer fords just opened (`Some(true)`) or went deep again.
    pub(crate) shallows: Option<bool>,
}

/// Runs along each row of the sorted `(y, x)` tiles.
pub(crate) fn runs_of(tiles: &[(i32, i32)]) -> Vec<[i32; 4]> {
    let mut runs: Vec<[i32; 4]> = Vec::new();
    for &(y, x) in tiles {
        match runs.last_mut() {
            Some(run) if run[1] == y && run[0] + run[2] == x => run[2] += 1,
            _ => runs.push([x, y, 1, 1]),
        }
    }
    runs
}

/// Of the sorted `(y, x)` tiles on a map `tiles` a side, those in groups
/// (side by side) that join ground on two sides: the land along a group's
/// rim, where `walk` says a tile can be walked on, falls in two or more
/// pieces. So a bar across a channel is kept and a strip along one bank
/// is not. Sorted as given.
fn crossings(cands: &[(i32, i32)], tiles: i32, walk: impl Fn(i32, i32) -> bool) -> Vec<(i32, i32)> {
    const NONE: u32 = u32::MAX;
    let at = |x: i32, y: i32| {
        ((0..tiles).contains(&x) && (0..tiles).contains(&y)).then(|| (y * tiles + x) as usize)
    };
    // Each candidate's group (`NONE - 1` until found), and each rim
    // tile's marks: the group whose rim it is, and the group it was last
    // walked for.
    let mut group = alloc::vec![NONE; (tiles * tiles) as usize];
    let mut rim = alloc::vec![NONE; (tiles * tiles) as usize];
    let mut walked = alloc::vec![NONE; (tiles * tiles) as usize];
    for &(y, x) in cands {
        group[(y * tiles + x) as usize] = NONE - 1;
    }
    let mut keep = Vec::new();
    let (mut stack, mut members, mut edge) = (Vec::new(), Vec::new(), Vec::new());
    for (g, &(y, x)) in cands.iter().enumerate() {
        let g = g as u32;
        if group[(y * tiles + x) as usize] != NONE - 1 {
            continue;
        }
        // The group, side by side.
        members.clear();
        edge.clear();
        group[(y * tiles + x) as usize] = g;
        stack.push((x, y));
        while let Some((x, y)) = stack.pop() {
            members.push((y, x));
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if let Some(i) = at(x + dx, y + dy).filter(|&i| group[i] == NONE - 1) {
                    group[i] = g;
                    stack.push((x + dx, y + dy));
                }
            }
            for (dx, dy) in NEIGHBOURS {
                let (nx, ny) = (x + dx, y + dy);
                if let Some(i) = at(nx, ny).filter(|&i| group[i] == NONE && rim[i] != g)
                    && walk(nx, ny)
                {
                    rim[i] = g;
                    edge.push((nx, ny));
                }
            }
        }
        // The rim's pieces: walk the first, and see if any is left.
        if let Some(&first) = edge.first() {
            stack.push(first);
            walked[at(first.0, first.1).unwrap_or(0)] = g;
        }
        while let Some((x, y)) = stack.pop() {
            for (dx, dy) in NEIGHBOURS {
                if let Some(i) = at(x + dx, y + dy).filter(|&i| rim[i] == g && walked[i] != g) {
                    walked[i] = g;
                    stack.push((x + dx, y + dy));
                }
            }
        }
        if edge
            .iter()
            .any(|&(x, y)| at(x, y).is_some_and(|i| walked[i] != g))
        {
            keep.extend_from_slice(&members);
        }
    }
    keep.sort_unstable();
    keep
}

/// The eight tiles round a tile.
const NEIGHBOURS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

impl Flood {
    /// The fords of a map `map_tiles` a side with `terrain`, walked on
    /// the grid `nav`.
    pub(crate) fn new(terrain: &Terrain, nav: &Nav, map_tiles: u32, seed: u64) -> Flood {
        let tiles = map_tiles as i32;
        let (mut fords, mut shallows) = (Vec::new(), Vec::new());
        for (y, x) in (0..tiles).flat_map(|y| (0..tiles).map(move |x| (y, x))) {
            if !terrain.tile_under_water(x, y) {
                continue;
            }
            if nav.walkable_ground(x, y) {
                fords.push((y, x));
            }
            // Deep water no deeper than wading depth below the low line,
            // with no cliff beside it.
            if terrain.tile_within(x, y, LOW_DROP + WADE_DEPTH)
                && terrain.tile_gentle(x, y)
                && nav.deep_ground(x, y)
            {
                shallows.push((y, x));
            }
        }
        let shallows = crossings(&shallows, tiles, |x, y| nav.walkable_ground(x, y));
        let flash = Flash::new(terrain, &fords, &shallows, map_tiles, seed);
        Flood {
            runs: runs_of(&fords).into(),
            fords: fords.into(),
            closed: false,
            shallow_runs: runs_of(&shallows).into(),
            shallows: shallows.into(),
            exposed: false,
            snowpack: 0,
            flash,
        }
    }

    /// Whether the fords are shut by the flood.
    pub fn closed(&self) -> bool {
        self.closed
    }

    /// The fords, as (column, row): shut while [`Flood::closed`].
    pub fn fords(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.fords.iter().map(|&(y, x)| (x, y))
    }

    /// The fords as runs along each row (`[x, y, w, 1]`).
    pub(crate) fn runs(&self) -> &[[i32; 4]] {
        &self.runs
    }

    /// Whether the summer fords are open.
    pub fn exposed(&self) -> bool {
        self.exposed
    }

    /// The summer fords, as (column, row): wadeable while
    /// [`Flood::exposed`].
    pub fn shallows(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.shallows.iter().map(|&(y, x)| (x, y))
    }

    /// The summer fords as runs along each row (`[x, y, w, 1]`).
    pub(crate) fn shallow_runs(&self) -> &[[i32; 4]] {
        &self.shallow_runs
    }

    /// Whether `p` is on a summer ford.
    pub fn on_shallows(&self, p: FxVec2) -> bool {
        (self.shallows)
            .binary_search(&(p.y.floor_int(), p.x.floor_int()))
            .is_ok()
    }

    /// The winter's snow still to melt (0 with none).
    pub fn snowpack(&self) -> u32 {
        self.snowpack
    }

    /// Whether `p` is in a ford the flood has shut.
    pub fn sweeps(&self, p: FxVec2, terrain: &Terrain) -> bool {
        let (x, y) = (p.x.floor_int(), p.y.floor_int());
        let shut =
            (self.closed && self.fords.binary_search(&(y, x)).is_ok()) || self.flash.shuts(x, y);
        shut && !terrain.on_deck(p)
    }

    /// The rivers, and the flash flood running down one, if any.
    pub fn flash(&self) -> &Flash {
        &self.flash
    }

    /// One second of the rivers, with the ground `wet`, `at` a point of the
    /// year (with seasons), with snow falling at `snow` (0 to 255): the water in
    /// `terrain` rises, holds or goes down, and with `shut` the fords shut
    /// or open with it, and the summer fords open or go deep (a rehearsal
    /// keeps them as it found them, so it never changes the walkable grid
    /// it shares). Returns what changed.
    pub(crate) fn second(
        &mut self,
        terrain: &mut Terrain,
        wet: u16,
        at: Option<SeasonAt>,
        snow: u8,
        shut: bool,
    ) -> Turn {
        if terrain.is_flat() || terrain.water_level() == 0 {
            return Turn::default();
        }
        // The winter's snow piles up, and melts in the spring: a little
        // melts away unseen, more swells the rivers as rain does.
        let season = at.map(|at| at.season);
        let melting = match season {
            Some(Season::Winter) => {
                self.snowpack += SNOW_PER_SECOND * u32::from(snow) / 255;
                false
            }
            Some(Season::Spring) => {
                // Weighed once, as the spring comes in.
                if at.is_some_and(|at| at.into < TICKS_PER_SECOND) && self.snowpack < MELT_MIN {
                    self.snowpack = 0;
                }
                let melt = self.snowpack.min(MELT_PER_SECOND);
                self.snowpack -= melt;
                melt > 0
            }
            // What the spring didn't melt is gone by the summer.
            _ => {
                self.snowpack = 0;
                false
            }
        };
        let (rise, low) = (terrain.rise(), terrain.low());
        if wet >= FLOOD_WET || melting {
            // Low water comes back first, then the flood.
            if low > 0 {
                terrain.set_low(low.saturating_sub(RISE_PER_SECOND));
            } else {
                terrain.set_rise((rise + RISE_PER_SECOND).min(FLOOD_RISE));
            }
        } else {
            if wet < EBB_WET {
                terrain.set_rise(rise.saturating_sub(EBB_PER_SECOND));
            }
            let dry_summer = season == Some(Season::Summer) && wet < EBB_WET;
            if dry_summer && terrain.rise() == 0 {
                terrain.set_low((low + LOW_PER_SECOND).min(LOW_DROP));
            } else if !dry_summer {
                terrain.set_low(low.saturating_sub(LOW_PER_SECOND));
            }
        }
        let mut turn = Turn::default();
        if !shut {
            return turn;
        }
        let need = if self.closed {
            FLOOD_RISE / 2
        } else {
            FLOOD_RISE
        };
        let now = !self.fords.is_empty() && terrain.rise() >= need;
        if now != self.closed {
            self.closed = now;
            turn.fords = Some(now);
        }
        let need = if self.exposed { LOW_DROP / 2 } else { LOW_DROP };
        let now = !self.shallows.is_empty() && terrain.low() >= need;
        if now != self.exposed {
            self.exposed = now;
            turn.shallows = Some(now);
        }
        turn
    }

    /// One second of the regiments caught in shut fords: they lose hit
    /// points and morale. Into `out` (cleared first), the slots of those
    /// left alive with nowhere to go but where they stand: no order, or
    /// a chase the shut ford cuts off, and not fighting. They struggle to
    /// the nearest bank (see `World::rivers`).
    pub(crate) fn sweep(&self, regs: &mut Regiments, terrain: &Terrain, out: &mut Vec<usize>) {
        out.clear();
        if !self.closed && !self.flash.shut() {
            return;
        }
        for i in 0..regs.alive.len() {
            if !regs.alive[i] || regs.models[i] == 0 || !self.sweeps(regs.position[i], terrain) {
                continue;
            }
            regs.take_damage(i, SWEPT_HP);
            if regs.models[i] == 0 {
                continue;
            }
            morale::shift(regs, i, -SWEPT_MORALE);
            let stuck = regs.target[i].is_none() || regs.attack_target[i].is_some();
            if stuck && regs.foe(i).is_none() {
                out.push(i);
            }
        }
    }

    /// Whether the year has left anything to hash: low water, or snow
    /// still to melt (see [`Flood::hash_into`]).
    pub(crate) fn seasonal(&self, low: u16) -> bool {
        low > 0 || self.snowpack > 0
    }

    /// Hashed with the ground while the water is up (`rise` height units
    /// above its line) or the year has left something (`low` water, or
    /// snow to melt), so a match it never changes hashes as before.
    pub(crate) fn hash_into(&self, rise: u16, low: u16, h: &mut StateHasher) {
        h.u16(rise);
        h.u8(u8::from(self.closed));
        if self.seasonal(low) {
            h.u16(low);
            h.u8(u8::from(self.exposed));
            h.u32(self.snowpack);
        }
    }
}
