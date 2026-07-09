use std::{env, ffi::OsStr};

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

pub fn color_status_prefix(message: &str) -> String {
    color_status_prefix_with_color(message, colors_enabled())
}

fn colors_enabled() -> bool {
    colors_enabled_for_no_color(env::var_os("NO_COLOR").as_deref())
}

fn colors_enabled_for_no_color(no_color: Option<&OsStr>) -> bool {
    match no_color {
        Some(value) => value.is_empty(),
        None => true,
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

fn color_status_prefix_with_color(message: &str, enabled: bool) -> String {
    if let Some(rest) = message.strip_prefix("PASS: ") {
        format!("{}: {rest}", pass_with_color(enabled))
    } else if let Some(rest) = message.strip_prefix("WARN: ") {
        format!("{}: {rest}", warn_with_color(enabled))
    } else if let Some(rest) = message.strip_prefix("FAIL: ") {
        format!("{}: {rest}", fail_with_color(enabled))
    } else {
        message.to_owned()
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
    fn non_empty_no_color_disables_colors() {
        assert!(colors_enabled_for_no_color(None));
        assert!(colors_enabled_for_no_color(Some(OsStr::new(""))));
        assert!(!colors_enabled_for_no_color(Some(OsStr::new("1"))));
    }

    #[test]
    fn only_status_prefix_is_colored() {
        assert_eq!(
            color_status_prefix_with_color("WARN: check this", true),
            "\x1b[1;33mWARN\x1b[0m: check this"
        );
        assert_eq!(color_status_prefix_with_color("plain", true), "plain");
    }

    #[test]
    fn status_prefix_is_plain_when_color_is_disabled() {
        assert_eq!(
            color_status_prefix_with_color("WARN: check this", false),
            "WARN: check this"
        );
        assert_eq!(color_status_prefix_with_color("plain", false), "plain");
    }
}
