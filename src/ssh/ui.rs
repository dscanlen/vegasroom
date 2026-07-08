use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    alert,
    config::{Config, SelectedSshKey, SshMode},
    paths::{display_path, expand_tilde, StatePaths},
};

use super::{
    discovery::{discover_keys, discovery_roots, initial_selection},
    DiscoveredSshKey,
};

const GREEN: &str = "\x1b[32m";
const RESET: &str = "\x1b[0m";

pub fn configure(paths: &[String], follow_symlinks: bool) -> Result<i32> {
    let roots = discovery_roots(paths)?;
    println!("Scanning SSH key roots:");
    for root in &roots {
        println!("  {}", display_path(root));
    }
    if follow_symlinks {
        println!(
            "{}: following symlinks can scan outside the requested roots.",
            alert::warn()
        );
    }

    let mut discovered = discover_keys(&roots, follow_symlinks)?;
    discovered.sort_by(|a, b| a.display_path.cmp(&b.display_path));

    if discovered.is_empty() {
        println!("No SSH private keys were detected.");
        return Ok(0);
    }

    let config = Config::load_or_default()?;
    let selected = initial_selection(&discovered, &config.ssh.selected_keys);

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return configure_line_mode(discovered, selected, config);
    }

    configure_tui(discovered, selected, config, roots, follow_symlinks)
}

fn configure_line_mode(
    discovered: Vec<DiscoveredSshKey>,
    mut selected: Vec<bool>,
    mut config: Config,
) -> Result<i32> {
    loop {
        print_selector(&discovered, &selected)?;
        print!("Command [number toggles, s save, q quit]: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.eq_ignore_ascii_case("q") {
            println!("No SSH configuration changes saved.");
            return Ok(0);
        }

        if input.eq_ignore_ascii_case("s") || input.is_empty() {
            save_selected_keys(&mut config, &discovered, &selected)?;
            println!(
                "Saved {} selected SSH key(s) to {}",
                config.ssh.selected_keys.len(),
                display_path(&StatePaths::default()?.config_yaml)
            );
            return Ok(0);
        }

        for token in input
            .split(|ch: char| ch == ',' || ch.is_whitespace())
            .filter(|token| !token.is_empty())
        {
            match token.parse::<usize>() {
                Ok(index) if index >= 1 && index <= selected.len() => {
                    let slot = &mut selected[index - 1];
                    *slot = !*slot;
                }
                _ => println!("Ignoring invalid selection: {token}"),
            }
        }
    }
}

fn configure_tui(
    discovered: Vec<DiscoveredSshKey>,
    selected: Vec<bool>,
    config: Config,
    roots: Vec<PathBuf>,
    follow_symlinks: bool,
) -> Result<i32> {
    let _terminal = TerminalSession::start()?;
    let mut state = ConfigureUiState::new(discovered, selected, config, roots, follow_symlinks);

    loop {
        render_configure_ui(&state)?;

        let Event::Key(key) = event::read().context("failed to read terminal key event")? else {
            continue;
        };

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => state.move_up(),
            KeyCode::Down | KeyCode::Char('j') => state.move_down(),
            KeyCode::Enter | KeyCode::Char(' ') => state.toggle_highlighted(),
            KeyCode::Char('s') => {
                state.save()?;
            }
            KeyCode::Char('r') => {
                state.rescan()?;
            }
            KeyCode::Char('q') => {
                if !state.is_dirty() {
                    return Ok(0);
                }

                render_quit_prompt()?;
                loop {
                    let Event::Key(confirm) =
                        event::read().context("failed to read terminal key event")?
                    else {
                        continue;
                    };

                    match confirm.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            state.save()?;
                            return Ok(0);
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') => {
                            return Ok(0);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
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

struct ConfigureUiState {
    keys: Vec<DiscoveredSshKey>,
    selected: Vec<bool>,
    original_selected: Vec<SelectedSshKey>,
    highlighted: usize,
    last_message: Option<String>,
    config: Config,
    roots: Vec<PathBuf>,
    follow_symlinks: bool,
}

impl ConfigureUiState {
    fn new(
        keys: Vec<DiscoveredSshKey>,
        selected: Vec<bool>,
        config: Config,
        roots: Vec<PathBuf>,
        follow_symlinks: bool,
    ) -> Self {
        let original_selected = selected_keys_from(&keys, &selected, &config.ssh.selected_keys);
        Self {
            keys,
            selected,
            original_selected,
            highlighted: 0,
            last_message: None,
            config,
            roots,
            follow_symlinks,
        }
    }

    fn move_up(&mut self) {
        if self.keys.is_empty() {
            return;
        }
        if self.highlighted == 0 {
            self.highlighted = self.keys.len() - 1;
        } else {
            self.highlighted -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.keys.is_empty() {
            return;
        }
        self.highlighted = (self.highlighted + 1) % self.keys.len();
    }

    fn toggle_highlighted(&mut self) {
        if let Some(slot) = self.selected.get_mut(self.highlighted) {
            *slot = !*slot;
            self.last_message = None;
        }
    }

    fn save(&mut self) -> Result<()> {
        save_selected_keys(&mut self.config, &self.keys, &self.selected)?;
        self.original_selected = self.config.ssh.selected_keys.clone();
        self.last_message = Some(format!(
            "Saved {} selected SSH key(s) to {}",
            self.config.ssh.selected_keys.len(),
            display_path(&StatePaths::default()?.config_yaml)
        ));
        Ok(())
    }

    fn rescan(&mut self) -> Result<()> {
        let current_selected =
            selected_keys_from(&self.keys, &self.selected, &self.config.ssh.selected_keys);
        let mut keys = discover_keys(&self.roots, self.follow_symlinks)?;
        keys.sort_by(|a, b| a.display_path.cmp(&b.display_path));
        let selected = initial_selection(&keys, &current_selected);

        self.keys = keys;
        self.selected = selected;
        if self.highlighted >= self.keys.len() {
            self.highlighted = self.keys.len().saturating_sub(1);
        }
        self.last_message = Some("Rescanned SSH key roots.".to_owned());
        Ok(())
    }

    fn is_dirty(&self) -> bool {
        selected_keys_from(&self.keys, &self.selected, &self.config.ssh.selected_keys)
            != self.original_selected
    }

    fn selected_count(&self) -> usize {
        self.selected.iter().filter(|selected| **selected).count()
    }
}

fn save_selected_keys(
    config: &mut Config,
    keys: &[DiscoveredSshKey],
    selected: &[bool],
) -> Result<()> {
    let previous = config.ssh.selected_keys.clone();
    config.ssh.mode = SshMode::Auto;
    config.ssh.selected_keys = selected_keys_from(keys, selected, &previous);
    config.save_to_default_path()
}

fn selected_keys_from(
    keys: &[DiscoveredSshKey],
    selected: &[bool],
    existing: &[SelectedSshKey],
) -> Vec<SelectedSshKey> {
    keys.iter()
        .zip(selected.iter())
        .filter(|(_, is_selected)| **is_selected)
        .map(|(key, _)| {
            let mut selected_key = key.to_selected();
            if let Some(existing_key) = matching_existing_key(key, existing) {
                selected_key.git_user_name = existing_key.git_user_name.clone();
                selected_key.git_user_email = existing_key.git_user_email.clone();
            }
            selected_key
        })
        .collect()
}

fn matching_existing_key<'a>(
    key: &DiscoveredSshKey,
    existing: &'a [SelectedSshKey],
) -> Option<&'a SelectedSshKey> {
    if let Some(fingerprint) = &key.fingerprint {
        if let Some(found) = existing
            .iter()
            .find(|selected| selected.fingerprint.as_ref() == Some(fingerprint))
        {
            return Some(found);
        }
    }

    existing.iter().find(|selected| {
        let selected_path = expand_tilde(&selected.path);
        selected.path == key.display_path
            || selected_path == key.path
            || selected_path
                .canonicalize()
                .map(|path| path == key.path)
                .unwrap_or(false)
    })
}

fn render_configure_ui(state: &ConfigureUiState) -> Result<()> {
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

    // Reserve room for the list title and possible up/down indicators.
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

fn render_quit_prompt() -> Result<()> {
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
    // Leave a spare column so terminals do not wrap when writing the last cell.
    // Use display width rather than character count so wide glyphs like ☑ and ↑
    // do not drift into the next terminal line.
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

fn print_selector(keys: &[DiscoveredSshKey], selected: &[bool]) -> Result<()> {
    println!();
    println!("Detected SSH keys:");
    for (index, (key, is_selected)) in keys.iter().zip(selected.iter()).enumerate() {
        let marker = if *is_selected { "[✓]" } else { "[ ]" };
        let first_line = format!("{marker} {}. {}", index + 1, key.display_path);
        if *is_selected {
            println!("{GREEN}{first_line}{RESET}");
        } else {
            println!("{first_line}");
        }

        let detail = format!(
            "    {}{}{}{}{}",
            key.key_type.as_deref().unwrap_or("unknown"),
            key.fingerprint
                .as_deref()
                .map(|fp| format!(" {fp}"))
                .unwrap_or_default(),
            key.comment
                .as_deref()
                .map(|comment| format!(" {comment}"))
                .unwrap_or_default(),
            if key.has_public_pair { " [pub]" } else { "" },
            match key.permissions_ok {
                Some(true) => "",
                Some(false) => " broad permissions",
                None => "",
            }
        );
        if *is_selected {
            println!("{GREEN}{detail}{RESET}");
        } else {
            println!("{detail}");
        }
    }
    println!();
    println!("Selected rows are green. Unselected rows use the default terminal color.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_key_metadata_preserves_existing_git_identity() {
        let discovered = vec![DiscoveredSshKey {
            path: PathBuf::from("/tmp/current-key"),
            display_path: "/tmp/current-key".to_owned(),
            fingerprint: Some("SHA256:abc123".to_owned()),
            comment: None,
            key_type: Some("ED25519".to_owned()),
            has_public_pair: false,
            permissions_ok: None,
        }];
        let existing = vec![SelectedSshKey {
            path: "/old/path".to_owned(),
            fingerprint: Some("SHA256:abc123".to_owned()),
            comment: None,
            key_type: None,
            git_user_name: Some("Agent User".to_owned()),
            git_user_email: Some("agent@example.com".to_owned()),
        }];

        let selected = selected_keys_from(&discovered, &[true], &existing);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].git_user_name.as_deref(), Some("Agent User"));
        assert_eq!(
            selected[0].git_user_email.as_deref(),
            Some("agent@example.com")
        );
    }

    #[test]
    fn truncation_respects_display_width_budget() {
        let truncated = truncate_to_width("abcdef", 5);

        assert_eq!(UnicodeWidthStr::width(truncated.as_str()), 3);
        assert!(truncated.ends_with('…'));
    }
}
