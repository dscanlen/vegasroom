use crate::config::{Config, RiskyMountPolicy, SshMode};

use super::color_mode_name;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SecurityPreset {
    DefaultCompatible,
    Safer,
    Strict,
}

impl SecurityPreset {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::DefaultCompatible => "Default / Compatible",
            Self::Safer => "Safer",
            Self::Strict => "Strict",
        }
    }

    pub(super) fn notes(self) -> Vec<&'static str> {
        match self {
            Self::DefaultCompatible => vec![
                "Maximum compatibility with the currently proven runtime.",
                "Alias: lowsec/default.",
            ],
            Self::Safer => vec![
                "Improves accidental exposure protection by denying risky workspace mounts.",
                "Keeps workspace writes, host networking, automatic SSH, and host Git inheritance.",
            ],
            Self::Strict => vec![
                "Security-forward settings with compatibility tradeoffs.",
                "Does not change host networking because bridge remains experimental for Pi login.",
            ],
        }
    }

    pub(super) fn apply(self, config: &mut Config) {
        match self {
            Self::DefaultCompatible => {
                config.workspace.risky_mount_policy = RiskyMountPolicy::Warn;
                config.harness.pi.read_only_workspace = false;
                config.harness.pi.read_only_rootfs = false;
                config.harness.pi.network = "host".to_owned();
                config.harness.pi.build_network = "host".to_owned();
                config.ssh.mode = SshMode::Auto;
                config.git.inherit_host = true;
            }
            Self::Safer => {
                config.workspace.risky_mount_policy = RiskyMountPolicy::Deny;
                config.harness.pi.read_only_workspace = false;
                config.harness.pi.read_only_rootfs = false;
                config.harness.pi.network = "host".to_owned();
                config.harness.pi.build_network = "host".to_owned();
                config.ssh.mode = SshMode::Auto;
                config.git.inherit_host = true;
            }
            Self::Strict => {
                config.workspace.risky_mount_policy = RiskyMountPolicy::Deny;
                config.harness.pi.read_only_workspace = true;
                config.harness.pi.read_only_rootfs = true;
                config.harness.pi.network = "host".to_owned();
                config.harness.pi.build_network = "host".to_owned();
                config.ssh.mode = SshMode::Managed;
                config.git.inherit_host = false;
            }
        }
    }
}

pub(super) struct ConfigChange {
    pub(super) field: &'static str,
    pub(super) before: String,
    pub(super) after: String,
}

pub(super) fn preset_changes(config: &Config, preset: SecurityPreset) -> Vec<ConfigChange> {
    let mut target = config.clone();
    preset.apply(&mut target);

    diff_preset_configs(config, &target)
}

fn diff_preset_configs(before: &Config, after: &Config) -> Vec<ConfigChange> {
    let mut changes = Vec::new();
    push_change(
        &mut changes,
        "workspace.risky_mount_policy",
        risky_mount_policy_name(before.workspace.risky_mount_policy),
        risky_mount_policy_name(after.workspace.risky_mount_policy),
    );
    push_change(
        &mut changes,
        "harness.pi.read_only_workspace",
        before.harness.pi.read_only_workspace,
        after.harness.pi.read_only_workspace,
    );
    push_change(
        &mut changes,
        "harness.pi.read_only_rootfs",
        before.harness.pi.read_only_rootfs,
        after.harness.pi.read_only_rootfs,
    );
    push_change(
        &mut changes,
        "harness.pi.network",
        before.harness.pi.network.as_str(),
        after.harness.pi.network.as_str(),
    );
    push_change(
        &mut changes,
        "harness.pi.build_network",
        before.harness.pi.build_network.as_str(),
        after.harness.pi.build_network.as_str(),
    );
    push_change(
        &mut changes,
        "ssh.mode",
        ssh_mode_name(before.ssh.mode),
        ssh_mode_name(after.ssh.mode),
    );
    push_change(
        &mut changes,
        "git.inherit_host",
        before.git.inherit_host,
        after.git.inherit_host,
    );
    changes
}

pub(super) fn reset_defaults_changes(config: &Config) -> Vec<ConfigChange> {
    diff_configs(config, &Config::default())
}

fn diff_configs(before: &Config, after: &Config) -> Vec<ConfigChange> {
    let mut changes = diff_preset_configs(before, after);
    push_change(
        &mut changes,
        "paths.workspace",
        before.paths.workspace.as_str(),
        after.paths.workspace.as_str(),
    );
    push_change(
        &mut changes,
        "docker.context",
        before.docker.context.as_str(),
        after.docker.context.as_str(),
    );
    push_change(
        &mut changes,
        "docker.compose_file",
        before.docker.compose_file.as_str(),
        after.docker.compose_file.as_str(),
    );
    push_change(
        &mut changes,
        "harness.pi.image",
        before.harness.pi.image.as_str(),
        after.harness.pi.image.as_str(),
    );
    push_change(
        &mut changes,
        "harness.pi.command",
        before.harness.pi.command.as_str(),
        after.harness.pi.command.as_str(),
    );
    push_change(
        &mut changes,
        "ssh.selected_keys",
        before.ssh.selected_keys.len(),
        after.ssh.selected_keys.len(),
    );
    push_change(
        &mut changes,
        "git.user_name",
        option_value(before.git.user_name.as_deref()),
        option_value(after.git.user_name.as_deref()),
    );
    push_change(
        &mut changes,
        "git.user_email",
        option_value(before.git.user_email.as_deref()),
        option_value(after.git.user_email.as_deref()),
    );
    push_change(
        &mut changes,
        "ui.color",
        color_mode_name(before.ui.color),
        color_mode_name(after.ui.color),
    );
    push_change(
        &mut changes,
        "environment.rust.enabled",
        before.environment.rust.enabled,
        after.environment.rust.enabled,
    );
    push_change(
        &mut changes,
        "environment.rust.toolchain",
        before.environment.rust.toolchain.as_str(),
        after.environment.rust.toolchain.as_str(),
    );
    push_change(
        &mut changes,
        "environment.rust.components",
        before.environment.rust.components.join(","),
        after.environment.rust.components.join(","),
    );
    push_change(
        &mut changes,
        "environment.python.enabled",
        before.environment.python.enabled,
        after.environment.python.enabled,
    );
    push_change(
        &mut changes,
        "environment.go.enabled",
        before.environment.go.enabled,
        after.environment.go.enabled,
    );
    push_change(
        &mut changes,
        "environment.typescript.enabled",
        before.environment.typescript.enabled,
        after.environment.typescript.enabled,
    );
    push_change(
        &mut changes,
        "environment.typescript.packages",
        before.environment.typescript.packages.join(","),
        after.environment.typescript.packages.join(","),
    );
    changes
}

fn option_value(value: Option<&str>) -> &str {
    value.unwrap_or("not set")
}

pub(super) fn enabled_name(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}

pub(super) fn toolchain_row_title(name: &str, enabled: bool) -> String {
    if enabled {
        format!("✓ {name}")
    } else {
        name.to_owned()
    }
}

fn push_change(
    changes: &mut Vec<ConfigChange>,
    field: &'static str,
    before: impl ToString,
    after: impl ToString,
) {
    let before = before.to_string();
    let after = after.to_string();
    if before != after {
        changes.push(ConfigChange {
            field,
            before,
            after,
        });
    }
}

pub(super) fn active_security_preset(config: &Config) -> Option<SecurityPreset> {
    if matches_default_compatible(config) {
        Some(SecurityPreset::DefaultCompatible)
    } else if matches_safer(config) {
        Some(SecurityPreset::Safer)
    } else if matches_strict(config) {
        Some(SecurityPreset::Strict)
    } else {
        None
    }
}

fn matches_default_compatible(config: &Config) -> bool {
    config.workspace.risky_mount_policy == RiskyMountPolicy::Warn
        && !config.harness.pi.read_only_workspace
        && !config.harness.pi.read_only_rootfs
        && config.harness.pi.network == "host"
        && config.harness.pi.build_network == "host"
        && config.ssh.mode == SshMode::Auto
        && config.git.inherit_host
}

fn matches_safer(config: &Config) -> bool {
    config.workspace.risky_mount_policy == RiskyMountPolicy::Deny
        && !config.harness.pi.read_only_workspace
        && !config.harness.pi.read_only_rootfs
        && config.harness.pi.network == "host"
        && config.harness.pi.build_network == "host"
        && config.ssh.mode == SshMode::Auto
        && config.git.inherit_host
}

fn matches_strict(config: &Config) -> bool {
    config.workspace.risky_mount_policy == RiskyMountPolicy::Deny
        && config.harness.pi.read_only_workspace
        && config.harness.pi.read_only_rootfs
        && config.harness.pi.network == "host"
        && config.harness.pi.build_network == "host"
        && config.ssh.mode == SshMode::Managed
        && !config.git.inherit_host
}

fn risky_mount_policy_name(policy: RiskyMountPolicy) -> &'static str {
    match policy {
        RiskyMountPolicy::Warn => "warn",
        RiskyMountPolicy::Deny => "deny",
    }
}

fn ssh_mode_name(mode: SshMode) -> &'static str {
    match mode {
        SshMode::Auto => "auto",
        SshMode::Host => "host",
        SshMode::Managed => "managed",
        SshMode::Off => "off",
    }
}
