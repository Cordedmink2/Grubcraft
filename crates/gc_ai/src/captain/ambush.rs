//! The Ambush doctrine (docs/20-ambushes.md, "Setting an ambush"): Hammer
//! and anvil with the hammers hidden. An army with no fast troops hides the
//! outer regiments of its line instead ([`line_ambushers`]).
//!
//! While the armies are still apart, the Captain looks for a hiding place
//! for each of its hammers: a spot where the whole block fits in a
//! wood (or tall grass), off one end of the line, between it and the enemy,
//! well clear of any enemy it can see and reachable long before the enemy
//! gets there; best just past the end of the line and a little in front of
//! it, beside where the enemy will be when it meets the line. The
//! ambushers march in and stand still facing the enemy; the rest of the
//! army holds, so the enemy comes on. An ambusher springs when an enemy
//! regiment is locked in melee near it, marches close by, or spots it with
//! enemies close, and from then on it is a hammer. If the battle moves away
//! from it, it gives up and rejoins as a hammer.

use gc_sim::Cover;
use glam::Vec2;

use super::{Captain, Ctx, Doctrine, Role, gap_between, line_centre};
use crate::orders::{Act, Want};
use crate::perception::{Class, Seen, right_of};

/// The most regiments that lie in wait at once.
const MAX_AMBUSHERS: usize = 3;
/// An army with no fast troops hides line regiments when its line has at
/// least this many: a third of them, at most [`MAX_AMBUSHERS`].
const LINE_FROM: usize = 4;
/// The armies must be at least this far apart (front to front, tiles) to
/// slip into hiding unseen.
const APART: f32 = 35.0;
/// A hiding place keeps this far from every enemy in sight, tiles.
const KEEP_CLEAR: f32 = 20.0;
/// How far out past the end of the line it looks for one, tiles.
const SEARCH_OUT: f32 = 32.0;
/// Spacing of the places it tries, tiles.
const SEARCH_STEP: f32 = 2.0;
/// An ambusher must get there this many times sooner than any enemy.
const HEAD_START: f32 = 1.5;
/// The best hiding place is this far past the end of the line, and this
/// far in front of it (tiles): beside where the enemy will be when it
/// meets the line.
const BEST_OUT: f32 = 8.0;
const BEST_AHEAD: f32 = 10.0;
/// It springs when an enemy is locked in melee this close to it (tiles), ...
const SPRING_MELEE: f32 = 35.0;
/// ... when an enemy comes this close to it (edge to edge, tiles), ...
const SPRING_CLOSE: f32 = 12.0;
/// ... or when it has been spotted and an enemy is this close.
const SPRING_SPOTTED: f32 = 20.0;
/// It gives up when the line and every enemy in sight are this far from
/// its hiding place (tiles).
const GIVE_UP: f32 = 45.0;
/// Seconds of extra patience in a stand-off while ambushers lie in wait:
/// the plan needs the enemy to come on.
pub(super) const WAIT_LONGER: f32 = 25.0;

/// Where the Captain's ambushers hide.
#[derive(Clone, Debug, Default)]
pub(super) struct Plan {
    hideouts: Vec<Hideout>,
    /// Someone has come to blows: too late to slip away into hiding.
    pub(super) joined: bool,
}

impl Plan {
    /// Whether regiment `h` was sent into hiding (sprung since or not).
    pub(super) fn has(&self, h: gc_sim::RegimentHandle) -> bool {
        self.hideouts.iter().any(|o| o.handle == h)
    }
}

/// How many of `line` line regiments an army with no fast troops sends
/// into hiding (the outermost, so the line stays whole).
pub(super) fn line_ambushers(line: usize) -> usize {
    if line >= LINE_FROM {
        (line / 3).min(MAX_AMBUSHERS)
    } else {
        0
    }
}

/// The class that would lie in wait in an army of `line` line and `shock`
/// fast regiments, as the layout makes hammers of them: its fast troops
/// beside a line (or a third of an army of fast troops only), otherwise its
/// line infantry when there are enough to spare.
fn ambusher_class(line: usize, shock: usize) -> Option<Class> {
    if shock > 0 && (line > 0 || shock >= 3) {
        Some(Class::Shock)
    } else {
        (line_ambushers(line) > 0).then_some(Class::Line)
    }
}

#[derive(Clone, Copy, Debug)]
struct Hideout {
    handle: gc_sim::RegimentHandle,
    /// The centre of the block when hidden.
    spot: Vec2,
    /// Sprung, or given up: a hammer from now on.
    sprung: bool,
}

impl Captain {
    /// Whether any ambusher is still lying in wait (or on its way there).
    pub(super) fn lying_in_wait(&self) -> bool {
        self.doctrine == Doctrine::Ambush && self.ambush.hideouts.iter().any(|h| !h.sprung)
    }

    /// How much better the Ambush doctrine fits than Hammer and anvil now,
    /// or `None` when it can't be used: no cover on the map, the armies
    /// already close, or nowhere for a would-be ambusher to hide. `line`
    /// and `shock` count our regiments of each class. Lying in wait suits
    /// the cunning and the patient, not the headlong.
    pub(super) fn ambush_fit(
        &self,
        ctx: &Ctx,
        ours: &[&Seen],
        fighting: &[&Seen],
        (line, shock): (usize, usize),
    ) -> Option<f32> {
        if !ctx.terrain.has_cover() {
            return None;
        }
        // Once laid, the plan stands while any of its regiments is still in
        // the fight, so the sprung ones keep their part.
        if self.doctrine == Doctrine::Ambush && !self.ambush.hideouts.is_empty() {
            return Some(1.0);
        }
        if self.ambush.joined || self.front_gap(fighting) < APART {
            return None;
        }
        let view = ctx.view;
        let class = ambusher_class(line, shock)?;
        let (_, wood) = ours
            .iter()
            .filter(|r| view.unit(r).class == class)
            .find_map(|r| self.hiding_place(ctx, r, fighting, &[]))?;
        let p = ctx.persona;
        let margin = self.estimate.map_or(0.0, |e| e.margin);
        Some(
            0.25 * p.cunning() + 0.15 * p.patience() - 0.15 * p.aggression()
                + if wood { 0.05 } else { 0.0 }
                + if margin < 0.0 { 0.1 } else { 0.0 },
        )
    }

    /// After the layout: with the Ambush doctrine, send hammers into
    /// hiding (the first time) or keep them there; otherwise forget the
    /// plan.
    pub(super) fn lay_ambush(&mut self, ctx: &mut Ctx) {
        let view = ctx.view;
        if self.doctrine != Doctrine::Ambush {
            self.ambush.hideouts.clear();
            return;
        }
        // A hider the layout took back into a smaller line is out of it.
        let members = &self.members;
        self.ambush.hideouts.retain(|h| {
            view.get(h.handle).is_some_and(|r| !r.broken())
                && members.iter().any(|m| {
                    m.handle == h.handle && matches!(m.home, Role::Hammer | Role::Ambusher)
                })
        });
        let fighting: Vec<&Seen> = view
            .regiments
            .iter()
            .filter(|r| view.hostile(ctx.party, r.owner) && !r.broken())
            .collect();
        let apart = !self.ambush.joined && self.front_gap(&fighting) >= APART;
        let mut laid = 0;
        for k in 0..self.members.len() {
            let m = self.members[k];
            if m.lost || m.home != Role::Hammer {
                continue;
            }
            let known = self.ambush.hideouts.iter().find(|h| h.handle == m.handle);
            match known {
                // Sprung: a hammer from now on.
                Some(h) if h.sprung => continue,
                Some(_) => {}
                None => {
                    let hiding = self.ambush.hideouts.iter().filter(|h| !h.sprung).count();
                    if !apart || hiding >= MAX_AMBUSHERS {
                        continue;
                    }
                    let Some(r) = view.get(m.handle) else {
                        continue;
                    };
                    let taken: Vec<Vec2> = self.ambush.hideouts.iter().map(|h| h.spot).collect();
                    let Some((spot, _)) = self.hiding_place(ctx, r, &fighting, &taken) else {
                        continue;
                    };
                    self.ambush.hideouts.push(Hideout {
                        handle: m.handle,
                        spot,
                        sprung: false,
                    });
                    laid += 1;
                }
            }
            let m = &mut self.members[k];
            m.home = Role::Ambusher;
            if m.role == Role::Hammer {
                m.role = Role::Ambusher;
                m.target = None;
                m.striking = false;
            }
        }
        if laid > 0 {
            let s = if laid == 1 { "" } else { "s" };
            ctx.journal.note(
                view.tick,
                format!("Laying an ambush: {laid} regiment{s} into hiding"),
            );
        }
    }

    /// The best place near the line for `r` to hide, and whether it is a
    /// wood (rather than tall grass). `taken` are hiding places already
    /// given out.
    fn hiding_place(
        &self,
        ctx: &Ctx,
        r: &Seen,
        fighting: &[&Seen],
        taken: &[Vec2],
    ) -> Option<(Vec2, bool)> {
        let terrain = ctx.terrain;
        let view = ctx.view;
        let right = right_of(self.forward);
        let half_line = self.line_width * 0.5;
        let gap = self.front_gap(fighting);
        let speed = view.unit(r).speed.max(0.5);
        let reach = half_line + SEARCH_OUT;
        let steps = |from: f32, to: f32| {
            (0..)
                .map(move |i| from + i as f32 * SEARCH_STEP)
                .take_while(move |&v| v <= to)
        };
        let mut best: Option<(f32, Vec2, bool)> = None;
        for ahead in steps(-6.0, gap * 0.5) {
            for across in steps(-reach, reach) {
                // Off an end of the line, not in front of its middle.
                if across.abs() < half_line * 0.5 {
                    continue;
                }
                let c = self.anchor + right * across + self.forward * ahead;
                if !terrain.inside(c, 6.0) || !terrain.walkable(c) {
                    continue;
                }
                let facing = (self.enemy_at - c).normalize_or(self.forward);
                let Some(wood) = block_cover(ctx, c, facing, r) else {
                    continue;
                };
                if taken.iter().any(|t| t.distance(c) < r.width + 2.0) {
                    continue;
                }
                // Well clear of every enemy in sight, and there long before
                // any of them could be.
                let ours = c.distance(r.pos) / speed;
                let clear = fighting.iter().all(|f| {
                    let d = f.pos.distance(c);
                    d >= KEEP_CLEAR + f.radius()
                        && d / view.unit(f).speed.max(0.5) >= ours * HEAD_START
                });
                if !clear {
                    continue;
                }
                let score = if wood { 1.0 } else { 0.6 }
                    - ours / 40.0
                    - (across.abs() - half_line - BEST_OUT).abs() / 30.0
                    - (ahead - BEST_AHEAD).abs() / 40.0;
                if best.is_none_or(|(s, _, _)| score > s) {
                    best = Some((score, c, wood));
                }
            }
        }
        best.map(|(_, c, wood)| (c, wood))
    }

    /// An ambusher's turn: into its hiding place and still, until it is
    /// time to spring.
    pub(super) fn ambusher_role(
        &mut self,
        ctx: &mut Ctx,
        m: &mut super::Member,
        r: &Seen,
        ours: &[&Seen],
        fighting: &[&Seen],
        wants: &mut Vec<Want>,
    ) {
        let Some(k) = self
            .ambush
            .hideouts
            .iter()
            .position(|h| h.handle == m.handle && !h.sprung)
        else {
            m.role = Role::Hammer;
            m.home = Role::Hammer;
            self.hammer_role(ctx, m, r, fighting, false, wants);
            return;
        };
        let spot = self.ambush.hideouts[k].spot;
        let melee = fighting
            .iter()
            .any(|f| f.engaged && f.pos.distance(r.pos) < SPRING_MELEE);
        let close = fighting.iter().any(|f| gap_between(r, f) < SPRING_CLOSE);
        let spotted = !r.hidden
            && r.pos.distance(spot) < 3.0
            && fighting.iter().any(|f| gap_between(r, f) < SPRING_SPOTTED);
        // Where the line stands now (while advancing, the anchor is where
        // it is going).
        let line_at = line_centre(&self.members, ours, self.anchor);
        let forgotten = line_at.distance(spot) > GIVE_UP
            && fighting.iter().all(|f| f.pos.distance(spot) > GIVE_UP);
        if melee || close || spotted || forgotten {
            self.ambush.hideouts[k].sprung = true;
            m.role = Role::Hammer;
            m.home = Role::Hammer;
            let note = if forgotten {
                "Calling off the ambush: the battle has moved on"
            } else {
                "Springing the ambush"
            };
            ctx.journal.note(ctx.view.tick, note.to_string());
            // The first blow, as a hammer whose moment has come.
            self.hammer_role(ctx, m, r, fighting, !forgotten, wants);
            return;
        }
        let facing = (self.enemy_at - spot).normalize_or(self.forward);
        m.target = None;
        wants.push(Want {
            regiment: m.handle,
            act: Act::Move {
                to: spot,
                facing: Some(facing),
                pace: 0.0,
            },
            priority: 3,
            group: 0,
        });
    }
}

/// Whether `r`'s block, standing at `c` facing `facing`, would be wholly in
/// cover: `Some(true)` all in a wood, `Some(false)` all in woods and tall
/// grass, `None` not hidden. (The simulation hides a block with four of its
/// five points covered; all five leaves room for the block to settle.)
fn block_cover(ctx: &Ctx, c: Vec2, facing: Vec2, r: &Seen) -> Option<bool> {
    let right = right_of(facing);
    let (w, d) = (r.width * 0.5, r.depth * 0.5);
    let mut wood = true;
    for p in [
        c,
        c + right * w + facing * d,
        c + right * w - facing * d,
        c - right * w + facing * d,
        c - right * w - facing * d,
    ] {
        match ctx.terrain.cover(p) {
            Cover::Wood => {}
            Cover::Grass => wood = false,
            Cover::Open => return None,
        }
    }
    Some(wood)
}
