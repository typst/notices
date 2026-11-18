//! The subcommands of the command line interface.

mod check;
#[cfg(feature = "cargo")]
mod release;

use std::process::ExitCode;

#[cfg(feature = "cargo")]
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
#[cfg(feature = "cargo")]
use notices::cargo::{CONFIG_PATH, Config, MetadataOptions};

/// Works with Typst NOTICE files.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validates the NOTICE file of a repository.
    Check(check::Args),
    /// Writes the NOTICE and JSON files for a release of a Cargo project.
    #[cfg(feature = "cargo")]
    Release(release::Args),
}

/// Runs the command selected on the command line.
pub fn run() -> ExitCode {
    match Cli::parse().command {
        Command::Check(args) => check::run(&args),
        #[cfg(feature = "cargo")]
        Command::Release(args) => release::run(&args),
    }
}

// The options for analyzing a Cargo project, shared by the subcommands.

/// Options for analyzing a Cargo project.
#[cfg(feature = "cargo")]
#[derive(Parser, Default, PartialEq)]
pub struct ProjectArgs {
    /// The `Cargo.toml` of the project.
    #[arg(long)]
    manifest_path: Option<PathBuf>,

    /// The configuration file. Defaults to `.cargo/notices.toml` in the
    /// workspace root.
    #[arg(long)]
    config: Option<PathBuf>,

    /// The features of the project to enable, separated by commas.
    #[arg(long, value_delimiter = ',')]
    features: Vec<String>,

    /// Enable all features of the project.
    #[arg(long)]
    all_features: bool,

    /// Disable the default features of the project.
    #[arg(long)]
    no_default_features: bool,

    /// Require an up-to-date `Cargo.lock`.
    #[arg(long)]
    locked: bool,

    /// Forbid network access.
    #[arg(long)]
    offline: bool,

    /// Require an up-to-date `Cargo.lock` and forbid network access.
    #[arg(long)]
    frozen: bool,

    /// A target to consider dependencies for. Can be repeated. Replaces the
    /// targets in the configuration.
    #[arg(long)]
    target: Vec<String>,
}

#[cfg(feature = "cargo")]
impl ProjectArgs {
    /// Loads the configuration, finding it from the manifest if it is not
    /// specified, and creates the options for gathering the project's metadata.
    /// The manifest is in `dir` unless specified.
    pub fn load(&self, dir: &Path) -> Result<(Config, MetadataOptions), String> {
        let manifest_path =
            self.manifest_path.clone().unwrap_or_else(|| dir.join("Cargo.toml"));
        let path = match &self.config {
            Some(path) => path.clone(),
            None => Config::find(&manifest_path).ok_or_else(|| {
                format!(
                    "no configuration file found for `{}`: expected `{CONFIG_PATH}` in \
                     the workspace root, or pass `--config`",
                    manifest_path.display(),
                )
            })?,
        };

        let mut config = Config::load(&path)?;
        if !self.target.is_empty() {
            config.set_targets(self.target.iter().cloned());
        }

        let options = MetadataOptions {
            manifest_path,
            features: self.features.clone(),
            all_features: self.all_features,
            no_default_features: self.no_default_features,
            locked: self.locked,
            offline: self.offline,
            frozen: self.frozen,
        };
        Ok((config, options))
    }
}
