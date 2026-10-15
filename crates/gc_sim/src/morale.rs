//! Morale and routing (docs/05-combat.md, "Morale").
//!
//! Armies lose because they break, not because every soldier dies. Each
//! regiment has a morale value, in points, that starts at its unit's
//! `morale` stat. Once a second (staggered by pool index, like melee blows)
//! it is checked:
//! - it falls with casualties taken since the last check, while the regiment
//!   is below half strength and fighting, when hit in the flank or rear, and
//!   when fighting while badly outnumbered nearby;
//! - it recovers when the regiment is out of the fight and losing nobody, up
//!   to the unit's stat scaled by how much of the regiment is left (half the
//!   stat for a regiment that has lost almost everyone).
//!
//! Below [`WAVER_BELOW`] points a regiment is *wavering* and deals less
//! damage. At zero it *routs*: it breaks off, ignores orders and runs from
//! the enemy, a little slower than it marches. Its rout shakes friends
//! nearby and heartens the enemies who broke it. After [`RALLY_SECONDS`]
//! with no enemy near, it rallies, wavering. A regiment that routs again
//! within [`SHATTER_SECONDS`] is *shattered*: it runs, then stands where it
//! stops and never takes orders again (later it will recover at a friendly
//! structure). Regiments whose unit has a morale stat of 0 never rout.

use gc_fixed::{Angle, Fx, FxVec2};

use crate::combat::{Combat, Placement, Whose};
use crate::regiments::Regiments;
use crate::{Formation, RegimentHandle, Stances, TICKS_PER_SECOND, UnitStats};

/// Morale is kept in tenths of a point.
pub const SCALE: i16 = 10;
/// Below this many points a regiment wavers.
pub const WAVER_BELOW: i16 = 25;
/// Percent of its usual damage a wavering regiment deals.
pub const WAVERING_DAMAGE_PERCENT: u64 = 85;
/// Tenths of a point lost per percent of full strength lost since the last
/// check: losing a tenth of the regiment in one second costs 12 points.
const CASUALTY_TENTHS_PER_PERCENT: i32 = 9;
/// Points lost each second while below half strength and in melee.
const HALF_STRENGTH_DRAIN: i16 = 2;
/// Points lost in a second with hits on the flank, or on the rear.
const FLANK_SHOCK: i16 = 3;
const REAR_SHOCK: i16 = 6;
/// Bits in `Regiments::shock`.
pub(crate) const SHOCK_FLANK: u8 = 1;
pub(crate) const SHOCK_REAR: u8 = 2;
/// Enemies and friends within this many tiles count toward being outnumbered.
const LOCAL_TILES: i32 = 12;
/// Points lost each second in melee when nearby enemies outnumber nearby
/// friends two to one, or three to one.
const OUTNUMBERED_2: i16 = 2;
const OUTNUMBERED_3: i16 = 4;
/// Points regained each second when out of the fight and losing nobody.
const RECOVERY: i16 = 3;
/// A rout costs friends within this many tiles some points, and gives the
/// enemies fighting the routers some.
const ROUT_SHOCK_TILES: i32 = 16;
const ROUT_SHOCK: i16 = 8;
const VICTORY_BOOST: i16 = 5;
/// Routing regiments run at this percent of their marching speed, so
/// pursuers catch them.
const ROUT_PACE_PERCENT: i32 = 90;
/// How far ahead a routing regiment aims each second, in tiles.
const FLEE_TILES: i32 = 10;
/// Seconds with no enemy within `RALLY_CLEAR_TILES` before routers rally.
pub const RALLY_SECONDS: u32 = 10;
const RALLY_CLEAR_TILES: i32 = 20;
/// Routing again within this many seconds of the last rout shatters a regiment.
pub const SHATTER_SECONDS: u32 = 120;
/// Fleeing regiments stay this far inside the map edge, in tiles.
const EDGE_MARGIN: i32 = 2;

/// A regiment's state of mind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MoraleState {
    Steady = 0,
    /// Shaken: deals 15% less damage.
    Wavering = 1,
    /// Running; ignores orders; rallies when the enemy is gone.
    Routing = 2,
    /// Broke twice: runs, then stands, and takes no more orders.
    Shattered = 3,
}

impl MoraleState {
    /// Running away (or past caring): no fighting, no orders.
    pub fn broken(self) -> bool {
        matches!(self, MoraleState::Routing | MoraleState::Shattered)
    }
}

/// Starting (and highest) morale for a regiment of this unit, in tenths.
pub(crate) fn fresh(stats: &UnitStats) -> i16 {
    stats.morale as i16 * SCALE
}

/// Borrow where everyone is without borrowing the rest of the pool.
macro_rules! placement {
    ($regs:expr, $stances:expr) => {
        Placement {
            alive: &$regs.alive,
            owner: &$regs.owner,
            position: &$regs.position,
            stances: $stances,
        }
    };
}

/// Check every regiment whose turn it is this tick.
pub(crate) fn update(
    regs: &mut Regiments,
    combat: &Combat,
    units: &[UnitStats],
    stances: &Stances,
    tick: u32,
    map_tiles: u32,
) {
    for i in 0..regs.alive.len() {
        // Half a second out of step with the melee blows.
        let turn = tick + i as u32 + TICKS_PER_SECOND / 2;
        if !regs.alive[i] || !turn.is_multiple_of(TICKS_PER_SECOND) {
            continue;
        }
        let stats = &units[regs.unit[i] as usize];
        if stats.morale != 0 {
            if regs.morale_state[i].broken() {
                flee(regs, combat, stances, i, map_tiles);
            } else {
                check(regs, combat, stances, stats, i, tick, map_tiles);
            }
        }
        regs.losses[i] = 0;
        regs.shock[i] = 0;
    }
}

/// The once-a-second check for a regiment that is still in the fight.
fn check(
    regs: &mut Regiments,
    combat: &Combat,
    stances: &Stances,
    stats: &UnitStats,
    i: usize,
    tick: u32,
    map_tiles: u32,
) {
    let full = regs.full_models[i].max(1) as i32;
    let mut tenths = -(regs.losses[i] as i32 * 100 * CASUALTY_TENTHS_PER_PERCENT / full);
    if regs.shock[i] & SHOCK_REAR != 0 {
        tenths -= (REAR_SHOCK * SCALE) as i32;
    } else if regs.shock[i] & SHOCK_FLANK != 0 {
        tenths -= (FLANK_SHOCK * SCALE) as i32;
    }
    if regs.engaged[i].is_some() {
        if (regs.models[i] as i32) * 2 < full {
            tenths -= (HALF_STRENGTH_DRAIN * SCALE) as i32;
        }
        let (friends, enemies) = local_strength(regs, combat, stances, i);
        if enemies >= friends.max(1) * 3 {
            tenths -= (OUTNUMBERED_3 * SCALE) as i32;
        } else if enemies >= friends.max(1) * 2 {
            tenths -= (OUTNUMBERED_2 * SCALE) as i32;
        }
    } else if regs.losses[i] == 0 {
        tenths += (RECOVERY * SCALE) as i32;
    }
    // A battered regiment never gets all its nerve back: the cap falls from
    // the unit's stat at full strength to half of it with nobody left.
    let cap = fresh(stats) as i32 * (full + regs.models[i] as i32) / (2 * full);
    let morale = (regs.morale[i] as i32 + tenths).clamp(0, cap) as i16;
    regs.morale[i] = morale;
    if morale > 0 {
        regs.morale_state[i] = steady_or_wavering(morale);
    } else {
        rout(regs, combat, stances, i, tick, map_tiles);
    }
}

fn steady_or_wavering(morale: i16) -> MoraleState {
    if morale < WAVER_BELOW * SCALE {
        MoraleState::Wavering
    } else {
        MoraleState::Steady
    }
}

/// Soldiers still fighting near regiment `i`: (its side's, the enemy's).
fn local_strength(regs: &Regiments, combat: &Combat, stances: &Stances, i: usize) -> (u32, u32) {
    let at = placement!(regs, stances);
    let (p, owner, local) = (regs.position[i], regs.owner[i], Fx::from_int(LOCAL_TILES));
    let (mut friends, mut enemies) = (0u32, 0u32);
    combat.each_near(&at, p, local, Whose::Friends(owner), |j| {
        if !regs.morale_state[j].broken() {
            friends += regs.models[j] as u32;
        }
    });
    combat.each_near(&at, p, local, Whose::Enemies(owner), |j| {
        if !regs.morale_state[j].broken() {
            enemies += regs.models[j] as u32;
        }
    });
    (friends, enemies)
}

/// Regiment `i` breaks and runs.
fn rout(
    regs: &mut Regiments,
    combat: &Combat,
    stances: &Stances,
    i: usize,
    tick: u32,
    map_tiles: u32,
) {
    let again = regs.last_rout[i] != 0
        && tick + 1 - regs.last_rout[i] <= SHATTER_SECONDS * TICKS_PER_SECOND;
    regs.morale_state[i] = if again {
        MoraleState::Shattered
    } else {
        MoraleState::Routing
    };
    regs.morale[i] = 0;
    regs.last_rout[i] = tick + 1;
    regs.rout_clock[i] = 0;
    regs.engaged[i] = None;
    regs.attack_target[i] = None;
    regs.target_facing[i] = None;
    regs.charge[i] = 0;
    // Running men keep no ranks.
    regs.formation[i] = Formation::Line;
    regs.pace[i] = Fx::from_raw(regs.speed[i].raw() * ROUT_PACE_PERCENT / 100);

    // Friends nearby are shaken; the enemies who broke them take heart. The
    // friends' own checks decide whether they break in turn.
    let me = RegimentHandle {
        index: i as u32,
        generation: regs.generation[i],
    };
    let (p, owner, near) = (
        regs.position[i],
        regs.owner[i],
        Fx::from_int(ROUT_SHOCK_TILES),
    );
    let at = placement!(regs, stances);
    let morale = &mut regs.morale;
    let state = &mut regs.morale_state;
    combat.each_near(&at, p, near, Whose::Friends(owner), |j| {
        if j != i && !state[j].broken() && morale[j] > 0 {
            morale[j] = (morale[j] - ROUT_SHOCK * SCALE).max(1);
            state[j] = steady_or_wavering(morale[j]);
        }
    });
    let engaged = &regs.engaged;
    combat.each_near(&at, p, near, Whose::Enemies(owner), |j| {
        if engaged[j] == Some(me) && !state[j].broken() && morale[j] > 0 {
            // May go above the unit's stat until its next check caps it.
            morale[j] = morale[j].saturating_add(VICTORY_BOOST * SCALE);
            state[j] = steady_or_wavering(morale[j]);
        }
    });
    flee(regs, combat, stances, i, map_tiles);
}

/// Steer a routing regiment away from the enemy, and rally (or, shattered,
/// stand) once the enemy has been gone long enough.
fn flee(regs: &mut Regiments, combat: &Combat, stances: &Stances, i: usize, map_tiles: u32) {
    let p = regs.position[i];
    let owner = regs.owner[i];
    let at = placement!(regs, stances);
    let mut away = (0i64, 0i64);
    let mut near = false;
    combat.each_near(
        &at,
        p,
        Fx::from_int(RALLY_CLEAR_TILES),
        Whose::Enemies(owner),
        |j| {
            near = true;
            let d = p - at.position[j];
            // Closer enemies push harder: weight by 1 / distance.
            let len = d.length().raw().max(1) as i64;
            away.0 += d.x.raw() as i64 * 65_536 / len;
            away.1 += d.y.raw() as i64 * 65_536 / len;
        },
    );
    if near {
        regs.rout_clock[i] = 0;
    } else {
        regs.rout_clock[i] = regs.rout_clock[i].saturating_add(TICKS_PER_SECOND as u16);
        if regs.rout_clock[i] as u32 >= RALLY_SECONDS * TICKS_PER_SECOND {
            regs.target[i] = None;
            if regs.morale_state[i] == MoraleState::Routing {
                // Rally: wavering, with a little heart back.
                regs.morale_state[i] = MoraleState::Wavering;
                regs.morale[i] = WAVER_BELOW * SCALE / 2;
                regs.pace[i] = regs.speed[i];
            }
            return;
        }
    }
    let dir = if away != (0, 0) {
        let v = FxVec2::new(
            Fx::from_raw(away.0.clamp(i32::MIN as i64, i32::MAX as i64) as i32),
            Fx::from_raw(away.1.clamp(i32::MIN as i64, i32::MAX as i64) as i32),
        );
        Angle::of_vec(v).to_vec()
    } else if let Some(t) = regs.target[i] {
        // Nobody close: keep running the way it was going.
        let d = t - p;
        if d == FxVec2::ZERO {
            return;
        }
        Angle::of_vec(d).to_vec()
    } else {
        regs.facing[i].to_vec()
    };
    let mut target = p + dir * Fx::from_int(FLEE_TILES);
    if map_tiles > 0 {
        let lo = Fx::from_int(EDGE_MARGIN);
        let hi = Fx::from_int(map_tiles as i32 - EDGE_MARGIN);
        target = FxVec2::new(target.x.clamp(lo, hi), target.y.clamp(lo, hi));
    }
    regs.target[i] = (target != p).then_some(target);
    regs.target_facing[i] = None;
}
