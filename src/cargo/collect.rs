//! Collects the license blocks for the dependencies of a Cargo project.

use spdx::LicenseItem;

use super::about::{self, CrateLicenses, LicenseText};
use super::{Config, MetadataOptions, Report, Severity};
use crate::merge::is_apache;
use crate::rules::has_scheme;
use crate::{ErrorKind, Inclusion, Item, License, LicenseBlock, Package, Usage};

/// The files that can contain an ordinary NOTICE, in order of preference.
///
/// Only the first can be a Typst NOTICE file.
const NOTICE_FILES: [&str; 3] = ["NOTICE", "NOTICE.txt", "NOTICE.md"];

/// The license blocks of a dependency.
pub(super) struct Collected {
    pub name: String,
    pub version: String,
    /// The blocks for the crate's licenses, with an ordinary notice attached.
    pub blocks: Vec<LicenseBlock>,
    /// The blocks of the crate's Typst NOTICE file, pruned for library use.
    pub typst: Vec<LicenseBlock>,
}

/// Collects the license blocks of all dependencies, ordered by crate name and
/// version.
///
/// Workspace members are not included, since their license is the project's
/// own.
pub(super) fn collect(
    config: &Config,
    options: &MetadataOptions,
    report: &mut Report,
) -> anyhow::Result<Vec<Collected>> {
    let crates = about::resolve(config, options, report)?;
    Ok(crates
        .iter()
        .map(|krate| {
            let mut collector = Collector { krate, report: &mut *report };
            let (blocks, typst) = collector.blocks();
            Collected {
                name: krate.name.clone(),
                version: krate.version.to_string(),
                blocks,
                typst,
            }
        })
        .collect())
}

/// Collects the blocks of a single crate.
struct Collector<'a> {
    krate: &'a CrateLicenses,
    report: &'a mut Report,
}

impl Collector<'_> {
    /// The blocks for the crate's licenses with an ordinary notice attached,
    /// and the blocks of its Typst NOTICE file.
    fn blocks(&mut self) -> (Vec<LicenseBlock>, Vec<LicenseBlock>) {
        let mut blocks: Vec<_> = self
            .krate
            .licenses
            .iter()
            .map(|license| self.license_block(license))
            .collect();

        let (notice, typst) = self.notices();
        if let Some(notice) = notice {
            // Apache 2.0 requires carrying along the notice of the work.
            let index = blocks
                .iter()
                .position(|block| is_apache(&block.license.req))
                .unwrap_or(0);
            match blocks.get_mut(index) {
                Some(block) => block.notice = Some(notice),
                None => {
                    self.warn("its NOTICE file is dropped as it has no license block")
                }
            }
        }

        (blocks, typst)
    }

    /// Creates the block for one of the crate's licenses.
    fn license_block(&mut self, license: &LicenseText) -> LicenseBlock {
        let krate = self.krate;

        // The format requires a name for license references. The identifier is
        // the best we can offer.
        let name = match license.license.license {
            LicenseItem::Other(_) => {
                let name = license.license.license.to_string();
                self.warn(format!(
                    "the license `{name}` has no name, using the identifier"
                ));
                Some(name)
            }
            LicenseItem::Spdx { .. } => None,
        };

        // Not every repository URL is a valid URI.
        let source = krate
            .repository
            .as_deref()
            .filter(|url| has_scheme(url) && !url.contains(char::is_whitespace))
            .map_or_else(
                || format!("https://crates.io/crates/{}", krate.name),
                str::to_owned,
            );

        LicenseBlock {
            license: License { req: license.license.clone(), name },
            items: vec![Item {
                description: format!("The `{}` crate", krate.name),
                package: Some(self.package()),
                paths: Vec::new(),
                source,
                notice: None,
                also_see: None,
                inclusion: Inclusion::Always,
            }],
            text: license.text.clone(),
            notice: None,
        }
    }

    /// Reads the NOTICE files of the crate.
    ///
    /// Returns the text of the ordinary notice to include, if any, and the
    /// blocks of the Typst NOTICE file, pruned for library use.
    fn notices(&mut self) -> (Option<String>, Vec<LicenseBlock>) {
        let label = format!("{} {}", self.krate.name, self.krate.version);
        let mut ordinary = Vec::new();
        let mut typst = Vec::new();

        for (i, name) in NOTICE_FILES.into_iter().enumerate() {
            let path = self.krate.manifest_dir.join(name);
            if !path.is_file() {
                continue;
            }

            let notes = vec![format!("NOTICE file: {}", path.display())];
            let text = match crate::read_text(&path) {
                Ok(text) => text,
                Err(err) => {
                    let message =
                        format!("failed to read the NOTICE file of `{label}`: {err}");
                    self.report.add(Severity::Error, message, notes);
                    continue;
                }
            };

            if i == 0 {
                match crate::parse(&text) {
                    Ok(blocks) => {
                        typst = self.with_package(blocks);
                        continue;
                    }
                    Err(err) if err.kind == ErrorKind::MissingHeader => {}
                    Err(err) => {
                        let message =
                            format!("malformed Typst NOTICE file of `{label}`: {err}");
                        self.report.add(Severity::Error, message, notes);
                        continue;
                    }
                }
            }
            ordinary.push((name, text));
        }

        if let Some(((first, _), rest)) = ordinary.split_first()
            && !rest.is_empty()
        {
            let ignored: Vec<_> = rest.iter().map(|(name, _)| *name).collect();
            self.warn(format!(
                "it has multiple ordinary NOTICE files, using `{first}` and ignoring {}",
                ignored.join(", ")
            ));
        }

        (ordinary.into_iter().next().map(|(_, text)| text), typst)
    }

    /// Prunes the blocks of a dependency's Typst NOTICE file and makes their
    /// items refer to the dependency.
    ///
    /// Items without a package get the dependency, so their paths are read
    /// relative to the package root. That equals the repository root for crates
    /// at the root of their repository.
    fn with_package(&self, blocks: Vec<LicenseBlock>) -> Vec<LicenseBlock> {
        let mut blocks = crate::prune(blocks, Usage::Library);
        for item in blocks.iter_mut().flat_map(|block| &mut block.items) {
            item.package.get_or_insert_with(|| self.package());
        }
        blocks
    }

    /// The package of the crate.
    fn package(&self) -> Package {
        Package {
            name: self.krate.name.clone(),
            version: self.krate.version.to_string(),
            ecosystem: "cargo".into(),
        }
    }

    /// Reports a warning about the crate.
    fn warn(&mut self, message: impl std::fmt::Display) {
        let krate = self.krate;
        self.report
            .warning(format!("`{} {}`: {message}", krate.name, krate.version));
    }
}
