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
            KeyCode::Esc | KeyCode::Backspace => state.go_back(),
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

    render_header(&mut stdout, state)?;

    match state.screen {
        ConfigScreen::Sections => render_sections_screen(&mut stdout, state)?,
        ConfigScreen::Section(section) => render_section_screen(&mut stdout, state, section)?,
    }

    if let Some(message) = &state.last_message {
        writeln!(stdout)?;
        writeln!(stdout, "{message}")?;
    }

    writeln!(stdout)?;
    render_keys(&mut stdout, state)?;

    stdout.flush()?;
    Ok(())
}

fn render_header(stdout: &mut impl Write, state: &ConfigUiState) -> Result<()> {
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
    Ok(())
}

fn render_sections_screen(stdout: &mut impl Write, state: &ConfigUiState) -> Result<()> {
    writeln!(stdout, "Sections")?;
    for (index, section) in SECTIONS.iter().enumerate() {
        let marker = if index == state.highlighted_section {
            ">"
        } else {
            " "
        };
        writeln!(stdout, "{marker} {}", section.title())?;
    }

    let section = state.highlighted_section();
    writeln!(stdout)?;
    writeln!(stdout, "Details: {}", section.title())?;
    for line in section.summary(&state.config) {
        writeln!(stdout, "  {line}")?;
    }

    Ok(())
}

fn render_section_screen(
    stdout: &mut impl Write,
    state: &ConfigUiState,
    section: ConfigSection,
) -> Result<()> {
    writeln!(stdout, "Section: {}", section.title())?;
    let rows = section.rows(&state.config, &state.state_paths);
    for (index, row) in rows.iter().enumerate() {
        let marker = if index == state.highlighted_row {
            ">"
        } else {
            " "
        };
        writeln!(stdout, "{marker} {}", row.title)?;
    }

    if let Some(row) = rows.get(state.highlighted_row) {
        writeln!(stdout)?;
        writeln!(stdout, "Details: {}", row.title)?;
        for line in &row.details {
            writeln!(stdout, "  {line}")?;
        }
    }

    Ok(())
}

fn render_keys(stdout: &mut impl Write, state: &ConfigUiState) -> Result<()> {
    match state.screen {
        ConfigScreen::Sections => writeln!(
            stdout,
            "Keys: ↑/↓ or k/j move · Enter open section · s save · q quit"
        )?,
        ConfigScreen::Section(_) => writeln!(
            stdout,
            "Keys: ↑/↓ or k/j move · Enter edit/toggle later · Esc/Backspace back · s save · q quit"
        )?,
    }

    writeln!(
        stdout,
        "Saving and dirty-state prompts will follow the existing vr ssh configure pattern."
    )?;
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
    screen: ConfigScreen,
    highlighted_section: usize,
    highlighted_row: usize,
    dirty: bool,
    last_message: Option<String>,
}

impl ConfigUiState {
    fn new(config: Config, state_paths: StatePaths) -> Self {
        Self {
            config,
            state_paths,
            screen: ConfigScreen::Sections,
            highlighted_section: 0,
            highlighted_row: 0,
            dirty: false,
            last_message: None,
        }
    }

    fn highlighted_section(&self) -> ConfigSection {
        SECTIONS[self.highlighted_section]
    }

    fn move_up(&mut self) {
        match self.screen {
            ConfigScreen::Sections => {
                if self.highlighted_section == 0 {
                    self.highlighted_section = SECTIONS.len() - 1;
                } else {
                    self.highlighted_section -= 1;
                }
            }
            ConfigScreen::Section(section) => {
                let len = section.rows(&self.config, &self.state_paths).len();
                if len == 0 {
                    return;
                }
                if self.highlighted_row == 0 {
                    self.highlighted_row = len - 1;
                } else {
                    self.highlighted_row -= 1;
                }
            }
        }
        self.last_message = None;
    }

    fn move_down(&mut self) {
        match self.screen {
            ConfigScreen::Sections => {
                self.highlighted_section = (self.highlighted_section + 1) % SECTIONS.len();
            }
            ConfigScreen::Section(section) => {
                let len = section.rows(&self.config, &self.state_paths).len();
                if len == 0 {
                    return;
                }
                self.highlighted_row = (self.highlighted_row + 1) % len;
            }
        }
        self.last_message = None;
    }

    fn open_highlighted(&mut self) {
        match self.screen {
            ConfigScreen::Sections => {
                self.screen = ConfigScreen::Section(self.highlighted_section());
                self.highlighted_row = 0;
                self.last_message = None;
            }
            ConfigScreen::Section(section) => {
                let rows = section.rows(&self.config, &self.state_paths);
                if let Some(row) = rows.get(self.highlighted_row) {
                    self.last_message = Some(format!(
                        "{} editing will be added in an upcoming slice.",
                        row.title
                    ));
                }
            }
        }
    }

    fn go_back(&mut self) {
        if matches!(self.screen, ConfigScreen::Section(_)) {
            self.screen = ConfigScreen::Sections;
            self.highlighted_row = 0;
            self.last_message = None;
        }
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
enum ConfigScreen {
    Sections,
    Section(ConfigSection),
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

    fn summary(self, config: &Config) -> Vec<String> {
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
            ],
            Self::SecurityPreset => vec![
                "Choose a security posture without memorizing YAML fields.".to_owned(),
                format!("Detected current preset: {}", security_preset_name(config)),
            ],
            Self::Workspace => vec![
                "Configure default workspace location and risky mount behavior.".to_owned(),
                format!(
                    "Current risky mount policy: {}",
                    risky_mount_policy_name(config.workspace.risky_mount_policy)
                ),
            ],
            Self::Ssh => vec![
                "Configure SSH agent behavior and selected managed keys.".to_owned(),
                format!("Current SSH mode: {}", ssh_mode_name(config.ssh.mode)),
            ],
            Self::GitIdentity => vec![
                "Configure the Git author identity injected into the room.".to_owned(),
                format!("Inherit host identity: {}", config.git.inherit_host),
            ],
            Self::RuntimeDocker => vec![
                "Configure Docker context, Compose runtime, image, command, and hardening."
                    .to_owned(),
                format!("Runtime network: {}", config.harness.pi.network),
            ],
            Self::OutputColor => vec![
                "Configure output color behavior alongside remaining color polish.".to_owned(),
                "Current behavior: color by default; non-empty NO_COLOR disables labels."
                    .to_owned(),
            ],
            Self::Advanced => vec![
                "Inspect config path and future reset/backup actions.".to_owned(),
                "Manual YAML editing remains supported.".to_owned(),
            ],
        }
    }

    fn rows(self, config: &Config, state_paths: &StatePaths) -> Vec<SectionRow> {
        match self {
            Self::Overview => vec![
                SectionRow::new(
                    "Security preset",
                    vec![format!("Current: {}", security_preset_name(config))],
                ),
                SectionRow::new(
                    "Workspace policy",
                    vec![format!(
                        "Current: {}",
                        risky_mount_policy_name(config.workspace.risky_mount_policy)
                    )],
                ),
                SectionRow::new(
                    "SSH mode",
                    vec![format!("Current: {}", ssh_mode_name(config.ssh.mode))],
                ),
                SectionRow::new(
                    "Runtime hardening",
                    vec![
                        format!(
                            "Workspace read-only: {}",
                            config.harness.pi.read_only_workspace
                        ),
                        format!(
                            "Root filesystem read-only: {}",
                            config.harness.pi.read_only_rootfs
                        ),
                    ],
                ),
            ],
            Self::SecurityPreset => vec![
                SectionRow::new(
                    "Current preset",
                    vec![format!("Detected: {}", security_preset_name(config))],
                ),
                SectionRow::new(
                    "Default / Compatible",
                    vec![
                        "Preserves current proven behavior and maximum compatibility.".to_owned(),
                        "Alias: lowsec/default.".to_owned(),
                    ],
                ),
                SectionRow::new(
                    "Safer",
                    vec![
                        "Sets risky workspace mount policy to deny.".to_owned(),
                        "Keeps workspace writes, host networking, and automatic SSH behavior."
                            .to_owned(),
                    ],
                ),
                SectionRow::new(
                    "Strict",
                    vec![
                        "Enables deny policy, read-only workspace, read-only rootfs, managed SSH, and no host Git inheritance.".to_owned(),
                        "May reduce editing, Git, login, or shell compatibility.".to_owned(),
                    ],
                ),
            ],
            Self::Workspace => vec![
                SectionRow::new(
                    "Default workspace root",
                    vec![format!("Current: {}", config.paths.workspace)],
                ),
                SectionRow::new(
                    "Risky mount policy",
                    vec![
                        format!(
                            "Current: {}",
                            risky_mount_policy_name(config.workspace.risky_mount_policy)
                        ),
                        "warn prints a warning and continues; deny refuses broad risky mounts."
                            .to_owned(),
                    ],
                ),
                SectionRow::new(
                    "Read-only workspace",
                    vec![
                        format!("Current: {}", config.harness.pi.read_only_workspace),
                        "When enabled, Pi may not be able to edit project files.".to_owned(),
                    ],
                ),
            ],
            Self::Ssh => vec![
                SectionRow::new(
                    "SSH mode",
                    vec![
                        format!("Current: {}", ssh_mode_name(config.ssh.mode)),
                        "auto, host, managed, or off.".to_owned(),
                    ],
                ),
                SectionRow::new(
                    "Selected managed SSH keys",
                    vec![
                        format!("Current selected keys: {}", config.ssh.selected_keys.len()),
                        "The first editable version should reuse the existing SSH configure flow."
                            .to_owned(),
                    ],
                ),
            ],
            Self::GitIdentity => vec![
                SectionRow::new(
                    "Inherit host Git identity",
                    vec![format!("Current: {}", config.git.inherit_host)],
                ),
                SectionRow::new(
                    "Configured user.name",
                    vec![format!(
                        "Current: {}",
                        config.git.user_name.as_deref().unwrap_or("not set")
                    )],
                ),
                SectionRow::new(
                    "Configured user.email",
                    vec![format!(
                        "Current: {}",
                        config.git.user_email.as_deref().unwrap_or("not set")
                    )],
                ),
                SectionRow::new(
                    "Effective identity preview",
                    vec![
                        "Future UI should show top-level, selected-key, host-inherited, or none."
                            .to_owned(),
                    ],
                ),
            ],
            Self::RuntimeDocker => vec![
                SectionRow::new(
                    "Docker context",
                    vec![format!("Current: {}", config.docker.context)],
                ),
                SectionRow::new(
                    "Compose file",
                    vec![
                        format!("Current: {}", config.docker.compose_file),
                        "Custom Compose files are advanced and may bypass managed runtime assumptions."
                            .to_owned(),
                    ],
                ),
                SectionRow::new("Pi image", vec![format!("Current: {}", config.harness.pi.image)]),
                SectionRow::new(
                    "Pi command",
                    vec![format!("Current: {}", config.harness.pi.command)],
                ),
                SectionRow::new(
                    "Runtime network",
                    vec![
                        format!("Current: {}", config.harness.pi.network),
                        "Bridge networking remains experimental for Pi login compatibility."
                            .to_owned(),
                    ],
                ),
                SectionRow::new(
                    "Build network",
                    vec![format!("Current: {}", config.harness.pi.build_network)],
                ),
                SectionRow::new(
                    "Read-only root filesystem",
                    vec![
                        format!("Current: {}", config.harness.pi.read_only_rootfs),
                        "Experimental hardening option with compatibility tradeoffs.".to_owned(),
                    ],
                ),
            ],
            Self::OutputColor => vec![
                SectionRow::new(
                    "Current color behavior",
                    vec![
                        "Color is enabled by default.".to_owned(),
                        "A non-empty NO_COLOR environment variable disables ANSI labels."
                            .to_owned(),
                    ],
                ),
                SectionRow::new(
                    "Future ui.color",
                    vec![
                        "Planned values: auto, always, never.".to_owned(),
                        "NO_COLOR should remain an override.".to_owned(),
                    ],
                ),
            ],
            Self::Advanced => vec![
                SectionRow::new(
                    "Config path",
                    vec![display_path(&state_paths.config_yaml)],
                ),
                SectionRow::new(
                    "Manual YAML editing",
                    vec!["Manual edits to ~/.vegasroom/config.yaml remain supported.".to_owned()],
                ),
                SectionRow::new(
                    "Backups before save",
                    vec!["Future saves should create timestamped backups before writing."
                        .to_owned()],
                ),
                SectionRow::new(
                    "Reset actions",
                    vec!["Future reset actions should preview changed fields first.".to_owned()],
                ),
            ],
        }
    }
}

struct SectionRow {
    title: String,
    details: Vec<String>,
}

impl SectionRow {
    fn new(title: impl Into<String>, details: Vec<String>) -> Self {
        Self {
            title: title.into(),
            details,
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

    #[test]
    fn workspace_section_exposes_current_config_rows() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let rows = ConfigSection::Workspace.rows(&config, &paths);

        assert!(rows.iter().any(|row| row.title == "Default workspace root"));
        assert!(rows.iter().any(|row| row.title == "Risky mount policy"));
        assert!(rows.iter().any(|row| row.title == "Read-only workspace"));
    }
}
