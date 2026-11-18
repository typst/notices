//! The `release` subcommand.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use notices::ErrorKind;
use notices::cargo::release;

use super::ProjectArgs;

/// Writes the NOTICE file and JSON document for a release of a Cargo project.
///
/// Merges the licenses of all dependencies with the project's own NOTICE file,
/// which is `NOTICE` next to the manifest, if there is one.
#[derive(Parser)]
pub struct Args {
    /// Where to write the licenses. Can be repeated or separated by commas.
    /// Files ending in `.json` are written as JSON (see `json.md`), all others
    /// as a Typst NOTICE file.
    #[arg(short, long, required = true, value_delimiter = ',')]
    output: Vec<PathBuf>,

    #[command(flatten)]
    project: ProjectArgs,
}

/// Runs the release and prints its results.
pub fn run(args: &Args) -> ExitCode {
    match write(args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Returns whether the outputs were written.
fn write(args: &Args) -> Result<bool, String> {
    let (config, options) = args.project.load(Path::new("."))?;

    let notice = options.manifest_path.with_file_name("NOTICE");
    let repo_notice = if notice.is_file() { Some(read_notice(&notice)?) } else { None };

    let (blocks, report) =
        release(repo_notice, &config, &options).map_err(|e| format!("{e:#}"))?;
    eprint!("{report}");
    if report.has_errors() {
        eprintln!("release failed, nothing was written");
        return Ok(false);
    }

    // Render everything before writing anything so that a failure leaves no
    // partial results.
    let mut rendered = Vec::new();
    for path in &args.output {
        let json = path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
        let text = if json {
            notices::to_json(&blocks)
        } else {
            notices::write(&blocks).map_err(|e| e.to_string())?
        };
        rendered.push((path, text));
    }
    for (path, text) in rendered {
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }

    let items = blocks.iter().map(|block| block.items.len()).sum::<usize>();
    println!("{} license blocks, {items} items", blocks.len());
    Ok(true)
}

/// Reads and parses the repository's Typst NOTICE file.
fn read_notice(path: &Path) -> Result<Vec<notices::LicenseBlock>, String> {
    let text =
        notices::read_text(path).map_err(|e| format!("{}: {e}", path.display()))?;
    notices::parse(&text).map_err(|err| match err.kind {
        ErrorKind::MissingHeader => format!(
            "{}: the file does not start with the Typst NOTICE header; the \
             repository's NOTICE file must be in the format",
            path.display(),
        ),
        _ => format!("{}:{}:{}: {}", path.display(), err.line, err.column, err.message),
    })
}
