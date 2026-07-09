use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::{
    config::{ColorMode, Config, RiskyMountPolicy, SshMode},
    paths::{display_path, StatePaths},
    ssh,
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
    let mut config = Config::load_or_default()?;
    let state_paths = StatePaths::default()?;

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        println!("Vegasroom configuration is interactive.");
        println!(
            "Run `vr config` from a terminal, or edit the config file manually: {}",
            display_path(&state_paths.config_yaml)
        );
        return Ok(0);
    }

    loop {
        match run_tui(config, state_paths.clone())? {
            ConfigUiExit::Quit(code) => return Ok(code),
            ConfigUiExit::OpenSshConfigure => {
                let code = ssh::configure(&[], false)?;
                if code != 0 {
                    return Ok(code);
                }
                config = Config::load_or_default()?;
            }
        }
    }
}

fn run_tui(config: Config, state_paths: StatePaths) -> Result<ConfigUiExit> {
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
            KeyCode::Enter => match state.open_highlighted() {
                ConfigUiAction::Continue => {}
                ConfigUiAction::OpenSshConfigure => return Ok(ConfigUiExit::OpenSshConfigure),
            },
            KeyCode::Esc | KeyCode::Backspace => state.go_back(),
            KeyCode::Char('s') => state.save()?,
            KeyCode::Char('q') => {
                if !state.dirty {
                    return Ok(ConfigUiExit::Quit(0));
                }

                match confirm_quit()? {
                    QuitDecision::Save => {
                        state.save()?;
                        return Ok(ConfigUiExit::Quit(0));
                    }
                    QuitDecision::Discard => return Ok(ConfigUiExit::Quit(0)),
                    QuitDecision::Cancel => {
                        state.last_message = Some("Quit canceled.".to_owned());
                    }
                }
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
        ConfigScreen::PresetPreview(preset) => render_preset_preview(&mut stdout, state, preset)?,
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

fn confirm_quit() -> Result<QuitDecision> {
    render_quit_prompt()?;

    loop {
        let Event::Key(key) = event::read().context("failed to read terminal key event")? else {
            continue;
        };

        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(QuitDecision::Save),
            KeyCode::Char('n') | KeyCode::Char('N') => return Ok(QuitDecision::Discard),
            KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Esc => {
                return Ok(QuitDecision::Cancel);
            }
            _ => {}
        }
    }
}

fn render_quit_prompt() -> Result<()> {
    let mut stdout = io::stdout();
    execute!(
        stdout,
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    )
    .context("failed to render config quit prompt")?;

    writeln!(stdout, "Unsaved config changes")?;
    writeln!(stdout)?;
    writeln!(stdout, "Save changes before quitting?")?;
    writeln!(stdout)?;
    writeln!(stdout, "  y  save and quit")?;
    writeln!(stdout, "  n  quit without saving")?;
    writeln!(stdout, "  c  cancel")?;
    stdout.flush()?;
    Ok(())
}

fn render_preset_preview(
    stdout: &mut impl Write,
    state: &ConfigUiState,
    preset: SecurityPreset,
) -> Result<()> {
    writeln!(stdout, "Security preset: {}", preset.title())?;
    writeln!(stdout)?;
    writeln!(stdout, "Changes to apply:")?;

    let changes = preset_changes(&state.config, preset);
    if changes.is_empty() {
        writeln!(
            stdout,
            "  No changes; this preset already matches current config."
        )?;
    } else {
        for change in changes {
            writeln!(
                stdout,
                "  {}: {} -> {}",
                change.field, change.before, change.after
            )?;
        }
    }

    writeln!(stdout)?;
    for line in preset.notes() {
        writeln!(stdout, "  {line}")?;
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
            concat!(
                "Keys: ↑/↓ or k/j move · Enter edit/toggle · ",
                "Esc/Backspace back · s save · q quit"
            )
        )?,
        ConfigScreen::PresetPreview(_) => writeln!(
            stdout,
            "Keys: Enter apply preset · Esc/Backspace back · s save · q quit"
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
            ConfigScreen::PresetPreview(_) => {}
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
            ConfigScreen::PresetPreview(_) => {}
        }
        self.last_message = None;
    }

    fn open_highlighted(&mut self) -> ConfigUiAction {
        match self.screen {
            ConfigScreen::Sections => {
                self.screen = ConfigScreen::Section(self.highlighted_section());
                self.highlighted_row = 0;
                self.last_message = None;
            }
            ConfigScreen::Section(section) => {
                let rows = section.rows(&self.config, &self.state_paths);
                if let Some(row) = rows.get(self.highlighted_row) {
                    match row.action {
                        RowAction::PreviewPreset(preset) => {
                            self.screen = ConfigScreen::PresetPreview(preset);
                            self.last_message = None;
                        }
                        RowAction::ToggleRiskyMountPolicy => self.toggle_risky_mount_policy(),
                        RowAction::ToggleReadOnlyWorkspace => self.toggle_read_only_workspace(),
                        RowAction::ToggleReadOnlyRootfs => self.toggle_read_only_rootfs(),
                        RowAction::CycleColorMode => self.cycle_color_mode(),
                        RowAction::CycleSshMode => self.cycle_ssh_mode(),
                        RowAction::OpenSshConfigure => {
                            if self.dirty {
                                self.last_message = Some(
                                    "Save or discard pending config changes before opening SSH key configuration."
                                        .to_owned(),
                                );
                            } else {
                                return ConfigUiAction::OpenSshConfigure;
                            }
                        }
                        RowAction::Placeholder => {
                            self.last_message = Some(format!(
                                "{} editing will be added in an upcoming slice.",
                                row.title
                            ));
                        }
                    }
                }
            }
            ConfigScreen::PresetPreview(preset) => self.apply_preset(preset),
        }

        ConfigUiAction::Continue
    }

    fn go_back(&mut self) {
        match self.screen {
            ConfigScreen::Sections => {}
            ConfigScreen::Section(_) => {
                self.screen = ConfigScreen::Sections;
                self.highlighted_row = 0;
                self.last_message = None;
            }
            ConfigScreen::PresetPreview(_) => {
                self.screen = ConfigScreen::Section(ConfigSection::SecurityPreset);
                self.last_message = None;
            }
        }
    }

    fn apply_preset(&mut self, preset: SecurityPreset) {
        let changes = preset_changes(&self.config, preset);
        preset.apply(&mut self.config);
        self.dirty |= !changes.is_empty();
        self.screen = ConfigScreen::Section(ConfigSection::SecurityPreset);
        self.last_message = Some(if changes.is_empty() {
            format!("{} preset already matched current config.", preset.title())
        } else {
            format!(
                "Applied {} preset with {} pending change(s). Press s to save.",
                preset.title(),
                changes.len()
            )
        });
    }

    fn toggle_risky_mount_policy(&mut self) {
        self.config.workspace.risky_mount_policy = match self.config.workspace.risky_mount_policy {
            RiskyMountPolicy::Warn => RiskyMountPolicy::Deny,
            RiskyMountPolicy::Deny => RiskyMountPolicy::Warn,
        };
        self.dirty = true;
        self.last_message = Some(format!(
            "Set risky mount policy to {}. Press s to save.",
            risky_mount_policy_name(self.config.workspace.risky_mount_policy)
        ));
    }

    fn toggle_read_only_workspace(&mut self) {
        self.config.harness.pi.read_only_workspace = !self.config.harness.pi.read_only_workspace;
        self.dirty = true;
        self.last_message = Some(format!(
            "Set read-only workspace to {}. Press s to save.",
            self.config.harness.pi.read_only_workspace
        ));
    }

    fn toggle_read_only_rootfs(&mut self) {
        self.config.harness.pi.read_only_rootfs = !self.config.harness.pi.read_only_rootfs;
        self.dirty = true;
        self.last_message = Some(format!(
            "Set read-only root filesystem to {}. Press s to save.",
            self.config.harness.pi.read_only_rootfs
        ));
    }

    fn cycle_color_mode(&mut self) {
        self.config.ui.color = match self.config.ui.color {
            ColorMode::Auto => ColorMode::Always,
            ColorMode::Always => ColorMode::Never,
            ColorMode::Never => ColorMode::Auto,
        };
        self.dirty = true;
        self.last_message = Some(format!(
            "Set color mode to {}. Press s to save.",
            color_mode_name(self.config.ui.color)
        ));
    }

    fn cycle_ssh_mode(&mut self) {
        self.config.ssh.mode = match self.config.ssh.mode {
            SshMode::Auto => SshMode::Host,
            SshMode::Host => SshMode::Managed,
            SshMode::Managed => SshMode::Off,
            SshMode::Off => SshMode::Auto,
        };
        self.dirty = true;
        self.last_message = Some(format!(
            "Set SSH mode to {}. Press s to save.",
            ssh_mode_name(self.config.ssh.mode)
        ));
    }

    fn save(&mut self) -> Result<()> {
        if !self.dirty {
            self.last_message = Some("No config changes to save.".to_owned());
            return Ok(());
        }

        let outcome = save_config_with_backup(&self.config, &self.state_paths.config_yaml)?;
        self.config = Config::load_from_path(self.state_paths.config_yaml.clone())?;
        self.dirty = false;
        self.last_message = Some(match outcome.backup_path {
            Some(path) => format!(
                "Saved config to {}. Backup: {}",
                display_path(&self.state_paths.config_yaml),
                display_path(&path)
            ),
            None => format!(
                "Saved config to {}.",
                display_path(&self.state_paths.config_yaml)
            ),
        });
        Ok(())
    }
}

struct SaveOutcome {
    backup_path: Option<PathBuf>,
}

fn save_config_with_backup(config: &Config, config_path: &Path) -> Result<SaveOutcome> {
    let backup_path = if config_path.exists() {
        let backup_path = next_backup_path(config_path)?;
        fs::copy(config_path, &backup_path).with_context(|| {
            format!(
                "failed to back up config from {} to {}",
                display_path(config_path),
                display_path(&backup_path)
            )
        })?;
        Some(backup_path)
    } else {
        None
    };

    config.save_to_path(config_path)?;
    Config::load_from_path(config_path.to_path_buf())?;

    Ok(SaveOutcome { backup_path })
}

fn next_backup_path(config_path: &Path) -> Result<PathBuf> {
    let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = config_path
        .file_name()
        .context("config path does not have a file name")?
        .to_string_lossy();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before Unix epoch")?
        .as_secs();

    for suffix in 0..1000 {
        let candidate = if suffix == 0 {
            parent.join(format!("{file_name}.backup-{timestamp}"))
        } else {
            parent.join(format!("{file_name}.backup-{timestamp}-{suffix}"))
        };

        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    anyhow::bail!(
        "could not allocate a backup path for config: {}",
        display_path(config_path)
    )
}

#[derive(Clone, Copy)]
enum QuitDecision {
    Save,
    Discard,
    Cancel,
}

enum ConfigUiExit {
    Quit(i32),
    OpenSshConfigure,
}

enum ConfigUiAction {
    Continue,
    OpenSshConfigure,
}

#[derive(Clone, Copy)]
enum ConfigScreen {
    Sections,
    Section(ConfigSection),
    PresetPreview(SecurityPreset),
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
                format!("Current color mode: {}", color_mode_name(config.ui.color)),
                "Non-empty NO_COLOR still disables labels as an override.".to_owned(),
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
                SectionRow::new(
                    "Color behavior",
                    vec![format!("Current: {}", color_mode_name(config.ui.color))],
                ),
            ],
            Self::SecurityPreset => vec![
                SectionRow::new(
                    "Current preset",
                    vec![format!("Detected: {}", security_preset_name(config))],
                ),
                SectionRow::preset(
                    SecurityPreset::DefaultCompatible,
                    vec![
                        "Preserves current proven behavior and maximum compatibility.".to_owned(),
                        "Alias: lowsec/default.".to_owned(),
                    ],
                ),
                SectionRow::preset(
                    SecurityPreset::Safer,
                    vec![
                        "Sets risky workspace mount policy to deny.".to_owned(),
                        "Keeps workspace writes, host networking, and automatic SSH behavior."
                            .to_owned(),
                    ],
                ),
                SectionRow::preset(
                    SecurityPreset::Strict,
                    vec![
                        "Enables deny policy, read-only workspace, read-only rootfs, managed SSH, \
and no host Git inheritance."
                            .to_owned(),
                        "May reduce editing, Git, login, or shell compatibility.".to_owned(),
                    ],
                ),
            ],
            Self::Workspace => vec![
                SectionRow::new(
                    "Default workspace root",
                    vec![format!("Current: {}", config.paths.workspace)],
                ),
                SectionRow::action(
                    "Risky mount policy",
                    vec![
                        format!(
                            "Current: {}",
                            risky_mount_policy_name(config.workspace.risky_mount_policy)
                        ),
                        "Press Enter to toggle warn/deny.".to_owned(),
                        "warn prints a warning and continues; deny refuses broad risky mounts."
                            .to_owned(),
                    ],
                    RowAction::ToggleRiskyMountPolicy,
                ),
                SectionRow::action(
                    "Read-only workspace",
                    vec![
                        format!("Current: {}", config.harness.pi.read_only_workspace),
                        "Press Enter to toggle true/false.".to_owned(),
                        "When enabled, Pi may not be able to edit project files.".to_owned(),
                    ],
                    RowAction::ToggleReadOnlyWorkspace,
                ),
            ],
            Self::Ssh => vec![
                SectionRow::action(
                    "SSH mode",
                    vec![
                        format!("Current: {}", ssh_mode_name(config.ssh.mode)),
                        "Press Enter to cycle auto/host/managed/off.".to_owned(),
                        "auto uses managed keys when selected, otherwise host SSH_AUTH_SOCK."
                            .to_owned(),
                    ],
                    RowAction::CycleSshMode,
                ),
                SectionRow::action(
                    "Selected managed SSH keys",
                    vec![
                        format!("Current selected keys: {}", config.ssh.selected_keys.len()),
                        "Press Enter to open the existing SSH key configuration flow.".to_owned(),
                        "Save or discard other pending config changes first.".to_owned(),
                    ],
                    RowAction::OpenSshConfigure,
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
                        "Custom Compose files are advanced and may bypass managed runtime \
assumptions."
                            .to_owned(),
                    ],
                ),
                SectionRow::new(
                    "Pi image",
                    vec![format!("Current: {}", config.harness.pi.image)],
                ),
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
                SectionRow::action(
                    "Read-only root filesystem",
                    vec![
                        format!("Current: {}", config.harness.pi.read_only_rootfs),
                        "Press Enter to toggle true/false.".to_owned(),
                        "Experimental hardening option with compatibility tradeoffs.".to_owned(),
                    ],
                    RowAction::ToggleReadOnlyRootfs,
                ),
            ],
            Self::OutputColor => vec![SectionRow::action(
                "Color mode",
                vec![
                    format!("Current: {}", color_mode_name(config.ui.color)),
                    "Press Enter to cycle auto/always/never.".to_owned(),
                    "auto colors terminal output; always forces ANSI; never disables ANSI."
                        .to_owned(),
                    "A non-empty NO_COLOR environment variable disables ANSI labels.".to_owned(),
                ],
                RowAction::CycleColorMode,
            )],
            Self::Advanced => vec![
                SectionRow::new("Config path", vec![display_path(&state_paths.config_yaml)]),
                SectionRow::new(
                    "Manual YAML editing",
                    vec!["Manual edits to ~/.vegasroom/config.yaml remain supported.".to_owned()],
                ),
                SectionRow::new(
                    "Backups before save",
                    vec![
                        "Future saves should create timestamped backups before writing.".to_owned(),
                    ],
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
    action: RowAction,
}

impl SectionRow {
    fn new(title: impl Into<String>, details: Vec<String>) -> Self {
        Self {
            title: title.into(),
            details,
            action: RowAction::Placeholder,
        }
    }

    fn preset(preset: SecurityPreset, details: Vec<String>) -> Self {
        Self {
            title: preset.title().to_owned(),
            details,
            action: RowAction::PreviewPreset(preset),
        }
    }

    fn action(title: impl Into<String>, details: Vec<String>, action: RowAction) -> Self {
        Self {
            title: title.into(),
            details,
            action,
        }
    }
}

#[derive(Clone, Copy)]
enum RowAction {
    Placeholder,
    PreviewPreset(SecurityPreset),
    ToggleRiskyMountPolicy,
    ToggleReadOnlyWorkspace,
    ToggleReadOnlyRootfs,
    CycleColorMode,
    CycleSshMode,
    OpenSshConfigure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SecurityPreset {
    DefaultCompatible,
    Safer,
    Strict,
}

impl SecurityPreset {
    fn title(self) -> &'static str {
        match self {
            Self::DefaultCompatible => "Default / Compatible",
            Self::Safer => "Safer",
            Self::Strict => "Strict",
        }
    }

    fn notes(self) -> Vec<&'static str> {
        match self {
            Self::DefaultCompatible => vec![
                "Maximum compatibility with the currently proven runtime.",
                "Alias: lowsec/default.",
            ],
            Self::Safer => vec![
                "Improves accidental exposure protection by denying risky workspace mounts.",
                "Keeps workspace writes, host networking, automatic SSH, and host Git inheritance.",
            ],
            Self::Strict => vec![
                "Security-forward settings with compatibility tradeoffs.",
                "Does not change host networking because bridge remains experimental for Pi login.",
            ],
        }
    }

    fn apply(self, config: &mut Config) {
        match self {
            Self::DefaultCompatible => {
                config.workspace.risky_mount_policy = RiskyMountPolicy::Warn;
                config.harness.pi.read_only_workspace = false;
                config.harness.pi.read_only_rootfs = false;
                config.harness.pi.network = "host".to_owned();
                config.harness.pi.build_network = "host".to_owned();
                config.ssh.mode = SshMode::Auto;
                config.git.inherit_host = true;
            }
            Self::Safer => {
                config.workspace.risky_mount_policy = RiskyMountPolicy::Deny;
                config.harness.pi.read_only_workspace = false;
                config.harness.pi.read_only_rootfs = false;
                config.harness.pi.network = "host".to_owned();
                config.harness.pi.build_network = "host".to_owned();
                config.ssh.mode = SshMode::Auto;
                config.git.inherit_host = true;
            }
            Self::Strict => {
                config.workspace.risky_mount_policy = RiskyMountPolicy::Deny;
                config.harness.pi.read_only_workspace = true;
                config.harness.pi.read_only_rootfs = true;
                config.harness.pi.network = "host".to_owned();
                config.harness.pi.build_network = "host".to_owned();
                config.ssh.mode = SshMode::Managed;
                config.git.inherit_host = false;
            }
        }
    }
}

struct ConfigChange {
    field: &'static str,
    before: String,
    after: String,
}

fn preset_changes(config: &Config, preset: SecurityPreset) -> Vec<ConfigChange> {
    let mut target = config.clone();
    preset.apply(&mut target);

    diff_preset_configs(config, &target)
}

fn diff_preset_configs(before: &Config, after: &Config) -> Vec<ConfigChange> {
    let mut changes = Vec::new();
    push_change(
        &mut changes,
        "workspace.risky_mount_policy",
        risky_mount_policy_name(before.workspace.risky_mount_policy),
        risky_mount_policy_name(after.workspace.risky_mount_policy),
    );
    push_change(
        &mut changes,
        "harness.pi.read_only_workspace",
        before.harness.pi.read_only_workspace,
        after.harness.pi.read_only_workspace,
    );
    push_change(
        &mut changes,
        "harness.pi.read_only_rootfs",
        before.harness.pi.read_only_rootfs,
        after.harness.pi.read_only_rootfs,
    );
    push_change(
        &mut changes,
        "harness.pi.network",
        before.harness.pi.network.as_str(),
        after.harness.pi.network.as_str(),
    );
    push_change(
        &mut changes,
        "harness.pi.build_network",
        before.harness.pi.build_network.as_str(),
        after.harness.pi.build_network.as_str(),
    );
    push_change(
        &mut changes,
        "ssh.mode",
        ssh_mode_name(before.ssh.mode),
        ssh_mode_name(after.ssh.mode),
    );
    push_change(
        &mut changes,
        "git.inherit_host",
        before.git.inherit_host,
        after.git.inherit_host,
    );
    changes
}

fn push_change(
    changes: &mut Vec<ConfigChange>,
    field: &'static str,
    before: impl ToString,
    after: impl ToString,
) {
    let before = before.to_string();
    let after = after.to_string();
    if before != after {
        changes.push(ConfigChange {
            field,
            before,
            after,
        });
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

fn color_mode_name(mode: ColorMode) -> &'static str {
    match mode {
        ColorMode::Auto => "auto",
        ColorMode::Always => "always",
        ColorMode::Never => "never",
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
    fn safer_preset_preview_lists_expected_change() {
        let config = Config::default();
        let changes = preset_changes(&config, SecurityPreset::Safer);

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "workspace.risky_mount_policy");
        assert_eq!(changes[0].before, "warn");
        assert_eq!(changes[0].after, "deny");
    }

    #[test]
    fn applying_strict_preset_updates_config_and_marks_dirty() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);

        state.apply_preset(SecurityPreset::Strict);

        assert!(state.dirty);
        assert_eq!(
            state.config.workspace.risky_mount_policy,
            RiskyMountPolicy::Deny
        );
        assert!(state.config.harness.pi.read_only_workspace);
        assert!(state.config.harness.pi.read_only_rootfs);
        assert_eq!(state.config.ssh.mode, SshMode::Managed);
        assert!(!state.config.git.inherit_host);
    }

    #[test]
    fn applying_matching_preset_does_not_mark_dirty() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);

        state.apply_preset(SecurityPreset::DefaultCompatible);

        assert!(!state.dirty);
    }

    #[test]
    fn workspace_editor_toggles_risky_mount_policy() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);

        state.toggle_risky_mount_policy();

        assert!(state.dirty);
        assert_eq!(
            state.config.workspace.risky_mount_policy,
            RiskyMountPolicy::Deny
        );
        assert!(state
            .last_message
            .as_deref()
            .is_some_and(|message| message.contains("Press s to save")));
    }

    #[test]
    fn workspace_editor_toggles_read_only_workspace() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);

        state.toggle_read_only_workspace();

        assert!(state.dirty);
        assert!(state.config.harness.pi.read_only_workspace);
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

    #[test]
    fn runtime_editor_toggles_read_only_rootfs() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);

        state.toggle_read_only_rootfs();

        assert!(state.dirty);
        assert!(state.config.harness.pi.read_only_rootfs);
        assert!(state
            .last_message
            .as_deref()
            .is_some_and(|message| message.contains("Press s to save")));
    }

    #[test]
    fn runtime_section_exposes_current_config_rows() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let rows = ConfigSection::RuntimeDocker.rows(&config, &paths);

        assert!(rows.iter().any(|row| row.title == "Runtime network"));
        assert!(rows
            .iter()
            .any(|row| row.title == "Read-only root filesystem"));
    }

    #[test]
    fn output_color_editor_cycles_color_mode() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);

        state.cycle_color_mode();

        assert!(state.dirty);
        assert_eq!(state.config.ui.color, ColorMode::Always);
        assert!(state
            .last_message
            .as_deref()
            .is_some_and(|message| message.contains("Press s to save")));
    }

    #[test]
    fn output_color_section_exposes_color_mode_row() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let rows = ConfigSection::OutputColor.rows(&config, &paths);

        assert!(rows.iter().any(|row| row.title == "Color mode"));
    }

    #[test]
    fn ssh_editor_cycles_ssh_mode() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);

        state.cycle_ssh_mode();

        assert!(state.dirty);
        assert_eq!(state.config.ssh.mode, SshMode::Host);
        assert!(state
            .last_message
            .as_deref()
            .is_some_and(|message| message.contains("Press s to save")));
    }

    #[test]
    fn ssh_section_exposes_mode_and_key_configuration_rows() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let rows = ConfigSection::Ssh.rows(&config, &paths);

        assert!(rows.iter().any(|row| row.title == "SSH mode"));
        assert!(rows
            .iter()
            .any(|row| row.title == "Selected managed SSH keys"));
    }

    #[test]
    fn ssh_key_configuration_is_blocked_when_dirty() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);
        state.screen = ConfigScreen::Section(ConfigSection::Ssh);
        state.highlighted_row = 1;
        state.dirty = true;

        let action = state.open_highlighted();

        assert!(matches!(action, ConfigUiAction::Continue));
        assert!(state
            .last_message
            .as_deref()
            .is_some_and(|message| message.contains("Save or discard")));
    }

    #[test]
    fn ssh_key_configuration_launches_existing_flow_when_clean() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);
        state.screen = ConfigScreen::Section(ConfigSection::Ssh);
        state.highlighted_row = 1;

        let action = state.open_highlighted();

        assert!(matches!(action, ConfigUiAction::OpenSshConfigure));
    }

    #[test]
    fn save_config_writes_backup_and_validates_saved_config() {
        let dir = unique_temp_dir("save-config-backup");
        fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("config.yaml");

        Config::default().save_to_path(&config_path).unwrap();
        let original = fs::read_to_string(&config_path).unwrap();

        let mut changed = Config::default();
        changed.paths.workspace = "/tmp/changed-workspace".to_owned();

        let outcome = save_config_with_backup(&changed, &config_path).unwrap();
        let backup_path = outcome.backup_path.unwrap();

        assert_eq!(fs::read_to_string(&backup_path).unwrap(), original);
        assert_eq!(
            Config::load_from_path(config_path).unwrap().paths.workspace,
            "/tmp/changed-workspace"
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn state_save_clears_dirty_after_writing_config() {
        let dir = unique_temp_dir("state-save");
        fs::create_dir_all(&dir).unwrap();
        let paths = StatePaths::from_root(dir.clone());
        Config::default().save_to_path(&paths.config_yaml).unwrap();

        let mut config = Config::default();
        config.paths.workspace = "/tmp/state-save-workspace".to_owned();
        let mut state = ConfigUiState::new(config, paths.clone());
        state.dirty = true;

        state.save().unwrap();

        assert!(!state.dirty);
        assert_eq!(
            Config::load_from_path(paths.config_yaml)
                .unwrap()
                .paths
                .workspace,
            "/tmp/state-save-workspace"
        );
        assert!(state
            .last_message
            .as_deref()
            .is_some_and(|message| message.contains("Backup:")));

        let _ = fs::remove_dir_all(dir);
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "vegasroom-config-ui-{name}-{}-{timestamp}",
            std::process::id()
        ))
    }
}
