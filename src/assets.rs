use crate::harness::{self, HarnessDescriptor};

/// Shared base foundation; the environment profile is appended at build time.
pub const MANAGED_BASE_DOCKERFILE: &str = include_str!("../harness/base/Dockerfile");
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
    }

    #[test]
    fn harness_dockerfiles_leave_the_foundation_to_the_base_image() {
        for descriptor in harness::HARNESSES {
            let dockerfile = managed_dockerfile(descriptor).unwrap();
            for foundation in [
                "apt-get",
                "node:24-bookworm-slim",
                "/root/.ssh",
                "WORKDIR",
                "HOME=",
                "NPM_CONFIG_CACHE",
            ] {
                assert!(
                    !dockerfile.contains(foundation),
                    "{} Dockerfile duplicates base foundation: {foundation}",
                    descriptor.id
                );
            }
        }
        assert!(MANAGED_BASE_DOCKERFILE.contains("ln -s /home/agent/.ssh /root/.ssh"));
        assert!(MANAGED_BASE_DOCKERFILE.contains("WORKDIR /workspace"));
    }

    #[test]
    fn harness_dockerfiles_build_from_base_image_arg() {
        for descriptor in harness::HARNESSES {
            let dockerfile = managed_dockerfile(descriptor).unwrap();
            let instructions: Vec<&str> = dockerfile
                .lines()
                .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
                .collect();
            assert_eq!(
                &instructions[..2],
                &[
                    format!("ARG BASE_IMAGE={}", crate::config::DEFAULT_BASE_IMAGE).as_str(),
                    "FROM ${BASE_IMAGE}"
                ],
                "{}",
                descriptor.id
            );
        }
    }
}
