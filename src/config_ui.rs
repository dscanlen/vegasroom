use std::io::{self, IsTerminal, Write};

use anyhow::{Context, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::{
    config::{Config, RiskyMountPolicy, SshMode},
    paths::{display_path, StatePaths},
};

const SECTIONS: &[ConfigSection] = &[
    ConfigSection::Overview,
    ConfigSection::SecurityPreset,
    ConfigSection::Workspace,
    ConfigSection::Ssh,
    ConfigSection::GitIdentity,
    ConfigSection::RuntimeDocker,
    ConfigSection::OutputColor,
    ConfigSection::Advanced,
];

pub fn run() -> Result<i32> {
    let config = Config::load_or_default()?;
    let state_paths = StatePaths::default()?;

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        println!("Vegasroom configuration is interactive.");
        println!(
            "Run `vr config` from a terminal, or edit the config file manually: {}",
            display_path(&state_paths.config_yaml)
        );
        return Ok(0);
    }

    run_tui(config, state_paths)
}

fn run_tui(config: Config, state_paths: StatePaths) -> Result<i32> {
    let _terminal = TerminalSession::start()?;
    let mut state = ConfigUiState::new(config, state_paths);

    loop {
        render(&state)?;

        let Event::Key(key) = event::read().context("failed to read terminal key event")? else {
            continue;
        };

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => state.move_up(),
            KeyCode::Down | KeyCode::Char('j') => state.move_down(),
            KeyCode::Enter => state.open_highlighted(),
            KeyCode::Char('s') => state.save(),
            KeyCode::Char('q') => {
                if !state.dirty {
                    return Ok(0);
                }
                state.last_message = Some(
                    "Unsaved config changes are not editable yet in this first TUI slice."
                        .to_owned(),
                );
            }
            _ => {}
        }
    }
}

fn render(state: &ConfigUiState) -> Result<()> {
    let mut stdout = io::stdout();
    execute!(
        stdout,
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    )
    .context("failed to render config UI")?;

    writeln!(stdout, "Vegasroom Configuration")?;
    writeln!(
        stdout,
        "Config file: {}",
        display_path(&state.state_paths.config_yaml)
    )?;
    writeln!(
        stdout,
        "Status: {}",
        if state.dirty {
            "unsaved changes"
        } else {
            "saved"
        }
    )?;
    writeln!(stdout)?;

    writeln!(stdout, "Sections")?;
    for (index, section) in SECTIONS.iter().enumerate() {
        let marker = if index == state.highlighted { ">" } else { " " };
        writeln!(stdout, "{marker} {}", section.title())?;
    }

    writeln!(stdout)?;
    writeln!(stdout, "Details: {}", state.highlighted_section().title())?;
    for line in state
        .highlighted_section()
        .details(&state.config, &state.state_paths)
    {
        writeln!(stdout, "  {line}")?;
    }

    if let Some(message) = &state.last_message {
        writeln!(stdout)?;
        writeln!(stdout, "{message}")?;
    }

    writeln!(stdout)?;
    writeln!(
        stdout,
        "Keys: ↑/↓ or k/j move · Enter open section · s save · q quit"
    )?;
    writeln!(
        stdout,
        "Saving, dirty-state prompts, and editable submenus will follow the existing vr ssh configure pattern."
    )?;

    stdout.flush()?;
    Ok(())
}

struct TerminalSession;

impl TerminalSession {
    fn start() -> Result<Self> {
        terminal::enable_raw_mode().context("failed to enable terminal raw mode")?;
        execute!(io::stdout(), EnterAlternateScreen, cursor::Hide)
            .context("failed to enter terminal alternate screen")?;
        Ok(Self)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), cursor::Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

struct ConfigUiState {
    config: Config,
    state_paths: StatePaths,
    highlighted: usize,
    dirty: bool,
    last_message: Option<String>,
}

impl ConfigUiState {
    fn new(config: Config, state_paths: StatePaths) -> Self {
        Self {
            config,
            state_paths,
            highlighted: 0,
            dirty: false,
            last_message: None,
        }
    }

    fn highlighted_section(&self) -> ConfigSection {
        SECTIONS[self.highlighted]
    }

    fn move_up(&mut self) {
        if self.highlighted == 0 {
            self.highlighted = SECTIONS.len() - 1;
        } else {
            self.highlighted -= 1;
        }
        self.last_message = None;
    }

    fn move_down(&mut self) {
        self.highlighted = (self.highlighted + 1) % SECTIONS.len();
        self.last_message = None;
    }

    fn open_highlighted(&mut self) {
        self.last_message = Some(format!(
            "{} editing will be added as a submenu in an upcoming slice.",
            self.highlighted_section().title()
        ));
    }

    fn save(&mut self) {
        self.last_message = Some(if self.dirty {
            "Save support will be enabled with the first editable config submenu.".to_owned()
        } else {
            "No config changes to save.".to_owned()
        });
    }
}

#[derive(Clone, Copy)]
enum ConfigSection {
    Overview,
    SecurityPreset,
    Workspace,
    Ssh,
    GitIdentity,
    RuntimeDocker,
    OutputColor,
    Advanced,
}

impl ConfigSection {
    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::SecurityPreset => "Security preset",
            Self::Workspace => "Workspace",
            Self::Ssh => "SSH",
            Self::GitIdentity => "Git identity",
            Self::RuntimeDocker => "Runtime / Docker",
            Self::OutputColor => "Output / color",
            Self::Advanced => "Advanced",
        }
    }

    fn details(self, config: &Config, state_paths: &StatePaths) -> Vec<String> {
        match self {
            Self::Overview => vec![
                format!("Security preset: {}", security_preset_name(config)),
                format!(
                    "Workspace policy: {}",
                    risky_mount_policy_name(config.workspace.risky_mount_policy)
                ),
                format!("SSH mode: {}", ssh_mode_name(config.ssh.mode)),
                format!(
                    "Workspace read-only: {}",
                    config.harness.pi.read_only_workspace
                ),
                format!(
                    "Root filesystem read-only: {}",
                    config.harness.pi.read_only_rootfs
                ),
                format!("Runtime network: {}", config.harness.pi.network),
                "Use Enter to open a section. This first slice is read-only.".to_owned(),
            ],
            Self::SecurityPreset => vec![
                "Planned presets: Default / Compatible, Safer, Strict.".to_owned(),
                format!("Detected current preset: {}", security_preset_name(config)),
                "Presets will preview exact field changes before saving.".to_owned(),
            ],
            Self::Workspace => vec![
                format!("Default workspace root: {}", config.paths.workspace),
                format!(
                    "Risky mount policy: {}",
                    risky_mount_policy_name(config.workspace.risky_mount_policy)
                ),
                format!(
                    "Read-only workspace: {}",
                    config.harness.pi.read_only_workspace
                ),
                "Risky mount deny mode refuses broad mounts before Docker starts.".to_owned(),
            ],
            Self::Ssh => vec![
                format!("SSH mode: {}", ssh_mode_name(config.ssh.mode)),
                format!("Selected managed keys: {}", config.ssh.selected_keys.len()),
                "Key selection should reuse the existing vr ssh configure flow first.".to_owned(),
            ],
            Self::GitIdentity => vec![
                format!("Inherit host Git identity: {}", config.git.inherit_host),
                format!(
                    "Configured user.name: {}",
                    config.git.user_name.as_deref().unwrap_or("not set")
                ),
                format!(
                    "Configured user.email: {}",
                    config.git.user_email.as_deref().unwrap_or("not set")
                ),
                "Future UI should show the effective identity preview.".to_owned(),
            ],
            Self::RuntimeDocker => vec![
                format!("Docker context: {}", config.docker.context),
                format!("Compose file: {}", config.docker.compose_file),
                format!("Pi image: {}", config.harness.pi.image),
                format!("Pi command: {}", config.harness.pi.command),
                format!("Runtime network: {}", config.harness.pi.network),
                format!("Build network: {}", config.harness.pi.build_network),
                format!(
                    "Read-only root filesystem: {}",
                    config.harness.pi.read_only_rootfs
                ),
                "Bridge networking remains experimental for Pi login compatibility.".to_owned(),
            ],
            Self::OutputColor => vec![
                "Current behavior: color by default, non-empty NO_COLOR disables ANSI labels."
                    .to_owned(),
                "Planned config: ui.color = auto | always | never.".to_owned(),
                "TTY auto-detection and config-backed color policy are deferred here.".to_owned(),
            ],
            Self::Advanced => vec![
                format!("Config path: {}", display_path(&state_paths.config_yaml)),
                "Manual YAML editing remains supported.".to_owned(),
                "Future saves should create timestamped backups before writing.".to_owned(),
                "Future reset actions should preview changed fields first.".to_owned(),
            ],
        }
    }
}

fn security_preset_name(config: &Config) -> &'static str {
    if matches_default_compatible(config) {
        "Default / Compatible"
    } else if matches_safer(config) {
        "Safer"
    } else if matches_strict(config) {
        "Strict"
    } else {
        "Custom"
    }
}

fn matches_default_compatible(config: &Config) -> bool {
    config.workspace.risky_mount_policy == RiskyMountPolicy::Warn
        && !config.harness.pi.read_only_workspace
        && !config.harness.pi.read_only_rootfs
        && config.harness.pi.network == "host"
        && config.harness.pi.build_network == "host"
        && config.ssh.mode == SshMode::Auto
        && config.git.inherit_host
}

fn matches_safer(config: &Config) -> bool {
    config.workspace.risky_mount_policy == RiskyMountPolicy::Deny
        && !config.harness.pi.read_only_workspace
        && !config.harness.pi.read_only_rootfs
        && config.harness.pi.network == "host"
        && config.harness.pi.build_network == "host"
        && config.ssh.mode == SshMode::Auto
        && config.git.inherit_host
}

fn matches_strict(config: &Config) -> bool {
    config.workspace.risky_mount_policy == RiskyMountPolicy::Deny
        && config.harness.pi.read_only_workspace
        && config.harness.pi.read_only_rootfs
        && config.harness.pi.network == "host"
        && config.harness.pi.build_network == "host"
        && config.ssh.mode == SshMode::Managed
        && !config.git.inherit_host
}

fn risky_mount_policy_name(policy: RiskyMountPolicy) -> &'static str {
    match policy {
        RiskyMountPolicy::Warn => "warn",
        RiskyMountPolicy::Deny => "deny",
    }
}

fn ssh_mode_name(mode: SshMode) -> &'static str {
    match mode {
        SshMode::Auto => "auto",
        SshMode::Host => "host",
        SshMode::Managed => "managed",
        SshMode::Off => "off",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_default_compatible_preset() {
        let config = Config::default();

        assert_eq!(security_preset_name(&config), "Default / Compatible");
    }

    #[test]
    fn safer_preset_is_detected() {
        let mut config = Config::default();
        config.workspace.risky_mount_policy = RiskyMountPolicy::Deny;

        assert_eq!(security_preset_name(&config), "Safer");
    }

    #[test]
    fn strict_preset_is_detected() {
        let mut config = Config::default();
        config.workspace.risky_mount_policy = RiskyMountPolicy::Deny;
        config.harness.pi.read_only_workspace = true;
        config.harness.pi.read_only_rootfs = true;
        config.ssh.mode = SshMode::Managed;
        config.git.inherit_host = false;

        assert_eq!(security_preset_name(&config), "Strict");
    }
}
