use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::{config::Config, docker, doctor, paths::StatePaths, ssh};

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
        Commands::Pi => launch_pi(),
        Commands::Shell => launch_shell(),
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

fn launch_pi() -> Result<i32> {
    let state = StatePaths::default()?;
    let _ = state.ensure()?;
    repair_managed_runtime_config()?;
    state.show_disclaimer_once()?;

    let config = Config::load_or_default()?;
    docker::ensure_pi_image_exists(&config)
        .with_context(|| "Pi image was not found. Run: vr init --build")?;
    docker::run_pi(&config)
}

fn launch_shell() -> Result<i32> {
    let state = StatePaths::default()?;
    let _ = state.ensure()?;
    repair_managed_runtime_config()?;

    let config = Config::load_or_default()?;
    docker::ensure_pi_image_exists(&config)
        .with_context(|| "Pi image was not found. Run: vr init --build")?;
    docker::run_shell(&config)
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
