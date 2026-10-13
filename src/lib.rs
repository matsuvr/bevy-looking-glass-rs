#![doc = include_str!("../docs/api.md")]
#![forbid(unsafe_code)]

pub mod bridge;
mod calibration;
mod capture;
mod devices;
mod generation;
mod generation_gpu;
mod optics;
mod placement;
mod postprocess;
mod render;
mod source;
mod view_array;

pub use calibration::{Calibration, parse_calibration};
pub use capture::{
    CaptureRequest, CapturedQuilt, QuiltCapture, QuiltCaptureResult, QuiltSaveError,
};
pub use devices::{DevicePreset, DisplayFormat, QuiltPreset, device_presets};
pub use generation::{GeneratedView, ViewGeneration, ViewGenerationPlan, plan_view_generation};
pub use optics::ViewLayout;
pub use optics::{HologramVolume, view_pose_from_volume};
pub use placement::{MonitorInfo, OutputWindowPlan, plan_output_window};
pub use source::{LookingGlassFrame, LookingGlassSource, QuiltMetadata};

use bevy::camera::{ClearColorConfig, Exposure, visibility::RenderLayers};
use bevy::light::cluster::ClusterConfig;
use bevy::prelude::*;
use bevy::render::view::{DebandDither, Tonemapping};
use bevy::render::{
    RenderApp,
    extract_component::{ExtractComponent, ExtractComponentPlugin},
};
use bevy::window::WindowResolution;

/// Invalid calibration or output configuration.
#[derive(Debug, thiserror::Error)]
pub enum LookingGlassError {
    /// The document does not match the raw calibration or catalog schema.
    #[error("invalid Looking Glass JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// Values cannot describe the requested projection or pixel layout.
    #[error("invalid Looking Glass configuration: {0}")]
    Invalid(&'static str),
}

/// Register rendering systems without allocating output windows or textures.
pub struct LookingGlassPlugin;

impl Plugin for LookingGlassPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LookingGlassControl>()
            .init_resource::<LookingGlassView>()
            .init_resource::<LookingGlassOptics>()
            .init_resource::<LookingGlassStatus>();
        app.add_plugins(ExtractComponentPlugin::<LookingGlassViewCamera>::default());
        render::install(app);
        postprocess::install(app);
        capture::install(app);
    }
}

/// Main-world scheduling hooks for integrations.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LookingGlassSystems {
    /// Process requests in `Update`. Run UI/request producers before this set.
    Control,
    /// Read the reference pose in `PostUpdate`, after transform propagation.
    SyncViews,
}

/// Render-world hook in Bevy's per-view `Core3d` schedule.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LookingGlassRenderSet {
    /// Capture after post-processing/upscaling, before the next camera reuses scratch.
    /// Order custom scene passes before this set in the render sub-app.
    Capture,
}

/// Display transport and its optical configuration.
#[derive(Clone, Debug)]
pub enum LookingGlassOutput {
    /// A calibrated multiview lenticular display.
    LightField {
        /// Virtual quilt grid and per-view resolution.
        layout: ViewLayout,
        /// Calibration belonging to the physical display.
        calibration: Calibration,
        /// Baseline multiplier: zero removes parallax, one uses the calibrated cone.
        depth_scale: f32,
    },
    /// A single-view HLD with a white matte, without lenticular calibration.
    Hld {
        /// Native portrait output resolution.
        screen: UVec2,
        /// Fraction of the panel occupied by the image, in each axis.
        safe_area: Vec2,
    },
}

impl LookingGlassOutput {
    /// Native panel size in physical pixels.
    pub fn screen_size(&self) -> UVec2 {
        match self {
            Self::LightField { calibration, .. } => calibration.screen_size(),
            Self::Hld { screen, .. } => *screen,
        }
    }

    fn layout(&self) -> ViewLayout {
        match self {
            Self::LightField { layout, .. } => *layout,
            Self::Hld { screen, .. } => ViewLayout {
                columns: 1,
                rows: 1,
                view_width: screen.x,
                view_height: screen.y,
            },
        }
    }

    fn validate(&self) -> Result<(), LookingGlassError> {
        match self {
            Self::LightField { depth_scale, .. }
                if !depth_scale.is_finite() || *depth_scale < 0.0 =>
            {
                Err(LookingGlassError::Invalid(
                    "depth_scale must be finite and nonnegative",
                ))
            }
            Self::Hld { screen, safe_area }
                if screen.x == 0
                    || screen.y == 0
                    || !safe_area.is_finite()
                    || safe_area.min_element() <= 0.0
                    || safe_area.max_element() > 1.0 =>
            {
                Err(LookingGlassError::Invalid(
                    "HLD dimensions and safe area must be positive; safe area must not exceed one",
                ))
            }
            _ => Ok(()),
        }
    }
}

/// Rendering policy for scene cameras. Final views are SDR, MSAA off.
#[derive(Clone, Default)]
pub struct ViewRenderSettings {
    /// Use HDR linear intermediate buffers, needed by Bevy Bloom. Tonemapping
    /// and final SDR conversion occur before storing each view in the array.
    pub hdr: bool,
    /// Scene exposure.
    pub exposure: Exposure,
    /// Scene tonemapping (enable Bevy's LUT features when the selected method needs them).
    pub tonemapping: Tonemapping,
    /// Dithering of the scene views.
    pub deband_dither: DebandDither,
    /// Light clustering strategy.
    pub cluster_config: ClusterConfig,
    /// Use direct draws instead of GPU indirect drawing for these cameras.
    pub no_indirect_drawing: bool,
    /// Background of light-field views. HLD views always use a white matte.
    pub clear_color: ClearColorConfig,
}

/// Configuration for a single output window and its owned rendering assets.
#[derive(Clone)]
pub struct LookingGlassConfig {
    /// Scene rendering, an external quilt, or an external view array.
    pub source: LookingGlassSource,
    /// Explicit scene view generation policy; transparent scenes should use Full.
    pub view_generation: ViewGeneration,
    /// Optional Bridge display for automatic native monitor placement.
    pub display: Option<bridge::Display>,
    /// Transport and calibration.
    pub output: LookingGlassOutput,
    /// Scene layers visible to the multiview cameras.
    pub scene_layers: RenderLayers,
    /// Exclusive layer for the interlace mesh. Keep all other cameras off this layer.
    pub display_layer: usize,
    /// Output window title, size, position, mode and presentation settings.
    pub window: Window,
    /// Policy for scene cameras, without affecting the application's other cameras.
    pub view_render: ViewRenderSettings,
}

impl LookingGlassConfig {
    /// Create a native-resolution window. Move/fullscreen it using the host app.
    pub fn new(
        output: LookingGlassOutput,
        scene_layers: RenderLayers,
        display_layer: usize,
    ) -> Self {
        let screen = output.screen_size();
        Self {
            source: LookingGlassSource::Scene,
            view_generation: ViewGeneration::Full,
            display: None,
            output,
            scene_layers,
            display_layer,
            window: Window {
                title: "Looking Glass".into(),
                resolution: WindowResolution::new(screen.x, screen.y)
                    .with_scale_factor_override(1.0),
                ..default()
            },
            view_render: default(),
        }
    }
}

#[derive(Resource)]
struct ActiveOutput(LookingGlassConfig);

/// Reference camera in world coordinates. Scale must be one, rotation normalized.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewPose {
    /// World-space camera transform, looking down local negative Z.
    pub transform: Transform,
    /// Positive distance to the plane where all views coincide.
    pub focus_distance: f32,
    /// Vertical field of view in radians, between zero and pi.
    pub fov_y: f32,
    /// Positive near clipping distance.
    pub near: f32,
    /// Far clipping distance, greater than near (infinity is supported).
    pub far: f32,
}

/// Current reference pose. `None` disables view cameras and hides retained pixels.
#[derive(Resource, Default)]
pub struct LookingGlassView {
    /// Reference pose supplied by the host app.
    pub pose: Option<ViewPose>,
}

/// Live optical controls. Updating this resource never recreates output assets.
#[derive(Resource, Default)]
pub struct LookingGlassOptics {
    depth_scale: Option<f32>,
}

impl LookingGlassOptics {
    /// Override the initial baseline multiplier (zero removes parallax).
    pub fn set_depth_scale(&mut self, value: f32) -> Result<(), LookingGlassError> {
        if !value.is_finite() || value < 0.0 {
            return Err(LookingGlassError::Invalid(
                "depth_scale must be finite and nonnegative",
            ));
        }
        self.depth_scale = Some(value);
        Ok(())
    }
    /// Use the initial multiplier from the active output configuration again.
    pub fn reset_depth_scale(&mut self) {
        self.depth_scale = None;
    }
    /// Current live override; `None` uses the initial output configuration.
    pub fn depth_scale(&self) -> Option<f32> {
        self.depth_scale
    }
}

/// A scene camera owned by a Looking Glass output. Attach Bloom, DepthOfField,
/// or other per-view Bevy components using this marker after the Control set.
#[derive(Component, Clone, Copy, Debug, ExtractComponent)]
#[extract_app(RenderApp)]
pub struct LookingGlassViewCamera {
    /// Owning output/window entity.
    pub output: Entity,
    /// Optical view index, including gaps when generating intermediate views.
    pub index: u32,
}

/// Keep an attached Bevy DepthOfField's focal distance on the hologram focus plane.
#[derive(Component, Default)]
pub struct LookingGlassSyncFocus;

/// Pending output request. The last start/stop request in a frame wins.
#[derive(Resource, Default)]
pub struct LookingGlassControl {
    pending: Option<Option<LookingGlassConfig>>,
    quilt_preview: bool,
}

impl LookingGlassControl {
    /// Start or replace output. Failure is reported through [`LookingGlassStatus`].
    pub fn start(&mut self, config: LookingGlassConfig) {
        self.pending = Some(Some(config));
    }
    /// Close the owned window and release all owned cameras and asset handles.
    pub fn stop(&mut self) {
        self.pending = Some(None);
    }
    /// Show a virtual quilt using the same array. Ignored for HLD output.
    pub fn set_quilt_preview(&mut self, enabled: bool) {
        self.quilt_preview = enabled;
    }
}

/// Current output state, available to UI and host window controls.
#[derive(Resource, Default)]
pub struct LookingGlassStatus {
    /// Owned output window, or `None` while stopped. May be queried/moved by the host.
    pub window: Option<Entity>,
    /// Whether the active output uses calibrated multiview rendering.
    pub light_field: bool,
    /// Whether the active light-field output shows its virtual quilt.
    pub quilt_preview: bool,
    /// Last start/stop error, cleared by a successful request.
    pub error: Option<String>,
    /// Current invalid reference-pose error. Clears when the pose is valid or
    /// absent; invalid poses disable scene views without recreating output.
    pub pose_error: Option<String>,
}

#[cfg(test)]
mod tests;
