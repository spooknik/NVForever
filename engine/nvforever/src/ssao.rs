//! Screen-space ambient occlusion of the enhanced graphics (`graphics.rs`,
//! docs/design/graphics-enhanced.md §3.1): creases, corners and the ground under things
//! are darkened by how much of the space around them the depth prepass shows filled.
//!
//! Bevy's own SSAO needs a normal prepass and a view without multisampling; the game's
//! views are multisampled (alpha-tested plants are smoothed with it) and the NIF, grass,
//! tree and water prepasses write depth only. So this pass works from the depth alone:
//! normals from the neighbouring depths, a 12-sample spiral turned by a 4 x 4 pattern,
//! worked out at half resolution (one pixel of each 2 x 2 block, a quarter of the
//! work: about 1.0-1.25 ms a frame at 1920x1080 at full resolution, #1370), the result
//! blurred over those 4 x 4 texels at about the same distance and multiplied
//! into the picture after tonemapping, before the image-space grade and its blur
//! (imagespace.rs). It darkens all light, not only the ambient light as the lighting
//! itself could [engineering choice: the strength is kept low for that]. While an upscaler
//! (FSR, DLSS or Bilinear) upscales it runs at the render size on the HDR picture before the
//! upscaler instead ([`ao_before_fsr`]; the names say FSR, the first to use it).
//!
//! On the main camera only, while it carries [`AmbientOcclusion`].

use crate::fly::FlyCam;
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntries, BindingResource, BlendComponent, BlendFactor, BlendOperation,
    BlendState, CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, FilterMode, FragmentState, LoadOp, Operations, PipelineCache,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderStages, ShaderType, StoreOp, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::texture::{CachedTexture, TextureCache};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};

/// Radius of the occlusion around a point, in game units (about 0.9 m) [engineering
/// choice: the size of a doorstep or a crate's foot; larger darkened whole walls].
const RADIUS: f32 = 64.0;
/// How strongly a fully closed point is darkened (0..1) [engineering choice, by eye].
const STRENGTH: f32 = 0.55;
/// Largest radius on the screen, in pixels (near things, e.g. the player's arms).
const MAX_SCREEN_RADIUS: f32 = 80.0;
/// The effect fades out between these distances (game units) [engineering choice: beyond
/// them the radius is a pixel or two and the fog takes over].
const FADE_START: f32 = 4000.0;
const FADE_END: f32 = 7000.0;

/// Ambient occlusion on a camera; the projection part is kept up to date by
/// [`follow_projection`].
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType)]
#[extract_app(bevy::render::RenderApp)]
pub struct AmbientOcclusion {
    /// Radius, strength, near plane, fade end.
    pub params: Vec4,
    /// Projection x and y scale, largest screen radius, fade start.
    pub proj: Vec4,
    /// The size the depth prepass was drawn at (x, y) and the picture's size (z, w): all 1
    /// here; set in the render world while a temporal upscaler upscales (`upscaler/prep.rs`
    /// `prepare`), whose common passes expand that depth to the picture's size, so the normals
    /// are taken from the texels as drawn (ssao.wesl `normal_at`). The extraction puts 1 back
    /// every frame.
    pub render_scale: Vec4,
    /// While the occlusion is applied before the upscaler ([`ao_before_fsr`]): the render
    /// size (x, y), the part of the depth prepass drawn, and z 1. Set in the render world by
    /// `upscaler/prep.rs` `prepare`; 0 otherwise (the extraction puts 0 back every frame).
    pub area: Vec4,
}

impl Default for AmbientOcclusion {
    fn default() -> Self {
        AmbientOcclusion {
            params: Vec4::new(RADIUS, STRENGTH, 5.0, FADE_END),
            proj: Vec4::new(1.0, 1.0, MAX_SCREEN_RADIUS, FADE_START),
            render_scale: Vec4::ONE,
            area: Vec4::ZERO,
        }
    }
}

impl AmbientOcclusion {
    /// Under FSR: the occlusion at the render size, on the HDR picture before FSR
    /// ([`ao_before_fsr`]), with the largest radius on the screen scaled to the render size
    /// so it covers what it did at the display size.
    pub(crate) fn before_fsr(&mut self, render_size: UVec2, output_size: UVec2) {
        self.area = Vec4::new(render_size.x as f32, render_size.y as f32, 1.0, 0.0);
        self.proj.z *= render_size.y as f32 / output_size.y.max(1) as f32;
    }

    fn runs_before_fsr(&self) -> bool {
        self.area.z > 0.5
    }
}

/// Whether the occlusion is applied before FSR while FSR upscales ([`ao_before_fsr`]);
/// `NVF_AO_BEFORE_FSR=0` applies it after tonemapping, at the display size, as without FSR
/// (the way of builds before 2026-11-08), for comparisons.
pub(crate) static AO_BEFORE_FSR: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| !std::env::var("NVF_AO_BEFORE_FSR").is_ok_and(|v| v.trim() == "0"));

pub struct SsaoPlugin;

impl Plugin for SsaoPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "ssao.wesl");
        app.add_plugins((ExtractComponentPlugin::<AmbientOcclusion>::default(), UniformComponentPlugin::<AmbientOcclusion>::default()))
            .add_systems(PostUpdate, follow_projection.after(bevy::camera::CameraUpdateSystems));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .add_systems(RenderStartup, init_pipelines)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources))
            .add_systems(Core3d, ao_pass.in_set(Core3dSystems::PostProcess).after(tonemapping).before(bevy::anti_alias::fxaa::fxaa).before(bevy::anti_alias::smaa::smaa).before(crate::imagespace::blur_pass))
            .add_systems(Core3d, ao_before_fsr.in_set(Core3dSystems::EarlyPostProcess).before(crate::fsr::fsr_pass).before(crate::dlss::dlss_pass));
    }
}

/// The camera's projection into the uniform (the aspect follows the window).
fn follow_projection(mut cams: Query<(&Projection, &mut AmbientOcclusion), With<FlyCam>>) {
    for (proj, mut ao) in &mut cams {
        let Projection::Perspective(p) = proj else { continue };
        let y = 1.0 / (p.fov * 0.5).tan();
        let x = y / p.aspect_ratio.max(1e-3);
        let params = Vec4::new(RADIUS, STRENGTH, p.near, FADE_END);
        let projv = Vec4::new(x, y, MAX_SCREEN_RADIUS, FADE_START);
        if ao.params != params || ao.proj != projv {
            ao.params = params;
            ao.proj = projv;
        }
    }
}

#[derive(Resource)]
pub(crate) struct AoPipelines {
    /// The occlusion pass's layouts with a single-sampled and a multisampled depth.
    occlusion_layouts: [BindGroupLayoutDescriptor; 2],
    /// The apply pass's.
    apply_layouts: [BindGroupLayoutDescriptor; 2],
    /// The multiply pass's before FSR (FSR runs without multisampling).
    multiply_layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    occlusion_base: RenderPipelineDescriptor,
    apply_base: RenderPipelineDescriptor,
    /// Occlusion pipeline by multisampling; apply pipeline by target format and it.
    occlusion: HashMap<bool, CachedRenderPipelineId>,
    apply: HashMap<(TextureFormat, bool), CachedRenderPipelineId>,
    multiply_base: RenderPipelineDescriptor,
    /// Before FSR: the factor multiplied into FSR's HDR input and its opaque copy (queued
    /// the first time a view needs it).
    multiply: Option<CachedRenderPipelineId>,
}

/// A view's pipelines, whether its depth is multisampled, and its occlusion texture.
#[derive(Component)]
pub(crate) struct AoView {
    occlusion: CachedRenderPipelineId,
    apply: CachedRenderPipelineId,
    multi: bool,
    /// The picture's format the apply pipeline was built for.
    format: TextureFormat,
    texture: CachedTexture,
    /// Applied before FSR this frame ([`ao_before_fsr`]), not after tonemapping.
    before_fsr: bool,
}

/// FSR's input and its opaque copy: the HDR format FSR requires (fsr.rs `prepare`).
const BEFORE_FSR_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

/// How open a texel is, and its distance (the apply pass weighs texels by it).
const OCCLUSION_FORMAT: TextureFormat = TextureFormat::Rg16Float;

fn init_pipelines(mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>) {
    let occlusion_layout = |multi: bool| {
        let depth = if multi { texture_depth_2d_multisampled() } else { texture_depth_2d() };
        BindGroupLayoutDescriptor::new("ssao_occlusion_layout", &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (depth, uniform_buffer::<AmbientOcclusion>(true))))
    };
    let apply_layout = |multi: bool| {
        let depth = if multi { texture_depth_2d_multisampled() } else { texture_depth_2d() };
        BindGroupLayoutDescriptor::new(
            "ssao_apply_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    depth,
                    uniform_buffer::<AmbientOcclusion>(true),
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    texture_2d(TextureSampleType::Float { filterable: false }),
                ),
            ),
        )
    };
    // The same bindings as `apply`'s, without the picture and its sampler (ssao.wesl).
    let multiply_layout = BindGroupLayoutDescriptor::new(
        "ssao_multiply_layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::FRAGMENT,
            ((0, texture_depth_2d()), (1, uniform_buffer::<AmbientOcclusion>(true)), (4, texture_2d(TextureSampleType::Float { filterable: false }))),
        ),
    );
    let shader = assets.load("embedded://nvforever/ssao.wesl");
    let base = |label: &'static str, entry: &'static str| RenderPipelineDescriptor {
        label: Some(label.into()),
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState { shader: shader.clone(), entry_point: Some(entry.into()), ..default() }),
        ..default()
    };
    let mut multiply_base = base("ssao_multiply_pipeline", "multiply");
    multiply_base.layout = vec![multiply_layout.clone()];
    if let Some(f) = multiply_base.fragment.as_mut() {
        // The picture times the factor; its alpha (the marks the post passes read) kept.
        let blend = BlendState {
            color: BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::Src, operation: BlendOperation::Add },
            alpha: BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::One, operation: BlendOperation::Add },
        };
        f.targets = vec![Some(ColorTargetState { format: BEFORE_FSR_FORMAT, blend: Some(blend), write_mask: ColorWrites::ALL })];
    }
    let sampler = device.create_sampler(&SamplerDescriptor { mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear, ..default() });
    commands.insert_resource(AoPipelines {
        occlusion_layouts: [occlusion_layout(false), occlusion_layout(true)],
        apply_layouts: [apply_layout(false), apply_layout(true)],
        multiply_layout,
        sampler,
        occlusion_base: base("ssao_occlusion_pipeline", "occlusion"),
        apply_base: base("ssao_apply_pipeline", "apply"),
        occlusion: HashMap::default(),
        apply: HashMap::default(),
        multiply_base,
        multiply: None,
    });
}

fn prepare(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    mut pipelines: ResMut<AoPipelines>,
    mut textures: ResMut<TextureCache>,
    device: Res<RenderDevice>,
    views: Query<(Entity, &ExtractedView, &Msaa, &AmbientOcclusion)>,
) {
    for (entity, view, msaa, settings) in &views {
        let before_fsr = settings.runs_before_fsr();
        if before_fsr && pipelines.multiply.is_none() {
            pipelines.multiply = Some(cache.queue_render_pipeline(pipelines.multiply_base.clone()));
        }
        let multi = msaa.samples() > 1;
        let format = view.target_format;
        let occlusion = match pipelines.occlusion.get(&multi) {
            Some(id) => *id,
            None => {
                let mut d = pipelines.occlusion_base.clone();
                d.layout = vec![pipelines.occlusion_layouts[multi as usize].clone()];
                if let Some(f) = d.fragment.as_mut() {
                    f.targets = vec![Some(ColorTargetState { format: OCCLUSION_FORMAT, blend: None, write_mask: ColorWrites::ALL })];
                    if multi {
                        f.shader_defs.push("MULTISAMPLED".into());
                    }
                }
                let id = cache.queue_render_pipeline(d);
                pipelines.occlusion.insert(multi, id);
                id
            }
        };
        let apply = match pipelines.apply.get(&(format, multi)) {
            Some(id) => *id,
            None => {
                let mut d = pipelines.apply_base.clone();
                d.layout = vec![pipelines.apply_layouts[multi as usize].clone()];
                if let Some(f) = d.fragment.as_mut() {
                    f.targets = vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })];
                    if multi {
                        f.shader_defs.push("MULTISAMPLED".into());
                    }
                }
                let id = cache.queue_render_pipeline(d);
                pipelines.apply.insert((format, multi), id);
                id
            }
        };
        // Half resolution, rounded up (texel q stands for the picture's pixel 2q); before FSR,
        // half its render size.
        let picture = if before_fsr { settings.area.truncate().truncate().as_uvec2() } else { view.viewport.zw() };
        let size = ((picture + UVec2::ONE) / 2).max(UVec2::ONE);
        let texture = textures.get(
            &device,
            TextureDescriptor {
                label: Some("ssao_openness"),
                size: Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: OCCLUSION_FORMAT,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        );
        commands.entity(entity).insert(AoView { occlusion, apply, multi, format, texture, before_fsr });
    }
}

/// Whether a pipeline built for `format` may draw into the view's picture now. In the
/// frame the view turns HDR or back (the light glow switched, `graphics.rs`) the view's
/// format and its main texture can disagree for a frame; a pipeline set on a pass of
/// the other format is a validation error that ends the run, so the pass is skipped.
pub(crate) fn format_matches(target: &ViewTarget, format: TextureFormat) -> bool {
    target.main_texture_format() == format
}

/// Whether the view's depth prepass texture is multisampled this frame. Read from the
/// texture itself: in the frame the multisampling changes (temporal anti-aliasing switched
/// on or off, `graphics.rs`) the view's `Msaa` and its prepass texture may disagree, and a
/// pass bound to the other kind of texture is a validation error that ends the run.
pub(crate) fn depth_multisampled(prepass: &ViewPrepassTextures) -> Option<bool> {
    prepass.depth.as_ref().map(|d| d.texture.texture.sample_count() > 1)
}

/// The occlusion into the view's texture.
#[allow(clippy::too_many_arguments)]
fn occlusion_pass(
    ctx: &mut RenderContext,
    device: &RenderDevice,
    layout: &BindGroupLayout,
    pipeline: &RenderPipeline,
    depth: &TextureView,
    binding: BindingResource<'_>,
    index: u32,
    ao: &AoView,
) {
    let bind_group = device.create_bind_group("ssao_occlusion_bind_group", layout, &BindGroupEntries::sequential((depth, binding)));
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("ssao_occlusion_pass"),
        color_attachments: &[Some(RenderPassColorAttachment { view: &ao.texture.default_view, depth_slice: None, resolve_target: None, ops: Operations::default() })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &bind_group, &[index]);
    pass.draw(0..3, 0..1);
}

/// While an upscaler upscales: the occlusion worked out at its render size from the depth as
/// drawn (before the common passes expand it) and multiplied into the render-size rectangle
/// of the HDR picture the upscaler reads, and, under FSR, of FSR's opaque copy, so its masks
/// compare the two as before. A temporal upscaler (FSR, DLSS) then accumulates it with
/// everything else: the occlusion is jittered with the scene and smoothed by it; Bilinear
/// (#2414) stretches it with the picture, unjittered. Either way at about half the cost of the
/// pass at the display size (FSR Quality: the render size has 0.44 of the pixels). It darkens
/// the HDR light before tonemapping, so bright surfaces lose less than when it is multiplied
/// into the tonemapped picture. Runs before every upscaler's pass (`fsr::fsr_pass` and
/// `dlss::dlss_pass` are ordered after it here; `upscaler::bilinear::bilinear_pass` orders
/// itself after it, `upscaler/prep.rs` `PrepPlugin`).
#[allow(clippy::type_complexity)]
pub(crate) fn ao_before_fsr(
    view: ViewQuery<(&ViewTarget, &DynamicUniformIndex<AmbientOcclusion>, &AoView, &ViewPrepassTextures, &crate::upscaler::prep::PreparedUpscale, Option<&crate::fsr::PreparedFsr>)>,
    pipelines: Res<AoPipelines>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<AmbientOcclusion>>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, index, ao, prepass, prepared, fsr) = view.into_inner();
    if !ao.before_fsr || depth_multisampled(prepass) != Some(false) || !format_matches(target, BEFORE_FSR_FORMAT) {
        return;
    }
    let Some(depth) = prepass.depth_only_view() else { return };
    let Some(multiply) = pipelines.multiply.and_then(|id| cache.get_render_pipeline(id)) else { return };
    let Some(occlusion) = cache.get_render_pipeline(ao.occlusion) else { return };
    let Some(binding) = uniforms.uniforms().binding() else { return };
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_ambient_occlusion");
    let layout = cache.get_bind_group_layout(&pipelines.occlusion_layouts[0]);
    occlusion_pass(&mut ctx, &device, &layout, occlusion, depth, binding.clone(), index.index(), ao);
    let layout = cache.get_bind_group_layout(&pipelines.multiply_layout);
    let bind_group = device.create_bind_group("ssao_multiply_bind_group", &layout, &BindGroupEntries::with_indices(((0, depth), (1, binding), (4, &ao.texture.default_view))));
    // FSR's opaque copy, only where FSR draws: DLSS has none.
    let opaque = fsr.map(|fsr| fsr.opaque().create_view(&default()));
    let picture: &wgpu::TextureView = target.main_texture_view();
    let size = prepared.render_size.as_vec2();
    // Loaded, not cleared (wgpu's default operations clear): the blend multiplies what is there.
    let ops = Operations { load: LoadOp::Load, store: StoreOp::Store };
    let targets = std::iter::once(("ssao_multiply_picture", picture)).chain(opaque.as_ref().map(|opaque| ("ssao_multiply_fsr_opaque", opaque)));
    for (label, view) in targets {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(0.0, 0.0, size.x, size.y, 0.0, 1.0);
        pass.set_pipeline(multiply);
        pass.set_bind_group(0, &bind_group, &[index.index()]);
        pass.draw(0..3, 0..1);
    }
    span.end(ctx.command_encoder());
}

/// The occlusion into the view's texture, then the picture multiplied by it.
#[allow(clippy::type_complexity)]
pub(crate) fn ao_pass(
    view: ViewQuery<(&ViewTarget, &DynamicUniformIndex<AmbientOcclusion>, &AoView, &ViewPrepassTextures, Has<crate::upscaler::prep::PreparedUpscale>)>,
    pipelines: Res<AoPipelines>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<AmbientOcclusion>>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, index, ao, prepass, upscaled) = view.into_inner();
    // Applied before the upscaler already ([`ao_before_fsr`]).
    if ao.before_fsr && upscaled {
        return;
    }
    if depth_multisampled(prepass) != Some(ao.multi) || !format_matches(target, ao.format) {
        return;
    }
    let Some(depth) = prepass.depth_only_view() else { return };
    let (Some(occlusion), Some(apply)) = (cache.get_render_pipeline(ao.occlusion), cache.get_render_pipeline(ao.apply)) else { return };
    let Some(binding) = uniforms.uniforms().binding() else { return };
    // GPU time of the pass (recorded with --log-frame-times, graphics.rs).
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_ambient_occlusion");
    let layout = cache.get_bind_group_layout(&pipelines.occlusion_layouts[ao.multi as usize]);
    occlusion_pass(&mut ctx, &device, &layout, occlusion, depth, binding.clone(), index.index(), ao);
    let post = target.post_process_write();
    let layout = cache.get_bind_group_layout(&pipelines.apply_layouts[ao.multi as usize]);
    let bind_group = device.create_bind_group(
        "ssao_apply_bind_group",
        &layout,
        &BindGroupEntries::sequential((depth, binding, post.source, &pipelines.sampler, &ao.texture.default_view)),
    );
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("ssao_apply_pass"),
        color_attachments: &[Some(RenderPassColorAttachment { view: post.destination, depth_slice: None, resolve_target: None, ops: Operations::default() })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(apply);
    pass.set_bind_group(0, &bind_group, &[index.index()]);
    pass.draw(0..3, 0..1);
    drop(pass);
    span.end(ctx.command_encoder());
}
