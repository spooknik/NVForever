# Scripted weapon use: UseWeapon and the Use Weapon package

What the documentation and the masters say about making an actor fire a weapon by script
or package. The design built on it is docs/design/use-weapon.md.

## 1. The UseWeapon function

[documented: GECK wiki UseWeapon (Function), the owner's local export]

- Called on an actor reference; opcode 0x119C [observed: the function catalogue].
- Nine parameters, the last five optional: the weapon (a base form), where to stand (a
  reference), the target (a reference), how many times to use the weapon, whether to
  crouch when reloading (default on), whether to hold fire when blocked (default on),
  whether to "automagically hit the target" ("for scripted forced kills", default off),
  whether to "prevent all damage" ("for scripted protracted fake combat", default off),
  and where the target needs to stand ("default/NULL means anywhere").
- The catalogue types the last parameter as an int; the page's example passes a marker
  reference there: `BuddyRef.UseWeapon Pistol12 BuddyRef HollyRef 5 0 0 0 0 XMarkerRef047`,
  explained as Buddy drawing his pistol ("or grab one if one was nearby") and firing at
  Holly five times from his own place, "but only while she is near XMarkerRef047".
- The page links the Use Weapon package and FireWeapon.

## 2. Calls in the shipped scripts

[observed: `fnvtool script decompile`, FalloutNV.esm; the catalogue counts 13 calls over
the masters]

| Unit | Call | Context |
|---|---|---|
| SCPT 0x0011D217 | `useweapon <weapon> <stand ref> <target> 5 1 0 1` | a GameMode block that also calls `startcombat <target>` just before: a scripted kill (always hit on) |
| SCPT 0x0011C54C | `useweapon <weapon> <stand ref> <target> 5 1 0 1` | the same pattern with the two actors swapped |
| SCPT 0x000FED4F … 0x000FED52 (4 trigger scripts) | `<actor>.useweapon <weapon> <stand ref> <target> 4 1 1 0 0` | OnTriggerEnter of the player: four shots at one of four targets, then `<actor>.startcombat player` |
| SCPT 0x000E07CE | `<actor>.useweapon <weapon> <actor> rtarget 1 0 0 1 0` | a timed sequence: the actor stands on its own reference, one shot (always hit) at the next living target; three seconds later the script calls `<actor>.resetai` |
| INFO 0x0011C543 (end) | `<actor>.useweapon <weapon> <actor> <target> 3 0 1` | a dialogue result after `startcombat` calls; the actor stands where it is |

So: where to stand is often the actor itself; counts are 1 to 5; UseWeapon is called right
next to StartCombat, and ResetAI is used to end it.

## 3. The Use Weapon package

- PACK PKW3 "Use Weapon Data" [documented: xEdit PACK PKW3]: flags Always Hit (bit 0), Do
  No Damage (bit 8), Crouch To Reload (bit 16), Hold Fire When Blocked (bit 24); fire
  rate (Auto Fire, Volley Fire); fire count (Number of Bursts, Repeat Fire); number of
  bursts; shots per volley min and max; pause between volleys min and max (seconds).
  The names are all there is: what each does in play is not documented.
- Package type 16 in PKDT; GetCurrentAIPackage code 33 (docs/spec-notes/packages.md §1).
- Package idles never play for Use Weapon [documented: GECK wiki Category:Packages;
  docs/spec-notes/actor-behaviour.md].
- In the masters: 173 Use Weapon packages; 9 have a location 2; target 2 is always a
  specific reference; target 1 is an object type in 52 ("Weapons: None" 32, "None" 10,
  melee 7, …) [observed: `fnvtool ai survey`, packages.md §2]. The type's own page (Use
  Weapon Package) is not in the owner's export; the offline GECK wiki copy has it.
- "Continue During Combat: Actor will continue to fire and stand at the designated
  position, even during Combat. Great for setting up immobile enemies." [documented: GECK
  wiki page "Use Weapon Package", section Flags, revision 35491 of 2024-08-04,
  https://geckwiki.com/index.php?oldid=35491]. The page does not say at whom it fires
  once in combat; the general flag page says a Travel package with the flag fires "at
  their target" on the way [documented: GECK wiki page "Combat Package Flags", revision
  34683 of 2024-02-17, https://geckwiki.com/index.php?oldid=34683], and its
  per-type chart has no Use Weapon row. We keep the package's target 2 [design;
  docs/design/use-weapon.md §3, #2120]. 43 of FalloutNV.esm's 82 Use Weapon packages set
  the flag (sniper posts, scripted kills, the Ultra-Luxe kitchen's flamers, the Hoover Dam
  Securitrons); 38 of them carry conditions [observed: `fnvtool ai packages` and `ai
  show`, 2026-11-11].

## 4. SetRestrained

Restrained actors still try Use Weapon packages and UseWeapon and their animations but
never attack [documented: catalogue notes on SetRestrained].

## 5. Open questions

- What "blocked" means for Hold Fire When Blocked (someone in the way, a wall, both).
- Whether the shots of the function form one volley or follow the weapon's own rate.
- What Auto Fire against Volley Fire changes, and whether a burst is a volley.
- How far from the "target should be here" reference the target may be.
- Whether a use from a script holds against the actor's combat state, or the combat AI
  takes over (the shipped scripts call both together).
- Whether the actor "grabs" a weapon it does not carry, and from where.
- How a melee weapon is used: whether the actor walks up to its target from where it is
  to stand, whether a blow counts as one of `numberOfShots`, and what Always Hit and Do No
  Damage do to a blow (ours: docs/design/use-weapon.md §3.1, all [hypothesis]). Which
  shipped calls name a melee weapon was not surveyed.
