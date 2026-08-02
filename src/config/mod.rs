//! Round-trip-safe reading, editing, and atomic writing of a Ghostty config.

mod line;

pub use line::{KeyValue, Line};

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Errors from the config layer.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config i/o error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("key `{0}` appears more than once; refusing to edit it")]
    RepeatedKey(String),
}

type Result<T> = std::result::Result<T, ConfigError>;

/// Line ending style, detected on load and reproduced on save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    Lf,
    Crlf,
}

impl Eol {
    fn as_str(self) -> &'static str {
        match self {
            Eol::Lf => "\n",
            Eol::Crlf => "\r\n",
        }
    }
}

/// A parsed Ghostty config document.
#[derive(Debug, Clone)]
pub struct ConfigDocument {
    pub path: PathBuf,
    pub lines: Vec<Line>,
    eol: Eol,
    trailing_newline: bool,
    /// True if the file did not exist on load (so we skip backup on first save).
    existed: bool,
}

impl ConfigDocument {
    /// Parse config text into a document. Pure; does no I/O.
    pub fn parse(path: impl Into<PathBuf>, content: &str) -> ConfigDocument {
        let path = path.into();
        let eol = if content.contains("\r\n") {
            Eol::Crlf
        } else {
            Eol::Lf
        };
        let trailing_newline = content.ends_with('\n');

        let mut lines = Vec::new();
        if !content.is_empty() {
            let mut parts: Vec<&str> = content.split('\n').collect();
            // A trailing newline yields a final empty element; drop it so it is
            // represented purely by `trailing_newline`.
            if trailing_newline {
                parts.pop();
            }
            for part in parts {
                let raw = part.strip_suffix('\r').unwrap_or(part);
                lines.push(Line::parse(raw));
            }
        }

        ConfigDocument {
            path,
            lines,
            eol,
            trailing_newline,
            existed: true,
        }
    }

    /// Load a config from disk. If the file is missing, returns an empty
    /// document (so the tool can create one on first save).
    pub fn load(path: impl Into<PathBuf>) -> Result<ConfigDocument> {
        let path = path.into();
        match fs::read_to_string(&path) {
            Ok(content) => Ok(ConfigDocument::parse(path, &content)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConfigDocument {
                path,
                lines: Vec::new(),
                eol: Eol::Lf,
                trailing_newline: true,
                existed: false,
            }),
            Err(source) => Err(ConfigError::Io { path, source }),
        }
    }

    /// Render the document back to exact bytes.
    pub fn render(&self) -> String {
        let sep = self.eol.as_str();
        let mut out = self
            .lines
            .iter()
            .map(Line::render)
            .collect::<Vec<_>>()
            .join(sep);
        if self.trailing_newline && !self.lines.is_empty() {
            out.push_str(sep);
        }
        out
    }

    // ---- queries -------------------------------------------------------

    /// Indices of every line with this (case-insensitive) key.
    pub fn indices_of(&self, key: &str) -> Vec<usize> {
        let k = key.trim().to_lowercase();
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| match l {
                Line::KeyValue(kv) if kv.key == k => Some(i),
                _ => None,
            })
            .collect()
    }

    /// Number of active lines with this key.
    pub fn count(&self, key: &str) -> usize {
        self.indices_of(key).len()
    }

    /// The value of a key that appears exactly once, if present.
    pub fn get_single(&self, key: &str) -> Option<&str> {
        let idx = self.indices_of(key);
        if idx.len() == 1 {
            if let Line::KeyValue(kv) = &self.lines[idx[0]] {
                return Some(&kv.value);
            }
        }
        None
    }

    // ---- surgical edits ------------------------------------------------

    /// Set a key that occurs 0 or 1 times. Absent → appended under a managed
    /// section. Present once → only its value substring changes. Present more
    /// than once → error (repeated keys are never in the managed registry).
    pub fn set_single(&mut self, key: &str, value: &str) -> Result<()> {
        let idx = self.indices_of(key);
        match idx.len() {
            0 => {
                self.append_managed(key, value);
                Ok(())
            }
            1 => {
                if let Line::KeyValue(kv) = &mut self.lines[idx[0]] {
                    kv.value = value.to_string();
                    kv.edited = true;
                }
                Ok(())
            }
            _ => Err(ConfigError::RepeatedKey(key.to_string())),
        }
    }

    /// Append a managed `key = value` line under a `# hauntty settings` marker,
    /// creating the marker (with a leading blank line) if it does not exist.
    fn append_managed(&mut self, key: &str, value: &str) {
        const MARKER: &str = "# ── hauntty settings ──";
        let marker_idx = self.lines.iter().position(|l| match l {
            Line::Comment(s) => s.contains("hauntty settings"),
            _ => false,
        });
        match marker_idx {
            Some(i) => {
                self.lines
                    .insert(i + 1, Line::KeyValue(KeyValue::new(key, value)));
            }
            None => {
                if !self.lines.is_empty() {
                    self.lines.push(Line::Blank(String::new()));
                }
                self.lines.push(Line::Comment(MARKER.to_string()));
                self.lines.push(Line::KeyValue(KeyValue::new(key, value)));
            }
        }
    }

    /// Remove every active line with this key. Returns how many were removed.
    pub fn remove_all(&mut self, key: &str) -> usize {
        let k = key.trim().to_lowercase();
        let before = self.lines.len();
        self.lines
            .retain(|l| !matches!(l, Line::KeyValue(kv) if kv.key == k));
        before - self.lines.len()
    }

    // ---- saving --------------------------------------------------------

    /// Atomically write the document to disk, creating a timestamped backup of
    /// any existing file first (`config.bak.YYYYMMDD-HHMMSS`, matching Ghostty
    /// users' own convention). The original is never left truncated: on any
    /// failure the temp file is removed and the original untouched.
    pub fn save(&self) -> Result<Option<PathBuf>> {
        // Resolve symlinks so we write *through* them, not replace them.
        let target_path = fs::canonicalize(&self.path).unwrap_or_else(|_| self.path.clone());
        let dir = target_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        fs::create_dir_all(&dir).map_err(|source| ConfigError::Io {
            path: dir.clone(),
            source,
        })?;

        // 1. Back up the current on-disk file, if it exists.
        let backup = if target_path.exists() {
            let b = self.unique_backup_path(&dir);
            fs::copy(&target_path, &b).map_err(|source| ConfigError::Io {
                path: b.clone(),
                source,
            })?;
            Some(b)
        } else {
            None
        };

        // 2. Write to a temp file in the same directory (unique name).
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = dir.join(format!(
            ".hauntty.config.tmp.{}_{}",
            std::process::id(),
            stamp
        ));
        let rendered = self.render();
        let existing_perms = fs::metadata(&target_path).ok().map(|m| m.permissions());
        let write_result = (|| -> std::io::Result<()> {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            if let Some(ref perms) = existing_perms {
                let _ = f.set_permissions(perms.clone());
            }
            f.write_all(rendered.as_bytes())?;
            f.sync_all()?;
            Ok(())
        })();
        if let Err(source) = write_result {
            let _ = fs::remove_file(&tmp);
            return Err(ConfigError::Io { path: tmp, source });
        }

        // 3. Atomically replace the original.
        if let Err(source) = fs::rename(&tmp, &target_path) {
            let _ = fs::remove_file(&tmp);
            return Err(ConfigError::Io {
                path: target_path,
                source,
            });
        }

        Ok(backup)
    }

    /// A backup path that does not collide with an existing file, suffixing
    /// `-N` if two saves land in the same second.
    fn unique_backup_path(&self, dir: &Path) -> PathBuf {
        let stamp = backup_timestamp();
        let base = dir.join(format!("config.bak.{stamp}"));
        if !base.exists() {
            return base;
        }
        for n in 1..=10_000 {
            let candidate = dir.join(format!("config.bak.{stamp}-{n}"));
            if !candidate.exists() {
                return candidate;
            }
        }
        // Fallback: include PID + nanosecond timestamp for uniqueness.
        dir.join(format!(
            "config.bak.{stamp}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }

    /// Whether the config existed on disk when loaded.
    pub fn existed(&self) -> bool {
        self.existed
    }
}

/// Format the current time as `YYYYMMDD-HHMMSS` in UTC using only `std`.
fn backup_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = civil_from_unix(secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// Convert a Unix timestamp (seconds) into a UTC civil date/time.
/// Uses Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_unix(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hour = (rem / 3600) as u32;
    let minute = ((rem % 3600) / 60) as u32;
    let second = (rem % 60) as u32;

    // days since 1970-01-01 → civil date
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d, hour, minute, second)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# ~/.config/ghostty/config
term = xterm-256color
font-family             = \"Maple Mono NF\"
font-size               = 16

# ── palette ──
background           = 282a36
palette = 0=#21222c
palette = 1=#ff5555
keybind = cmd+d=new_split:right
";

    #[test]
    fn round_trip_is_byte_identical() {
        let doc = ConfigDocument::parse("config", SAMPLE);
        assert_eq!(doc.render(), SAMPLE);
    }

    #[test]
    fn round_trip_no_trailing_newline() {
        let text = "font-size = 16\nbackground = 282a36";
        let doc = ConfigDocument::parse("config", text);
        assert_eq!(doc.render(), text);
    }

    #[test]
    fn round_trip_crlf() {
        let text = "font-size = 16\r\nbackground = 282a36\r\n";
        let doc = ConfigDocument::parse("config", text);
        assert_eq!(doc.render(), text);
    }

    #[test]
    fn round_trip_empty() {
        let doc = ConfigDocument::parse("config", "");
        assert_eq!(doc.render(), "");
    }

    #[test]
    fn set_single_changes_only_value() {
        let doc0 = ConfigDocument::parse("config", SAMPLE);
        let mut doc = doc0.clone();
        doc.set_single("font-size", "18").unwrap();
        let out = doc.render();
        // exactly one line differs
        let diff: Vec<_> = SAMPLE
            .lines()
            .zip(out.lines())
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].1, "font-size               = 18");
        // alignment (pad before '=') preserved
        assert!(diff[0].1.contains("font-size               = 18"));
    }

    #[test]
    fn set_single_refuses_repeated_key() {
        let mut doc = ConfigDocument::parse("config", SAMPLE);
        let err = doc.set_single("palette", "9=#000000").unwrap_err();
        assert!(matches!(err, ConfigError::RepeatedKey(_)));
    }

    #[test]
    fn get_single_and_count() {
        let doc = ConfigDocument::parse("config", SAMPLE);
        assert_eq!(doc.get_single("font-size"), Some("16"));
        assert_eq!(doc.get_single("font-family"), Some("\"Maple Mono NF\""));
        assert_eq!(doc.count("palette"), 2);
        assert_eq!(doc.get_single("palette"), None); // repeated → None
    }

    #[test]
    fn append_managed_when_absent() {
        let mut doc = ConfigDocument::parse("config", SAMPLE);
        doc.set_single("cursor-style", "bar").unwrap();
        let out = doc.render();
        assert!(out.contains("# ── hauntty settings ──"));
        assert!(out.contains("cursor-style = bar"));
        // original content still intact
        assert!(out.starts_with(SAMPLE));
    }

    #[test]
    fn remove_all_removes_repeated() {
        let mut doc = ConfigDocument::parse("config", SAMPLE);
        assert_eq!(doc.remove_all("palette"), 2);
        assert_eq!(doc.count("palette"), 0);
        assert!(doc.render().contains("keybind = cmd+d=new_split:right"));
    }

    #[test]
    fn civil_date_epoch() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
        // 2026-07-23 12:00:00 UTC
        assert_eq!(civil_from_unix(1_784_808_000), (2026, 7, 23, 12, 0, 0));
        // a leap-day boundary: 2024-02-29 00:00:00 UTC
        assert_eq!(civil_from_unix(1_709_164_800), (2024, 2, 29, 0, 0, 0));
    }
}
