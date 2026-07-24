//! hauntty — a TUI theme & settings manager for the Ghostty terminal.

mod app;
mod event;
mod ui;

#[cfg(test)]
mod smoke_test;

use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{App, ToastKind};
use hauntty::paths::Paths;

type Tui = Terminal<CrosstermBackend<Stdout>>;

struct Args {
    config: Option<PathBuf>,
    themes_dir: Option<PathBuf>,
    help: bool,
    version: bool,
}

fn parse_args() -> Result<Args> {
    let mut args = Args {
        config: None,
        themes_dir: None,
        help: false,
        version: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => args.help = true,
            "-V" | "--version" => args.version = true,
            "--config" => {
                args.config = Some(PathBuf::from(it.next().context("--config needs a path")?));
            }
            "--themes-dir" => {
                args.themes_dir = Some(PathBuf::from(
                    it.next().context("--themes-dir needs a path")?,
                ));
            }
            other => anyhow::bail!("unknown argument: {other} (try --help)"),
        }
    }
    Ok(args)
}

fn print_help() {
    println!(
        "hauntty {} — a TUI theme & settings manager for Ghostty\n\n\
USAGE:\n    hauntty [OPTIONS]\n\n\
OPTIONS:\n    \
--config <PATH>        Use a specific Ghostty config file\n    \
--themes-dir <PATH>    Add a directory to search for themes\n    \
-h, --help             Show this help\n    \
-V, --version          Show version\n\n\
ENVIRONMENT:\n    \
HAUNTTY_CONFIG         Default config path\n    \
GHOSTTY_RESOURCES_DIR  Ghostty resources dir (its /themes is searched)\n\n\
Applying a theme or saving settings edits your Ghostty config in place\n\
(a timestamped backup is written first). Reload Ghostty with cmd+shift+,.",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() -> Result<()> {
    let args = parse_args()?;
    if args.help {
        print_help();
        return Ok(());
    }
    if args.version {
        println!("hauntty {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let paths = Paths::resolve(args.config, args.themes_dir);
    let mut app = App::new(paths).context("initializing hauntty")?;
    if !app.warnings.is_empty() {
        app.toast(
            ToastKind::Info,
            format!("{} warning(s) — press ? for details", app.warnings.len()),
        );
    }

    run(&mut app)
}

fn run(app: &mut App) -> Result<()> {
    let mut terminal = setup_terminal().context("setting up terminal")?;
    let result = run_loop(&mut terminal, app);
    restore_terminal(&mut terminal).context("restoring terminal")?;
    result
}

fn setup_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_loop(terminal: &mut Tui, app: &mut App) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|f| ui::render(f, app))?;
        if crossterm::event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = crossterm::event::read()? {
                if key.kind == KeyEventKind::Press {
                    event::handle_key(app, key);
                }
            }
        }
    }
    Ok(())
}
