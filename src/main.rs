//! hauntty — a TUI theme & settings manager for the Ghostty terminal.

mod app;
mod customize;
mod event;
#[cfg(feature = "import-iterm")]
mod import_path;
mod tool_setup;
mod ui;

#[cfg(test)]
mod smoke_test;

use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
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
    // Install panic hook to guarantee terminal restoration on panic.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
        original_hook(panic_info);
    }));

    let mut terminal = setup_terminal().context("setting up terminal")?;
    let result = run_loop(&mut terminal, app);
    restore_terminal(&mut terminal).context("restoring terminal")?;
    result
}

fn setup_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    let result = (|| {
        execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
        Terminal::new(CrosstermBackend::new(stdout))
    })();
    if result.is_err() {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
    Ok(result?)
}

fn restore_terminal(terminal: &mut Tui) -> Result<()> {
    let raw = disable_raw_mode();
    let screen = execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        LeaveAlternateScreen
    );
    let cursor = terminal.show_cursor();
    raw?;
    screen?;
    cursor?;
    Ok(())
}

fn run_loop(terminal: &mut Tui, app: &mut App) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|f| ui::render(f, app))?;
        // A short poll keeps the spinner animating and surfaces background
        // network results promptly, without busy-waiting.
        if crossterm::event::poll(Duration::from_millis(120))? {
            event::handle_event(app, crossterm::event::read()?);
        }
        app.poll_background();
        app.poll_starship_install();
        app.tick();
        if let Some(plan) = app.tools.pending.take() {
            run_tool_installer(terminal, app, &plan)?;
        }
    }
    Ok(())
}

fn run_tool_installer(
    terminal: &mut Tui,
    app: &mut App,
    plan: &hauntty::tools::InstallPlan,
) -> Result<()> {
    use std::io::Write;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    // Ctrl-C reaches the foreground installer; retain unsaved settings and
    // return to the TUI when the installer is interrupted.
    struct InterruptGuard(signal_hook::SigId);
    impl Drop for InterruptGuard {
        fn drop(&mut self) {
            signal_hook::low_level::unregister(self.0);
        }
    }
    let interrupted = Arc::new(AtomicBool::new(false));
    let _guard = InterruptGuard(signal_hook::flag::register(
        signal_hook::consts::SIGINT,
        interrupted.clone(),
    )?);
    restore_terminal(terminal)?;
    println!("\nhauntty — {}\n", plan.title);
    let result = hauntty::tools::run_plan(plan, |command| {
        if interrupted.load(Ordering::Relaxed) {
            anyhow::bail!("Installation interrupted");
        }
        println!("\n$ {}\n", command.display());
        command.run()?;
        if interrupted.load(Ordering::Relaxed) {
            anyhow::bail!("Installation interrupted");
        }
        Ok(())
    });
    match &result {
        Ok(()) => println!("\nInstallation commands completed."),
        Err(e) => println!("\nInstallation stopped: {e:#}"),
    }
    for note in &plan.notes {
        println!("\n{note}");
    }
    print!("\nPress Enter to return to hauntty...");
    let _ = io::stdout().flush();
    let _ = io::stdin().read_line(&mut String::new());
    *terminal = setup_terminal().context("returning to hauntty after installation")?;
    app.finish_tool_install(result);
    Ok(())
}
