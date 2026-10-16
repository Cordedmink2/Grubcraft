//! Picking a doctrine and laying the army out in it: scoring, roles and slots.

use super::*;

impl Captain {
    pub(super) fn choose_doctrine(&mut self, ctx: &mut Ctx, ours: &[&Seen], fighting: &[&Seen]) {
        let view = ctx.view;
        let count = |class: Class| ours.iter().filter(|r| view.unit(r).class == class).count();
        let (line, shock, ranged) = (
            count(Class::Line),
            count(Class::Shock),
            count(Class::Ranged),
        );
        let their_shock = fighting
            .iter()
            .filter(|r| view.unit(r).class == Class::Shock)
            .count();
        let mine = Force::of(view, ours.iter().copied());
        let theirs = Force::of(view, fighting.iter().copied());
        let margin = self.estimate.map_or(0.0, |e| e.margin);
        let p = ctx.persona;
        let hammers = (shock > 0 && line + ranged > 0).then(|| {
            0.4 + 0.4 * (shock as f32 / 3.0).min(1.0)
                + if shock > their_shock { 0.2 } else { 0.0 }
                + 0.3 * p.cunning()
        });
        let mut best: Option<(f32, Doctrine)> = None;
        for &d in ctx.knobs.doctrines {
            // An ambush has its own test, which keeps one already laid going
            // whatever the army has lost since.
            if d != Doctrine::Ambush && !fits(d, line, shock, ranged) {
                continue;
            }
            let fit = match d {
                Doctrine::HoldTheLine => {
                    0.5 + if mine.ranged > theirs.ranged {
                        0.3
                    } else {
                        0.0
                    } + if margin < 0.0 { 0.2 } else { 0.0 }
                        + 0.2 * p.caution()
                }
                Doctrine::HammerAndAnvil => match hammers {
                    Some(fit) => fit,
                    None => continue,
                },
                Doctrine::RefusedFlank => {
                    0.3 + if margin < -0.05 { 0.4 } else { 0.0 } + 0.3 * p.cunning()
                }
                Doctrine::SkirmishScreen => {
                    0.3 + if mine.ranged > theirs.ranged * 1.3 {
                        0.5
                    } else {
                        0.0
                    }
                }
                // Hammer and anvil with the hammers hidden (or, with no fast
                // troops, the ends of the line): scores on top of it when
                // there is somewhere to hide.
                Doctrine::Ambush => match self.ambush_fit(ctx, ours, fighting, (line, shock)) {
                    Some(extra) => hammers.unwrap_or(0.4 + 0.3 * p.cunning()) + extra,
                    None => continue,
                },
            };
            let jitter = ctx.rng.below(1000) as f32 / 1000.0 * 0.15;
            let stick = if !self.doctrine_chosen || d != self.doctrine {
                0.0
            } else {
                COMMITMENT
            };
            // A plan's doctrine wins, when this army can fight that way.
            let planned = self.plan.is_some_and(|plan| plan.doctrine == d);
            let score = if planned {
                f32::INFINITY
            } else {
                fit + 0.6 * p.doctrines.of(d) + jitter + stick
            };
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, d));
            }
        }
        let Some((_, pick)) = best else {
            return;
        };
        if pick != self.doctrine || !self.doctrine_chosen {
            self.doctrine_chosen = true;
            let why = match self.estimate {
                Some(e) => format!("expects {}", describe_margin(e.margin)),
                None => String::new(),
            };
            ctx.journal
                .note(view.tick, format!("{}: {why}", pick.name()));
            self.doctrine = pick;
            if pick == Doctrine::RefusedFlank {
                self.strong_side = self.weaker_enemy_side(fighting);
            }
            self.relayout = true;
        }
    }

    /// The side (+1 right, -1 left) where the enemy has fewer soldiers.
    pub(super) fn weaker_enemy_side(&self, fighting: &[&Seen]) -> f32 {
        let right = right_of(self.forward);
        let lean: f32 = fighting
            .iter()
            .map(|f| (f.pos - self.anchor).dot(right).signum() * f32::from(f.models))
            .sum();
        if lean > 0.0 { -1.0 } else { 1.0 }
    }

    /// Give every regiment a role and a place in the formation.
    pub(super) fn layout(&mut self, ctx: &mut Ctx) {
        self.relayout = false;
        let view = ctx.view;
        let right = right_of(self.forward);
        // Each member's size and where it stands across the frame now.
        let n = self.members.len();
        let mut width = vec![4.0; n];
        let mut depth = vec![2.0; n];
        let mut lateral = vec![0.0; n];
        for (k, m) in self.members.iter().enumerate() {
            if let Some(r) = view.get(m.handle) {
                width[k] = r.width;
                depth[k] = r.depth;
                lateral[k] = (r.pos - self.anchor).dot(right);
            }
        }
        let by_lateral = |a: &usize, b: &usize| lateral[*a].total_cmp(&lateral[*b]);
        let by_middle = |a: &usize, b: &usize| lateral[*a].abs().total_cmp(&lateral[*b].abs());
        let mut line: Vec<usize> = Vec::new();
        let mut shock: Vec<usize> = Vec::new();
        let mut ranged: Vec<usize> = Vec::new();
        for (k, m) in self.members.iter().enumerate() {
            if m.lost {
                continue;
            }
            match m.class {
                Class::Line => line.push(k),
                Class::Shock => shock.push(k),
                Class::Ranged => ranged.push(k),
            }
        }
        // An army of fast troops only (the Brood's Skitterlings) still needs
        // a line: the outermost third are hammers, the rest stand in it.
        if line.is_empty() && !shock.is_empty() {
            let keep = if shock.len() >= 3 { shock.len() / 3 } else { 0 };
            shock.sort_by(by_middle);
            line = shock.split_off(0);
            shock = line.split_off(line.len() - keep);
        }
        // The Ambush doctrine with no fast troops hides line regiments
        // instead: those already sent into hiding, then the outermost.
        if self.doctrine == Doctrine::Ambush && shock.is_empty() {
            let hiding = |k: usize| self.ambush.has(self.members[k].handle);
            line.sort_by(|&a, &b| hiding(b).cmp(&hiding(a)).then(by_middle(&b, &a)));
            let n = ambush::line_ambushers(line.len());
            shock = line.drain(..n).collect();
        }
        // The reserve: a line regiment from near the middle, if there are
        // enough to spare one.
        let mut reserve: Option<usize> = None;
        if ctx.knobs.reserve
            && line.len() >= RESERVE_FROM
            && self.doctrine != Doctrine::SkirmishScreen
        {
            let existing = line
                .iter()
                .copied()
                .find(|&k| self.members[k].home == Role::Reserve);
            let pick = existing.or_else(|| line.iter().copied().min_by(by_middle));
            if let Some(k) = pick {
                line.retain(|&x| x != k);
                reserve = Some(k);
            }
        }

        // The line, left to right as they stand now so nobody crosses.
        line.sort_by(by_lateral);
        let total: f32 = line.iter().map(|&k| width[k]).sum::<f32>()
            + LINE_GAP * line.len().saturating_sub(1) as f32;
        let line_depth = line.iter().map(|&k| depth[k]).fold(0.0, f32::max);
        self.line_depth = line_depth;
        self.line_width = total;
        let refused = self.doctrine == Doctrine::RefusedFlank;
        // A line with one end covered leads with that end, and refuses the
        // open one.
        let strong_side = if self.covered != 0.0 {
            self.covered
        } else {
            self.strong_side
        };
        let mut x = -total * 0.5;
        for (i, &k) in line.iter().enumerate() {
            let cx = x + width[k] * 0.5;
            x += width[k] + LINE_GAP;
            // Refused flank: the weak wing steps back at an angle.
            let back = if refused && cx * strong_side < 0.0 {
                cx.abs() * 0.55
            } else {
                0.0
            };
            let edge = i == 0 || i + 1 == line.len();
            let role = if edge && line.len() >= 3 {
                Role::Wing
            } else {
                Role::Anchor
            };
            self.assign(k, role, Vec2::new(cx, back));
        }

        // Archers: spread behind the line, or in front of it as a screen.
        ranged.sort_by(by_lateral);
        let rw: f32 = ranged.iter().map(|&k| width[k]).sum::<f32>()
            + 2.0 * LINE_GAP * ranged.len().saturating_sub(1) as f32;
        let spread = (total * 0.8).max(rw);
        let (first, step) = match (ranged.first(), ranged.last()) {
            (Some(&a), Some(&b)) if ranged.len() > 1 => {
                let first = -spread * 0.5 + width[a] * 0.5;
                let last = spread * 0.5 - width[b] * 0.5;
                (first, (last - first) / (ranged.len() - 1) as f32)
            }
            _ => (0.0, 0.0),
        };
        let screen = self.doctrine == Doctrine::SkirmishScreen;
        self.rear = ranged
            .iter()
            .map(|&k| BATTERY_BACK + depth[k])
            .fold(0.0, f32::max);
        // How far behind the line the archers standing behind it reach (not
        // a screen out in front).
        let mut archers_back: f32 = 0.0;
        for (i, &k) in ranged.iter().enumerate() {
            let cx = first + step * i as f32;
            let behind = Vec2::new(cx, line_depth * 0.5 + BATTERY_BACK + depth[k] * 0.5);
            let ahead = Vec2::new(cx, -(line_depth * 0.5 + SCREEN_AHEAD + depth[k] * 0.5));
            let m = &mut self.members[k];
            m.fallback = behind;
            if !screen {
                m.fell_back = false;
            }
            // A screen that already fell back stays back.
            if screen && !m.fell_back {
                self.assign(k, Role::Screen, ahead);
            } else {
                self.assign(k, Role::Battery, behind);
                archers_back = archers_back.max(BATTERY_BACK + depth[k]);
            }
        }

        // Hammers wait behind the ends of the line, where the line shields
        // them, ready to swing round a flank. Hammer and anvil holds them
        // further out, already round the ends. A refused flank puts them all
        // behind the strong wing, and a line with a covered end puts them
        // all on the open side (nobody gets round the covered one, either
        // way).
        shock.sort_by(by_lateral);
        let half = shock.len().div_ceil(2);
        let anvil = self.doctrine == Doctrine::HammerAndAnvil;
        let (mut left_x, mut right_x) = if anvil {
            (-total * 0.5 - HAMMER_OUT, total * 0.5 + HAMMER_OUT)
        } else {
            (-total * 0.5, total * 0.5)
        };
        for (i, &k) in shock.iter().enumerate() {
            let w = width[k];
            let to_right = if self.covered != 0.0 {
                self.covered < 0.0
            } else if refused {
                strong_side > 0.0
            } else {
                i >= shock.len() - half
            };
            // Outward from the line's end with Hammer and anvil, inward from
            // it (behind the wing) otherwise.
            let cx = match (to_right, anvil) {
                (true, true) => {
                    right_x += w + LINE_GAP;
                    right_x - LINE_GAP - w * 0.5
                }
                (false, true) => {
                    left_x -= w + LINE_GAP;
                    left_x + LINE_GAP + w * 0.5
                }
                (true, false) => {
                    right_x -= w + LINE_GAP;
                    right_x + LINE_GAP + w * 0.5
                }
                (false, false) => {
                    left_x += w + LINE_GAP;
                    left_x - LINE_GAP - w * 0.5
                }
            };
            // Behind the archers, not among them.
            let back = if anvil {
                line_depth * 0.5
            } else {
                line_depth * 0.5 + HAMMER_BACK.max(archers_back + LINE_GAP) + depth[k] * 0.5
            };
            self.assign(k, Role::Hammer, Vec2::new(cx, back));
        }
        if let Some(k) = reserve {
            let slot = Vec2::new(0.0, line_depth + RESERVE_BACK + depth[k] * 0.5);
            self.assign(k, Role::Reserve, slot);
        }
        self.lay_ambush(ctx);

        // The formation marches at the pace of its slowest foot regiment.
        let slowest = line
            .iter()
            .chain(&ranged)
            .chain(reserve.iter())
            .filter_map(|&k| view.get(self.members[k].handle))
            .map(|r| view.pace(r))
            .fold(f32::MAX, f32::min);
        self.pace = if slowest == f32::MAX { 0.0 } else { slowest };
    }

    /// Give member `k` a role and slot, keeping a temporary role (hunting,
    /// recovering) until it ends.
    pub(super) fn assign(&mut self, k: usize, role: Role, slot: Vec2) {
        let m = &mut self.members[k];
        m.home = role;
        m.slot = slot;
        if !matches!(m.role, Role::Hunter | Role::Recover) {
            if m.role != role {
                m.target = None;
                m.striking = false;
            }
            m.role = role;
        }
    }

    /// Where a slot is on the map, nudged onto ground a regiment can stand on.
    pub(super) fn place(&self, ctx: &Ctx, slot: Vec2) -> Vec2 {
        place_at(ctx, self.anchor, self.forward, slot)
    }

    /// Goes to fetch an enemy that neither comes nor fights once that has
    /// gone on for `seconds` (since `was`, if it had already begun).
    pub(super) fn fetch_after(&mut self, ctx: &mut Ctx, was: Option<u32>, seconds: f32, why: &str) {
        let now = ctx.view.tick;
        let since = *self.waiting_since.insert(was.unwrap_or(now));
        if (now - since) as f32 / TICKS > seconds {
            self.fetching = true;
            ctx.journal.note(now, format!("{why}: going to fetch them"));
        }
    }
}
