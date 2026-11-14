# Layer 4 in the game: `engine/nvf-play` (workstream 4)

Status 2026-10-08: skeleton and parts P, A and U merged on `claude/layer4-play`
(each part built in its own worktree and branch by one coordinating session); dropped
items drawn, colliding, falling and taken back since `claude/layer4-play-dropped`
(`dropped.rs`, §4 Part U). Exit
criterion: M4 (docs/design/layer4.md §4.6) — the player walks the Mojave with collision,
opens doors and containers and takes items. Integration checks (§6) pass unattended; real
keyboard and mouse play has not been tried by a person yet.

## 1. What already exists

- `crates/fnv-sim`: `World` — references as a delta over the plugins, inventories,
  activation (`World::activate` → `Outcome`), game clock, merchant restocks, snapshots.
  Headless, no Bevy.
- `crates/fnv-phys`: `PhysWorld` — terrain per grid cell (`add_terrain`/`remove_terrain`),
  references' collision (`add_ref`/`remove_ref`, objects from `fnv_collide::nif`), ray and
  shape queries whose hits name their owner (`Owner::Ref { form_id, .. }`), a kinematic
  capsule controller (`add_character`, `move_character`), fixed-step `Stepper`. Headless.
  Controller values are placeholders (`CharacterParams::default`, physics.md §6).
- `engine/nvforever`: the M3 explorer — cell streaming around the `Camera3d`
  (`world::stream`), terrain, sky, LOD, free-fly camera.

## 2. Shape

`nvf-play` is a library crate in the engine workspace with one `PlayPlugin`, added by
`nvforever` when it runs with `--play`. Without `--play` the binary is unchanged (the
fly camera, screenshots and the lab's frame captures keep working).

- Game space is the game's (Z up, game units); Bevy space is Y up. Conversions:
  `nvf_play::space`. The sim and physics never see Bevy types.
- The player is the `Camera3d` entity (marked `PlayerCamera`): physics moves its feet,
  the camera sits at eye height. Render streaming follows that camera.
- **The main view** [design: issue #1753, `engine/nvforever/src/fly.rs`]: the main world
  camera is the one `setup` spawns (main.rs), marked `FlyCam` with or without `--play`
  (`PlayerCamera` joins it with `--play`); the marker stays on that one entity for the
  whole run (free camera, death camera, VATS and third person move it, none replaces it).
  It is not the only `Camera3d`: the water reflection, a scope's lens and reticle, and the
  menus that draw a model of their own (lockpicking, terminals, the Vigor Tester,
  blackjack, poker, roulette) spawn 3D cameras beside it. So a system that means "where
  the player looks from" takes the camera by the filter `fly::WorldCamera`, never as the
  only `Camera3d` (`.single()` stopped while a menu camera existed, `.iter().next()`
  could take the menu's): streaming and tree billboards (world.rs), level of detail
  (lod.rs, treelod.rs, imposters.rs), grass, particles' births and squares, effect
  billboards (shot_fx.rs), water, distant water, ripples, the reflection's mirror,
  underwater fog, the navmesh overlay, actors' feet, heads, lips and turning. Systems
  that mean every camera keep their own queries: particle culling tests each active 3D
  camera's frustum (particles.rs), per-view shadow and pointer code looks a camera up by
  its entity. World systems therefore keep running behind a menu's camera; world.rs
  `note_other_cameras` logs it (`world: streaming runs with another 3D camera active
  (<name>)`, smoke scenario `menu-camera-world-runs`).
- Physics streams its own cells around the player (radius 1, i.e. 3×3 exterior cells, or
  the current interior); render streaming (radius 2+) is unchanged and independent.
- The simulation (`SimState`) owns the truth about references (taken, disabled, open,
  inventories). Render entities carry `RefVisual(form_id)` so the game can hide what the
  simulation removed.
- Door travel: activation sends `Travel`; `nvforever` reloads render cells (interior or
  exterior, sky lighting, LOD) and physics rebuilds its cells and places the player.
  The worldspace is a run-time value (`Location`): a door into another worldspace
  switches physics (terrain, persistent references, water height) and rendering
  (`nvforever/src/worldspace.rs`: cells, sky and weather from the new climate, image
  space, distant LOD and generated terrain, distant water) to it.

## 3. Interfaces (in the skeleton; changes go through the coordinator)

Coordinator for the dropped-items change (2026-10-08, branch `claude/layer4-play-dropped`):
`RefVisual` now also marks render entities of references the simulation created,
`RenderCell` is new (the only change in `engine/nvforever`: one component on each streamed
cell entity), and `RefChanged` is sent by the inventory menu for each dropped reference.

| Item | Kind | Written by | Read by |
|---|---|---|---|
| `PlayData`, `PlayWorld` | plugin input / resource: `'static` load order, schema, `WorldIndex`, the worldspace the game starts in, start pose, test aids | nvforever | all |
| `Location` | resource: the worldspace and the interior (if any) the player is in now; from `PlayData` at the start, changed when physics applies a `Travel` | P | U (F10 aid), any part that needs the current worldspace |
| `SimState` | resource: `fnv_sim::world::World<'static>` + forms + refs; clock advanced in real time | sim.rs | A, U |
| `PlayState` | Bevy state `Playing` / `Menu` / `Loading` (behind a loading screen) | U (enter/leave menus) | P (no movement/look in menus), A |
| `PlaySet` | system sets, chained in `Update`: `Input` → `Physics` → `Activation` → `Ui` → `Sync` | lib.rs | all |
| `PhysState` | resource: `PhysWorld`, `Stepper`, the player's `Character` | P | A (ray queries; remove taken refs) |
| `PlayerCamera` | component on the `Camera3d` | nvforever | P, A |
| `Player` | resource: look angles, eye height, feet (game space) | P | A |
| `RefVisual(u32)` | component on every render entity spawned for a reference: placed ones (nvforever), created ones (dropped items, `dropped.rs`) | nvforever, `dropped.rs` | A (visual sync, render-bounds targeting) |
| `RenderCell` | component on each render cell entity nvforever streams in (`Exterior { world, x, y }` / `Interior(id)`); its transform is the cell origin | nvforever | `dropped.rs` (spawns created references as the cell's children, so they go with it) |
| `Travel { world, interior, feet, heading_deg }` | message: into an interior (`interior: Some`, `world: None`) or out into worldspace `world` (the current one or another); `Travel::destination(from)` gives the new `Location` | `loading.rs` (from a `TravelRequest`); nvforever's `--follow-npc` camera without `--play` | nvforever (render, worldspace switch), P |
| `TravelRequest(Travel)` | message (since 2026-10-10, docs/design/loading-screen.md): travel behind a loading screen. `loading.rs` brings the screen up (`PlayState::Loading`, virtual clock paused), sends the `Travel` two frames later and takes the screen down when physics and the renderer have loaded the destination | A, scripts | `loading.rs` |
| `OpenContainer { container }` | message | A | U |
| `RefChanged { form_id }` | message: the sim changed a reference's presence (taken, dropped, enabled, disabled), placed or created | A, U (one per dropped reference) | A (visual sync), P (collision; created references load with their physics cell) |

Coordinator change 2026-10-09 (player body, branch `claude/player-body`,
docs/design/player-body.md): `Player` gained `body_eye` (the eye of the drawn body, which
`Player::eye` returns when set); new resources `PlayerControl` (sneak, weapon out, aiming,
first/third person), `ControlSettings`, `BodyView`, and the set `player::BodySet` in
`PlaySet::Physics` between the character's move and the camera; the camera system moved
to `control.rs` (`place_camera`: first person at the eye, third person behind the
shoulder). Sneaking scales walking and running by `fMoveSneakMult`. Same day, second
change: every control is read through `bindings::Bindings` (the game's default keys and
mouse buttons, docs/spec-notes/controls.md), including Activate and Menu Mode;
`activate::HudMessage` is public; the nearest-container aid moved from F9 (the game's
Quick Load) to F10.

Coordinator change 2026-10-08 (worldspace travel, branch `claude/layer4-play-worldspace`):
`Travel` gained `world`; `Location` was added; `PlayData::world` is only the start.
`PhysState::worldspace` / `PhysState::set_world` hold physics' worldspace, and the sim's
player reference follows it (`World::worldspace_of`: a reference's worldspace is its
cell's, so position, cell and worldspace change together and snapshots keep them).
`outcome::effects` no longer takes the current worldspace: an exterior destination always
names its worldspace and the receivers compare. Each worldspace's own records are used;
taking data from a parent worldspace by WRLD PNAM is not implemented (no shipped child
worldspace asks for it: world.md §6).

## 4. Parts

### Part P — physics streaming and the player (`phys.rs`, `player.rs`)
- Load collision for the cells around the player: terrain heights, references' collision
  objects read from the archives through `nvf_assets::GameData` and
  `fnv_collide::nif::collision_objects` (cache per model path), CollisionMarker
  primitives, the worldspace's persistent references in those grid squares; skip
  disabled/deleted references and those the simulation has removed. Unload with
  hysteresis. Interiors: the one cell. Same selection rules as `fnvtool phys cell`
  (crates/fnvtool/src/phys_cmd.rs `build`) — share code where sensible, do not diverge.
- Player: `add_character` at the start (drop to the ground under the start position),
  WASD (run by default, Shift walks [hypothesis: the game's default]), mouse look with the
  cursor grabbed, Space jumps, fixed-step via `Stepper`; the camera at eye height
  [hypothesis value, documented]. `--auto-walk <speed>` test aid walks forward.
  `Travel`: rebuild physics for the destination and place the character. Keep the
  simulation's player reference position and cell in sync (`SimState`).
- Accept: unit tests for pure parts (input → velocity, cell selection); in game,
  `nvforever --data <Data> --play --cell Goodsprings --auto-walk 300 --run-seconds 40`
  exits 0, the player stays on the ground (log feet z vs terrain), walks into buildings'
  walls instead of through them.

### Part A — activation (`activate.rs`, visual sync)
- Crosshair and a prompt naming what is under it (base form name; "Open", "Take",
  "Talk", locked level); ray from the eye along the view (in third person the camera's line, the crosshair's,
  `control::Crosshair`, #259), reach [hypothesis value,
  documented], filter out the player's capsule. E activates: `World::activate(player,
  target)`; apply the `Outcome`: `PickedUp` → `RefChanged` (hide visuals, remove
  collision), `Teleported` → `Travel`, `OpenedContainer` → `OpenContainer`,
  `DoorToggled`/`Locked`/`Talk`/`NoDefault`/`Disabled` → on-screen message.
- Visual sync: on `RefChanged`, hide/show the `RefVisual` entities of that reference from
  the simulation's state (deleted, enabled). Also apply the simulation's state when
  cells stream in (a taken item must not reappear when its cell reloads).
- Items have no collision in many cases: if the ray misses, also test references' render
  bounds near the ray (document the rule as hypothesis).
- Accept: unit tests for outcome → effects; in game, Doc Mitchell's house
  (`--cell GSDocMitchellHouse --play`) take a stimpak and leave by the front door.

### Part U — container and inventory menus (`menu.rs`)
- `OpenContainer` opens a two-column menu (container / player) in `PlayState::Menu`:
  item names, counts, weight and value, take one / take all / store, close (Esc or E).
  Tab opens the player's inventory alone: equip/unequip (`World::equip`), drop
  (`World::drop_item` → `RefChanged` for each new reference), encumbrance from
  `World::encumbrance`. Non-playable items (ARMO BMDT "Non-Playable", e.g. the Pip-Boy
  Glove) are not listed in either menu, so they cannot be stored, equipped or dropped
  [observed by the owner: the game never shows the glove and it stays equipped]; Take
  All leaves them in the container [hypothesis]. Quest items (`World::is_quest_item`)
  are listed with a "Q" mark; dropping one is refused by the simulation and the status
  line says "<name> is a quest item and can't be dropped" [documented: GECK wiki
  Category:Items; the wording is ours]; storing one in a container is allowed
  [hypothesis] (2026-10-09, simulation.md §3 "Quest items").
- Since 2026-10-11 the container menu is drawn as the original's, from the game's
  `menus\container_menu.xml` (player's list at the left, categories, the item's picture
  and figures), and the barter menu has been added beside it (`ShowBarterMenu`, prices by
  the Barter skill, caps): `menu/look.rs`, `menu/barter.rs`,
  docs/spec-notes/barter-menu.md. The inventory menu keeps the plain look above.
- Dropped items (`dropped.rs` and `phys.rs`; added 2026-10-08): created
  references are drawn from their base form's model at the simulation's pose as children
  of their `RenderCell` (so render streaming and door travel unload and reload them);
  taken ones are despawned; their model's collision loads with their physics cell, dynamic
  objects fall and settle (`Stepper`) and the settled pose is written back to the
  simulation (`PhysState::settle_created`). The crosshair finds them by collision or render
  bounds, and E takes them back through `World::activate`. A created reference belongs
  to its cell's worldspace: render cells (`RenderCell::Exterior { world, .. }`) and
  physics (`PhysState::worldspace`) only take the ones of the worldspace being played, so
  after a door into another worldspace they are neither drawn nor colliding, and they
  come back when the player returns. Whether the game's dropped
  items fall is a hypothesis with a capture written for the owner (simulation.md §5 "After
  the drop").
- The cursor is released in menus and grabbed again on close.
- Accept: unit tests on the menu model (rows from an inventory, transfers through the
  sim); in game, open Ringo's cash box (`GSGasStation`) or a container in Doc Mitchell's
  house and move items both ways.

## 5. Rules for every part

- CLAUDE.md applies (clean room, evidence tags, layer order, never edit goldens or
  scenarios to pass). Every game value used (eye height, reach, walk/run speeds, jump) is
  a documented `[hypothesis]` until captured; add it to docs/spec-notes/physics.md §6 or
  simulation.md as appropriate, never present it as observed.
- Own your files; touching another part's file or `engine/nvforever` goes through the
  coordinator. Small additive changes to `lib.rs` interfaces: ask first.
- Checks before handing back: `cargo test --release` and `cargo clippy --release
  --all-targets -- -D warnings` in `engine/`, root workspace tests if a root crate was
  touched, and the moving-camera smoke test from CLAUDE.md plus the part's own `--play`
  run.

## 6. Integration (2026-10-08)

Unattended checks on the merged branch (test aids: `--auto-walk`, `--pos/--heading/--pitch`,
`NVF_PLAY_TEST_ACTIVATE=<seconds,...>` presses E, `NVF_PLAY_TEST_MENU=<container|nearest>`
drives the menus — it opens the container from wherever the player is and does not move
them —, and since 2026-10-08 `NVF_PLAY_TEST_NEAR=<reference FormID>` starts the player on
a standing spot from which that reference can be activated, looking at it: on rings 60–100
units around it, grounded on something other than the reference, room for the capsule,
the eye within reach (150) of a point of it whose view ray hits its collision first
(60, 30, 90 or 0 units above its position), else nothing in between; with no such spot
the start is kept and a warning logged; the reference must be in the cell or worldspace
played; since 2026-10-08 also `NVF_PLAY_TEST_DROP=<s>:<item>[:stack]` drops through the
inventory menu, `NVF_PLAY_TEST_LOOK=<s>:<heading>:<pitch>`,
`NVF_PLAY_TEST_GOTO=<s>:<x>:<y>:<z>` and `NVF_PLAY_TEST_SHOTS=<s>:<png>` (nvf-play
`test_aid.rs`); with any `NVF_PLAY_TEST_*` set the cursor is not grabbed, so the desktop
mouse no longer turns the view in unattended runs):
- Goodsprings, `--auto-walk 300`: placed on the terrain (feet 8369.0 on 8368.0), walks,
  cells stream, a house blocks the way.
- Doc Mitchell's house: the stimpak 0x00105BF3 is targeted by its collision at 110 units,
  E takes it (`PickedUp`), its render entity hides and its collision goes.
- The front door 0x00103E61: E teleports to the worldspace; physics places the feet at
  8691 on a floor at 8690 (the door marker is at 8686.43; the in-game capture put the
  player at 8693.87 [observed, `goodsprings_doc_mitchell`]); render streaming reloads 25
  exterior cells. Since 2026-10-08 travel uses the arrival rule (physics.md §6.2): feet on
  the terrain's contact shell, 8693.53.
- Unattended, the two Doc Mitchell checks: `NVF_PLAY_TEST_ACTIVATE=3 nvforever --data <Data>
  --play --cell GSDocMitchellHouse --pos=2050,1954,7480 --heading 90 --pitch -34.4
  --run-seconds 6` (the stimpak, targeted by collision at 85) and the same with
  `--pos=2350,1536,7480 --heading 90 --pitch -10 --run-seconds 12` (the front door, by
  bounds at 70); `RUST_LOG=info,nvf_play::phys=debug` shows the collision removed and the
  travel line.
- Goodsprings gas station: the metal box 0x001629C1 is targeted at 99 units and E opens
  the container menu; the menu test aid takes all of Ringo's 200 caps and stores items back.
- Interior starts and falls (2026-10-08, physics.md §6.7): a start beside Ringo's cash box
  0x001629C0 (outside the gas station's room) now warns and moves to the arrival of door
  0x00109040 into the cell, feet (511, −5951, 7554.2) on the floor, instead of falling for
  ever; walking off the box (`--pos` on it, `--auto-walk 100`) is caught 800 units under
  the cell's lowest collision and put back at that arrival, then walks into the room. A
  door destination with nothing under it is moved the same way (Doc Mitchell's front door
  from outside still arrives normally: floor 7360, no warning). With
  `NVF_PLAY_TEST_NEAR=001629C1` and E at 6 s the metal box is targeted by collision at
  106 units and opens; `NVF_PLAY_TEST_MENU=001629C1` still opens it and runs its script;
  `NVF_PLAY_TEST_NEAR=001629C0` finds no spot (the box is outside the room) and keeps the
  start. Doc Mitchell's house with the new aid: stimpak 0x00105BF3 targeted at 75 and
  taken; front door 0x00103E61 targeted at 72, E arrives at 8693.53 on 8690.03 as before.
  Goodsprings `--auto-walk 300` is unchanged (feet 8371.5 on terrain 8368.0; the terrain
  net is untouched). One unattended run's target changed while the player stood still
  [hypothesis: real mouse motion over the focused window, which grabs the cursor without
  `--auto-walk`]; reruns were steady.
- Dropped items (2026-10-08, `--cell GSDocMitchellHouse --play --pos 2032,1954,7472
  --heading 90 --pitch -28`): E takes the stimpak 0x00105BF3 (collision, 103 units); the
  drop aid drops it from the inventory: reference 0xFF000000 at (2082.0, 1954.0, 7390.7),
  drawn (`stimpak01.nif`) and given its dynamic collision; when the menu closes it falls
  and comes to rest on its side on the floor at z 7360.97 (floor 7360) 1.58 s later;
  looking down (pitch −62) the crosshair finds it by its render bounds at 125 units, E
  takes it back (`PickedUp`, count 1), its render entity is despawned and the inventory
  shows Stimpak x1 again. Screenshots checked: on the shelf, upright in the air, on its
  side on the floor, gone.
- Dropped items across cell unload (same run style, `NVF_PLAY_TEST_GOTO` to the front
  door): one stimpak dropped inside, the player leaves through the front door (the inside
  drop is unloaded with the interior) and drops the second outside: 0xFF000001 at
  (−73115.2, 1327.2, 8723.6), within 0.3 units of the game's own drop point for this spot
  (simulation.md §5), drawn in exterior cell (−18, 0), at rest on the ground at 8692.71.
  Back in through the exterior door 0x00103E69, the inside stimpak is drawn and collides
  again where its pose was last written back (it had not quite settled when the player
  left) and settles 1.7 lower; the outside one is unloaded. Out again: the outside
  stimpak is drawn at its resting pose and does not move (screenshots before and after
  match).
- Dropped items use their world model (2026-10-10, `--cell Goodsprings --play --hour 12`,
  `NVF_PLAY_TEST_GIVE=00020423,0010EA7F,0008F215,00004324`, one drop every 4 s from 14 s
  with `NVF_PLAY_TEST_LOOK` turning a quarter turn before each; the startup message
  boxes swallow a drop before about 12 s): Leather Armor is drawn and collides as
  `armor\leatherarmor\m\go.nif` (the folded ground object, 2 dynamic bodies; before, the
  body-shaped `outfitm.nif`), the Dapper Gambler Hat as `gambler_fedora_01_go.nif`, the
  .44 Magnum Revolver as `44magnumrevolvernoscopego.nif` and Brass Knuckles, which have
  no MODL and were not drawn before, as `brassknucklesworldobject.nif`. Each falls to
  the road and is offered as "E) Take" (screenshots checked: folded armour, revolver,
  knuckles on the ground).
- Non-playable items hidden (2026-10-08, after merging main): Doc Mitchell's and the
  gas station's inventories list no Pip-Boy rows; the drop/retake and door round-trip
  runs repeat with the same resting poses; the gas station menu script, finding nothing
  to equip, drops a bottle cap, which is drawn (`nukacolacap.nif`) and falls 31 units to
  the floor.

Doors into another worldspace (2026-10-08). The ten masters have 106 such doors between
22 worldspaces, all reciprocal, 6 locked [observed: `fnvtool sim doors`, simulation.md
§5]. Test aids added: `NVF_PLAY_TEST_SHOTS=<seconds,...>` saves screenshots (since the
dropped-items merge also `<seconds>:<path>`; one `test_aid.rs` holds both branches' aids),
`NVF_PLAY_TEST_FACE=<seconds:FormID,...>` puts the player 100 units in front of a door
on its arrival side, facing it (a stand-in for walking up to it; `nvf-play/src/test_aid.rs`).
Unattended runs, each with screenshots before and after every travel, all exit 0:
- Mojave north gate 0x00116420 → FreesideNorthWorld and back through 0x00116424: physics
  and render both log the switch (WastelandNV ↔ FreesideNorthWorld; climate
  NVDefaultClimate ↔ NVUrbanClearClimate); feet 1003.50 on a floor at 1000.00 (shell
  3.50; terrain 1000.00) in Freeside, 4344.46 on 4343.76 back at the Mojave gate. The
  frames after travel match direct starts at the same poses.
- One run through four worldspaces: TheStripWorldNew gate 0x0013BB0D → FreesideWorld
  (feet 1033.23 on 1032.53), side door 0x00133953 → FreesideNorthWorld (1027.50 on
  1024.00), north gate 0x00116424 → WastelandNV (4344.46 on 4343.76); after three
  switches the Mojave frame matches a direct start.
- Atomic Wrangler (started inside with the default `--world WastelandNV`): the front door
  0x00116E2D leads out into FreesideWorld (switch logged; feet 1014.50 on 1011.00).
- Big MT ↔ Higgs Village (0x0300BF95 / 0x0300BEBB): both switches; Higgs uses
  NVDefaultClimate, Big MT NVDLC03BigMTClimate; generated distant terrain baked for Big MT.
- Unchanged: the Doc Mitchell front door still lands at 8693.53 on 8690.03 with no
  worldspace switch; `--auto-walk 300` in Goodsprings stays on the terrain (+3.5) until a
  house stops it; the moving-camera smoke test exits 0.
- Dropped items across worldspaces (after merging the dropped-items branch; new aid
  `NVF_PLAY_TEST_GIVE=<item>[:count]` adds items at the start): a stimpak dropped beside
  the Strip gate 0x0013BB0D rests at z 1010.33, is unloaded on the first switch and is
  neither drawn nor given collision in FreesideWorld, FreesideNorthWorld or WastelandNV
  along the four-worldspace chain. One dropped beside the Mojave north gate 0x00116420
  rests at 4350.78, is unloaded in FreesideNorthWorld and, back through 0x00116424, is
  drawn and collides again at its resting pose (does not move; screenshots before and
  after match).
- Not compared with the game: where the game puts the player after a cross-worldspace
  door. A capture for the owner (not run; other sessions use the game): from a save at the
  Mojave north gate, activate 0x00116420 by script, then log `GetPos x/y/z`,
  `GetParentCell`, `GetParentWorldspace` and the heading; the same back through
  0x00116424, and for the Strip side 0x0013BB0D. NVForever predicts FreesideNorthGate
  0x0012DA5C at (8288, 6896, 1003.50) heading 220.7, and FreesideExtNorthGate 0x000DABEA
  at (−9721.49, 111797.62, 4344.46) heading 35.0.

Physics cells off the main thread (2026-10-08). Loading a cell took up to ~50 ms, all in
one frame, so walking across cell borders stuttered. Measured first (debug log
`physics cell … loaded`, exit line `physics streaming: …`; `RUST_LOG=info,nvf_play=debug`):
reading models from the archives and parsing them was most of it, building rapier shapes
the next part, inserting bodies and the broad-phase update about a millisecond:

| Run | Before: per cell (median), longest streaming frame | After: preparing per cell (median), inserting, longest streaming frame |
|---|---|---|
| Goodsprings `--auto-walk 300`, 40 s, 12 cells | 4.0–53.4 ms (12.7), 54.1 ms; second run 4.0–104.4 (18.5), 105.5 ms | 4.4–66.6 ms (11.3) on the loader thread, ≤ 1.1 ms, 2.3 ms |
| `--pos -60000,-20000,20000 --heading 180 --auto-walk 1500`, 60 s, 30–33 cells | 2.8–27.9 ms (8.8), 29.0 ms; second run 26.9 ms | 2.6–47.4 ms (8.1), ≤ 1.4 ms, 3.1 ms |
| Doc Mitchell's house, E on the front door, 12 s | 45.3 ms | 4.4 ms (a loaded machine: every part took twice as long as before) |

Parts before, 12 Goodsprings cells: read 73 ms, parse 89, shapes 36, records 19, convert
15, insert 5, broad phase 9. "Longest streaming frame" is the main thread's physics
streaming work in one frame once the player is placed; the first cell at the start and a
door's destination still load on the spot (allowed). Whole frames: alternating runs gave
51 and 54 frames/s after against 49 and 45 before; the longest frame (250 ms, Bevy's
clamp) is render streaming in both. Now: `phys/prepare.rs` reads records and models and
builds the shapes (`fnv_phys::PreparedRef` / `PreparedTerrain`, the halves of
`add_ref` / `add_terrain`); `phys/loader.rs` runs that on its own thread (not Bevy's
pools, which the LOD bakes keep busy) and hands cells back in request order; the main
thread inserts one ready cell per frame, references by FormID, deciding then whether each
is in the world. The cell under the player is never waited for: if it is not in, it loads
on the spot and `info` logs it (none did in these runs: a new row of cells is requested
a whole cell ahead of the player). The model cache is shared between the threads. Shape
caching per model and prefetching beyond the 3×3 were not needed for the frame time.
Determinism: tests insert made-up cells with the loader finishing forwards and backwards,
on the main thread, and in two walking runs, and get bit-identical worlds
(`phys/tests.rs`, physics.md §5); parity-lab L4 results are unchanged. Re-checked: the
Goodsprings walk (feet 3.5 above the terrain, stopped by the house 0x00177CA4 where it
stopped before), the stimpak and the front door (feet 8693.53, shell 3.50), the
`--auto-fly` smoke test; after merging the worldspace doors, the same runs gave longest
streaming frames of 3.5 and 3.4 ms, and the Mojave north gate round trip lands as
recorded above (1003.50 in Freeside, 4344.46 back).

After merging dropped items, jump/trees and the swimming follow-up (2026-10-09), against
main's binary (481db8d) run back to back: Goodsprings walk, main 3.4–71.6 ms per cell
(13.9), each in one frame, against 3.2–61.4 (10.1) prepared on the loader thread, ≤ 1.2 ms
inserting, longest streaming frame 2.0 ms; the long walk, main 2.4–28.0 (6.0) against
2.2–30.4 (5.4), ≤ 4.9 ms inserting (one dense cell), longest streaming frame 5.9 ms.
Placed water comes out of the prepared path (`RefShapes::water`, added before the bodies
as `fnvtool phys` does) and dropped items load with their cell when it is inserted
(`add_created`, FormIDs above every placed one). Same outcomes as main for every check:
walk ends, stimpak, door 8693.53, gate 1003.50 / 4344.46, a stimpak dropped by the north
gate (rests 4350.47, unloaded in Freeside, back at 4350.46), CampGuardianCaves (floats at
6425.5, 102.5 below the placed surface 6528), Boulder Beach (swims at −102.5 from
(59500, 38500), walks out at (55599, 38457, 2634.1)); parity-lab 336/351, every result
identical to main's `fnvtool`.

Input audit (2026-10-09, `claude/l4-play-closeout`; reading the code against the
original's control map, plus the coordinator's first hands-on run with real OS input on
main 9be35d7). The original's bindings [observed: the owner's `FalloutPrefs.ini`
[Controls], read-only; each value's second byte is a DirectInput scan code]: Forward W,
Back S, Slide Left A, Slide Right D, Activate E, Crouch/Sneak left Ctrl, Run left Shift,
Always Run Caps Lock, Auto Move Q, Jump Space, Menu Mode (Pip-Boy) Tab, Ready Item R,
Grab Z, Toggle POV F, VATS V, Rest T, QuickSave F5, QuickLoad F9, Use left mouse, Block
right mouse; `bAlwaysRunByDefault=1`, `bInvertYValues=0`, `fMouseSensitivity=0.0020`.
Bevy's `KeyCode` names physical keys, as scan codes do, so layouts other than QWERTY
keep the positions. Changed in `nvf-play` (`player.rs` `keys`, `read_keys`):
- Caps Lock toggles Always Run (Shift then runs instead of walking); Ctrl toggles
  sneaking (the original's control toggles [observed: `IsSneaking` after `TapControl 8`,
  mv_values]), at 197.7/344 of the speed [observed running; walking hypothesis]; Q
  toggles Auto Move, cancelled by Forward or Back [hypothesis]. Before, none of the three
  existed.
- Esc in the world opened nothing and quit the program at once (nvforever `keys`). It now
  opens a pause menu (`pause.rs`: Resume / Quit, keys and clickable buttons; a
  `PlayState::Menu`, so time, physics and mouse look stop and the cursor is free); Esc in
  a menu still closes it. Without `--play` Esc still quits. `--run-seconds` and the test
  aids are unaffected. Since 2026-10-10 the pause menu holds everything, in both modes of
  `--menu-pause`: the picture too (Bevy's virtual clock is paused) and the voice of a
  line being said; the test aids' clock and `--run-seconds` go on in real time meanwhile
  (`pause.rs`: `AidTime`; test aid `NVF_PLAY_TEST_PAUSE=<open>:<resume>`).
- T (the game's Rest control) opens the Wait menu, which is a Sleep menu while the
  player lies on a bed (`wait.rs`, docs/spec-notes/wait-menu.md): 1 to 24 hours on a
  slider, then the hours pass one at a time on the game's calendar. Also a
  `PlayState::Menu`; test aid `NVF_PLAY_TEST_WAIT=<seconds>:<step>[,…]`.
- `--menu-pause living|original` (default `living`; docs/design/dialogue.md §8) says
  whether the picture stands still while a menu is open: the container and inventory
  menus of Part U, message boxes, the menus scripts open and the dialogue screen.
  `living` is what these menus always did (game logic held, animations and wind
  running); `original` pauses the virtual clock under them.
- **Settings page** (2026-10-10, owner request: "a settings menu on ESC pause menu … all
  our runtime params such as pausing time on dialogue as toggles"). The pause menu is
  Resume / Settings / Quit; Settings (S, or a click) lists the on/off choices of the
  command line that can change while the game runs, each showing the state the game is
  using. Up/Down (or W/S) choose a row, Enter, Space, Left, Right or a click flip it,
  Esc or Backspace go back. A flip acts at once and nothing is saved: the next start
  goes by the command line again. Not the original's settings menu: neither its entries
  nor its layout are copied [decision]. Since 2026-10-16 (playtest list 5, #290) the
  pause menu is Continue / Save / Load / Settings / Quit in the original's look
  (`start_menu.rs`, from `start_menu.xml`), Quit asks Main Menu / Exit Game / Cancel, and
  Settings groups the switches by category (Gameplay, World, Display, Graphics, Debug,
  Saving) as ON/OFF rows; the entries are still NVForever's own switches. Since #1953
  (2026-11-09) the page shows the rows in seven tabs, Gameplay, Display, Audio,
  Controls, Graphics, Tweaks and Debug, under headings, from a table that only says
  where a row is drawn (`settings_layout.rs`; the registry, its groups and its order are
  unchanged): [settings-page.md](settings-page.md).
  - A switch is an `nvf_play::settings::Setting`: a key, a label, a note, and a `get` and
    `set` over the state its module already uses (so there is no second copy to keep in
    step). The owning module registers it (`add_setting`): nvf-play its own in its
    plugins, nvforever the renderer's in `nvforever/src/settings.rs`.
  - The switches: `menu-pause` (the picture stands still in dialogue and menus,
    `--menu-pause`), `voice` (`--no-voice`), `dialogue-look` (`--no-dialogue-look`),
    `dialogue-recentre` (`--dialogue-recentre`), `dialogue-zoom` (`--no-dialogue-zoom`), `lip-sync` (`--no-lip`),
    `head-tracking` (`--no-head-tracking`), `sky-clock` (`--still-sky`),
    `first-person-body` (`--first-person-body`, player-body.md §1.5), `weapon-sway`
    (`--no-weapon-sway`, player-body.md §1.4a), `sun-shadows`
    (`--sun-shadows`), `grass` (`--no-grass`), `distant-lod` (`--no-lod`), `grade`
    (`--no-grade`), `overlay` (`--no-overlay`), `navmesh` (`--show-navmesh`),
    `smooth-terrain` (`--no-smooth-terrain`; #2149, on by default, takes effect at the next
    start: `nvf_play::smooth_terrain`, docs/spec-notes/world.md §3 "Smoothing").
  - What switching while the game runs needed: grass switched off is hidden and cells
    loaded meanwhile are owed theirs (`grass.rs` `GrassOwed`), grown a cell a frame once
    it is on again; head tracking off lets heads ease back, and actors get their
    `HeadLook` only while it is on; lip sync off eases a moving face to rest; the sun's
    shadow cascades are added when shadows come on (`sky::set_shadows`; a run started
    with `--shadow-distance 0` uses the default distance); distant LOD keeps its
    generated-terrain job with `--no-lod` so it can come on; `--no-grade` is a flag
    (`SkyState::grade`) and now turns off an interior cell's grade as well.
  - Left on the command line: what is fixed when the game starts (`--no-npc-packages`,
    `--no-actors`, FaceGen, `--no-lodgen`), the numbers (distances, `--msaa`, `--fov`,
    volumes) and the test aids.
  - Test aid `NVF_PLAY_TEST_SETTINGS=<seconds>:<key>[,…]`: with the pause menu open
    (`NVF_PLAY_TEST_PAUSE`), shows the settings page and flips that switch as a click
    would; the key `page` only shows the page. Check (2026-10-10, Goodsprings): a run
    started with everything off (`--no-grass --no-lod --shadow-distance 0 --no-grade
    --no-overlay --no-head-tracking --still-sky`) and switched on from the page ends
    with the same picture and counts as a default run (20,538 grass plants, 45 LOD
    chunks, 60 generated); a default run with every switch flipped the other way shows
    each change behind the open page.
- The status overlay's help line showed the fly camera's keys in `--play`; it now shows
  `nvf_play::HELP` (the play controls).
- Message-box buttons can be clicked (the original's can [observed by the coordinator]):
  each is a button (`script_hud.rs` `BoxButton`), and the cursor is released while a box
  is on screen.
- Menu errors name items in words ("Cannot equip or use 10mm Round") instead of
  "Cannot: 0x00004241 cannot be equipped" (`menu/model.rs` `plain_error`).
- Lock prompts name the level: Very Easy, Easy, Average, Hard, Very Hard, Requires Key
  for 0/25/50/75/100/255 [documented: GECK "Non-NPC population", level-design.md §7;
  255 = Requires Key hypothesis], other levels as numbers.
Kept, with the reason: mouse look stays 0.0025 rad per count (what the original's
`fMouseSensitivity` 0.002 measures is not known) and is not inverted (the original's
default); F9 stays the test aid that opens the nearest container (in the original it is
QuickLoad; there is no saving yet); the right Shift also walks (unbound in the original).
Not built (2026-10-09): Ready Item, Grab, Toggle POV, VATS, Rest/Wait, hotkeys, saving.
All built since except hotkeys 1 and 3–8 (2026-10-25): Ready Item and Toggle POV
(`nvf-play` `control.rs`), Grab (docs/spec-notes/grab.md), VATS mode (vats-mode.md),
Rest/Wait (docs/spec-notes/wait-menu.md), saving with F5/F9 (save-load.md).

Open:
- Real keys and mouse: first hands-on run by the coordinator 2026-10-09 (mouse look,
  WASD, jump, inventory and container menus by keys and mouse, doors, message boxes);
  its findings are fixed above. Crouching and Auto Move by hand are not tried yet.
- ~~Dialogue and barter are not built.~~ Built since: dialogue-play.md and
  docs/spec-notes/barter-menu.md.
- ~~Locked doors that scripts open in the game stay locked~~: with Layer 5 scripts in
  `--play` (docs/design/layer5-play.md) the Freeside → Strip gate 0x00116425's own
  GameMode unlocks it once `VFreeformFreeside.PlayerPassport` is set (the Securitron's
  dialogue, Layer 6; a test aid stands in), and its OnActivate takes the player through.
- Renderer, seen in the worldspace tests and the same with a direct start (not the
  switch): NVUrbanClear (Freeside) is overexposed and Big MT's sky is white at noon.
- ~~Starting the game in an interior leaves generated distant terrain off for the start
  worldspace after going out~~ Fixed (2026-10-09): main.rs builds the generated-terrain
  job for interior starts too; LOD stays off inside and starts with it at the door out.
- ~~Armour placed in the world or dropped is drawn with its base form's MODL~~ Fixed
  (2026-10-10): an item placed in the world or dropped is drawn with, and collides by,
  its record's world model, ARMO MOD2 (the `GO.NIF` ground object) or WEAP MOD4, and by
  MODL when the record names none (`BaseObject::world_model_path`; simulation.md §3
  "Item models"). (The first drop test dropped the Pip-Boy Glove, which the game never
  lets the player drop: since 2026-10-08 the menus hide non-playable items, see Part U.)
- The player's non-playable items (Pip-Boy 3000, Pip-Boy Glove) start unequipped in the
  simulation; in game the glove is always equipped [observed by the owner].
- ~~Whether dropped items fall in the game~~: they do [observed 2026-10-09, `pc_drop_fall`,
  physics.md §4.3]; a drop now starts turned to the player's heading and clutter rests on
  the terrain's 3.5 shell, as in the game. ~~Placed dynamic clutter that physics moves is
  not written back~~: since 2026-10-09 it is (`PhysState::settle_placed`, drawn there by
  `moved.rs`, collision rebuilt there on reload; physics.md §4.3). Whether the original
  keeps pushed clutter where it rests across a cell reload is not captured [hypothesis].
- ~~Controller jitter against some rocks reproduces in `fnvtool phys walk`~~ Fixed:
  concave corners stop the character instead of alternating between faces (physics.md
  §6.1); the controller now rests on contact shells like the arrival (§6.2).
- ~~Interior safety net: what the game does with `coc` and with a start over nothing~~:
  captured 2026-10-09 (`pc_coc`, physics.md §6.7): `coc` goes to the cell's
  COCMarkerHeading, else its XMarker, else a door arrival; a fall is caught within about
  a second and put there. `nvf-play` follows (interior starts without `--pos`, rescues,
  a 256-unit fall margin). Standing on top of a reference outside the room (Ringo's cash
  box) is still allowed until the player walks off it.
- Crouching: sneaking toggles and slows the player [observed], but the crouched capsule
  height and eye height are not captured (physics.md §6, capsule height).
- Captures that leave Doc Mitchell's house through the front door make the game write
  `autosave.fos` (pc_drop_fall, 2026-10-09: the save guard reported it and kept the
  previous rolling saves in `C:\fnv-private\capture\saves-backup\1791556049`); the
  movement jobs of layer4-captures.md §5 all do this. Owner's call whether to restore.
