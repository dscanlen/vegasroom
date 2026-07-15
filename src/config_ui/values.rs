use crate::{
    config::{ColorMode, Config},
    docker,
};

pub(super) fn color_mode_name(mode: ColorMode) -> &'static str {
    match mode {
        ColorMode::Auto => "auto",
        ColorMode::Always => "always",
        ColorMode::Never => "never",
    }
}

pub(super) fn git_identity_preview(config: &Config) -> Vec<String> {
    match docker::effective_git_identity(config) {
        Some(identity) => vec![
            format!("Effective: {} <{}>", identity.name, identity.email),
            format!("Source: {}", identity.source),
        ],
        None => vec![
            "Effective: not configured".to_owned(),
            "Set git.user_name/git.user_email, selected-key Git metadata, or enable host inheritance."
                .to_owned(),
        ],
    }
}
