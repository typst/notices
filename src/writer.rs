use std::collections::HashSet;
use std::fmt::{self, Display, Formatter};

use crate::rules::{
    HEADER, INCLUSIONS, ITEM_START, MARKER_WORDS, MIN_MARKER_LEN, PROPERTY_STARTS, WIDTH,
};
use crate::{Item, LicenseBlock};

/// An error that occurred while writing a Typst NOTICE file: the license blocks
/// cannot be represented in the format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteError {
    /// A description of what is wrong and where.
    pub message: String,
}

impl Display for WriteError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for WriteError {}

/// Writes license blocks as a Typst NOTICE file and checks that they parse back
/// faithfully.
pub(crate) fn write(blocks: &[LicenseBlock]) -> Result<String, WriteError> {
    if blocks.is_empty() {
        return Err(WriteError {
            message: "a NOTICE file needs a license block".into(),
        });
    }

    let written: Vec<_> = blocks.iter().map(write_block).collect();
    let header = wrap(HEADER.split(' '), 0, 0);
    let text = format!("{header}\n\n{}\n", written.join("\n\n\n"));

    let parsed = crate::parser::parse(&text).map_err(|error| WriteError {
        message: format!(
            "the license blocks cannot be written as a valid NOTICE file: {error}"
        ),
    })?;

    for (i, (block, actual)) in blocks.iter().zip(&parsed).enumerate() {
        if *actual != lf_block(block) {
            return Err(WriteError {
                message: format!("license block {} cannot be written faithfully", i + 1),
            });
        }
    }
    if blocks.len() != parsed.len() {
        return Err(WriteError {
            message: format!(
                "license block {} cannot be written faithfully",
                blocks.len().min(parsed.len()) + 1
            ),
        });
    }

    Ok(text)
}

/// The block as the parser returns it: with LF instead of CRLF linebreaks in
/// its texts.
fn lf_block(block: &LicenseBlock) -> LicenseBlock {
    LicenseBlock {
        text: verbatim(&block.text),
        notice: block.notice.as_deref().map(verbatim),
        ..block.clone()
    }
}

/// Writes a license block without the linebreak after its end marker.
fn write_block(block: &LicenseBlock) -> String {
    let text = verbatim(&block.text);
    let notice = block.notice.as_deref().map(verbatim);

    let len = marker_len(&text, notice.as_deref());
    let items: Vec<_> = block.items.iter().map(write_item).collect();

    let mut out = format!(
        "{}\n{}\n\n{}\n{text}\n",
        "=".repeat(len),
        license_marker(block),
        items.join("\n"),
    );
    if let Some(notice) = notice {
        out.push_str(&format!("{}\n{notice}\n", "-".repeat(len)));
    }
    out.push_str(&"=".repeat(len));
    out
}

/// Converts a license or notice text to the written form: with LF linebreaks.
fn verbatim(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Finds the smallest marker length that does not occur as a line of the
/// license text or the notice text and that the format allows.
fn marker_len(text: &str, notice: Option<&str>) -> usize {
    let mut taken = HashSet::new();
    taken.extend(text.lines().filter_map(|line| marker_line_len(line, &['=', '-'])));
    taken.extend(
        notice
            .iter()
            .flat_map(|n| n.lines())
            .filter_map(|line| marker_line_len(line, &['='])),
    );
    let mut len = MIN_MARKER_LEN;
    while taken.contains(&len) {
        len += 1;
    }
    len
}

/// The length of the line if it consists of only one of the given characters.
fn marker_line_len(line: &str, chars: &[char]) -> Option<usize> {
    let first = line.chars().next().filter(|c| chars.contains(c))?;
    line.chars().all(|c| c == first).then(|| line.chars().count())
}

/// Writes the license marker, wrapped at spaces.
fn license_marker(block: &LicenseBlock) -> String {
    let req = block.license.req.to_string();
    let name = block.license.name.as_ref().map(|name| format!("({name})"));
    let words = req
        .split(' ')
        .chain(name.iter().flat_map(|name| name.split(' ')))
        .chain(MARKER_WORDS);
    wrap(words, 0, 0)
}

/// Writes an item, including the linebreak after its last line.
fn write_item(item: &Item) -> String {
    let mut out = format!("{ITEM_START}{}\n\n", wrap(words(&item.description), 2, 2));

    if let Some(package) = &item.package {
        // The name and the ecosystem must stay in one piece. The version cannot
        // contain spaces, so wrapping between the three parts is fine.
        let units = [
            format!("`{}`", package.name),
            package.version.clone(),
            format!("({})", package.ecosystem),
        ];
        property(&mut out, "Package", units);
    }

    if let Some((last, init)) = item.paths.split_last() {
        let key = if init.is_empty() { "Path" } else { "Paths" };
        // Paths wrap only after the comma that separates them.
        let units = init
            .iter()
            .map(|path| format!("`{path}`,"))
            .chain([format!("`{last}`")]);
        property(&mut out, key, units);
    }

    property(&mut out, "Source", [item.source.as_str()]);
    if let Some(notice) = &item.notice {
        property(&mut out, "Notice", words(notice));
    }
    if let Some(also_see) = &item.also_see {
        property(&mut out, "Also see", words(also_see));
    }

    if let Some((statement, _)) = INCLUSIONS.iter().find(|(_, i)| *i == item.inclusion) {
        out.push_str(&format!("  {statement}\n"));
    }
    out
}

/// Writes a property whose value consists of the given units, which are not
/// broken apart, including the linebreak at the end.
///
/// Continuation lines are aligned with the start of the value.
fn property<S: AsRef<str>>(
    out: &mut String,
    key: &str,
    units: impl IntoIterator<Item = S>,
) {
    let indent = 2 + key.len() + 2;
    out.push_str(&format!("  {key}: {}\n", wrap(units, indent, indent)));
}

/// Splits text into units at the spaces where it may wrap.
///
/// A space can only become a linebreak if it separates two words: otherwise the
/// linebreak would leave trailing whitespace or collapse neighboring spaces.
/// A line of a description must also not look like the start of the property
/// list.
fn words(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut units = Vec::new();
    let mut start = 0;
    for i in 1..bytes.len().saturating_sub(1) {
        let separates_words =
            bytes[i] == b' ' && bytes[i - 1] != b' ' && bytes[i + 1] != b' ';
        if separates_words
            && !PROPERTY_STARTS.iter().any(|p| text[i + 1..].starts_with(p))
        {
            units.push(&text[start..i]);
            start = i + 1;
        }
    }
    units.push(&text[start..]);
    units
}

/// Joins units with spaces and wraps them greedily at [`WIDTH`] columns.
///
/// `column` is the number of columns that the first line already occupies and
/// `indent` the number of spaces in front of continuation lines. A unit that
/// does not fit into a line on its own overflows.
fn wrap<S: AsRef<str>>(
    units: impl IntoIterator<Item = S>,
    mut column: usize,
    indent: usize,
) -> String {
    let mut out = String::new();
    for (i, unit) in units.into_iter().enumerate() {
        let unit = unit.as_ref();
        let len = unit.chars().count();
        if i > 0 && column + 1 + len <= WIDTH {
            out.push(' ');
            column += 1;
        } else if i > 0 {
            out.push('\n');
            out.push_str(&" ".repeat(indent));
            column = indent;
        }
        out.push_str(unit);
        column += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use spdx::{Expression, LicenseReq, ParseMode};

    use super::*;
    use crate::{Inclusion, License, Package, parse};

    const EXAMPLES: [(&str, &str); 4] = [
        ("CITATIONBERG", include_str!("../examples/CITATIONBERG-NOTICE")),
        ("HAYAGRIVA", include_str!("../examples/HAYAGRIVA-NOTICE")),
        ("PIXGLYPH", include_str!("../examples/PIXGLYPH-NOTICE")),
        ("TYPST", include_str!("../examples/TYPST-NOTICE")),
    ];

    fn req(text: &str) -> LicenseReq {
        let expression = Expression::parse_mode(text, ParseMode::STRICT).unwrap();
        expression.requirements().next().unwrap().req.clone()
    }

    fn item() -> Item {
        Item {
            description: "An item".into(),
            package: None,
            paths: vec!["a.rs".into()],
            source: "https://example.com/a".into(),
            notice: None,
            also_see: None,
            inclusion: Inclusion::Always,
        }
    }

    fn block() -> LicenseBlock {
        LicenseBlock {
            license: License { req: req("MIT"), name: None },
            items: vec![item()],
            text: "License text".into(),
            notice: None,
        }
    }

    /// Writes blocks that must be valid and checks that they parse back.
    #[track_caller]
    fn written(blocks: &[LicenseBlock]) -> String {
        let text = write(blocks).unwrap();
        assert_eq!(parse(&text).unwrap(), blocks, "{text}");
        text
    }

    #[track_caller]
    fn written_block(block: LicenseBlock) -> String {
        written(&[block])
    }

    #[track_caller]
    fn assert_error(blocks: &[LicenseBlock], message: &str) {
        let error = write(blocks).unwrap_err();
        assert!(error.message.contains(message), "unexpected message: {error}");
    }

    /// The part of a written file after the header.
    fn body(text: &str) -> &str {
        text.split_once("\n\n").unwrap().1
    }

    fn marker_lens(text: &str) -> Vec<usize> {
        text.lines()
            .filter(|l| l.starts_with(['=', '-']))
            .map(str::len)
            .collect()
    }

    #[test]
    fn test_examples_round_trip() {
        for (name, example) in EXAMPLES {
            let blocks = parse(example).unwrap();
            let text = write(&blocks).unwrap();
            assert_eq!(parse(&text).unwrap(), blocks, "{name}");
            assert_eq!(write(&parse(&text).unwrap()).unwrap(), text, "{name}");
        }
    }

    #[test]
    fn test_examples_are_wrapped_at_80_columns_except_texts() {
        for (name, example) in EXAMPLES {
            let text = write(&parse(example).unwrap()).unwrap();
            let blocks = parse(&text).unwrap();
            let verbatim: Vec<_> = blocks
                .iter()
                .flat_map(|b| {
                    b.text.lines().chain(b.notice.iter().flat_map(|n| n.lines()))
                })
                .collect();
            for line in text.lines().filter(|l| !verbatim.contains(l)) {
                // Only a URI can overflow, as it must not wrap.
                let overflow = line.starts_with("  Source: ");
                assert!(line.chars().count() <= WIDTH || overflow, "{name}: {line}");
            }
        }
    }

    #[test]
    fn test_layout() {
        let mut second = block();
        second.license.req = req("Apache-2.0 WITH LLVM-exception");
        second.notice = Some("Notice text".into());
        let bar = "=".repeat(80);
        let dashes = "-".repeat(80);

        let text = written(&[block(), second]);
        assert!(text.starts_with(
            "This NOTICE file compiles licenses and notices for code, data, and other\n"
        ));
        assert!(text.ends_with(&format!("{bar}\n")));
        let expected = format!(
            "{bar}\n\
             MIT --- This license applies to:\n\
             \n\
             * An item\n\
             \n\
             \x20 Path: `a.rs`\n\
             \x20 Source: https://example.com/a\n\
             \n\
             License text\n\
             {bar}\n\
             \n\
             \n\
             {bar}\n\
             Apache-2.0 WITH LLVM-exception --- This license applies to:\n\
             \n\
             * An item\n\
             \n\
             \x20 Path: `a.rs`\n\
             \x20 Source: https://example.com/a\n\
             \n\
             License text\n\
             {dashes}\n\
             Notice text\n\
             {bar}\n"
        );
        assert_eq!(body(&text), expected);
    }

    #[test]
    fn test_marker_length_avoids_text_lines() {
        let cases = [
            // (license text, notice, expected marker length)
            ("a\nb", None, 80),
            ("a\n--\n==", None, 80),
            ("a\n{=80}\nb", None, 81),
            ("a\n{-80}\nb", None, 81),
            ("a\n{=80}\n{-81}\n{=82}", None, 83),
            ("a\n{-81}", None, 80),
            ("a\n{=81}", None, 80),
            ("a\n{=80} \nb", None, 80),
            ("a", Some("b\n{=80}"), 81),
            // Dashes in a notice cannot be mistaken for a marker.
            ("a", Some("b\n{-80}"), 80),
            ("a\n{-80}", Some("b\n{=81}"), 82),
        ];
        for (text, notice, len) in cases {
            let expand = |s: &str| {
                s.replace("{=80}", &"=".repeat(80))
                    .replace("{=81}", &"=".repeat(81))
                    .replace("{=82}", &"=".repeat(82))
                    .replace("{-80}", &"-".repeat(80))
                    .replace("{-81}", &"-".repeat(81))
            };
            let mut block = block();
            block.text = expand(text);
            block.notice = notice.map(expand);

            let written = written_block(block);
            let lens = marker_lens(&written);
            assert!(lens.contains(&len), "{text:?}: {lens:?}");
            assert_eq!(*lens.last().unwrap(), len, "{text:?}");
        }
    }

    #[test]
    fn test_license_marker_wraps_at_spaces() {
        let mut block = block();
        block.license = License {
            req: req("LicenseRef-Long"),
            name: Some(
                "A rather long license name (with a parenthetical remark) that makes \
                 the marker long"
                    .into(),
            ),
        };
        let text = written_block(block);
        let marker: Vec<_> =
            body(&text).lines().skip(1).take_while(|l| !l.is_empty()).collect();
        assert_eq!(
            marker,
            [
                "LicenseRef-Long (A rather long license name (with a parenthetical remark) that",
                "makes the marker long) --- This license applies to:",
            ]
        );
    }

    #[test]
    fn test_description_wraps_with_indent_but_not_before_property_lookalike() {
        let mut first = block();
        first.items[0].description = "word ".repeat(30).trim_end().into();
        let text = written_block(first);
        let lines: Vec<_> =
            body(&text).lines().skip(3).take_while(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("* word") && lines[1].starts_with("  word"));
        assert!(lines[0].len() <= WIDTH && lines[0].len() > WIDTH - 5);

        let mut second = block();
        let filler = "x".repeat(78);
        second.items[0].description = format!("{filler} Path: here and `Paths: ` too");
        let text = written_block(second);
        assert!(text.contains(&format!("* {filler} Path:\n  here and")), "{text}");
    }

    #[test]
    fn test_package_and_path_properties() {
        let package = |name: &str| Package {
            name: name.into(),
            version: "1.2.3".into(),
            ecosystem: "cargo".into(),
        };
        let mut one = block();
        one.items[0].package = Some(package("foo"));
        one.items[0].paths.clear();
        assert!(written_block(one).contains("  Package: `foo` 1.2.3 (cargo)\n  Source:"));

        // Neither a long name nor a long path is broken.
        let name = "n".repeat(100);
        let mut two = block();
        two.items[0].package = Some(package(&name));
        let text = written_block(two);
        assert!(
            text.contains(&format!("  Package: `{name}`\n           1.2.3 (cargo)\n"))
        );
        assert!(text.contains("  Path: `a.rs`\n"));

        let long = "d/".repeat(60) + "f";
        let mut three = block();
        three.items[0].paths = vec!["a".into(), long.clone()];
        let text = written_block(three);
        assert!(text.contains(&format!("  Paths: `a`,\n         `{long}`\n")));
    }

    #[test]
    fn test_long_source_does_not_break() {
        let source = format!("https://example.com/{}", "a".repeat(100));
        let mut block = block();
        block.items[0].source = source.clone();
        block.items[0].also_see = Some(source.clone());
        let text = written_block(block);
        assert!(text.contains(&format!("  Source: {source}\n")));
        assert!(text.contains(&format!("  Also see: {source}\n")));
    }

    #[test]
    fn test_long_paths_wrap_after_commas() {
        let paths: Vec<String> =
            (0..8).map(|i| format!("some/directory/file{i}.rs")).collect();
        let mut block = block();
        block.items[0].paths = paths.clone();
        let text = written_block(block);
        let value: Vec<_> = text
            .lines()
            .skip_while(|l| !l.starts_with("  Paths:"))
            .take_while(|l| !l.starts_with("  Source"))
            .collect();
        assert!(value.len() > 1);
        for line in &value {
            assert!(line.chars().count() <= WIDTH, "{line}");
        }
        for line in &value[..value.len() - 1] {
            assert!(line.ends_with(','), "{line}");
        }
        for line in &value[1..] {
            assert!(line.starts_with("         `some"), "{line}");
        }
    }

    #[test]
    fn test_notice_and_also_see_wrap_with_alignment() {
        let mut block = block();
        block.items[0].notice = Some("Copyright word ".repeat(10).trim_end().into());
        block.items[0].also_see = Some("See also word ".repeat(10).trim_end().into());
        let text = written_block(block);
        let lines: Vec<_> = text.lines().collect();
        let start = |key: &str| lines.iter().position(|l| l.starts_with(key)).unwrap();
        let continuation = |key: &str, indent: usize| {
            let line = lines[start(key) + 1];
            line.starts_with(&" ".repeat(indent)) && !line[indent..].starts_with(' ')
        };
        assert!(continuation("  Notice: ", 10));
        assert!(continuation("  Also see: ", 12));
        for line in lines.iter().filter(|l| l.starts_with(' ')) {
            assert!(line.chars().count() <= WIDTH, "{line}");
        }
    }

    #[test]
    fn test_inclusion_statements() {
        for (inclusion, statement) in [
            (Inclusion::NotInLibrary, "Not included when used as a library"),
            (Inclusion::NotInBinary, "Not included when used as a binary"),
            (
                Inclusion::NotInBinaryOrLibrary,
                "Not included when used as a binary or a library",
            ),
        ] {
            let mut block = block();
            block.items[0].inclusion = inclusion;
            assert!(written_block(block).contains(&format!("\n  {statement}\n\n")));
        }
    }

    #[test]
    fn test_crlf_becomes_lf() {
        let mut crlf = block();
        crlf.text = "one\r\n\r\n\ttwo \r\nthree".into();
        crlf.notice = Some("n1\r\nn2".into());
        let text = write(&[crlf.clone()]).unwrap();
        assert!(!text.contains('\r'));

        crlf.text = crlf.text.replace("\r\n", "\n");
        crlf.notice = crlf.notice.map(|n| n.replace("\r\n", "\n"));
        assert_eq!(parse(&text).unwrap(), [crlf]);

        // Whitespace and double spaces elsewhere survive.
        let mut other = block();
        other.text = "  indented \n\n\n\ttab\ntrailing   \n".into();
        other.items[0].description = format!("{}  {}", "a".repeat(40), "b".repeat(40));
        other.items[0].notice = Some("x  y".into());
        written_block(other);
    }

    fn with_item(f: impl FnOnce(&mut Item)) -> [LicenseBlock; 1] {
        let mut block = block();
        f(&mut block.items[0]);
        [block]
    }

    fn with_block(f: impl FnOnce(&mut LicenseBlock)) -> [LicenseBlock; 1] {
        let mut block = block();
        f(&mut block);
        [block]
    }

    #[test]
    fn test_errors() {
        // A description with a linebreak.
        assert_error(&with_item(|i| i.description = "a\nb".into()), "cannot be written");
        // An item without a package or paths.
        assert_error(&with_item(|i| i.paths.clear()), "cannot be written");
        // A path with a backtick.
        assert_error(&with_item(|i| i.paths = vec!["a`b".into()]), "cannot be written");
        // A license reference without a name.
        assert_error(
            &with_block(|b| b.license.req = req("LicenseRef-x")),
            "cannot be written",
        );
        // A license text that starts like an item.
        assert_error(&with_block(|b| b.text = "* a\nb".into()), "cannot be written");

        // The description contains a property lookalike that would parse as a
        // real property list instead of as the description: it must error
        // rather than silently change.
        assert_error(
            &with_item(|i| {
                i.description =
                    "first\n\n  Path: `fake`\n  Source: https://fake.example".into()
            }),
            "faithfully",
        );

        // Not a single block.
        assert_error(&[], "needs a license block");
    }
}
