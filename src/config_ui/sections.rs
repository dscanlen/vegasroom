use crate::{
    config::Config,
    paths::{display_path, StatePaths},
};

use super::presets::{enabled_name, toolchain_row_title, SecurityPreset};
use super::{color_mode_name, git_identity_preview};

pub(super) const SECTIONS: &[ConfigSection] = &[
    ConfigSection::SecurityPreset,
    ConfigSection::Environment,
    ConfigSection::Ssh,
    ConfigSection::Advanced,
];

#[derive(Clone, Copy)]
pub(super) enum ConfigSection {
    SecurityPreset,
    Environment,
    Ssh,
    Advanced,
}

impl ConfigSection {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::SecurityPreset => "Security",
            Self::Environment => "Environment",
            Self::Ssh => "SSH",
            Self::Advanced => "Advanced",
        }
    }

    pub(super) fn rows(self, config: &Config, state_paths: &StatePaths) -> Vec<SectionRow> {
        match self {
            Self::SecurityPreset => vec![
                SectionRow::preset(
                    SecurityPreset::DefaultCompatible,
                    vec![
                        "Preserves current proven behavior and maximum compatibility.".to_owned(),
                        "Alias: lowsec/default.".to_owned(),
                    ],
                ),
                SectionRow::preset(
                    SecurityPreset::Safer,
                    vec![
                        "Sets risky workspace mount policy to deny.".to_owned(),
                        "Keeps workspace writes, host networking, and automatic SSH behavior."
                            .to_owned(),
                    ],
                ),
                SectionRow::preset(
                    SecurityPreset::Strict,
                    vec![
                        "Enables deny policy, read-only workspace, read-only rootfs, managed SSH, \
and no host Git inheritance."
                            .to_owned(),
                        "May reduce editing, Git, login, or shell compatibility.".to_owned(),
                    ],
                ),
            ],
            Self::Environment => vec![
                SectionRow::action(
                    toolchain_row_title("Rust", config.environment.rust.enabled),
                    vec![
                        format!(
                            "Current: {} ({})",
                            enabled_name(config.environment.rust.enabled),
                            config.environment.rust.toolchain
                        ),
                        "Press Enter to toggle. Press s to save.".to_owned(),
                        "Run `vr init --build` when ready to rebuild the environment image."
                            .to_owned(),
                    ],
                    RowAction::ToggleRustToolchain,
                ),
                SectionRow::action(
                    toolchain_row_title("Python", config.environment.python.enabled),
                    vec![
                        format!(
                            "Current: {}",
                            enabled_name(config.environment.python.enabled)
                        ),
                        "Press Enter to toggle. Press s to save.".to_owned(),
                        "Run `vr init --build` when ready to rebuild the environment image."
                            .to_owned(),
                    ],
                    RowAction::TogglePythonToolchain,
                ),
                SectionRow::action(
                    toolchain_row_title("Go", config.environment.go.enabled),
                    vec![
                        format!("Current: {}", enabled_name(config.environment.go.enabled)),
                        "Press Enter to toggle. Press s to save.".to_owned(),
                        "Run `vr init --build` when ready to rebuild the environment image."
                            .to_owned(),
                    ],
                    RowAction::ToggleGoToolchain,
                ),
                SectionRow::action(
                    toolchain_row_title("TypeScript", config.environment.typescript.enabled),
                    vec![
                        format!(
                            "Current: {}; packages: {}",
                            enabled_name(config.environment.typescript.enabled),
                            config.environment.typescript.packages.join(", ")
                        ),
                        "Press Enter to toggle. Press s to save.".to_owned(),
                        "Run `vr init --build` when ready to rebuild the environment image."
                            .to_owned(),
                    ],
                    RowAction::ToggleTypeScriptToolchain,
                ),
                SectionRow::action(
                    "Purge package download caches",
                    vec![
                        "Removes npm/pip download caches and Cargo registry/git caches.".to_owned(),
                        "Preserves workspaces, auth, SSH, Pi npm-global, and Cargo bin.".to_owned(),
                    ],
                    RowAction::PreviewPurgePackageCaches,
                ),
            ],
            Self::Ssh => Vec::new(),
            Self::Advanced => vec![
                SectionRow::manual_edit(
                    "Workspace path",
                    vec![
                        format!("Current: {}", config.paths.workspace),
                        "Edit paths.workspace manually in the config YAML.".to_owned(),
                    ],
                ),
                SectionRow::action(
                    "Git: inherit host identity",
                    vec![
                        format!("Current: {}", config.git.inherit_host),
                        "Press Enter to toggle true/false.".to_owned(),
                    ],
                    RowAction::ToggleGitInheritHost,
                ),
                SectionRow::manual_edit(
                    "Git: configured user.name",
                    vec![
                        format!(
                            "Current: {}",
                            config.git.user_name.as_deref().unwrap_or("not set")
                        ),
                        "Edit git.user_name manually in the config YAML.".to_owned(),
                    ],
                ),
                SectionRow::manual_edit(
                    "Git: configured user.email",
                    vec![
                        format!(
                            "Current: {}",
                            config.git.user_email.as_deref().unwrap_or("not set")
                        ),
                        "Edit git.user_email manually in the config YAML.".to_owned(),
                    ],
                ),
                SectionRow::new("Git: effective identity", git_identity_preview(config)),
                SectionRow::action(
                    "Color mode",
                    vec![format!("Current: {}", color_mode_name(config.ui.color))],
                    RowAction::CycleColorMode,
                ),
                SectionRow::new("Config path", vec![display_path(&state_paths.config_yaml)]),
                SectionRow::action(
                    "Validate current config",
                    vec!["Press Enter to validate the in-memory config model.".to_owned()],
                    RowAction::ValidateConfig,
                ),
                SectionRow::new(
                    "Recovery backup during save",
                    vec![
                        "Saving over an existing config creates a temporary recovery backup."
                            .to_owned(),
                        "The backup is removed after the new config is saved and validated."
                            .to_owned(),
                    ],
                ),
                SectionRow::action(
                    "Reset all to defaults",
                    vec![
                        "Press Enter to preview all fields that would change.".to_owned(),
                        "The reset is applied in memory first; press s to save it.".to_owned(),
                    ],
                    RowAction::PreviewResetDefaults,
                ),
            ],
        }
    }
}

pub(super) struct SectionRow {
    pub(super) title: String,
    pub(super) details: Vec<String>,
    pub(super) action: RowAction,
}

impl SectionRow {
    pub(super) fn new(title: impl Into<String>, details: Vec<String>) -> Self {
        Self {
            title: title.into(),
            details,
            action: RowAction::Placeholder,
        }
    }

    pub(super) fn preset(preset: SecurityPreset, details: Vec<String>) -> Self {
        Self {
            title: preset.title().to_owned(),
            details,
            action: RowAction::PreviewPreset(preset),
        }
    }

    pub(super) fn manual_edit(title: impl Into<String>, details: Vec<String>) -> Self {
        Self {
            title: title.into(),
            details,
            action: RowAction::ManualEdit,
        }
    }

    pub(super) fn action(
        title: impl Into<String>,
        details: Vec<String>,
        action: RowAction,
    ) -> Self {
        Self {
            title: title.into(),
            details,
            action,
        }
    }

    pub(super) fn security_preset(&self) -> Option<SecurityPreset> {
        match self.action {
            RowAction::PreviewPreset(preset) => Some(preset),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum RowAction {
    Placeholder,
    ManualEdit,
    PreviewPreset(SecurityPreset),
    CycleColorMode,
    ToggleGitInheritHost,
    ToggleRustToolchain,
    TogglePythonToolchain,
    ToggleGoToolchain,
    ToggleTypeScriptToolchain,
    ValidateConfig,
    PreviewResetDefaults,
    PreviewPurgePackageCaches,
}
