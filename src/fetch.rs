//! Optional, user-initiated fetching of extra themes from the upstream
//! `mbadolato/iTerm2-Color-Schemes` repository.
//!
//! This is the only part of hauntty that touches the network, and it never runs
//! unless the user explicitly asks for it. Failures never touch the config.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use crate::theme::{Theme, ThemeSource};

const USER_AGENT: &str = concat!("hauntty/", env!("CARGO_PKG_VERSION"));
const CONTENTS_API: &str =
    "https://api.github.com/repos/mbadolato/iTerm2-Color-Schemes/contents/ghostty?ref=master";

/// A theme available for download from the upstream repo.
#[derive(Debug, Clone)]
pub struct RemoteTheme {
    pub name: String,
    pub download_url: String,
}

/// A Starship preset available for download from community catalog.
#[derive(Debug, Clone)]
pub struct RemoteStarshipPreset {
    pub name: String,
    pub description: String,
    pub download_url: String,
}

/// List the Ghostty themes available upstream. Uses the unauthenticated GitHub
/// API (rate-limited to ~60 requests/hour per IP).
pub fn list_remote_themes() -> Result<Vec<RemoteTheme>> {
    let body = ureq::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .get(CONTENTS_API)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .context("requesting theme list from GitHub")?
        .into_string()
        .context("reading GitHub response")?;

    let json: serde_json::Value = serde_json::from_str(&body).context("parsing GitHub response")?;
    let arr = json
        .as_array()
        .ok_or_else(|| anyhow!("unexpected GitHub response (rate limited?)"))?;

    let mut out = Vec::new();
    for item in arr {
        if item.get("type").and_then(|v| v.as_str()) == Some("file") {
            if let (Some(name), Some(url)) = (
                item.get("name").and_then(|v| v.as_str()),
                item.get("download_url").and_then(|v| v.as_str()),
            ) {
                out.push(RemoteTheme {
                    name: name.to_string(),
                    download_url: url.to_string(),
                });
            }
        }
    }
    out.sort_by_key(|r| r.name.to_lowercase());
    Ok(out)
}

/// Download one remote theme into `dest_dir` and return the written path.
pub fn download_theme(remote: &RemoteTheme, dest_dir: &Path) -> Result<PathBuf> {
    let body = ureq::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .get(&remote.download_url)
        .set("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("downloading {}", remote.name))?
        .into_string()
        .context("reading theme body")?;

    // Sanitize remote filename to prevent path traversal.
    let safe_name = Path::new(&remote.name)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("invalid remote theme filename: '{}'", remote.name))?;
    let dest = dest_dir.join(safe_name);
    let theme = Theme::from_str(&remote.name, ThemeSource::User, dest.clone(), &body);
    if !theme.is_renderable() {
        return Err(anyhow!(
            "downloaded theme '{}' had no usable colors",
            remote.name
        ));
    }
    theme
        .save_atomic(&dest)
        .with_context(|| format!("writing {}", dest.display()))?;
    Ok(dest)
}

/// List community Starship presets available for download from GitHub.
///
/// `git_ref` pins the listing — and the download URLs GitHub derives from it —
/// to a release tag such as `v1.26.0`, so presets can't reference modules the
/// installed starship doesn't support yet. `None` lists the default branch.
pub fn list_remote_starship_presets(git_ref: Option<&str>) -> Result<Vec<RemoteStarshipPreset>> {
    let response = match git_ref {
        // A pinned ref can be missing upstream (e.g. starship built from an
        // untagged dev version): fall back to the default branch on 404.
        Some(pinned) => match request_starship_presets(pinned) {
            Err(e) if matches!(e.as_ref(), ureq::Error::Status(404, _)) => {
                request_starship_presets("main")
            }
            other => other,
        },
        None => request_starship_presets("main"),
    }
    .context("requesting Starship presets list from GitHub")?;

    let body = response.into_string().context("reading GitHub response")?;

    let json: serde_json::Value = serde_json::from_str(&body).context("parsing GitHub response")?;
    let arr = json
        .as_array()
        .ok_or_else(|| anyhow!("unexpected GitHub response for Starship presets"))?;

    let mut out = Vec::new();
    for item in arr {
        if item.get("type").and_then(|v| v.as_str()) == Some("file") {
            if let (Some(name), Some(url)) = (
                item.get("name").and_then(|v| v.as_str()),
                item.get("download_url").and_then(|v| v.as_str()),
            ) {
                if name.ends_with(".toml") {
                    let clean_name = name.trim_end_matches(".toml").replace('-', " ");
                    let formatted_name = clean_name
                        .split_whitespace()
                        .map(|w| {
                            let mut c = w.chars();
                            match c.next() {
                                None => String::new(),
                                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    out.push(RemoteStarshipPreset {
                        name: formatted_name,
                        description: format!("Official Starship Preset ({name})"),
                        download_url: url.to_string(),
                    });
                }
            }
        }
    }
    out.sort_by_key(|r| r.name.to_lowercase());
    Ok(out)
}

/// Derive the GitHub ref matching an installed starship version line, e.g.
/// `"starship 1.26.0"` → `"v1.26.0"`. Returns `None` when no `X.Y.Z` version
/// can be found, so callers can fall back to the default branch.
pub fn starship_ref_from_version(version_line: &str) -> Option<String> {
    version_line.split_whitespace().find_map(|token| {
        let end = token
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(token.len());
        let version = &token[..end];
        let parts: Vec<&str> = version.split('.').collect();
        let is_semver = parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
        is_semver.then(|| format!("v{version}"))
    })
}

fn starship_presets_api_url(git_ref: &str) -> String {
    format!(
        "https://api.github.com/repos/starship/starship/contents/docs/public/presets/toml?ref={git_ref}"
    )
}

fn request_starship_presets(git_ref: &str) -> Result<ureq::Response, Box<ureq::Error>> {
    ureq::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .get(&starship_presets_api_url(git_ref))
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(Box::new)
}

/// Download a remote Starship preset's TOML content string.
pub fn download_starship_preset_content(remote: &RemoteStarshipPreset) -> Result<String> {
    let body = ureq::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .get(&remote.download_url)
        .set("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("downloading Starship preset {}", remote.name))?
        .into_string()
        .context("reading preset body")?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_from_homebrew_version_line() {
        assert_eq!(
            starship_ref_from_version("starship 1.26.0"),
            Some("v1.26.0".to_string())
        );
    }

    #[test]
    fn ref_from_bare_version() {
        assert_eq!(
            starship_ref_from_version("1.26.0"),
            Some("v1.26.0".to_string())
        );
    }

    #[test]
    fn ref_from_git_describe_suffix_uses_numeric_prefix() {
        assert_eq!(
            starship_ref_from_version("starship 1.26.0-17-gabc123"),
            Some("v1.26.0".to_string())
        );
    }

    #[test]
    fn ref_rejects_missing_version() {
        assert_eq!(starship_ref_from_version("starship"), None);
        assert_eq!(starship_ref_from_version(""), None);
    }

    #[test]
    fn ref_rejects_incomplete_or_malformed_version() {
        assert_eq!(starship_ref_from_version("starship 1.26"), None);
        assert_eq!(starship_ref_from_version("starship 1..26.0"), None);
    }

    #[test]
    fn presets_url_pins_ref() {
        assert_eq!(
            starship_presets_api_url("v1.26.0"),
            "https://api.github.com/repos/starship/starship/contents/docs/public/presets/toml?ref=v1.26.0"
        );
    }

    #[test]
    fn presets_url_default_branch() {
        assert_eq!(
            starship_presets_api_url("main"),
            "https://api.github.com/repos/starship/starship/contents/docs/public/presets/toml?ref=main"
        );
    }

    // Live smoke tests — run explicitly with `cargo test -- --ignored`.

    #[test]
    #[ignore = "hits the GitHub API"]
    fn live_listing_pins_download_urls_to_ref() {
        let presets = list_remote_starship_presets(Some("v1.26.0")).unwrap();
        assert!(!presets.is_empty());
        assert!(presets
            .iter()
            .all(|p| p.download_url.contains("/starship/starship/v1.26.0/")));
    }

    #[test]
    #[ignore = "hits the GitHub API"]
    fn live_listing_falls_back_to_main_on_unknown_ref() {
        let presets = list_remote_starship_presets(Some("v99.99.99")).unwrap();
        assert!(!presets.is_empty());
    }
}
