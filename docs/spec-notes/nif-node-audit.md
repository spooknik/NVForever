# NIF node and extra-data audit (2026-10-20)

Every node name, extra-data name and value, animation text key and block type in the base
game's meshes, checked against what NVForever's engine reads. A name nothing reads is a
hint of an engine gap: the iron-sights work found `##SightingNode` in every gun and no
code that used it, and this audit looks for more like it. Tags as in CLAUDE.md.

**Sources.** `fnvtool nif nodes "Fallout - Meshes.bsa" Update.bsa --schema schemas/nif`
(new, docs/cli-contract.md): 19,277 `.nif`/`.kf` files, every block decoded (0 undecoded,
0 skipped) [observed]. No add-on archive was read. The JSON dumps stay in
`C:\fnv-private\nif-node-audit` (game-derived, not committed). "In ESM" counts files
whose path appears as a string in `FalloutNV.esm`. That is a lower bound: strings in
compressed records are missed, and `.kf` files are never named there (clips are found by
folder) [observed]. "NV" counts files outside the folders Fallout 3 left in the archive
(`dlc01`-`dlc05`, `dlcanch`, `dlcpitt`) and `effects\tests`. Many of those folders' files
are still used: `dlc03` paths appear 139 times in the ESM [observed].

**Method.** For each name I searched `engine/` and `crates/` (Rust, case-insensitive,
`git grep`), then read the code that matched. Each name is marked:

- **used**: code reads that name or type (file:line given);
- **generic**: a general rule covers it, e.g. any `##` part follows clips by name,
  interpolator data is read through the field that points to it, or a bone moves with
  its skin;
- **unused**: nothing reads it.

Where `docs/spec-notes/effects-animation-audit.md` §1 or `particles.md` §5 already lists a
gap, the table points there and does not repeat it.

## 1. Ranked gaps

Ranked by how likely the gap shows in play or looks, then by file count. Meanings are
[hypothesis] unless tagged otherwise.

| # | Name (kind) | Files (NV / in ESM) | Examples | Engine | What it may be |
|---|---|---|---|---|---|
| 1 | `SightingNode`, `SightingNOde` (node, no `##`) | 10 + 8 (18 / 18) | `weapons\2handrifle\cowboyrepeater*.nif`, `1handpistol\32snubnose.nif`, `44magnumrevolver*go.nif`, every `10mmpistol*heavyframe.nif` | **unused**. The iron-sights branch matches `##SightingNode` only (case-insensitive) | The same eye point as the 97 `##SightingNode` guns (same name without the prefix, guns of the same folders). Add `"SightingNode"` to the list and the 18 guns aim down their sights too |
| 2 | `Eject` (text key) | 87 `.kf` (84 / n.a.) | `characters\_male\2hrattack3.kf`, `2hrattack5/7/8/9`, `2hrattackright*`, their `is`/`sneak`/up/down forms; 12 `_1stperson` clips; Securitron `1hpattackleft*.kf` | **used** (2026-10-21): `ClipKeys::eject`; the body counts the attack clip passing it and combat throws the casing then (combat-rules.md §16) | The moment the casing leaves. Attack types 3/5/7/8/9 [hypothesis: lever, bolt and pump actions] eject when the action is worked, not at the shot |
| 3 | `BSDamageStage` (node type), `BSBlastNode`, `BSDebrisNode` | 49, 52, 6 (45, 42, 6 / 45, 42, 6) | `clutter\electricbox\electricbox1.nif`, `fusiongenerator\fusiongenerator01.nif`, `fireextinguisher\fireextinguisher01.nif`, `vehicles\vertibirdencount01.nif` | **unused**. DEST stages are read (`fnv-game/src/data.rs:621`) but `combat.rs:2356` logs "not drawn yet" | `DamageStage<n>` children are the model's look at destruction stage n (data.rs reads the stage number from DSTD). `BlastNode<n>` may mark where a stage's explosion is placed, and `DebrisNode<n>` its debris |
| 4 | `ProjectileNode_HeadLaser` (node) | 4 (4 / 3) | `creatures\eyebot\skeleton.nif`, `mistergutsy\skeleton.nif`, `robobrain\skeleton.nif`, `protectron\2hlheadlaser.nif` | **unused**. `MUZZLE_NODES` (`weapons.rs:339`) matches `ProjectileNode` and `##ProjectileNode` only | Where a robot's head laser fires from. Today these robots' shots leave from another point |
| 5 | `BSTreadTransfInterpolator`, `BSRotAccumTransfInterpolator` (block types) | 5, 21 `.kf` (5, 4 / n.a.) | `characters\_male\1hmidle.kf` and `_1stperson\1hmidle.kf` drive node `Ripper`; `_1stperson\2hmattackspin.kf` drives `##SawBlade`; `nvsecuritron\locomotion\mtforward.kf` drives `Bip01 Wheel` [observed] | The tread is **unused** (it becomes `Other` and nothing moves); the rotation that adds up is played since 2026-11-09 (animation.md §9.12) | The Ripper's moving chain (a tread), a blade that keeps spinning and a walking Securitron's wheel (rotation that adds up). The first two are visible in first person |
| 6 | `DecalPlacementVectorGroup<n>` / `DecalPlacementVector<n>` (nodes), `DVPG` (`BSDecalPlacementVectorExtraData`), `Decal: <n>` (text key) | 41 nodes, 45 DVPG, 127 `.kf` (40, 44, 127 / 41, 45, n.a.) | `weapons\1handmelee\leadpipe.nif`, `throwingknife.nif`; keys in `vats1hmattackpower*.kf`, `_1stperson\2hmattackright_a.kf` | **unused**. `player_clips.rs` has `Decal: 0` only in a test | Blood on melee weapons: the key names the group whose vectors place the decal on the weapon (group numbers 0-2 match the key values 0-2) [observed: values; meaning a hypothesis] |
| 7 | `BSOrderedNode` (type, all named `OrderedRenderingNode`) | 78 (22 / 28) | `architecture\hooverdam\nv_hooverdam_aagun.nif`, `dungeons\enclave\custom\enclavemainentrance01.nif`, `dungeons\vault\roomu\klaxon.nif` | **unused** (drawn as a plain node) | Its children are drawn in their stored order, not sorted by depth. Glows and smoke layers in these effects may sort wrongly |
| 8 | `AttachLight` (node) | 9 (5 / 5) | `clutter\barrel02firelight.nif`, `dungeons\vault\roomu\klaxonglow.nif` | **partly** (2026-10-23): a LIGH whose model's sequence drives its light hangs that light from the node holding it, `AttachLight` or below ([model-lights](model-lights.md)); other models' lights only give a colour (`model_light.rs`) | Where the model's light sits; with `NiLightColorController` (effects audit §1) it makes the klaxon's turning light |
| 9 | `AttachSound`, `SoundNode<x>` (nodes) | 17, 19 (12, 14 / 15, 16) | `architecture\freeside\nvfreesidestripdoor.nif`, `clutter\clfence\clfencegate01.nif`, `nv_hooverdam_aagun.nif` | **partly**. `anim_sounds.rs:72` reads the sound in `Sound: X SoundNodeSteam` and drops the node | Where a model's sounds play from (a door's sound at its hinge, not its origin) |
| 10 | `Bip01 Yaw`, `Bip01 Pitch` (bones) | 11, 7 (11, 7 / 6, 4) | `creatures\minisentryturret\skeleton.nif`, `nvmrhouse\skeleton.nif`, `nvfestus\skeleton.nif`, `nvsporeplant\skeleton.nif` | **generic** (clips move them by name); no code turns them toward a target | The bones a turret turns to aim. If the game aims them itself, turrets will not track |
| 11 | Particle blocks not read: `BSStripParticleSystem`, `NiPSysMeshEmitter`, `NiPSysColliderManager` with planar and spherical colliders, `NiPSysBombModifier`, `NiPSysColorModifier`, `BSWindModifier`, `BSParentVelocityModifier`, emitter `Ctlr`s | 9, 64, 43, 28, 10, 4, 2, 1-43 | `projectiles\flamerflame01.nif` (strip; 2 NV files in ESM), `clutter\snowglobes\snowglobes_goodsprings.nif` (colliders), `effects\explosiongrenadefrag.nif` (wind) | **unused**: `particles.rs:366` takes `NiParticleSystem` by exact name; already listed in particles.md §5 and effects-animation-audit.md §1 | The flamer's flame stream in `flamerflame01.nif` is a strip system, so it is not drawn |
| 12 | `NiPathInterpolator` (type) | 17 (14 / 11) | `effects\ambient\fxambdust01.nif`, `fxdustwhirlwind01.nif`, `megatonfanltrays01.nif`; VATS cameras | **used** since 2026-10-23: `fnv_nif::anim::PathChannel` (place along the curve; follow and bank not read), and particle emitters under the moved node follow it (particles.md §6.8) | Moves a node along a curve, e.g. drifting dust and whirlwinds |
| 13 | `BBX` (`BSBound`) | 50 (47 / 49) | `characters\_male\skeleton.nif`, `creatures\brahmin\skeleton.nif`, `characters\head\headchild.nif` | **unused** | A box (centre, half sizes [documented: schemas/nif/scene.json fields]) around the actor. It may set actor collision or selection size |
| 14 | `HeadTrack` (`NiFloatExtraData`), `NiFloatExtraDataController` | 56 (values 0: 63, 100: 5), 19 | `armor\leatherarmor\m\outfitm.nif`, `armor\headgear\gambler\gambler_fedora_01_go.nif`, `architecture\mojaveoutpost\nv_mojoutpost_statue01.nif` | **unused** (effects-animation-audit.md §1 lists the controller) | A float on outfits and a statue, sometimes animated. Possibly how far the head may turn while worn; not known |
| 15 | `BSAnimNotes` (type) | 125 `.kf` (125 / n.a.) | `characters\_male\idleanims\intromovieturn.kf`, `3rdp_brahminpush.kf`, `mt_clapan*.kf` | **unused** | Notes on a clip; possibly when foot or hand IK holds and where the head looks |
| 16 | `m:L`/`m:R`, `m:1`/`m:2`, `m:A`/`m:B` (text keys) | 248 `.kf` (216 / n.a.) | `characters\_1stperson\swimmtforward.kf` (`m:1`, `m:2`), `characters\_male\locomotion\hurt\1hpleft_hurt.kf` (`m:A`), Fallout 3 creatures | **unused** (footsteps use `Enum:`, `anim_sounds.rs:76`) | Marks in the movement cycle: left/right steps, swim strokes. May time the swim sounds |
| 17 | `##LaserSightBeam`, `LaserSight` (nodes) | 4, 12 (4, 12 / 4, 12) | `weapons\1handpistol\10mmpistollasersight.nif`, `10mmpistolallmods.nif` | **generic** (drawn as parts) | The beam of a laser-sight mod. Whether it should reach to the hit point, as a laser does, is not known |
| 18 | `ModelSwapNode`, `Camera3rd` (nodes) | 9, 6 (9, 6 / 9, 5) | `vehicles\vertibirdencount01.nif`; `armor\headgear\gambler\gambler_fedora_01_go.nif` (a `Camera3rd` node in a hat) | **unused** | Not known. A swap point for vertibird stages; `Camera3rd` in the fedoras may be an exporter leftover |
| 19 | `Hold`, `PM`, `LoopStart`/`LoopStop` (text keys) | 30, 4, 2 `.kf` | `characters\_male\idleanims\mt_hithead.kf` (`Hold`), `creatures\queenant\h2hequip.kf` (`PM`), `creatures\smspinebreaker\idleanims\specialidle_sitcycle.kf` | **unused**. `idle_play.rs:45` reads `StartLoop`/`EndLoop`, not this spelling | `Hold`: a pause point in throws and hit reactions. `LoopStart`/`LoopStop`: possibly the same as StartLoop/EndLoop |
| 20 | `BSX` (`BSXFlags`) value | 9,421 (7,449 / 7,485) | every placed model | **partly**. Editor markers are found by name (`nif_spawn.rs:324`); the value is not read | Bits set, by file [observed]: 1: 8,366, 0: 1,831, 6: 1,166, 3: 1,152, 9: 430, 5: 336, 7: 162, 4: 117, 2: 43. Their meanings are given by nif.xml [documented: nif.xml BSXFlags; bit 5 "EditorMarkers present" is already cited in nif_spawn.rs] |
| 21 | `SkeletonID` (`NiIntegerExtraData`) | 44 (41 / 43) | `characters\_male\skeleton.nif`, `creatures\brahmin\skeleton.nif` | **unused** | A number per skeleton (8 distinct values; 22 files share 1361955) [observed]. Possibly a hash for matching clips to skeletons |
| 22 | `NiBSBoneLODController`, UPB `BSBoneLOD#…` | 34 (31 / 34); UPB lines in 2,793 objects | `characters\_male\skeleton.nif` | **unused** | Bone level of detail: fewer bones animated far away. Speed only |

Lower still, all **unused**:

- `BobbleHeadRoot`/`Head` in 15 `clutter\bobblehead\*.nif`: perhaps the nodding head.
- `MarkerSource`/`MarkerTarget` (23/17, `markers\furnituremarker*.nif`): editor marker
  models, skipped as markers. **Handled generically** inside a drawn model:
  the Tops lounge chair (`furniture\loungechair_tops.nif`) holds property-less
  copies, with `Capsule01` and `ChairBoundary`; the renderer leaves them out as editor
  geometry (nif.md §6 "Shapes with no property at all", issue #2469).
- `Havok<n>` (96, SCOLs and explosions): exporter helper names.
- `PittedDecal` (14 road meshes): drawn as geometry already.
- `Screen01Root`/`HouseScreenRoot` (Securitron and Mr. House screens): the screens are
  drawn by other means (effects-animation-audit.md §2).
- `Card_0<n>`, `Player_<n>_Shadow`, `Deck_Shadow` (caravan and blackjack tables): caravan
  is drawn as a 2D screen (`caravan/screen.rs`); blackjack reads only its own nodes
  (`blackjack_view.rs:267`).
- `BSWArray` (1, Liberty Prime's projectile).
- `TileShaderProperty` (5 HUD and loading models; the HUD is built from the menu XMLs).
- `NiBinaryExtraData` "Tangent space" (8 test loading models).
- `-GlobalRatio …` text keys (9): exporter compression settings.

## 2. Used or generic (no gap)

| Name | Engine |
|---|---|
| `##` parts (552 distinct names, e.g. `##Clip`, `##Slide`, `##Hammer`, `##ScopeLens`) | generic: clips show, hide and move them by name (`player_body.rs:939`, `anim_blend.rs` extra parts, animation.md §9.7); `##LightOn`/`##LightOff` used by mines (`mines.rs:260`) |
| `ProjectileNode`, `##ProjectileNode` (218, 6) | used, `weapons.rs:339`, `player_body.rs:438` |
| `ShellCasingNode` (128) | used, `player_body.rs:931` |
| `##SightingNode` (97) | used on claude/true-iron-sights |
| `EditorMarker…` (323) | used: hidden, `nif_spawn.rs:324` |
| `AddOnNode<n>` (`BSValueNode`, 117 files) | used, `nvf-assets/src/model.rs:121`, particles.rs (ADDN) |
| `BeamEnd` (13) | used, `shot_fx.rs:491` |
| `Camera1st` (4) | used, `idles.rs:103`, `player_body.rs:1085` |
| `Camera01`, `Camera01.Target` (roulette) | used, `roulette_view.rs:234` |
| `Hand1_Arrow` (blackjack) | used, `blackjack_view.rs:267` |
| `NoHat`/`Hat`, `PipBoyOn`/`PipBoyOff`, `UpperBody`, `MeatCap…`/`limbcaps`/`meathead` caps | used: `actors.rs:417`, `fnv-actor/src/first_person.rs:126`, caps hidden through `BSDismemberSkinInstance` (`nvf-assets/src/actor.rs:16`; severing is claude/dismemberment) |
| `Bip01 NonAccum` (45) and other `NonAccum` nodes | used, `fnv-actor/src/furniture.rs:470`; clip roots |
| `Prn` (466) | used, `fnv-actor/src/mesh.rs:157`, `nvf-assets/src/actor.rs:403` |
| `UPB` (9,263 files) | partly: weapon parts' bone lines read (`nvf-assets/src/model.rs:635`). The other keys are 3ds Max export settings (`Unyielding`, `Mass`, `Friction`, `Simulation_Geometry`, `sgoKeep`, `zMode10`, `BSPriority#n#`, `billboard*`, `NoDecals` [146 lines]) [hypothesis: exporter settings already baked into Havok and other blocks] |
| `FRN` (`BSFurnitureMarker`, 134) | used, `fnv-actor/src/furniture.rs:37` |
| `NiTextKeyExtraData` keys `start`, `end`, `Enum:`, `Sound:`, `Blend…`, `Hit`, `a:`, `prn:`, `Attach`, `Detach`, `Fire`, `Release`, `Loop`, `StartLoop`/`EndLoop` | used: `anim_sounds.rs:76`, `idle_play.rs:45-51`, `player_clips.rs:411-423`, `fnv-actor/src/weapon.rs:178` |
| Skinned extra bones: `Bip01 L/RUpArmTwistBone` (259), `Ponytail…` (75), `Jaw`, `Tongue…`, `Pauldron`, `Brain`, `Sack` | generic: skinned to their parent and moved by clips' tracks of the same name |
| `NiBoolTimelineInterpolator` (97) | generic: every one drives `NiPSysModifierActiveCtlr` (1,123 tracks in 96 files) [observed], read by field name in `particles.rs:293`/`:426` |
| `NiBlend…Interpolator` (380, 236, 206), `NiStringPalette` (5 old `.kf`), `NiDefaultAVObjectPalette` | generic: sequences point at the real interpolators; palette names are read by `anim.rs:494` |
| `bhkBlendCollisionObject` (43 skeletons), `bhkRigidBodyT`, constraints | generic: any collision object's body (`fnv-collide/src/nif.rs:417`), ragdoll constraints `ragdoll.rs:222-228` |
| `BSMultiBound…` (2,678), `NiAdditionalGeometryData` (2,308) | generic: culling bounds and LOD vertex data; nothing visible lost [hypothesis] |
| Light and morph controllers, refraction controllers, `BSFrustumFOVController` | listed in effects-animation-audit.md §1 |

## 3. Counts

[observed] 19,277 files; 160 block types, all decoded. 25,620 distinct object names
(`:n` shape suffixes dropped): 552 `##` parts, 110 `…Node` names, 61 `…Marker` names,
820 `Bip01…` bones, 800 particle names (`p…`, `…-Emitter`), 1,411 3ds Max default names
(`Object01`, `Box01`, `Plane01`, `Dummy01`, `Havok0`…), and 21,866 plain mesh names. 17
extra-data names; 1,238 distinct text keys.
