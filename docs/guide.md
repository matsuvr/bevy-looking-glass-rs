# Integration guide

English | [日本語](guide.ja.md) | [README](../README.md) | [API quick start](api.md)

This guide describes the `0.1.x` API for Bevy 0.20. See [validation](validation.md)
for what was actually exercised on hardware. The [API quick start](api.md) contains
complete, rustdoc-testable examples.

## Device discovery and calibration

`bridge::discover() -> Result<Vec<bridge::Display>, bridge::Error>` is a synchronous,
window-independent query to Bridge's local HTTP API at `http://localhost:33334/`.
Call it before starting the app, or on a worker in an interactive app. Each request
has a five-second timeout and bypasses HTTP proxies; the whole discovery involves
multiple requests. The function opens an orchestration, requests
`available_output_devices`, and exits the orchestration, including after a parsing
failure. It does not install/start Bridge, write calibration files, or create windows.

| Result | Meaning |
| --- | --- |
| `Err(bridge::Error::Unavailable)` | The local Bridge request could not reach a usable service |
| `Ok(vec![])` | Bridge responded, but no calibrated light-field unit was found |
| `Err(bridge::Error::Response(_))` | An HTTP/protocol/device-data error was reported |
| `Ok(displays)` | Choose a display, normally by its stable serial |

A `Display` preserves the raw factory calibration string, serial, name, hardware
identifier, optional recommended Quilt, Bridge index, and desktop position.
Ordinary monitors and units with no calibration are omitted. Unknown model names
are accepted. **The Bridge index is not an OS monitor index.**

Pass the raw string's bytes to `parse_calibration`. Alternatively, the host can
read its own `visual.json` and pass those bytes directly. Parsing and optical
normalization do not perform I/O. Rendering does not require a running Bridge
after acquisition. Example programs require real calibration; synthetic values
in tests are not substitutes for a panel's calibration.

## Plugin and application responsibilities

Install `LookingGlassPlugin` after Bevy's asset/render/window plugins. The library
enables the rendering APIs it uses, not Bevy's complete default feature set. The
host supplies the window backend, PBR or other scene plugins, and any desired
post-processing features. Tonemappers needing LUTs require the matching Bevy LUT
features. Start with `DefaultPlugins` as shown in the quick start.

`LookingGlassControl::start(config)` starts or replaces the one managed output.
The last start/stop request in a frame wins. A rejected configuration keeps the
existing output and reports `LookingGlassStatus::error`. A successful request
clears that error. `stop()` releases the owned window, child cameras and asset
handles. Closing the output window releases its children. Retaining a frame image
handle in the host also retains that image.

The host owns scene animation, files, dialogs, settings persistence, keyboard
input, primary-window behavior, and application exit. The plugin does not modify
other cameras, task pools, or the shared `Core3d` executor. The example's keyboard
shortcuts and coordinated closing are not installed by the plugin.

`scene_layers` selects content visible to the view cameras. Reserve `display_layer`
exclusively for the interlace mesh; it must not overlap the scene or other cameras'
layers. This prevents the display image from appearing in scene views.

## Scheduling

| Host action | Where to schedule it |
| --- | --- |
| Start, stop, change Quilt preview | `Update`, before `LookingGlassSystems::Control` |
| Read output status; attach effects to newly created view cameras | `Update`, after `LookingGlassSystems::Control` |
| Publish world-space camera pose | Before `LookingGlassSystems::SyncViews` in `PostUpdate` |
| Custom per-view rendering to include in output | Render sub-app `Core3d`, before `LookingGlassRenderSet::Capture` |
| Request a Quilt capture | `Update` is convenient; its consumer runs in `PostUpdate` after `SyncViews` |
| Consume capture completion | Read `QuiltCaptureResult` messages in a host system |

`SyncViews` runs after transform propagation and before camera/visibility updates.
Capture follows post-processing and upscaling, once per view before the next
camera reuses the scratch target. Do not defer custom per-view copies until after
all cameras have rendered: the scratch image will already contain another view.

## Framing and live controls

Set `LookingGlassView::pose` to a `ViewPose`: a world-space transform looking down
local negative Z, positive focus distance, vertical FOV in radians, and near/far
clipping distances. The transform has unit scale and normalized rotation;
`0 < fov_y < pi` and `0 < near < far` (positive infinity is supported for `far`).
Panel aspect, not tile or preview-window aspect, determines scene projection.

`pose: None` disables scene cameras and hides retained scene pixels. Invalid poses
are reported in `LookingGlassStatus::pose_error`; correcting the pose resumes
scene output without restart. External images need no pose.

`view_pose_from_volume(&HologramVolume, fov_y)` is a pure calculation. The focal
plane's transform specifies the displayed center/orientation, and `height` is the
vertical span at that plane. Camera distance is `height / (2 * tan(fov_y / 2))`.
Near/far remain camera-relative clipping distances. Assign the returned pose to
change framing without recreating output resources.

`LookingGlassOptics::set_depth_scale(value)` overrides the initial baseline live.
Zero removes parallax; one uses the calibrated cone. `reset_depth_scale()` returns
to the active configuration's initial value. Changing the layout, source, or
other structural configuration requires a new `start(config)`.

## Native output placement

Set `config.display = Some(selected_display)` for automatic placement.
`plan_output_window` matches Bridge's physical desktop coordinates and native
resolution against Bevy's `MonitorInfo` entries, including negative coordinates.
Missing or ambiguous matches are reported rather than choosing another monitor.

On Windows and macOS, the plugin first creates a window at the planned native
size, then requests borderless fullscreen on that monitor after `WindowCreated`.
Only the mode changes during this transition. This avoids initial fullscreen on
the primary monitor on mixed-DPI desktops. If the selected monitor disappears
before creation completes, the pending output closes with an error. There is no
periodic hot-plug polling or automatic reconnection policy.

Leave `config.display = None` to use the host's `config.window` settings directly:
title, position, monitor/mode, size, and presentation policy. The constructor uses
native physical pixels with scale factor 1. For optical output, use the panel's
native resolution/orientation on an extended desktop, not mirroring or scaling.
Windowed or resized output is for preview only.

## View rendering, effects, and frame access

`ViewRenderSettings` controls exposure, tonemapping, dithering, light clustering,
direct/indirect drawing, HDR intermediates, and clear color on owned view cameras.
Final stored/output views are SDR and MSAA is off. HLD uses a white matte regardless
of the light-field clear-color setting.

Every real scene camera carries `LookingGlassViewCamera { output, index }`; its
owner carries `LookingGlassFrame { views, layout, metadata }`. Both are extracted
into the render world. `views` is an sRGB SDR array in optical view order.
`metadata.view_count` is the active count; `layout.count()` is the number of Quilt
slots, which may be greater for an external partial Quilt.

Attach Bevy effects to the public camera marker after `Control`. See the
[complete Bloom/DoF helper](api.md). Enable `config.view_render.hdr` for Bloom and
install Bevy's post-processing plugins. `LookingGlassSyncFocus` synchronizes DoF
with the reference pose's focus distance. Effects are applied to each view, never
as a blur over the final interlaced panel.

Bevy 0.20's DoF extractor does not accept custom projections. The integration uses
a settings-only child with a perspective FOV, then transfers Bevy's own DoF uniform
to the optical camera. The off-axis projection and culling frustum remain intact;
removing DoF removes the child. No separate blur engine or Bevy patch is used.

## Explicit depth generation

`ViewGeneration::Full` is the default and renders every view.
`ViewGeneration::DepthReprojection { rendered_views: n }` draws `n` evenly spaced
anchors including both ends; `2 <= n <= output view count`. The pure function
`plan_view_generation` selects the anchors and neighboring references.

Anchor color is captured after post-processing, with opaque depth copied before
the scratch target is reused. WESL forward-projects constant-depth pixel footprints
into missing views and combines both neighboring anchors with reverse-Z depth
testing. Anchor colors/depths have separate arrays, avoiding simultaneous reads
and writes of one texture array. Generated depth storage and uniforms are reused.

This is **opaque-scene reprojection**, not full scene rendering or temporal frame
interpolation. Transparent surfaces generally do not write matching depth.
Specular/reflection changes, blur across geometry, and surfaces hidden from both
anchors can produce artifacts. Uncovered pixels stay black. Explicitly choose
`Full` or more anchors as appropriate; there is no material classifier or automatic
fallback. Fewer scene cameras do **not** guarantee higher FPS: reprojection adds
GPU work, and speedups have not been established by the recorded checks.

## External Quilts and arrays

Set `config.source` to `LookingGlassSource::Quilt { image, metadata }` or
`LookingGlassSource::ViewArray { image, metadata }`; the default is `Scene`.
External sources create no scene cameras and need no reference pose. They use
`ViewGeneration::Full` because generation applies only to scene sources.

Start after the image exists in `Assets<Image>`. The image must be single-sampled,
single-mip, `Rgba8UnormSrgb`, 2D, and available in the render world. A Quilt needs
`COPY_SRC`; an array needs `COPY_SRC | TEXTURE_BINDING` and a `D2Array` view starting
at layer/mip zero. Its size and layers must agree with the metadata. Keep and
update the same image handle for streaming content; video decoding is the host's
responsibility.

`QuiltMetadata` contains full image size, columns, rows, active `view_count`, and
original `view_aspect`. Tile sizes use integer division. The content aspect is
independent of the tile's pixel aspect. View zero is bottom-left; unused edge
pixels lie at the top/right. Inactive slots are not sampled. Content is fit to the
panel preserving its original aspect. Quilts are copied tile-by-tile on the GPU;
external arrays are sampled directly. Quilt preview shows padding/inactive slots
as black.

```sh
cargo run --example quilt_input -- visual.json quilt.png 8 6 48 0.75
cargo run --example quilt_input -- visual.json quilt.png 8 6 48 0.75 --array
```

The `--array` example imports an array once on the CPU; an external GPU renderer
can supply its array without that import. F9 captures, F10 previews the Quilt,
and Escape stops output.

## Quilt PNG and fixed-time sequences

Call `QuiltCapture::request(CaptureRequest { output, frame, time })` before its
`PostUpdate` consumer. Requests are extracted with the same main-world frame even
when Bevy pipelines rendering on another thread. Unprepared GPU assets defer a
request until a completed frame is available. `frame` and `time` are **host-supplied
labels**, not a promise that a freely advancing simulation will be captured at an
earlier instant. Freeze the simulation until completion for fixed-time sequences.

Read `QuiltCaptureResult` messages containing `Result<CapturedQuilt, String>`.
Stopping/replacing output cancels pending requests that have not rendered. GPU
mapping failures are returned as errors. Only a request allocates a Quilt and
readback buffer; ordinary output does not assemble or read back a Quilt.

Active layers are copied into bottom-left-ordered tiles and read back
asynchronously. CPU output is top-left-oriented, tightly packed sRGB RGBA8;
GPU row-alignment padding is removed. Inactive slots/edge pixels are zero.
`CapturedQuilt::write_png` encodes to a host writer; `save_png` uses the exact path
the host supplies. Run encoding/file I/O on a worker when needed, not on the
renderer's hot path. `metadata.filename_suffix()` supplies `_qs8x6a0.75` naming;
store `view_count` separately for partial Quilts, since the suffix omits it.

```sh
cargo run --example capture_sequence -- visual.json output-directory
cargo run --example capture_sequence -- visual.json output-directory --generated
cargo run --example capture_sequence -- visual.json output-directory --hdr
cargo run --example capture_sequence -- visual.json output-directory --generated --hdr
```

The example holds each pose until capture completes, then advances frame time at
30 Hz independently of wall-clock rendering/readback. It writes four PNGs on
workers and exits after encoding. `--generated` renders three of eight views;
`--hdr` enables Bloom/DoF. There is no video encoder or FFmpeg dependency.

## Renderer and device catalog

Parallel off-axis cameras share a small scratch target. The first retains Bevy's
output blit to establish its attachment; later cameras skip that blit. Each
completed view is copied into an array layer. Interlacing and virtual Quilt
preview sample the array directly, without a Quilt texture, assembly camera, or
tile sprites on the live path. HDR intermediates are converted to SDR before
storage. Raw calibration is normalized on the CPU; the shader implements RGB
stripes and cell-pattern modes 0 through 4.

`device_presets()` reads `src/devices.json`, including specification sources and
the research date. `DevicePreset::light_field_output(calibration)` uses the
preset's reference grid and requires matching native panel dimensions. Or use
`ViewLayout` for a custom resolution. The catalog covers Go, Portrait, original
8.9/15.6/Pro, current/legacy 16/27/32/65-inch variants, and HLD 16/27/86. These are
specification presets, not evidence of testing on those units.

`hld_output()` uses a single portrait view and the catalog's safe area, with no
lens calibration. HLD is not multiview lenticular output. musubi is catalog
information only, without live desktop output.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| Bridge unavailable / empty device list | Bridge is running; both video and USB data are connected; distinguish transport error from an empty result |
| Start did not create output | Inspect `LookingGlassStatus::error` after `Control`; check image loading, metadata, and selected-monitor match |
| Output is black for a scene | Supply a valid pose; inspect `pose_error`; check scene layers, content, and lighting |
| Only the ordinary preview moves | Update `LookingGlassView::pose` as well as the host camera |
| Incorrect apparent depth or interlacing | Use the correct individual calibration, native panel pixels, orientation, and extended desktop; exit Quilt preview |
| Artifacts with generation | Explicitly use `Full` or more anchors; see the opaque-scene limitations |
| Bloom missing | Enable HDR on the view configuration and Bevy's effect plugin/features before starting |
| Capture has not completed | Keep processing frames/messages; check asset readiness and output status; do not freeze the Bevy event loop |

When filing an issue, useful details are crate/Bevy version, OS/GPU, device model,
input/generation mode, the relevant error, and a small reproducer. They are helpful
context, not a mandatory submission checklist. No response or fix is promised.

## References

- [Quilt conventions](https://lfdocs.lookingglassfactory.com/keyconcepts/quilts)
- [Bridge SDK scope](https://lfdocs.lookingglassfactory.com/software/looking-glass-bridge-sdk)
- [Official Bridge.js HTTP protocol](https://github.com/Looking-Glass/Bridge.js/tree/main/src/library)
- [Native Windows display settings](https://lfdocs.lookingglassfactory.com/software/looking-glass-bridge/display-settings-on-windows)
- [Calibration file instructions](https://lfdocs.lookingglassfactory.com/software/index/unity-plugin-4.0-alpha)
- [Calibration reference revision](https://github.com/Looking-Glass/looking-glass-webxr/blob/b44ed27fea197f28fbe4cb669bc350d141a9f8b1/src/LookingGlassConfig.ts)
- [Bevy 0.20 custom projection example](https://github.com/bevyengine/bevy/blob/v0.20.0/examples/camera/custom_projection.rs)

See [third-party notices](../THIRD_PARTY_NOTICES.md) for licenses and provenance.
