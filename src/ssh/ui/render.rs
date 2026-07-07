use std::io::{self, Write};

use anyhow::{Context, Result};
use crossterm::{
    cursor, execute,
    terminal::{self, ClearType},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{ConfigureUiState, GREEN, RESET};

pub(super) fn render_configure_ui(state: &ConfigureUiState) -> Result<()> {
    let mut stdout = io::stdout();
    let (width, height) = terminal::size().unwrap_or((100, 30));
    let width = width.max(1);
    let height = height.max(1);

    let lines = build_configure_ui_lines(state, width, height);
    draw_tui_lines(&mut stdout, width, height, &lines)
        .context("failed to redraw SSH configure UI")?;
    stdout.flush()?;
    Ok(())
}

pub(super) fn render_quit_prompt() -> Result<()> {
    let mut stdout = io::stdout();
    let (width, height) = terminal::size().unwrap_or((100, 30));
    let width = width.max(1);
    let height = height.max(1);

    let mut lines = Vec::new();
    for line in wrap_text_to_width("You have unsaved SSH key selection changes.", width, "", "") {
        lines.push(TuiLine::normal(line));
    }
    lines.push(TuiLine::normal(""));
    lines.push(TuiLine::normal("Save before quitting?"));
    lines.push(TuiLine::normal("  [y] save and quit"));
    lines.push(TuiLine::normal("  [n] discard and quit"));

    draw_tui_lines(&mut stdout, width, height, &lines)
        .context("failed to draw SSH configure quit prompt")?;
    stdout.flush()?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum TuiLineStyle {
    Normal,
    Selected,
    Highlighted,
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

    fn selected(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: TuiLineStyle::Selected,
        }
    }

    fn highlighted(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: TuiLineStyle::Highlighted,
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
    let mut lines = Vec::new();

    lines.push(TuiLine::normal("Vegasroom SSH Key Configuration"));
    for line in wrap_text_to_width(
        "Use ↑/↓ or k/j to move, Enter or Space to select, s to save, q to quit, r to rescan.",
        width,
        "",
        "",
    ) {
        lines.push(TuiLine::normal(line));
    }
    lines.push(TuiLine::normal(""));

    let dirty = if state.is_dirty() {
        "unsaved changes"
    } else {
        "saved"
    };
    let mut footer = Vec::new();
    footer.push(TuiLine::normal(""));
    for line in wrap_text_to_width("Actions: [s Save]  [q Quit]", width, "", "") {
        footer.push(TuiLine::normal(line));
    }
    for line in wrap_text_to_width(
        &format!("Status: {} selected · {dirty}", state.selected_count()),
        width,
        "",
        "",
    ) {
        footer.push(TuiLine::normal(line));
    }
    if let Some(message) = &state.last_message {
        for line in wrap_text_to_width(message, width, "", "") {
            footer.push(TuiLine::normal(line));
        }
    }

    let used_fixed_rows = lines.len() + footer.len();
    let available_rows = usize::from(height).saturating_sub(used_fixed_rows).max(1);

    if state.keys.is_empty() {
        for line in wrap_text_to_width("No SSH private keys were detected.", width, "", "") {
            lines.push(TuiLine::normal(line));
        }
    } else {
        let detail_rows = available_rows
            .clamp(6, 10)
            .min(available_rows.saturating_sub(3));
        let list_rows = available_rows.saturating_sub(detail_rows).max(1);
        append_key_list_lines(&mut lines, state, width, list_rows);
        append_highlighted_key_detail_lines(&mut lines, state, width, detail_rows);
    }

    lines.extend(footer);
    lines
}

fn append_key_list_lines(
    lines: &mut Vec<TuiLine>,
    state: &ConfigureUiState,
    width: u16,
    list_rows: usize,
) {
    let (start, end) = visible_list_window(state.keys.len(), state.highlighted, list_rows);

    let list_title = format!(
        "Keys: showing {}-{} of {}",
        start + 1,
        end,
        state.keys.len()
    );
    lines.push(TuiLine::normal(truncate_to_width(&list_title, width)));

    if start > 0 {
        lines.push(TuiLine::normal("  ↑ more keys above"));
    }

    for index in start..end {
        let key = &state.keys[index];
        let selected = state.selected.get(index).copied().unwrap_or(false);
        let highlighted = index == state.highlighted;
        let cursor = if highlighted { ">" } else { " " };
        let checkbox = if selected { "☑" } else { "☐" };
        let row = truncate_to_width(&format!("{cursor} {checkbox} {}", key.display_path), width);

        let line = match (selected, highlighted) {
            (true, true) => TuiLine::selected_highlighted(row),
            (true, false) => TuiLine::selected(row),
            (false, true) => TuiLine::highlighted(row),
            (false, false) => TuiLine::normal(row),
        };
        lines.push(line);
    }

    if end < state.keys.len() {
        lines.push(TuiLine::normal("  ↓ more keys below"));
    }
}

fn append_highlighted_key_detail_lines(
    lines: &mut Vec<TuiLine>,
    state: &ConfigureUiState,
    width: u16,
    detail_rows: usize,
) {
    if detail_rows == 0 {
        return;
    }

    lines.push(TuiLine::normal(""));
    lines.push(TuiLine::normal("Details"));

    let Some(key) = state.keys.get(state.highlighted) else {
        return;
    };

    let selected = state
        .selected
        .get(state.highlighted)
        .copied()
        .unwrap_or(false);
    let mut detail_lines = Vec::new();
    detail_lines.extend(wrap_text_to_width(
        &format!("Path: {}", key.display_path),
        width,
        "  ",
        "       ",
    ));
    detail_lines.extend(wrap_text_to_width(
        &format!(
            "Key: {}{}{}",
            key.key_type.as_deref().unwrap_or("unknown"),
            key.fingerprint
                .as_deref()
                .map(|fp| format!(" {fp}"))
                .unwrap_or_default(),
            key.comment
                .as_deref()
                .map(|comment| format!(" {comment}"))
                .unwrap_or_default(),
        ),
        width,
        "  ",
        "       ",
    ));
    detail_lines.push(format!(
        "  Public pair: {}",
        if key.has_public_pair { "yes" } else { "no" }
    ));
    if let Some(false) = key.permissions_ok {
        detail_lines.extend(wrap_text_to_width(
            "Permissions appear broad",
            width,
            "  ",
            "       ",
        ));
    }

    let max_detail_lines = detail_rows.saturating_sub(2).max(1);
    for line in detail_lines.into_iter().take(max_detail_lines) {
        if selected {
            lines.push(TuiLine::selected(line));
        } else {
            lines.push(TuiLine::normal(line));
        }
    }
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
    let max_width = usize::from(width).saturating_sub(2).max(1);
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
) -> Result<()> {
    execute!(stdout, terminal::Clear(ClearType::All))?;

    let max_rows = usize::from(height);
    for (row, line) in lines.iter().take(max_rows).enumerate() {
        execute!(
            stdout,
            cursor::MoveTo(0, row as u16),
            terminal::Clear(ClearType::CurrentLine)
        )?;

        let text = truncate_to_width(&line.text, width);
        match line.style {
            TuiLineStyle::Normal => write!(stdout, "{text}")?,
            TuiLineStyle::Selected => write!(stdout, "{GREEN}{text}{RESET}")?,
            TuiLineStyle::Highlighted => write!(stdout, "\x1b[7m{text}{RESET}")?,
            TuiLineStyle::SelectedHighlighted => write!(stdout, "\x1b[32;7m{text}{RESET}")?,
        }
    }

    Ok(())
}

fn wrap_text_to_width(
    text: &str,
    width: u16,
    first_prefix: &str,
    continuation_prefix: &str,
) -> Vec<String> {
    let max_width = usize::from(width).saturating_sub(2).max(1);
    let mut lines = Vec::new();
    let mut remaining = text.trim_end();
    let mut prefix = first_prefix;

    if remaining.is_empty() {
        lines.push(first_prefix.to_owned());
        return lines;
    }

    while !remaining.is_empty() {
        let prefix_width = UnicodeWidthStr::width(prefix);
        let content_width = max_width.saturating_sub(prefix_width).max(1);
        let (chunk, rest) = take_wrapped_chunk(remaining, content_width);
        lines.push(format!("{prefix}{chunk}"));
        remaining = rest.trim_start();
        prefix = continuation_prefix;
    }

    lines
}

fn take_wrapped_chunk(input: &str, max_width: usize) -> (&str, &str) {
    if UnicodeWidthStr::width(input) <= max_width {
        return (input, "");
    }

    let mut boundary = input.len();
    let mut used_width = 0usize;
    let mut saw_char = false;

    for (index, ch) in input.char_indices() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if saw_char && used_width + char_width > max_width {
            boundary = index;
            break;
        }

        if !saw_char && char_width > max_width {
            boundary = index + ch.len_utf8();
            break;
        }

        used_width += char_width;
        saw_char = true;
    }

    let hard_chunk = &input[..boundary];
    if let Some(split_at) = last_whitespace_boundary(hard_chunk) {
        if split_at > 0 {
            let chunk = input[..split_at].trim_end();
            let rest = &input[split_at..];
            return (chunk, rest);
        }
    }

    (&input[..boundary], &input[boundary..])
}

fn last_whitespace_boundary(input: &str) -> Option<usize> {
    input
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_respects_display_width_budget() {
        let truncated = truncate_to_width("abcdef", 5);

        assert_eq!(UnicodeWidthStr::width(truncated.as_str()), 3);
        assert!(truncated.ends_with('…'));
    }
}
