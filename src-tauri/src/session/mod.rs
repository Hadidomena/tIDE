//! Persistent GHCi + Tidal REPL session supervisor — plan-v2 §5.2 (lands in M1).
//! Owns one long-lived ghci child process: `:{ … :}` block framing, prompt detection,
//! serialized eval queue, timeout/restart recovery, stdout streamed to the console pane.

/// Session supervisor implementation — TODO(M1): spawn, framing, queue, recovery.
pub struct Supervisor;
