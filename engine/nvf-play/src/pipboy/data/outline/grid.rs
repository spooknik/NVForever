//! Scalar-only grid derivation and checked admission prerequisite.
//! [design: docs/spec-notes/local-map-checked-grid.md] No recipient/provider API.

use super::{extent, EXTERIOR_STEP, FEET_SNAP, INTERIOR_STEP, OutlineKey};
use bevy::prelude::Vec2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Basis {
    WorldNorth,
    Oriented(f32),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Request {
    pub interior: Option<u32>,
    pub world: u32,
    pub centre: [f32; 2],
    pub feet: f32,
    pub window: Vec2,
    pub scale: f32,
    pub basis: Basis,
}

/// Explicit caller limits, not defaults or a claim about total engine/GPU memory.
#[derive(Clone, Copy, Debug)]
pub(super) struct Limits {
    pub max_edge: u32,
    pub max_pixels: u64,
    pub max_payload_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Rejected {
    UnsupportedOrientation,
    InvalidLimits,
    InvalidInput,
    InvalidExtent,
    AnchorOutOfRange,
    DimensionsOutOfRange,
    ArithmeticOverflow,
    PixelLimit,
    PayloadLimit,
}

/// Requested payloads only: allocator overhead/capacity and GPU uploads are separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Payload {
    pub pixels: u64,
    pub heights: u64,
    pub alpha: u64,
    pub rgba: u64,
    pub total: u64,
}

/// Complete effective frame; private fields prevent callers editing a checked grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct OutlineGrid {
    key: OutlineKey,
    extent: Vec2,
    origin: [f32; 2],
    step: f32,
    size: [usize; 2],
    feet: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CheckedGrid {
    grid: OutlineGrid,
    payload: Payload,
}

impl CheckedGrid {
    pub fn grid(self) -> OutlineGrid { self.grid }
    pub fn payload(self) -> Payload { self.payload }
}

impl OutlineGrid {
    /// Existing ordinary caller's valid-input arithmetic, in its original f32 order.
    pub fn legacy(interior: Option<u32>, world: u32, centre: [f32; 2], feet: f32, window: Vec2, scale: f32) -> Self {
        let step = if interior.is_some() { INTERIOR_STEP } else { EXTERIOR_STEP };
        let extent = extent(interior.is_some(), window, scale);
        let snap = extent / 4.0;
        let anchor = [(centre[0] / snap.x).round() as i32, (centre[1] / snap.y).round() as i32, (feet / FEET_SNAP).round() as i32];
        let mid = [anchor[0] as f32 * snap.x, anchor[1] as f32 * snap.y];
        let size = [(extent.x / step).ceil() as usize, (extent.y / step).ceil() as usize];
        Self {
            key: (interior, world, anchor),
            extent,
            origin: [mid[0] - size[0] as f32 * step / 2.0, mid[1] + size[1] as f32 * step / 2.0],
            step,
            size,
            feet,
        }
    }

    /// Validates all inputs, conversions and prospective payloads without allocation.
    /// No arbitrary output resolution or oriented sampler is admitted here.
    pub fn checked(r: Request, limits: Limits) -> Result<CheckedGrid, Rejected> {
        if r.basis != Basis::WorldNorth { return Err(Rejected::UnsupportedOrientation); }
        if limits.max_edge == 0 || limits.max_pixels == 0 || limits.max_payload_bytes == 0 {
            return Err(Rejected::InvalidLimits);
        }
        if !r.window.is_finite() || r.window.min_element() <= 0.0 || !r.scale.is_finite() || r.scale <= 0.0
            || !r.centre.into_iter().all(f32::is_finite) || !r.feet.is_finite() {
            return Err(Rejected::InvalidInput);
        }
        // Check the product before the exterior minimum could conceal an overflow.
        let requested = r.window * r.scale * super::MARGIN;
        if !requested.is_finite() || requested.min_element() <= 0.0 { return Err(Rejected::InvalidExtent); }
        let extent = extent(r.interior.is_some(), r.window, r.scale);
        let snap = extent / 4.0;
        if !extent.is_finite() || !snap.is_finite() || snap.min_element() <= 0.0 { return Err(Rejected::InvalidExtent); }
        let anchors = [(r.centre[0] / snap.x).round(), (r.centre[1] / snap.y).round(), (r.feet / FEET_SNAP).round()];
        if anchors.into_iter().any(|a| !a.is_finite() || f64::from(a) < f64::from(i32::MIN) || f64::from(a) > f64::from(i32::MAX)) {
            return Err(Rejected::AnchorOutOfRange);
        }
        let step = if r.interior.is_some() { INTERIOR_STEP } else { EXTERIOR_STEP };
        let dimensions = [(extent.x / step).ceil(), (extent.y / step).ceil()];
        if dimensions.into_iter().any(|n| !n.is_finite() || n < 1.0 || f64::from(n) > f64::from(u32::MAX)) {
            return Err(Rejected::DimensionsOutOfRange);
        }
        let size = dimensions.map(|n| n as u32);
        if size.into_iter().any(|n| n > limits.max_edge) { return Err(Rejected::PixelLimit); }
        let pixels = u64::from(size[0]).checked_mul(u64::from(size[1])).ok_or(Rejected::ArithmeticOverflow)?;
        if pixels > limits.max_pixels { return Err(Rejected::PixelLimit); }
        let heights = pixels.checked_mul(std::mem::size_of::<Option<f32>>() as u64).ok_or(Rejected::ArithmeticOverflow)?;
        let alpha = pixels;
        let rgba = pixels.checked_mul(4).ok_or(Rejected::ArithmeticOverflow)?;
        let total = heights.checked_add(alpha).and_then(|n| n.checked_add(rgba)).ok_or(Rejected::ArithmeticOverflow)?;
        // Vec allocation and image dimensions must fit the current target too.
        if heights > isize::MAX as u64 || rgba > isize::MAX as u64 || alpha > isize::MAX as u64
            || usize::try_from(pixels).is_err() {
            return Err(Rejected::ArithmeticOverflow);
        }
        if total > limits.max_payload_bytes { return Err(Rejected::PayloadLimit); }
        let grid = Self::legacy(r.interior, r.world, r.centre, r.feet, r.window, r.scale);
        if !grid.origin.into_iter().all(f32::is_finite) || !grid.span().is_finite() { return Err(Rejected::InvalidExtent); }
        Ok(CheckedGrid { grid, payload: Payload { pixels, heights, alpha, rgba, total } })
    }

    pub fn key(self) -> OutlineKey { self.key }
    pub fn extent(self) -> Vec2 { self.extent }
    pub fn origin(self) -> [f32; 2] { self.origin }
    pub fn step(self) -> f32 { self.step }
    pub fn size(self) -> [usize; 2] { self.size }
    pub fn feet(self) -> f32 { self.feet }
    pub fn span(self) -> Vec2 { Vec2::new(self.size[0] as f32, self.size[1] as f32) * self.step }
}

#[cfg(test)]
mod tests;
