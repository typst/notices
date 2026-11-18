//! Tests for the individual rules of the format.
//!
//! Error cases are table-driven: `(text, at, message)`, where `at` is the snippet at
//! which the error is reported (empty for the end of the text, `ANY` to skip the check).

use crate::rules::{ECOSYSTEMS, HEADER};
use crate::{ErrorKind, Inclusion, LicenseBlock, Package, ParseError, parse};

/// A line of the minimum number of `=` characters.
const BAR: &str =
    "================================================================================";

const MARKER: &str = "MIT --- This license applies to:";
const ITEM: &str = "* An item\n\n  Path: `a.rs`\n  Source: https://example.com/a\n";
const TEXT: &str = "License text.\n\n\tIndented with a tab, trailing space \nLast line.";
const ANY: &str = "?";

fn dashes() -> String {
    "-".repeat(BAR.len())
}

fn block(marker: &str, items: &[&str], text: &str) -> String {
    format!("{BAR}\n{marker}\n\n{}\n{text}\n{BAR}\n", items.join("\n"))
}

fn block_with_notice(marker: &str, text: &str, notice: &str) -> String {
    format!("{BAR}\n{marker}\n\n{ITEM}\n{text}\n{}\n{notice}\n{BAR}\n", dashes())
}

fn file(blocks: &[String]) -> String {
    format!("{HEADER}\n\n{}", blocks.join("\n\n"))
}

fn valid() -> String {
    file(&[block(MARKER, &[ITEM], TEXT)])
}

fn with_marker(marker: &str) -> String {
    file(&[block(marker, &[ITEM], TEXT)])
}

fn with_text(text: &str) -> String {
    file(&[block(MARKER, &[ITEM], text)])
}

fn with_notice(notice: &str) -> String {
    file(&[block_with_notice(MARKER, TEXT, notice)])
}

fn item(description: &str, properties: &str) -> String {
    file(&[block(MARKER, &[&format!("* {description}\n\n{properties}")], TEXT)])
}

fn props(properties: &str) -> String {
    item("An item", properties)
}

fn tail(tail: &str) -> String {
    props(&format!("  Path: `a`\n  Source: x:y\n{tail}"))
}

fn with_package(value: &str) -> String {
    props(&format!("  Package: {value}\n  Source: x:y\n"))
}

#[track_caller]
fn parse_single(text: &str) -> LicenseBlock {
    let mut blocks = parse(text).unwrap();
    assert_eq!(blocks.len(), 1);
    blocks.remove(0)
}

fn parse_error(text: &str) -> ParseError {
    parse(text).unwrap_err()
}

fn position(text: &str, index: usize) -> (usize, usize) {
    let before = &text[..index];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
    (line, column)
}

#[track_caller]
fn assert_error_at(text: &str, index: usize, message: &str) {
    let error = parse(text).unwrap_err();
    assert_eq!(error.index, index, "{error}\n{text:?}");
    assert_eq!(position(text, index), (error.line, error.column), "{error}");
    assert_malformed(text, message);
}

#[track_caller]
fn assert_malformed(text: &str, message: &str) {
    let error = parse(text).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Malformed, "{error}\n{text:?}");
    assert!(error.message.contains(message), "unexpected message: {error}\n{text:?}");
}

#[track_caller]
fn check(cases: &[(String, &str, &str)]) {
    for (text, at, message) in cases {
        if *at == ANY {
            assert_malformed(text, message);
            continue;
        }
        let from = text.find(BAR).unwrap_or(0);
        let index = if at.is_empty() {
            text.len()
        } else {
            from + text[from..].find(at).unwrap()
        };
        assert_error_at(text, index, message);
    }
}

#[test]
fn test_valid_file() {
    let block = parse_single(&valid());
    assert_eq!(block.license.req.to_string(), "MIT");
    assert_eq!(block.license.name, None);
    assert_eq!(block.text, TEXT);
    assert_eq!(block.notice, None);

    let item = &block.items[0];
    assert_eq!(item.description, "An item");
    assert_eq!(item.package, None);
    assert_eq!(item.paths, ["a.rs"]);
    assert_eq!(item.source, "https://example.com/a");
    assert_eq!(item.notice, None);
    assert_eq!(item.also_see, None);
    assert_eq!(item.inclusion, Inclusion::Always);
}

#[test]
fn test_error_position() {
    let text = valid().replace("Path: `a.rs`", "Pat: `a.rs`");
    let error = parse_error(&text);
    assert_eq!(&text[error.index..][..11], "Pat: `a.rs`");
    assert_eq!((error.line, error.column), position(&text, error.index));
    assert!(
        error
            .to_string()
            .starts_with(&format!("{}:{}: ", error.line, error.column))
    );
}

mod line_endings {
    use super::*;

    #[test]
    fn test_crlf() {
        let block = parse_single(&valid().replace('\n', "\r\n"));
        assert_eq!(block.text, TEXT.replace('\n', "\r\n"));
        assert_eq!(block.items[0].paths, ["a.rs"]);

        let block = parse_single(&with_notice("Line 1\nLine 2").replace('\n', "\r\n"));
        assert_eq!(block.notice.as_deref(), Some("Line 1\r\nLine 2"));
    }

    #[test]
    #[rustfmt::skip]
    fn test_inconsistent() {
        let crlf = valid().replace('\n', "\r\n");
        let notice = with_notice("Line 1\nLine 2").replace('\n', "\r\n");
        let msg = "inconsistent linebreak";
        check(&[
            (crlf.replace("`a.rs`\r\n", "`a.rs`\n"), "\n  Source", msg),
            (crlf.replace("Last line", "Last\nline"), "\nline", msg),
            (notice.replace("Line 1\r\n", "Line 1\n"), "\nLine 2", msg),
            (valid().replace("`a.rs`\n", "`a.rs`\r"), "\r  Source", msg),
            (valid().replacen("An item\n", "An item\r\n", 1), "\r\n\n  Path", msg),
        ]);
    }
}

mod header {
    use super::*;

    #[test]
    fn test_fully_wrapped_header_crlf() {
        let wrapped = HEADER.replace(' ', "\r\n");
        let text = valid().replace('\n', "\r\n").replacen(HEADER, &wrapped, 1);
        assert_eq!(parse(&text).unwrap().len(), 1);
    }

    #[test]
    fn test_missing_header() {
        let other = valid().replacen(HEADER, "This NOTICE file compiles licenses.", 1);
        for text in ["", "Copyright (c) Someone\n\nSome license.\n", &other] {
            assert_eq!(parse_error(text).kind, ErrorKind::MissingHeader, "{text:?}");
        }
        let error = parse_error("");
        assert_eq!((error.line, error.column), (1, 1));
    }

    #[test]
    #[rustfmt::skip]
    fn test_structure() {
        let two = HEADER.len() + 1;
        assert_error_at(&valid().replacen("verbatim.\n\n", "verbatim.\n", 1), two, "found `=`");
        assert_error_at(&valid().replacen("verbatim.\n\n", "verbatim.\n\n\n", 1), two + 1, "a start marker");
        assert_error_at(&valid().replacen(BAR, &BAR[1..], 1), two + 1, "79 `=` characters, but at least 80");
        check(&[
            (format!("{HEADER}\n\n"), "", "start marker"),
            (valid().replacen("verbatim.\n", "verbatim. \n", 1), ANY, "trailing whitespace"),
        ]);
    }
}

mod marker {
    use super::*;

    #[test]
    #[rustfmt::skip]
    fn test_licenses() {
        let m = |marker: &str| parse_single(&with_marker(&format!("{marker} --- This license applies to:")));
        let block = m("Apache-2.0 WITH LLVM-exception");
        assert_eq!(block.license.req.to_string(), "Apache-2.0 WITH LLVM-exception");
        assert_eq!(block.license.req.addition.unwrap().id(), spdx::exception_id("LLVM-exception"));
        assert_eq!(m("Apache-2.0+").license.to_string(), "Apache-2.0+");

        let block = m("LicenseRef-X (A (nested (deeply)) name)");
        assert_eq!(block.license.name.as_deref(), Some("A (nested (deeply)) name"));
        assert_eq!(block.license.to_string(), "LicenseRef-X (A (nested (deeply)) name)");
        let block = m("DocumentRef-spdx-tool-1.2:LicenseRef-MyLicense (My License)");
        assert_eq!(block.license.req.to_string(), "DocumentRef-spdx-tool-1.2:LicenseRef-MyLicense");
    }

    #[test]
    fn test_wrapped_everywhere() {
        let block = parse_single(&with_marker(
            "LicenseRef-LPPL-1.3\n(LaTeX\nProject\nLicense\n1.3)\n---\nThis\nlicense\napplies\nto:",
        ));
        assert_eq!(block.license.name.as_deref(), Some("LaTeX Project License 1.3"));
    }

    #[test]
    #[rustfmt::skip]
    fn test_errors() {
        let m = |marker: &str| with_marker(&format!("{marker} --- This license applies to:"));
        let compound = "compound SPDX expressions are not allowed";
        let single = "single space";
        check(&[
            (m("NotALicense-1.0"), "NotALicense-1.0", "unknown term"),
            (m("Apache-2.0\nWITH\nNotAnException"), "NotAnException", "unknown term"),
            (m("MIT AND Apache-2.0"), "Apache-2.0", compound),
            (m("(MIT)"), "(MIT", compound),
            (m("MIT (The MIT License)"), "(The", "only `LicenseRef-` and `DocumentRef-`"),
            (m("LicenseRef-X"), "---", "expected the license name in parentheses"),
            (m("LicenseRef-X ()"), "()", "must not be empty"),
            (m("LicenseRef-X (Name (nested)"), "\n* An", "empty lines are not allowed"),
            (m("LicenseRef-X ( Name)"), " Name", "must not start with a space"),
            (m("LicenseRef-X (Name )"), ")", "must not end with a space"),
            (m("LicenseRef-X (My\tName)"), "\tName", "tabs are not allowed"),
            (m("MIT "), " ---", single),
            (with_marker("MIT ---\n\nThis license applies to:"), "\nThis", single),
            (with_marker("MIT --- This license applies to"), "to\n\n* An", "expected `to:`"),
            (with_marker("MIT --- This licence applies to:"), "licence", "expected `license`"),
            (with_marker("MIT --- This license applies to: "), " \n\n", "trailing whitespace"),
            (with_marker("MIT\t--- This license applies to:"), "\t---", "expected a space or linebreak, found a tab"),
            (valid().replacen("to:\n\n", "to:\n", 1), "* An item", "found `*`"),
            (valid().replacen("to:\n\n", "to:\n\n\n", 1), "\n* An item", "expected an item"),
        ]);
    }
}

mod items {
    use super::*;

    #[test]
    fn test_multiple_items_and_continuation_lines() {
        let first =
            "* First line\n  second line\n  third line\n\n  Path: `a`\n  Source: x:y\n";
        let second = "* Other\n\n  Path: `b`\n  Source: x:z\n";
        let block = parse_single(&file(&[block(MARKER, &[first, second], TEXT)]));
        assert_eq!(block.items.len(), 2);
        assert_eq!(block.items[0].description, "First line second line third line");
        assert_eq!(block.items[1].description, "Other");
        assert_eq!(block.text, TEXT);
    }

    #[test]
    fn test_description_line_that_looks_like_a_property() {
        let text =
            item("An item\n  Package: is a word here", "  Path: `a`\n  Source: x:y\n");
        let block = parse_single(&text);
        assert_eq!(block.items[0].description, "An item Package: is a word here");
    }

    #[test]
    #[rustfmt::skip]
    fn test_errors() {
        let props = "  Path: `a`\n  Source: x:y\n";
        let non_ws = "must start with a non-whitespace line";
        check(&[
            (item("An item\n\n  more", props), "more", "expected a property"),
            (item("An item\n   more", props), " more", "unexpected extra space"),
            (item("An item\n more", props), " more", "continuation line starting with two spaces"),
            (item("An item ", props), " \n", "trailing whitespace"),
            (item("An\titem", props), "\titem", "tabs are only allowed within license texts"),
            (item(" An item", props), " An", "unexpected extra space"),
            (item("", props), "\n\n  Path", "expected text"),
            (valid().replacen("item\n\n", "item\n", 1), "License text", "empty line between the description"),
            (valid().replacen("/a\n\nLicense", "/a\nLicense", 1), "License text", "expected a property"),
            (with_text("* Not an item\nLicense text"), "License text", "must not start with `* `"),
            (valid().replacen("/a\n\nLicense", "/a\n\n\nLicense", 1), "\nLicense", "found a second empty line"),
            (with_text("   \nText"), "   \nText", non_ws),
        ]);
    }
}

mod properties {
    use super::*;

    #[test]
    #[rustfmt::skip]
    fn test_all_properties() {
        let text = props("  Paths: `a/`, `b c.txt`\n  Source: https://example.com\n  Notice: Copyright 1\n  Also see: here\n");
        let item = parse_single(&text).items.remove(0);
        assert_eq!((item.paths, item.notice.as_deref()), (vec!["a/".into(), "b c.txt".into()], Some("Copyright 1")));
        assert_eq!((item.also_see.as_deref(), item.inclusion), (Some("here"), Inclusion::Always));

        for (statement, inclusion) in [
            ("library", Inclusion::NotInLibrary),
            ("binary", Inclusion::NotInBinary),
            ("binary or a library", Inclusion::NotInBinaryOrLibrary),
        ] {
            let text = tail(&format!("  Not included when used as a {statement}\n"));
            assert_eq!(parse_single(&text).items[0].inclusion, inclusion);
        }
    }

    #[test]
    #[rustfmt::skip]
    fn test_wrapped_values() {
        let text = props("  Paths: `a`, `b`,\n         `c`\n  Source: x:y\n  Notice: One two\n          three\n  Also see: five\n            six\n");
        let item = parse_single(&text).items.remove(0);
        assert_eq!(item.paths, ["a", "b", "c"]);
        assert_eq!(item.notice.as_deref(), Some("One two three"));
        assert_eq!(item.also_see.as_deref(), Some("five six"));
    }

    #[test]
    #[rustfmt::skip]
    fn test_value_errors() {
        let path = |p: &str| props(&format!("{p}  Source: x:y\n"));
        let src = |s: &str| props(&format!("  Path: `a`\n  Source: {s}\n"));
        let indent = "indentation of 10 spaces";
        check(&[
            (tail("  Notice: One\n         two\n"), "two", indent),
            (props("  Paths: `a`, `b\n         c`\n  Source: x:y\n"), "\n         c`", "paths must not wrap"),
            (src("https://example.com/\n          more"), "          more", "must not wrap"),
            (src("https://example.com/a b"), " b\n", "must not contain whitespace"),
            (src("example.com/a"), "example.com", "scheme"),
            (path("  Path: `a`, `b`\n"), ", `b`", "`Path` takes exactly one path"),
            (path("  Path: `/abs`\n"), "/abs", "must not start with a slash"),
            (path("  Path: ``\n"), "`\n", "must not be empty"),
            (path("  Path: a\n"), "a\n", "expected a path enclosed in backticks"),
            (path("  Path: `a\n"), "\n  Source: x:y\n\n", "closing backtick"),
            (path("  Paths: `a`,`b`\n"), "`b`", "expected a space or linebreak after the comma"),
            (path("  Paths: `a\tb`\n"), "\tb", "tabs are only allowed"),
        ]);
    }

    #[test]
    #[rustfmt::skip]
    fn test_structure_errors() {
        let missing_source = "missing required property `Source`";
        check(&[
            (props("  Source: x:y\n  Path: `a`\n"), "Path", "out of order"),
            (props("  Path: `a`\n  Package: `a` 1 (cargo)\n  Source: x:y\n"), "Package", "out of order"),
            (props("  Path: `a`\n  Notice: n\n  Source: x:y\n"), "Source", "out of order"),
            (tail("  Source: x:z\n"), "Source: x:z", "duplicate property `Source`"),
            (props("  Path: `a`\n  Paths: `a`\n  Source: x:y\n"), "Paths", "duplicate property `Paths`"),
            (tail("  License: MIT\n"), "License", "unknown property `License`"),
            (tail("  Also See: x\n"), "Also See", "unknown property"),
            (props("  Source: x:y\n"), "  Source", "at least one of `Package` and `Path` or `Paths`"),
            (props("  Package: `a` 1 (cargo)\n"), "  Package", missing_source),
            (props("  Path: `a`\n  Notice: x\n"), "  Path", missing_source),
            (tail("  Not included when used as a plugin\n"), "Not included", "expected a property"),
            (tail("  Not included when used as a library\n  Notice: x\n"), "  Notice", "empty line after the inclusion"),
            (props("   Path: `a`\n  Source: x:y\n"), " Path", "exactly two spaces"),
            (props("Path: `a`\n  Source: x:y\n"), "Path", "indented with two spaces"),
            (props("  Path: `a` \n  Source: x:y\n"), " \n  Source", "trailing whitespace"),
            (tail("  Notice: a\tb\n"), "\tb", "tabs are only allowed"),
        ]);
    }
}

mod blocks {
    use super::*;

    const SECOND: &str = "Apache-2.0 --- This license applies to:";

    #[test]
    fn test_two_blocks() {
        let first = block_with_notice(MARKER, TEXT, "N");
        let blocks = parse(&file(&[first, block(SECOND, &[ITEM], "Other")])).unwrap();
        let summary =
            |b: &LicenseBlock| (b.license.to_string(), b.text.clone(), b.notice.clone());
        assert_eq!(summary(&blocks[0]), ("MIT".into(), TEXT.into(), Some("N".into())));
        assert_eq!(summary(&blocks[1]), ("Apache-2.0".into(), "Other".into(), None));
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    #[rustfmt::skip]
    fn test_separation_and_ending() {
        let second = block(SECOND, &[ITEM], "Other");
        let next = format!("{BAR}\nApache");
        let separated = "separated by two empty lines";
        let no_final = valid().strip_suffix('\n').unwrap().to_string();
        check(&[
            (format!("{}\n{second}", valid()), &next, separated),
            (format!("{}{second}", valid()), &next, separated),
            (file(&[block(MARKER, &[ITEM], TEXT), format!("\n{second}")]), &format!("\n{next}"), "expected a start marker"),
            (no_final, "", "must end with a linebreak"),
            (valid() + "more\n", "more", "expected a linebreak"),
        ]);
        let text = valid() + "\n";
        assert_error_at(&text, text.len() - 1, "exactly one linebreak");
    }

    #[test]
    fn test_markers_of_other_lengths() {
        // Longer markers work, with and without notice.
        let bar = "=".repeat(90);
        assert_eq!(parse_single(&valid().replace(BAR, &bar)).text, TEXT);
        let text = with_notice("N")
            .replace(BAR, &bar)
            .replace(&dashes(), &"-".repeat(90));
        assert_eq!(parse_single(&text).notice.as_deref(), Some("N"));

        // Lines of `=` or `-` of a different length than the markers belong to the text.
        let text =
            format!("A\n{}\n{BAR}=\n{}\n{}-\nB", &BAR[1..], &dashes()[1..], dashes());
        let block = parse_single(&with_text(&text));
        assert_eq!((block.text, block.notice), (text, None));

        // The text is verbatim, and may even start and end with `=` lines.
        for text in [
            "  Indented\n\n\n\tTabbed \n\ntrailing   ",
            &format!("{0}\nTitle\n{0}", &BAR[2..]),
        ] {
            assert_eq!(parse_single(&with_text(text)).text, text);
        }
    }

    #[test]
    #[rustfmt::skip]
    fn test_marker_errors() {
        let base = format!("{HEADER}\n\n{BAR}\n{MARKER}\n\n{ITEM}\n");
        let short_end = valid()[..valid().len() - BAR.len() - 1].to_string() + &BAR[1..] + "\n";
        let long_end = (valid() + "x").replacen(&format!("{BAR}\nx"), &format!("{BAR}=\nx"), 1);
        check(&[
            (valid().replacen(BAR, "-----", 1), ANY, "start marker"),
            (valid().replacen(BAR, &format!("{BAR} "), 1), " \nMIT", "trailing whitespace"),
            (short_end, "", "missing end marker"),
            (long_end, "", "missing end marker"),
            (format!("{base}{TEXT}\n"), "", "missing end marker"),
            (format!("{base}{TEXT}"), "", "missing end marker"),
        ]);
        let text = format!("{base}{BAR}\n");
        assert_error_at(&text, text.rfind(BAR).unwrap(), "license text is empty");
    }
}

mod packages {
    use super::*;

    #[test]
    #[rustfmt::skip]
    fn test_package() {
        let text = props("  Package: `foo` 1.2.3 (cargo)\n  Path: `a`\n  Source: x:y\n");
        let item = parse_single(&text).items.remove(0);
        let package = item.package.unwrap();
        assert_eq!(package, Package { name: "foo".into(), version: "1.2.3".into(), ecosystem: "cargo".into() });
        assert_eq!(item.paths, ["a"]);
        assert_eq!(package.to_string(), "`foo` 1.2.3 (cargo)");

        // A package alone is enough, and unusual values round-trip.
        for value in ["`@scope/foo` 1.0.0-beta+7 (npm)", "`a.b` v2 (vscode-extension)"] {
            let item = parse_single(&with_package(value)).items.remove(0);
            assert_eq!(item.package.unwrap().to_string(), value);
            assert!(item.paths.is_empty());
        }
    }

    #[test]
    fn test_wrapped_package() {
        // The value starts at column 11 (after `  Package: `).
        let indent = " ".repeat(11);
        let wrapped = with_package(&format!("`foo`\n{indent}1.2.3\n{indent}(cargo)"));
        for text in [wrapped.clone(), wrapped.replace('\n', "\r\n")] {
            let package = parse_single(&text).items[0].package.clone().unwrap();
            assert_eq!(package.to_string(), "`foo` 1.2.3 (cargo)");
        }
    }

    #[test]
    fn test_ecosystems() {
        assert!(ECOSYSTEMS.is_sorted());
        for e in ECOSYSTEMS {
            let package = parse_single(&with_package(&format!("`foo` 1 ({e})")))
                .items
                .remove(0)
                .package;
            assert_eq!(&package.unwrap().ecosystem, e);
        }
    }

    #[test]
    #[rustfmt::skip]
    fn test_errors() {
        let p = with_package;
        let end = "\n  Source";
        let single = "single space";
        let indent = "indentation of 11 spaces";
        let ecosystem = "expected the package ecosystem";
        let name = "expected the package name enclosed in backticks";
        let no_ws = "must not contain whitespace";
        let no_ticks = "must not contain backticks";
        let dup = "  Package: `a` 1 (cargo)\n";
        check(&[
            (p("`foo`\n          1.2.3 (cargo)"), "1.2.3", indent),
            (p("`foo` 1.2.3\n            (cargo)"), "(cargo)", indent),
            (p("`foo` 1.2.3\n(cargo)"), "\n(cargo)", ecosystem),
            (p("`foo`\n\n           1 (cargo)"), "\n\n           1", "expected the package version"),
            (p("`foo` 1 (cargo)\n            more"), "          more", "exactly two spaces"),
            (p("`foo`  1 (cargo)"), " 1 (", single),
            (p(""), end, name),
            (p("foo 1 (cargo)"), "foo", name),
            (p("`foo 1 (cargo)"), end, "expected a closing backtick"),
            (p("`` 1 (cargo)"), "` 1", "the package name must not be empty"),
            (p("`foo bar` 1 (cargo)"), " bar", no_ws),
            (p("`foo`bar` 1 (cargo)"), "bar", no_ticks),
            (p("`foo`"), end, "expected the package version"),
            (p("`foo` 1"), end, ecosystem),
            (p("`foo` 1 cargo"), "cargo", "ecosystem in parentheses"),
            (p("`foo` 1 (cargo"), end, "expected a closing parenthesis"),
            (p("`foo` 1 (cargo) more"), " more", "found a space"),
            (p("`foo` 1 (Cargo)"), "Cargo", "unknown package ecosystem `Cargo`"),
            (p("`foo` 1 ()"), ")\n", "must not be empty"),
            (props(&format!("{dup}{dup}  Source: x:y\n")), "Package: `a` 1 (cargo)\n  Source", "duplicate property `Package`"),
            (file(&[block(MARKER, &["* An item\n  Package: `a` 1 (cargo)\n"], TEXT)]), "License text", "empty line between the description"),
        ]);
    }
}

mod notices {
    use super::*;

    #[test]
    fn test_notice() {
        let notice = "Copyright 2024 Someone.\nSecond line.";
        let block = parse_single(&with_notice(notice));
        assert_eq!(block.text, TEXT);
        assert_eq!(block.notice.as_deref(), Some(notice));

        let notice = "  Indented\n\n\n\tTabbed \n\ntrailing   ";
        assert_eq!(parse_single(&with_notice(notice)).notice.as_deref(), Some(notice));
    }

    #[test]
    fn test_markers_inside_notice() {
        // A second notice marker, short dashes and `=` lines belong to the notice.
        let notice =
            format!("One\n{}\nTwo\n{}\n{}\n{BAR}=", dashes(), "-".repeat(10), &BAR[1..]);
        let block = parse_single(&with_notice(&notice));
        assert_eq!(
            (block.text.as_str(), block.notice.as_deref()),
            (TEXT, Some(notice.as_str()))
        );
    }

    #[test]
    #[rustfmt::skip]
    fn test_errors() {
        let base = format!("{HEADER}\n\n{BAR}\n{MARKER}\n\n{ITEM}\n");
        let dashes = dashes();
        let blank = "the notice text must start with a non-whitespace";
        let last = format!("Last line.\n{BAR}\n");
        let end = valid().replacen(&last, &format!("Last line.\n{dashes}\nN\n"), 1);
        let at_eof = valid().replacen(&last, &format!("Last line.\n{dashes}"), 1);
        check(&[
            (with_notice(""), "\n=", "must start with a non-whitespace line"),
            (with_notice("   \nNotice"), "   \nNotice", blank),
            (with_notice("\nNotice"), "\nNotice\n", blank),
            (end, "", "after the notice text"),
            (at_eof, "", "expected a linebreak"),
        ]);

        let text = format!("{base}T\n{dashes}\n{BAR}\n");
        assert_error_at(&text, text.rfind(BAR).unwrap(), "the notice text is empty");
        let text = format!("{base}{dashes}\nN\n{BAR}\n");
        assert_error_at(&text, text.find(&dashes).unwrap(), "the license text is empty");
    }
}
