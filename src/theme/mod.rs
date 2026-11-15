//! Theme model: loading, indexing, and serializing Ghostty color themes.

pub mod color;
mod parse;

pub use color::Rgb;
pub(crate) use parse::capture_line;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where a theme came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeSource {
    /// Shipped with Ghostty (app bundle / system dir).
    Bundled,
    /// In the user's `~/.config/ghostty/themes` dir.
    User,
}

impl ThemeSource {
    pub fn label(self) -> &'static str {
        match self {
            ThemeSource::Bundled => "bundled",
            ThemeSource::User => "user",
        }
    }
}

/// A parsed color theme. Missing colors are `None` and fall back at render time.
#[derive(Debug, Clone)]
pub struct Theme {
    /// The theme's name — the exact filename, spaces and all. This is what goes
    /// into `theme = <name>`.
    pub name: String,
    pub source: ThemeSource,
    pub path: PathBuf,
    pub palette: [Option<Rgb>; 16],
    pub background: Option<Rgb>,
    pub foreground: Option<Rgb>,
    pub cursor_color: Option<Rgb>,
    pub cursor_text: Option<Rgb>,
    pub selection_background: Option<Rgb>,
    pub selection_foreground: Option<Rgb>,
    /// Color lines whose values Ghostty accepts but the [`Rgb`] model cannot
    /// represent (named X11 colors, `cell-foreground`, palette indices above
    /// 15). Kept as `(slot, verbatim line)` so a later value for the same
    /// slot replaces an earlier one, and re-emitted on save so no color is
    /// ever silently dropped. Ignored by previews.
    pub raw_extras: Vec<(String, String)>,
}

impl Theme {
    /// A theme with a name/source/path but no colors yet.
    pub fn empty(name: &str, source: ThemeSource, path: PathBuf) -> Theme {
        Theme {
            name: name.to_string(),
            source,
            path,
            palette: [None; 16],
            background: None,
            foreground: None,
            cursor_color: None,
            cursor_text: None,
            selection_background: None,
            selection_foreground: None,
            raw_extras: Vec::new(),
        }
    }

    /// Parse a theme from file text.
    pub fn from_str(name: &str, source: ThemeSource, path: PathBuf, content: &str) -> Theme {
        let mut t = Theme::empty(name, source, path);
        parse::parse_into(&mut t, content);
        t
    }

    /// Load a theme from a file on disk.
    pub fn load(path: &Path, source: ThemeSource) -> std::io::Result<Theme> {
        let content = std::fs::read_to_string(path)?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        Ok(Theme::from_str(&name, source, path.to_path_buf(), &content))
    }

    /// True if the theme defined at least a background and one palette color —
    /// enough to render a meaningful preview.
    pub fn is_renderable(&self) -> bool {
        self.background.is_some() && self.palette.iter().any(Option::is_some)
    }

    /// Foreground with a sensible fallback.
    pub fn fg(&self) -> Rgb {
        self.foreground
            .or(self.palette[7])
            .unwrap_or(Rgb::new(0xcc, 0xcc, 0xcc))
    }

    /// Background with a sensible fallback.
    pub fn bg(&self) -> Rgb {
        self.background
            .or(self.palette[0])
            .unwrap_or(Rgb::new(0x10, 0x10, 0x10))
    }

    /// A palette slot with a graceful fallback to the foreground.
    pub fn ansi(&self, i: usize) -> Rgb {
        self.palette.get(i).and_then(|c| *c).unwrap_or(self.fg())
    }

    /// Write the finished theme to a temp file beside `path` (creating the
    /// directory if needed) and return the temp path. Both save paths start
    /// here, so the final name is only ever taken by a complete file.
    fn write_tmp(&self, path: &Path) -> std::io::Result<PathBuf> {
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!(".hauntty.theme.tmp.{}", std::process::id()));
        std::fs::write(&tmp, self.to_ghostty_file())?;
        Ok(tmp)
    }

    /// Atomically write this theme to `path` in Ghostty format (temp file in the
    /// same directory, then rename).
    pub fn save_atomic(&self, path: &Path) -> std::io::Result<()> {
        let tmp = self.write_tmp(path)?;
        std::fs::rename(&tmp, path)
    }

    /// Like [`Theme::save_atomic`], but never replaces an existing filesystem
    /// entry at `path`; returns [`std::io::ErrorKind::AlreadyExists`] instead.
    ///
    /// The finished temp file is hard-linked to `path`. `link(2)` fails with
    /// `EEXIST` when anything already sits there — including a *dangling*
    /// symlink, which `exists()` reports as absent — so the claim is atomic
    /// against a concurrent writer and never clobbers an existing entry. And
    /// since the name is only ever taken by a complete file, an interrupted
    /// save cannot leave an empty placeholder behind for a later save to
    /// refuse and theme discovery to list as a colorless theme.
    ///
    /// Filesystems that refuse hard links fall back to [`claim_then_rename`].
    pub fn save_atomic_new(&self, path: &Path) -> std::io::Result<()> {
        let tmp = self.write_tmp(path)?;
        let claimed = match std::fs::hard_link(&tmp, path) {
            Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => {
                claim_then_rename(&tmp, path)
            }
            linked => linked,
        };
        // After a link `tmp` is a second name for the theme; after a refused
        // claim it is the unwanted write. Either way it goes. (The fallback
        // renames it away itself, in which case this is a harmless no-op.)
        let _ = std::fs::remove_file(&tmp);
        claimed
    }

    /// Serialize to Ghostty theme-file format (`#`-prefixed 6-digit hex).
    pub fn to_ghostty_file(&self) -> String {
        let mut out = String::new();
        for (i, slot) in self.palette.iter().enumerate() {
            if let Some(c) = slot {
                out.push_str(&format!("palette = {i}={}\n", c.to_hex()));
            }
        }
        let mut push = |key: &str, c: Option<Rgb>| {
            if let Some(c) = c {
                out.push_str(&format!("{key} = {}\n", c.to_hex()));
            }
        };
        push("background", self.background);
        push("foreground", self.foreground);
        push("cursor-color", self.cursor_color);
        push("cursor-text", self.cursor_text);
        push("selection-background", self.selection_background);
        push("selection-foreground", self.selection_foreground);
        // Raw lines go last; any field they override was cleared when they
        // were captured, so no key is emitted twice.
        for (_, line) in &self.raw_extras {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}

/// Fallback claim for filesystems without hard links (exFAT, some network
/// mounts): take `path` with `create_new` (`O_CREAT|O_EXCL`), then rename the
/// finished `tmp` over the placeholder. Unlike the link, this has a window
/// between the two syscalls in which an empty `path` exists.
fn claim_then_rename(tmp: &Path, path: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    std::fs::rename(tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(path);
    })
}

/// A collection of themes, indexed by name, sorted for display.
#[derive(Debug, Default, Clone)]
pub struct ThemeSet {
    /// Display order (case-insensitive, numeric-aware sort).
    pub ordered: Vec<Theme>,
}

impl ThemeSet {
    /// Load all themes from the given bundled/system dir(s) and the user dir.
    /// User themes with the same name shadow bundled ones. Missing dirs are
    /// silently skipped. Returns the set plus a list of non-fatal warnings.
    pub fn load(bundled_dirs: &[PathBuf], user_dir: Option<&Path>) -> (ThemeSet, Vec<String>) {
        let mut by_name: BTreeMap<String, Theme> = BTreeMap::new();
        let mut warnings = Vec::new();

        for dir in bundled_dirs {
            load_dir(dir, ThemeSource::Bundled, &mut by_name, &mut warnings);
        }
        if let Some(dir) = user_dir {
            // User themes override bundled ones of the same name.
            load_dir(dir, ThemeSource::User, &mut by_name, &mut warnings);
        }

        let mut ordered: Vec<Theme> = by_name.into_values().collect();
        ordered.sort_by(|a, b| natural_cmp(&a.name, &b.name));
        (ThemeSet { ordered }, warnings)
    }

    pub fn len(&self) -> usize {
        self.ordered.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ordered.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&Theme> {
        self.ordered.iter().find(|t| t.name == name)
    }
}

fn load_dir(
    dir: &Path,
    source: ThemeSource,
    by_name: &mut BTreeMap<String, Theme>,
    warnings: &mut Vec<String>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            warnings.push(format!("could not read {}: {e}", dir.display()));
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        // Skip dotfiles / obvious non-theme files.
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            if name.starts_with('.') {
                continue;
            }
            match Theme::load(&path, source) {
                Ok(theme) => {
                    by_name.insert(theme.name.clone(), theme);
                }
                Err(e) => warnings.push(format!("could not read {}: {e}", path.display())),
            }
        }
    }
}

/// Case-insensitive, digit-aware comparison so `Theme 2` sorts before `Theme 10`.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    // Compare digit runs without heap allocation.
                    let a_zeros = skip_char(&mut ai, '0');
                    let b_zeros = skip_char(&mut bi, '0');
                    match compare_digit_runs(&mut ai, &mut bi) {
                        Ordering::Equal => {
                            // Same numeric value — break tie by number of
                            // leading zeros (fewer zeros = sorts first), giving
                            // a total order so "Theme 01" != "Theme 1".
                            match a_zeros.cmp(&b_zeros) {
                                Ordering::Equal => continue,
                                ord => return ord,
                            }
                        }
                        ord => return ord,
                    }
                } else {
                    let la = ca.to_ascii_lowercase();
                    let lb = cb.to_ascii_lowercase();
                    match la.cmp(&lb) {
                        Ordering::Equal => {
                            ai.next();
                            bi.next();
                        }
                        ord => return ord,
                    }
                }
            }
        }
    }
}

/// Count and skip leading instances of `ch`, returning how many were skipped.
fn skip_char(it: &mut std::iter::Peekable<std::str::Chars>, ch: char) -> usize {
    let mut n = 0;
    while it.peek() == Some(&ch) {
        it.next();
        n += 1;
    }
    n
}

/// Compare two digit runs character-by-character (no allocation). Both
/// iterators are advanced past the digit run.
fn compare_digit_runs(
    a: &mut std::iter::Peekable<std::str::Chars>,
    b: &mut std::iter::Peekable<std::str::Chars>,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut first_diff = Ordering::Equal;
    loop {
        let ad = a.peek().is_some_and(|c| c.is_ascii_digit());
        let bd = b.peek().is_some_and(|c| c.is_ascii_digit());
        match (ad, bd) {
            (true, true) => {
                let ca = a.next().unwrap();
                let cb = b.next().unwrap();
                if first_diff == Ordering::Equal {
                    first_diff = ca.cmp(&cb);
                }
            }
            (true, false) => {
                // a has more digits → a is numerically larger.
                while a.peek().is_some_and(|c| c.is_ascii_digit()) {
                    a.next();
                }
                return Ordering::Greater;
            }
            (false, true) => {
                while b.peek().is_some_and(|c| c.is_ascii_digit()) {
                    b.next();
                }
                return Ordering::Less;
            }
            (false, false) => return first_diff,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_roundtrips_through_parse() {
        let mut t = Theme::empty("t", ThemeSource::User, "t".into());
        t.palette[0] = Some(Rgb::new(0x21, 0x22, 0x2c));
        t.palette[1] = Some(Rgb::new(0xff, 0x55, 0x55));
        t.background = Some(Rgb::new(0x28, 0x2a, 0x36));
        t.foreground = Some(Rgb::new(0xf8, 0xf8, 0xf2));
        let text = t.to_ghostty_file();
        let t2 = Theme::from_str("t", ThemeSource::User, "t".into(), &text);
        assert_eq!(t2.palette[0], t.palette[0]);
        assert_eq!(t2.palette[1], t.palette[1]);
        assert_eq!(t2.background, t.background);
        assert_eq!(t2.foreground, t.foreground);
    }

    #[test]
    fn natural_sort_orders_numbers() {
        let mut v = vec!["Theme 10", "Theme 2", "3024 Night", "abc", "Abd"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["3024 Night", "abc", "Abd", "Theme 2", "Theme 10"]);
    }

    struct TempDir(PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn temp_dir(tag: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!("hauntty-theme-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }

    fn sample_theme() -> Theme {
        let mut t = Theme::empty("t", ThemeSource::User, "t".into());
        t.background = Some(Rgb::new(0x28, 0x2a, 0x36));
        t.palette[0] = Some(Rgb::new(0x21, 0x22, 0x2c));
        t
    }

    /// Sorted entry names of `dir`, to assert no temp file was left behind.
    fn entries(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn save_atomic_new_writes_finished_file_and_leaves_no_temp() {
        let dir = temp_dir("new");
        let dest = dir.0.join("Saved");
        let theme = sample_theme();
        theme.save_atomic_new(&dest).unwrap();
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            theme.to_ghostty_file()
        );
        assert_eq!(entries(&dir.0), ["Saved"]);
    }

    #[test]
    fn save_atomic_new_refuses_existing_file_and_keeps_it() {
        let dir = temp_dir("exists");
        let dest = dir.0.join("Saved");
        std::fs::write(&dest, "background = 000000\n").unwrap();
        let err = sample_theme().save_atomic_new(&dest).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "background = 000000\n"
        );
        assert_eq!(entries(&dir.0), ["Saved"]);
    }

    // `exists()` says a dangling symlink is absent; the link-based claim must
    // still refuse it, and leave the link itself alone.
    #[cfg(unix)]
    #[test]
    fn save_atomic_new_refuses_dangling_symlink() {
        let dir = temp_dir("dangling");
        let dest = dir.0.join("Saved");
        std::os::unix::fs::symlink(dir.0.join("nowhere"), &dest).unwrap();
        let err = sample_theme().save_atomic_new(&dest).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(std::fs::symlink_metadata(&dest).unwrap().is_symlink());
        assert_eq!(entries(&dir.0), ["Saved"]);
    }
}
