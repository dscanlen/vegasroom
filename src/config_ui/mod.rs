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
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

mod cache;

use crate::{
    alert, atomic_write,
    config::{ColorMode, Config, RiskyMountPolicy, SshMode},
    docker,
    paths::{display_path, StatePaths},
    ssh,
};

use cache::{package_cache_paths, purge_package_cache_paths};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[32m";
const DIM: &str = "\x1b[2m";

#[derive(Clone, Copy)]
struct TuiStyles {
    enabled: bool,
}

impl TuiStyles {
    fn for_config(config: &Config) -> Self {
        Self {
            enabled: alert::colors_enabled_for_config(config, io::stdout().is_terminal()),
        }
    }

    #[cfg(test)]
    fn plain() -> Self {
        Self { enabled: false }
    }

    fn code(self, code: &'static str) -> &'static str {
        if self.enabled {
            code
        } else {
            ""
        }
    }

    fn bold(self, text: &str) -> String {
        format!("{}{}{}", self.code(BOLD), text, self.code(RESET))
    }

    fn green(self, text: &str) -> String {
        format!("{}{}{}", self.code(GREEN), text, self.code(RESET))
    }

    fn green_bold(self, text: &str) -> String {
        format!(
            "{}{}{}{}",
            self.code(GREEN),
            self.code(BOLD),
            text,
            self.code(RESET)
        )
    }

    fn dim(self, text: &str) -> String {
        format!("{}{}{}", self.code(DIM), text, self.code(RESET))
    }
}

const SECTIONS: &[ConfigSection] = &[
    ConfigSection::SecurityPreset,
    ConfigSection::Environment,
    ConfigSection::Ssh,
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
            KeyCode::Esc => {
                if matches!(state.screen, ConfigScreen::Sections) {
                    if let Some(exit) = confirm_config_quit_if_needed(&mut state)? {
                        return Ok(exit);
                    }
                } else {
                    state.go_back();
                }
            }
            KeyCode::Char('s') => state.save()?,
            KeyCode::Char('q') => {
                if let Some(exit) = confirm_config_quit_if_needed(&mut state)? {
                    return Ok(exit);
                }
            }
            _ => {}
        }
    }
}

fn render(state: &ConfigUiState) -> Result<()> {
    let mut buffer = Vec::new();
    let styles = TuiStyles::for_config(&state.config);

    render_header(&mut buffer, state, styles)?;

    match state.screen {
        ConfigScreen::Sections => render_sections_screen(&mut buffer, state, styles)?,
        ConfigScreen::Section(section) => {
            render_section_screen(&mut buffer, state, section, styles)?
        }
        ConfigScreen::PresetPreview(preset) => render_preset_preview(&mut buffer, state, preset)?,
        ConfigScreen::ResetDefaultsPreview => render_reset_defaults_preview(&mut buffer, state)?,
        ConfigScreen::PurgePackageCachesPreview => {
            render_purge_package_caches_preview(&mut buffer, state)?
        }
    }

    if let Some(message) = &state.last_message {
        writeln!(buffer)?;
        writeln!(buffer, "notice  {message}")?;
    }

    writeln!(buffer)?;
    render_keys(&mut buffer, state)?;

    let lines = buffer_lines(&buffer);
    let mut stdout = io::stdout();
    draw_bottom_panel(&mut stdout, &lines).context("failed to render config UI")?;
    stdout.flush()?;
    Ok(())
}

fn render_header(stdout: &mut impl Write, state: &ConfigUiState, styles: TuiStyles) -> Result<()> {
    let status = if state.dirty {
        styles.bold("unsaved")
    } else {
        styles.green("saved")
    };
    writeln!(stdout, "╭─ {} · {status}", styles.bold("vegasroom config"))?;
    writeln!(
        stdout,
        "│  {}",
        styles.dim(&display_path(&state.state_paths.config_yaml))
    )?;
    writeln!(stdout, "│")?;
    Ok(())
}

fn render_sections_screen(
    stdout: &mut impl Write,
    state: &ConfigUiState,
    styles: TuiStyles,
) -> Result<()> {
    writeln!(stdout, "│  {}", styles.dim("choose a section"))?;
    for (index, section) in SECTIONS.iter().enumerate() {
        let marker = if index == state.highlighted_section {
            "›"
        } else {
            " "
        };
        let title = if index == state.highlighted_section {
            styles.bold(section.title())
        } else {
            section.title().to_owned()
        };
        writeln!(stdout, "│  {marker} {title}")?;
    }

    Ok(())
}

fn render_section_screen(
    stdout: &mut impl Write,
    state: &ConfigUiState,
    section: ConfigSection,
    styles: TuiStyles,
) -> Result<()> {
    writeln!(stdout, "│  {}", styles.dim(section.title()))?;
    let rows = section.rows(&state.config, &state.state_paths);
    for (index, row) in rows.iter().enumerate() {
        let marker = if index == state.highlighted_row {
            "›"
        } else {
            " "
        };
        let title = styled_row_title(
            section,
            row,
            &state.config,
            index == state.highlighted_row,
            styles,
        );
        writeln!(stdout, "│  {marker} {title}")?;
        for detail in &row.details {
            writeln!(stdout, "│      {}", styles.dim(detail))?;
        }
    }

    Ok(())
}

fn styled_row_title(
    section: ConfigSection,
    row: &SectionRow,
    config: &Config,
    highlighted: bool,
    styles: TuiStyles,
) -> String {
    if matches!(section, ConfigSection::SecurityPreset)
        && row
            .security_preset()
            .is_some_and(|preset| Some(preset) == active_security_preset(config))
    {
        return styles.green_bold(&format!("✓ {}", row.title));
    }

    if matches!(section, ConfigSection::Environment) {
        match row.action {
            RowAction::ToggleRustToolchain if config.environment.rust.enabled => {
                return styles.green_bold(&row.title);
            }
            RowAction::TogglePythonToolchain if config.environment.python.enabled => {
                return styles.green_bold(&row.title);
            }
            RowAction::ToggleGoToolchain if config.environment.go.enabled => {
                return styles.green_bold(&row.title);
            }
            RowAction::ToggleTypeScriptToolchain if config.environment.typescript.enabled => {
                return styles.green_bold(&row.title);
            }
            _ => {}
        }
    }

    if highlighted {
        styles.bold(&row.title)
    } else {
        row.title.clone()
    }
}

fn confirm_config_quit_if_needed(state: &mut ConfigUiState) -> Result<Option<ConfigUiExit>> {
    if !state.dirty {
        return Ok(Some(ConfigUiExit::Quit(0)));
    }

    match confirm_quit()? {
        QuitDecision::Save => {
            state.save()?;
            Ok(Some(ConfigUiExit::Quit(0)))
        }
        QuitDecision::Discard => Ok(Some(ConfigUiExit::Quit(0))),
        QuitDecision::Cancel => {
            state.last_message = Some("Quit canceled.".to_owned());
            Ok(None)
        }
    }
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
    let lines = vec![
        "╭─ unsaved config changes".to_owned(),
        "│".to_owned(),
        "│  save changes before quitting?".to_owned(),
        "│".to_owned(),
        "│  y  save and quit".to_owned(),
        "│  n  quit without saving".to_owned(),
        "│  c  cancel".to_owned(),
        "╰".to_owned(),
    ];
    let mut stdout = io::stdout();
    draw_bottom_panel(&mut stdout, &lines).context("failed to render config quit prompt")?;
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

fn render_reset_defaults_preview(stdout: &mut impl Write, state: &ConfigUiState) -> Result<()> {
    writeln!(stdout, "Reset all config to defaults")?;
    writeln!(stdout)?;
    writeln!(stdout, "Changes to apply:")?;

    let changes = reset_defaults_changes(&state.config);
    if changes.is_empty() {
        writeln!(stdout, "  No changes; config already matches defaults.")?;
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
    writeln!(stdout, "  This resets all config fields in memory.")?;
    writeln!(
        stdout,
        "  Press s after applying to save the reset to disk."
    )?;
    Ok(())
}

fn render_purge_package_caches_preview(
    stdout: &mut impl Write,
    state: &ConfigUiState,
) -> Result<()> {
    writeln!(stdout, "Purge package download caches")?;
    writeln!(stdout)?;
    writeln!(stdout, "This removes safe package download caches only:")?;
    for path in package_cache_paths(&state.state_paths) {
        writeln!(stdout, "  {}", display_path(&path))?;
    }
    writeln!(stdout)?;
    writeln!(
        stdout,
        "It preserves toolchain settings, auth, SSH, workspaces,"
    )?;
    writeln!(
        stdout,
        "Pi npm-global installs, and Cargo-installed binaries."
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Press Enter to purge, or Esc to cancel.")?;
    Ok(())
}

fn render_keys(stdout: &mut impl Write, state: &ConfigUiState) -> Result<()> {
    match state.screen {
        ConfigScreen::Sections => {
            writeln!(stdout, "╰─ ↑↓/jk move  enter open  s save  esc/q quit")?
        }
        ConfigScreen::Section(_) => writeln!(
            stdout,
            "╰─ ↑↓/jk move  enter activate  esc back  s save  q quit"
        )?,
        ConfigScreen::PresetPreview(_) => {
            writeln!(stdout, "╰─ enter apply preset  esc back  s save  q quit")?
        }
        ConfigScreen::ResetDefaultsPreview => {
            writeln!(stdout, "╰─ enter reset defaults  esc back  s save  q quit")?
        }
        ConfigScreen::PurgePackageCachesPreview => {
            writeln!(stdout, "╰─ enter purge caches  esc cancel  s save  q quit")?
        }
    }

    Ok(())
}

fn buffer_lines(buffer: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(buffer)
        .lines()
        .map(str::to_owned)
        .collect()
}

fn draw_bottom_panel(stdout: &mut io::Stdout, lines: &[String]) -> Result<()> {
    let (width, height) = terminal::size().unwrap_or((100, 30));
    let width = width.max(1);
    let height = height.max(1);
    let max_rows = usize::from(height);
    let visible_lines = if lines.len() > max_rows {
        &lines[lines.len() - max_rows..]
    } else {
        lines
    };
    let start_row = height.saturating_sub(visible_lines.len() as u16);

    execute!(stdout, terminal::Clear(ClearType::All))?;
    for (index, line) in visible_lines.iter().enumerate() {
        execute!(
            stdout,
            cursor::MoveTo(0, start_row + index as u16),
            terminal::Clear(ClearType::CurrentLine)
        )?;
        write!(stdout, "{}", truncate_to_width(line, width))?;
    }

    Ok(())
}

fn truncate_to_width(text: &str, width: u16) -> String {
    let max_width = usize::from(width);
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_owned();
    }

    let ellipsis = "…";
    let ellipsis_width = UnicodeWidthStr::width(ellipsis);
    let target_width = max_width.saturating_sub(ellipsis_width);
    let mut output = String::new();
    let mut used_width = 0usize;

    for ch in text.chars() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used_width + char_width > target_width {
            break;
        }
        output.push(ch);
        used_width += char_width;
    }

    output.push_str(ellipsis);
    output
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
            ConfigScreen::PresetPreview(_)
            | ConfigScreen::ResetDefaultsPreview
            | ConfigScreen::PurgePackageCachesPreview => {}
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
            ConfigScreen::PresetPreview(_)
            | ConfigScreen::ResetDefaultsPreview
            | ConfigScreen::PurgePackageCachesPreview => {}
        }
        self.last_message = None;
    }

    fn open_highlighted(&mut self) -> ConfigUiAction {
        match self.screen {
            ConfigScreen::Sections => {
                if matches!(self.highlighted_section(), ConfigSection::Ssh) {
                    if self.dirty {
                        self.last_message = Some(
                            "Save or discard pending config changes before opening SSH key configuration."
                                .to_owned(),
                        );
                    } else {
                        return ConfigUiAction::OpenSshConfigure;
                    }
                } else {
                    self.screen = ConfigScreen::Section(self.highlighted_section());
                    self.highlighted_row = 0;
                    self.last_message = None;
                }
            }
            ConfigScreen::Section(section) => {
                let rows = section.rows(&self.config, &self.state_paths);
                if let Some(row) = rows.get(self.highlighted_row) {
                    match row.action {
                        RowAction::PreviewPreset(preset) => {
                            self.screen = ConfigScreen::PresetPreview(preset);
                            self.last_message = None;
                        }
                        RowAction::CycleColorMode => self.cycle_color_mode(),
                        RowAction::ToggleGitInheritHost => self.toggle_git_inherit_host(),
                        RowAction::ToggleRustToolchain => self.toggle_rust_toolchain(),
                        RowAction::TogglePythonToolchain => self.toggle_python_toolchain(),
                        RowAction::ToggleGoToolchain => self.toggle_go_toolchain(),
                        RowAction::ToggleTypeScriptToolchain => self.toggle_typescript_toolchain(),
                        RowAction::ValidateConfig => {
                            if let Err(error) = self.validate_config() {
                                self.last_message =
                                    Some(format!("Config validation failed: {error:#}"));
                            }
                        }
                        RowAction::PreviewResetDefaults => {
                            self.screen = ConfigScreen::ResetDefaultsPreview;
                            self.last_message = None;
                        }
                        RowAction::PreviewPurgePackageCaches => {
                            self.screen = ConfigScreen::PurgePackageCachesPreview;
                            self.last_message = None;
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
            ConfigScreen::ResetDefaultsPreview => self.apply_reset_defaults(),
            ConfigScreen::PurgePackageCachesPreview => {
                if let Err(error) = self.purge_package_caches() {
                    self.last_message = Some(format!("Package cache purge failed: {error:#}"));
                }
            }
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
            ConfigScreen::ResetDefaultsPreview => {
                self.screen = ConfigScreen::Section(ConfigSection::Advanced);
                self.last_message = None;
            }
            ConfigScreen::PurgePackageCachesPreview => {
                self.screen = ConfigScreen::Section(ConfigSection::Environment);
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

    fn apply_reset_defaults(&mut self) {
        let changes = reset_defaults_changes(&self.config);
        if !changes.is_empty() {
            self.config = Config::default();
            self.dirty = true;
        }
        self.screen = ConfigScreen::Section(ConfigSection::Advanced);
        self.last_message = Some(if changes.is_empty() {
            "Config already matched defaults.".to_owned()
        } else {
            format!(
                "Reset {} config field(s) to defaults. Press s to save.",
                changes.len()
            )
        });
    }

    fn validate_config(&mut self) -> Result<()> {
        let serialized =
            serde_yaml::to_string(&self.config).context("failed to serialize config")?;
        let reparsed: Config =
            serde_yaml::from_str(&serialized).context("failed to reload serialized config")?;
        reparsed.validate_semantics()?;
        self.last_message = Some("Current in-memory config validates successfully.".to_owned());
        Ok(())
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

    fn toggle_git_inherit_host(&mut self) {
        self.config.git.inherit_host = !self.config.git.inherit_host;
        self.dirty = true;
        self.last_message = Some(format!(
            "Set host Git identity inheritance to {}. Press s to save.",
            self.config.git.inherit_host
        ));
    }

    fn toggle_rust_toolchain(&mut self) {
        self.config.environment.rust.enabled = !self.config.environment.rust.enabled;
        self.dirty = true;
        self.last_message = Some(format!(
            "Set Rust toolchain to {}. Press s to save; run `vr init --build` when ready.",
            enabled_name(self.config.environment.rust.enabled)
        ));
    }

    fn toggle_python_toolchain(&mut self) {
        self.config.environment.python.enabled = !self.config.environment.python.enabled;
        self.dirty = true;
        self.last_message = Some(format!(
            "Set Python toolchain to {}. Press s to save; run `vr init --build` when ready.",
            enabled_name(self.config.environment.python.enabled)
        ));
    }

    fn toggle_go_toolchain(&mut self) {
        self.config.environment.go.enabled = !self.config.environment.go.enabled;
        self.dirty = true;
        self.last_message = Some(format!(
            "Set Go toolchain to {}. Press s to save; run `vr init --build` when ready.",
            enabled_name(self.config.environment.go.enabled)
        ));
    }

    fn toggle_typescript_toolchain(&mut self) {
        self.config.environment.typescript.enabled = !self.config.environment.typescript.enabled;
        self.dirty = true;
        self.last_message = Some(format!(
            "Set TypeScript toolchain to {}. Press s to save; run `vr init --build` when ready.",
            enabled_name(self.config.environment.typescript.enabled)
        ));
    }

    fn purge_package_caches(&mut self) -> Result<()> {
        let purged = purge_package_cache_paths(&self.state_paths)?;
        self.screen = ConfigScreen::Section(ConfigSection::Environment);
        self.last_message = Some(if purged == 0 {
            "No package cache directories were present.".to_owned()
        } else {
            format!("Purged {purged} package cache directorie(s).")
        });
        Ok(())
    }

    fn save(&mut self) -> Result<()> {
        if !self.dirty {
            self.last_message = Some("No config changes to save.".to_owned());
            return Ok(());
        }

        save_config_with_recovery_backup(&self.config, &self.state_paths.config_yaml)?;
        self.config = Config::load_from_path(self.state_paths.config_yaml.clone())?;
        self.dirty = false;
        self.last_message = Some(format!(
            "Saved config to {}.",
            display_path(&self.state_paths.config_yaml)
        ));
        Ok(())
    }
}

fn save_config_with_recovery_backup(config: &Config, config_path: &Path) -> Result<()> {
    let backup_path = if config_path.exists() {
        let backup_path = next_backup_path(config_path)?;
        atomic_write::copy_file(config_path, &backup_path).with_context(|| {
            format!(
                "failed to create recovery backup from {} to {}",
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

    if let Some(backup_path) = backup_path {
        fs::remove_file(&backup_path).with_context(|| {
            format!(
                "saved config but failed to remove recovery backup: {}",
                display_path(&backup_path)
            )
        })?;
    }

    Ok(())
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
    ResetDefaultsPreview,
    PurgePackageCachesPreview,
}

#[derive(Clone, Copy)]
enum ConfigSection {
    SecurityPreset,
    Environment,
    Ssh,
    Advanced,
}

impl ConfigSection {
    fn title(self) -> &'static str {
        match self {
            Self::SecurityPreset => "Security",
            Self::Environment => "Environment",
            Self::Ssh => "SSH",
            Self::Advanced => "Advanced",
        }
    }

    fn rows(self, config: &Config, state_paths: &StatePaths) -> Vec<SectionRow> {
        match self {
            Self::SecurityPreset => vec![
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
            Self::Environment => vec![
                SectionRow::action(
                    toolchain_row_title("Rust", config.environment.rust.enabled),
                    vec![
                        format!(
                            "Current: {} ({})",
                            enabled_name(config.environment.rust.enabled),
                            config.environment.rust.toolchain
                        ),
                        "Press Enter to toggle. Press s to save.".to_owned(),
                        "Run `vr init --build` when ready to rebuild the environment image."
                            .to_owned(),
                    ],
                    RowAction::ToggleRustToolchain,
                ),
                SectionRow::action(
                    toolchain_row_title("Python", config.environment.python.enabled),
                    vec![
                        format!(
                            "Current: {}",
                            enabled_name(config.environment.python.enabled)
                        ),
                        "Press Enter to toggle. Press s to save.".to_owned(),
                        "Run `vr init --build` when ready to rebuild the environment image."
                            .to_owned(),
                    ],
                    RowAction::TogglePythonToolchain,
                ),
                SectionRow::action(
                    toolchain_row_title("Go", config.environment.go.enabled),
                    vec![
                        format!("Current: {}", enabled_name(config.environment.go.enabled)),
                        "Press Enter to toggle. Press s to save.".to_owned(),
                        "Run `vr init --build` when ready to rebuild the environment image."
                            .to_owned(),
                    ],
                    RowAction::ToggleGoToolchain,
                ),
                SectionRow::action(
                    toolchain_row_title("TypeScript", config.environment.typescript.enabled),
                    vec![
                        format!(
                            "Current: {}; packages: {}",
                            enabled_name(config.environment.typescript.enabled),
                            config.environment.typescript.packages.join(", ")
                        ),
                        "Press Enter to toggle. Press s to save.".to_owned(),
                        "Run `vr init --build` when ready to rebuild the environment image."
                            .to_owned(),
                    ],
                    RowAction::ToggleTypeScriptToolchain,
                ),
                SectionRow::action(
                    "Purge package download caches",
                    vec![
                        "Removes npm/pip download caches and Cargo registry/git caches.".to_owned(),
                        "Preserves workspaces, auth, SSH, Pi npm-global, and Cargo bin.".to_owned(),
                    ],
                    RowAction::PreviewPurgePackageCaches,
                ),
            ],
            Self::Ssh => Vec::new(),
            Self::Advanced => vec![
                SectionRow::action(
                    "Git: inherit host identity",
                    vec![
                        format!("Current: {}", config.git.inherit_host),
                        "Press Enter to toggle true/false.".to_owned(),
                    ],
                    RowAction::ToggleGitInheritHost,
                ),
                SectionRow::new(
                    "Git: configured user.name",
                    vec![format!(
                        "Current: {}",
                        config.git.user_name.as_deref().unwrap_or("not set")
                    )],
                ),
                SectionRow::new(
                    "Git: configured user.email",
                    vec![format!(
                        "Current: {}",
                        config.git.user_email.as_deref().unwrap_or("not set")
                    )],
                ),
                SectionRow::new("Git: effective identity", git_identity_preview(config)),
                SectionRow::action(
                    "Color mode",
                    vec![format!("Current: {}", color_mode_name(config.ui.color))],
                    RowAction::CycleColorMode,
                ),
                SectionRow::new("Config path", vec![display_path(&state_paths.config_yaml)]),
                SectionRow::action(
                    "Validate current config",
                    vec!["Press Enter to validate the in-memory config model.".to_owned()],
                    RowAction::ValidateConfig,
                ),
                SectionRow::new(
                    "Recovery backup during save",
                    vec![
                        "Saving over an existing config creates a temporary recovery backup."
                            .to_owned(),
                        "The backup is removed after the new config is saved and validated."
                            .to_owned(),
                    ],
                ),
                SectionRow::action(
                    "Reset all to defaults",
                    vec![
                        "Press Enter to preview all fields that would change.".to_owned(),
                        "The reset is applied in memory first; press s to save it.".to_owned(),
                    ],
                    RowAction::PreviewResetDefaults,
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

    fn security_preset(&self) -> Option<SecurityPreset> {
        match self.action {
            RowAction::PreviewPreset(preset) => Some(preset),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum RowAction {
    Placeholder,
    PreviewPreset(SecurityPreset),
    CycleColorMode,
    ToggleGitInheritHost,
    ToggleRustToolchain,
    TogglePythonToolchain,
    ToggleGoToolchain,
    ToggleTypeScriptToolchain,
    ValidateConfig,
    PreviewResetDefaults,
    PreviewPurgePackageCaches,
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

fn reset_defaults_changes(config: &Config) -> Vec<ConfigChange> {
    diff_configs(config, &Config::default())
}

fn diff_configs(before: &Config, after: &Config) -> Vec<ConfigChange> {
    let mut changes = diff_preset_configs(before, after);
    push_change(
        &mut changes,
        "paths.workspace",
        before.paths.workspace.as_str(),
        after.paths.workspace.as_str(),
    );
    push_change(
        &mut changes,
        "docker.context",
        before.docker.context.as_str(),
        after.docker.context.as_str(),
    );
    push_change(
        &mut changes,
        "docker.compose_file",
        before.docker.compose_file.as_str(),
        after.docker.compose_file.as_str(),
    );
    push_change(
        &mut changes,
        "harness.pi.image",
        before.harness.pi.image.as_str(),
        after.harness.pi.image.as_str(),
    );
    push_change(
        &mut changes,
        "harness.pi.command",
        before.harness.pi.command.as_str(),
        after.harness.pi.command.as_str(),
    );
    push_change(
        &mut changes,
        "ssh.selected_keys",
        before.ssh.selected_keys.len(),
        after.ssh.selected_keys.len(),
    );
    push_change(
        &mut changes,
        "git.user_name",
        option_value(before.git.user_name.as_deref()),
        option_value(after.git.user_name.as_deref()),
    );
    push_change(
        &mut changes,
        "git.user_email",
        option_value(before.git.user_email.as_deref()),
        option_value(after.git.user_email.as_deref()),
    );
    push_change(
        &mut changes,
        "ui.color",
        color_mode_name(before.ui.color),
        color_mode_name(after.ui.color),
    );
    push_change(
        &mut changes,
        "environment.rust.enabled",
        before.environment.rust.enabled,
        after.environment.rust.enabled,
    );
    push_change(
        &mut changes,
        "environment.rust.toolchain",
        before.environment.rust.toolchain.as_str(),
        after.environment.rust.toolchain.as_str(),
    );
    push_change(
        &mut changes,
        "environment.rust.components",
        before.environment.rust.components.join(","),
        after.environment.rust.components.join(","),
    );
    push_change(
        &mut changes,
        "environment.python.enabled",
        before.environment.python.enabled,
        after.environment.python.enabled,
    );
    push_change(
        &mut changes,
        "environment.go.enabled",
        before.environment.go.enabled,
        after.environment.go.enabled,
    );
    push_change(
        &mut changes,
        "environment.typescript.enabled",
        before.environment.typescript.enabled,
        after.environment.typescript.enabled,
    );
    push_change(
        &mut changes,
        "environment.typescript.packages",
        before.environment.typescript.packages.join(","),
        after.environment.typescript.packages.join(","),
    );
    changes
}

fn option_value(value: Option<&str>) -> &str {
    value.unwrap_or("not set")
}

fn enabled_name(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}

fn toolchain_row_title(name: &str, enabled: bool) -> String {
    if enabled {
        format!("✓ {name}")
    } else {
        name.to_owned()
    }
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

fn active_security_preset(config: &Config) -> Option<SecurityPreset> {
    if matches_default_compatible(config) {
        Some(SecurityPreset::DefaultCompatible)
    } else if matches_safer(config) {
        Some(SecurityPreset::Safer)
    } else if matches_strict(config) {
        Some(SecurityPreset::Strict)
    } else {
        None
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

fn git_identity_preview(config: &Config) -> Vec<String> {
    match docker::effective_git_identity(config) {
        Some(identity) => vec![
            format!("Effective: {} <{}>", identity.name, identity.email),
            format!("Source: {}", identity.source),
        ],
        None => vec![
            "Effective: not configured".to_owned(),
            "Set git.user_name/git.user_email, selected-key Git metadata, or enable host inheritance."
                .to_owned(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_default_compatible_preset() {
        let config = Config::default();

        assert_eq!(
            active_security_preset(&config),
            Some(SecurityPreset::DefaultCompatible)
        );
    }

    #[test]
    fn truncation_respects_terminal_width() {
        let truncated = truncate_to_width("abcdef", 5);

        assert_eq!(UnicodeWidthStr::width(truncated.as_str()), 5);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn top_level_menu_is_minimal() {
        let sections: Vec<_> = SECTIONS.iter().map(|section| section.title()).collect();

        assert_eq!(sections, vec!["Security", "Environment", "SSH", "Advanced"]);
    }

    #[test]
    fn key_help_uses_enter_for_activation_and_escape_for_back_or_quit() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);
        let mut output = Vec::new();

        render_keys(&mut output, &state).unwrap();
        let root_help = String::from_utf8(output).unwrap();
        assert!(root_help.contains("enter open"));
        assert!(root_help.contains("esc/q quit"));

        state.screen = ConfigScreen::Section(ConfigSection::Advanced);
        let mut output = Vec::new();
        render_keys(&mut output, &state).unwrap();
        let section_help = String::from_utf8(output).unwrap();
        assert!(section_help.contains("enter activate"));
        assert!(section_help.contains("esc back"));
        assert!(!section_help.contains("space"));
        assert!(!section_help.contains("backspace"));
    }

    #[test]
    fn plain_tui_styles_omit_ansi_sequences() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let state = ConfigUiState::new(config, paths);
        let mut output = Vec::new();

        render_header(&mut output, &state, TuiStyles::plain()).unwrap();
        let header = String::from_utf8(output).unwrap();

        assert!(!header.contains("\x1b["));
        assert!(header.contains("vegasroom config"));
    }

    #[test]
    fn safer_preset_is_detected() {
        let mut config = Config::default();
        config.workspace.risky_mount_policy = RiskyMountPolicy::Deny;

        assert_eq!(active_security_preset(&config), Some(SecurityPreset::Safer));
    }

    #[test]
    fn strict_preset_is_detected() {
        let mut config = Config::default();
        config.workspace.risky_mount_policy = RiskyMountPolicy::Deny;
        config.harness.pi.read_only_workspace = true;
        config.harness.pi.read_only_rootfs = true;
        config.ssh.mode = SshMode::Managed;
        config.git.inherit_host = false;

        assert_eq!(
            active_security_preset(&config),
            Some(SecurityPreset::Strict)
        );
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
    fn security_section_only_lists_presets() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let rows = ConfigSection::SecurityPreset.rows(&config, &paths);
        let titles: Vec<_> = rows.iter().map(|row| row.title.as_str()).collect();

        assert_eq!(titles, vec!["Default / Compatible", "Safer", "Strict"]);
    }

    #[test]
    fn environment_section_render_includes_toolchain_state_and_cache_details() {
        let mut config = Config::default();
        config.environment.rust.enabled = true;
        config.environment.typescript.packages = vec!["typescript".to_owned(), "tsx".to_owned()];
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let state = ConfigUiState::new(config, paths);

        let output = render_section_to_string(&state, ConfigSection::Environment);

        assert!(output.contains("Current: enabled (stable)"));
        assert!(output.contains("Current: disabled"));
        assert!(output.contains("Current: disabled; packages: typescript, tsx"));
        assert!(output.contains("Removes npm/pip download caches"));
        assert!(output.contains("Preserves workspaces, auth, SSH, Pi npm-global, and Cargo bin"));
    }

    #[test]
    fn advanced_section_render_includes_git_identity_and_color_values() {
        let mut config = Config::default();
        config.git.inherit_host = false;
        config.git.user_name = Some("Configured User".to_owned());
        config.git.user_email = Some("configured@example.com".to_owned());
        config.ui.color = ColorMode::Never;
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let state = ConfigUiState::new(config, paths);

        let output = render_section_to_string(&state, ConfigSection::Advanced);

        assert!(output.contains("Current: false"));
        assert!(output.contains("Current: Configured User"));
        assert!(output.contains("Current: configured@example.com"));
        assert!(output.contains("Effective: Configured User <configured@example.com>"));
        assert!(output.contains("Current: never"));
    }

    #[test]
    fn section_detail_rendering_keeps_line_count_stable_as_highlight_moves() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);

        state.highlighted_row = 0;
        let first_output = render_section_to_string(&state, ConfigSection::Advanced);
        state.highlighted_row = 4;
        let second_output = render_section_to_string(&state, ConfigSection::Advanced);

        assert_eq!(first_output.lines().count(), second_output.lines().count());
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
    fn ssh_key_configuration_is_blocked_when_dirty() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);
        state.highlighted_section = SECTIONS
            .iter()
            .position(|section| matches!(section, ConfigSection::Ssh))
            .unwrap();
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
        state.highlighted_section = SECTIONS
            .iter()
            .position(|section| matches!(section, ConfigSection::Ssh))
            .unwrap();

        let action = state.open_highlighted();

        assert!(matches!(action, ConfigUiAction::OpenSshConfigure));
    }

    #[test]
    fn git_identity_editor_toggles_host_inheritance() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);

        state.toggle_git_inherit_host();

        assert!(state.dirty);
        assert!(!state.config.git.inherit_host);
        assert!(state
            .last_message
            .as_deref()
            .is_some_and(|message| message.contains("Press s to save")));
    }

    #[test]
    fn git_identity_preview_prefers_configured_identity() {
        let mut config = Config::default();
        config.git.user_name = Some("Configured User".to_owned());
        config.git.user_email = Some("configured@example.com".to_owned());
        config.git.inherit_host = false;

        let preview = git_identity_preview(&config);

        assert!(preview
            .iter()
            .any(|line| line.contains("Configured User <configured@example.com>")));
        assert!(preview.iter().any(|line| line.contains("git.user_name")));
    }

    #[test]
    fn advanced_section_exposes_validation_backup_and_reset_rows() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let rows = ConfigSection::Advanced.rows(&config, &paths);

        assert!(rows
            .iter()
            .any(|row| row.title == "Validate current config"));
        assert!(rows
            .iter()
            .any(|row| row.title == "Recovery backup during save"));
        assert!(rows.iter().any(|row| row.title == "Reset all to defaults"));
    }

    #[test]
    fn reset_defaults_preview_lists_expected_changes() {
        let mut config = Config::default();
        config.ssh.mode = SshMode::Managed;
        config.ui.color = ColorMode::Never;

        let changes = reset_defaults_changes(&config);

        assert!(changes
            .iter()
            .any(|change| change.field == "ssh.mode" && change.before == "managed"));
        assert!(changes
            .iter()
            .any(|change| change.field == "ui.color" && change.before == "never"));
    }

    #[test]
    fn applying_reset_defaults_marks_dirty_and_restores_defaults() {
        let mut config = Config::default();
        config.ssh.mode = SshMode::Managed;
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);

        state.apply_reset_defaults();

        assert!(state.dirty);
        assert_eq!(state.config.ssh.mode, SshMode::Auto);
        assert!(matches!(
            state.screen,
            ConfigScreen::Section(ConfigSection::Advanced)
        ));
    }

    #[test]
    fn validate_config_reports_success() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);

        state.validate_config().unwrap();

        assert!(state
            .last_message
            .as_deref()
            .is_some_and(|message| message.contains("validates successfully")));
    }

    #[test]
    fn save_config_removes_recovery_backup_after_validated_save() {
        let dir = unique_temp_dir("save-config-backup");
        fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("config.yaml");

        Config::default().save_to_path(&config_path).unwrap();

        let mut changed = Config::default();
        changed.paths.workspace = "/tmp/changed-workspace".to_owned();

        save_config_with_recovery_backup(&changed, &config_path).unwrap();

        assert_eq!(
            Config::load_from_path(config_path).unwrap().paths.workspace,
            "/tmp/changed-workspace"
        );
        assert!(backup_files(&dir).is_empty());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn save_config_keeps_recovery_backup_when_save_fails() {
        let dir = unique_temp_dir("save-config-failed-backup");
        fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("config.yaml");

        Config::default().save_to_path(&config_path).unwrap();
        let original = fs::read_to_string(&config_path).unwrap();

        let mut invalid = Config::default();
        invalid.paths.workspace = "".to_owned();

        let err = save_config_with_recovery_backup(&invalid, &config_path).unwrap_err();
        let backups = backup_files(&dir);

        assert!(err.to_string().contains("paths.workspace"));
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read_to_string(&backups[0]).unwrap(), original);

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
            .is_some_and(|message| message.starts_with("Saved config to ")));
        assert!(backup_files(&dir).is_empty());

        let _ = fs::remove_dir_all(dir);
    }

    fn render_section_to_string(state: &ConfigUiState, section: ConfigSection) -> String {
        let mut output = Vec::new();
        render_section_screen(&mut output, state, section, TuiStyles::plain()).unwrap();
        String::from_utf8(output).unwrap()
    }

    fn backup_files(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.contains(".backup-"))
            })
            .collect()
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
