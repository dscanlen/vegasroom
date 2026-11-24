use std::{collections::BTreeSet, fs};

use crate::{
    assets,
    config::{
        normalized_apt_packages, normalized_rust_components, normalized_rust_toolchain,
        normalized_typescript_packages, Config,
    },
    harness,
    paths::StatePaths,
};

pub(crate) fn packages(config: &Config) -> Vec<String> {
    normalized_apt_packages(&config.environment)
}

pub(crate) fn rust_enabled(config: &Config) -> bool {
    config.environment.rust.enabled
}

pub(crate) fn rust_toolchain(config: &Config) -> String {
    normalized_rust_toolchain(&config.environment)
}

pub(crate) fn rust_components(config: &Config) -> Vec<String> {
    normalized_rust_components(&config.environment)
}

pub(crate) fn python_enabled(config: &Config) -> bool {
    config.environment.python.enabled
}

pub(crate) fn go_enabled(config: &Config) -> bool {
    config.environment.go.enabled
}

pub(crate) fn typescript_enabled(config: &Config) -> bool {
    config.environment.typescript.enabled
}

pub(crate) fn typescript_packages(config: &Config) -> Vec<String> {
    normalized_typescript_packages(&config.environment)
}

/// Full Dockerfile for the shared base image: the embedded foundation followed by
/// the configured `environment:` profile. Harness images build `FROM` this.
pub(crate) fn base_dockerfile_contents(config: &Config) -> String {
    let mut contents = assets::MANAGED_BASE_DOCKERFILE.to_owned();
    let sections = environment_sections(config, harness::CONTAINER_HOME);
    if !sections.is_empty() {
        contents.push_str(
            "\n# --- environment profile (generated from config.yaml `environment:`) ---\n",
        );
        contents.push_str(&sections);
    }
    contents
}

/// Human-readable lines describing what the environment profile installs.
pub(crate) fn profile_summary(config: &Config) -> Vec<String> {
    let mut lines = Vec::new();
    let apt_packages = packages(config);
    if !apt_packages.is_empty() {
        lines.push(format!("apt packages: {}", apt_packages.join(", ")));
    }
    if python_enabled(config) {
        lines.push("Python toolchain".to_owned());
    }
    if go_enabled(config) {
        lines.push("Go toolchain".to_owned());
    }
    if typescript_enabled(config) {
        lines.push(format!(
            "TypeScript npm packages: {}",
            typescript_packages(config).join(", ")
        ));
    }
    if rust_enabled(config) {
        let components = rust_components(config);
        let mut line = format!("Rust toolchain: {}", rust_toolchain(config));
        if !components.is_empty() {
            line.push_str(&format!(" (components: {})", components.join(", ")));
        }
        lines.push(line);
    }
    lines
}

/// True when the base Dockerfile recorded at the last base build differs from what
/// the current config would generate (or no base has been built yet).
pub(crate) fn base_dockerfile_stale(config: &Config, state: &StatePaths) -> bool {
    fs::read_to_string(state.runtime_base_dockerfile())
        .map(|built| built != base_dockerfile_contents(config))
        .unwrap_or(true)
}

/// Dockerfile instructions for the configured environment profile. Empty when nothing is enabled.
fn environment_sections(config: &Config, container_home: &str) -> String {
    let mut contents = String::new();

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
            container_home = container_home,
        ));
    }

    if go_enabled(config) {
        contents.push_str(&format!(
            r#"
ENV GOCACHE={container_home}/.cache/go-build \
    GOMODCACHE={container_home}/.cache/go-mod \
    PATH=/usr/local/go/bin:${{PATH}}
"#,
            container_home = container_home,
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
            container_home = container_home,
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

// TEMPORARY(M3.3): consumed by the M3.4 leftover-image hint; remove this allow then.
#[allow(dead_code)]
/// Tag of the per-harness `-env` image that vegasroom built before the shared base
/// image existed (e.g. `vegasroom/pi:local-env`). Only used to point out leftovers.
pub(crate) fn legacy_env_image_tag(base: &str) -> String {
    if let Some((image, _digest)) = base.split_once('@') {
        return format!("{}:env", image_repository_without_tag(image));
    }

    if has_tag(base) {
        format!("{base}-env")
    } else {
        format!("{base}:env")
    }
}

#[allow(dead_code)] // TEMPORARY(M3.3), see legacy_env_image_tag
fn image_repository_without_tag(image: &str) -> &str {
    if let Some(colon) = tag_colon(image) {
        &image[..colon]
    } else {
        image
    }
}

#[allow(dead_code)] // TEMPORARY(M3.3), see legacy_env_image_tag
fn has_tag(image: &str) -> bool {
    tag_colon(image).is_some()
}

#[allow(dead_code)] // TEMPORARY(M3.3), see legacy_env_image_tag
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
            legacy_env_image_tag("vegasroom/pi:latest"),
            "vegasroom/pi:latest-env"
        );
        assert_eq!(legacy_env_image_tag("vegasroom/pi"), "vegasroom/pi:env");
    }

    #[test]
    fn derived_image_tag_handles_digest_base_images() {
        assert_eq!(
            legacy_env_image_tag("vegasroom/pi@sha256:abc123"),
            "vegasroom/pi:env"
        );
        assert_eq!(
            legacy_env_image_tag("vegasroom/pi:latest@sha256:abc123"),
            "vegasroom/pi:env"
        );
        assert_eq!(
            legacy_env_image_tag("localhost:5000/vegasroom/pi@sha256:abc123"),
            "localhost:5000/vegasroom/pi:env"
        );
    }

    fn base_config() -> Config {
        let mut config = Config::default();
        config.environment.apt.packages = vec!["jq".to_owned()];
        config.environment.python.enabled = true;
        config.environment.typescript.enabled = true;
        config.environment.rust.enabled = true;
        config
    }

    #[test]
    fn base_dockerfile_without_profile_is_just_the_foundation() {
        let contents = base_dockerfile_contents(&Config::default());
        assert_eq!(contents, assets::MANAGED_BASE_DOCKERFILE);
        assert!(contents.contains("FROM node:24-bookworm-slim"));
        assert!(contents.contains("ln -s /home/agent/.ssh /root/.ssh"));
        assert!(contents.contains("WORKDIR /workspace"));
    }

    #[test]
    fn base_dockerfile_appends_profile_after_foundation() {
        let contents = base_dockerfile_contents(&base_config());
        let foundation_end = contents.find("WORKDIR /workspace").unwrap();
        let profile = contents.find("--- environment profile").unwrap();

        assert!(contents.starts_with(assets::MANAGED_BASE_DOCKERFILE));
        assert!(foundation_end < profile);
        for expected in [
            "      jq \\",
            "      python3 \\",
            "PIP_CACHE_DIR=/home/agent/.cache/pip",
            "npm install -g --ignore-scripts ts-node typescript",
            "sh.rustup.rs",
            "CARGO_HOME=/home/agent/.cargo",
        ] {
            let at = contents
                .find(expected)
                .unwrap_or_else(|| panic!("missing {expected}"));
            assert!(at > profile, "{expected} should be in the profile section");
        }
        let from_lines = contents
            .lines()
            .filter(|line| line.starts_with("FROM "))
            .count();
        assert_eq!(from_lines, 1);
    }

    #[test]
    fn base_dockerfile_staleness_tracks_last_built_contents() {
        let state = StatePaths::from_root(
            crate::test_util::unique_temp_dir("base-stale").join(".vegasroom"),
        );
        let config = base_config();
        assert!(base_dockerfile_stale(&config, &state), "never built");

        fs::create_dir_all(state.runtime_base_dir()).unwrap();
        fs::write(
            state.runtime_base_dockerfile(),
            base_dockerfile_contents(&config),
        )
        .unwrap();
        assert!(!base_dockerfile_stale(&config, &state));

        let mut changed = config.clone();
        changed.environment.go.enabled = true;
        assert!(base_dockerfile_stale(&changed, &state));

        let _ = fs::remove_dir_all(state.root);
    }

    #[test]
    fn profile_summary_lists_enabled_parts_only() {
        assert!(profile_summary(&Config::default()).is_empty());
        let summary = profile_summary(&base_config());
        assert!(summary.contains(&"apt packages: jq".to_owned()));
        assert!(summary.contains(&"Python toolchain".to_owned()));
        assert!(summary
            .iter()
            .any(|line| line.starts_with("Rust toolchain: stable")));
        assert!(!summary.contains(&"Go toolchain".to_owned()));
    }

    #[test]
    fn base_dockerfile_contains_nothing_harness_specific() {
        let contents = base_dockerfile_contents(&base_config());
        for forbidden in [
            "pi-coding-agent",
            "claude-code",
            ".pi/",
            "CLAUDE_CONFIG_DIR",
            "npm-global",
            "CMD",
        ] {
            assert!(
                !contents.contains(forbidden),
                "base must not contain {forbidden}"
            );
        }
    }
}
