# Quest sweep: soft-lock shapes found in the data alone

`fnvtool quest sweep <Data> <plugins…> --schema schemas/fnv` (`fnv.quest.sweep/1`) reads a
load order and lists the shapes that can leave a player stuck, without running a script.
It is item 5 of issue #2365 (the soft-lock watchdog, the quest pilot, save/load at
milestones and this sweep). Code: `crates/fnvtool/src/quest_sweep.rs`; smoke scenario
`quest-sweep` in `tools/smoke/scenarios.toml`.

The sweep is a **list of places to look**, not a verdict. It never decides a condition it
cannot read statically, so every check below says which way it errs. A hit means the data
has the shape; whether the original game gets out of it (a fade ending, a hard-coded rule)
is a question for the observer (`docs/handoff/observer-checks.md`).

## How it reads the data

Every compiled script unit the game builds (`GameData::units`: object, quest and effect
scripts, stage, line, package and terminal result scripts) is walked once. For each
function call statement it keeps the function (lower case), the reference it runs on,
its parameters (numbers and forms), the `Begin` block it sits in and what the `If`
conditions around it read (function names and variables). `set <form>.<var> to …` is kept
as a write to another script's variable, and a local set from `GetSecondsPassed` as a
timer. Calls inside expressions are read only for the conditions; extender expressions
(`let`, `eval`) are not walked.

Conditions on dialogue lines are read as OR groups joined by AND (a condition with the OR
flag binds to the next one [documented: GECK wiki Conditions]). Of the functions on the
speaker (Run On Subject), five are decided per actor: `GetIsID`, `GetIsVoiceType`,
`GetInFaction` (the base's own factions; a template's are unknown), `GetIsSex`,
`GetIsRace`; a form list stands for its members. Every other condition counts as passing.
The line's speaker (ANAM) counts as a `GetIsID`.

## 1. Conversations with no way out (`conversations`)

**1a `traps`.** For every player line (type Topic, or a GREETING line) that offers
something to choose, the sweep follows the lines the speaker may say after it: the lines
of its choice topics (TCLT), then the lines of the topics linked to it by Link From
(`Game::links_after`, dialogue.md §3.5). A line ends the search when it has the Goodbye
flag or offers nothing (the player gets the topic list back, which NVForever lets them
leave, #1595). A line from which no chain of choices reaches either is a trap. The
speaker is the first actor the line names with `GetIsID`; without one, any speaker.

Errs towards silence: every undecided condition passes, so a hit has no way out even when
every line is available. A real conversation can still trap when the exits' conditions
fail in play (Say Once lines said, quest stages not reached); this check does not see that.

**1b `goodbye_less_top_levels`.** For every placed actor that some top-level topic line
names with `GetIsID`, whether a Goodbye is shown open to it: a line of GOODBYE, or a
top-level line with the Goodbye flag, whose every condition group the sweep can decide
for the actor and finds passing (here an undecided condition fails). The actors with
none are listed (#1595's shape), each with up to three Goodbyes open to it only if an
undecided condition passes (`exits_behind_undecided_conditions`). A line's quest's
dialogue conditions count as the line's own [documented: GECK wiki Quest Data Tab;
docs/spec-notes/dialogue.md §4]. NVForever's topic list can be left with Tab, so this is a
"how does the original let you out" list rather than a soft lock in NVForever.

## 2. Controls taken by one script and given back by another (`controls`)

A call **takes the controls** when it is `DisablePlayerControls` with no parameters or a
non-zero first one (movement) [documented: GECK wiki DisablePlayerControls], or
`SetRestrained 1` on the player. A call **gives them back** when it is
`EnablePlayerControls` (no parameters or movement non-zero) or `SetRestrained 0` on the
player.

From each unit that takes the controls, the sweep follows (breadth first, `--depth`
steps, 4 by default) what the unit sets going: the stage result scripts and quest script of
a stage it sets, the script of a quest it starts, the script of a form whose variable it
writes, the result scripts of a package it gives (`AddScriptPackage`) or makes the actor
re-pick (`EvaluatePackage`, the actor base's packages), the lines of a topic it says
(`SayTo`, `Say`, `StartConversation`, a Dialogue package's topic), a line's choices, the
script of a reference it activates, enables or moves. Every call that gives the controls
back in those units is a **release**, with the path that reached it and what it waits for:

| tag | when |
|---|---|
| package | a package result script, an `OnPackage…` block, a `GetCurrentAIPackage`/`GetIsCurrentPackage`/`GetCurrentAIProcedure` condition, or a path through a package |
| dialogue line / dialogue | a line's result script; a path through `SayTo`, a conversation or a choice; `IsTalking`/`GetTalkedToPC` conditions |
| trigger | an `OnTrigger`, `OnTriggerEnter` or `OnTriggerLeave` block |
| timer | a condition on a local counted with `GetSecondsPassed` |
| position, actor state, time of day, who triggers | `GetDistance`/`GetInCell`…, `GetDead`/`GetDisabled`/`IsInCombat`…, `GameHour`/`GameDaysPassed`, `IsActionRef` |
| stage, message box, activation, terminal | the rest, not counted as risky |

Each unit gets one **verdict**: `in place` (a release in the same block with none of the
risky tags: not listed), `released` (a release elsewhere with no risky tag), `waits`
(every release found waits for one of package, dialogue, trigger, timer, position, actor
state, time of day, who triggers), `unreleased` (no release found within the depth). The
two shapes the issue names: Hardin's training (#2309: the line's end script restrains the
player, the quest script's timer gives Hardin a Dialogue package, only that package's line
releases) and Ramos' walk (#2453: the line's end script takes the controls and writes a
variable on a trigger; the trigger's `OnTrigger player` block, behind that variable and a
timer, gives them back).

Errs towards noise: the walk ignores order and conditions on the way, so a release that
can never run after the hold still counts; and `unreleased` includes holds released by a
script the walk cannot reach (a global variable read by a quest script, an
`OnLoad` of another cell, the menu's own end). Read `waits` and `unreleased` as the places
where a stuck package, line or trigger leaves the player without controls. Most `waits`
are fades whose release counts seconds; `controls_waiting_beyond_a_timer` counts those
whose every release also waits on something risky other than a timer.

## 3. Stages only something hard to reach sets (`stages`)

For every `SetStage <quest> <stage>` with both written out, the stage's setters are
classed: a package result script or `OnPackage…` block (**package**), an `OnTrigger…`
block of a script whose placed references have no navmesh triangle within 128 units of
their box, horizontally, and 256 units plus the box vertically (**trigger without
navmesh**; the references of the cell, and for an exterior cell the 3×3 cells around it),
a trigger script with no placed reference (**trigger never placed**), an `OnActivate`
block of a script whose every placed reference starts disabled (**disabled activator**,
with its enable parent and the scripts that `Enable` it), or anything else (a line, a
terminal, a stage, a quest or object script). A stage that some condition or script tests
(`GetStage`, `GetStageDone`) is listed when **all** its setters are of the first four
kinds, and also when it is defined in the quest, tested, and set by nothing (**no
setter**). `SetStage` calls whose quest or stage is computed are counted, not followed.

A hypothesis list: a package's end script is how many quests are meant to go on (an
escort arriving), and an activator starting disabled is usually enabled by the step
before. The list says where the quest relies on an AI package finishing or on a reference
that may not be there; it is the check behind #2445 (Chauncey never comes to the steam
room).

## 4. Locked doors only a script opens (`doors`)

Every placed door locked at level 255 (Requires Key [documented: GECK wiki
GetLockLevel; `fnv_game::lockpick::KEY_ONLY`]) with no key, and the calls that open it:
`<door>.Unlock`, `<door>.Lock 0`, `<door>.SetOpenState 1`, or the same without a
reference in the door's own script. Each opener says what runs it: a line (its topic, its
speakers, whether a speaker is a talking activator: an intercom), a terminal item (its
text), a stage, a package, a quest or object script (the bases that use it). REPCONN's
`REPCONMidToTopDoor`, unlocked by INFO 0x000CD87E's end script, is the known case. Doors
with no opener at all are counted (`never_opened`; mostly doors that are not meant to
open).

## Output

```
{ "schema": "fnv.quest.sweep/1", "load_order": [...], "seconds": …,
  "summary": { "conversation_traps", "goodbye_less_top_levels", "control_holds",
               "control_verdicts": {"in place", "released", "waits", "unreleased"},
               "controls_waiting", "controls_unreleased",
               "controls_waiting_beyond_a_timer", "stages_hard_to_reach",
               "stages": {"stages_set_and_tested", "tested_never_set", "by_kind", …},
               "doors_opened_by_script", "doors": {"key_only_without_key", …},
               "units", "lines", "navmeshes_read" },
  "conversations": {"traps": [...], "goodbye_less_top_levels": [...]},
  "controls": [...], "stages": [...], "doors": [...] }
```

Forms are written `EditorID 0xFORMID` (load-order FormIDs). `--no-nav` skips the navmeshes
(faster; the trigger class is then never given).

## Results on FalloutNV.esm

See `docs/handoff/quest-sweep-2026-11-14.md` for the counts on the base game and how the
playtester reports of 2026-11-13 line up with them.
