//! Applying a theme to the config: back up any inline color block as a saved
//! user theme, then replace it with a single `theme = <name>` line.

use std::path::Path;

use anyhow::{Context, Result};

use crate::config::{ConfigDocument, KeyValue, Line};
use crate::theme::{Rgb, Theme, ThemeSource};

/// The inline color keys hauntty manages during a theme apply. If any of these
/// are present with no `theme =` line, they are the user's current look and get
/// backed up before removal.
pub const INLINE_COLOR_KEYS: &[&str] = &[
    "background",
    "foreground",
    "cursor-color",
    "cursor-text",
    "selection-background",
    "selection-foreground",
    "palette",
];

/// What applying a theme will do, computed up front so the UI can confirm.
#[derive(Debug, Clone)]
pub struct ApplyPlan {
    /// True if inline colors will be backed up to a new user theme first.
    pub will_backup: bool,
    /// A suggested name for that backup theme (editable by the user).
    pub suggested_backup_name: String,
}

/// Inspect the document and decide what an apply would entail.
pub fn plan(doc: &ConfigDocument) -> ApplyPlan {
    let has_theme = doc.count("theme") > 0;
    let inline_present = INLINE_COLOR_KEYS.iter().any(|k| doc.count(k) > 0);
    ApplyPlan {
        will_backup: inline_present && !has_theme,
        suggested_backup_name: "My Saved Theme".to_string(),
    }
}

/// Result of applying a theme.
#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    /// Path of the backup theme file written, if any.
    pub backup_theme_path: Option<std::path::PathBuf>,
    /// Path of the config backup written by save, if any.
    pub config_backup_path: Option<std::path::PathBuf>,
}

/// Apply `theme_name` to `doc` and save it atomically.
///
/// If the config currently carries an inline color block (and no `theme =`
/// line), those colors are first written out as a user theme named
/// `backup_name` so the current look is never lost. Only after that backup is
/// safely on disk are the inline lines removed. The user theme dir is created
/// if needed; if that fails we abort **before** touching the config.
pub fn apply_theme(
    doc: &mut ConfigDocument,
    theme_name: &str,
    backup_name: Option<&str>,
    user_theme_dir: &Path,
) -> Result<ApplyOutcome> {
    let apply_plan = plan(doc);

    let mut backup_theme_path = None;
    if apply_plan.will_backup {
        let name = backup_name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&apply_plan.suggested_backup_name);
        // Sanitize: strip any path components to prevent traversal.
        let safe_name = Path::new(name)
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("backup_theme");
        let theme = build_theme_from_inline(doc, safe_name, user_theme_dir.join(safe_name));

        std::fs::create_dir_all(user_theme_dir).with_context(|| {
            format!(
                "creating user theme dir {} (aborted before changing config)",
                user_theme_dir.display()
            )
        })?;

        let dest = user_theme_dir.join(safe_name);
        theme.save_atomic(&dest).with_context(|| {
            format!(
                "saving current colors as theme {} (config not changed)",
                dest.display()
            )
        })?;
        backup_theme_path = Some(dest);
    }

    neutralize_and_set_theme(doc, theme_name);

    let config_backup_path = doc.save().context("saving config")?;

    Ok(ApplyOutcome {
        backup_theme_path,
        config_backup_path,
    })
}

/// Build a [`Theme`] from the config's current inline color values.
pub fn build_theme_from_inline(
    doc: &ConfigDocument,
    name: &str,
    path: std::path::PathBuf,
) -> Theme {
    let mut t = Theme::empty(name, ThemeSource::User, path);
    let get = |k: &str| doc.get_single(k).and_then(Rgb::parse_hex);
    t.background = get("background");
    t.foreground = get("foreground");
    t.cursor_color = get("cursor-color");
    t.cursor_text = get("cursor-text");
    t.selection_background = get("selection-background");
    t.selection_foreground = get("selection-foreground");
    for idx in doc.indices_of("palette") {
        if let Line::KeyValue(kv) = &doc.lines[idx] {
            if let Some((n, hex)) = kv.value.split_once('=') {
                if let (Ok(i), Some(rgb)) = (n.trim().parse::<usize>(), Rgb::parse_hex(hex.trim()))
                {
                    if i < 16 {
                        t.palette[i] = Some(rgb);
                    }
                }
            }
        }
    }
    t
}

/// Remove every inline color line and ensure exactly one `theme = <name>` line.
/// The theme line takes the position of the removed block (or replaces an
/// existing `theme =` line, or is appended if neither exists).
pub fn neutralize_and_set_theme(doc: &mut ConfigDocument, theme_name: &str) {
    let has_theme = doc.count("theme") > 0;
    let mut inserted = false;
    let old = std::mem::take(&mut doc.lines);
    let mut new_lines = Vec::with_capacity(old.len());

    for line in old {
        match &line {
            Line::KeyValue(kv) if kv.key == "theme" => {
                if !inserted {
                    let mut kv = kv.clone();
                    kv.value = theme_name.to_string();
                    kv.edited = true;
                    new_lines.push(Line::KeyValue(kv));
                    inserted = true;
                }
                // Duplicate theme lines are dropped.
            }
            Line::KeyValue(kv) if INLINE_COLOR_KEYS.contains(&kv.key.as_str()) => {
                // Drop the inline color line. If there's no existing theme line,
                // the first dropped color line's slot becomes the theme line.
                if !has_theme && !inserted {
                    new_lines.push(Line::KeyValue(KeyValue::new("theme", theme_name)));
                    inserted = true;
                }
            }
            _ => new_lines.push(line),
        }
    }

    if !inserted {
        if !new_lines.is_empty() {
            new_lines.push(Line::Blank(String::new()));
        }
        new_lines.push(Line::KeyValue(KeyValue::new("theme", theme_name)));
    }

    doc.lines = new_lines;
}

#[cfg(test)]
mod tests {
    use super::*;

    const INLINE_CONFIG: &str = "\
term = xterm-256color
font-size = 16

# ── Dracula Pink palette ──
background           = 282a36
foreground           = f8f8f2
cursor-color         = ff4fbf
palette = 0=#21222c
palette = 1=#ff5555

window-width = 130
keybind = cmd+d=new_split:right
";

    #[test]
    fn plan_detects_backup_needed() {
        let doc = ConfigDocument::parse("config", INLINE_CONFIG);
        assert!(plan(&doc).will_backup);
    }

    #[test]
    fn plan_no_backup_when_theme_line_present() {
        let doc = ConfigDocument::parse("config", "theme = Dracula\nfont-size = 16\n");
        assert!(!plan(&doc).will_backup);
    }

    #[test]
    fn build_theme_reads_inline_colors() {
        let doc = ConfigDocument::parse("config", INLINE_CONFIG);
        let t = build_theme_from_inline(&doc, "MyDracula", "x".into());
        assert_eq!(t.background, Some(Rgb::new(0x28, 0x2a, 0x36)));
        assert_eq!(t.cursor_color, Some(Rgb::new(0xff, 0x4f, 0xbf)));
        assert_eq!(t.palette[0], Some(Rgb::new(0x21, 0x22, 0x2c)));
        assert_eq!(t.palette[1], Some(Rgb::new(0xff, 0x55, 0x55)));
    }

    #[test]
    fn neutralize_removes_colors_and_inserts_theme() {
        let mut doc = ConfigDocument::parse("config", INLINE_CONFIG);
        neutralize_and_set_theme(&mut doc, "Tokyo Night");
        let out = doc.render();
        // no inline color lines remain
        assert!(!out.contains("background           = 282a36"));
        assert!(!out.contains("palette = 0=#21222c"));
        // exactly one theme line, at the old block's position
        assert_eq!(out.matches("theme = Tokyo Night").count(), 1);
        // untouched lines preserved byte-for-byte
        assert!(out.contains("window-width = 130"));
        assert!(out.contains("keybind = cmd+d=new_split:right"));
        assert!(out.contains("# ── Dracula Pink palette ──"));
        // theme line sits where the color block was (before window-width)
        let theme_pos = out.find("theme = Tokyo Night").unwrap();
        let win_pos = out.find("window-width").unwrap();
        assert!(theme_pos < win_pos);
    }

    #[test]
    fn neutralize_replaces_existing_theme_line() {
        let mut doc = ConfigDocument::parse("config", "font-size = 16\ntheme = Old\n");
        neutralize_and_set_theme(&mut doc, "New");
        assert_eq!(doc.render(), "font-size = 16\ntheme = New\n");
    }

    #[test]
    fn neutralize_deduplicates_multiple_theme_lines() {
        let mut doc =
            ConfigDocument::parse("config", "font-size = 16\ntheme = Old1\ntheme = Old2\n");
        neutralize_and_set_theme(&mut doc, "New");
        assert_eq!(doc.render(), "font-size = 16\ntheme = New\n");
    }

    #[test]
    fn neutralize_appends_when_nothing_present() {
        let mut doc = ConfigDocument::parse("config", "font-size = 16\n");
        neutralize_and_set_theme(&mut doc, "Dracula");
        let out = doc.render();
        assert!(out.contains("font-size = 16"));
        assert!(out.contains("theme = Dracula"));
    }
}
