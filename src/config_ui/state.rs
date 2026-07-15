use anyhow::{Context, Result};

use crate::{
    config::{ColorMode, Config},
    paths::{display_path, StatePaths},
};

use super::{
    color_mode_name, enabled_name, preset_changes, purge_package_cache_paths,
    reset_defaults_changes, save_config_with_recovery_backup, ConfigSection, RowAction,
    SecurityPreset, SECTIONS,
};

pub(super) struct ConfigUiState {
    pub(super) config: Config,
    pub(super) state_paths: StatePaths,
    pub(super) screen: ConfigScreen,
    pub(super) highlighted_section: usize,
    pub(super) highlighted_row: usize,
    pub(super) dirty: bool,
    pub(super) last_message: Option<String>,
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

    pub(super) fn move_down(&mut self) {
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

    pub(super) fn open_highlighted(&mut self) -> ConfigUiAction {
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
                        RowAction::ManualEdit => {
                            self.last_message = Some(format!(
                                "Edit {} manually in {}.",
                                row.title,
                                display_path(&self.state_paths.config_yaml)
                            ));
                        }
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
        }

        ConfigUiAction::Continue
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
        }
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
    Discard,
    Cancel,
}

pub(super) enum ConfigUiExit {
    Quit(i32),
    OpenSshConfigure,
}

pub(super) enum ConfigUiAction {
    Continue,
    OpenSshConfigure,
}

#[derive(Clone, Copy)]
pub(super) enum ConfigScreen {
    Sections,
    Section(ConfigSection),
    PresetPreview(SecurityPreset),
    ResetDefaultsPreview,
    PurgePackageCachesPreview,
}
