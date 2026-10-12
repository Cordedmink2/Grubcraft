//! The game data pack, baked from `data/*.ron` at build time (see `build.rs`)
//! and embedded in the executable. Art and audio will live in a separate,
//! memory-mapped pack when they arrive.

use gc_assets::{Aligned, Pack};
use gc_data::GameData;

static PACK: &Aligned<[u8]> = &Aligned(*include_bytes!(concat!(env!("OUT_DIR"), "/grubcraft.pak")));

/// The embedded game data. The pack was validated when it was baked, so a
/// failure here means the build itself is broken.
pub fn load() -> GameData<'static> {
    let pack = Pack::parse(&PACK.0).unwrap_or_else(|e| panic!("embedded asset pack: {e}"));
    GameData::from_pack(&pack).unwrap_or_else(|e| panic!("embedded game data: {e}"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_data_loads() {
        let data = super::load();
        assert!(data.units.len() >= 4);
        assert_ne!(data.hash, [0; 32]);
    }
}
