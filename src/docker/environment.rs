use std::{collections::BTreeSet, fs, path::PathBuf, process::Stdio};

use anyhow::{bail, Context, Result};

use crate::{
    config::Config,
    harness,
    paths::{display_path, StatePaths},
};

use super::{base_docker, harness_image_exists};

pub(super) fn packages(config: &Config) -> Vec<String> {
    config
        .environment
        .apt
        .packages
        .iter()
        .map(|package| package.trim())
        .filter(|package| !package.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn rust_enabled(config: &Config) -> bool {
    config.environment.rust.enabled
}

pub(super) fn rust_toolchain(config: &Config) -> String {
    let toolchain = config.environment.rust.toolchain.trim();
    if toolchain.is_empty() {
        "stable".to_owned()
    } else {
        toolchain.to_owned()
    }
}

pub(super) fn rust_components(config: &Config) -> Vec<String> {
    config
        .environment
        .rust
        .components
        .iter()
        .map(|component| component.trim())
        .filter(|component| !component.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
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
    config
        .environment
        .typescript
        .packages
        .iter()
        .map(|package| package.trim())
        .filter(|package| !package.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
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
    ensure_image_inner(config, descriptor, false)
}

pub(super) fn build_image(config: &Config, descriptor: &harness::HarnessDescriptor) -> Result<()> {
    ensure_image_inner(config, descriptor, true)
}

fn ensure_image_inner(
    config: &Config,
    descriptor: &harness::HarnessDescriptor,
    force: bool,
) -> Result<()> {
    if !has_customization(config) {
        return Ok(());
    }

    validate_environment(config)?;

    let state = StatePaths::default()?;
    let dockerfile_path = dockerfile_path(&state, descriptor);
    let next_contents = dockerfile_contents(config, descriptor);
    let dockerfile_changed = fs::read_to_string(&dockerfile_path)
        .map(|current| current != next_contents)
        .unwrap_or(true);

    if let Some(parent) = dockerfile_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create environment runtime directory: {}",
                display_path(parent)
            )
        })?;
    }

    fs::write(&dockerfile_path, next_contents).with_context(|| {
        format!(
            "failed to write environment Dockerfile: {}",
            display_path(&dockerfile_path)
        )
    })?;

    let image = runtime_image(config, descriptor);
    if !force && !dockerfile_changed && image_tag_exists(config, &image)? {
        return Ok(());
    }

    if !harness_image_exists(config, descriptor)? {
        bail!(
            "base {} image was not found: {}\nRun: vr init --build",
            descriptor.display_name,
            base_image(config, descriptor)
        );
    }

    run_build_image(config, descriptor, &state, &image)
}

pub(super) fn image_exists(
    config: &Config,
    descriptor: &harness::HarnessDescriptor,
) -> Result<bool> {
    let image = runtime_image(config, descriptor);
    image_tag_exists(config, &image)
}

fn validate_environment(config: &Config) -> Result<()> {
    validate_packages(config)?;
    validate_rust(config)?;
    validate_typescript(config)
}

pub(super) fn validate_packages(config: &Config) -> Result<()> {
    for package in packages(config) {
        if !is_safe_apt_package_name(&package) {
            bail!(
                "invalid apt package name in environment.apt.packages: {package}\nPackage names may contain only ASCII letters, digits, '.', '+', '-', ':', and '_'"
            );
        }
    }
    Ok(())
}

fn validate_rust(config: &Config) -> Result<()> {
    if !rust_enabled(config) {
        return Ok(());
    }

    let toolchain = rust_toolchain(config);
    if !is_safe_rust_toolchain(&toolchain) {
        bail!(
            "invalid Rust toolchain in environment.rust.toolchain: {toolchain}\nToolchains may contain only ASCII letters, digits, '.', '-', and '_'"
        );
    }

    for component in rust_components(config) {
        if !is_safe_rust_component(&component) {
            bail!(
                "invalid Rust component in environment.rust.components: {component}\nComponents may contain only ASCII letters, digits, '-', and '_'"
            );
        }
    }

    Ok(())
}

fn validate_typescript(config: &Config) -> Result<()> {
    if !typescript_enabled(config) {
        return Ok(());
    }

    let packages = typescript_packages(config);
    if packages.is_empty() {
        bail!("environment.typescript.enabled is true but no npm packages are configured");
    }

    for package in packages {
        if !is_safe_npm_package_name(&package) {
            bail!(
                "invalid npm package in environment.typescript.packages: {package}\nPackage names may contain only ASCII letters, digits, '.', '+', '-', '_', '/', and one leading '@' for scoped packages"
            );
        }
    }

    Ok(())
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
    if base.contains('@') {
        return format!("{}:env", harness::PI.default_image);
    }

    let last_slash = base.rfind('/');
    let last_colon = base.rfind(':');
    if last_colon.is_some_and(|colon| last_slash.map(|slash| colon > slash).unwrap_or(true)) {
        format!("{base}-env")
    } else {
        format!("{base}:env")
    }
}

fn is_safe_apt_package_name(package: &str) -> bool {
    !package.is_empty()
        && package.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-' | b':' | b'_')
        })
}

fn is_safe_rust_toolchain(toolchain: &str) -> bool {
    !toolchain.is_empty()
        && toolchain
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn is_safe_rust_component(component: &str) -> bool {
    !component.is_empty()
        && component
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn is_safe_npm_package_name(package: &str) -> bool {
    if package.is_empty() || package.contains("//") {
        return false;
    }

    let at_count = package.bytes().filter(|byte| *byte == b'@').count();
    if at_count > 1 || at_count == 1 && !package.starts_with('@') {
        return false;
    }

    package.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(byte, b'.' | b'+' | b'-' | b'_' | b'/')
            || byte == b'@'
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_image_tag_appends_env_to_existing_tag() {
        assert_eq!(
            derived_image_tag("vegasroom/pi:local"),
            "vegasroom/pi:local-env"
        );
        assert_eq!(derived_image_tag("vegasroom/pi"), "vegasroom/pi:env");
    }

    #[test]
    fn package_names_are_validated() {
        assert!(is_safe_apt_package_name("build-essential"));
        assert!(is_safe_apt_package_name("libssl-dev"));
        assert!(!is_safe_apt_package_name("bad package"));
        assert!(!is_safe_apt_package_name("bad;package"));
    }

    #[test]
    fn rust_toolchains_and_components_are_validated() {
        assert!(is_safe_rust_toolchain("stable"));
        assert!(is_safe_rust_toolchain("nightly-2026-01-01"));
        assert!(!is_safe_rust_toolchain("bad toolchain"));
        assert!(is_safe_rust_component("rustfmt"));
        assert!(is_safe_rust_component("clippy"));
        assert!(!is_safe_rust_component("bad;component"));
    }

    #[test]
    fn npm_package_names_are_validated() {
        assert!(is_safe_npm_package_name("typescript"));
        assert!(is_safe_npm_package_name("ts-node"));
        assert!(is_safe_npm_package_name("@scope/package"));
        assert!(!is_safe_npm_package_name("bad package"));
        assert!(!is_safe_npm_package_name("bad;package"));
        assert!(!is_safe_npm_package_name("scope/@bad"));
    }
}
