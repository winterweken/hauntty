//! In-crate smoke test: build an App against a throwaway config and drive the
//! render + input path through a headless TestBackend so we catch panics in the
//! UI/event layer without needing a real terminal.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::app::{App, Mode, Tab};
use crate::event::handle_key;
use crate::ui;
use hauntty::paths::Paths;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// RAII guard that removes a directory on drop, even during panic unwind.
struct TempDir(std::path::PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A throwaway config and themes dir, and an `App` over them.
fn fresh_app(tag: &str) -> (App, TempDir) {
    let path = std::env::temp_dir().join(format!("hauntty-smoke-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    let dir = TempDir(path);
    let cfg = dir.0.join("config");
    std::fs::write(&cfg, "background = 282a36\n").unwrap();
    let themes = dir.0.join("bundled");
    std::fs::create_dir_all(&themes).unwrap();
    (
        App::new(Paths::resolve(Some(cfg), Some(themes))).unwrap(),
        dir,
    )
}

/// The rendered screen, one string per row.
fn screen_rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buf = terminal.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
                .collect()
        })
        .collect()
}

// A long toast grows downward rather than clipping: the release URL from an
// update check has to be fully readable on an 80-column terminal.
#[cfg(feature = "online")]
#[test]
fn long_toast_wraps_instead_of_clipping() {
    use hauntty::update::{UpdateStatus, Version};
    let (mut app, _dir) = fresh_app("toast");
    let url = "https://github.com/winterweken/hauntty/releases/tag/v1.1.0";
    app.show_update_status(Ok(UpdateStatus::Available {
        current: Version::parse("1.0.0").unwrap(),
        latest: Version::parse("1.1.0").unwrap(),
        url: url.into(),
    }));

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| ui::render(f, &app)).unwrap();
    let rows = screen_rows(&terminal);
    assert!(
        rows.iter().any(|r| r.contains("1.1.0 is available")),
        "{}",
        rows.join("\n")
    );
    assert!(
        rows.iter().any(|r| r.contains(url)),
        "the URL must be on screen in full:\n{}",
        rows.join("\n")
    );

    // Tiny terminals still render the same toast without panicking.
    for (w, h) in [(30, 10), (1, 1)] {
        Terminal::new(TestBackend::new(w, h))
            .unwrap()
            .draw(|f| ui::render(f, &app))
            .unwrap();
    }
}

#[test]
fn renders_and_handles_input_without_panicking() {
    let path = std::env::temp_dir().join(format!("hauntty-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    let dir = TempDir(path);

    let cfg = dir.0.join("config");
    std::fs::write(
        &cfg,
        "font-size = 16\nbackground = 282a36\npalette = 0=#21222c\n",
    )
    .unwrap();

    let themes = dir.0.join("bundled");
    std::fs::create_dir_all(&themes).unwrap();
    std::fs::write(
        themes.join("Dracula"),
        "palette = 0=#21222c\npalette = 4=#bd93f9\nbackground = #282a36\nforeground = #f8f8f2\n",
    )
    .unwrap();

    let paths = Paths::resolve(Some(cfg.clone()), Some(themes.clone()));
    let mut app = App::new(paths).unwrap();
    assert!(!app.themes.is_empty());

    let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
    terminal.draw(|f| ui::render(f, &app)).unwrap();

    // Settings tab: move to a numeric setting (font-size) and adjust it, which
    // should mark the doc dirty. (Right on the text field font-family is a
    // deliberate no-op.)
    handle_key(&mut app, key(KeyCode::Tab));
    assert_eq!(app.tab, Tab::Settings);
    terminal.draw(|f| ui::render(f, &app)).unwrap();
    handle_key(&mut app, key(KeyCode::Down)); // font-family -> font-size
    handle_key(&mut app, key(KeyCode::Right));
    assert!(app.dirty);
    terminal.draw(|f| ui::render(f, &app)).unwrap();

    // Move through Starship and Tools, then back to Themes.
    handle_key(&mut app, key(KeyCode::Tab));
    assert_eq!(app.tab, Tab::Starship);
    terminal.draw(|f| ui::render(f, &app)).unwrap();

    handle_key(&mut app, key(KeyCode::Tab));
    assert_eq!(app.tab, Tab::Tools);
    terminal.draw(|f| ui::render(f, &app)).unwrap();
    // Only explicit y queues installation; tests never execute installers.
    app.tools.confirmation = Some(hauntty::tools::homebrew_plan());
    app.mode = Mode::ToolInstall;
    for (width, height) in [(120, 32), (80, 24), (30, 10), (1, 1)] {
        let mut small = Terminal::new(TestBackend::new(width, height)).unwrap();
        small.draw(|f| ui::render(f, &app)).unwrap();
    }
    crate::event::handle_event(&mut app, crossterm::event::Event::Paste("y\n".into()));
    handle_key(&mut app, key(KeyCode::Enter));
    assert!(app.tools.pending.is_none());
    handle_key(&mut app, key(KeyCode::Esc));
    assert!(app.tools.confirmation.is_none());
    assert_eq!(app.mode, Mode::Normal);
    app.tools.confirmation = Some(hauntty::tools::homebrew_plan());
    app.mode = Mode::ToolInstall;
    handle_key(&mut app, key(KeyCode::Char('y')));
    assert!(app.tools.pending.take().is_some());
    assert!(app.dirty, "install handoff must retain unsaved settings");
    app.finish_tool_install(Err(anyhow::anyhow!("simulated interruption")));
    assert!(app.dirty);
    assert!(app
        .tools
        .last_result
        .as_ref()
        .unwrap()
        .contains("simulated interruption"));
    handle_key(&mut app, key(KeyCode::Tab));
    assert_eq!(app.tab, Tab::Themes);
    handle_key(&mut app, key(KeyCode::Char('/')));

    assert_eq!(app.mode, Mode::Filter);
    handle_key(&mut app, key(KeyCode::Char('d')));
    terminal.draw(|f| ui::render(f, &app)).unwrap();
    handle_key(&mut app, key(KeyCode::Esc));

    // Open the apply confirmation and render it.
    handle_key(&mut app, key(KeyCode::Enter));
    assert_eq!(app.mode, Mode::Confirm);
    terminal.draw(|f| ui::render(f, &app)).unwrap();

    // Help overlay renders.
    handle_key(&mut app, key(KeyCode::Esc));
    handle_key(&mut app, key(KeyCode::Char('?')));
    assert_eq!(app.mode, Mode::Help);
    terminal.draw(|f| ui::render(f, &app)).unwrap();
    handle_key(&mut app, key(KeyCode::Esc));

    // Fetch loading overlay renders its spinner (no real network needed).
    #[cfg(feature = "online")]
    {
        app.mode = Mode::Fetch;
        app.fetching = true;
        app.fetch = None;
        let before = app.spinner;
        app.tick();
        assert_ne!(app.spinner, before, "tick should advance the spinner");
        let _ = app.spinner_frame();
        terminal.draw(|f| ui::render(f, &app)).unwrap();
        app.cancel_overlay();
        assert!(!app.fetching, "cancel should stop the fetching state");
    }
}

/// A throwaway app whose theme list is filtered down to three fixture themes
/// (the host's real Ghostty themes are also discovered, so tests must not
/// assume the list holds only these). `theme_line` is the config's
/// `theme = …` value.
fn fixture(tag: &str, theme_line: &str) -> (TempDir, App) {
    let path = std::env::temp_dir().join(format!("hauntty-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    let dir = TempDir(path);
    let cfg = dir.0.join("config");
    std::fs::write(&cfg, format!("theme = {theme_line}\n")).unwrap();
    let themes = dir.0.join("bundled");
    std::fs::create_dir_all(&themes).unwrap();
    for name in ["HaunttyFixA", "HaunttyFixB", "HaunttyFixC"] {
        std::fs::write(
            themes.join(name),
            "palette = 0=#21222c\npalette = 4=#bd93f9\nbackground = #282a36\nforeground = #f8f8f2\n",
        )
        .unwrap();
    }
    let mut app = App::new(Paths::resolve(Some(cfg), Some(themes))).unwrap();
    app.filter = "HaunttyFix".into();
    app.recompute_filter();
    assert_eq!(app.filtered.len(), 3);
    (dir, app)
}

/// Render at the given size and return each row as plain text.
fn screen(app: &App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::render(f, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect()
        })
        .collect()
}

#[test]
fn active_theme_is_marked_in_the_list() {
    let (_dir, app) = fixture("active", "HaunttyFixB");
    assert_eq!(app.active_theme_names(), vec!["HaunttyFixB".to_string()]);
    let rows = screen(&app, 120, 30);
    let list_half = |name: &str| -> String {
        rows.iter()
            .map(|r| r.chars().take(50).collect::<String>())
            .find(|r| r.contains(name))
            .unwrap_or_else(|| panic!("{name} not listed"))
    };
    assert!(list_half("HaunttyFixB").contains('●'));
    assert!(!list_half("HaunttyFixC").contains('●'));
}

#[test]
fn active_theme_names_understand_light_dark_pairs() {
    let (_dir, app) = fixture("pair", "\"dark:HaunttyFixB,light:HaunttyFixA\"");
    assert_eq!(
        app.active_theme_names(),
        vec!["HaunttyFixB".to_string(), "HaunttyFixA".to_string()]
    );
}

#[test]
fn help_bar_keeps_the_help_hint_on_narrow_terminals() {
    let (_dir, mut app) = fixture("helpbar", "HaunttyFixB");
    for tab in [Tab::Themes, Tab::Settings, Tab::Starship, Tab::Tools] {
        app.tab = tab;
        for width in [120, 60, 44] {
            let rows = screen(&app, width, 24);
            let bar = rows.last().unwrap();
            assert!(
                bar.contains("? ") && bar.contains("help"),
                "{tab:?} at {width} columns: {bar:?}"
            );
        }
    }
}

#[test]
fn jump_keys_work_on_every_list() {
    let (_dir, mut app) = fixture("jump", "HaunttyFixB");
    handle_key(&mut app, key(KeyCode::Char('G')));
    assert_eq!(app.theme_selected, 2);
    // End from a non-zero row must not overflow back to the top.
    handle_key(&mut app, key(KeyCode::Up));
    handle_key(&mut app, key(KeyCode::End));
    assert_eq!(app.theme_selected, 2);
    handle_key(&mut app, key(KeyCode::Char('g')));
    assert_eq!(app.theme_selected, 0);

    app.tab = Tab::Settings;
    handle_key(&mut app, key(KeyCode::End));
    assert_eq!(app.setting_selected, app.settings.len() - 1);
    handle_key(&mut app, key(KeyCode::PageUp));
    assert!(app.setting_selected < app.settings.len() - 1);
    handle_key(&mut app, key(KeyCode::Char('g')));
    assert_eq!(app.setting_selected, 0);

    app.tab = Tab::Starship;
    handle_key(&mut app, key(KeyCode::Char('G')));
    assert_eq!(app.starship_selected, app.starship_filtered.len() - 1);

    app.tab = Tab::Tools;
    handle_key(&mut app, key(KeyCode::End));
    assert_eq!(app.tools.selected, hauntty::tools::CATALOG.len() - 1);
    handle_key(&mut app, key(KeyCode::Home));
    assert_eq!(app.tools.selected, 0);
}

#[test]
fn an_empty_filter_result_says_so_instead_of_blaming_the_install() {
    let (_dir, mut app) = fixture("nomatch", "HaunttyFixB");
    app.filter = "qqqq".into();
    app.recompute_filter();
    let text = screen(&app, 120, 30).join("\n");
    assert!(text.contains("No themes match"), "{text}");
    assert!(!text.contains("Check that Ghostty is installed"));
}

#[test]
fn narrow_terminals_keep_the_theme_list_readable() {
    let (_dir, app) = fixture("narrow", "HaunttyFixB");
    // Too narrow for side-by-side: the list takes the full width.
    let rows = screen(&app, 50, 16);
    let row = rows.iter().find(|r| r.contains("HaunttyFixC")).unwrap();
    assert!(row.trim_end().chars().count() > 30, "{row:?}");
    // Tall and narrow: the preview stacks underneath.
    let text = screen(&app, 50, 40).join("\n");
    assert!(text.contains("HaunttyFixC") && text.contains("cargo run"));
}

#[test]
fn preview_reports_contrast() {
    let (_dir, app) = fixture("contrast", "HaunttyFixB");
    let text = screen(&app, 120, 30).join("\n");
    assert!(text.contains("contrast"), "{text}");
}

#[test]
fn every_screen_survives_tiny_terminals() {
    let (_dir, mut app) = fixture("tiny", "HaunttyFixB");
    for tab in [Tab::Themes, Tab::Settings, Tab::Starship, Tab::Tools] {
        app.tab = tab;
        for mode in [Mode::Normal, Mode::Help] {
            app.mode = mode;
            for (w, h) in [(200, 60), (80, 24), (40, 12), (12, 4), (3, 2), (1, 1)] {
                screen(&app, w, h);
            }
        }
    }
}
