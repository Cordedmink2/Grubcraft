//! Gates open and shut (docs/26-fortifications.md, stage 2).
//!
//! A gate ([`GATE`]) is a wall its owner's side walks through: in the
//! walking grid its cells are a *door* (see [`Nav::set_door`]), open ground
//! to the sides it lets through and wall to the rest, so enemy routes treat
//! a shut gate as wall and friendly routes run through it. Its owner sets
//! it ([`crate::Command::SetGate`]) to:
//! - **Auto** (the default): it opens as its side's regiments march up to
//!   it, within [`GATE_REACH`] of it with somewhere to go, or stand in the
//!   gateway, and shuts [`GATE_HOLD`] after the last has gone. Open, it
//!   lets anyone through: a rout back through the gate can carry the
//!   pursuers in with it.
//! - **Locked**: shut to everyone, its own side too.
//! - **Open**: held open to everyone, for a sally.
//!
//! A gate going up is a wall to everyone until it is finished.

use crate::nav::{Door, Nav};
use crate::regiments::Regiments;
use crate::stances::Stances;
use crate::structures::{GATE, Structures};
use crate::veins::within;
use crate::{StateHasher, TICKS_PER_SECOND};

/// How a gate is set (see the module docs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum GateMode {
    #[default]
    Auto = 0,
    Locked = 1,
    Open = 2,
}

impl GateMode {
    pub const ALL: [GateMode; 3] = [GateMode::Auto, GateMode::Locked, GateMode::Open];

    pub fn from_u8(v: u8) -> Option<GateMode> {
        GateMode::ALL.get(usize::from(v)).copied()
    }
}

/// How near its walls, in tiles, a marching regiment of its side opens an
/// Auto gate.
pub const GATE_REACH: i32 = 4;
/// How near its walls, in tiles, a regiment of its side stands in the
/// gateway: the gate stays open over it, whatever it is doing.
pub const GATEWAY: i32 = 1;
/// Ticks an Auto gate stays open after the last regiment that opened it
/// has gone (2 s), so it doesn't swing open and shut with every step.
pub const GATE_HOLD: u16 = 2 * TICKS_PER_SECOND as u16;

/// A door shut to everyone, and one open to everyone.
const SHUT: Door = Door {
    own: false,
    others: false,
};
const OPEN: Door = Door {
    own: true,
    others: true,
};

impl Structures {
    /// Whether building slot `i` is a gate.
    pub(crate) fn is_gate(&self, i: usize) -> bool {
        self.flags_of(i) & GATE != 0
    }

    /// Set building slot `i`'s gate (the caller has checked it is one).
    pub(crate) fn set_gate(&mut self, i: usize, mode: GateMode) {
        self.gate[i] = mode;
    }

    /// Open and shut every finished gate as its setting and the regiments
    /// about it say, bringing the walking grid up to date for each that
    /// changed.
    pub(crate) fn work_gates(&mut self, regs: &Regiments, nav: &mut Nav, stances: &Stances) {
        for i in 0..self.gate_door.len() {
            if !self.stands(i) || !self.is_gate(i) || !self.finished(i) {
                continue;
            }
            let now = match self.gate[i] {
                GateMode::Locked => SHUT,
                GateMode::Open => OPEN,
                GateMode::Auto => {
                    let foot = self.footprint(i);
                    let owner = self.owner_of(i);
                    let called = (0..regs.alive.len()).any(|r| {
                        regs.alive[r]
                            && !stances.hostile(owner, regs.owner[r])
                            && (within(foot, regs.position[r], GATEWAY)
                                || regs.target[r].is_some()
                                    && within(foot, regs.position[r], GATE_REACH))
                    });
                    if called {
                        self.gate_hold[i] = GATE_HOLD;
                    } else {
                        self.gate_hold[i] = self.gate_hold[i].saturating_sub(1);
                    }
                    Door {
                        own: true,
                        others: self.gate_hold[i] > 0,
                    }
                }
            };
            self.swing(i, now, nav, regs);
        }
    }

    /// Shut building slot `i`'s door to everyone (it is coming down).
    pub(crate) fn shut_door(&mut self, i: usize, nav: &mut Nav, regs: &Regiments) {
        self.swing(i, SHUT, nav, regs);
    }

    /// Set building slot `i`'s door to `now`, bringing the walking grid up
    /// to date if that changes it.
    fn swing(&mut self, i: usize, now: Door, nav: &mut Nav, regs: &Regiments) {
        if now != self.gate_door[i] {
            nav.set_door(
                self.footprint(i),
                self.owner_of(i),
                self.gate_door[i],
                now,
                regs,
            );
            self.gate_door[i] = now;
        }
    }

    pub(crate) fn hash_gate(&self, i: usize, h: &mut StateHasher) {
        h.u8(self.gate[i] as u8);
        h.u8(u8::from(self.gate_door[i].own) | u8::from(self.gate_door[i].others) << 1);
        h.u16(self.gate_hold[i]);
    }
}
