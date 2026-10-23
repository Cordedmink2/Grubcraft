//! Towers shoot (docs/26-fortifications.md, stage 1).
//!
//! A finished building with an attack ([`crate::structures::TowerAttack`])
//! looses a volley of its `shots` each reload at an enemy regiment its side
//! can see within its range (further downhill, as for regiments), through
//! the same volleys as regiments' arrows ([`Combat::loose`]). Its shots are
//! aimed one by one, so every one lands: how close the target stands makes
//! no odds, though a shield wall's front and trees still catch them. A
//! Choir tower needs power to shoot, and a hasted one reloads faster.
//!
//! Each reload it picks, nearest first within each:
//! 1. the regiment its owner told it to shoot ([`crate::Command::FocusTower`]),
//!    while that one is in range;
//! 2. siege engines and artillery;
//! 3. regiments attacking its side's buildings;
//! 4. anyone else.
//!
//! A tower that shoots is known to its target's side from then on, as in
//! fog of war a building is once seen.
//!
//! The quirks:
//! - **Tollgate** ([`TOLL`]): every [`TOLL_HP`] hit points its volleys take
//!   earn its owner a Grub, less upkeep as for any Grub gathered.
//! - **Bell Tower** ([`SILENCES`]): when an enemy regiment comes within
//!   [`BELL_REACH`] of it, it tolls, at most once in [`BELL_EVERY`]; for
//!   [`BELL_SILENCE`] after, its enemies can't use a power on anything
//!   within [`BELL_REACH`] of it (see [`Structures::silenced`]).

use gc_fixed::{Fx, FxVec2};

use crate::combat::{Combat, Shot};
use crate::conquest::Conquest;
use crate::events::Shooter;
use crate::regiments::{RegimentHandle, Regiments, hash_handle};
use crate::stances::Stances;
use crate::structures::{Breaker, SILENCES, StructureHandle, Structures, TOLL};
use crate::terrain::Terrain;
use crate::veins::within;
use crate::vision::Vision;
use crate::{PlayerId, StateHasher, TICKS_PER_SECOND};

/// Hit points a Tollgate's volleys take for each Grub they earn.
pub const TOLL_HP: u32 = 4;
/// How far from a Bell Tower's walls an enemy sets it tolling, and its
/// silence reaches, in tiles.
pub const BELL_REACH: i32 = 10;
/// Ticks between a Bell Tower's tolls, at the least (20 s).
pub const BELL_EVERY: u16 = 20 * TICKS_PER_SECOND as u16;
/// Ticks a toll silences enemy powers for (5 s).
pub const BELL_SILENCE: u16 = 5 * TICKS_PER_SECOND as u16;
/// Percent of a tower's shots that land.
const ACCURACY: u8 = 100;
/// Ticks a tower with nobody to shoot waits before looking again.
const LOOK_EVERY: u16 = 5;

/// Which of a tower's targets it shoots first (lower first).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    Siege,
    Besieger,
    Other,
}

impl Conquest {
    /// One tick of towers: Tollgates earn from the volleys that came down
    /// this tick, bells toll, and towers that have reloaded shoot. Played
    /// after the tick's volleys have landed.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn towers(
        &mut self,
        combat: &mut Combat,
        regs: &Regiments,
        stances: &Stances,
        terrain: &Terrain,
        vision: &Vision,
        tick: u32,
    ) {
        let s = &mut self.structures;
        for &(tower, hp) in combat.tower_hits() {
            let Some(i) = s.resolve(tower).filter(|&i| s.flags_of(i) & TOLL != 0) else {
                continue;
            };
            let total = u32::from(s.toll[i]) + hp;
            s.toll[i] = (total % TOLL_HP) as u8;
            self.economy
                .earn_grub(s.owner_of(i), total / TOLL_HP, regs, &self.units);
        }
        s.tolled.clear();
        for i in 0..s.slots() {
            let Some(attack) = s.stats_of(i).attack.filter(|_| s.stands(i)) else {
                continue;
            };
            s.silence[i] = s.silence[i].saturating_sub(1);
            if !s.finished(i) || !s.powered(i) {
                continue;
            }
            // A haste speeds its reloads, not its bell.
            if s.flags_of(i) & SILENCES != 0 {
                s.bell[i] = s.bell[i].saturating_sub(1);
                if s.bell[i] == 0 && s.enemy_near(combat, regs, stances, vision, i) {
                    s.bell[i] = BELL_EVERY;
                    s.silence[i] = BELL_SILENCE;
                    s.tolled.push(s.handle(i));
                }
            }
            let steps = s.steps(i) as u16;
            s.reload[i] = s.reload[i].saturating_sub(steps);
            if s.reload[i] > 0 {
                continue;
            }
            // A regiment told to shoot that has fallen is forgotten.
            if s.focus[i].is_some_and(|h| regs.resolve(h).is_none()) {
                s.focus[i] = None;
            }
            let Some(j) = s.tower_target(combat, regs, &self.units, stances, terrain, vision, i)
            else {
                s.reload[i] = LOOK_EVERY;
                continue;
            };
            let shot = Shot {
                shooter: Shooter::Tower(s.handle(i)),
                owner: s.owner_of(i),
                from: s.centre(i),
                shots: u16::from(attack.shots),
                damage: attack.damage,
                accuracy: ACCURACY,
            };
            if combat.loose(regs, j, shot, tick) {
                s.reload[i] = attack.reload_ticks;
                s.mark_seen(i, vision.allies_of(regs.owner[j]));
            } else {
                // Every volley is in the air: try again in a moment.
                s.reload[i] = LOOK_EVERY;
            }
        }
    }
}

/// Whether regiment slot `j` is an enemy of `owner`'s that their side can
/// see.
fn foe_in_sight(
    owner: PlayerId,
    regs: &Regiments,
    stances: &Stances,
    vision: &Vision,
    j: usize,
) -> bool {
    regs.alive[j] && stances.hostile(owner, regs.owner[j]) && vision.sees(owner, j, regs.owner[j])
}

impl Structures {
    /// The regiment tower slot `i` shoots now, if any (see the module's
    /// order).
    #[allow(clippy::too_many_arguments)]
    fn tower_target(
        &self,
        combat: &Combat,
        regs: &Regiments,
        kinds: &[crate::ConquestUnit],
        stances: &Stances,
        terrain: &Terrain,
        vision: &Vision,
        i: usize,
    ) -> Option<usize> {
        let owner = self.owner_of(i);
        let range = self.stats_of(i).attack?.range;
        let at = self.centre(i);
        let shootable = |j: usize| {
            foe_in_sight(owner, regs, stances, vision, j)
                && terrain.in_range(range, at, regs.position[j])
        };
        let focus = self.focus[i].and_then(|h| regs.resolve(h));
        if let Some(j) = focus.filter(|&j| shootable(j)) {
            return Some(j);
        }
        let reach = terrain.reach(range) + combat.max_step();
        let mut best: Option<(Priority, u64, usize)> = None;
        combat.near_enemies(at, reach, owner, |j| {
            if !shootable(j) {
                return;
            }
            let siege = kinds
                .get(usize::from(regs.unit[j]))
                .is_some_and(|k| matches!(k.breaker, Breaker::Siege | Breaker::Artillery));
            let priority = if siege {
                Priority::Siege
            } else if (self.besieging(regs, j)).is_some_and(|b| !stances.hostile(owner, b)) {
                Priority::Besieger
            } else {
                Priority::Other
            };
            let key = (priority, (regs.position[j] - at).length_sq_raw(), j);
            if best.is_none_or(|b| key < b) {
                best = Some(key);
            }
        });
        best.map(|(_, _, j)| j)
    }

    /// The owner of the building regiment slot `r` is attacking, if it is.
    fn besieging(&self, regs: &Regiments, r: usize) -> Option<PlayerId> {
        match self.job_of(regs, r)? {
            (s, crate::structures::Work::Attack) => self.owner(s),
            _ => None,
        }
    }

    /// Whether an enemy regiment building slot `i`'s side can see stands
    /// within [`BELL_REACH`] of its walls.
    fn enemy_near(
        &self,
        combat: &Combat,
        regs: &Regiments,
        stances: &Stances,
        vision: &Vision,
        i: usize,
    ) -> bool {
        let owner = self.owner_of(i);
        let foot = self.footprint(i);
        let search = Fx::from_int(BELL_REACH) + self.block(i).radius() + combat.max_step();
        let mut near = false;
        combat.near_enemies(self.centre(i), search, owner, |j| {
            near |= foe_in_sight(owner, regs, stances, vision, j)
                && within(foot, regs.position[j], BELL_REACH);
        });
        near
    }

    /// Whether a power `player` uses on something at `at` is silenced: an
    /// enemy Bell Tower within [`BELL_REACH`] of it has tolled lately.
    pub(crate) fn silenced(&self, stances: &Stances, player: PlayerId, at: FxVec2) -> bool {
        self.standing().any(|i| {
            self.silence[i] > 0
                && stances.hostile(player, self.owner_of(i))
                && within(self.footprint(i), at, BELL_REACH)
        })
    }

    /// Tower slot `i` shoots regiment `target` first while it is in range;
    /// `None` lets it choose again. The caller has checked the tower is the
    /// issuer's.
    pub(crate) fn set_focus(&mut self, i: usize, target: Option<RegimentHandle>) {
        self.focus[i] = target;
    }

    /// The regiment tower `h` was told to shoot, if any and still there.
    pub fn focus_of(&self, h: StructureHandle) -> Option<RegimentHandle> {
        self.resolve(h).and_then(|i| self.focus[i])
    }

    /// Ticks of tower `h`'s silence left: 0 when it isn't silencing.
    pub fn silence_left(&self, h: StructureHandle) -> u16 {
        self.resolve(h).map_or(0, |i| self.silence[i])
    }

    pub(crate) fn hash_tower(&self, i: usize, h: &mut StateHasher) {
        h.u16(self.reload[i]);
        hash_handle(h, self.focus[i]);
        let flags = self.flags_of(i);
        if flags & TOLL != 0 {
            h.u8(self.toll[i]);
        }
        if flags & SILENCES != 0 {
            h.u16(self.bell[i]);
            h.u16(self.silence[i]);
        }
    }
}
