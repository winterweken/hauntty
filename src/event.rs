//! Translate key events into [`App`] actions, per interaction mode.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Mode, Tab, ToastKind};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    // Ctrl-C always quits immediately.
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }

    // Any keypress dismisses a lingering toast.
    app.toast = None;

    match app.mode {
        Mode::Normal => handle_normal(app, key),
        Mode::Filter => handle_filter(app, key),
        Mode::Confirm => handle_confirm(app, key),
        Mode::Input => handle_input(app, key),
        Mode::Help => {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
            ) {
                app.mode = Mode::Normal;
            }
        }
        #[cfg(feature = "online")]
        Mode::Fetch => handle_fetch(app, key),
    }
}

fn handle_normal(app: &mut App, key: KeyEvent) {
    let was_armed = app.armed_quit;
    app.armed_quit = false;

    match key.code {
        KeyCode::Char('q') => {
            if app.dirty && !was_armed {
                app.armed_quit = true;
                app.toast(
                    ToastKind::Info,
                    "Unsaved changes — press s to save, or q again to discard.",
                );
            } else {
                app.should_quit = true;
            }
            return;
        }
        KeyCode::Tab | KeyCode::BackTab => {
            app.tab = match app.tab {
                Tab::Themes => Tab::Settings,
                Tab::Settings => Tab::Themes,
            };
            return;
        }
        KeyCode::Char('?') => {
            app.mode = Mode::Help;
            return;
        }
        _ => {}
    }

    match app.tab {
        Tab::Themes => match key.code {
            KeyCode::Up | KeyCode::Char('k') => app.move_theme(-1),
            KeyCode::Down | KeyCode::Char('j') => app.move_theme(1),
            KeyCode::PageUp => app.move_theme(-10),
            KeyCode::PageDown => app.move_theme(10),
            KeyCode::Home => app.move_theme(-(i32::MAX)),
            KeyCode::End => app.move_theme(i32::MAX),
            KeyCode::Char('/') => app.mode = Mode::Filter,
            KeyCode::Enter => app.start_apply(),
            #[cfg(feature = "import-iterm")]
            KeyCode::Char('i') => app.start_import(),
            #[cfg(feature = "online")]
            KeyCode::Char('f') => app.start_fetch(),
            _ => {}
        },
        Tab::Settings => match key.code {
            KeyCode::Up | KeyCode::Char('k') => app.move_setting(-1),
            KeyCode::Down | KeyCode::Char('j') => app.move_setting(1),
            KeyCode::Left | KeyCode::Char('h') => app.adjust_setting(-1),
            KeyCode::Right | KeyCode::Char('l') => app.adjust_setting(1),
            KeyCode::Enter | KeyCode::Char(' ') => app.activate_setting(),
            KeyCode::Char('s') => app.save_settings(),
            _ => {}
        },
    }
}

fn handle_filter(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => app.mode = Mode::Normal,
        KeyCode::Backspace => {
            app.filter.pop();
            app.recompute_filter();
        }
        KeyCode::Up => app.move_theme(-1),
        KeyCode::Down => app.move_theme(1),
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.filter.push(c);
            app.recompute_filter();
        }
        _ => {}
    }
}

fn handle_confirm(app: &mut App, key: KeyEvent) {
    let editing = app
        .confirm
        .as_ref()
        .map(|c| c.editing_name)
        .unwrap_or(false);
    if editing {
        if let Some(c) = &mut app.confirm {
            match key.code {
                KeyCode::Enter | KeyCode::Esc => c.editing_name = false,
                KeyCode::Backspace => {
                    c.backup_name.pop();
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    c.backup_name.push(ch);
                }
                _ => {}
            }
        }
        return;
    }
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => app.confirm_apply(),
        KeyCode::Char('n') | KeyCode::Esc => app.cancel_overlay(),
        KeyCode::Char('e') => {
            if let Some(c) = &mut app.confirm {
                c.editing_name = true;
            }
        }
        _ => {}
    }
}

fn handle_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => app.submit_input(),
        KeyCode::Esc => app.cancel_overlay(),
        KeyCode::Backspace => {
            if let Some(i) = &mut app.input {
                i.buffer.pop();
            }
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(i) = &mut app.input {
                i.buffer.push(c);
            }
        }
        _ => {}
    }
}

#[cfg(feature = "online")]
fn handle_fetch(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.cancel_overlay(),
        KeyCode::Up => app.fetch_move(-1),
        KeyCode::Down => app.fetch_move(1),
        KeyCode::Enter => app.fetch_download_selected(),
        KeyCode::Backspace => {
            if let Some(f) = &mut app.fetch {
                f.filter.pop();
            }
            app.fetch_filter_changed();
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(f) = &mut app.fetch {
                f.filter.push(c);
            }
            app.fetch_filter_changed();
        }
        _ => {}
    }
}
