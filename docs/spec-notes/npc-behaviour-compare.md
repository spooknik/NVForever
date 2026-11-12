# Spec notes: NPC behaviour, the original against our engine (2026-10-17)

What the AI observer (ai-observer.md §9–10) measured in the **original** game, the same
scenes in **our engine** (smoke-style launches with the engine's own log lines), and what
differs. Numbers only; no game text. Every original number is [observed 2026-10-17] from
private captures (`C:\fnv-private\capture`, never committed), analysed with
`tools/capture/observe_map.py`; engine numbers are [observed 2026-10-17] from release
builds of this branch (`nvforever --play`, logs private). Owner's two complaints: NPCs
speak combat lines and chatter far too often, and melee enemies come so close they clip
into their target. Both are confirmed; §2 and §3 give the fixes made on this branch.

Distances are 2D, centre to centre (GetPos: an actor's reference point at its feet), in
game units. Real seconds come from game time at TimeScale 30 (a game minute is two real
seconds).

## 1. Scenes

| Scene | Original (campaign, save) | Ours |
|---|---|---|
| Melee against a still player | `melee_spacing` (nvftest): near Goodsprings where Violet's dogs roam; the two dogs, then an NCR trooper (leveled list) given fists, a knife, a machete, a lead pipe, a sledgehammer or a power fist, then one creature of each of ten kinds, each spawned 700 units away and set on the player (god mode); 30 real s each, every game loop | `cmp-dogs`, `cmp-trooper-*`, `cmp-creature-*`: the same spot (`--pos=-45600,65000`), god mode, `combat:` / `moveto:` / `call:` test aids; 30 s each |
| NCR troopers against Fiends | `npc_battles` session `ncr_fiends` (nvftest), twice: the six NCR troopers of the smoke scenarios moved 600 units north of the player south of Cook-Cook's camp, 120 real s, every 2 game loops; the player in god mode, crouched, Sneak 100, under a Stealth Boy | `battle-ncr-fiends` (`cmp-battle`): the same troopers and offsets, the player unseen (`NVF_PLAY_TEST_UNSEEN`), 120 s |
| Other fights | `npc_battles`: NCR troopers against Legion melee raiders, coyotes against troopers, a three-party fight with giant mantises (spawned) | not run (no spawn aid for our engine's fights; §8) |
| The Goodsprings gunfight | `goodsprings_fight_fast` (owner's save nvfCombatTown), 88 real s, every game loop | no comparable scene (needs the quest state) |
| Conversations | `conversations` (nvftest): the player standing at the arrival marker in Goodsprings (10 game h), Freeside (8), the Strip (2, cut off), Westside (6), Camp McCarran (8), Primm (6), Novac (6), every 30 game loops | `cmp-goodsprings-day` (5 real min from 08:00), `cmp-saloon-evening` (4 real min from 18:00); then Westside and Novac for 3 game hours each from 10:00, from the original's spots and next to the NPCs, old and new rule (§5) |
| Casinos | `casinos` session `gomorrah` only (the owner stopped the other five, 2026-10-17): Gomorrah's main floor for 24 game hours from 12:00, every 60 game loops | none (§6) |

Arrival by `coc` from the early-game save leaves NPCs that a trigger enables disabled
(the owner saw the Strip's crowd missing); the Strip was queued again from the owner's save
`nvfstrip` (campaign `strip`), not yet run.

## 2. Combat lines

| | Original | Ours (before this branch) |
|---|---|---|
| NCR troopers against Fiends | 12 fighters; 7 and 6 combat lines in two runs of 120 s (the fight still going at the end) | 11 fighters; 47 combat lines (26 Hit, 15 Attack, 5 Death, 1 DeathResponse) and 7 Detection barks in a fight over in 32 s |
| Lines per fighter-minute | 0.63 over all NPC battles (13 lines, 1,242 fighter-seconds); 0.31 in the Goodsprings gunfight (4 lines from 18 fighters in 88 s) | about 8 (47 lines, about 350 fighter-seconds) |
| Gap between one fighter's lines | median 20 s (gunfight), 2–3 s for the rare pair in the battles | median 3.2 s, least 0.3 s |
| Fighters speaking at once | 0 for 88–96 % of fight time, 1 for the rest; 2 at once only in the Legion fight (8 % of it) | not measured (two or more often: 54 lines in 32 s) |
| Hits followed by a line | about 1 %: at most 2 Hit-like lines for 217 hits (Health falling by more than 2) | 26 Hit lines for 110 hits (24 %) |
| Attacks followed by a line | about 3 Attack-like lines for some 175 attacks begun (GetAnimAction 2 or 3) | 15 Attack lines (a 5–30 s timer per actor, no chance) |

The topic of a line cannot be read with base-game or xNVSE functions; "Hit-like" and
"Attack-like" are the map step's guess from what happened in the 1.5 s before it
[hypothesis, ai-observer.md §10].

**Fix (this branch, `fnv_game::combat_lines`)**: an NPC or creature says Hit only when the
damage is over `fCombatSpeakHitThreshold` (0.01) **and** a random number is below
`fCombatSpeakHitChance` (0.01) (before: or); and Attack, PowerAttack and FireExplosive only
when a random number is below `fCombatSpeakAttackChance` (0.01, in FalloutNV.esm, unread
before) on top of the 5–30 s timer. The player keeps the old rules (the original player
is silent; its lines are our design, and the `combat-lines` smoke scenario checks them).
Both are [hypothesis fitted to the counts above]: the Combat Tab page names the settings
without saying how they combine. Death, DeathResponse, Crippled and the health marks are
unchanged (each is said once per event, and they were 6 of our 47).

## 3. Melee spacing

Original, a swing being GetAnimAction entering 2 or 3:

| Attacker (target) | Swings | Distance when the swing starts: median (least) | Closest during the swing: median (least) |
|---|---|---|---|
| Violet's dogs, CrNVDogFiend (player) | 33 | 133 (95) | 119 (87) |
| Violetta, her own dog (player) | 9 | 122 (100) | 99 (98) |
| NCR trooper, knife (player) | 17 | 96 (89) | 92 (41) |
| NCR trooper, machete (player) | 13 | 95 (56) | 91 (45) |
| Coyotes (NCR troopers) | 20 | 109 (83) | 83 (81) |
| Giant mantises (troopers, raiders) | 57 | 163 (133) | 161 (113) |
| Giant rat (player) | 2 | 84 (79) | 81 (73) |
| Legion melee raiders, long weapons (troopers) | 4 | 222 (155) | 93 (45) |

Between swings the attackers mostly stood: the median back-off between two swings was
0–26 units. A quarter (dogs, coyotes) to three quarters (mantises) of swings followed the
previous one within 0.6 s (combos).

Not explained: against the still player in god mode, the trooper with a lead pipe, a
sledgehammer or a power fist, and most spawned creatures (dog, coyote, mole rat, mantis,
radscorpion, cazador, young deathclaw, ghoul), stopped 147–177 units away and never swung
in 30 s; with the player's Health raised to 100,000 instead of god mode, every attacker
kept 280–500 units off. The same creature kinds swung normally at NPCs (the mantises
above). [hypothesis: the AI weighs its target's health or invulnerability; follow-up §8].

Ours, before this branch (`npc melee spacing` log line, added here): blows began a median
14–36 units from the target in every scene (dogs 14, troopers 24–27, the battle's Fiends
36) and the closest a melee fighter came was 0–1: attackers stood inside their target.
Cause: with both bodies known, an attacker stopped at a body gap of 6 units and struck
within 24 (`MELEE_STOP_GAP`, `MELEE_HIT_GAP`), about 50 units centre to centre for two
people, whatever its weapon, and the navmesh walkers' push-apart (18 units each) allowed
less.

**Fix (this branch, `npc_combat::melee_spacing`)**: an attacker with bodies to measure
also keeps its reach: it stops at 0.9 × reach plus half the two bodies' radii and strikes
within reach plus half the radii, never nearer than the old gaps. Reach is
`fCombatDistance` 150 × the weapon's Reach × scale (knife and machete 0.5: 75), so two
people with a knife stop at 89.5 and strike within 97, against the original's 96 at the
swing and 92 closest [hypothesis: the rule is fitted to the knife and machete rows; the
creatures' rows are larger than their reach (a dog's 58) gives, follow-up §8]. Results
after the fix: §7.

**Creatures (`npc_combat::creature_spacing`, follow-up §8.2)**: the creatures' rows set
against their records (`fnvtool esm decode`, the template's Attack Reach RNAM and Base
Scale BNAM as our engine reads them) [observed]:

| Creature | RNAM × scale | Original: swing starts, median | RNAM × scale + 59 × scale |
|---|---|---|---|
| Violet's dogs (CrNVDogFiend, template CrDogVicious) | 48 × 1.2 = 58 | 133 | 128 |
| Violetta (scale 1.0 on her record, 1.2 on the template) | 48–58 | 122 | 107–128 |
| Coyote (VCrTier1CoyoteMed, CrCoyote) | 48 × 1.0 = 48 | 109 | 107 |
| Giant mantis (VCrTier2GiantMantisMed) | 100 × 1.0 = 100 | 163 | 159 |
| Giant rat (VCrTier1GiantRatMed, 2 swings) | 25 × 1.1 = 28 | 84 | 92 |

A creature's blows reach a near-constant 55–75 units beyond its RNAM, more for the larger
dog: (RNAM + 59) × scale fits the four kinds within 10 units [hypothesis: the 59 stands
for the creature's body ahead of its centre and the target's; the records cannot say,
the dogs' and coyotes' Object Bounds being 0, so a creature far longer or shorter than
these (a deathclaw, a radroach) is not covered by the rows]. Ours now keeps that distance
centre to centre: it stops at 0.9 of it and bites within it, never nearer than the two
bodies' gaps.

**Refit by skeleton (#1334, 2026-11-06).** The airport rounds of 2026-11-01 (campaigns
o14_reach and o12_reach, `nvfairport` runway, a god-mode player standing still, every
game loop, the centre-to-centre distance at each swing start, median; combat-rules.md
§10) [observed] put more creatures against the 59. Their leveled-list forms as
`fnvtool esm decode` reads them (RNAM and BNAM from the Stats template source, as the
engine resolves them) [observed]:

| Creature (form) | Skeleton folder | RNAM × scale | Original: starts | (RNAM + 59) × scale | Beyond RNAM: start ÷ scale − RNAM |
|---|---|---|---|---|---|
| Gecko (VCrTier2GeckoMed) | NVGecko | 35 × 0.8 | 81 | 75 | 66 |
| Young cazador (VCrTier3CazadorSmall) | NVCazadores | 100 × 0.5 | 80 | 80 | 60 |
| Young night stalker (VCrTier2NightStalkerSmall) | NightStalker | 48 × 0.75 | 102 | 80 | 88 |
| Radscorpion (VCrTier2RadscorpionMed) | Radscorpion | 64 × 0.6 | 97-129 (middle 113) | 74 | 124 |
| Feral ghoul (VCrTier1FeralGhoul1A) | Ghoul | 96 × 1.0 | 135-140 | 155 | 42 |
| Giant mantis (VCrTier2GiantMantisMed) | NVMantis | 100 × 1.0 | 101-163 (163 on 2026-10-17) | 159 | 59 |
| Bighorner bull (VCrTier3BigHornerLarge) | NVBigHorner | 50 × 1.7 | 232 | 185 | 86 |
| Coyote (VCrTier1CoyoteMed) | Dog | 48 × 1.0 | 84-117 (109 on 2026-10-17) | 107 | 61 |
| Violet's dogs (above) | Dog | 48 × 1.2 | 133 | 128 | 63 |
| Deathclaw (VCrTier4DeathClawMed, template CrDeathClaw) | DeathClaw | 192 × 1.0 | 88 (one start; its hit at 108) | 251 | - |
| Dog (VCrTier1DogMed) | Dog | 48 × 0.8 | 172 (one start, still closing; its hit at 69) | 86 | - |

Which exact forms the campaigns spawned is read from the names they logged; the forms
above are the leveled-list ones of those names [hypothesis]. The reach beyond RNAM is not
one number: 42 (the ghoul) to 124 (the radscorpion, whose tail strikes over its body).
Ours now takes it from the skeleton (`npc_combat::creature_body_reach`): Gecko 66,
cazadores 60, night stalker 88, radscorpion 124, ghoul 42, bighorner 86, dog 62; any
other skeleton keeps 59 (the mantis's own fit) [hypothesis: that this reach belongs to
the skeleton and grows with the creature's scale, as RNAM's does, so a young and an
adult of one skeleton share it]. Not refitted: the deathclaw (one start, far nearer than
its RNAM 192 gives: open, to measure again) and the giant rat (two starts, 84 against
92).

The people's rows (same rounds: knife 95, sledgehammer 120, super sledge 150, bumper
sword 175, lead pipe 180, about 45 + 107 × Reach) moved `fCombatDistance` to 107 and
`melee_spacing` to the bodies' whole radii (combat-rules.md §10). Ours mostly begins its
blows once stopped, so the stop (people and creatures) moved from 0.9 of the strike
distance to 6 units inside it
(`MELEE_STOP_SHORT`, the knife rows' start 96 against closest 92), its walk ending 0.9
of that in: with the old 0.9 stop Violet's dogs began at 115 (original 133), now 133-134
[observed: smoke combat-dogs, 2026-11-06].

## 4. Encounters

| | Original | Ours |
|---|---|---|
| NCR troopers against Fiends, how long | still fighting after 120 real s in both runs (7 of 12 dead in one) | over in 32 s (5 Fiends dead, 1 trooper) |
| Target changes | 38 among 12 fighters (3.2 each) | not logged per fighter |
| Joined before anyone targeted them | 10 of 12 (troopers and Fiends alike: they saw the fight or were alerted) | `combat group: … alarms …` lines (allies-alerted scenario) |
| Fled | none in the NPC battles; 4 in the Goodsprings gunfight (animals, as on 2026-10-08) | `0 fled` |
| The player targeted | 6 of the 38 target choices in one NCR–Fiend run were the player, despite sneak and a Stealth Boy | never (unseen by the aid) |

Taking cover is not readable in the original (no function reports it).

## 5. Conversations between NPCs

**Correction (2026-10-17, later)**: the first map counted 10 conversations in the towns and
15 in Gomorrah, but `observe_map` took any dialogue package or dialogue target as a partner,
and half of them named a stool, a cook stove, a guard marker or an XMarker (a sandboxing
NPC's dialogue target). With both sides required to be sampled actors (fixed on this
branch), the original has **5** in the towns (Westside 4, Novac 1) and **5** in Gomorrah.
All five in the towns were **960–3,050 units from the player** (the observer standing at
the arrival marker): the original has no rule that the player be near.

Ours was run from the same spots and hours as the original (smoke-style launches of
`nvforever --play --hour 10 --no-voice`, 370 real s each = 3 game hours; logs private):
"next to the player" is our rule as shipped (only with the player within 300 units of the
starter or its partner, the owner's playtest list 5 item 28, the player among Westside's
citizens or in the Novac diner); "anywhere" is a measurement-only build with that limit
lifted, which is what the original's numbers compare with.

| | Original | Ours, old rule (a timer per actor) | Ours, new rule (one timer for the area) |
|---|---|---|---|
| Westside, anywhere | 4 in 6.07 game h (**0.66 an hour**), 36.4 NPC-hours | 10 in 3 game h (**3.3 an hour**) | 2 in 3 game h (**0.67 an hour**), 32.5 NPC-hours |
| Novac, anywhere | 1 in 6.07 game h (0.16 an hour), 84.1 NPC-hours | 2 in 3 game h (0.67 an hour); 22 tries, 6 won, most starters had no line for their partner | 2 in 3 game h (0.67 an hour), 49.1 NPC-hours |
| Goodsprings, Freeside, Camp McCarran, Primm | 0 in 32.4 game h | not run | not run |
| Gomorrah | 5 in 14.1 game h (0.35 an hour), 296 NPC-hours | not run | not run |
| Per NPC per loaded game hour (each conversation counts for both) | 0.032 over the seven towns (Westside 0.22, Novac 0.024); Gomorrah 0.034 | Westside 0.62 | Westside 0.12, Novac 0.08 |
| Westside, next to the player | — | 0 in 3 game h (15 tries, 1 won, no partner line) | 0 in 3 game h (5 tries, 2 won; one walk given up, the partner greeting the player) |
| Novac diner, next to the player | — | 0 in 3 game h (22 tries, 5 won, no partner line) | 1 in 3 game h (7 tries, 2 won) |
| Who | Westside: the four citizens 0x001381A7, 0x001381A8, 0x001393C2, 0x001393C6 | the same four and 0x0015D441 with 0x0015D443 | the same; Novac: AdaStrausREF with two settlers |
| Starter's distance when it starts walking up | median 624 (164–937) | 150–907 | 149–533 |
| Distance when the talk starts | median 120 (82–624) | 150 after a walk, but 7 of 10 started where the starter stood (218–907): the walk ended at once (follow-up 9) | 150, one at 533; with follow-up 9 fixed 148–149 after a walk (2 of 2) |
| Length | median 2.0 game minutes (0.8–5), 1 line each | until a Goodbye or no more links | the same |
| Hours of day | 10:00–13:00 (the runs' hours) | 10:00–13:00 | 10:00–13:00 |

**The timer** [hypothesis, fitted to the counts above]: `fAISocialTimerForConversationsMin`
/ `…Max` (30 / 60) count **seconds of play**, and there is **one timer for the loaded
area**, not one per actor. One try every 45 s on average at 25 % is one conversation per
180 real s, 0.67 per game hour at TimeScale 30: Westside's 4 in 6.07 hours (0.66) match
it, and no town or Gomorrah went above it. A timer per actor (our old rule) gave 3.3 an
hour in Westside, five times the original, and would grow with the crowd; a timer in game
seconds would try every 1–2 real s, a hundred times the original. Game minutes (one
conversation per 6 real min, 0.33 an hour) are not ruled out by four conversations.
Where few pairs have lines for each other (Novac, the four towns with none) the original
stays below the timer's rate; ours shows the same in Novac's old-rule run (6 chances won,
2 conversations).

**Change (this branch, `fnv_ai::social`)**: one timer for the loaded area: when it is due
and some actor that may start a conversation has someone within the radius (one of the two
next to the player), the timer is drawn again (30–60 s) and the chance (25 %) is rolled;
on a win a starter is drawn among those actors, and if it has no line for anyone near, the
next drawn. An actor rests a time drawn the same way after a conversation; one that has
just said hello does not start one. The next-to-the-player rule stays (the owner's), so in
play the player hears at most one conversation per 1.5 game hours, and only where a pair is
next to them. The `social summary` line now gives the tries, the chances won and the
actor-seconds next to the player; smoke scenario `conversation-timer` checks one to three
tries in 90 s of Westside.

Other speech outside combat (hellos, idle comments) in the original: 11 lines in the same
46 game hours (Freeside 4, Westside 4, Novac 3).

The game settings the masters hold for this [observed: FalloutNV.esm, `fnvtool esm
decode`]: `fAISocialTimerForConversationsMin` 30, `…Max` 60,
`fAISocialchanceForConversation` 25, `fAISocialchanceForConversationInterior` 25,
`fAISocialRadiusToTriggerConversationInterior` 1000, `fIdleChatterCommentTimer` 15,
`fIdleChatterCommentTimerMax` 60, `fAIDefaultSpeechMult` 0.15. No exterior radius and no
greeting distance is in the masters (their defaults are in the encrypted executable).

## 6. Casinos (Gomorrah only)

The casino part rests on **Gomorrah alone**: the owner stopped the campaign after its first
session, and that session too was ended early to free the game for other sessions. It
covers Gomorrah's main floor from 12:00 to about 02:00 (14.1 game hours, 1,759 samples,
21 actors in the cell). The Tops, the Ultra-Luxe, the Atomic Wrangler, Vikki & Vance and
Gomorrah's lobby were not observed.

- **Conversations**: 5 between NPCs (first counted 15, ten of them an NPC with an XMarker
  or an idle marker, §5), 0.35 per game hour in the cell, 0.034 per NPC per game hour
  (each counts for both; 0.032 over the seven towns), at 120–190 units when they start
  (median 161) and 120–175 when the talk starts, 0.5–1 game minute long, one line each; at
  12:00–13:00 (3), 14:00 and 21:00.
- **Seats**: 37 sitting edges; one NPC sat in up to five different chairs over the day,
  each for 1–14 game hours; chairs of furniture marker types 12, 14, 15 and 20.
- **What the time went to**, by procedure: PATROL (most), SITTING, GUARD, then TRAVEL and
  GET_UP; DIALOGUE took a fraction of a percent.
- Other speech outside combat: 10 lines in 14 hours. No fight.

Ours: no casino scene was run (the comparison of the casino rules waits for a scene with
the same actors; §8).

## 6a. The interaction map

All sessions together (`observe_map.py` over every output; JSON private, from
`fnv.observe.map/1`): 166 actors; edges: 138 targets (22.4 actor-hours), 71 uses of
markers and furniture as package targets (133.1), 57 sits (121.0), 24 talks to (0.9),
5 follows (10.8). Seats by furniture marker type: 15 (37.4 actor-hours), 14 (29.0), 20
(28.2), 19 (13.0), 12 (11.7), 17 and 11 (under 1.2).

What each package did, by record type (67 of the 80 packages seen have a record; the rest
were created at run time):

| Record type | Packages | Actor-hours | Procedures |
|---|---|---|---|
| Sandbox | 20 | 239.8 | WANDER 39 %, SITTING 38 %, TRAVEL 12 %, USE_IDLE_MARKER 6 %, GET_UP 5 % |
| Patrol | 17 | 168.2 | PATROL 99 %, DONE 1 % |
| Travel | 18 | 64.4 | DONE 79 %, TRAVEL 21 % |
| Guard | 2 | 28.3 | GUARD 100 % |
| Follow | 3 | 16.9 | FOLLOW 100 % |
| Dialogue | 2 | 14.1 | DONE 100 % |
| Eat | 4 | 3.9 | EAT 51 %, TRAVEL 49 % |
| Created at run time | 13 | 12.8 | GET_UP and DIALOGUE / DIALOGUE_ACTIVATE |

Every one of the 67 reported the record's own runtime type code (GetCurrentAIPackage), and
none was in use more than 5 % of its time outside its record's schedule window.

## 7. After the fixes

The same scenes in our engine with this branch's two changes:

| | Before | After | Original |
|---|---|---|---|
| NCR–Fiend battle: combat lines | 47 (26 Hit, 15 Attack) in 32 s | 11 (2 Hit, 2 Attack, 4 Death, 3 DeathResponse) in 46 s | 6–7 in 120 s |
| Melee blows in that battle: start, median (range) | 36 (34–86), closest 33 | 84 (64–90), closest 64 | weapons 95–96, closest 91–92 |
| Violet's dogs on the player: start, median | 14, closest 0 | 54 (52–63), closest 52; with the creature rule 121 (104–128), closest 104 | 133 (95), closest 87 |
| Trooper with a knife or machete scenes: blows, median | 24–25, closest 0 | 55–56, closest 16–44 (most blows there are the dogs') | 95–96 |

Combat lines are now within the original's order of magnitude (about 1.3 per
fighter-minute against 0.3–0.6) and melee people stand where the original's do; creatures
closed in nearer than the original's until the creature rule of §3 (follow-up §8.2,
`combat-dogs` smoke, 2026-10-17). Detection barks were
unchanged (6 in the battle).

## 8. Mismatches and follow-ups

1. **Combat lines** (fixed here, §2). Check the fixed rate against the original's 0.3–0.6
   per fighter-minute in a long fight; Detection barks (7 in our battle) were not looked at.
2. **Melee spacing** (fixed here for weapons, §3; creatures: done, §3, the rule
   (RNAM + 59) × scale fitted to dogs, coyotes, mantises and a rat). Open: creatures far
   longer or shorter than these (deathclaws, radroaches, cazadors) were only seen holding
   off a god-mode player; check them against NPCs.
3. **Fight length**: our NCR–Fiend battle ends in 32 s, the original's runs past 120 s.
   Look at hit chance and damage per shot at range (the original's 217 hits over about
   240 s of two runs against our 110 in 32 s).
4. **Attackers that hold off a god-mode or very healthy player** (§3): measure with a
   normal player (the owner's save, no god mode) before copying it.
5. **Conversation rate** (done, §5): one timer for the loaded area; Westside's rate now
   matches. Gomorrah and the four towns without conversations were not run in ours.
6. **Strip crowd**: run the `strip` campaign from `nvfstrip`.
7. **Spawn aid for our engine's fights**: `combat:placed:<base>` is not accepted by
   `NVF_PLAY_TEST_SCRIPT`, so the creature scenes in our engine only had Violet's dogs.
8. **Smoke scenarios moved by the fixes** (marked `known_failing` with the reason):
   `combat-lines` (the dogs' first bites now land before the player's power attack, which
   falls inside the player's own Hit line; fixed 2026-10-17: an attack's line now cuts
   the attacker's own Hit line, combat-dialogue.md §2, and the mark is gone) and
   `combat-cover` (no cover in 2 of 3 runs, 2 covers in the third; with the creature
   rule of §3 the same, cover in 1 of 3 runs: the mark stays).
9. **A walk up to a partner that ends at once** (§5; fixed, branch
   `claude/chat-walk-up`): in Westside 7 of our 10 old-rule conversations started where
   the starter stood, 218–907 units from its partner (the original's median is 120, but
   one of its five started at 624). **Cause** [observed: unit test
   `a_sandboxer_walking_up_to_a_conversation_partner_walks_the_whole_way` fails without
   the fix]: the walk's "reached" test (`Ai::walk`) still looked at the starter's package
   goal, which the walk to the partner does not replace. These citizens sandbox, and a
   sandbox's goal is its area (`Goal::Near`, the package's radius): a starter already
   inside it "arrived" in the walk's first frame, and `Ai::chats_on` took the walker that
   was no longer Travelling as "as near as the navmesh lets it get". **Fix**: a walk to a
   partner ends only at its path's end (as a sandbox's own walks and searches do), and a
   way that ends still farther than `TALK_DISTANCE` (150) gives the conversation up
   instead of talking across the distance [hypothesis: the original's 624 shows it can
   talk from farther; ours keeps to 150 until that is understood]. Westside at 10:00 with
   conversations allowed anywhere (test aid `NVF_PLAY_TEST_CHATS_ANYWHERE=1`), 5 min
   [observed 2026-10-17]: 2 conversations, both after a walk, 535 to 149 units in 4.5 s
   and 283 to 148, none given up. Smoke scenario `conversation-walk-up` checks that every
   conversation starts within 150 units.

Gomorrah stopped early and the Strip run from `nvfstrip` was dropped when the owner needed
the game for other sessions (2026-10-17).

## 9. Fight length and melee hold-off, measured again (2026-10-19)

AI observer campaign `fights_round` (tools/capture/observe/fights_round.campaign.json,
plugin version 9: every game loop, the fight field set and xNVSE's OnHit / OnDeath /
OnStartCombat events; `tools/capture/fight_stats.py` counts shots, hits and damage), save
`nvftest` [observed].

**NCR troopers against Fiends** (`ncr_fiends`, 1 game hour, 121 real s):

- Only **four** troopers took part. `MoveTo` on 0x000FCEB1 and 0x000FD572 failed ("script
  call failed"): both are non-persistent Camp McCarran sentries (NCRTrooperSentryCM) whose
  cell was not loaded, which the original's scripts cannot reach. Our `battle-ncr-fiends`
  aid moved all six.
- Three Fiends died within 8 s; **Cook-Cook** (FiendCookCookRef 0x000F5704, 215 Health, a
  flamer) killed the four troopers one by one (29.7, 51.5, 52.3 and 99.1 s) and lived
  with 15 Health; a second run (`ncr_fiends_far_resetai`) ended the same way at 89 s.
- Troopers: 36–138 shots each, **0.28–0.49 hits per shot**, a median **2.0–5.5 damage per
  hit** (most 23–27), shots from a median 650–1,500 units; all fighters together 291
  shots, 187 hits on actors (0.64 a shot), a median 2.0 damage a hit.
- **Ours with the same four troopers** (smoke `battle-ncr-fiends-four`, the engine's new
  `npc hits summary` line): over in 38–47 s, Cook-Cook dead. Hits from actors, by target:

  | Target | Original: hits, median damage a hit | Ours: hits, median damage a hit |
  |---|---|---|
  | Cook-Cook | 60, 2.0 (200 in all, of 215 Health) | 46, 6.2 (290 in all, of 290 Health) |
  | Fiends | 7–17 each, 4.2–11.1 | 3–10 each, 8.3–8.8 |
  | NCR troopers | 5–13 each, 0.1–1.9 (most died of burning) | 2–12 each, 1.7–30.4 (three died of 2–3 hits) |

  The hit rates are alike; **the damage a hit is two to fifteen times the original's** in
  ours, most of all a Fiend's gun on the troopers, and the actors' maximum Health differs
  too (Cook-Cook 290 against 215). Settled below ("Why the damage differed").
- The player moved 3,500 units off the fight at 52 s (the second run): the NPCs went on
  fighting with **IsInCombat 1** while 2,500–5,500 units from the player, until the last
  trooper died; then the rest left combat.

**Why the damage differed** (campaign `fight_stats_round`,
tools/capture/observe/fight_stats_round.campaign.json: the same fight every 10 game loops
with each fighter's GetLevel, GetBaseAV Health, Endurance, Agility, Guns, DamageThreshold
and DamageResist and GetEquippedCurrentHealth of its weapon, body armour and headgear;
output `obs_fight_stats_round_01_ncr_fiends_stats.1792409539.jsonl`, private) [observed
unless tagged]:

- The fighters' levels and skills are the record's in both games: troopers level 5,
  Guns 46; Fiend3 level 9, Guns 63; Cook-Cook level 14, Guns 58. The player's level (1)
  plays no part. The skill bonus is as documented.
- **NPC weapons are worn in the original, ours were new.** The troopers' Service Rifles
  had 120–200 of 400 health (30–50 %), the Fiend's Hunting Rifle 135 of 300, Cook-Cook's
  Incinerator 110 of 200. They come from the masters: the troopers' inventory names
  CondNVServiceRiflelLoot, a leveled list whose entries carry a COED item condition of
  0.30 to 0.60 (CondNVTrooperArmor 0.35 and 0.50, CondKnifeLoot 0.45 to 0.75). Our
  leveled-list reader dropped the COED that follows an LVLO, so every rolled item was
  whole. Fixed (fnv-sim `forms::leveled`).
- **Condition's damage penalty.** Raw trooper hits (the damage on Cook-Cook, DT 13,
  divided by the 20 % floor; on Fiends, DT 5, plus 5) were 10.94, 10.50 and 9.18 with
  rifles at 50, 45 and 30 %: 0.832, 0.799 and 0.698 of 18 × the skill bonus 0.73. That is
  `1 − (0.75 − condition) × 0.67` to three places [documented: GECK wiki
  SetConditionDamagePenalty, the hard-coded rule the JIP function overrides; full damage
  from 75 %]. Ours was a [design] line from 50 % at zero. Armour has the matching rule
  below 50 % [documented: SetArmorConditionPenalty]. Both now in fnv-combat.
- **NPC armour does not wear.** Cook-Cook's armour stayed at health 85 and his DT at 13
  through 60 hits; the troopers' and Fiends' armour likewise. Ours took 0.35 of each hit
  from it: his DT fell from 12 to 7 in the fight and the troopers' rifles did 6.5 instead
  of 2.6. Now only the player's armour wears. NPC weapons do wear from firing in the
  original (the Incinerator lost 2.19 a shot); ours do not yet (follow-up).
- **The 20 % floor holds**: Cook-Cook's hits were exactly 0.2 × the raw hit (2.1875 of
  10.9375); a critical added the weapon's full Crit Damage (18) before DT.
- **NPC Health**: the original's GetBaseAV Health is Base Health + (Endurance − 1) × 5,
  plus (level − 1) × 5 for an NPC with "Auto-calc stats"; a hand-set NPC's level adds
  nothing. Fifteen NPCs at full Health fit it (troopers 50, Fiends 65–105, Cook-Cook 215,
  Sunny 90, Doc Mitchell 65, Ringo 120, Trudy 75, Violet 170, …). Ours added Endurance ×
  5 + level × 5 for every NPC: 10 more for most, 75 more for Cook-Cook. Fixed (fnv-game
  `max_health`, combat-rules.md §9).

**After the fix** (smoke `battle-ncr-fiends-four`, 2026-10-19, one run):

| Target | Original: hits, median damage a hit | Ours before | Ours after |
|---|---|---|---|
| Cook-Cook | 60, 2.0 (200 of 215) | 46, 6.2 (290 of 290, dead) | 48, 1.9 (152 of 215, lives) |
| Fiends | 7–17 each, 4.2–11.1 | 3–10 each, 8.3–8.8 | 2–13 each, 5.1–5.2 (one 60.7: two hits from a Fiend's own Hunting Rifle) |
| NCR troopers | 5–13 each, 0.1–1.9 | 2–12 each, 1.7–30.4 | 9–23 each, 1.7–2.3 (one 8.2: the Hunting Rifle) |

Cook-Cook now wins, as in the original: the troopers died 34, 48, 50 and 57 s after the
first fighter engaged (the original's at 29.7, 51.5, 52.3 and 99.1 s), three of them by
his flamer. What is left: the last trooper dies some 40 s sooner than the original's, and
our Fiends live longer (17–52 s against the original's three dead within 8 s); one run
each, so the spread is not known.

The rule for damage between NPCs, and an offline check against the records'
values, is in combat-rules.md §18 (2026-10-19). A trooper's Service Rifle does 1.84 to
2.19 on Cook-Cook, the original's figures. The "under 2" on troopers is mostly burn
damage, so it is no measure of a Fiend's rifle. The run above still credited
Cook-Cook's Incinerator bursts to the player, so its trooper figures lack them; fixed
the same day (combat-rules.md §11).

**Melee against the player, with and without god mode** (inside GSGasStation; a MoveTo to
the outdoor spot failed, so both are in the same room; 30 s each; "no god mode" is Damage
Threshold 200 and Health restored every 2 s):

| Attacker | God mode | Distance to the player, median (least) | Swings | Hits |
|---|---|---|---|---|
| NCR trooper, knife | no | 97 (56) | 24 | 19 |
| NCR trooper, knife | yes | 95 (41) | 18 | 14 |
| NCR trooper, sledgehammer | no | 119 (77) | 22 | 18 |
| NCR trooper, sledgehammer | yes | 126 (98) | 19 | 16 |
| Dog (VCrTier1DogMed) | no | 71 (65) | 16 | 16 |
| Dog | yes | 70 (68) | 19 | 19 |

God mode changes nothing here: every attacker closed in and swung. The outdoor hold-off of
§3 (147–177 units, no swing) is therefore not god mode's doing [hypothesis left: something
of the outdoor spot near Violet's dogs; not settled]. NVForever's
attackers close in, as these do: no change.

### 9.1 Why the last trooper died sooner (2026-10-20, queue item 9)

The question: in ours the troopers of `battle-ncr-fiends-four` died 34–57 s after the
first fighter engaged, in the original 29.7–99.1 s. Read from the original's two
`npc_battles` runs of 2026-10-17 (plugin version 7, the same four troopers, each fighter's
combat target, weapon and Health every second frame; private outputs
`obs_npc_battles_01_ncr_fiends.1792229805.jsonl` and `.1792234477.jsonl`) against our
smoke logs [observed unless tagged]:

- **Cook-Cook's weapon.** The original's Cook-Cook held his Incinerator (WEAP
  0x000906DA) for the whole fight in all three runs (the two above and §9's), and stayed
  1,100–2,000 units from the troopers, lobbing globs. Ours drew his Flamer (0x0000432D):
  he carries both (`1ECookCook` CNTO: Incinerator at 55 %, Flamer at 80 %), and our
  weapon choice took the higher base damage (Flamer 16, Incinerator 1). With the Flamer's
  reach of 640 he ran at the troopers and burned them close up; the troopers, with him
  charging, shot at him rather than at the Fiends.
- **The troopers' targets.** In the original all four troopers shot the same Fiend
  first (LvlFiend2GunVarNV 0x000F9CE6 from 2.3–3.7 s, dead at 14.2 s), then the next
  (0x000F9CE8, from 14.9–16.4 s, dead at 19.7 s), and Cook-Cook last. Ours (before)
  opened on Cook-Cook, and his charge kept him the nearest.
- **Fixed: the weapon choice counts a gun's explosion** (actors.md §9.3; a hypothesis
  fitted to Cook-Cook). Smoke `battle-ncr-fiends-four` after it (one run): Cook-Cook
  fights with the Incinerator from about 1,500 units (his reach), the troopers turn on
  0x000F9CE6 together within 3 s as in the original, and it dies 16 s after the first
  fighter engaged (the original's 12 s); 0x000F9CE8 and 0x000F9CE9 at 24 s.
- **Still sooner: the troopers died 16, 21, 27 and 31 s after the first fighter
  engaged** (the original's 29.7–132.5 s over three runs). Two died of the Incinerator's
  burn, one of a Fiend's Hunting Rifle (a 34-point hit; the original's 0x000F9CE7 rolled a
  Lever-Action Shotgun in one run, so that weapon is the leveled list's chance), one of a
  melee Fiend.
- **The burn drains faster in ours.** The glob's explosion (NVIncineratorFireBurst)
  applies `NVEnchIncineratorEffect`: FlamerFireDamage, Health, magnitude 4, duration 8
  [observed: FalloutNV.esm ENCH 0x001519EF]. Ours drains 4 Health a second (hit-effects.md
  §2). In the original the troopers' Health fell by exactly 1 every 0.5 s (2 a second:
  0x000F36F6 from 33.0 to 63.5 s, on past its death to −22; 0x0015CE08 for 16.4 s and
  again 16.1 s; 0x0015CE0A likewise) in one run, and by 1 every 1.0 s (1 a second;
  0x0015CE08 for 19.5 s, 0x000F36F6) or every 0.5 s (0x000F36F5) in the other. None of
  the troopers has Fire Resistance (`fnvtool actor values`: 0), and FalloutNV.esm has no
  game setting that names such a scale. So the original burns an NPC at a quarter to half
  the effect's magnitude a second, for longer than its 8 s, and why is not known
  [hypothesis to test: the magnitude is scaled by the explosion's falloff at the target
  when it is applied, which would give different rates from one glob to the next; or
  by something of the caster]. Not built until a capture settles it: one actor burned
  by one glob at a known distance, its Health every frame (a campaign for the observer).
- Also seen: a death by an effect's drain is not written as `battle: … killed by …`, and
  the battle summary counts such a dead trooper as standing (two in the run above).

## 10. The Strip's crowd against ours (2026-10-19)

The original [observed]: campaign `strip` (tools/capture/observe/strip.campaign.json),
session `strip_evening` from `nvfstrip`, 6 game hours from 20:14, 1,238 samples every 30
game loops, with xNVSE's events (plugin version 9): 21 actors (the crowd of §1's list and
the securitrons), 5,682 events (2,504 OnPackageStart, 2,496 OnPackageDone, 236
OnPackageChange, 348 OnTriggerEnter, 53 OnActivate). Timeline private
(`C:\fnv-private\capture\observe\strip.timeline.json`).

Ours: `fnvtool ai run --samples` on the same capture: each actor starts where the capture
first saw it, at its hour, and is compared at every sample (smoke `ai-observed-strip`).

**First run: nothing matched**, for two reasons in the comparison, not in the AI:

1. Every crowd package (the travellers' and gamblers' ports, the MPs' patrol and check,
   the securitron messenger's dialogue) has the condition GetQuestVariable
   VStreetFluffNPC 52 == 1, which the game's own scripts set on the Strip; the headless run
   had no quest state. `ai run` now takes `--quest-var QUEST:INDEX=VALUE`.
2. Mr. Holdout and the dancers are placed initially disabled (a trigger enables them in
   the game); the run left them disabled. It now enables an actor the capture saw enabled
   (GetDisabled 0).

**With both** (1,219 samples an actor): the package was the same for 97–100 % of the
samples for Mr. Holdout (VMrHoldoutHangOutSell), the three dancers, the gamblers, the
travellers, MP A and the securitrons; MP B 83 %.

What still differs, and why [observed; the causes from the records]:

- **The NCR troopers A–D and the MPs' escorts**: the original ran the SS2 chain
  (VStripSS2Start1Trooper*Move, VStripSS2TravelEnd*, VStripSS2WallDrunk3,
  VStripSS2StartMP*Escort), which is in none of their package lists: a scene script gives
  it. Ours ran their own lists (VStreetSandbox38*, VStripFluff1HookerCheer*). The headless
  run runs no scripts; not an AI difference.
- **The travellers and gamblers walk back and forth in the original**: OnPackageDone and
  OnPackageStart of their port packages came in pairs 550–590 times per actor in 6 hours
  (bursts about a game second apart), and the timeline has 90–107 segments each. Ours made
  one trip each and stayed. Their packages (VStripFluff1PortRNDToGomPackage, …ToTop…) have
  an **end result script** [observed: the records] that, by the packages' names ("Port …
  RND"), sends the actor on to another port [hypothesis: the script's text was not read];
  the headless run does not run result scripts. Their positions therefore differ (median 630–1,040 units in the
  same cell). To compare the walking itself, the next step is the same span in
  `nvforever --play` with an engine-side sampler, which runs the scripts.
- MP B's 17 % and MP A's 3 %: the SS2 escort, above.

Nothing in the AI itself is shown wrong by this capture; the comparison tool was fixed.

### 10.1 The same evening in the engine, with the scripts (2026-10-20, queue item 13)

The original's first 2.2 game hours of `strip_evening` [observed, private timeline]
against `nvforever --play` (smoke `strip-crowd-evening`, release build of branch
claude/q13-strip-crowd, 4 runs; logs private): the package changes of the 17 actors of
§10 by the game clock, and the travellers' fade moves.

**Setting the scene.** The save `nvfstrip` holds the player just inside the Strip's north
gate (847, 8561, 1019), 20:08, with the crowd running. Our first run put the player there
with `--pos`: every crowd actor read "no package", as in §10's first headless run. The
cause is in the records, not the AI [observed: the decompiled scripts]: the gate door's
script (VFSVegasDoorSCRIPT on FreesideGateToStripREF, 0x00116425) is what sets
VStreetFluffNPC's `iplayerinarea` and `npcnormal` to 1 (the variable the packages test,
§10), names the street securitrons, and the first time through starts VStripSS2 (the
troopers' scene, `bstartsstation`). The scenario therefore starts in FreesideWorld at the
gate, gives the player the Vegas Gate Key (0x00119875) and activates the door, then puts
the player on the capture's spot 30 s later (Victor's messenger, VMQTopsSecMessengerREF,
is placed disabled and enabled by VMQTopsSecuritonMSGTrgB when the player walks into it,
0x0013B6D8 / 0x0016B48A beside the arrival point) and leaves the messenger's conversation.

**What matched** (from the second run on):

| Actor(s) | The original (first 2 h) | Ours |
|---|---|---|
| Mr. Holdout, the dancers, gambler C, the gate securitron | one package each (§10) | the same, all run |
| Travellers A/B, gamblers A/B | their port package all along; 6 fade moves each in 2 h (the end script: ResetAI, then MoveToMarkerWithFade to one of two markers by GetRandomPercent) | the same package; 7 fade moves each in 2.1 h |
| Trooper D | WallDrunk2 → Start1TrooperDMove 20.24 → Sandbox38B 20.61 → TravelEndD 20.81 | WallDrunk2 → Move 20.23 → Sandbox38B 20.60 → TravelEndD 20.82 (after the fix below) |
| Trooper C | Start1TrooperCMove → (HookerCheerC 0.02 h) → WallDrunk3 20.31 → TravelEndD 21.03 | Move → WallDrunk3 20.30 → TravelEndD 21.03 (after the fix) |
| Troopers A, B | Move until 20.29, then TravelEndA 20.44 / TravelEndB 20.33 | Move until 20.33, then TravelEnd at 20.33 |
| MP B | SS2StartMPBEscort until 20.91, then VStripMPABPatrol38 | the escort until 20.73, then the patrol |
| The messenger | VMQTopsSecuritronMsgDialog38 from the start (talking at the first sample) | enabled at 20.15 by the trigger, Dialog38 from 20.16, talks to the player |

**Fixed: a timed script package stays after its goal.** VStripSS2WallDrunk2 and
VStripSS2WallDrunk3 are Travel packages, Must Complete, with a 3-hour schedule duration
and no starting hour, given by AddScriptPackage (the quest script and the trooper
trigger scripts). In the original they stayed current after the trooper reached the wall:
WallDrunk3 0.72 h, until the next script package (TravelEndD) replaced it [observed]. Ours
let a script package go at the first choice after its goal (the od_06 rule, packages.md
§6), so trooper C went back to HookerCheerC at 20.48 for half an hour, and trooper D to
his sandbox 0.05 h early. Now a script package with a schedule duration and no starting
hour is timed as a list package is (`Game::start_schedule_duration`, from the moment it is
given) and is let go only once its goal is met and the duration has run out
(`Game::select_package`) [hypothesis: that the duration is what holds it; Ringo's
DefaultStayAtEditorLocation and RingoTravelPackage of od_06, let go after their goal,
have no duration, so the earlier observation still fits].

**Still differs, and why (not fixed):**

- **MP A's escort ends early in the original.** SS2StartMPAEscort (a five-point patrol,
  not looping, no duration) gave way to VStripFluff1MPCheckA at 20.29, while MP A was
  walking from its second point to the third, in the same sample in which troopers A, B
  and C left their Move patrols. Ours walks all five points (with the markers' waits)
  and ends at 20.72–20.80. Nothing in the scripts removes it (no RemoveScriptPackage,
  EvaluatePackage or ResetAI names MP A) [observed: every script that names the
  reference]; what ends four script packages in one sample is not known [hypothesis:
  something the scene does at that moment, e.g. a Say of the quest script; not
  settled].
- **Troopers A and B** spent 0.04 h (B) and 0.15 h (A) on a list package between their
  Move and their TravelEnd in the original; ours go straight from one to the other. Their
  TravelEnd packages come from the trigger activators VStripSS2Trooper*EndPack when the
  trooper walks into one; the 0.15 h is the walk to it, which the cut above starts
  earlier in the original [hypothesis].
- **MP B's escort** ends 0.18 h sooner in ours: the six-point patrol's walking and waits
  (our patrol waits come from the markers), not compared point by point.
- Positions were compared only by cell and by the fade moves; a per-sample position
  comparison needs an engine-side sampler, which this run did not need for packages.

## 11. IsInCombat while on alert: what the recordings already hold (2026-10-19)

Owner item "IsInCombat while Alert" (owner-review.md "NPCs on alert", item 4; docs/outstanding.md
AI list). Before any new run, every observer output on the desktop with the full field set
(`C:\fnv-private\capture\out\obs_*.jsonl`, 2026-10-08 to 2026-10-19: Goodsprings, the gunfight,
conversations, package kinds, cazador speed, the owner-decision sessions, gecko food, the
Strip) was read for IsInCombat beside the package type, procedure, combat target,
GetDetected player, IsWeaponOut and GetIsAlerted. The fights_round outputs are not on this
machine. All [observed] unless tagged.

- **No search state was ever recorded.** In about 480,000 living-actor samples, procedure
  SEARCH (36), PURSUE (15) and ALARM (14) never appear, nor package type
  SEARCH_FOR_ATTACKER (29) or ALARM (8). GetIsAlerted read 0 in every sample (as
  owner-decision-captures.md §3 found for the 2026-10-08 files).
- **IsInCombat goes with the combat package.** IsInCombat 1 was read only with package type
  COMBAT (5; 14,312 samples) or DIALOGUE (6; 175 samples, combat barks); package type
  COMBAT never came with IsInCombat 0. Onlookers in procedure OBSERVE_COMBAT read 0 (hv_knights,
  geckos: 606 samples). Fleeing on a flee package reads 0; FLEE as a combat procedure reads 1.
- **Unseen, still in combat.** Actors in combat with the player kept IsInCombat 1, the
  player as combat target and procedure COMBAT while GetDetected player read 0: 73 such
  stretches, most under 10 real s, the longest 293 s (a gecko of `geckos_package_chain`, to
  the session's end) [real seconds from game time at TimeScale 30]. 53 ended with the player
  seen again, 3 with the session; 17 with the actor leaving combat (IsInCombat 0, target
  cleared, back to PATROL or WANDER), 8 of them while still not detecting the player, after
  1.1 to 11.9 real s.
- **Entering and leaving.** An actor goes from its package's procedure (WANDER, GUARD, …)
  to COMBAT in one sample, with IsInCombat 1 and no combat target for 0.1–0.3 real s (up
  to 1.2 s for spawned gunmen) before the target is set. At the end of a fight the
  survivors read IsInCombat 1 with no target for 0.8–2.1 real s, then 0 (the Goodsprings
  gunfight's one game minute, ai-observer.md §7).
- **The `alert_noise` output** (`obs_owner_decisions_01_alert_noise.1792326458.jsonl`) has
  Cook-Cook and the camp Fiends alive (the file does not say which save it came from).
  Cook-Cook and two Fiends were already in combat with the hidden player at the first
  sample, before the shots; the other two joined within 0.2 real s each, straight into
  COMBAT with the player as target. No Fiend showed weapon out without combat. So it does
  not show an alert-before-combat state either.

**Reading** [hypothesis]: in the original, what scripts see as IsInCombat is the combat
package: 1 from the moment an actor takes it (before it has a target) until it drops it,
including the stretches where it has lost sight of its target, and 0 for onlookers. Our
alert.md has 1 on Alert, Combat and Lost; the recordings agree for Combat and for an unseen
target (our Lost), and have no case of an alert without combat, so the Alert row stays a
[documented] reading of the wiki, unconfirmed. The original went from unaware to combat in
one sample in every recording; a capture that should show an Alert has to give a hostile a
sound with no sight line at all (the shots of `alert_noise` came after the Fiends had seen
the player). No new run is planned for it in this round.

**Built since** (#2121, 2026-11-11; docs/design/alert.md §6): what scripts read of a fight
follows these samples: GetIsAlerted is SetAlert's alone (0 in a fight); an actor in Combat
or Lost keeps the target it fought as GetCombatTarget while it is out of sight; Lost reads
procedure COMBAT 13 and package type COMBAT 5, no longer SEARCH 36 / SEARCH_FOR_ATTACKER 29.
Alert alone (never fought) still reads SEARCH, uncaptured. How long the target is kept is
our own rule (until it dies or is forgotten, StopCombat or Normal) [design]; the original's
is the open question of #2010 and observer check 6.

## 12. The next round of observer campaigns (2026-10-19; run the same day, §13)

Written on the desktop; run on the other machine (owner's request), one at a time with both
game locks, each once. Generated jobs were checked against the masters (`observe.py jobs
--data`): every reference and form resolves.

| Campaign (tools/capture/observe/) | Save | Question | How to run |
|---|---|---|---|
| `fight_spacing` | nvftest | §8.3: fight length and damage per hit by start distance (500 / 1,000 / 2,000 units), guns only | each session alone (`--only spacing_500` …) |
| `melee_holdoff` | nvfstrip | §8.4: hold-off against a player without god mode, tgm sessions as the control at the same spot | one run |
| `creature_spacing` | nvftest | §8.2: swing distance of deathclaws, young deathclaws, radroaches, cazadors against NPCs | each session alone |
| `package_durations` | nvftest | owner-review "Package durations": a Novac day, switches against start + duration and the goal | one run (~50 real min) |
| `followers` | nvftest | Boone: through doors (both ways), follow distance, sneaking, joining combat | one run |

Findings go here (§8.2–8.4), into docs/spec-notes/packages.md (durations) and a followers note.

## 13. The §12 campaigns, run (2026-10-19) [observed]

**The run.** Run on the owner's desktop by the session that ran the second owner-decision
round, with both game locks held and one game launch per campaign (one per session where
§12 says so). Plugin version 9; `observe.py run … --data <Data>`. The jobs that name forms
by Editor ID need `--data`; without it they stop at once.
- **Outputs.** `C:\fnv-private\capture\out\obs_{fight_spacing,creature_spacing,melee_holdoff,followers,package_durations}_*`.
- **Timelines and run reports.** `C:\fnv-private\capture\round3`.
- **Per-fighter tables.** `round3\*.stats.json` (`fight_stats.py --json`).

### `fight_spacing` (§8.3): four troopers against four Fiends, guns only

| Start distance | Fight (s) | Shots | Hits | Hits per shot | Damage per hit (median, p90, max) | Shot range (median, max) | Deaths |
|---|---|---|---|---|---|---|---|
| 500 | 0.2–20.3 | 161 | 67 | 0.42 | 4.6, 11.0, 22.6 | 715, 1,690 | the four Fiends (4.5–14.0 s) |
| 1,000 | 0.3–39.5 | 154 | 72 | 0.47 | 5.2, 11.9, 24.6 | 492, 1,264 | four Fiends (5.0–36.4 s), two troopers (7.5, 8.5 s) |
| 2,000 | 0.3–28.1 | 233 | 56 | 0.24 | 5.5, 15.6, 47.0 | 1,282, 2,044 | the four Fiends (5.4–23.8 s) |

- **The troopers' damage.** Their hits did a median of 4.2 to 10.1 per hit (by trooper),
  and the troopers fired 10 to 59 shots each.
- **The Fiends' shots.** The Fiends fired 0 to 24 times each before dying. Their hits on
  troopers did a median of 1.6 to 21.2.
- **The verdict.** Without Cook-Cook a trooper squad kills the camp's Fiends in 20 to 40 s
  from any of the three start distances. The hit rate falls to about half at 2,000 units.
  Damage per hit stays at single figures, as in §9 [observed].
- **What it means for §8.3.** The original's long fights of §9 (89–99 s) are Cook-Cook's
  doing (215 Health, a flamer), not the distance.

### `creature_spacing` (§8.2): creatures set on four NCR troopers

The distance below is centre to centre at each OnHit by the creature on a trooper, taken
from the sample at or before the event (`hitdist` in this session's scratch, not
committed).

| Session | Creature hits seen | Distance at the hit (min / median / max) | Outcome |
|---|---|---|---|
| deathclaw (0x0014F42C) | 4 | 136 / 215 / 270 | killed all four troopers in 15 s, survived |
| young deathclaw (0x0014F42D) | 2 | 94 / 105 / 116 | killed one trooper, died at 15.4 s |
| radroaches | 0 | none | never closed in; three trooper shots at about 220 units, no hit |
| cazadors (0x0014F3FE) | 2 | 86 / 93 / 100 | four deaths in 13 s |

- **Few samples.** xNVSE drops repeated OnHit pairs within a frame (ai-observer.md §12),
  and many blows landed between the 1-loop samples' events, so these are few.
- **The adult deathclaw.** It strikes from about 215 units centre to centre, about twice
  a young one's reach.
- **Our rule.** `(RNAM + 59) × scale` (§3) was not recomputed here for these bases
  [hypothesis left for the next comparison].

### `melee_holdoff` (§8.4), from `nvfstrip`

- **NPC sessions: not settled.** In all four, the NPC attackers never engaged
  (sledgehammer, lead pipe, power fist, knife):
  - the spawned trooper stood still for the whole session (in_combat 0, Health 50);
  - the place's `attack: player` (StartCombat right after PlaceAtMe) did not take, in
    this save at this spot.
- **Creature sessions.** Distance of the in-combat creature to the player (median, p10),
  with its swings and hits on the player:

| Session | Samples in combat | Distance median / p10 | Swings | Hits on the player |
|---|---|---|---|---|
| coyote, no god mode | 903 | 127 / 84 | 5 | 5 |
| mantis, no god mode | 1,775 | 189 / 160 | 12 | 12 |
| mantis, god mode | 1,806 | 164 / 162 | 18 | 18 |

- **The mantis comparison.** The mantis kept the same distance with and without god mode
  and hit as often or more with it. So god mode does not make it hold off, as §9 found
  for the knife, sledgehammer and dog indoors.
- **The dog sessions** (normal and tgm) recorded no dog in combat either; the spawn did
  not engage.

### `followers`: Boone hired in his room (od_03's hire)

- **Out through his door, following.** He arrived about 200 units to the player's
  **left**, level with the player. This is the third run, and agrees with od_03's two
  (owner-decision-captures.md §7): NVForever now puts a teammate 160 to the left.
- **His package at the door.** At the door his Follow script package
  (0x000B1609) changed to his own PATROL (0x000CB118): OnPackageChange and OnPackageStart
  in the door's sample. So the cell change drops the script package, as od_03 also saw.
- **Back in through the outside door.** The script's `Activate player` on 0x000CAFA5 took
  the player in this time. Boone, on PATROL and no longer following, came along and stood
  on the player's spot: offset 0, then 50 units after a few loops.
  - **Second case.** It is the second case of a teammate on a non-following package
    brought along, after od_03's MoveTo.
  - **What it rules out.** The door against MoveTo is not what kept od_03's waiting Boone
    behind; his `Waiting` 1 is the remaining difference [hypothesis].
- **Not measured.**
  - **The walk.** In `follow_walk` Boone was on his patrol in another cell, so no follow
    distance or sneaking could be read.
  - **The coyotes.** In `combat` they never set on the player: one was seen, entered
    combat at 11:12 with another actor, and died. Boone's own combat at 10:53 (package
    FF001317) came before that and had no coyote as its target.
  - **To measure them, the session needs** Boone kept on his follow package after the
    door (re-adding it as `timed`) and coyotes placed nearer.

### `package_durations`: a Novac day, packages with a start hour

From 08:05 to 08:05 the next day (2,180 samples); the package events and the samples agree.

| Actor | Package (schedule) | What happened |
|---|---|---|
| Boone | PatrolToHome 0x000CB118 (9:00, 2 h) | began 9:00; still on its patrol procedure (41) at 11:00, when the Sandbox took over |
| Boone | Sandbox 0x000CAFC7 (9:00, 12 h) | 11:00 to 21:00 |
| Boone | PatrolToNest 0x000CB136 (21:00, 12 h) | began 21:00, DONE (17) at 0:00, held on |
| Manny Vargas | PatrolToNest 0x000CB144 (9:00, 12 h) | began 9:00, DONE at 12:00, held until 21:00 |
| Manny Vargas | PatrolToHome 0x000CB135 (21:00, 2 h) | 21:00 to 23:00, then his Sandbox 0x000CAFA6 |
| Cliff Briscoe | shop sandbox 0x000CD214 (6:00, 17 h) | held until 23:00 (past PatrolHome's and Sleep's 22:00 start), then 0x000CD215 until 6:02 |

- **Verdict [observed].** A package with a start hour runs exactly its window, start to
  start plus duration:
  - a goal met early does not end it (Manny's patrol, DONE at 12:00, held until 21:00);
  - a goal not met does not keep it past its end (Boone's PatrolToHome ended at 11:00
    mid-patrol).
- **Against our runtime.** This is NVForever's window rule, so it needs no change. It is
  the opposite of the packages with no start hour: the Goodsprings geckos' ran past their
  duration until their goal was met (owner-decision-captures.md §7; NVForever follows
  that since 2026-10-19).
- **Cliff Briscoe.** At 22:00 he kept his shop sandbox, still within its window, over
  PatrolHome and Sleep, which come later in his list. So the first valid package of the
  list wins [observed], as in our choice.
- **Not compared one by one.** Andy, No-bark and Jeannie May's switches are in the events
  file (`round3\package_durations.events.txt`).

### What is still open from §12

- **NPC melee hold-off.** It needs a spawn that engages: the trooper placed with
  `attack: player` never fought in `nvfstrip`.
- **Boone's follow distance, sneaking and joining a fight.** They need him kept on his
  follow package after a door, and coyotes that reach the player.
- **Radroaches against NPCs.** They never closed in.

## 14. Round 4: what §13 left open, run (2026-10-19) [observed]

**The run.** Owner's desktop, plugin version 9, both game locks held, save `nvftest`, one
launch per campaign (per session where the campaign says so); `observe.py run … --data
<Data>`. Campaigns in `tools/capture/observe/`: `melee_holdoff_npc` (run twice),
`followers_follow`, `radroach_spacing`, `blast_knockdown`.
- **Outputs.** `C:\fnv-private\capture\out\obs_{melee_holdoff_npc,followers_follow,radroach_spacing,blast_knockdown}_*`.
- **Run reports and timelines.** `C:\fnv-private\capture\round4`.

### Why the spawned troopers never fought (§13)

- **What the first run showed.** In `nvftest` too, every trooper spawned with an `offset`
  and `attack: player` stood still out of combat for the whole session. It was in
  procedure 11 (FOLLOW) from its first sample. The radroaches of `radroach_spacing`'s
  first sessions stood the same way (procedure 0, TRAVEL). This copy of `gun_reach`'s set-up did
  the same, with killall in `console_first`. So neither the save nor where killall runs
  is the cause.
- **What fixed it.** The second run gave StartCombat its own scan 30 game loops after
  the MoveTo (`place` key `attack_late`, ai-observer.md §12). Then all six troopers
  engaged, and so did a trooper spawned without an offset (as `fights_round`'s).
- **Why it fails.** That the queued MoveTo drops the StartCombat is a [hypothesis].
  Creature sessions worked because the creatures were hostile to their targets anyway.

### NPC melee hold-off (§8.4): troopers against a normal player outdoors

Second run. Distance is to the player in 2D, over the trooper's samples in combat. Swings
are GetAnimAction entering 2. Hits are OnHit events on the player.

| Session | Spot | God mode | Distance median / p10 / least | Swings | Hits |
|---|---|---|---|---|---|
| a_knife_late | A (Violet's dog) | no | 104 / 83 / 49 | 14 | 10 |
| a_sledge_late | A | no | 198 / 90 / 43 | 6 | 5 |
| a_sledge_tgm_late | A | yes | 149 / 119 / 98 | 15 | 12 |
| b_knife_nooffset | B (south of Cook-Cook's camp) | no | 107 / 72 / 39 | 16 | 10 |
| b_sledge_late | B | no | 137 / 119 / 116 | 15 | 11 |
| b_sledge_tgm_late | B | yes | 126 / 105 / 77 | 15 | 14 |
| b_knife_late | B | no | 1,366 / 700 / 690 | 7 | 1 |

- **Verdict.** Outdoors, at both spots, melee troopers close in on a player who is not in
  god mode and swing at them, as they did indoors (§9). God mode changes neither the
  distance nor how often they swing.
- **What it means for §3.** §3's hold-off at 147–177 units without a swing is not
  reproduced. A sledgehammer trooper did stand 126–149 units off, but swung 15 times.
  NVForever's attackers close in, so nothing changes, and §8.4 is closed.
- **The outlier.** b_knife_late's trooper was on the player the whole session (target
  0x14) but never came nearer than 690 units. It was one run [hypothesis: its way to the
  player was blocked].

### Boone, kept on his follow package (`followers_follow`)

- **Out through his door, again.** At the door the package changed to his PATROL, as in
  od_03 and §13. The script added the Follow package back 120 samples later
  (OnPackageStart 0x000B1609).
  - **Where he came out.** This time he arrived 136 units to the player's **right** and
    58 ahead: offset (−35, −144), player heading 126.8°.
  - **Against the earlier runs.** od_03's two runs and §13 had him on the left (about 170
    left, 12–30 ahead).
  - **What differed.** Before the door he stood 95 units in front of the player, not
    beside him. So the side is not fixed [observed]. What decides it is a [hypothesis to
    test: his place before the door]. NVForever's `followers::BESIDE` (160 left) holds
    for the earlier set-up only.
- **Standing still on the follow package.** After the door he stood 383 units off and
  did not set off. The package's distance is 330 (PTDT of
  FollowersBooneFollowPlayerDEFAULT). That fits a margin before a follower sets off; ours
  is 64 (`FOLLOW_SLACK`).
- **The walk to Manny Vargas** (the player walked about 2,000 units, then stood):
  - **On the way.** Boone kept 180–400 units behind and was running throughout. The
    package has Always Run, which ours follows too.
  - **Once the player stopped.** He came to 177 units behind (15 left) and stood there.
    So he came in well inside his package's 330. Ours stops once within the distance, so
    about 330 off [hypothesis: the original stops at about half the distance; one
    arrival].
- **Sneaking.** The player was force-sneaked for 600 samples (about 20 s, most of it
  standing after the walk). Boone's IsSneaking stayed 0 throughout [observed]. A follower
  does not crouch with the player, at least not when standing. (Withdrawn as a general
  reading: SetForceSneak did not make the player sneak in a later check
  (docs/handoff/observer-checks.md item 6), so the player here may not have been sneaking
  [hypothesis]; a teammate copied a sneak made with the sneak control, §15.1.)
- **Joining a fight.** Two coyotes spawned 600 units off started combat on the player.
  - **When he joined.** Boone's OnStartCombat, on a coyote, came 9 samples (about 10
    game seconds) after the first coyote's. His package became a combat package (0xFF…,
    procedure 13).
  - **How the fight went.** He hit each coyote once and killed both. His combat ended
    at the second death, and the Follow package (0x000B1609, procedure 11) came back by
    itself.
  - **Unexplained.** IsInCombat read 1 for him from the session's first sample, before
    either coyote's OnStartCombat.

### Radroaches against NPCs (§8.2)

- **First run.** The three radroaches were all moved to one spot by the offset and stood
  out of combat, as above. In the unarmed session one radroach fought.
- **Second run.** With one place item each, 150 units apart, and `attack_late`, all three
  engaged. The troopers shot two dead within about 25 s.
- **Hits.** The radroaches' hits on troopers landed at **107** and **112** units centre
  to centre.
- **Against our rule.** (RNAM + 59) × scale gives 123 for a radroach (CrRadroach RNAM 64,
  scale 1.0).

**§13's creature hits set against the rule.** The rule's figures are swing starts, the
measures here are hit distances, so the rule should come out somewhat larger:

| Creature (template, RNAM, scale) | (RNAM + 59) × scale | Hits seen (min / median / max) |
|---|---|---|
| Deathclaw (CrDeathClaw, 192, 1.0) | 251 | 136 / 215 / 270 |
| Young deathclaw (CrDeathClawBaby, 100, 0.75) | 119 | 94 / 105 / 116 |
| Cazador (NVCrCazador, 100, 1.0) | 159 | 86 / 93 / 100 |
| Radroach (CrRadroach, 64, 1.0) | 123 | 107, 112 |

- **Deathclaws and radroaches.** They fit the rule as it stands.
- **Cazadors.** They hit from well inside it [hypothesis: a flyer closes in to strike].
  This was too few hits to fit them a rule of their own.

### An explosion's knockdown (`blast_knockdown`; docs/outstanding.md row 1) — not reproduced

Each explosion was placed with PlaceAtMe on the actor (`<actor>.PlaceAtMe <EXPL> 1`). The
player was in god mode. Every hit arrived (OnHit and OnHitWith with the explosion's
form), but **no actor was knocked down**: GetKnockedState stayed 0, the body did not move
and no get-up began.

| Session | Explosion (flags) | Actor, what it was doing | Result |
|---|---|---|---|
| force | WeaponKnockdownExplosion 0x001535F0 (damage 0, Knock Down – Always, Push Explosion Source Ref Only), by Ringo's PlaceAtMe | Ringo walking (procedure TRAVEL) | hit, no damage, not knocked |
| force_player | the same, by the player's PlaceAtMe, 125 units from Ringo | the player | the player hit; Ringo untouched |
| frag | GrenadeFragExplosion 0x000179DA (damage 125, Knock Down – By Formula) | Ringo sitting (procedure 48) | 123 damage, not knocked |
| frag_standing | the same, three times, 300 samples apart | Chet behind his counter on an idle marker (procedure 46) | 123 damage each, not knocked; WAIT then DIALOGUE procedure for a moment after each, then his marker again |

What this leaves [hypothesis, to test]:
- **"Source Ref Only".** A PlaceAtMe'd explosion may have no source reference, so it
  pushes nobody.
- **Furniture and idle markers.** Actors sitting or on an idle marker may not be knocked
  down at all.
- **The formula.** Its chance may simply not have come up in four tries.

The test that would settle it: a frag on an actor standing free (a spawned NPC, out of
furniture) with several tries. NVForever's rule (unconscious.md §5.2: knockdown by the
EXPL flags and the formula) is unchanged.

### What is still open after round 4

- **Where a teammate comes out of a door.** The side changed with where Boone stood
  before the door (left in three runs, right in one). It needs a run with him placed
  beside, behind and in front of the player.
- **The follower's stopping distance.** He came in to 177 against the package's 330,
  from one arrival. Several arrivals (walks of different lengths) would show whether it is
  about half the distance.
- **An explosion's knockdown** on an actor standing free (above).
- **Cazadors' reach.** It needs more hits.

## 15. Parity round 1: ten questions put to the original (2026-11-10/11) [observed]

Campaign `parity_round1`, save hudtest, xNVSE 6.4.9 with jip_nvse and the capture plugin
(ai-observer-jip.md). JIP LN's fixes were installed, so "the original" here
is the original with JIP. Ten questions, twelve sessions, and re-reads of three older
captures (§15.8). Every reading of the original is from private captures (never
committed); §15.10 is a run of NVForever. One scene each: one actor, one place, one run unless a line says otherwise. Times are real
seconds from the session's first sample (game seconds / TimeScale). Set-up steps (moves,
SetAngle, AddScriptPackage, setting changes) are the campaign's, not observation. The
player was in god mode in the sessions with hostiles.

**Making the player sneak.** `player.SetForceSneak 1` left the player's IsSneaking at 0.
The sneak control (`TapControl 8`) toggled it 0 → 1 and 1 → 0 [observed:
docs/handoff/observer-checks.md item 6, 2026-11-01]. The sessions
below that needed a sneaking player used the control and read the player's IsSneaking.

### 15.1 A teammate's sneak (`q1_companion_sneak`)

GoodspringsSettler01Ref was made a teammate on DefaultFollowPlayerClose, standing 126
units from a standing player.

- **With the teammate flag set.** The player's IsSneaking went to 1 at 9.8 s, and the
  NPC's read 1 in the same sample. The player stood up at 67.9 s, and the NPC's went to 0
  in the same sample.
- **With the flag cleared** at 87.2 s, its Follow package still running. The player
  sneaked from 92.2 s to 131.0 s, and the NPC's IsSneaking stayed 0 throughout.
- **So.** This teammate copied the player's sneak at once. The same NPC as a follower that
  was not a teammate did not, standing at 126 units. This is what companion-wheel.md §6
  builds. A moving leader and an NPC leader were not tried.
- **Against §14.** There Boone's IsSneaking stayed 0 while the player was "force-sneaked"
  with SetForceSneak. Since that function did not make the player sneak in the 2026-11-01
  check, §14's reading may have had no sneaking player [hypothesis].

### 15.2 A door an NPC opened (`q10_door_follower`)

The Prospector Saloon's inner door 0x00107C77 was closed by script.
GoodspringsSettler04Ref followed the player through it and back.

- GetOpenState read 3 (closed) to 5.3 s, 2 (opening) from 5.5 to 6.5 s, and 1 (open) from
  6.6 s to the end at 203.9 s.
- The follower went through to the player. After the player was moved out at 43.3 s, he
  came back out.
- **So.** The door stayed open for the remaining 197 s, through two passages: the
  original did not close it behind the NPC in this scene. NVForever leaves it open too
  (navmesh.md §7, item 5).
- **Not tried.** A door on a package route with nobody following, doors with the
  Automatic Door flag, exterior load doors.

### 15.3 Find (`q8_find_settler`, `q8_find_dog`)

Both actors were given FFER24FindFood (Find, one mole rat meat, near the current
location, radius 1000), with the meat placed 300 to 440 units away in the open.

- **GoodspringsSettler03Ref.**
  - At 1.6 s: procedure 2 (ACQUIRE), with the placed meat as package target.
  - By 10.5 s: his distance to the meat had fallen from 438 to 111. His item count went
    from 6 to 7, with an OnActivate event by him, and the package then changed.
- **A copy of GSCheyenne** on the Goodsprings runway, the meat 300 units off.
  - At 0.1 s: procedure 2, package target the meat.
  - At 5.3 s: item count 0 → 1, with an OnActivate event by the dog, then its own package
    back.
- **So.** Both went straight to the object, which is what NVForever's Find does
  (packages.md §3.1); our matching run of the settler's scene did the same (the dog's
  scene did not unfold on our side and says nothing). The
  2026-10-17 run in the saloon (packages.md §4: Cheyenne wandering, the meat untouched)
  does not hold as a general rule. What made it differ is open: the object not reachable
  or not seen indoors [hypothesis].

### 15.4 A body on the way (`q2_corpse_found`)

A settler (GSSettlerAM) was killed by script 700 units up the runway before the walker
existed. A second settler (GSSettlerAAM) was then given a Follow package and ran 2,000
units to the player, past the body.

- He passed within 42 units of the body, then stood 424 units from it for the remaining
  150 s.
- GetIsAlerted 0, GetAlarmed 0, IsWeaponOut 0, IsTalking 0 and procedure 11 (FOLLOW) were
  read at every sample.
- **So.** A passer-by of the body's own kind, by day, did not react. Ours has no
  reaction to bodies either. The wiki prose that "dead bodies seen put an actor in
  Caution" (detection.md §9) is not supported for this case.
- **Not tried.** A guard, the body of an ally killed by a hostile, a body indoors.

### 15.5 A seen theft (`q3_theft_seen`)

Set-up in Chet's store (GSGeneralStore): the clock set to 10:00, Chet moved 200 units
from the player and turned to face the player. The owned carton 0x00106D34, 180 units in
front of Chet, was activated with the player as actor.

- **2.8 s.** The theft counted: the player's item count went 27 → 28 and karma 0 → −5.
  iStealWarnings read 2 from the game.
- **3.0 s to 23.0 s.** Chet ran procedure 47 (TAKE_BACK_ITEM) for 20.0 s, with IsInCombat
  0 and no combat target at every sample of it.
- **23.0 s** (20.2 s after the theft counted). IsInCombat 1 with the player as target, an
  OnStartCombat event in the same sample, IsTalking 1 from 23.0 to 24.8 s, then
  procedures 4 and 13.
- The item was not taken back. The player stood still, and any menu was closed by the
  campaign.
- **So.** This owner did not attack at once: he spent about 20 s in a take-back procedure
  first. NVForever goes straight to the fight (crime-karma.md §3.1, take-back not built).
- **Not seen.** The take-back when the player answers or drops the item; warnings counted
  against iStealWarnings; a second theft.

### 15.6 A target hidden outright (`q4_lost_default`, `q4_lost_x4`)

A lone LvlFiendGun, 700 units from the player on the runway, was set on the player. The
player's detection value was forced to −100 (JIP ForceActorDetectionValue) at about
5.6 s.

- **Built-in timers.** The running game reported fCombatDetectionLostTime 15 and
  fCombatTargetLostRemoveTime 30 (neither is set by a master).
  - The Fiend kept IsInCombat 1 and the player as combat target until 26.3 s, 20.7 s
    after the hide.
  - Then: IsInCombat 0, no target, procedure 0, and from 27.5 s procedure 7 (WANDER). It
    walked about, once within 94 units of the hidden player.
- **Four timers ×4**, set before the Fiend existed and read back: fCombatDetectionLostTime
  60, fCombatTargetLostRemoveTime 120, fCombatSearchTargetHiddenRemoveTime 40,
  fCombatSearchTargetHiddenMaxTime 80.
  - Combat with the player as target held until 70.4 s, 64.8 s after the hide. Then
    IsInCombat 0, no target, procedure 7.
- **Procedures.** No SEARCH (36) at any sample in either run; no PURSUE in the first run
  (not looked for in the second). GetIsAlerted read 0 throughout the first run. Shown again, the Fiend was in combat within the same sample
  (139.3 s, 265.4 s).
- **So.** These settings are live in the original: changing the four together moved the
  21 s to 65 s. Which one does it was not isolated. "fCombatDetectionLostTime plus about
  5 s" fits both runs (15 + 5.7, 60 + 4.8) [hypothesis: two points]. NVForever's timers
  are its own and read none of these settings (alert.md §2; #2010). The 2026-11-09 runs
  in which Fiends stayed in combat for 235 s lost the player differently (an invisibility
  effect, far away, the Fiends standing still).

### 15.7 Sandbox durations (`q9_saloon_sandbox_default`, `q5_saloon_sandbox_x5`): suggestive

GoodspringsSettler04Ref was on DefaultSandboxEditorLocation1024 in the Prospector Saloon.
The second session set fSandboxDurationBase to 50 (default 10).

- **Default, 480 s.** 30 state runs. Sits (procedure 48, GetSitting 3) of 135.0 s,
  134.4 s and three of 19.5 to 34.4 s; one wander (procedure 7) of 9.2 s.
- **×5, 360 s.** 10 state runs. One sit of 270.1 s (the session's end may have cut it)
  and one wander of 65.3 s.
- **So.** With the base raised he went through fewer states (10 in 360 s against 30 in
  480 s), and his one wander and his longest sit were longer. That fits the setting being
  live [hypothesis: one actor, a handful of spans, windows of different lengths and a sit
  the end may have cut]; no multiplier follows from it. He used chairs
  only (markers 11, 12, 15).

### 15.8 Work spots: Gomorrah's bartenders (an existing capture, re-read)

Re-read for GetFurnitureMarkerID of a six-hour Gomorrah capture from a sandbox check of
2026-11-04 (not the §6 casino session).

- Four actors read GetFurnitureMarkerID 22 with GetSitting 4 for about 2,700 of 2,745
  samples each (procedure 21). Their package targets were marker references and one
  other base.
- One target, 0x00122BC4, is a placed furniture whose Editor ID is `BarKeep` (base
  0x000B24DB). Actor 0x00122BC1 used it for 2,521 samples, and 0x00122BC5 used 0x00122BC7
  likewise.
- **So.** These bartenders stayed on their BarKeep work spots (marker 22) for hours. NVForever does not use work spots (furniture.md §7, "Not done").
- The two Goodsprings captures re-read the same way showed chairs only (markers 14, 15).

### 15.9 Inconclusive sessions

- **Greeting (`q6_greet_pete`).**
  - The running game reported fAIMinGreetingDistance 150, fHelloCooldownTime 30 and
    fAIGreetingTimer 20, the values of the wiki's Complete List of GameSettings (no
    master overrides them).
  - Easy Pete, seated (GetSitting 3), never read IsTalking 1 while the player stood at
    800 down to 100 units in front of him and back. He detected the player from 6.0 s.
  - So nothing about the distance or the cooldown. A talkative standing NPC is needed.
- **Ambush (`q7_ambush_undetected`).**
  - A LvlFiendGun was created 3,100 units from the player with
    QJDeathclawAmbushPackage. IsInCombat read 1 at 0.1 s and the player was its combat
    target from the next sample, while its GetDetected on the player read 0 (1 from
    4.4 s).
  - But the player already read "in combat" with detection state 3 at the first sample,
    so something else may have started the fight [hypothesis].
  - The scene needs the player hidden and out of combat before the ambusher exists.

### 15.10 Ours: a patrol interrupted by a fight (#2013) [observed in NVForever]

A build of NVForever at c27661b55, smoke scenes, the engine's own log lines. Oliver Swanick
(vNiptonSurvivorREF) was on his patrol vNiptonSurvivorCelebrate01 in Nipton: three
markers, waits of 1, 1 and 5 s.

- **Left alone, 150 s.** 33 patrol legs, looping to the end.
- **A gecko set on him mid-leg** at 23 s and killed by script at 32 s.
  - He stopped, faced it, and at its death logged `Combat → Normal` and `stops searching:
    back to Normal`, then holstered.
  - He logged nothing more for the remaining 118 s: 6 legs in all, his package unchanged,
    108 units short of his third marker.
- No capture of the original for this case is recorded here. Resuming an interrupted patrol is the
  design's own rule (`fnv_ai::carry::back_to_package`).

## 16. Parity round 2 (2026-11-11) [observed]

Campaign `parity_round2`, set up as §15 (save hudtest, xNVSE 6.4.9 with jip_nvse and the
capture plugin, so the original with JIP LN's fixes; private captures, never committed).
Each of the 25 sessions ran in its own game start after a god-mode session. One scene
each, one actor per role, one run unless a line says otherwise. Times are real seconds
from the session's first sample. Placed actors were told apart by a number written to
their Variable05 actor value. The runway scenes are on the Searchlight Airport runway
with the god-mode player; the player's detection was forced with JIP
ForceActorDetectionValue where a line says "hidden" or "made detectable". The
readings of the original are [observed]; lines on NVForever cite the code or design note
they come from, and wiki lines carry their own tag.

### 16.1 A target hidden outright: which setting (`lost_detect_only`, `lost_remove_only`, `lost_hidden_pair`, `lost_default_repeat`)

§15.6's scene again (a lone LvlFiendGun set on the player, the player hidden at 3.5 s).
fCombatDetectionLostTime and fCombatTargetLostRemoveTime were changed separately, the two
search settings together; the header read all four back each time. One run per setting,
the time from the hide to the combat target being cleared.

| Settings (DetectionLost / TargetLostRemove / SearchHiddenRemove / SearchHiddenMax) | Kept after the hide |
|---|---|
| 15 / 30 / 10 / 20 (the built-in values) | 20.7 s (round 1), 22.7 s (this round; combat ended 24.2 s after the hide) |
| 15 / **120** / 10 / 20 | 20.6 s |
| 15 / 30 / **40** / **80** | 22.6 s |
| **60** / 30 / 10 / 20 | 65.0 s |
| **60 / 120 / 40 / 80** (§15.6) | 64.8 s |

- **So.** In this scene only raising fCombatDetectionLostTime made a large shift: the time
  came to about its value plus 5 to 8 s (offsets 4.8 to 7.7 s). The other configurations
  fell inside the spread of the default runs (20.6 to 22.7 s) [hypothesis:
  fCombatDetectionLostTime governs it; one run per setting cannot rule out a small effect
  of the others or an interaction]. What the extra seconds are was not isolated.
- No SEARCH (36) procedure appeared at any sample in any run. After combat the Fiend
  wandered (procedure 7) in the runs where that was read (§15.6, `lost_detect_only`).

### 16.2 Ambush (`ambush_hidden_inside`, `ambush_detected_outside`)

A LvlFiendGun was given QJDeathclawAmbushPackage (ambush location: its editor location,
radius 5000). It was created only after the player read out of combat (player_in_combat
0 at sample 0 in both).

- **Hidden, inside the location.** The player was hidden 2,000 units away. The ambusher
  waited in procedure 30, not in combat, with GetDetected 0, for 56 s. The player was
  made detectable at 56.1 s: combat with the player as target in the same sample.
- **Detected, outside the location.** The player was detectable at 5,800 units:
  GetDetected 1 from 7.7 s, no combat for 58 s. The player was moved to 2,000 units
  (inside) at 65.9 s: combat in the same sample.
- **So.** This ambusher sprang on an enemy that was both detected and inside its ambush
  location, as the GECK wiki's Ambush Package page says [documented: Ambush Package].
  NVForever's ambush springs on an enemy inside the location without a detection test
  [design: packages.md §3.1], and a detected hostile outside it starts combat as any
  detected hostile does [design: alert.md §2].

### 16.3 Dying (`dying_timer`)

fDyingTimer read 2.

- **KillActor** at 1.9 s: GetDead read 1 in the same sample; the OnDeath event came at
  3.88 s.
- **Shot dead**: the last OnHit event came at 30.11 s (the event reported health 1.76;
  the sample at 30.1 s read -2.57); OnDeath at 32.12 s.
- **So.** In both deaths the xNVSE OnDeath event came 2.0 s (fDyingTimer) after the
  death, while GetDead already read 1. NVForever runs the OnDeath blocks with no dying
  delay: at the kill, or, for a death inside a script, once that script run ends [design:
  `Game::kill`, `crates/fnv-game/src/combat.rs:385`, `:412-416`]. That the OnDeath
  block runs with the event is a [hypothesis]. GetLifeState (JohnnyGuitar) was not
  installed.

### 16.4 StartCombat (`startcombat_allies_unseen`)

- **Between allies.** A GSSettlerAAM was told to StartCombat a GSSettlerAM, both in
  GoodspringsFaction, which lists itself as an ally. Neither read IsInCombat 1 from the
  order (3.7 s) until both were disabled (23 s). The wiki's StartCombat page says it does
  not work between allies [documented: StartCombat]; NVForever's StartCombat puts the
  target on the actor's list as detected whatever their factions [design:
  `Game::start_combat`, `crates/fnv-game/src/awareness.rs:1073`].
- **On an unseen target.** A LvlFiendGun created 1,500 units behind the hidden player was
  told to StartCombat the player. IsInCombat 1 and procedure 13 came in the same sample,
  the player as target in the next, and it ran to the player's spot; ours starts combat at
  once too [design: `Game::start_combat`]. It never detected the player and left combat
  after 10.1 s, about half the time a detected-then-hidden target was kept at the default
  settings (§16.1) [hypothesis: a separate rule for a target never detected].

### 16.5 An essential knocked out (`essential_getup`)

fEssentialDeathTime read 10. A GSSettlerAAM was made essential with
SetActorRefEssential and set on the player by StartCombat.

- DamageAV LeftMobilityCondition 100 at 7.7 s crippled the leg (0).
- DamageAV Health 500 at 9.7 s knocked it out. In that same sample: IsInCombat 0, no
  combat target, and the leg back at 100. It got up at 19.7 s (10 s later), out of combat.
- **So.** The original cleared the essential's crippled limb and its fight at the
  knockout. NVForever keeps both: the knockout clears neither the actor's own fight nor
  its limbs, and getting up restores only Fatigue and Health [design:
  `crates/fnv-game/src/unconscious.rs:221-256`, `:375-427`; `Game::settle_awareness`,
  `crates/fnv-game/src/awareness.rs:829-859`, drops only dead or absent actors]. The fight here came from a script's
  StartCombat, not an assault by the player.

### 16.6 Assistance 0 and a follower (`assist_follow`)

All in GoodspringsFaction and Aggression 0:
- L, default AI;
- F, Assistance 0, linked to L on DefaultFollowLinkedClose;
- C, Assistance 0, not following;
- D, its base's Helps Friends and Allies (the control).

A fist-only LvlFiendGun was set on L at 10.0 s.

- D started combat on the Fiend at 12.0 s. C and F, never hit, stayed out until 17.6 /
  17.7 s, when the Fiend fell (health 1.1, its combat ended in the same sample).
- **So.** Assistance 0 kept C out of a fellow's fight for 7.6 s while the control helped
  within 2 s, as the AI Data Tab's "Helps Nobody" says [documented: AI Data Tab].
  NVForever's combat-group alarm lets an actor of the sender's own kind answer whatever
  its Assistance [design: `crates/fnv-game/src/combat_group.rs:583-586`; combat-ai.md
  §12.3]. Why both joined when the Fiend fell is not known.
- F did not defend its leader either, but its own Assistance 0 confounds that case.

### 16.7 Aggression 3 (`frenzied`)

Three GoodspringsFaction settlers. SetAV Aggression 3 on one at 4.8 s: it was in combat
at 4.9 s, with one fellow as target at 5.1 s and the other at 8.0 s; both fought back.
The original's Frenzied actor attacked its own faction. NVForever's does not start combat
against these kin (a shared faction, with the exceptions alert.md §3 lists) [design:
alert.md §3].

### 16.8 CastImmediateOnSelf (`cios_duration0`)

- At 4.6 s a settler at 5 health was given CIOS VMQ03RestUp (Restore Health 10000,
  duration 0): 45 health in the next sample.
- At 15.3 s, with a crippled leg (0) and 15 health, it was given CIOS
  VPitFighterHealthBoost01 (Restore Health over 3 s, plus duration-0 limb restores): the
  leg read 107.5 in the next sample, and health was back to 45 within 0.4 s.
- **So.** CIOS applied duration-0 effects; the "Limitations" line of the wiki page does
  not hold here [documented: CastImmediateOnSelf]. NVForever applies the effects with
  their durations [design: `crates/fnv-game/src/functions/magic.rs:61-65`].

### 16.9 A body killed by a hostile; a theft beside an ally; crime after a day (`corpse_killed_by_hostile`, `theft_ally`, `theft_expiry_1day`, `theft_expiry_4day`)

- **A body.** A settler was killed by KillActor with a LvlFiendGun as the killer, and the
  Fiend was then disabled. A walker on a Follow package passed within 12 units of the body
  with no change: not alerted or alarmed, no weapon out, procedure 11 throughout. With
  §15.4 that makes two scenes with no reaction; ours has none built [design: detection.md
  §9].
- **A theft beside an ally.** §15.5's theft, with GoodspringsSettler01Ref held beside Chet.
  - Chet was again in procedure 47 from 3.0 to 23.0 s (20.0 s, as in §15.5), then attacked.
  - The settler, never hit, attacked the player at 26.9 s, 3.9 s after Chet. The
    victim's ally joined, as NVForever's alarms bring in the victim's own kind [design:
    combat-ai.md §12.3].
- **A day later.** After the same theft the clock was moved on by script, 1 day in one
  session and 4 in the other. Chet was not hostile at the start of either observation:
  - GetActorCrimePlayerEnemy 0 and not in combat;
  - karma -5, so the theft stood.

  When the hostility ended was not captured, so the 3-day iCoolDownTimerSetting was not
  tested.

### 16.10 Inconclusive and void sessions

- **Inconclusive.**
  - `tether_teammate`: the Fiends brought the fight to the player, and every actor
    dropped in and out of combat about every 13 s.
  - `cdc_wander` and `cdc_travel`: the Travel control left its package for combat too
    (package type 5 during the fight), so the comparison says nothing.
  - `no_combat_alert` (rerun): the control reacted to none of the stimuli either.
  - `aggro_radius_npc`: a radroach attacks a person it detects anyway.
  - `npc_leader_sneak`: out of combat the follower did not copy its NPC leader's sneak;
    the in-combat case did not occur.
  - `ko_essential_activate`: a script's `Activate player` opened no menu, not even
    dialogue for the conscious control.
- **Void.** In `door_creature`, game time stopped the sample after the player was moved
  through a closed door into a room, and the session never ended.
- **Hooks that did not compile.** Three sessions (`cdc_wander`, `cdc_travel` and the
  first `no_combat_alert`) had hooks that did not compile at run time. Each of them, and
  only those, declared a reference variable named `rS`, `rN` or `rK`; hooks of the same
  shape with other names compiled [observed]. Why is not known.
