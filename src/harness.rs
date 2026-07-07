#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarnessDescriptor {
    pub id: &'static str,
    pub display_name: &'static str,
    pub service_name: &'static str,
    pub default_image: &'static str,
    pub default_command: &'static str,
    pub dockerfile_path: &'static str,
    pub container_home: &'static str,
    pub state_dirs: &'static [HarnessStateDir],
    pub auth_state_relative_path: &'static str,
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

pub const PI_CONFIG_DIR: &str = "config";
pub const PI_EXTENSIONS_DIR: &str = "extensions";
pub const PI_SKILLS_DIR: &str = "skills";
pub const PI_SESSIONS_DIR: &str = "sessions";

pub const PI: HarnessDescriptor = HarnessDescriptor {
    id: "pi",
    display_name: "Pi",
    service_name: "pi",
    default_image: "vegasroom/pi:local",
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
    ],
    auth_state_relative_path: "config/auth.json",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_descriptor_contains_current_runtime_contract() {
        assert_eq!(PI.id, "pi");
        assert_eq!(PI.service_name, "pi");
        assert_eq!(PI.default_image, "vegasroom/pi:local");
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
        assert_eq!(PI.auth_state_relative_path, "config/auth.json");
    }
}
