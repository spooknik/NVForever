# Observer checks: gameplay guesses the original game can settle

Guesses about how the original behaves that a capture can settle without the owner.
The observer can run scripts, take screenshots, record video, and tap keys, so it can
also drive menus. Sorted by how much of the game each check moves, most first.
Taken from the old owner-review log on 2026-10-24 (full log:
`git show 85dcaace:docs/owner-review.md`); decisions that need the owner are in
[owner-review.md](../owner-review.md).

Tools: [ai-observer](../spec-notes/ai-observer.md) (campaigns, fight field set, events,
timed actions, shots), [xnvse-capture](../spec-notes/xnvse-capture.md) (jobs and short
scripts), [owner-decision-captures](../spec-notes/owner-decision-captures.md) (what
earlier rounds settled). Start from the `nvfstrip` save unless a check needs an early
game state. When a check is settled, write the answer into the linked spec, fix the
engine if needed, and delete the item here.

## P1: changes many fights or the whole run

1. **Which weapon NPCs carry and draw.** Ours ranks weapons by base damage plus
   explosion damage, so about 40 NPCs now draw grenade rifles or launchers. Any AddItem
   (even caps) makes an NPC equip its best weapon and armour again. Melee Only / Ranged
   Only styles switch weapons only in combat. To check: read GetEquippedObject 5 on
   those 40 NPCs and on a sample of armed NPCs at first load, and again after AddItem
   and after a combat start. ([npc-behaviour-compare §9.1](../spec-notes/npc-behaviour-compare.md),
   [combat-ai §18](../design/combat-ai.md), [companion-wheel §4](../spec-notes/companion-wheel.md))
2. **Who attacks you, and who joins a fight.** Ours:
   - 618 placed Aggressive (not Very Aggressive) animals ignore you until attacked.
   - Inside an aggro radius, an actor warns twice 5 s apart, then attacks.
   - Actors of a leveled template take the factions of our level-1 entry (192 types).
   - Sharing a faction or base NPC makes actors allies.
   - Onlookers who would be attacked by the attacker join at once.
   - Cowardly actors never help.
   - An escort fights its charge's attackers.
   - Anyone hostile to you is hostile to your teammates, and teammates ignore your hits.
   - People on Find with Allow Killing attack only actors carrying the item (creatures
     hunt anything; already captured).
   To check: run campaigns near coyotes, rats and bighorners at Goodsprings, the Primm
   ambushers, a gecko attack on a settler, an escort, and shots at Boone, with the fight
   field set and OnStartCombat events. ([factions](../spec-notes/factions.md),
   [combat-ai §10, §12, §13](../design/combat-ai.md), [followers](../design/followers.md),
   [packages §3.1](../spec-notes/packages.md))
3. **Damage numbers.** Ours:
   - Power attacks do 3× standing and 2× moving.
   - A damage type uses its resistance in place of DR (capped at 85 %, after DT).
   - Rippers, Chainsaw and Thermic Lance ignore DT/DR.
   - `fCombatPlayerLimbDamageMult` halves damage to the player's limbs.
   - Difficulty multipliers are 2/1.5/1/0.75/0.5 dealt and the inverse taken, including
     limb, burn and poison damage and your own grenades.
   - Your own explosives hurt you.
   - A second burn or poison hit restarts the timer rather than stacking.
   - Every weapon's Crit Effect applies.
   - Fall damage is 0.0252 × (h − 600)^1.65, with no leg crippling.
   To check: OnHit events plus Health and limb values for each case.
   ([combat-rules §10](../spec-notes/combat-rules.md),
   [weapon-data-fields](../spec-notes/weapon-data-fields.md),
   [crippled-limbs](../spec-notes/crippled-limbs.md), [difficulty](../spec-notes/difficulty.md),
   [hit-effects](../spec-notes/hit-effects.md), [physics §6.12](../spec-notes/physics.md))
4. **Hardcore needs and waiting.** Ours:
   - Hunger rises 4.8/h and sleep deprivation 2.4/h. Dehydration is guessed at 9.6/h
     (Minor at about 21 h).
   - An hour slept takes 100 off sleep deprivation. Needs rise while asleep.
   - Beer and Nuka-Cola make you thirstier (their "Damage Dehydration" read literally).
   - An hour waited counts as 120 s of effect time.
   - Plain beds don't heal, sleeping heals limbs only outside Hardcore, and waiting is
     refused while you are being irradiated.
   To check: read the need actor values over waits and sleeps, after drinks, and with
   Buffout active. ([aid-items §2, §6, §11](../spec-notes/aid-items.md),
   [wait-menu](../spec-notes/wait-menu.md))
5. **Weapon wear.** Ours:
   - A melee swing wears the weapon only when it lands (once per swing).
   - A record's Damage-to-Weapon Mult replaces 0.2 rather than multiplying it, so the
     Flamer, Incinerator, chainsaw and Rippers now wear 5× faster.
   - NPC weapons wear like the player's.
   To check: weapon health before and after N swings and shots, per weapon.
   ([combat-rules §17–§18](../spec-notes/combat-rules.md))
6. **Detection.** Ours:
   - Light level is ambient + sun + lamps; sunlight counts only where unshaded.
   - FOV is 95° each side, and sneak-running counts as running.
   - A lit flashlight doesn't raise your light.
   - Impacts and explosions alert at their own sound levels, and creature footsteps
     count ×2 Loud / ×0.2 Silent.
   - Lying on a bed counts as asleep.
   To check: GetDetected player while walking up to Sunny at noon and at night, with the
   flashlight on, on a porch, and with a missed shot near a sleeper.
   ([detection §8](../design/detection.md))
7. **Casino payouts.**
   - Slots: ours pays back about 1.35 per chip, so max winnings come fast. Check the
     reading of "= BET × N", the cherry rule and the middle row.
   - Blackjack: the dealer doesn't peek, split aces play on, one split only, surrender
     returns half rounded up, comps come at ¼ / ½ / ¾ of max winnings.
   To check: drive many hands and spins by key taps and read caps and chips.
   ([slots](../spec-notes/slots.md), [blackjack](../spec-notes/blackjack.md))
8. **Package timing.**
   - SetPackDuration 60 switched every 10–70 game minutes in the original. What does
     the number count?
   - A schedule duration with no start hour ends only when the goal is met.
   - Sandbox sit and stand times are our own numbers.
   - An unreachable package is set aside for 1 h.
   - MP A gives up her escort half-way in the original.
   - Random NPC chats start only with you within 300 units.
   - Force greets need a clear line, but facing doesn't matter.
   - A woken sleeper talks within 8 s and stays up 30 game-seconds.
   - People during a wait walk an hour's worth each hour.
   - **A far actor heading for you, past what was captured** (Malcolm Holmes, settled
     in part 2026-10-24: hourly jumps along a route, [ai-observer §13](../spec-notes/ai-observer.md)).
     Open: after you move, does he re-aim when his first route runs out (ours plans
     again then), or ever before? Run `holmes_far_moving` with part2 longer than 3 hours.
   To check: observer timelines, which already record packages.
   ([actor-behaviour §3, §5.5–§5.6, §5.9](../design/actor-behaviour.md),
   [owner-decision-captures §7](../spec-notes/owner-decision-captures.md))

## P2: rules met often

9. **Aid, perks and stats.** Ours:
   - Medicine/Food potency is 1 + 2 × the user's skill / 100, and Buffout boosts food
     healing.
   - Addiction: +30 usage per use, chance (usage − 70)/100, and it never wears off.
   - The Hardcore Stimpak heals 5/s for 6 s, and a normal Stimpak heals every limb.
   - A second dose doesn't re-run Start.
   - Only the highest rank of a ranked perk applies, and perks are the player's only.
   - Chemist doubles the Stealth Boy.
   - Skill books give 3 points (4 with Comprehension).
   - Worn bonuses (Vault Jumpsuit Speech +2) are not in actor values.
   - GetBaseActorValue includes ModActorValue.
   - Calm Heart's +50 Health goes to every teammate.
   - Four Eyes ignores "PER > 1".
   To check: read actor values in scripts around each use.
   ([aid-items](../spec-notes/aid-items.md), [levelup §9](../spec-notes/levelup.md),
   [skill-books](../spec-notes/skill-books.md))
10. **Karma, reputation and crime.** Ours:
    - Killing Very Evil gives +100, Good −100, and murdering a Neutral −25.
    - Each owned take or lock is −5.
    - A seen theft is +2 infamy and any seen kill +30, including hostile Powder Gangers.
    - An owner who sees a theft attacks at once.
    - Trespass: two warnings 5 s apart, then attack.
    - Crime fights can't be yielded.
    - Templated NPCs take their template's karma.
    - Unclamped AddReputationExact; 15/50/100 % levels for the eight reputations not
      captured.
    To check: read GetAV Karma (already in the header) and GetReputation around each act.
    ([crime-karma](../spec-notes/crime-karma.md))
11. **Vendors and loot.** Ours:
    - Vendors sell only their flagged categories and their vendor chest, but buy
      anything.
    - Restock follows Ringo's days for everyone.
    - Death items are rolled at death; the template's list wins.
    - Spent casings are player only, doubled by Hand Loader.
    - Respawn-flagged corpses return after 72 h at full health.
    - An encounter zone's level is fixed when its cell enters the loaded area (Easy to
      Boss is 50/75/125/200 %).
    - The three placed weapons that name their ammo give it once.
    To check: barter screenshots, inventory dumps, and timelines over 72 h.
    ([vendor-services](../spec-notes/vendor-services.md),
    [death-items](../spec-notes/death-items.md), [encounter-zones](../spec-notes/encounter-zones.md),
    [dead-on-spawn §4](../spec-notes/dead-on-spawn.md))
12. **Dialogue rules.** Ours:
    - Greetings flagged Goodbye are said at you; when an actor starts the conversation
      they hold you for one line.
    - A generic Goodbye is added where the only Goodbye has an end script (100+ lists).
    - Tab leaves only where a Goodbye is offered.
    - A skipped line still runs its end script.
    - Spent Say Once questions vanish.
    - No pause is added between voiced lines (scenes run 0.3–0.4 s short per line).
    - Shared (Info Connection) lines count as the topic's own.
    - Corner and quest messages wait until the conversation ends.
    - Voices without Allow Default Dialog get no generic lines.
    To check: screenshots and video of Ringo, Nash, Red Lucy and Doc Mitchell's stage 40.
    ([dialogue-play](../design/dialogue-play.md), [dialogue](../spec-notes/dialogue.md),
    [voice](../spec-notes/voice.md))
13. **Speeds and turning.** Settled (actors §8.10): people's walk and run with their
    armour and two-handed weapons, and creatures' walks and runs (clip pace × scale ×
    Speed Mult × their skeleton's share). Still open, ours:
    - People turn ≤180°/s walking, 120°/s standing; creatures 300°/s × scale (measured
      on cazadors only); the CREA Turning Speed field is unused.
    - Speed Mult on people: one actor (Cook-Cook, 110) ran as if it were 100 (asked in
      owner-review.md).
    - People walking with a weapon drawn (77–78 seen outside combat, about 100 in
      combat) are not built.
    - Follow distances: stand within distance + 64, run beyond 512.
    - Aiming or blocking slows you to a walk.
    - Swimming starts at 115 units deep.
    - The step-up is 24 units with a 45° slope limit.
    To check: the fight field set at every loop gives positions and headings; flat open
    ground and a cleared area: the `nvfairport` save (campaign `creature_runs`).
    ([actors §8.9–§8.10](../spec-notes/actors.md), [physics §6.4.2](../spec-notes/physics.md))
14. **Script rules that differ from the wiki.**
    - A call on an empty ref is skipped.
    - Return ends one block only.
    - Scripted items in other inventories don't run.
    - Message boxes don't freeze the player, and MenuMode 1002 is our guess.
    - The GetWeaponAnimType table (Sunny's tutorial depends on it).
    - tgm lifts carry weight and keeps magazines full.
    - Scripted UseWeapon runs before the combat AI; melee UseWeapon walks up to the
      target.
    - Quest items can be stored in containers.
    To check: short-script jobs for each.
    ([script-language](../spec-notes/script-language.md), [scripts §10](../spec-notes/scripts.md),
    [use-weapon](../design/use-weapon.md))
15. **Survival hazards.**
    - Environment radiation: strongest marker wins, and water gives 1 rad/s (2 if
      radioactive).
    - The Fat Man zone fades from 10 to 0 rads/s over 12 s within 616 units.
    - Turbo slows the world to 0.5.
    - A crippled head means a 40 s concussion.
    - A crippled arm makes an NPC drop its weapon.
    - Knockdowns last 3 s.
    - Only BMDT Power Armor needs training.
    - Traps give 2 XP (your note said 5).
    To check: actor values and events.
    ([radiation](../spec-notes/radiation.md), [magic-archetypes](../spec-notes/magic-archetypes.md),
    [unconscious §5.4](../spec-notes/unconscious.md), [traps](../spec-notes/traps.md))

## P3: smaller or rarer

16. **HUD and menus.**
    - The enemy health bar and red crosshair show only for hostiles, or for anyone hit in
      the last 5 s.
    - Sneak-indicator states.
    - Compass ticks within PER × 300; markers within PER × 2048.
    - "How many?" asks only above 5 (barter) or above 1 (crafting).
    - The crafting filter arrows.
    - Wait pace: 1 hour per 0.5 s, and it can be cancelled.
    - Wait refusals: within 2,048 units outdoors, 1,024 indoors.
    - Hacking word lengths by difficulty; a bracket resets attempts 1 in 5.
    - Holotapes keep playing with the Pip-Boy closed.
    - A second ShowTutorialMenu adds a page.
    - Aid effects are shown as totals.
    - The order of misc stats after #16, and what Barter Amount Traded counts.
    - Caravan currencies: caps first, then the least valued notes.
    - Cannibal and Sandman show a choice box.
    Screenshots. ([hud §8–§10](../spec-notes/hud.md), [wait-menu §9](../spec-notes/wait-menu.md),
    [hacking](../spec-notes/hacking.md), [misc-stats](../spec-notes/misc-stats.md))
17. **Weapons and bodies, by video.**
    - Minigun spin-up on every press and after a reload.
    - The Grenade Launcher reloads round by round.
    - Bodies go limp at once, with no death clip.
    - Shove speeds: shots 140, blows 220, bottles about 3 m/s.
    - Corpses: the player walks through them.
    - Small creatures block the player.
    - Bodies over 250 mass can't be pushed.
    - Loose items rest until touched.
    - Pushed NPCs move at half speed and return after 2 s (the `pc_actor_push` capture is
      ready).
    ([animation §9.8](../spec-notes/animation.md), [ragdoll §4](../spec-notes/ragdoll.md),
    [physics §4.3.1, §6.8, §6.10–§6.11](../spec-notes/physics.md))
18. **NPC odds and ends.**
    - Doors stay open behind NPCs.
    - No ledge links.
    - Locked doors are closed to NPCs unless they carry the key.
    - Idle conditions ignored for PlayIdle.
    - Sitting is instant for the player and holsters the weapon.
    - Followers arrive 160 units to your left (captured) and every teammate follows
      through doors.
    - Package-less actors walk home after a fight.
    - Creatures with Can't Open Doors never use load doors (the Searchlight ghouls
      following Edwards).
    - The companion-wheel details.
    - Challenges count every kill.
    - Quest targets indoors are marked at one door.
    ([navmesh §7](../design/navmesh.md), [idles](../spec-notes/idles.md),
    [furniture §5, §7](../spec-notes/furniture.md), [packages §7](../spec-notes/packages.md),
    [challenges](../spec-notes/challenges.md))

19. **The companion perks' highlights.** Ours: Spotter (Boone) and Search and Mark
    (Rex) glow only while the player aims with a weapon out; Spotter marks the actors
    the compass shows red within its range; Search and Mark marks chems, Stimpaks, guns,
    energy weapons and ammunition within 1,500 units; Enhanced Sensors (ED-E) sets the
    compass's actor range to Perception 10's. To check: with each companion hired, a
    frame before and while aiming near hostiles and near dropped items (a Stimpak, a
    pistol, a melee weapon, food) at a few distances; the compass with ED-E at low
    Perception. ([perk-entry-points §13](../spec-notes/perk-entry-points.md))

## Added 2026-10-25 (from the ESM-gap work)

The numbers above stay put so links to them hold; each item here gives its own priority.

20. **NPC grenades** (P1). Ours: the first throw comes 4–10 s after the NPC sees its
    target, then one every 12–25 s, only between 1.5× the blast radius and the throw's
    reach (at most 1,500 units), and each throw takes one from the inventory.
    To check:
    - When do NPCs throw grenades or dynamite: the distance to the target, how often,
      and how soon into the fight?
    - Do they visibly switch to the grenade and back?
    - Does their count drop?
    Setup: Powder Gangers with dynamite at NCRCF, or a Legion squad.
    ([combat-ai §20](../design/combat-ai.md))
21. **Packages that send NPCs through player-only load doors** (P2). Some door
    scripts' OnActivate blocks call Activate for the player only. `fnvtool ai reach`
    (routes_through_door) still finds FalloutNV.esm packages routed through them:
    - the Strip gates (VStripGate, refs 0016A14D–0016A150), about 420 actor/package
      pairs, among them the securitrons' VStripSecuritronGuardGate*/Patrol*, the NCR
      MPs' VStripMPBPatrolEmb and the Omerta thugs' VMS33ThugLeavePackage;
    - Gomorrah's elevator doors (0010DA76, 0010D6AC, 0010D6AE, 0010DDB4), 15 pairs, e.g.
      VMS21NeroCasinoTravel and VMS21BigSalHomeTravel.

    Ours lets an NPC through a load door whatever the block did (the block still runs)
    and refuses only at doors that open in place.
    To check: does the original move these NPCs through those doors? Does a Strip
    securitron's gate patrol cross the gate? Do Nero and Big Sal go between their rooms
    and the casino floor through the elevator, or never get there?
    ([doors §5.2](../spec-notes/doors.md))
22. **Combat answer lines and companion complaints** (P2). To check:
    - When a group-mate says NormalToAlert or NormalToCombat, does a nearby mate answer
      with NormalToAlertResponse or StartCombatResponse?
    - Are AttackResponse (BoS: "On it!") and FleeResponse (super mutants: "Coward!")
      said after Attack and Flee lines?
    - Does DeathResponse play without JIP? The wiki says it doesn't.
    - When does a companion say WeaponIneffective or ArmorIneffective? Ours says:
      - WeaponIneffective when DT cut the hit to its floor;
      - ArmorIneffective when one hit took 25 % of health with DT stopping 10 % or less;
      - each at most once a minute.

      Test: Boone with a 9mm against a high-DT target, and Boone in light armour taking
      a heavy hit.
    Read dialogue events in the observer timeline, with audio or subtitles.
    ([combat-dialogue §5](../spec-notes/combat-dialogue.md))
23. **Actor Effect spells in NPC and creature spell lists** (P2; decides whether we
    build it). About 110 actors carry SPEL records of type 0 "Actor Effect" (not
    Ability) in their NPC_/CREA SPLO lists, all duration 0 on Self. Ours applies only
    the Ability ones. Examples:
    - Ruthless on 21 Hoover Dam legionaries (+25 melee damage);
    - the Tunnelers' Increase Damage Threshold 2 and 5 (23 creatures);
    - SporeCarrierRegen (7);
    - NVDLC02QuietWatersPerkEffect (57).

    The GECK wiki's AddSpell page says the vanilla game uses only addictions and
    abilities. To check: does the original apply these type-0 entries? Read a Tunneler's
    DamageThreshold with GetActorValue against its base record's, and GetIsSpellTarget /
    IsSpellTarget for the SPEL on the actor. The earlier captures esmfx2 and esmfx3
    placed a spore carrier and a Ruthless legionary but did not ask.
    ([area-effects §3](../spec-notes/area-effects.md))
24. **A patrol after a load** (P3). Ours: an NPC on a non-repeating Patrol package
    goes on to its next point after a reload.
    To check: save while it is half-way along its route, reload, and see whether it
    goes on or starts again from the first point.
    ([save-load §5 item 4](../design/save-load.md))
25. **Dialogue at low Intelligence** (P3; few players run Intelligence 3 or below).
    Ours (PR #743), with the player's Intelligence at or below
    iDialogueDummySpeakThisIntOrBelow (3 in FalloutNV.esm):
    - a topic's Dumb Response (DIAL TDUM) replaces the player's line, e.g. "Pizza." for
      "Peace." in Helios One's ARCHIMEDES conversation;
    - INFO lines flagged Low Intelligence (acting note "{Dummy}") replace their High
      Intelligence twins ("{Non-Dummy}").

    To check: `player.setav intelligence 1`, then screenshots of those conversations.
    Check Intelligence 4 too: is the player dim there?
    ([dialogue §2](../spec-notes/dialogue.md))

## Added 2026-10-26 (UI tile tree)

26. **How the original's menu engine reads its files** (P2: every UI mod and MCM).
    Ours (fnv-ui):
    - `&true;` is 1 and `&false;` 0;
    - other constants are kept by name;
    - an end tag closes the innermost element of its name, else the innermost element;
    - a value a script set holds until what its expression reads changes;
    - SetUIFloat makes no trait that is not there;
    - `name:N` counts from 0 among the children of that name.

    To check: GetUIFloat on traits the files set to constants (`HUDMainMenu/locus`,
    `_CrosshairVisible`, a `justify`), DumpTileInfo on QuestAddedMenu and on MCM's
    StartMenu tree, SetUIFloat on a trait with an expression and on a missing one, and
    SetUIFloat against SetUIFloatAlt on a templated list row (the "children tiles" bug
    MCM depends on). ([menu-xml §8](../spec-notes/menu-xml.md))

## Added 2026-10-27 (first-person depth)

27. **Does the original's first-person weapon go into walls?** (P3: one look in every
    close fight indoors.) Ours draws the arms and weapon in front of the world whatever
    is nearer (first-person-depth.md §1, [hypothesis] that the original does the same).
    To check: in the Prospector Saloon, a varmint rifle drawn nose to the east wall
    (the `first-person-depth` smoke spot, feet at 469, 244, 3457 facing 80°), a screenshot
    hip-fired and one at the sights; then the same against a standing NPC. If the barrel
    is cut by the wall there, the original draws it with the world.
    ([first-person-depth](../design/first-person-depth.md))

## Added 2026-10-27 (main-quest walkthroughs)

28. **Does a SayTo that finds no line still end with SayToDone?** (P1: every ending.)
    Ours now runs the speaker's SayToDone blocks for the topic in the next frame
    [hypothesis]: the narrator's end-slide chain (SCPT 0x00161EB4) calls topics that have
    no line for some endings (VEndingBrotherhood for the NCR ending with the Brotherhood
    never met) and must still reach VEndingConclusion. To check: a query script that
    SayTos a topic whose lines all fail, and a SayToDone block that sets a variable; read
    it one and ten frames later. ([mq-walkthrough](mq-walkthrough-2026-10-27.md) §2)
29. **Does a line's fixed speaker (INFO ANAM) gate who says it?** (P2.) 262 lines name a
    fixed speaker other than the actor of their own GetIsID condition (167 end slides:
    a Boomer, Nolan McNamara ... as speaker, GetIsID Narrator). Ours now lets that
    condition decide when the line has one [hypothesis]. To check: the end slides of a
    finished game say the Boomers' slide; for a non-ending case, talk to Victor at the
    Lucky 38 door (0x00165102, speaker the entrance Victor, GetIsID the elevator).
    ([mq-walkthrough](mq-walkthrough-2026-10-27.md) §2)
30. **Reputation axes outside 0-2.** Scripts call SetReputation RepNVNCR with axis 2
    (Crocker's messenger, Vulpes's greeting) and conditions use GetReputationThreshold
    axis 3 (VDialogueBenny) and GetReputation axis 2 (VMS36); the wiki documents 0/1
    (fame/infamy) and 0-2 (mixed/good/bad). Ours does nothing and reads 0 (reputation.rs
    `axis`), so Benny's greeting for a player liked on the Strip (axis 3 >= 3) is never
    chosen. To check: GetReputation and GetReputationThreshold
    with those axes after SetReputation X 2 5 on a fresh save.
31. **Say Once on a line two actors share** (dialogue.md question 13): the Yes Man
    mainframe repeats the Tops Yes Man's first-meeting greeting (0x001164F1, Say Once,
    GetIsID YesManMainframe OR VMQTopsYesMan) once, because ours keeps Say Once per actor.
    To check: meet Yes Man at the Tops, then talk to the mainframe in the penthouse.
32. **An expression that leaves extra values** (`GetStage Q N == 1`, compiled with two
    arguments for GetStage): three begin scripts (e.g. INFO 0x00160731) stop with "2 values
    left on the stack" in ours. To check: what Side Bets' report line does in the original
    (it sets nothing on the canonical path).

## Added 2026-10-29 (radiation falloff, issue #894)

33. **Does a radiation marker fall off linearly with distance, and does Rad Resistance cut
    GetRadiationLevel?** (P2: every irradiated place.) Ours now gives a marker's XRAD ×
    (1 − d ÷ XRDS), d the 3-D distance from GetPos, and GetRadiationLevel and the HUD
    that times (1 − Rad Resistance ÷ 100). One distance was read (the shack's coc spot,
    693.65 units from marker 0x00154611, 15/2000, Rad Resistance 8: 9.013784, ours
    9.013783); the `hud_corners` jobs' moves to two other spots did not move the player.
    **Partly answered 2026-10-29** (private job `hud_radiation`, MoveTo to the coc XMarker
    plus offsets): 554.03 units, Rad Resistance 8: 9.977171 (ours 9.977171); the coc spot
    after Rad-X, Rad Resistance 39: 5.976524 (ours 5.976524). Three other spots read one
    value (9.570496) at three distances and one 8.710030; unexplained (radiation §2).
    Still open: a read near the marker, the overlapping markers, the Fat Man's zone.
    To check, in SLNuclearTestShackINT with GetPos after each move to prove it: at the
    marker's x, y (4358.3, −718.9; ours 13.475 with RadResist 8), halfway
    (4338.6, −373.4; ours 11.390), and at the coc spot after Rad-X (RadResist 58; ours
    4.115). Then, for overlapping markers (strongest or sum), on the Camp Searchlight
    crater's three markers (5/10000, 10/6000, 18/2000 on one point) 1000 units from their
    point (ours 9 × 0.92 = 8.28; summed 21.83 × 0.92 = 20.09); and a Fat Man's zone read
    at its centre and 300 units out (ours flat). ([radiation §2, §4](../spec-notes/radiation.md))
