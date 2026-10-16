//! Team games (docs/18-ai.md, "Team games"): with more than two armies on
//! the field, who is coming for whom.
//!
//! Every army on the field is turned towards the nearest army hostile to it
//! (as the Captain faces the nearest enemy army). From that each Captain
//! works out:
//! - **whom it is really fighting:** the army it faces and every enemy army
//!   turned on its side, with the friends turned on those. An enemy busy
//!   with a third side doesn't count against it, nor a friend busy
//!   elsewhere: in a 2v2v2 nobody expects to fight four armies at once.
//! - **a fight to wait out:** when the army it faces is turned on a third
//!   side hostile to both and no enemy is coming for it, it lets the two
//!   wear each other down, and falls on them once they are locked in melee.

use super::*;

/// An army on the field: a party's regiments still fighting.
#[derive(Clone, Copy, Debug)]
struct Army {
    party: PlayerId,
    centre: Vec2,
    /// The party whose army it is turned towards.
    faces: Option<PlayerId>,
}

/// Who faces whom, as of this look.
#[derive(Clone, Debug, Default)]
pub struct Field {
    armies: Vec<Army>,
}

impl Field {
    /// The armies in `view`, each turned towards its nearest hostile army
    /// (by centres; the lower party on a tie, so every mind agrees).
    pub fn of(view: &Perception) -> Field {
        let mut parties: Vec<PlayerId> = view
            .regiments
            .iter()
            .filter(|r| !r.broken() && r.models > 0)
            .map(|r| r.owner)
            .collect();
        parties.sort_unstable();
        parties.dedup();
        let armies: Vec<Army> = parties
            .into_iter()
            .map(|party| {
                let rs: Vec<&Seen> = view
                    .regiments
                    .iter()
                    .filter(|r| r.owner == party && !r.broken() && r.models > 0)
                    .collect();
                Army {
                    party,
                    centre: centroid(&rs),
                    faces: None,
                }
            })
            .collect();
        let mut field = Field { armies };
        for i in 0..field.armies.len() {
            let a = field.armies[i];
            field.armies[i].faces = field.nearest(a.centre, |b| view.hostile(a.party, b));
        }
        field
    }

    /// The army nearest `from` (by centres; the lower party on a tie) of the
    /// parties `pick` picks.
    fn nearest(&self, from: Vec2, pick: impl Fn(PlayerId) -> bool) -> Option<PlayerId> {
        self.armies
            .iter()
            .filter(|a| pick(a.party))
            .min_by(|x, y| {
                let (dx, dy) = (x.centre.distance(from), y.centre.distance(from));
                dx.total_cmp(&dy).then(x.party.cmp(&y.party))
            })
            .map(|a| a.party)
    }

    /// The party `party`'s army is turned towards.
    pub fn faces(&self, party: PlayerId) -> Option<PlayerId> {
        self.armies
            .iter()
            .find(|a| a.party == party)
            .and_then(|a| a.faces)
    }

    /// Whether more than two sides are in the fight: some two armies on
    /// the field are hostile to each other but not both hostile to `party`.
    pub fn three_sided(&self, view: &Perception, party: PlayerId) -> bool {
        self.armies.iter().any(|a| {
            self.armies.iter().any(|b| {
                view.hostile(a.party, b.party)
                    && view.hostile(party, a.party)
                    && view.hostile(party, b.party)
            })
        })
    }

    /// Whom `party` is really fighting: the parties against it (the one it
    /// faces and every enemy turned on its side) and with it (itself and
    /// every friend turned on one of those).
    pub fn sides(&self, view: &Perception, party: PlayerId) -> (Vec<PlayerId>, Vec<PlayerId>) {
        let facing = self.faces(party);
        let against: Vec<PlayerId> = self
            .armies
            .iter()
            .filter(|a| view.hostile(party, a.party))
            .filter(|a| Some(a.party) == facing || a.faces.is_some_and(|f| !view.hostile(party, f)))
            .map(|a| a.party)
            .collect();
        let with: Vec<PlayerId> = self
            .armies
            .iter()
            .filter(|a| !view.hostile(party, a.party))
            .filter(|a| a.party == party || a.faces.is_some_and(|f| against.contains(&f)))
            .map(|a| a.party)
            .collect();
        (against, with)
    }

    /// Two other sides who would fight each other: the army `party` faces
    /// (its nearest enemy), and the nearest army hostile to both.
    pub fn others_fighting(
        &self,
        view: &Perception,
        party: PlayerId,
    ) -> Option<(PlayerId, PlayerId)> {
        let me = self.armies.iter().find(|a| a.party == party)?;
        let first = me.faces?;
        let second = self.nearest(me.centre, |b| {
            view.hostile(party, b) && view.hostile(first, b)
        })?;
        Some((first, second))
    }
}

/// Tiles between regiments within which an enemy army is on us: an
/// onlooker keeps it further off than this while it can.
const ONLOOK_NEAR: f32 = 40.0;
/// An onlooker stands at least this much further from the nearer of the
/// two other sides than the other does, tiles.
const ONLOOK_SPARE: f32 = 10.0;

/// Whether some regiment of `a`'s still fighting is within `tiles` of one of
/// `b`'s.
fn within(view: &Perception, a: PlayerId, b: PlayerId, tiles: f32) -> bool {
    let fighting = |p: PlayerId| {
        view.regiments
            .iter()
            .filter(move |r| r.owner == p && !r.broken())
    };
    fighting(a).any(|x| fighting(b).any(|y| x.pos.distance(y.pos) < tiles))
}

/// Whether `party`'s melee regiments are mostly locked in a fight (half of
/// them or more) with regiments whose owners `with` picks.
pub(super) fn locked(view: &Perception, party: PlayerId, with: impl Fn(PlayerId) -> bool) -> bool {
    let (mut n, mut engaged) = (0, 0);
    for r in view
        .regiments
        .iter()
        .filter(|r| r.owner == party && !r.broken() && view.unit(r).class != Class::Ranged)
    {
        n += 1;
        // Who it is fighting: the nearest enemy of its touching it.
        let foe = view
            .regiments
            .iter()
            .filter(|e| e.engaged && view.hostile(party, e.owner) && e.clearance(r) < 1.0)
            .min_by(|x, y| x.pos.distance(r.pos).total_cmp(&y.pos.distance(r.pos)));
        if r.engaged && foe.is_some_and(|e| with(e.owner)) {
            engaged += 1;
        }
    }
    n > 0 && engaged * 2 >= n
}

impl Captain {
    /// Whether `owner`'s regiments count against it: in a three-sided
    /// fight, only those of the enemies it is really fighting.
    pub(super) fn counts_against(&self, owner: PlayerId) -> bool {
        self.against.is_empty() || self.against.contains(&owner)
    }

    /// Whether `owner`'s regiments fight beside it: in a three-sided
    /// fight, only friends turned on the same enemies.
    pub(super) fn counts_with(&self, owner: PlayerId) -> bool {
        self.with.is_empty() || self.with.contains(&owner)
    }

    /// In a team game, read who faces whom: whom it is really fighting,
    /// and whether two other sides are about to fight each other.
    pub(super) fn read_sides(&mut self, ctx: &mut Ctx) {
        self.against.clear();
        self.with.clear();
        let before = self.onlooking.take();
        self.shy = None;
        if !ctx.knobs.team_play {
            return;
        }
        let view = ctx.view;
        let field = Field::of(view);
        if !field.three_sided(view, ctx.party) {
            // Down to two sides: an ordinary fight from here.
            self.pouncing = false;
            self.onlook_since = None;
            return;
        }
        (self.against, self.with) = field.sides(view, ctx.party);
        if self.pouncing
            || self
                .onlook_since
                .is_some_and(|t| view.tick > t + ONLOOK_TICKS)
        {
            // Once it has gone in, or waited long enough, it fights on.
            return;
        }
        self.onlooking = field.others_fighting(view, ctx.party);
        let Some((a, b)) = self.onlooking else {
            return;
        };
        // One of them is coming at us before they have come to blows, or we
        // stand nearer the first than the second does: keep out of its way
        // while we can, so it turns on the other.
        let centre = |p: PlayerId| field.armies.iter().find(|x| x.party == p).map(|x| x.centre);
        let (Some(us), Some(ca), Some(cb)) = (centre(ctx.party), centre(a), centre(b)) else {
            return;
        };
        let nearer = us.distance(ca) < cb.distance(ca) + ONLOOK_SPARE;
        if nearer
            || [a, b]
                .iter()
                .any(|&p| within(view, ctx.party, p, ONLOOK_NEAR))
        {
            // Away from both; standing between them, away from the middle
            // of the map (towards our own edge).
            let away = (us - ca).normalize_or_zero() + (us - cb).normalize_or_zero();
            let middle = Vec2::splat(ctx.terrain.map_tiles() as f32 * 0.5);
            self.shy = Some(if away.length() > 0.5 {
                away.normalize()
            } else {
                (us - middle).normalize_or(-self.forward)
            });
        }
        self.onlook_since.get_or_insert(view.tick);
        // Either locked in melee with anyone but our side (each other, or
        // someone else): fall on the nearer one now, while it can't turn to
        // meet us.
        let others = |o: PlayerId| view.hostile(ctx.party, o);
        if locked(view, a, others) || locked(view, b, others) {
            self.pouncing = true;
            self.onlooking = None;
            self.shy = None;
            ctx.journal.note(
                view.tick,
                format!("Falling on player {} while it fights", a + 1),
            );
        } else if before.is_none_or(|(x, y)| x.min(y) != a.min(b) || x.max(y) != a.max(b)) {
            ctx.journal.note(
                view.tick,
                format!("Letting player {} and player {} fight it out", a + 1, b + 1),
            );
        }
    }
}
