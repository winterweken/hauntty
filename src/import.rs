//! Import iTerm2 `.itermcolors` files (XML plists) into Ghostty theme files.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use plist::Value;

use crate::theme::{Rgb, Theme, ThemeSource};

/// Parse an `.itermcolors` file and write the equivalent Ghostty theme into
/// `dest_dir`, returning the written path. The theme name is derived from the
/// source filename stem.
pub fn import_itermcolors(src: &Path, dest_dir: &Path) -> Result<PathBuf> {
    let name = src
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("could not derive a name from {}", src.display()))?
        .to_string();

    let file = std::fs::File::open(src).with_context(|| format!("opening {}", src.display()))?;
    let theme = parse_itermcolors(file, &name, dest_dir.join(&name))
        .with_context(|| format!("parsing {}", src.display()))?;

    let dest = dest_dir.join(&name);
    theme
        .save_atomic(&dest)
        .with_context(|| format!("writing {}", dest.display()))?;
    Ok(dest)
}

/// Parse `.itermcolors` content from any reader into a [`Theme`].
pub fn parse_itermcolors(reader: impl Read, name: &str, path: PathBuf) -> Result<Theme> {
    let value = Value::from_reader_xml(reader).context("not a valid .itermcolors plist")?;
    let dict = value
        .as_dictionary()
        .ok_or_else(|| anyhow!("expected a plist dictionary"))?;

    let color = |key: &str| -> Option<Rgb> {
        let c = dict.get(key)?.as_dictionary()?;
        let comp = |k: &str| -> Option<u8> {
            let v = c.get(k)?;
            let f = v
                .as_real()
                .or_else(|| v.as_signed_integer().map(|i| i as f64))?;
            Some((f * 255.0).round().clamp(0.0, 255.0) as u8)
        };
        Some(Rgb::new(
            comp("Red Component")?,
            comp("Green Component")?,
            comp("Blue Component")?,
        ))
    };

    let mut theme = Theme::empty(name, ThemeSource::User, path);
    for i in 0..16 {
        theme.palette[i] = color(&format!("Ansi {i} Color"));
    }
    theme.background = color("Background Color");
    theme.foreground = color("Foreground Color");
    theme.cursor_color = color("Cursor Color");
    theme.cursor_text = color("Cursor Text Color");
    theme.selection_background = color("Selection Color");
    theme.selection_foreground = color("Selected Text Color");

    if !theme.is_renderable() {
        return Err(anyhow!("no usable colors found"));
    }
    Ok(theme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Ansi 0 Color</key>
  <dict>
    <key>Red Component</key><real>0.0</real>
    <key>Green Component</key><real>0.0</real>
    <key>Blue Component</key><real>0.0</real>
  </dict>
  <key>Ansi 1 Color</key>
  <dict>
    <key>Red Component</key><real>1.0</real>
    <key>Green Component</key><real>0.3333333333333333</real>
    <key>Blue Component</key><real>0.3333333333333333</real>
  </dict>
  <key>Background Color</key>
  <dict>
    <key>Red Component</key><real>0.15686274509803921</real>
    <key>Green Component</key><real>0.16470588235294117</real>
    <key>Blue Component</key><real>0.21176470588235294</real>
  </dict>
  <key>Foreground Color</key>
  <dict>
    <key>Red Component</key><real>0.97254901960784312</real>
    <key>Green Component</key><real>0.97254901960784312</real>
    <key>Blue Component</key><real>0.94901960784313721</real>
  </dict>
</dict>
</plist>
"#;

    #[test]
    fn parses_components_to_255() {
        let t = parse_itermcolors(Cursor::new(SAMPLE), "Sample", "Sample".into()).unwrap();
        assert_eq!(t.palette[0], Some(Rgb::new(0, 0, 0)));
        assert_eq!(t.palette[1], Some(Rgb::new(255, 85, 85)));
        // 0.1568... * 255 ≈ 40 = 0x28
        assert_eq!(t.background, Some(Rgb::new(0x28, 0x2a, 0x36)));
        assert_eq!(t.foreground, Some(Rgb::new(0xf8, 0xf8, 0xf2)));
    }

    #[test]
    fn rejects_non_plist() {
        let err = parse_itermcolors(Cursor::new("not xml"), "x", "x".into());
        assert!(err.is_err());
    }
}
