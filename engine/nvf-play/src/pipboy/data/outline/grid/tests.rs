use super::*;
use crate::pipboy::data::outline::Outline;

fn request() -> Request {
    Request { interior: Some(1), world: 7, centre: [0.0, 0.0], feet: 0.0, window: Vec2::new(100.0, 80.0), scale: 8.0, basis: Basis::WorldNorth }
}

fn limits() -> Limits {
    Limits { max_edge: 2048, max_pixels: 2048 * 2048, max_payload_bytes: 128 * 1024 * 1024 }
}

fn assert_frame_bits(g: OutlineGrid, old: &Outline) {
    assert_eq!(g.origin().map(f32::to_bits), old.origin.map(f32::to_bits));
    let old_span = Vec2::new(old.size[0] as f32, old.size[1] as f32) * old.step;
    assert_eq!(g.span().to_array().map(f32::to_bits), old_span.to_array().map(f32::to_bits));
}

#[test]
fn an_asymmetric_frame_records_requested_and_effective_extents_separately() {
    let checked = OutlineGrid::checked(request(), limits()).unwrap();
    let g = checked.grid();
    assert_eq!(g.key(), (Some(1), 7, [0, 0, 0]));
    assert_eq!(g.extent(), Vec2::new(920.0, 736.0));
    assert_eq!(g.step(), 28.0);
    assert_eq!(g.size(), [33, 27]);
    assert_eq!(g.origin(), [-462.0, 378.0]);
    assert_eq!(g.span(), Vec2::new(924.0, 756.0));
    assert_eq!(g.feet(), 0.0);
    let p = checked.payload();
    assert_eq!(p.pixels, 891);
    assert_eq!(p.heights, 891 * std::mem::size_of::<Option<f32>>() as u64);
    assert_eq!(p.alpha, 891);
    assert_eq!(p.rgba, 3564);
    assert_eq!(p.total, p.heights + p.alpha + p.rgba);
}

#[test]
fn the_effective_frame_registers_an_authored_l_room_and_raised_table_north_up() {
    let r = request();
    let g = OutlineGrid::checked(r, limits()).unwrap().grid();
    let mut o = Outline::new(r.interior, r.world, r.centre, r.feet, r.window, r.scale);
    o.advance(891, |[x, y, z], reach| {
        assert_eq!((z, reach), (150.0, 450.0));
        let main_room = (-280.0..=280.0).contains(&x) && (-196.0..=196.0).contains(&y);
        let north_arm = (0.0..=168.0).contains(&x) && (196.0..=336.0).contains(&y);
        let table = (84.0..=140.0).contains(&x) && (56.0..=112.0).contains(&y);
        (main_room || north_arm).then_some(if table { 80.0 } else { 0.0 })
    });
    assert!(o.done());
    assert_eq!((g.origin(), g.size()), (o.origin, o.size));
    let at = |x: usize, y: usize| o.heights[y * 33 + x];
    assert_eq!(at(16, 13), Some(0.0), "center at world (0,0)");
    assert_eq!(at(16, 4), Some(0.0), "north arm at world (0,252)");
    assert_eq!(at(24, 13), Some(0.0), "east room at world (224,0)");
    assert_eq!(at(8, 4), None, "north-west corner outside the L at world (-224,252)");
    assert_eq!(at(20, 10), Some(80.0), "table at world (112,84)");
}

#[test]
fn halfpoint_anchors_round_away_from_zero_in_both_axes_and_floor() {
    for (centre, feet, anchor, origin) in [
        ([115.0, 92.0], 64.0, [1, 1, 1], [-232.0, 562.0]),
        ([-115.0, -92.0], -64.0, [-1, -1, -1], [-692.0, 194.0]),
        ([114.0, -91.0], 63.0, [0, 0, 0], [-462.0, 378.0]),
        ([-116.0, 93.0], -65.0, [-1, 1, -1], [-692.0, 562.0]),
        ([-0.0, 0.0], -0.0, [0, 0, 0], [-462.0, 378.0]),
    ] {
        let r = Request { centre, feet, ..request() };
        let g = OutlineGrid::checked(r, limits()).unwrap().grid();
        assert_eq!(g.key().2, anchor);
        assert_eq!(g.origin(), origin);
        assert_eq!(g.feet().to_bits(), feet.to_bits(), "actual ray height remains distinct from the floor bucket");
    }
}

#[test]
fn width_height_scale_and_zoom_change_the_actual_sampling_frame() {
    for (window, scale, size, origin) in [
        (Vec2::new(200.0, 80.0), 8.0, [66, 27], [-924.0, 378.0]),
        (Vec2::new(100.0, 160.0), 8.0, [33, 53], [-462.0, 742.0]),
        (Vec2::new(100.0, 80.0), 16.0, [66, 53], [-924.0, 742.0]),
        (Vec2::new(100.0, 80.0), 4.0, [17, 14], [-238.0, 196.0]),
    ] {
        let g = OutlineGrid::checked(Request { window, scale, ..request() }, limits()).unwrap().grid();
        assert_eq!(g.size(), size);
        assert_eq!(g.origin(), origin);
    }
    let a = OutlineGrid::checked(request(), limits()).unwrap().grid();
    let b = OutlineGrid::checked(Request { window: request().window * 2.0, scale: 4.0, ..request() }, limits()).unwrap().grid();
    assert_eq!(a, b, "equivalent f32 products retain the same complete frame");
}

#[test]
fn the_exterior_minimum_is_an_effective_frame_not_a_requested_resolution() {
    let r = Request { interior: None, window: Vec2::new(10.0, 20.0), scale: 1.0, ..request() };
    let g = OutlineGrid::checked(r, limits()).unwrap().grid();
    assert_eq!(g.extent(), Vec2::splat(23552.0));
    assert_eq!(g.size(), [246, 246]);
    assert_eq!(g.step(), 96.0);
    assert_eq!(g.origin(), [-11808.0, 11808.0]);
    assert_eq!(g.span(), Vec2::splat(23616.0));
    assert_eq!(g, OutlineGrid::checked(Request { window: Vec2::new(30.0, 40.0), scale: 2.0, ..r }, limits()).unwrap().grid());
}

#[test]
fn checked_frames_match_the_actual_ordinary_outline_including_negative_coordinates() {
    for r in [
        request(),
        Request { centre: [-115.0, -92.0], feet: -64.0, ..request() },
        Request { centre: [-12345.75, 23456.125], feet: 123.25, window: Vec2::new(777.25, 441.5), scale: 28.0, ..request() },
        Request { interior: None, centre: [-5000.5, -10000.25], feet: -1024.0, ..request() },
    ] {
        let g = OutlineGrid::checked(r, limits()).unwrap().grid();
        let old = Outline::new(r.interior, r.world, r.centre, r.feet, r.window, r.scale);
        assert_eq!(g.key(), old.key);
        assert_frame_bits(g, &old);
        assert_eq!(g.size(), old.size);
        assert_eq!(g.step().to_bits(), old.step.to_bits());
        assert_eq!(g.feet().to_bits(), old.feet.to_bits());
        let (origin, span) = Outline::frame_for(r.interior, r.world, r.centre, r.feet, r.window, r.scale);
        assert_eq!(g.origin().map(f32::to_bits), origin.map(f32::to_bits));
        assert_eq!(g.span().to_array().map(f32::to_bits), span.to_array().map(f32::to_bits));
        // The unchanged f30 key deliberately remains only its three original fields.
        let (interior, world, _) = g.key();
        assert_eq!((interior, world), (r.interior, r.world));
    }
}

#[test]
fn nonfinite_nonpositive_and_underflowed_requests_are_refused() {
    for r in [
        Request { window: Vec2::new(f32::NAN, 80.0), ..request() },
        Request { window: Vec2::new(100.0, f32::INFINITY), ..request() },
        Request { window: Vec2::new(0.0, 80.0), ..request() },
        Request { window: Vec2::new(100.0, -1.0), ..request() },
        Request { scale: f32::NAN, ..request() },
        Request { scale: 0.0, ..request() },
        Request { scale: -1.0, ..request() },
        Request { centre: [f32::INFINITY, 0.0], ..request() },
        Request { feet: f32::NAN, ..request() },
    ] {
        assert_eq!(OutlineGrid::checked(r, limits()).unwrap_err(), Rejected::InvalidInput);
    }
    for r in [
        Request { window: Vec2::splat(f32::MAX), scale: 2.0, ..request() },
        Request { window: Vec2::splat(f32::from_bits(1)), scale: f32::from_bits(1), ..request() },
    ] {
        assert_eq!(OutlineGrid::checked(r, limits()).unwrap_err(), Rejected::InvalidExtent);
    }
}

#[test]
fn unrepresentable_anchors_dimensions_and_payload_arithmetic_fail_before_allocation() {
    for r in [Request { centre: [f32::MAX, 0.0], ..request() }, Request { feet: f32::MAX, ..request() }] {
        assert_eq!(OutlineGrid::checked(r, limits()).unwrap_err(), Rejected::AnchorOutOfRange);
    }
    let unlimited = Limits { max_edge: u32::MAX, max_pixels: u64::MAX, max_payload_bytes: u64::MAX };
    let too_wide = Request { window: Vec2::splat(1e20), scale: 1.0, ..request() };
    assert_eq!(OutlineGrid::checked(too_wide, unlimited).unwrap_err(), Rejected::DimensionsOutOfRange);
    let byte_overflow = Request { window: Vec2::splat(1e11), scale: 1.0, ..request() };
    assert_eq!(OutlineGrid::checked(byte_overflow, unlimited).unwrap_err(), Rejected::ArithmeticOverflow);
}

#[test]
fn i32_float_cast_boundaries_reject_positive_overflow_and_accept_exact_negative_minimum() {
    // The fixed authored frame has snap X=230. The cast rounds i32::MAX up to
    // +2^31, while i32::MIN is exactly representable. No large grid is allocated.
    assert_eq!(i32::MAX as f32, 2147483648.0);
    assert_eq!(i32::MIN as f32, -2147483648.0);
    let upper = 493921239040.0_f32;
    assert_eq!((upper / 230.0).round(), 2147483648.0);
    let at_upper = Request { centre: [upper, 0.0], ..request() };
    let saturated = Outline::sized(at_upper.interior, at_upper.world, at_upper.centre, at_upper.feet, at_upper.window, at_upper.scale);
    assert_eq!(saturated.key.2[0], i32::MAX, "the legacy cast alone masks the positive overflow");
    assert_eq!(OutlineGrid::checked(at_upper, limits()).unwrap_err(), Rejected::AnchorOutOfRange);

    let before_upper = f32::from_bits(upper.to_bits() - 1);
    assert_eq!((before_upper / 230.0).round(), 2147483520.0);
    let valid = Request { centre: [before_upper, 0.0], ..request() };
    let g = OutlineGrid::checked(valid, limits()).unwrap().grid();
    assert_eq!(g.key().2[0], 2147483520);
    let old = Outline::sized(valid.interior, valid.world, valid.centre, valid.feet, valid.window, valid.scale);
    assert_frame_bits(g, &old);

    let lower = -upper;
    assert_eq!((lower / 230.0).round(), -2147483648.0);
    let exact_min = Request { centre: [lower, 0.0], ..request() };
    let g = OutlineGrid::checked(exact_min, limits()).unwrap().grid();
    assert_eq!(g.key().2[0], i32::MIN);
    let old = Outline::sized(exact_min.interior, exact_min.world, exact_min.centre, exact_min.feet, exact_min.window, exact_min.scale);
    assert_frame_bits(g, &old);
    let below_lower = f32::from_bits(lower.to_bits() + 1);
    assert_eq!((below_lower / 230.0).round(), -2147483904.0);
    assert_eq!(OutlineGrid::checked(Request { centre: [below_lower, 0.0], ..request() }, limits()).unwrap_err(), Rejected::AnchorOutOfRange);
}

#[test]
#[cfg(target_pointer_width = "64")]
fn u32_float_dimension_boundary_is_rejected_but_the_adjacent_input_is_admitted() {
    // Independent input/kernel observation: with scale1, margin1.15 and step28,
    // this exact f32 width reaches +2^32 samples. The adjacent lower width reaches
    // 2^32-512 samples. All other caps are explicitly generous, no allocation.
    assert_eq!(u32::MAX as f32, 4294967296.0);
    let upper_window = 104573116416.0_f32;
    let r = Request { window: Vec2::new(upper_window, 1.0), scale: 1.0, ..request() };
    let old = Outline::sized(r.interior, r.world, r.centre, r.feet, r.window, r.scale);
    assert_eq!(old.size, [4294967296, 1], "legacy no-allocation kernel reaches the intended dimension boundary");
    let generous = Limits { max_edge: u32::MAX, max_pixels: u64::MAX, max_payload_bytes: u64::MAX };
    assert_eq!(OutlineGrid::checked(r, generous).unwrap_err(), Rejected::DimensionsOutOfRange);

    let neighbor = Request { window: Vec2::new(f32::from_bits(upper_window.to_bits() - 1), 1.0), ..r };
    let old = Outline::sized(neighbor.interior, neighbor.world, neighbor.centre, neighbor.feet, neighbor.window, neighbor.scale);
    assert_eq!(old.size, [4294966784, 1]);
    let checked = OutlineGrid::checked(neighbor, generous).unwrap();
    assert_eq!(checked.grid().size(), old.size);
    assert_eq!(checked.payload().pixels, 4294966784);
    assert_frame_bits(checked.grid(), &old);
}

#[test]
fn caller_edge_pixel_and_payload_caps_are_checked_with_exact_boundaries() {
    let r = request();
    let p = OutlineGrid::checked(r, limits()).unwrap().payload();
    assert_eq!(OutlineGrid::checked(r, Limits { max_edge: 32, ..limits() }).unwrap_err(), Rejected::PixelLimit);
    assert_eq!(OutlineGrid::checked(r, Limits { max_pixels: p.pixels - 1, ..limits() }).unwrap_err(), Rejected::PixelLimit);
    assert_eq!(OutlineGrid::checked(r, Limits { max_payload_bytes: p.total - 1, ..limits() }).unwrap_err(), Rejected::PayloadLimit);
    assert!(OutlineGrid::checked(r, Limits { max_edge: 33, max_pixels: p.pixels, max_payload_bytes: p.total }).is_ok());
    assert_eq!(OutlineGrid::checked(r, Limits { max_pixels: 0, ..limits() }).unwrap_err(), Rejected::InvalidLimits);
}

#[test]
fn orientation_is_explicitly_unavailable_instead_of_ignored() {
    for yaw in [0.0, 1.0, f32::NAN] {
        let r = Request { basis: Basis::Oriented(yaw), ..request() };
        assert_eq!(OutlineGrid::checked(r, limits()).unwrap_err(), Rejected::UnsupportedOrientation);
    }
}
