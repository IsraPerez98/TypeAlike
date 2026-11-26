//! TypeAlike: system-wide inline autocomplete for Windows and macOS.
//!
//! The binary only does the wiring: the data folders, the settings load, the logging and the
//! platform event loop.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod logging;
mod platform;

use typealike_core::release::{Build, BuildKind};

/// The git commit of the source of this build, or `unknown` (`build.rs`).
const COMMIT: &str = env!("TYPEALIKE_COMMIT");

fn main() -> anyhow::Result<()> {
    // SAFETY: no other thread runs yet.
    unsafe { typealike_llm::disable_metal_residency_sets() };
    let build = Build {
        kind: BuildKind::from_mark(option_env!("TYPEALIKE_DEVELOPMENT_BUILD")),
        commit: COMMIT,
    };
    logging::start(build);
    let run = platform::run(build);
    logging::log_quit(&run);
    run
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_of_the_build_is_a_short_hash_or_unknown() {
        let is_short_hash =
            COMMIT.len() >= 7 && COMMIT.chars().all(|character| character.is_ascii_hexdigit());

        assert!(is_short_hash || COMMIT == "unknown", "commit {COMMIT}");
    }
}
