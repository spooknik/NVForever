//! The Local Map's outline of the place (docs/spec-notes/pipboy.md §5.3; the owner's
//! playtest list 5, item 52: "the local map is not rendering anything").
//!
//! The original renders the area from above [observed: its Local Map screenshot]; this
//! draws a plan of the collision the physics world holds instead [design]: a grid of rays
//! cast straight down, a few hundred a frame until the grid is done, then one picture.
//!
//! - **Indoors** the rays start a little over the player's head ([`INTERIOR_START`]) so
//!   the ceiling and the floors above are not seen; a sample whose ray lands near the
//!   player's feet is floor, one landing higher is something standing on it (furniture,
//!   a counter), one landing nowhere (or far below) is outside the rooms. The floor is
//!   filled faintly, what stands on it a little brighter, and the edge between the rooms
//!   and the outside (or a step up of [`EDGE_STEP`]) is drawn bright: the walls.
//! - **Outdoors** the rays start high over the player ([`EXTERIOR_START`]); the ground is
//!   shaded by its slope as lit from the north-west, and a step of [`EDGE_STEP`] between
//!   neighbours (a building's wall, a cliff) is drawn bright.
//!
//! The physics world holds only the 3×3 cells round the player (`phys::cells`), so a ray
//! outdoors that lands nowhere is answered by the cell's landscape heights instead (the
//! caller's `ray`): the 5×5 cells round the player, as the original's local map shows
//! [observed: uGridsToLoad=5, the owner's INI], are drawn, their buildings only where
//! the physics world holds them.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

// The checked admission API is a prerequisite; a recipient adapter is not wired yet.
#[allow(dead_code)]
mod grid;
use grid::OutlineGrid;

/// The local map's scale indoors, game units to a canvas unit at a magnification of 1
/// [design: outdoors it is 28, fitted to the owner's screenshot; a house at that scale is
/// a speck, so indoors the map is drawn 3.5 times closer].
pub const INTERIOR_SCALE: f32 = 8.0;
/// Samples apart, game units [design: about 3.4 canvas units at a magnification of 1,
/// indoors and out].
pub const INTERIOR_STEP: f32 = 28.0;
pub const EXTERIOR_STEP: f32 = 96.0;
/// The grid covers the map's window at a magnification of 1 and this much more each way
/// [design].
const MARGIN: f32 = 1.15;
/// Outdoors the grid spans at least this many cells each way, game units: the 5×5 cells
/// round the player and the margin [design].
const EXTERIOR_SPAN: f32 = 5.0 * 4096.0 * MARGIN;
/// Where the rays start above the player's feet and how far down they reach, game units
/// [design: under an ordinary ceiling indoors; over the hills round the player outdoors].
pub const INTERIOR_START: f32 = 150.0;
pub const INTERIOR_REACH: f32 = 450.0;
pub const EXTERIOR_START: f32 = 8000.0;
pub const EXTERIOR_REACH: f32 = 20000.0;
/// Indoors a sample is floor when its ray lands within this of the player's feet, below
/// or above [design: a stair's steps and a rug count as floor].
const FLOOR_BAND: (f32, f32) = (120.0, 40.0);
/// A height step down to a neighbour drawn as an edge (on the higher side), game units:
/// indoors furniture and ledges, outdoors only walls and cliffs, not the hills' slopes
/// at the wider spacing [design].
pub const EDGE_STEP: f32 = 64.0;
pub const EXTERIOR_EDGE_STEP: f32 = 192.0;
/// Alpha of the fills and the edge (the picture is tinted amber where drawn) [design].
const FLOOR_ALPHA: u8 = 45;
const RAISED_ALPHA: u8 = 100;
const EDGE_ALPHA: u8 = 210;
/// Outdoors: the darkest and brightest ground's alpha [design].
const GROUND_ALPHA: (f32, f32) = (6.0, 50.0);

/// The player's height steps that make a new grid (another floor), game units [design].
const FEET_SNAP: f32 = 128.0;

/// The game units a grid covers across and down: the window at a magnification of 1 and
/// the margin, outdoors at least [`EXTERIOR_SPAN`].
fn extent(interior: bool, window: Vec2, scale: f32) -> Vec2 {
    let e = window * scale * MARGIN;
    if interior {
        e
    } else {
        e.max(Vec2::splat(EXTERIOR_SPAN))
    }
}

/// What a grid was made for (see [`Outline::key`]).
pub type OutlineKey = (Option<u32>, u32, [i32; 3]);

/// The grid of samples and the picture made of it.
#[derive(Debug)]
pub struct Outline {
    /// What the grid was made for: the interior (or `None` outdoors and the worldspace)
    /// and the grid's anchor (game x, y of its middle, snapped, and the feet's height to
    /// [`FEET_SNAP`]).
    pub key: OutlineKey,
    /// The grid's top left corner, game x and y (y the largest).
    pub origin: [f32; 2],
    pub step: f32,
    pub size: [usize; 2],
    pub interior: bool,
    /// The player's feet's height when the grid was begun.
    pub feet: f32,
    /// Where each ray landed (row by row from the top), as far as the grid is done.
    pub heights: Vec<Option<f32>>,
    pub image: Option<Handle<Image>>,
}

impl Outline {
    /// A grid for the map's window (`window`, canvas units) at `scale` game units a unit,
    /// round `centre` (game x, y), anchored to a lattice a quarter of its size so a little
    /// panning keeps it.
    pub fn new(interior: Option<u32>, world: u32, centre: [f32; 2], feet: f32, window: Vec2, scale: f32) -> Outline {
        let o = Outline::sized(interior, world, centre, feet, window, scale);
        Outline { heights: Vec::with_capacity(o.size[0] * o.size[1]), ..o }
    }

    /// [`Outline::new`]'s grid with no room made for its samples.
    fn sized(interior: Option<u32>, world: u32, centre: [f32; 2], feet: f32, window: Vec2, scale: f32) -> Outline {
        let grid = OutlineGrid::legacy(interior, world, centre, feet, window, scale);
        Outline {
            key: grid.key(),
            origin: grid.origin(),
            step: grid.step(),
            size: grid.size(),
            interior: interior.is_some(),
            feet,
            heights: Vec::new(),
            image: None,
        }
    }

    /// The top left (game x, y, y the largest) and the size (game units) of the grid
    /// [`Outline::new`] would make for these, without making it: the Local Map's frame before
    /// its outline is begun (`super::view::LocalProjection`).
    pub fn frame_for(interior: Option<u32>, world: u32, centre: [f32; 2], feet: f32, window: Vec2, scale: f32) -> ([f32; 2], Vec2) {
        let o = Outline::sized(interior, world, centre, feet, window, scale);
        (o.origin, Vec2::new(o.size[0] as f32, o.size[1] as f32) * o.step)
    }

    /// The key a grid for these would have (to tell whether the one made still serves).
    pub fn key_for(interior: Option<u32>, world: u32, centre: [f32; 2], feet: f32, window: Vec2, scale: f32) -> OutlineKey {
        let snap = extent(interior.is_some(), window, scale) / 4.0;
        (interior, world, [(centre[0] / snap.x).round() as i32, (centre[1] / snap.y).round() as i32, (feet / FEET_SNAP).round() as i32])
    }

    pub fn done(&self) -> bool {
        self.heights.len() >= self.size[0] * self.size[1]
    }

    /// The game point of sample `i`'s middle.
    fn point(&self, i: usize) -> [f32; 2] {
        let (x, y) = (i % self.size[0], i / self.size[0]);
        [self.origin[0] + (x as f32 + 0.5) * self.step, self.origin[1] - (y as f32 + 0.5) * self.step]
    }

    /// Cast up to `budget` more rays; `ray(origin, reach)` gives the height where a ray
    /// straight down from `origin` lands.
    pub fn advance(&mut self, budget: usize, mut ray: impl FnMut([f32; 3], f32) -> Option<f32>) {
        let (start, reach) = if self.interior { (INTERIOR_START, INTERIOR_REACH) } else { (EXTERIOR_START, EXTERIOR_REACH) };
        let total = self.size[0] * self.size[1];
        for _ in 0..budget {
            let i = self.heights.len();
            if i >= total {
                break;
            }
            let [x, y] = self.point(i);
            self.heights.push(ray([x, y, self.feet + start], reach));
        }
    }

    /// The picture's alpha, row by row from the top ([`Outline`]'s rules).
    pub fn alpha(&self) -> Vec<u8> {
        let [w, h] = self.size;
        let at = |x: i64, y: i64| -> Option<f32> {
            if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
                return None;
            }
            self.heights.get(y as usize * w + x as usize).copied().flatten()
        };
        let mut out = vec![0u8; w * h];
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let Some(z) = at(x, y) else { continue };
                let near = [at(x - 1, y), at(x + 1, y), at(x, y - 1), at(x, y + 1)];
                let a = if self.interior {
                    let rel = z - self.feet;
                    let solid = |v: Option<f32>| v.is_some_and(|v| v - self.feet >= -FLOOR_BAND.0);
                    if rel < -FLOOR_BAND.0 {
                        0
                    } else if near.iter().any(|&n| !solid(n) || n.is_some_and(|n| z - n >= EDGE_STEP)) {
                        EDGE_ALPHA
                    } else if rel > FLOOR_BAND.1 {
                        RAISED_ALPHA
                    } else {
                        FLOOR_ALPHA
                    }
                } else if near.iter().any(|&n| n.is_some_and(|n| z - n >= EXTERIOR_EDGE_STEP)) {
                    EDGE_ALPHA
                } else {
                    // The slope lit from the north-west (up and left on the map).
                    let dx = (at(x + 1, y).unwrap_or(z) - at(x - 1, y).unwrap_or(z)) / (2.0 * self.step);
                    let dy = (at(x, y - 1).unwrap_or(z) - at(x, y + 1).unwrap_or(z)) / (2.0 * self.step);
                    let n = Vec3::new(-dx, -dy, 1.0).normalize();
                    let light = Vec3::new(-1.0, 1.0, 1.5).normalize();
                    let lit = n.dot(light).clamp(0.0, 1.0);
                    (GROUND_ALPHA.0 + (GROUND_ALPHA.1 - GROUND_ALPHA.0) * lit).round() as u8
                };
                out[y as usize * w + x as usize] = a;
            }
        }
        out
    }

    /// The picture: white of the alpha [`Outline::alpha`] gives (tinted when drawn).
    pub fn picture(&self) -> Image {
        let rgba: Vec<u8> = self.alpha().into_iter().flat_map(|a| [255, 255, 255, a]).collect();
        let size = Extent3d { width: self.size[0] as u32, height: self.size[1] as u32, depth_or_array_layers: 1 };
        let mut image = Image::new(size, TextureDimension::D2, rgba, TextureFormat::Rgba8Unorm, RenderAssetUsages::default());
        image.sampler = ImageSampler::linear();
        image
    }

    /// How many samples landed somewhere.
    pub fn hits(&self) -> usize {
        self.heights.iter().filter(|h| h.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A room 10 × 6 samples with a table, in a grid of 14 × 10.
    fn room() -> Outline {
        let mut o = Outline::new(Some(1), 2, [0.0, 0.0], 0.0, Vec2::new(14.0, 10.0) * INTERIOR_STEP / MARGIN / INTERIOR_SCALE, INTERIOR_SCALE);
        o.size = [14, 10];
        o.origin = [0.0, 0.0];
        o.advance(1000, |p, _| {
            let (x, y) = ((p[0] / INTERIOR_STEP) as i32, (-p[1] / INTERIOR_STEP) as i32);
            let inside = (2..12).contains(&x) && (2..8).contains(&y);
            let table = (6..8).contains(&x) && (4..6).contains(&y);
            inside.then_some(if table { 80.0 } else { 0.0 })
        });
        o
    }

    #[test]
    fn a_room_is_floor_inside_walls() {
        let o = room();
        assert!(o.done());
        assert_eq!(o.hits(), 60);
        let a = o.alpha();
        let at = |x: usize, y: usize| a[y * 14 + x];
        // Outside, the walls at the room's edge, the floor and the table.
        assert_eq!(at(0, 0), 0);
        assert_eq!(at(2, 2), EDGE_ALPHA);
        assert_eq!(at(11, 5), EDGE_ALPHA);
        assert_eq!(at(3, 3), FLOOR_ALPHA);
        // The table's edge is drawn on the table, the floor beside it stays floor.
        assert_eq!(at(6, 4), EDGE_ALPHA);
        assert_eq!(at(5, 4), FLOOR_ALPHA);
    }

    #[test]
    fn the_grid_is_kept_while_the_map_pans_a_little() {
        let w = Vec2::new(855.0, 500.0);
        let a = Outline::key_for(None, 7, [1000.0, 1000.0], 0.0, w, 28.0);
        assert_eq!(a, Outline::key_for(None, 7, [1500.0, 1300.0], 30.0, w, 28.0));
        assert_ne!(a, Outline::key_for(None, 7, [9000.0, 1000.0], 0.0, w, 28.0));
        // Another floor.
        assert_ne!(a, Outline::key_for(None, 7, [1000.0, 1000.0], 300.0, w, 28.0));
        let o = Outline::new(None, 7, [1000.0, 1000.0], 0.0, w, 28.0);
        assert_eq!(o.key, a);
        // It covers the window round its anchor, and the 5×5 cells.
        assert!(o.size[0] as f32 * o.step >= w.x * 28.0);
        assert!(o.size[1] as f32 * o.step >= 5.0 * 4096.0);
    }

    #[test]
    fn slopes_outdoors_are_shaded_and_cliffs_edged() {
        let mut o = Outline::new(None, 1, [0.0, 0.0], 0.0, Vec2::new(10.0, 10.0), 28.0);
        o.size = [8, 4];
        o.origin = [0.0, 0.0];
        o.heights.clear();
        o.advance(100, |p, _| Some(if p[0] > 4.0 * EXTERIOR_STEP { 500.0 } else { p[0] * 0.2 }));
        let a = o.alpha();
        // The cliff between columns 3 and 4 drawn on its top; the gentle slope shaded
        // between the limits.
        assert_ne!(a[3], EDGE_ALPHA);
        assert_eq!(a[4], EDGE_ALPHA);
        assert!(a[1] as f32 >= GROUND_ALPHA.0 && (a[1] as f32) <= GROUND_ALPHA.1);
    }
}
