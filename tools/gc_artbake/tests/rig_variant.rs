//! A recoloured variant must really differ from its base mesh: the
//! `--tint` path of `tools/blender_rig/rig_nonbiped.py` has to survive glTF
//! export on textured materials (as a baseColorFactor), or it is a copy.

use std::path::PathBuf;

fn rig_test(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets-src/rig-test")
        .join(name)
}

fn base_factors(name: &str) -> Vec<[f32; 4]> {
    let g = gltf::Gltf::open(rig_test(name)).expect("glb parses");
    g.materials()
        .map(|m| m.pbr_metallic_roughness().base_color_factor())
        .collect()
}

#[test]
fn the_tinted_variant_is_not_a_copy_of_its_base() {
    let base = std::fs::read(rig_test("siege-ram-engine.glb")).unwrap();
    let var = std::fs::read(rig_test("siege-ram-engine-tint.glb")).unwrap();
    assert_ne!(base, var, "variant is byte-identical to base");
    assert!(
        base_factors("siege-ram-engine.glb")
            .iter()
            .all(|f| *f == [1.0; 4])
    );
    let v = base_factors("siege-ram-engine-tint.glb");
    assert!(
        !v.is_empty() && v.iter().any(|f| f[..3] != [1.0; 3]),
        "tint not exported: {v:?}"
    );
}
