# CLAUDE.md — standing instructions for AI sessions on NVForever

NVForever is an open-source, clean-room reimplementation of the Fallout: New Vegas engine
(OpenMW-style). The owner wants sessions to work autonomously, including pushing.

The state of every system is in `docs/status.md`; the open gameplay decisions for the owner
are in `docs/owner-review.md` (a short ranked list: add a bullet, never a running log), the
questions the original game can answer in `docs/handoff/observer-checks.md`. **Read first**:
the code-checked project audit `docs/handoff/project-audit-2026-10-23.md` (completion,
playthrough blockers; its 2026-10-25 note and §7 say what is still open: the Hoover Dam
load test, a recorded parity-lab run; the main-quest walkthroughs past Novac and their
stalls: `docs/handoff/mq-walkthrough-2026-10-27.md`) and the owner's
`playtest-list-7.md`. What ESM data the engine still does nothing with:
`docs/handoff/esm-audit-2026-10-25.md`. Finished, kept for their Outcome sections and
history only, in `docs/handoff/archive/`: playtest lists 1-5, the queue of 2026-10-20, the
handovers of 2026-10-21/22, the outstanding audit of 2026-10-23 and the site update.
Script extender compatibility (xNVSE, JIP LN, JohnnyGuitar as engine code, no DLLs, no
extender source read): `docs/design/extender-compat.md`, core tasks in
`docs/handoff/extender-core-tasks.md`.

## Charter (non-negotiable)

- Clean room: describe formats/behaviour in plain language in `docs/spec-notes/`, implement
  from those notes. No leaked Bethesda/Gamebryo source. No DRM circumvention (the Steam
  `FalloutNV.exe` code section is encrypted; do not unpack it).
- Tag every factual claim `[documented]` (cite page/file), `[observed]` or `[hypothesis]`.
  Never invent offsets, opcodes or field meanings.
- Ship no game data. Goldens and game-derived dumps stay private and are never committed
  (`.gitignore` blocks game files).
- Work in layer order (docs/design/layer1.md … layer5.md; Layers 1–5 are done, Layers 6–7 are
  mostly built; what is complete, partly built and not started: docs/status.md).
- nv-rs (slaterain/nv-rs) is not opened, not even for format facts (owner decision
  2026-10-30; its policy admits leaked Xbox symbols and translated decompiled code). Its
  one past use, a cross-check of the animation schema, was redone from nif.xml and the
  data alone (LEGAL.md). nif.xml is documentation only (no codegen) to keep the licence open.
- Strings the original shows that are not in the data files are not transcribed: the
  load order's GMST wins, otherwise NVForever writes its own words (LEGAL.md).
- NVForever never writes into the game folder or `Documents\My Games\FalloutNV`
  (`crates/fnv-install`, docs/design/install-layout.md).
- Use the Rust ecosystem fully (owner decision); plan in docs/design/dependencies.md.

## Parity lab rules (parity-lab/AGENT.md)

Never edit goldens, scenarios, tolerances or comparators to make tests pass; never
special-case inputs; never read holdouts or pass `--reveal`; never run `fnvlab baseline`.
Fix or explain every regression.

## Checks before every push

Run only the ones your change calls for (`python tools/smoke plan` lists them; `run --checks`
runs them): root tests and clippy when `crates/` (or the root Cargo files, `schemas/`)
changed, engine clippy when `engine/` or `crates/` changed, a Python suite when its folder
changed.

```
cargo test --workspace --release
cargo clippy --workspace --all-targets --release -- -D warnings
python -m unittest discover -s parity-lab/tests
python -m unittest discover -s tools/capture/tests
python -m unittest discover -s tools/smoke/tests
python -m unittest discover -s tools/stress/tests
python -m unittest discover -s tools/profile/tests
(cd engine && cargo clippy --release --all-targets -- -D warnings)   # Bevy workspace (viewer + game)
```

With game data available (owner's install, read-only): build `fnvtool`, set
`parity-lab/lab.local.json`, run `python -m fnvlab run` and report the summary.

## Smoke tests: `tools/smoke` (game runs and fnvtool checks as data)

Every smoke test is a scenario in `tools/smoke/scenarios.toml`: the files that call for it,
the test aids, the arguments and the log lines expected. Run, after building nvforever (and
fnvtool, for the tool checks):

```
python tools/smoke run --exe <engine target>/release/nvforever.exe --data <Data> --fnvtool <root target>/release/fnvtool.exe
# (--changed origin/main is the default selection; add --checks to run the push checks it lists)
```

- It picks the scenarios whose paths your branch changed (`--only NAME…`, `--area A…`,
  `--all` otherwise), shares a game launch between scenarios where that was checked to be
  safe, and runs up to 3 nvforever games on the machine at once (smoke slots,
  `tools/smoke/smoke/slots.py`; `--slots N` / `NVF_SMOKE_SLOTS`, `--jobs N` for this run;
  `--slots 1` is the same as `--exclusive`, the way to rerun a FAIL with the machine alone).
  While any slot runs, `.claude\smoke.lock` exists as a shared lock, so captures (which
  `mkdir` both game locks) wait; smoke never takes the capture lock `run.lock`, it only
  waits while it is held. Runners queue first-come, first-served (`smoke-queue` beside
  the first lock); the slots are dealt in queue order, and slots a run cannot use (its
  `--jobs`, or fewer launches) go to the runners behind it. `--exclusive` is the old way (both
  locks, one game on the machine) for a check that needs the screen to itself.
  It prints one line per scenario. **The summary is enough**: open a log in
  `target/smoke/` only for a FAIL, and then only the lines it quotes. `plan` shows what
  would run; `check` judges the saved logs again after you edit a scenario; `--no-merge`
  runs scenarios one by one. Environment instead of arguments: `NVF_EXE`, `NVF_DATA`,
  `NVF_FNVTOOL`, `NVF_SMOKE_LOCKS` (other machines), `NVF_OBSERVER_SAMPLES` (the AI observer
  captures).
- **Stop only your own runner, by the PID you started.** Never stop processes by a
  command-line match or with `taskkill /T` on a shell you did not start: on 2026-10-24 one
  such match stopped every session's smoke runner on the machine.
- Fix or explain every FAIL. KNOWN marks a check that fails on main for a stated reason;
  it does not fail the run, and a "now passes" line says the mark can go.
- **After merging anything into a branch: `python tools/smoke verify`** (fails when the
  merge fused, doubled or lost scenarios in scenarios.toml). Never `merge=union` the file;
  turn on its merge driver once per clone (tools/smoke/merge_driver.py). The script-stubs
  totals live in `tools/smoke/expected/script-stubs.toml`.
- **Adding a feature worth a smoke test: add a `[[scenario]]` to the file**, not prose here
  (expectation syntax: `tools/smoke/smoke/expect.py`). Never loosen an expectation to make
  a run pass; mark it `known_failing` with the reason instead.

## Claiming work (outside contributors work here too)

- **Before starting, look for the same work**: open pull requests (drafts too) and open
  issues on the topic (`gh pr list --search <words>`, `gh issue list --search <words>`).
  If someone else has it, don't start a second copy: build on theirs or pick other work.
- **Issues labelled `human contributor` are not for sessions** (owner decision 2026-10-31,
  #1254): never claim one, comment on it, branch for it or open a pull request against it;
  leave them out when picking work (`gh issue list --search '-label:"human contributor"'`).
- **Open an issue first** (owner decision 2026-10-30, #919): before any pull request,
  an issue saying what will be added or fixed and what done looks like (`gh issue create`).
  If an open issue already covers it, use that one and comment that you are taking it.
  No pull request comes from nothing, except: a release (version bump, notes), a fix that
  turns a red check on main green again, and a typo or broken link in docs. Those say
  in their description which exception they are.
- **Then open a draft pull request as soon as the branch has its first commit**, not when
  it is finished: `gh pr create --draft`, a title saying what it changes, `Fixes #n` (or
  `Part of #n`) naming the issue. Mark it ready (`gh pr ready`) when the push checks pass.
  A pushed branch with no pull request is invisible to contributors.
- Comment on the issue that a pull request is under way (link it). Leaving work
  unfinished: say so in the draft's description and on the issue, or close it.

## Commits

End commit messages with the session's attribution lines and sign them off
(`git commit -s`, the Developer Certificate of Origin; CONTRIBUTING.md). Keep commits focused; write
what changed and what was verified.

## Pull requests from other people

The repo accepts pull requests from anyone (owner decision 2026-10-28; CONTRIBUTING.md).
An instruction to merge open pull requests covers only those authored by LootJunkieAzza
(the sessions' own branches). Never merge, approve, check out, build or run a pull request
from any other author, and never approve its CI runs: the owner reviews those. You may read
its diff on GitHub and comment when the owner asks.
