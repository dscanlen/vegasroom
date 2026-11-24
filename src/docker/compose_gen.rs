use crate::{config::Config, harness::HarnessDescriptor, paths::StatePaths};

/// Generate a Docker Compose YAML string for a single harness service.
///
/// The generated file uses environment variable substitution for values that
/// can change with config (image, network) and resolves StatePaths directly
/// for volume source paths.
pub(super) fn generate(
    descriptor: &HarnessDescriptor,
    config: &Config,
    state: &StatePaths,
) -> String {
    let runtime_config = config.harness_runtime_config(descriptor);
    let service = descriptor.service_name;
    let container_home = descriptor.container_home;
    let default_image = runtime_config.image;
    let dockerfile = descriptor.dockerfile_path;

    let mut yaml = format!(
        "services:\n\
         \x20 {service}:\n\
         \x20   build:\n\
         \x20     context: .\n\
         \x20     dockerfile: {dockerfile}\n\
         \x20     network: ${{VR_PI_BUILD_NETWORK:-host}}\n\
         \x20   image: ${{VR_PI_IMAGE:-{default_image}}}\n\
         \x20   user: \"0:0\"\n\
         \x20   init: true\n\
         \x20   security_opt:\n\
         \x20     - no-new-privileges:true\n\
         \x20   cap_drop:\n\
         \x20     - ALL\n\
         \x20   network_mode: ${{VR_PI_NETWORK_MODE:-host}}\n\
         \x20   stdin_open: true\n\
         \x20   tty: true\n\
         \x20   working_dir: /workspace\n\
         \x20   environment:\n\
         \x20     HOME: {container_home}\n",
    );

    for (key, value) in descriptor.extra_env {
        yaml.push_str(&format!("      {key}: {value}\n"));
    }

    yaml.push_str("    volumes:\n");

    // Workspace mount — source is dynamic per launch, resolved via env var
    yaml.push_str(&format!(
        "      - type: bind\n\
         \x20       source: ${{VR_WORKSPACE:-{default_workspace}}}\n\
         \x20       target: /workspace\n\
         \x20       read_only: ${{VR_WORKSPACE_READ_ONLY:-false}}\n",
        default_workspace = state.workspace.display(),
    ));

    // Harness state dir mounts
    let harness_state_root = state.harness_root.join(descriptor.id);
    for dir in descriptor.state_dirs {
        let source = harness_state_root.join(dir.name);
        yaml.push_str(&format!(
            "      - type: bind\n\
             \x20       source: {source}\n\
             \x20       target: {target}\n",
            source = source.display(),
            target = dir.container_path,
        ));
    }

    // Shared environment cargo home (for Rust environment customisation)
    yaml.push_str(&format!(
        "      - type: bind\n\
         \x20       source: {cargo_home}\n\
         \x20       target: {container_home}/.cargo\n",
        cargo_home = state.cargo_home.display(),
    ));

    // SSH state dir mount
    yaml.push_str(&format!(
        "      - type: bind\n\
         \x20       source: {ssh_dir}\n\
         \x20       target: {container_home}/.ssh\n",
        ssh_dir = state.ssh_dir.display(),
    ));

    // Shared cache dir mount
    yaml.push_str(&format!(
        "      - type: bind\n\
         \x20       source: {cache}\n\
         \x20       target: {container_home}/.cache\n",
        cache = state.cache.display(),
    ));

    yaml.push_str("    extra_hosts:\n      - \"host.docker.internal:host-gateway\"\n");

    yaml
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{harness, test_util::unique_temp_dir};

    fn test_state() -> StatePaths {
        StatePaths::from_root(unique_temp_dir("compose-gen").join(".vegasroom"))
    }

    #[test]
    fn generated_yaml_contains_pi_service_definition() {
        let config = Config::default();
        let state = test_state();
        let yaml = generate(&harness::PI, &config, &state);

        assert!(yaml.contains("services:"));
        assert!(yaml.contains("  pi:"));
        assert!(yaml.contains("image: ${VR_PI_IMAGE:-vegasroom/pi:latest}"));
        assert!(yaml.contains("network_mode: ${VR_PI_NETWORK_MODE:-host}"));
        assert!(yaml.contains("dockerfile: harness/pi/Dockerfile"));
        assert!(yaml.contains("HOME: /home/agent"));
    }

    #[test]
    fn generated_yaml_includes_harness_state_dir_mounts() {
        let config = Config::default();
        let state = test_state();
        let yaml = generate(&harness::PI, &config, &state);

        // All Pi state dirs appear as bind mounts
        assert!(yaml.contains("target: /home/agent/.pi/agent"));
        assert!(yaml.contains("target: /home/agent/.pi/sessions"));
        assert!(yaml.contains("target: /home/agent/.npm-global"));
        assert!(yaml.contains("target: /home/agent/.ssh"));
        assert!(yaml.contains("target: /home/agent/.cache"));
        assert!(yaml.contains("target: /home/agent/.cargo"));
        assert!(yaml.contains("target: /workspace"));
    }

    #[test]
    fn generated_yaml_includes_pi_extra_env() {
        let config = Config::default();
        let state = test_state();
        let yaml = generate(&harness::PI, &config, &state);

        assert!(yaml.contains("PI_CODING_AGENT_SESSION_DIR: /home/agent/.pi/sessions"));
        assert!(yaml.contains("BROWSER: echo"));
        assert!(yaml.contains("CARGO_HOME: /home/agent/.cargo"));
    }

    #[test]
    fn generated_yaml_uses_state_paths_for_volume_sources() {
        let config = Config::default();
        let state = test_state();
        let yaml = generate(&harness::PI, &config, &state);

        let ssh_dir = state.ssh_dir.display().to_string();
        let cache = state.cache.display().to_string();
        assert!(yaml.contains(&format!("source: {ssh_dir}")));
        assert!(yaml.contains(&format!("source: {cache}")));
    }

    #[test]
    fn generated_yaml_uses_custom_image_as_default() {
        let mut config = Config::default();
        config.harness.pi.image = "example/pi:custom".to_owned();
        let state = test_state();
        let yaml = generate(&harness::PI, &config, &state);

        assert!(yaml.contains("image: ${VR_PI_IMAGE:-example/pi:custom}"));
        assert!(!yaml.contains("vegasroom/pi:latest"));
    }
}
