//! Terrain collision: one triangle mesh per exterior cell from its landscape surface
//! (`fnv_world::land_surface`, docs/spec-notes/world.md §3): the LAND grid as stored, or
//! smoothed (#2149), the same surface the renderer draws.
//!
//! Vertices sit on the surface's grid (the 33 × 33 LAND grid, 128 units apart, or its
//! smoothed subdivision), relative to the cell's south-west corner (the collider is
//! translated there). Each grid square is split along its south-west → north-east
//! diagonal, the split the renderer uses (engine terrain, a [hypothesis] about the
//! original there too); [`height_at`] and `Surface::height_at` state the same surface
//! analytically and the tests check that the two agree. A triangle mesh rather than
//! rapier's heightfield: rapier 0.36's heightfield misses vertical rays that run exactly
//! along some grid lines [observed, physics.md §3], and ground probes at whole-grid
//! positions are common.

use fnv_world::land::{SIDE, SPACING};
use fnv_world::land_surface::{grid_height_at, grid_triangles, Surface};
use fnv_world::CELL_SIZE;
use rapier3d::geometry::{ColliderBuilder, TriMeshFlags};
use rapier3d::math::Vector;

/// Triangles of the LAND grid as stored, counter-clockwise seen from above (+z).
pub fn triangles() -> Vec<[u32; 3]> {
    grid_triangles(SIDE)
}

/// The terrain collider of the exterior cell at `grid`, placed in world space, on the LAND
/// grid as stored. `heights` is `fnv_world::Land::heights`: `SIDE * SIDE` world-unit
/// heights, row-major, rows south → north, columns west → east, from the cell's south-west
/// corner. Lengths are multiplied by `k` (rapier's internal unit per game unit, `PhysWorld`).
pub fn collider(grid: (i32, i32), heights: &[f32], k: f32) -> ColliderBuilder {
    assert_eq!(heights.len(), SIDE * SIDE, "LAND heights are {SIDE} x {SIDE}");
    collider_surface(grid, &Surface::stored(heights), k)
}

/// The terrain collider of the exterior cell at `grid` on `surface` (stored or smoothed),
/// placed in world space; lengths multiplied by `k` as in [`collider`].
pub fn collider_surface(grid: (i32, i32), surface: &Surface, k: f32) -> ColliderBuilder {
    let vertices = (0..surface.heights().len())
        .map(|i| {
            let [x, y, z] = surface.vertex(i);
            Vector::new(x, y, z) * k
        })
        .collect();
    let origin = Vector::new(grid.0 as f32 * CELL_SIZE, grid.1 as f32 * CELL_SIZE, 0.0) * k;
    ColliderBuilder::trimesh_with_flags(vertices, surface.triangles(), TriMeshFlags::FIX_INTERNAL_EDGES)
        .expect("the landscape grid is a valid mesh")
        .translation(origin)
}

/// Terrain height at (`x`, `y`) relative to the cell's south-west corner, on the surface
/// [`collider`] builds (`None` outside the cell). Independent of rapier, for checks.
pub fn height_at(heights: &[f32], x: f32, y: f32) -> Option<f32> {
    grid_height_at(heights, SIDE, SPACING, x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier3d::prelude::*;

    /// A bumpy, asymmetric surface so that axis swaps, flips and the diagonal all show.
    pub fn bumpy() -> Vec<f32> {
        (0..SIDE * SIDE)
            .map(|k| {
                let (r, c) = ((k / SIDE) as f32, (k % SIDE) as f32);
                3.0 * r + 0.5 * c + 40.0 * ((r * 0.7).sin() * (c * 1.3).cos()) + if (r as usize + c as usize).is_multiple_of(3) { 25.0 } else { 0.0 }
            })
            .collect()
    }

    #[test]
    fn analytic_corners_and_diagonal() {
        let h = bumpy();
        assert_eq!(height_at(&h, 0.0, 0.0), Some(h[0]));
        assert_eq!(height_at(&h, CELL_SIZE, 0.0), Some(h[SIDE - 1]));
        assert_eq!(height_at(&h, 0.0, CELL_SIZE), Some(h[(SIDE - 1) * SIDE]));
        assert_eq!(height_at(&h, CELL_SIZE, CELL_SIZE), Some(h[SIDE * SIDE - 1]));
        // Midpoint of a square lies on the SW–NE diagonal: the mean of SW and NE.
        let mid = height_at(&h, SPACING * 0.5, SPACING * 0.5).unwrap();
        assert!((mid - (h[0] + h[SIDE + 1]) / 2.0).abs() < 1e-4);
        assert_eq!(height_at(&h, -1.0, 5.0), None);
    }

    #[test]
    fn heightfield_matches_analytic_surface() {
        let h = bumpy();
        let grid = (-3, 2);
        let mut world = PhysicsWorld::default();
        world.insert_collider(collider(grid, &h, 1.0), None);
        world.detect_collisions(&(), &());
        let (x0, y0) = (grid.0 as f32 * CELL_SIZE, grid.1 as f32 * CELL_SIZE);
        let mut n = 0;
        // Points inside squares (both triangles) and on vertex columns; the outer border
        // belongs to the neighbouring cell as much as to this one.
        for k in 0..400 {
            let x = 1.0 + (k as f32 * 97.31) % (CELL_SIZE - 2.0);
            let y = 1.0 + (k as f32 * 61.17 + 13.0) % (CELL_SIZE - 2.0);
            for (x, y) in [(x, y), ((k % 31 + 1) as f32 * SPACING, y)] {
                let want = height_at(&h, x, y).unwrap();
                let ray = Ray::new(Vector::new(x0 + x, y0 + y, 10_000.0), Vector::new(0.0, 0.0, -1.0));
                let (_, toi) = world.cast_ray(&ray, 20_000.0, true, QueryFilter::default()).unwrap_or_else(|| panic!("ray misses terrain at ({x}, {y})"));
                let got = 10_000.0 - toi;
                assert!((got - want).abs() < 0.05, "at ({x}, {y}): physics {got}, analytic {want}");
                n += 1;
            }
        }
        assert_eq!(n, 800);
    }

    /// The smoothed surface collides where it is drawn: rays down hit its own heights.
    #[test]
    fn smoothed_collider_matches_the_surface() {
        let h = bumpy();
        let mut block: fnv_world::land_surface::Block = [[None; 3]; 3];
        block[1][1] = Some(&h);
        let s = Surface::smoothed(&block).unwrap();
        let grid = (4, -1);
        let mut world = PhysicsWorld::default();
        world.insert_collider(collider_surface(grid, &s, 1.0), None);
        world.detect_collisions(&(), &());
        let (x0, y0) = (grid.0 as f32 * CELL_SIZE, grid.1 as f32 * CELL_SIZE);
        for k in 0..400 {
            let x = 1.0 + (k as f32 * 97.31) % (CELL_SIZE - 2.0);
            let y = 1.0 + (k as f32 * 61.17 + 13.0) % (CELL_SIZE - 2.0);
            for (x, y) in [(x, y), ((k % 63 + 1) as f32 * s.spacing(), y)] {
                let want = s.height_at(x, y).unwrap();
                let ray = Ray::new(Vector::new(x0 + x, y0 + y, 10_000.0), Vector::new(0.0, 0.0, -1.0));
                let (_, toi) = world.cast_ray(&ray, 20_000.0, true, QueryFilter::default()).unwrap_or_else(|| panic!("ray misses terrain at ({x}, {y})"));
                let got = 10_000.0 - toi;
                assert!((got - want).abs() < 0.05, "at ({x}, {y}): physics {got}, surface {want}");
            }
        }
    }

    /// Off: the stored surface gives the collider it always did.
    #[test]
    fn stored_surface_is_the_land_grid() {
        let h = bumpy();
        let s = Surface::stored(&h);
        assert_eq!(s.triangles(), triangles());
        for (i, &z) in h.iter().enumerate() {
            assert_eq!(s.vertex(i), [(i % SIDE) as f32 * SPACING, (i / SIDE) as f32 * SPACING, z]);
        }
    }
}
