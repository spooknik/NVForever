# Pickpocketing (ESM full audit gap 6, 2026-10-24)

Sources: the GECK wiki export (owner's, `C:\fnv-private\geckwiki`): pages Pickpocket
(GameMechanics.xml), SetGhost, Category:NPC, Combat Tab (Pickpocket topics), Projectile
(notes on planted explosives), GetPlantedExplosive, ActorHasBaseFlag,
SetPlayerCanPickpocketEquippedItems and SetCaughtPCPickpocketting (functions.xml).
FalloutNV.esm's settings [observed: raw read, 2026-10-24]. Code: `fnv_game::pickpocket`
(rules), `nvf_play::pickpocket` (play), the container menu's steal mode
(`nvf_play::menu::model::Steal`).

## 1. Who can be pickpocketed

- Every NPC: NPCs "can all be pickpocketed" [documented: Category:NPC].
- A creature only with ACBS bit 28 Allow Pickpocket [documented: ActorHasBaseFlag]; 124
  in FalloutNV.esm (wild dogs, giant rats, big horners, ED-E's upgrade forms), 2 in Dead
  Money, 5 in Old World Blues, 4 in Honest Hearts, 1 in Lonesome Road [observed]. Read from
  the definition supplying the base data (template flag Base Data) [hypothesis].
- Not a ghost: "Cannot be pickpocketed, despite the prompt still showing up. Instead,
  regular activation (dialog) occurs" [documented: SetGhost notes]. NVForever shows
  Talk on a ghost instead of Steal [design: the original's prompt is a known quirk].
- Not dead (searched instead), not hostile to the player or fighting it (only its name
  shows, as for talking), not unconscious, not a teammate [hypothesis for the last two:
  a companion's inventory is open to the player through the wheel].
- Not one that caught the player before: it "has already caught you." (GMST
  `sNoPickPocketAgain` [observed]; SetCaughtPCPickpocketting "originally is set when
  player fails a pickpocket attempt … player can't pickpocket NPC anymore" [documented]).
  E on it shows that line after its name and opens nothing.
- The player must be sneaking (IsSneaking) [hypothesis: the Combat Tab's
  PLAYERPICKPOCKET topic is "used when the player is sneaking near an actor"; the
  original's E while sneaking opens the pocket].

The prompt reads **Steal** [hypothesis: the original's word is not in the masters' GMSTs].

## 2. The chance

```
chance = (thief Sneak × fPickPocketActorSkillMult + fPickPocketActorSkillBase)
       + (target Sneak × fPickPocketTargetSkillMult + fPickPocketTargetSkillBase)
       + (item value × fPickPocketAmountMult + fPickPocketAmountBase)
```

rounded down, kept within `fPickPocketMinChance` and `fPickPocketMaxChance` [documented:
Pickpocket]. Thief Skill is the player's Sneak, Target Skill the target's Sneak
[documented]. A creature's Sneak is its CREA Stealth skill (as GetActorValue Sneak reads
it, `actor_values`) [hypothesis].

| Setting | Wiki default | FalloutNV.esm |
|---|---|---|
| fPickPocketActorSkillMult | 0.6 | 0.6 |
| fPickPocketActorSkillBase | 40 | 40 |
| fPickPocketTargetSkillMult | −0.6 | −0.6 |
| fPickPocketTargetSkillBase | 0 | not set |
| fPickPocketAmountMult | −0.5 | −0.5 |
| fPickPocketAmountBase | 0 | not set |
| fPickPocketMinChance | 5 | not set |
| fPickPocketMaxChance | 85 | 85 |

- **Item value** is the moved units' value: one unit's value × the count moved
  [hypothesis: the wiki says "Item Value"; taking a stack of 50 caps at once is then much
  harder than taking one]. A key's value "is used to weight the chance of getting caught
  when pickpocketing the key" [documented: Key], which agrees.
- **Each take and each placement** rolls once against its chance [hypothesis: placing is
  judged as taking is]. The menu's line shows the chance of the selected row for one
  unit [design: the original's display is not captured].

## 3. The menu

The container menu on the victim's inventory, its title the victim's name. Equipped items
are not listed ("By default, the player cannot pickpocket equipped items" [documented:
SetPlayerCanPickpocketEquippedItems]; hypothesis: a stack with one unit equipped is hidden
whole). Take All is refused [design: each take is its own roll]. The game clock stops as
in any container menu.

## 4. Success and failure

- **Unseen take**: theft from the victim (its base is the owner): karma
  `fKarmaModStealing` (−5) unless the victim is Evil, as for any theft (crime-karma.md
  §3.1, an unseen theft costs karma too); Pockets Picked +1 per take [hypothesis: per take].
  No witness turns [hypothesis: others watching are not judged].
- **Caught** (the roll fails): nothing moves and the menu closes. The victim says
  **Pickpocket** ("Said by an actor if they notice you picking their pocket" [documented:
  Combat Tab]) and turns on the player as a seen thief does (a crime enemy; the crime
  counts against it as `Kind::Pickpocket`, a minor crime) [hypothesis: as Steal]. One that
  ignores crime says **PickpocketNoCrime** ("doesn't regard it as a crime" [documented])
  and does not turn. It will not be pickpocketed again (§1). No karma for a failed try
  [hypothesis].

## 5. Planted explosives ("exploding pants")

"Grenade and mine-type weapons that use an exploding projectile with the Alt. Trigger flag
can be reverse-pickpocketed onto NPCs. If successful the weapon goes live and explodes in
the NPC's inventory after fProjectileInventoryGrenadeTimer has elapsed, always killing it
regardless of its HP … GetPlantedExplosive will return true on the NPC after
fProjectileInventoryGrenadeFreakoutTime has elapsed" [documented: Projectile notes].
FalloutNV.esm: timer 2.0 s, freak-out 0.1 s [observed].

- A placed weapon whose projectile has Explosion and Alt. Trigger goes live, one per unit.
  The timers run on the game clock [hypothesis], so they stand still while the menu is
  open and start when it closes.
- **Not on a child or an essential actor.** Fallout 3 "just puts the explosive in a
  child's inventory", with no explosion [documented: Fallout Wiki, Fallout 3 Pickpocket
  page, owner's paste of 2026-11-05]. New Vegas is not described. NVForever does the same
  for a child (its race has the Child flag) and for an essential actor: the item stays an
  ordinary item and nothing goes off [hypothesis: observer check 98].
- **The panic.** From the freak-out time to the blast the victim panics. The masters'
  only GetPlantedExplosive condition is on the IDLE root `PanicExplosivePlanted`
  (0x00042F24, folder `Characters\_Male\IdleAnims`, GetPlantedExplosive == 1). Its two
  leaves are `PanicGrenadePlantPA` (0x00042F25, `PanicGrenadePlantPA.kf`, one condition:
  GetEquipped on form list 0x0008945E, the power armour) and `PanicGrenadePlant`
  (0x00040203, `PanicGrenadePlant.kf`, no conditions) [observed: FalloutNV.esm, raw read
  2026-11-06]. The add-on masters have none. The engine picks it the way the tree picks a
  fidget: the roots of the actor's own folder whose conditions name
  GetPlantedExplosive == 1, then the first leaf with a file whose conditions pass
  (`Game::panic_idle`). It plays once, as a special idle (GetAnimAction 13). No creature
  folder has such a root, so a creature plays nothing. People and creatures alike stand
  where they are until the blast; the issue says "slows or stops", and NVForever stops
  them [hypothesis]. The panic idle is not named in the code: a load order with another
  root naming GetPlantedExplosive gets its own.
- **When the timer is up**: the victim loses the explosive, Pants Exploded goes up by 1,
  and the projectile's explosion goes off where it stands. Its blast is the player's: it
  hurts others as a thrown grenade's does [hypothesis].
- **Only an explosion that hurts the victim's kind kills it** ("instantly killing them if
  it's a damaging type", owner 2026-11-05). The rule (`Game::explosive_harms`)
  [hypothesis: observer check 98]:
  - an actor that takes no damage (a child, an Invulnerable creature, a ghost) is not
    killed;
  - when the explosion's object effect (EXPL EITM) has effects that lower Health (base
    effect Hostile or Detrimental, Health as its actor value, magnitude above 0), they
    decide, by their conditions on the victim. The Pulse Grenade's `GrenadePulseExplosion`
    (0x000298A9, damage 10) carries ENCH `EMP` (0x0003A36B): Damage Health 200 when
    GetIsCreatureType 6 (robots), and 80 with a Pulse Disable effect when GetEquipped
    names one of six power armour lists [observed: FalloutNV.esm]. So a pulse grenade
    kills a robot or a power armour wearer and not an ordinary man, whose blast's 10
    points of EMP damage do not count;
  - otherwise the explosion's damage above 0, of a damage type (WEAP DNAM Resist) the
    victim does not wholly resist (its value of that resistance below 100). A frag grenade
    (`GrenadeFragExplosion`, 125, Resist None) and a plasma grenade (225, Energy
    Resistance) kill anyone [observed: `fnvtool combat weapon`].
  A victim that lives stays alive; its explosion goes off as for any other.
- **Crime and reputation.** "If the player stays hidden from the plant until the blast,
  nobody turns hostile and no infamy is gained" [documented: Fallout Wiki, New Vegas
  Pickpocket page, owner's paste of 2026-11-05]. The kill is the player's, judged as any
  kill (crime-karma.md: karma, murder, infamy, witnesses turning). Its witnesses are those
  who detect the player when it goes off, and also anyone who did while it was live: the
  witnesses are taken every frame from the plant to the blast
  (`Planted::seen_by`) [hypothesis: the sampling]. A player hidden all that time is seen
  by nobody, so the murder costs karma only.
- **Always murder.** Some important faction NPCs (Caesar, Lucius, Ambassador Crocker)
  always count as murder and give their faction infamy [documented: same page]. Nothing on
  the actors or factions marks them [observed: FalloutNV.esm: Caesar `FortCaesar`
  0x00121FEF has ACBS flags 0x04000010, Lucius `FortLucius` 0x0011FC9D 0x00000090,
  Crocker `VStreetDennisCrocker` 0x00112640 0x00000010; no shared flag, condition or
  faction]. Their own scripts do it, on death. Caesar's OnDeath (SCPT 0x00137622) sets
  stage 110 of quest 0x00129D14, Lucius's (0x001543B8) the same. That stage's result
  script calls `SetEnemy 0x000EE68A 0x0001B2A4`: the Legion and the player's faction
  become enemies. Crocker's OnDeath (0x001622CE) sets stage 200 of quest 0x0011F95B,
  whose result is `AddReputation 0x000F43DE 0 3`: NCR infamy [observed: `fnvtool script
  decompile`]. A planted kill is a kill (`Game::kill`), so these OnDeath blocks run
  whoever saw. NVForever has no rule or list of its own for them.
- **Kill credit.** "The kill counts as a kill with the weapon type the player has
  equipped, toward that weapon type's stats and challenges" [documented: same page]. The
  victim's last hit is set to the player with the weapon the player holds (fists when
  none) before it dies, so the challenges' weapon filters (challenges.md) count that
  weapon, not the grenade and not an earlier hit's weapon.
- GetPlantedExplosive is 1 from the freak-out time until it goes off.
- **The live item** (#1640 item 5, first part). While it is live the item is "marked
  'Live' in the pocket and the victim can't use it or drop it" [hypothesis for New Vegas,
  from Fallout 3's behaviour: Fallout Wiki, Fallout 3 Pickpocket page]. NVForever's rule
  [design, #1640]:
  - Each plant that goes live reserves one unit of the item in the victim's inventory at
    once (before the freak-out time): a unit apart from the victim's own units of the same
    item, kept as the extra data `live` (`fnv_sim::inventory::Extra::live`). Ordinary
    units of the item stay the victim's to use; the item type is not locked. The menu
    arms it as the command that placed it is applied (`pickpocket::arm_given`, called by
    `menu::apply_input`), and lists the open pocket again before the next command, so the
    pocket shows the reserved row at once. A take on a row whose units the simulation
    would not move (`Inventory::count_at` 0) is refused before the roll.
  - An inventory rebuilt in place (ResetInventory, a cell respawn, a resurrection's new
    pick, a merchant restock, `World::give_items_of`) keeps its live units
    (`Inventory::keep_live_of`), so the plant still finds its unit.
  - No action takes a live unit: no equip or use (`World::equip_at`), removal
    (`RemoveItem`, `RemoveAllItems`, which leaves it as it leaves non-playable items),
    drop, transfer (the container menu, `RemoveAllItems` with a target) or trade; an
    actor choosing a weapon to hold or a grenade to throw counts only its ordinary units
    (`Inventory::free_count` / `free_iter`; `rearm`, `npc_combat`'s grenades). The player
    cannot take it back out of the pocket either, and that costs no roll
    (owner-review.md). GetItemCount still counts it: it is in the pocket.
  - The pocket lists it as a row of its own, the item's name followed by "(Armed)".
    FalloutNV.esm has no game setting for this word [observed: none of its GMSTs names a
    live or armed item; read with `fnvtool esm inventory`], and the word "Live" is seen
    only on the original's screen, so the word is NVForever's own (LEGAL.md;
    owner-review.md).
  - The blast takes exactly that unit (`Inventory::take_live`), never an ordinary one.
    When the victim died before the blast, the plant is let off and its unit is an
    ordinary item again (lootable). On a child or an essential actor nothing goes live
    and the item stays ordinary. A plant whose item is not in the pocket (a script took
    it before it could be reserved) still goes off, reserving nothing.
  - Not built (#1640 item 5, second part, still open): the victim looking around during
    the panic and spotting a player standing nearby.
- Not built: the "Detonates" variant (DetonatePlacedExplosives).

[design] Active plants cross saves as the default-empty, omitted-when-empty `planted`
field of `GameSnapshot` (`crates/fnv-game/src/game.rs`, `snapshot`/`restore`). Each plant
keeps its victim, item, absolute placement time `at`, blast description and accumulated
`seen_by` witnesses (`crates/fnv-game/src/pickpocket.rs`, `Explosive`/`Planted`). The fuse
uses saved `Game::time`, seconds advanced only outside menus, loading and FreezeTime.
Loading neither restarts the fuse nor ages it by wall time or calendar hours. Cached
pickpocket rules are read afresh; `blast_witnesses`, used only around synchronous kill
processing, starts empty. A witness seen earlier in the fuse still counts after loading.

[design] `GameSnapshot::drop_gone` removes plants with a missing victim, item or required
explosion object effect after `fnv_save::remap_json` remaps the serialized FormIDs. It also
removes plants on references dropped because their saved base or cell disappeared, and
removes missing or dropped witnesses. Older snapshots without `planted` load with no live
plants; inventory alone cannot reconstruct their historical fuse times. This additive
JSON field changes none of the container version 1, `nvf.save.body/1`,
`nvf.engine.state/1` or `fnv.game.state/1` identifiers (`engine/nvf-play/src/save.rs`,
`crates/fnv-save/src/lib.rs`, `crates/fnv-game/src/game.rs`).

[design, #1640] The live units are saved with the inventory (`"live": true` on the unit's
group, omitted when false, so older saves read unchanged). After a load,
`Game::reconcile_live_explosives` makes each holder's live units of an item equal to its
saved plants of that item, as far as it carries the item: a save from before live units
gets its plants' units reserved, and a live unit whose plant was dropped (its plugin gone,
`drop_gone`) is an ordinary unit again. A save from before live units whose victim had
put the planted grenade on (its only unit equipped) gets that unit taken off and reserved
(`Inventory::mark_live` takes an equipped unit last). No other migration is needed; the
field is additive like `planted`, and changes none of the identifiers above.

## 6. Checks

Unit tests: `fnv_game::pickpocket::tests::chance_follows_the_wiki`; in
`crates/fnv-game/tests/factions.rs`: `a_planted_explosive_goes_off_after_the_timer`,
`only_an_explosion_that_hurts_the_victims_kind_kills`,
`a_planted_explosive_does_not_go_live_on_an_essential_actor`; in
`crates/fnv-game/tests/functions.rs`: `a_planted_explosive_stays_an_item_on_a_child`;
`a_planted_kill_is_seen_by_whoever_saw_the_player_while_it_was_live`,
`a_planted_kill_counts_for_the_weapon_the_player_holds`,
`each_plant_reserves_one_unit_that_its_blast_takes`,
`a_plant_on_an_essential_actor_stays_an_ordinary_item`,
`live_units_follow_the_saved_plants`,
`an_old_save_reserves_a_planted_grenade_its_victim_had_put_on`,
`reset_inventory_keeps_a_planted_explosive`; in `fnv_sim::inventory::tests`:
`a_live_unit_is_reserved_from_every_pick`, `the_live_count_is_set_from_the_plants`,
`an_equipped_unit_is_reserved_last_and_taken_off`; in
`crates/fnv-sim/tests/synthetic.rs`: `a_live_unit_is_not_equipped_removed_dropped_or_moved`,
`a_reset_inventory_keeps_live_units`; in `engine/nvf-play/src/menu/model/tests.rs`:
`a_live_explosive_is_a_row_of_its_own_that_stays`,
`an_explosive_armed_in_the_open_pocket_is_listed_and_refused_at_once`,
`a_pocket_row_listed_before_its_arming_is_not_rolled_for`; in
`crates/fnv-game/tests/idles.rs`: `the_panic_idle_is_the_root_naming_get_planted_explosive`. Smoke:
`pickpocket-live-item` (the planted Dynamite's unit is reserved; Chet's scripted
EquipItem, Drop and RemoveItem leave it; its blast takes it);
`pickpocket-steal` (sneaking, E on Goodsprings' Sunny Smiles opens her pocket; one take
is logged as unseen or caught); `pickpocket-plant-dynamite` (Dynamite placed on Chet while sneaking goes live, he plays `PanicGrenadePlant`, it goes off and kills him, and no faction turns: a Frag Grenade is worth 150 caps, which leaves a 16% chance, so the seeded roll would catch it).

[design] Save regressions in `crates/fnv-game/tests/factions.rs` cross JSON and
`World::restore` before/after freakout, retain remaining fuse time and witnesses, consume
each item once, count deaths/stats once, cover harmless and multiple plants, and load an
older save without the field. `crates/fnv-game/tests/synthetic.rs` covers reordered and
missing plugins, including victims removed with their saved base/cell and transient
witness reset. Smoke `pickpocket-save-write` saves a live fuse while the console pauses
it; `pickpocket-save-load` uses an authored load hook to query the plant before time
advances, then requires one blast/death and the consumed item. Run the pair in that order
with their isolated NVForever home. These scenarios require runtime validation.
