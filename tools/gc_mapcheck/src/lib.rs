//! Grubcraft's map validator.
//!
//! A map is a RON description (`data/maps/*.ron`, see [`def`]). [`Map::build`]
//! turns it into heights, cover, walkable ground and placed objects with
//! integer arithmetic only, and [`check`] reports whether it is valid and
//! fair. The game reads maps through the same [`Map`], so what the validator
//! checks is what gets played.

pub mod check;
pub mod def;
pub mod map;
pub mod preview;

pub use check::{Report, check};
pub use def::MapDef;
pub use map::Map;
