# JSON output

`notices release -o <file>.json` (and `notices::to_json` with the `serde` feature) writes license blocks as JSON, so that consumers such as web pages need neither Rust tooling nor a Typst NOTICE parser. The JSON is a lossless view of the parsed Typst NOTICE file: the same blocks in the same order. It is output only; there is no way to read it back.

The files are UTF-8, pretty-printed and end with a linebreak. All keys are always present. Absent optional values are `null`, never omitted. Keys use `snake_case`. Consumers must ignore keys they do not know, so that fields can be added without changing the version.

## Document

```json
{
  "version": 1,
  "blocks": [ ... ]
}
```

| Key       | Type             | Description                                       |
|-----------|------------------|---------------------------------------------------|
| `version` | integer          | The version of this schema. Incremented only for changes that are not backwards compatible (removing or renaming keys, changing types or meanings). Currently `1`. It is independent of the version of the NOTICE format. |
| `blocks`  | array of block   | The license blocks, in the order of the NOTICE file. |

## Block

A license together with all items it applies to.

| Key       | Type           | Description                                          |
|-----------|----------------|------------------------------------------------------|
| `license` | license        | The license that applies to the items.               |
| `items`   | array of item  | The items covered by the license. Never empty.       |
| `text`    | string         | The verbatim license text, without a trailing linebreak. Lines are separated by `\n` or `\r\n` as in the source. Render it in a monospace, whitespace-preserving element. |
| `notice`  | string or null | The verbatim ordinary notice that accompanies the items, with the same line-ending and whitespace rules as `text`. |

## License

| Key    | Type           | Description                                             |
|--------|----------------|---------------------------------------------------------|
| `req`  | string         | The SPDX license requirement as an SPDX expression: a license identifier or license reference, optionally with an exception, e.g. `MIT`, `Apache-2.0 WITH LLVM-exception`, `LicenseRef-LPPL-1.3`. Never a compound expression with `AND`/`OR`. |
| `name` | string or null | The full name of the license. Non-null if and only if `req` is a `LicenseRef-` or `DocumentRef-` reference. |

## Item

An item that is covered by a license.

| Key           | Type              | Description                                    |
|---------------|-------------------|------------------------------------------------|
| `description` | string            | A human-readable description, on a single line. |
| `package`     | package or null   | The package that contains the item.            |
| `paths`       | array of string   | Paths that contain the item, relative to the package root if `package` is non-null and to the repository root otherwise. Empty if only a package is given. |
| `source`      | string            | A URI for where the item was sourced.          |
| `notice`      | string or null    | Copyright notices or attributions to reproduce with the item. Wrapped lines are joined by single spaces, so it contains no linebreaks. |
| `also_see`    | string or null    | Additional information on how the license applies. |
| `inclusion`   | string            | In which distributions the item is included, one of `always`, `not_in_library`, `not_in_binary`, `not_in_binary_or_library`. |

## Package

| Key         | Type   | Description                                              |
|-------------|--------|----------------------------------------------------------|
| `name`      | string | The name of the package.                                 |
| `version`   | string | The version of the package.                              |
| `ecosystem` | string | The package ecosystem, as a package type of the purl specification, e.g. `cargo`, `npm`, `pypi`. |

Two items describe the same package if all three fields are equal, so the same crate at two versions yields two entries.

## Example

```json
{
  "version": 1,
  "blocks": [
    {
      "license": { "req": "Apache-2.0 WITH LLVM-exception", "name": null },
      "items": [
        {
          "description": "A package",
          "package": { "name": "foo", "version": "1.2.3", "ecosystem": "cargo" },
          "paths": [],
          "source": "https://example.com/foo",
          "notice": "Copyright (C) Foo.",
          "also_see": null,
          "inclusion": "always"
        }
      ],
      "text": "License text.",
      "notice": null
    }
  ]
}
```
