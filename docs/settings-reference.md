# Settings reference

The settings page (the pause menu's Settings) shows each row with a short note: what it does,
in one or two sentences [design: #2377, docs/design/settings-page.md §10]. This page keeps
the rest: for each row, by its INI key (`[Settings]` of `NVForever.ini`), where the page shows
it (tab and heading), its label, the note the page shows, the longer explanation the note
had before #2377 where it said more, and the command-line switches whose state the row
replaces (a test run that gives one keeps the command line's state over the INI's,
docs/design/settings-page.md §1). Rows are listed in the page's order
(`engine/nvf-play/src/settings_layout.rs`, `SECTIONS`). The words are NVForever's own; the
facts are the rows' (each row's module, named under its key). A row whose module is not part
of a run (lip sync without lip data, the draw distances, the quality presets without the
install's preset files) is not on the page then.

The Controls tab's own rows (Input Device, the controls, Invert Y, the sensitivities, Rumble)
are at the end.

## Gameplay › Game

### `difficulty`: Difficulty

- On the page: How hard the fights are: from Very Easy (your hits do double damage, you take
  half) to Very Hard (half and double); Normal as they are.
- In full: How hard the fights are: from Very Easy (the player's hits do double damage and
  the player takes half) to Very Hard (half and double); Normal as they are. (console:
  SetGameDifficulty 0-4)
- Registered in `engine/nvf-play/src/difficulty.rs`

### `hardcore`: Hardcore mode

- On the page: On: you need water, food and sleep, stimpaks heal over time, ammo has weight,
  crippled limbs need a doctor's bag or a doctor. Off: none of that.
- In full: On: you need water, food and sleep, stimpaks heal over time, ammunition has
  weight and crippled limbs heal only with a doctor's bag or a doctor. Off: none of that;
  turning it off clears your thirst, hunger and tiredness and their effects. (console:
  SetHardcore 1/0)
- Registered in `engine/nvf-play/src/hardcore.rs`

### `kill-camera`: Kill camera

- On the page: When a VATS shot kills its last mark, a camera shows the kill. Last enemy:
  only when no other enemy still fights you. Every kill: always. Off: never.
- In full: When a VATS shot kills its last mark, a cinematic camera can show the kill. Last
  enemy: only when no other enemy is still fighting you. Every kill: whoever is left. Off:
  never.
- Registered in `engine/nvf-play/src/kill_cam.rs`

### `vats-style`: V.A.T.S. style

- On the page: What the V.A.T.S. key starts. Classic: the world stops and you pick body
  parts to hit. Mark and Fire: time runs very slowly while you mark targets.
- In full: What the V.A.T.S. key starts. Classic: the world stops, you pick a target and its
  body parts from their chances to hit, then the queued attacks play. Mark and Fire: time
  runs very slowly while you mark targets under the crosshair.
- Registered in `engine/nvf-play/src/vats_classic.rs`

### `vats-slowdown`: V.A.T.S. time slowdown

- On the page: How far the world slows while you choose targets in Classic: 100% stops it,
  50% runs at half speed. Enemies attack at anything less than a stop.
- In full: How far the world slows while you choose targets in the Classic style: 100% stops
  it, 50% runs it at half speed. Enemies keep attacking at anything less than a full stop.
- Registered in `engine/nvf-play/src/vats_classic.rs`

### `reload-slows`: Slower while reloading

- On the page: On: you and the NPCs move at 75% of your speed while a reload runs. Off:
  reloading does not slow anyone, as in the original.
- Registered in `engine/nvf-play/src/combat.rs`

### `UsePipBoy`: Use Pip-Boy

- On the page: On: the Pip-Boy is worn and the menus come up on the arm. Off: it is taken
  off and the menus show full screen, as always without a Pip-Boy.
- In full: On: the Pip-Boy and its glove are worn and the menus come up on the arm. Off:
  both are taken off and the menus show full screen. Without a Pip-Boy worn the menus are
  always full screen.
- Registered in `engine/nvf-play/src/pipboy.rs`

## Gameplay › Saving

### `autosave-travel`: Autosave after travel

- On the page: On: the game saves to the autosave slot after each loading screen (doors,
  coc, a script moving you); fast travel has its own row.
- Registered in `engine/nvf-play/src/save.rs`

### `autosave-fast-travel`: Save on fast travel

- On the page: On: the game saves to the autosave slot when a fast travel arrives.
- Registered in `engine/nvf-play/src/save.rs`

### `autosave-wait`: Save on wait

- On the page: On: the game saves to the autosave slot when a wait ends with at least one
  hour passed.
- Registered in `engine/nvf-play/src/save.rs`

### `autosave-sleep`: Save on sleep

- On the page: On: the game saves to the autosave slot when the player wakes from a sleep of
  at least one hour.
- Registered in `engine/nvf-play/src/save.rs`

### `autosave-scripts`: Save when a script asks

- On the page: On: a script's Autosave saves to the autosave slot (quest milestones),
  SystemSave to the system slot, ForceSave as a new save.
- Registered in `engine/nvf-play/src/save.rs`

## Gameplay › Dialogue

### `menu-pause`: Freeze the picture in dialogue and menus

- On the page: On: animations, wind, water and sky stand still in conversations, the
  inventory and other menus, as in the original. Off: only game logic waits.
- In full: On: while a conversation, the inventory, a container or a message box is open,
  animations, wind, water and sky stand still, as in the original. Off: only game logic
  waits and the world lives on. This menu always holds everything.
- Command-line switches: `--menu-pause`
- Registered in `engine/nvf-play/src/pause.rs`

### `dialogue-goodbye-hint`: Goodbye hint

- On the page: On: "[Tab] Goodbye" stands at the left of the dialogue box while a Goodbye
  choice is offered. Off: hidden; Tab still says goodbye.
- In full: On: while a conversation offers a Goodbye choice, "[Tab] Goodbye" stands at the
  left of the dialogue box. Off: the words are hidden; Tab still says goodbye.
- Registered in `engine/nvf-play/src/dialogue_menu.rs`

### `dialogue-look`: Turn to each other in dialogue

- On the page: On: at the start of a conversation the actor turns to the player and the view
  turns to the actor.
- Command-line switches: `--no-dialogue-look`
- Registered in `engine/nvf-play/src/dialogue_look.rs`

### `dialogue-recentre`: Bring the view back when choices appear

- On the page: On: when choices appear the view returns to the speaker (with the close-up
  off). Needs "Turn to each other in dialogue".
- In full: On: with "Close in on the speaker in dialogue" off, after you look away the view
  returns to the speaker when choices appear. Needs "Turn to each other in dialogue".
- Command-line switches: `--dialogue-recentre`
- Registered in `engine/nvf-play/src/dialogue_look.rs`

### `dialogue-zoom`: Close in on the speaker in dialogue

- On the page: On: in a conversation the view closes in on the speaker, the background out
  of focus; the mouse moves only the cursor. Off: you can look around.
- In full: On: in a conversation the view narrows onto the speaker, with what is behind them
  out of focus; from third person the camera comes to the eyes. The view stays on the
  speaker and the mouse moves only the cursor. Off: no close-up; you can look around.
- Command-line switches: `--no-dialogue-zoom`
- Registered in `engine/nvf-play/src/dialogue_look.rs`

### `lip-sync`: Lip sync

- On the page: On: a speaker's face moves with the line. Off: a face that is moving eases
  back to rest.
- Command-line switches: `--no-lip`
- Registered in `engine/nvforever/src/settings.rs`

## Gameplay › Reports

### `send-reports`: Send reports to help fix NVForever

- On the page: After each session: build, hardware, settings, mod list, how it ran and the
  log; no personal details, no saves. From the next start
- More: docs/design/player-reports.md
- Registered in `engine/nvforever/src/reports/mod.rs`

### `reports-delete`: Reports already sent

- On the page: Delete them now asks the server to delete every report this PC sent (within 7
  days) and drops unsent ones; later reports get a new ID.
- In full: Delete them now asks the server to delete every report this PC sent (done within
  7 days) and drops the ones not sent yet; later reports, this session's too, get a new ID
- More: docs/design/player-reports.md
- Registered in `engine/nvforever/src/reports/delete.rs`

## Gameplay › Camera

### `detached-camera`: Detached third-person camera

- On the page: On: in third person with your weapon away the mouse turns the camera around
  you and you run whichever way you push. Off: the camera stays behind you.
- In full: On: in third person with your weapon away the mouse turns the camera around you,
  not your character, and your character turns to run whichever way you push, as the camera
  sees it. Drawing your weapon turns you to where the camera looks. Off: the camera stays
  behind you, as in the original.
- Registered in `engine/nvf-play/src/player.rs`

### `true-iron-sights`: True iron sights

- On the page: On: aiming a gun lines its sights up with your eye and hides the crosshair.
  Off: the gun is held low with the crosshair, as in the original.
- In full: On: aiming a gun lines its sights up with your eye. The crosshair is hidden while
  you aim down them. Off: the gun is held low with the crosshair, as in the original. Guns
  with no sights to look down (laser rifle, minigun) aim as the original either way.
- Command-line switches: `--no-true-iron-sights`
- Registered in `engine/nvforever/src/settings.rs`

### `first-person-body`: Body in first person

- On the page: On: in first person you see your own body and legs when you look down. Off:
  only the arms and the weapon are drawn, as in the original.
- Command-line switches: `--first-person-body`
- Registered in `engine/nvforever/src/settings.rs`

### `weapon-sway`: Weapon sway

- On the page: On: in first person the arms and weapon lag behind a turn and bob as you walk
  and run. Off: they stay still in the view.
- Command-line switches: `--no-weapon-sway`
- Registered in `engine/nvforever/src/settings.rs`

### `head-bob`: Head bob

- On the page: On: in first person the view dips with each step as you walk and run. Off: it
  stays level.
- Command-line switches: `--no-head-bob`
- Registered in `engine/nvforever/src/settings.rs`

### `weapon-lower`: Lower weapon near walls

- On the page: On: in first person a drawn weapon lowers at a wall or a friend and comes
  back up when you aim or fire. Off: it stays up, as in the original.
- In full: On: in first person a drawn weapon is lowered when a wall or a friendly person is
  right in front of you, and comes straight back up when you aim or fire. Off: it stays up
  and can poke through them, as in the original. Lower Weapon (H) lowers or raises it by
  hand either way.
- Command-line switches: `--no-weapon-lower`
- Registered in `engine/nvforever/src/settings.rs`

### `field-of-view`: Field of view

- On the page: The game's field of view in degrees across a 4:3 picture (75 as the
  original's default); wider screens see more at the sides.
- Command-line switches: `--fov`
- Registered in `engine/nvforever/src/settings.rs`

## Display › Screen

### `resolution`: Resolution

- On the page: The window's size, or in full screen the monitor's (its own size:
  borderless). A new one goes back after 20 seconds unless you keep it.
- Command-line switches: `--size`
- Registered in `engine/nvforever/src/settings.rs`

### `borderless`: Full screen

- On the page: On: the game fills its monitor without a frame, at the full screen resolution
  above. Off: a window of the window resolution.
- In full: On: the game fills the monitor it is on, without a frame, at the resolution
  chosen above for full screen. Off: a window of the resolution chosen for a window.
- Command-line switches: `--window`
- Registered in `engine/nvforever/src/settings.rs`

### `monitor`: Monitor

- On the page: The display the game opens on in full screen (the first display until you
  pick one).
- Command-line switches: `--window`
- Registered in `engine/nvforever/src/settings.rs`

### `display-sync`: Display sync

- On the page: Vertical sync: frames wait for the display (no tearing). Variable refresh:
  for a G-Sync or FreeSync screen. Off: frames show at once (may tear).
- In full: Vertical sync: each frame waits for the display's refresh (no tearing). Variable
  refresh: for a G-Sync or FreeSync screen, vertical sync with the frame rate held a few
  frames under the screen's refresh rate, so frames are shown as soon as they are drawn
  without tearing. Off: frames are shown as soon as they are drawn (may tear).
- Registered in `engine/nvforever/src/settings.rs`

### `frame-cap`: Frame rate limit

- On the page: At most this many frames a second (Off: no limit of its own). A lower limit
  keeps the computer cooler and quieter.
- In full: At most this many frames a second (Off: as many as the computer draws, or the
  display's refresh with vertical sync; with variable refresh the lower of this and its own
  limit). A lower limit keeps the computer cooler and quieter.
- Registered in `engine/nvforever/src/settings.rs`

### `ui-scale`: UI scale

- On the page: The size of the HUD and the menus, as a part of the original's. Above 100%
  only the HUD grows. Takes effect at the next start.
- In full: The size of the HUD and the menus, as a part of the original's (100%): smaller
  leaves more of the picture free, each HUD piece kept at its edge of the screen and each
  menu in the middle. Above 100% only the HUD grows: the menus stay at the original's size,
  so they keep on the screen. Takes effect at the next start (loading a save starts the game
  again). Not in the original.
- Registered in `engine/nvf-play/src/interface.rs`

## Display › Interface

### `hud-colour`: HUD colour

- On the page: The colour of the HUD and the menus: Amber (the default) or another; the
  sliders below set it to any colour.
- Registered in `engine/nvf-play/src/ui_colour.rs`

### `hud-red`: HUD red

- On the page: One part of the colour above, 0 to 255; drag the meter or use Left and Right.
- Registered in `engine/nvf-play/src/ui_colour.rs`

### `hud-green`: HUD green

- On the page: One part of the colour above, 0 to 255; drag the meter or use Left and Right.
- Registered in `engine/nvf-play/src/ui_colour.rs`

### `hud-blue`: HUD blue

- On the page: One part of the colour above, 0 to 255; drag the meter or use Left and Right.
- Registered in `engine/nvf-play/src/ui_colour.rs`

### `pipboy-colour`: Pip-Boy colour

- On the page: The colour of the Pip-Boy's screens (items, stats, data): Amber (the default)
  or another; the sliders below set it to any colour.
- Registered in `engine/nvf-play/src/ui_colour.rs`

### `pipboy-red`: Pip-Boy red

- On the page: One part of the colour above, 0 to 255; drag the meter or use Left and Right.
- Registered in `engine/nvf-play/src/ui_colour.rs`

### `pipboy-green`: Pip-Boy green

- On the page: One part of the colour above, 0 to 255; drag the meter or use Left and Right.
- Registered in `engine/nvf-play/src/ui_colour.rs`

### `pipboy-blue`: Pip-Boy blue

- On the page: One part of the colour above, 0 to 255; drag the meter or use Left and Right.
- Registered in `engine/nvf-play/src/ui_colour.rs`

### `language`: Language

- On the page: The language of NVForever's own words (menus, messages). The game's own text
  is the install's.
- Registered in `engine/nvf-play/src/language.rs`

### `dynamic-crosshair`: Dynamic crosshair

- On the page: On: with a gun or thrown weapon drawn the crosshair opens with how wide your
  shots spread now. Off: the arrows stay together.
- In full: On: with a gun or thrown weapon drawn the crosshair opens to show how wide your
  shots spread now (wider running, after shots, with crippled arms; narrower aimed or
  crouched); Crosshair style below picks how it looks. Off: the arrows stay together
  whatever is drawn.
- Registered in `engine/nvf-play/src/reticle.rs`

### `crosshair-style`: Crosshair style

- On the page: With Dynamic crosshair on. Cross: four bars that part with the spread.
  Arrows: the plain crosshair's two arrows part instead.
- In full: With Dynamic crosshair on and a gun or thrown weapon drawn. Cross: four bars that
  part with the spread. Arrows: the plain crosshair's two arrows part instead.
- Registered in `engine/nvf-play/src/reticle.rs`

### `sneak-gauges`: Light and noise gauges while sneaking

- On the page: On: two bars under the sneak word show how lit you are and how much noise you
  make, as detection counts them. Not in the original.
- Registered in `engine/nvf-play/src/sneak_gauges.rs`

## Display › Performance

### `fps-counter`: Frame rate counter

- On the page: On: a panel of performance readings at the top right; the Counter rows below
  choose what it shows. Not in the original.
- In full: On: a small panel of performance readings at the top right of the screen, the
  Counter rows below choosing what it shows; with only its frame rate row on it is a frame
  rate counter. Not in the original.
- Command-line switches: `--fps`
- Registered in `engine/nvforever/src/settings.rs`

### `fps-counter-fps`: Counter: frame rate

- On the page: Frames a second, the mean frame time and the 1% low or the slowest frame (the
  row below chooses), from the time between frames.
- In full: The frames a second and the mean frame time of the last half second, and the 1%
  low (the mean of the slowest hundredth of the frames of the last 10 seconds) or the
  slowest frame of the last 10 seconds, as the row below chooses. Measured from the time
  between frames. While frame generation shows generated frames, the frames shown a second
  come first and the rendered ones in brackets; the frame times are the rendered frames'.
- Registered in `engine/nvforever/src/settings.rs`

### `fps-counter-low`: Counter: last figure

- On the page: The frame rate row's last figure, over 10 seconds: 1% low (the mean of the
  slowest hundredth of frames) or the slowest frame.
- In full: The frame rate row's last figure, over the last 10 seconds: 1% low, the mean of
  the slowest hundredth of the frames; Slowest frame, the single slowest.
- Registered in `engine/nvforever/src/settings.rs`

### `fps-counter-graph`: Counter: frame graph

- On the page: The frame times of the last 5 seconds as a line under the frame rate, the
  newest at the right, a faint line at 60 Hz (16.7 ms).
- In full: The frame times of the last 5 seconds as a line under the frame rate, the newest
  at the right, with a faint line at a 60 Hz frame (16.7 ms); its top is the slowest frame
  drawn, or two 60 Hz frames when every frame was faster, so a spike reaches the top.
  Measured from the time between frames, drawn again twice a second.
- Registered in `engine/nvforever/src/settings.rs`

### `fps-counter-split`: Counter: frame split

- On the page: What a frame took on the main thread, the render thread (its wait for the
  window in brackets) and the GPU. The GPU part needs this row on at the start.
- In full: What a frame took on the main thread (the game's own work), on the render thread
  (preparing the picture, including the wait for the window's next image, shown in brackets:
  with vertical sync that wait is the display's time, not work) and on the graphics card
  (the render passes it times, from GPU timestamps), each the mean over half a second. A
  frame held up by the processor has a long main time, or a long render time beside a short
  wait; the GPU time is what tells a frame held up by the graphics card. The GPU part needs
  this row and the counter on when the game starts (it takes effect at the next start), and
  a graphics card that offers timestamps; without them it is left out. Unless a measuring
  aid also uses them, the timestamps are taken only while the row shows.
- Registered in `engine/nvforever/src/settings.rs`

### `fps-counter-cpu`: Counter: CPU use

- On the page: Machine and game CPU use, read once a second. Game use is cores kept busy:
  1.0 is one full core.
- In full: Machine and game CPU use, read once a second while shown. Windows uses
  performance counters; Linux uses /proc. Missing machine readings say Not available. Game
  use is cores kept busy: 1.0 is one full core.
- More: docs/design/settings-page.md (Performance usage availability)
- Registered in `engine/nvforever/src/settings.rs`

### `fps-counter-gpu`: Counter: GPU use

- On the page: GPU use, read once a second: on Windows the busiest engine across programs;
  on Linux the whole device's, or the game's own where the driver tells.
- In full: GPU use, read once a second while shown. Windows: busiest engine across programs.
  Linux: NVIDIA NVML, then amdgpu, then DRM client time. DRM reads game use and says game;
  the other Linux sources read whole-device use. Missing readings say Not available.
- More: docs/design/settings-page.md (Performance usage availability)
- Registered in `engine/nvforever/src/settings.rs`

### `fps-counter-vram`: Counter: GPU memory

- On the page: The graphics memory the game holds against the driver's budget, read every 2
  seconds. Marked ! while near it the game keeps fewer unused assets.
- In full: The graphics memory the game holds against the budget the driver gives it
  (Vulkan's memory budget, read every 2 seconds). It is the driver's figure: it counts the
  blocks kept empty for reuse, so it is what the game holds, not the size of what is drawn.
  Left out on a graphics chip that shares the machine's RAM, which has no budget of its own.
  Marked ! while the game keeps fewer unused models and textures because it is near the
  budget.
- Registered in `engine/nvforever/src/settings.rs`

### `fps-counter-ram`: Counter: RAM

- On the page: The game's RAM and the machine's RAM in use. Marked ! while under a tenth is
  free and the game keeps fewer unused assets. Windows only.
- In full: The game's memory in RAM (its working set, from Windows once a second while the
  counter shows) and the share of the machine's RAM in use (read every 2 seconds). Marked !
  while the game keeps fewer unused models and textures because the machine has under a
  tenth of its RAM free. Windows only.
- Registered in `engine/nvforever/src/settings.rs`

### `fps-counter-loaded`: Counter: loaded

- On the page: What the game holds now: the cells in the physics world, placed objects with
  a model, actors with a skeleton and engine entities.
- In full: What the game holds now: the cells loaded in the physics world, the placed
  objects with a model, the actors with a skeleton and every entity of the engine, counted
  twice a second while the row shows (the stress tests' counts).
- Registered in `engine/nvforever/src/settings.rs`

### `fps-counter-upscaling`: Counter: upscaling

- On the page: The upscaler drawing now: its name and mode, and the sizes the world is drawn
  at and rebuilt to. Off and the window's size while nothing upscales.
- In full: The upscaler that draws now: its name and mode, the size the world is drawn at
  and the size it is rebuilt to (with FSR 3.1 and Bilinear from its mode, with DLSS the size
  DLSS chose), and with DLSS the model asked for. Reads Off and the window's size while
  nothing upscales, also when the upscaler asked for cannot run on this computer. Follows a
  change within a frame or two, showing the upscaler without sizes until then.
- Registered in `engine/nvforever/src/settings.rs`

## Audio

### `volume-master`: Master

- On the page: How loud all sound is: each volume below is a part of this one.
- Registered in `engine/nvf-play/src/audio_out.rs`

### `volume-music`: Music

- On the page: The music of the place you are in and the main menu's theme.
- Registered in `engine/nvf-play/src/audio_out.rs`

### `volume-footstep`: Footstep

- On the page: Footsteps, jumps and landings, yours and everyone's.
- Registered in `engine/nvf-play/src/audio_out.rs`

### `volume-voice`: Voice

- On the page: Spoken lines: conversations and what people say around you.
- Registered in `engine/nvf-play/src/audio_out.rs`

### `volume-effects`: Effects

- On the page: Every other sound: weapons, impacts, menus, doors and the sounds of places.
- Registered in `engine/nvf-play/src/audio_out.rs`

### `volume-radio`: Radio

- On the page: Radio stations, from the Pip-Boy and from radios in the world.
- Registered in `engine/nvf-play/src/audio_out.rs`

### `music`: Music

- On the page: On: the music of the place you are in plays. Off: it is silent and goes on
  unheard.
- Command-line switches: `--no-music`
- Registered in `engine/nvf-play/src/music.rs`

### `voice`: Dialogue voice

- On the page: On: lines are spoken. Off: they are silent from the next line on, and still
  last as long as their sound.
- Command-line switches: `--no-voice`
- Registered in `engine/nvf-play/src/voice.rs`

### `reverb`: Reverb

- On the page: World sounds echo as the place you are in does (a cave, a hall, a small
  room). Off: they do not.
- Command-line switches: `--no-reverb`
- Registered in `engine/nvf-play/src/audio_out.rs`

### `voice-reverb`: Voice reverb

- On the page: What people say around you echoes in the place you are in, as Reverb does for
  world sounds. Conversations stay dry.
- In full: What people say around you echoes in the place you are in, up to the end of each
  line, as Reverb does for world sounds. Conversations stay dry.
- Registered in `engine/nvf-play/src/audio_out.rs`

## Controls
After the Controls tab's own rows (see the end of this page):

### `button-style`: Button style

- On the page: The names and pictures of the controller's buttons in prompts and hints.
  Auto: a PlayStation's when the controller says it is one, else an Xbox's.
- In full: Which names and pictures the controller's buttons go by in prompts, menu hints
  and the Controls tab. Auto: a PlayStation controller's (Cross, Circle, Square, Triangle,
  L1, R2, Select…) when the controller in use says it is one, else an Xbox controller's.
  Xbox or PlayStation: always those. The INI keeps the Xbox names either way.
- Registered in `engine/nvf-play/src/pad.rs`

### `aim-assist`: Aim assist

- On the page: Controller only. On: the right stick turns slower over a nearby hostile, and
  aiming down the sights pulls the view onto one near the crosshair.
- In full: Controller only, never the mouse. On: the right stick turns slower while the
  crosshair is on a nearby hostile, and aiming down the sights turns the view onto one near
  the crosshair for a moment. Off: neither.
- Registered in `engine/nvf-play/src/aim_assist.rs`

### `aim-slowdown`: Aim slowdown

- On the page: How much slower the right stick turns while the crosshair is on a nearby
  hostile (0: not at all).
- Registered in `engine/nvf-play/src/aim_assist.rs`

### `aim-lock`: Aim lock-on

- On the page: How hard aiming down the sights turns the view onto a hostile near the
  crosshair (0: not at all). Push the right stick away to break it.
- Registered in `engine/nvf-play/src/aim_assist.rs`

## Graphics › Quality

### `quality`: Quality preset

- On the page: Sets the rows the original's quality presets cover to the game folder's
  low.ini to VeryHigh.ini. Custom: those rows match none of them.
- In full: Sets the rows the original's quality presets cover to the values of the game
  folder's low.ini, medium.ini, high.ini or VeryHigh.ini: Sun shadows, Shadow filtering,
  Light glow, Mirrored water reflections and the object, actor, item, shadow and grass
  distances. Custom: those rows match none of them. A preset whose file is missing is not
  offered. The enhanced effects, anti-aliasing and FSR keep their own rows.
- More: docs/spec-notes/quality-presets.md
- Registered in `engine/nvforever/src/quality.rs`

## Graphics › Picture

### `enhanced`: Enhanced effects

- On the page: Sets every effect of the enhanced look at once. On: all of them. Off: none,
  the original's picture. Custom: some on, some off. Each keeps its own row.
- In full: On: every effect of the enhanced look (ambient occlusion, longer sun shadows,
  bloom, sharpening, anisotropic filtering 16x, sun shafts, screen-space water reflections).
  Off: none of them, the original's picture. Custom: some on, some off. Each effect keeps
  its own row and its own saved state; this row only sets them all at once.
- More: docs/design/graphics-enhanced.md
- Registered in `engine/nvforever/src/quality.rs`

### `grade`: Colour grade

- On the page: On: the picture is graded by the worldspace's, the weather's and the cell's
  image space. Off: the picture as lit, for comparisons.
- Command-line switches: `--no-grade`
- Registered in `engine/nvforever/src/settings.rs`

### `sun-shadows`: Sun shadows

- On the page: The sun's shadows: Off, Performance (cheaper outdoors; moving shadows update
  at half the frame rate) or Quality. The original casts none from the world.
- In full: The sun's shadows, out to --shadow-distance: Off; Performance (the shadow maps
  drawn every other frame in two cascades, trees casting shadows only near you, the
  first-person arms taking the world's shadows: saves frame time outdoors, shadows of moving
  things update at half the frame rate); or Quality (three cascades, the farther ones drawn
  less often). The original casts none from the world. Quality by default in the game.
- Command-line switches: `--shadow-distance`, `--sun-shadows`, `--no-sun-shadows`
- Registered in `engine/nvforever/src/settings.rs`

### `upscaler`: Upscaler

- On the page: FSR 3.1 or NVIDIA DLSS rebuilds the world at display resolution, Bilinear
  only stretches it, at the mode below. DLSS needs an RTX card and its files in the dlss
  folder.
- In full: Reconstruct the world at display resolution with FSR 3.1 or NVIDIA DLSS, or only
  stretch it there with Bilinear (softer; the cheapest, on any graphics card), at the
  Upscaling mode below. Disables MSAA and TAA while active; FSR 3.1 and DLSS also disable
  FXAA and SMAA. Menus stay sharp. FSR 3.1 and DLSS require Vulkan; unsupported devices keep
  native rendering. DLSS needs an NVIDIA RTX graphics card and NVIDIA's DLSS files in the
  dlss folder beside nvforever.exe (NVForever ships none). With the files there when the
  game starts, every choice takes effect at once; files put there later are found at the
  next start, so DLSS chosen until then reads DLSS (restart) (loading a save starts the game
  again). Sharpening controls FSR sharpening. Frame generation needs FSR 3.1 or DLSS.
- Command-line switches: `--upscaler`, `--upscaler-quality`, `--fsr`
- More: docs/dlss.md, docs/design/fsr-upscaling.md, docs/design/dlss-upscaling.md
- Registered in `engine/nvforever/src/settings.rs`

### `upscaler-quality`: Upscaling mode

- On the page: The share of the screen the world is drawn at; the upscaler rebuilds the rest
  (Bilinear stretches it). Native AA: full size. Quality to Ultra Performance: ever faster.
- In full: The share of the screen's width and height the world is drawn at; the upscaler
  rebuilds the rest. Native AA: full resolution, anti-aliasing only (DLSS: DLAA; Bilinear:
  no change). Quality: the sharpest upscaled look, for 1440p and 4K. Balanced: more speed
  with little visible loss. Performance: the highest frame rate, for 4K or heavy scenes.
  Ultra Performance: for very high resolutions only. Takes effect at once. With Upscaler Off
  there is nothing to set.
- Command-line switches: `--upscaler-quality`, `--upscaler`, `--fsr`
- More: docs/design/fsr-upscaling.md, docs/design/dlss-upscaling.md
- Registered in `engine/nvforever/src/settings.rs`

### `dlss-model`: DLSS model

- On the page: Which DLSS model rebuilds the picture (NVIDIA calls them presets). Default,
  recommended: NVIDIA picks per mode. Only while DLSS draws.
- In full: Which of NVIDIA's DLSS models rebuilds the picture (NVIDIA calls them presets).
  Default (recommended): NVIDIA picks per mode: K for Native AA, Quality and Balanced, M for
  Performance, L for Ultra Performance. K: transformer model, best detail, costs more. L:
  sharper and steadier, less ghosting, costs the most. M: close to L in looks, closer to K
  in speed. J: like K with a little less ghosting and more flicker. Takes effect at once.
  Only while DLSS draws.
- Command-line switches: `--dlss-model`
- More: docs/dlss.md, docs/design/dlss-upscaling.md
- Registered in `engine/nvforever/src/settings.rs`

### `fsr-library`: FSR library

- On the page: What FSR upscales with: NVForever's built-in FSR 3.1, or a FidelityFX library
  in the game's folder (for tools that replace the upscaler).
- In full: What FSR upscales with: NVForever's built-in FSR 3.1, or a FidelityFX library in
  the game's folder (amd_fidelityfx_loader_vk.dll or amd_fidelityfx_vk.dll; not part of
  NVForever), which tools that replace the upscaler use. The choice names the library found;
  with none there the built-in FSR runs.
- More: docs/design/fsr-upscaling.md
- Registered in `engine/nvforever/src/settings.rs`

### `frame-generation`: Frame generation

- On the page: Frames made between the real ones: smoother motion, input a little later. FSR
  3.1 needs FSR upscaling; DLSS an RTX 40 or newer. Not in the original.
- In full: Frames made between the real ones, so motion looks smoother; what you do reaches
  the screen a little later. FSR 3.1: one frame between each two real frames; needs FSR 3.1
  upscaling (any mode; not with DLSS) and vertical sync or variable refresh, and real frames
  are held to just under half the refresh rate. DLSS: NVIDIA DLSS Frame Generation, one to
  three frames for each real frame (Generated frames, below); needs an upscaler (FSR 3.1 or
  DLSS), Display sync Off, an NVIDIA RTX 40 series or newer graphics card, Windows'
  hardware-accelerated GPU scheduling and NVIDIA's frame generation files in the dlss folder
  beside nvforever.exe; frames are generated only while the game's window is in front.
  Latency reduction turns on with either. The HUD and menus are not generated; menus and
  loading screens show real frames only. Best from 60 real frames a second. Not in the
  original.
- Command-line switches: `--frame-generation`
- More: docs/design/frame-generation.md, docs/design/dlss-frame-generation.md
- Registered in `engine/nvforever/src/settings.rs`

### `frame-generation-frames`: Generated frames

- On the page: How many frames DLSS frame generation makes per real frame: 2x one, 3x two,
  4x three (3x and 4x on RTX 50 series). Not in the original.
- In full: How many frames DLSS frame generation makes for each real frame: 2x one, 3x two,
  4x three. More looks smoother and costs a little more latency and graphics memory; 3x and
  4x only where the graphics card offers them (RTX 50 series). Takes effect at once. Only
  with DLSS frame generation. Not in the original.
- More: docs/design/dlss-frame-generation.md
- Registered in `engine/nvforever/src/settings.rs`

### `anti-aliasing`: Anti-aliasing

- On the page: How edges are smoothed: multisampling 2x, 4x or 8x (dearer at each step) or
  FXAA, SMAA or TAA afterwards. Reads Off with FSR or DLSS; Bilinear keeps FXAA and SMAA.
- In full: How edges are smoothed: multisampling Off, 2x, 4x or 8x (--msaa; the cut-out
  edges of plants too), or the picture smoothed afterwards: FXAA (cheapest, softest), SMAA
  (sharper, a little dearer) or TAA (over the frames; plants too, soft while moving;
  --graphics-only fxaa, smaa or taa). Multisampling is the dearest, more at each step: at
  1920x1080 on an RTX 4070, 2x added about 0.8 ms a frame, 4x about 1.3 ms and 8x about 2.3
  ms (Goodsprings and Novac at noon); FXAA, SMAA and TAA cost less than 4x. Where the
  processor sets the pace (the Strip at night) the choice costs next to nothing. While FSR
  3.1 or DLSS is on this row reads Off (upscaling active): they do the anti-aliasing
  themselves; your choice comes back when Upscaler is Off. With Bilinear, which smooths
  nothing, FXAA and SMAA still apply after it; multisampling and TAA wait until Upscaler is
  Off.
- Command-line switches: `--msaa`, `--graphics-only`
- Registered in `engine/nvforever/src/settings.rs`

### `latency-reduction`: Latency reduction

- On the page: The graphics driver keeps the game from preparing frames ahead (AMD Anti-Lag,
  NVIDIA low latency), meant to show what you do sooner. Not in the original.
- In full: The graphics driver keeps the game from preparing frames ahead of the graphics
  card: AMD Anti-Lag on AMD cards (Windows: Adrenalin 24.9.1 or later; Linux: Mesa 26.3 or
  later), NVIDIA low latency on NVIDIA cards whose Vulkan driver has it. It is meant to make
  what you do reach the screen sooner; that has not been measured yet. The log says which of
  the two this graphics card uses, or why neither. Not in the original.
- Command-line switches: `--latency-reduction`
- Registered in `engine/nvforever/src/settings.rs`

### `shadow-filter`: Shadow filtering

- On the page: How the sun's shadow edges are softened: Fast (harder, cheapest), Smooth (as
  vanilla) or Temporal (grainy unless Anti-aliasing is TAA).
- In full: How the edges of the sun's shadows are softened: Fast (hardware 2x2, harder
  edges, cheapest), Smooth (as vanilla) or Temporal (soft but grainy unless Anti-aliasing is
  TAA).
- Registered in `engine/nvforever/src/settings.rs`

### `ambient-occlusion`: Ambient occlusion

- On the page: On: creases, corners and the ground under things are darkened by how enclosed
  they are (screen-space, from the depth). Not in the original.
- Command-line switches: `--graphics`
- More: docs/design/graphics-enhanced.md
- Registered in `engine/nvforever/src/settings.rs`

### `fine-shadows`: Longer sun shadows

- On the page: On: the sun's shadows reach half as far again as the shadow distance. Costs a
  little frame rate.
- Command-line switches: `--shadow-distance`, `--graphics`
- More: docs/design/graphics-enhanced.md
- Registered in `engine/nvforever/src/settings.rs`

### `bloom`: Bloom

- On the page: On: light brighter than a sunlit white wall (the sun, lamps, windows) glows
  into its surroundings, on top of the game's own image space.
- Command-line switches: `--graphics`
- More: docs/design/graphics-enhanced.md
- Registered in `engine/nvforever/src/settings.rs`

### `glow`: Light glow (HDR)

- On the page: On: neon, lamps, beams and fire glow softly, as the original's HDR glow does,
  as much as the place's image space asks. Bloom adds to it.
- In full: On: neon, lamps, glowing beams and fire glow softly into their surroundings, as
  the original's HDR glow does; how much follows the place's image space (the Strip's neon
  at night most). Bloom, when on, adds to it.
- Command-line switches: `--graphics-only`
- More: docs/design/graphics-enhanced.md
- Registered in `engine/nvforever/src/settings.rs`

### `eye-adaptation`: Eye adaptation

- On the page: On: dark views brighten after a moment, as the eye gets used to the dark, by
  the place's image space. Needs the light glow.
- Command-line switches: `--graphics-only`
- More: docs/design/graphics-enhanced.md
- Registered in `engine/nvforever/src/settings.rs`

### `neon-glow`: Neon glow

- On the page: On: where the image space asks for a strong glow (the Strip at night) neon
  burns with a wide halo, as in the original. Needs the light glow.
- In full: On: where the place's image space asks for a strong glow (the Strip at night)
  neon tubes and signs burn with a wide halo, as in the original. Needs the light glow.
- Command-line switches: `--graphics-only`
- More: docs/design/graphics-enhanced.md
- Registered in `engine/nvforever/src/settings.rs`

### `sharpen`: Sharpening

- On the page: On: the final picture is sharpened where it has little contrast
  (contrast-adaptive sharpening).
- Command-line switches: `--graphics`
- More: docs/design/graphics-enhanced.md
- Registered in `engine/nvforever/src/settings.rs`

### `anisotropic`: Anisotropic filtering 16x

- On the page: On: textures seen at a slant stay sharp further (16x instead of 8x). For
  textures loaded from now on (the cells loaded next).
- Command-line switches: `--graphics`
- More: docs/design/graphics-enhanced.md
- Registered in `engine/nvforever/src/settings.rs`

### `light-shafts`: Sun shafts

- On the page: On: sunlight streams past what stands in front of the sun, stronger the
  hazier the weather's fog. Not in the original.
- Command-line switches: `--graphics`
- More: docs/design/graphics-enhanced.md
- Registered in `engine/nvforever/src/settings.rs`

### `water-reflections`: Screen-space water reflections

- On the page: On: water the mirrored reflection does not reach (another height, interiors)
  reflects what is on the screen instead of only the sky.
- Command-line switches: `--graphics`
- More: docs/design/graphics-enhanced.md
- Registered in `engine/nvforever/src/settings.rs`

### `water-mirror`: Mirrored water reflections

- On the page: On: water mirrors the world above it, as in the original, drawing the scene a
  second time. Off: it reflects the sky.
- In full: On: water mirrors the world above it, as in the original; the scene is drawn a
  second time for it while water is on screen. Off: water reflects the sky, and what is on
  the screen with screen-space reflections.
- Command-line switches: `--graphics-only`
- More: docs/design/graphics-enhanced.md
- Registered in `engine/nvforever/src/settings.rs`

## Graphics › Distance

### `distant-lod`: Distant terrain and objects

- On the page: On: distant land, buildings and trees are drawn beyond the loaded cells
  (outdoors). Off: the world ends at the loaded cells.
- Command-line switches: `--no-lod`
- Registered in `engine/nvforever/src/settings.rs`

### `grass`: Grass

- On the page: On: grass grows on the landscape; the cells around you get theirs over the
  next frames. Off: it is hidden and none grows on new cells.
- Command-line switches: `--no-grass`
- Registered in `engine/nvforever/src/settings.rs`

### `object-distance`: Object distance

- On the page: How far from you objects of the loaded cells (buildings, rocks, furniture,
  clutter) are drawn. Lower draws fewer and runs faster.
- In full: How far from you objects (buildings, rocks, furniture, clutter) of the loaded
  cells are drawn, as a part of the farthest those cells reach. Lower draws fewer and runs
  faster.
- Registered in `engine/nvforever/src/settings.rs`

### `actor-distance`: Actor distance

- On the page: How far from you people and creatures are drawn, as a part of the farthest
  the loaded cells reach.
- Registered in `engine/nvforever/src/settings.rs`

### `item-distance`: Item distance

- On the page: How far from you items lying in the world are drawn, as a part of the
  farthest the loaded cells reach.
- In full: How far from you items lying in the world (weapons, ammo, aid, notes, caps,
  clutter you can take) are drawn, as a part of the farthest the loaded cells reach.
- Registered in `engine/nvforever/src/settings.rs`

### `shadow-distance`: Shadow distance

- On the page: How far from you the sun casts shadows (with Sun shadows on), as a part of
  the farthest shadow distance.
- Registered in `engine/nvforever/src/settings.rs`

### `grass-distance`: Grass distance

- On the page: How far from you grass is drawn, as a part of the farthest grass distance and
  its fade range.
- Registered in `engine/nvforever/src/settings.rs`

## Graphics › World

### `head-tracking`: Head tracking

- On the page: On: actors turn their heads to the player and to others near them. Off: heads
  ease back to their animation.
- Command-line switches: `--no-head-tracking`
- Registered in `engine/nvforever/src/settings.rs`

### `foot-ik`: Feet follow the ground

- On the page: On: on slopes and steps actors bend their knees and set each foot on the
  ground under it. Off: the legs play the animation as it is.
- Command-line switches: `--no-foot-ik`
- Registered in `engine/nvforever/src/settings.rs`

### `sky-clock`: Sky follows the game clock

- On the page: On: the sky shows the game's time of day as it passes. Off: the sky keeps its
  hour while the game's clock runs on ([ and ] still set it).
- Command-line switches: `--still-sky`
- Registered in `engine/nvforever/src/settings.rs`

## Debug

### `overlay`: Text overlay

- On the page: On: the position, loading counts and key help are written over the picture.
  Off by default in the game.
- Command-line switches: `--overlay`, `--no-overlay`
- Registered in `engine/nvforever/src/settings.rs`

### `hit-markers`: Shot markers

- On the page: On: a cross is drawn at each impact and a line from the muzzle along each
  hitscan shot.
- Command-line switches: `--hit-markers`
- Registered in `engine/nvforever/src/settings.rs`

### `god-mode`: God mode

- On the page: On: the player takes no damage from hits and cannot be killed by them.
- Command-line switches: `--god`
- Registered in `engine/nvforever/src/settings.rs`

### `navmesh`: Draw the navmesh

- On the page: On: the navmesh near the camera and the walking NPCs' paths are drawn.
- Command-line switches: `--show-navmesh`
- Registered in `engine/nvforever/src/settings.rs`

### `scope-3d`: 3D scopes (test)

- On the page: On: a scoped gun stays on screen and shows the zoomed view in its lens. Off:
  the full-screen scope picture, as in the original.
- In full: On: aiming a scoped gun keeps the gun on screen and shows the zoomed view in its
  scope's lens, with the reticle; the view around it zooms only a little. Off: the
  full-screen scope picture with its black border, as in the original. Guns whose scope lens
  is not known yet keep the picture.
- Command-line switches: `--scope-3d`
- Registered in `engine/nvforever/src/settings.rs`

## Controls › the tab's own rows

`engine/nvf-play/src/pause.rs` (`row_note`, `AUTOMATIC_INPUT_NOTE`) and
`engine/nvf-play/src/bindings.rs`; docs/design/controller.md, docs/design/settings-page.md §4.
None of these rows has a command-line switch.

### Input Device

- On the page: Enter or a click: work the controls with the keyboard and mouse or with a
  controller. Kept as "Input Device" in [Controls] of NVForever.ini.
- In full: with a controller the left stick moves, the right stick looks, Start opens the
  pause menu, and in menus the D-pad, A and B are the arrow keys, Enter and Esc.

### Automatic Input Switching

- On the page: On: a controller button, stick or trigger selects Controller; a key or mouse
  click selects Keyboard and Mouse (mouse motion does not). Off: choose Input Device yourself.
- In full: a controller button, or a stick or trigger past its dead zone, selects Controller;
  a key or mouse click selects Keyboard and Mouse. First release the current device's buttons
  and centre its sticks and triggers, then press again to switch on a later frame. Only the
  switching press is ignored; later holds and repeats and all releases work. Mouse motion does
  not switch. Off: choose Input Device by hand. Kept in [Controls] of NVForever.ini.

### A control (keyboard and mouse)

- On the page: Enter or a click: bind a new key or mouse button to the control (a control
  that had it gets the old one). Delete clears its key.
- In full: a key replaces the control's key, a mouse button its mouse button. Kept under the
  control's name in [Controls] of NVForever.ini.

### A control (controller)

- On the page: A or a click: bind a new controller button to the control (a control that had
  it gets the old one). X or Delete clears it.
- In full: kept under the control's name in [Controller] of NVForever.ini.

### Invert Y

- On the page: Enter or a click: look up and down the other way round (the mouse and the
  right stick). Kept as "Invert Y" in [Controls].
- In full: kept in [Controls] of NVForever.ini.

### Mouse Sensitivity / Stick Sensitivity

- On the page: Left/Right or a click on the meter: how far the view turns as the mouse moves
  (how fast the right stick turns it), between the least and the most, as a part of the
  game's. Kept as "Mouse Sensitivity" in [Controls] ("Stick Sensitivity" in [Controller]).
- In full: a step is about a tenth. The mouse's value is kept as radians a mouse count.

### Vertical Sensitivity

- On the page: Left/Right or a click on the meter: how far (how fast, with the right stick)
  the view turns up and down, as a part of how it turns left and right.
- In full: 100% is the same as left and right, as the game has it. Kept as "Mouse Vertical
  Sensitivity" in [Controls] and "Stick Vertical Sensitivity" in [Controller] of
  NVForever.ini.

### Rumble

- On the page: A or a click: the controller shakes when you fire, take damage, land a hard
  fall or an explosion goes off near you. Kept as "Rumble" in [Controller].
- In full: harder for a heavier weapon. Kept in [Controller] of NVForever.ini.
