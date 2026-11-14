//! The landscape surface NVForever draws, collides with and measures heights on: the
//! LAND grid as stored, or that grid smoothed (#2149). One surface for all of them, so
//! nothing stands above or sinks into the drawn ground.
//!
//! **As stored** ([`Surface::stored`]): the 33 × 33 LAND vertices 128 units apart, each
//! grid square split along its south-west → north-east diagonal (the split the renderer
//! and the collision have always used, a [hypothesis] about the original;
//! docs/spec-notes/world.md §3).
//!
//! **Smoothed** ([`Surface::smoothed`]) [design, #2149; NVForever's own choice, not the
//! original's behaviour; the maintainer decided on 2026-11-13 that it is the default and
//! that collision follows it]: each grid square is cut into [`SUBDIV`] × [`SUBDIV`]
//! squares (split along the same diagonal) whose new vertices lie on a Catmull-Rom
//! bicubic patch through the LAND vertices, with their distance from the stored
//! triangles held to [`MAX_DEVIATION`]:
//! - every LAND vertex keeps its stored height (copied, not computed);
//! - the patch over a square reads the vertices one square beyond it, so it reads the
//!   neighbouring cells' LAND along the edges ([`Block`]): a vertex outside the cell is
//!   read from the cell that owns it (the one where it is not on the north or east edge),
//!   then from the other cell that has it; where no loaded cell has it, it carries on the
//!   lines of present vertices nearest to it ([`extrapolated`]: the same value whichever
//!   cell asks, and a plane carries on as a plane);
//! - a new vertex `(i, j)` is always worked out in square `(i / SUBDIV, j / SUBDIV)`, so
//!   the two cells sharing an edge work out its vertices from the same numbers in the
//!   same order and they come out identical;
//! - its height is the stored triangle's plus the patch's difference from it, clamped
//!   to ± [`MAX_DEVIATION`]: cliffs and sharp edges keep their shape;
//! - every new triangle lies inside one stored triangle (the stored diagonal runs along
//!   new diagonals), so the whole surface between the vertices is within the bound too;
//! - normals are central differences of the smoothed heights, with one vertex of margin
//!   beyond the cell worked out the same way, so they agree across cell edges.

use crate::land::{load_land, LandParts, HEIGHT_SCALE, SIDE, SPACING};
use fnv_esm::{LoadOrder, Schema};

/// New squares per grid square and side when smoothed [design, #2149: the issue's
/// "start at 2 × 2"; the frame cost of 4 × 4 is to be measured first].
pub const SUBDIV: usize = 2;

/// How far (world units, up or down) the smoothed surface may leave the stored triangles
/// [design, #2149: one stored height step (`HEIGHT_SCALE`), so the surface stays within
/// the data's own resolution and placed rocks, roads and foundations neither float nor
/// sink by more than that; to be tuned from pictures].
pub const MAX_DEVIATION: f32 = HEIGHT_SCALE;

/// Vertices read beyond the cell on each side when smoothing: one for the patches of the
/// edge squares, one more for the margin vertex the normals take.
const PAD: usize = 2;
const PSIDE: usize = SIDE + 2 * PAD;

/// The LAND heights (`SIDE * SIDE`, world units) of a cell and of the eight around it:
/// `[dy + 1][dx + 1]`, rows south → north; the cell itself at `[1][1]`. `None` where
/// there is no landscape (or it is not read).
pub type Block<'a> = [[Option<&'a [f32]>; 3]; 3];

/// LAND FormIDs of a cell and its eight neighbours, laid out as [`Block`].
pub type LandIds = [[Option<u32>; 3]; 3];

/// [`LandIds`] of grid square `g` from `land_of` (grid square → its LAND).
pub fn land_ids(g: (i32, i32), land_of: impl Fn((i32, i32)) -> Option<u32>) -> LandIds {
    std::array::from_fn(|r| std::array::from_fn(|c| land_of((g.0 + c as i32 - 1, g.1 + r as i32 - 1))))
}

/// Height of a grid surface at (`x`, `y`) from its south-west corner: `heights` is
/// `side * side`, row-major, rows south → north, `spacing` apart, each square split
/// along its south-west → north-east diagonal. `None` outside the grid.
pub fn grid_height_at(heights: &[f32], side: usize, spacing: f32, x: f32, y: f32) -> Option<f32> {
    triangle_height(|r, c| heights[r * side + c], side, spacing, x, y)
}

/// [`grid_height_at`] over the vertex heights `h(row, column)`.
fn triangle_height(h: impl Fn(usize, usize) -> f32, side: usize, spacing: f32, x: f32, y: f32) -> Option<f32> {
    let size = spacing * (side - 1) as f32;
    if !(0.0..=size).contains(&x) || !(0.0..=size).contains(&y) {
        return None;
    }
    let (fx, fy) = (x / spacing, y / spacing);
    let (c, r) = ((fx as usize).min(side - 2), (fy as usize).min(side - 2));
    let (u, v) = (fx - c as f32, fy - r as f32);
    let (sw, se, nw, ne) = (h(r, c), h(r, c + 1), h(r + 1, c), h(r + 1, c + 1));
    Some(if u >= v { sw + u * (se - sw) + v * (ne - se) } else { sw + v * (nw - sw) + u * (ne - nw) })
}

/// The triangles of a `side × side` grid, counter-clockwise seen from above (+z), each
/// square south-west → south-east → north-east, then south-west → north-east → north-west.
pub fn grid_triangles(side: usize) -> Vec<[u32; 3]> {
    let mut out = Vec::with_capacity((side - 1) * (side - 1) * 2);
    for r in 0..side - 1 {
        for c in 0..side - 1 {
            let sw = (r * side + c) as u32;
            let (se, nw) = (sw + 1, sw + side as u32);
            let ne = nw + 1;
            out.push([sw, se, ne]);
            out.push([sw, ne, nw]);
        }
    }
    out
}

/// A cell's landscape surface (module notes).
#[derive(Debug, Clone, PartialEq)]
pub struct Surface {
    /// Squares per LAND grid square and side: 1 as stored, [`SUBDIV`] smoothed.
    sub: usize,
    /// `side() * side()` heights, row-major, rows south → north.
    heights: Vec<f32>,
    /// Unit normals at the vertices, smoothed only (the stored grid is shaded by its
    /// VNML normals).
    normals: Option<Vec<[f32; 3]>>,
}

impl Surface {
    /// The LAND grid as stored (`heights`: `Land::heights`).
    pub fn stored(heights: &[f32]) -> Surface {
        assert_eq!(heights.len(), SIDE * SIDE, "LAND heights are {SIDE} x {SIDE}");
        Surface { sub: 1, heights: heights.to_vec(), normals: None }
    }

    /// The surface of the cell at the centre of `block`, smoothed or as stored; `None`
    /// when the centre has no heights.
    pub fn new(block: &Block, smooth: bool) -> Option<Surface> {
        if smooth {
            Surface::smoothed(block)
        } else {
            block[1][1].map(Surface::stored)
        }
    }

    /// The smoothed surface of the cell at the centre of `block` (module notes).
    pub fn smoothed(block: &Block) -> Option<Surface> {
        let p = padded(block)?;
        let sub = SUBDIV;
        let side = (SIDE - 1) * sub + 1;
        // Vertices with one of margin all round: (side + 2)².
        let mside = side + 2;
        let mut m = vec![0.0f32; mside * mside];
        for mi in 0..mside {
            for mj in 0..mside {
                m[mi * mside + mj] = smooth_vertex(&p, sub, mi as isize - 1, mj as isize - 1);
            }
        }
        let mut heights = Vec::with_capacity(side * side);
        let mut normals = Vec::with_capacity(side * side);
        let two_h = 2.0 * SPACING / sub as f32;
        for i in 0..side {
            for j in 0..side {
                let at = |di: isize, dj: isize| m[(i as isize + 1 + di) as usize * mside + (j as isize + 1 + dj) as usize];
                heights.push(at(0, 0));
                let n = [-(at(0, 1) - at(0, -1)) / two_h, -(at(1, 0) - at(-1, 0)) / two_h, 1.0];
                let len = (n[0] * n[0] + n[1] * n[1] + 1.0).sqrt();
                normals.push(n.map(|k| k / len));
            }
        }
        Some(Surface { sub, heights, normals: Some(normals) })
    }

    /// Squares per LAND grid square and side (1: as stored).
    pub fn sub(&self) -> usize {
        self.sub
    }

    pub fn is_smoothed(&self) -> bool {
        self.sub > 1
    }

    /// Vertices per side.
    pub fn side(&self) -> usize {
        (SIDE - 1) * self.sub + 1
    }

    /// Distance between neighbouring vertices (world units).
    pub fn spacing(&self) -> f32 {
        SPACING / self.sub as f32
    }

    /// The vertex heights, `side() * side()`, row-major, rows south → north.
    pub fn heights(&self) -> &[f32] {
        &self.heights
    }

    /// Unit normals at the vertices when smoothed; `None` as stored (VNML shades it).
    pub fn normals(&self) -> Option<&[[f32; 3]]> {
        self.normals.as_deref()
    }

    /// The index of LAND vertex (`r`, `c`) among [`Surface::heights`].
    pub fn land_vertex(&self, r: usize, c: usize) -> usize {
        r * self.sub * self.side() + c * self.sub
    }

    /// Cell-local position of vertex `i`.
    pub fn vertex(&self, i: usize) -> [f32; 3] {
        let side = self.side();
        [(i % side) as f32 * self.spacing(), (i / side) as f32 * self.spacing(), self.heights[i]]
    }

    /// The triangles over [`Surface::heights`] ([`grid_triangles`]).
    pub fn triangles(&self) -> Vec<[u32; 3]> {
        grid_triangles(self.side())
    }

    /// Height at cell-local (`x`, `y`) on the triangles the surface is drawn and collided
    /// as; `None` outside the cell.
    pub fn height_at(&self, x: f32, y: f32) -> Option<f32> {
        grid_height_at(&self.heights, self.side(), self.spacing(), x, y)
    }

    /// Height at cell-local (`x`, `y`) on the stored LAND triangles, from the LAND vertices
    /// the surface keeps (the same as [`Surface::height_at`] when not smoothed); `None`
    /// outside the cell.
    pub fn stored_height_at(&self, x: f32, y: f32) -> Option<f32> {
        triangle_height(|r, c| self.heights[self.land_vertex(r, c)], SIDE, SPACING, x, y)
    }

    /// How far the surface lies above the stored LAND triangles at cell-local (`x`, `y`)
    /// (0 when not smoothed; at most [`MAX_DEVIATION`] either way).
    pub fn lift_at(&self, x: f32, y: f32) -> Option<f32> {
        if !self.is_smoothed() {
            return self.height_at(x, y).map(|_| 0.0);
        }
        Some(self.height_at(x, y)? - self.stored_height_at(x, y)?)
    }

    /// [`Surface::height_at`] with the point brought into the cell first.
    pub fn height_at_clamped(&self, x: f32, y: f32) -> f32 {
        let size = crate::CELL_SIZE;
        self.height_at(x.clamp(0.0, size), y.clamp(0.0, size)).unwrap_or(f32::NAN)
    }

    /// Unit normal at cell-local (`x`, `y`) when smoothed: the vertex normals blended
    /// between the four vertices around the point. `None` as stored.
    pub fn normal_at(&self, x: f32, y: f32) -> Option<[f32; 3]> {
        let nm = self.normals.as_ref()?;
        let side = self.side();
        let max = (side - 1) as f32;
        let (u, v) = ((x / self.spacing()).clamp(0.0, max), (y / self.spacing()).clamp(0.0, max));
        let (c, r) = ((u.floor() as usize).min(side - 2), (v.floor() as usize).min(side - 2));
        let (fx, fy) = (u - c as f32, v - r as f32);
        let at = |r: usize, c: usize| nm[r * side + c];
        let mut n = [0.0f32; 3];
        for (w, k) in [((1.0 - fx) * (1.0 - fy), at(r, c)), (fx * (1.0 - fy), at(r, c + 1)), ((1.0 - fx) * fy, at(r + 1, c)), (fx * fy, at(r + 1, c + 1))] {
            for a in 0..3 {
                n[a] += w * k[a];
            }
        }
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        Some(if len > 1e-6 { n.map(|k| k / len) } else { [0.0, 0.0, 1.0] })
    }
}

/// The heights of the LAND records `ids` (only the centre's unless `neighbours`).
pub fn load_heights(lo: &LoadOrder, schema: &Schema, ids: &LandIds, neighbours: bool) -> [[Option<Vec<f32>>; 3]; 3] {
    std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            if !neighbours && (r, c) != (1, 1) {
                return None;
            }
            ids[r][c].and_then(|id| load_land(lo, schema, id, LandParts::HEIGHTS)).and_then(|l| l.heights)
        })
    })
}

/// [`Block`] over heights held in `v`.
pub fn block_of(v: &[[Option<Vec<f32>>; 3]; 3]) -> Block<'_> {
    std::array::from_fn(|r| std::array::from_fn(|c| v[r][c].as_deref()))
}

/// The surface of the cell whose LAND and neighbours' LAND are `ids` (centre at
/// `[1][1]`), smoothed or as stored; `None` without heights.
pub fn load_surface(lo: &LoadOrder, schema: &Schema, ids: &LandIds, smooth: bool) -> Option<Surface> {
    let h = load_heights(lo, schema, ids, smooth);
    Surface::new(&block_of(&h), smooth)
}

/// The cell's heights with [`PAD`] vertices more on each side (module notes): `PSIDE²`,
/// index `(r + PAD) * PSIDE + (c + PAD)` for LAND row `r`, column `c` (-PAD..=32+PAD).
fn padded(block: &Block) -> Option<Vec<f32>> {
    let centre = block[1][1]?;
    assert_eq!(centre.len(), SIDE * SIDE, "LAND heights are {SIDE} x {SIDE}");
    let last = (SIDE - 1) as isize;
    let pad = PAD as isize;
    let idx = |r: isize, c: isize| (r + pad) as usize * PSIDE + (c + pad) as usize;
    let inside = |k: isize| (0..=last).contains(&k);
    let mut p = vec![f32::NAN; PSIDE * PSIDE];
    for r in -pad..=last + pad {
        for c in -pad..=last + pad {
            let v = if inside(r) && inside(c) { Some(centre[r as usize * SIDE + c as usize]) } else { world_vertex(block, r, c) };
            if let Some(v) = v.filter(|v| v.is_finite()) {
                p[idx(r, c)] = v;
            }
        }
    }
    // Missing (no loaded cell has it): extrapolated from the present vertices along the
    // eight grid directions, by a rule of the world vertex and of which cells are present
    // only, so every cell that reads it gets the same value (module notes).
    let present = |r: isize, c: isize| -> Option<f32> {
        if inside(r) && inside(c) {
            Some(centre[r as usize * SIDE + c as usize])
        } else {
            world_vertex(block, r, c).filter(|v| v.is_finite())
        }
    };
    for r in -pad..=last + pad {
        for c in -pad..=last + pad {
            if p[idx(r, c)].is_nan() {
                p[idx(r, c)] = extrapolated(&present, r, c);
            }
        }
    }
    Some(p)
}

/// The grid directions [`extrapolated`] looks along, in the order it adds them up
/// (rows first: south, north, west, east, then the diagonals).
const DIRECTIONS: [(isize, isize); 8] = [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (-1, 1), (1, -1), (1, 1)];

/// A vertex no loaded cell has, at (`r`, `c`): for the least step `k` (1 or 2) at which any
/// direction has present vertices `k` and `2k` steps away, the mean over those directions
/// of the line through them carried on to the vertex (`2·a − b`), added up in
/// [`DIRECTIONS`]' order. Every vertex within [`PAD`] of the cell has such a line into the
/// cell itself by `k` = 2, so the search always ends there; a plane carries on as a plane.
/// It reads only present vertices by their place in the world, so two cells asking for
/// the same vertex get the same number (their shared edges' normals agree too).
fn extrapolated(present: &impl Fn(isize, isize) -> Option<f32>, r: isize, c: isize) -> f32 {
    for k in 1..=PAD as isize {
        let (mut sum, mut n) = (0.0f32, 0u32);
        for (dr, dc) in DIRECTIONS {
            if let (Some(a), Some(b)) = (present(r + k * dr, c + k * dc), present(r + 2 * k * dr, c + 2 * k * dc)) {
                sum += 2.0 * a - b;
                n += 1;
            }
        }
        if n > 0 {
            return sum / n as f32;
        }
    }
    // Not reached for vertices within PAD of the cell (above); the nearest cell vertex.
    let last = (SIDE - 1) as isize;
    present(r.clamp(0, last), c.clamp(0, last)).unwrap_or(0.0)
}

/// LAND vertex (`r`, `c`) of the centre cell's frame, outside it: from the neighbouring
/// cell that owns it, else the other one that has it (module notes).
fn world_vertex(block: &Block, r: isize, c: isize) -> Option<f32> {
    let n = (SIDE - 1) as isize;
    // (cell offset, local index) choices along one axis: the owner (local 0..n-1) first,
    // then the cell below for a vertex on its last row.
    let options = |k: isize| {
        let own = (k.div_euclid(n), k.rem_euclid(n));
        let other = (own.1 == 0).then_some((own.0 - 1, n));
        (own, other)
    };
    let (ro, ra) = options(r);
    let (co, ca) = options(c);
    let candidates = [Some((ro, co)), ca.map(|ca| (ro, ca)), ra.map(|ra| (ra, co)), ra.zip(ca)];
    candidates.into_iter().flatten().find_map(|((dy, lr), (dx, lc))| {
        if !(-1..=1).contains(&dy) || !(-1..=1).contains(&dx) {
            return None;
        }
        let h = block[(dy + 1) as usize][(dx + 1) as usize]?;
        Some(h[lr as usize * SIDE + lc as usize])
    })
}

/// Catmull-Rom weights of the four vertices around parameter `t` in 0..1 (the second
/// and third are the square's ends; at 0 they are exactly 0, 1, 0, 0).
fn catmull_rom(t: f32) -> [f32; 4] {
    let (t2, t3) = (t * t, t * t * t);
    [0.5 * (-t3 + 2.0 * t2 - t), 0.5 * (3.0 * t3 - 5.0 * t2 + 2.0), 0.5 * (-3.0 * t3 + 4.0 * t2 + t), 0.5 * (t3 - t2)]
}

/// Smoothed height of new vertex (`i`, `j`) (rows, columns; -1..=side may lie in the
/// margin) over the padded grid `p`.
fn smooth_vertex(p: &[f32], sub: usize, i: isize, j: isize) -> f32 {
    let s = sub as isize;
    let (r, c) = (i.div_euclid(s), j.div_euclid(s));
    let (ri, ci) = (i.rem_euclid(s), j.rem_euclid(s));
    let pad = PAD as isize;
    let at = |r: isize, c: isize| p[(r + pad) as usize * PSIDE + (c + pad) as usize];
    if ri == 0 && ci == 0 {
        return at(r, c);
    }
    let (u, v) = (ci as f32 / sub as f32, ri as f32 / sub as f32);
    // The stored triangle under the point.
    let (sw, se, nw, ne) = (at(r, c), at(r, c + 1), at(r + 1, c), at(r + 1, c + 1));
    let flat = if u >= v { sw + u * (se - sw) + v * (ne - se) } else { sw + v * (nw - sw) + u * (ne - nw) };
    // The bicubic patch: along each of the four rows, then across them.
    let (wu, wv) = (catmull_rom(u), catmull_rom(v));
    let mut patch = 0.0f32;
    for (k, wr) in wv.iter().enumerate() {
        let rr = r - 1 + k as isize;
        let mut row = 0.0f32;
        for (m, wc) in wu.iter().enumerate() {
            row += wc * at(rr, c - 1 + m as isize);
        }
        patch += wr * row;
    }
    flat + (patch - flat).clamp(-MAX_DEVIATION, MAX_DEVIATION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CELL_SIZE;

    /// A hilly world: heights at world vertex (X, Y) (grid units), in steps of 8 as
    /// VHGT stores them, with ridges across the split diagonal.
    fn world(x: i32, y: i32) -> f32 {
        let (x, y) = (x as f32, y as f32);
        let h = 300.0 * (x * 0.21).sin() * (y * 0.13).cos() + 90.0 * ((x - y) * 0.6).sin() + 7.0 * x - 3.0 * y + if (x as i32 + 2 * y as i32).rem_euclid(5) == 0 { 40.0 } else { 0.0 };
        (h / HEIGHT_SCALE).round() * HEIGHT_SCALE
    }

    /// The LAND heights of cell `g` cut from [`world`].
    fn cell(g: (i32, i32)) -> Vec<f32> {
        cell_of(world, g)
    }

    /// The LAND heights of cell `g` cut from the world heights `f`.
    fn cell_of(f: fn(i32, i32) -> f32, g: (i32, i32)) -> Vec<f32> {
        let n = (SIDE - 1) as i32;
        (0..SIDE * SIDE).map(|k| f(g.0 * n + (k % SIDE) as i32, g.1 * n + (k / SIDE) as i32)).collect()
    }

    /// The heights around `g`, without the cells `missing` names.
    fn around(g: (i32, i32), missing: &[(i32, i32)]) -> [[Option<Vec<f32>>; 3]; 3] {
        around_of(world, g, missing)
    }

    fn around_of(f: fn(i32, i32) -> f32, g: (i32, i32), missing: &[(i32, i32)]) -> [[Option<Vec<f32>>; 3]; 3] {
        std::array::from_fn(|r| {
            std::array::from_fn(|c| {
                let at = (g.0 + c as i32 - 1, g.1 + r as i32 - 1);
                (!missing.contains(&at)).then(|| cell_of(f, at))
            })
        })
    }

    fn smoothed(g: (i32, i32), missing: &[(i32, i32)]) -> Surface {
        smoothed_of(world, g, missing)
    }

    fn smoothed_of(f: fn(i32, i32) -> f32, g: (i32, i32), missing: &[(i32, i32)]) -> Surface {
        Surface::smoothed(&block_of(&around_of(f, g, missing))).unwrap()
    }

    /// Both sides of the east edge of (0, 0) and of its north edge, heights and normals.
    fn assert_edges_agree(f: fn(i32, i32) -> f32, missing: &[(i32, i32)]) {
        let side = (SIDE - 1) * SUBDIV + 1;
        let last = side - 1;
        let a = smoothed_of(f, (0, 0), missing);
        let present = |g: (i32, i32)| !missing.contains(&g);
        let north = present((0, 1)).then(|| smoothed_of(f, (0, 1), missing));
        let east = present((1, 0)).then(|| smoothed_of(f, (1, 0), missing));
        for k in 0..side {
            if let Some(north) = &north {
                let (top, bottom) = (last * side + k, k);
                assert_eq!(a.heights()[top], north.heights()[bottom], "north edge {k} ({missing:?})");
                assert_eq!(a.normals().unwrap()[top], north.normals().unwrap()[bottom], "north normal {k} ({missing:?})");
            }
            if let Some(east) = &east {
                let (right, left) = (k * side + last, k * side);
                assert_eq!(a.heights()[right], east.heights()[left], "east edge {k} ({missing:?})");
                assert_eq!(a.normals().unwrap()[right], east.normals().unwrap()[left], "east normal {k} ({missing:?})");
            }
        }
        assert!(north.is_some() || east.is_some());
    }

    #[test]
    fn stored_is_the_land_grid_and_its_triangles() {
        let h = cell((3, -2));
        let s = Surface::stored(&h);
        assert_eq!((s.side(), s.spacing(), s.sub(), s.is_smoothed()), (SIDE, SPACING, 1, false));
        assert_eq!(s.heights(), &h[..]);
        assert!(s.normals().is_none() && s.normal_at(10.0, 10.0).is_none());
        // Off: `new` gives exactly the stored grid, whatever the neighbours.
        assert_eq!(Surface::new(&block_of(&around((3, -2), &[])), false).unwrap(), s);
        assert_eq!(s.triangles().len(), (SIDE - 1) * (SIDE - 1) * 2);
        // The diagonal: a square's middle is the mean of its south-west and north-east.
        let mid = s.height_at(SPACING * 0.5, SPACING * 0.5).unwrap();
        assert!((mid - (h[0] + h[SIDE + 1]) / 2.0).abs() < 1e-4);
        assert_eq!(s.height_at(-1.0, 5.0), None);
        assert_eq!(s.height_at_clamped(-50.0, 0.0), h[0]);
    }

    #[test]
    fn every_land_vertex_keeps_its_height() {
        let h = cell((0, 0));
        let s = smoothed((0, 0), &[]);
        assert_eq!((s.side(), s.spacing()), ((SIDE - 1) * SUBDIV + 1, SPACING / SUBDIV as f32));
        for r in 0..SIDE {
            for c in 0..SIDE {
                assert_eq!(s.heights()[s.land_vertex(r, c)], h[r * SIDE + c], "LAND vertex ({r}, {c})");
                let p = s.vertex(s.land_vertex(r, c));
                assert_eq!((p[0], p[1]), (c as f32 * SPACING, r as f32 * SPACING));
            }
        }
    }

    #[test]
    fn the_surface_stays_within_the_bound_of_the_stored_triangles() {
        let h = cell((1, 1));
        let stored = Surface::stored(&h);
        let s = smoothed((1, 1), &[]);
        let mut moved = 0;
        for i in 0..s.heights().len() {
            let [x, y, z] = s.vertex(i);
            let d = z - stored.height_at(x, y).unwrap();
            assert!(d.abs() <= MAX_DEVIATION + 1e-3, "vertex {i}: {d}");
            moved += usize::from(d.abs() > 0.5);
        }
        assert!(moved > 1000, "the hills are rounded: {moved} vertices moved");
        // Between the vertices too: every new triangle lies in one stored triangle.
        for k in 0..2000 {
            let x = (k as f32 * 37.77) % CELL_SIZE;
            let y = (k as f32 * 91.31 + 5.0) % CELL_SIZE;
            let d = s.height_at(x, y).unwrap() - stored.height_at(x, y).unwrap();
            assert!(d.abs() <= MAX_DEVIATION + 1e-3, "at ({x}, {y}): {d}");
        }
    }

    #[test]
    fn the_stored_triangles_and_the_lift_are_read_from_the_land_vertices() {
        let h = cell((1, 1));
        let stored = Surface::stored(&h);
        let s = smoothed((1, 1), &[]);
        for k in 0..500 {
            let (x, y) = ((k as f32 * 37.77) % CELL_SIZE, (k as f32 * 91.31 + 5.0) % CELL_SIZE);
            assert_eq!(s.stored_height_at(x, y), stored.height_at(x, y));
            assert_eq!(stored.lift_at(x, y), Some(0.0));
            let lift = s.lift_at(x, y).unwrap();
            assert!(lift.abs() <= MAX_DEVIATION + 1e-3 && (s.height_at(x, y).unwrap() - stored.height_at(x, y).unwrap() - lift).abs() < 1e-3);
        }
        assert_eq!(s.lift_at(-1.0, 0.0), None);
    }

    #[test]
    fn a_plane_stays_a_plane_with_or_without_neighbours() {
        let plane = |x: f32, y: f32| 1000.0 + 0.3 * x - 0.125 * y;
        let h: Vec<f32> = (0..SIDE * SIDE).map(|k| plane((k % SIDE) as f32 * SPACING, (k / SIDE) as f32 * SPACING)).collect();
        let mut block: Block = [[None; 3]; 3];
        block[1][1] = Some(&h);
        let s = Surface::smoothed(&block).unwrap();
        let want = {
            let n = [-0.3f32, 0.125, 1.0];
            let len = (n[0] * n[0] + n[1] * n[1] + 1.0).sqrt();
            n.map(|k| k / len)
        };
        for i in 0..s.heights().len() {
            let [x, y, z] = s.vertex(i);
            assert!((z - plane(x, y)).abs() < 1e-2, "vertex {i}");
            let n = s.normals().unwrap()[i];
            assert!((0..3).all(|a| (n[a] - want[a]).abs() < 1e-4), "normal {i}: {n:?}");
        }
    }

    /// Shared edges come out identical from both sides, heights and normals, also with
    /// cells missing around them.
    #[test]
    fn shared_cell_edges_are_identical_from_both_sides() {
        for missing in [&[][..], &[(-1, 1), (1, -1)][..], &[(-1, 0), (-1, 1), (2, 1)][..], &[(0, 1)][..], &[(1, 1), (-1, -1)][..]] {
            assert_edges_agree(world, missing);
        }
        // The corner four cells share.
        let side = (SIDE - 1) * SUBDIV + 1;
        let ne = smoothed((1, 1), &[]);
        assert_eq!(smoothed((0, 0), &[]).heights()[side * side - 1], ne.heights()[0]);
    }

    /// A missing cell is filled in the same way whichever cell reads it: with heights
    /// 8 (Y - 32)² and the cell north of (0, 0) missing (the one north-east of it there),
    /// (0, 0) and (1, 0) once disagreed on the normals along their edge (review of #2149).
    #[test]
    fn a_missing_neighbour_is_filled_the_same_from_both_sides() {
        fn bowl(_x: i32, y: i32) -> f32 {
            8.0 * ((y - 32) * (y - 32)) as f32
        }
        assert_edges_agree(bowl, &[(0, 1)]);
        assert_edges_agree(bowl, &[(0, 1), (-1, 0)]);
        assert_edges_agree(world, &[(0, 1), (1, -1), (-1, 1)]);
    }

    #[test]
    fn a_cell_without_neighbours_is_smoothed_too() {
        let h = cell((5, 5));
        let mut block: Block = [[None; 3]; 3];
        block[1][1] = Some(&h);
        let s = Surface::smoothed(&block).unwrap();
        assert!(s.heights().iter().chain(s.normals().unwrap().iter().flatten()).all(|v| v.is_finite()));
        assert_eq!(s.heights()[s.land_vertex(32, 32)], h[SIDE * SIDE - 1]);
        assert!(Surface::smoothed(&[[None; 3]; 3]).is_none());
    }

    #[test]
    fn normals_are_unit_and_follow_the_slope() {
        let s = smoothed((2, 0), &[]);
        for (i, n) in s.normals().unwrap().iter().enumerate() {
            assert!(((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() - 1.0).abs() < 1e-4 && n[2] > 0.0, "normal {i}");
        }
        let n = s.normal_at(1000.0, 1000.0).unwrap();
        assert!((n[0] * n[0] + n[1] * n[1] + n[2] * n[2] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn neighbours_are_laid_out_by_offset() {
        let ids = land_ids((10, -4), |(x, y)| Some((x * 100 + y) as u32));
        assert_eq!(ids[1][1], Some(996));
        assert_eq!(ids[0][0], Some(895));
        assert_eq!(ids[2][1], Some(997));
        assert_eq!(ids[1][2], Some(1096));
    }
}
