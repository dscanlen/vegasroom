use anyhow::{Context, Result};

use crate::{
    alert, config::Config, config_ui, docker, doctor as doctor_runner, paths::StatePaths, workspace,
};

pub(super) fn init(build: bool) -> Result<i32> {
    let state = StatePaths::default()?;
    let report = state.ensure()?;
    report.print();
    repair_managed_runtime_config()?;

    if build {
        let config = Config::load_or_default()?;
        config.validate_semantics()?;
        println!("Building Pi image: {}", config.harness.pi.image);
        docker::build_pi_image(&config)?;
    }

    Ok(0)
}

pub(super) fn doctor() -> Result<i32> {
    doctor_runner::run()
}

pub(super) fn config() -> Result<i32> {
    let state = StatePaths::default()?;
    let _ = state.ensure()?;
    config_ui::run()
}

pub(super) fn launch_pi(workspace_arg: Option<&str>, pi_args: Vec<String>) -> Result<i32> {
    let state = StatePaths::default()?;
    let _ = state.ensure()?;
    repair_managed_runtime_config()?;
    state.show_disclaimer_once()?;

    let config = Config::load_or_default()?;
    config.validate_semantics()?;
    let workspace = workspace::resolve_workspace(workspace_arg, &config)?;
    print_workspace_messages(&workspace, &config);
    print_environment_image_warning(&config)?;

    docker::ensure_pi_image_exists(&config)
        .with_context(|| "Pi/environment image was not ready. Run: vr init --build")?;
    docker::run_pi(&config, &workspace, &pi_args)
}

pub(super) fn launch_shell(workspace_arg: Option<&str>) -> Result<i32> {
    let state = StatePaths::default()?;
    let _ = state.ensure()?;
    repair_managed_runtime_config()?;

    let config = Config::load_or_default()?;
    config.validate_semantics()?;
    let workspace = workspace::resolve_workspace(workspace_arg, &config)?;
    print_workspace_messages(&workspace, &config);
    print_environment_image_warning(&config)?;

    docker::ensure_pi_image_exists(&config)
        .with_context(|| "Pi/environment image was not ready. Run: vr init --build")?;
    docker::run_shell(&config, &workspace)
}

fn print_environment_image_warning(config: &Config) -> Result<()> {
    if docker::environment_image_stale(config)? {
        eprintln!(
            "{}: Environment image is out of date for the current package/toolchain config. Run `vr init --build` when ready.",
            alert::warn()
        );
    }
    Ok(())
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
