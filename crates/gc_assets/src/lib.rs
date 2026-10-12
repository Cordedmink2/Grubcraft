//! The Grubcraft asset pack: one file holding every baked asset in its final
//! in-memory layout, so loading is validation plus pointer casts, never parsing.
//! See `docs/10-architecture.md#pack-format`.
//!
//! Layout (all integers little-endian):
//!
//! ```text
//! header   32 bytes   magic "GRUBPAK\0", version, entry count, TOC offset
//! blobs    ...        each entry's bytes, starting on a 16-byte boundary
//! TOC      32 × n     entries sorted by asset id: id, offset, size, kind, flags
//! ```
//!
//! The reader borrows the pack's bytes (embedded, read from disk or
//! memory-mapped) and hands out sub-slices; nothing is copied.

#![no_std]

extern crate alloc;

mod table;
mod writer;

pub use table::{TableError, read_table, table_bytes};
pub use writer::PackWriter;

use core::fmt;

/// Every blob starts on this boundary, so any `Pod` type up to 16-byte
/// alignment can be read in place.
pub const ALIGN: usize = 16;

const MAGIC: [u8; 8] = *b"GRUBPAK\0";
const VERSION: u32 = 1;
const HEADER_SIZE: usize = 32;
const TOC_ENTRY_SIZE: usize = 32;

/// Stable 64-bit id of an asset: a hash of its logical path, such as
/// `"data/units"`. Ids never change when other assets are added.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetId(pub u64);

impl AssetId {
    pub fn of(path: &str) -> AssetId {
        AssetId(xxhash_rust::xxh3::xxh3_64(path.as_bytes()))
    }
}

/// What an entry holds. Stored as a `u32`; unknown values are rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Kind {
    /// A table of fixed-size records (see [`read_table`]).
    Table = 1,
    /// Raw bytes with a meaning agreed by writer and reader.
    Blob = 2,
}

impl Kind {
    fn from_u32(v: u32) -> Option<Kind> {
        match v {
            1 => Some(Kind::Table),
            2 => Some(Kind::Blob),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackError {
    TooSmall,
    BadMagic,
    UnsupportedVersion(u32),
    /// The pack's bytes don't start on a 16-byte boundary.
    Misaligned,
    TocOutOfBounds,
    EntryOutOfBounds(AssetId),
    EntryMisaligned(AssetId),
    UnknownKind(AssetId, u32),
    /// TOC entries must be sorted by id with no duplicates.
    TocNotSorted,
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackError::TooSmall => write!(f, "asset pack is too small to be valid"),
            PackError::BadMagic => write!(f, "not a Grubcraft asset pack"),
            PackError::UnsupportedVersion(v) => {
                write!(f, "asset pack version {v} is not supported")
            }
            PackError::Misaligned => write!(f, "asset pack bytes are not 16-byte aligned"),
            PackError::TocOutOfBounds => write!(f, "asset pack table of contents is out of bounds"),
            PackError::EntryOutOfBounds(id) => write!(f, "asset {:016x} is out of bounds", id.0),
            PackError::EntryMisaligned(id) => write!(f, "asset {:016x} is misaligned", id.0),
            PackError::UnknownKind(id, k) => write!(f, "asset {:016x} has unknown kind {k}", id.0),
            PackError::TocNotSorted => write!(f, "asset pack table of contents is not sorted"),
        }
    }
}

impl core::error::Error for PackError {}

/// One asset inside a pack.
#[derive(Clone, Copy, Debug)]
pub struct Entry<'a> {
    pub id: AssetId,
    pub kind: Kind,
    pub flags: u32,
    pub bytes: &'a [u8],
}

/// A validated, borrowed asset pack.
#[derive(Clone, Copy)]
pub struct Pack<'a> {
    bytes: &'a [u8],
    toc: &'a [u8],
}

impl<'a> Pack<'a> {
    /// Validate `bytes` as a pack. Every entry is bounds- and alignment-checked
    /// here, once, so lookups afterwards cannot fail on a valid id.
    pub fn parse(bytes: &'a [u8]) -> Result<Pack<'a>, PackError> {
        if bytes.len() < HEADER_SIZE {
            return Err(PackError::TooSmall);
        }
        if bytes.as_ptr().align_offset(ALIGN) != 0 {
            return Err(PackError::Misaligned);
        }
        if bytes[..8] != MAGIC {
            return Err(PackError::BadMagic);
        }
        let version = u32_at(bytes, 8);
        if version != VERSION {
            return Err(PackError::UnsupportedVersion(version));
        }
        let count = u32_at(bytes, 12) as usize;
        let toc_offset = u64_at(bytes, 16);
        let toc_len = count
            .checked_mul(TOC_ENTRY_SIZE)
            .ok_or(PackError::TocOutOfBounds)?;
        let toc_start = usize::try_from(toc_offset).map_err(|_| PackError::TocOutOfBounds)?;
        let toc_end = toc_start
            .checked_add(toc_len)
            .ok_or(PackError::TocOutOfBounds)?;
        if toc_start < HEADER_SIZE || toc_end > bytes.len() {
            return Err(PackError::TocOutOfBounds);
        }
        let pack = Pack {
            bytes,
            toc: &bytes[toc_start..toc_end],
        };
        let mut previous: Option<AssetId> = None;
        for i in 0..count {
            let raw = pack.raw_entry(i);
            if previous.is_some_and(|p| p >= raw.id) {
                return Err(PackError::TocNotSorted);
            }
            previous = Some(raw.id);
            pack.resolve(raw)?;
        }
        Ok(pack)
    }

    pub fn len(&self) -> usize {
        self.toc.len() / TOC_ENTRY_SIZE
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Look up an asset by id (binary search over the sorted TOC).
    pub fn get(&self, id: AssetId) -> Option<Entry<'a>> {
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            let raw = self.raw_entry(mid);
            match raw.id.cmp(&id) {
                core::cmp::Ordering::Less => lo = mid + 1,
                core::cmp::Ordering::Greater => hi = mid,
                core::cmp::Ordering::Equal => return self.resolve(raw).ok(),
            }
        }
        None
    }

    /// Look up an asset by its logical path.
    pub fn get_path(&self, path: &str) -> Option<Entry<'a>> {
        self.get(AssetId::of(path))
    }

    /// All entries in id order.
    pub fn entries(&self) -> impl Iterator<Item = Entry<'a>> + '_ {
        (0..self.len()).filter_map(|i| self.resolve(self.raw_entry(i)).ok())
    }

    fn raw_entry(&self, i: usize) -> RawEntry {
        let e = &self.toc[i * TOC_ENTRY_SIZE..(i + 1) * TOC_ENTRY_SIZE];
        RawEntry {
            id: AssetId(u64_at(e, 0)),
            offset: u64_at(e, 8),
            size: u64_at(e, 16),
            kind: u32_at(e, 24),
            flags: u32_at(e, 28),
        }
    }

    fn resolve(&self, raw: RawEntry) -> Result<Entry<'a>, PackError> {
        let out_of_bounds = PackError::EntryOutOfBounds(raw.id);
        let start = usize::try_from(raw.offset).map_err(|_| out_of_bounds)?;
        let size = usize::try_from(raw.size).map_err(|_| out_of_bounds)?;
        let end = start.checked_add(size).ok_or(out_of_bounds)?;
        if start < HEADER_SIZE || end > self.bytes.len() {
            return Err(out_of_bounds);
        }
        if start % ALIGN != 0 {
            return Err(PackError::EntryMisaligned(raw.id));
        }
        let kind = Kind::from_u32(raw.kind).ok_or(PackError::UnknownKind(raw.id, raw.kind))?;
        Ok(Entry {
            id: raw.id,
            kind,
            flags: raw.flags,
            bytes: &self.bytes[start..end],
        })
    }
}

#[derive(Clone, Copy)]
struct RawEntry {
    id: AssetId,
    offset: u64,
    size: u64,
    kind: u32,
    flags: u32,
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

/// Bytes with 16-byte alignment, for embedding a pack in the executable:
///
/// ```ignore
/// static PACK: &Aligned<[u8]> = &Aligned(*include_bytes!("grubcraft.pak"));
/// ```
#[repr(C, align(16))]
pub struct Aligned<B: ?Sized>(pub B);

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn aligned(bytes: &[u8]) -> Vec<u128> {
        // A Vec<u128> is 16-byte aligned; copy the pack into it.
        let mut v = alloc::vec![0u128; bytes.len().div_ceil(16)];
        bytemuck::cast_slice_mut::<u128, u8>(&mut v)[..bytes.len()].copy_from_slice(bytes);
        v
    }

    fn view(v: &[u128], len: usize) -> &[u8] {
        &bytemuck::cast_slice::<u128, u8>(v)[..len]
    }

    #[test]
    fn round_trip() {
        let mut w = PackWriter::new();
        w.add("data/units", Kind::Table, &[1, 2, 3]);
        w.add("textures/grass", Kind::Blob, &[9; 40]);
        w.add("empty", Kind::Blob, &[]);
        let bytes = w.finish();
        let store = aligned(&bytes);
        let pack = Pack::parse(view(&store, bytes.len())).unwrap();
        assert_eq!(pack.len(), 3);
        let units = pack.get_path("data/units").unwrap();
        assert_eq!(units.kind, Kind::Table);
        assert_eq!(units.bytes, &[1, 2, 3]);
        assert_eq!(pack.get_path("textures/grass").unwrap().bytes, &[9; 40]);
        assert_eq!(pack.get_path("empty").unwrap().bytes, &[] as &[u8]);
        assert!(pack.get_path("missing").is_none());
        for e in pack.entries() {
            assert_eq!(e.bytes.as_ptr().align_offset(ALIGN), 0);
        }
    }

    #[test]
    fn writer_is_deterministic_regardless_of_insert_order() {
        let mut a = PackWriter::new();
        a.add("x", Kind::Blob, b"one");
        a.add("y", Kind::Blob, b"two");
        let mut b = PackWriter::new();
        b.add("y", Kind::Blob, b"two");
        b.add("x", Kind::Blob, b"one");
        assert_eq!(a.finish(), b.finish());
    }

    #[test]
    fn rejects_corruption() {
        let mut w = PackWriter::new();
        w.add("a", Kind::Blob, &[7; 20]);
        let bytes = w.finish();

        let mut bad = bytes.clone();
        bad[0] = b'X';
        assert_eq!(
            Pack::parse(view(&aligned(&bad), bad.len())).err(),
            Some(PackError::BadMagic)
        );

        let mut bad = bytes.clone();
        bad[8] = 99;
        assert_eq!(
            Pack::parse(view(&aligned(&bad), bad.len())).err(),
            Some(PackError::UnsupportedVersion(99))
        );

        // Truncate so the TOC runs off the end.
        let short = &bytes[..bytes.len() - 1];
        assert_eq!(
            Pack::parse(view(&aligned(short), short.len())).err(),
            Some(PackError::TocOutOfBounds)
        );

        // Point the entry past the end of the file.
        let mut bad = bytes.clone();
        let toc = u64_at(&bad, 16) as usize;
        bad[toc + 16..toc + 24].copy_from_slice(&1_000_000u64.to_le_bytes());
        assert!(matches!(
            Pack::parse(view(&aligned(&bad), bad.len())),
            Err(PackError::EntryOutOfBounds(_))
        ));

        // Unknown kind.
        let mut bad = bytes.clone();
        bad[toc + 24..toc + 28].copy_from_slice(&77u32.to_le_bytes());
        assert!(matches!(
            Pack::parse(view(&aligned(&bad), bad.len())),
            Err(PackError::UnknownKind(_, 77))
        ));

        // Misaligned base pointer.
        let store = aligned(&[&[0u8][..], &bytes].concat());
        assert_eq!(
            Pack::parse(&view(&store, bytes.len() + 1)[1..]).err(),
            Some(PackError::Misaligned)
        );
    }

    #[test]
    fn embedded_alignment_helper() {
        static BYTES: &Aligned<[u8]> = &Aligned([1, 2, 3, 4]);
        assert_eq!(BYTES.0.as_ptr().align_offset(ALIGN), 0);
    }
}
