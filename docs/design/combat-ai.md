# Combat AI: actors in Combat attack

Owner request 2026-10-11: "start on the combat AI so NPCs in Combat attack". Builds on the
detection states (docs/design/alert.md: Normal, Alert, Combat, Lost) and on the player's
combat (docs/design/combat.md, steps C1–C8). Combat is not a parity target (owner
2026-10-09): the mechanics here are **[design]** unless tagged otherwise; what scripts see
follows the documentation.

This is step **CA1**: actors attack with what they hold, close in to where they can, and
the player can be hurt and die. Later steps: §7.

## 1. One rule set for every attacker

combat.md §1 asked for it; CA1 makes it so. Every projectile in flight carries who fired
it, the weapon and the ammunition (`nvf_play::combat::By`), and its hit is that actor's:
its skill, Strength and Luck in the damage formula, its weapon's condition, the
attacker in OnHit and `GetLastHit…`, the owner of the impact's detection event. Hits by
others give the player no hit marker, no kill XP and no HUD line.

| Message | From | Does |
|---|---|---|
| `ActorFire` | nvforever `npc_combat` | one shot of an actor's gun: projectiles into flight (`shot::fire`, as the player's), the 3D shoot sound, a muzzle flash where it leaves |
| `ActorStrike` | same | one melee blow: the weapon's damage (else the Fists'; a creature's own CREA DATA Damage in their place) against the target's armour, applied with `Game::hit` |

Actors do not spend ammunition yet [design: until looting balance is looked at]. Their
shots are aimed from the eyes and drawn from the weapon's muzzle (§9).

## 2. The player is a target

- Shots fired by other actors hit the player's character capsule; the player has no
  hit volume of its own, so a hit counts as the torso [design]. The player's own shots
  and other rays (blasts, picks) pass it as before.
- Damage goes through `Game::hit` on the player's Health; armour the player wears counts.
- At 0 health the player dies: every control is held (`ScriptState::locks`), the screen
  says "You are dead." The death camera (`engine/nvforever/src/death_cam.rs`, owner
  request 2026-10-16) [design, every constant]: the body drops as a ragdoll; in first
  person the view stays fixed to its head, in third person the camera keeps its place
  and turns to the body (owner 2026-10-25); the game slows to a quarter speed for about 4 s and the picture blurs
  (`--death-imod` also plays an image space modifier). Reloading the last save is not
  built.
  Those fighting the player see their target dead and go back to Normal (alert.md §3).
- Test aid `NVF_PLAY_TEST_GOD=1`: the player takes no damage (the hits are still logged),
  so long unattended runs are not ended by a fight.

## 3. What an actor fights with

nvforever `npc_combat`, for every drawn living actor in Combat with a target:

- the weapon drawn on it (`HeldWeapon`): a gun if it fires a projectile, else melee;
- a creature's weapon: super mutants and nightkin (creatures on the `smspinebreaker`
  skeleton) draw and fight with the weapon they carry where their skeleton folder has its
  stance (`2hm`, `2hr`, `2ha`, `2hh`, `2hl`; no `1hm` or `1hp` there) (playtest list 7
  item 39, `outfits::arm_creatures`, §18);
- a creature's body weapons ("Embedded Weapon": Mister Handy's buzzsaw and flamer,
  Mister Gutsy's plasma gun and flamer, robots' lasers, a fire gecko's flame), the one
  its target's distance calls for (`body_weapons.rs`, §19);
- no weapon: a creature attacks with its own damage (CREA DATA Damage) and reach (RNAM
  Attack Reach) [documented: xEdit field names], from the template that supplies its
  stats [hypothesis for the reach]; an NPC fights with its fists (WEAP `Fists`).

With `--actor-weapons none` NPCs have no weapon drawn on them and fight with their
fists (the default draws it holstered).

## 4. When and how it attacks

- **Only what it sees**: it attacks while its latest detection of the target is above 0
  and in sight (detection.md), after a reaction time of 0.6 s from first seeing it in
  this fight, with its weapon drawn (alerted actors draw it, alert.md §5).
- **Guns** keep their own `fnv_combat::attack::Cycle`, as the player's: the weapon's fire
  rate, magazine and reload. An automatic weapon's trigger is held for a burst and let
  go for a pause (`fnv_combat::burst`, issue #1448) [design: the original's rule is not
  known]: a burst is the V.A.T.S. burst's rounds (0.4 s of the fire rate, 3 to 8), the
  pause 0.5–1.5 s with each end scaled by the combat style's Semi-Auto Firing Delay Mult
  (min, max); a burst cut short (sight lost, a wall, a reload) starts afresh without a
  pause. Embedded weapons (Securitrons, sentry bots) burst too. The trigger is pulled
  again for each shot of the others. Log: `npc combat: <name> fires a burst of N rounds
  at <target>, then lets go for S s`.
- **Reloads** (owner 2026-10-17: NPCs never reloaded in test runs): an empty magazine is
  reloaded at once, fired at or not, with the weapon's reload clips (the player's:
  `1hpreloada.kf`; round by round the opening clip `1hpreloadxstart.kf`, then
  `1hpreloadx.kf` from its start for each round, closing past its `Hit` key) for as
  long as they run in the actor's rig (2 s, the tuning's fallback, until they are
  known). It pulls no trigger while it reloads, so a .357 revolver or a lever-action
  loads every round. The same gun keeps its magazine on a new target. The rounds come
  from nowhere: NPCs use no ammunition from their inventory (owner 2026-10-17). Not
  built: reloading a part-empty magazine in cover or out of sight, the weapon model's
  own reload animation on NPCs (the revolver's cylinder). Smoke `combat-npc-reload`.
- **Aim**: from its eyes at the target's chest, inside a cone of the documented actor
  spread for its weapon skill (its sights, crouch and gait: walking or running as it
  fires widens it, `npc_combat::shot_stance`, #2125), plus 4° that
  settles away over 3 s of firing at the same target (the first shots of a burst miss
  more).
- **Melee**: a blow every 1.2 s divided by the weapon's attack speed, while the gap
  between the two bodies is within 24 units.

## 5. Where it stands

`fnv_ai::alert` moves actors run on their packages (`--npc-packages`); the combat AI tells
it, each frame, how near the actor wants to be and how far its target may be before it
closes in again (`Awarenesses::engage`):

| Fighting with | Stops at | Closes in again beyond |
|---|---|---|
| a gun | 80 % of its reach (§16: WEAP max range × style mult, within the projectile's range), between 400 and 2,500 units, clear of its own blast, never beyond 90 % of its reach | its reach (at most 3,125) |
| melee, a person | 6 units short of its reach (`fCombatDistance` 107 × scale × WEAP Reach) plus the two bodies' radii (two people's, 44, until the bodies are known), centre to centre, never nearer than a 6-unit gap between the bodies; the walk ends 10 % inside that, where it mostly swings (a lead pipe 170, the original 180: combat-rules.md §10, open) | its reach plus the radii (at least a 24-unit gap): a knife 97.5, a lead pipe 178 against the original's 95 and 180 [observed: o14_reach, 2026-11-01, combat-rules.md §10] |
| melee, a creature | 6 units short of (RNAM + its skeleton's reach beyond it) × scale, centre to centre, never nearer than a 6-unit gap | (RNAM + its skeleton's reach beyond it) × scale: a gecko 81, a bighorner bull 232, as the original's swing starts [observed: o14_reach, o12_reach; npc-behaviour-compare.md §3] |
| melee without bodies to measure | 80 % of its reach | its reach |

The bodies are the blocks `nvf_play::blocking` keeps (a dog's reaches well in front of
its feet). Closing in, it **runs** to the point that distance short of the target on the
line between them, not to the target itself, planned again when the target moves 96
units or after 1.5 s. Owner 2026-10-11, watching the first version: "the dogs should be
getting into range to attack then biting not pushing around the player". In Combat an
actor is not pushed aside by others; it stands its ground.

Actors not run on their packages stand where they are and attack whatever is within
their reach and field of view.

## 6. What scripts see

| Function | Now | Source |
|---|---|---|
| StartCombat | the actor fights the target from now on, hostile whatever its records say, until StopCombat; it counts as detecting the target at once (Combat) | [documented: catalogue notes]; the target's combat group: not built |
| StopCombat | the actor leaves its combat state; StartCombat's hostility is undone | [documented] |
| IsInCombat, GetCombatTarget, GetIsAlerted, GetCurrentAIProcedure / Package | as alert.md §6: a fight reads GetIsAlerted 0 (SetAlert's alone); GetCombatTarget keeps the target it fought while it is out of sight, through Lost; Lost reads procedure COMBAT 13 and package COMBAT 5 (#2121) | |
| StopCombatAlarmOnActor | every actor fighting or hunting the calling actor stops (StopCombat) | [documented: GECK wiki StopCombatAlarmOnActor] |
| GetFriendHit | the friendly hits of the target on the actor that still count (§10.1) | [documented: GECK wiki GetFriendHit, Friendly Fire] |
| GetAttacked | 1 once anyone attacked the actor, cleared when it leaves combat | [documented: GECK wiki GetAttacked] |
| SetIgnoreFriendlyHits | its friends' and allies' hits never turn it (§10.1) | [hypothesis: the name] |

The test aid `NVF_PLAY_TEST_SCRIPT=<s>:combat:<actor>:<target>` calls StartCombat
(`0x` before a hex FormID, `player` for the player).

StartCombat is called 938 times in the masters' scripts: quest fights now start for real.

## 7. Checks

- Unit tests: `npc_combat` engage distances and the settling aim; the fnv-ai and
  awareness tests of alert.md.
- In the game, Violet (hunting rifle) and the player at 3,000 units, `NVF_PLAY_TEST_GOD=1`:
  `nvforever --data <Data> --cell Goodsprings --pos=-43900,64896,7000 --play --hour 12 --no-voice --npc-packages FiendVioletRef --run-seconds 45`
  [observed 2026-10-11]: `npc combat: FiendVioletRef 0x000F570B fights the player with
  WeapHuntingRifle (gun)`, `runs at the player, 2650 units away`, `stops and faces the
  player, 1598 units away`, about 15 shots with 3 reloads, about 12 `Violet hits the
  player for 44.7`. Without the test aid the player dies after 5 hits ("the player died,
  killed by Violet", "player controls: all held, the player is dead") and she goes back to
  Normal.
- Melee, two of her dogs told to fight with the test aid:
  `NVF_PLAY_TEST_GOD=1 NVF_PLAY_TEST_SCRIPT=3:combat:0x000F573F:player,3:combat:0x0013E5BB:player`
  `nvforever … --pos=-45600,65000,7000 --play --hour 12 --no-voice --npc-packages 000F573F,0013E5BB --run-seconds 30`
  [observed]: both "fight the player with its own attack (25 damage, reach 58)" and
  (36, 62), run up and stop about 60–70 units off (bodies just apart), about 20 blows,
  4 run/stop lines in all, and the player's step changed by actors about 12 times.

- Smoke scenarios (`tools/smoke/scenarios.toml`): `combat-violet`, `combat-dogs`,
  `combat-sights` (§10.6), `provoked-yield` (§10.1–§10.2); the Fiends' `alert-fiend`
  and `cook-cook-camp` forbid or show no Combat ↔ Lost flicker (alert.md §8).

## 8. Next

- Fleeing, cover, combat groups with their searches and alarms, teamwork and
  NPC-against-NPC fights: built in list 2 (§12). Left: avoiding threats, blocking,
  dodging and power attacks of NPCs, grenades and explosives, combat barks (§12.10).
- StartCombat's group case; OnStartCombat / OnCombatEnd blocks; crimes
  (SendAssaultAlarm).
- Reloading the last save after the death; whether the original's death camera plays
  an image space modifier, and which.
- Weapon choice among several; ammunition use.

## 9. How fighters look (step CA2)

Owner request 2026-10-11: "add attack animations so NPCs aim, fire and bite visibly".
`engine/nvforever/src/attack_anims.rs`; the clip rules are the player body's
(`fnv_actor::player_clips`, docs/spec-notes/animation.md §9.6).

- **Aiming**: a fighting NPC with its weapon drawn mixes its stance's aim clip with the
  up or down form (`2hraimup.kf`, `2hraimdown.kf`) by the pitch from its eyes to the
  target's chest (`aim_weights`: the forms are the poses at ±90°) [hypothesis, as for
  the player].
- **Shots and blows**: each plays the weapon's attack clip, chosen as the player's
  (`attack_stem` of the WEAP attack animation, then `attack_clips`: Violet's hunting rifle
  plays `2hrattack3.kf`), with its own up or down form at the aim's weight; fists are
  `h2h`.
- **Creatures**: their own `h2hattack….kf` clips in the skeleton's folder, power and
  forward attacks left out, taken in turn [design]. A blow is a power attack by the
  combat style's Power Attack % Chance [documented: GECK wiki Combat Style], else 25 %
  [design: owner 2026-10-16, random power attacks where the rule is not found]; normal
  or forward by the style's shares (evenly where both are 0), among the power clips the
  creature has (`h2hattackpower.kf`, `h2hattackforwardpower.kf`: the radscorpion's
  sting). It lands at its own clip's `Hit` key with the creature's usual damage, and an
  unarmed attack effect naming its group goes with it (hit-effects.md §1). The dog has `h2hattackleft`,
  `h2hattackleft_midbite`, `h2hattackright`, `h2hattackright__midbite`; the radscorpion
  `h2hattackleft_mid`, `_high`, `_low` … [observed: the base meshes archive].
- **When a blow lands**: at its clip's `Hit` text key [documented: NiTextKeyExtraData]
  (the dog's `h2hattackleft.kf` at 0.50 s, `_midbite` at 0.27 s [observed]), if the
  target is still within reach then; otherwise it misses [design]. Shots leave at the
  start of their clip.
- **Where shots come from** (owner 2026-10-11: "the shots were coming from Violet's
  face, not the gun"): every drawn NPC weapon gets a marker at its model's
  `ProjectileNode` under the actor's `Weapon` node (`weapons::Muzzle`), so it follows
  the weapon. A shot is aimed from the eyes and drawn from the muzzle, closing onto the
  eyes' line by the aim point, and the muzzle flash and shoot sound are there; this is
  the player's first-person rule (`shot::leaving` without cover), so an NPC whose barrel
  is behind low cover it sees over still hits what it sees [design]. Violet's
  `ProjectileNode` is 89 units from her eyes and 64 below them [observed]; with shots
  leaving the muzzle itself she hit the sheet of metal in front of her 8 times out of 8.
- Layers: the attack clip and its form take blend layers 4 and 5 (the idles' layers: an
  idle stops for an attack), the aim's form layer 3 when the draw cycle is not fading
  there; attack clips win by track priority (55 over the aim's 50).
- Checks [observed 2026-10-11]: the dogs' fight logs `attack anims: 000F573F plays
  meshes\creatures\dog\h2hattackleft.kf (1.03 s, hit at 0.50 s)` and in about 30 s
  `attack anims summary: … h2hattackleft.kf 4, h2hattackleft_midbite.kf 4,
  h2hattackright.kf 3, h2hattackright__midbite.kf 3`; side-on frames 0.25 s apart show
  the dog crouch and rear up at the player's upper body. Violet's: `2hrattack3.kf
  (1.33 s)` per shot, `shots are drawn from its weapon's muzzle`, `9 shots (9 from a
  muzzle, 0 from the eyes)`, 8 hits.
- Not done: recoil of the `##` weapon parts on the muzzle node,
  turning a standing NPC that is not run on its packages to face its target, aiming
  while moving between shots (the movement clips drive the spine above the aim,
  animation.md §9.6).
- Shots take priority over movement [design: owner 2026-10-19, "shooting should always
  take priority"]: a shot's clip is held at its last frame at full weight for 1 s after
  it ends (`attack_anims::SHOT_HOLD`), and a shot that comes meanwhile takes over
  without a fade in. Attack clips drive the arms and spine at 55, above the armed run's
  36 [observed: `2haattackloop.kf`, `2hafastforward.kf`, 2026-10-19], so a fighter firing
  on the move holds its gun where the shots put it while its legs run. Before, a Powder
  Ganger running with a 9mm SMG showed the plain armed run: its 0.17 s attack clip
  faded in and out with each shot and never reached full weight [observed in NVForever:
  owner's playtest 2026-10-19].

## 10. Being attacked, allies, styles and sights (playtest list 2026-10-11)

The owner's playtest list asked for fights started by an attack, yielding, allies
helping allies, NPCs aiming down their sights, a machete that reaches a gecko, and a
comparison of all of combat with the GECK wiki's combat pages (the owner's export
`C:\fnv-private\geckwiki\Combat.xml`: Combat Flee, Combat Package Flags, Combat Search,
Combat Style, Combat Tab, Confidence, Friendly Fire, Yielding; with
GetFactionCombatReaction, StartCombat, StopCombat, GetFriendHit, GetAttacked,
SetIgnoreFriendlyHits, StopCombatAlarmOnActor from the functions export, and Weapons and
Gun Spread Formula (New Vegas) from the Items and Game Mechanics exports). The gap table
is §11.

### 10.1 Assault, friendly fire and the owner's exception (`fnv_game::provocation`)

Every hit an actor survives goes through `Game::attacked_by` before `hurt_by` (engine
`detection.rs`). Only the player's hits start a fight here [design: NPC against NPC is
a later workstream].

- **Neutral** (and an enemy not aggressive enough to attack on sight): turns on the
  player at once, in or out of combat [documented: GECK wiki Friendly Fire]. It holds a
  **grudge**: `would_attack` is true for the pair until it is back in Normal, until
  StopCombat, or until it accepts a yield. It says its Assault line [documented: GECK
  wiki Combat Tab, "Said when the actor is first attacked"].
- **Friend** (reaction 3): out of combat at once (iFriendHitNonCombatAllowed 0); in
  combat after more than iFriendHitCombatAllowed hits (FalloutNV.esm: 4) within
  fFriendHitTimer (10 s), hits closer than fFriendMinimumLastHitTime (0.5 s) counting
  once [documented: GECK wiki Friendly Fire and its defaults; the 4 observed 2026-10-12].
- **Ally** (reaction 2; the player's teammates count as allies [hypothesis]; followers.md §4): never in
  combat (iAllyHitCombatAllowed 1000, "infinite"; GetFriendHit stays at 1); out of
  combat after more than iAllyHitNonCombatAllowed (3) hits [documented].
- **The owner's exception**: a neutral actor already in Combat with someone else, hit
  by the player while the player is fighting too (someone in a combat state has the
  player on its target list), does not turn: "the neutral NPC will not start fighting
  the player" [owner's playtest list 2026-10-11].
- The engine (`provoked.rs`) logs `provocation: … is attacked by the player and fights
  back`, `… lets a hit by the player pass (n counted, m allowed)`, `… ignores a hit …:
  both were fighting someone else`, and a `provocation summary` at exit.

### 10.2 Yielding

The player putting the weapon away (`PlayerControl::weapon_out` going off) is a yield
[documented: GECK wiki Yielding]. Actors fighting the player only because of a grudge
(not their records, not StartCombat) accept it when their fight began at most
fCombatYieldTime (10 s) before: they stop (StopCombat), say AcceptYield, and the grudge
is gone. One that accepted a yield within fCombatYieldRetryTime (600 s) and was
attacked again ignores the next: `provocation: … ignores the yield …: it forgave the
player recently` [owner's playtest list 2026-10-11: "if they have already recently
forgiven and were attacked again then ignore the yield"]. The wiki's retry timer is the
player's, for everyone; ours is each actor's [design: the owner's wording]. Allies that
joined a fight against the player (§10.3) hold grudges too and yield the same way.

### 10.3 Assistance

An actor in a combat state that another detects (band Detected: seen or heard) makes
that other take up its fight against each actor on its target list (and whoever hurt
it) when the other's Assistance helps it: 1 Helps Allies for an ally, 2 Helps Friends and
Allies for a friend too [documented: GECK wiki GetFactionCombatReaction, "An actor's
Assistance setting determine whether it will help an Ally / a Friend in combat or not";
the codes: xEdit AIDT]. In one capture an actor with Assistance 0 stayed out of a
fellow's fight for 7.6 s while one with Helps Friends and Allies joined within 2 s
[observed: npc-behaviour-compare.md §16.6]. Never against one of its own allies or friends ("cannot attack
each other under any circumstances" [documented]). Actors sharing a faction, or placed
from the same base form, count as allies [engineering choice: the rule that keeps them
from fighting each other, alert.md §3]. The helper holds a grudge (`Assist`) and gets an
alarm at the attacker's position (`Cause::Ally`), so it becomes Alert and fights when it
detects the attacker: `provocation: … helps … against …`. `Game::call_allies` runs after
each round of measurements.

Goodsprings' settlers share GoodspringsFaction and have Assistance 2 [observed:
`fnvtool actor values`], so shooting one brings the others that see it; Violet's dogs
share FiendsFactionNV with her (Assistance 2), so they now join her fights.

### 10.4 Combat styles (`fnv_game::combat_style`, schema `ai.json` CSTY)

CSTY is read (CSTD, CSAD, CSSD) [documented: layout xEdit wbDefinitionsFNV.pas CSTY;
meanings GECK wiki Combat Style]; FalloutNV.esm has 84 [observed]. An actor's style is
its ZNAM, from the template that supplies its traits, as the death item is
[documented: GECK wiki Traits Tab - NPC and Traits Tab - Creatures list Combat Style
among the tab's fields, AI Data Tab has none; Template Data, "Use Traits: Use the traits
of the ActorBase"; #2138], else `DefaultCombatstyle` (0x3D) [hypothesis]. Used now (nvforever `npc_combat`):

- **Weapon Restrictions** Melee Only: a gun held is not fired; the actor fights with
  its fists. Ranged Only changes nothing yet (there is no weapon choice).
- **Ranged Weapon Max Range Mult** scales the weapon's max range in the distance the
  actor fights from (§5).
- **Semi-Auto Firing Delay Mult (min, max)**: a semi-automatic gun waits a random time
  between the WEAP's Semi-Auto Fire Delay Min and Max, each times its multiplier,
  between shots ("Determines semi-auto weapon's fire rate when used by actors"
  [documented: GECK wiki Weapons]).
- **Melee decision**: each time a blow is possible the actor attacks with chance
  (Attack % + the Hand-To-Hand bonus when unarmed or a creature + Attack Skill Modifier
  base + mult × skill / 100) × the Attack While / Not Under Attack multiplier, against a
  roll of 1–100; otherwise it holds for a random Hold Timer [documented: Combat Style,
  Advanced]. "Under attack": its opponent has a blow on the way [hypothesis]. With the
  dog's style that is 55 % at skill 50, so dogs bite less often than CA1's fixed rhythm.

### 10.5 Confidence

Once hurt, every random 0 to fCombatThreatRatioUpdateTime (7.5 s), a fighter compares
its threat ratio (its health × damage per second over its target's) with its confidence
threshold (Cowardly 1000, Cautious 0.375, Average 0.1875, Brave 0.0375, Foolhardy 0;
FalloutNV.esm sets the middle three), scaled by a confidence modifier drawn per fight
from fCombatConfidenceModifierMin −0.25 to Max 1.0, not for the foolhardy [documented:
GECK wiki Confidence; how the modifier applies, threshold × (1 − modifier):
hypothesis]. Damage per second is the weapon's damage times its shots or blows per
second [design]; armour is not counted yet. Fleeing was not built when this was written (the engine
logged `npc combat: … would flee …` and counted it); it is now, §12.2.

### 10.6 Aiming down the sights (NPCs)

The owner's list: NPCs aim down the sights "only if the player (target) is at a
distance; anything less than 20 metres they fire from the hip", and "the player and
NPCs should not be able to move at full speed whilst aiming" (the player's half is
`nvf_play::player::aiming_walks`).

- 20 m = 1,400 units at 70 units per metre [observed: gravity fits 9.81 m/s² at 70
  units/m, docs/spec-notes/physics.md §6.3]. A fighter firing a gun with sights (WEAP
  Sight FOV above 0 [documented: GECK wiki Weapons, Sight FOV 0 "essentially disables
  iron sights"]) at a target beyond 1,400 units aims down them; nearer it fires from the
  hip; once up, the sights stay up to 1,350 units [design: no switching at the edge].
  The rule lives in `nvf_play::combat::aims_down_sights`; the player's VATS mode shots
  in third person follow it too (owner 2026-10-23, docs/design/vats-mode.md §11.5).
- Aimed shots lose the unaimed spread penalty, fUnaimedSpreadPenalty [documented: Gun
  Spread Formula (New Vegas)].
- It plays the stance's iron-sights clips: `2hraimis.kf` with `2hraimisup/down.kf` in
  place of the level aim, and `2hrattack3is.kf` (and its up and down forms) for each
  shot [observed: the male skeleton folder has `aimis` and `attack…is` clips for rifles;
  `fnv_actor::player_clips` already chose them for the player].
- It walks while aiming (`Awarenesses::aiming`, `fnv_ai::alert`): closing in it walks,
  and the AI log says `walks at the player, … aiming down the sights`, or, raising its
  sights on a way already planned, `walks on, aiming down the sights at the player`
  (`runs on, sights lowered, at …` when it lowers them). The sights come up only while
  it can fire (it sees its target, its weapon drawn) [design].
- Its shots fired on the move take the formula's moving terms as the player's do
  (fWalkingSpreadPenalty walking, fRunningSpreadPenalty running [documented: Gun Spread
  Formula (New Vegas); docs/design/combat.md, its Gun Spread Formula notes]; before
  #2125 every NPC shot was spread as if standing). Its gait comes from its locomotion
  (`npc_combat::shot_gait`): moving, in any direction, while its velocity at its clips'
  pace reaches the speed a walk clip starts at; running while a run clip plays, the
  walk/run choice of its own clips, so an actor's scale or a creature's pace moves no
  fixed cut-off; a velocity of nought stands even while the run clip still plays, and an
  actor with no locomotion stands. The shot takes the most recent gait `fight` finds,
  read before `locomotion::drive` chooses the frame's clip and unordered against the
  walker's velocity update: up to a frame old at a start, a stop or a change between
  walk and run [design].
- Not used: the WEAP's Sight Usage ("the percentage of fCombatIronSightsDistance" beyond
  which an actor uses its sights [documented: GECK wiki Weapons]), because the owner's
  20 m rule stands in and fCombatIronSightsDistance is neither in FalloutNV.esm nor on
  the wiki; the NPC spread's fNPCMaxGunWobbleAngle term (Gun Spread Formula (New
  Vegas): spread × fNPCMaxGunWobbleAngle + spread, ×16 with the masters' 15) [design:
  NPCs keep the player's formula, its walking and running terms with it; combat is not
  a parity target].
- Log: `npc combat: … aims down the sights, its target … units away (beyond 1400):
  walks while aiming`, `… fires from the hip …`, and in the summary `… down the sights,
  … from the hip, sights raised … times`.
- Check (smoke scenario `combat-sights`) [observed 2026-10-12]: Violet, the player
  3,000 units off and put 1,000 units from her at 38 s: `aims down the sights, its
  target 2599 units away (beyond 1400): walks while aiming`, `walks on, aiming down the
  sights at the player`, later `fires from the hip, its target 1130 units away (within
  1400)`; `18 shots (… 8 down the sights, 10 from the hip, sights raised 3 times)` and
  `2hrattack3.kf 10, 2hrattack3is.kf 8` in the attack anims summary.

### 10.6.1 Checks of §10.1–§10.3

- Unit tests (`fnv-game` tests `factions.rs`, a synthetic plugin):
  `a_neutral_hit_by_the_player_fights_and_accepts_one_yield` (assault, a yield accepted,
  the next refused within fCombatYieldRetryTime, a yield past fCombatYieldTime refused),
  `a_neutral_fighting_someone_else_ignores_a_stray_hit_from_a_fighting_player` (the
  owner's exception), `an_ally_that_sees_a_fight_helps` (Assistance, never against its
  own ally).
- Smoke scenario `provoked-yield` [observed 2026-10-12]: in the general store the player
  shoots Chet (`provocation: GSChetRef 0x00104C79 (neutral) is attacked by the player
  and fights back`, his Assault line "What was that for?"), he fights back with his .357,
  the weapon put away 4 s later: `accepts the yield (3.8 s into the fight) and stops
  fighting`, AcceptYield said; shot again and the weapon put away: `ignores the yield …:
  it forgave the player recently`.

### 10.7 Melee reaches low targets

The owner could not hit a gecko with a machete. The player's swing was a fan of rays
from the eye at eye height, as long as the reach (then fCombatDistance 150 × Reach; 107 × Reach plus the player's
radius since #1334): a gecko
at the player's feet lies under the fan, and a ray pitched down to it spends most of
the reach on the drop from the eye. `fnv_combat::projectile::swing_reaching` adds every
actor whose centre is within the reach across the ground (plus 24 units for its body),
inside the swing's arc, from 32 units below the feet to 48 above the eyes, that a ray
from the eye to its centre meets first [design]. Tall targets are found by the fan as
before.

## 11. The GECK wiki's combat pages against what is built

Status: **have**, **partial**, **missing**; "next" marks the larger AI behaviours left
for a later workstream (2026-10-12). Every behaviour in the second column is
[documented] by the page in the first.

| Page | Documented behaviour | Status | Where / note |
|---|---|---|---|
| Friendly Fire | Neutral: aggro at once when hit by the player, in or out of combat | have | §10.1 |
| Friendly Fire | Friend: in combat after iFriendHitCombatAllowed hits within fFriendHitTimer; out of combat at once | have | §10.1 |
| Friendly Fire | Ally: never in combat; out of combat after iAllyHitNonCombatAllowed hits; GetFriendHit capped at 1 | have | §10.1 |
| Friendly Fire | fFriendMinimumLastHitTime between counted hits; 1000 or more is infinite | have | §10.1 |
| Yielding | Yield by holstering within fCombatYieldTime; only actors not hostile by their records | have | §10.2 |
| Yielding | fCombatYieldRetryTime after a yield accepted | partial | each actor's timer, not the player's (owner's wording) |
| GetFactionCombatReaction | Enemies attacked by Aggressive and Very Aggressive actors, neutrals by Very Aggressive | have | alert.md §3 |
| GetFactionCombatReaction | Allies and friends cannot attack each other | have | alert.md §3; grudges and assistance respect it |
| GetFactionCombatReaction | Assistance decides whether an actor helps an ally / a friend in combat | have | §10.3 |
| StartCombat | Target in combat: the attacker targets its combat group | missing | combat groups (next) |
| StartCombat | Does not work on an ally or friend of the target | missing | StartCombat forces any pair |
| StopCombat | Ends the combat state; the actor may resume when still hostile | have | §6 |
| StopCombatAlarmOnActor | Every actor targeting the actor stops | have | §6 |
| GetAttacked, GetFriendHit, SetIgnoreFriendlyHits | as named | have | §6 |
| SendAssaultAlarm | An assault crime; actors yell Assault | missing | crimes (next) |
| Combat Tab | Assault when first attacked; AcceptYield when a yield is accepted | have | §10.1, §10.2 |
| Combat Tab | Attack, PowerAttack, Hit, Death, Flee, Yield, HealthHalf, Crippled, NoAmmo … and their Responses, with their timers | missing | combat barks (next) |
| Combat Style | Melee decision: Attack % with the Hand-To-Hand bonus, Attack Skill Modifier, under-attack multipliers; Hold Timer | have | §10.4 |
| Combat Style | Melee decision: Block % and its multipliers | missing | NPC blocking (next) |
| Combat Style | Recoil/Stagger and Unconscious bonuses; Attack During Block Mult | missing | next, with blocking and knockdowns |
| Combat Style | Power attacks: chance, by movement or by % chance, fatigue modifier | have | combat-rules.md §10.1; the fatigue modifier §17.3 |
| Combat Style | Maneuver decision: dodge chances and timers, fatigue and encumbrance modifiers | have | §17.1 (when and which way: hypotheses); Acrobatic Dodge not built |
| Combat Style | Cover: search radius, take-cover chance, wait / wait-to-fire / fire timers | missing | next (cover) |
| Combat Style | Ranged Weapon Min/Max Range Mult | partial | max used (§10.4, §16: also the reach it fires within); min: no backing off (§12.6) |
| Combat Style | Semi-Auto Firing Delay Min/Max Mult | have | §10.4 |
| Combat Style | Weapon Restrictions | partial | Melee Only; Ranged Only needs weapon choice |
| Combat Style | Max Targeting FOV | have | §17.5: no new target outside it |
| Combat Style | Combat Radius: no pursuit beyond it | have | §17.4, from where the fight began or it was last hit |
| Combat Style | Ignore Threats; Ignore Damaging Self / Group / Spectators; iAvoidHurtingNonTargetsResponsibility | have | §17.6, §17.7: live grenades only; the setting's value guessed |
| Combat Style | Flee Based on Personal Survival | missing | read; next with fleeing |
| Confidence | Ratings and thresholds; threat ratio from health × DPS | have | §10.5, the decision only |
| Confidence | Health counts armour | partial | armour not counted yet |
| Confidence | Checks every 0–fCombatThreatRatioUpdateTime, only after damage; a confidence modifier per combat | have | §10.5 |
| Confidence | Group threat ratio; the personal survival check | missing | next (combat groups) |
| Confidence | Avoiding threats out of combat | missing | next |
| Confidence | Alarms by Alarm Confidence | missing | next (combat groups) |
| Confidence | Frenzied actors never flee | missing | with fleeing (next) |
| Combat Flee | Door, cover or run away; at the flee spot wait, more confident when hurt or cornered | missing | next (fleeing); `would flee` logged |
| Combat Package Flags | No Combat Alert | have | alert.md §2 |
| Combat Package Flags | Continue During Combat | wrong | built as "the package goes on in every combat state", which pins melee fighters (#2155). The original, by package type [documented, and observed 2026-11-12]: Travel holds only on the way, Guard only while the target is outside the guard radius; the rule to build is alert.md §5.2. No cover on the way; a Use Weapon package fires its own volleys at its own target instead of the ordinary attacks (use-weapon.md §3, #2120) |
| Combat Package Flags | Pretend In Combat | missing | next |
| Combat Package Flags | Follow and Guard radii in combat (fCombatFollowRadiusMult, fCombatGuardRadiusMult) | missing | next, with #2155: Guard radius = package radius × 1.5, at least 384, measured from the post to the target [documented; observed for two Guard packages, not for three with type-flag bit 2]; Follow radius × 3, at least 512 [documented]; alert.md §5.2 |
| Combat Search | Alert and Lost actors search | partial | alert.md §5: each alone, not the combat group splitting the navmesh |
| Combat Search | The combat group's searches; iCombatTargetLostRemoveSearchCount; the hidden-target timers | missing | next (search parties); our timers are alert.md §2's |
| Weapons | Sight FOV 0 disables iron sights | have | §10.6 |
| Weapons | Sight Usage × fCombatIronSightsDistance: when an actor uses its sights | partial | the owner's 20 m rule instead (§10.6) |
| Weapons | Semi-Auto Fire Delay Min/Max for actors | have | §10.4 |
| Weapons | Max Range: actors beyond it do not fire until within it | have | §16 |
| Weapons | Min Range: "they don't need to be closer than this value" | have | nothing to build: it never sends them back (§12.6) |
| Gun Spread Formula (New Vegas) | No unaimed penalty while aiming | have | player and NPCs (§10.6) |
| Gun Spread Formula (New Vegas) | NPC spread × fNPCMaxGunWobbleAngle + spread | missing | [design]: NPCs keep the player's formula |

### 11.1 Left for the combat AI workstream (list 2, "E2")

The playtest list's combat items (list 1: 20, 30, 32, 33, 34) end here (2026-10-12). What
the wiki pages describe and is not built, for the list-2 workstream ("fight, flee, take
cover, assist, teamwork; NPC-against-NPC at scale; caution and search"), in the order
the gaps above suggest:

1. **Fleeing** (Combat Flee, Confidence, Combat Style "Flee Based on Personal
   Survival"): the decision is made and logged (`would flee`, §10.5); the doing is not:
   a door, cover or running away, waiting at the flee spot, coming back more confident
   when hurt or cornered; frenzied actors never flee; armour in the threat ratio.
2. **Combat groups** (StartCombat's group case, Combat Search, Confidence's group threat
   ratio and alarms by Alarm Confidence): actors fighting the same target as one group,
   searching the navmesh between them (iCombatTargetLostRemoveSearchCount), alarming
   friends; today each actor fights and searches alone and assistance (§10.3) is the
   only link. StartCombat should not work on an ally or friend of the target.
3. **NPC against NPC**: hits by NPCs start no grudge yet (`fnv_game::provocation` only
   counts the player's), so a stray NPC shot does not turn its victim; assistance
   already sends allies against any attacker.
4. **Cover** (Combat Style cover chances and timers, Continue During Combat without
   leaving cover) and **Min range** (backing off below Ranged Weapon Min Range Mult).
5. **Melee**: NPC blocking (Block % and its multipliers), power attacks, dodging and the
   maneuver decision, recoil/stagger and unconscious bonuses.
6. **Threats**: grenades and explosives, Ignore Threats / Ignore Damaging …,
   iAvoidHurtingNonTargetsResponsibility, avoiding threats out of combat.
7. **Combat barks** (Combat Tab: Attack, Hit, Flee, Yield, HealthHalf, NoAmmo … with
   their timers) and crimes (SendAssaultAlarm).
8. **Pursuit limits**: Combat Radius, Max Targeting FOV, Follow and Guard radii in
   combat, Pretend In Combat.
9. Weapon choice among several (Weapon Restrictions "Ranged Only"), ammunition use,
   the fCombatYieldRetryTime as the player's timer if the owner
   prefers the wiki's reading (§10.2).

## 12. Fleeing, cover, groups and searching (list 2, "E2", 2026-10-12)

The owner's list 2: "start implementing more sophisticated combat ai, i want npcs to
fight, flee, take cover, assist allies. Create combat scenarios to observe fights
between npcs at scale. Have them work together in order to win. As long as we hit the
main points from geck wiki for functionality we can add more modern designs to combat.
This can be extended to detection and cautious state an npc and its allies are in, e.g.
spreading out to find the enemy, alerting their friends when they sense an enemy
presence." The wiki's points come first and are tagged [documented: page]; what
NVForever adds on top is [design]. Where it lives:

| Part | Where |
|---|---|
| Groups, threat ratios, flee checks, alarms, NPC-against-NPC grudges, shared searches (rules, headless) | `fnv_game::combat_group` (`Awarenesses::tactics`) |
| Fleeing, walking to posts, searching by sector (movement) | `fnv_ai::alert` |
| Cover edges of the navmesh | `fnv_nav::NavGraph::cover_edges_near` |
| Flee decisions, cover, flanks, line of fire (per fighter, with rays) | nvforever `npc_combat.rs`, `combat_tactics.rs` |
| Alarms raised after each round of measurements; the test aid `NVF_PLAY_TEST_UNSEEN` | nvforever `detection.rs` |
| Fight summary (`battle over`, `battle summary`) | nvforever `combat_tactics.rs` |
| Test aid `moveto:` | `nvf_play::script_events::ScriptAid::MoveTo` |

Only actors in the high process take part (the far-world rule of `fnv_ai::process`):
the engine measures, fights and alarms only drawn actors near the player, and
`fnv_ai::process` gives actors out of the loaded area no combat state.

### 12.1 Posts: where the combat AI wants a fighter

A **post** (`combat_group::Post`) is a place the engine's combat AI chooses for a
fighter: behind cover, a peek out of it, a flank place, a step aside, a step back.
`fnv_ai::alert` walks it there (running unless it aims down its sights) and has it stand
facing its target; a post comes before closing in (§5). A post it has no way to is
dropped (`has no way to …: closes in instead`) and the combat AI gives no post of that
kind for 3 s (`NO_POST_SECONDS`); cover it could not reach is not chosen again [design].

Arriving is measured at the actor's feet [design]. When the way to a post ends, or after
30 s (`FLEE_WALK_SECONDS`) since it was planned, feet within 64 units horizontally of the
point the post asks for (`combat_group::POST_ARRIVAL`, the engine's `AT_POST`; a way that
ends short of a point off the navmesh counts while it is within that) are arrival:
`AiEvent::AtPost`, with the point and the feet. Farther off, it failed:
`AiEvent::PostFailed` says why (`TimedOut`, `StoppedShort`), with the point, the feet, the
distance and the seconds; the post is released and the actor holds or closes in (§5) on
that same update. `fnv_ai` keeps no failed state and makes no automatic retry after
release; the recovery is the engine's, as for a post with no way to it (the 3 s above, the
cover discarded, a melee fighter's guard or bound ended). A later try at the place is a new
request from the combat AI. This says nothing about what the place is good for: 64 units
is nearness, not sight or cover. A door wait given up with the walk is told as that
(`AiEvent::DoorWaitCancelled`), not as a passage. The failure is logged for every fighter
(`post failed: <post> (timed out|stopped short) after …`).

The point is the one asked for now, not the one the way was planned to or the place the
way ends: the engine's tactics measure to it too (`AT_POST`), so arrival here means the same
to them. `POST_SLACK` (48 units) only decides whether a new request still is the post the
way was planned to, so a request moved within it plus a way that ended short of the first
request can add up past 64 and fail a way walked to its end.

A separate case is a walk within one assignment. Standing at its post, an actor stays there
until its feet are 96 units from the point (`POST_RETAINED`, wider than arrival so an actor
shoved about near 64 does not flip between the states and fill the log). Past that it plans
the way back to the same post: not a retry after release, as the post was never released.
That walk has its own 30 s and ends in arrival or in the failure above, so it cannot loop on
a way that stops short.

### 12.2 Fleeing [documented: GECK wiki Confidence, Combat Flee]

- **When**: once hurt, every random 0 to fCombatThreatRatioUpdateTime (7.5 s) a fighter
  makes a flee check: its **group threat ratio** (the sum of its group's combat
  strengths, health × damage per second, over the sum of its foes') below its confidence
  threshold, scaled by this fight's confidence modifier (§10.5), and, for a style with
  "Flee Based on Personal Survival", its **personal ratio** ((its health / the foes'
  average DPS) over (the foes' average health / its group's average DPS)) below it.
  Frenzied actors (Aggression 3) never flee. Each fighter's strength goes to
  `Tactics::strengths` every frame. Health does not count armour yet (the gap row stays
  partial).
- **Where to**: in the wiki's order, a door within fCombatFleeMaxDoorDistance (FalloutNV.esm
  1,400) that is not nearer the attacker than the actor is; cover between
  fCombatFleeMinCoverDistance and fCombatFleeMaxCoverDistance (1,000–2,000) from the
  attacker (a navmesh cover edge the attacker is beyond, nearest first); else
  fCombatFleeNormalDistance (1,400) straight away, turning up to 90° either way where the
  navmesh ends [design: the choice among several, nearest first; the turns]. With no way
  at all it is **cornered** and stands. It says its **Flee** line ("always plays").
- **At the flee spot** it waits facing the attacker. Hurt there, or a target it sees
  within fCombatFleeBoostConfidenceTargetRadius (512), its confidence rises one level for
  the rest of the fight and it checks again: it turns to fight, or flees on to a new
  spot. After fCombatFleeWaitTime (30 s) at the spot it checks again without a boost
  [hypothesis: the wiki lists the setting without saying what it does]. Healing is not
  built, so the wiki's third case does not happen.
- It does not attack while it flees.
- Log: `npc combat: … flees from … : group threat ratio … below its confidence …`,
  `ai … flees from … away / to cover / through door …`, `reached its flee spot`,
  `is cornered`, `turns to fight … again (hurt at its flee spot …)`, `flees on`.

### 12.2.1 Running and hiding (owner request 2026-10-17)

The owner: fleeing NPCs should run with the original's flee animations and "run and
hide, crouching down when they stop, to hide from their enemies".

- **The run**: the game data has no flee run set (docs/spec-notes/idles.md §13.1)
  [observed]. A fleeing actor **holsters its weapon** [owner 2026-10-17] and runs with
  the unarmed run, with the cower idle's arms and head laid over it (its hands over its
  head) [owner 2026-10-17: "they'll cower whilst running but they only crouch when
  stopped"; the clips' priorities make it so, idles.md §13.2]. It draws again when it
  fights again.
- **Where it runs**: the wiki's order (above) already prefers a door, then cover the
  attacker is beyond (a navmesh cover edge), and only then open ground.
- **At its flee spot, or cornered, a person hides crouched** [design: owner
  2026-10-17]. `Game::sneaks` says so in any combat state (`Game::at_flee_spot`: in the
  first smoke run Violet was Lost by the time she got there). So it takes the crouched
  set (cross-faded, `Locomotion::switch_set`), is measured as sneaking by detection
  and is a smaller target, whatever it holds. Over that set it plays the original's
  cower idle: the IDLE tree's `CowerIdles` (`3rdP_Cowering.kf`) wants a fleeing actor
  standing still [observed: FalloutNV.esm, idles.md §13.1], and `GetCurrentAIProcedure`
  reads 16 FLEE while it flees [hypothesis, idles.md §13.1]. The idle starts when a
  fleeing actor stands still (its flight's first moment, its flee spot) and goes on
  while it runs. Creatures do not crouch;
  their cower is whatever the tree gives their folder (nothing in FalloutNV.esm).
- **Seen there** (in Combat, where `npc_combat` runs it): if it sees its enemy
  (detected and in sight) for `HIDE_SEEN_SECONDS`
  (4 s) without a break while it hides, it runs on to a new flee spot, which may be
  cover or farther away. It does this at most `HIDE_MOVES` (2) times per flight
  [design]. The wiki's rules above still apply: hurt, or with the enemy within 512
  units in sight, it checks again and fights or flees on.
- **It stands up** when it flees on (a new flight has not arrived), fights again, or
  the fight ends.
- Log: `ai … reached its flee spot from …: hides there, crouched` (`is cornered by …:
  nowhere to flee, hides where it is, crouched`), in Combat also `npc combat: … hides
  crouched from … at its flee spot (… units away, in / out of its sight)` and `flees on
  from … (seen where it hides …)`, `idle: … plays CowerIdles … (Cower)`. The counts are
  in the npc combat summary (`… hid at their flee spot, … ran on when seen there`, in
  Combat only) and in `cower summary`; `alert: … holsters its weapon (it flees)`.

### 12.3 Alarms to allies [documented: Confidence, Alarms; design on top]

After each round of measurements (`detection.rs`), an actor that **started fighting**
(→ Combat) alarms its comrades near it, unless its group outmatches the foe: group threat
ratio at or above (confidence threshold + 0.5) × the alarm confidence multiplier (the
wiki's example, 3: no game setting of FalloutNV.esm names it) [documented]. An actor that
**noticed** a hostile (Normal → Alert) alarms them too [design: the owner's "alerting
their friends when they sense an enemy presence"]. Who hears it: the actors measured in
that round (drawn, near the player) and the other actors of the loaded cells, within
`ALARM_RADIUS` (9,216 units [design on the original's farthest combat allies, 9,026];
2,048 until the owner's playtest list 5 item 31, 4,096 until #972), that are
its own kind (§12.4) or an ally or friend whose Assistance helps it; never against their
own ally or friend. The alarm names the first of the foe's side to start fighting, not
the sender's own foe, and a fighter keeps a target while a member of its group detects
it (docs/spec-notes/combat-groups.md). An alarmed actor gets an alarm at the foe's position (`Cause::Ally`)
and holds a grudge (`Assist`) when its records would not attack the foe; it goes Alert,
searches, and fights when it detects the foe. An alarm passed on by an ally is not passed
on again. This is in addition to §10.3 (an ally that sees a fight joins it). Log:
`combat group: A alarms B about C (noticed it | started fighting)`, `… sends no alarm …`.

### 12.4 NPC against NPC [design on the wiki's Friendly Fire rules]

- A hit by another actor goes to `Game::attacked_by_actor`: a victim that would not
  attack its attacker turns on it (a grudge, its Assault line), as on the player's hit,
  except a stray hit by its own ally or friend, or between two actors both fighting
  someone else (the owner's exception of §10.1, for everyone). The wiki's friendly-fire
  counts (§10.1) stay the player's.
- **Kin**: `would_attack` keeps actors of one kind (the same base, or a faction in
  common) from fighting each other (alert.md §3). That rule had the NCR and the Fiends at
  peace: both are in GeneralIdleFaction, which names no reactions, and their own
  factions do not name each other (NCRFactionNV names 23 factions, FiendsFactionNV 6,
  neither the other: neutral both ways) [observed 2026-10-12]. Now (`Game::kin`) actors are
  never kin when either's factions name the other's an enemy, and a faction in common that
  names no reactions counts only when one of them has no faction that does [design]. So
  Very Aggressive Fiends attack NCR troopers on sight, as neutrals.

### 12.5 Searching together [documented: GECK wiki Combat Search; design for the split]

- A hostile only **noticed** (−20 to 0) is searched for midway between the actor and it
  [documented]; anything else (an event, a lost target, an ally's alarm) where it happened.
- The comrades searching for the same actor's alarm are a **search party**
  (`Game::search_party`): each takes its own sector of the circle round the search's
  centre (index i of n: angles 360°·i/n to 360°·(i+1)/n) [design: the wiki has the group
  split the navmesh area by what they cannot see; sectors stand in]. Three search points
  (`SEARCH_POINTS`) make one complete search of its share [design]; the first within the
  minimum search radius, later ones within the medium one [documented: the radii's
  roles; FalloutNV.esm 512 and 1,024 outside, 256 and 512 inside].
- After iCombatTargetLostRemoveSearchCount complete searches (FalloutNV.esm: 2) it drops
  the alarm's owner, unless the hostile is still being noticed (its alarm refreshed in
  the last second) [documented]. Log: `combat group: … has searched its share for … (n of
  2 complete searches)`.

### 12.6 Cover, flanks and the line of fire

- **Cover** [documented: Combat Style, Cover]: every 10 s a fighter with a gun decides by
  its style's Take Cover Chance whether it uses cover for the next 10 s. Using it, within
  the style's Combat Search Radius (1,024 when 0), never more than 1,024 units from where
  it stands (`COVER_MAX_RUN`: with the styles' radii troopers ran 2,000 units to cover in
  the middle of a fight) [design], it looks for a navmesh cover
  edge (§12.8) its target stands beyond, at least 400 and at most its weapon's range from
  the target, not taken by another fighter, from whose back (40 units in) a ray from the
  eyes to the target's chest is blocked, with a peek point 64 units past one end of the
  edge from which the ray is clear [design: the cover search is not described]. It runs
  there, **hides** for the style's Wait Timer (no shots), steps out to the peek point and
  **waits to fire** for the Wait To Fire Timer, **fires** for the Fire Timer once it sees
  its target, and hides again [documented: the three timers]. Cover whose edge no longer
  faces the target, or out of the weapon's range, is given up. Log: `takes cover … (a peek
  … from it)`, `leaves its cover`, `gives up its cover`. Since §16 the edge is looked for
  within the weapon's reach less the 64-unit peek, and a fighter at its cover whose
  target is beyond its weapon's reach leaves it to close in (`leaves its cover to close in
  on …`) instead of firing from it.
- **Flanks** [design: the owner's "work together in order to win"]: the gunmen of one
  combat group fighting one target, not in cover, spread round it: each gets a place at
  the distance it fights from, turned from the side the group comes from by 0, +35°,
  −35°, +70° … in FormID order, looked at again every 2 s and only walked to when 192
  units or more away. Log: `takes a flank place, 35° round …`.
- **Line of fire** [design]: a fighter does not fire while a comrade (or the player, when
  not its target) stands within 40 units of the line from its eyes to its target's chest;
  it steps 128 units aside, away from them. Bystanders do not hold it back (the Fiends'
  brahmin stand among them: with them counted, the troopers held fire 126 times in one
  fight). Log:
  `holds fire: … stands in its line of fire to …`.
- **A wall in the way** [design: owner 2026-10-23, NPCs in combat shooting at walls]:
  a gun fires only when the shot can reach. Detection does not decide it: it says whom
  an actor knows about and where, and it can say "seen" while a wall stops the shot
  (between its measurements, over the landscape alone where the collision world is not
  built, or with the muzzle below the eyes behind low cover). As the trigger is pulled,
  rays go from the eyes and, where the weapon has a muzzle node, from the muzzle (a shot
  leaves the eye line and, with something between the muzzle and the aim point, the
  muzzle: combat.md §5.1) to the target's chest, else its head; the world's solid
  shapes block them, actors' capsules do not (`detection::clear_sight`). The shot goes
  at the point that is clear. A clear line holds 0.25 s (`LINE_CLEAR_FOR`, an
  automatic's burst); a blocked one holds fire 0.5 s (`WALL_RECHECK`) before the next
  look, so there are no rays every frame. Held, the fighter steps 128 units aside to
  whichever side its eyes see the target's chest from (`wall_step`, a `Sidestep`
  post), else (or once there, still blocked) closes in as on a target it does not see
  (half the distance, `fnv_ai::alert::BLIND_CLOSE_SHARE`); out at a cover's peek point
  it gives that cover up. Melee is not checked. Log: `holds fire: a wall stands between
  it and … (… units); steps aside … / closes in`, `has a clear shot at … again after …
  s`; the battle summary counts `held for a wall (… steps aside, … clear again)`. Test
  aid `NVF_PLAY_TEST_SEE_THROUGH_WALLS=1` has fighters count their targets as seen
  whatever detection says (smoke `combat-wall-hold`).
- **A step back** [design: owner 2026-10-16, "backtracking in order to go backwards"]: a
  gunman whose target is nearer than 256 units (`BACK_OFF_WITHIN`) steps 192 units
  straight back from it, walking, and
  takes another step each time it gets there until the target is 64 units beyond that
  distance. It comes before cover and flanks (the target is on top of it). Log:
  `steps back from … , … units away (within …)`; the summary counts `steps back`. The
  weapon's Min Range is not used: it "tells combat AI they don't need to be closer than
  this value" [documented: GECK wiki Weapons], and the service rifle's 768 had the
  troopers backing away most of the fight (first run, 2026-10-16).
- **Keeping a target** [design]: a fighter keeps the target it fights while it detects
  it, unless another detected one is more than 50 above it in detection value
  (`awareness::TARGET_KEEP_MARGIN`). Before, it took the highest value at each
  measurement, and the troopers turned between Fiends 1,000 units apart every half
  second (334 target changes among 11 fighters in a 52 s battle). For 5 s after turning
  to a target (`awareness::TARGET_COMMIT`) the margin does not apply: it keeps the
  target while it detects it and turns earlier only when that one dies, goes or is out
  of sight [design: owner 2026-10-19, fighters "flicker between targets"], or when
  another is nearer than half its target's distance (`TARGET_COMMIT_NEARER`: without
  it NCR troopers kept firing at Fiends 2,000 units off while melee Fiends charged
  them, took no cover and lost the battle). A target out
  of sight is remembered, and the fighter goes back to it when it shows again within
  the margin.
- **Strafing** [design: owner 2026-10-16, troopers "look like they dont know if they want
  to move or shoot"]: an NPC fighter in Combat is drawn facing its target the whole
  fight, standing or on its way (closing in, a flank, a step aside, a step back); its
  legs play the walk or run clip of the way it goes in its own axes (forward, left,
  right, back: `Locomotion::velocity`, actors.md §8), so it fires on the move instead of
  turning between its way and its target. Not while it flees or runs to new cover: it
  faces the way it runs. Once at its cover it stays facing its target, so it strafes out
  to its peek point and back behind (a `Duck` post, not a `Cover` one) [design: owner
  2026-10-18, Violet turned between her way and her target at a caravan]. Creatures face the way they go. Summary at exit: `fight move summary:
  drawn fighters facing their target moved … s forward, … s sideways (strafing), … s
  back`.
- Melee fighters and creatures close in as before (§5); they flee by the same rules.

### 12.6.1 Cover from the gunman firing at the fighter (2026-10-19)

Before, a gunman looked for cover only from its own combat target. With the 5 s target
commitment (§12.6, "Keeping a target") the NCR troopers at the Cook-Cook camp no longer
turned to LvlFiend3GunVarNV 0x000F9CE7, the only Fiend with a cover edge facing it near
them, and smoke `combat-cover` saw no cover at all [observed: smoke runs of #470]. Cover
is meant to hide a fighter from the fire coming at it, not only from the one it aims at
[design; the original's cover search is not described in the GECK wiki].

- **Whom it hides from** [design]: when it looks for cover (every 2 s while its Take Cover
  Chance roll says so), a gunman tries, in order, each enemy gunman that is firing at it
  (an NPC fighter whose target it is and which has opened fire on it), nearest first,
  then its own target (`combat_tactics::threats`). The first one for which cover is found
  is the cover's `from` actor (`Cover::from`).
- **The cover** (`find_cover`): the edge faces the `from` actor, its back hides the
  fighter's eyes from the `from` actor's chest, and both the `from` actor and its own
  target are at least 400 units from the edge, the target within its reach as before. The
  peek point must still see its **own target**: it fires at its target from there, not at
  the gunman it hides from. Low cover hides its crouched eyes from the `from` actor and
  lets it see its target standing.
- **Range before the candidate cap** (2026-11-01, #1162) [design]: the edges out of that
  range are dropped before the 16 nearest (`COVER_CANDIDATES`) are taken, as the cap is
  there to limit the sight rays. Smoke `combat-cover` failed 3 runs of 3 without it: the
  troopers looked for cover while still 2,000 units or more from the Fiends, and the 16
  nearest facing edges all lay behind them, 2,000–2,700 units from their targets, beyond
  their reach of 1,286–1,736 (50 of 51 edges out of range, all beyond reach) [observed:
  smoke logs of #1292]; the edges in range, nearer the Fiends, were never looked at.
- **Give-up and peek rules follow it**: the cover is given up when its edge no longer
  faces the `from` actor (log `gives up its cover: … is no longer behind it` names that
  actor) or the edge is beyond its reach from its target (§16). Once the `from` gunman
  stops firing at it (dies, turns to someone else, or is out of the fight), the cover
  follows its own target: `from` becomes the target and the facing check goes on against
  it (log `keeps its cover against … : … no longer fires at it`). Peeking and firing are
  unchanged: it waits at the peek point until it sees its own target.
- Log: `takes cover … (a peek … from it), hidden from …, who fires at it`; the battle
  summary counts `… cover taken (… from a gunman firing at it)`.
- The player firing at an NPC is not counted as a gunman firing at it unless the player is
  its target (the player is not a combat-AI fighter); cover from the player comes through
  the target, as before [design].

### 12.6.2 Room round cover spots (2026-10-23)

The owner saw an NCR trooper in cover in Goodsprings stand half inside a house's wall
and drainpipe [observed: owner's screenshot 2026-10-23]. Two things let that happen:

- **The spot** [observed, code]: the spot behind a cover edge was the edge's middle
  `COVER_STANDOFF` (40) units in, and a peek point 64 past an end of the edge, each put on
  the navmesh, and never checked against the collision. Navmesh cover edges are not drawn
  a capsule's width off the wall [hypothesis: some run along a wall or under what stands
  out of it (a drainpipe, a porch post), and a navmesh's outline only follows its
  triangles]; snapping a peek point that falls off the navmesh puts it on the outline
  itself.
- **The walk** [observed, code]: the last 64 units of every walk (`residents::LAST_STRETCH`)
  were the navmesh walker's, which keeps to the navmesh and not the collision (it is there
  for chairs, beds and markers, which the character cannot stand in). A fighter went the
  last stretch to its cover spot through whatever stood there.

The fix [design]:

- `combat_tactics::room` measures the room round a spot: 16 horizontal rays at 32, 64 and
  100 units above the feet (above a step, up to the head), out to 48 units, not counting
  actors or the terrain. A spot inside a solid shape has none (the ray hits at once).
- `combat_tactics::spot_with_room` keeps a cover, peek or bound spot only with
  `SPOT_ROOM` (26: the capsule's 22 and a little) round it. Short of that it is pushed
  out of what is too near once, and failing that tried again up to 4 times 16 units
  farther in from the edge; with none, the edge is not used.
- People in combat (`Core::aims`) are walked by physics to the end of their walks.
- The log says the room round each spot chosen (`takes cover …; room round its cover
  spot N, its peek N`), the room where a fighter stands when it reaches its cover and when
  its wait behind it ends (`in cover at … (reaches|hid) …: room N (clear)`, or `(inside
  geometry)` under the capsule's radius), and in the battle summary how many spots had
  room, were moved to have it, or had none. Smoke `combat-cover-room` expects every spot
  to have room and nobody inside; its fight's cover edges are in the open [observed: all
  25 spots of a run of 2026-10-23 had full room], so the unit tests in `combat_tactics`
  check a wall beside a spot, a spot inside a wall and a kerb below the knee.

### 12.7 Watching fights: the test aids and the summary

- `NVF_PLAY_TEST_SCRIPT=<s>:moveto:<reference>:<marker>[:x:y:z]`: the reference's
  `MoveTo marker x y z`, `player` as the marker for the player: actors from elsewhere are
  put where a fight can be watched (they become residents from there, actor-behaviour.md
  §5.4).
- `NVF_PLAY_TEST_UNSEEN=1`: nobody perceives the player (its pairs and its detection
  events are not measured), so the player can stand by a fight [design].
- **Fight summary** (`combat_tactics::Battle`): sides (comrades together, named by the
  faction of each that names the most reactions), hits and kills by side, flights,
  confidence boosts, cover taken and peeks, flank places, steps aside, shots held,
  alarms, complete searches, fights started by a hit between actors, and the time:
  `battle: X killed by Y`, `battle over after … s` once nobody has fought for 5 s, and
  `battle summary` at exit.

### 12.8 Cover in the navmesh

NAVM's NVCA lists cover triangles; every listed triangle has a nonzero NVTR cover field
and the field's bits are not one nibble per edge in edge order [observed: FalloutNV.esm,
`fnvtool nav survey` "cover": 15,791 listed triangles, all nonzero; nonzero nibbles at
bits 8–11 mostly on internal edges]. The GECK's Find Cover Edges marks edges by the height
of what stands there [documented: GECK wiki Getting started, "Cover edges store data such
as the height of a piece of cover"]. NVForever takes a listed triangle's outline edges
(those leading nowhere) as **cover edges**, the cover standing beyond them
(`NavGraph::cover_edges_near`, `CoverEdge`) [hypothesis]; the height in the field is not
read, so cover is confirmed with a ray (§12.6).

### 12.9 Checks

Unit tests: `fnv-nav` (`cover_edges_are_the_outline_of_listed_triangles_facing_out`),
`fnv-game` (`combat_group` ratios and alarm confidence; factions.rs
`an_actor_hit_by_another_fights_it_but_not_an_ally`,
`an_actor_that_starts_fighting_alarms_its_allies_near_it`,
`a_search_party_shares_the_search_and_gives_up_after_its_searches`,
`a_shared_faction_does_not_make_enemies_kin`), `fnv-ai` (carry.rs
`a_fleeing_actor_runs_away_and_stays_at_its_spot`, `a_fleeing_actor_takes_a_door_near_it`,
`a_fighter_goes_to_its_post_and_faces_its_target`, `a_search_party_splits_the_circle`;
the search test now looks midway), nvforever (`combat_tactics` flank places, line of fire,
peek points).

Smoke scenarios (`tools/smoke/scenarios.toml`), observed on this branch 2026-10-12:

- `battle-ncr-fiends`, `combat-cover`, `allies-alerted` (one run): six NCR troopers put
  1,900 units south of Cook-Cook's camp, the player unseen. The troopers notice the camp
  and alarm each other (`combat group: LvlNCRTrooper 0x0015CE0A alarms LvlNCRTrooperGun …
  (noticed it)`), Cook-Cook alarms his four Fiends when he starts fighting, the Fiends
  attack the troopers on sight as neutrals (§12.4). One run: `battle summary: 2 sides, 11
  actors; FiendsFactionNV (5): 3 standing, 2 dead, 61 hits, 6 kills; NCRFactionNV (6): 0
  standing, 6 dead, 33 hits, 2 kills; … 2 cover taken; 60 flank posts …; 9 alarms, 9 not
  sent; 5 fights started by a hit between actors; first fighter at 6.7 s, resolved in
  62.5 s`. Who wins varies.
- `combat-flee`: a Goodsprings bighorner (Cowardly) shot once: `flees from the player:
  group threat ratio 0.230 below its confidence 1000.0000`, `flees from the player to cover
  … 1476 units from it`, `reached its flee spot … after 8.5s`; a settler that saw it
  joined the fight against the player (§10.3).
- `group-search`: the player sneaks 3,700 units south of the camp and fires once;
  Cook-Cook notices the player and alarms the four Fiends, which go Alert (`saw an ally
  fight the player`), search each in their own sector and log `has searched its share for
  the player (1 of 2 complete searches)` (four lines in one run).

### 12.10 The gap table, updated

| Page | Documented behaviour | Status now |
|---|---|---|
| StartCombat | Target in combat: the attacker targets its combat group | missing (groups exist now; StartCombat still names one target) |
| Confidence | Group threat ratio; the personal survival check | have (§12.2) |
| Confidence | Alarms by Alarm Confidence | have (§12.3) |
| Confidence | Frenzied actors never flee | have (§12.2) |
| Confidence | Health counts armour | partial (not counted) |
| Combat Flee | Door, cover or run away; at the flee spot wait, more confident when hurt or cornered | have (§12.2); healing not built |
| Combat Style | Flee Based on Personal Survival | have (§12.2) |
| Combat Style | Cover: search radius, take-cover chance, wait / wait-to-fire / fire timers | have (§12.6) |
| Combat Search | The combat group's searches; iCombatTargetLostRemoveSearchCount | have, by sectors (§12.5); fCombatSearchTargetHiddenRemoveTime/MaxTime: alert.md §2's timers stand in |
| Combat Search | Almost detected: the search centres midway | have (§12.5) |
| Combat Package Flags | Continue During Combat: "using cover along the way" | wrong reading built (#2155; the rule to build: alert.md §5.2); no cover on the way |
| Combat Tab | Flee line | have (§12.2); the other combat barks: missing |

Left for later: grenades and threats, blocking, dodging and power attacks, combat barks
other than Flee, Combat Radius and Max Targeting FOV, Follow and Guard radii in combat,
Pretend In Combat, weapon choice and ammunition, armour in the threat ratio, healing in
combat, crouching behind low cover (NPCs do not sneak), cover on a Continue During Combat
travel, the navmesh cover field's heights.

## 13. Creature data: aggro radius, cowards, template data (playtest list 4, 2026-10-13)

The owner's list 4, items 14, 18, 19 and 20 (`docs/handoff/archive/playtest-list-4.md` §4).

### 13.1 The aggro radius (`fnv_game::aggro`, item 18)

- **Records** [documented: xEdit AIDT "Aggro Radius Behavior", "Aggro Radius"]: the AI
  data (the "AI Data" template source) turns it on and gives the radius in game units.
  The current package must allow it: "Aggro Radius Behavior: Allow aggro radius behavior
  (if any)" is a package Fallout behaviour flag [documented: GECK wiki Category:Packages],
  read as `fnv_ai::social` reads them (a package without "Skip Fallout Behavior" allows
  them all [observed: packages.md, actor-behaviour.md §345]); no package allows it
  [hypothesis].
- **Behaviour**: an actor that would not attack the player on sight (its aggression
  against its faction reaction, `would_attack`), not the player's ally, friend or
  teammate, that has the player detected inside its radius says GuardTrespass
  [documented: Combat Tab, "a warning … from … Aggro Radius behavior"] every
  fAITrespassWarningTimer (5 s) up to iGuardWarnings (2) times, then attacks: a grudge
  (`Why::Aggro`) [hypothesis: the guard package's rule; GetTrespassWarningLevel counts
  "an aggro radius violation" with the guard and trespass warnings, documented]. Leaving
  the radius resets the count [hypothesis]. The player only [design]. Run with the
  trespass tick (`nvf_play::crime`, four times a second of game time).
- **Who it changes** [observed: `fnvtool actor factions --survey`,
  `placed.aggro_radius_not_hostile_on_sight`, FalloutNV.esm]: 467 placed creatures and
  14 NPCs, among them giant rats (radius 700) 75, coyotes (1,024) 50, bighorners (500,
  700) 80, worker ants (512) 33, bloatflies (1,000) 32, bark scorpions (400) 31, mole
  rats (700) 15 — the Aggressive, Neutral wildlife of factions.md §5 — and the
  Unaggressive gecko hunters of GS02 (650). The 14 NPCs include Primm's ambush gangers
  (768) and NCR snipers with a radius of 25,000 (vHDLVLNCRRangerEliteNCRBattleSniper, the
  Hoover Dam battle).
- **The owner's geckos** (cell -17,-6, VSpawnSpecialTier1GeckoSmallGS02Hunter
  0x0016518B and 0x0016518C, "Young Gecko Hunter"): Unaggressive, Foolhardy, neutral to
  the player, Aggro Radius Behavior on, radius 650 [observed]. Their package list starts
  with VCrGeckoFindFood (Once Per Day, 2 hours, Skip Fallout Behavior with no behaviour
  flag, so no aggro while it runs); after it they patrol and sandbox, which allow it.
  NVForever chooses VCrGeckoFindFood all day (`fnvtool ai day`): a Once Per Day package
  that is running is kept and a Find package never completes here, so these two do not
  aggro yet (owner-review.md, "Playtest list 4: creature data").
- Log: `provocation: … warns the player inside its aggro radius (warning n; d of r
  units)`, `provocation: … attacks the player, who stayed inside its aggro radius`; the
  provocation summary counts both. Smoke `aggro-radius` (a Goodsprings coyote).
- **The look** (owner's playtest 2026-10-24: young geckos showed no warning before they
  attacked) [hypothesis]: from its first warning until it attacks or the player leaves
  its radius (or its sight, at an aggro tick), the actor is alerted (`Game::warning_to`,
  `Game::is_alerted`, GetIsAlerted 1): a person draws its weapon, a creature its unarmed
  stance (`creature_stance.rs`); the AI stops it and turns it to the player
  (`fnv_ai::alert`, `AiEvent::Warn`, back to its package after); standing, a creature
  plays its tree's combat idles (the gecko's hiss and side hops, docs/spec-notes/idles.md
  §14). A trespass warning (`crate::crime`) gives the same look, so a warning owner or
  guard draws its weapon (owner 2026-10-24).
- **The timing and the cycle** (capture `warnings`, 2026-10-24, idles.md §14.3)
  [observed; the split hypotheses]: the attack comes 15 s after the first warning (the
  second warning at 5 s, the attack two timers after it); inside half the aggro radius
  the actor attacks without warning. The original's warners drew and put away their
  weapon or stance every 2-2.5 s; the owner judged that a quirk (2026-10-25): NVForever's
  draw once and hold it, and say a GuardTrespass line at each warning (the Detection
  lines' cooldown does not hold warnings back).
- **Spawned actors do not warn** (#1341, built 2026-11-06) [observed: capture
  o25_coyote, original game, docs/handoff/observer-checks.md item 2]: a CrCoyote
  PlaceAtMe'd at the spot of the placed VERGSCoyote6REF, the player 700 units off (of
  1,024), was in combat at the first sample with no warning, where the placed coyote
  warned for 14.9 s first; the base has the same aggro radius and packages. Spawned
  coyotes on the airport runway did the same at 250 and 700 units (o16_air). So an actor
  created at run time (a dynamic reference, FormID 0xFF……) that the aggro rules above
  would warn attacks at once instead, as one inside half its radius does; who, the
  package, the radius stay as for placed ones [hypothesis: every created actor, people
  too, which no capture tried; why: a PlaceAtMe'd actor has no editor location for the
  radius to be kept around, or it starts alerted]. Placed ones keep warning. Smoke
  `aggro-spawned-no-warning`.
- **Guard packages do not warn** (built 2026-10-24, taken out 2026-10-25): Gomorrah's
  room guards on vGomorrahGuardPackage, with no aggro radius of their own, neither
  warned nor fought the player by the door they guard [observed: capture `warnings2`].
  A guard warns by its own aggro radius (the cash-room guard's 128). GetTrespassWarningLevel
  counts the aggro radius warnings as well as the trespass ones [documented].
- **Attack delays** [observed, `warnings2`]: creatures 15 s after the first warning,
  people 21 s (Gomorrah's cash-room guard, four runs).
  Smoke `aggro-warning-look` (VERGSGecko6REF, a Young Gecko, radius 500).

### 13.2 Cowards stay out (`fnv_game::provocation`, item 14)

An actor whose Confidence is Cowardly ("I will always flee from combat" [documented:
GECK wiki Confidence]) never takes up another's fight, by assistance (§10.3) or by an
alarm (§12.3): `provocation: … stays out of …'s fight: its confidence is Cowardly`, once
per pair. It still fights whom its own records make it attack and whoever attacks it
(then it flees by §12.2: a Cowardly threshold of 1,000). Unaggressive alone does not keep
a helper out [design: the wiki's aggression levels say whom an actor attacks on sight
and whom it might avoid, not whether it assists]. The Goodsprings bighorners (GSBigHorner:
Unaggressive, Cowardly, Helps Allies, in GoodspringsAnimalFaction) [observed] no longer
join the Powder Ganger fight. Running away from a fight it is not in ("avoiding",
Confidence page) is not built (§11, "Avoiding threats out of combat").

### 13.3 Talk on creatures and hostile actors (`nvf_play::activate`, item 19)

The prompt on a living actor (`activate::actor_action`): one hostile to the player (it
would attack the player, or it is fighting the player) shows only its name; a creature
shows Talk only when it has a GREETING line it can say now (`Game::has_greeting`), else
only its name; other NPCs show Talk [hypothesis: the game offers Talk on creatures with
dialogue, Rex and ED-E, and not on animals; not captured]. A name-only target does
nothing on E (`activate 0x… …: nothing to do (name only)`). The reticle draws the name
alone, without the key and the action line.

### 13.4 Data from the template (`fnv_game::templates`, item 20)

Each ACBS template flag now decides where its group is read (factions.md §7):

| Flag | Read from the template's source now | Before |
|---|---|---|
| Traits | GetIsRace, GetIsSex, GetIsVoiceType, GetIsClass, IsChild, child immunity; combat style (#2138; before it, from the AI Data source) | the placed base |
| Stats | health, attributes, skills, level, fatigue, speed; a creature's attack, GetIsCreatureType [hypothesis: the type sits in CREA DATA with the stats] | the same (type: the base) |
| Factions | factions | the same |
| Actor Effect List | not read by the game layer | — |
| AI Data | aggression, confidence, assistance, mood, energy, responsibility, aggro radius | the same |
| AI Packages | the package list | the same |
| Model/Animation | the drawn model, skeleton and animations (`fnv_actor::resolve`) | the same |
| Base Data | the name shown (prompt, loot menu, conversation, hit messages); Essential, Respawn, No Low Level Processing, Invulnerable | the base's (Respawn: the same) |
| Inventory | the simulation's carried items (`fnv_sim::World::set_inventory_sources`; a placed leveled list: its entry's) | the placed base's (none for a list) |
| Script | the script a reference runs (`Game::ref_script`) | the placed base's |

Karma (ACBS) is still the placed base's: which flag carries it is not known.

## 14. Crouching, blocking and closing in (owner request 2026-10-16)

The owner's request: NPCs crouch in combat or block with a melee weapon, by context.

- **Crouching**: "if an npc is firing at a distance at an enemy they can see they can
  crouch to better increase their accuracy and make themselves a smaller target, they
  dont want to be crouching when targets are too close … as they want to be able to
  move/avoid close combat focused enemies and creatures. this doesnt mean an npc should
  immediately crouch and fire … they might want to find something that can be used as
  cover first or just to get out of the open space … if can make it not predicable it
  wants to feel random and authentic".
- **Blocking**: "an npc shouldn't be blocking if their target is ranged they want to get in
  close and get hits in … they cant block a bullet. however if their opponent is also
  melee/close combat oriented then they will attempt to block swings".
- **Closing in**: "melee focused actor npcs should try and close ground quickly but also
  avoid running out in the open for miles unless they have no choice they could also be
  used to cover their ranged friendlies keeping close in order to engage attackers".
- And, sent mid-task: "all the above only affect human enemies not creatures".

Combat is not a parity target (owner 2026-10-09), so everything here is **[design]**
unless tagged otherwise. The cover system it needs already exists (§12.6, §12.8). It is
extended below with low cover and with cover used on the way in.

### 14.1 Who

Only **people**: placed NPCs (`ACHR`), never creatures (`ACRE`), whatever they hold (a
Mr. Gutsy fires a gun, but it does not crouch, block or bound). A person without a
crouched clip set (`locomotion::Locomotion::sneak`, `sneaking.rs`) still counts as
crouching for its aim and its spread, but it is drawn standing.

Every fighter draws three **temperaments** when it starts a fight, uniform in [0, 1).
These and every other roll of this section come from a random sequence of their own
(`Fighters::stance_rng`), so fights where nobody crouches, blocks or bounds (creatures,
gunmen at close range) roll exactly as before. They make two fighters of the same style act
differently, and the same fighter differently from one fight to the next:

| Temperament | Used for |
|---|---|
| `crouch` | how readily a gunman crouches (§14.2) |
| `cover` | how much it seeks cover beyond its style's Take Cover Chance (§14.2) |
| `guard` | whether a melee fighter stays by its gunmen or goes ahead (§14.4) |

### 14.2 Crouching to fire

A person fighting with a gun **crouches** (`Awarenesses::crouching`, read by
`Game::sneaks`: the locomotion takes its `sneakmt…` set, `sneaking.rs`) when all of
these hold:

- it sees its target, and it is not fleeing or staggering;
- it has stood still for a moment: at least `CROUCH_SETTLE` (0.5 s) without moving more
  than 16 units. The AI that moves actors stands every crouched fighter up when it sets
  off for a post or closes in (`fnv_ai::alert`), so it never crouch-walks across a
  fight;
- its target is farther than `CROUCH_NEVER_WITHIN` (600 units, about 8.5 m);
- its target is not close-combat oriented (fighting with a melee weapon, fists or a
  creature's own attack; the player counts by the weapon in use) within
  `CLOSE_THREAT_WITHIN` (1,500 units, about 21 m). Such an enemy coming in stands it up
  at once, so it can move.

It does not crouch the moment it may. It **decides** every random 1.5–4 s
(`STANCE_DECIDE`), with a chance of

`P = (0.25 + 0.65 × crouch) × f(d)`, where `f` rises linearly from 0.4 at 600 units to 1
at 1,400 units (20 m) and beyond.

When it has just settled somewhere (a post reached, a stop), its next decision comes
within 0.8 s (`SETTLED_DECIDE`), so arriving at a firing place is when it most often
gets down. Crouched, it keeps the place it has: its group's flank places (§12.6) are not
looked at again until it stands. In the first battle run (2026-10-16), deciding only on
the timer, the troopers crouched once in 120 s: flank re-planning every 2 s kept them
moving.

Crouched, each decision may instead **shift**: stand up and move 128–320 units sideways
across the line to its target, to one side or the other at random (`PostKind::Shift`,
run). The chance is `0.08 + 0.04 × s`, capped at 0.4, where `s` is the seconds crouched
divided by 4. A fighter that stays put too long gets shot. After a shift it decides
again as before.

When a rule above stops holding (target too near, a close-combat enemy coming in, sight
lost for 2 s, it has to move), it **stands up** at once.

**Cover first.** A person whose style's Take Cover Chance roll fails (§12.6, every 10 s)
gets a second chance of `40 × cover` %. The original's styles leave the chance at 0 for
some fighters (CSRaider), and even they now sometimes look for cover before settling in
the open. The first roll comes when the fight starts (§12.6), so a fighter
that decides on cover goes there before it ever crouches in the open.

**Low cover** extends §12.6's search. Behind a cover edge, if the ray from its standing
eyes to the target is clear but the ray from its crouched eyes is blocked, the cover is
**low**. It takes such cover with its peek point where it stands. It hides crouched for
the Wait Timer, then stands to look for and fire at its target (Wait To Fire, Fire
Timer), then crouches again. Full cover is preferred, as before: an edge is checked for
full cover first.

**What crouching does** (§14.5 lists the code):

- Spread: the spread formula's standing term is dropped while crouched
  (`fnv_combat::spread::Stance::sneaking`, as for the player) [documented: GECK wiki Gun
  Spread Formula (New Vegas)].
- A smaller target: hit capsules follow the skeleton's bones (`limbs.rs`), so the
  crouched pose is lower and narrower [observed: the code].
- Its shots leave from crouched eyes, at `CROUCH_EYE` (0.65) of its eye height
  [hypothesis: the crouched idle's head height was not measured].
- Harder to see: detection measures it as sneaking (`detection.rs`).
- How it looks: the crouched aim clips (`sneak2hraim.kf`, `sneak2hraimup/down.kf`,
  `sneak2hraimis.kf` …) in place of the standing ones, and the crouched attack clips
  (`sneak2hrattack3.kf` …) first [observed: the base meshes archive lists `sneak…aim…`
  clips for every stance and `sneak…attack…` clips; `fnv_actor::player_clips` chooses
  them for the player already].

### 14.3 Blocking

A person fighting with a melee weapon or its fists blocks (a stance whose prefix
blocks, `fnv_actor::player_clips::blocks`: `h2h`, `1hm`, `2hm`). It never blocks while
its target is a gunman ("they cant block a bullet"). Against a close-combat opponent
(melee weapon, fists, a creature's attack, the player with one of those) it makes the
documented **melee decision** in its order: block, then attack, else hold [documented:
GECK wiki Combat Style, Melee Decision and Advanced]:

- block chance = (Block % + Block Skill Modifier Base + Mult × 50 / 100) × (Block While
  Under Attack Mult when its opponent is in a blow, else Block Not Under Attack Mult),
  against a roll of 1–100 [documented: the formula, "Modifier = Base + Mult × (50 /
  100)"]. FalloutNV.esm's DefaultCombatstyle, CSNCRTrooper, CSLegionary and
  CSRaiderMelee give (30 + 20 × 0.5) × 2 = 80 % under attack and 40 % otherwise;
  CSSupermutantMelee's Block % is 0 [observed: `fnvtool esm decode`];
- attack chance as §10.4, plus the Recoil/Stagger Bonus to Attack while the opponent
  recoils or staggers, times Attack During Block Mult while the opponent blocks
  [documented];
- a block is held for the style's Hold Timer, at least 0.6 s [design].

When it may decide is the context:

- each time its blow cycle comes round (`strike_in`), as before;
- and when its opponent **starts a blow** (an NPC's blow queued, or the player's swing
  count going up, `CombatStats::swings`), after a reaction time drawn from 0.12–0.35 s
  [design], provided it is not in its own blow and faces the attacker within 75°. It is
  "under attack" for that decision. A block chosen this way lasts until the blow has
  landed or missed, plus a random 0.3–0.6 s.

The roll and the reaction time make it neither certain nor predictable: a blow may
still get through, and a reaction slower than the blow comes too late.

**What a block does**:

- A melee or unarmed blow on a blocking person (from the player or an NPC), coming from
  within 90° of where it faces, deals what is left after `CombatTuning::block` (60 %)
  of it is stopped (`fnv_combat::damage::blocked`; combat.md §5.4: "Block reduces
  incoming melee damage and staggers an attacker who hits it") [design: the share]. Shots
  and explosions are never blocked.
- A **power attack** (the player's, a creature's, a person's) breaks through the block instead
  (owner request 2026-10-16; `damage::against_block`, combat-rules.md §12): all of it
  is dealt, its attacker does not recoil, and the blocker staggers (its torso hit
  reaction), which ends its block and holds its attacks while it plays. A player whose
  block is broken cannot block for `CombatTuning::guard_break` (1 s) [design].
- The NPC whose blow was blocked **recoils**: it plays its stance's recoil clip
  (`1hmrecoil.kf`, `2hmrecoil.kf`, `h2hrecoil.kf` [observed: the archive]) and cannot
  strike for 1.0 s [design]. While it recoils its opponent gets the Recoil/Stagger
  Bonus [documented: "An actor is recoiling when their strike is blocked"]. The player
  does not recoil yet (player body: not built).
- The blocker plays its stance's block-hit clip (`1hmblockhit.kf`, `mtblockhit.kf` …)
  and, while blocking, holds `{prefix}blockidle.kf` (`mtblockidle.kf` for fists) in
  place of the level aim [observed: the clips exist for `mt`, `1hm`, `2hm` and their
  `sneak` forms].
- The player blocking (Block held with fists or a melee weapon drawn, combat.md §5.4)
  now gets the same reduction against NPC blows from the front. The rule was written
  (`damage::blocked`) but never called.

### 14.4 Closing in, cover to cover, and guarding gunmen

A person fighting in close combat (melee weapon or fists) closes in as §5 says, with
two additions:

**Bounding** (owner: "close ground quickly but also avoid running out in the open for
miles unless they have no choice"). When its target fights with a gun, has a clear line
from its eyes to the fighter's chest, and is farther than `CHARGE_WITHIN` (1,000
units), the fighter looks for a **bound**:

- a navmesh cover edge (§12.8) within `BOUND_RADIUS` (900) of it;
- its back point (§12.6) at least `BOUND_MIN_GAIN` (250) nearer the target than the
  fighter is, and within 50° of the straight line to it;
- hidden from the target's eyes (the ray to the point's chest height is blocked);
- not taken by another fighter.

Of those it takes one of the two that gain the most, at random. It runs there
(`PostKind::Advance`), waits 0.4–1.6 s, and looks again. With no bound to be found it
**charges** straight in ("no choice"), and looks again after 1.5 s in case cover
appears. Within 1,000 units, or once the target is not a gunman or cannot see it, it
charges.

**Guarding** (owner: "they could also be used to cover their ranged friendlies keeping
close in order to engage attackers"). A melee person may stay by a comrade gunman of
its group (one in a fight, within 1,500 units of it) instead of setting off. This needs
its `guard` temperament at 0.5 or above (about half of them), and its target to be a
gunman more than 1,800 units from it and from that comrade. It stands 160 units from
the comrade on the comrade's side towards the target, turned up to 40° to a random side
(`PostKind::Guard`, run), facing the target. It goes for the target (bounding or
charging, as above) as soon as one of these holds:

- the target comes within `GUARD_ENGAGE` (900 units) of the comrade or of it;
- the target is close-combat oriented, so it meets the attacker before it reaches its
  gunmen;
- the comrade dies, flees or stops fighting;
- its patience runs out, a random 10–25 s from when it began guarding.

Taking on someone other than its own combat target (say, the enemy rushing its comrade)
is not built: whom it fights is still the detection states' (§3).

### 14.5 Where it lives

| Part | Where |
|---|---|
| Temperaments, crouch decisions, shifts, the melee decision with blocks, bounds, guards | nvforever `npc_combat.rs`, with the rules as plain functions in `combat_stance.rs` (unit-tested) |
| Low cover, bounds in the navmesh | nvforever `combat_tactics.rs` (`find_cover`, `find_bound`) |
| `Awarenesses::crouching`; the new post kinds Shift, Advance, Guard | `fnv_game::awareness`, `fnv_game::combat_group` |
| Standing up to move | `fnv_ai::alert` |
| `Game::sneaks` reads `crouching` in Combat | `fnv_game::functions::senses` |
| Block fields of the style (CSAD, CSTD) | `fnv_game::data::CombatStyleDef` |
| Blocked blows (NPC and player), the player's block | `nvf_play::combat` (`Combat::guarding`, `Combat::blocked_blows`) |
| Crouched aim and attack clips, block idle, block hit, recoil | nvforever `attack_anims.rs` |

Logs: `npc combat: … crouches to fire at … (… units away)`, `… stands up (…)`, `…
shifts …`, `… blocks …'s blow (reacting|decided)`, `combat: … blocks …'s blow: … of …
stopped`, `npc combat: … recoils: its blow was blocked`, `… bounds to cover …`, `…
charges … across open ground: no cover on the way`, `… guards … against …`, `… leaves
… to go for …`; the summary line `npc stance summary: …` at exit.

### 14.6 Checks

- Unit tests (`combat_stance.rs`): the crouch chance by distance and temperament, the
  stand-up rules (near, a close-combat enemy, no sight), the shift chance, the block
  chance from FalloutNV.esm's default style (80 % and 40 %), no block against a gunman,
  the attack chance's recoil and during-block terms, a block from the front only, the
  guard post and its release, a bound's rules (gain, lane).
- Smoke scenarios (`tools/smoke/scenarios.toml`): `combat-crouch` (the NCR troopers and
  the Fiends of §12.9 crouch to fire, and stand up), `combat-block` (the same two groups
  with `--actor-weapons none`: fists against fists, blocks and recoils), and
  `combat-bound` (melee fighters against gunmen at a distance: bounds or a charge, no
  block).

## 15. Melee combos (owner request 2026-10-16)

Owner: "melee combatant enemies need to be able to chain melee attacks for different
animation combos". The player's swings already follow the attack clips' `a:` text keys
(`fnv_combat::melee`, combat.md §5.4); NPC blows now do too.

- **The keys** [documented: GECK wiki NiTextKeyExtraData, `a:R` plays AttackRight, `a:7`
  Attack7, a blank one a random attack; observed: every melee and fist `AttackRight` clip
  of both rigs carries `a:L`, every `AttackLeft` clip `a:R`; power clips none]. The key's
  time is when the clip may be cut for the next swing.
- **When an NPC goes on** [design]: a blow's clip with an `a:` key brings the fighter's
  next melee decision (§10.4: block, attack by the style's attack chance, or hold) forward
  to that key. Attacking between the key and the clip's end goes on the combo: the clip
  of the group named (right → left → right…; fists alternate hands), cut in at once, as
  the player's swing is. The combo ends on a hold, a block, a recoil (its blow was
  blocked), or the target out of reach until the clip ends; the next blow starts again
  from the weapon's own attack group. So the style's attack chance sets how long combos
  run: at FalloutNV.esm's default 55 % about one blow in two goes on [hypothesis: the
  original's rule for when an NPC chains is not documented].
- **Variants**: each swing takes its `_a` or `_b` clip by a roll of its own (as the
  player's, `fnv_combat::melee::swing_stem`), on a random sequence of its own so other
  fights roll as before.
- **The blow lands at its own clip's `Hit` key** (before, the previous clip's), once
  `attack_anims` has started the clip (`AttackTimes::swings`, a frame later).
- **Creatures** take, on a combo, the clip whose name is `h2h` + the group named
  (`h2hattackleft.kf`, `h2hattackleft_mid.kf`) first where their clips carry an `a:` key;
  otherwise they keep taking their clips in turn (§9). The Goodsprings dogs' clips chain
  [observed in NVForever: smoke `combat-dogs` 2026-10-16, 6 of their blows went on combos].
- **People's power attacks** (built 2026-10-20; rules in
  docs/spec-notes/combat-rules.md §10.1): a melee decision to attack is a power attack
  by the style's Power Attack % Chance with its recoil/stagger and unconscious bonuses;
  the direction by the style's five shares ("Choose Attack using % Chance") or by how
  the actor moves against its facing; its own clip (`1hmattackleftpower.kf`), its
  direction's damage bonus, no combo through it, the PowerAttack line, and it breaks
  blocks (§14.3).
- Checks: unit tests in `npc_combat.rs` (the combo window, people's power attacks) and `attack_anims.rs` (a
  creature's chained clip first); smoke scenario `combat-melee-combo` (the `combat-block`
  fist fight: blows go on combos, `attackleft` clips play).

## 16. A gun's reach and the distance it fights from (owner request 2026-10-18)

Owner: "npc's are firing their weapons out of range noticed cook cook will try and fire
his flamer from cover and starts firing as soon as player is in sight, this makes no
sense as his flamer is limited range and from a far will never hit the player / we may
also need to get npcs to get into an optimal range for shooting e.g. a shotgun wants to
be closer than a sniper rifle or a missle launcher wants to be further away to avoid
blowing self up". Until now a gunman fired at any target it saw, whatever the distance;
our projectiles end at their PROJ range (`fnv_combat::projectile`), so Cook-Cook's flames
died 640 units out while he stood 1,700 away. Combat mechanics are not a parity target
(owner, combat.md); the rules below are the GECK wiki's where it has them.

**Its reach** (`npc_combat::gun_reach`): the WEAP's Max Range times its combat style's
Ranged Weapon Max Range Mult [documented: GECK wiki Combat Style, "modify the
minimum/maximum range value on the combatant's weapon when doing range calculations"],
and never beyond its projectile's Range ("Maximum distance the projectile will fire"
[documented: GECK wiki Projectile]) — the projectile its default ammo fires, as the shot
chain resolves it. A range of 0 sets no limit. Cook-Cook (FiendCookCookRef 0x000F5704,
base 1ECookCook) carries WeapFlamer: Max Range 650, FlameProjectile Range 640, the
default style's multiplier 1.0: reach 640 [observed: FalloutNV.esm].

**No shot beyond it — for flames and explosives only** (owner's playtest list 7 item 40,
2026-10-23). "Max Range: Tells combat AI they don't want to be any further away
than this value. Actors outside this range will not fire their weapon until they are
within distance of this value to the target" [documented: GECK wiki Weapons]. Combat is
not a parity target, and the owner asked for the opposite for ordinary firearms: "remove
the minimum firing distance from all projectile based weapons except the flamer, missiles
and explosives … pistol users running directly at the player and not shooting until
point blank. this completely negates the NPC aim down sights functionality" [design:
owner]. So a gun whose projectile is a flame or explodes (`GunReach::limited`: the
flamer, the incinerator, missile and grenade launchers, the Fat Man, grenade rifles and
machine guns) still holds fire beyond its reach; every other gun fires at a target it
sees out to its projectile's own Range (10,000 for the masters' bullets [observed:
`fnvtool combat weapon`, 10mmBulletProjectile, BuckshotProjectile]), and its reach is only
the distance it fights from (the stop and close-in distances below). A pistol user now
fires from where it first sees its target, down its sights beyond 1,400 units, while it
runs in to 614 units. The distance is the shot's: from its eyes to the target's chest.
Log: `npc combat: … holds fire: … is beyond its weapon's reach (… units, reach …)`, and
`… has … in reach` when it may fire again; the summary's "shots beyond it" now counts
an ordinary gun's shots from beyond its reach.

**No shot into its own blast** [design]: a gun whose projectile explodes (PROJ Explosion
flag and EXPL) does not fire at a target nearer than the explosion's radius. Log: `holds
fire: … is within its weapon's blast (… units, radius …)`. It does not change weapons
(weapon choice is not built); it steps back (below).

**Where it stops** (`GunReach::engage`, told to `fnv_ai::alert` as before, §5): 80 % of
its reach, between 400 and 2,500 units, at least the explosion's radius plus
`BLAST_MARGIN` (64) for an explosive projectile, and never farther than
`GUN_REACH_EDGE` (90 %) of its reach; it closes in again once the target is beyond the
stop ÷ the share, never beyond 90 % of its reach, so it sets off while it can still fire
and walks in aiming (before, it stood until the target was out of reach, and a gun under
500 units stopped at 400 and could stand beyond its reach) [design]. Two kinds stop otherwise, after the original was
watched (below):

- a **flame** (PROJ type Flame) stops at half its reach (`FLAME_RANGE_SHARE`), without the
  400 floor [hypothesis: fitted to one run];
- an **area weapon**, whose explosion's radius is at least a quarter of its reach
  (`AREA_BLAST_SHARE`: the launchers, grenades, grenade rifles and grenade machine guns,
  not the Gauss rifle's 40 or the incinerator's 212), stops just clear of its blast
  (radius + 64) instead of at 80 % of its reach [design].

With the masters' weapons [observed: FalloutNV.esm WEAP Max Range, PROJ Range, EXPL
Radius]:

| Weapon | Max Range | Reach | Stops at | Closes in beyond |
|---|---|---|---|---|
| Sawed-off shotgun | 500 | 500 | 400 | 450 |
| Hunting shotgun, caravan shotgun | 600 | 600 | 480 | 540 |
| Flamer (flame 640) | 650 | 640 | 320 | 576 |
| 10mm pistol | 768 | 768 | 614 | 691 |
| Laser rifle | 1750 | 1750 | 1400 | 1575 |
| Sniper rifle, service rifle | 3548 | 3548 | 2500 | 3125 |
| Grenade rifle (blast 750) | 1536 | 1536 | 814 | 1017 |
| Missile launcher (blast 1000) | 1536 | 1536 | 1064 | 1330 |
| Fat Man (blast 1700) | 2000 | 2000 | 1764 | 1800 |

So shotguns and flamers close in, rifles hang back, launchers keep clear of their blast;
melee is unchanged (§5, §14.4).

**The original, watched** (AI observer campaign `gun_reach`, save nvftest, 2026-10-18,
one run each; GetAnimAction 2/3 = attack; distances flat to the player) [observed]:

| Trooper with | Spawned at | First attack at | Then kept |
|---|---|---|---|
| Flamer (max range 650) | 1,283 | 726 | 164–320 |
| Hunting shotgun (600) | 321 | 328 | drifted out to about 500 |
| Sniper rifle (3548) | 458 | 534 | backed off to 2,150 |
| Missile launcher (1536, blast 1000) | 323 | about 450 | about 550 |

- The flamer trooper did not fire from 1,283 units: it holds fire beyond its range, as the
  wiki says, and our rule does the same. Its first attack began at 726, about 13 % beyond
  the max range [hypothesis: an attack begun on the way in, or a margin; our flame ends
  at 640, so the reach stays 640]. It then fought from 164–320: the flame's stop.
- The sniper backing off to 2,150 supports a long distance for long guns (ours 2,500).
- The shotgun is consistent with 480 (it spawned too near to show holding fire).
- The missile trooper fought from inside its own blast. The owner asked for launchers to
  keep clear of it, so the blast rules stay, but an area weapon now stops just clear of
  its blast (1,064) and not at 80 % of its reach (1,229): nearer the original without
  firing into its own explosion [design].
- In that run PlaceAtMe's distance put the shotgun, sniper and missile troopers 300–460
  units away; the campaign now moves each spawn by an `offset` (and arrives first), for
  a second run.

**Cover** (§12.6): a cover edge is looked for within its reach less the 64-unit peek
step (the peek point lies past the edge), and a fighter at its cover (hiding, waiting to
fire or firing) whose target is beyond its reach leaves it to close in, and does not take
that cover again (`leaves its cover to close in on …: beyond its weapon's reach (… units)`)
[design: owner, "fire his flamer from cover"].

**Closing in straight** [design]: while its target is beyond its reach, a gunman takes
no flank place and no step aside for a friend in its line of fire (it has no shot to
line up); it runs at the target (`fnv_ai::alert`, `runs at …`). Log of each fighter's
first shot in a fight: `npc combat: … opens fire on … from … units (reach …)`.

**A step back** (§12.6): an explosive gun steps back from a target nearer than its
blast's clearance (radius + 64) instead of 256 [design]. The weapon's Min Range is still
not a back-off distance: "they don't need to be closer than this value" [documented:
GECK wiki Weapons].

**Summary** at exit: `npc gun reach summary: held fire … times at a target beyond reach,
… times within a blast; left cover … times to close in; the farthest shot … % of its
gun's reach, … shots beyond it`.

**Checks**: unit tests in `npc_combat.rs` (`a_gun_reaches_its_max_range_and_its_projectile_range`,
`shotguns_close_in_rifles_hang_back`, `launchers_keep_clear_of_their_blast`); smoke
`combat-flamer-reach` (Cook-Cook holds fire from 1,700 units, closes in, fires within 640)
`combat-shotgun-reach` / `combat-sniper-reach` (a shotgun closes in, a sniper rifle
fires from afar) and `combat-sights-beyond-reach` (Violet from 3,000 units: holds fire,
runs in with sights lowered, first shot within reach; `combat-sights` now starts within
her reach). The AI observer campaign `gun_reach`
(tools/capture/observe/gun_reach.campaign.json: an NCR trooper with a flamer, a shotgun, a
sniper rifle and a missile launcher in turn) gave the readings above; a second run with
the offsets would show the shotgun, sniper and missile troopers from afar.

**Not built**: the reach of the other ammo (an NPC fires its weapon's default ammo);
holding fire on a cliff where the shot is beyond reach but the flat distance is not (it
stands within its stop, silent; the AI moves by the flat distance); switching to another
weapon when the target is within the blast.

## 17. The combat style's other fields (ESM gap audit, medium group 1, 2026-10-21)

The ESM audit (docs/handoff/esm-gap-audit-2026-10-21.md, "Combat style") found the CSTY
fields below decoded but unused. Meanings: GECK wiki Combat Style [documented]; field
names and order: GetCombatStyleTraitNumeric / GetCombatStyleFlag [documented]; layout:
xEdit CSTY (schema `ai.json`). FalloutNV.esm has 84 styles [observed: fnvtool esm decode,
2026-10-21]: dodge chance 75 in 32 of them, 0 in 23; rushing attack chance 25 or more
in 66; Max Targeting FOV 0 in 83 (140 in `CSLibertyPrime`); Combat Radius 10000 in 47,
0 in 2; Ignore Threats in 44, Ignore Damaging Self in 15, Group in 32, Spectators in 33;
Melee Alert OK in 4; Cannot Use Stealthboy in all 84. Combat mechanics are not a parity
target (docs/design/combat.md): the documented meaning is honoured, and where the
documentation stops the rule is ours, tagged.

### 17.1 The maneuver decision: dodging and idling (`combat_style::maneuver`)

- **When** [hypothesis: the wiki gives no moment]: each time a melee fighter's melee
  decision (§10.4) holds, neither blocking nor attacking, it also makes the maneuver
  decision, unless an idle or a dodge from the last one is still running. People and
  creatures alike; not while fleeing, staggering or recoiling.
- **The chance** [documented: Combat Style, Maneuver Decision and Maneuver Settings]:
  Dodge % Chance + the Dodge Fatigue Modifier (base + mult × fatigue percent) + the
  Encumbered Speed Modifier (base + mult × its walking speed), times Dodge While Under
  Attack Mult when its opponent has a blow on the way (§10.4's "under attack"), else
  Dodge Not Under Attack Mult; a roll of 1–100 at or below it dodges, otherwise the actor
  stays IDLE for a random Idle Timer.
  - Fatigue percent: 1 − Fatigue / base Fatigue, 0 to 1 ("a fully fatigued actor (100%
    Fatigue Percent, which is 1.0)" [documented]; reading it off the actor value
    [hypothesis]).
  - Walking speed: the AI's alert walking speed in game units a second (86.8 for people
    without their own) [hypothesis: the wiki's "around 110" from GetWalkSpeed is
    Oblivion's; with the masters' −110 base a person's modifier is about −23].
- **Which way** [hypothesis: the wiki names the multipliers but not how they combine]:
  weights back = Dodge Back While / Not Under Attack Mult, forward = Dodge Forward While /
  Not Attacking Mult ("attacking": its own blow on the way), sideways = 1, one roll among
  them. Sideways then rolls 1–100 against Left/Right % Chance [documented]: at or below,
  left or right at even odds; above, no dodge this time, and it idles.
- **How long and how far** [documented: the four timers, real seconds]: a random Dodge
  L/R, Forward or Back Timer; it walks that long at its walking speed [hypothesis], to a
  navmesh point that far left, right, back or forward of the line to its target, facing
  its target as fighters do (people; creatures turn to the way they go, as for every
  post). Forward stops at its melee stop distance (it never dodges into its target); a
  dodge shorter than 32 units is an idle instead [design].
- **Not built**: Acrobatic Dodge % Chance (no documented meaning; 50 in one style).
- Log: `npc combat: … dodges left|right|back|forward from … (… s, … units; … %)`; summary
  counts.

### 17.2 Rushing attacks (`combat_style::rush_distance`)

The fields are named (Rushing Attack % Chance, Rushing Attack Distance Mult) but not
described anywhere we have [documented: GetCombatStyleTraitNumeric]. Our reading
[hypothesis]: a melee fighter closing in rolls Rushing Attack % Chance once each time its
target comes within reach × (1 + Distance Mult) (the mult scales a rush's start beyond
plain reach; 0 is a rush that starts at reach). A rush runs in as any approach does, and its
first melee decision on arrival is an attack: no block, no hold, no attack-chance roll
(the power-attack roll is made as usual). It rolls again only after its target has gone
beyond that distance. Log: `npc combat: … rushes … from … units (… %)`.

### 17.3 Power Attack Fatigue Modifier

[documented: Combat Style, Advanced]: base + mult × fatigue percent is added to the
power attack chance, for people (§10.4, with the recoil and unconscious bonuses) and for
creatures (their style's chance). The masters' 5 and −10 make a rested actor's chance 5
points higher than before.

### 17.4 Combat Radius

"The radius around the combat start location/last hit location that the actor should
not pursue their targets beyond" [documented]. [hypothesis: whose location]: where the
fighter stood when its fight began, moved to where it stands each time it loses health.
[hypothesis: what is limited] the fighter's own way out from that place, not where its
target may stand: it pursues a target farther than the radius from that place until it
has itself come the radius from it; there, while its target is still beyond the radius,
it does not close in further and takes no flank, bound or guard post: it holds where it
is (a gun still fires when in reach). 0 means no limit [hypothesis: two styles, both for
non-fighters]. Until 2026-10-23 a target beyond the radius stopped the fighter where it
began, so a creature shot or set on from farther than its radius never came at all; VCG02
places Sunny's sneak marker, where she and the player take aim at the first well's two
geckos, 3,600 to 3,950 units from them (VCG02CrGecko's style: radius 3000)
[observed: fnvtool world refs, VCG02SunnySneakMarkerREF 0010A1FF, VCG02Gecko1REF/2REF;
smoke npc-sneak-sunny], which made the quest's own shot useless. Log: `npc combat: …
stops pursuing …: … units from where its fight began or it was last hit, … come (combat
radius …)`.

### 17.5 Max Targeting FOV

"Any targets outside of this FOV for the attacker will not be targeted" [documented].
0 means no limit [hypothesis: 83 of 84 styles]. Otherwise a fighter does not take up a
new target more than half the angle off its heading; one it already fights it keeps
[hypothesis: "targeted" is the choice of a target]. It still turns to what it detects
(the AI's hold), so the limit delays a fight rather than barring it.

### 17.6 Explosive weapons: Ignore Damaging Self, Group, Spectators

"Actors normally try to avoid hitting themselves, their fellow group members, and
spectators (unless their responsibility is below iAvoidHurtingNonTargetsResponsibility)"
[documented]. Before a shot of a gun whose projectile explodes, with the blast centred on
its target's chest and its explosion's radius:

- **Self**: within the blast it holds fire (§16, built before), now unless Ignore
  Damaging Self.
- **Group**: a comrade (`Game::comrades`) within the blast holds its fire, unless Ignore
  Damaging Group.
- **Spectators**: anyone else within the blast it would not attack (the player when not
  its target, bystanders) holds its fire, unless Ignore Damaging Spectators or its
  Responsibility is below iAvoidHurtingNonTargetsResponsibility. The setting is in no
  master; we use 30 [hypothesis].

It holds its fire until they are clear and keeps its post. Log: `npc combat: … holds
fire: … would be caught in its blast at …`.

### 17.7 Ignore Threats: getting away from grenades

"The actor will ignore all threats like grenades and exploding cars and will not try to
move away from them" [documented], so without the flag it moves away. Built for live
grenades [hypothesis: what counts]: a lobbed projectile with an explosion (a thrown
grenade, a grenade launcher's round), in flight or at rest, within its explosion's radius
plus 64 units of a fighter's feet. The fighter runs to a navmesh point 64 units beyond the
radius, straight away from the grenade, and says its AvoidThreat line once per grenade
[documented: the topic, docs/spec-notes/combat-dialogue.md]. This comes before any other
post. Only fighters in combat (the AI runs posts in combat); not the player; not the
thrower's own round in flight within its first 0.5 s [design: it leaves from its hand].
**Not built**: threats out of combat, mines, exploding cars, AvoidThreatResponse.

### 17.8 Melee Alert OK and Cannot Use Stealthboy: nothing to build

- **Melee Alert OK**: the wiki calls it "an old field for Oblivion that should be
  removed": with it, a fighter that cannot path to its target and has no ranged attack
  waits for the target to come closer. Ours already does so for every fighter (the AI's
  hold when it finds no way, `fnv_ai::alert::close_in`); the flag changes nothing
  [hypothesis: New Vegas ignores it].
- **Cannot Use Stealthboy**: every style has it and no NPC uses a Stealth Boy in ours.

### 17.9 Checks

Unit tests in `fnv_game::combat_style` (the maneuver decision's chance, direction and
timers; the rush distance; the fatigue modifier; the radius and the FOV) and in
`npc_combat` (the dodge point, the blast check). Smoke: `combat-style-dodge` (two melee
fighters: a dodge and an idle logged), `combat-grenade-evade` (a grenade lands among
fighters: AvoidThreat and a run away logged).

## 18. Playtest list 7, combat AI (W4, 2026-10-23)

Owner's items 12, 13, 36, 39, 40, 43 and 45 (`C:\fnv-private\todo 7.txt`). Combat is not a
parity target; the rules below are **[design]** unless tagged.

### 18.1 AddItem arms NPCs (item 12, Trudy)

Trudy (GSTrudy 0x00104C6C) carries no weapon, only .357 ammunition and her outfit
[observed: FalloutNV.esm CNTO]; VMS16 stage 70 (Trudy helps) and VMS16b stage 70 give
her a shotgun list by AddItem (WithAmmoNVSingleShotgunLoot 0x000F9DCF, WithAmmoNVCaravan
ShotgunLoot 0x00096C91) [observed: the stages' result scripts, `fnvtool script
decompile`]. "Executing AddItem function on NPCs will cause them to review their inventory
and equip the best armor and weapon they have" [documented: GECK wiki AddItem notes];
that review was not built, so an outfitted NPC kept its empty hands until a Combat state
made `style_weapon` arm it; in the gunfight she stood with her fists drawn [hypothesis:
alerted but not in Combat]. Now AddItem on any actor but the player runs
`fnv_sim::World::review_equipment` (the outfitting's own choice of best weapon and
armour; OnEquip/OnUnequip run; log `equipment review: … equips …`). Smoke
`trudy-gunfight-armed`. Open: stage 70 run from outside (`stage:VMS16:70`) moved her out
of the saloon by MoveTo and she was never drawn outside in that run (not chased).

### 18.2 Staggers hold everyone (item 13)

NPCs already stood while a hit reaction played (`residents.rs` holds their walk). Two
gaps: no creature ever staggered, as only the people's `idleanims\mt_hit….kf` names were
looked for, while the creature folders name theirs `specialidle_hit…`,
`mtspecialidle_hit…`, `hitreaction_…` or `h2hhitlegs` [observed: `Fallout - Meshes.bsa`
listing] (`locomotion::FLINCHES` now tries each name in turn); and the player's flinch
(`nvf_play::combat::player_flinch`) only held the trigger: now the walk and jump are held
too (`player::flinch_holds_walk`, log `player: held where they stand while they flinch`).
Smoke `combat-supermutant-club` (a super mutant staggers on its crippled leg and holds
its blows), `crippled-torso-arms` (the player is held).

### 18.3 Feral ghouls' pace, attacks on the run and lunges (item 36)

- **Pace**: a feral ghoul (CrFeralGhoul1A: Speed Mult 100, scale 1) ran at 269 units/s by
  the cazador-fitted 0.70 of its clip pace, below the player's 344, so a running player
  left it behind [observed in NVForever: smoke `combat-ghoul-lunge`]. The owner's guess,
  25 % faster (336), was built as a type share until 2026-10-25 [design: owner]; then the
  original was measured: a feral ghoul runs at its whole clip pace, 384 (charges of 388
  and 393 on the Searchlight Airport runway, actors.md §8.10) [observed], faster than the
  guess, so the type share was taken out and ghouls run 384.
- **Attacks on the run** (`npc_combat::lunge_band`): a creature closing in faster than
  100 units/s may begin a blow before it is within its strike distance, by the ground it
  covers until its clip's `Hit` key (closing speed × hit time, capped at its own running
  speed and at 200 units); where it has a forward power attack clip
  (`h2hattackforwardpower.kf`: ghoul, radscorpion's sting, gecko, deathclaw …) the
  forward power attack's own hit time gives a longer band, a lunge, rolled by its style's
  power attack chance; the ordinary blow goes without the attack-chance roll (a creature
  running in means to strike). Rolled once an approach. The blow lands only
  if the target is within reach at the `Hit` key, as every blow. Before, a creature
  attacked only once it stood within reach, and a failed attack roll made it hold for the
  style's hold timer (0.5–1.5 s): the "awkward delay". Log `npc combat: … attacks … on the
  run from … units (a lunge, closing at …)`; style summary counts. Smoke
  `combat-ghoul-lunge`.

### 18.4 Creatures' weapons (item 39, the nightkin's rebar club)

Super mutants and nightkin are creatures (CREA type Super Mutant on
`creatures\smspinebreaker\skeleton.nif`) that carry weapons; creatures were never armed,
so all of them fought with their own attack ("fists"). Their skeleton folder has the
weapon stances `2ha`, `2hh`, `2hl`, `2hm`, `2hr`, `1gt` (equip, aim, attack, reload) but no
`1hm` or `1hp` [observed]. `outfits::arm_creatures` now draws the weapon the rules have a
creature hold (its best drawable weapon, as an NPC's) whenever its folder has that stance;
the combat AI then fights with it as an NPC does (melee or gun; attack and reload clips
from its folder). A creature whose folder lacks the stance keeps its own attack (log
`creature weapons: … has no stance for it`). In the Jacobstown run every armed mutant
took up its hunting rifle, sledgehammer, nail board or Lily's sword [observed in
NVForever, 2026-10-23]. Not built: dropping a creature's weapon on death or a crippled
arm (NPCs only), the CREA "Weapon & Shield" flag (the clips decide).

### 18.5 No minimum firing distance (item 40)

§16: ordinary firearms fire beyond their reach (out to the projectile's range); flames
and explosives keep the limit. Smokes `combat-sights-beyond-reach` and
`combat-shotgun-reach` rewritten for the owner's new rule (Violet fires from about
3,000 units; before she held fire), `combat-sniper-reach` and `combat-flamer-reach`
unchanged.

### 18.6 Strafing into cover (item 43)

Fighters going to new cover turned to their way there: the facing rule
(`residents.rs`, owner 2026-10-16) left out the Cover post ("running to new cover, out
of the fight for a while"). Now only fleeing fighters turn to their way; going to
cover they face their target and strafe or back there. The fight move summary counts
the seconds on the way to cover (`… s of it on the way to new cover`); smoke
`combat-cover` expects some.

### 18.7 A recorded fight (item 45)

Smoke launch `ncr-fiends` (six NCR troopers against Cook-Cook and four Fiends, the
player unseen), read per fighter from the log [observed in NVForever, 2026-10-23, one
run; resolved in 43 s, 4 NCR and 2 Fiends dead]:

| What | Seen | Done |
|---|---|---|
| Stop-and-go | a Fiend (0x000F9CE7) 1,500 units from a trooper, within its stop distance, logged `stops and faces` / `runs at` 3 times a second for 15 s: its sight of the target came and went behind other fighters, and a fighter closed in on an unseen target at once | `fnv_ai::alert::BLIND_CLOSE_AFTER`: it closes in on an unseen target only after 1.5 s without detecting it |
| Friendly line of fire | 172 shots held for a comrade in the line of fire (Cook-Cook alone 102 lines), against 267 shots fired; 56 steps aside | left: a gunman held this long should step aside sooner or take a flank; the side step waits for its post rules |
| Melee Fiends against gunmen | the two melee Fiends (pool cue, 0x000F9CE8/9) charged 1,500–2,600 units over open ground and one never reached anyone; they changed target mid-charge | left: a melee fighter far from every enemy could take cover from cover (§14.4 does this for people already when cover is found) or keep its first target |
| First shots | troopers opened fire at 1,730–2,030 units, Fiends at 1,410–1,880 (item 40: before, the shotgun Fiend would have run to 600) | — |
| Cover | 1 cover taken in the battle (the styles' Take Cover Chance, rolled every 10 s) | — |
| Staggers | 11 staggers held a fighter's attacks | — |

## 19. Creatures' body weapons (owner request 2026-10-24)

Owner: Mister Handy and Mister Gutsy "werent attacking with the saw, they also need to be
able to switch to flamer/plasma weapon too for their ai". Creatures carry their built-in
weapons as WEAP records flagged "Embedded Weapon" ("Checked if the weapon is a built in
part of the creature" [documented: GECK wiki Weapons]); nothing used them: §3's creature
without a drawable weapon fought with its CREA damage and its folder's `h2hattack…` clips,
and the Mister Handy/Gutsy folder (`creatures\mistergutsy`) has none, so they dealt
damage with no clip at all. The same held for every robot, turret and securitron and for
fire geckos', fire ants', centaurs' and lakelurks' ranged attacks (`fnvtool actor
creature-weapons`: 560 embedded rows, actors.md §10.6).

**The records** [observed: FalloutNV.esm, 2026-10-24]:

| Creature | Weapon | Stance | Range (Min–Max) | Node (NNAM) | Limb (Actor Value) |
|---|---|---|---|---|---|
| CrMisterHandy, CrMisterHandyNV | MisterHandyBuzzsaw (20 damage, reach 0.65) | `1hm` | melee | `Weapon` | Right Attack |
| | MisterHandyFlamer2HL (2 × 8/s) | `2hl`, AttackSpin | 128–600 | `ProjectileNode_HeadLaser` | Left Attack |
| CrMisterGutsy | MisterGutsyPlasma1HP (60 × 2.5/s) | `1hp`, AttackRight | 512–1024 | `ProjectileNode_HeadLaser01` | Right Attack |
| | MisterGutsyFlamer2HL (6 × 8/s) | `2hl`, AttackSpin | 128–600 | `ProjectileNode_HeadLaser` | Left Attack |

Their clips are in `creatures\mistergutsy\specialanims\` (`1hmaim.kf`,
`1hmattackleft_chop.kf`, `1hmattackright_chop.kf`, `…_stab`, `…_uppercut`; `1hpaim.kf`,
`1hpattackright.kf`; `2hlaim.kf`, `2hlattackspin.kf` with up and down forms; equip,
holster and unequip of each), as their CREA KFFZ lists name them; the skeleton has the
`Weapon`, `ProjectileNode_HeadLaser` and `ProjectileNode_HeadLaser01` nodes. Only the
bloatfly and protectron folders also keep stance clips there.

**What is built** (`engine/nvforever/src/body_weapons.rs`; the choice
`fnv_combat::body_weapon`):

- A creature without a drawable weapon gets as its arms each embedded weapon it carries
  whose stance its skeleton has (root or `specialanims`), and its own attack where its
  folder has `h2hattack…` clips (a fire gecko bites and breathes fire).
- The arm in use is drawn as a weapon without a model (its stance's draw cycle and aim);
  `npc_combat` fights with it as with any weapon: a gun shoots its projectile from the
  WEAP NNAM node, a melee weapon strikes with its damage and reach, its clips are the
  stance's attack clips (people's names first, else the stance's `<prefix>attack…` clips
  in its folder in turn: the buzzsaw's chop, stab and uppercut).
- **Switching by distance** [design: the original's rule is not documented; built on the
  wiki's Min Range "they don't need to be closer than this value" and Max Range "they
  don't want to be any further away than this value"]: melee within 192 units; a gun
  within its Min–Max Range; the arm in use kept within its band widened by 15 %; if no
  gun suits, the one that reaches furthest (beyond all) or the melee arm, else the gun
  with the smallest Min Range (nearer than all); among guns that suit, the most damage
  per second; at most one switch every 1.5 s, the new arm's equip clip played before it
  attacks. Mister Handy: saw within 192, flamer beyond. Mister Gutsy: flamer within 512,
  plasma gun beyond 600, and between the two the one in use.
- **A gun suits no target within its own blast** [design, #1621]: a gun whose projectile
  explodes counts its explosion radius as one more Min Range (a fresh choice wants the
  target 15 % beyond it, the arm in use keeps to the radius itself), unless the
  creature's combat style has Ignore Damaging Self (the exception `npc_combat`'s blast
  hold makes, §16). Before (observed 2026-11-05, testing #1585): a Hardened Sentry Bot
  (CrSentryBotMGHard: SentryBotMinigun2HR 500–2000, 9 × 15/s; SentryBotMissileLauncher2HL
  500–2000, 20 × 1.58/s, MissileExplosion radius 1000) nearer than both Min Ranges took
  the launcher (the tie on the smallest Min Range went to it), held fire within the
  blast and kept it, so it stopped attacking. Now it fires its minigun at any distance
  (its damage per second is higher) and takes up the launcher only beyond 1000 units
  with the minigun lost to a crippled limb.
- The first choice in a fight is made at once, and a creature with a melee arm does not
  step back from a target within `BACK_OFF_WITHIN` (256) [design]. Before (2026-10-25,
  full smoke on main 8ec4062d): Mister Handy, drawn with its flamer, stepped back from the
  player at its feet before its first switch was allowed, kept 256 away, never came
  within the saw's 192 and never used it.
- A body gun's shots and its line-of-fire check leave from its NNAM node by
  `npc_combat`'s embedded-node rule (`embedded_at`, from the ESM audit's gap 14), the one
  path for every embedded gun; `body_weapons.rs` puts no muzzle entity of its own there
  (it did, and that hid the embedded-node path and its log line).
- An arm whose limb condition (the WEAP Embedded Weapon Actor Value) is 0 is left out
  [hypothesis: "the condition value affected when the weapon is targeted", GECK wiki
  Weapons]: crippling Mister Handy's left arm leaves it its saw.
- Body weapons do not wear or break, do not reload (no magazine) and do not slow their
  creature as two-handed weapons in hand do [design].

**Not built**: VATS targeting of a body weapon; the original's own switching rule (a
capture of a Mister Gutsy at several distances would settle it); the protectron's and
other robots' clips and nodes were not looked at one by one (they take the same path:
whatever stances their folders have).

**Checks**: unit tests (`fnv_combat::body_weapon`, `fnv_actor::anims`,
`attack_anims::body_attack_names`); smoke `body-weapons-handy-gutsy`.

## 20. Grenades (2026-10-25, `engine/nvforever/src/npc_grenades.rs`)

No page says when the original's actors throw a grenade: the combat style has no grenade
field and FalloutNV.esm sets no such setting [observed: CSTY's fields; the GMSTs named
Grenade or Throw are VATS's, the AP cost, friction, restitution and the reverse-pickpocket
timer]. Combat is not a parity target (combat.md), so the rules are ours [design], to be
checked against the original (observer-checks.md item 20):

- **What**: a weapon of animation type "Grenade Throw (1 Hand)" whose projectile explodes
  (frag, plasma and pulse grenades, dynamite), from the actor's inventory, the lowest
  FormID of those it carries; one used up per throw (rounds of guns still come from
  nowhere, owner 2026-10-17: a grenade is the weapon itself).
- **Who** (2026-11-01, issue #1303): only an actor whose held weapon is the grenade. A
  gunman keeps the grenades it carries [observed: observer check 20, o12_grenades and
  o13_airport on the airport runway: Powder Gangers carrying 4-10 dynamite threw none in
  90 s at 426-1,595 units, nor at 76-667 in 24 s; the one that spawned holding its
  dynamite threw it once at about 1,225 units within a second, then drew a gun and threw
  no more]. In ours a held weapon always comes before a thrown one (actors.md §9.3) and
  re-arming passes grenades over. An actor with only drawable thrown weapons can hold
  a grenade (`fnv_actor::weapon::choose_weapon`); a gunman does not swap to one for a throw.
- **Frag grenades** (2026-11-01, observer check 20, o18_air and o24_air, god-mode
  player) [observed]: a trooper holding a frag grenade (6 carried, nothing else) threw
  from 800 units within a second of combat and again at about 1.8, 5.4, 7.2 and 9.2 s,
  then held nothing (14.9 s) and fought with a knife; from 1,200 units its throws landed
  at 0, 1.6, 4.0, 5.7 and 9.3 s. Two holding frags 300 units off threw none: they closed
  in and struck (2.25 Health a blow) and put the grenade away after 6 s. A 10mm trooper
  carrying 5 frags threw none in 27 s at 600 units. So a held grenade is thrown every
  1.6-3.6 s from the start, from 800 units at least (not from 300), against ours' first
  throw after 4-10 s, one every 12-25 s and nothing within 990 units [issue #1351].
- **When** (since 2026-11-06, #1351): holding it, not fleeing, ready on a target it sees;
  the first throw 0 to 1 s after it is first ready on that target, then one every 1.6 to
  3.6 s, rolled on the grenade rolls' own sequence [observed: the frag captures above].
  It was 4 to 10 s, then 12 to 25 s [design].
- **Where**: only at a target beyond 550 units [design: between the observed 300 (no
  throw) and 800 (thrown); the original's floor is not pinned. Until #1351 it was 1.1
  times the blast's radius, 990 for a frag] and within the throw's reach:
  the low arc's under the projectile's gravity, at most 1,500 units. Held when a comrade
  or a bystander would be caught, by the combat style's Ignore Damaging flags as an
  explosive shot is (§17.6).
- **How**: from the eyes on the low arc that lands on the target's feet (VATS mode's
  `lob_dir`), 2° of spread, the grenade's own attack clip asked for, the FireExplosive line
  tried on the attack timer. The live grenade is
  run from by those it threatens (§17.7).
- Log: `npc combat: X throws W at T (D units, blast R); N left`, `X keeps its grenade: K
  would be caught …`; summary `N grenades thrown, M kept for a comrade or a bystander`.
  Smoke `combat-npc-grenades`: Violet, given three frag grenades, fights with her gun and
  throws none.
- **Execution ownership** (#1498) [design]: holding a grenade uses a dedicated
  controller, with no firearm magazine, reload or queued shot. Before the deadline,
  while a throw is denied, and after depletion, it cannot fall through to gun firing.
  Switching from a firearm discards any pending firearm wind-up; a firearm retained
  across target changes still keeps its own cycle. The chosen grenade's item must be
  removed successfully before its shot, animation, success counter or bark is emitted.
  Lowest-FormID inventory selection remains the policy above: the selected grenade
  supplies the projectile, ballistic parameters, animation and consumed item, even
  when another grenade type is held. This fixes execution and inventory ownership;
  it does not retune timing toward the original or redefine mines and other throws.
- Not built: choosing to swap from a gun to a grenade for one throw, throwing from the
  clip's release key, grenades for creatures, mines.

## 21. NPCs use their stimpaks (owner request 2026-10-25)

The owner's request: an NPC that takes damage uses a stimpak it carries, never one that
would heal "only 1 hp if they have almost full health"; and the original's stimpak
animation, which holds the NPC still for five seconds, is played on the top half of the
body only, "so that they can still move and stimpak animation at the same time". Combat
is not a parity target; the rules are **[design]** unless tagged. Rules:
`fnv_game::npc_stimpaks`; animation: `engine/nvforever/src/npc_stimpaks.rs`.

- **When.** A living NPC (not a creature, not the player), not knocked out, on furniture
  or in a conversation, in Combat or within 15 s of its health last falling (an NPC
  hurt before anyone watched it, a quest's wounded, is left alone), with no stimpak in
  use and none used in the last 3 s (or the last one's effect duration: Hardcore's heal
  over 6 s). The engine starts one only while no scripted, package or dialogue idle plays
  on it, it is not staggered, not reloading, with no blow on its way.
- **Which, not wasteful.** Its carried ingestibles of equip type Stimpak (ETYP 11
  [documented: xEdit]; the Stimpak and Super Stimpak [observed: FalloutNV.esm]), each
  with the health using it would give it now (`Game::aid_preview`: its Medicine's
  potency, perks). One is used only if at most 25 % of its heal would go past full health;
  of those, the one that heals most. Below 35 % health it uses the smallest even with
  more waste.
- **The animation.** The children of the idle tree's `ActStimpakSource` (0x00023035), the
  first whose conditions pass: `3rdP2HHStimpak` (GetWeaponAnimType 7, `3rdP_2HH_Stimpak.kf`,
  3.83 s) else `3rdPSpecialIdleStimpak` (`3rdP_SpecialIdle_Stempak.kf`, 5.0 s), the
  syringe `AnimStempax` (`stempax.nif`, on `Bip01 L Hand`) with the latter [observed:
  FalloutNV.esm, the base meshes archive]. `ActStimpakSource`'s own condition
  (GetIsUsedItem Stimpak) is not asked, so the Super Stimpak has an idle too; the NPC's
  used item is the stimpak while it plays (GetIsUsedItem answers 1). The clip's tracks for
  `Bip01`, `Bip01 NonAccum`, the pelvis and the legs are left out and the rest raised to
  priority 57, above the armed run (36), the aim (35, 50) and a held shot (55/56); the
  idle is laid over the fight's pose. So the legs walk, run and strafe while the arms
  inject. The stempak clip itself drives the left arm at 30 [observed], which the run
  would beat.
- **The injection** at half the clip [hypothesis: the clips carry only `start` and `end`
  keys, observed]: one stimpak leaves the inventory and is used (`Game::ingest`, so the
  potency, the trace and the addiction rules are the player's), and the sound
  `NPCHumanUsingStimpak` (0x000296B9) plays at the NPC [hypothesis: its use, by its
  name]. Stopped before then (killed, knocked out, a scripted idle), nothing is used.
- **Fire held** for as long as the idle plays (`npc_combat`'s `ready`).

**Checks**: unit tests `fnv_game::npc_stimpaks` (the waste rule) and
`npc_stimpaks::the_upper_body_keeps_spine_arms_and_head_above_the_run`; smoke
`combat-npc-stimpak` (Chet at 63/65 uses none; at 23/65 one, healed 39 to 62).
Observed 2026-10-25 in a trial run: Violet with a service rifle at 110/170 injected
while backing off at 85 units/s, rifle in her right hand and the syringe in her left.

**Not built**: a syringe for the two-handed heavy idle (no ANIO names it); the clip's
visibility track of the syringe (it shows for the whole idle); NPCs not drawn do not
heal; companions are treated as any NPC (the wheel's Use Stimpak gives the player's).

## 22. NPCs re-arm: better weapons, their own dropped one, fists last (owner request 2026-10-25)

The owner's report: a Viper Gunslinger fought with her fists. The audit found the records
and both leveled-list resolutions sound (every Viper rolls a weapon from
`LL2WastelanderVipersWeaponNV`; half of its level-1 entries are a tire iron or a knife
[observed: FalloutNV.esm]); she had lost hers to a hit on the weapon (§17 of
combat-rules.md, a broken weapon falls) or a crippled arm, and nothing had an NPC take up
a weapon again. The owner's rules, 2026-10-25: every NPC goes for a weapon that is
better than its own by overall damage per second; one skilled with guns does not put a
melee weapon above its gun even when the melee weapon is better; a gun is usable only
with ammunition for it, which the NPC must find too, though using it spends none; the
NPC takes a copy of the ammunition and leaves the source as it was ("a weird engine
quirk but lets keep it in"); fists are the last resort; it plays a pick-up animation.
Combat is not a parity target; the rules are **[design]** unless tagged. Rules:
`fnv_game::rearm` (ranking), `fnv_ai::rearm` (looking and fetching); engine:
`npc_combat` holds fire while it fetches.

- **Who.** A living actor other than the player in a combat state (Alert, Lost, Combat),
  not fleeing, not on furniture, whose package does not continue during combat: an NPC,
  or a Super Mutant creature (CREA type 4, the creatures whose skeleton has the people's
  weapon stances, `outfits.rs`). Animals, insects, robots and ghouls do not.
- **The rank of a weapon** for an actor (`Game::weapon_rank`), compared in this order:
  1. its class is the actor's: ranged (Guns, Energy Weapons, Big Guns, Explosives) for an
     NPC whose best ranged skill is at least its best melee skill (Melee Weapons,
     Unarmed), else melee; a creature has no class of its own (every weapon matches);
  2. damage per second: per attack the damage the outfitting ranks by (base damage, the
     projectile's explosion for a weapon not thrown, `forms::weapon_choice_damage`) times
     its condition's damage share (`inventory::condition_mult`, combat-rules.md §17),
     times attacks per second: an automatic weapon's Fire Rate, else its Attack
     Shots/Sec, else 1 for a gun; for a melee weapon its Attack Mult (at least 0.25)
     over 1.2 s, the combat AI's strike pace (`npc_combat::STRIKE_SECONDS`).
  A weapon is not ranked at all (unusable) when it cannot be drawn, is Player Only or Not
  Used In Normal Combat, is broken (condition 0), is ruled out by the actor's combat
  style (Melee Only, Ranged Only), is a grenade, mine or thrown weapon (never a primary
  weapon, as in `fnv_game::style_weapon`; grenades are thrown by §20's rules), or is a gun
  whose accepted ammunition (WEAP `NAM0`, an AMMO or a form list of them [documented:
  xEdit]) the actor neither carries nor can find. The weapon it holds now keeps whatever
  rank it has.
- **What it takes up**, looking every second: with no usable weapon in hand (it broke,
  fell, was taken), first the best usable weapon it carries, equipped at once (the
  fight's empty-hand rule, `Game::arm_by_style`, takes up the same one by this rank). Its other
  carried weapons do not replace a usable one it holds: the outfitting's choice stands
  (in a first trial Cook-Cook swapped his Incinerator for his Flamer mid-fight, against
  what the original was observed to do, actors.md §9.3). Then, within
  1024 units (4096 with no usable weapon in hand), the best weapon it can reach that
  ranks above the one it holds, nearest on ties: a weapon lying on the ground (not owned
  by someone else, not taken by another fetcher) or carried by a dead actor. A gun it has
  no ammunition for counts only with a source of that ammunition within the same reach:
  ammunition on the ground, in a dead actor's inventory or in a container that is not
  locked. With no way there (navmesh) the item is given up for this fight.
- **The target's distance** (owner 2026-10-25: "the incinerator would be better at a
  distance and the flamer up close"). A weapon with a flame or an explosive projectile
  fires only within its reach (WEAP Max Range times the style's Ranged Weapon Max Range
  Mult unless Range - Fixed, never beyond its projectile's Range) and, explosive, only
  beyond its explosion's radius plus 64 units: the combat AI's own limits (§16,
  `npc_combat::gun_reach`); other weapons fire at every distance. A fighter whose weapon
  does not fire at its target's distance holds the best weapon of its own class it
  carries that does, 64 units inside that weapon's limits, at most every 3 s
  (`Game::take_up_for_distance`). So Cook-Cook keeps his Incinerator from 276 units
  (its fire burst's radius 212, plus 64) out to 1,500 (its Max Range), takes up his Flamer
  (128 damage a second against the Incinerator's 42; its flame reaches 640) for a target
  nearer than 276, and the Incinerator again for one beyond 640 [observed: FalloutNV.esm,
  `fnvtool combat weapon`]. The weapon's own projectile is read: an ammunition that
  brings its own would change the reach (none of these two does).
- **The fetch.** It runs there (the ammunition first when it is nearer), and holds its
  fire meanwhile (as a fleeing actor, `npc_combat`). At each item it stops and plays the
  pick-up idle, `LoosePickUpGroundObject` (`PickUpGroundObject_0.3.kf`), a child of
  `NPCPickUpItem` (0x00069B6E) [observed: FalloutNV.esm], or `SBGroundGrab`
  (`GroundGrab_0.83.kf`) for a super mutant, and stands for 0.6 s (the people's clip runs 0.57 s). A weapon is taken (the
  ground reference leaves the world; from a body it leaves the body's inventory, its
  condition kept). Ammunition is copied: one round of it is added to the fetcher's
  inventory and the source keeps its own. Then the weapon it went for is equipped. A
  fetch that has not arrived after 20 s, or whose item is gone, is dropped and the item
  given up.
- **Fists** remain what an NPC fights with when nothing is usable and nothing is within
  reach.
- **Not changed**: the weapon an actor starts with (the outfitting's rule by damage,
  observed in the original for Cook-Cook, actors.md §9.3; it gives way only for the
  target's distance, above) and what a held gun without
  ammunition does (it fires: NPCs use no ammunition, owner 2026-10-17).

**Checks**: unit tests `fnv_game::rearm` (damage per second, the rank's order, a weapon's
class); `crates/fnv-ai/tests/carry.rs` (an unarmed fighter fetches a gun and a copy of its
rounds; a gunman takes a weak gun over a strong knife, and the knife only with no rounds
anywhere; a gun with no rounds is left; the weapon held is kept over other carried ones
until lost); smoke `combat-npc-rearm`. Observed 2026-10-25: Violet, stripped of her items,
ran 1,638 units, copied a 5.56mm round from a placed `LootAmmoBox556mm` (nearer than the
box laid for her), picked up the service rifle and fired with it; in `battle-ncr-fiends`
seven fetches, none given up, and Cook-Cook kept his Incinerator.

## 23. Locked on a target in sight (owner request 2026-10-25)

The owner: NPCs "are still turning sideways when shooting at their target, they need to
keep locked on whilst shooting"; "if they have a target in sight they should be locked on
them"; Violet "still breaks focus to take cover"; the strafing clips (aimed and from the
hip) are there for moving sideways. The original's Fiends stayed within 1.5° of their
target through every phase of the attack and fired only once turned to it
(spec-notes/ai-observer.md §15).

- **The lock** (`npc_combat::Fighters::lock_on`): each frame, a person the AI runs, in combat,
  that sees its target and does not run from it (a flight away or by a door; a flight to cover keeps
  it) is locked on where that target is drawn this frame. `residents` draws it facing that
  place, turning at the combat turn rate (standing 225° a second with its target within a
  quarter turn, 275° beyond; walking 180°; #1352, below) and, once
  within 10° (`LOCK_HOLD`), following it exactly; once held it stays on that target even
  when the target sweeps more than 10° in one frame (2026-11-01, #1162 [design]: melee
  Fiends circling troopers 70–340 units away, and long frames, slipped out of the hold and
  turned back at the turn rate, up to 21 frames past 10° in smoke battle-ncr-fiends
  [observed: smoke logs of #1292]); a new target is still turned to at the turn rate. On its way to cover, a post or a flee
  spot it strafes or backs away with its target in front. Out of sight it faces the
  target's AI place as before (`Core::aims`).
- **The turn onto a target** (#1352, built 2026-11-06 from capture o25_turn,
  docs/spec-notes/actors.md §8.9 [observed]): a person going into Combat from Normal or
  Alert stands on its heading for 1.1 s (`fnv_ai::alert::COMBAT_REACTION_SECONDS`;
  `Awareness::combat_began`), neither turning, setting off nor locked on, then turns at
  225°/s while its target is within 90° of its heading and 275°/s beyond
  (`combat_turn_rate`), steadily with no ease-out, and walking at the walk's own turn
  rate (180°/s). Before, it turned at once at 240°/s standing and 360°/s walking. Back in
  Combat from Lost it does not stand again [hypothesis: not measured; nor is the delay
  when combat starts by detection rather than StartCombat]. `combat_turn_rate` picks the
  rate from the angle still left to turn; the original may set it when the turn begins,
  and the data cannot tell the two apart [hypothesis]. Creatures are unchanged
  (`fnv_ai::alert::creature`, `combat_began` is `None` for them): no stand, and their
  standing combat turn is still twice the standing rate with their own × 1.25
  (`COMBAT_TURN_CREATURE`) [hypothesis: a creature's reaction and turn are unmeasured;
  only a human trooper was, and o25's spawned coyotes were in combat at once]. The facing summary (`npc_combat`, smoke battle-ncr-fiends,
  combat-violet) gives a fighter its 0.6 s to turn (`REACTION_SECONDS`) from the end of
  that moment, not from first seeing its target, and does not count it locked meanwhile.
- **Why it slipped**: the drawn facing used the target's AI place, which for a charging
  Fiend was 110 units from where it was drawn; a trooper shooting at it faced 16° off.
  Flights to cover were not turned to the target at all.
- **Side-steps** (the owner's video of Violet, 2026-10-25: side-on to the player at each
  shot while strafing; PR #620 had fixed this for the player's body only): a rifle has no
  walk left of its own, so NPCs walked left with the unarmed `mtleft.kf`, whose `Bip01
  NonAccum` turns the whole body 76° from the rifle's run (`mtfastleft.kf` 90°; the right
  and back side-steps 14–16°) [observed: kf_pose_compare, 2026-10-25]. A standing weapon
  stance without its own left side-steps now takes the fists' `h2hleft.kf` /
  `h2hfastleft.kf` (`fnv_actor::locomotion::weapon_moves`); the player's body takes
  `h2hleft.kf` walking (animation.md §9.6).
- **Fire arc**: a gun fires only within 3° of straight ahead (`FIRE_FACING`, was 30°), or
  its weapon's Aim Arc when above 0. A scripted weapon use keeps 30° (`USE_FACING`).
- **Measured**: the `npc combat facing` line at exit (how far off its target a gunman faced
  at each shot, and a person the AI runs in each frame it was locked on); `battle-ncr-fiends`
  and `combat-violet` check it. Before: a 2-minute NCR–Fiend battle had 84 frames over 10°
  (up to 47°) and shots up to 29° off; after: none over 10° (largest 3.6°), shots within
  0.9°.
