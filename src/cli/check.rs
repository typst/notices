//! The `check` subcommand.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use notices::ErrorKind;
#[cfg(feature = "cargo")]
use notices::cargo::Severity;

/// Validates the NOTICE file of a repository.
#[derive(Parser)]
pub struct Args {
    /// The NOTICE file, or a directory containing a file named `NOTICE`.
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Accept files without the Typst NOTICE header as ordinary NOTICE files.
    #[arg(long)]
    allow_ordinary: bool,

    /// Accept a directory without a NOTICE file.
    #[arg(long)]
    allow_missing: bool,

    /// Also check that all dependencies use allowed licenses.
    #[cfg(feature = "cargo")]
    #[arg(long)]
    deps: bool,

    #[cfg(feature = "cargo")]
    #[command(flatten)]
    project: super::ProjectArgs,
}

/// Runs the checks selected by the arguments and prints their results.
pub fn run(args: &Args) -> ExitCode {
    let mut success = true;

    #[cfg(feature = "cargo")]
    if !args.deps && args.project != super::ProjectArgs::default() {
        eprintln!("error: the options for the dependency check require `--deps`");
        return ExitCode::FAILURE;
    }

    if let Err(message) = check_notice(args) {
        eprintln!("error: {message}");
        success = false;
    }

    #[cfg(feature = "cargo")]
    if args.deps {
        success &= check_deps(args).unwrap_or_else(|message| {
            eprintln!("error: {message}");
            false
        });
    }

    if success { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Validates the repository's NOTICE file and prints a summary.
fn check_notice(args: &Args) -> Result<(), String> {
    let path = if args.path.is_dir() {
        let file = args.path.join("NOTICE");
        if !file.is_file() {
            if args.allow_missing {
                println!(
                    "note: {}: the directory contains no NOTICE file",
                    args.path.display()
                );
                return Ok(());
            }

            return Err(format!(
                "{}: the directory contains no file named `NOTICE`",
                args.path.display(),
            ));
        }
        file
    } else {
        args.path.clone()
    };

    let text =
        notices::read_text(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    match notices::parse(&text) {
        Ok(blocks) => {
            let mut items = 0;
            for block in &blocks {
                let notice = if block.notice.is_some() { ", with notice" } else { "" };
                println!("{}: {} item(s){notice}", block.license, block.items.len());
                items += block.items.len();
            }
            println!(
                "valid Typst NOTICE file: {} license blocks, {items} items",
                blocks.len()
            );
            Ok(())
        }
        Err(error) if error.kind == ErrorKind::MissingHeader => {
            if args.allow_ordinary {
                println!("{}: accepted as an ordinary NOTICE file", path.display());
                Ok(())
            } else {
                Err(format!(
                    "{}: the file does not start with the Typst NOTICE header; pass \
                     `--allow-ordinary` to accept ordinary NOTICE files",
                    path.display(),
                ))
            }
        }
        Err(error) => Err(format!(
            "{}:{}:{}: {}",
            path.display(),
            error.line,
            error.column,
            error.message,
        )),
    }
}

/// Checks the licenses of the dependencies of the project and prints the
/// problems. Returns whether there were no errors.
#[cfg(feature = "cargo")]
fn check_deps(args: &Args) -> Result<bool, String> {
    let dir = if args.path.is_dir() {
        args.path.as_path()
    } else {
        args.path.parent().unwrap_or(".".as_ref())
    };
    let (config, options) = args.project.load(dir)?;

    let report =
        notices::cargo::check(&config, &options).map_err(|e| format!("{e:#}"))?;
    eprint!("{report}");
    eprintln!(
        "dependency licenses: {} error(s), {} warning(s)",
        report.count(Severity::Error),
        report.count(Severity::Warning),
    );
    Ok(!report.has_errors())
}
