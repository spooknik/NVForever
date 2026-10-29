# Environment radiation

Radiation the player takes from the world: placed radiation markers and irradiated water,
cut by Rad Resistance, the Geiger counter's clicks, the HUD's radiation meter and
GetRadiationLevel. Rads from food, drink and chems, RadAway, Rad-X and the radiation
sickness stages were built before (aid-items.md §3, §11) and are only used here.
Code: `crates/fnv-game/src/radiation.rs` (the rule), `engine/nvf-play/src/radiation.rs`
(water, Geiger counter, meter, log). Queue item 26, 2026-10-21; explosions' radiation and
damaging water: ESM gap audit row 7, branch claude/e7-explosion-water-radiation.

## 1. Sources

- GECK wiki export (`C:\fnv-private\geckwiki`): GetRadiationLevel ("a radiation level of 3
  would give you 3 rads/sec"; the actor's own rads are GetAV RadiationRads),
  GetRadiationLevelAlt (the vanilla function "would only return a value when called on
  the player"), Get/SetRadiationExtraData ("mostly used for Radiation Markers, where it
  determines the amount of rads gained while standing in the marker's radius"), the
  Settings page's "Radiation Settings" (fRadiationAccumulationRate,
  fSwimRadiationDamageMult, fWadeRadiationDamageMult). The wiki's own Radiation and
  Radiation Marker pages are not in the export.
- GECK wiki (offline snapshot 2026-10-28, `E:\Games\geck-wiki`): Reference, the Extra tab:
  "Radius: Radiation Marker: The distance from the Radiation Marker that starts giving
  the player radiation"; "Radiation: The amount of Radiation given off per second by the
  Radiation Marker, at the location of the marker." Markers: the Radiation variable sets
  "the radiation per second gained at the marker's location", the Radius "how far out
  radiation gain starts to occur". GetRadiationLevel: "The radiation level is directly
  proportional to your rads/sec meter." fRadiationAccumulationRate: "a direct multiplier
  of the radiation the player and creatures receive from non-water sources"; it "does
  not influence the "+? Rad/Sec" UI element, since it does not change the base amount".
- The original game [observed: Steam 1.4.0.525 with xNVSE, the capture plugin's script
  queries, private jobs `hud_corners` and `hud_corners2`, 2026-10-29; issue #894]: after
  `coc SLNuclearTestShackINT`, at GetPos (4318.96, −27.96, −135.08 … −135.11),
  `player.GetRadiationLevel` read 9.013766 … 9.013797 on six reads and
  `player.GetAV RadResist` 8; the HUD showed "+9 RAD/SEC". (The jobs' moves to two other
  spots did not move the player: every read is at the coc spot.)
- xEdit wbDefinitionsFNV.pas: REFR XRAD "Radiation" (f32), XRDS "Radius" (f32); WATR XNAM
  "Actor Effect".
- GECK wiki export, Special-Effect "Explosion Dialog": "Radiation: RAD Level: Amount of
  RADs per second. Radius: Size of radiation effect. Dissipation Time: Duration of
  radiation effect." WaterType: "Causes *n* damage per second: … the number of health
  points lost per second while in this water type. Vanilla water types do not use this
  option, but cause rad damage instead", with the damage taken "every second they are
  submerged". IsInDangerousWater: "Returns 1 if a calling actor reference is in water
  that causes damage." JIP's GetWaterTrait: trait 1 is the flags, "1 - Causes damage".
- xEdit wbDefinitionsFNV.pas: EXPL DATA ends with the radiation struct (level,
  dissipation time, radius) before the sound level; WATR FNAM bit 0 "Causes Damage",
  DATA "Damage" u16 (the whole 2-byte DATA, or the last field of the old 186-byte form).
- FalloutNV.esm and the add-ons [observed, read with a private script over the plugins]:
  the records below.

## 2. Radiation markers

- **The data** [observed]: 119 references carry XRAD, every one a reference of the static
  `RadiationMarker`. XRAD runs from 0.2 to 720 (1, 5, 10, 3, 2 most often); XRDS from 10
  to 10000 (512, 256 and 1000 most often). 69 stand in exterior cells of the Mojave, the
  rest in interiors (Vault 34, the Searchlight buildings, Techatticup Mine, the nuclear
  test site's shack, Rat Cave …).
- **Level in the radius**: a marker gives its whole XRAD at its position, falling
  linearly with the 3-D distance d from it to nothing at its radius:
  XRAD × (1 − d ÷ XRDS); outside the radius nothing. [documented: §1 Reference, the level
  is given "at the location of the marker" and the radius is where radiation "starts";
  observed: §1, the shack's marker 0x00154611 (XRAD 15, XRDS 2000, at 4358.33, −718.89,
  −88.0 [observed: FalloutNV.esm, `fnvtool esm decode`]) stands 693.65 units from the
  original's reads; 15 × (1 − 693.65 ÷ 2000) = 9.7976, and after Rad Resistance 8 (§4)
  9.7976 × 0.92 = 9.013783 against the read 9.013784, the three reads' small spread in
  z following the formula to 1e-6; the 2-D distance (692.05) would give 9.0248 and
  no falloff 13.8. Confirmed at a second distance [observed: private job `hud_radiation`,
  2026-10-29, MoveTo to the coc XMarker plus offsets, GetPos and GetDistance after each
  move]: at (4300, −170, −135.1), 554.03 units from the marker, Rad Resistance 8, the read
  9.977171 against the formula's 9.977124 … 9.977193 for the distance read to 0.01 (554.03 …
  554.04; 2-D: 9.9910); and at the coc spot after Rad-X (Rad Resistance 39) 5.976524
  against 5.976506 … 5.976551. Three further spots (612.97, 789.78 and
  920.32 units) all read 9.570496 with Rad Resistance 6, the same value at three distances,
  and a fourth (737.68 units) 8.710030; those reads do not follow any distance and are
  left unexplained (the player may not have stood where GetPos reported, or the level was
  not updated between moves) [hypothesis]. Issue #894.] Until 2026-10-29 the whole XRAD held in the radius, which
  gave 15 there.
- **Overlapping markers** [hypothesis]: the strongest at the point counts; they do not
  add. The masters put markers of rising level and falling radius on one spot — Clark
  Field 3/2000 with three 720/10 spots, the crater by Camp Searchlight 5/10000, 10/6000
  and 18/2000 on one point, Black Mountain's crater 1/3500, 10/1500, 5/500. (That this
  made a stepped gradient was the reason for flat markers; with the falloff they make a
  gradient whether they add or not, so the sum is as possible as the strongest; no read
  of the original settles it.)
- A disabled marker (an enable parent, `Disable`) gives nothing [hypothesis].
- The markers are kept by the space they stand in (the worldspace for exterior cells, else
  the interior); every marker of the player's space is measured each frame (119 in all,
  no culling by loaded cells).

## 2.1 Explosions' radiation zones

- **The data** [observed]: 24 FalloutNV.esm explosions carry a radiation level, 5 in
  OldWorldBlues, 7 in LonesomeRoad, 5 in GunRunnersArsenal (one, GrenadeNukaFireBurst,
  overrides the base game's). Level (rads/s) / dissipation time / radiation radius:
  FatManNukeExplosion 10 / 12 / 616 (its blast radius 1700); SmallNukeExplosion
  10 / 10 / 900; BusNukeExplosion and VertibirdExplosion 10 / 12 / 616;
  SatcomNukeExplosion 100 / 24 / 1232; HVBunkerExplosion 5 / 30 / 2000; FFE08Explosion
  2 / 30 / 3000; GlowingOneBurstExplode 50 / 1 / 640; GrenadeNukaFireBurst 50 / 2 / 200;
  NVDLC05FatManBigKidExplosion 30 / 20 / 2500; NVDLC04WarheadNukeExplosion 20 / 18 / 1024.
  The dissipation times (1 to 30) read as seconds; every radiation radius is set (100 to
  3000).
- **The rule** [hypothesis]: an explosion with a radiation level above 0 and a
  dissipation time above 0 leaves a zone where it went off. Its level starts at the
  EXPL's and falls linearly to nothing over the dissipation time, counted in seconds of
  play (game-mode time, as the rads are; the hours of a wait or sleep count at the
  TimeScale, an hour = 120 s at 30, so they end every zone). Its radius is the radiation
  radius (the blast's own radius if that were under 1 unit; no master needs it). A zone
  counts whole within its radius (3-D distance), and the strongest of all markers and
  zones counts, not their sum. Reasons: the wiki calls the time a "duration", which a
  fade over it reads naturally (a flat level ending at once is the other reading);
  markers are taken not to add (§2). Whether a zone falls off with distance as a marker
  does (§2) is not known: kept flat until a read of the original says.
- So a Fat Man's blast gives up to 10 rads a second within 616 units for 12 s, about 60
  rads before Rad Resistance to one who stays at its centre.
- The zone lies in the space the player is in when it goes off (explosions happen in the
  loaded world around the player) [design]. Zones are not saved [design: they last
  seconds].
- The explosion's log line no longer says radiation is not built.

## 3. Irradiated water

- **The data** [observed]: no FalloutNV.esm water has a non-zero WATR damage (DATA's u16,
  all 0). Every water but three names an actor effect in XNAM: WaterHeal1Purified (no
  rads), WaterHeal2Good (4 rads), WaterHeal3Average (6), WaterHeal4Bad (10),
  WaterHeal5Terrible (20), WaterHeal1Rads500 (500); these are the effects of drinking
  the water (a Damage Radiation Level effect, plus health and dehydration).
- **The rule** [hypothesis]: swimming in a water gives its effect's rads a second times
  `fSwimRadiationDamageMult`; wading (the feet under the surface, not swimming) times
  `fWadeRadiationDamageMult`. Both are 0.1 in FalloutNV.esm [observed], so the Mojave's
  river and lake waters (WaterHeal4Bad) give 1 rad a second.
- **What the engine tells** (`tell_water`): swimming is the physics character's state;
  wading is the feet under the water surface the physics has at the player's x, y. The
  water type is the cell's XCWT, else the worldspace's NAM2 [hypothesis: a placed water
  volume is taken as its cell's water type; interiors have physics water only through
  their placed volumes].

## 3.1 Damaging water

- **The rule** [documented: §1 WaterType]: a water type with "Causes Damage" (FNAM bit 0)
  takes its DATA damage of health from the player once a second while the player is in it
  [hypothesis: "in it" is swimming or wading, as for its rads (§3); the first hit comes a
  second after going in; the hit is an effect's health damage (essential actors fall
  unconscious, others die at 0) with no attacker]. The wiki's camera shake and hurt
  sound are not played [not built].
- **The data** [observed]: no water of FalloutNV.esm or the add-ons has the flag or a
  damage above 0, so the vanilla game never uses it; only plugins can.
- IsInDangerousWater (opcode 0x114C) is 1 for the player in such a water, 0 otherwise and
  on any other actor [documented: §1; observed: 0 in the Mojave's water (physics.md
  `sw_trial`), whose types lack the flag].

## 4. The level and the rads taken

- **Level** = the place's level (the strongest marker or explosion zone, §2, §2.1) + the
  water's (§3): rads a second before resistance.
- **GetRadiationLevel** = level × (1 − Rad Resistance ÷ 100) for the player, 0 for anyone
  else [documented: §1, and the wiki's "directly proportional to your rads/sec meter";
  observed: §2, the original's 9.013784 is the marker's 9.7976 there times 0.92 for its
  Rad Resistance 8, issue #894; hypothesis: the same for water's rads, not read]. It
  gave the level before resistance until 2026-10-29.
- **Rads taken a second** = level × `fRadiationAccumulationRate` (1.0 [observed]) ×
  (1 − Rad Resistance ÷ 100), Rad Resistance held to 0…85 [documented: §1, the setting
  is "a direct multiplier" of the radiation received; observed: Rad Resistance cuts
  GetRadiationLevel as a percentage (above); hypothesis: that the rads taken are cut
  the same way, the cap as Damage Resistance's 85]. They raise the player's Rads actor value (54) every frame of game-mode time (not in
  menus or conversations).
- Rad Resistance is the actor value 20 (the Endurance formula, `(END − 1) × 2`, Rad-X's
  modifier, apparel's enchantments: actor-values and aid-items). Rad-X and RadAway work
  through it and through the Rads value with nothing more here.
- **Radiation sickness** follows the Rads value's stages (RADS records, 200 Minor … 1000
  Fatal; aid-items.md §11) as before.
- `fRadiationDecayRate` (0.01) and `fRadiationResistanceAvoidLevel` (50) are read by
  nothing: what they do is not documented [observed: the values; hypothesis: the second is
  for the AI packages' Avoid Radiation flag, which is not built].
- Only the player takes environment radiation [design]; NPCs and creatures do not.
  Revisited for audit row 7 (2026-10-21) and kept: an NPC's Rads value drives nothing (the
  RADS sickness stages are the player's, GetRadiationLevel answers 0 for anyone else
  [documented: §1]), and the one visible NPC use, feral ghouls healing in radiation, is not
  documented. Building it would be guesswork with no effect to check.
- **Waiting and sleeping** are refused while rads are taken (the rate after Rad
  Resistance above 0) [owner request 2026-10-17; wait-menu.md §9], which the world's
  radiation now drives.

## 5. Geiger counter

- **The records** [observed]: `UIPipBoyRadiationA`, `…B`, `…C` name the folders
  `fx\ui\pipboy\radiation\a\` … `c\`, three takes each, each about one second (88,2xx
  bytes of 16-bit 44.1 kHz mono); `UIPipBoyRadiationTestLP` is a 11-second loop. Their
  FormIDs follow fRadiationDecayRate's and fRadiationAccumulationRate's (0x235B8 …
  0x235BC).
- **The rule** [hypothesis]: A, B, C are rising strengths; while the level is above 0 one
  burst plays every second, A below 5 rads a second, B below 10, C from 10; a new band
  starts with a burst at once. The bursts play at the listener (interface sounds). The
  test loop is not used.

## 6. The HUD's radiation meter

- **The file** [documented: `menus\main\hud_main_menu.xml` `RadiationMeter`]: a piece 367
  × 60 in the HUD's alternative colour (`&hudalt;`, 255 67 42 [observed:
  Fallout_default.ini]), a bracket picture `glow_messages_radiation_seperator_right.dds`
  (567 × 120 at x −135), a pointer picture, `radiation_text_value` (font 8, right-justified
  at x 237, y 65) and `radiation_text` (font 7, from x 245); the texts are empty and every
  part's alpha 0 in the file: the code fills and shows them.
- **The file, the pointer** [documented]: `radiation_pointer` is
  `glow_messages_radiation_seperator_arrow.dds` (from `InterfaceShared.tai`), 46 × 24 at
  x 15, y 25; the bracket's own `texatlas` line is commented out, so the bracket is its
  own file.
- **The pictures** [observed: the textures]: the bracket (512 × 128) is a top-right
  corner: a line across at rows 12–16 from about column 132 (fading in) to the upright at
  columns 494–499, which runs down to row 105. Sixteen notches hang from the line every
  20 columns from 173 to 473: slanted wedges at the two ends, long notches (to row 29) at
  233, 293, 353 and 413, short ones (to row 18) between. The pointer (64 × 32) is a
  triangle pointing up, its tip at row 8, its middle at column 32.
- **Built**: while rads are taken the meter shows "+n" (GetRadiationLevel's value, §4,
  rounded, at least 1) and "RADS" (`sStatsRADAbbrev` is set by no master), with the
  bracket and the pointer [documented: §1, fRadiationAccumulationRate does not change
  the number; observed: the original showed "+9 RAD/SEC" at its 9.0138, issue #894;
  hypothesis: the rounding, the word (the original's reads "RAD/SEC", not matched yet)].
  - **The pointer** [hypothesis]: its tip, just under the notches (6 under the picture's
    top at y 25), points at the player's rads: the left end's notch is 0, the right end's
    1000, so the long notches are 200, 400, 600 and 800, the radiation sickness steps
    (death at 1000). Over 1000 it stays at the end. The file's x 15 is taken as a start
    the code moves.
  - **The place** [owner, 2026-10-23: "in the top right corner of the screen symmetrical
    to the popup notification on the left side"]: the mirror of the corner messages'
    bracket (corner-messages.md §2), whose line across stands 47 under the view's top
    and upright 57 in from the left edge [observed: the atlas's picture as drawn]: the
    piece's top 33.5 under the view's top and its right edge 105 from the view's right
    edge, which puts the radiation bracket's line 47 under the top and its upright 57
    in from the right. Smoke `environment-radiation` saves `radiation_meter.png`.
  - **The pictures are stretched** to the file's sizes (567 × 120, 46 × 24), not fitted
    to their own shape (which drew the bracket 480 wide and put the upright through
    the text).
  - **The texts** [owner, 2026-10-23: "lower and to the right against the bracket but not
    interfering with the notches"]: "RADS" ends at x 396, inside the upright (the letters'
    glow reaches about 10 past their width), the value before it with the file's gap of
    8; their line 88 under the piece's top, so the capitals start below the pointer's
    lowest place (y 49) and the notches (y 28). The file's x 237/245 and y 65 are not
    used.

## 7. Log lines (smoke `environment-radiation`)

- `radiation: level <l> rads/s (marker <ref> <l>, swimming in <water> <w>), taking <r>/s
  (Rad Resistance <rr> %), rads <n>` when the source or the level (in whole rads: it
  falls off as the player moves) changes;
  `radiation: out of radiation, rads <n>`.
- `radiation: the player is swimming|wading in water <name>` (with `(it causes <n> damage a
  second)` for a damaging water), `… out of the water`;
  `radiation: water <name> takes <n> health (<k> hit(s) in all)`.
- `radiation: explosion <EXPL> leaves a radiation zone (#<serial>): <l> rads/s within <r>
  units, fading over <t> s`; the level line's source reads `zone of <EXPL> <l>`;
  `radiation: the zone of <EXPL> (#<serial>) has faded`. Smoke `explosion-radiation`.
- `geiger: UIPipBoyRadiation<A|B|C> (level <l>)`, `geiger: quiet` when the band changes.
- At exit: `radiation summary: <s> s in radiation, highest level <l> rads/s, rads <a> -> <b>
  (+<d>), <k> Geiger bursts, <z> explosion zone(s), <h> water hit(s)`.
- The sickness stage: `needs: Rad Level <n>: Radiation200` (aid.rs); the game's own CGTutorialRadiation box follows.

## 8. Not built

- NPCs and creatures taking rads (feral ghouls' healing in radiation, the Avoid Radiation
  package flag, companions' remarks rest on GetRadiationLevel, which answers for the player).
- Rads from a radioactive actor's aura: area effects are built (area-effects.md, a cast
  or a weapon hit spreads them; the Glowing One's attack burst reaches 30 feet), but
  `vGlowingOneRadAuraEffect` itself has no area (its script places
  vGlowingOneRadiationAuraExplosion) and nothing found casts it; the type-0 actor
  effects in actors' lists are not cast (area-effects.md §3).
- The Pip-Boy's own radiation readout beyond the Rads value it lists.
- Damaging water's camera shake and hurt sound; damaging water for NPCs.
- Explosions' radiation zones in saves.
