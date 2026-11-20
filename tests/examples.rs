//! Tests that parse the example files from the `examples` directory.

use notices::{Inclusion, parse};

const HAYAGRIVA: &str = include_str!("../examples/HAYAGRIVA-NOTICE");
const PIXGLYPH: &str = include_str!("../examples/PIXGLYPH-NOTICE");
const TYPST: &str = include_str!("../examples/TYPST-NOTICE");
const CITATIONBERG: &str = include_str!("../examples/CITATIONBERG-NOTICE");

/// The number of items in each block of a parsed file.
fn shape(text: &str) -> Vec<usize> {
    parse(text).unwrap().iter().map(|block| block.items.len()).collect()
}

#[test]
fn test_examples_parse() {
    assert_eq!(shape(HAYAGRIVA), [3]);
    assert_eq!(shape(PIXGLYPH), [3, 4]);
    assert_eq!(shape(CITATIONBERG), [2]);
    assert_eq!(shape(TYPST).len(), 6);
}

#[test]
fn test_example_details() {
    let blocks = parse(HAYAGRIVA).unwrap();
    let item = &blocks[0].items[0];
    assert_eq!(blocks[0].license.to_string(), "CC-BY-SA-3.0");
    assert!(blocks[0].text.starts_with("Creative Commons Legal Code\n"));
    assert_eq!(item.description, "The CSL styles used for testing");
    assert_eq!(item.paths, ["tests/data/"]);
    assert_eq!(item.inclusion, Inclusion::NotInLibrary);
    assert_eq!(blocks[0].items[1].inclusion, Inclusion::Always);
    assert_eq!(
        item.notice.as_deref(),
        Some("Styles created by the CSL project at https://citationstyles.org/")
    );

    let blocks = parse(TYPST).unwrap();
    let bsd = &blocks[4].license;
    assert_eq!(bsd.to_string(), "BSD-3-Clause");
    let llvm = &blocks[5];
    assert_eq!(llvm.license.req.to_string(), "Apache-2.0 WITH LLVM-exception");
    // The license text may start with a line that looks like a marker.
    assert!(llvm.text.lines().next().unwrap().chars().all(|c| c == '='));
}

#[cfg(feature = "serde")]
#[test]
fn test_json() {
    use serde_json::{Value, json};

    // The header of an example plus a block with all properties.
    let header = HAYAGRIVA.split("=====").next().unwrap();
    let notice = format!(
        "{header}{}\nApache-2.0 WITH LLVM-exception --- This license applies to:\n\n\
         * A package\n\n  Package: `foo` 1.2.3 (cargo)\n  Paths: `a/`, `b/`\n  \
         Source: https://example.com/foo\n  Notice: Copyright (C) Foo.\n  \
         Also see: Some remark.\n  Not included when used as a library\n\n\
         License text.\n{}\n",
        "=".repeat(80),
        "=".repeat(80),
    );
    let json = notices::to_json(&parse(&notice).unwrap());
    assert!(json.starts_with("{\n  \"version\": 1,\n  \"blocks\": [\n"));
    let value: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["version"], notices::JSON_VERSION);
    assert_eq!(
        value["blocks"],
        json!([{
            "license": { "req": "Apache-2.0 WITH LLVM-exception", "name": null },
            "items": [{
                "description": "A package",
                "package": { "name": "foo", "version": "1.2.3", "ecosystem": "cargo" },
                "paths": ["a/", "b/"],
                "source": "https://example.com/foo",
                "notice": "Copyright (C) Foo.",
                "also_see": "Some remark.",
                "inclusion": "not_in_library",
            }],
            "text": "License text.",
            "notice": null,
        }])
    );

    // Absent values have defaults.
    let json = notices::to_json(&parse(TYPST).unwrap());
    let value: Value = serde_json::from_str(&json).unwrap();
    let item = &value["blocks"][4]["items"][0];
    assert_eq!(value["blocks"][3]["license"]["req"], "LPPL-1.3c");
    assert_eq!(item["package"], Value::Null);
    assert_eq!(item["inclusion"], "always");
}
