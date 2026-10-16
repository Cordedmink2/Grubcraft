//! What the ground does in a fight (docs/05-combat.md, "Terrain"): high
//! ground and fords, and the woods and tall grass regiments hide in
//! (docs/20-ambushes.md; the hiding itself is [`crate::vision`]).
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
//! - **Cover:** each tile is open, tall grass or wood ([`Cover`]).
//! - **Woods:** a regiment whose centre is in a wood marches at
//!   [`WOODS_SPEED_PERCENT`] of its pace ([`WOODS_HEAVY_SPEED_PERCENT`] for
//!   heavy troops: cavalry and monsters), and the trees catch
//!   [`TREES_CATCH_PERCENT`] of the arrows shot out of a wood, and as many
//!   again of those shot at a regiment standing in one.
//!
//! Heights are integers ([`HEIGHT_UNITS`] per tile), so every machine
//! agrees. A match with no heights is flat, dry ground; one with no cover
//! has nowhere to hide.

use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2};

use crate::Mass;

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
/// Marching pace in woods, percent, for soldiers on foot and swarms.
pub const WOODS_SPEED_PERCENT: i32 = 85;
/// Marching pace in woods, percent, for heavy troops (cavalry and
/// monsters), who have to pick their way between the trunks.
pub const WOODS_HEAVY_SPEED_PERCENT: i32 = 70;
/// Arrows the trees catch, percent, at each end of a volley's flight that
/// lies in a wood (both ends: about half the volley).
pub const TREES_CATCH_PERCENT: u64 = 30;

/// What covers a tile, for hiding (see [`crate::vision`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Cover {
    Open = 0,
    /// Hides regiments standing still.
    Grass = 1,
    /// Hides regiments standing or marching.
    Wood = 2,
}

impl Cover {
    /// The cover a map byte stands for; anything unknown is open ground.
    pub fn from_u8(v: u8) -> Cover {
        match v {
            1 => Cover::Grass,
            2 => Cover::Wood,
            _ => Cover::Open,
        }
    }
}

/// The map's heights, water and cover, as the match setup gives them.
#[derive(Clone, Copy, Debug)]
pub struct Ground<'a> {
    /// Ground height at each tile's centre, row by row, in
    /// [`HEIGHT_UNITS`] per tile. Empty means flat; otherwise there must be
    /// one per tile of the map, or the ground is taken as flat.
    pub heights: &'a [u16],
    /// Ground lower than this is water (deep water is also blocked in
    /// the nav grid; the rest is fords and shallows).
    pub water_level: u16,
    /// What covers each tile, row by row, as [`Cover`] bytes. Empty means
    /// open ground everywhere; otherwise there must be one per tile of the
    /// map, or the map is taken as having no cover.
    pub cover: &'a [u8],
}

impl Ground<'static> {
    /// Flat, dry, open ground: no terrain effects.
    pub const FLAT: Ground<'static> = Ground {
        heights: &[],
        water_level: 0,
        cover: &[],
    };
}

/// The ground during a match. Built once when the match starts.
pub struct Terrain {
    tiles: i32,
    height: Vec<u16>,
    water_level: u16,
    /// Map edge in tiles when the map has cover, else 0.
    cover_tiles: i32,
    cover: Vec<u8>,
}

impl Terrain {
    pub(crate) fn new(ground: &Ground, map_tiles: u32) -> Terrain {
        let n = map_tiles as usize;
        let (cover_tiles, cover) = if n > 0 && ground.cover.len() == n * n {
            (map_tiles as i32, ground.cover.to_vec())
        } else {
            (0, Vec::new())
        };
        if n == 0 || ground.heights.len() != n * n {
            return Terrain {
                tiles: 0,
                height: Vec::new(),
                water_level: 0,
                cover_tiles,
                cover,
            };
        }
        Terrain {
            tiles: map_tiles as i32,
            height: ground.heights.to_vec(),
            water_level: ground.water_level,
            cover_tiles,
            cover,
        }
    }

    /// Whether the map has any cover to hide in.
    pub fn has_cover(&self) -> bool {
        self.cover_tiles != 0
    }

    /// What covers the tile `p` is on (open off the map or with no cover).
    pub fn cover(&self, p: FxVec2) -> Cover {
        if self.cover_tiles == 0 {
            return Cover::Open;
        }
        let (x, y) = (p.x.floor_int(), p.y.floor_int());
        if x < 0 || y < 0 || x >= self.cover_tiles || y >= self.cover_tiles {
            return Cover::Open;
        }
        Cover::from_u8(self.cover[(y * self.cover_tiles + x) as usize])
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
        i32::from(self.height[(y * self.tiles + x) as usize])
    }

    /// Whether `p` is in water.
    pub fn wet(&self, p: FxVec2) -> bool {
        self.tiles != 0 && self.height(p) < i32::from(self.water_level)
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
        Fx::from_raw((i64::from(range.raw()) * i64::from(DOWNHILL_RANGE_PERCENT) / 100) as i32)
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

    /// `step` slowed by the ground at `p` for troops of `mass`: water,
    /// then woods.
    pub fn going(&self, step: Fx, p: FxVec2, mass: Mass) -> Fx {
        let wading = if self.wet(p) {
            WADING_SPEED_PERCENT
        } else {
            100
        };
        let woods = match (self.cover(p), mass) {
            (Cover::Wood, Mass::Heavy) => WOODS_HEAVY_SPEED_PERCENT,
            (Cover::Wood, _) => WOODS_SPEED_PERCENT,
            _ => 100,
        };
        if wading * woods == 100 * 100 {
            step
        } else {
            let raw = i64::from(step.raw()) * i64::from(wading * woods) / (100 * 100);
            Fx::from_raw((raw as i32).max(1))
        }
    }

    /// Percent of a volley's arrows that get through the trees, loosed from
    /// `from` at `aim`.
    pub fn through_trees(&self, from: FxVec2, aim: FxVec2) -> u64 {
        let through = |p| {
            if self.cover(p) == Cover::Wood {
                100 - TREES_CATCH_PERCENT
            } else {
                100
            }
        };
        through(from) * through(aim) / 100
    }
}
