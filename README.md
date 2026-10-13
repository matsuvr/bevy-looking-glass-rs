# bevy_looking_glass

English | [日本語](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/README.ja.md)

An **unofficial, experimental Bevy plugin** for displaying 3D scenes on Looking
Glass hardware. A personal research project: small, direct, and provided without
support commitments. It is not an official Looking Glass SDK or a claim of
compatibility with every display.

The crate's CPU implementation is safe Rust; GPU shaders are embedded WESL. It
does not link or redistribute the Looking Glass Bridge SDK, C/C++ bindings, or
JavaScript. Optional device discovery talks to a **separately installed Looking
Glass Bridge** over local HTTP. Once calibration is acquired, live rendering runs
directly in Bevy and stays on the GPU. Requested PNG captures use asynchronous
readback and Rust PNG encoding.

## What it does

- Off-axis multiview rendering, texture-array interlacing, virtual Quilt preview,
  live framing/depth adjustment, and selected-monitor fullscreen placement.
- Optional depth-based view generation, per-view Bevy Bloom/DoF integration,
  external Quilt/texture-array input, and on-demand Quilt PNG/sequence capture.
- A hardware specification catalog and single-view HLD output. Catalog entries
  are presets, **not a hardware-tested compatibility list**.

One Bevy app manages one output through this plugin. Final output is SDR and
MSAA is off. No automatic quality fallback, video codec, cross-view GPU
instancing, multi-device simultaneous output, or Unity editor tooling is included.
Depth generation is an explicit opaque-scene mode; use full rendering for scenes
whose transparency or view-dependent effects require it.

## Compatibility and prerequisites

| Item | Scope |
| --- | --- |
| Crate API | `0.1.x`; breaking API changes move to a new minor version |
| Bevy | `0.20` |
| Rust | `1.99` or newer; the repository uses the stable toolchain |
| Main intended hosts | Windows and macOS; see the actual checks below |
| Light-field output | A connected panel, its real factory calibration, and native-pixel video output |
| Automatic discovery | Installed/running Bridge, with video and USB data connections |

Your application supplies Bevy's window backend and any scene/effect features it
uses. `bevy = "0.20"` with default features is the simplest starting point; the
library itself does not enable Bevy's full default feature set. The host also
needs the normal build prerequisites for its chosen Bevy features.

For optical output, the framebuffer must map one-to-one to the panel's physical
pixels: use an extended desktop with the correct resolution and orientation.
A resized window is a preview, not calibrated optical output. Bridge is not
needed for rendering when the host supplies its own calibration bytes.

## Installation

The existing [v0.1.0 release](https://github.com/matsuvr/bevy-looking-glass-rs/releases/tag/v0.1.0)
is a **GitHub source release**, not a crates.io release. This documentation update
does not publish the crate. Until a registry release is announced, use:

```toml
[dependencies]
bevy = "0.20"
bevy_looking_glass = { git = "https://github.com/matsuvr/bevy-looking-glass-rs", tag = "v0.1.0" }
```

After a `0.1.x` release has actually been published on crates.io, replace the Git
entry with the following registry entry (not an alternative to use before then):

```toml
bevy_looking_glass = "0.1"
```

## First run

With Bridge running and a calibrated panel connected, run the model-free example
from a checkout. It selects the first discovered display; applications can select
by serial instead.

```sh
git clone https://github.com/matsuvr/bevy-looking-glass-rs.git
cd bevy-looking-glass-rs
cargo run --example minimal -- --bridge
```

Alternatively, provide your panel's `visual.json` explicitly:

```sh
cargo run --example minimal -- /path/to/visual.json
```

The file-input path starts a smaller preview window. Move it to the correct panel
and fullscreen it for optical output. `--bridge` requests native placement.
The scene contains a foreground cube, a sphere on the focus plane, and a background
cube. Arrow keys orbit/dolly; PageUp/PageDown change depth. With the output window
focused, F10 toggles Quilt preview, F11 toggles fullscreen, and Escape stops output.
Closing the main window also closes the example's output. These are **example
policies**, not keyboard or application-exit behavior imposed by the plugin.

For integration into your own application, see the
[complete cube example and API overview / 完全な導入例とAPI概要](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/api.md).
It includes a scene and light, not just an empty renderer configuration. The same
page becomes the crate's rustdoc landing page; `cargo test --doc` compiles its
`no_run` example without opening a window or contacting Bridge.

## Documentation

| Topic | English | 日本語 |
| --- | --- | --- |
| Integration, scheduling, effects, input, capture, troubleshooting | [Guide](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/guide.md) | [利用ガイド](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/guide.ja.md) |
| API quick start | [Bilingual API overview](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/api.md) | 同左 |
| Tested scope and historical records | [Validation](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/validation.md) | 同ファイルに併記 |
| Maintainer's publication steps | [Publishing](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/publishing.md) | 同ファイルに併記 |
| Release changes | [Changelog](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/CHANGELOG.md) | 同ファイルに併記 |

Generate the API reference locally with `cargo doc --no-deps --lib --open`.
[docs.rs](https://docs.rs/bevy_looking_glass) is the intended hosted reference after
registry publication and a successful docs.rs build; it is not evidence of a
completed publication.

## What has been checked

The recorded v0.1.0 Windows review reports 40 library tests, three example builds,
Clippy, and compilation of the packaged archive. Earlier Windows/RTX 4090/Vulkan
checks exercised generated views, HDR Bloom/DoF, external inputs, and PNG output
on ordinary desktop windows. PR #2 separately records macOS/Apple M4/Portrait
placement and an avatar visible on the physical panel.

These are **dated reports**, not new checks performed by this documentation
change. Full optical-quality assessment across devices, Windows physical-panel
placement, Linux runtime, camera/NDI concurrency, and measured reprojection
speedups are not established by them. See the
[validation record](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/docs/validation.md)
for exact scope and source links. The earlier RusTuberV frame-rate measurements
are not a performance guarantee for this crate.

## License and project scope

Project source: [MIT](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/LICENSE).
See [third-party notices](https://github.com/matsuvr/bevy-looking-glass-rs/blob/main/THIRD_PARTY_NOTICES.md)
for RusTuberV provenance, reference-material licenses, and dependency notices.
Dependencies and separately installed Looking Glass software retain their own
terms. SDK binaries, factory calibration files, models, and fonts are not bundled.
Looking Glass names identify compatible hardware; this project is independent of
the manufacturer.

Issues and pull requests are welcome, but responses, fixes, and continued
maintenance are not promised. Keep changes focused: pure calculations apart from
ECS/GPU/I/O effects, without introducing an application framework or automatic
fallback behavior. There is no requirement to support every OS or device before
contributing a useful, accurately documented improvement.
