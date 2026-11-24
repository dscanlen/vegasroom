mod alert;
mod assets;
mod atomic_write;
mod cli;
mod config;
mod config_ui;
mod docker;
mod doctor;
mod harness;
mod paths;
mod ssh;
mod workspace;

#[cfg(test)]
mod test_util;

fn main() {
    match cli::run() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("error: {:#}", err);
            std::process::exit(1);
        }
    }
}
