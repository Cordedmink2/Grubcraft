//! From decisions to commands (docs/18-ai.md, "Commands and attention").
//!
//! The battle layers say what they want each regiment to do; this layer
//! decides what actually gets sent, the way a person's hands would:
//! - **No order spam.** A regiment's order is only replaced when the new
//!   one differs meaningfully, or when the old one plainly isn't being
//!   carried out (it stopped short, or wandered off chasing someone else).
//! - **Attention budget.** Orders cost attention, which refills at the
//!   difficulty's orders-per-minute rate. A line moving together is one
//!   order, like a right-drag. When attention runs short the most urgent
//!   orders go first and the rest wait for the next look.
//! - **Reaction time.** Every order leaves a fixed delay after it is decided,
//!   like a human's reaction (on top of the network's own input delay).

use std::collections::VecDeque;

use gc_sim::{Command, PlayerCommand, PlayerId, RegimentHandle};
use glam::Vec2;

use crate::perception::{Perception, to_fx, to_fx_vec, vec_angle};

/// What a regiment should do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    /// March to `to`, then face `facing` (or keep the direction of travel).
    /// `pace` is tiles per second; zero means the regiment's own speed.
    Move {
        to: Vec2,
        facing: Option<Vec2>,
        pace: f32,
    },
    /// Close with an enemy and fight it, or shoot it from range.
    Attack(RegimentHandle),
}

/// A decision about one regiment.
#[derive(Clone, Copy, Debug)]
pub struct Want {
    pub regiment: RegimentHandle,
    pub act: Act,
    /// Higher goes first when attention is short.
    pub priority: u8,
    /// Orders in the same nonzero group are given together and cost one
    /// order between them.
    pub group: u8,
}

/// Moves to points closer than this to the last one sent aren't re-sent.
const MOVE_SLACK: f32 = 2.5;
/// Facings closer than this (cosine of about 10 degrees) count as the same.
const FACING_SLACK: f32 = 0.985;
/// A regiment at rest is sent on to a place moved at least this far from
/// the one it was sent to.
const SETTLE: f32 = 1.5;
/// A regiment within this many tiles of its destination has arrived.
const ARRIVED: f32 = 2.0;
/// Ticks after an order lands before checking it is being carried out.
const GRACE_TICKS: u32 = 20;
/// The group for orders to attack `target`: 2 to 63 (0 is no group and 1
/// is the formation's moves).
pub fn attack_group(target: RegimentHandle) -> u8 {
    2 + (target.index % 62) as u8
}

/// Orders at this priority or below are routine: they wait while attention
/// is short, keeping some in hand for urgent ones.
pub const ROUTINE: u8 = 1;
/// How many times a move that stops short is re-sent before giving up on it
/// (its destination may be somewhere it can't reach).
const RETRIES: u8 = 2;

#[derive(Clone, Copy, Debug)]
struct Sent {
    regiment: RegimentHandle,
    act: Act,
    /// Tick the order reaches the simulation.
    lands: u32,
    /// Distance to the destination or target at the last look after it landed.
    last_distance: Option<f32>,
    retries: u8,
}

pub struct Orders {
    party: PlayerId,
    reaction: u32,
    /// Attention per tick and the most that can be saved up.
    refill: f32,
    cap: f32,
    attention: f32,
    last_tick: Option<u32>,
    /// The order last decided for each regiment, by pool index.
    sent: Vec<Option<Sent>>,
    /// Orders decided but not yet sent, in the order they were decided.
    queue: VecDeque<(u32, Command)>,
    /// Orders sent so far, for the journal and tests.
    pub count: u32,
    /// Orders decided so far, by priority, and how many were repeats of
    /// one not being carried out.
    pub decided: [u32; 8],
    /// Looks at which a wanted order had to wait for attention, by priority.
    pub waited: [u32; 8],
    pub repeats: u32,
    /// Attention spent so far.
    pub spent: u32,
}

impl Orders {
    pub fn new(party: PlayerId, reaction_ticks: u32, orders_per_minute: u32) -> Orders {
        let per_minute = orders_per_minute.max(1) as f32;
        let cap = (per_minute / 4.0).max(3.0);
        Orders {
            party,
            reaction: reaction_ticks,
            refill: per_minute / (60.0 * crate::perception::TICKS),
            cap,
            attention: cap,
            last_tick: None,
            sent: Vec::new(),
            queue: VecDeque::new(),
            count: 0,
            decided: [0; 8],
            waited: [0; 8],
            repeats: 0,
            spent: 0,
        }
    }

    /// Decide which of `wants` to send. Wants for regiments that are gone
    /// are ignored. Reorders `wants` by priority.
    pub fn decide(&mut self, view: &Perception, wants: &mut [Want]) {
        let now = view.tick;
        if let Some(last) = self.last_tick {
            let dt = now.saturating_sub(last) as f32;
            self.attention = (self.attention + dt * self.refill).min(self.cap);
        }
        self.last_tick = Some(now);
        // Most urgent first; among equals, the order they were decided.
        wants.sort_by_key(|w| std::cmp::Reverse(w.priority));
        let mut paid_groups: u64 = 0;
        for w in wants.iter() {
            let i = w.regiment.index as usize;
            if self.sent.len() <= i {
                self.sent.resize(i + 1, None);
            }
            let Some(r) = view.get(w.regiment) else {
                continue;
            };
            let prev = self.sent[i].filter(|s| s.regiment == w.regiment);
            if already_there(r, &w.act) {
                continue;
            }
            let needed = match prev {
                None => true,
                Some(s) if !same(&s.act, &w.act) => true,
                Some(s) if now >= s.lands && moved_on(r, &s.act, &w.act) => true,
                Some(s) => self.not_carried_out(view, r, s),
            };
            if !needed {
                continue;
            }
            let free = w.group != 0 && paid_groups & (1 << (w.group % 64)) != 0;
            if !free {
                // Routine orders leave some attention in hand for urgent ones.
                let keep = if w.priority <= ROUTINE {
                    self.cap * 0.5
                } else {
                    0.0
                };
                if self.attention < 1.0 + keep {
                    self.waited[(w.priority as usize).min(7)] += 1;
                    continue;
                }
                self.attention -= 1.0;
                self.spent += 1;
                if w.group != 0 {
                    paid_groups |= 1 << (w.group % 64);
                }
            }
            let retries = match prev {
                // Sending a regiment at rest on to its shifted place is a new
                // order, not another try at one it stopped short of.
                Some(s) if same(&s.act, &w.act) && !moved_on(r, &s.act, &w.act) => s.retries + 1,
                _ => 0,
            };
            self.decided[(w.priority as usize).min(7)] += 1;
            if retries > 0 {
                self.repeats += 1;
            }
            self.sent[i] = Some(Sent {
                regiment: w.regiment,
                act: w.act,
                lands: now + self.reaction,
                last_distance: None,
                retries,
            });
            self.queue
                .push_back((now + self.reaction, command(w.regiment, &w.act)));
        }
    }

    /// Whether the last order sent to `r` plainly isn't being carried out,
    /// so sending it again is worth an order. Also notes how far it has to go.
    fn not_carried_out(&mut self, view: &Perception, r: &crate::perception::Seen, s: Sent) -> bool {
        let now = view.tick;
        if now < s.lands + GRACE_TICKS || r.engaged || r.broken() {
            return false;
        }
        let i = r.handle.index as usize;
        let distance = match s.act {
            Act::Move { to, .. } => r.pos.distance(to),
            Act::Attack(t) => match view.get(t) {
                Some(t) => r.pos.distance(t.pos),
                None => return false,
            },
        };
        let last = s.last_distance;
        if let Some(sent) = self.sent[i].as_mut() {
            sent.last_distance = Some(distance);
        }
        let stopped_short = match s.act {
            Act::Move { .. } => !r.moving && distance > ARRIVED,
            Act::Attack(t) => {
                let range = view.unit(r).range;
                let reach = view.get(t).map_or(0.0, |t| r.radius() + t.radius());
                !r.moving && distance > range.max(reach) + 2.0
            }
        };
        let walking_away = last.is_some_and(|d| distance > d + 1.0);
        (stopped_short && s.retries < RETRIES) || walking_away
    }

    /// Send the orders whose reaction time is up.
    pub fn release(&mut self, tick: u32, out: &mut Vec<PlayerCommand>) {
        while let Some(&(at, command)) = self.queue.front() {
            if at > tick {
                break;
            }
            self.queue.pop_front();
            self.count += 1;
            out.push(PlayerCommand {
                player: self.party,
                command,
            });
        }
    }

    /// The order last decided for a regiment, if any.
    pub fn last(&self, h: RegimentHandle) -> Option<Act> {
        self.sent
            .get(h.index as usize)
            .copied()
            .flatten()
            .filter(|s| s.regiment == h)
            .map(|s| s.act)
    }

    /// Orders waiting out their reaction time.
    pub fn pending(&self) -> usize {
        self.queue.len()
    }
}

/// A small change to a move that has already arrived: worth an order once
/// the regiment stands at rest, where on the march it would only be jitter.
fn moved_on(r: &crate::perception::Seen, before: &Act, now: &Act) -> bool {
    match (*before, *now) {
        (Act::Move { to: a, .. }, Act::Move { to: b, .. }) => {
            !r.moving && a.distance(b) >= SETTLE && r.pos.distance(b) >= SETTLE
        }
        _ => false,
    }
}

/// A move to where the regiment already stands, facing the right way.
fn already_there(r: &crate::perception::Seen, act: &Act) -> bool {
    match *act {
        Act::Move { to, facing, .. } => {
            !r.moving
                && r.pos.distance(to) < ARRIVED
                && facing.is_none_or(|f| f.dot(r.facing) > FACING_SLACK)
        }
        Act::Attack(_) => false,
    }
}

fn same(a: &Act, b: &Act) -> bool {
    match (a, b) {
        (Act::Attack(x), Act::Attack(y)) => x == y,
        (
            Act::Move {
                to: a,
                facing: fa,
                pace: pa,
            },
            Act::Move {
                to: b,
                facing: fb,
                pace: pb,
            },
        ) => {
            a.distance(*b) < MOVE_SLACK
                && (pa - pb).abs() < 0.2
                && match (fa, fb) {
                    (None, None) => true,
                    (Some(x), Some(y)) => x.dot(*y) > FACING_SLACK,
                    _ => false,
                }
        }
        _ => false,
    }
}

fn command(regiment: RegimentHandle, act: &Act) -> Command {
    match *act {
        Act::Move { to, facing, pace } => Command::MoveTo {
            regiment,
            target: to_fx_vec(to),
            facing: facing.map(vec_angle),
            files: 0,
            pace: to_fx(pace),
        },
        Act::Attack(target) => Command::Attack { regiment, target },
    }
}
