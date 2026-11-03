//! Obstacles: placed objects whose base is flagged Obstacle (record header bit 25 of STAT,
//! MSTT, ACTI, SCOL, CONT and LIGH [documented: xEdit wbDefinitionsFNV.pas flag lists;
//! schemas/fnv]) "dynamically cut the navmesh in game preventing actors from walking into
//! it" [documented: GECK wiki Activator, Flags].
//!
//! The navmesh is made in the editor around what stands in the cell, so an object that
//! is always there already has its hole. What the cut adds is for objects that come and
//! go: NVForever learns the references that can be enabled or disabled (Initially
//! Disabled, or with an enable parent) [hypothesis: the cut matters for those;
//! FalloutNV.esm places 59,925 obstacle references, mostly rocks and cliffs whose bounds
//! are far larger than their shape, and 149 that can come and go: sandbags, guard rails,
//! force fields, box cars, observed 2026-10-24], and cuts for one only while it is
//! enabled and was placed Initially Disabled: one placed enabled stood there when the
//! navmesh was made, so the navmesh already goes round it (or through it, where its
//! bounds are larger than its shape) [hypothesis; observed 2026-10-28: the sandbag
//! 0x00168F64 at Hoover Dam, placed enabled under the enable parent
//! VMQ03aGrahamRadioREF, has navmesh triangles under its bounds that join Colonel
//! Moore's patrol points 0x001377C5 and 0x001377C6 on her way to Kimball's podium; cut,
//! they split the deck and she never reached it]. The cut takes the
//! triangles under the reference's bounds (OBND, placed and scaled), seen from above,
//! whose heights meet the bounds from 64 units under the base to [`TOP_CLEAR`] under the
//! top [engine choice: a triangle near the top is on the object, not under it]
//! (docs/spec-notes/navmesh.md §7).

use crate::doors::{box_overlaps, Bounds, InPlaceDoor};
use crate::graph::{NavGraph, Space};
use fnv_esm::{LoadOrder, Schema};
use fnv_world::WorldIndex;
use std::collections::HashMap;

/// The header flag's name in the schema (`header_flags`).
pub const OBSTACLE_FLAG: &str = "Obstacle";

/// A triangle this near the bounds' top or above is not cut [engine choice].
pub const TOP_CLEAR: f32 = 32.0;

/// How far under the bounds' base a triangle is still cut [engine choice, as a door's
/// triangles, `crate::doors`].
const BELOW: f32 = 64.0;

/// An obstacle reference as placed.
#[derive(Debug, Clone, PartialEq)]
pub struct Obstacle {
    pub reference: u32,
    pub space: Space,
    pub position: [f32; 3],
    /// Row-major rotation (`fnv_world::transform::ref_rotation`).
    pub rotation: [f32; 9],
    pub scale: f32,
    /// The base's OBND corners in the model's frame.
    pub bounds: Bounds,
    /// Not Initially Disabled.
    pub initially_enabled: bool,
}

impl Obstacle {
    /// The same box as a door's, for the shared geometry (`crate::doors`).
    fn shape(&self) -> InPlaceDoor {
        InPlaceDoor { door: self.reference, space: self.space, position: self.position, rotation: self.rotation, scale: self.scale, bounds: self.bounds }
    }

    /// Whether the obstacle cuts triangle `t` (corners).
    pub fn cuts(&self, t: [[f32; 3]; 3]) -> bool {
        let s = self.shape();
        box_overlaps(s.footprint(), s.z_range(), t, BELOW, TOP_CLEAR)
    }
}

/// An obstacle the graph knows: the triangles it cuts and whether it is enabled now.
#[derive(Debug, Clone, PartialEq)]
pub struct ObstacleCut {
    pub tris: Vec<u32>,
    pub enabled: bool,
    /// Placed enabled (not Initially Disabled): the navmesh was made with it standing,
    /// so it never cuts (module notes).
    pub placed_enabled: bool,
}

impl ObstacleCut {
    /// Whether it cuts its triangles now: enabled, and absent when the navmesh was made.
    pub fn cuts_now(&self) -> bool {
        self.enabled && !self.placed_enabled
    }
}

/// Every obstacle reference of the load order that can be enabled or disabled (module
/// notes), in the space of its cell, with its base's bounds (deleted references and bases
/// without bounds left out).
pub fn obstacles(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex) -> Vec<Obstacle> {
    use fnv_world::fields::{winner, Fields};
    use fnv_world::placed::ref_flags::{DELETED, INITIALLY_DISABLED};
    use rayon::prelude::*;
    let mut ids: Vec<(u32, u32)> = ix.refs.iter().filter(|(_, e)| &e.sig == b"REFR").map(|(&r, e)| (r, e.cell)).collect();
    ids.sort_unstable();
    // Cheap tests first: the reference's own flags and XESP, then its base's header.
    let found: Vec<(u32, u32, u32)> = ids
        .par_iter()
        .filter_map(|&(r, cell)| {
            let (pi, rec) = winner(lo, r)?;
            if rec.header.flags & DELETED != 0 {
                return None;
            }
            let f = Fields::new(rec, pi, schema).ok()?;
            if rec.header.flags & INITIALLY_DISABLED == 0 && f.raw(b"XESP").is_none() {
                return None;
            }
            let base = f.formid(b"NAME", lo)?;
            let b = winner(lo, base)?.1;
            schema.header_flag(&b.header.sig, OBSTACLE_FLAG).is_some_and(|m| b.header.flags & m != 0).then_some((r, cell, base))
        })
        .collect();
    let mut bases: HashMap<u32, Option<Bounds>> = HashMap::new();
    let mut out = Vec::new();
    for (r, cell, base) in found {
        let Some(c) = ix.cells.get(&cell) else { continue };
        let Some(pl) = fnv_world::placed::placement(lo, schema, r) else { continue };
        let bounds = *bases.entry(base).or_insert_with(|| record_bounds(lo, schema, base));
        let Some(bounds) = bounds else { continue };
        let space = match c.world {
            Some(w) => Space::World(w),
            None => Space::Interior(c.form_id),
        };
        out.push(Obstacle {
            reference: r,
            space,
            position: pl.position,
            rotation: fnv_world::transform::ref_rotation(pl.rotation),
            scale: pl.scale,
            bounds,
            initially_enabled: !pl.initially_disabled(),
        });
    }
    out
}

/// A record's OBND corners (min, max), when it has bounds that are not empty.
fn record_bounds(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<Bounds> {
    use fnv_world::fields::{num, winner, Fields};
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let b = f.get(b"OBND")?;
    let corner = |k: &str| -> Option<[f32; 3]> {
        let c = b.get(k)?;
        Some([num(c, "x")? as f32, num(c, "y")? as f32, num(c, "z")? as f32])
    };
    let (lo, hi) = (corner("min")?, corner("max")?);
    (0..3).all(|k| hi[k] > lo[k]).then_some((lo, hi))
}

/// A stamp for [`NavGraph::obstacles_stamp`] that no graph of this process had.
fn next_obstacles_stamp() -> u64 {
    static STAMPS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    STAMPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl NavGraph {
    /// Changes when the obstacles or their enabled states change ([`NavGraph::set_obstacles`],
    /// [`NavGraph::sync_obstacles`]): each change takes a number no graph of this process
    /// had, so two graphs (or one graph before and after) with the same stamp have the
    /// same obstacles in the same states. 0 while none was learnt. A writer of the public
    /// `obstacles` map other than these two must not exist (none does,
    /// fork-notes/cpu-frame-cost/frame-budget/ai-split-2026-11-03.md).
    pub fn obstacles_stamp(&self) -> u64 {
        self.obstacles_stamp
    }

    /// Learn the obstacles: the triangles each cuts (those its bounds stand on); each is
    /// enabled as `enabled` says. Replaces what was learnt before. An obstacle no
    /// triangle lies under is left out.
    pub fn set_obstacles(&mut self, list: Vec<Obstacle>, enabled: impl Fn(&Obstacle) -> bool) {
        self.obstacles_stamp = next_obstacles_stamp();
        self.obstacles.clear();
        for o in list {
            let (lo, hi) = o.bounds;
            let reach = o.scale.max(0.0) * (0..3).map(|k| lo[k].abs().max(hi[k].abs())).fold(0.0, f32::max) * 1.5;
            let tris: Vec<u32> = self.triangles_near(o.space, o.position[0], o.position[1], reach).into_iter().filter(|&t| o.cuts(self.corners(t))).collect();
            if !tris.is_empty() {
                let on = enabled(&o);
                self.obstacles.insert(o.reference, ObstacleCut { tris, enabled: on, placed_enabled: o.initially_enabled });
            }
        }
        self.recut();
    }

    /// [`NavGraph::set_obstacles`] with the load order's obstacles, each as placed
    /// (enabled unless Initially Disabled).
    pub fn learn_obstacles(&mut self, lo: &LoadOrder, schema: &Schema, ix: &WorldIndex) {
        self.set_obstacles(obstacles(lo, schema, ix), |o| o.initially_enabled);
    }

    /// Bring the obstacles' enabled states in line with `states` (reference, enabled);
    /// references the graph does not know are ignored. Returns the obstacles that
    /// changed; the walkable areas are worked out again when any did.
    pub fn sync_obstacles(&mut self, states: impl IntoIterator<Item = (u32, bool)>) -> Vec<u32> {
        let mut changed = Vec::new();
        for (r, on) in states {
            if let Some(o) = self.obstacles.get_mut(&r) {
                if o.enabled != on {
                    o.enabled = on;
                    changed.push(r);
                }
            }
        }
        if !changed.is_empty() {
            self.obstacles_stamp = next_obstacles_stamp();
            self.recut();
        }
        changed
    }

    /// The triangles cut now by enabled obstacles ([`ObstacleCut::cuts_now`]).
    pub fn cut_triangles(&self) -> usize {
        self.cut.iter().filter(|&&c| c > 0).count()
    }

    /// Count the cutting obstacles over each triangle ([`ObstacleCut::cuts_now`]) and,
    /// when that changed, work out the areas again.
    fn recut(&mut self) {
        let before = std::mem::take(&mut self.cut);
        if self.obstacles.values().any(ObstacleCut::cuts_now) {
            self.cut.resize(self.tris.len(), 0);
            for o in self.obstacles.values().filter(|o| o.cuts_now()) {
                for &t in &o.tris {
                    if let Some(c) = self.cut.get_mut(t as usize) {
                        *c = c.saturating_add(1);
                    }
                }
            }
        }
        if self.cut != before {
            self.compute_areas();
        }
    }
}
