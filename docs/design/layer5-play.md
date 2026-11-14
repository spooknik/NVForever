# Layer 5 in the game: scripts in `nvforever --play` (workstream 5D)

Status 2026-10-09: first cut on `claude/layer5-play`; round 2 on `claude/layer5-play-2`
(menus and message boxes through the runtime's API, `--new-game` and the opening in Doc
Mitchell's house, triggers for any actor with its capsule, subtitles). Quest and object
scripts run in `--play` through `fnv-game` (docs/spec-notes/scripts.md §4–9, workstream
5B), with the events the engine can produce today and their effects applied to the
world. Row 5D of docs/design/layer5.md §7. The engine side of Layer 4 is
docs/design/layer4-play.md.

## 1. Shape

All of it lives in `engine/nvf-play/src/script*.rs`; the rest of `nvf-play` changed only
where the runtime hooks in (`lib.rs`: modules, `PlaySet::Scripts`, the plugin;
`activate.rs`: E goes through the scripts; `menu.rs`: the container and inventory menus
open and close the runtime's menu), plus `--new-game` in `engine/nvforever/src/main.rs`.

| File | Role |
|---|---|
| `script.rs` | `ScriptState` (the `fnv_game::Game` and the engine's bookkeeping), loading, the frame system, menus and answers, holding the player, activation by hand, the new game, the trace log, test aids |
| `script_events.rs` | pure parts, unit-tested: trace events → engine effects, inventory comparison, trigger geometry (capsules), the test-aid parser |
| `script_trigger.rs` | trigger volumes → OnTriggerEnter / OnTrigger / OnTriggerLeave, for every actor |
| `script_hud.rs` | lines, the runtime's message boxes, placeholders for character-creation menus, subtitles; the keys that answer them |
| `script_draw.rs` | references enabled at run time that render streaming never drew |

- **One world.** `SimState::world` stays the only Layer 4 world, read by every other
  part. The `Game` holds a placeholder world; each script run swaps the real one in and
  back (`script::with_world`). Nothing else had to change to keep reading `sim.world`.
- **Loading.** `GameData` (every unit decoded, quests, messages, form lists) is built on
  a thread of its own at the start (about 1 s; render streaming and physics start
  meanwhile). The function catalogue is the repository's
  `schemas/script/functions.json` (or `NVF_PLAY_SCRIPT_FUNCTIONS`). If it cannot be read,
  the game plays on without scripts and says why.
- **New game.** When the data is ready, `Game::new_game` starts every quest flagged
  "Start game enabled" (scripts.md §5 [documented: GECK wiki Quest Data Tab]). Without
  `--new-game`, VCG00, which the New Game menu starts, is not started (test aid
  `start:VCG00`).
- **`--play --new-game`** (round 2): the player starts at `VCG01PlayerStartMarkerREF`
  in GSDocMitchellHouse (`script::new_game_start`, read from the plugins; the heading
  from the marker's rotation; `--cell`, `--pos` and `--heading` are ignored). On the
  first frame scripts run, VCG00 is started and its stage 0 set
  (`Runtime::start_opening`): VCG00's MenuMode block sets stage 0 while its stage is
  below 5 [observed in the masters, SCPT 0x00102035], which in the game happens in the
  main menu as the new game starts [hypothesis]. Stage 0 moves the player to the marker
  (already there: no travel) and sets stage 90 with its 3 s countdown; from there the
  game's own timers, Doc Mitchell's lines and the menus drive the opening (§6.2).
- **Start.** Scripts tick once the player stands in the world (`PhysState::player`),
  from the player's cell then (§2).

## 2. A frame (`PlaySet::Scripts`, between `Activation` and `Ui`)

1. **Keys for a script's open menu** (`script_hud::menu_keys`): they answer it
   (`ScriptState::answer`, below).
2. **Test aids due** (§5); an unattended run's open script menu is answered after
   `NVF_PLAY_TEST_SCRIPT_MENUS` seconds.
3. With the world swapped in:
   - **Inventory changes made outside scripts** (the container and inventory menus)
     since the last script run: items added → OnAdd, newly equipped → OnEquip,
     unequipped → OnUnequip, removed while a new created reference of that item
     appeared → OnDrop; the item's script runs in its per-holder instance, subject the
     player [hypothesis: per change, not per unit; storing into a container runs
     nothing here].
   - **Items used up** in the inventory menu (an aid item; the menu sends `ItemUsed`
     after `World::equip` removed it): `Game::apply_magic_item` starts its effects,
     including ScriptEffectStart/Update/Finish of effect scripts (scripts.md §7.6).
   - **The player's cell** (`RefState::cell`, kept by `player.rs`): when it changes,
     `Game::player_entered` loads it (the 5×5 grid outdoors [hypothesis, scripts.md
     §7.2]) and the scripted references now in newly loaded cells get OnLoad, chosen as
     `Game::active_refs` chooses those whose GameMode blocks run: a reference moved or
     created there runs it with that cell, and one moved away does not run it with the cell
     it was placed in [design: `Game::scripted_refs_in`; hypothesis: that the original
     goes by the current cell, no moved reference was seen there].
   - **OnLoad after Enable** is fnv-game's own since #63 (`Game::note_enabled` queues it
     for the start of the next tick) [observed: layer5-captures.md §8.3 d, "by the next
     frame, not inside Enable"]; nvf-play no longer tracks it.
   - **The opening** on the first frame of a new game (§1).
   - **Test-aid steps** (§5).
   - **Triggers** (§3).
   - **`Game::tick`** with the frame's real time (`Time::delta_secs_f64`): running quest
     scripts whose delay has passed, then GameMode/MenuMode of the scripted references
     in loaded cells, the player's items, effects and dialogue lines (fnv-game's order,
     scripts.md §7.2). In a menu MenuMode runs and game time stops (the runtime's rule);
     `game.loading` is the engine's, set for the two frames after the player's place
     changed (a door's destination loads in one of them; no script runs).
     `frame_seconds` is always the real frame time [observed for a container menu, a
     message box and a loading screen: layer5-captures.md §8.3 e, §10.1].
   - **The answered menu closes** after the tick (below).
4. **Effects** of the frame's trace events (§4), the trace log (§5), log lines.
5. **`walk:` test aids** (they need physics) and **holding the player** (below).

**Menus are the runtime's** (round 2; scripts.md §7.2, §7.7). The engine no longer sets
`Game::menu` itself:
- Scripts open message boxes (1001; queued in `Game::boxes`, one on screen at a time,
  the next when it closes) and the character-creation menus: name 1051, race 1036,
  S.P.E.C.I.A.L. 1074 (the Vigor Tester), tag skills 1048, traits 1084.
- The container and inventory menus open 1008 and 1002 [hypothesis for 1002] with
  `Game::open_menu` and close them with `Game::close_menu` (`menu.rs` →
  `script::engine_menu_opened` / `engine_menu_closed`). A box a MenuMode script shows
  meanwhile waits and opens when the menu closes (the runtime's rule).
- **Answers** (`ScriptState::answer`): a number key or a click picks a button of the
  front box (the buttons are clickable, #97); 1, Enter, Space or a click on OK close a box
  without buttons; Enter or a click on Done closes a character-creation menu. The button goes into `Game::message_box` *before* the frame's tick and
  `Game::close_menu` runs *after* it, so a `Begin MenuMode 1001` block still reads
  `GetButtonPressed` once (the Goodsprings chemistry set's does; `Game::press_button`
  closes the box at once, before MenuMode could run) [hypothesis: the original runs the
  frame's MenuMode with the button pressed]. A box that waits for a button and is
  closed with OK gets button 0.
- The **placeholder** for a character-creation menu names it and says nothing is chosen
  (Layer 7 builds the real menus); the player's name, race and picks stay as they are.
- **Holding the player** (`script::hold_player`): while a script's menu or box is open
  the game is in `PlayState::Menu`: no moving, looking or activating, the cursor is
  free, the game clock stops (`sim.rs`) [observed: the pack boxes held the world clock,
  layer5-captures.md §10.1]. Round 1 left the player free under a box. Whether the
  picture stands still too is `--menu-pause` (default `living`: it does not;
  docs/design/dialogue.md §8); a SayTo line held by the box has its sound paused with it
  (`voice.rs`).
- **The pause menu** (Esc, `crate::pause`, #97) is a `PlayState::Menu` of its own:
  `hold_player` leaves it alone, and scripts do not tick while it is open (no MenuMode
  code of the original's pause menu is known, so script time simply stops) [decision].
  It cannot open while a script's menu holds the player (Esc opens it only while
  playing or in a conversation). It holds everything, whatever `--menu-pause` says: the
  picture (the virtual clock is paused) and the voices of the lines being said.

Activation by hand (`activate.rs`, `PlaySet::Activation`, before this) calls
`script::activate_by_hand`: a target whose script has an OnActivate block runs it
*instead of* the default action, which then runs only if the script calls `Activate`
(fnv-game's rule [documented: GECK wiki Activate]); its effects come through the trace
like any other script's. Without such a block `Game::default_activation_by_hand` runs:
the same default action as `Game::activate` (the extender's OnActivate, the receiver's
switch, a pile's or a pushed-down actor's inventory, the door the player arrived through,
children, OnAdd, drinking, disarm XP, followers), except that the engine's loading stays
in charge of entering the new cell (`player_entered` follows when the player's cell has
changed). `activate.rs` applies the outcome itself, so the activation's `Activated` and
`ItemAdded` events do not become effects as well.

## 3. Trigger volumes

A trigger is a scripted reference in a loaded cell whose script has OnTriggerEnter,
OnTrigger or OnTriggerLeave and which carries a placed primitive (`XPRM`, box or sphere;
bounds as half extents / radius as `fnv_phys::primitive` reads them [hypothesis,
physics.md §4.1]). Physics does not hold these volumes (it loads primitives of
CollisionMarker references only), so the test is geometric, once per frame. Round 2
follows docs/spec-notes/level-design.md §11.2 T1–T3:

- **Who** [documented, T1]: any actor, not only the player: the player and every
  present, living NPC and creature (ACHR, ACRE) of the loaded cells (and actors a script
  moved there), at the simulation's position (placed NPCs have no AI movement yet, so
  they stand where they were placed). The subject and the action reference are the
  actor; the runtime matches a block parameter (`Begin OnTriggerEnter player`, as both
  opening triggers have) against the subject, so such a block runs only for the
  player. Items and projectiles fire nothing (the ActorZone and Projectile flags are not
  read, T5).
- **How** [documented: a collision check of the object against the volume, T2]: the
  actor's upright capsule against the box or sphere (`script_events::Capsule`,
  `TriggerShape::touches`: the closest distance from the capsule's axis to the volume,
  by a golden-section search for the box). The player's capsule is its physics
  character's (radius 22, height 128); other actors use `CharacterParams::default()` ×
  the reference's scale [hypothesis: the game uses each actor's own collision]. Round 1
  tested three points on the player's axis, which missed a capsule overlapping the side
  of a volume.
- **When**: OnTriggerEnter once on the frame an actor first touches the volume, then
  OnTrigger every frame it does, **the entering frame included, after OnTriggerEnter**
  [documented: every frame inside, T3; hypothesis: the order on the entering frame,
  open in game, level-design.md §13 question 2]; OnTriggerLeave once on the first frame
  it no longer does. A trigger that is disabled gives no Leave; an actor that is
  disabled, dies or leaves the loaded cells gives one [hypothesis]. A trigger whose cell
  unloads gives none to the actors still in it, but gives one to the player, who went
  away (fast travel, a door, MoveTo) [design, #2449: the Boomers' artillery field counts
  the player in and out, and a player who travelled out stayed counted;
  observer-checks.md check 137].

## 4. Effects

`script_events::effects` maps trace events (`fnv_game::trace::EventKind`) to what the
engine does:

| Event | Effect |
|---|---|
| `ref_enabled` / `ref_disabled` | `RefChanged` for the reference and every reference whose enable-parent chain leads to it: visuals hide or show (`activate.rs` sync), collision goes or comes back (`phys.rs`) |
| a reference newly marked for delete (`MarkForDelete`; no trace event, read from `Game::extra`) | the same `RefChanged` |
| `moved` of the player (`MoveTo`) | `Travel` to the destination cell (interior or worldspace), feet at the marker, heading kept; none when the player already stands there (within 32 units, same cell) |
| `activated` (a script's `Activate`) | the outcome read back from its JSON and turned into effects by `activate::outcome::effects`, as for E: `RefChanged`, `Travel` and `OpenContainer` when the actor is the player, the outcome's message; an actor activated for the player is talked to instead (`Runtime::talk_after`, docs/design/dialogue-play.md) |
| `message_shown` | a line for a message that is not a box; for a box (`MESG` "Message Box" flag or buttons) its text and buttons are kept for when the runtime's queue brings it to the front |
| `line_started` | the line's response texts (INFO `NAM1`) are read for the subtitles |
| `objective_displayed` / `_completed` (on), `quest_completed` / `_failed` | a line ("Objective: …", "Completed: …") |
| `item_added` / `item_removed` for the player | a line; four or more items at once collapse into one |
| `image_modifier_applied` / `_removed` | none through the events: nvforever's grade pass reads `Game::image_modifiers` every frame and draws them (fade colour, blur, double vision, radial blur, grade changes; docs/spec-notes/image-modifiers.md §5–6) |

**Subtitles** (round 2): each line being said (`Game::speech`) shows at the bottom of
the view, its text alone in the HUD's amber and font 7 (docs/spec-notes/corner-messages.md
§8; the log keeps "Speaker: text"), one response of the INFO at a time, each for as long as it
lasts (its voice file's length, or the length fitted to its text when it has none,
scripts.md §7.7), and goes when the runtime ends or cuts the line. The voice file is
played (`voice.rs`, docs/spec-notes/voice.md §9; `--no-voice`, `--voice-volume`).

**Player controls** (2026-10-10): `DisablePlayerControls` and `EnablePlayerControls`
have no trace event; after each frame's scripts the runtime's seven flags
(`PlayerState::controls_disabled`) are copied to `control::ControlLocks`
(`script::sync_locks`), and the player's input, activation, the crosshair and prompt and
the inventory key obey them from the next frame (docs/spec-notes/controls.md §5). A
change is a log line and a `controls` line in the script trace.

Not applied (open, §7): `SetOpenState` on a drawn door, sound and the other Layer 6 stubs (of the image-space functions, the
screen splatter and `SetCellImageSpace`).

**References enabled at run time** that render streaming skipped (it draws only what is
enabled at the start of a game) are drawn by `script_draw.rs` while scripts run: the base
form's model, at the reference's pose (the simulation's once it holds one), under the
cell's `RenderCell` entity, with `RefVisual` on every mesh. Actors, SpeedTrees, markers
and light sources are not; the enabled-at-start rule is a copy of render streaming's.
A light source has its own `RefVisual` (`lights.rs` `spawn_light`, docs/spec-notes/model-lights.md
§2), so it goes out and comes back with its reference [design].

**References created at run time** (`dropped.rs`) go by the same filter on what is drawn
[design: `dropped::draws_model`]: a marker, or a base of a type placed references are not
drawn for (PlaceAtMe of an explosion), draws nothing. The reference stays in the
simulation for scripts, and its collision still comes from its model
(`phys::add_created`). That the original hides a marker made at run time as it hides a
placed one is [hypothesis, as render streaming's `is_marker`]. An explosion's created
reference is not removed after the blast (open).

**Placed NPCs** carry `RefVisual(reference)` on their root and every mesh entity
(`engine/nvforever/src/actors.rs`, `spawn`), so a script's Disable hides them and Enable
of one that was drawn shows it again; their collision is the physics side's (none for
actors yet).

## 5. Test aids and the trace log

- `NVF_PLAY_SCRIPT_TRACE=<path>`: every trace event as one JSON line (`frame`, `kind`
  and the kind's fields as in `fnv.script.trace/1`, plus `t`, seconds since scripts
  started, and `gt`, the runtime's game time `Game::time` at the end of the frame), the
  test-aid steps that ran (`aid`, with any error) and the answers to script menus
  (`menu_answer`), after a header line; at exit a `final` line: frames, tick times,
  events, the open menu and waiting boxes, quests that have a stage, objective or
  completion or were stopped, the player state, unhandled and stubbed calls by opcode.
  Unattended checks read this file.
- `NVF_PLAY_TEST_SCRIPT=<when>:<op>[,...]`, stand-ins for Layer 6 and the player.
  `<when>` is seconds after the start, or (round 2) `<seconds>@<quest>=<stage>`: that
  many seconds after the frame the stage was first seen done (the opening's timing
  depends on loading and frame rate, so its steps wait for stages). Ops:
  `start:<quest>`, `stop:<quest>`, `enable:<reference>`, `stage:<quest>:<n>`, `objective:<quest>:<n>`
  (display), `objective_done:<quest>:<n>`, `info:<INFO>[:<speaker>]` (a dialogue line
  said: its result scripts at once, conditions not required), `button:<n>` (answer the
  front box with button n, 0-based), `close` (OK / Done of the open script menu),
  `var:<quest or ref>:<name>:<value>`, `walk:<reference>` (the player's feet on the
  floor under the reference, e.g. a trigger's centre: a stand-in for walking there; the
  trigger test then fires as for a walk), `activate:<reference>` (E on it, wherever the
  crosshair is). Names are Editor IDs, hex FormIDs, `player`, or the runner's
  `scripted:<SCPT>` / `placed:<base>`.
- `NVF_PLAY_TEST_SCRIPT_MENUS=<seconds>|off`: every menu or box a script opens is
  answered with OK / Done (button 0) once it has been on screen that long. Unattended
  runs (any `NVF_PLAY_TEST_*` variable, or `--auto-walk`) default to 0.25 s, so checks
  that do not answer them (the Freeside gate, Goodsprings with `--auto-walk`) are not
  held by the pack and add-on boxes [decision; owner review]; attended runs default to
  off.
- `NVF_PLAY_TEST_TALK` (conversations: start one, choose by index, topic or text) and
  the rule that unattended runs never wait in a conversation:
  docs/design/dialogue-play.md §4. The `var:` step takes names with a colon
  (`var:scripted:<SCPT>:<variable>:<value>`).
- `NVF_PLAY_SCRIPTS=0`: no scripts (Layer 4 play exactly as before).
- `NVF_PLAY_TEST_DROP=<seconds>:<item>:use` (menu/test_aid.rs) uses an item from the
  inventory as Enter does; with an aid item its effects start in the runtime (§2).
- With the Layer 4 aids (`NVF_PLAY_TEST_NEAR`, `_ACTIVATE`, `_GOTO`, `_LOOK`, `_SHOTS`,
  layer4-play.md §6).

## 6. Checks

### 6.1 Round 1 (2026-10-09)

Unattended runs (`RUST_LOG=info`, `NVF_PLAY_SCRIPT_TRACE`), all exit 0 with "run
finished":

- **Goodsprings**, `--cell Goodsprings --play`, 40 s (and 45 s with `--auto-walk 300`
  after merging main): data ready in 0.93–1.22 s (77,456 units, 3,707 scripts, 640
  quests; 359 start-game-enabled quests running). ControlRumble's stage 0 runs all five
  entries at once (its own script; the same end state as the capture saves,
  layer5-captures.md §8.3 a). Gun Runners' Arsenal's start script moves 17 vendor
  containers. At 5 s the pre-order packs' quest adds 150 items (one line, "19 items
  added (150 in all)") with four message boxes; Old World Blues, Honest Hearts and
  Lonesome Road show their two boxes each, set their MQ00 stage 10 and display objective
  10 ("Objective: Investigate the midnight showing at the Mojave Drive-in." …) and
  enable their radio references; Gun Runners' Arsenal's boxes at 20 s. 48 OnLoad blocks
  ran as the 25 cells loaded. Script time 0.23–0.25 ms per frame on average, longest
  1.4–2.4 ms (about 2,260–2,560 frames). The unhandled calls are mostly two per-frame
  conditions (IsAnimPlaying, IsRaining).
- **Doc Mitchell's house**, `--cell GSDocMitchellHouse --play`, 28 s
  (`NVF_PLAY_TEST_NEAR=00105235`, E at 8 and 19 s, `GOTO`, `FACE`, `LOOK`, script aids):
  - E on the Chemistry Set runs its OnActivate (no default action): message box
    GSChemistrySetMsg with three buttons; the aid picks button 1 (2 on screen); the
    set's `Begin MenuMode 1001` block reads it and adds five Stimpaks ("Stimpak (5)
    added").
  - Walking into VCG01VigorTesterTrigger 0x00104C16 (goto at 14 s): OnTriggerEnter sets
    VCG01 stage 60, whose result script completes objective 10 ("Completed: Walk to the
    Vit-o-matic Vigor Tester.").
  - Doc's line 0x001074A2 (aid) displays objective 30; E on the Vigor Tester runs its
    OnActivate (stage 60 and objective 30 shown): ShowLoveTesterMenuParams (stub) and
    stage 65, which completes objective 30.
  - Leaving the trigger (goto at 22 s, into the couch trigger): OnTriggerLeave shows the
    two tutorial lines (their `&sUActn…;` key tokens shown as written).
  - Doc's line 0x00105599 (aid) enables the Pip-Boy card 0x001084B0, initially disabled:
    `script_draw` draws it (pipboycard01.nif) on its easel by the couch (screenshot at
    25 s). Script time 0.07–0.09 ms per frame.
- **Freeside → Strip gate** 0x00116425 (`--world FreesideWorld --cell=-2,-3 --play`,
  `NVF_PLAY_TEST_NEAR=00116425`, 24 s): the layer4-play.md open item. E at 9 s runs the
  gate's OnActivate (SCPT 0x00119787); without a passport it calls `Activate` and the
  default action says "Gate is locked (requires a key)". The aid sets
  `VFreeformFreeside.PlayerPassport` to 1 (the stand-in for the Securitron's dialogue);
  the gate's GameMode unlocks it (the prompt turns from "Locked (requires a key)" to
  "Open: The Strip"). E at 15 s: OnActivate starts the Strip quests, then `Activate`
  teleports the player; the trace's `activated` outcome becomes a `Travel`, physics and
  rendering switch to TheStripWorldNew and the player stands at (1004, 8722) on a floor
  at 1017.68. Triggers by the gate fired on the way (OnTriggerEnter of 0x00163BDF and
  0x001768D4).
- The CLAUDE.md checks: moving-camera smoke test exit 0 (215 cells), time-lapse exit 0
  (30 hourly lines; it runs with `--play`, so scripts ran too, 0.24 ms per frame), the
  workspace and engine tests and clippy, both Python suites. The parity lab gives
  347/374 (11 fail, 16 error); this branch changes no root crate, so the `fnvtool` is
  `main`'s and so are the results.

### 6.2 Round 2: the new game in `--play` (2026-10-09)

*Since the dialogue screen (2026-10-10) the psych test and Doc's farewell of this run are
real conversations driven by `NVF_PLAY_TEST_TALK`; the aids as given today and the
result are in docs/design/dialogue-play.md §5–§6. What follows is the round 2 run as it
was, with stand-in lines.*

`nvforever --data <Data> --play --new-game --run-seconds 230` with
`NVF_PLAY_TEST_SCRIPT_MENUS=1` (every box and character-creation menu answered OK /
Done after 1 s on screen) and these stand-ins for what the player does
(`NVF_PLAY_TEST_SCRIPT`, each gated on a VCG01 stage; the walk-through's inputs,
crates/fnv-game/walkthrough/intro.actions.json, scripts.md §9): 2 s after 55 `walk` into
the Vigor Tester's trigger (`scripted:0x00104C02`, so the real trigger test fires); 12 s
after 60 `activate` VCG01VigorTesterREF; after 80 the psych test's result scripts (the
scores, `bBeganTest`, objective 40 completed, `bDoFinalCalculation`) and Doc's verdict
line 0x00105570 (the line the walk-through's `say_first` picked); 2 s after 110 `walk`
into the exit trigger (`scripted:0x0010722F`); after 115 Doc's two farewell lines
0x001057E8 and 0x001057E7. Everything else is the game's own: the timers, Doc's SayTo
lines (`Game::say`, timed by `Game::speech`, shown as subtitles), the boxes and menus
the scripts open. The aids as given (one line):

```
NVF_PLAY_TEST_SCRIPT=2@VCG01=55:walk:scripted:0x00104C02,12@VCG01=60:activate:VCG01VigorTesterREF,
1@VCG01=80:var:VCG01:bBeganTest:1,1@VCG01=80:objective_done:VCG01:40,1@VCG01=80:var:VCG01Test:nSneak:2,
1@VCG01=80:var:VCG01Test:nScience:3,1@VCG01=80:var:VCG01Test:nSpeech:1,1@VCG01=80:var:VCG01Test:nSmallGuns:2,
1@VCG01=80:var:VCG01Test:nMedicine:1,1@VCG01=80:var:VCG01Test:nSurvival:1,
1.5@VCG01=80:var:VCG01Test:bDoFinalCalculation:1,3@VCG01=80:info:0x00105570:DocMitchellREF,
2@VCG01=110:walk:scripted:0x0010722F,2@VCG01=115:info:0x001057E8:DocMitchellREF,
3@VCG01=115:info:0x001057E7:DocMitchellREF
```

**Result** (after merging main, 59 frames per second): exit 0. The chain reaches
**VCG01 200** (completed, stopped), VMQ01 10, VCG02 5, CGTutorial 2, with VCG00 stages
0, 90, 95, 100 and VCG01 stages 0, 1, 3, 5, 7, 8, 10, 15, 25, 30, 35, 36, 40, 45, 50, 55,
60, 65, 70, 75, 76, 79, 80, 85, 90, 95, 98, 100, 102, 105, 110, 115, 200: the
walk-through's set and both capture saves' (scripts.md §9). Both opening triggers fired
from the player's capsule (`OnTriggerEnter` of 0x00104C16 → 60, of 0x00107236 → 115;
leaving the first ran its OnTriggerLeave tutorial lines); no NPC fired a trigger in the
house (Doc stands outside them). Four pack boxes, menus 1051, 1036, 1074, 1048, 1084 and
the hardcore box opened and were answered; Doc's lines showed as subtitles. An earlier
run at 8.7 frames per second (the machine was busy) reached the same end with steps up
to one frame (0.1–0.25 s) longer.

**Timeline** (game seconds, `gt` in the trace, each step from the one before;
walk-through and capture from scripts.md §9 and layer5-captures.md §10.2, §10.3, §10.7):

| Stage | `--play` | Walk-through | Capture |
|---|---|---|---|
| VCG00 0 → 90 | the first script frame | given (save) | in the save |
| VCG00 95 / 100 / VCG01 0 | 3.03 / +1 frame / same frame | 3.03 / +1 frame / same | 3.03 / +1 sample / same |
| pack boxes | 5.0 (between 1 and 3), one at a time, the next on the frame after a close; game time stood still | 5.0, as here | one open in the save before VCG00 ran; the next 0.45–1.4 s after each close |
| VCG01 1, 3 | +0.19, +2.83 | +0.22, +2.93 | +0.2, +2.8 |
| 5 (Doc's first line) | +2.91 | +2.90 | +2.9 |
| 7, 8 | +3.27, +1 frame | +3.27, +1 frame | +3.22, +1 sample |
| 10 (three lines), name menu | +15.36, 1051 | +15.37, 1051 | +15.4, 1051 |
| 15 after the name menu | 1.02 | 1.05 | 0.95–0.97 |
| 25, 30 (lines) | +10.52, +14.77 | +10.50, +14.77 | lines |
| 35, 36, race menu, 40 | +3.01, +1.78, 1036, 40 inside it | +3.02, +1.78, 1036, 40 inside | +2.97, a line, 1036, 40 inside |
| 45 (lines), 50 | +11.08, +3.01 | +11.15, +3.02 | lines, +2.98 |
| 55 (two lines) | +10.20 | +10.20 | about 10 s |
| 60 | the trigger, on the frame of the walk | by input | by the job |
| 65, menu 1074 | inside the activation | inside | inside |
| 70 | 1.02 after the menu | 1.02 | 0.96 |
| 75, 76, 79 | one frame, +7.47 after 70 | one frame, +7.47 | one frame |
| 80 (psych intro lines) | +18.06 | +18.07 | about 13 s |
| 85 → 90, menu 1048 | 85 by the verdict line, 90 +1.00 | +1.02 | 1 s |
| 95 | 1.02 after the menu | 1.02 | 0.96 |
| 98, 100, 102, menu 1084 | +17.95, +1.01, next frame | +17.93, +1.02, next frame | lines, 0.98, next frame |
| 105 | 1.02 after the menu | 1.02 | 0.97 |
| 110 (lines) | +3.47 | +3.47 | lines |
| 115, 200 | the exit trigger on the walk's frame; 200 0.34 after the hardcore box was answered | by input; 200 0.38 after the box | by the job; event 3 |

Differences: (1) the pack boxes come 5 s into the game (the pack quests' first run),
after VCG00's countdown, as in the walk-through; in the capture's save one box was
already open and held the countdown (§7); (2) every span of Doc's lines uses the
runtime's fitted line length (80: 18.1 s against about 13 s), as in the walk-through;
(3) at low frame rates timer steps read up to a frame later. Which
stage follows which, by what, and which menu each opens match the walk-through and the
capture.

### 6.3 Round 2 checks (2026-10-09, after merging main up to #102)

All windowed runs exit 0 with "run finished" (repeated after each merge of main; the
last after #97–#102: the new game again to VCG01 200 with the same stages, 56 frames per
second):
- **Freeside → Strip gate** (round 1's command, 24 s): unattended, so the pack and
  add-on boxes (12) were answered after 0.25 s each; E at 9 s says "Gate is locked", the
  passport aid unlocks it, E at 15 s travels to TheStripWorldNew (feet on the floor at
  1017.68), as in round 1. On the way NPCs fired triggers too: the Freeside Securitrons
  entered VFSSecuritronWarningActivatorREF, whose script (SCPT 0x0011CB52) has a
  `Begin OnTriggerEnter player` block and a parameterless one that reads
  `GetActionRef` and checks its faction [observed in the masters], the case T1
  describes; Strip NPCs entered their triggers. Two Securitrons stand inside
  VFSVegasAnnihilationVolumeREF, whose parameterless `Begin OnTrigger` (SCPT 0x00119786)
  ran every frame for them (1,630 runs in 24 s); its faction checks on `GetActionRef`
  sort Securitrons from intruders [observed in the masters], so it did nothing visible
  here. Whether the game runs it for actors standing in a volume from the start is part
  of the T1 capture question (§7).
- **Goodsprings**, 40 s without aids: the first pack box stays open (attended runs do not
  answer boxes), the player is held and script time stops, as in the capture's save;
  45 s with `--auto-walk 300`: every box answered after 0.25 s, the walk goes on.
- The moving-camera smoke test (213–215 cells) and the time-lapse (29–30 hourly lines).
- Workspace and engine tests and clippy, both Python suites. Parity lab with this
  branch's `fnvtool`: 368/387 after #102 (365/384 before; the same 11 fail and 8
  error, the three new scenarios pass), L5 28/32, as on main (the four known fails); the headless walk-through gives the same observations and events with and
  without the SayToDone change (D1).

### 6.4 Voice (2026-10-10, dialogue step D3)

The §6.2 run again (same command and aids, 230 s; after merging main up to #117) with
dialogue voice
(`engine/nvf-play/src/voice.rs`, docs/spec-notes/voice.md): exit 0, "run finished", 54.7
frames per second, the same stages in the same order to **VCG01 200** (completed), and
one `voice: playing <path> (<seconds> s)` log line for each of the 23 lines Doc Mitchell
says by SayTo (23 `says` lines, 23 sounds, none missing, none stopped). The three lines
the test aids stand in for (`info:` runs a line's result scripts, it does not say the
line) have no sound.

Steps that are made of Doc's lines, game seconds from the stage before (fitted: §6.2;
capture: scripts.md §9):

| Stage | Fitted length (§6.2) | Voice length | Capture |
|---|---|---|---|
| 5 (one line) | +2.91 | +2.60 | 2.9 |
| 10 (three lines) | +15.36 | +14.49 | 15.4 |
| 25, 30 | +10.52, +14.77 | +8.85, +12.76 | lines |
| 36 | +1.78 | +0.87 | a line |
| 45 | +11.08 | +10.67 | lines |
| 55 (two lines) | +10.20 | +9.22 | about 10 |
| 75 after 70 | +7.47 | +5.78 | not timed |
| 80 (psych intro lines) | +18.06 | +11.73 | about 13 |
| 98 | +17.95 | +13.15 | lines |
| 110 | +3.47 | +2.34 | lines |

All other steps (timers, menus, triggers) are as in §6.2 within 0.05 s. The 23 lines
take 103.5 s instead of 125.3 s, so the run reaches VCG01 200 at 136.4 game seconds. The
voice lengths end the fit's large error at 80 and leave every timed span 0.3 to 0.4 s per
line shorter than the capture (voice.md §8); nothing was tuned.

Goodsprings `--play` for 40 s: exit 0 (no line is said there in that time). Workspace
and engine tests and clippy, both Python suites; parity lab 368/387 (the same 11 fail
and 8 error as on main), L5 28/32 (the four known fails), the four `L5.intro.*` pass
with identical observations with and without voice lengths.

## 7. Open

- **Runtime follow-ups**: done in fnv-game rounds 3–4 (a scripted `Activate player`,
  menus and loading screens, OnLoad after Enable, items' GameMode); nvf-play uses the
  runtime's rules for all of them.
- **`GetButtonPressed`** is per owner since fnv-game round 4 (only the script that
  showed the box receives its button), which ended the chemistry set taking every box's
  button.
- **Round 2 open items**:
  - The character-creation menus are placeholders: no name, race, S.P.E.C.I.A.L., tag
    skill or trait is chosen (Layer 7). The psych test's tags come from the runtime's
    VCG01Test calculation, fed by test aids in §6.2.
  - A script's `DisablePlayerControls` holds the player since 2026-10-10 (§4,
    docs/spec-notes/controls.md §5): in the opening the player cannot move or look until
    VCG01 stage 55 gives both back.
  - Dialogue with player choices (the psych test, Doc's farewell) is played on the
    dialogue screen since 2026-10-10 (docs/design/dialogue-play.md); the couch
    (`IsCurrentFurnitureRef`, AI), Doc's walk to the player for the farewell and walking
    are still test aids; Doc's other lines are the runtime's own (`Game::say` from the
    quests' SayTo).
  - Which of OnTriggerEnter and OnTrigger runs first on the entering frame
    (level-design.md §13 question 2), whether a capsule that touches a volume without
    its axis inside fires it, and whether non-actors fire triggers without ActorZone:
    capture candidates.
  - The pack boxes open when the pack quests first run, 5 s after the start (their
    quest delay); in `nvfnew` one was open at once and VCG00's countdown waited for the
    boxes (§6.2).
  - Bevy's virtual clock caps a frame at 0.25 s, so a very slow frame loses real time
    for scripts as for everything else.
- A reference a script moves, turns or resizes is drawn where the simulation holds it
  (`moved.rs` for render streaming's entities, `script_draw.rs` for its own) from
  `Game::turned`, which holds only what the tick's own scripts moved by then, and from
  `RefRepose`, which also names what was in `turned` when `script::run_scripts` copied it
  just after the console's lines [design]. The console's move is the case this was made
  for (smoke `i2-setpos-console`, a pair of frames to compare by eye). Not audited for the
  drawing: OnActivate by hand, terminal and dialogue result scripts, moves made under the
  menu and loading early returns, the calls the tick runs before it clears `turned`
  (extender main loop and delayed calls), and a move into another cell.
- Voice (§6.4): sounds are not placed in space, do not pause with menus, and the
  dialogue the test aids stand in for is not heard (it waits for the dialogue screen,
  docs/design/dialogue.md D4).
- NPCs enabled at run time that render streaming skipped are not drawn (`actors.rs`
  would have to build them; coordination).
- OnHit, OnDeath, combat, dialogue and AI package events wait for Layer 6.
