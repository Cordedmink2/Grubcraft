//! Conquest map recipes (random maps step 2): each a small program that
//! writes a whole Conquest map description ([`MapDef`]) from a seed.
//!
//! A recipe writes the first player's share: the ground around its start
//! and the economy a Conquest match needs (the start, two seeps and a
//! Marrow vein in the base, a natural expansion of two seeps, a hamlet,
//! contested seeps and veins guarded by creep camps, a Shrine, Bloodseep
//! sites and frontier slots). The loader turns that share round for the
//! other players (`Rot2` for two, `Rot4` for four), so every start gets
//! the same ground and the same economy. The Heart, and on a Warfield the
//! Old Mouth and the Bazaar, are placed once in the middle.
//!
//! Whether a map is fair is the checker's call (`gc_mapcheck`), which
//! rerolls a seed whose map fails; the recipes keep their random choices
//! inside ranges where most seeds pass.

use gc_fixed::Pcg32;

use crate::def::{
    CoverKind, CoverOp, FRONTIER_SLOTS, Kind, MIN_BLOODSEEPS, MapDef, Mode, Noise, Object, Pos,
    SetPieceKind, Shape, SizeClass, Symmetry,
};
use crate::recipe::{DEEP, FORD, RISE_REACH, WATER, Writer};
use crate::stamp::{dist2, tile_copies};

/// The kinds of Conquest map a seed can make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConquestRecipe {
    /// Each start on a high fang with cliff sides and two ramps down;
    /// rock spines split the open ground into lanes, and the players meet
    /// at wading places.
    Teeth,
    /// A wide shallow bowl in the middle, the Heart on a raised tongue in
    /// it, the starts on its rim and hills round the edge.
    Bowl,
    /// Low wet ground: wadeable pools everywhere, a few deep meres, the
    /// starts on dry islands and reeds to hide in.
    Fen,
}

impl ConquestRecipe {
    pub const ALL: [ConquestRecipe; 3] = [
        ConquestRecipe::Teeth,
        ConquestRecipe::Bowl,
        ConquestRecipe::Fen,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ConquestRecipe::Teeth => "Teeth",
            ConquestRecipe::Bowl => "Bowl",
            ConquestRecipe::Fen => "Fen",
        }
    }
}

/// The sizes the Conquest recipes make: two players on a Duel or a
/// Skirmish map, four on a Warfield.
pub const CONQUEST_SIZES: [SizeClass; 3] =
    [SizeClass::Duel, SizeClass::Skirmish, SizeClass::Warfield];

/// Ground level (sixteenths of a tile), as the shipped maps have it, and
/// a fen's dry ground, half a tile above the water.
const BASE: i32 = 40;
const FEN_BASE: i32 = WATER + 8;
/// Objects and shaped ground keep this far in from the map's edge.
const MARGIN: i32 = 8;
/// A share has two Bloodseep places and two frontier-slot places, and
/// every frame turns it at least twice, so the rules' minimums fit.
const _: () = assert!(MIN_BLOODSEEPS.div_ceil(2) <= 2 && FRONTIER_SLOTS.start().div_ceil(2) <= 2);

/// The description a recipe makes from a seed, or `None` for a size it
/// doesn't make (see [`CONQUEST_SIZES`]).
pub fn describe(recipe: ConquestRecipe, size: SizeClass, seed: u64) -> Option<MapDef> {
    let frame = Frame::of(size)?;
    let mut rng = Pcg32::new(seed, 0x0063_6f6e);
    let base = if recipe == ConquestRecipe::Fen {
        FEN_BASE
    } else {
        BASE
    };
    let mut share = Share {
        w: Writer::new(&mut rng, frame.n, base),
        f: frame,
        objects: Vec::new(),
        features: Vec::new(),
    };
    // Rolling ground first; the objects next, so the ground the recipe
    // shapes after can keep clear of them; then the recipe's shape and
    // its cover.
    let noise = Noise {
        seed: share.w.rng.next_u32(),
        // A fen's swells stay small, so its dry ground stays out of the
        // water: the noise reaches half its amplitude either way.
        amplitude: if recipe == ConquestRecipe::Fen {
            share.w.range(8, 2 * (FEN_BASE - WATER) - 2)
        } else {
            share.w.range(12, 24)
        },
        scale: share.w.range(32, 48),
    };
    share.economy();
    match recipe {
        ConquestRecipe::Teeth => share.teeth(),
        ConquestRecipe::Bowl => share.bowl(),
        ConquestRecipe::Fen => share.fen(),
    }
    let Share { w, f, objects, .. } = share;
    Some(MapDef {
        name: format!("{} {size:?} {seed}", recipe.name()),
        size,
        symmetry: f.symmetry,
        players: f.symmetry.copies() as u8,
        base,
        water: WATER,
        noise: Some(noise),
        terrain: w.terrain,
        cover: w.cover,
        objects,
        set_pieces: w.set_pieces,
    })
}

/// Where the first player's share sits on a map of one size.
#[derive(Clone, Copy)]
struct Frame {
    n: i32,
    start: Pos,
    centre: Pos,
    symmetry: Symmetry,
}

impl Frame {
    fn of(size: SizeClass) -> Option<Frame> {
        // How far the start is in from the north-west corner; one player
        // to each turned copy.
        let (inset, symmetry) = match size {
            SizeClass::Duel => (22, Symmetry::Rot2),
            SizeClass::Skirmish => (30, Symmetry::Rot2),
            SizeClass::Warfield => (48, Symmetry::Rot4),
            SizeClass::Grand | SizeClass::Field(_) => return None,
        };
        let n = size.tiles();
        Some(Frame {
            n,
            start: (inset, inset),
            centre: (n / 2, n / 2),
            symmetry,
        })
    }

    /// The point `u` thousandths of the way from the start to the centre
    /// and `v` tiles to the side of that line (positive is north-east).
    fn at(&self, u: i32, v: i32) -> Pos {
        let (sx, sy) = self.start;
        let (cx, cy) = self.centre;
        let side = v * 707 / 1000;
        (
            sx + (cx - sx) * u / 1000 + side,
            sy + (cy - sy) * u / 1000 - side,
        )
    }

    /// `t` tiles on a Warfield, scaled to this map's size.
    fn scaled(&self, t: i32) -> i32 {
        t * self.n / 256
    }

    /// Whether a tile is far enough in from the map's edge.
    fn inside(&self, (x, y): Pos) -> bool {
        let keep = MARGIN..self.n - MARGIN;
        keep.contains(&x) && keep.contains(&y)
    }
}

/// The first player's share being written.
struct Share<'a> {
    w: Writer<'a>,
    f: Frame,
    objects: Vec<Object>,
    /// Ground already shaped (hills, pools, meres, spines): the centre and
    /// how far it reaches, kept clear of each other.
    features: Vec<(Pos, i32)>,
}

impl Share<'_> {
    /// An object of the first player's share (`owned` ones belong to it),
    /// or one placed `once`, not turned.
    fn put(&mut self, kind: Kind, at: Pos, owned: bool, once: bool, label: &str) {
        self.objects.push(Object {
            kind,
            at,
            owner: owned.then_some(0),
            label: label.into(),
            once,
        });
    }

    /// Whether ground stamped at `at` reaching `r` tiles (and its turned
    /// copies) stays clear of every object and every feature already
    /// shaped, turned copies included. Turning is a rigid move, so
    /// checking each copy of `at` against each thing where it was written
    /// covers every pair of copies. Shaped ground turns about the map's
    /// corner rather than its last tile, a tile further than objects
    /// do, so features keep a tile more apart.
    fn clear(&self, at: Pos, r: i32) -> bool {
        let spots = tile_copies(at, self.f.symmetry, self.f.n);
        let apart = |p: Pos, d: i32| spots.iter().all(|&s| dist2(p, s) >= d * d);
        self.objects.iter().all(|o| apart(o.at, r))
            && self.features.iter().all(|&(p, fr)| apart(p, r + fr + 1))
    }

    /// A random spot in the share, `u` and `v` within the ranges (see
    /// [`Frame::at`]), clear by `r` (see [`Share::clear`]); `None` if
    /// none is found in a few dozen tries.
    fn spot(&mut self, u: (i32, i32), v: (i32, i32), r: i32) -> Option<Pos> {
        for _ in 0..32 {
            let at = self.f.at(self.w.range(u.0, u.1), self.w.range(v.0, v.1));
            if self.f.inside(at) && self.clear(at, r) {
                return Some(at);
            }
        }
        None
    }

    /// The economy: the same for every recipe, with a little jitter.
    fn economy(&mut self) {
        let f = self.f;
        let s = f.start;
        self.put(Kind::Start, s, true, false, "");
        // The base, within a dozen tiles (well inside BASE_AREA), and the
        // natural expansion out to the east, past BASE_AREA.
        let own = [
            (Kind::GrubSeep, -9, 8),
            (Kind::GrubSeep, 8, -9),
            (Kind::MarrowVein, 8, 8),
            (Kind::GrubSeep, 32, -6),
            (Kind::GrubSeep, 34, 6),
        ];
        for (kind, dx, dy) in own {
            let at = (
                s.0 + dx + self.w.range(-1, 1),
                s.1 + dy + self.w.range(-1, 1),
            );
            self.put(kind, at, true, false, "");
        }
        // A hamlet on the way out to the south-west.
        let village = f.at(self.w.range(420, 480), -f.scaled(24));
        self.put(Kind::Village, village, false, false, "");
        // Contested nodes, each with a camp of creeps between it and the middle.
        let contested = [
            (Kind::GrubSeep, (640, 700), 28, (4, 2), "Bile Toads"),
            (Kind::MarrowVein, (620, 680), -30, (2, 4), "Marrow Golems"),
        ];
        for (kind, u, v, (dx, dy), creeps) in contested {
            let at = f.at(self.w.range(u.0, u.1), f.scaled(v));
            self.put(kind, at, false, false, "");
            self.put(
                Kind::CreepCamp,
                (at.0 + dx, at.1 + dy),
                false,
                false,
                creeps,
            );
        }
        // Enough Bloodseep sites and frontier slots in each share that the
        // turned copies make up the map's minimum.
        let copies = f.symmetry.copies();
        // Two places of each are enough while every frame has two or more
        // copies (checked at compile time, by the assert by `MARGIN`).
        let bloodseeps = [((820, 860), 18), ((820, 860), -22)];
        let slots = [((380, 420), -64), ((560, 620), 84)];
        let wanted = [
            (
                Kind::Bloodseep,
                &bloodseeps[..MIN_BLOODSEEPS.div_ceil(copies)],
            ),
            (
                Kind::FrontierSlot,
                &slots[..FRONTIER_SLOTS.start().div_ceil(copies)],
            ),
        ];
        for (kind, places) in wanted {
            for &(u, v) in places {
                let at = f.at(self.w.range(u.0, u.1), f.scaled(v));
                self.put(kind, at, false, false, "");
            }
        }
        // The middle.
        let (cx, cy) = f.centre;
        self.put(Kind::Heart, (cx, cy), false, true, "");
        if f.symmetry == Symmetry::Rot4 {
            self.put(Kind::OldMouth, (cx + 4, cy - 1), false, true, "");
            self.put(Kind::Bazaar, (cx, cy - 12), false, true, "");
        }
        // A Shrine off to the north-east, wherever there is room.
        let shrine = self
            .spot((250, 450), (f.scaled(36), f.scaled(60)), 4)
            .unwrap_or_else(|| f.at(300, f.scaled(52)));
        self.put(Kind::Shrine, shrine, false, false, "");
    }

    /// Mark ground shaped at `at` out to `r` tiles, so later features keep
    /// clear of it.
    fn claim(&mut self, at: Pos, r: i32) {
        self.features.push((at, r));
    }

    /// A hill a line can hold, clear of objects (its top and the upper
    /// half of its slope) and of the steeper ground round other features,
    /// named as a set piece.
    fn hill(&mut self, u: (i32, i32), v: (i32, i32), label: &str) {
        let clearance = RISE_REACH / 2 + 2;
        let wide = self.f.scaled(60);
        let Some(at) = self
            .spot(u, v, clearance)
            .or_else(|| self.spot((250, 900), (-wide, wide), clearance))
        else {
            return;
        };
        self.claim(at, RISE_REACH * 2 / 3);
        self.w.rise(at, label);
    }

    /// A few clumps of `kind` clear of the objects.
    fn clumps(&mut self, kind: CoverKind, count: i32) {
        for _ in 0..count {
            let r = self.w.range(4, 8);
            let half = self.f.scaled(60);
            if let Some(at) = self.spot((250, 950), (-half, half), r + 3) {
                self.w.cover.push(CoverOp {
                    shape: Shape::Disc { at, r },
                    kind,
                });
            }
        }
    }

    /// The wading place where the players meet: a pool round the Heart's
    /// island on two-player maps, a stream between neighbours on four.
    fn wading_place(&mut self) {
        let f = self.f;
        let (cx, cy) = f.centre;
        let at = match f.symmetry {
            Symmetry::Rot2 => {
                let r = self.w.range(6, 8);
                self.w
                    .stamp(Shape::Disc { at: f.centre, r }, Mode::Set, FORD, 4);
                self.w.stamp(
                    Shape::Disc { at: f.centre, r: 2 },
                    Mode::Set,
                    self.w.base + 4,
                    2,
                );
                self.claim(f.centre, r + 4);
                (cx, cy + 3)
            }
            _ => {
                // Narrow, so its banks stop short of the contested seep
                // beside it.
                let (y0, y1) = (f.scaled(60), f.scaled(96));
                let a = (cx, y0);
                let b = (cx, y1);
                self.w.stamp(
                    Shape::Line {
                        a,
                        b,
                        half_width: 2,
                    },
                    Mode::Lower,
                    FORD - 1,
                    2,
                );
                for y in (y0..=y1).step_by(6) {
                    self.claim((cx, y), 6);
                }
                (cx, (y0 + y1) / 2)
            }
        };
        self.w.set_piece(SetPieceKind::Ford, at, "The wading place");
    }

    /// Fangs: a high plateau with cliff sides for the start, two ramps
    /// down, rock spines between the lanes.
    fn teeth(&mut self) {
        let f = self.f;
        let s = f.start;
        let r = f.scaled(20).max(14);
        self.w
            .stamp(Shape::Disc { at: s, r }, Mode::Set, self.w.base + 56, 1);
        self.claim(s, r + 6);
        // The ramps, east (to the natural expansion) and south.
        for at in [(s.0 + r, s.1), (s.0, s.1 + r)] {
            self.w
                .stamp(Shape::Disc { at, r: 5 }, Mode::Set, self.w.base + 16, 5);
            self.claim(at, 10);
        }
        self.wading_place();
        self.hill(
            (420, 680),
            (-f.scaled(36), f.scaled(36)),
            "Hill of the Back Molar",
        );
        // Rock spines that split the open ground into lanes.
        for _ in 0..self.w.range(2, 3) {
            self.spine();
        }
        self.clumps(CoverKind::Wood, 2);
        self.clumps(CoverKind::Grass, 2);
    }

    /// A short wall of rock across open ground, clear of the objects and
    /// of the ground already shaped.
    fn spine(&mut self) {
        let f = self.f;
        for _ in 0..32 {
            let half = self.w.range(f.scaled(10), f.scaled(16));
            let mid = f.at(
                self.w.range(380, 780),
                self.w.range(-f.scaled(48), f.scaled(48)),
            );
            let (a, b) = if self.w.range(0, 1) == 0 {
                ((mid.0 - half, mid.1), (mid.0 + half, mid.1))
            } else {
                ((mid.0, mid.1 - half), (mid.0, mid.1 + half))
            };
            let along = (0..=4).map(|k| (a.0 + (b.0 - a.0) * k / 4, a.1 + (b.1 - a.1) * k / 4));
            let points: Vec<Pos> = along.collect();
            if points.iter().all(|&p| f.inside(p) && self.clear(p, 7)) {
                for &p in &points {
                    self.claim(p, 4);
                }
                self.w.stamp(
                    Shape::Line {
                        a,
                        b,
                        half_width: 2,
                    },
                    Mode::Raise,
                    self.w.base + 80,
                    2,
                );
                return;
            }
        }
    }

    /// A wide bowl in the middle with the Heart on a raised tongue, the
    /// starts on its rim.
    fn bowl(&mut self) {
        let f = self.f;
        // The bowl reaches a third of the way out to each start.
        let (r, edge) = (f.n / 6, f.n / 8);
        self.w.stamp(
            Shape::Disc { at: f.centre, r },
            Mode::Lower,
            self.w.base - 14,
            edge,
        );
        let tongue = self.w.range(3, 4);
        self.w.stamp(
            Shape::Disc {
                at: f.centre,
                r: tongue,
            },
            Mode::Set,
            self.w.base + 6,
            6,
        );
        self.claim(f.centre, tongue + 6);
        self.w.set_piece(SetPieceKind::High, f.centre, "The tongue");
        let s = f.start;
        self.w.stamp(
            Shape::Disc { at: s, r: 14 },
            Mode::Set,
            self.w.base + 10,
            10,
        );
        self.claim(s, 14);
        self.hill(
            (480, 680),
            (-f.scaled(48), -f.scaled(8)),
            "The west rim hill",
        );
        self.hill((480, 680), (f.scaled(8), f.scaled(48)), "The east rim hill");
        self.clumps(CoverKind::Wood, 3);
        self.clumps(CoverKind::Grass, 2);
    }

    /// Low wet ground: wadeable pools, a deep mere or two, the starts on
    /// dry islands.
    fn fen(&mut self) {
        let f = self.f;
        let s = f.start;
        self.w
            .stamp(Shape::Disc { at: s, r: 16 }, Mode::Set, self.w.base + 12, 6);
        self.claim(s, 22);
        self.hill((420, 680), (-f.scaled(36), f.scaled(36)), "The mound");
        let half = f.scaled(56);
        let mut named = false;
        for _ in 0..4 + f.n / 48 {
            let r = self.w.range(4, 7);
            let Some(at) = self.spot((250, 920), (-half, half), r + 3) else {
                continue;
            };
            self.claim(at, r + 2);
            self.w.stamp(Shape::Disc { at, r }, Mode::Set, FORD, 3);
            if !named && r >= 5 {
                self.w.set_piece(SetPieceKind::Ford, at, "The reed ford");
                named = true;
            }
        }
        // Channels between the pools, and a deep mere or two no one wades.
        for _ in 0..self.w.range(2, 3) {
            let len = self.w.range(6, 10);
            // The channel reaches 1.12 × len from its middle, and its banks
            // a few tiles more.
            let reach = len * 9 / 8 + 5;
            let Some(mid) = self.spot((300, 880), (-half, half), reach + 1) else {
                continue;
            };
            self.claim(mid, reach);
            let (a, b) = if self.w.range(0, 1) == 0 {
                (
                    (mid.0 - len, mid.1 - len / 2),
                    (mid.0 + len, mid.1 + len / 2),
                )
            } else {
                (
                    (mid.0 - len / 2, mid.1 - len),
                    (mid.0 + len / 2, mid.1 + len),
                )
            };
            let half_width = self.w.range(1, 2);
            self.w
                .stamp(Shape::Line { a, b, half_width }, Mode::Set, FORD, 3);
        }
        for _ in 0..self.w.range(1, 2) {
            let r = self.w.range(3, 4);
            if let Some(at) = self.spot((300, 850), (-half, half), r + 5) {
                self.claim(at, r + 3);
                self.w.stamp(Shape::Disc { at, r }, Mode::Lower, DEEP, 3);
            }
        }
        self.clumps(CoverKind::Grass, 5);
        self.clumps(CoverKind::Wood, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_recipe_size_and_seed_give_the_same_description() {
        for recipe in ConquestRecipe::ALL {
            for size in CONQUEST_SIZES {
                for seed in 1..=3 {
                    let a = describe(recipe, size, seed).unwrap().to_ron();
                    assert_eq!(a, describe(recipe, size, seed).unwrap().to_ron());
                    let mut next = describe(recipe, size, seed + 1).unwrap();
                    next.name = describe(recipe, size, seed).unwrap().name;
                    assert_ne!(a, next.to_ron(), "the seed changes the ground");
                }
            }
        }
    }

    #[test]
    fn grand_and_field_sizes_are_not_made() {
        assert!(describe(ConquestRecipe::Teeth, SizeClass::Grand, 1).is_none());
        assert!(describe(ConquestRecipe::Bowl, SizeClass::Field(256), 1).is_none());
    }
}
