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
    Tools,
}

/// The current interaction mode (drives which overlay/handler is active).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Filter,
    Confirm,
    Input,
    Customize,
    ToolInstall,
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
    CustomColor(usize),
    CustomName,
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
    pub customize: Option<crate::customize::Draft>,
    pub tools: crate::tool_setup::ToolsState,
    #[cfg(feature = "import-iterm")]
    pub import_browser: Option<crate::import_path::Browser>,
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
            customize: None,
            tools: crate::tool_setup::ToolsState::new(),
            #[cfg(feature = "import-iterm")]
            import_browser: None,
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
        let base_theme_name = self
            .config
            .indices_of("theme")
            .last()
            .and_then(|&i| match &self.config.lines[i] {
                hauntty::config::Line::KeyValue(kv) => Some(strip_quotes(&kv.value)),
                _ => None,
            })
            .filter(|name| !name.is_empty());
        let base_theme = match base_theme_name {
            Some(name) => match self.themes.get(&name).cloned() {
                Some(t) => Some(t),
                // A base we can't resolve (Ghostty's conditional dark/light
                // syntax, or a theme hauntty can't find) means the backup
                // would silently miss the base theme's colors — refuse
                // rather than claim the look was saved.
                None if confirm.will_backup => {
                    let why = if name.contains(':') {
                        format!(
                            "`theme = {name}` is conditional (dark/light), \
                             so there is no single look to save"
                        )
                    } else {
                        format!("base theme '{name}' was not found, so the backup would miss its colors")
                    };
                    self.toast(
                        ToastKind::Error,
                        format!("Can't back up your current colors: {why} — config not changed"),
                    );
                    return;
                }
                None => None,
            },
            None => None,
        };
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
        if self.customize.is_some() && self.mode == Mode::Input {
            self.input = None;
            self.mode = Mode::Customize;
            return;
        }
        self.confirm = None;
        self.input = None;
        #[cfg(feature = "import-iterm")]
        {
            self.import_browser = None;
        }
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
                // Placeholder defaults like "(system default)" are display
                // labels, not values — start empty so Enter can't write them
                // verbatim. Real-valued defaults (shell-integration-features'
                // "cursor,sudo,title") prefill so they can be append-edited.
                buffer: if is_default && spec.default_is_label() {
                    String::new()
                } else {
                    cur
                },
                purpose: InputPurpose::Setting(spec.key.to_string()),
            });
            self.mode = Mode::Input;
        } else {
            self.adjust_setting(1);
        }
    }

    fn set_setting(&mut self, key: &str, value: &str) {
        match self.config.set_single(key, value) {
            Ok(()) => {
                self.dirty = true;
                // Ghostty's shell integration forces a bar cursor at the
                // prompt, so a cursor-style change silently "doesn't work"
                // there — point the user at the documented fix.
                if key == "cursor-style" && self.shell_integration_overrides_cursor() {
                    self.toast(
                        ToastKind::Info,
                        "Shell integration overrides the cursor at the prompt — \
                         set Shell integration features to no-cursor to make this stick.",
                    );
                }
            }
            Err(e) => self.toast(ToastKind::Error, format!("{e}")),
        }
    }

    /// True when Ghostty's shell integration would override `cursor-style`
    /// at the prompt: integration is on (default) and its `cursor` feature
    /// has not been disabled via `no-cursor`.
    fn shell_integration_overrides_cursor(&self) -> bool {
        if self.config.get_single("shell-integration") == Some("none") {
            return false;
        }
        !self
            .config
            .get_single("shell-integration-features")
            .is_some_and(|v| v.contains("no-cursor"))
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

    pub fn starship_install_running(&self) -> bool {
        self.starship_install_rx.is_some()
    }

    // ---- input overlay submit -----------------------------------------

    pub fn submit_input(&mut self) {
        let Some(input) = self.input.take() else {
            self.mode = Mode::Normal;
            return;
        };
        self.mode = Mode::Normal;
        if matches!(
            input.purpose,
            InputPurpose::CustomColor(_) | InputPurpose::CustomName
        ) {
            self.submit_custom_input(input);
            return;
        }
        match input.purpose {
            InputPurpose::CustomColor(_) | InputPurpose::CustomName => unreachable!(),
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
            InputPurpose::ImportPath => {
                if !self.do_import(&input.buffer) {
                    self.input = Some(input);
                    self.mode = Mode::Input;
                }
            }
        }
    }

    // ---- import --------------------------------------------------------

    #[cfg(feature = "import-iterm")]
    pub fn start_import(&mut self) {
        self.input = Some(InputState {
            title: "Drop a .itermcolors file here, or paste/type its path".to_string(),
            buffer: String::new(),
            purpose: InputPurpose::ImportPath,
        });
        self.mode = Mode::Input;
        self.import_browser = None;
    }

    #[cfg(feature = "import-iterm")]
    fn do_import(&mut self, path: &str) -> bool {
        let result = crate::import_path::resolve(path).and_then(|path| {
            hauntty::import::import_itermcolors(&path, &self.paths.user_theme_dir)
        });
        match result {
            Ok(dest) => {
                let name = dest
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("theme")
                    .to_string();
                // A stale search can hide the imported file. Reveal it and
                // select its position in the refreshed display list.
                self.filter.clear();
                self.reload_themes();
                self.theme_selected = self
                    .filtered
                    .iter()
                    .position(|&i| self.themes.ordered[i].path == dest)
                    .unwrap_or(0);
                self.tab = Tab::Themes;
                self.toast(
                    ToastKind::Success,
                    format!("Imported '{name}'. Press Enter to apply."),
                );
                true
            }
            Err(e) => {
                self.toast(ToastKind::Error, format!("Import failed: {e:#}"));
                false
            }
        }
    }

    #[cfg(feature = "import-iterm")]
    pub fn toggle_import_browser(&mut self) {
        if self.import_browser.take().is_some() {
            return;
        }
        let path = self
            .input
            .as_ref()
            .and_then(|i| crate::import_path::resolve(&i.buffer).ok());
        let directory = path
            .and_then(|p| {
                if p.is_dir() {
                    Some(p)
                } else {
                    p.parent().filter(|p| p.is_dir()).map(|p| p.to_path_buf())
                }
            })
            .or_else(|| dirs::download_dir().filter(|p| p.is_dir()))
            .or_else(|| dirs::home_dir().filter(|p| p.is_dir()))
            .or_else(|| std::env::current_dir().ok());
        if let Some(directory) = directory {
            self.open_import_directory(&directory);
        } else {
            self.toast(ToastKind::Error, "Could not find a directory to browse.");
        }
    }

    #[cfg(feature = "import-iterm")]
    fn open_import_directory(&mut self, path: &std::path::Path) {
        match crate::import_path::Browser::open(path) {
            Ok(browser) => self.import_browser = Some(browser),
            Err(e) => self.toast(ToastKind::Error, format!("Cannot browse: {e:#}")),
        }
    }

    #[cfg(feature = "import-iterm")]
    pub fn import_parent(&mut self) {
        let parent = self
            .import_browser
            .as_ref()
            .and_then(|b| b.directory.parent())
            .map(|p| p.to_path_buf());
        if let Some(parent) = parent {
            self.open_import_directory(&parent);
        }
    }

    #[cfg(feature = "import-iterm")]
    pub fn choose_import_entry(&mut self) {
        let entry = self
            .import_browser
            .as_ref()
            .and_then(|b| b.entries.get(b.selected))
            .map(|e| (e.path.clone(), e.is_dir));
        if let Some((path, is_dir)) = entry {
            if is_dir {
                self.open_import_directory(&path);
            } else if let Some(path) = path.to_str() {
                if let Some(input) = &mut self.input {
                    input.buffer = path.to_string();
                }
                self.import_browser = None;
            } else {
                self.toast(
                    ToastKind::Error,
                    "This filename cannot be displayed as UTF-8.",
                );
            }
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
        let git_ref = self
            .starship_status
            .version
            .as_deref()
            .and_then(fetch::starship_ref_from_version);
        std::thread::spawn(move || {
            let res = fetch::list_remote_starship_presets(git_ref.as_deref())
                .map_err(|e| format!("{e:#}"));
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

    #[cfg(feature = "import-iterm")]
    const IMPORT_SAMPLE: &str = r#"<?xml version="1.0"?><plist version="1.0"><dict>
      <key>Background Color</key><dict>
        <key>Red Component</key><real>0.1</real>
        <key>Green Component</key><real>0.2</real>
        <key>Blue Component</key><real>0.3</real>
      </dict><key>Ansi 0 Color</key><dict>
        <key>Red Component</key><real>0.0</real>
        <key>Green Component</key><real>0.0</real>
        <key>Blue Component</key><real>0.0</real>
      </dict></dict></plist>"#;

    #[cfg(feature = "import-iterm")]
    #[test]
    fn dropped_paths_import_only_on_enter_and_preserve_config() {
        use crate::event::handle_event;
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
        let config = "# Leave me alone\nfont-size = 16\n";
        let (mut app, dir) = test_app("import-drop", config);
        let source = dir.0.join("Ocean's 蓝 Theme.itermcolors");
        std::fs::write(&source, IMPORT_SAMPLE).unwrap();
        let path = source.to_str().unwrap();
        let escaped = path.replace(' ', "\\ ").replace('\'', "\\'");
        for text in [path.to_string(), format!("\"{path}\""), escaped] {
            app.start_import();
            handle_event(&mut app, Event::Paste(format!("{text}\r\n")));
            assert_eq!(app.mode, Mode::Input);
            assert_eq!(app.input.as_ref().unwrap().buffer, text);
            handle_event(
                &mut app,
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            );
            assert_eq!(app.mode, Mode::Normal, "{:?}", app.toast);
            assert_eq!(app.toast.as_ref().unwrap().kind, ToastKind::Success);
            assert!(app.paths.user_theme_dir.join("Ocean's 蓝 Theme").is_file());
        }
        assert_eq!(std::fs::read_to_string(&app.paths.config).unwrap(), config);
        assert_eq!(std::fs::read_to_string(source).unwrap(), IMPORT_SAMPLE);
    }

    #[cfg(feature = "import-iterm")]
    #[test]
    fn import_reveals_and_selects_theme_in_refreshed_list() {
        let config = "# Keep the current theme until explicitly applied\ntheme = Existing\n";
        let (mut app, dir) = test_app("import-selection", config);
        let bundled = dir.0.join("bundled");
        // Keep this test independent of themes installed on the host.
        app.paths.bundled_theme_dirs = vec![bundled.clone()];
        for name in ["Existing", "Zebra"] {
            std::fs::write(
                bundled.join(name),
                "background = #ffffff\npalette = 0=#ffffff\n",
            )
            .unwrap();
        }
        let source = dir.0.join("Zebra.itermcolors");
        std::fs::write(&source, IMPORT_SAMPLE).unwrap();
        for filter in ["", "Zebra", "Existing"] {
            app.filter = filter.to_string();
            app.reload_themes();
            app.theme_selected = 0;
            app.start_import();
            app.input.as_mut().unwrap().buffer = source.to_string_lossy().into_owned();
            app.submit_input();

            assert_eq!(app.mode, Mode::Normal);
            assert_eq!(app.tab, Tab::Themes);
            assert!(app.filter.is_empty());
            assert_eq!(app.theme_selected, 1);
            let selected = app.current_theme().expect("import is selected immediately");
            assert_eq!(selected.name, "Zebra");
            assert_eq!(selected.source, hauntty::theme::ThemeSource::User);
            assert_eq!(selected.path, app.paths.user_theme_dir.join("Zebra"));
            assert_eq!(
                selected.background,
                Some(hauntty::theme::Rgb::new(26, 51, 77))
            );
            assert_eq!(app.themes.len(), 2, "reimport replaces the same theme");
            assert_eq!(std::fs::read_to_string(&app.paths.config).unwrap(), config);
        }
    }

    #[cfg(feature = "import-iterm")]
    #[test]
    fn import_failure_keeps_path_for_correction() {
        use crate::event::handle_event;
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
        let (mut app, dir) = test_app("import-retry", "");
        app.start_import();
        let missing = format!("'{}'", dir.0.join("missing.itermcolors").display());
        handle_event(&mut app, Event::Paste(missing.clone()));
        app.submit_input();
        assert_eq!(app.mode, Mode::Input);
        assert_eq!(app.input.as_ref().unwrap().buffer, missing);
        assert_eq!(app.toast.as_ref().unwrap().kind, ToastKind::Error);
        handle_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
        );
        assert!(app.input.as_ref().unwrap().buffer.is_empty());
        let source = dir.0.join("Fixed.itermcolors");
        std::fs::write(&source, IMPORT_SAMPLE).unwrap();
        handle_event(&mut app, Event::Paste(format!("'{}'", source.display())));
        app.submit_input();
        assert_eq!(app.toast.as_ref().unwrap().kind, ToastKind::Success);
    }

    #[cfg(feature = "import-iterm")]
    #[test]
    fn optional_browser_selects_file_without_importing() {
        use crate::event::handle_event;
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
        let (mut app, dir) = test_app("import-browser", "");
        let folder = dir.0.join("Downloads");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("Ocean.itermcolors"), IMPORT_SAMPLE).unwrap();
        std::fs::write(folder.join("ignore.txt"), "ignored").unwrap();
        app.start_import();
        app.input.as_mut().unwrap().buffer = folder.to_string_lossy().into_owned();
        handle_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        );
        let browser = app.import_browser.as_ref().unwrap();
        assert_eq!(browser.entries.len(), 2); // parent and theme
        assert!(browser.entries[0].is_dir);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| crate::ui::render(f, &app)).unwrap();
        handle_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
        );
        handle_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        );
        assert!(app.import_browser.is_none());
        assert_eq!(app.mode, Mode::Input);
        assert!(!app.paths.user_theme_dir.join("Ocean").exists());
        terminal.draw(|f| crate::ui::render(f, &app)).unwrap();
        app.submit_input();
        assert!(app.paths.user_theme_dir.join("Ocean").is_file());

        app.start_import();
        app.input.as_mut().unwrap().buffer = folder.to_string_lossy().into_owned();
        app.toggle_import_browser();
        app.import_parent();
        assert_eq!(
            app.import_browser.as_ref().unwrap().directory,
            std::fs::canonicalize(&dir.0).unwrap()
        );
        app.cancel_overlay();
        assert!(app.import_browser.is_none());
        assert!(app.input.is_none());
    }

    #[test]
    fn paste_is_text_and_never_a_shortcut_or_submission() {
        use crate::event::handle_event;
        use crossterm::event::Event;
        let (mut app, _dir) = test_app("paste-text", "font-family = Menlo\n");
        handle_event(&mut app, Event::Paste("qys".into()));
        assert!(!app.should_quit);
        assert!(!app.dirty);
        app.tab = Tab::Settings;
        app.activate_setting();
        handle_event(&mut app, Event::Paste(" Mono\n".into()));
        assert_eq!(app.input.as_ref().unwrap().buffer, "Menlo Mono");
        assert_eq!(app.mode, Mode::Input);
        handle_event(&mut app, Event::Paste("first\nsecond".into()));
        assert_eq!(app.input.as_ref().unwrap().buffer, "Menlo Mono");
        app.cancel_overlay();
        app.mode = Mode::Filter;
        app.tab = Tab::Themes;
        handle_event(&mut app, Event::Paste("Ocean".into()));
        assert_eq!(app.filter, "Ocean");
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
    fn text_input_prefills_real_valued_default() {
        // shell-integration-features' default "cursor,sudo,title" is a real
        // value, not a placeholder label — it must prefill even when unset so
        // the user can append (e.g. ",clipboard") without losing the three
        // default features.
        let (mut app, _dir) = test_app("input-real-default", "font-size = 16\n");
        app.tab = Tab::Settings;
        app.setting_selected = app
            .settings
            .iter()
            .position(|s| s.key == "shell-integration-features")
            .unwrap();
        app.activate_setting();
        assert_eq!(app.input.as_ref().unwrap().buffer, "cursor,sudo,title");
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

    fn confirm(theme_name: &str, will_backup: bool) -> ConfirmState {
        ConfirmState {
            theme_name: theme_name.to_string(),
            will_backup,
            backup_name: "My Saved Theme".to_string(),
            editing_name: false,
        }
    }

    #[test]
    fn apply_refuses_backup_when_base_theme_is_conditional() {
        // Ghostty's conditional syntax names two themes; there is no single
        // effective look to save, so an apply that promises a backup must
        // refuse instead of writing one that misses the base palette.
        let (mut app, _dir) = test_app(
            "conditional-base",
            "theme = dark:Foo,light:Bar\nbackground = #111111\n",
        );
        app.confirm = Some(confirm("Whatever", true));
        app.confirm_apply();
        assert!(matches!(&app.toast, Some(t) if t.kind == ToastKind::Error));
        // Config untouched: conditional theme line and override both intact.
        assert_eq!(app.config.get_single("theme"), Some("dark:Foo,light:Bar"));
        assert_eq!(app.config.count("background"), 1);
        assert!(!app.paths.user_theme_dir.join("My Saved Theme").exists());
    }

    #[test]
    fn apply_refuses_backup_when_base_theme_is_missing() {
        // Same guard for a plain theme name hauntty cannot find: the backup
        // would silently lack the base theme's colors.
        let (mut app, _dir) = test_app(
            "missing-base",
            "theme = NoSuchTheme\nbackground = #111111\n",
        );
        app.confirm = Some(confirm("Whatever", true));
        app.confirm_apply();
        assert!(matches!(&app.toast, Some(t) if t.kind == ToastKind::Error));
        assert_eq!(app.config.get_single("theme"), Some("NoSuchTheme"));
        assert!(!app.paths.user_theme_dir.join("My Saved Theme").exists());
    }

    #[test]
    fn apply_replaces_unresolved_theme_when_no_backup_needed() {
        // Without inline colors there is nothing to back up — replacing a
        // conditional theme line is an ordinary apply.
        let (mut app, _dir) = test_app("conditional-nobackup", "theme = dark:Foo,light:Bar\n");
        app.confirm = Some(confirm("New Theme", false));
        app.confirm_apply();
        assert_eq!(app.config.get_single("theme"), Some("New Theme"));
        assert!(matches!(&app.toast, Some(t) if t.kind == ToastKind::Success));
    }

    fn select_setting(app: &mut App, key: &str) {
        app.tab = Tab::Settings;
        app.setting_selected = app.settings.iter().position(|s| s.key == key).unwrap();
    }

    #[test]
    fn cursor_style_change_hints_about_shell_integration_override() {
        // Ghostty's shell integration forces a bar cursor at the prompt, so a
        // cursor-style change looks like it "doesn't work". Changing it while
        // that override is active must point the user at no-cursor.
        let (mut app, _dir) = test_app("cursor-hint", "cursor-style = block\n");
        select_setting(&mut app, "cursor-style");
        app.adjust_setting(1);
        assert_eq!(app.config.get_single("cursor-style"), Some("bar"));
        assert!(app.dirty);
        let toast = app.toast.as_ref().expect("expected a hint toast");
        assert_eq!(toast.kind, ToastKind::Info);
        assert!(toast.text.contains("no-cursor"));
    }

    #[test]
    fn no_cursor_hint_when_cursor_feature_disabled() {
        let (mut app, _dir) = test_app(
            "cursor-nohint-feature",
            "cursor-style = block\nshell-integration-features = no-cursor\n",
        );
        select_setting(&mut app, "cursor-style");
        app.adjust_setting(1);
        assert_eq!(app.config.get_single("cursor-style"), Some("bar"));
        assert!(app.toast.is_none());
    }

    #[test]
    fn no_cursor_hint_when_shell_integration_off() {
        let (mut app, _dir) = test_app(
            "cursor-nohint-off",
            "cursor-style = block\nshell-integration = none\n",
        );
        select_setting(&mut app, "cursor-style");
        app.adjust_setting(1);
        assert!(app.toast.is_none());
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
