use alloc::vec::Vec;

use crate::{ALIGN, AssetId, HEADER_SIZE, Kind, MAGIC, TOC_ENTRY_SIZE, VERSION};

/// Builds a pack. Output depends only on the set of entries, not the order
/// they were added in, so the same inputs always bake to identical bytes.
#[derive(Default)]
pub struct PackWriter {
    entries: Vec<(AssetId, Kind, u32, Vec<u8>)>,
}

impl PackWriter {
    pub fn new() -> PackWriter {
        PackWriter::default()
    }

    /// Add an asset under its logical path. Panics if the path (or another
    /// path with the same 64-bit id) was already added.
    pub fn add(&mut self, path: &str, kind: Kind, bytes: &[u8]) {
        self.add_with_flags(path, kind, 0, bytes);
    }

    pub fn add_with_flags(&mut self, path: &str, kind: Kind, flags: u32, bytes: &[u8]) {
        let id = AssetId::of(path);
        assert!(
            self.entries.iter().all(|e| e.0 != id),
            "asset path {path:?} added twice (or its id collides)"
        );
        self.entries.push((id, kind, flags, bytes.to_vec()));
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.entries.sort_by_key(|e| e.0);
        let mut out = alloc::vec![0; HEADER_SIZE];
        let mut toc = Vec::with_capacity(self.entries.len() * TOC_ENTRY_SIZE);
        for (id, kind, flags, bytes) in &self.entries {
            pad_to(&mut out, ALIGN);
            let offset = out.len() as u64;
            out.extend_from_slice(bytes);
            toc.extend_from_slice(&id.0.to_le_bytes());
            toc.extend_from_slice(&offset.to_le_bytes());
            toc.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            toc.extend_from_slice(&(*kind as u32).to_le_bytes());
            toc.extend_from_slice(&flags.to_le_bytes());
        }
        pad_to(&mut out, ALIGN);
        let toc_offset = out.len() as u64;
        out.extend_from_slice(&toc);

        out[..8].copy_from_slice(&MAGIC);
        out[8..12].copy_from_slice(&VERSION.to_le_bytes());
        out[12..16].copy_from_slice(&(self.entries.len() as u32).to_le_bytes());
        out[16..24].copy_from_slice(&toc_offset.to_le_bytes());
        out
    }
}

fn pad_to(out: &mut Vec<u8>, align: usize) {
    let padded = out.len().next_multiple_of(align);
    out.resize(padded, 0);
}
