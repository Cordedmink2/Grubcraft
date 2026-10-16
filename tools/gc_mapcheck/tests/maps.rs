use gc_mapcheck::{Map, MapDef, check};
use std::path::PathBuf;

fn maps_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/maps")
}

fn source(name: &str) -> String {
    std::fs::read_to_string(maps_dir().join(name)).unwrap()
}

/// The shipped map with one piece of text swapped; the piece must be there.
fn edited(name: &str, from: &str, to: &str) -> String {
    let text = source(name);
    assert!(text.contains(from), "the map no longer has {from:?}");
    text.replace(from, to)
}

fn errors_of(text: &str) -> Vec<String> {
    check(&MapDef::parse(text).unwrap()).errors
}

#[test]
fn shipped_maps_are_valid_and_fair() {
    let mut seen = 0;
    for entry in std::fs::read_dir(maps_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "ron") {
            let r = check(&MapDef::parse(&std::fs::read_to_string(&path).unwrap()).unwrap());
            assert!(r.ok(), "{}: {:?}", path.display(), r.errors);
            seen += 1;
        }
    }
    assert!(seen >= 2);
}

#[test]
fn rotated_copies_are_exactly_symmetric() {
    let duel = Map::build(&MapDef::parse(&source("ribcage_pass.ron")).unwrap());
    let n = duel.tiles;
    for y in 0..n {
        for x in 0..n {
            let (a, b) = (
                (y * n + x) as usize,
                ((n - 1 - y) * n + (n - 1 - x)) as usize,
            );
            assert_eq!(duel.heights[a], duel.heights[b], "height at ({x}, {y})");
            assert_eq!(duel.cover[a], duel.cover[b], "cover at ({x}, {y})");
        }
    }
    let teeth = Map::build(&MapDef::parse(&source("four_teeth.ron")).unwrap());
    let n = teeth.tiles;
    for y in 0..n {
        for x in 0..n {
            let a = (y * n + x) as usize;
            let b = (x * n + (n - 1 - y)) as usize; // turned a quarter circle
            assert_eq!(teeth.heights[a], teeth.heights[b], "height at ({x}, {y})");
        }
    }
}

#[test]
fn building_a_map_twice_gives_the_same_hash() {
    let def = MapDef::parse(&source("four_teeth.ron")).unwrap();
    assert_eq!(Map::build(&def).hash(), Map::build(&def).hash());
}

#[test]
fn a_missing_seep_is_an_error() {
    let text = edited(
        "ribcage_pass.ron",
        "(kind: GrubSeep, at: (13, 30), owner: Some(0)),",
        "",
    );
    let e = errors_of(&text);
    assert!(e.iter().any(|m| m.contains("in its base area")), "{e:?}");
}

#[test]
fn a_start_on_an_unreachable_ridge_is_an_error() {
    let text = edited(
        "ribcage_pass.ron",
        "(kind: Start, at: (22, 22), owner: Some(0))",
        "(kind: Start, at: (42, 36), owner: Some(0))",
    );
    let e = errors_of(&text);
    assert!(e.iter().any(|m| m.contains("can't be reached")), "{e:?}");
}

/// A flat 2-player map written out in full (no symmetry), with player 1's
/// second base seep at x = `seep_x`.
fn flat_map(seep_x: i32) -> String {
    format!(
        r#"MapDef(name: "Flat", size: Duel, symmetry: None, players: 2, base: 40, water: 10,
    terrain: [
        (shape: Disc(at: (40, 64), r: 6), mode: Raise, height: 90, edge: 5),
        (shape: Disc(at: (88, 64), r: 6), mode: Raise, height: 90, edge: 5),
    ],
    objects: [
        (kind: Start, at: (20, 64), owner: Some(0)),
        (kind: GrubSeep, at: (14, 70), owner: Some(0)),
        (kind: GrubSeep, at: (14, 58), owner: Some(0)),
        (kind: MarrowVein, at: (24, 74), owner: Some(0)),
        (kind: GrubSeep, at: (50, 60), owner: Some(0)),
        (kind: GrubSeep, at: (50, 68), owner: Some(0)),
        (kind: Start, at: (107, 64), owner: Some(1)),
        (kind: GrubSeep, at: (113, 70), owner: Some(1)),
        (kind: GrubSeep, at: ({seep_x}, 58), owner: Some(1)),
        (kind: MarrowVein, at: (103, 54), owner: Some(1)),
        (kind: GrubSeep, at: (77, 68), owner: Some(1)),
        (kind: GrubSeep, at: (77, 60), owner: Some(1)),
        (kind: Village, at: (64, 40)),
        (kind: Heart, at: (64, 64)),
        (kind: Bloodseep, at: (40, 30)),
        (kind: Bloodseep, at: (60, 100)),
        (kind: Bloodseep, at: (90, 30)),
        (kind: FrontierSlot, at: (30, 100)),
        (kind: FrontierSlot, at: (100, 100)),
        (kind: FrontierSlot, at: (64, 100)),
        (kind: FrontierSlot, at: (64, 20)),
    ],
    set_pieces: [
        (kind: High, at: (40, 64), label: "West hill"),
        (kind: High, at: (88, 64), label: "East hill"),
    ],
)"#
    )
}

#[test]
fn a_hand_written_flat_map_can_be_valid() {
    let e = errors_of(&flat_map(113));
    assert!(e.iter().all(|m| !m.contains("unfair")), "{e:?}");
}

#[test]
fn a_lopsided_resource_is_unfair() {
    let e = errors_of(&flat_map(95));
    assert!(e.iter().any(|m| m.contains("unfair")), "{e:?}");
}

#[test]
fn a_walled_off_map_has_no_counter_route() {
    // Close the two passes and the flanks and narrow the ford's gap: one way
    // across is left, and a rush along it could be walled off.
    let text = edited(
        "ribcage_pass.ron",
        "// The ford.",
        "(shape: Line(a: (88, 40), b: (92, 36), half_width: 3), mode: Raise, height: 120, edge: 2),\n        (shape: Line(a: (108, 20), b: (126, 2), half_width: 3), mode: Raise, height: 120, edge: 2),\n        (shape: Line(a: (60, 68), b: (62, 66), half_width: 3), mode: Raise, height: 120, edge: 2),\n        (shape: Line(a: (66, 62), b: (68, 60), half_width: 3), mode: Raise, height: 120, edge: 2),\n        // The ford.",
    );
    let e = errors_of(&text);
    assert!(e.iter().any(|m| m.contains("counter-route")), "{e:?}");
}

#[test]
fn objects_must_be_unique_where_the_rules_say() {
    let text = edited(
        "ribcage_pass.ron",
        "(kind: Heart, at: (64, 64), once: true),",
        "",
    );
    let e = errors_of(&text);
    assert!(e.iter().any(|m| m.contains("Heart")), "{e:?}");
}

#[test]
fn a_cut_off_stretch_of_land_is_an_error() {
    // A ring of cliffs round a wide meadow nobody can reach.
    let ring: String = [
        ((4, 88), (28, 88)),
        ((28, 88), (28, 112)),
        ((28, 112), (4, 112)),
        ((4, 112), (4, 88)),
    ]
    .iter()
    .map(|((ax, ay), (bx, by))| {
        format!("(shape: Line(a: ({ax}, {ay}), b: ({bx}, {by}), half_width: 1), mode: Raise, height: 140, edge: 0),\n        ")
    })
    .collect();
    let text = edited(
        "ribcage_pass.ron",
        "// The ford.",
        &format!("{ring}// The ford."),
    );
    let e = errors_of(&text);
    assert!(e.iter().any(|m| m.contains("can't be reached")), "{e:?}");
}

#[test]
fn the_png_preview_is_a_png() {
    let map = Map::build(&MapDef::parse(&source("ribcage_pass.ron")).unwrap());
    let (side, rgb) = gc_mapcheck::preview::render(&map, 2);
    let png = gc_mapcheck::preview::png(side, &rgb);
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
}
