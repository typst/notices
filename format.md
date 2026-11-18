# Typst NOTICE file format specification

Next to original work, code repositories can incorporate work from various openly licensed sources. Dependencies pulled by a programming language package manager are tracked through manifests and lockfiles, but a project may contain additional external intellectual property like code snippets, vendored libraries, and data.

The Typst NOTICE file aims to make these items not covered by dependency managers traceable. The repository and its downstream consumers should be set up to track item licensing, comply with licenses, and create complete Software Bill of Materials (SBOMs). For this purpose, we are defining a file format for tracking these items that is both machine- and human-readable.

For each given item, Typst NOTICE files track:

- A human-readable description
- A source URL
- An SPDX simple expression, optionally WITH a license exception ID
- For licenses without an SPDX identifier, their full name
- Which package from a package ecosystem contains the item
- Which files or directories in the repository or package contain the item
- Any attribution or copyright identifier not included in the license text
- Additional information on how the license applies
- Full license text
- Ordinary NOTICE text that accompanies the item
- Whether the item is included in compiled binary or library distributions

A distribution of the software, e.g. a release binary, can ship a Typst NOTICE file that combines the repository's NOTICE file with the licenses and notices of all dependencies included in it.

The format is designed to be easily read by humans, too. When used in repositories licensed under the Apache 2.0 License, consumers of the repository are compelled to include the NOTICE file, propagating any tracked items to their distribution. Consumers who are aware of the Typst NOTICE format can consume the file, for example to merge duplicate license blocks or to prune items not present in their distribution.

## Format definition

In this section, _must_ and _must not_ state requirements, whereas _can_ and _may_ grant permissions. Recommendations are collected in the section on considerations for authors.

A Typst NOTICE file:

- is named `NOTICE` and located at the root of the repository,
- is encoded in UTF-8,
- uses either LF or CRLF linebreaks, but not both,
- has no trailing whitespace on any line except within license and notice texts, and
- uses spaces, not tabs, wherever this specification prescribes spaces. Tabs can only occur within license and notice texts.

It must start with this text:

```
This NOTICE file compiles licenses and notices for code, data, and other
contents of this software that originate from third parties. In a source
repository, it covers contents not tracked by the dependency tree or other
machine-readable manifests; accompanying a distribution, it can also cover the
dependencies included in it. It is machine-readable and follows the format set
out in https://github.com/typst/notice (version 1). Tools that can parse this
file can use it to incorporate it into license notices, Software Bills of
Materials (SBOMs), etc of their distributions of this software, provided the
contents within are represented faithfully. Other distributions must reproduce
this file verbatim.
```

In the header, a single space and a single linebreak are interchangeable. The header must be followed by two linebreaks and then by one or more license blocks.

License blocks group multiple items to which the same verbatim license text applies. Each license block contains the following:

- **Start marker** of at least eighty `=` characters and a linebreak

- **License marker:**
    - A single SPDX license identifier (an SPDX simple expression), optionally followed by `WITH` and a license exception ID. The operators `AND` and `OR` and parentheses are not allowed.

    - If the SPDX expression is a license reference (`DocumentRef-` or `LicenseRef-`), a space followed by the name of the license in round parentheses. The name consists of words separated by single spaces. If it contains parentheses, they must be matched.

    - The string ` --- This license applies to:`, followed by two linebreaks

  The license marker starts on the line directly after the start marker. Its components are separated by single spaces, each of which can be replaced by a single linebreak to wrap the marker at 80 columns.

- One or more **item descriptions**
    - At the start of the line, an asterisk followed by a space

    - A _human readable description of the item._ It can have multiple lines, each but the first line must start with two spaces. It must not contain empty lines.

    - Two linebreaks, followed by a _property list_ describing the item.

      The property list is a key-value list where the keys are separated from the values by a colon and a space. Each pair starts on a new line, and keys must be indented with two spaces. Property values can break onto multiple lines at spaces. When they do so, each but the first line must be indented such that its text aligns with the start of the value on the first line. The linebreak and the indentation then represent a single space. A URI and a path enclosed in backticks must not break. Properties must occur in the order defined in this specification.

      The property list can end with an _inclusion statement_. It is not a key-value pair, but a line of its own that follows all key-value pairs, is indented with two spaces, and whose text after the indentation matches the whole expression `/Not included when used as a (library|(binary( or a library)?))/`.

      In the following, here are the allowed keys in property lists.

        + _`Package`:_ The package that contains the item, written as the package name enclosed in backticks, a space, the package version, a space, and the package ecosystem enclosed in round parentheses, e.g. ``Package: `foo` 1.2.3 (cargo)``. The name and the version must not contain whitespace. The ecosystem must be a package type from the [purl specification](https://github.com/package-url/purl-spec), e.g. `cargo`, `npm`, or `pypi`.

        + _`Path` or `Paths`:_ A comma-separated list of directories or file paths that contain the item. If a `Package` is given, the paths are relative to the package's root. Otherwise, they are relative to the repository root. Each path must not start with a slash and be enclosed in backticks. When the key is `Path`, this must be one item. When the key is `Paths`, it can be one or more items. The paths need not exist, e.g. because files were excluded from a package or distribution.

          At least one of `Package` and `Path` or `Paths` is required.

        + _`Source`:_ Required. An URI for where the original item was sourced.

        + _`Notice`:_ Any copyright notices or attributions not part of the license text that need to be included with the item. Consumers must reproduce them in their own notices.

        + _`Also see`:_ String with additional information that clarifies how the license applies to the item, e.g., an URL.

        If the inclusion statement is present, it indicates that the item may not be present in some distributions of this repository. Downstream consumers that understand Typst NOTICE files may prune them if they do not apply.

    - Two item descriptions or the last item description and the license text a separated by an empty line (two newline characters after the last non-newline property list character).

- The **license text** applying to all items in the license block. The license text's first line must contain non-whitespace characters and must not start with an asterisk followed by a space, which would start another item. It starts on the line after the empty line that follows the last item and consists of everything up to the linebreak that precedes the notice marker or, if there is none, the end marker.

- Optionally, an ordinary **notice** that accompanies all items in the license block, e.g. the contents of an Apache 2.0 `NOTICE` file:
    - The **notice marker**, a line of exactly as many `-` characters as the start marker has `=` characters. The license text ends at the first such line.

    - The **notice text**, reproduced verbatim. Its first line must contain non-whitespace characters. It consists of everything after the notice marker up to the linebreak that precedes the end marker.

  A license block contains at most one notice. Items can only share a license block if their notices are identical.

- The **end marker**, a line of exactly as many `=` characters as the start marker. The license and notice texts end at the first such line. Authors must choose a marker length such that no line of the license text consists of exactly that many `=` or `-` characters and no line of the notice text consists of exactly that many `=` characters.

Two license blocks must be separated by two empty lines (three linebreaks after the end marker). The file ends with a single linebreak after the end marker of its last license block.

## Considerations for authors

Typst NOTICE files should be wrapped at 80 columns, including license texts where line breaks are not semantically relevant.

A SPDX identifier may occur in multiple license blocks if the license texts for their items differ. For example, in the MIT license, a customizable copyright note is an integral part of the license. In such a case, each unique license needs its own license block.

Text below the license heading in an item's license file, such as the copyright line of the MIT license, is part of the license text and must be reproduced as such. The `Notice` key is only for copyright notes and attributions that are not part of the license text.

The SPDX expression names the license that this repository applies to the item, not all licenses the item is offered under. If an item is available under several licenses, for example `MIT OR Apache-2.0`, name the one that is applied.

Where a license itself provides a short reference text that may be reproduced in place of its full terms (such as the LPPL's reference text), the license text in a license block may be that reference text instead.

To keep the file short, authors should seek to unify as many license blocks as possible per unique SPDX expression. Rewrap license texts to 80 columns and unify formatting elements where they are not semantically relevant. In Rust terms, two texts equivalent under `str::split_whitespace` should be in the same block. Where a license file contains more than the license that needs to be reproduced, truncate to that part:

- The Apache 2.0 License contains an appendix with instructions for license application. Its definition of the term _License_ does not include that appendix and it only requires to redistribute the License, hence it is trimmed.

- The SIL Open Font License 1.1 contains a copyright note above the license header. Place that note in the `Notice` key of the property list and only reproduce the file starting at the license header.

The `Notice` key must be populated whenever a license requires reproduction of a copyright note or attribution and where the license text does not include it. When the project has no explicit instructions for attributions, as a source, prefer notes in the following order:

1. Any copyright notes from their license file, including at the top or completed Apache appendices.
2. Copyright notices in code file headers
3. Authors key in the package manifest
4. GitHub username or other identifier