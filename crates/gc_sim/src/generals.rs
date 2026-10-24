//! The commander on the field (docs/37-commander.md, idea 12).
//!
//! In a Field Battle each side's general rides with one of its regiments:
//! picked [`PICK_DELAY`] after the side has its first regiment on the field,
//! from its best fighting regiment, and never replaced. He is a soldier of
//! that regiment, kept at the centre of its second rank (see [`place`]), so
//! blows reach him once the regiment is cut down to one rank or is struck
//! from the rear, and arrows once he is the last man behind; he falls when
//! his own hit points run out, or with his regiment.
//!
//! While his regiment stands, every regiment of his side within
//! [`LEAD_TILES`] of it gains morale each second (see
//! [`crate::morale`]). The first time an enemy his side can see comes within
//! [`SPEECH_TILES`] while his regiment is neither fighting nor running, he
//! speaks: those regiments gain [`SPEECH`] morale and a ceiling as much
//! higher for [`SPEECH_SECONDS`]. When he falls, his side's regiments within
//! [`PANIC_TILES`] of where he fell lose [`PANIC`] morale and the rest
//! [`PANIC_FAR`], and enemies fighting his regiment gain [`FELLED_BOOST`].
//!
//! Undead (morale 0) never rout, so instead those he leads strike
//! [`DRIVEN_PERCENT`] as hard in melee (applied in [`crate::combat`], from
//! the `led` flags of the tick before), and when he falls those within
//! [`PANIC_TILES`] fall into full disorder. The side's pick is looked at
//! once a second.
//! Conquest has no generals: heroes come there later.

use gc_fixed::{Fx, FxVec2};

use crate::combat::{Combat, Whose, placement};
use crate::morale;
use crate::regiments::{Regiments, hash_handle};
use crate::vision::Vision;
use crate::{
    MAX_PARTIES, PlayerId, RegimentHandle, Stances, StateHasher, TICKS_PER_SECOND, UnitStats,
};

/// A side's general is picked this long after its first regiment takes
/// the field.
pub const PICK_DELAY: u32 = TICKS_PER_SECOND;
/// His side's regiments this close to his (centre to centre, in tiles) are
/// led by him and hear his speech.
pub const LEAD_TILES: i32 = 20;
/// Morale points a second his leadership is worth, before the battle's pace
/// (see [`crate::combat::BATTLE_PACE_PERCENT`]) scales it like every drain.
pub const LEAD: i16 = 1;
/// An enemy his side sees this close to his regiment calls out his speech.
pub const SPEECH_TILES: i32 = 45;
/// Morale points his speech gives, and raises the ceiling by.
pub const SPEECH: i16 = 5;
/// How long the speech's higher ceiling lasts.
pub const SPEECH_SECONDS: u32 = 60;
/// His side's regiments this close to where he fell take the full panic.
pub const PANIC_TILES: i32 = 25;
/// Morale points lost near him, and by the rest of his side.
pub const PANIC: i16 = 10;
pub const PANIC_FAR: i16 = 4;
/// Morale points the enemies fighting his regiment gain when he falls.
pub const FELLED_BOOST: i16 = 5;
/// Melee damage, in percent, that undead (regiments that never rout, so
/// have no morale for him to steady) deal while he leads them: the dead are
/// driven on by the will that raised them.
pub const DRIVEN_PERCENT: u64 = 110;
/// A general's hit points, in percent of one of his men's: better armour,
/// and the best fighter in the regiment.
pub const GENERAL_HP_PERCENT: u32 = 300;

/// One side's general.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct General {
    pub party: PlayerId,
    /// The regiment he rides with (his slot in it is
    /// [`crate::RegimentView::general`]).
    pub regiment: RegimentHandle,
    /// Where his regiment stood at the end of the last tick, or where he
    /// fell.
    pub at: FxVec2,
    /// Tick he was picked.
    pub picked: u32,
    /// Tick he gave his speech, if he has.
    pub spoke: Option<u32>,
    /// Tick he fell, if he has.
    pub fell: Option<u32>,
}

/// The slot a general keeps in a regiment of `models` soldiers in ranks of
/// `files`: the centre of the second rank, or the end of the only one.
pub(crate) fn place(files: u16, models: u16) -> u8 {
    let files = files.max(1);
    let slot = if models > files {
        files + files / 2
    } else {
        models
    };
    slot.min(models.max(1) - 1) as u8
}

/// Every side's general.
#[derive(Clone, Debug)]
pub(crate) struct Generals {
    /// Parties that get no general: in a copy of the world, those whose
    /// general's regiment was left out of it.
    skip: [bool; MAX_PARTIES],
    /// The tick each party first had a regiment on the field, if it has.
    seen: [Option<u32>; MAX_PARTIES],
    /// Each party's general, once picked (he is never replaced).
    list: [Option<General>; MAX_PARTIES],
}

impl Generals {
    pub(crate) fn new() -> Generals {
        Generals {
            skip: [false; MAX_PARTIES],
            seen: [None; MAX_PARTIES],
            list: [None; MAX_PARTIES],
        }
    }

    /// The same generals for a copy of the world holding only the
    /// regiments `kept` says: a general whose regiment was left out isn't
    /// in the copy at all (he hasn't fallen), and nobody takes his place.
    /// Sides not yet given one get theirs as they would here.
    pub(crate) fn fork(&self, kept: impl Fn(RegimentHandle) -> bool) -> Generals {
        let mut g = self.clone();
        for (p, slot) in g.list.iter_mut().enumerate() {
            if slot.is_some_and(|s| s.fell.is_none() && !kept(s.regiment)) {
                *slot = None;
                g.skip[p] = true;
            }
        }
        g
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &General> {
        self.list.iter().flatten()
    }

    /// Once a tick, before the morale checks: pick the generals that are
    /// due, keep each in his place, see who has fallen, give the speeches
    /// that are due, and mark the regiments each leads now
    /// ([`Regiments::led`]).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update(
        &mut self,
        regs: &mut Regiments,
        combat: &Combat,
        units: &[UnitStats],
        stances: &Stances,
        vision: &Vision,
        tick: u32,
    ) {
        if tick.is_multiple_of(TICKS_PER_SECOND) {
            self.pick_due(regs, units, tick);
        }
        regs.led.fill(false);
        for p in 0..MAX_PARTIES {
            let Some(mut g) = self.list[p] else {
                continue;
            };
            if g.fell.is_none() {
                match regs
                    .resolve(g.regiment)
                    .filter(|&i| regs.general[i].is_some())
                {
                    Some(i) => {
                        keep_place(regs, i);
                        g.at = regs.position[i];
                    }
                    None => {
                        g.fell = Some(tick);
                        fall(regs, units, stances, &g);
                    }
                }
            }
            // He leads while he stands and his regiment isn't running.
            let leads = regs
                .resolve(g.regiment)
                .filter(|&i| g.fell.is_none() && !regs.morale_state[i].broken());
            if let Some(i) = leads {
                if g.spoke.is_none()
                    && tick.is_multiple_of(TICKS_PER_SECOND)
                    && regs.engaged[i].is_none()
                    && enemy_in_sight(regs, combat, stances, vision, i)
                {
                    g.spoke = Some(tick);
                    speak(regs, combat, stances, i, tick);
                }
                lead(regs, combat, stances, i);
            }
            self.list[p] = Some(g);
        }
    }

    /// Give each party that has had a regiment on the field for
    /// [`PICK_DELAY`] and no general yet its general. Once a second.
    fn pick_due(&mut self, regs: &mut Regiments, units: &[UnitStats], tick: u32) {
        let due = |g: &Generals, p: usize| g.list[p].is_none() && !g.skip[p];
        if !(0..MAX_PARTIES).any(|p| due(self, p)) {
            return;
        }
        for i in 0..regs.alive.len() {
            if regs.alive[i] {
                self.seen[regs.owner[i] as usize % MAX_PARTIES].get_or_insert(tick);
            }
        }
        for p in 0..MAX_PARTIES {
            if !due(self, p) || self.seen[p].is_none_or(|t| tick < t + PICK_DELAY) {
                continue;
            }
            if let Some(i) = best(regs, units, p as PlayerId) {
                let slot = place(regs.files[i], regs.models[i]);
                regs.general[i] = Some(slot);
                let hp = regs.hp_mut(i, slot);
                *hp = (u32::from(*hp) * GENERAL_HP_PERCENT / 100).min(u32::from(u16::MAX)) as u16;
                self.list[p] = Some(General {
                    party: p as PlayerId,
                    regiment: regs.handle(i),
                    at: regs.position[i],
                    picked: tick,
                    spoke: None,
                    fell: None,
                });
            }
        }
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        for &skip in &self.skip {
            h.u8(u8::from(skip));
        }
        for seen in &self.seen {
            h.u32(seen.unwrap_or(u32::MAX));
        }
        for g in &self.list {
            let Some(g) = g else {
                h.u8(0);
                continue;
            };
            h.u8(1);
            h.u8(g.party);
            hash_handle(h, Some(g.regiment));
            h.i32(g.at.x.raw());
            h.i32(g.at.y.raw());
            h.u32(g.picked);
            h.u32(g.spoke.unwrap_or(u32::MAX));
            h.u32(g.fell.unwrap_or(u32::MAX));
        }
    }
}

/// Party `p`'s best fighting regiment, the one its general rides with: one
/// that fights in melee rather than shooting if it has one, then the highest
/// morale plus melee attack plus defence, then the most soldiers, then the
/// first slot. A running regiment can't lead.
fn best(regs: &Regiments, units: &[UnitStats], p: PlayerId) -> Option<usize> {
    (0..regs.alive.len())
        .filter(|&i| {
            regs.alive[i]
                && regs.owner[i] == p
                && regs.models[i] > 0
                && !regs.morale_state[i].broken()
        })
        .max_by_key(|&i| {
            let s = &units[regs.unit[i] as usize];
            (
                s.range == Fx::ZERO,
                u32::from(s.morale) + u32::from(s.melee_attack) + u32::from(s.melee_defence),
                regs.models[i],
                core::cmp::Reverse(i),
            )
        })
}

/// Put regiment `i`'s general back in his place if he has left it (he
/// stepped up into a gap, or the frontage changed), swapping with the man
/// who stands there.
fn keep_place(regs: &mut Regiments, i: usize) {
    let Some(slot) = regs.general[i] else {
        return;
    };
    let want = place(regs.files[i], regs.models[i]);
    if slot != want && u16::from(want) < regs.models[i] {
        regs.swap_men(i, slot, want);
        regs.general[i] = Some(want);
    }
}

/// Whether an enemy regiment that `i`'s side can see is within
/// [`SPEECH_TILES`] of it.
fn enemy_in_sight(
    regs: &Regiments,
    combat: &Combat,
    stances: &Stances,
    vision: &Vision,
    i: usize,
) -> bool {
    let owner = regs.owner[i];
    let at = placement!(regs, stances);
    let mut seen = false;
    combat.each_near(
        &at,
        regs.position[i],
        Fx::from_int(SPEECH_TILES),
        Whose::Enemies(owner),
        |j| seen |= vision.sees(owner, j, regs.owner[j]),
    );
    seen
}

/// Regiment `i`'s side's regiments within [`LEAD_TILES`] of it (its own
/// included).
fn each_led(
    regs: &Regiments,
    combat: &Combat,
    stances: &Stances,
    i: usize,
    mut f: impl FnMut(usize),
) {
    let owner = regs.owner[i];
    let at = placement!(regs, stances);
    combat.each_near(
        &at,
        regs.position[i],
        Fx::from_int(LEAD_TILES),
        Whose::Friends(owner),
        |j| {
            if regs.owner[j] == owner {
                f(j);
            }
        },
    );
}

/// Regiment `i`'s general leads his side's regiments near him this tick.
fn lead(regs: &mut Regiments, combat: &Combat, stances: &Stances, i: usize) {
    let mut led = core::mem::take(&mut regs.led);
    each_led(regs, combat, stances, i, |j| led[j] = true);
    regs.led = led;
}

/// Regiment `i`'s general speaks to the regiments he leads.
/// Ticks never allocate, so those who heard him are marked first, then
/// lifted in a pass over the pool (once a battle).
fn speak(regs: &mut Regiments, combat: &Combat, stances: &Stances, i: usize, tick: u32) {
    let until = tick + SPEECH_SECONDS * TICKS_PER_SECOND;
    let mut heard = core::mem::take(&mut regs.heartened_until);
    each_led(regs, combat, stances, i, |j| heard[j] = until);
    regs.heartened_until = heard;
    let owner = regs.owner[i];
    for j in 0..regs.alive.len() {
        if regs.alive[j] && regs.owner[j] == owner && regs.heartened_until[j] == until {
            morale::shift(regs, j, SPEECH);
        }
    }
}

/// General `g` has fallen: his side panics (its undead, who can't, falter
/// into disorder near him), and the men who cut him down take heart. Once a
/// battle, so a pass over the pool.
fn fall(regs: &mut Regiments, units: &[UnitStats], stances: &Stances, g: &General) {
    for j in 0..regs.alive.len() {
        if !regs.alive[j] {
            continue;
        }
        let close = near(regs.position[j], g.at, PANIC_TILES);
        if regs.owner[j] == g.party {
            if units[regs.unit[j] as usize].morale == 0 {
                if close {
                    regs.disorder(j);
                }
            } else {
                morale::shift(regs, j, if close { -PANIC } else { -PANIC_FAR });
            }
        } else if regs.engaged[j] == Some(g.regiment) && stances.hostile(regs.owner[j], g.party) {
            morale::shift(regs, j, FELLED_BOOST);
        }
    }
}

fn near(a: FxVec2, b: FxVec2, tiles: i32) -> bool {
    let r = Fx::from_int(tiles).raw() as u64;
    (a - b).length_sq_raw() <= r * r
}

#[cfg(test)]
mod tests {
    use super::place;

    #[test]
    fn the_general_stands_in_the_second_rank_or_at_the_end_of_the_only_one() {
        assert_eq!(place(8, 32), 12);
        assert_eq!(place(8, 10), 9);
        assert_eq!(place(8, 8), 7);
        assert_eq!(place(4, 4), 3);
        assert_eq!(place(1, 1), 0);
        assert_eq!(place(5, 30), 7);
    }
}
