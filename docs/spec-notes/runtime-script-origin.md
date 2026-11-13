# Runtime script origin and private temporary AuxVar ownership

[observed] The owned native `native_runtime_scope_37` job executes two public-interface
reads of player private temporary AuxVar `*NVFRuntimeScope`: ScriptRunner to a separately
compiled query, then ScriptRunner to a CompileScript UDF. Both return17. The capture has
its end marker (SHA256 `895371af0fbb1eb3751b3a60890190bad8bd6524b9359900558feebd109ac77d`).
The native process faults on shutdown after completed results; this is no stability
acceptance. No native extender implementation was read.

[observed: baseline03a41455 portable source] `Game::compile_statements` gives ScriptRunner units
the diagnostic plugin string `ScriptRunner\<path>` and `UnitOwner::Other` signature `SR`.
CompileScript gives a unit the diagnostic string `CompileScript`. On that baseline,
`Exec::calling_plugin` treats both diagnostic strings as load-order plugins. Query and console units
already have no owning plugin. Private JIP stores use the returned plugin as a namespace.

[hypothesis: minimal implementation] Treat query, console, CompileScript and SR units
as no-plugin runtime origins, preserving real ESM/ESP unit plugin identities in lowercase.
This aligns runtime private temporary AuxVar access across file/compilation boundaries;
it neither changes per-reference ownership nor removes real plugin-private separation.
The change does not rename diagnostic units or alter script-cache paths.

[hypothesis: authored regression controls] Use actual `Game::run_script_runner`,
`Game::run_query`, `Game::run_source` and public CompileScript/Call/AuxVar commands on
synthetic records and owned text files. Keep the exact fixture and explicit standalone
test registration unchanged on the pinned baseline and candidate. Assert actual result
values, successful ScriptRunner execution and no traced script errors. Real owned ESM
and ESP function units must still keep private values separate while sharing underscore
names. Those additional controls protect portable behavior; the native two-row result
alone does not establish every private/shared real-plugin contract.

[observed: test infrastructure] ROOTS is process-global. The new `runtime_origin`
standalone test target owns one immutable synthetic file tree initialized once; tests
may run in parallel because they use the same roots and separate Game stores. It is
not a module in the combined integration target. Fixture paths are task TEMP paths,
not a game folder. A focused fixture pass is still separate from whole-target/full-suite
and strict lint checks.

[hypothesis: scope limit] This prerequisite is ordinary JIP AuxVar runtime ownership
only. It does not prove ShowOff AuxStringMap private temporary behavior, the B42 Loot or
ItemCards consumers, map persistence, real native plugin-private semantics or a complete
save/reload contract. Those need their own public-interface controls and receipts.
