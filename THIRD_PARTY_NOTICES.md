# Third-party notices and provenance

## Project source

`bevy_looking_glass` is distributed under the MIT license in [LICENSE](LICENSE),
copyright (c) 2026 matsuvr and RusTuberV contributors. This covers the project's
Rust source, WESL shaders, examples, documentation and authored device catalog.

The initial renderer, calibration parser and catalog were extracted from
[RusTuberV commit a198c1b9](https://github.com/matsuvr/RusTuberV/tree/a198c1b9)
under the same [MIT license](https://github.com/matsuvr/RusTuberV/blob/a198c1b9/LICENSE).
The original copyright and permission notice are retained without alteration.
Later plugin work is recorded in this repository's Git history.

## Reference material

The following projects were consulted for interoperability. Their licenses
govern their own material; they are not relicensed by this project's MIT notice.
The source release does not bundle their JavaScript/TypeScript implementations.

- **Looking Glass WebXR**, revision
  `b44ed27fea197f28fbe4cb669bc350d141a9f8b1`: the
  [configuration source](https://github.com/Looking-Glass/looking-glass-webxr/blob/b44ed27fea197f28fbe4cb669bc350d141a9f8b1/src/LookingGlassConfig.ts)
  documents calibration fields, normalization and quilt conventions. That file
  carries copyright 2021 Google LLC and is licensed under
  [Apache-2.0](https://github.com/Looking-Glass/looking-glass-webxr/blob/b44ed27fea197f28fbe4cb669bc350d141a9f8b1/LICENSE).
  This crate implements the data format and optical equations in Rust/WESL;
  it does not include the WebXR polyfill, HoloPlay Core or their default lens data.
- **Looking Glass Bridge.js**: reference for local HTTP request envelopes,
  orchestration and display discovery. License reviewed at revision
  `f09f7f7510b09bb502d602c26907c958ed77075f`:
  [MIT, copyright (c) 2022 Looking Glass Factory](https://github.com/Looking-Glass/Bridge.js/blob/f09f7f7510b09bb502d602c26907c958ed77075f/LICENSE.md).
  `src/bridge.rs` is the Rust HTTP client; Bridge.js is not a Cargo dependency.
- **Bevy 0.20.0**: render/window APIs and custom projection examples, licensed
  under [MIT](https://github.com/bevyengine/bevy/blob/v0.20.0/LICENSE-MIT) OR
  [Apache-2.0](https://github.com/bevyengine/bevy/blob/v0.20.0/LICENSE-APACHE).
  Bevy shader modules are imported through Bevy's embedded asset paths.

`src/devices.json` is an authored catalog of numerical specifications and
capabilities. Its entries retain source URLs and a research date. Manufacturer
documentation, photographs, logos, firmware and factory calibration files are
not included. Device presets are not a claim of hardware-tested compatibility.

## Cargo dependencies

Dependencies are fetched by Cargo and are not vendored in this source release.
Their licenses remain separate from the license of this project. The direct
dependency versions in the release lockfile are:

| Dependency | Version | Upstream license |
| --- | --- | --- |
| Bevy | 0.20.0 | MIT OR Apache-2.0 |
| png | 0.18.1 | MIT OR Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.152 | MIT OR Apache-2.0 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |
| ureq | 3.4.2 | MIT OR Apache-2.0 |

On 2026-10-13, `cargo-deny 0.20.2` checked the locked graph with all features,
including example and target-specific dependencies, and reported `licenses ok`.
The allowed SPDX identifiers for that check were MIT, MIT-0, Apache-2.0,
BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, Zlib and CC0-1.0. No license
exceptions or package-specific overrides were used. This records the checked
lockfile, not future dependency resolutions or the licensing of an entire app.

Distributors of compiled applications must preserve the notices required by
the dependencies and assets actually included in their build. This source
release includes no compiled examples, DLLs, models or fonts.

## Separately installed software and names

Looking Glass Bridge is installed separately by the user and retains its
manufacturer's terms. This project does not redistribute Bridge, a Bridge SDK,
HoloPlay Core, or device calibration. Querying Bridge's local API does not
relicense that software or its returned data.

Looking Glass product names identify compatible hardware. This is an
independent project, not an official Looking Glass product; the MIT license
does not grant rights to third-party trademarks.
