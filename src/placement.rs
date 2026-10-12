//! Pure monitor matching and Bevy window application.
use crate::{LookingGlassError, bridge::Display, parse_calibration};
use bevy::{
    prelude::*,
    window::{MonitorSelection, WindowMode, WindowPosition, WindowResolution},
};

/// OS monitor identity and physical desktop rectangle.
#[derive(Clone, Copy, Debug)]
pub struct MonitorInfo {
    /// Bevy monitor entity; Bridge indices are not OS monitor indices.
    pub entity: Entity,
    /// Desktop top-left in physical pixels (negative positions are supported).
    pub position: IVec2,
    /// Physical pixel resolution.
    pub size: UVec2,
}

/// Exact native output placement; no primary-monitor fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputWindowPlan {
    /// Matching OS monitor.
    pub monitor: Entity,
    /// Desktop top-left physical pixel.
    pub position: IVec2,
    /// Native panel resolution.
    pub size: UVec2,
}

/// Match Bridge coordinates and calibrated native dimensions to an OS monitor.
pub fn plan_output_window(
    display: &Display,
    monitors: &[MonitorInfo],
) -> Result<OutputWindowPlan, LookingGlassError> {
    let size = parse_calibration(display.calibration.as_bytes())?.screen_size();
    let mut matching = monitors
        .iter()
        .filter(|monitor| monitor.position == display.window_position && monitor.size == size);
    let monitor = matching.next().ok_or(LookingGlassError::Invalid(
        "no native monitor matches the Bridge display coordinates and resolution",
    ))?;
    if matching.next().is_some() {
        return Err(LookingGlassError::Invalid(
            "Bridge display matches multiple monitors",
        ));
    }
    Ok(OutputWindowPlan {
        monitor: monitor.entity,
        position: monitor.position,
        size,
    })
}

impl OutputWindowPlan {
    /// Apply native resolution, physical position and borderless fullscreen.
    pub fn apply(self, window: &mut Window) {
        window.position = WindowPosition::At(self.position);
        window.resolution =
            WindowResolution::new(self.size.x, self.size.y).with_scale_factor_override(1.0);
        window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Entity(self.monitor));
    }
}
