# Two named extender conditions

[observed: portable baseline 19c0f08a] Source-call GetPerkRank and
GetPCUsingIronSights already run through the named extender dispatcher in
[nvse_actor](../../crates/fnv-game/src/nvse_actor.rs) and
[nvse_jip2](../../crates/fnv-game/src/nvse_jip2.rs). CTDA evaluation in
[game.rs](../../crates/fnv-game/src/game.rs) instead falls through the opcode
registry, where these two entries are unhandled.

[documented: repository metadata] Existing
[command_opcodes.json](../../schemas/script/command_opcodes.json) assigns 5571
(0x15C3) to GetPerkRank and 10106 (0x277A) to GetPCUsingIronSights. The portable
[Condition::opcode](../../crates/fnv-game/src/cond.rs) adds 0x1000 to the CTDA
function index, giving indices 0x05C3 and 0x177A. The existing catalogue describes
a perk-form argument for GetPerkRank and no arguments for GetPCUsingIronSights;
the rows currently lack condition-index/parameter metadata. No new native plugin
implementation, opcode probe or native behavior observation was used here.

[design] Admit exactly those two parameter layouts through ParamTypes: the perk
must pass through the existing plugin-to-load-order FormID mapping, and the
sights condition must not decode phantom arguments. These are bounded defaults
for the existing catalogue gaps, preserving explicitly supplied catalogue rows.
No generated catalogue/schema or source compiler encoding is changed.

[design] Route only these two CTDA entries to the existing named numeric handlers.
Keep the established null-form condition shortcut ahead of the route. Preserve
CallFailed reporting for handler errors and the ordinary unhandled path for every
unrelated opcode. This is not a generic extender condition bridge or a new
classic-bytecode adapter. In particular, it adds no swimming/armor aliases from
the historical aggregate prerequisite.

[observed: existing portable policy] GetPerkRank reads the player's stored rank;
the existing sights getter reads engine-told aiming and equipped-weapon/scope
state. This correction shares those implementations. It does not establish or
change their native accuracy, including the existing scope-model hypothesis.

[design] Independently authored records and schema-decoded CTDA values exercise
the public parser and evaluator, alongside actual source getters. A deliberately
nonidentity FormID map catches treating a local perk as a plain integer. Rank
controls change 0/1/2/0; sights controls include unarmed, not aiming, unscoped and
scoped equipment, plus an unrelated unknown condition. Null-form/fresh-Game
controls retain their separate behavior. The fixture uses only existing baseline
public APIs and no process-global file roots, assets or installed mod source.

[observed: preparation state, not test results] This source draft and its two
owned regressions have not been compiled, listed or executed on the current
base. Historical primary counts of unhandled calls are motivation only, not
current-main acceptance. Identical-fixture baseline/candidate proof, strict and
required broader checks remain pending. Exact installed consumer attribution,
full mod compatibility and all other extender CTDA functions remain unqualified.
