# kNVSE animation paths in NVForever (N5)

kNVSE lets scripts swap the animation files actors and weapons play, and play sequence
files directly. NVForever's mod lists call SetActorAnimationPath about 2,150 times
(docs/handoff/nvse-next-batch.md). This note says how it fits the engine. Facts are from
the kNVSE pages in the wiki export `[documented]` and the peer's knvse-anim set in the
original game (kNVSE 37) `[observed]`. No kNVSE source is read.

## 1. What the original does [observed: knvse-anim]

- A sequence is named by its path under `meshes`, in the spelling it was first loaded
  under, and lookups compare it exactly: `IsAnimSequencePlaying` with
  `characters\_male\locomotion\mtidle.kf` reads 0 while the game plays
  `Characters\_Male\Locomotion\mtidle.kf`. A sequence a script loads keeps the
  script's spelling.
- `GetPlayingAnimSequencePaths` gives 8 slots, the body's main clip first.
- `SetActorAnimationPath first enable path` returns 1 when the file exists, 0 when
  it does not; disabling returns 1. The replacement applies to the group the file's
  sequence belongs to (`h2hidle.kf` is H2HIdle, so Idle is unchanged).
- `PlayAnimationPath` plays at once: `IsAnimSequencePlaying` reads 1 in the same frame.
  `ActivateAnim` adds a sequence to the active ones.

## 2. The split

- **Game side (`fnv_game::nvse_anim`, slice 1, built):** what scripts asked (the
  replacement paths by actor or weapon and by view; the sequences scripts played) and
  the getters, answered from what the engine tells: the clip each actor's body plays
  (`AnimState::clip`, a path under meshes) and its movement group.
- **Engine side, slice 1 (built):** `nvforever` anim_state.rs tells each NPC's clip
  from its locomotion set.
- **Engine side, slice 2 (built for locomotion, `nvforever` anim_paths.rs):** an actor
  with replacement paths plays from its own copy of the standing set, a file whose stem
  is `mt` + a move's group taking that move; taking the paths back restores the shared
  set. Still to build: the idles, script-played sequences and first person. The original
  plan: the locomotion and idle code asks the game for
  an actor's replacement of a group before taking the set's clip: a per-actor clip
  set made from the shared one with the replaced moves (`Locomotion::set` is an
  `Arc<ClipSet>` shared per skeleton and body, so a replacement clones it for that
  actor), and a script-played sequence goes through the idle player as a special idle.
  The player's first-person clips the same way. The group of a replacement file is its
  sequence's name (`h2hidle.kf` → H2HIdle), read when the file is loaded.
- **Spelling:** NVForever's paths do not keep the original's spelling, so the game side
  compares paths without regard to case [decision; the original compares exactly].

## 3. The folders (2026-11-03, #1461)

kNVSE's AnimGroupOverride folders (`meshes\AnimGroupOverride`, JSON files naming a form,
a folder of KF files and a condition) are read by `fnv_game::nvse_anim_folders`
(docs/spec-notes/knvse-anim-folders.md). Each drawn actor's holding entries give files
that come before SetActorAnimationPath's in `Game::anim_overrides`, which the engine's
slice 2 plays; CopyAnimationsToForm copies the entries too.

## 4. The second slice (2026-11-01)

GetAnimationAttributes, Get/SetGlobalAnimTime, the blend functions, SetAnimWeight,
SetAnimAdditive, the text-key functions, CopyAnimationsToForm and the rest are built on
the game side (`nvse_knvse.rs`, docs/spec-notes/ext-knvse.md). The engine's per-actor
player does not read their state yet.

## 5. The player, conditions and folders (2026-11-13)

- **Conditions** [documented: SetActorAnimationPath, `pollConditionFunction
  conditionFunction`]: a path's condition function is kept with it
  (`AnimPaths::conditions`). `Game::anim_overrides_now` runs each condition on the
  actor (`this`) and keeps the paths whose condition holds; the engine asks every
  frame, which covers polling [hypothesis: the original also stops an override midway
  when `pollConditionFunction` is 1 and its condition turns false]. `Game::anim_overrides`
  (shared borrow, NPCs) leaves conditional paths out.
- **Folders** [documented: "Path may also be a folder containing KF files"]: a path
  that is not a file gives the `.kf` files directly in the folder (`nvse_anim::kf_files`,
  loose files only); SetActorAnimationPath returns 0 when a path gives none.
- **Stacking** [documented: "the previous override will be restored once the other mod
  disables"]: enabling a path again puts it on top; a later file of a clip's name wins.
- **The player** (`nvforever` player_body.rs, `apply_anim_overrides`): each frame the
  game's list for each view (third person: first person off; first person: on) gives each
  rig a map of file names to replacement paths. `Rig::clip` takes a replacement before
  the rig's own folder: the one whose path ends as the clip's candidate does
  (`locomotion\male\mtforward.kf`), else one of that file name not in the other sex's
  folder [hypothesis: a replacement is named as the clip it replaces, as the mods'
  folders do]. A change clears the rig's chosen clips.
- **Seen in Blue Moon:** Enhanced Movement's sprint plays `EM_SprintM\mtfastforward.kf`
  (third person) and `EM_Sprint_1stPerson\mtfastforward.kf` (first) while sprinting, the
  vanilla ones otherwise; Titans of the New West's power armour puts 408 third-person
  files on the player (`AnimGroupOverride\TNW\_male\…`), and its walk plays them.
- **Weapon paths** [documented: SetWeaponAnimationPath; hypothesis: they apply to
  whoever holds the weapon]: the files of the weapon an actor has equipped come after
  the folders' and before the actor's own paths (`Game::weapon_overrides`), for NPCs and
  the player alike.
- **Script-played sequences on the player** (`nvforever` script_sequences.rs,
  docs/spec-notes/player-script-sequence-bridge.md): PlayAnimationPath's and
  ActivateAnim's sequences are requests in `AnimPaths::sequences`, played on the player's
  rigs beside locomotion; PlayAnimationPath's finish by themselves, ActivateAnim's stay
  until DeactivateAnim takes them back [documented: ActivateAnim, "adds a sequence to the
  active ones"] (smoke `knvse-player-sequences`). An earlier slice of this branch played
  them through the idle slot and a layer list of its own; that went when the request
  model reached main (#2395), so one sequence is not played twice.
- **Script-played sequences on NPCs** (`nvforever` idles.rs, `play_npc_sequences`):
  PlayAnimationPath's sequence starts on the drawn actor's idle slot, bound to its
  skeleton, once or looping as on the player; DeactivateAnim stops it; an actor out of
  view starts it again when drawn (smoke `knvse-npc-sequences`).
- **NPCs' conditional paths** (anim_paths.rs): an actor with a conditional
  SetActorAnimationPath gets `Game::anim_overrides_now` each frame, the world put back,
  so the replacement comes and goes with its condition; the others keep
  `Game::anim_overrides`.
- **Still not built:** ActivateAnim's layered sequences on NPCs; NPCs' replacements
  for anything but their standing locomotion.
