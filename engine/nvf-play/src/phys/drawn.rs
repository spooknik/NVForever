//! The drawn floors of a model: where its drawn (render) geometry lies under a point, so
//! a body can be drawn on what the player sees rather than on the collision under it
//! (docs/spec-notes/floating-actors.md §7).
//!
//! A model's collision need not lie on its drawn surface: the Prospector Saloon's porch
//! collision is a flat plane 2.5 units under the top of its drawn boards (and 2.6 above
//! their underside) [observed: `nv_prospectorsaloon.nif`, its packed collision strips
//! against its `NV_ProspectorSaloon:2` shape under Easy Pete's placement]. The terrain's
//! collision is the drawn landscape itself, so this is only read for models.

use fnv_nif::file::NifFile;
use fnv_nif::scene::{mesh_data, scene_objects, Decoded, Transform};

/// Triangles this close to vertical are walls, not floors: left out (the cosine of the
/// steepest floor, 60 degrees) [design].
const FLOOR_MIN_NZ: f32 = 0.5;
/// The grid's cell size, model units [design].
const CELL: f32 = 32.0;
/// The most grid cells along one side; larger models get larger cells [design].
const MAX_SIDE: usize = 256;

/// The floor-like triangles of a model's drawn geometry, in model space, with a grid
/// over x and y to find those over a point.
#[derive(Debug, Clone)]
pub struct DrawnFloors {
    tris: Vec<[[f32; 3]; 3]>,
    origin: [f32; 2],
    cell: f32,
    dims: [usize; 2],
    cells: Vec<Vec<u32>>,
}

impl DrawnFloors {
    /// The floors of a model's drawn shapes, as `nvf_assets::model` draws a placed
    /// reference's: every triangle shape with a shader (not an editor helper), not
    /// skinned and not hidden (nor under a hidden node) [documented: nif.xml NiAVObject
    /// flags, bit 0 Hidden], placed by its node transforms below the root (a placed
    /// reference's transform replaces the root's). `None` when there are none.
    /// Shapes with no property at all on themselves or a parent, in a model that holds
    /// a shader property, are editor geometry too (the furniture-marker figures and
    /// capsules of `furniture\loungechair_tops.nif`, `nvf_assets`' `Props::is_helper`,
    /// docs/spec-notes/nif.md §6 "Shapes with no property at all").
    pub fn from_nif(file: &NifFile, dec: &Decoded) -> Option<DrawnFloors> {
        let objects = scene_objects(file, dec);
        let by_block: std::collections::HashMap<usize, usize> = objects.iter().enumerate().map(|(i, o)| (o.block, i)).collect();
        let hidden = |block: usize| dec.blocks.get(block).and_then(|b| b.as_ref().ok()).and_then(|s| s.int("flags")).is_some_and(|f| f & 1 != 0);
        let has_properties = |block: usize| dec.blocks.get(block).and_then(|b| b.as_ref().ok()).and_then(|s| s.refs("properties")).is_some_and(|p| !p.is_empty());
        let shaded = (0..file.blocks.len()).any(|i| dec.is_a(i, "BSShaderProperty"));
        let mut tris = Vec::new();
        for o in objects.iter().filter(|o| !o.is_node) {
            let Ok(shape) = dec.blocks[o.block].as_ref() else { continue };
            if shape.block_ref("skin_instance").is_some_and(|s| s >= 0) || helper(file, shape) {
                continue;
            }
            // Up the chain: the shape's transform, then each parent's.
            let mut chain = Vec::new();
            let mut at = Some(o.block);
            let mut shown = true;
            let mut bare = true;
            while let Some(b) = at {
                shown &= !hidden(b);
                bare &= !has_properties(b);
                let Some(&i) = by_block.get(&b) else { break };
                at = objects[i].parent;
                if at.is_some() {
                    chain.push(objects[i].transform);
                }
            }
            if !shown || (bare && shaded) {
                continue;
            }
            let Some(data) = shape.block_ref("data").and_then(|r| dec.get(r)) else { continue };
            let m = mesh_data(data);
            let place = |v: [f32; 3]| chain.iter().fold(v, |v, t| apply(t, v));
            let pos: Vec<[f32; 3]> = m.positions.iter().map(|&v| place(v)).collect();
            for t in &m.triangles {
                let Some(tri) = t.iter().map(|&i| pos.get(i as usize).copied()).collect::<Option<Vec<_>>>() else { continue };
                let tri = [tri[0], tri[1], tri[2]];
                if floor_like(&tri) {
                    tris.push(tri);
                }
            }
        }
        DrawnFloors::new(tris)
    }

    /// Floors made of these triangles (model space); `None` when there are none.
    pub fn new(tris: Vec<[[f32; 3]; 3]>) -> Option<DrawnFloors> {
        if tris.is_empty() {
            return None;
        }
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for v in tris.iter().flatten() {
            for k in 0..2 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        let span = (hi[0] - lo[0]).max(hi[1] - lo[1]);
        let cell = CELL.max(span / MAX_SIDE as f32);
        let dims = [((hi[0] - lo[0]) / cell) as usize + 1, ((hi[1] - lo[1]) / cell) as usize + 1];
        let mut cells = vec![Vec::new(); dims[0] * dims[1]];
        for (i, t) in tris.iter().enumerate() {
            let (mut a, mut b) = ([f32::MAX; 2], [f32::MIN; 2]);
            for v in t {
                for k in 0..2 {
                    a[k] = a[k].min(v[k]);
                    b[k] = b[k].max(v[k]);
                }
            }
            let c0 = [((a[0] - lo[0]) / cell) as usize, ((a[1] - lo[1]) / cell) as usize];
            let c1 = [((b[0] - lo[0]) / cell) as usize, ((b[1] - lo[1]) / cell) as usize];
            for y in c0[1]..=c1[1].min(dims[1] - 1) {
                for x in c0[0]..=c1[0].min(dims[0] - 1) {
                    cells[y * dims[0] + x].push(i as u32);
                }
            }
        }
        Some(DrawnFloors { tris, origin: lo, cell, dims, cells })
    }

    /// The highest drawn floor over model point (x, y) between heights `lo` and `hi`.
    pub fn height_at(&self, x: f32, y: f32, lo: f32, hi: f32) -> Option<f32> {
        let cx = ((x - self.origin[0]) / self.cell).floor();
        let cy = ((y - self.origin[1]) / self.cell).floor();
        if cx < 0.0 || cy < 0.0 || cx as usize >= self.dims[0] || cy as usize >= self.dims[1] {
            return None;
        }
        self.cells[cy as usize * self.dims[0] + cx as usize]
            .iter()
            .filter_map(|&i| z_over(&self.tris[i as usize], x, y))
            .filter(|z| (lo..=hi).contains(z))
            .reduce(f32::max)
    }

    pub fn triangles(&self) -> usize {
        self.tris.len()
    }
}

/// A shape with properties but no shader among them: an editor helper the renderer
/// leaves out (`nvf_assets`' `ShapeProps::is_helper` also asks for no textures, which
/// only a shader property names).
fn helper(file: &NifFile, shape: &fnv_nif::value::Struct) -> bool {
    let props = shape.refs("properties").unwrap_or_default();
    let shader = |&r: &i32| usize::try_from(r).ok().and_then(|r| file.blocks.get(r)).is_some_and(|b| b.type_name.ends_with("ShaderProperty") || b.type_name.starts_with("BSShader"));
    !props.is_empty() && !props.iter().any(shader)
}

/// Model point `v` through one node transform (scale, rotate, translate; rotation
/// row-major as nif.xml stores it applied to column vectors).
fn apply(t: &Transform, v: [f32; 3]) -> [f32; 3] {
    let r = &t.rotation;
    let s = t.scale;
    std::array::from_fn(|i| s * (r[3 * i] * v[0] + r[3 * i + 1] * v[1] + r[3 * i + 2] * v[2]) + t.translation[i])
}

/// Whether a triangle faces up or down more than sideways (strip winding does not say
/// which side is drawn, so both count).
fn floor_like(t: &[[f32; 3]; 3]) -> bool {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    len > 1e-9 && n[2].abs() / len >= FLOOR_MIN_NZ
}

/// The height of a triangle over (x, y), when (x, y) is inside it seen from above.
fn z_over(t: &[[f32; 3]; 3], x: f32, y: f32) -> Option<f32> {
    let [[x1, y1, z1], [x2, y2, z2], [x3, y3, z3]] = *t;
    let den = (y2 - y3) * (x1 - x3) + (x3 - x2) * (y1 - y3);
    if den.abs() < 1e-9 {
        return None;
    }
    let a = ((y2 - y3) * (x - x3) + (x3 - x2) * (y - y3)) / den;
    let b = ((y3 - y1) * (x - x3) + (x1 - x3) * (y - y3)) / den;
    let c = 1.0 - a - b;
    const EPS: f32 = -1e-4;
    (a >= EPS && b >= EPS && c >= EPS).then_some(a * z1 + b * z2 + c * z3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(z: f32) -> Vec<[[f32; 3]; 3]> {
        vec![[[0.0, 0.0, z], [100.0, 0.0, z], [100.0, 100.0, z]], [[0.0, 0.0, z], [100.0, 100.0, z], [0.0, 100.0, z]]]
    }

    #[test]
    fn highest_floor_in_the_window() {
        // A plank: drawn top at 5, underside at 0, and a wall (left out).
        let mut tris = quad(5.0);
        tris.extend(quad(0.0));
        tris.push([[0.0, 0.0, 0.0], [0.0, 0.0, 50.0], [0.0, 100.0, 0.0]]);
        let f = DrawnFloors::new(tris.into_iter().filter(floor_like).collect()).expect("floors");
        assert_eq!(f.triangles(), 4);
        assert_eq!(f.height_at(50.0, 30.0, -4.0, 6.0), Some(5.0));
        assert_eq!(f.height_at(50.0, 30.0, -4.0, 4.0), Some(0.0));
        assert_eq!(f.height_at(50.0, 30.0, 6.0, 9.0), None);
        assert_eq!(f.height_at(150.0, 30.0, -4.0, 6.0), None);
    }

    /// A synthetic NIF (20.2.0.7, user version 11, BS version 34; the layout of
    /// `crates/fnv-nif/tests/synthetic.rs` and `schemas/nif`, no game data): a root node
    /// over a 100 × 100 floor square at height 5, with a BSShaderPPLightingProperty when
    /// `shader`, and a property-less square at 50 under a `Capsule01` node (the lounge
    /// chair's set-up, issue #2469).
    fn chair(shader: bool) -> Vec<u8> {
        fn words(out: &mut Vec<u8>, v: &[u32]) {
            out.extend(v.iter().flat_map(|v| v.to_le_bytes()));
        }
        fn floats(out: &mut Vec<u8>, v: &[f32]) {
            out.extend(v.iter().flat_map(|v| v.to_le_bytes()));
        }
        fn av(name: u32, props: &[u32]) -> Vec<u8> {
            let mut out = vec![];
            words(&mut out, &[name, 0, u32::MAX, 14]);
            floats(&mut out, &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0]);
            words(&mut out, &[props.len() as u32]);
            words(&mut out, props);
            words(&mut out, &[u32::MAX]);
            out
        }
        fn node(name: u32, children: &[u32]) -> Vec<u8> {
            let mut out = av(name, &[]);
            words(&mut out, &[children.len() as u32]);
            words(&mut out, children);
            words(&mut out, &[0]);
            out
        }
        fn strips(name: u32, props: &[u32], data: u32) -> Vec<u8> {
            let mut out = av(name, props);
            words(&mut out, &[data, u32::MAX, 0, u32::MAX]);
            out.push(0);
            out
        }
        fn square(z: f32) -> Vec<u8> {
            let mut out = vec![];
            words(&mut out, &[0]);
            out.extend(4u16.to_le_bytes());
            out.extend([0, 0, 1]);
            floats(&mut out, &[0.0, 0.0, z, 100.0, 0.0, z, 0.0, 100.0, z, 100.0, 100.0, z]);
            out.extend(1u16.to_le_bytes());
            out.push(0);
            floats(&mut out, &[50.0, 50.0, z, 71.0]);
            out.push(0);
            floats(&mut out, &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
            out.extend(0u16.to_le_bytes());
            words(&mut out, &[u32::MAX]);
            for v in [2u16, 1, 4] {
                out.extend(v.to_le_bytes()); // triangles, strips, strip length
            }
            out.push(1); // has points
            for v in [0u16, 1, 2, 3] {
                out.extend(v.to_le_bytes());
            }
            out
        }
        let mut shader_block = vec![];
        words(&mut shader_block, &[u32::MAX, 0, u32::MAX]);
        shader_block.extend(1u16.to_le_bytes());
        words(&mut shader_block, &[1, 0, 0]);
        floats(&mut shader_block, &[1.0]);
        words(&mut shader_block, &[3, u32::MAX]);
        floats(&mut shader_block, &[0.0]);
        words(&mut shader_block, &[0]);
        floats(&mut shader_block, &[4.0, 1.0]);
        let (seat_props, block2): (&[u32], (&str, Vec<u8>)) =
            if shader { (&[2], ("BSShaderPPLightingProperty", shader_block)) } else { (&[], ("NiNode", node(u32::MAX, &[]))) };
        let blocks = [
            ("BSFadeNode", node(0, &[1, 3])),
            ("NiTriStrips", strips(1, seat_props, 5)),
            block2,
            ("NiNode", node(2, &[4])),
            ("NiTriStrips", strips(3, &[], 6)),
            ("NiTriStripsData", square(5.0)),
            ("NiTriStripsData", square(50.0)),
        ];
        let strings = ["Shape01", "Shape01:0", "Capsule01", "Capsule01:0"];
        let string = |out: &mut Vec<u8>, s: &str| {
            words(out, &[s.len() as u32]);
            out.extend(s.as_bytes());
        };
        let mut out = b"Gamebryo File Format, Version 20.2.0.7\n".to_vec();
        words(&mut out, &[0x1402_0007]);
        out.push(1);
        words(&mut out, &[11, blocks.len() as u32, 34]);
        out.extend([1, 0, 1, 0, 1, 0]);
        let mut types: Vec<&str> = vec![];
        for (kind, _) in &blocks {
            if !types.contains(kind) {
                types.push(kind);
            }
        }
        out.extend((types.len() as u16).to_le_bytes());
        for kind in &types {
            string(&mut out, kind);
        }
        for (kind, _) in &blocks {
            out.extend((types.iter().position(|t| t == kind).unwrap() as u16).to_le_bytes());
        }
        for (_, b) in &blocks {
            words(&mut out, &[b.len() as u32]);
        }
        words(&mut out, &[strings.len() as u32, 11]);
        for s in strings {
            string(&mut out, s);
        }
        words(&mut out, &[0]);
        for (_, b) in &blocks {
            out.extend(b);
        }
        words(&mut out, &[1, 0]);
        out
    }

    fn floors_of(bytes: &[u8]) -> DrawnFloors {
        let schema = fnv_nif::Schema::load_dir(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/nif")).unwrap();
        let file = NifFile::parse(bytes, Some(&schema)).unwrap();
        let dec = Decoded::new(&file, bytes, &schema);
        DrawnFloors::from_nif(&file, &dec).expect("floors")
    }

    #[test]
    fn bare_marker_shapes_are_no_floor() {
        // Issue #2469: beside a shaded seat, the property-less capsule is editor
        // geometry the renderer leaves out, so it is no drawn floor either.
        let f = floors_of(&chair(true));
        assert_eq!(f.triangles(), 2);
        assert_eq!(f.height_at(30.0, 20.0, 0.0, 100.0), Some(5.0));
        // In a model with no shader property anywhere both squares count.
        let f = floors_of(&chair(false));
        assert_eq!(f.triangles(), 4);
        assert_eq!(f.height_at(30.0, 20.0, 0.0, 100.0), Some(50.0));
    }

    #[test]
    fn large_models_get_larger_cells() {
        let tris = vec![[[0.0, 0.0, 1.0], [100_000.0, 0.0, 1.0], [0.0, 100_000.0, 1.0]]];
        let f = DrawnFloors::new(tris).expect("floors");
        assert!(f.dims[0] <= MAX_SIDE + 1 && f.dims[1] <= MAX_SIDE + 1);
        assert_eq!(f.height_at(10.0, 10.0, 0.0, 2.0), Some(1.0));
    }
}
