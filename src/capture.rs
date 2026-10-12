//! On-demand GPU quilt assembly/readback. PNG encoding runs only when the host
//! calls write_png/save_png; neither encoding nor file I/O runs on the renderer.
use crate::{
    ActiveOutput, LookingGlassFrame, LookingGlassSource, LookingGlassSystems, LookingGlassView,
    QuiltMetadata,
    view_array::{ReadyFrames, VIEW_FORMAT},
};
use bevy::{
    core_pipeline::schedule::camera_driver,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_component::ExtractComponentPlugin,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::*,
        renderer::{RenderContext, RenderDevice, RenderGraph, RenderGraphSystems},
        sync_world::MainEntity,
        texture::GpuImage,
    },
};
use std::{
    io::Write,
    path::Path,
    sync::{Mutex, mpsc},
    time::Duration,
};

/// Identity of a requested frame. The host controls simulation time and can
/// freeze it until completion to capture deterministic image sequences.
#[derive(Clone, Copy, Debug)]
pub struct CaptureRequest {
    /// Output entity carrying LookingGlassFrame.
    pub output: Entity,
    /// Host-selected frame number, preserved through asynchronous readback.
    pub frame: u64,
    /// Host-selected scene time, not the wall-clock readback completion time.
    pub time: Duration,
}

/// Capture requests consumed after LookingGlassSystems::SyncViews.
#[derive(Resource, Default)]
pub struct QuiltCapture {
    pending: Vec<CaptureRequest>,
}
impl QuiltCapture {
    /// Assemble/read back a quilt for the next available rendered frame.
    pub fn request(&mut self, request: CaptureRequest) {
        self.pending.push(request);
    }
}

/// Unpadded, top-left-oriented sRGB RGBA pixels, with bottom-left quilt ordering.
#[derive(Debug)]
pub struct CapturedQuilt {
    /// Original frame identity.
    pub request: CaptureRequest,
    /// Quilt geometry including unused tiles and edge pixels.
    pub metadata: QuiltMetadata,
    /// Four bytes per pixel, rows packed without GPU alignment padding.
    pub rgba: Vec<u8>,
}

/// Asynchronous result. Failures include stopped output, invalid capture sizes
/// and GPU mapping errors; no capture failure panics the application.
#[derive(Message, Debug)]
pub struct QuiltCaptureResult {
    /// Original frame identity.
    pub request: CaptureRequest,
    /// Captured pixels or a diagnostic error.
    pub result: Result<CapturedQuilt, String>,
}

/// PNG encoding or file creation failed.
#[derive(Debug, thiserror::Error)]
pub enum QuiltSaveError {
    /// File creation/writing failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Invalid pixels or PNG encoding failed.
    #[error(transparent)]
    Png(#[from] png::EncodingError),
}

impl CapturedQuilt {
    /// Encode lossless PNG to a caller-provided writer. Call on a worker for
    /// large quilts; this function is deliberately separate from GPU capture.
    pub fn write_png(&self, writer: impl Write) -> Result<(), png::EncodingError> {
        let mut encoder = png::Encoder::new(
            writer,
            self.metadata.image_size.x,
            self.metadata.image_size.y,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        let mut png = encoder.write_header()?;
        png.write_image_data(&self.rgba)?;
        png.finish()
    }
    /// Save a PNG using the exact host-supplied path. Use filename_suffix to
    /// include the geometry convention expected by Looking Glass applications.
    pub fn save_png(&self, path: impl AsRef<Path>) -> Result<(), QuiltSaveError> {
        self.write_png(std::io::BufWriter::new(std::fs::File::create(path)?))?;
        Ok(())
    }
}

#[derive(Clone)]
struct Job {
    request: CaptureRequest,
    metadata: QuiltMetadata,
    views: Handle<Image>,
    wait_ready: bool,
}
#[derive(Resource)]
struct MainChannel {
    results: Mutex<mpsc::Receiver<QuiltCaptureResult>>,
}
#[derive(Resource)]
struct RenderChannel {
    results: mpsc::Sender<QuiltCaptureResult>,
}
#[derive(Resource, Clone, Default, ExtractResource)]
#[extract_app(RenderApp)]
struct QueuedJobs(Vec<Job>);

#[derive(Resource, Default)]
struct Pending {
    jobs: Vec<Job>,
    buffers: Vec<(Job, Buffer, u32)>,
}

pub(crate) fn install(app: &mut App) {
    let (results_tx, results_rx) = mpsc::channel();
    app.add_plugins((
        ExtractComponentPlugin::<LookingGlassFrame>::default(),
        ExtractResourcePlugin::<QueuedJobs>::default(),
    ))
    .init_resource::<QueuedJobs>()
    .init_resource::<QuiltCapture>()
    .add_message::<QuiltCaptureResult>()
    .insert_resource(MainChannel {
        results: Mutex::new(results_rx),
    })
    .add_systems(First, collect)
    .add_systems(PostUpdate, queue.after(LookingGlassSystems::SyncViews));
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .insert_resource(RenderChannel {
                results: results_tx,
            })
            .init_resource::<Pending>()
            .add_systems(
                RenderGraph,
                copy.in_set(RenderGraphSystems::Render).after(camera_driver),
            )
            .add_systems(Render, map.in_set(RenderSystems::Cleanup));
    }
}

fn collect(channel: Res<MainChannel>, mut results: MessageWriter<QuiltCaptureResult>) {
    if let Ok(receiver) = channel.results.lock() {
        for result in receiver.try_iter() {
            results.write(result);
        }
    }
}

fn queue(
    mut requests: ResMut<QuiltCapture>,
    frames: Query<&LookingGlassFrame>,
    mut queued: ResMut<QueuedJobs>,
    config: Option<Res<ActiveOutput>>,
    view: Res<LookingGlassView>,
    device: Res<RenderDevice>,
    mut results: MessageWriter<QuiltCaptureResult>,
) {
    queued.0.clear();
    for request in requests.pending.drain(..) {
        let result = (|| {
            let frame = frames
                .get(request.output)
                .map_err(|_| "capture output is stopped or missing".to_string())?;
            frame
                .metadata
                .validate()
                .map_err(|error| error.to_string())?;
            let size = frame.metadata.image_size;
            let limits = device.limits();
            let row = aligned_row(size.x).ok_or("capture row size overflow")?;
            if size.x > limits.max_texture_dimension_2d
                || size.y > limits.max_texture_dimension_2d
                || u64::from(row) * u64::from(size.y) > limits.max_buffer_size
            {
                return Err("capture quilt exceeds GPU texture/buffer limits".into());
            }
            let source = config
                .as_ref()
                .map(|config| &config.0.source)
                .ok_or("capture output is stopped")?;
            if matches!(source, LookingGlassSource::Scene)
                && view.pose.is_none_or(|pose| pose.validate().is_err())
            {
                return Err("scene capture requires a valid view pose".into());
            }
            queued.0.push(Job {
                request,
                metadata: frame.metadata,
                views: frame.views.clone(),
                wait_ready: !matches!(source, LookingGlassSource::ViewArray { .. }),
            });
            Ok(())
        })();
        if let Err(error) = result {
            results.write(QuiltCaptureResult {
                request,
                result: Err(error),
            });
        }
    }
}

fn aligned_row(width: u32) -> Option<u32> {
    width
        .checked_mul(4)?
        .checked_add(255)
        .map(|row| row / 256 * 256)
}

fn copy(
    channel: Res<RenderChannel>,
    (mut pending, mut queued): (ResMut<Pending>, ResMut<QueuedJobs>),
    images: Res<RenderAssets<GpuImage>>,
    ready: Res<ReadyFrames>,
    frames: Query<(&MainEntity, &LookingGlassFrame)>,
    device: Res<RenderDevice>,
    mut context: RenderContext,
) {
    pending.jobs.append(&mut queued.0);
    let jobs = std::mem::take(&mut pending.jobs);
    for job in jobs {
        if !frames
            .iter()
            .any(|(entity, frame)| entity.id() == job.request.output && frame.views == job.views)
        {
            let _ = channel.results.send(QuiltCaptureResult {
                request: job.request,
                result: Err("capture output was stopped or replaced before rendering".into()),
            });
            continue;
        }
        let Some(array) = images.get(&job.views) else {
            pending.jobs.push(job);
            continue;
        };
        if job.wait_ready && !ready.0.contains(&job.views.id()) {
            pending.jobs.push(job);
            continue;
        }
        let metadata = job.metadata;
        let (Ok(tile), Some(row)) = (metadata.view_size(), aligned_row(metadata.image_size.x))
        else {
            continue;
        };
        let size = Extent3d {
            width: metadata.image_size.x,
            height: metadata.image_size.y,
            depth_or_array_layers: 1,
        };
        let quilt = device.create_texture(&TextureDescriptor {
            label: Some("looking_glass_capture_quilt"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: VIEW_FORMAT,
            usage: TextureUsages::COPY_DST | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        for index in 0..metadata.view_count {
            let Ok(origin) = metadata.tile_origin(index) else {
                continue;
            };
            context.command_encoder().copy_texture_to_texture(
                TexelCopyTextureInfo {
                    origin: Origin3d {
                        x: 0,
                        y: 0,
                        z: index,
                    },
                    ..array.texture.as_image_copy()
                },
                TexelCopyTextureInfo {
                    origin: Origin3d {
                        x: origin.x,
                        y: origin.y,
                        z: 0,
                    },
                    ..quilt.as_image_copy()
                },
                Extent3d {
                    width: tile.x,
                    height: tile.y,
                    depth_or_array_layers: 1,
                },
            );
        }
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("looking_glass_capture_readback"),
            size: u64::from(row) * u64::from(size.height),
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        context.command_encoder().copy_texture_to_buffer(
            quilt.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: None,
                },
            },
            size,
        );
        pending.buffers.push((job, buffer, row));
    }
}

fn map(channel: Res<RenderChannel>, mut pending: ResMut<Pending>) {
    for (job, buffer, row) in pending.buffers.drain(..) {
        let retained = buffer.clone();
        let sender = channel.results.clone();
        buffer.slice(..).map_async(MapMode::Read, move |mapped| {
            let result = mapped.map_err(|error| error.to_string()).and_then(|()| {
                let range = retained
                    .slice(..)
                    .get_mapped_range()
                    .map_err(|error| error.to_string())?;
                let rgba = unpad(&range, row as usize, job.metadata.image_size)
                    .ok_or("incomplete GPU quilt readback")?;
                drop(range);
                retained.unmap();
                Ok(CapturedQuilt {
                    request: job.request,
                    metadata: job.metadata,
                    rgba,
                })
            });
            let _ = sender.send(QuiltCaptureResult {
                request: job.request,
                result,
            });
        });
    }
}

fn unpad(data: &[u8], row: usize, size: UVec2) -> Option<Vec<u8>> {
    let width = (size.x as usize).checked_mul(4)?;
    let len = width.checked_mul(size.y as usize)?;
    if row < width || data.len() < row.checked_mul(size.y as usize)? {
        return None;
    }
    let mut rgba = Vec::with_capacity(len);
    for line in data.chunks_exact(row).take(size.y as usize) {
        rgba.extend_from_slice(line.get(..width)?);
    }
    Some(rgba)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic)]
    use super::*;
    #[test]
    fn strips_gpu_row_padding_and_rejects_truncation() {
        let mut data = vec![99; 512];
        for (i, byte) in data.iter_mut().take(4).enumerate() {
            *byte = i as u8;
        }
        for (i, byte) in data.iter_mut().skip(256).take(4).enumerate() {
            *byte = i as u8 + 4;
        }
        assert_eq!(unpad(&data, 256, UVec2::new(1, 2)), Some((0..8).collect()));
        assert!(unpad(&data, 256, UVec2::new(1, 3)).is_none());
        assert_eq!(aligned_row(65), Some(512));
        assert_eq!(aligned_row(u32::MAX), None);
    }

    #[test]
    fn png_round_trip_preserves_dimensions_pixels_and_reports_write_failure() {
        let quilt = CapturedQuilt {
            request: CaptureRequest {
                output: Entity::PLACEHOLDER,
                frame: 9,
                time: Duration::from_millis(300),
            },
            metadata: QuiltMetadata {
                image_size: UVec2::new(2, 2),
                columns: 2,
                rows: 1,
                view_count: 2,
                view_aspect: 0.75,
            },
            rgba: (0..16).collect(),
        };
        let mut encoded = Vec::new();
        assert!(quilt.write_png(&mut encoded).is_ok());
        let decoded = png::Decoder::new(std::io::Cursor::new(encoded)).read_info();
        let Ok(mut decoder) = decoded else {
            panic!("PNG decode failed");
        };
        let mut pixels = vec![0; 16];
        let Ok(info) = decoder.next_frame(&mut pixels) else {
            panic!("PNG frame failed");
        };
        assert_eq!((info.width, info.height), (2, 2));
        assert_eq!(pixels, quilt.rgba);
        struct FailingWriter;
        impl Write for FailingWriter {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("test write error"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::Error::other("test flush error"))
            }
        }
        assert!(quilt.write_png(FailingWriter).is_err());
    }
}
