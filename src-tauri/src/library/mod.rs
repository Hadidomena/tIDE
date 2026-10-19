//! Sample library — plan-v2 §6.1 (lands in M3).
//! Import = copy + normalize into managed dirs, blake3 dedupe, tag persistence,
//! name sanitization to SuperDirt conventions, watcher for external edits (`is_stale`).

pub mod dsp;

/// Library implementation — TODO(M3): import, FTS search, tag joins, sample-map refresh.
pub struct SampleLibrary;
