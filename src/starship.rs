//! Starship prompt detection, preset management, and installer integration.

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
    if let Ok(p) = std::env::var("STARSHIP_CONFIG") {
        return PathBuf::from(p);
    }
    let xdg = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
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

/// An official or curated Starship prompt preset (theme).
#[derive(Debug, Clone)]
pub struct StarshipPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub preview: &'static str,
    pub toml_content: &'static str,
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
            id: "nerd-font-symbols",
            name: "Nerd Font Symbols",
            description: "High-detail icons for git, languages, cloud, and OS environments.",
            preview: "❯ 📁 ~/hauntty ❯  main ⇣1 ❯  v1.80 ❯",
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
"#,
        },
        StarshipPreset {
            id: "no-nerd-fonts",
            name: "No Nerd Fonts",
            description: "Clean standard Unicode symbols compatible with any terminal font.",
            preview: "hauntty on git:main [rust 1.80] >",
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
"#,
        },
        StarshipPreset {
            id: "tokyo-night",
            name: "Tokyo Night",
            description:
                "Vibrant neon blue, violet, and cyan prompts matching Tokyo Night terminal themes.",
            preview: "󰄛 hauntty   main   1.80 ",
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
"#,
        },
        StarshipPreset {
            id: "pastel-powerline",
            name: "Pastel Powerline",
            description: "Soft pastel powerline segments with smooth color transitions.",
            preview: " 📁 hauntty    main   🦀 1.80  ❯",
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
"#,
        },
        StarshipPreset {
            id: "gruvbox-rainbow",
            name: "Gruvbox Rainbow",
            description: "Warm retro yellow, orange, and green segmented prompt.",
            preview: " hauntty  󰊢 main   1.80  ❯",
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
"#,
        },
        StarshipPreset {
            id: "pure-preset",
            name: "Pure Preset",
            description: "Ultra-minimalist two-line prompt layout inspired by Pure Zsh.",
            preview: "hauntty main*\n❯ ",
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
"#,
        },
        StarshipPreset {
            id: "bracketed-segments",
            name: "Bracketed Segments",
            description: "Structured bracketed [path] [git] segments.",
            preview: "[~/hauntty] [git:main] [rust:1.80] $ ",
            toml_content: r#"# Starship - Bracketed Segments Preset
format = "[$directory]($style) [$git_branch]($style) $character"

[directory]
format = "\\[[$path]($style)\\]"
style = "bold blue"

[git_branch]
format = "\\[[git:$branch]($style)\\]"
style = "bold purple"
"#,
        },
        StarshipPreset {
            id: "plain-text-symbols",
            name: "Plain Text ASCII",
            description: "Lightweight ASCII prompt for basic terminals without custom glyphs.",
            preview: "DIR:hauntty GIT:main > ",
            toml_content: r#"# Starship - Plain Text ASCII Preset
format = "$directory $git_branch $character"

[directory]
format = "DIR:$path"

[git_branch]
format = "GIT:$branch"

[character]
success_symbol = "> "
error_symbol = "!> "
"#,
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
    fs::write(&tmp_path, preset.toml_content)
        .with_context(|| format!("writing temp starship preset to {}", tmp_path.display()))?;
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
