//! Part P (docs/design/layer4-play.md §4): the physics world around the player — terrain
//! and references' collision streamed by cell, stepped at a fixed rate.
//!
//! Cells follow the player's feet ([`cells`]: 3×3 exterior squares with one square of
//! hysteresis, or the one interior). Reading and building a cell (records, models,
//! rapier shapes: [`prepare`]) happens on the loader thread ([`loader`]); the main thread
//! inserts at most [`CELLS_PER_FRAME`] prepared cells per frame, in the order they were
//! requested, and loads the player's own cell on the spot if it is not in yet (the
//! player never stands in a cell without collision). What a reference contributes is
//! `fnv_phys::ref_collision`, the rule `fnvtool phys cell` uses; whether it is in the
//! world is the simulation's call ([`in_world`]), made when the cell is inserted.
//! `RefChanged` removes or restores one reference's collision; `Travel` rebuilds
//! everything for the destination, which may lie in another worldspace (its own terrain,
//! persistent references and default water height; [`PhysState::worldspace`]). The
//! physics world sets [`Location`] when it applies a `Travel`.
//!
//! References the simulation created (dropped items, `crate::dropped`) load with the
//! physics cell containing them: their base form's model collision, dynamic when the
//! model says so ([`PhysState::add_created`]). While a dynamic one moves, its pose is
//! written back to the simulation ([`PhysState::settle_created`]).
//!
//! Placed references whose collision is one dynamic object (clutter) are written back
//! the same way once physics has moved them (pushed, knocked down: more than
//! [`MOVED_DISTANCE`] or [`MOVED_ROTATION`] from where they were added, so settling on
//! load is not a move): [`PhysState::settle_placed`]. Their cells then build their
//! collision at the simulation's pose (`prepare::Job::poses`), `crate::moved` draws them
//! there, and snapshots keep it. They stay in their placed cell (the cell lists come from
//! the plugins) [engineering choice]. That the game keeps a pushed item where it came to
//! rest is [hypothesis]. Placed clutter is added asleep, so it stays where it was placed
//! until something disturbs it, and a reference with more than one dynamic body (the
//! parts of a jointed object) keeps them keyframed (docs/spec-notes/physics.md §4.3.1;
//! `NVF_PLAY_TEST_PHYS_TRACE`, `phys/trace.rs`, logs what moves after a cell loads).

pub mod cells;
pub mod drawn;
mod loader;
pub mod prepare;
pub mod step_lift;
#[cfg(test)]
mod tests;
mod trace;

use crate::dropped;
use crate::player::Player;
use crate::sim::SimState;
use crate::{Location, PlayData, PlayWorld, RefChanged, Travel};
use bevy::prelude::*;
use cells::{PhysCell, Space};
use fnv_collide::{Iso, Motion};
use fnv_phys::terrain::height_at;
use drawn::DrawnFloors;
use fnv_phys::{Arrival, Character, CharacterParams, Filter, Owner, PhysParams, PhysWorld, RayHit, RefPose, Stepper, WaterArea};
use fnv_sim::{FormId, World};
use fnv_world::placed::placement;
use fnv_world::transform::{max_diff, ref_angles, ref_rotation};
use fnv_world::{cell_of, Placement, WorldIndex, CELL_SIZE};
use nvf_assets::GameData;
use prepare::{ms_since, Job, ModelCache, Prepared, RefShapes, Source};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Instant;

/// Prepared cells inserted per frame at most. Inserting one costs about a millisecond
/// (bodies, colliders and the broad-phase update); preparing it, 3–60 ms, mostly reading
/// and parsing models, happens on the loader thread (layer4-play.md §6).
pub const CELLS_PER_FRAME: usize = 1;

/// The same behind a loading screen (`crate::loading`), where frame time does not show.
pub const CELLS_BEHIND_LOADING_SCREEN: usize = 8;

/// The physics world. Ray queries for activation go through [`PhysState::ray`], which
/// keeps the player's capsule out (or `world.queries()` after `world.update_queries()`).
#[derive(Resource)]
pub struct PhysState {
    pub world: PhysWorld,
    pub stepper: Stepper,
    /// The player's character, once placed.
    pub player: Option<Character>,
    /// Characters of actors another system walks (`nvforever`'s residents), by
    /// reference: each is moved by its velocity at every fixed step, with the player.
    pub npcs: BTreeMap<u32, NpcBody>,
    /// The worldspace whose exterior cells and persistent references load outdoors
    /// (FormID; set with [`PhysState::set_world`]).
    pub worldspace: u32,
    /// The worldspace or the interior being played.
    pub space: Space,
    /// Loaded cells.
    pub cells: BTreeMap<PhysCell, LoadedCell>,
    /// The cell of every reference of the loaded cells, in the world or not (so a
    /// reference the simulation brings back can get its collision again).
    pub ref_cell: HashMap<u32, PhysCell>,
    /// References created at run time (dropped items) that have collision now.
    pub created: BTreeMap<u32, CreatedBody>,
    /// Placed references with one dynamic collision object, in the world now.
    pub placed_dynamic: BTreeMap<u32, PlacedDynamic>,
    /// Placed references whose pose [`PhysState::settle_placed`] wrote to the simulation
    /// this frame (for `crate::moved`).
    pub moved_now: Vec<u32>,
    /// Doors that open in place, in the world now: their keyframed bodies follow
    /// `crate::doors::Doors` ([`pose_doors`]).
    pub doors: BTreeMap<u32, PosedDoor>,
    /// The drawn floors of the references in the world whose collision stays where it
    /// was placed, with their pose ([`PhysState::drawn_ground`]).
    drawn: HashMap<u32, (Arc<DrawnFloors>, RefPose)>,
    /// How far below the player character's feet its body is drawn ([`drawn_sink`]),
    /// as of its last physics step.
    player_sink: f32,
    /// How far the player's body and view are drawn above where its feet and drawn
    /// ground put them, after a step or a bump ([`step_lift::StepLift`]).
    pub player_lift: step_lift::StepLift,
    /// The height of each living actor's drawn bounds, game units, as `crate::hitvol`
    /// last measured them: an actor's character is made this tall ([`PhysState::add_npc`]).
    pub actor_heights: HashMap<u32, f32>,
    /// Actors drawn standing without a character: their feet and how far below them
    /// the body is drawn ([`PhysState::standing_drawn_feet`]).
    standing: HashMap<u32, ([f32; 3], f32)>,
    pub stats: PhysStats,
    /// Feet before the last fixed step (the camera interpolates from it).
    pub(crate) prev_feet: [f32; 3],
    /// The worldspace's persistent references by the grid square they stand in.
    persistent_by_grid: HashMap<(i32, i32), Vec<u32>>,
    /// Collision objects per model path, shared with the loader thread.
    models: Arc<ModelCache>,
    /// Cells being prepared on the loader thread.
    loader: loader::Loader,
    /// Test aid: prepares cells instead of the plugins and archives.
    #[cfg(test)]
    fake: Option<fn(&Job) -> Prepared>,
}

/// The longest frame the game's virtual clock passes on, seconds (Bevy's `Time<Virtual>`
/// default `max_delta`): longer frames are slowed down to it.
pub const MAX_FRAME_SECONDS: f32 = 0.25;

/// The physics settings of play: as `PhysParams::default`, with enough fixed steps for a
/// whole frame of the virtual clock ([`MAX_FRAME_SECONDS`]). With the default 8 steps of
/// 1/60 s a frame longer than 0.133 s lost the rest, so on a busy machine (6 frames a
/// second) the player and the actors walking near it covered less ground than the game's
/// clock passed: Goodsprings' settlers came into view seconds of game time late
/// (detection.md §5) [design].
pub fn play_params() -> PhysParams {
    let p = PhysParams::default();
    PhysParams { max_substeps: (MAX_FRAME_SECONDS / p.dt).ceil() as u32, ..p }
}

impl Default for PhysState {
    fn default() -> Self {
        PhysState {
            world: PhysWorld::new(play_params()),
            stepper: Stepper::default(),
            player: None,
            npcs: BTreeMap::new(),
            worldspace: 0,
            space: Space::Exterior,
            cells: BTreeMap::new(),
            ref_cell: HashMap::new(),
            created: BTreeMap::new(),
            placed_dynamic: BTreeMap::new(),
            moved_now: Vec::new(),
            doors: BTreeMap::new(),
            drawn: HashMap::new(),
            player_sink: 0.0,
            player_lift: step_lift::StepLift::default(),
            actor_heights: HashMap::new(),
            standing: HashMap::new(),
            stats: PhysStats::default(),
            prev_feet: [0.0; 3],
            persistent_by_grid: HashMap::new(),
            models: Arc::default(),
            loader: loader::Loader::default(),
            #[cfg(test)]
            fake: None,
        }
    }
}

/// Drawn geometry this far below or above the collision under a character is not the
/// floor it stands on [design: the Prospector Saloon's porch boards are drawn 2.5 above
/// their collision, floating-actors.md §7].
pub const DRAWN_FLOOR_WINDOW: f32 = 4.0;

/// How far below a character's feet its body is drawn: down to the drawn ground under
/// it (`drawn`, from [`PhysState::drawn_ground`]), so the soles touch what the player
/// sees while the feet, and with them the reference's position, the eye and every
/// query, stay where the game measures them. Over terrain the drawn ground is the
/// collision's bare surface, the rest gap (`CharacterParams::rest_gap`, 3.5 units) below
/// the feet [observed: rest heights, physics.md §6.2; that the game draws the body on
/// the bare surface is [hypothesis] from the owner's report of 2026-10-16 that the
/// player's boots hung in the air, floating-actors.md §6]. Over a model it is the top of
/// its drawn geometry, which can lie above its collision and so above the feet: the body
/// is then drawn raised (a negative sink) [hypothesis: the owner's report of 2026-10-17
/// that Easy Pete sank into the Prospector Saloon's porch, floating-actors.md §7]. With
/// no ground found (`drawn` is `None`), the rest gap. None while swimming (the float
/// depth counts from the feet).
pub fn drawn_sink(c: &Character, drawn: Option<f32>) -> f32 {
    if c.swimming {
        0.0
    } else {
        drawn.map_or(c.gap, |z| c.feet[2] - z)
    }
}

/// Feet this close to a character's rest height over the collision under them stand
/// where a character would rest, game units [design: the observed rest heights spread
/// 0.35 either side of the shell, physics.md §6.2].
pub const REST_TOLERANCE: f32 = 0.5;

/// The ground under a point ([`PhysState::ground_drawn`]): the bare collision surface,
/// the drawn ground and the collision's contact shell.
struct GroundUnder {
    bare: f32,
    drawn: f32,
    shell: f32,
}

/// Tilted more than this (the cosine of the reference's up axis against the world's), a
/// model's drawn floors are not looked at [design: placed floors stand upright].
const UPRIGHT_MIN_COS: f32 = 0.996;

/// The highest drawn floor of a placed model over game point `at` within
/// [`DRAWN_FLOOR_WINDOW`] of its height, in game space; `None` when there is none or the
/// reference is tilted (its floors are then not under the point the same way).
fn drawn_floor(floors: &DrawnFloors, pose: &RefPose, at: [f32; 3]) -> Option<f32> {
    let r = &pose.rotation;
    let s = pose.scale;
    if r[8] < UPRIGHT_MIN_COS || s <= 0.0 {
        return None;
    }
    // Into the model's frame: world = R·(s·v) + position, R row-major.
    let d = [at[0] - pose.position[0], at[1] - pose.position[1], at[2] - pose.position[2]];
    let local = |k: usize| (r[k] * d[0] + r[3 + k] * d[1] + r[6 + k] * d[2]) / s;
    let (x, y, z) = (local(0), local(1), local(2));
    let w = DRAWN_FLOOR_WINDOW / s;
    let top = floors.height_at(x, y, z - w, z + w)?;
    Some(at[2] + (top - z) * s)
}

/// An actor's character in the physics world: who moves it sets `velocity`.
pub struct NpcBody {
    pub ch: Character,
    /// Feet before the last physics step, for drawing between steps
    /// ([`PhysState::npc_drawn_feet`]).
    pub prev_feet: [f32; 3],
    /// Wanted horizontal velocity (x, y), game units per second.
    pub velocity: [f32; 2],
    /// Jump at the next physics step (taken only standing on the ground, as the
    /// player's; cleared once stepped): an actor jumping a ledge or an obstacle
    /// (docs/spec-notes/actor-jumps.md).
    pub jump: bool,
    /// How far below the feet the body is drawn ([`drawn_sink`]), as of the last
    /// physics step.
    pub sink: f32,
    /// Drawn above that after a step or a bump ([`step_lift::StepLift`]).
    pub lift: step_lift::StepLift,
}

/// The steepest slope an actor's character walks: navmeshes cross slopes a little above
/// the player's limit [observed: 46 to 49 degrees on paths of the walk tests,
/// docs/design/navmesh.md §5; the value is an engine choice].
pub const NPC_MAX_SLOPE_DEG: f32 = 60.0;

/// The shortest character an actor gets, game units [design: a capsule at least twice
/// [`crate::blocking::MIN_RADIUS`] tall].
pub const NPC_MIN_HEIGHT: f32 = 2.0 * crate::blocking::MIN_RADIUS;

/// Actors drawn shorter than this share of the player's height get a character of their
/// own size ([`npc_params`]) [design].
pub const NPC_SMALL_SHARE: f32 = 0.75;

/// The character of an actor whose drawn bounds are `height` tall (`None`: not measured
/// yet): the player's, but for an actor under [`NPC_SMALL_SHARE`] of the player's height
/// as tall as it is drawn
/// and as wide as its block (`crate::blocking`: [`crate::blocking::RADIUS_SHARE`] of
/// its height, at least [`crate::blocking::MIN_RADIUS`]), so a small creature (a mantis
/// nymph drawn 30 units tall) is not walked in, and in the way as, a person's capsule
/// (owner's playtest list 7 item 35; physics.md §6.8) [design]. Larger actors keep the
/// player's capsule: a wider one would not fit through the doors and paths they walk.
pub fn npc_params(height: Option<f32>) -> CharacterParams {
    let base = CharacterParams { max_slope_deg: NPC_MAX_SLOPE_DEG, ..CharacterParams::default() };
    match height.filter(|h| h.is_finite() && *h < NPC_SMALL_SHARE * base.height) {
        Some(h) => {
            let height = h.max(NPC_MIN_HEIGHT);
            let radius = (crate::blocking::RADIUS_SHARE * height).clamp(crate::blocking::MIN_RADIUS, base.radius);
            CharacterParams { radius, height, step_width: base.step_width.min(radius), ..base }
        }
        None => base,
    }
}

/// A loaded cell.
#[derive(Debug, Clone, Default)]
pub struct LoadedCell {
    /// Its references (and, outdoors, the worldspace's persistent references standing in
    /// the square), by FormID, in the world or not.
    pub refs: Vec<u32>,
    /// LAND heights (exterior squares with terrain).
    pub heights: Option<Vec<f32>>,
}

/// The collision of a reference created at run time (a dropped item).
#[derive(Debug, Clone, PartialEq)]
pub struct CreatedBody {
    /// The collision object's node and body transforms, when its model has exactly one
    /// object and it became one body: that body's pose gives the reference's. Otherwise
    /// the pose is not written back (a model's bodies are not joined in `fnv-phys`).
    pub local: Option<Iso>,
    /// Some object is dynamic (it can fall, be pushed and roll).
    pub dynamic: bool,
    /// Its position when its collision was added.
    pub start: [f32; 3],
    /// The world's step count then.
    pub added_step: u64,
    /// At rest (its body asleep) since it last moved.
    pub settled: bool,
}

/// A door whose keyframed bodies follow its animation.
#[derive(Clone)]
pub struct PosedDoor {
    pub pose: RefPose,
    pub bodies: prepare::DoorBodies,
    /// The ONAM flag of its placement (its state when no simulation knows it).
    pub open_by_default: bool,
    /// The sequence (true: "Open") and time its bodies were last posed at.
    pub shown: Option<(bool, f32)>,
    /// Whether its moving parts were last made solid (not while it swings).
    pub solid: Option<bool>,
}

impl PosedDoor {
    /// Pose the door's keyframed bodies for `seq` ("Open" when true) at `time` seconds.
    fn pose(&mut self, world: &mut PhysWorld, open: bool, time: f32) {
        if self.shown == Some((open, time)) {
            return;
        }
        let anim = &self.bodies.anim;
        let Some((o, c)) = anim.door() else { return };
        let seq = if open { o } else { c };
        for (object, block, body) in &self.bodies.objects {
            if let Some(node) = anim.node_to_root(Some(seq), time, *block) {
                world.pose_ref_object(&self.pose, *object, &node.iso, body);
            }
        }
        self.shown = Some((open, time));
    }

    /// The timing of its sequences, for [`crate::doors::Doors::register`].
    pub fn timing(&self) -> Option<crate::doors::DoorTiming> {
        self.bodies.anim.door().map(|(o, c)| crate::doors::DoorTiming::from_sequences(o, c))
    }
}

/// A placed reference that physics can move: its collision is one dynamic object.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedDynamic {
    /// The object's node and body transforms (see [`CreatedBody::local`]).
    pub local: Iso,
    pub scale: f32,
    /// Position and row-major rotation it was added at.
    pub added: ([f32; 3], [f32; 9]),
    /// Physics has moved it since (beyond [`MOVED_DISTANCE`] / [`MOVED_ROTATION`]).
    pub moved: bool,
    /// It has come to rest again since it moved (its body asleep), for the log.
    pub rested: bool,
}

/// A placed dynamic reference counts as moved by physics once its pose is this far (game
/// units) from where it was added [engineering choice: clutter placed a little above its
/// support settles by a unit or less when its cell loads, which is not a push].
pub const MOVED_DISTANCE: f32 = 1.0;
/// Or turned this much (largest difference of a rotation matrix entry, about 1.1 degrees)
/// [engineering choice].
pub const MOVED_ROTATION: f32 = 0.02;

/// A dynamic created reference that falls this far below where its collision was added
/// has fallen through the world [engineering choice]: its collision is removed and it is
/// put back where it started (logged as a warning).
pub const CREATED_FELL_THROUGH: f32 = 10_000.0;

/// Running totals for logs.
#[derive(Debug, Clone, Default)]
pub struct PhysStats {
    pub cells_loaded: usize,
    pub load_ms: f64,
    /// Outcomes per reference: the `fnv.phys.cell/1` `refs.counts` names, plus
    /// `not_present` (deleted, disabled, or taken in the simulation).
    pub counts: BTreeMap<&'static str, usize>,
    /// Time spent per part of loading, all cells so far (preparing on whichever thread
    /// did it, inserting on the main thread).
    pub times: LoadTimes,
    /// Longest physics streaming work on the main thread in one frame once the player
    /// is placed (ms); the start loads the player's cell on the spot.
    pub worst_frame_ms: f64,
    /// Cells the player was about to stand in before the loader had them ready, loaded
    /// on the main thread.
    pub loaded_inline: usize,
}

/// Milliseconds per part of loading cells.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadTimes {
    /// Plugin records: cell, placements, base objects, LAND.
    pub records: f64,
    /// Model files read from the archives.
    pub read: f64,
    /// NIF parsing.
    pub parse: f64,
    /// NIF collision blocks to `fnv_collide` collision objects.
    pub convert: f64,
    /// rapier shapes: terrain meshes, reference shapes (trimesh trees, hulls).
    pub shapes: f64,
    /// Bodies and colliders into the world.
    pub insert: f64,
    /// Broad-phase update after inserting (`PhysWorld::update_queries`).
    pub queries: f64,
}

impl LoadTimes {
    fn add(&mut self, o: &LoadTimes) {
        self.records += o.records;
        self.read += o.read;
        self.parse += o.parse;
        self.convert += o.convert;
        self.shapes += o.shapes;
        self.insert += o.insert;
        self.queries += o.queries;
    }
}

impl std::fmt::Display for LoadTimes {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "records {:.1}, read {:.1}, parse {:.1}, convert {:.1}, shapes {:.1}, insert {:.1}, queries {:.1} ms",
            self.records, self.read, self.parse, self.convert, self.shapes, self.insert, self.queries
        )
    }
}

/// The preparing parts of [`LoadTimes`].
struct PrepareTimes<'a>(&'a LoadTimes);

impl std::fmt::Display for PrepareTimes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let t = self.0;
        write!(
            f,
            "records {:.1}, read {:.1}, parse {:.1}, convert {:.1}, shapes {:.1} ms",
            t.records, t.read, t.parse, t.convert, t.shapes
        )
    }
}

/// `job` with the simulation's poses of its references that physics or a script moved,
/// and the scales scripts set (SetScale).
fn with_poses(mut job: Job, sim: &SimState) -> Job {
    job.poses = job.refs.iter().filter_map(|&r| sim.moved_pose(r).map(|(p, a)| (r, p, a))).collect();
    job.scales = job.refs.iter().filter_map(|&r| sim.script_scale(r).map(|s| (r, s))).collect();
    job
}

/// What loading needs besides the physics state.
pub struct LoadCtx<'a> {
    pub d: &'a PlayData,
    pub sim: &'a SimState,
    /// The archives; without them only terrain and collision primitives load.
    pub data: Option<&'a GameData>,
}

/// Whether a placed reference is in the world now: not deleted in the plugins, and
/// present for the simulation. A reference the simulation has not touched also needs to
/// be enabled by its own flag, as render streaming and `fnvtool phys` require; once the
/// simulation holds state for it, the simulation alone decides (so a reference enabled
/// at run time gets its collision).
pub fn in_world(sim: &SimState, pl: &Placement) -> bool {
    if pl.deleted() {
        return false;
    }
    match sim.world.peek(FormId(pl.form_id)) {
        Some(_) => sim.present(pl.form_id),
        None => !pl.initially_disabled() && sim.present(pl.form_id),
    }
}

impl PhysState {
    /// Play outdoors in worldspace `world`: index its persistent references by grid
    /// square (the square containing the placement, as render streaming and `fnvtool
    /// phys` assign them). Its terrain, cells and default water height follow from
    /// `world` as cells load. Loaded exterior cells belong to the previous worldspace:
    /// [`PhysState::reset`] first.
    ///
    /// Each worldspace's own records are used. A child worldspace (WRLD `WNAM`) may take
    /// land, water and other data from its parent by its `PNAM` flags [documented: xEdit
    /// names, schemas/fnv/world.json]; every child worldspace in the ten masters sets only
    /// "Use Map Data" and has its own land, `DNAM` and `NAM2` [observed: `fnvtool world
    /// list`, `esm decode`], so taking the parent's is not implemented (logged when a
    /// plugin asks for it).
    pub fn set_world(&mut self, d: &PlayData, world: u32) {
        self.worldspace = world;
        self.persistent_by_grid.clear();
        let ws = d.index.worldspaces.get(&world);
        if let Some(w) = ws.filter(|w| w.parent.is_some() && w.parent_flags & !PNAM_USE_MAP_DATA != 0) {
            warn!(
                "worldspace {world:08X} takes data from its parent (PNAM 0x{:X}); NVForever uses its own records only",
                w.parent_flags
            );
        }
        if let Some(pc) = ws.and_then(|w| w.persistent_cell).and_then(|c| d.index.cells.get(&c)) {
            for &r in &pc.refs {
                if let Some(p) = placement(d.lo, d.schema, r) {
                    self.persistent_by_grid.entry(cell_of(p.position[0], p.position[1])).or_default().push(r);
                }
            }
        }
    }

    /// The cell the player stands in at `feet`.
    pub fn cell_at(&self, feet: [f32; 3]) -> PhysCell {
        self.space.cell_at(feet)
    }

    /// Whether the cell under `feet` is loaded (the player may move).
    pub fn ready_at(&self, feet: [f32; 3]) -> bool {
        self.cells.contains_key(&self.cell_at(feet))
    }

    /// Whether every cell kept around `feet` is loaded (outdoors the squares of
    /// [`cells::wanted`], indoors the one cell). The start waits for this, not only for
    /// the cell underfoot: a reference placed in a neighbouring square can reach over the
    /// start (the Bison Steve rollercoaster track over Primm, #1103), and a start placed
    /// before it is in finds only the terrain below.
    pub fn ready_around(&self, feet: [f32; 3]) -> bool {
        match self.space {
            Space::Interior(_) => self.ready_at(feet),
            Space::Exterior => cells::wanted(cell_of(feet[0], feet[1])).into_iter().all(|(x, y)| self.cells.contains_key(&PhysCell::Exterior(x, y))),
        }
    }

    /// Where an actor's character is drawn this frame: between its feet before and after
    /// the last physics step, by the time accumulated towards the next (as the player's
    /// body), so a walk is as smooth on screen as the frame rate allows. `None` when the
    /// actor has no character.
    /// The body is drawn [`drawn_sink`] below the character's feet.
    pub fn npc_drawn_feet(&self, id: u32) -> Option<[f32; 3]> {
        let b = self.npcs.get(&id)?;
        let a = (self.stepper.accumulated / self.world.params.dt).clamp(0.0, 1.0);
        Some(std::array::from_fn(|i| b.prev_feet[i] + (b.ch.feet[i] - b.prev_feet[i]) * a - if i == 2 { b.sink - b.lift.at(a) } else { 0.0 }))
    }

    /// How far below its character's feet actor `id`'s body is drawn ([`drawn_sink`]; the
    /// player is `FormId::PLAYER_REF`); for an actor drawn standing without one, how far
    /// below its feet it was last drawn ([`PhysState::standing_drawn_feet`]); else 0.
    pub fn drawn_sink_of(&self, id: u32) -> f32 {
        if id == fnv_sim::FormId::PLAYER_REF.0 {
            if self.player.is_some() { self.player_sink } else { 0.0 }
        } else {
            self.npcs.get(&id).map(|b| b.sink).or_else(|| self.standing.get(&id).map(|l| l.1)).unwrap_or(0.0)
        }
    }

    /// The height of the drawn ground under a character's feet ([`PhysState::ground_drawn`]
    /// from just above them, as far down as its gap and [`DRAWN_FLOOR_WINDOW`] reach).
    pub fn drawn_ground(&mut self, c: &Character) -> Option<f32> {
        self.ground_drawn([c.feet[0], c.feet[1], c.feet[2] + 1.0], 1.0 + c.gap + DRAWN_FLOOR_WINDOW, Some(c.key())).map(|(_, drawn)| drawn)
    }

    /// The ground under `at` within `down` (the first collision a vertical ray meets,
    /// leaving out characters, actor hit volumes and the layers characters pass
    /// through): its bare surface's height and the drawn ground's. The drawn ground is
    /// that surface, or, when it is a placed model with drawn floors, the highest of its
    /// drawn floors within [`DRAWN_FLOOR_WINDOW`] of it (`drawn::DrawnFloors`).
    pub fn ground_drawn(&mut self, at: [f32; 3], down: f32, exclude: Option<fnv_phys::Key>) -> Option<(f32, f32)> {
        self.ground_under(at, down, exclude).map(|g| (g.bare, g.drawn))
    }

    fn ground_under(&mut self, at: [f32; 3], down: f32, exclude: Option<fnv_phys::Key>) -> Option<GroundUnder> {
        let mut filter = Filter { exclude_layers: fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes()), ..Default::default() };
        filter.exclude.extend(self.npcs.values().map(|b| b.ch.key()).chain(exclude));
        let hit = self.ray(at, [0.0, 0.0, -1.0], down, filter)?;
        Some(GroundUnder { bare: hit.point[2], drawn: self.drawn_over(&hit), shell: hit.shell })
    }

    /// The drawn ground at a downward ray's hit: the hit's height, or, on a placed model
    /// with drawn floors, the highest of them within [`DRAWN_FLOOR_WINDOW`] of it.
    pub fn drawn_over(&self, hit: &RayHit) -> f32 {
        let bare = hit.point[2];
        let drawn = match hit.owner {
            Owner::Ref { form_id, .. } => self.drawn.get(&form_id).and_then(|(floors, pose)| drawn_floor(floors, pose, hit.point)),
            _ => None,
        };
        drawn.unwrap_or(bare)
    }

    /// Bring every character's drawn sink up to date (after its physics step). A change
    /// of the drawn ground is taken up by the character's lift ([`step_lift::StepLift`]).
    pub fn update_drawn_sinks(&mut self) {
        if let Some(c) = self.player.clone() {
            let sink = drawn_sink(&c, self.drawn_ground(&c));
            if !c.swimming {
                self.player_lift.ground_moved(self.player_sink - sink);
            }
            self.player_sink = sink;
        }
        let chars: Vec<(u32, Character)> = self.npcs.iter().map(|(id, b)| (*id, b.ch.clone())).collect();
        for (id, ch) in chars {
            let sink = drawn_sink(&ch, self.drawn_ground(&ch));
            if let Some(b) = self.npcs.get_mut(&id) {
                if !ch.swimming {
                    b.lift.ground_moved(b.sink - sink);
                }
                b.sink = sink;
            }
        }
    }

    /// How far above its feet's drawn height actor `id`'s body is drawn this frame, after
    /// a step or a bump ([`step_lift::StepLift`]; the player is `FormId::PLAYER_REF`); 0
    /// for an actor without a character.
    pub fn drawn_lift_of(&self, id: u32) -> f32 {
        let a = (self.stepper.accumulated / self.world.params.dt).clamp(0.0, 1.0);
        if id == fnv_sim::FormId::PLAYER_REF.0 {
            if self.player.is_some() { self.player_lift.at(a) } else { 0.0 }
        } else {
            self.npcs.get(&id).map_or(0.0, |b| b.lift.at(a))
        }
    }

    /// Whether the player is placed and every cell asked for around them is in (the
    /// loading screen waits for this, `crate::loading`).
    pub fn loading_done(&self) -> bool {
        self.player.as_ref().is_some_and(|c| self.ready_at(c.feet)) && self.loader.pending().next().is_none()
    }

    /// Terrain height under (x, y), when that square is loaded and has terrain.
    pub fn terrain_z(&self, x: f32, y: f32) -> Option<f32> {
        if self.space != Space::Exterior {
            return None;
        }
        let (gx, gy) = cell_of(x, y);
        let h = self.cells.get(&PhysCell::Exterior(gx, gy))?.heights.as_deref()?;
        height_at(h, x - gx as f32 * CELL_SIZE, y - gy as f32 * CELL_SIZE)
    }

    /// Whether a reference has collision bodies in the world now.
    /// The trigger volumes (Trigger-layer phantoms) of a model, in the model's frame, once
    /// the model was read for a loaded cell; `None` until then (`crate::script_trigger`).
    pub fn model_triggers(&self, path: &str) -> Option<Arc<Vec<fnv_collide::CollisionObject>>> {
        self.models.triggers(path)
    }

    pub fn has_collision(&self, form_id: u32) -> bool {
        !self.world.ref_bodies(form_id).is_empty()
    }

    /// First hit of a ray (game space), never the player's own capsule. For the crosshair:
    /// `ray(player.eye(), player.look_dir(), reach, Filter::default())`.
    pub fn ray(&mut self, origin: [f32; 3], dir: [f32; 3], max_distance: f32, mut filter: Filter) -> Option<RayHit> {
        if let Some(c) = &self.player {
            filter.exclude.push(c.key());
        }
        self.world.update_queries();
        self.world.queries().ray(origin, dir, max_distance, &filter)
    }

    /// Every hit of a ray (game space), nearest first, never the player's own capsule.
    pub fn ray_all(&mut self, origin: [f32; 3], dir: [f32; 3], max_distance: f32, mut filter: Filter) -> Vec<RayHit> {
        if let Some(c) = &self.player {
            filter.exclude.push(c.key());
        }
        self.world.update_queries();
        self.world.queries().ray_all(origin, dir, max_distance, &filter)
    }

    /// What stands under `at`, from `up` above it to `down` below, ignoring the layers
    /// the character passes through and actor hit volumes.
    pub fn ground_below(&mut self, at: [f32; 3], up: f32, down: f32) -> Option<RayHit> {
        let filter = Filter { exclude_layers: fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes()), ..Default::default() };
        self.ray([at[0], at[1], at[2] + up], [0.0, 0.0, -1.0], up + down, filter)
    }

    /// What preparing a cell reads.
    fn job(&self, d: &PlayData, cell: PhysCell) -> Job {
        let (record, persistent) = match cell {
            PhysCell::Exterior(x, y) => {
                let ws = d.index.worldspaces.get(&self.worldspace);
                (ws.and_then(|w| w.grid.get(&(x, y))).and_then(|id| d.index.cells.get(id)), self.persistent_by_grid.get(&(x, y)))
            }
            PhysCell::Interior(id) => (d.index.cells.get(&id), None),
        };
        let mut refs: Vec<u32> = record.map(|c| c.refs.clone()).unwrap_or_default();
        refs.extend(persistent.into_iter().flatten());
        refs.sort_unstable();
        refs.dedup();
        let land = match cell {
            PhysCell::Exterior(..) => record.and_then(|c| c.land),
            PhysCell::Interior(_) => None,
        };
        Job { cell, refs, land, poses: Vec::new(), scales: Vec::new() }
    }

    fn source(&self, ctx: &LoadCtx) -> Source {
        Source { d: *ctx.d, data: ctx.data.cloned(), models: self.models.clone(), params: self.world.params }
    }

    /// The work of preparing `cell`, to run on any thread.
    fn work(&self, ctx: &LoadCtx, cell: PhysCell) -> loader::Work {
        let job = with_poses(self.job(ctx.d, cell), ctx.sim);
        #[cfg(test)]
        if let Some(fake) = self.fake {
            return Box::new(move || fake(&job));
        }
        let src = self.source(ctx);
        Box::new(move || prepare::cell(&src, &job))
    }

    /// Load one cell now, on this thread: terrain, then its references' collision by
    /// FormID (forgetting a request for it on the loader thread).
    pub fn load(&mut self, ctx: &LoadCtx, cell: PhysCell) {
        if self.cells.contains_key(&cell) {
            return;
        }
        self.loader.cancel(cell);
        let p = self.work(ctx, cell)();
        self.insert(ctx, p);
    }

    /// Put a prepared cell into the world: water, terrain, then its references in the
    /// job's (FormID) order, those not in the world now left out.
    fn insert(&mut self, ctx: &LoadCtx, p: Prepared) {
        let t0 = Instant::now();
        let Prepared { cell, heights, terrain, refs, times } = p;
        let d = ctx.d;
        // An exterior cell's water (`Cell::water_surface`, the rule the renderer draws it
        // with); not an interior's XCLW, where the game swam at a placed water object's
        // height instead [observed: physics.md §6.6; placed water not modelled yet].
        if let PhysCell::Exterior(x, y) = cell {
            let ws = d.index.worldspaces.get(&self.worldspace);
            let record = ws.and_then(|w| w.grid.get(&(x, y))).and_then(|id| d.index.cells.get(id));
            let surface = record.and_then(|c| c.water_surface(ws.and_then(|w| w.default_water_height)));
            self.world.set_water(water_area(cell), surface);
        }
        if let Some(t) = terrain {
            self.world.insert_terrain(t);
        }
        let mut ids = Vec::with_capacity(refs.len());
        let mut with_collision = 0;
        for e in refs {
            ids.push(e.form_id);
            self.ref_cell.insert(e.form_id, cell);
            let shapes = e.shapes;
            with_collision += self.apply(ctx, e.placement, |_| shapes.expect("prepared with its placement")) as usize;
        }
        // Dropped items: the simulation's created references standing in this cell
        // (FormIDs above every placed one, so the order stays by FormID).
        let (space, world) = (self.space, self.worldspace);
        let created: Vec<u32> = ctx
            .sim
            .world
            .created()
            .filter(|(_, s)| dropped::present(s) && !fnv_game::ai::is_actor_base(ctx.sim.world.forms, s.base) && dropped::phys_cell(d.index, world, space, s) == Some(cell))
            .map(|(id, _)| id.0)
            .collect();
        for r in created {
            self.ref_cell.insert(r, cell);
            with_collision += self.add_created(ctx, r) as usize;
            ids.push(r);
        }
        let n = ids.len();
        self.cells.insert(cell, LoadedCell { refs: ids, heights });
        let insert_ms = ms_since(t0);
        self.stats.times.add(&times);
        self.stats.times.insert += insert_ms;
        let prepare_ms = times.records + times.read + times.parse + times.convert + times.shapes;
        self.stats.cells_loaded += 1;
        self.stats.load_ms += prepare_ms + insert_ms;
        debug!(
            "physics cell {cell:?} loaded: {n} references, {with_collision} with collision; prepared in {prepare_ms:.1} ms ({}), inserted in {insert_ms:.1} ms; all cells so far: {:?}",
            PrepareTimes(&times),
            self.stats.counts
        );
    }

    /// Add a reference's collision if it is in the world and has any (`shapes` builds it
    /// once it is known to be). Returns whether it now has collision.
    fn apply(&mut self, ctx: &LoadCtx, pl: Option<Placement>, shapes: impl FnOnce(&Placement) -> RefShapes) -> bool {
        let counts = &mut self.stats.counts;
        let Some(pl) = pl else {
            *counts.entry("unreadable").or_default() += 1;
            return false;
        };
        if !in_world(ctx.sim, &pl) {
            *counts.entry("not_present").or_default() += 1;
            return false;
        }
        let RefShapes { tallies, water, prepared, dynamic_local, door, drawn } = shapes(&pl);
        for what in tallies {
            *counts.entry(what).or_default() += 1;
        }
        // Placed water (physics.md §6.6): volumes only, removed with the reference; added
        // before the bodies, in the order `fnvtool phys` uses.
        if !water.is_empty() {
            self.world.add_placed_water(&RefPose::from_placement(&pl), &water);
            *counts.entry("water_volume_refs").or_default() += 1;
        }
        let Some(prepared) = prepared else { return false };
        // A panic putting the bodies in the world (bad data met by a bug of ours,
        // `nvf_assets::guard`): the reference has no collision; the bodies added before
        // it stopped are taken out.
        let rep = match nvf_assets::guard::guarded(|| Ok(self.world.insert_ref(prepared))) {
            Ok(rep) => rep,
            Err(e) => {
                warn!("phys: 0x{:08X}: collision not added ({e}); the reference has none", pl.form_id);
                self.world.remove_ref(pl.form_id);
                *self.stats.counts.entry("add_failed").or_default() += 1;
                return false;
            }
        };
        let counts = &mut self.stats.counts;
        *counts.entry("with_collision").or_default() += 1;
        match drawn {
            Some(d) => {
                self.drawn.insert(pl.form_id, (d, RefPose::from_placement(&pl)));
            }
            None => {
                self.drawn.remove(&pl.form_id);
            }
        }
        if !rep.errors.is_empty() {
            *counts.entry("object_errors").or_default() += rep.errors.len();
        }
        if let (Some(local), 1) = (dynamic_local, rep.bodies) {
            let added = (pl.position, ref_rotation(pl.rotation));
            self.placed_dynamic.insert(pl.form_id, PlacedDynamic { local, scale: pl.scale, added, moved: false, rested: false });
        }
        if let Some(bodies) = door.filter(|d| !d.objects.is_empty()) {
            *counts.entry("animated_doors").or_default() += 1;
            let pose = RefPose::from_placement(&pl);
            self.doors.insert(pl.form_id, PosedDoor { pose, bodies, open_by_default: pl.open_by_default, shown: None, solid: None });
        }
        rep.bodies > 0
    }

    /// Add a reference's collision if it is in the world and has any. Returns whether it
    /// now has collision.
    fn add_ref(&mut self, ctx: &LoadCtx, r: u32) -> bool {
        let src = self.source(ctx);
        let mut times = LoadTimes::default();
        let t = Instant::now();
        let mut pl = placement(ctx.d.lo, ctx.d.schema, r).map(|pl| prepare::at_pose(pl, ctx.sim.moved_pose(r)));
        // A script's scale (SetScale) in place of the placement's; one too small to see
        // has no collision (`crate::sim::MIN_DRAWN_SCALE`).
        if let (Some(p), Some(s)) = (pl.as_mut(), ctx.sim.script_scale(r)) {
            if s <= crate::sim::MIN_DRAWN_SCALE {
                return false;
            }
            p.scale = s;
        }
        times.records += ms_since(t);
        let added = self.apply(ctx, pl, |pl| prepare::reference(&src, pl, &mut times));
        self.stats.times.add(&times);
        added
    }

    /// Drop a cell: its terrain and its references' collision (or its request).
    pub fn unload(&mut self, cell: PhysCell) {
        if self.loader.cancel(cell) {
            debug!("physics cell {cell:?} no longer wanted before it was ready");
        }
        let Some(c) = self.cells.remove(&cell) else { return };
        self.world.set_water(water_area(cell), None);
        if let PhysCell::Exterior(x, y) = cell {
            self.world.remove_terrain((x, y));
        }
        for r in c.refs {
            self.world.remove_ref(r);
            self.created.remove(&r);
            self.placed_dynamic.remove(&r);
            self.doors.remove(&r);
            self.drawn.remove(&r);
            if self.ref_cell.get(&r) == Some(&cell) {
                self.ref_cell.remove(&r);
            }
        }
        debug!("physics cell {cell:?} unloaded");
    }

    /// Bring a reference's collision in line with the simulation: removed when it is no
    /// longer present, added back when it is present again and its cell is loaded.
    pub fn sync_ref(&mut self, ctx: &LoadCtx, form_id: u32) {
        if FormId(form_id).is_dynamic() {
            self.sync_created(ctx, form_id);
            return;
        }
        if !ctx.sim.present(form_id) {
            self.placed_dynamic.remove(&form_id);
            self.doors.remove(&form_id);
            self.drawn.remove(&form_id);
            if self.world.remove_ref(form_id) {
                debug!("collision of {form_id:08X} removed");
            }
        } else if self.ref_cell.contains_key(&form_id) && ctx.sim.script_scale(form_id).is_some() {
            // A script set its scale (SetScale): built again at that scale.
            self.placed_dynamic.remove(&form_id);
            self.doors.remove(&form_id);
            self.drawn.remove(&form_id);
            self.world.remove_ref(form_id);
            if self.add_ref(ctx, form_id) {
                debug!("collision of {form_id:08X} built at scale {}", ctx.sim.ref_scale(form_id));
            }
        } else if self.ref_cell.contains_key(&form_id) && !self.has_collision(form_id) && self.add_ref(ctx, form_id) {
            debug!("collision of {form_id:08X} restored");
        }
    }

    /// A script moved or turned reference `id` (SetPos, SetAngle, Rotate, SetScale, the
    /// console's too): when its collision is one dynamic body still at the old pose, the
    /// settle write-back ([`Self::settle_created`], [`Self::settle_placed`]) would copy that
    /// pose over the move, so the collision is built again at the simulation's pose.
    /// Found 2026-11-01: a raised bottle read its old height again at
    /// once [observed: GetPos Z 8396.80 after SetPos Z 8461]. Collision that is not one
    /// dynamic body is left where it was (not written back, so no move is lost; it
    /// collides at the old place until its cell loads again) [engineering choice].
    pub fn repose_ref(&mut self, ctx: &LoadCtx, id: u32) {
        let single = match self.created.get(&id) {
            Some(cb) => cb.local.filter(|_| cb.dynamic).map(|l| (l, 1.0)),
            None => self.placed_dynamic.get(&id).map(|p| (p.local, p.scale)),
        };
        let Some((local, scale)) = single else { return };
        let (Some(b), Some(s)) = (self.world.ref_bodies(id).first().copied(), ctx.sim.world.peek(FormId(id))) else { return };
        let (pos, rot) = dropped::ref_pose_from_body(&b, &local, scale);
        let dist = (0..3).map(|i| (pos[i] - s.pos[i]).powi(2)).sum::<f32>().sqrt();
        if dist <= MOVED_DISTANCE && max_diff(&ref_rotation(s.rot), &rot) <= 1e-3 {
            return;
        }
        self.world.remove_ref(id);
        let built = if FormId(id).is_dynamic() {
            self.created.remove(&id);
            self.add_created(ctx, id)
        } else {
            self.placed_dynamic.remove(&id);
            self.doors.remove(&id);
            self.drawn.remove(&id);
            self.add_ref(ctx, id)
        };
        info!("collision of 0x{id:08X} built again where a script moved it ({:.1} from its body){}", dist, if built { "" } else { ": none now" });
    }

    /// A created reference (dropped item): collision while it is present and the physics
    /// cell containing it is loaded, none otherwise; tracked in that cell so it unloads
    /// with it.
    fn sync_created(&mut self, ctx: &LoadCtx, id: u32) {
        let s = ctx.sim.world.peek(FormId(id)).filter(|s| dropped::present(s));
        let cell = s.and_then(|s| dropped::phys_cell(ctx.d.index, self.worldspace, self.space, s)).filter(|c| self.cells.contains_key(c));
        match cell {
            None => {
                if self.world.remove_ref(id) {
                    debug!("collision of {id:08X} removed");
                }
                self.created.remove(&id);
                self.move_created(id, None);
            }
            Some(c) => {
                self.move_created(id, Some(c));
                if !self.has_collision(id) {
                    self.add_created(ctx, id);
                }
            }
        }
    }

    /// Track a created reference in loaded physics cell `to` (or in none).
    fn move_created(&mut self, id: u32, to: Option<PhysCell>) {
        let from = self.ref_cell.get(&id).copied();
        if from == to {
            return;
        }
        if let Some(c) = from.and_then(|f| self.cells.get_mut(&f)) {
            c.refs.retain(|&r| r != id);
        }
        self.ref_cell.remove(&id);
        if let Some(c) = to.and_then(|t| self.cells.get_mut(&t).map(|c| (t, c))) {
            c.1.refs.push(id);
            self.ref_cell.insert(id, c.0);
        }
    }

    /// Add a created reference's collision: its base form's model at the simulation's
    /// pose, scale 1. Returns whether it now has collision.
    pub fn add_created(&mut self, ctx: &LoadCtx, id: u32) -> bool {
        let Some(s) = ctx.sim.world.peek(FormId(id)).filter(|s| dropped::present(s)) else { return false };
        let counts = &mut self.stats.counts;
        let Some(path) = dropped::model_path(ctx.d, s.base, s.extra.mods) else {
            *counts.entry("created_no_model").or_default() += 1;
            return false;
        };
        let Some(data) = ctx.data else {
            *counts.entry("awaiting_collision").or_default() += 1;
            return false;
        };
        let mut times = LoadTimes::default();
        let read = self.models.objects(data, &path, &mut times);
        self.stats.times.add(&times);
        let objects = match read {
            Ok(o) if !o.is_empty() => o,
            _ => {
                *self.stats.counts.entry("created_without_collision").or_default() += 1;
                return false;
            }
        };
        let pose = RefPose { form_id: id, position: s.pos, rotation: ref_rotation(s.rot), scale: 1.0 };
        // A panic converting or adding the bodies (`nvf_assets::guard`): no collision for
        // this reference, and nothing of it left in the world.
        let rep = match nvf_assets::guard::guarded(|| Ok(self.world.add_ref(&pose, &objects))) {
            Ok(rep) => rep,
            Err(e) => {
                warn!("collision of dropped 0x{id:08X} not added ({e}): {path}");
                self.world.remove_ref(id);
                *self.stats.counts.entry("created_without_collision").or_default() += 1;
                return false;
            }
        };
        if rep.bodies == 0 {
            *self.stats.counts.entry("object_errors").or_default() += rep.errors.len();
            return false;
        }
        *self.stats.counts.entry("created_with_collision").or_default() += 1;
        let local = (objects.len() == 1 && rep.bodies == 1).then(|| objects[0].node_transform.compose(&objects[0].body_transform));
        let dynamic = objects.iter().any(|o| o.motion == Motion::Dynamic);
        info!(
            "collision of dropped 0x{id:08X} added: {path}, {} bodies ({}), at ({:.1}, {:.1}, {:.1})",
            rep.bodies,
            if dynamic { "dynamic" } else { "not dynamic" },
            s.pos[0],
            s.pos[1],
            s.pos[2]
        );
        let added_step = self.world.steps();
        self.created.insert(id, CreatedBody { local, dynamic, start: s.pos, added_step, settled: false });
        true
    }

    /// Write the poses of moving dynamic created references back to the simulation (and,
    /// outdoors, the grid cell they are now in), so a cell reload, a snapshot or a resume
    /// finds them where physics put them. Logs each one coming to rest (its body asleep).
    /// Returns the references whose pose changed.
    pub fn settle_created(&mut self, index: &WorldIndex, world_id: u32, sim: &mut World) -> Vec<u32> {
        let mut moved = Vec::new();
        let mut lost = Vec::new();
        let elapsed = |since: u64| (self.world.steps() - since) as f32 * self.world.params.dt;
        for (&id, cb) in self.created.iter_mut() {
            let Some(local) = cb.local.filter(|_| cb.dynamic) else { continue };
            let Some(b) = self.world.ref_bodies(id).first().copied() else { continue };
            let (pos, rot) = dropped::ref_pose_from_body(&b, &local, 1.0);
            if pos[2] < cb.start[2] - CREATED_FELL_THROUGH {
                lost.push(id);
                continue;
            }
            let Ok(s) = sim.get(FormId(id)) else { continue };
            let dist = (0..3).map(|i| (pos[i] - s.pos[i]).powi(2)).sum::<f32>().sqrt();
            if dist > 0.01 || max_diff(&ref_rotation(s.rot), &rot) > 1e-4 {
                s.pos = pos;
                s.rot = ref_angles(&rot);
                // Outdoors the reference is in the grid cell containing it (as an actor
                // is after a teleport into a worldspace [observed, simulation.md §5]).
                let world = s.cell.and_then(|c| index.cells.get(&c.0)).and_then(|c| c.world);
                if let Some(&cell) = world.and_then(|w| index.worldspaces.get(&w)).and_then(|w| w.grid.get(&cell_of(pos[0], pos[1]))) {
                    s.cell = Some(FormId(cell));
                }
                moved.push(id);
            }
            if b.sleeping && !cb.settled {
                cb.settled = true;
                info!(
                    "dropped 0x{id:08X} came to rest at ({:.1}, {:.1}, {:.2}), {:.1} below where its collision was added, {:.2} s after",
                    pos[0],
                    pos[1],
                    pos[2],
                    cb.start[2] - pos[2],
                    elapsed(cb.added_step)
                );
            } else if !b.sleeping {
                cb.settled = false;
            }
        }
        for id in lost {
            let start = self.created.remove(&id).map(|c| c.start);
            self.world.remove_ref(id);
            if let (Ok(s), Some(start)) = (sim.get(FormId(id)), start) {
                s.pos = start;
            }
            warn!("dropped 0x{id:08X} fell through the world: collision removed, put back where it was dropped");
        }
        // Outdoors a reference that rolled into another grid square moves to that
        // square's physics cell (when it is loaded), so it unloads with it.
        if self.space == Space::Exterior {
            for &id in &moved {
                let to = sim.peek(FormId(id)).and_then(|p| dropped::phys_cell(index, world_id, self.space, p));
                if to.is_some_and(|c| self.cells.contains_key(&c)) {
                    self.move_created(id, to);
                }
            }
        }
        moved
    }

    /// Write the poses of placed dynamic references that physics has moved back to the
    /// simulation (see the module notes); logs each one's first move. Returns the
    /// references whose pose changed.
    pub fn settle_placed(&mut self, sim: &mut World) -> Vec<u32> {
        let mut changed = Vec::new();
        for (&id, p) in self.placed_dynamic.iter_mut() {
            let Some(b) = self.world.ref_bodies(id).first().copied() else { continue };
            if b.sleeping && !p.moved {
                continue;
            }
            let (pos, rot) = dropped::ref_pose_from_body(&b, &p.local, p.scale);
            if !p.moved {
                let dist = (0..3).map(|i| (pos[i] - p.added.0[i]).powi(2)).sum::<f32>().sqrt();
                if dist < MOVED_DISTANCE && max_diff(&rot, &p.added.1) < MOVED_ROTATION {
                    continue;
                }
                p.moved = true;
                debug!("placed {id:08X} moved by physics: at ({:.1}, {:.1}, {:.1}), {dist:.1} from where it was added", pos[0], pos[1], pos[2]);
            }
            if b.sleeping != p.rested {
                p.rested = b.sleeping;
                if b.sleeping {
                    let from = (0..3).map(|i| (pos[i] - p.added.0[i]).powi(2)).sum::<f32>().sqrt();
                    debug!("placed {id:08X} came to rest at ({:.1}, {:.1}, {:.1}), {from:.1} from where it was added", pos[0], pos[1], pos[2]);
                }
            }
            let Ok(s) = sim.get(FormId(id)) else { continue };
            let dist = (0..3).map(|i| (pos[i] - s.pos[i]).powi(2)).sum::<f32>().sqrt();
            if dist > 0.01 || max_diff(&ref_rotation(s.rot), &rot) > 1e-4 {
                s.pos = pos;
                s.rot = ref_angles(&rot);
                changed.push(id);
            }
        }
        changed
    }

    /// Unload everything and switch to `space`.
    /// Give actor `id` a character standing on what is under `feet` (a ray from a little
    /// above them, as actors are placed under furniture and roofs). False when the
    /// collision around `feet` is not loaded or nothing is under them. The character
    /// passes through actor hit volumes (its own included).
    pub fn add_npc(&mut self, id: u32, feet: [f32; 3]) -> bool {
        if self.npcs.contains_key(&id) {
            return true;
        }
        if !self.ready_at(feet) {
            return false;
        }
        let Some(hit) = self.ground_below(feet, 48.0, 400.0) else { return false };
        let params = npc_params(self.actor_heights.get(&id).copied());
        let start = [feet[0], feet[1], hit.point[2] + params.rest_gap(hit.shell)];
        let mut ch = self.world.add_character(id, params, start);
        ch.ignore_layers = fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes());
        let mut b = NpcBody { prev_feet: ch.feet, ch, velocity: [0.0, 0.0], jump: false, sink: 0.0, lift: Default::default() };
        b.sink = drawn_sink(&b.ch, self.drawn_ground(&b.ch));
        self.npcs.insert(id, b);
        true
    }

    pub fn remove_npc(&mut self, id: u32) {
        if let Some(b) = self.npcs.remove(&id) {
            self.world.remove_character(b.ch);
        }
    }

    /// Where actor `id`, standing at `feet` without a character, is drawn. Feet where a
    /// character rests (the contact shell above the collision under them, within
    /// [`REST_TOLERANCE`]: where a walk by physics, the arrival rule or a placement on
    /// the ground left them) are drawn as a character's would be, on the drawn ground
    /// ([`PhysState::ground_drawn`]); others (a placed height, the navmesh's height) at
    /// `feet`. Looked up again only when the feet move.
    pub fn standing_drawn_feet(&mut self, id: u32, feet: [f32; 3]) -> [f32; 3] {
        let sink = match self.standing.get(&id) {
            Some((at, sink)) if *at == feet => *sink,
            _ => {
                let down = 1.0 + fnv_phys::TERRAIN_SHELL + REST_TOLERANCE;
                let sink = match self.ground_under([feet[0], feet[1], feet[2] + 1.0], down, None) {
                    Some(g) if (feet[2] - g.bare - g.shell).abs() <= REST_TOLERANCE => feet[2] - g.drawn,
                    _ => 0.0,
                };
                self.standing.insert(id, (feet, sink));
                sink
            }
        };
        [feet[0], feet[1], feet[2] - sink]
    }

    pub fn reset(&mut self, space: Space) {
        for id in self.npcs.keys().copied().collect::<Vec<_>>() {
            self.remove_npc(id);
        }
        self.standing.clear();
        self.loader.clear();
        let all: Vec<PhysCell> = self.cells.keys().copied().collect();
        for c in all {
            self.unload(c);
        }
        self.space = space;
    }

    /// Unload what is out of range, request the missing cells around `feet` from the
    /// loader (nearest first), load the cell under `feet` now if it is not in, and insert
    /// up to `budget` prepared cells, in request order.
    pub fn stream(&mut self, ctx: &LoadCtx, feet: [f32; 3], budget: usize) {
        let mut known: Vec<PhysCell> = self.cells.keys().copied().collect();
        known.extend(self.loader.pending());
        let plan = cells::plan(self.space, &known, feet);
        for c in plan.unload {
            self.unload(c);
        }
        let own = self.cell_at(feet);
        for c in plan.load {
            if c != own {
                let work = self.work(ctx, c);
                if !self.loader.request(c, work) {
                    self.load(ctx, c);
                }
            }
        }
        if !self.cells.contains_key(&own) {
            // The start, or the loader fell behind the player: never let the player stand
            // where there is no collision yet.
            if self.player.is_some() {
                self.stats.loaded_inline += 1;
                info!("physics cell {own:?} under the player was not ready: loading it now");
            }
            self.load(ctx, own);
        }
        self.loader.poll();
        for _ in 0..budget {
            let Some(p) = self.loader.pop_ready() else { break };
            self.insert(ctx, p);
        }
    }

    /// Wait for the loader to finish every requested cell and insert them all (tests).
    #[cfg(test)]
    pub(crate) fn finish_loading(&mut self, ctx: &LoadCtx) {
        self.loader.wait_all();
        while let Some(p) = self.loader.pop_ready() {
            self.insert(ctx, p);
        }
    }

    /// Put the player's character (created on first use) with its feet on whatever is
    /// under `feet`, searching from `up` above to `down` below, on that support's contact
    /// shell (`CharacterParams::rest_gap`, where the controller keeps it and where
    /// `fnvtool phys walk` starts), or at `feet` when nothing is there. Returns the hit.
    pub fn place_player(&mut self, feet: [f32; 3], up: f32, down: f32) -> Option<RayHit> {
        let hit = self.ground_below(feet, up, down);
        let params = self.player.as_ref().map_or_else(CharacterParams::default, |c| c.params);
        let at = hit.as_ref().map_or(feet, |h| [feet[0], feet[1], h.point[2] + params.rest_gap(h.shell)]);
        match self.player.as_mut() {
            Some(c) => self.world.place_character(c, at),
            None => self.player = Some(self.world.add_character(0, CharacterParams::default(), at)),
        }
        self.stands_placed(hit.is_some());
        self.prev_feet = at;
        self.stepper = Stepper::default();
        hit
    }

    /// A player put on something stands on it: it counts as on the ground until its next
    /// physics step says otherwise. Placed characters start in the air (`fnv_phys`), and
    /// the steps wait behind a loading screen, dialogue or menus: the body played the
    /// jump loop meanwhile and a landing after it, as if the player had dropped from a
    /// height on arrival (playtest 2026-10-12, item 10).
    fn stands_placed(&mut self, on_support: bool) {
        if let Some(c) = self.player.as_mut().filter(|_| on_support) {
            c.grounded = true;
            c.ground_gap = Some(0.0);
        }
    }

    /// Put the player's character (created on first use) where an actor teleported to
    /// `feet` comes to rest: `fnv_phys`'s arrival rule, the one `fnvtool sim run` applies
    /// (physics.md §6.2).
    pub fn arrive_player(&mut self, feet: [f32; 3]) -> Arrival {
        let params = self.player.as_ref().map_or_else(CharacterParams::default, |c| c.params);
        let mut filter = Filter { exclude_layers: fnv_phys::layers::character_ignores(), ..Default::default() };
        if let Some(c) = &self.player {
            filter.exclude.push(c.key());
        }
        let a = self.world.arrival(feet, &params, &filter);
        match self.player.as_mut() {
            Some(c) => self.world.place_character(c, a.feet),
            None => self.player = Some(self.world.add_character(0, params, a.feet)),
        }
        self.stands_placed(a.support.is_some());
        self.prev_feet = a.feet;
        self.stepper = Stepper::default();
        a
    }
}

/// WRLD PNAM bit 2, "Use Map Data" [documented: xEdit, schemas/fnv/world.json]: the map
/// is not physics' concern.
const PNAM_USE_MAP_DATA: u16 = 1 << 2;

/// The water area of a physics cell.
fn water_area(cell: PhysCell) -> WaterArea {
    match cell {
        PhysCell::Exterior(x, y) => WaterArea::Exterior(x, y),
        PhysCell::Interior(_) => WaterArea::Interior,
    }
}

/// The physics streaming systems (Physics set); the player's systems run after them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysSystems;

pub struct PhysPlugin;

impl Plugin for PhysPlugin {
    fn build(&self, app: &mut App) {
        if let Some(t) = trace::PhysTrace::from_env() {
            app.insert_resource(t);
        }
        app.init_resource::<PhysState>()
            .init_resource::<FrameTimes>()
            .add_systems(Startup, init_phys)
            .add_systems(Last, (frame_times, report).chain())
            .add_message::<crate::RefRepose>()
            .add_systems(Update, (travel, ref_changed, stream, pose_doors).chain().in_set(PhysSystems).in_set(crate::PlaySet::Physics))
            .add_systems(PostUpdate, trace::trace);
    }
}

fn init_phys(d: Res<PlayWorld>, mut phys: ResMut<PhysState>) {
    phys.set_world(&d, d.world);
    phys.space = match d.interior {
        Some(id) => Space::Interior(id),
        None => Space::Exterior,
    };
}

/// Door travel: drop every cell, switch worldspace if the destination lies in another,
/// load the destination's own cell now and put the player there by the arrival rule (the
/// rest streams in over the next frames).
#[allow(clippy::too_many_arguments)]
fn travel(
    mut msgs: MessageReader<Travel>,
    d: Res<PlayWorld>,
    sim: Res<SimState>,
    data: Option<Res<GameData>>,
    mut phys: ResMut<PhysState>,
    mut player: ResMut<Player>,
    mut location: ResMut<Location>,
) {
    let Some(t) = msgs.read().last().copied() else { return };
    let to = t.destination(*location);
    let space = match to.interior {
        Some(id) => Space::Interior(id),
        None => Space::Exterior,
    };
    phys.reset(space);
    if to.world != phys.worldspace {
        info!("physics: worldspace {:08X} -> {:08X} ({})", phys.worldspace, to.world, world_name(&d, to.world));
        phys.set_world(&d, to.world);
    }
    *location = to;
    let ctx = LoadCtx { d: &d, sim: &sim, data: data.as_deref() };
    let cell = phys.cell_at(t.feet);
    phys.load(&ctx, cell);
    let hit = phys.arrive_player(t.feet).support;
    let feet = phys.player.as_ref().map_or(t.feet, |c| c.feet);
    player.feet = feet;
    player.placed = Some(feet);
    player.heading = t.heading_deg.to_radians();
    info!(
        "travel to {}: feet ({:.0}, {:.0}, {:.2}), floor {}, terrain {}",
        to.interior.map_or_else(|| format!("worldspace {:08X} ({})", to.world, world_name(&d, to.world)), |c| format!("interior {c:08X}")),
        feet[0],
        feet[1],
        feet[2],
        hit.map_or("not found".to_string(), |h| format!("z {:.2} (shell {:.2})", h.point[2], h.shell)),
        phys.terrain_z(feet[0], feet[1]).map_or("-".to_string(), |z| format!("z {z:.2}"))
    );
}

/// A worldspace's Editor ID for logs.
fn world_name(d: &PlayData, world: u32) -> &str {
    d.index.worldspaces.get(&world).and_then(|w| w.edid.as_deref()).unwrap_or("?")
}

/// References whose collision should follow the simulation: those named by
/// `RefChanged`, and the weapons actors drop when they die, are disarmed or break one
/// (`crate::combat::WeaponBroken`, whose reference would otherwise get no collision and
/// hang where it was placed until its cell loads again).
fn ref_changed(
    mut msgs: MessageReader<RefChanged>,
    mut repose: MessageReader<crate::RefRepose>,
    (broken, mut broken_seen): (Option<Res<Messages<crate::combat::WeaponBroken>>>, Local<bevy::ecs::message::MessageCursor<crate::combat::WeaponBroken>>),
    d: Res<PlayWorld>,
    sim: Res<SimState>,
    data: Option<Res<GameData>>,
    mut phys: ResMut<PhysState>,
) {
    let mut ids: Vec<u32> = msgs.read().map(|m| m.form_id).collect();
    if let Some(broken) = broken.as_deref() {
        ids.extend(broken_seen.read(broken).filter_map(|b| b.dropped).map(|r| r.0));
    }
    let moved: BTreeSet<u32> = repose.read().map(|m| m.form_id).collect();
    if ids.is_empty() && moved.is_empty() {
        return;
    }
    let ctx = LoadCtx { d: &d, sim: &sim, data: data.as_deref() };
    for id in ids {
        phys.sync_ref(&ctx, id);
    }
    for id in moved {
        phys.repose_ref(&ctx, id);
    }
}

/// The settle write-back for dropped items ([`PhysState::settle_created`]) after the
/// frame's fixed steps (in the player's chain, after its step).
pub(crate) fn settle_created(d: Res<PlayWorld>, mut sim: ResMut<SimState>, mut phys: ResMut<PhysState>) {
    phys.moved_now = if phys.placed_dynamic.is_empty() { Vec::new() } else { phys.settle_placed(&mut sim.world) };
    if phys.created.is_empty() {
        return;
    }
    let world = phys.worldspace;
    phys.settle_created(d.index, world, &mut sim.world);
}

/// Keep the cells around the player loaded (around the start position until the
/// character is placed).
fn stream(
    d: Res<PlayWorld>,
    sim: Res<SimState>,
    data: Option<Res<GameData>>,
    player: Res<Player>,
    loading: Option<Res<crate::loading::Loading>>,
    mut phys: ResMut<PhysState>,
) {
    let t0 = Instant::now();
    let playing = phys.player.is_some();
    let feet = phys.player.as_ref().map_or(player.feet, |c| c.feet);
    let ctx = LoadCtx { d: &d, sim: &sim, data: data.as_deref() };
    // Behind a loading screen a long frame does not show: take more prepared cells.
    let budget = if loading.is_some_and(|l| l.unseen_frames()) { CELLS_BEHIND_LOADING_SCREEN } else { CELLS_PER_FRAME };
    phys.stream(&ctx, feet, budget);
    // The broad phase takes in what was inserted now rather than in the player's first
    // step, so that the frame's streaming cost is measured in one place.
    let t = Instant::now();
    phys.world.update_queries();
    phys.stats.times.queries += ms_since(t);
    let ms = ms_since(t0);
    if playing && ms > phys.stats.worst_frame_ms {
        phys.stats.worst_frame_ms = ms;
        debug!("physics streaming took {ms:.1} ms this frame on the main thread (the longest so far)");
    }
}

/// Frame times while playing, for the exit report.
#[derive(Resource, Default)]
struct FrameTimes {
    frames: u32,
    worst_ms: f64,
    over_33: u32,
}

fn frame_times(time: Res<Time>, phys: Res<PhysState>, mut f: ResMut<FrameTimes>) {
    // From the frame after the player is placed: startup loading is not walking.
    if phys.player.is_none() {
        return;
    }
    let ms = time.delta_secs_f64() * 1000.0;
    f.frames += 1;
    if f.frames > 1 {
        f.worst_ms = f.worst_ms.max(ms);
        f.over_33 += (ms > 33.3) as u32;
    }
}

fn report(mut exit: MessageReader<AppExit>, phys: Res<PhysState>, f: Res<FrameTimes>) {
    if exit.read().last().is_none() {
        return;
    }
    let refresh = phys.world.refresh_stats();
    let mut reasons: Vec<_> = refresh.by_reasons.iter().collect();
    reasons.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    let reasons: Vec<_> = reasons.into_iter().map(|(reason, count)| format!("{reason} {count}")).collect();
    info!(
        "physics refresh summary: {} full refreshes; full by reasons: {}; {} physics steps",
        refresh.full,
        if reasons.is_empty() { "none".to_string() } else { reasons.join(", ") },
        phys.world.steps()
    );
    let s = &phys.stats;
    info!(
        "physics streaming: {} cells loaded in {:.0} ms ({}), {} on the main thread for the player; longest streaming frame while playing {:.1} ms on the main thread; frames while playing {} (longest {:.1} ms, {} over 33 ms)",
        s.cells_loaded, s.load_ms, s.times, s.loaded_inline, s.worst_frame_ms, f.frames, f.worst_ms, f.over_33
    );
    if let Some(c) = phys.player.as_ref() {
        // Ledges climbed by the controller's own step-up (`fnv_phys` `step_up_move`).
        info!("physics: the player stepped up {} times, feet at ({:.0}, {:.0}, {:.0})", c.stepped, c.feet[0], c.feet[1], c.feet[2]);
        // Steps and bumps the drawn body and view climbed smoothly (`step_lift`).
        let l = &phys.player_lift;
        info!("physics: the player's drawn height took up {} steps and bumps, lifted {:.1} units at most", l.taken_up, l.most);
    }
    // Loose items in water (`fnv_phys::buoyancy`): floating at the surface or sunk.
    let wet = phys.world.loose_in_water();
    if !wet.is_empty() {
        let list: Vec<String> = wet.iter().map(|(id, floats, top)| format!("{id:08X} ({}, its top {top:.0} units from the surface)", if *floats { "floats" } else { "sinks" })).collect();
        info!("physics: {} loose item bodies in water at exit: {}", wet.len(), list.join(", "));
    }
    // Placed bodies physics has moved (pushed, shot, knocked over), as they are now.
    for (&id, p) in phys.placed_dynamic.iter().filter(|(_, p)| p.moved) {
        if let Some(b) = phys.world.ref_bodies(id).first() {
            let (pos, _) = dropped::ref_pose_from_body(b, &p.local, p.scale);
            let d = (0..3).map(|i| (pos[i] - p.added.0[i]).powi(2)).sum::<f32>().sqrt();
            info!("physics: placed {id:08X} moved by physics: {d:.1} units from where it was added, at exit");
        }
    }
}

/// Move the keyframed bodies of the loaded doors to their animation's pose
/// (`crate::doors`), not solid while the door swings, registering doors the renderer has
/// not drawn yet.
fn pose_doors(sim: Res<SimState>, mut doors: ResMut<crate::doors::Doors>, mut phys: ResMut<PhysState>) {
    let phys = &mut *phys;
    for (&id, d) in phys.doors.iter_mut() {
        if doors.get(id).is_none() {
            let Some(timing) = d.timing() else { continue };
            doors.register(id, crate::doors::initial_open(Some(&sim), id, d.open_by_default), timing);
        }
        let Some(door) = doors.get(id) else { continue };
        let (open, time) = door.motion.pose();
        d.pose(&mut phys.world, open, time);
        // The moving parts are not solid while the door swings (`DoorMotion::swinging`).
        let solid = !door.motion.swinging();
        if d.solid != Some(solid) {
            phys.world.set_ref_keyframed_solid(id, solid);
            d.solid = Some(solid);
        }
    }
}
