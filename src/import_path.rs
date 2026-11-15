//! Local file input: accept terminal quoting without executing shell syntax.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

pub fn resolve(input: &str) -> Result<PathBuf> {
    let input = input.trim();
    if input.is_empty() {
        bail!("Drop a .itermcolors file here, paste its path, or press Tab to browse.");
    }
    // Real filenames may contain literal quotes and backslashes.
    let literal = expand_home(input);
    if literal.exists() {
        return Ok(literal);
    }
    let mut path = String::new();
    let mut quote = None;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('\''), _) => path.push(c),
            (None, '\'' | '"') => quote = Some(c),
            (None, '\\') => path.push(chars.next().context("Incomplete path escape.")?),
            (Some('"'), '\\') => {
                if chars
                    .peek()
                    .is_some_and(|c| matches!(c, '"' | '\\' | '$' | '`'))
                {
                    path.push(chars.next().unwrap());
                } else {
                    path.push(c);
                }
            }
            _ => path.push(c),
        }
    }
    if quote.is_some() {
        bail!("Unclosed quote in file path.");
    }
    if path.is_empty() {
        bail!("Choose a .itermcolors file.");
    }
    Ok(expand_home(&path))
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" || path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(path.strip_prefix("~/").unwrap_or(""));
        }
    }
    PathBuf::from(path)
}

pub struct Entry {
    pub path: PathBuf,
    pub is_dir: bool,
}

pub struct Browser {
    pub directory: PathBuf,
    pub entries: Vec<Entry>,
    pub selected: usize,
}

impl Browser {
    pub fn open(directory: &Path) -> Result<Self> {
        // Absolute paths keep parent navigation working for relative input.
        let directory = &std::fs::canonicalize(directory)
            .with_context(|| format!("opening {}", directory.display()))?;
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(directory)
            .with_context(|| format!("opening {}", directory.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let is_dir = path.is_dir();
            if is_dir
                || path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("itermcolors"))
            {
                entries.push(Entry { path, is_dir });
            }
        }
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.path.cmp(&b.path)));
        if let Some(parent) = directory.parent() {
            entries.insert(
                0,
                Entry {
                    path: parent.to_path_buf(),
                    is_dir: true,
                },
            );
        }
        Ok(Self {
            directory: directory.to_path_buf(),
            entries,
            selected: 0,
        })
    }

    pub fn move_selection(&mut self, delta: i32) {
        self.selected = (self.selected as i64 + delta as i64)
            .clamp(0, self.entries.len().saturating_sub(1) as i64) as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_quoted_escaped_and_plain_paths() {
        for input in [
            "/tmp/My Theme.itermcolors",
            " '/tmp/My Theme.itermcolors' ",
            "\"/tmp/My Theme.itermcolors\"",
            r"/tmp/My\ Theme.itermcolors ",
        ] {
            assert_eq!(
                resolve(input).unwrap(),
                Path::new("/tmp/My Theme.itermcolors")
            );
        }
        for input in [
            r"/tmp/O\'Brien.itermcolors",
            r#""/tmp/O'Brien.itermcolors""#,
            r"'/tmp/O'\''Brien.itermcolors'",
        ] {
            assert_eq!(
                resolve(input).unwrap(),
                Path::new("/tmp/O'Brien.itermcolors")
            );
        }
        assert_eq!(
            resolve("./theme.itermcolors").unwrap(),
            Path::new("./theme.itermcolors")
        );
        if let Some(home) = dirs::home_dir() {
            assert_eq!(
                resolve("'~/My Theme.itermcolors'").unwrap(),
                home.join("My Theme.itermcolors")
            );
        }
    }

    #[test]
    fn rejects_empty_and_incomplete_paths() {
        for input in ["", "  ", "''", "\"\"", "'/tmp/theme", "/tmp/theme\\"] {
            assert!(resolve(input).is_err(), "{input:?}");
        }
    }
}
