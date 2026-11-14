//! Grass: the GRAS records landscape textures name (LTEX GNAM) and where the plants
//! stand on a cell's landscape. Grass is not placed by references: it is generated from
//! the texture layers. Facts, sources and confidence tags: docs/spec-notes/grass.md.

use crate::fields::{formid_of, global, num, winner, Fields};
use crate::land::{Land, QUAD_SIDE, SIDE, SPACING};
use crate::land_surface::Surface;
use crate::CELL_SIZE;
use fnv_esm::{LoadOrder, Schema};
use serde_json::Value;
use std::collections::HashMap;

/// GRAS DATA "unit from water type" [documented: xEdit wbDefinitionsFNV.pas, same names].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterRule {
    #[default]
    AboveAtLeast,
    AboveAtMost,
    BelowAtLeast,
    BelowAtMost,
    EitherAtLeast,
    EitherAtMost,
    EitherAtMostAbove,
    EitherAtMostBelow,
}

/// The names of the enum in `schemas/fnv/placed.json` (GrassData), by stored value.
pub const WATER_RULES: [(&str, WaterRule); 8] = [
    ("Above - At Least", WaterRule::AboveAtLeast),
    ("Above - At Most", WaterRule::AboveAtMost),
    ("Below - At Least", WaterRule::BelowAtLeast),
    ("Below - At Most", WaterRule::BelowAtMost),
    ("Either - At Least", WaterRule::EitherAtLeast),
    ("Either - At Most", WaterRule::EitherAtMost),
    ("Either - At Most Above", WaterRule::EitherAtMostAbove),
    ("Either - At Most Below", WaterRule::EitherAtMostBelow),
];

impl WaterRule {
    /// Whether ground at `height` may carry the grass, `amount` units being the record's
    /// distance and `water` the cell's water surface (`None`: the cell has no water)
    /// [hypothesis: the readings of the eight names, grass.md §4].
    pub fn allows(self, height: f32, amount: f32, water: Option<f32>) -> bool {
        let Some(w) = water else {
            // No water: nothing is below it, and no distance to it is too small.
            return !matches!(self, WaterRule::BelowAtLeast | WaterRule::BelowAtMost);
        };
        let d = height - w;
        match self {
            WaterRule::AboveAtLeast => d >= amount,
            WaterRule::AboveAtMost => d >= 0.0 && d <= amount,
            WaterRule::BelowAtLeast => -d >= amount,
            WaterRule::BelowAtMost => d <= 0.0 && -d <= amount,
            WaterRule::EitherAtLeast => d.abs() >= amount,
            WaterRule::EitherAtMost => d.abs() <= amount,
            WaterRule::EitherAtMostAbove => d <= amount,
            WaterRule::EitherAtMostBelow => -d <= amount,
        }
    }
}

/// A grass type (GRAS).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Grass {
    pub form_id: u32,
    pub edid: Option<String>,
    /// MODL as stored (relative to `meshes\`).
    pub model: Option<String>,
    /// 0-100.
    pub density: u8,
    /// Degrees from level ground.
    pub min_slope: u8,
    pub max_slope: u8,
    pub water_distance: u16,
    pub water_rule: WaterRule,
    pub position_range: f32,
    pub height_range: f32,
    pub color_range: f32,
    pub wave_period: f32,
    pub vertex_lighting: bool,
    pub uniform_scaling: bool,
    pub fit_to_slope: bool,
}

pub fn load_grass(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<Grass> {
    let (pi, r) = winner(lo, fid)?;
    if &r.header.sig != b"GRAS" {
        return None;
    }
    let f = Fields::new(r, pi, schema).ok()?;
    let d = f.get(b"DATA")?;
    let n = |k: &str| num(&d, k).unwrap_or(0.0);
    let rule = match d.get("unit_from_water_type") {
        Some(Value::String(s)) => WATER_RULES.iter().find(|(name, _)| name == s).map(|(_, r)| *r),
        Some(v) => v.as_u64().and_then(|i| WATER_RULES.get(i as usize)).map(|(_, r)| *r),
        None => None,
    };
    let flags = d.get("flags").and_then(|v| num(v, "value").or_else(|| v.as_f64())).unwrap_or(0.0) as u32;
    Some(Grass {
        form_id: fid,
        edid: f.string(b"EDID"),
        model: f.string(b"MODL").filter(|m| !m.trim().is_empty()),
        density: n("density") as u8,
        min_slope: n("min_slope") as u8,
        max_slope: n("max_slope") as u8,
        water_distance: n("unit_from_water_amount") as u16,
        water_rule: rule.unwrap_or_default(),
        position_range: n("position_range") as f32,
        height_range: n("height_range") as f32,
        color_range: n("color_range") as f32,
        wave_period: n("wave_period") as f32,
        vertex_lighting: flags & 1 != 0,
        uniform_scaling: flags & 2 != 0,
        fit_to_slope: flags & 4 != 0,
    })
}

/// The grass types a landscape texture names (every LTEX GNAM, in stored order).
pub fn texture_grasses(lo: &LoadOrder, schema: &Schema, ltex: u32) -> Vec<u32> {
    let Some((pi, r)) = winner(lo, ltex) else { return Vec::new() };
    let Ok(f) = Fields::new(r, pi, schema) else { return Vec::new() };
    f.subs
        .iter()
        .filter(|s| &s.sig == b"GNAM")
        .filter_map(|s| f.decode(s).as_ref().and_then(formid_of).and_then(|g| global(lo, pi, g)))
        .collect()
}

/// How densely grass is generated: the game's `[Grass]` INI settings (grass.md §3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrassSettings {
    /// Edge of the squares the cell is divided into, one candidate plant per grass type
    /// in each (`iMinGrassSize`, 80 in the game's default INI).
    pub min_size: f32,
    /// Grass types used per texture (`iMaxGrassTypesPerTexure`, 2).
    pub max_types_per_texture: usize,
    /// A texture grows its grass where its share of the ground is above this
    /// (`fTexturePctThreshold`, 0: wherever it shows at all).
    pub texture_threshold: f32,
    /// Multiplier on every type's density (1: as stored).
    pub density_scale: f32,
}

impl Default for GrassSettings {
    fn default() -> Self {
        GrassSettings { min_size: 80.0, max_types_per_texture: 2, texture_threshold: 0.0, density_scale: 1.0 }
    }
}

/// One generated plant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plant {
    /// Index into the `types` slice given to [`place`].
    pub kind: usize,
    /// Relative to the cell's south-west corner; z on the landscape.
    pub position: [f32; 3],
    /// The landscape's normal there (unit, game space).
    pub normal: [f32; 3],
    /// Turn about the vertical, radians.
    pub yaw: f32,
    /// Scale across (x, y) and up (z).
    pub scale: [f32; 2],
    /// Colour multiplier, 0-1 (the colour range darkens).
    pub shade: f32,
}

/// The textures of a landscape as opacity grids per quadrant, to ask how much of a
/// texture shows at a point.
pub struct Coverage {
    quadrants: [QuadrantLayers; 4],
}

/// One quadrant's textures, bottom to top: (texture, opacity grid; `None` for the base).
type QuadrantLayers = Vec<(Option<u32>, Option<Vec<f32>>)>;

impl Coverage {
    /// Layers stack in ascending layer number over the quadrant's base, each hiding what
    /// lies under it by its opacity (the renderer's rule, world.md §3) [hypothesis].
    pub fn new(land: &Land) -> Coverage {
        let mut quadrants: [QuadrantLayers; 4] = Default::default();
        for (q, out) in quadrants.iter_mut().enumerate() {
            let q = q as u8;
            if let Some(b) = land.layers.iter().rev().find(|l| l.base && l.quadrant == q) {
                out.push((b.texture, None));
            } else {
                // No BTXT: the default texture, which is not an LTEX and so has no grass.
                out.push((None, None));
            }
            let mut layers: Vec<_> = land.layers.iter().filter(|l| !l.base && l.quadrant == q).collect();
            layers.sort_by_key(|l| l.layer);
            for l in layers {
                let mut grid = vec![0.0f32; QUAD_SIDE * QUAD_SIDE];
                for &(p, o) in &l.alpha {
                    if let Some(a) = grid.get_mut(p as usize) {
                        *a = o.clamp(0.0, 1.0);
                    }
                }
                out.push((l.texture, Some(grid)));
            }
        }
        Coverage { quadrants }
    }

    /// Every texture named by the landscape.
    pub fn textures(&self) -> Vec<u32> {
        let mut v: Vec<u32> = self.quadrants.iter().flatten().filter_map(|(t, _)| *t).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// The share of `texture` in what shows at cell-local (x, y), 0-1.
    pub fn at(&self, texture: u32, x: f32, y: f32) -> f32 {
        let half = CELL_SIZE / 2.0;
        let (qx, qy) = (usize::from(x >= half), usize::from(y >= half));
        let layers = &self.quadrants[qy * 2 + qx];
        if !layers.iter().any(|(t, _)| *t == Some(texture)) {
            return 0.0;
        }
        let (u, v) = ((x - qx as f32 * half) / SPACING, (y - qy as f32 * half) / SPACING);
        let max = (QUAD_SIDE - 1) as f32;
        let (u, v) = (u.clamp(0.0, max), v.clamp(0.0, max));
        let (c, r) = ((u.floor() as usize).min(QUAD_SIDE - 2), (v.floor() as usize).min(QUAD_SIDE - 2));
        let (fu, fv) = (u - c as f32, v - r as f32);
        let sample = |g: &[f32]| {
            let a = |r: usize, c: usize| g[r * QUAD_SIDE + c];
            (a(r, c) * (1.0 - fu) + a(r, c + 1) * fu) * (1.0 - fv) + (a(r + 1, c) * (1.0 - fu) + a(r + 1, c + 1) * fu) * fv
        };
        // From the top down: what a layer shows is its opacity times what the layers
        // above it let through.
        let (mut through, mut share) = (1.0f32, 0.0f32);
        for (t, grid) in layers.iter().rev() {
            let a = grid.as_deref().map_or(1.0, sample);
            if *t == Some(texture) {
                share += through * a;
            }
            through *= 1.0 - a;
            if through <= 0.0 {
                break;
            }
        }
        share
    }
}

/// Height of the landscape at cell-local (x, y) on the LAND grid as stored: each grid
/// square is two triangles split along its south-west → north-east diagonal, as the
/// renderer draws it (world.md §3); the point is brought into the cell first. The one
/// triangle rule of `crate::land_surface` ([`crate::land_surface::grid_height_at`]).
pub fn height_at(heights: &[f32], x: f32, y: f32) -> f32 {
    let (x, y) = (x.clamp(0.0, CELL_SIZE), y.clamp(0.0, CELL_SIZE));
    crate::land_surface::grid_height_at(heights, SIDE, SPACING, x, y).unwrap_or(f32::NAN)
}

/// Unit normal of the landscape at cell-local (x, y): the stored normals blended between
/// the four vertices around the point, or the heights' own slope without them.
pub fn normal_at(land: &Land, x: f32, y: f32) -> [f32; 3] {
    let max = (SIDE - 1) as f32;
    let (u, v) = ((x / SPACING).clamp(0.0, max), (y / SPACING).clamp(0.0, max));
    let (c, r) = ((u.floor() as usize).min(SIDE - 2), (v.floor() as usize).min(SIDE - 2));
    let (fx, fy) = (u - c as f32, v - r as f32);
    let n = match (&land.normals, &land.heights) {
        (Some(nm), _) => {
            let at = |r: usize, c: usize| nm[r * SIDE + c].map(|k| k as f32);
            let mix = |a: [f32; 3], b: [f32; 3], t: f32| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t);
            mix(mix(at(r, c), at(r, c + 1), fx), mix(at(r + 1, c), at(r + 1, c + 1), fx), fy)
        }
        (None, Some(h)) => {
            let at = |r: usize, c: usize| h[r * SIDE + c];
            let dx = ((at(r, c + 1) - at(r, c)) * (1.0 - fy) + (at(r + 1, c + 1) - at(r + 1, c)) * fy) / SPACING;
            let dy = ((at(r + 1, c) - at(r, c)) * (1.0 - fx) + (at(r + 1, c + 1) - at(r, c + 1)) * fx) / SPACING;
            [-dx, -dy, 1.0]
        }
        (None, None) => [0.0, 0.0, 1.0],
    };
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 1e-6 && n[2] > 0.0 {
        n.map(|k| k / len)
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// A repeatable pseudo-random number for a lattice square, a grass type and a purpose.
fn hash(a: i32, b: i32, c: u32, salt: u32) -> u32 {
    let mut h = 0x9E37_79B9u32;
    for v in [a as u32, b as u32, c, salt] {
        h ^= v.wrapping_mul(0x85EB_CA6B);
        h = h.rotate_left(13).wrapping_mul(5).wrapping_add(0xE654_6B64);
    }
    h ^= h >> 16;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^ (h >> 16)
}

/// The hash as a number in 0..1.
fn unit(a: i32, b: i32, c: u32, salt: u32) -> f32 {
    (hash(a, b, c, salt) >> 8) as f32 / (1u32 << 24) as f32
}

/// Generate the plants of one cell's landscape [hypothesis: the whole rule, grass.md §4;
/// the game's own generator is not known].
///
/// The world is divided into squares `settings.min_size` on a side (a lattice over the
/// whole worldspace, so cells join without seams). In each square every grass type of a
/// texture gets one candidate plant, kept with probability density / 100. The plant
/// stands at the square's centre moved by up to the type's position range each way, if
/// the texture's share of the ground there is above the threshold and the slope and the
/// water rule allow it. All numbers depend only on the square, the texture and the type,
/// so a cell always grows the same grass.
///
/// `types` are the grass types of interest; `texture_types` gives, per LTEX FormID, the
/// indices into `types` it grows (already cut to `max_types_per_texture`). `grid` is the
/// cell's grid position and `water` its water surface.
pub fn place(
    land: &Land,
    grid: (i32, i32),
    water: Option<f32>,
    types: &[Grass],
    texture_types: &HashMap<u32, Vec<usize>>,
    settings: &GrassSettings,
) -> Vec<Plant> {
    let Some(heights) = land.heights.as_deref() else { return Vec::new() };
    place_on(land, &Surface::stored(heights), grid, water, types, texture_types, settings)
}

/// [`place`] on the landscape surface the cell is drawn with (`surface`, smoothed or as
/// stored, `crate::land_surface`): the plants stand on its heights, tilted by its normals
/// when smoothed and by the LAND's own ([`normal_at`]) otherwise.
pub fn place_on(
    land: &Land,
    surface: &Surface,
    grid: (i32, i32),
    water: Option<f32>,
    types: &[Grass],
    texture_types: &HashMap<u32, Vec<usize>>,
    settings: &GrassSettings,
) -> Vec<Plant> {
    let s = settings.min_size;
    if s.is_nan() || s <= 1.0 || settings.density_scale <= 0.0 {
        return Vec::new();
    }
    let cover = Coverage::new(land);
    // (texture, type index) pairs this landscape can grow.
    let pairs: Vec<(u32, usize)> = cover
        .textures()
        .into_iter()
        .flat_map(|t| texture_types.get(&t).into_iter().flatten().map(move |&k| (t, k)))
        .filter(|&(_, k)| types.get(k).is_some_and(|g| g.density > 0))
        .collect();
    if pairs.is_empty() {
        return Vec::new();
    }
    let origin = (grid.0 as f64 * CELL_SIZE as f64, grid.1 as f64 * CELL_SIZE as f64);
    // Squares whose centre lies in this cell.
    let first = |o: f64| (o / s as f64 - 0.5).ceil() as i32;
    let last = |o: f64| ((o + CELL_SIZE as f64) / s as f64 - 0.5).ceil() as i32;
    let mut out = Vec::new();
    for gj in first(origin.1)..last(origin.1) {
        for gi in first(origin.0)..last(origin.0) {
            let (cx, cy) = (((gi as f64 + 0.5) * s as f64 - origin.0) as f32, ((gj as f64 + 0.5) * s as f64 - origin.1) as f32);
            for &(texture, kind) in &pairs {
                let g = &types[kind];
                // Texture and type both seed the numbers: two textures growing the same
                // type do not double it at a blend, and two types do not stand together.
                let seed = g.form_id ^ texture.rotate_left(16);
                let chance = (g.density.min(100) as f32 / 100.0 * settings.density_scale).min(1.0);
                let roll = unit(gi, gj, seed, 0);
                if roll >= chance {
                    continue;
                }
                let r = g.position_range.max(0.0);
                let x = (cx + (unit(gi, gj, seed, 1) * 2.0 - 1.0) * r).clamp(0.0, CELL_SIZE);
                let y = (cy + (unit(gi, gj, seed, 2) * 2.0 - 1.0) * r).clamp(0.0, CELL_SIZE);
                if cover.at(texture, x, y) <= settings.texture_threshold {
                    continue;
                }
                let z = surface.height_at_clamped(x, y);
                if !g.water_rule.allows(z, g.water_distance as f32, water) {
                    continue;
                }
                let normal = surface.normal_at(x, y).unwrap_or_else(|| normal_at(land, x, y));
                let slope = normal[2].clamp(-1.0, 1.0).acos().to_degrees();
                if slope < g.min_slope as f32 || slope > g.max_slope as f32 {
                    continue;
                }
                let up = (1.0 + (unit(gi, gj, seed, 3) * 2.0 - 1.0) * g.height_range).max(0.05);
                out.push(Plant {
                    kind,
                    position: [x, y, z],
                    normal,
                    yaw: unit(gi, gj, seed, 4) * std::f32::consts::TAU,
                    scale: [if g.uniform_scaling { up } else { 1.0 }, up],
                    shade: (1.0 - unit(gi, gj, seed, 5) * g.color_range.max(0.0)).clamp(0.0, 1.0),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::land::Layer;

    fn flat(layers: Vec<Layer>) -> Land {
        Land { heights: Some(vec![100.0; SIDE * SIDE]), layers, ..Default::default() }
    }

    fn base(q: u8, t: u32) -> Layer {
        Layer { quadrant: q, texture: Some(t), layer: 0, base: true, alpha: Vec::new() }
    }

    fn grass(density: u8) -> Grass {
        Grass { form_id: 0x700, density, max_slope: 90, position_range: 20.0, height_range: 0.3, color_range: 0.2, uniform_scaling: true, ..Default::default() }
    }

    fn grow(land: &Land, g: Grass, water: Option<f32>) -> Vec<Plant> {
        let map = HashMap::from([(0x10, vec![0])]);
        place(land, (-3, 2), water, &[g], &map, &GrassSettings::default())
    }

    #[test]
    fn density_sets_the_number_of_plants() {
        let land = flat((0..4).map(|q| base(q, 0x10)).collect());
        // 4096 / 80 = 51.2 squares a side: 51 or 52 centres fall in a cell.
        let all = grow(&land, grass(100), None).len();
        assert!((51 * 51..=52 * 52).contains(&all), "{all}");
        let some = grow(&land, grass(30), None).len() as f32 / all as f32;
        assert!((some - 0.3).abs() < 0.04, "{some}");
        assert!(grow(&land, grass(0), None).is_empty());
        // Twice the same question, twice the same answer.
        assert_eq!(grow(&land, grass(30), None), grow(&land, grass(30), None));
    }

    #[test]
    fn grass_grows_only_on_its_texture() {
        // The texture fills the south-west quadrant only.
        let land = flat(vec![base(0, 0x10), base(1, 0x11), base(2, 0x11), base(3, 0x11)]);
        let plants = grow(&land, grass(100), None);
        assert!(!plants.is_empty());
        assert!(plants.iter().all(|p| p.position[0] < 2048.0 && p.position[1] < 2048.0));
        // A layer that covers the quadrant entirely hides the base and its grass.
        let cover = Layer { quadrant: 0, texture: Some(0x11), layer: 1, base: false, alpha: (0..289).map(|p| (p, 1.0)).collect() };
        let land = flat(vec![base(0, 0x10), cover]);
        assert!(grow(&land, grass(100), None).is_empty());
    }

    #[test]
    fn grass_grows_where_its_layer_shows() {
        // West half of the quadrant fully the grass texture, east half not at all.
        let alpha = (0..289u16).filter(|p| p % 17 < 8).map(|p| (p, 1.0)).collect();
        let layer = Layer { quadrant: 0, texture: Some(0x10), layer: 1, base: false, alpha };
        let land = flat(vec![base(0, 0x11), layer]);
        let c = Coverage::new(&land);
        assert_eq!(c.at(0x10, 100.0, 500.0), 1.0);
        assert_eq!(c.at(0x11, 100.0, 500.0), 0.0);
        assert_eq!(c.at(0x10, 1500.0, 500.0), 0.0);
        assert_eq!(c.at(0x11, 1500.0, 500.0), 1.0);
        // Half way along the blend between grid columns 7 and 8.
        assert!((c.at(0x10, 7.5 * SPACING, 500.0) - 0.5).abs() < 1e-5);
        // Wherever the layer shows at all, up to where its opacity reaches 0 at column 8.
        let plants = grow(&land, grass(100), None);
        assert!(plants.iter().all(|p| p.position[0] < 8.0 * SPACING));
        assert!(plants.iter().any(|p| p.position[0] > 7.5 * SPACING));
        // A threshold of a half stops the grass half way along the blend.
        let map = HashMap::from([(0x10, vec![0])]);
        let settings = GrassSettings { texture_threshold: 0.5, ..Default::default() };
        let fewer = place(&land, (-3, 2), None, &[grass(100)], &map, &settings);
        assert!(!fewer.is_empty() && fewer.len() < plants.len());
        assert!(fewer.iter().all(|p| p.position[0] < 7.5 * SPACING));
    }

    /// On a smoothed surface (#2149) the plants stand on its heights and lean with its
    /// normals, not the stored triangles'.
    #[test]
    fn plants_stand_on_the_surface_they_are_drawn_on() {
        let mut land = flat((0..4).map(|q| base(q, 0x10)).collect());
        let heights: Vec<f32> = (0..SIDE * SIDE).map(|i| (((i % SIDE) as f32 * 0.5).sin() * 80.0 / 8.0).round() * 8.0 + (i / SIDE) as f32 * 8.0).collect();
        land.heights = Some(heights.clone());
        let mut block: crate::land_surface::Block = [[None; 3]; 3];
        block[1][1] = Some(&heights);
        let s = Surface::smoothed(&block).unwrap();
        let map = HashMap::from([(0x10, vec![0])]);
        let plants = place_on(&land, &s, (-3, 2), None, &[grass(100)], &map, &GrassSettings::default());
        assert!(!plants.is_empty());
        let mut off_stored = 0;
        for p in &plants {
            let [x, y, z] = p.position;
            assert_eq!(z, s.height_at_clamped(x, y));
            assert_eq!(p.normal, s.normal_at(x, y).unwrap());
            off_stored += usize::from((z - height_at(&heights, x, y)).abs() > 0.1);
        }
        assert!(off_stored > 0, "some stand where smoothing moved the ground");
    }

    #[test]
    fn slope_and_water_rules() {
        let mut land = flat((0..4).map(|q| base(q, 0x10)).collect());
        // A ramp climbing one unit per unit eastwards: 45 degrees.
        land.heights = Some((0..SIDE * SIDE).map(|i| (i % SIDE) as f32 * SPACING).collect());
        let n = normal_at(&land, 1000.0, 1000.0);
        assert!((n[0] + std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4 && (n[2] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4, "{n:?}");
        assert!((height_at(land.heights.as_ref().unwrap(), 1000.0, 77.0) - 1000.0).abs() < 1e-3);
        let mut g = grass(100);
        g.max_slope = 40;
        assert!(grow(&land, g.clone(), None).is_empty());
        g.max_slope = 50;
        let all = grow(&land, g.clone(), None);
        assert!(!all.is_empty());
        // Water at 2,000: only the ground above it grows this grass.
        let dry = grow(&land, g.clone(), Some(2000.0));
        assert!(!dry.is_empty() && dry.len() < all.len());
        assert!(dry.iter().all(|p| p.position[2] >= 2000.0));
        g.water_rule = WaterRule::BelowAtLeast;
        g.water_distance = 500;
        let wet = grow(&land, g.clone(), Some(2000.0));
        assert!(!wet.is_empty() && wet.iter().all(|p| p.position[2] <= 1500.0));
        assert!(grow(&land, g, None).is_empty());
    }

    #[test]
    fn water_rule_readings() {
        use WaterRule::*;
        let w = Some(0.0);
        assert!(AboveAtLeast.allows(10.0, 10.0, w) && !AboveAtLeast.allows(9.0, 10.0, w));
        assert!(AboveAtMost.allows(5.0, 10.0, w) && !AboveAtMost.allows(11.0, 10.0, w) && !AboveAtMost.allows(-1.0, 10.0, w));
        assert!(BelowAtMost.allows(-5.0, 10.0, w) && !BelowAtMost.allows(-11.0, 10.0, w) && !BelowAtMost.allows(1.0, 10.0, w));
        assert!(EitherAtLeast.allows(-20.0, 10.0, w) && !EitherAtLeast.allows(5.0, 10.0, w));
        assert!(EitherAtMost.allows(-5.0, 10.0, w) && !EitherAtMost.allows(20.0, 10.0, w));
        assert!(EitherAtMostAbove.allows(-500.0, 10.0, w) && !EitherAtMostAbove.allows(20.0, 10.0, w));
        assert!(EitherAtMostBelow.allows(500.0, 10.0, w) && !EitherAtMostBelow.allows(-20.0, 10.0, w));
        assert!(AboveAtLeast.allows(-900.0, 10.0, None) && !BelowAtMost.allows(-900.0, 10.0, None));
    }

    #[test]
    fn plants_vary_within_their_ranges() {
        let land = flat((0..4).map(|q| base(q, 0x10)).collect());
        let plants = grow(&land, grass(100), None);
        let (lo, hi) = plants.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.scale[1]), hi.max(p.scale[1])));
        assert!(lo >= 0.7 && hi <= 1.3 && hi - lo > 0.5, "{lo} {hi}");
        assert!(plants.iter().all(|p| p.scale[0] == p.scale[1] && p.shade > 0.8 - 1e-6 && p.shade <= 1.0));
        assert!(plants.iter().all(|p| p.position[2] == 100.0 && p.normal == [0.0, 0.0, 1.0]));
        // Without uniform scaling only the height changes.
        let mut g = grass(100);
        g.uniform_scaling = false;
        assert!(grow(&land, g, None).iter().all(|p| p.scale[0] == 1.0));
    }
}
