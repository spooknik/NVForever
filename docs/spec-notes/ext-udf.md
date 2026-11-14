# Spec note: extender user-defined functions

Covers function scripts (`Begin Function {...}`), calling them, their return value, the
calling script and lambdas.

Filled for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
from the GECK wiki export. Everything the pages state is tagged `[documented: <page>]`;
what they leave out is **open (to observe)**, to be settled by the GECK compile cases
(tools/capture/ext-compile/, cases 15–19) or captures in the original game. The
"User Defined Function" page arrived in the 2026-10-25 export (`NVSE.xml`).

Behaviour is our own wording of the GECK wiki export (`C:\fnv-private\geckwiki\`,
private); no extender source, no opcode numbers or argument encodings (those come only
from a cited document or from compiling our own test scripts with xNVSE,
[extender-compat.md](../design/extender-compat.md) §4.1).

## 1. Function scripts

- **Form** (NVSE 4): saved as an **object** script that uses only the `Function`
  block; it is attached to nothing and called by name with Call. Up to **15 parameters**
  of any type, listed in braces, possibly empty [documented: User Defined Function;
  Function]:

  ```
  scn MyUDF
  int a
  int b
  Begin Function {a, b}
      ...
  End
  ```

  The names in the braces are variables declared in the script's own variable list
  [documented: Lambda, the OnKeyDownUDF example; CallAfterSeconds, `{ ref rPlayer }`
  with an inline type]. Parameters are separated by commas in the examples; whether
  spaces alone also separate: open (compile case 15 uses commas).
- **Only the Function block** may be used [documented: User Defined Function].
- **Any-type parameters**: an array_var parameter receives a boxed value (`call MyUDF
  &MyRef`), unboxed with `*` and tested with TypeOf or GetType [documented: User Defined
  Function].
- **Arguments** arrive in the parameter variables in order. Too few or too many, or a
  value of the wrong type: open.
- **Arrays made inside a function script are freed when it ends** (NVSE 4.6.2 and
  later), unlike arrays in other scripts [documented: Ar Null].

## 2. Calling: Call

- **Syntax**: `Call udf [arg1 … arg15]`, or `ref.Call udf …` [documented: Call].
- **Arguments**: the function script, then up to 15 arguments matching its parameters.
- **Result**: whatever the function set with SetFunctionValue (§3); any type.
- **Calling reference**: with `ref.Call`, the commands inside the function act on that
  reference [documented: Call]. Inside, `this` names it [documented: Lambda, `this.Say
  Hit`; Ar Generate, `let ref rActor := this`].
- **Using the result**: store it with `let` (not `set`) and test it with `if eval`
  (not plain `if`): `let v := Call MyUDF x`, `if eval (Call MyUDF)` [documented: Call;
  SetFunctionValue].
- **A ref variable holding a function** can be called: `Call rCallback 10` [documented:
  Lambda].
- **Passing a boxed value**: `call MyUDF &MyRef` passes a one-element array, for
  parameters of array type [documented: NVSE Expressions].

- **Recursion** works: a function may call itself, but the script must first be saved
  once without the call (the name does not exist yet). **At most 30 nested calls**; past
  that, NVSE aborts further recursion [documented: User Defined Function].

Open: what Call returns when the function never set a value; what the 31st nested call
returns; a variable holding something other than a function script.

## 3. The return value: SetFunctionValue

- **Syntax**: `SetFunctionValue value` (any type, an array included) [documented:
  SetFunctionValue].
- **Only inside a Function block.** A function returns one value; when it is set more
  than once, the last one set is returned [documented: SetFunctionValue].
- **It does not end the function**: the lines after it still run to the end of the
  block [documented: SetFunctionValue].
- **Return with a value** (`return x`) is shown only in NVSEScript, the newer language;
  the ordinary-language examples use SetFunctionValue and then a bare `return`
  [documented: User Defined Function]. Plain `Return` stops the script there
  [documented: Return]; that a value set before it still counts: [hypothesis, by the
  fnShuffle example]. Whether `Return x` compiles in the ordinary language: open (probe
  udf/02).

## 4. The calling script: GetCallingScript

- **Syntax**: `let r := GetCallingScript` [documented: GetCallingScript].
- **Only inside a user-defined function**; gives the script that called it (useful for
  debugging, or as an array key).
- **Edge cases**: **0 when the function was called from a dialogue result or a quest
  stage result**. Called from a lambda, an event handler or a timer (§6): open.
- **Evidence**: [documented: GetCallingScript].

## 5. Lambdas (xNVSE 6.1.0)

A lambda is a nameless function written inside another script; it can be stored in a
ref variable or passed straight to anything that takes a script (Call, event handlers,
timers, the `ar_*` functions that take a function) [documented: Lambda; Ar Filter; Ar
MapTo].

- **Block form**: `(begin function {params} … end)` in parentheses, as an inline
  expression [documented: Lambda].
- **One-line form**: `{params} => expr`. The expression's value is returned as if by
  SetFunctionValue [documented: Lambda]. `({} => expr)` with no parameters. The
  expression runs to the end of what holds the lambda, an assignment included: `({} =>
  iInventory = iRefresh = 1)` sets both when the lambda runs [observed:
  JustAssortedMods.esp 4.6 ships that line in JLMScript, once stored in a ref and once as
  SetEventHandler's handler, with compiled bytes; hypothesis: the original binds it so.
  Until 2026-11-12 NVForever stopped the lambda before the `=`, which made the line an
  assignment to the lambda itself, refused when it ran].
- **Variables**: a lambda reads and changes every variable of the script it is written
  in; its parameters are also variables of that script, declared there [documented:
  Lambda, "access and modify all variables defined in the parent script"; the
  SetOnKeyDownEventHandler example declares `iKeyID` in the parent].
- **Stored lambdas**: `let rCallback := (begin function {} … end)`, then
  `Call rCallback`. With the newer compiler (NVSEScript), a ref variable given a lambda
  at once cannot be reassigned later [documented: Lambda]; whether the ordinary compiler
  allows it: open.
- **Hot reload** (a mod tool) invalidates the lambdas of a reloaded script, so handlers
  registered with lambdas must be registered again [documented: Lambda notes]. Not a
  parity concern for NVForever (no hot reload).
- **Length**: a GECK line is at most 512 characters, which long lambdas reach; a full
  function script avoids it [documented: Lambda; Inline Expression].

Open: what a lambda's variables hold when the parent script's instance is gone (a
lambda stored in a quest variable after the object script that made it unloads); the
calling reference inside a lambda passed to a handler (`this` is used in the examples
but not defined).

## 6. Deferred calls (related; xNVSE 6.1 and later)

Not in the core udf family of the catalogue, but they take a function and Call-style
arguments, so they share §1–§5. Each page lists a calling reference (`reference`);
what it does to the call: open.

| Function | Runs the function | Since |
|---|---|---|
| CallAfterSeconds (alias CallAfter) `secs, udf[, runWhen, args…]` | once, after that many in-game seconds | 6.1 |
| CallForSeconds (alias CallFor) `secs, udf[, runWhen, args…]` | every frame until that many in-game seconds pass | 6.1 |
| CallAfterFrames `frames, udf[, runWhen, args…]` | once, after that many frames | 6.2.8 |
| CallWhile `udf, cond[, flags, args…]` | every frame while `cond` returns true; never again once it is false | 6.1 |
| CallWhen `udf, cond[, flags, args…]` | once, the first frame `cond` returns true (`cond` tested each frame) | 6.2.4 |
| CallWhilePerSeconds `interval, udf, cond[, flags, args…]` | every `interval` seconds until `cond` returns false | 6.2.8 |

[documented: each page]. Shared rules:

- **runWhen** (the seconds and frames timers): 0 or left out, GameMode only; 1, also in
  MenuMode; 2 (6.3.0), also, but paused while the main, pause or console menu is open
  [documented: CallAfterSeconds; CallForSeconds; CallAfterFrames].
- **flags** (the condition timers), a bitmask: 1 pass the arguments to `udf`; 2 pass
  them to `cond` (both 6.2.6); 4 not in MenuMode; 8 not in GameMode (both 6.2.8); 16
  wait while the main, pause or console menu is open (6.3.0). Without flags they run in
  GameMode and MenuMode, also while paused [documented: CallWhile; CallWhen;
  CallWhilePerSeconds].
- **Arguments**: up to 15, passed like Call's (6.2.6 for the seconds timers); an array
  argument lives at least until the calls stop [documented: each page].
- **Seconds are game seconds**, scaled by the time multiplier, also in MenuMode;
  `CallAfterSeconds 0` runs next frame [documented: CallAfterSeconds; CallForSeconds].
- **Not saved**: loading a game drops every queued call; scripts re-queue them on load
  [documented: each page]. This is a persistence rule NVForever must copy (ext-save.md).

## 7. Open (to observe), collected

Parameter separators; other blocks in a function script; argument count and type
mismatches; Call's result with no SetFunctionValue; recursion; `Return` with a value;
GetCallingScript from lambdas, handlers and timers; lambda variables after the parent
instance is gone; `this` in a lambda; reassigning a ref holding a lambda under the
ordinary compiler.

## Implementation table (D3)

One row per function the plan lists for this task (docs/handoff/extender-core-tasks.md), taken from the sections above and their pages; "fmt" means a format string with up to 20 values (ext-format.md). Anything not stated by a page is marked open.

| Function | Aliases | Arguments (type, ? = optional) | Returns | Behaviour [documented] | Edge cases [documented unless marked] |
|---|---|---|---|---|---|
| Call | — | function script (or a ref holding one); up to 15 values any | any (the function's value) | runs the function; with `ref.Call` its commands act on ref | result via `let`/`if eval`; no value set: open; at most 30 nested calls |
| SetFunctionValue | — | value any | none | sets the calling Call's result | only in a Function block; last one wins; does not end the function |
| GetCallingScript | — | — | script (form) | the script that called this function | 0 when called from a dialogue or quest-stage result; from lambdas/handlers/timers: open |
| CallAfterSeconds | CallAfter | seconds float; udf; runWhen int? (0); up to 15 args (6.2.6) | none | runs udf once after game seconds | runWhen 0 GameMode, 1 also MenuMode, 2 also paused-menus wait (6.3.0); 0 seconds = next frame; dropped on load |
| CallForSeconds | CallFor | seconds float; udf; runWhen int? (0); up to 15 args (6.2.6) | none | runs udf every frame until the seconds pass | as above; dropped on load |
| CallAfterFrames | — | frames int; udf; runWhen int? (0); up to 15 args | none | runs udf once after that many frames (6.2.8) | runWhen as above; dropped on load |
| CallWhile | — | udf; condition udf; flags int? (0); up to 15 args | none | runs udf every frame while condition is true; never again once false | flags: 1 args to udf, 2 args to condition (6.2.6), 4 not MenuMode, 8 not GameMode (6.2.8), 16 wait in paused menus (6.3.0); dropped on load |
| CallWhen | — | udf; condition udf; flags int? (0); up to 15 args | none | runs udf once, the first frame condition is true (6.2.4) | flags as CallWhile; dropped on load |
| CallWhilePerSeconds | — | interval float; udf; condition udf; flags int? (0); up to 15 args | none | runs udf every interval seconds until condition is false (6.2.8) | flags as CallWhile; dropped on load |

## Observed in the original game (2026-10-26)

[observed: <id>] = tools/capture/ext-probes, run unattended in the original game (xNVSE 6.4.9, JIP 57.30) on 2026-10-26 by tools/capture/ext_probe_run.py from the owner's private test plugin; the results file is private (`C:\fnv-private\goldens\ext\`). Where a row settles an "open" point above, the observation wins.

| Probe | Input | Result | So |
|---|---|---|---|
| udf.num_by_value | the function sets its number parameter to 99 | caller still 5 | numbers pass by value |
| udf.str_after_call | the function appends to its string parameter | caller still [hi] | strings pass as a copy |
| udf.arr_after_call | the function writes element 0 | caller sees 99 | arrays are shared |
| udf.no_value | no SetFunctionValue (the target held 42) | 0 | Call gives 0 |
| udf.return_stops | SetFunctionValue 3; Return; SetFunctionValue 4 | 3 | Return ends the function and keeps the value |
| udf.return_value | a function whose body is Return 7 | 0 | Return x compiles but its value is ignored |
| udf.this_player | PlayerREF.Call, `this` inside | PlayerREF | this is the calling reference |
| udf.getself_no_ref | GetSelf in a Call without a calling reference | <NULL>, and an extender error in nvse.log | an error, not 0 |
| udf.caller_valid / caller_text | GetCallingScript from a quest script | valid; $ gives <no name> | scripts have no name text |
| lambda.result | a lambda {} => iP += 1 | 6 | += yields the assigned value |
