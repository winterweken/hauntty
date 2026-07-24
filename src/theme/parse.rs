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
        let key = key.trim().to_lowercase();
        let value = value.trim();
        match key.as_str() {
            "palette" => {
                // value is "N=#hex"
                if let Some((idx, hex)) = value.split_once('=') {
                    if let (Ok(i), Some(rgb)) =
                        (idx.trim().parse::<usize>(), Rgb::parse_hex(hex.trim()))
                    {
                        if i < 16 {
                            theme.palette[i] = Some(rgb);
                        }
                    }
                }
            }
            "background" => theme.background = Rgb::parse_hex(value),
            "foreground" => theme.foreground = Rgb::parse_hex(value),
            "cursor-color" => theme.cursor_color = Rgb::parse_hex(value),
            "cursor-text" => theme.cursor_text = Rgb::parse_hex(value),
            "selection-background" => theme.selection_background = Rgb::parse_hex(value),
            "selection-foreground" => theme.selection_foreground = Rgb::parse_hex(value),
            _ => {}
        }
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
