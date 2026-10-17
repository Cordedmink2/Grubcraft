//! Faction powers (docs/17-unit-rosters.md, the faction meters' abilities):
//! what a player spends their faction meter on.
//!
//! A power costs points of its meter, needs its caster's tier, and can't be
//! used again until its cooldown has passed. Each player spends only their
//! own faction's meter ([`crate::ConquestSetup::meters`]): the Choir's
//! Resonance (see [`crate::chords`]) or the Clans' Honor (see
//! [`crate::honor`]).
//!
//! - **Haste** (Overclock): one of the caster's buildings or regiments works
//!   [`PowerEffect::Haste`]'s percent faster for the power's duration: a
//!   building trains, upgrades, is built and tunes faster, and a regiment
//!   reloads, strikes and builds faster. Used again on the same target, the
//!   new haste replaces the old.
//! - **Shift** (Phase Shift): the caster's chosen regiments step straight
//!   to one of their finished buildings of the power's type, round it,
//!   leaving any fight. Every [`Command::UsePower`](crate::Command::UsePower)
//!   shifting a regiment to the same building within [`RIDE_TICKS`] of the
//!   payment rides on it, so a group order is one use even when the network
//!   carries it over a few turns.
//! - **Grant** (Rice Tax): the caster gains Grub at once.
//! - **March** (Forced March): the caster's chosen regiments march
//!   [`PowerEffect::March`]'s percent faster for the duration; like a
//!   shift, a group order is one use.
//! - **Steadfast** (Banner of the Ancestors): the caster's regiments within
//!   [`PowerEffect::Steadfast`]'s tiles of the chosen point can't rout for
//!   the duration (their morale bottoms out instead).
//!
//! - **Summon** (Ancestral Return): a full regiment of the power's unit
//!   (the Clans' Spectral Samurai) takes the field beside one of the
//!   caster's finished Shrines, facing away from it, for the duration. It
//!   takes no supply, never routs, and vanishes when the time is up (see
//!   [`Regiments::expire_summons`]). Heroes aren't in Conquest yet, so a
//!   Shrine is the only place to call it.
//!
//! The powers table, when each player last used each power and the group
//! order being paid for are in the state hash; what a power left on a
//! building or regiment is, with it.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::conquest::Conquest;
use crate::hash::StateHasher;
use crate::nav::Nav;
use crate::regiments::Regiments;
use crate::structures::{BuildingId, HONOR, Refusal, StructureHandle, Structures};
use crate::{Formation, MAX_PARTIES, PlayerId, RegimentHandle, UnitId, UnitStats};

/// The longest a power's cooldown or effect lasts, in ticks (10 minutes).
pub const MAX_POWER_TICKS: u32 = 10 * 60 * crate::TICKS_PER_SECOND;
/// The most percent faster a haste or a march makes anything.
pub const MAX_HASTE: u8 = 100;
/// The widest a steadfast power reaches, in tiles.
pub const MAX_STEADFAST_TILES: u16 = 64;
/// The most powers a match's table holds.
pub const MAX_POWERS: usize = 64;
/// How far from the edge of the building a shifted regiment stands, in
/// tiles, and how much further each ring out.
const SHIFT_OUT: i32 = 3;
const SHIFT_RING: i32 = 4;
/// Rings of eight spots round the building a shift fills, nearest first.
const SHIFT_RINGS: i32 = 5;
/// How near another regiment's centre a summoned one isn't put down, in
/// tiles, if there is room elsewhere round the building.
const SUMMON_CLEAR: i32 = 5;
/// How long after a group order (a shift or a march) is paid for more
/// regiments in it ride on the payment, in ticks (a second).
pub const RIDE_TICKS: u32 = crate::TICKS_PER_SECOND;

/// A faction meter: what a player earns and spends on powers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Meter {
    /// None the game has built (the player has no powers).
    #[default]
    NotBuilt,
    /// The Choir's, charged by chords.
    Resonance,
    /// The Clans', earned by fighting well and by Shrines.
    Honor,
}

impl Meter {
    /// The meter numbered `n` (as `self as u8` numbers them, the baked
    /// tables' `METER_*` too), if there is one.
    pub fn from_u8(n: u8) -> Option<Meter> {
        [Meter::NotBuilt, Meter::Resonance, Meter::Honor]
            .get(usize::from(n))
            .copied()
    }

    pub(crate) fn hash_into(self, h: &mut StateHasher) {
        h.u8(self as u8);
    }
}

/// What a power does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PowerEffect {
    /// Not built yet: it can't be used.
    #[default]
    NotBuilt,
    /// A building or regiment works this many percent faster.
    Haste { percent: u8 },
    /// Regiments step to one of the caster's buildings of this type.
    Shift { to: BuildingId },
    /// The caster gains this much Grub.
    Grant { grub: u16 },
    /// Regiments march this many percent faster.
    March { percent: u8 },
    /// The caster's regiments this many tiles from the chosen point can't
    /// rout.
    Steadfast { tiles: u16 },
    /// A regiment of this unit type takes the field beside one of the
    /// caster's finished buildings that earns Honor (a Shrine).
    Summon { unit: UnitId },
}

impl PowerEffect {
    /// Whether it lasts a while (its power's duration).
    pub fn lasts(self) -> bool {
        matches!(
            self,
            PowerEffect::Haste { .. }
                | PowerEffect::March { .. }
                | PowerEffect::Steadfast { .. }
                | PowerEffect::Summon { .. }
        )
    }
}

/// What the simulation needs from a power. The game builds these from the
/// baked powers table (`gc_data`); tests build them by hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PowerStats {
    /// Main-building tier needed to use it.
    pub tier: u8,
    /// Points of `meter` it costs.
    pub cost: u8,
    pub meter: Meter,
    pub cooldown_ticks: u32,
    /// How long its effect lasts (0 for one that doesn't).
    pub duration_ticks: u32,
    pub effect: PowerEffect,
}

impl PowerStats {
    /// The same power, with every value in the range the simulation works
    /// with: times up to [`MAX_POWER_TICKS`], a haste or march of 1 to
    /// [`MAX_HASTE`] percent and a steadfast of 1 to
    /// [`MAX_STEADFAST_TILES`] tiles, each lasting at least a tick.
    pub fn sanitized(self) -> PowerStats {
        let effect = match self.effect {
            PowerEffect::Haste { percent } => PowerEffect::Haste {
                percent: percent.clamp(1, MAX_HASTE),
            },
            PowerEffect::March { percent } => PowerEffect::March {
                percent: percent.clamp(1, MAX_HASTE),
            },
            PowerEffect::Steadfast { tiles } => PowerEffect::Steadfast {
                tiles: tiles.clamp(1, MAX_STEADFAST_TILES),
            },
            other => other,
        };
        let duration = if effect.lasts() {
            self.duration_ticks.clamp(1, MAX_POWER_TICKS)
        } else {
            0
        };
        PowerStats {
            cooldown_ticks: self.cooldown_ticks.min(MAX_POWER_TICKS),
            duration_ticks: duration,
            effect,
            ..self
        }
    }

    fn hash_into(&self, h: &mut StateHasher) {
        h.u8(self.tier);
        h.u8(self.cost);
        self.meter.hash_into(h);
        h.u32(self.cooldown_ticks);
        h.u32(self.duration_ticks);
        match self.effect {
            PowerEffect::NotBuilt => h.u8(0),
            PowerEffect::Haste { percent } => {
                h.u8(1);
                h.u8(percent);
            }
            PowerEffect::Shift { to } => {
                h.u8(2);
                h.u16(to);
            }
            PowerEffect::Grant { grub } => {
                h.u8(3);
                h.u16(grub);
            }
            PowerEffect::March { percent } => {
                h.u8(4);
                h.u8(percent);
            }
            PowerEffect::Steadfast { tiles } => {
                h.u8(5);
                h.u16(tiles);
            }
            PowerEffect::Summon { unit } => {
                h.u8(6);
                h.u16(unit);
            }
        }
    }
}

/// What a [`Command::UsePower`](crate::Command::UsePower) is used on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerTarget {
    /// Nothing but the caster (a grant).
    Caster,
    /// One of the caster's buildings (a haste).
    Structure(StructureHandle),
    /// One of the caster's regiments (a haste or a march).
    Regiment(RegimentHandle),
    /// One of the caster's regiments, to step to their building `to` (a
    /// shift).
    Shift {
        regiment: RegimentHandle,
        to: StructureHandle,
    },
    /// A point on the field, in tiles (a steadfast power).
    Point(FxVec2),
}

/// What using a power came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cast {
    /// Refused: nothing was spent.
    Refused,
    /// Done.
    Done,
    /// Done, and this regiment took the field.
    Raised(RegimentHandle),
}

/// The powers table and each player's use of it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Powers {
    stats: Vec<PowerStats>,
    /// By player, then power: the tick it was last used, if ever.
    used_at: Vec<Option<u32>>,
    /// By player: the group order (power, and building for a shift) their
    /// last group payment was for, and how many regiments have joined it,
    /// to spread a shift's round its building.
    group: [Option<(u8, Option<StructureHandle>)>; MAX_PARTIES],
    grouped: [u8; MAX_PARTIES],
}

impl Powers {
    pub(crate) fn new(stats: &[PowerStats]) -> Powers {
        let stats: Vec<PowerStats> = (stats.iter().take(MAX_POWERS))
            .map(|p| p.sanitized())
            .collect();
        Powers {
            used_at: vec![None; stats.len() * MAX_PARTIES],
            stats,
            group: [None; MAX_PARTIES],
            grouped: [0; MAX_PARTIES],
        }
    }

    pub(crate) fn stats(&self) -> &[PowerStats] {
        &self.stats
    }

    fn slot(&self, player: PlayerId, power: u8) -> Option<usize> {
        let (p, k) = (usize::from(player), usize::from(power));
        (p < MAX_PARTIES && k < self.stats.len()).then_some(p * self.stats.len() + k)
    }

    /// Ticks until `player` can use `power` again at `now`: 0 when ready.
    pub(crate) fn cooling(&self, player: PlayerId, power: u8, now: u32) -> u32 {
        let Some((slot, stats)) = self
            .slot(player, power)
            .zip(self.stats.get(usize::from(power)))
        else {
            return 0;
        };
        self.used_at[slot].map_or(0, |at| (at + stats.cooldown_ticks).saturating_sub(now))
    }

    /// Whether `player`, whose meter is `meter` with `points` in it and
    /// whose main building is tier `tier`, could use `power` at `now`, and
    /// if not, why: built and paid from their meter, their tier, its
    /// cooldown and its price.
    pub(crate) fn can_use(
        &self,
        player: PlayerId,
        power: u8,
        now: u32,
        (meter, points, tier): (Meter, u32, u8),
    ) -> Result<PowerStats, Refusal> {
        let stats = *self
            .stats
            .get(usize::from(power))
            .filter(|s| s.effect != PowerEffect::NotBuilt && s.meter == meter)
            .ok_or(Refusal::Unknown)?;
        self.slot(player, power).ok_or(Refusal::Unknown)?;
        if tier < stats.tier {
            return Err(Refusal::Tier);
        }
        if self.cooling(player, power, now) > 0 {
            return Err(Refusal::Cooling);
        }
        if points < u32::from(stats.cost) {
            return Err(Refusal::Price);
        }
        Ok(stats)
    }

    /// Whether `player`'s use of `power` in group `key` at `now` rides on
    /// one they paid for: the same group order, within [`RIDE_TICKS`].
    fn rides(&self, player: PlayerId, power: u8, key: Option<StructureHandle>, now: u32) -> bool {
        let paid = self.slot(player, power).and_then(|slot| self.used_at[slot]);
        paid.is_some_and(|at| now.wrapping_sub(at) < RIDE_TICKS)
            && self.group.get(usize::from(player)) == Some(&Some((power, key)))
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        h.u32(self.stats.len() as u32);
        for s in &self.stats {
            s.hash_into(h);
        }
        for at in &self.used_at {
            h.u32(at.map_or(u32::MAX, |t| t));
        }
        for (group, &n) in self.group.iter().zip(&self.grouped) {
            let (power, to) = group.map_or((u8::MAX, None), |(p, to)| (p, to));
            h.u8(power);
            h.u32(to.map_or(u32::MAX, |t| t.index));
            h.u32(to.map_or(u32::MAX, |t| u32::from(t.generation)));
            h.u8(n);
        }
    }
}

/// The group order a use of a power of `effect` on `target` joins (the
/// shift's building, if a shift), if it is one.
fn group_of(effect: PowerEffect, target: PowerTarget) -> Option<Option<StructureHandle>> {
    match (effect, target) {
        (PowerEffect::Shift { .. }, PowerTarget::Shift { to, .. }) => Some(Some(to)),
        (PowerEffect::March { .. }, PowerTarget::Regiment(_)) => Some(None),
        _ => None,
    }
}

impl Conquest {
    /// The points in `player`'s meter, and which meter it is.
    fn meter_of(&self, player: PlayerId) -> (Meter, u32) {
        let meter = self
            .meters
            .get(usize::from(player))
            .copied()
            .unwrap_or_default();
        let points = match meter {
            Meter::NotBuilt => 0,
            Meter::Resonance => self.structures.resonance(player),
            Meter::Honor => self.honor.points(player),
        };
        (meter, points)
    }

    /// Whether `player` could use `power` at `now`, and if not, why (see
    /// [`Powers::can_use`]).
    pub(crate) fn can_use_power(
        &self,
        player: PlayerId,
        power: u8,
        now: u32,
    ) -> Result<PowerStats, Refusal> {
        let (meter, points) = self.meter_of(player);
        let tier = self.structures.tier_of(player);
        let stats = self
            .powers
            .can_use(player, power, now, (meter, points, tier))?;
        // Edicts are cast from a Shrine.
        let shrine = || self.structures.finished_with(player, HONOR).next();
        if meter == Meter::Honor && shrine().is_none() {
            return Err(Refusal::NoShrine);
        }
        Ok(stats)
    }

    /// Whether `power`, if it summons, has room to put its regiment down
    /// round `player`'s Shrine `shrine` (see [`summon_spot`]); the same
    /// rule a use of it is refused by. Says nothing of the Honor.
    pub(crate) fn can_summon_at(
        &self,
        regs: &Regiments,
        nav: &Nav,
        units: &[UnitStats],
        player: PlayerId,
        power: u8,
        shrine: StructureHandle,
    ) -> bool {
        let Some(&PowerStats {
            effect: PowerEffect::Summon { unit },
            ..
        }) = self.powers.stats.get(usize::from(power))
        else {
            return false;
        };
        units.get(usize::from(unit)).is_some_and(|stats| {
            summon_spot(&self.structures, regs, nav, stats, player, shrine).is_some()
        })
    }

    /// `player` uses `power` on `target` at `now`; refused (and nothing
    /// spent) if they can't.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn use_power(
        &mut self,
        regs: &mut Regiments,
        nav: &Nav,
        units: &[UnitStats],
        player: PlayerId,
        power: u8,
        target: PowerTarget,
        now: u32,
    ) -> Cast {
        let Some(&stats) = self.powers.stats.get(usize::from(power)) else {
            return Cast::Refused;
        };
        let group = group_of(stats.effect, target);
        let riding = group.is_some_and(|key| self.powers.rides(player, power, key, now));
        if !riding && self.can_use_power(player, power, now).is_err() {
            return Cast::Refused;
        }
        let p = usize::from(player);
        let joined = if riding { self.powers.grouped[p] } else { 0 };
        let lasts = effect_ticks(stats.duration_ticks);
        let mut raised = None;
        let done = match (stats.effect, target) {
            (PowerEffect::Haste { percent }, PowerTarget::Structure(h)) => {
                let structures = &mut self.structures;
                structures
                    .owned(h, player)
                    .map(|i| structures.haste(i, percent, lasts))
                    .is_some()
            }
            (PowerEffect::Haste { percent }, PowerTarget::Regiment(h)) => regs
                .owned(h, player)
                .map(|r| {
                    regs.haste_left[r] = lasts;
                    regs.haste[r] = percent;
                })
                .is_some(),
            (PowerEffect::Shift { to: building }, PowerTarget::Shift { regiment, to }) => shift(
                &self.structures,
                regs,
                nav,
                player,
                regiment,
                building,
                to,
                joined,
            ),
            (PowerEffect::Grant { grub }, PowerTarget::Caster) => {
                self.economy.refund(player, u32::from(grub), 0);
                true
            }
            (PowerEffect::March { percent }, PowerTarget::Regiment(h)) => regs
                .standing(h, player)
                .map(|r| {
                    regs.march_left[r] = lasts;
                    regs.march[r] = percent;
                })
                .is_some(),
            (PowerEffect::Steadfast { tiles }, PowerTarget::Point(at)) => {
                // Only paid for when it steadies one of theirs.
                let reach = u64::from(Fx::from_int(i32::from(tiles)).raw().unsigned_abs());
                let mut steadied = false;
                for r in 0..regs.alive.len() {
                    if regs.alive[r]
                        && regs.owner[r] == player
                        && (regs.position[r] - at).length_sq_raw() <= reach * reach
                    {
                        regs.steadfast_left[r] = lasts;
                        steadied = true;
                    }
                }
                steadied
            }
            (PowerEffect::Summon { unit }, PowerTarget::Structure(h)) => {
                raised = summon(&self.structures, regs, nav, units, player, unit, h, lasts);
                raised.is_some()
            }
            _ => false,
        };
        if !done {
            return Cast::Refused;
        }
        if let Some(key) = group {
            self.powers.group[p] = Some((power, key));
            self.powers.grouped[p] = joined.saturating_add(1);
        }
        if !riding {
            let cost = u32::from(stats.cost);
            match stats.meter {
                Meter::Resonance => self.structures.spend_resonance(player, cost),
                Meter::Honor => self.honor.spend(player, cost),
                Meter::NotBuilt => {}
            }
            if let Some(slot) = self.powers.slot(player, power) {
                self.powers.used_at[slot] = Some(now);
            }
        }
        raised.map_or(Cast::Done, Cast::Raised)
    }
}

/// Step `player`'s regiment out of any fight (its foes let it go) to the
/// spot after the first `before` round their finished building `to` of
/// type `building`.
#[allow(clippy::too_many_arguments)]
fn shift(
    structures: &Structures,
    regs: &mut Regiments,
    nav: &Nav,
    player: PlayerId,
    regiment: RegimentHandle,
    building: BuildingId,
    to: StructureHandle,
    before: u8,
) -> bool {
    let Some(r) = regs
        .resolve(regiment)
        .filter(|&r| regs.owner[r] == player && regs.alive[r] && !regs.morale_state[r].broken())
    else {
        return false;
    };
    let Some(s) = structures.resolve(to).filter(|&s| {
        structures.owner_of(s) == player
            && structures.finished(s)
            && structures.building_of(s) == building
    }) else {
        return false;
    };
    let Some(at) = spots_round(structures, s)
        .filter(|&at| nav.walkable(at))
        .nth(usize::from(before))
    else {
        return false;
    };
    let facing = regs.facing[r];
    regs.place(regiment, at, facing, 0).is_some_and(|i| {
        regs.vanish_from_fight(i);
        true
    })
}

/// The spots round building slot `s`, nearest ring first, eight to a ring.
fn spots_round(structures: &Structures, s: usize) -> impl Iterator<Item = FxVec2> + use<> {
    let foot = structures.footprint(s);
    let centre = structures.centre(s);
    (0..SHIFT_RINGS).flat_map(move |ring| {
        let out = foot[2].max(foot[3]) / 2 + SHIFT_OUT + ring * SHIFT_RING;
        [
            (0, 1),
            (1, 1),
            (1, 0),
            (1, -1),
            (0, -1),
            (-1, -1),
            (-1, 0),
            (-1, 1),
        ]
        .map(|(x, y)| centre + FxVec2::new(Fx::from_int(x * out), Fx::from_int(y * out)))
    })
}

/// Where and facing which way a regiment of `unit` would stand if
/// `player`'s Shrine `shrine` summoned it: the nearest spot round the
/// Shrine where open ground holds its whole block (a spot too near
/// blocked ground is settled to ground that does, as a rallied regiment
/// is, and one that can't be is out), with no other regiment close, else
/// the nearest such spot. Facing away from the Shrine. `None` if `shrine`
/// isn't one of their finished buildings that earn Honor, there is no
/// such spot, or the regiment pool is full.
fn summon_spot(
    structures: &Structures,
    regs: &Regiments,
    nav: &Nav,
    stats: &UnitStats,
    player: PlayerId,
    shrine: StructureHandle,
) -> Option<(FxVec2, Angle)> {
    let s = structures.resolve(shrine)?;
    if !structures.finished_with(player, HONOR).any(|i| i == s) || regs.free.is_empty() {
        return None;
    }
    let berth = Regiments::berth_of(stats, Formation::Line, stats.files, stats.models);
    let near = u64::from(Fx::from_int(SUMMON_CLEAR).raw().unsigned_abs());
    let crowded = |at: FxVec2| {
        (0..regs.alive.len())
            .any(|r| regs.alive[r] && (regs.position[r] - at).length_sq_raw() <= near * near)
    };
    let mut room = spots_round(structures, s)
        .filter(|&at| nav.walkable(at))
        .map(|at| nav.settle(at, berth))
        .filter(|&at| nav.fits(at, berth));
    let first = room.next()?;
    let at = core::iter::once(first)
        .chain(room)
        .find(|&at| !crowded(at))
        .unwrap_or(first);
    Some((at, Angle::of_vec(at - structures.centre(s))))
}

/// Put a regiment of `unit` down beside `player`'s finished Shrine `shrine`
/// for `lasts` ticks, where [`summon_spot`] says. `None` if there is no
/// room for it.
#[allow(clippy::too_many_arguments)]
fn summon(
    structures: &Structures,
    regs: &mut Regiments,
    nav: &Nav,
    units: &[UnitStats],
    player: PlayerId,
    unit: UnitId,
    shrine: StructureHandle,
    lasts: u16,
) -> Option<RegimentHandle> {
    let stats = units.get(usize::from(unit))?;
    let (at, facing) = summon_spot(structures, regs, nav, stats, player, shrine)?;
    let h = regs.spawn(player, unit, at, facing, stats)?;
    regs.summoned_left[h.index as usize] = lasts;
    Some(h)
}

/// A lasting effect's ticks, as a building's or regiment's count of them.
fn effect_ticks(duration: u32) -> u16 {
    u16::try_from(duration).unwrap_or(u16::MAX)
}

/// How many steps of work something hasted `percent` percent with `left`
/// ticks of haste to go does this tick: 1, and one more on the ticks that
/// add up to `percent` percent over the haste.
pub(crate) fn steps(left: u16, percent: u8) -> u16 {
    if left == 0 {
        return 1;
    }
    let (left, p) = (u32::from(left), u32::from(percent.min(MAX_HASTE)));
    // At most 2: a haste is at most 100 percent.
    1 + (left * p / 100 - (left - 1) * p / 100) as u16
}

/// Whether something whose turns fall on multiples of the strike interval,
/// hasted `percent` percent, has an extra turn at `t`: halfway through turn
/// `k`, each time the turns so far times the percent pass another hundred.
fn extra_turn(t: u64, percent: u8) -> bool {
    let interval = u64::from(crate::combat::STRIKE_INTERVAL);
    if t % interval != interval / 2 {
        return false;
    }
    let (k, p) = (t / interval, u64::from(percent.min(MAX_HASTE)));
    (k + 1) * p / 100 != k * p / 100
}

impl Structures {
    /// Building slot `i` works `percent` percent faster for `ticks` ticks.
    pub(crate) fn haste(&mut self, i: usize, percent: u8, ticks: u16) {
        self.haste_left[i] = ticks;
        self.haste_percent[i] = percent;
    }

    /// The steps of work building slot `i` does this tick (see [`steps`]).
    pub(crate) fn steps(&self, i: usize) -> u32 {
        u32::from(steps(self.haste_left[i], self.haste_percent[i]))
    }

    /// Building `h`'s haste: ticks left, 0 for none.
    pub fn haste_left(&self, h: StructureHandle) -> u16 {
        self.resolve(h).map_or(0, |i| self.haste_left[i])
    }

    /// A tick of every haste goes by.
    pub(crate) fn wear_haste(&mut self) {
        for left in &mut self.haste_left {
            *left = left.saturating_sub(1);
        }
    }
}

impl Regiments {
    /// The steps of work regiment slot `i` does this tick (see [`steps`]).
    pub(crate) fn steps(&self, i: usize) -> u16 {
        steps(self.haste_left[i], self.haste[i])
    }

    /// Whether hasted regiment slot `i` has an extra turn to strike at
    /// `tick`: as many more turns as it is hasted percent, halfway between
    /// its own, so the two never fall together.
    pub(crate) fn extra_turn(&self, i: usize, tick: u32) -> bool {
        self.haste_left[i] > 0 && extra_turn(u64::from(tick) + i as u64, self.haste[i])
    }

    /// A tick of every regiment's haste, march and steadfastness goes by.
    pub(crate) fn wear_powers(&mut self) {
        for left in (self.haste_left.iter_mut())
            .chain(&mut self.march_left)
            .chain(&mut self.steadfast_left)
        {
            *left = left.saturating_sub(1);
        }
    }

    /// How far regiment slot `i` goes at `pace` this tick: further on a
    /// march (see [`PowerEffect::March`]).
    pub(crate) fn marched(&self, i: usize, pace: Fx) -> Fx {
        if self.march_left[i] == 0 {
            return pace;
        }
        let percent = i64::from(self.march[i].min(MAX_HASTE));
        Fx::from_raw((i64::from(pace.raw()) * (100 + percent) / 100) as i32)
    }

    /// Regiment `h`'s march: ticks left, 0 for none.
    pub fn march_left(&self, h: RegimentHandle) -> u16 {
        self.resolve(h).map_or(0, |i| self.march_left[i])
    }

    /// Regiment `h`'s steadfastness: ticks left, 0 for none.
    pub fn steadfast_left(&self, h: RegimentHandle) -> u16 {
        self.resolve(h).map_or(0, |i| self.steadfast_left[i])
    }

    /// Regiment `h`'s haste: ticks left, 0 for none.
    pub fn haste_left(&self, h: RegimentHandle) -> u16 {
        self.resolve(h).map_or(0, |i| self.haste_left[i])
    }

    /// Regiment `h`'s time as a summoned regiment: ticks left before it
    /// vanishes, 0 for one that isn't summoned.
    pub fn summoned_left(&self, h: RegimentHandle) -> u16 {
        self.resolve(h).map_or(0, |i| self.summoned_left[i])
    }

    /// A tick of every summoned regiment's time goes by. Each whose time
    /// is up is gone: `leaving` is told its slot, then it is out of any
    /// fight, with nobody left locked with it, and freed.
    pub(crate) fn expire_summons(&mut self, mut leaving: impl FnMut(usize)) {
        for i in 0..self.alive.len() {
            if !self.alive[i] || self.summoned_left[i] == 0 {
                continue;
            }
            self.summoned_left[i] -= 1;
            if self.summoned_left[i] == 0 {
                leaving(i);
                self.vanish_from_fight(i);
                self.despawn(self.handle(i));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fifty_percent_haste_takes_three_steps_every_two_ticks() {
        let total: u32 = (1..=400).rev().map(|left| u32::from(steps(left, 50))).sum();
        assert_eq!(total, 600);
        assert_eq!(steps(0, 50), 1);
        let total: u32 = (1..=7).rev().map(|left| u32::from(steps(left, 100))).sum();
        assert_eq!(total, 14);
    }

    #[test]
    fn a_haste_adds_its_percent_of_strike_turns_and_never_on_a_turn_of_its_own() {
        let interval = u64::from(crate::combat::STRIKE_INTERVAL);
        for percent in [1, 30, 40, 50, 60, 75, 100] {
            let turns = 100 * interval;
            let extra: Vec<u64> = (0..turns).filter(|&t| extra_turn(t, percent)).collect();
            assert_eq!(extra.len(), usize::from(percent), "{percent}%");
            assert!(extra.iter().all(|t| t % interval != 0), "{percent}%");
        }
    }

    #[test]
    fn sanitizing_keeps_times_and_haste_in_range() {
        let s = PowerStats {
            tier: 1,
            cost: 20,
            meter: Meter::Resonance,
            cooldown_ticks: u32::MAX,
            duration_ticks: 0,
            effect: PowerEffect::Haste { percent: 250 },
        }
        .sanitized();
        assert_eq!(s.cooldown_ticks, MAX_POWER_TICKS);
        assert_eq!(s.duration_ticks, 1);
        assert_eq!(s.effect, PowerEffect::Haste { percent: MAX_HASTE });
    }
}
