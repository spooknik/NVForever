//! Bilinear (#2414): the scene drawn at a lower resolution and stretched to the window with a
//! plain bilinear filter. No jitter, no motion vectors, no history, nothing rebuilt: the
//! cheapest way to draw fewer pixels, for a GPU where FSR's motion-vector prepass and its
//! compute passes at the display size cost more than they save, or that cannot run FSR or
//! DLSS at all (docs/design/fsr-upscaling.md, "Bilinear").
//!
//! It is an upscaler choice beside FSR 3.1 and DLSS (`upscaler::Upscaler::Bilinear`), off
//! unless chosen, available on every GPU and switched at once. The camera gets
//! [`super::BilinearUpscaling`] (`graphics::apply`); the common preparation (`prep::prepare`)
//! draws the main pass at [`policy`]'s render size (`MainPassResolutionOverride`), and
//! [`bilinear_pass`] stretches it where FSR's pass sits, before bloom, tonemapping and the UI,
//! which stay at the window's size.
//!
//! The view is HDR only when something else asks for it (the light glow: `graphics::apply`);
//! otherwise its picture is 8-bit, tonemapped in the material shaders, with no separate
//! tonemapping pass, and the stretch reads and writes half the bytes (`prep::format_suitable`).
//! Measured on an Intel N150 at 1920x1080, Performance, with the HDR view: the stretch with the
//! depth expansion 2.6 ms of the GPU's frame and the tonemapping pass 1.3 ms [observed,
//! 2026-11-14: the main session's profile build].

use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::post_process::auto_exposure::AutoExposure;
use bevy::prelude::*;
use bevy::render::{
    diagnostic::RecordDiagnostics,
    renderer::{RenderContext, RenderDevice, ViewQuery},
    view::ViewTarget,
};
use std::sync::atomic::{AtomicU8, Ordering};

use super::prep::{Policy, PreparedUpscale, UpscaleRenderer, UpscaleSettings};
use super::UpscaleQuality;

/// Bilinear's [`Policy`] for a view of `output` size: FSR's ratio for the quality
/// (`fsr::FsrQuality::render_size`, so each mode draws the share of the screen it does with
/// FSR), no jitter phases and no mip bias [design, #2414: nothing gathers detail over the
/// frames, so a jitter would only shake the picture and a negative bias make textures
/// shimmer]. Always a size: the stretch needs nothing of the GPU beyond a filtered texture.
pub(crate) fn policy(quality: UpscaleQuality, output: UVec2) -> Policy {
    Policy { render_size: quality.render_size(output), jitter_phases: 0, mip_bias: 0.0 }
}

/// What reads the depth prepass at the display size after the stretch, this frame. The main
/// passes draw it at the render size into the render rectangle; FSR and DLSS always expand it
/// to the whole texture afterwards (`prep::Pipelines::expand_depth`: a display-size depth pass
/// and a copy back, about as many bytes as the stretch itself at 1080p [hypothesis: by the
/// texture sizes]). Bilinear expands it only when one of these will read it:
/// - the image space's blur passes (`imagespace::blur_pass`: a motion smear, a modifier's blur
///   such as the menus', a depth of field), whenever the grade asks for one;
/// - the sun shafts (`shafts::shafts_pass`), while the sun is on the screen;
/// - the eye adaptation's metering mask (`eye_adapt::mask_pass`), on with the light glow;
/// - the occlusion applied after tonemapping (`ssao::ao_pass`, `NVF_AO_BEFORE_FSR=0` only;
///   before the upscaler it reads the depth as drawn, `ssao::ao_before_fsr`), while the view
///   carries the occlusion (`ssao::reads_depth_after_upscaler`).
///
/// Nothing else after the stretch reads it [design: a search of the engine's passes for the
/// prepass depth, 2026-11-14: FXAA, SMAA, bloom, the bright pass, the grade itself, the
/// sharpening and tonemapping do not; TAA and frame generation never run with Bilinear; water
/// (`water.wesl`) and the soft edges of blended shapes (`nif_material.wesl`) read it inside the
/// main passes, before the stretch]. With the Low preset
/// (no glow, no shafts, no occlusion) only a blurring modifier asks for it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DepthReaders {
    pub(crate) blur: bool,
    pub(crate) shafts: bool,
    pub(crate) eye_mask: bool,
    pub(crate) ao_after: bool,
}

impl DepthReaders {
    pub(crate) fn any(self) -> bool {
        self.blur || self.shafts || self.eye_mask || self.ao_after
    }
}

/// Whether the stretch says what it draws this frame (`Bilinear stretch active: ...`): the first
/// frame of a prepared view, and each frame its picture has turned HDR or 8-bit since it last
/// said so (the light glow switched while Bilinear draws: `graphics::apply`); `said` is the
/// prepared view's [`super::prep::PreparedUpscale::drawing_said`] (0 nothing yet, 1 8-bit, 2 HDR).
fn says_drawing(said: &AtomicU8, hdr: bool) -> bool {
    let now = if hdr { 2 } else { 1 };
    said.swap(now, Ordering::Relaxed) != now
}

/// Bilinear's pass for a view it draws: the render rectangle stretched to the display size
/// (`prep::Pipelines::stretch`, with the scene's alpha restored where a shape marks it), then
/// the depth prepass expanded to the display size, as FSR and DLSS do, but only when a pass
/// after it reads the depth this frame ([`DepthReaders`]). Nothing is reconstructed, so the
/// prepared view's history stays invalid (DLSS frame generation and FSR's frame generation
/// never take a Bilinear view).
#[allow(clippy::type_complexity)]
pub(crate) fn bilinear_pass(
    view: ViewQuery<(
        &ViewTarget,
        &PreparedUpscale,
        Option<&ViewPrepassTextures>,
        (
            Option<&crate::imagespace::ImageSpaceGrade>,
            Option<&crate::shafts::LightShafts>,
            Has<AutoExposure>,
            Option<&crate::ssao::AoView>,
            Has<crate::ssao::AmbientOcclusion>,
        ),
    )>,
    upscale: Res<UpscaleRenderer>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, prepared, prepass, (grade, shafts, eye_mask, ao, ao_live)) = view.into_inner();
    let UpscaleSettings::Bilinear(_) = prepared.camera.settings else { return };
    let Some(common) = &upscale.0 else { return };
    let device = device.wgpu_device();
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_bilinear");
    let encoder = ctx.command_encoder();
    prepared.history_valid.store(false, Ordering::Relaxed);
    if !common.stretch(device, encoder, prepared, target) {
        span.end(ctx.command_encoder());
        return;
    }
    let hdr = target.main_texture_format() == wgpu::TextureFormat::Rgba16Float;
    if says_drawing(&prepared.drawing_said, hdr) {
        let (render, output) = (prepared.render_size, prepared.output_size);
        let picture = if hdr { "HDR" } else { "8-bit" };
        info!("Bilinear stretch active: {}x{} -> {}x{}, {picture} picture", render.x, render.y, output.x, output.y);
    }
    let readers = DepthReaders {
        blur: grade.is_some_and(|g| g.blurs()),
        shafts: shafts.is_some_and(|s| s.draws()),
        eye_mask,
        ao_after: crate::ssao::reads_depth_after_upscaler(ao.map(|ao| ao.after_upscaler()), ao_live),
    };
    if let Some(depth) = prepass.and_then(|prepass| prepass.depth.as_ref()).filter(|_| readers.any()) {
        common.expand_depth(device, encoder, prepared, &depth.texture.texture);
    }
    span.end(ctx.command_encoder());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The render sizes the issue measured on its N150 at 1920x1080 (#2414: 1280x720 and
    /// 960x540) are Quality's and Performance's; every mode draws FSR's share.
    #[test]
    fn bilinear_draws_fsrs_share_of_the_screen() {
        let output = UVec2::new(1920, 1080);
        let sizes: Vec<UVec2> = UpscaleQuality::ALL.iter().map(|q| policy(*q, output).render_size).collect();
        assert_eq!(sizes, [UVec2::new(1920, 1080), UVec2::new(1280, 720), UVec2::new(1129, 635), UVec2::new(960, 540), UVec2::new(640, 360)]);
        for quality in UpscaleQuality::ALL {
            let p = policy(quality, output);
            assert_eq!((p.jitter_phases, p.mip_bias), (0, 0.0), "{quality:?}: no jitter, no bias");
            assert_eq!(p.render_size, quality.render_size(output));
        }
        assert_eq!(policy(UpscaleQuality::UltraPerformance, UVec2::new(2, 2)).render_size, UVec2::ONE, "never empty");
    }

    /// The stretch says what it draws on a prepared view's first frame and again each time its
    /// picture turns HDR or 8-bit (the glow switched), not every frame.
    #[test]
    fn the_stretch_says_each_picture_format_it_turns_to() {
        let said = AtomicU8::new(0);
        let frames = [false, false, true, true, false, false, true];
        let says: Vec<bool> = frames.iter().map(|hdr| says_drawing(&said, *hdr)).collect();
        assert_eq!(says, [true, false, true, false, true, false, true]);
        assert!(says_drawing(&AtomicU8::new(0), true), "a new prepared view (switched on, resized) says it at once");
    }

    /// The depth is expanded only for a pass that reads it after the stretch: none at the Low
    /// preset's settings without a blurring modifier, each reader alone enough.
    #[test]
    fn the_depth_is_expanded_only_for_a_pass_that_reads_it() {
        assert!(!DepthReaders::default().any(), "nothing reads it: no expansion");
        for readers in [
            DepthReaders { blur: true, ..default() },
            DepthReaders { shafts: true, ..default() },
            DepthReaders { eye_mask: true, ..default() },
            DepthReaders { ao_after: true, ..default() },
        ] {
            assert!(readers.any(), "{readers:?}");
        }
        // The grade asks for the blur passes with a smear, a blur or a depth of field.
        let mut grade = crate::imagespace::ImageSpaceGrade::default();
        assert!(!grade.blurs(), "the default grade blurs nothing");
        grade.blur = 0.5;
        assert!(grade.blurs());
        let mut grade = crate::imagespace::ImageSpaceGrade::default();
        grade.motion.x = 1.0;
        assert!(grade.blurs(), "a motion smear");
        let mut grade = crate::imagespace::ImageSpaceGrade::default();
        grade.dof.x = 1.0;
        assert!(grade.blurs(), "a depth of field");
        // The shafts read it only while the sun is on the screen.
        assert!(!crate::shafts::LightShafts::default().draws());
        assert!(crate::shafts::LightShafts { sun: Vec4::new(0.5, 0.5, 1.0, 1.7), ..default() }.draws());
    }
}
