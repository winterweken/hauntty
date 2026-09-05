//! hauntty — a TUI theme & settings manager for the Ghostty terminal.
//!
//! The library crate holds the pure, terminal-free core (config round-trip,
//! theme model, apply logic, settings registry, importers). The binary crate
//! (`main.rs`) wires these to the ratatui UI.
//!
//! # Stability
//!
//! This library target exists to serve the `hauntty` binary and the integration
//! tests. It is published only because the binary is, and it carries **no API
//! stability guarantee**: any item here may change or disappear in any release,
//! including a patch release. Depend on the `hauntty` binary, not on this crate
//! as a library.

pub mod apply;
pub mod config;
pub mod paths;
pub mod settings;
pub mod starship;
pub mod theme;

#[cfg(feature = "import-iterm")]
pub mod import;

#[cfg(feature = "online")]
pub mod fetch;
