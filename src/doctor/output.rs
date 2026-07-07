use crate::alert;

use super::{Check, Status};

pub(super) fn print_checks(checks: &[Check]) {
    for check in checks {
        let label = match check.status {
            Status::Pass => alert::pass(),
            Status::Warn => alert::warn(),
            Status::Fail => alert::fail(),
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

    println!(
        "\nSummary: {pass} {}, {warn} {}, {fail} {}",
        alert::pass(),
        alert::warn(),
        alert::fail()
    );
}
