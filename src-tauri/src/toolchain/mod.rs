//! Toolchain detection — plan-v2 §5.1 (lands in M2).
//! Probes ghc/ghci, Tidal library, sclang, sc3-plugins, Dirt-Samples per platform,
//! feeds the Settings & Diagnostics pane. Decision D2: pre-install required, no bundling.

/// Outcome of the full probe matrix; consumed by engine selection (§5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolchainStatus {
    /// GHC + Tidal + SuperCollider + SuperDirt + samples all present.
    SuperDirtReady,
    /// Only the light path available: GHC + Tidal (miniPE works, §5.3 / D3).
    MiniPeOnly,
    /// Something required is missing; `Diagnostics` shows what.
    Broken,
}

/// Probe implementation — TODO(M2): spawn version checks, cache results, expose via Tauri command.
pub struct Detector;
