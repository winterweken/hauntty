//! Application state and logic for the hauntty TUI.

use anyhow::Result;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use hauntty::apply;
use hauntty::config::ConfigDocument;
use hauntty::paths::Paths;
use hauntty::settings::{SettingSpec, Widget};
use hauntty::theme::{Theme, ThemeSet};

use std::sync::mpsc::{Receiver, TryRecvError};

#[cfg(feature = "online")]
use hauntty::fetch::{self, RemoteStarshipPreset, RemoteTheme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Themes,
    Settings,
    Starship,
}

/// The current interaction mode (drives which overlay/handler is active).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Filter,
    Confirm,
    Input,
    Help,
    #[cfg(feature = "online")]
    Fetch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
}

/// State of the apply-confirmation modal.
#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub theme_name: String,
    pub will_backup: bool,
    pub backup_name: String,
    /// True while the user is editing the backup name field.
    pub editing_name: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputPurpose {
    /// Edit a Ghostty setting by its config key.
    Setting(String),
    #[cfg(feature = "import-iterm")]
    ImportPath,
}

/// A generic single-line text input overlay.
#[derive(Debug, Clone)]
pub struct InputState {
    pub title: String,
    pub buffer: String,
    pub purpose: InputPurpose,
}

#[cfg(feature = "online")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchTarget {
    Themes,
    Starship,
}

#[cfg(feature = "online")]
pub struct FetchState {
    pub target: FetchTarget,
    pub remotes: Vec<RemoteTheme>,
    pub starship_remotes: Vec<RemoteStarshipPreset>,
    pub filter: String,
    pub filtered: Vec<usize>,
    pub selected: usize,
}

/// A result delivered from a background network thread. Errors are carried as
/// strings so the message is `Send` regardless of the underlying error type.
#[cfg(feature = "online")]
enum FetchMsg {
    ListThemes(std::result::Result<Vec<RemoteTheme>, String>),
    DownloadTheme(std::result::Result<String, String>),
    ListStarship(std::result::Result<Vec<RemoteStarshipPreset>, String>),
    DownloadStarship(std::result::Result<(String, String), String>),
}

pub struct App {
    pub paths: Paths,
    pub config: ConfigDocument,
    pub themes: ThemeSet,
    pub warnings: Vec<String>,
    pub settings: Vec<SettingSpec>,

    pub tab: Tab,
    pub mode: Mode,
    pub should_quit: bool,
    /// Set when the user pressed quit with unsaved changes; a second quit
    /// discards them.
    pub armed_quit: bool,

    // Themes tab
    pub filter: String,
    pub filtered: Vec<usize>,
    pub theme_selected: usize,

    // Settings tab
    pub setting_selected: usize,
    pub dirty: bool,

    // Starship tab
    pub starship_status: hauntty::starship::StarshipStatus,
    pub starship_presets: Vec<hauntty::starship::StarshipPreset>,
    pub starship_selected: usize,
    pub starship_filter: String,
    pub starship_filtered: Vec<usize>,

    // Overlays
    pub toast: Option<Toast>,
    pub confirm: Option<ConfirmState>,
    pub input: Option<InputState>,
    #[cfg(feature = "online")]
    pub fetch: Option<FetchState>,
    /// Receiver for the in-flight background network request, if any.
    #[cfg(feature = "online")]
    fetch_rx: Option<Receiver<FetchMsg>>,
    /// True while a network request is running (drives the spinner + guards
    /// against launching a second one).
    #[cfg(feature = "online")]
    pub fetching: bool,
    /// Spinner animation tick.
    #[cfg(feature = "online")]
    pub spinner: usize,

    /// Receiver for an in-flight Starship install, if any.
    starship_install_rx: Option<Receiver<Result<String, String>>>,

    matcher: Matcher,
}

impl App {
    pub fn new(paths: Paths) -> Result<App> {
        let config = ConfigDocument::load(&paths.config)?;
        let bundled = paths.existing_bundled_dirs();
        let (themes, mut warnings) = ThemeSet::load(&bundled, Some(&paths.user_theme_dir));
        if bundled.is_empty() {
            warnings.push(format!(
                "No bundled Ghostty themes found. Looked in: {}",
                paths
                    .bundled_theme_dirs
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        let mut app = App {
            paths,
            config,
            themes,
            warnings,
            settings: hauntty::settings::registry(),
            tab: Tab::Themes,
            mode: Mode::Normal,
            should_quit: false,
            armed_quit: false,
            filter: String::new(),
            filtered: Vec::new(),
            theme_selected: 0,
            setting_selected: 0,
            dirty: false,
            starship_status: hauntty::starship::StarshipStatus::detect(),

            starship_presets: hauntty::starship::official_presets(),
            starship_selected: 0,
            starship_filter: String::new(),
            starship_filtered: Vec::new(),
            toast: None,
            confirm: None,
            input: None,
            #[cfg(feature = "online")]
            fetch: None,
            #[cfg(feature = "online")]
            fetch_rx: None,
            #[cfg(feature = "online")]
            fetching: false,
            #[cfg(feature = "online")]
            spinner: 0,
            starship_install_rx: None,
            matcher: Matcher::new(Config::DEFAULT),
        };
        app.recompute_filter();
        app.recompute_starship_filter();
        Ok(app)
    }

    // ---- toasts --------------------------------------------------------

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.toast = Some(Toast {
            text: text.into(),
            kind,
        });
    }

    // ---- themes: filtering & selection --------------------------------

    pub fn recompute_filter(&mut self) {
        if self.filter.is_empty() {
            self.filtered = (0..self.themes.len()).collect();
        } else {
            let pattern = Pattern::parse(&self.filter, CaseMatching::Ignore, Normalization::Smart);
            let mut buf = Vec::new();
            let mut scored: Vec<(u32, usize)> = Vec::new();
            for (i, t) in self.themes.ordered.iter().enumerate() {
                let hay = Utf32Str::new(&t.name, &mut buf);
                if let Some(score) = pattern.score(hay, &mut self.matcher) {
                    scored.push((score, i));
                }
            }
            scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            self.filtered = scored.into_iter().map(|(_, i)| i).collect();
        }
        if self.theme_selected >= self.filtered.len() {
            self.theme_selected = self.filtered.len().saturating_sub(1);
        }
    }

    pub fn current_theme(&self) -> Option<&Theme> {
        self.filtered
            .get(self.theme_selected)
            .and_then(|&i| self.themes.ordered.get(i))
    }

    pub fn move_theme(&mut self, delta: i32) {
        if self.filtered.is_empty() {
            return;
        }
        let len = self.filtered.len() as i32;
        let next = (self.theme_selected as i32 + delta).clamp(0, len - 1);
        self.theme_selected = next as usize;
    }

    pub fn move_setting(&mut self, delta: i32) {
        if self.settings.is_empty() {
            return;
        }
        let len = self.settings.len() as i32;
        let next = (self.setting_selected as i32 + delta).clamp(0, len - 1);
        self.setting_selected = next as usize;
    }

    pub fn reload_themes(&mut self) {
        let bundled = self.paths.existing_bundled_dirs();
        let (themes, warnings) = ThemeSet::load(&bundled, Some(&self.paths.user_theme_dir));
        self.themes = themes;
        self.warnings = warnings;
        self.recompute_filter();
    }

    // ---- apply flow ----------------------------------------------------

    pub fn start_apply(&mut self) {
        let Some(theme) = self.current_theme() else {
            return;
        };
        let theme_name = theme.name.clone();
        let plan = apply::plan(&self.config);
        // Suggest a backup name based on any detected inline look.
        self.confirm = Some(ConfirmState {
            theme_name,
            will_backup: plan.will_backup,
            backup_name: plan.suggested_backup_name,
            editing_name: false,
        });
        self.mode = Mode::Confirm;
    }

    pub fn confirm_apply(&mut self) {
        let Some(confirm) = self.confirm.take() else {
            self.mode = Mode::Normal;
            return;
        };
        self.mode = Mode::Normal;
        // Validate backup name: reject path separators and traversal sequences.
        if confirm.will_backup {
            let trimmed = confirm.backup_name.trim();
            if trimmed.is_empty()
                || trimmed.contains('/')
                || trimmed.contains('\\')
                || trimmed.contains("..")
            {
                self.toast(
                    ToastKind::Error,
                    "Invalid backup name — path separators and '..' are not allowed",
                );
                return;
            }
            // Refuse names that collide with any known theme: overwriting a
            // user theme would destroy it, and reusing a bundled theme's name
            // would shadow it.
            if self.themes.get(trimmed).is_some() {
                self.toast(
                    ToastKind::Error,
                    format!(
                        "A theme named '{trimmed}' already exists — choose a different backup name"
                    ),
                );
                return;
            }
        }
        let backup = if confirm.will_backup {
            Some(confirm.backup_name.as_str())
        } else {
            None
        };
        // The currently-applied named theme, if any, so the backup captures
        // the effective look (base theme + inline overrides). Ghostty's last
        // `theme =` line wins, so resolve against the last one.
        let base_theme = self
            .config
            .indices_of("theme")
            .last()
            .and_then(|&i| match &self.config.lines[i] {
                hauntty::config::Line::KeyValue(kv) => Some(strip_quotes(&kv.value)),
                _ => None,
            })
            .and_then(|name| self.themes.get(&name).cloned());
        match apply::apply_theme(
            &mut self.config,
            &confirm.theme_name,
            backup,
            &self.paths.user_theme_dir,
            base_theme.as_ref(),
        ) {
            Ok(outcome) => {
                self.dirty = false;
                let mut msg = format!(
                    "Applied '{}'.  Reload Ghostty with ⌘⇧, (cmd+shift+,)",
                    confirm.theme_name
                );
                if outcome.backup_theme_path.is_some() {
                    msg = format!("Saved your colors as '{}'. {msg}", confirm.backup_name);
                }
                self.toast(ToastKind::Success, msg);
                if outcome.backup_theme_path.is_some() {
                    self.reload_themes();
                }
            }
            Err(e) => self.toast(ToastKind::Error, format!("Apply failed: {e:#}")),
        }
    }

    pub fn cancel_overlay(&mut self) {
        self.confirm = None;
        self.input = None;
        #[cfg(feature = "online")]
        {
            self.fetch = None;
            // Abandon any in-flight request; its result will be dropped.
            self.fetch_rx = None;
            self.fetching = false;
        }
        self.mode = Mode::Normal;
    }

    /// Advance the spinner animation (called once per UI tick).
    pub fn tick(&mut self) {
        #[cfg(feature = "online")]
        if self.fetching {
            self.spinner = self.spinner.wrapping_add(1);
        }
    }

    /// Drain any completed background network result and apply it.
    #[cfg(feature = "online")]
    pub fn poll_background(&mut self) {
        let msg = match &self.fetch_rx {
            Some(rx) => match rx.try_recv() {
                Ok(m) => m,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.fetch_rx = None;
                    self.fetching = false;
                    return;
                }
            },
            None => return,
        };
        self.fetch_rx = None;
        self.fetching = false;
        match msg {
            FetchMsg::ListThemes(Ok(remotes)) => {
                if self.mode != Mode::Fetch {
                    return;
                }
                let filtered = (0..remotes.len()).collect();
                self.fetch = Some(FetchState {
                    target: FetchTarget::Themes,
                    remotes,
                    starship_remotes: Vec::new(),
                    filter: String::new(),
                    filtered,
                    selected: 0,
                });
            }
            FetchMsg::ListThemes(Err(e)) => {
                self.mode = Mode::Normal;
                self.fetch = None;
                self.toast(ToastKind::Error, format!("Fetch failed: {e}"));
            }
            FetchMsg::DownloadTheme(Ok(name)) => {
                self.reload_themes();
                self.toast(ToastKind::Success, format!("Downloaded '{name}'."));
            }
            FetchMsg::DownloadTheme(Err(e)) => {
                self.toast(ToastKind::Error, format!("Download failed: {e}"));
            }
            FetchMsg::ListStarship(Ok(remotes)) => {
                if self.mode != Mode::Fetch {
                    return;
                }
                let filtered = (0..remotes.len()).collect();
                self.fetch = Some(FetchState {
                    target: FetchTarget::Starship,
                    remotes: Vec::new(),
                    starship_remotes: remotes,
                    filter: String::new(),
                    filtered,
                    selected: 0,
                });
            }
            FetchMsg::ListStarship(Err(e)) => {
                self.mode = Mode::Normal;
                self.fetch = None;
                self.toast(ToastKind::Error, format!("Preset fetch failed: {e}"));
            }
            FetchMsg::DownloadStarship(Ok((name, content))) => {
                let preset = hauntty::starship::StarshipPreset {
                    id: format!("remote-{name}").into(),
                    name: name.clone().into(),
                    description: "Downloaded remote preset (previewing)".into(),
                    preview: "Remote Preset".into(),
                    toml_content: content.into(),
                };
                // Re-downloading a preset replaces the earlier copy instead of
                // duplicating it in the list.
                let new_idx = match self.starship_presets.iter().position(|p| p.id == preset.id) {
                    Some(i) => {
                        self.starship_presets[i] = preset;
                        i
                    }
                    None => {
                        self.starship_presets.push(preset);
                        self.starship_presets.len() - 1
                    }
                };
                // Clear any active filter so the new preset is visible, then
                // select it — `starship_selected` indexes `starship_filtered`,
                // not `starship_presets`.
                self.starship_filter.clear();
                self.recompute_starship_filter();
                self.starship_selected = self
                    .starship_filtered
                    .iter()
                    .position(|&i| i == new_idx)
                    .unwrap_or(0);
                self.mode = Mode::Normal;
                self.tab = Tab::Starship;
                self.toast(
                    ToastKind::Success,
                    format!("Downloaded preset '{name}'. Press Enter to apply."),
                );
            }
            FetchMsg::DownloadStarship(Err(e)) => {
                self.toast(ToastKind::Error, format!("Download failed: {e}"));
            }
        }
    }

    /// No-op when the online feature is disabled.
    #[cfg(not(feature = "online"))]
    pub fn poll_background(&mut self) {}

    /// Drain a completed Starship install result, if any.
    pub fn poll_starship_install(&mut self) {
        let msg = match &self.starship_install_rx {
            Some(rx) => match rx.try_recv() {
                Ok(m) => m,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.starship_install_rx = None;
                    return;
                }
            },
            None => return,
        };
        self.starship_install_rx = None;
        match msg {
            Ok(success_msg) => {
                self.starship_status = hauntty::starship::StarshipStatus::detect();
                self.toast(ToastKind::Success, success_msg);
            }
            Err(e) => self.toast(ToastKind::Error, format!("Installation failed: {e}")),
        }
    }

    /// The current spinner glyph.
    #[cfg(feature = "online")]
    pub fn spinner_frame(&self) -> char {
        const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        FRAMES[self.spinner % FRAMES.len()]
    }

    // ---- settings ------------------------------------------------------

    /// The displayed value of a setting and whether it is the (unset) default.
    pub fn setting_value(&self, i: usize) -> (String, bool) {
        let spec = &self.settings[i];
        // A repeated key (e.g. a font-family fallback stack) is set, not
        // default — but has no single value to display or edit.
        if self.config.count(spec.key) > 1 {
            return ("(multiple entries)".to_string(), false);
        }
        match self.config.get_single(spec.key) {
            Some(v) => (strip_quotes(v), false),
            None => (spec.default.to_string(), true),
        }
    }

    /// Explain why a repeated key cannot be edited from the settings list.
    fn toast_repeated_key(&mut self, key: &str) {
        self.toast(
            ToastKind::Info,
            format!("{key} appears multiple times in the config (a fallback list) — edit the file directly."),
        );
    }

    /// Adjust the selected setting by `dir` (+1/-1). No-op for text widgets.
    pub fn adjust_setting(&mut self, dir: i32) {
        let i = self.setting_selected;
        if i >= self.settings.len() {
            return;
        }
        let spec = self.settings[i].clone();
        if self.config.count(spec.key) > 1 {
            self.toast_repeated_key(spec.key);
            return;
        }
        let (cur, is_default) = self.setting_value(i);
        // If unset, start from the default value.
        let base = if is_default {
            spec.default.to_string()
        } else {
            cur
        };
        let new_value = match &spec.widget {
            Widget::Stepper { .. } => spec.step(&base, dir),
            Widget::Select(_) | Widget::Toggle => spec.cycle(&base, dir),
            Widget::Text => None,
        };
        if let Some(v) = new_value {
            let formatted = spec.format_for_config(&v);
            self.set_setting(spec.key, &formatted);
        }
    }

    /// Enter: for text settings, open the input overlay; otherwise nudge +1.
    pub fn activate_setting(&mut self) {
        let i = self.setting_selected;
        if i >= self.settings.len() {
            return;
        }
        let spec = self.settings[i].clone();
        if matches!(spec.widget, Widget::Text) {
            if self.config.count(spec.key) > 1 {
                self.toast_repeated_key(spec.key);
                return;
            }
            let (cur, is_default) = self.setting_value(i);
            self.input = Some(InputState {
                title: format!("{} — type a value, Enter to set", spec.label),
                // Defaults like "(system default)" are display labels, not
                // values — start empty so Enter can't write them verbatim.
                buffer: if is_default { String::new() } else { cur },
                purpose: InputPurpose::Setting(spec.key.to_string()),
            });
            self.mode = Mode::Input;
        } else {
            self.adjust_setting(1);
        }
    }

    fn set_setting(&mut self, key: &str, value: &str) {
        match self.config.set_single(key, value) {
            Ok(()) => self.dirty = true,
            Err(e) => self.toast(ToastKind::Error, format!("{e}")),
        }
    }

    pub fn save_settings(&mut self) {
        if !self.dirty {
            self.toast(ToastKind::Info, "No unsaved changes.");
            return;
        }
        match self.config.save() {
            Ok(_) => {
                self.dirty = false;
                self.toast(
                    ToastKind::Success,
                    "Saved.  Reload Ghostty with ⌘⇧, (cmd+shift+,)",
                );
            }
            Err(e) => self.toast(ToastKind::Error, format!("Save failed: {e:#}")),
        }
    }

    // ---- starship prompt -----------------------------------------------

    pub fn recompute_starship_filter(&mut self) {
        if self.starship_filter.is_empty() {
            self.starship_filtered = (0..self.starship_presets.len()).collect();
        } else {
            let q = self.starship_filter.to_lowercase();
            self.starship_filtered = self
                .starship_presets
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    p.name.to_lowercase().contains(&q) || p.description.to_lowercase().contains(&q)
                })
                .map(|(i, _)| i)
                .collect();
        }
        if self.starship_selected >= self.starship_filtered.len() {
            self.starship_selected = self.starship_filtered.len().saturating_sub(1);
        }
    }

    pub fn current_starship_preset(&self) -> Option<&hauntty::starship::StarshipPreset> {
        self.starship_filtered
            .get(self.starship_selected)
            .and_then(|&i| self.starship_presets.get(i))
    }

    pub fn move_starship(&mut self, delta: i32) {
        if self.starship_filtered.is_empty() {
            return;
        }
        let len = self.starship_filtered.len() as i32;
        let next = (self.starship_selected as i32 + delta).clamp(0, len - 1);
        self.starship_selected = next as usize;
    }

    pub fn apply_starship_preset(&mut self) {
        let Some(preset) = self.current_starship_preset().cloned() else {
            return;
        };
        let config_path = self.starship_status.config_path.clone();
        match hauntty::starship::apply_preset(&preset, &config_path) {
            Ok(outcome) => {
                self.starship_status.config_exists = true;
                let mut msg = format!(
                    "Applied Starship preset '{}' to {}",
                    preset.name,
                    outcome.config_path.display()
                );
                if let Some(bk) = outcome.backup_path {
                    msg = format!(
                        "Backed up to {}. {msg}",
                        bk.file_name().unwrap_or_default().to_string_lossy()
                    );
                }
                self.toast(ToastKind::Success, msg);
            }
            Err(e) => self.toast(ToastKind::Error, format!("Failed to apply preset: {e:#}")),
        }
    }

    pub fn install_starship(&mut self) {
        if self.starship_install_rx.is_some() {
            return; // Already installing.
        }
        self.toast(ToastKind::Info, "Installing Starship…");
        let (tx, rx) = std::sync::mpsc::channel();
        self.starship_install_rx = Some(rx);
        std::thread::spawn(move || {
            let res = hauntty::starship::install_starship().map_err(|e| format!("{e:#}"));
            let _ = tx.send(res);
        });
    }

    // ---- input overlay submit -----------------------------------------

    pub fn submit_input(&mut self) {
        let Some(input) = self.input.take() else {
            self.mode = Mode::Normal;
            return;
        };
        self.mode = Mode::Normal;
        match input.purpose {
            InputPurpose::Setting(key) => {
                // An empty buffer clears a set key (Ghostty then falls back
                // to its default); on an unset key it leaves the config
                // unchanged. Never write an empty value line, and never
                // delete a repeated-key stack (e.g. font-family fallbacks)
                // the editor was not showing.
                if input.buffer.trim().is_empty() {
                    match self.config.count(&key) {
                        0 => {}
                        1 => {
                            self.config.remove_all(&key);
                            self.dirty = true;
                            self.toast(
                                ToastKind::Success,
                                format!("Cleared {key} — Ghostty's default applies."),
                            );
                        }
                        _ => self.toast_repeated_key(&key),
                    }
                    return;
                }
                let spec = self.settings.iter().find(|s| s.key == key).cloned();
                if let Some(spec) = spec {
                    let formatted = spec.format_for_config(&input.buffer);
                    self.set_setting(&key, &formatted);
                }
            }
            #[cfg(feature = "import-iterm")]
            InputPurpose::ImportPath => self.do_import(&input.buffer),
        }
    }

    // ---- import --------------------------------------------------------

    #[cfg(feature = "import-iterm")]
    pub fn start_import(&mut self) {
        self.input = Some(InputState {
            title: "Import .itermcolors — paste a file path, Enter to import".to_string(),
            buffer: String::new(),
            purpose: InputPurpose::ImportPath,
        });
        self.mode = Mode::Input;
    }

    #[cfg(feature = "import-iterm")]
    fn do_import(&mut self, path: &str) {
        let path = shellexpand_tilde(path.trim());
        match hauntty::import::import_itermcolors(&path, &self.paths.user_theme_dir) {
            Ok(dest) => {
                let name = dest
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("theme")
                    .to_string();
                self.reload_themes();
                self.toast(ToastKind::Success, format!("Imported '{name}'."));
            }
            Err(e) => self.toast(ToastKind::Error, format!("Import failed: {e:#}")),
        }
    }

    // ---- online fetch --------------------------------------------------

    #[cfg(feature = "online")]
    pub fn start_fetch(&mut self) {
        if self.fetching {
            return;
        }
        self.mode = Mode::Fetch;
        self.fetch = None;
        self.fetching = true;
        self.spinner = 0;
        self.toast = None;

        let (tx, rx) = std::sync::mpsc::channel();
        self.fetch_rx = Some(rx);
        std::thread::spawn(move || {
            let res = fetch::list_remote_themes().map_err(|e| format!("{e:#}"));
            let _ = tx.send(FetchMsg::ListThemes(res));
        });
    }

    #[cfg(feature = "online")]
    pub fn start_starship_fetch(&mut self) {
        if self.fetching {
            return;
        }
        self.mode = Mode::Fetch;
        self.fetch = None;
        self.fetching = true;
        self.spinner = 0;
        self.toast = None;

        let (tx, rx) = std::sync::mpsc::channel();
        self.fetch_rx = Some(rx);
        std::thread::spawn(move || {
            let res = fetch::list_remote_starship_presets().map_err(|e| format!("{e:#}"));
            let _ = tx.send(FetchMsg::ListStarship(res));
        });
    }

    #[cfg(feature = "online")]
    pub fn fetch_move(&mut self, delta: i32) {
        if let Some(f) = &mut self.fetch {
            if f.filtered.is_empty() {
                return;
            }
            let len = f.filtered.len() as i32;
            f.selected = (f.selected as i32 + delta).clamp(0, len - 1) as usize;
        }
    }

    #[cfg(feature = "online")]
    pub fn fetch_filter_changed(&mut self) {
        if let Some(f) = &mut self.fetch {
            let q = f.filter.to_lowercase();
            match f.target {
                FetchTarget::Themes => {
                    f.filtered = f
                        .remotes
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| q.is_empty() || r.name.to_lowercase().contains(&q))
                        .map(|(i, _)| i)
                        .collect();
                }
                FetchTarget::Starship => {
                    f.filtered = f
                        .starship_remotes
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| q.is_empty() || r.name.to_lowercase().contains(&q))
                        .map(|(i, _)| i)
                        .collect();
                }
            }
            if f.selected >= f.filtered.len() {
                f.selected = f.filtered.len().saturating_sub(1);
            }
        }
    }

    #[cfg(feature = "online")]
    pub fn fetch_download_selected(&mut self) {
        if self.fetching {
            return;
        }
        let Some(f) = &self.fetch else { return };
        if f.filtered.is_empty() {
            return;
        }
        let idx = f.filtered[f.selected];

        self.fetching = true;
        self.spinner = 0;
        let (tx, rx) = std::sync::mpsc::channel();
        self.fetch_rx = Some(rx);

        match f.target {
            FetchTarget::Themes => {
                let remote = f.remotes[idx].clone();
                let dest_dir = self.paths.user_theme_dir.clone();
                std::thread::spawn(move || {
                    let res = fetch::download_theme(&remote, &dest_dir)
                        .map(|_| remote.name)
                        .map_err(|e| format!("{e:#}"));
                    let _ = tx.send(FetchMsg::DownloadTheme(res));
                });
            }
            FetchTarget::Starship => {
                let remote = f.starship_remotes[idx].clone();
                std::thread::spawn(move || {
                    let res = fetch::download_starship_preset_content(&remote)
                        .map(|content| (remote.name, content))
                        .map_err(|e| format!("{e:#}"));
                    let _ = tx.send(FetchMsg::DownloadStarship(res));
                });
            }
        }
    }
}

fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(feature = "import-iterm")]
fn shellexpand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RAII temp dir removed on drop, even during panic unwind.
    struct TempDir(std::path::PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn test_app(tag: &str, config: &str) -> (App, TempDir) {
        let path = std::env::temp_dir().join(format!("hauntty-app-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        let dir = TempDir(path);
        let cfg = dir.0.join("config");
        std::fs::write(&cfg, config).unwrap();
        let themes = dir.0.join("bundled");
        std::fs::create_dir_all(&themes).unwrap();
        let paths = Paths::resolve(Some(cfg), Some(themes));
        (App::new(paths).unwrap(), dir)
    }

    #[test]
    fn text_input_on_unset_setting_starts_empty() {
        let (mut app, _dir) = test_app("input-default", "font-size = 16\n");
        app.tab = Tab::Settings;
        app.setting_selected = 0; // font-family: Text widget, unset
        app.activate_setting();
        assert_eq!(app.input.as_ref().unwrap().buffer, "");
        // Enter on the empty buffer must not write "(system default)" (or
        // anything else) to the config.
        app.submit_input();
        assert!(!app.dirty);
        assert_eq!(app.config.count("font-family"), 0);
    }

    #[test]
    fn text_input_prefills_current_value() {
        let (mut app, _dir) = test_app("input-current", "font-family = Menlo\n");
        app.tab = Tab::Settings;
        app.setting_selected = 0;
        app.activate_setting();
        assert_eq!(app.input.as_ref().unwrap().buffer, "Menlo");
    }

    #[test]
    fn clearing_text_input_unsets_the_key() {
        let (mut app, _dir) = test_app("input-clear", "font-family = Menlo\n");
        app.tab = Tab::Settings;
        app.setting_selected = 0;
        app.activate_setting();
        app.input.as_mut().unwrap().buffer.clear();
        app.submit_input();
        assert_eq!(app.config.count("font-family"), 0);
        assert!(app.dirty);
        assert!(app.toast.is_some(), "clearing should confirm via toast");
    }

    const FALLBACK_STACK: &str = "font-family = Menlo\nfont-family = Symbols Nerd Font\n";

    #[test]
    fn repeated_key_shows_multiple_entries_not_default() {
        // A font-family fallback stack is set — the settings list must not
        // display it as the unset default.
        let (app, _dir) = test_app("repeated-display", FALLBACK_STACK);
        let (value, is_default) = app.setting_value(0);
        assert!(!is_default);
        assert_eq!(value, "(multiple entries)");
    }

    #[test]
    fn repeated_key_refuses_text_editor() {
        let (mut app, _dir) = test_app("repeated-edit", FALLBACK_STACK);
        app.tab = Tab::Settings;
        app.setting_selected = 0;
        app.activate_setting();
        assert!(
            app.input.is_none(),
            "editor must not open on a repeated key"
        );
        assert!(app.toast.is_some(), "refusal should explain via toast");
        assert_eq!(app.config.count("font-family"), 2);
        assert!(!app.dirty);
    }

    #[test]
    fn empty_submission_never_clears_a_repeated_key() {
        // Defense in depth: even if an input reaches submit for a repeated
        // key, Enter on an empty buffer must not delete the whole stack.
        let (mut app, _dir) = test_app("repeated-submit", FALLBACK_STACK);
        app.input = Some(InputState {
            title: String::new(),
            buffer: String::new(),
            purpose: InputPurpose::Setting("font-family".to_string()),
        });
        app.mode = Mode::Input;
        app.submit_input();
        assert_eq!(app.config.count("font-family"), 2);
        assert!(!app.dirty);
    }

    #[test]
    fn backup_base_resolves_last_theme_line() {
        // Ghostty's last `theme =` line wins; the backup must compose from it
        // even when an earlier (stale) theme line exists.
        let path = std::env::temp_dir().join(format!("hauntty-app-base-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        let dir = TempDir(path);
        let cfg = dir.0.join("config");
        std::fs::write(&cfg, "theme = Stale\ntheme = Base\nbackground = 000000\n").unwrap();
        let themes = dir.0.join("bundled");
        std::fs::create_dir_all(&themes).unwrap();
        std::fs::write(themes.join("Base"), "foreground = #f8f8f2\n").unwrap();
        let mut app = App::new(Paths::resolve(Some(cfg), Some(themes))).unwrap();

        app.confirm = Some(ConfirmState {
            theme_name: "Base".to_string(),
            will_backup: true,
            backup_name: "Combo".to_string(),
            editing_name: false,
        });
        app.mode = Mode::Confirm;
        app.confirm_apply();

        let backup = std::fs::read_to_string(app.paths.user_theme_dir.join("Combo")).unwrap();
        assert!(backup.contains("background = #000000")); // inline override wins
        assert!(backup.contains("foreground = #f8f8f2")); // from the last theme line's base
    }

    #[cfg(feature = "online")]
    fn deliver_download(app: &mut App, name: &str, content: &str) {
        let (tx, rx) = std::sync::mpsc::channel();
        app.fetch_rx = Some(rx);
        app.fetching = true;
        tx.send(FetchMsg::DownloadStarship(Ok((
            name.to_string(),
            content.to_string(),
        ))))
        .unwrap();
        app.poll_background();
    }

    #[cfg(feature = "online")]
    #[test]
    fn downloaded_starship_preset_selected_despite_filter() {
        let (mut app, _dir) = test_app("dl-filter", "");
        app.starship_filter = "tokyo".to_string();
        app.recompute_starship_filter();
        deliver_download(&mut app, "Remote Pastel", "format = \"$all\"");
        // The filter is cleared and the new preset is the live selection, so
        // "Press Enter to apply" actually applies it.
        assert!(app.starship_filter.is_empty());
        let p = app
            .current_starship_preset()
            .expect("downloaded preset is selected");
        assert_eq!(p.name, "Remote Pastel");
        assert_eq!(p.toml_content, "format = \"$all\"");
    }

    #[cfg(feature = "online")]
    #[test]
    fn redownloading_preset_replaces_instead_of_duplicating() {
        let (mut app, _dir) = test_app("dl-dup", "");
        let baseline = app.starship_presets.len();
        deliver_download(&mut app, "Remote Pastel", "format = \"v1\"");
        deliver_download(&mut app, "Remote Pastel", "format = \"v2\"");
        assert_eq!(app.starship_presets.len(), baseline + 1);
        assert_eq!(
            app.current_starship_preset().unwrap().toml_content,
            "format = \"v2\""
        );
    }
}
