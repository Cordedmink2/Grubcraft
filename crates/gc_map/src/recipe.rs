//! Field Battle map recipes: each a small program that writes a map
//! description ([`MapDef`]) from a seed, with its random choices kept
//! inside ranges a designer set, so every Pass map feels like a pass.
//!
//! A recipe writes the north half and the loader turns it half a circle
//! for the south (`Rot2`), so both sides get exactly the same ground. Every
//! recipe places the six Field Battle starts where the seeded maps have
//! their plateaus (north-west, north-east, south-west, south-east, north,
//! south), so deployment works the same on any of them.

use gc_fixed::Pcg32;

use crate::def::{
    CoverKind, CoverOp, Kind, MapDef, Mode, Noise, Object, Pos, SetPiece, SetPieceKind, Shape,
    SizeClass, Symmetry, TerrainOp,
};
use crate::generate::{COVER_CLEAR_OF_STARTS, plateaus};

/// The kinds of Field Battle map a seed can make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipe {
    /// A cliff wall across the middle with two to four ways through, one
    /// of them a ford.
    Pass,
    /// A river across the middle with one to three fords.
    Ford,
    /// One big hill in the middle, open ground round it.
    Hill,
    /// Rolling ground, low hills and woods, no walls.
    Open,
}

impl Recipe {
    pub const ALL: [Recipe; 4] = [Recipe::Pass, Recipe::Ford, Recipe::Hill, Recipe::Open];

    pub fn name(self) -> &'static str {
        match self {
            Recipe::Pass => "Pass",
            Recipe::Ford => "Ford",
            Recipe::Hill => "Hill",
            Recipe::Open => "Open",
        }
    }
}

/// Ground level before the stamps, and the water line (sixteenths of a
/// tile); the Conquest recipes share the water.
const BASE: i32 = 48;
pub(crate) const WATER: i32 = 24;
/// Water this far under the line is a ford (shallower than the 0.35 tiles
/// the walking rules allow), and this far is too deep to wade.
pub(crate) const FORD: i32 = WATER - 3;
pub(crate) const DEEP: i32 = WATER - 14;
/// How far a rise's slope reaches from its centre, in tiles: the widest
/// top plus the widest edge (see [`Writer::rise`]).
pub(crate) const RISE_REACH: i32 = 5 + 14;

/// The description a recipe makes from a seed, `tiles` on a side.
pub fn describe(recipe: Recipe, tiles: i32, seed: u64) -> MapDef {
    let mut rng = Pcg32::new(seed, 0x006d_6170);
    let mut w = Writer::new(&mut rng, tiles, BASE);
    // Rolling ground first, then the start plateaus, then the recipe's
    // shape, then cover.
    let noise = Noise {
        seed: w.rng.next_u32(),
        amplitude: w.range(28, 48),
        scale: w.range(36, 64),
    };
    // The north starts' plateaus; the south ones are their turned copies.
    let starts = plateaus(tiles as u32);
    let mid = w.mid;
    for &at in starts.iter().filter(|s| s.1 < mid) {
        w.flat(at, 16, 12);
    }
    match recipe {
        Recipe::Pass => w.pass(),
        Recipe::Ford => w.river(),
        Recipe::Hill => w.hill(),
        Recipe::Open => w.open(),
    }
    let (woods, meadows) = match recipe {
        Recipe::Open => (w.range(4, 7), w.range(3, 5)),
        Recipe::Hill => (w.range(3, 5), w.range(2, 4)),
        Recipe::Pass | Recipe::Ford => (w.range(2, 4), w.range(2, 4)),
    };
    for _ in 0..woods {
        w.clump(CoverKind::Wood, &starts);
    }
    for _ in 0..meadows {
        w.clump(CoverKind::Grass, &starts);
    }
    let objects = starts
        .iter()
        .enumerate()
        .map(|(owner, &at)| Object {
            kind: Kind::Start,
            at,
            owner: Some(owner as u8),
            label: String::new(),
            once: true,
        })
        .collect();
    MapDef {
        name: format!("{} {seed}", recipe.name()),
        size: SizeClass::Field(tiles),
        symmetry: Symmetry::Rot2,
        players: starts.len() as u8,
        base: BASE,
        water: WATER,
        noise: Some(noise),
        terrain: w.terrain,
        cover: w.cover,
        objects,
        set_pieces: w.set_pieces,
    }
}

/// Writes the first player's share of a map: the north half of a Field
/// Battle map (the Conquest recipes use it too).
pub(crate) struct Writer<'a> {
    pub(crate) rng: &'a mut Pcg32,
    tiles: i32,
    /// The middle line, where the halves meet.
    mid: i32,
    /// Ground level before the stamps.
    pub(crate) base: i32,
    pub(crate) terrain: Vec<TerrainOp>,
    pub(crate) cover: Vec<CoverOp>,
    pub(crate) set_pieces: Vec<SetPiece>,
}

impl<'a> Writer<'a> {
    pub(crate) fn new(rng: &'a mut Pcg32, tiles: i32, base: i32) -> Self {
        Writer {
            rng,
            tiles,
            mid: tiles / 2,
            base,
            terrain: Vec::new(),
            cover: Vec::new(),
            set_pieces: Vec::new(),
        }
    }

    /// A whole number from `lo` to `hi`, both included.
    pub(crate) fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.rng.below((hi - lo + 1) as u32) as i32
    }

    pub(crate) fn stamp(&mut self, shape: Shape, mode: Mode, height: i32, edge: i32) {
        self.terrain.push(TerrainOp {
            shape,
            mode,
            height,
            edge,
        });
    }

    /// A flat, open place round a start.
    fn flat(&mut self, at: Pos, r: i32, edge: i32) {
        self.stamp(Shape::Disc { at, r }, Mode::Set, BASE, edge);
    }

    /// Where the ways through the middle line go: the distances from the
    /// centre of one side's gaps (the turned copy mirrors them on the
    /// other side), and whether there is one at the centre itself.
    fn gaps(&mut self, fewest: i32, most: i32) -> (Vec<i32>, bool) {
        let n = self.range(fewest, most);
        let centre = n % 2 == 1;
        let pairs = n / 2;
        let reach = self.mid - 16;
        let offsets = (0..pairs)
            .map(|k| {
                // Spread the pairs out, each in its own band.
                let band = (reach - 12) / pairs.max(1);
                12 + band * k + self.range(band / 4, band * 3 / 4)
            })
            .collect();
        (offsets, centre)
    }

    /// A cliff wall along the middle line, broken by gaps.
    fn pass(&mut self) {
        let (offsets, centre) = self.gaps(2, 4);
        let width = self.range(5, 7);
        let height = BASE + self.range(80, 112);
        // The west half of the line, minus the gaps; the turned copy is the east.
        let mut cuts: Vec<(i32, i32)> = offsets
            .iter()
            .map(|&d| (self.mid - d - width, self.mid - d + width))
            .collect();
        if centre {
            cuts.push((self.mid - width, self.mid + width));
        }
        cuts.sort_unstable();
        let mut from = -4;
        for &(a, b) in &cuts {
            if a > from {
                self.wall(from, a, height);
            }
            from = from.max(b);
        }
        if from < self.mid {
            self.wall(from, self.mid, height);
        }
        // One way through is a ford: the centre if there is one, else a pair.
        let at = if centre {
            (self.mid, self.mid)
        } else {
            (self.mid - offsets[0], self.mid)
        };
        self.stamp(Shape::Disc { at, r: width + 1 }, Mode::Set, FORD, 3);
        self.set_piece(SetPieceKind::Ford, at, "The ford");
        // A rise behind one of the gaps, to hold it from.
        let d = offsets.last().copied().unwrap_or(0);
        let hill = (self.mid - d, self.mid - self.range(16, 24));
        self.rise(hill, "The gap hill");
    }

    fn wall(&mut self, x0: i32, x1: i32, height: i32) {
        let (a, b) = ((x0, self.mid), (x1, self.mid));
        self.stamp(
            Shape::Line {
                a,
                b,
                half_width: 1,
            },
            Mode::Raise,
            height,
            3,
        );
    }

    /// A river along the middle line, with fords.
    fn river(&mut self) {
        let (offsets, centre) = self.gaps(1, 3);
        let half_width = self.range(3, 5);
        let r = half_width + self.range(3, 5);
        let mut fords: Vec<Pos> = offsets.iter().map(|&d| (self.mid - d, self.mid)).collect();
        if centre {
            fords.push((self.mid, self.mid));
        }
        // Level the banks before the river cuts through them, so rolling
        // ground can't leave a ford between two cliffs.
        for &at in &fords {
            self.stamp(Shape::Disc { at, r: r + 6 }, Mode::Set, BASE, 10);
        }
        let (a, b) = ((-4, self.mid), (self.mid, self.mid));
        self.stamp(Shape::Line { a, b, half_width }, Mode::Lower, DEEP, 4);
        for &at in &fords {
            self.stamp(Shape::Disc { at, r }, Mode::Set, FORD, 3);
        }
        self.set_piece(SetPieceKind::Ford, fords[0], "The ford");
        // A rise above the first ford, far enough back that its slope
        // stops short of the middle line, where a centre ford meets the
        // turned copy's rise.
        let back = self.range(RISE_REACH + 1, RISE_REACH + 7);
        let bank = (fords[0].0, self.mid - back);
        self.rise(bank, "The bank");
    }

    /// One big hill in the middle and a knoll on each side.
    fn hill(&mut self) {
        let at = (self.mid, self.mid);
        let r = self.range(3, 5);
        let height = BASE + self.range(64, 88);
        let edge = self.range(20, 28);
        self.stamp(Shape::Disc { at, r }, Mode::Raise, height, edge);
        self.set_piece(SetPieceKind::High, at, "The hill");
        let side = (
            self.range(self.tiles / 8, self.tiles / 4),
            self.mid - self.range(10, 16),
        );
        self.rise(side, "The knoll");
    }

    /// Two rises far apart on rolling ground.
    fn open(&mut self) {
        let t = self.tiles;
        let west = (
            self.range(t / 8, t * 3 / 8),
            self.mid - self.range(10, t / 6),
        );
        let east = (
            self.range(t * 5 / 8, t * 7 / 8),
            self.mid - self.range(10, t / 6),
        );
        self.rise(west, "The west rise");
        self.rise(east, "The east rise");
    }

    /// A hill a line can hold, named as a set piece. Its slope reaches
    /// [`RISE_REACH`] tiles.
    pub(crate) fn rise(&mut self, at: Pos, label: &str) {
        let r = self.range(3, 5);
        let height = self.base + self.range(40, 56);
        let edge = self.range(10, 14);
        self.stamp(Shape::Disc { at, r }, Mode::Raise, height, edge);
        self.set_piece(SetPieceKind::High, at, label);
    }

    /// Name a spot as a set piece.
    pub(crate) fn set_piece(&mut self, kind: SetPieceKind, at: Pos, label: &str) {
        self.set_pieces.push(SetPiece {
            kind,
            at,
            label: label.into(),
        });
    }

    /// A clump of woods or grass in the north half, clear of the starts.
    fn clump(&mut self, kind: CoverKind, starts: &[Pos]) {
        for _ in 0..12 {
            let r = self.range(4, 9);
            let at = (
                self.range(10, self.tiles - 10),
                self.range(10, self.mid - 4),
            );
            // Clear of every start, including the turned copies' (the
            // south starts are in `starts` too).
            let clear = starts.iter().all(|&s| {
                let (dx, dy) = (s.0 - at.0, s.1 - at.1);
                let near = COVER_CLEAR_OF_STARTS as i32 + r;
                dx * dx + dy * dy >= near * near
            });
            if clear {
                self.cover.push(CoverOp {
                    shape: Shape::Disc { at, r },
                    kind,
                });
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FIELD_MAPS;

    /// The same recipe, size and seed always write the same description.
    #[test]
    fn the_same_recipe_size_and_seed_give_the_same_description() {
        for recipe in Recipe::ALL {
            for tiles in [256, 320] {
                for seed in 1..=4 {
                    let first = describe(recipe, tiles, seed).to_ron();
                    let again = describe(recipe, tiles, seed).to_ron();
                    assert_eq!(first, again, "{} {tiles} seed {seed}", recipe.name());
                }
            }
        }
    }

    /// Every recipe fits a Field Battle: six starts, a Field size, turned
    /// half a circle, and at least one set piece.
    #[test]
    fn every_map_has_six_starts_and_a_set_piece() {
        for recipe in Recipe::ALL {
            for tiles in [256, 320] {
                for seed in 1..=4 {
                    let def = describe(recipe, tiles, seed);
                    let what = format!("{} {tiles} seed {seed}", recipe.name());
                    let starts = def.objects.iter().filter(|o| o.kind == Kind::Start).count();
                    assert_eq!(starts, 6, "{what}");
                    assert_eq!(def.size, SizeClass::Field(tiles), "{what}");
                    assert_eq!(def.symmetry, Symmetry::Rot2, "{what}");
                    assert!(!def.set_pieces.is_empty(), "{what}");
                }
            }
        }
    }

    /// The recipes are the random choices in the lobby, in the same order.
    #[test]
    fn recipe_names_are_the_lobby_names() {
        assert_eq!(Recipe::ALL.len(), 4);
        for (recipe, lobby) in Recipe::ALL.into_iter().zip(&FIELD_MAPS[1..=4]) {
            assert_eq!(*lobby, format!("Random: {}", recipe.name()));
        }
    }
}
