//! Platform glue for Grubcraft: memory accounting, per-OS process stats, the log file and crash reports, atomic file writes.

pub mod diag;
pub mod fsutil;
pub mod memory;
pub mod tagged_alloc;
pub mod thread;
