//! A parser and validator for Typst NOTICE files.
//!
//! A Typst NOTICE file tracks items in a repository that are covered by
//! third-party licenses. The format is defined in `format.md`.
//!
//! ```
//! # let text = "";
//! match notices::parse(text) {
//!     Ok(blocks) => println!("{} license blocks", blocks.len()),
//!     Err(error) => eprintln!("{error}"),
//! }
//! ```

use std::io;
use std::path::Path;

#[cfg(feature = "serde")]
use serde::Serialize;

#[cfg(feature = "cargo")]
pub mod cargo;
mod merge;
mod model;
mod parser;
mod rules;
mod writer;

#[cfg(test)]
mod tests;

pub use self::merge::{Usage, merge, normalize_text, prune};
pub use self::model::{Inclusion, Item, License, LicenseBlock, Package};
pub use self::parser::{ErrorKind, ParseError};
pub use self::writer::WriteError;
pub use spdx::LicenseReq;

/// Parses and validates a Typst NOTICE file.
///
/// Returns [`ErrorKind::MissingHeader`] if the text does not start with the
/// Typst NOTICE header and [`ErrorKind::Malformed`] if it does but violates the
/// format.
pub fn parse(text: &str) -> Result<Vec<LicenseBlock>, ParseError> {
    parser::parse(text)
}

/// Writes license blocks as a Typst NOTICE file that [`parse`] accepts.
///
/// Wraps the file at 80 columns and chooses the marker length of each block so
/// that it does not clash with its texts; texts are written verbatim except
/// that CRLF linebreaks become LF. The file is parsed back and a [`WriteError`]
/// is returned if it is invalid or does not reproduce the blocks faithfully.
pub fn write(blocks: &[LicenseBlock]) -> Result<String, WriteError> {
    writer::write(blocks)
}

/// Reads a file that must be encoded in UTF-8, like Typst NOTICE files.
///
/// Unlike [`std::fs::read_to_string`], the error for invalid UTF-8 tells where
/// the invalid byte is.
pub fn read_text(path: impl AsRef<Path>) -> io::Result<String> {
    String::from_utf8(std::fs::read(path)?).map_err(|err| {
        let at = err.utf8_error().valid_up_to();
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("not valid UTF-8 (at byte {at})"),
        )
    })
}

/// The version of the JSON schema, see `json.md`. It is incremented on every
/// change that is not backwards compatible.
#[cfg(feature = "serde")]
pub const JSON_VERSION: u32 = 1;

/// The top-level JSON object.
#[cfg(feature = "serde")]
#[derive(Serialize)]
struct Document<'a> {
    version: u32,
    blocks: &'a [LicenseBlock],
}

/// Serializes license blocks as a JSON document, as specified in `json.md`.
///
/// The output is pretty-printed and ends with a linebreak.
#[cfg(feature = "serde")]
pub fn to_json(blocks: &[LicenseBlock]) -> String {
    let document = Document { version: JSON_VERSION, blocks };
    let mut json = serde_json::to_string_pretty(&document)
        .expect("the model only has string keys and serializes infallibly");
    json.push('\n');
    json
}
