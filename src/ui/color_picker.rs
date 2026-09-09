//! Keyboard-operated RGB picker with adjustment swatches and channel ramps.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use hauntty::theme::Rgb;

use super::{ACCENT, MUTED};
use crate::customize::{label, ColorPicker, Draft, PICKER_CONTROLS, PICKER_STEPS};

fn chip(text: String, color: Rgb) -> Span<'static> {
    Span::styled(
        text,
        Style::default()
            .bg(color.to_ratatui())
            .fg(color.contrast_text().to_ratatui()),
    )
}

pub fn render(f: &mut Frame, area: Rect, draft: &Draft, picker: &ColorPicker) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} · color picker ", label(draft.selected)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let rows = Layout::vertical([
        Constraint::Length(6),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .split(inner);

    let steps = PICKER_STEPS
        .iter()
        .enumerate()
        .map(|(i, (name, _))| {
            Span::styled(
                format!("{} {}  ", i + 1, name),
                if picker.step == i {
                    Style::default().fg(ACCENT).bold()
                } else {
                    Style::default().fg(MUTED)
                },
            )
        })
        .collect::<Vec<_>>();
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::raw("Color "),
                chip(format!(" {} ", picker.color.to_hex()), picker.color),
                Span::raw(format!("  was {}", {
                    let value = draft.value(draft.selected);
                    if value.is_empty() {
                        "default".into()
                    } else {
                        value
                    }
                })),
            ]),
            Line::from(if picker.uses_fallback {
                "Change gray to replace this value"
            } else {
                "Adjust with a live theme preview"
            })
            .fg(MUTED),
            Line::from(steps),
            Line::from(format!(
                "Step: {} / 255 per channel",
                PICKER_STEPS[picker.step].1
            ))
            .fg(MUTED),
            Line::from(vec![
                chip(" ← Down ".into(), picker.adjusted(-1)),
                Span::raw(" "),
                chip(" Current ".into(), picker.color),
                Span::raw(" "),
                chip(" Up → ".into(), picker.adjusted(1)),
            ]),
            Line::from(""),
        ]),
        rows[0],
    );

    let color = picker.color;
    let values = [
        ((u16::from(color.r) + u16::from(color.g) + u16::from(color.b)) / 3) as u8,
        color.r,
        color.g,
        color.b,
    ];
    let width = rows[1].width.saturating_sub(2).max(1);
    let items = PICKER_CONTROLS
        .iter()
        .enumerate()
        .map(|(control, name)| {
            let value = values[control];
            let marker = (u32::from(value) * u32::from(width - 1) + 127) / 255;
            let ramp = (0..width)
                .map(|x| {
                    let level = (u32::from(x) * 255 / u32::from((width - 1).max(1))) as u8;
                    let sample = match control {
                        0 => Rgb::new(level, level, level),
                        1 => Rgb::new(level, color.g, color.b),
                        2 => Rgb::new(color.r, level, color.b),
                        _ => Rgb::new(color.r, color.g, level),
                    };
                    chip(
                        if u32::from(x) == marker { "│" } else { " " }.into(),
                        sample,
                    )
                })
                .collect::<Vec<_>>();
            ListItem::new(vec![
                Line::from(if control == 0 {
                    "Brightness (all RGB channels)".into()
                } else {
                    format!("{name}  {value:>3} / 255")
                }),
                Line::from(ramp),
            ])
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(picker.control));
    f.render_stateful_widget(
        List::new(items)
            .highlight_symbol("› ")
            .highlight_style(Style::default().fg(ACCENT).bold()),
        rows[1],
        &mut state,
    );
    f.render_widget(
        Paragraph::new(
            "↑↓ control · ←/→ Down/Up\n1/2/3 or Tab step · r reset\nEnter keep · Esc cancel",
        )
        .fg(MUTED),
        rows[2],
    );
}
