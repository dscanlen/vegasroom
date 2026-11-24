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

    pub fn required_state_dir_container_path(&self, name: &str) -> &'static str {
        self.state_dir_container_path(name)
            .expect("harness descriptor should define required state dir")
    }
}

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
    container_home: "/home/agent",
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
        ("NPM_CONFIG_CACHE", "/home/agent/.cache/npm"),
        ("npm_config_cache", "/home/agent/.cache/npm"),
        ("CARGO_HOME", "/home/agent/.cargo"),
        ("RUSTUP_HOME", "/usr/local/rustup"),
        ("PIP_CACHE_DIR", "/home/agent/.cache/pip"),
        ("GOCACHE", "/home/agent/.cache/go-build"),
        ("GOMODCACHE", "/home/agent/.cache/go-mod"),
        (
            "PATH",
            "/home/agent/.npm-global/bin:/home/agent/.cargo/bin:/usr/local/cargo/bin:/usr/local/go/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        ),
        ("BROWSER", "echo"),
    ],
};

/// All registered harnesses, in preference order. The first entry is the default.
pub const HARNESSES: &[&HarnessDescriptor] = &[&PI];

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
    }
}
