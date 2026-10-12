//! Pure anchor selection. No automatic transparency detection or mode switching.
use crate::LookingGlassError;

/// Explicit tradeoff between scene draw count and depth reprojection artifacts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewGeneration {
    /// Draw every view, including transparent and view-dependent effects.
    #[default]
    Full,
    /// Draw evenly spaced anchors, including both endpoints, and forward-project
    /// opaque depth into the gaps. Disocclusions visible in neither anchor stay black.
    DepthReprojection {
        /// At least two anchors, no more than the total output view count.
        rendered_views: u32,
    },
}

/// Two anchor layers supplying a generated optical view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneratedView {
    /// Destination optical view.
    pub index: u32,
    /// Left reference's layer in the compact anchor array.
    pub left_anchor: u32,
    /// Right reference's layer in the compact anchor array.
    pub right_anchor: u32,
}

/// Exact scene cameras and synthesized views for one output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewGenerationPlan {
    /// Optical indices drawn by Bevy, in increasing order.
    pub rendered: Vec<u32>,
    /// Remaining views, bounded by their nearest rendered anchors.
    pub generated: Vec<GeneratedView>,
}

/// Plan anchors without rendering. Invalid counts are errors, never silently clamped.
pub fn plan_view_generation(
    output_views: u32,
    mode: ViewGeneration,
) -> Result<ViewGenerationPlan, LookingGlassError> {
    let rendered_views = match mode {
        ViewGeneration::Full if output_views > 0 => output_views,
        ViewGeneration::DepthReprojection { rendered_views }
            if rendered_views >= 2 && rendered_views <= output_views =>
        {
            rendered_views
        }
        _ => {
            return Err(LookingGlassError::Invalid(
                "invalid output/anchor view count",
            ));
        }
    };
    let rendered: Vec<u32> = (0..rendered_views)
        .map(|index| {
            if rendered_views == 1 {
                0
            } else {
                (u64::from(index) * u64::from(output_views - 1) / u64::from(rendered_views - 1))
                    as u32
            }
        })
        .collect();
    let mut generated = Vec::new();
    for (left, pair) in rendered.windows(2).enumerate() {
        if let [start, end] = pair {
            generated.extend((start + 1..*end).map(|index| GeneratedView {
                index,
                left_anchor: left as u32,
                right_anchor: left as u32 + 1,
            }));
        }
    }
    Ok(ViewGenerationPlan {
        rendered,
        generated,
    })
}
