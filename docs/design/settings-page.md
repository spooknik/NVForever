# The settings page: tabs, headings and rows (#1953)

The pause menu's settings page (`engine/nvf-play/src/pause.rs`) shows every registered
setting (`engine/nvf-play/src/settings.rs`) and the controls of
`engine/nvf-play/src/bindings.rs`. Since #1953 it shows them in seven tabs with headings,
from a table that only says where each row is drawn. This note is the page's model; the
code's module notes say the same at their own level.

## 1. The registry stays as it is

A row is registered once, by the module that owns its state (`add_setting`,
`add_choice_list`), with a key, a group, a label, a note and its default (the state before
the INI is read) [documented: `settings.rs` `Settings::add`, `apply_stored`]. The
registry's order (groups in the order they first appear, each row after its group's last)
is what the values, the INI, Defaults' snapshot (`StartStates`, `StartChoices`), the
pending resolution question and the test aids go by, all by a row's index. #1953 changes
none of that: no key, label, note, default or group changes, and nothing is reordered.

What changed is presentation only: `engine/nvf-play/src/settings_layout.rs` (where a row
is shown) and `engine/nvf-play/src/settings_view.rs` (what the page lists, as data).

## 2. The layout table

Seven tabs, the original's four categories first: **Gameplay, Display, Audio, Controls,
Graphics, Tweaks, Debug** [design: #1953; the original's Settings opens Gameplay, Display,
Audio and Controls, observed in the original's pause menu, 2026-11-09].

| Tab | Heading | Rows (keys), in order |
|---|---|---|
| Gameplay | Game | `difficulty`, `hardcore`, `kill-camera`, `vats-style`, `vats-slowdown`, `reload-slows`, `UsePipBoy` |
| Gameplay | Saving | `autosave-travel`, `autosave-fast-travel`, `autosave-wait`, `autosave-sleep`, `autosave-scripts` |
| Gameplay | Dialogue | `menu-pause`, `dialogue-goodbye-hint`, `dialogue-look`, `dialogue-recentre`, `dialogue-zoom`, `lip-sync` |
| Gameplay | Camera | `detached-camera`, `true-iron-sights`, `first-person-body`, `weapon-sway`, `head-bob`, `weapon-lower`, `field-of-view` |
| Display | Screen | `resolution`, `borderless`, `monitor`, `display-sync`, `frame-cap`, `ui-scale` |
| Display | Interface | `hud-colour`, `hud-red`, `hud-green`, `hud-blue`, `pipboy-colour`, `pipboy-red`, `pipboy-green`, `pipboy-blue`, `language`, `dynamic-crosshair`, `crosshair-style`, `sneak-gauges` |
| Display | Performance | `fps-counter`, `fps-counter-fps`, `fps-counter-low`, `fps-counter-graph`, `fps-counter-split`, `fps-counter-cpu`, `fps-counter-gpu`, `fps-counter-vram`, `fps-counter-ram`, `fps-counter-loaded`, `fps-counter-upscaling` |
| Audio | (none) | `volume-master`, `volume-music`, `volume-footstep`, `volume-voice`, `volume-effects`, `volume-radio`, `music`, `voice`, `reverb`, `voice-reverb` |
| Controls | (none) | the Controls tab's own rows (§4), then `button-style`, `aim-assist`, `aim-slowdown`, `aim-lock` |
| Graphics | Quality | `quality` (#1966; only when the install has its preset files: docs/spec-notes/quality-presets.md) |
| Graphics | Picture | `enhanced` (#1966), `grade`, `sun-shadows`, `upscaler`, `upscaler-quality`, `dlss-model`, `fsr-library`, `frame-generation`, `frame-generation-frames`, `anti-aliasing`, `latency-reduction`, `shadow-filter`, `ambient-occlusion`, `fine-shadows`, `bloom`, `glow`, `eye-adaptation`, `neon-glow`, `sharpen`, `anisotropic`, `light-shafts`, `water-reflections`, `water-mirror`, `smooth-terrain` (#2149) |
| Graphics | Distance | `distant-lod`, `grass`, `object-distance`, `actor-distance`, `item-distance`, `shadow-distance`, `grass-distance` |
| Graphics | World | `head-tracking`, `foot-ik`, `sky-clock` |
| Tweaks | (none) | none named yet |
| Debug | (none) | `overlay`, `hit-markers`, `god-mode`, `navmesh`, `scope-3d` |

The table names every row the registry can hold [documented: `settings_layout.rs::SECTIONS`],
109 with #1966's `quality` and `enhanced`,
#2014's nine `fps-counter-*` rows, #2242's `fps-counter-upscaling` and #2317's
`frame-generation-frames` (`KNOWN` in
`settings_layout.rs`); a test keeps
it so (`settings_layout.rs`, `the_table_names_each_key_once_in_one_of_the_tabs`). The
original puts the HUD and Pip-Boy colours and the crosshair in its Display category
[observed: #1953], hence Interface there.

**How the table is applied** (`resolve_layout`) [design: #1953]:

1. Each section takes the rows of its keys that are registered; a missing optional row
   (lip sync without lip data, the draw distances without `DrawDistances`, …) is left
   out, a section left with none is dropped.
2. All seven tabs are there, an empty one too (Tweaks today).
3. **A row the table does not name** keeps the group it registered with as its tab, with
   no heading, after the tab's named rows, in registry order. A group that is none of the
   seven becomes a tab of its own after Debug, in the order such groups first appear in
   the registry. No group is renamed: an unnamed `Sound` row shows under a `Sound` tab,
   not under Audio.
4. Every registered row is listed exactly once.

So #1892's tweaks, registered under `Tweaks`, show under Tweaks in registry order without
#1892 naming them here.

**The Tweaks list** (#1953 point 4, a later change): `Placement` has a `subgroup`, `None`
throughout today. The later change gives Tweaks rows a placement `Tweaks › <group> ›
<heading>` in `SECTIONS` and draws the groups in place of the rail; neither the
registrations nor `resolve_layout`'s interface change for it.

## 3. What the page lists (`settings_view.rs`)

- **A row** (`RowId`) is a registered setting by its registry index, or one of the
  Controls tab's own rows by what it is (`ControlRow`: `Device`, `Binding(control,
  device)`, `Invert`, `Sensitivity(device)`, `Rumble`), never by where it stands.
- **A line** (`ViewEntry`) is a heading or a row. A heading takes one of the window's nine
  lines and counts in the scroll bar, but it is never chosen: no box, no pointer, no
  value [design: #1953]. It is drawn dim in the page's font (font 7, `Shade::Dim`).
- **What is listed** (`SettingsScope`): a tab, or the search's results or the Changed
  list (§6, §7). These two list rows of every tab, each under a heading saying where it
  lives (`listing`: `Gameplay > Dialogue`, `Controls`); an empty one says "No results."
  or "No changed settings.", an empty tab "No settings here.".
- **What is shown** (`Visibility`, worked out each frame from the game's state): a
  colour's red, green and blue only while the colour is Custom (§5); the four
  controller rows of the Controls tab only with Controller as the input device. A hidden
  row keeps its tab and heading. The choice on a hidden row moves to the row that stands
  for it (`parent_of`: the colour's list; the Input Device row), on a control of the other
  device to that control's row of the device in use; else to the first row.
- **`normalize`** makes the page whole for the frame: on a tab, the tab is the chosen
  row's (a test aid or a search may choose a row of another tab), the choice is repaired
  as above, the lines listed are returned. The page runs it before and after the frame's
  keys, and again in drawing, after the frame's changes (so a slider hidden by a colour
  choice gives the choice back to its colour at once; an open dropdown or a drag on a row
  no longer listed closes).

## 4. The keys, the pointer and the window

| Action | Keyboard | Controller | Pointer |
|---|---|---|---|
| Row before/after (round the ends, headings passed) | Up/Down, W/S | D-pad Up/Down | moving onto a row |
| First row under the heading before/after (round) | Page Up/Page Down | LB/RB | "Previous heading", "Next heading" over the rows |
| Tab before/after (round) | Shift+Tab or F1 / Tab | LT/RT | a click on a tab |
| Keys onto the rail and back | F6 | Y | — |
| On the rail: line before/after; take it | Up/Down; Enter | D-pad; A | a click |
| Act on the row | Enter, Space, Left/Right (A/D on sliders and lists), Delete | A, D-pad Left/Right, X | a click, the arrows, the meter |
| Back | Esc, Backspace | B | Back |

[design: #1953] The controller's keys are the pause menu's own in `pad.rs`
(`menu_buttons(PAUSE_AT)`: X Delete, LT F1, RT Tab, Y F6; LB and RB the shared Page Up and
Page Down). A tab without headings is one section: Page Up/Down go to its first row. The
rail's lines are Search, Changed, the tabs, then Defaults and Back (those two keep their
place at the bottom right); with a controller in use the rail's keys pass over Search.
Ctrl+F opens the search (§6).

The window shows nine lines from `PauseView::scroll`: the keys keep the chosen row in it,
and its heading too when it is the heading's first row (`follow`); a heading is never drawn
alone on the window's last line (`drawn_slots`: it comes with its first row in the next
window); the wheel and the scroll bar move the window, the choice moving to the nearest row
still drawn (`clamp_into_window`), and the window stays where they put it: the heading over
the chosen row is brought back only when the choice changes (`window_first`, after
`PauseView::followed`). The pointer takes a row, its meter or its arrows only
when the row is drawn in the current window.

**One dispatcher** (`pause.rs` `act_on_row`, `RowAction::{Activate, Step, Clear}`) does
what the keys, the pointer and the aids ask of a row, a registered row or a Controls row
alike, so the search and the Changed list act on their rows the same way, drawn by the
same `row_line`.

[design, #2200; `settings_view.rs::ControlRow::rows`] The Controls tab lists Input Device,
Automatic Input Switching (default ON), the 29 controls, Invert Y, the device's sensitivity
and Vertical Sensitivity, and with a controller Rumble, then the four controller rows.
Automatic Input Switching selects and saves Input Device on fresh use only when the old
device was idle at frame start and has no release in that frame. Release its buttons and
return sticks/triggers to neutral, then press again on a later frame. Only the switching
press edge is ignored; later holds/repeats and all releases work (controller.md section 1).
Turning it off keeps device selection manual. Its Defaults puts back the controls of the device in use (`bindings::reset`, as before) and the tab's
registered rows; Defaults on every other tab puts back every registered row, as before
(`pause menu: settings back to their defaults`). A control waiting for its new key is
`pause::Listening` (the pad presses no menu keys meanwhile).

**A list's choices** [design: #2198, step 7 of docs/design/dlss-upscaling.md]: besides a
whole row being disabled with a status in place of its value (`set_disabled_status`, the
anti-aliasing row's "Off (upscaling active)" while an upscaler draws), a list row can show
its choices by other words than their names (`set_choice_labels`) and hold some out of reach
for a reason (`set_choice_unavailable`), both kept per row key in the registry, so the other
lists draw as before. The names stay what the INI, the log and the test aids use; only the
words shown change (the Upscaler row shows its requested `DLSS` as "DLSS (restart)" while a
restart is pending for it, the long reason in the row's note). A choice out of
reach is drawn dim in the open list, and in the row's box when it is the chosen one; its
reason is the note under the rows while the keys or the pointer are on it; Enter, A or a
click leave the list open, and Left/Right on the row pass over it. `settings::pick` refuses
it centrally, also by a name in another case (`settings: <key> <name> not taken: <reason>`),
so the dropdown, Left/Right, Defaults' fallback by index and the `NVF_PLAY_TEST_SETTINGS` aid
cannot take it. Loading the INI does not go through that check: a stored choice out of reach
still loads, so a request survives a run in which it cannot be met.

**A choice that is an action** [design: #2250]: a list's `names` may hold a choice that does
something instead of being kept, as Reports' "Delete them now" does (`reports/delete.rs`).
The Upscaler row lists `Download DLSS...` (shown as "Download DLSS…", or "Downloading DLSS…
n%" while its download runs) while DLSS is out of reach only because its files are missing
(`upscaler::Unavailable::FilesMissing`, Windows with Vulkan) and no download installed them
this session. Its `pick` asks for nvforever's `dlss_download` dialog (over the page as the
hand report's form is, `pause::FormOver`) and leaves `Graphics::upscaler` as it is. The row
marks it as an action (`set_choice_action`): `settings::pick` hands it to the list's `pick` and
then neither owns the row nor writes the INI, so a run whose command line set another upscaler
does not write that over the stored choice (`KeptRows`); the log says `settings: upscaler
Download DLSS... (an action; nothing kept)`, or `... not taken` where it is not offered. The
test aids name it `upscaler=Download DLSS...`. The dialog and its test aid
`NVF_PLAY_TEST_DLSS_DOWNLOAD` are described in docs/dlss.md and
`engine/nvforever/src/dlss_download/`.

**A list that took over from an older row** [design: #2198's play-test]: the Upscaler and
Upscaling mode rows replaced the one `fsr-upscaling` row. `set_older_key` lets a list read
the older row's INI key while its own is not in the file (its `load` takes the older text;
the row is written under its own key from then on, the older line left as it is). Where the
older row's one value stands for several rows, `set_older_split` splits it into each row's
own value before the INI is read (`with_older_split`: only while the row of `set_older_key`
lacks its own key, and only for rows whose own key is missing too), so each row loads, and in
a test run keeps (`KeptRows`), a value of its own [design: from the review of 2026-11-12: the
Upscaler row alone kept the older text, and a save then wrote the command line's quality
beside it]. And
`set_list_alias` lets the test aids still name the older row (`fsr-upscaling=<choice>`):
`pick_alias` takes the older row's choice as `pick` takes a list's, refused with a log line
while out of reach, logged as `settings: <older key> <choice>`, the rows it sets the
player's and kept in the INI. The older key is no row of its own: it is not on the page, in
the search or in the Changed list.

## 5. Custom colours (`ui_colour.rs`)

The HUD's and the Pip-Boy's colour lists always offer Custom [design: #1953]. Taking it
keeps the colour as it is and shows its three sliders under it; taking a named colour sets
it and hides them. A colour is Custom while it was chosen so (`CustomColours`, also set
when a slider moves, so the sliders stay up as they pass a named colour's values) or while
its values are none of the named colours (an INI's, a test aid's). `hud-colour=Custom` in
the INI keeps the choice; the six channel keys keep the values; there is no new key.
Defaults loads the start's text (`Amber`) and so leaves Custom. A hidden slider keeps its
key: the INI and a test aid (`pipboy-red=0.2`) set it as before.

## 6. The search (#1953 point 5)

**Opening and closing** [design: #1953]: the rail's Search line (a click, or Enter on it
with the keys on the rail), or Ctrl+F anywhere on the page. A key being awaited, an open
dropdown and the keep question keep the keys until they are done, so Ctrl+F does nothing
then. Esc (a controller's B), Back or the rail's Back closes the search, clears its text
and shows what was listed before, with the same row chosen and the same first line; it
goes no further (the settings page stays; only with the search closed does Back leave
it). Taking a tab or the Changed list closes it too.

**The text** is the window's (`KeyboardInput`), read every frame in `PreUpdate`, whether
the search is open or not, so nothing typed before it opened comes in later
(`pause::collect_search_keys`). A pressed key's text goes in (repeats too) unless Ctrl,
Alt or the logo key is held, or it is a control character, or the game's fonts cannot draw
it (`fnv_util::text::encodable`, as the console); Backspace takes the last letter off.
A frame's keys are taken in the order typed: text and Backspace go into the text, Enter
moves from the text to the results, Esc closes the search and drops what came after it
(an Enter alone on the results is theirs: it acts on the chosen row). The commands that
come as keys without text (a controller's B, A and D-pad Down, the aids' `back`, `pick`,
`down`) are taken after that frame's text, never lost to it. A click on the pause column's
entries from an open search closes it as Esc does, and that frame's keys reach no other
page.
Ctrl+F's F is not typed: the search opens after that frame's text was read.

**Who has the keys**: while the search is open the letters are its own. On the pause menu
the search is looked at before every other key: what was typed goes into the text (and
the keys back into it), Esc closes it; in its text Down or Enter put the keys on the
results and every other key does nothing. W/S, A/D and Space are no keys of the page there
(Up/Down, Left/Right and Enter are). The game's controls read no key while it is open
(`bindings::KeyboardCapture`: `ControlInput::suppressed()` and `just_released()`), set in
`PreUpdate` before the game's input and still on the frame the search closes.

**Matching** (`settings_view::search`) [design: #1953]: a row's words are its label and
note in the language shown and in English and its INI key (a tweak's note names its
Stewie's Tweaks key); the Controls tab's own rows have their label, note and INI name
(`pause::control_words`), both devices' controls, so a controller's button is found from
the keyboard. The query is split at whitespace and lowercased (Unicode); a row is found
when every word is somewhere in its words. No stemming, no ranking: the results are in the
page's order (`catalogue`: tab by tab, the Controls tab's own rows first on it). An empty
text lists every row. A row hidden where it lives is listed as the row that stands for it
(`shown_as`: a colour's channel as its colour; a controller row, a control's binding or the
sensitivity of the device not in use, and Rumble without a controller as Input Device),
once, in the page's order of the rows shown. Its note then begins with what it stands for
(`stands_for`: "Stands for: Jump (Controller)."). So a result of the other device is never
edited through the device in use: acting on it switches the device, and the row is then
listed as itself.

The search's line takes the place of the "Previous heading" and "Next heading" links, the
rows' whole width; Page Up/Down (LB/RB) still go from heading to heading of the results.

**On the results** the keys act as on the row's own tab (Enter, Left/Right, Delete, a
click, the dropdown, the keep question, a key's rebinding), Page Up/Down go from place to
place, and the search stays open; typing goes back to the text, a click on the search's
line too. Defaults on the search is the page's global one.

**A controller** types nothing yet: the rail's keys pass over Search, which says
"Keyboard search". A keyboard can still open it with a controller in use, and the
controller then works its results (D-pad, A, B).

## 7. The Changed list (#1953 point 6)

**What it lists** [design: #1953]: every row set away from what Defaults puts back, of
every tab, hidden and disabled rows too, in the page's order, under where each lives:

| Row | Away from its default when |
|---|---|
| A switch | its state differs from Defaults' (`StartStates`, or the states the page was first shown with) |
| A slider | its value as `Slider::fit` keeps it differs (a getter's rounding is no change) |
| A list | its state as the INI keeps it (`ChoiceList::save`) differs from the start's (`StartChoices`; taken when the page is first shown when there is none) |
| A control's key and mouse button; its controller button | it is not the game's (`Bindings::game_defaults`), each device's its own row |
| Invert Y, Rumble, Input Device, each sensitivity | as the game has it by default |
| Automatic Input Switching | OFF, away from its ON default [design, #2200; `bindings::changed_controls`] |

(`settings::changed_rows`, `bindings::changed_controls`.) A disabled row counts by its
saved state, not by the words shown in its place. A hidden row is listed as the row that
stands for it, as on the search; the log and Defaults go by the rows themselves.

**Defaults on the Changed list** puts back exactly the rows it lists, wherever they are
drawn: the registered rows by `settings::restore` (the resolution's keep question as
ever), twice over so a row another one held disabled (the anti-aliasing under FSR) goes
back once that one has, the Controls rows by `bindings::restore_rows`, each set to the
game's directly with no swap (a control's key goes back without taking another's). A row
still disabled stays as it is, and stays listed. The page's other Defaults are as before:
on the Controls tab its controls and registered rows, elsewhere every registered row,
logged `pause menu: settings back to their defaults`.

A row put back leaves the list; the choice goes to the row after it (else the one before).

## 8. Where the page opens; the rail under the pointer

**Where it opens** [design: #1953, the fork owner's request 2026-11-09]: Settings (the
pause menu's column, S, the main menu's Settings, a script's ShowPauseMenu 3) opens the
page where it was left this run: the same tab (or the Changed list), the same chosen row
and first line (`pause::open_settings`, `PauseView::last`). Left from the search, it opens
on what the search came from; the search's text is not kept. A remembered row no longer
listed gives way to the tab's first row. Continue and a save's load start NVForever again,
so the **tab** is also kept in `NVForever.ini` as `[UI] SettingsTab=<tab>` (its English
name, beside the other UI switch, `[UI] TileMenus`), written when the tab shown changes
and read at start (`pause::SettingsTabStore`); the first opening of a run is on that tab,
an unknown or missing name on the first tab. As every INI line here, an unattended run
in the player's own home reads and writes none (`settings::SettingsFile`). The Changed
list and the search are not kept across runs.

**The rail under the pointer** [design: #1953; as the screen menu's page names,
docs/design/screen-menu.md §3.1, and the original's lists' mouse-over sound,
list_box_template.xml `mouseoversound`]: a rail line (Search, Changed, a tab) under the
pointer is drawn bright with the highlight box's line round it, unfilled, and the focus
sound plays once as the pointer comes onto it (not again while it rests there). The line
shown keeps its bright look, the keys' line on the rail its filled box; Defaults and Back
are as before. Resting on a line takes nothing: a click does.

## 9. Test aids and the log

`NVF_PLAY_TEST_SETTINGS` adds `tab:<name>`, `select:<key>`, `layout`, `search:<text>`
(the search open on that text, the keys on its results) and `changed`;
`NVF_PLAY_TEST_PAUSE_MENU` adds `heading-down`, `heading-up`, `tab-down`, `tab-up`, `rail`,
`left`, `right`, `pad:<button>` (the key the button presses on the pause menu), `search`
(the rail's Search), `settings` (the column's Settings: the page where it was left) and
`inspect` (also the rail line hovered); they press the keys they stand for. Real input, the next
frame: `type:<text>` (each character's key pressed with its text, as the window's
`KeyboardInput` messages before Bevy reads them, so the keys' state has the letters too
and the search's hold on them is tested), `backspace`, `ctrl-f`; `point:<target>` (the
pointer onto the middle of what was drawn for `search`, `search-field`, `changed`,
`defaults`, `back`, `heading-up`, `heading-down`, `tab:<name>`, `row:<key>`, `choice:<n>`),
`press` and `release` (the left button, before the UI looks under the pointer, as the
Wait menu's aid). The existing aids keep their grammar and their log lines. The new lines:

```text
settings layout: tabs Gameplay, Display, Audio, Controls, Graphics, Tweaks, Debug
settings layout: menu-pause -> Gameplay > Dialogue
settings layout: 87 row(s) in 7 tab(s), 0 not in the table
pause menu: tab Display; selected resolution
pause menu: heading Gameplay > Saving; selected autosave-travel
pause menu: the keys on the rail (Display)
pause menu: rail Changed
pause menu: colour sliders HUD shown, Pip-Boy hidden
pause menu: inspect: Display, keys on the rows, selected hud-colour, lines 1-9 of 21
pause menu: search open
pause menu: search "menu-pause": 1 result(s); keys menu-pause
pause menu: the keys on the results
pause menu: search closed
pause menu: list Changed; selected difficulty
pause menu: changed 3 row(s); keys difficulty,crosshair-style,god-mode
pause menu: changed defaults; keys difficulty,crosshair-style,god-mode
controls: padbind:Jump, rumble back to the game's
pause menu: the pointer on heading-down
pause menu: settings open on Debug; selected god-mode
settings page: [UI] SettingsTab=Graphics kept in <home>/NVForever.ini
settings page: [UI] SettingsTab=Graphics read from <home>/NVForever.ini
pause menu: hover Debug
pause menu: inspect: hovered Debug
```

Smoke scenarios: `settings-layout`, `settings-headings`, `settings-scroll-headings`, `settings-custom-sliders`,
`settings-controller-navigation`, `settings-search`, `settings-search-input`,
`settings-changed`, `settings-heading-pointer`, `settings-search-language`,
`settings-reopen`, `settings-tab-write` and `settings-tab-read` (a pair), `settings-rail-hover`
(tools/smoke/scenarios.toml).


## Performance usage availability (#2249)

[design, #2249: `engine/nvforever/src/perf_overlay/{usage,readings,mod}.rs`,
`engine/nvforever/src/settings.rs`] The existing switches and defaults stay the same.
CPU and GPU rows each say **Not available** when their reading is missing, including
while an initial rate awaits its second sample. A CPU row can still show the game's
cores when the machine reading is missing. These are NVForever's own words; the new
unavailable and game-GPU wording uses `lang/*.toml`.

| Existing setting | Systems and scope [design: `perf_overlay/usage.rs`] |
|---|---|
| `fps-counter-cpu` | Windows PDH machine use and process time; Linux `/proc/stat` machine use and `/proc/self/stat` process time. Other systems show Not available. |
| `fps-counter-gpu` | Windows PDH busiest engine over all programs. Linux tries runtime NVML, readable amdgpu `gpu_busy_percent`, then DRM client nanosecond counters. Unsupported telemetry shows Not available. |

[design, #2249] `usage::linux::GPU_SOURCE_ORDER` prefers NVML then amdgpu to keep
whole-device use where available; DRM is the fallback and shows **game 71%**, for
example. NVML and amdgpu take the busiest readable device; these readers do not match
a device to the renderer on a multi-GPU machine. Selection occurs at the first GPU
request, logs once (including no available source), and keeps its reader. DRM rescans
fdinfo each requested second so clients opened later can start reporting. A lost
selected source shows unavailable; restarting retries the higher-priority sources.
Hidden overlays and disabled GPU rows reset DRM rate baselines. The first numeric
Windows usage log stays unchanged; a separate availability log waits at most ten
half-second updates with both usage rows enabled, then spells out missing parts.

[documented: Linux kernel, [DRM client usage stats](https://docs.kernel.org/gpu/drm-usage-stats.html)]
The usage and identity fields are optional: `drm-engine-<name>` gives client busy nanoseconds,
`drm-engine-capacity-<name>` gives grouped engine capacity (default one), and
`drm-client-id` with `drm-pdev` identifies duplicate client descriptors. Counters can
briefly decrease; readers must retain their high-water mark.
[design, #2249: `perf_overlay/drm_usage.rs`] We sum distinct clients per device and
engine, divide busy-time deltas by elapsed time and capacity, choose the busiest
engine and clamp to 0-100%. New clients need two samples; unreadable files, malformed
units, missing client IDs or unsupported counters cannot supply a percentage.
Without a PCI device identity, clients are kept separate when choosing the busiest
engine, so work on unknown devices is not added together [design, #2249].
This applies to drivers exporting these fields (including supporting amdgpu, i915/xe
and nouveau versions); a driver name alone does not guarantee telemetry.

[documented: NVIDIA, [NVML Device Queries](https://docs.nvidia.com/deploy/nvml-api/620/api/group__nvmlDeviceQueries.html),
[Initialization and Cleanup](https://docs.nvidia.com/deploy/nvml-api/620/api/group__nvmlInitializationAndCleanup.html),
[nvmlUtilization_t](https://docs.nvidia.com/deploy/nvml-api/620/api/structnvmlUtilization__t.html)]
NVML supplies device GPU utilization on supported NVIDIA proprietary drivers. We load
`libnvidia-ml.so.1`, initialize with `nvmlInit_v2`, enumerate with
`nvmlDeviceGetCount_v2` / `nvmlDeviceGetHandleByIndex_v2`, and read
`nvmlDeviceGetUtilizationRates`; unavailable libraries, symbols, permissions and
unsupported utilization allow fallback [design: `perf_overlay/nvml.rs`].

[design, #2249] Synthetic fixtures test parser/arithmetic on every test platform.
Smoke scenario `perf-overlay-usage-availability` accepts a numeric or explicit missing
part independently. The older `fps-counter` numeric expectation is preserved.
Linux validation still needs Intel/nouveau game-only rates and labels, NVIDIA with
and without NVML, amdgpu fallback, no telemetry, and hide/show or GPU-row off/on.
