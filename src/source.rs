//! External multiview sources and their geometry.
use crate::{LookingGlassError, ViewLayout};
use bevy::prelude::*;
use bevy::render::{RenderApp, extract_component::ExtractComponent};

/// Source content. External images must be Rgba8UnormSrgb, single-sampled,
/// one mip level, and remain available in Bevy's render world.
#[derive(Clone, Debug, Default)]
pub enum LookingGlassSource {
    /// Render the host's 3D scene.
    #[default]
    Scene,
    /// Copy the active tiles of a conventional bottom-left ordered quilt.
    /// The image needs COPY_SRC usage. Unused top/right pixels are ignored.
    Quilt {
        /// Image updated by the host (still images or video frames).
        image: Handle<Image>,
        /// Tile geometry and original content aspect ratio.
        metadata: QuiltMetadata,
    },
    /// Display an existing 2D texture array without scene cameras or a copy pass.
    ViewArray {
        /// One top-left oriented image per layer in optical view order.
        /// Requires a D2Array view and TEXTURE_BINDING | COPY_SRC usages.
        image: Handle<Image>,
        /// Virtual quilt geometry for preview and export.
        metadata: QuiltMetadata,
    },
}

/// Quilt geometry. Valid views can occupy fewer slots than columns times rows.
/// The physical content aspect is independent of the stretched tile pixel aspect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuiltMetadata {
    /// Full image dimensions, including unused edge pixels.
    pub image_size: UVec2,
    /// Grid columns.
    pub columns: u32,
    /// Grid rows.
    pub rows: u32,
    /// Number of active views, starting at bottom-left.
    pub view_count: u32,
    /// Original content width divided by height.
    pub view_aspect: f32,
}

impl QuiltMetadata {
    /// Check divisors, active slots, aspect and dimensions before GPU use.
    pub fn validate(self) -> Result<(), LookingGlassError> {
        let slots = self.columns.checked_mul(self.rows);
        if self.columns == 0
            || self.rows == 0
            || self.view_count == 0
            || slots.is_none_or(|slots| self.view_count > slots)
            || self.image_size.x < self.columns
            || self.image_size.y < self.rows
            || !self.view_aspect.is_finite()
            || self.view_aspect <= 0.0
        {
            return Err(LookingGlassError::Invalid(
                "invalid quilt dimensions, view count or aspect",
            ));
        }
        Ok(())
    }
    /// Integer tile size, excluding unused edge pixels. Call validate first.
    pub fn view_size(self) -> Result<UVec2, LookingGlassError> {
        self.validate()?;
        Ok(self.image_size / UVec2::new(self.columns, self.rows))
    }
    /// Top-left image pixel of a view, anchoring the quilt at bottom-left.
    pub fn tile_origin(self, index: u32) -> Result<UVec2, LookingGlassError> {
        let size = self.view_size()?;
        if index >= self.view_count {
            return Err(LookingGlassError::Invalid(
                "quilt view index is out of range",
            ));
        }
        Ok(UVec2::new(
            (index % self.columns) * size.x,
            self.image_size.y - (index / self.columns + 1) * size.y,
        ))
    }
    /// Filename metadata understood by Looking Glass applications.
    pub fn filename_suffix(self) -> Result<String, LookingGlassError> {
        self.validate()?;
        Ok(format!(
            "_qs{}x{}a{}",
            self.columns, self.rows, self.view_aspect
        ))
    }
    pub(crate) fn layout(self) -> Result<ViewLayout, LookingGlassError> {
        let size = self.view_size()?;
        // A one-view quilt/array is useful for HLD as well.
        Ok(ViewLayout {
            columns: self.columns,
            rows: self.rows,
            view_width: size.x,
            view_height: size.y,
        })
    }
    pub(crate) fn from_layout(layout: ViewLayout, view_aspect: f32) -> Self {
        Self {
            image_size: UVec2::new(layout.width(), layout.height()),
            columns: layout.columns(),
            rows: layout.rows(),
            view_count: layout.count(),
            view_aspect,
        }
    }
}

/// Completed view storage, attached to the output/window entity.
/// Scene pixels update after per-view post-processing; generated views are
/// depth-composited afterwards. Holding this handle keeps the image alive.
#[derive(Component, Clone, Debug, ExtractComponent)]
#[extract_app(RenderApp)]
pub struct LookingGlassFrame {
    /// SDR array sampled by the interlacer and capture API.
    pub views: Handle<Image>,
    /// Virtual tile layout.
    pub layout: ViewLayout,
    /// Active views, original aspect and export geometry.
    pub metadata: QuiltMetadata,
}
