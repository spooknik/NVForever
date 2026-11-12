# Cells and references from the ESM (ESM gap audit, 2026-10-22)

The ESM gap audit's medium group "Cells and references"
([esm-gap-audit-2026-10-21](../handoff/esm-gap-audit-2026-10-21.md)) lists CELL, REFR,
ACTI and TERM fields that were decoded but not acted on. Activate parents (XAPD/XAPR)
were built before (#547, traps.md). Bit meanings: GECK wiki GetFormFlag (record header
flags, adapted from FNVEdit) and GetCellFlag [documented]; layouts from xEdit (schemas
`world.json`, `common.json`, `objects.json`). Counts are from FalloutNV.esm [observed: a
raw read of the master, 2026-10-22]: 30,497 cells and 307,710 placed references.

## 1. Can't Wait (CELL record header bit 19)

Nine interior cells: `Gomorrah00`, `Gomorrah01`, `HooverDamExt`, `HiddenValleyBunker1`,
`ULMembersOnly`, `OVWestSewers03`, `LegateCampTent`, `FreesideAtomicWrangler2`,
`FollowersOutpostInterior`. The Wait menu (and sleeping) refuses in such a cell with the
line a script's ban gives, "You cannot wait here" (`Refusal::Forbidden`, wait-menu.md)
[hypothesis: which line]. It is checked after the script's ban, swimming, falling and
radiation, and before trespass (wait-menu.md §9 order). Logged: `wait menu: refused: …
is flagged Can't Wait`.

## 2. Inaccessible doors (REFR record header bit 8)

85 door references (and 47 others, where bit 8 has other meanings for other record
types) carry it; many have an Activation Prompt such as "Inaccessible Tunnel". Such a
door is shown with its name and the action "Inaccessible", and E does nothing: no
teleport, no opening, no script [hypothesis: the flag's name; GetLockedOut's value 3
"Inaccessible" (JIP) [documented] says such a door is locked out]. Scripts may still
activate it.

**Doors without a name** (no FULL, or a blank one) give no prompt at all, as unnamed
activators do, when the reference also has no teleport destination (XTEL) and no Activation
Prompt (XATO): the crosshair passes them by and E does nothing to them. A nameless door that
has either stays offered (a travel door shows its destination; a prompted one its prompt), so
a door a quest sends the player through is never hidden. FalloutNV.esm has 8 nameless DOOR
bases of 320: NVInvisibleDoorNPCOnly, VHDTowerHatchNoUse, OffDoorHidden01 (Mick & Ralph's back
room), CG04SecretDoor, FortArenaGate, REPCONHatch2, and two never placed [observed, #1598].
Which of their placed references have an XTEL or XATO is not yet checked against the data
(#1627) [hypothesis: OffDoorHidden01's has neither, so it stays hidden]. The rule itself is a
hypothesis (that the original's prompt needs a name); scripts may still activate them.

## 3. Activation prompts (XATO)

156 references (doors mostly: "Exit to Mojave Wasteland" 24, "Climb Ladder to Mojave
Wasteland" 11, "Call Elevator" 10 …) and 59 activators ("Play Roulette", "Play Slots",
"Read Note", "Pick Banana Yucca" …) carry one. The JohnnyGuitar function
GetRefActivationPromptOverride names it the reference's "Activation Prompt Override"
[documented]. Under the crosshair the prompt's action line shows the reference's XATO,
else its ACTI base's, in place of the action word ("Open", "Activate"); the second line
(the name, or a door's destination) stays [hypothesis: the override replaces the verb].
It does not replace a lock's or key's line, nor an inaccessible door's.

**A nameless activator with a prompt is offered.** An activator, talking activator or
furniture with no FULL gives no prompt (it is taken for a trigger or marker), unless its
reference or base has an XATO: then the crosshair takes it and shows the prompt alone, with
no name line [hypothesis: that the original shows no second line]. The case that needs it:
VHDMainControlPanel1REF (0x00153FCE, HooverDamIntPowerPlant01b), a reference of the talking
activator VHDControlPanelTalkActivator (0x0015423A, no FULL) with the XATO "Activate"; its
script VHDControlOverrideBoxSCRIPT shows the override-chip message from OnActivate, so
without the prompt the Yes Man and House endings stop there [observed in the records;
owner's playtest 2026-11-11, #2160]. Primitives stay unoffered whatever they carry.

## 4. Nothing to build

- **Off Limits** (CELL header bit 17, "Dangerous / Off limits (Interior cell)"
  [documented: GetFormFlag]): one cell, `ULPenthouse`. The wiki does not say what the
  flag does; ownership and trespass already decide the penthouse. Stored as before.
- **Behave Like Exterior** (CELL DATA bit 7) and **XCCM** (an interior's climate): no
  cell in FalloutNV.esm uses either; the add-ons have one cell each [observed: `fnvtool
  esm fields` over the ten masters, esm-audit-2026-10-25 §4]. Neither is read.
- **Leveled Lock** (XLOC flag bit 2): none of FalloutNV.esm's 426 locked references has
  it, but the add-ons do: Dead Money 22, Old World Blues 3, Honest Hearts 1 [observed:
  docs/handoff/esm-full-audit-2026-10-24.md gap 24]. No source gives the rule; since
  2026-10-24 a leveled lock hardens from a level fixed for its reference, so a door keeps
  its authored level (simulation.md §5; encounter-zones.md) [hypothesis].
- **TERM Leveled** (DNAM flags bit 0): eleven terminals. "If the terminal is leveled, the
  actual lock level may be adjusted" [documented: GECK wiki Lock] gives no rule, and no
  game setting does either; hacking.md §6 keeps the stored difficulty.

## 5. Checks

Unit tests: `crime::tests::cell_flags_from_raw` (Can't Wait), nvf-play `activate::tests`
(an inaccessible door, a reference's and an activator's prompt). Smoke: `wait-cant-wait`
(the Wait menu refused in Gomorrah), `activation-prompt-door` (the prompt of a Goodsprings
door with XATO), `door-behind-wall-face` (below). No smoke for §2: the two flagged doors
tried (the Prospector Saloon's 0x00108BC8, the town hall's interior 0x00131E6B) were never
under the crosshair in ours [observed, 2026-10-22]; whether the original lets the player
target them is a capture question.

**Doors behind a building's face.** Some buildings' collision has a face a few units in
front of their door: the Nipton town hall's mesh (0x0013D7E4) lies 8.5 units before the
slab of its outside door (0x00130CFF), so the crosshair ray stopped on the building and the
door could never be used [observed 2026-11-01, #1339]. When the ray's first hit cannot be
activated, a door within 16 units behind it is the target (`pick::door_behind`) [design:
the reach; the original's picking rule is not known]. Only doors are looked for, so items
behind a wall stay out of reach. This may also be why the 2026-10-22 doors above were never
targeted; not retried.

**Invisible walls do not take the crosshair.** The activation ray passes collision on
layer 27, Invisible Wall, as it passes trigger volumes; what stands behind such a wall is
the target. The Gun Runners' kiosk (`nv_gunrun_kiosk.nif`, one static body on layer 27,
306 vertices) stood between the crosshair and the Vendortron inside it (GunRunnerProtectron,
VendortronRef 0x000F2EAA), so it could not be talked to [observed: fnvtool collide objects,
#1602]. [hypothesis: by the layer's name; sight and blast cover pass the layer too.] Shots
and the player still stop at it, and PlaceAtReticle's far ray stops at it.
