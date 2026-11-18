use std::fmt::{self, Display, Formatter};

use spdx::LicenseReq;

/// A license together with all items it applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LicenseBlock {
    /// The license that applies to the items.
    pub license: License,
    /// The items covered by the license.
    pub items: Vec<Item>,
    /// The verbatim license text, with the original line endings and without the
    /// linebreak that precedes the end marker.
    pub text: String,
    /// The verbatim ordinary notice that accompanies the items, if there is one,
    /// with the original line endings and without the linebreak that precedes the
    /// end marker.
    pub notice: Option<String>,
}

/// A license as named in a license marker.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct License {
    /// The SPDX license requirement: a single license identifier or license
    /// reference (e.g. `Apache-2.0` or `LicenseRef-LPPL-1.3`), optionally with an
    /// exception (e.g. `Apache-2.0 WITH LLVM-exception`).
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_req"))]
    pub req: LicenseReq,
    /// The full name of the license. Present if and only if the license is a
    /// `LicenseRef-` or `DocumentRef-` reference.
    pub name: Option<String>,
}

/// Serializes a license requirement as its SPDX expression.
#[cfg(feature = "serde")]
fn serialize_req<S: serde::Serializer>(
    req: &LicenseReq,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_str(req)
}

impl Display for License {
    /// Formats the license like it appears in the license marker.
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.req)?;
        if let Some(name) = &self.name {
            write!(f, " ({name})")?;
        }
        Ok(())
    }
}

/// An item that is covered by a license.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Item {
    /// A human-readable description. Continuation lines are joined by single
    /// spaces.
    pub description: String,
    /// The package that contains the item.
    pub package: Option<Package>,
    /// The paths that contain the item, without the enclosing backticks. They are
    /// relative to the package root if `package` is set and to the repository
    /// root otherwise. Empty if only a package is given.
    pub paths: Vec<String>,
    /// A URI for where the item was sourced.
    pub source: String,
    /// Copyright notices or attributions that need to be reproduced with the
    /// item. Wrapped lines are joined by single spaces.
    pub notice: Option<String>,
    /// Additional information on how the license applies to the item.
    pub also_see: Option<String>,
    /// In which distributions the item is included.
    pub inclusion: Inclusion,
}

/// A package from a package ecosystem.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Package {
    /// The name of the package, without the enclosing backticks.
    pub name: String,
    /// The version of the package.
    pub version: String,
    /// The package ecosystem, as a package type of the purl specification (e.g.
    /// `cargo`, `npm`, or `pypi`), without the enclosing parentheses.
    pub ecosystem: String,
}

impl Display for Package {
    /// Formats the package like it appears in a `Package` property.
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "`{}` {} ({})", self.name, self.version, self.ecosystem)
    }
}

/// In which kinds of distributions an item is included.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Inclusion {
    /// The item is included in all distributions.
    #[default]
    Always,
    /// The item is not included when the repository is used as a library.
    NotInLibrary,
    /// The item is not included when the repository is used as a binary.
    NotInBinary,
    /// The item is not included when the repository is used as a binary or as a
    /// library.
    NotInBinaryOrLibrary,
}
