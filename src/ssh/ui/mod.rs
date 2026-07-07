use std::{
    io::{self, IsTerminal},
    path::PathBuf,
};

use anyhow::{Context, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};

mod line_mode;
mod render;

use line_mode::configure_line_mode;
use render::{render_configure_ui, render_quit_prompt};

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
}
