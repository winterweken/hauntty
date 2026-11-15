//! Opt-in check for a newer hauntty release on GitHub.
//!
//! Like everything else in hauntty that touches the network, this runs only
//! when the user asks — `u` in the TUI or `hauntty --check-update` — never on
//! startup. It installs nothing: it reports what is newer and where to get it.

use std::cmp::Ordering;
use std::fmt;

use anyhow::{anyhow, Context, Result};

use crate::fetch::USER_AGENT;

const RELEASES_API: &str = "https://api.github.com/repos/winterweken/hauntty/releases?per_page=30";
/// The releases page, for when a release carries no URL of its own.
pub const RELEASES_PAGE: &str = "https://github.com/winterweken/hauntty/releases";

/// A `MAJOR.MINOR.PATCH[-pre.release]` version, ordered by semver precedence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Dot-separated pre-release identifiers; empty for a stable release.
    pub pre: Vec<String>,
}

impl Version {
    /// Parse `1.2.3`, `v1.2.3` or `1.2.3-rc.1`. Build metadata after `+` is
    /// ignored; anything else that is not three numbers is `None`.
    pub fn parse(text: &str) -> Option<Version> {
        fn number(part: &str) -> Option<u64> {
            (!part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
                .then(|| part.parse().ok())
                .flatten()
        }
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let text = text.split('+').next()?;
        let (core, pre) = match text.split_once('-') {
            // `1.2.3-` is not a stable 1.2.3.
            Some((_, "")) => return None,
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };

        let mut parts = core.split('.');
        let major = number(parts.next()?)?;
        let minor = number(parts.next()?)?;
        let patch = number(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }

        let pre: Vec<String> = pre
            .map(|pre| pre.split('.').map(str::to_string).collect())
            .unwrap_or_default();
        let valid_id = |id: &String| {
            !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        };
        if !pre.iter().all(valid_id) {
            return None;
        }
        Some(Version {
            major,
            minor,
            patch,
            pre,
        })
    }

    /// The version this binary was built as.
    pub fn current() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).expect("CARGO_PKG_VERSION is valid semver")
    }

    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                // A stable release outranks every pre-release of the same core.
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => cmp_prerelease(&self.pre, &other.pre),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

/// Semver precedence for pre-release identifiers: numeric ones compare as
/// numbers and rank below alphanumeric ones, alphanumeric ones compare in
/// ASCII order, and when every shared identifier is equal the longer list
/// wins (`rc.1` > `rc`). Numerically equal identifiers that differ in text
/// (`01` vs `1`) fall back to text order so `cmp` agrees with `Eq`.
fn cmp_prerelease(a: &[String], b: &[String]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(xn), Ok(yn)) => xn.cmp(&yn).then_with(|| x.cmp(y)),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

/// One published release, as listed by the GitHub API. Drafts are dropped
/// before they get here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// Marked pre-release on GitHub, or tagged with a pre-release suffix.
    pub prerelease: bool,
    pub url: String,
}

/// What the check found, relative to the running version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    /// A newer release exists on this build's channel.
    Available {
        current: Version,
        latest: Version,
        url: String,
    },
    /// This build is the newest release on its channel.
    Current { current: Version },
    /// This build is newer than anything published on its channel: a build
    /// from source, or a release candidate that has not been tagged yet.
    Ahead {
        current: Version,
        latest: Option<Version>,
    },
}

impl UpdateStatus {
    /// One plain line for a toast or the terminal.
    pub fn summary(&self) -> String {
        match self {
            UpdateStatus::Available {
                current, latest, ..
            } => format!("hauntty {latest} is available (you have {current})"),
            UpdateStatus::Current { current } => format!("hauntty {current} is up to date"),
            UpdateStatus::Ahead {
                current,
                latest: Some(latest),
            } => format!("hauntty {current} is newer than the latest published release ({latest})"),
            UpdateStatus::Ahead {
                current,
                latest: None,
            } => format!("hauntty {current}: no releases are published yet"),
        }
    }
}

/// Decide where `current` stands against `releases`. A stable build looks at
/// stable releases only, so running 1.0.0 is never nagged about 1.1.0-rc.1; a
/// pre-release build is already on the candidate channel, so it sees newer
/// candidates and the stable release they lead to.
pub fn status(current: &Version, releases: &[Release]) -> UpdateStatus {
    let latest = releases
        .iter()
        .filter(|r| current.is_prerelease() || !r.prerelease)
        .max_by(|a, b| a.version.cmp(&b.version));
    match latest {
        Some(r) if r.version > *current => UpdateStatus::Available {
            current: current.clone(),
            latest: r.version.clone(),
            url: r.url.clone(),
        },
        Some(r) if r.version == *current => UpdateStatus::Current {
            current: current.clone(),
        },
        other => UpdateStatus::Ahead {
            current: current.clone(),
            latest: other.map(|r| r.version.clone()),
        },
    }
}

/// Parse the GitHub releases listing. Drafts and tags that are not versions
/// are skipped rather than failing the whole check.
pub fn parse_releases(json: &str) -> Result<Vec<Release>> {
    let value: serde_json::Value =
        serde_json::from_str(json).context("parsing GitHub releases response")?;
    let arr = value
        .as_array()
        .ok_or_else(|| anyhow!("unexpected GitHub releases response (rate limited?)"))?;
    Ok(arr
        .iter()
        .filter_map(|item| {
            let flag = |key: &str| item.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
            if flag("draft") {
                return None;
            }
            let version = Version::parse(item.get("tag_name")?.as_str()?)?;
            let prerelease = flag("prerelease") || version.is_prerelease();
            let url = item
                .get("html_url")
                .and_then(|v| v.as_str())
                .unwrap_or(RELEASES_PAGE)
                .to_string();
            Some(Release {
                version,
                prerelease,
                url,
            })
        })
        .collect())
}

/// Ask GitHub for the published releases and compare them with this build.
/// Uses the unauthenticated API (rate-limited to ~60 requests/hour per IP).
pub fn check_for_update() -> Result<UpdateStatus> {
    let body = ureq::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .get(RELEASES_API)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .context("requesting the release list from GitHub")?
        .into_string()
        .context("reading GitHub response")?;
    let releases = parse_releases(&body)?;
    Ok(status(&Version::current(), &releases))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
    }

    fn release(tag: &str, prerelease: bool) -> Release {
        Release {
            version: v(tag),
            prerelease: prerelease || v(tag).is_prerelease(),
            url: format!("https://example.invalid/releases/{tag}"),
        }
    }

    #[test]
    fn parses_stable_prefixed_prerelease_and_build_metadata() {
        assert_eq!(v("1.2.3"), v("v1.2.3"));
        assert_eq!(v("1.2.3+build.7"), v("1.2.3"));
        let rc = v("1.1.0-rc.1");
        assert!(rc.is_prerelease());
        assert_eq!(rc.pre, ["rc", "1"]);
        assert!(!v("1.0.0").is_prerelease());
    }

    #[test]
    fn rejects_non_versions() {
        for text in [
            "",
            "1.2",
            "1.2.3.4",
            "abc",
            "1.2.x",
            "1..3",
            "nightly",
            "1.2.3-",
            "1.2.3-rc..1",
        ] {
            assert_eq!(Version::parse(text), None, "{text:?} should not parse");
        }
    }

    #[test]
    fn orders_by_semver_precedence() {
        let ordered = [
            "0.1.4-rc.3",
            "1.0.0",
            "1.0.1",
            "1.1.0-alpha",
            "1.1.0-rc",
            "1.1.0-rc.1",
            "1.1.0-rc.2",
            "1.1.0-rc.10",
            "1.1.0",
            "2.0.0",
        ];
        for pair in ordered.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
    }

    #[test]
    fn display_roundtrips() {
        for text in ["1.0.0", "1.1.0-rc.1", "10.20.30-beta.2"] {
            assert_eq!(v(text).to_string(), text);
        }
    }

    #[test]
    fn current_version_parses() {
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn stable_build_ignores_release_candidates() {
        let releases = [release("v1.0.0", false), release("v1.1.0-rc.1", true)];
        assert_eq!(
            status(&v("1.0.0"), &releases),
            UpdateStatus::Current {
                current: v("1.0.0")
            }
        );
    }

    #[test]
    fn stable_build_sees_newer_stable() {
        let releases = [release("v1.0.0", false), release("v1.0.1", false)];
        assert_eq!(
            status(&v("1.0.0"), &releases),
            UpdateStatus::Available {
                current: v("1.0.0"),
                latest: v("1.0.1"),
                url: "https://example.invalid/releases/v1.0.1".into(),
            }
        );
    }

    #[test]
    fn candidate_build_sees_newer_candidate_and_final() {
        let current = v("1.1.0-rc.1");
        let rc2 = [release("v1.0.0", false), release("v1.1.0-rc.2", true)];
        assert!(matches!(
            status(&current, &rc2),
            UpdateStatus::Available { latest, .. } if latest == v("1.1.0-rc.2")
        ));
        let final_release = [release("v1.1.0-rc.1", true), release("v1.1.0", false)];
        assert!(matches!(
            status(&current, &final_release),
            UpdateStatus::Available { latest, .. } if latest == v("1.1.0")
        ));
        let same = [release("v1.0.0", false), release("v1.1.0-rc.1", true)];
        assert_eq!(status(&current, &same), UpdateStatus::Current { current });
    }

    #[test]
    fn untagged_build_is_ahead() {
        let releases = [release("v1.0.0", false)];
        assert_eq!(
            status(&v("1.1.0-rc.1"), &releases),
            UpdateStatus::Ahead {
                current: v("1.1.0-rc.1"),
                latest: Some(v("1.0.0")),
            }
        );
        assert_eq!(
            status(&v("0.1.0"), &[]),
            UpdateStatus::Ahead {
                current: v("0.1.0"),
                latest: None,
            }
        );
    }

    #[test]
    fn summaries_name_both_versions() {
        let s = status(&v("1.0.0"), &[release("v1.1.0", false)]).summary();
        assert!(s.contains("1.1.0") && s.contains("1.0.0"), "{s}");
        assert!(status(&v("1.0.0"), &[release("v1.0.0", false)])
            .summary()
            .contains("up to date"));
    }

    #[test]
    fn parses_github_listing_skipping_drafts_and_odd_tags() {
        let json = r#"[
            {"tag_name": "v1.2.0", "draft": true, "prerelease": false, "html_url": "https://example.invalid/d"},
            {"tag_name": "v1.1.0-rc.1", "draft": false, "prerelease": false, "html_url": "https://example.invalid/rc"},
            {"tag_name": "v1.0.0", "draft": false, "prerelease": false, "html_url": "https://example.invalid/stable"},
            {"tag_name": "nightly", "draft": false, "prerelease": true, "html_url": "https://example.invalid/n"},
            {"tag_name": "v0.9.0", "draft": false, "prerelease": true}
        ]"#;
        let releases = parse_releases(json).unwrap();
        assert_eq!(releases.len(), 3);
        // A pre-release suffix counts even when GitHub's flag is off.
        assert!(releases[0].prerelease);
        assert_eq!(releases[0].url, "https://example.invalid/rc");
        assert!(!releases[1].prerelease);
        // Flagged pre-release without a URL falls back to the releases page.
        assert!(releases[2].prerelease);
        assert_eq!(releases[2].url, RELEASES_PAGE);
    }

    #[test]
    fn rate_limit_object_is_an_error() {
        let err = parse_releases(r#"{"message": "API rate limit exceeded"}"#).unwrap_err();
        assert!(err.to_string().contains("rate limited"));
    }

    #[test]
    #[ignore = "hits the GitHub API"]
    fn live_check_completes() {
        let status = check_for_update().unwrap();
        println!("{}", status.summary());
    }
}
