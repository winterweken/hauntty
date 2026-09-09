# Repository guide

## Project

hauntty is a Rust terminal UI for managing Ghostty themes and settings and
Starship prompt presets. It uses ratatui and crossterm and supports macOS and
Linux. Rust edition is 2021; minimum supported Rust is 1.88. Keep `rust-version`
in `Cargo.toml` and the MSRV job in `.github/workflows/ci.yml` in sync.

## Code layout

- `src/main.rs`: CLI arguments, terminal lifecycle, and event loop.
- `src/app.rs`: application state and actions; `src/event.rs`: keyboard handling.
- `src/ui/`: rendering and live theme preview.
- `src/lib.rs`: core library module exports; the library serves the binary and
  tests and has no API stability guarantee.
- `src/config/`: lossless config parsing, rendering, backups, and file writes.
- `src/apply.rs`: theme application and preservation of the previous appearance.
- `src/theme/`: theme loading, color parsing, and theme model.
- `src/settings.rs`: editable settings registry and widget definitions.
- `src/paths.rs`: platform-specific config and theme discovery.
- `src/starship.rs`: Starship detection, presets, installation, and config writes.
- `src/import.rs` and `src/fetch.rs`: optional iTerm2 import and online catalogs.
- `tests/`: config round trips, file application, and Starship integration tests.
  Unit tests live beside implementation; TUI smoke tests are in `src/smoke_test.rs`.

## Build and validation

```sh
cargo build --release
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo check --no-default-features
```

CI runs format, Clippy, and tests on Rust 1.92.0 on macOS and Linux, plus an
all-targets/all-features check on Rust 1.88.0. Default features are `import-iterm`
and `online`; preserve compilation without them. Run checks appropriate to the
change and report any that could not run. Some round-trip tests depend on local
Ghostty files; inspect their behavior before interpreting a pass as coverage.

For manual testing, use `cargo run -- --config /path/to/test/config` and optionally
`--themes-dir /path/to/test/themes`. Use temporary paths for file-writing tests;
do not modify the developer's real Ghostty or Starship configuration.

## Behavior to preserve

- Config edits must preserve unmanaged lines, comments, formatting, and line
  endings. Avoid replacing lossless parsing with a normalized serializer.
- Maintain timestamped backups, atomic writes, locking, symlink handling, and
  permission preservation in the existing file-writing paths.
- Preserve effective colors when saving the previous theme, including repeated
  keys and raw values the RGB model cannot represent. Refuse lossy operations.
- Preserve repeated settings such as font fallback stacks rather than silently
  collapsing them to a single value.
- Keep terminal restoration on exit and panic, and keep network work from
  blocking the interactive event loop.
- Gate optional dependencies and their callers with the matching Cargo feature.

## Releases

`scripts/release.sh` performs publishing actions, including commits, pushes,
tags, release workflow coordination, and Homebrew tap updates. Run it only when
release work is requested. See `README.md` for installation, usage, and release
channel details.
