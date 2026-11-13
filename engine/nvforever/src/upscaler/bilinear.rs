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

use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::prelude::*;
use bevy::render::{
    diagnostic::RecordDiagnostics,
    renderer::{RenderContext, RenderDevice, ViewQuery},
    view::ViewTarget,
};
use std::sync::atomic::Ordering;

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

/// Bilinear's pass for a view it draws: the render rectangle stretched to the display size
/// (`prep::Pipelines::stretch`, with the scene's alpha restored where a shape marks it), then
/// the depth prepass expanded to the display size for the screen-space effects after it, as
/// FSR and DLSS do. Nothing is reconstructed, so the prepared view's history stays invalid
/// (DLSS frame generation and FSR's frame generation never take a Bilinear view).
pub(crate) fn bilinear_pass(
    view: ViewQuery<(&ViewTarget, &PreparedUpscale, Option<&ViewPrepassTextures>)>,
    upscale: Res<UpscaleRenderer>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, prepared, prepass) = view.into_inner();
    let UpscaleSettings::Bilinear(_) = prepared.camera.settings else { return };
    let Some(common) = &upscale.0 else { return };
    let device = device.wgpu_device();
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_bilinear");
    let encoder = ctx.command_encoder();
    prepared.history_valid.store(false, Ordering::Relaxed);
    common.stretch(device, encoder, prepared, target);
    if !prepared.drawing_said.swap(true, Ordering::Relaxed) {
        let (render, output) = (prepared.render_size, prepared.output_size);
        info!("Bilinear stretch active: {}x{} -> {}x{}", render.x, render.y, output.x, output.y);
    }
    if let Some(depth) = prepass.and_then(|prepass| prepass.depth.as_ref()) {
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
}
