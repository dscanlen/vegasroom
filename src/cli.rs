use std::env;

use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod help;
mod parser;

use help::{CONFIG_AFTER_HELP, DOCTOR_AFTER_HELP, INIT_AFTER_HELP, TOP_LEVEL_AFTER_HELP};
use parser::{default_harness, parse_manual_launch, ManualLaunch};

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

    /// Open the interactive Vegasroom configuration TUI.
    #[command(after_help = CONFIG_AFTER_HELP)]
    Config,

    /// Launch Pi in the proven Docker/Compose runtime.
    ///
    /// Use `vr pi --help` for workspace and pass-through syntax.
    Pi,

    /// Launch a shell in the same Docker/Compose runtime.
    ///
    /// Use `vr shell --help` for workspace syntax.
    Shell,
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
        Commands::Config => commands::config(),
        Commands::Pi => commands::launch_harness(default_harness(), None, Vec::new()),
        Commands::Shell => commands::launch_shell(None, Vec::new()),
    }
}

fn maybe_run_manual_launch(args: &[String]) -> Result<Option<i32>> {
    match parse_manual_launch(args)? {
        ManualLaunch::DeferToClap => Ok(None),
        ManualLaunch::PrintHarnessHelp(descriptor) => {
            help::print_harness_help(descriptor);
            Ok(Some(0))
        }
        ManualLaunch::PrintShellHelp => {
            help::print_shell_help();
            Ok(Some(0))
        }
        ManualLaunch::Harness {
            descriptor,
            workspace,
            harness_args,
        } => commands::launch_harness(descriptor, workspace.as_deref(), harness_args).map(Some),
        ManualLaunch::Shell { workspace, command } => {
            commands::launch_shell(workspace.as_deref(), command).map(Some)
        }
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
        assert!(help.contains("vr config"));
        assert!(help.contains("vr pi ."));
        assert!(help.contains("Use `vr pi --help`"));
    }
}
