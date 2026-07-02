mod assets;
mod cli;
mod config;
mod docker;
mod doctor;
mod paths;
mod ssh;

fn main() {
    match cli::run() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("error: {:#}", err);
            std::process::exit(1);
        }
    }
}
