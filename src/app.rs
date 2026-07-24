//! Application state and logic for the hauntty TUI.

use anyhow::Result;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use hauntty::apply;
use hauntty::config::ConfigDocument;
use hauntty::paths::Paths;
use hauntty::settings::{SettingSpec, Widget};
use hauntty::theme::{Theme, ThemeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Themes,
    Settings,
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
    FontFamily,
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
pub struct FetchState {
    pub remotes: Vec<hauntty::fetch::RemoteTheme>,
    pub filter: String,
    pub filtered: Vec<usize>,
    pub selected: usize,
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

    // Overlays
    pub toast: Option<Toast>,
    pub confirm: Option<ConfirmState>,
    pub input: Option<InputState>,
    #[cfg(feature = "online")]
    pub fetch: Option<FetchState>,

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
            toast: None,
            confirm: None,
            input: None,
            #[cfg(feature = "online")]
            fetch: None,
            matcher: Matcher::new(Config::DEFAULT),
        };
        app.recompute_filter();
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
        let backup = if confirm.will_backup {
            Some(confirm.backup_name.as_str())
        } else {
            None
        };
        match apply::apply_theme(
            &mut self.config,
            &confirm.theme_name,
            backup,
            &self.paths.user_theme_dir,
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
        }
        self.mode = Mode::Normal;
    }

    // ---- settings ------------------------------------------------------

    /// The displayed value of a setting and whether it is the (unset) default.
    pub fn setting_value(&self, i: usize) -> (String, bool) {
        let spec = &self.settings[i];
        match self.config.get_single(spec.key) {
            Some(v) => (strip_quotes(v), false),
            None => (spec.default.to_string(), true),
        }
    }

    /// Adjust the selected setting by `dir` (+1/-1). No-op for text widgets.
    pub fn adjust_setting(&mut self, dir: i32) {
        let i = self.setting_selected;
        let spec = self.settings[i].clone();
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
        let spec = self.settings[i].clone();
        if matches!(spec.widget, Widget::Text) {
            let (cur, _) = self.setting_value(i);
            self.input = Some(InputState {
                title: format!("{} — type a value, Enter to set", spec.label),
                buffer: cur,
                purpose: InputPurpose::FontFamily,
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

    // ---- input overlay submit -----------------------------------------

    pub fn submit_input(&mut self) {
        let Some(input) = self.input.take() else {
            self.mode = Mode::Normal;
            return;
        };
        self.mode = Mode::Normal;
        match input.purpose {
            InputPurpose::FontFamily => {
                let spec = self
                    .settings
                    .iter()
                    .find(|s| s.key == "font-family")
                    .cloned();
                if let Some(spec) = spec {
                    let formatted = spec.format_for_config(&input.buffer);
                    self.set_setting("font-family", &formatted);
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
        self.toast(ToastKind::Info, "Fetching theme list from GitHub…");
        match hauntty::fetch::list_remote_themes() {
            Ok(remotes) => {
                let filtered = (0..remotes.len()).collect();
                self.fetch = Some(FetchState {
                    remotes,
                    filter: String::new(),
                    filtered,
                    selected: 0,
                });
                self.mode = Mode::Fetch;
                self.toast = None;
            }
            Err(e) => self.toast(ToastKind::Error, format!("Fetch failed: {e:#}")),
        }
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
            f.filtered = f
                .remotes
                .iter()
                .enumerate()
                .filter(|(_, r)| q.is_empty() || r.name.to_lowercase().contains(&q))
                .map(|(i, _)| i)
                .collect();
            if f.selected >= f.filtered.len() {
                f.selected = f.filtered.len().saturating_sub(1);
            }
        }
    }

    #[cfg(feature = "online")]
    pub fn fetch_download_selected(&mut self) {
        let Some(f) = &self.fetch else { return };
        let Some(&idx) = f.filtered.get(f.selected) else {
            return;
        };
        let remote = f.remotes[idx].clone();
        match hauntty::fetch::download_theme(&remote, &self.paths.user_theme_dir) {
            Ok(_) => {
                self.reload_themes();
                self.toast(ToastKind::Success, format!("Downloaded '{}'.", remote.name));
            }
            Err(e) => self.toast(ToastKind::Error, format!("Download failed: {e:#}")),
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
