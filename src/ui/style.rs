//! Shared palette and widget styling, so every pane reads as one interface.

use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;

pub const ACCENT: Color = Color::Rgb(0xbb, 0x9a, 0xf7); // soft violet
pub const MUTED: Color = Color::Rgb(0x6a, 0x71, 0x89);
pub const TEXT: Color = Color::Rgb(0xc0, 0xca, 0xf5);
pub const GREEN: Color = Color::Rgb(0x9e, 0xce, 0x6a);
pub const YELLOW: Color = Color::Rgb(0xe0, 0xaf, 0x68);
pub const RED: Color = Color::Rgb(0xf7, 0x76, 0x8e);
pub const SELECT_BG: Color = Color::Rgb(0x2a, 0x2b, 0x3c);

/// Marker drawn beside the selected row of every list.
pub const CURSOR: &str = "▸ ";

fn bordered(color: Color) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
}

/// A quiet pane border with no title.
pub fn plain_panel() -> Block<'static> {
    bordered(MUTED)
}

/// A quiet pane border with a title.
pub fn panel<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
    bordered(MUTED).title(title)
}

/// An accented border for overlays and whatever currently has focus.
pub fn overlay<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
    bordered(ACCENT).title(title)
}

/// Style of the selected row in a list.
pub fn selection() -> Style {
    Style::default().bg(SELECT_BG).add_modifier(Modifier::BOLD)
}

/// A list title that shows the cursor position, e.g. ` themes 3/432 `.
pub fn list_title(name: &str, selected: usize, total: usize) -> String {
    if total == 0 {
        format!(" {name} (0) ")
    } else {
        format!(" {name} {}/{total} ", selected.min(total - 1) + 1)
    }
}

/// Draw a scrollbar on the right border of a bordered list, but only when the
/// list has more single-line rows than fit.
pub fn scrollbar(f: &mut Frame, area: Rect, total: usize, selected: usize) {
    let track = area.inner(Margin {
        vertical: 1,
        horizontal: 0,
    });
    if track.is_empty() || total <= track.height as usize {
        return;
    }
    let mut state = ScrollbarState::new(total).position(selected);
    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("┃")
            .track_style(Style::default().fg(MUTED))
            .thumb_style(Style::default().fg(ACCENT)),
        track,
        &mut state,
    );
}
