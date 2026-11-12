//! DDS textures as shipped with Fallout: New Vegas (docs/spec-notes/dds.md).
//!
//! Parses the 128-byte header, names the pixel format, computes the face/mip layout
//! and checks it against the payload size. Block-compressed (DXT) data is meant to be
//! uploaded to the GPU as is; the three uncompressed layouts in the game convert to
//! RGBA8 here, and [`decode_blocks`] decodes DXT1/3/5 on the CPU for the few uses that
//! need the pixels themselves (the mouse pointer).

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DdsError {
    #[error("not a DDS file (magic {0:02x?})")]
    BadMagic([u8; 4]),
    #[error("file too short for a DDS header ({0} bytes)")]
    Truncated(usize),
    #[error("unsupported pixel format {0}")]
    Unsupported(String),
    /// Less pixel data than the layout needs (more is taken, [`Dds::parse`]).
    #[error("pixel data is {actual} bytes, the layout needs {expected}")]
    SizeMismatch { expected: usize, actual: usize },
    #[error("size {width}x{height} out of range (1 to {MAX_SIDE})")]
    BadSize { width: u32, height: u32 },
}

/// The largest side [`Dds::parse`] takes. Above every texture the game ships (4096 at
/// most [observed: dds.md §3]) and the engine's own cap (`nvf_assets::texture::MAX_TEXTURE_SIDE`);
/// a larger side, or 0, is a broken header [#1995].
pub const MAX_SIDE: u32 = 16384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelFormat {
    pub flags: u32,
    pub fourcc: [u8; 4],
    pub rgb_bits: u32,
    /// Red, green, blue, alpha masks.
    pub masks: [u32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Dxt1,
    Dxt3,
    Dxt5,
    A8R8G8B8,
    R8G8B8,
    A4R4G4B4,
    // Layouts no file of the base game uses (dds.md §2.1): a mod's or a localized
    // install's textures may.
    X8R8G8B8,
    A8B8G8R8,
    X8B8G8R8,
    R5G6B5,
    A1R5G5B5,
    X1R5G5B5,
    L8,
    A8L8,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Dxt1 => "DXT1",
            Format::Dxt3 => "DXT3",
            Format::Dxt5 => "DXT5",
            Format::A8R8G8B8 => "A8R8G8B8",
            Format::R8G8B8 => "R8G8B8",
            Format::A4R4G4B4 => "A4R4G4B4",
            Format::X8R8G8B8 => "X8R8G8B8",
            Format::A8B8G8R8 => "A8B8G8R8",
            Format::X8B8G8R8 => "X8B8G8R8",
            Format::R5G6B5 => "R5G6B5",
            Format::A1R5G5B5 => "A1R5G5B5",
            Format::X1R5G5B5 => "X1R5G5B5",
            Format::L8 => "L8",
            Format::A8L8 => "A8L8",
        }
    }

    /// Bytes per 4×4 block for block formats.
    pub fn block_bytes(self) -> Option<usize> {
        match self {
            Format::Dxt1 => Some(8),
            Format::Dxt3 | Format::Dxt5 => Some(16),
            _ => None,
        }
    }

    /// Bytes of one level `w`×`h`, saturating on absurd sizes (a product that would wrap
    /// gave 0, so an empty payload matched a 2-billion-wide header [#1995]).
    pub fn level_size(self, w: u32, h: u32) -> usize {
        let (w, h) = (w as usize, h as usize);
        match self.block_bytes() {
            Some(b) => w.div_ceil(4).max(1).saturating_mul(h.div_ceil(4).max(1)).saturating_mul(b),
            None => {
                let bpp = match self {
                    Format::A8R8G8B8 | Format::X8R8G8B8 | Format::A8B8G8R8 | Format::X8B8G8R8 => 4,
                    Format::R8G8B8 => 3,
                    Format::L8 => 1,
                    _ => 2,
                };
                w.saturating_mul(h).saturating_mul(bpp)
            }
        }
    }
}

const PF_FOURCC: u32 = 0x4;
const PF_RGB: u32 = 0x40;
const PF_ALPHA_PIXELS: u32 = 0x1;
const PF_LUMINANCE: u32 = 0x20000;
const CAPS2_CUBEMAP: u32 = 0x200;
const CAPS2_ALL_FACES: u32 = 0xFC00;

impl PixelFormat {
    /// The format named in dds.md §2, or a description of an unknown one.
    pub fn format(&self) -> Result<Format, String> {
        // The rows of dds.md §2 match on the whole flags value.
        if self.flags == PF_FOURCC {
            return match &self.fourcc {
                b"DXT1" => Ok(Format::Dxt1),
                b"DXT3" => Ok(Format::Dxt3),
                b"DXT5" => Ok(Format::Dxt5),
                _ => Err(self.describe()),
            };
        }
        if self.flags == PF_RGB || self.flags == PF_RGB | PF_ALPHA_PIXELS {
            let alpha = self.flags & PF_ALPHA_PIXELS != 0;
            return match (self.rgb_bits, self.masks, alpha) {
                (32, [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000], true) => Ok(Format::A8R8G8B8),
                (24, [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, _], false) => Ok(Format::R8G8B8),
                (16, [0x0F00, 0x00F0, 0x000F, 0xF000], true) => Ok(Format::A4R4G4B4),
                (32, [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, _], false) => Ok(Format::X8R8G8B8),
                (32, [0x0000_00FF, 0x0000_FF00, 0x00FF_0000, 0xFF00_0000], true) => Ok(Format::A8B8G8R8),
                (32, [0x0000_00FF, 0x0000_FF00, 0x00FF_0000, _], false) => Ok(Format::X8B8G8R8),
                (16, [0xF800, 0x07E0, 0x001F, _], false) => Ok(Format::R5G6B5),
                (16, [0x7C00, 0x03E0, 0x001F, 0x8000], true) => Ok(Format::A1R5G5B5),
                (16, [0x7C00, 0x03E0, 0x001F, _], false) => Ok(Format::X1R5G5B5),
                _ => Err(self.describe()),
            };
        }
        if self.flags == PF_LUMINANCE || self.flags == PF_LUMINANCE | PF_ALPHA_PIXELS {
            let alpha = self.flags & PF_ALPHA_PIXELS != 0;
            return match (self.rgb_bits, self.masks[0], self.masks[3], alpha) {
                (8, 0xFF, _, false) => Ok(Format::L8),
                (16, 0xFF, 0xFF00, true) => Ok(Format::A8L8),
                _ => Err(self.describe()),
            };
        }
        Err(self.describe())
    }

    pub fn describe(&self) -> String {
        format!(
            "unknown(flags=0x{:x},fourcc={},bits={},masks={:08x}/{:08x}/{:08x}/{:08x})",
            self.flags,
            String::from_utf8_lossy(&self.fourcc).replace('\0', ""),
            self.rgb_bits,
            self.masks[0],
            self.masks[1],
            self.masks[2],
            self.masks[3]
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdsHeader {
    pub flags: u32,
    pub height: u32,
    pub width: u32,
    pub pitch_or_linear_size: u32,
    pub depth: u32,
    pub mip_count: u32,
    pub pixel_format: PixelFormat,
    pub caps: [u32; 4],
}

pub const HEADER_LEN: usize = 128;

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

impl DdsHeader {
    pub fn parse(data: &[u8]) -> Result<DdsHeader, DdsError> {
        if data.len() >= 4 && &data[..4] != b"DDS " {
            return Err(DdsError::BadMagic(data[..4].try_into().unwrap()));
        }
        if data.len() < HEADER_LEN {
            return Err(DdsError::Truncated(data.len()));
        }
        let h = |i: usize| u32_at(data, 4 + 4 * i);
        Ok(DdsHeader {
            flags: h(1),
            height: h(2),
            width: h(3),
            pitch_or_linear_size: h(4),
            depth: h(5),
            mip_count: h(6),
            pixel_format: PixelFormat {
                flags: u32_at(data, 80),
                fourcc: data[84..88].try_into().unwrap(),
                rgb_bits: u32_at(data, 88),
                masks: [u32_at(data, 92), u32_at(data, 96), u32_at(data, 100), u32_at(data, 104)],
            },
            caps: [u32_at(data, 108), u32_at(data, 112), u32_at(data, 116), u32_at(data, 120)],
        })
    }

    /// Mip levels in the file: the stored count, or 1 when it is 0 (dds.md §3).
    pub fn levels(&self) -> u32 {
        self.mip_count.max(1)
    }

    pub fn is_cubemap(&self) -> bool {
        self.caps[1] & CAPS2_CUBEMAP != 0
    }

    pub fn faces(&self) -> u32 {
        if self.is_cubemap() {
            (self.caps[1] & CAPS2_ALL_FACES).count_ones().max(1)
        } else {
            1
        }
    }

    /// (width, height) of mip level `i`.
    pub fn level_dims(&self, i: u32) -> (u32, u32) {
        (self.width.checked_shr(i).unwrap_or(0).max(1), self.height.checked_shr(i).unwrap_or(0).max(1))
    }

    /// Payload bytes the layout needs (all faces, all levels), saturating on absurd headers.
    pub fn expected_size(&self, f: Format) -> usize {
        let per_face = (0..self.levels().min(32)).fold(0usize, |acc, i| {
            let (w, h) = self.level_dims(i);
            acc.saturating_add(f.level_size(w, h))
        });
        per_face.saturating_mul(self.faces() as usize)
    }
}

/// A parsed texture with its level slices.
#[derive(Debug, Clone)]
pub struct Dds<'a> {
    pub header: DdsHeader,
    pub format: Format,
    /// `levels[face][mip]` slices of the payload.
    pub levels: Vec<Vec<&'a [u8]>>,
}

impl<'a> Dds<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Dds<'a>, DdsError> {
        let header = DdsHeader::parse(data)?;
        let format = header.pixel_format.format().map_err(DdsError::Unsupported)?;
        // A side of 0 laid out as 1 wide while the header kept 0; one past the cap gave
        // callers that size the image from the header a 2-billion-wide texture [#1995].
        if !(1..=MAX_SIDE).contains(&header.width) || !(1..=MAX_SIDE).contains(&header.height) {
            return Err(DdsError::BadSize { width: header.width, height: header.height });
        }
        let payload = &data[HEADER_LEN..];
        let expected = header.expected_size(format);
        // More data than the layout needs is taken: the levels the header counts are
        // read and the rest is left alone (dds.md §3, #2164). Less is a cut-off file.
        if payload.len() < expected || header.levels() > 32 {
            return Err(DdsError::SizeMismatch { expected, actual: payload.len() });
        }
        let mut levels = Vec::new();
        let mut off = 0;
        for _ in 0..header.faces() {
            let mut face = Vec::new();
            for i in 0..header.levels() {
                let (w, h) = header.level_dims(i);
                let n = format.level_size(w, h);
                face.push(&payload[off..off + n]);
                off += n;
            }
            levels.push(face);
        }
        Ok(Dds { header, format, levels })
    }
}

/// Convert one level of an uncompressed format to RGBA8. `None` for block formats.
pub fn to_rgba8(format: Format, src: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    match format {
        Format::A8R8G8B8 => {
            out.reserve(src.len());
            for p in src.as_chunks::<4>().0 {
                out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        Format::R8G8B8 => {
            out.reserve(src.len() / 3 * 4);
            for p in src.as_chunks::<3>().0 {
                out.extend_from_slice(&[p[2], p[1], p[0], 255]);
            }
        }
        Format::A4R4G4B4 => {
            out.reserve(src.len() * 2);
            for p in src.as_chunks::<2>().0 {
                let v = u16::from_le_bytes([p[0], p[1]]);
                let ch = |shift: u16| (((v >> shift) & 0xF) as u8) * 17;
                out.extend_from_slice(&[ch(8), ch(4), ch(0), ch(12)]);
            }
        }
        Format::X8R8G8B8 => {
            for p in src.as_chunks::<4>().0 {
                out.extend_from_slice(&[p[2], p[1], p[0], 255]);
            }
        }
        Format::A8B8G8R8 => out.extend_from_slice(src),
        Format::X8B8G8R8 => {
            for p in src.as_chunks::<4>().0 {
                out.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        Format::R5G6B5 => {
            for p in src.as_chunks::<2>().0 {
                let [r, g, b] = rgb565(u16::from_le_bytes([p[0], p[1]]));
                out.extend_from_slice(&[r, g, b, 255]);
            }
        }
        Format::A1R5G5B5 | Format::X1R5G5B5 => {
            for p in src.as_chunks::<2>().0 {
                let v = u16::from_le_bytes([p[0], p[1]]);
                let ch = |shift: u16| { let c = ((v >> shift) & 31) as u8; (c << 3) | (c >> 2) };
                let a = if format == Format::X1R5G5B5 || v & 0x8000 != 0 { 255 } else { 0 };
                out.extend_from_slice(&[ch(10), ch(5), ch(0), a]);
            }
        }
        // Luminance is the grey of all three colours, alpha 1 unless stored [documented:
        // MS "Map Direct3D 9 Formats to Direct3D 10", L8 and A8L8 read as R8G8B8A8 with
        // the grey copied].
        Format::L8 => {
            for &l in src {
                out.extend_from_slice(&[l, l, l, 255]);
            }
        }
        Format::A8L8 => {
            for p in src.as_chunks::<2>().0 {
                out.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
            }
        }
        _ => return None,
    }
    Some(out)
}

/// A colour as RGB565, widened to 8 bits a channel.
fn rgb565(v: u16) -> [u8; 3] {
    let (r, g, b) = ((v >> 11) & 31, (v >> 5) & 63, v & 31);
    [((r << 3) | (r >> 2)) as u8, ((g << 2) | (g >> 4)) as u8, ((b << 3) | (b >> 2)) as u8]
}

/// The 16 colours of a DXT colour block (8 bytes), alpha 255 except DXT1's transparent
/// entry. `four_always`: DXT3/5 always use the four-colour mode [documented: Microsoft,
/// "Block Compression (Direct3D 10)", BC1–BC3].
fn color_block(b: &[u8], four_always: bool) -> [[u8; 4]; 16] {
    let (c0, c1) = (u16::from_le_bytes([b[0], b[1]]), u16::from_le_bytes([b[2], b[3]]));
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |a: u8, b: u8, wa: u16, wb: u16| ((u16::from(a) * wa + u16::from(b) * wb) / (wa + wb)) as u8;
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [p0[0], p0[1], p0[2], 255];
    pal[1] = [p1[0], p1[1], p1[2], 255];
    if c0 > c1 || four_always {
        for k in 0..3 {
            pal[2][k] = mix(p0[k], p1[k], 2, 1);
            pal[3][k] = mix(p0[k], p1[k], 1, 2);
        }
        pal[2][3] = 255;
        pal[3][3] = 255;
    } else {
        for k in 0..3 {
            pal[2][k] = mix(p0[k], p1[k], 1, 1);
        }
        pal[2][3] = 255;
        pal[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    std::array::from_fn(|i| pal[((bits >> (2 * i)) & 3) as usize])
}

/// The 16 alphas of a DXT5 alpha block (8 bytes).
fn alpha_block5(b: &[u8]) -> [u8; 16] {
    let (a0, a1) = (u16::from(b[0]), u16::from(b[1]));
    let mut pal = [0u8; 8];
    pal[0] = a0 as u8;
    pal[1] = a1 as u8;
    if a0 > a1 {
        for i in 1..7u16 {
            pal[i as usize + 1] = ((a0 * (7 - i) + a1 * i) / 7) as u8;
        }
    } else {
        for i in 1..5u16 {
            pal[i as usize + 1] = ((a0 * (5 - i) + a1 * i) / 5) as u8;
        }
        pal[6] = 0;
        pal[7] = 255;
    }
    let bits = b[2..8].iter().rev().fold(0u64, |acc, &x| (acc << 8) | u64::from(x));
    std::array::from_fn(|i| pal[((bits >> (3 * i)) & 7) as usize])
}

/// Decode one level of DXT1/3/5 data of `w`×`h` texels to RGBA8 (row-major); `None` for
/// other formats or data too short.
pub fn decode_blocks(format: Format, w: u32, h: u32, src: &[u8]) -> Option<Vec<u8>> {
    let block = format.block_bytes().filter(|_| matches!(format, Format::Dxt1 | Format::Dxt3 | Format::Dxt5))?;
    let (bw, bh) = (w.div_ceil(4) as usize, h.div_ceil(4) as usize);
    if src.len() < bw * bh * block {
        return None;
    }
    let (w, h) = (w as usize, h as usize);
    let mut out = vec![0u8; w * h * 4];
    for by in 0..bh {
        for bx in 0..bw {
            let b = &src[(by * bw + bx) * block..][..block];
            let texels: [[u8; 4]; 16] = match format {
                Format::Dxt1 => color_block(b, false),
                Format::Dxt3 => {
                    let mut c = color_block(&b[8..], true);
                    for (i, t) in c.iter_mut().enumerate() {
                        let nib = (b[i / 2] >> (4 * (i % 2))) & 15;
                        t[3] = nib * 17;
                    }
                    c
                }
                _ => {
                    let a = alpha_block5(b);
                    let mut c = color_block(&b[8..], true);
                    for (t, a) in c.iter_mut().zip(a) {
                        t[3] = a;
                    }
                    c
                }
            };
            for (i, t) in texels.iter().enumerate() {
                let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                if x < w && y < h {
                    out[(y * w + x) * 4..][..4].copy_from_slice(t);
                }
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxt_blocks_decode_to_their_colours_and_alphas() {
        // DXT1: c0 white (0xFFFF) > c1 black: indices 0,1,2,3 across the first row.
        let b = [0xFF, 0xFF, 0x00, 0x00, 0b11_10_01_00, 0, 0, 0];
        let px = decode_blocks(Format::Dxt1, 4, 4, &b).unwrap();
        assert_eq!(&px[0..16], &[255, 255, 255, 255, 0, 0, 0, 255, 170, 170, 170, 255, 85, 85, 85, 255]);
        // c0 <= c1: index 3 is transparent.
        let b = [0x00, 0x00, 0xFF, 0xFF, 0b11, 0, 0, 0];
        assert_eq!(&decode_blocks(Format::Dxt1, 4, 4, &b).unwrap()[0..4], &[0, 0, 0, 0]);
        // DXT5: alpha 255 / 0 with index 1 (0) on texel 0, index 0 (255) elsewhere.
        let mut b = [0u8; 16];
        b[0] = 255;
        b[1] = 0;
        b[2] = 1;
        b[8..10].copy_from_slice(&0xFFFFu16.to_le_bytes());
        let px = decode_blocks(Format::Dxt5, 4, 4, &b).unwrap();
        assert_eq!((px[3], px[7]), (0, 255));
        // DXT3: explicit 4-bit alphas.
        let mut b = [0u8; 16];
        b[0] = 0xF0;
        let px = decode_blocks(Format::Dxt3, 4, 4, &b).unwrap();
        assert_eq!((px[3], px[7]), (0, 255));
        assert!(decode_blocks(Format::A8R8G8B8, 4, 4, &b).is_none());
    }

    #[allow(clippy::too_many_arguments)]
    fn build(w: u32, h: u32, mips: u32, pf_flags: u32, fourcc: &[u8; 4], bits: u32, masks: [u32; 4], caps2: u32, payload: usize) -> Vec<u8> {
        let mut d = vec![0u8; HEADER_LEN];
        d[..4].copy_from_slice(b"DDS ");
        let put = |d: &mut Vec<u8>, o: usize, v: u32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut d, 4, 124);
        put(&mut d, 8, 0x1007 | 0x20000);
        put(&mut d, 12, h);
        put(&mut d, 16, w);
        put(&mut d, 28, mips);
        put(&mut d, 76, 32);
        put(&mut d, 80, pf_flags);
        d[84..88].copy_from_slice(fourcc);
        put(&mut d, 88, bits);
        for (k, m) in masks.iter().enumerate() {
            put(&mut d, 92 + 4 * k, *m);
        }
        put(&mut d, 112, caps2);
        d.resize(HEADER_LEN + payload, 0xAB);
        d
    }

    #[test]
    fn dxt1_chain() {
        // 8×4: levels 8×4 (2 blocks), 4×2 (1), 2×1 (1), 1×1 (1) = 5 blocks × 8 bytes.
        let d = build(8, 4, 4, PF_FOURCC, b"DXT1", 0, [0; 4], 0, 40);
        let t = Dds::parse(&d).unwrap();
        assert_eq!(t.format, Format::Dxt1);
        assert_eq!(t.levels[0].iter().map(|l| l.len()).collect::<Vec<_>>(), [16, 8, 8, 8]);
        let short = build(8, 4, 4, PF_FOURCC, b"DXT1", 0, [0; 4], 0, 39);
        assert_eq!(Dds::parse(&short).unwrap_err(), DdsError::SizeMismatch { expected: 40, actual: 39 });
    }

    /// Bytes after the last level the header counts are left alone: a mod's DXT1 with
    /// one block more in the file than its counted levels need loads those levels [#2164].
    #[test]
    fn data_after_the_last_level_is_ignored() {
        let d = build(8, 4, 3, PF_FOURCC, b"DXT1", 0, [0; 4], 0, 40);
        let t = Dds::parse(&d).unwrap();
        assert_eq!(t.levels[0].iter().map(|l| l.len()).collect::<Vec<_>>(), [16, 8, 8]);
        let argb = build(2, 1, 1, PF_RGB | PF_ALPHA_PIXELS, &[0; 4], 32, [0xFF0000, 0xFF00, 0xFF, 0xFF000000], 0, 11);
        assert_eq!(Dds::parse(&argb).unwrap().levels[0][0].len(), 8);
    }

    #[test]
    fn zero_mip_count_means_one_level() {
        let d = build(4, 4, 0, PF_FOURCC, b"DXT5", 0, [0; 4], 0, 16);
        assert_eq!(Dds::parse(&d).unwrap().levels[0].len(), 1);
    }

    #[test]
    fn cubemap_faces() {
        let d = build(4, 4, 1, PF_FOURCC, b"DXT3", 0, [0; 4], CAPS2_CUBEMAP | CAPS2_ALL_FACES, 6 * 16);
        let t = Dds::parse(&d).unwrap();
        assert_eq!(t.header.faces(), 6);
        assert_eq!(t.levels.len(), 6);
    }

    #[test]
    fn uncompressed_formats() {
        let argb = build(1, 1, 1, PF_RGB | PF_ALPHA_PIXELS, &[0; 4], 32, [0xFF0000, 0xFF00, 0xFF, 0xFF000000], 0, 4);
        assert_eq!(Dds::parse(&argb).unwrap().format, Format::A8R8G8B8);
        assert_eq!(to_rgba8(Format::A8R8G8B8, &[1, 2, 3, 4]).unwrap(), [3, 2, 1, 4]);
        assert_eq!(to_rgba8(Format::R8G8B8, &[1, 2, 3]).unwrap(), [3, 2, 1, 255]);
        assert_eq!(to_rgba8(Format::A4R4G4B4, &[0x21, 0xF3]).unwrap(), [0x33, 0x22, 0x11, 0xFF]);
        let odd = build(1, 1, 1, PF_RGB, &[0; 4], 16, [0x1F, 0x7E0, 0xF800, 0], 0, 2);
        assert!(matches!(Dds::parse(&odd), Err(DdsError::Unsupported(_))));
    }

    #[test]
    fn layouts_beyond_the_base_games() {
        let fmt = |flags, bits, masks| Dds::parse(&build(1, 1, 1, flags, &[0; 4], bits, masks, 0, (bits / 8) as usize)).map(|d| d.format);
        assert_eq!(fmt(PF_RGB, 32, [0xFF0000, 0xFF00, 0xFF, 0]), Ok(Format::X8R8G8B8));
        assert_eq!(fmt(PF_RGB | PF_ALPHA_PIXELS, 32, [0xFF, 0xFF00, 0xFF0000, 0xFF000000]), Ok(Format::A8B8G8R8));
        assert_eq!(fmt(PF_RGB, 32, [0xFF, 0xFF00, 0xFF0000, 0]), Ok(Format::X8B8G8R8));
        assert_eq!(fmt(PF_RGB, 16, [0xF800, 0x7E0, 0x1F, 0]), Ok(Format::R5G6B5));
        assert_eq!(fmt(PF_RGB | PF_ALPHA_PIXELS, 16, [0x7C00, 0x3E0, 0x1F, 0x8000]), Ok(Format::A1R5G5B5));
        assert_eq!(fmt(PF_RGB, 16, [0x7C00, 0x3E0, 0x1F, 0]), Ok(Format::X1R5G5B5));
        assert_eq!(fmt(PF_LUMINANCE, 8, [0xFF, 0, 0, 0]), Ok(Format::L8));
        assert_eq!(fmt(PF_LUMINANCE | PF_ALPHA_PIXELS, 16, [0xFF, 0, 0, 0xFF00]), Ok(Format::A8L8));
        assert_eq!(to_rgba8(Format::X8R8G8B8, &[1, 2, 3, 0]).unwrap(), [3, 2, 1, 255]);
        assert_eq!(to_rgba8(Format::A8B8G8R8, &[1, 2, 3, 4]).unwrap(), [1, 2, 3, 4]);
        assert_eq!(to_rgba8(Format::X8B8G8R8, &[1, 2, 3, 0]).unwrap(), [1, 2, 3, 255]);
        assert_eq!(to_rgba8(Format::R5G6B5, &0xF800u16.to_le_bytes()).unwrap(), [255, 0, 0, 255]);
        assert_eq!(to_rgba8(Format::A1R5G5B5, &0x03E0u16.to_le_bytes()).unwrap(), [0, 255, 0, 0]);
        assert_eq!(to_rgba8(Format::A1R5G5B5, &0x801Fu16.to_le_bytes()).unwrap(), [0, 0, 255, 255]);
        assert_eq!(to_rgba8(Format::X1R5G5B5, &0x001Fu16.to_le_bytes()).unwrap(), [0, 0, 255, 255]);
        assert_eq!(to_rgba8(Format::L8, &[7]).unwrap(), [7, 7, 7, 255]);
        assert_eq!(to_rgba8(Format::A8L8, &[7, 9]).unwrap(), [7, 7, 7, 9]);
        // A level is w × h × the bytes of one texel.
        assert_eq!(Format::L8.level_size(4, 2), 8);
        assert_eq!(Format::X8B8G8R8.level_size(4, 2), 32);
    }

    #[test]
    fn bad_input() {
        assert!(matches!(DdsHeader::parse(b"XXXX"), Err(DdsError::BadMagic(_))));
        assert_eq!(DdsHeader::parse(b"DDS \0\0"), Err(DdsError::Truncated(6)));
    }

    /// A side of 0 or past the cap is refused; the overflowing header that used to parse
    /// with an empty payload (0x80000000 square A8R8G8B8: its size wrapped to 0) too [#1995].
    #[test]
    fn sizes_out_of_range_are_refused() {
        let argb = |w: u32, h: u32, payload: usize| build(w, h, 1, PF_RGB | PF_ALPHA_PIXELS, b"\0\0\0\0", 32, [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000], 0, payload);
        let huge = argb(0x8000_0000, 0x8000_0000, 0);
        assert_eq!(Dds::parse(&huge).unwrap_err(), DdsError::BadSize { width: 0x8000_0000, height: 0x8000_0000 });
        assert!(matches!(Dds::parse(&argb(0, 4, 16)), Err(DdsError::BadSize { width: 0, height: 4 })));
        assert!(matches!(Dds::parse(&argb(4, 0, 16)), Err(DdsError::BadSize { .. })));
        assert!(matches!(Dds::parse(&argb(MAX_SIDE * 2, 1, 0)), Err(DdsError::BadSize { .. })));
        assert!(Dds::parse(&argb(4, 4, 64)).is_ok());
        // The listing reads the header as it is (fnvtool dds list).
        assert_eq!(DdsHeader::parse(&huge).unwrap().width, 0x8000_0000);
        // Sizes saturate rather than wrap.
        assert_eq!(Format::A8R8G8B8.level_size(u32::MAX, u32::MAX), (u32::MAX as usize).saturating_mul(u32::MAX as usize).saturating_mul(4));
        assert_ne!(Format::A8R8G8B8.level_size(0x8000_0000, 0x8000_0000), 0);
    }

    proptest::proptest! {
        #[test]
        fn never_panics(data in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..300)) {
            let _ = Dds::parse(&data);
        }

        #[test]
        fn header_fuzz_never_panics(w in 0u32..u32::MAX, h in 0u32..u32::MAX, mips in 0u32..u32::MAX) {
            let d = build(w, h, mips, PF_FOURCC, b"DXT1", 0, [0; 4], 0, 64);
            let _ = Dds::parse(&d);
        }
    }
}
