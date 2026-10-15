//! Soldier puppets: the presentation layer that turns each regiment block from
//! the simulation into individual soldiers (docs/10-architecture.md, "The
//! two-tier simulation").
//!
//! Every soldier steers toward its formation slot with a little lag and its
//! own pace, so blocks ripple and jostle instead of sliding rigidly. Soldiers
//! speed up and slow down rather than snapping to full pace, the front rank
//! sets off before the rear, a halting block overshoots a touch and settles,
//! and a regiment at rest sways and looks about. In melee each front-rank
//! soldier pairs off with the nearest enemy across the line and the two trade
//! blows (docs/09-art-and-audio.md, "Animation"). None of this feeds back
//! into the simulation, so it may use floats and frame time.

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
/// How quickly a front-rank soldier gains or sheds speed, in tiles per second
/// squared; each rank further back is slower to react, so a block stretches a
/// little as it sets off and bunches as it halts.
const ACCEL: f32 = 10.0;
const ACCEL_FALLOFF_PER_RANK: f32 = 0.3;
/// Near his slot a soldier is pulled in like a spring that is a little short
/// of fully damped (natural frequency in radians per second, and damping
/// ratio), which gives the small overshoot and settle.
const SPRING: f32 = 4.0;
const DAMPING: f32 = 0.75;
/// How much of the block's own pace a soldier matches outright; the rest he
/// makes up by trailing his slot slightly, which softens the halt.
const LEAD: f32 = 0.6;
/// At rest: side-to-side sway in tiles, and how far a soldier glances aside,
/// in radians.
const SWAY: f32 = 0.025;
const GLANCE: f32 = 0.22;
/// Duel pairs: a front-rank soldier looks for an enemy within this reach, in
/// tiles, and keeps him until he is this far away.
const DUEL_REACH: f32 = 2.6;
const DUEL_KEEP: f32 = 3.4;
/// Paired soldiers close to this gap, in tiles, but stray no further than
/// the leash from their own slot.
const DUEL_GAP: f32 = 0.85;
const DUEL_LEASH: f32 = 0.6;
/// The step in and back as a pair trades blows, in tiles, and its rate.
const DUEL_STEP: f32 = 0.16;
const DUEL_RATE: f32 = 0.8;
/// Regiments this far apart, centre to centre, are never paired.
const DUEL_REGIMENT_RANGE: f32 = 24.0;
/// A regiment not updated for this long, in seconds, is gone.
const STALE: f32 = 0.5;
/// No partner.
const NONE: u32 = u32::MAX;
/// A routing regiment's soldiers spread this much further apart (as a
/// fraction of their spacing) and this much further off their slots, in tiles.
const ROUT_SPREAD: f32 = 0.8;
const ROUT_JITTER: f32 = 0.45;
/// Fallen soldiers stay on the field this long, in seconds, then sink away.
const CORPSE_SECONDS: f32 = 20.0;
const CORPSE_SINK_SECONDS: f32 = 3.0;

/// One regiment as the renderer sees it this frame, already interpolated.
#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub handle: RegimentHandle,
    /// Unit type, so the fallen keep their model.
    pub unit: u16,
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
    /// Running away: the ranks come apart.
    pub routing: bool,
}

/// One soldier ready to draw.
#[derive(Clone, Copy, Debug)]
pub struct Soldier {
    pub pos: Vec2,
    pub facing: Vec2,
    /// How fast he moved this frame, in tiles per second.
    pub speed: f32,
    /// In a fighting front rank, striking at the enemy.
    pub striking: bool,
    /// Paired with an enemy soldier and trading blows with him.
    pub dueling: bool,
    /// A stable value in [0, 1) per soldier, to keep animations out of step.
    pub phase: f32,
}

/// A soldier lying where he fell.
#[derive(Clone, Copy, Debug)]
pub struct Corpse {
    pub pos: Vec2,
    pub facing: Vec2,
    pub owner: u8,
    pub unit: u16,
    /// Seconds since he fell.
    pub age: f32,
    /// How far it has sunk into the ground, in tiles (0 while fresh).
    pub sink: f32,
}

/// Puppet state for every regiment slot in the pool: `MAX_MODELS` soldiers
/// per regiment, by pool index. Allocated once.
pub struct Puppets {
    owner: Vec<Option<RegimentHandle>>,
    /// Soldiers each regiment had last frame, to notice casualties.
    models: Vec<u16>,
    /// Last frame's centre, player, fighting state and files, for pairing.
    centre: Vec<Vec2>,
    /// Each regiment's own velocity, smoothed, so soldiers keep pace with
    /// a marching block instead of trailing it.
    drift: Vec<Vec2>,
    player: Vec<u8>,
    engaged: Vec<bool>,
    files: Vec<u16>,
    /// When each regiment was last updated, to skip ones that are gone.
    seen: Vec<f32>,
    pos: Vec<Vec2>,
    vel: Vec<Vec2>,
    facing: Vec<Vec2>,
    /// Each soldier's duel partner, a puppet index, or `NONE`.
    partner: Vec<u32>,
    /// Scratch for pairing: regiments in melee, and the enemies near one.
    fighting: Vec<u32>,
    near: Vec<u32>,
    /// Fallen soldiers, a ring buffer; the oldest are reused first.
    corpses: Vec<(Vec2, Vec2, u8, u16, f32)>,
    corpse_capacity: usize,
    next_corpse: usize,
    time: f32,
}

impl Puppets {
    pub fn new(regiment_capacity: usize, corpse_capacity: usize) -> Puppets {
        Puppets {
            owner: vec![None; regiment_capacity],
            models: vec![0; regiment_capacity],
            centre: vec![Vec2::ZERO; regiment_capacity],
            drift: vec![Vec2::ZERO; regiment_capacity],
            player: vec![0; regiment_capacity],
            engaged: vec![false; regiment_capacity],
            files: vec![1; regiment_capacity],
            seen: vec![f32::NEG_INFINITY; regiment_capacity],
            pos: vec![Vec2::ZERO; regiment_capacity * SLOTS],
            vel: vec![Vec2::ZERO; regiment_capacity * SLOTS],
            facing: vec![Vec2::NEG_Y; regiment_capacity * SLOTS],
            partner: vec![NONE; regiment_capacity * SLOTS],
            fighting: Vec::with_capacity(regiment_capacity),
            near: Vec::with_capacity(regiment_capacity),
            corpses: Vec::with_capacity(corpse_capacity.max(1)),
            corpse_capacity: corpse_capacity.max(1),
            next_corpse: 0,
            time: 0.0,
        }
    }

    /// Game seconds since the puppets started (stops while paused).
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Start a new frame `dt` seconds after the last one.
    pub fn advance(&mut self, dt: f32) {
        self.time += dt;
    }

    /// Pair front-rank soldiers of fighting regiments with the nearest enemy
    /// across the line, from last frame's positions. A soldier keeps his
    /// partner while the partner stands within reach; several may pick the
    /// same man. `hostile` says whether two players fight. Call once a frame,
    /// before the updates.
    pub fn pair(&mut self, hostile: impl Fn(u8, u8) -> bool) {
        let mut fighting = core::mem::take(&mut self.fighting);
        let mut near = core::mem::take(&mut self.near);
        fighting.clear();
        for r in 0..self.owner.len() {
            if self.live(r) && self.engaged[r] {
                fighting.push(r as u32);
            } else {
                self.partner[r * SLOTS..(r + 1) * SLOTS].fill(NONE);
            }
        }
        for &a in &fighting {
            let a = a as usize;
            near.clear();
            near.extend(fighting.iter().copied().filter(|&b| {
                let b = b as usize;
                b != a
                    && hostile(self.player[a], self.player[b])
                    && self.centre[a].distance_squared(self.centre[b])
                        <= DUEL_REGIMENT_RANGE * DUEL_REGIMENT_RANGE
            }));
            let front = self.front(a);
            for k in 0..SLOTS {
                let i = a * SLOTS + k;
                if k >= front || near.is_empty() {
                    self.partner[i] = NONE;
                    continue;
                }
                let me = self.pos[i];
                let j = self.partner[i];
                if j != NONE && self.can_duel(a, j as usize, me, &near) {
                    continue;
                }
                let mut best = (NONE, DUEL_REACH * DUEL_REACH);
                for &b in &near {
                    let b = b as usize;
                    for j in b * SLOTS..b * SLOTS + self.front(b) {
                        let d = me.distance_squared(self.pos[j]);
                        if d < best.1 {
                            best = (j as u32, d);
                        }
                    }
                }
                self.partner[i] = best.0;
            }
        }
        self.fighting = fighting;
        self.near = near;
    }

    /// Whether regiment `r` was updated recently and is still in play.
    fn live(&self, r: usize) -> bool {
        self.owner[r].is_some() && self.time - self.seen[r] <= STALE
    }

    /// How many of regiment `r`'s soldiers stand in its front rank.
    fn front(&self, r: usize) -> usize {
        (self.files[r] as usize).min(self.models[r] as usize)
    }

    /// Whether the soldier at `me` in regiment `a` can go on fighting puppet
    /// `j`: still standing in the front rank of a nearby enemy regiment
    /// (`near`), and within reach.
    fn can_duel(&self, a: usize, j: usize, me: Vec2, near: &[u32]) -> bool {
        let b = j / SLOTS;
        b != a
            && near.contains(&(b as u32))
            && j % SLOTS < self.front(b)
            && me.distance_squared(self.pos[j]) <= DUEL_KEEP * DUEL_KEEP
    }

    /// Advance one regiment's soldiers and hand each to `out`. `owner` is the
    /// regiment's player, to colour anyone who falls.
    pub fn update(&mut self, block: &Block, owner: u8, dt: f32, mut out: impl FnMut(Soldier)) {
        let r = block.handle.index as usize;
        let models = block.models.min(MAX_MODELS) as usize;
        let fresh = self.owner[r] != Some(block.handle);
        self.owner[r] = Some(block.handle);
        if fresh {
            for i in r * SLOTS..(r + 1) * SLOTS {
                self.partner[i] = NONE;
            }
        } else {
            self.fall(block, owner, models);
        }
        self.models[r] = models as u16;
        self.drift[r] = if fresh || dt <= 0.0 {
            Vec2::ZERO
        } else {
            let now = (block.centre - self.centre[r]) / dt;
            self.drift[r].lerp(now, (dt * 8.0).min(1.0))
        };
        self.centre[r] = block.centre;
        self.player[r] = owner;
        self.engaged[r] = block.engaged;
        self.files[r] = block.files.clamp(1, block.models.max(1));
        self.seen[r] = self.time;
        let right = Vec2::new(-block.facing.y, block.facing.x);
        let back = -block.facing;
        let files = block.files.clamp(1, block.models.max(1));
        let (spread, scatter) = if block.routing {
            (1.0 + ROUT_SPREAD, SLOT_JITTER + ROUT_JITTER)
        } else {
            (1.0, SLOT_JITTER)
        };
        let tau = core::f32::consts::TAU;
        for k in 0..models {
            let i = r * SLOTS + k;
            let local = slot_offset(
                k as u16,
                files,
                block.models,
                block.file_spacing,
                block.rank_spacing,
            ) * spread
                + jitter(block.handle, k) * scatter;
            let slot = block.centre + right * local.x + back * local.y;
            // A dueling soldier closes on his man, on a leash from his slot.
            let j = self.partner[i];
            let foe = (j != NONE && !block.routing).then(|| self.pos[j as usize]);
            let target = match foe {
                Some(f) => {
                    let gap = (self.pos[i] - f).normalize_or(back);
                    let spot = f + gap * DUEL_GAP;
                    slot + (spot - slot).clamp_length_max(DUEL_LEASH)
                }
                None => slot,
            };
            if fresh {
                self.pos[i] = target;
                self.vel[i] = Vec2::ZERO;
                self.facing[i] = block.facing;
            } else if dt > 0.0 {
                let to = target - self.pos[i];
                let dist = to.length();
                let pace = 0.9 + 0.2 * unit_hash(block.handle, k, 1);
                let top = (block.speed * CATCH_UP + dist * PULL_PER_TILE) * pace;
                let rank = (k as u16 / files) as f32;
                let accel = ACCEL * pace / (1.0 + rank * ACCEL_FALLOFF_PER_RANK);
                let accel = if block.routing { accel * 1.5 } else { accel };
                let v = self.vel[i];
                let pull = if dist > SETTLED { to } else { Vec2::ZERO };
                let push = pull * (SPRING * SPRING)
                    - (v - self.drift[r] * LEAD) * (2.0 * DAMPING * SPRING);
                self.vel[i] = (v + push.clamp_length_max(accel) * dt).clamp_length_max(top);
                self.pos[i] += self.vel[i] * dt;
                // Face your man, or where you walk when it's more than a
                // shuffle, else the regiment's way.
                let walking = self.vel[i].length() > 0.6 && dist > 0.4;
                let look = match foe {
                    Some(f) if !walking => (f - self.pos[i]).normalize_or(block.facing),
                    _ if walking => self.vel[i].normalize_or(block.facing),
                    _ => block.facing,
                };
                self.facing[i] = turn_toward(self.facing[i], look, TURN_RATE * dt);
            }
            let speed = if fresh { 0.0 } else { self.vel[i].length() };
            let mut pos = self.pos[i];
            let mut facing = self.facing[i];
            let striking = block.engaged && (k as u16) < files * 2;
            let mut phase = unit_hash(block.handle, k, 5);
            let dueling = foe.is_some() && striking;
            if dueling {
                // The pair shares a rhythm, half a beat apart: one steps in
                // as the other gives ground.
                let (lo, hi) = (i.min(j as usize), i.max(j as usize));
                let beat = pair_hash(lo, hi);
                let half = if i == lo { 0.0 } else { 0.5 };
                let step = ((self.time * DUEL_RATE + beat + half) * tau).sin();
                pos += facing * DUEL_STEP * step;
                phase = (beat + half * 0.25).fract();
            } else if striking {
                let lunge = unit_hash(block.handle, k, 4);
                let swing = ((self.time * LUNGE_RATE + lunge) * tau).sin();
                pos += facing * LUNGE * swing.max(0.0);
            } else if speed < 0.15 && !block.routing {
                // At rest: shift weight and glance about.
                let p = unit_hash(block.handle, k, 6) * tau;
                let sway = (self.time * 0.9 + p).sin() + 0.5 * (self.time * 2.3 + p * 1.7).sin();
                pos += right * SWAY * sway;
                let glance = (self.time * 0.31 + p).sin() * (self.time * 0.13 + p * 0.6).sin();
                facing = Vec2::from_angle(GLANCE * glance).rotate(facing);
            }
            out(Soldier {
                pos,
                facing,
                speed,
                striking,
                dueling,
                phase,
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
            let corpse = (
                self.pos[victim],
                self.facing[victim],
                owner,
                block.unit,
                self.time,
            );
            if self.corpses.len() < self.corpse_capacity {
                self.corpses.push(corpse);
            } else {
                self.corpses[self.next_corpse] = corpse;
                self.next_corpse = (self.next_corpse + 1) % self.corpse_capacity;
            }
            self.pos[victim] = self.pos[last];
            self.vel[victim] = self.vel[last];
            self.facing[victim] = self.facing[last];
            // Whoever steps into the gap finds his own man next frame.
            self.partner[victim] = NONE;
            self.partner[last] = NONE;
        }
    }

    /// Every fallen soldier still on the field.
    pub fn corpses(&self) -> impl Iterator<Item = Corpse> + '_ {
        self.corpses
            .iter()
            .filter_map(move |&(pos, facing, owner, unit, fell)| {
                let age = self.time - fell;
                let sink = (age - CORPSE_SECONDS) / CORPSE_SINK_SECONDS;
                (sink < 1.0).then_some(Corpse {
                    pos,
                    facing,
                    owner,
                    unit,
                    age,
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

/// A stable value in [0, 1) for a pair of puppets.
fn pair_hash(a: usize, b: usize) -> f32 {
    let mut x = (a as u32).wrapping_mul(0x9E37_79B9) ^ (b as u32).wrapping_mul(0x85EB_CA6B);
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
            unit: 0,
            centre: Vec2::new(10.0, 10.0),
            facing: Vec2::NEG_Y,
            files: 4,
            models: 8,
            file_spacing: 1.0,
            rank_spacing: 1.0,
            speed: 3.0,
            engaged: false,
            routing: false,
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
            // Settled, give or take the sway at rest.
            assert!(
                (moved - Vec2::new(0.0, 5.0)).length() < 0.05 + 3.0 * SWAY,
                "{moved}"
            );
            assert!(b.facing.angle_to(Vec2::NEG_Y).abs() <= GLANCE + 1e-3);
        }
    }

    fn block(index: u32, centre: Vec2, facing: Vec2, engaged: bool) -> Block {
        Block {
            handle: RegimentHandle {
                index,
                generation: 0,
            },
            unit: 0,
            centre,
            facing,
            files: 4,
            models: 8,
            file_spacing: 1.0,
            rank_spacing: 1.0,
            speed: 3.0,
            engaged,
            routing: false,
        }
    }

    #[test]
    fn soldiers_gather_speed_and_the_front_leads() {
        let mut p = Puppets::new(1, 4);
        let mut b = block(0, Vec2::ZERO, Vec2::NEG_Y, false);
        p.update(&b, 0, 0.016, |_| {});
        b.centre.y -= 10.0;
        let mut speeds = Vec::new();
        p.update(&b, 0, 0.016, |s| speeds.push(s.speed));
        // One frame in, nobody is near marching pace.
        assert!(speeds.iter().all(|&v| v < 0.5), "{speeds:?}");
        let mut at = Vec::new();
        for _ in 0..20 {
            at.clear();
            p.update(&b, 0, 0.016, |s| at.push(s.pos.y));
        }
        // The front rank (slots 0..4) has gone further than the rear.
        let front: f32 = at[..4].iter().sum::<f32>() / 4.0;
        let rear: f32 = at[4..].iter().sum::<f32>() / 4.0 - 1.0;
        assert!(front < rear - 0.05, "front {front} rear {rear}");
    }

    #[test]
    fn a_halting_block_overshoots_and_settles() {
        let mut p = Puppets::new(1, 4);
        let mut b = block(0, Vec2::ZERO, Vec2::NEG_Y, false);
        b.models = 1;
        b.files = 1;
        p.update(&b, 0, 0.016, |_| {});
        // March north at 3 tiles a second for two seconds, then halt.
        let mut furthest = 0.0f32;
        let mut last = Vec2::ZERO;
        for f in 0..400 {
            if f < 125 {
                b.centre.y -= 3.0 * 0.016;
            }
            p.update(&b, 0, 0.016, |s| last = s.pos);
            furthest = furthest.min(last.y);
        }
        // He settles on his slot (within its jitter), having gone past it.
        assert!((last.y + 6.0).abs() < SLOT_JITTER + 0.05, "{last}");
        let past = last.y - furthest;
        assert!(past > 0.02, "no overshoot: {past}");
        assert!(past < 0.25, "too much overshoot: {past}");
    }

    #[test]
    fn front_ranks_pair_off_and_face_their_man() {
        for hostile in [true, false] {
            let mut p = Puppets::new(2, 4);
            // Two blocks face to face, fronts 1.2 tiles apart.
            let a = block(0, Vec2::new(0.0, 0.0), Vec2::NEG_Y, true);
            let b = block(1, Vec2::new(0.3, -3.2), Vec2::Y, true);
            for _ in 0..60 {
                p.advance(0.016);
                p.pair(|x, y| hostile && x != y);
                p.update(&a, 0, 0.016, |_| {});
                p.update(&b, 1, 0.016, |_| {});
            }
            let mut out = Vec::new();
            p.update(&a, 0, 0.016, |s| out.push(s));
            let dueling = out.iter().filter(|s| s.dueling).count();
            if !hostile {
                assert_eq!(dueling, 0);
                continue;
            }
            assert_eq!(dueling, 4, "the whole front rank is paired");
            for (k, s) in out[..4].iter().enumerate() {
                let foe = p.pos[p.partner[k] as usize];
                assert!(p.partner[k] as usize / SLOTS == 1);
                let to = (foe - p.pos[k]).normalize();
                assert!(p.facing[k].dot(to) > 0.95, "slot {k} faces away");
                assert!(s.striking);
            }
            // The rear rank is not paired.
            assert!(out[4..].iter().all(|s| !s.dueling));
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
            unit: 0,
            centre: Vec2::ZERO,
            facing: Vec2::NEG_Y,
            files: 4,
            models: 8,
            file_spacing: 1.0,
            rank_spacing: 1.0,
            speed: 3.0,
            engaged: true,
            routing: false,
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
