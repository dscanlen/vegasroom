use std::io::{self, IsTerminal, Write};

use anyhow::{Context, Result};
use crossterm::{
    cursor, execute,
    terminal::{self, ClearType},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{alert, config::Config};

use super::{ConfigureUiState, GREEN, RESET};

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";

pub(super) fn render_configure_ui(state: &ConfigureUiState) -> Result<()> {
    let mut stdout = io::stdout();
    let (width, height) = terminal::size().unwrap_or((100, 30));
    let width = width.max(1);
    let height = height.max(1);

    let lines = build_configure_ui_lines(state, width, height);
    let colors_enabled =
        alert::colors_enabled_for_config(&state.config, io::stdout().is_terminal());
    draw_tui_lines(&mut stdout, width, height, &lines, colors_enabled)
        .context("failed to redraw SSH configure UI")?;
    stdout.flush()?;
    Ok(())
}

pub(super) fn render_quit_prompt() -> Result<()> {
    let mut stdout = io::stdout();
    let (width, height) = terminal::size().unwrap_or((100, 30));
    let width = width.max(1);
    let height = height.max(1);

    let lines = vec![
        TuiLine::highlighted("╭─ Unsaved SSH Key Changes"),
        TuiLine::normal("│"),
        TuiLine::normal("│  Save changes before quitting?"),
        TuiLine::normal("│"),
        TuiLine::normal("│  y  Save and Quit"),
        TuiLine::normal("│  n  Quit Without Saving"),
        TuiLine::normal("│  c  Cancel"),
        TuiLine::normal("╰"),
    ];

    let config = Config::load_or_default().unwrap_or_default();
    let colors_enabled = alert::colors_enabled_for_config(&config, io::stdout().is_terminal());
    draw_tui_lines(&mut stdout, width, height, &lines, colors_enabled)
        .context("failed to draw SSH configure quit prompt")?;
    stdout.flush()?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum TuiLineStyle {
    Normal,
    Dim,
    Highlighted,
    Selected,
    SelectedHighlighted,
}

#[derive(Debug, Clone)]
struct TuiLine {
    text: String,
    style: TuiLineStyle,
}

impl TuiLine {
    fn normal(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: TuiLineStyle::Normal,
        }
    }

    fn dim(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: TuiLineStyle::Dim,
        }
    }

    fn highlighted(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: TuiLineStyle::Highlighted,
        }
    }

    fn selected(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: TuiLineStyle::Selected,
        }
    }

    fn selected_highlighted(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: TuiLineStyle::SelectedHighlighted,
        }
    }
}

fn build_configure_ui_lines(state: &ConfigureUiState, width: u16, height: u16) -> Vec<TuiLine> {
    let dirty = if state.is_dirty() { "Unsaved" } else { "Saved" };
    let selected_count = state.selected_count();
    let mut lines = vec![
        TuiLine::highlighted(format!("╭─ SSH Keys · {selected_count} Selected · {dirty}")),
        TuiLine::dim("│  Select Managed SSH Keys for Vegasroom"),
        TuiLine::normal("│"),
    ];

    let detail_rows = usize::from(!state.keys.is_empty()) * 5;
    let message_rows = usize::from(state.last_message.is_some()) * 2;
    let fixed_rows = lines.len() + detail_rows + message_rows + 2;
    let key_rows = usize::from(height).saturating_sub(fixed_rows).max(1);
    append_key_list_lines(&mut lines, state, width, key_rows);
    append_highlighted_key_detail_lines(&mut lines, state, width);

    if let Some(message) = &state.last_message {
        lines.push(TuiLine::normal("│"));
        lines.push(TuiLine::normal(truncate_to_width(
            &format!("│  Notice  {message}"),
            width,
        )));
    }

    lines.push(TuiLine::normal("│"));
    lines.push(TuiLine::normal(
        "╰─ ↑↓/jk move  enter toggle  r rescan  s save  esc/q quit",
    ));
    lines
}

fn append_key_list_lines(
    lines: &mut Vec<TuiLine>,
    state: &ConfigureUiState,
    width: u16,
    list_rows: usize,
) {
    if state.keys.is_empty() {
        lines.push(TuiLine::normal("│  No SSH Private Keys Detected"));
        return;
    }

    let (start, end) = visible_list_window(state.keys.len(), state.highlighted, list_rows);
    lines.push(TuiLine::dim(truncate_to_width(
        &format!("│  Keys {}-{} of {}", start + 1, end, state.keys.len()),
        width,
    )));

    if start > 0 {
        lines.push(TuiLine::dim("│    ↑ more"));
    }

    for index in start..end {
        let key = &state.keys[index];
        let selected = state.selected.get(index).copied().unwrap_or(false);
        let highlighted = index == state.highlighted;
        let cursor = if highlighted { "›" } else { " " };
        let checkbox = if selected { "✓" } else { "○" };
        let row = truncate_to_width(
            &format!("│  {cursor} {checkbox} {}", key.display_path),
            width,
        );

        let line = match (selected, highlighted) {
            (true, true) => TuiLine::selected_highlighted(row),
            (true, false) => TuiLine::selected(row),
            (false, true) => TuiLine::highlighted(row),
            (false, false) => TuiLine::normal(row),
        };
        lines.push(line);
    }

    if end < state.keys.len() {
        lines.push(TuiLine::dim("│    ↓ more"));
    }
}

fn append_highlighted_key_detail_lines(
    lines: &mut Vec<TuiLine>,
    state: &ConfigureUiState,
    width: u16,
) {
    let Some(key) = state.keys.get(state.highlighted) else {
        return;
    };

    let selected = state
        .selected
        .get(state.highlighted)
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

    lines.push(TuiLine::normal("│"));
    lines.push(TuiLine::dim(truncate_to_width(
        &format!("│  Key  {selected_label} · {key_type} · Public Pair {public_pair}"),
        width,
    )));
    lines.push(TuiLine::dim(truncate_to_width(
        &format!("│  FP   {fingerprint}"),
        width,
    )));
    lines.push(TuiLine::dim(truncate_to_width(
        &format!("│  Note {comment}"),
        width,
    )));
    lines.push(TuiLine::dim(truncate_to_width(
        &format!("│  Permissions {permissions}"),
        width,
    )));
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

fn truncate_to_width(text: &str, width: u16) -> String {
    let max_width = usize::from(width).max(1);
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_owned();
    }

    let ellipsis = "…";
    let ellipsis_width = UnicodeWidthStr::width(ellipsis);
    let target_width = max_width.saturating_sub(ellipsis_width).max(1);
    let mut out = String::new();
    let mut used = 0usize;

    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > target_width {
            break;
        }
        out.push(ch);
        used += w;
    }

    out.push_str(ellipsis);
    out
}

fn draw_tui_lines(
    stdout: &mut io::Stdout,
    width: u16,
    height: u16,
    lines: &[TuiLine],
    colors_enabled: bool,
) -> Result<()> {
    execute!(stdout, terminal::Clear(ClearType::All))?;

    let max_rows = usize::from(height);
    let visible_lines = if lines.len() > max_rows {
        &lines[lines.len() - max_rows..]
    } else {
        lines
    };
    let start_row = height.saturating_sub(visible_lines.len() as u16);

    for (index, line) in visible_lines.iter().enumerate() {
        execute!(
            stdout,
            cursor::MoveTo(0, start_row + index as u16),
            terminal::Clear(ClearType::CurrentLine)
        )?;

        let text = truncate_to_width(&line.text, width);
        match (colors_enabled, line.style) {
            (_, TuiLineStyle::Normal) | (false, _) => write!(stdout, "{text}")?,
            (true, TuiLineStyle::Dim) => write!(stdout, "{DIM}{text}{RESET}")?,
            (true, TuiLineStyle::Highlighted) => write!(stdout, "{BOLD}{text}{RESET}")?,
            (true, TuiLineStyle::Selected) => write!(stdout, "{GREEN}{text}{RESET}")?,
            (true, TuiLineStyle::SelectedHighlighted) => {
                write!(stdout, "{GREEN}{BOLD}{text}{RESET}")?
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::config::Config;

    #[test]
    fn truncation_respects_display_width_budget() {
        let truncated = truncate_to_width("abcdef", 5);

        assert_eq!(UnicodeWidthStr::width(truncated.as_str()), 5);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn list_window_keeps_highlight_visible() {
        let (start, end) = visible_list_window(20, 10, 8);

        assert!(start <= 10);
        assert!(end > 10);
    }

    #[test]
    fn help_uses_enter_for_toggle_and_escape_for_quit() {
        let state = ConfigureUiState::new(
            Vec::new(),
            Vec::new(),
            Config::default(),
            vec![PathBuf::from("/tmp")],
            false,
        );

        let lines = build_configure_ui_lines(&state, 100, 30);
        let footer = &lines.last().unwrap().text;

        assert!(footer.contains("enter toggle"));
        assert!(footer.contains("esc/q quit"));
        assert!(!footer.contains("space"));
    }

    #[test]
    fn menu_keeps_pipe_spacer_before_footer() {
        let state = ConfigureUiState::new(
            Vec::new(),
            Vec::new(),
            Config::default(),
            vec![PathBuf::from("/tmp")],
            false,
        );

        let lines = build_configure_ui_lines(&state, 100, 30);

        assert_eq!(lines[lines.len() - 2].text, "│");
        assert!(lines[0].text.contains("SSH Keys"));
    }
}
