//! RGB color parsing and conversion for Ghostty theme files.

/// A 24-bit RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }

    /// Parse a hex color, accepting an optional leading `#` and both 3- and
    /// 6-digit forms (`#abc`, `abc`, `#aabbcc`, `aabbcc`), case-insensitive.
    pub fn parse_hex(s: &str) -> Option<Rgb> {
        let h = s.trim().strip_prefix('#').unwrap_or(s.trim());
        // Guard: hex digits are always ASCII, so reject non-ASCII early to
        // avoid panics from byte-index slicing on multi-byte characters.
        if !h.is_ascii() {
            return None;
        }
        match h.len() {
            3 => {
                let r = u8::from_str_radix(&h[0..1], 16).ok()?;
                let g = u8::from_str_radix(&h[1..2], 16).ok()?;
                let b = u8::from_str_radix(&h[2..3], 16).ok()?;
                Some(Rgb::new(r * 17, g * 17, b * 17))
            }
            4 => {
                let r = u8::from_str_radix(&h[0..1], 16).ok()?;
                let g = u8::from_str_radix(&h[1..2], 16).ok()?;
                let b = u8::from_str_radix(&h[2..3], 16).ok()?;
                let _a = u8::from_str_radix(&h[3..4], 16).ok()?;
                Some(Rgb::new(r * 17, g * 17, b * 17))
            }
            6 => {
                let r = u8::from_str_radix(&h[0..2], 16).ok()?;
                let g = u8::from_str_radix(&h[2..4], 16).ok()?;
                let b = u8::from_str_radix(&h[4..6], 16).ok()?;
                Some(Rgb::new(r, g, b))
            }
            8 => {
                let r = u8::from_str_radix(&h[0..2], 16).ok()?;
                let g = u8::from_str_radix(&h[2..4], 16).ok()?;
                let b = u8::from_str_radix(&h[4..6], 16).ok()?;
                let _a = u8::from_str_radix(&h[6..8], 16).ok()?;
                Some(Rgb::new(r, g, b))
            }
            _ => None,
        }
    }

    /// Render as a `#rrggbb` lowercase hex string (the form theme files use).
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Convert to a ratatui color.
    pub fn to_ratatui(self) -> ratatui::style::Color {
        ratatui::style::Color::Rgb(self.r, self.g, self.b)
    }

    /// Relative luminance (0.0–1.0), used to pick a legible label color.
    pub fn luminance(self) -> f32 {
        let lin = |c: u8| {
            let c = c as f32 / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// Black or white, whichever contrasts better on this color as a background.
    pub fn contrast_text(self) -> Rgb {
        if self.luminance() > 0.179 {
            Rgb::new(0, 0, 0)
        } else {
            Rgb::new(255, 255, 255)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_with_and_without_hash() {
        assert_eq!(Rgb::parse_hex("#282a36"), Some(Rgb::new(0x28, 0x2a, 0x36)));
        assert_eq!(Rgb::parse_hex("282a36"), Some(Rgb::new(0x28, 0x2a, 0x36)));
    }

    #[test]
    fn parses_three_digit() {
        assert_eq!(Rgb::parse_hex("#abc"), Some(Rgb::new(0xaa, 0xbb, 0xcc)));
    }

    #[test]
    fn case_insensitive_and_roundtrip() {
        assert_eq!(Rgb::parse_hex("FF5555"), Some(Rgb::new(255, 85, 85)));
        assert_eq!(Rgb::new(255, 85, 85).to_hex(), "#ff5555");
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(Rgb::parse_hex("nope"), None);
        assert_eq!(Rgb::parse_hex("#12"), None);
    }

    #[test]
    fn contrast_text_picks_readable() {
        assert_eq!(Rgb::new(255, 255, 255).contrast_text(), Rgb::new(0, 0, 0));
        assert_eq!(Rgb::new(0, 0, 0).contrast_text(), Rgb::new(255, 255, 255));
    }

    #[test]
    fn parses_four_and_eight_digit() {
        assert_eq!(Rgb::parse_hex("#abca"), Some(Rgb::new(0xaa, 0xbb, 0xcc)));
        assert_eq!(
            Rgb::parse_hex("#282a36ff"),
            Some(Rgb::new(0x28, 0x2a, 0x36))
        );
    }

    #[test]
    fn non_ascii_returns_none_instead_of_panic() {
        // Multi-byte UTF-8 whose byte length is 3 (like '€') must not panic.
        assert_eq!(Rgb::parse_hex("€"), None);
        assert_eq!(Rgb::parse_hex("#€€"), None);
    }
}
