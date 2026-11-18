//! The configuration shared by license checking and release.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, de::Error};
use spdx::{Expression, Licensee};

/// The conventional location of the configuration file, relative to the root
/// of the workspace.
pub const CONFIG_PATH: &str = ".cargo/notices.toml";

/// The syntax in which licenses and license expressions are accepted, which is
/// the one `cargo-about` uses for its configuration.
const MODE: spdx::ParseMode = spdx::ParseMode {
    allow_deprecated: true,
    allow_imprecise_license_names: false,
    allow_postfix_plus_on_gpl: false,
    allow_slash_as_or_operator: false,
    allow_unknown: false,
};

/// Which licenses are acceptable in a dependency tree and how to treat
/// dependencies whose licenses cannot be determined automatically.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// The licenses that dependencies may use, ordered by priority.
    ///
    /// The most preferred license comes first. Checking only cares about the
    /// set of licenses, but for releases, the order decides which alternative
    /// of an `OR` expression is chosen.
    #[serde(deserialize_with = "licensees")]
    pub allow: Vec<Licensee>,
    /// The targets the dependencies are restricted to. All targets are
    /// considered if the list is empty.
    pub targets: Vec<String>,
    /// Whether to ignore build dependencies.
    pub ignore_build_dependencies: bool,
    /// Whether to ignore dev dependencies.
    pub ignore_dev_dependencies: bool,
    /// How to detect crates that are not published.
    pub private: Private,
    /// Licenses that are allowed for individual crates only.
    pub exceptions: Vec<Exception>,
    /// Licenses of crates that cannot be detected, by crate name.
    pub clarify: BTreeMap<String, Clarification>,
    /// Crates that are left out of release NOTICE files. Their licenses are
    /// still checked.
    pub omit: Vec<String>,
}

impl Config {
    /// Reads and parses the configuration file at the given path.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|err| format!("{}: {err}", path.display()))?;
        text.parse().map_err(|err| format!("{}: {err}", path.display()))
    }

    /// Finds the configuration file for the project with the given manifest.
    ///
    /// Looks for [`CONFIG_PATH`] in the manifest's directory and then in each
    /// parent directory. The search ends after the root of the workspace, i.e.
    /// the first directory whose `Cargo.toml` has a `[workspace]` table, so that
    /// the configuration of an unrelated enclosing project is never picked up.
    pub fn find(manifest_path: &Path) -> Option<PathBuf> {
        let manifest_path = std::path::absolute(manifest_path).ok()?;
        for dir in manifest_path.parent()?.ancestors() {
            let candidate = dir.join(CONFIG_PATH);
            if candidate.is_file() {
                return Some(candidate);
            }

            let manifest = std::fs::read_to_string(dir.join("Cargo.toml"));
            let table = manifest.ok().and_then(|text| text.parse::<toml::Table>().ok());
            if table.is_some_and(|table| table.contains_key("workspace")) {
                break;
            }
        }
        None
    }

    /// Replaces the targets, e.g. with ones given on the command line.
    pub fn set_targets(&mut self, targets: impl IntoIterator<Item = String>) {
        self.targets = targets.into_iter().collect();
    }
}

impl FromStr for Config {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let config: Self = toml::from_str(text).map_err(|err| err.to_string())?;
        if config.allow.is_empty() {
            return Err("`allow` must list at least one license".into());
        }
        Ok(config)
    }
}

/// Configures which crates are private and thus not checked.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Private {
    /// Whether to ignore crates that are not published or only published to
    /// one of the private registries.
    pub ignore: bool,
    /// The names of registries that crates are published to privately.
    pub registries: Vec<String>,
}

/// Allows additional licenses for all versions of a single crate.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exception {
    /// The name of the crate.
    #[serde(rename = "crate")]
    pub krate: String,
    /// The licenses that are allowed for the crate in addition to the global
    /// ones.
    #[serde(deserialize_with = "licensees")]
    pub allow: Vec<Licensee>,
}

/// States the license of a crate that cannot be detected reliably.
///
/// The clarification only applies as long as the checksums of its files match,
/// so it is not applied by accident if the crate's license changes. The format
/// and semantics are those of the same-named setting of `cargo-about`, except
/// that it applies to all versions of the crate.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clarification {
    /// The license expression of the crate, as if it was written in its
    /// `Cargo.toml`.
    #[serde(deserialize_with = "expression")]
    pub license: Expression,
    /// Files in the published crate that prove the license.
    pub files: Vec<ClarificationFile>,
}

/// A file whose checksum is the ground truth for a [`Clarification`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClarificationFile {
    /// The path of the file, relative to the crate.
    pub path: String,
    /// The SHA-256 checksum of the file's text, in hex.
    pub checksum: String,
    /// The license that applies to this file, if it differs from the
    /// clarification's.
    #[serde(default, deserialize_with = "optional_expression")]
    pub license: Option<Expression>,
    /// Restricts the checksummed text to start at the first occurrence of this
    /// text.
    pub start: Option<String>,
    /// Restricts the checksummed text to end after the first occurrence of
    /// this text following the start.
    pub end: Option<String>,
}

fn licensees<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<Licensee>, D::Error> {
    let strings = Vec::<String>::deserialize(de)?;
    strings
        .iter()
        .map(|text| Licensee::parse_mode(text, MODE).map_err(D::Error::custom))
        .collect()
}

fn expression<'de, D: Deserializer<'de>>(de: D) -> Result<Expression, D::Error> {
    let text = String::deserialize(de)?;
    Expression::parse_mode(&text, MODE).map_err(D::Error::custom)
}

fn optional_expression<'de, D: Deserializer<'de>>(
    de: D,
) -> Result<Option<Expression>, D::Error> {
    match Option::<String>::deserialize(de)? {
        Some(text) => Expression::parse_mode(&text, MODE)
            .map(Some)
            .map_err(D::Error::custom),
        None => Ok(None),
    }
}
