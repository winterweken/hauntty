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
