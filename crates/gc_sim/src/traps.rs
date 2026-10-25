//! Hidden traps (docs/26-fortifications.md, "Field works"): one a faction,
//! a tile big, built by workers like any building. Only its owner ever
//! sees one: it keeps no watch, and the enemy's eyes pass over it (see
//! [`crate::vision`]). The first enemy regiment whose block comes onto a
//! finished trap springs it, and it is gone:
//!
//! - **Spike pit** ([`Trap::Pit`], Ember): each soldier of the regiment's
//!   front rank loses [`PIT_HP`], and it falls into disorder.
//! - **Grave-hands** ([`Trap::Hands`], Concord): the regiment is held where
//!   it stands for [`HANDS_TICKS`]; it can still fight and turn.
//! - **Shard mine** ([`Trap::Mine`], Choir): the regiment is stunned for
//!   [`MINE_TICKS`]: halted, in disorder and held.
//! - **Burrow nest** ([`Trap::Nest`], Brood): a regiment of the nest's unit
//!   (Skitterlings) bursts out beside it, the owner's for [`NEST_TICKS`].
//!
//! Workers walk onto their own side's traps freely, and nothing can be
//! built over a trap.

use crate::combat::{Block, Combat, Whose, placement};
use crate::nav::Nav;
use crate::powers::raise_beside;
use crate::regiments::{RegimentHandle, Regiments};
use crate::structures::{Role, StructureHandle, Structures, Trap};
use crate::{PlayerId, Stances, TICKS_PER_SECOND, UnitStats};
use gc_fixed::Fx;

/// What each front-rank soldier of a regiment loses falling into a spike pit.
pub const PIT_HP: u32 = 20;
/// How long grave-hands hold a regiment.
pub const HANDS_TICKS: u16 = 6 * TICKS_PER_SECOND as u16;
/// How long a shard mine stuns a regiment.
pub const MINE_TICKS: u16 = 4 * TICKS_PER_SECOND as u16;
/// How long a burrow nest's regiment fights before it burrows away.
pub const NEST_TICKS: u16 = 30 * TICKS_PER_SECOND as u16;
/// How far from a trap a regiment's centre may be and still come onto it,
/// in tiles: past any regiment's half-diagonal, so the block test decides
/// (the grid search reaches this far).
const NEAR_TILES: i32 = 12;

/// A trap that went off this tick, for the events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sprung {
    /// The trap, its handle no longer live.
    pub structure: StructureHandle,
    pub tile: (i32, i32),
    pub owner: PlayerId,
    pub trap: Trap,
    /// The regiment that sprang it.
    pub on: RegimentHandle,
    /// A burrow nest's regiment, if there was room for it.
    pub raised: Option<RegimentHandle>,
}

/// One tick of traps: each finished trap an enemy regiment has come onto
/// goes off on it (see the module docs) and is gone. What went off is
/// added to `structures.sprung`.
pub(crate) fn spring(
    structures: &mut Structures,
    regs: &mut Regiments,
    units: &[UnitStats],
    nav: &mut Nav,
    combat: &Combat,
    stances: &Stances,
) {
    if structures.traps == 0 {
        return;
    }
    let mut traps = core::mem::take(&mut structures.trap_scratch);
    traps.clear();
    traps.extend(
        (structures.walked_tiles.iter())
            .map(|&(_, s)| s as usize)
            .filter(|&s| structures.stats_of(s).role == Role::Trap && structures.finished(s)),
    );
    let reach = Fx::from_int(NEAR_TILES);
    for &s in &traps {
        let owner = structures.owner_of(s);
        let block = structures.block(s);
        // The lowest slot onto it, whatever order the grid gives them in.
        let mut onto = None;
        combat.each_near(
            &placement!(regs, stances),
            block.centre,
            reach,
            Whose::Enemies(owner),
            |j| {
                if !regs.on_wall[j]
                    && onto.is_none_or(|k| j < k)
                    && block.touches(&Block::of(regs, units, j), Fx::ZERO)
                {
                    onto = Some(j);
                }
            },
        );
        let Some(j) = onto else {
            continue;
        };
        let trap = structures.stats_of(s).trap;
        let on = regs.handle(j);
        let mut raised = None;
        match trap {
            Trap::None => {}
            Trap::Pit => regs.onto_points(j, PIT_HP),
            Trap::Hands => regs.trapped_left[j] = regs.trapped_left[j].max(HANDS_TICKS),
            Trap::Mine => {
                regs.halt(j);
                regs.disorder(j);
                regs.trapped_left[j] = regs.trapped_left[j].max(MINE_TICKS);
            }
            Trap::Nest(unit) => {
                raised = raise_beside(structures, regs, nav, units, owner, unit, s, NEST_TICKS);
            }
        }
        let [x, y, _, _] = structures.footprint(s);
        structures.sprung.push(Sprung {
            structure: structures.handle(s),
            tile: (x, y),
            owner,
            trap,
            on,
            raised,
        });
        structures.remove(s, nav, regs);
    }
    structures.trap_scratch = traps;
}
