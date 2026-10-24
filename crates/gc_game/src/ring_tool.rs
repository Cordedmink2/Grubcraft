//! The ring tool's plain logic (docs/26-fortifications.md, "Drawn walls";
//! `wall_tool` has the clicks and the ghost). A ring is a closed loop of
//! corners; each side of it is a straight wall, cut into orders the
//! simulation takes (at most `MAX_WALL_TILES` tiles each), and cut around
//! rough ground, so a ring leans on a cliff instead of failing at it. None of
//! this knows the world: the check of a line is passed in.

use std::collections::HashSet;

use gc_sim::walls::{MAX_WALL_TILES, WallTile};
use gc_sim::{BuildingId, Command, Refusal, RegimentHandle};
use glam::Vec2;

pub type Tile = (i32, i32);

/// How far, in tiles, a gate suggestion may sit from where the route
/// crosses the ring and still count as on the wall.
const GATE_REACH: f32 = 1.5;
/// Gate suggestions closer than this to another (in tiles) are dropped.
const GATE_APART: f32 = 4.0;

/// The straight sides of the ring through `points`, the last back to the
/// first. A side whose ends are the same tile is left out; a lone corner is
/// one tile.
pub fn ring_segments(points: &[Tile]) -> Vec<(Tile, Tile)> {
    let n = points.len();
    let mut sides: Vec<(Tile, Tile)> = match n {
        0 => Vec::new(),
        1 | 2 => points.windows(2).map(|w| (w[0], w[1])).collect(),
        _ => (0..n).map(|i| (points[i], points[(i + 1) % n])).collect(),
    };
    sides.retain(|(a, b)| a != b);
    if sides.is_empty()
        && let Some(&p) = points.first()
    {
        sides.push((p, p));
    }
    sides
}

/// The pieces a straight side from `a` to `b` is cut into so no order
/// covers more than `MAX_WALL_TILES` tiles. Pieces share the tile they meet
/// on, and the first starts on `a` and the last ends on `b`.
pub fn split_segment(a: Tile, b: Tile) -> Vec<(Tile, Tile)> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let steps = dx.abs().max(dy.abs());
    let most = MAX_WALL_TILES as i32 - 1;
    if steps <= most {
        return vec![(a, b)];
    }
    let n = (steps + most - 1) / most;
    let at = |k: i32| -> Tile {
        let part = |d: i32| (d as f32 * k as f32 / n as f32).round() as i32;
        (a.0 + part(dx), a.1 + part(dy))
    };
    (0..n).map(|k| (at(k), at(k + 1))).collect()
}

/// The pieces of a line's `tiles` (with whether each is refused for rough
/// ground) that are left once the rough tiles are cut out: the first and
/// last tile of each unbroken run.
pub fn cut_around(tiles: &[(Tile, bool)]) -> Vec<(Tile, Tile)> {
    let mut out = Vec::new();
    let mut run: Option<(Tile, Tile)> = None;
    for &(tile, rough) in tiles {
        match (rough, run) {
            (false, None) => run = Some((tile, tile)),
            (false, Some((first, _))) => run = Some((first, tile)),
            (true, Some(done)) => {
                out.push(done);
                run = None;
            }
            (true, None) => {}
        }
    }
    out.extend(run);
    out
}

/// One order's worth of a ring: a straight line and what the check made of
/// it.
#[derive(Clone, Debug)]
pub struct Piece {
    pub line: [i32; 4],
    pub plan: Result<Vec<WallTile>, Refusal>,
}

impl Piece {
    /// Whether the order for it would go through: some tiles to lay and
    /// every one of them can.
    pub fn sendable(&self) -> bool {
        matches!(&self.plan, Ok(p) if !p.is_empty() && p.iter().all(|t| t.fits.is_ok()))
    }

    /// Whether it is left out for a reason (it shows red).
    pub fn red(&self) -> bool {
        !self.sendable() && self.why().is_some()
    }

    /// The first reason it can't go, if it can't.
    pub fn why(&self) -> Option<Refusal> {
        match &self.plan {
            Ok(p) => p.iter().find_map(|t| t.fits.err()),
            Err(why) => Some(*why),
        }
    }
}

/// Whether tile `t` of a checked line was refused only for rough ground.
fn rough(t: &WallTile) -> bool {
    t.fits == Err(Refusal::Rough)
}

/// All the pieces of the ring through `points`: each side cut to the most
/// tiles an order takes, and cut around the tiles `check` finds rough. A
/// piece that `check` refuses for another reason is kept (it shows red, and
/// is not sent).
pub fn ring_pieces(
    points: &[Tile],
    check: &dyn Fn(Tile, Tile) -> Result<Vec<WallTile>, Refusal>,
) -> Vec<Piece> {
    let mut out = Vec::new();
    for (a, b) in ring_segments(points) {
        for (from, to) in split_segment(a, b) {
            let probe = check(from, to);
            let cuts = match &probe {
                Ok(tiles) if tiles.iter().any(rough) => {
                    let marked: Vec<_> = tiles.iter().map(|t| (t.tile, rough(t))).collect();
                    cut_around(&marked)
                }
                _ => vec![(from, to)],
            };
            for (f, t) in cuts {
                let plan = if (f, t) == (from, to) {
                    probe.clone()
                } else {
                    check(f, t)
                };
                out.push(Piece {
                    line: [f.0, f.1, t.0, t.1],
                    plan,
                });
            }
        }
    }
    out
}

/// The tiles the sendable pieces would lay, each once (pieces share the
/// tile they meet on).
pub fn laid_tiles(pieces: &[Piece]) -> HashSet<Tile> {
    pieces
        .iter()
        .filter(|p| p.sendable())
        .filter_map(|p| p.plan.as_ref().ok())
        .flat_map(|plan| plan.iter().map(|t| t.tile))
        .collect()
}

/// The orders for a ring: one per sendable piece, in order. A drawn wall
/// names `worker` (every order the same one); a blueprint names none.
pub fn ring_commands(
    pieces: &[Piece],
    building: BuildingId,
    blueprint: bool,
    worker: Option<RegimentHandle>,
) -> Vec<Command> {
    pieces
        .iter()
        .filter(|p| p.sendable())
        .map(|p| {
            let [x0, y0, x1, y1] = p.line.map(|v| v as i16);
            let (from, to) = ((x0, y0), (x1, y1));
            if blueprint {
                Command::PlanWall { building, from, to }
            } else {
                Command::BuildWall {
                    regiment: worker,
                    building,
                    from,
                    to,
                }
            }
        })
        .collect()
}

/// Where the straight run from `a` to `b` meets the one from `c` to `d`.
pub fn crossing(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<Vec2> {
    let (r, s) = (b - a, d - c);
    let denom = r.perp_dot(s);
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = (c - a).perp_dot(s) / denom;
    let u = (c - a).perp_dot(r) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| a + r * t)
}

/// A gate the ring suggests: the wall tile it is centred on, and the turn
/// that runs it along the wall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateSpot {
    pub tile: Tile,
    pub turn: u8,
}

/// Where gates would suit the ring `sides`: where each of `routes` (a
/// straight run between two spots, say a hall and a seep) crosses a side,
/// on the nearest of the `walls` tiles. `long_along_x` is whether the gate
/// runs along x when not turned. Spots near each other are one.
pub fn gate_spots(
    sides: &[(Tile, Tile)],
    routes: &[(Vec2, Vec2)],
    walls: &HashSet<Tile>,
    long_along_x: bool,
) -> Vec<GateSpot> {
    let centre = |t: Tile| Vec2::new(t.0 as f32 + 0.5, t.1 as f32 + 0.5);
    let mut out: Vec<GateSpot> = Vec::new();
    for &(from, to) in routes {
        for &(a, b) in sides {
            let Some(at) = crossing(from, to, centre(a), centre(b)) else {
                continue;
            };
            let Some(tile) = walls
                .iter()
                .map(|&t| (centre(t).distance(at), t))
                .filter(|&(d, _)| d <= GATE_REACH)
                // The tile itself breaks a tie, so the pick never wavers.
                .min_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)))
                .map(|(_, t)| t)
            else {
                continue;
            };
            if out
                .iter()
                .any(|g| centre(g.tile).distance(centre(tile)) < GATE_APART)
            {
                continue;
            }
            let along_x = (b.0 - a.0).abs() >= (b.1 - a.1).abs();
            out.push(GateSpot {
                tile,
                turn: u8::from(along_x != long_along_x),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_sim::walls::wall_tiles;

    /// A check that finds every tile open, except those `bad` marks with
    /// the refusal given.
    fn check_with(
        bad: Vec<(Tile, Refusal)>,
    ) -> impl Fn(Tile, Tile) -> Result<Vec<WallTile>, Refusal> {
        move |a, b| {
            let tiles = wall_tiles(a, b).ok_or(Refusal::Unknown)?;
            Ok(tiles
                .into_iter()
                .map(|tile| WallTile {
                    tile,
                    fits: match bad.iter().find(|(t, _)| *t == tile) {
                        Some(&(_, why)) => Err(why),
                        None => Ok(()),
                    },
                })
                .collect())
        }
    }

    #[test]
    fn a_ring_closes_from_the_last_corner_to_the_first() {
        let sides = ring_segments(&[(0, 0), (10, 0), (10, 10), (0, 10)]);
        assert_eq!(
            sides,
            [
                ((0, 0), (10, 0)),
                ((10, 0), (10, 10)),
                ((10, 10), (0, 10)),
                ((0, 10), (0, 0)),
            ]
        );
        // Three corners are a triangle.
        let tri = ring_segments(&[(0, 0), (8, 0), (4, 6)]);
        assert_eq!(tri.len(), 3);
        assert_eq!(tri[2], ((4, 6), (0, 0)));
    }

    #[test]
    fn a_line_or_a_lone_corner_is_not_doubled_back() {
        assert!(ring_segments(&[]).is_empty());
        assert_eq!(ring_segments(&[(3, 4)]), [((3, 4), (3, 4))]);
        assert_eq!(ring_segments(&[(3, 4), (9, 4)]), [((3, 4), (9, 4))]);
        // A corner clicked twice adds no side.
        assert_eq!(
            ring_segments(&[(0, 0), (5, 0), (5, 0), (5, 5)]),
            [((0, 0), (5, 0)), ((5, 0), (5, 5)), ((5, 5), (0, 0))]
        );
    }

    #[test]
    fn a_short_side_is_one_piece() {
        assert_eq!(split_segment((2, 2), (33, 2)), [((2, 2), (33, 2))]);
        assert_eq!(split_segment((2, 2), (2, 2)), [((2, 2), (2, 2))]);
    }

    #[test]
    fn a_long_side_is_cut_into_pieces_of_at_most_the_most_tiles() {
        for (a, b) in [
            ((0, 0), (32, 0)),
            ((0, 0), (100, 0)),
            ((50, 50), (50, -40)),
            ((10, 10), (170, 90)),
            ((100, 100), (3, 61)),
            ((0, 0), (500, 499)),
        ] {
            let pieces = split_segment(a, b);
            assert_eq!(pieces.first().unwrap().0, a, "{a:?} {b:?}");
            assert_eq!(pieces.last().unwrap().1, b, "{a:?} {b:?}");
            for w in pieces.windows(2) {
                assert_eq!(w[0].1, w[1].0, "pieces meet on a tile");
            }
            for &(f, t) in &pieces {
                let tiles = wall_tiles(f, t).expect("within the most tiles");
                assert!(tiles.len() <= MAX_WALL_TILES, "{f:?} {t:?}");
            }
        }
        // 100 tiles across: four pieces, none over 32 tiles.
        assert_eq!(split_segment((0, 0), (99, 0)).len(), 4);
    }

    #[test]
    fn a_ring_of_long_sides_orders_only_pieces_the_sim_takes() {
        let ring = [(0, 0), (90, 0), (90, 70)];
        let pieces = ring_pieces(&ring, &check_with(vec![]));
        assert!(pieces.iter().all(Piece::sendable));
        let cmds = ring_commands(&pieces, 7, true, None);
        assert_eq!(cmds.len(), pieces.len());
        for c in &cmds {
            let Command::PlanWall { from, to, .. } = *c else {
                panic!("{c:?}");
            };
            let tiles = wall_tiles(
                (i32::from(from.0), i32::from(from.1)),
                (i32::from(to.0), i32::from(to.1)),
            );
            assert!(tiles.is_some());
        }
        // The last piece comes back to the first corner.
        let Command::PlanWall { to, .. } = *cmds.last().unwrap() else {
            panic!()
        };
        assert_eq!(to, (0, 0));
    }

    #[test]
    fn rough_ground_is_cut_out_and_the_rest_kept() {
        // A flat side from 0 to 20 with rough ground at 8..=11.
        let bad: Vec<_> = (8..=11).map(|x| ((x, 0), Refusal::Rough)).collect();
        let pieces = ring_pieces(&[(0, 0), (20, 0)], &check_with(bad));
        let lines: Vec<_> = pieces.iter().map(|p| p.line).collect();
        assert_eq!(lines, [[0, 0, 7, 0], [12, 0, 20, 0]]);
        assert!(pieces.iter().all(Piece::sendable));
        // None of the cut tiles is laid.
        let laid = laid_tiles(&pieces);
        assert!(!laid.contains(&(9, 0)));
        assert!(laid.contains(&(7, 0)) && laid.contains(&(12, 0)));
    }

    #[test]
    fn rough_ground_at_the_end_of_a_side_leaves_a_shorter_piece() {
        let bad: Vec<_> = (0..=3).map(|x| ((x, 5), Refusal::Rough)).collect();
        let pieces = ring_pieces(&[(0, 5), (10, 5)], &check_with(bad));
        let lines: Vec<_> = pieces.iter().map(|p| p.line).collect();
        assert_eq!(lines, [[4, 5, 10, 5]]);
    }

    #[test]
    fn a_side_that_is_all_rough_gives_no_piece() {
        let bad: Vec<_> = (0..=10).map(|x| ((x, 0), Refusal::Rough)).collect();
        assert!(ring_pieces(&[(0, 0), (10, 0)], &check_with(bad)).is_empty());
    }

    #[test]
    fn rough_ground_in_a_long_side_is_cut_in_its_piece() {
        // 100 across, rough at 40..=44, which falls in the second piece.
        let bad: Vec<_> = (40..=44).map(|x| ((x, 0), Refusal::Rough)).collect();
        let pieces = ring_pieces(&[(0, 0), (99, 0)], &check_with(bad));
        assert!(pieces.iter().all(Piece::sendable));
        let laid = laid_tiles(&pieces);
        for x in 0..=99 {
            assert_eq!(laid.contains(&(x, 0)), !(40..=44).contains(&x), "{x}");
        }
        assert!(
            pieces
                .iter()
                .all(|p| { wall_tiles((p.line[0], p.line[1]), (p.line[2], p.line[3])).is_some() })
        );
    }

    #[test]
    fn other_refusals_are_kept_red_and_not_sent() {
        let bad = vec![((5, 0), Refusal::Built), ((15, 0), Refusal::Rough)];
        let pieces = ring_pieces(&[(0, 0), (20, 0)], &check_with(bad));
        // The rough tile is cut round; the built one stays in its piece.
        assert_eq!(pieces.len(), 2);
        assert!(!pieces[0].sendable());
        assert_eq!(pieces[0].why(), Some(Refusal::Built));
        assert!(pieces[1].sendable());
        let cmds = ring_commands(&pieces, 3, false, None);
        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            cmds[0],
            Command::BuildWall {
                from: (16, 0),
                to: (20, 0),
                ..
            }
        ));
    }

    #[test]
    fn a_side_the_check_refuses_whole_is_not_sent() {
        let refuse = |_: Tile, _: Tile| Err(Refusal::Tier);
        let pieces = ring_pieces(&[(0, 0), (10, 0), (10, 10)], &refuse);
        assert_eq!(pieces.len(), 3);
        assert!(pieces.iter().all(|p| p.why() == Some(Refusal::Tier)));
        assert!(ring_commands(&pieces, 1, true, None).is_empty());
    }

    #[test]
    fn tiles_already_walled_make_no_order() {
        let none = |_: Tile, _: Tile| Ok(Vec::new());
        let pieces = ring_pieces(&[(0, 0), (10, 0), (10, 10)], &none);
        assert!(ring_commands(&pieces, 1, true, None).is_empty());
    }

    #[test]
    fn corners_and_joins_are_counted_once() {
        let pieces = ring_pieces(&[(0, 0), (5, 0), (5, 5)], &check_with(vec![]));
        let laid = laid_tiles(&pieces);
        // A 5-by-5 triangle: 6 + 5 along the legs sharing (5,0), and the
        // diagonal sharing both its ends.
        let sum: usize = pieces
            .iter()
            .map(|p| p.plan.as_ref().map_or(0, Vec::len))
            .sum();
        assert!(sum > laid.len());
        assert!(laid.contains(&(5, 0)) && laid.contains(&(0, 0)));
    }

    #[test]
    fn two_runs_cross_where_they_meet() {
        let at = crossing(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
            Vec2::new(10.0, 0.0),
        );
        assert!(at.is_some_and(|p| p.distance(Vec2::new(5.0, 5.0)) < 1e-4));
        // Parallel, or short of each other, never.
        let flat = |y| (Vec2::new(0.0, y), Vec2::new(10.0, y));
        assert!(crossing(flat(0.0).0, flat(0.0).1, flat(1.0).0, flat(1.0).1).is_none());
        assert!(
            crossing(
                Vec2::new(0.0, 0.0),
                Vec2::new(4.0, 4.0),
                Vec2::new(0.0, 10.0),
                Vec2::new(10.0, 0.0)
            )
            .is_none()
        );
    }

    fn square_walls() -> (Vec<(Tile, Tile)>, HashSet<Tile>) {
        let corners = [(10, 10), (30, 10), (30, 30), (10, 30)];
        let sides = ring_segments(&corners);
        let pieces = ring_pieces(&corners, &check_with(vec![]));
        (sides, laid_tiles(&pieces))
    }

    #[test]
    fn a_route_out_of_the_ring_gets_a_gate_where_it_crosses() {
        let (sides, walls) = square_walls();
        // From the middle of the ring to a seep to the east.
        let route = (Vec2::new(20.5, 20.5), Vec2::new(45.5, 22.5));
        let gates = gate_spots(&sides, &[route], &walls, false);
        assert_eq!(gates.len(), 1);
        let g = gates[0];
        // On the east side, near y 21 or 22.
        assert_eq!(g.tile.0, 30);
        assert!((21..=22).contains(&g.tile.1), "{g:?}");
        // The east side runs along y: a gate long along y (the Ember Gate,
        // 1 by 3) already runs along it; one long along x turns once.
        assert_eq!(g.turn, 0);
        assert_eq!(gate_spots(&sides, &[route], &walls, true)[0].turn, 1);
    }

    #[test]
    fn a_route_that_stays_on_one_side_gets_no_gate() {
        let (sides, walls) = square_walls();
        let inside = (Vec2::new(15.5, 15.5), Vec2::new(25.5, 25.5));
        let outside = (Vec2::new(40.5, 40.5), Vec2::new(50.5, 45.5));
        assert!(gate_spots(&sides, &[inside, outside], &walls, true).is_empty());
    }

    #[test]
    fn gates_close_together_are_one_and_none_goes_on_a_gap() {
        let (sides, mut walls) = square_walls();
        let a = (Vec2::new(20.5, 20.5), Vec2::new(45.5, 21.5));
        let b = (Vec2::new(20.5, 20.5), Vec2::new(45.5, 22.5));
        assert_eq!(gate_spots(&sides, &[a, b], &walls, true).len(), 1);
        // Where the ring was cut for a cliff, no gate is suggested.
        walls.retain(|t| !(t.0 == 30 && (15..=28).contains(&t.1)));
        assert!(gate_spots(&sides, &[a], &walls, true).is_empty());
    }
}
