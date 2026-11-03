//! The seam to the service that finds the newest release of TypeAlike.

use std::fmt;
use std::sync::Arc;

/// The tag of a release, such as `v0.4.0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseTag(pub String);

/// The read of the newest release failed. The service logs why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseReadFailed;

/// Receives the tag of the newest release, or the failure of the read. The feed may call it from
/// any thread.
pub type ReleaseSink = Arc<dyn Fn(Result<ReleaseTag, ReleaseReadFailed>) + Send + Sync>;

/// Finds the newest release of TypeAlike without blocking the caller.
pub trait ReleaseFeed {
    /// Starts to read the newest release and returns at once. The answer reaches `sink` once.
    fn read_newest(&mut self, sink: &ReleaseSink);
}

/// A version of TypeAlike: major, minor and patch numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    /// The version of this build.
    pub(crate) fn of_build() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("the workspace version has three numbers")
    }

    /// Reads `text`, such as `0.4.0` or the tag `v0.4.0`. Returns `None` for any other form, such as
    /// a version with a pre-release part.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let mut numbers = text
            .strip_prefix('v')
            .unwrap_or(text)
            .split('.')
            .map(|number| number.parse().ok());
        let version = Self {
            major: numbers.next()??,
            minor: numbers.next()??,
            patch: numbers.next()??,
        };
        numbers.next().is_none().then_some(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reads_a_tag_and_a_plain_version() {
        let versions = ["v0.4.0", "0.4.0", "v1.12.3"].map(Version::parse);

        assert_eq!(
            versions.map(|version| version.map(|shown| shown.to_string())),
            [
                Some("0.4.0".to_owned()),
                Some("0.4.0".to_owned()),
                Some("1.12.3".to_owned()),
            ]
        );
    }

    #[test]
    fn parse_refuses_other_forms() {
        let versions = ["", "v0.4", "v0.4.0.1", "v0.4.0-beta.1", "latest", "v-1.0.0"].map(Version::parse);

        assert_eq!(versions, [None; 6]);
    }

    #[test]
    fn a_higher_number_is_a_newer_version() {
        let pairs = [
            ("0.3.0", "0.3.1"),
            ("0.3.9", "0.4.0"),
            ("0.9.9", "1.0.0"),
            ("0.3.10", "0.10.0"),
        ];

        let are_newer = pairs.map(|(older, newer)| Version::parse(newer) > Version::parse(older));

        assert_eq!(are_newer, [true; 4]);
    }

    #[test]
    fn version_of_build_is_the_version_of_the_workspace() {
        let version = Version::of_build();

        assert_eq!(version.to_string(), env!("CARGO_PKG_VERSION"));
    }
}
