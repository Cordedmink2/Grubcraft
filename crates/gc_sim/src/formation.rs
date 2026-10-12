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
    /// Width of its block across the front, in tiles.
    pub width: Fx,
    /// Its own marching speed, tiles per second.
    pub speed: Fx,
}

/// Turn a group order into one `MoveTo` per regiment: the regiments stand side
/// by side, centred on `target`, all facing `facing` (by default the direction
/// from the group's centre to the target), and march at the slowest member's
/// speed so they arrive together.
///
/// Members are reordered left to right by where they stand now, so blocks
/// don't cross each other's paths. Runs on the ordering player's machine only;
/// the commands it produces are what the lockstep turn carries.
pub fn group_move(
    members: &mut [GroupMember],
    target: FxVec2,
    facing: Option<Angle>,
    out: &mut Vec<Command>,
) {
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
    let facing = facing.unwrap_or_else(|| {
        let travel = target - centre;
        if travel == FxVec2::ZERO {
            first.facing
        } else {
            Angle::of_vec(travel)
        }
    });
    let right = facing.wrapping_add(Angle::QUARTER).to_vec();
    members.sort_unstable_by_key(|m| ((m.position - centre).dot(right), m.handle.index));

    let mut total = GROUP_GAP * (n - 1);
    let mut pace = first.speed;
    for m in members.iter() {
        total += m.width;
        pace = pace.min(m.speed);
    }
    let mut left = -Fx::from_raw(total.raw() / 2);
    for m in members.iter() {
        let lateral = left + Fx::from_raw(m.width.raw() / 2);
        left += m.width + GROUP_GAP;
        out.push(Command::MoveTo {
            regiment: m.handle,
            target: target + right * lateral,
            facing: Some(facing),
            files: 0,
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

    #[test]
    fn group_lines_up_left_to_right() {
        let member = |index, x| GroupMember {
            handle: RegimentHandle {
                index,
                generation: 0,
            },
            position: FxVec2::from_ints(x, 50),
            facing: Angle(0xC000),
            width: Fx::from_int(6),
            speed: Fx::from_int(3 + index as i32),
        };
        // Listed right to left; they should be sorted by where they stand.
        let mut members = [member(0, 40), member(1, 20), member(2, 30)];
        let mut out = Vec::new();
        group_move(&mut members, FxVec2::from_ints(30, 10), None, &mut out);
        let got: Vec<_> = out
            .iter()
            .map(|c| match *c {
                Command::MoveTo {
                    regiment,
                    target,
                    facing,
                    pace,
                    ..
                } => (regiment.index, target, facing, pace),
                _ => unreachable!(),
            })
            .collect();
        let north = Some(Angle(0xC000));
        // Total width 6 + 1 + 6 + 1 + 6 = 20; centres at -7, 0, 7.
        assert_eq!(
            got,
            [
                (1, FxVec2::from_ints(23, 10), north, Fx::from_int(3)),
                (2, FxVec2::from_ints(30, 10), north, Fx::from_int(3)),
                (0, FxVec2::from_ints(37, 10), north, Fx::from_int(3)),
            ]
        );
    }
}
