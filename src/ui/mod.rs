//! Top-level TUI rendering.

mod preview;

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap};
use ratatui::Frame;

use hauntty::settings::Widget;
use hauntty::theme::Theme;

use crate::app::{App, Mode, Tab, ToastKind};

const ACCENT: Color = Color::Rgb(0xbb, 0x9a, 0xf7); // soft violet
const MUTED: Color = Color::Rgb(0x6a, 0x71, 0x89);

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
    }
    render_help_bar(f, chunks[2], app);

    // Overlays.
    match app.mode {
        Mode::Confirm => render_confirm(f, area, app),
        Mode::Input => render_input(f, area, app),
        Mode::Help => render_help_overlay(f, area, app),
        #[cfg(feature = "online")]
        Mode::Fetch => render_fetch(f, area, app),
        _ => {}
    }

    if let Some(toast) = &app.toast {
        render_toast(f, area, toast);
    }
}

fn render_tabs(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::horizontal([
        Constraint::Length(10),
        Constraint::Min(0),
        Constraint::Length(24),
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Span::styled(
            " hauntty ",
            Style::default().fg(Color::Black).bg(ACCENT).bold(),
        )),
        cols[0],
    );

    let titles = vec![Line::from("  Themes  "), Line::from("  Settings  ")];
    let selected = match app.tab {
        Tab::Themes => 0,
        Tab::Settings => 1,
    };
    let tabs = Tabs::new(titles)
        .select(selected)
        .style(Style::default().fg(MUTED))
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, cols[1]);

    let right = if app.dirty {
        Span::styled(
            "● unsaved changes ",
            Style::default().fg(Color::Rgb(0xe0, 0xaf, 0x68)),
        )
    } else if !app.warnings.is_empty() {
        Span::styled(
            format!("⚠ {} warning(s)  ?", app.warnings.len()),
            Style::default().fg(Color::Rgb(0xe0, 0xaf, 0x68)),
        )
    } else {
        Span::styled("", Style::default())
    };
    f.render_widget(Paragraph::new(right).alignment(Alignment::Right), cols[2]);
}

fn render_themes(f: &mut Frame, area: Rect, app: &App) {
    let cols =
        Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)]).split(area);

    // Left: filter + list.
    let left = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(cols[0]);
    let filter_line = if app.mode == Mode::Filter || !app.filter.is_empty() {
        Line::from(vec![
            Span::styled(" / ", Style::default().fg(ACCENT)),
            Span::raw(app.filter.clone()),
            if app.mode == Mode::Filter {
                Span::styled("▏", Style::default().fg(ACCENT))
            } else {
                Span::raw("")
            },
        ])
    } else {
        Line::from(Span::styled(
            " press / to filter themes",
            Style::default().fg(MUTED),
        ))
    };
    f.render_widget(Paragraph::new(filter_line), left[0]);

    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .filter_map(|&i| app.themes.ordered.get(i))
        .map(theme_list_item)
        .collect();

    let count = format!(" themes ({}) ", app.filtered.len());
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(MUTED))
                .title(count),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(0x2a, 0x2b, 0x3c))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");
    let mut state = ListState::default();
    if !app.filtered.is_empty() {
        state.select(Some(app.theme_selected));
    }
    f.render_stateful_widget(list, left[1], &mut state);

    // Right: live preview.
    match app.current_theme() {
        Some(theme) => preview::render(f, cols[1], theme),
        None => {
            let p = Paragraph::new("No themes found.\n\nCheck that Ghostty is installed, or press i to import a .itermcolors file.")
                .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(MUTED)))
                .wrap(Wrap { trim: true });
            f.render_widget(p, cols[1]);
        }
    }
}

fn theme_list_item(theme: &Theme) -> ListItem<'static> {
    let mut spans = vec![Span::raw(format!("{:<28}", truncate(&theme.name, 28)))];
    // mini swatch strip
    for i in [1usize, 2, 4, 5, 6, 3] {
        if let Some(c) = theme.palette[i] {
            spans.push(Span::styled(" ", Style::default().bg(c.to_ratatui())));
        }
    }
    if theme.source == hauntty::theme::ThemeSource::User {
        spans.push(Span::styled("  user", Style::default().fg(MUTED)));
    }
    ListItem::new(Line::from(spans))
}

fn render_settings(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).split(area);

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
            let value_style = if is_default {
                Style::default().fg(MUTED)
            } else {
                Style::default().fg(Color::Rgb(0x9e, 0xce, 0x6a))
            };
            let suffix = if is_default { "  (default)" } else { "" };
            ListItem::new(Line::from(vec![
                Span::raw(format!("  {:<24}", spec.label)),
                Span::styled(format!("{:<18}", widget_hint), value_style),
                Span::styled(suffix, Style::default().fg(MUTED)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(MUTED))
                .title(" settings "),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(0x2a, 0x2b, 0x3c))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");
    let mut state = ListState::default();
    state.select(Some(app.setting_selected));
    f.render_stateful_widget(list, rows[0], &mut state);

    // Help text for the selected setting.
    let help = app
        .settings
        .get(app.setting_selected)
        .map(|s| s.help)
        .unwrap_or("");
    f.render_widget(
        Paragraph::new(help)
            .style(Style::default().fg(MUTED))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(MUTED)),
            )
            .wrap(Wrap { trim: true }),
        rows[1],
    );
}

fn render_help_bar(f: &mut Frame, area: Rect, app: &App) {
    let keys: &[(&str, &str)] = match (app.tab, &app.mode) {
        (_, Mode::Filter) => &[("type", "filter"), ("↵/esc", "done")],
        (_, Mode::Confirm) => &[("y", "apply"), ("e", "edit name"), ("n/esc", "cancel")],
        (_, Mode::Input) => &[("type", "value"), ("↵", "set"), ("esc", "cancel")],
        (_, Mode::Help) => &[("esc", "close")],
        #[cfg(feature = "online")]
        (_, Mode::Fetch) => &[
            ("↑↓", "move"),
            ("type", "filter"),
            ("↵", "download"),
            ("esc", "close"),
        ],
        (Tab::Themes, _) => THEME_KEYS,
        (Tab::Settings, _) => SETTINGS_KEYS,
    };
    let mut spans = Vec::new();
    for (k, label) in keys {
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

#[cfg(feature = "online")]
const THEME_KEYS: &[(&str, &str)] = &[
    ("↑↓", "move"),
    ("/", "filter"),
    ("↵", "apply"),
    ("tab", "settings"),
    ("i", "import"),
    ("f", "fetch"),
    ("?", "help"),
    ("q", "quit"),
];
#[cfg(not(feature = "online"))]
const THEME_KEYS: &[(&str, &str)] = &[
    ("↑↓", "move"),
    ("/", "filter"),
    ("↵", "apply"),
    ("tab", "settings"),
    ("i", "import"),
    ("?", "help"),
    ("q", "quit"),
];

const SETTINGS_KEYS: &[(&str, &str)] = &[
    ("↑↓", "move"),
    ("←→", "change"),
    ("↵", "edit/toggle"),
    ("s", "save"),
    ("tab", "themes"),
    ("q", "quit"),
];

fn render_confirm(f: &mut Frame, area: Rect, app: &App) {
    let Some(c) = &app.confirm else { return };
    let rect = center(area, 62, if c.will_backup { 9 } else { 6 });
    f.render_widget(Clear, rect);

    let mut lines = vec![
        Line::from(Span::styled(
            format!("Apply theme “{}”?", c.theme_name),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    if c.will_backup {
        lines.push(Line::from(Span::styled(
            "Your current colors aren't from a named theme, so hauntty",
            Style::default().fg(MUTED),
        )));
        lines.push(Line::from(Span::styled(
            "will save them first as:",
            Style::default().fg(MUTED),
        )));
        let name_span = if c.editing_name {
            Span::styled(
                format!("  {}▏", c.backup_name),
                Style::default().fg(Color::Rgb(0x9e, 0xce, 0x6a)),
            )
        } else {
            Span::styled(
                format!("  {}   (press e to rename)", c.backup_name),
                Style::default().fg(Color::Rgb(0x9e, 0xce, 0x6a)),
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT))
                .title(" confirm "),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(p, rect);
}

fn render_input(f: &mut Frame, area: Rect, app: &App) {
    let Some(input) = &app.input else { return };
    let rect = center(area, 66, 5);
    f.render_widget(Clear, rect);
    let p = Paragraph::new(vec![
        Line::from(Span::styled(&*input.title, Style::default().fg(MUTED))),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::raw(input.buffer.clone()),
            Span::styled("▏", Style::default().fg(ACCENT)),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ACCENT))
            .title(" input "),
    );
    f.render_widget(p, rect);
}

fn render_help_overlay(f: &mut Frame, area: Rect, app: &App) {
    let rect = center(area, 70, 20);
    f.render_widget(Clear, rect);
    let mut lines = vec![
        Line::from(Span::styled(
            "hauntty — Ghostty theme & settings manager",
            Style::default().fg(ACCENT).bold(),
        )),
        Line::from(""),
        Line::from("Themes tab:  ↑↓ move · / filter · Enter apply · i import .itermcolors"),
        Line::from("Settings tab: ↑↓ move · ←→ change · Enter edit/toggle · s save"),
        Line::from("Tab switches panes · q quits · applying edits ~/.config/ghostty/config"),
        Line::from(""),
        Line::from(Span::styled(
            "After applying, reload Ghostty with ⌘⇧, (cmd+shift+,)",
            Style::default().fg(Color::Rgb(0x9e, 0xce, 0x6a)),
        )),
    ];
    if !app.warnings.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Warnings:",
            Style::default().fg(Color::Rgb(0xe0, 0xaf, 0x68)),
        )));
        for w in &app.warnings {
            lines.push(Line::from(Span::styled(
                format!("  • {w}"),
                Style::default().fg(MUTED),
            )));
        }
    }
    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT))
                .title(" help "),
        )
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
                    "   {}  fetching theme list from GitHub…",
                    app.spinner_frame()
                ),
                Style::default().fg(ACCENT),
            )),
            Line::from(""),
            Line::from(Span::styled("   esc to cancel", Style::default().fg(MUTED))),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT))
                .title(" fetch themes "),
        );
        f.render_widget(p, rect);
        return;
    };

    let title = if app.fetching {
        format!(" fetch themes  {} downloading… ", app.spinner_frame())
    } else {
        format!(" fetch themes ({}) ", fetch.filtered.len())
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" filter: ", Style::default().fg(ACCENT)),
            Span::raw(fetch.filter.clone()),
            Span::styled("▏", Style::default().fg(ACCENT)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT))
                .title(title),
        ),
        rect,
    );

    let items: Vec<ListItem> = fetch
        .filtered
        .iter()
        .filter_map(|&i| fetch.remotes.get(i))
        .map(|r| ListItem::new(Line::from(format!("  {}", r.name))))
        .collect();
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(0x2a, 0x2b, 0x3c))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");
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
        ToastKind::Success => (Color::Rgb(0x9e, 0xce, 0x6a), "✓"),
        ToastKind::Error => (Color::Rgb(0xf7, 0x76, 0x8e), "✗"),
    };
    let text = format!(" {tag} {} ", toast.text);
    let w = (text.chars().count() as u16 + 2).min(area.width.saturating_sub(2));
    let rect = Rect {
        x: area.x + area.width.saturating_sub(w + 1),
        y: area.y + area.height.saturating_sub(4),
        width: w,
        height: 3,
    };
    f.render_widget(Clear, rect);
    f.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(color))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(color)),
            )
            .wrap(Wrap { trim: true }),
        rect,
    );
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
