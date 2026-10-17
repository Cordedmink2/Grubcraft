//! Free helpers shared by the Captain's submodules.

use super::*;

/// `r`'s block standing against `f`, having come straight at its centre.
pub(super) fn meeting(r: &Seen, f: &Seen) -> Seen {
    let dir = (f.pos - r.pos).normalize_or(-f.facing);
    Seen {
        pos: f.pos - dir * (f.half_extent(dir) + 0.5 * r.depth + MEET_GAP),
        facing: dir,
        ..*r
    }
}

/// Whether an army with this many line, shock and ranged regiments can
/// fight as `d` asks: a hammer needs fast troops and something to pin the
/// enemy with, a refused flank a line long enough to bend, a screen archers
/// and a line to fall back behind, an ambush regiments to hide (and, on
/// the field, somewhere to hide them: see `Captain::may_ambush`).
pub fn fits(d: Doctrine, line: usize, shock: usize, ranged: usize) -> bool {
    match d {
        Doctrine::HoldTheLine => true,
        Doctrine::HammerAndAnvil => shock > 0 && line + ranged > 0,
        Doctrine::RefusedFlank => line >= 4,
        Doctrine::SkirmishScreen => ranged >= 2 && line > 0,
        Doctrine::Ambush => ambush::ambusher_class(line, shock).is_some(),
    }
}

/// Tiles between two blocks' edges, roughly (their circles).
pub fn gap_between(a: &Seen, b: &Seen) -> f32 {
    a.pos.distance(b.pos) - a.depth * 0.5 - b.radius() * 0.8
}

pub(crate) fn centroid(rs: &[&Seen]) -> Vec2 {
    let mut sum = Vec2::ZERO;
    let mut n = 0.0;
    for r in rs {
        let w = f32::from(r.models.max(1));
        sum += r.pos * w;
        n += w;
    }
    if n > 0.0 { sum / n } else { Vec2::ZERO }
}

/// The centre of the hostile army nearest `from` (the one the line faces
/// when several enemies are on the field).
pub(super) fn nearest_army(_party: PlayerId, from: Vec2, fighting: &[&Seen]) -> Vec2 {
    let mut best: Option<(f32, Vec2)> = None;
    let mut owners: Vec<PlayerId> = fighting.iter().map(|f| f.owner).collect();
    owners.sort_unstable();
    owners.dedup();
    for o in owners {
        let army: Vec<&Seen> = fighting.iter().copied().filter(|f| f.owner == o).collect();
        let c = centroid(&army);
        let d = c.distance(from);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, c));
        }
    }
    best.map_or(from, |(_, c)| c)
}

/// Where `slot` of a line standing at `anchor` facing `forward` is on the
/// map, nudged onto ground a regiment can stand on.
pub(super) fn place_at(ctx: &Ctx, anchor: Vec2, forward: Vec2, slot: Vec2) -> Vec2 {
    let p = ctx
        .terrain
        .clamp(anchor + right_of(forward) * slot.x - forward * slot.y, 3.0);
    if ctx.terrain.walkable(p) {
        p
    } else {
        ctx.terrain.nearest_walkable(p, 8.0).unwrap_or(p)
    }
}

/// The middle of the line as it stands (its line regiments, or everyone).
pub(super) fn line_centre(members: &[Member], ours: &[&Seen], fallback: Vec2) -> Vec2 {
    let line: Vec<&Seen> = ours
        .iter()
        .copied()
        .filter(|r| {
            members
                .iter()
                .any(|m| m.handle == r.handle && matches!(m.home, Role::Anchor | Role::Wing))
        })
        .collect();
    if line.is_empty() {
        fallback
    } else {
        centroid(&line)
    }
}

/// Soldiers still fighting within [`LOCAL_TILES`] of `p`: on `party`'s
/// side (leaving out `skip`) and against it.
pub(super) fn local_soldiers(
    view: &Perception,
    party: PlayerId,
    p: Vec2,
    skip: RegimentHandle,
) -> (f32, f32) {
    let (mut ours, mut theirs) = (0.0, 0.0);
    for g in &view.regiments {
        if g.handle == skip || g.broken() || g.pos.distance(p) > LOCAL_TILES {
            continue;
        }
        if view.hostile(party, g.owner) {
            theirs += f32::from(g.models);
        } else {
            ours += f32::from(g.models);
        }
    }
    (ours, theirs)
}

/// Whether the way from `from` to `to` passes clear of enemy melee
/// regiments that are free to turn on whoever goes by.
pub(super) fn path_clear(
    view: &Perception,
    fighting: &[&Seen],
    from: Vec2,
    to: Vec2,
    skip: RegimentHandle,
) -> bool {
    let d = to - from;
    let len2 = d.length_squared().max(1e-6);
    fighting.iter().all(|f| {
        if f.handle == skip || f.engaged || view.unit(f).class == Class::Ranged {
            return true;
        }
        let t = ((f.pos - from).dot(d) / len2).clamp(0.0, 1.0);
        (from + d * t).distance(f.pos) > f.radius() + 5.0
    })
}

/// Archers with no melee regiment of theirs close enough to protect them.
pub(super) fn exposed(view: &Perception, fighting: &[&Seen], f: &Seen) -> bool {
    view.unit(f).class == Class::Ranged
        && !fighting.iter().any(|g| {
            g.handle != f.handle
                && g.owner == f.owner
                && view.unit(g).class != Class::Ranged
                && g.pos.distance(f.pos) < 12.0
        })
}

/// The enemy regiment whose rout would do most harm, among the wavering
/// within [`FOCUS_TILES`] of the line at `anchor`: the one with most shaken
/// friends beside it, closest to breaking. None if none is wavering.
pub(super) fn point_of_attack(
    view: &Perception,
    fighting: &[&Seen],
    anchor: Vec2,
) -> Option<RegimentHandle> {
    fighting
        .iter()
        .filter(|f| f.morale == MoraleState::Wavering && !view.unit(f).stalwart)
        .filter(|f| f.pos.distance(anchor) <= FOCUS_TILES)
        .map(|f| {
            let s = shaken_near_all(view, f) as f32 + (1.0 - f.strength())
                - f32::from(f.morale_points) / 25.0
                - f.pos.distance(anchor) / FOCUS_TILES;
            (s, f.handle)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, h)| h)
}

/// Shaken enemies within 16 tiles of `f` (a rout there would cascade).
pub(super) fn shaken_near(fighting: &[&Seen], f: &Seen) -> usize {
    fighting
        .iter()
        .filter(|g| g.handle != f.handle && g.shaken() && g.pos.distance(f.pos) < 16.0)
        .count()
}

pub(super) fn shaken_near_all(view: &Perception, f: &Seen) -> usize {
    view.regiments
        .iter()
        .filter(|g| g.owner == f.owner && g.handle != f.handle && g.shaken() && !g.broken())
        .filter(|g| g.pos.distance(f.pos) < 16.0)
        .count()
}

pub(super) fn count(on: &[(RegimentHandle, u8)], h: RegimentHandle) -> u8 {
    on.iter().find(|(x, _)| *x == h).map_or(0, |(_, n)| *n)
}

pub(super) fn bump(on: &mut Vec<(RegimentHandle, u8)>, h: RegimentHandle) {
    match on.iter_mut().find(|(x, _)| *x == h) {
        Some((_, n)) => *n += 1,
        None => on.push((h, 1)),
    }
}

pub(super) fn unbump(on: &mut [(RegimentHandle, u8)], h: RegimentHandle) {
    if let Some((_, n)) = on.iter_mut().find(|(x, _)| *x == h) {
        *n = n.saturating_sub(1);
    }
}

/// An attack. Regiments sent at the same enemy in the same look go
/// together as one order, like selecting them all and clicking it once.
pub(super) fn attack(regiment: RegimentHandle, target: RegimentHandle, priority: u8) -> Want {
    Want {
        regiment,
        act: Act::Attack(target),
        priority,
        group: crate::orders::attack_group(target),
    }
}

/// A move to a place in the formation. These go out together as one order.
pub(super) fn slot_move(regiment: RegimentHandle, to: Vec2, facing: Vec2, pace: f32) -> Want {
    Want {
        regiment,
        act: Act::Move {
            to,
            facing: Some(facing),
            pace,
        },
        priority: 1,
        group: 1,
    }
}

/// "a close fight", "a clear win", ... for the journal.
pub fn describe_margin(m: f32) -> &'static str {
    match m {
        m if m > 0.5 => "a clear win",
        m if m > 0.15 => "to win",
        m if m > -0.15 => "a close fight",
        m if m > -0.5 => "to lose",
        _ => "a heavy defeat",
    }
}

pub(super) fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}
