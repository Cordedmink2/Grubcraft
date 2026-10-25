//! Per-model team masks for the Meshy core assets (assets-src/meshy/masks/*.png).
//! Meshy textures are painted, so the hue rule selects whole orange/brown models; each model instead
//! takes a mask of its red cloth only (strict rule + clean-up, see docs/39). These tests keep every
//! committed mask usable: a square power-of-two map the baker accepts, strictly black/white, a small
//! share of the texture (a hue-rule-sized mask means the wrong thing was picked), and named by the
//! core configs that use it.

use std::path::PathBuf;

fn meshy() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets-src/meshy")
}

fn masks() -> Vec<(String, u32, u32, Vec<u8>)> {
    let mut out: Vec<(String, u32, u32, Vec<u8>)> = Vec::new();
    for e in std::fs::read_dir(meshy().join("masks")).unwrap() {
        let path = e.unwrap().path();
        let mut dec = png::Decoder::new(std::fs::File::open(&path).unwrap());
        dec.set_transformations(png::Transformations::EXPAND);
        let mut r = dec.read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size()];
        let info = r.next_frame(&mut buf).unwrap();
        let ch = info.color_type.samples();
        let first: Vec<u8> = buf[..info.buffer_size()].chunks(ch).map(|c| c[0]).collect();
        out.push((
            path.file_stem().unwrap().to_string_lossy().to_string(),
            info.width,
            info.height,
            first,
        ));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn every_team_mask_is_a_small_black_and_white_square() {
    let m = masks();
    assert!(
        m.len() >= 17,
        "expected the 17 core masks, found {}",
        m.len()
    );
    for (name, w, h, px) in &m {
        assert_eq!(w, h, "{name}: not square");
        assert!(w.is_power_of_two() && *w >= 512, "{name}: {w} texels");
        assert!(
            px.iter().all(|v| *v == 0 || *v == 255),
            "{name}: not strictly black/white"
        );
        let pct = px.iter().filter(|v| **v == 255).count() as f32 * 100.0 / px.len() as f32;
        assert!(
            pct <= 10.0,
            "{name}: {pct:.1}% of the texture is team colour (hue-rule sized)"
        );
    }
}

#[test]
fn a_mask_selects_only_a_few_texels_and_is_never_the_whole_map() {
    // The regression this guards: `team_hue: (0, 30, 0.5)` picked 51-64% of Meshy building textures.
    for (name, _, _, px) in masks() {
        let on = px.iter().filter(|v| **v == 255).count();
        assert!(on < px.len() / 10, "{name}: {on} of {} texels", px.len());
    }
}

#[test]
fn the_core_configs_use_exactly_the_committed_masks() {
    let committed: Vec<String> = masks().into_iter().map(|m| m.0).collect();
    let mut used = Vec::new();
    for f in ["oathbound-core.ron", "brood-core.ron"] {
        let text = std::fs::read_to_string(meshy().join(f)).unwrap();
        assert!(
            !text.contains("team_hue"),
            "{f}: the hue rule must not be used for Meshy models"
        );
        for part in text.split("repo:meshy/masks/").skip(1) {
            used.push(part.split(".png").next().unwrap().to_string());
        }
    }
    used.sort();
    let mut c = committed.clone();
    c.sort();
    assert_eq!(used, c, "configs and masks differ");
}

#[test]
fn every_mask_marks_a_visible_team_region() {
    // The regression this guards: V-13 shipped with a 0.00% mask (no team colour at all) and four more
    // were under 0.5% (speckle). Units and heroes need >= 1.5% of the map; buildings >= 0.75%.
    // V-19 hearth hut and B-23 pustule had thin masks (1.35% / 1.94%: a door banner, a few veins). They were
    // widened by hand over the matching hue family (docs/39), so they now carry a higher floor (3.0%) and a
    // too-thin mask fails.
    const UNITS: [&str; 7] = ["V-01", "V-03", "V-13", "B-02", "B-03", "B-04", "B-13"];
    const THIN_FIXED: [&str; 2] = ["V-19", "B-23"];
    for (name, _, _, px) in masks() {
        let pct = px.iter().filter(|v| **v == 255).count() as f32 * 100.0 / px.len() as f32;
        let floor = if THIN_FIXED.contains(&name.as_str()) {
            3.0
        } else if UNITS.contains(&name.as_str()) {
            1.5
        } else {
            0.75
        };
        assert!(
            pct >= floor,
            "{name}: only {pct:.2}% team colour (floor {floor}%)"
        );
    }
}
