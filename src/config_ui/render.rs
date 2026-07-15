use std::io::{self, IsTerminal, Write};

use anyhow::{Context, Result};
use crossterm::{
    cursor, execute,
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{alert, config::Config, paths::display_path};

use super::{
    active_security_preset, package_cache_estimates, preset_changes, reset_defaults_changes,
    total_package_cache_bytes, ConfigScreen, ConfigSection, ConfigUiState, RowAction, SectionRow,
    SecurityPreset, TextField, SECTIONS,
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
        writeln!(buffer, "│")?;
        writeln!(buffer, "│  Notice  {message}")?;
    }

    writeln!(buffer, "│")?;
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
        styles.bold("Unsaved")
    } else {
        styles.green("Saved")
    };
    writeln!(stdout, "╭─ {} · {status}", styles.bold("Vegasroom Config"))?;
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
    writeln!(stdout, "│  {}", styles.dim("Choose a Section"))?;
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
    if matches!(section, ConfigSection::Ssh) {
        return render_ssh_key_section(stdout, state, styles);
    }

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

fn render_ssh_key_section(
    stdout: &mut impl Write,
    state: &ConfigUiState,
    styles: TuiStyles,
) -> Result<()> {
    writeln!(
        stdout,
        "│  {}",
        styles.dim(&format!(
            "SSH Keys · {} Selected",
            state.selected_ssh_key_count()
        ))
    )?;

    if state.ssh_keys.is_empty() {
        writeln!(stdout, "│  No SSH private keys detected.")?;
        writeln!(stdout, "│      {}", styles.dim("Press r to rescan."))?;
        return Ok(());
    }

    let (_, height) = terminal::size().unwrap_or((100, 30));
    let list_rows = usize::from(height).saturating_sub(16).max(4);
    let (start, end) = visible_list_window(state.ssh_keys.len(), state.highlighted_row, list_rows);
    writeln!(
        stdout,
        "│  {}",
        styles.dim(&format!(
            "Keys {}-{} of {}",
            start + 1,
            end,
            state.ssh_keys.len()
        ))
    )?;

    if start > 0 {
        writeln!(stdout, "│      {}", styles.dim("↑ more"))?;
    }

    for index in start..end {
        let key = &state.ssh_keys[index];
        let selected = state.ssh_selected.get(index).copied().unwrap_or(false);
        let highlighted = index == state.highlighted_row;
        let marker = if highlighted { "›" } else { " " };
        let checkbox = if selected { "✓" } else { "○" };
        let title = format!("{checkbox} {}", key.display_path);
        let title = match (selected, highlighted) {
            (true, true) => styles.green_bold(&title),
            (true, false) => styles.green(&title),
            (false, true) => styles.bold(&title),
            (false, false) => title,
        };
        writeln!(stdout, "│  {marker} {title}")?;
    }

    if end < state.ssh_keys.len() {
        writeln!(stdout, "│      {}", styles.dim("↓ more"))?;
    }

    if let Some(key) = state.ssh_keys.get(state.highlighted_row) {
        let selected = state
            .ssh_selected
            .get(state.highlighted_row)
            .copied()
            .unwrap_or(false);
        let selected_label = if selected { "Selected" } else { "Not Selected" };
        let key_type = key.key_type.as_deref().unwrap_or("unknown");
        let fingerprint = key.fingerprint.as_deref().unwrap_or("unknown fingerprint");
        let comment = key.comment.as_deref().unwrap_or("no comment");
        let public_pair = if key.has_public_pair { "yes" } else { "no" };
        let permissions = match key.permissions_ok {
            Some(true) => "ok",
            Some(false) => "broad",
            None => "unknown",
        };

        writeln!(stdout, "│")?;
        writeln!(
            stdout,
            "│  {}",
            styles.dim(&format!(
                "Key  {selected_label} · {key_type} · Public Pair {public_pair}"
            ))
        )?;
        writeln!(stdout, "│  {}", styles.dim(&format!("FP   {fingerprint}")))?;
        writeln!(stdout, "│  {}", styles.dim(&format!("Note {comment}")))?;
        writeln!(
            stdout,
            "│  {}",
            styles.dim(&format!("Permissions {permissions}"))
        )?;
    }

    Ok(())
}

fn visible_list_window(total: usize, highlighted: usize, max_rows: usize) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }

    let row_budget = max_rows.saturating_sub(3).max(1).min(total);
    let highlighted = highlighted.min(total - 1);
    let half = row_budget / 2;

    let mut start = highlighted.saturating_sub(half);
    if start + row_budget > total {
        start = total.saturating_sub(row_budget);
    }
    let end = (start + row_budget).min(total);
    (start, end)
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

pub(super) fn render_quit_prompt(dirty: bool) -> Result<()> {
    let lines = quit_prompt_lines(dirty);
    let mut stdout = io::stdout();
    draw_bottom_panel(&mut stdout, &lines).context("failed to render config quit prompt")?;
    stdout.flush()?;
    Ok(())
}

pub(super) fn quit_prompt_lines(dirty: bool) -> Vec<String> {
    if dirty {
        vec![
            "╭─ Unsaved Config Changes".to_owned(),
            "│".to_owned(),
            "│  Save changes before quitting?".to_owned(),
            "│".to_owned(),
            "│  y  Save and Quit".to_owned(),
            "│  n  Quit Without Saving".to_owned(),
            "│  c  Cancel".to_owned(),
            "╰".to_owned(),
        ]
    } else {
        vec![
            "╭─ Quit Config".to_owned(),
            "│".to_owned(),
            "│  No unsaved changes. Quit?".to_owned(),
            "│".to_owned(),
            "│  y  Quit".to_owned(),
            "│  n  Cancel".to_owned(),
            "╰".to_owned(),
        ]
    }
}

fn render_preset_preview(
    stdout: &mut impl Write,
    state: &ConfigUiState,
    preset: SecurityPreset,
) -> Result<()> {
    writeln!(stdout, "│  Security Preset: {}", preset.title())?;
    writeln!(stdout, "│")?;
    writeln!(stdout, "│  Changes to Apply")?;

    let changes = preset_changes(&state.config, preset);
    if changes.is_empty() {
        writeln!(
            stdout,
            "│    No changes; this preset already matches current config."
        )?;
    } else {
        for change in changes {
            writeln!(
                stdout,
                "│    {}: {} -> {}",
                change.field, change.before, change.after
            )?;
        }
    }

    writeln!(stdout, "│")?;
    for line in preset.notes() {
        writeln!(stdout, "│    {line}")?;
    }

    Ok(())
}

fn render_reset_defaults_preview(stdout: &mut impl Write, state: &ConfigUiState) -> Result<()> {
    writeln!(stdout, "│  Reset All Config to Defaults")?;
    writeln!(stdout, "│")?;
    writeln!(stdout, "│  Changes to Apply")?;

    let changes = reset_defaults_changes(&state.config);
    if changes.is_empty() {
        writeln!(stdout, "│    No changes; config already matches defaults.")?;
    } else {
        for change in changes {
            writeln!(
                stdout,
                "│    {}: {} -> {}",
                change.field, change.before, change.after
            )?;
        }
    }

    writeln!(stdout, "│")?;
    writeln!(stdout, "│    This resets all config fields in memory.")?;
    writeln!(
        stdout,
        "│    Press s after applying to save the reset to disk."
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
    writeln!(stdout, "│  Value")?;
    writeln!(stdout, "│  › {}{}", state.input_buffer, styles.dim("_"))?;
    Ok(())
}

pub(super) fn render_purge_package_caches_preview(
    stdout: &mut impl Write,
    state: &ConfigUiState,
) -> Result<()> {
    let estimates = package_cache_estimates(&state.state_paths);
    let total_bytes = total_package_cache_bytes(&estimates);

    writeln!(stdout, "│  Purge Package Download Caches")?;
    writeln!(stdout, "│")?;
    writeln!(
        stdout,
        "│  Estimated removable cache: {}",
        format_bytes(total_bytes)
    )?;
    writeln!(stdout, "│")?;
    writeln!(stdout, "│  This removes safe package download caches only:")?;
    for estimate in estimates {
        writeln!(
            stdout,
            "│    {}  {}",
            format_bytes(estimate.bytes),
            display_path(&estimate.path)
        )?;
    }
    writeln!(stdout, "│")?;
    writeln!(
        stdout,
        "│  Preserves toolchain settings, auth, SSH, workspaces,"
    )?;
    writeln!(
        stdout,
        "│  Pi npm-global installs, and Cargo-installed binaries."
    )?;
    writeln!(stdout, "│")?;
    writeln!(stdout, "│  Press Enter to purge, or Esc to cancel.")?;
    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

pub(super) fn render_keys(stdout: &mut impl Write, state: &ConfigUiState) -> Result<()> {
    match state.screen {
        ConfigScreen::Sections => {
            writeln!(stdout, "╰─ ↑↓/jk move  enter open  s save  esc/q quit")?
        }
        ConfigScreen::Section(ConfigSection::Ssh) => writeln!(
            stdout,
            "╰─ ↑↓/jk move  enter toggle  r rescan  esc back  s save  q quit"
        )?,
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
