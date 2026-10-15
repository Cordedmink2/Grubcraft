//! A Field Battle's deployment phase (docs/06-game-modes.md): before the
//! battle, each player places their regiments inside their own zone, without
//! seeing the enemy, until everyone is ready or time runs out.
//!
//! While it lasts, time stands still: nothing marches, fights or loses
//! heart, and only placing, formations and readiness are accepted.

use gc_fixed::{Fx, FxVec2};

use crate::{PlayerId, StateHasher, TICKS_PER_SECOND};

/// Longest a deployment phase lasts.
pub const DEPLOY_SECONDS: u32 = 90;
/// Players that can have a deployment zone.
pub const MAX_ZONES: usize = 8;

/// Where one player may place their regiments: a circle on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zone {
    pub center: FxVec2,
    pub radius: Fx,
}

impl Zone {
    pub fn contains(&self, p: FxVec2) -> bool {
        let r = self.radius.raw().max(0) as u64;
        (p - self.center).length_sq_raw() <= r * r
    }
}

/// A player taking part in the deployment.
#[derive(Clone, Copy, Debug)]
struct Deployer {
    zone: Zone,
    ready: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Deployment {
    deployers: [Option<Deployer>; MAX_ZONES],
    /// The tick the battle starts on; 0 when there is no deployment.
    end: u32,
}

impl Deployment {
    pub fn active(&self, tick: u32) -> bool {
        tick < self.end
    }

    /// Give `player` a zone, once. The first zone starts the deployment;
    /// more are accepted until the battle begins, so a host's setup that
    /// spans several turns still reaches everyone.
    pub fn set_zone(&mut self, player: PlayerId, zone: Zone, tick: u32) -> bool {
        let started = self.end != 0;
        if (started && !self.active(tick)) || zone.radius <= Fx::ZERO {
            return false;
        }
        match self.deployers.get_mut(player as usize) {
            Some(slot @ None) => *slot = Some(Deployer { zone, ready: false }),
            _ => return false,
        }
        if !started {
            self.end = tick + DEPLOY_SECONDS * TICKS_PER_SECOND;
        }
        true
    }

    fn deployer(&self, player: PlayerId) -> Option<Deployer> {
        self.deployers.get(player as usize).copied().flatten()
    }

    pub fn zone(&self, player: PlayerId) -> Option<Zone> {
        self.deployer(player).map(|d| d.zone)
    }

    pub fn is_ready(&self, player: PlayerId) -> bool {
        self.deployer(player).is_some_and(|d| d.ready)
    }

    /// `player` is done placing. Once every player with a zone is, the
    /// battle starts on the next tick.
    pub fn set_ready(&mut self, player: PlayerId, tick: u32) -> bool {
        if !self.active(tick) {
            return false;
        }
        match self.deployers.get_mut(player as usize) {
            Some(Some(d)) if !d.ready => d.ready = true,
            _ => return false,
        }
        if self.deployers.iter().flatten().all(|d| d.ready) {
            self.end = tick + 1;
        }
        true
    }

    /// Ticks since the battle started.
    pub fn battle_ticks(&self, tick: u32) -> u32 {
        tick.saturating_sub(self.end)
    }

    /// Ticks left until the battle starts.
    pub fn ticks_left(&self, tick: u32) -> u32 {
        self.end.saturating_sub(tick)
    }

    /// Nothing at all when there was never a deployment, so worlds without
    /// one keep their hashes.
    pub fn hash_into(&self, h: &mut StateHasher) {
        if self.end == 0 {
            return;
        }
        h.u32(self.end);
        for d in &self.deployers {
            match d {
                Some(d) => {
                    h.u8(1 + u8::from(d.ready));
                    h.i32(d.zone.center.x.raw());
                    h.i32(d.zone.center.y.raw());
                    h.i32(d.zone.radius.raw());
                }
                None => h.u8(0),
            }
        }
    }
}
