//! Grubcraft's map validator.
//!
//! A map is a RON description (`data/maps/*.ron`, see [`gc_map::def`]).
//! `gc_map` builds it into heights, cover, walkable ground and placed
//! objects with the same rules the game plays by, and [`check`] reports
//! whether it is valid and fair.

pub mod check;
pub mod generate;
pub mod map;
pub mod preview;

pub use check::{Report, check};
pub use gc_map::MapDef;
pub use generate::generate;
pub use map::Map;
