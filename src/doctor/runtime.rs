use std::{fs, path::Path};

use super::{check_bool, Check, Status};

pub(super) fn check_compose_runtime_settings(compose_file: &Path) -> Vec<Check> {
    let mut checks = Vec::new();
    let Ok(contents) = fs::read_to_string(compose_file) else {
        return checks;
    };

    checks.push(check_bool(
        Status::Warn,
        "Compose image setting",
        contents.contains("image: ${VR_PI_IMAGE:-vegasroom/pi:local}"),
        "image is controlled by harness.pi.image through VR_PI_IMAGE",
        "Compose image is not controlled by VR_PI_IMAGE",
    ));

    checks.push(check_bool(
        Status::Warn,
        "Compose build network",
        contents.contains("network: ${VR_PI_BUILD_NETWORK:-host}"),
        "build.network is controlled by harness.pi.network through VR_PI_BUILD_NETWORK",
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

    checks.push(check_bool(
        Status::Warn,
        "SSH directory mount model",
        contents.contains(".vegasroom/ssh")
            && contents.contains("target: /home/agent/.ssh")
            && contents.contains("target: /root/.ssh"),
        "SSH directory mount is preserved for Pi HOME and root SSH",
        "SSH directory mount was not found for both /home/agent/.ssh and /root/.ssh in compose.yaml",
    ));

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
