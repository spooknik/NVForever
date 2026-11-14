# JIP handler failure context

## Scope and provenance

- [documented: `crates/fnv-game/src/jip_events.rs`, `jip_run`] NVForever keeps a
  callable value, registration sequence and filters for each JIP event registration.
  Its dispatch attempts the selected handlers newest first and reports failures
  through `Game::handler_failed`.
- [documented: `crates/fnv-game/src/game.rs`, `handler_failed`; issue
  [2280](https://github.com/LootJunkieAzza/NVForever/issues/2280), merged PR
  [2285](https://github.com/LootJunkieAzza/NVForever/pull/2285)] Failures saying that
  a callable is not a function script or lacks a Function block are reported once
  per original handler kind and error. Other errors retain their existing reporting
  frequency. This change preserves that policy rather than extending it.
- [documented: `crates/fnv-game/src/game.rs`, `run_function_here`] An unknown lambda
  currently reports its error without the attempted callable value. A report alone
  therefore cannot identify which registration failed.
- [hypothesis] A missing lambda could have several causes. Neither this diagnostic
  nor the closed historical identity issue
  [1088](https://github.com/LootJunkieAzza/NVForever/issues/1088) establishes one.
  No native extender implementation or game executable was inspected.

## NVForever diagnostic policy

- [documented: this implementation policy] When a JIP event failure is actually
  reported, also emit a structured tracing warning containing its frame, original
  unit/error and a bounded handler context. The ordinary `ScriptError` event's
  unit/error remain unchanged; no trace schema changes are required.
- [documented: this implementation policy] The context names the callable's value
  kind and raw form/array handle or number, registration sequence, receiver, argument
  count, total filter count and at most four filter values. It does not dereference
  arrays or inspect script-owned object contents. Strings keep at most 32 Unicode
  scalar values, escaped for controls, with an explicit truncation marker.
- [documented: this implementation policy] Deduplication uses the original unit and
  error before the diagnostic context is constructed or logged. A lazy callback is
  invoked only after that gate admits a report. Different diagnostic context must not make a
  previously suppressed unusable-function error repeat. No failed registration is
  removed or repaired; attempts, ordering, successful-neighbor execution and return
  counts remain unchanged.

## Authored controls

- [documented: `tests/jip_handler_context.rs`] Synthetic dispatch controls use two
  invalid callable IDs to require distinguishable warning context, a live neighboring
  lambda to require continued dispatch, repeated unusable-function registrations to
  preserve the original deduplication, and oversized Unicode/control-character
  values to exercise bounded diagnostic formatting through the actual failure path.
  These fixtures contain no game data. Execution results are tracked separately;
  the presence of a fixture is not a passing result or a native-parity claim.
- [documented: `src/handler_diagnostics_tests.rs`] A Game/helper control counts actual
  context-callback invocations. Repeated unusable errors with changing prospective
  context must not invoke the callback again; a different handler kind and ordinary
  repeated errors still invoke it when their original reporting policy admits them.
