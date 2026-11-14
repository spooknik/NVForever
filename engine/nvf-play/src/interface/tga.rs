//! Bounded UI true-color TGA policy (ui-tga-content.md), without codec features.

const MAX_PIXELS: usize = 4_000_000;

pub(super) struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// TGA has no unique leading magic. A complete header with a supported type/depth
/// pair is a candidate; decode validates the remaining fields and payload. DDS
/// magic is kept on the DDS path by the caller.
pub(super) fn recognizable(bytes: &[u8]) -> bool {
    bytes.len() >= 18 && matches!(bytes[2], 2 | 10) && matches!(bytes[16], 24 | 32)
}

fn error(message: &str) -> String {
    format!("TGA UI image: {message}")
}

pub(super) fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    if bytes.len() < 18 {
        return Err(error("truncated header"));
    }
    if !matches!(bytes[2], 2 | 10) || !matches!(bytes[16], 24 | 32) {
        return Err(error("unsupported image type or pixel depth"));
    }
    if bytes[1] != 0 || bytes[3..8].iter().any(|&v| v != 0) {
        return Err(error("color maps are unsupported"));
    }
    let descriptor = bytes[17];
    if descriptor & 0xC0 != 0 {
        return Err(error("interleaved images are unsupported"));
    }
    let pixel_bytes = usize::from(bytes[16] / 8);
    if !matches!((pixel_bytes, descriptor & 15), (3, 0) | (4, 0 | 8)) {
        return Err(error("unsupported attribute-bit declaration"));
    }
    let width = usize::from(u16::from_le_bytes([bytes[12], bytes[13]]));
    let height = usize::from(u16::from_le_bytes([bytes[14], bytes[15]]));
    let pixels = width
        .checked_mul(height)
        .ok_or_else(|| error("dimension overflow"))?;
    if pixels == 0 || pixels > MAX_PIXELS {
        return Err(error("zero dimensions or decoded pixel limit exceeded"));
    }
    let rgba_bytes = pixels
        .checked_mul(4)
        .ok_or_else(|| error("decoded byte count overflow"))?;
    let mut at = 18usize
        .checked_add(usize::from(bytes[0]))
        .ok_or_else(|| error("identifier overflow"))?;
    if at > bytes.len() {
        return Err(error("truncated identifier"));
    }
    let mut rgba = vec![0; rgba_bytes];
    if bytes[2] == 2 {
        let count = pixels
            .checked_mul(pixel_bytes)
            .ok_or_else(|| error("pixel byte count overflow"))?;
        let end = at
            .checked_add(count)
            .ok_or_else(|| error("pixel payload overflow"))?;
        let raw = bytes
            .get(at..end)
            .ok_or_else(|| error("truncated uncompressed pixels"))?;
        for (index, pixel) in raw.chunks_exact(pixel_bytes).enumerate() {
            write_pixel(&mut rgba, index, pixel, width, height, descriptor);
        }
    } else {
        let mut written = 0;
        while written < pixels {
            let packet = *bytes.get(at).ok_or_else(|| error("truncated RLE packet"))?;
            at += 1;
            let count = usize::from(packet & 127) + 1;
            if count > pixels - written {
                return Err(error("RLE packet exceeds remaining pixels"));
            }
            let values = if packet & 128 != 0 { 1 } else { count };
            let needed = values
                .checked_mul(pixel_bytes)
                .ok_or_else(|| error("RLE byte count overflow"))?;
            let end = at
                .checked_add(needed)
                .ok_or_else(|| error("RLE payload overflow"))?;
            let raw = bytes
                .get(at..end)
                .ok_or_else(|| error("truncated RLE pixels"))?;
            if packet & 128 != 0 {
                for index in written..written + count {
                    write_pixel(&mut rgba, index, raw, width, height, descriptor);
                }
            } else {
                for (offset, pixel) in raw.chunks_exact(pixel_bytes).enumerate() {
                    write_pixel(
                        &mut rgba,
                        written + offset,
                        pixel,
                        width,
                        height,
                        descriptor,
                    );
                }
            }
            at = end;
            written += count;
        }
    }
    Ok(Decoded {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}

fn write_pixel(
    rgba: &mut [u8],
    index: usize,
    pixel: &[u8],
    width: usize,
    height: usize,
    descriptor: u8,
) {
    // All dimensions, pixel slices and output counts were checked before this call.
    let mut x = index % width;
    let mut y = index / width;
    if descriptor & 16 != 0 {
        x = width - 1 - x;
    }
    if descriptor & 32 == 0 {
        y = height - 1 - y;
    }
    let target = (y * width + x) * 4;
    let alpha = if pixel.len() == 4 { pixel[3] } else { 255 };
    rgba[target..target + 4].copy_from_slice(&[pixel[2], pixel[1], pixel[0], alpha]);
}

#[cfg(test)]
#[path = "tga_tests.rs"]
mod tests;
