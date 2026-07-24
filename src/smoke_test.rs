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

#[test]
fn renders_and_handles_input_without_panicking() {
    let dir = std::env::temp_dir().join(format!("hauntty-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let cfg = dir.join("config");
    std::fs::write(
        &cfg,
        "font-size = 16\nbackground = 282a36\npalette = 0=#21222c\n",
    )
    .unwrap();

    let themes = dir.join("bundled");
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

    // Back to themes, enter filter mode and type.
    handle_key(&mut app, key(KeyCode::Tab));
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

    let _ = std::fs::remove_dir_all(&dir);
}
