use anyhow::Result;

use crate::{
    alert,
    config::{Config, SshMode},
    paths::{display_path, expand_tilde},
};

use super::{detect_host_agent, discovery::fingerprint_key, HostSshAgent};

pub fn status() -> Result<i32> {
    let config = Config::load_or_default()?;
    let host_agent = detect_host_agent();

    println!("SSH mode: {:?}", config.ssh.mode);
    println!();

    if config.ssh.selected_keys.is_empty() {
        println!("Selected keys: none");
    } else {
        println!("Selected keys:");
        for selected in &config.ssh.selected_keys {
            let path = expand_tilde(&selected.path);
            let display = display_path(&path);
            if !path.exists() {
                println!("{}: {display} - missing", alert::warn());
                continue;
            }

            match fingerprint_key(&path) {
                Ok(metadata) => {
                    let fp_status = match (&selected.fingerprint, &metadata.fingerprint) {
                        (Some(expected), Some(actual)) if expected == actual => alert::pass(),
                        (Some(expected), Some(actual)) => {
                            println!(
                                "{}: {display} - fingerprint changed: expected {expected}, got {actual}",
                                alert::fail()
                            );
                            continue;
                        }
                        (Some(expected), None) => {
                            println!(
                                "{}: {display} - could not verify configured fingerprint {expected}",
                                alert::warn()
                            );
                            continue;
                        }
                        _ => alert::pass(),
                    };
                    println!(
                        "{fp_status}: {display}{}{}{}",
                        metadata
                            .key_type
                            .as_deref()
                            .map(|v| format!(" - {v}"))
                            .unwrap_or_default(),
                        metadata
                            .fingerprint
                            .as_deref()
                            .map(|v| format!(" {v}"))
                            .unwrap_or_default(),
                        metadata
                            .comment
                            .as_deref()
                            .map(|v| format!(" {v}"))
                            .unwrap_or_default(),
                    );
                    if let (Some(name), Some(email)) =
                        (&selected.git_user_name, &selected.git_user_email)
                    {
                        println!("      Git identity override: {name} <{email}>");
                    }
                }
                Err(err) => println!(
                    "{}: {display} - could not inspect key: {err:#}",
                    alert::warn()
                ),
            }
        }
    }

    println!();
    println!("Host agent:");
    match &host_agent {
        HostSshAgent::Ready(_) => println!("{}: {}", alert::pass(), host_agent.status_detail()),
        _ => println!("{}: {}", alert::warn(), host_agent.status_detail()),
    }

    println!();
    println!("Next launch:");
    println!(
        "{}",
        alert::color_status_prefix(&next_launch_detail(&config, &host_agent))
    );

    Ok(0)
}

fn next_launch_detail(config: &Config, host_agent: &HostSshAgent) -> String {
    match config.ssh.mode {
        SshMode::Off => "SSH forwarding is disabled.".to_owned(),
        SshMode::Managed => {
            if config.ssh.selected_keys.is_empty() {
                "WARN: managed mode is enabled but no keys are selected. Run: vr ssh configure"
                    .to_owned()
            } else {
                format!(
                    "PASS: Vegasroom will start a managed temporary ssh-agent with {} configured key(s).",
                    config.ssh.selected_keys.len()
                )
            }
        }
        SshMode::Host => {
            if host_agent.is_ready() {
                "PASS: Vegasroom will forward the existing host SSH_AUTH_SOCK.".to_owned()
            } else {
                "WARN: host mode is enabled but no usable host SSH_AUTH_SOCK was detected."
                    .to_owned()
            }
        }
        SshMode::Auto => {
            if !config.ssh.selected_keys.is_empty() {
                format!(
                    "PASS: Vegasroom will start a managed temporary ssh-agent with {} configured key(s).",
                    config.ssh.selected_keys.len()
                )
            } else if host_agent.is_ready() {
                "PASS: Vegasroom will forward the existing host SSH_AUTH_SOCK.".to_owned()
            } else {
                "WARN: no managed keys or host SSH agent detected. Git over SSH may not work inside the room.".to_owned()
            }
        }
    }
}
