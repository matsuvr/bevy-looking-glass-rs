//! Display a PNG quilt without scene cameras. Usage: quilt_input visual.json
//! quilt.png columns rows view_count view_aspect [--array]. F9 saves a new quilt.
#![forbid(unsafe_code)]
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::RenderLayers,
    image::{CompressedImageFormats, ImageSampler, ImageType},
    prelude::*,
    render::render_resource::*,
};
use bevy_looking_glass::*;
use std::time::Duration;

#[derive(Resource)]
struct Input {
    config: LookingGlassConfig,
    image: Option<Image>,
    metadata: QuiltMetadata,
    array: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let calibration =
        parse_calibration(&std::fs::read(args.next().ok_or("missing visual.json")?)?)?;
    let path = args.next().ok_or("missing quilt PNG")?;
    let mut image = Image::from_buffer(
        &std::fs::read(path)?,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::default(),
        RenderAssetUsages::default(),
    )?;
    let metadata = QuiltMetadata {
        image_size: UVec2::new(image.width(), image.height()),
        columns: args.next().ok_or("missing columns")?.parse()?,
        rows: args.next().ok_or("missing rows")?.parse()?,
        view_count: args.next().ok_or("missing view_count")?.parse()?,
        view_aspect: args.next().ok_or("missing view_aspect")?.parse()?,
    };
    metadata.validate()?;
    let size = metadata.view_size()?;
    let config = LookingGlassConfig::new(
        LookingGlassOutput::LightField {
            calibration,
            layout: ViewLayout::new(metadata.columns, metadata.rows, size.x, size.y)?,
            depth_scale: 1.0,
        },
        RenderLayers::layer(0),
        1,
    );
    let array = args.any(|flag| flag == "--array");
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    if array {
        image = to_array(&image, metadata)?;
    }
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LookingGlassPlugin)
        .insert_resource(Input {
            config,
            image: Some(image),
            metadata,
            array,
        })
        .add_systems(Startup, start)
        .add_systems(Update, (keys, save).after(LookingGlassSystems::Control))
        .run();
    Ok(())
}

fn to_array(quilt: &Image, metadata: QuiltMetadata) -> Result<Image, Box<dyn std::error::Error>> {
    let size = metadata.view_size()?;
    if quilt.texture_descriptor.format != TextureFormat::Rgba8UnormSrgb {
        return Err("example requires an 8-bit RGBA quilt".into());
    }
    let source = quilt.data.as_ref().ok_or("quilt pixels unavailable")?;
    let row_bytes = (size.x as usize)
        .checked_mul(4)
        .ok_or("tile row overflow")?;
    let count = row_bytes
        .checked_mul(size.y as usize)
        .and_then(|bytes| bytes.checked_mul(metadata.view_count as usize))
        .ok_or("array size overflow")?;
    let mut pixels = Vec::with_capacity(count);
    for index in 0..metadata.view_count {
        let origin = metadata.tile_origin(index)?;
        for y in origin.y..origin.y + size.y {
            let start = (u64::from(y) * u64::from(metadata.image_size.x) + u64::from(origin.x)) * 4;
            let start = usize::try_from(start)?;
            let end = start.checked_add(row_bytes).ok_or("pixel row overflow")?;
            pixels.extend_from_slice(source.get(start..end).ok_or("incomplete quilt pixels")?);
        }
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: metadata.view_count,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.data = Some(pixels);
    image.texture_descriptor.usage =
        TextureUsages::COPY_DST | TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    Ok(image)
}

fn start(
    mut input: ResMut<Input>,
    mut images: ResMut<Assets<Image>>,
    mut control: ResMut<LookingGlassControl>,
) {
    let Some(image) = input.image.take() else {
        return;
    };
    let image = images.add(image);
    let mut config = input.config.clone();
    config.source = if input.array {
        LookingGlassSource::ViewArray {
            image,
            metadata: input.metadata,
        }
    } else {
        LookingGlassSource::Quilt {
            image,
            metadata: input.metadata,
        }
    };
    config.window.title = "External quilt — F9 capture / F10 preview / Escape stop".into();
    config.window.resolution.set_physical_resolution(360, 640);
    control.start(config);
}

fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    status: Res<LookingGlassStatus>,
    mut control: ResMut<LookingGlassControl>,
    mut capture: ResMut<QuiltCapture>,
    mut frame: Local<u64>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        control.stop();
    }
    if keys.just_pressed(KeyCode::F10) {
        control.set_quilt_preview(!status.quilt_preview);
    }
    if keys.just_pressed(KeyCode::F9)
        && let Some(output) = status.window
    {
        capture.request(CaptureRequest {
            output,
            frame: *frame,
            time: Duration::ZERO,
        });
        *frame = frame.saturating_add(1);
    }
}

fn save(mut results: MessageReader<QuiltCaptureResult>) {
    for result in results.read() {
        match &result.result {
            Ok(quilt) => {
                let Ok(suffix) = quilt.metadata.filename_suffix() else {
                    continue;
                };
                let path = format!("capture_{:06}{suffix}.png", quilt.request.frame);
                let owned = CapturedQuilt {
                    request: quilt.request,
                    metadata: quilt.metadata,
                    rgba: quilt.rgba.clone(),
                };
                std::thread::spawn(move || {
                    if let Err(error) = owned.save_png(path) {
                        error!("{error}");
                    }
                });
            }
            Err(error) => error!("{error}"),
        }
    }
}
