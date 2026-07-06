use super::{Check, Status};

pub(super) fn print_checks(checks: &[Check]) {
    for check in checks {
        let label = match check.status {
            Status::Pass => "PASS",
            Status::Warn => "WARN",
            Status::Fail => "FAIL",
        };
        println!("{label}: {} - {}", check.name, check.detail);
    }

    let pass = checks
        .iter()
        .filter(|check| check.status == Status::Pass)
        .count();
    let warn = checks
        .iter()
        .filter(|check| check.status == Status::Warn)
        .count();
    let fail = checks
        .iter()
        .filter(|check| check.status == Status::Fail)
        .count();

    println!("\nSummary: {pass} PASS, {warn} WARN, {fail} FAIL");
}
