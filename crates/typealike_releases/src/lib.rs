//! The newest release of TypeAlike on GitHub, for the check for updates of the About pane.
//!
//! [`GitHubReleases`] implements the core [`ReleaseFeed`] seam. Each read runs on its own thread
//! and reports through the caller's sink, so the network never blocks the caller's thread.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use typealike_core::release::{ReleaseFeed, ReleaseReadFailed, ReleaseSink, ReleaseTag};
use ureq::Agent;

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/IsraPerez98/TypeAlike/releases/latest";
const ACCEPT: &str = "application/vnd.github+json";
const USER_AGENT: &str = concat!("TypeAlike/", env!("CARGO_PKG_VERSION"));
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Why a read of the newest release failed, with the details worth logging.
#[derive(Debug, thiserror::Error)]
enum FetchError {
    #[error("github request failed")]
    Http(#[from] ureq::Error),
    #[error("github sent malformed json")]
    Json(#[from] serde_json::Error),
}

/// The part of a release of the GitHub API that the check needs.
#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
}

/// Reads the newest release of TypeAlike from the GitHub API. Drafts and pre-releases do not
/// count.
#[derive(Debug)]
pub struct GitHubReleases {
    agent: Agent,
}

impl GitHubReleases {
    /// Creates a feed with an HTTPS-only client that has bounded connect and response waits.
    #[must_use]
    pub fn new() -> Self {
        let agent = Agent::config_builder()
            .https_only(true)
            .user_agent(USER_AGENT)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_recv_response(Some(RESPONSE_TIMEOUT))
            .build()
            .into();
        Self { agent }
    }
}

impl Default for GitHubReleases {
    fn default() -> Self {
        Self::new()
    }
}

impl ReleaseFeed for GitHubReleases {
    fn read_newest(&mut self, sink: &ReleaseSink) {
        let agent = self.agent.clone();
        let sink = Arc::clone(sink);
        thread::spawn(move || {
            let release = fetch_newest_tag(&agent).map_err(|error| {
                tracing::warn!(?error, "failed to read the newest release");
                ReleaseReadFailed
            });
            sink(release);
        });
    }
}

fn fetch_newest_tag(agent: &Agent) -> Result<ReleaseTag, FetchError> {
    let body = agent
        .get(LATEST_RELEASE_URL)
        .header("Accept", ACCEPT)
        .call()?
        .body_mut()
        .read_to_string()?;
    Ok(parse_tag(&body)?)
}

fn parse_tag(body: &str) -> Result<ReleaseTag, serde_json::Error> {
    let release: LatestRelease = serde_json::from_str(body)?;
    Ok(ReleaseTag(release.tag_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tag_reads_the_tag_of_the_release() {
        let body = r#"{"tag_name":"v0.4.0","name":"TypeAlike 0.4.0","draft":false,"assets":[]}"#;

        let tag = parse_tag(body).expect("valid json");

        assert_eq!(tag, ReleaseTag("v0.4.0".to_owned()));
    }

    #[test]
    fn parse_tag_refuses_an_answer_without_a_tag() {
        let body = r#"{"message":"Not Found"}"#;

        assert!(parse_tag(body).is_err());
    }
}
