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

    checks.push(check_bool(
        Status::Warn,
        "Compose image setting",
        contents
            .contains(format!("image: ${{VR_PI_IMAGE:-{}}}", harness::PI.default_image).as_str()),
        "image is controlled by harness.pi.image through VR_PI_IMAGE",
        "Compose image is not controlled by VR_PI_IMAGE",
    ));

    checks.push(check_bool(
        Status::Warn,
        "Compose build network",
        contents.contains("network: ${VR_PI_BUILD_NETWORK:-host}"),
        "build.network is controlled by harness.pi.build_network through VR_PI_BUILD_NETWORK",
        "build.network is not controlled by VR_PI_BUILD_NETWORK",
    ));

    checks.push(check_bool(
        Status::Warn,
        "Compose runtime network",
        contents.contains("network_mode: ${VR_PI_NETWORK_MODE:-host}"),
        "network_mode is controlled by harness.pi.network through VR_PI_NETWORK_MODE",
        "network_mode is not controlled by VR_PI_NETWORK_MODE",
    ));

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
        "workspace mount read-only mode is controlled by harness.pi.read_only_workspace",
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
