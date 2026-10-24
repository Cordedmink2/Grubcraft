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
        if river.is_none() {
            self.baiting = false;
        }
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
        let bank_back = if self.baiting {
            bank_back.max(BAIT_BACK)
        } else {
            bank_back
        };
        let (anchor, forward, enemy_at) = (self.anchor, self.forward, self.enemy_at);
        let (width, depth, rear) = (self.line_width, self.line_depth, self.rear);
        // Where they would come out of the water: wait just behind it, to
        // fall on each regiment as it wades out.
        let wait_at = river
            .and_then(|r| ground::ford(t, anchor, forward, r, FORD_REACH))
            .map(|exit| exit - forward * (bank_back + depth * 0.5));
        let read = ctx.knobs.read_ground;
        let outshot = theirs.outshoots(&mine);
        let hill_worth = HILL * (1.0 + (-margin).max(0.0));
        let our_c = centroid(ours);
        let enemy_front = fighting
            .iter()
            .map(|f| f.pos)
            .min_by(|a, b| a.distance(our_c).total_cmp(&b.distance(our_c)))
            .unwrap_or(enemy_at);
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
            // Reading the ground: high enough to fight downhill is worth a
            // great deal more than a gentle rise; and against better
            // archers, trees to catch their arrows.
            if read && line.rise(t, SLOPE_AHEAD) >= ground::DOWNHILL {
                s += hill_worth;
            }
            if read && outshot {
                s += WOODS * line.wooded(t);
            }
            if river.is_some()
                && let Some(c) = ground::crossing(t, p, enemy_at)
                && c.near < bank_back
            {
                s -= (bank_back - c.near) / 4.0;
            }
            let moved = p - anchor;
            s -= moved.length() / MOVE_TILES;
            // Ground we reach well before them is ours to take: going
            // forward to it costs no more than going anywhere else.
            let first = read && p.distance(enemy_front) > p.distance(our_c) + FIRST_BY;
            match wait_at {
                // Up to the ford is safe ground: they must cross to reach it.
                Some(w) => s -= p.distance(w) / 5.0,
                None if first => {}
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
        } else if read {
            // And further forward, for a hill to take before they do.
            found = look(anchor, GROUND_ACROSS, HILL_REACH, 0.0, 2.0 * GROUND_STEP);
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

/// Behind water with nobody coming and nobody fighting for this long, a
/// Captain that reads the ground goes across after all, at a ford the
/// enemy isn't watching (seconds): two armies each waiting for the other
/// to cross would otherwise wait for ever.
pub(super) const RIVER_WAIT: f32 = 45.0;
/// A ford is unwatched when it comes out at least this far from every
/// enemy regiment, tiles; and worth going for only if it is at most this
/// much further from the army than from the enemy (who must see us go
/// before they move).
const UNWATCHED: f32 = 30.0;
const LEAD: f32 = 20.0;
/// Drawing back to tempt the enemy across, the line stands this far back
/// from the water, tiles: far enough that the ford is no longer watched.
const BAIT_BACK: f32 = 36.0;
/// How often it looks along the bank for a ford to cross at, ticks.
pub(super) const FORD_LOOK_TICKS: u32 = 20;
/// How far along the river it looks for fords, tiles to either side.
const FORD_SCAN: f32 = 90.0;
/// Share of the army across before the bridgehead is held and the line
/// fights on from there.
const ACROSS_SHARE: f32 = 0.75;

impl Captain {
    /// Where to cross the water it waits behind: beyond the far bank of the
    /// ford least watched by the enemy, against the walk there. None if
    /// every ford comes out near them.
    /// Returns where the line will stand and where the ford comes out.
    pub(super) fn bridgehead_at(
        &self,
        ctx: &Ctx,
        ours: &[&Seen],
        fighting: &[&Seen],
    ) -> Option<(Vec2, Vec2)> {
        let t = ctx.terrain;
        let river = self.river?;
        let our_c = centroid(ours);
        let watched = |p: Vec2| {
            fighting
                .iter()
                .map(|f| f.pos.distance(p))
                .fold(f32::MAX, f32::min)
        };
        ground::fords(t, self.anchor, self.forward, river, FORD_SCAN)
            .into_iter()
            // Unwatched, and not much further from us than from them: we
            // get there before they can.
            .filter_map(|f| {
                let theirs = watched(f.exit);
                if theirs < UNWATCHED || theirs + LEAD < f.exit.distance(our_c) {
                    return None;
                }
                let beyond = f.exit + self.forward * (self.line_depth * 0.5 + 3.0);
                let at = t.nearest_walkable(beyond, 6.0).unwrap_or(beyond);
                let score = theirs.min(60.0) - f.entry.distance(our_c) / 3.0;
                Some((score, (at, f.exit)))
            })
            .filter(|(_, (at, _))| !t.wet(*at))
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, b)| b)
    }

    /// Crossing to a bridgehead: the line's place is beyond the ford until
    /// most of the army is across, and it fights on from there.
    /// If the enemy comes to watch the ford before anyone has gone into the
    /// water, it goes back to waiting.
    pub(super) fn hold_bridgehead(&mut self, ctx: &mut Ctx, ours: &[&Seen], fighting: &[&Seen]) {
        let Some((b, exit)) = self.bridgehead else {
            return;
        };
        let t = ctx.terrain;
        let wading = ours.iter().any(|r| t.wet(r.pos));
        let across = ours
            .iter()
            .filter(|r| !t.wet(r.pos) && ground::crossing(t, r.pos, b).is_none())
            .count();
        let watched = fighting
            .iter()
            .any(|f| f.pos.distance(exit) < UNWATCHED * 0.7);
        if watched && !wading && across == 0 {
            self.bridgehead = None;
            self.crossing = false;
            self.standoff_since = ctx.view.tick;
            self.posture = Posture::Hold;
            self.posture_since = ctx.view.tick;
            self.anchor = t.clamp(line_centre(&self.members, ours, centroid(ours)), 4.0);
            ctx.journal.note(
                ctx.view.tick,
                "They have come to watch that ford: waiting again".to_string(),
            );
            return;
        }
        if across as f32 >= ours.len() as f32 * ACROSS_SHARE {
            self.bridgehead = None;
            ctx.journal.note(
                ctx.view.tick,
                "Across the water: forming up on the far bank".to_string(),
            );
            return;
        }
        self.anchor = b;
        if self.posture != Posture::Advance {
            self.posture = Posture::Advance;
            self.posture_since = ctx.view.tick;
        }
    }
}

/// Tiles in front of a line whose ground the other side will stand on to
/// fight it.
const SLOPE_AHEAD: f32 = 6.0;
/// What standing high enough to fight downhill is worth, before odds (a
/// covered end of the line is worth 1).
const HILL: f32 = 0.8;
/// How far forward it looks for a hill to take, tiles; and how much nearer
/// us than the nearest enemy a place must be to be ours to take.
const HILL_REACH: f32 = 40.0;
const FIRST_BY: f32 = 15.0;
/// What a line wholly in a wood is worth against better archers.
const WOODS: f32 = 0.8;

impl Captain {
    /// The line standing at `centre`.
    fn line_at(&self, centre: Vec2) -> Line {
        Line {
            centre,
            forward: self.forward,
            width: self.line_width,
            depth: self.line_depth,
        }
    }

    /// Who will fight downhill where the lines meet (Captains that read the
    /// ground only): Greater us, Less them, Equal neither. They meet at our line when
    /// the enemy is coming on, and at theirs otherwise (we would have to go
    /// to them). The side standing [`ground::DOWNHILL`] above the ground in
    /// front of it fights downhill.
    pub(super) fn slope(&self, ctx: &Ctx, fighting: &[&Seen]) -> Ordering {
        if !ctx.knobs.read_ground || fighting.is_empty() {
            return Ordering::Equal;
        }
        let t = ctx.terrain;
        let (line, ahead, ours) = if self.closing > 1.0 {
            (self.anchor, self.forward, true)
        } else {
            let theirs: Vec<&Seen> = fighting
                .iter()
                .copied()
                .filter(|f| self.counts_against(f.owner))
                .collect();
            if theirs.is_empty() {
                return Ordering::Equal;
            }
            (centroid(&theirs), -self.forward, false)
        };
        let rise = t.height(line) - t.height(line + ahead * SLOPE_AHEAD);
        let higher = if rise >= ground::DOWNHILL {
            Ordering::Greater
        } else if rise <= -ground::DOWNHILL {
            Ordering::Less
        } else {
            Ordering::Equal
        };
        if ours { higher } else { higher.reverse() }
    }
}

/// Share of the line in a wood from which it counts as sheltered.
const SHELTER: f32 = 0.5;

impl Captain {
    /// Whether the line stands mostly in a wood, where better archers
    /// can't shoot it to pieces.
    pub(super) fn sheltered(&self, ctx: &Ctx) -> bool {
        self.line_at(self.anchor).wooded(ctx.terrain) >= SHELTER
    }
}
