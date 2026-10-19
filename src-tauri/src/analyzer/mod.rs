//! Repeated-pattern detection — plan-v2 §7 (lands in M5), decision D5.
//! Deterministic text heuristics: normalize tokens → sliding n-gram windows →
//! blake3 hashes → merge overlapping hits → rank → persist `pattern_occurrences`.
//! UI consumes this for editor decorations + "save as snippet".

/// Analyzer implementation — TODO(M5): pure functions first (unit-testable), then IPC wiring.
pub struct PatternAnalyzer;
