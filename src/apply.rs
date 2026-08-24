//! Applying a theme to the config: back up any inline color block as a saved
//! user theme, then replace it with a single `theme = <name>` line.

use std::path::Path;

use anyhow::{Context, Result};

use crate::config::{ConfigDocument, KeyValue, Line};
use crate::theme::{Rgb, Theme, ThemeSource};

/// The inline color keys hauntty manages during a theme apply. If any of these
/// are present, they are (part of) the user's current look and get backed up
/// before removal — whether they stand alone or layer on top of a `theme =`
/// line.
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
    let inline_present = INLINE_COLOR_KEYS.iter().any(|k| doc.count(k) > 0);
    ApplyPlan {
        will_backup: inline_present,
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
/// If the config currently carries inline color lines, those colors are first
/// written out as a user theme named `backup_name` so the current look is
/// never lost. When the inline colors layer on top of an existing `theme =`
/// line, pass that theme as `base_theme` so the backup captures the effective
/// look (base colors with the overrides applied). The backup refuses to
/// overwrite an existing theme file. Only after the backup is safely on disk
/// are the inline lines removed. The user theme dir is created if needed; if
/// that fails we abort **before** touching the config.
pub fn apply_theme(
    doc: &mut ConfigDocument,
    theme_name: &str,
    backup_name: Option<&str>,
    user_theme_dir: &Path,
    base_theme: Option<&Theme>,
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
        let dest = user_theme_dir.join(safe_name);
        let backup = build_theme_from_inline(doc, base_theme, safe_name, dest.clone());

        std::fs::create_dir_all(user_theme_dir).with_context(|| {
            format!(
                "creating user theme dir {} (aborted before changing config)",
                user_theme_dir.display()
            )
        })?;

        if dest.exists() {
            anyhow::bail!(
                "a theme named `{safe_name}` already exists at {} — \
                 choose a different backup name (config not changed)",
                dest.display()
            );
        }
        backup.save_atomic(&dest).with_context(|| {
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

/// A backup theme built from the config's inline color lines: the values we
/// can model as [`Rgb`], plus any color lines whose values Ghostty accepts but
/// hauntty cannot parse (named X11 colors, `cell-foreground`, palette indices
/// above 15, …). Those are preserved verbatim so the written backup file still
/// reproduces the exact effective look.
#[derive(Debug, Clone)]
pub struct InlineBackup {
    pub theme: Theme,
    /// Raw `key = value` lines carried into the backup file unchanged.
    pub raw_lines: Vec<String>,
}

impl InlineBackup {
    /// Serialize to Ghostty theme-file format. Raw lines go last; any field
    /// they override was cleared on the [`Theme`], so no key is emitted twice.
    pub fn to_ghostty_file(&self) -> String {
        let mut out = self.theme.to_ghostty_file();
        for line in &self.raw_lines {
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    /// Atomically write this backup to `path` in Ghostty theme format.
    pub fn save_atomic(&self, path: &Path) -> std::io::Result<()> {
        crate::theme::write_atomic(path, &self.to_ghostty_file())
    }
}

/// Build an [`InlineBackup`] from the config's current inline color values,
/// layered on top of `base` (the currently-applied named theme, if any) so the
/// result reproduces the effective look. Repeated keys follow Ghostty's
/// last-one-wins rule; values we cannot parse are preserved verbatim.
pub fn build_theme_from_inline(
    doc: &ConfigDocument,
    base: Option<&Theme>,
    name: &str,
    path: std::path::PathBuf,
) -> InlineBackup {
    let mut t = Theme::empty(name, ThemeSource::User, path);
    if let Some(base) = base {
        t.palette = base.palette;
        t.background = base.background;
        t.foreground = base.foreground;
        t.cursor_color = base.cursor_color;
        t.cursor_text = base.cursor_text;
        t.selection_background = base.selection_background;
        t.selection_foreground = base.selection_foreground;
    }
    let mut raw_lines: Vec<String> = Vec::new();

    // Ghostty applies a repeated key last-one-wins, so the last occurrence is
    // the effective value. A hex value lands in the theme; an empty value is
    // Ghostty's "reset to default", so the base color must not survive it;
    // anything else (named X11 colors, `cell-foreground`, …) is preserved
    // verbatim rather than silently dropped.
    let mut capture = |key: &str, field: &mut Option<Rgb>| {
        let Some(&idx) = doc.indices_of(key).last() else {
            return;
        };
        let Line::KeyValue(kv) = &doc.lines[idx] else {
            return;
        };
        if kv.value.is_empty() {
            *field = None;
        } else if let Some(rgb) = Rgb::parse_hex(&kv.value) {
            *field = Some(rgb);
        } else {
            *field = None;
            raw_lines.push(format!("{key} = {}", kv.value));
        }
    };
    capture("background", &mut t.background);
    capture("foreground", &mut t.foreground);
    capture("cursor-color", &mut t.cursor_color);
    capture("cursor-text", &mut t.cursor_text);
    capture("selection-background", &mut t.selection_background);
    capture("selection-foreground", &mut t.selection_foreground);

    // Palette lines replay in order under the same last-wins rule, keyed by
    // index. Entries hauntty can model (index 0-15, hex value) land in the
    // palette array; anything else Ghostty may accept (named colors, indices
    // 16-255) is preserved verbatim.
    let mut raw_palette: Vec<(String, String)> = Vec::new();
    for idx in doc.indices_of("palette") {
        let Line::KeyValue(kv) = &doc.lines[idx] else {
            continue;
        };
        let parsed = kv
            .value
            .split_once('=')
            .and_then(|(n, color)| n.trim().parse::<usize>().ok().map(|i| (i, color.trim())));
        // Dedupe raw entries by palette index so a later line for the same
        // slot replaces an earlier one.
        let dedupe_key = parsed.map_or_else(|| kv.value.clone(), |(i, _)| i.to_string());
        raw_palette.retain(|(k, _)| *k != dedupe_key);
        match parsed {
            Some((i, color)) if i < 16 => {
                if color.is_empty() {
                    t.palette[i] = None;
                } else if let Some(rgb) = Rgb::parse_hex(color) {
                    t.palette[i] = Some(rgb);
                } else {
                    t.palette[i] = None;
                    raw_palette.push((dedupe_key, format!("palette = {}", kv.value)));
                }
            }
            _ => raw_palette.push((dedupe_key, format!("palette = {}", kv.value))),
        }
    }
    raw_lines.extend(raw_palette.into_iter().map(|(_, line)| line));

    InlineBackup {
        theme: t,
        raw_lines,
    }
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
        let t = build_theme_from_inline(&doc, None, "MyDracula", "x".into()).theme;
        assert_eq!(t.background, Some(Rgb::new(0x28, 0x2a, 0x36)));
        assert_eq!(t.cursor_color, Some(Rgb::new(0xff, 0x4f, 0xbf)));
        assert_eq!(t.palette[0], Some(Rgb::new(0x21, 0x22, 0x2c)));
        assert_eq!(t.palette[1], Some(Rgb::new(0xff, 0x55, 0x55)));
    }

    #[test]
    fn plan_backs_up_inline_overrides_even_with_theme_line() {
        // `theme =` plus inline overrides is Ghostty's documented layering
        // pattern; the overrides are part of the current look.
        let doc = ConfigDocument::parse("config", "theme = Dracula\nbackground = 000000\n");
        assert!(plan(&doc).will_backup);
    }

    #[test]
    fn build_theme_layers_overrides_on_base() {
        let mut base = Theme::empty("Base", crate::theme::ThemeSource::Bundled, "b".into());
        base.background = Some(Rgb::new(0x11, 0x11, 0x11));
        base.foreground = Some(Rgb::new(0xee, 0xee, 0xee));
        base.palette[4] = Some(Rgb::new(0x00, 0x00, 0xff));
        let doc = ConfigDocument::parse(
            "config",
            "theme = Base\nbackground = 000000\npalette = 4=#7aa2f7\n",
        );
        let t = build_theme_from_inline(&doc, Some(&base), "Backup", "x".into()).theme;
        assert_eq!(t.background, Some(Rgb::new(0x00, 0x00, 0x00))); // override wins
        assert_eq!(t.foreground, Some(Rgb::new(0xee, 0xee, 0xee))); // base survives
        assert_eq!(t.palette[4], Some(Rgb::new(0x7a, 0xa2, 0xf7))); // override wins
    }

    #[test]
    fn build_theme_repeated_key_uses_last_value() {
        // Ghostty applies repeated keys last-one-wins; the backup must capture
        // the winner, not skip the key entirely.
        let doc = ConfigDocument::parse("config", "background = 111111\nbackground = 282a36\n");
        let b = build_theme_from_inline(&doc, None, "Backup", "x".into());
        assert_eq!(b.theme.background, Some(Rgb::new(0x28, 0x2a, 0x36)));
    }

    #[test]
    fn build_theme_preserves_named_color_verbatim() {
        // Ghostty accepts named X11 colors; hauntty cannot model them as Rgb
        // but must not drop them from the backup.
        let doc = ConfigDocument::parse("config", "background = black\n");
        let b = build_theme_from_inline(&doc, None, "Backup", "x".into());
        assert_eq!(b.theme.background, None);
        assert_eq!(b.raw_lines, vec!["background = black".to_string()]);
        assert!(b.to_ghostty_file().contains("background = black\n"));
    }

    #[test]
    fn build_theme_preserves_cell_selection_values() {
        let doc = ConfigDocument::parse(
            "config",
            "selection-foreground = cell-foreground\nselection-background = cell-background\n",
        );
        let b = build_theme_from_inline(&doc, None, "Backup", "x".into());
        let file = b.to_ghostty_file();
        assert!(file.contains("selection-foreground = cell-foreground\n"));
        assert!(file.contains("selection-background = cell-background\n"));
    }

    #[test]
    fn build_theme_raw_value_overrides_base_without_duplicate_key() {
        let mut base = Theme::empty("Base", crate::theme::ThemeSource::Bundled, "b".into());
        base.background = Some(Rgb::new(0x11, 0x11, 0x11));
        let doc = ConfigDocument::parse("config", "theme = Base\nbackground = black\n");
        let b = build_theme_from_inline(&doc, Some(&base), "Backup", "x".into());
        let file = b.to_ghostty_file();
        // The named override wins; the base hex must not shadow or duplicate it.
        assert_eq!(file.matches("background").count(), 1);
        assert!(file.contains("background = black\n"));
    }

    #[test]
    fn build_theme_preserves_named_and_out_of_range_palette_entries() {
        // Ghostty palette indices go up to 255; hauntty models 0-15 but must
        // not lose the rest, nor named palette colors.
        let doc = ConfigDocument::parse("config", "palette = 1=red\npalette = 200=#ff00ff\n");
        let b = build_theme_from_inline(&doc, None, "Backup", "x".into());
        assert_eq!(b.theme.palette[1], None);
        let file = b.to_ghostty_file();
        assert!(file.contains("palette = 1=red\n"));
        assert!(file.contains("palette = 200=#ff00ff\n"));
    }

    #[test]
    fn build_theme_repeated_palette_index_uses_last_value() {
        // hex then named: the named value (raw) wins and the stale hex slot
        // is cleared.
        let doc = ConfigDocument::parse("config", "palette = 1=#ff5555\npalette = 1=red\n");
        let b = build_theme_from_inline(&doc, None, "Backup", "x".into());
        assert_eq!(b.theme.palette[1], None);
        assert_eq!(b.raw_lines, vec!["palette = 1=red".to_string()]);

        // named then hex: the hex wins and no stale raw line remains.
        let doc = ConfigDocument::parse("config", "palette = 1=red\npalette = 1=#ff5555\n");
        let b = build_theme_from_inline(&doc, None, "Backup", "x".into());
        assert_eq!(b.theme.palette[1], Some(Rgb::new(0xff, 0x55, 0x55)));
        assert!(b.raw_lines.is_empty());
    }

    #[test]
    fn build_theme_empty_value_resets_base_color() {
        // Ghostty treats an empty value as "reset to default": the base
        // theme's color must not leak into the backup.
        let mut base = Theme::empty("Base", crate::theme::ThemeSource::Bundled, "b".into());
        base.background = Some(Rgb::new(0x11, 0x11, 0x11));
        let doc = ConfigDocument::parse(
            "config",
            "theme = Base\nforeground = f8f8f2\nbackground =\n",
        );
        let b = build_theme_from_inline(&doc, Some(&base), "Backup", "x".into());
        assert_eq!(b.theme.background, None);
        assert!(b.raw_lines.is_empty());
    }

    /// RAII temp dir removed on drop, even during panic unwind.
    struct TempDir(std::path::PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn temp_dir(tag: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!("hauntty-apply-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }

    #[test]
    fn apply_refuses_backup_name_collision() {
        let dir = temp_dir("collision");
        let themes = dir.0.join("themes");
        std::fs::create_dir_all(&themes).unwrap();
        std::fs::write(themes.join("My Saved Theme"), "background = 111111\n").unwrap();
        let cfg_path = dir.0.join("config");
        std::fs::write(&cfg_path, "background = 282a36\n").unwrap();
        let mut doc = ConfigDocument::load(&cfg_path).unwrap();

        let err = apply_theme(
            &mut doc,
            "Tokyo Night",
            Some("My Saved Theme"),
            &themes,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("already exists"));
        // Neither the earlier backup theme nor the config were touched.
        assert_eq!(
            std::fs::read_to_string(themes.join("My Saved Theme")).unwrap(),
            "background = 111111\n"
        );
        assert_eq!(
            std::fs::read_to_string(&cfg_path).unwrap(),
            "background = 282a36\n"
        );
    }

    #[test]
    fn apply_backs_up_effective_look_when_theme_line_present() {
        let dir = temp_dir("layered");
        let themes = dir.0.join("themes");
        let cfg_path = dir.0.join("config");
        std::fs::write(&cfg_path, "theme = Old\nbackground = 000000\n").unwrap();
        let mut doc = ConfigDocument::load(&cfg_path).unwrap();
        let mut base = Theme::empty("Old", crate::theme::ThemeSource::Bundled, "old".into());
        base.background = Some(Rgb::new(0x28, 0x2a, 0x36));
        base.foreground = Some(Rgb::new(0xf8, 0xf8, 0xf2));

        let outcome = apply_theme(&mut doc, "New", Some("Combo"), &themes, Some(&base)).unwrap();

        let backup =
            std::fs::read_to_string(outcome.backup_theme_path.expect("backup written")).unwrap();
        assert!(backup.contains("background = #000000")); // inline override won
        assert!(backup.contains("foreground = #f8f8f2")); // base color survived
        let cfg = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(cfg.contains("theme = New"));
        assert!(!cfg.contains("background = 000000"));
    }

    #[test]
    fn apply_backs_up_repeated_and_unparseable_values() {
        let dir = temp_dir("effective");
        let themes = dir.0.join("themes");
        let cfg_path = dir.0.join("config");
        std::fs::write(
            &cfg_path,
            "background = 111111\nbackground = 282a36\nselection-foreground = cell-foreground\n",
        )
        .unwrap();
        let mut doc = ConfigDocument::load(&cfg_path).unwrap();

        let outcome = apply_theme(&mut doc, "New", Some("Rescue"), &themes, None).unwrap();

        let backup =
            std::fs::read_to_string(outcome.backup_theme_path.expect("backup written")).unwrap();
        assert!(backup.contains("background = #282a36")); // last-wins captured
        assert!(backup.contains("selection-foreground = cell-foreground")); // preserved verbatim
        let cfg = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(cfg.contains("theme = New"));
        assert!(!cfg.contains("cell-foreground"));
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
