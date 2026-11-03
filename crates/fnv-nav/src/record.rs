//! NAVM (navmesh) and NAVI (navmesh info map) records, read from their subrecord bytes.
//!
//! Layouts [documented: xEdit wbDefinitionsFNV.pas NAVM/NAVI, wbDefinitionsCommon.pas
//! for the triangle flags and edge-link types]; what the masters hold and how the parts
//! fit together: docs/spec-notes/navmesh.md. FormIDs in the bytes are local to the
//! plugin; the readers convert them to load-order FormIDs.

use fnv_esm::{LoadOrder, Record};

/// NAVM record header flag "Initially Disabled" (bit 11) [documented: xEdit].
pub const NAVM_INITIALLY_DISABLED: u32 = 1 << 11;
/// NAVM record header flag "AutoGen" (bit 26) [documented: xEdit].
pub const NAVM_AUTOGEN: u32 = 1 << 26;
/// Record header flag "Deleted" [documented: xEdit, all records].
pub const RECORD_DELETED: u32 = 0x20;

/// Triangle flags (NVTR `Flags`) [documented: xEdit wbNavmeshTriangleFlags].
pub mod tri_flags {
    /// Bits 0–2: edge 0-1, 1-2, 2-0 leads to another navmesh (its value indexes NVEX).
    pub const EDGE_LINK_0: u16 = 1 << 0;
    pub const DELETED: u16 = 1 << 3;
    pub const NO_LARGE_CREATURES: u16 = 1 << 4;
    pub const OVERLAPPING: u16 = 1 << 5;
    pub const PREFERRED: u16 = 1 << 6;
    pub const WATER: u16 = 1 << 9;
    pub const DOOR: u16 = 1 << 10;
    pub const FOUND: u16 = 1 << 11;
}

/// NVEX edge-link types [documented: xEdit wbNavmeshEdgeLinkEnum].
pub mod link_kind {
    pub const PORTAL: u32 = 0;
    pub const LEDGE_UP: u32 = 1;
    pub const LEDGE_DOWN: u32 = 2;
    pub const ENABLE_DISABLE_PORTAL: u32 = 3;

    pub fn name(k: u32) -> &'static str {
        match k {
            PORTAL => "portal",
            LEDGE_UP => "ledge_up",
            LEDGE_DOWN => "ledge_down",
            ENABLE_DISABLE_PORTAL => "enable_disable_portal",
            _ => "unknown",
        }
    }
}

/// One NVTR triangle (16 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triangle {
    /// Vertex indices into the navmesh's NVVX.
    pub v: [u16; 3],
    /// Edge 0-1, 1-2, 2-0: a triangle of the same navmesh, or (when the edge's link flag
    /// is set) an index into NVEX; −1 for none [documented: xEdit wbNVTREdgeToStr reads a
    /// value as an NVEX index exactly when the edge's flag bit is set; −1 for "no
    /// neighbour" observed, docs/spec-notes/navmesh.md].
    pub edges: [i16; 3],
    pub flags: u16,
    /// Per-edge cover values and flags [documented: xEdit says its own flag names for
    /// this field are wrong; not used].
    pub cover: u16,
}

impl Triangle {
    /// Whether edge `e` (0: v0-v1, 1: v1-v2, 2: v2-v0) leads to another navmesh.
    pub fn edge_is_link(&self, e: usize) -> bool {
        self.flags & (tri_flags::EDGE_LINK_0 << e) != 0
    }
    pub fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }
}

/// NVDP: a door next to a triangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoorLink {
    /// The door reference (load-order FormID).
    pub door: u32,
    pub triangle: u16,
}

/// NVEX: where a linked edge leads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeLink {
    /// [`link_kind`].
    pub kind: u32,
    /// The navmesh on the other side (load-order FormID; `None` when null or not
    /// mappable).
    pub navmesh: Option<u32>,
    pub triangle: u16,
}

/// NVGD: the navmesh's own triangle lookup grid.
#[derive(Debug, Clone, PartialEq)]
pub struct NavGrid {
    pub divisor: u32,
    pub max_x_distance: f32,
    pub max_y_distance: f32,
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// Triangle lists of the grid squares, in storage order.
    pub cells: Vec<Vec<u16>>,
}

/// A NAVM record.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NavMeshRecord {
    /// Load-order FormID.
    pub form_id: u32,
    /// Load-order index of the plugin this definition comes from.
    pub plugin: usize,
    /// Record header flags.
    pub flags: u32,
    pub edid: Option<String>,
    pub version: Option<u32>,
    /// DATA: the cell (load-order FormID).
    pub cell: Option<u32>,
    /// DATA counts: vertices, triangles, edge links, cover triangles, door links.
    pub counts: [u32; 5],
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<Triangle>,
    pub cover_triangles: Vec<u16>,
    pub door_links: Vec<DoorLink>,
    pub grid: Option<NavGrid>,
    pub edge_links: Vec<EdgeLink>,
    /// Subrecords not named by the layout, and size problems (signature, message).
    pub issues: Vec<String>,
}

impl NavMeshRecord {
    pub fn initially_disabled(&self) -> bool {
        self.flags & NAVM_INITIALLY_DISABLED != 0
    }
    pub fn deleted(&self) -> bool {
        self.flags & RECORD_DELETED != 0
    }
}

/// NVMI island data: a simplified mesh of a navmesh island.
#[derive(Debug, Clone, PartialEq)]
pub struct Island {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u16; 3]>,
}

/// NVMI: one navmesh's entry in the info map.
#[derive(Debug, Clone, PartialEq)]
pub struct NavInfo {
    pub flags: u32,
    pub navmesh: Option<u32>,
    /// The cell or worldspace.
    pub location: Option<u32>,
    pub grid: (i16, i16),
    pub approx: [f32; 3],
    pub island: Option<Island>,
    pub preferred: f32,
}

/// NVCI: the navmeshes a navmesh connects to.
#[derive(Debug, Clone, PartialEq)]
pub struct NavConnection {
    pub navmesh: Option<u32>,
    pub standard: Vec<Option<u32>>,
    pub preferred: Vec<Option<u32>>,
    pub doors: Vec<Option<u32>>,
}

/// A NAVI record.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NavInfoMap {
    pub form_id: u32,
    pub plugin: usize,
    pub version: Option<u32>,
    pub infos: Vec<NavInfo>,
    pub connections: Vec<NavConnection>,
    pub issues: Vec<String>,
}

/// NVMI flag "Is Island" (bit 5) [documented: xEdit wbNAVINVMIDecider].
pub const NVMI_IS_ISLAND: u32 = 1 << 5;
/// NVMI flag "Initially Disabled" (bit 4) [documented: xEdit].
pub const NVMI_INITIALLY_DISABLED: u32 = 1 << 4;

/// Little-endian reader over a subrecord's bytes.
struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn new(b: &'a [u8]) -> Self {
        Rd { b, p: 0 }
    }
    fn left(&self) -> usize {
        self.b.len() - self.p
    }
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let s = self.b.get(self.p..self.p + N)?;
        self.p += N;
        s.try_into().ok()
    }
    fn u16(&mut self) -> Option<u16> {
        self.take().map(u16::from_le_bytes)
    }
    fn i16(&mut self) -> Option<i16> {
        self.take().map(i16::from_le_bytes)
    }
    fn u32(&mut self) -> Option<u32> {
        self.take().map(u32::from_le_bytes)
    }
    fn f32(&mut self) -> Option<f32> {
        self.take().map(f32::from_le_bytes)
    }
    fn vec3(&mut self) -> Option<[f32; 3]> {
        Some([self.f32()?, self.f32()?, self.f32()?])
    }
}

fn fid(lo: &LoadOrder, pi: usize, local: u32) -> Option<u32> {
    if local == 0 {
        None
    } else {
        lo.to_global(pi, local)
    }
}

fn zstring(b: &[u8]) -> String {
    // In this run's code page (fnv_util::text, #1439).
    fnv_util::text::decode_z(b)
}

/// Read a NAVM record of plugin `pi`.
pub fn read_navm(lo: &LoadOrder, pi: usize, r: &Record) -> Result<NavMeshRecord, String> {
    let subs = r.subrecords().map_err(|e| e.to_string())?;
    let form_id = lo.to_global(pi, r.header.form_id).ok_or("unmappable FormID")?;
    let mut m = NavMeshRecord { form_id, plugin: pi, flags: r.header.flags, ..Default::default() };
    for s in &subs {
        let d = &s.data[..];
        let mut rd = Rd::new(d);
        let sig = s.sig_str();
        let size = |elem: usize, m: &mut NavMeshRecord| {
            if d.len() % elem != 0 {
                m.issues.push(format!("{sig}: {} bytes is not a multiple of {elem}", d.len()));
            }
        };
        match &s.sig {
            b"EDID" => m.edid = Some(zstring(d)),
            b"NVER" => m.version = rd.u32(),
            b"DATA" => {
                m.cell = rd.u32().and_then(|c| fid(lo, pi, c));
                for c in m.counts.iter_mut() {
                    *c = rd.u32().unwrap_or(0);
                }
                if d.len() != 24 {
                    m.issues.push(format!("DATA: {} bytes, expected 24", d.len()));
                }
            }
            b"NVVX" => {
                size(12, &mut m);
                while let Some(v) = rd.vec3() {
                    m.vertices.push(v);
                }
            }
            b"NVTR" => {
                size(16, &mut m);
                while rd.left() >= 16 {
                    let v = [rd.u16().unwrap(), rd.u16().unwrap(), rd.u16().unwrap()];
                    let edges = [rd.i16().unwrap(), rd.i16().unwrap(), rd.i16().unwrap()];
                    let flags = rd.u16().unwrap();
                    let cover = rd.u16().unwrap();
                    m.triangles.push(Triangle { v, edges, flags, cover });
                }
            }
            b"NVCA" => {
                size(2, &mut m);
                while let Some(t) = rd.u16() {
                    m.cover_triangles.push(t);
                }
            }
            b"NVDP" => {
                size(8, &mut m);
                while rd.left() >= 8 {
                    let door = rd.u32().unwrap();
                    let triangle = rd.u16().unwrap();
                    rd.u16();
                    match fid(lo, pi, door) {
                        Some(door) => m.door_links.push(DoorLink { door, triangle }),
                        None => m.issues.push(format!("NVDP: door {door:08X} not mappable")),
                    }
                }
            }
            b"NVGD" => match read_grid(d) {
                Ok(g) => m.grid = Some(g),
                Err(e) => m.issues.push(format!("NVGD: {e}")),
            },
            b"NVEX" => {
                size(10, &mut m);
                while rd.left() >= 10 {
                    let kind = rd.u32().unwrap();
                    let nav = rd.u32().unwrap();
                    let triangle = rd.u16().unwrap();
                    m.edge_links.push(EdgeLink { kind, navmesh: fid(lo, pi, nav), triangle });
                }
            }
            _ => m.issues.push(format!("unexpected subrecord {sig} ({} bytes)", d.len())),
        }
    }
    Ok(m)
}

/// NVGD: divisor, max X and Y distance, bounds, then each grid square's triangles as a
/// u16 count followed by that many u16 indices [documented: xEdit, `wbArray(..., -2)`
/// is a u16-counted array] until the end of the subrecord.
fn read_grid(d: &[u8]) -> Result<NavGrid, String> {
    let mut rd = Rd::new(d);
    let short = || "too short".to_string();
    let divisor = rd.u32().ok_or_else(short)?;
    let max_x_distance = rd.f32().ok_or_else(short)?;
    let max_y_distance = rd.f32().ok_or_else(short)?;
    let min = rd.vec3().ok_or_else(short)?;
    let max = rd.vec3().ok_or_else(short)?;
    let mut cells = Vec::new();
    while rd.left() > 0 {
        let n = rd.u16().ok_or("truncated cell count")? as usize;
        let mut c = Vec::with_capacity(n);
        for _ in 0..n {
            c.push(rd.u16().ok_or("truncated cell")?);
        }
        cells.push(c);
    }
    Ok(NavGrid { divisor, max_x_distance, max_y_distance, min, max, cells })
}

/// Read a NAVI record of plugin `pi`. NVMI: flags, navmesh, location, grid Y and X,
/// approximate location, island data when flag bit 5 is set, preferred share; NVCI:
/// navmesh, then three u32-counted FormID arrays (standard, preferred, doors)
/// [documented: xEdit].
pub fn read_navi(lo: &LoadOrder, pi: usize, r: &Record) -> Result<NavInfoMap, String> {
    let subs = r.subrecords().map_err(|e| e.to_string())?;
    let form_id = lo.to_global(pi, r.header.form_id).ok_or("unmappable FormID")?;
    let mut m = NavInfoMap { form_id, plugin: pi, ..Default::default() };
    for s in &subs {
        let d = &s.data[..];
        let mut rd = Rd::new(d);
        match &s.sig {
            b"EDID" => {}
            b"NVER" => m.version = rd.u32(),
            b"NVMI" => match read_nvmi(lo, pi, &mut rd) {
                Some(i) => {
                    if rd.left() != 0 {
                        m.issues.push(format!("NVMI: {} bytes left over", rd.left()));
                    }
                    m.infos.push(i)
                }
                None => m.issues.push(format!("NVMI: truncated ({} bytes)", d.len())),
            },
            b"NVCI" => {
                let list = |rd: &mut Rd| -> Option<Vec<Option<u32>>> {
                    let n = rd.u32()? as usize;
                    (0..n).map(|_| rd.u32().map(|f| fid(lo, pi, f))).collect()
                };
                let c = (|| {
                    let navmesh = fid(lo, pi, rd.u32()?);
                    Some(NavConnection { navmesh, standard: list(&mut rd)?, preferred: list(&mut rd)?, doors: list(&mut rd)? })
                })();
                match c {
                    Some(c) if rd.left() == 0 => m.connections.push(c),
                    Some(c) => {
                        m.issues.push(format!("NVCI: {} bytes left over", rd.left()));
                        m.connections.push(c)
                    }
                    None => m.issues.push(format!("NVCI: truncated ({} bytes)", d.len())),
                }
            }
            _ => m.issues.push(format!("unexpected subrecord {} ({} bytes)", s.sig_str(), d.len())),
        }
    }
    Ok(m)
}

fn read_nvmi(lo: &LoadOrder, pi: usize, rd: &mut Rd) -> Option<NavInfo> {
    let flags = rd.u32()?;
    let navmesh = fid(lo, pi, rd.u32()?);
    let location = fid(lo, pi, rd.u32()?);
    let gy = rd.i16()?;
    let gx = rd.i16()?;
    let approx = rd.vec3()?;
    let island = if flags & NVMI_IS_ISLAND != 0 {
        let min = rd.vec3()?;
        let max = rd.vec3()?;
        let nv = rd.u16()? as usize;
        let nt = rd.u16()? as usize;
        let vertices = (0..nv).map(|_| rd.vec3()).collect::<Option<Vec<_>>>()?;
        let triangles = (0..nt).map(|_| Some([rd.u16()?, rd.u16()?, rd.u16()?])).collect::<Option<Vec<_>>>()?;
        Some(Island { min, max, vertices, triangles })
    } else {
        None
    };
    let preferred = rd.f32()?;
    Some(NavInfo { flags, navmesh, location, grid: (gx, gy), approx, island, preferred })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_reads_counted_cells() {
        let mut d = Vec::new();
        d.extend(2u32.to_le_bytes());
        for f in [10.0f32, 20.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0] {
            d.extend(f.to_le_bytes());
        }
        for cell in [&[][..], &[1u16, 2][..], &[7u16][..], &[][..]] {
            d.extend((cell.len() as u16).to_le_bytes());
            for t in cell {
                d.extend(t.to_le_bytes());
            }
        }
        let g = read_grid(&d).unwrap();
        assert_eq!(g.divisor, 2);
        assert_eq!(g.max, [3.0, 4.0, 5.0]);
        assert_eq!(g.cells, vec![vec![], vec![1, 2], vec![7], vec![]]);
        assert!(read_grid(&d[..d.len() - 1]).is_err());
    }

    #[test]
    fn edge_link_flags() {
        let t = Triangle { v: [0, 1, 2], edges: [3, -1, 0], flags: 0b101 | tri_flags::PREFERRED, cover: 0 };
        assert!(t.edge_is_link(0));
        assert!(!t.edge_is_link(1));
        assert!(t.edge_is_link(2));
        assert!(t.has(tri_flags::PREFERRED));
        assert!(!t.has(tri_flags::DELETED));
    }
}
