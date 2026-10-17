//! Wilds: the big Field Battle maps (512 tiles and more) that feel like a
//! world. Mario asked for maps where an army can't march straight from
//! one side to the other: mountains, rivers and thicket too dense to
//! march through lie across the way, with passes, fords and forest paths
//! through them, so the way between two camps winds through the land.
//! Between them the land changes from region to region: open meadow, old
//! forest, wet fen, raised highland and pale bone waste, each with its
//! own colour and dressing, and plenty of open ground and walkable woods.
//!
//! How wild a map is, is chosen with it ([`Wildness`]): plains have one
//! band with plenty of ways over and mostly open land; wild maps have
//! every band and spur, fewer ways through, and more forest and highland.
//!
//! Like the other Field Battle recipes, Wilds writes the north half and
//! the loader turns it for the south (`Rot2`), so every side gets the
//! same land; the six starts are on the seeded map's plateaus.
//!
//! The layout, north half:
//! - a **band** across the whole map between the north starts and the
//!   middle, with two or three gaps (its turned copy is the south band);
//! - often a **middle band** along the middle line, crossing the turned
//!   copy's at the centre, with gaps of its own;
//! - **spurs** from the north edge down to the band, between the north
//!   starts, each with one gap, so neighbours go round too;
//! - **regions** of land between them, dressed by what they are.
//!
//! Each band or spur is a mountain range (cliffs on foothills, passes
//! through them), a river (deep, with fords) or a belt of thicket (with
//! walkable woods along it and paths through).

use gc_fixed::Pcg32;

use crate::def::{
    BIG_MAP_TILES, CoverKind, Land, LandOp, MapDef, Mode, Noise, Pos, SetPieceKind, Shape,
    SizeClass, Symmetry,
};
use crate::generate::plateaus;
use crate::recipe::{BASE, DEEP, RISE_REACH, WATER, Writer, start_objects};
use crate::stamp::dist2;

/// The smallest Wilds map, in tiles a side.
pub const MIN_TILES: i32 = 256;

/// How wild a Wilds map is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wildness {
    /// Mostly open meadow, one band with plenty of ways over it.
    Plains,
    /// A band or two and a spur now and then; every kind of land.
    Mixed,
    /// Every band and spur, fewer ways through, more forest and highland.
    Wild,
}

impl Wildness {
    pub const ALL: [Wildness; 3] = [Wildness::Plains, Wildness::Mixed, Wildness::Wild];

    pub fn name(self) -> &'static str {
        match self {
            Wildness::Plains => "Plains",
            Wildness::Mixed => "Mixed",
            Wildness::Wild => "Wild",
        }
    }
}

/// What lies across the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Barrier {
    Mountains,
    River,
    Thicket,
}

/// A barrier's line, as points about every [`STEP`] tiles, and the gaps
/// through it.
struct Course {
    points: Vec<Pos>,
    /// Which points lie in a gap.
    open: Vec<bool>,
    /// Each gap's middle point.
    gaps: Vec<Pos>,
}

/// Tiles between a barrier's points.
const STEP: i32 = 8;

/// The map a seed makes, `tiles` on a side, as wild as `wildness`. The
/// Wilds are for the lobby's sizes, 256 tiles and up ([`MIN_TILES`]): a
/// smaller map has no room for a band between its starts.
pub fn describe(tiles: i32, seed: u64, wildness: Wildness) -> MapDef {
    let mut rng = Pcg32::new(seed, 0x0077_696c);
    let mut w = Writer::new(&mut rng, tiles, BASE);
    let noise = Noise {
        seed: w.rng.next_u32(),
        amplitude: match wildness {
            Wildness::Plains => w.range(20, 32),
            _ => w.range(28, 44),
        },
        scale: w.range(56, 96),
    };
    let starts = plateaus(tiles as u32);
    let mut wilds = Wilds {
        wildness,
        starts,
        keep: Vec::new(),
        gaps: Vec::new(),
        fords: Vec::new(),
        wet: Vec::new(),
        land: Vec::new(),
        w,
    };
    wilds.layout();
    let Wilds { w, land, .. } = wilds;
    MapDef {
        name: format!("Wilds {seed}"),
        size: SizeClass::Field(tiles),
        symmetry: Symmetry::Rot2,
        players: starts.len() as u8,
        base: BASE,
        water: WATER,
        noise: Some(noise),
        terrain: w.terrain,
        cover: w.cover,
        land,
        objects: start_objects(&starts),
        set_pieces: w.set_pieces,
    }
}

struct Wilds<'a> {
    wildness: Wildness,
    starts: [Pos; 6],
    /// Places later things stay clear of, with how far.
    keep: Vec<(Pos, i32)>,
    /// The ways through the barriers, which nothing may block.
    gaps: Vec<(Pos, i32)>,
    /// The fords, stamped last so nothing later fills them in.
    fords: Vec<(Pos, i32)>,
    /// Water no copse grows in: the rivers, pools and meres.
    wet: Vec<(Pos, i32)>,
    land: Vec<LandOp>,
    w: Writer<'a>,
}

impl Wilds<'_> {
    /// `t` tiles on a 512-tile map, scaled to this one.
    fn scaled(&self, t: i32) -> i32 {
        t * self.w.tiles / 512
    }

    /// Whether a band or spur that only some maps have is on this one:
    /// never on plains, half the time on a mixed map, always on a wild one.
    fn often(&mut self) -> bool {
        match self.wildness {
            Wildness::Plains => false,
            Wildness::Mixed => self.w.range(0, 1) == 0,
            Wildness::Wild => true,
        }
    }

    fn layout(&mut self) {
        let (n, mid) = (self.w.tiles, self.w.mid);
        // Round every start: open meadow, kept clear.
        let clear = self.scaled(60);
        for &s in &self.starts {
            self.keep.push((s, clear));
            self.gaps.push((s, clear));
        }

        // Between neighbouring north starts: where a wilder map's ways
        // through the band are, and its spurs, so every start turns aside
        // to reach one.
        let inset = self.starts[0].0;
        let jitter = n / 32;
        let between = (inset + mid) / 2;
        let betweens = [
            between + self.w.range(-jitter, jitter),
            n - between + self.w.range(-jitter, jitter),
        ];

        // The band across the map, between the north starts and the middle:
        // on plains with ways over all along it; on wilder maps only
        // between the starts, and one more in the middle of a big mixed map.
        let band_y = (self.starts[0].1 + mid) / 2 + n / 40;
        let wave = n / 24;
        let mut band = vec![(-8, band_y + self.w.range(-wave, wave))];
        let mut x = 0;
        while x < n {
            x = (x + n / 8).min(n + 8);
            band.push((x, band_y + self.w.range(-wave, wave)));
        }
        let gap_xs = match self.wildness {
            Wildness::Plains => self.w.spread(n / 10, n * 9 / 10, 3 + i32::from(n >= 640)),
            Wildness::Mixed if n >= 640 => {
                vec![
                    betweens[0],
                    mid + self.w.range(-jitter, jitter),
                    betweens[1],
                ]
            }
            _ => betweens.to_vec(),
        };
        let band_points = self.barrier_along(&band, &gap_xs, true);

        // A band along the middle line too, on a wild map and often on a
        // mixed one: the west half, which meets its turned copy at the
        // centre. A small map has no room for it.
        if self.often() && n >= BIG_MAP_TILES {
            let mut line = vec![(-8, mid + self.w.range(-wave, wave))];
            let mut x = 0;
            while x < mid - n / 8 {
                x += n / 8;
                line.push((x, mid + self.w.range(-wave / 2, wave / 2)));
            }
            line.push((mid, mid));
            // A wild map's one way over (and its turned copy's) is below
            // the corner starts, back the way the band's ways led, so a
            // march from corner to corner zigzags. A mixed map has one or
            // two anywhere, and sometimes one at the centre.
            let xs = if self.wildness == Wildness::Wild {
                vec![inset + self.w.range(-jitter, jitter)]
            } else {
                let count = self.w.range(1, 2);
                let mut xs = self.w.spread(n / 12, mid - n / 10, count);
                if self.w.range(0, 1) == 0 {
                    xs.push(mid);
                }
                xs
            };
            self.barrier_along(&line, &xs, true);
        }

        // Spurs between the north starts, from the edge down towards the
        // band's ways, stopping short of them: neighbours go round the end
        // (or, on a mixed map, through a pass). A small map has no room.
        for x in betweens {
            if !self.often() || n < BIG_MAP_TILES {
                continue;
            }
            let end_y = height_at(&band_points, x) - self.scaled(48);
            let mut line = vec![(x, -8)];
            let mut y = 0;
            while y < end_y {
                y = (y + n / 10).min(end_y);
                line.push((x + self.w.range(-wave / 2, wave / 2), y));
            }
            let gap = if self.wildness == Wildness::Mixed && self.w.range(0, 1) == 0 {
                vec![self.w.range(n / 16, end_y / 2)]
            } else {
                Vec::new()
            };
            self.barrier_along(&line, &gap, false);
        }

        // The land between them, patch by patch, then copses and meadows
        // everywhere, and two knolls to hold. Their cover and colour go
        // under the barriers', so no copse fills a river or opens a way
        // through a belt of thicket.
        let barrier_cover = std::mem::take(&mut self.w.cover);
        let barrier_land = std::mem::take(&mut self.land);
        self.patches();
        let (woods, meadows) = (self.w.many(3, 5), self.w.many(3, 5));
        for _ in 0..woods {
            self.clump(CoverKind::Wood, 6, 12);
        }
        for _ in 0..meadows {
            self.clump(CoverKind::Grass, 6, 12);
        }
        self.w.cover.extend(barrier_cover);
        self.land.extend(barrier_land);
        for label in ["The knoll", "The far knoll"] {
            if let Some(at) = self.spot(RISE_REACH) {
                self.w.rise(at, label);
                self.keep.push((at, RISE_REACH));
            }
        }

        // Last, the starts: flat and open.
        let starts = self.starts;
        for s in starts {
            if s.1 < mid {
                let r = self.scaled(20);
                self.w.flat(s, r, r * 4 / 5);
                let open = Shape::Disc {
                    at: s,
                    r: clear - 20,
                };
                self.w.cover(open, CoverKind::Open);
            }
        }
        for (at, r) in std::mem::take(&mut self.fords) {
            self.w.ford(at, r);
        }
    }

    /// A barrier along `line`, with ways through at `at` (see
    /// [`Wilds::course`]): its points.
    fn barrier_along(&mut self, line: &[Pos], at: &[i32], across: bool) -> Vec<Pos> {
        let course = self.course(line, at, across);
        let kind = self.barrier();
        self.lay(kind, &course);
        course.points
    }

    /// Which barrier a band or spur is.
    fn barrier(&mut self) -> Barrier {
        // Plains are mostly crossed by rivers; mountains and thicket are
        // for wilder maps.
        let (mountains, river) = match self.wildness {
            Wildness::Plains => (4, 16),
            _ => (8, 15),
        };
        match self.w.range(0, 19) {
            k if k < mountains => Barrier::Mountains,
            k if k < river => Barrier::River,
            _ => Barrier::Thicket,
        }
    }

    /// A barrier's course along `line`, its points every [`STEP`] tiles,
    /// with gaps where the point's x (`across`: a band) or y (a spur) is
    /// within a gap's half-width of one of `at`.
    fn course(&mut self, line: &[Pos], at: &[i32], across: bool) -> Course {
        let mut points = Vec::new();
        for (a, b) in segments(line) {
            let steps = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) / STEP).max(1);
            for i in 0..steps {
                points.push((a.0 + (b.0 - a.0) * i / steps, a.1 + (b.1 - a.1) * i / steps));
            }
        }
        if let Some(&last) = line.last() {
            points.push(last);
        }
        let half = self.w.range(10, 14);
        let along = |p: Pos| if across { p.0 } else { p.1 };
        let open: Vec<bool> = points
            .iter()
            .map(|&p| at.iter().any(|&g| (along(p) - g).abs() <= half))
            .collect();
        let gaps: Vec<Pos> = at
            .iter()
            .filter_map(|&g| points.iter().copied().min_by_key(|&p| (along(p) - g).abs()))
            .collect();
        for &g in &gaps {
            self.gaps.push((g, half + 16));
        }
        Course { points, open, gaps }
    }

    /// Stamp a barrier along its course.
    fn lay(&mut self, kind: Barrier, course: &Course) {
        let base = self.w.base;
        let pts = &course.points;
        // The solid stretches, as runs of points not in a gap.
        let marked: Vec<(Pos, bool)> = pts
            .iter()
            .copied()
            .zip(course.open.iter().copied())
            .collect();
        let runs: Vec<Vec<Pos>> = marked
            .chunk_by(|a, b| a.1 == b.1)
            .filter(|run| !run[0].1 && run.len() > 1)
            .map(|run| run.iter().map(|&(p, _)| p).collect())
            .collect();
        let line = |a: Pos, b: Pos, half_width: i32| Shape::Line { a, b, half_width };
        match kind {
            Barrier::Mountains => {
                // Foothills all along, through the passes too; the cliffs
                // only on the solid stretches, with peaks.
                for (a, b) in segments(pts) {
                    self.w.stamp(line(a, b, 8), Mode::Raise, base + 36, 14);
                    self.paint(line(a, b, 30), Land::Highland);
                }
                // The ridges and peaks come to a point, with sides too
                // steep to climb: at least 64 sixteenths over the
                // foothills in 3 tiles, 1.3 tiles a tile.
                for run in &runs {
                    let height = base + self.w.range(100, 130);
                    for (a, b) in segments(run) {
                        self.w.stamp(line(a, b, 0), Mode::Raise, height, 3);
                    }
                    for &p in run.iter().step_by(5) {
                        let peak = height + self.w.range(20, 40);
                        let at = Shape::Disc { at: p, r: 0 };
                        self.w.stamp(at, Mode::Raise, peak, 4);
                    }
                }
                self.keep_along(pts, 26);
            }
            Barrier::River => {
                let hw = self.w.range(3, 5);
                let r = hw + self.w.range(4, 6);
                // Level the banks at the fords first, so rolling ground
                // can't leave a ford between two cliffs.
                for &g in &course.gaps {
                    self.w
                        .stamp(Shape::Disc { at: g, r: r + 6 }, Mode::Set, base, 10);
                }
                for (a, b) in segments(pts) {
                    self.w.stamp(line(a, b, hw), Mode::Lower, DEEP, 4);
                    self.paint(line(a, b, 24), Land::Valley);
                }
                self.fords.extend(course.gaps.iter().map(|&g| (g, r)));
                if let Some(&g) = course.gaps.first() {
                    self.w.set_piece(SetPieceKind::Ford, g, "The ford");
                }
                // Reeds and tall grass on the banks, here and there.
                for &p in pts.iter().step_by(9) {
                    let off = self.w.range(-(hw + 6), hw + 6);
                    let at = (p.0 + off / 2, p.1 + off);
                    let s = self.w.range(4, 7);
                    self.clump_at(CoverKind::Grass, at, s);
                }
                self.keep_along(pts, hw + 10);
                self.wet.extend(pts.iter().map(|&p| (p, hw + 4)));
            }
            Barrier::Thicket => {
                let hw = self.w.range(5, 8);
                // Walkable woods along it, through the paths too, then the
                // thicket on the solid stretches.
                for (a, b) in segments(pts) {
                    self.w.cover(line(a, b, hw + 8), CoverKind::Wood);
                    self.paint(line(a, b, 30), Land::Forest);
                }
                for run in &runs {
                    for (a, b) in segments(run) {
                        self.w.cover(line(a, b, hw), CoverKind::Thicket);
                    }
                }
                self.keep_along(pts, hw + 12);
            }
        }
    }

    /// Keep later things `r` clear of a barrier's points.
    fn keep_along(&mut self, pts: &[Pos], r: i32) {
        self.keep.extend(pts.iter().map(|&p| (p, r)));
    }

    /// The land between the barriers, patch by patch: a jittered grid of
    /// patches over the north half, each a kind of land with its dressing,
    /// painted in a random order so their borders are ragged.
    fn patches(&mut self) {
        let n = self.w.tiles;
        let size = self.scaled(170).max(96);
        let cols = (n + size - 1) / size;
        let rows = (n / 2 + size - 1) / size;
        let (dx, dy) = (n / cols, n / 2 / rows);
        let mut centres = Vec::new();
        for row in 0..rows {
            for col in 0..cols {
                let x = col * dx + dx / 2 + self.w.range(-dx / 4, dx / 4);
                let y = row * dy + dy / 2 + self.w.range(-dy / 4, dy / 4);
                centres.push((x, y));
            }
        }
        for i in (1..centres.len()).rev() {
            let j = self.w.range(0, i as i32) as usize;
            centres.swap(i, j);
        }
        for at in centres {
            self.patch(at, dx.max(dy) * 3 / 4, dx.min(dy) / 2);
        }
    }

    /// A patch of land round `at`, painted out to `r` and dressed out to
    /// `core`, by what it is.
    fn patch(&mut self, at: Pos, r: i32, core: i32) {
        // The odds of each land, out of their sum.
        let odds: [(Land, i32); 5] = match self.wildness {
            Wildness::Plains => [
                (Land::Forest, 3),
                (Land::Meadow, 6),
                (Land::Fen, 2),
                (Land::Highland, 0),
                (Land::Waste, 1),
            ],
            Wildness::Mixed => [
                (Land::Forest, 3),
                (Land::Meadow, 2),
                (Land::Fen, 2),
                (Land::Highland, 2),
                (Land::Waste, 2),
            ],
            Wildness::Wild => [
                (Land::Forest, 4),
                (Land::Meadow, 1),
                (Land::Fen, 2),
                (Land::Highland, 3),
                (Land::Waste, 2),
            ],
        };
        let mut k = self.w.range(0, odds.iter().map(|o| o.1).sum::<i32>() - 1);
        let mut land = Land::Meadow;
        for (l, odd) in odds {
            if k < odd {
                land = l;
                break;
            }
            k -= odd;
        }
        // A lumpy patch, not a disc: a few overlapping discs.
        self.paint(Shape::Disc { at, r: r * 3 / 5 }, land);
        for _ in 0..5 {
            let p = self.inside(at, r * 9 / 20);
            let blob = self.w.range(r * 2 / 5, r * 3 / 5);
            self.paint(Shape::Disc { at: p, r: blob }, land);
        }
        let base = self.w.base;
        // How many things of a kind: one per `per` square tiles of the core.
        let count = |per: i32| core * core / per;
        match land {
            Land::Forest => {
                for _ in 0..count(200) {
                    let p = self.inside(at, core);
                    let s = self.w.range(6, 12);
                    self.clump_at(CoverKind::Wood, p, s);
                }
                for _ in 0..count(4000).clamp(1, 3) {
                    let p = self.inside(at, core * 2 / 3);
                    let s = self.w.range(4, 7);
                    if self.open_for(p, s + 2) {
                        self.w
                            .cover(Shape::Disc { at: p, r: s }, CoverKind::Thicket);
                        self.keep.push((p, s + 2));
                    }
                }
            }
            Land::Meadow => {
                for _ in 0..count(700) {
                    let p = self.inside(at, core);
                    let s = self.w.range(6, 12);
                    self.clump_at(CoverKind::Grass, p, s);
                }
                for _ in 0..count(2500).max(1) {
                    let p = self.inside(at, core);
                    let s = self.w.range(4, 8);
                    self.clump_at(CoverKind::Wood, p, s);
                }
            }
            Land::Fen => {
                // Wet low ground, wadeable pools, a mere or two too deep.
                let wet = core * 2 / 3;
                if self.open_for(at, wet + wet / 2) {
                    self.w
                        .stamp(Shape::Disc { at, r: wet }, Mode::Set, WATER + 5, wet / 2);
                }
                for _ in 0..count(500) {
                    let p = self.inside(at, wet);
                    let s = self.w.range(4, 8);
                    if self.open_for(p, s + 3) {
                        self.w.ford(p, s);
                        self.wet.push((p, s));
                    }
                }
                for _ in 0..self.w.range(0, 2) {
                    let p = self.inside(at, wet / 2);
                    let s = self.w.range(4, 7);
                    if self.open_for(p, s + 4) {
                        self.w
                            .stamp(Shape::Disc { at: p, r: s }, Mode::Lower, DEEP, 3);
                        self.keep.push((p, s + 4));
                        self.wet.push((p, s + 2));
                    }
                }
                for _ in 0..count(400) {
                    let p = self.inside(at, core);
                    let s = self.w.range(5, 10);
                    self.clump_at(CoverKind::Grass, p, s);
                }
            }
            Land::Highland => {
                // A raised table with cliffs round it, ramps up, and a rise
                // on top to hold; where there's no room, a broad hill.
                // The table's ramps reach `top` + 38 (see [`Wilds::table`]).
                let top = core * 45 / 100;
                let hill = core / 3;
                if self.open_for(at, top + 38) {
                    self.table(at, top);
                    self.keep.push((at, top + 38));
                } else if self.open_for(at, hill * 2) {
                    let shape = Shape::Disc { at, r: hill };
                    self.w.stamp(shape, Mode::Raise, base + 24, hill);
                }
                for _ in 0..count(900) {
                    let p = self.inside(at, core);
                    let s = self.w.range(5, 9);
                    self.clump_at(CoverKind::Grass, p, s);
                }
            }
            Land::Waste | Land::Valley => {
                // Rock spurs no one climbs, a low ridge, little cover.
                self.w.cover(Shape::Disc { at, r: core }, CoverKind::Open);
                for _ in 0..count(700) {
                    let p = self.inside(at, core);
                    let s = self.w.range(2, 4);
                    if self.open_for(p, s + 3) {
                        let h = base + self.w.range(64, 96);
                        self.w.stamp(Shape::Disc { at: p, r: s }, Mode::Raise, h, 2);
                        self.keep.push((p, s + 3));
                    }
                }
                let (dx, dy) = DIRECTIONS[self.w.range(0, 7) as usize];
                let half = core / 2;
                if self.open_for(at, half + 11) {
                    let a = (at.0 - dx * half / 10, at.1 - dy * half / 10);
                    let b = (at.0 + dx * half / 10, at.1 + dy * half / 10);
                    self.w.stamp(
                        Shape::Line {
                            a,
                            b,
                            half_width: 1,
                        },
                        Mode::Raise,
                        base + 24,
                        10,
                    );
                }
            }
        }
    }

    /// A highland table of radius `top` at `at`: cliffs round it, two or
    /// three ramps up (reaching `top` + 38 from `at`, edges and all), and
    /// a rise in the middle to hold.
    fn table(&mut self, at: Pos, top: i32) {
        let height = self.w.base + self.w.range(40, 56);
        self.w
            .stamp(Shape::Disc { at, r: top }, Mode::Raise, height, 2);
        let ramps = self.w.range(2, 3);
        let turn = self.w.range(0, 7);
        for k in 0..ramps {
            let (dx, dy) = DIRECTIONS[((turn + k * 8 / ramps) % 8) as usize];
            let reach = top + 18;
            let b = (at.0 + dx * reach / 10, at.1 + dy * reach / 10);
            self.w.stamp(
                Shape::Line {
                    a: at,
                    b,
                    half_width: 4,
                },
                Mode::Raise,
                height,
                16,
            );
        }
        let rise = self.w.range(3, 5);
        let peak = height + self.w.range(36, 48);
        self.w
            .stamp(Shape::Disc { at, r: rise }, Mode::Raise, peak, 12);
        self.w.set_piece(SetPieceKind::High, at, "The highland");
    }

    /// A random point within `r` of `at`.
    fn inside(&mut self, at: Pos, r: i32) -> Pos {
        loop {
            let (dx, dy) = (self.w.range(-r, r), self.w.range(-r, r));
            if dx * dx + dy * dy <= r * r {
                return (at.0 + dx, at.1 + dy);
            }
        }
    }

    /// Whether something `r` across at `at` leaves every way through and
    /// every start alone, and stays on the north half's map.
    fn free(&self, at: Pos, r: i32) -> bool {
        let n = self.w.tiles;
        (r + 4..n - r - 4).contains(&at.0)
            && (r + 4..n / 2 - 4).contains(&at.1)
            && apart(&self.gaps, at, n, r)
    }

    /// Whether a region or knoll of radius `r` can go at `at`: clear of the
    /// barriers, the starts and the other regions.
    fn clear(&self, at: Pos, r: i32) -> bool {
        self.free(at, r) && apart(&self.keep, at, self.w.tiles, r / 2)
    }

    /// Whether something that blocks or floods, `r` across at `at`, can go
    /// there: [`Wilds::free`], and clear of the barriers and everything
    /// else kept clear by its whole width.
    fn open_for(&self, at: Pos, r: i32) -> bool {
        self.free(at, r) && apart(&self.keep, at, self.w.tiles, r)
    }

    /// A random spot for something of radius `r` (see [`Wilds::clear`]).
    fn spot(&mut self, r: i32) -> Option<Pos> {
        let n = self.w.tiles;
        for _ in 0..48 {
            let at = (self.w.range(0, n - 1), self.w.range(0, n / 2));
            if self.clear(at, r) {
                return Some(at);
            }
        }
        // A crowded map: any clear spot on an eight-tile grid.
        let spots: Vec<Pos> = (0..n / 2)
            .step_by(8)
            .flat_map(|y| (0..n).step_by(8).map(move |x| (x, y)))
            .filter(|&at| self.clear(at, r))
            .collect();
        if spots.is_empty() {
            return None;
        }
        let k = self.w.range(0, spots.len() as i32 - 1);
        Some(spots[k as usize])
    }

    /// A clump of cover somewhere in the north half, where it
    /// [`Wilds::grows`].
    fn clump(&mut self, kind: CoverKind, lo: i32, hi: i32) {
        for _ in 0..12 {
            let r = self.w.range(lo, hi);
            let n = self.w.tiles;
            let at = (self.w.range(0, n - 1), self.w.range(0, n / 2));
            if self.grows(at, r) {
                self.w.cover(Shape::Disc { at, r }, kind);
                return;
            }
        }
    }

    /// A clump of cover at `at`, if it [`Wilds::grows`] there.
    fn clump_at(&mut self, kind: CoverKind, at: Pos, r: i32) {
        if self.grows(at, r) {
            self.w.cover(Shape::Disc { at, r }, kind);
        }
    }

    /// Whether a clump of cover `r` across can grow at `at`: not on a way
    /// through or a start, and not in water.
    fn grows(&self, at: Pos, r: i32) -> bool {
        self.free(at, r) && apart(&self.wet, at, self.w.tiles, r)
    }

    fn paint(&mut self, shape: Shape, land: Land) {
        self.land.push(LandOp { shape, land });
    }
}

/// Eight directions, in tenths of a tile.
const DIRECTIONS: [(i32, i32); 8] = [
    (10, 0),
    (7, 7),
    (0, 10),
    (-7, 7),
    (-10, 0),
    (-7, -7),
    (0, -10),
    (7, -7),
];

/// Whether `at` and its turned copy are each at least `r` plus a place's
/// own reach from every one of `places`, so nothing crowds the turned copy
/// of a band or a gap either (the middle band's east half is one).
fn apart(places: &[(Pos, i32)], at: Pos, n: i32, r: i32) -> bool {
    let copy = (n - 1 - at.0, n - 1 - at.1);
    places.iter().all(|&(p, d)| {
        let reach = (r + d) * (r + d);
        dist2(p, at) >= reach && dist2(p, copy) >= reach
    })
}

/// A line's segments, each pair of neighbouring points.
fn segments(points: &[Pos]) -> impl Iterator<Item = (Pos, Pos)> + '_ {
    points.windows(2).map(|pair| (pair[0], pair[1]))
}

/// The y of a band's line at `x`, from its points.
fn height_at(points: &[Pos], x: i32) -> i32 {
    points
        .iter()
        .min_by_key(|p| (p.0 - x).abs())
        .map_or(0, |p| p.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same size and seed always write the same description, and the
    /// seed changes it.
    #[test]
    fn the_same_seed_gives_the_same_wilds() {
        for tiles in [512, 768] {
            for wild in Wildness::ALL {
                let a = describe(tiles, 3, wild).to_ron();
                assert_eq!(a, describe(tiles, 3, wild).to_ron());
                assert_ne!(
                    a.replace("Wilds 3", ""),
                    describe(tiles, 4, wild).to_ron().replace("Wilds 4", "")
                );
            }
        }
    }
}
