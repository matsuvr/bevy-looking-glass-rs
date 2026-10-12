//! Bevy resource/entity effects for the optional output.

use crate::{
    ActiveOutput, LookingGlassControl, LookingGlassFrame, LookingGlassOptics, LookingGlassOutput,
    LookingGlassSource, LookingGlassStatus, LookingGlassSyncFocus, LookingGlassSystems,
    LookingGlassView, LookingGlassViewCamera, MonitorInfo, QuiltMetadata, ViewGeneration, ViewPose,
    generation_gpu::{ReprojectionUniform, Synthesis},
    optics::{InterlaceUniforms, view_pose},
    plan_output_window, plan_view_generation,
    view_array::{self, AnchorTarget, QuiltInput, VIEW_FORMAT, ViewCamera},
};
use bevy::asset::embedded_asset;
use bevy::camera::{
    CameraOutputMode, CameraProjection, CameraUpdateSystems, ClearColorConfig, CompositingSpace,
    Hdr, RenderTarget, ScalingMode,
    visibility::{RenderLayers, VisibilitySystems},
};
use bevy::ecs::system::SystemParam;
use bevy::post_process::dof::DepthOfField;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{
    TextureDimension, TextureFormat, TextureUsages, TextureViewDimension,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::view::{DebandDither, NoIndirectDrawing, Tonemapping};
use bevy::render::{render_resource::AsBindGroup, storage::ShaderBuffer};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dPlugin};
use bevy::window::{Monitor, WindowRef};

#[derive(Component)]
struct OutputMesh;
#[derive(Component)]
struct OutputWindow(Handle<InterlaceMaterial>);

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct InterlaceMaterial {
    #[uniform(0)]
    uniforms: InterlaceUniforms,
    #[texture(1, dimension = "2d_array")]
    #[sampler(2)]
    views: Handle<Image>,
    #[storage(3, read_only)]
    cells: Handle<ShaderBuffer>,
}

impl Material2d for InterlaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_looking_glass/interlace.wesl".into()
    }
}

pub(super) fn install(app: &mut App) {
    embedded_asset!(app, "interlace.wesl");
    view_array::install(app);
    crate::generation_gpu::install(app);
    app.add_plugins(Material2dPlugin::<InterlaceMaterial>::default())
        .add_systems(Update, apply_request.in_set(LookingGlassSystems::Control))
        .add_systems(
            PostUpdate,
            (sync_views, sync_focus)
                .chain()
                .in_set(LookingGlassSystems::SyncViews)
                .run_if(resource_exists::<ActiveOutput>)
                .after(TransformSystems::Propagate)
                .before(CameraUpdateSystems)
                .before(VisibilitySystems::VisibilityPropagate),
        );
}

fn apply_request(world: &mut World) {
    let Some(mut control) = world.get_resource_mut::<LookingGlassControl>() else {
        return;
    };
    let pending = control.pending.take();
    let preview = control.quilt_preview;
    let windows: Vec<Entity> = world
        .query_filtered::<Entity, With<OutputWindow>>()
        .iter(world)
        .collect();
    if windows.is_empty() {
        world.remove_resource::<ActiveOutput>();
    }
    if let Some(request) = pending {
        let result = request.as_ref().map_or(Ok(()), |config| {
            config
                .output
                .validate()
                .map_err(|error| error.to_string())?;
            if config
                .scene_layers
                .intersects(&RenderLayers::layer(config.display_layer))
            {
                return Err("display_layer must be separate from scene_layers".to_string());
            }
            let metadata = metadata(config).map_err(|error| error.to_string())?;
            let layout = metadata.layout().map_err(|error| error.to_string())?;
            if !matches!(config.source, LookingGlassSource::Scene)
                && config.view_generation != ViewGeneration::Full
            {
                return Err("view generation applies only to Scene sources".into());
            }
            validate_source(config, world).map_err(|error| error.to_string())?;
            if let Some(display) = &config.display {
                if crate::parse_calibration(display.calibration.as_bytes())
                    .map_err(|error| error.to_string())?
                    .screen_size()
                    != config.output.screen_size()
                {
                    return Err("placement display does not match the output resolution".into());
                }
                let monitors = monitor_info(world);
                plan_output_window(display, &monitors).map_err(|error| error.to_string())?;
            }
            let device = world
                .get_resource::<RenderDevice>()
                .ok_or("render device is unavailable")?;
            let limits = device.limits();
            if metadata.view_count > limits.max_texture_array_layers
                || layout.view_width > limits.max_texture_dimension_2d
                || layout.view_height > limits.max_texture_dimension_2d
            {
                return Err("Looking Glass views exceed this GPU's texture limits".into());
            }
            plan_view_generation(metadata.view_count, config.view_generation)
                .map_err(|error| error.to_string())?;
            Ok(())
        });
        match result {
            Ok(()) => {
                for window in windows {
                    world.despawn(window);
                }
                world.remove_resource::<ActiveOutput>();
                world.resource_mut::<LookingGlassStatus>().error = None;
                if let Some(config) = request {
                    world.insert_resource(ActiveOutput(config));
                    if let Err(error) = world.run_system_cached(setup) {
                        world.remove_resource::<ActiveOutput>();
                        world.resource_mut::<LookingGlassStatus>().error = Some(error.to_string());
                    }
                }
            }
            Err(error) => world.resource_mut::<LookingGlassStatus>().error = Some(error),
        }
    }
    let window = world
        .query_filtered::<Entity, With<OutputWindow>>()
        .iter(world)
        .next();
    let light_field = world
        .get_resource::<ActiveOutput>()
        .is_some_and(|config| matches!(config.0.output, LookingGlassOutput::LightField { .. }));
    let material = window
        .and_then(|entity| world.get::<OutputWindow>(entity))
        .map(|o| o.0.clone());
    if let Some(handle) = material
        && let Some(mut materials) = world.get_resource_mut::<Assets<InterlaceMaterial>>()
        && materials.get(&handle).is_some_and(|material| {
            (material.uniforms.flags.w & 8 != 0) != (preview && light_field)
        })
        && let Some(mut material) = materials.get_mut(&handle)
    {
        let flags = &mut material.uniforms.flags.w;
        *flags = (*flags & !8) | if preview && light_field { 8 } else { 0 };
    }
    let mut status = world.resource_mut::<LookingGlassStatus>();
    status.window = window;
    status.light_field = light_field;
    status.quilt_preview = preview && light_field;
    if window.is_none() {
        status.pose_error = None;
    }
}

fn flat_projection(width: f32, height: f32) -> Projection {
    Projection::Orthographic(OrthographicProjection {
        scaling_mode: ScalingMode::Fixed { width, height },
        ..OrthographicProjection::default_2d()
    })
}

fn view_camera(index: u32, count: u32) -> Camera {
    Camera {
        // Distinct orders on one shared target prevent Bevy's ambiguous-order
        // warning, and ensure each layer is captured before scratch is reused.
        // Every view runs before the output camera (order 0).
        order: index as isize - count as isize - 1,
        is_active: false,
        clear_color: ClearColorConfig::Custom(Color::BLACK),
        // Bevy 0.20 omits ViewTarget for a clearing target with only Skip
        // cameras. Keep the first output to establish the shared attachment;
        // the remaining views need only the capture copy, not an output blit.
        output_mode: if index == 0 {
            CameraOutputMode::default()
        } else {
            CameraOutputMode::Skip
        },
        ..default()
    }
}

fn setup(
    mut commands: Commands,
    config: Res<ActiveOutput>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<InterlaceMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    monitors: Query<(Entity, &Monitor)>,
) {
    let config = &config.0;
    let Ok(metadata) = metadata(config) else {
        return;
    };
    let Ok(layout) = metadata.layout() else {
        return;
    };
    let Ok(plan) = plan_view_generation(metadata.view_count, config.view_generation) else {
        return;
    };
    let screen = config.output.screen_size();
    let (mut uniforms, cells) = match &config.output {
        LookingGlassOutput::LightField { calibration, .. } => {
            (calibration.uniforms(layout), calibration.cells.clone())
        }
        LookingGlassOutput::Hld { safe_area, .. } => (
            InterlaceUniforms {
                optics: Vec4::ZERO,
                // HLD safe area: 108px sides, 100px top/bottom at 2160x3840.
                screen: Vec4::new(screen.x as f32, screen.y as f32, safe_area.x, safe_area.y),
                grid: UVec4::new(1, 1, layout.view_width, layout.view_height),
                flags: UVec4::new(0, 0, 0, 16),
                source: UVec4::new(1, layout.width(), layout.height(), 0),
                content: Vec4::ONE,
            },
            vec![Vec4::ZERO],
        ),
    };
    uniforms.source = UVec4::new(
        metadata.view_count,
        metadata.image_size.x,
        metadata.image_size.y,
        0,
    );
    let panel_aspect = match &config.output {
        LookingGlassOutput::Hld { safe_area, .. } => {
            (screen.x as f32 * safe_area.x) / (screen.y as f32 * safe_area.y)
        }
        _ => screen.x as f32 / screen.y as f32,
    };
    let ratio = metadata.view_aspect / panel_aspect;
    uniforms.content = Vec4::new(ratio.min(1.0), (1.0 / ratio).min(1.0), 0.0, 0.0);
    let views = match &config.source {
        LookingGlassSource::ViewArray { image, .. } => image.clone(),
        _ => images.add(view_array::image(layout, metadata.view_count)),
    };
    let scene = matches!(config.source, LookingGlassSource::Scene);
    let anchors = if scene && !plan.generated.is_empty() {
        Some((
            images.add(view_array::anchor_image(
                layout.view_size(),
                plan.rendered.len() as u32,
                VIEW_FORMAT,
            )),
            images.add(view_array::anchor_image(
                layout.view_size(),
                plan.rendered.len() as u32,
                TextureFormat::Depth32Float,
            )),
        ))
    } else {
        None
    };
    // One logical target makes Bevy share its small main/depth textures across
    // views. The first camera writes it to keep the target attachment available;
    // the other cameras skip that blit. Only the array is sampled for display.
    let scratch = scene.then(|| {
        images.add(Image::new_target_texture(
            layout.view_width,
            layout.view_height,
            VIEW_FORMAT,
            None,
        ))
    });
    let material = materials.add(InterlaceMaterial {
        uniforms,
        views: views.clone(),
        cells: buffers.add(ShaderBuffer::from(cells)),
    });
    let mut output_window = config.window.clone();
    if let Some(display) = &config.display {
        let infos: Vec<_> = monitors
            .iter()
            .map(|(entity, monitor)| MonitorInfo {
                entity,
                position: monitor.physical_position,
                size: monitor.physical_size(),
            })
            .collect();
        if let Ok(plan) = plan_output_window(display, &infos) {
            plan.apply(&mut output_window);
        }
    }
    let window = commands
        .spawn((
            output_window,
            LookingGlassFrame {
                views: views.clone(),
                layout,
                metadata,
            },
            OutputWindow(material.clone()),
            Transform::default(),
            Visibility::Visible,
        ))
        .id();

    if let LookingGlassSource::Quilt { image, metadata } = &config.source {
        commands.entity(window).insert(QuiltInput {
            image: image.clone(),
            views: views.clone(),
            metadata: *metadata,
        });
    }
    if let Some(scratch) = scratch {
        for (anchor_index, &index) in plan.rendered.iter().enumerate() {
            let mut camera = commands.spawn((
                Camera3d {
                    depth_texture_usages: (TextureUsages::RENDER_ATTACHMENT
                        | TextureUsages::COPY_SRC)
                        .into(),
                    ..default()
                },
                Camera {
                    clear_color: if matches!(config.output, LookingGlassOutput::Hld { .. }) {
                        ClearColorConfig::Custom(Color::WHITE)
                    } else {
                        config.view_render.clear_color
                    },
                    ..view_camera(index, layout.count())
                },
                RenderTarget::Image(scratch.clone().into()),
                Msaa::Off,
                config.view_render.exposure,
                config.view_render.tonemapping,
                config.view_render.deband_dither,
                config.view_render.cluster_config,
                config.scene_layers.clone(),
                ViewCamera {
                    index,
                    array: views.clone(),
                    anchor: anchors.as_ref().map(|(color, depth)| AnchorTarget {
                        color: color.clone(),
                        depth: depth.clone(),
                        layer: anchor_index as u32,
                    }),
                    last: anchor_index + 1 == plan.rendered.len(),
                },
                LookingGlassViewCamera {
                    output: window,
                    index,
                },
                ChildOf(window),
            ));
            if anchor_index + 1 == plan.rendered.len()
                && let Some((color, depth)) = &anchors
            {
                camera.insert(Synthesis {
                    color: color.clone(),
                    depth: depth.clone(),
                    views: views.clone(),
                    jobs: Vec::new(),
                });
            }
            if config.view_render.hdr {
                camera.insert((Hdr, CompositingSpace::Linear));
            }
            if config.view_render.no_indirect_drawing {
                camera.insert(NoIndirectDrawing);
            }
        }
    }
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        RenderTarget::Window(WindowRef::Entity(window)),
        flat_projection(2.0, 2.0),
        Tonemapping::None,
        DebandDither::Disabled,
        Msaa::Off,
        RenderLayers::layer(config.display_layer),
        ChildOf(window),
    ));
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(material),
        Visibility::Hidden,
        OutputMesh,
        RenderLayers::layer(config.display_layer),
        ChildOf(window),
    ));
}

#[derive(SystemParam)]
struct ViewState<'w> {
    config: Res<'w, ActiveOutput>,
    view: Res<'w, LookingGlassView>,
    optics: Res<'w, LookingGlassOptics>,
    status: ResMut<'w, LookingGlassStatus>,
}

fn sync_views(
    state: ViewState,
    mut syntheses: Query<&mut Synthesis>,
    mut cameras: Query<(
        &ViewCamera,
        &mut Camera,
        &mut Projection,
        &mut Transform,
        &mut GlobalTransform,
    )>,
    mut output: Query<&mut Visibility, With<OutputMesh>>,
    mut last_pose: Local<Option<ViewPose>>,
) {
    let ViewState {
        config,
        view,
        optics,
        mut status,
    } = state;
    // Disabled cameras retain array pixels. Hide the mesh when the host has no pose.
    let external = !matches!(config.0.source, LookingGlassSource::Scene);
    status.pose_error = None;
    let pose = view.pose.filter(|pose| match pose.validate() {
        Ok(()) => true,
        Err(error) => {
            status.pose_error = Some(error.to_string());
            false
        }
    });
    if *last_pose == pose && !config.is_changed() && !optics.is_changed() {
        return;
    }
    *last_pose = pose;
    for mut visibility in &mut output {
        *visibility = output_visibility(external || pose.is_some());
    }
    if !syntheses.is_empty()
        && let Some(pose) = pose
        && let Ok(metadata) = metadata(&config.0)
        && let Ok(plan) = plan_view_generation(metadata.view_count, config.0.view_generation)
    {
        let perspective = PerspectiveProjection {
            fov: pose.fov_y,
            near: pose.near,
            near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -pose.near),
            far: pose.far,
            aspect_ratio: metadata.view_aspect,
        };
        let (cone, scale) = match &config.0.output {
            LookingGlassOutput::LightField {
                calibration,
                depth_scale,
                ..
            } => (
                calibration.view_cone,
                optics.depth_scale().unwrap_or(*depth_scale),
            ),
            _ => (0.0, 0.0),
        };
        let Ok(view_size) = metadata.view_size() else {
            return;
        };
        let jobs: Vec<_> = plan
            .generated
            .iter()
            .filter_map(|generated| {
                let left = *plan.rendered.get(generated.left_anchor as usize)?;
                let right = *plan.rendered.get(generated.right_anchor as usize)?;
                let (target, lens) = view_pose(
                    pose.transform,
                    pose.focus_distance,
                    perspective.clone(),
                    generated.index,
                    metadata.view_count,
                    cone,
                    scale,
                );
                let uniforms = [
                    (left, generated.left_anchor),
                    (right, generated.right_anchor),
                ]
                .map(|(index, layer)| {
                    let (source, source_lens) = view_pose(
                        pose.transform,
                        pose.focus_distance,
                        perspective.clone(),
                        index,
                        metadata.view_count,
                        cone,
                        scale,
                    );
                    ReprojectionUniform {
                        view_from_clip: source_lens.get_clip_from_view().inverse(),
                        target_from_view: lens.get_clip_from_view()
                            * Mat4::from_translation(
                                pose.transform.rotation.inverse()
                                    * (source.translation - target.translation),
                            ),
                        size_layer: UVec4::new(view_size.x, view_size.y, layer, 0),
                    }
                });
                Some((generated.index, uniforms))
            })
            .collect();
        for mut synthesis in &mut syntheses {
            synthesis.jobs.clone_from(&jobs);
        }
    }
    for (view, mut camera, mut projection, mut transform, mut global) in &mut cameras {
        camera.is_active = pose.is_some();
        let Some(pose) = pose else {
            continue;
        };
        let center = pose.transform;
        let focus_distance = pose.focus_distance;
        let perspective = PerspectiveProjection {
            fov: pose.fov_y,
            near: pose.near,
            near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -pose.near),
            far: pose.far,
            aspect_ratio: match &config.0.output {
                LookingGlassOutput::LightField { calibration, .. } => {
                    calibration.width as f32 / calibration.height as f32
                }
                LookingGlassOutput::Hld { screen, safe_area } => {
                    (screen.x as f32 * safe_area.x) / (screen.y as f32 * safe_area.y)
                }
            },
        };
        let (count, cone, depth_scale) = match &config.0.output {
            LookingGlassOutput::LightField {
                layout,
                calibration,
                depth_scale,
            } => (
                layout.count(),
                calibration.view_cone,
                optics.depth_scale().unwrap_or(*depth_scale),
            ),
            LookingGlassOutput::Hld { .. } => (1, 0.0, 0.0),
        };
        let (next, lens) = view_pose(
            center,
            focus_distance,
            perspective,
            view.index,
            count,
            cone,
            depth_scale,
        );
        *transform = next;
        // This system runs after propagation. All output entities are direct
        // children of an identity-transform window, so publish globals too.
        *global = GlobalTransform::from(next);
        *projection = Projection::custom(lens);
    }
}

fn metadata(config: &crate::LookingGlassConfig) -> Result<QuiltMetadata, crate::LookingGlassError> {
    let metadata = match &config.source {
        LookingGlassSource::Scene => {
            let aspect = match &config.output {
                LookingGlassOutput::LightField { calibration, .. } => {
                    calibration.width as f32 / calibration.height as f32
                }
                LookingGlassOutput::Hld { screen, safe_area } => {
                    (screen.x as f32 * safe_area.x) / (screen.y as f32 * safe_area.y)
                }
            };
            QuiltMetadata::from_layout(config.output.layout(), aspect)
        }
        LookingGlassSource::Quilt { metadata, .. }
        | LookingGlassSource::ViewArray { metadata, .. } => *metadata,
    };
    metadata.validate()?;
    Ok(metadata)
}

fn validate_source(
    config: &crate::LookingGlassConfig,
    world: &World,
) -> Result<(), crate::LookingGlassError> {
    let (handle, metadata, array) = match &config.source {
        LookingGlassSource::Scene => return Ok(()),
        LookingGlassSource::Quilt { image, metadata } => (image, *metadata, false),
        LookingGlassSource::ViewArray { image, metadata } => (image, *metadata, true),
    };
    let image = world
        .get_resource::<Assets<Image>>()
        .and_then(|images| images.get(handle))
        .ok_or(crate::LookingGlassError::Invalid(
            "external source image must be loaded before start",
        ))?;
    let size = metadata.view_size()?;
    let descriptor = &image.texture_descriptor;
    let valid_size = if array {
        descriptor.size.width == size.x
            && descriptor.size.height == size.y
            && descriptor.size.depth_or_array_layers >= metadata.view_count
    } else {
        descriptor.size.width == metadata.image_size.x
            && descriptor.size.height == metadata.image_size.y
            && descriptor.size.depth_or_array_layers == 1
    };
    let valid_view = !array
        || image.texture_view_descriptor.as_ref().is_some_and(|view| {
            view.dimension == Some(TextureViewDimension::D2Array)
                && view.base_array_layer == 0
                && view.base_mip_level == 0
                && view
                    .array_layer_count
                    .is_none_or(|layers| layers >= metadata.view_count)
                && view.format.is_none_or(|format| format == VIEW_FORMAT)
        });
    let usage = TextureUsages::COPY_SRC
        | if array {
            TextureUsages::TEXTURE_BINDING
        } else {
            TextureUsages::empty()
        };
    if !valid_size
        || !valid_view
        || descriptor.format != VIEW_FORMAT
        || descriptor.dimension != TextureDimension::D2
        || descriptor.sample_count != 1
        || descriptor.mip_level_count != 1
        || !descriptor.usage.contains(usage)
        || !image
            .asset_usage
            .contains(bevy::asset::RenderAssetUsages::RENDER_WORLD)
    {
        return Err(crate::LookingGlassError::Invalid(
            "external source image has incompatible size, format, dimension or usages",
        ));
    }
    Ok(())
}

fn monitor_info(world: &mut World) -> Vec<MonitorInfo> {
    world
        .query::<(Entity, &Monitor)>()
        .iter(world)
        .map(|(entity, monitor)| MonitorInfo {
            entity,
            position: monitor.physical_position,
            size: monitor.physical_size(),
        })
        .collect()
}

fn sync_focus(
    view: Res<LookingGlassView>,
    mut cameras: Query<
        &mut DepthOfField,
        (With<LookingGlassViewCamera>, With<LookingGlassSyncFocus>),
    >,
) {
    if let Some(pose) = view.pose
        && pose.validate().is_ok()
    {
        for mut dof in &mut cameras {
            dof.focal_distance = pose.focus_distance;
        }
    }
}

fn output_visibility(has_pose: bool) -> Visibility {
    if has_pose {
        Visibility::Visible
    } else {
        Visibility::Hidden
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::{Calibration, LookingGlassConfig, ViewLayout as Layout};
    use bevy::window::PrimaryWindow;

    #[test]
    fn stopping_from_settings_releases_the_window_and_its_views() {
        let mut app = App::new();
        app.init_resource::<LookingGlassControl>()
            .init_resource::<LookingGlassStatus>();
        let window = app.world_mut().spawn(OutputWindow(Handle::default())).id();
        let camera = app
            .world_mut()
            .spawn((Camera3d::default(), ChildOf(window)))
            .id();
        app.world_mut().resource_mut::<LookingGlassControl>().stop();
        apply_request(app.world_mut());
        assert!(app.world().get_entity(window).is_err());
        assert!(app.world().get_entity(camera).is_err());
        assert!(
            !app.world()
                .resource::<LookingGlassStatus>()
                .window
                .is_some()
        );
        assert!(!app.world().contains_resource::<ActiveOutput>());
    }

    #[test]
    fn closing_output_despawns_all_its_cameras_but_not_the_main_window() {
        let mut app = App::new();
        let main = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let output = app.world_mut().spawn(OutputWindow(Handle::default())).id();
        let child = app
            .world_mut()
            .spawn((
                ViewCamera {
                    index: 0,
                    array: Handle::default(),
                    anchor: None,
                    last: true,
                },
                ChildOf(output),
            ))
            .id();
        app.world_mut().entity_mut(output).despawn();
        assert!(app.world().get_entity(child).is_err());
        assert!(app.world().get_entity(main).is_ok());
    }

    #[test]
    fn setup_has_one_array_one_shared_scratch_and_no_quilt_camera_or_sprites() {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<InterlaceMaterial>>()
            .init_resource::<Assets<ShaderBuffer>>()
            .insert_resource(ActiveOutput(LookingGlassConfig::new(
                LookingGlassOutput::LightField {
                    layout: Layout {
                        columns: 11,
                        rows: 6,
                        view_width: 186,
                        view_height: 341,
                    },
                    // Entity/resource topology only; not a hardware calibration.
                    calibration: Calibration {
                        width: 1440,
                        height: 2560,
                        view_cone: 40_f32.to_radians(),
                        optics: Vec4::ZERO,
                        flags: UVec4::ZERO,
                        cells: vec![Vec4::ZERO],
                    },
                    depth_scale: 1.0,
                },
                RenderLayers::layer(0),
                3,
            )))
            .add_systems(Startup, setup);
        app.update();
        let world = app.world_mut();
        let mut views = world.query::<(&ViewCamera, &RenderTarget)>();
        let mut scratch_id = None;
        let mut array_id = None;
        let mut indices = Vec::new();
        for (view, target) in views.iter(world) {
            let target_id = target.as_image().map(Handle::id);
            assert!(target_id.is_some());
            if let Some(expected) = scratch_id {
                assert_eq!(target_id, Some(expected));
            } else {
                scratch_id = target_id;
            }
            if let Some(expected) = array_id {
                assert_eq!(view.array.id(), expected);
            } else {
                array_id = Some(view.array.id());
            }
            indices.push(view.index);
        }
        indices.sort_unstable();
        assert_eq!(indices, (0..66).collect::<Vec<_>>());
        assert_ne!(scratch_id, array_id);
        assert_eq!(world.resource::<Assets<Image>>().len(), 2);
        assert_eq!(world.query::<&Camera2d>().iter(world).count(), 1);
        assert_eq!(world.query::<&Sprite>().iter(world).count(), 0);
        assert!(
            world
                .query_filtered::<&Visibility, With<OutputMesh>>()
                .iter(world)
                .all(|visibility| *visibility == Visibility::Hidden)
        );
    }

    #[test]
    fn every_view_clears_scratch_in_order_and_first_keeps_the_target_attachment() {
        let mut previous_order = None;
        for index in 0..66 {
            let camera = view_camera(index, 66);
            assert!(camera.order < 0);
            assert!(previous_order.is_none_or(|order| order < camera.order));
            if index == 0 {
                assert!(matches!(camera.output_mode, CameraOutputMode::Write { .. }));
            } else {
                assert!(matches!(camera.output_mode, CameraOutputMode::Skip));
            }
            assert!(matches!(
                camera.clear_color,
                ClearColorConfig::Custom(color) if color == Color::BLACK
            ));
            assert!(!camera.is_active);
            previous_order = Some(camera.order);
        }
    }

    #[test]
    fn missing_pose_hides_retained_array_pixels() {
        assert_eq!(output_visibility(true), Visibility::Visible);
        assert_eq!(output_visibility(false), Visibility::Hidden);
    }

    fn hld_config() -> ActiveOutput {
        ActiveOutput(LookingGlassConfig::new(
            LookingGlassOutput::Hld {
                screen: UVec2::new(32, 64),
                safe_area: Vec2::splat(0.9),
            },
            RenderLayers::layer(2),
            5,
        ))
    }

    fn scene_app(generation: ViewGeneration) -> App {
        let mut config = hld_config().0;
        config.output = LookingGlassOutput::LightField {
            layout: Layout::new(4, 2, 8, 8).unwrap(),
            calibration: Calibration {
                width: 32,
                height: 64,
                view_cone: 0.7,
                optics: Vec4::ZERO,
                flags: UVec4::ZERO,
                cells: vec![Vec4::ZERO],
            },
            depth_scale: 1.0,
        };
        config.view_generation = generation;
        config.view_render.hdr = true;
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<InterlaceMaterial>>()
            .init_resource::<Assets<ShaderBuffer>>()
            .init_resource::<LookingGlassStatus>()
            .init_resource::<LookingGlassOptics>()
            .insert_resource(LookingGlassView {
                pose: Some(ViewPose {
                    transform: Transform::from_xyz(0.0, 0.0, 5.0),
                    focus_distance: 5.0,
                    fov_y: 0.7,
                    near: 0.2,
                    far: 50.0,
                }),
            })
            .insert_resource(ActiveOutput(config))
            .add_systems(Startup, setup)
            .add_systems(Update, (sync_views, sync_focus).chain());
        app
    }

    #[test]
    fn live_depth_and_pose_updates_preserve_output_entities_and_images() {
        let mut app = scene_app(ViewGeneration::Full);
        app.update();
        let world = app.world_mut();
        let (output, frame) = world
            .query::<(Entity, &LookingGlassFrame)>()
            .single(world)
            .unwrap();
        let array = frame.views.id();
        let before: Vec<_> = world
            .query::<(Entity, &LookingGlassViewCamera, &Transform)>()
            .iter(world)
            .map(|(entity, camera, pose)| (entity, camera.output, camera.index, pose.translation))
            .collect();
        assert_eq!(before.len(), 8);
        assert!(before.iter().all(|(_, owner, _, _)| *owner == output));
        world
            .resource_mut::<LookingGlassOptics>()
            .set_depth_scale(0.0)
            .unwrap();
        world
            .resource_mut::<LookingGlassView>()
            .pose
            .as_mut()
            .unwrap()
            .focus_distance = 7.0;
        app.update();
        let world = app.world_mut();
        assert_eq!(
            world.get::<LookingGlassFrame>(output).unwrap().views.id(),
            array
        );
        assert_eq!(world.resource::<Assets<Image>>().len(), 2);
        for (entity, _, _, _) in before {
            assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 0.0);
            assert!(world.get::<Hdr>(entity).is_some());
        }
        assert!(
            world
                .resource_mut::<LookingGlassOptics>()
                .set_depth_scale(-1.0)
                .is_err()
        );
        assert_eq!(
            world.resource::<LookingGlassOptics>().depth_scale(),
            Some(0.0)
        );
    }

    #[test]
    fn invalid_live_pose_reports_error_and_recovers_without_recreating_output() {
        let mut app = scene_app(ViewGeneration::Full);
        app.update();
        let world = app.world_mut();
        let output = world
            .query_filtered::<Entity, With<LookingGlassFrame>>()
            .single(world)
            .unwrap();
        let valid = world.resource::<LookingGlassView>().pose.unwrap();
        world.resource_mut::<LookingGlassView>().pose = Some(ViewPose {
            focus_distance: 0.0,
            ..valid
        });
        app.update();
        let world = app.world_mut();
        assert!(world.resource::<LookingGlassStatus>().pose_error.is_some());
        assert!(
            world
                .query_filtered::<&Camera, With<ViewCamera>>()
                .iter(world)
                .all(|camera| !camera.is_active)
        );
        world.resource_mut::<LookingGlassView>().pose = Some(valid);
        app.update();
        assert!(app.world().get_entity(output).is_ok());
        assert!(
            app.world()
                .resource::<LookingGlassStatus>()
                .pose_error
                .is_none()
        );
    }

    #[test]
    fn generated_output_creates_only_anchor_cameras_and_correct_reprojection_matrices() {
        let mut app = scene_app(ViewGeneration::DepthReprojection { rendered_views: 3 });
        app.update();
        let world = app.world_mut();
        let mut indices: Vec<_> = world
            .query::<&LookingGlassViewCamera>()
            .iter(world)
            .map(|camera| camera.index)
            .collect();
        indices.sort_unstable();
        assert_eq!(indices, [0, 3, 7]);
        let synthesis = world.query::<&Synthesis>().single(world).unwrap();
        assert_eq!(synthesis.jobs.len(), 5);
        assert_eq!(world.resource::<Assets<Image>>().len(), 4);
        let reference = world.resource::<LookingGlassView>().pose.unwrap();
        let perspective = PerspectiveProjection {
            fov: reference.fov_y,
            near: reference.near,
            far: reference.far,
            aspect_ratio: 0.5,
            near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -reference.near),
        };
        let plan = plan_view_generation(8, ViewGeneration::DepthReprojection { rendered_views: 3 })
            .unwrap();
        for (target_index, uniforms) in &synthesis.jobs {
            let (_, target_lens) = view_pose(
                reference.transform,
                5.0,
                perspective.clone(),
                *target_index,
                8,
                0.7,
                1.0,
            );
            let job = plan
                .generated
                .iter()
                .find(|job| job.index == *target_index)
                .unwrap();
            for (uniform, anchor) in uniforms.iter().zip([job.left_anchor, job.right_anchor]) {
                let source_index = *plan.rendered.get(anchor as usize).unwrap();
                let (source, source_lens) = view_pose(
                    reference.transform,
                    5.0,
                    perspective.clone(),
                    source_index,
                    8,
                    0.7,
                    1.0,
                );
                for z in [-2.0, -5.0, -9.0] {
                    let source_point = Vec4::new(-source.translation.x, 0.0, z, 1.0);
                    let clip = source_lens.get_clip_from_view() * source_point;
                    let reprojected = uniform.target_from_view * uniform.view_from_clip * clip;
                    let (target, _) = view_pose(
                        reference.transform,
                        5.0,
                        perspective.clone(),
                        *target_index,
                        8,
                        0.7,
                        1.0,
                    );
                    let expected = target_lens.get_clip_from_view()
                        * Vec4::new(-target.translation.x, 0.0, z, 1.0);
                    assert!((reprojected - expected).length() < 1e-4);
                }
            }
        }
    }

    #[test]
    fn external_quilt_needs_no_pose_or_scene_cameras_and_preserves_active_views() {
        let mut app = scene_app(ViewGeneration::Full);
        let metadata = QuiltMetadata {
            image_size: UVec2::new(11, 9),
            columns: 3,
            rows: 2,
            view_count: 5,
            view_aspect: 0.75,
        };
        let mut image = Image::new_target_texture(11, 9, VIEW_FORMAT, None);
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let handle = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        app.world_mut().resource_mut::<ActiveOutput>().0.source = LookingGlassSource::Quilt {
            image: handle,
            metadata,
        };
        app.world_mut().resource_mut::<LookingGlassView>().pose = None;
        app.update();
        let world = app.world_mut();
        assert_eq!(
            world.query::<&LookingGlassViewCamera>().iter(world).count(),
            0
        );
        assert_eq!(world.query::<&QuiltInput>().iter(world).count(), 1);
        assert_eq!(
            world
                .query::<&LookingGlassFrame>()
                .single(world)
                .unwrap()
                .metadata,
            metadata
        );
        assert_eq!(
            *world
                .query_filtered::<&Visibility, With<OutputMesh>>()
                .single(world)
                .unwrap(),
            Visibility::Visible
        );
        let material = world
            .resource::<Assets<InterlaceMaterial>>()
            .iter()
            .next()
            .unwrap()
            .1;
        assert_eq!(material.uniforms.source.x, 5);
    }

    #[test]
    fn generic_pose_disables_retained_pixels_and_output_survives_without_primary_window() {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<InterlaceMaterial>>()
            .init_resource::<Assets<ShaderBuffer>>()
            .init_resource::<LookingGlassControl>()
            .init_resource::<LookingGlassStatus>()
            .init_resource::<LookingGlassView>()
            .init_resource::<LookingGlassOptics>()
            .insert_resource(hld_config())
            .add_systems(Startup, setup)
            .add_systems(Update, (apply_request, sync_views).chain());
        let pose = ViewPose {
            transform: Transform::from_xyz(0.0, 1.0, 5.0),
            focus_distance: 5.0,
            fov_y: 0.6,
            near: 0.2,
            far: 50.0,
        };
        app.world_mut().resource_mut::<LookingGlassView>().pose = Some(pose);
        app.update();
        let window = app.world().resource::<LookingGlassStatus>().window.unwrap();
        let world = app.world_mut();
        let (camera, transform, layers) = world
            .query_filtered::<(&Camera, &Transform, &RenderLayers), With<ViewCamera>>()
            .single(world)
            .unwrap();
        assert!(camera.is_active);
        assert_eq!(*transform, pose.transform);
        assert_eq!(*layers, RenderLayers::layer(2));
        world.resource_mut::<LookingGlassView>().pose = None;
        app.update();
        let world = app.world_mut();
        assert!(world.get_entity(window).is_ok());
        assert!(
            !world
                .query_filtered::<&Camera, With<ViewCamera>>()
                .single(world)
                .unwrap()
                .is_active
        );
        assert_eq!(
            *world
                .query_filtered::<&Visibility, With<OutputMesh>>()
                .single(world)
                .unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn stopping_releases_owned_assets_and_preserves_host_assets() {
        use bevy::asset::AssetApp;
        let mut app = App::new();
        app.add_plugins((bevy::app::TaskPoolPlugin::default(), AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<Mesh>()
            .init_asset::<InterlaceMaterial>()
            .init_asset::<ShaderBuffer>()
            .init_resource::<LookingGlassControl>()
            .init_resource::<LookingGlassStatus>()
            .insert_resource(hld_config())
            .add_systems(Startup, setup)
            .add_systems(Update, apply_request);
        let host_image = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        let host_window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.update();
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 3);
        app.world_mut().resource_mut::<LookingGlassControl>().stop();
        // Asset handle drops propagate from material -> array/buffer over successive frames.
        for _ in 0..5 {
            app.update();
        }
        assert!(app.world().get_entity(host_window).is_ok());
        assert!(
            app.world()
                .resource::<Assets<Image>>()
                .contains(host_image.id())
        );
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
        assert!(app.world().resource::<Assets<Mesh>>().is_empty());
        assert!(
            app.world()
                .resource::<Assets<InterlaceMaterial>>()
                .is_empty()
        );
        assert!(app.world().resource::<Assets<ShaderBuffer>>().is_empty());
    }

    #[test]
    fn rejected_replacement_keeps_existing_output_and_reports_error() {
        let mut app = App::new();
        app.init_resource::<LookingGlassControl>()
            .init_resource::<LookingGlassStatus>()
            .insert_resource(hld_config());
        let window = app.world_mut().spawn(OutputWindow(Handle::default())).id();
        let mut config = hld_config().0;
        config.output = LookingGlassOutput::Hld {
            screen: UVec2::ZERO,
            safe_area: Vec2::ONE,
        };
        app.world_mut()
            .resource_mut::<LookingGlassControl>()
            .start(config);
        apply_request(app.world_mut());
        let status = app.world().resource::<LookingGlassStatus>();
        assert_eq!(status.window, Some(window));
        assert!(status.error.is_some());
        assert!(app.world().contains_resource::<ActiveOutput>());
        app.world_mut().resource_mut::<LookingGlassControl>().stop();
        apply_request(app.world_mut());
        assert!(app.world().resource::<LookingGlassStatus>().error.is_none());
    }
}
