use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use spdx::{LicenseItem, LicenseReq};

use crate::rules::WIDTH;
use crate::{Inclusion, LicenseBlock};

/// The line that ends the terms of the Apache 2.0 License.
const APACHE_END: &str = "END OF TERMS AND CONDITIONS";

/// The start of the heading of the Apache 2.0 License's appendix.
const APACHE_APPENDIX: &str = "APPENDIX: How to apply the Apache License";

/// The last line of the Apache 2.0 License's appendix.
const APACHE_APPENDIX_END: &str = "limitations under the License.";

/// The last line of the instructions in the Apache 2.0 License's appendix. Some
/// copies end there and leave out the boilerplate notice that follows.
const APACHE_APPENDIX_INSTRUCTIONS_END: &str = "within third-party archives.";

/// How a repository is used by a downstream consumer.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Usage {
    /// The consumer distributes a binary built from the repository.
    Binary,
    /// The consumer depends on the repository as a library.
    Library,
}

impl Usage {
    /// Whether an item with the given inclusion statement is present.
    fn includes(self, inclusion: Inclusion) -> bool {
        !matches!(
            (self, inclusion),
            (Self::Binary, Inclusion::NotInBinary | Inclusion::NotInBinaryOrLibrary)
                | (
                    Self::Library,
                    Inclusion::NotInLibrary | Inclusion::NotInBinaryOrLibrary
                )
        )
    }
}

/// Removes the items that are not present in the given usage and the license
/// blocks that are left without items.
pub fn prune(
    blocks: impl IntoIterator<Item = LicenseBlock>,
    usage: Usage,
) -> Vec<LicenseBlock> {
    blocks
        .into_iter()
        .filter_map(|mut block| {
            block.items.retain(|item| usage.includes(item.inclusion));
            (!block.items.is_empty()).then_some(block)
        })
        .collect()
}

/// Brings a license text into a canonical form.
///
/// - Linebreaks become LF.
/// - Empty lines at the start and whitespace at the end are removed.
/// - The appendix of the Apache 2.0 License is removed if the text ends with
///   it, with or without its boilerplate notice. Texts that continue after the
///   appendix, like the one of the LLVM exception, are kept whole.
/// - Lines longer than 80 columns are wrapped at spaces. Continuation lines
///   keep the indentation of the line. Short lines are never joined, as their
///   linebreaks may be significant.
pub fn normalize_text(license: &LicenseReq, text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let text = trim_blank_lines(&text).trim_end();
    let text = if is_apache(license) { strip_apache_appendix(text) } else { text };

    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        push_wrapped(&mut out, line);
    }
    out
}

/// Merges license blocks that can share a text.
///
/// Blocks are merged if they have the same license, the same notice, and
/// license texts that are equal up to whitespace and the scheme of `http` and
/// `https` URLs. The merged block has the text of the first block of its group
/// and all items of the group, without exact duplicates. The result is ordered
/// by the first appearance of each group.
pub fn merge(blocks: impl IntoIterator<Item = LicenseBlock>) -> Vec<LicenseBlock> {
    let mut merged: Vec<LicenseBlock> = Vec::new();
    // The indices of the merged blocks by the hash of their group.
    let mut groups: HashMap<u64, usize> = HashMap::new();

    for mut block in blocks {
        block.text = normalize_text(&block.license.req, &block.text);
        block.notice = block.notice.as_deref().and_then(normalize_notice);

        let items = std::mem::take(&mut block.items);
        let index = *groups.entry(group_hash(&block)).or_insert_with(|| {
            merged.push(block);
            merged.len() - 1
        });

        let target = &mut merged[index].items;
        for item in items {
            if !target.contains(&item) {
                target.push(item);
            }
        }
    }

    merged
}

/// Hashes what license blocks must have in common to be merged: the license as
/// written in the license marker, the [`words`] of the license text, and the
/// notice.
fn group_hash(block: &LicenseBlock) -> u64 {
    let mut hasher = DefaultHasher::new();
    block.license.to_string().hash(&mut hasher);
    for word in words(&block.text) {
        word.hash(&mut hasher);
    }
    block.notice.hash(&mut hasher);
    hasher.finish()
}

/// The words of a license text, with the scheme of `http://` URLs split off so
/// that they equal `https://` URLs.
fn words(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.split_whitespace()
        .map(|word| match word.strip_prefix("http://") {
            Some(rest) => ("https://", rest),
            None => match word.strip_prefix("https://") {
                Some(rest) => ("https://", rest),
                None => ("", word),
            },
        })
}

/// Brings a notice into a canonical form: LF linebreaks and no empty lines at
/// the start and the end. Returns `None` if nothing is left.
fn normalize_notice(notice: &str) -> Option<String> {
    let notice = notice.replace("\r\n", "\n");
    let notice = trim_blank_lines(&notice);
    (!notice.is_empty()).then(|| notice.to_owned())
}

/// Removes empty (or whitespace-only) lines at the start and the end of the
/// text, but keeps the indentation of the first and the trailing whitespace of
/// the last remaining line.
fn trim_blank_lines(text: &str) -> &str {
    let content = text.trim_start();
    if content.is_empty() {
        return "";
    }
    let start = text[..text.len() - content.len()].rfind('\n').map_or(0, |i| i + 1);
    let text = &text[start..];

    let content_end = text.trim_end().len();
    let line_end = text[content_end..].find('\n').map_or(text.len(), |i| content_end + i);
    &text[..line_end]
}

/// Whether the license is the Apache 2.0 License, with or without an exception.
pub(crate) fn is_apache(license: &LicenseReq) -> bool {
    matches!(&license.license, LicenseItem::Spdx { id, .. } if id.name == "Apache-2.0")
}

/// Removes the appendix of the Apache 2.0 License, which is not part of the
/// terms.
///
/// The text must end with the appendix to be shortened: the appendix heading
/// must come right after the line that ends the terms and the last line of the
/// text must be the last line of the appendix, or of its instructions if the
/// boilerplate notice is left out. Texts that continue after the
/// appendix, like the one of the LLVM exception, are not cut because the
/// additional terms behind the appendix cannot be told apart from it.
fn strip_apache_appendix(text: &str) -> &str {
    let mut offset = 0;
    for line in text.split('\n') {
        if line.trim_start().starts_with(APACHE_APPENDIX) {
            let terms = text[..offset].trim_end();
            let end = text.trim_end();
            let ends_text = end.ends_with(APACHE_APPENDIX_END)
                || end.ends_with(APACHE_APPENDIX_INSTRUCTIONS_END);
            let ends_terms =
                terms.lines().next_back().is_some_and(|l| l.trim() == APACHE_END);
            return if ends_text && ends_terms { terms } else { text };
        }
        offset += line.len() + 1;
    }
    text
}

/// Appends a line, wrapped at spaces if it is longer than [`WIDTH`] columns.
///
/// The wrapped lines keep the indentation of the line. Runs of spaces at a
/// wrapping point are dropped. A word that is too long for a line overflows.
fn push_wrapped(out: &mut String, line: &str) {
    let content = line.trim_start();
    let (indent, mut rest) = line.split_at(line.len() - content.len());
    let room = WIDTH.saturating_sub(indent.chars().count());

    loop {
        out.push_str(indent);
        let Some(at) = break_point(rest, room) else {
            out.push_str(rest);
            return;
        };
        out.push_str(rest[..at].trim_end_matches(' '));
        out.push('\n');
        rest = rest[at..].trim_start_matches(' ');
    }
}

/// Finds the space at which to wrap text that must fit into `room` columns.
///
/// This is the last space that leaves a line that fits, or if there is none, the
/// first one. Returns `None` if the text fits or has no space to break at.
fn break_point(text: &str, room: usize) -> Option<usize> {
    let (limit, _) = text.char_indices().nth(room)?;
    let content_end = text.trim_end().len();
    let mut spaces = text.match_indices(' ').map(|(i, _)| i).filter(|&i| i < content_end);
    let fitting = spaces.clone().take_while(|&i| i <= limit).last();
    fitting.or_else(|| spaces.next())
}

#[cfg(test)]
mod tests {
    use spdx::{Expression, ParseMode};

    use super::*;
    use crate::{Item, License, parse};

    const TYPST: &str = include_str!("../examples/TYPST-NOTICE");
    const PIXGLYPH: &str = include_str!("../examples/PIXGLYPH-NOTICE");
    const EXAMPLES: [&str; 4] = [
        include_str!("../examples/CITATIONBERG-NOTICE"),
        include_str!("../examples/HAYAGRIVA-NOTICE"),
        PIXGLYPH,
        TYPST,
    ];

    fn req(text: &str) -> LicenseReq {
        let expression = Expression::parse_mode(text, ParseMode::STRICT).unwrap();
        expression.requirements().next().unwrap().req.clone()
    }

    fn item(description: &str, inclusion: Inclusion) -> Item {
        Item {
            description: description.into(),
            package: None,
            paths: vec![format!("{description}.rs")],
            source: "https://example.com".into(),
            notice: None,
            also_see: None,
            inclusion,
        }
    }

    fn block(license: &str, items: Vec<Item>, text: &str) -> LicenseBlock {
        LicenseBlock {
            license: License { req: req(license), name: None },
            items,
            text: text.into(),
            notice: None,
        }
    }

    fn descriptions(block: &LicenseBlock) -> Vec<&str> {
        block.items.iter().map(|i| i.description.as_str()).collect()
    }

    /// The license text of the first block with the given license in a file.
    fn example_text(file: &str, license: &str) -> String {
        let block = parse(file)
            .unwrap()
            .into_iter()
            .find(|b| b.license.to_string() == license);
        block.unwrap().text
    }

    #[test]
    fn test_prune() {
        let items = || {
            vec![
                item("always", Inclusion::Always),
                item("no-lib", Inclusion::NotInLibrary),
                item("no-bin", Inclusion::NotInBinary),
                item("neither", Inclusion::NotInBinaryOrLibrary),
            ]
        };
        let blocks = vec![
            block("MIT", items(), "text"),
            block("ISC", vec![item("neither", Inclusion::NotInBinaryOrLibrary)], "text"),
            block("0BSD", vec![item("no-lib", Inclusion::NotInLibrary)], "text"),
        ];

        let binary = prune(blocks.clone(), Usage::Binary);
        assert_eq!(binary.len(), 2);
        assert_eq!(descriptions(&binary[0]), ["always", "no-lib"]);
        assert_eq!(descriptions(&binary[1]), ["no-lib"]);
        assert_eq!(binary[1].license.to_string(), "0BSD");

        let library = prune(blocks, Usage::Library);
        assert_eq!(library.len(), 1);
        assert_eq!(descriptions(&library[0]), ["always", "no-bin"]);
    }

    #[test]
    fn test_normalize_text_whitespace() {
        let mit = req("MIT");
        assert_eq!(
            normalize_text(&mit, "\r\n  \r\n  a\r\n\r\n b \r\n\r\n  \r\n"),
            "  a\n\n b"
        );
        // Short lines are never joined.
        assert_eq!(normalize_text(&mit, "a\n\n\n b\n"), "a\n\n\n b");
        assert_eq!(normalize_text(&mit, " \n\t\n"), "");
    }

    #[test]
    fn test_normalize_text_wraps_at_80_columns() {
        let mit = req("MIT");
        let (a, b, x) = ("a".repeat(40), "b".repeat(40), "x".repeat(100));
        let exact = format!("{a} {}", &b[1..]);
        assert_eq!(normalize_text(&mit, &exact), exact);
        assert_eq!(normalize_text(&mit, &format!("{a} {b}")), format!("{a}\n{b}"));
        // A space at column 81 is where the line breaks.
        let at_limit = format!("{} b", "a".repeat(80));
        assert_eq!(normalize_text(&mit, &at_limit), format!("{}\nb", "a".repeat(80)));
        // Indentation is kept on continuation lines.
        let indented = format!("    {a} {b} {}", "c".repeat(40));
        assert_eq!(
            normalize_text(&mit, &indented),
            format!("    {a}\n    {b}\n    {}", "c".repeat(40))
        );
        // Long words overflow; space runs are only dropped at breaks.
        let text = format!("{x} short\n  {x}");
        assert_eq!(normalize_text(&mit, &text), format!("{x}\nshort\n  {x}"));
        let text = format!("{}   {}  end", "a".repeat(78), "b".repeat(10));
        let expected = format!("{}\n{}  end", "a".repeat(78), "b".repeat(10));
        assert_eq!(normalize_text(&mit, &text), expected);
    }

    #[test]
    fn test_normalize_text_is_idempotent() {
        for example in EXAMPLES {
            for block in parse(example).unwrap() {
                let once = normalize_text(&block.license.req, &block.text);
                assert_eq!(normalize_text(&block.license.req, &once), once);
            }
        }
    }

    #[test]
    fn test_apache_appendix_is_stripped() {
        let apache = req("Apache-2.0");
        let text = example_text(TYPST, "Apache-2.0");
        assert!(text.ends_with("END OF TERMS AND CONDITIONS"));
        let with_appendix = format!(
            "{text}\n\n    APPENDIX: How to apply the Apache License to your work.\n\n\
             \x20     Boilerplate.\n\n    Copyright [yyyy] [name of copyright owner]\n\n\
             \x20   limitations under the License.\n\n"
        );
        assert_eq!(
            normalize_text(&apache, &with_appendix),
            normalize_text(&apache, &text)
        );
        assert_eq!(
            normalize_text(&req("Apache-2.0 WITH LLVM-exception"), &with_appendix),
            normalize_text(&apache, &text),
        );
        // Some copies end after the instructions, without the boilerplate.
        let without_boilerplate = format!(
            "{text}\n\n    APPENDIX: How to apply the Apache License to your work.\n\n\
             \x20     Instructions for easier\n      identification within \
             third-party archives.\n"
        );
        assert_eq!(
            normalize_text(&apache, &without_boilerplate),
            normalize_text(&apache, &text)
        );
        // Other licenses have no appendix to strip.
        assert!(normalize_text(&req("MIT"), &with_appendix).contains("APPENDIX"));
        // Without the end of the terms nothing is cut.
        let text = "Terms\n\nAPPENDIX: How to apply the Apache License to your work.\n\n\
                    limitations under the License.";
        assert_eq!(normalize_text(&apache, text), text);
        // Texts without an appendix are unchanged.
        let plain = example_text(PIXGLYPH, "Apache-2.0");
        assert_eq!(normalize_text(&apache, &plain), plain.trim_end());
    }

    #[test]
    fn test_apache_llvm_text_keeps_exceptions() {
        let license = "Apache-2.0 WITH LLVM-exception";
        let text = example_text(TYPST, license);
        assert!(text.contains("APPENDIX: How to apply"));
        assert!(text.contains("LLVM Exceptions to the Apache 2.0 License"));

        // The exceptions follow the appendix, so nothing is cut.
        let normalized = normalize_text(&req(license), &text);
        assert!(normalized.contains("APPENDIX: How to apply"));
        assert_eq!(
            normalized.split_whitespace().collect::<Vec<_>>(),
            text.split_whitespace().collect::<Vec<_>>(),
        );
    }

    #[test]
    fn test_merge_groups_whitespace_differences() {
        let blocks = [
            block(
                "MIT",
                vec![item("a", Inclusion::Always)],
                "Permission is\nhereby granted\n",
            ),
            block(
                "MIT",
                vec![item("b", Inclusion::Always)],
                "\r\nPermission  is hereby\r\n  granted",
            ),
        ];
        let merged = merge(blocks);
        assert_eq!(merged.len(), 1);
        assert_eq!(descriptions(&merged[0]), ["a", "b"]);
        assert_eq!(merged[0].text, "Permission is\nhereby granted");
    }

    #[test]
    fn test_merge_groups_url_schemes() {
        let blocks = [
            block("MIT", vec![item("a", Inclusion::Always)], "See http://example.com/"),
            block("MIT", vec![item("b", Inclusion::Always)], "See https://example.com/"),
            block("MIT", vec![item("c", Inclusion::Always)], "See ftp://example.com/"),
        ];
        let merged = merge(blocks);
        assert_eq!(merged.len(), 2);
        assert_eq!(descriptions(&merged[0]), ["a", "b"]);
        // The text of the first block is kept.
        assert_eq!(merged[0].text, "See http://example.com/");
    }

    #[test]
    fn test_merge_keeps_different_licenses_texts_and_notices_apart() {
        let mut with_notice = block("MIT", vec![item("n", Inclusion::Always)], "text");
        with_notice.notice = Some("Notice".into());
        let mut other_notice = with_notice.clone();
        other_notice.notice = Some("Other".into());
        let named = |name: &str| {
            let mut block =
                block("LicenseRef-x", vec![item(name, Inclusion::Always)], "text");
            block.license.name = Some(name.into());
            block
        };
        let blocks = [
            block("MIT", vec![item("a", Inclusion::Always)], "Copyright A"),
            block("MIT", vec![item("b", Inclusion::Always)], "Copyright B"),
            block("ISC", vec![item("c", Inclusion::Always)], "Copyright A"),
            block("MIT", vec![item("d", Inclusion::Always)], "text"),
            with_notice,
            other_notice,
            named("One"),
            named("Two"),
        ];
        let merged = merge(blocks);
        assert_eq!(merged.len(), 8);
    }

    #[test]
    fn test_merge_notice_is_normalized_but_exact() {
        let with_notice = |notice: &str| LicenseBlock {
            notice: Some(notice.into()),
            ..block("MIT", vec![item(notice, Inclusion::Always)], "text")
        };
        let merged = merge([
            with_notice("\r\n\r\nCopyright 1\r\nline 2\r\n\r\n"),
            with_notice("Copyright 1\nline 2"),
            with_notice("Copyright  1\nline 2"),
            with_notice("  \n \n"),
        ]);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].notice.as_deref(), Some("Copyright 1\nline 2"));
        assert_eq!(merged[0].items.len(), 2);
        assert_eq!(merged[2].notice, None);
    }

    #[test]
    fn test_merge_removes_duplicate_items_and_keeps_order() {
        let a = item("a", Inclusion::Always);
        let b = item("b", Inclusion::Always);
        let b_pruned = item("b", Inclusion::NotInLibrary);
        let merged = merge([
            block("MIT", vec![a.clone(), b.clone(), a.clone()], "one"),
            block("ISC", vec![b.clone()], "two"),
            block("MIT", vec![b.clone(), b_pruned.clone()], "one"),
        ]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].items, [a, b.clone(), b_pruned]);
        assert_eq!(merged[0].license.to_string(), "MIT");
        assert_eq!(merged[1].items, [b]);
    }

    #[test]
    fn test_merge_is_deterministic_and_idempotent() {
        let blocks: Vec<_> = EXAMPLES.iter().flat_map(|e| parse(e).unwrap()).collect();
        let merged = merge(blocks.clone());
        assert_eq!(merge(blocks), merged);
        assert_eq!(merge(merged.clone()), merged);
    }
}
