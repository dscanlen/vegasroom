#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarnessDescriptor {
    pub id: &'static str,
    pub display_name: &'static str,
    pub service_name: &'static str,
    pub default_image: &'static str,
    pub versioned_image: &'static str,
    pub default_command: &'static str,
    pub dockerfile_path: &'static str,
    pub container_home: &'static str,
    pub state_dirs: &'static [HarnessStateDir],
    pub auth_state_relative_path: &'static str,
    /// Extra environment variables to inject into the container at runtime.
    /// These are harness-specific and are written directly into the generated compose service.
    pub extra_env: &'static [(&'static str, &'static str)],
    /// Host environment variable names forwarded into the container when set on the host.
    /// Rendered as bare `KEY:` entries so Compose takes the value from the invoking shell
    /// and leaves the variable unset in the container when the host has no value.
    pub passthrough_env: &'static [&'static str],
    /// HTTPS endpoint `vr doctor` fetches from inside the room to confirm the
    /// harness can reach its provider (any HTTP response counts as reachable).
    pub connectivity_url: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarnessStateDir {
    pub name: &'static str,
    pub container_path: &'static str,
}

impl HarnessDescriptor {
    pub fn state_dir(&self, name: &str) -> Option<&HarnessStateDir> {
        self.state_dirs.iter().find(|dir| dir.name == name)
    }

    pub fn state_dir_container_path(&self, name: &str) -> Option<&'static str> {
        self.state_dir(name).map(|dir| dir.container_path)
    }

    /// Name of the Compose interpolation variable for this harness, e.g. `VR_PI_IMAGE`.
    pub fn compose_var(&self, suffix: &str) -> String {
        format!(
            "VR_{}_{suffix}",
            self.id.to_ascii_uppercase().replace('-', "_")
        )
    }

    pub fn required_state_dir_container_path(&self, name: &str) -> &'static str {
        self.state_dir_container_path(name)
            .expect("harness descriptor should define required state dir")
    }
}

/// Home directory of the room user in every harness image (provided by the base image).
pub const CONTAINER_HOME: &str = "/home/agent";

/// Toolchain and cache environment shared by every harness container, so the
/// environment image layering (rust/python/go/ts) behaves the same in each room.
pub const TOOLCHAIN_ENV: &[(&str, &str)] = &[
    ("NPM_CONFIG_CACHE", "/home/agent/.cache/npm"),
    ("npm_config_cache", "/home/agent/.cache/npm"),
    ("CARGO_HOME", "/home/agent/.cargo"),
    ("RUSTUP_HOME", "/usr/local/rustup"),
    ("PIP_CACHE_DIR", "/home/agent/.cache/pip"),
    ("GOCACHE", "/home/agent/.cache/go-build"),
    ("GOMODCACHE", "/home/agent/.cache/go-mod"),
];

pub const PI_DEFAULT_IMAGE: &str = "vegasroom/pi:latest";
pub const PI_VERSIONED_IMAGE: &str = concat!("vegasroom/pi:", env!("CARGO_PKG_VERSION"));

pub const PI_CONFIG_DIR: &str = "config";
pub const PI_EXTENSIONS_DIR: &str = "extensions";
pub const PI_SKILLS_DIR: &str = "skills";
pub const PI_SESSIONS_DIR: &str = "sessions";
pub const PI_NPM_GLOBAL_DIR: &str = "npm-global";

pub const PI: HarnessDescriptor = HarnessDescriptor {
    id: "pi",
    display_name: "Pi",
    service_name: "pi",
    default_image: PI_DEFAULT_IMAGE,
    versioned_image: PI_VERSIONED_IMAGE,
    default_command: "pi",
    dockerfile_path: "harness/pi/Dockerfile",
    container_home: CONTAINER_HOME,
    state_dirs: &[
        HarnessStateDir {
            name: PI_CONFIG_DIR,
            container_path: "/home/agent/.pi/agent",
        },
        HarnessStateDir {
            name: PI_EXTENSIONS_DIR,
            container_path: "/home/agent/.pi/extensions",
        },
        HarnessStateDir {
            name: PI_SKILLS_DIR,
            container_path: "/home/agent/.pi/skills",
        },
        HarnessStateDir {
            name: PI_SESSIONS_DIR,
            container_path: "/home/agent/.pi/sessions",
        },
        HarnessStateDir {
            name: PI_NPM_GLOBAL_DIR,
            container_path: "/home/agent/.npm-global",
        },
    ],
    auth_state_relative_path: "config/auth.json",
    extra_env: &[
        ("PI_CODING_AGENT_SESSION_DIR", "/home/agent/.pi/sessions"),
        ("NPM_CONFIG_PREFIX", "/home/agent/.npm-global"),
        ("npm_config_prefix", "/home/agent/.npm-global"),
        (
            "PATH",
            "/home/agent/.npm-global/bin:/home/agent/.cargo/bin:/usr/local/cargo/bin:/usr/local/go/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        ),
        ("BROWSER", "echo"),
    ],
    passthrough_env: &[],
    connectivity_url: "https://pi.dev",
};

pub const CLAUDE_DEFAULT_IMAGE: &str = "vegasroom/claude:latest";
pub const CLAUDE_VERSIONED_IMAGE: &str = concat!("vegasroom/claude:", env!("CARGO_PKG_VERSION"));

/// Host state dir name for Claude Code's CLAUDE_CONFIG_DIR (settings, /login
/// credentials, sessions, .claude.json all live inside it).
pub const CLAUDE_CONFIG_STATE_DIR: &str = "config";

pub const CLAUDE: HarnessDescriptor = HarnessDescriptor {
    id: "claude",
    display_name: "Claude Code",
    service_name: "claude",
    default_image: CLAUDE_DEFAULT_IMAGE,
    versioned_image: CLAUDE_VERSIONED_IMAGE,
    default_command: "claude",
    dockerfile_path: "harness/claude/Dockerfile",
    container_home: CONTAINER_HOME,
    state_dirs: &[HarnessStateDir {
        name: CLAUDE_CONFIG_STATE_DIR,
        container_path: "/home/agent/.claude",
    }],
    auth_state_relative_path: "config/.credentials.json",
    extra_env: &[
        ("CLAUDE_CONFIG_DIR", "/home/agent/.claude"),
        ("DISABLE_AUTOUPDATER", "\"1\""),
        (
            "PATH",
            "/home/agent/.cargo/bin:/usr/local/cargo/bin:/usr/local/go/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        ),
        ("BROWSER", "echo"),
    ],
    passthrough_env: &["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"],
    connectivity_url: "https://api.anthropic.com",
};

/// All registered harnesses, in preference order. The first entry is the default.
pub const HARNESSES: &[&HarnessDescriptor] = &[&PI, &CLAUDE];

/// Look up a harness descriptor by its ID. Returns `None` if no harness with that ID is registered.
pub fn find_by_id(id: &str) -> Option<&'static HarnessDescriptor> {
    HARNESSES.iter().find(|&&d| d.id == id).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_descriptor_contains_current_runtime_contract() {
        assert_eq!(PI.id, "pi");
        assert_eq!(PI.service_name, "pi");
        assert_eq!(PI.default_image, "vegasroom/pi:latest");
        assert_eq!(
            PI.versioned_image,
            concat!("vegasroom/pi:", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(PI.default_command, "pi");
        assert_eq!(PI.dockerfile_path, "harness/pi/Dockerfile");
        assert_eq!(PI.container_home, "/home/agent");
        assert_eq!(
            PI.state_dir(PI_CONFIG_DIR).map(|dir| dir.container_path),
            Some("/home/agent/.pi/agent")
        );
        assert_eq!(
            PI.state_dir(PI_SESSIONS_DIR).map(|dir| dir.container_path),
            Some("/home/agent/.pi/sessions")
        );
        assert_eq!(
            PI.required_state_dir_container_path(PI_CONFIG_DIR),
            "/home/agent/.pi/agent"
        );
        assert_eq!(
            PI.required_state_dir_container_path(PI_NPM_GLOBAL_DIR),
            "/home/agent/.npm-global"
        );
        assert_eq!(PI.auth_state_relative_path, "config/auth.json");
        assert!(PI.passthrough_env.is_empty());
    }

    #[test]
    fn claude_descriptor_contains_runtime_contract() {
        assert_eq!(CLAUDE.id, "claude");
        assert_eq!(CLAUDE.service_name, "claude");
        assert_eq!(CLAUDE.default_image, "vegasroom/claude:latest");
        assert_eq!(
            CLAUDE.versioned_image,
            concat!("vegasroom/claude:", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(CLAUDE.default_command, "claude");
        assert_eq!(CLAUDE.dockerfile_path, "harness/claude/Dockerfile");
        assert_eq!(CLAUDE.container_home, "/home/agent");
        assert_eq!(
            CLAUDE.required_state_dir_container_path(CLAUDE_CONFIG_STATE_DIR),
            "/home/agent/.claude"
        );
        assert_eq!(CLAUDE.auth_state_relative_path, "config/.credentials.json");
        assert_eq!(CLAUDE.compose_var("IMAGE"), "VR_CLAUDE_IMAGE");
    }

    #[test]
    fn claude_config_dir_env_matches_mounted_state_dir() {
        let mounted = CLAUDE.required_state_dir_container_path(CLAUDE_CONFIG_STATE_DIR);
        assert!(CLAUDE.extra_env.contains(&("CLAUDE_CONFIG_DIR", mounted)));
        assert!(CLAUDE
            .auth_state_relative_path
            .starts_with(&format!("{CLAUDE_CONFIG_STATE_DIR}/")));
    }

    #[test]
    fn claude_forwards_auth_env_from_host_without_baking_values() {
        assert_eq!(
            CLAUDE.passthrough_env,
            &["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"]
        );
        for key in CLAUDE.passthrough_env {
            assert!(!CLAUDE.extra_env.iter().any(|(k, _)| k == key));
        }
    }

    #[test]
    fn registry_keeps_pi_as_default_and_includes_claude() {
        assert_eq!(HARNESSES[0].id, PI.id);
        assert_eq!(find_by_id("claude"), Some(&CLAUDE));
        assert_eq!(find_by_id("pi"), Some(&PI));
        assert_eq!(find_by_id("nope"), None);
    }

    #[test]
    fn compose_var_uses_uppercased_harness_id() {
        assert_eq!(PI.compose_var("IMAGE"), "VR_PI_IMAGE");
        assert_eq!(PI.compose_var("NETWORK_MODE"), "VR_PI_NETWORK_MODE");
    }

    #[test]
    fn every_harness_uses_the_base_image_container_home() {
        for descriptor in HARNESSES {
            assert_eq!(
                descriptor.container_home, CONTAINER_HOME,
                "{}",
                descriptor.id
            );
        }
    }

    #[test]
    fn harness_ids_and_services_are_unique() {
        for (i, a) in HARNESSES.iter().enumerate() {
            for b in &HARNESSES[i + 1..] {
                assert_ne!(a.id, b.id);
                assert_ne!(a.service_name, b.service_name);
                assert_ne!(a.compose_var("IMAGE"), b.compose_var("IMAGE"));
            }
        }
    }
}
