//! Destructive transform pipeline — plan-v2 §6.2 (lands in M4), decisions D4 + D8.
//! decode (Symphonia: wav/flac/mp3/ogg) → f32 buffer → op (reverse | trim | normalize |
//! gain | fade) → WAV encode (hound) → derived file + `samples`/`sample_transformations` rows.
//! Pure Rust, no FFmpeg sidecar; sources are never overwritten.

/// Pipeline implementation — TODO(M4): blocking-thread-pool job with progress events.
pub struct TransformPipeline;
