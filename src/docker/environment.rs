use std::{collections::BTreeSet, fs, path::PathBuf, process::Stdio};

use anyhow::{bail, Context, Result};

use crate::{
    config::{
        normalized_apt_packages, normalized_rust_components, normalized_rust_toolchain,
        normalized_typescript_packages, Config,
    },
    harness,
    paths::{display_path, StatePaths},
};

use super::{base_docker, harness_image_exists};

pub(super) fn packages(config: &Config) -> Vec<String> {
    normalized_apt_packages(&config.environment)
}

pub(super) fn rust_enabled(config: &Config) -> bool {
    config.environment.rust.enabled
}

pub(super) fn rust_toolchain(config: &Config) -> String {
    normalized_rust_toolchain(&config.environment)
}

pub(super) fn rust_components(config: &Config) -> Vec<String> {
    normalized_rust_components(&config.environment)
}

pub(super) fn python_enabled(config: &Config) -> bool {
    config.environment.python.enabled
}

pub(super) fn go_enabled(config: &Config) -> bool {
    config.environment.go.enabled
}

pub(super) fn typescript_enabled(config: &Config) -> bool {
    config.environment.typescript.enabled
}

pub(super) fn typescript_packages(config: &Config) -> Vec<String> {
    normalized_typescript_packages(&config.environment)
}

pub(super) fn has_customization(config: &Config) -> bool {
    !packages(config).is_empty()
        || rust_enabled(config)
        || python_enabled(config)
        || go_enabled(config)
        || typescript_enabled(config)
}

pub(super) fn runtime_image(config: &Config, descriptor: &harness::HarnessDescriptor) -> String {
    let base = base_image(config, descriptor);
    if has_customization(config) {
        derived_image_tag(base)
    } else {
        base.to_owned()
    }
}

pub(super) fn ensure_image(config: &Config, descriptor: &harness::HarnessDescriptor) -> Result<()> {
    if !has_customization(config) {
        return Ok(());
    }

    validate_environment(config)?;

    let image = runtime_image(config, descriptor);
    if image_tag_exists(config, &image)? {
        Ok(())
    } else {
        bail!("environment image not found: {image}\nRun: vr init --build")
    }
}

pub(super) fn build_image(config: &Config, descriptor: &harness::HarnessDescriptor) -> Result<()> {
    if !has_customization(config) {
        return Ok(());
    }

    validate_environment(config)?;

    if !harness_image_exists(config, descriptor)? {
        bail!(
            "base {} image was not found: {}\nRun: vr init --build",
            descriptor.display_name,
            base_image(config, descriptor)
        );
    }

    let state = StatePaths::default()?;
    write_dockerfile(config, descriptor, &state)?;
    let image = runtime_image(config, descriptor);
    run_build_image(config, descriptor, &state, &image)
}

pub(super) fn image_stale(
    config: &Config,
    descriptor: &harness::HarnessDescriptor,
) -> Result<bool> {
    if !has_customization(config) {
        return Ok(false);
    }

    validate_environment(config)?;

    let state = StatePaths::default()?;
    let dockerfile_path = dockerfile_path(&state, descriptor);
    let next_contents = dockerfile_contents(config, descriptor);
    Ok(fs::read_to_string(&dockerfile_path)
        .map(|current| current != next_contents)
        .unwrap_or(true))
}

pub(super) fn image_exists(
    config: &Config,
    descriptor: &harness::HarnessDescriptor,
) -> Result<bool> {
    let image = runtime_image(config, descriptor);
    image_tag_exists(config, &image)
}

fn validate_environment(config: &Config) -> Result<()> {
    config.validate_semantics()
}

pub(super) fn dockerfile_path(
    state: &StatePaths,
    descriptor: &harness::HarnessDescriptor,
) -> PathBuf {
    state
        .runtime_root
        .join("environment")
        .join(descriptor.id)
        .join("Dockerfile")
}

fn write_dockerfile(
    config: &Config,
    descriptor: &harness::HarnessDescriptor,
    state: &StatePaths,
) -> Result<PathBuf> {
    let dockerfile_path = dockerfile_path(state, descriptor);
    if let Some(parent) = dockerfile_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create environment runtime directory: {}",
                display_path(parent)
            )
        })?;
    }

    fs::write(&dockerfile_path, dockerfile_contents(config, descriptor)).with_context(|| {
        format!(
            "failed to write environment Dockerfile: {}",
            display_path(&dockerfile_path)
        )
    })?;

    Ok(dockerfile_path)
}

fn run_build_image(
    config: &Config,
    descriptor: &harness::HarnessDescriptor,
    state: &StatePaths,
    image: &str,
) -> Result<()> {
    let dockerfile = dockerfile_path(state, descriptor);
    let context = dockerfile
        .parent()
        .context("environment Dockerfile has no parent directory")?;

    println!(
        "Building {} environment image: {image}",
        descriptor.display_name
    );
    let apt_packages = packages(config);
    if !apt_packages.is_empty() {
        println!("Installing apt packages: {}", apt_packages.join(", "));
    }
    if python_enabled(config) {
        println!("Installing Python toolchain");
    }
    if go_enabled(config) {
        println!("Installing Go toolchain");
    }
    if typescript_enabled(config) {
        println!(
            "Installing TypeScript npm packages: {}",
            typescript_packages(config).join(", ")
        );
    }
    if rust_enabled(config) {
        let components = rust_components(config);
        println!("Installing Rust toolchain: {}", rust_toolchain(config));
        if !components.is_empty() {
            println!("Installing Rust components: {}", components.join(", "));
        }
    }

    let status = base_docker(config)
        .arg("build")
        .arg("--network")
        .arg(&config.harness.pi.build_network)
        .arg("-f")
        .arg(&dockerfile)
        .arg("-t")
        .arg(image)
        .arg(context)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to start Docker environment image build command")?;

    if status.success() {
        Ok(())
    } else {
        bail!("Docker environment image build failed with status: {status}")
    }
}

fn image_tag_exists(config: &Config, image: &str) -> Result<bool> {
    let status = base_docker(config)
        .args(["image", "inspect", image])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to inspect Docker image")?;

    Ok(status.success())
}

fn dockerfile_contents(config: &Config, descriptor: &harness::HarnessDescriptor) -> String {
    let mut contents = format!(
        r#"FROM {base_image}

ENV DEBIAN_FRONTEND=noninteractive
"#,
        base_image = base_image(config, descriptor),
    );

    let apt_packages = build_apt_packages(config);
    if !apt_packages.is_empty() {
        let package_lines = apt_packages
            .into_iter()
            .map(|package| format!("      {package} \\"))
            .collect::<Vec<_>>()
            .join("\n");

        contents.push_str(&format!(
            r#"
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
{package_lines}
    && rm -rf /var/lib/apt/lists/*
"#,
        ));
    }

    if python_enabled(config) {
        contents.push_str(&format!(
            r#"
ENV PIP_CACHE_DIR={container_home}/.cache/pip
"#,
            container_home = descriptor.container_home,
        ));
    }

    if go_enabled(config) {
        contents.push_str(&format!(
            r#"
ENV GOCACHE={container_home}/.cache/go-build \
    GOMODCACHE={container_home}/.cache/go-mod \
    PATH=/usr/local/go/bin:${{PATH}}
"#,
            container_home = descriptor.container_home,
        ));
    }

    if typescript_enabled(config) {
        let npm_packages = typescript_packages(config).join(" ");
        contents.push_str(&format!(
            r#"
RUN NPM_CONFIG_PREFIX=/usr/local npm install -g --ignore-scripts {npm_packages}
"#,
        ));
    }

    if rust_enabled(config) {
        contents.push_str(&format!(
            r#"
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME={container_home}/.cargo \
    PATH={container_home}/.cargo/bin:/usr/local/cargo/bin:${{PATH}}

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | CARGO_HOME=/usr/local/cargo sh -s -- -y --no-modify-path --profile minimal --default-toolchain {toolchain}
"#,
            container_home = descriptor.container_home,
            toolchain = rust_toolchain(config),
        ));

        let components = rust_components(config);
        if !components.is_empty() {
            let component_lines = components
                .into_iter()
                .map(|component| format!("      {component}"))
                .collect::<Vec<_>>()
                .join(" \\\n");
            contents.push_str(&format!(
                r#"
RUN CARGO_HOME=/usr/local/cargo rustup component add \
{component_lines}
"#,
            ));
        }
    }

    contents
}

fn build_apt_packages(config: &Config) -> Vec<String> {
    let mut packages = packages(config);
    if python_enabled(config) {
        packages.extend(
            [
                "python3",
                "python3-pip",
                "python3-venv",
                "python-is-python3",
            ]
            .into_iter()
            .map(str::to_owned),
        );
    }
    if go_enabled(config) {
        packages.extend(["golang"].into_iter().map(str::to_owned));
    }
    packages
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn base_image<'a>(config: &'a Config, descriptor: &'a harness::HarnessDescriptor) -> &'a str {
    if descriptor.id == harness::PI.id {
        config.harness.pi.image.as_str()
    } else {
        descriptor.default_image
    }
}

fn derived_image_tag(base: &str) -> String {
    if let Some((image, _digest)) = base.split_once('@') {
        return format!("{}:env", image_repository_without_tag(image));
    }

    if has_tag(base) {
        format!("{base}-env")
    } else {
        format!("{base}:env")
    }
}

fn image_repository_without_tag(image: &str) -> &str {
    if let Some(colon) = tag_colon(image) {
        &image[..colon]
    } else {
        image
    }
}

fn has_tag(image: &str) -> bool {
    tag_colon(image).is_some()
}

fn tag_colon(image: &str) -> Option<usize> {
    let last_slash = image.rfind('/');
    let last_colon = image.rfind(':');
    last_colon.filter(|colon| last_slash.map(|slash| *colon > slash).unwrap_or(true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_image_tag_appends_env_to_existing_tag() {
        assert_eq!(
            derived_image_tag("vegasroom/pi:latest"),
            "vegasroom/pi:latest-env"
        );
        assert_eq!(derived_image_tag("vegasroom/pi"), "vegasroom/pi:env");
    }

    #[test]
    fn derived_image_tag_handles_digest_base_images() {
        assert_eq!(
            derived_image_tag("vegasroom/pi@sha256:abc123"),
            "vegasroom/pi:env"
        );
        assert_eq!(
            derived_image_tag("vegasroom/pi:latest@sha256:abc123"),
            "vegasroom/pi:env"
        );
        assert_eq!(
            derived_image_tag("localhost:5000/vegasroom/pi@sha256:abc123"),
            "localhost:5000/vegasroom/pi:env"
        );
    }
}
