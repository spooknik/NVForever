# Unlisted JIP LN commands that mod lists call (2026-11-13, #2275 follow-up)

The installed JIP LN NVSE 57.30 registers commands that have no GECK wiki page, so the
catalogue (`schemas/script/functions.json`) had no row for them and a mod's call compiled
to nothing ("no entry"). The owner's two mod lists, Blue Moon (MO2 `D:\Modlists\ABM`,
profile "1 - A Blue Moon 2.0") and High and Dry (`D:\Modlists\HighDry`, profile
"1 - Normal"), call 33 of them. This note says what each does in the original game, in
our words, and what NVForever does. Code: `crates/fnv-game/src/nvse_unlisted.rs`; the
catalogue rows come from `PAGELESS` in `tools/capture/script_functions.py`.

## 1. Sources and how they were found

- **Names and opcodes**: `nvse.log` of the original game with xNVSE 6.4.9 and the mod
  lists' plugins (each list's `stock game` folder holds a copy). Each `RegisterCommand`
  line gives the opcode, the name and an alias. The plugin is known from the log's order:
  JIP LN's block runs from its listener line to JohnnyGuitar's [observed: nvse.log,
  2026-11-13]. 241 registered names (aliases counted with their command) had no catalogue
  row; the 33 here are the ones the two lists' scripts call. The opcodes are below; the
  catalogue rows carry none, as with the other pageless rows (calls go by name).
- **Argument shapes**: the mods' own script source (SCTX), read locally. Only names and
  counts go in the repository.
- **Behaviour**: probes in the original game, run unattended by `nvf_capture` (plugin
  version 10) in the Blue Moon sandbox's `stock game` folder, save `nvftest` (the
  Goodsprings gas station). The sets are `cmd1`, `cmd2` and `cmd3` in
  `tools/capture/jip_probes.py`, whose rows are cited below as `cmdN.label`. The goldens
  stay in `C:\fnv-private`.
- **Not read**: no extender DLL or source, and no nv-rs.

| Command | Opcode | Called by (units, both lists) |
|---|---|---|
| ListToArray | 0x2212 | 1 |
| CCCOnLoad, CCCSetFloat, CCCSetString, CCCSetTrait, CCCGetDistance | 0x26FD-0x2701 | 1, 9, 4, 3, 1 |
| CCCSetEquipped, CCCInFaction, CCCSetNCCS, CCCLoadNCCS, CCCSavedForm | 0x2703-0x2708 | 2, 9, 2, 1, 5 |
| CCCLocationName, CCCGetReputation | 0x2709, 0x270A | 3, 1 |
| CCCSayTo, CCCRunResultScripts | 0x2752, 0x2753 | 1, 1 |
| GetTeammateUsingAmmo, SetTeammateUsingAmmo, ToggleDetectionFix | 0x275A-0x275C | 0, 3, 3 |
| ToggleIgnoreLockedDoors, CCCSetFollowState | 0x275F, 0x2760 | 1, 3 |
| RefToPosStr, MoveToPosStr, LockEquipment, GetCenterPos, CCCSMS | 0x2762, 0x2763, 0x2768, 0x276F, 0x2771 | 2, 2, 3, 1, 3 |
| MoveAwayFromPlayer, KillActorAlt, DisableScriptedActivate | 0x2781, 0x280C, 0x282B | 1, 1, 1 |
| AttachLight, RemoveLight | 0x2879, 0x287A | 37, 36 |
| GetConditionDamagePenalty, GetHitNode, RefreshAnimData | 0x28C0, 0x28D2, 0x290C | 1, 1, 7 |

GetTeammateUsingAmmo is called by no script; it is built with its setter.

## 2. Actors

### 2.1 AttachLight, RemoveLight

`ref.AttachLight node light x y z` puts a light on a node of the reference's drawn model:

- 1 when the node exists and the form is a light; 0 for a node the model does not have,
  or a form that is not a light [observed: cmd1.light.attach, light.attach.nonode,
  light.attach.notlight; cmd2.light.attach.other]. A second call on the same node also
  gives 1 [observed: cmd1.light.attach2].
- The light is a new child node of that node [observed: cmd2.light.children: "Bip01
  Head" had 13 children before and 14 after, the new one named `PtLight` and the light's
  FormID in hex]. The offset is in that node's own space [hypothesis].
- `ref.RemoveLight node` takes that node's light away and gives 0 whether or not there was
  one [observed: cmd1.light.remove, remove2; cmd2.light.children.after back to 13].

NVForever: the node goes into the script's NIF mirror at once (`Game::attach_light_now`),
so JIP's NIF block functions see it, and the engine draws it as a point light with the
LIGH's colour, brightness and radius, as a muzzle flash's light is drawn
(`engine/nvforever/src/nif_mirror.rs`, `SceneEdit::Light`). One light per node: a second
AttachLight on the node replaces the first [hypothesis: the probe did not count the
children after two attaches]. The lights last for the session [hypothesis].

### 2.2 LockEquipment

`actor.LockEquipment 0|1` answers 0 [observed: cmd1.lockequip.on, off]. It does not stop
a script: while it holds, EquipItem still equips (GetEquipped reads 0 in the same frame
and 1 ten frames later) and UnequipItem still unequips [observed: cmd4.equip.locked,
equip.locked.after, unequip.locked.after, in both saves]. (cmd1's lockequip.equipped read
0 one frame after EquipItem in the gas station's save, 1 in the airport's: the equip was
still under way.) The CCC mod sets it from its "managed gear" option, so it probably holds
back the actor's own choices of what to wear or wield [hypothesis]. NVForever keeps the
switch per actor (`Game::equipment_locked`) and changes nothing else until that is seen.

### 2.3 GetTeammateUsingAmmo, SetTeammateUsingAmmo

Every actor reads 1 until a script sets it; the setter answers 0 and the getter then reads
what was set [observed: cmd1.teamammo.*]. The name and the CCC mod's own setting ("infinite
ammo") say 0 means a teammate fires without spending ammunition [hypothesis]. NPCs in
NVForever fire without ammunition anyway (owner decision 2026-10-17,
`engine/nvforever/src/npc_combat.rs`), so the flag is kept and read back, nothing more.

### 2.4 KillActorAlt

`actor.KillActorAlt [killer]` kills the actor, the killer recorded, and answers 0
[observed: cmd1.kill.alt, kill.dead 1, kill.killer the player]. How it differs from
KillActor was not seen. NVForever kills as KillActor does, without its essential-actor
knock-out [hypothesis].

### 2.5 MoveAwayFromPlayer, RefreshAnimData

Both answer 0 [observed: cmd1.moveaway, cmd1.refreshanim.*]. After MoveAwayFromPlayer a
placed Doc Mitchell came no farther from the player in about 4 s; he walked as his package
took him (206, then 125 and 134 units) [observed: cmd3.moveaway.*]. NVForever: no move,
and nothing to refresh (the engine reads animation data as it plays).

### 2.6 GetHitNode

A string: `NULL` for an actor no hit has struck [observed: cmd1.hitnode.doc]. A hit's node
was not probed. NVForever: the body part node (BPNN) of the last hit's location in a
creature's body-part data, else `NULL` [hypothesis].

## 3. References

### 3.1 GetCenterPos

`ref.GetCenterPos X|Y|Z`: the centre of the reference's bounds. A footlocker whose OBND
runs from 0 to 23 in z read 11.5 above its position, 0 in x and y; a crate 18; Doc Mitchell
and the player 66.2 [observed: cmd1.centerpos.*, cmd2.crates.center]. NVForever: the
position plus the centre of the base's OBND, its x/y part turned by the heading
[hypothesis: the symmetric boxes probed could not show the turn].

### 3.2 RefToPosStr, MoveToPosStr

RefToPosStr gives the interior cell's FormID, or outdoors the worldspace's, in hex
without leading zeros, then X and Y as whole numbers cut toward zero and Z as its whole
part plus 1: the player at 500.145 -5789.315 7552.582 in the gas station read
`109038 500 -5789 7553`, Doc Mitchell at 619.839 -5883.541 7552.355 read `619 -5883 7553`,
and outdoors at the airport 37000.297 -117159.648 6057.049 read `DA726 37000 -117159 6058`
[observed: cmd4.posstr.*, position and text in one frame]; a crate put at z 5000 read 5001
[observed: cmd3.crate.bare.pos]. The Z of a negative height was not seen [hypothesis: the
same rule]. A reference with no cell gives an empty string [observed: cmd3.crate.bare.pos
in the gas station's save].

MoveToPosStr takes that text back: 1 once moved [observed: cmd2.movetopos.literal, the
crate at 100 200 300]. The numbers are read as whole numbers cut toward zero: `10.7 -20.2
30.9` put the crate at 10 -20 30 and `-10.7 -20.9 -30.5` at -10 -20 -30 [observed:
cmd4.movetopos.frac, movetopos.neg]. The FormID may be lower case or have leading zeros,
and words after the three numbers are ignored [observed: cmd4.movetopos.lower, zeros,
extra]. It gives 0 when the first word is not a cell or worldspace or fewer than three
numbers follow [observed: cmd3.crate.badform, crate.short, cmd1.movetopos.bad]. Moved out
of an interior to a worldspace position, the original left the crate with no cell at all
[observed: cmd3.crate.goodsprings.cell, crate.bare.cell 00000000 in the gas station's
save]; from outdoors the same move worked [observed: the airport's save, cell 000DEB16].
NVForever puts the reference in the exterior cell at that position either way [decision:
a reference with no cell helps no script].

### 3.3 DisableScriptedActivate

Answers 0 for 1 and 0 [observed: cmd1.scriptedact.*]. What it stops was not probed (the
name says the reference's OnActivate block [hypothesis]). NVForever keeps the switch only.

### 3.4 GetConditionDamagePenalty

0.67 at every difficulty, and no command sets it [observed: cmd1.condpenalty,
cmd3.condpenalty.0-4]. No game setting holds 0.67 [observed: FalloutNV.esm GMSTs].

### 3.5 ListToArray

`ListToArray list`: an array of the list's forms in list order; a form that is not a list
gives no array (ar_Size -1) [observed: cmd2.listtoarray, 18 of NVCompanionWeapons' 18,
first and last as ListGetNthForm; listtoarray.notlist].

## 4. Switches kept without an effect

ToggleDetectionFix and ToggleIgnoreLockedDoors answer 0 [observed: cmd1.detfix.*,
lockeddoors.*]. Their effects (a detection fix, pathing through locked doors) cannot be read
back by a script and were not measured. NVForever keeps each switch per reference and
changes nothing else.

## 5. Companions Command & Control

These belong to the JIP Companions Command & Control mod, built into JIP LN. The probes ran
without that mod's plugin: a load order of the sandbox's own (a private `LOCALAPPDATA`
plugins.txt) did not load it (GetModIndex 255), and the game's real plugins.txt belongs
to the owner's own install. What they do with the mod loaded is therefore open.

### 5.1 CCCGetDistance

`ref.CCCGetDistance target mode`: mode 4 is the height between the two (the same from
either end), every other mode the distance across the ground. Two crates 300 400 100
apart read 500 in modes 0-3 and 5-7 and 104 in mode 4 (the crates had settled a little)
[observed: cmd2.crates.modes, crates.modes.back, doc.modes].

### 5.2 CCCInFaction

`ref.CCCInFaction faction [withBase]`: 1 when a script gave the reference a rank of 0 or
more in the faction (AddToFaction, SetFactionRank), 0 at -1. The factions the base's record
lists count only with the second argument non-zero: Doc Mitchell, whose NPC_ record lists
GoodspringsFaction at rank 0, read 0 and with `1` read 1 [observed: cmd1.infac.*,
cmd2.infac.*].

### 5.3 CCCLocationName

A string naming where the reference is: indoors the cell's name with one word before it
(`Goodsprings Gas Station`), outdoors the map name of the cell's regions with another
(`Searchlight Airport`; `Goodsprings`, the RDMP of VMapGoodspringsRegion), outdoors in a
cell no map region names a compass direction and the nearest map marker (`Legate's
Camp`), and a phrase of its own for a reference with no cell [observed: cmd1.locname.* in
both saves, cmd2.locname.pete, cmd3.crate.*.name]. The words are the original's and in no
data file, so NVForever writes its own (CLAUDE.md, strings): "inside" and "around" before
the name, "near" before the nearest map marker (without a direction), "somewhere unknown"
with no cell.

### 5.4 CCCGetReputation

The reputation (REPU) of the first of the base's factions that counts towards one: Doc
Mitchell and Easy Pete both gave RepNVGoodsprings; the player and a crate 0 [observed:
cmd2.rep.*]. On a base form instead of a reference the original returned the form itself
[observed: cmd2.rep.boone]; NVForever reads the base's factions there too.

### 5.5 The CCC mod's HUD and orders

CCCSavedForm gives no form without the mod [observed: cmd1.savedform.0-9]. CCCSetFloat,
CCCSetString, CCCSetTrait, CCCSetEquipped, CCCSetNCCS, CCCSMS, CCCSetFollowState, CCCSayTo
and CCCRunResultScripts answer 0 [observed: cmd2.setfloat … runresult]. The mod's scripts
pass them its HUD settings by index and its orders to the companions (its HUD file
`cccHUD.xml` has traits such as `_NumHired` and `_Layout` that match the variables passed
[hypothesis]). CCCLoadNCCS and CCCOnLoad were not called (they may expect the mod's data).
NVForever answers 0 and changes nothing [decision: until the mod's plugin can be loaded in
the original, see §6].

## 6. Left open

- The CCC functions with the mod loaded (§5.5): needs the mod in the original's load
  order. The next step is an observer check on a game whose load order has the mod.
- LockEquipment's reach beyond EquipItem, MoveAwayFromPlayer's move, KillActorAlt against
  KillActor, GetHitNode after a hit, DisableScriptedActivate, ToggleDetectionFix and
  ToggleIgnoreLockedDoors's effects.
- GetContainerOpenSound and GetContainerCloseSound (also called, by Just Assorted Mods) are
  probed in cmd1/cmd2 (`opensound.*`, `closesound.*`: the base's SNAM and QNAM, from the
  reference or the base form; SetContainerOpenSound on a reference changes the base) but
  not built here: an outside contributor's pull request (#2300) covers them.
- The other 208 registered commands with no row (§1) are not called by either list.
