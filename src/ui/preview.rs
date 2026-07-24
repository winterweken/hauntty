//! Renders a live, truecolor "sample terminal" for a theme.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use hauntty::theme::{Rgb, Theme};

fn span(text: impl Into<String>, fg: Rgb, bg: Rgb) -> Span<'static> {
    Span::styled(
        text.into(),
        Style::default().fg(fg.to_ratatui()).bg(bg.to_ratatui()),
    )
}

/// Render the preview of `theme` into `area`.
pub fn render(f: &mut Frame, area: Rect, theme: &Theme) {
    let bg = theme.bg();
    let fg = theme.fg();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} · {} ", theme.name, theme.source.label()))
        .border_style(Style::default().fg(fg.to_ratatui()))
        .style(Style::default().bg(bg.to_ratatui()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if !theme.is_renderable() {
        let p = Paragraph::new(Line::from(Span::styled(
            "  (could not parse this theme's colors — it can still be applied)",
            Style::default().fg(fg.to_ratatui()).bg(bg.to_ratatui()),
        )))
        .style(Style::default().bg(bg.to_ratatui()));
        f.render_widget(p, inner);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    // ANSI palette, two rows of 8.
    for row in 0..2 {
        let mut spans = vec![span("  ", fg, bg)];
        for col in 0..8 {
            let i = row * 8 + col;
            let c = theme.ansi(i);
            spans.push(span(format!("{i:>2} "), c.contrast_text(), c));
            spans.push(span(" ", fg, bg));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(span("", fg, bg)));

    // Special colors row.
    let mut specials = vec![span("  ", fg, bg)];
    let chip = |spans: &mut Vec<Span<'static>>, label: &str, c: Rgb| {
        spans.push(span(format!(" {label} "), c.contrast_text(), c));
        spans.push(span(" ", fg, bg));
    };
    chip(&mut specials, "bg", bg);
    chip(&mut specials, "fg", fg);
    chip(&mut specials, "cursor", theme.cursor_color.unwrap_or(fg));
    chip(
        &mut specials,
        "select",
        theme.selection_background.unwrap_or(theme.ansi(8)),
    );
    lines.push(Line::from(specials));
    lines.push(Line::from(span("", fg, bg)));

    // A fake prompt line, with a block cursor.
    let blue = theme.ansi(4);
    let green = theme.ansi(2);
    let cursor = theme.cursor_color.unwrap_or(fg);
    lines.push(Line::from(vec![
        span("  ", fg, bg),
        span("~/code/hauntty", blue, bg),
        span(" ❯ ", green, bg),
        span("cargo run", fg, bg),
        span(" ", theme.cursor_text.unwrap_or(bg), cursor),
    ]));
    lines.push(Line::from(span("", fg, bg)));

    // A small syntax-highlighted code sample.
    let kw = theme.ansi(5); // magenta
    let string = theme.ansi(2); // green
    let comment = theme.ansi(8); // bright black
    let ty = theme.ansi(3); // yellow
    let num = theme.ansi(4); // blue
    let code: Vec<Vec<Span>> = vec![
        vec![
            span("  ", fg, bg),
            span("fn ", kw, bg),
            span("greet", ty, bg),
            span("(name: ", fg, bg),
            span("&str", ty, bg),
            span(") {", fg, bg),
        ],
        vec![
            span("      ", fg, bg),
            span("let ", kw, bg),
            span("n", fg, bg),
            span(" = ", fg, bg),
            span("42", num, bg),
            span(";", fg, bg),
            span("  // count", comment, bg),
        ],
        vec![
            span("      ", fg, bg),
            span("println!", ty, bg),
            span("(", fg, bg),
            span("\"hi {name}\"", string, bg),
            span(");", fg, bg),
        ],
        vec![span("  }", fg, bg)],
    ];
    for row in code {
        lines.push(Line::from(row));
    }
    lines.push(Line::from(span("", fg, bg)));

    // Selection legibility demo.
    let sel_bg = theme.selection_background.unwrap_or(theme.ansi(8));
    let sel_fg = theme.selection_foreground.unwrap_or(fg);
    lines.push(Line::from(vec![
        span("  ", fg, bg),
        span("selected text", sel_fg, sel_bg),
        span("  looks like this", fg, bg),
    ]));

    // Bold/dim sample.
    lines.push(Line::from(vec![
        span("  ", fg, bg),
        Span::styled(
            "bold",
            Style::default()
                .fg(fg.to_ratatui())
                .bg(bg.to_ratatui())
                .add_modifier(Modifier::BOLD),
        ),
        span("  ", fg, bg),
        Span::styled(
            "dim",
            Style::default()
                .fg(theme.ansi(8).to_ratatui())
                .bg(bg.to_ratatui()),
        ),
    ]));

    let para =
        Paragraph::new(lines).style(Style::default().bg(bg.to_ratatui()).fg(fg.to_ratatui()));
    f.render_widget(
        FillBg {
            color: bg.to_ratatui(),
        },
        inner,
    );
    f.render_widget(para, inner);
}

/// A widget that fills its area with a solid background color (so the preview
/// pane reads as a real terminal even where text is short).
struct FillBg {
    color: Color,
}

impl ratatui::widgets::Widget for FillBg {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_bg(self.color).set_char(' ');
            }
        }
    }
}
