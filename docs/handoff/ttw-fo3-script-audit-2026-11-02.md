# TTW and Fallout 3 script audit, 2026-11-02

What the scripts of Tale of Two Wastelands 3.4 call that NVForever does not run yet. Base
New Vegas has no stubs left (`tools/smoke/expected/script-stubs.toml`), but that audit
covers the ten New Vegas masters only; this one covers the eighteen TTW masters
[observed: fnvtool built from main of 2026-11-02, nothing built for the audit].

Load order (TTW's own `plugins.txt`, read-only): FalloutNV, DeadMoney, HonestHearts,
OldWorldBlues, LonesomeRoad, GunRunnersArsenal, Fallout3, Anchorage, ThePitt,
BrokenSteel, PointLookout, Zeta, CaravanPack, ClassicPack, MercenaryPack, TribalPack,
TaleOfTwoWastelands, YUPTTW (all `.esm`).

```
fnvtool script coverage  <TTW Data> <the 18 masters> --schema schemas/fnv
fnvtool script stubs     <TTW Data> <the 18 masters> --schema schemas/fnv
fnvtool script decompile <TTW Data> <the 18 masters> --schema schemas/fnv
fnvtool ext scan         <TTW Data> <the 18 masters> --schema schemas/fnv
```

## Totals [observed]

- 36,190 script units, 166,867 calls, 566 different opcodes (the ten New Vegas masters
  alone: 401 opcodes, 67,957 calls).
- `script stubs` reports 0 stubs and 3 unregistered functions (GetPCIsRace,
  GetLastPlayerAction, GetIsObjectType). It undercounts here: it only lists catalogue
  functions, so opcodes the New Vegas catalogue never saw used do not show.
- `script coverage` marks 164 opcodes "not implemented", but its `implemented()` is the
  vanilla registry only; extender functions are dispatched by name (`nvse*.rs`). Checked by
  name against main, 110 of the 164 are built; 54 (about 437 calls) are not.
- `ext scan`: 328 units use the extender (TaleOfTwoWastelands 313, YUPTTW 13, Fallout3 2);
  every one has source text, so NVForever compiles them from source. 7 units have no
  source; none of them uses the extender.
- Counts include Fallout3.esm units and TaleOfTwoWastelands.esm's overrides of the same
  units, so many vanilla counts are roughly double the distinct uses.

## 1. Vanilla functions only Fallout 3 calls (26) [observed]

Registered in the catalogue (GECK1, opcode recorded), never called by the New Vegas
masters, absent from `functions/mod.rs`.

| Opcode | Function | Calls | Where |
|---|---|---|---|
| 0x1053 | ModDisposition | 84 | Fallout3, TTW, BrokenSteel |
| 0x1082 | GetPCIsRace | 32 | Fallout3, BrokenSteel, TTW |
| 0x10D2 | GetForceSneak | 24 | Fallout3, TTW |
| 0x11B7 | SetMerchantContainer | 24 | Fallout3, TTW (vendors) |
| 0x11E5 | MatchRace | 20 | **character creation** (Vault101ExitScript: Dad matches the player) |
| 0x111B | SetCellFullName | 18 | Fallout3, TTW, YUPTTW |
| 0x1200 | SetLocationSpecificLoadScreensOnly | 16 | **character creation** (CG01) |
| 0x11E6 | SetPCYoung | 14 | **character creation** (CG01) |
| 0x11FC | SetNoActivationSound | 12 | **character creation** (CG01), BrokenSteel, Zeta |
| 0x1230 | SetLastExtDoorActivated | 11 | Fallout3, Anchorage, BrokenSteel, TTW |
| 0x1144 | TriggerHitShader | 10 | mostly Anchorage |
| 0x11DF | MatchFaceGeometry | 10 | **character creation** (Vault101ExitScript) |
| 0x1127 | ResetInterior | 8 | mostly ThePitt |
| 0x11FD | ClearNoActivationSound | 6 | Fallout3, BrokenSteel, Zeta |
| 0x1227 | FadeSFX | 6 | Fallout3, BrokenSteel |
| 0x10A8 | SetFactionReaction | 5 | Fallout3, PointLookout |
| 0x11EA | SetZoneRespawns | 5 | mostly BrokenSteel |
| 0x10BC | GetPCSleepHours | 4 | Fallout3 |
| 0x11CA | ClearActorsFactionsPlayerEnemyFlag | 4 | Fallout3, Anchorage |
| 0x1202 | SetPCToddler | 4 | **character creation** (CG01) |
| 0x122F | DisableAllMines | 4 | Fallout3, BrokenSteel |
| 0x10B8 | StopWaiting | 3 | Fallout3 |
| 0x1205 | TriggerScreenBlood | 2 | Fallout3 |
| 0x1237 | RemapWaterType | 2 | BrokenSteel |
| 0x116F | GetLastPlayerAction | 1 | ThePitt |
| 0x11EE | ShowSPECIALBookMenuParams | 1 | Fallout3 |

Two more vanilla-range opcodes have no name in the catalogue: 0x11F3 and 0x1231, each a
bare statement without arguments in a quest stage [observed]. Lined up with their
stages' source text (2026-11-03): 0x1231 is **KillQuestUpdates** (CG01 stage 90, between
`AutoDisplayObjectives 0` and `imod FadeToWhiteAndBackISFX`) and 0x11F3 is
**TriggerLODApocalypse** (QUST 0x06014E9E is MS11, The Power of the Atom, not a
character-creation quest: stage 60, Megaton's destruction, between
`MS11MegatonToggle.disable` and `MegatonMapMarkerREF.disable`) [observed: SCTX lines
against the compiled order]. Neither is in the catalogue or in the GetCommandOpcode
capture; the GECK wiki extract has a page for KillQuestUpdates (`Misc.xml`), none for
TriggerLODApocalypse.

The character-creation group (SetPCToddler, SetPCYoung, MatchRace, MatchFaceGeometry,
SetLocationSpecificLoadScreensOnly, SetNoActivationSound, the two unnamed opcodes) is
TTW's Fallout 3 start in Vault 101 (CG01–CG04), so it comes first.

## 2. Extender functions not built (15, all in TaleOfTwoWastelands.esm) [observed]

| Function | Origin | Calls |
|---|---|---|
| SetOnProjectileImpactEventHandler | JIP | 9 |
| SetOnMenuCloseEventHandler | JIP | 6 |
| InitExtraMiscStat | JohnnyGuitar | 5 |
| AttachAshPileEx | JIP | 3 |
| MarkActivatorAshPile | JIP | 3 |
| SetMusicState | JIP | 2 |
| SetOnUseAidItemEventHandler | JIP | 2 |
| SetOnMenuOpenEventHandler | JIP | 1 |
| SetOnKeyUpEventHandler | JIP | 1 |
| SetOnCraftingEventHandler | JIP | 1 |
| SetOnHitEventHandler | JIP | 1 |
| SetOnLocationDiscoverEventHandler | JIP | 1 |
| ScriptWait | JIP | 1 |
| SetTerminalUIModel | JIP | 1 |
| DonnerReedKuruParty | JIP | 1 |

Nearly all the event handlers are set up in one script, TTWGenericHandlerScript: gauss
and Zhu Rong impacts, bean-bag, coin-shot, pulse-slug, dragon's-breath, incendiary and
dart ammo, the workbench and Baby Book menus, ammo switching on a key, crafting,
discovered locations, healing and chem items. One piece of work for the JIP event list
(docs/spec-notes/jip-events.md).

## 3. Opcodes no catalogue names [observed / hypothesis]

| Opcode | Calls | Reading |
|---|---|---|
| 0x3CE0 | 21 | ActorHasBaseFlag (ShowOff) by its source line [hypothesis]; already built by name (`nvse_plugins.rs`), so nothing to do once the opcode is not needed |
| 0x3808 | 8 | `TTW_EnableRadioFix` by source order in TTWGenericHandlerScript [hypothesis] |
| 0x28D7 | 1 | `InitRockItLauncher` by source order in TTWGenericHandlerScript [hypothesis] |
| 0x3800 | 2 | an argument-less statement in Vault101ExitScript; not ForceSave (vanilla 0x1217) [observed]; unknown |

`TTW_EnableRadioFix` and `InitRockItLauncher` are in no catalogue; the same script asks
`IsPluginInstalled "ttw_nvse"` [observed], so they are probably TTW's own NVSE plugin
[hypothesis]. TTW's script also says it has required ShowOff since 2024 [observed: source
comment]. Neither plugin's opcodes are in `schemas/script/command_opcodes.json` (captured
with xNVSE, JIP LN and JohnnyGuitar only); a GetCommandOpcode capture with ShowOff and
ttw_nvse installed would settle §3.

## 4. Built, but first called by TTW [observed]

GetInCellParam (140 calls), GetInZone (70), IsOwner (48), GetKillingBlowLimb (7), IsMoving
(5), IsChild (2): implemented, never called by the New Vegas masters, so TTW is their
first real data.

## Suggested order

1. The character-creation group of §1 (TTW's start).
2. The rest of §1 by calls (ModDisposition, GetPCIsRace, GetForceSneak,
   SetMerchantContainer first).
3. The JIP event handlers of §2 (one script registers them all).
4. The §3 opcode capture, then ttw_nvse's two functions.

## Outcome (2026-11-03)

Built (`crates/fnv-game/src/functions/fo3.rs`; the catalogue entries got their
opcodes from `schemas/script/command_opcodes.json`): ModDisposition, GetPCIsRace,
GetForceSneak, SetMerchantContainer, MatchRace, ClearActorsFactionsPlayerEnemyFlag,
GetPCSleepHours, SetLastExtDoorActivated. Kept as state only, nothing draws or plays by
them yet: SetPCYoung, SetPCToddler, SetNoActivationSound, ClearNoActivationSound,
SetLocationSpecificLoadScreensOnly (docs/spec-notes/scripts.md §7.3).

Left in §1: MatchFaceGeometry (Vault101ExitScript and CG04 stage 65 call
`Dad.MatchFaceGeometry player CGMatchFace`, a global for the percentage [observed:
source]: it changes an NPC's FaceGen geometry, and the engine has no per-reference face
yet), the two opcodes above, and SetCellFullName, TriggerHitShader, ResetInterior,
FadeSFX, SetFactionReaction, SetZoneRespawns, DisableAllMines, StopWaiting,
TriggerScreenBlood, RemapWaterType, GetLastPlayerAction.
