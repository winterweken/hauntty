//! Byte-stability of the config round-trip against real-world files on this
//! machine. These tests skip (rather than fail) when the files aren't present,
//! so they're safe in CI and on machines without Ghostty installed.

use std::fs;

use hauntty::config::ConfigDocument;
use hauntty::paths::Paths;

#[test]
fn roundtrip_all_bundled_theme_files_byte_identical() {
    let dirs = Paths::resolve(None, None).existing_bundled_dirs();
    if dirs.is_empty() {
        eprintln!("skip: no bundled Ghostty themes found on this machine");
        return;
    }

    let mut checked = 0usize;
    let mut failures = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                let doc = ConfigDocument::parse(&path, &content);
                if doc.render() != content {
                    failures.push(path.display().to_string());
                }
                checked += 1;
            }
        }
    }

    assert!(
        checked > 0,
        "found bundled theme dirs but no readable files"
    );
    assert!(
        failures.is_empty(),
        "{}/{} theme files did not round-trip byte-identically; first few: {:?}",
        failures.len(),
        checked,
        &failures[..failures.len().min(5)]
    );
    eprintln!("round-tripped {checked} bundled theme files byte-identically");
}

#[test]
fn roundtrip_real_user_config_byte_identical() {
    let path = Paths::resolve(None, None).config;
    let Ok(content) = fs::read_to_string(&path) else {
        eprintln!("skip: no user config at {}", path.display());
        return;
    };
    let doc = ConfigDocument::parse(&path, &content);
    assert_eq!(
        doc.render(),
        content,
        "real user config did not round-trip byte-identically"
    );
    eprintln!("round-tripped real user config byte-identically");
}
