//! Fixed-time PNG sequence. Usage: capture_sequence visual.json directory
//! [--generated] [--hdr]. Both modes are explicit; no material auto-detection.
#![forbid(unsafe_code)]
use bevy::{
    camera::visibility::RenderLayers,
    post_process::{bloom::Bloom, dof::DepthOfField},
    prelude::*,
};
use bevy_looking_glass::*;
use std::{path::PathBuf, thread::JoinHandle, time::Duration};

#[derive(Resource)]
struct Sequence {
    directory: PathBuf,
    frame: u64,
    waiting: bool,
    warmup: u32,
    writers: Vec<JoinHandle<Result<(), String>>>,
    hdr: bool,
}
#[derive(Component)]
struct Animated;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .ok_or("usage: capture_sequence visual.json directory [--generated] [--hdr]")?;
    let directory = PathBuf::from(args.next().ok_or("missing output directory")?);
    let flags: Vec<_> = args.collect();
    let hdr = flags.iter().any(|flag| flag == "--hdr");
    let calibration = parse_calibration(&std::fs::read(path)?)?;
    let mut config = LookingGlassConfig::new(
        LookingGlassOutput::LightField {
            calibration,
            layout: ViewLayout::new(4, 2, 128, 128)?,
            depth_scale: 1.0,
        },
        RenderLayers::layer(0),
        1,
    );
    if flags.iter().any(|flag| flag == "--generated") {
        config.view_generation = ViewGeneration::DepthReprojection { rendered_views: 3 };
    }
    config.view_render.hdr = hdr;
    config.window.title = "Looking Glass capture — four fixed-time frames".into();
    config.window.resolution.set_physical_resolution(360, 640);
    std::fs::create_dir_all(&directory)?;
    let mut control = LookingGlassControl::default();
    control.start(config);
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LookingGlassPlugin)
        .insert_resource(control)
        .insert_resource(bevy::winit::WinitSettings::continuous())
        .insert_resource(Sequence {
            directory,
            frame: 0,
            waiting: false,
            warmup: 120,
            writers: Vec::new(),
            hdr,
        })
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (receive, advance)
                .chain()
                .after(LookingGlassSystems::Control),
        )
        .add_systems(Update, postprocessing.after(LookingGlassSystems::Control))
        .run();
    Ok(())
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut view: ResMut<LookingGlassView>,
) {
    let volume = HologramVolume {
        focal_plane: Transform::default(),
        height: 3.0,
        near: 0.1,
        far: 50.0,
    };
    match view_pose_from_volume(&volume, 0.7) {
        Ok(pose) => view.pose = Some(pose),
        Err(error) => error!("{error}"),
    }
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 0.0, 5.0)));
    for (z, color) in [
        (0.9, Color::srgb(1.0, 0.1, 0.1)),
        (0.0, Color::srgb(0.1, 1.0, 0.1)),
        (-1.2, Color::srgb(0.1, 0.2, 1.0)),
    ] {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(0.5, 0.5, 0.5))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                emissive: color.to_linear() * 2.0,
                ..default()
            })),
            Transform::from_xyz(z * 0.7, 0.0, z),
            Animated,
        ));
    }
    commands.spawn((
        PointLight {
            intensity: 2_000_000.0,
            ..default()
        },
        Transform::from_xyz(2.0, 4.0, 4.0),
    ));
}

fn postprocessing(
    mut commands: Commands,
    cameras: Query<Entity, Added<LookingGlassViewCamera>>,
    sequence: Res<Sequence>,
) {
    if sequence.hdr {
        for camera in &cameras {
            commands.entity(camera).insert((
                Bloom::default(),
                DepthOfField {
                    aperture_f_stops: 1.0,
                    ..default()
                },
                LookingGlassSyncFocus,
            ));
        }
    }
}

fn advance(
    mut sequence: ResMut<Sequence>,
    status: Res<LookingGlassStatus>,
    mut capture: ResMut<QuiltCapture>,
    mut transforms: Query<&mut Transform, With<Animated>>,
    mut exit: MessageWriter<bevy::app::AppExit>,
) {
    if let Some(error) = &status.error {
        error!("{error}");
        exit.write(bevy::app::AppExit::error());
        return;
    }
    if sequence.warmup > 0 {
        sequence.warmup -= 1;
        return;
    }
    if sequence.waiting {
        return;
    }
    if sequence.frame >= 4 {
        if sequence.writers.iter().all(JoinHandle::is_finished) {
            let mut failed = false;
            for writer in sequence.writers.drain(..) {
                match writer.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        error!("{error}");
                        failed = true;
                    }
                    Err(_) => {
                        error!("PNG worker failed");
                        failed = true;
                    }
                }
            }
            exit.write(if failed {
                bevy::app::AppExit::error()
            } else {
                bevy::app::AppExit::Success
            });
        }
        return;
    }
    let Some(output) = status.window else {
        return;
    };
    let time = Duration::from_nanos(sequence.frame * 1_000_000_000 / 30);
    // No wall-clock delta drives the animation. Keep this pose until readback.
    for mut transform in &mut transforms {
        transform.rotation = Quat::from_rotation_y(time.as_secs_f32() * 2.0);
    }
    capture.request(CaptureRequest {
        output,
        frame: sequence.frame,
        time,
    });
    sequence.waiting = true;
}

fn receive(
    mut results: MessageReader<QuiltCaptureResult>,
    mut sequence: ResMut<Sequence>,
    mut exit: MessageWriter<bevy::app::AppExit>,
) {
    for result in results.read() {
        match &result.result {
            Ok(quilt) => {
                let path = match quilt.metadata.filename_suffix() {
                    Ok(suffix) => sequence
                        .directory
                        .join(format!("frame_{:06}{suffix}.png", quilt.request.frame)),
                    Err(error) => {
                        error!("{error}");
                        exit.write(bevy::app::AppExit::error());
                        continue;
                    }
                };
                let owned = CapturedQuilt {
                    request: quilt.request,
                    metadata: quilt.metadata,
                    rgba: quilt.rgba.clone(),
                };
                sequence.writers.push(std::thread::spawn(move || {
                    owned.save_png(path).map_err(|error| error.to_string())
                }));
                sequence.frame += 1;
                sequence.waiting = false;
            }
            Err(error) => {
                error!("{error}");
                exit.write(bevy::app::AppExit::error());
            }
        }
    }
}
