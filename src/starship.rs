//! Starship prompt detection, preset management, and installer integration.

use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

/// Official documentation and resources for Starship.
pub const STARSHIP_WEBSITE: &str = "https://starship.rs";
pub const STARSHIP_PRESETS_URL: &str = "https://starship.rs/presets/";

/// Installation and configuration status of Starship on this machine.
#[derive(Debug, Clone)]
pub struct StarshipStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub bin_path: Option<PathBuf>,
    pub config_path: PathBuf,
    pub config_exists: bool,
}

impl StarshipStatus {
    pub fn detect() -> Self {
        // Starship always uses $STARSHIP_CONFIG or $XDG_CONFIG_HOME/starship.toml,
        // falling back to ~/.config/starship.toml — NOT ~/Library/Application Support.
        let config_path = starship_config_path();
        let config_exists = config_path.exists();

        let bin_path = which_starship();
        let (installed, version) = if let Some(ref path) = bin_path {
            let ver = Command::new(path)
                .arg("--version")
                .output()
                .ok()
                .and_then(|out| {
                    if out.status.success() {
                        let s = String::from_utf8_lossy(&out.stdout);
                        s.lines().next().map(|l| l.trim().to_string())
                    } else {
                        None
                    }
                });
            (true, ver)
        } else {
            (false, None)
        };

        StarshipStatus {
            installed,
            version,
            bin_path,
            config_path,
            config_exists,
        }
    }
}

/// Resolve the Starship config path, respecting $STARSHIP_CONFIG and
/// $XDG_CONFIG_HOME, and always falling back to `~/.config/starship.toml`.
fn starship_config_path() -> PathBuf {
    // Empty values are treated as unset (per the XDG spec) — otherwise the
    // path would resolve relative to the current directory.
    if let Some(p) = std::env::var_os("STARSHIP_CONFIG") {
        let p = PathBuf::from(p);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".config")
        });
    xdg.join("starship.toml")
}

fn which_starship() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("starship");
            if candidate.is_file() && is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true // Windows doesn't use permission bits; .is_file() is sufficient.
}

/// An official or curated Starship prompt preset (theme). `Cow` lets the
/// bundled catalog keep zero-cost static strings while downloaded presets own
/// theirs.
#[derive(Debug, Clone)]
pub struct StarshipPreset {
    pub id: Cow<'static, str>,
    pub name: Cow<'static, str>,
    pub description: Cow<'static, str>,
    pub preview: Cow<'static, str>,
    pub toml_content: Cow<'static, str>,
}

/// Outcome of applying a Starship preset.
#[derive(Debug, Clone)]
pub struct StarshipApplyOutcome {
    pub backup_path: Option<PathBuf>,
    pub config_path: PathBuf,
}

/// Return the list of curated official Starship presets.
pub fn official_presets() -> Vec<StarshipPreset> {
    vec![
        StarshipPreset {
            id: "nerd-font-symbols".into(),
            name: "Nerd Font Symbols".into(),
            description: "High-detail icons for git, languages, cloud, and OS environments.".into(),
            preview: "❯ 📁 ~/hauntty ❯  main ⇣1 ❯  v1.80 ❯".into(),
            toml_content: r#"# Starship - Nerd Font Symbols Preset
format = "$all$fill$line_break$character"

[character]
success_symbol = "[❯](bold green)"
error_symbol = "[❯](bold red)"

[directory]
truncation_length = 3
truncation_symbol = "…/"

[git_branch]
symbol = " "

[rust]
symbol = " "
"#
            .into(),
        },
        StarshipPreset {
            id: "no-nerd-fonts".into(),
            name: "No Nerd Fonts".into(),
            description: "Clean standard Unicode symbols compatible with any terminal font.".into(),
            preview: "hauntty on git:main [rust 1.80] >".into(),
            toml_content: r#"# Starship - No Nerd Fonts Preset
[character]
success_symbol = "[>](bold green)"
error_symbol = "[>](bold red)"

[directory]
read_only = " [RO]"

[git_branch]
symbol = "git:"

[rust]
symbol = "rust "
"#
            .into(),
        },
        StarshipPreset {
            id: "tokyo-night".into(),
            name: "Tokyo Night".into(),
            description:
                "Vibrant neon blue, violet, and cyan prompts matching Tokyo Night terminal themes."
                    .into(),
            preview: "󰄛 hauntty   main   1.80 ".into(),
            toml_content: r#"# Starship - Tokyo Night Preset
format = """
[░▒▓](fg:#7aa2f7)\
[ 󰄛 $directory ](fg:#1a1b26 bg:#7aa2f7)\
[](fg:#7aa2f7 bg:#bb9af7)\
[  $git_branch ](fg:#1a1b26 bg:#bb9af7)\
[](fg:#bb9af7 bg:#7dcfff)\
[  $rust ](fg:#1a1b26 bg:#7dcfff)\
[](fg:#7dcfff)\
$character"""

[directory]
style = "fg:#1a1b26 bg:#7aa2f7"
format = "[$path]($style)"

[git_branch]
style = "fg:#1a1b26 bg:#bb9af7"
format = "[$branch]($style)"
"#
            .into(),
        },
        StarshipPreset {
            id: "pastel-powerline".into(),
            name: "Pastel Powerline".into(),
            description: "Soft pastel powerline segments with smooth color transitions.".into(),
            preview: " 📁 hauntty    main   🦀 1.80  ❯".into(),
            toml_content: r#"# Starship - Pastel Powerline Preset
format = """
[░▒▓](fg:#a3be8c)\
[ $directory ](fg:#2e3440 bg:#a3be8c)\
[](fg:#a3be8c bg:#ebcb8b)\
[ $git_branch ](fg:#2e3440 bg:#ebcb8b)\
[](fg:#ebcb8b bg:#b48ead)\
[ $rust ](fg:#2e3440 bg:#b48ead)\
[](fg:#b48ead)\
\n$character"""

[character]
success_symbol = "[❯](bold #a3be8c)"
error_symbol = "[❯](bold #bf616a)"
"#
            .into(),
        },
        StarshipPreset {
            id: "gruvbox-rainbow".into(),
            name: "Gruvbox Rainbow".into(),
            description: "Warm retro yellow, orange, and green segmented prompt.".into(),
            preview: " hauntty  󰊢 main   1.80  ❯".into(),
            toml_content: r#"# Starship - Gruvbox Rainbow Preset
format = """
[░▒▓](fg:#d79921)\
[ $directory ](fg:#282828 bg:#d79921)\
[](fg:#d79921 bg:#fe8019)\
[ $git_branch ](fg:#282828 bg:#fe8019)\
[](fg:#fe8019 bg:#b8bb26)\
[ $rust ](fg:#282828 bg:#b8bb26)\
[](fg:#b8bb26)\
$character"""

[character]
success_symbol = "[❯](bold #b8bb26)"
error_symbol = "[❯](bold #fb4934)"
"#
            .into(),
        },
        StarshipPreset {
            id: "pure-preset".into(),
            name: "Pure Preset".into(),
            description: "Ultra-minimalist two-line prompt layout inspired by Pure Zsh.".into(),
            preview: "hauntty main*\n❯ ".into(),
            toml_content: r#"# Starship - Pure Preset
format = """
$directory\
$git_branch\
$git_status\
$line_break\
$character"""

[directory]
style = "cyan"

[character]
success_symbol = "[❯](bold magenta)"
error_symbol = "[❯](bold red)"
"#
            .into(),
        },
        StarshipPreset {
            id: "bracketed-segments".into(),
            name: "Bracketed Segments".into(),
            description: "Structured bracketed [path] [git] segments.".into(),
            preview: "[~/hauntty] [git:main] [rust:1.80] $ ".into(),
            toml_content: r#"# Starship - Bracketed Segments Preset
format = "[$directory]($style) [$git_branch]($style) $character"

[directory]
format = "\\[[$path]($style)\\]"
style = "bold blue"

[git_branch]
format = "\\[[git:$branch]($style)\\]"
style = "bold purple"
"#
            .into(),
        },
        StarshipPreset {
            id: "plain-text-symbols".into(),
            name: "Plain Text ASCII".into(),
            description: "Lightweight ASCII prompt for basic terminals without custom glyphs."
                .into(),
            preview: "DIR:hauntty GIT:main > ".into(),
            toml_content: r#"# Starship - Plain Text ASCII Preset
format = "$directory $git_branch $character"

[directory]
format = "DIR:$path"

[git_branch]
format = "GIT:$branch"

[character]
success_symbol = "> "
error_symbol = "!> "
"#
            .into(),
        },
    ]
}

/// Apply a Starship preset to `~/.config/starship.toml`, creating a timestamped backup first.
pub fn apply_preset(preset: &StarshipPreset, config_path: &Path) -> Result<StarshipApplyOutcome> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating config dir {}", parent.display()))?;
    }

    let backup_path = if config_path.exists() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut bpath = config_path.with_file_name(format!("starship.toml.bak.{ts}"));
        // Avoid overwriting an earlier backup made in the same second.
        let mut suffix = 1u32;
        while bpath.exists() {
            bpath = config_path.with_file_name(format!("starship.toml.bak.{ts}.{suffix}"));
            suffix += 1;
        }
        fs::copy(config_path, &bpath)
            .with_context(|| format!("backing up starship.toml to {}", bpath.display()))?;
        Some(bpath)
    } else {
        None
    };

    // Atomic write: write to a temp file and rename, so a partial write
    // cannot leave the config empty/truncated. Resolve symlinks first so
    // dotfile-managed configs are written through the link, not replaced.
    let real_config = fs::canonicalize(config_path).unwrap_or_else(|_| config_path.to_path_buf());
    let parent = real_config.parent().unwrap_or_else(|| Path::new("."));
    let tmp_path = parent.join(format!(
        ".starship.toml.hauntty.tmp.{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    // Preserve the destination's permissions (e.g. a chmod 600 config): rename
    // keeps the temp file's mode, so copy the existing mode onto it first.
    let existing_perms = fs::metadata(&real_config).ok().map(|m| m.permissions());
    let write_result = (|| -> std::io::Result<()> {
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        // Must not be ignored: proceeding after a failure here would
        // replace a restrictive config (e.g. 0600) with the temp file's
        // default mode.
        if let Some(ref perms) = existing_perms {
            f.set_permissions(perms.clone())?;
        }
        f.write_all(preset.toml_content.as_bytes())?;
        f.sync_all()?;
        Ok(())
    })();
    if let Err(e) = write_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(e)
            .with_context(|| format!("writing temp starship preset to {}", tmp_path.display()));
    }
    if let Err(e) = fs::rename(&tmp_path, &real_config) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e).with_context(|| format!("renaming temp file to {}", real_config.display()));
    }

    Ok(StarshipApplyOutcome {
        backup_path,
        config_path: config_path.to_path_buf(),
    })
}

/// Attempt to install Starship via Homebrew or official install script.
pub fn install_starship() -> Result<String> {
    // Check if brew is available first
    let brew = Command::new("brew").args(["install", "starship"]).output();

    match brew {
        Ok(out) if out.status.success() => {
            // Verify it actually landed on PATH
            if which_starship().is_some() {
                Ok("Starship successfully installed via Homebrew!".to_string())
            } else {
                anyhow::bail!("Homebrew reported success but `starship` not found on PATH.");
            }
        }
        _ => {
            // Fallback: download the install script separately so we can
            // detect network/curl failures before piping into sh.
            let dl = Command::new("curl")
                .args(["-fsSL", "https://starship.rs/install.sh"])
                .output()
                .context("downloading starship install script")?;

            if !dl.status.success() || dl.stdout.is_empty() {
                let err = String::from_utf8_lossy(&dl.stderr);
                anyhow::bail!("Failed to download install script: {}", err.trim());
            }

            let script = Command::new("sh")
                .args(["-s", "--", "-y"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .context("spawning install script")?;

            use std::io::Write;
            let mut child = script;
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(&dl.stdout)
                    .context("piping install script")?;
            }
            let out = child
                .wait_with_output()
                .context("waiting for install script")?;

            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                anyhow::bail!("Installation failed: {}", err.trim());
            }

            // Final verification
            if which_starship().is_some() {
                Ok("Starship successfully installed via starship.rs script!".to_string())
            } else {
                anyhow::bail!(
                    "Install script completed but `starship` not found on PATH. \
                     You may need to restart your shell."
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RAII temp dir removed on drop, even during panic unwind.
    struct TempDir(PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(unix)]
    #[test]
    fn apply_preset_preserves_config_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("hauntty-starship-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        let dir = TempDir(path);
        let config = dir.0.join("starship.toml");
        fs::write(&config, "format = \"$all\"\n").unwrap();
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();

        let presets = official_presets();
        let outcome = apply_preset(&presets[0], &config).unwrap();
        assert!(outcome.backup_path.is_some());

        let mode = fs::metadata(&config).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "chmod 600 should survive an apply");
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            presets[0].toml_content.as_ref()
        );
    }
}
