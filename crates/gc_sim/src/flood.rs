//! Fords that flood (docs/32-changing-land.md, "Fords that flood"): long
//! rain swells the rivers. The water climbs the banks, so troops wade
//! further out from the river, and once it is up the fords can't be
//! crossed: their cells are shut on the walkable grid, and anyone still
//! in one is swept about and struggles to the nearest bank. When the rain
//! stops the water goes down again and the fords open.

use alloc::sync::Arc;
use alloc::vec::Vec;

use gc_fixed::FxVec2;

use crate::hash::StateHasher;
use crate::morale;
use crate::nav::Nav;
use crate::regiments::Regiments;
use crate::sky::WET_FULL;
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
}

impl Flood {
    /// The fords of a map `map_tiles` a side with `terrain`, walked on
    /// the grid `nav`.
    pub(crate) fn new(terrain: &Terrain, nav: &Nav, map_tiles: u32) -> Flood {
        let tiles = map_tiles as i32;
        let fords: Vec<(i32, i32)> = (0..tiles)
            .flat_map(|y| (0..tiles).map(move |x| (y, x)))
            .filter(|&(y, x)| terrain.tile_under_water(x, y) && nav.walkable_ground(x, y))
            .collect();
        let mut runs: Vec<[i32; 4]> = Vec::new();
        for &(y, x) in &fords {
            match runs.last_mut() {
                Some(run) if run[1] == y && run[0] + run[2] == x => run[2] += 1,
                _ => runs.push([x, y, 1, 1]),
            }
        }
        Flood {
            fords: fords.into(),
            runs: runs.into(),
            closed: false,
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

    /// Whether `p` is in a ford the flood has shut.
    pub fn sweeps(&self, p: FxVec2, terrain: &Terrain) -> bool {
        self.closed
            && !terrain.on_deck(p)
            && (self.fords)
                .binary_search(&(p.y.floor_int(), p.x.floor_int()))
                .is_ok()
    }

    /// One second of the rivers, with the ground `wet`: the water in
    /// `terrain` rises, holds or goes down, and with `shut` the fords shut
    /// or open with it (a rehearsal keeps them as it found them, so it
    /// never changes the walkable grid it shares). Returns whether the
    /// fords just shut (`Some(true)`) or opened again (`Some(false)`).
    pub(crate) fn second(&mut self, terrain: &mut Terrain, wet: u16, shut: bool) -> Option<bool> {
        if terrain.is_flat() || terrain.water_level() == 0 {
            return None;
        }
        let rise = terrain.rise();
        if wet >= FLOOD_WET {
            terrain.set_rise((rise + RISE_PER_SECOND).min(FLOOD_RISE));
        } else if wet < EBB_WET {
            terrain.set_rise(rise.saturating_sub(EBB_PER_SECOND));
        }
        let need = if self.closed {
            FLOOD_RISE / 2
        } else {
            FLOOD_RISE
        };
        let now = !self.fords.is_empty() && terrain.rise() >= need;
        if !shut || now == self.closed {
            return None;
        }
        self.closed = now;
        Some(now)
    }

    /// One second of the regiments caught in shut fords: they lose hit
    /// points and morale. Into `out` (cleared first), the slots of those
    /// left alive with nowhere to go but where they stand: no order, or
    /// a chase the shut ford cuts off, and not fighting. They struggle to
    /// the nearest bank (see `World::rivers`).
    pub(crate) fn sweep(&self, regs: &mut Regiments, terrain: &Terrain, out: &mut Vec<usize>) {
        out.clear();
        if !self.closed {
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

    /// Hashed with the ground while the water is up (`rise` height units
    /// above its line), so a match it never floods hashes as before.
    pub(crate) fn hash_into(&self, rise: u16, h: &mut StateHasher) {
        h.u16(rise);
        h.u8(u8::from(self.closed));
    }
}
