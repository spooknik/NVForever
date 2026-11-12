# Graphics quality presets (#1966)

What the original's install holds for its quality presets, and how NVForever's settings
page reads them. Implemented in `engine/nvforever/src/quality.rs`; the page's layout in
`engine/nvf-play/src/settings_layout.rs` (docs/design/settings-page.md).

## The files

- The install carries four preset files in the game folder, beside `Fallout_default.ini`:
  `low.ini`, `medium.ini`, `high.ini` and `VeryHigh.ini` [documented: the install's files;
  also cited in impact-decals.md and world.md]. Each is a plain INI of `[Section]` headings
  and `key=value` lines.
- Between them they set **41 keys** in seven sections: `[Display]` 17, `[Water]` 11,
  `[TerrainManager]` 5, `[LOD]` 3, `[Grass]` 2, `[Imagespace]` 2, `[BlurShaderHDR]` 1
  [documented: the four files, read 2026-11-09]. Every file sets the same 41 keys.
- A key name's first letter gives its kind (`b` a switch 0/1, `i` a whole number, `f` a
  number) [hypothesis: by the names; it holds for all 41].
- What the original does with each number is not documented here, only what each name says
  [hypothesis]; the mapping below is NVForever's own [design].

**No value from the files is in NVForever.** The page reads them from the player's install
at run time, only reading them (`fnv_install::GameInstall::open` on `--data`, the root
folder, names matched without case). The unit tests and the smoke scenarios use made-up
files (`tools/smoke/fixtures/quality-presets*`) [design: LEGAL.md].

**Encoding** [design: #1966 review]: the install's files are plain 8-bit text with ASCII
keys and numbers [documented: the four files]. A UTF-8 byte order mark is dropped (it
would hide the first `[Display]` heading); a UTF-16 mark (FF FE or FE FF) decodes the file
as UTF-16; anything else is read as UTF-8, a byte that is not becoming a replacement
character, which no key or number holds. A preset left with no row (UTF-16 without its
mark, or not a preset file) is logged as setting no row.

## Which key sets which row [design: #1966]

Nine keys set a row. A key a file lacks leaves its row out of that preset.

| Section | Key | Row (`[Settings]` key) | How |
| --- | --- | --- | --- |
| `[Display]` | `fShadowLODStartFade` | Shadow distance (`shadow-distance`) | part, see below |
| `[Display]` | `bDrawShadows` | Sun shadows (`sun-shadows`) | on when not 0 |
| `[Display]` | `iShadowFilter` | Shadow filtering (`shadow-filter`) | 0 is Fast, any other number Smooth (the vanilla filter); Temporal is NVForever's own and no preset takes it |
| `[LOD]` | `fLODFadeOutMultActors` | Actor distance (`actor-distance`) | part |
| `[LOD]` | `fLODFadeOutMultObjects` | Object distance (`object-distance`) | part |
| `[LOD]` | `fLODFadeOutMultItems` | Item distance (`item-distance`) | part |
| `[Grass]` | `fGrassStartFadeDistance` | Grass distance (`grass-distance`) | part |
| `[Water]` | `bUseWaterReflections` | Mirrored water reflections (`water-mirror`) | on when not 0 |
| `[BlurShaderHDR]` | `bDoHighDynamicRange` | Light glow (HDR) (`glow`) | on when not 0 |

**A part** (the distance sliders are parts of their full distance, `draw_distance.rs`): the
file's number divided by the largest number the preset files found give that key, so the
fullest preset draws at 100% and the others in proportion; the row then brings it onto its
own range and steps (at least 10%, in 5% steps). The original's own distances are not
copied: the sliders keep NVForever's full distances (`--radius`, `--shadow-distance`,
`--grass-distance`). When only some files are found, the largest of those found is the 100%.
A key that is 0 in every file found sets nothing (no 100% to scale by) and the log says so.

The shadow distance row follows `fShadowLODStartFade` as the nearest counterpart: NVForever's
sun shadows have no other quality key, and the original casts no sun shadows from the world (the Sun shadows row's
note, `engine/nvforever/src/settings.rs`) [design].

## Keys not used [design: #1966]

No NVForever row counts for these, so a preset leaves them unused:

- `[Display] iMultiSample`, `iWaterMultisamples`, `bTransparencyMultisampling`: the
  multisampling. Left to the Anti-aliasing row until the maintainer decides (#1966's
  question: whether presets should set anti-aliasing and FSR).
- `[Display] iMaxAnisotropy`: the anisotropic filtering is one of the enhanced effects
  (16x on, 8x off), and the presets never touch those.
- `[Display] iMaxDecalsPerFrame`, `fDecalLOD1`, `fDecalLOD2`: the impact decals have no
  settings row (impact-decals.md).
- `[Display] iTexMipMapSkip`, `iTexMipMapMinimum`: no row sets a texture size.
- `[Display] iActorShadowCountInt`, `iActorShadowCountExt`, `iShadowMapResolution`: the
  sun's shadow map size is fixed (`graphics.rs` `SHADOW_MAP`); no row counts actors'
  shadows.
- `[Display] fSpecularLODStartFade`, `fLightLODStartFade`: no row fades specular light or
  lights by distance.
- `[TerrainManager] fTreeLoadDistance`, `fBlockLoadDistanceLow`, `fBlockLoadDistance`,
  `fSplitDistanceMult`, `fHighBlockLoadDistanceLow`: distant land and trees are one switch
  (Distant terrain and objects), not a distance.
- `[Grass] b30GrassVS`: no counterpart.
- `[Imagespace] bDoDepthOfField`, `iRadialBlurLevel`: no row.
- `[Water] bUseWaterRefractions`, `bUseWaterReflectionBlur`, `bUseWaterDisplacements`,
  `bUseWaterDepth`, `iWaterReflectWidth`, `iWaterReflectHeight`, `bReflectExplosions`,
  `iWaterBlurAmount`, `bAutoWaterSilhouetteReflections`, `bForceHighDetailReflections`: the
  water has only the mirrored and the screen-space reflections as rows.

A key that is in neither list is logged as not known and left unused.

## The Quality row [design: #1966]

- Under its own heading, Graphics > Quality, the first row of the Graphics tab:
  `Low`, `Medium`, `High`, `Very High` (one per file found) and `Custom`. The names are our
  own words: the load order has no game setting for them [documented: FalloutNV.esm holds
  no "Very High" string, searched 2026-11-09].
- Taking a preset sets each row it maps through that row's own setter, as a change on the
  page would (each logged `settings: <key> <value>`, kept in the INI).
- The row **reads** the first preset whose values every mapped row (of those registered)
  holds, else Custom. Custom is what the row reads, not a choice to take: the list offers it
  only while it is read. Any change to a mapped row shows Custom; moving it back shows the
  preset again.
- A missing or unreadable file leaves its preset out of the list and says so once in the
  log (`quality presets: Medium is not offered: medium.ini not found in …`). With none of
  the four, the row is not registered at all (`… the Quality row is not shown`).
- Kept as `quality=<name>` in `[Settings]` of `NVForever.ini`. The stored name sets
  nothing at the start: the rows are kept on their own and are the truth, so a test run's
  command line still wins over the INI row by row (#1382).
- Defaults unchanged: nothing changes until the player takes a preset. With the owner's
  install, today's defaults (sun shadows on, Smooth filtering, light glow and mirrored water
  on, every distance at 100%) read as `Very High`.
- It never touches the enhanced effects, the anti-aliasing or FSR.

## The Enhanced effects row [design: #1966]

Under Graphics > Picture, its first row: `On` sets every switch `--graphics enhanced` turns
on and `vanilla` leaves off (ambient occlusion, longer sun shadows, bloom, sharpening,
anisotropic filtering 16x, sun shafts, screen-space water reflections; taken from
`graphics::Graphics::preset`, so a new effect joins by itself), `Off` clears them all, and
it reads `Custom` when they are mixed (then offered as the one chosen). The effects keep
their own rows. The light glow, eye adaptation, neon glow and mirrored water are on in both
looks, so they are not effects of it, and the two rows never set the same row. Kept as
`enhanced=On|Off|Custom`. As the Quality row's, the stored name sets nothing at the start:
each effect is kept on its own and read with its own command-line precedence (#1382), so
a test run given `--shadow-distance` keeps Longer sun shadows as the command line set it
even with `enhanced=On` stored [design: #1966 review]. The note names no command-line
switch, so the INI read never reports one overridden.

## Test aid and log

- `NVF_PLAY_TEST_QUALITY_PRESETS=<folder>` reads the preset files from that folder instead
  of the game folder.
- `quality presets: Low, Medium, High, Very High from <folder>; 9 key(s) set rows, 32 not
  used` at the start; `quality presets: <name> from <folder>: <n> row(s) set` when one is
  taken; `quality presets: the Quality row reads <name>` and `… the Enhanced effects row
  reads <name>` at the start and whenever it changes while the pause menu is open.
- Smoke: `settings-quality-presets`, `settings-quality-presets-missing`,
  `settings-quality-presets-part`, `settings-enhanced-effects`.

## Main-menu edit measurement (#2248)

[design, #2248: engine/nvforever/src/quality_probe.rs;
engine/nvf-play/src/settings.rs] While the main menu is open, each effective
Quality choice or mapped row edit logs `quality measure begin:` and then one
`quality measure:` line. A preset's nested row setters are one measurement. No-op
sliders and already chosen list entries start no measurement. The window follows
the setter for `FOLLOW_SECS` (three real seconds), retaining any frame crossing
its deadline; overlapping windows remain independent. No renderer or gameplay
default is changed by this diagnostic.

[design: quality_probe.rs `watching`] Asset and entity sampling runs only while
the main menu is open or an edit's measurement is still active.

[design: quality_probe.rs] `longest_frame_ms` is the largest wall-clock interval
between main-frame starts, including the entire frame containing the edit. It
includes scheduling and renderer backpressure, rather than only CPU execution or
GPU time. `setter_ms` times the outer setter and its persistence. `elapsed_secs`
includes that setter and the follow-up; `frames` counts sampled main intervals.
`main_menu=true` means the menu remained open at every sampled interval; it does
not prove that the black UI cover was drawn on the GPU.

[design: quality_probe.rs, world.rs, graphics.rs, draw_distance.rs, pipeline_gate.rs]
The other fields describe work during the interval, including unrelated work:

- `cell_starts` and `reference_attempts`: cumulative calls to cell construction
  (`begin_cell`) and reference spawning (`spawn_ref`), including attempts which
  produce no visual. These cover new construction as well as repeated work; they
  are not a count of disk reads. Startup construction before an edit is excluded.
- `world_roots` and `visual_parts`: added world-root and RefVisual components;
  multiple parts can belong to the same reference. Deferred additions are counted
  at the end of the frame, rather than at the original request.
- `camera_spawns`, `shadow_updates`: added camera components (including secondary
  views) and calls to the sun-shadow setter (`sky.rs`), even without a sun entity.
- `graphics_applies`, `range_passes`, `range_meshes`: graphics application passes
  past the unchanged-state guard, whole-scene distance passes, and meshes visited
  by those distance passes.
- `pipelines_asked`, `pipelines_pending_peak`, `render_frames`: new pipeline
  requests, the largest pending count sampled by the main world, and completed
  render frames. A compile that starts and ends between samples may not appear in
  the peak. The existing `NVF_PLAY_TEST_PIPELINE_LOG=1` aid adds pipeline labels
  and request-to-ready times; neither aid times individual driver shader compiles.
- `mesh_events`, `image_events`: `[added, modified, removed]` asset events, sampled
  at frame end. Images include menu slides and UI assets; these are not necessarily
  world reloads. Events delayed until frame end can include earlier frame activity.
- `window_changes`: sampled changes to a window's identity, physical size, scale,
  mode or presentation mode. This does not count backend swapchain recreations.
- `ini_write_attempts`: settings-file write attempts, including failed writes;
  nested row writes and the enclosing preset write all count.

[design, #2248: tools/smoke/scenarios.toml] `quality-main-menu-measure` opens
Settings from the main menu, takes all four made-up preset fixtures, and then
edits each of the nine mapped rows alone, eight seconds apart. Stage 1 requires
the thirteen summary lines, with no frame-time limits or zero-load expectations.
Run it on Windows and Linux before choosing a fix. [hypothesis, #2248] Pipeline
compilation or presentation behavior may differ by GPU, driver and window backend;
source inspection alone does not establish the reported Linux hitch or the cover
becoming invisible. The repeated synchronous INI writes are present in the shared
setters [design: settings.rs `take`, `flip`, `set_slider`, `store`]; their share of
the hitch still needs measurement. Stage 2 remains deferred until the cause is
clear. No observer check or gameplay choice is added.
