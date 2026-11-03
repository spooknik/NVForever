# Enhanced graphics: engine-level improvements (workstream L)

Status: first set built 2026-10-12 (branch `claude/graphics-engine`, PR #236); follow-ups
(temporal anti-aliasing, sun shafts, screen-space water reflections; Bevy's volumetric fog tried
and dropped) the same day (branch `claude/graphics-2`). Owner's request (playtest
list 2, docs/handoff/archive/playtest-2026-10-12.md): "Add any modern engine level graphics rendering
functionality to improve the game visual without modifying the base assets or implementing new
ones."

Everything here is an **engineering choice**, not a fact about the original game: none of it
is tagged `[documented]`/`[observed]` about Fallout: New Vegas, and none of it may be used to
explain a difference against the game's own frames. The parity checks compare the `vanilla`
look only.

## 1. What the renderer does now (Bevy 0.19.1, `engine/nvforever`)

| Part | State before this work | Where |
|---|---|---|
| Main view | Not HDR: forward PBR with tonemapping (Tony McMapface) in the shaders into an 8-bit sRGB target; MSAA 4x (`--msaa`, the owner's game runs 8); depth prepass (water and the image-space blur read it) | `main.rs` `setup` |
| Alpha-tested edges | Smoothed with MSAA (soft edges standing in for transparency multisampling) | `nvf_assets::nif_material`, grass, trees |
| Sun | One directional light; shadows on by default in the game since 2026-10-26 (owner, once the first-person flicker was fixed: player-body.md; off 2026-10-13 to 10-26 for frame cost and that flicker; `--no-sun-shadows`, off without `--play`; `--sun-shadows` or the settings page): 3 shadow cascades out to `--shadow-distance` (8,000 since #254; §5.3), maps 2048² (Bevy's default), Gaussian PCF (Bevy's default); a second "shade" light for the first-person arms casts within 256 units of the camera (`ARMS_SHADE_REACH`) | `sky.rs`, `player_body.rs` |
| Local lights | Point/spot lights from LIGH records, flashlight with shadows | `lights.rs`, `flashlight.rs` |
| Sky | The game's sky meshes, colours mapped through the inverse of the tonemapper so the grade sees the weather's colours | `sky.rs`, `sky.wesl` |
| Fog | Bevy distance fog, linear, from the weather's near/far | `sky.rs` |
| Water | Planar reflection camera, refraction from the view transmission copy, depth-based shore fade | `water.rs`, `reflection.rs` |
| Image space | The game's grade (saturation, tint, brightness, contrast) and the modifiers' blur, double vision, radial blur, fade, as passes after tonemapping | `imagespace.rs` |
| Texture filtering | Trilinear, 8x anisotropic for the game's DDS textures; trilinear (no anisotropy) for the default sampler | `nvf_assets::texture` |
| Bloom, AO, eye adaptation | None (image-modifiers.md §5: the HDR and bloom parameters are not drawn) | |

Constraints that decide what fits:

- The custom prepass fragments (NIF, grass, tree, water) write **depth only**; Bevy's SSAO, SSR,
  TAA and motion blur need normal or motion-vector prepasses from every material.
- The view is **multisampled**, and the alpha-tested foliage depends on it. Bevy's SSAO and TAA
  require MSAA off.
- The owner's PC has a **GeForce GTX 1050 (4 GB)**, shared with other sessions' game runs.
- `vanilla` must stay the picture the parity lab and the smoke checks were made with.

## 2. Candidates

| Candidate | Value | Cost / risk | Decision |
|---|---|---|---|
| Ambient occlusion (own pass from the depth prepass) | High: grounds objects, adds depth to interiors | ~1-2 ms; darkens direct light too (post-process) | **Built** (§3.1) |
| Bevy SSAO (GTAO-like) | High | Needs MSAA off + normal prepass in every custom prepass shader | Rejected |
| Longer sun shadows (reach ×1.5; 4096² maps until #1367) | Medium: shadows further out | About 0.2-0.3 ms at Novac for the longer reach; the 4096² maps cost about 2 ms more and went | **Built** (§3.2) |
| Contact shadows (Bevy, screen-space ray march) | Low-medium | Tried: speckled self-shadowing on near surfaces and per-frame noise without TAA | Rejected (§4) |
| Bloom (own LDR pass, thresholded) | Medium: lamps, windows, the sun glow as in the game's own frames (world.md §6 notes the halos the renderer lacks) | ~0.5 ms at quarter size | **Built** (§3.3) |
| Bevy bloom | Medium | Needs an HDR main view: Rgba16Float targets (twice the traffic with MSAA 4x on the 1050) and every pass reading the picture runs on another format | **Built** 2026-10-23 as the light glow, on in vanilla (§3.11; the owner's PC now has an RTX 4070) |
| Sharpening (own contrast-adaptive pass) | Medium: the 2010 textures look crisper | ~0.2 ms; can add rims if overdone | **Built** (§3.4) |
| Bevy CAS | Medium | Tried: garbled the non-HDR view (yellow/white noise) | Rejected |
| Anisotropic filtering 16x | Low-medium: ground and roads at a slant | Nearly free on the GPU | **Built** (§3.5) |
| Sun in-scattering in the fog (Bevy `DistanceFog` directional light) | Low: NVWastelandClear's fog reaches 200,000 units, so nothing showed | A shadow-map fetch per fragment | Tried, rejected |
| Bevy TAA | Medium | Needs motion vectors from every material's prepass (the custom NIF, grass, tree and water prepasses write depth only; wind-animated grass and trees would need last frame's wind) and MSAA off | Not used: own pass instead |
| TAA from depth and camera alone (own pass) | Medium: smooth edges and plant cut-outs without multisampling, cheaper than MSAA 4x | Softer textures while the view moves; what moves by itself is not followed | **Built**, as the other choice to MSAA, not in `enhanced` (§3.6) |
| Bevy FXAA, Bevy SMAA | Low-medium: smooth edges without multisampling, cheaper than MSAA; on top of MSAA they add little | Blur fine texture detail (FXAA more); plants' cut-outs are not smoothed (no multisampling) | **Offered** in the anti-aliasing choice instead of MSAA, not in `enhanced` (§3.6, 2026-10-16) |
| Shadow filtering method (Bevy `ShadowFilteringMethod`) | Low: harder or softer sun-shadow edges | Temporal is grainy without TAA | **Offered** as a choice, Bevy's default (Gaussian) kept (§3.9) |
| Sun shafts (own screen-space pass), tied to the weather's fog | Medium: dusk and dawn, the sun behind roofs and poles | ~0.03 ms here | **Built** (§3.7) |
| Bevy volumetric fog | Would replace the game's fog look | Tried: darkens the whole picture on the non-HDR view, box edges | Rejected (§4) |
| SSR on water | Low: the planar reflection is better where it reaches | Own march in the water shader from the depth prepass | **Built** for where the planar reflection does not reach (§3.8) |
| Bevy atmosphere | Would replace the game's sky look | | Rejected |
| Auto exposure, depth of field, motion blur | Change the look; the game's own HDR/eye-adapt values (IMGS DNAM) are not decoded | | Not now (§6) |
| PCSS soft shadows | Medium | Needs the `experimental_pbr_pcss` feature, noisy without TAA | Not now |

## 3. What was built

`--graphics vanilla|enhanced` picks the look; each improvement is also a switch on the pause
menu's settings page (category "Graphics") and can be compared alone with
`--graphics-only <keys>`. The log says what is on: `graphics: ambient occlusion, longer
shadows, bloom, sharpening, 16x anisotropic filtering, sun shafts, screen-space water
reflections` (or `graphics: vanilla (no engine-level improvements)`), again whenever a switch
changes. Anti-aliasing (`anti-aliasing`: multisampling off/2x/4x/8x, FXAA, SMAA or TAA) and
shadow filtering (`shadow-filter`) are choices of their own, not part of `enhanced` (§3.6,
§3.9); the field of view, the display sync and a frame-rate limit are on the Display page (§3.10).
Code: `graphics.rs` (preset, switches, applying them), `ssao.rs`/`ssao.wesl`,
`post.rs`/`post.wesl`, `taa.rs`/`taa.wesl`, `shafts.rs`/`shafts.wesl`, `water.wesl` (`ssr`),
`sky.rs` (`shadow_reach`), `nvf_assets::texture::set_anisotropy`, `frame_pace.rs`, the rows in
`settings.rs`.

Pass order after the main pass: tonemapping → ambient occlusion → temporal anti-aliasing, FXAA or
SMAA → sun shafts → bloom and sharpening → the game's image-space blur and grade (imagespace.rs) → UI. So the game's grade and its modifiers
(fades, blur, double vision) work on top of the improvements exactly as on the vanilla picture,
and nothing repeats them.

### 3.1 Ambient occlusion (`ambient-occlusion`)

From the depth prepass alone (multisampled or not: sample 0 is read): the view-space position
from the reverse-infinite depth (distance = near / depth), the normal from the neighbouring
depths on the side with the smaller step, 12 samples on a 3-turn spiral within 64 game units
(about 0.9 m; at most 80 pixels on the screen), turned per pixel by a 4 × 4 ordered pattern;
each sample occludes by its cosine to the normal above 0.12, fading out between half and the
whole radius. Strength 0.55. Since 2026-11-02 (#1370) this is worked out at half resolution,
for the top-left pixel of each 2 × 2 block, into a two-channel texture that keeps the pixel's
distance beside its openness (a quarter of the work; the pattern turns per half-resolution
texel). The result is blurred over the 4 × 4 texels of the pattern around each pixel, keeping
to texels within about 5 % of the pixel's own distance (so nothing bleeds across a depth edge),
and multiplied into the tonemapped picture. Those 4 × 4 texels span about 8 × 8 pixels of the
picture against 4 × 4 before, so contact shadows are a little softer. A pixel no nearby texel
shares a distance with (a pole or a grass blade a pixel wide) takes the texel nearest its
distance. Distances are held to 60,000 units (half floats end at 65,504; the sky dome is at
175,000 or more, far past the fade). It fades out between 4,000 and 7,000 units (where the radius is a pixel or two and the
fog takes over). The sky is left alone: the dome is opaque and in the depth prepass, but at
175,000 units or more, far beyond the fade (corrected 2026-10-12: not depth 0 as first written).

Known limits: it darkens sunlit creases too (a post-process cannot tell ambient from direct
light); water is not in the depth prepass, so the lake bed's occlusion shows faintly through
the water surface; the first-person arms take a little occlusion from their own folds.

### 3.2 Longer sun shadows (`fine-shadows`)

**Since 2026-11-03 (#1367) the maps stay 2048²**; the switch keeps only the longer reach,
and the settings page calls it "Longer sun shadows" (the INI key stays `fine-shadows`).
The owner chose the frame rate: "we're going for the smaller one i want as much frames we can
get". [observed 2026-11-03, main 650911ee, i9-14900K, RTX 4070, 1920 × 1080,
`--graphics enhanced`, `--play` standing still, `NVFOREVER_HOME` without stored settings;
one run per place cycling the three sets every 10 s (`--graphics-cycle`), the first loop left
out, three or four periods each]

| Place | 4096², reach 12,000 (before) | 2048², reach 12,000 (now) | 2048², reach 8,000 (switch off) |
|---|---|---|---|
| Novac 07:00 | 11.30 ms | 9.02 ms | 8.70 ms |
| Novac 12:00 | 10.88 ms | 8.75 ms | 8.52 ms |
| Goodsprings 12:00 | 7.60 ms | 6.94 ms | 6.77 ms |
| Strip 22:00 (moon shadows, always 2048²) | 6.14 ms | 6.10 ms | 6.11 ms |

The shadow passes' GPU time at Novac 12:00 went from 1.82 to 0.87 ms with the smaller maps;
the rest of the difference was the render thread waiting for the GPU. Screenshots from the
free camera close to the ground (Goodsprings 12:00, Novac 07:00, two headings each, pitch
−35°) show the same shadows with slightly softer edges at 2048².

Before #1367: `DirectionalLightShadowMap` 4096² instead of 2048², and the last cascade at 1.5 ×
`--shadow-distance` (12,000 units by default since #254), the first cascade bound unchanged (a twelfth of
`--shadow-distance`): the nearest shadows get twice the texels per unit, the far cascades about
1.3 times, and shadows reach half as far again. Before #254 the arms' shade light copied the
cascades, so it also got 4096² maps: about 512 MB of shadow maps in all (vanilla: 128 MB).
Since #254 it has one cascade of its own (256 units); the total has not been measured again.

**By night and at the horizon** (#1369, owner 2026-11-02: cheaper moon shadows;
`graphics::sun_shadow_quality`): below the horizon the sun's light is the moon's (`sky.rs`),
and it casts one cascade out to 3,000 units (`MOON_SHADOW_DISTANCE`) in 2048² maps whatever
this switch says; while the light is under a tenth of its strength in its fade at the
horizon (`SHADOW_FADED`), no shadow maps are drawn. [observed 2026-11-02, i9-14900K, RTX 4070,
1920 × 1080, 8x MSAA, enhanced] With the day's 3 cascades at 4096² (before #1367) the moon's shadows cost
about 2.5 ms on the Strip at 22:00 (9.2 ms against 6.7 ms without sun shadows).
In interiors the sun is the cell's directional light (XCLL) and the clock stands still, so
it always casts the day's shadows there. The map size (`DirectionalLightShadowMap`) is one
setting for every directional light, so the first-person arms' shade light (`player_body.rs`
`shade_light`, its own 256-unit cascade) has 2048² maps too.
The phase is applied again when a new sun is spawned (a worldspace switch) and when
something else puts cascades on the sun (`sky::set_shadows`, the settings page).

### 3.3 Bloom (`bloom`)

On the tonemapped picture (linear values 0..1): the brightest channel over 0.82 with a soft knee
of 0.12 is kept at a quarter of the size (four bilinear taps over 4 × 4 pixels), blurred across
and down with a 9-tap Gaussian two texels a step (about 34 quarter-size texels wide, ~136 pixels
of the full picture), and added back at 0.9. This is how bloom was done in the original's era,
on display values; it does not touch the HDR parameters of the game's image spaces, which are
still not decoded (image-modifiers.md §5).

### 3.4 Sharpening (`sharpen`)

Contrast-adaptive against the four neighbours: the centre is pushed away from their sum by
0.5 × ¼, less where the neighbourhood already has contrast (none above 0.6), and held within
85 %-115 % of the neighbourhood's range so no bright or dark rims appear.

### 3.5 Anisotropic filtering 16x (`anisotropic`)

The game's textures (`nvf_assets::texture::sampler`) get 16x instead of 8x, and the default
sampler (generated LOD and other engine-made images) 16x instead of none. The switch applies to
textures loaded after it changes (cells loaded next); the start's textures follow the command
line.

### 3.6 Anti-aliasing (`anti-aliasing`; not part of `enhanced`)

One choice on the settings page (since 2026-10-16; before, temporal anti-aliasing was a switch
`taa` of its own and the multisampling only `--msaa`): **Off, 2x, 4x, 8x** (multisampling, the
start being `--msaa`, default 4x), **FXAA**, **SMAA** or **TAA**. The last three draw the view
without multisampling and smooth the picture after tonemapping, before the sun shafts, bloom and
the game's image space (the same place as TAA, so the grade is not smoothed). Choosing one of them
keeps the multisampling level for when a level is chosen again. Stored as a part in the INI like
any slider (`anti-aliasing=0.33` is 4x; the seven choices are the steps 0, 1/6, … 1). Unattended
runs keep `--msaa`, so `--graphics vanilla` draws as before.

- **FXAA**: Bevy's (`bevy::anti_alias::fxaa::Fxaa`) at its default thresholds (High, High)
  [engineering choice, not tuned by eye]. Cheapest; softens fine texture detail and does not
  smooth the plants' cut-outs (tested per pixel without multisampling).
- **SMAA**: Bevy's (`bevy::anti_alias::smaa::Smaa`) at its default preset (High) [engineering
  choice]. Sharper than FXAA, a little dearer; the same limit on the cut-outs.
- **TAA**: ours, as follows.

#### Temporal anti-aliasing

The other choice to multisampling, not on top of it: while it is on the main view is drawn
without multisampling (`Msaa::Off`); switching it off puts back `--msaa`. The alpha-tested
cut-outs (plants, fences, hair, grass) are then tested per pixel and smoothed by the jitter
over the frames instead of by alpha to coverage.

- The projection is shifted every frame by a sub-pixel amount (Halton 2,3, eight steps;
  Bevy's `TemporalJitter`), textures sampled at a mip bias of -0.5.
- After tonemapping and the ambient occlusion, before the sun shafts, bloom, sharpening and the
  game's image space (so the grade, fades and blurs are not smeared over the frames): each
  pixel's point is carried from this frame's camera to the last frame's (the depth and both
  cameras' matrices; the reprojection is worked out on the CPU in double precision, the world
  being hundreds of thousands of units across), taking the nearest depth of the 3 x 3
  neighbourhood so edges keep to the object in front. The history is sampled with a
  Catmull-Rom filter and clipped to the colours around the pixel this frame (variance clipping
  in YCoCg, 1.25 sigma, within the neighbourhood's min/max), then blended: 10 % of this frame
  standing still, up to 50 % when the picture moves 16 pixels a frame or more, weighted by
  inverse brightness. The history is a pair of Rgba16Float textures; it starts over on the
  first frame, after a resize, when the camera jumps more than 400 units in a frame (doors,
  MoveTo) and whenever the pass did not run the frame before.
- Bevy's own TAA needs motion vectors from every material's prepass; ours write depth only,
  so what moves by itself (actors, plants in the wind, water) is looked for where it was not;
  the clipping keeps that from smearing, at the cost of less smoothing on it while it moves.
- The first-person arms and weapon move with the camera: points nearer than 48 units are taken
  as not moving on the screen, and the neighbourhood's nearest depth is taken only among
  pixels on the same side of that distance (otherwise the world next to the hand took the
  hand's motion and left a dark rim while turning [observed 2026-10-12, fixed]). A wall nearer
  than 48 units counts as camera-locked too (it then keeps less smoothing while turning).
- Switching the multisampling while the game runs: in the frame it changes, the view's `Msaa`
  and its depth prepass texture can disagree; the passes that bind the depth (ambient
  occlusion, sun shafts, TAA, the image-space blur) now go by the texture's own sample count
  and skip that frame (before, a validation error ended the run).
- Looks [observed 2026-10-12, Goodsprings noon, standing, and walking while turning 40 degrees a
  second]: edges as smooth as MSAA 4x standing still, plants' cut-outs smoother; textures a
  little softer standing still and clearly softer while the view moves (the usual TAA trade);
  the arms stay sharp. **Not in `enhanced`** for that softness and because the owner plays with
  8x multisampling; the settings page offers it (`anti-aliasing` TAA).
- Cost: see §5.1; the pass itself ~0.05 ms here, and the main pass and the prepass are cheaper
  without multisampling, so TAA costs less than MSAA 4x.

### 3.7 Sun shafts (`light-shafts`)

Screen-space light scattering, at a quarter of the size, on the tonemapped picture after the
temporal anti-aliasing and before bloom: the sky around the sun (where the depth prepass has only
the sky dome, beyond 157,500 units) within 0.3 of the picture's height of the sun, its brighter
part (linear luminance 0.04 to 0.4), is blurred along the lines towards the sun (24 taps over
1/24 of the length, then 24 taps over 85 % of the way to the sun, each weighted 0.97 of the one
before: about 576 taps in all) and added to the picture, kept below white without a hard clip.

Tied to the original's look: the light is the game's own sky and sun as drawn (their colours,
the clouds in front of the sun), and how strongly it scatters follows the weather's fog:
strength `0.6 + 0.9 x haze`, `haze = min(1, 20,000 / fog far) x fog strength` (NVWastelandClear,
fog far 200,000: haze 0.1, strength 0.69; a weather with its fog far at 20,000 or less: 1.5).
None indoors, none once the sun is down (the sun disc's own fade, -2 to 3 degrees of elevation),
fading out as the sun leaves the screen (from the edge to 1.6 x the half-width beyond it).

Looks [observed 2026-10-12, Goodsprings 19:00 facing the sunset]: light streams past the
mountains and over the town, the sun's glow is wider; at noon looking away from the sun nothing
changes. Every constant is a choice by eye on that one view; the first try (strength 1.5, source
radius 0.45) washed the whole view out.

### 3.8 Screen-space water reflections (`water-reflections`)

The planar reflection (reflection.rs) shows one water plane at a time, the nearest below the
camera; water at another height in view reflects only the sky gradient. There, with this switch,
the water shader (water.wesl `ssr`) marches the reflected ray through the depth prepass (28 steps
from 24 units, each 1.16 times the one before, about 9,000 units in all, then four halvings) and
takes the opaque picture (the transmission copy) where the ray passes behind something by less
than a step; trust fades near the picture's edges and towards the end of the march. Mixed in
after fog and tonemapping, in the reflected share's place (an approximation: the copy is already
fogged and tonemapped).

Where it applies: exterior water at a second height in view, e.g. Lake Las Vegas (water 3,000)
next to Lake Mead's cells (2,600), Camp Forlorn Hope (5,360), the Devil's Throat (2,290)
[observed: fnvtool world cells, the cells' water heights in WastelandNV]. Interior cells draw no
WATR planes in this engine (the Goodsprings Cave pool is a model with its own material), so the
switch does nothing indoors. Checked only by forcing it in place of the planar reflection at Lake
Las Vegas (`--pos=28500,59000,3500 --heading 200 --pitch -12`, a temporary shader change): the
far shore's cliffs show in the water as with the planar reflection, without artefacts; that water
reflects weakly, so the difference is small. It runs only on the water pixels it applies to, so
its cost was not measured.

### 3.8.1 Mirrored water (`water-mirror`; on in both presets)

The planar reflection itself (reflection.rs): a second camera draws the world mirrored in the
nearest water plane while that water is on screen. It is on in vanilla too, since the original's
water reflects the world; `--graphics-only no-water-mirror` or the settings switch turns it off
(#1380), and water then reflects the sky gradient, and the picture where `water-reflections` is
on. Cost at Hoover Dam by day, Lake Mead in view, where the CPU sets the frame: 7.6 ms a frame
with it, 7.1 without [observed 2026-11-02, RTX 4070, 1920x1080, `enhanced`]. The camera keeps
its own set of the sun's cascades (Bevy's renderer needs them for every view the sun lights);
emptying their caster lists was tried and saved 0.12 ms of shadow-pass GPU time and nothing of
the frame, so the reflected world keeps its shadows.

### 3.9 Shadow filtering (`shadow-filter`; not part of `enhanced`)

How the sun's shadow maps are sampled at a shadow's edge, Bevy's `ShadowFilteringMethod` on the
main camera: **Fast** (`Hardware2x2`: one hardware-filtered tap, harder and more stair-stepped
edges), **Smooth** (`Gaussian`, Bevy's default and what vanilla has always drawn: the camera then
carries no component at all) or **Temporal** (a rotated 8-tap pattern that changes every frame:
soft but grainy unless TAA is the anti-aliasing, which it is meant for). Matters only with sun
shadows on. The names are engineering choices; Bevy's PCSS is not offered (it needs the
`experimental_pbr_pcss` feature).

### 3.10 Field of view, vertical sync, frame-rate limit (Display page)

Settings rows that are not improvements and do not change the vanilla picture unless chosen:

- **Field of view** (`field-of-view`): the game's FOV setting (the horizontal angle at 4:3, as
  `--fov`, default 75) from 50 to 110 in steps of 5, shown in degrees. It sets the player
  camera's projection at once; aiming down sights zooms from it (aim_zoom.rs takes a changed
  angle as its new base).
- **Display sync** (`display-sync`, a dropdown; #1082): Vertical sync, on as Bevy's default
  (`PresentMode::AutoVsync`); Off is `AutoNoVsync`; Variable refresh (G-Sync, FreeSync) is
  vsync with the frame rate capped a few frames under the monitor's refresh rate (winit's
  current video mode; 144 Hz → 138). Kept as `display-sync=off|vsync|variable`; an INI with
  only the old `vsync=1`/`0` switch still sets it. Not on the page while `--log-frame-times`
  measures (which turns vsync off). What vendors document about VRR in borderless and
  exclusive full screen: spec-notes/display-sync.md.
- **Frame rate limit** (`frame-cap`): Off, 30, 60, 90, 120 or 144 frames a second
  (`frame_pace.rs`): the main world's frame waits at its end until 1/limit s has passed since the
  last (sleep, then spin the last millisecond); a late frame does not catch up. Bevy has no
  limiter of its own (`bevy_framepace` is a separate crate); this plain one is enough to keep
  a fast GPU from drawing hundreds of frames a second in menus [engineering choice].

### 3.11 Light glow (`glow`; on in vanilla too)

The original as the owner plays it (`bDoHighDynamicRange=1`) glows: neon, bulbs, lit
signs, fire and beams bleed soft halos into the night (the fx audit frames of 2026-10-23,
`C:\fnv-private\references\fx_audit\game`). The owner asked for that look, and for a
natural one rather than a copy, so this is on by default in `vanilla` and in `enhanced`;
`--graphics-only no-glow` or the settings switch "Light glow" turns it off, and the
enhanced "Bloom" (§3.3), when on, stands in for it.

- The main camera gets an HDR view (`bevy::camera::Hdr`: values over 1 survive until the
  tonemapper, which then runs as its own pass) and Bevy's bloom with its NATURAL preset
  (energy-conserving, no threshold: every pixel scatters a share of its light, so only
  what is far brighter than its surroundings shows a halo) (`post.rs` `glow_bloom`).
- Its strength is 0.15 at the image space's Bright Scale 2 (NVDefaultExterior) and
  follows Bright Scale / 2 (held to half and twice) as the place's image space and the
  weather's adapters change (`post.rs` `glow_intensity`, world.md §6 "HDR and bloom").
- What emits light is brighter than white only in this view: unlit shapes are drawn at
  their emissive colour × multiplier (nif.md §6 "Emissive multiplier of unlit shapes"),
  so Gomorrah's neon at multiplier 25 burns; lit shapes' emissive light already is.
- Every pass after the tonemapper (ambient occlusion, anti-aliasing, sun shafts, Bloom and
  sharpening, the image-space grade) sees the same tonemapped values as before; the
  passes key their pipelines on the view's format.

Engineering choices made by eye against the original's frames; not used to explain
differences in the parity frames.

## 4. Tried and dropped

- **Bevy `ContactShadows`** on the sun (16 steps, 40 units, thickness 12): the right effect on
  sunlit bushes, but a band of dark speckles on near surfaces seen at a grazing angle (the
  near roof at Goodsprings) and per-frame noise, which Bevy expects TAA to clean up.
- **Fog in-scattering towards the sun**: invisible in NVWastelandClear (fog to 200,000) and
  a shadow-map fetch per fragment.
- **Bevy bloom with an HDR main view** and **Bevy CAS**: see §2.
- **Bevy volumetric fog** (`VolumetricFog` on the camera, `VolumetricLight` on the sun, a
  30,000 x 6,000 x 30,000-unit `FogVolume` following the camera, density 0.0005, 64 steps) at
  Goodsprings 19:00: the whole picture went dark brown and the volume's box edge showed as a
  hard step across the mountains [observed 2026-10-12, private frame
  `C:\fnv-private\scratch\graphics-2\vol-dusk.png`]. Its pass blends absorption and linear
  in-scattered light (times the exposure) onto the main texture, which in this non-HDR view
  already holds tonemapped display values, so it darkens without adding visible light; and a
  box big enough for the Mojave's distances needs coarse steps. It would also replace the
  weather's distance fog look rather than follow it. The sun shafts (§3.7) give the light
  shafts it was wanted for.

## 5. Frame rate and the defaults

### 5.1 Follow-ups (2026-10-12, this machine)

Measured on the machine that ran the follow-ups (GeForce RTX 4070, not the owner's GTX 1050),
1280 x 720, free camera standing still at Goodsprings 19:00 facing the sunset, with the test aids
`--log-frame-times 6 --graphics-cycle "vanilla;taa;light-shafts;enhanced;..."` (one run switching
between the sets every 6 s, three rounds; GPU time per pass from timestamp queries, Bevy's
`RenderDiagnosticsPlugin`, about the last second's average). The frame rate itself stayed at the
display's 60 Hz in every set (presented with vsync whatever was asked), so the GPU figures are
the measure:

| Pass | GPU ms |
|---|---|
| Main opaque pass, MSAA 4x / TAA (no multisampling) | 1.00-1.03 / 0.70-0.74 |
| Depth prepass, MSAA 4x / TAA | 0.32-0.33 / 0.22-0.23 |
| Temporal anti-aliasing pass | 0.04-0.05 |
| Sun shafts (four quarter-size passes) | 0.02-0.09, mostly 0.03 |
| Ambient occlusion (§3.1, for comparison) | 0.11-0.19 |
| Bloom and sharpening (§3.3-§3.4) | 0.03-0.05 |

So on this GPU each addition costs a few hundredths of a millisecond, and TAA saves about
0.4 ms against MSAA 4x. On the GTX 1050 (several times slower [hypothesis]) the same passes
would cost several times as much; not measured there.

### 5.2 First set (PR #236)

Measured 2026-10-12 on the owner's PC (GeForce GTX 1050 4 GB, 8 cores), 1280 × 720, Goodsprings
at noon (`--pos=-69000,3500,8900 --heading 30 --pitch -8`), free camera standing still,
`--run-seconds 60` (frames over the whole run, cell loading included), one after the other.
Other sessions' game runs and builds shared the machine, so the figures are noisy: the same
vanilla run gave 14.5 and later 7.2 frames per second.

| Run | Frames per second |
|---|---|
| `--graphics vanilla` (first) | 14.5 |
| `--graphics enhanced` (right after) | 10.0 |
| `--graphics-only fine-shadows` | 19.6 |
| `--graphics-only ambient-occlusion,bloom,sharpen,anisotropic` | 10.6 |
| `--graphics vanilla` (again, last) | 7.2 |

The back-to-back pair says enhanced costs about a third of the frame rate; the noise does not
allow a figure per feature. On this GPU the vanilla picture is already below 20 frames per
second, so:

- **Normal play defaults to `vanilla`** (`graphics::PLAY_DEFAULT`); `--graphics enhanced` or
  the settings page's "Graphics" switches turn the improvements on. On a faster GPU
  changing `PLAY_DEFAULT` to `Enhanced` is the one-line switch.
- `vanilla` is also the look of every test run: `--screenshot`, `--timelapse`, `--frame-json` and
  `--run-seconds` default to it whatever `PLAY_DEFAULT` says, so the parity lab, the picture
  checks and every smoke scenario keep their pictures, timings and expectations. `--graphics
  enhanced` on such a run asks for the enhanced look explicitly (the smoke scenario
  `graphics-enhanced` does).

### 5.3 Sun shadows (2026-10-12, playtest list 4 item 6)

The owner: "shadow rendering seriously affects performance; maybe limits on draw distance or
quality". Bevy times no shadow pass (they do not appear in the GPU list of `--log-frame-times`),
so the cost was measured as frame time with the cycle's shadow keys (§6) and the casters each
cascade drew (the log's "shadow casters per cascade" line). Machine: GeForce RTX 4070, 1280 × 800,
`--play` standing still at Goodsprings, (−69698, 2246), 14:00 looking east down the road (where
the owner's screenshots of item 5 were taken), `vanilla`, cycle switched every 4 s, three rounds
each; the figures are the round means (vsync off). The GPU's own time stayed between 1.9 and
4.1 ms in every set: the cost is the CPU's, one pass per cascade per view, each culling,
specialising and queueing its casters.

| Sun shadows | Casters per cascade | Frame time (ms) |
|---|---|---|
| off | | 7.3-7.9 |
| 8,000 in 2 cascades | 61, 2,000 | 12.4-13.8 |
| 8,000 in 3 cascades | 61, 340, 1,900 | 13.7-15.2 |
| 8,000 in 4 cascades | 61, 220, 710, 1,880 | 15.0-16.8 |
| 16,000 in 3 cascades | 200, 1,090, 3,060 | 16.4-18.3 |
| 16,000 in 4 cascades (the old default) | 200, 570, 1,690, 3,030 | 19.0-20.8 |

All with the arms' shade light (`player_body.rs`) taking the sun's first cascade, about 190
casters, as it did. Changed [engineering choices]:

- **The shade light reaches 256 units** from the camera (`ARMS_SHADE_REACH`): about 30 casters, and
  the old default 16,000 in 4 went from 19.0-20.8 to 16.9-17.6 ms. A cascade keeps every caster
  between it and the sun, so walls and roofs still shade the arms; their shadows on the arms
  are finer than before.
- **The sun's shadows default to 8,000 units in 3 cascades** (`sky::DEFAULT_SHADOW_DISTANCE`,
  `DEFAULT_SHADOW_CASCADES`; `--shadow-distance` still sets the reach): 12.5-12.9 ms against
  the old 19.0-20.8, about 7 ms (a third of the frame) saved; shadows off is 7.3-7.5. The first
  cascade, a twelfth of the reach, is now 667 units instead of 1,333, so shadows near the player
  are finer. Visible change: nothing farther than 8,000 units (about 114 m) casts a sun shadow;
  at noon the view down the road is unchanged
  (`C:\fnv-private\references\shadows_sun\shadow_limits_before_after.png`); at dusk the long
  shadows of distant rocks and buildings end nearer. The original casts no sun shadows from
  the world at all (world.md §6), so this is no further from its look.
- Finer shadows (§3.2) keep their 4096² maps and reach 1.5 × the distance, now 12,000
  (the 4096² maps went in #1367; the longer reach stays).

Not changed: the water's reflection camera, while active, gets its own cascades of the sun (Bevy
builds them for every active camera, with no way to leave one out), doubling the shadow passes
while water is in view; casters are not thinned by size with distance (no per-cascade filter in
Bevy; built and measured in §5.5).

### 5.4 Anti-aliasing and shadow-filter choices (2026-10-16, this machine)

GeForce RTX 4070 (not the owner's GTX 1050), 1280 × 720, free camera standing still at
Goodsprings 14:00 (`--pos=-69000,3500,8900 --heading 30 --pitch=-8 --sun-shadows`),
`--log-frame-times 4|5 --graphics-cycle` over vanilla (MSAA 4x), fxaa, smaa, taa and the two
shadow filters, five runs. Other sessions shared the machine; the frame time (11.8-12.5 ms in the
quiet runs) was the CPU's and did not tell the choices apart, so the GPU passes are the measure:

| Pass | GPU ms |
|---|---|
| Main opaque pass, MSAA 4x / without multisampling (FXAA, SMAA, TAA) | 0.72-0.78 / 0.56-0.58 |
| Depth prepass, MSAA 4x / without | 0.30-0.31 / 0.24 |
| FXAA pass | 0.04 |
| SMAA (its three passes) | 0.08-0.11 |
| Main opaque pass with the shadow filter Fast / Smooth / Temporal | 0.69-0.77 / 0.73-0.78 / 0.73-0.76 (within the noise) |

So FXAA and SMAA, like TAA, cost less than MSAA 4x in all (the main pass and the prepass save more
than their own pass), FXAA the least; the shadow filters' difference does not show at this
scale. On the GTX 1050 every figure would be several times larger [hypothesis, not measured
there], with the same order: MSAA 8x dearest, then 4x, then SMAA, TAA, FXAA.

One run stopped on a validation error ("A query of type PipelineStatistics was not ended") at
a switch into vanilla after the cycle had gone through SMAA and the shadow filters; four more
runs making the same switches (about 45 in all) did not repeat it. PipelineStatistics queries
come only from the measuring aid's `RenderDiagnosticsPlugin`; the smoke scenario
`graphics-settings`, which makes every switch without it, saw no error.

### 5.5 Where the sun shadows' cost goes (2026-10-23)

The owner: sun shadows drain performance, presumably the main thread. Measured with the span
profile (`NVF_PLAY_TEST_PROFILE`, a `--features nvforever/profile` build; frame-rate-audit.md
§1) and `--log-frame-times 8 --graphics-cycle`, so each comparison is one run. Desktop
(32 threads, RTX 4070), 1280 × 800, `--play` at the Goodsprings start, 14:00, standing still,
vanilla; other sessions built and ran the game at the same time, so only the cleanest rounds
(those that agree within about 0.5 ms) are quoted [observed].

**It is the render thread.** With shadows on the main thread's own work hardly changes
(about 7 against 6 ms a frame, Bevy's caster culling `check_dir_light_mesh_visibility` about
1 ms of it); the render thread goes from about 9.7 to 16.3 ms, and the main thread waits for
it (the `RenderExtractApp` span holds the wait). The render spans that grow: writing the
binned instance buffers (1.6 → 4.0 ms), preparing the preprocessing bind groups (1.2 → 2.4),
recording the passes (`RenderContextState::apply` 2.2 → 3.7) and submitting (`queue_submit`
1.3 → 2.4). The GPU's time stays small (§5.3).

| Sun shadows (8,000 units) | Casters per cascade | Frame (ms) | Render thread (ms) |
|---|---|---|---|
| off | | 10.2 | 9.7 |
| 1 cascade | 2,200 | 14.3 | 13.8 |
| 2 cascades | 85, 2,200 | 14.9 | 14.3 |
| 3 cascades (default) | 85, 460, 2,130 | 16.8 | 16.3 |

All with the arms' shade light (about 30 casters). So about 4 ms comes with having sun
shadows at all and the third cascade adds about 2 ms.

**Thinning the casters does not pay** (`shadow_thin.rs`, `--shadow-thin <ratio>`, off by
default): beyond 1,000 units a placed reference whose bounding radius is under `ratio` × its
distance casts nothing. The sun's last cascade, broken down by that angular size (the
frame-time log's `sun's last cascade:` line): about 75 meshes under 0.01, 450 at 0.01-0.02,
490 at 0.02-0.04, 340 at 0.04-0.08, 280 over 0.08, the rest near or not placed references.
Clutter whose shadow covers a few pixels is a small part.

| `--shadow-thin` | Meshes thinned | Last cascade | Render thread (ms) |
|---|---|---|---|
| 0 | 0 | 2,080-2,330 | 16.2-17.3 |
| 0.01 | 235 | 2,000-2,260 | 16.0-16.6 |
| 0.02 | 1,240 | 1,535-1,670 | 15.4-16.4 |
| 0.04 | 2,140 | 1,030-1,150 | 14.4-16.2 |

Halving the far cascade saves 1-2 ms of about 6.5, and at 0.04 an object under 2 m in radius
loses its shadow beyond 50 m: a visible change for little. The rule stays off.

**Why** [hypothesis, from Bevy 0.19.1's sources]: the per-view work scales with batch sets
more than with casters. `write_binned_instance_buffers` writes two GPU buffers and a GPU
unpacking job per batch set of every view, and the preprocessing bind groups are made per
view each frame. A shadow draw that only writes depth shares one batch set across materials,
but an alpha-tested NIF shape (plants, fences, signs) needs its material's textures, and
`NifMaterial` is not bindless, so each such material is a batch set of its own in every cascade.
Fewer casters rarely removes a whole material.

What would cut it, in order of what it saves:
- **A bindless `NifSurface`** (frame-rate-audit.md §4): alpha-tested materials would share
  batch sets in every view, the main view included. Built in §5.6: it takes the shadows'
  cost almost to nothing.
- **2 cascades instead of 3**: about 2 ms, with coarser shadows from 667 to about 2,300 units
  (the middle cascade's range; the first cascade stays 667). The owner's open choice from §5.3.
- **The arms' shade light** is one more shadow view while the first-person arms are drawn.
- **Water in view** gives the reflection camera a set of cascades of its own (§5.3).

### 5.6 Bindless NIF materials (2026-10-23)

`NifSurface`, the NIF material's extension of `StandardMaterial`, is bindless where the device
allows it, as `StandardMaterial` already was (Bevy turns bindless on for an extended material
only when both parts are): its values go to one storage array for all NIF materials and its
textures (holes, FaceGen offsets) to Bevy's shared texture arrays, and the shaders read them
through the index tables (`nif_surface_bindings.wesl`; without bindless the plain bindings
50-53). [engineering choice; nif_material.rs "Bindless"]

**Batch sets** (the profile aid's `profile batches:` line, Goodsprings start, 14:00, sun shadows
on) [observed]:

| | Before | Bindless |
|---|---|---|
| Shadow phases (4 views: 3 cascades and the arms' shade light) | ~830 | 49 |
| Main view, opaque / alpha-mask (and their prepasses) | 169 / 147 | 9 / 14 |

**Frame time** (same place, `--log-frame-times 8 --graphics-cycle "vanilla;vanilla,no-sun-shadows"`,
the span-profile build; the runs of each build alternated, and only uncapped windows are quoted:
in some runs the frames were held to the display's 60 Hz whatever the build, the window or the
compositor's doing) [observed]:

| | Before | Bindless |
|---|---|---|
| Sun shadows on (8,000 units, 3 cascades) | 16.5-17.3 ms | 6.4-6.6 ms |
| Sun shadows off | 9.9-10.0 ms | 6.7-6.8 ms |
| Render thread, shadows on / off | 16.3-17.0 / 10.0-10.4 ms | 6.3-8.3 / 5.2-6.1 ms |

The render spans that had grown with the shadows shrank most: writing the binned instance
buffers 4.0 → 0.4 ms, preparing the preprocessing bind groups 2.4 → 0.5, recording the passes
3.7 → 1.7, submitting 2.4 → 0.8. On the GPU the bin-unpacking pass went from about 1.7 to 0.15 ms.
The main thread is now about as busy as the render thread (6-8 ms), so the frame is no longer
held by either at this place; sun shadows cost next to nothing here.

**Same picture** [observed, screenshots of both builds at 1280 × 720 compared pixel by pixel]:
an NPC's face (Doc Mitchell, FaceGen tint: identical), the Pip-Boy (largest difference 12 of
255), the Prospector Saloon, Goodsprings at noon and dusk with sun shadows, a weapon in first
person, Gomorrah's roof fires: the only differences are what changes from run to run anyway
(wind-blown plants, a debug text line, HUD messages shown a moment apart). The refraction smoke
(`stealth-boy-refraction`, a moving bend) passes with both builds and its frames agree within
10 of 255. A build with the bindless attribute taken out (the fallback for devices without
bindless) drew the same face, Pip-Boy and first-person frames and passed the refraction smoke.
No pipeline or shader error was logged by any of the three builds.

Not measured: the owner's GTX 1050 (Bevy decides bindless per device; without it the plain
bindings are used, as before this change), and other places than Goodsprings.

### 5.7 Casters over the slice, not the square (2026-11-03, #1378)

The sun's shadows cost more at a low sun: at Novac the frame was 11.4 ms at 07:00 against 10.7
at noon, with the shadow passes' GPU time at 2.17 against 1.84 ms. Counting the casters each cascade
drew showed why [observed: a temporary probe of `CascadesVisibleEntities`]:

- Bevy fits each cascade to the light-space box around its slice of the camera's view and
  draws every caster in that box's square, plus everything between the box and the sun
  (`check_dir_light_mesh_visibility` skips the near plane). At 07:00 the sun stands 15°
  above the horizon, so more ground lies up-sun of each slice. That was 32, 201 and 13
  casters in the three cascades that lay wholly toward the sun past the near plane, against
  none at noon.
- Those are not waste. A test of whether any ray from the lowest ground could pass through
  them, given their height, removed 10 of them. They are the long morning shadows.
- The waste is the square. Seen from the sun, a slice of a perspective view is a trapezoid,
  and the square around it holds about as many casters again that shade nothing in the slice.
  This holds at every hour.

Changed (`sky.rs` `sun_casters_over_slice`, after Bevy's light visibility): each cascade
keeps only the casters whose light-space box overlaps the outline of its slice seen from
the sun, plus 8 shadow-map texels for the filter's reach. A cascade's receivers all lie in its
slice (the shader picks a cascade by view depth, and the overlap with the next cascade is in
both slices), and a caster can only shade what lies under it seen from the sun.

| Novac, casters per cascade | 07:00 | 12:00 |
|---|---|---|
| before | 154, 1,438, 3,248 | 120, 1,283, 2,928 |
| over the slice | 80, 761, 1,767 | 65, 480, 1,549 |

[observed 2026-11-03] Release builds, i9-14900K, RTX 4070, 1920 × 1080, `--graphics enhanced`,
`--play` standing still, `NVFOREVER_HOME` without stored settings. Each run is 45 s and logs its
frame time every 10 s; main and this change were interleaved over 3 rounds. One main run at 07:00
hit a throttled window at 16.67 ms and is left out.

| Novac | frame, main | frame, over the slice | shadow passes GPU, main / over the slice |
|---|---|---|---|
| 12:00 | 10.70-10.76 ms | 9.41-9.49 ms | 1.84 / 1.24 ms |
| 07:00 | 11.39-11.47 ms | 9.75-9.80 ms | 2.17 / 1.39 ms |

So 07:00 is now 0.33 ms from noon, against about 0.7 before. Screenshots of both builds at
07:00 and noon, from the free camera in four headings, differ only in wind-blown grass. The
casters dropped from the passes are still extracted for the frame (they stay visible to the
light), so the saving is in the passes, not in extraction.

## 6. Test aids for measuring

`--log-frame-times <seconds>` logs the mean and slowest frame time and the GPU time of the
costliest render passes every so many seconds (and asks for presenting without vsync, which the
driver may still impose); `--graphics-cycle "<set>;<set>;..."` switches between sets of
improvements after each logged period (`vanilla`, `enhanced` or keys as for `--graphics-only`;
the first half second after a switch is left out), so one run compares them under the same load.
A set may also change the sun's shadows: `no-sun-shadows`, `shadow-distance=<units>`,
`cascades=<n>`, `shadow-thin=<ratio>` (§5.5) among its keys
(`vanilla;no-sun-shadows;shadow-distance=16000,cascades=4`). The
anti-aliasing and shadow filters have keys too: `fxaa`, `smaa`, `taa`, `shadow-filter-hardware`,
`shadow-filter-temporal` (a set without one of them has `--msaa` and the Gaussian filter); the
log then gives the sun's shadow reach and cascades with each frame time, the sun's heading and
elevation, and the casters each cascade of each light drew; with sun shadows on also what
`--shadow-thin` thinned and the sun's last cascade broken down by angular size (§5.5).

## 7. Next candidates

- Bloom and eye adaptation driven by the image spaces' own HDR values (IMGS DNAM's 14 HDR
  floats and 3 bloom floats), once their meanings are documented from xEdit: that would move
  bloom from an engine choice towards parity.
- Bevy SSAO or GTAO once the custom prepasses write normals; motion vectors from the custom
  prepasses (last frame's wind and skinning) would let the TAA follow what moves by itself.
- Sun shafts through interior windows are the game's own light-ray meshes already; none from
  the moon.
- A shadow distance and map size setting with more steps than on/off (the cycle keys of §6
  measure them).
