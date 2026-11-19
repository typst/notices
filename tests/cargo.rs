//! Tests that check and release the licenses of the fixture projects.
//!
//! The fixtures only have path dependencies, so these tests work offline.

#![cfg(feature = "cargo")]

use std::path::Path;

use notices::cargo::{CONFIG_PATH, Config, MetadataOptions, Report, check, release};
use notices::{Inclusion, LicenseBlock};

/// The checksum of `crates/custom/LICENSE` in the `cargo` fixture.
const CUSTOM_CHECKSUM: &str =
    "25da9fbabb03d6914c365a92c16d334842c4fb8ec35d812a06d2f357fc84af71";

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn options(dir: &str, features: &[&str]) -> MetadataOptions {
    MetadataOptions {
        features: features.iter().map(|&feature| feature.to_owned()).collect(),
        locked: true,
        offline: true,
        ..MetadataOptions::new(format!("{FIXTURES}/{dir}/Cargo.toml"))
    }
}

/// Checks a fixture with the given features and configuration.
fn check_fixture(dir: &str, features: &[&str], config: &str) -> (Report, String) {
    let report = check(&config.parse().unwrap(), &options(dir, features)).unwrap();
    let output = report.to_string();
    (report, output)
}

/// The configuration of the `custom` crate with the given checksum.
fn clarify_custom(checksum: &str) -> String {
    format!(
        "[clarify.custom]\nlicense = \"MIT\"\nfiles = [{{ path = \"LICENSE\", checksum = \"{checksum}\" }}]\n"
    )
}

#[test]
fn test_license_policy() {
    // The fixture's own configuration accepts everything.
    let config = Config::load(format!("{FIXTURES}/cargo/{CONFIG_PATH}").as_ref());
    let report = check(&config.unwrap(), &options("cargo", &[])).unwrap();
    assert!(!report.has_errors(), "{report}");

    // `restricted` uses a license that is not allowed ...
    let rest = clarify_custom(CUSTOM_CHECKSUM);
    let (report, output) =
        check_fixture("cargo", &[], &format!("allow = [\"MIT\"]\n{rest}"));
    assert!(report.has_errors() && output.contains("Zlib"), "{output}");

    // ... unless there is an exception.
    let exception = "[[exceptions]]\ncrate = \"restricted\"\nallow = [\"Zlib\"]\n";
    let config = format!("allow = [\"MIT\"]\n{rest}{exception}");
    let (report, output) = check_fixture("cargo", &[], &config);
    assert!(!report.has_errors(), "{output}");

    // Omitted crates are still checked.
    let config = format!("allow = [\"MIT\"]\nomit = [\"restricted\"]\n{rest}");
    assert!(check_fixture("cargo", &[], &config).0.has_errors());
}

#[test]
fn test_clarifications() {
    let allow = "allow = [\"MIT\", \"Zlib\"]\n";
    let (report, output) = check_fixture("cargo", &[], allow);
    assert!(report.has_errors() && output.contains("custom"), "{output}");

    let config = allow.to_owned() + &clarify_custom(&"0".repeat(64));
    let (report, output) = check_fixture("cargo", &[], &config);
    assert!(report.has_errors());
    assert!(output.contains("clarification for `custom 1.0.0` is outdated"), "{output}");
    assert!(output.contains("checksum mismatch"), "{output}");
}

#[test]
fn test_notices_of_dependencies() {
    // Pruned and ordinary NOTICE files are fine.
    let allow = "allow = [\"MIT\"]\n";
    let (report, output) = check_fixture("notices", &[], allow);
    assert!(report.is_empty(), "{output}");

    // The licenses in a kept NOTICE file must be allowed.
    let (report, output) = check_fixture("notices", &["kept"], allow);
    assert!(report.has_errors());
    assert!(
        output.contains("license `CC-BY-SA-3.0` in the NOTICE file of `kept 1.0.0`"),
        "{output}"
    );
    assert!(output.contains("- Example data used for testing"), "{output}");

    // An exception for the crate allows it, one for another crate does not.
    let config =
        format!("{allow}[[exceptions]]\ncrate = \"kept\"\nallow = [\"CC-BY-SA-3.0\"]\n");
    let (report, output) = check_fixture("notices", &["kept"], &config);
    assert!(report.is_empty(), "{output}");
    let config = config.replace("kept", "other");
    assert!(check_fixture("notices", &["kept"], &config).0.has_errors());

    // A malformed NOTICE file is an error.
    let (report, output) = check_fixture("notices", &["malformed"], allow);
    assert!(report.has_errors());
    assert!(
        output.contains("malformed Typst NOTICE file of `malformed 1.0.0`: 13:14"),
        "{output}"
    );
}

mod config {
    //! Tests for loading the configuration file.

    use super::*;
    use notices::cargo::find_in_workspace;
    use std::path::PathBuf;

    #[test]
    fn test_parse_typst() {
        let config: Config = include_str!("fixtures/typst-notices.toml").parse().unwrap();

        let allow: Vec<_> = config.allow.iter().map(|l| l.to_string()).collect();
        assert_eq!(allow[..4], ["Apache-2.0", "0BSD", "CC0-1.0", "Zlib"]);
        assert_eq!(allow.len(), 12);
        assert_eq!(config.targets.len(), 8);
        assert!(config.ignore_build_dependencies && config.ignore_dev_dependencies);
        assert!(config.private.ignore);
        assert_eq!(config.omit, ["roman-numerals-rs"]);
        assert_eq!(config.clarify.keys().collect::<Vec<_>>(), ["kamadak-exif", "libm"]);

        let libm = &config.clarify["libm"];
        assert_eq!(libm.license.to_string(), "MIT");
        assert_eq!(libm.files[0].path, "LICENSE.txt");
        assert_eq!(libm.files[0].start.as_deref(), Some("rust-lang/libm as a whole"));
        assert_eq!(libm.files[0].end.as_deref(), Some("SOFTWARE."));
    }

    #[test]
    fn test_errors() {
        let err = Config::load("tests/fixtures/does-not-exist.toml".as_ref());
        assert!(err.unwrap_err().contains("does-not-exist.toml"));

        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("broken.toml");
        std::fs::write(&path, "allow = [\"MIT\"\n").unwrap();
        let err = Config::load(&path).unwrap_err();
        assert!(err.contains("broken.toml") && err.contains("line 1"), "{err}");

        let err = "allow = [\"MIT\"]\nalow = 1\n".parse::<Config>().unwrap_err();
        assert!(err.contains("unknown field `alow`"), "{err}");
        assert!("allow = []\n".parse::<Config>().unwrap_err().contains("at least one"));
    }

    /// Creates a package with a configuration file two levels above it.
    fn tree(name: &str, workspace: &str) -> (PathBuf, PathBuf) {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("find-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        let package = root.join("workspace/package");
        std::fs::create_dir_all(root.join(".cargo")).unwrap();
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(root.join(CONFIG_PATH), "allow = [\"MIT\"]\n").unwrap();
        std::fs::write(root.join("workspace/Cargo.toml"), workspace).unwrap();
        (root, package.join("Cargo.toml"))
    }

    #[test]
    fn test_discovery() {
        // Without a workspace, the search continues upwards.
        let (root, manifest) = tree("up", "[package]\nname = \"outer\"\n");
        assert_eq!(Config::find(&manifest), Some(root.join(CONFIG_PATH)));

        // A workspace root ends the search.
        let (_, manifest) = tree("stop", "[workspace]\nmembers = [\"package\"]\n");
        assert_eq!(Config::find(&manifest), None);

        // Other files are found the same way, up to and including the
        // workspace root.
        let (root, manifest) = tree("notice", "[workspace]\nmembers = [\"package\"]\n");
        assert_eq!(find_in_workspace(&manifest, "NOTICE"), None);
        let notice = root.join("workspace/NOTICE");
        std::fs::write(&notice, "").unwrap();
        assert_eq!(find_in_workspace(&manifest, "NOTICE"), Some(notice));
    }
}

mod release_fixture {
    //! Tests that release the licenses of the `release` fixture.

    use super::*;

    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/release");

    fn config() -> Config {
        Config::load(format!("{DIR}/{CONFIG_PATH}").as_ref()).unwrap()
    }

    /// Releases the fixture project with its repository NOTICE file.
    fn released(config: &Config) -> Vec<LicenseBlock> {
        let text = std::fs::read_to_string(format!("{DIR}/NOTICE")).unwrap();
        let repo = notices::parse(&text).unwrap();
        let (blocks, report) =
            release(Some(repo), config, &options("release", &[])).unwrap();
        assert!(report.is_empty(), "{report}");
        blocks
    }

    /// The descriptions of a block's items.
    fn descriptions(block: &LicenseBlock) -> Vec<&str> {
        block.items.iter().map(|item| item.description.as_str()).collect()
    }

    /// Finds the blocks of a license with or without a notice.
    fn find<'a>(
        blocks: &'a [LicenseBlock],
        license: &str,
        notice: bool,
    ) -> Vec<&'a LicenseBlock> {
        blocks
            .iter()
            .filter(|b| b.license.to_string() == license && b.notice.is_some() == notice)
            .collect()
    }

    #[test]
    fn test_repository_notice() {
        let blocks = released(&config());

        // The output is a valid NOTICE file.
        let text = notices::write(&blocks).unwrap();
        assert_eq!(notices::parse(&text).unwrap().len(), blocks.len());

        // The repository comes first, pruned for binaries.
        assert_eq!(blocks[0].license.to_string(), "Zlib");
        assert_eq!(descriptions(&blocks[0]), ["Vendored code"]);
        assert_eq!(blocks[0].items[0].package, None);
        assert!(blocks.iter().all(|block| block.license.to_string() != "ISC"));

        // Workspace members are not included.
        let all = blocks.iter().flat_map(descriptions).collect::<Vec<_>>();
        assert!(!all.contains(&"The `app` crate"));
    }

    #[test]
    fn test_grouping_and_notices() {
        let blocks = released(&config());

        // Identical texts are grouped.
        let [mit] = find(&blocks, "MIT", false)[..] else { panic!("one MIT block") };
        assert_eq!(
            descriptions(mit),
            ["The `apache-and` crate", "The `plain` crate", "The `typst-notice` crate"],
        );
        let item = &mit.items[1];
        assert_eq!(item.package.as_ref().unwrap().to_string(), "`plain` 1.0.0 (cargo)");
        assert_eq!(item.source, "https://example.com/plain");
        assert_eq!(item.inclusion, Inclusion::Always);

        // An ordinary NOTICE file is attached to the Apache block.
        let [apache] = find(&blocks, "Apache-2.0", true)[..] else { panic!("one") };
        assert_eq!(descriptions(apache), ["The `apache-and` crate"]);
        let notice = apache.notice.as_deref().unwrap();
        assert!(notice.starts_with("Apache notice of apache-and."));

        // So is a `NOTICE.txt`, to the MIT block of another crate.
        let [plain] = find(&blocks, "MIT", true)[..] else { panic!("one") };
        assert_eq!(descriptions(plain), ["The `plain-notice` crate"]);
        assert_eq!(plain.notice.as_deref(), Some("Notice of plain-notice."));
    }

    #[test]
    fn test_or_expression() {
        let blocks = released(&config());
        let all: Vec<_> = blocks
            .iter()
            .filter(|b| descriptions(b).contains(&"The `either` crate"))
            .collect();

        // The first allowed license is chosen.
        let [apache] = all[..] else { panic!("expected one block for `either`") };
        assert_eq!(apache.license.to_string(), "Apache-2.0");

        // A repository that is not a URI is replaced by the crates.io page.
        assert_eq!(apache.items[0].source, "https://crates.io/crates/either");
    }

    #[test]
    fn test_typst_notice_of_dependency() {
        let blocks = released(&config());
        let [cc] = &blocks
            .iter()
            .filter(|b| b.license.to_string() == "CC-BY-SA-3.0")
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one CC-BY-SA-3.0 block");
        };

        // The library-only item is pruned and the package is added.
        let [item] = cc.items.as_slice() else { panic!("expected one item") };
        assert_eq!(item.description, "Example data used for testing");
        let package = item.package.as_ref().unwrap();
        assert_eq!(package.to_string(), "`typst-notice` 1.0.0 (cargo)");
        assert_eq!(item.paths, ["data/"]);
    }

    #[test]
    fn test_omit() {
        let mut config = config();
        config.omit.push("plain".into());
        let blocks = released(&config);
        let all: Vec<_> = blocks.iter().flat_map(descriptions).collect();
        assert!(!all.contains(&"The `plain` crate"));
        assert!(all.contains(&"The `plain-notice` crate"));
    }
}
