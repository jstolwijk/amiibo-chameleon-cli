mod backup;
mod cli;
mod database;
mod device;
mod dump;
mod error;
mod names;
mod protocol;

use std::process::ExitCode;

#[cfg(not(target_os = "macos"))]
compile_error!("amiibo v1 supports macOS only");

fn main() -> ExitCode {
    match cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
