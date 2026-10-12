//! Formation geometry: where each soldier stands inside a regiment's block,
//! and how a group of regiments lines up for a group order.
//!
//! A block is `files` soldiers wide and `ranks` deep. Rank 0 is the front.
//! Local coordinates are in tiles with `x` to the regiment's right and `y`
//! toward its rear, centred on the block.

use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::{Command, RegimentHandle};

/// Most soldiers a regiment can have. The largest regiments in the design are
/// 24; this leaves room without making per-soldier storage large.
pub const MAX_MODELS: u16 = 32;

/// Space left between regiments standing side by side after a group order.
pub const GROUP_GAP: Fx = Fx::from_int(1);

/// Number of ranks for `models` soldiers in `files` files (at least 1).
pub fn ranks(files: u16, models: u16) -> u16 {
    models.div_ceil(files.max(1)).max(1)
}

/// Size of the block: width across the front and depth front to back.
pub fn block_size(files: u16, models: u16, file_spacing: Fx, rank_spacing: Fx) -> (Fx, Fx) {
    let files = files.clamp(1, models.max(1));
    (
        file_spacing * files as i32,
        rank_spacing * ranks(files, models) as i32,
    )
}

/// Local position of slot `k` (see the module docs). A partial last rank is
/// centred behind the full ones.
pub fn slot_offset(k: u16, files: u16, models: u16, file_spacing: Fx, rank_spacing: Fx) -> FxVec2 {
    let files = files.clamp(1, models.max(1));
    let ranks = ranks(files, models) as i32;
    let rank = (k / files) as i32;
    let file = (k % files) as i32;
    let in_rank = if rank == ranks - 1 {
        models as i32 - (ranks - 1) * files as i32
    } else {
        files as i32
    };
    FxVec2::new(
        Fx::from_raw((file_spacing * (2 * file - (in_rank - 1))).raw() / 2),
        Fx::from_raw((rank_spacing * (2 * rank - (ranks - 1))).raw() / 2),
    )
}

/// Map a local formation position to the map, for a block centred at
/// `centre` and facing `facing`.
pub fn local_to_world(centre: FxVec2, facing: Angle, local: FxVec2) -> FxVec2 {
    centre + facing.wrapping_add(Angle::QUARTER).rotate(local)
}

/// One regiment taking part in a group order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupMember {
    pub handle: RegimentHandle,
    pub position: FxVec2,
    pub facing: Angle,
    /// Its current frontage in soldiers, and how many soldiers it has.
    pub files: u16,
    pub models: u16,
    /// Distance between neighbouring soldiers in a rank, in tiles.
    pub file_spacing: Fx,
    /// Its own marching speed, tiles per second.
    pub speed: Fx,
}

impl GroupMember {
    fn width(&self, files: u16) -> Fx {
        block_size(files, self.models, self.file_spacing, Fx::ONE).0
    }
}

/// Where a group order sends its regiments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupOrder {
    /// Centre of the line the regiments form.
    pub target: FxVec2,
    /// Facing on arrival; by default the direction from the group's centre to
    /// the target.
    pub facing: Option<Angle>,
    /// Length of the whole line in tiles (a right-drag). Each regiment's
    /// frontage is refitted so the line spans about this much; `None` keeps
    /// every regiment's frontage.
    pub width: Option<Fx>,
}

/// Turn a group order into one `MoveTo` per regiment: the regiments stand side
/// by side, centred on the target, all facing the same way, and march at the
/// slowest member's speed so they arrive together.
///
/// Members are reordered left to right by where they stand now, so blocks
/// don't cross each other's paths. Runs on the ordering player's machine only;
/// the commands it produces are what the lockstep turn carries.
pub fn group_move(members: &mut [GroupMember], order: GroupOrder, out: &mut Vec<Command>) {
    let Some(&first) = members.first() else {
        return;
    };
    let n = members.len() as i32;
    let mut sum = (0i64, 0i64);
    for m in members.iter() {
        sum.0 += m.position.x.raw() as i64;
        sum.1 += m.position.y.raw() as i64;
    }
    let centre = FxVec2::new(
        Fx::from_raw((sum.0 / n as i64) as i32),
        Fx::from_raw((sum.1 / n as i64) as i32),
    );
    let facing = order.facing.unwrap_or_else(|| {
        let travel = order.target - centre;
        if travel == FxVec2::ZERO {
            first.facing
        } else {
            Angle::of_vec(travel)
        }
    });
    let right = facing.wrapping_add(Angle::QUARTER).to_vec();
    members.sort_unstable_by_key(|m| ((m.position - centre).dot(right), m.handle.index));

    // Refit frontages: each regiment gets a share of the line in proportion
    // to its current width.
    if let Some(width) = order.width {
        let current: i64 = members.iter().map(|m| m.width(m.files).raw() as i64).sum();
        let room = (width - GROUP_GAP * (n - 1)).max(Fx::ZERO).raw() as i64;
        for m in members.iter_mut() {
            let share = room * m.width(m.files).raw() as i64 / current.max(1);
            let spacing = m.file_spacing.raw().max(1) as i64;
            let files = (share + spacing / 2) / spacing;
            m.files = files.clamp(1, m.models.max(1) as i64) as u16;
        }
    }

    let mut total = GROUP_GAP * (n - 1);
    let mut pace = first.speed;
    for m in members.iter() {
        total += m.width(m.files);
        pace = pace.min(m.speed);
    }
    let mut left = -Fx::from_raw(total.raw() / 2);
    for m in members.iter() {
        let width = m.width(m.files);
        let lateral = left + Fx::from_raw(width.raw() / 2);
        left += width + GROUP_GAP;
        out.push(Command::MoveTo {
            regiment: m.handle,
            target: order.target + right * lateral,
            facing: Some(facing),
            files: if order.width.is_some() { m.files } else { 0 },
            pace: if members.len() > 1 { pace } else { Fx::ZERO },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: Fx = Fx::ONE;

    #[test]
    fn full_block_is_centred() {
        // 4 files x 2 ranks, spacing 1.
        let front_left = slot_offset(0, 4, 8, S, S);
        let back_right = slot_offset(7, 4, 8, S, S);
        assert_eq!(front_left, FxVec2::new(Fx::from_ratio(-3, 2), -Fx::HALF));
        assert_eq!(back_right, FxVec2::new(Fx::from_ratio(3, 2), Fx::HALF));
        assert_eq!(block_size(4, 8, S, S), (Fx::from_int(4), Fx::from_int(2)));
    }

    #[test]
    fn partial_rear_rank_is_centred() {
        // 10 soldiers in 4 files: ranks of 4, 4, 2.
        assert_eq!(ranks(4, 10), 3);
        let a = slot_offset(8, 4, 10, S, S);
        let b = slot_offset(9, 4, 10, S, S);
        assert_eq!(a, FxVec2::new(-Fx::HALF, Fx::ONE));
        assert_eq!(b, FxVec2::new(Fx::HALF, Fx::ONE));
    }

    #[test]
    fn front_is_toward_facing() {
        let north = Angle(0xC000);
        let centre = FxVec2::from_ints(10, 10);
        // A soldier in the front rank, right-hand file, facing north (-y).
        let p = local_to_world(centre, north, FxVec2::new(Fx::ONE, -Fx::ONE));
        assert_eq!(p, FxVec2::from_ints(11, 9));
    }

    fn member(index: u32, x: i32) -> GroupMember {
        GroupMember {
            handle: RegimentHandle {
                index,
                generation: 0,
            },
            position: FxVec2::from_ints(x, 50),
            facing: Angle(0xC000),
            files: 6,
            models: 24,
            file_spacing: Fx::ONE,
            speed: Fx::from_int(3 + index as i32),
        }
    }

    fn moves(out: &[Command]) -> Vec<(u32, FxVec2, Option<Angle>, u16, Fx)> {
        out.iter()
            .map(|c| match *c {
                Command::MoveTo {
                    regiment,
                    target,
                    facing,
                    files,
                    pace,
                } => (regiment.index, target, facing, files, pace),
                _ => unreachable!(),
            })
            .collect()
    }

    #[test]
    fn group_lines_up_left_to_right() {
        // Listed right to left; they should be sorted by where they stand.
        let mut members = [member(0, 40), member(1, 20), member(2, 30)];
        let mut out = Vec::new();
        let order = GroupOrder {
            target: FxVec2::from_ints(30, 10),
            facing: None,
            width: None,
        };
        group_move(&mut members, order, &mut out);
        let north = Some(Angle(0xC000));
        let pace = Fx::from_int(3);
        // Total width 6 + 1 + 6 + 1 + 6 = 20; centres at -7, 0, 7.
        assert_eq!(
            moves(&out),
            [
                (1, FxVec2::from_ints(23, 10), north, 0, pace),
                (2, FxVec2::from_ints(30, 10), north, 0, pace),
                (0, FxVec2::from_ints(37, 10), north, 0, pace),
            ]
        );
    }

    #[test]
    fn dragged_width_refits_frontages() {
        let mut members = [member(0, 20), member(1, 30)];
        let mut out = Vec::new();
        // A 25-tile line facing east: 24 tiles of soldiers, 12 files each.
        let order = GroupOrder {
            target: FxVec2::from_ints(50, 50),
            facing: Some(Angle::ZERO),
            width: Some(Fx::from_int(25)),
        };
        group_move(&mut members, order, &mut out);
        let got = moves(&out);
        assert!(
            got.iter().all(|m| m.3 == 12 && m.2 == Some(Angle::ZERO)),
            "{got:?}"
        );
        // Facing east, "right" is south: centres 6.5 tiles either side.
        assert_eq!(got[0].1.y.raw() + got[1].1.y.raw(), Fx::from_int(100).raw());
        assert_eq!((got[1].1.y - got[0].1.y).abs(), Fx::from_int(13));

        // Too narrow: one file each, never zero.
        out.clear();
        let order = GroupOrder {
            width: Some(Fx::ONE),
            ..order
        };
        group_move(&mut members, order, &mut out);
        assert!(moves(&out).iter().all(|m| m.3 == 1));
    }
}
