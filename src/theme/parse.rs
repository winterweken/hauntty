//! Tolerant parser for Ghostty theme files.
//!
//! A theme file is a sequence of `key = value` lines:
//! ```text
//! palette = 0=#21222c
//! ...
//! palette = 15=#ffffff
//! background = #282a36
//! foreground = #f8f8f2
//! cursor-color = #f8f8f2
//! cursor-text = #282a36
//! selection-background = #44475a
//! selection-foreground = #ffffff
//! ```
//! Unknown or malformed lines are skipped rather than failing the whole file.

use super::color::Rgb;
use super::Theme;

/// Parse theme file text into a [`Theme`]. `name`/`source`/`path` are filled in
/// by the caller; this only fills the colors.
pub fn parse_into(theme: &mut Theme, content: &str) {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        capture_line(theme, &key.trim().to_lowercase(), value.trim());
    }
}

/// Apply one color `key = value` line to the theme with Ghostty's
/// last-one-wins semantics. This is the single capture routine shared by
/// theme-file parsing and inline-config backup, so both layers agree:
///
/// - a hex value lands in the modeled field/slot;
/// - an empty value is Ghostty's "reset to default" and clears it;
/// - anything else Ghostty may accept (named X11 colors, `cell-foreground`,
///   palette indices 16-255) is preserved verbatim in `raw_extras`.
///
/// `key` must be trimmed + lowercased and `value` trimmed. Non-color keys are
/// ignored.
pub(crate) fn capture_line(theme: &mut Theme, key: &str, value: &str) {
    if key == "palette" {
        // value is "N=color"
        let parsed = value
            .split_once('=')
            .and_then(|(n, color)| n.trim().parse::<usize>().ok().map(|i| (i, color.trim())));
        // A later line for the same slot replaces an earlier raw one; entries
        // without a parseable index dedupe on the whole value.
        let slot = parsed.map_or_else(
            || format!("palette {value}"),
            |(i, _)| format!("palette {i}"),
        );
        theme.raw_extras.retain(|(s, _)| *s != slot);
        match parsed {
            Some((i, color)) if i < 16 => {
                if color.is_empty() {
                    theme.palette[i] = None;
                } else if let Some(rgb) = Rgb::parse_hex(color) {
                    theme.palette[i] = Some(rgb);
                } else {
                    theme.palette[i] = None;
                    theme.raw_extras.push((slot, format!("palette = {value}")));
                }
            }
            _ => theme.raw_extras.push((slot, format!("palette = {value}"))),
        }
        return;
    }

    let rgb = Rgb::parse_hex(value);
    let is_raw = !value.is_empty() && rgb.is_none();
    let field = match key {
        "background" => &mut theme.background,
        "foreground" => &mut theme.foreground,
        "cursor-color" => &mut theme.cursor_color,
        "cursor-text" => &mut theme.cursor_text,
        "selection-background" => &mut theme.selection_background,
        "selection-foreground" => &mut theme.selection_foreground,
        _ => return,
    };
    *field = rgb;
    theme.raw_extras.retain(|(s, _)| s != key);
    if is_raw {
        theme
            .raw_extras
            .push((key.to_string(), format!("{key} = {value}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Theme, ThemeSource};

    const DRACULA: &str = "\
palette = 0=#21222c
palette = 1=#ff5555
palette = 15=#ffffff
background = #282a36
foreground = #f8f8f2
cursor-color = #f8f8f2
cursor-text = #282a36
selection-background = #44475a
selection-foreground = #ffffff
";

    #[test]
    fn parses_dracula() {
        let mut t = Theme::empty("Dracula", ThemeSource::Bundled, "Dracula".into());
        parse_into(&mut t, DRACULA);
        assert_eq!(t.palette[0], Some(Rgb::new(0x21, 0x22, 0x2c)));
        assert_eq!(t.palette[1], Some(Rgb::new(0xff, 0x55, 0x55)));
        assert_eq!(t.palette[15], Some(Rgb::new(0xff, 0xff, 0xff)));
        assert_eq!(t.background, Some(Rgb::new(0x28, 0x2a, 0x36)));
        assert_eq!(t.selection_background, Some(Rgb::new(0x44, 0x47, 0x5a)));
    }

    #[test]
    fn tolerates_hex_without_hash() {
        let mut t = Theme::empty("x", ThemeSource::User, "x".into());
        parse_into(&mut t, "background = 282a36\nforeground = f8f8f2\n");
        assert_eq!(t.background, Some(Rgb::new(0x28, 0x2a, 0x36)));
        assert_eq!(t.foreground, Some(Rgb::new(0xf8, 0xf8, 0xf2)));
    }

    #[test]
    fn preserves_unmodelable_values_verbatim() {
        // Named X11 colors, `cell-foreground`, and palette indices above 15
        // are valid Ghostty but unrepresentable as Rgb; they must survive a
        // parse → serialize round trip instead of being dropped.
        let mut t = Theme::empty("x", ThemeSource::User, "x".into());
        parse_into(
            &mut t,
            "background = black\nselection-foreground = cell-foreground\npalette = 200=#ff00ff\npalette = 1=red\n",
        );
        assert_eq!(t.background, None);
        assert_eq!(t.palette[1], None);
        let out = t.to_ghostty_file();
        assert!(out.contains("background = black\n"));
        assert!(out.contains("selection-foreground = cell-foreground\n"));
        assert!(out.contains("palette = 200=#ff00ff\n"));
        assert!(out.contains("palette = 1=red\n"));
    }

    #[test]
    fn repeated_keys_are_last_wins() {
        // Ghostty applies repeated keys last-one-wins, in both directions
        // between modeled and raw values.
        let mut t = Theme::empty("x", ThemeSource::User, "x".into());
        parse_into(&mut t, "background = #111111\nbackground = black\n");
        assert_eq!(t.background, None);
        assert_eq!(t.to_ghostty_file(), "background = black\n");

        let mut t = Theme::empty("x", ThemeSource::User, "x".into());
        parse_into(&mut t, "background = black\nbackground = #111111\n");
        assert_eq!(t.background, Some(Rgb::new(0x11, 0x11, 0x11)));
        assert_eq!(t.to_ghostty_file(), "background = #111111\n");

        let mut t = Theme::empty("x", ThemeSource::User, "x".into());
        parse_into(&mut t, "palette = 1=red\npalette = 1=#ff5555\n");
        assert_eq!(t.palette[1], Some(Rgb::new(0xff, 0x55, 0x55)));
        assert!(t.raw_extras.is_empty());
    }

    #[test]
    fn skips_garbage_lines() {
        let mut t = Theme::empty("x", ThemeSource::User, "x".into());
        parse_into(
            &mut t,
            "this is not valid\npalette = 99=#000000\nbackground = #111111\n",
        );
        assert_eq!(t.background, Some(Rgb::new(0x11, 0x11, 0x11)));
        // palette index out of range ignored
        assert!(t.palette.iter().all(|p| p.is_none()));
    }
}
