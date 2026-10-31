//! gsp-visualizer: shows the Elite Smash threshold on the Quickplay character
//! select of Smash Ultimate 13.0.5, as the GSP it takes and an estimate of the
//! matches to reach it (or, for a fighter already in Elite Smash, the losses
//! to fall out). UI only: no rating, network or match state changes.
pub mod chain;
pub mod glicko;
pub mod label;
pub mod model;
#[cfg(target_os = "switch")]
mod runtime;
pub mod share;
