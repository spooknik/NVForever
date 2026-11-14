# Worker texture load coordination

[observed: baseline19c0f08a portable source] worker_texture checks success/failure caches before decoding, but workers insert the shared handle only after their independent decodes complete. Simultaneous requests for one resolved path and color space can duplicate reads and decoded image allocations. take_workers drains failures while worker snapshots hold successes only, so stale snapshots can retry a failed texture.

[hypothesis: engineering design] Acquire a per-key producer gate, then recheck successful and failed results before decoding/reserving a handle. Do not hold the gate-directory mutex during I/O or while waiting for a producer. Distinct keys remain independent. Weak gate entries permit idle metadata pruning at256 entries while retaining every active key; this is a pruning threshold, not a bound on concurrent active keys. Failed-key tombstones persist after their first diagnostic is collected.

[observed: this source candidate] Five independently authored regressions cover8 contending worker requests sharing one actual decoded/uploaded handle, different-key progress during a held producer, retained failure memoization, distinct color-space handles/formats and active-key-safe pruning. DDS data is generated in the test; no installed texture or game asset is included. The tests use bounded channels/deadlines to expose serialization mistakes.

[observed: earlier local source, separate identity] Those tests and selected texture tests passed in the original repair branch. That is historical proof only. This19c0f08a port adapts the changed DDS import context and preserves current-main alpha-map tests; no current-port compilation, test, strict lint or rendered acceptance has run yet.

[hypothesis: limits] This does not deduplicate simultaneous main-thread and worker loads. Failed keys keep their existing session failure lifetime rather than retry automatically. It does not change DDS decoding or duplicate merged DDS fixes. Total process memory, rendered FPS/stalls and all-mod stability remain unmeasured; no improvement amount is claimed. Current-port focused/broader checks and renderer qualification remain pending.
