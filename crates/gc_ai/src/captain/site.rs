//! Picking the ground: line ends against cliffs and water, high ground, a stand
//! back from a river.

use super::*;

impl Captain {
    /// While forming up or holding, every few seconds: is there better ground
    /// close by to receive the enemy on? It scores the places the line could
    /// stand within reach by cover at its ends, rise over the ground in
    /// front and, with water between the armies, keeping back from the
    /// bank, against the distance to get there; and moves the line if one
    /// is clearly better (docs/18-ai.md, "Using ground").
    pub(super) fn choose_ground(&mut self, ctx: &mut Ctx, ours: &[&Seen], fighting: &[&Seen]) {
        let view = ctx.view;
        let now = view.tick;
        let t = ctx.terrain;
        if !ctx.knobs.ground || now < self.next_ground || self.line_width <= 0.0 {
            return;
        }
        // On a beat of its own, so several AIs don't all look in one tick.
        let phase = u32::from(ctx.party) * 13 % GROUND_TICKS;
        self.next_ground = (now / GROUND_TICKS + 1) * GROUND_TICKS + phase;
        let river = ground::crossing(t, self.anchor, self.enemy_at);
        if river.is_some() != self.river.is_some() && !self.crossing {
            let note = if river.is_some() {
                "Water between us: waiting for them to cross"
            } else {
                "No water between us now"
            };
            ctx.journal.note(now, note.to_string());
        }
        self.river = river;
        if !matches!(self.posture, Posture::Form | Posture::Hold)
            || self.front_gap(fighting) < GROUND_GAP
        {
            return;
        }
        let mine = Force::of(view, ours.iter().copied());
        let theirs = Force::of(view, fighting.iter().copied());
        let margin = self.estimate.map_or(0.0, |e| e.margin);
        // Cover is worth more the worse the odds.
        let cover_worth = COVER * (1.0 + 2.0 * (-margin).max(0.0));
        // Behind water, out of reach of archers on the far bank if they
        // shoot better.
        let bank_back = match river {
            Some(r) if theirs.ranged > mine.ranged * 1.15 => {
                BANK_BACK.max(theirs.range - (r.far - r.near) + 3.0)
            }
            _ => BANK_BACK,
        };
        let (anchor, forward, enemy_at) = (self.anchor, self.forward, self.enemy_at);
        let (width, depth, rear) = (self.line_width, self.line_depth, self.rear);
        // Where they would come out of the water: wait just behind it, to
        // fall on each regiment as it wades out.
        let wait_at = river
            .and_then(|r| ground::ford(t, anchor, forward, r, FORD_REACH))
            .map(|exit| exit - forward * (bank_back + depth * 0.5));
        let score = |p: Vec2| -> Option<(f32, f32, f32)> {
            let line = Line {
                centre: p,
                forward,
                width,
                depth,
            };
            if line.footing(t, rear) < FOOTING {
                return None;
            }
            let mut s = 0.0;
            let mut side = 0.0;
            for (cover, sign) in line.cover(t).into_iter().zip([-1.0, 1.0]) {
                if let Some(k) = cover {
                    let snug = if k <= 2.0 {
                        1.0
                    } else if k <= 4.0 {
                        0.5
                    } else {
                        0.25
                    };
                    s += cover_worth * snug;
                    side += sign;
                }
            }
            let rise = line.rise(t, 12.0).clamp(0.0, 3.0);
            s += HIGH_GROUND * rise;
            if river.is_some()
                && let Some(c) = ground::crossing(t, p, enemy_at)
                && c.near < bank_back
            {
                s -= (bank_back - c.near) / 4.0;
            }
            let moved = p - anchor;
            s -= moved.length() / MOVE_TILES;
            match wait_at {
                // Up to the ford is safe ground: they must cross to reach it.
                Some(w) => s -= p.distance(w) / 5.0,
                None => s -= moved.dot(forward).max(0.0) / FORWARD_TILES,
            }
            Some((s, side, rise))
        };
        let right = right_of(forward);
        let mut best = score(anchor).map(|(s, side, rise)| (s + STAY, anchor, side, rise));
        let mut look = |centre: Vec2, wide: f32, ahead: f32, behind: f32, step: f32| {
            let mut back = -ahead;
            while back <= behind {
                let mut across = -wide;
                while across <= wide {
                    let p = centre + right * across - forward * back;
                    if t.inside(p, 6.0)
                        && let Some((s, side, rise)) = score(p)
                        && best.is_none_or(|b| s > b.0)
                    {
                        best = Some((s, p, side, rise));
                    }
                    across += step;
                }
                back += step;
            }
            best
        };
        // A rough look around where the line stands and around the ford,
        // then a closer one around the best place found.
        let mut found = look(
            anchor,
            GROUND_ACROSS,
            GROUND_FORWARD,
            GROUND_BACK,
            GROUND_STEP,
        );
        if let Some(w) = wait_at {
            found = look(w, 8.0, 6.0, 6.0, GROUND_STEP);
        }
        let near = GROUND_STEP - 1.0;
        if let Some((_, p, _, _)) = found
            && p != anchor
        {
            found = look(p, near, near, near, 1.0);
        }
        let Some((_, to, side, rise)) = found else {
            return;
        };
        if to != anchor {
            self.anchor = to;
            let what = if side != 0.0 {
                format!(
                    "anchoring the {} on ground nobody crosses",
                    if side > 0.0 { "right" } else { "left" }
                )
            } else if wait_at.is_some() {
                "facing the ford".to_string()
            } else if river.is_some() {
                "standing back from the water".to_string()
            } else if rise >= 0.5 {
                "the high ground".to_string()
            } else {
                "a better place".to_string()
            };
            let note = format!("Taking ground: {what}");
            // A small step to better ground for the same reason isn't news.
            if note != self.ground_note || to.distance(anchor) > GROUND_NEWS {
                ctx.journal.note(now, note.clone());
                self.ground_note = note;
            }
        }
        if side != self.covered {
            self.covered = side;
            self.relayout = true;
        }
    }

    /// Whether `f` stands beyond water the line is waiting behind, from
    /// `r`'s side of it: not to be attacked across the water.
    pub(super) fn beyond_water(&self, terrain: &Terrain, r: &Seen, f: &Seen) -> bool {
        self.river.is_some()
            && !self.crossing
            && !terrain.wet(f.pos)
            && ground::crossing(terrain, r.pos, f.pos).is_some()
    }
}
