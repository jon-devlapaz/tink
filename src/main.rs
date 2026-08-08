use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use clap_complete::CompleteEnv;

use tink::{Cli, run};

fn main() -> ExitCode {
    CompleteEnv::with_factory(Cli::command).complete();

    // Parse first so --help / --version work when the process has no cwd
    // (e.g. shell still open after `rm -rf` of the working directory).
    let cli = Cli::parse();
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(err) => {
            eprintln!("Failed to resolve current directory: {err}");
            return ExitCode::from(1);
        }
    };
    run(cli, cwd)
}
