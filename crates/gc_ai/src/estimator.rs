//! The battle estimator: a fast analytic model of a fight between two groups
//! of regiments, built from the simulation's own rules (docs/18-ai.md,
//! "Engage or withdraw").
//!
//! Each side is boiled down to a few numbers: hit points, average morale,
//! how many hit points a second it deals in melee and with arrows. The model
//! then steps the fight forward half a second at a time, the way the sim's
//! rules play out on average:
//! - **Melee:** every front-rank soldier strikes once a second (spears and
//!   pikes add half their second rank; formations cramp or free weapons and
//!   change defence) and lands with chance 50 + attack - defence percent
//!   (10 to 90); regiments beyond the enemy's frontage hit flanks for 40%
//!   more.
//! - **Arrows:** soldiers x accuracy x damage per volley, from as soon as the
//!   enemy is in range, and on through the melee; shield walls turn most of
//!   them.
//! - **Breaking:** losses cost 0.9 morale per 1% of strength, plus the
//!   drains for being outnumbered two or three to one, for being below half
//!   strength, and for being flanked. A side whose morale is spent breaks;
//!   regiments that never rout fight until they are dead.
//!
//! The result says who breaks first and how decisively. It is quick (a few
//! microseconds) and approximate; the AI re-runs it every second. Its
//! coefficients are checked against real sim battles in `tests/estimator.rs`.

use crate::perception::{Class, Perception, Seen};

/// Longest fight the model plays out, in seconds.
pub const HORIZON: f32 = 240.0;
const STEP: f32 = 0.5;
/// Morale points lost per 1% of strength lost (the sim's 9 tenths).
const MORALE_PER_PERCENT: f32 = 0.9;
/// A side breaks a little before its average morale reaches zero: its
/// shakiest regiments rout first and every rout costs its neighbours 8
/// points.
const BREAK_AT: f32 = 6.0;
/// Melee damage multiplier for regiments that reach a flank.
const FLANK_DAMAGE: f32 = 1.4;
/// Morale drains in the sim, points per second.
const OUTNUMBERED_2: f32 = 2.0;
const OUTNUMBERED_3: f32 = 4.0;
const HALF_STRENGTH: f32 = 2.0;
const FLANK_SHOCK: f32 = 3.0;
/// A charge adds its bonus to the first blows, fading over three seconds:
/// on average worth this many seconds of blows at the full bonus.
const CHARGE_SECONDS: f32 = 1.5;

/// One side of a fight, boiled down.
#[derive(Clone, Copy, Debug, Default)]
pub struct Force {
    /// Hit points standing, and at full strength.
    pub hp: f32,
    pub hp_full: f32,
    /// Hit points in regiments that never rout.
    pub stalwart_hp: f32,
    /// Average morale of the regiments that can rout, weighted by soldiers.
    pub morale: f32,
    pub soldiers: f32,
    /// Regiments that fight in melee.
    pub melee_regiments: f32,
    /// Soldiers in the front ranks, with each one's chance to land a blow
    /// left to the matchup: sum of front-rank soldiers x damage, and of
    /// front-rank soldiers x damage x attack.
    front_damage: f32,
    front_damage_attack: f32,
    /// Extra damage from charges, as a fraction of a second's blows.
    charge: f32,
    /// Hit points a second dealt by arrows at full strength.
    pub ranged: f32,
    /// Longest range among its shooters, tiles.
    pub range: f32,
    /// Average melee defence, weighted by soldiers.
    pub defence: f32,
    /// Slowest marching speed, tiles per second.
    pub speed: f32,
    /// Share of the arrows coming at its front that do no harm (shield
    /// walls), weighted by soldiers.
    pub arrows_turned: f32,
}

impl Force {
    /// Sum up the regiments in `group` (broken regiments count for nothing).
    pub fn of<'a>(view: &Perception, group: impl IntoIterator<Item = &'a Seen>) -> Force {
        let mut f = Force {
            speed: f32::MAX,
            ..Force::default()
        };
        let mut morale_soldiers = 0.0;
        let mut defence_soldiers = 0.0;
        for r in group {
            if r.broken() || r.models == 0 {
                continue;
            }
            let u = view.unit(r);
            let s = &u.stats;
            let n = r.models as f32;
            let hp = n * s.hp_per_model as f32;
            f.hp += hp;
            f.hp_full += r.full as f32 * s.hp_per_model as f32;
            f.soldiers += n;
            if u.stalwart {
                f.stalwart_hp += hp;
            } else {
                f.morale += r.morale_points as f32 * n;
                morale_soldiers += n;
            }
            // Taking the enemy head on.
            f.defence +=
                (s.melee_defence as i32 + s.formation_defence(r.formation, true)) as f32 * n;
            f.arrows_turned += (100 - r.formation.arrow_percent(true)) as f32 / 100.0 * n;
            defence_soldiers += n;
            f.speed = f.speed.min(view.pace(r));
            if u.class == Class::Ranged {
                // Archers fight in melee too, badly, if caught; the model
                // keeps them shooting from behind the line.
                f.ranged +=
                    n * s.accuracy as f32 / 100.0 * s.missile_damage as f32 / u.reload_seconds();
                f.range = f.range.max(u.range);
            } else {
                // The front rank (and a spear block's second); the formation
                // can cramp the weapons.
                let front = (s.front_fighters_percent(r.files, r.models)
                    * s.weapon_room_percent(r.formation)) as f32
                    / 10_000.0;
                f.melee_regiments += 1.0;
                f.front_damage += front * s.melee_damage as f32;
                f.front_damage_attack += front * s.melee_damage as f32 * s.melee_attack as f32;
                if r.formation.can_charge() {
                    f.charge += front * s.melee_damage as f32 * s.charge_bonus as f32 / 100.0
                        * CHARGE_SECONDS;
                }
            }
        }
        if morale_soldiers > 0.0 {
            f.morale /= morale_soldiers;
        }
        if defence_soldiers > 0.0 {
            f.defence /= defence_soldiers;
            f.arrows_turned /= defence_soldiers;
        }
        if f.speed == f32::MAX {
            f.speed = 0.0;
        }
        f
    }

    pub fn is_empty(&self) -> bool {
        self.hp <= 0.0
    }

    /// Hit points a second this force deals in melee against `defence`,
    /// at full strength and before flanking.
    fn melee_against(&self, defence: f32) -> f32 {
        if self.front_damage <= 0.0 {
            return 0.0;
        }
        // Each soldier's chance is 50 + attack - defence, clamped; using the
        // damage-weighted average attack keeps this one division.
        let attack = self.front_damage_attack / self.front_damage;
        let hit = (50.0 + attack - defence).clamp(10.0, 90.0) / 100.0;
        self.front_damage * hit
    }
}

/// What the estimator expects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate {
    /// From -1 (we break at once, they barely notice) through 0 (a coin
    /// toss) to +1 (they break at once).
    pub margin: f32,
    /// Seconds until each side breaks; [`HORIZON`] if it holds that long.
    pub ours_break: f32,
    pub theirs_break: f32,
    /// Fraction of each side's soldiers lost by the time the fight is decided.
    pub ours_lost: f32,
    pub theirs_lost: f32,
}

impl Estimate {
    pub fn we_win(&self) -> bool {
        self.margin > 0.0
    }
}

/// How the fight starts.
#[derive(Clone, Copy, Debug)]
pub struct Approach {
    /// Tiles between the two fronts.
    pub gap: f32,
    /// How fast the gap closes, tiles per second (both marching: the sum of
    /// their speeds; one standing: the other's).
    pub closing: f32,
}

impl Approach {
    /// Already in contact.
    pub const CONTACT: Approach = Approach {
        gap: 0.0,
        closing: 1.0,
    };
}

/// Running state of one side in the model.
#[derive(Clone, Copy)]
struct Fighting {
    hp: f32,
    /// Hit points that can still rout (the rest never do).
    routable: f32,
    morale: f32,
    broken_at: Option<f32>,
}

/// Play out a fight between `ours` and `theirs`.
pub fn estimate(ours: &Force, theirs: &Force, approach: Approach) -> Estimate {
    if ours.is_empty() || theirs.is_empty() {
        let margin = match (ours.is_empty(), theirs.is_empty()) {
            (false, true) => 1.0,
            (true, false) => -1.0,
            _ => 0.0,
        };
        return Estimate {
            margin,
            ours_break: if ours.is_empty() { 0.0 } else { HORIZON },
            theirs_break: if theirs.is_empty() { 0.0 } else { HORIZON },
            ours_lost: 0.0,
            theirs_lost: 0.0,
        };
    }
    let contact_at = if approach.gap <= 0.0 {
        0.0
    } else {
        approach.gap / approach.closing.max(0.1)
    };
    // Extra melee regiments wrap the enemy's flanks.
    let flank = |a: &Force, b: &Force| {
        if a.melee_regiments > b.melee_regiments && a.melee_regiments > 0.0 {
            (a.melee_regiments - b.melee_regiments) / a.melee_regiments
        } else {
            0.0
        }
    };
    let (flank_a, flank_b) = (flank(ours, theirs), flank(theirs, ours));
    let melee_a = ours.melee_against(theirs.defence) * (1.0 + (FLANK_DAMAGE - 1.0) * flank_a);
    let melee_b = theirs.melee_against(ours.defence) * (1.0 + (FLANK_DAMAGE - 1.0) * flank_b);
    let charge_hit = |a: &Force, b: &Force| {
        if a.front_damage <= 0.0 {
            return 0.0;
        }
        let attack = a.front_damage_attack / a.front_damage;
        a.charge * (50.0 + attack - b.defence).clamp(10.0, 90.0) / 100.0
    };
    let (charge_a, charge_b) = (charge_hit(ours, theirs), charge_hit(theirs, ours));

    let start = |f: &Force| Fighting {
        hp: f.hp,
        routable: f.hp - f.stalwart_hp,
        morale: f.morale,
        broken_at: None,
    };
    let (mut a, mut b) = (start(ours), start(theirs));
    // Regiments that can't rout have nothing to break: they fight to the end.
    if a.routable <= 0.0 {
        a.morale = f32::MAX;
    }
    if b.routable <= 0.0 {
        b.morale = f32::MAX;
    }
    let mut t = 0.0;
    let mut charged = false;
    while t < HORIZON && a.broken_at.is_none() && b.broken_at.is_none() {
        let contact = t >= contact_at;
        let gap_now = (approach.gap - approach.closing * t).max(0.0);
        // Output falls off only once the front ranks thin (losses come off
        // the back), so it holds near full until a block is shallow.
        let vigour = |s: &Fighting, f: &Force| (2.5 * s.hp / f.hp.max(1.0)).min(1.0);
        let (va, vb) = (vigour(&a, ours), vigour(&b, theirs));
        let mut to_b = 0.0;
        let mut to_a = 0.0;
        if gap_now <= ours.range {
            to_b += ours.ranged * va * STEP * (1.0 - theirs.arrows_turned);
        }
        if gap_now <= theirs.range {
            to_a += theirs.ranged * vb * STEP * (1.0 - ours.arrows_turned);
        }
        if contact {
            to_b += melee_a * va * STEP;
            to_a += melee_b * vb * STEP;
            if !charged {
                to_b += charge_a * va;
                to_a += charge_b * vb;
                charged = true;
            }
        }
        let drain = |me: &Fighting, them: &Fighting, flanked: f32, mine: &Force| {
            if !contact {
                return 0.0;
            }
            let mut d = FLANK_SHOCK * flanked.min(1.0);
            // Soldiers standing on each side, from hit points left.
            let ratio = them.hp / me.hp.max(1.0);
            if ratio >= 3.0 {
                d += OUTNUMBERED_3;
            } else if ratio >= 2.0 {
                d += OUTNUMBERED_2;
            }
            if me.hp * 2.0 < mine.hp_full {
                d += HALF_STRENGTH;
            }
            d * STEP
        };
        let (drain_a, drain_b) = (
            drain(&a, &b, flank_b * 2.0, ours),
            drain(&b, &a, flank_a * 2.0, theirs),
        );
        hurt(&mut a, ours, to_a, drain_a, t);
        hurt(&mut b, theirs, to_b, drain_b, t);
        t += STEP;
    }
    let ours_break = a.broken_at.unwrap_or(HORIZON);
    let theirs_break = b.broken_at.unwrap_or(HORIZON);
    let lost = |s: &Fighting, f: &Force| 1.0 - s.hp.max(0.0) / f.hp.max(1.0);
    let (ours_lost, theirs_lost) = (lost(&a, ours), lost(&b, theirs));
    // How decisive: the winner's remaining nerve and strength when the
    // loser breaks.
    let left = |s: &Fighting, f: &Force| {
        let morale = if f.morale > 0.0 && s.routable > 0.0 {
            (s.morale / f.morale).clamp(0.0, 1.0)
        } else {
            1.0
        };
        (morale * (s.hp / f.hp.max(1.0)).clamp(0.0, 1.0)).sqrt()
    };
    let margin = match (a.broken_at, b.broken_at) {
        (None, Some(_)) => left(&a, ours),
        (Some(_), None) => -left(&b, theirs),
        // Both held for the whole horizon (or broke together): whoever lost
        // the smaller share.
        _ => (theirs_lost - ours_lost).clamp(-1.0, 1.0),
    };
    Estimate {
        margin,
        ours_break,
        theirs_break,
        ours_lost,
        theirs_lost,
    }
}

/// Take `damage` hit points and `drain` morale points off a side.
fn hurt(s: &mut Fighting, f: &Force, damage: f32, drain: f32, t: f32) {
    if damage <= 0.0 && drain <= 0.0 {
        return;
    }
    let damage = damage.min(s.hp);
    // Losses spread over the whole side, routable or not.
    let routable_share = if s.hp > 0.0 { s.routable / s.hp } else { 0.0 };
    s.hp -= damage;
    s.routable -= damage * routable_share;
    if s.routable > 0.0 && s.morale != f32::MAX {
        s.morale -= MORALE_PER_PERCENT * 100.0 * damage / f.hp_full.max(1.0) + drain;
        if s.morale <= BREAK_AT {
            // The routable regiments run; whatever never routs fights on.
            s.hp -= s.routable;
            s.routable = 0.0;
            s.morale = f32::MAX;
        }
    }
    if s.hp <= 0.5 {
        s.broken_at = Some(t);
    }
}
