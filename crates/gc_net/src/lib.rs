//! LAN play for Grubcraft (docs/11-networking.md).
//!
//! Every machine runs the same deterministic simulation; only orders cross
//! the network. The host gathers everyone's orders, puts each in a turn
//! [`INPUT_DELAY`] turns ahead, and sends every machine a bundle per turn,
//! 20 times a second, whether or not anyone gave an order. Each machine
//! simulates a turn once it holds its bundle, so the host never waits for a
//! slow player: a late order just lands a turn later.
//!
//! - [`wire`]: compact byte encoding of commands.
//! - [`proto`]: packets and messages.
//! - [`link`]: reliable, ordered messages over UDP by redundancy.
//! - [`Lan`]: the lobby (it implements the menus' `Network`), discovery on
//!   the local network, and the turns of a match.
//!
//! Not built yet (later M2 steps): adapting the input delay to measured
//! round trips, desync report files and resyncing, encryption, join codes.

mod lan;
pub mod link;
pub mod proto;
pub mod wire;

pub use lan::{
    DEFAULT_PORT, HASH_EVERY, HostMap, INPUT_DELAY, Identity, Lan, MatchEvent, PORTS, TURN,
};
