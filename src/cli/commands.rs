use anyhow::{Context, Result};

use crate::{
    alert, config::Config, docker, doctor as doctor_runner, paths::StatePaths, ssh, workspace,
};

pub(super) fn init(build: bool) -> Result<i32> {
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

pub(super) fn doctor() -> Result<i32> {
    doctor_runner::run()
}

pub(super) fn configure_ssh(paths: &[String], follow_symlinks: bool) -> Result<i32> {
    let state = StatePaths::default()?;
    let _ = state.ensure()?;
    repair_managed_runtime_config()?;
    ssh::configure(paths, follow_symlinks)
}

pub(super) fn ssh_status() -> Result<i32> {
    let state = StatePaths::default()?;
    let _ = state.ensure()?;
    repair_managed_runtime_config()?;
    ssh::status()
}

pub(super) fn launch_pi(workspace_arg: Option<&str>, pi_args: Vec<String>) -> Result<i32> {
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

pub(super) fn launch_shell(workspace_arg: Option<&str>) -> Result<i32> {
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
