use std::env;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use crate::{config::Config, docker, doctor, paths::StatePaths, ssh, workspace};

#[derive(Debug, Parser)]
#[command(name = "vr")]
#[command(about = "Vegasroom CLI", version)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Create or repair Vegasroom local state and managed runtime files.
    Init {
        /// Build the local Pi image after creating state.
        #[arg(long)]
        build: bool,
    },

    /// Check whether the local system is ready to run Vegasroom.
    Doctor,

    /// Configure or inspect Vegasroom SSH key behavior.
    Ssh {
        #[command(subcommand)]
        command: SshCommands,
    },

    /// Launch Pi in the proven Docker/Compose runtime.
    Pi,

    /// Launch a shell in the same Docker/Compose runtime.
    Shell,
}

#[derive(Debug, Subcommand)]
pub enum SshCommands {
    /// Recursively scan SSH key roots and interactively choose managed keys.
    Configure {
        /// Follow symlinked directories while scanning. This can scan outside the requested roots.
        #[arg(long)]
        follow_symlinks: bool,

        /// Optional scan roots. Defaults to ~/.ssh when omitted.
        paths: Vec<String>,
    },

    /// Show managed SSH key configuration and next-launch behavior.
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
    let Some(first) = args.get(1) else {
        return launch_pi(None, Vec::new()).map(Some);
    };

    match first.as_str() {
        "pi" => {
            let rest = &args[2..];
            if matches!(rest.first().map(String::as_str), Some("--help" | "-h")) {
                print_pi_help();
                return Ok(Some(0));
            }

            let invocation = parse_pi_invocation(rest);
            launch_pi(invocation.workspace.as_deref(), invocation.pi_args).map(Some)
        }
        "shell" => {
            let rest = &args[2..];
            if matches!(rest.first().map(String::as_str), Some("--help" | "-h")) {
                print_shell_help();
                return Ok(Some(0));
            }

            let workspace = parse_shell_workspace(rest)?;
            launch_shell(workspace.as_deref()).map(Some)
        }
        "--" => launch_pi(None, args[2..].to_vec()).map(Some),
        "--help" | "-h" | "--version" | "-V" | "init" | "doctor" | "ssh" => Ok(None),
        value if value.starts_with('-') => launch_pi(None, args[1..].to_vec()).map(Some),
        _ => Ok(None),
    }
}

struct PiInvocation {
    workspace: Option<String>,
    pi_args: Vec<String>,
}

fn parse_pi_invocation(args: &[String]) -> PiInvocation {
    let Some(first) = args.first() else {
        return PiInvocation {
            workspace: None,
            pi_args: Vec::new(),
        };
    };

    if first == "--" {
        return PiInvocation {
            workspace: None,
            pi_args: args[1..].to_vec(),
        };
    }

    if first.starts_with('-') {
        return PiInvocation {
            workspace: None,
            pi_args: args.to_vec(),
        };
    }

    let pi_args = if args.get(1).map(String::as_str) == Some("--") {
        args[2..].to_vec()
    } else {
        args[1..].to_vec()
    };

    PiInvocation {
        workspace: Some(first.clone()),
        pi_args,
    }
}

fn parse_shell_workspace(args: &[String]) -> Result<Option<String>> {
    if args.is_empty() {
        return Ok(None);
    }

    if args.len() == 1 {
        return Ok(Some(args[0].clone()));
    }

    bail!("usage: vr shell [workspace]");
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
    print_workspace_messages(&workspace);

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
    print_workspace_messages(&workspace);

    docker::ensure_pi_image_exists(&config)
        .with_context(|| "Pi image was not found. Run: vr init --build")?;
    docker::run_shell(&config, &workspace)
}

fn print_workspace_messages(workspace: &workspace::ResolvedWorkspace) {
    if workspace.created {
        println!("Created workspace: {}", workspace.path.display());
    }

    for warning in &workspace.warnings {
        eprintln!("WARN: {warning}");
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
    println!(
        r#"Launch Pi in a Vegasroom workspace.

Usage:
  vr pi [workspace] [pi-args...]
  vr pi [workspace] -- [pi-args...]
  vr [pi-args...]
  vr -- [pi-args...]

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

Notes:
  The explicit -- separator is preferred when Pi arguments are ambiguous.
  Direct Pi flags after vr pi are supported when the first token begins with '-'.
  Direct Pi flags after a workspace are passed through to Pi.
  Use vr --help for top-level Vegasroom help."#
    );
}

fn print_shell_help() {
    println!(
        r#"Launch a shell in a Vegasroom workspace.

Usage:
  vr shell [workspace]

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
  vr shell my-git-repo"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn pi_invocation_without_args_uses_default_workspace_and_no_pi_args() {
        let invocation = parse_pi_invocation(&[]);

        assert!(invocation.workspace.is_none());
        assert!(invocation.pi_args.is_empty());
    }

    #[test]
    fn pi_invocation_treats_leading_flag_as_pi_arg() {
        let invocation = parse_pi_invocation(&args(&["--session", "abc123"]));

        assert!(invocation.workspace.is_none());
        assert_eq!(invocation.pi_args, args(&["--session", "abc123"]));
    }

    #[test]
    fn pi_invocation_accepts_workspace_before_pi_args() {
        let invocation = parse_pi_invocation(&args(&[".", "--session", "abc123"]));

        assert_eq!(invocation.workspace.as_deref(), Some("."));
        assert_eq!(invocation.pi_args, args(&["--session", "abc123"]));
    }

    #[test]
    fn pi_invocation_strips_separator_after_workspace() {
        let invocation = parse_pi_invocation(&args(&["my-repo", "--", "--help"]));

        assert_eq!(invocation.workspace.as_deref(), Some("my-repo"));
        assert_eq!(invocation.pi_args, args(&["--help"]));
    }

    #[test]
    fn pi_invocation_strips_separator_without_workspace() {
        let invocation = parse_pi_invocation(&args(&["--", "--help"]));

        assert!(invocation.workspace.is_none());
        assert_eq!(invocation.pi_args, args(&["--help"]));
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
