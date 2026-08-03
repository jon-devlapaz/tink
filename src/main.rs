use std::process::ExitCode;

use clap::Parser;
use tink::{run, Cli};

fn main() -> ExitCode {
    let cli = Cli::parse();
    run(cli, std::env::current_dir().expect("current directory"))
}
