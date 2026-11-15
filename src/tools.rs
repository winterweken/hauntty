//! Curated terminal tools and reviewable package-manager installation plans.
//! Detection only inspects files; installers run after the TUI hands off the terminal.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

pub struct Tool {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub website: &'static str,
    pub setup: &'static str,
}

pub const CATALOG: &[Tool] = &[
    Tool { id: "lazydocker", name: "lazydocker", description: "Browse containers, logs, images, and Docker services.", website: "https://github.com/jesseduffield/lazydocker", setup: "Requires a running Docker engine. macOS: open Docker Desktop and finish its first-run setup. Linux: start your Docker service and configure user access using https://docs.docker.com/engine/install/linux-postinstall/. Compose is optional; install it separately for Compose projects." },
    Tool { id: "mc", name: "Midnight Commander", description: "A two-pane file manager with a built-in editor. Launch with mc.", website: "https://midnight-commander.org", setup: "Run mc. Use Tab to switch panes and F10 to quit." },
    Tool { id: "lazygit", name: "lazygit", description: "Stage, commit, and browse Git changes in a terminal UI.", website: "https://github.com/jesseduffield/lazygit", setup: "Run lazygit inside a Git repository. Git is included when missing." },
    Tool { id: "tmux", name: "tmux", description: "Keep terminal sessions alive, with windows and split panes.", website: "https://github.com/tmux/tmux", setup: "Run tmux. Ctrl-B then d detaches; tmux attach reconnects." },
    Tool { id: "fzf", name: "fzf", description: "Find files and filter lists interactively with fuzzy search.", website: "https://github.com/junegunn/fzf", setup: "Run fzf. Optional shell keybindings: https://github.com/junegunn/fzf#setting-up-shell-integration (commands depend on the installed version)." },
    Tool { id: "rg", name: "ripgrep", description: "Quickly search file contents, respecting Git ignore rules.", website: "https://github.com/BurntSushi/ripgrep", setup: "Run rg 'search text' in a project directory." },
    Tool { id: "bat", name: "bat", description: "Read files with syntax highlighting and Git change markers.", website: "https://github.com/sharkdp/bat", setup: "Run bat filename. Debian/Ubuntu may name the command batcat." },
    Tool { id: "zoxide", name: "zoxide", description: "Jump to frequently used directories with a smarter cd.", website: "https://github.com/ajeetdsouza/zoxide", setup: "Add to ~/.zshrc: eval \"$(zoxide init zsh)\". For bash, use eval \"$(zoxide init bash)\" in ~/.bashrc; for fish, zoxide init fish | source in config.fish. Restart your shell, then use z. Shell files are not changed automatically." },
    Tool { id: "git", name: "Git", description: "Version control for code and dotfiles; required by lazygit.", website: "https://git-scm.com", setup: "Before your first commit, set your Git user.name and user.email." },
    Tool { id: "docker", name: "Docker", description: "The container runtime used by lazydocker.", website: "https://docs.docker.com/get-started/", setup: "macOS: open Docker Desktop to complete setup and start the engine. Linux: start the Docker service and configure access: https://docs.docker.com/engine/install/linux-postinstall/. Detection checks installed files, not engine connectivity." },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Manager {
    Brew,
    Apt,
    Dnf,
    Pacman,
}

impl Manager {
    pub fn binary(self) -> &'static str {
        match self {
            Self::Brew => "brew",
            Self::Apt => "apt-get",
            Self::Dnf => "dnf",
            Self::Pacman => "pacman",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Host {
    pub macos: bool,
    pub home: Option<PathBuf>,
    pub binaries: BTreeMap<String, PathBuf>,
    pub docker_desktop: bool,
    pub remote_docker: bool,
}

fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<_> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".docker/bin"));
    }
    dirs.extend(
        [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/home/linuxbrew/.linuxbrew/bin",
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin",
        ]
        .map(PathBuf::from),
    );
    dirs
}

impl Host {
    pub fn detect() -> Self {
        let dirs = search_dirs();
        let mut binaries = BTreeMap::new();
        for name in CATALOG.iter().map(|t| t.id).chain([
            "brew", "apt-get", "dnf", "pacman", "go", "sudo", "batcat", "dockerd", "colima", "less",
        ]) {
            if let Some(path) = dirs.iter().map(|d| d.join(name)).find(|p| executable(p)) {
                binaries.insert(name.into(), path);
            }
        }
        Self {
            macos: cfg!(target_os = "macos"),
            home: dirs::home_dir(),
            binaries,
            docker_desktop: Path::new("/Applications/Docker.app").is_dir()
                || dirs::home_dir().is_some_and(|h| h.join("Applications/Docker.app").is_dir()),
            remote_docker: ["DOCKER_HOST", "DOCKER_CONTEXT"]
                .iter()
                .any(|key| std::env::var_os(key).is_some_and(|v| !v.is_empty())),
        }
    }

    pub fn has(&self, binary: &str) -> bool {
        self.binaries.contains_key(binary)
    }

    pub fn installed(&self, id: &str) -> bool {
        self.has(id)
            || (id == "bat" && self.has("batcat"))
            || (id == "docker" && self.macos && self.docker_desktop)
    }

    pub fn native_manager(&self) -> Option<Manager> {
        if self.macos {
            return None;
        }
        [Manager::Apt, Manager::Dnf, Manager::Pacman]
            .into_iter()
            .find(|m| self.has(m.binary()))
    }

    pub fn manager(&self) -> Option<Manager> {
        if self.has("brew") {
            Some(Manager::Brew)
        } else {
            self.native_manager()
        }
    }

    pub fn docker_available(&self) -> bool {
        if self.has("docker") && self.remote_docker {
            return true;
        }
        if self.macos {
            self.docker_desktop || (self.has("docker") && self.has("colima"))
        } else {
            self.has("docker") && self.has("dockerd")
        }
    }
}

#[derive(Clone, Debug)]
pub struct InstallCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
}

impl InstallCommand {
    fn new(program: impl Into<PathBuf>, args: &[&str]) -> Self {
        Self {
            program: program.into(),
            args: args.iter().map(OsString::from).collect(),
            env: Vec::new(),
        }
    }

    pub fn display(&self) -> String {
        fn quote(s: &std::ffi::OsStr) -> String {
            let s = s.to_string_lossy();
            if s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "/._-=:@".contains(c))
            {
                s.into_owned()
            } else {
                format!("'{}'", s.replace('\'', "'\\''"))
            }
        }
        self.env
            .iter()
            .map(|(k, v)| format!("{}={}", quote(k), quote(v)))
            .chain(std::iter::once(quote(self.program.as_os_str())))
            .chain(self.args.iter().map(|a| quote(a)))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn run(&self) -> Result<()> {
        let mut command = Command::new(&self.program);
        command.args(&self.args).envs(self.env.iter().cloned());
        // Include standard installation locations without editing the user's shell.
        command.env("PATH", std::env::join_paths(search_dirs())?);
        let status = command
            .status()
            .with_context(|| format!("starting {}", self.program.display()))?;
        if !status.success() {
            bail!(
                "{} exited with {status}; remaining steps were not run",
                self.program.display()
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct InstallPlan {
    pub title: String,
    pub commands: Vec<InstallCommand>,
    pub notes: Vec<String>,
}

fn packages(host: &Host, manager: Manager, names: &[&str]) -> Vec<InstallCommand> {
    if names.is_empty() {
        return Vec::new();
    }
    let program = host
        .binaries
        .get(manager.binary())
        .cloned()
        .unwrap_or_else(|| manager.binary().into());
    let mut command = InstallCommand::new(
        program,
        match manager {
            Manager::Brew => &["install"],
            Manager::Apt | Manager::Dnf => &["install"],
            Manager::Pacman => &["-S", "--needed"],
        },
    );
    command.args.extend(names.iter().map(OsString::from));
    let elevate = |mut command: InstallCommand| {
        if manager != Manager::Brew {
            if let Some(sudo) = host.binaries.get("sudo") {
                command.args.insert(0, command.program.into_os_string());
                command.program = sudo.clone();
            }
        }
        command
    };
    let mut commands = Vec::new();
    if manager == Manager::Apt {
        commands.push(elevate(InstallCommand::new(
            host.binaries
                .get("apt-get")
                .cloned()
                .unwrap_or_else(|| "apt-get".into()),
            &["update"],
        )));
    }
    commands.push(elevate(command));
    commands
}

pub fn plan(host: &Host, tool: &Tool) -> Result<InstallPlan> {
    let manager = host.manager().context("No supported package manager found. Press b to set up Homebrew, then retry. Linux also supports apt-get, dnf, and pacman.")?;
    let mut plan = InstallPlan { title: format!("Install {}", tool.name), commands: Vec::new(), notes: vec!["Package managers install required library dependencies automatically. Review their prompts before proceeding.".into(), tool.setup.into()] };
    if matches!(tool.id, "lazydocker" | "docker") && !host.docker_available() {
        if host.macos {
            let brew = host
                .binaries
                .get("brew")
                .context("Homebrew is required for Docker Desktop")?;
            plan.commands.push(InstallCommand::new(
                brew,
                &["install", "--cask", "docker-desktop"],
            ));
        } else {
            let native = host.native_manager().context("A native Linux package manager is needed to install the Docker engine. Install Docker using https://docs.docker.com/engine/install/, then retry.")?;
            let names: &[&str] = match native {
                Manager::Apt => &["docker.io"],
                Manager::Dnf => &["moby-engine", "docker-cli"],
                Manager::Pacman => &["docker"],
                Manager::Brew => unreachable!(),
            };
            plan.commands.extend(packages(host, native, names));
        }
    }
    if tool.id == "docker" {
        return Ok(plan);
    }
    let mut names = Vec::new();
    if tool.id == "lazygit" && !host.has("git") {
        names.push("git");
    }
    if tool.id == "bat" && !host.has("less") {
        names.push("less");
    }
    if !host.installed(tool.id) {
        if manager != Manager::Brew && matches!(tool.id, "lazydocker" | "lazygit") {
            if !host.has("go") {
                names.push(match manager {
                    Manager::Apt => "golang-go",
                    Manager::Dnf => "golang",
                    _ => "go",
                });
            }
            if !host.has("git") && !names.contains(&"git") {
                names.push("git");
            }
            names.push("ca-certificates");
            plan.commands.extend(packages(host, manager, &names));
            let bin = host
                .home
                .as_ref()
                .context("Could not find your home directory for the Go installation")?
                .join(".local/bin");
            let module = format!("github.com/jesseduffield/{}@latest", tool.id);
            let mut command = InstallCommand::new(
                host.binaries
                    .get("go")
                    .cloned()
                    .unwrap_or_else(|| "go".into()),
                &["install", &module],
            );
            command
                .env
                .push(("GOBIN".into(), bin.clone().into_os_string()));
            command.env.push(("CGO_ENABLED".into(), "0".into()));
            plan.commands.push(command);
            plan.notes.push(format!("Builds the upstream release with Go, which resolves its dependencies. Add {} to your shell PATH if needed.", bin.display()));
            return Ok(plan);
        }
        names.push(match tool.id {
            "mc" if manager == Manager::Brew => "midnight-commander",
            "rg" => "ripgrep",
            id => id,
        });
    }
    plan.commands.extend(packages(host, manager, &names));
    Ok(plan)
}

/// The official Homebrew bootstrap command, with curl failure propagated.
/// This fixed script contains no user input. It runs only after confirmation.
pub fn homebrew_plan() -> InstallPlan {
    InstallPlan { title: "Set up Homebrew".into(), commands: vec![InstallCommand::new("/bin/bash", &["-c", "script=$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh) || exit; /bin/bash -c \"$script\""])], notes: vec!["Downloads and runs the official Homebrew installer from https://brew.sh. It may request administrator access and install build tools.".into(), "Follow the installer's Next steps to add Homebrew to your shell, then return here and retry the tool installation.".into()] }
}

/// Stop at the first failure; inject the runner in tests so no software is installed.
pub fn run_plan(
    plan: &InstallPlan,
    mut run: impl FnMut(&InstallCommand) -> Result<()>,
) -> Result<()> {
    for command in &plan.commands {
        run(command)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(macos: bool, bins: &[&str]) -> Host {
        Host {
            macos,
            home: Some("/tmp/terminal user's home".into()),
            binaries: bins
                .iter()
                .map(|b| (b.to_string(), PathBuf::from(format!("/usr/bin/{b}"))))
                .collect(),
            docker_desktop: false,
            remote_docker: false,
        }
    }
    fn tool(id: &str) -> &'static Tool {
        CATALOG.iter().find(|t| t.id == id).unwrap()
    }
    fn commands(plan: &InstallPlan) -> Vec<String> {
        plan.commands.iter().map(InstallCommand::display).collect()
    }

    #[test]
    fn macos_lazydocker_includes_runtime_before_client() {
        let mut h = host(true, &["brew"]);
        let p = plan(&h, tool("lazydocker")).unwrap();
        assert_eq!(
            commands(&p),
            [
                "/usr/bin/brew install --cask docker-desktop",
                "/usr/bin/brew install lazydocker"
            ]
        );
        h.docker_desktop = true;
        assert_eq!(
            commands(&plan(&h, tool("lazydocker")).unwrap()),
            ["/usr/bin/brew install lazydocker"]
        );
        h.binaries
            .insert("lazydocker".into(), "/usr/bin/lazydocker".into());
        assert!(plan(&h, tool("lazydocker")).unwrap().commands.is_empty());
        h.docker_desktop = false;
        assert_eq!(
            commands(&plan(&h, tool("lazydocker")).unwrap()),
            ["/usr/bin/brew install --cask docker-desktop"]
        );
    }

    #[test]
    fn native_linux_resolves_engine_and_go_build_dependencies() {
        for (manager, engine, go) in [
            ("apt-get", "docker.io", "golang-go"),
            ("dnf", "moby-engine", "golang"),
            ("pacman", "docker", "go"),
        ] {
            let h = host(false, &[manager, "sudo"]);
            let p = plan(&h, tool("lazydocker")).unwrap();
            let text = commands(&p).join("\n");
            assert!(text.contains(engine), "{text}");
            assert!(text.contains(go), "{text}");
            assert!(text.contains("git ca-certificates"), "{text}");
            assert!(text.contains("/usr/bin/sudo"));
            let last = p.commands.last().unwrap();
            assert_eq!(last.program, Path::new("go"));
            assert_eq!(
                last.args,
                ["install", "github.com/jesseduffield/lazydocker@latest"]
            );
            assert_eq!(
                last.env[0],
                (
                    "GOBIN".into(),
                    "/tmp/terminal user's home/.local/bin".into()
                )
            );
            assert!(!last.display().contains("sudo"));
            if manager == "pacman" {
                assert!(
                    !text.contains("-Sy"),
                    "never perform a partial system upgrade"
                );
            }
        }
    }

    #[test]
    fn every_catalog_entry_has_a_plan_for_supported_managers() {
        for h in [
            host(true, &["brew"]),
            host(false, &["apt-get"]),
            host(false, &["dnf"]),
            host(false, &["pacman"]),
            host(false, &["brew", "apt-get"]),
        ] {
            for tool in CATALOG {
                let p = plan(&h, tool).unwrap();
                assert!(!p.commands.is_empty(), "{}", tool.id);
                assert!(!p.notes.is_empty());
            }
        }
        assert_eq!(
            commands(&plan(&host(true, &["brew"]), tool("mc")).unwrap()),
            ["/usr/bin/brew install midnight-commander"]
        );
        assert_eq!(
            commands(&plan(&host(false, &["dnf"]), tool("rg")).unwrap()),
            ["/usr/bin/dnf install ripgrep"]
        );
    }

    #[test]
    fn installed_tools_skip_reinstallation_but_missing_dependencies_are_added() {
        let h = host(true, &["brew", "lazygit"]);
        assert_eq!(
            commands(&plan(&h, tool("lazygit")).unwrap()),
            ["/usr/bin/brew install git"]
        );
        let h = host(false, &["apt-get", "batcat", "less"]);
        assert!(h.installed("bat"));
        assert!(plan(&h, tool("bat")).unwrap().commands.is_empty());
        let h = host(false, &["apt-get", "go", "git", "docker", "dockerd"]);
        let text = commands(&plan(&h, tool("lazydocker")).unwrap()).join("\n");
        assert!(!text.contains("golang-go"));
        assert!(!text.contains("docker.io"));
    }

    #[test]
    fn docker_cli_alone_does_not_satisfy_runtime_dependency() {
        let mut h = host(false, &["apt-get", "docker", "lazydocker"]);
        assert!(commands(&plan(&h, tool("lazydocker")).unwrap())
            .join("\n")
            .contains("docker.io"));
        h.remote_docker = true;
        assert!(plan(&h, tool("lazydocker")).unwrap().commands.is_empty());
    }

    #[test]
    fn unsupported_hosts_report_actionable_setup() {
        assert!(plan(&host(true, &[]), tool("mc"))
            .unwrap_err()
            .to_string()
            .contains("Homebrew"));
        assert!(plan(&host(false, &["brew"]), tool("lazydocker"))
            .unwrap_err()
            .to_string()
            .contains("Docker engine"));
        let mut h = host(false, &["apt-get"]);
        h.home = None;
        assert!(plan(&h, tool("lazygit")).is_err());
    }

    #[test]
    fn runner_stops_on_first_failure_without_running_remaining_steps() {
        let p = plan(&host(true, &["brew"]), tool("lazydocker")).unwrap();
        let mut ran = Vec::new();
        let result = run_plan(&p, |cmd| {
            ran.push(cmd.display());
            bail!("simulated cancellation")
        });
        assert!(result.is_err());
        assert_eq!(ran.len(), 1);
        let mut successes = 0;
        run_plan(&p, |_| {
            successes += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(successes, 2);
    }
}
