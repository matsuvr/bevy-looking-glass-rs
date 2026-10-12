# bevy_looking_glass

A small Bevy **0.20** plugin for direct Looking Glass display output. CPU code is
safe Rust; GPU code is embedded WESL. No Looking Glass Bridge SDK, C/C++ bindings,
DLL loading, or JavaScript is added by this crate. Live output stays on the GPU;
optional quilt captures use asynchronous readback and pure Rust PNG encoding.

Install and run Looking Glass Bridge and connect the display's video and USB
data connections. `bridge::discover()` retrieves attached light-field displays,
their factory calibration and recommended quilts through Bridge's local HTTP API
on Windows and macOS. Applications can use those values directly without asking
users to find or create a calibration file.

## Usage

```toml
[dependencies]
bevy = "0.20"
bevy_looking_glass = { git = "https://github.com/matsuvr/bevy-looking-glass-rs" }
```

```rust,no_run
use bevy::{prelude::*, camera::visibility::RenderLayers};
use bevy_looking_glass::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let display = bridge::discover()?.into_iter().next()
        .ok_or("no calibrated Looking Glass is connected")?;
    let quilt = display.quilt.as_ref().ok_or("Bridge returned no default quilt")?;
    let layout = ViewLayout::new(
        quilt.columns,
        quilt.rows,
        quilt.width.checked_div(quilt.columns).ok_or("zero quilt columns")?,
        quilt.height.checked_div(quilt.rows).ok_or("zero quilt rows")?,
    )?;
    let calibration = parse_calibration(display.calibration.as_bytes())?;
    let mut config = LookingGlassConfig::new(
        LookingGlassOutput::LightField {
            calibration,
            layout,
            depth_scale: 1.0,
        },
        RenderLayers::layer(0), // scene
        1,                     // reserved exclusively for the output mesh
    );
    config.display = Some(display); // match the native OS monitor and fullscreen
    let mut control = LookingGlassControl::default();
    control.start(config);
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LookingGlassPlugin)
        .insert_resource(control)
        .insert_resource(LookingGlassView {
            pose: Some(ViewPose {
                transform: Transform::from_xyz(0.0, 0.0, 5.0),
                focus_distance: 5.0,
                fov_y: 45_f32.to_radians(),
                near: 0.1,
                far: 100.0,
            }),
        })
        // Add your scene/animation systems here.
        .run();
    Ok(())
}
```

For a complete scene requiring no external model:

```sh
cargo run --example minimal -- /path/to/visual.json
```

The example places a red cube in front of the focus plane, a green sphere on it,
and a blue cube behind it. Arrow keys orbit/dolly; PageUp/Down adjust depth live. With the output window focused,
F10 toggles the quilt, F11 toggles fullscreen, and Escape stops output. Closing
the main window closes the example's output. These policies are example code,
not plugin behavior. The sample requires real calibration; synthetic calibration
is confined to tests and desktop pipeline validation.

## Bridge discovery

`bridge::discover() -> Result<Vec<bridge::Display>, bridge::Error>` is a synchronous,
window-independent query. Call it on a background worker in an interactive app.
Each HTTP request has a five-second timeout and bypasses HTTP proxies.

- `Err(bridge::Error::Unavailable)` means the local Bridge API cannot be reached.
  The caller chooses its UI message; RusTuberV displays `Bridgeが見つかりません`.
- `Ok(vec![])` means Bridge responded but no calibrated light-field unit was found.
- Each `Display` contains its serial, name, hardware model, unmodified raw
  `visual.json` string, optional default `Quilt` geometry, Bridge device `index`,
  and physical desktop `window_position` (including negative coordinates). Select by serial
  when multiple units are attached. Unknown model identifiers are accepted.
- `Err(bridge::Error::Response(_))` reports failed or invalid Bridge responses.

Discovery enters an orchestration, requests `available_output_devices`, and exits
the session even if device parsing fails. Ordinary monitors and units without
calibration are omitted. The function does not launch/install Bridge, write files
or start output. `parse_calibration` still accepts raw bytes supplied directly by
the host; rendering does not require a running Bridge after acquisition.

## Integration contract

- Install after Bevy's render/asset/window plugins. Supply the window backend in
  your app (e.g. `bevy_winit` plus the appropriate Linux backend). Normal plugin
  dependencies enable only needed rendering APIs, not Bevy's complete default
  feature set. The example also enables PBR, Winit and tonemapping LUTs.
- One instance manages one output. `LookingGlassControl::start(config)` replaces
  it; `stop()` releases the window, cameras and owned asset handles. Invalid
  requests preserve existing output and report `LookingGlassStatus::error`.
  Inspect `status.window` for the output entity. Closing it releases its children.
- The host owns scene animation, files, dialogs, settings persistence, keyboard
  input and application exit. The plugin does not find or modify a primary window,
  other cameras, task pools or the shared Core3d executor.
- Write `LookingGlassView::pose` each time your camera changes. The transform is
  world space, with unit scale and normalized rotation; focus distance is positive,
  `0 < fov_y < pi`, `0 < near < far`. `pose: None` disables the scene cameras and
  hides old scene pixels. Invalid poses are reported in `status.pose_error` and
  recover when corrected, without restarting. External sources do not require a pose. Panel aspect
  determines scene projection, not tile/preview aspect.
- `scene_layers` selects the scene. Reserve `display_layer` for the interlace mesh;
  it must not intersect scene layers or any other application's camera layers.
- `ViewRenderSettings` configures exposure, tonemapping, dithering, light clusters,
  direct/indirect draws, HDR intermediates and clear color on owned view cameras.
  Final output is SDR
  and MSAA is off. Enable Bevy LUT features for tonemappers that require them.
  HLD uses a white matte independently of the light-field clear color setting.
- `config.window` is a Bevy `Window`: set title, position, monitor/mode, resolution
  and presentation policy there. The constructor requests native physical pixels
  with scale factor 1. Windowed/scaled output is only a preview. For optics the
  actual framebuffer and panel must have one-to-one native pixel correspondence;
  use an extended desktop in the correct orientation, not display mirroring/scaling.
- Schedule request producers before `LookingGlassSystems::Control` in `Update`;
  read status after it. Publish camera poses before `LookingGlassSystems::SyncViews`
  in `PostUpdate` (the plugin reads after transform propagation).
- Custom render passes belong before `LookingGlassRenderSet::Capture` in the
  render sub-app's `Core3d` schedule. Capture already follows post-processing and
  upscaling. It must run once per view before the next camera reuses scratch.

## Rendering

Parallel off-axis cameras share one small render target and copy completed views
into a texture array. The first view retains Bevy 0.20's output blit to establish
the shared attachment; remaining cameras use `CameraOutputMode::Skip`. Interlacing
samples the array directly. Quilt preview uses the same array without a quilt
texture, assembly camera or tile sprites. View zero is bottom-left in a quilt.

Raw lens values are normalized on the CPU; RGB stripes and cell-pattern modes
0–4 use the embedded shader. Bridge is used only for optional device discovery;
rendering runs directly in Bevy. Optional depth reprojection reduces the scene
camera count; cross-view GPU instancing and Bevy 0.19 compatibility are not included.

## Live framing and output placement

`view_pose_from_volume(&HologramVolume, fov_y)` returns a validated `ViewPose`.
The focal plane's transform specifies the center and orientation of the displayed
space; `height` specifies its vertical span. Camera distance is
`height / (2 * tan(fov_y / 2))`. Near/far are camera-relative clipping distances.
Assign the result to `LookingGlassView::pose` to change framing without recreating
cameras, textures or windows. `LookingGlassOptics::set_depth_scale(value)` changes
the baseline live; `reset_depth_scale()` returns to the output's initial value.

Set `config.display = Some(display)` using a selected Bridge display for native
placement and borderless fullscreen. `plan_output_window` matches physical desktop
position **and native resolution** to `MonitorInfo` entries from Bevy's monitors.
Bridge's index is not an OS monitor index. Missing/ambiguous native matches report
an error and preserve existing output. On Windows and macOS the plugin first
creates a window at the planned native size, then requests fullscreen on the
selected monitor after Bevy reports native window creation. This avoids entering
fullscreen on the primary screen during creation, including mixed-DPI desktops.
It changes only the output window's mode at that point, so an OS-reported position
cannot trigger another move during the fullscreen transition. If the selected
monitor disappears before creation completes, output closes with an error.
The plugin does not perform hot-plug polling. Leave `display` as `None` to use `config.window`.
`cargo run --example minimal -- --bridge` demonstrates selected-display placement.

## Per-view post-processing and frame access

Every real scene camera has public `LookingGlassViewCamera { output, index }`.
The output entity has `LookingGlassFrame { views, layout, metadata }`. Both are
extracted into the render world too. The frame image is an sRGB SDR array in
optical order; `metadata.view_count` is the active count, while `layout.count()`
is the number of virtual quilt slots. Retaining its handle retains the image.

Attach ordinary Bevy effects after `LookingGlassSystems::Control` in `Update`:

```rust,ignore
fn effects(mut commands: Commands, cameras: Query<Entity, Added<LookingGlassViewCamera>>) {
    for camera in &cameras {
        commands.entity(camera).insert((
            Bloom::default(), DepthOfField::default(), LookingGlassSyncFocus,
        ));
    }
}
```

Set `config.view_render.hdr = true` for Bloom. Enable Bevy's post-processing
plugins (included in `DefaultPlugins`). The plugin captures each view after
post-processing and tonemapping, converting float HDR buffers through an SDR
render attachment before storing them. It never blurs the interlaced panel.
`LookingGlassSyncFocus` synchronizes DoF with `ViewPose::focus_distance`.
Bevy 0.20's DoF extractor rejects custom projections: a settings-only child gives
its extractor a perspective FOV, then transfers Bevy's own uniform to the optical
camera. The off-axis projection and its culling frustum remain intact. Removing
DoF releases that child. No separate blur engine or Bevy patch is required.

## Explicit depth view generation

The default `config.view_generation = ViewGeneration::Full` draws every view.
Select `ViewGeneration::DepthReprojection { rendered_views: n }` to draw only
`n` evenly spaced anchors, including both ends of the calibrated cone. `n` must
be between two and the output view count. `plan_view_generation` is a pure,
fallible function returning rendered indices and neighboring anchor references.

Anchor color is saved after post-processing and opaque depth before scratch is
reused. A WESL pass reconstructs each source pixel in homogeneous coordinates,
projects its footprint into each missing view, and depth-composites both neighboring
anchors with reverse-Z testing. It does not blend adjacent images without geometry.
Colors and depths use separate compact anchor arrays, avoiding simultaneous
sampling/writing of the same array. Generated depth and uniform storage are reused.

This is explicit opaque-scene reprojection. Transparent surfaces generally do not
write matching depth; reflection/specular changes, blur extending over geometry,
and surfaces hidden from both anchors can produce artifacts. Uncovered pixels
stay black. Use `Full` for such scenes or increase the anchor count. Scene camera
reduction is not a measured speedup guarantee: forward projection adds GPU work.
No automatic scene classification or fallback is performed.

## External quilts and arrays

Set `config.source` to `LookingGlassSource::Quilt { image, metadata }` or
`LookingGlassSource::ViewArray { image, metadata }`. The default is `Scene`.
External sources create no scene cameras and require no `LookingGlassView::pose`.
Start after the image is loaded in `Assets<Image]`. Images must be single-sampled,
single-mip `Rgba8UnormSrgb` 2D textures available in the render world. Quilts need
`COPY_SRC`; arrays need `COPY_SRC | TEXTURE_BINDING` and a `D2Array` view starting
at layer/mip zero. Keep the handle and update that image for streaming content.

`QuiltMetadata` records full image size, columns, rows, active `view_count` and
original `view_aspect`. The tile size is integer division; it is independent of
the original content aspect. View zero is bottom-left. Unused edge pixels lie at
the top/right; inactive tiles are not sampled. Content is fit to the panel while
preserving its original aspect. Quilts are copied tile-by-tile on the GPU;
external arrays are sampled directly. Preview displays padding/inactive tiles black.

```sh
cargo run --example quilt_input -- visual.json quilt.png 8 6 48 0.75
cargo run --example quilt_input -- visual.json quilt.png 8 6 48 0.75 --array
```

The second example path builds an external array once on the CPU to demonstrate
that source variant. A renderer supplying its own array does not need this import
step. F9 captures, F10 previews the virtual quilt, and Escape stops output.

## Quilt PNG and fixed-time sequences

Call `QuiltCapture::request(CaptureRequest { output, frame, time })` before its
`PostUpdate` consumer (which runs after `SyncViews`). The request is extracted with
the same main-world frame, including when Bevy pipelines rendering on another thread.
GPU assets still preparing defer the request to the next available completed frame.
Read `QuiltCaptureResult` messages for `Result<CapturedQuilt, String>`; the original
frame number and time are preserved. Stopping/replacing output cancels pending
unrendered requests. GPU map failures are reported as errors.

Only requests allocate a quilt/readback buffer. The GPU copies active array layers
to bottom-left-ordered quilt tiles, then asynchronously reads back. Alignment
padding is removed; inactive tiles/edge pixels are zero. `CapturedQuilt::write_png`
or `save_png` performs lossless encoding/file I/O when called by the host, preferably
on a worker. `metadata.filename_suffix()` supplies `_qs8x6a0.75` naming. The suffix
contains geometry/aspect, so preserve `view_count` separately for partial quilts.

```sh
cargo run --example capture_sequence -- visual.json output-directory
cargo run --example capture_sequence -- visual.json output-directory --generated
cargo run --example capture_sequence -- visual.json output-directory --hdr
```

The example freezes each simulation pose until its capture completes and advances
using the frame number at 30 Hz, independently of wall-clock rendering/readback time.
It writes four PNGs on workers and exits after encoding completes. `--hdr` attaches
Bloom/DoF; `--generated` renders three of eight views. There is no video encoder
or FFmpeg dependency.

## Catalog and HLD

`device_presets()` reads `src/devices.json`. It preserves manufacturer specification
references and the research date. `DevicePreset::light_field_output(calibration)`
uses that preset's reference grid and rejects a calibration with different native
panel dimensions. Or use `ViewLayout` directly for custom view resolution.

Catalog entries cover Go, Portrait, original 8.9/15.6/Pro, current and legacy 16,
27, 32, 65-inch light-field variants, and HLD 16/27/86. These are **specification
presets, not a list of hardware-tested devices**. `hld_output()` uses one portrait
view and the catalog's safe area without any lens calibration. musubi is catalog
information only: it does not offer live desktop output.

## Validation scope

Extracted from [RusTuberV a198c1b](https://github.com/matsuvr/RusTuberV/tree/a198c1b9)
under MIT, preserving the array/copy/interlace path. Previous RusTuberV measurements
of roughly 60 fps concern a specified Windows desktop workload with synthetic
calibration. They do not guarantee plugin performance or physical-panel optical
correctness. Physical Looking Glass panels and all-device compatibility remain
unverified. See RusTuberV's [integration record](https://github.com/matsuvr/RusTuberV/blob/main/docs/looking-glass.md)
for the historical workload and limitations.

Extraction validation on Windows / RTX 4090 / Vulkan (2026-10-11): 21 library
tests and all-target Clippy passed outside the RusTuberV workspace, with the
registry's unpatched `bevy_window`. A separate minimal consumer rendered the
model-free example using synthetic calibration: quilt, camera movement,
interlacing, a completely black output for `pose: None`, and stop/exit succeeded.
Screenshots were inspected; moving the camera changed the quilt pixels. Bevy
logged late events for the closed window; the consumer exited with code 0.
The packaged crate also passed `cargo publish --dry-run`, including compilation
with only normal dependencies; the WESL and device JSON are in the archive.
Physical-panel optics, macOS execution of this extracted crate, and performance
across devices remain unverified.

Bridge discovery validation (2026-10-12): all 25 library tests and all-target
Clippy with warnings denied passed. The new tests cover unavailable Bridge,
session cleanup after parsing errors, multiple serials and preservation of raw
optical cells and unknown model identifiers. The relocated HTTP module retrieved
a connected Portrait's calibration and 3360-square, 8 x 6 default quilt. The same
source passed an isolated `aarch64-apple-darwin` check. Checking the full Bevy crate
for that target from Windows stopped at Bevy's `blake3` dependency because the
Mac C compiler was unavailable. Mac hardware execution remains unverified.

High-priority feature validation (2026-10-12): 36 library tests and all-target
Clippy with warnings denied passed, as did formatting, whitespace checks and
`cargo publish --dry-run` (packaged normal dependencies, no upload). GPU execution
on Windows / RTX 4090 / NVIDIA 617.14 / Vulkan produced four PNG frames with
full rendering, three-anchor depth generation, HDR Bloom/DoF and combined
HDR/generation. A render-world check found Bevy DoF uniforms on all three anchors
while preserving their custom projections. The combined sequence produced four
distinct files whose SHA256 values matched a repeated run at the same frame times.

External quilt and array consumers with five active views in a 3 x 2 grid at
11 x 9 pixels passed exact RGBA readback comparisons, including unused top/right
pixels and an inactive tile, without a scene pose. Frame/time identity was retained.
PNG results from generation and HDR were inspected. Validation used synthetic
calibration on ordinary desktop windows. Physical optics, automatic fullscreen
placement on a connected panel, macOS execution and performance gains remain
unmeasured. The reprojection's disocclusion/transparent-material limitations above
are part of its explicit mode contract.

Use current stable Rust (minimum 1.99), Bevy 0.20, and normal Cargo checks:

```sh
cargo fmt --all -- --check
cargo test --lib -j 1
cargo clippy --all-targets -j 1
cargo package --list
cargo publish --dry-run
```

## License and provenance

MIT; see [LICENSE](LICENSE). Copyright notices from RusTuberV are retained.
The code directly implements geometry and pixel-mapping equations; manufacturer
SDK binaries and JavaScript source are not redistributed.

- [Raw calibration and quilt conventions](https://github.com/Looking-Glass/looking-glass-webxr/blob/b44ed27fea197f28fbe4cb669bc350d141a9f8b1/src/LookingGlassConfig.ts)
- [Quilt specifications](https://lfdocs.lookingglassfactory.com/keyconcepts/quilts)
- [Calibration file instructions](https://lfdocs.lookingglassfactory.com/software/index/unity-plugin-4.0-alpha)
- [Native panel resolutions/orientation](https://lfdocs.lookingglassfactory.com/software/looking-glass-bridge/display-settings-on-windows)
- [Bridge SDK scope](https://lfdocs.lookingglassfactory.com/software/looking-glass-bridge-sdk)
- [Official Bridge.js HTTP protocol](https://github.com/Looking-Glass/Bridge.js/tree/main/src/library)
- [Bevy custom projections](https://github.com/bevyengine/bevy/blob/v0.20.0/examples/camera/custom_projection.rs)

Per-device specification sources are included in `src/devices.json`.
