//! Top-level TUI rendering.

mod color_picker;
mod preview;
mod style;
mod tools;

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use hauntty::settings::Widget;
use hauntty::theme::Theme;

use crate::app::{App, Mode, Tab, ToastKind};
use style::{ACCENT, CURSOR, GREEN, MUTED, RED, TEXT, YELLOW};

/// Below this width a list and its preview no longer fit side by side.
const SIDE_BY_SIDE_MIN_WIDTH: u16 = 72;
/// Rows the theme preview needs when it is stacked under the list.
const STACKED_PREVIEW_HEIGHT: u16 = 15;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    render_tabs(f, chunks[0], app);
    match app.tab {
        Tab::Themes => render_themes(f, chunks[1], app),
        Tab::Settings => render_settings(f, chunks[1], app),
        Tab::Starship => render_starship(f, chunks[1], app),
        Tab::Tools => tools::render(f, chunks[1], app),
    }
    render_help_bar(f, chunks[2], app);

    // Overlays.
    match app.mode {
        Mode::Confirm => render_confirm(f, area, app),
        Mode::Input => render_input(f, area, app),
        Mode::Help => render_help_overlay(f, area, app),
        Mode::ToolInstall => tools::render_confirmation(f, area, app),
        #[cfg(feature = "online")]
        Mode::Fetch => render_fetch(f, area, app),
        _ => {}
    }

    if let Some(toast) = &app.toast {
        render_toast(f, area, toast);
    }
}

fn render_tabs(f: &mut Frame, area: Rect, app: &App) {
    let mut tabs = Vec::new();
    for (i, (tab, name)) in [
        (Tab::Themes, "Themes"),
        (Tab::Settings, "Settings"),
        (Tab::Starship, "Starship"),
        (Tab::Tools, "Tools"),
    ]
    .into_iter()
    .enumerate()
    {
        let style = if tab == app.tab {
            Style::default().fg(ACCENT).bold().underlined()
        } else {
            Style::default().fg(MUTED)
        };
        tabs.push(Span::styled(
            format!(" {} ", i + 1),
            Style::default().fg(MUTED),
        ));
        tabs.push(Span::styled(name, style));
        tabs.push(Span::raw(" "));
    }
    let tabs = Line::from(tabs);

    // The status goes on the right when it fits, shortened before it is dropped.
    let (full, short) = if app.dirty {
        ("● unsaved changes ".to_string(), "● ".to_string())
    } else if !app.warnings.is_empty() {
        let n = app.warnings.len();
        (format!("⚠ {n} warning(s) · ? "), format!("⚠ {n} "))
    } else {
        (String::new(), String::new())
    };
    const BRAND_WIDTH: u16 = 10;
    let room = area.width.saturating_sub(BRAND_WIDTH + tabs.width() as u16) as usize;
    let status = [full, short]
        .into_iter()
        .find(|text| Span::raw(text.as_str()).width() <= room)
        .unwrap_or_default();

    let cols = Layout::horizontal([
        Constraint::Length(BRAND_WIDTH),
        Constraint::Min(0),
        Constraint::Length(Span::raw(status.as_str()).width() as u16),
    ])
    .split(area);
    f.render_widget(
        Paragraph::new(Span::styled(
            " hauntty ",
            Style::default().fg(Color::Black).bg(ACCENT).bold(),
        )),
        cols[0],
    );
    f.render_widget(Paragraph::new(tabs), cols[1]);
    f.render_widget(
        Paragraph::new(Span::styled(status, Style::default().fg(YELLOW)))
            .alignment(Alignment::Right),
        cols[2],
    );
}

/// Split a pane into a list and an optional preview: side by side when wide,
/// stacked when narrow but tall, and list-only when neither fits.
fn list_and_preview(area: Rect, list_percent: u16) -> (Rect, Option<Rect>) {
    if area.width >= SIDE_BY_SIDE_MIN_WIDTH {
        let cols = Layout::horizontal([
            Constraint::Percentage(list_percent),
            Constraint::Percentage(100 - list_percent),
        ])
        .split(area);
        (cols[0], Some(cols[1]))
    } else if area.height >= STACKED_PREVIEW_HEIGHT + 10 {
        let rows = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(STACKED_PREVIEW_HEIGHT),
        ])
        .split(area);
        (rows[0], Some(rows[1]))
    } else {
        (area, None)
    }
}

/// The one-line filter prompt above a filterable list.
fn filter_line<'a>(filter: &'a str, editing: bool, noun: &str) -> Line<'a> {
    if editing || !filter.is_empty() {
        Line::from(vec![
            Span::styled(" / ", Style::default().fg(ACCENT)),
            Span::raw(filter),
            Span::styled(if editing { "▏" } else { "" }, Style::default().fg(ACCENT)),
        ])
    } else {
        Line::from(Span::styled(
            format!(" press / to filter {noun}"),
            Style::default().fg(MUTED),
        ))
    }
}

fn render_themes(f: &mut Frame, area: Rect, app: &App) {
    if let Some(draft) = &app.customize {
        render_customize(f, area, draft);
        return;
    }
    let (list_area, preview_area) = list_and_preview(area, 42);

    // Left: filter + list.
    let left = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(list_area);
    f.render_widget(
        Paragraph::new(filter_line(&app.filter, app.mode == Mode::Filter, "themes")),
        left[0],
    );

    let empty_message = if !app.filtered.is_empty() {
        None
    } else if app.themes.is_empty() {
        Some("No themes found.\n\nCheck that Ghostty is installed, or press i to import a .itermcolors file.".to_string())
    } else {
        Some(format!(
            "No themes match “{}”.\n\nPress / and Backspace to widen the filter.",
            app.filter
        ))
    };
    let empty = |text: String| {
        Paragraph::new(text)
            .fg(MUTED)
            .block(style::plain_panel())
            .wrap(Wrap { trim: true })
    };

    // With no room for a preview pane, the message replaces the list.
    if let (Some(text), None) = (&empty_message, preview_area) {
        f.render_widget(empty(text.clone()), left[1]);
        return;
    }

    let active = app.active_theme_names();
    // Row = cursor (2) + active marker (2) + name + gap (1) + swatches (6) + tag (6).
    let name_width = (left[1].width as usize).saturating_sub(2 + 17).clamp(8, 40);
    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .filter_map(|&i| app.themes.ordered.get(i))
        .map(|theme| theme_list_item(theme, active.contains(&theme.name), name_width))
        .collect();

    let list = List::new(items)
        .block(style::panel(style::list_title(
            "themes",
            app.theme_selected,
            app.filtered.len(),
        )))
        .highlight_style(style::selection())
        .highlight_symbol(CURSOR);
    let mut state = ListState::default();
    if !app.filtered.is_empty() {
        state.select(Some(app.theme_selected));
    }
    f.render_stateful_widget(list, left[1], &mut state);
    style::scrollbar(f, left[1], app.filtered.len(), app.theme_selected);

    // Right (or below): live preview.
    let Some(preview_area) = preview_area else {
        return;
    };
    match (app.current_theme(), empty_message) {
        (Some(theme), _) => preview::render(f, preview_area, theme),
        (None, message) => f.render_widget(empty(message.unwrap_or_default()), preview_area),
    }
}

fn render_customize(f: &mut Frame, area: Rect, draft: &crate::customize::Draft) {
    let (controls, preview_area) = list_and_preview(area, 45);
    if let Some(preview_area) = preview_area {
        preview::render(f, preview_area, &draft.preview());
    }
    if let Some(picker) = &draft.picker {
        color_picker::render(f, controls, draft, picker);
        return;
    }
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(controls);
    let items: Vec<ListItem> = (0..crate::customize::COLOR_COUNT)
        .map(|index| {
            let value = draft.value(index);
            let swatch = hauntty::theme::Rgb::parse_hex(&value).map(|c| c.to_ratatui());
            ListItem::new(Line::from(vec![
                Span::styled(
                    "  ",
                    swatch.map(|c| Style::default().bg(c)).unwrap_or_default(),
                ),
                Span::raw(format!(" {:<22}", crate::customize::label(index))),
                if value.is_empty() {
                    Span::styled("default", Style::default().fg(MUTED))
                } else {
                    Span::styled(value, Style::default().fg(TEXT))
                },
            ]))
        })
        .collect();
    let title = Line::from(if draft.dirty() {
        vec![
            Span::raw(" custom colors "),
            Span::styled("● unsaved ", Style::default().fg(YELLOW)),
        ]
    } else {
        vec![Span::raw(" custom colors ")]
    });
    let mut state = ListState::default().with_selected(Some(draft.selected));
    f.render_stateful_widget(
        List::new(items)
            .block(style::panel(title))
            .highlight_style(style::selection())
            .highlight_symbol(CURSOR),
        rows[0],
        &mut state,
    );
    style::scrollbar(f, rows[0], crate::customize::COLOR_COUNT, draft.selected);
    f.render_widget(
        Paragraph::new(" Hex colors preview live; other values are preserved.").fg(MUTED),
        rows[1],
    );
}

fn theme_list_item(theme: &Theme, active: bool, name_width: usize) -> ListItem<'static> {
    let mut spans = vec![
        Span::styled(if active { "● " } else { "  " }, Style::default().fg(GREEN)),
        Span::raw(format!(
            "{:<name_width$} ",
            truncate(&theme.name, name_width)
        )),
    ];
    // mini swatch strip
    for i in [1usize, 2, 4, 5, 6, 3] {
        spans.push(match theme.palette[i] {
            Some(c) => Span::styled(" ", Style::default().bg(c.to_ratatui())),
            None => Span::raw(" "),
        });
    }
    if theme.source == hauntty::theme::ThemeSource::User {
        spans.push(Span::styled("  user", Style::default().fg(MUTED)));
    }
    ListItem::new(Line::from(spans))
}

fn render_settings(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(4)]).split(area);

    let mut changed = 0;
    let items: Vec<ListItem> = app
        .settings
        .iter()
        .enumerate()
        .map(|(i, spec)| {
            let (value, is_default) = app.setting_value(i);
            let widget_hint = match &spec.widget {
                Widget::Toggle => {
                    if value == "true" {
                        "◉ on ".to_string()
                    } else {
                        "◯ off".to_string()
                    }
                }
                Widget::Select(_) => format!("‹ {value} ›"),
                Widget::Stepper { .. } => format!("– {value} +"),
                Widget::Text => value.clone(),
            };
            if !is_default {
                changed += 1;
            }
            let (marker, value_style, suffix) = if is_default {
                ("  ", Style::default().fg(MUTED), "  (default)")
            } else {
                ("● ", Style::default().fg(GREEN), "")
            };
            ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(GREEN)),
                Span::raw(format!("{:<24}", spec.label)),
                Span::styled(format!("{:<18}", widget_hint), value_style),
                Span::styled(suffix, Style::default().fg(MUTED)),
            ]))
        })
        .collect();

    let mut title = vec![Span::raw(style::list_title(
        "settings",
        app.setting_selected,
        app.settings.len(),
    ))];
    if changed > 0 {
        title.push(Span::styled(
            format!("● {changed} set "),
            Style::default().fg(GREEN),
        ));
    }
    let list = List::new(items)
        .block(style::panel(Line::from(title)))
        .highlight_style(style::selection())
        .highlight_symbol(CURSOR);
    let mut state = ListState::default();
    state.select(Some(app.setting_selected));
    f.render_stateful_widget(list, rows[0], &mut state);
    style::scrollbar(f, rows[0], app.settings.len(), app.setting_selected);

    // Help text for the selected setting, titled with its config key.
    let selected = app.settings.get(app.setting_selected);
    f.render_widget(
        Paragraph::new(selected.map(|s| s.help).unwrap_or(""))
            .style(Style::default().fg(MUTED))
            .block(style::panel(
                selected.map(|s| format!(" {} ", s.key)).unwrap_or_default(),
            ))
            .wrap(Wrap { trim: true }),
        rows[1],
    );
}

type Hint = (&'static str, &'static str);

/// Key hints for the current tab and mode, most useful first.
fn hints(app: &App) -> Vec<Hint> {
    let picking = app
        .customize
        .as_ref()
        .is_some_and(|draft| draft.picker.is_some());
    let mut keys: Vec<Hint> = match (app.tab, &app.mode) {
        (_, Mode::Filter) => vec![("type", "filter"), ("↑↓", "move"), ("↵/esc", "done")],
        (_, Mode::Confirm) => vec![("y", "apply"), ("e", "edit name"), ("n/esc", "cancel")],
        (_, Mode::Input) => vec![("type", "value"), ("↵", "set"), ("esc", "cancel")],
        (_, Mode::ToolInstall) => vec![("y", "install"), ("n/esc", "cancel"), ("↑↓", "scroll")],
        (_, Mode::Customize) if picking => vec![
            ("↑↓", "control"),
            ("←/→", "Down/Up"),
            ("1/2/3", "step"),
            ("r", "reset"),
            ("↵", "keep"),
            ("esc", "cancel"),
        ],
        (_, Mode::Customize) => vec![
            ("↑↓", "move"),
            ("↵", "hex"),
            ("p", "picker"),
            ("r", "reset"),
            ("s", "save copy"),
            ("esc", "back"),
        ],
        (_, Mode::Help) => vec![("esc", "close")],
        #[cfg(feature = "online")]
        (_, Mode::Fetch) => vec![
            ("↑↓", "move"),
            ("type", "filter"),
            ("↵", "download"),
            ("esc", "close"),
        ],
        (Tab::Themes, _) => {
            let mut keys = vec![
                ("↑↓", "move"),
                ("/", "filter"),
                ("↵", "apply"),
                ("c", "customize"),
            ];
            if cfg!(feature = "import-iterm") {
                keys.push(("i", "import"));
            }
            if cfg!(feature = "online") {
                keys.push(("f", "fetch"));
            }
            keys
        }
        (Tab::Settings, _) => vec![
            ("↑↓", "move"),
            ("←→", "change"),
            ("↵", "edit/toggle"),
            ("s", "save"),
        ],
        (Tab::Starship, _) => {
            let mut keys = vec![
                ("↑↓", "move"),
                ("/", "filter"),
                ("↵", "apply preset"),
                ("i", "install"),
            ];
            if cfg!(feature = "online") {
                keys.push(("f", "fetch"));
            }
            keys
        }
        (Tab::Tools, _) => vec![
            ("↑↓", "move"),
            ("i/↵", "install"),
            ("r", "refresh"),
            ("b", "Homebrew"),
            ("PgUp/Dn", "details"),
        ],
    };
    if app.mode == Mode::Normal {
        keys.extend([("tab", "next tab"), ("?", "help"), ("q", "quit")]);
    }
    keys
}

fn hint_width((key, label): Hint) -> usize {
    Span::raw(key).width() + Span::raw(label).width() + 5
}

/// The hints that fit in `width`, in order. When some must go, `? help` stays
/// so the rest are always one key away.
fn fit_hints(keys: Vec<Hint>, width: usize) -> Vec<Hint> {
    // The last hint's trailing padding may run off the edge.
    let total: usize = keys.iter().copied().map(hint_width).sum();
    if total <= width + 2 {
        return keys;
    }
    let help = keys.iter().copied().find(|(key, _)| *key == "?");
    let mut room = width.saturating_sub(help.map(hint_width).unwrap_or(0)) + 2;
    let mut shown = Vec::new();
    for hint in keys.into_iter().filter(|(key, _)| *key != "?") {
        if hint_width(hint) > room {
            break;
        }
        room -= hint_width(hint);
        shown.push(hint);
    }
    shown.extend(help);
    shown
}

fn render_help_bar(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = Vec::new();
    for (k, label) in fit_hints(hints(app), area.width as usize) {
        spans.push(Span::styled(
            format!(" {k} "),
            Style::default().fg(Color::Black).bg(MUTED),
        ));
        spans.push(Span::styled(
            format!(" {label}  "),
            Style::default().fg(MUTED),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_starship(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::vertical([Constraint::Length(4), Constraint::Min(0)]).split(area);

    let status = &app.starship_status;
    let status_span = if status.installed {
        let ver = status.version.as_deref().unwrap_or("installed");
        Span::styled(format!("✓ {ver}"), Style::default().fg(GREEN).bold())
    } else {
        Span::styled(
            "✗ Not Installed (press i to install)",
            Style::default().fg(RED).bold(),
        )
    };

    let cfg_str = status.config_path.display().to_string();
    let banner_text = vec![
        Line::from(vec![
            Span::styled(" Status: ", Style::default().fg(MUTED)),
            status_span,
            Span::styled("  │  Config: ", Style::default().fg(MUTED)),
            Span::raw(cfg_str),
        ]),
        Line::from(vec![
            Span::styled(" Docs: ", Style::default().fg(MUTED)),
            Span::styled(
                hauntty::starship::STARSHIP_WEBSITE,
                Style::default().fg(ACCENT),
            ),
            Span::styled("  │  Presets Catalog: ", Style::default().fg(MUTED)),
            Span::styled(
                hauntty::starship::STARSHIP_PRESETS_URL,
                Style::default().fg(ACCENT),
            ),
        ]),
    ];
    f.render_widget(
        Paragraph::new(banner_text).block(style::panel(" Starship Prompt Status & Links ")),
        chunks[0],
    );

    let (list_area, detail_area) = if chunks[1].width >= SIDE_BY_SIDE_MIN_WIDTH {
        let cols = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[1]);
        (cols[0], cols[1])
    } else {
        let rows = Layout::vertical([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[1]);
        (rows[0], rows[1])
    };

    let left = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(list_area);
    f.render_widget(
        Paragraph::new(filter_line(
            &app.starship_filter,
            app.mode == Mode::Filter,
            "presets",
        )),
        left[0],
    );

    let name_width = (left[1].width as usize).saturating_sub(4).max(8);
    let items: Vec<ListItem> = app
        .starship_filtered
        .iter()
        .filter_map(|&i| app.starship_presets.get(i))
        .map(|p| ListItem::new(truncate(&p.name, name_width)))
        .collect();

    let list = List::new(items)
        .block(style::panel(style::list_title(
            "presets",
            app.starship_selected,
            app.starship_filtered.len(),
        )))
        .highlight_style(style::selection())
        .highlight_symbol(CURSOR);
    let mut state = ListState::default();
    if !app.starship_filtered.is_empty() {
        state.select(Some(app.starship_selected));
    }
    f.render_stateful_widget(list, left[1], &mut state);
    style::scrollbar(
        f,
        left[1],
        app.starship_filtered.len(),
        app.starship_selected,
    );

    match app.current_starship_preset() {
        Some(preset) => {
            let right_rows = Layout::vertical([
                Constraint::Length(4),
                Constraint::Length(4),
                Constraint::Min(0),
            ])
            .split(detail_area);

            let overview = Paragraph::new(vec![
                Line::from(Span::styled(
                    preset.name.as_ref(),
                    Style::default().fg(ACCENT).bold(),
                )),
                Line::from(Span::styled(
                    preset.description.as_ref(),
                    Style::default().fg(MUTED),
                )),
            ])
            .block(style::panel(" Preset Description "))
            .wrap(Wrap { trim: true });
            f.render_widget(overview, right_rows[0]);

            let preview_p = Paragraph::new(Span::styled(
                preset.preview.as_ref(),
                Style::default().fg(GREEN).bold(),
            ))
            .block(style::overlay(" Prompt Preview "));
            f.render_widget(preview_p, right_rows[1]);

            let toml_p = Paragraph::new(preset.toml_content.as_ref())
                .style(Style::default().fg(TEXT))
                .block(style::panel(" TOML Config (~/.config/starship.toml) "))
                .wrap(Wrap { trim: false });
            f.render_widget(toml_p, right_rows[2]);
        }
        None => {
            let p = Paragraph::new(format!(
                "No Starship presets match “{}”.",
                app.starship_filter
            ))
            .fg(MUTED)
            .block(style::plain_panel());
            f.render_widget(p, detail_area);
        }
    }
}

fn render_confirm(f: &mut Frame, area: Rect, app: &App) {
    let Some(c) = &app.confirm else { return };
    let rect = center(area, 62, if c.will_backup { 9 } else { 6 });
    f.render_widget(Clear, rect);

    let mut lines = vec![
        Line::from(Span::styled(
            format!("Apply theme “{}”?", c.theme_name),
            Style::default().fg(ACCENT).bold(),
        )),
        Line::from(""),
    ];
    if c.will_backup {
        lines.push(Line::from(Span::styled(
            "Your config has inline colors, so hauntty will",
            Style::default().fg(MUTED),
        )));
        lines.push(Line::from(Span::styled(
            "save your current look first as:",
            Style::default().fg(MUTED),
        )));
        let name_span = if c.editing_name {
            Span::styled(format!("  {}▏", c.backup_name), Style::default().fg(GREEN))
        } else {
            Span::styled(
                format!("  {}   (press e to rename)", c.backup_name),
                Style::default().fg(GREEN),
            )
        };
        lines.push(Line::from(name_span));
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        "y = apply    e = rename backup    n/esc = cancel",
        Style::default().fg(MUTED),
    )));

    let p = Paragraph::new(lines)
        .block(style::overlay(" confirm "))
        .wrap(Wrap { trim: true });
    f.render_widget(p, rect);
}

fn render_input(f: &mut Frame, area: Rect, app: &App) {
    let Some(input) = &app.input else { return };
    #[cfg(feature = "import-iterm")]
    if let Some(browser) = &app.import_browser {
        render_import_browser(f, area, browser);
        return;
    }
    let rect = center(area, 76, 7);
    f.render_widget(Clear, rect);
    let block = style::overlay(" input ");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);
    f.render_widget(
        Paragraph::new(input.title.as_str())
            .fg(MUTED)
            .wrap(Wrap { trim: false }),
        rows[0],
    );
    let line = Line::from(vec![
        Span::raw(input.buffer.as_str()),
        Span::styled("▏", Style::default().fg(ACCENT)),
    ]);
    let scroll = line
        .width()
        .saturating_sub(rows[1].width as usize)
        .min(u16::MAX as usize) as u16;
    f.render_widget(Paragraph::new(line).scroll((0, scroll)), rows[1]);
    let hint = "Enter submit · Esc cancel · Ctrl-U clear";
    #[cfg(feature = "import-iterm")]
    let hint = if input.purpose == crate::app::InputPurpose::ImportPath {
        "Enter import · Tab browse · Esc cancel · Ctrl-U clear"
    } else {
        hint
    };
    f.render_widget(
        Paragraph::new(hint).fg(MUTED).wrap(Wrap { trim: false }),
        rows[2],
    );
}

#[cfg(feature = "import-iterm")]
fn render_import_browser(f: &mut Frame, area: Rect, browser: &crate::import_path::Browser) {
    let rect = center(area, 76, 20);
    f.render_widget(Clear, rect);
    let block = style::overlay(" browse .itermcolors ");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(inner);
    f.render_widget(
        Paragraph::new(browser.directory.to_string_lossy())
            .fg(MUTED)
            .wrap(Wrap { trim: false }),
        rows[0],
    );
    let entries: Vec<ListItem> = browser
        .entries
        .iter()
        .map(|entry| {
            let name = if Some(entry.path.as_path()) == browser.directory.parent() {
                "..".to_string()
            } else {
                entry
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            };
            ListItem::new(format!("{name}{}", if entry.is_dir { "/" } else { "" }))
        })
        .collect();
    if entries.is_empty() {
        f.render_widget(
            Paragraph::new("No .itermcolors files in this folder.").fg(MUTED),
            rows[1],
        );
    } else {
        let mut state = ListState::default().with_selected(Some(browser.selected));
        f.render_stateful_widget(
            List::new(entries)
                .highlight_symbol(CURSOR)
                .highlight_style(style::selection()),
            rows[1],
            &mut state,
        );
    }
    f.render_widget(
        Paragraph::new("↑↓ move · Enter choose · ← parent\nTab / Esc return to path").fg(MUTED),
        rows[2],
    );
}

fn render_help_overlay(f: &mut Frame, area: Rect, app: &App) {
    let mut themes = "/ filter · Enter apply · c customize".to_string();
    let mut starship = "/ filter · Enter apply preset · i install".to_string();
    if cfg!(feature = "import-iterm") {
        themes.push_str(" · i import");
    }
    if cfg!(feature = "online") {
        themes.push_str(" · f fetch");
        starship.push_str(" · f fetch");
    }
    let sections: [(&str, &str); 7] = [
        (
            "Global",
            if cfg!(feature = "online") {
                "Tab/Shift-Tab switch tab · 1-4 jump · u check for update · ? help · q quit"
            } else {
                "Tab/Shift-Tab switch tab · 1-4 jump · ? help · q quit"
            },
        ),
        ("Lists", "↑↓ or j/k move · g/G first/last · PgUp/PgDn page"),
        ("Themes", &themes),
        (
            "Customize",
            "Enter hex · p picker · r reset · s save copy · Esc back",
        ),
        ("Settings", "←→ change · Enter edit/toggle · s save"),
        ("Starship", &starship),
        (
            "Tools",
            "Enter review install · r refresh · b Homebrew · PgUp/PgDn details",
        ),
    ];

    let mut lines = vec![
        Line::from(Span::styled(
            "hauntty — Ghostty theme & settings manager",
            Style::default().fg(ACCENT).bold(),
        )),
        Line::from(""),
    ];
    for (name, keys) in sections {
        lines.push(Line::from(vec![
            Span::styled(format!("{name:<11}"), Style::default().fg(ACCENT)),
            Span::raw(keys.to_string()),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "● marks the theme your config uses, and settings you have changed.",
        Style::default().fg(MUTED),
    )));
    lines.push(Line::from(Span::styled(
        "After applying Ghostty settings, reload with ⌘⇧, (cmd+shift+,)",
        Style::default().fg(GREEN),
    )));

    if !app.warnings.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Warnings:",
            Style::default().fg(YELLOW),
        )));
        for w in &app.warnings {
            lines.push(Line::from(Span::styled(
                format!("  • {w}"),
                Style::default().fg(MUTED),
            )));
        }
    }
    // Tall enough for every line, with slack for warnings that wrap.
    let height = lines.len() as u16 + 2 + 2 * app.warnings.len() as u16;
    let rect = center(area, 82, height);
    f.render_widget(Clear, rect);
    let p = Paragraph::new(lines)
        .block(style::overlay(" help ").padding(ratatui::widgets::Padding::horizontal(1)))
        .wrap(Wrap { trim: true });
    f.render_widget(p, rect);
}

#[cfg(feature = "online")]
fn render_fetch(f: &mut Frame, area: Rect, app: &App) {
    let rect = center(area, 60, 22);
    f.render_widget(Clear, rect);

    // Still loading the list: show a spinner instead of an empty box.
    let Some(fetch) = &app.fetch else {
        let p = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                format!(
                    "   {}  fetching item list from GitHub…",
                    app.spinner_frame()
                ),
                Style::default().fg(ACCENT),
            )),
            Line::from(""),
            Line::from(Span::styled("   esc to cancel", Style::default().fg(MUTED))),
        ])
        .block(style::overlay(" fetch items "));
        f.render_widget(p, rect);
        return;
    };

    let title_prefix = match fetch.target {
        crate::app::FetchTarget::Themes => "fetch themes",
        crate::app::FetchTarget::Starship => "fetch starship presets",
    };

    let title = if app.fetching {
        format!(" {title_prefix}  {} downloading… ", app.spinner_frame())
    } else {
        format!(" {title_prefix} ({}) ", fetch.filtered.len())
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" filter: ", Style::default().fg(ACCENT)),
            Span::raw(fetch.filter.clone()),
            Span::styled("▏", Style::default().fg(ACCENT)),
        ]))
        .block(style::overlay(title)),
        rect,
    );

    let items: Vec<ListItem> = match fetch.target {
        crate::app::FetchTarget::Themes => fetch
            .filtered
            .iter()
            .filter_map(|&i| fetch.remotes.get(i))
            .map(|r| ListItem::new(Line::from(format!("  {}", r.name))))
            .collect(),
        crate::app::FetchTarget::Starship => fetch
            .filtered
            .iter()
            .filter_map(|&i| fetch.starship_remotes.get(i))
            .map(|r| ListItem::new(Line::from(format!("  {}", r.name))))
            .collect(),
    };

    let list = List::new(items)
        .highlight_style(style::selection())
        .highlight_symbol(CURSOR);
    let list_area = Rect {
        x: rect.x + 1,
        y: rect.y + 2,
        width: rect.width.saturating_sub(2),
        height: rect.height.saturating_sub(3),
    };
    let mut state = ListState::default();
    if !fetch.filtered.is_empty() {
        state.select(Some(fetch.selected));
    }
    f.render_stateful_widget(list, list_area, &mut state);
}

fn render_toast(f: &mut Frame, area: Rect, toast: &crate::app::Toast) {
    let (color, tag) = match toast.kind {
        ToastKind::Info => (MUTED, "·"),
        ToastKind::Success => (GREEN, "✓"),
        ToastKind::Error => (RED, "✗"),
    };
    // A message wider than the terminal wraps, and the box grows with it (up
    // to a few rows) instead of clipping the overflow: a release URL or an
    // error chain has to stay readable on an 80-column terminal.
    const MAX_ROWS: usize = 4;
    let prefix = format!(" {tag} ");
    let indent = " ".repeat(prefix.chars().count());
    // Box borders and a one-column margin each side; the prefix and a
    // trailing space sit inside that.
    let max_line = (area.width as usize).saturating_sub(4);
    let text_width = max_line.saturating_sub(prefix.chars().count() + 1).max(1);
    let mut wrapped = wrap_words(&toast.text, text_width);
    if wrapped.len() > MAX_ROWS {
        wrapped.truncate(MAX_ROWS);
        if let Some(last) = wrapped.last_mut() {
            if last.chars().count() >= text_width {
                last.pop();
            }
            last.push('…');
        }
    }
    let lines: Vec<Line> = wrapped
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let lead = if i == 0 { &prefix } else { &indent };
            Line::from(format!("{lead}{text} "))
        })
        .collect();
    let longest = lines.iter().map(Line::width).max().unwrap_or(0) as u16;
    let w = (longest + 2).min(area.width.saturating_sub(2));
    let h = (lines.len() as u16 + 2).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(w + 1),
        y: area.y + area.height.saturating_sub(h + 1),
        width: w,
        height: h,
    };
    f.render_widget(Clear, rect);
    f.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(color))
            .block(style::plain_panel().border_style(Style::default().fg(color))),
        rect,
    );
}

/// Greedy word wrap to `width` columns (counted in chars). A word longer than
/// the width — a URL, typically — is split so nothing is lost; empty input
/// yields one empty line.
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word = word;
        loop {
            let word_len = word.chars().count();
            if line.is_empty() && word_len > width {
                let split = word
                    .char_indices()
                    .nth(width)
                    .map(|(i, _)| i)
                    .unwrap_or(word.len());
                lines.push(word[..split].to_string());
                word = &word[split..];
                continue;
            }
            let needed = if line.is_empty() {
                word_len
            } else {
                line.chars().count() + 1 + word_len
            };
            if needed <= width {
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
                break;
            }
            lines.push(std::mem::take(&mut line));
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// A centered rectangle of the given size, clamped to the area.
fn center(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::wrap_words;

    #[test]
    fn wraps_at_word_boundaries() {
        assert_eq!(wrap_words("short", 20), ["short"]);
        assert_eq!(
            wrap_words("hauntty 1.1.0 is available (you have 1.0.0)", 24),
            ["hauntty 1.1.0 is", "available (you have", "1.0.0)"]
        );
    }

    #[test]
    fn splits_words_longer_than_the_width() {
        assert_eq!(
            wrap_words("https://example.invalid/abcdef", 10),
            ["https://ex", "ample.inva", "lid/abcdef"]
        );
        assert_eq!(
            wrap_words("a https://example.invalid/abc b", 12),
            ["a", "https://exam", "ple.invalid/", "abc b"]
        );
    }

    #[test]
    fn degenerate_inputs() {
        assert_eq!(wrap_words("", 10), [""]);
        assert_eq!(wrap_words("x y", 0), ["x", "y"]);
    }
}
