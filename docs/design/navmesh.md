# Navmesh design: the nav graph, paths and travelling NPCs

Owner request 2026-10-09: navmesh building for worldspaces and AI, tested with an NPC that
navigates the world; the AI must be able to tell that it can reach where its AI package
sends it, even through several interior cells and worldspaces. Facts and tags:
docs/spec-notes/navmesh.md. This is the navigation half of Layer 6 (AI); packages
themselves (PACK records, schedules, conditions) are not part of it.

Status 2026-10-09: **done**: the NAVM/NAVI reader, the load order's nav graph,
hierarchical paths across spaces with locks, path following with stuck recovery,
`fnvtool nav` (survey, graph, near, path, walk), travelling NPCs in `nvforever`
(`--npc-travel`, `--follow-npc`, `--show-navmesh`), and **doors that open in place**
(§6.1: drawn and colliding with their Open/Close animation, toggled by activation, and
opened by followers, who wait for the animation to end). **Next**: AI packages driving
trips (§6.2), physics for travelling NPCs in the game (§6.3).

## 1. Summary and assumptions

- Navmeshes ship in the plugins (NAVM, 6,129 in the masters). NVForever reads them; it
  does not generate navmesh. "Building" is assembling them into one graph per load order,
  which takes 0.33 s for all masters, so it is done at start, not cached.
- Headless first (spec section 7): `fnv-nav` has no Bevy dependency; `fnvtool nav`
  tests it against the game data; the engine consumes it.
- How the original game chooses and follows paths is not documented. Costs, clearances,
  radii, speeds and the stuck rules below are engine choices, marked as such in the code,
  to be replaced by captured behaviour (spec-notes §7).

## 2. Crate `fnv-nav`

Depends on `fnv-esm` and `fnv-world` only.

| Module | Role |
|---|---|
| `record` | NAVM and NAVI from subrecord bytes (schema v1 cannot express NVGD's counted arrays or NVMI's union, so this reads bytes directly; the layouts are xEdit's) |
| `load` | every NAVM/NAVI of the load order, last definition winning (`NavRecords`) |
| `graph` | `NavGraph`: global triangle array, edges resolved (internal, portal, ledge, outline), outline vertices, a 512-unit bucket index per space, doors (XTEL), door locks (XLOC), walkable areas, the doors leaving each area; point location (`locate`, `nearest`), portals between triangles, `set_enabled` (`EnableNavMesh`/`DisableNavmesh`), `set_door_lock` |
| `doors` | doors that open in place (DOOR references without XTEL): where they stand on the navmesh (`in_place_doors`, `set_in_place_doors`), their footprint, opening and stand-off; locked ones close their triangles |
| `path` | `find_path`, `door_route`, `can_reach`; A*, funnel, shortcut pass, `walkable_line`; door stops on legs (`door_stops`, `square_approach`) |
| `follow` | `Follower` (where to head along a path; `Steer::OpenDoor`), `Unstuck` (detours, replans), `Walker` (moves an actor on the navmesh surface without physics; `update_with` waits at doors) |

## 3. Paths

1. **Snap** start and goal onto the navmesh of their spaces (within 512 units; the snap
   distances are returned so a caller can treat a far snap as "not really there").
2. **Within one walkable area**: A* over triangles. Costs run between the middles of the
   portals (shrunk by the clearance at outline ends), so each triangle has one entry point
   and the search is a plain graph search. Preferred triangles cost 0.85 per unit, water
   3, others 1 (engine choice). Edges between two outline vertices narrower than the
   clearance are closed. Options close ledges (default), "No Large Creatures" triangles
   (for large creatures) and triangles steeper than a mover can climb (`max_slope_deg`).
3. **Across areas** (other spaces, or islands of the same space): the **door route**
   first, a Dijkstra over doors between walkable areas, costed by straight lines between
   doors plus a door cost (256 units). This is the "can it get there?" answer
   (`door_route`, `can_reach`): from Doc Mitchell's house to the Strip it finds
   house → Mojave → two sewer interiors → Mojave → Freeside North → Freeside → Strip in
   11 ms. Then A* per leg; a leg that cannot be walked takes its door out of the route and
   the route is planned again (up to 16 times). Plain A* over every space at once flooded
   the whole Mojave (575k triangles) before giving up, because no distance estimate
   exists between spaces; the door route avoids that.
4. **Locked doors** are closed unless the actor holds the key (`keys`) or may open locks
   (`open_locked`). The authored XLOC is the starting state. The game's own lock state
   reaches the graph through `fnv_ai::Ai::apply_door_locks` (`NavGraph::sync_door_locks`),
   whenever `fnv_sim::World::lock_serial` moved: a script's Lock or Unlock, a package's
   lock/unlock flags, a key used, a lock picked. A door unlocked lets the packages set
   aside as out of reach be chosen again at once; a door locked sends the actors whose
   way goes through it to plan again (2026-10-23; before, nothing updated the graph).
5. **Funnel** (string pulling) per leg over the corridor's portals, oriented by the
   counter-clockwise winding (spec-notes §2), with each outline end moved in by the
   clearance (24 units; the physics capsule is 22): corners round walls become short
   arcs at the clearance.
6. **Shortcut pass**: drop corners where a straight line to a later corner (up to 32 ahead)
   stays on passable triangles and keeps 3/4 of the clearance from impassable edges
   (`walkable_line`, a navmesh raycast). It straightens what a midpoint corridor bends.
   The raycast follows one surface: it reaches the later corner only on a triangle whose
   height there is within `LEVEL_GAP` (40 units) of the corner's, so a line never joins
   a floor to a catwalk stacked over it (spec-notes/navmesh.md §4b).

7. **Door stops** (§6.1): each door that opens in place whose bounds the leg's polyline
   enters gets a stop. The polyline is made to cross the door square through the middle
   of its opening, from the stop to a point beyond it.

A path is a list of legs, one per space visited, each with its polyline, its corridor of
triangles, its door stops and the door that ends it.

## 4. Following

- `Follower` advances past corners within 16 units (or once passed while close), uses a
  door from within 48 units of the leg's end (in front of the door, where the frame can
  keep a body from the exact point), and arrives within `goal_radius` (1 unit by default;
  next to an actor target: two capsule radii more).
- `Unstuck`, for actors moved by physics: when the path's remaining length has dropped by
  less than a fifth of the speed over a second, detour 70° left, then right, then 110°
  each way (0.6 s each); detours count as having helped only after half a second's walk
  of real progress past where the actor got stuck; then replan; give up after 5 replans.
  The game's residents measure progress by how far the feet moved along the way to go
  (`Unstuck::advise_moved`), not by the path's remaining length, which stays the same
  for a path planned again and again after a moving target (Follow packages replan every
  half second; owner's playtest 2026-10-17: a Novac pack brahmin following its merchant
  counted as stuck all the way and detoured into the motel gate's fence).
  They skip instead of replanning
  (`Unstuck::skip`; owner request 2026-10-17: an NPC stuck navigating is put on the next
  part of the navmesh it was heading for): `Follower::skip_ahead` moves the actor 160
  units on along its path (past corners, never past a door stop it has not opened or the
  end of its leg; the point is on the path, so on the navmesh), and physics puts it down
  there again; the log says "actor … stuck at …: moved on along its path to …" and the
  residents' summary counts "moved N stuck actors on". Stuck again where it was put down,
  before a second's progress, it is moved on again after a second, without the four
  detours that already failed (`Unstuck::after_skip`): a stretch blocked for longer than
  one skip cost 7.4 s per 160 units otherwise (Boone just past his room's door, smoke
  follower-boone, 2026-10-17: 16.7 s to reach the player, against 9.5 s with the walker
  fallback before the skip).
- `Walker` moves an actor along the path on the navmesh surface (height from the
  triangle under it), turning at up to 180°/s (`WALK_TURN_RATE`; 720°/s before
  2026-10-11) and slowing while its heading is off the way (`turn_slowdown`; actors.md
  §8.9), through doors by teleport to the arrival.
  Standing with its way more than 20° off its heading it first turns on the spot at
  120°/s (`WalkEvent::Turned`; docs/spec-notes/actors.md §8.9); `Walker::face` turns a
  standing walker to a heading the same way.
- **Feet on another floor** (#1500) [design]. Distances, directions, the path's remaining
  length, skipping, progress, facing, speed and the distance travelled are horizontal; but a
  corner (by its radius or once passed), a door stop, a ledge's take-off or landing, the door
  that ends a leg and the end of the path count as reached only with the feet at the path's
  height (`Follower::steer`; also the first asking of a door, `door_ahead`, and the snap
  to a take-off). A door stop already held stays held, however far the door pushes the
  actor. Within reach seen from above but not at that height, the follower consumes
  nothing and says `Steer::OffPath`; beyond a corner it does not fit, it heads back to it.
  - *The tolerance* is 40 units (`HEIGHT_TOLERANCE`), against the walking segment at the
    feet's place seen from above (projection clamped; a segment with no horizontal length
    says nothing of a slope, and the point itself counts), never against the corner alone,
    and with no radius-times-slope allowance, which could admit another floor. An ordinary
    corner also fits by its outgoing segment, so walking down a slope past it is not
    rejected. A ledge's flight is never interpolated. 40 is an engineering number, not
    measured safe: step height 24, terrain shell 3.5 and a margin for the capsule's shape
    and for mesh against collision, and the same as `LEVEL_GAP`. **Its limits:** nothing
    bounds how far an authored navmesh is from its collision; floors stacked closer than 40
    are not told apart; on a long straight segment the line's height is not the surface's
    (shortcuts keep only the end's height, §3), so judging happens only near what is being
    reached, where they meet; past a corner with a turning outgoing segment on a steep
    slope, an actor is sent back to the corner.
  - *Recovery.* `Walker` first puts feet it did not put there onto the navmesh under them
    (its next step would); if they then fit it walks on, so an actor placed off the mesh
    is as before. Otherwise `WalkEvent::PathLost`, standing where it is, the path kept. It
    is not an arrival. `fnv_ai::Ai::walk` plans from the feet to the path's destination
    with `path_params` (`Walker::replan`, `AiEvent::PathReplanned`). A failed plan goes the
    way a walk with no way there goes (`AiEvent::PathLost`, then the package set aside; a
    sandbox's own walk chooses again; a fight's, search's, flight's, conversation's,
    fetch's or weapon use's walk sees the actor no longer on its way). `PathRecovery`
    allows 3 plans until the actor has moved 256 units or 30 s have passed, so an actor no
    plan helps fails rather than plan on every frame. A new path's first segment starts
    at the surface under the feet, so it fits them. Its start more than 64 units over the
    feet (`REPLAN_BELOW_LIMIT`) is a floor they cannot get back onto: no way. Feet further
    than the tolerance from the only surface near show no other floor (on something the
    navmesh does not know, a navmesh above its collision): the new path allows for that
    (`height_offset`), and arrival there is what it was before.
  - *Physics* (`residents.rs`, `PhysMover`): the outcome comes before the last stretch,
    turning and skipping; the body stops on its surface and the AI plans. The last
    stretch to the walker (furniture's final approach) needs the feet to fit too. In the
    air nothing is called a fall until it lands, and a swimmer's height is not judged
    (`steer_any_height`: its feet are at the water's surface).
    `fnvtool nav walk` does the same, logging `replanned_off_path` events.
  - *Left out, as separate policy:* a package's own idea of being there is not this
    tolerance: `Ai::within` accepts a package's place within 256 units of height
    (`carry.rs`) and can end a walk through it; Follow packages stop within 256 and set off
    again by horizontal distance; combat spacing and formation order stay horizontal.
    The walker's rule that a step takes the height of the navmesh near it (128 units) is
    unchanged, so an actor left to the walker with no floor under it but one over it still
    goes up to that one, as before. None of this was measured against the original.
    A headless walk on the navmesh has its feet at the surface, so nothing changes; the one
    exception is a ledge whose ends are within a corner's radius seen from above and more
    than 40 apart in height: its flight, skipped before, is flown.
- At a door stop `Follower` returns `Steer::OpenDoor { door, at }` and keeps returning it,
  wherever the actor is pushed meanwhile, until the caller calls `door_opened` (§6.1).
  `Walker::update_with(nav, dt, open)` asks `open(door)` every update until it reports
  the door fully open, standing meanwhile (`WalkEvent::WaitingForDoor`); `update` passes
  doors as if open.

## 5. Tests and tools

- Unit tests (`crates/fnv-nav/src/tests.rs`, synthetic navmeshes): internal edges and
  outline, straight paths, a wall with clearance, a seam between navmeshes whose grids
  differ, unreachable goals and opt-in ledges, doors between spaces with a walker going
  through, **a chain of interiors into another worldspace behind a locked door** (key,
  runtime unlock), a disabled navmesh, a walker on a slope; funnel, follower, `Unstuck`
  and turning tests. Doors that open in place: a stop in front of the door and the
  follower held there until `door_opened` (also when pushed off the stop), a walker
  standing while a 1.67 s door opens and not waiting at an open one, a locked door
  closing a corridor (key, `open_locked`, runtime unlock), a path beside a door not
  stopping and a locked one sending it round, a diagonal approach squared up to the
  middle of the doorway, an actor starting at the door backing off to open it.
- `fnvtool nav walk` (headless NPC test): an NPC-sized fnv-phys character walks the path
  in the collision world of the cells along each leg. On the owner's install
  (2026-10-09), all arrived:

  | Trip | Doors | Path | Walked | Time | Detours |
  |---|---|---|---|---|---|
  | EasyPeteRef → TrudyRef (outside → saloon) | 1 | 1,145 | 1,014 | 11.7 s | 0 |
  | GSChetRef → SunnyRef | 2 | 2,899 | 2,781 | 35.1 s | 3 |
  | DocMitchellREF → GSChetRef (house → town → store) | 2 | 7,226 | 7,101 | 88.7 s | 4 |
  | SunnyRef → DocMitchellREF | 2 | 8,532 | 8,390 | 101.0 s | 1 |
  | DocMitchellREF → Strip gate 0x0013BB0D (`--keys 0x00119875 --speed 344`) | 7 | 147,625 | 147,825 | 447.9 s | 11 |

  With doors that open in place (§6.1), re-run 2026-10-09; all arrived:

  | Trip | Path | Walked | Time | Detours | Doors opened (wait) |
  |---|---|---|---|---|---|
  | EasyPeteRef → TrudyRef | 1,145 | 1,015 | 11.7 s | 0 | none |
  | GSChetRef → SunnyRef | 2,899 | 2,746 | 33.9 s | 2 | none |
  | DocMitchellREF → GSChetRef | 7,239 | 7,030 | 82.3 s | 1 | gate 0x0010757E (Open by Default: 0 s) |
  | SunnyRef → DocMitchellREF | 8,542 | 8,388 | 100.7 s | 1 | gate 0x0010757E (0 s) |
  | DocMitchellREF → 0x0013BB0D (`--keys 0x00119875 --speed 344`) | 147,684 | 147,498 | 445.7 s | 8 | gate 0x0010757E (0 s), sewer door 0x0012C97C (1.68 s) |
  | TrudyRef → "-193,600,3456" in GSProspectorSaloonInterior | 968 | 921 | 11.8 s | 0 | saloon door 0x00107C77 (1.02 s) |

  Walk-test rules (engine choices for the test world): the walker's own body and other
  placed actors' bodies are left out (in the game they are characters that move and give
  way; `--with-actors` keeps them); doors that open in place whose model has Open and
  Close stay in the world, posed by their animation, and are opened on the way (§6.1),
  while the others are left out unless locked against the walker; and paths keep off
  triangles steeper than the capsule's 45° limit (`--any-slope` lifts that). What got each of these rules added is
  in spec-notes §2 and §4: the walker spawning inside its own body, a creature placed in
  a sewer tunnel, a hinged sewer door, 46–49° Mojave slopes.
- Against the original game: `fnvtool nav observed` on the AI observer captures
  (spec-notes §4a): 99.7% of recorded NPC positions are within 32 units of our navmesh,
  every recorded walk can be planned, and recorded walks stay a median 17.6 units from
  our planned paths.
- In the game: `nvforever --npc-travel ACTOR=TARGET` (repeatable; target a reference or
  "x,y,z"), `--npc-speed` (86.8 by default), `--follow-npc` (the free camera chases the
  first traveller and the renderer follows it through doors), `--show-navmesh` (outline
  red, cell seams cyan, ledges orange, preferred green, water blue, door triangles yellow,
  paths magenta). A traveller's root is moved by the walker, and its `Locomotion`
  (`engine/nvforever/src/locomotion.rs`, workstream 5 part G) gets the velocity it
  actually moved at and its turn rate, so the actor's own clip set (walk, turns, idle)
  is chosen and blended as for any actor; cells do not spawn travellers while they
  travel. A traveller at a door that opens in place opens it through
  `nvf_play::doors::Doors` (the drawn door and its collision follow) and stands until it
  is fully open; a door not drawn (its cell not loaded) is passed at once. A traveller in
  the interior being drawn starts once that cell is in. Door check:
  `nvforever --data <Data> --cell GSProspectorSaloonInterior --npc-travel
  "TrudyRef=-193,600,3456" --follow-npc --run-seconds 25` (expect "TrudyRef opens door
  00107C77 and waits", "walks through door 00107C77 after waiting 1.02s" and "1 of 1
  travellers arrived"). Smoke test:
  `nvforever --data <Data> --cell Goodsprings --npc-travel DocMitchellREF=GSChetRef
  --npc-travel SunnyRef=EasyPeteRef --follow-npc --run-seconds 105` (expect "nav summary:
  2 of 2 travellers arrived"; Doc arrives after 83–90 s depending on frame rate).

## 6. Next

### 6.1 Animated in-cell doors (owner request 2026-10-09) — done

Doors that are not cell transitions must animate open and closed. What the data holds
[observed: `UtlDoor01` 0x0003940E, `Dungeons\Utility\Rooms\UtlDoor01.NIF`]: the model has
an NiControllerManager with two sequences, **"Open" and "Close"** (1.67 s each), whose
text keys name the sounds (`sound: DRSMetalUtilitySmallOpen` / `...Close`); five
collision bodies on the AnimStatic layer (keyframed, so collision is meant to follow the
animated nodes); BSXFlags 11. The DOOR record has SNAM/ANAM sounds and FNAM flags ("Sliding
Door" set here). Work: play the sequences on the drawn door (the renderer already plays
Idle sequences on animated references), move the door's keyframed physics bodies with
the animated node poses (fnv-phys), keep open/closed in the simulation and open with
activation (nvf-play), and let followers open a door their path crosses (the walk test
then stops leaving such doors out).

Followers and doors (owner decision 2026-10-09): **an NPC waits until the open animation
has finished before walking through.** The graph learns where hinged doors stand on the
navmesh (DOOR references without XTEL, by their bounds against the corridor); a leg whose
corridor crosses one gets a stop in front of it. There `Follower` returns a new step,
"open door X", the actor stops, the door's "Open" sequence starts, and the follower holds
until the sequence ends (its `end` text key: 1.67 s for `UtlDoor01`; the door's state in
the simulation says open), then walks on. A door already open is walked through; a
locked one is closed to actors without the key, as teleport doors are. Open questions:
whether doors close by themselves behind actors, and whether NPCs open doors from a
distance or must touch them.

**As built** (facts: docs/spec-notes/doors.md; 263 of 474 DOOR models have Open and
Close, standing at 1,844 references without XTEL):

- `fnv_collide::anim`: a model's node hierarchy and sequences (`ModelAnim`), node poses
  in the root's frame under a sequence (`node_to_root`), and `DoorMotion`, the clock of
  one door (closed, opening, open, closing; finished at the `end` text key; told to turn
  back mid-way it continues from the same fraction of the other sequence [engine
  choice]).
- Renderer (nvforever `doors.rs`, world.rs): a DOOR reference without XTEL whose model
  has Open and Close is drawn with its node hierarchy (`Rig::door`) and posed from its
  clock whenever it changes; teleport doors stay static. `--open-doors` (test aid) opens
  every such door as it is drawn.
- Physics (`fnv_phys::PhysWorld::pose_ref_object`; nvf-play `phys.rs` `pose_doors`): the
  door's keyframed bodies are moved to `ref ∘ animated node ∘ body transform` every frame
  the clock changes, and are not solid while the door swings
  (`PhysWorld::set_ref_keyframed_solid`, `DoorMotion::swinging`; owner request
  2026-10-09), so a swinging leaf cannot hit or push an actor; at rest they are solid.
  **Later (not built):** stay non-solid after the sequence ends until no character
  overlaps the moving parts at rest, so a door closing on someone in the doorway does not
  trap them; logic and tests to write in docs/spec-notes/doors.md §4. An Open by Default door keeps its moving parts (posed open) through
  `RefCollisionOptions::posed_doors`; the shared rule `ref_collision` used by the parity
  lab is unchanged.
- Simulation and activation (`nvf_play::doors`): `Doors` holds every door's clock,
  follows the simulation's open flag (activation with E, `SetOpenState`), writes back
  doors opened by actors, and logs the sequences' sound keys (no sound playback yet).
  fnv-sim starts a door flagged Open by Default open [hypothesis].
- Navigation (`fnv_nav::doors`): 1,043 such doors stand on the masters' navmesh. A
  door's stop is in line with the middle of its opening (the navmesh's walkable stretch
  along the door's line within its bounds), the opening's width plus the clearance in
  front of it, and the path then goes straight through to a point a clearance beyond:
  a hinged leaf swung open lies along that line at the hinge end, half the opening's
  width to the side, whichever way it opens; where the navmesh is shallower the stop
  comes closer (down to the clearance and 24 units), and where that cannot be walked the
  stop goes on the polyline. All engine choices: the first try (a stop 48 units before
  the door on the polyline) let a door in the Prospector Saloon swing into the waiting
  actor and leave its open leaf across the diagonal approach, and a stop centred on the
  bounds missed the sewer doorway, whose bounds are wider than its opening.

### 6.2 AI packages

Decode PACK (types, locations, targets, schedules, conditions), evaluate the current
package per actor, and send Travel/Sandbox locations to `door_route` / `find_path`. Keys
come from the actor's inventory (fnv-sim); a package location the actor cannot reach
should fall to the next package, as `can_reach` allows checking cheaply.
Done headless (2026-10-10): the records, the reader, the choice and the walk to the
package's location (`fnv-ai`); see docs/design/actor-behaviour.md §5, and §5.4 for the
engine.

### 6.3 Travelling NPCs with physics in the game

(Done for residents, 2026-10-10: docs/design/actor-behaviour.md §5.4.) In `--play`, travellers near the player should move by the character controller
(`Follower` + `Unstuck`, as in `fnvtool nav walk`) where the physics world is loaded, and
by `Walker` elsewhere (the game's high and low processing levels are the model;
[hypothesis] on how they divide).

## 7. Decisions for the owner

1. NPC walking speed 86.8 units/s (the player's observed walk; the walk clip's own stride
   gives 85.3) until captured.
2. Ledge links unused by default (humans do not take drops of ~300 units).
3. Locked doors closed to actors without the key; the authored lock is the starting
   state (as fnv-sim assumes).
4. In the walk test, other actors are left out of the collision world (§5) until actor
   avoidance exists; doors that open in place are kept and opened when they can animate,
   left out otherwise.
5. Doors that open in place (§6.1): an actor opens one from a stop in line with the
   middle of the opening, the opening's width plus its clearance in front of it, and
   waits there until the Open sequence has ended; doors stay open (they do not close by
   themselves); a door told to close while opening turns back; a swinging door's moving
   parts are not solid (owner request). How the original game does
   each of these is not known.
