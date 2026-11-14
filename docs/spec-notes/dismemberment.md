# Dismemberment: severed and exploded limbs

Status: first version built 2026-10-20 (branch `claude/dismemberment`). Sources: the GECK wiki
(owner's local export, pages Body Part, BodyPartData, Weapons, Debris, KillActor, IsLimbGone,
GetKillingBlowLimb, Bloody Mess perk text), the masters read with `fnvtool esm decode`
(FalloutNV.esm), the models read with `fnvtool nif decode`, and the owner's play of the original
(2026-10-20, quoted below as **[observed: owner]**).

## 1. What the original does, as seen by the owner [observed: owner, 2026-10-20]

The measured odds are in §4.1: overkill turned out not to matter there.

- Overkill damage is what makes a limb explode.
- Some calibres never dismember; .308 and .50 BMG are very likely to explode a limb.
- An exploded limb leaves broken pieces of that part (leg bits; skull bits and eyeballs from a
  head) that start where the part was and fly all over the place.
- Knives and swords are more likely to cut a part off.
- The pieces left over are lootable: activating one opens the dead actor's inventory.
- Bodies and armour carry meat caps that cover the stump where an arm or leg was; some models
  may lack them.

## 2. The data

### 2.1 The weapon's On Hit choice

WEAP `DNAM` `on_hit` [documented: GECK wiki Weapons]: *Normal formula behavior* (the weapon
rolls to sever and to explode), *Dismember Only* (sever only), *Explode Only* (explode only),
*No Dismember/Explode* (limbs stay whole).

In FalloutNV.esm's 261 weapons [observed: `fnvtool esm decode`, every WEAP]: Normal 112,
Dismember Only 43, Explode Only 52, No Dismember/Explode 54. This is what the owner's
"calibres" are:

| Weapons | On Hit |
|---|---|
| sniper rifle, hunting rifle, anti-materiel rifle, Boone's and Oliver's rifles, hunting revolver, hunting shotgun, sledgehammer, super sledge | Explode Only |
| knife, combat knife, machete (all five), cleaver, mantis gauntlet, Ratslayer | Dismember Only |
| varmint rifle, baseball bat, fists, 11 of 12 grenades, all 8 mines | No Dismember/Explode |
| 9mm and 10mm pistols and SMGs, cowboy repeater, chainsaw, ripper, power fist, Legate's blade | Normal |

### 2.2 The body part (BPTD part, `BPND`)

Per part [documented: GECK wiki Body Part; xEdit layout in `schemas/fnv/combat.json`]:

- **Severable**: the part can be severed. Severing has an explosion, a generic debris and its
  count and scale, a blood spurt / decal impact data set, and a decal count. It has **no
  chance field**.
- **Explodable**: the part can explode, with an explosion chance (percent), a limb replacement
  model (`NAM1`) and its scale, an explosion, a generic debris (count, scale), an impact data set
  and a decal count.
- **Gore effects positioning**: a target bone (`NAM4`) and a translation and rotation from it.
  The blood spout starts there.
- Flag bit 6, "To Hit Chance - Absolute" in xEdit, is "Absolute" under Explodable in the GECK
  dialog [documented: GECK wiki Body Part]. No part of `DefaultBodyPartData` sets it [observed].
- When a part is dismembered, every severable or explodable part sharing its actor value goes
  too (Head 1 and Brain share Perception Condition). A part not flagged Explodable never
  explodes, even when a part sharing its actor value does [documented: GECK wiki KillActor
  notes].

`DefaultBodyPartData` (0x1D, people) [observed]:

| Part | Flags | Explode % | Debris × count | Replacement model | Gore bone, offset |
|---|---|---|---|---|---|
| Head (Head 1) | Severable, Explodable | 30 | GoreBits01 × 3 | `Gore\HeadBits01.NIF` | Bip01 Neck1 |
| Torso | Explodable | 50 | GoreBits01 × 4 | `Gore\GoreTorsoGore.NIF` | Bip01 Pelvis |
| Left / Right Arm | Severable, Explodable | 10 | GoreBits01 × 4 (right: 3) | `Gore\GoreArmGore01.NIF` | the upper arm, x 15–16 |
| Left / Right Leg | Severable, Explodable | 10 | GoreBits01 × 4 | `Gore\GoreLegGore01.NIF` | the thigh, x 16–18 |

Every part's explosion is `LimbExplosion` (0x1540D: model `Gore\FXGorePopHead01.NIF`, a sound,
radius 55, force 8, damage 0, impact data set 0x2ED47). Severing names no explosion and no
debris. Decal counts are 4 (sever) and 6 (explode).

### 2.3 Debris (DEBR)

`DATA` is repeated, one per model: a percentage (u8), the model path ended by NUL, then a
"has collision" byte [observed: the bytes of GoreBits01, which match combat-data.md §1.7].
GoreBits01 (0x1B2BF): `Gore\MeatBit02.NIF` 90 %, `Gore\MeatBit01.NIF` 70 %,
`Gore\MeatBit03.NIF` 40 %, each with collision 1. "The chance that this particular kind of
debris will appear" [documented: GECK wiki Debris]; spawned debris starts at a random rotation;
it goes when the cell unloads or `iDebrisMaxCount` is exceeded [documented: same]. The masters
have six DEBR records (gore, insect and robot bits, two effects, vertibird parts) [observed].

### 2.4 The gore models [observed: `fnvtool nif decode`]

Each replacement model holds several pieces, each a node with its own `bhkRigidBody` and
geometry:

- `HeadBits01.NIF`: Eyeball, Eyeball01, JawBoneBloody01, SkullChunk01, SpinePart01,
  SkullStemPart01, and one ragdoll constraint between two of them.
- `GoreLegGore01.NIF`: LegUpper, LegLower, Foot.
- `GoreArmGore01.NIF`: Hand, ArmLower, ArmGoreMidLow02.

### 2.5 Body and armour skins: partitions and caps

Body and armour shapes use `BSDismemberSkinInstance`. Every skin partition names a body part
(nif.xml `BSDismemberBodyPartType`, documentation only) [documented]. In
`characters\_male\upperbody.nif` [observed]:

- `Arms:0`: BP_TORSO, BP_TORSOSECTION_LEFTARM, _RIGHTARM, _LEFTLEG, _RIGHTLEG.
- `Arms:1`: BP_RIGHTARM, BP_LEFTLEG, BP_LEFTARM, BP_RIGHTLEG and the four torso sections.
- `bodycaps`: BP_TORSOCAP_LEFTLEG, _RIGHTLEG, _LEFTARM, _RIGHTARM.
- `limbcaps`: BP_SECTIONCAP_RIGHTARM, _LEFTARM, _LEFTLEG, _RIGHTLEG.
- `meatneck01`: BP_TORSOCAP_HEAD. `meathead01`: BP_SECTIONCAP_HEAD.

Cap partitions have part-flag bit 0 (editor visible) clear; the others have it set. Caps are
not drawn on an intact actor (actors.md §1). A shape all of whose partitions clear bit 0 is a
cap shape, hidden whole. In a shape with drawn partitions, a partition of a cap body part
(section caps 1xx, torso caps 2xx) with bit 0 clear is hidden by itself; any other partition is
drawn, whatever its flag [hypothesis: the original shows no caps on an intact radscorpion, owner
2026-10-30, #1075]. Over the 205 creature meshes and the 845 `characters\` and `armor\`
meshes of `Fallout - Meshes.bsa` (skeletons and gore left out; the add-ons' archives not
surveyed), the shapes mixing the two [observed: `fnvtool nif decode`, 2026-10-31]:

- `creatures\radscorpion\radscorpion.nif` `meatcapbody` and `barkscorpion.nif`
  `meatcapbody:0`: torso caps 201, 203–212 (bit 0 clear) and one partition of BP_TORSO (0)
  with bit 0 set, holding a single triangle. Hidden by whole shape, the caps showed on every
  leg root, claw and the stinger of an intact radscorpion. Its section caps sit alone in
  `meatcaplimbs` (101, 103–112, all clear).
- `creatures\mistergutsy\mistergutsy.nif` and `misterhandy.nif` `Object01:0`: BP_TORSO (set)
  with one torso cap 213 (clear) in the same shape.
- `armor\combatranger\combatranger.nif`, `combatranger_f.nif` and
  `armor\bigbattle\crangerbattlearmor.nif` `GorecapsBody:0`: BP_TORSO (set) with torso caps
  203, 205, 207, 210 (clear); `armor\boone\boone02.nif` `Meatcaparms_body:1`: BP_TORSO with
  torso cap 205. Their caps showed on intact wearers the same way.
- `armor\wastelandchild02\outfitf.nif` `Arms`: body parts 0–4 (set) and one partition of
  body part 65535 (clear); `armor\1950stylechild\outfitf.nif` `UpperBody`: 0 (set) and 65533
  (clear). Not cap body parts: drawn, which is why the rule asks for 1xx/2xx.

Every other creature shape keeps caps and drawn partitions in shapes of their own, and no
editor-visible partition names a cap (1xx/2xx) or a hidden one a non-cap. Partition flags seen:
0 or 256 for caps, 1 or 257 for the rest (bit 8's meaning is not known).

Meanings [hypothesis, from the names]:
- A *torso cap* covers the body's side of the cut. It is drawn on the body once the part is gone.
- A *section cap* covers the severed piece's end. It is drawn on the piece.
- A *torso section* (`BP_TORSOSECTION_X`, numbered X × 1000) is the strip of the body mesh that
  leaves with part X.

### 2.6 Creatures [observed: the 49 BPTD records of FalloutNV.esm; creature meshes in `Fallout - Meshes.bsa`]

A creature uses its own body part data (CREA `PNAM`) under the same rules. Of the 49 records, 43
have parts that can go. Examples (S: severable, E: explodable, explode chance; * names a limb
replacement model):

| Creature | Parts that can go |
|---|---|
| NVGecko, NVGiantRat, MoleRat | head S E 50 (no model); legs (and the rat's tail) S only |
| Dog, DogMeat (coyotes, mongrels) | head and legs S only: never explode |
| Rex | head S E 100; legs S only |
| NVNightStalker | head E 100 (no model); legs S only |
| NVCazador | head, wings, legs S E, mostly 0–50 %; no models, no debris |
| RadScorpion, ants, NVMantis, mirelurks | every leg, claw, stinger and head S E 30–100, insect limb models (`GoreInsectLimb.NIF`) |
| Feral ghouls, super mutants, aliens | as people: head S E 95, limbs S E 40–50, torso E 10, human gore models |
| NVBigHorner | head E 100*, legs S E 50 |
| Protectron, Robobrain, Mr. Gutsy, Sentry Bot, Securitron | heads, arms, legs S E 30–100, robot limb models (`GoreRobotLimb.NIF`, `RoboBit0N.NIF`) |
| Eyebot, ceiling turrets, NVTumbleweed, NVraven | torso E 100 (the whole thing) |
| Bloatfly, Liberty Prime, floor turrets, spore plant, Void, Zax eye | nothing |

- Debris is per kind:
  - GoreBits01 (flesh: geckos, rats, ghouls, people);
  - InsectGoreBits01 (0xB5B9E: scorpions, ants, mantises);
  - RobotGoreBits01 (0xB8FF4: robots).
- Explosions are per kind:
  - LimbExplosion (flesh);
  - LimbExplosionBug (0xB67B9, `Gore\FXGorePopHeadBug01.NIF`);
  - LimbExplosionRobot (0xBB636, `Effects\ExplosionRobotLimb01.NIF`);
  - LimbExplosionGlow (0xA18F5, glowing ones).

  Several creatures' severable parts name an explosion too (0x31CF9), so a cut spurts.
- The skins are split as people's are: every creature mesh looked at has
  `BSDismemberSkinInstance` partitions by body part and meat-cap shapes (torso caps 2xx,
  section caps 1xx). This holds for the gecko, dogs, the giant rat, radscorpions, ghouls, the
  mantis, the cazador, the nightstalker, the mole rat, deathclaws, the bighorner, brahmin,
  mirelurks, centaurs, super mutants, the Securitron, ants, the radroach, Mr. Gutsy, robobrains
  and sentry bots. The bloatfly and the eyebot have partitions but no caps.

So for a gecko, a giant rat or a mole rat an exploded head is GoreBits01 meat bits and a neck
cap, with no limb model. Their legs, and every part of a dog, can only be cut off.

## 3. Game settings [observed: FalloutNV.esm GMST values]

| Setting | Value |
|---|---|
| iCombatDismemberPartChance | 50 |
| iCombatExplodePartChance | 75 |
| iMessIntactLimbDismemberChance | 15 |
| iMessIntactLimbExplodeBonus | 25 |
| iMessCrippledLimbExplodeBonus | 25 |
| iMessTargetedLimbExplodeBonus | 0 |
| iMessTorsoExplodeChance | 10 |
| fExplodeLimbRemovalDelay | 0.03 |
| fExplodeLimbRemovalDelayVATS | 0 |

The wiki export has no page for any of them. The names tie them to Bloody Mess ("Mess"),
intact, crippled and targeted limbs, and the torso.

Bloody Mess (PERK 0x94EBA) gives the ability `PerkBloodyMess` (SPEL 0x79AE3, effect
`BloodyMessEffect`), and there is an actor value `Bloody Mess` (AVIF 0x5E5) [observed]. "Characters and creatures you kill will often explode" [documented: the perk's DESC].
That the effect raises the actor value, and that the value is what the rule reads, is a
**[hypothesis]**.

## 4. How often a kill dismembers

### 4.1 Measured in the original [observed: capture campaign `dismember_odds`, 2026-10-25]

`tools/capture/observe/dismember_odds.campaign.json`, save `nvfairport` (the Searchlight Airport
runway, its creatures killed), 23:00, player at level 50 with god mode and Sneak 100. A
Wastelander (WastelanderAM 0x0002E29D) with its items taken, restrained and turned away is
placed 300 units north of the player (the missile launcher 600); its health is set to 25, 75, 125
and 175 in turn, so the overkill runs from about 5 to 400. The player is turned to its head
(120 units above its feet) or its torso (85) every game loop and fires once a second with
`player.FireWeapon` and the equipped weapon (TapControl does nothing while the game is not in
front; JIP `TogglePlayerSneaking` and `SetWeaponOut` set the posture). The kills read as the
player's: sneaking, every hit carried JIP's Sneak Attack and Critical Hit flags, and Lucky's head
shot did 190.2 (30 × 2 head × 2 sneak, + 30 critical, + the skill bonus). Melee could not be
driven this way (JIP `FireWeaponEx` with a machete killed nothing), so blades and hammers are
not measured.

Per kill, at the first sample the target is dead: GetHitLocation (the part hit), IsLimbGone 0–14
within 10 samples, JIP GetHitExtendedFlag (1 critical, 3 dismembered, 4 exploded, 5 crippled,
7 sneak attack), GetHitHealthDamage, GetHitLimbDamage, and the health before and after
(health stays below 0 on a corpse: 25 − 91.1 read as −66.1, so overkill = −health after).

**Exploded or severed.** IsLimbGone does not tell them apart, and JIP's Limb Exploded flag
is unreliable (set on kills where nothing went, clear on heads that went). What separates
them: a part gone **with** the Dismembered flag never came with a new live explosion, and a
part gone **without** it came with one (GetNumRefs 81, the LimbExplosion of §2.2) in 7 of 7
trial kills and in most campaign kills (the count misses when an earlier explosion ends in the
same sample). So: gone + Dismembered = severed, gone otherwise = exploded. GetKillingBlowLimb
answers −1 on kills where nothing went, so kills are filed by GetHitLocation.

Counts: exploded + severed / kills, by the part the killing hit landed on (sneaking: every
kill a sneak attack critical, about 2,000 kills):


| Weapon | On Hit | Dmg | Head | Torso | Head, Bloody Mess | Torso, Bloody Mess |
|---|---|---|---|---|---|---|
| 9mm pistol | Normal | 16 | 16+6/39 | 0+0/29 | 19+10/29 | 24+2/35 |
| .357 revolver | Normal | 26 | 15+3/23 | 0+0/23 | 20+5/25 | 8+2/30 |
| Lucky | Normal | 30 | 18+4/29 | 0+0/28 | 19+8/27 | 15+1/32 |
| This Machine | Normal | 55 | 18+7/32 | 0+0/90 | 19+10/29 | 32+4/61 |
| Plasma rifle | Normal | 47 | 23+9/35 | 1+0/38 | 21+8/29 | 27+1/33 |
| Missile launcher | Normal | 125 | 13+4/17 | 7+1/10 | 13+5/18 | 16+2/18 |
| Hunting revolver | Explode Only | 58 | 19+0/25 | 0+0/34 | 33+0/33 | 19+0/30 |
| Hunting rifle | Explode Only | 52 | 22+0/27 | 0+0/32 | 34+0/34 | 22+0/28 |
| Sniper rifle | Explode Only | 45 | 17+0/26 | 0+0/34 | 38+0/38 | 19+0/29 |
| Anti-materiel rifle | Explode Only | 110 | 20+0/24 | 0+0/38 | 37+0/39 | 22+0/26 |
| Hunting shotgun | Explode Only | 70 | 7+0/8 | 1+0/32 | 30+0/30 | 14+0/15 |
| Laser rifle | Dismember Only | 22 | 11+9/31 | 0+0/35 | 18+16/34 | 11+6/32 |
| Varmint rifle | No Dismember/Explode | 18 | 0+0/16 | 0+0/35 | 13+0/26 | 6+0/23 |


Standing (no sneak attack; criticals only by chance, 26 % of these kills). This Machine's
standing head session crashed the original four times at different points (its sneak torso
session with Bloody Mess once); its row counts the kills before each crash:

| Weapon | On Hit | Dmg | Head | Torso | Head, Bloody Mess | Torso, Bloody Mess |
|---|---|---|---|---|---|---|
| 9mm pistol | Normal | 16 | 4+3/9 | 0+0/5 | 11+4/15 | 9+0/11 |
| .357 revolver | Normal | 26 | 5+2/8 | 0+0/16 | 8+1/10 | 8+3/13 |
| Lucky | Normal | 30 | 8+6/16 | 0+0/16 | 8+3/13 | 15+0/19 |
| This Machine | Normal | 55 | 9+10/26 | 0+0/21 | 11+9/20 | 18+2/26 |
| Plasma rifle | Normal | 47 | 5+5/14 | 0+0/21 | 12+6/19 | 18+1/23 |
| Missile launcher | Normal | 125 | 1+1/2 | 1+0/1 | – | 1+1/2 |
| Hunting revolver | Explode Only | 58 | 7+0/11 | 0+0/16 | 13+0/13 | 18+0/24 |
| Hunting rifle | Explode Only | 52 | 3+0/8 | 0+0/21 | 16+0/16 | 23+0/28 |
| Sniper rifle | Explode Only | 45 | 10+0/11 | 0+0/21 | 8+0/8 | 20+0/24 |
| Anti-materiel rifle | Explode Only | 110 | 22+0/30 | 0+0/31 | 21+0/21 | 22+0/32 |
| Hunting shotgun | Explode Only | 70 | 4+0/4 | 0+0/28 | 6+0/6 | 25+0/27 |
| Laser rifle | Dismember Only | 22 | 4+3/10 | 0+0/17 | 5+7/12 | 2+2/7 |
| Varmint rifle | No Dismember/Explode | 18 | 0+0/4 | 0+0/15 | 3+0/6 | 7+0/12 |

What the counts say:

- **Overkill does not matter.** Head kills without Bloody Mess went 65–75 % of the time in every
  band of overkill ÷ the head's health share, from under 0.5 to over 4; so did kills that did
  not cripple the head (19 of 27). The owner's "overkill explodes a limb" (§1) is the high
  damage weapons being Explode Only (§2.1), not the damage.
- **Without Bloody Mess the torso never goes** (1 of 340 bullet kills sneaking, a plasma kill;
  0 of 175 standing), and the head goes about three times in four: sneaking, Normal weapons 87
  of 123 (71 %) and Explode Only 85 of 110 (77 %); standing, 47 of 59 (80 %) and 46 of 64 (72 %).
  Arms hit by the killing shot went 5 of 9. Sneak attacks and criticals change none of it.
- **The weapon's On Hit decides the way**, not its ammunition: the .308 This Machine (Normal)
  severed 7 of the 25 heads it took, the .308 hunting and sniper rifles (Explode Only) none.
  Normal weapons (9mm, .357, Lucky, This Machine) sever 91 of the 299 head parts they take
  (30 %), with or without the perk, sneaking or standing. One cell stands out: standing kills
  that were not criticals, 16 of 30 severed; no trend with overkill or damage explains it
  (30 %, 33 %, 18 %, 34 % across the overkill bands), so it is taken as chance. No
  Dismember/Explode (the varmint rifle) took nothing in 70 kills.
- **Bloody Mess** takes the head every time (sneaking, Normal 110 of 110, Explode Only 172 of
  174; standing 55 of 58 and 64 of 64) and the torso: sneaking 168 of 211 (80 %) when the hit
  crippled it, 16 of 75 (21 %) when it did not; standing 163 of 204 (80 %) all told. It overrides No Dismember/Explode at about half the rate: the varmint
  rifle exploded 13 of 26 heads and 6 of 23 torsos.
- **Energy weapons** kill with their critical effects: every plasma sneak kill was in the goo
  stage and every laser one disintegrated (IsInCriticalStage), so their "gone" counts are not
  dismemberment and are left out of the fit.
- **Explosions** (the missile launcher) take several parts at once: head and arms, both legs,
  without the perk never the torso; with Bloody Mess often every part (8 of 18 head kills).

The owner's two nvforever kills with Lucky from sneak both blew apart (2026-10-25). In the
original, base Lucky head kills from sneak take the head 76 % of the time (22 of 29; 18
exploded, 4 severed) and torso kills never (0 of 28). The old NVForever rule gave such a head
kill 100 % and a torso kill 100 %: the torso part was the error.

### 4.2 The rule (NVForever)

`fnv_combat::dismember`. Two hits can dismember:

- a hit that **kills** its target;
- a hit on a **corpse** ("KillActor can be called on dead actors to dismember limbs"
  [documented: GECK wiki KillActor]; shooting corpses dismembers them, an everyday sight in the
  original). A corpse hit uses the killing hit's odds, its part counted as crippled
  [hypothesis: not measured].

The hit must land on a body part: a projectile's or the player's blow's; since 2026-10-25 a
blast's, the part the line from the blast's centre to the actor first meets, and an NPC's
blow's, the part a short line along the blow meets at its landing point [design: as a
shot's part is the one its ray meets; `nvf_play::combat::facing_part`]. A blast is judged
by its weapon's On Hit (most grenades and every mine are No Dismember/Explode, §2.1). Until
#2346 a blast never reached a corpse whose ragdoll had been built (its hit volume was gone, and
blasts looked only at volumes); it now reaches it by its ragdoll's bodies and judges the part
facing it, and logs for each corpse it catches whether the part was checked or why not (no
source weapon, no part facing it) [design]. A blast with no source weapon rolls nothing: a
placed explosion, and a mine with no actor owner (`nvf_play::mines`). Both that gate and the
mines' No Dismember/Explode are open until observer check 136 (a frag mine and a frag grenade
beside a settled corpse in the original) says what the original does. Then
one roll decides, with these odds [observed, §4.1, unless tagged]:

1. **A severable part** (head, arm, leg): 75 % on a kill, 100 % with the attacker's Bloody
   Mess. Overkill and crippling do not change it.
2. **The torso** (explodable only): never without Bloody Mess; with it 80 % if the hit crippled
   the torso (its condition at or below 0 after the hit), 20 % if not.
3. **No Dismember/Explode** weapons: nothing without Bloody Mess; with it half the odds above.
4. **Which way**: Explode Only explodes; Dismember Only severs [hypothesis for the rate: the
   laser's kills were disintegrations]; Normal severs 30 % of the parts it takes and explodes
   the rest; Bloody Mess explodes what the weapon could not take otherwise (No
   Dismember/Explode, the torso).
5. The part and every severable or explodable part sharing its actor value are gone (§2.2).
   Only the Explodable ones among them explode; the others are severed.

The masters' settings of §3 are not read: iCombatExplodePartChance 75 equals the measured
chance and iMess* names fit Bloody Mess, but no reading of them gives every measured rate
(the head's 75 % without the perk and 100 % with it, the torso's 0 % / 80 % / 20 %) [hypothesis].
Only fExplodeLimbRemovalDelay is read.

Rolls use their own generator (`nvf_play::dismember::Dismemberment`) so other rolls, and
smoke timings, keep their sequences.

Where it runs [design]: combat queues a check for each hit that may dismember: a projectile on
a part, the player's melee, the hit test aid with `limb=`, and KillActor's limb.
`nvf_play::dismember::resolve` rolls the checks, records the result in the game, logs
`dismember: <name> <id>: <part> exploded|severed (<why>)` and sends `Dismembered` to the
renderer.

A part that is already gone cannot be dismembered again. Hits on its stump do nothing more.

## 5. What a gone part does

State (`fnv-game`, saved): per reference, the gone locations (GetHitLocation numbering, 0–14)
and whether each was severed or exploded (`RefExtra::limbs_gone`).

- **IsLimbGone** `a [b]`: 1 when location `a` (or any of `a..=b`) is gone [documented: GECK
  wiki IsLimbGone].
- **KillActor** `killer limb cause`: a limb given and ≥ 0 is dismembered on the kill (and on an
  already dead actor). It explodes if the part is Explodable, else it is severed [hypothesis: the
  wiki's "their head is destroyed" for limb 1, and "If a body part is not checked as
  Explodable, neither it nor any body parts that share its ActorValue will explode when
  dismembered"]. Severable or Explodable parts only.
- **GetKillingBlowLimb**: the location of the hit that killed the actor, −1 if no hit did
  [documented: GECK wiki]. The record is `RefExtra::killing_hit`, written only by a death
  through `Game::hit` (also when an OnHit script or the hit's Script effect took the last of
  the Health); KillActor, an effect's or a script's damage and a planted explosive leave none,
  whatever wounds came before, and GetHitLocation still reads the last hit
  (`RefExtra::last_hit`) [design: `crates/fnv-game/src/combat.rs`]. A save from before the
  record reads −1: what killed is unknown and is not taken from the last hit [design].
- **Dismembered Limbs** misc stat (36): +1 per part gone by the player's hit
  [hypothesis: counted per part, the player's kills only].

The engine (`engine/nvforever/src/dismember.rs`) [design]:

- **Skins.** Each skinned shape with a `BSDismemberSkinInstance` keeps its partitions'
  body parts and triangles. When a part is gone, the body's shapes draw without that part's
  partitions **if it exploded**, and draw its torso cap. A severed part keeps its partitions
  and section cap; they leave with the severed bones. Shapes without partitions whose bones all
  lie in an exploded part's bone tree (head, hair, eyes, hats, gloves) are hidden.
  (`headhuman.nif`'s skin has partitions 1 and 1000, so the head goes by them [observed:
  `fnvtool nif decode`].) The shapes are drawn again whenever another system gives a
  redrawn shape a mesh back: the moving face (`lips.rs`: blinks, glances, lip sync) gives
  the head, eyes and hair meshes of their own while it runs and hands their meshes back
  when it lets the face go, at death. That put an exploded head back on the body (the
  owner's playtest list 7 item 26, 2026-10-23) [observed in code]. Log: `dismember: <ref>'s
  shape was given its whole mesh back: drawn without its gone parts again`.
  A model with no cap for the part shows the open cut (owner: "some may not have meatcaps").
- **Cutting a severed part's skin apart.** Vertices near a joint are weighted to bones on both
  sides of the cut, so a triangle there stretched from the body to the flying piece (owner,
  2026-10-22: a severed head hung on a long strip of the neck and collar). A shape skinned to
  bones of both sides is now drawn in two: the piece's triangles (the part's partitions, its
  torso section and its section cap [hypothesis: the torso section leaves with the part, as it
  goes when the part explodes]) and the body's (the rest, with the torso cap). Each triangle has
  vertices of its own, weighted only to bones on its side (the weights scaled back up to 1); a
  vertex with no weight on its side is carried whole by the piece's bone nearest the cut, or for
  the body by the nearest bone above the cut. A shape without partitions (head, hair, hat) is
  split triangle by triangle, by the side most of its corners' weight lies on [design].
  Log: `dismember: <ref> cut apart for severed [<locations>]: <n> shapes, <m> triangles leave
  with the pieces`.
- **Severing.** The ragdoll joints between the part's bones and the rest are cut. The limb
  falls away with its own skin. The cut survives the ragdoll's rest and rouse rebuilds.
- **Exploding.** The part's skin goes. `fExplodeLimbRemovalDelay` (0.03 s) is not applied: the
  skin goes in the frame the explosion appears. At the gore bone (`NAM4` + the gore offset)
  appear:
  - the part's explosion (visual and sound; LimbExplosion's damage is 0);
  - `decal count` blood impacts of its impact data set, sprayed about the spot. Decals
    themselves are not drawn yet (shot_fx.rs);
  - each piece of the limb replacement model, at its scale. Each piece is a separate
    flying body;
  - `debris count` debris pieces, each model picked by its percentage.

  The pieces start at the gore bone with speeds outward and along the shot, spin, bounce
  and settle (§6).
- **Where the pieces start.** The explosion and the blood go at the gore bone plus its offset,
  in the bone's axes (the GECK's "blood spurt" positioning). The replacement model's origin
  goes at the gore bone itself. The model is drawn in the actor's axes as they stand in the
  bind pose, then turned as far as the bone has turned from its bind pose [design]. The models
  are built that way [observed]: GoreLegGore01's pieces hang 15, 41 and 59 units below their
  origin, and HeadBits01's lie within 12 units above it. So an upright actor's leg bits hang
  from the hip, and a lying corpse's lie along its leg.
  Checked in the `dismember-chet` smoke run (2026-10-20): the head bits started within 12 units
  of `Bip01 Neck1` as Chet died standing. The leg bits of his lying corpse started within 59
  units of `Bip01 L Thigh`, the lowest at z 549, just above the floor. An earlier version
  placed the model in the bone's axes plus the blood offset, and its leg bits started under
  the floor.
- **Severed blood**: a spurt of the severable impact data set at the cut.

## 6. Gore pieces [design]

- Each piece is a dynamic physics body (`fnv_phys::gore`): a box from its model's bounds, 0.5 kg.
  It starts at 150–350 units/s in a random direction biased along the hit and up, with a
  random spin up to 12 rad/s. Pieces collide with the world and each other. They do not collide
  with characters, living actors or corpses.
- **Lootable**: a piece belongs to its actor. Its collider is owned by the actor
  (`Owner::Actor`, layer of the corpse), so the crosshair names the dead actor and
  activating a piece opens its inventory (Search) [observed: owner].
- Pieces stay while their actor is loaded. At most 64 gore pieces exist at once
  [hypothesis: `iDebrisMaxCount` is not in the masters]; the oldest go first [documented:
  GECK wiki Debris, for debris].
- The pieces of a replacement model are the nodes right below its root, each with the shapes
  under it. Each box is the bounds of its shapes. Debris models are picked in proportion to
  their percentages [hypothesis].
- Exploding the **torso** draws none of the body. The invisible ragdoll stays, so the corpse is
  still searched where it lies.
- An exploded part's ragdoll bodies stay jointed and undrawn. The body's skin partly weighted to
  them keeps its shape.

## 7. Not built

- A blast takes several parts at once in the original (§4.1); NVForever judges one part per blast.
- Blasts on corpses (§4.2, #2346): whether a mine or grenade beside a settled corpse takes its
  parts in the original, how often and how many, is not known (observer check 136); the mine
  source-weapon gate and the weapon's On Hit for blasts stay as they are until then.
- Melee odds (blades, hammers) are not measured (§4.1; the owner set them aside, 2026-10-25).
  How to drive them: `player.PlayGroup AttackRight 1` / `AttackPower 1` play the swing but never
  hit, and FireWeapon with a machete does nothing; with the game window held in front (both game
  locks taken, no smoke run) `TapControl 4` swings land (a machete trial killed a target).

- Decals (blood on the ground and walls).
- Pins Limbs projectiles (two in the masters) pinning a severed part to a wall.
- Creature gore beyond the BPTD data: insect and robot bits come from their own BPTD records and
  need nothing more, but no creature has been checked by eye.
- VATS: "targeted limb" bonus (iMessTargetedLimbExplodeBonus is 0) and the VATS removal delay.
- Weapons dropping from an exploded hand.
- Living actors losing limbs: dismemberment here happens only on the killing hit or on a corpse.
  The original does not dismember living actors either [observed: no target in §4.1 lost a part
  to a hit that did not kill it].
