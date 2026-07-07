pub fn pass() -> &'static str {
    "\x1b[1;32mPASS\x1b[0m"
}

pub fn warn() -> &'static str {
    "\x1b[1;33mWARN\x1b[0m"
}

pub fn fail() -> &'static str {
    "\x1b[1;31mFAIL\x1b[0m"
}

pub fn color_status_prefix(message: &str) -> String {
    if let Some(rest) = message.strip_prefix("PASS: ") {
        format!("{}: {rest}", pass())
    } else if let Some(rest) = message.strip_prefix("WARN: ") {
        format!("{}: {rest}", warn())
    } else if let Some(rest) = message.strip_prefix("FAIL: ") {
        format!("{}: {rest}", fail())
    } else {
        message.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_labels_are_bold_and_colored() {
        assert_eq!(pass(), "\x1b[1;32mPASS\x1b[0m");
        assert_eq!(warn(), "\x1b[1;33mWARN\x1b[0m");
        assert_eq!(fail(), "\x1b[1;31mFAIL\x1b[0m");
    }

    #[test]
    fn only_status_prefix_is_colored() {
        assert_eq!(
            color_status_prefix("WARN: check this"),
            "\x1b[1;33mWARN\x1b[0m: check this"
        );
        assert_eq!(color_status_prefix("plain"), "plain");
    }
}
