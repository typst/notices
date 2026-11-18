//! Tests that run the command line tool.

#![cfg(feature = "cli")]

use std::path::Path;
#[cfg(feature = "cargo")]
use std::path::PathBuf;
use std::process::{Command, Output};

/// The root of the package, which contains the fixtures.
const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// Runs `notices` with the given arguments in the given directory.
fn notices_in(dir: impl AsRef<Path>, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_notices"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

/// Runs `notices` with the given arguments in the package root.
fn notices(args: &[&str]) -> Output {
    notices_in(ROOT, args)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A fresh directory in the target directory, unique to the named test.
#[cfg(feature = "cargo")]
fn fresh_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("cli-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_check_notice_files() {
    // A valid file, and a directory that defaults to the current one.
    let output = notices(&["check", "examples/TYPST-NOTICE"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("valid Typst NOTICE file"));
    let pruned = format!("{ROOT}/tests/fixtures/notices/crates/pruned");
    assert!(notices_in(pruned, &["check"]).status.success());

    // A malformed file.
    let output = notices(&["check", "tests/fixtures/notices/crates/malformed/NOTICE"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("NOTICE:13:14: "), "{}", stderr(&output));

    // An ordinary file.
    let path = "tests/fixtures/notices/crates/ordinary/NOTICE";
    let output = notices(&["check", path]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("--allow-ordinary"));
    assert!(notices(&["check", path, "--allow-ordinary"]).status.success());

    // A directory without a NOTICE file, and a missing file.
    let dir = "tests/fixtures/notices/src";
    let output = notices(&["check", dir]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("no file named `NOTICE`"));
    let output = notices(&["check", dir, "--allow-missing"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("note: "));
    let missing = notices(&["check", "tests/fixtures/notices/NOTICE", "--allow-missing"]);
    assert!(!missing.status.success());
}

#[cfg(not(feature = "cargo"))]
#[test]
fn test_deps_needs_the_cargo_feature() {
    let output = notices(&["check", "--deps"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("--deps"));
}

#[cfg(feature = "cargo")]
mod cargo {
    use super::*;

    const FIXTURE: &str = "tests/fixtures/notices";
    const RELEASE: &str = "tests/fixtures/release";

    /// Checks the fixture with the dependency check and extra arguments.
    fn check_deps(args: &[&str]) -> Output {
        let base =
            ["check", FIXTURE, "--allow-missing", "--deps", "--locked", "--offline"];
        notices(&[&base[..], args].concat())
    }

    /// Runs `release` for a fixture with extra arguments.
    fn release(fixture: &str, args: &[&str]) -> Output {
        let manifest = format!("{fixture}/Cargo.toml");
        let base = ["release", "--manifest-path", &manifest, "--locked", "--offline"];
        notices(&[&base[..], args].concat())
    }

    #[test]
    fn test_check_deps() {
        let output = check_deps(&[]);
        assert!(output.status.success(), "{}", stderr(&output));
        assert!(stderr(&output).contains("0 error(s), 0 warning(s)"));

        let output = check_deps(&["--features", "kept"]);
        assert!(!output.status.success());
        let stderr = stderr(&output);
        assert!(stderr.contains("`CC-BY-SA-3.0` in the NOTICE file of `kept 1.0.0`"));
        assert!(stderr.contains("1 error(s)"), "{stderr}");
    }

    #[test]
    fn test_check_deps_configuration_errors() {
        // An explicit configuration that does not exist.
        let output = check_deps(&["--config", "does-not-exist.toml"]);
        assert!(!output.status.success());
        assert!(stderr(&output).contains("does-not-exist.toml"));

        // No configuration anywhere up the tree.
        let dir = fresh_dir("no-config");
        std::fs::write(dir.join("Cargo.toml"), "[workspace]\n").unwrap();
        let output = notices_in(&dir, &["check", "--allow-missing", "--deps"]);
        assert!(!output.status.success());
        assert!(stderr(&output).contains(".cargo/notices.toml"));

        // The options require `--deps`.
        assert!(!notices(&["check", "--offline"]).status.success());
    }

    #[test]
    fn test_release_writes_notice_and_json() {
        let dir = fresh_dir("outputs");
        let notice = dir.join("out.NOTICE").to_str().unwrap().to_owned();
        let json = dir.join("out.json").to_str().unwrap().to_owned();
        let out = release(RELEASE, &["-o", &format!("{notice},{json}")]);
        assert!(out.status.success(), "{}", stderr(&out));
        assert!(stdout(&out).contains("6 license blocks, 8 items"), "{}", stdout(&out));

        // The repository's NOTICE next to the manifest was used.
        assert!(std::fs::read_to_string(&notice).unwrap().contains("* Vendored code"));
        assert!(notices(&["check", &notice]).status.success());
        let json = std::fs::read_to_string(&json).unwrap();
        assert!(json.contains("\"description\": \"Vendored code\""));
    }

    #[test]
    fn test_release_writes_nothing_on_errors() {
        let dir = fresh_dir("failure");
        let notice = dir.join("out.NOTICE").to_str().unwrap().to_owned();
        let json = dir.join("out.json").to_str().unwrap().to_owned();
        let out = release(
            FIXTURE,
            &["--features", "malformed", "-o", &format!("{notice},{json}")],
        );
        assert!(!out.status.success());
        assert!(stderr(&out).contains("malformed Typst NOTICE file"), "{}", stderr(&out));
        assert!(!Path::new(&notice).exists() && !Path::new(&json).exists());
    }

    #[test]
    fn test_release_rejects_ordinary_repository_notice() {
        let dir = fresh_dir("ordinary");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "").unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("NOTICE"), "Just a notice.\n").unwrap();
        let config = Path::new(ROOT).join(FIXTURE).join(".cargo/notices.toml");

        let out = notices_in(
            &dir,
            &["release", "--config", config.to_str().unwrap(), "-o", "out.NOTICE"],
        );
        assert!(!out.status.success());
        let stderr = stderr(&out);
        assert!(
            stderr.contains("does not start with the Typst NOTICE header"),
            "{stderr}"
        );
    }
}
