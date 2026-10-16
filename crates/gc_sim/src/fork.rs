//! Copies of a battle (engine hooks 5 and 6 in docs/18-ai.md): a scratch
//! world on the same map, a regiment put on the field just as it stands
//! mid-battle, and a copy of every regiment in one area. The battle AI
//! rehearses its plans in scratch worlds; "Restage this battle" and save
//! games will use the same pieces.
//!
//! A scratch world shares the match's map (the walkable grid and its portal
//! graph, and the ground's heights, built once when the match starts) instead of building it again, so
//! making one costs little more than its regiment pool. Nothing here runs
//! inside [`World::tick`], so it may allocate.

use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2, Pcg32};

use crate::combat::Combat;
use crate::formation::MAX_MODELS;
use crate::morale::{self, MoraleState};
use crate::regiments::{RegimentHandle, RegimentView, Regiments};
use crate::vision::Vision;
use crate::{Formation, PlayerId, UnitId, World, streams};

/// A regiment as it stands mid-battle: what [`World::spawn_standing`] puts on
/// the field. Everything here is something a player can see of a regiment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Standing {
    pub owner: PlayerId,
    pub unit: UnitId,
    /// Centre of the block.
    pub position: FxVec2,
    pub facing: Angle,
    /// Soldiers per rank (kept between 1 and `models`).
    pub files: u16,
    /// Line unless the unit can form this one.
    pub formation: Formation,
    /// Soldiers standing, each at full hit points (at most the unit's full
    /// strength; zero puts nothing on the field).
    pub models: u16,
    /// Morale in points, 0 to 100 (ignored for units that never rout), and
    /// the state it is in.
    pub morale_points: u8,
    pub morale: MoraleState,
    /// Hidden from every enemy: standing in cover, it starts unseen by
    /// them. Otherwise everyone has seen it for a while, so it is no
    /// surprise to anyone.
    pub hidden: bool,
}

impl Standing {
    /// How the regiment `view` shows stands, `hidden` or not.
    pub fn of(view: &RegimentView, hidden: bool) -> Standing {
        Standing {
            owner: view.owner,
            unit: view.unit,
            position: view.position,
            facing: view.facing,
            files: view.files,
            formation: view.formation,
            models: view.models,
            morale_points: view.morale_points,
            morale: view.morale,
            hidden,
        }
    }
}

impl World {
    /// An empty world on the same map as this one, sharing its walkable
    /// grid, with the same regiment types and stances (and no champions or
    /// deployment),
    /// its clock at this
    /// world's tick (so staggered checks fall on the same ticks), room for
    /// `capacity` regiments and its own random streams from `seed`.
    pub fn scratch(&self, capacity: u32, seed: u64) -> World {
        let n = capacity.max(1);
        World {
            tick: self.tick,
            rng: Pcg32::new(seed, streams::WORLD),
            combat_rng: Pcg32::new(seed, streams::COMBAT),
            combat: Combat::new(n as usize),
            units: self.units.clone(),
            map_tiles: self.map_tiles,
            nav: self.nav.sharing(n as usize),
            stances: self.stances.clone(),
            terrain: self.terrain.clone(),
            vision: Vision::new(n as usize, &self.terrain, &self.stances),
            // A rehearsal is all battle.
            deploy: crate::deploy::Deployment::default(),
            champions: None,
            regiments: Regiments::with_capacity(n),
        }
    }

    /// Give this world new random streams from `seed`.
    pub fn reseed(&mut self, seed: u64) {
        self.rng = Pcg32::new(seed, streams::WORLD);
        self.combat_rng = Pcg32::new(seed, streams::COMBAT);
    }

    /// Put a regiment on the field as `s` describes it, mid-battle (engine
    /// hook 6). With `slot`, it takes that handle (its slot must be free),
    /// so it can be found by the handle it has in another world. It stands
    /// with no orders; a routing or shattered regiment counts as having just
    /// broken; one not `hidden` has been seen by everyone for a while. Returns `None` if the unit is unknown, there are no soldiers,
    /// or there is no room (or `slot` is taken).
    pub fn spawn_standing(
        &mut self,
        s: &Standing,
        slot: Option<RegimentHandle>,
    ) -> Option<RegimentHandle> {
        let stats = *self.units.get(s.unit as usize)?;
        if s.models == 0 {
            return None;
        }
        if let Some(h) = slot
            && !self.regiments.claim(h)
        {
            return None;
        }
        let h = self
            .regiments
            .spawn(s.owner, s.unit, s.position, s.facing, &stats)?;
        let r = &mut self.regiments;
        let i = h.index as usize;
        let models = s.models.min(stats.models);
        r.models[i] = models;
        r.files[i] = s.files.clamp(1, models);
        if stats.can_form(s.formation) {
            r.formation[i] = s.formation;
        }
        let m = MAX_MODELS as usize;
        r.hp[i * m..][models as usize..m].fill(0);
        if stats.morale > 0 {
            let points = i16::from(s.morale_points).min(i16::from(stats.morale));
            r.morale[i] = points * morale::SCALE;
        }
        r.morale_state[i] = s.morale;
        if s.morale.broken() {
            r.last_rout[i] = self.tick + 1;
        }
        self.vision
            .start(&self.regiments, &self.units, &self.terrain, i);
        if !s.hidden {
            self.vision.show(i);
        }
        Some(h)
    }

    /// A copy of this world holding only the regiments whose centres are
    /// within `radius` of `centre` (engine hook 5), each exactly as it is
    /// here, with the same handle, orders and route, on the same map and at
    /// the same tick, with the same random streams. Orders aimed at
    /// regiments left out lapse in the copy. Arrows already in the air and
    /// a duel of champions under way are left out (what the last duel did
    /// to each side's heart is kept), and later volleys may land in a
    /// different order than in this world, so a copy where archers shoot
    /// plays on alike but not identically.
    pub fn fork_region(&self, centre: FxVec2, radius: Fx) -> World {
        let near = |p: FxVec2| {
            let dx = i64::from((p.x - centre.x).raw());
            let dy = i64::from((p.y - centre.y).raw());
            let r = i64::from(radius.raw());
            dx * dx + dy * dy <= r * r
        };
        let regs = &self.regiments;
        let picked: Vec<usize> = (0..regs.alive.len())
            .filter(|&i| regs.alive[i] && near(regs.position[i]))
            .collect();
        let capacity = picked.last().map_or(1, |&i| i as u32 + 1);
        let mut w = self.scratch(capacity, 0);
        w.rng = self.rng.clone();
        w.combat_rng = self.combat_rng.clone();
        w.regiments.party_morale.clone_from(&regs.party_morale);
        let mut copied = alloc::vec![false; capacity as usize];
        for &i in &picked {
            if w.regiments.copy_from(regs, i) {
                w.nav.copy_route(&self.nav, i);
                w.vision.copy_from(&self.vision, i);
                copied[i] = true;
            }
        }
        w.nav
            .copy_queue(&self.nav, |i| copied.get(i).copied().unwrap_or(false));
        w
    }
}
