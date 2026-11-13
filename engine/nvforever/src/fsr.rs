//! FSR 3.1.4 temporal upscaling through the MIT-licensed wgpu-ffx source port.
//! See docs/design/fsr-upscaling.md for the render contract, measurements and limits.
//!
//! The scene is rendered into the top-left, lower-resolution part of Bevy's HDR
//! target. FSR reconstructs the entire target before bloom, tonemapping and UI.
//! Opaque/transparent colour differences supply a conservative reactive mask; the sky's
//! blended parts and the terrain's blended texture layers count as opaque there
//! ([`reference_sky_and_terrain_layers`]). Last frame's mask, where each pixel's history
//! comes from, and light that moved away from FSR's last output mark the history that would
//! trail (`fsr_reactive.wgsl`).
//! The depth prepass is expanded afterwards for NVForever's screen-space effects.
//!
//! What any temporal upscaler needs (camera history and cuts, the render size, jitter and mip
//! bias applied, the corrected and expanded depth, the resolution uniform, the spatial fallback
//! and the alpha copy) is the common preparation in `upscaler/prep.rs`, which runs first; this
//! module adds FSR's own: its [`policy`] (ratios, phase count, mip bias), its context, the
//! opaque copy and reference, the masks and their history, and the dilation buffers
//! ([`PreparedFsr`]), made only for a camera FSR draws.
//!
//! A FidelityFX API library in the game's folder (ffx_api.rs, the FSR library setting) takes
//! the port's place for the reconstruction itself ([`Upscaler::Api`]): it is given the same
//! colour, motion and masks, the common corrected depth as a colour image, and the output.

use std::{ops::{Deref, Range}, sync::{atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering}, Mutex}};

use bevy::camera::{CameraMainTextureUsages, Hdr, MainPassResolutionOverride, Viewport};
use bevy::core_pipeline::{
    core_3d::{main_opaque_pass_3d, main_transparent_pass_3d, Transparent3d},
    prepass::{DepthPrepass, MotionVectorPrepass, ViewPrepassTextures},
    Core3d, Core3dSystems,
};
use bevy::prelude::*;
use bevy::render::{
    camera::{ExtractedCamera, MipBias, TemporalJitter},
    diagnostic::RecordDiagnostics,
    render_phase::ViewSortedRenderPhases,
    renderer::{RenderAdapter, RenderContext, RenderDevice, RenderQueue, ViewQuery},
    view::{prepare_view_targets, ExtractedView, ViewDepthStencilTexture, ViewTarget},
    Render, RenderApp, RenderSystems,
};
use wgpu::util::DeviceExt;
use wgpu_ffx::{FsrContext, FsrContextFlags, FsrContextInfo, FsrDispatchFlags, FsrDispatchInfo, FsrView};

use crate::upscaler::prep::{self, Allocation, Policy, PreparedUpscale, UpscaleRenderer, UpscaleSettings};

/// AMD's standard fixed-resolution modes. Native AA reconstructs at 1:1. The quality list is
/// shared with DLSS (`upscaler.rs`); the ratios below are FSR's alone.
pub type FsrQuality = crate::upscaler::UpscaleQuality;

impl FsrQuality {
    pub fn ratio(self) -> f32 {
        match self {
            Self::NativeAa => 1.0,
            Self::Quality => 1.5,
            Self::Balanced => 1.7,
            Self::Performance => 2.0,
            Self::UltraPerformance => 3.0,
        }
    }

    pub fn render_size(self, output: UVec2) -> UVec2 {
        (output.as_vec2() / self.ratio()).as_uvec2().max(UVec2::ONE)
    }
}

/// Enable FSR on a perspective HDR camera. The graphics settings enforce Msaa::Off
/// and remove competing temporal/spatial anti-aliasing components.
#[derive(Component, Clone, Copy, Debug)]
#[require(Hdr, DepthPrepass, MotionVectorPrepass, TemporalJitter, MipBias)]
pub struct FsrUpscaling {
    pub quality: FsrQuality,
    pub sharpness: f32,
}

impl Default for FsrUpscaling {
    fn default() -> Self {
        Self { quality: FsrQuality::Quality, sharpness: 0.2 }
    }
}

pub struct FsrPlugin;

/// What is added to log2(render / display) for the texture mip bias: AMD's −1 [documented:
/// AMD's FSR 3.1 guide], or `NVF_FSR_MIP_BIAS_OFFSET` for comparisons (a diagnostic).
static MIP_BIAS_OFFSET: std::sync::LazyLock<f32> = std::sync::LazyLock::new(|| {
    let offset = std::env::var("NVF_FSR_MIP_BIAS_OFFSET").ok().and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite());
    if let Some(offset) = offset {
        info!("FSR mip bias: log2(render / display) {offset:+} (NVF_FSR_MIP_BIAS_OFFSET)");
    }
    offset.unwrap_or(-1.0)
});

/// AMD's half-precision variant of FSR's accumulate pass where the GPU has 16-bit shader
/// floats and integers (wgpu-ffx `FsrContextFlags::ALLOW_FP16`); `NVF_FSR_FP16=0` keeps full
/// precision, for comparisons.
static FP16: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| !std::env::var("NVF_FSR_FP16").is_ok_and(|v| v.trim() == "0"));

/// Whether this device can execute the audited Vulkan SPIR-V shader path.
#[derive(Resource, Clone, Copy, Default)]
pub struct FsrSupport(pub bool);

impl Plugin for FsrPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<FsrRenderer>()
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareViews).after(prep::prepare).before(prepare_view_targets))
            .add_systems(
                Core3d,
                capture_opaque
                    .in_set(Core3dSystems::MainPass)
                    .after(main_opaque_pass_3d)
                    .before(bevy::pbr::main_transmissive_pass_3d)
                    .before(main_transparent_pass_3d),
            )
            .add_systems(
                Core3d,
                reference_sky_and_terrain_layers
                    .in_set(Core3dSystems::MainPass)
                    .after(capture_opaque)
                    .after(bevy::pbr::main_transmissive_pass_3d)
                    .before(main_transparent_pass_3d),
            )
            .add_systems(
                Core3d,
                fsr_pass
                    .in_set(Core3dSystems::EarlyPostProcess)
                    .before(bevy::post_process::bloom::bloom)
                    .before(crate::post::bright_pass_hdr),
            );
    }

    fn finish(&self, app: &mut App) {
        let supported = app.get_sub_app(RenderApp).is_some_and(|render| {
            let world = render.world();
            let (Some(adapter), Some(device)) = (world.get_resource::<RenderAdapter>(), world.get_resource::<RenderDevice>()) else { return false };
            let features = wgpu::Features::PASSTHROUGH_SHADERS | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
                | wgpu::Features::FLOAT32_FILTERABLE | wgpu::Features::TEXTURE_FORMAT_16BIT_NORM | wgpu::Features::CLEAR_TEXTURE;
            adapter.get_info().backend == wgpu::Backend::Vulkan && device.features().contains(features) && supports_limits(&device.limits())
                && [wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::R16Float, wgpu::TextureFormat::Rg16Float,
                    wgpu::TextureFormat::Rgba16Float, wgpu::TextureFormat::Rgba32Float]
                    .into_iter().all(|format| {
                        let properties = adapter.get_texture_format_features(format);
                        properties.allowed_usages.contains(wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING)
                            && properties.flags.contains(wgpu::TextureFormatFeatureFlags::STORAGE_READ_WRITE)
                    })
        });
        app.insert_resource(FsrSupport(supported));
        if !supported {
            info!("FSR 3.1 unavailable: Vulkan native SPIR-V and the required storage/filterable texture formats are needed; using native rendering");
        }
    }
}

/// Maxima from the pinned port's pass.rs and full-precision SPD shader: 256
/// threads; four 16x16 float shared arrays plus a uint; eight storage textures;
/// nine sampled textures; fifteen total bindings; 240 bytes of constants.
fn supports_limits(limits: &wgpu::Limits) -> bool {
    limits.max_compute_invocations_per_workgroup >= 256
        && limits.max_compute_workgroup_size_x >= 256
        && limits.max_compute_workgroup_size_y >= 8
        && limits.max_compute_workgroup_size_z >= 1
        && limits.max_compute_workgroup_storage_size >= 4100
        && limits.max_storage_textures_per_shader_stage >= 8
        && limits.max_storage_buffers_per_shader_stage >= 1
        && limits.max_sampled_textures_per_shader_stage >= 9
        && limits.max_samplers_per_shader_stage >= 2
        && limits.max_uniform_buffers_per_shader_stage >= 1
        && limits.max_uniform_buffer_binding_size >= 240
        && limits.max_bindings_per_bind_group >= 15
        && limits.max_bind_groups >= 1
}

fn supports_size(limits: &wgpu::Limits, render: UVec2, output: UVec2) -> bool {
    // The port binds six SPD mip levels from a half-render-sized texture.
    let reconstruction_bytes = render.x as u64 * render.y as u64 * 4;
    render.min_element() > 0 && render.max_element() >= 64
        && output.max_element() <= limits.max_texture_dimension_2d
        && reconstruction_bytes <= limits.max_buffer_size
        && reconstruction_bytes <= limits.max_storage_buffer_binding_size
        && output.max_element().div_ceil(8) <= limits.max_compute_workgroups_per_dimension
}

/// FSR's [`Policy`] for a view of `output` size: AMD's fixed ratio for the quality, AMD's jitter
/// phase count for the two widths, and the mip bias log2(render / display) plus AMD's −1 (or
/// `NVF_FSR_MIP_BIAS_OFFSET`). `None` where the size is beyond what the port can run
/// ([`supports_size`]): the view renders natively.
pub(crate) fn policy(quality: FsrQuality, output: UVec2, limits: &wgpu::Limits) -> Option<Policy> {
    let render_size = quality.render_size(output);
    if !supports_size(limits, render_size, output) {
        return None;
    }
    Some(Policy {
        render_size,
        jitter_phases: wgpu_ffx::get_jitter_phase_count(render_size.x as i32, output.x as i32),
        mip_bias: (render_size.x as f32 / output.x as f32).log2() + *MIP_BIAS_OFFSET,
    })
}

#[derive(Resource, Default)]
pub(crate) struct FsrRenderer {
    auxiliary: Option<AuxiliaryPipelines>,
    /// The wgpu-ffx port's context, made the first time a view upscales with it: not at all
    /// while a FidelityFX API library does the upscaling.
    port: Option<FsrContext>,
}

/// What reconstructs a view.
enum Upscaler {
    /// The vendored wgpu-ffx port, with the three resources FSR writes beside its output.
    Port { view: Box<Mutex<FsrView>>, dilated_depth: wgpu::Texture, dilated_motion: wgpu::Texture, reconstructed_depth: wgpu::Buffer },
    /// A FidelityFX API library in the game's folder (ffx_api.rs), with the corrected depth as
    /// the colour image it is given, the dispatches that failed in a row, and the view's main
    /// textures the library has been given to write ([`Upscaler::first_output`]).
    Api { context: Mutex<crate::ffx_api::Upscaler>, depth: wgpu::Texture, failures: AtomicU32, outputs: Mutex<Vec<wgpu::Texture>> },
}

impl Upscaler {
    /// Whether a FidelityFX API library is given this output for the first time. wgpu clears
    /// a texture it has seen no write to before the first read, and it does not see the
    /// library's: left alone, that clear would wipe the first upscaled frame. The caller
    /// clears such a texture itself, before the library writes. A view has two main textures;
    /// the two given last are remembered.
    fn first_output(outputs: &Mutex<Vec<wgpu::Texture>>, output: &wgpu::Texture) -> bool {
        let mut outputs = outputs.lock().unwrap();
        if outputs.contains(output) {
            return false;
        }
        if outputs.len() == 2 {
            outputs.remove(0);
        }
        outputs.push(output.clone());
        true
    }
}

/// A FidelityFX API library whose dispatch fails this many frames in a row is not used any
/// more: the port takes over [our choice].
const API_FAILURES: u32 = 3;

/// The port's context, with its pipelines.
fn port_context(device: &wgpu::Device) -> FsrContext {
    let mut flags = FsrContextFlags::HIGH_DYNAMIC_RANGE | FsrContextFlags::DEPTH_INVERTED | FsrContextFlags::DEPTH_INFINITE | FsrContextFlags::AUTO_EXPOSURE;
    if *FP16 {
        flags |= FsrContextFlags::ALLOW_FP16;
    }
    let context = FsrContext::new(FsrContextInfo { device: device.clone(), flags });
    let shaders = if context.uses_fp16() {
        "half precision (accumulate pass)"
    } else if *FP16 {
        "full precision (the GPU lacks 16-bit shader floats or integers)"
    } else {
        "full precision (NVF_FSR_FP16=0)"
    };
    info!("FSR 3.1 shaders: {shaders}");
    context
}

/// FSR's own state beside the common prepared view ([`PreparedUpscale`]): only a camera FSR
/// draws has one.
#[derive(Component)]
pub(crate) struct PreparedFsr {
    upscaler: Upscaler,
    /// The sizes it was made for (the prepared view's when it was made).
    pub(crate) render_size: UVec2,
    pub(crate) output_size: UVec2,
    opaque: wgpu::Texture,
    /// FSR's reactive and transparency/composition masks (`fsr_reactive.wgsl`).
    reactive: wgpu::Texture,
    composition: wgpu::Texture,
    /// The transparent phase's presence, this frame's and last frame's: written to
    /// `presence[presence_written]`, and the other is read.
    presence: [wgpu::Texture; 2],
    presence_written: AtomicUsize,
    /// This frame's light change (render size): each texel's luminance against FSR's last
    /// output at the same surface point, from the masks' first pass.
    light_change: wgpu::Texture,
    /// The luminance of FSR's last output (render size), for the masks' light term; cleared
    /// when FSR did not reconstruct.
    history_luma: wgpu::Texture,
    /// FSR's jitter this frame, for the masks (render pixels, AMD's sign).
    mask_jitter: wgpu::Buffer,
    dispatch_logged: AtomicBool,
    reference_logged: AtomicBool,
    sky_logged: AtomicBool,
}

/// Whether a view keeps (or gets) FSR's own state: only while its prepared view is FSR's. A view
/// without a prepared view (native, an unusable view) or drawn by another upscaler (DLSS,
/// Bilinear, #2414) loses it.
pub(crate) fn keeps_fsr_state(prepared: Option<UpscaleSettings>) -> bool {
    matches!(prepared, Some(UpscaleSettings::Fsr(_)))
}

/// FSR's part of the view preparation, after the common one (`upscaler::prep::prepare`, which
/// checked the view, applied FSR's [`policy`] and made or kept the [`PreparedUpscale`]): the
/// pipelines, the main textures' usages FSR needs, and [`PreparedFsr`], made again when the
/// prepared view's sizes change or what should reconstruct it did (its FidelityFX API library
/// was given up on, or the FSR library setting was switched). Every view that has FSR's state
/// and should not keep it ([`keeps_fsr_state`]) loses it, whatever else it has: a Bilinear view
/// has a prepared view but no `TemporalJitter` (#2414), so FSR's views are matched with the
/// jitter optional.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn prepare(
    mut commands: Commands,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut renderer: ResMut<FsrRenderer>,
    upscale: Res<UpscaleRenderer>,
    frame_generation: Option<Res<crate::frame_gen::FrameGenRequest>>,
    mut views: Query<(Entity, &PreparedUpscale, Option<&TemporalJitter>, &mut CameraMainTextureUsages, Option<&PreparedFsr>)>,
    with_state: Query<(Entity, Option<&PreparedUpscale>), With<PreparedFsr>>,
) {
    for (entity, prepared) in &with_state {
        if !keeps_fsr_state(prepared.map(|p| p.camera.settings)) {
            // Once per switch away from FSR (the smoke scenario bilinear-switch waits for it).
            info!("FSR 3.1: its own state let go, the view is not FSR's now");
            commands.entity(entity).remove::<PreparedFsr>();
        }
    }
    for (entity, prepared, jitter, mut usages, old) in &mut views {
        let UpscaleSettings::Fsr(settings) = prepared.camera.settings else { continue };
        debug_assert!(prep::allocates_fsr_state(prepared.camera.settings.upscaler()));
        // Made with the prepared view (`prep::prepare`).
        let Some(common) = &upscale.0 else { continue };
        renderer.auxiliary.get_or_insert_with(|| AuxiliaryPipelines::new(device.wgpu_device(), common));
        usages.0 |= wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC;
        // FSR's jitter (AMD's sign): the common preparation gave Bevy its opposite. An FSR
        // camera always has the component (`FsrUpscaling` requires it).
        let offset = jitter.map_or(Vec2::ZERO, |jitter| -jitter.offset);
        let (render_size, output_size) = (prepared.render_size, prepared.output_size);
        match old {
            Some(old) if old.render_size == render_size && old.output_size == output_size && old.through_api() == crate::ffx_api::library().is_some() => {
                queue.write_buffer(&old.mask_jitter, 0, &mask_jitter_bytes(offset));
            }
            _ => {
                let fsr = PreparedFsr::new(&mut renderer.port, &device, &queue, render_size, output_size, offset);
                let through = if fsr.through_api() { "FSR through the FidelityFX API" } else { "FSR 3.1.4" };
                info!("{through}: {:?}, {}x{} -> {}x{}, frame generation {}", settings.quality, render_size.x, render_size.y, output_size.x, output_size.y, on_off(frame_generation.as_ref().is_some_and(|f| f.on())));
                commands.entity(entity).insert(fsr);
            }
        }
    }
}

/// The masks' uniform (`fsr_reactive.wgsl` `jitter`): FSR's jitter, then two unused floats.
fn mask_jitter_bytes(jitter: Vec2) -> Vec<u8> {
    [jitter.x, jitter.y, 0.0, 0.0].into_iter().flat_map(f32::to_ne_bytes).collect()
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// FSR's own textures, in [`PreparedFsr`]'s order: the opaque copy (display-sized like the
/// depth attachment it is drawn with, [`reference_sky_and_terrain_layers`]; only the render
/// rectangle is used), the masks, the two presences, the light change, the history luminance
/// and the dilation targets. Its buffers are the mask jitter and the reconstructed depth.
/// The dilation targets and the reconstructed depth are the port's alone: through a
/// FidelityFX API library they are not made, and [`api_depth_texture`] is.
pub(crate) fn fsr_textures(render_size: UVec2, output_size: UVec2) -> [Allocation; 9] {
    let sampled = wgpu::TextureUsages::TEXTURE_BINDING;
    let storage = sampled | wgpu::TextureUsages::STORAGE_BINDING;
    let attachment = sampled | wgpu::TextureUsages::RENDER_ATTACHMENT;
    let texture = |label, size, format, usage| Allocation { label, size, format, usage };
    [
        texture("fsr_opaque", output_size, wgpu::TextureFormat::Rgba16Float, attachment | wgpu::TextureUsages::COPY_DST),
        texture("fsr_reactive", render_size, wgpu::TextureFormat::R8Unorm, attachment),
        texture("fsr_transparency_and_composition", render_size, wgpu::TextureFormat::R8Unorm, attachment),
        texture("fsr_transparency_presence", render_size, wgpu::TextureFormat::R8Unorm, attachment | wgpu::TextureUsages::COPY_DST),
        texture("fsr_transparency_presence", render_size, wgpu::TextureFormat::R8Unorm, attachment | wgpu::TextureUsages::COPY_DST),
        texture("fsr_light_change", render_size, wgpu::TextureFormat::R16Float, attachment),
        texture("fsr_history_luma", render_size, wgpu::TextureFormat::R16Float, attachment | wgpu::TextureUsages::COPY_DST),
        texture("fsr_dilated_depth", render_size, wgpu::TextureFormat::R32Float, storage),
        texture("fsr_dilated_motion", render_size, wgpu::TextureFormat::Rg16Float, storage),
    ]
}

/// The colour copy of the corrected depth a FidelityFX API library is given
/// (`prep::Pipelines::depth_as_color`).
fn api_depth_texture(render_size: UVec2) -> Allocation {
    let usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT;
    Allocation { label: "fsr_api_depth", size: render_size, format: wgpu::TextureFormat::R32Float, usage }
}

impl PreparedFsr {
    /// The opaque copy the masks compare the finished frame with (display-sized; the render
    /// rectangle is used): the ambient occlusion before FSR darkens it as it does the frame.
    pub(crate) fn opaque(&self) -> &wgpu::Texture {
        &self.opaque
    }

    /// Whether a FidelityFX API library upscales the view, not the port.
    fn through_api(&self) -> bool {
        matches!(self.upscaler, Upscaler::Api { .. })
    }

    /// `port` is the port's context, made here when this is the first view to use it.
    fn new(port: &mut Option<FsrContext>, device: &RenderDevice, queue: &RenderQueue, render_size: UVec2, output_size: UVec2, jitter: Vec2) -> Self {
        let device = device.wgpu_device();
        let [opaque, reactive, composition, presence_0, presence_1, light_change, history_luma, dilated_depth, dilated_motion] = fsr_textures(render_size, output_size);
        let [opaque, reactive, composition, presence_0, presence_1, light_change, history_luma] =
            [opaque, reactive, composition, presence_0, presence_1, light_change, history_luma].map(|a| a.create(device));
        // A FidelityFX API library in the game's folder upscales when it gives a context;
        // otherwise, and with none there, the port.
        let api = crate::ffx_api::library().and_then(|library| match crate::ffx_api::Upscaler::new(library, device, render_size.to_array(), output_size.to_array()) {
            Ok(context) => Some(Upscaler::Api {
                context: Mutex::new(context),
                depth: api_depth_texture(render_size).create(device),
                failures: AtomicU32::new(0),
                outputs: Mutex::new(Vec::new()),
            }),
            Err(why) => {
                crate::ffx_api::give_up(&why);
                None
            }
        });
        let upscaler = api.unwrap_or_else(|| Upscaler::Port {
            view: Box::new(Mutex::new(port.get_or_insert_with(|| port_context(device)).create_view(queue, render_size.to_array(), output_size.to_array()))),
            dilated_depth: dilated_depth.create(device),
            dilated_motion: dilated_motion.create(device),
            reconstructed_depth: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("fsr_reconstructed_depth"), size: render_size.x as u64 * render_size.y as u64 * 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
            }),
        });
        Self {
            upscaler,
            render_size, output_size,
            dispatch_logged: AtomicBool::new(false),
            reference_logged: AtomicBool::new(false),
            sky_logged: AtomicBool::new(false),
            opaque, reactive, composition,
            presence: [presence_0, presence_1],
            presence_written: AtomicUsize::new(0),
            light_change, history_luma,
            mask_jitter: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("fsr_mask_jitter"), contents: &mask_jitter_bytes(jitter), usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
        }
    }
}

fn capture_opaque(view: ViewQuery<(&ViewTarget, &PreparedFsr)>, mut ctx: RenderContext) {
    let (target, fsr) = view.into_inner();
    ctx.command_encoder().copy_texture_to_texture(
        target.main_texture().as_image_copy(), fsr.opaque.as_image_copy(),
        wgpu::Extent3d { width: fsr.render_size.x, height: fsr.render_size.y, depth_or_array_layers: 1 },
    );
}

/// The first transparent draws go into the reactive mask's opaque reference: the same
/// draws, with the same pipelines and depth buffer, over the opaque copy, so FSR does not
/// count them as translucent.
///
/// The terrain's texture layers (terrain.rs) are blended copies of opaque ground, drawn
/// first in the transparent phase after the sky (`nif_material::sort_first`). They move with
/// the ground's depth and motion. Without them every pixel of ground a layer textures
/// counted as translucent, FSR discarded most of its history there, and the edges of
/// grass, bushes and trees standing on that ground were left nearly unaccumulated: they
/// shimmered under the jitter [observed 2026-11-05, docs/design/fsr-upscaling.md
/// "The reactive mask"].
///
/// The sky's blended and additive parts (sky.rs `sorts_with_sky`: the cloud layers, the
/// night's starfield layer, the stars and the moon) are drawn over the opaque sky dome,
/// which is in the depth and motion prepass: their pixels carry the dome's motion, which a
/// turn of the camera moves them by as it does the dome (both are around the camera, far
/// beyond the world). Left reactive, they kept FSR from accumulating the clouds and stars
/// and everything standing against them [observed 2026-11-06, docs/design/fsr-upscaling.md
/// "The reactive mask"]. Every other blended mesh stays reactive.
fn reference_sky_and_terrain_layers(
    world: &World,
    view: ViewQuery<(&ExtractedCamera, &ExtractedView, &ViewDepthStencilTexture, &PreparedFsr, Option<&MainPassResolutionOverride>)>,
    phases: Res<ViewSortedRenderPhases<Transparent3d>>,
    mut ctx: RenderContext,
) {
    let view_entity = view.entity();
    let (camera, extracted, depth, fsr, resolution) = view.into_inner();
    let Some(phase) = phases.get(&extracted.retained_view_entity) else { return };
    let sky = sky_range(phase.items.values().map(|item| item.distance));
    let layers = sort_first_range(phase.items.values().map(|item| item.distance));
    if sky.is_empty() && layers.is_empty() {
        return;
    }
    let target = fsr.opaque.create_view(&default());
    let mut pass = ctx.begin_tracked_render_pass(wgpu::RenderPassDescriptor {
        label: Some("fsr_reactive_reference_sky_and_terrain_layers"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &target, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
        // Blended parts test against the depth buffer but write none; store keeps it.
        depth_stencil_attachment: Some(depth.get_attachment(wgpu::StoreOp::Store)),
        timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
    });
    if let Some(viewport) = Viewport::from_viewport_and_override(camera.viewport.as_ref(), resolution) {
        pass.set_camera_viewport(&viewport);
    }
    // In the phase's order: the sky's band, then the terrain layers' band right after it.
    for (range, logged, one, all) in [(sky, &fsr.sky_logged, "sky", "sky parts"), (layers, &fsr.reference_logged, "terrain layer", "terrain layers")] {
        if range.is_empty() {
            continue;
        }
        let draws = range.len();
        if let Err(error) = phase.render_range(&mut pass, world, view_entity, range) {
            error!("FSR reactive reference: {all} not drawn: {error:?}");
        } else if !logged.swap(true, Ordering::Relaxed) {
            info!("FSR reactive reference: {draws} {one} draws over the opaque copy");
        }
    }
}

/// The indices of the sorted transparent items whose sort keys `in_band` takes. The phase
/// is sorted by these keys, ascending, so a band is one run.
fn band_range(sort_keys: impl Iterator<Item = f32>, in_band: impl Fn(f32) -> bool) -> Range<usize> {
    let mut range: Option<Range<usize>> = None;
    for (index, key) in sort_keys.enumerate() {
        if in_band(key) {
            range.get_or_insert(index..index).end = index + 1;
        } else if range.is_some() {
            break;
        }
    }
    range.unwrap_or(0..0)
}

/// The `sort_first` band (terrain layers).
fn sort_first_range(sort_keys: impl Iterator<Item = f32>) -> Range<usize> {
    band_range(sort_keys, nvf_assets::nif_material::sorts_first)
}

/// The sky's band (sky.rs `sorts_with_sky`), the first transparent items.
fn sky_range(sort_keys: impl Iterator<Item = f32>) -> Range<usize> {
    band_range(sort_keys, crate::sky::sorts_with_sky)
}

/// `NVF_FSR_DEBUG_VIEW=1`: FSR draws AMD's debug view over its output (a 3x3 grid: dilated
/// motion, protected areas and depth on top; disocclusion, shading change with the reactive
/// mask, and the transparency/composition mask with FSR's motion divergence (its protection
/// takedown, the `REACTIVE` channel of `ffx_fsr3upscaler_prepare_reactivity.h`) in the bottom
/// row, in green). For checking where the masks act; a diagnostic, not a setting. With the debug
/// view the light term is off: the output holds the grid, so the history luminance is cleared
/// each frame instead of read from it (as on a reset), and the masks shown are those of play
/// without the light term.
static DEBUG_VIEW: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| std::env::var("NVF_FSR_DEBUG_VIEW").is_ok_and(|v| v.trim() == "1"));

/// Reconstruction is before every effect expecting the full display-sized image. The common
/// passes (the corrected depth, the fallback, the alpha copy, the expanded depth) are
/// `upscaler::prep::Pipelines`', called here in the order FSR always ran them.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn fsr_pass(
    view: ViewQuery<(&ViewTarget, &PreparedUpscale, &PreparedFsr, Option<&ViewPrepassTextures>, &TemporalJitter)>,
    renderer: Res<FsrRenderer>, upscale: Res<UpscaleRenderer>, device: Res<RenderDevice>, frame_generation: Option<Res<crate::frame_gen::FrameGenRequest>>, mut ctx: RenderContext,
) {
    let (target, prepared, fsr, prepass, jitter) = view.into_inner();
    let (Some(auxiliary), Some(common)) = (&renderer.auxiliary, &upscale.0) else { return };
    let UpscaleSettings::Fsr(settings) = prepared.camera.settings else { return };
    let recording = crate::frame_pace::recording();
    let inputs = prepass.and_then(|prepass| prepass.depth.as_ref().zip(prepass.motion_vectors.as_ref()));
    let Some((depth, motion)) = inputs else {
        prepared.history_valid.store(false, Ordering::Relaxed);
        common.fallback(device.wgpu_device(), ctx.command_encoder(), prepared, target);
        if let Some(depth) = prepass.and_then(|prepass| prepass.depth.as_ref()) {
            common.expand_depth(device.wgpu_device(), ctx.command_encoder(), prepared, &depth.texture.texture);
        }
        return;
    };
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_fsr_3_1");
    let device = device.wgpu_device();
    let encoder = ctx.command_encoder();
    let reset = prepared.reset || !prepared.history_valid.load(Ordering::Relaxed);
    // Sub-spans (`nvf_fsr_3_1/...`, logged with `--log-frame-times`): NVForever's own passes
    // around AMD's, to tell their cost apart.
    let inputs_span = diagnostics.time_span(encoder, "inputs");
    common.corrected_depth(device, encoder, prepared, &depth.texture.texture);
    auxiliary.prepare_inputs(device, encoder, common, fsr, target.main_texture(), &motion.texture.texture, reset);
    inputs_span.end(encoder);
    let source = target.main_texture().deref().clone();
    let destination = target.main_texture_other().deref().clone();
    let jitter_offset = (-jitter.offset).to_array();
    let motion_vector_scale = (-prepared.render_size.as_vec2()).to_array();
    let sharpness = settings.sharpness.clamp(0.0, 1.0);
    // NVForever's world coordinates are the game's units, about 70 a metre [hypothesis, the
    // scale fnv-phys uses (`PhysParams::units_per_metre`): the game's gravity of 684-686
    // units/s² (physics.md §6.3, observed) is 9.81 m/s² at 70 units a metre].
    let view_space_to_meters_factor = 1.0 / 70.0;
    let dispatched: Result<(), String> = match &fsr.upscaler {
        Upscaler::Port { view, dilated_depth, dilated_motion, reconstructed_depth } => {
            let info = FsrDispatchInfo {
                color: source, depth: prepared.depth.clone(), motion_vectors: motion.texture.texture.deref().clone(),
                exposure: None, reactive_mask: Some(fsr.reactive.clone()), transparency_and_composition: Some(fsr.composition.clone()),
                dilated_depth: dilated_depth.clone(), dilated_motion_vectors: dilated_motion.clone(),
                reconstructed_previous_depth: reconstructed_depth.clone(), output: destination,
                jitter_offset, motion_vector_scale,
                render_size: prepared.render_size.to_array(), upscale_size: prepared.output_size.to_array(),
                enable_sharpening: sharpness > 0.0, sharpness,
                frame_time_delta: prepared.camera.delta_ms, pre_exposure: 1.0,
                reset_history: reset,
                camera_near: f32::MAX, camera_far: prepared.camera.near, camera_fov_y: prepared.camera.fov,
                view_space_to_meters_factor,
                flags: if *DEBUG_VIEW { FsrDispatchFlags::DRAW_DEBUG_VIEW } else { FsrDispatchFlags::empty() },
            };
            let upscale_span = diagnostics.time_span(encoder, "upscale");
            let dispatched = match &renderer.port {
                Some(context) => context.dispatch(&mut view.lock().unwrap(), encoder, &info).map_err(|error| error.to_string()),
                None => Err("the FSR context is missing".into()),
            };
            upscale_span.end(encoder);
            dispatched
        }
        Upscaler::Api { context, depth: api_depth, failures, outputs } => {
            common.depth_as_color(device, encoder, prepared, api_depth);
            if Upscaler::first_output(outputs, &destination) {
                encoder.clear_texture(&destination, &wgpu::ImageSubresourceRange::default());
            }
            let frame = crate::ffx_api::Frame {
                color: &source, depth: api_depth, motion_vectors: &motion.texture.texture,
                reactive: &fsr.reactive, transparency_and_composition: &fsr.composition, output: &destination,
                jitter_offset, motion_vector_scale,
                render_size: prepared.render_size.to_array(), upscale_size: prepared.output_size.to_array(),
                sharpness, frame_time_delta: prepared.camera.delta_ms, reset,
                camera_near: f32::MAX, camera_far: prepared.camera.near, camera_fov_y: prepared.camera.fov,
                view_space_to_meters_factor, debug_view: *DEBUG_VIEW,
            };
            // The last thing in these commands: the library's own, which wgpu does not
            // track, find every image in the state the API is told (ffx_api.rs).
            encoder.transition_resources(std::iter::empty(), frame.transitions().into_iter());
            let upscale_span = diagnostics.time_span(encoder, "upscale");
            // Its commands go between the ones recorded so far and the ones that follow.
            let dispatched = context.lock().unwrap().dispatch(&frame).map(|commands| ctx.add_command_buffer(commands));
            upscale_span.end(ctx.command_encoder());
            match &dispatched {
                Ok(()) => failures.store(0, Ordering::Relaxed),
                Err(why) if failures.fetch_add(1, Ordering::Relaxed) + 1 >= API_FAILURES => crate::ffx_api::give_up(why),
                Err(_) => {}
            }
            dispatched
        }
    };
    let api = fsr.through_api();
    let encoder = ctx.command_encoder();
    match dispatched {
        Ok(()) => {
            let after_span = diagnostics.time_span(encoder, "alpha_and_history");
            // Only while a shape marks its pixels in alpha (the Pip-Boy's screen on its
            // model): otherwise FSR's alpha of 1 is what the post passes expect, and the
            // display-sized pass is saved. What another upscaler behind the FidelityFX API
            // leaves in alpha is not known, so there the scene's own always comes back.
            if prepared.camera.screen_mark || api {
                common.restore_alpha(device, encoder, prepared, target);
            }
            target.post_process_write();
            if *DEBUG_VIEW {
                // The output is AMD's grid, not what FSR's history shows: a cleared history
                // makes the next frame's light term skip (`DEBUG_VIEW`).
                encoder.clear_texture(&fsr.history_luma, &wgpu::ImageSubresourceRange::default());
            } else {
                auxiliary.keep_history_luma(device, encoder, common, prepared, fsr, target);
            }
            after_span.end(encoder);
            prepared.history_valid.store(true, Ordering::Relaxed);
            if !fsr.dispatch_logged.swap(true, Ordering::Relaxed) {
                let through = match crate::ffx_api::library().filter(|_| api) {
                    Some(library) => format!("FidelityFX API ({})", library.path),
                    None => "3.1.4".into(),
                };
                let shaders = if api { "the library's own upscaler" } else { "Vulkan source shaders" };
                info!("FSR dispatch active: {through}, {}x{} -> {}x{}, {shaders}, frame generation {}", prepared.render_size.x, prepared.render_size.y, prepared.output_size.x, prepared.output_size.y, on_off(frame_generation.as_ref().is_some_and(|f| f.on())));
            }
        }
        Err(error) => {
            prepared.history_valid.store(false, Ordering::Relaxed);
            error!("FSR dispatch failed: {error}; presenting spatial fallback");
            common.fallback(device, encoder, prepared, target);
        }
    }
    let depth_span = diagnostics.time_span(encoder, "depth_expand");
    common.expand_depth(device, encoder, prepared, &depth.texture.texture);
    depth_span.end(encoder);
    span.end(ctx.command_encoder());
    crate::frame_pace::recorded(crate::frame_pace::Recorded::Fsr, recording, true);
}

/// FSR's own passes: the masks (`fsr_reactive.wgsl`) and its output's luminance for the next
/// frame's light term (`fsr_fallback.wgsl` `luma`, on the common fallback layout).
struct AuxiliaryPipelines {
    presence_layout: wgpu::BindGroupLayout,
    reactive_layout: wgpu::BindGroupLayout,
    presence: wgpu::RenderPipeline,
    reactive: wgpu::RenderPipeline,
    history_luma: wgpu::RenderPipeline,
}

impl AuxiliaryPipelines {
    fn new(device: &wgpu::Device, common: &prep::Pipelines) -> Self {
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding, visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None,
        };
        // fsr_reactive.wgsl's first pass: the opaque copy and the finished frame, motion, FSR's
        // last output luminance, a linear sampler and the jitter; its second: motion, last
        // presence, the sampler, and the first pass's presence and light change.
        let sampler_entry = wgpu::BindGroupLayoutEntry { binding: 5, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None };
        let presence_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("fsr_presence_layout"), entries: &[
            texture_entry(0), texture_entry(1), texture_entry(2), texture_entry(4), sampler_entry,
            wgpu::BindGroupLayoutEntry { binding: 6, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
        ] });
        let reactive_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("fsr_reactive_layout"), entries: &[
            texture_entry(2), texture_entry(3), sampler_entry, texture_entry(7), texture_entry(8),
        ] });
        let mask = (wgpu::TextureFormat::R8Unorm, wgpu::ColorWrites::ALL);
        Self {
            presence: prep::fullscreen_pipeline(device, "fsr_presence", include_str!("fsr_reactive.wgsl"), &presence_layout, "presence_pass", &[mask, (wgpu::TextureFormat::R16Float, wgpu::ColorWrites::ALL)]),
            reactive: prep::fullscreen_pipeline(device, "fsr_reactive", include_str!("fsr_reactive.wgsl"), &reactive_layout, "fragment", &[mask, mask]),
            history_luma: prep::fullscreen_pipeline(device, "fsr_history_luma", include_str!("fsr_fallback.wgsl"), &common.fallback_layout, "luma", &[(wgpu::TextureFormat::R16Float, wgpu::ColorWrites::ALL)]),
            presence_layout, reactive_layout,
        }
    }

    /// FSR's two masks (its depth is the common corrected depth, drawn just before). On a
    /// history reset last frame's presence and output luminance are cleared: the masks then
    /// hold this frame's presence alone.
    #[allow(clippy::too_many_arguments)]
    fn prepare_inputs(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, common: &prep::Pipelines, fsr: &PreparedFsr, color: &wgpu::Texture, motion: &wgpu::Texture, reset: bool) {
        let written = 1 - fsr.presence_written.load(Ordering::Relaxed);
        let previous = &fsr.presence[1 - written];
        if reset {
            encoder.clear_texture(previous, &wgpu::ImageSubresourceRange::default());
            encoder.clear_texture(&fsr.history_luma, &wgpu::ImageSubresourceRange::default());
        }
        let view = |texture: &wgpu::Texture| texture.create_view(&default());
        let (opaque, color, motion, previous, history) = (view(&fsr.opaque), view(color), view(motion), view(previous), view(&fsr.history_luma));
        let (presence, change) = (view(&fsr.presence[written]), view(&fsr.light_change));
        let attachment = |view| Some(wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } });
        let draw = |encoder: &mut wgpu::CommandEncoder, label, attachments: &[Option<wgpu::RenderPassColorAttachment>], pipeline, group| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label), color_attachments: attachments,
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        };
        // This frame's presence and light change, then the masks from them.
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("fsr_presence_group"), layout: &self.presence_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&opaque) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&color) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&motion) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&history) },
            wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(&common.sampler) },
            wgpu::BindGroupEntry { binding: 6, resource: fsr.mask_jitter.as_entire_binding() },
        ] });
        draw(encoder, "fsr_presence", &[attachment(&presence), attachment(&change)], &self.presence, &group);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("fsr_reactive_group"), layout: &self.reactive_layout, entries: &[
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&motion) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&previous) },
            wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(&common.sampler) },
            wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&presence) },
            wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(&change) },
        ] });
        draw(encoder, "fsr_reactive_mask", &[attachment(&view(&fsr.reactive)), attachment(&view(&fsr.composition))], &self.reactive, &group);
        fsr.presence_written.store(written, Ordering::Relaxed);
    }

    /// The luminance of FSR's output (the main texture, after the flip), for the next
    /// frame's masks (`fsr_reactive.wgsl` `light_change`).
    fn keep_history_luma(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, common: &prep::Pipelines, prepared: &PreparedUpscale, fsr: &PreparedFsr, target: &ViewTarget) {
        let group = common.fallback_group(device, "fsr_history_luma_group", prepared, target.main_texture_view());
        let history = fsr.history_luma.create_view(&default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fsr_history_luma"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &history, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        });
        pass.set_pipeline(&self.history_luma);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The camera history and its cuts moved to the common preparation unchanged.
    use crate::upscaler::prep::UpscaleCamera as FsrCamera;

    /// FSR's state goes with FSR alone: a live switch to Bilinear (a prepared view, no jitter)
    /// or DLSS, or to native (no prepared view), takes it away (#2414's review).
    #[test]
    fn fsr_state_is_kept_only_while_fsr_draws_the_view() {
        use crate::upscaler::UpscaleQuality::*;
        assert!(keeps_fsr_state(Some(UpscaleSettings::Fsr(FsrUpscaling::default()))));
        assert!(!keeps_fsr_state(Some(UpscaleSettings::Bilinear(Quality))), "FSR -> Bilinear");
        assert!(!keeps_fsr_state(Some(UpscaleSettings::Dlss(Quality))), "FSR -> DLSS");
        assert!(!keeps_fsr_state(None), "FSR -> off, or an unusable view");
    }

    #[test]
    fn quality_modes_have_standard_ratios() {
        let output = UVec2::new(3840, 2160);
        assert_eq!(FsrQuality::NativeAa.render_size(output), output);
        assert_eq!(FsrQuality::Quality.render_size(output), UVec2::new(2560, 1440));
        assert_eq!(FsrQuality::Balanced.render_size(output), UVec2::new(2258, 1270));
        assert_eq!(FsrQuality::Performance.render_size(output), UVec2::new(1920, 1080));
        assert_eq!(FsrQuality::UltraPerformance.render_size(output), UVec2::new(1280, 720));
    }

    #[test]
    fn history_survives_motion_and_zoom_but_resets_on_cuts_and_lens_jumps() {
        let before = FsrCamera { settings: default(), translation: Vec3::ZERO, rotation: Quat::IDENTITY, near: 5.0, fov: 1.0, frame: 10, delta_ms: 16.0, loading_generation: 0, play_state: None, screen_mark: false };
        let after = FsrCamera { frame: 11, translation: Vec3::X * 20.0, ..before };
        assert!(!after.cut_from(&before));
        assert!(FsrCamera { translation: Vec3::X * 401.0, ..after }.cut_from(&before));
        assert!(FsrCamera { rotation: Quat::from_rotation_y(1.0), ..after }.cut_from(&before));
        assert!(!FsrCamera { fov: 0.95, ..after }.cut_from(&before), "a zoom step keeps the history");
        assert!(FsrCamera { fov: 0.3, ..after }.cut_from(&before), "a lens jump resets it");
        assert!(FsrCamera { frame: 14, ..after }.cut_from(&before));
        assert!(FsrCamera { delta_ms: 500.0, ..after }.cut_from(&before));
        assert!(FsrCamera { loading_generation: 1, ..after }.cut_from(&before));
        assert!(FsrCamera { play_state: Some(nvf_play::PlayState::Loading), ..after }.cut_from(&before));
    }

    #[test]
    fn the_pip_boy_and_menus_keep_the_history_and_dialogue_and_loading_reset_it() {
        use nvf_play::PlayState::{Dialogue, Loading, Menu, Playing};
        let playing = FsrCamera { settings: default(), translation: Vec3::ZERO, rotation: Quat::IDENTITY, near: 5.0, fov: 1.0, frame: 10, delta_ms: 16.0, loading_generation: 0, play_state: Some(Playing), screen_mark: false };
        let next = |state| FsrCamera { frame: 11, play_state: Some(state), ..playing };
        assert!(!next(Menu).cut_from(&playing), "the Pip-Boy opens");
        assert!(!FsrCamera { play_state: Some(Playing), ..next(Menu) }.cut_from(&FsrCamera { play_state: Some(Menu), ..playing }), "and closes");
        assert!(next(Dialogue).cut_from(&playing));
        assert!(FsrCamera { play_state: Some(Playing), ..next(Dialogue) }.cut_from(&FsrCamera { play_state: Some(Dialogue), ..playing }));
        assert!(next(Loading).cut_from(&playing));
        assert!(!next(Playing).cut_from(&playing));
    }

    #[test]
    fn fsr_depth_unsqueezes_with_the_first_person_share() {
        // fsr_depth.wgsl is plain WGSL without imports: its copy of the share has to be the
        // one the NIF shaders squeeze first-person depth into.
        let line = format!("const FIRST_PERSON_DEPTH_SHARE: f32 = {:?};", nvf_assets::nif_material::FIRST_PERSON_DEPTH_SHARE);
        assert!(include_str!("fsr_depth.wgsl").contains(&line), "fsr_depth.wgsl must declare `{line}`");
    }

    #[test]
    fn an_aim_zoom_keeps_the_history_and_a_lens_jump_resets_it() {
        use nvf_play::aim_zoom::{zoomed_fov, ZOOM_SECONDS};
        use nvf_play::free_cam::vertical_fov;
        let base = vertical_fov(75.0);
        let start = FsrCamera { settings: default(), translation: Vec3::ZERO, rotation: Quat::IDENTITY, near: 5.0, fov: base, frame: 0, delta_ms: 16.0, loading_generation: 0, play_state: None, screen_mark: false };
        // The 9mm pistol's iron sights (65°), a closer sight and a 20° scope, zoomed in and
        // out again a frame at a time as `aim_zoom::zoom` steps it.
        for sight_deg in [65.0, 55.0, 20.0] {
            let sight = vertical_fov(sight_deg);
            for fps in [15.0, 30.0, 60.0, 144.0] {
                let step = (1.0f32 / fps).min(0.25) / ZOOM_SECONDS;
                let mut blends = Vec::new();
                let mut blend = 0.0f32;
                while blend < 1.0 {
                    blend = (blend + step).min(1.0);
                    blends.push(blend);
                }
                while blend > 0.0 {
                    blend = (blend - step).max(0.0);
                    blends.push(blend);
                }
                let mut camera = FsrCamera { delta_ms: 1000.0 / fps, ..start };
                for blend in blends {
                    let next = FsrCamera { frame: camera.frame + 1, fov: zoomed_fov(base, sight, blend), ..camera };
                    assert!(!next.cut_from(&camera), "{sight_deg}° sight at {fps} fps: {:.2}° -> {:.2}° reset", camera.fov.to_degrees(), next.fov.to_degrees());
                    camera = next;
                }
            }
        }
        // A jump to the 20° scope's angle (the console's `fov`) and back resets.
        let jumped = FsrCamera { frame: 1, fov: vertical_fov(20.0), ..start };
        assert!(jumped.cut_from(&start));
        assert!(FsrCamera { frame: 2, fov: base, ..jumped }.cut_from(&jumped));
    }

    #[test]
    fn reactive_reference_takes_only_the_terrain_layer_band() {
        use nvf_assets::nif_material::sort_first;
        // Sorted keys as Bevy draws them: sky parts, terrain layers near and far, then
        // ordinary blended meshes (particles, glass, decals) near and far.
        let sky = [-1.0e6 - 30.0, -1.0e6 - 20.0, -1.0e6 + 1.0];
        let layers = [sort_first(2.0) - 9_000.0, sort_first(4.0) - 300.0, sort_first(2.0) + 2_500.0];
        let others = [-12_000.0, -40.0, 3.0];
        let keys: Vec<f32> = sky.iter().chain(&layers).chain(&others).copied().collect();
        assert_eq!(sort_first_range(keys.iter().copied()), 3..6);
        assert_eq!(sort_first_range(sky.iter().chain(&others).copied()), 0..0, "no layers in view");
        assert_eq!(sort_first_range(layers.iter().copied()), 0..3);
        assert_eq!(sort_first_range(std::iter::empty()), 0..0);
    }

    #[test]
    fn reactive_reference_takes_the_sky_band_then_the_terrain_layers() {
        use nvf_assets::nif_material::sort_first;
        // Sorted keys as Bevy draws them: the stars (-30) at the world's origin up to
        // 200,000 units ahead, the moon (-20) 140,000 units out, the cloud layers (0-7) at
        // the camera, the stars again with the origin 200,000 units behind; then terrain
        // layers, then ordinary blended meshes.
        let sky = [-1.0e6 - 30.0 - 200_000.0, -1.0e6 - 20.0 - 140_000.0, -1.0e6, -1.0e6 + 7.0, -1.0e6 - 30.0 + 200_000.0];
        let layers = [sort_first(2.0) - 9_000.0, sort_first(4.0) - 300.0, sort_first(2.0) + 2_500.0];
        let others = [-60_000.0, -40.0, 3.0];
        let keys: Vec<f32> = sky.iter().chain(&layers).chain(&others).copied().collect();
        let (sky_band, layer_band) = (sky_range(keys.iter().copied()), sort_first_range(keys.iter().copied()));
        assert_eq!(sky_band, 0..5);
        assert_eq!(layer_band, 5..8);
        assert_eq!(sky_band.end, layer_band.start, "the two bands are drawn as one run, in order");
        assert_eq!(sky_range(layers.iter().chain(&others).copied()), 0..0, "no sky parts in view (clear sky by day)");
        assert_eq!(sky_range(others.iter().copied()), 0..0);
    }

    #[test]
    fn fsr_allocates_its_textures_as_before() {
        let (render, output) = (UVec2::new(853, 480), UVec2::new(1280, 720));
        let sampled = wgpu::TextureUsages::TEXTURE_BINDING;
        let attachment = sampled | wgpu::TextureUsages::RENDER_ATTACHMENT;
        let storage = sampled | wgpu::TextureUsages::STORAGE_BINDING;
        let copy_dst = wgpu::TextureUsages::COPY_DST;
        use wgpu::TextureFormat::{R16Float, R32Float, R8Unorm, Rg16Float, Rgba16Float};
        let expected = [
            ("fsr_opaque", output, Rgba16Float, attachment | copy_dst),
            ("fsr_reactive", render, R8Unorm, attachment),
            ("fsr_transparency_and_composition", render, R8Unorm, attachment),
            ("fsr_transparency_presence", render, R8Unorm, attachment | copy_dst),
            ("fsr_transparency_presence", render, R8Unorm, attachment | copy_dst),
            ("fsr_light_change", render, R16Float, attachment),
            ("fsr_history_luma", render, R16Float, attachment | copy_dst),
            ("fsr_dilated_depth", render, R32Float, storage),
            ("fsr_dilated_motion", render, Rg16Float, storage),
        ];
        let actual: Vec<_> = fsr_textures(render, output).iter().map(|a| (a.label, a.size, a.format, a.usage)).collect();
        assert_eq!(actual, expected);
        // The depths are the common prepared view's, not FSR's own.
        let common: Vec<&str> = prep::common_textures(render, output).iter().map(|a| a.label).collect();
        assert_eq!(common, ["fsr_linear_camera_depth", "fsr_full_resolution_depth"]);
        assert!(fsr_textures(render, output).iter().all(|a| !common.contains(&a.label)));
    }

    #[test]
    fn fsr_policy_keeps_its_ratio_phases_and_bias() {
        let limits = wgpu::Limits { max_storage_textures_per_shader_stage: 8, ..default() };
        let output = UVec2::new(1920, 1080);
        for quality in FsrQuality::ALL {
            let render = quality.render_size(output);
            let policy = policy(quality, output, &limits).expect("supported");
            assert_eq!(policy.render_size, render);
            assert_eq!(policy.jitter_phases, wgpu_ffx::get_jitter_phase_count(render.x as i32, output.x as i32));
            assert_eq!(policy.mip_bias, (render.x as f32 / output.x as f32).log2() + *MIP_BIAS_OFFSET);
        }
        assert_eq!(policy(FsrQuality::UltraPerformance, UVec2::splat(96), &limits), None, "renders natively below 64 texels");
    }

    #[test]
    fn unsupported_limits_and_tiny_views_do_not_create_fsr_contexts() {
        assert!(!supports_limits(&wgpu::Limits::default()), "WebGPU's four-storage-texture baseline is insufficient");
        let limits = wgpu::Limits { max_storage_textures_per_shader_stage: 8, ..default() };
        assert!(supports_limits(&limits));
        assert!(!supports_limits(&wgpu::Limits { max_storage_textures_per_shader_stage: 7, ..limits.clone() }));
        assert!(!supports_limits(&wgpu::Limits { max_compute_workgroup_size_x: 128, ..limits.clone() }));
        assert!(!supports_limits(&wgpu::Limits { max_compute_workgroup_storage_size: 4096, ..limits.clone() }));
        assert!(supports_size(&limits, UVec2::new(853, 480), UVec2::new(1280, 720)));
        assert!(!supports_size(&limits, UVec2::splat(32), UVec2::splat(96)));
        assert!(!supports_size(&limits, UVec2::splat(16384), UVec2::splat(16384)));
    }
}
