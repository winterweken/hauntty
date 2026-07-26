//! The curated set of Ghostty settings hauntty exposes, each with a friendly
//! widget so the user never edits raw config text.

/// How a setting is presented and edited.
#[derive(Debug, Clone)]
pub enum Widget {
    /// Free text (e.g. font family). Quoted in the config if it has spaces.
    Text,
    /// A numeric stepper with bounds.
    Stepper {
        min: f64,
        max: f64,
        step: f64,
        /// Decimal places to display/write (0 = integer).
        decimals: usize,
    },
    /// One of a fixed list of values.
    Select(&'static [&'static str]),
    /// A boolean `true`/`false`.
    Toggle,
}

/// A single curated setting.
#[derive(Debug, Clone)]
pub struct SettingSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub widget: Widget,
    /// Ghostty's default, shown dimmed when the key is unset.
    pub default: &'static str,
}

impl SettingSpec {
    /// Format a raw user value for writing to the config (quoting text with
    /// spaces, normalizing numbers to the widget's precision).
    pub fn format_for_config(&self, value: &str) -> String {
        match &self.widget {
            Widget::Text => {
                let v = value.trim();
                if v.contains(char::is_whitespace) && !(v.starts_with('"') && v.ends_with('"')) {
                    format!("\"{v}\"")
                } else {
                    v.to_string()
                }
            }
            Widget::Stepper { decimals, .. } => {
                if let Ok(n) = value.trim().parse::<f64>() {
                    format!("{n:.*}", decimals)
                } else {
                    value.trim().to_string()
                }
            }
            Widget::Select(_) | Widget::Toggle => value.trim().to_string(),
        }
    }

    /// Clamp/step a numeric value in the given direction (+1 / -1).
    pub fn step(&self, current: &str, dir: i32) -> Option<String> {
        if let Widget::Stepper {
            min,
            max,
            step,
            decimals,
        } = &self.widget
        {
            let n = current.trim().parse::<f64>().unwrap_or(*min);
            let next = (n + step * dir as f64).clamp(*min, *max);
            return Some(format!("{next:.*}", decimals));
        }
        None
    }

    /// Cycle a select/toggle value in the given direction.
    pub fn cycle(&self, current: &str, dir: i32) -> Option<String> {
        match &self.widget {
            Widget::Select(opts) => {
                let cur = current.trim();
                let idx = opts.iter().position(|o| *o == cur).unwrap_or(0) as i32;
                let len = opts.len() as i32;
                let next = ((idx + dir) % len + len) % len;
                Some(opts[next as usize].to_string())
            }
            Widget::Toggle => Some(if current.trim() == "true" {
                "false".to_string()
            } else {
                "true".to_string()
            }),
            _ => None,
        }
    }
}

/// The full curated registry, in display order.
pub fn registry() -> Vec<SettingSpec> {
    use Widget::*;
    vec![
        SettingSpec {
            key: "font-family",
            label: "Font family",
            help: "Primary font. Quoted automatically if it contains spaces.",
            widget: Text,
            default: "(system default)",
        },
        SettingSpec {
            key: "font-size",
            label: "Font size",
            help: "Point size of the font.",
            widget: Stepper {
                min: 6.0,
                max: 72.0,
                step: 1.0,
                decimals: 0,
            },
            default: "13",
        },
        SettingSpec {
            key: "background-opacity",
            label: "Background opacity",
            help: "0.0 = transparent, 1.0 = opaque.",
            widget: Stepper {
                min: 0.0,
                max: 1.0,
                step: 0.05,
                decimals: 2,
            },
            default: "1.00",
        },
        SettingSpec {
            key: "window-padding-x",
            label: "Window padding X",
            help: "Horizontal padding inside the window, in points.",
            widget: Stepper {
                min: 0.0,
                max: 100.0,
                step: 1.0,
                decimals: 0,
            },
            default: "2",
        },
        SettingSpec {
            key: "window-padding-y",
            label: "Window padding Y",
            help: "Vertical padding inside the window, in points.",
            widget: Stepper {
                min: 0.0,
                max: 100.0,
                step: 1.0,
                decimals: 0,
            },
            default: "2",
        },
        SettingSpec {
            key: "window-width",
            label: "Window width (cols)",
            help: "Initial window width in columns.",
            widget: Stepper {
                min: 20.0,
                max: 500.0,
                step: 1.0,
                decimals: 0,
            },
            default: "80",
        },
        SettingSpec {
            key: "window-height",
            label: "Window height (rows)",
            help: "Initial window height in rows.",
            widget: Stepper {
                min: 5.0,
                max: 200.0,
                step: 1.0,
                decimals: 0,
            },
            default: "24",
        },
        SettingSpec {
            key: "cursor-style",
            label: "Cursor style",
            help: "Shape of the text cursor.",
            widget: Select(&["block", "bar", "underline"]),
            default: "block",
        },
        SettingSpec {
            key: "cursor-style-blink",
            label: "Cursor blink",
            help: "Whether the cursor blinks.",
            widget: Toggle,
            default: "true",
        },
        SettingSpec {
            key: "copy-on-select",
            label: "Copy on select",
            help: "Copy selected text automatically.",
            widget: Select(&["false", "true", "clipboard"]),
            default: "true",
        },
        SettingSpec {
            key: "macos-titlebar-style",
            label: "macOS titlebar style",
            help: "Titlebar appearance (macOS only).",
            widget: Select(&["native", "transparent", "tabs", "hidden"]),
            default: "transparent",
        },
        SettingSpec {
            key: "confirm-close-surface",
            label: "Confirm close",
            help: "Ask before closing a surface with running processes.",
            widget: Select(&["false", "true", "always"]),
            default: "true",
        },
        SettingSpec {
            key: "scrollback-limit",
            label: "Scrollback limit",
            help: "Max scrollback in bytes.",
            widget: Stepper {
                min: 0.0,
                max: 10_000_000.0,
                step: 10_000.0,
                decimals: 0,
            },
            default: "10000000",
        },
        SettingSpec {
            key: "command",
            label: "Custom command",
            help: "Override default shell executable or startup command.",
            widget: Text,
            default: "(default shell)",
        },
        SettingSpec {
            key: "shell-integration",
            label: "Shell integration",
            help: "Ghostty shell integration (detect, none, zsh, bash, fish).",
            widget: Select(&["detect", "none", "zsh", "bash", "fish"]),
            default: "detect",
        },
        SettingSpec {
            key: "shell-integration-features",
            label: "Shell integration features",
            help: "Features enabled (comma-separated: cursor, sudo, title, etc.).",
            widget: Text,
            default: "cursor,sudo,title",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(key: &str) -> SettingSpec {
        registry().into_iter().find(|s| s.key == key).unwrap()
    }

    #[test]
    fn text_quotes_when_spaces() {
        let s = spec("font-family");
        assert_eq!(s.format_for_config("Maple Mono NF"), "\"Maple Mono NF\"");
        assert_eq!(s.format_for_config("Menlo"), "Menlo");
        assert_eq!(
            s.format_for_config("\"Already Quoted\""),
            "\"Already Quoted\""
        );
    }

    #[test]
    fn stepper_clamps_and_steps() {
        let s = spec("font-size");
        assert_eq!(s.step("16", 1).as_deref(), Some("17"));
        assert_eq!(s.step("72", 1).as_deref(), Some("72")); // clamped
        assert_eq!(s.step("6", -1).as_deref(), Some("6")); // clamped
    }

    #[test]
    fn opacity_respects_decimals() {
        let s = spec("background-opacity");
        assert_eq!(s.step("1.0", -1).as_deref(), Some("0.95"));
        assert_eq!(s.format_for_config("1"), "1.00");
    }

    #[test]
    fn select_cycles_both_ways() {
        let s = spec("cursor-style");
        assert_eq!(s.cycle("block", 1).as_deref(), Some("bar"));
        assert_eq!(s.cycle("block", -1).as_deref(), Some("underline"));
    }

    #[test]
    fn toggle_flips() {
        let s = spec("cursor-style-blink");
        assert_eq!(s.cycle("true", 1).as_deref(), Some("false"));
        assert_eq!(s.cycle("false", 1).as_deref(), Some("true"));
    }
}
