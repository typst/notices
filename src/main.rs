//! The command line interface of `notices`.

mod cli;

use std::process::ExitCode;

fn main() -> ExitCode {
    cli::run()
}
