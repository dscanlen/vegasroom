pub(super) const TOP_LEVEL_AFTER_HELP: &str = r#"Examples:
  vr init --build
  vr doctor
  vr config
  vr
  vr pi .
  vr claude .
  vr shell .

Running `vr` with no subcommand launches the default harness (Pi).
Use `vr pi --help` or `vr claude --help` for workspace and pass-through help.
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

pub(super) fn print_harness_help(descriptor: &crate::harness::HarnessDescriptor) {
    let is_default = descriptor.id == super::parser::default_harness().id;
    println!("{}", harness_help_text(descriptor, is_default));
}

/// Help for `vr <harness> --help`. The top-level `vr [flags]` / `vr -- args`
/// shortcuts only launch the default harness, so they are listed only for it.
fn harness_help_text(descriptor: &crate::harness::HarnessDescriptor, is_default: bool) -> String {
    let name = descriptor.id;
    let display_name = descriptor.display_name;
    let command = descriptor.default_command;
    let args_label = format!("{command}-args");

    let top_level_usage = if is_default {
        format!("\n  vr [{command}-flags...]\n  vr -- [{command}-args...]")
    } else {
        String::new()
    };
    let top_level_examples = if is_default {
        format!("\n  vr --session abc123\n  vr -- ask {display_name} a question")
    } else {
        String::new()
    };
    let top_level_notes = if is_default {
        format!(
            "\n  {display_name} is the default harness: `vr` with no subcommand launches it.\n  At top level, direct pass-through is only used when the first token begins with '-'."
        )
    } else {
        format!("\n  Top-level `vr [flags]` and `vr -- args` launch the default harness, not {display_name}.")
    };

    format!(
        r#"Launch {display_name} in a Vegasroom workspace.

Usage:
  vr {name} [workspace] [{command}-args...]
  vr {name} [workspace] -- [{command}-args...]{top_level_usage}

Arguments:
  workspace       Optional host workspace to mount at /workspace
  {args_label:<16}Arguments passed through to {display_name}

Workspace resolution:
  no workspace     ~/.vegasroom/workspace
  .                current host directory
  name             ~/.vegasroom/workspace/name
  relative/path    relative to current host directory
  ~/path           expanded against host home
  /absolute/path   used directly if it exists

Examples:
  vr {name}
  vr {name} .
  vr {name} my-git-repo
  vr {name} . -- --help{top_level_examples}

Notes:
  The explicit -- separator is preferred when {display_name} arguments are ambiguous.
  Direct {display_name} flags after vr {name} are supported when the first token begins with '-'.
  Direct {display_name} flags after a workspace are passed through to {display_name}.{top_level_notes}
  Top-level --help, -h, --version, and -V are reserved for Vegasroom.
  Use vr --help for top-level Vegasroom help."#
    )
}

pub(super) fn print_shell_help() {
    println!("{}", shell_help_text());
}

fn shell_help_text() -> &'static str {
    r#"Launch a shell in a Vegasroom workspace.

Usage:
  vr shell [workspace]
  vr shell [workspace] -- <command> [args...]

Arguments:
  workspace       Optional host workspace to mount at /workspace
  command         Optional command to run instead of an interactive shell

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
  vr shell . -- sh -c 'node --version'

Notes:
  Commands after -- run without a pseudo-TTY and return their exit status.
  Without a command, starts an interactive sh shell."#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_harness_help_lists_top_level_shortcuts() {
        let help = harness_help_text(&crate::harness::PI, true);

        assert!(help.contains("vr pi [workspace] [pi-args...]"));
        assert!(help.contains("vr [pi-flags...]"));
        assert!(help.contains("vr -- [pi-args...]"));
        assert!(help.contains("Pi is the default harness"));
    }

    #[test]
    fn non_default_harness_help_omits_top_level_shortcuts() {
        let help = harness_help_text(&crate::harness::CLAUDE, false);

        assert!(help.starts_with("Launch Claude Code in a Vegasroom workspace."));
        assert!(help.contains("vr claude [workspace] [claude-args...]"));
        assert!(help.contains("vr claude [workspace] -- [claude-args...]"));
        assert!(help.contains("vr claude . -- --help"));
        assert!(help.contains("\n  claude-args     Arguments passed through to Claude Code"));
        assert!(help.contains("\n  workspace       Optional host workspace"));
        assert!(!help.contains("vr [claude-flags...]"));
        assert!(!help.contains("vr -- [claude-args...]"));
        assert!(help.contains("launch the default harness, not Claude Code"));
    }

    #[test]
    fn shell_help_describes_current_command_surface() {
        let help = shell_help_text();

        assert!(help.contains("Usage:\n  vr shell [workspace]"));
        assert!(help.contains("Arguments:\n  workspace"));
        assert!(help.contains("Notes:"));
        assert!(help.contains("vr shell [workspace] -- <command> [args...]"));
        assert!(help.contains("without a pseudo-TTY"));
    }
}
