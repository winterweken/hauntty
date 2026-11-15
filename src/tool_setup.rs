//! Tools tab state and the explicit handoff to an interactive installer.

use crate::app::{App, Mode, ToastKind};
use hauntty::tools::{self, Host, InstallPlan, CATALOG};

pub struct ToolsState {
    pub host: Host,
    pub selected: usize,
    pub confirmation: Option<InstallPlan>,
    pub pending: Option<InstallPlan>,
    pub scroll: u16,
    pub last_result: Option<String>,
}

impl ToolsState {
    pub fn new() -> Self {
        Self {
            host: Host::detect(),
            selected: 0,
            confirmation: None,
            pending: None,
            scroll: 0,
            last_result: None,
        }
    }
}

impl App {
    pub fn review_tool_install(&mut self) {
        self.tools.host = Host::detect();
        match tools::plan(&self.tools.host, &CATALOG[self.tools.selected]) {
            Ok(plan) if plan.commands.is_empty() => self.toast(
                ToastKind::Info,
                "Already installed. See the setup notes for how to get started.",
            ),
            Ok(plan) => {
                self.tools.confirmation = Some(plan);
                self.tools.scroll = 0;
                self.mode = Mode::ToolInstall;
            }
            Err(e) => self.toast(ToastKind::Error, format!("{e:#}")),
        }
    }

    pub fn review_homebrew_install(&mut self) {
        self.tools.host = Host::detect();
        if self.tools.host.has("brew") {
            self.toast(ToastKind::Info, "Homebrew is already installed.");
            return;
        }
        self.tools.confirmation = Some(tools::homebrew_plan());
        self.tools.scroll = 0;
        self.mode = Mode::ToolInstall;
    }

    pub fn confirm_tool_install(&mut self) {
        if self.starship_install_running() {
            self.toast(
                ToastKind::Info,
                "Wait for the Starship installation to finish, then retry.",
            );
            return;
        }
        self.tools.pending = self.tools.confirmation.take();
        self.mode = Mode::Normal;
    }

    pub fn finish_tool_install(&mut self, result: anyhow::Result<()>) {
        self.tools.host = Host::detect();
        let (kind, message) = match result {
            Ok(()) => (ToastKind::Success, "Installation commands completed. Check setup notes; a new shell or Docker startup may still be needed.".to_string()),
            Err(e) => (ToastKind::Error, format!("Installation stopped: {e:#}. Completed steps remain installed; refresh and retry when ready.")),
        };
        self.tools.last_result = Some(message.clone());
        self.toast(kind, message);
    }
}
