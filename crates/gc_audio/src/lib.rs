//! Grubcraft audio: a compressed sound bank, a 32-voice mixer that runs on
//! the audio thread, a lock-free command queue from the game, and `cpal`
//! output. See docs/09-art-and-audio.md and docs/10-architecture.md.
//!
//! The game builds a [`Bank`] from the embedded blob, calls [`Audio::start`],
//! and then only ever calls `play` and the `set_*` methods, which push small
//! commands onto the queue. The mixer ([`Mixer`]) has no device code in it,
//! so it is tested without a sound card.

// Stereo frames are walked as 2-sample chunks; `as_chunks` needs a newer Rust
// than the one this workspace declares.
#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]

pub mod adpcm;
pub mod bank;
mod engine;
pub mod mixer;
pub mod queue;

pub use bank::{Bank, BankError, Clip, ClipSource, SoundId, write_bank};
pub use engine::{Audio, AudioError};
pub use mixer::{MAX_VOICES, Meter, Mixer};
