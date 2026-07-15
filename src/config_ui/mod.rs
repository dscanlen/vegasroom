use std::io::{self, IsTerminal};

#[cfg(test)]
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode};
#[cfg(test)]
use unicode_width::UnicodeWidthStr;

mod cache;
mod persistence;
mod presets;
mod render;
mod sections;
mod state;
mod values;

#[cfg(test)]
use crate::config::{ColorMode, RiskyMountPolicy, SshMode};
use crate::{
    config::Config,
    paths::{display_path, StatePaths},
    ssh,
};

use cache::{package_cache_paths, purge_package_cache_paths};
use persistence::save_config_with_recovery_backup;
use presets::{
    active_security_preset, enabled_name, preset_changes, reset_defaults_changes, SecurityPreset,
};
use render::{render, render_quit_prompt, TerminalSession};
#[cfg(test)]
use render::{
    render_header, render_keys, render_section_screen, render_text_input, truncate_to_width,
    TuiStyles,
};
use sections::{ConfigSection, RowAction, SectionRow, TextField, SECTIONS};
use state::{ConfigScreen, ConfigUiAction, ConfigUiExit, ConfigUiState, QuitDecision};
use values::{color_mode_name, git_identity_preview};

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

        if matches!(state.screen, ConfigScreen::TextInput(_)) {
            handle_text_input_key(&mut state, key.code)?;
            continue;
        }

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

fn handle_text_input_key(state: &mut ConfigUiState, code: KeyCode) -> Result<()> {
    match code {
        KeyCode::Enter => state.apply_text_input()?,
        KeyCode::Esc => state.cancel_text_input(),
        KeyCode::Backspace | KeyCode::Delete => state.pop_input_char(),
        KeyCode::Char(ch) => state.push_input_char(ch),
        _ => {}
    }

    Ok(())
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
        config.environment.rust.toolchain = "nightly".to_owned();
        config.environment.typescript.packages = vec!["typescript".to_owned(), "tsx".to_owned()];
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let state = ConfigUiState::new(config, paths);

        let output = render_section_to_string(&state, ConfigSection::Environment);

        assert!(output.contains("Current: enabled"));
        assert!(output.contains("Current: disabled"));
        assert!(!output.contains("nightly"));
        assert!(!output.contains("packages: typescript, tsx"));
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

        assert!(output.contains("Workspace path"));
        assert!(output.contains("Press Enter to edit paths.workspace."));
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
    fn editable_text_rows_open_input_screen_with_current_value() {
        let mut config = Config::default();
        config.git.user_name = Some("Configured User".to_owned());
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);
        state.screen = ConfigScreen::Section(ConfigSection::Advanced);
        state.highlighted_row = ConfigSection::Advanced
            .rows(&state.config, &state.state_paths)
            .iter()
            .position(|row| row.title == "Git: configured user.name")
            .unwrap();

        let action = state.open_highlighted();

        assert!(matches!(action, ConfigUiAction::Continue));
        assert!(!state.dirty);
        assert!(matches!(
            state.screen,
            ConfigScreen::TextInput(TextField::GitUserName)
        ));
        assert_eq!(state.input_buffer, "Configured User");
    }

    #[test]
    fn text_input_render_shows_field_help_and_buffer() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);
        state.screen = ConfigScreen::TextInput(TextField::WorkspacePath);
        state.input_buffer = "/tmp/workspace".to_owned();
        let mut output = Vec::new();

        render_text_input(
            &mut output,
            &state,
            TextField::WorkspacePath,
            TuiStyles::plain(),
        )
        .unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("Field: paths.workspace"));
        assert!(output.contains("Workspace path must not be blank."));
        assert!(output.contains("/tmp/workspace"));
    }

    #[test]
    fn text_input_updates_workspace_path_and_marks_dirty() {
        let config = Config::default();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);
        state.screen = ConfigScreen::TextInput(TextField::WorkspacePath);
        state.input_buffer = "/tmp/new-workspace".to_owned();

        state.apply_text_input().unwrap();

        assert!(state.dirty);
        assert_eq!(state.config.paths.workspace, "/tmp/new-workspace");
        assert!(matches!(
            state.screen,
            ConfigScreen::Section(ConfigSection::Advanced)
        ));
    }

    #[test]
    fn text_input_blank_git_value_clears_optional_field() {
        let mut config = Config::default();
        config.git.user_email = Some("configured@example.com".to_owned());
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);
        state.screen = ConfigScreen::TextInput(TextField::GitUserEmail);
        state.input_buffer = "   ".to_owned();

        state.apply_text_input().unwrap();

        assert!(state.dirty);
        assert_eq!(state.config.git.user_email, None);
    }

    #[test]
    fn text_input_rejects_blank_workspace_path() {
        let config = Config::default();
        let original_workspace = config.paths.workspace.clone();
        let paths = StatePaths::from_root(std::path::PathBuf::from("/tmp/vegasroom-test"));
        let mut state = ConfigUiState::new(config, paths);
        state.screen = ConfigScreen::TextInput(TextField::WorkspacePath);
        state.input_buffer = "   ".to_owned();

        let err = state.apply_text_input().unwrap_err();

        assert!(err.to_string().contains("paths.workspace"));
        assert!(!state.dirty);
        assert_eq!(state.config.paths.workspace, original_workspace);
        assert!(matches!(
            state.screen,
            ConfigScreen::TextInput(TextField::WorkspacePath)
        ));
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
