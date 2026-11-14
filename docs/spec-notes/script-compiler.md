# Spec note: the script compiler (source → compiled bytes)

Tasks A1–A5 of docs/handoff/extender-core-tasks.md; design in docs/design/extender-compat.md
§5.2. Code: `crates/fnv-script/src/compile/` (`lex`, `parse`, `gen`, `env`), CLI
`fnvtool script compile`, lab scenario `L5.script.compile.vanilla`.

The compiler turns a unit's source text (`SCTX`) into the bytes the editor writes (`SCDA`),
with the unit's local variables (`SLSD`/`SCVR`) and reference list (`SCRO`/`SCRV`). The
decoder (scripts.md §2) then gives the IR the interpreter runs, so the compiler needs no
second runtime path. The extender grammar (A7–A12) builds on it.

**Sources.** The byte layout is scripts.md §2 and the source rules are scripts.md §3.4
(both ours). Every rule in §3 below was found by compiling all shipped units and comparing
the result with the shipped bytes: that is observing the editor's output. No editor,
extender or game code was read, and no extender documentation was needed for the vanilla
grammar.

## 1. Lines and tokens (`lex`)

- A line ends at a line break. Comments run from `;` outside double quotes to the end of
  the line [observed: probe strings/03 splits "a,b;c" by ",;" in the original game; a GECK
  compile case reported "Mismatched quotes" for a `;` in a string (ext-bytecode.md), to be
  reconciled; no shipped string holds one, so vanilla parity is the same either way]. Blank lines and lines with no letter or digit produce no statement
  [observed: scripts.md §3.4].
- Tokens: words (letters, digits, `_`, `.`), quoted strings (an unterminated one runs to
  the end of the line), the operators `== != <= >= && || < > + - * / %`, parentheses, and
  any other single character as punctuation. Commas separate like spaces. A word ending
  in `.` joins the next word (`SomeREF. Disable`) [observed: scripts.md §3.4].
- A number is digits with at most one `.` (`19.50`, `.5`, `1.`). Expressions keep its text
  as written [observed: scripts.md §2.3].
- Lines over 512 characters get a warning [documented: Inline Expression]. The editor's
  behaviour on them is not observed.

## 2. Statements (`parse`, `gen`)

| Source | Bytes (scripts.md §2.2) |
|---|---|
| `scn` / `ScriptName` | 0x001D |
| `short` `int` `long` `float` `ref` `reference` | no statement; declares a local (`Integer` for the first three, `FloatOrRef` for the rest) |
| `Begin <block> [args]` … `End` | 0x0010 with the event code from the catalogue's `begin_blocks` (`OnPackageEnd` is `OnPackageDone`), the length of the block's bytes, and the parameter block |
| `If` / `ElseIf` / `Else` / `EndIf` | 0x0016 / 0x0018 / 0x0017 / 0x0019 with skip counts (§3) |
| `Set <var> To <expr>` | 0x0015: target, expression length, postfix expression |
| `Return` | 0x001E |
| `[Ref.]Function args` | the function's opcode with its parameter block; `Ref.` becomes the 0x001C prefix |

Declarations are read before any statement, so a variable declared inside a block (six
shipped units) is known everywhere [observed: scripts.md §2.6].

Expressions are written in postfix with each token preceded by a space. The precedence,
loosest first, is `&&`, `||`, `==` `!=`, `<` `>` `<=` `>=`, `+` `-`, `*` `/` `%`, then unary
minus, which is written `~` after its operand. Binary operators group left to right
[observed: scripts.md §3.3]. Inside an expression, a function takes as many following
operands as it has parameters [observed: scripts.md §3.4].

## 3. Rules found against the shipped bytes [observed: 14,511 units]

Each rule is followed by the evidence that pinned it down.

1. **Parameter counts.** A call takes as many arguments as the function has parameters,
   and the rest are dropped (scripts.md §3.4). For twelve functions the wiki lists fewer
   parameters than shipped calls carry, so the counts come from the bytes
   (encodings.json `compiler.param_counts`): GetShouldAttack, Unlock, StartMisterSandman,
   StartCannibal, setAllReachable (1); SetNPCRadio, SetScreenSplatterFade (2);
   ShowCaravanMenu, SwapTexture (3); TriggerScreenSplatter (4); IsDLCInstalled,
   EnableLoadingMenu (1). Without them about 420 units differed.
2. **Parameter block.** A call has a parameter block when the function takes parameters
   or has a message trailer, and none otherwise. Two exceptions, both from the bytes
   (`compiler.block`): WakeUpPC always has an empty block (count 0), and GetFriendHit,
   which the wiki gives one parameter, never has one.
3. **Begin blocks.** Whether a Begin statement has a parameter block depends only on the
   event, the same in every shipped unit. The 21 events that have one are listed in
   `compiler.begin_params` (OnActivate, OnAdd, OnTrigger, MenuMode and the others). Begin
   arguments are tagged values. A block that takes none drops its arguments
   (`begin ScriptEffectStart EncHArchimedesEffect`, FalloutNV SCPT 0x0011F676).
4. **Numbers in parameters.** A number in a float parameter is stored as a float (`z` +
   f64) even when written without a point (`SetQuestDelay Quest 3`). In any other
   parameter it is stored as an integer (`n` + i32). The parameter's type is the
   catalogue kind, except for seven functions whose stored type is the other one
   (`compiler.number_types`): DamageActorValue's amount, SetPackDuration,
   TriggerScreenSplatter, SetScreenSplatterFade and ShowCaravanMenu store floats; Rotate's
   speed and PlaceAtMe's count store integers. Every shipped (function, position) pair
   uses one type only. A number written with a point in an integer parameter is stored as
   a float [hypothesis: no shipped case].
5. **Local `ref` variables as parameters.** A local declared `ref` in a parameter is
   written as its reference-list entry (`r` + the index of its `SCRV`), not as the
   variable (`player.GetDistance rTarget`). Other locals are written as variables
   (`s`/`f` + index). 125 units depend on this.
6. **Globals.** A `GLOB` in an expression, a number parameter or a message's format
   variables is the global's value (`G` + index). In a form parameter it is the form
   (`r` + index: `GetGlobalValue SomeGlobal`).
7. **Skip counts.** The skip count of `If`/`ElseIf`/`Else` is the number of statements up
   to the chain's next marker (scripts.md §2.6). An `ElseIf` or `Else` with no open chain
   starts a chain of its own: its count runs to the next marker, and that marker closes
   it. A `ref` declaration inside a span counts as one statement (all six such units in
   scripts.md §2.6); a `short` declaration inside a span does not (FalloutNV SCPT
   0x0014B67B) [hypothesis: the declared type decides; seven cases].
8. **One form, two names.** A form written two ways, in practice `player` and
   `PlayerREF`, gets one reference-list entry per spelling (FalloutNV SCPT 0x00130C77:
   `Player.GetDistance` uses entry 45 and `PlayerREF.IsSpellTarget` entry 51, both the
   player). Which spelling holds which entry is not fixed by the source (§5).
9. **Message trailer.** `ShowMessage` stores one parameter (the message). Its other
   arguments are stored as format variables, and a trailing number becomes the duration
   (scripts.md §2.4).

## 4. Names (`env`, seeded and fresh)

A name is looked up as, in order: a local variable; `player` or `playerref` (the player,
0x14); an entry of the unit's own reference list, by Editor ID (seeded mode); the Editor ID
of a form the unit's plugin can see (the plugin and its masters, the last definition
wins); then an 8-digit hex FormID, stored file-local [observed: scripts.md §3.4]. A quoted
Editor ID names its form. In an enum position (actor values, animation groups) a name is a
spelling from encodings.json `enums`. `Head.var` is variable `var` of the script that
`Head` runs: a quest's script, or a reference's base form's script.

- **Seeded** (the default for `fnvtool script compile`, and what a running engine should
  use for a unit that has lists): the unit's own variable and reference lists start the
  compiler, so every index stays the same. This matters because other scripts' bytes
  name this script's variables by index (scripts.md §2.5). New names are added after the
  existing ones, with a warning.
- **Fresh** (`--fresh`): empty lists. Variables get indices 1, 2, … in declaration order,
  and references are added in order of first use. This mode tests name resolution
  through the load order.

## 5. Parity (`L5.script.compile.vanilla`, `fnvtool script compile`)

Result on 2026-10-25, base game + DLC + packs, the 14,511 units with both source and
bytes:

| Mode | Bytes equal | Canonical text equal | Reference lists | Variable lists |
|---|---|---|---|---|
| seeded | **14,489** | 14,500 | 14,510 equal | 14,506 equal |
| fresh | 10,741 | 14,500 | 10,891 equal, 14,162 same entries | 13,707 equal, 14,496 same entries |

In fresh mode the text matches as often as in seeded mode, so resolving names through the
load order works. The shipped lists are in a different order, or carry entries that the
source no longer uses. Both come from earlier compiles, which the source cannot show.

The 22 seeded units that differ are all cases where the shipped bytes cannot come from the
shipped source:

| Units | Why |
|---|---|
| 7 | A form was renamed after the script was compiled, so the source uses the old Editor ID (scripts.md §3.6): DeadMoney SCPT 0x0100636B, QUST 0x01013AA5 stage 21; HonestHearts SCPT 0x0200F8EF, 0x0200B667; FalloutNV REFR 0x001551B2, 0x001551B3 patrols; OldWorldBlues SCPT 0x0300D592. |
| 2 | Compiled against an older version of another script (scripts.md §2.5): FalloutNV SCPT 0x00127959, LonesomeRoad SCPT 0x0400AC93. |
| 8 | DeadMoney INFO result scripts (0x0101136F, 0x01011369, 0x01010D0B–0x01010D10, begin) write `s` (short) to `iL1`…`iL8`, which the target script now declares `float`. Compiled against an older version too. |
| 3 | `player` and `PlayerREF` take each other's entries compared with source order: FalloutNV SCPT 0x0015725D, DeadMoney SCPT 0x01010DD9, 0x0100E883 (§3 rule 8). |
| 1 | Opcode 0x127C where the source says GetInCharGen (scripts.md §2.6): OldWorldBlues SCPT 0x030154E8. |
| 1 | A fourth argument to AddItem after a quoted `"*"`, which the editor dropped (FalloutNV INFO 0x0014DDD7 end) [hypothesis: an argument that is not a value ends the list; one case]. |

Golden: `python tools/capture/oracle_script.py compiled <Data> <the ten masters> -o
<goldens>/script/compile.json` (private) holds the SHA-256 of every shipped unit's bytes.
The oracle never runs the compiler. Candidate: `fnvtool script compile … --digests`.

`fnvtool script compile` without `--digests` prints `fnv.script.compile/1`: `summary`
(the counts above), `text_differs` (kinds of the first differing line, with samples),
`bytes_differ_text_equal` and `errors`. With `--unit KEY` it adds both sides' text, bytes,
reference and variable lists, and the compiler's diagnostics.

## 6. Diagnostics

`Compiled::diags` holds each problem with its line, kind and whether it is an error.
Errors: `syntax`, `unknown_block`, `unknown_function`, `unresolved_name`,
`unresolved_remote_variable`, `remote_variable_on_local_ref`, `bad_set_target`,
`unknown_enum_name`, `bad_axis`, `end_without_begin`. Warnings: `line_too_long`,
`variable_added`, `reference_added`, `variable_type_differs`, `unmatched_marker`,
`stray_punctuation`. Which of these the editor really refuses is to be checked with the
GECK compile cases (extender-core-tasks.md G2) [hypothesis].

## 7. Running compiled source (task A6)

**At load time** (`fnv_game::source_compile`, called by `GameData::build`): the original
game runs compiled bytes only. A mod compiled with the script extender loaded has bytes
that call the extender's functions at opcodes the extender assigned. Our function table
has no such opcodes, so those bytes would not run. Rule: a unit whose bytes call an
opcode above the highest one in the function table, or that the decoder cannot read (the
extender's own statements have byte forms of their own [hypothesis]), and that has source,
runs from its source compiled by NVForever, seeded with its own lists [decision; no shipped unit calls
such an opcode, so the shipped game is unchanged in `auto`]. The rule agrees with the
documented opcode ranges: the base game's run to 0x13FF, the extenders' start at 0x1400
(xNVSE 0x1400–0x1FFF, JIP LN 0x2200–0x2FFF, JohnnyGuitar 0x3100–0x35FF [documented: NVSE
Opcode Base]), and the function table's highest opcode is below 0x1400. A unit whose source does not
compile keeps its bytes. Names the lists lacked are added after the existing entries. A
unit without bytes stays unrun, as in the original (the three never-compiled units of
scripts.md §3.6).

`[Extenders] CompileSource` in NVForever.ini, read by the engine before the script data is
built (`NVF_COMPILE_SOURCE` overrides it):

| Value | Units compiled from source |
|---|---|
| `auto` (default) | those calling an opcode the table does not have (above its highest) |
| `always` | every unit with source and bytes (a test of the compiler) |
| `never` | none (bytes only, as the original game) |

The engine logs `scripts: N units run from their source text, M kept their bytes`, with
samples of the ones that kept them. A game reads only the winning records, so a script
overridden by a later plugin is known only in its winning version. The compiler then
takes that version's variables when the unit's own plugin sees none (FalloutNV's
`VMQTopsScript` under DeadMoney's override).

Checked 2026-10-25 with `always` over the ten masters: 14,466 units run from source and 7
keep their bytes. Those 7 are the units of §5 whose source names forms renamed since, or
another script's variable that is gone. The headless intro walkthrough
(`fnvtool script run … --actions crates/fnv-game/walkthrough/intro.actions.json`)
gives a trace byte-identical to the one from the bytes. Smoke `scripts-from-source`
starts the game this way.

**What a mod's source may hold that nothing here knows** (2026-11-12). The catalogue is the
wiki's function pages, and mods call plugin functions that have no page. One such name
used to fail the whole unit, which then kept bytes NVForever cannot run [observed:
`fnvtool ext report` over The Mod Configuration Menu 1.5 and JustAssortedMods.esp 4.6, 8 of
63 picked units kept their bytes: MCM's main menu script, and JAM's sprint, bullet-time
and loot-menu handlers]. In the game's compile of a unit's source (`Seed::unknown_calls`;
the editor-parity and runtime compiles keep their errors) [decision]:

- **A function nothing knows** compiles, with a warning, as a call by its name with every
  argument that follows, wherever a call can stand: at a line's start, as an operand of
  `let`, `eval`, `if` or `set … to`, after a reference (`rRef.Name`), after a quest's ref
  variable (`Quest.rRef.Name`) or after an element (`aItem[0].Name`). A vanilla `if` or
  `set` holding such a name is read with the extender's grammar. Run, the call reports
  "not built", which skips the statement or reads 0, and the script goes on. As a
  function's argument an unknown bare word stays its own text, as before, except in
  parentheses with an operand right after it, where text would leave two operands side
  by side: there it is the call [observed: JIP Companions Command & Control 2.60,
  `SetFunctionValue (CCCInFaction <faction> == 0)`].
- **A variable with an extender function's name** (`short break`) is a warning there, not
  the editor's error: the unit shipped with compiled bytes, and a local is read before a
  function of its name [observed: Project Nevada - Core.esm 2.5, a stage's result script].
- **A form of the game itself that no plugin defines**, named by a script: `MapMarker` is
  0x00000010 (`compile::env::BUILT_IN`) [observed: CASM with MCM 1.8's compiled script
  lists that form where its source says `rRef.gbo == MapMarker`]. Until then the name
  compiled as an unknown function and the line was skipped every frame (256,575 skips in
  the owner's eight-minute run of 2026-11-12). Only forms read this way are listed.
- **An enumeration name the table lacks** (an animation group, an actor value, a
  statistic) compiles, with a warning, to code 65535, which no entry has. The table holds
  only the names seen in compiled scripts (scripts.md §2).

What a load order's units came to is kept (`GameData::source_units`): the game's log
names the functions not in NVForever (`scripts: N functions the plugins' scripts call are
not in NVForever … `), and `fnvtool ext report <Data> <plugins…> --schema schemas/fnv`
prints, per plugin, the units run from source, the units that kept their bytes with the
line and kind of their first error, and those functions with the number of units calling
each (fnv.ext.report/1; `--detail` adds the compiler's words, which may quote the mod's
script and so stay private). A plugin outside the Data folder is given by its full path.

For a played run, the test aid `NVF_PLAY_TEST_MOD_LOG=1` (`nvf_play::mod_log`) writes the
same per plugin into the game's log in lines that start with `mods:` [design]: the load
order; each mod's scripts (how many use the extender and run from source, which do not
compile, the functions and names not in NVForever); then, each once, what a mod's scripts
ask about plugins and read (IsPluginInstalled, GetPluginVersion, IsDLLLoaded, IsModLoaded,
GetModIndex: `fnv_game::plugin_questions`, kept only under the aid) and the first of each
failure in a mod's script; at exit the counts per mod. The lines quote no script text.

**At the console** (docs/design/console.md): a statement the console's own reader does
not take goes through the compiler and runs as a one-line result script
(`Game::run_source`), on the selected reference. That covers a `set` whose value is an
expression (`set TimeScale to 10 + 2 * 10`) and a call the reader refuses that contains
an operator or parenthesis. Smoke `console-compiled-source`.

## 8. The extender's grammar (tasks A7–A12)

Written from the wiki export's pages (NVSE Expressions, Let, Let Macro, Eval, Inline
Expression, Script Compiler Override), our notes in docs/spec-notes/ext-syntax.md. Code:
`compile/lex.rs` (`xtokens`), `compile/parse.rs` (which lines), `compile/gen/x.rs` (the
expression parser), `vm.rs` (`xeval`).

- **Extended units.** A unit that uses any of the extender's statements, declares a
  `string_var`/`array_var` or an initialised variable, calls a function that has no
  opcode, or has a compiler-override block is compiled with the extender's rules. In it,
  a call line holding parentheses or calling such a function is one extended expression
  run for what it does (`SetAV Health (GetAV Health + 50)` [documented: Inline
  Expression], `Ar_Append a 5`). Any other unit is vanilla and its parentheses are dropped
  as the editor drops them (§3), so the shipped units compile as before.
- **Declarations** (A10): `string_var` and `array_var` [documented: String Variable; Array
  Variable] are kept in the variable list as `VarType::Other("string_var"/"array_var")`
  [hypothesis on how the extender's editor stores them]. `int i = 5` declares and assigns
  [documented: xNVSE 6.1 inline declarations].
- **Literals and comments** (A9): `0x1F` and `0b101` are numbers [documented: Hexadecimal
  Notation; Binary Notation]. `/* … */` comments may span lines [documented: xNVSE 6.1.7].
  `expr.Function args` calls a function on a result or an element [documented: NVSE
  Expressions `.`].
- **User-defined functions** (A11): `Begin Function {a, b}` is a block of event code
  0x7F00, NVForever's own (`ir::FUNCTION_BLOCK`), whose parameter block lists the
  parameter variables [documented: User Defined Function: an object script holding only
  that block, up to 15 parameters]. `Call udf args…` takes the function and up to 15
  arguments [documented: Call]; a `ref` local holding a function is passed as the
  variable. Lambdas: `(begin function {params} … end)`, which may span lines, and
  `{params} => expr` [documented: Lambda], both an `XExpr::Lambda` whose parameters and
  variables are the enclosing script's; the block form's statements are compiled with the
  unit's own lists. Running them (calls, the 30-call nesting limit, SetFunctionValue) is
  B7/B8.
- **Compiler-override blocks** (A12): `Begin _GameMode` (any block name after `_`) is that
  block, and inside it `if`, `elseif` and `set` take extended expressions too, with the
  argument rule above [documented: Script Compiler Override].
- **Which lines.** `let …`; the Let macro, a name followed by `=`, `+=`, `-=` or `*=`
  [documented: Let Macro lists these four]; `if eval …` / `elseif eval …`; `while …`,
  `foreach <var> <- <source>`, `loop`, `break`, `continue` [documented: While; ForEach]. Every
  other line is read as vanilla, so the shipped units compile as before (§5, unchanged).
- **Tokens.** The operators of NVSE Expressions (`:=` and the compound assignments,
  `::` `->` `<<` `>>` `^` `|` `&` `!` `$` `#`), brackets, braces and commas, besides the
  vanilla ones.
- **Precedence** is the page's table (header of `compile/gen/x.rs`). `:=` groups right to
  left [documented; observed: the GECK with xNVSE compiles `a := b := 2` with both
  assignments last, docs/spec-notes/ext-bytecode.md] and `&&` binds tighter than `||`
  [observed: `iA || iB && iC` compiles with `&&` first, ext-bytecode.md]. The other binary
  operators group left to right and the compound assignments right to left [hypothesis;
  probes in extender-core-tasks.md G3].
- **Functions in an expression.** A function takes up to as many arguments as it has
  parameters (the catalogue's count, or the compiler's observed one, §3). An argument is
  an operand: `Player.GetActorValue Health + 50` is `(GetAV Health) + 50` [documented:
  Inline Expression; Let]. The arguments of an NVSE-aware function (the wiki's category,
  80 functions: Call, Print, SetFunctionValue, the ar_* family…) are whole expressions
  [documented: Script Compiler Override]. Inside a compiler-override block an argument
  takes arithmetic but stops at a comparison: `Abs -3 + 1` is 2 and `Sin 90 > 0.5` is
  `(Sin 90) > 0.5` [observed: probes expressions/08, 09 in xNVSE 6.4.9, against the page's
  `Sin 14 == 1` example; the bitwise operators inside, hypothesis]. Arguments are separated
  by a comma or by two operands meeting without an operator, and a `-` starts the first
  argument or one after a comma (`Abs -3`, `Rand -5, -1`) [hypothesis; probes in
  tools/capture/ext-probes/expressions]. A function with a format string takes all the
  arguments that follow (up to 20 values [documented: String Formatting]); the catalogue
  marks those as a parameter named or typed "Formatting"/"formatable". In a parameter that takes an actor value, an axis or a
  string, a lone name is that name. A function the catalogue lists without an opcode (the
  extender's own) is kept by its catalogue name (`FuncId::Name`). A local variable's name
  hides a function of the same name, as in vanilla.
- **Values** (VM `xeval`): numbers and references. `&&` and `||` evaluate left to right,
  stop when the left operand decides, and return the deciding operand's value [documented:
  NVSE Expressions; Eval]. Bitwise operators and shifts take the operands as integers
  [documented]. `^` is a power. An assignment stores the value and yields it [documented].
  Strings, arrays, `$` `#` `*` `&`, `[]`, `->`, `:` `::`, `foreach` and the extender's
  own functions stop the statement with `NotBuilt` until tasks B1–B4 and D build them.
- **Loops** (VM): `while` tests its condition before each round; `break` leaves the
  innermost loop, `continue` goes to its next round, through any `If` around them; each
  round counts against the run's step budget (100,000 statements), so a loop that never
  ends stops the script [decision; what the original does is to be observed, B4]. A
  `loop` without its `while`, `break` or `continue` outside a loop, and a block ending
  inside a loop are compile errors [hypothesis: as the editor refuses them, G2].

**NVForever's own byte forms.** The extender's own byte encodings are not known here
(extender-compat.md §5.2), and none is needed: the compiler writes the extended statements
in a form of NVForever's own, which only its decoder reads. These forms never go into a
plugin or a save:

| Form | Bytes |
|---|---|
| `let` statement | opcode 0x00F0, body = the expression tree as JSON (`ir::XExpr`) |
| extended condition (`if eval`) | the If/ElseIf condition holds one token: 0x20, 0xF0, u16 length, the tree as JSON |
| `while` / `foreach` | opcode 0x00F1 (body: the condition as JSON) / 0x00F2 (body: `ir::XForEach` as JSON) |
| `loop` / `break` / `continue` | opcodes 0x00F3 / 0x00F4 / 0x00F5, empty |
| extended expression statement | opcode 0x00F6, body: the expression as JSON |

The decoder nests a loop's statements between its head and its `loop` (`If` chains open
outside a loop stay open around it); a loop left open at `End` is closed there and
reported (`unterminated_loop`). All these opcodes lie between the vanilla statements
(0x0010–0x001E) and the functions (0x1000 on), inside the base game's documented range of
opcodes (0x0000–0x13FF [documented: NVSE Opcode Base, the owner's NVSE.xml export]), where
the shipped bytes use only the statements of scripts.md §2.2.

The If chain keeps its ordinary skip counts, so the decoder's nesting rules (scripts.md
§2.6) apply unchanged. A JSON body that does not parse is a Raw statement or token, and a
unit with one runs from source (§7).

### 8.1 What mods' sources write and the GECK with xNVSE took (#2275)

The units of the owner's 211-plugin Blue Moon list that the engine compiles from source
(`fnvtool ext report … --mo2-profile …`, smoke scenario `modlist-blue-moon`) failed 66
times; each failing unit ships compiled bytes the editor wrote from that source, so the
bytes say what the editor made of the line [observed, each rule; decompiled with `fnvtool
script decompile`]. "Mod source" below means the engine's compile (`Seed::unknown_calls`).

- `x := v` on a line of its own is a `let`, as `x = v` is (thirteen units).
- `Let r : = …`, the colon and the equals apart, is `:=`.
- A declaration takes any assignment: `float f += rand a, b` declares `f` alone and adds.
- `(begin function{} =>` ends its parameters at the brace; the arrow is no parameter.
- `({params} =>` with nothing after the arrow opens a lambda whose body is the next lines,
  closed by a line starting with `)`; the rest of that line goes on with the call.
- An `if`/`elseif` whose parentheses are open at the line's end takes the following lines
  that start with `||`, `&&` or `)` [hypothesis: only those].
- In mod source, a vanilla function's argument in parentheses is an inline expression
  (`RestoreAv Health (player.getav Survival)`), as in a unit of the extender's grammar.
- Inside parentheses, what is left before the `)` once the expression is whole and ends in
  a call, or from a comma on, is dropped: `(ar_size (GetActiveObjectives Q))` gives
  GetActiveObjectives no argument, `((GetFormDescription X), y)` the description alone.
- A call line followed by an operator (`DebugPrint "t= " + $f`) keeps the call; the rest
  of the line is dropped.
- In mod source, a `:` right after a vanilla call's argument ends its arguments
  (`player.say Hit:Ugh! || …` says `Hit`).
- In mod source, a vanilla parameter's name nothing has that is a short hex number names
  that FormID when the load order has it (`AddItem f n`: the caps).
- `GetIsAlignment` takes Good 0, Neutral 1, Evil 2, VeryGood 3, VeryEvil 4.
- `SetEventHandlerAlt` takes up to 16 filters and a priority, all pairs (`a::b`).
- A game setting's name given alone to a plugin function's string parameter is text.
- `SetStringSetting` is LU's two-parameter function; the catalogue's console row of that
  name (from another page) gave way to it.
- A plugin can hold two records under one FormID (oHUD.esm: QUST `aHUD` and MESG
  `iHUDMainMenu`, both 0x01000801). A name resolves through whichever of the winning
  plugin's records carries it, and a quest variable through the one with a script
  [observed: oHUD's shipped bytes write `0x…801.f19` where the source says
  `set aHUD.mcmVersion`].
- Inside an expression (an `if eval` or `let` line, another call's arguments,
  parentheses) an NVSE-aware function's argument stops at a comparison or logical
  operator, which then applies to the call's result: `if eval Ar_HasKey aConvo,
  "Conditions" == 1` compares the call with 1, `GetINIFloatOrDefault_Cached "S:k" "f.ini"
  == 1` likewise [observed: Party Banter 2.esm SCPT 0x0701D892 and mil_Add-On.esp SCPT
  0x07002549 ship such lines with compiled bytes]. Arithmetic stays in the argument and a
  range or pair (`1:2`, `"k"::v`) continues it [hypothesis, as in an override block].
  A call that is its own line, a parameter documented as a whole expression (`eval`,
  `TestExpr`, `Print`, `while`, `ToString`) and `let` keep whole expressions
  (`eval x && let s += "+"`, JustAssortedMods.esp). On Blue Moon this changes 14 units
  (LifeAfterDeath, Another Millenia, Party Banter, WeaponModMod, Immersive Fast Travel
  Encounters) and MCM Extender's Script Runner file, all toward the shipped reading.

The owner's 232-plugin High and Dry list (smoke scenario `modlist-high-dry`) added:

- A `&` after a comma boxes the argument: `Call Fn, &rRef, 0` passes the boxed reference
  [documented: NVSE Expressions, unary `&`; observed: Famine.esp's bytes hold the variable
  and the box operator].
- `{expr}` not followed by `=>` is the expression: `call Fn {(BuildRef 0 n)}` and `call Fn
  {Quest.var}` pass the value as one argument [observed: Animated Ingestibles.esp].
- Inside parentheses an operand that meets the whole expression without an operator is
  dropped with the rest before the `)`: `("…" + $sv ".nif")` is `"…" + $sv` [observed:
  MrShersh Functional Backpack.esp].
- `GetRefsInCell` takes a sixth argument, which its page does not list [observed: JAM Misc
  Tweaks.esp, `GetRefsInCell (Player.GPC) 28 2 0 2000 45` compiled with all six; what it
  does is not known, and the VM does not read it]. The VM reads the fifth, `maxDistance`,
  for GetRefs and GetRefsInCell: references farther than it are left out [documented:
  their pages], measured from the calling reference, else the player [hypothesis: as
  CallPerRef's distance, documented, ext-jg6.md]; it was ignored before.
- On a `set` line the editor drops parentheses (§3), so a call the `)` closes before it has
  all its arguments takes the ones after it: `set t to 1 + (GetVariable (…)) Quest` reads
  the quest's variable [observed: Animated Ingestibles.esp].

From the Begin Again list (Tale of Two Wastelands, 203 plugins, smoke scenario
`modlist-begin-again`, #2335):

- A quoted name right before a dot is the call's reference (`"E32A9".GetItemCount X`),
  and a quoted hex FormID of any length names that form when the load order has it
  [observed: Veronica Outfit Replacer.esp's bytes call both lines on 0x000E32A9; YUP's
  and YUPTTW's units that write quoted Editor IDs that way became byte-equal too;
  hypothesis: an Editor ID is tried first].
- `Ref.Set x to v` is a set of the script's own `x` with the reference as its
  calling-reference prefix, `1C 00 <ref> 15 00 …` [observed: Fallout3.esm's, TTW's and
  RAD.esp's MS14PowerArmorScript]. The decoder reads it as the set [hypothesis: the
  reference does nothing in the original].
- `MoveToMarker` is MoveTo, 0x109E [observed: TTW No Hardcore Sacrifice.esp's bytes].
- In an extender function's form parameter, a bare word or number of hex digits that
  names nothing is the form with that FormID, when the load order has one: `f` is the
  caps (0xF), `a` the bobby pin (0xA), `14` the player's reference [observed 2026-11-14,
  the stock game's runtime compiler: `AuxVarSetFlt "*_q" 7 0 f` read back 7 under
  Caps001; `g` and `FFFFFF` (no form) did not compile, nor did `let rX := f`; Armor
  Damage Fix's gr_CondDamFix.txt ships `… 0 f`]. Any other undeclared word there failed
  the whole script's compile in the original (`AuxVarSetRef "*_r" BobbyPin -1`; MCM
  Extender's gr_MCMExtender.txt line 76, and no plugin of the list has that Editor ID).
  NVForever keeps it as its own text, so only that statement is skipped (#2275).

Left with their bytes, as the source cannot be compiled to them: five result scripts whose
classic `if` has a surplus argument (`GetObjectiveDisplayed Q 30 1 && …`), which stop the
script in the original as in the VM (observer-checks.md check 32); and three units whose
source names forms renamed or replaced since their bytes were written. On High
and Dry the same five result scripts (shared with Blue Moon) and four more:
three whose source names references renamed since their bytes were written
(RG_PrimmPlayerHome.esm's `RGGeneratorPower` is `RGGeneratorPowerREF` in its own records,
ICE.esp's `rActorRef` is declared nowhere), and Leveled DLC Delay.esm, whose
`WeapNVDLC00Faderator` is a record each DLC master defines, which the list's "Fixed ESM's
Remastered" masters rename `WeapNVDLC01Faderator` and so on, so no master of this load
order has the name; its bytes name DeadMoney.esm's record by FormID.

### 8.2 NVSEScript, the extender's second language

xNVSE 6.3.6 added a second compiler for a C-like language [documented: NVSEScript; Lambda;
User Defined Function]. A unit whose first statement is `name X;` is in it. The compiler
(`compile/nvsescript.rs`) reads it and writes the same unit in the first grammar, which
§1–§8 build; each line it writes keeps the number of the source line it came from, so
diagnostics name the source's lines. What it reads, all [documented: NVSEScript] unless
tagged:

- `name X;` → `scn X`; declarations at the root → the script's variables (`string` →
  `string_var`, `array` → `array_var`, `double` → `float`). A value given there is dropped
  with a warning (`nvsescript_initializer_dropped`) [hypothesis: the second compiler runs
  it at the script's first run]. Every variable, in any block, is the script's.
- `fn (int a) { … }` at the root is the unit's user-defined function (`begin function
  {a}`) [documented: Lambda's `name OnKeyDownUDF;` example]; `GameMode { … }` and
  `OnEquip::Player { … }` are event blocks.
- Statements end in `;`. `x = e`, `x += e`, `x++` → `let`; any other expression → `eval`.
  `F(a, b)` → `(F a, b)`, each argument parenthesised when it is not one word or literal;
  a variable called (`rCallback(10)`) → `Call` [documented: Lambda].
- `if (c) {…} else if (c) {…} else {…}` → `if eval`/`elseif eval`/`else`/`endif`;
  `while (c)` → `while … loop`; `for (init; cond; inc)` → the init, a `while` on the
  condition, the body and the increment, and `continue` runs the increment first;
  `for (array a in e)` → `foreach`; `for ([T v] in e)` and `for ([T k, T v] in e)` take
  the iterator's `value`, or `key` and `value`, with `_` for one not wanted [documented:
  NVSEScript; ForEach]. One variable takes the value [documented for arrays; the page
  gives a map's key there; decision: the value until a map case is met].
- `match (e) { a -> {…} _ -> {…} }` → an `if eval (e == a)` chain [decision: the subject
  is read again for each arm].
- `return e;` → `SetFunctionValue e` and `return` [documented: User Defined Function].
- `[a, b]` → `Ar_List`, `{k::v}` → `Ar_Map` [documented: Array/Map literals]; `"…${e}…"`
  → a sum of the pieces with `$(e)`; a `;`, `"` or line break in a string is written as
  `AsciiToChar` of its code (a `;` would start a comment in the first grammar, §1).
  `
`, `	`, `\"` are the C escapes [hypothesis: the page shows none; the mod lists'
  scripts write `
`].
- `fn (params) { … }` and `fn (params) -> e` in an expression are lambdas [documented:
  Lambda]. `x++` inside an expression gives the new value [hypothesis].
- The variable list the second compiler writes holds its own iterators and renames block
  locals `__temp_<name>_<n>`, and types them 2 string, 3 array, 4 ref (the schema's SLSD
  enumeration) [observed: WeaponModCraftInfo.esp]. The reader takes those types as
  `string_var`, `array_var` and ref; compiled from source, a declared type wins over the
  seeded slot's, at the same index.

## 9. Values in the VM (tasks B1, B3, B7, B8)

`fnv_script::vm::XVal` is a value of the extended expressions: a number, a form, a string
(its text) or an array (by id, B3). docs/spec-notes/ext-values.md has the documented
rules; what NVForever decides where they are open is tagged here.

- **Storage.** A `string_var` slot holds the id of the variable's own string in the
  game's `ExtStore` (`Game::ext`); 0 is none [documented: a string_var or array_var holds
  a number underneath, Array Variable; for strings hypothesis]. Assigning a string copies
  its text into the variable's string [documented: NVSE Expressions]. A string keeps at
  most 16,384 characters [documented: String Variable]. Strings and arrays are kept in
  NVForever's save (`GameSnapshot::ext`, F1), as the original keeps them in its co-save
  [documented: ext-save.md]; NVForever never reads or writes the original's `.nvse`
  file. Lambdas are saved with their unit's key and the instance whose variables they
  share; one whose unit is gone after a load keeps its place and fails when called.
- **Conversions.** `$x` / ToString: a number's text (whole numbers without decimals,
  others with up to six [hypothesis]), a form's name (FULL; a reference's base form's),
  or its FormID in hex without one [documented: ToString]. `#s` / ToNumber: the number in
  the text, `0x…` as hex [documented: ToNumber], 0 when unreadable [hypothesis].
- **Operators.** `+` joins two strings [documented]. Strings compare without regard to
  case, `==`/`!=` included [documented for `<` `>` `<=` `>=`; `==` hypothesis]. `s[i]` is
  one character and `s[a:b]` the characters a to b, both included, counting from the end
  when negative [documented: NVSE Expressions, Slice; for `s[i]` hypothesis]. A string is
  true when not empty [hypothesis; ext-values.md §3 open]. A non-number deciding `&&` or
  `||` gives 1 or 0 [documented].
- **Errors.** A string where a number is needed (or the reverse) and division by zero are
  run-time errors [decision; NVSE Error lists such errors, division by zero
  documented]. A failing `let` or expression statement reports its error and the script
  goes on with the next line [documented: Let; NVSE Error]; the trace records it as a
  skipped piece. Strict runs (tests) stop at it.

- **Arrays** (B3; docs/spec-notes/ext-arrays.md has the documented rules). An
  `array_var` slot holds an array id; the arrays live in the `ExtStore` with a count of
  the variables and elements that hold each. Assigning an array shares it, and Ar_Null
  (0) lets go of one [documented: NVSE Expressions; Ar Null]. An array nothing holds is
  freed with what only it held [documented: xNVSE 6 counts references, ext-save.md], and
  the temporaries of a statement are freed when the statement ends. Lists keep keys 0…n−1;
  maps and string maps keep their keys sorted [documented: Ar First], string keys without
  regard to case [hypothesis]. `a[k]` reads an element, and a missing key is an error
  [documented: NVSE Error]. `let a[k] := v` and `let m->k := v` replace or add an element.
  A list takes a key up to its size, where the size appends [hypothesis], and a key
  beyond it is an error [documented]. `a[i:j]` is a copy of keys i to j of the same kind
  [documented; kind hypothesis]. `&x` and `*a` box and unbox [documented]. `==` compares
  top-level elements [documented]. `$` of an array is our own dump text, `kind #id [k: v,
  …]` [decision: the original's format is open].
- **foreach** over an array gives a string map {"key", "value"} per element; over a
  string, one character each [documented: ForEach]. The source is read element by element
  as the loop goes, so erasing during it skips elements, as documented. Form lists and
  containers: B5.
- **Array functions** (D2): Ar_Construct, Ar_List, Ar_Map (pairs `k::v`; keys mixing
  kinds or forms are an error), Ar_Null, Ar_BadNumericIndex (−99999), Ar_BadStringIndex,
  Ar_Size (−1 without an array), Ar_Packed, Ar_HasKey, Ar_Keys, Ar_First, Ar_Last,
  Ar_Next, Ar_Prev, Ar_Append, Ar_Insert, Ar_Erase (key, range or all), Ar_Resize,
  Ar_Copy, Ar_DeepCopy, Ar_Find, Ar_Sort, Ar_SortAlpha, Ar_Range, ar_Init, Ar_Unique,
  Ar_DeepEquals, Ar_Dump (a `ScriptPrint` trace line), TypeOf; each as its page says
  [documented: ext-arrays.md §5]. Ar_Find and Ar_Unique compare strings without regard to
  case [hypothesis]. A parameter whose catalogue name starts with `...` (`...entry19`)
  makes the list open-ended. Ar_ForEach, Ar_Filter, Ar_MapTo, ar_Any, ar_All, Ar_FindWhere
  and Ar_Generate run their function once per element, with one array per element (`*it`
  the value, `it["key"]` the key) [documented: Ar Filter; Ar MapTo; Ar ForEach; Ar Any;
  Ar All; Ar Generate]; ar_Any and ar_All stop at the first element that decides
  [hypothesis]. Ar_CustomSort: open.
- **User-defined functions** (B7, the game host `Exec::call_function`): `Call udf args`
  runs the function script's `Begin Function` block with variables of its own for this
  call, its parameters set from the arguments in order, on the calling reference of
  `ref.Call` [documented: Call; User Defined Function]. Its value is the last one
  SetFunctionValue set, which does not end the function, or 0 [documented:
  SetFunctionValue; 0 hypothesis]. Its strings and arrays go when it ends [documented: Ar
  Null], and an array it returns lives on as a temporary of the caller's statement.
  GetCallingScript gives the calling script, 0 from a result script [documented]. At most
  30 calls nest; a deeper one is refused [documented: User Defined Function].
- **Lambdas** (B8): evaluating a lambda gives a value that names it, kept by the game
  with the instance whose variables it shares [documented: Lambda: it reads and changes
  the parent's variables]; the same lambda of the same instance keeps its name: the
  same lambda is the same unit and the same lambda of it, numbered by the compiler,
  not where its expression happens to be in memory (#1088; a user-defined function
  runs from a copy of its body). One made again by a maker with no instance (a
  function or result script) keeps its name and reads the variables of the run that
  made it last [hypothesis]. The
  values start at 0xFE800000, in a `ref` slot [decision: a FormID that high would need a
  255th plugin]. `Call f args` runs it; the `{params} => expr` form returns its
  expression's value [documented: Lambda]. A lambda made by a result script has fresh
  variables of its own [hypothesis]. A loaded save's lambdas keep their names, and
  making one again after the load gives the saved name while the unit's lambda of that
  number is still the saved code [decision: a save from before the numbering reads
  every lambda as number 0, and an edited plugin can number other code the same; such
  a lambda gets a new name and runs the current code].

- **Formats** (C4; docs/spec-notes/ext-format.md): a format string's values are as many
  as its codes [documented]. `%X.Yf`, `%g` (an integer, 1,000,000 and up as `1E+006`),
  `%.Ne` (`1.230E+005`), the `+` ` ` `-` `0` switches, `%%`, `%a`, `%i`, `%n`, `%v`,
  `%x` with its width digit, `%z`, `%{ … %}` [documented: String Formatting]. `%g` rounds
  half away from zero [hypothesis]. `%B`/`%b` write nothing [decision: no colours in
  NVForever's console]. `%k` writes the scancode's number until the scancode table is
  in hand [hypothesis]; `%c` and `%po`/`%pp`/`%ps` take their values and write nothing
  yet. `%q`, `%r` and a bare `%e` work in every string literal [documented].
- **String functions** (D1; ext-strings.md's table): Sv_Construct, Sv_Length (−1 for no
  string), Sv_Destruct, Sv_Find (from a position, within a length, without regard to case
  unless asked; −1 when absent), Sv_Replace (`"old|new"`, in place, every match without
  regard to case by default; the number replaced), Sv_Split (by delimiter characters, or
  the whole delimiter with the flag), Sv_ToNumeric, Sv_GetChar, NumToHex/IntToHex,
  GetFormIDString [documented: each page].
- **Output** (D4 in part): PrintToConsole/PrintC (a format) and Print (an expression)
  print to the console [documented]; the game records them as `ScriptPrint` trace lines.
  MessageEx, MessageBoxEx, DebugPrint and SetDebugMode: not yet.
- **What the engine reports** (C5; ext-version.md): GetNVSEVersion 6, GetNVSERevision 4,
  GetNVSEBeta 0 [decision: the xNVSE 6.4 behaviour these tasks follow; how a version
  splits into the three numbers is open]. IsPluginInstalled gives 0 and GetPluginVersion
  −1 for every plugin until NVForever has that plugin's functions [documented: −1 when
  absent; decision]. The `[Extenders]` switch per module is C2's.

- **Events** (E1–E3; docs/spec-notes/ext-events.md). SetEventHandler (`"first"::`,
  `"second"::` filters, an array filter ignored), SetEventHandlerAlt (`0::` the calling
  reference, `1::`…`15::` the arguments; an array filter matches any of its elements),
  `"priority"::n` (default 1, higher first, ties in registration order, 0 and values past
  ±9999 refused), RemoveEventHandler (all of that handler for that event, or those
  matching the filters given), DispatchEvent (one string map: `eventName` and
  `eventSender`, the calling script's plugin, or the info map given with them added
  [hypothesis]), DispatchEventAlt (the arguments, a calling reference; a handler may take
  fewer parameters), GetCurrentEventName [documented: each page]. Filters match numbers
  equal, strings without regard to case, a form the same form, a reference also its base
  form, a form list any member [documented: New xNVSE Event Filtering System]. Handlers
  run at once, inside the line that dispatches [hypothesis]. They are not saved, as
  documented. Firing points so far: OnActivate first in every activation, even refused
  ones [documented: NVSE Event Handling], and OnDeath before the OnDeath block [order
  hypothesis]; LoadGame (the save's file name) and PostLoadGame (1) after a load, SaveGame
  (the save's name) before a save is taken [documented: NVSE Event Handling, session
  events]. A firing point costs nothing while no handler is registered.
- **Sessions.** NVForever starts the engine again to load a save, so every load begins a
  new session: SetEventHandler registrations end there, and GetGameRestarted is true
  again for every script, which is when mods register them [documented: SetEventHandler,
  "register again in a GetGameRestarted test"]. Within one run a game state taken over
  keeps the session's SetEventHandler registrations of function scripts
  (`Game::take_session`). GetGameLoaded is true once per script after each load,
  GetGameRestarted once per script per session [documented: each page].
- **Catalogue rows**: when several rows carry one name (the pages of events and examples
  that show a call), the row from the function's own page gives the parameters
  [decision].
- **Variable types from source**: a record's variable list can only say Integer or
  FloatOrRef [documented: xEdit SLSD]; the editor with xNVSE stores a `string_var` as
  Integer and an `array_var` as FloatOrRef [observed: ext-bytecode.md]. So a unit whose
  source declares either runs from its source (§7), and a seeded slot declared that way
  takes the declared type at the same index.
- **More of the editor's rules** [observed: ext-bytecode.md, the owner's GECK and game]:
  `!-1` is refused ("too many operators") and `-!0` compiles; `Return value` compiles in a
  function and is a plain Return: Call gives what SetFunctionValue set, not the value, and
  Return ends the function keeping it [observed: probe udf]; `1 && iX += 5` reads
  `(1 && iX) += 5` and fails, as the compiler here refuses it (an assignment to something
  that is not a variable).

- **Forms and types** (D5; ext-forms.md): GetObjectType/GetType (the form-type code of a
  base form, a reference's base form's; the codes of the Form Type IDs table),
  IsFormValid, IsReference, GetModIndex (255 when not loaded), GetSourceModIndex (the
  FormID's top byte, FormIDs being load-order FormIDs here) [documented: each page].
  Every form of the load order is in memory, so IsFormValid is true for any form that
  exists [hypothesis]; an armor addon reports 96, the table's ARMA code [documented: the
  table lists both ARMA 96 and "ARMO, armor addons too"; which an addon reports, open].
- **From the original game** [observed: the peer session's probe runs in xNVSE 6.4.9,
  goldens in C:nv-private\goldens\ext, which beat the wiki]: Ar_Sort keeps the
  elements of the first element's type (`[3, "x"]` → `[3]`); Ar_SortAlpha sorts numbers by
  value (9 before 10), ahead of the rest [hypothesis on the mix]; Sv_Length of a destructed
  or unset string is −1; Sv_Count counts matches that do not overlap; numbers pass to a
  function by value, strings as a copy, arrays shared; nested break/continue act on the
  inner loop. The two calls that crash the original get a safe answer here: Sv_Replace
  with an empty old text replaces nothing (0), and Ar_Range with a step of 0 is a run-time
  error the script survives [decision].

- **From the first probe lab run** (173 of 228 rows equal at e63e5edd; the peer's
  mismatch list) [observed unless tagged]: `$` of no form is `<NULL>` and of a form
  without a name `<no name>` (the page says its FormID); `$PlayerREF` is the name typed in
  the name menu; `#"12abc"` is 12; ToNumber's hex flag reads `"1F"` as 31; a string slice
  ending past the string stops at its end; Sv_Split drops empty pieces; Sv_Insert,
  Sv_Erase, Sv_SubString, sv_ToUpper/ToLower (the source kept), sv_Percentify and
  Sv_Compare (1 when the format text sorts after the variable's, −1 before, without regard
  to case unless the flag) [documented + observed]; the trigonometric functions take and
  give degrees, Rand gives a number from its minimum up to its maximum; Ar_Construct of an
  unknown kind makes a list; Ar_Map drops the keys of another kind than the first;
  Ar_Next/Ar_Prev of a key the array lacks give the bad index; Ar_Find takes a key range;
  Ar_InsertRange; Ar_CustomSort (its test says whether the first comes before the
  second); writing a list key at or past its size appends (the page calls past the size
  an error); `1 && iX += 5` compiles and fails when it runs.
- **Label and GoTo** (A8): `Label n` marks a place, `GoTo n` goes on after the label of
  that number in its statement list or one around it [documented: Label; GoTo; observed:
  probe loops/04 counts to 3]. NVForever's own opcodes 0x00F7/0x00F8, body n as i32.
- **A failing extended condition** (`if eval`, `while`) counts as false and the script
  goes on [observed: probe loops/03 reads past a shrunk list and finishes].
- **foreach over a form** (B5 in part): a form list gives its forms, a container or an
  actor one entry per kind of item [documented: ForEach; observed: as many as
  GetNumItems]; the entry here is the item's base form, where the original gives a
  temporary reference to the stack that stops being valid the next frame [hypothesis
  until B5 builds inventory references].

## 10. Open

- The editor's error messages and refusals (G2).
- Rule 7's declaration counting and rule 8's entry order (more cases would come from mods'
  scripts, recompiled in the editor).
- What the compiler does with a line it can't read in the middle of a block: ours writes
  nothing for that line and goes on.
