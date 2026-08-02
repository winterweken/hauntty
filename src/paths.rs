//! Cross-platform discovery of Ghostty's config file and theme directories.
//!
//! Everything here is overridable via CLI flags / environment so the tool works
//! on non-standard installs and can be pointed at a throwaway copy for testing.

use std::path::{Path, PathBuf};

/// Resolved locations hauntty operates on.
#[derive(Debug, Clone)]
pub struct Paths {
    /// The Ghostty config file (may not exist yet).
    pub config: PathBuf,
    /// Directories of bundled/system themes, in priority order (first existing wins).
    pub bundled_theme_dirs: Vec<PathBuf>,
    /// The user's theme directory (`<config-dir>/themes`), created on demand.
    pub user_theme_dir: PathBuf,
}

impl Paths {
    /// Resolve using optional explicit overrides (from CLI flags), otherwise
    /// falling back to environment and platform defaults.
    pub fn resolve(config_override: Option<PathBuf>, themes_override: Option<PathBuf>) -> Paths {
        let config = config_override
            .or_else(|| std::env::var_os("HAUNTTY_CONFIG").map(PathBuf::from))
            .unwrap_or_else(default_config_path);

        let config_dir = config
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(ghostty_config_dir);

        let user_theme_dir = config_dir.join("themes");

        let mut bundled_theme_dirs = Vec::new();
        if let Some(dir) = themes_override {
            bundled_theme_dirs.push(dir);
        }
        if let Some(dir) = std::env::var_os("GHOSTTY_RESOURCES_DIR") {
            bundled_theme_dirs.push(PathBuf::from(dir).join("themes"));
        }
        bundled_theme_dirs.extend(platform_bundled_dirs());

        Paths {
            config,
            bundled_theme_dirs,
            user_theme_dir,
        }
    }

    /// The bundled dirs that actually exist on disk.
    pub fn existing_bundled_dirs(&self) -> Vec<PathBuf> {
        self.bundled_theme_dirs
            .iter()
            .filter(|d| d.is_dir())
            .cloned()
            .collect()
    }
}

/// `<config-dir>/ghostty` following XDG, defaulting to `~/.config/ghostty`.
fn ghostty_config_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let p = PathBuf::from(xdg);
        if !p.as_os_str().is_empty() {
            return p.join("ghostty");
        }
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".config").join("ghostty");
    }
    PathBuf::from(".config/ghostty")
}

/// The default config file path, preferring an existing macOS Application
/// Support config if present, else the XDG location.
fn default_config_path() -> PathBuf {
    let xdg = ghostty_config_dir().join("config");
    if xdg.exists() {
        return xdg;
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            let app_support = home
                .join("Library")
                .join("Application Support")
                .join("com.mitchellh.ghostty")
                .join("config");
            if app_support.exists() {
                return app_support;
            }
        }
    }
    // Neither exists yet: default to the XDG path (created on first save).
    xdg
}

/// Candidate directories where Ghostty ships its bundled themes, per platform.
fn platform_bundled_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from(
            "/Applications/Ghostty.app/Contents/Resources/ghostty/themes",
        ));
        // Homebrew cask sometimes symlinks elsewhere; also check a user-local app.
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join("Applications/Ghostty.app/Contents/Resources/ghostty/themes"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/share/ghostty/themes"));
        dirs.push(PathBuf::from("/usr/local/share/ghostty/themes"));
        // Flatpak
        dirs.push(PathBuf::from(
            "/var/lib/flatpak/app/com.mitchellh.ghostty/current/active/files/share/ghostty/themes",
        ));
        if let Some(data) = dirs::data_dir() {
            dirs.push(data.join(
                "flatpak/app/com.mitchellh.ghostty/current/active/files/share/ghostty/themes",
            ));
            dirs.push(data.join("ghostty/themes"));
        }
    }

    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_override_wins() {
        let p = Paths::resolve(
            Some(PathBuf::from("/tmp/x/config")),
            Some(PathBuf::from("/tmp/themes")),
        );
        assert_eq!(p.config, PathBuf::from("/tmp/x/config"));
        assert_eq!(p.user_theme_dir, PathBuf::from("/tmp/x/themes"));
        assert_eq!(p.bundled_theme_dirs[0], PathBuf::from("/tmp/themes"));
    }
}
