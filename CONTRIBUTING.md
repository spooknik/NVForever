# Contributing to NVForever

Thanks for helping. NVForever is a clean-room reimplementation of the *Fallout: New Vegas*
engine, so a few rules here are stricter than in most projects. Please read
**[Clean-room rules](#clean-room-rules)** before you write any code. A pull request that
breaks them can't be merged, however good it is.

## Ways to help

- **Report bugs and differences from the original** with the
  [issue forms](https://github.com/LootJunkieAzza/NVForever/issues/new/choose). Never
  attach game files. Crash logs from `Documents\My Games\NVForever\logs` are fine.
  Report security problems privately instead: see [SECURITY.md](SECURITY.md).
- **Fix something or build something.** [docs/status.md](docs/status.md) says what is
  built and [docs/outstanding.md](docs/outstanding.md) says what is left. Every pull
  request starts from an issue: open one first (or use an open one) saying what you will
  add or fix, so we can agree on the approach before you spend time on it. Only a typo or
  broken link in the docs can go straight to a pull request.
- **Document a format or a behaviour** in [docs/spec-notes/](docs/spec-notes/). Notes are
  worth contributing even without code, because they are what the code is written from.

## Clean-room rules

These come from [LEGAL.md](LEGAL.md), which has the full reasoning.

1. **Never use leaked or decompiled code.** No leaked Bethesda/Gamebryo source or debug
   symbols from any platform. No disassembly or decompilation of the game, the G.E.C.K.
   or the engine. If you have worked from such material for *Fallout* or Gamebryo, please
   don't contribute code for the same systems.
2. **Don't circumvent DRM.** The Steam `FalloutNV.exe` code section is encrypted. It is
   not unpacked.
3. **Write it down first.** Formats and behaviour are described in plain language in
   `docs/spec-notes/`, and then the code is written from those notes. Tag every factual
   claim in a note:
   - `[documented]`: from a public source. Cite the page or file.
   - `[observed]`: measured on your own install or in the running original game. Say how.
   - `[hypothesis]`: not yet verified.

   Never invent offsets, opcodes or field meanings. A `[hypothesis]` tag is fine. A guess
   without one is not.
4. **Copy only from code whose provenance is clean.** Some projects are fine to read as
   documentation but not to copy from:
   - **nv-rs**: don't open it, not even for format facts. Its own policy admits leaked
     debug symbols and translated decompiled code.
   - **niftools `nif.xml`** and **xEdit's definitions**: read them as documentation. Don't
     generate code from them.
5. **Don't transcribe the original's text.** Words the game shows that are not in its
   data files (built-in labels, a terminal's header) are not copied from the screen or
   the program. Read them from the load order when it has them; otherwise write
   NVForever's own wording.
6. **Commit no game data.** That means no `.esm`/`.esp`/`.bsa`, meshes, textures, audio,
   saves or executables, and no screenshots or frames rendered from game data. Goldens,
   dumps and captures made from your copy stay in a private folder outside the
   repository. CI checks this (`game-data-guard`), but don't rely on it. It refuses
   every image except the app's own icons and the vendored Bevy files; an image of
   NVForever's own is added to its allow-list by path, in the pull request that adds it.
7. **Don't write into the game.** The NVForever program never writes into the game folder
   or `Documents\My Games\FalloutNV`. See
   [docs/design/install-layout.md](docs/design/install-layout.md).

### The parity lab

The [parity lab](parity-lab/README.md) scores the engine against the original game. Its
rules ([parity-lab/AGENT.md](parity-lab/AGENT.md)) apply to people too:

- Never edit goldens, scenarios, tolerances or comparators to make a test pass. If one
  looks wrong, open an issue with the evidence.
- Never special-case inputs, such as hard-coded FormIDs or expected values.
- Never read holdouts, pass `--reveal` or run `fnvlab baseline`.

## Before you start: check nobody else has it

Several people and the project's own AI sessions work on NVForever at once. To avoid two
people building the same thing:

- **Search the open [pull requests](https://github.com/LootJunkieAzza/NVForever/pulls)
  (drafts included) and [issues](https://github.com/LootJunkieAzza/NVForever/issues)**
  for your topic. A draft pull request means the work is under way. An issue labelled
  `in progress` has been claimed.
- **Claim it.** Comment on the issue that you are taking it (or open one first, if there
  is none). The maintainer labels it `in progress`. Issues labelled `help wanted` or
  `good first issue` are free to take.
- **Open your pull request early, as a draft**, once you have a first commit, and mark it
  ready for review when it is done. If you stop, say so on the issue so someone else can
  pick it up.

## Making a change

1. **Fork** the repository and create a branch from `main`.
2. **Build and test.** Setup is in [docs/development.md](docs/development.md). You need
   Rust stable and Python 3.11+. Run the checks your change calls for:

   ```bash
   cargo test --workspace --release
   cargo clippy --workspace --all-targets --release -- -D warnings
   (cd engine && cargo clippy --release --all-targets -- -D warnings)
   python -m unittest discover -s parity-lab/tests      # and tools/capture, tools/smoke, tools/stress
   ```

   `python tools/smoke plan` lists the checks and smoke scenarios that your changed files
   call for.
3. **With the game installed** (optional, but it helps): run the smoke scenarios
   (`python tools/smoke run …`) and the parity lab (`python -m fnvlab run`), and report
   the summaries in the pull request. You need your own copy of the game for these. If
   you don't have one, say so in the pull request and the maintainer will run them.

   **Don't send player reports from development builds.** NVForever can send opt-in
   reports to the project's server (docs/design/player-reports.md); they are meant to
   show how *players'* games run. A build of yours, a half-finished change or a test
   session would mix into those numbers (crash rates, performance, which quests players
   can finish). Runs with `--data` / `NVFOREVER_DATA` and every smoke test never ask and
   never send. If you start a build of your own like a player would, answer **Not now**
   to the "Help fix NVForever?" popup, or set `NVF_REPORTS=0` in your environment, or
   `Send=0` under `[Reports]` in your `NVForever.ini`. To work on reports themselves, run
   the server locally (LootJunkieAzza/NVForever-Data, `npm run dev:ingest`) and use
   `NVF_REPORTS=1 NVF_REPORTS_ENDPOINT=http://127.0.0.1:8787`; a test run never sends to
   the project's server without its own `NVF_REPORTS_ENDPOINT`. A build you make yourself
   never sends to the project's server anyway: only builds from the release workflow do.
4. **Open a pull request** against `main` and fill in the template. CI runs the build and
   tests on Linux. First-time contributors' CI runs wait for a maintainer's approval.

### What a good pull request looks like

- **One topic per pull request.** A fix and an unrelated clean-up go in separate pull
  requests.
- **Tests.** A new behaviour comes with a test. A user-visible feature that needs the
  game to check comes with a `[[scenario]]` in `tools/smoke/scenarios.toml`. Never loosen
  an expectation to make a run pass. Mark it `known_failing` with the reason instead.
- **Follow the surrounding code**: naming, comment density and idiom. Code must pass
  clippy with no warnings.
- **Dependencies.** A new crate needs an entry in
  [docs/design/dependencies.md](docs/design/dependencies.md), and its licence must pass
  `cargo deny` ([deny.toml](deny.toml)).
- **Docs.** Update `docs/status.md` when you finish or start a system.
- **Commit messages** say what changed and how it was checked.

### AI-assisted contributions

Much of NVForever is written with AI coding assistants, so AI-assisted contributions are
welcome. You are still responsible for every line. Check that the result follows the
clean-room rules, and never give an assistant leaked or decompiled code as input.
[CLAUDE.md](CLAUDE.md) holds the standing instructions the project's own sessions follow,
and it is a good brief for any assistant.

## Licence

NVForever is licensed under **GPL-3.0-or-later** ([LICENSE](LICENSE)). By opening a pull
request you agree that your contribution is licensed under the same terms, that it is
your own work or you have the right to submit it, and that it follows the clean-room
rules above.

Sign off every commit (`git commit -s`). The `Signed-off-by:` line certifies the
[Developer Certificate of Origin](https://developercertificate.org/): that you wrote the
change or otherwise have the right to submit it under the project's licence.

The capture plugin (`tools/capture/nvf_capture`) carries an extra permission under
section 7 of the GPL that lets it be loaded into the game and xNVSE
([its README](tools/capture/nvf_capture/README.md#source-building-and-licence)). Contributions to it are
made under the same permission.

## Conduct

Be kind and assume good faith. Criticise code, not people. The maintainer may close
issues or pull requests, or block accounts, that are abusive or that ask for or share
game files or leaked material.
