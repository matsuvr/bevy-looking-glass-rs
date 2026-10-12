//! Per-view SDR conversion and depth-composited forward reprojection.
use crate::view_array::VIEW_FORMAT;
use bevy::ecs::system::SystemParam;
use bevy::{
    asset::{embedded_asset, load_embedded_asset},
    prelude::*,
    render::{
        RenderApp, RenderStartup,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_asset::RenderAssets,
        render_resource::{
            binding_types::{texture_2d, texture_2d_array, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        texture::{GpuImage, TextureCache},
    },
};

#[derive(Clone, Debug, ShaderType)]
pub(crate) struct ReprojectionUniform {
    pub view_from_clip: Mat4,
    pub target_from_view: Mat4,
    pub size_layer: UVec4,
}

#[derive(Component, Clone, ExtractComponent)]
#[extract_app(RenderApp)]
pub(crate) struct Synthesis {
    pub color: Handle<Image>,
    pub depth: Handle<Image>,
    pub views: Handle<Image>,
    pub jobs: Vec<(u32, [ReprojectionUniform; 2])>,
}

#[derive(Resource)]
pub(crate) struct ViewPipelines {
    conversion: CachedRenderPipelineId,
    conversion_layout: BindGroupLayoutDescriptor,
    synthesis: CachedRenderPipelineId,
    synthesis_layout: BindGroupLayoutDescriptor,
}

#[derive(SystemParam)]
pub(crate) struct ViewGpu<'w, 's> {
    pub images: Res<'w, RenderAssets<GpuImage>>,
    pub pipelines: Res<'w, ViewPipelines>,
    pub cache: Res<'w, PipelineCache>,
    pub device: Res<'w, RenderDevice>,
    pub queue: Res<'w, RenderQueue>,
    texture_cache: ResMut<'w, TextureCache>,
    buffer: Local<'s, DynamicUniformBuffer<ReprojectionUniform>>,
}

pub(crate) fn install(app: &mut App) {
    embedded_asset!(app, "sdr.wesl");
    embedded_asset!(app, "reproject.wesl");
    app.add_plugins(ExtractComponentPlugin::<Synthesis>::default());
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app.add_systems(RenderStartup, init);
    }
}

fn init(mut commands: Commands, server: Res<AssetServer>, cache: Res<PipelineCache>) {
    let conversion_layout = BindGroupLayoutDescriptor::new(
        "looking_glass_sdr",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (texture_2d(TextureSampleType::Float { filterable: false }),),
        ),
    );
    let synthesis_layout = BindGroupLayoutDescriptor::new(
        "looking_glass_reprojection",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::VERTEX_FRAGMENT,
            (
                texture_2d_array(TextureSampleType::Float { filterable: false }),
                texture_2d_array(TextureSampleType::Depth),
                uniform_buffer::<ReprojectionUniform>(true),
            ),
        ),
    );
    let conversion_shader = load_embedded_asset!(server.as_ref(), "sdr.wesl");
    let synthesis_shader = load_embedded_asset!(server.as_ref(), "reproject.wesl");
    let pipeline = |shader: Handle<Shader>, layout: BindGroupLayoutDescriptor, depth: bool| {
        cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("looking_glass_views".into()),
            layout: vec![layout],
            vertex: VertexState {
                shader: shader.clone(),
                entry_point: Some("vertex".into()),
                ..default()
            },
            fragment: Some(FragmentState {
                shader,
                entry_point: Some("fragment".into()),
                targets: vec![Some(ColorTargetState {
                    format: VIEW_FORMAT,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            depth_stencil: depth.then_some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(CompareFunction::GreaterEqual),
                stencil: default(),
                bias: default(),
            }),
            ..default()
        })
    };
    commands.insert_resource(ViewPipelines {
        conversion: pipeline(conversion_shader, conversion_layout.clone(), false),
        conversion_layout,
        synthesis: pipeline(synthesis_shader, synthesis_layout.clone(), true),
        synthesis_layout,
    });
}

pub(crate) fn layer_view(texture: &Texture, layer: u32) -> TextureView {
    texture.create_view(&TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2),
        base_array_layer: layer,
        array_layer_count: Some(1),
        ..default()
    })
}

/// Convert already tone-mapped HDR/linear pixels to the public SDR array.
pub(crate) fn convert(
    source: &TextureView,
    destination: &Texture,
    layer: u32,
    pipelines: &ViewPipelines,
    cache: &PipelineCache,
    device: &RenderDevice,
    context: &mut RenderContext,
) -> bool {
    let Some(pipeline) = cache.get_render_pipeline(pipelines.conversion) else {
        return false;
    };
    let bind = device.create_bind_group(
        "looking_glass_sdr",
        &cache.get_bind_group_layout(&pipelines.conversion_layout),
        &BindGroupEntries::sequential((source,)),
    );
    let target = layer_view(destination, layer);
    let mut pass = context
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("looking_glass_sdr"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(LinearRgba::BLACK.into()),
                    store: StoreOp::Store,
                },
            })],
            ..default()
        });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &bind, &[]);
    pass.draw(0..3, 0..1);
    true
}

pub(crate) fn synthesize(
    synthesis: &Synthesis,
    gpu: &mut ViewGpu,
    context: &mut RenderContext,
) -> bool {
    let ViewGpu {
        images,
        pipelines,
        cache,
        device,
        queue,
        texture_cache,
        buffer,
    } = gpu;
    let Some(pipeline) = cache.get_render_pipeline(pipelines.synthesis) else {
        return false;
    };
    let (Some(colors), Some(depths), Some(destination)) = (
        images.get(&synthesis.color),
        images.get(&synthesis.depth),
        images.get(&synthesis.views),
    ) else {
        return false;
    };
    let size = Extent3d {
        depth_or_array_layers: 1,
        ..destination.texture.size()
    };
    let z = texture_cache.get(
        device,
        TextureDescriptor {
            label: Some("looking_glass_generated_depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth32Float,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        },
    );
    let depth_view = &z.default_view;
    buffer.clear();
    let offsets: Vec<_> = synthesis
        .jobs
        .iter()
        .map(|(index, references)| {
            (
                *index,
                references.each_ref().map(|uniform| buffer.push(uniform)),
            )
        })
        .collect();
    buffer.write_buffer(device, queue);
    let Some(binding) = buffer.binding() else {
        return false;
    };
    let bind = device.create_bind_group(
        "looking_glass_reprojection",
        &cache.get_bind_group_layout(&pipelines.synthesis_layout),
        &BindGroupEntries::sequential((&colors.texture_view, &depths.texture_view, binding)),
    );
    for (index, offsets) in &offsets {
        let target = layer_view(&destination.texture, *index);
        let mut pass = context
            .command_encoder()
            .begin_render_pass(&RenderPassDescriptor {
                label: Some("looking_glass_reprojection"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(LinearRgba::BLACK.into()),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(0.0),
                        store: StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..default()
            });
        pass.set_pipeline(pipeline);
        for offset in offsets {
            pass.set_bind_group(0, &bind, &[*offset]);
            pass.draw(0..6, 0..size.width.saturating_mul(size.height));
        }
    }
    true
}
