use crate::harness::{self, HarnessDescriptor};

pub const MANAGED_PI_DOCKERFILE: &str = include_str!("../harness/pi/Dockerfile");

/// Embedded Dockerfile for a registered harness, written to the runtime dir at `vr init`.
pub fn managed_dockerfile(descriptor: &HarnessDescriptor) -> Option<&'static str> {
    match descriptor.id {
        id if id == harness::PI.id => Some(MANAGED_PI_DOCKERFILE),
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
}
