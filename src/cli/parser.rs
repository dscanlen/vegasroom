use anyhow::{bail, Result};

use crate::harness::{self, HarnessDescriptor};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ManualLaunch {
    DeferToClap,
    PrintHarnessHelp(&'static HarnessDescriptor),
    PrintShellHelp,
    Harness {
        descriptor: &'static HarnessDescriptor,
        workspace: Option<String>,
        harness_args: Vec<String>,
    },
    Shell {
        workspace: Option<String>,
        command: Vec<String>,
    },
}

pub(super) fn default_harness() -> &'static HarnessDescriptor {
    harness::HARNESSES[0]
}

pub(super) fn parse_manual_launch(args: &[String]) -> Result<ManualLaunch> {
    let Some(first) = args.get(1) else {
        return Ok(harness_invocation(default_harness(), None, Vec::new()));
    };

    match first.as_str() {
        "shell" => parse_explicit_shell(&args[2..]),
        "--" => Ok(harness_invocation(
            default_harness(),
            None,
            args[2..].to_vec(),
        )),
        "--help" | "-h" | "--version" | "-V" | "init" | "doctor" | "config" | "ssh" => {
            Ok(ManualLaunch::DeferToClap)
        }
        value if value.starts_with('-') => Ok(harness_invocation(
            default_harness(),
            None,
            args[1..].to_vec(),
        )),
        name => {
            if let Some(descriptor) = harness::find_by_id(name) {
                Ok(parse_explicit_harness(descriptor, &args[2..]))
            } else {
                Ok(ManualLaunch::DeferToClap)
            }
        }
    }
}

fn harness_invocation(
    descriptor: &'static HarnessDescriptor,
    workspace: Option<String>,
    harness_args: Vec<String>,
) -> ManualLaunch {
    ManualLaunch::Harness {
        descriptor,
        workspace,
        harness_args,
    }
}

fn parse_explicit_harness(descriptor: &'static HarnessDescriptor, args: &[String]) -> ManualLaunch {
    if is_help_arg(args.first()) {
        return ManualLaunch::PrintHarnessHelp(descriptor);
    }
    parse_harness_invocation(descriptor, args)
}

fn parse_harness_invocation(
    descriptor: &'static HarnessDescriptor,
    args: &[String],
) -> ManualLaunch {
    let Some(first) = args.first() else {
        return harness_invocation(descriptor, None, Vec::new());
    };

    if first == "--" {
        return harness_invocation(descriptor, None, args[1..].to_vec());
    }

    if first.starts_with('-') {
        return harness_invocation(descriptor, None, args.to_vec());
    }

    let harness_args = if args.get(1).map(String::as_str) == Some("--") {
        args[2..].to_vec()
    } else {
        args[1..].to_vec()
    };

    harness_invocation(descriptor, Some(first.clone()), harness_args)
}

fn parse_explicit_shell(args: &[String]) -> Result<ManualLaunch> {
    if is_help_arg(args.first()) {
        return Ok(ManualLaunch::PrintShellHelp);
    }

    let (workspace_args, command) = match args.iter().position(|arg| arg == "--") {
        Some(index) if index + 1 < args.len() => (&args[..index], args[index + 1..].to_vec()),
        Some(_) => bail!("usage: vr shell [workspace] -- <command> [args...]"),
        None => (args, Vec::new()),
    };
    Ok(ManualLaunch::Shell {
        workspace: parse_shell_workspace(workspace_args)?,
        command,
    })
}

fn is_help_arg(arg: Option<&String>) -> bool {
    matches!(arg.map(String::as_str), Some("--help" | "-h"))
}

fn parse_shell_workspace(args: &[String]) -> Result<Option<String>> {
    match args {
        [] => Ok(None),
        [workspace] => Ok(Some(workspace.clone())),
        _ => bail!("usage: vr shell [workspace]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn argv(values: &[&str]) -> Vec<String> {
        let mut argv = vec!["vr".to_owned()];
        argv.extend(args(values));
        argv
    }

    fn pi_harness(workspace: Option<&str>, harness_args: Vec<String>) -> ManualLaunch {
        ManualLaunch::Harness {
            descriptor: &harness::PI,
            workspace: workspace.map(str::to_owned),
            harness_args,
        }
    }

    #[test]
    fn manual_parser_defaults_empty_command_to_default_harness() {
        assert_eq!(
            parse_manual_launch(&argv(&[])).unwrap(),
            pi_harness(None, Vec::new())
        );
    }

    #[test]
    fn manual_parser_defers_clap_owned_commands_and_top_level_flags() {
        let cases: &[&[&str]] = &[
            &["--help"],
            &["-h"],
            &["--version"],
            &["-V"],
            &["init"],
            &["doctor"],
            &["config"],
            &["ssh"],
            &["unknown"],
        ];

        for &values in cases {
            assert_eq!(
                parse_manual_launch(&argv(values)).unwrap(),
                ManualLaunch::DeferToClap
            );
        }
    }

    #[test]
    fn manual_parser_passes_top_level_leading_flags_to_default_harness() {
        assert_eq!(
            parse_manual_launch(&argv(&["--session", "abc123"])).unwrap(),
            pi_harness(None, args(&["--session", "abc123"]))
        );
    }

    #[test]
    fn manual_parser_passes_top_level_separator_args_to_default_harness() {
        assert_eq!(
            parse_manual_launch(&argv(&["--", "ask", "Pi"])).unwrap(),
            pi_harness(None, args(&["ask", "Pi"]))
        );
    }

    #[test]
    fn manual_parser_routes_explicit_harness_help_to_wrapper_help() {
        assert_eq!(
            parse_manual_launch(&argv(&["pi", "--help"])).unwrap(),
            ManualLaunch::PrintHarnessHelp(&harness::PI)
        );
        assert_eq!(
            parse_manual_launch(&argv(&["pi", "-h"])).unwrap(),
            ManualLaunch::PrintHarnessHelp(&harness::PI)
        );
    }

    #[test]
    fn manual_parser_routes_claude_subcommand() {
        let claude = |workspace: Option<&str>, harness_args: Vec<String>| ManualLaunch::Harness {
            descriptor: &harness::CLAUDE,
            workspace: workspace.map(str::to_owned),
            harness_args,
        };

        assert_eq!(
            parse_manual_launch(&argv(&["claude"])).unwrap(),
            claude(None, Vec::new())
        );
        assert_eq!(
            parse_manual_launch(&argv(&["claude", "."])).unwrap(),
            claude(Some("."), Vec::new())
        );
        assert_eq!(
            parse_manual_launch(&argv(&["claude", ".", "--", "--help"])).unwrap(),
            claude(Some("."), args(&["--help"]))
        );
        assert_eq!(
            parse_manual_launch(&argv(&["claude", "--resume"])).unwrap(),
            claude(None, args(&["--resume"]))
        );
        assert_eq!(
            parse_manual_launch(&argv(&["claude", "--help"])).unwrap(),
            ManualLaunch::PrintHarnessHelp(&harness::CLAUDE)
        );
    }

    #[test]
    fn manual_parser_top_level_flags_still_go_to_default_harness_not_claude() {
        assert_eq!(
            parse_manual_launch(&argv(&["--resume"])).unwrap(),
            pi_harness(None, args(&["--resume"]))
        );
    }

    #[test]
    fn manual_parser_allows_separator_before_harness_help_arg() {
        assert_eq!(
            parse_manual_launch(&argv(&["pi", "--", "--help"])).unwrap(),
            pi_harness(None, args(&["--help"]))
        );
    }

    #[test]
    fn manual_parser_routes_explicit_shell() {
        assert_eq!(
            parse_manual_launch(&argv(&["shell"])).unwrap(),
            ManualLaunch::Shell {
                workspace: None,
                command: Vec::new(),
            }
        );
        assert_eq!(
            parse_manual_launch(&argv(&["shell", "my-repo"])).unwrap(),
            ManualLaunch::Shell {
                workspace: Some("my-repo".to_owned()),
                command: Vec::new(),
            }
        );
        assert_eq!(
            parse_manual_launch(&argv(&["shell", "--help"])).unwrap(),
            ManualLaunch::PrintShellHelp
        );
    }

    #[test]
    fn manual_parser_passes_shell_command_without_reinterpreting_arguments() {
        for workspace in [None, Some("my repo")] {
            let mut values = vec!["shell"];
            values.extend(workspace);
            values.extend(["--", "sh", "-eu", "-c", "echo \"$1\"", "test", "a b", "--"]);
            assert_eq!(
                parse_manual_launch(&argv(&values)).unwrap(),
                ManualLaunch::Shell {
                    workspace: workspace.map(str::to_owned),
                    command: args(&["sh", "-eu", "-c", "echo \"$1\"", "test", "a b", "--"]),
                }
            );
        }
    }

    #[test]
    fn manual_parser_rejects_empty_shell_command() {
        assert!(parse_manual_launch(&argv(&["shell", "--"])).is_err());
        assert!(parse_manual_launch(&argv(&["shell", ".", "--"])).is_err());
    }

    #[test]
    fn manual_parser_rejects_shell_extra_arguments() {
        let err = parse_manual_launch(&argv(&["shell", "one", "two"])).unwrap_err();

        assert!(err.to_string().contains("usage: vr shell [workspace]"));
    }

    #[test]
    fn harness_invocation_without_args_uses_default_workspace_and_no_harness_args() {
        let launch = parse_harness_invocation(&harness::PI, &[]);

        assert_eq!(launch, pi_harness(None, Vec::new()));
    }

    #[test]
    fn harness_invocation_treats_leading_flag_as_harness_arg() {
        let launch = parse_harness_invocation(&harness::PI, &args(&["--session", "abc123"]));

        assert_eq!(launch, pi_harness(None, args(&["--session", "abc123"])));
    }

    #[test]
    fn harness_invocation_treats_first_non_flag_as_workspace() {
        let launch = parse_harness_invocation(&harness::PI, &args(&["my-repo"]));

        assert_eq!(launch, pi_harness(Some("my-repo"), Vec::new()));
    }

    #[test]
    fn harness_invocation_accepts_workspace_before_harness_args() {
        let launch = parse_harness_invocation(&harness::PI, &args(&[".", "--session", "abc123"]));

        assert_eq!(
            launch,
            pi_harness(Some("."), args(&["--session", "abc123"]))
        );
    }

    #[test]
    fn harness_invocation_strips_separator_after_workspace() {
        let launch = parse_harness_invocation(&harness::PI, &args(&["my-repo", "--", "--help"]));

        assert_eq!(launch, pi_harness(Some("my-repo"), args(&["--help"])));
    }

    #[test]
    fn harness_invocation_strips_separator_without_workspace() {
        let launch = parse_harness_invocation(&harness::PI, &args(&["--", "--help"]));

        assert_eq!(launch, pi_harness(None, args(&["--help"])));
    }

    #[test]
    fn shell_workspace_accepts_zero_or_one_argument() {
        assert!(parse_shell_workspace(&[]).unwrap().is_none());
        assert_eq!(
            parse_shell_workspace(&args(&["my-repo"]))
                .unwrap()
                .as_deref(),
            Some("my-repo")
        );
    }

    #[test]
    fn shell_workspace_rejects_extra_arguments() {
        let err = parse_shell_workspace(&args(&["one", "two"])).unwrap_err();

        assert!(err.to_string().contains("usage: vr shell [workspace]"));
    }
}
