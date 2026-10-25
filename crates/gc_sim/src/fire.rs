//! Woods that burn (docs/32-changing-land.md, "Woods that burn"): a
//! regiment sets the scrub or woods beside it alight, the fire spreads
//! tile by tile with the wind and dies in the rain, burns whoever stands
//! in it, and lights the night round it. A tile that burns out goes
//! through the one ground change (`World::change_ground`) as burnt: open,
//! scorched ground from then on.

use alloc::vec::Vec;

use gc_fixed::Fx;

use crate::combat::Block;
use crate::hash::StateHasher;
use crate::nav::DIRS;
use crate::regiments::Regiments;
use crate::sky::{WET_FULL, Weather, WeatherKind};
use crate::structures::footprint_centre;
use crate::terrain::{Growth, Terrain};
use crate::{TICKS_PER_SECOND, UnitStats, morale};

/// The ground is too wet to catch from here up (see [`WET_FULL`]): some
/// 25 seconds of heavy rain, which dries off in under a minute.
pub const DAMP_WET: u16 = WET_FULL / 10;
/// The most tiles that burn at once: past it a fire doesn't spread until
/// some burn out.
pub const MAX_FIRES: usize = 2048;
/// A tile must have burned this long before it spreads.
pub const SPREAD_AFTER: u16 = 2 * TICKS_PER_SECOND as u16;
/// Hit points a burning tile takes off a regiment touching it, a second.
pub const BURN_HP: u32 = 4;
/// The most burning tiles that hurt one regiment at once.
pub const MAX_BURNS: u32 = 6;
/// Morale points a regiment in fire loses a second.
pub const BURN_MORALE: i16 = 3;

/// How long a tile of `growth` burns, in ticks (0: it doesn't).
pub fn burn_ticks(growth: Growth) -> u16 {
    let seconds = match growth {
        Growth::Open => 0,
        Growth::Scrub => 10,
        Growth::Woods => 25,
        Growth::Thicket => 40,
    };
    seconds * TICKS_PER_SECOND as u16
}

/// The chance a second, in thousandths, that fire next door sets a tile
/// of `growth` alight in still, dry air.
fn catch_permille(growth: Growth) -> u32 {
    match growth {
        Growth::Open => 0,
        Growth::Scrub => 300,
        Growth::Woods => 180,
        Growth::Thicket => 120,
    }
}

/// How the wind changes the chance fire spreads one step `(dx, dy)`, in
/// percent: up to three times with it, down to a quarter against it.
fn wind_percent(wind: (i8, i8), dx: i32, dy: i32) -> u32 {
    let along = i32::from(wind.0) * dx + i32::from(wind.1) * dy;
    // A diagonal step is longer: its share of the wind is about 7/10.
    let along = if dx != 0 && dy != 0 {
        along * 7 / 10
    } else {
        along
    };
    (100 + along * 8).clamp(25, 300) as u32
}

/// A burning tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Burn {
    pub x: i32,
    pub y: i32,
    /// What grew there (and burns).
    pub growth: Growth,
    /// Ticks left before it burns out.
    pub left: u16,
    /// Ticks it burns in all, as lit.
    pub total: u16,
}

impl Burn {
    fn key(&self) -> (i32, i32) {
        (self.y, self.x)
    }
}

/// Tile `(x, y)` as a block, for touching regiments.
fn tile_block(x: i32, y: i32) -> Block {
    Block::square(footprint_centre([x, y, 1, 1]), Fx::HALF, Fx::HALF)
}

/// The fires of a match.
#[derive(Clone)]
pub(crate) struct Fires {
    seed: u64,
    /// Sorted by row, then column.
    burning: Vec<Burn>,
    /// Whether a fire was ever lit: until then the fires hash as nothing.
    ever: bool,
    /// Tiles that caught this tick, for [`crate::events::Event::Kindled`]
    /// (the world tells and clears them).
    pub(crate) kindled: Vec<(i32, i32)>,
    /// Tiles that burned out this tick, by row then column, for the ground
    /// change (the world clears them).
    pub(crate) burnt: Vec<(i32, i32)>,
}

impl Fires {
    pub(crate) fn new(seed: u64) -> Fires {
        Fires {
            seed,
            burning: Vec::new(),
            ever: false,
            kindled: Vec::new(),
            burnt: Vec::new(),
        }
    }

    /// Put out the fire on tile `(x, y)`, if it burns: the ground there
    /// changed under it (cleared by workers).
    pub(crate) fn put_out(&mut self, x: i32, y: i32) {
        if let Ok(at) = self.find(x, y) {
            self.burning.remove(at);
        }
    }

    /// The tiles burning now, by row then column.
    pub(crate) fn burning(&self) -> &[Burn] {
        &self.burning
    }

    pub(crate) fn ever(&self) -> bool {
        self.ever
    }

    fn find(&self, x: i32, y: i32) -> Result<usize, usize> {
        self.burning.binary_search_by_key(&(y, x), Burn::key)
    }

    pub(crate) fn is_burning(&self, x: i32, y: i32) -> bool {
        self.find(x, y).is_ok()
    }

    /// The burning tiles within `gap` of `block`, edge to edge: how many
    /// it touches (gap zero), and whether any is within `gap`. Looks only
    /// at the rows and columns in reach.
    pub(crate) fn near(&self, block: &Block, gap: Fx) -> (u32, bool) {
        if self.burning.is_empty() {
            return (0, false);
        }
        let r = (block.radius() + gap + Fx::ONE).ceil_int();
        let (cx, cy) = (block.centre.x.floor_int(), block.centre.y.floor_int());
        let (mut touched, mut within) = (0, false);
        for y in cy - r..=cy + r {
            let from = self.burning.partition_point(|b| b.key() < (y, cx - r));
            for b in self.burning[from..]
                .iter()
                .take_while(|b| b.key() <= (y, cx + r))
            {
                let tile = tile_block(b.x, b.y);
                if block.touches(&tile, Fx::ZERO) {
                    touched += 1;
                }
                within = within || block.touches(&tile, gap);
            }
        }
        (touched, within)
    }

    /// Set tile `(x, y)` alight if anything burnable grows there and it
    /// isn't burning yet; returns whether it caught.
    pub(crate) fn kindle(&mut self, terrain: &Terrain, x: i32, y: i32) -> bool {
        let growth = terrain.growth(x, y);
        let total = burn_ticks(growth);
        if total == 0 || self.burning.len() >= MAX_FIRES {
            return false;
        }
        let Err(at) = self.find(x, y) else {
            return false;
        };
        self.burning.insert(
            at,
            Burn {
                x,
                y,
                growth,
                left: total,
                total,
            },
        );
        self.ever = true;
        self.kindled.push((x, y));
        true
    }

    /// The burnable tile regiment `i` would fire: the nearest its centre
    /// of those its block touches or comes within a tile of, not burning
    /// yet (ties to the lower row, then column).
    pub(crate) fn tinder(
        &self,
        terrain: &Terrain,
        regs: &Regiments,
        units: &[UnitStats],
        i: usize,
    ) -> Option<(i32, i32)> {
        let b = Block::of(regs, units, i);
        let r = (b.radius() + Fx::ONE).ceil_int();
        let (cx, cy) = (b.centre.x.floor_int(), b.centre.y.floor_int());
        let mut best: Option<(u64, (i32, i32))> = None;
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                if terrain.growth(x, y) == Growth::Open || self.is_burning(x, y) {
                    continue;
                }
                let tile = tile_block(x, y);
                if !b.touches(&tile, Fx::ONE) {
                    continue;
                }
                let d = (tile.centre - b.centre).length_sq_raw();
                if best.is_none_or(|(e, _)| d < e) {
                    best = Some((d, (x, y)));
                }
            }
        }
        best.map(|(_, t)| t)
    }

    /// One tick of the fires: each burns down (faster in rain or snow),
    /// and those that burn out go to [`Fires::burnt`]; once a second they
    /// spread (at `season_percent` of the usual chance, see
    /// [`crate::sky::Season::fire_percent`]) and burn the regiments
    /// touching them.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update(
        &mut self,
        tick: u32,
        terrain: &Terrain,
        weather: Weather,
        wet: u16,
        season_percent: u32,
        regs: &mut Regiments,
        units: &[UnitStats],
    ) {
        if self.burning.is_empty() {
            return;
        }
        let second = tick.is_multiple_of(TICKS_PER_SECOND);
        // Rain or snow falling burns tiles out faster, up to twice.
        let falling = match weather.kind {
            WeatherKind::Rain | WeatherKind::Snow => u32::from(weather.strength),
            _ => 0,
        };
        let extra = if second {
            (TICKS_PER_SECOND * falling / 255) as u16
        } else {
            0
        };
        let burnt = &mut self.burnt;
        self.burning.retain_mut(|b| {
            b.left = b.left.saturating_sub(1 + extra);
            if b.left == 0 {
                burnt.push((b.x, b.y));
            }
            b.left > 0
        });
        if second {
            self.spread(tick, terrain, weather.wind, falling, wet, season_percent);
            self.burn_troops(regs, units);
        }
    }

    /// Fire next door may set each burnable tile alight.
    fn spread(
        &mut self,
        tick: u32,
        terrain: &Terrain,
        wind: (i8, i8),
        falling: u32,
        wet: u16,
        season_percent: u32,
    ) {
        let room = MAX_FIRES.saturating_sub(self.burning.len());
        if wet >= DAMP_WET || room == 0 {
            return;
        }
        // Damp ground and rain or snow falling hold it back in step.
        // So does the season (the summer's dry woods catch more).
        let dry =
            u32::from(DAMP_WET - wet) * (255 - falling.min(255)) * season_percent / (255 * 100);
        if dry == 0 {
            return;
        }
        let mut caught: Vec<Burn> = Vec::new();
        for b in &self.burning {
            if b.total - b.left < SPREAD_AFTER {
                continue;
            }
            for (dx, dy) in DIRS {
                let (x, y) = (b.x + dx, b.y + dy);
                let growth = terrain.growth(x, y);
                let base = catch_permille(growth);
                // A tile that burned out this tick is open ground already,
                // though the ground change comes after.
                let burnt = (self.burnt)
                    .binary_search_by_key(&(y, x), |&(x, y)| (y, x))
                    .is_ok();
                if base == 0 || burnt || self.is_burning(x, y) {
                    continue;
                }
                let chance = base * wind_percent(wind, dx, dy) / 100 * dry / u32::from(DAMP_WET);
                if self.roll(tick, b.x, b.y, x, y) % 1000 < chance {
                    let total = burn_ticks(growth);
                    caught.push(Burn {
                        x,
                        y,
                        growth,
                        left: total,
                        total,
                    });
                }
            }
        }
        // Each tile once, and only as many as there is room for, in tile
        // order so every peer keeps the same ones.
        caught.sort_unstable_by_key(Burn::key);
        caught.dedup_by_key(|b| b.key());
        caught.truncate(room);
        if caught.is_empty() {
            return;
        }
        self.kindled.extend(caught.iter().map(|b| (b.x, b.y)));
        self.burning.extend(caught);
        self.burning.sort_unstable_by_key(Burn::key);
    }

    /// A roll for fire spreading from one tile to another this tick: a
    /// hash of the match seed, the tick and the two tiles, so no random
    /// stream moves.
    fn roll(&self, tick: u32, x0: i32, y0: i32, x1: i32, y1: i32) -> u32 {
        let mut z = self.seed
            ^ u64::from(tick).rotate_left(17)
            ^ (u64::from(x0 as u16) | u64::from(y0 as u16) << 16) << 32
            ^ (u64::from(x1 as u16) | u64::from(y1 as u16) << 16);
        // splitmix64's finish.
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) as u32
    }

    /// Each regiment touching burning tiles takes [`BURN_HP`] for each (up
    /// to [`MAX_BURNS`]) and loses [`BURN_MORALE`].
    fn burn_troops(&self, regs: &mut Regiments, units: &[UnitStats]) {
        for i in 0..regs.alive.len() {
            if !regs.alive[i] || regs.models[i] == 0 {
                continue;
            }
            let (touched, _) = self.near(&Block::of(regs, units, i), Fx::ZERO);
            if touched > 0 {
                regs.take_damage(i, BURN_HP * touched.min(MAX_BURNS));
                morale::shift(regs, i, -BURN_MORALE);
            }
        }
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        h.u32(self.burning.len() as u32);
        for b in &self.burning {
            h.u32(b.x as u32);
            h.u32(b.y as u32);
            h.u16(b.left);
            h.u16(b.total);
            h.u8(b.growth as u8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wind_carries_fire() {
        assert_eq!(wind_percent((0, 0), 1, 0), 100);
        assert_eq!(wind_percent((24, 0), 1, 0), 292);
        assert_eq!(wind_percent((24, 0), -1, 0), 25);
        assert_eq!(wind_percent((24, 24), 1, 1), 300);
    }

    #[test]
    fn only_growth_burns() {
        assert_eq!(burn_ticks(Growth::Open), 0);
        assert!(burn_ticks(Growth::Scrub) < burn_ticks(Growth::Woods));
        assert!(burn_ticks(Growth::Woods) < burn_ticks(Growth::Thicket));
    }
}
