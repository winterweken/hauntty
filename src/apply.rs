//! Applying a theme to the config: back up any inline color block as a saved
//! user theme, then replace it with a single `theme = <name>` line.

use std::path::Path;

use anyhow::{Context, Result};

use crate::config::{ConfigDocument, KeyValue, Line};
use crate::theme::{Theme, ThemeSource};

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

        // Claim the name atomically rather than checking `exists()` first: two
        // hauntty instances racing on the same backup name must not both pass
        // the check and have the loser's rename silently eat the winner's
        // backup, and a dangling symlink at `dest` must be refused, not
        // replaced.
        backup.save_atomic_new(&dest).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                anyhow::anyhow!(
                    "a theme named `{safe_name}` already exists at {} — \
                     choose a different backup name (config not changed)",
                    dest.display()
                )
            } else {
                anyhow::Error::new(e).context(format!(
                    "saving current colors as theme {} (config not changed)",
                    dest.display()
                ))
            }
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

/// Build a backup [`Theme`] from the config's current inline color values,
/// layered on top of `base` (the currently-applied named theme, if any) so the
/// result reproduces the effective look. The inline lines replay in document
/// order with Ghostty's last-one-wins semantics, through the same capture
/// routine the theme-file parser uses: hex values land in the modeled fields,
/// an empty value resets a key to Ghostty's default, and values the model
/// cannot represent (named X11 colors, `cell-foreground`, palette indices
/// above 15) are preserved verbatim in `raw_extras` — from the base theme and
/// the inline lines alike — so nothing is silently dropped.
pub fn build_theme_from_inline(
    doc: &ConfigDocument,
    base: Option<&Theme>,
    name: &str,
    path: std::path::PathBuf,
) -> Theme {
    let mut t = Theme::empty(name, ThemeSource::User, path);
    if let Some(base) = base {
        t.palette = base.palette;
        t.background = base.background;
        t.foreground = base.foreground;
        t.cursor_color = base.cursor_color;
        t.cursor_text = base.cursor_text;
        t.selection_background = base.selection_background;
        t.selection_foreground = base.selection_foreground;
        t.raw_extras = base.raw_extras.clone();
    }
    for line in &doc.lines {
        if let Line::KeyValue(kv) = line {
            if INLINE_COLOR_KEYS.contains(&kv.key.as_str()) {
                crate::theme::capture_line(&mut t, &kv.key, &kv.value);
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
    use crate::theme::Rgb;

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
        let t = build_theme_from_inline(&doc, None, "MyDracula", "x".into());
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
        let t = build_theme_from_inline(&doc, Some(&base), "Backup", "x".into());
        assert_eq!(t.background, Some(Rgb::new(0x00, 0x00, 0x00))); // override wins
        assert_eq!(t.foreground, Some(Rgb::new(0xee, 0xee, 0xee))); // base survives
        assert_eq!(t.palette[4], Some(Rgb::new(0x7a, 0xa2, 0xf7))); // override wins
    }

    #[test]
    fn build_theme_repeated_key_uses_last_value() {
        // Ghostty applies repeated keys last-one-wins; the backup must capture
        // the winner, not skip the key entirely.
        let doc = ConfigDocument::parse("config", "background = 111111\nbackground = 282a36\n");
        let t = build_theme_from_inline(&doc, None, "Backup", "x".into());
        assert_eq!(t.background, Some(Rgb::new(0x28, 0x2a, 0x36)));
    }

    #[test]
    fn build_theme_preserves_named_color_verbatim() {
        // Ghostty accepts named X11 colors; hauntty cannot model them as Rgb
        // but must not drop them from the backup.
        let doc = ConfigDocument::parse("config", "background = black\n");
        let t = build_theme_from_inline(&doc, None, "Backup", "x".into());
        assert_eq!(t.background, None);
        assert_eq!(t.to_ghostty_file(), "background = black\n");
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
        assert_eq!(b.palette[1], None);
        let file = b.to_ghostty_file();
        assert!(file.contains("palette = 1=red\n"));
        assert!(file.contains("palette = 200=#ff00ff\n"));
    }

    #[test]
    fn build_theme_repeated_palette_index_uses_last_value() {
        // hex then named: the named value (raw) wins and the stale hex slot
        // is cleared.
        let doc = ConfigDocument::parse("config", "palette = 1=#ff5555\npalette = 1=red\n");
        let t = build_theme_from_inline(&doc, None, "Backup", "x".into());
        assert_eq!(t.palette[1], None);
        assert_eq!(t.to_ghostty_file(), "palette = 1=red\n");

        // named then hex: the hex wins and no stale raw line remains.
        let doc = ConfigDocument::parse("config", "palette = 1=red\npalette = 1=#ff5555\n");
        let t = build_theme_from_inline(&doc, None, "Backup", "x".into());
        assert_eq!(t.palette[1], Some(Rgb::new(0xff, 0x55, 0x55)));
        assert!(t.raw_extras.is_empty());
    }

    #[test]
    fn build_theme_keeps_base_raw_values() {
        // A base theme can itself carry values the Rgb model cannot
        // represent; layering inline overrides on top must not drop them.
        let base = Theme::from_str(
            "Base",
            crate::theme::ThemeSource::Bundled,
            "b".into(),
            "background = black\nselection-foreground = cell-foreground\npalette = 200=#ff00ff\n",
        );

        let doc = ConfigDocument::parse("config", "theme = Base\nforeground = f8f8f2\n");
        let out =
            build_theme_from_inline(&doc, Some(&base), "Backup", "x".into()).to_ghostty_file();
        assert!(out.contains("background = black\n")); // base raw survives
        assert!(out.contains("selection-foreground = cell-foreground\n"));
        assert!(out.contains("palette = 200=#ff00ff\n"));
        assert!(out.contains("foreground = #f8f8f2\n")); // inline override captured

        // An inline override of the same key replaces the base's raw value…
        let doc = ConfigDocument::parse("config", "theme = Base\nbackground = 282a36\n");
        let out =
            build_theme_from_inline(&doc, Some(&base), "Backup", "x".into()).to_ghostty_file();
        assert!(out.contains("background = #282a36\n"));
        assert!(!out.contains("black"));

        // …and an inline empty value resets it.
        let doc = ConfigDocument::parse("config", "theme = Base\nselection-foreground =\n");
        let out =
            build_theme_from_inline(&doc, Some(&base), "Backup", "x".into()).to_ghostty_file();
        assert!(!out.contains("selection-foreground"));
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
        let t = build_theme_from_inline(&doc, Some(&base), "Backup", "x".into());
        assert_eq!(t.background, None);
        assert!(t.raw_extras.is_empty());
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

    // A dangling symlink is absent as far as `Path::exists()` is concerned,
    // so the old check-then-rename would have destroyed the link. The
    // `create_new` claim refuses it instead.
    #[cfg(unix)]
    #[test]
    fn apply_refuses_backup_over_dangling_symlink() {
        let dir = temp_dir("dangling");
        let themes = dir.0.join("themes");
        std::fs::create_dir_all(&themes).unwrap();
        let link = themes.join("My Saved Theme");
        std::os::unix::fs::symlink(dir.0.join("nowhere"), &link).unwrap();
        assert!(!link.exists(), "precondition: dangling link looks absent");

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
        // The link itself survived, and the config was not touched.
        assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(
            std::fs::read_to_string(&cfg_path).unwrap(),
            "background = 282a36\n"
        );
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
