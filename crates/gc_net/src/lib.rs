//! LAN play for Grubcraft (docs/11-networking.md).
//!
//! Every machine runs the same deterministic simulation; only orders cross
//! the network. The host gathers everyone's orders, puts each in a turn
//! a few turns ahead (the input delay, which follows the slowest
//! connection), and sends every machine a bundle per turn,
//! 20 times a second, whether or not anyone gave an order. Each machine
//! simulates a turn once it holds its bundle, so the host never waits for a
//! slow player: a late order just lands a turn later.
//!
//! - [`wire`]: compact byte encoding of commands.
//! - [`proto`]: packets and messages.
//! - [`link`]: reliable, ordered messages over UDP by redundancy.
//! - [`Lan`]: the lobby (it implements the menus' `Network`), discovery on
//!   the local network, and the turns of a match.
//! - [`crypt`]: the key exchange and encryption that secure every link.
//! - [`joincode`]: a host's address as a short code to type.
//! - [`net`]: the clock and sockets a [`Lan`] runs on: real UDP, or a
//!   network inside the process for tests.
//!
//! Not built yet (later M2 steps): resyncing a player whose world
//! diverged.

pub mod crypt;
pub mod joincode;
mod lan;
pub mod link;
pub mod net;
pub mod proto;
pub mod wire;

pub use lan::{
    DEFAULT_PORT, HASH_EVERY, HASHES_KEPT, HOST_TIMEOUT, HostMap, INPUT_DELAY, Identity,
    JOIN_TIMEOUT, Lan, MAX_DELAY, MIN_DELAY, MatchEvent, PLAYER_TIMEOUT, PORTS, RECONNECT_WAIT,
    TURN,
};
