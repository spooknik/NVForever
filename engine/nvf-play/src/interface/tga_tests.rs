use super::*;

fn fixture(
    kind: u8,
    width: u16,
    height: u16,
    bits: u8,
    descriptor: u8,
    id: &[u8],
    payload: &[u8],
) -> Vec<u8> {
    let mut bytes = vec![
        u8::try_from(id.len()).unwrap(),
        0,
        kind,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    bytes.extend([bits, descriptor]);
    bytes.extend(id);
    bytes.extend(payload);
    bytes
}

const BGRA: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
const RGBA: [u8; 16] = [3, 2, 1, 4, 7, 6, 5, 8, 11, 10, 9, 12, 15, 14, 13, 16];

#[test]
fn colored_raw_and_rle_pixels_use_rgba_and_preserve_declared_or_undeclared_fourth_bytes() {
    for descriptor in [32, 40] {
        let raw = fixture(2, 2, 2, 32, descriptor, &[], &BGRA);
        assert_eq!(decode(&raw).unwrap().rgba, RGBA);
        let rle = fixture(
            10,
            2,
            2,
            32,
            descriptor,
            &[],
            &[vec![3], BGRA.to_vec()].concat(),
        );
        assert_eq!(decode(&rle).unwrap().rgba, RGBA);
    }
    let rgba = decode(&fixture(
        2,
        2,
        1,
        32,
        32,
        &[],
        &[9, 17, 31, 0, 10, 18, 32, 204],
    ))
    .unwrap();
    assert_eq!(rgba.rgba, [31, 17, 9, 0, 32, 18, 10, 204]);
}

#[test]
fn all_four_origins_identifier_bytes_and_absolute_origin_metadata_are_separate() {
    for (descriptor, order) in [
        (32, [0, 1, 2, 3]),
        (0, [2, 3, 0, 1]),
        (48, [1, 0, 3, 2]),
        (16, [3, 2, 1, 0]),
    ] {
        let mut bytes = fixture(
            10,
            2,
            2,
            32,
            descriptor,
            b"owned-id",
            &[vec![3], BGRA.to_vec()].concat(),
        );
        bytes[8..10].copy_from_slice(&123u16.to_le_bytes());
        bytes[10..12].copy_from_slice(&456u16.to_le_bytes());
        let decoded = decode(&bytes).unwrap();
        let expected: Vec<u8> = order
            .into_iter()
            .flat_map(|i| RGBA[i * 4..i * 4 + 4].iter().copied())
            .collect();
        assert_eq!((decoded.width, decoded.height), (2, 2));
        assert_eq!(decoded.rgba, expected);
    }
}

#[test]
fn truecolor24_is_opaque_and_rle_packets_cover_exactly_the_remaining_pixels() {
    for kind in [2, 10] {
        let pixels = [9, 17, 31, 10, 18, 32];
        let payload = if kind == 10 {
            [vec![1], pixels.to_vec()].concat()
        } else {
            pixels.to_vec()
        };
        assert_eq!(
            decode(&fixture(kind, 2, 1, 24, 32, &[], &payload))
                .unwrap()
                .rgba,
            [31, 17, 9, 255, 32, 18, 10, 255]
        );
    }
    let decoded = decode(&fixture(
        10,
        129,
        1,
        32,
        32,
        &[],
        &[255, 1, 2, 3, 4, 0, 5, 6, 7, 8],
    ))
    .unwrap();
    assert_eq!(decoded.rgba.len(), 129 * 4);
    assert!(decoded.rgba[..128 * 4]
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| *p == [3, 2, 1, 4]));
    assert_eq!(&decoded.rgba[128 * 4..], &[7, 6, 5, 8]);
    let raw_packet = [vec![127], [1, 2, 3, 4].repeat(128)].concat();
    assert_eq!(
        decode(&fixture(10, 128, 1, 32, 32, &[], &raw_packet))
            .unwrap()
            .rgba,
        [3, 2, 1, 4].repeat(128)
    );
}

#[test]
fn truncated_headers_identifiers_pixels_and_overrunning_packets_are_errors() {
    let complete = fixture(2, 2, 2, 32, 32, &[], &BGRA);
    for end in 0..18 {
        assert!(decode(&complete[..end]).is_err());
    }
    let mut missing_id = fixture(2, 1, 1, 32, 32, &[], &[]);
    missing_id[0] = 20;
    assert!(decode(&missing_id)
        .err()
        .expect("truncated identifier")
        .contains("identifier"));
    for bytes in [
        fixture(2, 2, 2, 32, 32, &[], &BGRA[..15]),
        fixture(10, 2, 2, 32, 32, &[], &[]),
        fixture(10, 2, 2, 32, 32, &[], &[131, 1, 2, 3]),
        fixture(
            10,
            2,
            2,
            32,
            32,
            &[],
            &[vec![3], BGRA[..15].to_vec()].concat(),
        ),
        fixture(10, 2, 2, 32, 32, &[], &[132, 1, 2, 3, 4]),
    ] {
        assert!(decode(&bytes).is_err());
    }
}

#[test]
fn unsupported_metadata_and_over_cap_dimensions_never_produce_a_placeholder() {
    for (offset, value) in [(1, 1), (3, 1), (5, 1), (7, 24), (17, 64), (17, 32 | 7)] {
        let mut bytes = fixture(2, 1, 1, 32, 32, &[], &[1, 2, 3, 4]);
        bytes[offset] = value;
        assert!(decode(&bytes).is_err(), "metadata at {offset}");
    }
    assert!(decode(&fixture(2, 1, 1, 24, 40, &[], &[1, 2, 3])).is_err());
    for (w, h) in [(0, 1), (1, 0), (2000, 2001), (u16::MAX, u16::MAX)] {
        assert!(decode(&fixture(10, w, h, 32, 32, &[], &[]))
            .err()
            .expect("invalid dimensions")
            .contains("pixel limit"));
    }
    assert!(!recognizable(b"DDS "));
    assert!(!recognizable(&fixture(1, 1, 1, 32, 32, &[], &[1, 2, 3, 4])));
}

#[test]
fn the_exact_pixel_cap_accepts_bounded_runs_and_trailing_metadata_is_not_reinterpreted() {
    let mut payload = Vec::new();
    for _ in 0..MAX_PIXELS / 128 {
        payload.extend([255, 1, 2, 3, 4]);
    }
    let decoded = decode(&fixture(10, 2000, 2000, 32, 32, &[], &payload)).unwrap();
    assert_eq!(decoded.rgba.len(), MAX_PIXELS * 4);
    assert_eq!(&decoded.rgba[..4], &[3, 2, 1, 4]);
    assert_eq!(&decoded.rgba[decoded.rgba.len() - 4..], &[3, 2, 1, 4]);
    let mut one = fixture(2, 1, 1, 24, 32, &[], &[1, 2, 3]);
    one.extend(b"uninterpreted trailing metadata");
    assert_eq!(decode(&one).unwrap().rgba, [3, 2, 1, 255]);
}
