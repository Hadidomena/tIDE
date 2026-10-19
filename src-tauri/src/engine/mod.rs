//! Engine selection & bootstrap — plan-v2 §5.3 (lands in M2), decision D3.
//! SuperDirt: launch `sclang` with boot script, watch exit + OSC port 57120.
//! miniPE: runs inside the GHCi process via `Tidal.Synths`; health = session alive.
//! The selected engine is persisted in the `settings` table (key `engine`).

/// Which synth the session prelude targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    SuperDirt,
    MiniPe,
}

/// Bootstrap implementation — TODO(M2): sclang spawn/monitor, port liveness probe.
pub struct EngineLauncher;
