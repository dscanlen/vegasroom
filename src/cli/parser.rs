use anyhow::{bail, Result};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ManualLaunch {
    DeferToClap,
    PrintPiHelp,
    PrintShellHelp,
    Pi(PiInvocation),
    Shell(Option<String>),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct PiInvocation {
    pub(super) workspace: Option<String>,
    pub(super) pi_args: Vec<String>,
}

impl PiInvocation {
    fn new(workspace: Option<String>, pi_args: Vec<String>) -> Self {
        Self { workspace, pi_args }
    }

    fn default_workspace() -> Self {
        Self::new(None, Vec::new())
    }
}

pub(super) fn parse_manual_launch(args: &[String]) -> Result<ManualLaunch> {
    let Some(first) = args.get(1) else {
        return Ok(ManualLaunch::Pi(PiInvocation::default_workspace()));
    };

    match first.as_str() {
        "pi" => Ok(parse_explicit_pi(&args[2..])),
        "shell" => parse_explicit_shell(&args[2..]),
        "--" => Ok(ManualLaunch::Pi(PiInvocation::new(
            None,
            args[2..].to_vec(),
        ))),
        "--help" | "-h" | "--version" | "-V" | "init" | "doctor" | "ssh" => {
            Ok(ManualLaunch::DeferToClap)
        }
        value if value.starts_with('-') => Ok(ManualLaunch::Pi(PiInvocation::new(
            None,
            args[1..].to_vec(),
        ))),
        _ => Ok(ManualLaunch::DeferToClap),
    }
}

fn parse_explicit_pi(args: &[String]) -> ManualLaunch {
    if is_help_arg(args.first()) {
        return ManualLaunch::PrintPiHelp;
    }

    ManualLaunch::Pi(parse_pi_invocation(args))
}

fn parse_explicit_shell(args: &[String]) -> Result<ManualLaunch> {
    if is_help_arg(args.first()) {
        return Ok(ManualLaunch::PrintShellHelp);
    }

    parse_shell_workspace(args).map(ManualLaunch::Shell)
}

fn is_help_arg(arg: Option<&String>) -> bool {
    matches!(arg.map(String::as_str), Some("--help" | "-h"))
}

fn parse_pi_invocation(args: &[String]) -> PiInvocation {
    let Some(first) = args.first() else {
        return PiInvocation::default_workspace();
    };

    if first == "--" {
        return PiInvocation::new(None, args[1..].to_vec());
    }

    if first.starts_with('-') {
        return PiInvocation::new(None, args.to_vec());
    }

    let pi_args = if args.get(1).map(String::as_str) == Some("--") {
        args[2..].to_vec()
    } else {
        args[1..].to_vec()
    };

    PiInvocation::new(Some(first.clone()), pi_args)
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

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn argv(values: &[&str]) -> Vec<String> {
        let mut argv = vec!["vr".to_owned()];
        argv.extend(args(values));
        argv
    }

    #[test]
    fn manual_parser_defaults_empty_command_to_pi() {
        assert_eq!(
            parse_manual_launch(&argv(&[])).unwrap(),
            ManualLaunch::Pi(PiInvocation::default_workspace())
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
    fn manual_parser_passes_top_level_leading_flags_to_pi() {
        assert_eq!(
            parse_manual_launch(&argv(&["--session", "abc123"])).unwrap(),
            ManualLaunch::Pi(PiInvocation::new(None, args(&["--session", "abc123"])))
        );
    }

    #[test]
    fn manual_parser_passes_top_level_separator_args_to_pi() {
        assert_eq!(
            parse_manual_launch(&argv(&["--", "ask", "Pi"])).unwrap(),
            ManualLaunch::Pi(PiInvocation::new(None, args(&["ask", "Pi"])))
        );
    }

    #[test]
    fn manual_parser_routes_explicit_pi_help_to_wrapper_help() {
        assert_eq!(
            parse_manual_launch(&argv(&["pi", "--help"])).unwrap(),
            ManualLaunch::PrintPiHelp
        );
        assert_eq!(
            parse_manual_launch(&argv(&["pi", "-h"])).unwrap(),
            ManualLaunch::PrintPiHelp
        );
    }

    #[test]
    fn manual_parser_allows_separator_before_pi_help_arg() {
        assert_eq!(
            parse_manual_launch(&argv(&["pi", "--", "--help"])).unwrap(),
            ManualLaunch::Pi(PiInvocation::new(None, args(&["--help"])))
        );
    }

    #[test]
    fn manual_parser_routes_explicit_shell() {
        assert_eq!(
            parse_manual_launch(&argv(&["shell"])).unwrap(),
            ManualLaunch::Shell(None)
        );
        assert_eq!(
            parse_manual_launch(&argv(&["shell", "my-repo"])).unwrap(),
            ManualLaunch::Shell(Some("my-repo".to_owned()))
        );
        assert_eq!(
            parse_manual_launch(&argv(&["shell", "--help"])).unwrap(),
            ManualLaunch::PrintShellHelp
        );
    }

    #[test]
    fn manual_parser_rejects_shell_extra_arguments() {
        let err = parse_manual_launch(&argv(&["shell", "one", "two"])).unwrap_err();

        assert!(err.to_string().contains("usage: vr shell [workspace]"));
    }

    #[test]
    fn pi_invocation_without_args_uses_default_workspace_and_no_pi_args() {
        let invocation = parse_pi_invocation(&[]);

        assert_eq!(invocation, PiInvocation::default_workspace());
    }

    #[test]
    fn pi_invocation_treats_leading_flag_as_pi_arg() {
        let invocation = parse_pi_invocation(&args(&["--session", "abc123"]));

        assert_eq!(
            invocation,
            PiInvocation::new(None, args(&["--session", "abc123"]))
        );
    }

    #[test]
    fn pi_invocation_treats_first_non_flag_as_workspace() {
        let invocation = parse_pi_invocation(&args(&["my-repo"]));

        assert_eq!(
            invocation,
            PiInvocation::new(Some("my-repo".to_owned()), Vec::new())
        );
    }

    #[test]
    fn pi_invocation_accepts_workspace_before_pi_args() {
        let invocation = parse_pi_invocation(&args(&[".", "--session", "abc123"]));

        assert_eq!(
            invocation,
            PiInvocation::new(Some(".".to_owned()), args(&["--session", "abc123"]))
        );
    }

    #[test]
    fn pi_invocation_strips_separator_after_workspace() {
        let invocation = parse_pi_invocation(&args(&["my-repo", "--", "--help"]));

        assert_eq!(
            invocation,
            PiInvocation::new(Some("my-repo".to_owned()), args(&["--help"]))
        );
    }

    #[test]
    fn pi_invocation_strips_separator_without_workspace() {
        let invocation = parse_pi_invocation(&args(&["--", "--help"]));

        assert_eq!(invocation, PiInvocation::new(None, args(&["--help"])));
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
