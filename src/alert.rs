use std::{
    env,
    ffi::OsStr,
    io::{self, IsTerminal},
};

use crate::config::{ColorMode, Config};

const PASS: &str = "PASS";
const WARN: &str = "WARN";
const FAIL: &str = "FAIL";

const COLORED_PASS: &str = "\x1b[1;32mPASS\x1b[0m";
const COLORED_WARN: &str = "\x1b[1;33mWARN\x1b[0m";
const COLORED_FAIL: &str = "\x1b[1;31mFAIL\x1b[0m";

pub fn pass() -> &'static str {
    pass_with_color(colors_enabled())
}

pub fn warn() -> &'static str {
    warn_with_color(colors_enabled())
}

pub fn fail() -> &'static str {
    fail_with_color(colors_enabled())
}

fn colors_enabled() -> bool {
    let config = Config::load_or_default().unwrap_or_default();
    colors_enabled_for_config(&config, io::stdout().is_terminal())
}

pub(crate) fn colors_enabled_for_config(config: &Config, stdout_is_terminal: bool) -> bool {
    colors_enabled_for_policy(
        config.ui.color,
        env::var_os("NO_COLOR").as_deref(),
        stdout_is_terminal,
    )
}

fn colors_enabled_for_policy(
    color_mode: ColorMode,
    no_color: Option<&OsStr>,
    stdout_is_terminal: bool,
) -> bool {
    if no_color.is_some_and(|value| !value.is_empty()) {
        return false;
    }

    match color_mode {
        ColorMode::Auto => stdout_is_terminal,
        ColorMode::Always => true,
        ColorMode::Never => false,
    }
}

fn pass_with_color(enabled: bool) -> &'static str {
    if enabled {
        COLORED_PASS
    } else {
        PASS
    }
}

fn warn_with_color(enabled: bool) -> &'static str {
    if enabled {
        COLORED_WARN
    } else {
        WARN
    }
}

fn fail_with_color(enabled: bool) -> &'static str {
    if enabled {
        COLORED_FAIL
    } else {
        FAIL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_labels_are_bold_and_colored_when_enabled() {
        assert_eq!(pass_with_color(true), "\x1b[1;32mPASS\x1b[0m");
        assert_eq!(warn_with_color(true), "\x1b[1;33mWARN\x1b[0m");
        assert_eq!(fail_with_color(true), "\x1b[1;31mFAIL\x1b[0m");
    }

    #[test]
    fn status_labels_are_plain_when_color_is_disabled() {
        assert_eq!(pass_with_color(false), "PASS");
        assert_eq!(warn_with_color(false), "WARN");
        assert_eq!(fail_with_color(false), "FAIL");
    }

    #[test]
    fn auto_color_follows_terminal_detection() {
        assert!(colors_enabled_for_policy(ColorMode::Auto, None, true));
        assert!(!colors_enabled_for_policy(ColorMode::Auto, None, false));
    }

    #[test]
    fn configured_color_policy_is_honored() {
        assert!(colors_enabled_for_policy(ColorMode::Always, None, false));
        assert!(!colors_enabled_for_policy(ColorMode::Never, None, true));
    }

    #[test]
    fn non_empty_no_color_disables_colors() {
        assert!(colors_enabled_for_policy(
            ColorMode::Always,
            Some(OsStr::new("")),
            false
        ));
        assert!(!colors_enabled_for_policy(
            ColorMode::Always,
            Some(OsStr::new("1")),
            true
        ));
    }
}
