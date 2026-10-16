//! Mopping up: searching for and hunting what is left of a beaten enemy.

use super::*;

impl Captain {
    /// Nobody left standing against us: hunters chase routers, everyone
    /// else stands in formation.
    pub(super) fn mop_up(
        &mut self,
        ctx: &mut Ctx,
        ours: &[&Seen],
        foes: &[&Seen],
        wants: &mut Vec<Want>,
    ) {
        if self.relayout && self.started {
            self.layout(ctx);
        }
        // Time spent here isn't time spent waiting for an enemy in sight.
        self.waiting_since = None;
        let our_c = centroid(ours);
        let sweep = self.search_for(ctx, ours, our_c);
        let mut sweepers: u16 = 0;
        for k in 0..self.members.len() {
            let m = self.members[k];
            let Some(r) = ctx.view.get(m.handle) else {
                continue;
            };
            // Ambushers stay hidden while nothing is in sight.
            if r.broken() || r.engaged || m.lost || m.role == Role::Ambusher {
                continue;
            }
            let mut m = m;
            // Fast troops chase; so does everyone at the easiest level.
            let hunts = m.class != Class::Ranged
                && (m.class == Class::Shock || ctx.knobs.slip_percent >= 30)
                && self.hunt(ctx, &mut m, r, our_c, foes, wants);
            if !hunts && self.started {
                m.target = None;
                let (to, facing) = match sweep {
                    // Abreast across the place, so whoever comes nearest
                    // the hiding place spots it: the first on it, then one
                    // gap right, one left, two right, ...
                    Some((spot, dir)) => {
                        let side = if sweepers % 2 == 1 { 1.0 } else { -1.0 };
                        let across = side * SWEEP * f32::from(sweepers.div_ceil(2));
                        sweepers += 1;
                        (place_at(ctx, spot, dir, Vec2::new(across, 0.0)), dir)
                    }
                    None => (self.place(ctx, m.slot), self.forward),
                };
                wants.push(slot_move(m.handle, to, facing, 0.0));
            }
            self.members[k] = m;
        }
    }

    /// With nothing standing in sight: an enemy it lost sight of may have
    /// rallied where it vanished (in a wood, as often as not), and the
    /// battle isn't over until it is found. Where to look, and which way
    /// to sweep, if there is anyone to look for. Only for what is left of
    /// a beaten army, or one it has gone to fetch: a whole army that steps
    /// into a wood is waiting there for it to walk in.
    pub(super) fn search_for(
        &mut self,
        ctx: &mut Ctx,
        ours: &[&Seen],
        our_c: Vec2,
    ) -> Option<(Vec2, Vec2)> {
        let view = ctx.view;
        let unseen = || {
            view.last_seen()
                .filter(|f| f.morale != MoraleState::Shattered)
        };
        let mine = Force::of(view, ours.iter().copied()).soldiers;
        if !self.fetching && Force::of(view, unseen()).soldiers > mine * FINISH_SHARE {
            self.search = None;
            return None;
        }
        // Seen again since, or gone from the field: nothing to look for there.
        self.search
            .take_if(|&mut (h, at)| !unseen().any(|f| f.handle == h && f.pos == at));
        if let Some(looked) = self.search.take_if(|&mut (_, at)| {
            let there = place_at(ctx, at, self.forward, Vec2::ZERO);
            ours.iter().any(|r| r.pos.distance(there) < SEARCHED)
        }) {
            self.searched.push(looked);
        }
        if self.search.is_none() {
            let searched = &self.searched;
            self.search = unseen()
                .filter(|f| !searched.contains(&(f.handle, f.pos)))
                .min_by(|a, b| a.pos.distance(our_c).total_cmp(&b.pos.distance(our_c)))
                .map(|f| (f.handle, f.pos));
            if self.search.is_some() {
                ctx.journal.note(
                    view.tick,
                    "Looking for them where they were last seen".to_string(),
                );
            }
        }
        let (_, spot) = self.search?;
        Some((spot, (spot - our_c).normalize_or(self.forward)))
    }

    /// Chase the nearest router within the leash. Returns whether it hunts.
    pub(super) fn hunt(
        &self,
        ctx: &Ctx,
        m: &mut Member,
        r: &Seen,
        our_c: Vec2,
        foes: &[&Seen],
        wants: &mut Vec<Want>,
    ) -> bool {
        let leash = ctx.knobs.hunt_leash;
        let quarry = foes
            .iter()
            // Routers can rally; shattered regiments are out for good.
            .filter(|f| f.morale == MoraleState::Routing)
            .filter(|f| f.pos.distance(our_c) <= leash)
            .min_by(|a, b| a.pos.distance(r.pos).total_cmp(&b.pos.distance(r.pos)));
        match quarry {
            Some(q) => {
                if m.role != Role::Hunter {
                    m.home = if m.role == Role::Recover {
                        m.home
                    } else {
                        m.role
                    };
                    m.role = Role::Hunter;
                }
                m.target = Some(q.handle);
                wants.push(Want {
                    regiment: m.handle,
                    act: Act::Attack(q.handle),
                    priority: 2,
                    group: 0,
                });
                true
            }
            None => {
                if m.role == Role::Hunter {
                    m.role = m.home;
                    m.target = None;
                }
                false
            }
        }
    }
}
