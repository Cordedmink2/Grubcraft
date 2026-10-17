//! Choosing a posture: forming up, holding, advancing or falling back.

use super::*;

impl Captain {
    pub(super) fn choose_posture(&mut self, ctx: &mut Ctx, ours: &[&Seen], fighting: &[&Seen]) {
        let view = ctx.view;
        let now = view.tick;
        let p = ctx.persona;
        let before = self.posture;
        // Only kept while it goes on waiting, below.
        let was_waiting = self.waiting_since.take();
        let engaged = ours.iter().any(|r| r.engaged);
        // How far the enemy is from where the line stands now (the anchor
        // is where it is going, which while advancing is near the enemy).
        let line_now = line_centre(&self.members, ours, centroid(ours));
        let gap = self.front_gap_from(line_now, fighting);
        let mine = Force::of(view, ours.iter().copied());
        let theirs = Force::of(view, fighting.iter().copied());
        let margin = self.estimate.map_or(0.0, |e| e.margin);

        if self.posture == Posture::Form {
            let placed = self
                .members
                .iter()
                .filter(|m| !m.lost)
                .filter_map(|m| view.get(m.handle).map(|r| (m, r)))
                .filter(|(m, r)| r.engaged || r.pos.distance(self.place(ctx, m.slot)) < 4.0)
                .count();
            let total = self.members.iter().filter(|m| !m.lost).count().max(1);
            let formed = placed as f32 / total as f32 >= FORMED_SHARE;
            if formed || now >= self.formed_at + FORM_TICKS || gap < 25.0 {
                self.posture = Posture::Hold;
                self.standoff_since = now;
            }
        }

        // Losing badly while the enemy is still well off and not coming on:
        // fall back to better odds rather than fight on. Never with the
        // enemy close, where turning away only invites a charge in the back.
        let threshold = -(0.15 + 0.35 * p.caution());
        let well_off = gap > WITHDRAW_GAP && self.closing < 1.0;
        // Waiting for two other sides to fight, with one coming at it
        // first: step out of its way, so it finds the other.
        let shy = self.shy.filter(|_| gap > SHY_GAP);
        if (margin < threshold && well_off) || shy.is_some() {
            let since = *self.losing_since.get_or_insert(if shy.is_some() {
                now.saturating_sub(WITHDRAW_AFTER_TICKS)
            } else {
                now
            });
            let recently = self
                .withdrawn_at
                .is_some_and(|t| now < t + 30 * TICKS_PER_SECOND);
            if now >= since + WITHDRAW_AFTER_TICKS
                && !recently
                && self.withdrawals < WITHDRAWALS
                && self.posture != Posture::Withdraw
            {
                self.posture = Posture::Withdraw;
                self.withdrawn_at = Some(now);
                // Stepping out of others' way doesn't use up its falling
                // back from a losing fight.
                if shy.is_none() {
                    self.withdrawals += 1;
                }
                // Away from the enemy it faces, or keeping out of a fight,
                // away from both sides in it.
                let away = shy.unwrap_or(-self.forward);
                let back = ctx.terrain.clamp(self.anchor + away * WITHDRAW_TILES, 8.0);
                self.anchor = back;
                let why = if shy.is_some() {
                    "keeping out of their fight".to_string()
                } else {
                    format!("expects {}", describe_margin(margin))
                };
                ctx.journal.note(now, format!("Falling back: {why}"));
            }
        } else {
            self.losing_since = None;
        }
        if self.posture == Posture::Withdraw {
            let home = ours
                .iter()
                .filter(|r| r.pos.distance(self.anchor) < self.line_width * 0.5 + 10.0)
                .count();
            let done = home * 3 >= ours.len() * 2
                || now >= self.withdrawn_at.unwrap_or(now) + 20 * TICKS_PER_SECOND;
            if done || gap < WITHDRAW_GAP * 0.5 {
                self.posture = Posture::Hold;
                self.standoff_since = now;
            }
        }

        if matches!(self.posture, Posture::Hold | Posture::Advance) {
            let our_range = self.battery_reach(ctx);
            let only_archers = theirs.melee_regiments == 0.0;
            // They are coming: receive them where we stand, unless keen to
            // meet them in the open.
            let coming = self.closing > 1.0 && gap < 60.0;
            let meet = p.aggression() >= 0.7 && gap > 45.0 && !self.lying_in_wait();
            // Outshot only matters once their archers can reach us.
            let under_fire = theirs.ranged > 0.0 && gap < theirs.range + 2.0;
            // Under the trees their arrows do less: no need to go to them.
            let sheltered = ctx.knobs.read_ground && self.sheltered(ctx);
            let outshot = theirs.outshoots(&mine) && under_fire && !sheltered;
            let out_of_range = mine.ranged > 0.0 && gap > our_range;
            let losing = margin < LOSING;
            let patience = 3.0 + 25.0 * p.patience() - 10.0 * p.aggression()
                + if margin > 0.25 { -5.0 } else { 0.0 }
                + if self.lying_in_wait() {
                    ambush::WAIT_LONGER
                } else {
                    0.0
                };
            // Its side's champion is fighting between the lines: stand and
            // watch. The time spent watching doesn't use up its patience.
            let watching = view
                .champions
                .is_some_and(|sides| sides.iter().any(|&s| !view.hostile(ctx.party, s)));
            match (watching, self.watching_since) {
                (true, None) => self.watching_since = Some(now),
                (false, Some(since)) => {
                    self.standoff_since = (self.standoff_since + (now - since)).min(now);
                    self.watching_since = None;
                }
                _ => {}
            }
            let waited = (now - self.standoff_since) as f32 / TICKS;
            // Water between the armies: whoever crosses comes out of it a
            // regiment at a time, so wait for them, unless clearly the
            // stronger and tired of waiting.
            if self.river.is_some()
                && !self.crossing
                && margin > CROSS_MARGIN
                && waited > 2.0 * patience.max(5.0)
            {
                self.crossing = true;
                self.baiting = false;
                if ctx.knobs.read_ground {
                    self.bridgehead = self.bridgehead_at(ctx, ours, fighting);
                }
                ctx.journal.note(
                    now,
                    format!("Crossing the water: expects {}", describe_margin(margin)),
                );
            }
            // Both sides waiting for the other to cross: go over where they
            // aren't watching, if anywhere, unless the water is all that
            // keeps them off. Once a second: it walks the whole bank.
            let quiet = self.closing <= 1.0 && !engaged;
            if ctx.knobs.read_ground
                && self.river.is_some()
                && !self.crossing
                && quiet
                && waited > site::RIVER_WAIT
                && margin > BAIT_MARGIN
                && now >= self.next_ford_look
            {
                self.next_ford_look = now + site::FORD_LOOK_TICKS;
                match self.bridgehead_at(ctx, ours, fighting) {
                    Some(b) => {
                        self.crossing = true;
                        self.baiting = false;
                        self.bridgehead = Some(b);
                        ctx.journal
                            .note(now, "Crossing at a ford they aren't watching".to_string());
                    }
                    // They watch every ford: draw back from the water, to
                    // tempt them across.
                    None if !self.baiting => {
                        self.baiting = true;
                        ctx.journal.note(
                            now,
                            "Drawing back from the water to tempt them across".to_string(),
                        );
                    }
                    None => {}
                }
            }
            let behind_water = self.river.is_some() && !self.crossing;
            // Behind water it waits for them; while its champion fights it
            // watches; whatever the plan.
            let halted = behind_water || watching;
            let advance = if halted {
                false
            } else if only_archers || self.pouncing || self.bridgehead.is_some() {
                // Falling on two sides at grips, or going over to the far
                // bank: whatever the odds.
                true
            } else if self.onlooking.is_some() {
                // Two other sides are about to fight: let them.
                false
            } else if coming {
                meet && !losing
            } else if losing {
                // Wait on our own ground for them to come. Being shot to
                // pieces with no falling back left is the exception. An
                // enemy that never comes has to be fetched in the end, or
                // the battle never ends. Only time with nobody coming and
                // nobody fighting counts.
                if !engaged {
                    self.fetch_after(ctx, was_waiting, TIRED_OF_WAITING, "Tired of waiting");
                }
                outshot && self.withdrawals >= WITHDRAWALS
            } else {
                // Ambushers lying in wait need the enemy to come to them,
                // archers in range or not.
                outshot || (out_of_range && !self.lying_in_wait()) || waited > patience.max(0.0)
            };
            let advance = match self.plan.map(|plan| plan.stance) {
                _ if self.pressing => !halted,
                Some(Stance::Advance) => !halted && self.onlooking.is_none(),
                Some(Stance::Hold) => {
                    // Holding still gives up on an enemy that never comes,
                    // or the battle never ends: once nobody has come on or
                    // fought for longer than anyone waits before going to
                    // fetch an enemy, so it never gives up a standoff first.
                    let quiet = self.closing <= 1.0 && !engaged;
                    if !halted && !only_archers && quiet && !self.fetching {
                        self.fetch_after(
                            ctx,
                            was_waiting,
                            2.0 * TIRED_OF_WAITING,
                            "Tired of waiting",
                        );
                    }
                    !halted && (only_archers || self.pouncing || self.bridgehead.is_some())
                }
                _ => advance,
            };
            let want = if advance {
                Posture::Advance
            } else {
                Posture::Hold
            };
            // Give each posture a moment before changing it again, so the
            // line doesn't stop and start with every look.
            let settled = now >= self.posture_since + POSTURE_TICKS;
            if want != self.posture && (settled || watching) {
                self.posture = want;
                if want == Posture::Advance && self.covered != 0.0 {
                    // Leaving its cover behind: hammers on both wings again.
                    self.covered = 0.0;
                    self.relayout = true;
                }
                if want == Posture::Hold {
                    // Halt where the line stands now; one halting to watch
                    // keeps the patience it had left.
                    if watching {
                        ctx.journal
                            .note(now, "Halting to watch the champions".to_string());
                    } else {
                        self.standoff_since = now;
                    }
                    let our_c = centroid(ours);
                    self.anchor = ctx
                        .terrain
                        .clamp(line_centre(&self.members, ours, our_c), 4.0);
                }
            }
            if self.posture == Posture::Advance {
                // March until the archers are in range and let them work,
                // if they shoot better than the enemy's and there is still
                // patience for it. Otherwise right up to the enemy, and the
                // charges take it from there.
                let shoot = mine.ranged > 0.0
                    && mine.outshoots(&theirs)
                    && !only_archers
                    && waited < 2.0 * patience.max(5.0);
                let shoot_from = if shoot {
                    (our_range - 2.0).max(STAND_OFF)
                } else {
                    STAND_OFF
                };
                let step = (self.front_gap(fighting) - shoot_from).max(0.0);
                let goal = ctx.terrain.clamp(self.anchor + self.forward * step, 6.0);
                if goal.distance(self.anchor) >= ANCHOR_SLACK || step < ANCHOR_SLACK {
                    self.anchor = goal;
                }
                // The line is at the enemy, as it sees it, yet nobody comes
                // to blows: the survivors of an army it broke have rallied
                // here and there (some even behind it), too scattered for a
                // line to march on. Go and fetch them one by one.
                let arrived = !shoot
                    && step < ANCHOR_SLACK
                    && line_now.distance(self.anchor) < 2.0 * ANCHOR_SLACK;
                if arrived && !coming && !engaged && margin >= LOSING && !self.fetching {
                    self.fetch_after(ctx, was_waiting, NOBODY_FIGHTS, "Nobody stands to fight");
                }
            }
        }
        if self.posture != before {
            self.posture_since = now;
            ctx.journal.note(
                now,
                format!(
                    "{} ({})",
                    capitalised(self.posture.name()),
                    self.doctrine.name()
                ),
            );
        }
    }

    /// How far in front of the line its archers can reach, tiles.
    pub(super) fn battery_reach(&self, ctx: &Ctx) -> f32 {
        self.members
            .iter()
            .filter(|m| matches!(m.role, Role::Battery | Role::Screen))
            .filter_map(|m| ctx.view.get(m.handle).map(|r| (m, r)))
            .map(|(m, r)| ctx.view.unit(r).range - m.slot.y - self.line_depth * 0.5)
            .fold(0.0, f32::max)
    }
}
