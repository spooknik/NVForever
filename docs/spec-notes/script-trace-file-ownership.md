# Script trace ownership and sequential reuse

The documented `NVF_PLAY_SCRIPT_TRACE=<path>` contract writes trace events to that
path (script.rs and docs/design/layer5-play.md). Maintainer review of6507c5f,
PR2383 comment6875026800, identifies that treating every existing file as a live
writer violates this contract and leaves readers looking at a finished older run.

[documented] Rust1.89 introduced [File::try_lock](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock)
and [TryLockError](https://doc.rust-lang.org/std/fs/enum.TryLockError.html).
`WouldBlock` means another handle/process holds the lock. The lock remains owned
by the file handle; closing it releases ownership. Platform locking behavior differs;
this code does not promise live unlocked readers on Windows.

[design] Open the requested file without truncating, acquire its exclusive OS lock,
then truncate and write the header. Keep the locked File inside BufWriter through
the writer's entire lifetime. Only WouldBlock selects a PID/attempt sibling; other
open, lock or truncate errors remain visible. Finished requested files are reused
at the exact path rather than accumulating suffixes. Do not delete an old pathname
as a cleanup trick: it could be another writer's identity. Retain the64-name bound,
format/processId header, immediate header flush and actual-path log.

The authored controls cover overlapping same-process handles, a real independently
opened child-process writer while the requested file is locked, and sequential
reuse of a stale requested path. All content/JSONL/no-NUL/separation assertions
remain; completed files are inspected after close because Windows locks may exclude
other read handles. The child test is isolated through its own exact test entrypoint.
At initial source preparation these controls were authored; subsequent source-bound
execution results are recorded in the PR.

Earlier private trace-corruption evidence and the original overlapping-writer test
are historical regression context, not original-game behavior observations or proof
of this revision. No real game/save-load handoff, native parity, full-mod, FPS or
crash repair is claimed. A creation/header failure may leave a partial file, and
uncooperative writers that ignore OS locking remain outside the ownership protocol.
