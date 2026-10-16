//! Playing each role: turning roles and slots into move and attack orders.

use super::*;

impl Captain {
    pub(super) fn act(
        &mut self,
        ctx: &mut Ctx,
        ours: &[&Seen],
        fighting: &[&Seen],
        foes: &[&Seen],
        wants: &mut Vec<Want>,
    ) {
        let view = ctx.view;
        let our_c = centroid(ours);
        // The enemy line is locked once most of its melee regiments are
        // fighting: then it can't turn to meet a hammer.
        let melee = fighting
            .iter()
            .filter(|f| view.unit(f).class != Class::Ranged);
        let (fighting_melee, engaged_melee) =
            melee.fold((0, 0), |(n, e), f| (n + 1, e + usize::from(f.engaged)));
        let locked = engaged_melee * 2 >= fighting_melee.max(1);
        let pace = if self.posture == Posture::Advance {
            self.pace
        } else {
            0.0
        };
        // Who is already going for whom, so friends can gang up (or not).
        let mut on: Vec<(RegimentHandle, u8)> = Vec::new();
        for m in &self.members {
            if let Some(t) = m.target
                && view.get(m.handle).is_some_and(|r| !r.broken())
            {
                bump(&mut on, t);
            }
        }
        let routers = foes.iter().any(|f| f.broken());
        self.taken = self.taken_ground(view, ctx.party);
        self.focus = if ctx.knobs.morale_targeting {
            point_of_attack(view, fighting, self.anchor)
        } else {
            None
        };
        for k in 0..self.members.len() {
            let mut m = self.members[k];
            let Some(r) = view.get(m.handle) else {
                continue;
            };
            if m.lost || r.broken() {
                continue;
            }
            if r.engaged {
                // Locked in melee: nothing an order can change. When the
                // fight ends the role picks up from here.
                continue;
            }
            // Out of the fight while wavering: pull it back to recover.
            if ctx.knobs.save_wavering
                && r.morale == MoraleState::Wavering
                && !matches!(m.role, Role::Recover | Role::Hunter)
                && !(m.role == Role::Hammer && m.striking)
            {
                m.home = m.role;
                m.role = Role::Recover;
                m.target = None;
                // One place to fall back to, not a new one every look.
                m.retreat_to = Some(self.place(ctx, m.slot + Vec2::new(0.0, RECOVER_BACK)));
            }
            if m.role == Role::Recover && r.morale == MoraleState::Steady {
                m.role = m.home;
                m.retreat_to = None;
            }
            let before_target = m.target;
            match m.role {
                Role::Recover => {
                    let back = m
                        .retreat_to
                        .unwrap_or_else(|| self.place(ctx, m.slot + Vec2::new(0.0, RECOVER_BACK)));
                    wants.push(Want {
                        regiment: m.handle,
                        act: Act::Move {
                            to: back,
                            facing: Some(self.forward),
                            pace: 0.0,
                        },
                        priority: 4,
                        group: 0,
                    });
                }
                Role::Hunter => {
                    if !self.hunt(ctx, &mut m, r, our_c, foes, wants) {
                        // The quarry got away (or rallied): back to work.
                        let to = self.place(ctx, m.slot);
                        wants.push(slot_move(m.handle, to, self.forward, pace));
                    }
                }
                Role::Anchor | Role::Wing => {
                    self.line_role(ctx, &mut m, r, fighting, foes, &on, pace, wants);
                }
                Role::Reserve => {
                    self.reserve_role(ctx, &mut m, r, fighting, &on, pace, wants);
                }
                Role::Hammer => {
                    let idle = !m.striking
                        && routers
                        && !fighting.iter().any(|f| f.pos.distance(r.pos) < 30.0);
                    if !(idle && self.hunt(ctx, &mut m, r, our_c, foes, wants)) {
                        self.hammer_role(ctx, &mut m, r, fighting, locked, wants);
                    }
                }
                Role::Battery | Role::Screen => {
                    self.archer_role(ctx, &mut m, r, fighting, foes, &on, pace, wants);
                }
                Role::Ambusher => {
                    self.ambusher_role(ctx, &mut m, r, ours, fighting, wants);
                }
            }
            if m.target != before_target {
                if let Some(t) = before_target {
                    unbump(&mut on, t);
                }
                if let Some(t) = m.target {
                    bump(&mut on, t);
                }
            }
            self.members[k] = m;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn line_role(
        &self,
        ctx: &mut Ctx,
        m: &mut Member,
        r: &Seen,
        fighting: &[&Seen],
        foes: &[&Seen],
        on: &[(RegimentHandle, u8)],
        pace: f32,
        wants: &mut Vec<Want>,
    ) {
        let view = ctx.view;
        let home = self.place(ctx, m.slot);
        let reach = ctx.knobs.engage_tiles + 3.0 * ctx.persona.aggression();
        let leash = LINE_LEASH + reach;
        let right = right_of(self.forward);
        let help = ctx.knobs.focus_fire;
        // The wing holding the end of the line against cover.
        let anchored = m.role == Role::Wing && m.slot.x * self.covered > 0.0;
        let terrain = ctx.terrain;
        // Only archers still standing against us: go and get them.
        let only_archers = fighting.iter().all(|f| view.unit(f).class == Class::Ranged);
        let wanted = |f: &Seen, slack: f32| {
            let gap = gap_between(r, f) - slack;
            if gap <= 3.0 {
                // Close enough to hit us: always an answer.
                return true;
            }
            if self.beyond_water(terrain, r, f) {
                return false;
            }
            if only_archers {
                return true;
            }
            let from_home = f.pos.distance(home) - slack;
            // Coming at our stretch of the line (or the point of attack,
            // a little further).
            let across = (f.pos - home).dot(right).abs();
            let reach = if Some(f.handle) == self.focus {
                reach + FOCUS_REACH
            } else {
                reach
            };
            // An anchored wing only meets what comes straight at it, rather
            // than leave the end of the line open.
            let side_slack = if anchored { 0.0 } else { 8.0 };
            if gap <= reach
                && from_home <= leash
                && across <= r.width * 0.5 + f.width * 0.5 + side_slack
            {
                return true;
            }
            // Locked in a fight with a friend close by: join in, where there
            // is room beside the friends already fighting it.
            if help
                && !anchored
                && f.engaged
                && gap <= HELP_TILES
                && from_home <= leash
                && self.room_at(r, f)
            {
                return true;
            }
            // Archers left unguarded close by, with nothing in the way.
            view.unit(f).class == Class::Ranged
                && gap <= 2.0 * reach
                && exposed(view, fighting, f)
                && path_clear(view, fighting, r.pos, f.pos, f.handle)
        };
        let keep = m
            .target
            .and_then(|t| view.get(t))
            .filter(|t| !t.broken() && wanted(t, 6.0))
            .map(|t| t.handle);
        let candidates = fighting.iter().filter(|f| wanted(f, 0.0));
        match self.best_melee_target(ctx, r, candidates, on, keep) {
            Some(t) => {
                m.target = Some(t);
                wants.push(attack(m.handle, t, 3));
            }
            None => {
                // Nobody to fight: see off a router close by, so it can't
                // rally and come back, then return to its place.
                match self.pursuit(ctx, r, home, foes) {
                    Some(q) => {
                        m.target = Some(q);
                        wants.push(attack(m.handle, q, 2));
                    }
                    None => {
                        m.target = None;
                        wants.push(slot_move(m.handle, home, self.forward, pace));
                    }
                }
            }
        }
    }

    /// A router close to `r` and its place in the line, worth chasing so it
    /// can't rally (routers rally once no enemy is near for a while).
    pub(super) fn pursuit(
        &self,
        ctx: &Ctx,
        r: &Seen,
        home: Vec2,
        foes: &[&Seen],
    ) -> Option<RegimentHandle> {
        if !ctx.knobs.focus_fire {
            return None;
        }
        foes.iter()
            .filter(|f| f.morale == MoraleState::Routing)
            .filter(|f| {
                gap_between(r, f) <= PURSUE_TILES
                    && f.pos.distance(home) <= LINE_LEASH + PURSUE_TILES
            })
            .min_by(|a, b| a.pos.distance(r.pos).total_cmp(&b.pos.distance(r.pos)))
            .map(|f| f.handle)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn reserve_role(
        &self,
        ctx: &mut Ctx,
        m: &mut Member,
        r: &Seen,
        fighting: &[&Seen],
        on: &[(RegimentHandle, u8)],
        pace: f32,
        wants: &mut Vec<Want>,
    ) {
        let view = ctx.view;
        let back_of_line = self.line_depth * 0.5;
        // A breach: an enemy that isn't fighting is behind our line or close
        // to our archers.
        let archers: Vec<&Seen> = self
            .members
            .iter()
            .filter(|m| matches!(m.role, Role::Battery | Role::Screen))
            .filter_map(|m| view.get(m.handle))
            .collect();
        let breach = fighting.iter().filter(|f| {
            !f.engaged
                && f.pos.distance(r.pos) < 40.0
                && ((self.anchor - f.pos).dot(self.forward) > back_of_line
                    || archers.iter().any(|a| gap_between(a, f) < 12.0))
        });
        // Or a gap: a line regiment gone, broken or recovering, with the
        // enemy near its place.
        let gap_slot = self
            .members
            .iter()
            .filter(|o| matches!(o.home, Role::Anchor | Role::Wing))
            .filter(|o| {
                o.lost || o.role == Role::Recover || view.get(o.handle).is_none_or(Seen::broken)
            })
            .map(|o| self.place(ctx, o.slot))
            .find(|p| fighting.iter().any(|f| f.pos.distance(*p) < 22.0));
        // Or a wavering enemy locked in melee nearby, to finish.
        let finish = fighting.iter().filter(|f| {
            ctx.knobs.morale_targeting
                && f.engaged
                && f.morale == MoraleState::Wavering
                && f.pos.distance(r.pos) < 22.0
        });
        // Or a friend in a fight it is losing on numbers: weigh in there.
        let party = ctx.party;
        let support = fighting.iter().filter(|f| {
            f.engaged && f.pos.distance(r.pos) < SUPPORT_TILES && {
                let (ours, theirs) = local_soldiers(view, party, f.pos, r.handle);
                theirs >= ours * 0.9
            }
        });
        let keep = m
            .target
            .and_then(|t| view.get(t))
            .filter(|t| !t.broken() && t.pos.distance(r.pos) < 45.0)
            .map(|t| t.handle);
        // A breach first; then a gap in the line with the enemy coming at
        // it, before helping anywhere else.
        let breach: Vec<&&Seen> = breach.collect();
        let target = match gap_slot {
            Some(_) if breach.is_empty() => None,
            _ => self.best_melee_target(
                ctx,
                r,
                breach.into_iter().chain(finish).chain(support),
                on,
                keep,
            ),
        };
        if let Some(t) = target {
            m.target = Some(t);
            wants.push(attack(m.handle, t, 3));
            return;
        }
        let to = match gap_slot {
            Some(p) => {
                // Standing in the gap, it fights like the line. On its way
                // there it answers only an enemy close enough to hit it, so
                // the line's reach doesn't draw it off before it arrives.
                let there = r.pos.distance(p) < GAP_THERE;
                let near = fighting.iter().filter(|f| {
                    let reach = if there { ctx.knobs.engage_tiles } else { 3.0 };
                    gap_between(r, f) <= reach && f.pos.distance(p) < 16.0
                });
                let keep = keep.filter(|&t| {
                    view.get(t).is_some_and(|t| {
                        t.pos.distance(p) < 16.0 && (there || gap_between(r, t) <= 3.0)
                    })
                });
                if let Some(t) = self.best_melee_target(ctx, r, near, on, keep) {
                    m.target = Some(t);
                    wants.push(attack(m.handle, t, 3));
                    return;
                }
                p
            }
            None => self.place(ctx, m.slot),
        };
        m.target = None;
        wants.push(slot_move(m.handle, to, self.forward, pace));
    }

    pub(super) fn hammer_role(
        &self,
        ctx: &mut Ctx,
        m: &mut Member,
        r: &Seen,
        fighting: &[&Seen],
        locked: bool,
        wants: &mut Vec<Want>,
    ) {
        let view = ctx.view;
        let timed = ctx.knobs.timed_hammer;
        // A strike in progress: keep at it while the target stands.
        if m.striking {
            match m.target.and_then(|t| view.get(t)).filter(|t| !t.broken()) {
                Some(t) => {
                    self.strike(ctx, m, r, t, wants);
                    return;
                }
                None => {
                    m.striking = false;
                    m.target = None;
                }
            }
        }
        let side = m.slot.x.signum();
        let right = right_of(self.forward);
        let exposed = |f: &Seen| exposed(view, fighting, f);
        // Something coming at it while it waits: meet it head on if it can
        // win that fight (both then charge), or get out of its way.
        if let Some(threat) = self.threat(ctx, r, fighting) {
            let mine = Force::of(view, [r]);
            let theirs = Force::of(view, [threat]);
            let duel = estimator::estimate(&mine, &theirs, Approach::CONTACT);
            // Running from something as fast only gets it charged in the back.
            let outruns = view.pace(r) > view.pace(threat) * 1.1;
            if duel.margin > 0.2 || !outruns || ctx.slip() {
                m.target = Some(threat.handle);
                wants.push(attack(m.handle, threat.handle, 4));
            } else {
                // Behind our own line, where the line can take it in the flank.
                let behind = Vec2::new(m.slot.x * 0.6, self.line_depth + RESERVE_BACK);
                let to = self.place(ctx, behind);
                m.target = None;
                wants.push(Want {
                    regiment: m.handle,
                    act: Act::Move {
                        to,
                        facing: Some(self.forward),
                        pace: 0.0,
                    },
                    priority: 4,
                    group: 0,
                });
            }
            return;
        }
        let terrain = ctx.terrain;
        let go = if !timed {
            fighting.iter().any(|f| gap_between(r, f) < 30.0)
        } else {
            locked
                || fighting.iter().any(|f| {
                    exposed(f)
                        && f.pos.distance(r.pos) < 45.0
                        && path_clear(view, fighting, r.pos, f.pos, f.handle)
                        && !self.beyond_water(terrain, r, f)
                })
        };
        if go {
            let reckless = !timed || ctx.slip();
            let target = fighting
                .iter()
                .filter(|f| !self.beyond_water(terrain, r, f))
                .filter(|f| {
                    // Never fed into the front of a steady enemy that is
                    // free to meet it.
                    let can_reach = locked || path_clear(view, fighting, r.pos, f.pos, f.handle);
                    can_reach
                        && (reckless
                            || f.engaged
                            || exposed(f)
                            || f.side_facing(r.pos) != Side::Front
                            || view.unit(f).class == Class::Ranged)
                })
                .map(|f| {
                    let mut s = -f.pos.distance(r.pos) / 15.0;
                    if f.engaged {
                        s += 2.0;
                    }
                    if exposed(f) {
                        s += 1.5;
                    }
                    if (f.pos - self.anchor).dot(right) * side > 0.0 {
                        s += 1.0;
                    }
                    if ctx.knobs.morale_targeting {
                        if f.morale == MoraleState::Wavering {
                            s += 1.5 + ctx.persona.opportunism();
                        }
                        if view.unit(f).stalwart {
                            s -= 1.0;
                        }
                    }
                    (s, f.handle)
                })
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, h)| h);
            if let Some(t) = target.and_then(|t| view.get(t)) {
                m.striking = true;
                m.target = Some(t.handle);
                self.strike(ctx, m, r, t, wants);
                return;
            }
        }
        // Not yet: wait in its place, keeping level with the line on the
        // march rather than racing ahead of it.
        let pace = if self.posture == Posture::Advance {
            self.pace
        } else {
            0.0
        };
        let to = self.place(ctx, m.slot);
        wants.push(slot_move(m.handle, to, self.forward, pace));
    }

    /// The nearest enemy melee regiment marching at `r` and about to reach it.
    pub(super) fn threat<'a>(
        &self,
        ctx: &Ctx,
        r: &Seen,
        fighting: &[&'a Seen],
    ) -> Option<&'a Seen> {
        let view = ctx.view;
        fighting
            .iter()
            .copied()
            .filter(|f| !f.engaged && f.moving && view.unit(f).class != Class::Ranged)
            .filter(|f| gap_between(r, f) <= COUNTER_TILES)
            .filter(|f| {
                let to_us = (r.pos - f.pos).normalize_or_zero();
                f.facing.dot(to_us) > 0.8
            })
            .min_by(|a, b| a.pos.distance(r.pos).total_cmp(&b.pos.distance(r.pos)))
    }

    /// Swing round to the target's flank or rear, then charge it.
    pub(super) fn strike(
        &self,
        ctx: &mut Ctx,
        m: &mut Member,
        r: &Seen,
        t: &Seen,
        wants: &mut Vec<Want>,
    ) {
        let timed = ctx.knobs.timed_hammer;
        let across = right_of(t.facing);
        let side = {
            let s = (r.pos - t.pos).dot(across);
            if s == 0.0 { 1.0 } else { s.signum() }
        };
        let stage = t.pos + across * side * (t.width * 0.5 + STAGE_OUT)
            - t.facing * (t.depth * 0.5 + STAGE_BACK);
        let level = (r.pos - t.pos).dot(t.facing) < t.depth * 0.5;
        let flank = t.side_facing(r.pos) != Side::Front;
        let charge = !timed
            || (flank
                && level
                && (r.pos.distance(t.pos) < 16.0 || (r.pos - t.pos).dot(t.facing) < 0.0))
            || r.pos.distance(stage) < 4.0;
        if charge {
            wants.push(attack(m.handle, t.handle, 3));
        } else {
            let to = ctx.terrain.clamp(stage, 3.0);
            wants.push(Want {
                regiment: m.handle,
                act: Act::Move {
                    to,
                    facing: Some((t.pos - to).normalize_or(self.forward)),
                    pace: 0.0,
                },
                priority: 3,
                group: 0,
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn archer_role(
        &self,
        ctx: &mut Ctx,
        m: &mut Member,
        r: &Seen,
        fighting: &[&Seen],
        foes: &[&Seen],
        on: &[(RegimentHandle, u8)],
        pace: f32,
        wants: &mut Vec<Want>,
    ) {
        let view = ctx.view;
        let range = view.unit(r).range;
        let danger_at = if m.role == Role::Screen {
            SCREEN_DANGER
        } else {
            BATTERY_DANGER
        };
        let danger = fighting
            .iter()
            .filter(|f| !f.engaged && view.unit(f).class != Class::Ranged)
            .filter(|f| gap_between(r, f) < danger_at)
            .min_by(|a, b| gap_between(r, a).total_cmp(&gap_between(r, b)));
        if let Some(d) = danger {
            if m.role == Role::Screen {
                // The screen's job is done: back through the line.
                m.role = Role::Battery;
                m.home = Role::Battery;
                m.slot = m.fallback;
                m.fell_back = true;
                m.target = None;
                let to = self.place(ctx, m.slot);
                wants.push(Want {
                    regiment: m.handle,
                    act: Act::Move {
                        to,
                        facing: Some(self.forward),
                        pace: 0.0,
                    },
                    priority: 4,
                    group: 0,
                });
                return;
            }
            if !ctx.slip() {
                // Step back out of reach, away from the threat. Keep going
                // to the same place while it is still out of reach, rather
                // than giving a new order every look.
                let safe = |p: Vec2| p.distance(d.pos) - d.radius() > danger_at + 4.0;
                let to = match m.retreat_to.filter(|&p| safe(p)) {
                    Some(p) => p,
                    None => {
                        let away = (r.pos - d.pos).normalize_or(-self.forward);
                        let to = ctx.terrain.clamp(r.pos + away * 14.0, 3.0);
                        ctx.terrain.nearest_walkable(to, 6.0).unwrap_or(to)
                    }
                };
                m.retreat_to = Some(to);
                m.target = None;
                wants.push(Want {
                    regiment: m.handle,
                    act: Act::Move {
                        to,
                        facing: Some(self.forward),
                        pace: 0.0,
                    },
                    priority: 4,
                    group: 0,
                });
                return;
            }
        }
        m.retreat_to = None;
        // Shoot what helps most among those in range.
        let in_range = |f: &&&Seen| f.pos.distance(r.pos) <= range - 0.5;
        let score = |f: &Seen| {
            let mut s = -f.pos.distance(r.pos) / range.max(1.0);
            if f.engaged {
                s += 1.5;
            }
            if ctx.knobs.morale_targeting {
                if f.morale == MoraleState::Wavering {
                    s += 1.5 + ctx.persona.opportunism();
                }
                s += 0.4 * shaken_near(fighting, f) as f32;
                if view.unit(f).stalwart {
                    s -= 0.5;
                }
            }
            if view.unit(f).class == Class::Ranged {
                s += 0.5;
            }
            // Denser blocks take more arrows.
            s += 0.3 * (f32::from(f.models) / (f.width * f.depth).max(1.0)).min(2.0);
            if ctx.knobs.focus_fire {
                s += 0.2 * f32::from(count(on, f.handle));
            }
            s
        };
        let keep = m
            .target
            .and_then(|t| view.get(t))
            .filter(|t| !t.broken() && t.pos.distance(r.pos) <= range - 0.5);
        let best = fighting
            .iter()
            .filter(in_range)
            .map(|f| (score(f), f.handle))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        let pick = match (keep, best) {
            (Some(k), Some((s, h))) if s > score(k) + 1.5 => Some(h),
            (Some(k), _) => Some(k.handle),
            (None, b) => b.map(|(_, h)| h),
        };
        // With nothing standing in range, thin out routers before they rally.
        let pick = pick.or_else(|| {
            foes.iter()
                .filter(|f| f.broken() && f.pos.distance(r.pos) <= range - 0.5)
                .min_by(|a, b| a.pos.distance(r.pos).total_cmp(&b.pos.distance(r.pos)))
                .map(|f| f.handle)
        });
        match pick {
            Some(t) => {
                m.target = Some(t);
                wants.push(attack(m.handle, t, 2));
            }
            None => {
                m.target = None;
                let to = self.place(ctx, m.slot);
                wants.push(slot_move(m.handle, to, self.forward, pace));
            }
        }
    }

    /// Whether regiment `r` can meet enemy `f` straight in without
    /// overlapping a friend there: one fighting, or one nearer its own enemy
    /// and closing on it (where it will meet that enemy). Two regiments on
    /// one face of an enemy push into each other and both fall out of order;
    /// a second does better on another face, or elsewhere.
    pub(super) fn room_at(&self, r: &Seen, f: &Seen) -> bool {
        let at = meeting(r, f);
        let near = r.clearance(f);
        let reach = r.pos.distance(f.pos) + f.radius() + r.radius();
        self.taken.iter().all(|t| {
            t.handle == r.handle
                || t.pos.distance(f.pos) > reach
                || t.closing.is_some_and(|c| c >= near)
                || at.overlap(&t.block) <= ROOM
        })
    }

    /// Friends' ground in the fight as they stand now, for
    /// [`Captain::room_at`]: allies' fighting regiments, and ours fighting
    /// or closing on an enemy.
    pub(super) fn taken_ground(&self, view: &Perception, party: PlayerId) -> Vec<Taken> {
        view.regiments
            .iter()
            .filter(|g| !g.broken() && !view.hostile(party, g.owner))
            .filter_map(|g| {
                let (block, closing) = if g.engaged {
                    (*g, None)
                } else {
                    let e = self
                        .members
                        .iter()
                        .find(|m| m.handle == g.handle)
                        .and_then(|m| view.get(m.target?))
                        .filter(|e| !e.broken() && view.hostile(party, e.owner))?;
                    (meeting(g, e), Some(g.clearance(e)))
                };
                Some(Taken {
                    handle: g.handle,
                    pos: g.pos,
                    block,
                    closing,
                })
            })
            .collect()
    }

    /// The best enemy for a melee regiment to go for among `candidates`.
    /// `keep` is its current target, which it sticks with unless something
    /// is clearly better.
    pub(super) fn best_melee_target<'a>(
        &self,
        ctx: &mut Ctx,
        r: &Seen,
        candidates: impl Iterator<Item = &'a &'a Seen>,
        on: &[(RegimentHandle, u8)],
        keep: Option<RegimentHandle>,
    ) -> Option<RegimentHandle> {
        let view = ctx.view;
        let me = view.unit(r);
        let knobs = ctx.knobs;
        let party = ctx.party;
        let opportunism = ctx.persona.opportunism();
        let score = |f: &Seen| {
            let them = view.unit(f);
            let mut s = -f.pos.distance(r.pos) / 8.0;
            s += match f.side_facing(r.pos) {
                Side::Front => 0.0,
                Side::Flank => 1.0,
                Side::Rear => 1.8,
            };
            if f.engaged {
                s += 1.0;
            }
            let wavering = f.morale == MoraleState::Wavering;
            if Some(f.handle) == self.focus {
                s += 1.0;
            }
            if knobs.morale_targeting {
                let shaken = shaken_near_all(view, f) as f32;
                if wavering {
                    // About to break, and its rout costs every friend
                    // within 16 tiles 8 points: worth most beside others
                    // already shaken, which it may take with it.
                    s += 1.5 + opportunism + 0.8 * shaken;
                } else {
                    s += 0.5 * shaken;
                }
                if them.stalwart {
                    s -= 1.0;
                }
                s += 1.0 - f.strength();
            }
            if knobs.focus_fire {
                // Local odds: worth most where joining makes them two or
                // three to one (which breaks nerve), little where they
                // already are, and walking alone into a crowd is a poor idea
                // (less so if the one it hits is about to run).
                let (ours, theirs) = local_soldiers(view, party, f.pos, r.handle);
                let theirs = theirs.max(1.0);
                let before = ours / theirs;
                let after = (ours + f32::from(r.models)) / theirs;
                s += if before >= 2.0 {
                    -0.5
                } else if after >= 2.0 {
                    1.0
                } else if after >= 1.2 {
                    0.4
                } else if after >= 0.8 {
                    0.0
                } else if wavering && knobs.morale_targeting {
                    -0.2
                } else {
                    -0.8
                };
            }
            // Two on one is the aim; a third is better spent elsewhere.
            let n = i32::from(count(on, f.handle)) - i32::from(keep == Some(f.handle));
            s += if knobs.focus_fire {
                match n {
                    0 => 0.0,
                    1 => 0.8,
                    2 => -0.6,
                    _ => -2.0,
                }
            } else {
                -0.5 * n.max(0) as f32
            };
            if knobs.focus_fire && !self.room_at(r, f) {
                s -= NO_ROOM;
            }
            let hit = (50.0 + f32::from(me.stats.melee_attack)
                - f32::from(them.stats.melee_defence))
            .clamp(10.0, 90.0);
            s += (hit - 50.0) / 25.0;
            if them.class == Class::Ranged {
                s += 0.8;
            }
            s
        };
        let mut best: Option<(f32, RegimentHandle)> = None;
        let mut kept: Option<f32> = None;
        for f in candidates {
            let s = score(f);
            if Some(f.handle) == keep {
                kept = Some(s);
            }
            if best.is_none_or(|(b, _)| s > b) {
                best = Some((s, f.handle));
            }
        }
        match (kept, best) {
            (Some(k), Some((b, h))) => Some(if b > k + 1.0 { h } else { keep.unwrap() }),
            (None, b) => b.map(|(_, h)| h),
            (Some(_), None) => keep,
        }
    }
}
