//! Translate key events into [`App`] actions, per interaction mode.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::{App, Mode, Tab, ToastKind};

pub fn handle_event(app: &mut App, event: Event) {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(app, key),
        Event::Paste(text) => handle_paste(app, &text),
        _ => {}
    }
}

fn handle_paste(app: &mut App, text: &str) {
    // Paste is data, never shortcuts or an implicit Enter/confirmation.
    let text = text.trim_end_matches(['\r', '\n']);
    if text.chars().any(char::is_control) {
        app.toast(
            ToastKind::Error,
            "Paste a single line (one file at a time for import).",
        );
        return;
    }
    match app.mode {
        Mode::Input => {
            #[cfg(feature = "import-iterm")]
            {
                app.import_browser = None;
            }
            if let Some(input) = &mut app.input {
                input.buffer.push_str(text);
            }
        }
        Mode::Filter => {
            if app.tab == Tab::Starship {
                app.starship_filter.push_str(text);
                app.recompute_starship_filter();
            } else {
                app.filter.push_str(text);
                app.recompute_filter();
            }
        }
        Mode::Confirm => {
            if let Some(confirm) = &mut app.confirm {
                if confirm.editing_name {
                    confirm.backup_name.push_str(text);
                }
            }
        }
        #[cfg(feature = "online")]
        Mode::Fetch => {
            if let Some(fetch) = &mut app.fetch {
                fetch.filter.push_str(text);
            }
            app.fetch_filter_changed();
        }
        _ => {}
    }
}

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
        Mode::Customize => handle_customize(app, key),
        Mode::ToolInstall => match key.code {
            KeyCode::Char('y') => app.confirm_tool_install(),
            KeyCode::Esc | KeyCode::Char('n') => {
                app.tools.confirmation = None;
                app.mode = Mode::Normal;
            }
            KeyCode::Down | KeyCode::PageDown => {
                app.tools.scroll = app.tools.scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::PageUp => app.tools.scroll = app.tools.scroll.saturating_sub(1),
            _ => {}
        },
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
        KeyCode::Tab => {
            app.tab = match app.tab {
                Tab::Themes => Tab::Settings,
                Tab::Settings => Tab::Starship,
                Tab::Starship => Tab::Tools,
                Tab::Tools => Tab::Themes,
            };
            return;
        }
        KeyCode::BackTab => {
            app.tab = match app.tab {
                Tab::Themes => Tab::Tools,
                Tab::Settings => Tab::Themes,
                Tab::Starship => Tab::Settings,
                Tab::Tools => Tab::Starship,
            };
            return;
        }
        KeyCode::Char('1') => {
            app.tab = Tab::Themes;
            return;
        }
        KeyCode::Char('2') => {
            app.tab = Tab::Settings;
            return;
        }
        KeyCode::Char('3') => {
            app.tab = Tab::Starship;
            return;
        }
        KeyCode::Char('4') => {
            app.tab = Tab::Tools;
            return;
        }
        KeyCode::Char('?') => {
            app.mode = Mode::Help;
            return;
        }
        _ => {}
    }

    match app.tab {
        Tab::Tools => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.tools.selected = app.tools.selected.saturating_sub(1);
                app.tools.scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.tools.selected =
                    (app.tools.selected + 1).min(hauntty::tools::CATALOG.len() - 1);
                app.tools.scroll = 0;
            }
            KeyCode::Enter | KeyCode::Char('i') => app.review_tool_install(),
            KeyCode::Char('b') => app.review_homebrew_install(),
            KeyCode::Char('r') => app.tools.host = hauntty::tools::Host::detect(),
            KeyCode::PageDown => app.tools.scroll = app.tools.scroll.saturating_add(5),
            KeyCode::PageUp => app.tools.scroll = app.tools.scroll.saturating_sub(5),
            _ => {}
        },
        Tab::Themes => match key.code {
            KeyCode::Up | KeyCode::Char('k') => app.move_theme(-1),
            KeyCode::Down | KeyCode::Char('j') => app.move_theme(1),
            KeyCode::PageUp => app.move_theme(-10),
            KeyCode::PageDown => app.move_theme(10),
            KeyCode::Home => app.move_theme(-(i32::MAX)),
            KeyCode::End => app.move_theme(i32::MAX),
            KeyCode::Char('/') => app.mode = Mode::Filter,
            KeyCode::Enter => app.start_apply(),
            KeyCode::Char('c') => app.start_customize(),
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
        Tab::Starship => match key.code {
            KeyCode::Up | KeyCode::Char('k') => app.move_starship(-1),
            KeyCode::Down | KeyCode::Char('j') => app.move_starship(1),
            KeyCode::PageUp => app.move_starship(-5),
            KeyCode::PageDown => app.move_starship(5),
            KeyCode::Home => app.move_starship(-(i32::MAX)),
            KeyCode::End => app.move_starship(i32::MAX),
            KeyCode::Char('/') => app.mode = Mode::Filter,
            KeyCode::Enter => app.apply_starship_preset(),
            KeyCode::Char('i') => app.install_starship(),
            #[cfg(feature = "online")]
            KeyCode::Char('f') => app.start_starship_fetch(),
            _ => {}
        },
    }
}

fn handle_customize(app: &mut App, key: KeyEvent) {
    let Some(draft) = &mut app.customize else {
        return;
    };
    let was_armed = draft.discard_armed;
    draft.discard_armed = false;
    if let Some(picker) = &mut draft.picker {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => picker.control = picker.control.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => picker.control = (picker.control + 1).min(3),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('-') => {
                picker.color = picker.adjusted(-1)
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('+') | KeyCode::Char('=') => {
                picker.color = picker.adjusted(1)
            }
            KeyCode::Char('1'..='3') => {
                if let KeyCode::Char(n) = key.code {
                    picker.step = n as usize - '1' as usize;
                }
            }
            KeyCode::Tab => picker.step = (picker.step + 1) % crate::customize::PICKER_STEPS.len(),
            KeyCode::BackTab => {
                picker.step = (picker.step + crate::customize::PICKER_STEPS.len() - 1)
                    % crate::customize::PICKER_STEPS.len()
            }
            KeyCode::Char('r') => picker.color = picker.original,
            KeyCode::Enter => draft.accept_picker(),
            KeyCode::Esc | KeyCode::Char('q') => draft.picker = None,
            _ => {}
        }
        return;
    }
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => draft.selected = draft.selected.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            draft.selected = (draft.selected + 1).min(crate::customize::COLOR_COUNT - 1)
        }
        KeyCode::Enter => app.edit_custom_color(),
        KeyCode::Char('p') => draft.open_picker(),
        KeyCode::Char('r') => app.reset_custom_color(),
        KeyCode::Char('s') => app.name_custom_theme(),
        KeyCode::Esc | KeyCode::Char('q') => {
            if draft.dirty() && !was_armed {
                draft.discard_armed = true;
                app.toast(
                    ToastKind::Info,
                    "Unsaved custom colors — s to save, or Esc / q again to discard.",
                );
            } else {
                app.customize = None;
                app.mode = Mode::Normal;
            }
        }
        _ => {}
    }
}

fn handle_filter(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => app.mode = Mode::Normal,
        KeyCode::Backspace => {
            if app.tab == Tab::Starship {
                app.starship_filter.pop();
                app.recompute_starship_filter();
            } else {
                app.filter.pop();
                app.recompute_filter();
            }
        }
        KeyCode::Up => {
            if app.tab == Tab::Starship {
                app.move_starship(-1);
            } else {
                app.move_theme(-1);
            }
        }
        KeyCode::Down => {
            if app.tab == Tab::Starship {
                app.move_starship(1);
            } else {
                app.move_theme(1);
            }
        }
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER) =>
        {
            if app.tab == Tab::Starship {
                app.starship_filter.push(c);
                app.recompute_starship_filter();
            } else {
                app.filter.push(c);
                app.recompute_filter();
            }
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
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER) =>
                {
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
    #[cfg(feature = "import-iterm")]
    if app
        .input
        .as_ref()
        .is_some_and(|i| i.purpose == crate::app::InputPurpose::ImportPath)
    {
        if key.code == KeyCode::Tab {
            app.toggle_import_browser();
            return;
        }
        if let Some(browser) = &mut app.import_browser {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => browser.move_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => browser.move_selection(1),
                KeyCode::Enter | KeyCode::Right => app.choose_import_entry(),
                KeyCode::Backspace | KeyCode::Left => app.import_parent(),
                KeyCode::Esc => app.import_browser = None,
                _ => {}
            }
            return;
        }
    }
    match key.code {
        KeyCode::Enter => app.submit_input(),
        KeyCode::Esc => app.cancel_overlay(),
        KeyCode::Backspace => {
            if let Some(i) = &mut app.input {
                i.buffer.pop();
            }
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(i) = &mut app.input {
                i.buffer.clear();
            }
        }
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER) =>
        {
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
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER) =>
        {
            if let Some(f) = &mut app.fetch {
                f.filter.push(c);
            }
            app.fetch_filter_changed();
        }
        _ => {}
    }
}
