//! NIF container: header, block table and footer (docs/spec-notes/nif.md §1–§3).
//!
//! Only the versions present in the FNV corpus are accepted: 20.0.0.4 and 20.2.0.7,
//! both with a Bethesda stream header. Every byte read is kept so that `write` can
//! reproduce the original header exactly.

use fnv_util::{Cursor, OutOfBounds};

/// Packed file version, e.g. `0x14020007` for 20.2.0.7.
pub type Version = u32;

pub const V20_0_0_4: Version = 0x1400_0004;
pub const V20_1_0_1: Version = 0x1401_0001;
pub const V20_1_0_3: Version = 0x1401_0003;
pub const V20_2_0_5: Version = 0x1402_0005;
pub const V20_2_0_7: Version = 0x1402_0007;

/// Parse a dotted version such as `"20.2.0.7"` into its packed form.
pub fn parse_version(s: &str) -> Option<Version> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut v: u32 = 0;
    for p in parts {
        let n: u8 = p.parse().ok()?;
        v = (v << 8) | n as u32;
    }
    Some(v)
}

pub fn version_string(v: Version) -> String {
    format!("{}.{}.{}.{}", v >> 24, (v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NifError {
    #[error("not a NIF: {0}")]
    NotNif(String),
    #[error("unsupported NIF version {0}")]
    Unsupported(String),
    #[error("truncated: {0}")]
    Truncated(#[from] OutOfBounds),
    #[error("{0}")]
    Invalid(String),
}

/// A length-prefixed string kept as raw bytes (Windows-1252 on disk).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawString(pub Vec<u8>);

impl RawString {
    pub fn text(&self) -> String {
        fnv_util::text::decode(&self.0)
    }
}

/// Bethesda's export information block (nif.xml `BSStreamHeader`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BsHeader {
    pub bs_version: u32,
    /// The three (or four) export strings, raw: a u8 length followed by that many
    /// bytes, which normally include a terminating NUL.
    pub author: Vec<u8>,
    pub unknown_int: Option<u32>,
    pub process_script: Option<Vec<u8>>,
    pub export_script: Vec<u8>,
    pub max_filepath: Option<Vec<u8>>,
}

impl BsHeader {
    pub fn author_text(&self) -> String {
        export_text(&self.author)
    }
}

/// Text of a raw export string (skips the length byte, stops at the NUL).
pub fn export_text(raw: &[u8]) -> String {
    fnv_util::text::decode_z(raw.get(1..).unwrap_or_default())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// The first line without its terminating 0x0A.
    pub header_string: Vec<u8>,
    pub version: Version,
    pub endian: u8,
    pub user_version: u32,
    pub num_blocks: u32,
    pub bs: Option<BsHeader>,
    pub block_types: Vec<RawString>,
    /// Raw u16 index per block into `block_types`.
    pub block_type_index: Vec<u16>,
    /// Byte size of every block (20.2.0.5 and later).
    pub block_sizes: Option<Vec<u32>>,
    /// Global string table (20.1.0.1 and later).
    pub max_string_length: Option<u32>,
    pub strings: Option<Vec<RawString>>,
    pub groups: Vec<u32>,
}

impl Header {
    pub fn bs_version(&self) -> u32 {
        self.bs.as_ref().map_or(0, |b| b.bs_version)
    }

    /// Type name of block `i`. The top bit of the index is masked off (nif.xml notes it
    /// as a flag for PhysX types; it is never set in the FNV corpus [observed]).
    pub fn block_type_name(&self, i: usize) -> Option<String> {
        let idx = *self.block_type_index.get(i)? as usize & 0x7FFF;
        self.block_types.get(idx).map(|s| s.text())
    }

    /// Text of string-table entry `i`.
    pub fn string(&self, i: u32) -> Option<String> {
        self.strings.as_ref()?.get(i as usize).map(|s| s.text())
    }

    pub fn parse(c: &mut Cursor<'_>) -> Result<Header, NifError> {
        let rest = &c.data[c.pos..];
        let nl = rest
            .iter()
            .take(128)
            .position(|&b| b == 0x0A)
            .ok_or_else(|| NifError::NotNif("no header line within 128 bytes".into()))?;
        let header_string = rest[..nl].to_vec();
        let line = String::from_utf8_lossy(&header_string).into_owned();
        if !line.starts_with("Gamebryo File Format, Version ") && !line.starts_with("NetImmerse File Format") {
            return Err(NifError::NotNif(format!("header line {line:?}")));
        }
        c.pos += nl + 1;
        let version = c.u32()?;
        if !(V20_0_0_4..=V20_2_0_7).contains(&version) {
            return Err(NifError::Unsupported(version_string(version)));
        }
        let endian = c.u8()?;
        if endian != 1 {
            return Err(NifError::Unsupported(format!("big-endian file (endian byte {endian})")));
        }
        let user_version = c.u32()?;
        let num_blocks = c.u32()?;
        // nif.xml #BSSTREAMHEADER#, restricted to the versions accepted above.
        let has_bs = user_version >= 3
            && (version == V20_2_0_7 || version == 0x1400_0005 || (version == V20_0_0_4 && user_version <= 11));
        let bs = if has_bs {
            let bs_version = c.u32()?;
            let author = export_string(c)?;
            let unknown_int = if bs_version > 130 { Some(c.u32()?) } else { None };
            let process_script = if bs_version < 131 { Some(export_string(c)?) } else { None };
            let export_script = export_string(c)?;
            let max_filepath = if bs_version >= 103 { Some(export_string(c)?) } else { None };
            Some(BsHeader { bs_version, author, unknown_int, process_script, export_script, max_filepath })
        } else {
            None
        };
        let num_types = c.u16()?;
        let mut block_types = Vec::with_capacity(num_types as usize);
        for _ in 0..num_types {
            block_types.push(sized_string(c)?);
        }
        let n = num_blocks as usize;
        if n > c.remaining() / 2 {
            return Err(NifError::Invalid(format!("{num_blocks} blocks cannot fit in the file")));
        }
        let mut block_type_index = Vec::with_capacity(n);
        for i in 0..n {
            let t = c.u16()?;
            if (t & 0x7FFF) as usize >= block_types.len() {
                return Err(NifError::Invalid(format!("block {i} has type index {t} of {num_types}")));
            }
            block_type_index.push(t);
        }
        let block_sizes = if version >= V20_2_0_5 {
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(c.u32()?);
            }
            Some(v)
        } else {
            None
        };
        let (max_string_length, strings) = if version >= V20_1_0_1 {
            let count = c.u32()?;
            let max = c.u32()?;
            if count as usize > c.remaining() / 4 {
                return Err(NifError::Invalid(format!("{count} strings cannot fit in the file")));
            }
            let mut v = Vec::with_capacity(count as usize);
            for _ in 0..count {
                v.push(sized_string(c)?);
            }
            (Some(max), Some(v))
        } else {
            (None, None)
        };
        let num_groups = c.u32()?;
        if num_groups as usize > c.remaining() / 4 {
            return Err(NifError::Invalid(format!("{num_groups} groups cannot fit in the file")));
        }
        let mut groups = Vec::with_capacity(num_groups as usize);
        for _ in 0..num_groups {
            groups.push(c.u32()?);
        }
        Ok(Header {
            header_string,
            version,
            endian,
            user_version,
            num_blocks,
            bs,
            block_types,
            block_type_index,
            block_sizes,
            max_string_length,
            strings,
            groups,
        })
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.header_string);
        out.push(0x0A);
        out.extend_from_slice(&self.version.to_le_bytes());
        out.push(self.endian);
        out.extend_from_slice(&self.user_version.to_le_bytes());
        out.extend_from_slice(&self.num_blocks.to_le_bytes());
        if let Some(bs) = &self.bs {
            out.extend_from_slice(&bs.bs_version.to_le_bytes());
            out.extend_from_slice(&bs.author);
            if let Some(u) = bs.unknown_int {
                out.extend_from_slice(&u.to_le_bytes());
            }
            if let Some(p) = &bs.process_script {
                out.extend_from_slice(p);
            }
            out.extend_from_slice(&bs.export_script);
            if let Some(m) = &bs.max_filepath {
                out.extend_from_slice(m);
            }
        }
        out.extend_from_slice(&(self.block_types.len() as u16).to_le_bytes());
        for t in &self.block_types {
            write_sized(out, &t.0);
        }
        for t in &self.block_type_index {
            out.extend_from_slice(&t.to_le_bytes());
        }
        if let Some(sizes) = &self.block_sizes {
            for s in sizes {
                out.extend_from_slice(&s.to_le_bytes());
            }
        }
        if let Some(strings) = &self.strings {
            out.extend_from_slice(&(strings.len() as u32).to_le_bytes());
            out.extend_from_slice(&self.max_string_length.unwrap_or(0).to_le_bytes());
            for s in strings {
                write_sized(out, &s.0);
            }
        }
        out.extend_from_slice(&(self.groups.len() as u32).to_le_bytes());
        for g in &self.groups {
            out.extend_from_slice(&g.to_le_bytes());
        }
    }
}

/// Export string, kept raw including its length byte.
fn export_string(c: &mut Cursor<'_>) -> Result<Vec<u8>, NifError> {
    let start = c.pos;
    let n = c.u8()? as usize;
    c.bytes(n)?;
    Ok(c.data[start..c.pos].to_vec())
}

pub(crate) fn sized_string(c: &mut Cursor<'_>) -> Result<RawString, NifError> {
    let n = c.u32()? as usize;
    Ok(RawString(c.bytes(n)?.to_vec()))
}

pub(crate) fn write_sized(out: &mut Vec<u8>, b: &[u8]) {
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(b);
}

/// The footer: root block references.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Footer {
    pub roots: Vec<i32>,
}

impl Footer {
    pub fn parse(c: &mut Cursor<'_>) -> Result<Footer, NifError> {
        let n = c.u32()?;
        if n as usize > c.remaining() / 4 {
            return Err(NifError::Invalid(format!("footer claims {n} roots")));
        }
        let mut roots = Vec::with_capacity(n as usize);
        for _ in 0..n {
            roots.push(c.i32()?);
        }
        Ok(Footer { roots })
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&(self.roots.len() as u32).to_le_bytes());
        for r in &self.roots {
            out.extend_from_slice(&r.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("20.2.0.7"), Some(V20_2_0_7));
        assert_eq!(parse_version("20.0.0.4"), Some(V20_0_0_4));
        assert_eq!(parse_version("20.2.0"), None);
        assert_eq!(version_string(V20_2_0_7), "20.2.0.7");
    }
}
