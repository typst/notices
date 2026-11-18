//! Checks and releases the licenses of a Cargo dependency tree.
//!
//! The heavy lifting is done by `cargo-about`, which determines the licenses of
//! all dependencies and finds their texts. This module drives it from a single
//! [`Config`].

mod about;
mod collect;
mod config;

use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

pub use self::config::{
    CONFIG_PATH, Clarification, ClarificationFile, Config, Exception, Private,
};

use crate::{LicenseBlock, Usage};

/// Options for gathering the metadata of a Cargo project.
#[derive(Debug, Clone)]
pub struct MetadataOptions {
    /// The path to the `Cargo.toml` of the project.
    pub manifest_path: PathBuf,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    /// Whether to require an up-to-date `Cargo.lock`.
    pub locked: bool,
    /// Whether to forbid network access.
    pub offline: bool,
    /// Implies `locked` and `offline`.
    pub frozen: bool,
}

impl MetadataOptions {
    /// Creates options with default features and without lock restrictions.
    pub fn new(manifest_path: impl Into<PathBuf>) -> Self {
        Self {
            manifest_path: manifest_path.into(),
            features: Vec::new(),
            all_features: false,
            no_default_features: false,
            locked: false,
            offline: false,
            frozen: false,
        }
    }
}

// Problems found in a dependency tree.

/// The warnings and errors found when checking or releasing.
#[derive(Debug, Default)]
pub struct Report {
    pub problems: Vec<Problem>,
}

/// A warning or error with optional notes that explain it.
#[derive(Debug)]
pub struct Problem {
    pub severity: Severity,
    pub message: String,
    pub notes: Vec<String>,
}

/// How serious a [`Problem`] is.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Report {
    /// Whether at least one problem is an error.
    pub fn has_errors(&self) -> bool {
        self.count(Severity::Error) > 0
    }

    /// Whether there are neither errors nor warnings.
    pub fn is_empty(&self) -> bool {
        self.problems.is_empty()
    }

    /// The number of problems with the given severity.
    pub fn count(&self, severity: Severity) -> usize {
        self.problems
            .iter()
            .filter(|problem| problem.severity == severity)
            .count()
    }

    pub(in crate::cargo) fn error(&mut self, message: impl Into<String>) {
        self.add(Severity::Error, message, Vec::new());
    }

    pub(in crate::cargo) fn warning(&mut self, message: impl Into<String>) {
        self.add(Severity::Warning, message, Vec::new());
    }

    pub(in crate::cargo) fn add(
        &mut self,
        severity: Severity,
        message: impl Into<String>,
        notes: Vec<String>,
    ) {
        self.problems
            .push(Problem { severity, message: message.into(), notes });
    }
}

impl Display for Report {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        for problem in &self.problems {
            let kind = match problem.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            writeln!(f, "{kind}: {}", problem.message)?;
            for note in &problem.notes {
                writeln!(f, "  = {note}")?;
            }
        }
        Ok(())
    }
}

/// Checks that all dependencies of a Cargo project use allowed licenses.
///
/// This includes the licenses of the items in the Typst NOTICE files that
/// dependencies ship, which must be allowed globally or by an exception for the
/// dependency. Clarifications are verified first. Outdated ones are reported as
/// errors and have no effect on the check.
///
/// Problems with the dependencies are not an error of this function but
/// reported in the returned [`Report`]. The error case is reserved for
/// failures to gather the dependencies.
pub fn check(config: &Config, options: &MetadataOptions) -> anyhow::Result<Report> {
    let mut report = Report::default();
    for krate in collect::collect(config, options, &mut report)? {
        let exceptions: Vec<_> = config
            .exceptions
            .iter()
            .filter(|exception| exception.krate == krate.name)
            .flat_map(|exception| &exception.allow)
            .collect();

        for block in &krate.typst {
            let req = &block.license.req;
            let allowed = config.allow.iter().chain(exceptions.iter().copied());
            if allowed.into_iter().any(|licensee| licensee.satisfies(req)) {
                continue;
            }

            let mut notes = vec!["affected items:".to_owned()];
            notes.extend(
                block.items.iter().map(|item| format!("  - {}", item.description)),
            );
            report.add(
                Severity::Error,
                format!(
                    "license `{req}` in the NOTICE file of `{} {}` is not allowed",
                    krate.name, krate.version
                ),
                notes,
            );
        }
    }
    Ok(report)
}

/// Merges the licenses of a Cargo project and its dependencies for a release.
///
/// Combines the blocks of the repository's own NOTICE file, pruned for use as
/// a binary, with a block for each license of each dependency and the contents
/// of the Typst NOTICE files and ordinary NOTICE files of the dependencies.
/// Where a dependency offers a choice of licenses, the one listed first in
/// [`Config::allow`] is preferred. Blocks that can share a license text are
/// merged. Dependencies in [`Config::omit`] are left out.
///
/// This does not check the licenses of the Typst NOTICE files of dependencies,
/// see [`check`]. Dependencies from git repositories are errors, since their
/// licenses are not pinned to a published version.
///
/// The returned blocks can be written with [`crate::write`], which fails if
/// they cannot be represented in the format. If the report has errors, the
/// blocks are incomplete and must not be used.
///
/// Items of the Typst NOTICE files of dependencies that have no `Package` get
/// the dependency as their package. Their paths are then interpreted relative
/// to the package root, which equals the repository root for crates at the root
/// of their repository.
pub fn release(
    repo_notice: Option<Vec<LicenseBlock>>,
    config: &Config,
    options: &MetadataOptions,
) -> anyhow::Result<(Vec<LicenseBlock>, Report)> {
    let mut report = Report::default();
    let mut blocks = crate::prune(repo_notice.unwrap_or_default(), Usage::Binary);

    for krate in collect::collect(config, options, &mut report)? {
        if krate.git {
            report.error(format!(
                "`{} {}` is a git dependency, which cannot be released",
                krate.name, krate.version
            ));
        } else if !config.omit.contains(&krate.name) {
            blocks.extend(krate.blocks);
            blocks.extend(krate.typst);
        }
    }

    Ok((crate::merge(blocks), report))
}
