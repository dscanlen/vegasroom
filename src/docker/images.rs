//! Shared base image status, harness-on-base checks, and unused image detection.
//!
//! Docker access is kept to small `inspect`/`ls` helpers; the decisions are pure
//! functions so they can be tested without a Docker daemon.

use std::process::Stdio;

use anyhow::{Context, Result};

use crate::{config::Config, harness};

use super::{base_docker, environment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseStatus {
    /// `environment.base_image` does not exist locally.
    Missing,
    /// It exists but was built from a different environment profile (or before
    /// fingerprints were recorded).
    Stale,
    /// It was built from the current environment profile.
    Current,
}

/// Status of the shared base image for the current config.
pub fn base_status(config: &Config) -> Result<BaseStatus> {
    let image = &config.environment.base_image;
    let Some(label) = image_label(config, image, environment::BASE_FINGERPRINT_LABEL)? else {
        return Ok(BaseStatus::Missing);
    };
    Ok(classify_base(
        label.as_deref(),
        &environment::base_fingerprint(config),
    ))
}

/// Whether `descriptor`'s image was built on the current base image. `None` when
/// either image is missing, so there is nothing to compare.
pub fn harness_on_current_base(
    config: &Config,
    descriptor: &harness::HarnessDescriptor,
) -> Result<Option<bool>> {
    let Some(base) = image_layers(config, &config.environment.base_image)? else {
        return Ok(None);
    };
    let harness_image = config.harness_runtime_config(descriptor).image;
    let Some(layers) = image_layers(config, harness_image)? else {
        return Ok(None);
    };
    Ok(Some(is_layer_prefix(&base, &layers)))
}

/// Local tags in the base/harness repositories that the current config no longer
/// uses: e.g. pre-M3 `-env` images or a previously configured harness tag.
pub fn unused_images(config: &Config) -> Result<Vec<String>> {
    let (keep, repositories) = keep_and_repositories(config);
    let output = base_docker(config)
        .args(["image", "ls", "--format", "{{.Repository}}:{{.Tag}}"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .context("failed to list Docker images")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    Ok(select_unused(listing.lines(), &keep, &repositories))
}

/// Ready-to-run removal command for `images`, using the configured Docker context.
pub fn removal_command(config: &Config, images: &[String]) -> String {
    format!(
        "docker --context {} image rm {}",
        config.docker.context,
        images.join(" ")
    )
}

fn keep_and_repositories(config: &Config) -> (Vec<String>, Vec<String>) {
    let mut keep = vec![config.environment.base_image.clone()];
    let mut repositories = vec![
        repository(&config.environment.base_image).to_owned(),
        repository(crate::config::DEFAULT_BASE_IMAGE).to_owned(),
    ];
    for descriptor in harness::HARNESSES {
        let configured = config.harness_runtime_config(descriptor).image;
        keep.extend([
            configured.to_owned(),
            descriptor.default_image.to_owned(),
            descriptor.versioned_image.to_owned(),
        ]);
        repositories.extend([
            repository(configured).to_owned(),
            repository(descriptor.default_image).to_owned(),
        ]);
    }
    repositories.sort();
    repositories.dedup();
    (keep, repositories)
}

fn select_unused<'a>(
    listing: impl Iterator<Item = &'a str>,
    keep: &[String],
    repositories: &[String],
) -> Vec<String> {
    let mut unused: Vec<String> = listing
        .map(str::trim)
        .filter(|tagged| !tagged.is_empty() && !tagged.contains("<none>"))
        .map(|tagged| tagged.strip_prefix("docker.io/").unwrap_or(tagged))
        .filter(|tagged| repositories.iter().any(|repo| repository(tagged) == repo))
        .filter(|tagged| !keep.iter().any(|kept| kept == tagged))
        .map(str::to_owned)
        .collect();
    unused.sort();
    unused.dedup();
    unused
}

fn classify_base(label: Option<&str>, expected: &str) -> BaseStatus {
    if label == Some(expected) {
        BaseStatus::Current
    } else {
        BaseStatus::Stale
    }
}

fn is_layer_prefix(base: &[String], harness: &[String]) -> bool {
    !base.is_empty() && harness.len() > base.len() && harness.starts_with(base)
}

/// `Ok(None)` when the image does not exist; `Ok(Some(None))` when it exists
/// without the label.
fn image_label(config: &Config, image: &str, key: &str) -> Result<Option<Option<String>>> {
    let format = format!("{{{{ index .Config.Labels \"{key}\" }}}}");
    let Some(value) = inspect(config, image, &format)? else {
        return Ok(None);
    };
    let value = value.trim();
    Ok(Some(
        (!value.is_empty() && value != "<no value>").then(|| value.to_owned()),
    ))
}

fn image_layers(config: &Config, image: &str) -> Result<Option<Vec<String>>> {
    let Some(json) = inspect(config, image, "{{json .RootFS.Layers}}")? else {
        return Ok(None);
    };
    parse_layers(&json).map(Some)
}

fn parse_layers(json: &str) -> Result<Vec<String>> {
    serde_yaml::from_str::<Option<Vec<String>>>(json.trim())
        .map(Option::unwrap_or_default)
        .context("failed to parse image layers")
}

fn inspect(config: &Config, image: &str, format: &str) -> Result<Option<String>> {
    let output = base_docker(config)
        .args(["image", "inspect", "-f", format, image])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .context("failed to inspect Docker image")?;
    Ok(output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned()))
}

/// Image reference without its tag or digest (`vegasroom/pi:local` -> `vegasroom/pi`).
fn repository(image: &str) -> &str {
    let image = image.split_once('@').map_or(image, |(name, _)| name);
    let last_slash = image.rfind('/');
    match image.rfind(':') {
        Some(colon) if last_slash.is_none_or(|slash| colon > slash) => &image[..colon],
        _ => image,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layers(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn repository_strips_tag_and_digest_but_not_registry_port() {
        assert_eq!(repository("vegasroom/pi:local"), "vegasroom/pi");
        assert_eq!(repository("vegasroom/pi"), "vegasroom/pi");
        assert_eq!(repository("vegasroom/pi@sha256:abc"), "vegasroom/pi");
        assert_eq!(repository("vegasroom/pi:latest@sha256:abc"), "vegasroom/pi");
        assert_eq!(
            repository("localhost:5000/vegasroom/pi:x"),
            "localhost:5000/vegasroom/pi"
        );
        assert_eq!(
            repository("localhost:5000/vegasroom/pi"),
            "localhost:5000/vegasroom/pi"
        );
    }

    #[test]
    fn harness_must_extend_base_layers() {
        let base = layers(&["a", "b"]);
        assert!(is_layer_prefix(&base, &layers(&["a", "b", "c"])));
        assert!(!is_layer_prefix(&base, &layers(&["a", "x", "c"])));
        assert!(!is_layer_prefix(&base, &layers(&["a", "b"])));
        assert!(!is_layer_prefix(&[], &layers(&["a"])));
    }

    #[test]
    fn base_without_matching_fingerprint_is_stale() {
        assert_eq!(
            classify_base(Some("fnv1a64:1"), "fnv1a64:1"),
            BaseStatus::Current
        );
        assert_eq!(
            classify_base(Some("fnv1a64:2"), "fnv1a64:1"),
            BaseStatus::Stale
        );
        assert_eq!(classify_base(None, "fnv1a64:1"), BaseStatus::Stale);
    }

    #[test]
    fn parse_layers_reads_inspect_json() {
        assert_eq!(
            parse_layers("[\"sha256:a\",\"sha256:b\"]\n").unwrap(),
            layers(&["sha256:a", "sha256:b"])
        );
        assert!(parse_layers("null").unwrap().is_empty());
    }

    #[test]
    fn unused_images_are_unkept_tags_in_vegasroom_repositories() {
        let mut config = Config::default();
        config.harness.pi.image = "vegasroom/pi:latest".to_owned();
        let (keep, repositories) = keep_and_repositories(&config);
        let listing = [
            "vegasroom/pi:latest",
            "vegasroom/pi:local",
            "vegasroom/pi:local-env",
            "docker.io/vegasroom/claude:latest-env",
            "vegasroom/claude:latest",
            "vegasroom/base:latest",
            "vegasroom/base:old",
            harness::PI.versioned_image,
            "node:24-bookworm-slim",
            "other/pi:local",
            "<none>:<none>",
            "",
        ];

        assert_eq!(
            select_unused(listing.into_iter(), &keep, &repositories),
            vec![
                "vegasroom/base:old",
                "vegasroom/claude:latest-env",
                "vegasroom/pi:local",
                "vegasroom/pi:local-env",
            ]
        );
    }

    #[test]
    fn custom_configured_tags_are_kept() {
        let mut config = Config::default();
        config.harness.pi.image = "example/pi:mine".to_owned();
        config.environment.base_image = "example/base:mine".to_owned();
        let (keep, repositories) = keep_and_repositories(&config);
        let listing = ["example/pi:mine", "example/pi:old", "example/base:mine"];

        assert_eq!(
            select_unused(listing.into_iter(), &keep, &repositories),
            vec!["example/pi:old"]
        );
    }

    #[test]
    fn removal_command_uses_configured_context() {
        let config = Config::default();
        assert_eq!(
            removal_command(&config, &["vegasroom/pi:local".to_owned()]),
            "docker --context rootless image rm vegasroom/pi:local"
        );
    }
}
