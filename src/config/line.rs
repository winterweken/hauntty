//! The line-preserving model for a Ghostty config file.
//!
//! Every physical line of the config is kept as a [`Line`]. Lines we do not
//! touch are re-emitted **verbatim**; only a line we explicitly edit is rebuilt
//! from its parts, and even then only the value substring changes. This is what
//! lets `hauntty` edit a config the user never looks at without ever mangling
//! comments, alignment, blank lines, or keys it does not manage.

/// A single physical line of the config file (newline stripped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// Blank / whitespace-only line, stored verbatim.
    Blank(String),
    /// A comment line (first non-whitespace char is `#`), stored verbatim.
    /// This also covers "commented-out keys" like `# font-size = 14`, which we
    /// deliberately leave as comments rather than treating as managed keys.
    Comment(String),
    /// A `key = value` line we understood.
    KeyValue(KeyValue),
    /// Anything we could not classify — stored verbatim, never modified.
    Other(String),
}

/// A parsed `key = value` line. All the surrounding whitespace is captured so an
/// edited line can be rebuilt byte-for-byte except for the value itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyValue {
    /// Exact original bytes of the line. Source of truth while `edited` is false.
    pub raw: String,
    /// Normalized key: trimmed + lowercased. Used for matching.
    pub key: String,
    /// Original key spelling, preserved for rendering edited lines.
    pub key_raw: String,
    /// Trimmed value (the whole RHS after the first `=`).
    pub value: String,
    /// Leading whitespace before the key.
    pub indent: String,
    /// Whitespace between the key and `=` (preserves column alignment).
    pub pad_before_eq: String,
    /// Whitespace between `=` and the value.
    pub pad_after_eq: String,
    /// True once `value` has been changed this session; render then rebuilds
    /// from parts instead of emitting `raw`.
    pub edited: bool,
}

impl KeyValue {
    /// Build a brand-new managed line (used for inserts/appends), rendered as
    /// `key = value`.
    pub fn new(key: &str, value: &str) -> Self {
        KeyValue {
            raw: String::new(),
            key: key.to_lowercase(),
            key_raw: key.to_string(),
            value: value.to_string(),
            indent: String::new(),
            pad_before_eq: " ".to_string(),
            pad_after_eq: " ".to_string(),
            edited: true,
        }
    }

    /// Render this line to its exact bytes (no trailing newline).
    pub fn render(&self) -> String {
        if self.edited {
            format!(
                "{}{}{}={}{}",
                self.indent, self.key_raw, self.pad_before_eq, self.pad_after_eq, self.value
            )
        } else {
            self.raw.clone()
        }
    }
}

impl Line {
    /// Parse one raw line (newline and any trailing `\r` already stripped).
    pub fn parse(raw: &str) -> Line {
        let trimmed_start = raw.trim_start();
        if trimmed_start.is_empty() {
            return Line::Blank(raw.to_string());
        }
        if trimmed_start.starts_with('#') {
            return Line::Comment(raw.to_string());
        }
        // Note: Ghostty does not support inline trailing comments, so the entire
        // RHS after the first `=` is the value.
        if let Some(eq) = raw.find('=') {
            let indent_len = raw.len() - trimmed_start.len();
            let indent = raw[..indent_len].to_string();
            let key_region = &raw[indent_len..eq];
            let key_raw = key_region.trim_end();
            if key_raw.is_empty() {
                // e.g. "= value" — nothing sensible to key on.
                return Line::Other(raw.to_string());
            }
            let pad_before_eq = key_region[key_raw.len()..].to_string();
            let after = &raw[eq + 1..];
            let after_trimmed = after.trim_start();
            let pad_after_eq = after[..after.len() - after_trimmed.len()].to_string();
            let value = after_trimmed.trim_end().to_string();
            return Line::KeyValue(KeyValue {
                raw: raw.to_string(),
                key: key_raw.to_lowercase(),
                key_raw: key_raw.to_string(),
                value,
                indent,
                pad_before_eq,
                pad_after_eq,
                edited: false,
            });
        }
        Line::Other(raw.to_string())
    }

    /// Render this line to its exact bytes (no trailing newline).
    pub fn render(&self) -> String {
        match self {
            Line::Blank(s) | Line::Comment(s) | Line::Other(s) => s.clone(),
            Line::KeyValue(kv) => kv.render(),
        }
    }
}
