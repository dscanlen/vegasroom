use std::{
    collections::HashSet,
    env, fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    config::{Config, SelectedSshKey, SshMode},
    paths::{display_path, expand_tilde, StatePaths},
};

pub const CONTAINER_SSH_AUTH_SOCK: &str = "/tmp/vegasroom/ssh-agent.sock";

const GREEN: &str = "\x1b[32m";
const RESET: &str = "\x1b[0m";

#[derive(Debug, Clone)]
pub enum HostSshAgent {
    Ready(PathBuf),
    MissingEnv,
    MissingPath(PathBuf),
    NotSocket(PathBuf),
}

impl HostSshAgent {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    pub fn status_detail(&self) -> String {
        match self {
            Self::Ready(path) => format!("SSH_AUTH_SOCK is a socket: {}", path.display()),
            Self::MissingEnv => "SSH_AUTH_SOCK is not set. Git over SSH may not work inside the room.".to_owned(),
            Self::MissingPath(path) => format!(
                "SSH_AUTH_SOCK points to a missing path: {}. Git over SSH may not work inside the room.",
                path.display()
            ),
            Self::NotSocket(path) => format!(
                "SSH_AUTH_SOCK does not appear to be a socket: {}. Git over SSH may not work inside the room.",
                path.display()
            ),
        }
    }

    pub fn warning(&self) -> Option<String> {
        if self.is_ready() {
            None
        } else {
            Some(format!("WARN: {}", self.status_detail()))
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiscoveredSshKey {
    pub path: PathBuf,
    pub display_path: String,
    pub fingerprint: Option<String>,
    pub comment: Option<String>,
    pub key_type: Option<String>,
    pub has_public_pair: bool,
    pub permissions_ok: Option<bool>,
}

impl DiscoveredSshKey {
    fn to_selected(&self) -> SelectedSshKey {
        SelectedSshKey {
            path: self.display_path.clone(),
            fingerprint: self.fingerprint.clone(),
            comment: self.comment.clone(),
            key_type: self.key_type.clone(),
            git_user_name: None,
            git_user_email: None,
        }
    }
}

#[derive(Debug)]
pub struct SshRuntime {
    override_path: Option<PathBuf>,
    _managed_agent: Option<ManagedSshAgent>,
}

impl SshRuntime {
    pub fn empty() -> Self {
        Self {
            override_path: None,
            _managed_agent: None,
        }
    }

    pub fn override_path(&self) -> Option<&Path> {
        self.override_path.as_deref()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SshRuntimeMode {
    Interactive,
    NonInteractive,
}

#[derive(Debug)]
struct ManagedSshAgent {
    socket: PathBuf,
    pid: String,
    temp_dir: PathBuf,
}

impl Drop for ManagedSshAgent {
    fn drop(&mut self) {
        let _ = Command::new("ssh-agent")
            .arg("-k")
            .env("SSH_AUTH_SOCK", &self.socket)
            .env("SSH_AGENT_PID", &self.pid)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        let _ = fs::remove_dir_all(&self.temp_dir);
    }
}

pub fn detect_host_agent() -> HostSshAgent {
    let Ok(raw) = env::var("SSH_AUTH_SOCK") else {
        return HostSshAgent::MissingEnv;
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return HostSshAgent::MissingEnv;
    }

    let path = PathBuf::from(trimmed);
    let Ok(metadata) = fs::metadata(&path) else {
        return HostSshAgent::MissingPath(path);
    };

    if is_socket(&metadata.file_type()) {
        HostSshAgent::Ready(path)
    } else {
        HostSshAgent::NotSocket(path)
    }
}

pub fn configure(paths: &[String], follow_symlinks: bool) -> Result<i32> {
    let roots = discovery_roots(paths)?;
    println!("Scanning SSH key roots:");
    for root in &roots {
        println!("  {}", display_path(root));
    }
    if follow_symlinks {
        println!("WARN: following symlinks can scan outside the requested roots.");
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
            .min(available_rows.saturating_sub(3).max(0));
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
            "WARN: permissions appear broad",
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

pub fn status() -> Result<i32> {
    let config = Config::load_or_default()?;
    let host_agent = detect_host_agent();

    println!("SSH mode: {:?}", config.ssh.mode);
    println!();

    if config.ssh.selected_keys.is_empty() {
        println!("Selected keys: none");
    } else {
        println!("Selected keys:");
        for selected in &config.ssh.selected_keys {
            let path = expand_tilde(&selected.path);
            let display = display_path(&path);
            if !path.exists() {
                println!("WARN: {display} - missing");
                continue;
            }

            match fingerprint_key(&path) {
                Ok(metadata) => {
                    let fp_status = match (&selected.fingerprint, &metadata.fingerprint) {
                        (Some(expected), Some(actual)) if expected == actual => "PASS",
                        (Some(expected), Some(actual)) => {
                            println!("FAIL: {display} - fingerprint changed: expected {expected}, got {actual}");
                            continue;
                        }
                        (Some(expected), None) => {
                            println!("WARN: {display} - could not verify configured fingerprint {expected}");
                            continue;
                        }
                        _ => "PASS",
                    };
                    println!(
                        "{fp_status}: {display}{}{}{}",
                        metadata
                            .key_type
                            .as_deref()
                            .map(|v| format!(" - {v}"))
                            .unwrap_or_default(),
                        metadata
                            .fingerprint
                            .as_deref()
                            .map(|v| format!(" {v}"))
                            .unwrap_or_default(),
                        metadata
                            .comment
                            .as_deref()
                            .map(|v| format!(" {v}"))
                            .unwrap_or_default(),
                    );
                    if let (Some(name), Some(email)) =
                        (&selected.git_user_name, &selected.git_user_email)
                    {
                        println!("      Git identity override: {name} <{email}>");
                    }
                }
                Err(err) => println!("WARN: {display} - could not inspect key: {err:#}"),
            }
        }
    }

    println!();
    println!("Host agent:");
    match &host_agent {
        HostSshAgent::Ready(_) => println!("PASS: {}", host_agent.status_detail()),
        _ => println!("WARN: {}", host_agent.status_detail()),
    }

    println!();
    println!("Next launch:");
    println!("{}", next_launch_detail(&config, &host_agent));

    Ok(0)
}

pub fn prepare_agent_override(
    config: &Config,
    state: &StatePaths,
    warn: bool,
    mode: SshRuntimeMode,
) -> Result<SshRuntime> {
    match config.ssh.mode {
        SshMode::Off => Ok(SshRuntime {
            override_path: None,
            _managed_agent: None,
        }),
        SshMode::Host => prepare_host_runtime(state, warn),
        SshMode::Managed => prepare_managed_runtime(config, state, mode)
            .map_err(|err| anyhow!("managed SSH agent setup failed: {err:#}")),
        SshMode::Auto => {
            if !config.ssh.selected_keys.is_empty() {
                match prepare_managed_runtime(config, state, mode) {
                    Ok(runtime) => Ok(runtime),
                    Err(err) => {
                        if warn {
                            eprintln!("WARN: managed SSH agent setup failed: {err:#}");
                            eprintln!("WARN: falling back to host SSH_AUTH_SOCK if available");
                        }
                        prepare_host_runtime(state, warn)
                    }
                }
            } else {
                prepare_host_runtime(state, warn)
            }
        }
    }
}

pub fn planned_ssh_available(config: &Config) -> bool {
    match config.ssh.mode {
        SshMode::Off => false,
        SshMode::Managed => !config.ssh.selected_keys.is_empty(),
        SshMode::Host => detect_host_agent().is_ready(),
        SshMode::Auto => !config.ssh.selected_keys.is_empty() || detect_host_agent().is_ready(),
    }
}

pub fn managed_keys_configured(config: &Config) -> bool {
    !config.ssh.selected_keys.is_empty()
}

pub fn selected_key_checks(config: &Config) -> Vec<String> {
    let mut details = Vec::new();

    for selected in &config.ssh.selected_keys {
        let path = expand_tilde(&selected.path);
        let display = display_path(&path);
        if !path.exists() {
            details.push(format!("FAIL: selected SSH key missing: {display}"));
            continue;
        }

        match fingerprint_key(&path) {
            Ok(metadata) => match (&selected.fingerprint, &metadata.fingerprint) {
                (Some(expected), Some(actual)) if expected == actual => {
                    details.push(format!(
                        "PASS: selected SSH key fingerprint matches: {display}"
                    ));
                }
                (Some(expected), Some(actual)) => {
                    details.push(format!(
                        "FAIL: selected SSH key fingerprint changed: {display}; expected {expected}, got {actual}"
                    ));
                }
                _ => details.push(format!(
                    "WARN: selected SSH key fingerprint could not be fully verified: {display}"
                )),
            },
            Err(err) => details.push(format!(
                "WARN: selected SSH key could not be inspected: {display}: {err:#}"
            )),
        }
    }

    details
}

fn prepare_host_runtime(state: &StatePaths, warn: bool) -> Result<SshRuntime> {
    let agent = detect_host_agent();
    if warn {
        if let Some(message) = agent.warning() {
            eprintln!("{message}");
        }
    }

    let override_path = match agent {
        HostSshAgent::Ready(path) => Some(write_agent_compose_override_for_socket(state, &path)?),
        _ => None,
    };

    Ok(SshRuntime {
        override_path,
        _managed_agent: None,
    })
}

fn prepare_managed_runtime(
    config: &Config,
    state: &StatePaths,
    mode: SshRuntimeMode,
) -> Result<SshRuntime> {
    if config.ssh.selected_keys.is_empty() {
        bail!("no managed SSH keys configured. Run: vr ssh configure");
    }

    let agent = start_managed_agent()?;
    for key in &config.ssh.selected_keys {
        add_key_to_agent(&agent, key, mode)?;
    }

    let override_path = write_agent_compose_override_for_socket(state, &agent.socket)?;
    Ok(SshRuntime {
        override_path: Some(override_path),
        _managed_agent: Some(agent),
    })
}

fn write_agent_compose_override_for_socket(
    state: &StatePaths,
    host_sock: &Path,
) -> Result<PathBuf> {
    fs::create_dir_all(&state.cache).with_context(|| {
        format!(
            "failed to create cache directory: {}",
            display_path(&state.cache)
        )
    })?;

    let override_path = state.cache.join("ssh-agent.compose.yaml");
    let contents = format!(
        r#"services:
  pi:
    environment:
      SSH_AUTH_SOCK: {container_sock}
    volumes:
      - type: bind
        source: "{host_sock}"
        target: {container_sock}
"#,
        container_sock = CONTAINER_SSH_AUTH_SOCK,
        host_sock = yaml_double_quoted(host_sock),
    );

    fs::write(&override_path, contents).with_context(|| {
        format!(
            "failed to write SSH agent Compose override: {}",
            display_path(&override_path)
        )
    })?;

    Ok(override_path)
}

fn start_managed_agent() -> Result<ManagedSshAgent> {
    let temp_dir = unique_agent_dir();
    fs::create_dir_all(&temp_dir).with_context(|| {
        format!(
            "failed to create temporary ssh-agent directory: {}",
            temp_dir.display()
        )
    })?;
    set_private_dir_permissions(&temp_dir)?;

    let socket = temp_dir.join("agent.sock");
    let output = Command::new("ssh-agent")
        .arg("-a")
        .arg(&socket)
        .arg("-s")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to start ssh-agent")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!("ssh-agent failed to start: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(pid) = parse_agent_pid(&stdout) else {
        bail!("ssh-agent started but SSH_AGENT_PID could not be parsed");
    };

    Ok(ManagedSshAgent {
        socket,
        pid,
        temp_dir,
    })
}

fn add_key_to_agent(
    agent: &ManagedSshAgent,
    key: &SelectedSshKey,
    mode: SshRuntimeMode,
) -> Result<()> {
    let path = expand_tilde(&key.path);
    if !path.is_file() {
        bail!(
            "selected SSH key is missing or not a file: {}",
            display_path(&path)
        );
    }

    let mut command = Command::new("ssh-add");
    command
        .arg(&path)
        .env("SSH_AUTH_SOCK", &agent.socket)
        .env("SSH_AGENT_PID", &agent.pid);

    let status = match mode {
        SshRuntimeMode::Interactive => command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status(),
        SshRuntimeMode::NonInteractive => command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    }
    .with_context(|| format!("failed to run ssh-add for {}", display_path(&path)))?;

    if status.success() {
        Ok(())
    } else {
        bail!("ssh-add failed for {}", display_path(&path));
    }
}

fn parse_agent_pid(output: &str) -> Option<String> {
    for part in output.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("SSH_AGENT_PID=") {
            let pid = value.trim();
            if !pid.is_empty() {
                return Some(pid.to_owned());
            }
        }
    }
    None
}

fn unique_agent_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    env::temp_dir().join(format!("vegasroom-agent-{}-{nanos}", std::process::id()))
}

fn discovery_roots(paths: &[String]) -> Result<Vec<PathBuf>> {
    if paths.is_empty() {
        let home = directories::BaseDirs::new()
            .context("could not determine home directory")?
            .home_dir()
            .to_path_buf();
        return Ok(vec![home.join(".ssh")]);
    }

    Ok(paths.iter().map(|path| expand_tilde(path)).collect())
}

fn discover_keys(roots: &[PathBuf], follow_symlinks: bool) -> Result<Vec<DiscoveredSshKey>> {
    let mut keys = Vec::new();
    let mut visited_dirs = HashSet::new();

    for root in roots {
        if !root.exists() {
            println!("WARN: scan root does not exist: {}", display_path(root));
            continue;
        }
        scan_path(root, follow_symlinks, &mut visited_dirs, &mut keys)?;
    }

    keys.dedup_by(|a, b| a.path == b.path);
    Ok(keys)
}

fn scan_path(
    path: &Path,
    follow_symlinks: bool,
    visited_dirs: &mut HashSet<PathBuf>,
    keys: &mut Vec<DiscoveredSshKey>,
) -> Result<()> {
    let metadata = fs::symlink_metadata(path).with_context(|| {
        format!(
            "failed to inspect path while scanning SSH keys: {}",
            display_path(path)
        )
    })?;

    if metadata.file_type().is_symlink() && !follow_symlinks {
        return Ok(());
    }

    let metadata = if follow_symlinks {
        fs::metadata(path)?
    } else {
        metadata
    };

    if metadata.is_dir() {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !visited_dirs.insert(canonical) {
            return Ok(());
        }
        for entry in fs::read_dir(path).with_context(|| {
            format!(
                "failed to read directory while scanning SSH keys: {}",
                display_path(path)
            )
        })? {
            let entry = entry?;
            scan_path(&entry.path(), follow_symlinks, visited_dirs, keys)?;
        }
        return Ok(());
    }

    if metadata.is_file() && is_private_key_candidate(path) {
        if let Ok(key) = inspect_private_key(path) {
            keys.push(key);
        }
    }

    Ok(())
}

fn is_private_key_candidate(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    if name.ends_with(".pub")
        || matches!(
            name,
            "known_hosts" | "known_hosts.old" | "authorized_keys" | "config"
        )
        || name.ends_with(".bak")
        || name.ends_with(".old")
        || name.ends_with(".tmp")
    {
        return false;
    }

    let Ok(contents) = fs::read(path) else {
        return false;
    };
    let sample_len = contents.len().min(4096);
    let sample = String::from_utf8_lossy(&contents[..sample_len]);
    sample.contains("PRIVATE KEY")
}

fn inspect_private_key(path: &Path) -> Result<DiscoveredSshKey> {
    let metadata = fingerprint_key(path).unwrap_or_default();
    Ok(DiscoveredSshKey {
        path: path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
        display_path: display_path(path),
        fingerprint: metadata.fingerprint,
        comment: metadata.comment,
        key_type: metadata.key_type,
        has_public_pair: path
            .with_extension(format!(
                "{}pub",
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| format!("{ext}."))
                    .unwrap_or_default()
            ))
            .is_file()
            || PathBuf::from(format!("{}.pub", path.display())).is_file(),
        permissions_ok: private_key_permissions_ok(path),
    })
}

#[derive(Debug, Default)]
struct KeyMetadata {
    fingerprint: Option<String>,
    comment: Option<String>,
    key_type: Option<String>,
}

fn fingerprint_key(path: &Path) -> Result<KeyMetadata> {
    let output = Command::new("ssh-keygen")
        .arg("-lf")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run ssh-keygen for {}", display_path(path)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!(
            "ssh-keygen could not fingerprint {}: {stderr}",
            display_path(path)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_ssh_keygen_fingerprint(stdout.trim()))
}

fn parse_ssh_keygen_fingerprint(line: &str) -> KeyMetadata {
    let mut metadata = KeyMetadata::default();
    let mut parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() >= 2 {
        metadata.fingerprint = Some(parts[1].to_owned());
    }

    if let Some(last) = parts.last() {
        if last.starts_with('(') && last.ends_with(')') {
            metadata.key_type = Some(
                last.trim_start_matches('(')
                    .trim_end_matches(')')
                    .to_owned(),
            );
            parts.pop();
        }
    }

    if parts.len() > 2 {
        metadata.comment = Some(parts[2..].join(" "));
    }

    metadata
}

fn initial_selection(discovered: &[DiscoveredSshKey], selected: &[SelectedSshKey]) -> Vec<bool> {
    discovered
        .iter()
        .map(|candidate| {
            selected.iter().any(|configured| {
                let configured_path = expand_tilde(&configured.path);
                configured_path == candidate.path
                    || configured.fingerprint.is_some()
                        && configured.fingerprint == candidate.fingerprint
            })
        })
        .collect()
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
                Some(false) => " WARN: broad permissions",
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

fn next_launch_detail(config: &Config, host_agent: &HostSshAgent) -> String {
    match config.ssh.mode {
        SshMode::Off => "SSH forwarding is disabled.".to_owned(),
        SshMode::Managed => {
            if config.ssh.selected_keys.is_empty() {
                "WARN: managed mode is enabled but no keys are selected. Run: vr ssh configure"
                    .to_owned()
            } else {
                format!(
                    "PASS: Vegasroom will start a managed temporary ssh-agent with {} configured key(s).",
                    config.ssh.selected_keys.len()
                )
            }
        }
        SshMode::Host => {
            if host_agent.is_ready() {
                "PASS: Vegasroom will forward the existing host SSH_AUTH_SOCK.".to_owned()
            } else {
                "WARN: host mode is enabled but no usable host SSH_AUTH_SOCK was detected."
                    .to_owned()
            }
        }
        SshMode::Auto => {
            if !config.ssh.selected_keys.is_empty() {
                format!(
                    "PASS: Vegasroom will start a managed temporary ssh-agent with {} configured key(s).",
                    config.ssh.selected_keys.len()
                )
            } else if host_agent.is_ready() {
                "PASS: Vegasroom will forward the existing host SSH_AUTH_SOCK.".to_owned()
            } else {
                "WARN: no managed keys or host SSH agent detected. Git over SSH may not work inside the room.".to_owned()
            }
        }
    }
}

fn yaml_double_quoted(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[cfg(unix)]
fn is_socket(file_type: &fs::FileType) -> bool {
    use std::os::unix::fs::FileTypeExt;
    file_type.is_socket()
}

#[cfg(not(unix))]
fn is_socket(_file_type: &fs::FileType) -> bool {
    false
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn private_key_permissions_ok(path: &Path) -> Option<bool> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path).ok()?.permissions().mode();
    Some(mode & 0o077 == 0)
}

#[cfg(not(unix))]
fn private_key_permissions_ok(_path: &Path) -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_keygen_fingerprint_with_comment_and_type() {
        let metadata =
            parse_ssh_keygen_fingerprint("256 SHA256:abc123 user@example.com test key (ED25519)");

        assert_eq!(metadata.fingerprint.as_deref(), Some("SHA256:abc123"));
        assert_eq!(
            metadata.comment.as_deref(),
            Some("user@example.com test key")
        );
        assert_eq!(metadata.key_type.as_deref(), Some("ED25519"));
    }

    #[test]
    fn initial_selection_matches_by_fingerprint() {
        let discovered = vec![DiscoveredSshKey {
            path: PathBuf::from("/tmp/current-key"),
            display_path: "/tmp/current-key".to_owned(),
            fingerprint: Some("SHA256:abc123".to_owned()),
            comment: None,
            key_type: Some("ED25519".to_owned()),
            has_public_pair: false,
            permissions_ok: None,
        }];
        let selected = vec![SelectedSshKey {
            path: "/old/path".to_owned(),
            fingerprint: Some("SHA256:abc123".to_owned()),
            comment: None,
            key_type: None,
            git_user_name: None,
            git_user_email: None,
        }];

        assert_eq!(initial_selection(&discovered, &selected), vec![true]);
    }

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

    #[test]
    fn ssh_agent_socket_path_is_yaml_escaped() {
        let escaped = yaml_double_quoted(Path::new(r#"/tmp/agent "quoted"/sock"#));

        assert_eq!(escaped, r#"/tmp/agent \"quoted\"/sock"#);
    }
}
