//! Simulation state written to bytes and read back exactly (review fix
//! R2/T6): the building block of snapshots (step 6c, issue #59). Each
//! system lists its fields once and uses [`State`] for each, so a snapshot
//! can't miss one the way a hand-written list can.
//!
//! The bytes are little-endian and only for this build: a snapshot is read
//! by the same build that wrote it (a rejoining machine, or a save of a
//! match in progress), never across versions.

use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::Formation;
use crate::hash::StateHasher;
use crate::morale::MoraleState;
use crate::regiments::{RegimentHandle, Strike};

/// A value that is part of the simulation's state.
pub(crate) trait State: Copy + PartialEq {
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "snapshots of a whole world come with step 6c")
    )]
    fn write(&self, out: &mut Vec<u8>);
    /// The value [`State::write`] wrote, taken off the front of `input`;
    /// `None` when `input` is short or holds no such value.
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "snapshots of a whole world come with step 6c")
    )]
    fn read(input: &mut &[u8]) -> Option<Self>;
    /// Feed the value to the state hash, as the bytes [`State::write`]
    /// writes.
    fn hash(&self, h: &mut StateHasher);
}

#[cfg_attr(
    not(test),
    allow(dead_code, reason = "snapshots of a whole world come with step 6c")
)]
fn take<const N: usize>(input: &mut &[u8]) -> Option<[u8; N]> {
    let (head, rest) = input.split_first_chunk::<N>()?;
    *input = rest;
    Some(*head)
}

macro_rules! number {
    ($($t:ty),*) => {$(
        impl State for $t {
            fn write(&self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.to_le_bytes());
            }
            fn read(input: &mut &[u8]) -> Option<Self> {
                take(input).map(<$t>::from_le_bytes)
            }
            fn hash(&self, h: &mut StateHasher) {
                h.bytes(&self.to_le_bytes());
            }
        }
    )*};
}
number!(u8, u16, u32, u64, i16, i32);

impl State for bool {
    fn write(&self, out: &mut Vec<u8>) {
        out.push(u8::from(*self));
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        match u8::read(input)? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
    fn hash(&self, h: &mut StateHasher) {
        u8::from(*self).hash(h);
    }
}

impl State for Fx {
    fn write(&self, out: &mut Vec<u8>) {
        self.raw().write(out);
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        i32::read(input).map(Fx::from_raw)
    }
    fn hash(&self, h: &mut StateHasher) {
        self.raw().hash(h);
    }
}

impl State for FxVec2 {
    fn write(&self, out: &mut Vec<u8>) {
        self.x.write(out);
        self.y.write(out);
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        Some(FxVec2::new(Fx::read(input)?, Fx::read(input)?))
    }
    fn hash(&self, h: &mut StateHasher) {
        self.x.hash(h);
        self.y.hash(h);
    }
}

impl State for Angle {
    fn write(&self, out: &mut Vec<u8>) {
        self.0.write(out);
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        u16::read(input).map(Angle)
    }
    fn hash(&self, h: &mut StateHasher) {
        self.0.hash(h);
    }
}

impl State for RegimentHandle {
    fn write(&self, out: &mut Vec<u8>) {
        self.index.write(out);
        self.generation.write(out);
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        Some(RegimentHandle {
            index: u32::read(input)?,
            generation: u16::read(input)?,
        })
    }
    fn hash(&self, h: &mut StateHasher) {
        self.index.hash(h);
        self.generation.hash(h);
    }
}

impl<T: State> State for Option<T> {
    fn write(&self, out: &mut Vec<u8>) {
        self.is_some().write(out);
        if let Some(v) = self {
            v.write(out);
        }
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        Some(if bool::read(input)? {
            Some(T::read(input)?)
        } else {
            None
        })
    }
    fn hash(&self, h: &mut StateHasher) {
        self.is_some().hash(h);
        if let Some(v) = self {
            v.hash(h);
        }
    }
}

impl State for Formation {
    fn write(&self, out: &mut Vec<u8>) {
        (*self as u8).write(out);
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        Formation::from_u8(u8::read(input)?)
    }
    fn hash(&self, h: &mut StateHasher) {
        (*self as u8).hash(h);
    }
}

impl State for MoraleState {
    fn write(&self, out: &mut Vec<u8>) {
        (*self as u8).write(out);
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        Some(match u8::read(input)? {
            0 => MoraleState::Steady,
            1 => MoraleState::Wavering,
            2 => MoraleState::Routing,
            3 => MoraleState::Shattered,
            _ => return None,
        })
    }
    fn hash(&self, h: &mut StateHasher) {
        (*self as u8).hash(h);
    }
}

impl State for Strike {
    fn write(&self, out: &mut Vec<u8>) {
        (*self as u8).write(out);
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        Some(match u8::read(input)? {
            0 => Strike::Turn,
            1 => Strike::Early,
            2 => Strike::Rest,
            _ => return None,
        })
    }
    fn hash(&self, h: &mut StateHasher) {
        (*self as u8).hash(h);
    }
}

/// A value of `v`'s type that differs from it, for tests that check every
/// field reaches the state hash: `v`'s bytes with one bit flipped, the
/// first flip that still reads back as a value.
#[cfg(test)]
pub(crate) fn other<T: State>(v: T) -> T {
    let mut bytes = Vec::new();
    v.write(&mut bytes);
    // Room for a value where `v` has none (`None` becoming `Some`).
    let flips = bytes.len() * 8;
    bytes.resize(bytes.len() + 16, 0);
    for bit in 0..flips {
        let mut b = bytes.clone();
        b[bit / 8] ^= 1 << (bit % 8);
        if let Some(w) = T::read(&mut &b[..])
            && w != v
        {
            return w;
        }
    }
    panic!("no other value of this type");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T: State + core::fmt::Debug>(v: T) {
        let mut bytes = Vec::new();
        v.write(&mut bytes);
        let mut input = &bytes[..];
        assert_eq!(T::read(&mut input), Some(v));
        assert!(input.is_empty());
        // Short input reads as nothing.
        assert_eq!(T::read(&mut &bytes[..bytes.len() - 1]), None);
    }

    #[test]
    fn values_read_back_as_written() {
        round_trip(0xa5u8);
        round_trip(-12_345i16);
        round_trip(u64::MAX - 3);
        round_trip(true);
        round_trip(Fx::from_int(-7));
        round_trip(FxVec2::from_ints(3, -4));
        round_trip(Angle(0x8001));
        round_trip(Some(RegimentHandle {
            index: 9,
            generation: 4,
        }));
        round_trip(None::<Angle>);
        round_trip(Formation::Tight);
        round_trip(MoraleState::Shattered);
        round_trip(Strike::Rest);
    }

    #[test]
    fn a_bad_tag_reads_as_nothing() {
        assert_eq!(bool::read(&mut &[2u8][..]), None);
        assert_eq!(MoraleState::read(&mut &[4u8][..]), None);
        assert_eq!(Strike::read(&mut &[9u8][..]), None);
    }
}
