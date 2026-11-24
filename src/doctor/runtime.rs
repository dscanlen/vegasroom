use std::{fs, path::Path};

use crate::harness;

use super::{check_bool, Check, Status};

fn container_home_path(relative: &str) -> String {
    format!(
        "{}/{}",
        harness::PI.container_home.trim_end_matches('/'),
        relative.trim_start_matches('/')
    )
}

pub(super) fn check_compose_runtime_settings(compose_file: &Path) -> Vec<Check> {
    let mut checks = Vec::new();
    let Ok(contents) = fs::read_to_string(compose_file) else {
        return checks;
    };
    let harness_ssh_dir = container_home_path(".ssh");
    let harness_ssh_target = format!("target: {harness_ssh_dir}");
    let root_ssh_symlink = format!("ln -s {harness_ssh_dir} /root/.ssh");

    checks.extend(compose_var_checks(&contents, harness::HARNESSES));

    checks.push(check_bool(
        Status::Warn,
        "Container user",
        contents.contains("user: \"0:0\"")
            || contents.contains("user: '0:0'")
            || contents.contains("user: 0:0"),
        "container-root runtime is preserved",
        "container-root runtime setting was not found in compose.yaml",
    ));

    checks.push(check_bool(
        Status::Warn,
        "No new privileges",
        contents.contains("no-new-privileges:true"),
        "no-new-privileges is enabled for the room container",
        "no-new-privileges was not found in compose.yaml",
    ));

    checks.push(check_bool(
        Status::Warn,
        "Capability drop",
        contents.contains("cap_drop:") && contents.contains("- ALL"),
        "all default Linux capabilities are dropped for the room container",
        "cap_drop: ALL was not found in compose.yaml",
    ));

    checks.push(check_bool(
        Status::Warn,
        "Container init",
        contents.contains("init: true"),
        "a minimal init process is enabled for child-process reaping",
        "init: true was not found in compose.yaml",
    ));

    checks.push(check_bool(
        Status::Warn,
        "Workspace read-only option",
        contents.contains("read_only: ${VR_WORKSPACE_READ_ONLY:-false}"),
        "workspace mount read-only mode is controlled by harness.<id>.read_only_workspace of the launched harness",
        "workspace mount is not controlled by VR_WORKSPACE_READ_ONLY",
    ));

    let npm_global_path = container_home_path(".npm-global");
    checks.push(check_bool(
        Status::Warn,
        "Pi npm-global persistence",
        contents.contains(".vegasroom/harness/pi/npm-global")
            && contents.contains(format!("target: {npm_global_path}").as_str())
            && contents.contains("NPM_CONFIG_PREFIX: /home/agent/.npm-global")
            && contents.contains("/home/agent/.npm-global/bin"),
        "in-room npm-global installs are persisted and placed before the baked Pi fallback on PATH",
        "persistent npm-global mount/PATH configuration was not found; in-room Pi updates may not persist",
    ));

    checks.push(check_bool(
        Status::Warn,
        "SSH directory mount model",
        contents.contains(".vegasroom/ssh")
            && contents.contains(&harness_ssh_target)
            && !contents.contains("target: /root/.ssh"),
        format!("SSH state is mounted once at {harness_ssh_dir}"),
        format!(
            "SSH directory mount should target {harness_ssh_dir} without a duplicate /root/.ssh bind mount"
        ),
    ));

    if let Some(project_dir) = compose_file.parent() {
        let dockerfile = project_dir.join(harness::PI.dockerfile_path);
        if let Ok(dockerfile_contents) = fs::read_to_string(&dockerfile) {
            checks.push(check_bool(
                Status::Warn,
                "Root SSH symlink model",
                dockerfile_contents.contains(&root_ssh_symlink),
                format!(
                    "/root/.ssh is created as an image-level symlink to {harness_ssh_dir}"
                ),
                format!(
                    "/root/.ssh image-level symlink to {harness_ssh_dir} was not found in the Pi Dockerfile"
                ),
            ));

            checks.push(check_bool(
                Status::Warn,
                "Pi Dockerfile npm-global PATH",
                dockerfile_contents.contains("NPM_CONFIG_PREFIX=/home/agent/.npm-global")
                    && dockerfile_contents.contains("/home/agent/.npm-global/bin"),
                "Dockerfile keeps user-updated Pi packages ahead of the baked fallback on PATH",
                "Dockerfile does not configure the persistent npm-global prefix/PATH",
            ));
        }
    }

    checks.push(Check {
        status: Status::Pass,
        name: "SSH agent mount model",
        detail: "ssh-agent socket mount is generated dynamically when SSH_AUTH_SOCK is usable"
            .to_owned(),
    });

    checks.push(check_bool(
        Status::Warn,
        "Login browser opener",
        contents.contains("BROWSER: echo"),
        "BROWSER=echo is set so Pi browser-login URLs are printed for host-browser use",
        "BROWSER=echo was not found in compose.yaml; Pi may try to open a browser inside the container",
    ));

    checks
}

/// Check that each harness service's image/network settings are driven by its
/// `VR_<ID>_*` interpolation variable. One check per category covering all harnesses.
fn compose_var_checks(contents: &str, harnesses: &[&harness::HarnessDescriptor]) -> Vec<Check> {
    type Pattern = fn(&str) -> String;
    let categories: [(&'static str, &str, Pattern, &str); 3] = [
        (
            "Compose image setting",
            "IMAGE",
            |var| format!("image: ${{{var}:-"),
            "image",
        ),
        (
            "Compose build network",
            "BUILD_NETWORK",
            |var| format!("network: ${{{var}:-host}}"),
            "build_network",
        ),
        (
            "Compose runtime network",
            "NETWORK_MODE",
            |var| format!("network_mode: ${{{var}:-host}}"),
            "network",
        ),
    ];

    categories
        .into_iter()
        .map(|(name, suffix, pattern, field)| {
            let vars: Vec<String> = harnesses.iter().map(|d| d.compose_var(suffix)).collect();
            let missing: Vec<&str> = vars
                .iter()
                .filter(|var| !contents.contains(&pattern(var)))
                .map(String::as_str)
                .collect();
            check_bool(
                Status::Warn,
                name,
                missing.is_empty(),
                format!(
                    "controlled by harness.<id>.{field} through {}",
                    vars.join(", ")
                ),
                format!("not controlled by {}", missing.join(", ")),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_var_checks_pass_for_each_registered_harness() {
        let mut contents = String::new();
        for d in harness::HARNESSES {
            contents.push_str(&format!(
                "image: ${{{}:-x}}\nnetwork: ${{{}:-host}}\nnetwork_mode: ${{{}:-host}}\n",
                d.compose_var("IMAGE"),
                d.compose_var("BUILD_NETWORK"),
                d.compose_var("NETWORK_MODE"),
            ));
        }
        let checks = compose_var_checks(&contents, harness::HARNESSES);
        assert_eq!(checks.len(), 3);
        assert!(checks.iter().all(|c| c.status == Status::Pass));
    }

    #[test]
    fn compose_var_checks_name_the_missing_variable() {
        let checks = compose_var_checks("services:\n", &[&harness::PI]);
        assert!(checks.iter().all(|c| c.status == Status::Warn));
        assert!(checks[0].detail.contains("VR_PI_IMAGE"));
    }
}
