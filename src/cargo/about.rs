//! License resolution with `cargo-about`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use cargo_about::licenses::config::{
    Clarification as AboutClarification, ClarificationFile as AboutFile,
    Config as AboutConfig, KrateConfig, Private,
};
use cargo_about::licenses::resolution::{self, Files, Severity as AboutSeverity};
use cargo_about::licenses::{
    Gatherer, KrateLicense, LicenseFileKind, LicenseSource, store_from_cache,
};
use krates::semver::Version;
use spdx::{AdditionItem, LicenseItem, LicenseReq};
use toml_span::Spanned;
use unscanny::Scanner;

use super::{
    Clarification, ClarificationFile, Config, MetadataOptions, Report, Severity,
};

/// The confidence from which license texts are recognized as a license.
const CONFIDENCE_THRESHOLD: f32 = 0.8;

/// The licenses that apply to a dependency.
pub(super) struct CrateLicenses {
    pub name: String,
    pub version: Version,
    /// The URL of the crate's source repository, if it declares one.
    pub repository: Option<String>,
    /// The directory containing the crate's `Cargo.toml`.
    pub manifest_dir: PathBuf,
    /// The licenses the crate is used under.
    ///
    /// If the crate offers a choice of licenses, only the most preferred allowed
    /// one is included. If it requires multiple licenses, all of them are.
    pub licenses: Vec<LicenseText>,
}

/// A license that applies to a crate along with its text.
///
/// A license appears multiple times for a crate if its text differs between
/// the files of the crate, e.g. because of different copyright holders.
pub(super) struct LicenseText {
    pub license: LicenseReq,
    /// The license text. This is the text found in the crate or, if there is
    /// none, the canonical text of the license.
    pub text: String,
}

/// Resolves the licenses of all dependencies, ordered by name and version.
///
/// Dependencies with errors and workspace members are missing from the result.
pub(super) fn resolve(
    config: &Config,
    options: &MetadataOptions,
    report: &mut Report,
) -> anyhow::Result<Vec<CrateLicenses>> {
    let mut about_config = about_config(config);

    let manifest_path = krates::Utf8Path::from_path(&options.manifest_path)
        .context("the manifest path is not valid UTF-8")?;
    let krates = cargo_about::get_all_crates(
        manifest_path,
        options.no_default_features,
        options.all_features,
        options.features.clone(),
        // Only the dependencies of the root package, not of the whole workspace.
        false,
        krates::LockOptions {
            frozen: options.frozen,
            locked: options.locked,
            offline: options.offline,
        },
        &about_config,
        &[],
    )
    .context("failed to gather the crate graph")?;

    let verified = verify(config, &krates, report);
    about_config.crates = krate_configs(config, &verified);

    let store = Arc::new(store_from_cache()?);
    let gathered = Gatherer::with_store(store)
        .with_confidence_threshold(CONFIDENCE_THRESHOLD)
        .gather(&krates, &about_config, None);

    let krate_configs: BTreeMap<_, _> = about_config
        .crates
        .into_iter()
        .map(|(name, krate)| (name, krate.value))
        .collect();
    let mut files = Files::new();
    let resolved = resolution::resolve(
        &gathered,
        &about_config.accepted,
        &krate_configs,
        &mut files,
        true,
    );

    let members: BTreeSet<_> = krates
        .workspace_members()
        .filter_map(|node| match node {
            krates::Node::Krate { id, .. } => Some(id.repr.as_str()),
            _ => None,
        })
        .collect();

    let mut crates = Vec::new();
    for (krate_license, resolved) in gathered.iter().zip(resolved) {
        // Private crates are ignored.
        let Some(resolved) = resolved else { continue };
        let krate = krate_license.krate;

        let mut failed = false;
        for diagnostic in resolved.diagnostics {
            let severity = match diagnostic.severity {
                AboutSeverity::Bug | AboutSeverity::Error => Severity::Error,
                AboutSeverity::Warning => Severity::Warning,
                _ => continue,
            };
            failed |= severity == Severity::Error;
            // The labels point into manifests and have no message if they mark
            // a license that is not allowed.
            let labels = diagnostic.labels.into_iter().map(|label| {
                match label.message.is_empty() {
                    true => files.source(label.file_id)[label.range].to_owned(),
                    false => label.message,
                }
            });
            let notes = labels.chain(diagnostic.notes).collect();
            report.add(severity, format!("`{krate}`: {}", diagnostic.message), notes);
        }
        if failed {
            continue;
        }

        // `cargo-about` only logs a warning if it cannot minimize the licenses,
        // which would silently drop the crate from the result.
        if resolved.licenses.is_empty() {
            report.error(format!("failed to determine the licenses of `{krate}`"));
            continue;
        }

        let mut licenses = Vec::new();
        for license in resolved.licenses {
            let texts = license_texts(krate_license, &license);
            if texts.is_empty() {
                report.warning(format!(
                    "no text found for license `{license}` of `{krate}`"
                ));
            }
            licenses.extend(
                texts
                    .into_iter()
                    .map(|text| LicenseText { license: license.clone(), text }),
            );
        }

        if members.contains(krate.id.repr.as_str()) {
            continue;
        }

        let manifest_dir = krate.manifest_path.parent().unwrap_or(&krate.manifest_path);
        crates.push(CrateLicenses {
            name: krate.name.clone(),
            version: krate.version.clone(),
            repository: krate.repository.clone(),
            manifest_dir: manifest_dir.as_std_path().to_owned(),
            licenses,
        });
    }

    crates.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    Ok(crates)
}

/// Translates the global part of the configuration into the one of
/// `cargo-about`.
///
/// Workarounds for popular crates that `cargo-about` ships are not enabled.
fn about_config(config: &Config) -> AboutConfig {
    AboutConfig {
        targets: config.targets.clone(),
        private: Private {
            ignore: config.private.ignore,
            registries: config.private.registries.clone(),
        },
        ignore_build_dependencies: config.ignore_build_dependencies,
        ignore_dev_dependencies: config.ignore_dev_dependencies,
        accepted: config.allow.clone(),
        ..AboutConfig::default()
    }
}

/// Translates the per-crate part of the configuration into the one of
/// `cargo-about`: the exceptions and the verified clarifications.
fn krate_configs(
    config: &Config,
    verified: &[(&str, &Clarification)],
) -> BTreeMap<String, Spanned<KrateConfig>> {
    let mut krates: BTreeMap<String, Spanned<KrateConfig>> = BTreeMap::new();
    fn entry<'a>(
        krates: &'a mut BTreeMap<String, Spanned<KrateConfig>>,
        name: &str,
    ) -> &'a mut KrateConfig {
        let empty = Spanned::new(KrateConfig { accepted: Vec::new(), clarify: None });
        &mut krates.entry(name.to_owned()).or_insert(empty).value
    }

    for exception in &config.exceptions {
        entry(&mut krates, &exception.krate)
            .accepted
            .extend(exception.allow.iter().cloned());
    }

    for (name, clarification) in verified {
        let files = clarification
            .files
            .iter()
            .map(|file| AboutFile {
                path: file.path.clone().into(),
                checksum: file.checksum.clone(),
                license: file.license.clone(),
                start: file.start.clone(),
                end: file.end.clone(),
            })
            .collect();
        entry(&mut krates, name).clarify = Some(AboutClarification {
            license: clarification.license.clone(),
            override_git_commit: None,
            files,
            git: Vec::new(),
        });
    }

    krates
}

/// Finds the texts of a license in a crate.
///
/// Falls back to the canonical text for licenses with an SPDX identifier. For a
/// license with an exception, texts that include the exception are preferred.
/// Otherwise, the canonical text of the exception is appended.
fn license_texts(krate_license: &KrateLicense, req: &LicenseReq) -> Vec<String> {
    let mut texts: Vec<String> = Vec::new();

    for file in &krate_license.license_files {
        let (LicenseFileKind::Text(text) | LicenseFileKind::AddendumText(text, _)) =
            &file.kind
        else {
            continue;
        };

        if !applies(&file.license, &req.license) {
            continue;
        }
        let stripped = strip_comment_markers(text);
        if !texts.contains(stripped.as_ref().unwrap_or(text)) {
            texts.push(stripped.unwrap_or_else(|| text.clone()));
        }
    }

    if texts.is_empty()
        && let Some(id) = req.license.id()
    {
        texts.push(id.text().to_owned());
    }

    let Some(exception) = req.addition.as_ref().and_then(AdditionItem::id) else {
        return texts;
    };
    let (mut with, without): (Vec<_>, Vec<_>) =
        texts.into_iter().partition(|text| includes(text, exception.text()));
    if with.is_empty() {
        with = without
            .into_iter()
            .map(|text| format!("{}\n\n{}", text.trim_end(), exception.text()))
            .collect();
    }
    with
}

/// Removes the line comment markers from a license text that is entirely
/// commented out, e.g. a license file or a source file header whose every line
/// starts with `// ` or `//! `.
///
/// Returns `None` if any other line occurs or if nothing but blank lines
/// remains.
fn strip_comment_markers(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut has_content = false;
    for line in text.lines() {
        let mut s = Scanner::new(line);
        if s.eat_if("//") {
            s.eat_if('!');
            if !s.done() && !s.eat_if(' ') {
                return None;
            }
            has_content |= !s.after().trim().is_empty();
            out.push_str(s.after());
        } else if !line.trim().is_empty() {
            return None;
        }
        out.push('\n');
    }
    has_content.then_some(out)
}

/// Whether the text contains the needle, ignoring differences in whitespace.
fn includes(text: &str, needle: &str) -> bool {
    let words = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    words(text).contains(&words(needle))
}

/// Whether a file with the given license contains the text of a license.
fn applies(source: &LicenseSource, license: &LicenseItem) -> bool {
    match source {
        LicenseSource::Detected(id) => license.id() == Some(*id),
        LicenseSource::Clarified(expr) => {
            expr.requirements().any(|req| same_license(&req.req.license, license))
        }
    }
}

/// Whether two licenses are the same, ignoring a trailing `+`.
fn same_license(a: &LicenseItem, b: &LicenseItem) -> bool {
    match a.id() {
        Some(id) => b.id() == Some(id),
        None => a == b,
    }
}

// Verifies that clarifications still describe the crates they are meant for.

/// Verifies the clarifications against the crates in the dependency tree.
///
/// Returns the clarifications that hold for all versions of their crate that
/// they are meant for. Outdated ones are reported as errors.
///
/// `cargo-about` only logs a warning if a checksum does not match and then
/// silently falls back to another license text. To make this an error, the
/// clarifications are verified here and only the verified ones are handed to
/// `cargo-about`.
pub(super) fn verify<'a>(
    config: &'a Config,
    krates: &cargo_about::Krates,
    report: &mut Report,
) -> Vec<(&'a str, &'a Clarification)> {
    let mut verified = Vec::new();

    for (name, clarification) in &config.clarify {
        let mut holds = true;
        for krate in krates.krates().filter(|krate| {
            krate.name == *name && clarification.applies_to(&krate.version)
        }) {
            let dir = krate.manifest_path.parent().unwrap_or(&krate.manifest_path);
            for file in &clarification.files {
                let result = std::fs::read_to_string(dir.join(&file.path))
                    .map_err(|err| anyhow::anyhow!("unable to read the file: {err}"))
                    .and_then(|text| check_text(&text, file));
                if let Err(reason) = result {
                    report.add(
                        Severity::Error,
                        format!(
                            "clarification for `{name} {}` is outdated: {}: {reason:#}",
                            krate.version, file.path
                        ),
                        vec![
                            "the license may have changed, review it and update the \
                             clarification"
                                .into(),
                        ],
                    );
                    holds = false;
                }
            }
        }

        if holds {
            verified.push((name.as_str(), clarification));
        }
    }

    verified
}

/// Checks the text of a clarification file against its checksum.
///
/// If the file has markers, only the text from the first occurrence of the start
/// marker through the first occurrence of the end marker after it is checked.
/// This is how `cargo-about` selects the text, too.
fn check_text(contents: &str, file: &ClarificationFile) -> anyhow::Result<()> {
    anyhow::ensure!(!contents.is_empty(), "the file is empty");

    let start = match &file.start {
        Some(marker) => contents.find(marker).with_context(|| {
            format!("failed to find the text starting with `{marker}`")
        })?,
        None => 0,
    };

    let end = match &file.end {
        Some(marker) => {
            let offset = contents[start..].find(marker).with_context(|| {
                format!("failed to find the text ending with `{marker}`")
            })?;
            start + offset + marker.len()
        }
        None => contents.len(),
    };

    cargo_about::validate_sha256(&contents[start..end], &file.checksum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_comment_markers() {
        let strip = strip_comment_markers;
        assert_eq!(
            strip("// Copyright A\n//\n//   indented\r\n\n//! doc\n//!\n").as_deref(),
            Some("Copyright A\n\n  indented\n\ndoc\n\n"),
        );

        // Texts with other lines, other markers, or a marker without a space are
        // not stripped.
        assert_eq!(strip("MIT License\n// not a comment\n"), None);
        assert_eq!(strip("  // indented\n"), None);
        assert_eq!(strip("/// doc\n"), None);
        assert_eq!(strip("//text\n"), None);
        assert_eq!(strip("# a\n# b\n"), None);
        assert_eq!(strip("/* a */\n"), None);

        // Nothing remains.
        assert_eq!(strip("//\n//!\n\n"), None);
        assert_eq!(strip(""), None);
    }

    #[test]
    fn test_same_license_ignores_or_later() {
        let parse = |s: &str| {
            spdx::Expression::parse(s)
                .unwrap()
                .requirements()
                .next()
                .unwrap()
                .req
                .license
                .clone()
        };
        assert!(same_license(&parse("Apache-2.0"), &parse("Apache-2.0+")));
        assert!(!same_license(&parse("Apache-2.0"), &parse("MIT")));
        assert!(same_license(&parse("LicenseRef-a"), &parse("LicenseRef-a")));
        assert!(!same_license(&parse("LicenseRef-a"), &parse("LicenseRef-b")));
        assert!(!same_license(&parse("LicenseRef-a"), &parse("MIT")));
    }

    const TEXT: &str = "header\nBEGIN\nlicense body\nEND\ntrailer\n";
    const TEXT_CHECKSUM: &str =
        "be0fc49f6dfb25ddd6cf62a30f6c647c39357bccdbf9786cefbb6aeee9149dc1";
    const SECTION_CHECKSUM: &str =
        "8c84ba51a8c7b4afced5013f68793cafab7fb0f7a88dbc3f4185e731a72710b9";

    fn file(checksum: &str, start: Option<&str>, end: Option<&str>) -> ClarificationFile {
        ClarificationFile {
            path: "LICENSE".into(),
            checksum: checksum.into(),
            license: None,
            start: start.map(str::to_owned),
            end: end.map(str::to_owned),
        }
    }

    fn error(contents: &str, file: &ClarificationFile) -> String {
        format!("{:#}", check_text(contents, file).unwrap_err())
    }

    #[test]
    fn test_check_text_whole_file() {
        check_text(TEXT, &file(TEXT_CHECKSUM, None, None)).unwrap();
        let mismatch = error(TEXT, &file(SECTION_CHECKSUM, None, None));
        assert!(mismatch.contains("checksum mismatch"));
        assert!(error("", &file(TEXT_CHECKSUM, None, None)).contains("empty"));
    }

    #[test]
    fn test_check_text_between_markers() {
        let section = file(SECTION_CHECKSUM, Some("BEGIN"), Some("END"));
        check_text(TEXT, &section).unwrap();
        // The end marker is searched for after the start marker.
        let early_end = "END\nheader\nBEGIN\nlicense body\nEND\n";
        check_text(early_end, &section).unwrap();

        // Both markers are part of the checked text.
        for file in [
            file(SECTION_CHECKSUM, Some("BEGIN"), None),
            file(SECTION_CHECKSUM, None, Some("END")),
        ] {
            assert!(error(TEXT, &file).contains("checksum mismatch"));
        }
    }

    #[test]
    fn test_check_text_missing_marker() {
        let missing_start = file(TEXT_CHECKSUM, Some("MISSING"), None);
        assert!(error(TEXT, &missing_start).contains("starting with `MISSING`"));
        let missing_end = file(TEXT_CHECKSUM, None, Some("MISSING"));
        assert!(error(TEXT, &missing_end).contains("ending with `MISSING`"));
        // The end marker only counts after the start marker.
        let before_start = file(TEXT_CHECKSUM, Some("trailer"), Some("header"));
        assert!(error(TEXT, &before_start).contains("ending with `header`"));
    }
}
