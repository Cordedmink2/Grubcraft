//! The faction twists of the top wall level (docs/26-fortifications.md,
//! stage 4c): what each faction's third wall does besides stand there.
//!
//! - Ember Castle Wall, [`WallTwist::Stones`]: a regiment striking it in
//!   melee has a stone dropped on it every [`STONE_TICKS`]
//!   ([`STONE_HP`] hit points), and its own archers standing on it shoot
//!   through loopholes, reloading [`LOOPHOLE_PERCENT`] faster.
//! - Concord Mausoleum Wall, [`WallTwist::Mends`]: each soldier of any side
//!   who dies within [`MEND_TILES`] of it mends its nearest hurt tile by
//!   [`MEND_HP`] hit points.
//! - Choir Singing Wall, [`WallTwist::Sings`]: a regiment striking it is
//!   slowed for [`SING_TICKS`]: half as many blows on buildings, and it
//!   walks at [`SUNG_PACE_PERCENT`] of its pace.
//! - Brood Thorn Bulwark, [`WallTwist::Thorns`]: each melee blow on it
//!   costs the striking regiment [`THORN_HP`] hit points.

use crate::TICKS_PER_SECOND;
use crate::regiments::{RegimentHandle, Regiments};
use crate::structures::{StructureHandle, Structures, WallTwist, nearest_point};
use crate::veins::within;

/// How often a stone falls on a regiment striking a Castle Wall.
pub const STONE_TICKS: u16 = 3 * TICKS_PER_SECOND as u16;
/// What one stone takes off the regiment it falls on.
pub const STONE_HP: u32 = 6;
/// How much faster archers on a Castle Wall reload, in percent.
pub const LOOPHOLE_PERCENT: u16 = 25;
/// How far from a Mausoleum Wall a death mends it, in tiles (from the
/// middle of the regiment that lost the soldier, so its front ranks are
/// at the foot).
pub const MEND_TILES: i32 = 4;
/// What each death mends of a Mausoleum Wall's tile.
pub const MEND_HP: u32 = 15;
/// How long a Singing Wall slows a regiment that strikes it.
pub const SING_TICKS: u16 = 3 * TICKS_PER_SECOND as u16;
/// How fast a regiment slowed by a Singing Wall walks, in percent of its
/// pace.
pub const SUNG_PACE_PERCENT: i64 = 70;
/// What each melee blow on a Thorn Bulwark takes off the striker.
pub const THORN_HP: u32 = 3;

impl Structures {
    /// Regiment slot `r` struck a blow on building slot `s` (a melee blow
    /// unless `shot`): the wall's twist answers it.
    pub(crate) fn twist_blow(&mut self, regs: &mut Regiments, r: usize, s: usize, shot: bool) {
        let twist = self.stats_of(s).twist;
        let hurt = match twist {
            WallTwist::Stones if !shot && regs.stoned_left[r] == 0 => {
                regs.stoned_left[r] = STONE_TICKS;
                STONE_HP
            }
            WallTwist::Thorns if !shot => THORN_HP,
            // A Singing Wall without power is silent.
            WallTwist::Sings if self.powered(s) => {
                regs.sung_left[r] = SING_TICKS;
                0
            }
            _ => return,
        };
        self.twisted
            .push((self.handle(s), twist, Some(regs.handle(r))));
        if hurt > 0 {
            regs.take_damage(r, hurt);
            if regs.models[r] == 0 {
                regs.despawn(regs.handle(r));
            }
        }
    }

    /// The dead of the tick mend the Mausoleum Walls near where they fell:
    /// each regiment's losses its nearest hurt such tile within
    /// [`MEND_TILES`].
    pub(crate) fn mend_by_the_dead(&mut self, regs: &Regiments) {
        // A world with no such wall type (a Field Battle's, a test's) has
        // nothing to look for.
        if !self.buildings().iter().any(|b| b.twist == WallTwist::Mends) {
            return;
        }
        self.mending.clear();
        for i in 0..regs.deaths.len() {
            let dead = regs.deaths[i];
            if dead == 0 {
                continue;
            }
            if self.mending.is_empty() {
                // The hurt Mausoleum Wall tiles, found at the first death.
                let mut mending = core::mem::take(&mut self.mending);
                mending.extend((self.standing()).filter_map(|s| {
                    let mends = self.stats_of(s).twist == WallTwist::Mends;
                    (mends && self.finished(s) && self.hurt(s)).then_some(s as u32)
                }));
                self.mending = mending;
                if self.mending.is_empty() {
                    return;
                }
            }
            let at = regs.position[i];
            let nearest = (self.mending.iter().map(|&s| s as usize))
                .filter(|&s| self.hurt(s) && within(self.footprint(s), at, MEND_TILES))
                .min_by_key(|&s| {
                    let gap = (nearest_point(self.footprint(s), at) - at).length_sq_raw();
                    (gap, s)
                });
            if let Some(s) = nearest {
                self.mend(s, u32::from(dead) * MEND_HP);
                self.twisted.push((self.handle(s), WallTwist::Mends, None));
            }
        }
    }

    /// Whether building slot `i` is a Castle Wall (or any wall with
    /// loopholes) for the archers standing on it.
    pub(crate) fn loopholes(&self, i: usize) -> bool {
        self.stats_of(i).twist == WallTwist::Stones
    }
}

/// Whether a regiment slowed by a Singing Wall, `sung_left` ticks of it to
/// go, sits this tick out of its work on buildings: every other one.
pub(crate) fn sung_rest(sung_left: u16) -> bool {
    sung_left % 2 == 1
}

/// A wall twist that answered in the last tick: on a regiment (a stone, a
/// thorn, a song), or a Mausoleum Wall mended (no regiment).
pub type Twisted = (StructureHandle, WallTwist, Option<RegimentHandle>);
