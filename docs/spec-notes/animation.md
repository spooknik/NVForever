# Spec notes: animation data (workstream 5, part F) — IDLE records and `.kf` files

Clean-room description in plain language: headless groundwork for animation *selection*
(what an actor plays when, Layer 6). §1–§8 record what the data holds; §9 adds the
movement, sneak and weapon clips and the choices the player body makes with them
(docs/design/player-body.md). Tags: **[documented]** cites a public source, **[observed]** was measured
on the owner's install (FNV 1.4.0.525, the ten masters in the load order of
docs/spec-notes/simulation.md, the archives `fnv_vfs::Vfs::open_default` loads from
`Data`) with `fnvtool actor anims` (§6) unless another command is named,
**[hypothesis]** needs verification against the original game.

Record layouts come from the Layer 1 schema (`schemas/fnv`), re-expressed from xEdit's
public definitions (`Core/wbDefinitionsFNV.pas`, `Core/wbDefinitionsCommon.pas`, branch
`dev-4.1.6`, MPL-2.0) as documentation only; no xEdit code is used. `.kf` decoding is
`fnv-nif` (docs/spec-notes/nif.md). Code: `crates/fnv-actor/src/idle.rs` (IDLE reader),
`crates/fnv-actor/src/anims.rs` (folders, skeletons, bindings, KFFZ/NIFZ lists),
`crates/fnvtool/src/actor_anims.rs` (the catalogue).

## 1. Records

### 1.1 IDLE

Subrecords in order [documented: xEdit IDLE]: `EDID`; a model (`MODL` file name, optional
`MODB`; `MODT`/`MODS`/`MODD` are allowed by xEdit and absent in the masters [observed]);
any number of `CTDA` conditions; `ANAM` (required); `DATA` (required).

- **ANAM** (8 bytes): parent IDLE, previous IDLE (FormIDs; each may be null).
- **DATA** (6 or 8 bytes): animation group section (u8), loop minimum (u8), loop maximum
  (u8), unused (u8), replay delay (i16); in the 8-byte form also flags (u8; bit 0 "No
  attacking") and an unused byte [documented: xEdit, flags optional from the fifth field].
  Instances: 896 × 8 bytes, 849 × 6 bytes [observed: `fnvtool esm survey`].
- **Animation group section byte**: the low six bits name the section — 0 Idle,
  1 Movement, 2 Left Arm, 3 Left Hand, 4 Weapon, 5 Weapon Up, 6 Weapon Down,
  7 Special Idle, 20 Whole Body, 21 Upper Body [documented: xEdit wbIdleAnam]. xEdit names
  bit 0x40 "Loose Idle" and prints "Must return a file" when bit 0x80 is clear
  [documented]; what the engine does with either bit is not known (§5). The reader keeps
  the byte raw (`Idle::group_raw`) and exposes `section()` and `group_high_bits()`.
- **MODL** is relative to `meshes\` [observed: every name resolves under `meshes\`, §2.2].

Counts [observed]: 1,742 winning IDLE records (FalloutNV 1,594, OldWorldBlues 48,
HonestHearts 45, LonesomeRoad 28, DeadMoney 27; 1,745 instances with overrides); none is
deleted. By section: Special Idle 1,299, Whole Body 178, Movement 125, Weapon 70, Idle 49,
Upper Body 21; no record uses Left Arm, Left Hand, Weapon Up or Weapon Down. High bits:
none 1,203, 0x40 441, 0x80 93, both 5. Flags: absent 849, 0 828, "No attacking" 65. Loop
(min, max): (0, 0) 1,462, (255, 255) 93, other small ranges the rest. Replay delay: 0 in
1,696 records.

### 1.2 Conditions (CTDA)

Layout [documented: xEdit wbConditions, already in `schemas/fnv/common.json`]: type (u8),
3 unused bytes, comparison value (4 bytes), function index (u16), 2 unused bytes,
parameter 1 and 2 (4 bytes each), then optionally run-on (u32) and reference (4 bytes).
Instances on IDLE: 1,807 × 28 bytes, 98 × 20 bytes, 2 × 24 bytes [observed: survey].

- **Type byte** [documented: xEdit wbConditionTypeToStr]: top three bits are the
  comparison — 0 `==`, 1 `!=`, 2 `>`, 3 `>=`, 4 `<`, 5 `<=` (6 and 7 undefined); bit 0
  "Or"; bit 2 "Use Global" (the comparison value is then a GLOB FormID, otherwise an f32
  [documented: wbConditionCompValueDecider]). Bit 1 is named "Run On Target" for
  Oblivion/Fallout 3 and "Use Aliases" for later games in xEdit; which applies to FNV is
  not settled here, so the reader exposes only `or()` and `use_global()`.
- **Function**: index into the condition function table; the script opcode is
  0x1000 + index (docs/spec-notes/script-functions.md §4). The catalogue names functions
  from `schemas/script/functions.json`.
- **Parameters, run-on and reference stay raw** (`Condition::param1_raw` …): their types
  depend on the function, and they are file-local values of the IDLE's defining plugin
  (`Idle::plugin`), so a FormID parameter must be mapped through that plugin's master
  list by whoever evaluates it (Layer 5/6).
- xEdit reads run-on as an animation group, not an actor, for `IsFacingUp` and `IsLeftUp`
  (indices 106 and 285) [documented: wbConditionRunOnDecider].

Observed on IDLE [observed]: 1,903 conditions on 1,161 records (at most 13 on one), 62
distinct functions. Most used: GetRandomPercent 362, GetHitLocation 305, GetActorValue
178, IsLastIdlePlayed 86, GetFurnitureMarkerID 79, GetSitting 70, GetCurrentAIProcedure
64, IsCurrentFurnitureObj 56, IsWeaponOut 51, IsMoving 48, IsInCombat 47. Three indices
have no opcode in the script catalogue: 451 (86 uses), 258 (2), 123 (1); xEdit names
them IsLastIdlePlayed (parameter: an IDLE), GetUsedItemLevel and IsGreetingPlayer
[documented: xEdit wbCTDAFunctions]. Run-on values: 0 (subject) 1,797, absent 98,
20 × 5 (all IsLeftUp), 1 × 2 (GetIsID), 7 × 1 (IsInCombat — outside xEdit's 0–4 actor
list; §5).

### 1.3 Actor bases: KFFZ, NIFZ, NIFT

- **KFFZ** (NPC_ and CREA): a list of `.kf` file names; **NIFZ** (CREA): a list of model
  file names; both shown by xEdit only when the actor does not take Model/Animation from
  its template [documented: xEdit]. Stored as NUL-terminated names with one extra NUL
  after the last [observed: all instances in FalloutNV.esm]; decoded with the schema type
  `zstring_array` (added for this note; `crates/fnv-esm/src/schema.rs`). No NPC_ in the
  ten masters has a KFFZ; 202 CREA do (2,719 names); 1,572 CREA have a NIFZ (3,454 names)
  [observed].
- **NIFT** (CREA, 2,238 instances): per NIFZ model, a count-prefixed list of 24-byte
  texture hashes [documented: xEdit wbModelInfos]. Schema v1 cannot express nested
  count-prefixed arrays, so NIFT stays undecoded (docs/design/layer1.md); 1,751 instances
  are 4 bytes [observed], presumably a zero count [hypothesis].

## 2. The IDLE tree

### 2.1 Structure [observed]

- Every parent and previous link resolves to a loaded IDLE; there are no parent cycles.
  597 roots (no parent); depth up to 9 (by depth: 597, 400, 112, 195, 272, 96, 32, 27, 9, 2).
- IDLEs whose `MODL` names a folder (411, e.g. `Characters\_Male\IdleAnims`) are the
  inner nodes: 362 of them have children. IDLEs naming a `.kf` (1,331) are the leaves:
  1,330 have no children. 1,322 of the found `.kf` names are inside an `idleanims\`
  folder; 170 `.kf` files are named by more than one IDLE (1,132 distinct).
- Siblings (same parent) chain through `previous`: in 357 of 364 sibling groups the links
  form exactly one chain from a first sibling (previous null or outside the group)
  through all of them. `Idles::children` returns siblings in that chain order.
- Conditions sit on both levels: 331 of 363 inner nodes and 830 of 1,379 leaves have some.
- 452 roots name a `.kf` and have no children; 244 of their Editor IDs start with `Loose`.

### 2.2 Reading of the structure [hypothesis]

The tree reads like a decision tree for Layer 6: starting from the roots in chain order,
an actor descends into the first node whose conditions pass and plays the first passing
leaf; the section byte says which body part or slot the result drives. Childless roots
would then be idles played on request (scripts, packages, dialogue) rather than chosen
by the tree. None of this is verified; see §5 and §7.

## 3. Animation files

### 3.1 Where `.kf` files sit [observed]

- 4,765 `.kf` files in the archives, all readable, 4,810 sequences (33 files hold more or
  fewer than one). Cycle type (nif.xml CycleType) 2 "clamp" 3,497, 0 "loop" 1,313.
- **Skeleton folder rule** (`anims::SkeletonIndex`): a `.kf` belongs to the nearest folder
  above it that holds a `skeleton*.nif` (prefer `skeleton.nif`), ignoring
  `meshes\clutter\` (bone-pile props). 4,498 files resolve that way in 51 folders.
  43 more are in folders whose actor folder ends like exactly one skeleton folder
  (`meshes\update\characters\_male` and `meshes\nvdlc01\characters\_male` →
  `characters\_male`; `creatures\yaoguai` → `nvdlc02\creatures\yaoguai`;
  `dlcanch\creatures\spidermine` → `nvdlc03\creatures\spidermine`) [rule: hypothesis].
  224 are in Fallout 3 DLC creature folders with no skeleton in the archives
  (`dlc04\creatures\hillfolk*`, `dlc05\creatures\abomination|combatdroid|maintenancerobot`,
  `dlcanch\creatures\chimera`).
- **Layout inside a skeleton folder**: files directly in it (weapon/movement sets,
  1,007 in `characters\_male`), `idleanims\` (982 in `_male`), `locomotion\` with
  `hurt\`, `male\`, `female\`, `child\` (and `toddler\` in `_1stperson`), and in some
  creature folders `specialanims\` (KFFZ targets, §4).
- Groups: `characters\_male` 2,114 files (666 named by an IDLE), `characters\_1stperson`
  442 (none named by an IDLE), 57 creature folders 2,209 files.

### 3.2 Names [observed]

- A sequence name is rarely the file name (494 of 4,810). File stems are mostly a prefix
  plus the sequence name (`1hpattack5down.kf` → `Attack5Down`). Prefixes by files: `mt`
  567, `h2h` 448, `1hp` 346, `2hr` 287, `2ha` 152, `2hl` 150, `2hm` 133, `2hh` 128, `1hm`
  123, `sneak2hr` 108, `sneak1hp` 97, `1gt` 74, `sneak2ha` 64, `swimmt` 56, then `sneak*`,
  `pa*`, `vats*`, `1lm`, `1md`; 1,181 files do not end in their sequence name
  (1,007 of them in `idleanims\`).
- The weapon prefixes line up with xEdit's 14 WEAP animation types (Hand to Hand, Melee
  1/2 Hand, Pistol Ballistic/Energy, Rifle Ballistic/Automatic/Energy, Handle, Launcher,
  Grenade Throw, Land Mine, Mine Drop, Thrown) [documented: xEdit wbWeaponAnimTypeEnum];
  the mapping (`h2h`, `1hm`, `2hm`, `1hp`, `2hr`, `2ha`, `2hh`, `2hl`, `1gt`, `1lm`,
  `1md`; no separate energy prefixes) is [observed] for every type an NPC carries, from
  the weapon parts the clips animate and the WEAP attack animations
  (docs/spec-notes/actors.md §9.3); `1lm`/`1md` and the `mt`/`sneak`/`swim`/`pa`/`vats`
  stance prefixes are [hypothesis].
- Sequence names (270 kinds, `SpecialIdle_…` collapsed): `SpecialIdle_*` 1,525, Equip 138,
  Unequip 135, Aim 132, Forward 115, FastForward 96, Holster 92, Idle 86, AttackRight 81,
  Backward 77, TurnRight 75, TurnLeft 74, … — the animation group names Layer 6 will
  select by [hypothesis].
- Text keys, by first word: start and end 4,848 each, enum 3,754, sound 3,071, blend
  1,918, hit 931, a 734, blendin 708, m 563, endloop 435, startloop 426, prn 376, attach
  183, decal 147, detach 141, eject 88, blendout 72, fire 62, loop 62, release 40, hold
  32; misspellings occur (`blemd`, `blen`, `eum`). Their meanings are Layer 6 work.

### 3.3 Binding to the skeleton [observed]

A track binds when its target node name is a node name of the folder's skeleton (exact
match). 222,219 of 235,204 tracks (94.5 %) bind; 29 of 53 groups with a skeleton bind
every track. The rest name nodes that are not in the skeleton: `##`-prefixed nodes
(weapon and prop parts such as `##SpeedLoader`, `##Trigger`; 2,634 among the per-group
top names), `HeadAnims:0` (1,108 in `characters\_male`, a target on the head mesh
[hypothesis: facial morph controller]) and creature props (Mr. House's bed panels,
Securitron screens). `characters\_male`: 111,189 of 119,031 bound; `_1stperson`: 16,559
of 18,881; creatures: 94,471 of 97,292.

## 4. Lookups

- **IDLE `.kf`** [observed]: 1,322 of 1,331 found; 9 missing (e.g. a roach wings idle and
  three Securitron screen idles; listed by `idle_missing`). 408 of the 411 folder models name
  a folder that holds `.kf` files; the other 3 name `idleanims` folders of creatures with
  no `.kf` there (a roach and a Zax eye).
- **CREA KFFZ** names are bare file names looked up as `<skeleton folder>\specialanims\
  <name>`, then `<skeleton folder>\<name>` [rule: hypothesis; observed: 2,694 names found
  in `specialanims\`, none needed the second place]. 25 names are missing (19 distinct,
  on Mr. Gutsy and Robobrain records).
- **CREA NIFZ** names are found as `<skeleton folder>\<name>` 3,372 times; 82 are missing
  (21 distinct; some on records whose skeleton is `meshes\marker_creature.nif`).

## 5. Open questions for the owner

Idles played on request (scripts, packages, dialogue) and what they showed about loop
counts, loop keys and first-person clips: docs/spec-notes/idles.md. Its §9 (the GECK
wiki's Idle Animations Window page) answers question 2 (the walk is as in §2.2, from the
top item of the actor's skeleton) and says what "Must return a file" in question 1 does;
it also documents Loop Forever, without saying how it is stored.

1. **Bits 0x40 / 0x80 of the IDLE group byte**: xEdit calls 0x40 "Loose Idle" and links
   0x80 to "must return a file". 441 records set 0x40 (414 Special Idles); 93 set 0x80.
   Do you know (GECK UI, wiki) what they do?
2. **Tree walk**: is the reading in §2.2 right — roots in previous-chain order, first
   passing node, first passing leaf? A capture could settle it (§7).
3. **Childless roots** (452, 244 named `Loose…`): played only on request (scripts,
   packages, dialogue)?
4. **Seven sibling groups** whose previous links do not form one chain: data slips, or a
   rule (several chains, evaluated by FormID)?
5. **Run-on 7** on one IsInCombat condition (outside 0–4): ignore, or does it mean
   something?
6. **Condition functions without an opcode** in `schemas/script/functions.json` (451
   IsLastIdlePlayed, 258, 123): should the generated catalogue take xEdit's index for
   them, as it does for other unused vanilla functions?
7. ~~File prefixes~~ → answered from the data (2026-10-09, actors.md §9.3): energy
   pistols use `1hp`, energy rifles `2hr`, thrown weapons `1gt`.
8. **Fallout 3 DLC folders without skeletons** (224 files): leftovers to ignore?

## 6. CLI contract

`fnvtool actor anims <Data> <plugins…> --schema <dir> [--nif-schema <dir>]
[--functions <file>] [--summary-only]` → `fnv.actor.anims/1`. `--nif-schema` defaults to
`nif` next to `--schema`, `--functions` to `script/functions.json` next to it (condition
names are left out when that file is absent). Paths are VFS paths (lower case,
backslashes); FormIDs print as `0x%08X`; f32 values are widened to f64.

Top level: `schema`, `summary`, `groups`, `idle_missing`, `kffz_missing`, `nifz_missing`,
`issues` (IDLE loader issues; `actor_bases` for the KFFZ/NIFZ reader when not empty),
`load_order`, `archives`, `condition_function_names` (how many indices have a name);
unless `--summary-only`, also `files` and `idles`.

- `summary`: `kf_files`, `unreadable`, `sequences`, `files_not_one_sequence`,
  `sequences_named_like_file`, `groups`, `files_by_skeleton_rule` {`ancestor`,
  `sametail`, `none`}, `tracks_checked`, `tracks_bound` (only files with a readable
  skeleton), `text_key_kinds` (first word, lower case; `-options` for exporter option
  strings), `sequence_name_kinds` (names with `_` collapsed to `Head_*`),
  `file_prefixes` (file stem minus its first sequence name that it ends with; `(named
  like file)`, `(no suffix match)`), `idles` {`records`, `conditions`, `by_section`
  {section name or `unknown N` or `no DATA`: {`records`, `high_bits_0xNN`, `kf_found`,
  `kf_missing`, `folder_with_kf`, `other_model`, `no_model`}}, `found_kf_by_group`,
  `tree` (`fnv_actor::idle::TreeStats`: `idles`, `roots`, `dangling_parent`,
  `dangling_previous`, `previous_other_parent`, `sibling_groups`,
  `sibling_groups_one_chain`, `in_parent_cycle`, `by_depth`), `condition_functions`
  (name or `index N`: count), `condition_run_on` (value or `absent`: count)}, `kffz` and
  `nifz` (counts: `CREA with KFFZ`, `names found in specialanims`, `names found in
  skeleton folder`, `names missing`, `lists without MODL`).
- `groups[]`, ordered by folder: {`folder`, `skeleton`, `skeleton_rule` (`ancestor` |
  `same_tail` | `none`), `skeleton_error`, `skeleton_nodes`, `skeletons_in_folder`,
  `files`, `unreadable`, `sequences`, `tracks`, `bound`, `subfolders` {relative folder:
  files}, `named_by_idle`, `named_by_kffz`, `unbound_names`, `unbound_top` [[name,
  count] × ≤ 12]}.
- `files[]`, ordered by path: {`path`, `group`, `sequences` [{`name`, `start_time`,
  `stop_time`, `cycle_type`, `text_keys` (count), `tracks`, `bound` (null without a
  skeleton)}]} or {`path`, `group`, `error`}.
- `idles[]`, ordered by FormID: {`form`, `edid`, `plugin`, `model`, `model_kind` (`kf` |
  `other` | `none`), `found` (kf: file exists; other: a folder holding `.kf` files; null
  without a model), `group_raw`, `section`, `loop` [min, max], `replay_delay`, `flags`,
  `parent`, `previous`, `conditions` [{`function`, `name`, `operator`, `or`, `type_raw`,
  `comparison` (f32, null when it is a global), `comparison_raw`, `param1_raw`,
  `param2_raw`, `run_on`, `reference_raw`}]}.
- `idle_missing[]` {`form`, `edid`, `section`, `model`}; `kffz_missing[]` and
  `nifz_missing[]` {`form`, `edid`, `skeleton`, `name`}.

Library: `fnv_actor::idle::{Idles, Idle, Condition}` (`Idles::build(lo, schema)`,
`children`, `depth`, `tree_stats`) and `fnv_actor::anims::{SkeletonIndex, bind,
kffz_candidates, actor_anim_lists}`.

## 7. Captures wanted

- With `nvf_capture`: an NPC standing idle near furniture and in combat, logging the
  idle played (`IsLastIdlePlayed` / the console's `GetCurrentAIProcedure`) over time, to
  check the tree walk of §2.2.
- `PlayIdle` on a childless `Loose…` root and on an inner node, to see which leaf plays.
- For §9: the player sneaking backwards and sideways, and running sideways (the speeds of
  §9.2 against `fMoveSneakMult`); in third person, screenshots of the pistol and the
  rifle holstered and drawn, standing and crouched, against the player body's (§9.3); a
  video of a walk-to-run change and of drawing a weapon while walking, for blend times
  and layering (§9.4).
- For §9.6: third person from the side with a rifle out, looking 60° up and 60° down
  while walking, while running and while reloading (does the body stay bent?), and
  standing at 30° and 60° (is the bend linear in the pitch?).

## 8. "Random Anim Start" (placed models)

- **Fact** [documented: xEdit wbDefinitionsFNV.pas, record flag lists]: record header
  flag bit 16 (`0x00010000`) is "Random Anim Start" on ACTI, CONT, DOOR, FURN, LIGH,
  MSTT, TACT and TERM; on other types bit 16 means something else (REFR: "High Priority
  LOD") or nothing. The GECK wiki's "World Objects" page (owner's export
  `objects.xml`) describes it: the object starts its idle animation at a random time, so
  several copies in one scene do not animate in step.
- **Data** [observed, ten masters]: 124 base records set it — ACTI 38 of 1,837, CONT 10 of
  3,417, DOOR 1 of 474, FURN 1 of 256, LIGH 9 of 728, MSTT 65 of 300, TACT 0, TERM 0.
- **Engine**: a placed model that plays its embedded idle (`nvf_assets::model::Rig`,
  docs/spec-notes/nif.md §6) starts at a phase spread by the reference FormID when its
  base has the flag (`Rig::spawn_at_phase`, `fnv_actor::anims::random_anim_start`), the
  same spread as actors' idles (actors.md §7.3) — repeatable, not the game's random
  roll. Bases without the flag start at the clip's start, so their copies stay in step
  [hypothesis: the game starts unflagged objects at time 0]. `StreamStats::
  refs_random_start` counts them.

## 9. Movement, sneaking and weapon clips (the player body)

What the human clips hold for moving, crouching and handling a weapon, measured for the
player body (docs/design/player-body.md). [observed] values come from the diagnostic
`crates/fnv-nif/examples/kf_layer_survey.rs` (per clip: timing, text keys, transform
tracks grouped by priority; `KF_ROOT=1` adds the accumulation root's translation at the
start and stop times) on `Fallout - Meshes.bsa` and `Update.bsa`, and from the
`fnv.bsa.listing/1` file list of `meshes\characters\_male`.

### 9.1 Files [observed]

In `meshes\characters\_male` (1,063 `.kf` outside `idleanims\`; `Update.bsa` adds 14):

- Standing still: `locomotion\mtidle.kf` (§7.1 of actors.md); `locomotion\mtturnleft.kf`
  and `mtturnright.kf` (1 s loops, no root rotation track).
- Unarmed movement: `locomotion\male\mt{,fast}{forward,backward,left,right}.kf` and the
  same eight under `locomotion\female\` (plus `pamtforward.kf`, power armour), `child\`
  and `hurt\` (`*_hurt.kf`). `fast` is running.
- Weapon movement: `locomotion\<prefix>{,fast}<dir>.kf` exist only for some types and
  directions: `1hp` and `1hm`, `h2h`, `2hh`, `2hl` have all eight; `2ha` and `2hr` have the
  four `fast` clips and `forward`; `2hm` and `1gt` the four `fast` ones.
- Sneaking: `sneakmtidle.kf`, `sneakmt{,fast}<dir>.kf`, `sneakmtturn{left,right}.kf`,
  `sneakmtjump{loop,land}<dir>.kf` (no plain `sneakmtjumploop.kf`), and per weapon
  `sneak<prefix>{aim,aimup,aimdown,aimis…,equip,unequip,attack…,reload…}.kf`; only `2hh`
  has its own sneak movement (`sneak2hh<dir>.kf`).
- Swimming: `swimmtidle.kf`, `swimmt{,fast}<dir>.kf`, `swimmtturn{left,right}.kf`.
- Jumping: `mtjump{loop,land}{,forward,backward,left,right}.kf`, and per weapon
  `<prefix>jump{loop,land}<dir>.kf` for `1hp`, `1hm`, `2ha`, `2hh`, `2hm`, `2hr`.
- Weapon handling: per prefix `<prefix>equip.kf`, `<prefix>unequip.kf`,
  `<prefix>holster.kf`, `<prefix>aim{,up,down}.kf`; `1hp`, `2ha`, `2hr` also
  `aimis{,up,down}.kf` (iron sights); `1hm` and `h2h` also `<prefix>idle.kf`.

### 9.2 Ground speed of the movement clips [observed]

Every movement clip drives its accumulation root `Bip01` with a translation that grows
over the cycle (no rotation); the distance over the cycle divided by its length:

| Clip | Cycle (s) | Distance | Speed (units/s) |
|---|---|---|---|
| `locomotion\male\mtforward.kf` | 1.200 | 102.3 | 85.3 |
| `…\male\mtbackward.kf` | 1.133 | 76.4 | 67.5 |
| `…\male\mtleft.kf` / `mtright.kf` | 1.200 | 103.8 / 102.8 | 86.5 / 85.7 |
| `…\male\mtfastforward.kf` | 0.800 | 283.1 | 353.9 |
| `…\male\mtfastbackward.kf` | 0.800 | 226.0 | 282.5 |
| `…\male\mtfastleft.kf` / `mtfastright.kf` | 0.800 | 283.0 | 353.8 |
| `…\female\mtforward.kf` / `mtfastforward.kf` | 1.200 / 0.800 | 102.1 / 283.1 | 85.1 / 353.9 |
| `locomotion\1hpfastforward.kf` | 0.800 | 283.0 | 353.8 |
| `sneakmtforward.kf` / `sneakmtfastforward.kf` | 1.533 / 0.867 | 92.0 / 132.6 | 60.0 / 153.0 |
| `sneakmtleft.kf` | 1.267 | 99.7 | 78.7 |
| `sneakmtbackward.kf` / `sneakmtfastbackward.kf` | 1.733 / 0.933 | 83.5 / 100.6 | 48.2 / 107.8 |
| `swimmtforward.kf` | 1.600 | 180.0 | 112.5 |

Compared with the player measured in game (physics.md §6.3): walking 86.8 against 85.3,
running 344.0 against 353.9, running backwards 272.8 against 282.5 — the backward share
is 0.793 measured and 0.798 in the clips. Sneaking forward was measured at 197.7 (344 ×
`fMoveSneakMult` 0.57), not the sneak clips' 153.0. Reading [hypothesis]: the game's
standing speeds come from the clips' root motion (within 3%), and sneaking scales the
standing speed by `fMoveSneakMult` instead; the player body plays every movement clip at
the speed the character actually moves (clip rate = ground speed / root speed).

### 9.3 Priorities and text keys [observed]

Controlled-block priorities (nif.xml ControlledBlock "Priority") of the transform tracks:

| Clip | Legs and pelvis | Spine, neck, head | Arms and hands | Other |
|---|---|---|---|---|
| `mtidle.kf` | 10 | 10 | 10 | `##VisCtrl` 20 |
| `sneakmtidle.kf` | 20 | 20 | 20 | |
| `mt*`, `sneakmt*` movement | 30 | 30 | 30 | |
| `locomotion\1hpforward.kf` | 30 | `Bip01 Spine` 30, the rest 31 | 31, right arm 35 | |
| `locomotion\1hpfastforward.kf` | 30 (pelvis 31) | 36 | 36 | `Weapon` 35 |
| `1hpaim.kf` | 25 | 30 | 30, right arm 35 | weapon parts (`##…`) 0–55 |
| `2hraim.kf` | 25 | 30 | 35, fingers 45 | |
| `h2haim.kf` | 15 | 30 | 45 | |
| `1hpaimis.kf` / `2hraimis.kf` | 26 | 50 | 50 | |
| `1hpequip.kf` / `1hpunequip.kf` | 27 (L toe 26) | 55 | left arm 29, right arm 55 | |
| `2hrequip.kf` | 26 | 55 | 55 | |
| `h2hequip.kf` | 25 | 30 | 25, hands 55 | |
| `1hpholster.kf` | pelvis 5 | | | `Weapon` 55 |
| `2hrholster.kf` | | `Bip01 Spine2` 5 | | `Weapon` 55 |

So clips appear to be made to be laid over each other: idle below everything, the
posture's movement above it, a weapon's aim pose taking the arms and spine but leaving
the legs to the movement (25 < 30), drawing and holstering above all.

Text keys (each key's text may hold several lines; spelling and spaces vary):

- `Blend:N`: `Blend:6` on `mtforward.kf` and `mtleft.kf`, `Blend:9` / `Blend: 9` on
  `mtfastforward.kf` and `mtbackward.kf`, `Blend:3` on `1hpfastforward.kf`,
  `mtjumploop.kf`, `mtjumpland.kf` and `mtturnleft.kf`, `Blend: 12` on `sneakmtidle.kf`,
  `Blend:1` / `Blend: 1` on the holster clips. Key times sit on a 1/30 s grid (0.033,
  0.067, 0.133 …).
- `prn: <bone>`: `prn: Bip01 R Hand` on `1hpequip.kf` (0.133 s), `1hpaim.kf` and
  `2hrequip.kf` (0.633 s); `prn: Bip01 Pelvis` on `1hpunequip.kf` (0.200 s) and
  `1hpholster.kf`; `prn: Bip01 Spine2` on `2hrholster.kf`; `prn: Bip01 R ForeTwist` on
  the `h2h` clips.
- `Attach` on equip clips (`1hpequip.kf` 0.167 s, `2hrequip.kf` 0.300 s, `h2hequip.kf`
  0.133 s), `Detach` on unequip clips (`1hpunequip.kf` 0.233 s, `h2hunequip.kf` 0.167 s).
- `Enum: Equip` / `Enum: Unequip`, `Enum: Left` / `Right`, `Enum: ArmorWalk`,
  `FoleyWalk`, `ArmorRun`, `FoleyRun`, `ArmorSneak`, `FoleySneak`, `m:L` / `m:R`
  (footsteps and sounds; played as audio.md §10 says).

The skeleton (`characters\_male\skeleton.nif`) has a `Weapon` node under `Bip01 R Hand`
(translation 6.17, 1.99, 0.76) and a `Camera3rd` node under the root at (0, −3.74,
118.36) [observed: node tree].

Readings used by the player body [hypothesis]:

- `Blend:N` is the clip's blend-in time in frames at 30 per second.
- The weapon hangs from the `Weapon` node; while holstered that node hangs from the bone
  the holster clip's `prn:` names, posed by the holster clip's `Weapon` track: on the
  right hip for pistols, across the back for rifles. Drawing moves it to `Bip01 R Hand`
  at the equip clip's `Attach` key; holstering moves it back at `Detach`. Rendered, the
  9mm pistol sits at the right hip and the varmint rifle across the back [observed:
  screenshots, kept privately].
- The weapon-out stance is the type's Aim group (looped), with `aimup`/`aimdown` blended
  in by the view's pitch, and the `AimIS` clips while aiming down the sights (§9.6); in
  first person only for a gun whose model has a sighting node, else the hip clips go on
  (camera.md §4.1).

### 9.4 Blending [hypothesis]

nif.xml names the fields of `NiBlendInterpolator` "Array Size", "Weight Threshold",
"Interp Count", "Single Index", "High Priority", "Next High Priority", "Single Time",
"High Weights Sum", "Next High Weights Sum" and "High Ease Spinner" [documented: nif.xml],
which reads as blending by priority first, then by weight within a priority, with the
weights of a priority summed (so shared in proportion) and an ease apart from the
weights. The player body uses that rule per bone (`nvf_assets::anim_blend`):

- tracks are taken from the highest priority down;
- tracks of equal priority share by their clips' weights, whatever the weights add up
  to (a clip alone at its priority on a bone has all of that priority's part);
- how much of the bone a priority takes is its clips' fade (ease, 0 to 1), added up and
  at most what the priorities above left; the bind pose fills any rest;
- clips fade in over their `Blend` time (0.2 s without one) and out over 0.2 s; a clip
  fading out keeps its weight, so its share shrinks with its ease only.

Until 2026-10-10 a priority took its clips' weights added up instead. That is wrong for
the Aim group, whose level clip's weight falls as the view pitches (§9.6): the arms,
which only the level clip drives for rifles, slid to the idle pose. Nothing here was
compared with the game frame by frame; §7 lists the captures that could.

### 9.6 Aiming up and down [observed unless tagged]

Measured with the diagnostic `crates/fnv-nif/examples/kf_pose_compare.rs` (clips side by
side, bone by bone: track priority, and the rotation's angle from the first clip's at the
clips' start) on `Fallout - Meshes.bsa` and `Update.bsa`, `meshes\characters\_male`.

- Which clips have `up` and `down` forms (the same file name with `up` or `down` added,
  after `is` for the sights form: `2hraimup.kf`, `2hraimisdown.kf`, `2hrattack8up.kf`,
  `2hrattackleftisup.kf`, `sneak2hraimup.kf`, `sneak2hrattack8down.kf`): the Aim clips of
  `h2h`, `1hm`, `1hp`, `2hm`, `2hr`, `2ha`, `2hl`, `2hh` (not `1gt`, `1md`, `1lm`), and
  attack clips of the gun types (`2hr`: `attackleft`, `attackright`, `attack3`–`attack9`,
  each also `is`; `1hp`, `2ha`, `2hl`, `2hh`: those looked for exist, `attackloop` too
  for `1hp`, `2ha`, `2hh`). Movement, jump, equip, reload, block and melee power-attack
  clips have none (`2hrfastforwardup.kf`, `2hrjumploopforwardup.kf`, `2hrequipup.kf`,
  `2hrreloadaup.kf`, `2hmblockidleup.kf`, `2hmattackpowerup.kf` do not exist). The
  first-person folder has none (§9.5).
- An up or down form has the length, text keys and track priorities of its level clip,
  but not always its bones:

  | Type | Level clip tracks | Up/down tracks | What the up/down form drives |
  |---|---|---|---|
  | `2hr` aim, aim `is` | 92, 99 | 19, 18 | spine (3 bones), neck, head, pelvis, legs, `Weapon`; no arm bones |
  | `2hr` attacks | 61–85 | 13–25 (`attack6`, `attack7`: 58, 65) | `attackleftup`: spine, pelvis; no arm bones (the others not listed bone by bone) |
  | `2hm` aim | 69 | 12 | head and upper neck, legs; no spine, no arm bones |
  | `1hm` aim | 67 | 15 | head and upper neck, the four arm twist bones, legs; no spine |
  | `2ha` aim | 77 | 30 | spine, both arms, neck, head |
  | `1hp` aim | 120 | 68 | spine, both arms, neck, head |
  | `2hl`, `2hh`, `h2h` aim | 64, 71, 61 | 59, 60, 61 | nearly as many tracks as the level clip (`2hl`: spine and arms) |

- `2hraimup.kf` and `2hraimdown.kf` turn `Bip01 Spine`, `Spine1` and `Spine2` 30.0° each
  from their `2hraim.kf` rotations (priority 30 in all three clips) and move nothing
  else by more than 2.1° (the head). The same 30.0° on each spine bone holds for
  `2hraimisup`/`down` against `2hraimis` (priority 50), `2hrattackleftup`/`down` against
  `2hrattackleft` (55) and `2hrattackleftisup` against `2hrattackleftis` (56). `2haaimup`
  turns them 29.2°, 27.3° and 28.5°; `1hpaimup` 13.1°, 13.4° and 17.5°, with the right
  upper arm 100° from its level pose.
- Readings [hypothesis]:
  - the up and down forms are the poses at the ends of the view's pitch, ±90° (three
    times 30° for the rifle), and the game mixes level with up or down by the pitch;
  - a bone that only the level clip drives stays with the level clip in full however
    the three are mixed: a rifle's arms and hands are in `2hraim.kf` alone, so the
    level clip's weight cannot be what frees a bone for the clips below (§9.4);
  - an attack plays with its own up and down forms mixed the same way, so the body
    stays bent while firing (the level attack clip drives the spine above the aim
    clips' priority and would straighten it).
- Engine (`nvforever::player_body`, third person only): level, up and down aim clips at
  weights linear in the pitch, the level clip played even at weight 0; an attack starts
  its up and down forms with it (`fnv_actor::player_clips::aim_variant`) at the same
  weights. Rendered with the varmint rifle [observed: screenshots, kept privately]: at
  45° and 80° up and down, from the hip and down the sights, and while firing 60° up and
  45° down, both hands stay on the rifle and the rifle points along the view. Before,
  the rifle hung from the right hand at the hip at 80° and the left arm trailed.
- Not explained: with a rifle out and moving, the movement clip drives the spine
  (`2hrforward.kf` 31, `2hrfastforward.kf` 36) and, running, the arms (36) above the aim
  clips (spine 30, arms 35), and it has no up or down form. By the priorities alone the
  body does not bend with the pitch while walking or running, and ours does not: the
  rifle stays level. Whether the game's does is not known (owner-review.md).
  Reloading (`2hrreloada.kf`, spine at 65, no up or down form) straightens the body in
  ours for the same reason.
- Down the sights while strafing [observed: `kf_pose_compare`, 2026-10-23]: `2hr` has no
  side-step clips (`locomotion\2hrleft.kf` does not exist), so the unarmed
  `locomotion\male\mtleft.kf` plays. It drives `Bip01 NonAccum` at 30, above
  `2hraimis.kf`'s 26, and turns it 87.5° from the sights clip's pose (`mtright.kf` 2.7°,
  `mtforward.kf` 2.7°, `mtbackward.kf` 0.1°), turning its head back by 88°. Sampled
  (`fnvtool actor locomotion --samples 5`), `Bip01 NonAccum` stays at yaw 87.5–90.9° in
  every move of `male\` and `female\` and in the weapons' own side-steps (`1hpleft`,
  `1hpright`, `1hmleft`, `2hhleft`, `h2hleft`, `1hpfastleft`) but sits at 177.5° in
  `male\mtleft.kf` and `female\mtleft.kf` and −168.6° to −179.4° in their
  `mtfastleft.kf`: the unarmed walk left turns the whole body (`child\mtleft.kf` does
  not). `Bip01 Spine` and `Bip01 Pelvis` both hang from `Bip01 NonAccum`, the thighs from
  the pelvis [observed: `skeleton.nif` node tree]. With the sights clip's spine (50) over
  that turned node, the body and rifle faced a quarter turn left of the view.
- In the original, strafing left down the sights with a rifle, the body is yawed some
  30–45° right of the view, knees bent, the legs stepping back across one another (the
  owner: "almost like a backpedal"), the rifle on the crosshair [observed: the owner's
  capture 2026-10-23, camera zoomed out]. Rendered in ours under `2hraimis.kf`,
  `h2hleft.kf` and `1hpleft.kf` look like that; `mtleft.kf` (the body turned, then with
  the body held square a forward stride) and `mtbackward.kf` (square) do not [observed:
  screenshots, kept privately]. Both side-steps keep every track at priority 30, under
  the rifle's aim arms (35) and sights clip (50).
- Engine [hypothesis: which side-step the game takes is not known]: a weapon with no
  walk left of its own walks left with `h2hleft.kf`
  (`fnv_actor::player_clips::move_clips`; running left keeps `mtfastleft.kf`, which turns
  the body the same way: not seen in the original yet). NPCs take the same side-step
  from 2026-10-25, and `h2hfastleft.kf` running left where the stance has no run left
  (`fnv_actor::locomotion::weapon_moves`; design/combat-ai.md §23). The first-person folder has no
  `h2hleft.kf`, so the arms rig keeps `mtleft.kf`. Also, while down the sights in third
  person, `player_body::keep_sights_facing` gives the nodes above `Bip01 Spine` the aim
  clips' own rotation in place of the blend's, faded with the aim clips (2.7° with
  `h2hleft.kf`; it keeps the rifle on the crosshair under `mtfastleft.kf`). Turning only
  the spine back instead (the first try) wrung the skin at the waist a quarter turn,
  and holding the body square over `mtleft.kf` (the second) made the legs walk forward
  [observed: the owner's screenshots]. Smoke `third-person-sights-strafe`.

### 9.7 The `Weapon` node between clips [observed unless tagged]

Measured with `kf_pose_compare` (`KF_OTHER=1` lists each transform track's place and the
tracks that are not transforms) on `Fallout - Meshes.bsa` and `Update.bsa`.

- Not every clip of a weapon type drives the `Weapon` node (the node under
  `Bip01 R Hand` the weapon model hangs from; bind translation 6.17, 1.99, 0.76):
  - `2hh`: `2hhequip.kf`, `2hhunequip.kf` (priority 55), `2hhattackspin.kf` and
    `2hhreloade.kf` (57) all put it at (6.40, 2.07, 2.83) with one rotation;
    `2hhaim.kf`, `2hhaimup.kf`, `2hhaimdown.kf` and the `2hh` movement and turn clips
    have no `Weapon` track.
  - `1hp`: `1hpequip.kf` has one (55), `1hpaim.kf` has none (`1hpaimup.kf` and
    `1hpaimdown.kf` do, at 35).
  - The Aim clips of `2hr` (45), `2ha` (35), `2hl` (37), `2hm` (45), `1hm` (45) and `h2h`
    (45) drive it.
- The arms of `2hhaim.kf` are those of `2hhattackspin.kf` at its start to within 0.1°
  on every bone.
- Reading [hypothesis]: a node keeps the pose the last clip gave it while no playing
  clip drives it, so the minigun stays where its draw clip put it through the aim and
  movement clips. Returning the node to its bind pose instead turns the minigun in the
  hand: the left hand then misses the handle while idle and meets it only while firing
  [observed in ours before 2026-10-10: owner's screenshots].
- Engine (`nvforever::player_body`): while the weapon is in the hand, the `Weapon`
  node's rest pose is where the draw clip leaves it at its end
  (`Animator::set_rest`), in place of the bind pose; clips that drive the node still
  do. Not the last clip's pose as such: the draw, attack and reload clips checked agree,
  and a fixed pose does not depend on what played before. Rendered [observed:
  screenshots, kept privately]: the minigun idle, level and 50° up and down, has the
  left hand on the top handle as while firing, with no jump when firing stops; the 9mm
  pistol is held as before.
- NPCs (`nvforever::weapons`, since 2026-10-16): the same rest pose, the end of the
  stance's equip clip's `Weapon` track, while the weapon hangs from the drawn bone
  (`BlendPlayer::set_rest`); holstered, the bind transform. Before, an NPC's flamer was
  turned in the hand and the left hand missed it [observed in NVForever: owner's
  playtest 2026-10-16, Cook-Cook]. Rendered [observed: screenshots of Cook-Cook with
  `--actor-weapons drawn`, kept privately]: the flamer is held forward, the right hand
  on the rear grip and the left on the top handle; before the change it hung turned
  down at his side.
- The left hand on the weapon, measured with `nvf-assets/examples/hand_probe.rs` (the
  skeleton posed from the idle, aim and attack clips at weight 1, the `Weapon` node
  without a rest pose): its place in the `Weapon` node's frame is (27.2, -5.0, -15.7)
  through `2hhattackspin2.kf`, `2hhattackspin.kf` and `2hhattackloop.kf` standing,
  (27.1, -5.6, -17.0) through `sneak2hhattackspin2.kf` crouched, and at the end of
  `2hhequip.kf` (23.7, -6.4, -14.8), of `sneak2hhequip.kf` (27.1, -5.6, -17.0). The
  right hand is 7.3 units from the node in every attack clip. `2hhreloadb.kf` takes the
  left hand to (24.0, 3.0, 11.4), off the handle. The crouched equip clip leaves the
  `2hh` node where the standing one does (0.0 units, 0°); `2hr` 0.3 units and 13°, `2ha`
  0.8 units and 5° apart [observed: NVForever's stance log].
- Engine (`nvforever::weapons`, `attack_anims::grip_check`, since 2026-11-01): crouched,
  the rest pose is the crouched equip clip's end. While a shot's clip plays (not a reload
  or a blow) the log gives, every 0.5 s, the right hand's distance from the `Weapon` node
  and the left hand's from its place at the equip clip's end, as drawn. Cook-Cook with
  the flamer, standing before and after a reload: 7.3 and 3.9 units; crouched: 7.3 and
  0.0 to 0.7 [observed: smoke `cook-cook-flamer-hands`]. Its frames show the flamer at
  the hip, the right hand on the rear grip and the left on the top handle, standing and
  crouched. The pose the owner saw on 2026-11-01 (#1309: the flamer at knee height below
  his hands) was not seen again in these runs, nor in `npc-muzzle-flash-flamer`'s frames
  on the same build.
- The `2hh` holster, draw and put-away clips also carry visibility tracks
  (`NiVisController` with `NiBoolInterpolator`) on parts of the weapon model named
  `##MinigunGun`, `##FlamerGun`, `##GatlingLaserGun` and `##RIL`: off in
  `2hhholster.kf`; in `2hhequip.kf` off until 0.500 s (0.533 s for the minigun; the
  `Attach` key is at 0.50) and on after; in `2hhunequip.kf` on until 0.500 s (`Detach`)
  and off after. `2hhholster.kf` names `prn: Bip01 R Hand`, not a back bone.
  `weapons\2handhandle\minigun.nif` has the gun under a node `##MinigunGun` and a
  separate node `Backpack` whose `UPB` string is `Bip01 Spine2`. Reading [hypothesis]:
  a holstered minigun shows its backpack on the back and no gun; the gun appears half
  way through the draw.
- In the game [observed: owner's screenshots 2026-10-10, kept privately]: the minigun
  holstered, seen from behind, shows the backpack on the upper back and no gun; drawn,
  the gun is in both hands and the backpack is still on the back.
- Engine (`nvforever::player_body`, since 2026-10-10):
  - clips carry their visibility tracks (`fnv_nif::anim::BoolTrack`,
    `nvf_assets::anim_play::Clip::visibility`); a shape of the weapon model is hidden
    while any node above it whose name begins `##` is switched off. Among the playing
    clips the track of the highest priority decides, the latest started of equals
    (`Animator::visible`); a node no playing clip has a track for stays as it was, and
    starts shown [hypothesis: as the `Weapon` node keeps its place];
  - a shape under a node whose `UPB` string names a bone of the skeleton hangs from
    that bone instead of from the `Weapon` node, with its transform in the model kept
    [hypothesis: that this is what the string means]; such shapes are not drawn on the
    first-person rig.
  - Rendered with the minigun [observed: screenshots, kept privately]: holstered, the
    backpack on the back and no gun, from behind and from the side as in the owner's
    game screenshot; 0.3 s into the draw still no gun, 0.8 s in the gun is in the
    hands; drawn, gun and ammunition box in the hands and the backpack on the back;
    0.3 s into putting it away the gun still shows, 0.8 s in it is gone.
  - Not built: the `NiMaterialColorController` tracks on `##GatlingLaserGun:0`. The
    transform tracks on `##` nodes are §9.8.
- `weaponshandhandlelamer.nif` [observed: fnvtool nif decode] is laid out as the
  minigun: the gun under `##FlamerGun`, the tank under a node `Backpack` whose `UPB`
  string is `Bip01 Spine2`, and one untagged shape `Flamer:0` under the root.
- NPCs (`nvforever::weapons`, since 2026-10-16): the same two rules. A shape under a
  node whose `UPB` string names a skeleton bone hangs from that bone (not part of the
  weapon's hit volume, and hidden with the weapon under Weapons Unequipped). The nodes
  the stance's holster clip switches off are shown as the current stage's clip's
  visibility track says at its time; a clip without a track for the node (the aim clip)
  leaves it as it was, shown at first [hypothesis, as for the player]. Rendered
  [observed: screenshots of Cook-Cook, kept privately]: holstered, the tank on his back
  and empty hands; with `--actor-weapons drawn`, the flamer in both hands and the tank
  still on the back. Before, the holstered flamer hung whole, tank included, from the
  hand bone (the `2hh` holster bone is `Bip01 R Hand`) [observed in NVForever: owner,
  2026-10-16].
- The round seen in the weapon, `##NifRound` [observed: `fnvtool nif nodes` over the
  game's and the DLCs' archives; `fnvtool nif decode` of the clips, 2026-11-14, #2470]:
  - seven models have the node: `missilelauncher.nif` (with its guidance-system and
    unique forms), `fatman.nif`, `fatmanlittleboy.nif`, the Gun Runners' Arsenal unique
    Fat Man and `dartgun.nif`; in `missilelauncher.nif` it holds a copy of the missile's
    shape (the mesh of `projectiles\mlprojectile.nif`, 444 vertices) in the tube. The
    world models (`missilelaunchergo.nif`) have none.
  - its `NiVisController` tracks: `2hlattackright.kf` and `2hlattackleft.kf` show it to
    0.033 s and hide it from 0.067 s (the `Hit` key) to their end, in `_1stperson`
    (priority 55) and `_male` (57); `2hlreloada.kf` hides it until 0.867 s and
    `2hlreloade.kf` until 1.833 s (`_male` 1.867 s), then shows it; `2hlattack3.kf`,
    the equip, unequip, holster and jam clips have no track for it. `_1stperson\2hlaim.kf`
    shows it throughout (an interpolator value of true, no keys, priority 45);
    `_male\2hlaim.kf` has no track. The `1hp` clips do the same for the dart gun
    (`1hpattackright.kf` hides it from 0.033 s; `_1stperson\1hpaim.kf` shows it, 55;
    `1hpreloadb.kf` shows it from 1.333 s, `_male` 1.0 s).
  - Reading [hypothesis]: the round is shown while one is loaded; the first-person aim
    clip's track would show it again in an empty launcher once the attack clip ends,
    unless a reload follows the shot at once. How the original hides it in an empty
    weapon is not known.
  - Engine: the round is drawn as the clips say; an empty weapon's round is not hidden
    [design]. It is not seen from the player's cameras [observed: NVForever on main,
    2026-11-14, pictures kept privately: first person after the shot, third person after
    the shot and an empty launcher drawn in third person, the back of the tube dark].
    NPC weapons play no clips on their parts (`nvforever::weapons`): an NPC's launcher
    always shows its round.

### 9.8 The moving parts of a weapon [observed unless tagged]

Measured with `crates/fnv-nif/examples/weapon_parts_survey.rs` (a model's scene objects;
per clip, the tracks whose target is not a skeleton node, how far each moves its target
and which of the models have an object of that name; `PARTS_DETAIL=<node>` samples one
track) on `Fallout - Meshes.bsa` and `Update.bsa`.

- A weapon model holds each moving part as a node of its own named `##…`, under the
  root or under another part, with the part's shape `<name>:0` below it and a
  `NiTransformController` on the node. The weapon files hold no sequences.
  - `weapons\1handpistol\10mmpistol.nif`: `##Slide`, `##Trigger`, `##Catch`, `##Hammer`,
    `##Clip` under the root `10MMPistol`, beside `Object` (the frame), `ShellCasingNode`,
    `ProjectileNode` and `##SightingNode`.
  - `weapons\2handhandle\minigun.nif`: `##MinigunGun` (a `NiVisController`, §9.7) with
    `##MinigunAmmo`, `##MinigunAmmoLock` and `##MinigunBarrel` under it.
  - `weapons\2handrifle\varmintrifle.nif`: `##VRBolt`, `##VRTrigger`, `##VRClip`, and a
    node `Silencer` with a controller but no `##`.
- The actor clips move those parts: a clip's transform tracks name them as they name
  bones. These are the tracks that bind to no skeleton node (§3.3).
  - `characters\_male\1hpreloada.kf` (1.3 s; 64 tracks, 62 on bones): `##Clip` at
    priority 65 travels 52.7 units from its place and turns 115°, `##Slide` (65) goes
    back 4.85 units. The first-person clip of the same name: `##Clip` 57.5 units and
    146°, `##Slide` 4.2 units, `##Hammer` 41°.
  - `1hpattackright.kf` (0.5 s; 112 tracks, 61 on bones): `##Slide` (55) goes back 3.79
    units, `##Trigger` turns 17°, `##Hammer` 43°. The other 48 tracks off the skeleton
    (30 of them transforms) name parts of other pistols (`##cmSlide`, `##nmBar`,
    `##NifRound`, `##dgLever`, …).
  - `1hpaim.kf` (120 tracks, 60 on bones) has tracks for parts of other pistols and
    none for the 10mm's; `2hhaim.kf` holds `##MinigunAmmo`, `##MinigunAmmoLock` and
    `##MinigunBarrel` at their places (priority 35).
  - `2hhattackspin.kf` (2.5 s, cycle type 2; text keys `Fire` at 0.97 s and `Loop` at
    1.63 s): its one part track turns `##MinigunBarrel` (57) about the gun's long axis
    (x), with no translation. Sampled 30 times a second, the barrel turns at 675° a
    second for 0.4 s, 900° to 0.87 s, then 2,700° a second until 1.63 s (five whole
    turns between `Fire` and `Loop`), and slows from there to 450° a second at the end,
    where it is back at its starting angle after ten turns. So the clip is a spin-up
    to `Fire`, a steady part from `Fire` to `Loop` and a spin-down after `Loop`.
    `2hhattackloop.kf` (0.2 s) has a `##MinigunBarrel` track that pulls the node back
    11.5 units along x and returns it with no turn; the minigun's WEAP names
    `AttackSpin`, so that clip is not the one it plays.
  - The `Loop` key is on the `attackspin` clips only: 30 of the 938 clips directly in
    `characters\_male` (`1hm`, `2hm` and `2hh`; `attackspin` and `attackspin2`, with
    their up, down and sneak forms), each with a `Fire` key before it.
  - The keys' meaning [documented: GECK wiki NiTextKeyExtraData, owner export; quoted
    in docs/design/combat.md §4.2]: `Fire` works like `Hit` (the shot leaves) and also
    starts the internal loop of a clamped sequence; `Loop` jumps back to it. The
    barrel's speed above fits: the steady part is the loop.
  - On the owner's install the engine resolves a different `2hhattackspin.kf`, in both
    folders [observed: the body's debug line, which now gives each chosen clip's length
    and keys]: 1.733 s long, `Fire` at 0.200 s, `Loop` at 0.867 s. A later archive of
    the install overrides the base file; which one was not looked up, and that file
    was not measured (surveys read the base archives only).
- One clip carries the tracks for the parts of every weapon of its type; a weapon takes
  the tracks whose names its model has. Which clip plays comes from the WEAP record
  (attack and reload animation), so two weapons of one type can move differently.
- Transform tracks target nodes. The other kinds on weapon parts target shapes or
  nodes: `NiVisController` (`##MinigunGun`, `##NifRound`, `##cmClip`; §9.7),
  `NiMaterialColorController` (`##GatlingLaserGun:0`), `NiTextureTransformController`
  (`##Display01:0`) and `NiGeomMorpherController` (`##dgRubberBand:0`).
- Reading [hypothesis]: the weapon model is attached below the skeleton's `Weapon` node
  and the clip's tracks find their targets by name in the whole tree, the weapon's nodes
  included.
- Engine (since 2026-10-10):
  - a prepared model keeps its nodes below the root and each shape's node and own
    transform (`nvf_assets::model::PreparedModel::nodes`, `PreparedShape::node`, `own`);
  - the player's weapon model is spawned as that tree under the `Weapon` node, and
    every named node and shape of it is given to the rig's animator as an extra node
    (`nvf_assets::anim_blend::Animator::set_extra`): a track drives the skeleton node of
    its name, else the weapon's. Parts blend by priority and fade like bones; a part no
    playing clip drives is at its place in the model (since 2026-10-20 in the hand: where
    the draw clip leaves it, below). Both rigs, first and third person;
  - an attack clip with a `Loop` key is not started again by the shots that follow
    while it runs; it goes round from its `Fire` key to its `Loop` key while shots come
    no more than 0.25 s apart [engineering choice], then plays out from where it is
    (`nvforever::player_body`, `LOOP_HOLD`);
  - spin-up [owner decision 2026-10-10]: an automatic weapon with such a clip fires its
    first shot at the clip's `Fire` key. On the pull, combat starts the spin-up
    (`fnv_combat::attack`, `Phase::SpinUp`, `Event::SpinUp`) and the body starts the
    clip from its beginning (`WeaponAnim::spin_ups`), so the shot and the key meet;
    the shots then come at the weapon's rate while the trigger stays held. Letting go
    before the first shot fires nothing and the clip plays out; every new pull, and
    firing on after a reload, spins up again [design]. The time is the key's time in
    the clip the rig in view resolves, not scaled by the weapon's animation
    multipliers, because the body plays attack clips at their own speed;
  - the body's once-a-second debug line lists the parts the clips hold away from their
    place, and each attack or reload clip that starts lists the parts it moves
    (`RUST_LOG=info,nvforever::player_body=debug`).
- Checked in ours [observed 2026-10-10, `--play` at Goodsprings with
  `NVF_PLAY_TEST_EQUIP`, `NVF_PLAY_TEST_GIVE` (ammunition) and
  `NVF_PLAY_TEST_CONTROL=<t>:weapon,<t>:fire,<t>:fire,<t>:reload`, exit 0 and "run
  finished" each time; screenshots kept privately]:
  - 10mm pistol (`0000434F`), first person: 0.08 s after the shot the slide is back and
    the barrel shows; 0.35 s into the reload the magazine is out of the grip, in the
    left hand, and at 0.6 s the hand is bringing it back up. The clips report
    `##Hammer`, `##Slide`, `##Trigger` (attack) and `##Clip`, `##Hammer`, `##Slide`
    (reload; the third-person clip `##Clip`, `##Slide`).
  - Minigun (`0000433F`), trigger held 4 s (80 shots), before the spin-up rule:
    `AttackSpin` stays for the whole burst and about a second after it, with the
    barrel at a different angle in each status line (46°, 141°, 45°, 70°, 170°, 148°),
    then the barrel is back at its place. Before, each shot started the clip again,
    so it never got past its first twentieth of a second.
  - Minigun with the spin-up rule (the install's clip, `Fire` at 0.2 s): the first
    shot left 0.18 s after the pull ("spins up, first shot in 0.20 s", then the first
    "shot" line); a 3 s hold gave 57 shots at 20 a second; a pull let go 0.1 s into
    the spin-up fired nothing while the clip and the barrel ran on.
  - Parts reported for other weapons: varmint rifle `2hrattack8.kf` `##VRBolt`,
    `##VRClip`, `##VRTrigger`; 9mm pistol `##nmBar`, `##nmHammer`, `##nmTrigger`; cowboy
    repeater `2hrattack5.kf` `##BoltLock`, `##CRHammer`, `##CRLever`, `##LeverBolt`,
    `##ReloadCap`; assault carbine `2haattackloop.kf` and `2hareloadf.kf`
    `##ChargingHandle`, `##Clip`, `##CockingMech`; laser rifle `2hrreloadb.kf`
    `##LR_CellRelease`, `##LR_CellReleaseLatch`, `##LR_CellReleaseSpring`,
    `##MicroFusionCell` (its attack clip moves none).
- Not built: the colour, texture-transform and morph tracks on weapon parts; shell
  casings (`ShellCasingNode`) and the muzzle flash model (`ProjectileNode` is read
  for where shots are drawn from, §9.9); placed NPCs'
  weapons, which are still drawn as one rigid model (`nvforever::weapons`; NPCs play no
  attack or reload clips yet). The spinning melee weapons (`1hm` and `2hm`
  `attackspin`) keep combat's swing timing: no spin-up for them.
- NPCs (`nvforever::attack_anims`, since 2026-10-16): a shot from a weapon whose attack
  clip has a `Fire` and a `Loop` key starts the clip at `Fire` (NPCs do not spin up
  [design]); the shots that follow while it plays do not start it again, and it goes
  round from `Fire` to `Loop` while they come no more than 0.25 s apart, then plays out.
  Cook-Cook's flamer plays `2hhattackspin2.kf` (1.33 s on the owner's install).
  Started again at each shot, the clip pulled the arms and shoulders back to its start
  many times a second, which looked like recoil [observed in NVForever: owner's playtest
  2026-10-16]. Run (smoke `cook-cook-flamer`): 4 clip starts and 10 shots carried on
  by a running clip in 40 s.
- Where the parts rest [observed unless tagged; measured 2026-10-20 with
  `crates/fnv-nif/examples/weapon_rest_survey.rs` (each `##` node of a model: the turn
  and offset each clip gives it at its start and end against its place in the model,
  `-` where the clip has no transform track for it) on `Fallout - Meshes.bsa` and
  `Update.bsa`, both folders (`_1stperson`, `_male`), each weapon's WEAP attack and
  reload clips]:
  - 9mm pistol (`WeapNV9mmPistol`, `weapons\1handpistol\9mm.nif`, `AttackRight`,
    `ReloadL`): the draw clip `1hpequip.kf` turns `##nmHammer` from 0° to 45° about
    its z axis (between 0.10 and 0.23 s, held to the end); `1hpaim.kf` has no track for
    any `##nm…` part (neither archive's file) and `1hpaimis.kf` has only translations
    for them in first person; `1hpattackright.kf` starts the hammer at 0° and has it
    back at 45° by 0.033 s (Euler keys in first person, a B-spline in third),
    holding it to the end; `1hpreloadl.kf` holds it at 45° throughout; `1hpunequip.kf`
    brings it from 45° to 0°. The owner's screenshot of the game's 9mm in first person,
    idle with 13 of 8 rounds, shows the hammer back [observed: owner, 2026-10-20].
    Reading [hypothesis, as §9.7 reads the `Weapon` node]: a part no playing clip
    drives stays where the last clip put it, so the hammer is cocked from the draw on,
    falls as the shot leaves and is cocked again at once, and goes down as the pistol
    is put away. NVForever had the hammer forward (the model's pose) at idle.
  - The 9mm model has two nodes named `##nmClip` under the root: the first has no
    children, the second holds the magazine shape `##nmClip:0`. The reload clips'
    `##nmClip` track (54 units of travel in first person) moved only the first in
    NVForever, so the magazine never left the grip [observed: owner, 2026-10-20].
    Which of the two the game drives is not known; ours drives both.
  - .357 revolver (`WeapNV357Revolver`, `357revolver.nif`, `Attack5`, `ReloadX`): the
    aim clips (`1hpaim.kf`, `1hpaimis.kf`, both folders) hold `##357Hammer` at 45°
    (cocked); `1hpattack5.kf` goes 0° to 45°; `1hpreloadxstart.kf` lowers it to 0°
    and opens `##LoadingGate` 85°, `1hpreloadx.kf` closes the gate and cocks the
    hammer again; the draw clip cocks it (0° to 45°) as the 9mm's.
  - Cowboy Repeater (`Attack5`, `ReloadZ`) and lever-action shotgun (`Attack5`,
    `ReloadX`): the `2hr` aim clips hold `##CRHammer` at 91° and `##LASHammer` at
    61.3°; the attack and the reload clips end there; the draw clip keys neither.
  - 10mm pistol (`AttackRight`, `ReloadA`), hunting rifle (`Attack3`, `ReloadA`) and
    varmint rifle (`Attack8`, `ReloadM`): every clip starts and ends each part at its
    place in the model (`##Hammer`, `##HRBolt`, `##VRBolt` and the rest at 0° and 0
    units); the two readings agree for them.
- Engine (since 2026-10-20):
  - player (`nvforever::player_body`, both rigs): while the weapon is in the hand, each
    part the draw clip has a track for rests where that clip leaves it at its end
    (`Animator::set_extra_rest`), as the `Weapon` node does (§9.7); the others rest at
    their place in the model. Holstered, all at their place. A track drives every node
    of its name (`Animator::set_extra`). Each time the weapon comes to rest in the
    hand (no draw, attack or reload clip playing) the body logs the parts away from
    their place, and each reload clip that starts lists the parts it moves;
  - NPCs (`nvforever::weapons`, rigid models with no clips on their parts): in the
    hand, a shape under a part takes the pose of the stance's aim clip for that part,
    else the draw clip's end (`drawn_place`); holstered, the model's pose;
  - smoke `weapon-parts-rest`.

### 9.9 The muzzle: `ProjectileNode` [observed unless tagged]

Measured with `crates/fnv-nif/examples/muzzle_node_survey.rs` (every `.nif` under
`meshes\weapons\`; `--list` prints one line per file) on `Fallout - Meshes.bsa` and
`Update.bsa`, 362 models, all parsed.

- 198 of the 362 have a node named `ProjectileNode`; in 6 of them it is spelt
  `##ProjectileNode` (the four `lasergatling*.nif` hand models and the two
  `plasmacaster*.nif`, under `##GatlingLaserGun`, with a `NiTransformController`).
- By folder, with and without: `1handpistol` 65 and 3, `2handautomatic` 31 and 2,
  `2handrifle` 73 and 7, `2handhandle` 21 and 13, `2handlauncher` 7 and 2, `hand2hand`
  1 (`ballisticfistrigid.nif`) and 51; none of `1handmelee` (32), `2handmelee` (23),
  `1handgrenadethrow` (21), `1handminedrop` (7) and `modkits` (3). The 27 without one
  in the gun folders are not a gun in the hand: world models (`…go.nif`), magazines
  and cells, shells and shell casings, hoses, the BB gun's can, and projectiles (the
  missile, the mini nuke, the rifle grenade).
- Where it hangs: directly under the root in 167; else under a part of the weapon
  (`##MinigunGun` 7, `##GatlingLaserGun` 6, `##FlamerGun` 3, `##GaussRifleFront` 3,
  `##sosgStock` 2, and one each under `##MinigunBarrel`, `##RIL`, `##Gun`,
  `##CVSHStock`, `##snBarrell`, `HuntingRevolver`, `LaserPistol`, `Marksman`,
  `Pulsegun`, `TeslaCannon`).
- Where it is: 6.2 to 89.3 units from the model's origin, furthest along the model's
  x axis in 189 and along y in 9 (the shoulder launchers).
  - `weapons\1handpistol\10mmpistol.nif` (17.84, 4.05, 0.00); the same pistol with a
    silencer (`10mmpistolsilenced.nif`) (34.27, 4.05, 0.00), 16.4 units further out;
    `127mmpistol.nif` 18.69 and `127mmpistolsilencer.nif` 34.79 along x.
  - `weapons\2handhandle\minigun.nif` (79.74, −11.80, 0.00) under `##MinigunGun`;
    `weapons\2handrifle\varmintrifle.nif` (46.46, 7.00, 0.00);
    `weapons\2handlauncher\missilelauncher.nif` (−5.89, −40.53, −11.23).
- How it is turned: 196 of the 198 have a rotation of their own. In 193 the node's own
  y axis lies along the model axis the node is furthest out on, pointing away from the
  origin (the node's x, y, z along the model's z, x, y in 179). The five others:
  `1handpistol\camera.nif`, `2handhandle\25mmgrndlnchr.nif` (turned by no right angle),
  `hand2hand\ballisticfistrigid.nif` and the two plasma casters (not turned).
- Creatures (the same survey with `--under=meshes\creatures`, 246 models, 2026-10-20):
  5 have a `ProjectileNode`, all skeletons: `centaur\skeleton.nif` and
  `skeletonevolved.nif` (under `Bip01 Head`, 57.9 along y and 67.8 up), the
  `sentryturret` (under `Bip01 Pitch`, its axes the model's) and `minisentryturret`
  (under `Bip01 Pitch`) and `nvsporeplant` (under `Bip01 Orifice`). None in the
  `mistergutsy`, `nvsecuritron`, `securitron`, `robobrain`, `protectron`, `sentrybot`,
  `eyebot` or `libertyprime` folders; the Securitron's skeleton has a `ShellCasingNode`
  (under `Dummy01_MachineGun`). Where those robots' shots leave is not known.
- `ShellCasingNode`: 123 of the 362, 112 directly under the root. Ours throws the
  shell casing from it along its own +z, at the attack clip's `Eject` key where it has
  one [hypothesis for the axis; the reasons and the build are in
  docs/spec-notes/combat-rules.md §16].
- First-person models: 10 files named `1st…`, nine of them melee weapons and one
  `2handrifle\1stpersongaussrifleunique.nif` (the only one with the node). The 10mm
  pistol and the minigun use one model for both rigs [observed: the engine's "player
  weapon model" lines name the same file for the first- and third-person rig].
- A clip moves it: `characters\_male\2hhattackloop.kf` and the first-person clip of the
  same name have a transform track `##ProjectileNode` (priority 55) that starts at
  (70.60, −14.76, 0.00), the node's place in `lasergatling.nif`, and travels 6.0 units
  (5.75 in first person) with no turn. The minigun's node has no `##` and no track.
- Reading [hypothesis]: the node marks the muzzle. The projectile and the muzzle flash
  start there, and its y axis is the way the barrel points. For: its name; it sits at
  the far end of the barrel; a silencer moves it out by the silencer's length; weapons
  that fire nothing have none; a firing clip moves it with the gun's front. Not
  checked against the game.
- Engine (since 2026-10-10; docs/design/combat.md §5.1):
  - the player body finds `ProjectileNode` or `##ProjectileNode` in the weapon model
    of each rig (`nvf_assets::model::node_path`) and, each frame the weapon is in
    hand, works out where it is in the rig in view from the pose it has just made: the
    node the `Weapon` node hangs from, the `Weapon` node, then down the model's nodes,
    each where the clips have it (`nvforever::player_body`, `Rig::muzzle_at`). It
    gives that to combat as `nvf_play::combat::WeaponAnim::muzzle` (game space);
  - combat draws shots from it and uses it for the third-person cover rule; the aim
    is the view's, not the node's;
  - the PROJ's muzzle flash model is spawned under the node, so the node's rotation
    points it (`nvforever::shot_fx`; docs/spec-notes/combat-rules.md §15);
  - a weapon model without the node, a holstered weapon and a run without a player
    body (`--no-actors`) give no muzzle, and shots are drawn from the eye as before.
- Checked in ours [observed 2026-10-10, `--play` at Goodsprings, `--hour 12`, exit 0
  and "run finished" each time, `RUST_LOG=info,nvf_play::combat=debug`; screenshots
  kept privately]:
  - 10mm pistol (`0000434F`), third person: eye (−71680.0, 2044.3, 8490.4), muzzle
    (−71678.7, 2086.8, 8445.0): 42.5 units ahead of the eye and 45.4 below it. First
    person: muzzle (−71672.9, 2087.2, 8476.7), 7.1 units right of the eye, 42.9 ahead
    and 13.7 below; the tracer starts there and ends at the hit by the crosshair.
  - Minigun (`0000433F`), third person, trigger held: muzzle (−71659.5, 2128.6,
    8429.1), 20.5 units right of the eye, 84.3 ahead and 61.3 below. Held 1.2 s: "24
    shots (24 drawn from the muzzle, 0 from behind cover)", and each logged tracer
    starts at the muzzle of its shot, within 6 units of the first (the gun moves as it
    fires). Seen from the side (`orbit=75:5`) with `NVF_PLAY_TEST_TRACERS=3`, the
    tracers of a 7-shot burst fan out from the end of the barrel, with the flash
    marker on it.
  - Gatling laser (`0000432E`, the `##ProjectileNode` spelling), third person,
    trigger held 0.5 s: "ProjectileNode 2 node(s) down" for both rigs; muzzle
    (−71659.6, 2119.2, 8426.8) at the first shot; "16 shots (16 drawn from the muzzle,
    0 from behind cover)".
- Not built: NPCs' shots. The muzzle flash and the projectiles' models are in
  docs/spec-notes/combat-rules.md §15, shell casings in §16.

### 9.10 Which bone, and when: `prn:` against `Attach` / `Detach` [observed unless tagged]

Measured with `kf_pose_compare` (`KF_TRACK=Weapon` prints the `Weapon` track every
1/30 s) on `Fallout - Meshes.bsa` and `Update.bsa`, and with `fnvtool actor weapons`.

- `2hmaim.kf` is the only aim clip whose `prn:` names another bone than its equip clip:
  `prn: Bip01 L Hand` at 0.13 s, where `2hmequip.kf` says `prn: Bip01 R Hand` (0.03 s).
  Every other stance's aim clip names the equip clip's bone or has no `prn:` (the eleven
  stances of `characters\_male\`; `_female\` has no stance clips of its own).
- Its `Weapon` track is not a pose under the left hand: `2hmaim.kf` holds (3.78, 2.68,
  −13.11), 125.0° from the holster pose, which is where `2hmequip.kf` puts the node from
  `Attach` (0.30 s) to its end, and where `2hmunequip.kf`, `2hmattackright_a.kf` and
  `2hmattackpower.kf` start — clips that name no left hand.
- Rendered in NVForever [observed, screenshots kept privately, Fiend 0x000F9CE8 north of
  Goodsprings]: hung from `Bip01 L Hand` the bat lay across the body with the knob up
  and the hands in its middle (the owner's report, playtest 2026-10-16: "held the wrong
  way round"); hung from `Bip01 R Hand` with the same track, both hands are on the handle
  with the knob below the left hand, at the ready, walking and through `2hmattackright_a`.
  Reading [hypothesis]: the aim clip's `prn:` key does not move the weapon (what it is
  for is not known); the weapon stays on the bone the equip clip named.
- The `Weapon` track of the equip and unequip clips changes between the holster pose and
  the hand pose in one step, exactly at `Attach` / `Detach`, and is constant on either
  side: `2hmequip.kf` (16.98, −12.08, 4.45) until 0.267 s, the hand pose from 0.300 s
  (`Attach` 0.30); `2hmunequip.kf` the hand pose until 0.833 s, the holster pose from
  0.867 s (`Detach` 0.87); `1hmequip.kf` from (10.11, 7.04, 12.96) to (6.96, 2.26, 1.43)
  at 0.30 s (`Attach` 0.30); `1hmunequip.kf` back at 0.70 s (`Detach` 0.70). The holster
  pose is the holster clip's (`2hmholster.kf`: (16.98, −12.08, 4.45)).
- The `prn:` keys of those clips sit elsewhere, before or after `Attach` / `Detach`
  (seconds): `1hm` equip 0.03 / `Attach` 0.30, unequip `Detach` 0.70 / 1.07; `1hp` 0.13 /
  0.17, 0.20 / 0.23; `2hm` 0.03 / 0.30, 0.03 / 0.87; `2hr` `Attach` 0.30 / 0.63, `Detach`
  0.47 / 0.80; `2hl` 0.47 / 0.67, 0.67 / 1.97; `2hh` 0.50 / 0.97 both; `h2h` 0.03 / 0.13,
  0.03 / 0.17. Reading [hypothesis]: the `prn:` key names the bone and `Attach` /
  `Detach` is the moment; the track is authored against the bone it changes to there.
- Engine (`fnv_actor::weapon`): the drawn bone is the equip clip's `prn:` (else the aim
  clip's), and the weapon changes bones at `Attach` / `Detach` (`ClipFacts::switch`),
  for NPCs as for the player body (`player_clips::HandEvent`).

### 9.11 The Land Mine weapons in the first-person hand [observed unless tagged]

Sources: the owner's first-person frames of the frag mine (WEAP 0x0000433C, animation
type 11 "Land Mine") drawn in the original game, level and looking down (2026-10-23, kept
privately in `C:\fnv-private\references\mines`); `_1stperson\1lmaim.kf`, `1lmequip.kf`,
`1lmhandgrip1.kf`; NifSkope 2.0 Dev 7 playing `1lmaim.kf` on `_1stperson\skeleton.nif`
with `minefrag.nif` under `Weapon`.

- In the game the mine's ringed top faces up and back towards the view, tilted back
  about 40–50°, with its wire handle at the right in the hand and the thumb over the
  rim; the left hand is not in view. Only the dark lens (`##LightOff:0`) shows: no glow
  (`##Glow:0`) and no lit lens (`##LightOn:0`), which a live placed mine shows
  (combat-rules.md §11a).
- The clips alone leave the mine on its edge, underside to the view, handle up: NVForever
  posed `1lmaim.kf` bone for bone as the file says (every arm bone's rotation checked
  against the track values), and NifSkope shows the same. The `Weapon` track of every
  `1lm` clip holds the pistol's rotation (about 88° about the hand's x axis, as in
  `1hpaim.kf` and the skeleton's own `Weapon` node); what turns `1lm` apart is the hand,
  which `1lmaim.kf` rolls about 90° from the pistol's.
- `1lmhandgrip1.kf` (the type's only grip clip) drives no right-arm bone: it poses the
  left arm (priority 45, as the aim), the fingers and thumbs (56) and `Weapon` (45, the
  aim's value). Played over the aim it takes the left hand out of view and puts the right
  thumb over the rim, as in the game's frames. The frag mine's WEAP hand grip is the
  default (DNAM byte 13 = 0xFF) [hypothesis: the default grip plays `handgrip1` where the
  stance has one; not checked for the grenades' `1gthandgrip1–3`].
- The turn the game adds is not in any file read here [hypothesis: an engine rule for
  these weapons]. NVForever turns the `Weapon` node 240° about the hand's x axis and
  moves it (4.4, −3.9, 4.5) in the hand's frame (`player_body::land_mine_grip`): fitted
  by eye to the level frame, where the lens lands within about 30 pixels of the game's at
  1280×720. Not matched: looking down, the game's mine rises towards the middle of the
  view while ours stays where it is (the arms rig turns with the view, §9.5).
- The plasma (0x433D), pulse (0x433E) and powder charge (0x109A0B) mines are also
  "Land Mine" and get the same; they were not compared with the game.

### 9.12 Rotation that adds up: `BSRotAccumTransfInterpolator` [observed unless tagged]

Sources: `fnvtool nif stats` over every mesh archive of the base game and the add-ons
(block types per file), `fnvtool nif decode` of the files named below.

- The block is an NiTransformInterpolator with no fields of its own [documented:
  nif.xml, schemas/nif/anim.json]. 21 `.kf` files in `Fallout - Meshes.bsa` use it, none
  in `Update.bsa` or the add-on archives: 17 of Anchorage's chimera tank (no creature
  of FalloutNV.esm uses that skeleton, actors.md §8.11's table), the first-person
  `_1stperson\2hmattackspin.kf` and `2hmattackspin2.kf` and the third-person
  `_male\idleanims\vats2hmattackpower_attackspin.kf` (node `##SawBlade`), and the
  Securitron's walk `creatures\nvsecuritron\locomotion\mtforward.kf` (node `Bip01
  Wheel`). Until 2026-11-09 NVForever played none of them [design:
  `fnv_nif::anim::transform_track` had no case for the type, so its tracks became
  `TrackData::Other` and were skipped], so a walking Securitron's wheel stood still
  (Victor's, seen in a playthrough of NVForever).
- **The Securitron's wheel.** `mtforward.kf` (Forward, 2.67 s, loops) keys `Bip01 Wheel`
  with 81 quaternion keys a frame apart. Each key is the first key followed by a turn
  about the node's own X axis (`conj(first) · key` is a pure X turn at every key), 8.2–8.3°
  a frame, 661° backwards over the cycle; so the last key is 59° on from the first, not
  back where it started. The run (`mtfastforward.kf`), `mtbackward.kf`, the turns and every
  limping clip key the wheel with ordinary interpolators (B-spline, or 81 plain
  quaternion keys sweeping 773° in the limping walk and run), interpolators NVForever
  already played [design].
  At the walk's ground speed (120.2 units a second, actors.md §8.10) 661° in 2.67 s is a
  wheel about 28 units in radius rolling without slipping.
- **The saw blade.** The three `attackspin` clips are played once (cycle type 2, clamp)
  and key `##SawBlade` with Euler XYZ keys: Z from 0 to −53.8 radians (about 8.5 turns)
  over the first person's 1.53 s clips. Euler angles are interpolated angle by angle, so
  the turns come out whole within the one play.
- **What NVForever does** [design, `crates/fnv-nif/src/anim.rs`,
  `engine/nvf-assets/src/anim_play.rs`, `anim_blend.rs`; hypothesis for the adding up:
  from the name and from the wheel's keys, whose last key is where a further cycle would
  go on from without a seam; not compared with the original]: it reads the block as an NiTransformInterpolator and
  keeps the turn one cycle makes, `conj(first key) · last key`
  (`fnv_nif::anim::TransformTrack::per_cycle`). A looping clip that has played `n`
  whole cycles gives the keyed rotation followed by that turn `n` times
  (`TransformTrack::rotation_after`), so the wheel keeps rolling instead of jumping back
  59° every 2.67 s. Actors and creatures (`anim_play::BlendPlayer`) count a slot's cycles
  from its phase wrapping (a clip moved to another slot, as the walk is when it fades
  out, keeps its count; a clip new to every slot counts from none); the player's body
  (`anim_blend::Animator`) counts them from a looping layer's time; clamped clips count
  none, which is all the saw blade needs.
- Not handled [design]: `BSTreadTransfInterpolator` (the Ripper's chain in `1hmidle.kf` and
  `sneak1hmidle.kf`, Liberty Prime's `1gtequip.kf`), whose formula is not known
  (nif-engine-gap-audit.md §3.4).

### 9.5 The first-person rig [observed unless tagged]

- `meshes\characters\_1stperson\skeleton.nif` is the only non-`.kf` file of its folder
  (426 `.kf` in `Fallout - Meshes.bsa`, 9 more in `Update.bsa`). Its tree: `Scene Root` →
  `Bip01` → `Bip01 NonAccum` → `Bip01 Looking` (translation 0, 0, 118) → `Bip01 Translate`
  (0, 0, −118) → `Bip01 Rotate` → `Bip` (0, 0.66, 67.77, the third-person `Bip01`'s offset)
  → pelvis, spine, arms (with extra `Bip01 L/R Hand01`–`04` finger roots), legs; and
  `Camera1st` under `Bip01 Looking` at (0, −3.74, 0) — 118 units above the feet, against
  the third-person skeleton's `Camera3rd` at (0, −3.74, 118.36). `Weapon` hangs from
  `Bip01 R Hand` at the same offset as in the third-person skeleton.
- Layout: `mtidle.kf`, `sneakmt*.kf`, `swimmt*.kf`, `mtjump*.kf` and `mtturn*.kf` sit in
  the folder itself; unarmed movement in `locomotion\mt{,fast}<dir>.kf` (no `male\`/
  `female\`); weapon movement only for `1hp` and `1hm`; per weapon `aim`, `aimis` (no
  `aimup`/`aimdown`), `equip`, `unequip`, `holster`, attacks, reloads, jams.
- Tracks: `mtidle.kf` drives `Camera1st`, `Bip` and the arms (priority 20) and the
  fingers (5); `locomotion\mtforward.kf` only `Bip01` (102.3 units over 1.2 s, as the
  third-person clip) and `Bip01 NonAccum`; `sneakmtidle.kf` and `sneakmtforward.kf` put
  `Bip01 NonAccum` at (0, 0, −40) (priority 28 and 30); `1hpaim.kf` drives `Bip`, the
  spine, the arms and `Weapon` (45), `Camera1st` and `Bip01 NonAccum` (25);
  `1hpequip.kf` arms and `Camera1st` (55), `Attach` at 0.167 s as in third person;
  `1hpholster.kf` has `Prn: Bip01 Translate`; `2hraimis.kf` the arms at 54. No clip
  drives `Bip01 Looking`.
- Readings [hypothesis]: the game turns `Bip01 Looking` with the view's pitch (it is the
  only node at camera height between the root and the arms, and `Bip01 Translate` undoes
  its offset), so arms and weapon stay in front of the camera at `Camera1st`; crouching
  lowers the whole rig and the camera by 40 (to 78); a holstered weapon hangs from
  `Bip01 Translate`, out of view.
- Parts: the hands have first-person meshes beside the third-person ones
  (`characters\_male\lefthand1st.nif`, `righthand1st.nif`, `lefthandpipboyglove1st.nif`,
  `femalelefthand1st.nif`, `femalerighthand1st.nif`, `femalelefthandpipboyglove1st.nif`).
  Upper-body models name their sleeves `Arms…`: `upperbody.nif` `Arms:0`, `Arms:1`;
  `vaultsuitarmor\m\outfitm.nif` `UpperBody`, `Arms`, `PipBoyOn`/`PipBoyOff`, `Arms\t`,
  `Upperbody`, caps; `leatherarmor\m\outfitm.nif` `Arms`, `Arms02`, `Upperbody`, caps;
  `raiderarmor01\m\outfit.nif` `Arms`, `UpperBody`, `Arms2`, caps;
  `powerarmor\powerarmorcomplete.nif` `Arms`, `UpperBody\t`, `buttcover`, caps;
  `leatherarmor\m\mark2leather.nif` `Arms01:0`, `body:0`, `upperbody:0`, `Armor:0`, caps.
  The owner states that the first-person view draws only shapes called `Arms`
  (2026-10-09); `fnv_actor::first_person` follows that for the upper body and draws hand
  models whole.
- Weapons: WEAP `WNAM` "1st Person Model" names a STAT [documented: xEdit]; the 9mm
  pistol's is `1stPerson9mm` (0x000E7653), whose model is the same `9mm.NIF` with
  texture swaps (`MODS`).
