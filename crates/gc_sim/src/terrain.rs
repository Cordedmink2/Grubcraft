//! What the ground does in a fight (docs/05-combat.md, "Terrain"): high
//! ground and fords.
//!
//! The match hands the simulation one height per tile, taken at the tile's
//! centre, and the water line. From those:
//! - **High ground:** a regiment standing at least [`HIGH_GROUND`] above its
//!   foe deals [`DOWNHILL_MELEE_PERCENT`] melee damage, and an archer that
//!   far above its target shoots [`DOWNHILL_RANGE_PERCENT`] as far.
//! - **Fords:** a regiment whose centre is in water wades at
//!   [`WADING_SPEED_PERCENT`] of its pace, takes [`WADING_DAMAGE_PERCENT`]
//!   melee damage, and can't charge.
//!
//! Heights are integers ([`HEIGHT_UNITS`] per tile), so every machine
//! agrees. A match with no heights is flat, dry ground.

use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2};

/// Height units per tile of height.
pub const HEIGHT_UNITS: u32 = 256;
/// How much higher a regiment must stand than its foe to fight downhill:
/// half a tile.
pub const HIGH_GROUND: i32 = HEIGHT_UNITS as i32 / 2;
/// Melee damage dealt fighting downhill, percent.
pub const DOWNHILL_MELEE_PERCENT: u64 = 110;
/// Shooting range from high ground, percent.
pub const DOWNHILL_RANGE_PERCENT: i32 = 115;
/// Marching pace in water, percent.
pub const WADING_SPEED_PERCENT: i32 = 60;
/// Melee damage taken standing in water, percent.
pub const WADING_DAMAGE_PERCENT: u64 = 120;

/// The map's heights, as the match setup gives them.
#[derive(Clone, Copy, Debug)]
pub struct Ground<'a> {
    /// Ground height at each tile's centre, row by row, in
    /// [`HEIGHT_UNITS`] per tile. Empty means flat; otherwise there must be
    /// one per tile of the map, or the ground is taken as flat.
    pub heights: &'a [u16],
    /// Ground lower than this is water (deep water is also blocked in
    /// the nav grid; the rest is fords and shallows).
    pub water_level: u16,
}

impl Ground<'static> {
    /// Flat, dry ground: no terrain effects.
    pub const FLAT: Ground<'static> = Ground {
        heights: &[],
        water_level: 0,
    };
}

/// The ground during a match. Built once when the match starts.
pub struct Terrain {
    tiles: i32,
    height: Vec<u16>,
    water_level: u16,
}

impl Terrain {
    pub(crate) fn new(ground: &Ground, map_tiles: u32) -> Terrain {
        let n = map_tiles as usize;
        if n == 0 || ground.heights.len() != n * n {
            return Terrain {
                tiles: 0,
                height: Vec::new(),
                water_level: 0,
            };
        }
        Terrain {
            tiles: map_tiles as i32,
            height: ground.heights.to_vec(),
            water_level: ground.water_level,
        }
    }

    /// Whether the match has heights at all.
    pub fn is_flat(&self) -> bool {
        self.tiles == 0
    }

    /// Ground height at `p`, in [`HEIGHT_UNITS`] per tile: the height of the
    /// tile `p` is on (0 on flat ground).
    pub fn height(&self, p: FxVec2) -> i32 {
        if self.tiles == 0 {
            return 0;
        }
        let x = p.x.floor_int().clamp(0, self.tiles - 1);
        let y = p.y.floor_int().clamp(0, self.tiles - 1);
        self.height[(y * self.tiles + x) as usize] as i32
    }

    /// Whether `p` is in water.
    pub fn wet(&self, p: FxVec2) -> bool {
        self.tiles != 0 && self.height(p) < self.water_level as i32
    }

    /// Whether something at `from` fights downhill against something at `to`.
    pub fn downhill(&self, from: FxVec2, to: FxVec2) -> bool {
        self.tiles != 0 && self.height(from) - self.height(to) >= HIGH_GROUND
    }

    /// How far a regiment with `range` at `from` shoots at `to`.
    pub fn range(&self, range: Fx, from: FxVec2, to: FxVec2) -> Fx {
        if self.downhill(from, to) {
            Self::range_from_high_ground(range)
        } else {
            range
        }
    }

    /// The longest `range` can reach, from high ground.
    pub fn range_from_high_ground(range: Fx) -> Fx {
        Fx::from_raw((range.raw() as i64 * DOWNHILL_RANGE_PERCENT as i64 / 100) as i32)
    }

    /// Melee damage percent for a blow from `from` landing on `to`.
    pub fn melee_percent(&self, from: FxVec2, to: FxVec2) -> u64 {
        let mut percent = 100;
        if self.downhill(from, to) {
            percent = percent * DOWNHILL_MELEE_PERCENT / 100;
        }
        if self.wet(to) {
            percent = percent * WADING_DAMAGE_PERCENT / 100;
        }
        percent
    }

    /// `step` slowed if `p` is in water.
    pub fn wading(&self, step: Fx, p: FxVec2) -> Fx {
        if self.wet(p) {
            Fx::from_raw((step.raw() * WADING_SPEED_PERCENT / 100).max(1))
        } else {
            step
        }
    }
}
