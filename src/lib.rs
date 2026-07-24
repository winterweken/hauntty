//! hauntty — a TUI theme & settings manager for the Ghostty terminal.
//!
//! The library crate holds the pure, terminal-free core (config round-trip,
//! theme model, apply logic, settings registry, importers). The binary crate
//! (`main.rs`) wires these to the ratatui UI.

pub mod apply;
pub mod config;
pub mod paths;
pub mod settings;
pub mod theme;

#[cfg(feature = "import-iterm")]
pub mod import;

#[cfg(feature = "online")]
pub mod fetch;
