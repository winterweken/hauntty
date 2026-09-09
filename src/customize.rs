//! A lossless theme draft, saved as a new user theme before it can be applied.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use hauntty::config::{ConfigDocument, KeyValue, Line};
use hauntty::theme::{Rgb, Theme, ThemeSource};

use crate::app::{App, InputPurpose, InputState, Mode, ToastKind};

pub const COLOR_KEYS: [&str; 6] = [
    "background",
    "foreground",
    "cursor-color",
    "cursor-text",
    "selection-background",
    "selection-foreground",
];
pub const COLOR_COUNT: usize = COLOR_KEYS.len() + 16;

pub fn label(index: usize) -> String {
    if index < COLOR_KEYS.len() {
        COLOR_KEYS[index].to_string()
    } else {
        format!("palette {}", index - COLOR_KEYS.len())
    }
}

fn matches_slot(kv: &KeyValue, index: usize) -> bool {
    if index < COLOR_KEYS.len() {
        kv.key == COLOR_KEYS[index]
    } else {
        kv.key == "palette"
            && kv
                .value
                .split_once('=')
                .and_then(|(n, _)| n.trim().parse::<usize>().ok())
                == Some(index - COLOR_KEYS.len())
    }
}

pub struct Draft {
    pub name: String,
    pub selected: usize,
    pub discard_armed: bool,
    source: ConfigDocument,
    permissions: fs::Permissions,
    edits: BTreeMap<usize, String>,
}

impl Draft {
    pub fn new(theme: &Theme, config: &ConfigDocument) -> Result<Self> {
        let content = fs::read_to_string(&theme.path).context("reading the selected theme")?;
        let mut source = ConfigDocument::parse(&theme.path, &content);
        let active = config.lines.iter().rev().find_map(|line| match line {
            Line::KeyValue(kv) if kv.key == "theme" => Some(kv.value.trim().trim_matches('"')),
            _ => None,
        });
        if active == Some(theme.name.as_str()) {
            source.lines.extend(config.lines.iter().filter(|line| {
                matches!(line, Line::KeyValue(kv) if hauntty::apply::INLINE_COLOR_KEYS.contains(&kv.key.as_str()))
            }).cloned());
        }
        Ok(Self {
            name: format!("{} Custom", theme.name),
            selected: 0,
            discard_armed: false,
            source,
            permissions: fs::metadata(&theme.path)?.permissions(),
            edits: BTreeMap::new(),
        })
    }

    pub fn dirty(&self) -> bool {
        !self.edits.is_empty()
    }

    pub fn document(&self) -> ConfigDocument {
        let mut doc = self.source.clone();
        for (&index, color) in &self.edits {
            let (key, value) = if index < COLOR_KEYS.len() {
                (COLOR_KEYS[index], color.clone())
            } else {
                ("palette", format!("{}={color}", index - COLOR_KEYS.len()))
            };
            if let Some(Line::KeyValue(kv)) = doc
                .lines
                .iter_mut()
                .rev()
                .find(|line| matches!(line, Line::KeyValue(kv) if matches_slot(kv, index)))
            {
                kv.value = value;
                kv.edited = true;
            } else {
                doc.lines.push(Line::KeyValue(KeyValue::new(key, &value)));
            }
        }
        doc
    }

    pub fn value(&self, index: usize) -> String {
        self.document()
            .lines
            .iter()
            .rev()
            .find_map(|line| match line {
                Line::KeyValue(kv) if matches_slot(kv, index) => {
                    Some(if index < COLOR_KEYS.len() {
                        kv.value.clone()
                    } else {
                        kv.value.split_once('=').unwrap().1.trim().to_string()
                    })
                }
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn preview(&self) -> Theme {
        Theme::from_str(
            &self.name,
            ThemeSource::User,
            self.source.path.clone(),
            &self.document().render(),
        )
    }

    pub fn set_color(&mut self, index: usize, input: &str) -> Result<()> {
        let input = input.trim();
        let hex = input.strip_prefix('#').unwrap_or(input);
        if !matches!(hex.len(), 3 | 6) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("Use a hex color such as #282a36 or #abc.");
        }
        let color = Rgb::parse_hex(input).context("Invalid hex color")?;
        self.edits.insert(index, color.to_hex());
        Ok(())
    }

    /// Publish a complete file atomically without replacing any existing file
    /// or symlink, including one created while the draft was being edited.
    pub fn save(&self, directory: &Path, name: &str) -> Result<()> {
        if name.is_empty()
            || name.starts_with('.')
            || name.contains(['/', '\\', ':', '"', '\''])
            || name.chars().any(char::is_control)
        {
            bail!("Choose a theme name without path separators, quotes, colons, or a leading dot.");
        }
        fs::create_dir_all(directory)?;
        let dest = directory.join(name);
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let tmp = directory.join(format!(".hauntty.custom.{}.{stamp}", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        let result = (|| -> Result<()> {
            file.set_permissions(self.permissions.clone())?;
            file.write_all(self.document().render().as_bytes())?;
            file.sync_all()?;
            fs::hard_link(&tmp, &dest).context("creating custom theme (choose an unused name)")?;
            Ok(())
        })();
        let _ = fs::remove_file(&tmp);
        result
    }
}

impl App {
    pub fn start_customize(&mut self) {
        let Some(theme) = self.current_theme() else {
            return;
        };
        match Draft::new(theme, &self.config) {
            Ok(draft) => {
                self.customize = Some(draft);
                self.mode = Mode::Customize;
            }
            Err(e) => self.toast(ToastKind::Error, format!("Cannot customize: {e:#}")),
        }
    }

    pub fn edit_custom_color(&mut self) {
        let Some(draft) = &self.customize else { return };
        self.input = Some(InputState {
            title: format!("{} — enter #RRGGBB or #RGB", label(draft.selected)),
            buffer: draft.value(draft.selected),
            purpose: InputPurpose::CustomColor(draft.selected),
        });
        self.mode = Mode::Input;
    }

    pub fn name_custom_theme(&mut self) {
        let Some(draft) = &self.customize else { return };
        self.input = Some(InputState {
            title: "Save custom theme as — choose a new name".into(),
            buffer: draft.name.clone(),
            purpose: InputPurpose::CustomName,
        });
        self.mode = Mode::Input;
    }

    pub fn submit_custom_input(&mut self, input: InputState) {
        self.mode = Mode::Customize;
        let result = match input.purpose {
            InputPurpose::CustomColor(index) => self
                .customize
                .as_mut()
                .unwrap()
                .set_color(index, &input.buffer),
            InputPurpose::CustomName => {
                let name = input.buffer.trim();
                if self.themes.get(name).is_some() {
                    Err(anyhow::anyhow!(
                        "A theme with that name already exists. Choose a new name."
                    ))
                } else {
                    self.customize
                        .as_ref()
                        .unwrap()
                        .save(&self.paths.user_theme_dir, name)
                }
            }
            _ => unreachable!(),
        };
        if let Err(e) = result {
            self.input = Some(input);
            self.mode = Mode::Input;
            self.toast(ToastKind::Error, format!("{e:#}"));
        } else if input.purpose == InputPurpose::CustomName {
            self.customize = None;
            self.mode = Mode::Normal;
            self.filter.clear();
            self.reload_themes();
            self.theme_selected = self
                .filtered
                .iter()
                .position(|&i| self.themes.ordered[i].name == input.buffer.trim())
                .unwrap_or(0);
            self.toast(
                ToastKind::Success,
                "Custom theme saved. Press Enter to apply, or c to customize further.",
            );
        }
    }

    pub fn reset_custom_color(&mut self) {
        if let Some(draft) = &mut self.customize {
            draft.edits.remove(&draft.selected);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use hauntty::paths::Paths;

    struct Fixture {
        app: App,
        dir: std::path::PathBuf,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
    fn fixture(tag: &str, source: &str, config: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("hauntty-custom-{tag}-{}", std::process::id()));
        fs::create_dir_all(dir.join("bundled")).unwrap();
        fs::write(dir.join("bundled/Base"), source).unwrap();
        fs::write(dir.join("config"), config).unwrap();
        let paths = Paths {
            config: dir.join("config"),
            bundled_theme_dirs: vec![dir.join("bundled")],
            user_theme_dir: dir.join("themes"),
        };
        Fixture {
            app: App::new(paths).unwrap(),
            dir,
        }
    }
    fn key(app: &mut App, code: KeyCode) {
        crate::event::handle_event(app, Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }
    fn submit(app: &mut App, text: &str) {
        crate::event::handle_event(
            app,
            Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
        );
        crate::event::handle_event(app, Event::Paste(text.into()));
        key(app, KeyCode::Enter);
    }

    #[test]
    fn edit_save_and_apply_preserves_source_and_config_until_confirmation() {
        let source = "# source\r\nbackground   = #111111\r\nbackground   = #222222\r\npalette = 1=red\r\npalette = 200=#112233\r\nselection-foreground = cell-foreground\r\nfont-family = One\r\nfont-family = Two\r\n";
        let config = "# keep\r\ntheme = Base\r\nfont-size = 16\r\n";
        let mut f = fixture("save", source, config);
        let app = &mut f.app;
        key(app, KeyCode::Char('c'));
        assert_eq!(app.mode, Mode::Customize);
        key(app, KeyCode::Enter);
        submit(app, "#abc");
        assert_eq!(app.mode, Mode::Customize);
        assert_eq!(
            app.customize.as_ref().unwrap().preview().background,
            Some(Rgb::new(170, 187, 204))
        );
        for (width, height) in [(120, 32), (80, 24), (30, 10), (1, 1)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| crate::ui::render(frame, app))
                .unwrap();
        }
        key(app, KeyCode::Char('s'));
        submit(app, "Personal");
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.current_theme().unwrap().name, "Personal");
        assert_eq!(
            fs::read_to_string(f.dir.join("bundled/Base")).unwrap(),
            source
        );
        assert_eq!(
            fs::read_to_string(f.dir.join("themes/Personal")).unwrap(),
            source.replace("background   = #222222", "background   = #aabbcc")
        );
        assert_eq!(fs::read_to_string(f.dir.join("config")).unwrap(), config);
        key(app, KeyCode::Enter);
        assert_eq!(app.mode, Mode::Confirm);
        assert_eq!(fs::read_to_string(f.dir.join("config")).unwrap(), config);
        key(app, KeyCode::Enter);
        assert_eq!(app.toast.as_ref().unwrap().kind, ToastKind::Success);
        assert_eq!(
            fs::read_to_string(f.dir.join("config")).unwrap(),
            config.replace("theme = Base", "theme = Personal")
        );
        assert!(fs::read_dir(&f.dir)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("config.bak.")));
    }

    #[test]
    fn validation_reset_cancel_and_name_collisions_keep_draft() {
        let mut f = fixture("cancel", "background = #123456\n", "");
        let app = &mut f.app;
        key(app, KeyCode::Char('c'));
        key(app, KeyCode::Enter);
        submit(app, "#123456ff");
        assert_eq!(app.mode, Mode::Input);
        assert!(!app.customize.as_ref().unwrap().dirty());
        key(app, KeyCode::Esc);
        assert_eq!(app.mode, Mode::Customize);
        key(app, KeyCode::Enter);
        submit(app, "#654321");
        key(app, KeyCode::Char('r'));
        assert_eq!(app.customize.as_ref().unwrap().value(0), "#123456");
        assert!(!app.customize.as_ref().unwrap().dirty());
        key(app, KeyCode::Enter);
        submit(app, "#abcdef");
        key(app, KeyCode::Char('s'));
        for name in ["Base", "../escape", ".hidden", "dark:Bad", ""] {
            submit(app, name);
            assert_eq!(app.mode, Mode::Input);
            assert!(app.customize.as_ref().unwrap().dirty());
        }
        key(app, KeyCode::Esc);
        key(app, KeyCode::Esc);
        assert_eq!(app.mode, Mode::Customize);
        key(app, KeyCode::Esc);
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.customize.is_none());
        assert!(!f.dir.join("themes").exists());
    }

    #[test]
    fn active_overrides_and_repeated_palette_slots_survive() {
        let mut f = fixture("inline", "background = #123456\npalette = 1=#111111\npalette = 2=blue\npalette = 200=red\n", "theme = Other\ntheme = \"Base\"\nbackground = black\nbackground = #654321\npalette = 1=#222222\npalette = 1=#333333\nselection-foreground = cell-foreground\n");
        f.app.start_customize();
        let draft = f.app.customize.as_mut().unwrap();
        assert_eq!(draft.value(0), "#654321");
        draft.set_color(7, "#abc").unwrap();
        assert_eq!(draft.preview().palette[1], Some(Rgb::new(170, 187, 204)));
        let text = draft.document().render();
        for line in [
            "palette = 1=#111111",
            "palette = 1=#222222",
            "palette = 2=blue",
            "palette = 200=red",
            "selection-foreground = cell-foreground",
            "background = black",
        ] {
            assert!(text.contains(line), "{line}");
        }
        assert!(!text.contains("palette = 1=#333333"));
        assert!(!text.contains("theme ="));
    }

    #[test]
    fn new_file_save_refuses_existing_files_and_symlinks_and_preserves_permissions() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let mut f = fixture(
            "collision",
            "background = #123456\n",
            "theme = Other\nbackground = #ffffff\n",
        );
        fs::set_permissions(
            f.dir.join("bundled/Base"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        f.app.start_customize();
        let draft = f.app.customize.as_ref().unwrap();
        assert_eq!(
            draft.value(0),
            "#123456",
            "other theme overrides must not be copied"
        );
        let dest = f.dir.join("themes");
        draft.save(&dest, "Personal").unwrap();
        assert_eq!(
            fs::metadata(dest.join("Personal"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        fs::write(dest.join("Personal"), "keep").unwrap();
        assert!(draft.save(&dest, "Personal").is_err());
        assert_eq!(fs::read_to_string(dest.join("Personal")).unwrap(), "keep");
        symlink(dest.join("missing"), dest.join("Link")).unwrap();
        assert!(draft.save(&dest, "Link").is_err());
        assert!(!dest.join("missing").exists());
        assert_eq!(
            fs::read_dir(dest).unwrap().count(),
            2,
            "temporary files cleaned up"
        );
    }
}
