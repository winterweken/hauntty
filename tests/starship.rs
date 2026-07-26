//! Tests for Starship preset detection, preset application, and backup logic.

use hauntty::starship::{self, StarshipStatus};
use std::fs;

#[test]
fn starship_presets_catalog_has_official_presets() {
    let presets = starship::official_presets();
    assert!(!presets.is_empty(), "Presets catalog should not be empty");
    assert!(
        presets.iter().any(|p| p.id == "nerd-font-symbols"),
        "Catalog should include Nerd Font Symbols preset"
    );
    assert!(
        presets.iter().any(|p| p.id == "tokyo-night"),
        "Catalog should include Tokyo Night preset"
    );
}

#[test]
fn starship_status_detection_runs_without_panic() {
    let status = StarshipStatus::detect();
    assert!(status
        .config_path
        .to_string_lossy()
        .contains("starship.toml"));
}

#[test]
fn starship_apply_preset_creates_backup_and_writes_toml() {
    let temp_dir =
        std::env::temp_dir().join(format!("hauntty-starship-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let cfg_path = temp_dir.join("starship.toml");

    // Write initial config
    fs::write(&cfg_path, "# Initial starship config\n").unwrap();

    let preset = starship::official_presets()
        .into_iter()
        .find(|p| p.id == "tokyo-night")
        .expect("tokyo-night preset should exist");

    let outcome = starship::apply_preset(&preset, &cfg_path).unwrap();

    // Verify backup created
    assert!(
        outcome.backup_path.is_some(),
        "Backup path should be present"
    );
    let backup_path = outcome.backup_path.unwrap();
    assert!(backup_path.exists(), "Backup file should exist on disk");
    assert_eq!(
        fs::read_to_string(&backup_path).unwrap(),
        "# Initial starship config\n"
    );

    // Verify written content matches preset TOML
    let written = fs::read_to_string(&cfg_path).unwrap();
    assert_eq!(written, preset.toml_content);

    let _ = fs::remove_dir_all(&temp_dir);
}
