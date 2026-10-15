//! Hiding, spotting and springing ambushes (docs/20-ambushes.md).
//!
//! On a map with cover ([`crate::terrain::Cover`]), every tick works out
//! which parties see each regiment:
//! - its own side and its allies always do;
//! - in the open, in melee, or within [`VOLLEY_REVEAL_TICKS`] of loosing a
//!   volley, everyone does;
//! - with most of its block (four of five points: the centre and the
//!   corners) in a wood, it is seen only by parties with a regiment within
//!   [`WOOD_STILL`] tiles of it, edge to edge ([`WOOD_MOVING`] while it
//!   marches); in tall grass, standing still, within [`GRASS_STILL`];
//! - once seen, it stays seen for [`LINGER_TICKS`] after the last reason.
//!
//! A regiment that makes melee contact with an enemy whose side first saw
//! it less than [`SURPRISE_TICKS`] before springs an **ambush**: the victim
//! loses [`AMBUSH_SHOCK`] morale, is caught off guard for
//! [`OFF_GUARD_TICKS`] (its blows deal [`OFF_GUARD_DAMAGE_PERCENT`]), and
//! its friends within [`RIPPLE_TILES`] lose [`AMBUSH_RIPPLE`]. An ordered
//! volley from hiding costs its target [`VOLLEY_SHOCK`]. Each hidden
//! regiment springs one ambush: after it, the victim's side has it fully in
//! view.
//!
//! Combat asks [`Vision::sees`] before an archer picks a target, holds a
//! hidden archer's fire, and lists the volleys loosed each tick for the
//! volley check; morale (being outnumbered, a router's flight and rally)
//! counts only seen enemies; an attack order needs a seen target, and a
//! chase after a target that goes out of sight becomes a march to where it
//! was last seen.
//!
//! On a map with no cover all of this is switched off: everyone sees
//! everything, nothing is a surprise, nothing is stored or hashed.
//!
//! Sightings are kept for eight parties, the combat grid's owner slots.
//! Parties numbered 8 and up (none exist yet) see everything and are seen
//! by everyone.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::Fx;

use crate::combat::{Block, Combat, Whose, placement};
use crate::morale;
use crate::regiments::{RegimentHandle, Regiments};
use crate::terrain::{Cover, Terrain};
use crate::{PlayerId, Stances, StateHasher, TICKS_PER_SECOND, UnitStats};

/// Parties whose sight is tracked.
pub const PARTIES: usize = 8;
/// Spotting distances, edge to edge, in tiles.
pub const GRASS_STILL: Fx = Fx::from_int(3);
pub const WOOD_STILL: Fx = Fx::from_int(4);
pub const WOOD_MOVING: Fx = Fx::from_int(10);
/// How long a regiment stays seen after the last reason to see it.
pub const LINGER_TICKS: u8 = 3 * TICKS_PER_SECOND as u8;
/// How long loosing a volley keeps a regiment seen by everyone.
pub const VOLLEY_REVEAL_TICKS: u8 = 5 * TICKS_PER_SECOND as u8;
/// Contact made less than this long after the victim's side first saw the
/// attacker springs an ambush.
pub const SURPRISE_TICKS: u16 = 4 * TICKS_PER_SECOND as u16;
/// Morale points the victim of an ambush loses at once.
pub const AMBUSH_SHOCK: i16 = 20;
/// Morale points the victim's friends within [`RIPPLE_TILES`] lose.
pub const AMBUSH_RIPPLE: i16 = 5;
pub const RIPPLE_TILES: i32 = 16;
/// Morale points the target of an ordered volley from hiding loses.
pub const VOLLEY_SHOCK: i16 = 10;
/// How long an ambush's victim is caught off guard, and the damage its
/// blows deal meanwhile, percent.
pub const OFF_GUARD_TICKS: u8 = 6 * TICKS_PER_SECOND as u8;
pub const OFF_GUARD_DAMAGE_PERCENT: u64 = 70;
/// Ambushes remembered per tick, for the game's alerts.
pub const MAX_AMBUSHES: usize = 64;

/// An ambush sprung this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ambush {
    pub ambusher: RegimentHandle,
    pub victim: RegimentHandle,
    /// Sprung with a volley rather than a charge.
    pub volley: bool,
}

/// Who sees whom. All storage is reserved when the match starts.
pub(crate) struct Vision {
    /// The map has cover; otherwise none of this runs.
    active: bool,
    /// Per party: the parties that share its sight (itself and its allies).
    allies: [u8; PARTIES],
    /// Per slot: the generation of the regiment the state below belongs
    /// to, if any (a new one in the slot starts afresh).
    tracked: Vec<Option<u16>>,
    /// Per slot and party: ticks the party has had the regiment in view,
    /// counting up to [`SURPRISE_TICKS`]; 0 is unseen.
    sighting: Vec<u16>,
    /// Per slot and party: ticks a sighting lingers after the last reason.
    linger: Vec<u8>,
    /// Per slot: ticks a volley keeps it seen by everyone.
    revealed: Vec<u8>,
    /// Per slot: ticks it is still caught off guard.
    off_guard: Vec<u8>,
    /// Per slot: in melee at the end of the last tick.
    was_engaged: Vec<bool>,
    /// Ambushes sprung this tick.
    ambushes: Vec<Ambush>,
}

impl Vision {
    pub(crate) fn new(capacity: usize, terrain: &Terrain, stances: &Stances) -> Vision {
        let active = terrain.has_cover();
        let n = if active { capacity } else { 0 };
        let mut allies = [0u8; PARTIES];
        for (p, mask) in allies.iter_mut().enumerate() {
            for q in 0..PARTIES {
                if !stances.hostile(p as PlayerId, q as PlayerId) {
                    *mask |= 1 << q;
                }
            }
        }
        Vision {
            active,
            allies,
            tracked: vec![None; n],
            sighting: vec![0; n * PARTIES],
            linger: vec![0; n * PARTIES],
            revealed: vec![0; n],
            off_guard: vec![0; n],
            was_engaged: vec![false; n],
            ambushes: Vec::with_capacity(if active { MAX_AMBUSHES } else { 0 }),
        }
    }

    /// Whether `party` sees the regiment in slot `i`, owned by `owner`.
    pub(crate) fn sees(&self, party: PlayerId, i: usize, owner: PlayerId) -> bool {
        if !self.active || party as usize >= PARTIES || owner as usize >= PARTIES {
            return true;
        }
        if self.allies[owner as usize] & (1 << party) != 0 {
            return true;
        }
        self.sighting[i * PARTIES + party as usize] > 0
    }

    /// Whether no party hostile to its owner sees the regiment in slot `i`.
    pub(crate) fn hidden(&self, i: usize, owner: PlayerId) -> bool {
        if !self.active || owner as usize >= PARTIES {
            return false;
        }
        let friends = self.allies[owner as usize];
        (0..PARTIES).all(|p| friends & (1 << p) != 0 || self.sighting[i * PARTIES + p] == 0)
    }

    /// Whether the regiment in slot `i` is caught off guard by an ambush.
    pub(crate) fn off_guard(&self, i: usize) -> bool {
        self.active && self.off_guard[i] > 0
    }

    /// Ambushes sprung in the last tick.
    pub fn ambushes(&self) -> &[Ambush] {
        &self.ambushes
    }

    /// Work out who sees whom after this tick's moves and blows, spring
    /// any ambushes, and end chases after targets gone out of sight.
    pub(crate) fn update(
        &mut self,
        regs: &mut Regiments,
        combat: &Combat,
        units: &[UnitStats],
        stances: &Stances,
        terrain: &Terrain,
    ) {
        if !self.active {
            return;
        }
        self.ambushes.clear();
        // Timers, and regiments new to their slots.
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                self.tracked[i] = None;
                continue;
            }
            if self.tracked[i] != Some(regs.generation[i]) {
                self.start(regs, units, terrain, i);
            }
            self.revealed[i] = self.revealed[i].saturating_sub(1);
            self.off_guard[i] = self.off_guard[i].saturating_sub(1);
        }

        // Ambushes, judged on what each side saw before this tick: volleys
        // loosed from hiding, then charges out of it.
        for &(shooter, target) in combat.loosed() {
            let Some(i) = regs.resolve(shooter) else {
                continue;
            };
            self.revealed[i] = VOLLEY_REVEAL_TICKS;
            if let Some(j) = regs.resolve(target)
                && self.surprises(i, regs.owner[j])
            {
                self.spring(regs, combat, stances, i, j, true);
            }
        }
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            let engaged = regs.engaged[i].and_then(|h| regs.resolve(h));
            if let Some(j) = engaged
                && !self.was_engaged[i]
                && self.surprises(i, regs.owner[j])
            {
                self.spring(regs, combat, stances, i, j, false);
            }
            self.was_engaged[i] = engaged.is_some();
        }

        // Who sees whom now.
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            let owner = regs.owner[i];
            let direct = match self.spotting(regs, units, terrain, i) {
                None => u8::MAX,
                Some(_) if owner as usize >= PARTIES => u8::MAX,
                Some(distance) => {
                    self.allies[owner as usize]
                        | self.spotters(regs, combat, units, stances, i, distance)
                }
            };
            for p in 0..PARTIES {
                let k = i * PARTIES + p;
                if direct & (1 << p) != 0 {
                    self.linger[k] = LINGER_TICKS;
                } else if self.linger[k] > 0 {
                    self.linger[k] -= 1;
                } else {
                    self.sighting[k] = 0;
                    continue;
                }
                self.sighting[k] = (self.sighting[k] + 1).min(SURPRISE_TICKS);
            }
        }

        // A chase after a target its side no longer sees ends where the
        // target was last seen.
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            let Some(j) = regs.attack_target[i].and_then(|h| regs.resolve(h)) else {
                continue;
            };
            if self.sees(regs.owner[i], j, regs.owner[j]) {
                continue;
            }
            regs.attack_target[i] = None;
            if regs.target[i].is_some() {
                regs.target[i] = Some(regs.position[j]);
            }
        }
    }

    /// Start tracking the regiment just placed in slot `i`: seen by
    /// everyone if it stands in the open (so a regiment placed in plain
    /// view is no surprise), by its own side only if it starts in cover.
    pub(crate) fn start(
        &mut self,
        regs: &Regiments,
        units: &[UnitStats],
        terrain: &Terrain,
        i: usize,
    ) {
        if !self.active {
            return;
        }
        self.tracked[i] = Some(regs.generation[i]);
        self.revealed[i] = 0;
        self.off_guard[i] = 0;
        self.was_engaged[i] = false;
        let in_open = self.spotting(regs, units, terrain, i).is_none();
        let friends = self
            .allies
            .get(regs.owner[i] as usize)
            .copied()
            .unwrap_or(u8::MAX);
        for p in 0..PARTIES {
            let k = i * PARTIES + p;
            self.sighting[k] = if in_open || friends & (1 << p) != 0 {
                SURPRISE_TICKS
            } else {
                0
            };
            self.linger[k] = 0;
        }
    }

    /// Whether the regiment in slot `i` takes `party` by surprise: that
    /// party first saw it less than [`SURPRISE_TICKS`] ago.
    fn surprises(&self, i: usize, party: PlayerId) -> bool {
        party < PARTIES as u8 && self.sighting[i * PARTIES + party as usize] < SURPRISE_TICKS
    }

    /// Regiment `i` springs an ambush on regiment `j`.
    fn spring(
        &mut self,
        regs: &mut Regiments,
        combat: &Combat,
        stances: &Stances,
        i: usize,
        j: usize,
        volley: bool,
    ) {
        if volley {
            morale::shake(regs, j, VOLLEY_SHOCK);
        } else {
            morale::shake(regs, j, AMBUSH_SHOCK);
            self.off_guard[j] = OFF_GUARD_TICKS;
            morale::shake_friends(regs, combat, stances, j, RIPPLE_TILES, AMBUSH_RIPPLE);
        }
        if self.ambushes.len() < MAX_AMBUSHES {
            self.ambushes.push(Ambush {
                ambusher: handle(regs, i),
                victim: handle(regs, j),
                volley,
            });
        }
        // Sprung: the victim's side knows all about it now, so it springs
        // one ambush, not one per blow or volley.
        let friends = self.allies[regs.owner[j] as usize];
        for p in 0..PARTIES {
            if friends & (1 << p) != 0 {
                self.sighting[i * PARTIES + p] = SURPRISE_TICKS;
                self.linger[i * PARTIES + p] = LINGER_TICKS;
            }
        }
    }

    /// How close, edge to edge, an enemy must come to see regiment `i`, or
    /// `None` when everyone sees it: it stands in the open, fights, or has
    /// just loosed a volley.
    fn spotting(
        &self,
        regs: &Regiments,
        units: &[UnitStats],
        terrain: &Terrain,
        i: usize,
    ) -> Option<Fx> {
        if regs.engaged[i].is_some() || self.revealed[i] > 0 {
            return None;
        }
        let b = Block::of(regs, units, i);
        let (c, across, along) = (b.centre, b.right * b.half_width, b.forward * b.half_depth);
        let (mut wood, mut covered) = (0, 0);
        for p in [
            c,
            c + across + along,
            c + across - along,
            c - across + along,
            c - across - along,
        ] {
            match terrain.cover(p) {
                Cover::Wood => {
                    wood += 1;
                    covered += 1;
                }
                Cover::Grass => covered += 1,
                Cover::Open => {}
            }
        }
        let moving = regs.target[i].is_some();
        if wood >= 4 {
            Some(if moving { WOOD_MOVING } else { WOOD_STILL })
        } else if covered >= 4 && !moving {
            Some(GRASS_STILL)
        } else {
            None
        }
    }

    /// The parties with a regiment (theirs or an ally's) within `distance`
    /// of regiment `i`, edge to edge, as a bit set.
    fn spotters(
        &self,
        regs: &Regiments,
        combat: &Combat,
        units: &[UnitStats],
        stances: &Stances,
        i: usize,
        distance: Fx,
    ) -> u8 {
        let at = placement!(regs, stances);
        let me = Block::of(regs, units, i);
        let mut seen = 0u8;
        combat.each_near(
            &at,
            me.centre,
            distance + me.radius() + combat.max_radius(),
            Whose::Enemies(regs.owner[i]),
            |j| {
                let party = regs.owner[j] as usize;
                if party < PARTIES
                    && seen & (1 << party) == 0
                    && me.touches(&Block::of(regs, units, j), distance)
                {
                    seen |= self.allies[party];
                }
            },
        );
        seen
    }

    pub(crate) fn hash_into(&self, regs: &Regiments, h: &mut StateHasher) {
        if !self.active {
            return;
        }
        for i in 0..regs.alive.len() {
            if !regs.alive[i] || self.tracked[i].is_none() {
                continue;
            }
            for k in i * PARTIES..(i + 1) * PARTIES {
                h.u16(self.sighting[k]);
                h.u8(self.linger[k]);
            }
            h.u8(self.revealed[i]);
            h.u8(self.off_guard[i]);
            h.u8(self.was_engaged[i] as u8);
        }
    }
}

fn handle(regs: &Regiments, i: usize) -> RegimentHandle {
    RegimentHandle {
        index: i as u32,
        generation: regs.generation[i],
    }
}
