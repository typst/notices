//! The constants of the format used by both the parser and the writer and the
//! value rules that the parser enforces.

use spdx::{Expression, LicenseReq, ParseMode};

use crate::Inclusion;

/// The column at which lines are wrapped.
pub(crate) const WIDTH: usize = 80;

/// The start of an item.
pub(crate) const ITEM_START: &str = "* ";

/// The starts of the lines that begin a property list.
pub(crate) const PROPERTY_STARTS: [&str; 3] = ["Package: ", "Path: ", "Paths: "];

/// The text every Typst NOTICE file starts with, with all linebreaks replaced by
/// spaces.
pub(crate) const HEADER: &str = "This NOTICE file compiles licenses and notices for \
    code, data, and other contents of this software that originate from third parties. \
    In a source repository, it covers contents not tracked by the dependency tree or \
    other machine-readable manifests; accompanying a distribution, it can also cover \
    the dependencies included in it. It is machine-readable and follows the format set \
    out in https://github.com/typst/notice (version 1). Tools that can parse this file \
    can use it to incorporate it into license notices, Software Bills of Materials \
    (SBOMs), etc of their distributions of this software, provided the contents within \
    are represented faithfully. Other distributions must reproduce this file verbatim.";

/// The minimum number of `=` characters in a start marker.
pub(crate) const MIN_MARKER_LEN: usize = 80;

/// The words of the fixed part of the license marker.
pub(crate) const MARKER_WORDS: [&str; 5] = ["---", "This", "license", "applies", "to:"];

/// The statements that can end a property list, and what they mean.
pub(crate) const INCLUSIONS: [(&str, Inclusion); 3] = [
    ("Not included when used as a library", Inclusion::NotInLibrary),
    ("Not included when used as a binary", Inclusion::NotInBinary),
    ("Not included when used as a binary or a library", Inclusion::NotInBinaryOrLibrary),
];

/// The package types of the purl specification, sorted, that are allowed as
/// package ecosystems.
pub(crate) const ECOSYSTEMS: &[&str] = &[
    "alpm",
    "apk",
    "bitbucket",
    "bitnami",
    "cargo",
    "cocoapods",
    "composer",
    "conan",
    "conda",
    "cpan",
    "cran",
    "deb",
    "docker",
    "gem",
    "generic",
    "github",
    "golang",
    "hackage",
    "hex",
    "huggingface",
    "julia",
    "luarocks",
    "maven",
    "mlflow",
    "npm",
    "nuget",
    "oci",
    "otp",
    "pub",
    "pypi",
    "qpkg",
    "rpm",
    "swid",
    "swift",
    "vscode-extension",
    "yocto",
];

const COMPOUND_EXPRESSION: &str = "compound SPDX expressions are not allowed; use a \
    single license identifier, optionally followed by `WITH` and an exception";

/// A violation of a rule, located within the checked value.
#[derive(Debug)]
pub(crate) struct Violation {
    /// The byte offset in the value at which the rule is violated.
    pub offset: usize,
    pub message: String,
}

impl Violation {
    fn new(offset: usize, message: impl Into<String>) -> Self {
        Self { offset, message: message.into() }
    }
}

type Check = Result<(), Violation>;

/// The kinds of verbatim text in a license block.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum Verbatim {
    License,
    Notice,
}

impl Verbatim {
    /// A description of the text.
    pub fn name(self) -> &'static str {
        match self {
            Self::License => "license text",
            Self::Notice => "notice text",
        }
    }
}

/// Checks the first line of a verbatim text.
pub(crate) fn verbatim_start(kind: Verbatim, first_line: &str) -> Result<(), String> {
    if first_line.trim().is_empty() {
        return Err(format!("the {} must start with a non-whitespace line", kind.name()));
    }
    if kind == Verbatim::License && first_line.starts_with(ITEM_START) {
        return Err(format!(
            "the license text must not start with `{ITEM_START}`, which would start \
             another item"
        ));
    }
    Ok(())
}

/// Parses the SPDX expression of a license marker, which must be a single
/// license requirement.
pub(crate) fn license_req(text: &str) -> Result<LicenseReq, Violation> {
    if let Some(i) = text.find(['(', ')']) {
        return Err(Violation::new(i, COMPOUND_EXPRESSION));
    }

    let expression = Expression::parse_mode(text, ParseMode::STRICT).map_err(|e| {
        let term = &text[e.span.clone()];
        Violation::new(e.span.start, format!("{}: `{term}`", e.reason))
    })?;

    let mut reqs = expression.requirements();
    match (reqs.next(), reqs.next()) {
        (Some(only), None) => Ok(only.req.clone()),
        (_, Some(second)) => {
            Err(Violation::new(second.span.start as usize, COMPOUND_EXPRESSION))
        }
        (None, None) => Err(Violation::new(0, COMPOUND_EXPRESSION)),
    }
}

/// Checks a package name, written enclosed in backticks.
pub(crate) fn package_name(name: &str) -> Check {
    if name.is_empty() {
        return Err(Violation::new(0, "the package name must not be empty"));
    }
    match name.find(|c: char| c.is_whitespace() || c == '`') {
        Some(i) if name[i..].starts_with('`') => {
            Err(Violation::new(i, "the package name must not contain backticks"))
        }
        Some(i) => Err(Violation::new(i, "the package name must not contain whitespace")),
        None => Ok(()),
    }
}

/// Checks a package version.
pub(crate) fn package_version(version: &str) -> Check {
    if version.is_empty() {
        return Err(Violation::new(0, "the package version must not be empty"));
    }
    match version.find(char::is_whitespace) {
        Some(i) => {
            Err(Violation::new(i, "the package version must not contain whitespace"))
        }
        None => Ok(()),
    }
}

/// Checks a package ecosystem, written enclosed in parentheses.
pub(crate) fn ecosystem(ecosystem: &str) -> Check {
    if ecosystem.is_empty() {
        return Err(Violation::new(0, "the package ecosystem must not be empty"));
    }
    if ECOSYSTEMS.binary_search(&ecosystem).is_err() {
        return Err(Violation::new(
            0,
            format!(
                "unknown package ecosystem `{ecosystem}`; expected a package type of \
                 the purl specification, like `cargo`, `npm`, or `pypi`"
            ),
        ));
    }
    Ok(())
}

/// Checks a path, written enclosed in backticks.
pub(crate) fn path(path: &str) -> Check {
    if path.is_empty() {
        return Err(Violation::new(0, "the path must not be empty"));
    }
    if path.starts_with('/') {
        return Err(Violation::new(
            0,
            "paths must be relative and must not start with a slash",
        ));
    }
    match path.find(['`', '\t', '\r', '\n']) {
        Some(i) if path[i..].starts_with('\t') => {
            Err(Violation::new(i, "tabs are only allowed within license texts"))
        }
        Some(i) => {
            Err(Violation::new(i, "a path must not contain backticks or linebreaks"))
        }
        None => Ok(()),
    }
}

/// Checks the URI of a `Source` property.
pub(crate) fn source(uri: &str) -> Check {
    if let Some(i) = uri.find(char::is_whitespace) {
        return Err(Violation::new(i, "a URI must not contain whitespace"));
    }
    if !has_scheme(uri) {
        return Err(Violation::new(
            0,
            "expected a URI starting with a scheme, like `https://`",
        ));
    }
    Ok(())
}

/// Whether the URI starts with a valid scheme followed by a colon.
pub(crate) fn has_scheme(uri: &str) -> bool {
    let Some((scheme, _)) = uri.split_once(':') else { return false };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
}
