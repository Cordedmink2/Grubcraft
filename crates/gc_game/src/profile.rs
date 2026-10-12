//! Tracy profiling hooks. With the `profile` feature off (the default) every
//! macro here expands to nothing, so release builds pay no cost.
//!
//! To profile: `cargo run --release -p gc_game --features profile`, then
//! connect the Tracy profiler (matching the `tracy-client` version) to it.

/// Time the rest of the enclosing block as a named Tracy zone.
#[cfg(feature = "profile")]
macro_rules! zone {
    ($name:literal) => {
        let _zone = tracy_client::span!($name);
    };
}

#[cfg(not(feature = "profile"))]
macro_rules! zone {
    ($name:literal) => {};
}

pub(crate) use zone;

/// Start the Tracy client (no-op without the `profile` feature).
pub fn start() {
    #[cfg(feature = "profile")]
    tracy_client::Client::start();
}

/// Mark the end of a rendered frame.
pub fn frame_mark() {
    #[cfg(feature = "profile")]
    tracy_client::frame_mark();
}
