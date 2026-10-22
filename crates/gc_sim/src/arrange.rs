//! Arranging a mixed army on a group order (docs/08-ui-ux.md, "Group
//! formation"): melee in front, ranged behind them, cavalry on the flanks,
//! support in the middle and engines at the back, all facing the way the
//! order faces. A few presets change the shape, and a player can save the
//! shape their army stands in and use it again.
//!
//! Like [`formation::group_move`], this runs on the ordering player's
//! machine only and turns one order into one `MoveTo` per regiment; the
//! simulation never sees anything but those moves.
//!
//! Positions here are in the order's frame: `lateral` is tiles to the
//! right of the target as the group faces, `back` tiles behind it.

use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::formation::{self, GROUP_GAP, GROUP_LINE_GAP, GROUP_LINE_MAX, GroupMember, GroupOrder};
use crate::{Command, UnitStats};

/// Space between the infantry and the cavalry on its flanks, in tiles.
pub const FLANK_GAP: Fx = Fx::from_int(3);
/// The Deep preset's lines are no longer than this without a dragged width.
pub const DEEP_LINE_MAX: Fx = Fx::from_int(24);
/// Inside a Square, the lines of whoever it shelters are no longer than this.
pub const SQUARE_INNER_MAX: Fx = Fx::from_int(20);
/// Space between a Square's walls and whoever is inside, in tiles, and the
/// least room inside when nobody is.
pub const SQUARE_GAP: Fx = Fx::from_int(2);
/// Most regiments a saved shape keeps.
pub const MAX_SLOTS: usize = 64;

/// Where a regiment belongs in an arranged army.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Role {
    /// Infantry and monsters: the fighting line.
    #[default]
    Melee = 0,
    /// Bows, crossbows, spitters: behind the fighting line.
    Ranged = 1,
    /// Cavalry and fliers: the wings.
    Flanker = 2,
    /// Casters and the like: the middle, among the ranged.
    Support = 3,
    /// Engines, siege and workers: the back.
    Rear = 4,
}

impl Role {
    pub const ALL: [Role; 5] = [
        Role::Melee,
        Role::Ranged,
        Role::Flanker,
        Role::Support,
        Role::Rear,
    ];

    pub fn from_u8(v: u8) -> Option<Role> {
        Role::ALL.get(v as usize).copied()
    }

    /// A best guess from the stats alone, for regiment types with no class
    /// (the game sets the role from the unit's class when it has one): fast
    /// regiments ride the wings, regiments that shoot well stand behind.
    pub fn guess(stats: &UnitStats) -> Role {
        if stats.speed >= Fx::from_int(5) {
            Role::Flanker
        } else if stats.range >= Fx::from_int(10) {
            Role::Ranged
        } else {
            Role::Melee
        }
    }
}

/// The shapes an arranged group order can take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Preset {
    /// Melee in one line, ranged behind, cavalry split between the wings.
    #[default]
    Line,
    /// Narrow and deep, to hold a gap or march: melee lines one behind the
    /// other, then the ranged, the cavalry as a reserve, then the rest.
    Deep,
    /// Like Line, but every horse on one wing (the side most of them stand
    /// on now) to swing round and crush the enemy against the infantry.
    Hammer,
    /// Melee facing out on four sides, everyone else inside.
    Square,
    /// No arranging: side by side in the order they stand, as before.
    Plain,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::Line,
        Preset::Deep,
        Preset::Hammer,
        Preset::Square,
        Preset::Plain,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Preset::Line => "Line",
            Preset::Deep => "Deep",
            Preset::Hammer => "Hammer",
            Preset::Square => "Square",
            Preset::Plain => "Plain",
        }
    }

    /// One line on what it does, for tooltips and the help scroll.
    pub fn tip(self) -> &'static str {
        match self {
            Preset::Line => "Melee in front, ranged behind, cavalry on both wings.",
            Preset::Deep => "Narrow and deep: to hold a gap or march. Cavalry in reserve.",
            Preset::Hammer => "All cavalry on one wing, to hit the enemy's flank.",
            Preset::Square => "Melee face out on all four sides, the rest inside.",
            Preset::Plain => "Side by side as they stand, no arranging.",
        }
    }
}

/// One regiment's place in a saved shape, in the order's frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Slot {
    pub role: Role,
    pub lateral: Fx,
    pub back: Fx,
    /// Its facing, turned from the order's.
    pub turn: Angle,
    /// Its frontage in soldiers.
    pub files: u16,
}

/// A shape a player saved from how their army stood.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Shape {
    pub slots: Vec<Slot>,
}

impl Shape {
    /// The shape `members` stand in now, about their middle and their
    /// average facing, with the front-most regiment's centre on the line
    /// the order marks. `None` for fewer than two regiments.
    pub fn capture(members: &[GroupMember]) -> Option<Shape> {
        if members.len() < 2 {
            return None;
        }
        let centre = formation::centre_of(members);
        let n = members.len() as i64;
        let (mut x, mut y) = (0i64, 0i64);
        for m in members {
            let v = m.facing.to_vec();
            x += i64::from(v.x.raw());
            y += i64::from(v.y.raw());
        }
        let mean = FxVec2::new(Fx::from_raw((x / n) as i32), Fx::from_raw((y / n) as i32));
        let facing = if mean == FxVec2::ZERO {
            members[0].facing
        } else {
            Angle::of_vec(mean)
        };
        let frame = Frame::new(centre, facing);
        let mut slots: Vec<Slot> = members
            .iter()
            .take(MAX_SLOTS)
            .map(|m| Slot {
                role: m.role,
                lateral: frame.lateral(m.position),
                back: frame.back(m.position),
                turn: Angle(m.facing.0.wrapping_sub(facing.0)),
                files: m.files,
            })
            .collect();
        let front = slots.iter().map(|s| s.back).min().unwrap_or(Fx::ZERO);
        for s in &mut slots {
            s.back -= front;
        }
        Some(Shape { slots })
    }
}

/// How to arrange a group order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrangement<'a> {
    Preset(Preset),
    Saved(&'a Shape),
}

/// A three-quarter turn: facing left of the order's facing.
const LEFT_TURN: Angle = Angle(Angle::HALF.0 + Angle::QUARTER.0);

/// A regiment's place in the order's frame, before it becomes a command.
#[derive(Clone, Copy, Debug)]
struct Spot {
    member: usize,
    lateral: Fx,
    back: Fx,
    turn: Angle,
    /// New frontage; 0 keeps the regiment's own.
    files: u16,
}

/// The order's frame: where the group stands now, and which ways are
/// ahead and right.
#[derive(Clone, Copy)]
struct Frame {
    centre: FxVec2,
    ahead: FxVec2,
    right: FxVec2,
}

impl Frame {
    fn new(centre: FxVec2, facing: Angle) -> Frame {
        Frame {
            centre,
            ahead: facing.to_vec(),
            right: facing.wrapping_add(Angle::QUARTER).to_vec(),
        }
    }

    fn lateral(&self, p: FxVec2) -> Fx {
        (p - self.centre).dot(self.right)
    }

    fn back(&self, p: FxVec2) -> Fx {
        -(p - self.centre).dot(self.ahead)
    }
}

/// Turn a group order into one `MoveTo` per regiment, arranged by role
/// (see the module docs). A group of one, or [`Preset::Plain`], is a plain
/// [`formation::group_move`].
///
/// With a dragged width, Line and Hammer refit every regiment's frontage by
/// the same factor so the front line (wings included) spans the drag; Deep
/// keeps frontages and makes no line wider than the drag. A Square re-forms
/// every regiment to its raised frontage and a saved shape to the frontages
/// it saved; both take only the drag's place and facing.
pub fn arranged_move(
    members: &mut [GroupMember],
    order: GroupOrder,
    how: Arrangement,
    out: &mut Vec<Command>,
) {
    if members.len() < 2 {
        formation::group_move(members, order, out);
        return;
    }
    let centre = formation::centre_of(members);
    let facing = formation::facing_of(order, centre, members[0].facing);
    let frame = Frame::new(centre, facing);
    let mut spots = Vec::with_capacity(members.len());
    // Whether the members' frontages were changed and must be ordered.
    let reformed = match how {
        Arrangement::Preset(Preset::Plain) => {
            formation::group_move(members, order, out);
            return;
        }
        Arrangement::Preset(p @ (Preset::Line | Preset::Hammer)) => {
            battle_line(
                members,
                &frame,
                order.width,
                p == Preset::Hammer,
                &mut spots,
            );
            order.width.is_some()
        }
        Arrangement::Preset(Preset::Deep) => {
            deep(members, &frame, order.width, &mut spots);
            false
        }
        Arrangement::Preset(Preset::Square) => {
            // Back to the frontage they were raised with: a square of
            // blocks squeezed thin by an earlier narrow drag is all corners.
            for m in members.iter_mut().filter(|m| m.raised_files > 0) {
                m.files = m.raised_files.min(m.models.max(1));
            }
            square(members, &frame, &mut spots);
            true
        }
        Arrangement::Saved(shape) => {
            saved(members, &frame, shape, &mut spots);
            false
        }
    };
    let pace = formation::pace_of(members);
    for s in &spots {
        out.push(Command::MoveTo {
            regiment: members[s.member].handle,
            target: order.target + frame.right * s.lateral - frame.ahead * s.back,
            facing: Some(facing.wrapping_add(s.turn)),
            files: if reformed {
                members[s.member].files
            } else {
                s.files
            },
            pace,
        });
    }
}

/// The members of `role`, as indices, sorted left to right as they stand.
fn of_role(members: &[GroupMember], frame: &Frame, role: Role) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..members.len())
        .filter(|&i| members[i].role == role)
        .collect();
    sort_by(&mut idx, members, |m| frame.lateral(m.position));
    idx
}

/// Sort indices by a key of their member, ties by handle so the order is
/// the same on every machine.
fn sort_by(idx: &mut [usize], members: &[GroupMember], key: impl Fn(&GroupMember) -> Fx) {
    idx.sort_unstable_by_key(|&i| (key(&members[i]), members[i].handle.index));
}

/// Width of regiments side by side, `gap` apart.
fn row_width(members: &[GroupMember], row: &[usize], gap: Fx) -> Fx {
    let blocks = row
        .iter()
        .fold(Fx::ZERO, |w, &i| w + members[i].width(members[i].files));
    blocks + gap * (row.len().max(1) as i32 - 1)
}

fn row_depth(members: &[GroupMember], row: &[usize]) -> Fx {
    row.iter()
        .map(|&i| members[i].depth())
        .max()
        .unwrap_or(Fx::ZERO)
}

/// Split one band (already sorted left to right) into rows no wider than
/// `max`, of equal counts, the regiments furthest forward now in the front
/// row; each row left to right.
fn rows(members: &[GroupMember], frame: &Frame, band: &[usize], max: Fx) -> Vec<Vec<usize>> {
    if band.is_empty() {
        return Vec::new();
    }
    let total = row_width(members, band, GROUP_GAP);
    let lines = ((total.raw().max(1) as u32).div_ceil(max.raw().max(1) as u32) as usize)
        .clamp(1, band.len());
    if lines == 1 {
        return alloc::vec![band.to_vec()];
    }
    let mut by_depth = band.to_vec();
    sort_by(&mut by_depth, members, |m| frame.back(m.position));
    let per = band.len().div_ceil(lines);
    by_depth
        .chunks(per)
        .map(|c| {
            let mut row = c.to_vec();
            sort_by(&mut row, members, |m| frame.lateral(m.position));
            row
        })
        .collect()
}

/// Put `row` side by side, `gap` apart, centred `mid` tiles to the right,
/// `back` tiles behind the target.
fn lay(members: &[GroupMember], row: &[usize], mid: Fx, back: Fx, gap: Fx, spots: &mut Vec<Spot>) {
    let mut left = mid - row_width(members, row, gap) / 2;
    for &i in row {
        let w = members[i].width(members[i].files);
        spots.push(Spot {
            member: i,
            lateral: left + w / 2,
            back,
            turn: Angle::ZERO,
            files: 0,
        });
        left += w + gap;
    }
}

/// Stack `rows` behind one another, the first at `back` (its centre),
/// each centred `mid` tiles to the right. Returns where the last row's
/// rear edge is.
fn stack(
    members: &[GroupMember],
    rows: &[Vec<usize>],
    mid: Fx,
    mut back: Fx,
    spots: &mut Vec<Spot>,
) -> Fx {
    let mut last_depth: Option<Fx> = None;
    for row in rows {
        let depth = row_depth(members, row);
        if let Some(d) = last_depth {
            back += (d + depth) / 2 + GROUP_LINE_GAP;
        }
        lay(members, row, mid, back, GROUP_GAP, spots);
        last_depth = Some(depth);
    }
    back + last_depth.unwrap_or(Fx::ZERO) / 2
}

/// Stack the non-empty bands behind one another, the first row's front
/// edge at `edge`, centred `mid` tiles to the right, in rows no wider than
/// `max`. Returns where the last row's rear edge is.
fn stack_bands(
    members: &[GroupMember],
    frame: &Frame,
    bands: &[Vec<usize>],
    max: Fx,
    mid: Fx,
    mut edge: Fx,
    spots: &mut Vec<Spot>,
) -> Fx {
    for (k, band) in bands.iter().filter(|b| !b.is_empty()).enumerate() {
        let band_rows = rows(members, frame, band, max);
        if k > 0 {
            edge += GROUP_LINE_GAP;
        }
        let depth = row_depth(members, &band_rows[0]);
        edge = stack(members, &band_rows, mid, edge + depth / 2, spots);
    }
    edge
}

/// The ranged with support regiments in the middle of them, left to right.
fn ranged_band(members: &[GroupMember], frame: &Frame) -> Vec<usize> {
    let mut band = of_role(members, frame, Role::Ranged);
    let support = of_role(members, frame, Role::Support);
    let at = band.len() / 2;
    band.splice(at..at, support);
    band
}

/// Multiply every member's frontage by `num / den`, at least one file and
/// no more files than soldiers.
fn refit(members: &mut [GroupMember], num: i64, den: i64) {
    for m in members.iter_mut() {
        let share = i64::from(m.width(m.files).raw()) * num / den.max(1);
        let spacing = i64::from(m.file_spacing.raw().max(1));
        let files = (share + spacing / 2) / spacing;
        m.files = files.clamp(1, i64::from(m.models.max(1))) as u16;
    }
}

/// Width of a front row with its wings, `gap` between neighbours: the
/// wings stand [`FLANK_GAP`] off the row's ends.
fn front_width(members: &[GroupMember], row: &[usize], wings: [&[usize]; 2], gap: Fx) -> Fx {
    wings
        .iter()
        .filter(|w| !w.is_empty())
        .fold(row_width(members, row, gap), |total, w| {
            total + row_width(members, w, gap) + FLANK_GAP
        })
}

/// Line and Hammer: the first band (melee, else ranged, else the rest)
/// with the flankers on its wings, then the bands after it.
fn battle_line(
    members: &mut [GroupMember],
    frame: &Frame,
    width: Option<Fx>,
    hammer: bool,
    spots: &mut Vec<Spot>,
) {
    let flankers = of_role(members, frame, Role::Flanker);
    let mut bands: Vec<Vec<usize>> = [
        of_role(members, frame, Role::Melee),
        ranged_band(members, frame),
        of_role(members, frame, Role::Rear),
    ]
    .into_iter()
    .filter(|b| !b.is_empty())
    .collect();
    let (left, right) = if bands.is_empty() {
        // An army of horse alone is just a line of them.
        bands.push(flankers);
        (Vec::new(), Vec::new())
    } else if hammer || flankers.len() == 1 {
        // All on one wing: the side they stand on now.
        let side: i64 = flankers
            .iter()
            .map(|&i| i64::from(frame.lateral(members[i].position).raw()))
            .sum();
        if side < 0 {
            (flankers, Vec::new())
        } else {
            (Vec::new(), flankers)
        }
    } else {
        let half = flankers.len() / 2;
        (flankers[..half].to_vec(), flankers[half..].to_vec())
    };

    // With a drag the first band is one line, refitted so it spans the
    // drag with its wings; later bands make no line wider than the drag.
    let max = width.unwrap_or(GROUP_LINE_MAX);
    let first_rows = if let Some(width) = width {
        let blocks = front_width(members, &bands[0], [&left, &right], Fx::ZERO);
        let gaps = front_width(members, &bands[0], [&left, &right], GROUP_GAP) - blocks;
        let room = (width - gaps).max(Fx::ZERO);
        refit(members, i64::from(room.raw()), i64::from(blocks.raw()));
        alloc::vec![bands[0].clone()]
    } else {
        rows(members, frame, &bands[0], max)
    };
    // The front row with its wings is centred on the target.
    let front = &first_rows[0];
    let core = row_width(members, front, GROUP_GAP);
    let total = front_width(members, front, [&left, &right], GROUP_GAP);
    let lw = if left.is_empty() {
        Fx::ZERO
    } else {
        row_width(members, &left, GROUP_GAP)
    };
    let core_left = -total / 2
        + if left.is_empty() {
            Fx::ZERO
        } else {
            lw + FLANK_GAP
        };
    let mid = core_left + core / 2;
    lay(
        members,
        &left,
        core_left - FLANK_GAP - lw / 2,
        Fx::ZERO,
        GROUP_GAP,
        spots,
    );
    let right_mid = core_left + core + FLANK_GAP + row_width(members, &right, GROUP_GAP) / 2;
    lay(members, &right, right_mid, Fx::ZERO, GROUP_GAP, spots);

    let rear = stack(members, &first_rows, mid, Fx::ZERO, spots);
    stack_bands(
        members,
        frame,
        &bands[1..],
        max,
        mid,
        rear + GROUP_LINE_GAP,
        spots,
    );
}

/// Deep: every band in lines no wider than the drag or [`DEEP_LINE_MAX`],
/// one behind the other: melee, ranged, horse in reserve, then the rest.
/// The front row's centre is on the target, as in a Line.
fn deep(members: &[GroupMember], frame: &Frame, width: Option<Fx>, spots: &mut Vec<Spot>) {
    let bands = [
        of_role(members, frame, Role::Melee),
        ranged_band(members, frame),
        of_role(members, frame, Role::Flanker),
        of_role(members, frame, Role::Rear),
    ];
    let max = width.unwrap_or(DEEP_LINE_MAX);
    stack_bands(members, frame, &bands, max, Fx::ZERO, Fx::ZERO, spots);
    if let Some(first) = spots.first().map(|s| s.back) {
        for s in spots.iter_mut() {
            s.back -= first;
        }
    }
}

/// Square: whoever isn't melee in a block in the middle, the melee facing
/// out round it on four sides. The target is the square's centre.
fn square(members: &[GroupMember], frame: &Frame, spots: &mut Vec<Spot>) {
    let mut melee = of_role(members, frame, Role::Melee);
    if melee.is_empty() {
        deep(members, frame, None, spots);
        return;
    }
    // The inside, centred on the target: `a` and `b` are its half width
    // and depth.
    let inner = [
        ranged_band(members, frame),
        of_role(members, frame, Role::Flanker),
        of_role(members, frame, Role::Rear),
    ];
    let rear = stack_bands(
        members,
        frame,
        &inner,
        SQUARE_INNER_MAX,
        Fx::ZERO,
        Fx::ZERO,
        spots,
    );
    let mut a = SQUARE_GAP;
    for s in spots.iter_mut() {
        s.back -= rear / 2;
        let m = &members[s.member];
        a = a.max(s.lateral.abs() + m.width(m.files) / 2);
    }
    let b = SQUARE_GAP.max(rear / 2);

    // Who takes which wall: [front, back, left, right]. The front-most now
    // take the front, the rear-most the back, the rest the sides.
    let n = melee.len();
    let counts: [usize; 4] = if n == 3 {
        [1, 0, 1, 1]
    } else {
        core::array::from_fn(|k| n / 4 + usize::from(k < n % 4))
    };
    sort_by(&mut melee, members, |m| frame.back(m.position));
    let mut front = melee[..counts[0]].to_vec();
    let mut back_wall = melee[n - counts[1]..].to_vec();
    let mut sides = melee[counts[0]..n - counts[1]].to_vec();
    sort_by(&mut sides, members, |m| frame.lateral(m.position));
    let mut left = sides[..counts[2]].to_vec();
    let mut right = sides[counts[2]..].to_vec();
    for wall in [&mut front, &mut back_wall] {
        sort_by(wall, members, |m| frame.lateral(m.position));
    }
    for wall in [&mut left, &mut right] {
        sort_by(wall, members, |m| frame.back(m.position));
    }

    // Big enough that no wall runs past the ends of the walls across it.
    let half = |wall: &[usize]| row_width(members, wall, GROUP_GAP) / 2;
    let a = a.max(half(&front)).max(half(&back_wall));
    let b = b.max(half(&left)).max(half(&right));

    // Each wall is laid as a row across the order, then moved out to its
    // side; the side walls are turned on their side, so a row's left to
    // right runs front to back along the wall.
    for (wall, out, turn) in [
        (&front, -b, Angle::ZERO),
        (&back_wall, b, Angle::HALF),
        (&left, -a, LEFT_TURN),
        (&right, a, Angle::QUARTER),
    ] {
        let from = spots.len();
        let sign = if out < Fx::ZERO { -1 } else { 1 };
        let off = out + (SQUARE_GAP + row_depth(members, wall) / 2) * sign;
        lay(members, wall, Fx::ZERO, Fx::ZERO, GROUP_GAP, spots);
        let sideways = turn == LEFT_TURN || turn == Angle::QUARTER;
        for s in &mut spots[from..] {
            if sideways {
                s.back = s.lateral;
                s.lateral = off;
            } else {
                s.back = off;
            }
            s.turn = turn;
        }
    }
}

/// A saved shape: each regiment takes a slot of its own role, in left to
/// right order, spread across the slots when there are more slots than
/// regiments; then any slot left; then a line behind the rest.
fn saved(members: &[GroupMember], frame: &Frame, shape: &Shape, spots: &mut Vec<Spot>) {
    let mut slots: Vec<usize> = (0..shape.slots.len()).collect();
    slots.sort_unstable_by_key(|&k| (shape.slots[k].lateral, k));
    let mut used = alloc::vec![false; shape.slots.len()];
    let mut placed = alloc::vec![false; members.len()];
    // `who` take the free slots (of `role`, or any), left to right.
    let mut fill = |who: &[usize], role: Option<Role>, placed: &mut [bool]| {
        let free: Vec<usize> = (slots.iter().copied())
            .filter(|&k| !used[k] && role.is_none_or(|r| shape.slots[k].role == r))
            .collect();
        for (i, k) in spread(who, &free) {
            let slot = &shape.slots[k];
            used[k] = true;
            placed[i] = true;
            spots.push(Spot {
                member: i,
                lateral: slot.lateral,
                back: slot.back,
                turn: slot.turn,
                files: slot.files.clamp(1, members[i].models.max(1)),
            });
        }
    };
    for role in Role::ALL {
        fill(&of_role(members, frame, role), Some(role), &mut placed);
    }
    let mut rest: Vec<usize> = (0..members.len()).filter(|&i| !placed[i]).collect();
    sort_by(&mut rest, members, |m| frame.lateral(m.position));
    fill(&rest, None, &mut placed);
    rest.retain(|&i| !placed[i]);
    if !rest.is_empty() {
        let rear = spots
            .iter()
            .map(|s| s.back + members[s.member].depth() / 2)
            .max()
            .unwrap_or(Fx::ZERO);
        let depth = row_depth(members, &rest);
        lay(
            members,
            &rest,
            Fx::ZERO,
            rear + GROUP_LINE_GAP + depth / 2,
            GROUP_GAP,
            spots,
        );
    }
}

/// Pair `who` with `slots` in order: all of `who` when there are enough
/// slots, spread evenly across them; otherwise the first `slots.len()`.
fn spread(who: &[usize], slots: &[usize]) -> Vec<(usize, usize)> {
    let (m, s) = (who.len(), slots.len());
    if m >= s {
        return who.iter().copied().zip(slots.iter().copied()).collect();
    }
    if m == 1 {
        return alloc::vec![(who[0], slots[(s - 1) / 2])];
    }
    who.iter()
        .enumerate()
        .map(|(j, &i)| (i, slots[(j * (s - 1) + (m - 1) / 2) / (m - 1)]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RegimentHandle;

    use crate::scenario::NORTH;
    const TARGET: FxVec2 = FxVec2::from_ints(100, 20);

    /// A regiment of `role` standing at (x, 60) facing north: 24 soldiers
    /// in 6 files, a tile apart (6 wide, 4 deep); cavalry 8 in 4 files, 2
    /// tiles apart (8 wide, 4 deep).
    fn member(index: u32, role: Role, x: i32) -> GroupMember {
        let horse = role == Role::Flanker;
        GroupMember {
            handle: RegimentHandle {
                index,
                generation: 0,
            },
            position: FxVec2::from_ints(x, 60),
            facing: NORTH,
            files: if horse { 4 } else { 6 },
            models: if horse { 8 } else { 24 },
            file_spacing: Fx::from_int(if horse { 2 } else { 1 }),
            rank_spacing: Fx::ONE,
            speed: Fx::from_int(if horse { 6 } else { 3 }),
            role,
            raised_files: if horse { 4 } else { 6 },
        }
    }

    fn army(roles: &[Role]) -> Vec<GroupMember> {
        roles
            .iter()
            .enumerate()
            .map(|(i, &r)| member(i as u32, r, 20 + i as i32 * 9))
            .collect()
    }

    /// One placed regiment, in the order's frame (facing north at TARGET).
    #[derive(Clone, Copy, Debug)]
    struct Placed {
        index: u32,
        role: Role,
        lateral: f64,
        back: f64,
        turn: u16,
        /// Extent across and along the order's frame.
        across: f64,
        along: f64,
    }

    fn f(x: Fx) -> f64 {
        f64::from(x.raw()) / f64::from(Fx::ONE.raw())
    }

    fn arrange(members: &[GroupMember], order: GroupOrder, how: Arrangement) -> Vec<Placed> {
        let mut ms = members.to_vec();
        let mut out = Vec::new();
        arranged_move(&mut ms, order, how, &mut out);
        assert_eq!(out.len(), members.len());
        out.iter()
            .map(|c| {
                let Command::MoveTo {
                    regiment,
                    target,
                    facing,
                    files,
                    ..
                } = *c
                else {
                    unreachable!()
                };
                let m = members.iter().find(|m| m.handle == regiment).unwrap();
                let files = if files == 0 { m.files } else { files };
                let (w, d) = formation::block_size(files, m.models, m.file_spacing, m.rank_spacing);
                let turn = facing.unwrap().0.wrapping_sub(NORTH.0);
                let sideways = turn == Angle::QUARTER.0 || turn == 0xC000;
                let (across, along) = if sideways { (d, w) } else { (w, d) };
                Placed {
                    index: regiment.index,
                    role: m.role,
                    lateral: f(target.x - TARGET.x),
                    back: f(target.y - TARGET.y),
                    turn,
                    across: f(across),
                    along: f(along),
                }
            })
            .collect()
    }

    fn order(width: Option<i32>) -> GroupOrder {
        GroupOrder {
            target: TARGET,
            facing: Some(NORTH),
            width: width.map(Fx::from_int),
        }
    }

    fn no_overlaps(placed: &[Placed]) {
        for (k, a) in placed.iter().enumerate() {
            for b in &placed[k + 1..] {
                let apart_x = (a.lateral - b.lateral).abs() >= (a.across + b.across) / 2.0 - 1e-3;
                let apart_y = (a.back - b.back).abs() >= (a.along + b.along) / 2.0 - 1e-3;
                assert!(apart_x || apart_y, "{a:?} overlaps {b:?}");
            }
        }
    }

    fn of(placed: &[Placed], role: Role) -> Vec<Placed> {
        placed.iter().copied().filter(|p| p.role == role).collect()
    }

    use Role::{Flanker as Horse, Melee, Ranged, Rear, Support};

    const MIXED: [Role; 9] = [
        Ranged, Horse, Melee, Rear, Melee, Support, Ranged, Horse, Melee,
    ];

    #[test]
    fn line_puts_melee_in_front_and_horse_on_the_wings() {
        let placed = arrange(
            &army(&MIXED),
            order(None),
            Arrangement::Preset(Preset::Line),
        );
        no_overlaps(&placed);
        let melee = of(&placed, Melee);
        let horse = of(&placed, Horse);
        let ranged = of(&placed, Ranged);
        // The front line is on the target line, everyone faces north.
        assert!(placed.iter().all(|p| p.turn == 0));
        assert!(melee.iter().all(|p| p.back == 0.0));
        assert!(horse.iter().all(|p| p.back == 0.0));
        // One squadron beyond each end of the infantry.
        let (lo, hi) = melee.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p.lateral), hi.max(p.lateral))
        });
        assert!(horse.iter().any(|p| p.lateral < lo) && horse.iter().any(|p| p.lateral > hi));
        // Ranged behind the melee, support in among them, engines last.
        assert!(ranged.iter().all(|p| p.back > 4.0));
        let support = of(&placed, Support)[0];
        assert_eq!(support.back, ranged[0].back);
        assert!(ranged.iter().any(|p| p.lateral < support.lateral));
        assert!(ranged.iter().any(|p| p.lateral > support.lateral));
        let rear = of(&placed, Rear)[0];
        assert!(rear.back > support.back);
        // The front row, wings and all, is centred on the target.
        let front: Vec<Placed> = placed.iter().copied().filter(|p| p.back == 0.0).collect();
        let left = front
            .iter()
            .map(|p| p.lateral - p.across / 2.0)
            .fold(f64::MAX, f64::min);
        let right = front
            .iter()
            .map(|p| p.lateral + p.across / 2.0)
            .fold(f64::MIN, f64::max);
        assert!((left + right).abs() < 0.01, "{left} {right}");
    }

    #[test]
    fn a_drag_sets_the_whole_front_width() {
        for width in [30, 45, 60] {
            let placed = arrange(
                &army(&MIXED),
                order(Some(width)),
                Arrangement::Preset(Preset::Line),
            );
            no_overlaps(&placed);
            let front: Vec<Placed> = placed.iter().copied().filter(|p| p.back == 0.0).collect();
            assert_eq!(front.len(), 5, "{placed:?}");
            let left = front
                .iter()
                .map(|p| p.lateral - p.across / 2.0)
                .fold(f64::MAX, f64::min);
            let right = front
                .iter()
                .map(|p| p.lateral + p.across / 2.0)
                .fold(f64::MIN, f64::max);
            // Frontages are whole soldiers, so within a few tiles.
            assert!(
                (right - left - f64::from(width)).abs() <= 6.0,
                "{width}: {left}..{right}"
            );
        }
    }

    #[test]
    fn hammer_puts_every_horse_on_the_wing_they_stand_on() {
        // The horse stand at the east end of the army now.
        let roles = [Melee, Melee, Ranged, Horse, Horse];
        let placed = arrange(
            &army(&roles),
            order(None),
            Arrangement::Preset(Preset::Hammer),
        );
        no_overlaps(&placed);
        let max_melee = of(&placed, Melee)
            .iter()
            .map(|p| p.lateral)
            .fold(f64::MIN, f64::max);
        assert!(
            of(&placed, Horse).iter().all(|p| p.lateral > max_melee),
            "{placed:?}"
        );
    }

    #[test]
    fn deep_is_narrow_and_keeps_horse_in_reserve() {
        let roles = [
            Melee, Melee, Melee, Melee, Melee, Melee, Ranged, Ranged, Horse,
        ];
        let placed = arrange(
            &army(&roles),
            order(None),
            Arrangement::Preset(Preset::Deep),
        );
        no_overlaps(&placed);
        let left = placed
            .iter()
            .map(|p| p.lateral - p.across / 2.0)
            .fold(f64::MAX, f64::min);
        let right = placed
            .iter()
            .map(|p| p.lateral + p.across / 2.0)
            .fold(f64::MIN, f64::max);
        assert!(right - left <= f(DEEP_LINE_MAX), "{left}..{right}");
        let deepest = |r: Role| {
            of(&placed, r)
                .iter()
                .map(|p| p.back)
                .fold(f64::MIN, f64::max)
        };
        let shallowest = |r: Role| {
            of(&placed, r)
                .iter()
                .map(|p| p.back)
                .fold(f64::MAX, f64::min)
        };
        assert!(shallowest(Ranged) > deepest(Melee));
        assert!(shallowest(Horse) > deepest(Ranged));
        assert_eq!(shallowest(Melee), 0.0);
    }

    #[test]
    fn square_faces_out_and_shelters_the_rest() {
        for n_melee in 1..=7 {
            let mut roles = alloc::vec![Melee; n_melee];
            roles.extend([Ranged, Support, Horse]);
            let placed = arrange(
                &army(&roles),
                order(None),
                Arrangement::Preset(Preset::Square),
            );
            no_overlaps(&placed);
            let melee = of(&placed, Melee);
            // Each wall faces away from the middle.
            for p in &melee {
                let out = match p.turn {
                    0 => -p.back,
                    0x8000 => p.back,
                    0x4000 => p.lateral,
                    0xC000 => -p.lateral,
                    t => panic!("turned {t:#x}"),
                };
                assert!(out > 0.0, "{n_melee}: {p:?}");
            }
            assert!(melee.iter().any(|p| p.turn == 0), "{n_melee}: a front wall");
            // The sheltered stand inside the walls' reach.
            let reach = melee
                .iter()
                .map(|p| p.lateral.abs().max(p.back.abs()))
                .fold(0.0, f64::max);
            for p in placed.iter().filter(|p| p.role != Melee) {
                assert!(p.lateral.abs() < reach && p.back.abs() < reach, "{p:?}");
                assert_eq!(p.turn, 0);
            }
        }
    }

    #[test]
    fn a_saved_shape_comes_back_as_it_was() {
        // A hand-made shape: two blocks of foot side by side, archers
        // behind the right one, horse ahead on the left facing east; the
        // left foot face west, so on average the shape faces north.
        let mut members = army(&[Melee, Melee, Ranged, Horse]);
        let at = [(40, 60), (50, 60), (50, 68), (30, 54)];
        for (m, (x, y)) in members.iter_mut().zip(at) {
            m.position = FxVec2::from_ints(x, y);
        }
        members[3].facing = Angle::ZERO;
        members[0].facing = Angle::HALF;
        let shape = Shape::capture(&members).unwrap();
        let placed = arrange(&members, order(None), Arrangement::Saved(&shape));
        // Same offsets as they stood, the front-most (the horse) on the
        // target line, each turned as it was.
        let horse = placed.iter().find(|p| p.index == 3).unwrap();
        assert_eq!(horse.back, 0.0);
        assert_eq!(horse.turn, 0x4000);
        assert_eq!(placed.iter().find(|p| p.index == 0).unwrap().turn, 0xC000);
        for p in &placed {
            let (x, y) = at[p.index as usize];
            assert!(
                (p.lateral - horse.lateral - f64::from(x - 30)).abs() < 0.01,
                "{p:?}"
            );
            assert!((p.back - f64::from(y - 54)).abs() < 0.01, "{p:?}");
        }
    }

    #[test]
    fn a_saved_shape_fits_a_different_army() {
        let mut members = army(&[Melee, Melee, Melee, Ranged]);
        let shape = Shape::capture(&members).unwrap();
        // Fewer melee than melee slots: spread across them. Extra ranged
        // take what slots are left, then stand behind.
        members = army(&[Melee, Melee, Ranged, Ranged, Ranged]);
        let placed = arrange(&members, order(None), Arrangement::Saved(&shape));
        assert_eq!(placed.len(), 5);
        let melee = of(&placed, Melee);
        let spread = (melee[0].lateral - melee[1].lateral).abs();
        assert!(
            (spread - 18.0).abs() < 0.01,
            "outer melee slots: {placed:?}"
        );
        no_overlaps(&placed);
    }

    #[test]
    fn plain_and_single_regiments_are_plain_moves() {
        let members = army(&MIXED);
        let mut a = members.clone();
        let mut b = members.clone();
        let (mut out_a, mut out_b) = (Vec::new(), Vec::new());
        arranged_move(
            &mut a,
            order(None),
            Arrangement::Preset(Preset::Plain),
            &mut out_a,
        );
        formation::group_move(&mut b, order(None), &mut out_b);
        assert_eq!(out_a, out_b);
        let mut one = [members[0]];
        out_a.clear();
        arranged_move(
            &mut one,
            order(None),
            Arrangement::Preset(Preset::Square),
            &mut out_a,
        );
        assert!(matches!(
            out_a[..],
            [Command::MoveTo { pace: Fx::ZERO, .. }]
        ));
    }

    #[test]
    fn every_preset_keeps_blocks_apart() {
        let mixes: [&[Role]; 6] = [
            &MIXED,
            &[Horse, Horse, Horse],
            &[Ranged, Ranged, Support],
            &[Melee; 14],
            &[Rear, Melee, Horse],
            &[
                Melee, Melee, Melee, Melee, Ranged, Ranged, Ranged, Ranged, Horse, Horse, Horse,
                Horse, Rear, Rear, Support, Support,
            ],
        ];
        for roles in mixes {
            for preset in Preset::ALL {
                for width in [None, Some(20), Some(80)] {
                    let placed = arrange(&army(roles), order(width), Arrangement::Preset(preset));
                    no_overlaps(&placed);
                }
            }
        }
    }

    #[test]
    fn spread_pairs_in_order() {
        assert_eq!(spread(&[7], &[0, 1, 2]), [(7, 1)]);
        assert_eq!(spread(&[7, 8], &[0, 1, 2]), [(7, 0), (8, 2)]);
        assert_eq!(
            spread(&[7, 8, 9], &[0, 1, 2, 3, 4]),
            [(7, 0), (8, 2), (9, 4)]
        );
        assert_eq!(spread(&[7, 8, 9], &[0, 1]), [(7, 0), (8, 1)]);
    }
}
