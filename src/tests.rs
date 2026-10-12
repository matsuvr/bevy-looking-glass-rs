#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use super::*;
use crate::{ViewLayout as Layout, calibration::RawCalibration, optics::view_pose};
use bevy::camera::CameraProjection;

#[test]
fn generation_has_exact_endpoints_coverage_and_nearest_references() {
    for count in 2..70 {
        for anchors in 2..=count {
            let plan = plan_view_generation(
                count,
                ViewGeneration::DepthReprojection {
                    rendered_views: anchors,
                },
            )
            .unwrap();
            assert_eq!(plan.rendered.len(), anchors as usize);
            assert_eq!(plan.rendered.first(), Some(&0));
            assert_eq!(plan.rendered.last(), Some(&(count - 1)));
            let mut coverage = plan.rendered.clone();
            for view in &plan.generated {
                let left = plan.rendered[view.left_anchor as usize];
                let right = plan.rendered[view.right_anchor as usize];
                assert_eq!(view.right_anchor, view.left_anchor + 1);
                assert!(left < view.index && view.index < right);
                coverage.push(view.index);
            }
            coverage.sort_unstable();
            assert_eq!(coverage, (0..count).collect::<Vec<_>>());
        }
    }
    assert_eq!(
        plan_view_generation(1, ViewGeneration::Full)
            .unwrap()
            .rendered,
        [0]
    );
    assert!(plan_view_generation(0, ViewGeneration::Full).is_err());
    for anchors in [0, 1, 5] {
        assert!(
            plan_view_generation(
                4,
                ViewGeneration::DepthReprojection {
                    rendered_views: anchors
                }
            )
            .is_err()
        );
    }
}

#[test]
fn rotated_volume_frames_the_given_center_and_height() {
    let volume = HologramVolume {
        focal_plane: Transform::from_xyz(1.0, 2.0, -3.0).with_rotation(Quat::from_rotation_y(0.8)),
        height: 3.5,
        near: 0.2,
        far: 80.0,
    };
    let pose = view_pose_from_volume(&volume, 0.7).unwrap();
    let center = pose.transform.translation + *pose.transform.forward() * pose.focus_distance;
    assert!(center.distance(volume.focal_plane.translation) < 1e-5);
    assert!((2.0 * pose.focus_distance * (pose.fov_y * 0.5).tan() - volume.height).abs() < 1e-5);
    assert_eq!(pose.transform.rotation, volume.focal_plane.rotation);
    assert!(
        view_pose_from_volume(
            &HologramVolume {
                height: 0.0,
                ..volume
            },
            0.7
        )
        .is_err()
    );
    assert!(view_pose_from_volume(&volume, f32::NAN).is_err());
}

#[test]
fn partial_quilt_preserves_bottom_left_order_and_edge_pixels() {
    let metadata = QuiltMetadata {
        image_size: UVec2::new(11, 9),
        columns: 3,
        rows: 2,
        view_count: 5,
        view_aspect: 0.75,
    };
    assert_eq!(metadata.view_size().unwrap(), UVec2::new(3, 4));
    assert_eq!(metadata.tile_origin(0).unwrap(), UVec2::new(0, 5));
    assert_eq!(metadata.tile_origin(2).unwrap(), UVec2::new(6, 5));
    assert_eq!(metadata.tile_origin(3).unwrap(), UVec2::new(0, 1));
    assert!(metadata.tile_origin(5).is_err());
    assert_eq!(metadata.filename_suffix().unwrap(), "_qs3x2a0.75");
    for invalid in [
        QuiltMetadata {
            columns: 0,
            ..metadata
        },
        QuiltMetadata {
            view_count: 7,
            ..metadata
        },
        QuiltMetadata {
            view_aspect: f32::NAN,
            ..metadata
        },
        QuiltMetadata {
            image_size: UVec2::ONE,
            ..metadata
        },
    ] {
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn monitor_matching_uses_physical_coordinates_and_native_size_not_bridge_index() {
    let display = bridge::Display {
        index: 77,
        window_position: IVec2::new(-1440, 120),
        serial: "TEST".into(),
        name: "TEST".into(),
        hardware_version: "TEST".into(),
        calibration: raw().to_string(),
        quilt: None,
    };
    let wrong_size = MonitorInfo {
        entity: Entity::from_bits(20),
        position: display.window_position,
        size: UVec2::new(720, 1280),
    };
    let right = MonitorInfo {
        entity: Entity::from_bits(21),
        position: display.window_position,
        size: UVec2::new(1440, 2560),
    };
    assert!(plan_output_window(&display, &[wrong_size]).is_err());
    let plan = plan_output_window(&display, &[wrong_size, right]).unwrap();
    assert_eq!(plan.monitor, right.entity);
    let mut window = Window::default();
    plan.apply(&mut window);
    assert_eq!(window.physical_width(), 1440);
    assert_eq!(
        window.position,
        bevy::window::WindowPosition::At(display.window_position)
    );
    assert!(
        matches!(window.mode, bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Entity(entity)) if entity == right.entity)
    );
    assert!(plan_output_window(&display, &[right, right]).is_err());
}

// Synthetic lens values for equation tests, not a usable device calibration.
fn raw() -> serde_json::Value {
    serde_json::json!({
        "pitch": {"value": 45.0}, "slope": {"value": -5.0},
        "center": {"value": -0.5}, "DPI": {"value": 300.0},
        "screenW": {"value": 1440.0}, "screenH": {"value": 2560.0},
        "viewCone": {"value": 40.0}, "invView": {"value": 1},
        "flipImageX": {"value": 0}, "flipImageY": {"value": 0},
        "flipSubp": {"value": 0}
    })
}

fn calibration(value: serde_json::Value) -> Result<Calibration, LookingGlassError> {
    parse_calibration(&serde_json::to_vec(&value).unwrap())
}

#[test]
fn quilt_layout_dimensions_and_view_count() {
    let layout = Layout::new(3, 2, 10, 20).unwrap();
    assert_eq!(
        (layout.width(), layout.height(), layout.count()),
        (30, 40, 6)
    );
}

#[test]
fn quilt_rejects_undefined_divisors_and_overflow_not_a_resolution_budget() {
    assert!(Layout::new(1, 1, 10, 10).is_err());
    assert!(Layout::new(0, 2, 10, 10).is_err());
    assert!(Layout::new(2, 2, 0, 10).is_err());
    assert!(Layout::new(u32::MAX, 2, 1, 1).is_err());
    assert!(Layout::new(2, 1, u32::MAX, 1).is_err());
    assert!(Layout::new(11, 6, 186, 341).is_ok());
}

#[test]
fn normalized_lens_uses_dpi_and_slope_not_raw_pitch_as_a_shader_pitch() {
    let c = calibration(raw()).unwrap();
    let expected = 45.0_f32 * 1440.0 / 300.0 * (-0.2_f32).atan().cos();
    assert!((c.optics.x - expected).abs() < 1e-5);
    assert!((c.optics.y - 2560.0 / (1440.0 * -5.0)).abs() < 1e-6);
    assert!((c.optics.w - 1.0 / 4320.0).abs() < 1e-8);
    assert_eq!(c.flags, UVec4::new(0, 0, 1, 0));
}

#[test]
fn cell_offsets_are_normalized_once_and_orientation_flags_are_retained() {
    let mut value = raw();
    value["CellPatternMode"] = serde_json::json!({"value": 3});
    value["flipImageX"]["value"] = 1.into();
    value["flipImageY"]["value"] = 1.into();
    value["flipSubp"]["value"] = 1.into();
    value["subpixelCells"] = serde_json::json!([{
        "ROffsetX": 1.0, "ROffsetY": 2.0, "GOffsetX": 3.0,
        "GOffsetY": 4.0, "BOffsetX": 5.0, "BOffsetY": 6.0
    }]);
    let c = calibration(value).unwrap();
    assert_eq!(c.flags, UVec4::new(1, 3, 1, 7));
    assert_eq!(
        c.cells[0],
        Vec4::new(1.0 / 1440.0, 2.0 / 2560.0, 3.0 / 1440.0, 4.0 / 2560.0)
    );
    assert_eq!(c.cells[1], Vec4::new(5.0 / 1440.0, 6.0 / 2560.0, 0.0, 0.0));
    let before = c.cells.clone();
    let layout = Layout::new(11, 6, 186, 341).unwrap();
    let _ = c.uniforms(layout);
    let _ = c.uniforms(layout);
    assert_eq!(c.cells, before);
    assert!(c.optics.y > 0.0 && c.optics.w < 0.0);
}

#[test]
fn missing_calibration_is_not_replaced_with_a_default_lens() {
    assert!(serde_json::from_value::<RawCalibration>(serde_json::json!({})).is_err());
    let mut value = raw();
    value["slope"]["value"] = 0.into();
    assert!(calibration(value).is_err());
}

#[test]
fn focus_plane_remains_at_the_same_screen_position_for_every_view() {
    let center = Transform::from_xyz(0.0, 1.0, 4.0);
    for index in 0..66 {
        let (camera, lens) = view_pose(
            center,
            4.0,
            PerspectiveProjection::default(),
            index,
            66,
            40_f32.to_radians(),
            1.0,
        );
        let offset = camera.translation.x - center.translation.x;
        let projected = lens.get_clip_from_view() * Vec4::new(-offset, 0.0, -4.0, 1.0);
        assert!((projected.x / projected.w).abs() < 1e-5);
        assert_eq!(camera.rotation, center.rotation);
    }
}

#[test]
fn depth_has_opposite_disparity_on_either_side_of_the_focus_plane() {
    let center = Transform::from_xyz(0.0, 0.0, 4.0);
    let (camera, lens) = view_pose(
        center,
        4.0,
        PerspectiveProjection::default(),
        65,
        66,
        40_f32.to_radians(),
        1.0,
    );
    let clip = lens.get_clip_from_view();
    let front = clip * Vec4::new(-camera.translation.x, 0.0, -2.0, 1.0);
    let back = clip * Vec4::new(-camera.translation.x, 0.0, -6.0, 1.0);
    assert!(front.x / front.w < 0.0);
    assert!(back.x / back.w > 0.0);
}

#[test]
fn view_offsets_follow_camera_right_without_rotating_the_cameras() {
    let center = Transform::from_rotation(Quat::from_rotation_y(0.7));
    let (left, _) = view_pose(
        center,
        4.0,
        PerspectiveProjection::default(),
        0,
        45,
        0.7,
        1.0,
    );
    let (right, _) = view_pose(
        center,
        4.0,
        PerspectiveProjection::default(),
        44,
        45,
        0.7,
        1.0,
    );
    assert!((left.translation + right.translation).length() < 1e-6);
    assert!((right.translation.cross(*center.right())).length() < 1e-6);
    assert_eq!(left.rotation, right.rotation);
}

#[test]
fn off_axis_frustum_corners_and_tile_aspect_agree_with_the_projection() {
    let perspective = PerspectiveProjection {
        aspect_ratio: 1440.0 / 2560.0,
        ..default()
    };
    let (_, mut lens) = view_pose(Transform::default(), 4.0, perspective, 0, 66, 0.7, 1.0);
    lens.update(186.0, 341.0);
    assert_eq!(lens.perspective.aspect_ratio, 1440.0 / 2560.0);
    for corner in lens.get_frustum_corners(-0.1, -10.0) {
        let clip = lens.get_clip_from_view() * Vec3::from(corner).extend(1.0);
        assert!(((clip.x / clip.w).abs() - 1.0).abs() < 1e-5);
        assert!(((clip.y / clip.w).abs() - 1.0).abs() < 1e-5);
    }
}

#[test]
fn every_light_field_preset_uses_its_reference_grid_and_physical_aspect() {
    let devices = device_presets().unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for device in &devices {
        assert!(ids.insert(&device.id));
        assert!(!device.sources.is_empty());
        let [width, height] = device.screen;
        assert!((device.aspect - width as f32 / height as f32).abs() < 1e-6);
        if device.format != DisplayFormat::LightField {
            continue;
        }
        let mut value = raw();
        value["screenW"]["value"] = width.into();
        value["screenH"]["value"] = height.into();
        let config = device
            .light_field_output(calibration(value).unwrap())
            .unwrap();
        let LookingGlassOutput::LightField {
            layout,
            calibration,
            ..
        } = config
        else {
            panic!("light field expected");
        };
        let quilt = device.quilt.as_ref().unwrap();
        assert_eq!(layout.count(), quilt.columns * quilt.rows);
        assert_eq!(layout.view_width, quilt.width / quilt.columns);
        assert_eq!(layout.view_height, quilt.height / quilt.rows);
        assert_eq!((calibration.width, calibration.height), (width, height));
    }
    let portrait = devices
        .iter()
        .find(|d| d.id == "portrait")
        .unwrap()
        .quilt
        .as_ref()
        .unwrap();
    assert_eq!(
        (
            portrait.columns,
            portrait.rows,
            portrait.width,
            portrait.height
        ),
        (8, 6, 3360, 3360)
    );
    assert!(devices.iter().any(|d| d.id == "original-8-9"));
}

#[test]
fn selected_model_never_substitutes_a_different_devices_lens() {
    let devices = device_presets().unwrap();
    let portrait = devices.iter().find(|d| d.id == "portrait").unwrap();
    // Synthetic Go-sized calibration cannot drive a Portrait preset.
    assert!(
        portrait
            .light_field_output(calibration(raw()).unwrap())
            .is_err()
    );
}

#[test]
fn hld_has_one_view_and_standalone_has_no_live_output() {
    let center = Transform::from_xyz(0.0, 1.0, 4.0);
    let (view, projection) = view_pose(
        center,
        4.0,
        PerspectiveProjection::default(),
        0,
        1,
        0.0,
        0.0,
    );
    assert_eq!(view, center);
    assert!(projection.get_clip_from_view().is_finite());
    for device in device_presets().unwrap() {
        match device.format {
            DisplayFormat::Hld => {
                let output = device.hld_output().unwrap();
                assert_eq!(output.layout().count(), 1);
                assert_eq!(output.screen_size().to_array(), device.screen);
                let safe = device.safe_area.unwrap();
                assert!(safe.into_iter().all(|value| value > 0.0 && value < 1.0));
            }
            DisplayFormat::Standalone => {
                assert!(device.hld_output().is_err())
            }
            DisplayFormat::LightField => {}
        }
    }
}
