use crate::{
    config::Config,
    harness::{HarnessDescriptor, TOOLCHAIN_ENV},
    paths::StatePaths,
};

/// Generate a Docker Compose YAML string with one service per harness.
///
/// The generated file uses per-harness environment variable substitution
/// (`VR_<ID>_IMAGE`, ...) for values that can change with config, and resolves
/// StatePaths directly for volume source paths.
pub(super) fn generate(
    harnesses: &[&HarnessDescriptor],
    config: &Config,
    state: &StatePaths,
) -> String {
    let mut yaml = String::from("services:\n");
    for descriptor in harnesses {
        let image = config.harness_runtime_config(descriptor).image;
        yaml.push_str(&service_yaml(descriptor, image, state));
    }
    yaml
}

/// Render a single harness service block (indented under `services:`).
fn service_yaml(descriptor: &HarnessDescriptor, default_image: &str, state: &StatePaths) -> String {
    let service = descriptor.service_name;
    let container_home = descriptor.container_home;
    let dockerfile = descriptor.dockerfile_path;
    let image_var = descriptor.compose_var("IMAGE");
    let build_network_var = descriptor.compose_var("BUILD_NETWORK");
    let network_mode_var = descriptor.compose_var("NETWORK_MODE");

    let mut yaml = format!(
        "\x20 {service}:\n\
         \x20   build:\n\
         \x20     context: .\n\
         \x20     dockerfile: {dockerfile}\n\
         \x20     network: ${{{build_network_var}:-host}}\n\
         \x20   image: ${{{image_var}:-{default_image}}}\n\
         \x20   user: \"0:0\"\n\
         \x20   init: true\n\
         \x20   security_opt:\n\
         \x20     - no-new-privileges:true\n\
         \x20   cap_drop:\n\
         \x20     - ALL\n\
         \x20   network_mode: ${{{network_mode_var}:-host}}\n\
         \x20   stdin_open: true\n\
         \x20   tty: true\n\
         \x20   working_dir: /workspace\n\
         \x20   environment:\n\
         \x20     HOME: {container_home}\n",
    );

    for (key, value) in TOOLCHAIN_ENV.iter().chain(descriptor.extra_env) {
        yaml.push_str(&format!("      {key}: {value}\n"));
    }

    for key in descriptor.passthrough_env {
        yaml.push_str(&format!("      {key}:\n"));
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
    for dir in descriptor.state_dirs {
        let source = state.harness_state_dir(descriptor, dir.name);
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
        let yaml = generate(harness::HARNESSES, &config, &state);

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
        let yaml = generate(harness::HARNESSES, &config, &state);

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
        let yaml = generate(harness::HARNESSES, &config, &state);

        assert!(yaml.contains("PI_CODING_AGENT_SESSION_DIR: /home/agent/.pi/sessions"));
        assert!(yaml.contains("BROWSER: echo"));
        assert!(yaml.contains("CARGO_HOME: /home/agent/.cargo"));
    }

    #[test]
    fn generated_yaml_uses_state_paths_for_volume_sources() {
        let config = Config::default();
        let state = test_state();
        let yaml = generate(harness::HARNESSES, &config, &state);

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
        let yaml = generate(harness::HARNESSES, &config, &state);

        assert!(yaml.contains("image: ${VR_PI_IMAGE:-example/pi:custom}"));
        assert!(!yaml.contains("vegasroom/pi:latest"));
    }

    const ALT: HarnessDescriptor = HarnessDescriptor {
        id: "alt-agent",
        display_name: "Alt",
        service_name: "alt",
        default_image: "vegasroom/alt:latest",
        versioned_image: "vegasroom/alt:0",
        default_command: "alt",
        dockerfile_path: "harness/alt/Dockerfile",
        container_home: "/home/agent",
        state_dirs: &[harness::HarnessStateDir {
            name: "config",
            container_path: "/home/agent/.alt",
        }],
        auth_state_relative_path: "config/creds.json",
        extra_env: &[("ALT_CONFIG_DIR", "/home/agent/.alt")],
        passthrough_env: &["ALT_API_KEY", "ALT_TOKEN"],
    };

    #[test]
    fn generated_yaml_has_one_service_per_registered_harness() {
        let config = Config::default();
        let state = test_state();
        let yaml = generate(harness::HARNESSES, &config, &state);

        assert_eq!(yaml.matches("services:").count(), 1);
        for descriptor in harness::HARNESSES {
            assert!(yaml.contains(&format!("\n  {}:\n", descriptor.service_name)));
            assert!(yaml.contains(&format!("${{{}:-", descriptor.compose_var("IMAGE"))));
        }
    }

    #[test]
    fn service_yaml_uses_per_harness_compose_vars() {
        let state = test_state();
        let yaml = service_yaml(&ALT, ALT.default_image, &state);

        assert!(yaml.contains("  alt:\n"));
        assert!(yaml.contains("image: ${VR_ALT_AGENT_IMAGE:-vegasroom/alt:latest}"));
        assert!(yaml.contains("network: ${VR_ALT_AGENT_BUILD_NETWORK:-host}"));
        assert!(yaml.contains("network_mode: ${VR_ALT_AGENT_NETWORK_MODE:-host}"));
        assert!(!yaml.contains("VR_PI_"));
        assert!(yaml.contains("target: /home/agent/.alt"));
        assert!(yaml.contains(&format!(
            "source: {}",
            state
                .harness_root
                .join("alt-agent")
                .join("config")
                .display()
        )));
    }

    #[test]
    fn service_yaml_renders_shared_toolchain_env_then_extra_then_passthrough() {
        let state = test_state();
        let yaml = service_yaml(&ALT, ALT.default_image, &state);

        let cargo = yaml.find("CARGO_HOME: /home/agent/.cargo").unwrap();
        let extra = yaml.find("ALT_CONFIG_DIR: /home/agent/.alt").unwrap();
        let passthrough = yaml.find("      ALT_API_KEY:\n").unwrap();
        assert!(cargo < extra && extra < passthrough);
        assert!(yaml.contains("      ALT_TOKEN:\n"));
    }

    #[test]
    fn generated_yaml_parses_as_compose_document() {
        let config = Config::default();
        let state = test_state();
        let yaml = generate(harness::HARNESSES, &config, &state);
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();

        let services = doc.get("services").and_then(|s| s.as_mapping()).unwrap();
        assert_eq!(services.len(), harness::HARNESSES.len());
    }

    #[test]
    fn passthrough_env_parses_as_null_values() {
        let state = test_state();
        let yaml = format!(
            "services:\n{}",
            service_yaml(&ALT, ALT.default_image, &state)
        );
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();

        let env = &doc["services"]["alt"]["environment"];
        assert!(env["ALT_API_KEY"].is_null());
        assert_eq!(env["ALT_CONFIG_DIR"].as_str(), Some("/home/agent/.alt"));
    }

    #[test]
    fn claude_service_yaml_forwards_auth_env_and_mounts_config_dir() {
        let state = test_state();
        let yaml = format!(
            "services:\n{}",
            service_yaml(&harness::CLAUDE, harness::CLAUDE.default_image, &state)
        );
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        let service = &doc["services"]["claude"];
        let env = &service["environment"];

        assert_eq!(
            service["image"].as_str(),
            Some("${VR_CLAUDE_IMAGE:-vegasroom/claude:latest}")
        );
        assert_eq!(
            env["CLAUDE_CONFIG_DIR"].as_str(),
            Some("/home/agent/.claude")
        );
        assert_eq!(env["DISABLE_AUTOUPDATER"].as_str(), Some("1"));
        assert!(env["ANTHROPIC_API_KEY"].is_null());
        assert!(env["CLAUDE_CODE_OAUTH_TOKEN"].is_null());
        assert!(env.get("PI_CODING_AGENT_SESSION_DIR").is_none());
        assert!(yaml.contains(&format!(
            "source: {}",
            state.harness_root.join("claude").join("config").display()
        )));
        assert!(yaml.contains("target: /home/agent/.claude\n"));
    }
}
