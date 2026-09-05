//! End-to-end test of the on-disk apply flow: backup inline colors → write a
//! theme file → replace inline block with `theme =` → atomic save with a
//! config backup. Runs entirely on a throwaway config in a temp dir.

use std::fs;
use std::path::PathBuf;

use hauntty::apply;
use hauntty::config::ConfigDocument;

/// Mirrors the user's real config shape: inline "Dracula Pink", no `theme =`.
const REAL_SHAPE: &str = "\
term = xterm-256color
font-family             = \"Maple Mono NF\"
font-size               = 16

# ── Dracula Pink palette ──
background           = 282a36
foreground           = f8f8f2
cursor-color         = ff4fbf
cursor-text          = 282a36
selection-background = ff4fbf
selection-foreground = 282a36
palette = 0=#21222c
palette = 1=#ff5555
palette = 15=#ffffff

# ── Window ──
window-width          = 130
keybind = cmd+d=new_split:right
";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hauntty-test-{}-{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn apply_backs_up_inline_colors_and_switches_to_theme_line() {
    let dir = temp_dir("apply");
    let config_path = dir.join("config");
    fs::write(&config_path, REAL_SHAPE).unwrap();
    let user_themes = dir.join("themes");

    let mut doc = ConfigDocument::load(&config_path).unwrap();
    let outcome = apply::apply_theme(
        &mut doc,
        "Tokyo Night",
        Some("My Dracula"),
        &user_themes,
        None,
    )
    .unwrap();

    // 1. Config now has exactly one theme line and no inline colors.
    let written = fs::read_to_string(&config_path).unwrap();
    assert_eq!(written.matches("theme = Tokyo Night").count(), 1);
    assert!(!written.contains("palette = 0=#21222c"));
    assert!(!written.contains("background           = 282a36"));

    // 2. Untouched lines preserved byte-for-byte.
    assert!(written.contains("font-family             = \"Maple Mono NF\""));
    assert!(written.contains("window-width          = 130"));
    assert!(written.contains("keybind = cmd+d=new_split:right"));
    assert!(written.contains("# ── Dracula Pink palette ──"));

    // 3. A config backup was written.
    let backup = outcome
        .config_backup_path
        .expect("expected a config backup");
    assert!(backup.exists());
    assert_eq!(fs::read_to_string(&backup).unwrap(), REAL_SHAPE);

    // 4. The current colors were saved as a named theme file...
    let theme_file = outcome.backup_theme_path.expect("expected a saved theme");
    assert_eq!(theme_file, user_themes.join("My Dracula"));
    let theme_text = fs::read_to_string(&theme_file).unwrap();
    // ...serialized in canonical Ghostty form (# + 6-digit hex).
    assert!(theme_text.contains("palette = 0=#21222c"));
    assert!(theme_text.contains("background = #282a36"));
    assert!(theme_text.contains("cursor-color = #ff4fbf"));

    // 5. Re-applying now that a theme line exists (and no inline colors
    //    remain) needs no further backup.
    let mut doc2 = ConfigDocument::load(&config_path).unwrap();
    assert!(!apply::plan(&doc2).will_backup);
    let outcome2 = apply::apply_theme(&mut doc2, "Dracula", None, &user_themes, None).unwrap();
    assert!(outcome2.backup_theme_path.is_none());
    let written2 = fs::read_to_string(&config_path).unwrap();
    assert_eq!(written2.matches("theme = ").count(), 1);
    assert!(written2.contains("theme = Dracula"));

    fs::remove_dir_all(&dir).unwrap();
}
