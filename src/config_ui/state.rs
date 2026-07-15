use anyhow::{Context, Result};

use std::path::PathBuf;

use crate::{
    config::{ColorMode, Config, SelectedSshKey, SshMode},
    paths::{display_path, expand_tilde, StatePaths},
    ssh::{self, DiscoveredSshKey},
};

use super::{
    color_mode_name, enabled_name, preset_changes, purge_package_cache_paths,
    reset_defaults_changes, save_config_with_recovery_backup, ConfigSection, RowAction,
    SecurityPreset, TextField, SECTIONS,
};

pub(super) struct ConfigUiState {
    pub(super) config: Config,
    pub(super) state_paths: StatePaths,
    pub(super) screen: ConfigScreen,
    pub(super) highlighted_section: usize,
    pub(super) highlighted_row: usize,
    pub(super) dirty: bool,
    pub(super) last_message: Option<String>,
    pub(super) input_buffer: String,
    pub(super) ssh_keys: Vec<DiscoveredSshKey>,
    pub(super) ssh_selected: Vec<bool>,
    pub(super) ssh_roots: Vec<PathBuf>,
    pub(super) ssh_follow_symlinks: bool,
}

impl ConfigUiState {
    pub(super) fn new(config: Config, state_paths: StatePaths) -> Self {
        Self {
            config,
            state_paths,
            screen: ConfigScreen::Sections,
            highlighted_section: 0,
            highlighted_row: 0,
            dirty: false,
            last_message: None,
            input_buffer: String::new(),
            ssh_keys: Vec::new(),
            ssh_selected: Vec::new(),
            ssh_roots: Vec::new(),
            ssh_follow_symlinks: false,
        }
    }

    fn highlighted_section(&self) -> ConfigSection {
        SECTIONS[self.highlighted_section]
    }

    pub(super) fn move_up(&mut self) {
        match self.screen {
            ConfigScreen::Sections => {
                if self.highlighted_section == 0 {
                    self.highlighted_section = SECTIONS.len() - 1;
                } else {
                    self.highlighted_section -= 1;
                }
            }
            ConfigScreen::Section(ConfigSection::Ssh) => {
                if self.ssh_keys.is_empty() {
                    return;
                }
                if self.highlighted_row == 0 {
                    self.highlighted_row = self.ssh_keys.len() - 1;
                } else {
                    self.highlighted_row -= 1;
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
            | ConfigScreen::PurgePackageCachesPreview
            | ConfigScreen::TextInput(_) => {}
        }
        self.last_message = None;
    }

    pub(super) fn move_down(&mut self) {
        match self.screen {
            ConfigScreen::Sections => {
                self.highlighted_section = (self.highlighted_section + 1) % SECTIONS.len();
            }
            ConfigScreen::Section(ConfigSection::Ssh) => {
                if self.ssh_keys.is_empty() {
                    return;
                }
                self.highlighted_row = (self.highlighted_row + 1) % self.ssh_keys.len();
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
            | ConfigScreen::PurgePackageCachesPreview
            | ConfigScreen::TextInput(_) => {}
        }
        self.last_message = None;
    }

    pub(super) fn open_highlighted(&mut self) {
        match self.screen {
            ConfigScreen::Sections => {
                let section = self.highlighted_section();
                self.screen = ConfigScreen::Section(section);
                self.highlighted_row = 0;
                self.last_message = None;
                if matches!(section, ConfigSection::Ssh) {
                    self.load_ssh_keys();
                }
            }
            ConfigScreen::Section(ConfigSection::Ssh) => self.toggle_highlighted_ssh_key(),
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
                        RowAction::EditText(field) => self.start_text_input(field),
                        RowAction::Placeholder => {
                            self.last_message = Some(format!("{} is informational.", row.title));
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
            ConfigScreen::TextInput(_) => {
                if let Err(error) = self.apply_text_input() {
                    self.last_message = Some(format!("Edit failed: {error:#}"));
                }
            }
        }
    }

    pub(super) fn go_back(&mut self) {
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
            ConfigScreen::TextInput(_) => {
                self.screen = ConfigScreen::Section(ConfigSection::Advanced);
                self.input_buffer.clear();
                self.last_message = Some("Edit canceled.".to_owned());
            }
        }
    }

    fn start_text_input(&mut self, field: TextField) {
        self.input_buffer = field.current_value(&self.config);
        self.screen = ConfigScreen::TextInput(field);
        self.last_message = None;
    }

    pub(super) fn push_input_char(&mut self, ch: char) {
        self.input_buffer.push(ch);
        self.last_message = None;
    }

    pub(super) fn pop_input_char(&mut self) {
        self.input_buffer.pop();
        self.last_message = None;
    }

    pub(super) fn apply_text_input(&mut self) -> Result<()> {
        let ConfigScreen::TextInput(field) = self.screen else {
            return Ok(());
        };
        let value = self.input_buffer.trim().to_owned();
        let mut next_config = self.config.clone();

        match field {
            TextField::WorkspacePath => next_config.paths.workspace = value.clone(),
            TextField::GitUserName => next_config.git.user_name = optional_text_value(&value),
            TextField::GitUserEmail => next_config.git.user_email = optional_text_value(&value),
        }

        next_config.validate_semantics()?;
        let changed = field.current_value(&self.config) != field.current_value(&next_config);
        self.config = next_config;
        self.dirty |= changed;
        self.screen = ConfigScreen::Section(ConfigSection::Advanced);
        self.input_buffer.clear();
        self.last_message = Some(if changed {
            format!("Updated {}. Press s to save.", field.config_path())
        } else {
            format!("{} unchanged.", field.config_path())
        });
        Ok(())
    }

    pub(super) fn cancel_text_input(&mut self) {
        self.go_back();
    }

    pub(super) fn load_ssh_keys(&mut self) {
        let roots = match ssh::discovery_roots(&[]) {
            Ok(roots) => roots,
            Err(error) => {
                self.ssh_keys.clear();
                self.ssh_selected.clear();
                self.last_message = Some(format!("SSH key scan failed: {error:#}"));
                return;
            }
        };

        self.ssh_roots = roots;
        self.rescan_ssh_keys();
    }

    pub(super) fn rescan_ssh_keys(&mut self) {
        if self.ssh_roots.is_empty() {
            match ssh::discovery_roots(&[]) {
                Ok(roots) => self.ssh_roots = roots,
                Err(error) => {
                    self.last_message = Some(format!("SSH key scan failed: {error:#}"));
                    return;
                }
            }
        }

        match ssh::discover_keys(&self.ssh_roots, self.ssh_follow_symlinks) {
            Ok(mut keys) => {
                keys.sort_by(|a, b| a.display_path.cmp(&b.display_path));
                self.ssh_selected = ssh::initial_selection(&keys, &self.config.ssh.selected_keys);
                self.ssh_keys = keys;
                self.highlighted_row = self
                    .highlighted_row
                    .min(self.ssh_keys.len().saturating_sub(1));
                self.last_message = Some(if self.ssh_keys.is_empty() {
                    "No SSH private keys were detected.".to_owned()
                } else {
                    format!("Scanned {} SSH key(s).", self.ssh_keys.len())
                });
            }
            Err(error) => {
                self.last_message = Some(format!("SSH key scan failed: {error:#}"));
            }
        }
    }

    fn toggle_highlighted_ssh_key(&mut self) {
        if let Some(slot) = self.ssh_selected.get_mut(self.highlighted_row) {
            *slot = !*slot;
            self.config.ssh.mode = SshMode::Auto;
            self.config.ssh.selected_keys = selected_ssh_keys_from(
                &self.ssh_keys,
                &self.ssh_selected,
                &self.config.ssh.selected_keys,
            );
            self.dirty = true;
            self.last_message = Some(format!(
                "{} SSH key(s) selected. Press s to save.",
                self.config.ssh.selected_keys.len()
            ));
        }
    }

    pub(super) fn selected_ssh_key_count(&self) -> usize {
        self.ssh_selected
            .iter()
            .filter(|selected| **selected)
            .count()
    }

    pub(super) fn apply_preset(&mut self, preset: SecurityPreset) {
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

    pub(super) fn apply_reset_defaults(&mut self) {
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

    pub(super) fn validate_config(&mut self) -> Result<()> {
        let serialized =
            serde_yaml::to_string(&self.config).context("failed to serialize config")?;
        let reparsed: Config =
            serde_yaml::from_str(&serialized).context("failed to reload serialized config")?;
        reparsed.validate_semantics()?;
        self.last_message = Some("Current in-memory config validates successfully.".to_owned());
        Ok(())
    }

    pub(super) fn cycle_color_mode(&mut self) {
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

    pub(super) fn toggle_git_inherit_host(&mut self) {
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

    pub(super) fn save(&mut self) -> Result<()> {
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

#[derive(Clone, Copy)]
pub(super) enum QuitDecision {
    Save,
    Quit,
    Cancel,
}

pub(super) enum ConfigUiExit {
    Quit(i32),
}

fn optional_text_value(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn selected_ssh_keys_from(
    keys: &[DiscoveredSshKey],
    selected: &[bool],
    existing: &[SelectedSshKey],
) -> Vec<SelectedSshKey> {
    keys.iter()
        .zip(selected.iter())
        .filter(|(_, is_selected)| **is_selected)
        .map(|(key, _)| {
            let mut selected_key = SelectedSshKey {
                path: key.display_path.clone(),
                fingerprint: key.fingerprint.clone(),
                comment: key.comment.clone(),
                key_type: key.key_type.clone(),
                git_user_name: None,
                git_user_email: None,
            };
            if let Some(existing_key) = matching_existing_ssh_key(key, existing) {
                selected_key.git_user_name = existing_key.git_user_name.clone();
                selected_key.git_user_email = existing_key.git_user_email.clone();
            }
            selected_key
        })
        .collect()
}

fn matching_existing_ssh_key<'a>(
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

#[derive(Clone, Copy)]
pub(super) enum ConfigScreen {
    Sections,
    Section(ConfigSection),
    PresetPreview(SecurityPreset),
    ResetDefaultsPreview,
    PurgePackageCachesPreview,
    TextInput(TextField),
}
