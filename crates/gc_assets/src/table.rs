//! Tables of fixed-size records, read in place.
//!
//! A table blob is a 16-byte header followed by the records:
//! `record_size: u32, count: u32, schema: u32, reserved: u32`.
//! `schema` is a version number the data crate bumps whenever a record's
//! layout or meaning changes, so a stale pack is refused rather than misread.

use alloc::vec::Vec;
use core::fmt;

use bytemuck::Pod;

const TABLE_HEADER: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableError {
    TooSmall,
    RecordSize { expected: u32, found: u32 },
    Schema { expected: u32, found: u32 },
    Length,
    Misaligned,
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TableError::TooSmall => write!(f, "table is smaller than its header"),
            TableError::RecordSize { expected, found } => {
                write!(f, "table records are {found} bytes, expected {expected}")
            }
            TableError::Schema { expected, found } => {
                write!(
                    f,
                    "table schema is {found}, expected {expected}; rebuild the pack"
                )
            }
            TableError::Length => write!(f, "table length does not match its record count"),
            TableError::Misaligned => write!(f, "table records are misaligned"),
        }
    }
}

impl core::error::Error for TableError {}

/// Serialise records as a table blob.
pub fn table_bytes<T: Pod>(schema: u32, records: &[T]) -> Vec<u8> {
    let mut out = Vec::with_capacity(TABLE_HEADER + size_of_val(records));
    out.extend_from_slice(&(size_of::<T>() as u32).to_le_bytes());
    out.extend_from_slice(&(records.len() as u32).to_le_bytes());
    out.extend_from_slice(&schema.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(bytemuck::cast_slice(records));
    out
}

/// View a table blob as records without copying.
pub fn read_table<T: Pod>(schema: u32, bytes: &[u8]) -> Result<&[T], TableError> {
    if bytes.len() < TABLE_HEADER {
        return Err(TableError::TooSmall);
    }
    let field = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let (record_size, count, found_schema) = (field(0), field(4), field(8));
    let expected = size_of::<T>() as u32;
    if record_size != expected {
        return Err(TableError::RecordSize {
            expected,
            found: record_size,
        });
    }
    if found_schema != schema {
        return Err(TableError::Schema {
            expected: schema,
            found: found_schema,
        });
    }
    let body = &bytes[TABLE_HEADER..];
    if body.len() != count as usize * size_of::<T>() {
        return Err(TableError::Length);
    }
    bytemuck::try_cast_slice(body).map_err(|_| TableError::Misaligned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
    struct Rec {
        a: u32,
        b: i32,
    }

    #[test]
    fn round_trip_and_checks() {
        let recs = [Rec { a: 1, b: -2 }, Rec { a: 3, b: 4 }];
        let bytes = table_bytes(7, &recs);
        // Copy into aligned storage, as a pack guarantees.
        let mut store = alloc::vec![0u128; bytes.len().div_ceil(16)];
        bytemuck::cast_slice_mut::<u128, u8>(&mut store)[..bytes.len()].copy_from_slice(&bytes);
        let view = &bytemuck::cast_slice::<u128, u8>(&store)[..bytes.len()];

        assert_eq!(read_table::<Rec>(7, view).unwrap(), &recs);
        assert_eq!(
            read_table::<Rec>(8, view).err(),
            Some(TableError::Schema {
                expected: 8,
                found: 7
            })
        );
        assert_eq!(
            read_table::<u32>(7, view).err(),
            Some(TableError::RecordSize {
                expected: 4,
                found: 8
            })
        );
        assert_eq!(
            read_table::<Rec>(7, &view[..view.len() - 1]).err(),
            Some(TableError::Length)
        );
    }
}
