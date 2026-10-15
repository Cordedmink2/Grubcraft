//! Champions before battle (docs/05-combat.md, "Champions before battle";
//! Mario's pick MB-4).
//!
//! In a match that allows it ([`crate::MatchSetup::champions`]), the first
//! time two hostile regiments that see each other come within
//! [`CALL_TILES`], before either side has begun fighting, there is a
//! [`CHANCE_PERCENT`] chance that each side sends a champion out between
//! the lines. Until heroes exist, a side's
//! champion is the best man of its best fighting regiment near the front
//! (melee attack plus defence) that isn't hiding in cover. The two run to
//! the point halfway between their regiments and trade blows, one swing
//! each in turn: a blow lands with the simulation's chance to hit for
//! their regiments' attack and defence, and a man it lands on reels and
//! loses his next swing. The [`WOUNDS`]th blow to land kills.
//!
//! The whole fight is rolled when the challenge is called, from the match's
//! events stream, so every machine and every replay sees the same duel and
//! the presentation can play it out blow by blow. Nothing changes until the
//! last blow lands: then every regiment on the winner's side gains
//! [`HEART`] morale points, and the side keeps them as a higher ceiling for
//! the rest of the battle, while every regiment on the loser's side loses
//! as many, from its morale and its ceiling. The loser's regiment loses
//! him. If neither falls within [`MAX_BEATS`] swings they part and nothing
//! changes. If the battle starts around them first (a regiment of either
//! side fights in melee or loses soldiers), or a champion's regiment breaks
//! or is destroyed, the duel is abandoned, also without effect. The armies
//! are never held back: a challenge is a moment in the battle, not a pause
//! in it.

use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2, Pcg32};

use crate::combat::hit_percent;
use crate::morale::{self, SCALE};
use crate::regiments::{Regiments, hash_handle};
use crate::vision::Vision;
use crate::{PlayerId, RegimentHandle, Stances, StateHasher, TICKS_PER_SECOND, UnitStats};

/// Hostile regiments this close (centre to centre, in tiles) call out
/// champions, if they are going to: far enough out that the duel is
/// usually over before the lines meet.
pub const CALL_TILES: i32 = 60;
/// Percent of matches that allow champions in which they come out.
pub const CHANCE_PERCENT: u32 = 50;
/// A side's champion comes from a regiment within this many tiles of its
/// regiment nearest the enemy.
const PICK_TILES: i32 = 25;
/// Champions run out at this many tiles per second, and take at least
/// [`MIN_WALK_TICKS`] and at most [`MAX_WALK_TICKS`] to meet.
const PACE: Fx = Fx::from_int(5);
const MIN_WALK_TICKS: u32 = 2 * TICKS_PER_SECOND;
const MAX_WALK_TICKS: u32 = 12 * TICKS_PER_SECOND;
/// Ticks per beat: one champion swings each beat, in turn.
pub const BEAT_TICKS: u32 = 12;
/// A swing's blow lands this many ticks into its beat.
pub const BLOW_TICKS: u32 = 6;
/// Blows that must land to kill a champion.
pub const WOUNDS: u8 = 3;
/// Beats they fight before parting (at most 32: see [`Challenge::landed`]).
pub const MAX_BEATS: u8 = 30;
/// Morale points the winner's side gains and the loser's loses.
pub const HEART: i16 = 6;

/// Two champions' duel between the lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Challenge {
    /// The regiments the champions step out of, for their two sides.
    pub champions: [RegimentHandle; 2],
    /// The parties those regiments belong to.
    pub sides: [PlayerId; 2],
    /// Where they fight: halfway between their regiments when called.
    pub spot: FxVec2,
    /// Tick the challenge was called.
    pub called: u32,
    /// Tick they meet; the first beat starts then.
    pub meet: u32,
    /// Beats fought. Champion `beat % 2` swings on each beat, unless the
    /// blow of the beat before landed on him.
    pub beats: u8,
    /// Bit `b` is set when the swing of beat `b` landed.
    pub landed: u32,
    /// The champion (0 or 1) whose last blow kills the other, or `None` when
    /// they part with nobody down.
    pub winner: Option<u8>,
    /// Tick the duel was cut short, if it was.
    pub abandoned: Option<u32>,
}

/// How a duel stands at a given tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Fighting,
    /// A champion's regiment was drawn into the battle, broke or was
    /// destroyed first.
    CutShort,
    /// Champion `winner` (0 or 1) killed the other.
    Killed {
        winner: u8,
    },
    /// Nobody fell and they went back to their lines.
    Parted,
}

/// Whether the man who swings on `beat` is still reeling from a blow that
/// landed on the beat before, and loses his swing.
fn reeling(landed: u32, beat: u8) -> bool {
    beat > 0 && landed & (1 << (beat - 1)) != 0
}

impl Challenge {
    /// Whether champion `beat % 2` swings on `beat`: he does unless the blow
    /// of the beat before landed on him.
    pub fn swings(&self, beat: u8) -> bool {
        beat < self.beats && !reeling(self.landed, beat)
    }

    /// Whether the swing of `beat` landed.
    pub fn lands(&self, beat: u8) -> bool {
        beat < self.beats && self.landed & (1 << beat) != 0
    }

    /// Tick the blow of `beat` lands (or would have).
    pub fn blow_tick(&self, beat: u8) -> u32 {
        self.meet + u32::from(beat) * BEAT_TICKS + BLOW_TICKS
    }

    /// Tick the duel is decided: the killing blow lands, or the last beat is
    /// over and they part.
    pub fn ends(&self) -> u32 {
        match self.winner {
            Some(_) => self.blow_tick(self.beats - 1),
            None => self.meet + u32::from(self.beats) * BEAT_TICKS,
        }
    }

    /// Tick the duel ended, cut short or not.
    pub fn decided(&self) -> u32 {
        self.abandoned.unwrap_or(self.ends())
    }

    /// How the duel stands once the world has run `ticks` ticks
    /// ([`crate::World::tick_count`]): the tick it ends on decides it.
    pub fn outcome(&self, ticks: u32) -> Outcome {
        match (self.abandoned, self.winner) {
            (Some(_), _) => Outcome::CutShort,
            _ if ticks <= self.ends() => Outcome::Fighting,
            (None, Some(winner)) => Outcome::Killed { winner },
            (None, None) => Outcome::Parted,
        }
    }
}

/// The champions of a match that allows them, and the one challenge it
/// may have.
pub(crate) struct Champions {
    /// The chance has been rolled (once a battle), whether or not they came
    /// out.
    rolled: bool,
    rng: Pcg32,
    challenge: Option<Challenge>,
    /// Scratch for [`Champions::look`]: regiments near an enemy's ground,
    /// allocated once.
    near: Vec<u16>,
}

impl Champions {
    pub(crate) fn new(regiments: u32, rng: Pcg32) -> Champions {
        Champions {
            rolled: false,
            rng,
            challenge: None,
            near: Vec::with_capacity(regiments as usize),
        }
    }

    pub(crate) fn challenge(&self) -> Option<&Challenge> {
        self.challenge.as_ref()
    }

    /// Look for armies drawing near once a second until the chance is
    /// rolled; then follow the duel and settle it. Runs at the end of the
    /// tick, after morale.
    pub(crate) fn update(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        stances: &Stances,
        vision: &Vision,
        tick: u32,
    ) {
        if !self.rolled {
            if tick.is_multiple_of(TICKS_PER_SECOND) {
                self.look(regs, units, stances, vision, tick);
            }
            return;
        }
        let Some(c) = &mut self.challenge else {
            return;
        };
        if c.abandoned.is_some() || tick > c.ends() {
            return;
        }
        let standing = |h: RegimentHandle| {
            regs.resolve(h)
                .is_some_and(|i| !regs.morale_state[i].broken())
        };
        if !c.champions.iter().all(|&h| standing(h)) || battle_begun(regs, stances, c.sides) {
            c.abandoned = Some(tick);
        } else if tick == c.ends()
            && let Some(w) = c.winner
        {
            let (won, lost) = (c.champions[w as usize], c.champions[1 - w as usize]);
            settle(regs, stances, won, lost);
        }
    }

    /// Once two hostile regiments that see each other are within
    /// [`CALL_TILES`] and both sides have a champion to send, roll whether
    /// they come out, and if so roll their duel. If the battle has already
    /// begun, there will be none.
    fn look(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        stances: &Stances,
        vision: &Vision,
        tick: u32,
    ) {
        let Some((a, b)) = self.nearest_pair(regs, stances, vision) else {
            return;
        };
        if battle_begun(regs, stances, [regs.owner[a], regs.owner[b]]) {
            self.rolled = true;
            return;
        }
        let (Some(x), Some(y)) = (
            pick(regs, units, stances, vision, a),
            pick(regs, units, stances, vision, b),
        ) else {
            return;
        };
        self.rolled = true;
        if self.rng.below(100) >= CHANCE_PERCENT {
            return;
        }
        let (px, py) = (regs.position[x], regs.position[y]);
        let spot = FxVec2::new(
            Fx::from_raw(px.x.raw().midpoint(py.x.raw())),
            Fx::from_raw(px.y.raw().midpoint(py.y.raw())),
        );
        // Ticks for the champion further from the spot to run there.
        let far = i64::from((px - spot).length().raw());
        let walk = (far * i64::from(TICKS_PER_SECOND) / i64::from(PACE.raw())) as u32;
        let stats = |i: usize| &units[regs.unit[i] as usize];
        let fighters = [stats(x), stats(y)];
        let (beats, landed, winner) = duel(fighters, &mut self.rng);
        self.challenge = Some(Challenge {
            champions: [regs.handle(x), regs.handle(y)],
            sides: [regs.owner[x], regs.owner[y]],
            spot,
            called: tick,
            meet: tick + walk.clamp(MIN_WALK_TICKS, MAX_WALK_TICKS),
            beats,
            landed,
            winner,
            abandoned: None,
        });
    }

    /// The closest two hostile regiments, neither running, that see each
    /// other within [`CALL_TILES`]. Only regiments that close to an enemy
    /// party's ground (the box around its regiments) are paired up, so
    /// armies still far apart cost one pass over the regiments.
    fn nearest_pair(
        &mut self,
        regs: &Regiments,
        stances: &Stances,
        vision: &Vision,
    ) -> Option<(usize, usize)> {
        let reach = i64::from(Fx::from_int(CALL_TILES).raw());
        let ready = |i: usize| regs.alive[i] && !regs.morale_state[i].broken();
        // Each party's ground, (least x, least y, most x, most y) raw, and
        // the parties with regiments ready.
        let mut ground = [(i32::MAX, i32::MAX, i32::MIN, i32::MIN); 256];
        let (mut parties, mut count) = ([0 as PlayerId; 256], 0);
        for i in (0..regs.alive.len()).filter(|&i| ready(i)) {
            let (p, g) = (regs.position[i], &mut ground[regs.owner[i] as usize]);
            if g.0 == i32::MAX {
                parties[count] = regs.owner[i];
                count += 1;
            }
            *g = (
                g.0.min(p.x.raw()),
                g.1.min(p.y.raw()),
                g.2.max(p.x.raw()),
                g.3.max(p.y.raw()),
            );
        }
        let near_enemy = |i: usize| {
            let (x, y) = (regs.position[i].x.raw(), regs.position[i].y.raw());
            parties[..count].iter().any(|&o| {
                let g = ground[o as usize];
                let dx = i64::from((g.0 - x).max(x - g.2).max(0));
                let dy = i64::from((g.1 - y).max(y - g.3).max(0));
                stances.hostile(regs.owner[i], o) && dx * dx + dy * dy <= reach * reach
            })
        };
        self.near.clear();
        self.near.extend(
            (0..regs.alive.len())
                .filter(|&i| ready(i) && near_enemy(i))
                .map(|i| i as u16),
        );
        let reach = (reach * reach) as u64;
        let mut nearest: Option<(u64, usize, usize)> = None;
        for (k, &a) in self.near.iter().enumerate() {
            let a = a as usize;
            for &b in &self.near[k + 1..] {
                let b = b as usize;
                let d = (regs.position[a] - regs.position[b]).length_sq_raw();
                if d > reach
                    || nearest.is_some_and(|(best, _, _)| d >= best)
                    || !stances.hostile(regs.owner[a], regs.owner[b])
                    || !vision.sees(regs.owner[a], b, regs.owner[b])
                    || !vision.sees(regs.owner[b], a, regs.owner[a])
                {
                    continue;
                }
                nearest = Some((d, a, b));
            }
        }
        nearest.map(|(_, a, b)| (a, b))
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        h.u8(u8::from(self.rolled));
        let (state, inc) = self.rng.raw_state();
        h.u64(state);
        h.u64(inc);
        match &self.challenge {
            Some(c) => {
                h.u8(1);
                for &r in &c.champions {
                    hash_handle(h, Some(r));
                }
                h.u8(c.sides[0]);
                h.u8(c.sides[1]);
                h.i32(c.spot.x.raw());
                h.i32(c.spot.y.raw());
                h.u32(c.called);
                h.u32(c.meet);
                h.u8(c.beats);
                h.u32(c.landed);
                h.u8(c.winner.unwrap_or(u8::MAX));
                h.u32(c.abandoned.unwrap_or(u32::MAX));
            }
            None => h.u8(0),
        }
    }
}

/// The regiment that sends out the champion for the side of regiment `a`:
/// the best fighter (melee attack plus defence, then the nearest to `a`)
/// among `a` and its allies within [`PICK_TILES`] that are ready to fight,
/// can spare a man, fight in melee at all, and are not hiding (a champion
/// would give them away).
fn pick(
    regs: &Regiments,
    units: &[UnitStats],
    stances: &Stances,
    vision: &Vision,
    a: usize,
) -> Option<usize> {
    let near = Fx::from_int(PICK_TILES).raw() as u64;
    let mut best: Option<(i32, u64, usize)> = None;
    for i in 0..regs.alive.len() {
        let stats = &units[regs.unit[i] as usize];
        if !regs.alive[i]
            || stances.hostile(regs.owner[a], regs.owner[i])
            || regs.morale_state[i].broken()
            || regs.engaged[i].is_some()
            || regs.models[i] < 2
            || stats.melee_attack == 0
            || vision.hidden(i, regs.owner[i])
        {
            continue;
        }
        let d = (regs.position[i] - regs.position[a]).length_sq_raw();
        if d > near * near {
            continue;
        }
        let score = i32::from(stats.melee_attack) + i32::from(stats.melee_defence);
        if best.is_none_or(|(s, bd, _)| score > s || score == s && d < bd) {
            best = Some((score, d, i));
        }
    }
    best.map(|(_, _, i)| i)
}

/// Whether the battle has begun for either of `sides`: a regiment of theirs
/// or their allies is fighting in melee, or has lost soldiers lately (since
/// its last morale check).
fn battle_begun(regs: &Regiments, stances: &Stances, sides: [PlayerId; 2]) -> bool {
    (0..regs.alive.len()).any(|i| {
        regs.alive[i]
            && (regs.engaged[i].is_some() || regs.losses[i] > 0)
            && sides.iter().any(|&s| !stances.hostile(s, regs.owner[i]))
    })
}

/// Roll a duel between champions of `fighters`: (beats fought, the beats
/// whose blows landed, the winner if anyone fell).
fn duel(fighters: [&UnitStats; 2], rng: &mut Pcg32) -> (u8, u32, Option<u8>) {
    let mut wounds = [0u8; 2];
    let mut landed = 0u32;
    for beat in 0..MAX_BEATS {
        let s = (beat % 2) as usize;
        if reeling(landed, beat) {
            continue;
        }
        let (me, them) = (fighters[s], fighters[1 - s]);
        // He stands alone: no formation helps his guard.
        let chance = hit_percent(i32::from(me.melee_attack), i32::from(them.melee_defence));
        if rng.below(100) < chance as u32 {
            landed |= 1 << beat;
            wounds[1 - s] += 1;
            if wounds[1 - s] == WOUNDS {
                return (beat + 1, landed, Some(s as u8));
            }
        }
    }
    (MAX_BEATS, landed, None)
}

/// The champion of regiment `won` has killed the champion of `lost`: his
/// side takes heart and theirs loses it, and `lost` loses its man.
fn settle(regs: &mut Regiments, stances: &Stances, won: RegimentHandle, lost: RegimentHandle) {
    let (Some(w), Some(l)) = (regs.resolve(won), regs.resolve(lost)) else {
        return;
    };
    let (winners, losers) = (regs.owner[w], regs.owner[l]);
    let heart = |party: PlayerId| {
        if !stances.hostile(winners, party) {
            HEART
        } else if !stances.hostile(losers, party) {
            -HEART
        } else {
            0
        }
    };
    // Each side's ceiling moves for the rest of the battle, for regiments
    // it raises later too; its regiments' morale moves now.
    for party in 0..=PlayerId::MAX {
        let bonus = &mut regs.party_morale[party as usize];
        *bonus = bonus.saturating_add(heart(party) * SCALE);
    }
    for i in 0..regs.alive.len() {
        if regs.alive[i] {
            morale::shift(regs, i, heart(regs.owner[i]));
        }
    }
    let last = regs.models[l] as usize - 1;
    let hp = regs.hp[l * crate::formation::MAX_MODELS as usize + last];
    regs.take_damage(l, u32::from(hp));
    if regs.models[l] == 0 {
        regs.despawn(lost);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fighter(attack: u8, defence: u8) -> UnitStats {
        UnitStats {
            melee_attack: attack,
            melee_defence: defence,
            ..crate::scenario::DRILL_UNITS[0]
        }
    }

    #[test]
    fn a_reeling_man_loses_his_next_swing_and_the_third_blow_kills() {
        let (a, b) = (fighter(60, 30), fighter(30, 30));
        for seed in 0..200 {
            let mut rng = Pcg32::new(seed, 3);
            let (beats, landed, winner) = duel([&a, &b], &mut rng);
            let c = Challenge {
                champions: [RegimentHandle {
                    index: 0,
                    generation: 0,
                }; 2],
                spot: FxVec2::ZERO,
                sides: [0, 1],
                called: 0,
                meet: 0,
                beats,
                landed,
                winner,
                abandoned: None,
            };
            let mut wounds = [0; 2];
            for beat in 0..beats {
                if c.lands(beat) {
                    assert!(c.swings(beat), "seed {seed}: a lost swing landed");
                    wounds[1 - (beat % 2) as usize] += 1;
                }
                if beat > 0 && c.lands(beat - 1) {
                    assert!(!c.swings(beat), "seed {seed}: a reeling man swung");
                }
            }
            match winner {
                Some(w) => {
                    assert_eq!(wounds[1 - w as usize], WOUNDS, "seed {seed}");
                    assert!(c.lands(beats - 1) && (beats - 1) % 2 == w);
                }
                None => {
                    assert_eq!(beats, MAX_BEATS);
                    assert!(wounds.iter().all(|&w| w < WOUNDS));
                }
            }
        }
    }

    #[test]
    fn the_better_fighter_usually_wins() {
        let (strong, weak) = (fighter(50, 50), fighter(25, 25));
        let mut wins = [0; 3];
        for seed in 0..1000 {
            let mut rng = Pcg32::new(seed, 3);
            // Half the time the weak man swings first.
            let (first, second) = if seed % 2 == 0 {
                (&strong, &weak)
            } else {
                (&weak, &strong)
            };
            let winner = duel([first, second], &mut rng).2;
            let strong_won = winner.map(|w| usize::from(u64::from(w) == seed % 2));
            wins[strong_won.unwrap_or(2)] += 1;
        }
        // wins[1]: the strong man; wins[0]: the weak; wins[2]: they parted.
        assert!(wins[1] > 850, "{wins:?}");
        assert!(wins[0] > 10, "the weak man never won: {wins:?}");
    }
}
