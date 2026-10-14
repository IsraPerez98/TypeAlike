//! Platform-independent core of GhostType.
//!
//! Suggestion matching, acceptance logic and app state live here. This crate must not depend on
//! any OS API, so it builds and tests on every platform.

pub mod app;
mod catalog;
pub mod engine;
pub mod hub;
pub mod key;
pub mod models_pane;
pub mod phrases;
pub mod suggestion;
