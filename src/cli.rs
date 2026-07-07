use std::env;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use crate::{alert, config::Config, docker, doctor, paths::StatePaths, ssh, workspace};

const TOP_LEVEL_AFTER_HELP: &str = r#"Examples:
  vr init --build
  vr doctor
  vr
  vr pi .
  vr shell .
  vr ssh configure

Use `vr pi --help` for Pi workspace and pass-through help.
Use `vr shell --help` for shell workspace help."#;

const INIT_AFTER_HELP: &str = r#"Examples:
  vr init
  vr init --build"#;

const DOCTOR_AFTER_HELP: &str = r#"Examples:
  vr doctor"#;

const SSH_AFTER_HELP: &str = r#"Examples:
  vr ssh configure
  vr ssh configure ~/.ssh ~/work-keys
  vr ssh status"#;

const SSH_CONFIGURE_AFTER_HELP: &str = r#"Examples:
  vr ssh configure
  vr ssh configure ~/.ssh ~/work-keys
  vr ssh configure --follow-symlinks ~/.ssh"#;

const SSH_STATUS_AFTER_HELP: &str = r#"Examples:
  vr ssh status"#;

#[derive(Debug, Parser)]
#[command(name = "vr")]
#[command(
    about = "Run Pi inside a Vegasroom container.",
    version,
    after_help = TOP_LEVEL_AFTER_HELP
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Create or repair Vegasroom local state and managed runtime files.
    #[command(after_help = INIT_AFTER_HELP)]
    Init {
        /// Build the local Pi image after creating state.
        #[arg(long)]
        build: bool,
    },

    /// Check whether the local system is ready to run Vegasroom.
    #[command(after_help = DOCTOR_AFTER_HELP)]
    Doctor,

    /// Configure or inspect Vegasroom SSH key behavior.
    #[command(after_help = SSH_AFTER_HELP)]
    Ssh {
        #[command(subcommand)]
        command: SshCommands,
    },

    /// Launch Pi in the proven Docker/Compose runtime.
    ///
    /// Use `vr pi --help` for workspace and pass-through syntax.
    Pi,

    /// Launch a shell in the same Docker/Compose runtime.
    ///
    /// Use `vr shell --help` for workspace syntax.
    Shell,
}

#[derive(Debug, Subcommand)]
pub enum SshCommands {
    /// Recursively scan SSH key roots and interactively choose managed keys.
    #[command(after_help = SSH_CONFIGURE_AFTER_HELP)]
    Configure {
        /// Follow symlinked directories while scanning. This can scan outside the requested roots.
        #[arg(long)]
        follow_symlinks: bool,

        /// Optional scan roots. Defaults to ~/.ssh when omitted.
        #[arg(value_name = "PATH")]
        paths: Vec<String>,
    },

    /// Show managed SSH key configuration and next-launch behavior.
    #[command(after_help = SSH_STATUS_AFTER_HELP)]
    Status,
}

pub fn run() -> Result<i32> {
    let raw_args: Vec<String> = env::args().collect();
    if let Some(code) = maybe_run_manual_launch(&raw_args)? {
        return Ok(code);
    }

    let cli = Cli::parse();

    match cli.command.unwrap_or(Commands::Pi) {
        Commands::Init { build } => init(build),
        Commands::Doctor => doctor::run(),
        Commands::Ssh { command } => match command {
            SshCommands::Configure {
                paths,
                follow_symlinks,
            } => configure_ssh(&paths, follow_symlinks),
            SshCommands::Status => ssh_status(),
        },
        Commands::Pi => launch_pi(None, Vec::new()),
        Commands::Shell => launch_shell(None),
    }
}

fn maybe_run_manual_launch(args: &[String]) -> Result<Option<i32>> {
    match parse_manual_launch(args)? {
        ManualLaunch::DeferToClap => Ok(None),
        ManualLaunch::PrintPiHelp => {
            print_pi_help();
            Ok(Some(0))
        }
        ManualLaunch::PrintShellHelp => {
            print_shell_help();
            Ok(Some(0))
        }
        ManualLaunch::Pi(invocation) => {
            let PiInvocation { workspace, pi_args } = invocation;
            launch_pi(workspace.as_deref(), pi_args).map(Some)
        }
        ManualLaunch::Shell(workspace) => launch_shell(workspace.as_deref()).map(Some),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ManualLaunch {
    DeferToClap,
    PrintPiHelp,
    PrintShellHelp,
    Pi(PiInvocation),
    Shell(Option<String>),
}

#[derive(Debug, PartialEq, Eq)]
struct PiInvocation {
    workspace: Option<String>,
    pi_args: Vec<String>,
}

impl PiInvocation {
    fn new(workspace: Option<String>, pi_args: Vec<String>) -> Self {
        Self { workspace, pi_args }
    }

    fn default_workspace() -> Self {
        Self::new(None, Vec::new())
    }
}

fn parse_manual_launch(args: &[String]) -> Result<ManualLaunch> {
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

fn init(build: bool) -> Result<i32> {
    let state = StatePaths::default()?;
    let report = state.ensure()?;
    report.print();
    repair_managed_runtime_config()?;

    if build {
        let config = Config::load_or_default()?;
        println!("Building Pi image: {}", config.harness.pi.image);
        docker::build_pi_image(&config)?;
    }

    Ok(0)
}

fn configure_ssh(paths: &[String], follow_symlinks: bool) -> Result<i32> {
    let state = StatePaths::default()?;
    let _ = state.ensure()?;
    repair_managed_runtime_config()?;
    ssh::configure(paths, follow_symlinks)
}

fn ssh_status() -> Result<i32> {
    let state = StatePaths::default()?;
    let _ = state.ensure()?;
    repair_managed_runtime_config()?;
    ssh::status()
}

fn launch_pi(workspace_arg: Option<&str>, pi_args: Vec<String>) -> Result<i32> {
    let state = StatePaths::default()?;
    let _ = state.ensure()?;
    repair_managed_runtime_config()?;
    state.show_disclaimer_once()?;

    let config = Config::load_or_default()?;
    let workspace = workspace::resolve_workspace(workspace_arg, &config)?;
    print_workspace_messages(&workspace, &config);

    docker::ensure_pi_image_exists(&config)
        .with_context(|| "Pi image was not found. Run: vr init --build")?;
    docker::run_pi(&config, &workspace, &pi_args)
}

fn launch_shell(workspace_arg: Option<&str>) -> Result<i32> {
    let state = StatePaths::default()?;
    let _ = state.ensure()?;
    repair_managed_runtime_config()?;

    let config = Config::load_or_default()?;
    let workspace = workspace::resolve_workspace(workspace_arg, &config)?;
    print_workspace_messages(&workspace, &config);

    docker::ensure_pi_image_exists(&config)
        .with_context(|| "Pi image was not found. Run: vr init --build")?;
    docker::run_shell(&config, &workspace)
}

fn print_workspace_messages(workspace: &workspace::ResolvedWorkspace, config: &Config) {
    if workspace.created {
        println!("Created workspace: {}", workspace.path.display());
    }

    if config.harness.pi.read_only_workspace {
        println!(
            "Workspace will be mounted read-only: {}",
            workspace.path.display()
        );
    }

    for warning in &workspace.warnings {
        eprintln!("{}: {warning}", alert::warn());
    }
}

fn repair_managed_runtime_config() -> Result<()> {
    let mut config = Config::load_or_default()?;
    if config.uses_managed_compose_file()? {
        return Ok(());
    }

    config.set_managed_compose_file()?;
    config.save_to_default_path()?;
    println!(
        "Configured managed Compose runtime: {}",
        config.docker.compose_file
    );
    Ok(())
}

fn print_pi_help() {
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

fn print_shell_help() {
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
    use clap::CommandFactory;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn argv(values: &[&str]) -> Vec<String> {
        let mut argv = vec!["vr".to_owned()];
        argv.extend(args(values));
        argv
    }

    fn top_level_help() -> String {
        let mut command = Cli::command();
        let mut buffer = Vec::new();
        command.write_long_help(&mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
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

    #[test]
    fn top_level_help_includes_consistent_examples() {
        let help = top_level_help();

        assert!(help.contains("Run Pi inside a Vegasroom container."));
        assert!(help.contains("Examples:"));
        assert!(help.contains("vr init --build"));
        assert!(help.contains("vr pi ."));
        assert!(help.contains("Use `vr pi --help`"));
    }

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
