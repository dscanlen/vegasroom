use std::io::{self, IsTerminal, Write};

use anyhow::{Context, Result};
use crossterm::{
    cursor, execute,
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{alert, config::Config, paths::display_path};

use super::{
    active_security_preset, package_cache_paths, preset_changes, reset_defaults_changes,
    ConfigScreen, ConfigSection, ConfigUiState, RowAction, SectionRow, SecurityPreset, TextField,
    SECTIONS,
};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[32m";
const DIM: &str = "\x1b[2m";

#[derive(Clone, Copy)]
pub(super) struct TuiStyles {
    enabled: bool,
}

impl TuiStyles {
    fn for_config(config: &Config) -> Self {
        Self {
            enabled: alert::colors_enabled_for_config(config, io::stdout().is_terminal()),
        }
    }

    #[cfg(test)]
    pub(super) fn plain() -> Self {
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

pub(super) fn render(state: &ConfigUiState) -> Result<()> {
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
        ConfigScreen::TextInput(field) => render_text_input(&mut buffer, state, field, styles)?,
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

pub(super) fn render_header(
    stdout: &mut impl Write,
    state: &ConfigUiState,
    styles: TuiStyles,
) -> Result<()> {
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

pub(super) fn render_sections_screen(
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

pub(super) fn render_section_screen(
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

pub(super) fn render_quit_prompt() -> Result<()> {
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

pub(super) fn render_text_input(
    stdout: &mut impl Write,
    state: &ConfigUiState,
    field: TextField,
    styles: TuiStyles,
) -> Result<()> {
    writeln!(stdout, "│  {}", styles.dim("Advanced"))?;
    writeln!(stdout, "│  {}", styles.bold(field.title()))?;
    writeln!(stdout, "│")?;
    writeln!(stdout, "│  Field: {}", field.config_path())?;
    writeln!(stdout, "│  {}", field.help())?;
    writeln!(stdout, "│")?;
    writeln!(stdout, "│  Value:")?;
    writeln!(stdout, "│  › {}{}", state.input_buffer, styles.dim("_"))?;
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

pub(super) fn render_keys(stdout: &mut impl Write, state: &ConfigUiState) -> Result<()> {
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
        ConfigScreen::TextInput(_) => writeln!(
            stdout,
            "╰─ type edit  backspace delete  enter apply  esc cancel"
        )?,
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

pub(super) fn truncate_to_width(text: &str, width: u16) -> String {
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

pub(super) struct TerminalSession;

impl TerminalSession {
    pub(super) fn start() -> Result<Self> {
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
