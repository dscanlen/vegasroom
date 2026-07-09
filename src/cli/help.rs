pub(super) const TOP_LEVEL_AFTER_HELP: &str = r#"Examples:
  vr init --build
  vr doctor
  vr config
  vr
  vr pi .
  vr shell .
  vr ssh configure

Use `vr pi --help` for Pi workspace and pass-through help.
Use `vr shell --help` for shell workspace help."#;

pub(super) const INIT_AFTER_HELP: &str = r#"Examples:
  vr init
  vr init --build"#;

pub(super) const DOCTOR_AFTER_HELP: &str = r#"Examples:
  vr doctor"#;

pub(super) const CONFIG_AFTER_HELP: &str = r#"Examples:
  vr config

Notes:
  Opens the interactive Vegasroom configuration TUI.
  Manual YAML editing remains supported at ~/.vegasroom/config.yaml."#;

pub(super) const SSH_AFTER_HELP: &str = r#"Examples:
  vr ssh configure
  vr ssh configure ~/.ssh ~/work-keys
  vr ssh status"#;

pub(super) const SSH_CONFIGURE_AFTER_HELP: &str = r#"Examples:
  vr ssh configure
  vr ssh configure ~/.ssh ~/work-keys
  vr ssh configure --follow-symlinks ~/.ssh"#;

pub(super) const SSH_STATUS_AFTER_HELP: &str = r#"Examples:
  vr ssh status"#;

pub(super) fn print_pi_help() {
    println!("{}", pi_help_text());
}

fn pi_help_text() -> &'static str {
    r#"Launch Pi in a Vegasroom workspace.

Usage:
  vr pi [workspace] [pi-args...]
  vr pi [workspace] -- [pi-args...]
  vr [pi-flags...]
  vr -- [pi-args...]

Arguments:
  workspace       Optional host workspace to mount at /workspace
  pi-args         Arguments passed through to Pi

Workspace resolution:
  no workspace     ~/.vegasroom/workspace
  .                current host directory
  name             ~/.vegasroom/workspace/name
  relative/path    relative to current host directory
  ~/path           expanded against host home
  /absolute/path   used directly if it exists

Examples:
  vr pi
  vr pi .
  vr pi my-git-repo
  vr pi ~/workspace/my-git-repo
  vr pi /home/dan/workspace/my-git-repo
  vr pi --session abc123
  vr pi . --session abc123
  vr pi . -- --help
  vr --session abc123
  vr -- ask Pi a question

Notes:
  The explicit -- separator is preferred when Pi arguments are ambiguous.
  Direct Pi flags after vr pi are supported when the first token begins with '-'.
  Direct Pi flags after a workspace are passed through to Pi.
  At top level, direct pass-through is only used when the first token begins with '-'.
  Top-level --help, -h, --version, and -V are reserved for Vegasroom.
  Use vr --help for top-level Vegasroom help."#
}

pub(super) fn print_shell_help() {
    println!("{}", shell_help_text());
}

fn shell_help_text() -> &'static str {
    r#"Launch a shell in a Vegasroom workspace.

Usage:
  vr shell [workspace]

Arguments:
  workspace       Optional host workspace to mount at /workspace

Workspace resolution:
  no workspace     ~/.vegasroom/workspace
  .                current host directory
  name             ~/.vegasroom/workspace/name
  relative/path    relative to current host directory
  ~/path           expanded against host home
  /absolute/path   used directly if it exists

Examples:
  vr shell
  vr shell .
  vr shell my-git-repo

Notes:
  Shell does not accept pass-through command arguments."#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_help_describes_actual_top_level_pass_through_behavior() {
        let help = pi_help_text();

        assert!(help.contains("Usage:\n  vr pi [workspace] [pi-args...]"));
        assert!(help.contains("Arguments:\n  workspace"));
        assert!(help.contains("vr [pi-flags...]"));
        assert!(help.contains("vr -- [pi-args...]"));
        assert!(
            help.contains("direct pass-through is only used when the first token begins with '-'")
        );
        assert!(help.contains("--version, and -V are reserved for Vegasroom"));
    }

    #[test]
    fn shell_help_describes_current_command_surface() {
        let help = shell_help_text();

        assert!(help.contains("Usage:\n  vr shell [workspace]"));
        assert!(help.contains("Arguments:\n  workspace"));
        assert!(help.contains("Notes:"));
        assert!(!help.contains("shell [workspace] [args"));
    }
}
