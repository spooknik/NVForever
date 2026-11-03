//! A whole plugin file: the TES4 header record plus the top-level groups.

use crate::raw::{self, EsmError, Node, Record};
use bytes::Bytes;
use fnv_util::Cursor;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct MasterRef {
    pub name: String,
    /// The u64 that follows each MAST in a DATA subrecord (zero in all shipped
    /// masters [observed]; community docs describe it as the master's file size).
    pub data: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginHeader {
    pub flags: u32,
    pub form_version: u16,
    /// HEDR: format version float (1.32–1.34 across the shipped masters [observed]).
    pub version: f32,
    /// HEDR: equals (records + groups − 1), i.e. excluding the TES4 record itself,
    /// in every shipped master [observed].
    pub num_records: i32,
    pub next_object_id: u32,
    pub author: Option<String>,
    pub description: Option<String>,
    pub masters: Vec<MasterRef>,
    /// ONAM: FormIDs of master records this plugin overrides (array of u32).
    pub overridden_forms: Vec<u32>,
    /// Subrecords of TES4 not interpreted above (preserved for reporting).
    pub other: Vec<String>,
}

impl PluginHeader {
    pub fn is_master(&self) -> bool {
        self.flags & raw::record_flags::MASTER != 0
    }
}

pub struct Plugin {
    pub name: String,
    pub nodes: Vec<Node>,
    pub header: PluginHeader,
    pub file_size: u64,
}

impl Plugin {
    /// Memory-map the file; records reference the mapping without copying.
    pub fn open(path: impl AsRef<Path>) -> Result<Plugin, EsmError> {
        let p = path.as_ref();
        let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        Plugin::parse_bytes(map_file(p)?, name)
    }

    /// Parse a copy of `data`.
    pub fn parse(data: &[u8], name: String) -> Result<Plugin, EsmError> {
        Plugin::parse_bytes(Bytes::copy_from_slice(data), name)
    }

    /// Parse without copying: records are slices of `data`.
    pub fn parse_bytes(data: Bytes, name: String) -> Result<Plugin, EsmError> {
        let nodes = raw::parse_nodes(&data, 0, data.len(), 0)?;
        let first = match nodes.first() {
            Some(Node::Record(r)) if &r.header.sig == b"TES4" => r,
            _ => {
                return Err(EsmError::Structure { offset: 0, msg: "file does not start with a TES4 record".into() })
            }
        };
        let header = parse_header(first)?;
        tracing::debug!(plugin = %name, masters = header.masters.len(), bytes = data.len(), "parsed plugin");
        Ok(Plugin { name, nodes, header, file_size: data.len() as u64 })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.file_size as usize);
        raw::write_nodes(&self.nodes, &mut out);
        out
    }

    pub fn header_record(&self) -> &Record {
        match &self.nodes[0] {
            Node::Record(r) => r,
            _ => unreachable!("checked in parse"),
        }
    }

    /// Index this plugin's own records have in its FormIDs' top byte.
    pub fn self_index(&self) -> u8 {
        self.header.masters.len().min(255) as u8
    }
}

/// Read-only shared mapping of a plugin file.
pub fn map_file(path: &Path) -> Result<Bytes, EsmError> {
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a regular file").into());
    }
    // SAFETY: plugins are read-only inputs (the game install is never written by
    // NVForever, docs/design/install-layout.md). If another process truncates or
    // rewrites the file while it is mapped, reads may fault or see changed bytes;
    // that is the accepted trade-off for not copying 245 MB masters.
    let map = unsafe { memmap2::Mmap::map(&file)? };
    Ok(Bytes::from_owner(map))
}

fn parse_header(r: &Record) -> Result<PluginHeader, EsmError> {
    let subs = r.subrecords()?;
    let mut h = PluginHeader {
        flags: r.header.flags,
        form_version: r.header.form_version,
        version: 0.0,
        num_records: 0,
        next_object_id: 0,
        author: None,
        description: None,
        masters: Vec::new(),
        overridden_forms: Vec::new(),
        other: Vec::new(),
    };
    let err = |m: &str| EsmError::Structure { offset: r.offset, msg: format!("TES4: {m}") };
    let mut saw_hedr = false;
    for s in &subs {
        match &s.sig {
            b"HEDR" => {
                let mut c = Cursor::new(&s.data);
                h.version = c.f32().map_err(|_| err("HEDR too short"))?;
                h.num_records = c.i32().map_err(|_| err("HEDR too short"))?;
                h.next_object_id = c.u32().map_err(|_| err("HEDR too short"))?;
                saw_hedr = true;
            }
            b"CNAM" => h.author = Some(fnv_util::text::decode_z(&s.data)),
            b"SNAM" => h.description = Some(fnv_util::text::decode_z(&s.data)),
            b"MAST" => h.masters.push(MasterRef { name: fnv_util::text::decode_z(&s.data), data: None }),
            b"DATA" => {
                if let Some(m) = h.masters.last_mut() {
                    if s.data.len() == 8 {
                        m.data = Some(u64::from_le_bytes(s.data[..8].try_into().unwrap()));
                    }
                }
            }
            b"ONAM" => {
                h.overridden_forms = s.data.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
            }
            _ => h.other.push(s.sig_str()),
        }
    }
    if !saw_hedr {
        return Err(err("missing HEDR"));
    }
    Ok(h)
}
