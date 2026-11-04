//! `fnvtool nav graph | path | walk`: the nav graph, paths over it, and an NPC-sized
//! character walking a path in the physics world. JSON schemas:
//! docs/spec-notes/navmesh.md §6.

use crate::nav_cmd::fid;
use crate::phys_cmd::{build_with, plugin_in_world, Built, Meshes};
use anyhow::{anyhow, bail};
use clap::Args;
use fnv_esm::{LoadOrder, Schema};
use fnv_nav::graph::{EdgeKind, NONE};
use fnv_nav::follow::Recovery;
use fnv_nav::{Follower, NavGraph, NavRecords, Path, PathParams, Place, Space, Steer};
use fnv_collide::anim::{DoorMotion, ModelAnim};
use fnv_phys::{layers, CharacterInput, CharacterParams, Filter, Owner, PhysParams, PhysWorld, RefCollisionOptions, RefPose};
use fnv_sim::RefTable;
use fnv_world::{cell_of, WorldIndex};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path as FsPath;
use std::sync::Arc;
use std::time::Instant;

/// Where a path starts and ends.
#[derive(Args, Clone)]
pub struct Ends {
    /// Start: a placed reference (Editor ID or FormID) or "x,y,z" in --space.
    #[arg(long, allow_hyphen_values = true)]
    pub from: String,
    /// Goal: a placed reference (Editor ID or FormID) or "x,y,z" in --space.
    #[arg(long, allow_hyphen_values = true)]
    pub to: String,
    /// The space of coordinates given as "x,y,z": a worldspace or an interior cell
    /// (Editor ID or FormID).
    #[arg(long, default_value = "WastelandNV")]
    pub space: String,
}

/// Path options (defaults: `fnv_nav::PathParams::default()`).
#[derive(Args, Clone)]
pub struct PathOpts {
    /// Clearance from the walkable area's outline, game units.
    #[arg(long)]
    pub clearance: Option<f32>,
    /// Use ledge links (drops and climbs).
    #[arg(long)]
    pub ledges: bool,
    /// Do not use doors.
    #[arg(long)]
    pub no_doors: bool,
    /// A large creature (keeps off "No Large Creatures" triangles).
    #[arg(long)]
    pub large: bool,
    /// A creature that does not swim (keeps off water triangles).
    #[arg(long)]
    pub no_water: bool,
    /// Locked doors may be used.
    #[arg(long)]
    pub open_locked: bool,
    /// Keys the actor carries (FormIDs, comma-separated): the locked doors they open may
    /// be used.
    #[arg(long, value_delimiter = ',', value_parser = crate::parse_formid)]
    pub keys: Vec<u32>,
}

impl PathOpts {
    fn params(&self) -> PathParams {
        let d = PathParams::default();
        PathParams {
            radius: self.clearance.unwrap_or(d.radius),
            ledges: self.ledges,
            doors: !self.no_doors,
            large: self.large,
            water: !self.no_water,
            open_locked: self.open_locked,
            keys: self.keys.clone(),
            ..d
        }
    }
}

/// Walk options.
#[derive(Args, Clone)]
pub struct WalkOpts {
    /// Walking speed, game units per second (default: the player's observed walking
    /// speed, 86.8 [observed, physics.md §6.3]; NPC speeds are not captured).
    #[arg(long, default_value_t = 86.8)]
    pub speed: f32,
    /// Give up after this many seconds of game time.
    #[arg(long, default_value_t = 600.0)]
    pub max_seconds: f32,
    /// The goal counts as reached within this horizontal distance.
    #[arg(long, default_value_t = 32.0)]
    pub goal_radius: f32,
    /// Record every Nth step in the trace (0: no trace).
    #[arg(long, default_value_t = 60)]
    pub every: usize,
    /// NIF schema directory (default: `nif` next to --schema).
    #[arg(long)]
    pub nif_schema: Option<std::path::PathBuf>,
    /// Terrain only: no collision from models.
    #[arg(long)]
    pub no_meshes: bool,
    /// The character's steepest walkable slope, degrees (default: fnv-phys's, 45
    /// [hypothesis]). Paths keep off steeper triangles unless --any-slope.
    #[arg(long)]
    pub max_slope: Option<f32>,
    /// Plan over every navmesh triangle, however steep for the character.
    #[arg(long)]
    pub any_slope: bool,
    /// Keep other placed actors' body collision in the world. Left out by default: in
    /// the game they are characters that move and give way, and actors avoiding each
    /// other is Layer 6 work.
    #[arg(long)]
    pub with_actors: bool,
}

/// The load order's records and the graph built on them.
struct Nav<'a> {
    lo: &'a LoadOrder<'a>,
    schema: &'a Schema,
    ix: WorldIndex,
    refs: RefTable,
    graph: NavGraph,
    timings: BTreeMap<&'static str, f64>,
}

fn ms(t: Instant) -> f64 {
    (t.elapsed().as_secs_f64() * 1e4).round() / 10.0
}

impl<'a> Nav<'a> {
    fn open(lo: &'a LoadOrder<'a>, schema: &'a Schema, with_refs: bool) -> Nav<'a> {
        let mut timings = BTreeMap::new();
        let t = Instant::now();
        let ix = WorldIndex::build(lo, schema);
        timings.insert("world_index_ms", ms(t));
        let t = Instant::now();
        let records = NavRecords::read(lo);
        timings.insert("read_navmeshes_ms", ms(t));
        let t = Instant::now();
        let graph = NavGraph::build(lo, schema, &ix, &records);
        timings.insert("build_graph_ms", ms(t));
        let refs = if with_refs {
            let t = Instant::now();
            let r = RefTable::from_index(lo, schema, &ix);
            timings.insert("refs_ms", ms(t));
            r
        } else {
            RefTable::default()
        };
        Nav { lo, schema, ix, refs, graph, timings }
    }

    fn space_of_cell(&self, cell: u32) -> Option<Space> {
        let c = self.ix.cells.get(&cell)?;
        Some(match c.world {
            Some(w) => Space::World(w),
            None => Space::Interior(c.form_id),
        })
    }

    fn space_by_name(&self, name: &str) -> anyhow::Result<Space> {
        if let Some(w) = self.ix.world_by_name(name) {
            return Ok(Space::World(w.form_id));
        }
        match self.ix.cell_by_name(name) {
            Some(c) if c.world.is_none() => Ok(Space::Interior(c.form_id)),
            Some(c) => Ok(Space::World(c.world.expect("exterior"))),
            None => bail!("no worldspace or cell {name}"),
        }
    }

    fn space_name(&self, s: Space) -> String {
        match s {
            Space::World(w) => self.ix.worldspaces.get(&w).and_then(|w| w.edid.clone()).unwrap_or_else(|| fid(w)),
            Space::Interior(c) => self.ix.cells.get(&c).and_then(|c| c.edid.clone()).unwrap_or_else(|| fid(c)),
        }
    }

    fn space_json(&self, s: Space) -> Value {
        let (kind, id) = match s {
            Space::World(w) => ("world", w),
            Space::Interior(c) => ("interior", c),
        };
        json!({"kind": kind, "form_id": fid(id), "edid": self.space_name(s)})
    }

    /// A place: "x,y,z" in `space`, or a placed reference's position and space.
    fn place(&self, spec: &str, space: &str) -> anyhow::Result<(Place, Option<u32>)> {
        let parts: Vec<&str> = spec.split(',').map(str::trim).collect();
        if parts.len() == 3 {
            if let (Ok(x), Ok(y), Ok(z)) = (parts[0].parse(), parts[1].parse(), parts[2].parse()) {
                return Ok((Place { space: self.space_by_name(space)?, pos: [x, y, z] }, None));
            }
        }
        let id = match self.refs.by_edid(spec) {
            Some(id) => id,
            None => spec.trim_start_matches("0x").trim_start_matches("0X").parse::<fnv_sim::FormId>().map_err(|_| anyhow!("{spec}: not x,y,z, a reference Editor ID or a FormID"))?,
        };
        let r = self.refs.get(id).ok_or_else(|| anyhow!("{spec}: no placed reference {id}"))?;
        let cell = r.cell.ok_or_else(|| anyhow!("{spec}: reference {id} has no cell"))?;
        let space = self.space_of_cell(cell.0).ok_or_else(|| anyhow!("{spec}: cell {} not indexed", cell))?;
        Ok((Place { space, pos: r.pos }, Some(id.0)))
    }

    /// How many navmeshes a corridor crosses (counting re-entries).
    fn navmeshes_crossed(&self, corridor: &[u32]) -> usize {
        let mut ms: Vec<u32> = corridor.iter().map(|&t| self.graph.mesh_of(t).form_id).collect();
        ms.dedup();
        ms.len()
    }

    fn path_json(&self, p: &Path) -> Value {
        json!({
            "length": p.length,
            "expanded": p.expanded,
            "start_snap": p.start_snap,
            "goal_snap": p.goal_snap,
            "legs": p.legs.iter().map(|l| json!({
                "space": self.space_json(l.space),
                "points": l.points,
                "corridor_triangles": l.corridor.len(),
                "water_triangles": l.corridor.iter().filter(|&&t| self.graph.tris[t as usize].has(fnv_nav::record::tri_flags::WATER)).count(),
                "navmeshes": self.navmeshes_crossed(&l.corridor),
                "door": l.door.map(|d| json!({
                    "door": fid(d.door),
                    "to_door": fid(d.to_door),
                    "arrival": d.arrival,
                })),
                "door_stops": l.door_stops.iter().map(|s| json!({"door": fid(s.door), "at": l.points[s.point]})).collect::<Vec<_>>(),
                "hops": l.hops.iter().map(|h| json!({"up": h.up, "take_off": l.points[h.point], "landing": l.points[h.point + 1]})).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }
}

fn place_json(nav: &Nav, p: &Place, r: Option<u32>) -> Value {
    json!({"space": nav.space_json(p.space), "pos": p.pos, "ref": r.map(fid)})
}

/// Connected areas per space, through edges a walking human may cross (internal and
/// portal links; no ledges, no doors): each usable triangle's area (the id of its first
/// triangle), and per space its areas as (id, triangles).
type Areas = BTreeMap<Space, Vec<(u32, usize)>>;

fn connected_areas(g: &fnv_nav::graph::NavGraph, only: Option<Space>) -> (Vec<u32>, Areas) {
    let mut comp = vec![NONE; g.tris.len()];
    let mut areas: Areas = BTreeMap::new();
    let mut stack = Vec::new();
    for s in 0..g.tris.len() as u32 {
        if comp[s as usize] != NONE || !g.usable(s) {
            continue;
        }
        let Some(space) = g.space_of_tri(s) else { continue };
        if only.is_some_and(|o| o != space) {
            continue;
        }
        let id = s;
        let mut n = 0usize;
        stack.push(s);
        comp[s as usize] = id;
        while let Some(t) = stack.pop() {
            n += 1;
            let tri = &g.tris[t as usize];
            for e in 0..3 {
                let m = tri.next[e];
                if matches!(tri.kind[e], EdgeKind::Internal | EdgeKind::Portal) && m != NONE && comp[m as usize] == NONE && g.usable(m) {
                    comp[m as usize] = id;
                    stack.push(m);
                }
            }
        }
        areas.entry(space).or_default().push((id, n));
    }
    (comp, areas)
}

pub fn graph_cmd(lo: &LoadOrder, schema: &Schema, only: Option<&str>) -> anyhow::Result<Value> {
    let nav = Nav::open(lo, schema, false);
    let g = &nav.graph;
    let only = only.map(|s| nav.space_by_name(s)).transpose()?;
    let (_, areas) = connected_areas(g, only);
    let mut spaces: Vec<(Space, Vec<usize>)> = areas.into_iter().map(|(s, a)| (s, a.into_iter().map(|(_, n)| n).collect())).collect();
    spaces.sort_by_key(|(_, a)| std::cmp::Reverse(a.iter().sum::<usize>()));
    Ok(json!({
        "schema": "fnv.nav.graph/1",
        "timings": nav.timings,
        "stats": g.stats,
        "spaces": spaces.iter().map(|(s, a)| {
            let mut a = a.clone();
            a.sort_unstable_by(|x, y| y.cmp(x));
            let total: usize = a.iter().sum();
            json!({
                "space": nav.space_json(*s),
                "triangles": total,
                "areas": a.len(),
                "largest_areas": a.iter().take(5).collect::<Vec<_>>(),
                "largest_share": a.first().map_or(0.0, |&l| l as f64 / total as f64),
                "single_triangle_areas": a.iter().filter(|&&n| n == 1).count(),
            })
        }).collect::<Vec<_>>(),
    }))
}

/// The exterior cells the largest connected navmesh area of a worldspace lies in, with
/// its triangles in each (fnv.nav.cells/1): where an actor can walk to from most of the
/// world. For WastelandNV it is the playable Mojave; the landscape beyond it, which has
/// no navmesh, is left out (the stress tests' tour drew its ring there, 2026-10-18).
pub fn cells_cmd(lo: &LoadOrder, schema: &Schema, space: &str) -> anyhow::Result<Value> {
    let nav = Nav::open(lo, schema, false);
    let g = &nav.graph;
    let sp = nav.space_by_name(space)?;
    if !matches!(sp, Space::World(_)) {
        anyhow::bail!("{space} is not a worldspace");
    }
    let (comp, areas) = connected_areas(g, Some(sp));
    let Some(&(largest, size)) = areas.get(&sp).and_then(|a| a.iter().max_by_key(|(_, n)| *n)) else {
        anyhow::bail!("{space} has no navmesh");
    };
    let mut cells: BTreeMap<(i32, i32), usize> = BTreeMap::new();
    for t in 0..g.tris.len() as u32 {
        if comp[t as usize] != largest {
            continue;
        }
        let c = g.corners(t);
        let (x, y) = ((c[0][0] + c[1][0] + c[2][0]) / 3.0, (c[0][1] + c[1][1] + c[2][1]) / 3.0);
        *cells.entry(((x / 4096.0).floor() as i32, (y / 4096.0).floor() as i32)).or_default() += 1;
    }
    Ok(json!({
        "schema": "fnv.nav.cells/1",
        "space": nav.space_json(sp),
        "triangles": size,
        "areas": areas.get(&sp).map_or(0, Vec::len),
        "cells": cells.iter().map(|(&(x, y), &n)| json!([x, y, n])).collect::<Vec<_>>(),
    }))
}

pub fn near_cmd(lo: &LoadOrder, schema: &Schema, at: &str, space: &str, radius: f32) -> anyhow::Result<Value> {
    let nav = Nav::open(lo, schema, true);
    let (p, r) = nav.place(at, space)?;
    let g = &nav.graph;
    let mut tris: Vec<(f32, u32)> = Vec::new();
    for t in 0..g.tris.len() as u32 {
        if g.space_of_tri(t) != Some(p.space) {
            continue;
        }
        let q = fnv_nav::graph::closest_on_triangle(p.pos, g.corners(t));
        let h = hdist(q, p.pos);
        if h <= radius {
            tris.push((h, t));
        }
    }
    tris.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(json!({
        "schema": "fnv.nav.near/1",
        "at": place_json(&nav, &p, r),
        "radius": radius,
        "located": g.locate(p.space, p.pos, 256.0),
        "triangles": tris.iter().map(|&(h, t)| {
            let tri = &g.tris[t as usize];
            let m = g.mesh_of(t);
            json!({
                "triangle": t,
                "navmesh": fid(m.form_id),
                "index": t - m.first_tri,
                "distance": h,
                "corners": g.corners(t),
                "flags": format!("{:#06x}", tri.flags),
                "edges": (0..3).map(|e| json!({
                    "kind": format!("{:?}", tri.kind[e]),
                    "to": (tri.next[e] != NONE).then_some(tri.next[e]),
                    "outline_ends": [g.boundary_vertex[tri.v[e] as usize], g.boundary_vertex[tri.v[(e + 1) % 3] as usize]],
                })).collect::<Vec<_>>(),
                "doors": g.doors.get(&t).map(|d| d.iter().map(|d| fid(d.door)).collect::<Vec<_>>()),
                "in_place_doors": g.door_tris.get(&t).map(|d| d.iter().map(|&d| fid(d)).collect::<Vec<_>>()),
            })
        }).collect::<Vec<_>>(),
    }))
}

pub fn path_cmd(lo: &LoadOrder, schema: &Schema, ends: &Ends, opts: &PathOpts) -> anyhow::Result<Value> {
    let nav = Nav::open(lo, schema, true);
    let (from, fr) = nav.place(&ends.from, &ends.space)?;
    let (to, tr) = nav.place(&ends.to, &ends.space)?;
    let params = opts.params();
    let t = Instant::now();
    let route = nav.graph.door_route(from, to, &params);
    let route_ms = ms(t);
    let t = Instant::now();
    let res = nav.graph.find_path(from, to, &params);
    let search_ms = ms(t);
    let mut v = json!({
        "schema": "fnv.nav.path/1",
        "from": place_json(&nav, &from, fr),
        "to": place_json(&nav, &to, tr),
        "params": params,
        "timings": nav.timings,
        "search_ms": search_ms,
        "door_route_ms": route_ms,
        "door_route": match &route {
            Ok(r) => json!(r.iter().map(|d| json!({"door": fid(d.door), "to_door": fid(d.to_door), "into": nav.graph.space_of_tri(d.to).map(|s| nav.space_name(s)), "locked": nav.graph.door_locks.get(&d.door)})).collect::<Vec<_>>()),
            Err(e) => json!(e.to_string()),
        },
    });
    match res {
        Ok(p) => v["path"] = nav.path_json(&p),
        Err(e) => v["error"] = json!(e.to_string()),
    }
    Ok(v)
}

/// The physics world around one leg of the walk.
struct LegWorld<'a> {
    space: Space,
    built: Built<'a>,
}

fn hdist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Distance from `p` to segment a-b, seen from above.
fn seg_dist(p: [f32; 3], a: [f32; 3], b: [f32; 3]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-9 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// A door that opens in place in the walk's physics world: its keyframed bodies follow
/// its animation clock (`fnv_collide::anim::DoorMotion`, kept across world rebuilds).
struct WalkDoor {
    pose: RefPose,
    anim: Arc<ModelAnim>,
    /// (object index, node block, body transform) of each keyframed collision object.
    objects: Vec<(u16, usize, fnv_collide::Iso)>,
    /// The sequence and time the bodies were last posed at.
    shown: Option<(bool, f32)>,
    /// Whether its moving parts were last made solid (not while it swings).
    solid: Option<bool>,
}

impl WalkDoor {
    fn pose(&mut self, world: &mut PhysWorld, m: &DoorMotion) {
        // Not solid while it swings (`DoorMotion::swinging`): the leaf cannot hit the walker.
        let solid = !m.swinging();
        if self.solid != Some(solid) {
            world.set_ref_keyframed_solid(self.pose.form_id, solid);
            self.solid = Some(solid);
        }
        let want = m.pose();
        if self.shown == Some(want) {
            return;
        }
        let Some((open, close)) = self.anim.door() else { return };
        let seq = if want.0 { open } else { close };
        for (object, block, body) in &self.objects {
            if let Some(node) = self.anim.node_to_root(Some(seq), want.1, *block) {
                world.pose_ref_object(&self.pose, *object, &node.iso, body);
            }
        }
        self.shown = Some(want);
    }
}

/// Advance the doors' clocks by `dt` and pose their bodies.
fn advance_doors(world: &mut PhysWorld, doors: &mut HashMap<u32, WalkDoor>, motion: &mut HashMap<u32, DoorMotion>, dt: f32) {
    for (id, d) in doors.iter_mut() {
        if let Some(m) = motion.get_mut(id) {
            m.update(dt);
            d.pose(world, m);
        }
    }
}

fn owner_json(o: &Owner) -> Value {
    serde_json::to_value(o).unwrap_or(Value::Null)
}

pub fn walk_cmd(lo: &LoadOrder, schema: &Schema, ends: &Ends, opts: &PathOpts, walk: &WalkOpts, (data_dir, esm_schema): (&FsPath, &FsPath)) -> anyhow::Result<Value> {
    let nav = Nav::open(lo, schema, true);
    let (from, fr) = nav.place(&ends.from, &ends.space)?;
    let (to, tr) = nav.place(&ends.to, &ends.space)?;
    let character = CharacterParams { max_slope_deg: walk.max_slope.unwrap_or(CharacterParams::default().max_slope_deg), ..Default::default() };
    let params = PathParams { max_slope_deg: (!walk.any_slope).then_some(character.max_slope_deg), ..opts.params() };
    let t = Instant::now();
    let path = nav.graph.find_path(from, to, &params).map_err(|e| anyhow!("no path: {e}"))?;
    let search_ms = ms(t);
    let first_path = nav.path_json(&path);
    let mut meshes = if walk.no_meshes { None } else { Meshes::open(data_dir, esm_schema, walk.nif_schema.as_deref())? };
    let phys = PhysParams::default();
    let dt = phys.dt;
    // An actor at the goal stands on it with its own body: arriving means getting next
    // to it (two capsule radii and the goal radius) [engine choice].
    let target_is_actor = tr.and_then(|r| nav.refs.get(fnv_sim::FormId(r))).is_some_and(|r| matches!(&r.sig, b"ACHR" | b"ACRE"));
    let goal_radius = if target_is_actor { walk.goal_radius + 2.0 * CharacterParams::default().radius + 8.0 } else { walk.goal_radius };
    let mut follower = Follower::new(path);
    follower.goal_radius = goal_radius;
    let mut leg_world: Option<LegWorld> = None;
    let mut c: Option<fnv_phys::Character> = None;
    let mut feet = from.pos;
    // Whether the character stands on something (not yet: until its first step).
    let mut on_ground = false;
    let mut recovery = fnv_nav::follow::PathRecovery::default();
    let mut build_ms = 0.0;
    let (mut builds, mut doors_used) = (0usize, Vec::new());
    let (mut steps, mut walked, mut max_dev, mut off_mesh, mut mesh_checks) = (0usize, 0.0f32, 0.0f32, 0usize, 0usize);
    let mut unstuck = fnv_nav::follow::Unstuck::new(walk.speed);
    let mut last_touched: Vec<Owner> = Vec::new();
    let mut detouring = false;
    let mut touched: BTreeMap<Owner, usize> = BTreeMap::new();
    let mut trace = Vec::new();
    let mut events = Vec::new();
    // Doors that open in place: their clocks (kept while the walk goes on), those in the
    // current leg's world, and the openings waited for.
    let mut door_motion: HashMap<u32, DoorMotion> = HashMap::new();
    let mut walk_doors: HashMap<u32, WalkDoor> = HashMap::new();
    let mut doors_opened: Vec<Value> = Vec::new();
    let mut waiting: Option<(u32, usize)> = None;
    let mut outcome = "timeout";
    let max_steps = (walk.max_seconds / dt) as usize;
    let solid = Filter { exclude_layers: layers::character_ignores(), ..Default::default() };
    let plugin_rule = plugin_in_world(nav.lo, nav.schema);
    // Doors that are not teleports (hinged doors, gates inside a cell) are opened by
    // actors walking through them [hypothesis: the game's NPCs open unlocked doors on
    // their way; the navmesh runs through them]. Those whose model has Open and Close
    // sequences stay in the world and are opened on the way, the walker waiting until
    // the door has opened (docs/design/navmesh.md §6.1); the others have no collision,
    // unless locked against the walker.
    let opens = |pl: &fnv_world::Placement| {
        let is_door = pl.base.and_then(|b| fnv_world::placed::base_object(nav.lo, nav.schema, b)).is_some_and(|b| &b.sig == b"DOOR");
        is_door
            && fnv_nav::graph::door_teleport(nav.lo, nav.schema, pl.form_id).is_none()
            && fnv_nav::graph::door_lock(nav.lo, nav.schema, pl.form_id).is_none_or(|l| params.open_locked || l.key.is_some_and(|k| params.keys.contains(&k)))
    };
    while steps < max_steps {
        let space = follower.space().unwrap_or(to.space);
        // (Re)build the physics world when the leg's space changes or the path leaves it.
        let leg_pts = follower.path.legs[follower.leg].points.clone();
        let needs = match &leg_world {
            None => true,
            Some(w) => w.space != space || (matches!(space, Space::World(_)) && leg_pts.iter().any(|p| !w.built.grids.contains_key(&cell_of(p[0], p[1])))),
        };
        if needs {
            let t = Instant::now();
            let centre = match space {
                Space::Interior(cell) => nav.ix.cells.get(&cell),
                Space::World(w) => nav.ix.worldspaces.get(&w).and_then(|ws| {
                    let g = cell_of(feet[0], feet[1]);
                    ws.grid.get(&g).or_else(|| ws.grid.values().next()).and_then(|id| nav.ix.cells.get(id))
                }),
            }
            .ok_or_else(|| anyhow!("no cell to build the physics world of {}", nav.space_name(space)))?;
            // Points every 2,048 units along the leg, so every cell it crosses loads.
            let mut around: Vec<(f32, f32)> = vec![(feet[0], feet[1])];
            for w in leg_pts.windows(2) {
                let n = (hdist(w[0], w[1]) / 2048.0).ceil().max(1.0) as usize;
                for k in 0..=n {
                    let s = k as f32 / n as f32;
                    around.push((w[0][0] + (w[1][0] - w[0][0]) * s, w[0][1] + (w[1][1] - w[0][1]) * s));
                }
            }
            // Doors of this space that open in place and can animate.
            let mut animated: HashMap<u32, (fnv_world::Placement, Arc<ModelAnim>)> = HashMap::new();
            if let Some(ms) = meshes.as_mut() {
                for d in nav.graph.in_place.values().filter(|d| d.space == space) {
                    let Some(pl) = fnv_world::placed::placement(nav.lo, nav.schema, d.door) else { continue };
                    let model = pl.base.and_then(|b| fnv_world::placed::base_object(nav.lo, nav.schema, b)).and_then(|b| b.model_path());
                    if let Some(anim) = model.and_then(|m| ms.door_anim(&m)) {
                        animated.insert(d.door, (pl, anim));
                    }
                }
            }
            // The walking actor's own reference (its body collision) is not in the world.
            let in_world = |pl: &fnv_world::Placement| {
                if Some(pl.form_id) == fr {
                    Err("the walker")
                } else if !walk.with_actors && matches!(&pl.sig, b"ACHR" | b"ACRE") {
                    Err("actor")
                } else if animated.contains_key(&pl.form_id) {
                    plugin_rule(pl)
                } else if opens(pl) {
                    Err("door opened by the walker")
                } else {
                    plugin_rule(pl)
                }
            };
            let options = |pl: &fnv_world::Placement| RefCollisionOptions { posed_doors: animated.contains_key(&pl.form_id), ..Default::default() };
            let b = build_with(nav.lo, nav.schema, &nav.ix, centre, 1, &around, phys, meshes.as_mut(), &in_world, &options);
            build_ms += ms(t);
            builds += 1;
            let mut lw = LegWorld { space, built: b };
            // Animated doors in the world: their bodies follow their clocks.
            walk_doors.clear();
            let ids: HashSet<u32> = animated.keys().copied().collect();
            for id in ids {
                if lw.built.world.ref_bodies(id).is_empty() {
                    continue;
                }
                let (pl, anim) = animated.remove(&id).expect("listed");
                let models = meshes.as_mut().map(|m| m as &mut dyn fnv_phys::ModelCollision);
                let rc = fnv_phys::ref_collision_with(nav.lo, nav.schema, &pl, models, &mut |_| {}, RefCollisionOptions { posed_doors: true, ..Default::default() });
                let objects: Vec<(u16, usize, fnv_collide::Iso)> = rc
                    .objects
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| o.motion == fnv_collide::Motion::Keyframed)
                    .map(|(i, o)| (i as u16, o.node_block, o.body_transform))
                    .collect();
                let Some((open, close)) = anim.door() else { continue };
                let m = *door_motion.entry(id).or_insert_with(|| DoorMotion::new(pl.open_by_default, open.end(), close.end()));
                let mut wd = WalkDoor { pose: RefPose::from_placement(&pl), anim: anim.clone(), objects, shown: None, solid: None };
                wd.pose(&mut lw.built.world, &m);
                walk_doors.insert(id, wd);
            }
            if !walk_doors.is_empty() {
                events.push(json!({"step": steps, "event": "animated_doors", "doors": walk_doors.keys().copied().map(fid).collect::<Vec<_>>()}));
            }
            // Stand the character on what is under its feet, starting the ray a little above
            // the navmesh surface there (actors are placed on the navmesh; a ray from high
            // up would land on furniture above an actor placed under it).
            if follower.next == 1 {
                feet = leg_pts[0];
            }
            let top = feet[2] + 48.0;
            let hit = lw.built.world.queries().ray([feet[0], feet[1], top], [0.0, 0.0, -1.0], 600.0, &solid);
            let start = match hit {
                Some(h) => [feet[0], feet[1], h.point[2] + character.rest_gap(h.shell)],
                None => feet,
            };
            c = Some(lw.built.world.add_character(0, character, start));
            feet = start;
            events.push(json!({"step": steps, "event": "world_built", "space": nav.space_name(space), "cells": lw.built.grids.len(), "feet": feet}));
            leg_world = Some(lw);
            unstuck.reset();
        }
        let lw = leg_world.as_mut().expect("built");
        let ch = c.as_mut().expect("placed");
        follower.swimming = ch.swimming;
        // In the air nothing is called a fall until it lands, as for the game's residents.
        let airborne = recovery.settling(!on_ground && !follower.swimming, dt);
        match follower.steer(feet) {
            Steer::Arrived => {
                outcome = "arrived";
                break;
            }
            Steer::OffPath => {
                let way = if airborne { Recovery::Waiting } else { follower.recover(&mut recovery, &nav.graph, Place { space, pos: feet }, walked, &params, dt) };
                match way {
                    // Stand (or fall) a step.
                    Recovery::Waiting => {
                        advance_doors(&mut lw.built.world, &mut walk_doors, &mut door_motion, dt);
                        let s = lw.built.world.move_character(ch, CharacterInput::default());
                        lw.built.world.step();
                        steps += 1;
                        feet = s.feet;
                        on_ground = s.grounded;
                    }
                    Recovery::Replanned(length) => {
                        events.push(json!({"step": steps, "event": "replanned_off_path", "feet": feet, "length": length}));
                        unstuck.reset();
                    }
                    Recovery::Waived(why) => {
                        events.push(json!({"step": steps, "event": "off_path_height_waived", "feet": feet, "why": why}));
                        unstuck.reset();
                    }
                }
            }
            Steer::OpenDoor { door, at } => {
                if !walk_doors.contains_key(&door) {
                    // No animation (or not in the world): nothing to wait for.
                    follower.door_opened();
                    events.push(json!({"step": steps, "event": "door_passed", "door": fid(door), "at": at}));
                    continue;
                }
                let m = door_motion.get_mut(&door).expect("animated doors have a clock");
                if m.is_open() {
                    follower.door_opened();
                    let waited = waiting.take().filter(|w| w.0 == door).map_or(0.0, |w| (steps - w.1) as f32 * dt);
                    doors_opened.push(json!({"step": steps, "door": fid(door), "at": at, "feet": feet, "waited": waited}));
                    unstuck.reset();
                    continue;
                }
                if waiting.is_none_or(|w| w.0 != door) {
                    m.set_open(true);
                    waiting = Some((door, steps));
                    events.push(json!({"step": steps, "event": "door_opening", "door": fid(door), "feet": feet}));
                }
                // Stand while it opens.
                advance_doors(&mut lw.built.world, &mut walk_doors, &mut door_motion, dt);
                let s = lw.built.world.move_character(ch, CharacterInput::default());
                lw.built.world.step();
                steps += 1;
                feet = s.feet;
                on_ground = s.grounded;
                unstuck.reset();
            }
            Steer::Door(d) => {
                follower.door_used();
                doors_used.push(json!({"step": steps, "door": fid(d.door), "to_door": fid(d.to_door), "arrival": d.arrival}));
                feet = d.arrival;
                unstuck.reset();
                leg_world = None; // the next leg's space builds on the next step
                continue;
            }
            Steer::Walk { dir, remaining, .. } => {
                let head = match unstuck.advise(remaining, dir, dt) {
                    fnv_nav::follow::Advice::Head(d) => d,
                    advice => {
                        let blockers: Vec<Value> = last_touched.iter().map(owner_json).collect();
                        if advice == fnv_nav::follow::Advice::GiveUp {
                            events.push(json!({"step": steps, "event": "gave_up", "feet": feet, "touching": blockers}));
                            outcome = "stuck";
                            break;
                        }
                        let goal = follower.destination().unwrap_or(to);
                        match nav.graph.find_path(Place { space, pos: feet }, goal, &params) {
                            Ok(p) => {
                                events.push(json!({"step": steps, "event": "replanned", "feet": feet, "touching": blockers, "length": p.length}));
                                follower = Follower::new(p);
                                follower.goal_radius = goal_radius;
                                unstuck.reset();
                                continue;
                            }
                            Err(e) => {
                                events.push(json!({"step": steps, "event": "replan_failed", "feet": feet, "error": e.to_string()}));
                                outcome = "stuck";
                                break;
                            }
                        }
                    }
                };
                if head != dir && !detouring {
                    events.push(json!({"step": steps, "event": "detour", "feet": feet, "touching": last_touched.iter().map(owner_json).collect::<Vec<_>>()}));
                }
                detouring = head != dir;
                let speed = walk.speed.min(remaining / dt);
                let input = CharacterInput { velocity: [head[0] * speed, head[1] * speed], ..Default::default() };
                let before = feet;
                advance_doors(&mut lw.built.world, &mut walk_doors, &mut door_motion, dt);
                let s = lw.built.world.move_character(ch, input);
                lw.built.world.step();
                steps += 1;
                feet = s.feet;
                on_ground = s.grounded;
                walked += hdist(before, feet);
                for o in &s.touched {
                    *touched.entry(*o).or_default() += 1;
                }
                last_touched = s.touched.clone();
                // Deviation from the segment being walked.
                let leg = &follower.path.legs[follower.leg];
                if follower.next >= 1 && follower.next < leg.points.len() {
                    max_dev = max_dev.max(seg_dist(feet, leg.points[follower.next - 1], leg.points[follower.next]));
                }
                if steps.is_multiple_of(15) {
                    mesh_checks += 1;
                    if nav.graph.nearest(space, feet, 48.0).is_none() {
                        off_mesh += 1;
                    }
                }
                if walk.every > 0 && steps.is_multiple_of(walk.every) {
                    trace.push(json!({"step": steps, "feet": feet, "leg": follower.leg, "next": follower.next, "grounded": s.grounded}));
                }
            }
        }
    }
    let goal = follower.destination().map(|d| d.pos).unwrap_or(to.pos);
    Ok(json!({
        "schema": "fnv.nav.walk/1",
        "from": place_json(&nav, &from, fr),
        "to": place_json(&nav, &to, tr),
        "params": params,
        "character": serde_json::to_value(character)?,
        "speed": walk.speed,
        "goal_radius": goal_radius,
        "timings": nav.timings,
        "search_ms": search_ms,
        "path": first_path,
        "summary": {
            "outcome": outcome,
            "seconds": steps as f32 * dt,
            "steps": steps,
            "end": feet,
            "end_space": follower.space().map(|s| nav.space_name(s)),
            "distance_to_goal": hdist(feet, goal),
            "walked": walked,
            "replans": unstuck.replans,
            "detours": unstuck.detours,
            "doors": doors_used,
            "doors_opened": doors_opened,
            "max_deviation": max_dev,
            "off_navmesh_checks": off_mesh,
            "navmesh_checks": mesh_checks,
            "physics_builds": builds,
            "physics_build_ms": build_ms,
            "touched": touched.iter().map(|(o, n)| json!({"owner": owner_json(o), "steps": n})).collect::<Vec<_>>(),
        },
        "events": events,
        "trace": trace,
    }))
}

/// One recorded actor sample.
struct Seen {
    cell: u32,
    pos: [f32; 3],
    moving: bool,
    /// Sitting, sleeping, dead or disabled: not standing on the navmesh.
    off_feet: bool,
}

/// Distance from `p` to a polyline, seen from above.
fn poly_dist(p: [f32; 3], pts: &[[f32; 3]]) -> f32 {
    match pts {
        [] => f32::INFINITY,
        [a] => hdist(p, *a),
        _ => pts.windows(2).map(|w| seg_dist(p, w[0], w[1])).fold(f32::INFINITY, f32::min),
    }
}

fn pct(v: &mut [f32], q: f32) -> f32 {
    if v.is_empty() {
        return f32::NAN;
    }
    v.sort_by(f32::total_cmp);
    v[((v.len() - 1) as f32 * q).round() as usize]
}

/// Read AI observer output ([`crate::observer`]): per actor, its samples in file order.
fn read_observed(lo: &LoadOrder, files: &[std::path::PathBuf]) -> anyhow::Result<BTreeMap<u32, Vec<Seen>>> {
    let mut actors: BTreeMap<u32, Vec<Seen>> = BTreeMap::new();
    crate::observer::read(lo, files, |row| {
        let (Some(id), Some(cell), Some(x), Some(y), Some(z)) = (row.id("id"), row.id("cell"), row.num("x"), row.num("y"), row.num("z")) else {
            return;
        };
        actors.entry(id).or_default().push(Seen {
            cell,
            pos: [x, y, z],
            moving: row.flag("moving"),
            off_feet: row.flag("sitting") || row.flag("sleeping") || row.flag("dead") || row.flag("disabled"),
        });
    })?;
    Ok(actors)
}

pub fn observed_cmd(lo: &LoadOrder, schema: &Schema, files: &[std::path::PathBuf], min_walk: f32) -> anyhow::Result<Value> {
    let nav = Nav::open(lo, schema, true);
    let actors = read_observed(lo, files)?;
    let g = &nav.graph;
    // (a) Recorded standing positions against the navmesh, each distinct position once.
    let mut dists: Vec<f32> = Vec::new();
    let mut far: Vec<(f32, u32, [f32; 3], String)> = Vec::new();
    let mut no_space = 0usize;
    for (&id, seen) in &actors {
        let mut distinct = std::collections::HashSet::new();
        for s in seen.iter().filter(|s| !s.off_feet) {
            if !distinct.insert((s.cell, s.pos.map(|v| v.round() as i32))) {
                continue;
            }
            let Some(space) = nav.space_of_cell(s.cell) else {
                no_space += 1;
                continue;
            };
            let d = g
                .nearest(space, s.pos, 512.0)
                .map_or(f32::INFINITY, |(_, q)| ((q[0] - s.pos[0]).powi(2) + (q[1] - s.pos[1]).powi(2) + (q[2] - s.pos[2]).powi(2)).sqrt());
            if d > 32.0 {
                far.push((d, id, s.pos, nav.space_name(space)));
            }
            dists.push(d);
        }
    }
    far.sort_by(|a, b| b.0.total_cmp(&a.0));
    let within = |r: f32| dists.iter().filter(|&&d| d <= r).count();
    // (b) Recorded walks (runs of moving samples in one cell) against planned paths.
    let mut walks = Vec::new();
    let mut all_dev: Vec<f32> = Vec::new();
    for (&id, seen) in &actors {
        let mut i = 0;
        while i < seen.len() {
            if !seen[i].moving {
                i += 1;
                continue;
            }
            let start = i.saturating_sub(1);
            let mut j = i;
            while j + 1 < seen.len() && seen[j + 1].moving && seen[j + 1].cell == seen[i].cell {
                j += 1;
            }
            let end = if j + 1 < seen.len() && seen[j + 1].cell == seen[i].cell { j + 1 } else { j };
            i = j + 1;
            let run = &seen[start..=end];
            if run.iter().any(|s| s.cell != run[0].cell) || hdist(run[0].pos, run[run.len() - 1].pos) < min_walk {
                continue;
            }
            let Some(space) = nav.space_of_cell(run[0].cell) else { continue };
            let (a, b) = (run[0].pos, run[run.len() - 1].pos);
            let observed_len: f32 = run.windows(2).map(|w| hdist(w[0].pos, w[1].pos)).sum();
            let name = nav.refs.get(fnv_sim::FormId(id)).and_then(|r| r.edid.clone()).unwrap_or_else(|| fid(id));
            match g.find_path(Place { space, pos: a }, Place { space, pos: b }, &PathParams::default()) {
                Ok(p) => {
                    let pts: Vec<[f32; 3]> = p.legs.iter().filter(|l| l.space == space).flat_map(|l| l.points.iter().copied()).collect();
                    let mut dev: Vec<f32> = run.iter().map(|s| poly_dist(s.pos, &pts)).collect();
                    all_dev.extend(&dev);
                    let mean = dev.iter().sum::<f32>() / dev.len() as f32;
                    walks.push(json!({
                        "actor": fid(id),
                        "edid": name,
                        "space": nav.space_name(space),
                        "samples": run.len(),
                        "from": a,
                        "to": b,
                        "observed_length": observed_len,
                        "planned_length": p.length,
                        "length_ratio": observed_len / p.length.max(1.0),
                        "deviation": {"mean": mean, "p90": pct(&mut dev, 0.9), "max": pct(&mut dev, 1.0)},
                    }));
                }
                Err(e) => walks.push(json!({"actor": fid(id), "edid": name, "space": nav.space_name(space), "from": a, "to": b, "error": e.to_string()})),
            }
        }
    }
    let planned = walks.iter().filter(|w| w.get("error").is_none()).count();
    let mut ratios: Vec<f32> = walks.iter().filter_map(|w| w["length_ratio"].as_f64()).map(|v| v as f32).collect();
    Ok(json!({
        "schema": "fnv.nav.observed/1",
        "files": files.iter().map(|f| f.file_name().map(|n| n.to_string_lossy().into_owned())).collect::<Vec<_>>(),
        "actors": actors.len(),
        "positions": {
            "distinct_standing": dists.len(),
            "no_space": no_space,
            "within_8": within(8.0),
            "within_32": within(32.0),
            "within_128": within(128.0),
            "within_512": within(512.0),
            "median": pct(&mut dists.clone(), 0.5),
            "p99": pct(&mut dists.clone(), 0.99),
            "farthest": far.iter().take(10).map(|(d, id, p, s)| json!({"distance": d, "actor": fid(*id), "pos": p, "space": s})).collect::<Vec<_>>(),
        },
        "walks": {
            "count": walks.len(),
            "planned": planned,
            "samples": all_dev.len(),
            "deviation_within_32": all_dev.iter().filter(|&&d| d <= 32.0).count(),
            "deviation_within_128": all_dev.iter().filter(|&&d| d <= 128.0).count(),
            "deviation_median": pct(&mut all_dev.clone(), 0.5),
            "deviation_p90": pct(&mut all_dev.clone(), 0.9),
            "length_ratio_median": pct(&mut ratios, 0.5),
            "list": walks,
        },
    }))
}
