mod alert;
mod assets;
mod cli;
mod config;
mod docker;
mod doctor;
mod harness;
mod paths;
mod ssh;
mod workspace;

fn main() {
    match cli::run() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("error: {:#}", err);
            std::process::exit(1);
        }
    }
}
