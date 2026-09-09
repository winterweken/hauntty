use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use super::{ACCENT, MUTED};
use crate::app::App;
use hauntty::tools::{self, CATALOG};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let host = &app.tools.host;
    let rows = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).split(area);
    let manager = host
        .manager()
        .map(|m| format!("Installer: {} · required libraries included", m.binary()))
        .unwrap_or_else(|| "No package manager found · b to set up Homebrew".into());
    f.render_widget(
        Paragraph::new(manager).fg(MUTED).wrap(Wrap { trim: false }),
        rows[0],
    );
    let cols =
        Layout::horizontal([Constraint::Percentage(37), Constraint::Percentage(63)]).split(rows[1]);
    let items: Vec<ListItem> = CATALOG
        .iter()
        .map(|tool| {
            let status = if host.installed(tool.id) {
                "installed"
            } else {
                "not installed"
            };
            ListItem::new(vec![
                Line::from(tool.name),
                Line::from(format!("  {status}")).fg(MUTED),
            ])
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.tools.selected));
    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" terminal tools "),
            )
            .highlight_symbol("› ")
            .highlight_style(Style::default().fg(ACCENT).bold()),
        cols[0],
        &mut state,
    );
    let tool = &CATALOG[app.tools.selected];
    let mut lines = vec![
        Line::from(tool.description),
        Line::from(""),
        Line::from(tool.website).fg(MUTED),
        Line::from(""),
    ];
    if let Some(path) = host.binaries.get(tool.id).or_else(|| {
        if tool.id == "bat" {
            host.binaries.get("batcat")
        } else {
            None
        }
    }) {
        lines.push(Line::from(format!("Found: {}", path.display())));
        lines.push(Line::from(""));
    }
    lines.push(Line::from("Install plan").fg(ACCENT).bold());
    match tools::plan(host, tool) {
        Ok(plan) => {
            if plan.commands.is_empty() {
                lines.push(Line::from("Already installed; see setup notes below."));
            }
            for command in &plan.commands {
                lines.push(Line::from(format!("$ {}", command.display())));
            }
            lines.push(Line::from(""));
            for note in &plan.notes {
                lines.push(Line::from(note.clone()));
                lines.push(Line::from(""));
            }
        }
        Err(e) => {
            lines.push(Line::from(format!("{e:#}")));
            lines.push(Line::from(""));
            lines.push(Line::from(tool.setup));
        }
    }
    lines.push(Line::from("Installed means a program was found; services and shell integration may still need setup.").fg(MUTED));
    if let Some(result) = &app.tools.last_result {
        lines.push(Line::from(""));
        lines.push(Line::from(result.clone()));
    }
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.tools.scroll, 0))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" details · PgUp/PgDn to scroll "),
            ),
        cols[1],
    );
}

pub fn render_confirmation(f: &mut Frame, area: Rect, app: &App) {
    let Some(plan) = &app.tools.confirmation else {
        return;
    };
    let rect = super::center(area, 90, 26);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(format!(" {} ", plan.title));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).split(inner);
    let mut lines = vec![
        Line::from("The installer opens in the normal terminal for progress and password prompts."),
        Line::from(""),
    ];
    for command in &plan.commands {
        lines.push(Line::from(format!("$ {}", command.display())));
        lines.push(Line::from(""));
    }
    for note in &plan.notes {
        lines.push(Line::from(note.clone()));
        lines.push(Line::from(""));
    }
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.tools.scroll, 0)),
        rows[0],
    );
    f.render_widget(
        Paragraph::new(
            "y install · n / Esc cancel · ↑↓ scroll\nReturn here when the installer finishes.",
        )
        .fg(ACCENT),
        rows[1],
    );
}
