//! Soldier puppets: the presentation layer that turns each regiment block from
//! the simulation into individual soldiers (docs/10-architecture.md, "The
//! two-tier simulation").
//!
//! Every soldier steers toward its formation slot with a little lag and its
//! own pace, so blocks ripple and jostle instead of sliding rigidly. None of
//! this feeds back into the simulation, so it may use floats and frame time.

use gc_sim::RegimentHandle;
use gc_sim::formation::MAX_MODELS;
use glam::Vec2;

const SLOTS: usize = MAX_MODELS as usize;
/// How far a soldier may stand from its exact slot, in tiles, so ranks don't
/// look ruled with a straight edge.
const SLOT_JITTER: f32 = 0.07;
/// Soldiers jog faster than the regiment marches so they can catch up.
const CATCH_UP: f32 = 1.35;
/// Extra speed per tile of distance from the slot, so stragglers close gaps.
const PULL_PER_TILE: f32 = 1.5;
/// Below this distance a soldier is standing in its slot.
const SETTLED: f32 = 0.03;
/// Turn speed for a soldier's facing, radians per second.
const TURN_RATE: f32 = 6.0;
/// How far a soldier in a fighting front rank lunges forward, in tiles, and
/// how many lunges a second.
const LUNGE: f32 = 0.18;
const LUNGE_RATE: f32 = 1.6;
/// Fallen soldiers stay on the field this long, in seconds, then sink away.
const CORPSE_SECONDS: f32 = 20.0;
const CORPSE_SINK_SECONDS: f32 = 3.0;

/// One regiment as the renderer sees it this frame, already interpolated.
#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub handle: RegimentHandle,
    pub centre: Vec2,
    /// Unit vector of the regiment's facing.
    pub facing: Vec2,
    pub files: u16,
    pub models: u16,
    pub file_spacing: f32,
    pub rank_spacing: f32,
    /// Marching speed in tiles per second.
    pub speed: f32,
    /// Fighting in melee: the front ranks lunge at the enemy.
    pub engaged: bool,
}

/// One soldier ready to draw.
#[derive(Clone, Copy, Debug)]
pub struct Soldier {
    pub pos: Vec2,
    pub facing: Vec2,
}

/// A soldier lying where he fell.
#[derive(Clone, Copy, Debug)]
pub struct Corpse {
    pub pos: Vec2,
    pub facing: Vec2,
    pub owner: u8,
    /// How far it has sunk into the ground, in tiles (0 while fresh).
    pub sink: f32,
}

/// Puppet state for every regiment slot in the pool: `MAX_MODELS` soldiers
/// per regiment, by pool index. Allocated once.
pub struct Puppets {
    owner: Vec<Option<RegimentHandle>>,
    /// Soldiers each regiment had last frame, to notice casualties.
    models: Vec<u16>,
    pos: Vec<Vec2>,
    facing: Vec<Vec2>,
    /// Fallen soldiers, a ring buffer; the oldest are reused first.
    corpses: Vec<(Vec2, Vec2, u8, f32)>,
    corpse_capacity: usize,
    next_corpse: usize,
    time: f32,
}

impl Puppets {
    pub fn new(regiment_capacity: usize, corpse_capacity: usize) -> Puppets {
        Puppets {
            owner: vec![None; regiment_capacity],
            models: vec![0; regiment_capacity],
            pos: vec![Vec2::ZERO; regiment_capacity * SLOTS],
            facing: vec![Vec2::NEG_Y; regiment_capacity * SLOTS],
            corpses: Vec::with_capacity(corpse_capacity.max(1)),
            corpse_capacity: corpse_capacity.max(1),
            next_corpse: 0,
            time: 0.0,
        }
    }

    /// Start a new frame `dt` seconds after the last one.
    pub fn advance(&mut self, dt: f32) {
        self.time += dt;
    }

    /// Advance one regiment's soldiers and hand each to `out`. `owner` is the
    /// regiment's player, to colour anyone who falls.
    pub fn update(&mut self, block: &Block, owner: u8, dt: f32, mut out: impl FnMut(Soldier)) {
        let r = block.handle.index as usize;
        let models = block.models.min(MAX_MODELS) as usize;
        let fresh = self.owner[r] != Some(block.handle);
        self.owner[r] = Some(block.handle);
        if !fresh {
            self.fall(block, owner, models);
        }
        self.models[r] = models as u16;
        let right = Vec2::new(-block.facing.y, block.facing.x);
        let back = -block.facing;
        let files = block.files.clamp(1, block.models.max(1));
        for k in 0..models {
            let i = r * SLOTS + k;
            let local = slot_offset(
                k as u16,
                files,
                block.models,
                block.file_spacing,
                block.rank_spacing,
            ) + jitter(block.handle, k) * SLOT_JITTER;
            let target = block.centre + right * local.x + back * local.y;
            if fresh {
                self.pos[i] = target;
                self.facing[i] = block.facing;
            } else {
                let to = target - self.pos[i];
                let dist = to.length();
                let pace = 0.9 + 0.2 * unit_hash(block.handle, k, 1);
                let speed = (block.speed * CATCH_UP + dist * PULL_PER_TILE) * pace;
                if dist > SETTLED {
                    let step = (speed * dt).min(dist);
                    self.pos[i] += to / dist * step;
                }
                // Face where you walk when it's more than a shuffle, else the regiment's way.
                let want = if dist > 0.4 { to / dist } else { block.facing };
                self.facing[i] = turn_toward(self.facing[i], want, TURN_RATE * dt);
            }
            let mut pos = self.pos[i];
            if block.engaged && (k as u16) < files * 2 {
                let phase = unit_hash(block.handle, k, 4);
                let swing = ((self.time * LUNGE_RATE + phase) * core::f32::consts::TAU).sin();
                pos += self.facing[i] * LUNGE * swing.max(0.0);
            }
            out(Soldier {
                pos,
                facing: self.facing[i],
            });
        }
    }

    /// Soldiers lost since last frame fall where they stand. The simulation
    /// takes casualties off the last slots, but on screen the men who fall
    /// are the ones fighting: a front-rank soldier (any soldier, for a block
    /// under arrows) drops, and the soldier from the emptied slot takes his
    /// place and walks up to fill the gap.
    fn fall(&mut self, block: &Block, owner: u8, models: usize) {
        let r = block.handle.index as usize;
        let before = (self.models[r] as usize).min(SLOTS);
        let files = block.files.clamp(1, block.models.max(1)) as usize;
        for gone in (models..before).rev() {
            let pool = if block.engaged {
                files.min(gone + 1)
            } else {
                gone + 1
            };
            let pick = (unit_hash(block.handle, gone, self.time.to_bits()) * pool as f32) as usize;
            let (victim, last) = (r * SLOTS + pick.min(gone), r * SLOTS + gone);
            let corpse = (self.pos[victim], self.facing[victim], owner, self.time);
            if self.corpses.len() < self.corpse_capacity {
                self.corpses.push(corpse);
            } else {
                self.corpses[self.next_corpse] = corpse;
                self.next_corpse = (self.next_corpse + 1) % self.corpse_capacity;
            }
            self.pos[victim] = self.pos[last];
            self.facing[victim] = self.facing[last];
        }
    }

    /// Every fallen soldier still on the field.
    pub fn corpses(&self) -> impl Iterator<Item = Corpse> + '_ {
        self.corpses
            .iter()
            .filter_map(move |&(pos, facing, owner, fell)| {
                let age = self.time - fell;
                let sink = (age - CORPSE_SECONDS) / CORPSE_SINK_SECONDS;
                (sink < 1.0).then_some(Corpse {
                    pos,
                    facing,
                    owner,
                    sink: sink.max(0.0),
                })
            })
    }
}

/// Local slot position, x to the right and y toward the rear, matching
/// `gc_sim::formation::slot_offset` (a partial rear rank is centred).
fn slot_offset(k: u16, files: u16, models: u16, file_spacing: f32, rank_spacing: f32) -> Vec2 {
    let ranks = models.div_ceil(files).max(1);
    let rank = k / files;
    let file = k % files;
    let in_rank = if rank == ranks - 1 {
        models - (ranks - 1) * files
    } else {
        files
    };
    Vec2::new(
        (file as f32 - (in_rank as f32 - 1.0) / 2.0) * file_spacing,
        (rank as f32 - (ranks as f32 - 1.0) / 2.0) * rank_spacing,
    )
}

/// A stable pseudo-random value in [0, 1) per soldier and purpose.
fn unit_hash(h: RegimentHandle, k: usize, salt: u32) -> f32 {
    let mut x = h.index.wrapping_mul(0x9E37_79B9)
        ^ (h.generation as u32).wrapping_mul(0x85EB_CA6B)
        ^ (k as u32).wrapping_mul(0xC2B2_AE35)
        ^ salt.wrapping_mul(0x27D4_EB2F);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

fn jitter(h: RegimentHandle, k: usize) -> Vec2 {
    Vec2::new(
        unit_hash(h, k, 2) * 2.0 - 1.0,
        unit_hash(h, k, 3) * 2.0 - 1.0,
    )
}

fn turn_toward(from: Vec2, to: Vec2, max_angle: f32) -> Vec2 {
    let angle = from.angle_to(to);
    if angle.abs() <= max_angle {
        to
    } else {
        Vec2::from_angle(max_angle.copysign(angle)).rotate(from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_fixed::Fx;

    #[test]
    fn slots_match_the_simulation() {
        let fs = Fx::from_ratio(3, 4);
        let rs = Fx::from_ratio(17, 20);
        for (files, models) in [(8, 24), (6, 12), (7, 20), (24, 24), (5, 1)] {
            for k in 0..models {
                let a = gc_sim::formation::slot_offset(k, files, models, fs, rs);
                let b = slot_offset(k, files.min(models), models, 0.75, 0.85);
                let a = Vec2::new(a.x.raw() as f32, a.y.raw() as f32) / 65536.0;
                assert!(
                    a.distance(b) < 1e-3,
                    "{files}x{models} slot {k}: {a} vs {b}"
                );
            }
        }
    }

    #[test]
    fn soldiers_settle_into_their_slots() {
        let mut p = Puppets::new(4, 16);
        let mut block = Block {
            handle: RegimentHandle {
                index: 2,
                generation: 0,
            },
            centre: Vec2::new(10.0, 10.0),
            facing: Vec2::NEG_Y,
            files: 4,
            models: 8,
            file_spacing: 1.0,
            rank_spacing: 1.0,
            speed: 3.0,
            engaged: false,
        };
        let mut first = Vec::new();
        p.update(&block, 0, 0.016, |s| first.push(s.pos));
        // The block jumps 5 tiles north; soldiers follow and settle.
        block.centre.y -= 5.0;
        let mut last = Vec::new();
        for _ in 0..300 {
            last.clear();
            p.update(&block, 0, 0.016, |s| last.push(s));
        }
        for (a, b) in first.iter().zip(&last) {
            let moved = *a - b.pos;
            assert!((moved - Vec2::new(0.0, 5.0)).length() < 0.05, "{moved}");
            assert!(b.facing.distance(Vec2::NEG_Y) < 1e-3);
        }
    }

    #[test]
    fn casualties_fall_and_the_rest_close_up() {
        let mut p = Puppets::new(1, 3);
        let mut block = Block {
            handle: RegimentHandle {
                index: 0,
                generation: 0,
            },
            centre: Vec2::ZERO,
            facing: Vec2::NEG_Y,
            files: 4,
            models: 8,
            file_spacing: 1.0,
            rank_spacing: 1.0,
            speed: 3.0,
            engaged: true,
        };
        let mut soldiers = Vec::new();
        p.update(&block, 2, 0.016, |s| soldiers.push(s.pos));
        let front: Vec<Vec2> = soldiers[..4].to_vec();
        block.models = 6;
        p.advance(0.016);
        soldiers.clear();
        p.update(&block, 2, 0.016, |s| soldiers.push(s.pos));
        assert_eq!(soldiers.len(), 6);
        let corpses: Vec<Corpse> = p.corpses().collect();
        assert_eq!(corpses.len(), 2);
        for c in &corpses {
            // Engaged: the fallen are front-rank men.
            assert!(front.iter().any(|f| f.distance(c.pos) < 0.3), "{c:?}");
            assert_eq!((c.owner, c.sink), (2, 0.0));
        }

        // The ring buffer keeps the newest; old corpses sink away.
        block.models = 2;
        p.advance(0.016);
        p.update(&block, 2, 0.016, |_| {});
        assert_eq!(p.corpses().count(), 3);
        p.advance(CORPSE_SECONDS + CORPSE_SINK_SECONDS);
        assert_eq!(p.corpses().count(), 0);
    }
}
