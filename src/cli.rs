use std::env;

use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod help;
mod parser;

use help::{
    DOCTOR_AFTER_HELP, INIT_AFTER_HELP, SSH_AFTER_HELP, SSH_CONFIGURE_AFTER_HELP,
    SSH_STATUS_AFTER_HELP, TOP_LEVEL_AFTER_HELP,
};
use parser::{parse_manual_launch, ManualLaunch, PiInvocation};

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
        Commands::Init { build } => commands::init(build),
        Commands::Doctor => commands::doctor(),
        Commands::Ssh { command } => match command {
            SshCommands::Configure {
                paths,
                follow_symlinks,
            } => commands::configure_ssh(&paths, follow_symlinks),
            SshCommands::Status => commands::ssh_status(),
        },
        Commands::Pi => commands::launch_pi(None, Vec::new()),
        Commands::Shell => commands::launch_shell(None),
    }
}

fn maybe_run_manual_launch(args: &[String]) -> Result<Option<i32>> {
    match parse_manual_launch(args)? {
        ManualLaunch::DeferToClap => Ok(None),
        ManualLaunch::PrintPiHelp => {
            help::print_pi_help();
            Ok(Some(0))
        }
        ManualLaunch::PrintShellHelp => {
            help::print_shell_help();
            Ok(Some(0))
        }
        ManualLaunch::Pi(invocation) => {
            let PiInvocation { workspace, pi_args } = invocation;
            commands::launch_pi(workspace.as_deref(), pi_args).map(Some)
        }
        ManualLaunch::Shell(workspace) => commands::launch_shell(workspace.as_deref()).map(Some),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn top_level_help() -> String {
        let mut command = Cli::command();
        let mut buffer = Vec::new();
        command.write_long_help(&mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
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
}
