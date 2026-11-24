use anyhow::{Context, Result};

use crate::{
    alert,
    config::Config,
    config_ui, docker, doctor as doctor_runner,
    harness::{self, HarnessDescriptor},
    paths::StatePaths,
    workspace,
};

pub(super) fn init(build: bool) -> Result<i32> {
    let state = StatePaths::default()?;
    let report = state.ensure()?;
    report.print();
    repair_managed_runtime_config(&state)?;

    let config = Config::load_with(&state)?;
    docker::write_runtime_compose_yaml(&config, &state)?;

    if build {
        config.validate_semantics()?;
        docker::build_base_image(&config, &state)?;
        for descriptor in harness::HARNESSES {
            println!(
                "Building {} image: {}",
                descriptor.display_name,
                config.harness_runtime_config(descriptor).image
            );
            docker::build_image(&config, descriptor)?;
        }
        print_unused_images_hint(&config);
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

pub(super) fn launch_harness(
    descriptor: &'static HarnessDescriptor,
    workspace_arg: Option<&str>,
    harness_args: Vec<String>,
) -> Result<i32> {
    let state = StatePaths::default()?;
    let _ = state.ensure()?;
    repair_managed_runtime_config(&state)?;
    state.show_disclaimer_once()?;

    let config = Config::load_with(&state)?;
    config.validate_semantics()?;
    let workspace = workspace::resolve_workspace_with_state(workspace_arg, &config, &state)?;
    print_workspace_messages(&workspace, &config, descriptor);
    print_image_freshness_warnings(&config, descriptor);

    docker::ensure_harness_image_exists(descriptor, &config).with_context(|| {
        format!(
            "{} image was not ready. Run: vr init --build",
            descriptor.display_name
        )
    })?;
    docker::run_harness(descriptor, &config, &workspace, &harness_args, &state)
}

pub(super) fn launch_shell(workspace_arg: Option<&str>, command: Vec<String>) -> Result<i32> {
    let state = StatePaths::default()?;
    let _ = state.ensure()?;
    repair_managed_runtime_config(&state)?;

    let config = Config::load_with(&state)?;
    config.validate_semantics()?;
    let workspace = workspace::resolve_workspace_with_state(workspace_arg, &config, &state)?;
    print_workspace_messages(&workspace, &config, &harness::PI);
    print_image_freshness_warnings(&config, &harness::PI);

    docker::ensure_harness_image_exists(&harness::PI, &config)
        .with_context(|| "Pi/environment image was not ready. Run: vr init --build")?;
    docker::run_shell(&config, &workspace, &command, &state)
}

/// Non-blocking launch warnings: the base no longer matches the environment profile,
/// or the harness image was built on an older base. Docker errors are ignored here;
/// `ensure_harness_image_exists` reports a missing harness image.
fn print_image_freshness_warnings(config: &Config, descriptor: &HarnessDescriptor) {
    match docker::base_status(config) {
        Ok(docker::BaseStatus::Stale) => eprintln!(
            "{}: Shared base image {} is out of date for the current environment profile. Run `vr init --build` when ready.",
            alert::warn(),
            config.environment.base_image
        ),
        Ok(docker::BaseStatus::Missing) => eprintln!(
            "{}: Shared base image {} was not found. Run `vr init --build` when ready.",
            alert::warn(),
            config.environment.base_image
        ),
        Ok(docker::BaseStatus::Current) | Err(_) => {}
    }

    if let Ok(Some(false)) = docker::harness_on_current_base(config, descriptor) {
        eprintln!(
            "{}: {} image {} was built on an older base image, so it may not have your current environment profile. Run `vr init --build` when ready.",
            alert::warn(),
            descriptor.display_name,
            config.harness_runtime_config(descriptor).image
        );
    }
}

/// After a build, point out local vegasroom images the current config no longer uses.
fn print_unused_images_hint(config: &Config) {
    let Ok(unused) = docker::unused_images(config) else {
        return;
    };
    if unused.is_empty() {
        return;
    }
    println!("\nThese local images are no longer used by your config:");
    for image in &unused {
        println!("  {image}");
    }
    println!(
        "Remove them when you no longer need them:\n  {}",
        docker::removal_command(config, &unused)
    );
}

fn print_workspace_messages(
    workspace: &workspace::ResolvedWorkspace,
    config: &Config,
    descriptor: &HarnessDescriptor,
) {
    if workspace.created {
        println!("Created workspace: {}", workspace.path.display());
    }

    if config.harness.instance(descriptor).read_only_workspace {
        println!(
            "Workspace will be mounted read-only: {}",
            workspace.path.display()
        );
    }

    for warning in &workspace.warnings {
        eprintln!("{}: {warning}", alert::warn());
    }
}

fn repair_managed_runtime_config(state: &StatePaths) -> Result<()> {
    let mut config = Config::load_with(state)?;
    if config.uses_managed_compose_file_with(state) {
        return Ok(());
    }

    config.set_managed_compose_file_with(state)?;
    config.save_to_default_path_with(state)?;
    println!(
        "Configured managed Compose runtime: {}",
        config.docker.compose_file
    );
    Ok(())
}
