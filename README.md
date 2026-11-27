# Typst NOTICE files

This repository defines and contains tooling for the Typst NOTICE file format. Typst NOTICE files fulfill two purposes:

1. Inside a repository, they catalog the licensing for intellectual property like code snippets, vendored libraries, and data that is not pulled through a dependency manager and not tracked in other machine-readable data.

2. In a release, it serves as a full manifest of the included third-party intellectual property that is concise, complete, and both human- and machine-readable.

To this effect, this repository contains:

- [A specification for the Typst NOTICE file format](https://github.com/typst/notices/blob/main/format.md), including guidance for authors
- A crate to process Typst NOTICE files
- A GitHub Action to check and create Typst NOTICE files

## Crate

The `notices` crate can check a repository for a valid Typst NOTICE file or prepare a full Typst notice file for a release.

**To validate a Typst NOTICE file,** run `notices check`. When the `--deps` flag is passed, it additionally checks the licenses of the dependency tree and of the Typst NOTICE files that dependencies ship against the allowed licenses.

For a release, this crate will merge the local NOTICE as well as all dependency crates and their licenses and notices into a singular Typst NOTICE file. Where dependencies distribute Typst NOTICE files, they will be parsed and merged with the final output, so that contents not covered by their package license are included, too. **To create a release notice file,** run `notices release -o NOTICE`. Pass `-o` more than once (or separate paths with commas) to write several outputs; files ending in `.json` are [written as JSON](https://github.com/typst/notices/blob/main/json.md) (intended for consumption in tools that are not aware of Typst NOTICE files).

The crate can also be used as a library to parse, write, and merge Typst NOTICE files.

## Configuration

Dependency checks and releases read `.cargo/notices.toml` in the workspace root:

```toml
# Allowed licenses. When a dependency offers a choice of licenses, the first
# one in this list that it offers is chosen for a release.
allow = ["Apache-2.0", "MIT", "BSD-3-Clause"]

# Only consider dependencies of these targets.
targets = ["x86_64-unknown-linux-musl", "aarch64-apple-darwin"]

ignore-build-dependencies = true
ignore-dev-dependencies = true
private = { ignore = true }

# Crates that are left out of release NOTICE files, e.g. because they ship no
# license file. Their licenses are still checked.
omit = ["roman-numerals-rs"]

# Licenses allowed for one crate only (all versions), including licenses from
# its Typst NOTICE.
[[exceptions]]
crate = "hayagriva"
allow = ["CC-BY-SA-3.0"]

# States the license of a crate whose license cannot be detected correctly
# automatically. Only applies while the checksums of the files match, for all
# versions of the crate or those matching `version`.
[clarify.libm]
version = "^0.2"
license = "MIT"
files = [{ path = "LICENSE.txt", checksum = "88abfa95…", start = "rust-lang/libm as a whole", end = "SOFTWARE." }]

# A crate can have several clarifications for different versions. At most one
# may apply to each version.
[[clarify.kamadak-exif]]
version = "^0.5"
license = "BSD-2-Clause"
files = [{ path = "src/lib.rs", checksum = "2e7f6651…", end = "SUCH DAMAGE." }]

[[clarify.kamadak-exif]]
version = "^0.6"
license = "BSD-2-Clause"
files = [{ path = "LICENSE", checksum = "17c496a7…" }]
```

A release reads the NOTICE files of dependencies, too: items of a dependency's Typst NOTICE file that have no `Package` get the dependency as their package, so their paths are interpreted relative to the package root, which equals the repository root for crates at the root of their repository. If a dependency has no NOTICE file but the root of its workspace does, e.g. for a crate from a Git repository with several crates, that file is used. Of a Typst NOTICE file there, only the items with paths in the crate's directory, without paths, or naming a package are kept, and paths in the crate's directory are made relative to it. Packaged crates, e.g. from a registry, are self-contained and never use a NOTICE file of a parent directory.

`release` does not check licenses, so run `check --deps` first, for example in the same workflow. It includes the repository's NOTICE file, which it looks for next to the manifest and then in each parent directory up to the workspace root. The `--target` option of both commands replaces `targets`.

## GitHub Action

The action downloads a prebuilt binary from this repository's releases, verifies its checksum, and runs it. It supports Linux (x86-64 and AArch64), macOS (AArch64), and Windows (x86-64). Dependency checks and releases call `cargo metadata`, so they need a Rust toolchain on the runner. Pin the action by commit SHA.

```yaml
# On pull requests
- uses: typst/notices@<sha>
  with:
    args: check --deps --allow-missing

# On releases
- uses: typst/notices@<sha>
  with:
    args: release --locked -o NOTICE,NOTICE.json
```

The `repository` input (default `typst/notices`) selects the repository to download releases from. Arguments are split at whitespace; quoting is not supported. The `SHA256SUMS` file guards against corrupted downloads, not against a compromised release.

## License

This crate is dual-licensed under the MIT and Apache 2.0 licenses. The released binaries come with a Typst NOTICE file for their dependencies, created by the binaries themselves.

## AI Disclosure

As this crate is intended as CI-only tooling, we have created it using AI, including Claude Sonnet 5.5, Claude Opus 5.5, and Xiaomi MiMo v2.6 Pro. This README and the format.json file have been created manually.