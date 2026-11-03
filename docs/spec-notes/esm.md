# Spec note: plugin files (ESM/ESP), Fallout: New Vegas

Clean-room description in plain language; `crates/fnv-esm` is implemented from it.
Tags: **[documented]** (public source named), **[observed]** (checked against the
user's FNV 1.4.0.525 masters: FalloutNV.esm, the 4 story DLCs, GunRunnersArsenal and
the 4 pre-order packs), **[hypothesis]**.

Sources: UESP "Tes4Mod:Mod File Format" (record/group/subrecord container, shared by
the Oblivion/Fallout 3/New Vegas generation); xEdit `wbDefinitionsFNV.pas`
(record layouts; cited per entry in `schemas/`).

## Container structure

A plugin is a sequence of **records** and **groups**. All integers little-endian.

**Record header, 24 bytes** [documented][observed]:
type (4 ASCII chars), data size u32, flags u32, FormID u32, version-control u32,
form version u16, version-control u16. The data that follows is `data size` bytes.
- Form version is 15 for 99% of FalloutNV.esm records; older values (1–14) occur on
  records last saved by older tool versions [observed].
- Flag 0x00040000: the data is a u32 uncompressed size followed by a zlib stream
  [documented][observed: 33,179 compressed records in FalloutNV.esm].
- Flag 0x1 on the first record (TES4) marks a master file [documented][observed].
- Flag 0x20 marks a deleted record [documented: xEdit wbFlagsList names bit 5 "Deleted"
  for every record type].

**Record header flags per record type.** The other bits of the flags field mean
different things per record type. xEdit gives each record type its own list of named
bits (`wbRecord(<sig>, …, wbFlags(wbFlagsList([bit, name, …])))`, bit numbers counted
from 0 [documented: wbDefinitionsFNV.pas and wbInterface.pas `wbFlagsList`, dev-4.1.6]);
the GECK wiki's GetFormFlag/SetFormFlag pages print one merged table of all of them
[documented: GECK wiki GetFormFlag, owner export 2026-10-07]. NVForever keeps these
names as data: a record entry in `schemas/fnv` may carry `header_flags` (format in
`schemas/README.md`), read through `Schema::header_flag` and printed by `fnvtool esm
decode` as `flag_names`. The container-level bits the reader itself needs (master,
deleted, compressed) stay in code (`raw::record_flags`).

- **Quest Item, bit 10 (0x400)** on item records [documented: xEdit flag lists of WEAP,
  ARMO, ALCH, BOOK, KEYM, MISC and LIGH; the GECK wiki IsQuestItem and SetQuestObject
  pages list Ammo and Weapon Mod among the form types that can be quest items, and
  GetFormFlag names bit 10 "Quest item", so AMMO and IMOD carry the same name in the
  schema, although xEdit gives those two types no flag list]. The same bit means
  "Persistent" on placed references (REFR/ACHR/ACRE) [documented: GECK wiki GetFormFlag;
  `fnv_sim::refs::FLAG_PERSISTENT`]. NOTE, CHIP, CCRD and CMNY have no quest flag in
  either source; non-item records with the bit (ACTI, CONT, DOOR, NPC_, CREA, …) are not
  named in the schema yet.
- In the ten masters, bit 10 is set on 339 winning item records: MISC 244, KEYM 68, WEAP
  12, ARMO 10, ALCH 4, BOOK 1; on no AMMO, IMOD, LIGH, NOTE, CHIP, CCRD or CMNY record
  [observed: `fnvtool sim forms`, `quest_items`, 2026-10-09]. Examples: The Platinum Chip
  (MISC 0x001164FB), Doc Hoff's Supply Key (KEYM 0x00068E8D), the snow globes, Boone's
  Beret (ARMO 0x000B7F53), the Dead Money collar (ARMO 0x01006E8F). The Pip-Boy 3000 and
  Pip-Boy Glove do not carry it (they are hidden by the ARMO "Non-Playable" flag
  instead). Other bits set on item records: ALCH bit 29 (15), ARMO bit 19 (2, "Has
  Platform Specific Textures"), LIGH bit 16 (9, "Random Anim Start"), WEAP bits 27 (9)
  and 29 (23) [observed]; xEdit calls ALCH 29 and WEAP 27/29 "Unknown", so the schema
  leaves them unnamed.

**Group header, 24 bytes** [documented][observed]: `GRUP`, total size u32 (including
this header), label (4 bytes), group type i32, then four u16s (stamp, unknown,
version, unknown) that are preserved verbatim. Group types:

| Type | Label | Contents |
|---|---|---|
| 0 | record type | all top-level records of that type |
| 1 | parent WRLD FormID | worldspace children |
| 2 / 3 | block / sub-block number (i32) | interior cell blocks |
| 4 / 5 | grid y, x (two i16) | exterior cell blocks |
| 6 | parent CELL FormID | cell children |
| 7 | parent DIAL FormID | topic children (INFO) |
| 8 / 9 | parent CELL FormID | persistent / temporary references |
| 10 | parent CELL FormID | visible-distant children; documented, **absent** in all shipped FNV masters [observed] |

**Subrecords** inside record data: type (4 chars), size u16, data. A subrecord of type
`XXXX` with size 4 carries a u32 that is the real size of the *next* subrecord, whose own
size field is then 0 [documented][observed: one such subrecord per large master, in the
WRLD offset table]. 

**Text** is Windows-1252, usually NUL-terminated (`zstring`) [observed]. FNV has no
localized-string tables [observed: no TES4 localized flag, strings are inline]. A
localized install is assumed to carry its text inline the same way in its own code page
(Windows-1251 for Russian) [hypothesis: not looked at]; NVForever reads text in the
code page set for the run (`fonts.md` §2.1).

## TES4 header record

Subrecords [documented][observed]:
- `HEDR` (12 bytes): version f32 (1.32–1.34 across masters), record count i32, next
  object ID u32. **The record count equals (records + groups − 1)**, i.e. every record
  and group except TES4 itself, in all 10 masters [observed]. This gives a free
  self-check that does not depend on any third-party tool.
- `CNAM` author, `SNAM` description (optional zstrings).
- `MAST` zstring + `DATA` u64 pairs, one per master, in order. DATA is 0 in every
  shipped plugin [observed]; described as master file size in community docs.
- `ONAM`: array of u32 FormIDs. In every shipped DLC, ONAM lists **exactly** the
  overridden records that sit in cell temporary-children groups (type 9) and nothing
  else [observed: 179/109/202/159/94 entries for DM/HH/OWB/LR/GRA, set-equal]. Those are
  REFR, ACRE, ACHR and NAVM records, plus 7 LAND records in LonesomeRoad.esm [observed]. Purpose believed to be letting the engine find overridden
  temporary references without loading the cell [hypothesis].

## FormIDs and load order

- A stored FormID's top byte is an index into **that file's** master list; the value
  equal to the master count means "this file" [documented][observed].
- At load, the top byte is replaced by the load-order index of the referenced file;
  later files override earlier definitions of the same global FormID [documented].
- **Injected forms** [observed]: the four pre-order packs (Caravan/Classic/Mercenary/
  Tribal) define *all* their records with top byte 00 — FalloutNV.esm's space — although
  those FormIDs do not exist in FalloutNV.esm. LonesomeRoad injects one AVIF the same
  way. The engine must accept new forms in a master's ID space.
- **Out-of-range index** [observed]: GunRunnersArsenal.esm has one REFR with FormID
  0x02000801 although it has only one master (valid indices 0 and 1). The game treats the
  index as the file's own [observed in game, 2026-10-09; see "Cross-check against
  xEdit"]. Our loader reads such an index as the file's own, as the game does
  (`LoadOrder::to_global`, #281; see below).
- FormIDs below 0x800 in the first master are reserved for engine built-ins
  [hypothesis, community knowledge]; references into that range need not resolve to a
  record.

## Data irregularities in the user's files

- No compressed record of the ten masters has bytes after its Adler-32: `fnvtool esm
  verify` reported no `Extra` trailer among their 46,696 compressed records; the one
  anomaly is the LAND below [observed 2026-10-31, #998].

- FalloutNV.esm, LAND 0x00150FC0 (file offset 0xB0CFF04): the zlib stream decodes
  completely to exactly the declared 4,385 bytes and parses into valid subrecords, but the
  stored Adler-32 is wrong (stored 0x6E704ECC, computed 0x6C404EC4). Python's zlib
  rejects it. Arithmetic on the two checksums says the authored data had one byte, at
  offset 4,315 (inside VHGT terrain heights), larger by 8 than what decodes [observed;
  the single-byte explanation is a hypothesis]. Either the shipped file has this defect
  or the user's copy is damaged — **verify the SHA-256 against a second copy or after a
  Steam "Verify integrity of game files"**. The reader accepts such streams when the
  output size matches, and reports them. Re-hashed read-only on 2026-10-09: the owner's
  current `Data\FalloutNV.esm` (245,650,747 bytes) still has SHA-256
  `50991d36804b7d1e70df1afd7471b72f0e29d1b456ee2516a9717c002564e7c1`, the value the
  anomaly was first found in [observed]; telling a shipped defect from local damage still
  needs a Steam "verify" (owner-only) or a second copy.

## Cross-check against xEdit (M1)

The owner's FNVEdit export (`tools/capture/xedit/export_inventory.pas`, private goldens
`esm/<Plugin>.xedit.inventory.json`) covers all 10 masters, 629,788 records. Type,
EditorID and FULL agree with `fnvtool` and with the independent `oracle_esm.py` on every
record except **four rows** (2026-10-09) [observed]. In all four, both of our readers
report what the file holds and the export shows xEdit's own normalisation:

| Plugin | Record | File holds | xEdit export | Ours = oracle |
|---|---|---|---|---|
| FalloutNV.esm | DIAL 0x00152CCA | PNAM, DATA only; **no EDID** | `edid: ""` | `edid: null` |
| DeadMoney.esm | RACE 0x01005819 `Christine` | EDID, DESC, DATA, ...; **no FULL** | `full: ""` | `full: null` |
| OldWorldBlues.esm | CLAS 0x0101533F `NVDLC03SinkVendorClass` | EDID, DESC, DATA, ATTR; **no FULL** | `full: ""` | `full: null` |
| GunRunnersArsenal.esm | REFR stored as **0x02000801** | that FormID | `0x01000801` | `0x02000801` |

- **xEdit adds missing required subrecords as empty ones** [documented: xEdit
  `Core/wbImplementation.pas`, branch dev-4.1.6, the record-loading code right after
  `mrDef.AfterLoad`: for a record that is neither deleted nor a partial form, every member
  the record definition marks *required* and the file lacks is added as an empty element,
  logged as "Adding missing record"]. In `wbDefinitionsFNV.pas` DIAL starts with
  `wbEDIDReqKC`, RACE and CLAS with `wbEDIDReq, wbFULLReq` [documented]. The export script
  tests presence (`ElementExists` / `ElementBySignature`), finds the placeholder and writes
  `""`. Check over all 10 masters [observed]: these three are the *only* records that lack
  an EDID or FULL which xEdit's definition marks as required; all 1,126,037 other absent
  EDID/FULL values (optional in xEdit) are `null` in the export, as in ours. The file has
  no such subrecord, so `null` (absent) is the faithful reading; the difference is xEdit
  normalisation, not data.
- **xEdit folds an out-of-range master index into the file's own index** [documented:
  xEdit `wbImplementation.pas`, `TwbMainRecord.DoGetFixedFormID`: a file index greater
  than or equal to the plugin's master count is replaced by the plugin's own index]. GRA
  has one master, so 0x02000801 is shown as 0x01000801. Our inventory keeps the stored value
  (it describes the file) [observed]. Details of that REFR [observed]: its
  base (NAME) is 0x02000800, also out of range (GRA's own 0x01000800 is the WEAP
  `WeapNVDLC00Faderator`, "The Faderator"); it sits in the temporary children of
  `TestMentats` (0x00108BED, FalloutNV.esm's "Mentats Test Level"); DATA position
  (2341.27, 2058.89, 6991.75), rotation 0; no other subrecords. A developer test
  placement saved while the file had one more master is the likely origin [hypothesis].
  **The game does the same as xEdit** [observed, in-game capture `l1_oob_formid`
  2026-10-09, `tools/capture/jobs/l1_oob_formid.steps.json`, owner's load order with GRA
  at index 05]: after `coc TestMentats`, the cell holds a reference `05000801`, enabled,
  base `05000800` (GRA's own WEAP), within 0.005 units of the stored position, and it is
  the only reference in the cell whose source mod is GRA (99 references in the cell). So
  the out-of-range index 02 is read as "this file", not as load-order slot 02
  (HonestHearts) and not dropped; the base FormID in NAME is mapped the same way. Before
  the cell was loaded, `BuildRef 5 2049` gave no form (a temporary reference exists only
  while its cell is loaded [hypothesis]). `LoadOrder::to_global` does the same since 2026-10-15 (PR #281): any index at or beyond
the master list is the plugin itself. Index master-count + 1 is the capture above; a far
larger one is observed too: `CM4A1.esp` (a Colt M4A1 mod, one master) stores all its own
records as 0x12xxxxxx, and the original game places its rifle in `GSDocMitchellHouse`
(owner's screenshot 2026-10-15), which NVForever dropped under the old rule. The change
adds GRA's reference to `TestMentats` (98 instead of 97), so `L3.world.cells.interiors` /
`L3.world.refs.interiors` fail by that one reference: their golden comes from
`oracle_world.py`, which drops such records like our old rule. Updating the oracle and
regenerating that golden waits for the owner.

Consequence: `L1.esm.inventory_xedit.{FalloutNV,DeadMoney,OldWorldBlues,GunRunnersArsenal}`
fail by exactly these rows (scores 0.999998, 0.999975, 0.999981, 0.997433) and the other
six pass with every row equal. Copying xEdit's behaviour into our reader would make the
inventory say something the file does not, so the reader stays as it is; the comparator,
scenarios and goldens are untouched (parity-lab/AGENT.md).

## Round-trip guarantees (tested)

- Re-serializing the record/group tree reproduces all 10 masters byte-for-byte
  (FalloutNV.esm, 245,650,747 bytes, in ~3 s) [observed].
- Every record payload re-serializes identically from its parsed subrecords, including
  XXXX handling [observed].

## Unknowns (need analysis or the original game)

1. Field-level layouts for the (record, subrecord) pairs not yet in `schemas/fnv`
   (FalloutNV.esm, 2026-10-09: 183 of 1,155 pairs, 5.3% of subrecord instances; biggest:
   PACK, NAVM, NAVI, DIAL, TERM) — tracked by `fnvtool esm coverage` as the work queue.
2. Order-dependent subrecords (same signature meaning different things depending on what
   precedes it, e.g. condition blocks, model sub-structures) — schema format v1 has no
   context rules yet.
3. GMST `DATA` type depends on the first letter of the Editor ID (s/i/f) [documented in
   xEdit as a union]; schema v1 has only size-decided unions (`{"union": [...]}`, used for
   WATR DATA), not EditorID-decided ones, so GMST DATA stays undecoded.
4. Engine handling of the bad-checksum LAND and of the dangling reference DeadMoney INFO
   0x01011B23 PNAM ("Previous INFO" [documented: xEdit INFO]) → 0x010108DD (no such
   record in any loaded file) [observed]. In game the INFO exists (`BuildRef 1 72483` →
   `01011B23`) and 0x010108DD does not (`BuildRef 1 67805` → no form) [observed,
   `l1_oob_formid`]; whether the dangling "previous" link changes the topic's response
   order is not observed. The INFO is the last line of topic `NVDLC01RadioElijahEnd`
   (DIAL 0x01011B12, quest `NVDLC01RadioElijah` 0x0100F2BB, Elijah trapped in the vault
   at the end of Dead Money; its result script sets `bEndBreathing`). Capture plan: a job
   that needs a save at the end of Dead Money (none of the three capture saves is there),
   so it needs the owner: from such a save, let the radio play and record, per game
   loop, `NVDLC01RadioElijah.bEndBreathing` and the topic's spoken INFO order, then
   compare with the order our loader builds when PNAM points nowhere.
   The out-of-range GRA FormID is settled (Cross-check section above).
7. FalloutNV.esm has 18 ANIO records whose DATA ("Animation", an IDLE [documented:
   xEdit ANIO]) is 0x0100xxxx although the file has no masters, e.g. ANIO 0x0009266D →
   0x0100BA20; folded to the file itself they name no record (no IDLE with those low IDs
   in any master either) [observed: `fnvtool esm loadorder --schema`, 2026-10-09].
   The same report finds 6 REGN and 7 WTHR FormID fields in FalloutNV.esm that name no
   record (e.g. REGN 0x00174D19 → 0x000D705D, WTHR 0x00092BC4 → 0x000922EA) [observed];
   which subrecords they are in is not analysed yet (layouts from the sky workstream).
5. Version-control fields and group stamp/unknown fields — preserved, not interpreted.
6. Two QUST `DATA` subrecords in FalloutNV.esm (e.g. 0x0001E5CA) are 2 bytes long, while
   the documented layout needs at least 4 [observed]; engine behaviour unknown.
