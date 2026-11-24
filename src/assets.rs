use crate::harness::{self, HarnessDescriptor};

pub const MANAGED_PI_DOCKERFILE: &str = include_str!("../harness/pi/Dockerfile");
pub const MANAGED_CLAUDE_DOCKERFILE: &str = include_str!("../harness/claude/Dockerfile");

/// Embedded Dockerfile for a registered harness, written to the runtime dir at `vr init`.
pub fn managed_dockerfile(descriptor: &HarnessDescriptor) -> Option<&'static str> {
    match descriptor.id {
        id if id == harness::PI.id => Some(MANAGED_PI_DOCKERFILE),
        id if id == harness::CLAUDE.id => Some(MANAGED_CLAUDE_DOCKERFILE),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_harness_has_a_managed_dockerfile() {
        for descriptor in harness::HARNESSES {
            let dockerfile = managed_dockerfile(descriptor)
                .unwrap_or_else(|| panic!("no embedded Dockerfile for {}", descriptor.id));
            assert!(dockerfile.contains("FROM "));
        }
    }

    #[test]
    fn claude_dockerfile_pins_version_and_runs_install_scripts() {
        let dockerfile = managed_dockerfile(&harness::CLAUDE).unwrap();
        assert!(dockerfile.contains("ARG CLAUDE_CODE_VERSION="));
        assert!(dockerfile.contains("@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}"));
        // postinstall links the native binary; --ignore-scripts leaves `claude` broken.
        assert!(!dockerfile.contains("--ignore-scripts"));
        assert!(dockerfile.contains("DISABLE_AUTOUPDATER=1"));
        assert!(dockerfile.contains("CLAUDE_CONFIG_DIR=/home/agent/.claude"));
        assert!(dockerfile.contains("ln -s /home/agent/.ssh /root/.ssh"));
    }
}
