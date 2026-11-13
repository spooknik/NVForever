//! The view preparation every upscaler shares (#2198, docs/design/dlss-upscaling.md
//! "The prepared-view seam"). Lifted out of `fsr.rs` unchanged; FSR uses it, and DLSS (`dlss::dlss_pass`)
//! and Bilinear (`upscaler::bilinear`, #2414) use it without creating any of FSR's state.
//!
//! Per camera that asks for an upscaler ([`FsrUpscaling`], [`DlssUpscaling`] or
//! [`BilinearUpscaling`]):
//! - [`extract`]: the camera's history metadata ([`UpscaleCamera`]), for cut detection;
//! - [`prepare`] (`RenderSystems::PrepareViews`, before the view targets): the view checked,
//!   the backend's [`Policy`] (render size, jitter phase count, mip bias: FSR's in `fsr.rs`,
//!   DLSS's in `dlss/sizing.rs` from the size DLSS reports, Bilinear's in `bilinear.rs`: FSR's
//!   ratio, no jitter, no bias), the render-size override (`MainPassResolutionOverride`), the
//!   jitter and mip bias applied to the camera (a Bilinear camera has neither), the ambient
//!   occlusion's render size, and the
//!   [`PreparedUpscale`] (the corrected depth, the depth expanded afterwards, the resolution
//!   uniform, whether the upscaler reconstructed this frame);
//! - the passes in [`Pipelines`] that a backend's pass calls: the corrected depth before it,
//!   the spatial fallback when it cannot reconstruct, the alpha restored after it and the depth
//!   expanded to the display size for the screen-space effects.
//!
//! Each backend adds only its own: FSR its context, masks and history (`fsr::PreparedFsr`,
//! made in `fsr::prepare` after [`prepare`]). [design] No trait objects: a backend is an enum
//! case ([`UpscaleSettings`]) and its policy a function ([`policy`]).
//!
//! The GPU labels keep their `fsr_` names: they are what FSR's captures and validation runs
//! have always shown.

// `upscaler.rs` allows dead code for its not yet wired status types; this module is wired.
#![warn(dead_code)]

use std::{borrow::Cow, sync::atomic::AtomicBool};

use bevy::camera::{CameraMainTextureUsages, MainPassResolutionOverride};
use bevy::diagnostic::FrameCount;
use bevy::prelude::*;
use bevy::render::{
    camera::{MipBias, TemporalJitter},
    renderer::RenderDevice,
    sync_world::RenderEntity,
    view::{prepare_view_targets, ExtractedView, ViewTarget},
    Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
};
use wgpu::util::DeviceExt;

use super::{BilinearUpscaling, DlssUpscaling, DrawnUpscale, UpscaleDrawn, UpscaleQuality, Upscaler, UpscalerChoice};
use crate::fsr::FsrUpscaling;

/// The common preparation's systems (graphics.rs adds it beside `fsr::FsrPlugin`, whose
/// `prepare` runs after [`prepare`]).
pub struct PrepPlugin;

impl Plugin for PrepPlugin {
    fn build(&self, app: &mut App) {
        // What the main camera was drawn with, written by [`prepare`] and read by the
        // performance overlay (#2242): one `Arc` in each world.
        let drawn = UpscaleDrawn::default();
        app.insert_resource(drawn.clone());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .insert_resource(drawn)
            .init_resource::<UpscaleRenderer>()
            .init_resource::<crate::dlss::OptimalSizes>()
            .add_systems(ExtractSchedule, extract)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareViews).before(prepare_view_targets))
            .add_systems(
                bevy::core_pipeline::Core3d,
                // Where `fsr_pass` and `dlss_pass` are (#2414): after the occlusion applied
                // before the upscaler, before bloom and the bright pass, and before `fsr_pass`,
                // which every other full-resolution effect (the eye's metering mask, frame
                // generation's passes) follows.
                super::bilinear::bilinear_pass
                    .in_set(bevy::core_pipeline::Core3dSystems::EarlyPostProcess)
                    .after(crate::ssao::ao_before_fsr)
                    .before(crate::fsr::fsr_pass)
                    .before(bevy::post_process::bloom::bloom)
                    .before(crate::post::bright_pass_hdr),
            );
    }
}

/// Which upscaler draws a camera, with its own settings (render world).
#[derive(Clone, Copy, Debug)]
pub(crate) enum UpscaleSettings {
    /// FSR 3.1 with its quality and its own sharpening strength.
    Fsr(FsrUpscaling),
    /// DLSS at a quality (`dlss::dlss_pass`).
    Dlss(UpscaleQuality),
    /// Bilinear at a quality (`bilinear::bilinear_pass`, #2414).
    Bilinear(UpscaleQuality),
}

impl Default for UpscaleSettings {
    fn default() -> Self {
        Self::Fsr(FsrUpscaling::default())
    }
}

impl UpscaleSettings {
    pub(crate) fn choice(self) -> UpscalerChoice {
        match self {
            Self::Fsr(fsr) => UpscalerChoice::Fsr(fsr.quality),
            Self::Dlss(quality) => UpscalerChoice::Dlss(quality),
            Self::Bilinear(quality) => UpscalerChoice::Bilinear(quality),
        }
    }

    pub(crate) fn upscaler(self) -> Upscaler {
        self.choice().upscaler()
    }
}

/// The camera as the upscaler's history sees it (render world, extracted every frame).
#[derive(Component, Clone, Copy)]
pub(crate) struct UpscaleCamera {
    pub(crate) settings: UpscaleSettings,
    pub(crate) translation: Vec3,
    pub(crate) rotation: Quat,
    pub(crate) near: f32,
    pub(crate) fov: f32,
    pub(crate) frame: u32,
    pub(crate) delta_ms: f32,
    pub(crate) loading_generation: u32,
    pub(crate) play_state: Option<nvf_play::PlayState>,
    /// The Pip-Boy's screen is on its model: the one shape that marks its pixels in the
    /// picture's alpha (`imagespace.wesl` `SCREEN_MARK`), which the upscaler's output has to
    /// get back ([`Pipelines::restore_alpha`]).
    pub(crate) screen_mark: bool,
}

/// The most the field of view may magnify or widen the view in one frame (the ratio of
/// tan(fov / 2) to the previous frame's) while the upscaler keeps its history. Bevy computes
/// the motion vectors from this and the previous frame's projections, so a smooth zoom, such
/// as the aim zoom's 0.25 s ease (`nvf_play::aim_zoom`), reprojects like any camera move.
/// Resetting on every changed frame threw the history away for the whole zoom, in and out
/// [observed 2026-11-06: in an aim zoom capture all 18 frames whose field of view changed
/// reset (FSR)]. A jump (the console's `fov`, a cut to another lens) still resets.
/// [engineering choice: 2 keeps the history through the aim zoom of a 20° sight at 15 frames a
/// second and up, fsr.rs `an_aim_zoom_keeps_the_history_and_a_lens_jump_resets_it`.]
const LENS_CUT_RATIO: f32 = 2.0;

impl UpscaleCamera {
    /// A camera cut: the history is reset. A change of upscaler or of its quality is one too
    /// (for FSR, its quality was always one).
    pub(crate) fn cut_from(&self, previous: &Self) -> bool {
        let zoom = (self.fov * 0.5).tan() / (previous.fov * 0.5).tan();
        self.frame != previous.frame.wrapping_add(1)
            || self.translation.distance_squared(previous.translation) >= 400.0 * 400.0
            || self.rotation.angle_between(previous.rotation).abs() > std::f32::consts::FRAC_PI_4
            || !(1.0 / LENS_CUT_RATIO..=LENS_CUT_RATIO).contains(&zoom)
            || self.near != previous.near
            || self.settings.choice() != previous.settings.choice()
            || self.delta_ms > 250.0
            || self.loading_generation != previous.loading_generation
            || play_state_cut(previous.play_state, self.play_state)
    }
}

/// Whether a change of play state is a cut for the upscaler's history: into or out of
/// dialogue (its close-up) or a loading screen. The Pip-Boy and the other menus
/// (`PlayState::Menu`) raise over the view or stop the world without moving the camera, so the
/// history stays: thrown away, the whole view was rebuilt from single jittered frames for about
/// a second every time the Pip-Boy opened or closed [observed 2026-11-08, frame-exact captures
/// of the Pip-Boy's raise at Goodsprings with FSR, docs/design/fsr-upscaling.md "History
/// resets"]. A view the camera stops drawing resets anyway (`frame` skips).
fn play_state_cut(before: Option<nvf_play::PlayState>, now: Option<nvf_play::PlayState>) -> bool {
    use nvf_play::PlayState::{Dialogue, Loading};
    before != now && [before, now].iter().any(|state| matches!(state, Some(Dialogue | Loading)))
}

/// What a backend decides for a view of a given output size: the size the scene is drawn at,
/// the number of jitter phases and the texture mip bias. `None` from [`policy`]: the view
/// renders natively, without jitter or mip bias, and gets no prepared view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Policy {
    pub(crate) render_size: UVec2,
    pub(crate) jitter_phases: i32,
    pub(crate) mip_bias: f32,
}

/// The backend's policy for a view: `fsr::policy` for FSR; for DLSS `dlss::sizing::policy`
/// from the optimal settings `dlss_optimal` gives for the quality (DLSS's reported render size,
/// never FSR's ratio; `None`: the view renders natively); `bilinear::policy` for Bilinear
/// (always a size). Only DLSS calls `dlss_optimal`.
pub(crate) fn policy(
    settings: UpscaleSettings,
    output: UVec2,
    limits: &wgpu::Limits,
    dlss_optimal: impl FnOnce(UpscaleQuality) -> Option<crate::dlss::sizing::Optimal>,
) -> Option<Policy> {
    match settings {
        UpscaleSettings::Fsr(fsr) => crate::fsr::policy(fsr.quality, output, limits),
        UpscaleSettings::Dlss(quality) => crate::dlss::sizing::policy(quality, output, dlss_optimal(quality)),
        UpscaleSettings::Bilinear(quality) => Some(super::bilinear::policy(quality, output)),
    }
}

/// The jitter for `frame` among `phases` (AMD's Halton sequence from the wgpu-ffx port, in
/// render pixels and AMD's sign; Bevy's `TemporalJitter` takes the opposite sign). No phases
/// (Bilinear's policy): none, without asking the sequence.
pub(crate) fn jitter_offset(frame: u32, phases: i32) -> Vec2 {
    if phases <= 0 {
        return Vec2::ZERO;
    }
    Vec2::from_array(wgpu_ffx::get_jitter_offset((frame % i32::MAX as u32) as i32, phases))
}

/// The upscaler a camera's components ask for: FSR where the device can run it, else DLSS's
/// component, else Bilinear's. The graphics settings put one of them on the camera only while
/// its upscaler is the effective choice (`graphics::apply`, `upscaler::UpscalerStatus`), and
/// never two; FSR is taken first. A DLSS or Bilinear camera does not depend on FSR's support.
pub(crate) fn camera_settings(fsr: Option<&FsrUpscaling>, dlss: Option<&DlssUpscaling>, bilinear: Option<&BilinearUpscaling>, fsr_supported: bool) -> Option<UpscaleSettings> {
    match (fsr, dlss, bilinear) {
        (Some(fsr), _, _) => fsr_supported.then_some(UpscaleSettings::Fsr(*fsr)),
        (None, Some(dlss), _) => Some(UpscaleSettings::Dlss(dlss.quality)),
        (None, None, Some(bilinear)) => Some(UpscaleSettings::Bilinear(bilinear.quality)),
        (None, None, None) => None,
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn extract(
    mut commands: Commands,
    cameras: Extract<Query<(RenderEntity, &Camera, &Projection, &GlobalTransform, Option<&FsrUpscaling>, Option<&DlssUpscaling>, Option<&BilinearUpscaling>)>>,
    existing: Query<Entity, With<UpscaleCamera>>,
    frame: Extract<Res<FrameCount>>,
    time: Extract<Res<Time<Real>>>,
    support: Extract<Res<crate::fsr::FsrSupport>>,
    loading: Extract<Option<Res<nvf_play::loading::Loading>>>,
    play_state: Extract<Option<Res<State<nvf_play::PlayState>>>>,
    pipboy_screen: Extract<Option<Res<nvf_play::pipboy::PipBoyScreen>>>,
) {
    let mut active = Vec::new();
    for (entity, camera, projection, transform, fsr, dlss, bilinear) in &cameras {
        let (Some(settings), Projection::Perspective(projection)) = (camera_settings(fsr, dlss, bilinear, support.0), projection) else { continue };
        if !camera.is_active || camera.physical_viewport_size().is_none_or(|s| s.x < 32 || s.y < 32) {
            continue;
        }
        active.push(entity);
        commands.entity(entity).insert(UpscaleCamera {
            settings,
            translation: transform.translation(),
            rotation: transform.rotation(),
            near: projection.near,
            fov: projection.fov,
            frame: frame.0,
            delta_ms: (time.delta_secs() * 1000.0).max(1.0),
            loading_generation: loading.as_ref().map_or(0, |loading| loading.shown),
            play_state: play_state.as_ref().map(|state| *state.get()),
            screen_mark: pipboy_screen.as_ref().is_some_and(|screen| screen.on_model),
        });
    }
    for entity in &existing {
        if !active.contains(&entity) {
            // A backend's own prepared state goes in its prepare (`fsr::prepare`).
            commands.entity(entity).remove::<(UpscaleCamera, PreparedUpscale, MainPassResolutionOverride, UnsuitableViewWarned)>();
        }
    }
}

/// A view the upscaler cannot use (multisampled, not HDR, or offset) whose warning has been
/// logged: [`prepare`] meets it every frame and says so once, until the view is usable again
/// or the upscaler is switched off.
#[derive(Component)]
pub(crate) struct UnsuitableViewWarned;

/// The pipelines of the shared passes, made with the first prepared view.
#[derive(Resource, Default)]
pub(crate) struct UpscaleRenderer(pub(crate) Option<Pipelines>);

/// One texture of a prepared view: its label, size, format and usages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Allocation {
    pub(crate) label: &'static str,
    pub(crate) size: UVec2,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) usage: wgpu::TextureUsages,
}

impl Allocation {
    pub(crate) fn create(&self, device: &wgpu::Device) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(self.label), size: wgpu::Extent3d { width: self.size.x, height: self.size.y, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: self.format, usage: self.usage, view_formats: &[],
        })
    }
}

/// The shared textures of a view: the corrected depth (render size), then the expanded depth
/// (display size). With the resolution uniform, all a prepared view holds whatever the backend.
pub(crate) fn common_textures(render_size: UVec2, output_size: UVec2) -> [Allocation; 2] {
    let attachment = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT;
    [
        Allocation { label: "fsr_linear_camera_depth", size: render_size, format: wgpu::TextureFormat::Depth32Float, usage: attachment },
        Allocation { label: "fsr_full_resolution_depth", size: output_size, format: wgpu::TextureFormat::Depth32Float, usage: attachment | wgpu::TextureUsages::COPY_SRC },
    ]
}

/// Whether a backend gets FSR's own state (context, masks, history, dilation buffers) beside
/// the common prepared view: only FSR does. `fsr::prepare` asks this.
pub(crate) fn allocates_fsr_state(upscaler: Upscaler) -> bool {
    upscaler == Upscaler::Fsr
}

/// The prepared view every temporal upscaler shares.
#[derive(Component)]
pub(crate) struct PreparedUpscale {
    pub(crate) render_size: UVec2,
    pub(crate) output_size: UVec2,
    pub(crate) camera: UpscaleCamera,
    /// A camera cut this frame ([`UpscaleCamera::cut_from`]), or a new prepared view.
    pub(crate) reset: bool,
    /// The depth the upscaler reads (render size, first-person depth unsqueezed); frame
    /// generation's prepare pass reads it too.
    pub(crate) depth: wgpu::Texture,
    expanded_depth: wgpu::Texture,
    resolution: wgpu::Buffer,
    /// The upscaler reconstructed this frame (not the spatial fallback).
    pub(crate) history_valid: AtomicBool,
    /// A backend without state of its own has said that it draws this view (Bilinear's
    /// `bilinear_pass`, once per prepared view: each switch on and each new size).
    pub(crate) drawing_said: AtomicBool,
}

impl PreparedUpscale {
    fn new(device: &RenderDevice, camera: UpscaleCamera, render_size: UVec2, output_size: UVec2) -> Self {
        let device = device.wgpu_device();
        let [depth, expanded_depth] = common_textures(render_size, output_size).map(|a| a.create(device));
        let resolution_bytes: Vec<u8> = [render_size.x, render_size.y, output_size.x, output_size.y].into_iter().flat_map(u32::to_ne_bytes).collect();
        Self {
            render_size, output_size, camera, reset: true,
            depth, expanded_depth,
            resolution: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("fsr_resolution"), contents: &resolution_bytes, usage: wgpu::BufferUsages::UNIFORM,
            }),
            history_valid: AtomicBool::new(false),
            drawing_said: AtomicBool::new(false),
        }
    }
}

/// What [`prepare`] does with a view's prepared state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Transition {
    /// The same sizes and backend: kept, `reset` on a camera cut.
    Keep { reset: bool },
    /// New sizes, a new backend or no prepared view yet: made again (history reset).
    Recreate,
}

/// The rule for a view's prepared state; `old` is the kept camera and sizes, if any.
pub(crate) fn transition(old: Option<(&UpscaleCamera, UVec2, UVec2)>, camera: &UpscaleCamera, render_size: UVec2, output_size: UVec2) -> Transition {
    match old {
        Some((previous, render, output)) if render == render_size && output == output_size && previous.settings.upscaler() == camera.settings.upscaler() => {
            Transition::Keep { reset: camera.cut_from(previous) }
        }
        _ => Transition::Recreate,
    }
}

/// What a view with an upscaler camera asking for `choice` was drawn with: upscaled from the
/// policy's render size when the view is `suitable` and the backend gave one, else natively at
/// the output size ([`prepare`]'s two native branches).
pub(crate) fn drawn(choice: UpscalerChoice, policy_render: Option<UVec2>, output: UVec2, suitable: bool) -> DrawnUpscale {
    match policy_render.filter(|_| suitable) {
        Some(render) => DrawnUpscale::Upscaled { choice, render, output },
        None => DrawnUpscale::Native { choice, output },
    }
}

/// The view's common preparation, before any backend's own (`fsr::prepare` runs after it).
/// Also says what the first view was drawn with ([`UpscaleDrawn`], `None` without an upscaler
/// camera), for the performance overlay's Upscaling row. The jitter and mip bias are set where
/// the camera has them: FSR's and DLSS's cameras always do, Bilinear's never.
#[allow(clippy::type_complexity)]
pub(crate) fn prepare(
    mut commands: Commands,
    device: Res<RenderDevice>,
    shared: Res<UpscaleDrawn>,
    mut renderer: ResMut<UpscaleRenderer>,
    mut dlss_sizes: ResMut<crate::dlss::OptimalSizes>,
    mut views: Query<(
        Entity, &ExtractedView, &UpscaleCamera, &Msaa, &mut CameraMainTextureUsages,
        Option<&mut TemporalJitter>, Option<&mut MipBias>, Option<&mut PreparedUpscale>, Option<&mut crate::ssao::AmbientOcclusion>,
        Has<UnsuitableViewWarned>,
    )>,
) {
    let mut first: Option<DrawnUpscale> = None;
    for (entity, view, camera, msaa, mut usages, mut jitter, mut bias, old, ao, warned) in &mut views {
        let choice = camera.settings.choice();
        if *msaa != Msaa::Off || view.target_format != wgpu::TextureFormat::Rgba16Float || view.viewport.xy() != UVec2::ZERO {
            first.get_or_insert(drawn(choice, None, view.viewport.zw(), false));
            // Met every frame while it lasts: said once. The view renders natively, without
            // the upscaler's jitter and mip bias, as for an unsupported size below.
            if !warned {
                warn!("{} requires a single-sampled HDR view with no viewport offset; rendering natively until it is one", camera.settings.upscaler().name());
                commands.entity(entity).insert(UnsuitableViewWarned);
            }
            set_jitter_and_bias(jitter.as_deref_mut(), bias.as_deref_mut(), Vec2::ZERO, 0.0);
            commands.entity(entity).remove::<(PreparedUpscale, MainPassResolutionOverride)>();
            continue;
        }
        if warned {
            commands.entity(entity).remove::<UnsuitableViewWarned>();
        }
        let output_size = view.viewport.zw();
        // DLSS's optimal settings asked here, once per mode, output size and HDR-ness, before
        // the render targets and uniforms are prepared (dlss/sizing.rs).
        let hdr = crate::dlss::sizing::hdr_format(view.target_format);
        let dlss_optimal = |quality| dlss_sizes.render_size(crate::dlss::sizing::Key { quality, output: output_size, hdr });
        let policy = policy(camera.settings, output_size, &device.limits(), dlss_optimal);
        first.get_or_insert(drawn(choice, policy.map(|p| p.render_size), output_size, true));
        let Some(policy) = policy else {
            set_jitter_and_bias(jitter.as_deref_mut(), bias.as_deref_mut(), Vec2::ZERO, 0.0);
            commands.entity(entity).remove::<(PreparedUpscale, MainPassResolutionOverride)>();
            continue;
        };
        let render_size = policy.render_size;
        renderer.0.get_or_insert_with(|| Pipelines::new(device.wgpu_device()));
        // DLSS's own usages on the cameras it draws (FSR adds its own in `fsr::prepare`).
        if matches!(camera.settings, UpscaleSettings::Dlss(_)) {
            usages.0 |= crate::dlss::main_texture_usages();
        }
        // Bevy applies its projection jitter with the opposite displacement sign.
        set_jitter_and_bias(jitter.as_deref_mut(), bias.as_deref_mut(), -jitter_offset(camera.frame, policy.jitter_phases), policy.mip_bias);
        // The ambient occlusion: at the render size before the upscaler (ssao.rs
        // `ao_before_fsr`), or with `NVF_AO_BEFORE_FSR=0` after tonemapping, from the depth
        // prepass drawn at the render size and expanded by nearest texels
        // ([`Pipelines::expand_depth`]), its normals taken from neighbouring texels as drawn
        // (ssao.wesl `normal_at`). Before the uniforms are written
        // (RenderSystems::PrepareResources); the extraction resets it every frame.
        if let Some(mut ao) = ao {
            if *crate::ssao::AO_BEFORE_FSR {
                ao.before_fsr(render_size, output_size);
            } else if render_size != output_size {
                ao.render_scale = Vec4::new(render_size.x as f32, render_size.y as f32, output_size.x as f32, output_size.y as f32);
            }
        }
        match (transition(old.as_deref().map(|old| (&old.camera, old.render_size, old.output_size)), camera, render_size, output_size), old) {
            (Transition::Keep { reset }, Some(mut old)) => {
                old.reset = reset;
                old.camera = *camera;
            }
            _ => {
                commands.entity(entity).insert(PreparedUpscale::new(&device, *camera, render_size, output_size));
            }
        }
        commands.entity(entity).insert(MainPassResolutionOverride(render_size));
    }
    shared.set(first);
}

/// The jitter and mip bias put on a camera that has them (a Bilinear camera has neither).
fn set_jitter_and_bias(jitter: Option<&mut TemporalJitter>, bias: Option<&mut MipBias>, offset: Vec2, mip_bias: f32) {
    if let Some(jitter) = jitter {
        jitter.offset = offset;
    }
    if let Some(bias) = bias {
        bias.0 = mip_bias;
    }
}

/// A full-screen triangle pipeline (`vertex` entry point) drawing into `colors`, or into a
/// Depth32Float target written always when `colors` is empty.
pub(crate) fn fullscreen_pipeline(
    device: &wgpu::Device,
    label: &str,
    source: &'static str,
    layout: &wgpu::BindGroupLayout,
    entry: &str,
    colors: &[(wgpu::TextureFormat, wgpu::ColorWrites)],
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)) });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(layout)], immediate_size: 0 });
    let targets: Vec<_> = colors.iter().map(|&(format, write_mask)| Some(wgpu::ColorTargetState { format, blend: None, write_mask })).collect();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label), layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("vertex"), compilation_options: default(), buffers: &[] },
        fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some(entry), compilation_options: default(), targets: &targets }),
        primitive: default(), depth_stencil: colors.is_empty().then_some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::Always), stencil: default(), bias: default() }),
        multisample: default(), multiview_mask: None, cache: None,
    })
}

/// The shared passes: the depth conversions (`fsr_depth.wgsl`) and the spatial fallback and
/// alpha copy (`fsr_fallback.wgsl`). A backend's own passes may reuse the fallback layout and
/// the sampler (FSR's history luminance does).
pub(crate) struct Pipelines {
    depth_layout: wgpu::BindGroupLayout,
    pub(crate) fallback_layout: wgpu::BindGroupLayout,
    prepare_depth: wgpu::RenderPipeline,
    expand_depth: wgpu::RenderPipeline,
    depth_as_color: wgpu::RenderPipeline,
    fallback: wgpu::RenderPipeline,
    alpha: wgpu::RenderPipeline,
    /// Linear filtering.
    pub(crate) sampler: wgpu::Sampler,
}

impl Pipelines {
    fn new(device: &wgpu::Device) -> Self {
        let depth_entry = wgpu::BindGroupLayoutEntry {
            binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None,
        };
        let color_entry = wgpu::BindGroupLayoutEntry {
            binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None,
        };
        let uniform_entry = wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("fsr_depth_layout"), entries: &[depth_entry, uniform_entry] });
        let fallback_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("fsr_fallback_layout"), entries: &[
            color_entry, uniform_entry,
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
        ] });
        let rgba = [(wgpu::TextureFormat::Rgba16Float, wgpu::ColorWrites::ALL)];
        let alpha = [(wgpu::TextureFormat::Rgba16Float, wgpu::ColorWrites::ALPHA)];
        Self {
            prepare_depth: fullscreen_pipeline(device, "fsr_prepare_depth", include_str!("../fsr_depth.wgsl"), &depth_layout, "prepare_depth", &[]),
            expand_depth: fullscreen_pipeline(device, "fsr_expand_depth", include_str!("../fsr_depth.wgsl"), &depth_layout, "expand_depth", &[]),
            depth_as_color: fullscreen_pipeline(device, "fsr_depth_as_color", include_str!("../fsr_depth.wgsl"), &depth_layout, "depth_as_color", &[(wgpu::TextureFormat::R32Float, wgpu::ColorWrites::ALL)]),
            fallback: fullscreen_pipeline(device, "fsr_spatial_fallback", include_str!("../fsr_fallback.wgsl"), &fallback_layout, "fragment", &rgba),
            alpha: fullscreen_pipeline(device, "fsr_restore_alpha", include_str!("../fsr_fallback.wgsl"), &fallback_layout, "alpha", &alpha),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..default() }),
            depth_layout, fallback_layout,
        }
    }

    fn depth_group(&self, device: &wgpu::Device, prepared: &PreparedUpscale, source: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("fsr_depth_group"), layout: &self.depth_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source) },
            wgpu::BindGroupEntry { binding: 1, resource: prepared.resolution.as_entire_binding() },
        ] })
    }

    /// A bind group of the fallback layout: `source`, the resolution uniform and the sampler.
    pub(crate) fn fallback_group(&self, device: &wgpu::Device, label: &str, prepared: &PreparedUpscale, source: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some(label), layout: &self.fallback_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source) },
            wgpu::BindGroupEntry { binding: 1, resource: prepared.resolution.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
        ] })
    }

    fn draw_depth(encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, pipeline: &wgpu::RenderPipeline, group: &wgpu::BindGroup) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fsr_depth_conversion"), color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: target, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
            timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// The depth prepass (render rectangle) into [`PreparedUpscale::depth`], first-person
    /// depth unsqueezed (`fsr_depth.wgsl` `prepare_depth`).
    pub(crate) fn corrected_depth(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, prepared: &PreparedUpscale, depth: &wgpu::Texture) {
        let depth_view = depth.create_view(&default());
        let group = self.depth_group(device, prepared, &depth_view);
        Self::draw_depth(encoder, &prepared.depth.create_view(&default()), &self.prepare_depth, &group);
    }

    /// [`PreparedUpscale::depth`] (drawn by [`Self::corrected_depth`]) again as an R32Float
    /// colour image, for an FSR given its images through a FidelityFX API library (fsr.rs,
    /// ffx_api.rs `Frame::depth`; `fsr_depth.wgsl` `depth_as_color`).
    pub(crate) fn depth_as_color(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, prepared: &PreparedUpscale, target: &wgpu::Texture) {
        let group = self.depth_group(device, prepared, &prepared.depth.create_view(&default()));
        let target = target.create_view(&default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fsr_depth_as_color"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &target, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        });
        pass.set_pipeline(&self.depth_as_color);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// The depth prepass drawn at the render size expanded to the display size by nearest
    /// texels and copied back over it, for the screen-space effects after the upscaler.
    pub(crate) fn expand_depth(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, prepared: &PreparedUpscale, depth: &wgpu::Texture) {
        if prepared.render_size == prepared.output_size { return; }
        let source = depth.create_view(&default());
        let group = self.depth_group(device, prepared, &source);
        Self::draw_depth(encoder, &prepared.expanded_depth.create_view(&default()), &self.expand_depth, &group);
        encoder.copy_texture_to_texture(prepared.expanded_depth.as_image_copy(), depth.as_image_copy(), prepared.expanded_depth.size());
    }

    /// The render rectangle scaled up to the display size without history, when the upscaler
    /// cannot reconstruct (`fsr_fallback.wgsl` `fragment`).
    pub(crate) fn fallback(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, prepared: &PreparedUpscale, target: &ViewTarget) {
        let post = target.post_process_write();
        self.stretch_into(device, encoder, prepared, "fsr_spatial_fallback", post.source, post.destination);
    }

    /// Bilinear's whole upscale (#2414): the render rectangle stretched to the display size by
    /// the fallback's bilinear filter into the view's other main texture, the scene's alpha
    /// restored over it where a shape marks it (the Pip-Boy's screen on its model, as for FSR
    /// and DLSS: the filter would blend the mark's edge), then the flip that makes it the main
    /// texture for the passes after.
    pub(crate) fn stretch(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, prepared: &PreparedUpscale, target: &ViewTarget) {
        self.stretch_into(device, encoder, prepared, "nvf_bilinear", target.main_texture_view(), target.main_texture_other_view());
        if prepared.camera.screen_mark {
            self.restore_alpha(device, encoder, prepared, target);
        }
        target.post_process_write();
    }

    /// `source`'s render rectangle drawn over the whole of `destination` by the fallback's
    /// bilinear filter (`fsr_fallback.wgsl` `fragment`).
    fn stretch_into(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, prepared: &PreparedUpscale, label: &str, source: &wgpu::TextureView, destination: &wgpu::TextureView) {
        let group = self.fallback_group(device, "fsr_fallback_group", prepared, source);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: destination, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        });
        pass.set_pipeline(&self.fallback);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// The upscaler's output has alpha 1: the scene's own alpha is copied over from the nearest
    /// render-resolution texel (the main texture, before the flip), so the marks drawn into
    /// it (the Pip-Boy screen's, `imagespace.wesl` `SCREEN_MARK`) reach the post passes.
    pub(crate) fn restore_alpha(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, prepared: &PreparedUpscale, target: &ViewTarget) {
        let group = self.fallback_group(device, "fsr_restore_alpha_group", prepared, target.main_texture_view());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fsr_restore_alpha"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target.main_texture_other_view(), depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        });
        pass.set_pipeline(&self.alpha);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use UpscaleQuality::*;

    fn camera(settings: UpscaleSettings, frame: u32) -> UpscaleCamera {
        UpscaleCamera { settings, translation: Vec3::ZERO, rotation: Quat::IDENTITY, near: 5.0, fov: 1.0, frame, delta_ms: 16.0, loading_generation: 0, play_state: None, screen_mark: false }
    }

    fn fsr(quality: UpscaleQuality) -> UpscaleSettings {
        UpscaleSettings::Fsr(FsrUpscaling { quality, ..default() })
    }

    /// Every texture a view of `upscaler` gets: the common ones, and FSR's own only for FSR.
    fn allocation_plan(upscaler: Upscaler, render: UVec2, output: UVec2) -> Vec<Allocation> {
        let mut plan = common_textures(render, output).to_vec();
        if allocates_fsr_state(upscaler) {
            plan.extend(crate::fsr::fsr_textures(render, output));
        }
        plan
    }

    #[test]
    fn the_dlss_plan_allocates_no_fsr_state() {
        let (render, output) = (UVec2::new(853, 480), UVec2::new(1280, 720));
        assert!(allocates_fsr_state(Upscaler::Fsr));
        assert!(!allocates_fsr_state(Upscaler::Dlss));
        assert!(!allocates_fsr_state(Upscaler::Off));
        let dlss = allocation_plan(Upscaler::Dlss, render, output);
        assert_eq!(dlss, common_textures(render, output).to_vec(), "only the common prepared view");
        let fsr_own = crate::fsr::fsr_textures(render, output);
        for texture in &fsr_own {
            assert!(!dlss.iter().any(|d| d.label == texture.label), "{} is FSR's own", texture.label);
        }
        let fsr = allocation_plan(Upscaler::Fsr, render, output);
        assert_eq!(fsr.len(), common_textures(render, output).len() + fsr_own.len());
    }

    #[test]
    fn the_common_textures_are_the_depths_fsr_always_made() {
        let attachment = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT;
        let [depth, expanded] = common_textures(UVec2::new(853, 480), UVec2::new(1280, 720));
        assert_eq!(depth, Allocation { label: "fsr_linear_camera_depth", size: UVec2::new(853, 480), format: wgpu::TextureFormat::Depth32Float, usage: attachment });
        assert_eq!(expanded, Allocation { label: "fsr_full_resolution_depth", size: UVec2::new(1280, 720), format: wgpu::TextureFormat::Depth32Float, usage: attachment | wgpu::TextureUsages::COPY_SRC });
    }

    #[test]
    fn switching_off_fsr_and_dlss_in_the_common_state() {
        let (render, output) = (UVec2::new(853, 480), UVec2::new(1280, 720));
        let unsupported = || camera_settings(Some(&FsrUpscaling::default()), None, None, false);
        // Off: no component, no prepared view.
        assert!(camera_settings(None, None, None, true).is_none());
        assert!(unsupported().is_none(), "FSR where the device cannot run it renders natively");
        // Off -> FSR: made.
        let fsr_camera = camera(camera_settings(Some(&FsrUpscaling::default()), None, None, true).unwrap(), 10);
        assert_eq!(fsr_camera.settings.choice(), UpscalerChoice::Fsr(Quality));
        assert_eq!(transition(None, &fsr_camera, render, output), Transition::Recreate);
        // FSR, the next frame: kept, no reset.
        let next = camera(fsr(Quality), 11);
        assert_eq!(transition(Some((&fsr_camera, render, output)), &next, render, output), Transition::Keep { reset: false });
        // FSR -> DLSS at the same sizes: a new backend is made again.
        let dlss_camera = camera(camera_settings(None, Some(&DlssUpscaling { quality: Quality }), None, true).unwrap(), 12);
        assert_eq!(dlss_camera.settings.choice(), UpscalerChoice::Dlss(Quality));
        assert_eq!(transition(Some((&next, render, output)), &dlss_camera, render, output), Transition::Recreate);
        // DLSS's quality changed at the sizes DLSS reported: kept, but a cut.
        let balanced = camera(UpscaleSettings::Dlss(Balanced), 13);
        assert_eq!(transition(Some((&dlss_camera, render, output)), &balanced, render, output), Transition::Keep { reset: true });
        assert!(balanced.cut_from(&dlss_camera));
        // New sizes: made again.
        assert_eq!(transition(Some((&balanced, render, output)), &camera(UpscaleSettings::Dlss(Balanced), 14), UVec2::new(640, 360), output), Transition::Recreate);
        // DLSS -> FSR: made again, with FSR's own state.
        assert_eq!(transition(Some((&balanced, render, output)), &camera(fsr(Quality), 14), render, output), Transition::Recreate);
        // FSR is taken when both components are there (the settings never put both).
        assert_eq!(camera_settings(Some(&FsrUpscaling::default()), Some(&DlssUpscaling { quality: Performance }), None, true).unwrap().upscaler(), Upscaler::Fsr);
        // DLSS -> Bilinear at the same sizes: made again, without FSR's state (#2414).
        let bilinear = camera(camera_settings(None, None, Some(&BilinearUpscaling { quality: Quality }), true).unwrap(), 15);
        assert_eq!(bilinear.settings.choice(), UpscalerChoice::Bilinear(Quality));
        assert_eq!(transition(Some((&balanced, render, output)), &bilinear, render, output), Transition::Recreate);
        assert!(!allocates_fsr_state(bilinear.settings.upscaler()));
        // Bilinear, the next frame: kept; its quality changed at the same sizes: kept, a cut.
        assert_eq!(transition(Some((&bilinear, render, output)), &camera(UpscaleSettings::Bilinear(Quality), 16), render, output), Transition::Keep { reset: false });
        assert_eq!(transition(Some((&bilinear, render, output)), &camera(UpscaleSettings::Bilinear(Balanced), 16), render, output), Transition::Keep { reset: true });
        // Bilinear -> FSR: made again; FSR and DLSS are taken before it.
        assert_eq!(transition(Some((&bilinear, render, output)), &camera(fsr(Quality), 16), render, output), Transition::Recreate);
        assert_eq!(camera_settings(None, Some(&DlssUpscaling { quality: Quality }), Some(&BilinearUpscaling { quality: Performance }), false).unwrap().upscaler(), Upscaler::Dlss);
        assert_eq!(camera_settings(None, None, Some(&BilinearUpscaling { quality: Performance }), false).unwrap().upscaler(), Upscaler::Bilinear, "whatever FSR's support");
    }

    #[test]
    fn the_backend_policy_is_fsrs_for_fsr_and_dlsss_reported_size_for_dlss() {
        use crate::dlss::sizing::{self, Optimal};
        let limits = wgpu::Limits { max_storage_textures_per_shader_stage: 8, ..default() };
        let output = UVec2::new(1280, 720);
        let never = |_| -> Option<Optimal> { panic!("FSR never asks DLSS") };
        let fsr_policy = policy(fsr(Quality), output, &limits, never).expect("FSR Quality at 720p");
        assert_eq!(fsr_policy.render_size, UVec2::new(853, 480));
        assert_eq!(fsr_policy.jitter_phases, wgpu_ffx::get_jitter_phase_count(853, 1280));
        assert_eq!(policy(fsr(UltraPerformance), UVec2::splat(96), &limits, never), None, "below FSR's smallest render size");
        // DLSS: what DLSS reports for the asked quality, never FSR's ratio.
        let optimal = Optimal { render: UVec2::new(862, 485), min: UVec2::new(640, 360), max: output };
        let mut asked = None;
        let dlss_policy = policy(UpscaleSettings::Dlss(Balanced), output, &limits, |q| {
            asked = Some(q);
            Some(optimal)
        })
        .expect("DLSS reported a size");
        assert_eq!(asked, Some(Balanced));
        assert_eq!(dlss_policy.render_size, UVec2::new(862, 485));
        assert_eq!(dlss_policy.jitter_phases, sizing::jitter_phases(Balanced, optimal.render, output));
        assert_eq!(dlss_policy.mip_bias, sizing::mip_bias(optimal.render, output));
        assert_eq!(policy(UpscaleSettings::Dlss(Quality), output, &limits, |_| None), None, "no size from DLSS: native");
    }

    /// The camera's components alone choose the backend (the setting puts them there): a DLSS
    /// camera is DLSS's at its quality whatever FSR's support, gets the common prepared view
    /// only, and an FSR camera is never turned into a DLSS one.
    #[test]
    fn the_components_choose_the_backend_and_a_dlss_camera_gets_no_fsr_state() {
        for quality in UpscaleQuality::ALL {
            for fsr_supported in [true, false] {
                let dlss = camera_settings(None, Some(&DlssUpscaling { quality }), None, fsr_supported).unwrap();
                assert!(matches!(dlss, UpscaleSettings::Dlss(q) if q == quality), "{quality:?} {fsr_supported}");
                assert!(!allocates_fsr_state(dlss.upscaler()));
            }
            let fsr_camera = camera_settings(Some(&FsrUpscaling { quality, sharpness: 0.2 }), None, None, true).unwrap();
            assert!(matches!(fsr_camera, UpscaleSettings::Fsr(f) if f.quality == quality), "FSR stays FSR");
        }
    }

    /// What the overlay's Upscaling row is told: the policy's render size when the view could
    /// use the upscaler and the backend gave one, else the output size drawn natively.
    #[test]
    fn what_a_view_was_drawn_with() {
        let (render, output) = (UVec2::new(853, 480), UVec2::new(1280, 720));
        let dlss = UpscalerChoice::Dlss(Quality);
        assert_eq!(drawn(dlss, Some(render), output, true), DrawnUpscale::Upscaled { choice: dlss, render, output }, "upscaled at the policy's size");
        let fsr = UpscalerChoice::Fsr(NativeAa);
        assert_eq!(drawn(fsr, Some(output), output, true), DrawnUpscale::Upscaled { choice: fsr, render: output, output }, "Native AA: upscaled at 1:1");
        assert_eq!(drawn(dlss, Some(render), output, false), DrawnUpscale::Native { choice: dlss, output }, "an unsuitable view renders natively");
        assert_eq!(drawn(dlss, None, output, true), DrawnUpscale::Native { choice: dlss, output }, "no size from the backend: native");
        // The policy's own None for DLSS (no optimal settings) comes to the same.
        let limits = wgpu::Limits { max_storage_textures_per_shader_stage: 8, ..default() };
        let none = policy(UpscaleSettings::Dlss(Quality), output, &limits, |_| None).map(|p| p.render_size);
        assert_eq!(drawn(dlss, none, output, true), DrawnUpscale::Native { choice: dlss, output });
    }

    #[test]
    fn the_jitter_is_amds_sequence_for_the_frame() {
        let phases = wgpu_ffx::get_jitter_phase_count(853, 1280);
        for frame in [0, 1, 7, 1000, u32::MAX] {
            assert_eq!(jitter_offset(frame, phases), Vec2::from_array(wgpu_ffx::get_jitter_offset((frame % i32::MAX as u32) as i32, phases)));
            assert_eq!(jitter_offset(frame, 0), Vec2::ZERO, "no phases (Bilinear): no jitter");
            assert_eq!(jitter_offset(frame, -1), Vec2::ZERO);
        }
    }

    /// The jitter and bias go only where the camera has the components; a Bilinear camera has
    /// neither, and nothing is made for it.
    #[test]
    fn jitter_and_bias_are_set_only_where_the_camera_has_them() {
        let (mut jitter, mut bias) = (TemporalJitter::default(), MipBias(0.0));
        set_jitter_and_bias(Some(&mut jitter), Some(&mut bias), Vec2::new(0.25, -0.5), -1.5);
        assert_eq!((jitter.offset, bias.0), (Vec2::new(0.25, -0.5), -1.5));
        set_jitter_and_bias(None, None, Vec2::ONE, 2.0);
        assert_eq!((jitter.offset, bias.0), (Vec2::new(0.25, -0.5), -1.5), "untouched: they were not given");
    }

    /// Bilinear's policy through the backend switch: FSR's ratio at every quality, no jitter
    /// phases, no bias, DLSS never asked, and a size even where FSR gives none (#2414).
    #[test]
    fn the_bilinear_policy_is_fsrs_ratio_without_jitter_or_bias() {
        let limits = wgpu::Limits::default();
        let never = |_| -> Option<crate::dlss::sizing::Optimal> { panic!("Bilinear never asks DLSS") };
        for output in [UVec2::new(1280, 720), UVec2::new(1920, 1080), UVec2::new(2560, 1440)] {
            for quality in UpscaleQuality::ALL {
                let policy = policy(UpscaleSettings::Bilinear(quality), output, &limits, never).expect("always a size");
                assert_eq!(policy, Policy { render_size: quality.render_size(output), jitter_phases: 0, mip_bias: 0.0 }, "{quality:?} at {output}");
                assert_eq!(drawn(UpscalerChoice::Bilinear(quality), Some(policy.render_size), output, true), DrawnUpscale::Upscaled { choice: UpscalerChoice::Bilinear(quality), render: policy.render_size, output });
            }
        }
        // Below FSR's smallest render size (FSR renders natively there) Bilinear still draws
        // smaller.
        assert_eq!(crate::fsr::policy(UltraPerformance, UVec2::splat(96), &limits), None);
        let tiny = policy(UpscaleSettings::Bilinear(UltraPerformance), UVec2::splat(96), &limits, never).unwrap();
        assert_eq!(tiny.render_size, UVec2::splat(32), "below FSR's smallest size too");
    }
}
